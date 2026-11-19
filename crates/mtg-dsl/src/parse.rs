//! Parser for card files: `card "Name" { ... }` blocks of one statement per
//! line. See `SYNTAX.md` in this crate.

use mtg_core::card::{
    CardType, Cmp, ColorCount, Condition, CounterKind, Keyword, Num, PtValue, Relation,
    Replacement as Outcome, RuleKind, Supertype, TargetSpec, Who,
};
use mtg_core::mana::{Color, Mana, ManaCost};
use mtg_core::state::Zone;
use mtg_core::turn::Step;
use thiserror::Error;

use crate::ast::{
    Ability, Affects, BlockRule, Card, CdaCount, Cost, CostItem, Effect, Except, Filter, Join,
    Kind, Mechanic, Mod, Mode, Recipient, Replacement, Replaces, SacCost, Sentence, StaticCond,
    Target, Trigger, TriggerObj,
};
use crate::lex::{Spanned, Tok, lex};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("line {line}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

type R<T> = Result<T, ParseError>;

fn capitalize_word(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn parse(text: &str) -> R<Vec<Card>> {
    let toks = lex(text)?;
    let mut p = P {
        toks,
        pos: 0,
        targets: Vec::new(),
        groups: Vec::new(),
        counts: Vec::new(),
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
    (
        &["attacks", "each", "combat", "if", "able"],
        Keyword::MustAttack,
    ),
    (&["can't", "be", "blocked"], Keyword::Unblockable),
    (&["can't", "attack", "alone"], Keyword::CantAttackAlone),
    (&["can't", "block", "alone"], Keyword::CantBlockAlone),
    (&["can't", "attack"], Keyword::CantAttack),
    (&["can't", "block"], Keyword::CantBlock),
    (&["double", "strike"], Keyword::DoubleStrike),
    (&["first", "strike"], Keyword::FirstStrike),
    (&["deathtouch"], Keyword::Deathtouch),
    (&["plainswalk"], Keyword::Plainswalk),
    (&["islandwalk"], Keyword::Islandwalk),
    (&["swampwalk"], Keyword::Swampwalk),
    (&["mountainwalk"], Keyword::Mountainwalk),
    (&["forestwalk"], Keyword::Forestwalk),
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
    (
        &["protection", "from", "white"],
        Keyword::ProtectionFromWhite,
    ),
    (&["protection", "from", "blue"], Keyword::ProtectionFromBlue),
    (
        &["protection", "from", "black"],
        Keyword::ProtectionFromBlack,
    ),
    (&["protection", "from", "red"], Keyword::ProtectionFromRed),
    (
        &["protection", "from", "green"],
        Keyword::ProtectionFromGreen,
    ),
    (&["devoid"], Keyword::Devoid),
    (&["changeling"], Keyword::Changeling),
    (&["shroud"], Keyword::Shroud),
    (&["fear"], Keyword::Fear),
    (&["intimidate"], Keyword::Intimidate),
    (&["shadow"], Keyword::Shadow),
    (&["horsemanship"], Keyword::Horsemanship),
    (&["skulk"], Keyword::Skulk),
    (&["wither"], Keyword::Wither),
    (&["infect"], Keyword::Infect),
    (&["exalted"], Keyword::Exalted),
    (&["persist"], Keyword::Persist),
    (&["undying"], Keyword::Undying),
    (&["flanking"], Keyword::Flanking),
    (&["convoke"], Keyword::Convoke),
    (&["improvise"], Keyword::Improvise),
    (&["cascade"], Keyword::Cascade),
    (&["rebound"], Keyword::Rebound),
    (&["exploit"], Keyword::Exploit),
    (&["delve"], Keyword::Delve),
    (
        &[
            "doesn't",
            "untap",
            "during",
            "its",
            "controller's",
            "untap",
            "step",
        ],
        Keyword::DoesntUntap,
    ),
    (&["doesn't", "untap"], Keyword::DoesntUntap),
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
        &["attacking", "or", "blocking", "creature"],
        TargetSpec::AttackingOrBlockingCreature,
    ),
    (&["attacking", "creature"], TargetSpec::AttackingCreature),
    (&["blocking", "creature"], TargetSpec::BlockingCreature),
    (
        &["artifact", "or", "creature"],
        TargetSpec::ArtifactOrCreature,
    ),
    (&["noncreature", "spell"], TargetSpec::NoncreatureSpell),
    (&["creature", "spell"], TargetSpec::CreatureSpell),
    (&["spell"], TargetSpec::Spell),
    (
        &["opponent", "or", "planeswalker"],
        TargetSpec::OpponentOrPlaneswalker,
    ),
    (&["opponent"], TargetSpec::Opponent),
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
    /// Group recipients declared so far in the card being parsed.
    groups: Vec<Filter>,
    /// Variable amounts declared so far in the card being parsed.
    counts: Vec<crate::ast::Count>,
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
            display_name: None,
            layout: mtg_core::card::Layout::Normal,
            faces: Vec::new(),
            counts: Vec::new(),
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
            kicker: None,
            flashback: None,
            groups: Vec::new(),
            abilities: Vec::new(),
            except: None,
            mismatch: None,
            line,
        };
        self.groups.clear();
        self.counts.clear();
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
        card.groups = std::mem::take(&mut self.groups);
        card.counts = std::mem::take(&mut self.counts);
        Ok(card)
    }

    #[allow(clippy::too_many_lines)]
    fn card_statement(&mut self, card: &mut Card) -> R<()> {
        let w = self.word_at(0).unwrap_or_default();
        match w.as_str() {
            "name" => {
                self.pos += 1;
                card.display_name = Some(self.string()?);
                self.end_stmt()
            }
            "layout" => {
                self.pos += 1;
                card.layout = match self.word()?.as_str() {
                    "adventure" => mtg_core::card::Layout::Adventure,
                    "omen" => mtg_core::card::Layout::Omen,
                    "transform" => mtg_core::card::Layout::Transform,
                    "split" => mtg_core::card::Layout::Split,
                    w => {
                        return self.fail(format!(
                            "unknown layout `{w}` (adventure, omen, transform, split)"
                        ));
                    }
                };
                self.end_stmt()
            }
            "face" => {
                self.pos += 1;
                let line = self.line();
                let name = self.string()?;
                self.expect(&Tok::LBrace)?;
                self.end_stmt()?;
                let outer_groups = std::mem::take(&mut self.groups);
                let outer_counts = std::mem::take(&mut self.counts);
                let mut face = Card {
                    name,
                    line,
                    ..card.blank()
                };
                loop {
                    self.skip_newlines();
                    if self.peek() == Some(&Tok::RBrace) {
                        self.pos += 1;
                        self.end_stmt()?;
                        break;
                    }
                    if self.at_end() {
                        return self.fail("unterminated face block");
                    }
                    self.card_statement(&mut face)?;
                }
                face.groups = std::mem::replace(&mut self.groups, outer_groups);
                face.counts = std::mem::replace(&mut self.counts, outer_counts);
                card.faces.push(face);
                Ok(())
            }
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
            "kicker" => {
                self.pos += 1;
                card.kicker = Some(self.mana_cost()?);
                self.end_stmt()
            }
            "flashback" => {
                self.pos += 1;
                card.flashback = Some(self.cost()?);
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
            "spells" => {
                self.pos += 1;
                let filter = if self.at_word("you") || self.at_word("your") || self.at_word("cost")
                {
                    Filter::default()
                } else {
                    self.filter()?
                };
                let whose = if self.eat_words(&["you", "cast"]) {
                    Who::You
                } else if self.eat_words(&["your", "opponents", "cast"]) {
                    Who::Opponents
                } else {
                    Who::Any
                };
                self.expect_word("cost")?;
                let n = i32::try_from(self.mana_cost()?.mana_value()).unwrap_or(0);
                let generic = if self.eat_word("less") {
                    -n
                } else {
                    self.expect_word("more")?;
                    n
                };
                self.end_stmt()?;
                Ok(Ability::CostChange {
                    filter,
                    whose,
                    generic,
                })
            }
            "this" if self.at_words(&["this", "spell", "costs"]) => {
                self.pos += 3;
                let per = self.mana_cost()?.mana_value();
                self.expect_word("less")?;
                let times = self.num()?;
                self.end_stmt()?;
                Ok(Ability::SelfCost { per, times })
            }
            "protection"
                if self.word_at(1).as_deref() == Some("from")
                    && self.word_at(2).as_deref().and_then(color).is_none() =>
            {
                self.pos += 2;
                let filter = if self.eat_word("everything") {
                    Filter::default()
                } else {
                    self.filter()?
                };
                self.end_stmt()?;
                Ok(Ability::Protection(filter))
            }
            "hand" if self.word_at(1).as_deref() == Some("size") => {
                self.pos += 2;
                let whose = if self.eat_word("opponents") {
                    Who::Opponents
                } else {
                    Who::You
                };
                let delta = if self.eat_word("none") {
                    None
                } else {
                    match self.next() {
                        Some(Tok::Signed(n)) => Some(n),
                        _ => return self.fail("expected `none` or a signed number such as +1"),
                    }
                };
                self.end_stmt()?;
                Ok(Ability::HandSize { whose, delta })
            }
            "escapes" if self.word_at(1).as_deref() == Some("with") => {
                self.pos += 2;
                let n = self.number()?;
                if self.counter_kind()? != CounterKind::PlusOnePlusOne {
                    return self.fail("escapes with +1/+1 counters only");
                }
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::EscapesWith(n)))
            }
            "modular" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let n = self.number()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Modular(n)))
            }
            "cumulative" if self.word_at(1).as_deref() == Some("upkeep") => {
                self.pos += 2;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::CumulativeUpkeep(c)))
            }
            "exert" if self.peek_at(1) == Some(&Tok::Colon) => {
                self.pos += 1;
                let effects = self.effects_body()?;
                if !self.targets.is_empty() {
                    return self.fail("an exert ability has no targets");
                }
                Ok(Ability::Exert(effects))
            }
            "escape" if matches!(self.peek_at(1), Some(Tok::Sym(_))) => {
                self.pos += 1;
                let c = self.mana_cost()?;
                self.expect_word("exile")?;
                let n = self.number()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Escape(c, n)))
            }
            "bushido" | "soulshift" | "afflict" | "toxic" | "crew" | "bloodthirst" => {
                self.pos += 1;
                let n = self.number()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(match w.as_str() {
                    "bushido" => Mechanic::Bushido(n),
                    "soulshift" => Mechanic::Soulshift(n),
                    "afflict" => Mechanic::Afflict(n),
                    "crew" => Mechanic::Crew(n),
                    "bloodthirst" => Mechanic::Bloodthirst(n),
                    _ => Mechanic::Toxic(n),
                }))
            }
            "ward" | "echo" | "unearth" | "buyback" | "outlast" | "morph" | "megamorph"
            | "disguise" | "evoke" | "madness" | "foretell" | "entwine"
                if matches!(self.peek_at(1), Some(Tok::Sym(_))) =>
            {
                self.pos += 1;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(match w.as_str() {
                    "ward" => Mechanic::Ward(c),
                    "echo" => Mechanic::Echo(c),
                    "unearth" => Mechanic::Unearth(c),
                    "buyback" => Mechanic::Buyback(c),
                    "morph" => Mechanic::Morph(c),
                    "evoke" => Mechanic::Evoke(c),
                    "madness" => Mechanic::Madness(c),
                    "foretell" => Mechanic::Foretell(c),
                    "entwine" => Mechanic::Entwine(c),
                    "megamorph" => Mechanic::Megamorph(c),
                    "disguise" => Mechanic::Disguise(c),
                    _ => Mechanic::Outlast(c),
                }))
            }
            "suspend" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let n = self.number()?;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Suspend(n, c)))
            }
            "affinity" => {
                self.pos += 1;
                self.expect_word("for")?;
                let filter = self.filter()?;
                self.end_stmt()?;
                Ok(Ability::Affinity(filter))
            }
            w if w.ends_with("cycling") && w != "cycling"
                || w == "basic" && self.word_at(1).as_deref() == Some("landcycling") =>
            {
                // Typecycling (CR 702.29e): "Plainscycling {2}" searches
                // for a Plains card; "basic landcycling" for a basic land.
                let mut filter = Filter::default();
                if w == "basic" {
                    self.pos += 2;
                    filter.types.push(CardType::Land);
                    filter.supertypes.push(Supertype::Basic);
                } else {
                    self.pos += 1;
                    let stem = &w[..w.len() - "cycling".len()];
                    if stem == "land" {
                        filter.types.push(CardType::Land);
                    } else {
                        filter.subtypes.push(capitalize_word(stem));
                    }
                }
                let mana = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Activated {
                    cost: Cost {
                        mana,
                        discard_self: true,
                        ..Default::default()
                    },
                    targets: Vec::new(),
                    effects: vec![Sentence::new(vec![Effect::Search {
                        filter,
                        count: Num::N(1),
                        up_to: false,
                        to: mtg_core::card::SearchTo::Hand,
                        reveal: true,
                        that_card: false,
                    }])],
                    equip: false,
                    cycling: true,
                })
            }
            "cycling" => {
                self.pos += 1;
                let mana = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Activated {
                    cost: Cost {
                        mana,
                        discard_self: true,
                        ..Default::default()
                    },
                    targets: Vec::new(),
                    effects: vec![Sentence::new(vec![Effect::DrawCards {
                        count: Num::N(1),
                        who: None,
                    }])],
                    equip: false,
                    cycling: true,
                })
            }
            "mana" => {
                self.pos += 1;
                let mut out = Vec::new();
                loop {
                    if self.eat_word("any") {
                        out.push(Mana::Any);
                        if !(self.eat_word("or") || self.eat_word("and")) {
                            break;
                        }
                        continue;
                    }
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
                if self.peek() == Some(&Tok::Comma) {
                    self.next();
                    self.expect_word("sacrifice")?;
                    self.expect_word("this")?;
                    let [one] = out.as_slice() else {
                        return self.fail("a sacrifice mana ability makes one kind of mana");
                    };
                    let one = *one;
                    self.end_stmt()?;
                    return Ok(Ability::SacMana(one));
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
                let both = self.eat_words(&["or", "both"]);
                let modes = self.modes_block()?;
                self.end_stmt()?;
                Ok(if both {
                    Ability::ModalBoth(modes)
                } else {
                    Ability::Modal(modes)
                })
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
                    cycling: false,
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
                    cycling: false,
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
                    cycling: false,
                })
            }
            "triggered" => {
                self.pos += 1;
                let when = self.at_word("when");
                let mut trigger = self.trigger()?;
                if when && !crate::format::natural_when(&trigger) {
                    trigger = Trigger::When(Box::new(trigger));
                }
                let condition = if self.eat_word("if") {
                    let at = self.pos;
                    if let Ok(c) = self.condition() {
                        Some(crate::ast::TrigCond::Core(c))
                    } else {
                        self.pos = at;
                        Some(crate::ast::TrigCond::Static(self.static_cond()?))
                    }
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
            "enters" if self.at_words(&["enters", "tapped", "unless"]) => {
                self.pos += 3;
                let c = self.static_cond()?;
                self.end_stmt()?;
                Ok(Ability::EntersTappedUnless(c))
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
            "label" => {
                self.pos += 1;
                let label = self.string()?;
                let ability = self.ability()?;
                Ok(Ability::Labeled {
                    label,
                    ability: Box::new(ability),
                })
            }
            "can't"
                if self.at_words(&["can't", "attack", "unless"])
                    || self.at_words(&["can't", "block", "unless"])
                    || self.at_words(&["can't", "attack", "or", "block", "unless"]) =>
            {
                self.pos += 1;
                let (attack, block) = if self.eat_words(&["attack", "or", "block"]) {
                    (true, true)
                } else if self.eat_word("attack") {
                    (true, false)
                } else {
                    self.expect_word("block")?;
                    (false, true)
                };
                self.expect_word("unless")?;
                let cond = self.static_cond()?;
                self.end_stmt()?;
                Ok(Ability::AttackUnless {
                    cond,
                    attack,
                    block,
                })
            }
            "can't" | "can" => {
                self.pos += 1;
                let rule = if w == "can" {
                    self.expect_words(&["block", "only"])?;
                    BlockRule::BlocksOnly(self.filter()?)
                } else if self.eat_word("block") {
                    BlockRule::CantBlock(self.filter()?)
                } else {
                    self.expect_words(&["be", "blocked"])?;
                    if self.eat_words(&["by", "more", "than", "one", "creature"]) {
                        BlockRule::AtMostOne
                    } else if self.eat_words(&["except", "by"]) {
                        BlockRule::OnlyBy(self.filter()?)
                    } else {
                        self.expect_word("by")?;
                        BlockRule::NotBy(self.filter()?)
                    }
                };
                self.end_stmt()?;
                Ok(Ability::BlockRule(rule))
            }
            "static" => {
                self.pos += 1;
                let mut condition = None;
                let mut cond_first = false;
                if self.eat_words(&["as", "long", "as"]) {
                    condition = Some(self.static_cond()?);
                    cond_first = true;
                    self.expect(&Tok::Comma)?;
                } else if self.eat_words(&["during", "your", "turn"]) {
                    condition = Some(StaticCond::DuringYourTurn);
                    cond_first = true;
                    self.expect(&Tok::Comma)?;
                }
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
                if condition.is_none() && self.eat_words(&["as", "long", "as"]) {
                    condition = Some(self.static_cond()?);
                }
                self.end_stmt()?;
                Ok(Ability::Static {
                    filter,
                    mods,
                    condition,
                    cond_first,
                })
            }
            _ => self.unexpected("a statement"),
        }
    }

    /// `7 cards in your graveyard`, `4 card types in your graveyard`,
    /// `you control [3] <filter>`, `your turn`, or the ability words
    /// `threshold`, `delirium`, `metalcraft`.
    /// `{ <ability line> }`: an ability granted by another ("has \"...\""),
    /// parsed with targets of its own.
    fn nested_ability(&mut self) -> R<Ability> {
        self.expect(&Tok::LBrace)?;
        self.skip_newlines();
        let targets = std::mem::take(&mut self.targets);
        let subject = self.subject.take();
        let a = self.ability();
        self.targets = targets;
        self.subject = subject;
        let a = a?;
        self.skip_newlines();
        self.expect(&Tok::RBrace)?;
        Ok(a)
    }

    /// `{ mode: <effects> ... }`: each mode with its own targets.
    fn modes_block(&mut self) -> R<Vec<Mode>> {
        self.expect(&Tok::LBrace)?;
        self.end_stmt()?;
        let outer = std::mem::take(&mut self.targets);
        let mut modes = Vec::new();
        loop {
            self.skip_newlines();
            if self.peek() == Some(&Tok::RBrace) {
                self.pos += 1;
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
        self.targets = outer;
        Ok(modes)
    }

    fn static_cond(&mut self) -> R<StaticCond> {
        if self.eat_word("threshold") {
            return Ok(StaticCond::CardsInGraveyard(7));
        }
        if self.eat_word("delirium") {
            return Ok(StaticCond::CardTypesInGraveyard(4));
        }
        if self.eat_word("metalcraft") {
            return Ok(StaticCond::YouControl {
                filter: Filter {
                    types: vec![CardType::Artifact],
                    ..Default::default()
                },
                at_least: 3,
            });
        }
        if self.eat_words(&["your", "turn"]) {
            return Ok(StaticCond::YourTurn);
        }
        if self.eat_word("attacking") {
            return Ok(StaticCond::Attacking);
        }
        if self.eat_words(&["attacked", "this", "turn"]) {
            return Ok(StaticCond::AttackedThisTurn);
        }
        if self.eat_words(&["creature", "died", "this", "turn"]) {
            return Ok(StaticCond::CreatureDiedThisTurn);
        }
        if self.eat_words(&["opponent", "damaged", "this", "turn"]) {
            return Ok(StaticCond::OpponentDamagedThisTurn);
        }
        if self.eat_words(&["player", "life"]) {
            let n = i32::try_from(self.number()?).unwrap_or(0);
            self.expect_words(&["or", "less"])?;
            return Ok(StaticCond::AnyPlayerLifeAtMost(n));
        }
        if self.eat_word("hand") {
            let n = self.number()?;
            self.expect_word("or")?;
            let at_most = if self.eat_word("less") {
                true
            } else {
                self.expect_word("more")?;
                false
            };
            return Ok(StaticCond::CardsInHand { n, at_most });
        }
        if self.eat_words(&["defending", "player", "controls"]) {
            let at_least = if matches!(self.peek(), Some(Tok::Num(_))) {
                self.number()?
            } else {
                1
            };
            let filter = self.filter()?;
            return Ok(StaticCond::DefenderControls { filter, at_least });
        }
        if self.eat_words(&["you", "control", "no"]) {
            return Ok(StaticCond::YouControlNone(self.filter()?));
        }
        if self.eat_word("enchanted") {
            return Ok(StaticCond::Enchanted);
        }
        if self.eat_word("equipped") {
            return Ok(StaticCond::Equipped);
        }
        if self.eat_words(&["opponent", "life"]) {
            let n = i32::try_from(self.number()?).unwrap_or(0);
            self.expect_words(&["or", "less"])?;
            return Ok(StaticCond::OpponentLifeAtMost(n));
        }
        if self.eat_words(&["your", "life"]) {
            let n = i32::try_from(self.number()?).unwrap_or(0);
            self.expect_word("or")?;
            let at_least = if self.eat_word("more") {
                true
            } else {
                self.expect_word("less")?;
                false
            };
            return Ok(StaticCond::YourLife { n, at_least });
        }
        if self.eat_words(&["you", "control"]) {
            let at_least = if matches!(self.peek(), Some(Tok::Num(_))) {
                self.number()?
            } else {
                1
            };
            let filter = self.filter()?;
            return Ok(StaticCond::YouControl { filter, at_least });
        }
        let n = self.number()?;
        if self.eat_words(&["cards", "in", "your", "graveyard"]) {
            return Ok(StaticCond::CardsInGraveyard(n));
        }
        if self.eat_words(&["card", "types", "in", "your", "graveyard"]) {
            return Ok(StaticCond::CardTypesInGraveyard(n));
        }
        self.unexpected("a condition (threshold, 7 cards in your graveyard, you control 3 artifact, your turn, ...)")
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
        // "until end of turn, <clauses>": the duration printed first.
        let eot_first = self.eat_words(&["until", "end", "of", "turn"]);
        if eot_first && self.peek() == Some(&Tok::Comma) {
            self.pos += 1;
        }
        let first = self.clause()?;
        let mut clauses = vec![self.unless_suffix(first)?];
        let mut joins = Vec::new();
        loop {
            if self.at_word("and") {
                self.pos += 1;
                joins.push(Join::And);
            } else if self.peek() == Some(&Tok::Comma) && self.word_at(1).as_deref() == Some("and")
            {
                self.pos += 2;
                joins.push(Join::AndRepeat);
            } else if self.at_word("then")
                || (self.peek() == Some(&Tok::Comma) && self.word_at(1).as_deref() == Some("then"))
            {
                if self.peek() == Some(&Tok::Comma) {
                    self.pos += 1;
                }
                self.pos += 1;
                joins.push(Join::Then);
            } else {
                break;
            }
            let c = self.clause()?;
            clauses.push(self.unless_suffix(c)?);
        }
        Ok(Sentence {
            effects: clauses,
            para: false,
            eot_first,
            joins,
        })
    }

    /// `<clause> unless you pay {2}` / `unless they pay 2 life` / `unless
    /// <recipient> pays {2}`.
    fn unless_suffix(&mut self, e: Effect) -> R<Effect> {
        if !self.at_word("unless") {
            return Ok(e);
        }
        self.pos += 1;
        let payer = if self.eat_words(&["you", "pay"]) {
            Recipient::You
        } else if self.eat_words(&["they", "pay"]) {
            match self.subject {
                Some(s) => s,
                None => return self.fail("`unless they pay` needs a clause with a subject"),
            }
        } else {
            let r = self.recipient()?;
            self.expect_word("pays")?;
            r
        };
        let (mana, life) = self.pay_amount()?;
        Ok(Effect::UnlessPays {
            payer,
            mana,
            life,
            effect: Box::new(e),
        })
    }

    /// `{2}` or `2 life`.
    fn pay_amount(&mut self) -> R<(ManaCost, u32)> {
        if matches!(self.peek(), Some(Tok::Sym(_))) {
            return Ok((self.mana_cost()?, 0));
        }
        let n = self.number()?;
        self.expect_word("life")?;
        Ok((ManaCost::default(), n))
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
        if w == "targets" && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            return true;
        }
        if w == "up" && self.word_at(1).as_deref() == Some("to") {
            return true;
        }
        if w == "attached"
            || (matches!(w.as_str(), "enchanted" | "equipped")
                && matches!(
                    self.word_at(1).as_deref(),
                    Some("creature" | "land" | "permanent")
                ))
        {
            return true;
        }
        if w == "its" && matches!(self.word_at(1).as_deref(), Some("controller" | "owner")) {
            return true;
        }
        if w == "any" && self.word_at(1).as_deref() == Some("target") {
            return true;
        }
        if w == "that" && matches!(self.word_at(1).as_deref(), Some("card" | "target")) {
            return true;
        }
        if w == "each" || w == "all" {
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
                    | "discard"
                    | "discards"
                    | "fights"
                    | "can't"
                    | "sacrifices"
                    | "sacrifice"
                    | "reveals"
                    | "reveal"
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
        // "choose one { mode: ... }": a modal triggered ability.
        if self.at_words(&["choose", "one"]) && self.peek_at(2) == Some(&Tok::LBrace) {
            self.pos += 2;
            return Ok(Effect::Modes(self.modes_block()?));
        }
        // "if you do, ...": after an optional action.
        if self.at_words(&["if", "you", "do"]) {
            self.pos += 3;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfYouDo(vec![s]));
        }
        // "if kicked, ..." (CR 702.33c).
        if self.at_words(&["if", "kicked"]) {
            self.pos += 2;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            let inner = if self.peek() == Some(&Tok::LBrace) {
                self.effects_body()?
            } else {
                let saved = self.subject.take();
                let s = self.sentence()?;
                self.subject = saved;
                vec![s]
            };
            return Ok(Effect::IfKicked(inner));
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
            // "up to two target creatures each get": consecutive targets
            // as one subject.
            let group = match subjects.first() {
                Some(Recipient::Target(a)) if subjects.len() > 1 => subjects
                    .iter()
                    .enumerate()
                    .all(|(k, r)| *r == Recipient::Target(a + k))
                    .then_some(Recipient::Targets(*a, subjects.len())),
                _ => None,
            };
            if group.is_some() {
                self.eat_word("each");
            }
            let subject = if let Some(g) = group {
                g
            } else if subjects.len() == 1 {
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
            && !self.at_word("mill")
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
            "gain" | "gains" if self.at_word("ability") => {
                self.pos += 1;
                let ability = Box::new(self.nested_ability()?);
                self.until_end_of_turn();
                Ok(Effect::GrantAbility {
                    ability,
                    to: subject,
                })
            }
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
            "lose" | "loses" if self.keyword_ahead() => {
                let mut keywords = Vec::new();
                while let Some(k) = self.keyword() {
                    keywords.push(k);
                    if self.peek() == Some(&Tok::Comma) {
                        self.pos += 1;
                    }
                    if self.at_word("and") {
                        self.pos += 1;
                        if !self.keyword_ahead() {
                            self.pos -= 1;
                            break;
                        }
                    } else if !self.keyword_ahead() {
                        break;
                    }
                }
                self.until_end_of_turn();
                Ok(Effect::Strip {
                    keywords,
                    to: subject,
                })
            }
            "sacrifices" | "sacrifice" => {
                let count = if self.eat_word("a") || self.eat_word("an") {
                    Num::N(1)
                } else {
                    self.num()?
                };
                let filter = self.filter()?;
                self.eat_words(&["of", "their", "choice"]);
                Ok(Effect::SacrificeChosen {
                    who: subject,
                    count,
                    filter,
                })
            }
            "reveal" | "reveals" => {
                self.eat_word("their");
                self.expect_word("hand")?;
                if self.eat_word("discarding") {
                    let filter = if self.eat_word("card") {
                        None
                    } else {
                        let f = self.filter()?;
                        self.expect_word("card")?;
                        Some(f)
                    };
                    return Ok(Effect::DiscardChosen {
                        who: subject,
                        filter,
                    });
                }
                Ok(Effect::RevealHand { who: subject })
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
                    who: (explicit || subject != Recipient::You).then_some(subject),
                })
            }
            "discard" | "discards" => {
                let count = self.num()?;
                self.cards_word()?;
                if self.eat_words(&["at", "random"]) {
                    return Ok(Effect::DiscardRandom {
                        count,
                        who: subject,
                    });
                }
                Ok(Effect::Discard {
                    count,
                    who: (explicit || subject != Recipient::You).then_some(subject),
                })
            }
            "fights" => {
                let b = self.recipient()?;
                Ok(Effect::Fight { a: subject, b })
            }
            "has" | "have" => {
                self.expect_words(&["base", "power", "and", "toughness"])?;
                let Some(Tok::Pt {
                    power,
                    toughness,
                    signed: false,
                }) = self.next()
                else {
                    return self.fail("expected N/N after `base power and toughness`");
                };
                self.until_end_of_turn();
                Ok(Effect::SetBasePt {
                    to: subject,
                    power,
                    toughness,
                })
            }
            "can't" => {
                let keywords = if self.eat_word("block") {
                    vec![Keyword::CantBlock]
                } else if self.eat_word("attack") {
                    if self.eat_words(&["or", "block"]) {
                        vec![Keyword::CantAttack, Keyword::CantBlock]
                    } else {
                        vec![Keyword::CantAttack]
                    }
                } else if self.eat_words(&["be", "blocked"]) {
                    vec![Keyword::Unblockable]
                } else {
                    return self.unexpected("`block`, `attack`, or `be blocked`");
                };
                self.eat_words(&["this", "turn"]);
                self.until_end_of_turn();
                Ok(Effect::Grant {
                    keywords,
                    to: subject,
                })
            }
            "get" | "gets"
                if matches!(self.peek(), Some(Tok::Num(_)))
                    && matches!(self.word_at(1).as_deref(), Some("energy")) =>
            {
                let count = self.num()?;
                self.pos += 1;
                Ok(Effect::AddEnergy {
                    count,
                    who: subject,
                })
            }
            "get" | "gets"
                if matches!(self.peek(), Some(Tok::Num(_)))
                    && matches!(self.word_at(1).as_deref(), Some("poison")) =>
            {
                let count = self.num()?;
                self.pos += 1;
                if !(self.eat_word("counters") || self.eat_word("counter")) {
                    return self.unexpected("`counters`");
                }
                Ok(Effect::AddPoison {
                    count,
                    who: subject,
                })
            }
            "explores" => Ok(Effect::Explore { what: subject }),
            "connives" => Ok(Effect::Connive { what: subject }),
            "doesn't" => {
                self.expect_words(&["untap", "next", "untap", "step"])?;
                Ok(Effect::SkipNextUntap { to: subject })
            }
            "get" | "gets" => match self.next() {
                Some(Tok::Pt {
                    power,
                    toughness,
                    signed: true,
                }) if self.at_word("count") || self.at_word("x") => {
                    let times = self.num()?;
                    self.until_end_of_turn();
                    Ok(Effect::PumpVar {
                        power,
                        toughness,
                        times,
                        to: subject,
                    })
                }
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
            "put"
                if (0..8).any(|k| {
                    self.word_at(k).as_deref() == Some("from")
                        && self.word_at(k + 1).as_deref() == Some("hand")
                }) =>
            {
                let filter = if self.at_word("card") {
                    Filter::default()
                } else {
                    self.filter()?
                };
                self.expect_words(&["card", "from", "hand", "onto", "battlefield"])?;
                let tapped = self.eat_word("tapped");
                Ok(Effect::PutFromHand { filter, tapped })
            }
            "put"
                if !matches!(self.peek(), Some(Tok::Num(_)))
                    && !self
                        .word_at(0)
                        .is_some_and(|w| NUMBER_WORDS.contains(&w.as_str()) || w == "x") =>
            {
                let what = self.recipient()?;
                self.expect_word("on")?;
                let top = if self.eat_word("top") {
                    true
                } else {
                    self.expect_word("bottom")?;
                    false
                };
                self.expect_word("of")?;
                self.eat_words(&["its", "owner's"]);
                self.expect_word("library")?;
                Ok(Effect::PutInLibrary { what, top })
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
            // `return 1 land you control chosen`: the controller chooses.
            "return"
                if matches!(self.peek(), Some(Tok::Num(_)))
                    && (1..8).any(|k| self.word_at(k).as_deref() == Some("chosen")) =>
            {
                let count = self.num()?;
                let filter = self.filter()?;
                self.expect_word("chosen")?;
                Ok(Effect::ReturnChosen { count, filter })
            }
            "return" => {
                let all = self.recipients()?;
                let what = all[0];
                self.expect_word("to")?;
                if self.eat_words(&["its", "owner's", "hand"])
                    || self.eat_words(&["their", "owners'", "hands"])
                    || self.eat_words(&["your", "hand"])
                    || self.eat_word("hand")
                {
                    return Ok(Effect::ReturnToHand { what: all });
                }
                if all.len() > 1 {
                    return self.fail("only one recipient returns to the battlefield");
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
                let what = self.recipients()?;
                Ok(Effect::Exile { what })
            }
            "pay"
                if matches!(self.peek(), Some(Tok::Num(_)))
                    && self.word_at(1).as_deref() == Some("energy") =>
            {
                let n = self.count()?;
                self.pos += 1;
                Ok(Effect::PayEnergy(n))
            }
            "pay" => {
                let (mana, life) = self.pay_amount()?;
                Ok(Effect::Pay { mana, life })
            }
            "proliferate" => Ok(Effect::Proliferate),
            "adapt" => Ok(Effect::Adapt(self.count()?)),
            "discover" => Ok(Effect::Discover(self.count()?)),
            "dig" => {
                let count = self.num()?;
                let reveal_top = self.eat_word("reveal");
                self.expect_word("take")?;
                let all = self.eat_word("all");
                let filter = if self.eat_word("card") {
                    None
                } else {
                    let f = self.filter()?;
                    self.expect_word("card")?;
                    Some(f)
                };
                let optional = self.eat_word("may");
                let revealing = self.eat_word("revealing");
                self.expect_word("rest")?;
                let rest_graveyard = if self.eat_word("graveyard") {
                    true
                } else {
                    self.expect_word("bottom")?;
                    false
                };
                Ok(Effect::Dig {
                    count,
                    filter,
                    optional,
                    all,
                    rest_graveyard,
                    reveal_top,
                    revealing,
                })
            }
            "look" if self.at_words(&["at", "hand", "of"]) => {
                self.pos += 3;
                Ok(Effect::LookAtHand {
                    who: self.recipient()?,
                })
            }
            "look" if self.at_words(&["at", "top"]) => {
                self.pos += 2;
                Ok(Effect::LookAtTop(self.num()?))
            }
            "manifest" => {
                self.eat_words(&["the", "top", "card", "of", "your", "library"]);
                Ok(Effect::Manifest)
            }
            "switch" => {
                self.expect_words(&["pt", "of"])?;
                let to = self.recipient()?;
                self.until_end_of_turn();
                Ok(Effect::SwitchPt { to })
            }

            "amass" => {
                let kind = self.word()?;
                let count = self.num()?;
                Ok(Effect::Amass { kind, count })
            }
            "clear" => {
                self.expect_word("echo")?;
                Ok(Effect::ClearEcho)
            }
            "sacrifice" => {
                let what = self.recipient()?;
                Ok(Effect::Sacrifice { what })
            }
            "tap" if self.at_words(&["or", "untap"]) => {
                self.pos += 2;
                Ok(Effect::TapOrUntap {
                    to: self.recipient()?,
                })
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
            "mill" => {
                let count = self.num()?;
                self.cards_word()?;
                Ok(Effect::Mill { count, who: None })
            }
            "discard" => {
                let count = self.num()?;
                self.cards_word()?;
                Ok(Effect::Discard { count, who: None })
            }
            "scry" => Ok(Effect::Scry(self.num()?)),
            "investigate" => Ok(Effect::Investigate),
            "transform" => Ok(Effect::Transform {
                what: self.recipient()?,
            }),
            "search" => {
                let up_to = self.eat_words(&["up", "to"]);
                let count = if matches!(self.peek(), Some(Tok::Num(_))) {
                    self.num()?
                } else {
                    Num::N(1)
                };
                let filter = if self.eat_word("card") || self.eat_word("cards") {
                    Filter::default()
                } else {
                    let f = self.filter()?;
                    if !(self.eat_word("card") || self.eat_word("cards")) {
                        return self.unexpected("`card`");
                    }
                    f
                };
                let reveal = self.eat_word("reveal");
                self.expect_word("to")?;
                let to = if self.eat_word("hand") {
                    mtg_core::card::SearchTo::Hand
                } else if self.eat_word("battlefield") {
                    mtg_core::card::SearchTo::Battlefield {
                        tapped: self.eat_word("tapped"),
                    }
                } else if self.eat_word("top") {
                    mtg_core::card::SearchTo::TopOfLibrary
                } else {
                    return self.unexpected("`hand`, `battlefield`, or `top`");
                };
                let that_card = self.eat_words(&["that", "card"]);
                Ok(Effect::Search {
                    filter,
                    count,
                    up_to,
                    to,
                    reveal,
                    that_card,
                })
            }
            "add" => {
                if self.eat_word("any") {
                    return Ok(Effect::AddMana(vec![Mana::Any]));
                }
                let cost = self.mana_cost()?;
                let mut out = Vec::new();
                for s in cost.symbols() {
                    out.push(match s {
                        mtg_core::mana::ManaSymbol::Colored(c) => Mana::Colored(*c),
                        mtg_core::mana::ManaSymbol::Colorless => Mana::Colorless,
                        _ => return self.unexpected("{W}, {U}, {B}, {R}, {G}, {C}, or `any`"),
                    });
                }
                if out.is_empty() {
                    return self.unexpected("mana to add");
                }
                if out.len() == 1 && self.at_word("or") {
                    while self.eat_word("or") {
                        let c = self.mana_cost()?;
                        match c.symbols() {
                            [mtg_core::mana::ManaSymbol::Colored(c)] => out.push(Mana::Colored(*c)),
                            [mtg_core::mana::ManaSymbol::Colorless] => out.push(Mana::Colorless),
                            _ => return self.unexpected("one mana symbol after `or`"),
                        }
                    }
                    return Ok(Effect::AddManaOneOf(out));
                }
                if out.len() == 1 && self.at_word("count") {
                    let count = self.num()?;
                    return Ok(Effect::AddManaTimes {
                        mana: out[0],
                        count,
                    });
                }
                Ok(Effect::AddMana(out))
            }
            "surveil" => Ok(Effect::Surveil(self.num()?)),
            "counter" => {
                let to = self.recipient()?;
                if self.eat_word("unless") {
                    self.expect_words(&["its", "controller", "pays"])?;
                    let mana = self.mana_cost()?;
                    return Ok(Effect::CounterUnlessPays { to, mana });
                }
                Ok(Effect::Counter { to })
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
        if self.at_word("count")
            && matches!(self.word_at(1).as_deref(), Some("where" | "equal" | "each"))
        {
            self.pos += 1;
            let style = match self.word()?.as_str() {
                "where" => crate::ast::CountStyle::Where,
                "equal" => crate::ast::CountStyle::Equal,
                _ => crate::ast::CountStyle::Each,
            };
            let expr = self.count_expr()?;
            let c = crate::ast::Count { expr, style };
            let i = if let Some(i) = self.counts.iter().position(|x| *x == c) {
                i
            } else {
                self.counts.push(c);
                self.counts.len() - 1
            };
            return Ok(Num::Count(u8::try_from(i).map_err(|_| ParseError {
                line: self.line(),
                message: "too many counts".to_string(),
            })?));
        }
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
            Num::X | Num::Count(_) => self.fail("X is not allowed here"),
        }
    }

    /// `domain`, `hand`, `graveyard [<filter>]`, `power`, `power of target
    /// N`, `life`, or a filter (`creature you control`, `Elf on
    /// battlefield`).
    fn count_expr(&mut self) -> R<crate::ast::CountExpr> {
        use crate::ast::CountExpr;
        if self.eat_word("domain") {
            return Ok(CountExpr::Domain);
        }
        if self.eat_word("hand") {
            return Ok(CountExpr::CardsInHand);
        }
        if self.eat_word("life") {
            return Ok(CountExpr::Life);
        }
        if self.eat_word("power") {
            if self.eat_words(&["of", "target"]) {
                let n = self.number()? as usize;
                if n == 0 || n > self.targets.len() {
                    return self.fail(format!("target {n} has not been declared"));
                }
                return Ok(CountExpr::TargetPower(n - 1));
            }
            return Ok(CountExpr::SourcePower);
        }
        if self.eat_word("graveyard") {
            let f = if self.at_stmt_end()
                || !self.word_at(0).is_some_and(|w| {
                    card_type(&w).is_some() || w.starts_with("non") || color(&w).is_some()
                }) {
                None
            } else {
                Some(self.filter()?)
            };
            return Ok(CountExpr::CardsInGraveyard(f));
        }
        let f = self.filter()?;
        self.eat_words(&["on", "battlefield"]);
        Ok(CountExpr::Permanents(f))
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
            Some(Tok::Word(w)) => {
                // "stun", "first strike": the named kinds.
                let two = self.word_at(0).map(|n| format!("{w} {n}"));
                if let Some((_, k)) = two
                    .as_deref()
                    .and_then(|t| CounterKind::NAMED.iter().find(|(n, _)| *n == t))
                {
                    self.pos += 1;
                    *k
                } else if let Some((_, k)) = CounterKind::NAMED.iter().find(|(n, _)| *n == w) {
                    *k
                } else {
                    return self.fail(format!("unknown counter kind `{w}`"));
                }
            }
            _ => return self.fail("expected a counter kind (+1/+1, -1/-1, loyalty, stun, ...)"),
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
            groups: self.groups.clone(),
            counts: self.counts.clone(),
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
        // "up to two target creatures": optional targets (CR 601.2c).
        let up_to = self.at_words(&["up", "to"])
            && (matches!(self.peek_at(2), Some(Tok::Num(_)))
                || self
                    .word_at(2)
                    .is_some_and(|w| NUMBER_WORDS.contains(&w.to_ascii_lowercase().as_str())))
            && self.word_at(3).as_deref() == Some("target");
        if up_to {
            self.pos += 2;
        }
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
        if count > 1 || up_to {
            self.expect_word("target")?;
            let mut spec = self.target_spec()?;
            if up_to {
                spec = Target::UpTo(Box::new(spec));
            }
            let mut out = Vec::new();
            for _ in 0..count {
                out.push(Recipient::Target(self.targets.len()));
                self.targets.push(spec.clone());
            }
            return Ok(out);
        }
        Ok(vec![self.recipient()?])
    }

    #[allow(clippy::too_many_lines)] // one check per recipient form
    fn recipient(&mut self) -> R<Recipient> {
        // "up to 1 target creature", "up to 2 target creatures".
        if self.at_words(&["up", "to"])
            && (matches!(self.peek_at(2), Some(Tok::Num(_)))
                || self
                    .word_at(2)
                    .is_some_and(|w| NUMBER_WORDS.contains(&w.to_ascii_lowercase().as_str())))
            && self.word_at(3).as_deref() == Some("target")
        {
            let g = self.recipient_group()?;
            return match g.as_slice() {
                [one] => Ok(*one),
                [Recipient::Target(a), ..] => Ok(Recipient::Targets(*a, g.len())),
                _ => self.fail("expected targets"),
            };
        }
        // `targets 1 to 2`: a declared group again.
        if self.at_word("targets") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            let a = self.number()? as usize;
            self.expect_word("to")?;
            let b = self.number()? as usize;
            if a == 0 || b < a || b > self.targets.len() {
                return self.fail("targets out of range");
            }
            return Ok(Recipient::Targets(a - 1, b - a + 1));
        }
        if self.eat_word("it") {
            if self.in_delayed {
                return Ok(Recipient::SameObject);
            }
            return self.fail(
                "`it` is only meaningful inside a delayed trigger; name the target (`target 1`)",
            );
        }
        if self.eat_word("attached")
            || self.at_words(&["enchanted", "creature"]) && {
                self.pos += 2;
                true
            }
            || self.at_words(&["equipped", "creature"]) && {
                self.pos += 2;
                true
            }
            || self.at_words(&["enchanted", "land"]) && {
                self.pos += 2;
                true
            }
            || self.at_words(&["enchanted", "permanent"]) && {
                self.pos += 2;
                true
            }
        {
            return Ok(Recipient::Attached);
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
        if !self.targets.is_empty() {
            let last = self.targets.len() - 1;
            if self.eat_words(&["its", "controller"]) {
                return Ok(Recipient::ControllerOf(last));
            }
            if self.eat_words(&["its", "owner"]) {
                return Ok(Recipient::OwnerOf(last));
            }
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
                    Target::Spec(TargetSpec::AttackingCreature) => Target::Filter(Filter {
                        types: vec![CardType::Creature],
                        attacking: true,
                        relation: Relation::Other,
                        ..Default::default()
                    }),
                    Target::Spec(TargetSpec::Artifact) => Target::Filter(Filter {
                        types: vec![CardType::Artifact],
                        relation: Relation::Other,
                        ..Default::default()
                    }),
                    Target::Spec(TargetSpec::CreatureYouControl) => Target::Filter(Filter {
                        types: vec![CardType::Creature],
                        who: Some(Who::You),
                        relation: Relation::Other,
                        ..Default::default()
                    }),
                    Target::Filter(mut f) if f.relation == Relation::Any => {
                        f.relation = Relation::Other;
                        Target::Filter(f)
                    }
                    Target::GraveyardCard { mut filter, who } => {
                        filter.relation = Relation::Other;
                        Target::GraveyardCard { filter, who }
                    }
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
            if self.at_words(ws) {
                // "each creature with flying" is a group, not "each creature".
                let continues = (ws[0] == "each" || ws[0] == "all")
                    && matches!(self.word_at(ws.len()).as_deref(), Some("with" | "without"));
                if !continues {
                    self.pos += ws.len();
                    return Ok(*r);
                }
            }
        }
        // "each <filter>" / "all <filter>": every matching permanent.
        if self.eat_word("each") || self.eat_word("all") {
            let f = self.filter()?;
            let i = if let Some(i) = self.groups.iter().position(|g| *g == f) {
                i
            } else {
                self.groups.push(f);
                self.groups.len() - 1
            };
            return Ok(Recipient::Group(u16::try_from(i).map_err(|_| {
                ParseError {
                    line: self.line(),
                    message: "too many groups".to_string(),
                }
            })?));
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

    /// `that targets you or a creature you control`, if there.
    fn spell_targeting(&mut self) -> R<Option<Box<crate::ast::Targeting>>> {
        if !self.eat_words(&["that", "targets"]) {
            return Ok(None);
        }
        let mut t = crate::ast::Targeting::default();
        if self.eat_word("you") {
            t.you = true;
            if !self.eat_word("or") {
                return Ok(Some(Box::new(t)));
            }
        }
        if self.eat_words(&["a", "player"]) {
            t.player = true;
            return Ok(Some(Box::new(t)));
        }
        t.filter = Some(self.filter()?);
        Ok(Some(Box::new(t)))
    }

    fn target_spec(&mut self) -> R<Target> {
        for (ws, spec) in TARGETS {
            if self.at_words_plural(ws) {
                // "creature you don't control", "creature with flying",
                // "artifact creature": a filter goes on.
                let next = self.word_at(ws.len());
                let after = self.word_at(ws.len() + 1);
                let keyword_after = matches!(next.as_deref(), Some("with" | "without")) && {
                    let mut probe = P {
                        toks: self.toks[self.pos + ws.len() + 1..].to_vec(),
                        pos: 0,
                        targets: Vec::new(),
                        groups: Vec::new(),
                        counts: Vec::new(),
                        in_delayed: false,
                        subject: None,
                    };
                    probe.keyword().is_some()
                };
                // "creature, Vehicle, or nonbasic land": a list goes on.
                let subtype_listed = matches!(self.peek_at(ws.len() + 1), Some(Tok::Word(w))
                    if w.chars().next().is_some_and(char::is_uppercase))
                    && (self.peek_at(ws.len() + 2) == Some(&Tok::Comma)
                        || self.word_at(ws.len() + 2).as_deref() == Some("or"));
                let comma_type = self.peek_at(ws.len()) == Some(&Tok::Comma)
                    && (subtype_listed
                        || self
                            .word_at(ws.len() + 1)
                            .is_some_and(|w| card_type(&w).is_some() || w == "or"));
                let more = matches!(
                    (next.as_deref(), after.as_deref()),
                    (Some("you"), Some("don't" | "control"))
                        | (Some("an"), Some("opponent"))
                        | (Some("that"), Some("was"))
                ) || comma_type
                    || (next.as_deref() == Some("or")
                        && !matches!(
                            after.as_deref(),
                            Some("ability" | "abilities" | "player" | "players")
                        ))
                    || keyword_after
                    || (matches!(next.as_deref(), Some("with"))
                        && matches!(after.as_deref(), Some("power" | "toughness" | "mana")))
                    || next.as_deref().and_then(card_type).is_some()
                    || (matches!(next.as_deref(), Some("spell" | "spells"))
                        && !ws.ends_with(&["spell"]))
                    || (matches!(next.as_deref(), Some("card" | "cards"))
                        && after.as_deref() == Some("from"));
                if more {
                    break;
                }
                self.pos += ws.len();
                // "target spell that targets a creature".
                if matches!(spec, TargetSpec::Spell) && self.at_words(&["that", "targets"]) {
                    return Ok(Target::Spell(Filter {
                        targeting: self.spell_targeting()?,
                        ..Default::default()
                    }));
                }
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
        // "card from a graveyard" / "instant card from your graveyard".
        let f = if self.at_word("card") {
            Filter::default()
        } else {
            self.filter()?
        };
        if self.eat_word("spell") || self.eat_word("spells") {
            let mut f = f;
            f.targeting = self.spell_targeting()?;
            return Ok(Target::Spell(f));
        }
        if self.at_words(&["card", "from"]) || self.at_words(&["cards", "from"]) {
            self.pos += 2;
            let who = if self.eat_word("your") {
                Who::You
            } else if self.eat_words(&["an", "opponent's"]) {
                Who::Opponents
            } else {
                self.expect_word("a")?;
                Who::Any
            };
            self.expect_word("graveyard")?;
            return Ok(Target::GraveyardCard { filter: f, who });
        }
        Ok(Target::Filter(f))
    }

    // ---- filters ----

    #[allow(clippy::too_many_lines)]
    fn filter(&mut self) -> R<Filter> {
        let mut f = Filter::default();
        let mut consumed = 0;
        let mut or_blocking = false;
        loop {
            // "nonartifact, nonblack creature"; "artifact, enchantment, or
            // land": a comma inside the filter.
            if self.peek() == Some(&Tok::Comma) && consumed > 0 {
                let next = self.word_at(1).unwrap_or_default();
                let negation = next.starts_with("non")
                    && (!f.not_types.is_empty()
                        || !f.not_colors.is_empty()
                        || !f.not_subtypes.is_empty());
                // "artifact, enchantment, or land": a list goes on to an
                // `or`; "creature, sorcery" in a cost list does not.
                let listed_type = (!f.types.is_empty() || !f.any_types.is_empty())
                    && f.subtypes.is_empty()
                    && ((card_type(&next).is_some()
                        && (self.word_at(2).as_deref() == Some("or")
                            || self.peek_at(2) == Some(&Tok::Comma)))
                        || (next == "or"
                            && self.word_at(2).as_deref().and_then(card_type).is_some()));
                if negation {
                    self.pos += 1;
                    continue;
                }
                // "Bear, Spider, or Wolf", "creature, Vehicle, or nonbasic
                // land": a list of alternatives, each an `or`.
                let alternatives = !listed_type
                    && f != Filter::default()
                    && (next == "or"
                        || (self.peek_at(2) == Some(&Tok::Comma)
                            && self.word_at(3).as_deref() == Some("or")));
                if alternatives {
                    self.pos += 1;
                    if next != "or" {
                        let before = self.pos;
                        match self.filter() {
                            Ok(mut alt) if f.or.is_none() => {
                                crate::format::share_qualifiers(&mut f, &mut alt);
                                f.or = Some(Box::new(alt));
                                consumed += self.pos - before;
                            }
                            _ => self.pos = before - 1,
                        }
                        break;
                    }
                    continue;
                }
                if listed_type {
                    self.pos += 1;
                    self.eat_word("or");
                    let t = card_type(&self.word()?).expect("checked");
                    f.any_types.append(&mut f.types);
                    f.any_types.push(t);
                    consumed += 2;
                    continue;
                }
                break;
            }
            let Some(w) = self.word_at(0) else {
                break;
            };
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
                "all" => {
                    f.all = true;
                    self.pos += 1;
                }
                "a" | "an" | "each" | "permanent" | "permanents" => self.pos += 1,
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
                "basic" | "legendary" | "snow" => {
                    f.supertypes.push(supertype(&w).expect("a supertype word"));
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
                "your" if self.at_words(&["your", "opponents", "control"]) => {
                    f.who = Some(Who::Opponents);
                    f.your_opponents = true;
                    self.pos += 3;
                }
                "you" if self.at_words(&["you", "don't", "control"]) => {
                    f.who = Some(Who::Opponents);
                    f.dont_control = true;
                    self.pos += 3;
                }
                "with" if self.at_words(&["with", "no", "abilities"]) => {
                    f.no_abilities = true;
                    self.pos += 3;
                }
                "with"
                    if matches!(self.word_at(1).as_deref(), Some("power" | "toughness"))
                        || self.at_words(&["with", "mana", "value"]) =>
                {
                    self.pos += 1;
                    let what = self.word()?;
                    if what == "mana" {
                        self.expect_word("value")?;
                    }
                    let n = self.number()?;
                    let cmp = if self.eat_words(&["or", "less"]) {
                        Cmp::AtMost
                    } else if self.eat_words(&["or", "greater"]) {
                        Cmp::AtLeast
                    } else {
                        Cmp::Exactly
                    };
                    match what.as_str() {
                        "power" => f.power = Some((cmp, i16::try_from(n).unwrap_or(i16::MAX))),
                        "toughness" => {
                            f.toughness = Some((cmp, i16::try_from(n).unwrap_or(i16::MAX)));
                        }
                        _ => f.mana_value = Some((cmp, u8::try_from(n).unwrap_or(u8::MAX))),
                    }
                }
                "tapped" | "untapped" => {
                    f.tapped = Some(w == "tapped");
                    self.pos += 1;
                }
                "attacking" if self.at_words(&["attacking", "or", "blocking"]) => {
                    f.attacking = true;
                    or_blocking = true;
                    self.pos += 3;
                }
                "attacking" => {
                    f.attacking = true;
                    self.pos += 1;
                }
                "blocking" => {
                    f.blocking = true;
                    self.pos += 1;
                }
                "damaged" => {
                    f.damaged = true;
                    self.pos += 1;
                }
                "that" if self.at_words(&["that", "was", "dealt", "damage", "this", "turn"]) => {
                    f.damaged = true;
                    self.pos += 6;
                }
                "multicolored" | "monocolored" | "colorless" => {
                    f.color_count = Some(match w.as_str() {
                        "multicolored" => ColorCount::Multicolored,
                        "monocolored" => ColorCount::Monocolored,
                        _ => ColorCount::Colorless,
                    });
                    self.pos += 1;
                }
                "or" if self.word_at(1).as_deref().and_then(color).is_some()
                    && f.colors.is_some() =>
                {
                    self.pos += 1;
                }
                "or" if self.word_at(1).as_deref().and_then(card_type).is_some()
                    && (!f.types.is_empty() || !f.any_types.is_empty()) =>
                {
                    // "artifact or enchantment": the types so far and the
                    // next become alternatives.
                    self.pos += 1;
                    let t = card_type(&self.word()?).expect("checked");
                    f.any_types.append(&mut f.types);
                    f.any_types.push(t);
                }
                // "creature or Vehicle", "land or nonblack creature": an
                // alternative filter, if one follows.
                "or" if f != Filter::default()
                    && !matches!(
                        self.word_at(1).as_deref(),
                        Some("ability" | "abilities" | "player" | "players" | "less" | "greater")
                    ) =>
                {
                    let save = self.pos;
                    self.pos += 1;
                    match self.filter() {
                        Ok(mut alt) if f.or.is_none() => {
                            crate::format::share_qualifiers(&mut f, &mut alt);
                            f.or = Some(Box::new(alt));
                            consumed += self.pos - before;
                        }
                        _ => self.pos = save,
                    }
                    break;
                }
                "without" if self.word_at(1).as_deref() != Some("regeneration") => {
                    self.pos += 1;
                    let Some(k) = self.keyword() else {
                        return self.unexpected("a keyword after `without`");
                    };
                    f.not_keywords.push(k);
                }
                "with" => {
                    self.pos += 1;
                    let Some(k) = self.keyword() else {
                        return self.unexpected("a keyword after `with`");
                    };
                    f.keywords.push(k);
                    while self.eat_word("and") {
                        let Some(k) = self.keyword() else {
                            return self.unexpected("a keyword after `and`");
                        };
                        f.keywords.push(k);
                    }
                    // "creatures with flying or reach": the same filter
                    // with the other keyword, as an alternative.
                    if self.at_word("or") && f.or.is_none() {
                        let save = self.pos;
                        self.pos += 1;
                        if let Some(k2) = self.keyword() {
                            let mut alt = f.clone();
                            alt.keywords = vec![k2];
                            f.or = Some(Box::new(alt));
                        } else {
                            self.pos = save;
                        }
                    }
                }
                _ => {
                    if let Some(rest) = w.strip_prefix("non") {
                        let rest_raw = &raw[3..];
                        let rest_raw = rest_raw.strip_prefix('-').unwrap_or(rest_raw);
                        if let Some(t) = card_type(rest) {
                            f.not_types.push(t);
                        } else if let Some(c) = color(rest) {
                            f.not_colors.push(c);
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
                        // Plural subtypes are singular in a filter; a few
                        // end in "s" in the singular (Plains, Elves are
                        // "Elf" though).
                        let keep = matches!(raw.as_str(), "Plains" | "Locus" | "Homunculus");
                        f.subtypes.push(if keep {
                            raw.clone()
                        } else {
                            raw.trim_end_matches('s').to_string()
                        });
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
        if or_blocking && f.or.is_none() {
            let mut alt = f.clone();
            alt.attacking = false;
            alt.blocking = true;
            f.or = Some(Box::new(alt));
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
            if self.eat_word("every") {
                let w = self.word()?;
                let Some(step) = step_by_name(&w) else {
                    return self.fail(format!("unknown step `{w}`"));
                };
                self.eat_word("step");
                return Ok(Trigger::EveryStep(step));
            }
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
            // "the next turn's upkeep".
            self.eat_word("turn's");
            let w = self.word()?;
            let Some(step) = step_by_name(&w) else {
                return self.fail(format!("unknown step `{w}`"));
            };
            self.eat_word("step");
            return Ok(Trigger::BeginningOfStep { step, whose });
        }
        let whenever = self.at_word("whenever");
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
        if self.at_words(&["you", "cast"])
            || self.at_words(&["an", "opponent", "casts"])
            || self.at_words(&["a", "player", "casts"])
        {
            let whose = if self.at_word("you") {
                Who::You
            } else if self.at_word("an") {
                Who::Opponents
            } else {
                Who::Any
            };
            self.pos += if whose == Who::You { 2 } else { 3 };
            self.eat_word("a");
            self.eat_word("an");
            let simple = self.at_word("spell")
                || self.at_words(&["noncreature", "spell"])
                || self.at_words(&["instant", "or", "sorcery", "spell"]);
            if !simple || whose == Who::Any {
                let filter = self.filter()?;
                self.expect_word("spell")?;
                return Ok(Trigger::CastsFiltered { whose, filter });
            }
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
            "exploits" if self.at_words(&["a", "creature"]) => {
                self.pos += 2;
                if obj != TriggerObj::This {
                    return self.fail("only `this` exploits");
                }
                Ok(Trigger::Exploits)
            }
            "is" if self.at_words(&["turned", "face", "up"]) => {
                self.pos += 3;
                if obj != TriggerObj::This {
                    return self.fail("only `this` is turned face up");
                }
                Ok(Trigger::TurnedFaceUp)
            }
            "is" if self.at_words(&["dealt", "damage"]) => {
                self.pos += 2;
                Ok(Trigger::DealtDamage(obj))
            }
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
            "attacks" => {
                if self.eat_word("alone") {
                    return Ok(Trigger::AttacksAlone(obj));
                }
                Ok(Trigger::Attacks(obj))
            }
            "blocks" if self.at_words(&["or", "becomes", "blocked"]) => {
                self.pos += 3;
                Ok(Trigger::BlocksOrBlocked(obj))
            }
            "blocks" => Ok(Trigger::Blocks(obj)),
            "deals" => {
                if self.at_words(&["combat", "damage", "to", "a", "player"]) {
                    self.pos += 5;
                    return Ok(Trigger::DealsCombatDamageToPlayer(obj));
                }
                let combat = self.eat_word("combat");
                self.expect_word("damage")?;
                let to = if self.eat_word("to") {
                    self.eat_word("a");
                    self.eat_word("an");
                    match self.word()?.as_str() {
                        "creature" => mtg_core::card::DamageTo::Creature,
                        "player" => mtg_core::card::DamageTo::Player,
                        "opponent" => mtg_core::card::DamageTo::Opponent,
                        w => return self.fail(format!("unknown damage recipient `{w}`")),
                    }
                } else {
                    mtg_core::card::DamageTo::Any
                };
                Ok(Trigger::DealsDamage { obj, to, combat })
            }
            "becomes" if self.at_word("tapped") => {
                self.pos += 1;
                Ok(Trigger::BecomesTapped(obj))
            }
            "becomes" if self.at_word("untapped") => {
                self.pos += 1;
                Ok(Trigger::BecomesUntapped(obj))
            }
            "becomes" if self.at_word("blocked") => {
                self.pos += 1;
                Ok(Trigger::BecomesBlocked(obj))
            }
            "becomes" => {
                if !(self.eat_word("targeted")
                    || self.eat_words(&["the", "target", "of", "a", "spell", "or", "ability"]))
                {
                    return self.unexpected("`targeted`");
                }
                if obj == TriggerObj::This && self.eat_words(&["by", "opponent"]) {
                    return Ok(Trigger::TargetedByOpponent);
                }
                if whenever {
                    return Ok(Trigger::BecomesTargetWhenever(obj));
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
        if self.eat_word("kicked") {
            return Ok(Condition::Kicked);
        }
        if self.eat_words(&["echo", "due"]) {
            return Ok(Condition::EchoDue);
        }
        if self.eat_word("evoked") {
            return Ok(Condition::Evoked);
        }
        if self.eat_words(&["no", "spells", "were", "cast", "last", "turn"]) {
            return Ok(Condition::NoSpellsLastTurn);
        }
        if self.eat_words(&[
            "a", "player", "cast", "two", "or", "more", "spells", "last", "turn",
        ]) {
            return Ok(Condition::TwoSpellsLastTurn);
        }
        if self.eat_words(&["it", "had", "no"]) {
            let k = self.counter_kind()?;
            self.eat_word("counters");
            return Ok(Condition::HadNoCounters(k));
        }
        self.unexpected("a condition")
    }

    #[allow(clippy::too_many_lines)] // one arm per cost part
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
                    "remove"
                        if (1..5).any(|k| {
                            self.word_at(k).as_deref() == Some("from")
                                && self.word_at(k + 1).as_deref() == Some("this")
                        }) =>
                    {
                        self.pos += 1;
                        let n = self.count()?;
                        let k = self.counter_kind()?;
                        self.expect_words(&["from", "this"])?;
                        cost.remove_counters = Some((k, n));
                        cost.order.push(CostItem::RemoveCounters);
                    }
                    "return" | "tap" if !matches!(self.peek_at(1), Some(Tok::Sym(_))) => {
                        cost.chosen = if w.eq_ignore_ascii_case("return") {
                            mtg_core::card::ChosenCost::Return
                        } else {
                            mtg_core::card::ChosenCost::Tap
                        };
                        self.pos += 1;
                        cost.sacrifice = Some(SacCost::Filter(self.filter()?));
                        cost.order.push(CostItem::Sacrifice);
                    }
                    "from" if self.at_words(&["from", "graveyard"]) => {
                        self.pos += 2;
                        cost.from_graveyard = true;
                        cost.order.push(CostItem::FromGraveyard);
                    }
                    "exile" if self.at_words(&["exile", "this", "from", "graveyard"]) => {
                        self.pos += 4;
                        cost.exile_self = true;
                        cost.order.push(CostItem::Exile);
                    }
                    "discard" => {
                        self.pos += 1;
                        if self.eat_word("this") {
                            self.eat_word("card");
                            cost.discard_self = true;
                        } else if matches!(self.peek(), Some(Tok::Num(_))) {
                            cost.discard = self.count()?;
                            self.cards_word()?;
                        } else {
                            // "discard a creature card": one card matching
                            // a filter.
                            let f = self.filter()?;
                            self.cards_word()?;
                            cost.discard = 1;
                            cost.discard_filter = Some(f);
                        }
                        cost.order.push(CostItem::Discard);
                    }
                    "once" => {
                        self.pos += 1;
                        self.eat_words(&["per", "turn"]);
                        self.eat_words(&["each", "turn"]);
                        cost.once = true;
                    }
                    "pay" => {
                        self.pos += 1;
                        let n = self.count()?;
                        if self.eat_word("energy") {
                            cost.energy = n;
                            cost.order.push(CostItem::Energy);
                        } else {
                            cost.life = n;
                            self.expect_word("life")?;
                            cost.order.push(CostItem::Life);
                        }
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

    #[allow(clippy::too_many_lines)] // one arm per event
    fn replaces(&mut self) -> R<Replaces> {
        if self.eat_word("enters") {
            return Ok(Replaces::Enters);
        }
        // `target 1 dies`.
        if self.at_word("target")
            && matches!(self.peek_at(1), Some(Tok::Num(_)))
            && self.word_at(2).as_deref() == Some("dies")
        {
            self.pos += 1;
            let Some(Tok::Num(n)) = self.next() else {
                unreachable!("checked")
            };
            self.pos += 1;
            let n = n as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(Replaces::Dies {
                to: Affects::Target(n - 1),
            });
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
            if self.at_word("count") {
                let count = self.num()?;
                return Ok(Mod::ModifyPtPer {
                    power: p,
                    toughness: t,
                    count,
                });
            }
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
            "has" | "have" if self.at_word("ability") => {
                self.pos += 1;
                Ok(Mod::AddAbility(Box::new(self.nested_ability()?)))
            }
            "has" | "have" => Ok(Mod::AddKeywords(self.keyword_list()?)),
            "loses" | "lose" if self.keyword_ahead() => {
                Ok(Mod::LosesKeywords(self.keyword_list()?))
            }
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
