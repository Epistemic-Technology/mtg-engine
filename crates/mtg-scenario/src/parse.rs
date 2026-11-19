//! Line-oriented parser for scenario files.

use mtg_core::ids::PlayerId;
use mtg_core::turn::Step;
use thiserror::Error;

use crate::ast::{
    Action, Assertion, CardSpec, ObjRef, Pick, Position, RunTarget, Scenario, ScriptedAction,
    SetupSpec, Statement, TargetRef, ZoneSpec, step_by_name, zone_by_name,
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("line {line}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

fn err<T>(line: usize, message: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError {
        line,
        message: message.into(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Str(String),
    Num(i64),
    Ident(String),
    Colon,
    Comma,
    LBrace,
    RBrace,
}

impl std::fmt::Display for Tok {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tok::Str(s) => write!(f, "\"{s}\""),
            Tok::Num(n) => write!(f, "{n}"),
            Tok::Ident(s) => write!(f, "{s}"),
            Tok::Colon => write!(f, ":"),
            Tok::Comma => write!(f, ","),
            Tok::LBrace => write!(f, "{{"),
            Tok::RBrace => write!(f, "}}"),
        }
    }
}

fn lex_line(line_no: usize, text: &str) -> Result<Vec<Tok>, ParseError> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            ' ' | '\t' | '\r' => {}
            '#' => break,
            '/' if text[i..].starts_with("//") => break,
            ':' => out.push(Tok::Colon),
            ',' => out.push(Tok::Comma),
            '{' => out.push(Tok::LBrace),
            '}' => out.push(Tok::RBrace),
            '"' => {
                let mut s = String::new();
                let mut closed = false;
                for (_, c) in chars.by_ref() {
                    if c == '"' {
                        closed = true;
                        break;
                    }
                    s.push(c);
                }
                if !closed {
                    return err(line_no, "unterminated string");
                }
                out.push(Tok::Str(s));
            }
            // A power/toughness counter kind: "+1/+1", "-0/-1", "+1/+2".
            '+' | '-'
                if mtg_core::card::CounterKind::PT
                    .iter()
                    .any(|(n, ..)| text[i..].starts_with(n)) =>
            {
                let len = mtg_core::card::CounterKind::PT
                    .iter()
                    .find(|(n, ..)| text[i..].starts_with(n))
                    .map_or(0, |(n, ..)| n.len());
                out.push(Tok::Ident(text[i..i + len].to_string()));
                for _ in 1..len {
                    chars.next();
                }
            }
            c if c.is_ascii_digit() || c == '-' => {
                let mut s = String::from(c);
                while let Some((_, d)) = chars.peek() {
                    if d.is_ascii_digit() {
                        s.push(*d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let n: i64 = s.parse().map_err(|_| ParseError {
                    line: line_no,
                    message: format!("bad number {s}"),
                })?;
                out.push(Tok::Num(n));
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut s = String::from(c);
                while let Some((_, d)) = chars.peek() {
                    if d.is_alphanumeric() || *d == '_' || *d == '\'' {
                        s.push(*d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(Tok::Ident(s));
            }
            other => return err(line_no, format!("unexpected character {other:?}")),
        }
    }
    Ok(out)
}

/// A cursor over one line's tokens.
struct Line {
    no: usize,
    toks: Vec<Tok>,
    pos: usize,
}

impl Line {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn fail<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        err(self.no, message)
    }

    fn expect_end(&self) -> Result<(), ParseError> {
        match self.peek() {
            None => Ok(()),
            Some(t) => self.fail(format!("unexpected {t} at end of statement")),
        }
    }

    fn ident(&mut self) -> Result<String, ParseError> {
        match self.next() {
            Some(Tok::Ident(s)) => Ok(s),
            Some(t) => self.fail(format!("expected a word, found {t}")),
            None => self.fail("expected a word, found end of line"),
        }
    }

    fn keyword(&mut self, kw: &str) -> Result<(), ParseError> {
        match self.next() {
            Some(Tok::Ident(s)) if s == kw => Ok(()),
            Some(t) => self.fail(format!("expected `{kw}`, found {t}")),
            None => self.fail(format!("expected `{kw}`, found end of line")),
        }
    }

    fn peek_ident(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s == kw)
    }

    fn string(&mut self) -> Result<String, ParseError> {
        match self.next() {
            Some(Tok::Str(s)) => Ok(s),
            Some(t) => self.fail(format!("expected a quoted card name, found {t}")),
            None => self.fail("expected a quoted card name, found end of line"),
        }
    }

    fn number(&mut self) -> Result<i64, ParseError> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(n),
            Some(t) => self.fail(format!("expected a number, found {t}")),
            None => self.fail("expected a number, found end of line"),
        }
    }

    fn unsigned(&mut self) -> Result<u32, ParseError> {
        let n = self.number()?;
        u32::try_from(n)
            .or_else(|_| self.fail(format!("expected a non-negative number, found {n}")))
    }

    fn count(&mut self) -> Result<usize, ParseError> {
        let n = self.number()?;
        usize::try_from(n).or_else(|_| self.fail(format!("expected a count, found {n}")))
    }

    fn colon(&mut self) -> Result<(), ParseError> {
        match self.next() {
            Some(Tok::Colon) => Ok(()),
            Some(t) => self.fail(format!("expected `:`, found {t}")),
            None => self.fail("expected `:`, found end of line"),
        }
    }

    fn comma_or_end(&mut self) -> Result<bool, ParseError> {
        match self.peek() {
            Some(Tok::Comma) => {
                self.pos += 1;
                Ok(true)
            }
            None => Ok(false),
            Some(t) => self.fail(format!("expected `,` or end of line, found {t}")),
        }
    }

    fn player(&mut self) -> Result<PlayerId, ParseError> {
        match self.next() {
            Some(Tok::Ident(s)) if s == "A" => Ok(PlayerId(0)),
            Some(Tok::Ident(s)) if s == "B" => Ok(PlayerId(1)),
            Some(t) => self.fail(format!("expected player A or B, found {t}")),
            None => self.fail("expected player A or B, found end of line"),
        }
    }

    fn peek_player(&self) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s == "A" || s == "B")
    }

    fn step(&mut self) -> Result<Step, ParseError> {
        let name = self.ident()?;
        step_by_name(&name).ok_or_else(|| ParseError {
            line: self.no,
            message: format!("unknown step `{name}`"),
        })
    }

    fn peek_step(&self) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if step_by_name(s).is_some())
    }

    /// `"name"` or `A "name"`.
    /// `[oldest|newest] [A|B] "name"`.
    fn obj_ref(&mut self) -> Result<ObjRef, ParseError> {
        let pick = if self.peek_ident("oldest") {
            self.pos += 1;
            Some(Pick::Oldest)
        } else if self.peek_ident("newest") {
            self.pos += 1;
            Some(Pick::Newest)
        } else {
            None
        };
        let player = if self.peek_player() {
            Some(self.player()?)
        } else {
            None
        };
        let name = self.string()?;
        Ok(ObjRef { player, name, pick })
    }

    /// `player A` | `"name"` | `A "name"`.
    fn target_ref(&mut self) -> Result<TargetRef, ParseError> {
        if self.peek_ident("auto") {
            self.pos += 1;
            return Ok(TargetRef::Auto);
        }
        if self.peek_ident("player") {
            self.pos += 1;
            return Ok(TargetRef::Player(self.player()?));
        }
        Ok(TargetRef::Object(self.obj_ref()?))
    }

    /// `[N] "name" [tapped] [on "name"] [with N +1/+1]`.
    fn card_spec(&mut self) -> Result<CardSpec, ParseError> {
        let count = if matches!(self.peek(), Some(Tok::Num(_))) {
            self.count()?
        } else {
            1
        };
        let name = self.string()?;
        let tapped = if self.peek_ident("tapped") {
            self.pos += 1;
            true
        } else {
            false
        };
        let attached_to = if self.peek_ident("on") {
            self.pos += 1;
            Some(self.string()?)
        } else {
            None
        };
        let mut counters = Vec::new();
        if self.peek_ident("with") {
            self.pos += 1;
            let n = self.unsigned()?;
            let kind = self.ident()?;
            counters.push((kind, n));
        }
        Ok(CardSpec {
            name,
            count,
            tapped,
            attached_to,
            counters,
        })
    }
}

