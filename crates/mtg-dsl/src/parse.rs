//! Parser for card files: `card "Name" { ... }` blocks of one statement per
//! line. See `SYNTAX.md` in this crate.

use mtg_core::card::{
    CardType, Condition, CounterKind, Keyword, Num, PtValue, Relation, Replacement as Outcome,
    RuleKind, Supertype, TargetSpec, Who,
};
use mtg_core::mana::{Color, Mana, ManaCost};
use mtg_core::state::Zone;
use mtg_core::turn::Step;
use thiserror::Error;

use crate::ast::{
    Ability, Affects, Card, CdaCount, Cost, CostItem, Effect, Except, Filter, Kind, Mod, Mode,
    Recipient, Replacement, Replaces, SacCost, Sentence, Target, Trigger, TriggerObj,
};
use crate::lex::{Spanned, Tok, lex};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("line {line}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

type R<T> = Result<T, ParseError>;

pub fn parse(text: &str) -> R<Vec<Card>> {
    let toks = lex(text)?;
    let mut p = P {
        toks,
        pos: 0,
        targets: Vec::new(),
        in_delayed: false,
        subject: None,
    };
    let mut cards = Vec::new();
    loop {
        p.skip_newlines();
        if p.at_end() {
            break;
        }
        cards.push(p.card()?);
    }
    Ok(cards)
}

/// Keywords by their text, longest first.
const KEYWORDS: &[(&[&str], Keyword)] = &[
    (&["double", "strike"], Keyword::DoubleStrike),
    (&["first", "strike"], Keyword::FirstStrike),
    (&["deathtouch"], Keyword::Deathtouch),
    (&["defender"], Keyword::Defender),
    (&["flash"], Keyword::Flash),
    (&["flying"], Keyword::Flying),
    (&["haste"], Keyword::Haste),
    (&["hexproof"], Keyword::Hexproof),
    (&["indestructible"], Keyword::Indestructible),
    (&["lifelink"], Keyword::Lifelink),
    (&["menace"], Keyword::Menace),
    (&["prowess"], Keyword::Prowess),
    (&["reach"], Keyword::Reach),
    (&["trample"], Keyword::Trample),
    (&["vigilance"], Keyword::Vigilance),
];

/// Target specifications by their text, longest first. `,` stands for a
/// comma token.
pub(crate) const TARGETS: &[(&[&str], TargetSpec)] = &[
    (
        &["nonlegendary", "creature", "you", "control"],
        TargetSpec::NonlegendaryCreatureYouControl,
    ),
    (
        &["creature", "card", "from", "your", "graveyard"],
        TargetSpec::CreatureCardInYourGraveyard,
    ),
    (
        &["creature", "card", "in", "your", "graveyard"],
        TargetSpec::CreatureCardInYourGraveyard,
    ),
    (
        &["artifact", ",", "creature", ",", "or", "land"],
        TargetSpec::ArtifactCreatureOrLand,
    ),
    (
        &["creature", "you", "control"],
        TargetSpec::CreatureYouControl,
    ),
    (
        &["player", "or", "planeswalker"],
        TargetSpec::PlayerOrPlaneswalker,
    ),
    (
        &["creature", "or", "planeswalker"],
        TargetSpec::CreatureOrPlaneswalker,
    ),
    (
        &["artifact", "or", "enchantment"],
        TargetSpec::ArtifactOrEnchantment,
    ),
    (&["tapped", "creature"], TargetSpec::TappedCreature),
    (&["other", "creature"], TargetSpec::OtherCreature),
    (&["another", "creature"], TargetSpec::OtherCreature),
    (&["other", "permanent"], TargetSpec::OtherPermanent),
    (&["another", "permanent"], TargetSpec::OtherPermanent),
    (&["any", "target"], TargetSpec::Any),
    (&["any"], TargetSpec::Any),
    (&["creature"], TargetSpec::Creature),
    (&["land"], TargetSpec::Land),
    (&["player"], TargetSpec::Player),
    (&["artifact"], TargetSpec::Artifact),
    (&["enchantment"], TargetSpec::Enchantment),
    (&["permanent"], TargetSpec::Permanent),
];

const RECIPIENTS: &[(&[&str], Recipient)] = &[
    (
        &["each", "creature", "you", "control"],
        Recipient::EachCreatureYouControl,
    ),
    (
        &["each", "permanent", "you", "control"],
        Recipient::EachPermanentYouControl,
    ),
    (
        &["creatures", "you", "control"],
        Recipient::EachCreatureYouControl,
    ),
    (
        &["permanents", "you", "control"],
        Recipient::EachPermanentYouControl,
    ),
    (&["each", "opponent"], Recipient::EachOpponent),
    (&["each", "player"], Recipient::EachPlayer),
    (&["each", "creature"], Recipient::EachCreature),
    (&["all", "creatures"], Recipient::EachCreature),
    (&["those", "creatures"], Recipient::EachCreatureYouControl),
    (&["those", "permanents"], Recipient::EachPermanentYouControl),
    (&["that", "creature"], Recipient::TriggeringObject),
    (&["that", "permanent"], Recipient::TriggeringObject),
    (&["that", "object"], Recipient::TriggeringObject),
    (&["that", "player"], Recipient::TriggeringPlayer),
    (&["defending", "player"], Recipient::DefendingPlayer),
    (&["that", "token"], Recipient::CreatedObject),
    (&["you"], Recipient::You),
    (&["this"], Recipient::Source),
];

const NUMBER_WORDS: [&str; 20] = [
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
];

fn card_type(w: &str) -> Option<CardType> {
    Some(match w.to_ascii_lowercase().as_str() {
        "artifact" | "artifacts" => CardType::Artifact,
        "battle" | "battles" => CardType::Battle,
        "creature" | "creatures" => CardType::Creature,
        "enchantment" | "enchantments" => CardType::Enchantment,
        "instant" | "instants" => CardType::Instant,
        "kindred" => CardType::Kindred,
        "land" | "lands" => CardType::Land,
        "planeswalker" | "planeswalkers" => CardType::Planeswalker,
        "sorcery" | "sorceries" => CardType::Sorcery,
        _ => return None,
    })
}

fn supertype(w: &str) -> Option<Supertype> {
    Some(match w.to_ascii_lowercase().as_str() {
        "basic" => Supertype::Basic,
        "legendary" => Supertype::Legendary,
        "snow" => Supertype::Snow,
        "world" => Supertype::World,
        _ => return None,
    })
}

fn color(w: &str) -> Option<Color> {
    Some(match w {
        "white" => Color::White,
        "blue" => Color::Blue,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        _ => return None,
    })
}

fn basic_land_type(w: &str) -> Option<Color> {
    Some(match w {
        "Plains" => Color::White,
        "Island" => Color::Blue,
        "Swamp" => Color::Black,
        "Mountain" => Color::Red,
        "Forest" => Color::Green,
        _ => return None,
    })
}

fn step_by_name(w: &str) -> Option<Step> {
    Some(match w {
        "untap" => Step::Untap,
        "upkeep" => Step::Upkeep,
        "draw" => Step::Draw,
        "combat" => Step::BeginningOfCombat,
        "end" => Step::End,
        _ => return None,
    })
}

struct P {
    toks: Vec<Spanned>,
    pos: usize,
    /// Targets declared so far in the ability being parsed.
    targets: Vec<Target>,
    /// Inside a delayed trigger's effects, where "it" is the object the
    /// trigger is about.
    in_delayed: bool,
    /// The subject of the previous clause of the sentence being parsed.
    subject: Option<Recipient>,
}

impl P {
    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n).map(|t| &t.tok)
    }

    fn line(&self) -> usize {
        self.toks
            .get(self.pos)
            .or(self.toks.last())
            .map_or(0, |t| t.line)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|t| t.tok.clone());
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn fail<T>(&self, message: impl Into<String>) -> R<T> {
        Err(ParseError {
            line: self.line(),
            message: message.into(),
        })
    }

    fn unexpected<T>(&self, wanted: &str) -> R<T> {
        match self.peek() {
            Some(t) => self.fail(format!("expected {wanted}, found {t}")),
            None => self.fail(format!("expected {wanted}, found end of file")),
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek() == Some(&Tok::Newline) {
            self.pos += 1;
        }
    }

    /// The word at offset `n`, lowercased, if it is a word.
    fn word_at(&self, n: usize) -> Option<String> {
        match self.peek_at(n) {
            Some(Tok::Word(w)) => Some(w.to_ascii_lowercase()),
            _ => None,
        }
    }

    fn at_word(&self, w: &str) -> bool {
        self.word_at(0).as_deref() == Some(w)
    }

    fn at_words(&self, ws: &[&str]) -> bool {
        ws.iter().enumerate().all(|(i, w)| match self.peek_at(i) {
            Some(Tok::Word(x)) => x.eq_ignore_ascii_case(w),
            Some(Tok::Comma) => *w == ",",
            _ => false,
        })
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if self.at_word(w) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_words(&mut self, ws: &[&str]) -> bool {
        if self.at_words(ws) {
            self.pos += ws.len();
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, w: &str) -> R<()> {
        if self.eat_word(w) {
            Ok(())
        } else {
            self.unexpected(&format!("`{w}`"))
        }
    }

    fn expect(&mut self, t: &Tok) -> R<()> {
        if self.peek() == Some(t) {
            self.pos += 1;
            Ok(())
        } else {
            self.unexpected(&format!("`{t}`"))
        }
    }

    /// Any word, as written.
    fn word(&mut self) -> R<String> {
        if let Some(Tok::Word(w)) = self.peek() {
            let w = w.clone();
            self.pos += 1;
            return Ok(w);
        }
        self.unexpected("a word")
    }

    fn string(&mut self) -> R<String> {
        if let Some(Tok::Str(s)) = self.peek() {
            let s = s.clone();
            self.pos += 1;
            return Ok(s);
        }
        self.unexpected("a quoted string")
    }

    fn number(&mut self) -> R<u32> {
        if let Some(Tok::Num(n)) = self.peek() {
            let n = *n;
            self.pos += 1;
            return Ok(n);
        }
        self.unexpected("a number")
    }

    /// End of a statement: newline, `}`, or end of file (not consumed).
    fn at_stmt_end(&self) -> bool {
        matches!(self.peek(), None | Some(Tok::Newline | Tok::RBrace))
    }

    fn end_stmt(&mut self) -> R<()> {
        match self.peek() {
            Some(Tok::Newline) => {
                self.pos += 1;
                Ok(())
            }
            None | Some(Tok::RBrace) => Ok(()),
            Some(t) => self.fail(format!("unexpected {t} at end of statement")),
        }
    }

    // ---- cards ----

    fn card(&mut self) -> R<Card> {
        let line = self.line();
        let kind = match self.word()?.as_str() {
            "card" => Kind::Card,
            "token" => Kind::Token,
            "emblem" => Kind::Emblem,
            w => {
                return self.fail(format!(
                    "expected `card`, `token`, or `emblem`, found `{w}`"
                ));
            }
        };
        let name = self.string()?;
        self.expect(&Tok::LBrace)?;
        self.end_stmt()?;
        let mut card = Card {
            kind,
            name,
            oracle_id: None,
            cost: ManaCost::default(),
            color_indicator: None,
            supertypes: Vec::new(),
            types: Vec::new(),
            subtypes: Vec::new(),
            pt: None,
            loyalty: None,
            keywords: Vec::new(),
            additional: None,
            abilities: Vec::new(),
            except: None,
            mismatch: None,
            line,
        };
        loop {
            self.skip_newlines();
            if self.peek() == Some(&Tok::RBrace) {
                self.pos += 1;
                self.end_stmt()?;
                break;
            }
            if self.at_end() {
                return self.fail("unterminated card block");
            }
            self.card_statement(&mut card)?;
        }
        Ok(card)
    }

    #[allow(clippy::too_many_lines)]
    fn card_statement(&mut self, card: &mut Card) -> R<()> {
        let w = self.word_at(0).unwrap_or_default();
        match w.as_str() {
            "oracle_id" => {
                self.pos += 1;
                card.oracle_id = Some(self.word()?);
                self.end_stmt()
            }
            "cost" => {
                self.pos += 1;
                if self.eat_word("none") {
                    card.cost = ManaCost::default();
                } else {
                    card.cost = self.mana_cost()?;
                }
                self.end_stmt()
            }
            "type" => {
                self.pos += 1;
                self.type_line(card)?;
                self.end_stmt()
            }
            "pt" => {
                self.pos += 1;
                match self.next() {
                    Some(Tok::Pt {
                        power,
                        toughness,
                        signed: false,
                    }) => card.pt = Some((power, toughness)),
                    _ => return self.fail("expected power/toughness such as 2/2"),
                }
                self.end_stmt()
            }
            "loyalty" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                card.loyalty = Some(self.number()?);
                self.end_stmt()
            }
            "color" | "colors" => {
                self.pos += 1;
                let mut colors = Vec::new();
                while let Some(w) = self.word_at(0) {
                    let Some(c) = color(&w) else {
                        return self.fail(format!("unknown color `{w}`"));
                    };
                    colors.push(c);
                    self.pos += 1;
                    self.eat_word("and");
                }
                card.color_indicator = Some(colors);
                self.end_stmt()
            }
            "keyword" | "keywords" => {
                self.pos += 1;
                let ks = self.keyword_list()?;
                if card.abilities.is_empty() {
                    card.keywords.extend(ks);
                } else {
                    card.abilities.extend(ks.into_iter().map(Ability::Keyword));
                }
                self.end_stmt()
            }
            "additional" => {
                self.pos += 1;
                self.expect_word("cost")?;
                card.additional = Some(self.cost()?);
                self.end_stmt()
            }
            "oracle_mismatch" => {
                self.pos += 1;
                card.mismatch = Some(self.string()?);
                self.end_stmt()
            }
            "except" => {
                self.pos += 1;
                self.expect(&Tok::LBrace)?;
                self.end_stmt()?;
                let mut except = Except::default();
                loop {
                    self.skip_newlines();
                    if self.peek() == Some(&Tok::RBrace) {
                        self.pos += 1;
                        self.end_stmt()?;
                        break;
                    }
                    match self.word_at(0).as_deref() {
                        Some("subtype" | "subtypes") => {
                            self.pos += 1;
                            while let Some(Tok::Word(w)) = self.peek() {
                                except.subtypes.push(w.clone());
                                self.pos += 1;
                                self.eat_word("and");
                            }
                            self.end_stmt()?;
                        }
                        Some("keyword" | "keywords") => {
                            self.pos += 1;
                            except.keywords.extend(self.keyword_list()?);
                            self.end_stmt()?;
                        }
                        _ => except.abilities.push(self.ability()?),
                    }
                }
                card.except = Some(except);
                Ok(())
            }
            _ => {
                let a = self.ability()?;
                card.abilities.push(a);
                Ok(())
            }
        }
    }

    fn mana_cost(&mut self) -> R<ManaCost> {
        let mut text = String::new();
        while let Some(Tok::Sym(s)) = self.peek() {
            text.push('{');
            text.push_str(s);
            text.push('}');
            self.pos += 1;
        }
        ManaCost::parse(&text).map_err(|e| ParseError {
            line: self.line(),
            message: e.to_string(),
        })
    }

    fn type_line(&mut self, card: &mut Card) -> R<()> {
        let mut after_dash = false;
        while !self.at_stmt_end() {
            match self.next() {
                Some(Tok::Dash) => after_dash = true,
                Some(Tok::Word(w)) => {
                    if after_dash {
                        card.subtypes.push(w);
                    } else if let Some(s) = supertype(&w) {
                        card.supertypes.push(s);
                    } else if let Some(t) = card_type(&w) {
                        card.types.push(t);
                    } else {
                        return self.fail(format!("unknown type `{w}` (subtypes go after —)"));
                    }
                }
                Some(t) => return self.fail(format!("unexpected {t} in type line")),
                None => break,
            }
        }
        Ok(())
    }

    /// Keywords separated by commas or `and`.
    fn keyword_list(&mut self) -> R<Vec<Keyword>> {
        let mut out = Vec::new();
        loop {
            let Some(k) = self.keyword() else {
                if out.is_empty() {
                    return self.unexpected("a keyword");
                }
                break;
            };
            out.push(k);
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            self.eat_word("and");
        }
        Ok(out)
    }

    fn keyword(&mut self) -> Option<Keyword> {
        for (words, k) in KEYWORDS {
            if self.eat_words(words) {
                return Some(*k);
            }
        }
        None
    }

    // ---- abilities ----

    #[allow(clippy::too_many_lines)]
    fn ability(&mut self) -> R<Ability> {
        self.targets.clear();
        self.subject = None;
        let w = self.word_at(0).unwrap_or_default();
        match w.as_str() {
            "mana" => {
                self.pos += 1;
                let mut out = Vec::new();
                loop {
                    match self.next() {
                        Some(Tok::Sym(s)) => out.push(match s.as_str() {
                            "C" => Mana::Colorless,
                            "W" => Mana::Colored(Color::White),
                            "U" => Mana::Colored(Color::Blue),
                            "B" => Mana::Colored(Color::Black),
                            "R" => Mana::Colored(Color::Red),
                            "G" => Mana::Colored(Color::Green),
                            _ => return self.fail(format!("not a mana symbol: {{{s}}}")),
                        }),
                        _ => return self.fail("expected a mana symbol"),
                    }
                    if !(self.eat_word("or") || self.eat_word("and")) {
                        break;
                    }
                }
                self.end_stmt()?;
                Ok(Ability::Mana(out))
            }
            "enchant" => {
                self.pos += 1;
                let t = self.target_spec()?;
                self.end_stmt()?;
                Ok(Ability::Enchant(t))
            }
            "spell" => {
                self.pos += 1;
                let effects = self.effects_body()?;
                Ok(Ability::Spell {
                    targets: std::mem::take(&mut self.targets),
                    effects,
                })
            }
            "choose" => {
                self.pos += 1;
                self.expect_word("one")?;
                self.expect(&Tok::LBrace)?;
                self.end_stmt()?;
                let mut modes = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.peek() == Some(&Tok::RBrace) {
                        self.pos += 1;
                        self.end_stmt()?;
                        break;
                    }
                    self.expect_word("mode")?;
                    self.targets.clear();
                    self.subject = None;
                    let effects = self.effects_body()?;
                    modes.push(Mode {
                        targets: std::mem::take(&mut self.targets),
                        effects,
                    });
                }
                Ok(Ability::Modal(modes))
            }
            "activated" => {
                self.pos += 1;
                let cost = self.cost()?;
                let effects = self.effects_body()?;
                Ok(Ability::Activated {
                    cost,
                    targets: std::mem::take(&mut self.targets),
                    effects,
                    equip: false,
                })
            }
            "loyalty" => {
                self.pos += 1;
                let Some(Tok::Signed(n)) = self.next() else {
                    return self.fail("expected a loyalty cost such as +1 or -3");
                };
                let effects = self.effects_body()?;
                Ok(Ability::Activated {
                    cost: Cost {
                        loyalty: Some(n),
                        sorcery: true,
                        ..Default::default()
                    },
                    targets: std::mem::take(&mut self.targets),
                    effects,
                    equip: false,
                })
            }
            "equip" => {
                self.pos += 1;
                let mana = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Activated {
                    cost: Cost {
                        mana,
                        sorcery: true,
                        ..Default::default()
                    },
                    targets: vec![Target::Spec(TargetSpec::CreatureYouControl)],
                    effects: vec![Sentence::new(vec![Effect::Attach {
                        to: Recipient::Target(0),
                    }])],
                    equip: true,
                })
            }
            "triggered" => {
                self.pos += 1;
                let trigger = self.trigger()?;
                let condition = if self.eat_word("if") {
                    Some(self.condition()?)
                } else {
                    None
                };
                let effects = self.effects_body()?;
                Ok(Ability::Triggered {
                    trigger,
                    condition,
                    targets: std::mem::take(&mut self.targets),
                    effects,
                })
            }
            "replace" => {
                self.pos += 1;
                let r = self.replacement()?;
                Ok(Ability::Replacement(r))
            }
            "enters" if self.at_words(&["enters", "as"]) => {
                self.pos += 2;
                self.expect_word("copy")?;
                self.expect_word("of")?;
                let f = self.filter()?;
                self.end_stmt()?;
                Ok(Ability::EntersAsCopy(f))
            }
            "enters" => {
                self.pos += 1;
                let outcome = self.enters_outcome()?;
                self.end_stmt()?;
                Ok(Ability::Replacement(Replacement {
                    event: Replaces::Enters,
                    outcome,
                    then: Vec::new(),
                }))
            }
            "static" => {
                self.pos += 1;
                let filter = self.filter()?;
                self.expect(&Tok::Colon)?;
                let mut mods = Vec::new();
                loop {
                    mods.push(self.modification()?);
                    if self.peek() == Some(&Tok::Comma) {
                        self.pos += 1;
                        continue;
                    }
                    if self.eat_word("and") {
                        continue;
                    }
                    break;
                }
                self.end_stmt()?;
                Ok(Ability::Static { filter, mods })
            }
            _ => self.unexpected("a statement"),
        }
    }

    fn enters_outcome(&mut self) -> R<Outcome> {
        if self.eat_word("tapped") {
            return Ok(Outcome::EnterTapped);
        }
        if self.eat_word("with") {
            let n = self.num()?;
            let kind = self.counter_kind()?;
            return Ok(Outcome::EnterWithCounters { kind, n });
        }
        self.unexpected("`tapped` or `with N counters`")
    }

    /// `: effects` on one line, or `{` effect lines `}`.
    fn effects_body(&mut self) -> R<Vec<Sentence>> {
        match self.peek() {
            Some(Tok::Colon) => {
                self.pos += 1;
                let s = self.sentences_inline()?;
                self.end_stmt()?;
                Ok(s)
            }
            Some(Tok::LBrace) => {
                self.pos += 1;
                self.end_stmt()?;
                let mut out: Vec<Sentence> = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.peek() == Some(&Tok::RBrace) {
                        self.pos += 1;
                        self.end_stmt()?;
                        break;
                    }
                    if self.at_end() {
                        return self.fail("unterminated block");
                    }
                    let mut line = self.sentences_inline()?;
                    if !out.is_empty() {
                        line[0].para = true;
                    }
                    out.extend(line);
                    self.end_stmt()?;
                }
                Ok(out)
            }
            _ => self.unexpected("`:` or `{`"),
        }
    }

    /// Sentences separated by `;` up to the end of the line.
    fn sentences_inline(&mut self) -> R<Vec<Sentence>> {
        let mut out = Vec::new();
        loop {
            out.push(self.sentence()?);
            if self.peek() == Some(&Tok::Semi) {
                self.pos += 1;
                continue;
            }
            break;
        }
        Ok(out)
    }

    /// Clauses joined by `and`.
    fn sentence(&mut self) -> R<Sentence> {
        self.subject = None;
        let mut clauses = vec![self.clause()?];
        while self.at_word("and") {
            self.pos += 1;
            clauses.push(self.clause()?);
        }
        Ok(Sentence::new(clauses))
    }

    fn at_recipient(&self) -> bool {
        if matches!(self.peek(), Some(Tok::Num(_))) && self.word_at(1).as_deref() == Some("target")
        {
            return true;
        }
        let Some(w) = self.word_at(0) else {
            return false;
        };
        if NUMBER_WORDS.contains(&w.as_str()) && self.word_at(1).as_deref() == Some("target") {
            return true;
        }
        if w == "it" || w == "target" || w == "another" {
            return true;
        }
        if w == "any" && self.word_at(1).as_deref() == Some("target") {
            return true;
        }
        if w == "that" && matches!(self.word_at(1).as_deref(), Some("card" | "target")) {
            return true;
        }
        RECIPIENTS.iter().any(|(ws, _)| self.at_words(ws))
    }

    /// Verbs that take the previous clause's subject when none is written
    /// ("you draw a card and lose 1 life").
    fn at_subject_verb(&self) -> bool {
        matches!(
            self.word_at(0).as_deref(),
            Some(
                "gain"
                    | "gains"
                    | "lose"
                    | "loses"
                    | "draw"
                    | "draws"
                    | "get"
                    | "gets"
                    | "mill"
                    | "mills"
                    | "become"
                    | "becomes"
            )
        )
    }

    fn clause(&mut self) -> R<Effect> {
        // "you may ..." before "you" as a subject.
        if self.at_words(&["you", "may"]) {
            self.pos += 2;
            let inner = if self.peek() == Some(&Tok::LBrace) {
                self.effects_body()?
            } else {
                let saved = self.subject.take();
                let s = self.sentence()?;
                self.subject = saved;
                vec![s]
            };
            return Ok(Effect::YouMay(inner));
        }
        if self.at_words(&["this", "turn"]) {
            self.pos += 2;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            self.expect_word("replace")?;
            let r = self.replacement_inline()?;
            return Ok(Effect::CreateReplacement(r));
        }
        if self.at_recipient() {
            let subjects = self.recipients()?;
            let subject = if subjects.len() == 1 {
                subjects[0]
            } else {
                return self.fail("a sentence subject must be one recipient");
            };
            self.subject = Some(subject);
            return self.subject_verb(subject, true);
        }
        if self.at_subject_verb()
            && let Some(subject) = self.subject
            && !self.at_word("draw")
        {
            return self.subject_verb(subject, false);
        }
        self.verb_first()
    }

    /// A clause whose subject is known: "<subject> gains 3 life".
    #[allow(clippy::too_many_lines)]
    fn subject_verb(&mut self, subject: Recipient, explicit: bool) -> R<Effect> {
        let w = self.word()?;
        match w.as_str() {
            "gain" | "gains" => {
                if self.at_word("control") {
                    self.pos += 1;
                    self.expect_word("of")?;
                    let of = self.recipient()?;
                    self.until_end_of_turn();
                    return Ok(Effect::GainControl { of });
                }
                if let Some(k) = self.keyword() {
                    let mut keywords = vec![k];
                    loop {
                        if self.peek() == Some(&Tok::Comma) {
                            self.pos += 1;
                        }
                        if self.eat_word("and") && self.keyword_ahead() {
                            keywords.push(self.keyword().expect("keyword ahead"));
                            continue;
                        }
                        if let Some(k) = self.keyword() {
                            keywords.push(k);
                            continue;
                        }
                        break;
                    }
                    self.until_end_of_turn();
                    return Ok(Effect::Grant {
                        keywords,
                        to: subject,
                    });
                }
                let amount = self.num()?;
                self.expect_word("life")?;
                Ok(Effect::GainLife {
                    amount,
                    who: subject,
                })
            }
            "lose" | "loses" => {
                let amount = self.num()?;
                self.expect_word("life")?;
                Ok(Effect::LoseLife {
                    amount,
                    who: subject,
                })
            }
            "draw" | "draws" => {
                let count = self.num()?;
                self.cards_word()?;
                Ok(Effect::DrawCards {
                    count,
                    who: (explicit || subject != Recipient::You).then_some(subject),
                })
            }
            "mill" | "mills" => {
                let count = self.num()?;
                self.cards_word()?;
                Ok(Effect::Mill {
                    count,
                    who: subject,
                })
            }
            "get" | "gets" => match self.next() {
                Some(Tok::Pt {
                    power,
                    toughness,
                    signed: true,
                }) => {
                    self.until_end_of_turn();
                    Ok(Effect::Pump {
                        power,
                        toughness,
                        to: subject,
                    })
                }
                _ => self.fail("expected +N/+N"),
            },
            "become" | "becomes" => {
                self.eat_word("a");
                if self.eat_word("copy") {
                    self.expect_word("of")?;
                    let of = self.recipient()?;
                    let except = self.except_clause();
                    return Ok(Effect::BecomeCopy { of, except });
                }
                let Some(Tok::Pt {
                    power,
                    toughness,
                    signed: false,
                }) = self.next()
                else {
                    return self.fail("expected P/T after `becomes`");
                };
                let mut types = Vec::new();
                let mut subtypes = Vec::new();
                while let Some(Tok::Word(w)) = self.peek() {
                    if w == "until" {
                        break;
                    }
                    if let Some(t) = card_type(w) {
                        types.push(t);
                    } else {
                        subtypes.push(w.clone());
                    }
                    self.pos += 1;
                }
                self.until_end_of_turn();
                Ok(Effect::Animate {
                    to: subject,
                    types,
                    subtypes,
                    power,
                    toughness,
                })
            }
            _ => self.fail(format!("unknown verb `{w}` after a subject")),
        }
    }

    fn keyword_ahead(&self) -> bool {
        KEYWORDS.iter().any(|(ws, _)| self.at_words(ws))
    }

    /// `, except it has haste` / `with exceptions`: the card's `except`
    /// clause applies.
    fn except_clause(&mut self) -> bool {
        if self.peek() == Some(&Tok::Comma) && self.word_at(1).as_deref() == Some("except") {
            self.pos += 2;
            while !self.at_stmt_end() && self.peek() != Some(&Tok::Semi) && !self.at_word("and") {
                self.pos += 1;
            }
            return true;
        }
        self.eat_words(&["with", "exceptions"])
    }

    fn cards_word(&mut self) -> R<()> {
        if self.eat_word("card") || self.eat_word("cards") {
            Ok(())
        } else {
            self.unexpected("`card` or `cards`")
        }
    }

    fn until_end_of_turn(&mut self) {
        self.eat_words(&["until", "end", "of", "turn"]);
    }

    #[allow(clippy::too_many_lines)]
    fn verb_first(&mut self) -> R<Effect> {
        let w = self.word()?;
        match w.as_str() {
            "deal" | "deals" => {
                let amount = self.num()?;
                self.expect_word("damage")?;
                self.expect_word("to")?;
                let to = self.recipients()?;
                Ok(Effect::DealDamage { amount, to })
            }
            "destroy" => {
                let to = self.recipients()?;
                let no_regen = self.eat_words(&["without", "regeneration"]);
                Ok(Effect::Destroy { to, no_regen })
            }
            "put" => {
                let count = self.num()?;
                let kind = self.counter_kind()?;
                self.expect_word("on")?;
                let to = self.recipient()?;
                Ok(Effect::AddCounters { kind, count, to })
            }
            "remove" => {
                if self.eat_words(&["all", "counters", "from"]) {
                    let to = self.recipient()?;
                    return Ok(Effect::RemoveAllCounters { to });
                }
                let count = self.num()?;
                let kind = self.counter_kind()?;
                self.expect_word("from")?;
                let to = self.recipient()?;
                Ok(Effect::RemoveCounters { kind, count, to })
            }
            "return" => {
                let what = self.recipient()?;
                self.expect_word("to")?;
                if self.eat_words(&["its", "owner's", "hand"])
                    || self.eat_words(&["your", "hand"])
                    || self.eat_word("hand")
                {
                    return Ok(Effect::ReturnToHand { what });
                }
                self.eat_word("the");
                self.expect_word("battlefield")?;
                self.eat_words(&["under", "its", "owner's", "control"]);
                let counters = if self.eat_word("with") {
                    let n = self.count()?;
                    let kind = self.counter_kind()?;
                    self.eat_words(&["on", "it"]);
                    Some((kind, n))
                } else {
                    None
                };
                Ok(Effect::ReturnToBattlefield { what, counters })
            }
            "regenerate" => {
                let to = self.recipient()?;
                Ok(Effect::Regenerate { to })
            }
            "exile" => {
                if self.eat_words(&["all", "graveyards"]) {
                    return Ok(Effect::ExileAllGraveyards);
                }
                let what = self.recipient()?;
                Ok(Effect::Exile { what })
            }
            "sacrifice" => {
                let what = self.recipient()?;
                Ok(Effect::Sacrifice { what })
            }
            "tap" => {
                let to = self.recipients()?;
                Ok(Effect::Tap { to })
            }
            "untap" => {
                let to = self.recipients()?;
                Ok(Effect::Untap { to })
            }
            "gain" => {
                self.expect_word("control")?;
                self.expect_word("of")?;
                let of = self.recipient()?;
                self.until_end_of_turn();
                Ok(Effect::GainControl { of })
            }
            "draw" => {
                let count = self.num()?;
                self.cards_word()?;
                Ok(Effect::DrawCards { count, who: None })
            }
            "create" => {
                if self.eat_word("token") || self.eat_words(&["a", "token"]) {
                    self.expect_word("copy")?;
                    self.expect_word("of")?;
                    let of = self.recipient()?;
                    let except = self.except_clause();
                    return Ok(Effect::CreateTokenCopy { of, except });
                }
                let count = self.num()?;
                let token = self.string()?;
                self.eat_word("token");
                self.eat_word("tokens");
                Ok(Effect::CreateToken { token, count })
            }
            "attach" => {
                self.eat_word("this");
                self.expect_word("to")?;
                let to = self.recipient()?;
                Ok(Effect::Attach { to })
            }
            "emblem" => {
                let emblem = self.string()?;
                Ok(Effect::CreateEmblem { emblem })
            }
            "delayed" => {
                let about = self.recipient()?;
                self.expect(&Tok::Colon)?;
                let saved_subject = self.subject.take();
                let was = self.in_delayed;
                self.in_delayed = true;
                let (trigger, effects, trigger_first) =
                    if self.at_word("at") || self.at_word("when") || self.at_word("whenever") {
                        let t = self.trigger()?;
                        if self.peek() == Some(&Tok::Comma) {
                            self.pos += 1;
                        }
                        let e = self.sentences_inline()?;
                        (t, e, true)
                    } else {
                        let e = self.delayed_effects()?;
                        let t = self.trigger()?;
                        (t, e, false)
                    };
                self.in_delayed = was;
                self.subject = saved_subject;
                Ok(Effect::Delayed {
                    about,
                    trigger,
                    effects,
                    trigger_first,
                })
            }
            "prevent" => {
                let next = if self.eat_word("all") {
                    None
                } else {
                    self.eat_word("the");
                    self.expect_word("next")?;
                    Some(self.number()?)
                };
                let combat = if self.eat_word("combat") {
                    Some(true)
                } else if self.eat_word("noncombat") {
                    Some(false)
                } else {
                    None
                };
                self.expect_word("damage")?;
                let to = if self.eat_word("to") {
                    self.affects()?
                } else {
                    Affects::Any
                };
                self.expect_words(&["this", "turn"])?;
                Ok(Effect::CreateReplacement(Replacement {
                    event: Replaces::Damage {
                        from: None,
                        to,
                        combat,
                    },
                    outcome: match next {
                        Some(n) => Outcome::PreventNext(n),
                        None => Outcome::Prevent,
                    },
                    then: Vec::new(),
                }))
            }
            "players" => {
                self.expect_words(&["can't", "gain", "life", "this", "turn"])?;
                Ok(Effect::Rule(RuleKind::NoLifeGain))
            }
            "damage" => {
                self.expect_words(&["can't", "be", "prevented", "this", "turn"])?;
                Ok(Effect::Rule(RuleKind::DamageCantBePrevented))
            }
            _ => self.fail(format!("unknown effect `{w}`")),
        }
    }

    /// Effects of a delayed trigger written before its trigger: sentences up
    /// to the word `at`/`when`/`whenever`.
    fn delayed_effects(&mut self) -> R<Vec<Sentence>> {
        let mut out = Vec::new();
        loop {
            out.push(self.sentence()?);
            if self.peek() == Some(&Tok::Semi) {
                self.pos += 1;
                continue;
            }
            break;
        }
        Ok(out)
    }

    fn expect_words(&mut self, ws: &[&str]) -> R<()> {
        if self.eat_words(ws) {
            Ok(())
        } else {
            self.unexpected(&format!("`{}`", ws.join(" ")))
        }
    }

    fn num(&mut self) -> R<Num> {
        match self.peek() {
            Some(Tok::Num(n)) => {
                let n = *n;
                self.pos += 1;
                Ok(Num::N(n))
            }
            Some(Tok::Word(w)) => {
                let lw = w.to_ascii_lowercase();
                if lw == "x" {
                    self.pos += 1;
                    return Ok(Num::X);
                }
                if lw == "a" || lw == "an" {
                    self.pos += 1;
                    return Ok(Num::N(1));
                }
                if let Some(i) = NUMBER_WORDS.iter().position(|x| *x == lw) {
                    self.pos += 1;
                    return Ok(Num::N(u32::try_from(i + 1).expect("small")));
                }
                self.unexpected("a number")
            }
            _ => self.unexpected("a number"),
        }
    }

    fn count(&mut self) -> R<u32> {
        match self.num()? {
            Num::N(n) => Ok(n),
            Num::X => self.fail("X is not allowed here"),
        }
    }

    fn counter_kind(&mut self) -> R<CounterKind> {
        let kind = match self.next() {
            Some(Tok::Pt {
                power: 1,
                toughness: 1,
                signed: true,
            }) => CounterKind::PlusOnePlusOne,
            Some(Tok::Pt {
                power: -1,
                toughness: -1,
                signed: true,
            }) => CounterKind::MinusOneMinusOne,
            Some(Tok::Word(w)) if w == "loyalty" => CounterKind::Loyalty,
            _ => return self.fail("expected a counter kind (+1/+1, -1/-1, loyalty)"),
        };
        if !(self.eat_word("counter") || self.eat_word("counters")) {
            return self.unexpected("`counter` or `counters`");
        }
        Ok(kind)
    }

    // ---- recipients and targets ----

    /// One or more recipients joined by `and` ("each creature and each
    /// player", "two target lands").
    fn recipients(&mut self) -> R<Vec<Recipient>> {
        let mut out = self.recipient_group()?;
        while self.at_word("and") && self.recipient_follows_and() {
            self.pos += 1;
            out.extend(self.recipient_group()?);
        }
        Ok(out)
    }

    /// Whether `and` continues a recipient list rather than starting a new
    /// clause: a recipient that is not followed by a verb.
    fn recipient_follows_and(&self) -> bool {
        let mut probe = P {
            toks: self.toks.clone(),
            pos: self.pos + 1,
            targets: self.targets.clone(),
            in_delayed: self.in_delayed,
            subject: None,
        };
        if !probe.at_recipient() {
            return false;
        }
        if probe.recipient_group().is_err() {
            return false;
        }
        !probe.at_subject_verb()
    }

    /// A recipient, or `N target <spec>` declaring several.
    fn recipient_group(&mut self) -> R<Vec<Recipient>> {
        let count = match self.peek() {
            Some(Tok::Num(n)) if self.word_at(1).as_deref() == Some("target") => {
                let n = *n;
                self.pos += 1;
                n
            }
            Some(Tok::Word(w))
                if NUMBER_WORDS.contains(&w.to_ascii_lowercase().as_str())
                    && self.word_at(1).as_deref() == Some("target") =>
            {
                let n = NUMBER_WORDS
                    .iter()
                    .position(|x| x.eq_ignore_ascii_case(w))
                    .expect("number word");
                self.pos += 1;
                u32::try_from(n + 1).expect("small")
            }
            _ => 1,
        };
        if count > 1 {
            self.expect_word("target")?;
            let spec = self.target_spec()?;
            let mut out = Vec::new();
            for _ in 0..count {
                out.push(Recipient::Target(self.targets.len()));
                self.targets.push(spec.clone());
            }
            return Ok(out);
        }
        Ok(vec![self.recipient()?])
    }

    fn recipient(&mut self) -> R<Recipient> {
        if self.eat_word("it") {
            if self.in_delayed {
                return Ok(Recipient::SameObject);
            }
            return self.fail(
                "`it` is only meaningful inside a delayed trigger; name the target (`target 1`)",
            );
        }
        if self.at_words(&["that", "card"]) {
            self.pos += 2;
            return Ok(if self.in_delayed {
                Recipient::SameObject
            } else {
                Recipient::CreatedObject
            });
        }
        if self.eat_words(&["any", "target"]) {
            self.targets.push(Target::Spec(TargetSpec::Any));
            return Ok(Recipient::Target(self.targets.len() - 1));
        }
        if self.at_words(&["that", "target"]) {
            self.pos += 2;
            let Some(Tok::Num(n)) = self.next() else {
                return self.fail("expected a target number after `that target`");
            };
            let n = n as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(Recipient::ThatTarget(n - 1));
        }
        let another = self.eat_word("another");
        if self.eat_word("target") {
            if let Some(Tok::Num(n)) = self.peek() {
                let n = *n as usize;
                self.pos += 1;
                if n == 0 || n > self.targets.len() {
                    return self.fail(format!("target {n} has not been declared"));
                }
                return Ok(Recipient::Target(n - 1));
            }
            let mut spec = self.target_spec()?;
            if another {
                spec = match spec {
                    Target::Spec(TargetSpec::Creature) => Target::Spec(TargetSpec::OtherCreature),
                    Target::Spec(TargetSpec::Permanent) => Target::Spec(TargetSpec::OtherPermanent),
                    other => other,
                };
            }
            self.targets.push(spec);
            return Ok(Recipient::Target(self.targets.len() - 1));
        }
        if another {
            return self.unexpected("`target`");
        }
        for (ws, r) in RECIPIENTS {
            if self.eat_words(ws) {
                return Ok(*r);
            }
        }
        self.unexpected("a recipient (you, target ..., this, each creature, ...)")
    }

    /// Like `at_words`, also accepting the plural of each word ("two
    /// target lands").
    fn at_words_plural(&self, ws: &[&str]) -> bool {
        ws.iter().enumerate().all(|(i, w)| match self.peek_at(i) {
            Some(Tok::Word(x)) => {
                let x = x.to_ascii_lowercase();
                x == *w || x.strip_suffix('s') == Some(w)
            }
            Some(Tok::Comma) => *w == ",",
            _ => false,
        })
    }

    fn target_spec(&mut self) -> R<Target> {
        for (ws, spec) in TARGETS {
            if self.at_words_plural(ws) {
                self.pos += ws.len();
                return Ok(Target::Spec(*spec));
            }
        }
        if let (Some(Tok::Word(sub)), Some(Tok::Word(c))) = (self.peek_at(0), self.peek_at(1))
            && sub.chars().next().is_some_and(char::is_uppercase)
            && c == "creature"
        {
            let sub = sub.clone();
            self.pos += 2;
            return Ok(Target::CreatureOfSubtype(sub));
        }
        self.unexpected("a target specification")
    }

    // ---- filters ----

    fn filter(&mut self) -> R<Filter> {
        let mut f = Filter::default();
        let mut consumed = 0;
        while let Some(w) = self.word_at(0) {
            let raw = match self.peek() {
                Some(Tok::Word(x)) => x.clone(),
                _ => unreachable!(),
            };
            let before = self.pos;
            match w.as_str() {
                "an" | "opponents" | "opponent"
                    if self.at_words(&["an", "opponent", "controls"])
                        || self.at_words(&["opponents", "control"]) =>
                {
                    f.who = Some(Who::Opponents);
                    self.pos += if w == "an" { 3 } else { 2 };
                }
                "a" | "an" | "all" | "each" | "permanent" | "permanents" => self.pos += 1,
                "other" | "another" => {
                    f.relation = Relation::Other;
                    self.pos += 1;
                }
                "this" if self.word_at(1).as_deref() != Some("turn") => {
                    f.relation = Relation::This;
                    self.pos += 1;
                }
                "enchanted" | "equipped" => {
                    f.enchanted = true;
                    self.pos += 1;
                }
                "nonbasic" => {
                    f.not_supertypes.push(Supertype::Basic);
                    self.pos += 1;
                }
                "nonlegendary" => {
                    f.not_supertypes.push(Supertype::Legendary);
                    self.pos += 1;
                }
                "you" if self.at_words(&["you", "control"]) => {
                    f.who = Some(Who::You);
                    self.pos += 2;
                }
                "with" if self.at_words(&["with", "no", "abilities"]) => {
                    f.no_abilities = true;
                    self.pos += 3;
                }
                _ => {
                    if let Some(rest) = w.strip_prefix("non") {
                        let rest_raw = &raw[3..];
                        let rest_raw = rest_raw.strip_prefix('-').unwrap_or(rest_raw);
                        if let Some(t) = card_type(rest) {
                            f.not_types.push(t);
                        } else if rest_raw.chars().next().is_some_and(char::is_uppercase) {
                            f.not_subtypes.push(rest_raw.to_string());
                        } else {
                            break;
                        }
                        self.pos += 1;
                    } else if let Some(c) = color(&w) {
                        f.colors.get_or_insert_with(Vec::new).push(c);
                        self.pos += 1;
                    } else if let Some(t) = card_type(&w) {
                        f.types.push(t);
                        self.pos += 1;
                    } else if raw.chars().next().is_some_and(char::is_uppercase) {
                        f.subtypes.push(raw.trim_end_matches('s').to_string());
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            consumed += self.pos - before;
        }
        if consumed == 0 {
            return self.unexpected("a filter (creature you control, nonbasic land, ...)");
        }
        Ok(f)
    }

    fn affects(&mut self) -> R<Affects> {
        if self.at_words(&["any", "target"]) {
            return match self.recipient()? {
                Recipient::Target(i) => Ok(Affects::Target(i)),
                _ => unreachable!(),
            };
        }
        if self.eat_word("any") {
            return Ok(Affects::Any);
        }
        if self.at_words(&["this"]) && !self.at_words(&["this", "turn"]) {
            self.pos += 1;
            return Ok(Affects::This);
        }
        if self.at_words(&["you", "and"]) {
            self.pos += 2;
            return Ok(Affects::YouAnd(self.filter()?));
        }
        if self.eat_word("you") {
            return Ok(Affects::You);
        }
        if self.eat_words(&["each", "opponent"]) || self.eat_words(&["an", "opponent"]) {
            return Ok(Affects::Players(Who::Opponents));
        }
        if self.eat_words(&["each", "player"])
            || self.eat_words(&["a", "player"])
            || self.eat_words(&["any", "player"])
        {
            return Ok(Affects::Players(Who::Any));
        }
        if self.at_word("target") {
            return match self.recipient()? {
                Recipient::Target(i) => Ok(Affects::Target(i)),
                _ => unreachable!(),
            };
        }
        Ok(Affects::Objects(self.filter()?))
    }

    // ---- triggers, conditions, costs, replacements, modifications ----

    fn trigger_obj(&mut self) -> R<TriggerObj> {
        if self.at_word("this")
            && !matches!(self.word_at(1).as_deref(), Some("creature" | "permanent"))
        {
            self.pos += 1;
            return Ok(TriggerObj::This);
        }
        Ok(TriggerObj::Filter(self.filter()?))
    }

    #[allow(clippy::too_many_lines)]
    fn trigger(&mut self) -> R<Trigger> {
        if self.eat_word("at") {
            self.eat_word("the");
            self.expect_word("beginning")?;
            self.expect_word("of")?;
            let whose = if self.eat_word("your") {
                Who::You
            } else if self.eat_words(&["each", "player's"])
                || self.eat_word("each")
                || self.eat_word("the")
                || self.eat_words(&["the", "next"])
            {
                self.eat_word("next");
                Who::Any
            } else if self.eat_words(&["each", "opponent's"])
                || self.eat_words(&["an", "opponent's"])
            {
                Who::Opponents
            } else {
                return self.unexpected("`your`, `each`, or `the`");
            };
            let w = self.word()?;
            let Some(step) = step_by_name(&w) else {
                return self.fail(format!("unknown step `{w}`"));
            };
            self.eat_word("step");
            return Ok(Trigger::BeginningOfStep { step, whose });
        }
        if !(self.eat_word("when") || self.eat_word("whenever")) {
            return self.unexpected("`when`, `whenever`, or `at`");
        }
        if self.at_words(&["you", "gain", "life"]) {
            self.pos += 3;
            return Ok(Trigger::GainsLife { whose: Who::You });
        }
        if self.at_words(&["a", "player", "gains", "life"]) {
            self.pos += 4;
            return Ok(Trigger::GainsLife { whose: Who::Any });
        }
        if self.at_words(&["you", "cast"]) || self.at_words(&["an", "opponent", "casts"]) {
            let whose = if self.at_word("you") {
                Who::You
            } else {
                Who::Opponents
            };
            self.pos += if whose == Who::You { 2 } else { 3 };
            self.eat_word("a");
            self.eat_word("an");
            let noncreature = self.eat_word("noncreature");
            let instant_or_sorcery = self.eat_words(&["instant", "or", "sorcery"]);
            self.expect_word("spell")?;
            return Ok(Trigger::CastsSpell {
                whose,
                noncreature,
                instant_or_sorcery,
            });
        }
        let obj = self.trigger_obj()?;
        let w = self.word()?;
        match w.as_str() {
            "enters" => Ok(Trigger::Enters(obj)),
            "dies" => Ok(Trigger::Dies(obj)),
            "is" => {
                self.expect_words(&[
                    "put",
                    "into",
                    "a",
                    "graveyard",
                    "from",
                    "the",
                    "battlefield",
                ])?;
                Ok(Trigger::Dies(obj))
            }
            "attacks" => Ok(Trigger::Attacks(obj)),
            "blocks" => Ok(Trigger::Blocks(obj)),
            "deals" => {
                self.expect_words(&["combat", "damage", "to", "a", "player"])?;
                Ok(Trigger::DealsCombatDamageToPlayer(obj))
            }
            "becomes" => {
                if !(self.eat_word("targeted")
                    || self.eat_words(&["the", "target", "of", "a", "spell", "or", "ability"]))
                {
                    return self.unexpected("`targeted`");
                }
                Ok(Trigger::BecomesTarget(obj))
            }
            _ => self.fail(format!("unknown trigger event `{w}`")),
        }
    }

    fn condition(&mut self) -> R<Condition> {
        if self.eat_words(&["this", "is", "untapped"]) {
            return Ok(Condition::SourceUntapped);
        }
        if self.eat_words(&["no", "creatures", "are", "on", "the", "battlefield"]) {
            return Ok(Condition::NoCreatures);
        }
        self.unexpected("a condition")
    }

    fn cost(&mut self) -> R<Cost> {
        let mut cost = Cost::default();
        let mut mana = String::new();
        loop {
            match self.peek() {
                Some(Tok::Sym(_)) => {
                    let mut saw_mana = false;
                    while let Some(Tok::Sym(s)) = self.peek() {
                        if s == "T" {
                            cost.tap = true;
                            if saw_mana {
                                cost.order.push(CostItem::Mana);
                                saw_mana = false;
                            }
                            cost.order.push(CostItem::Tap);
                        } else {
                            mana.push('{');
                            mana.push_str(s);
                            mana.push('}');
                            saw_mana = true;
                        }
                        self.pos += 1;
                    }
                    if saw_mana {
                        cost.order.push(CostItem::Mana);
                    }
                }
                Some(Tok::Signed(n)) => {
                    cost.loyalty = Some(*n);
                    cost.sorcery = true;
                    self.pos += 1;
                }
                Some(Tok::Word(w)) => match w.to_ascii_lowercase().as_str() {
                    "sacrifice" => {
                        self.pos += 1;
                        if self.eat_word("this") {
                            cost.sacrifice = Some(SacCost::This);
                        } else {
                            cost.sacrifice = Some(SacCost::Filter(self.filter()?));
                        }
                        cost.order.push(CostItem::Sacrifice);
                    }
                    "discard" => {
                        self.pos += 1;
                        cost.discard = self.count()?;
                        self.cards_word()?;
                        cost.order.push(CostItem::Discard);
                    }
                    "pay" => {
                        self.pos += 1;
                        cost.life = self.count()?;
                        self.expect_word("life")?;
                        cost.order.push(CostItem::Life);
                    }
                    "sorcery" => {
                        self.pos += 1;
                        cost.sorcery = true;
                    }
                    "none" => self.pos += 1,
                    _ => return self.unexpected("a cost"),
                },
                _ => return self.unexpected("a cost"),
            }
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
                continue;
            }
            break;
        }
        cost.mana = ManaCost::parse(&mana).map_err(|e| ParseError {
            line: self.line(),
            message: e.to_string(),
        })?;
        Ok(cost)
    }

    fn replacement(&mut self) -> R<Replacement> {
        let mut r = self.replacement_inline()?;
        if self.peek() == Some(&Tok::LBrace) {
            r.then = self.effects_body()?;
        } else {
            self.end_stmt()?;
        }
        Ok(r)
    }

    fn replacement_inline(&mut self) -> R<Replacement> {
        let event = self.replaces()?;
        self.expect(&Tok::Colon)?;
        let outcome = self.outcome()?;
        Ok(Replacement {
            event,
            outcome,
            then: Vec::new(),
        })
    }

    fn replaces(&mut self) -> R<Replaces> {
        if self.eat_word("enters") {
            return Ok(Replaces::Enters);
        }
        if self.eat_word("destroy") {
            return Ok(Replaces::Destroy {
                to: self.affects()?,
            });
        }
        if self.at_words(&["you", "gain", "life"]) {
            self.pos += 3;
            return Ok(Replaces::GainLife { whose: Who::You });
        }
        if self.at_words(&["a", "player", "gains", "life"]) {
            self.pos += 4;
            return Ok(Replaces::GainLife { whose: Who::Any });
        }
        if self.at_words(&["an", "opponent", "gains", "life"]) {
            self.pos += 4;
            return Ok(Replaces::GainLife {
                whose: Who::Opponents,
            });
        }
        if self.at_words(&["you", "draw"]) || self.at_words(&["a", "player", "draws"]) {
            let whose = if self.at_word("you") {
                Who::You
            } else {
                Who::Any
            };
            self.pos += if whose == Who::You { 2 } else { 3 };
            self.eat_words(&["a", "card"]);
            let library_empty = self.eat_words(&["with", "empty", "library"])
                || self.eat_words(&["while", "your", "library", "has", "no", "cards", "in", "it"]);
            return Ok(Replaces::Draw {
                whose,
                library_empty,
            });
        }
        if self.at_words(&["card", "put", "into"]) || self.at_words(&["a", "card", "put", "into"]) {
            self.eat_word("a");
            self.pos += 3;
            let whose = if self.eat_words(&["an", "opponent's"]) {
                Who::Opponents
            } else if self.eat_word("your") {
                Who::You
            } else if self.eat_word("a") || self.eat_word("any") {
                Who::Any
            } else {
                return self.unexpected("`a`, `your`, or `an opponent's`");
            };
            self.expect_word("graveyard")?;
            let from_battlefield = self.eat_words(&["from", "the", "battlefield"]);
            self.eat_words(&["from", "anywhere"]);
            return Ok(Replaces::ToGraveyard {
                whose,
                from_battlefield,
            });
        }
        if matches!(
            self.peek(),
            Some(Tok::Pt {
                power: 1,
                toughness: 1,
                signed: true
            })
        ) {
            self.pos += 1;
            self.expect_words(&["counters", "put", "on"])?;
            return Ok(Replaces::PlusCounters { on: self.filter()? });
        }
        let combat = if self.eat_word("combat") {
            Some(true)
        } else if self.eat_word("noncombat") {
            Some(false)
        } else {
            None
        };
        self.expect_word("damage")?;
        let from = if self.eat_word("from") {
            Some(self.filter()?)
        } else {
            None
        };
        self.expect_word("to")?;
        let to = self.affects()?;
        Ok(Replaces::Damage { from, to, combat })
    }

    fn outcome(&mut self) -> R<Outcome> {
        let w = self.word()?;
        let out = match w.as_str() {
            "prevent" => {
                if self.eat_word("next") || self.eat_words(&["the", "next"]) {
                    Outcome::PreventNext(self.number()?)
                } else {
                    self.eat_word("it");
                    self.eat_words(&["that", "damage"]);
                    Outcome::Prevent
                }
            }
            "double" => Outcome::Double,
            "plus" => {
                self.expect_word("one")?;
                Outcome::PlusOne
            }
            "exile" => {
                self.eat_word("it");
                Outcome::Instead(Zone::Exile)
            }
            "draw" => {
                self.expect_word("two")?;
                self.eat_word("cards");
                Outcome::DrawTwo
            }
            "win" => {
                self.eat_words(&["the", "game"]);
                Outcome::WinGame
            }
            "tapped" => Outcome::EnterTapped,
            "with" => {
                let n = self.num()?;
                let kind = self.counter_kind()?;
                Outcome::EnterWithCounters { kind, n }
            }
            "regenerate" => Outcome::Regenerate,
            "redirect" => {
                self.expect_word("to")?;
                if !(self.eat_word("attached") || self.eat_word("enchanted")) {
                    return self.unexpected("`attached`");
                }
                Outcome::RedirectToAttached
            }
            _ => return self.fail(format!("unknown replacement outcome `{w}`")),
        };
        self.eat_word("instead");
        Ok(out)
    }

    #[allow(clippy::too_many_lines)]
    fn modification(&mut self) -> R<Mod> {
        if let Some(Tok::Pt {
            power,
            toughness,
            signed: true,
        }) = self.peek()
        {
            let (p, t) = (*power, *toughness);
            self.pos += 1;
            return Ok(Mod::ModifyPt {
                power: p,
                toughness: t,
            });
        }
        let w = self.word()?;
        match w.as_str() {
            "gets" | "get" => self.modification(),
            "base" => {
                if self.eat_word("mv") {
                    return Ok(Mod::SetPt {
                        power: PtValue::ManaValue,
                        toughness: PtValue::ManaValue,
                    });
                }
                match self.next() {
                    Some(Tok::Pt {
                        power,
                        toughness,
                        signed: false,
                    }) => Ok(Mod::SetPt {
                        power: PtValue::Fixed(power),
                        toughness: PtValue::Fixed(toughness),
                    }),
                    _ => self.fail("expected P/T or `mv` after `base`"),
                }
            }
            "has" | "have" => Ok(Mod::AddKeywords(self.keyword_list()?)),
            "loses" | "lose" => {
                self.expect_words(&["all", "abilities"])?;
                Ok(Mod::RemoveAllAbilities)
            }
            "no" => {
                self.expect_word("abilities")?;
                Ok(Mod::RemoveAllAbilities)
            }
            "control" => Ok(Mod::Control),
            "switch" => {
                self.eat_word("pt");
                Ok(Mod::SwitchPt)
            }
            "add" | "set" => {
                let set = w == "set";
                let what = self.word()?;
                match what.as_str() {
                    "type" | "types" => {
                        let mut types = Vec::new();
                        while let Some(x) = self.word_at(0) {
                            let Some(t) = card_type(&x) else { break };
                            types.push(t);
                            self.pos += 1;
                        }
                        if types.is_empty() || set {
                            return self.fail("expected `add type <types>`");
                        }
                        Ok(Mod::AddTypes(types))
                    }
                    "land" => {
                        let name = self.word()?;
                        let Some(c) = basic_land_type(&name) else {
                            return self.fail(format!("not a basic land type: {name}"));
                        };
                        Ok(if set {
                            Mod::SetLandType(c)
                        } else {
                            Mod::AddLandType(c)
                        })
                    }
                    "subtype" => {
                        if set {
                            return self.fail("`set subtype` is not supported");
                        }
                        Ok(Mod::AddSubtype(self.word()?))
                    }
                    "color" | "colors" => {
                        let colors = self.color_list()?;
                        Ok(if set {
                            Mod::SetColors(colors)
                        } else {
                            Mod::AddColors(colors)
                        })
                    }
                    _ => self.fail(format!("unknown modification `{w} {what}`")),
                }
            }
            "cda" => {
                let what = self.word()?;
                match what.as_str() {
                    "color" | "colors" => {
                        if self.eat_word("all") {
                            return Ok(Mod::CdaColors(Color::ALL.to_vec()));
                        }
                        Ok(Mod::CdaColors(self.color_list()?))
                    }
                    "pt" => {
                        let count = if self.eat_words(&["card", "types", "in", "graveyards"]) {
                            CdaCount::CardTypesInGraveyards
                        } else if self.eat_words(&["creature", "cards", "in", "graveyards"]) {
                            CdaCount::CreatureCardsInGraveyards
                        } else if self.eat_words(&["your", "life"]) {
                            self.eat_word("total");
                            CdaCount::ControllerLife
                        } else if self.eat_word("count") {
                            CdaCount::Permanents(self.filter()?)
                        } else {
                            return self.unexpected("what to count");
                        };
                        let plus_toughness = match self.peek() {
                            Some(Tok::Signed(n)) => {
                                let n = *n;
                                self.pos += 1;
                                n
                            }
                            _ => 0,
                        };
                        Ok(Mod::CdaPt {
                            count,
                            plus_toughness,
                        })
                    }
                    _ => self.fail(format!("unknown `cda {what}`")),
                }
            }
            _ => self.fail(format!("unknown modification `{w}`")),
        }
    }

    fn color_list(&mut self) -> R<Vec<Color>> {
        let mut out = Vec::new();
        while let Some(w) = self.word_at(0) {
            let Some(c) = color(&w) else { break };
            out.push(c);
            self.pos += 1;
            self.eat_word("and");
        }
        if out.is_empty() {
            return self.unexpected("a color");
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_vanilla_creature_and_a_spell() {
        let cards = parse(
            r#"
card "Grizzly Bears" {
  oracle_id 1e4c8b1d-0000-0000-0000-000000000000
  cost {1}{G}
  type Creature — Bear
  pt 2/2
}

card "Lightning Bolt" {
  cost {R}
  type Instant
  spell: deal 3 damage to target any
}
"#,
        )
        .unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].pt, Some((2, 2)));
        assert_eq!(cards[0].subtypes, vec!["Bear"]);
        match &cards[1].abilities[0] {
            Ability::Spell { targets, effects } => {
                assert_eq!(targets, &[Target::Spec(TargetSpec::Any)]);
                assert_eq!(
                    effects,
                    &[Sentence::new(vec![Effect::DealDamage {
                        amount: Num::N(3),
                        to: vec![Recipient::Target(0)]
                    }])]
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_joined_clauses_and_blocks() {
        let cards = parse(
            r#"
card "Blood Artist" {
  cost {1}{B}
  type Creature — Vampire
  pt 0/1
  triggered whenever creature dies: target player loses 1 life and you gain 1 life
  activated {1}{R}, sacrifice a Goblin {
    deal 2 damage to target any
  }
}
"#,
        )
        .unwrap();
        let Ability::Triggered { effects, .. } = &cards[0].abilities[0] else {
            panic!()
        };
        assert_eq!(effects[0].effects.len(), 2);
        let Ability::Activated { cost, .. } = &cards[0].abilities[1] else {
            panic!()
        };
        assert!(matches!(cost.sacrifice, Some(SacCost::Filter(_))));
        assert_eq!(cost.mana.to_string(), "{1}{R}");
    }

    #[test]
    fn reports_line_numbers() {
        let err = parse("card \"X\" {\n  cost {R}\n  bogus here\n}\n").unwrap_err();
        assert_eq!(err.line, 3);
    }
}