/// Parse a scenario file into its scenarios.
pub fn parse(text: &str) -> Result<Vec<Scenario>, ParseError> {
    let mut scenarios = Vec::new();
    let mut current: Option<Scenario> = None;
    for (idx, raw) in text.lines().enumerate() {
        let no = idx + 1;
        let toks = lex_line(no, raw)?;
        if toks.is_empty() {
            continue;
        }
        let mut line = Line { no, toks, pos: 0 };
        match current.as_mut() {
            None => {
                line.keyword("scenario")?;
                let name = line.string()?;
                match line.next() {
                    Some(Tok::LBrace) => {}
                    _ => return err(no, "expected `{` after the scenario name"),
                }
                line.expect_end()?;
                current = Some(Scenario {
                    name,
                    line: no,
                    skip: None,
                    setup: SetupSpec::default(),
                    statements: Vec::new(),
                });
            }
            Some(sc) => {
                if line.peek() == Some(&Tok::RBrace) {
                    line.pos += 1;
                    line.expect_end()?;
                    scenarios.push(current.take().expect("open scenario"));
                    continue;
                }
                parse_statement(&mut line, sc)?;
                line.expect_end()?;
            }
        }
    }
    if let Some(sc) = current {
        return err(
            sc.line,
            format!("scenario \"{}\" is missing its closing `}}`", sc.name),
        );
    }
    Ok(scenarios)
}

#[allow(clippy::too_many_lines)] // one arm per statement
fn parse_statement(line: &mut Line, sc: &mut Scenario) -> Result<(), ParseError> {
    let head = line.ident()?;
    match head.as_str() {
        "skip" => {
            sc.skip = Some(line.string()?);
        }
        "seed" => {
            sc.setup.seed = Some(u64::from(line.unsigned()?));
        }
        "life" => {
            let p = line.player()?;
            let n = line.number()?;
            let total = i32::try_from(n).or_else(|_| line.fail("life out of range"))?;
            sc.setup.life[p.index()] = Some(total);
        }
        "poison" => {
            let p = line.player()?;
            line.colon()?;
            sc.setup.poison[p.index()] =
                u32::try_from(line.number()?).or_else(|_| line.fail("poison out of range"))?;
        }
        "speed" => {
            let p = line.player()?;
            line.colon()?;
            sc.setup.speed[p.index()] =
                u8::try_from(line.number()?).or_else(|_| line.fail("speed out of range"))?;
        }
        "energy" => {
            let p = line.player()?;
            line.colon()?;
            sc.setup.energy[p.index()] =
                u32::try_from(line.number()?).or_else(|_| line.fail("energy out of range"))?;
        }
        "battlefield" | "hand" | "graveyard" | "library" | "exile" => {
            let zone = zone_by_name(&head).expect("matched zone name");
            let player = line.player()?;
            line.colon()?;
            let mut cards = Vec::new();
            if !line.at_end() {
                loop {
                    cards.push(line.card_spec()?);
                    if !line.comma_or_end()? {
                        break;
                    }
                }
            }
            sc.setup.zones.push(ZoneSpec {
                line: line.no,
                zone,
                player,
                cards,
            });
        }
        "start" => {
            line.keyword("turn")?;
            let turn = line.unsigned()?;
            let player = line.player()?;
            let step = if line.at_end() {
                Step::Untap
            } else {
                line.step()?
            };
            sc.setup.start = Some((turn, player, step));
        }
        "illegal" => {
            line.keyword("turn")?;
            sc.statements
                .push(Statement::Action(parse_action(line, true)?));
        }
        "turn" => {
            sc.statements
                .push(Statement::Action(parse_action(line, false)?));
        }
        "run_to" => {
            let target = if line.peek_ident("end") {
                line.pos += 1;
                RunTarget::End
            } else {
                line.keyword("turn")?;
                let turn = line.unsigned()?;
                let step = if line.at_end() {
                    Step::Untap
                } else {
                    line.step()?
                };
                RunTarget::Position(Position { turn, step })
            };
            sc.statements.push(Statement::RunTo {
                line: line.no,
                target,
            });
        }
        "assert" => {
            let assertion = parse_assertion(line)?;
            sc.statements.push(Statement::Assert {
                line: line.no,
                assertion,
            });
        }
        other => return line.fail(format!("unknown statement `{other}`")),
    }
    Ok(())
}

/// After `turn`: `N [step] P: action`.
#[allow(clippy::too_many_lines)] // one arm per action kind
fn parse_action(line: &mut Line, illegal: bool) -> Result<ScriptedAction, ParseError> {
    let turn = line.unsigned()?;
    let explicit_step = if line.peek_step() {
        Some(line.step()?)
    } else {
        None
    };
    let player = line.player()?;
    line.colon()?;
    let verb = line.ident()?;
    let (action, default_step) = match verb.as_str() {
        "play" => (Action::Play(line.string()?), Some(Step::PrecombatMain)),
        "cast" => {
            let name = line.string()?;
            let from_graveyard = if line.peek_ident("from") {
                line.pos += 1;
                line.keyword("graveyard")?;
                true
            } else {
                false
            };
            let evoked = if line.peek_ident("evoked") {
                line.pos += 1;
                true
            } else {
                false
            };
            let dashed = if line.peek_ident("dashed") {
                line.pos += 1;
                true
            } else {
                false
            };
            let alternative = if line.peek_ident("alternative") {
                line.pos += 1;
                true
            } else {
                false
            };
            let bestowed = if line.peek_ident("bestowed") {
                line.pos += 1;
                true
            } else {
                false
            };
            let other_cost = if line.peek_ident("other_cost") {
                line.pos += 1;
                true
            } else {
                false
            };
            let face_down = if line.peek_ident("face_down") {
                line.pos += 1;
                true
            } else {
                false
            };
            let kicked = if line.peek_ident("kicked") {
                line.pos += 1;
                true
            } else {
                false
            };
            let mode = if line.peek_ident("mode") {
                line.pos += 1;
                let n = line.count()?;
                if n == 0 {
                    return line.fail("modes are numbered from 1");
                }
                Some(n)
            } else {
                None
            };
            let x = if line.peek_ident("x") {
                line.pos += 1;
                Some(u32::try_from(line.count()?).unwrap_or(u32::MAX))
            } else {
                None
            };
            let sacrifice = if line.peek_ident("sacrificing") {
                line.pos += 1;
                Some(line.string()?)
            } else {
                None
            };
            let discard = if line.peek_ident("discarding") {
                line.pos += 1;
                Some(line.string()?)
            } else {
                None
            };
            let mut splice = Vec::new();
            if line.peek_ident("splicing") {
                line.pos += 1;
                loop {
                    splice.push(line.string()?);
                    if line.peek() == Some(&Tok::Comma) {
                        line.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            let mut targets = Vec::new();
            if line.peek_ident("targeting") {
                line.pos += 1;
                loop {
                    targets.push(line.target_ref()?);
                    if line.peek() == Some(&Tok::Comma) {
                        line.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            let mut divide = Vec::new();
            if line.peek_ident("dividing") {
                line.pos += 1;
                loop {
                    divide.push(line.unsigned()?);
                    if line.peek() == Some(&Tok::Comma) {
                        line.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            let respond_to = if line.peek_ident("in") {
                line.pos += 1;
                line.keyword("response")?;
                line.keyword("to")?;
                Some(line.string()?)
            } else {
                None
            };
            (
                Action::Cast {
                    name,
                    from_graveyard,
                    evoked,
                    dashed,
                    alternative,
                    bestowed,
                    splice,
                    other_cost,
                    face_down,
                    kicked,
                    mode,
                    x,
                    sacrifice,
                    discard,
                    targets,
                    divide,
                    respond_to,
                },
                Some(Step::PrecombatMain),
            )
        }
        "attack" => {
            let mut names = Vec::new();
            loop {
                let name = line.string()?;
                let at = if line.peek_ident("at") {
                    line.pos += 1;
                    Some(line.target_ref()?)
                } else {
                    None
                };
                names.push((name, at));
                if !line.comma_or_end()? {
                    break;
                }
            }
            (Action::Attack(names), Some(Step::DeclareAttackers))
        }
        "block" => {
            let attacker = line.string()?;
            line.keyword("with")?;
            let blocker = line.string()?;
            (
                Action::Block { attacker, blocker },
                Some(Step::DeclareBlockers),
            )
        }
        "assign" => {
            let attacker = line.string()?;
            line.keyword("to")?;
            let mut parts = Vec::new();
            loop {
                let target = line.target_ref()?;
                let amount = line.unsigned()?;
                parts.push((target, amount));
                if !line.comma_or_end()? {
                    break;
                }
            }
            (Action::Assign { attacker, parts }, None)
        }
        "discard" => (Action::Discard(line.string()?), Some(Step::Cleanup)),
        "trigger" => {
            let source = line.string()?;
            let mut targets = Vec::new();
            if line.peek_ident("targeting") {
                line.pos += 1;
                loop {
                    targets.push(line.target_ref()?);
                    if line.peek() == Some(&Tok::Comma) {
                        line.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            (Action::Trigger { source, targets }, None)
        }
        "apply" => (Action::Apply(line.string()?), None),
        "unmorph" => (Action::TurnFaceUp(line.string()?), None),
        "suspend" => (Action::Suspend(line.string()?), None),
        "foretell" => (Action::Foretell(line.string()?), None),
        "wait" => (Action::Wait, Some(Step::PrecombatMain)),
        "activate" => {
            let source = line.string()?;
            let ability = if line.peek_ident("ability") {
                line.pos += 1;
                let n = line.count()?;
                if n == 0 {
                    return line.fail("abilities are numbered from 1");
                }
                Some(n)
            } else {
                None
            };
            let sacrifice = if line.peek_ident("sacrificing") {
                line.pos += 1;
                Some(line.string()?)
            } else {
                None
            };
            let discard = if line.peek_ident("discarding") {
                line.pos += 1;
                Some(line.string()?)
            } else {
                None
            };
            let mut targets = Vec::new();
            if line.peek_ident("targeting") {
                line.pos += 1;
                loop {
                    targets.push(line.target_ref()?);
                    if line.peek() == Some(&Tok::Comma) {
                        line.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            (
                Action::Activate {
                    source,
                    ability,
                    sacrifice,
                    discard,
                    targets,
                },
                Some(Step::PrecombatMain),
            )
        }
        "accept" | "decline" => (
            Action::YouMay {
                name: line.string()?,
                accept: verb == "accept",
            },
            None,
        ),
        "choose" => {
            let what = if matches!(line.peek(), Some(Tok::Str(_))) {
                line.string()?
            } else {
                line.ident()?
            };
            (Action::Choose(what), None)
        }
        "copy" => {
            if line.peek_ident("nothing") {
                line.pos += 1;
                (Action::Copy(None), None)
            } else {
                (Action::Copy(Some(line.obj_ref()?)), None)
            }
        }
        "keep" => {
            let which = line.ident()?;
            let newest = match which.as_str() {
                "newest" => true,
                "oldest" => false,
                other => return line.fail(format!("expected `oldest` or `newest`, got `{other}`")),
            };
            (
                Action::Keep {
                    name: line.string()?,
                    newest,
                },
                None,
            )
        }
        other => return line.fail(format!("unknown action `{other}`")),
    };
    Ok(ScriptedAction {
        line: line.no,
        turn,
        step: explicit_step.or(default_step),
        player,
        action,
        illegal,
    })
}

#[allow(clippy::too_many_lines)] // one arm per assertion kind
fn parse_assertion(line: &mut Line) -> Result<Assertion, ParseError> {
    let what = line.ident()?;
    Ok(match what.as_str() {
        "monarch" | "initiative" | "daynight" => {
            let Some(Tok::Ident(want) | Tok::Str(want)) = line.next() else {
                return line.fail("expected A, B, none, day, night, or neither");
            };
            Assertion::Designation {
                key: what,
                player: None,
                want,
            }
        }
        "speed" | "tempted" | "ring_bearer" | "blessing" | "dungeon" | "completed" => {
            let p = line.player()?;
            let mut parts = Vec::new();
            while let Some(t) = line.next() {
                parts.push(match t {
                    Tok::Ident(s) | Tok::Str(s) => s,
                    Tok::Num(n) => n.to_string(),
                    _ => return line.fail("expected a value"),
                });
            }
            Assertion::Designation {
                key: what,
                player: Some(p),
                want: parts.join(" / "),
            }
        }
        "energy" => {
            let p = line.player()?;
            Assertion::Energy(
                p,
                u32::try_from(line.number()?).or_else(|_| line.fail("energy out of range"))?,
            )
        }
        "poison" => {
            let p = line.player()?;
            Assertion::Poison(
                p,
                u32::try_from(line.number()?).or_else(|_| line.fail("poison out of range"))?,
            )
        }
        "life" => {
            let p = line.player()?;
            let n = line.number()?;
            Assertion::Life(
                p,
                i32::try_from(n).or_else(|_| line.fail("life out of range"))?,
            )
        }
        "battlefield" | "hand" | "graveyard" | "library" | "exile" => {
            let zone = zone_by_name(&what).expect("matched zone name");
            let player = line.player()?;
            let rel = line.ident()?;
            match rel.as_str() {
                "contains" => {
                    let count = if matches!(line.peek(), Some(Tok::Num(_))) {
                        Some(line.count()?)
                    } else {
                        None
                    };
                    let name = line.string()?;
                    Assertion::ZoneContains {
                        zone,
                        player,
                        name,
                        count,
                    }
                }
                "lacks" => Assertion::ZoneLacks {
                    zone,
                    player,
                    name: line.string()?,
                },
                "count" => Assertion::ZoneCount {
                    zone,
                    player,
                    count: line.count()?,
                },
                other => {
                    return line.fail(format!("expected contains/lacks/count, found `{other}`"));
                }
            }
        }
        "power" => {
            let r = line.obj_ref()?;
            let n = line.number()?;
            Assertion::Power(
                r,
                i32::try_from(n).or_else(|_| line.fail("power out of range"))?,
            )
        }
        "toughness" => {
            let r = line.obj_ref()?;
            let n = line.number()?;
            Assertion::Toughness(
                r,
                i32::try_from(n).or_else(|_| line.fail("toughness out of range"))?,
            )
        }
        "damage" => {
            let r = line.obj_ref()?;
            Assertion::Damage(r, line.unsigned()?)
        }
        "tapped" => Assertion::Tapped(line.obj_ref()?, true),
        "untapped" => Assertion::Tapped(line.obj_ref()?, false),
        "winner" => Assertion::Winner(line.player()?),
        "draw" => Assertion::Draw,
        "game_over" => Assertion::GameOver(true),
        "not" => {
            line.keyword("game_over")?;
            Assertion::GameOver(false)
        }
        "turn" => Assertion::Turn(line.unsigned()?),
        "step" => Assertion::Step(line.step()?),
        "stack" => {
            if line.peek_ident("top") {
                line.pos += 1;
                Assertion::StackTop(line.string()?)
            } else {
                line.keyword("count")?;
                Assertion::StackCount(line.count()?)
            }
        }
        "has" => {
            let negated = line.peek_ident("not");
            if negated {
                line.pos += 1;
            }
            let r = line.obj_ref()?;
            let kw = line.ident()?;
            Assertion::HasKeyword(r, kw, !negated)
        }
        "is" => {
            let negated = line.peek_ident("not");
            if negated {
                line.pos += 1;
            }
            let r = line.obj_ref()?;
            // A quoted word for subtypes with punctuation ("Assembly-Worker").
            let what = if matches!(line.peek(), Some(Tok::Str(_))) {
                line.string()?
            } else {
                line.ident()?
            };
            Assertion::Is(r, what, !negated)
        }
        "counters" => {
            let r = line.obj_ref()?;
            let kind = line.ident()?;
            Assertion::Counters(r, kind, line.unsigned()?)
        }
        "attached" => {
            let aura = line.obj_ref()?;
            line.keyword("to")?;
            Assertion::Attached(aura, line.obj_ref()?)
        }
        "no_abilities" => Assertion::NoAbilities(line.obj_ref()?),
        other => return line.fail(format!("unknown assertion `{other}`")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_design_example() {
        let text = r#"
# comment
scenario "deathtouch kills regardless of toughness" {
  battlefield A: "Serra Angel"
  battlefield B: "Typhoid Rats" tapped, 2 "Swamp"
  start turn 2 B
  turn 2 B: attack "Typhoid Rats"
  turn 2 A: block "Typhoid Rats" with "Serra Angel"
  run_to turn 2 postcombat_main
  assert life A 20
  assert graveyard A contains "Serra Angel"
  assert graveyard B contains "Typhoid Rats"
  assert battlefield B count 2
}
"#;
        let s = parse(text).unwrap();
        assert_eq!(s.len(), 1);
        let sc = &s[0];
        assert_eq!(sc.setup.zones.len(), 2);
        assert_eq!(sc.setup.zones[1].cards.len(), 2);
        assert!(sc.setup.zones[1].cards[0].tapped);
        assert_eq!(sc.setup.zones[1].cards[1].count, 2);
        assert_eq!(sc.setup.start, Some((2, PlayerId(1), Step::Untap)));
        assert_eq!(sc.statements.len(), 7);
        match &sc.statements[0] {
            Statement::Action(a) => {
                assert_eq!(a.step, Some(Step::DeclareAttackers));
                assert_eq!(a.player, PlayerId(1));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn reports_errors_with_line_numbers() {
        let e = parse("scenario \"x\" {\n  assert lifee A 3\n}\n").unwrap_err();
        assert_eq!(e.line, 2);
        let e = parse("scenario \"x\" {\n  turn 1 A: cast \"Shock\" targeting\n").unwrap_err();
        assert_eq!(e.line, 2);
        let e = parse("scenario \"x\" {\n").unwrap_err();
        assert_eq!(e.line, 1);
    }

    #[test]
    fn parses_targets_and_assignments() {
        let text = r#"scenario "t" {
  turn 1 A: cast "Lightning Bolt" targeting player B
  turn 1 main2 A: cast "Shock" targeting B "Grizzly Bears", "Hill Giant"
  turn 3 B: assign "Colossal Dreadmaw" to "Grizzly Bears" 2, player A 4
  illegal turn 2 A: block "Boggart Brute" with "Grizzly Bears"
  run_to end
}"#;
        let s = parse(text).unwrap();
        let st = &s[0].statements;
        match &st[1] {
            Statement::Action(a) => {
                assert_eq!(a.step, Some(Step::PostcombatMain));
                match &a.action {
                    Action::Cast { targets, .. } => assert_eq!(targets.len(), 2),
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
        match &st[2] {
            Statement::Action(a) => assert_eq!(a.step, None),
            other => panic!("{other:?}"),
        }
        match &st[3] {
            Statement::Action(a) => assert!(a.illegal),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            st[4],
            Statement::RunTo {
                target: RunTarget::End,
                ..
            }
        ));
    }
}
