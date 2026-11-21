//! Parser for card files: `card "Name" { ... }` blocks of one statement per
//! line. See `SYNTAX.md` in this crate.

use mtg_core::card::{
    CardType, Cmp, ColorCount, Condition, CounterKind, Keyword, Num, PtValue, RedirectTo, Relation,
    Replacement as Outcome, RuleKind, Supertype, TargetSpec, Who,
};
use mtg_core::mana::{Color, Mana, ManaCost};
use mtg_core::state::Zone;
use mtg_core::turn::Step;
use thiserror::Error;

use crate::ast::{
    Ability, Affects, AttachedRef, BlockRule, Card, ChoiceKind, Cost, CostItem, DamageWording,
    DieRow, Effect, Except, Filter, InsteadCond, InsteadWording, Join, Kind, LibraryPlace,
    LibraryWording, Mechanic, Mod, Mode, OfChosen, Recipient, Replacement, Replaces, Restrict,
    SacCost, SameRef, Sentence, StaticCond, Target, Trigger, TriggerObj,
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
        delayed_that: false,
        delayed_dies: false,
        in_trigger: false,
        it_source: false,
        in_spell: false,
        group_named: false,
        subject: None,
        votes: Vec::new(),
        returned: None,
        revealed: false,
        card_filter: false,
        kicked_targets: None,
        kicked_instead: None,
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
        &[
            "can", "attack", "as", "though", "it", "didn't", "have", "defender",
        ],
        Keyword::AttackDespiteDefender,
    ),
    (
        &["attacks", "each", "combat", "if", "able"],
        Keyword::MustAttack,
    ),
    (
        &["blocks", "each", "combat", "if", "able"],
        Keyword::MustBlock,
    ),
    (
        &["must", "be", "blocked", "if", "able"],
        Keyword::MustBeBlocked,
    ),
    (
        &[
            "can",
            "block",
            "an",
            "additional",
            "creature",
            "each",
            "combat",
        ],
        Keyword::BlockAdditional,
    ),
    (
        &["can", "block", "any", "number", "of", "creatures"],
        Keyword::BlockAny,
    ),
    (&["lure"], Keyword::Lure),
    (
        &["can't", "activate", "except", "mana"],
        Keyword::CantActivateNonMana,
    ),
    (&["can't", "activate"], Keyword::CantActivate),
    (&["can't", "crew", "Vehicles"], Keyword::CantCrew),
    (
        &["can't", "become", "untapped"],
        Keyword::CantBecomeUntapped,
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
    (&["banding"], Keyword::Banding),
    (&["decayed"], Keyword::Decayed),
    (&["phasing"], Keyword::Phasing),
    (&["convoke"], Keyword::Convoke),
    (&["improvise"], Keyword::Improvise),
    (&["cascade"], Keyword::Cascade),
    (&["rebound"], Keyword::Rebound),
    (&["cipher"], Keyword::Cipher),
    (&["paradigm"], Keyword::Paradigm),
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
    (
        &[
            "activated",
            "or",
            "triggered",
            "ability",
            "you",
            "control",
            "from",
            "a",
            "creature",
            "source",
        ],
        TargetSpec::OnStack {
            spells: false,
            activated: true,
            triggered: true,
            source: Some(CardType::Creature),
            yours: true,
        },
    ),
    (
        &[
            "activated",
            "or",
            "triggered",
            "ability",
            "you",
            "control",
            "from",
            "an",
            "artifact",
            "source",
        ],
        TargetSpec::OnStack {
            spells: false,
            activated: true,
            triggered: true,
            source: Some(CardType::Artifact),
            yours: true,
        },
    ),
    (
        &["activated", "or", "triggered", "ability", "you", "control"],
        TargetSpec::OnStack {
            spells: false,
            activated: true,
            triggered: true,
            source: None,
            yours: true,
        },
    ),
    (
        &["triggered", "ability", "you", "control"],
        TargetSpec::OnStack {
            spells: false,
            activated: false,
            triggered: true,
            source: None,
            yours: true,
        },
    ),
    (
        &["spell", "or", "ability"],
        TargetSpec::OnStack {
            spells: true,
            activated: true,
            triggered: true,
            source: None,
            yours: false,
        },
    ),
    (
        &["activated", "or", "triggered", "ability"],
        TargetSpec::OnStack {
            spells: false,
            activated: true,
            triggered: true,
            source: None,
            yours: false,
        },
    ),
    (
        &["activated", "ability", "from", "an", "artifact", "source"],
        TargetSpec::OnStack {
            spells: false,
            activated: true,
            triggered: false,
            source: Some(CardType::Artifact),
            yours: false,
        },
    ),
    (
        &["activated", "ability"],
        TargetSpec::OnStack {
            spells: false,
            activated: true,
            triggered: false,
            source: None,
            yours: false,
        },
    ),
    (
        &["triggered", "ability"],
        TargetSpec::OnStack {
            spells: false,
            activated: false,
            triggered: true,
            source: None,
            yours: false,
        },
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
    (&["player", "or", "battle"], TargetSpec::PlayerOrBattle),
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
    (&["each", "other", "player"], Recipient::EachOtherPlayer),
    (&["each", "player"], Recipient::EachPlayer),
    (&["each", "creature"], Recipient::EachCreature),
    (&["all", "creatures"], Recipient::EachCreature),
    (&["that", "creature"], Recipient::TriggeringObject),
    (&["that", "permanent"], Recipient::TriggeringObject),
    (&["that", "Vehicle"], Recipient::TriggeringObject),
    (&["that", "object"], Recipient::TriggeringObject),
    (&["that", "spell"], Recipient::TriggeringObject),
    (&["that", "player"], Recipient::TriggeringPlayer),
    (
        &["that", "defending", "player"],
        Recipient::DefendingPlayerWord,
    ),
    (&["defending", "player"], Recipient::DefendingPlayer),
    (&["that", "token"], Recipient::CreatedObject),
    (&["manifested", "creature"], Recipient::Manifested),
    // "attach this Equipment to the amassed Army": amass's Army (CR
    // 701.47a), created or not.
    (&["amassed", "Army"], Recipient::CreatedObject),
    (&["you"], Recipient::You),
    (&["this"], Recipient::Source),
];

/// The nouns a card uses for itself (`this creature`; see
/// `render::self_noun`).
pub(crate) const SELF_NOUNS: [&str; 10] = [
    "creature",
    "land",
    "artifact",
    "enchantment",
    "planeswalker",
    "permanent",
    "token",
    "aura",
    "equipment",
    "vehicle",
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
        "first" | "precombat" => Step::PrecombatMain,
        "second" | "postcombat" => Step::PostcombatMain,
        _ => return None,
    })
}

#[allow(clippy::struct_excessive_bools)] // independent context flags
struct P {
    toks: Vec<Spanned>,
    pos: usize,
    /// The choices of the vote in the ability being parsed, for `votes for
    /// <choice>` counts.
    votes: Vec<String>,
    /// Targets declared so far in the ability being parsed.
    targets: Vec<Target>,
    /// Group recipients declared so far in the card being parsed.
    groups: Vec<Filter>,
    /// Variable amounts declared so far in the card being parsed.
    counts: Vec<crate::ast::Count>,
    /// Inside a delayed trigger's effects, where "it" is the object the
    /// trigger is about.
    in_delayed: bool,
    /// Inside a delayed trigger about the triggering object: "that
    /// creature" is the object it is about (`it`).
    delayed_that: bool,
    /// Inside "When that creature dies (leaves the battlefield) this
    /// turn": "its controller" is the controller that object had.
    delayed_dies: bool,
    /// Inside a triggered ability: "that creature" is the triggering
    /// object there, the last target elsewhere.
    in_trigger: bool,
    /// Inside a trigger on casting "a spell that targets this creature"
    /// (heroic): `it` is the source, as Oracle has it ("put a +1/+1
    /// counter on it").
    it_source: bool,
    /// Inside an instant or sorcery's own effects (`spell:`, or the modes
    /// of a modal spell): `it` after a target is that target, since the
    /// spell itself can only be the "it" that deals damage.
    in_spell: bool,
    /// A group (`each <filter>`, `creatures you control`) has been named
    /// in this ability, for `them` and `those creatures` after it.
    group_named: bool,
    /// The subject of the previous clause of the sentence being parsed.
    subject: Option<Recipient>,
    /// The target an earlier effect of the ability returned to its
    /// owner's hand: "that player" is then its owner.
    returned: Option<usize>,
    /// A card revealed or looked at earlier in the ability (`reveal top
    /// card`, `reveal until`): `it` / `that card` is that card in `if it
    /// is`, `put it ...`.
    revealed: bool,
    /// Parsing the kind of card a dig takes: "permanent with mana value 5
    /// or less" is a permanent card (CR 110.4a), its qualities before the
    /// noun.
    card_filter: bool,
    /// In a spell, how many targets were declared before an "if kicked"
    /// sentence declared more (CR 601.2c: those are chosen only if the
    /// spell is kicked).
    kicked_targets: Option<usize>,
    /// Likewise for an "if kicked, instead ..." with its own targets: how
    /// many targets came before it (`Card::kicked_drops`).
    kicked_instead: Option<usize>,
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

    /// `color [except blue]`, `creature type [except Wall]`, `land type`;
    /// with `of_choice`, each followed by `of choice` (before `except`).
    fn choice_kind(&mut self, of_choice: bool) -> R<ChoiceKind> {
        let suffix = |p: &mut Self| -> R<()> {
            if of_choice {
                p.expect_words(&["of", "choice"])?;
            }
            Ok(())
        };
        if self.eat_word("colors") {
            suffix(self)?;
            return Ok(ChoiceKind::Colors);
        }
        if self.eat_word("color") {
            suffix(self)?;
            let except = if self.eat_word("except") {
                let Some(c) = self.word_at(0).as_deref().and_then(color) else {
                    return self.fail("expected a color after `except`");
                };
                self.pos += 1;
                Some(c)
            } else {
                None
            };
            return Ok(ChoiceKind::Color { except });
        }
        if self.eat_words(&["creature", "type"]) {
            suffix(self)?;
            let except = if self.eat_word("except") {
                Some(self.word()?)
            } else {
                None
            };
            return Ok(ChoiceKind::CreatureType { except });
        }
        if self.eat_words(&["land", "type"]) {
            suffix(self)?;
            return Ok(ChoiceKind::LandType(Color::ALL.to_vec()));
        }
        self.unexpected("`color`, `creature type` or `land type`")
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
            defense: None,
            keywords: Vec::new(),
            additional: None,
            additional_or: None,
            optional_cost: None,
            alt_cost: None,
            flash_if: None,
            flash_if_may: false,
            flash_cleanup: false,
            pronoun: None,
            short_name: None,
            unkicked_targets: None,
            kicked_drops: None,
            cast_only: None,
            kicker: None,
            kicker_targets: None,
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
                    "prepare" => mtg_core::card::Layout::Prepare,
                    "modal_dfc" => mtg_core::card::Layout::ModalDfc,
                    w => {
                        return self.fail(format!(
                            "unknown layout `{w}` (adventure, omen, transform, split, prepare, modal_dfc)"
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
            "loyalty"
                if matches!(self.peek_at(1), Some(Tok::Num(_)))
                    && self.peek_at(2) != Some(&Tok::Colon) =>
            {
                self.pos += 1;
                card.loyalty = Some(self.number()?);
                self.end_stmt()
            }
            "defense" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                card.defense = Some(self.number()?);
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
            "additional" if !self.at_words(&["additional", "land"]) => {
                self.pos += 1;
                self.expect_word("cost")?;
                // `additional cost or {3}`: the other of two (CR 601.2b).
                if self.eat_word("or") {
                    if card.additional.is_none() {
                        return self.fail("`additional cost or` follows an `additional cost`");
                    }
                    card.additional_or = Some(self.cost()?);
                } else if !self.at_words(&["may", "behold"]) && self.eat_word("may") {
                    // `additional cost may collect evidence 6` (CR 601.2b).
                    let c = self.cost()?;
                    if !c.mana.is_empty() || c.blight == 0 && c.evidence == 0 {
                        return self
                            .fail("an optional additional cost is blight or collect evidence");
                    }
                    card.optional_cost = Some(c);
                } else {
                    card.additional = Some(self.cost()?);
                }
                self.end_stmt()
            }
            "alternative" => {
                self.pos += 1;
                self.expect_word("cost")?;
                // `sacrifice 2 Mountain`: the count, then a cost of one.
                let mut count = 1;
                // `each other player gains 6 life`: a cost part of its own,
                // before any other.
                let mut others_gain = 0;
                if self.eat_words(&["each", "other", "player", "gains"]) {
                    others_gain = self.number()?;
                    self.expect_word("life")?;
                    if self.peek() == Some(&Tok::Comma) {
                        self.pos += 1;
                    } else {
                        card.alt_cost = Some(crate::ast::AltCost {
                            cost: crate::ast::Cost::default(),
                            count,
                            others_gain,
                        });
                        return self.end_stmt();
                    }
                }
                if self.at_word("sacrifice")
                    && let Some(Tok::Num(n)) = self.peek_at(1)
                {
                    count = *n;
                    self.toks.remove(self.pos + 1);
                }
                let cost = self.cost()?;
                if cost.sacrifice.is_none() && count > 1 {
                    return self.fail("a count needs a permanent to sacrifice");
                }
                card.alt_cost = Some(crate::ast::AltCost {
                    cost,
                    count,
                    others_gain,
                });
                self.end_stmt()
            }
            "kicker" => {
                self.pos += 1;
                card.kicker = Some(self.mana_cost()?);
                self.end_stmt()
            }
            "short_name" => {
                self.pos += 1;
                card.short_name = Some(self.string()?);
                self.end_stmt()
            }
            "pronoun" => {
                self.pos += 1;
                card.pronoun = Some(match self.word()?.as_str() {
                    "he" => crate::ast::Pronoun::He,
                    "she" => crate::ast::Pronoun::She,
                    w => return self.fail(format!("unknown pronoun `{w}` (he, she)")),
                });
                self.end_stmt()
            }
            "flash" if self.at_words(&["flash", "with", "cleanup", "sacrifice"]) => {
                self.pos += 4;
                card.flash_cleanup = true;
                self.end_stmt()
            }
            "flash" if self.at_words(&["flash", "if"]) => {
                self.pos += 2;
                card.flash_if = Some(self.static_cond()?);
                card.flash_if_may = true;
                self.end_stmt()
            }
            "flash" if self.at_words(&["flash", "as", "long", "as"]) => {
                self.pos += 4;
                card.flash_if = Some(self.static_cond()?);
                self.end_stmt()
            }
            "cast" if self.at_words(&["cast", "only"]) => {
                self.pos += 2;
                self.eat_word("if");
                card.cast_only = Some(self.static_cond()?);
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
                        // "it isn't legendary" (CR 707.9b).
                        Some("not") if self.word_at(1).as_deref() == Some("legendary") => {
                            self.pos += 2;
                            except.not_legendary = true;
                            self.end_stmt()?;
                        }
                        // "it's an artifact in addition to its other types".
                        Some("type" | "types") => {
                            self.pos += 1;
                            while let Some(t) = self.word_at(0).as_deref().and_then(card_type) {
                                except.types.push(t);
                                self.pos += 1;
                                self.eat_word("and");
                            }
                            if except.types.is_empty() {
                                return self.unexpected("a card type");
                            }
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
                if matches!(a, Ability::Spell { .. }) {
                    card.unkicked_targets = self.kicked_targets.take();
                    card.kicked_drops = self.kicked_instead.take();
                    if card.kicked_drops.is_some() {
                        card.unkicked_targets = card.kicked_drops;
                    }
                }
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
            // `, ` and `and` continue the list only when a keyword follows
            // (`has can't be blocked, has shroud` is two modifications).
            let at = self.pos;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            self.eat_word("and");
            if !self.keyword_ahead() {
                self.pos = at;
                break;
            }
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

    /// `only to cast instant or sorcery spell`, `only to cast artifact
    /// spell or [to] activate`, `only to activate [or cast artifact
    /// spell]`, `not to cast nonartifact spell` (CR 106.6).
    fn mana_restriction(&mut self) -> R<crate::ast::ManaRestriction> {
        use mtg_core::card::SpendWording;
        if self.eat_word("not") {
            self.expect_words(&["to", "cast"])?;
            let f = self.filter()?;
            self.expect_word("spell")?;
            return Ok(crate::ast::ManaRestriction {
                spells: Some(f),
                abilities: true,
                ability_source: None,
                foretell: false,
                disturb: false,
                wording: SpendWording::CantCast,
            });
        }
        self.expect_words(&["only", "to"])?;
        if self.eat_word("activate") {
            let ability_source = self.activate_of()?;
            let spells = if self.eat_words(&["or", "cast"]) {
                let f = self.filter()?;
                self.expect_word("spell")?;
                Some(f)
            } else {
                None
            };
            let wording = if spells.is_some() {
                SpendWording::ActivateFirst
            } else {
                SpendWording::CastFirst
            };
            return Ok(crate::ast::ManaRestriction {
                spells,
                abilities: true,
                ability_source,
                foretell: false,
                disturb: false,
                wording,
            });
        }
        // "foretell a card from your hand or cast an instant or sorcery
        // spell" (CR 702.143a).
        let foretell = self.eat_words(&["foretell", "or"]);
        let disturb = self.eat_words(&["pay", "disturb", "or"]);
        self.expect_word("cast")?;
        let mut f = self.filter()?;
        self.expect_word("spell")?;
        // "an Elemental spell or a Chandra planeswalker spell": each side
        // whole.
        if self.at_word("or") && !matches!(self.word_at(1).as_deref(), Some("to" | "activate")) {
            self.pos += 1;
            let alt = self.filter()?;
            self.expect_word("spell")?;
            f.or = Some(Box::new(alt));
            // Each side with its own article ("a Dragon spell or an Omen
            // spell"), against `Lesson or Shrine spell` ("a Lesson or
            // Shrine spell").
            f.alt_article = true;
        }
        let (abilities, wording) = if self.eat_words(&["or", "to", "activate"]) {
            (true, SpendWording::CastFirstTo)
        } else if self.eat_words(&["or", "activate"]) {
            (true, SpendWording::CastFirst)
        } else {
            (false, SpendWording::CastFirst)
        };
        let ability_source = if abilities { self.activate_of()? } else { None };
        Ok(crate::ast::ManaRestriction {
            spells: Some(f),
            abilities,
            ability_source,
            foretell,
            disturb,
            wording,
        })
    }

    /// After `activate`: `of artifact` ("an ability of an artifact
    /// source"), or nothing for any source.
    fn activate_of(&mut self) -> R<Option<Filter>> {
        if self.eat_word("of") {
            Ok(Some(self.filter()?))
        } else {
            Ok(None)
        }
    }

    /// The rest of a player-level static ability: an optional `as long as
    /// <condition>`.
    fn player_static(&mut self, kind: mtg_core::card::PlayerStaticKind) -> R<Ability> {
        let cond = if self.eat_words(&["as", "long", "as"]) {
            Some(self.static_cond()?)
        } else {
            None
        };
        self.end_stmt()?;
        Ok(Ability::PlayerStatic { kind, cond })
    }

    #[allow(clippy::too_many_lines)]
    fn ability(&mut self) -> R<Ability> {
        self.targets.clear();
        self.kicked_targets = None;
        self.kicked_instead = None;
        self.votes.clear();
        self.subject = None;
        self.revealed = false;
        self.in_trigger = false;
        self.it_source = false;
        self.in_spell = false;
        self.group_named = false;
        self.returned = None;
        let w = self.word_at(0).unwrap_or_default();
        match w.as_str() {
            "spend" if self.word_at(1).as_deref() == Some("only") => {
                self.pos += 2;
                let w = self.word()?;
                let Some(c) = color(&w) else {
                    return self.fail(format!("expected a color, found `{w}`"));
                };
                self.expect_words(&["mana", "on", "x"])?;
                self.end_stmt()?;
                Ok(Ability::XManaColor(c))
            }
            "spells" => self.cost_change(None),
            "hexproof" if self.word_at(1).as_deref() == Some("from") => {
                self.pos += 2;
                let filter = self.filter()?;
                self.end_stmt()?;
                Ok(Ability::HexproofFrom(filter))
            }
            "opening" if self.at_words(&["opening", "hand"]) => {
                self.pos += 2;
                self.end_stmt()?;
                Ok(Ability::OpeningHand)
            }
            "you" if self.at_words(&["you", "have", "hexproof"]) => {
                self.pos += 3;
                self.player_static(mtg_core::card::PlayerStaticKind::Hexproof)
            }
            "you" if self.at_words(&["you", "have", "shroud"]) => {
                self.pos += 3;
                self.player_static(mtg_core::card::PlayerStaticKind::Shroud)
            }
            "opponents"
                if self.at_words(&[
                    "opponents",
                    "can't",
                    "cast",
                    "spells",
                    "during",
                    "your",
                    "turn",
                ]) =>
            {
                self.pos += 7;
                self.player_static(mtg_core::card::PlayerStaticKind::OpponentsCantCastYourTurn)
            }
            "players" if self.at_words(&["players", "can't", "gain", "life"]) => {
                self.pos += 4;
                self.player_static(mtg_core::card::PlayerStaticKind::Rule(
                    mtg_core::card::RuleKind::NoLifeGain,
                ))
            }
            "damage" if self.at_words(&["damage", "can't", "be", "prevented"]) => {
                self.pos += 4;
                self.player_static(mtg_core::card::PlayerStaticKind::Rule(
                    mtg_core::card::RuleKind::DamageCantBePrevented,
                ))
            }
            "additional" if self.at_words(&["additional", "land"]) => {
                self.pos += 2;
                let n = if matches!(self.peek(), Some(Tok::Num(_))) {
                    self.number()?
                } else {
                    1
                };
                let each_player = self.eat_words(&["each", "player"]);
                self.player_static(mtg_core::card::PlayerStaticKind::ExtraLands { n, each_player })
            }
            // CR 716.2a: a Class's level ability.
            "level" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let n = self.number()?;
                let Some(level) = u8::try_from(n).ok().filter(|n| (2..=9).contains(n)) else {
                    return self.fail("a Class level is 2 to 9");
                };
                let cost = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::ClassLevel { level, cost })
            }
            // CR 719.2: a Case's "To solve" and "Solved".
            "to" if self.at_words(&["to", "solve"]) => {
                self.pos += 2;
                let cond = self.static_cond()?;
                self.end_stmt()?;
                Ok(Ability::ToSolve(cond))
            }
            "solved" => {
                self.pos += 1;
                let inner = self.ability()?;
                Ok(Ability::Solved(Box::new(inner)))
            }
            "this" if self.at_words(&["this", "spell", "can't", "be", "countered"]) => {
                self.pos += 5;
                self.end_stmt()?;
                Ok(Ability::Uncounterable)
            }
            "this" if self.at_words(&["this", "spell", "costs"]) => {
                self.pos += 3;
                let per = self.mana_cost()?.mana_value();
                if self.at_words(&["less", "if", "it", "targets"])
                    || self.at_words(&["more", "if", "it", "targets"])
                {
                    let less = self.at_word("less");
                    self.pos += 4;
                    let noun = if self.eat_word("spell") {
                        crate::ast::TargetNoun::Spell
                    } else if self.eat_word("card") {
                        crate::ast::TargetNoun::Card
                    } else {
                        crate::ast::TargetNoun::Permanent
                    };
                    let filter = self.filter()?;
                    self.end_stmt()?;
                    let n = i32::try_from(per).unwrap_or(0);
                    return Ok(Ability::TargetCost {
                        delta: if less { -n } else { n },
                        filter,
                        noun,
                    });
                }
                if self.at_words(&["less", "if"]) || self.at_words(&["more", "if"]) {
                    let less = self.at_word("less");
                    self.pos += 2;
                    let cond = self.static_cond()?;
                    self.end_stmt()?;
                    let n = i32::try_from(per).unwrap_or(0);
                    return Ok(Ability::CondCost {
                        delta: if less { -n } else { n },
                        cond,
                    });
                }
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
            "crews" | "saddles"
                if self.at_words(&["crews", "using"])
                    || self.at_words(&["crews", "plus"])
                    || self.at_words(&["saddles", "and", "crews"]) =>
            {
                let saddles = self.eat_words(&["saddles", "and"]);
                self.expect_word("crews")?;
                let (toughness, plus) = if self.eat_words(&["using", "toughness"]) {
                    (true, 0)
                } else {
                    self.expect_word("plus")?;
                    (false, self.number()?)
                };
                self.end_stmt()?;
                Ok(Ability::CrewsAs {
                    saddles,
                    toughness,
                    plus,
                })
            }
            "as" if self.at_words(&["as", "turned", "face", "up", "put"])
                || self.at_words(&["as", "enters", "or", "turned", "face", "up", "put"]) =>
            {
                let enters = self.word_at(1).as_deref() == Some("enters");
                self.pos += if enters { 7 } else { 5 };
                let n = self.num()?;
                let kind = self.counter_kind()?;
                self.end_stmt()?;
                Ok(Ability::AsTurnedFaceUp { enters, kind, n })
            }
            "may" if self.word_at(1).as_deref() == Some("look") => {
                self.pos += 2;
                let top = self.eat_word("top");
                let face_down = self.eat_word("face-down");
                let opponents = face_down && self.eat_word("opponents");
                if !top && !face_down {
                    return self.fail("expected `top` or `face-down`".to_string());
                }
                self.end_stmt()?;
                Ok(Ability::MayLook {
                    top,
                    face_down,
                    opponents,
                })
            }
            "play" if self.at_words(&["play", "from", "top"]) => {
                self.pos += 3;
                let once = self.eat_word("once");
                let lands = self.eat_word("lands");
                let spells = if (!lands || self.eat_word("and")) && self.eat_word("spells") {
                    if self.at_stmt_end() {
                        Some(Filter::default())
                    } else {
                        Some(self.filter()?)
                    }
                } else {
                    None
                };
                if !lands && spells.is_none() {
                    return self.fail("expected `lands` or `spells`".to_string());
                }
                self.end_stmt()?;
                Ok(Ability::PlayFromTop {
                    lands,
                    spells,
                    once,
                })
            }
            "top" if self.at_words(&["top", "revealed"]) => {
                self.pos += 2;
                let players = self.eat_word("players");
                self.end_stmt()?;
                Ok(Ability::TopRevealed { players })
            }
            "extra" if self.word_at(1).as_deref() == Some("vote") => {
                self.pos += 2;
                let optional = self.eat_word("may");
                self.end_stmt()?;
                Ok(Ability::ExtraVote { optional })
            }
            "protection" if self.at_words(&["protection", "doesn't", "remove", "this"]) => {
                self.pos += 4;
                self.end_stmt()?;
                Ok(Ability::AuraStays)
            }
            "deck" if self.at_words(&["deck", "any", "number"]) => {
                self.pos += 3;
                self.end_stmt()?;
                Ok(Ability::DeckAnyNumber)
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
            // `enters with choice flying counter or first strike counter`:
            // "... enters with your choice of a flying counter or a first
            // strike counter on it."
            "enters" | "escapes"
                if self.word_at(1).as_deref() == Some("with")
                    && self.word_at(2).as_deref() == Some("choice") =>
            {
                let escapes = w.eq_ignore_ascii_case("escapes");
                self.pos += 3;
                let a = self.counter_kind()?;
                self.expect_word("or")?;
                let b = self.counter_kind()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::CounterChoice([a, b], escapes)))
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
            "kicked" if self.word_at(1).as_deref() == Some("with") => {
                self.pos += 2;
                let n = self.number()?;
                if self.counter_kind()? != CounterKind::PlusOnePlusOne {
                    return self.fail("kicked with +1/+1 counters only");
                }
                let keywords = if self.eat_word("and") {
                    self.keyword_list()?
                } else {
                    Vec::new()
                };
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::KickedWith(n, keywords)))
            }
            "modular" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let n = self.number()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Modular(n)))
            }
            "soulbond" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Soulbond))
            }
            "sunburst" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Sunburst))
            }
            "graft" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let n = self.number()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Graft(n)))
            }
            "vanishing" => {
                self.pos += 1;
                let n = if matches!(self.peek(), Some(Tok::Num(_))) {
                    self.number()?
                } else {
                    0
                };
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Vanishing(n)))
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
            "provoke" => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Provoke))
            }
            "ingest" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Ingest))
            }
            "reinforce"
                if matches!(self.peek_at(1), Some(Tok::Num(_)))
                    && matches!(self.peek_at(2), Some(Tok::Sym(_))) =>
            {
                self.pos += 1;
                let n = self.number()?;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Reinforce(n, c)))
            }
            "retrace" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Retrace))
            }
            "jump" if self.at_words(&["jump", "start"]) => {
                self.pos += 2;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::JumpStart))
            }
            "riot" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Riot))
            }
            "living" if self.at_words(&["living", "weapon"]) => {
                self.pos += 2;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::LivingWeapon))
            }
            "impending" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let n = self.number()?;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Impending(n, c)))
            }
            "web"
                if self.at_words(&["web", "slinging"])
                    && matches!(self.peek_at(2), Some(Tok::Sym(_))) =>
            {
                self.pos += 2;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::WebSlinging(c)))
            }
            "job" if self.at_words(&["job", "select"]) => {
                self.pos += 2;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::JobSelect))
            }
            "extort" | "mentor" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(if w == "extort" {
                    Mechanic::Extort
                } else {
                    Mechanic::Mentor
                }))
            }
            "firebending" => {
                self.pos += 1;
                let n = self.num()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Firebending(n)))
            }
            "evolve" | "unleash" | "increment" => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(match w.as_str() {
                    "evolve" => Mechanic::Evolve,
                    "increment" => Mechanic::Increment,
                    _ => Mechanic::Unleash,
                }))
            }
            "bushido" | "soulshift" | "afflict" | "toxic" | "crew" | "bloodthirst" | "backup"
            | "saddle" | "poisonous" | "frenzy" | "absorb" | "fabricate" | "renown"
            | "afterlife" | "casualty" | "teamwork" | "ripple" | "dredge"
                if matches!(self.peek_at(1), Some(Tok::Num(_))) =>
            {
                self.pos += 1;
                let n = self.number()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(match w.as_str() {
                    "bushido" => Mechanic::Bushido(n),
                    "soulshift" => Mechanic::Soulshift(n),
                    "afflict" => Mechanic::Afflict(n),
                    "crew" => Mechanic::Crew(n),
                    "bloodthirst" => Mechanic::Bloodthirst(n),
                    "backup" => Mechanic::Backup(n),
                    "saddle" => Mechanic::Saddle(n),
                    "poisonous" => Mechanic::Poisonous(n),
                    "frenzy" => Mechanic::Frenzy(n),
                    "absorb" => Mechanic::Absorb(n),
                    "fabricate" => Mechanic::Fabricate(n),
                    "renown" => Mechanic::Renown(n),
                    "afterlife" => Mechanic::Afterlife(n),
                    "casualty" => Mechanic::Casualty(n),
                    "dredge" => Mechanic::Dredge(n),
                    "teamwork" => Mechanic::Teamwork(n),
                    "ripple" => Mechanic::Ripple(n),
                    _ => Mechanic::Toxic(n),
                }))
            }
            "ward" | "echo" | "unearth" | "buyback" | "outlast" | "morph" | "megamorph"
            | "disguise" | "evoke" | "madness" | "mayhem" | "foretell" | "plot" | "sneak"
            | "harmonize" | "entwine" | "ninjutsu" | "scavenge" | "dash" | "warp" | "embalm"
            | "eternalize" | "offspring" | "bestow" | "splice" | "strive" | "overload"
            | "surge" | "freerunning" | "spectacle" | "replicate" | "encore" | "multikicker"
            | "transmute" | "disturb"
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
                    "mayhem" => Mechanic::Mayhem(c),
                    "foretell" => Mechanic::Foretell(c),
                    "plot" => Mechanic::Plot(c),
                    "sneak" => Mechanic::Sneak(c),
                    "harmonize" => Mechanic::Harmonize(c),
                    "entwine" => Mechanic::Entwine(c),
                    "megamorph" => Mechanic::Megamorph(c),
                    "disguise" => Mechanic::Disguise(c),
                    "ninjutsu" => Mechanic::Ninjutsu(c),
                    "scavenge" => Mechanic::Scavenge(c),
                    "dash" => Mechanic::Dash(c),
                    "warp" => Mechanic::Warp(c),
                    "bestow" => Mechanic::Bestow(c),
                    "overload" => Mechanic::Overload(c),
                    "splice" => Mechanic::Splice(c),
                    "embalm" => Mechanic::Embalm(c),
                    "eternalize" => Mechanic::Eternalize(c),
                    "offspring" => Mechanic::Offspring(c),
                    "strive" => Mechanic::Strive(c),
                    "surge" => Mechanic::Surge(c),
                    "freerunning" => Mechanic::Freerunning(c),
                    "spectacle" => Mechanic::Spectacle(c),
                    "replicate" => Mechanic::Replicate(c),
                    "encore" => Mechanic::Encore(c),
                    "multikicker" => Mechanic::Multikicker(c),
                    "transmute" => Mechanic::Transmute(c),
                    "disturb" => Mechanic::Disturb(c),
                    _ => Mechanic::Outlast(c),
                }))
            }
            // "Ward—Pay 3 life.", "Morph—Discard a card.", "Buyback—
            // Sacrifice a land.": a keyword whose cost isn't mana.
            "ward" | "morph" | "buyback" | "splice" | "echo"
                if matches!(self.peek_at(1), Some(Tok::Word(_))) =>
            {
                self.pos += 1;
                let c = self.cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(match w.as_str() {
                    "ward" => Mechanic::WardCost(c),
                    "morph" => Mechanic::MorphCost(c),
                    "splice" => Mechanic::SpliceCost(c),
                    "echo" => Mechanic::EchoCost(c),
                    _ => Mechanic::BuybackCost(c),
                }))
            }
            // CR 702.167: `craft with artifact {2}{R}`.
            "craft" if self.word_at(1).as_deref() == Some("with") => {
                self.pos += 2;
                let f = self.filter()?;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Craft(f, c)))
            }
            "conspire" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Conspire))
            }
            "awaken" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let n = self.number()?;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Awaken(n, c)))
            }
            "storm" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Storm))
            }
            "assist" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Assist))
            }
            "enlist" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Enlist))
            }
            "mobilize" => {
                self.pos += 1;
                let n = self.num()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Mobilize(n)))
            }
            "station" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Station))
            }
            "station" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let n = self.number()?;
                self.expect(&Tok::Colon)?;
                if !self.keyword_ahead() {
                    let ability = Box::new(self.ability()?);
                    return Ok(Ability::StationAbility { n, ability });
                }
                let keywords = self.keyword_list()?;
                self.end_stmt()?;
                Ok(Ability::StationLevel { n, keywords })
            }
            "bargain" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Bargain))
            }
            "ascend" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Ascend))
            }
            "storied" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Storied))
            }
            "start" if self.at_words(&["start", "your", "engines"]) => {
                self.pos += 3;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::StartYourEngines))
            }
            "exiled" if self.at_words(&["exiled", "instead", "of", "graveyard"]) => {
                self.pos += 4;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::GraveyardExile))
            }
            "daybound" | "nightbound" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(if w == "daybound" {
                    Mechanic::Daybound
                } else {
                    Mechanic::Nightbound
                }))
            }
            "becomes" if self.at_words(&["becomes", "day", "as", "this", "enters"]) => {
                self.pos += 5;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::BecomesDay))
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
                        or_card: crate::ast::OrCard::No,
                        who: None,
                        may: false,
                    }])],
                    equip: false,
                    cycling: true,
                })
            }
            "cycling" => {
                self.pos += 1;
                // "Cycling—Pay 2 life.": a cost other than mana.
                let cost = if matches!(self.peek(), Some(Tok::Word(_))) {
                    Cost {
                        discard_self: true,
                        ..self.cost()?
                    }
                } else {
                    Cost {
                        mana: self.mana_cost()?,
                        discard_self: true,
                        ..Default::default()
                    }
                };
                self.end_stmt()?;
                Ok(Ability::Activated {
                    cost,
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
                    if self.eat_word("any") || self.at_word("chosen") {
                        out.push(if self.eat_word("chosen") {
                            Mana::Chosen
                        } else {
                            Mana::Any
                        });
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
                if self.at_word("only") || self.at_word("not") {
                    let [one] = out.as_slice() else {
                        return self.fail("a restricted mana ability makes one kind of mana");
                    };
                    let mana = *one;
                    let restriction = self.mana_restriction()?;
                    self.end_stmt()?;
                    return Ok(Ability::RestrictedMana { mana, restriction });
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
                self.kicked_targets = None;
                self.kicked_instead = None;
                self.in_spell = true;
                let mut effects = self.effects_body()?;
                // `spell: choose one { ... }` is a modal spell: its modes
                // are chosen on casting (CR 700.2a), not on resolution.
                let lone_modes = self.targets.is_empty()
                    && matches!(effects.as_slice(), [s] if s.label.is_none()
                        && matches!(s.effects.as_slice(), [Effect::Modes(_) | Effect::ModesTwo(_)]));
                if lone_modes {
                    match effects.pop().and_then(|mut s| s.effects.pop()) {
                        Some(Effect::Modes(modes)) => return Ok(Ability::Modal(modes)),
                        // `spell: choose two { ... }`: "Choose two —".
                        Some(Effect::ModesTwo(modes)) => return Ok(Ability::ModalTwo(modes)),
                        _ => unreachable!("checked"),
                    }
                }
                Ok(Ability::Spell {
                    targets: std::mem::take(&mut self.targets),
                    effects,
                })
            }
            "gift" => {
                self.pos += 1;
                let kind = if self.eat_word("card") {
                    crate::ast::GiftKind::Card
                } else if self.eat_word("food") {
                    crate::ast::GiftKind::Food
                } else if self.eat_words(&["tapped", "fish"]) {
                    crate::ast::GiftKind::TappedFish
                } else if self.eat_word("treasure") {
                    crate::ast::GiftKind::Treasure
                } else {
                    return self.fail(
                        "expected `card`, `food`, `tapped fish`, or `treasure` after `gift`",
                    );
                };
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Gift(kind)))
            }
            "pawprint" if self.peek_at(2) == Some(&Tok::LBrace) => {
                self.pos += 1;
                let limit = u8::try_from(self.number()?).unwrap_or(u8::MAX);
                let modes = self.pawprint_block()?;
                self.end_stmt()?;
                Ok(Ability::Pawprint { limit, modes })
            }
            "spree" if self.peek_at(1) == Some(&Tok::LBrace) => {
                self.pos += 1;
                let modes = self.spree_block()?;
                self.end_stmt()?;
                Ok(Ability::Spree(modes))
            }
            "choose" => {
                self.pos += 1;
                if self.eat_word("two") {
                    let modes = self.modes_block()?;
                    self.end_stmt()?;
                    return Ok(Ability::ModalTwo(modes));
                }
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
            "activated" | "boast" | "exhaust" | "power-up" => {
                self.pos += 1;
                let mut cost = self.cost()?;
                cost.keyword = match w.as_str() {
                    "boast" => Some(mtg_core::card::ActivatedKeyword::Boast),
                    "exhaust" => Some(mtg_core::card::ActivatedKeyword::Exhaust),
                    "power-up" => Some(mtg_core::card::ActivatedKeyword::PowerUp),
                    _ => None,
                };
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
                // "0:" (CR 606.3) is written `loyalty 0:` or `loyalty +0:`.
                let n = match self.next() {
                    Some(Tok::Signed(n)) => n,
                    Some(Tok::Num(0)) => 0,
                    _ => return self.fail("expected a loyalty cost such as +1, 0, or -3"),
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
                // "Equip Knight {1}": only onto a creature of that type
                // you control (CR 702.6c).
                let target = match self.peek() {
                    Some(Tok::Word(w)) if w.chars().next().is_some_and(char::is_uppercase) => {
                        let sub = w.clone();
                        self.pos += 1;
                        Target::Filter(Filter {
                            who: Some(Who::You),
                            types: vec![CardType::Creature],
                            subtypes: vec![sub],
                            ..Default::default()
                        })
                    }
                    _ => Target::Spec(TargetSpec::CreatureYouControl),
                };
                let mana = self.mana_cost()?;
                // "Equip {0}. Activate only once each turn."
                let per_turn = if self.peek() == Some(&Tok::Comma) {
                    self.pos += 1;
                    self.expect_word("once")?;
                    1
                } else {
                    0
                };
                self.end_stmt()?;
                Ok(Ability::Activated {
                    cost: Cost {
                        mana,
                        sorcery: true,
                        per_turn,
                        ..Default::default()
                    },
                    targets: vec![target],
                    effects: vec![Sentence::new(vec![Effect::Attach {
                        what: Recipient::Source,
                        to: Recipient::Target(0),
                    }])],
                    equip: true,
                    cycling: false,
                })
            }
            "triggered" => {
                self.pos += 1;
                self.in_trigger = true;
                // CR 113.6k: works only from the graveyard.
                let from_graveyard = self.eat_words(&["from", "graveyard"]);
                // "This ability triggers only once each turn.", "Do this
                // only once each turn."
                let limit = if self.eat_words(&["once", "each", "turn"]) {
                    mtg_core::card::TurnLimit::Triggers(1)
                } else if self.eat_words(&["twice", "each", "turn"]) {
                    mtg_core::card::TurnLimit::Triggers(2)
                } else if self.eat_words(&["do", "once", "each", "turn"]) {
                    mtg_core::card::TurnLimit::Do
                } else {
                    mtg_core::card::TurnLimit::None
                };
                let when = self.at_word("when");
                let mut trigger = self.trigger()?;
                self.it_source = matches!(&trigger, Trigger::CastsFiltered { filter, .. }
                    if filter.targeting.as_ref().and_then(|t| t.filter.as_ref())
                        .is_some_and(|f| f.relation == Relation::This));
                if when && !crate::format::natural_when(&trigger) {
                    trigger = Trigger::When(Box::new(trigger));
                }
                // "attacks while saddled", "attacks while you control two
                // or more artifacts": a trigger condition, not an
                // intervening "if" (CR 603.2, 603.4).
                let condition = if self.eat_word("while") {
                    Some(crate::ast::TrigCond::While(self.static_cond()?))
                } else if self.eat_words(&["during", "your", "turn"]) {
                    // "Whenever a land enters during your turn": a trigger
                    // condition (CR 603.2).
                    Some(crate::ast::TrigCond::While(StaticCond::DuringYourTurn))
                } else if self.eat_word("if") {
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
                    limit,
                    trigger,
                    condition,
                    targets: std::mem::take(&mut self.targets),
                    effects,
                    from_graveyard,
                })
            }
            "replace" => {
                self.pos += 1;
                let r = self.replacement()?;
                Ok(Ability::Replacement(r))
            }
            "as" if self.at_words(&["as", "this", "enters", "choose"])
                || self.at_words(&["as", "it", "enters", "choose"]) =>
            {
                let joined = self.at_words(&["as", "it"]);
                self.pos += 4;
                let kind = self.choice_kind(false)?;
                self.end_stmt()?;
                Ok(Ability::AsEntersChoose { kind, joined })
            }
            "enters" if self.at_words(&["enters", "as"]) => {
                self.pos += 2;
                self.expect_word("copy")?;
                self.expect_word("of")?;
                let f = self.filter()?;
                self.end_stmt()?;
                Ok(Ability::EntersAsCopy(f))
            }
            // "As this land enters, you may pay 3 life. If you don't, it
            // enters tapped."
            "enters" if self.at_words(&["enters", "tapped", "unless", "pay"]) => {
                self.pos += 4;
                let n = self.number()?;
                self.expect_word("life")?;
                self.end_stmt()?;
                Ok(Ability::Replacement(Replacement {
                    event: Replaces::Enters,
                    outcome: Outcome::EnterTappedUnlessPay(n),
                    then: Vec::new(),
                }))
            }
            "enters" if self.at_words(&["enters", "tapped", "if", "not", "your", "turn"]) => {
                self.pos += 6;
                self.end_stmt()?;
                Ok(Ability::EntersTappedIfNotYourTurn)
            }
            "enters" if self.at_words(&["enters", "tapped", "unless"]) => {
                self.pos += 3;
                let c = self.static_cond()?;
                self.end_stmt()?;
                Ok(Ability::EntersTappedUnless(c))
            }
            // Adamant: "If at least three white mana was spent to cast this
            // spell, this creature enters with a +1/+1 counter on it."
            "if" => {
                self.pos += 1;
                let cond = self.static_cond()?;
                self.expect(&Tok::Comma)?;
                self.expect_word("enters")?;
                self.expect_word("with")?;
                let n = self.number()?;
                let kind = self.counter_kind()?;
                self.end_stmt()?;
                Ok(Ability::EntersWithIf {
                    n,
                    kind,
                    cond,
                    first: true,
                })
            }
            "enters"
                if self.word_at(1).as_deref() == Some("with")
                    && matches!(self.peek_at(2), Some(Tok::Num(_)))
                    && self.counter_then_if() =>
            {
                self.pos += 2;
                let n = self.number()?;
                let kind = self.counter_kind()?;
                self.expect_word("if")?;
                let cond = self.static_cond()?;
                self.end_stmt()?;
                Ok(Ability::EntersWithIf {
                    n,
                    kind,
                    cond,
                    first: false,
                })
            }
            "enters" => {
                self.pos += 1;
                let outcome = self.enters_outcome()?;
                if let Outcome::EnterWithCounters { kind, n: Num::N(n) } = outcome
                    && self.eat_word("if")
                {
                    let cond = self.static_cond()?;
                    self.end_stmt()?;
                    return Ok(Ability::EntersWithIf {
                        n,
                        kind,
                        cond,
                        first: false,
                    });
                }
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
                    || self.at_words(&["can't", "attack", "or", "block", "unless"])
                    || self.at_words(&["can't", "attack", "if"])
                    || self.at_words(&["can't", "block", "if"])
                    || self.at_words(&["can't", "attack", "or", "block", "if"]) =>
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
                let negated = self.eat_word("if");
                if !negated {
                    self.expect_word("unless")?;
                }
                let cond = self.static_cond()?;
                self.end_stmt()?;
                Ok(Ability::AttackUnless {
                    cond,
                    attack,
                    block,
                    negated,
                })
            }
            "can't" | "can" => {
                self.pos += 1;
                let rule = if w == "can" {
                    self.expect_word("block")?;
                    if self.eat_words(&["shadow", "as", "though"]) {
                        let had = self.eat_word("had");
                        if !had {
                            self.expect_words(&["didn't", "have"])?;
                        }
                        self.expect_word("shadow")?;
                        BlockRule::BlocksShadow { had }
                    } else if self.eat_words(&["landwalk", "as", "though", "didn't", "have"]) {
                        BlockRule::BlocksLandwalk
                    } else if self.eat_words(&["an", "additional"]) {
                        let n = u8::try_from(self.number()?).unwrap_or(u8::MAX);
                        self.expect_words(&["creatures", "each", "combat"])?;
                        BlockRule::BlocksAdditional(n)
                    } else {
                        self.expect_word("only")?;
                        BlockRule::BlocksOnly(self.filter()?)
                    }
                } else if self.eat_word("block") {
                    BlockRule::CantBlock(self.filter()?)
                } else {
                    self.expect_words(&["be", "blocked"])?;
                    if self.eat_words(&["by", "more", "than", "one", "creature"]) {
                        BlockRule::AtMostOne
                    } else if self.eat_words(&["by", "creature", "with", "lesser", "power"]) {
                        BlockRule::NotByLesserPower
                    } else if self.eat_words(&["except", "by"]) {
                        // "except by three or more creatures".
                        if matches!(self.peek(), Some(Tok::Num(_)))
                            && self.word_at(1).as_deref() == Some("or")
                        {
                            let n = u8::try_from(self.number()?).unwrap_or(u8::MAX);
                            self.expect_words(&["or", "more", "creatures"])?;
                            BlockRule::MinBlockers(n)
                        } else {
                            BlockRule::OnlyBy(self.filter()?)
                        }
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
                } else if self.eat_words(&["during", "turns", "other", "than", "yours"]) {
                    condition = Some(StaticCond::NotYourTurn);
                    cond_first = true;
                    self.expect(&Tok::Comma)?;
                }
                if condition.is_some() && self.at_word("spells") {
                    return self.cost_change(condition);
                }
                let each = self.at_word("each");
                let mut filter = self.filter()?;
                filter.each = each;
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
                let mut cond_if = false;
                if condition.is_none() && self.eat_words(&["as", "long", "as"]) {
                    condition = Some(self.static_cond()?);
                } else if condition.is_none() && self.eat_word("if") {
                    condition = Some(self.static_cond()?);
                    cond_if = true;
                }
                self.end_stmt()?;
                Ok(Ability::Static {
                    filter,
                    mods,
                    condition,
                    cond_first,
                    cond_if,
                })
            }
            _ => self.unexpected("a statement"),
        }
    }

    /// `spells artifact you cast cost {1} less` (at `spells`), with the
    /// static condition written before it, if any.
    fn cost_change(&mut self, cond: Option<StaticCond>) -> R<Ability> {
        self.pos += 1;
        let filter = if self.at_word("you") || self.at_word("your") || self.at_word("cost") {
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
            cond,
        })
    }

    /// `7 cards in your graveyard`, `4 card types in your graveyard`,
    /// `you control [3] <filter>`, `your turn`, or the ability words
    /// `threshold`, `delirium`, `metalcraft`.
    /// `when you do, <sentence>` (CR 603.12): a reflexive trigger, with
    /// targets of its own.
    fn when_you_do(&mut self) -> R<Effect> {
        self.pos += 3;
        if self.peek() == Some(&Tok::Comma) {
            self.pos += 1;
        }
        let outer = std::mem::take(&mut self.targets);
        let saved = self.subject.take();
        let s = self.sentence();
        self.subject = saved;
        let targets = std::mem::replace(&mut self.targets, outer);
        Ok(Effect::WhenYouDo {
            targets,
            effects: vec![s?],
        })
    }

    /// `{ <ability line> }`: an ability granted by another ("has \"...\""),
    /// parsed with targets of its own.
    /// `ward {2}` or `toxic 1` after `has` or `gains`: a keyword ability
    /// with a parameter, granted bare ("has ward {2}", "gains toxic 1"; CR
    /// 702.21, 702.164).
    fn granted_mechanic(&mut self) -> R<Option<Ability>> {
        if self.at_word("ward") && matches!(self.peek_at(1), Some(Tok::Sym(_))) {
            self.pos += 1;
            return Ok(Some(Ability::Mechanic(Mechanic::Ward(self.mana_cost()?))));
        }
        // `has ward pay 2 life`: 'have "Ward—Pay 2 life."'.
        if self.at_words(&["ward", "pay"]) {
            self.pos += 1;
            return Ok(Some(Ability::Mechanic(Mechanic::WardCost(self.cost()?))));
        }
        if self.at_word("toxic") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            return Ok(Some(Ability::Mechanic(Mechanic::Toxic(self.number()?))));
        }
        // "All Sliver creatures have poisonous 1", "... have provoke",
        // "... have outlast {2}".
        if let Some(w) = self.word_at(0)
            && matches!(w.as_str(), "poisonous" | "frenzy" | "absorb")
            && matches!(self.peek_at(1), Some(Tok::Num(_)))
        {
            self.pos += 1;
            let n = self.number()?;
            return Ok(Some(Ability::Mechanic(match w.as_str() {
                "poisonous" => Mechanic::Poisonous(n),
                "frenzy" => Mechanic::Frenzy(n),
                _ => Mechanic::Absorb(n),
            })));
        }
        if self.eat_word("provoke") {
            return Ok(Some(Ability::Mechanic(Mechanic::Provoke)));
        }
        if self.eat_word("mentor") {
            return Ok(Some(Ability::Mechanic(Mechanic::Mentor)));
        }
        if self.at_word("outlast") && matches!(self.peek_at(1), Some(Tok::Sym(_))) {
            self.pos += 1;
            return Ok(Some(Ability::Mechanic(Mechanic::Outlast(
                self.mana_cost()?,
            ))));
        }
        if self.granted_protection_ahead() {
            self.pos += 2;
            let filter = if self.eat_word("everything") {
                Filter::default()
            } else {
                self.filter()?
            };
            return Ok(Some(Ability::Protection(filter)));
        }
        Ok(None)
    }

    fn granted_mechanic_ahead(&self) -> bool {
        (self.at_word("ward") && matches!(self.peek_at(1), Some(Tok::Sym(_))))
            || self.at_words(&["ward", "pay"])
            || (self.at_word("toxic") && matches!(self.peek_at(1), Some(Tok::Num(_))))
            || self.granted_protection_ahead()
            || (self
                .word_at(0)
                .is_some_and(|w| matches!(w.as_str(), "poisonous" | "frenzy" | "absorb"))
                && matches!(self.peek_at(1), Some(Tok::Num(_))))
            || self.at_word("provoke")
            || self.at_word("mentor")
            || (self.at_word("outlast") && matches!(self.peek_at(1), Some(Tok::Sym(_))))
    }

    /// `protection from artifact` granted (CR 702.16): from a quality
    /// other than a color (colors are keywords) or a choice.
    fn granted_protection_ahead(&self) -> bool {
        self.at_word("protection")
            && self.word_at(1).as_deref() == Some("from")
            && self
                .word_at(2)
                .as_deref()
                .is_some_and(|w| color(w).is_none() && !matches!(w, "chosen" | "color" | "colors"))
    }

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
            let name = match self.peek() {
                Some(Tok::Str(s)) => {
                    let s = s.clone();
                    self.pos += 1;
                    Some(s)
                }
                _ => None,
            };
            self.targets.clear();
            self.subject = None;
            self.revealed = false;
            let effects = self.effects_body()?;
            modes.push(Mode {
                targets: std::mem::take(&mut self.targets),
                effects,
                name,
            });
        }
        self.targets = outer;
        Ok(modes)
    }

    /// `{ mode {2}: ... mode {1}: ... }` after `spree`: each mode's cost,
    /// then its effects.
    /// `{ mode 1: ... mode 2: ... }` after `pawprint 5`: each mode with its
    /// number of pawprints.
    fn pawprint_block(&mut self) -> R<Vec<(u8, Mode)>> {
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
            let Some(w) = u8::try_from(self.number()?).ok().filter(|w| *w > 0) else {
                return self.fail("a pawprint mode has one or more pawprints");
            };
            self.targets.clear();
            self.subject = None;
            self.revealed = false;
            let effects = self.effects_body()?;
            modes.push((
                w,
                Mode {
                    targets: std::mem::take(&mut self.targets),
                    effects,
                    name: None,
                },
            ));
        }
        self.targets = outer;
        if modes.len() < 2 {
            return self.fail("`pawprint` takes two or more modes");
        }
        Ok(modes)
    }

    fn spree_block(&mut self) -> R<Vec<(ManaCost, Mode)>> {
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
            let cost = self.mana_cost()?;
            self.targets.clear();
            self.subject = None;
            self.revealed = false;
            let effects = self.effects_body()?;
            modes.push((
                cost,
                Mode {
                    targets: std::mem::take(&mut self.targets),
                    effects,
                    name: None,
                },
            ));
        }
        self.targets = outer;
        if !(1..=3).contains(&modes.len()) {
            return self.fail("`spree` takes one to three modes");
        }
        Ok(modes)
    }

    /// After `this has`: `4 or more oil counters`, `no arrowhead counters`.
    fn source_counters_cond(&mut self) -> R<StaticCond> {
        if self.eat_word("no") {
            let kind = self.counter_kind()?;
            return Ok(StaticCond::SourceCounters {
                kind,
                n: 0,
                at_least: false,
            });
        }
        let n = self.number()?;
        self.expect_words(&["or", "more"])?;
        let kind = self.counter_kind()?;
        Ok(StaticCond::SourceCounters {
            kind,
            n,
            at_least: true,
        })
    }

    /// After `spent`: `{B}`, `{R}{R}`, `3 red`, `4 mana`, `treasure`.
    fn spent_cond(&mut self) -> R<StaticCond> {
        use mtg_core::card::SpentKind;
        if self.eat_word("treasure") {
            return Ok(StaticCond::ManaSpent {
                kind: SpentKind::Treasure,
                at_least: 1,
                that: false,
            });
        }
        if matches!(self.peek(), Some(Tok::Sym(_))) {
            let cost = self.mana_cost()?;
            let colors = cost.colors();
            let mut it = colors.iter();
            let (Some(c), None) = (it.next(), it.next()) else {
                return self.fail("`spent` names mana of one color");
            };
            let n = u8::try_from(cost.mana_value()).unwrap_or(0);
            if n == 0
                || cost
                    .symbols()
                    .iter()
                    .any(|s| *s != mtg_core::mana::ManaSymbol::Colored(c))
            {
                return self.fail("`spent` names colored symbols only");
            }
            return Ok(StaticCond::ManaSpent {
                kind: SpentKind::Color(c),
                at_least: n,
                that: false,
            });
        }
        let n = u8::try_from(self.number()?).unwrap_or(u8::MAX);
        if self.eat_word("mana") {
            return Ok(StaticCond::ManaSpent {
                kind: SpentKind::Total,
                at_least: n,
                that: false,
            });
        }
        let Some(Tok::Word(w)) = self.peek() else {
            return self.fail("expected a color after `spent N`");
        };
        let Some(c) = color(w) else {
            return self.fail("expected a color after `spent N`");
        };
        self.pos += 1;
        Ok(StaticCond::ManaSpent {
            kind: SpentKind::Color(c),
            at_least: n,
            that: false,
        })
    }

    #[allow(clippy::too_many_lines)] // one branch per condition
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
        if self.eat_words(&["your", "main", "phase"]) {
            return Ok(StaticCond::YourMainPhase);
        }
        if self.eat_words(&["your", "combat", "phase"]) {
            return Ok(StaticCond::YourCombatPhase);
        }
        if self.eat_words(&["first", "combat", "phase"]) {
            return Ok(StaticCond::FirstCombatPhase);
        }
        if self.eat_words(&["put", "counter", "this", "turn"]) {
            return Ok(StaticCond::PutCounterThisTurn);
        }
        if self.eat_word("resolution") {
            let n = u8::try_from(self.number()?).unwrap_or(u8::MAX);
            let again = self.eat_word("again");
            return Ok(StaticCond::NthResolution { n, again });
        }
        if self.eat_words(&["not", "your", "turn"]) {
            return Ok(StaticCond::NotYourTurn);
        }
        if self.eat_words(&["cast", "during", "main", "phase"]) {
            return Ok(StaticCond::CastDuringMain);
        }
        if self.eat_word("controlled") {
            let f = self.filter()?;
            self.expect_words(&["as", "cast"])?;
            return Ok(StaticCond::ControlledAsCast(f));
        }
        // "there is a land card in your graveyard", "there's a Lesson card
        // in your graveyard".
        let there = if self.eat_words(&["there", "is"]) {
            Some(crate::ast::GraveyardWording::ThereIs)
        } else if self.eat_word("there's") {
            Some(crate::ast::GraveyardWording::Theres)
        } else {
            None
        };
        if let Some(wording) = there {
            let filter = self.filter()?;
            self.graveyard_cards_tail()?;
            return Ok(StaticCond::GraveyardCards {
                filter,
                at_least: 1,
                wording,
            });
        }
        if let Some(t) = self.turn_time() {
            return Ok(t);
        }
        if self.eat_words(&["this", "is"]) {
            return Ok(StaticCond::Is(self.filter()?));
        }
        if self.eat_words(&["attached", "is"]) {
            return Ok(StaticCond::AttachedIs(self.filter()?));
        }
        if self.eat_word("modified") {
            return Ok(StaticCond::Modified);
        }
        if self.eat_words(&["attacking", "alone"]) {
            return Ok(StaticCond::AttackingAlone);
        }
        if self.eat_word("blocked") {
            return Ok(StaticCond::Blocked);
        }
        if self.eat_words(&["being", "blocked"]) {
            return Ok(StaticCond::BeingBlocked);
        }
        if self.eat_word("drew") {
            let n = self.number()?;
            // `drew 2 cards this turn` (the "instead" form) and `drew 2 this
            // turn` (statics) read the same count.
            let cards = self.eat_word("cards");
            self.expect_words(&["this", "turn"])?;
            return Ok(if cards {
                StaticCond::CardsDrawnThisTurn(n)
            } else {
                StaticCond::DrawnThisTurn(n)
            });
        }
        if self.eat_word("cast") {
            if self.eat_word("from") {
                let zone = if self.eat_word("hand") {
                    mtg_core::state::Zone::Hand
                } else if self.eat_word("graveyard") {
                    mtg_core::state::Zone::Graveyard
                } else if self.eat_word("exile") {
                    mtg_core::state::Zone::Exile
                } else {
                    return self.unexpected("`hand`, `graveyard`, or `exile`");
                };
                return Ok(StaticCond::CastFrom(zone));
            }
            let at_least = if matches!(self.peek(), Some(Tok::Num(_))) {
                self.number()?
            } else {
                1
            };
            let other = self.eat_word("another");
            let filter = if self.at_word("spell") {
                Filter::default()
            } else {
                self.filter()?
            };
            self.eat_word("spell");
            self.expect_words(&["this", "turn"])?;
            return Ok(StaticCond::CastThisTurn {
                filter,
                at_least,
                other,
            });
        }
        if self.eat_words(&["gained", "life", "this", "turn"]) {
            return Ok(StaticCond::GainedLifeThisTurn { or_lost: false });
        }
        if self.eat_words(&["gained", "or", "lost", "life", "this", "turn"]) {
            return Ok(StaticCond::GainedLifeThisTurn { or_lost: true });
        }
        if self.eat_words(&["descended", "this", "turn"]) {
            return Ok(StaticCond::DescendedThisTurn);
        }
        if self.eat_words(&["spell", "number"]) {
            let n = self.number()?;
            self.expect_words(&["this", "turn"])?;
            return Ok(StaticCond::SpellNumber(n));
        }
        if self.eat_words(&["surveilled", "this", "turn"]) {
            return Ok(StaticCond::SurveilledThisTurn);
        }
        if self.eat_words(&["counter", "put", "this", "turn"]) {
            return Ok(StaticCond::CounterPutThisTurn);
        }
        if self.eat_words(&["face-down", "creature", "entered", "this", "turn"]) {
            return Ok(StaticCond::FaceDownEnteredThisTurn);
        }
        if self.eat_words(&["creature", "entered", "this", "turn"]) {
            return Ok(StaticCond::CreatureEnteredThisTurn { other: false });
        }
        if self.eat_words(&["another", "creature", "entered", "this", "turn"]) {
            return Ok(StaticCond::CreatureEnteredThisTurn { other: true });
        }
        // `2 nonland entered this turn` (celebration).
        if matches!(self.peek(), Some(Tok::Num(_)))
            && self.word_at(1).as_deref() == Some("nonland")
            && self.word_at(2).as_deref() == Some("entered")
        {
            let n = self.number()?;
            self.expect_words(&["nonland", "entered", "this", "turn"])?;
            return Ok(StaticCond::NonlandEnteredThisTurn(n));
        }
        // `artifact or creature to graveyard this turn`.
        let type_at = |w: Option<String>| {
            w.and_then(|w| {
                crate::render::CARD_TYPES
                    .into_iter()
                    .find(|t| crate::render::type_noun(*t) == w)
            })
        };
        if type_at(self.word_at(0)).is_some() {
            let mut k = 0;
            let mut types = Vec::new();
            while let Some(t) = type_at(self.word_at(k)) {
                types.push(t);
                if self.word_at(k + 1).as_deref() == Some("or") {
                    k += 2;
                } else {
                    k += 1;
                    break;
                }
            }
            let tail = ["to", "graveyard", "this", "turn"];
            if (0..4).all(|i| self.word_at(k + i).as_deref() == Some(tail[i])) {
                self.pos += k + 4;
                return Ok(StaticCond::PutIntoGraveyardThisTurn(types));
            }
            // `creature card to your graveyard this turn`.
            let tail = ["card", "to", "your", "graveyard", "this", "turn"];
            if (0..6).all(|i| self.word_at(k + i).as_deref() == Some(tail[i])) {
                self.pos += k + 6;
                return Ok(StaticCond::CardToGraveyardThisTurn(types));
            }
        }
        if self.eat_words(&["discarded", "this", "turn"]) {
            return Ok(StaticCond::DiscardedThisTurn);
        }
        if self.eat_word("night") {
            return Ok(StaticCond::Night);
        }
        if self.at_word("gained") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            let n = self.number()?;
            self.expect_words(&["life", "this", "turn"])?;
            return Ok(StaticCond::LifeGainedThisTurn(n));
        }
        if self.eat_word("sacrificed") {
            let mut types = Vec::new();
            loop {
                let w = self.word()?;
                let Some(t) = card_type(&w) else {
                    return self.fail(format!("expected a card type, found {w}"));
                };
                types.push(t);
                if !self.eat_word("or") {
                    break;
                }
            }
            self.expect_words(&["this", "turn"])?;
            return Ok(StaticCond::SacrificedThisTurn(types));
        }
        if self.eat_words(&["committed", "crime", "this", "turn"]) {
            return Ok(StaticCond::CrimeThisTurn);
        }
        if self.eat_words(&["card", "left", "your", "graveyard", "this", "turn"]) {
            return Ok(StaticCond::LeftGraveyardThisTurn);
        }
        if self.eat_words(&["more", "cards", "in", "hand", "than", "each", "opponent"]) {
            return Ok(StaticCond::MoreCardsInHand);
        }
        if self.eat_words(&["opponent", "poisoned"]) {
            return Ok(StaticCond::OpponentPoison(1));
        }
        if self.eat_words(&["opponent", "poison"]) {
            let n = self.number()?;
            self.expect_words(&["or", "more"])?;
            return Ok(StaticCond::OpponentPoison(n));
        }
        if self.eat_words(&["opponent", "graveyard"]) {
            let n = self.number()?;
            self.expect_words(&["or", "more"])?;
            return Ok(StaticCond::OpponentGraveyard(n));
        }
        if self.eat_words(&["opponent", "controls"]) {
            let at_least = if matches!(self.peek(), Some(Tok::Num(_))) {
                self.number()?
            } else {
                1
            };
            let filter = self.filter()?;
            return Ok(StaticCond::OpponentControls { filter, at_least });
        }
        if self.eat_words(&["you", "are", "monarch"]) {
            return Ok(StaticCond::Monarch);
        }
        if self.eat_words(&["completed", "dungeon"]) {
            return Ok(StaticCond::CompletedDungeon);
        }
        if self.eat_words(&["city's", "blessing"]) {
            return Ok(StaticCond::CityBlessing);
        }
        if self.eat_words(&["enduring", "story"]) {
            return Ok(StaticCond::EnduringStory);
        }
        if self.eat_words(&["no", "enduring", "story"]) {
            return Ok(StaticCond::NoEnduringStory);
        }
        if self.eat_words(&["max", "speed"]) {
            return Ok(StaticCond::MaxSpeed);
        }
        if self.eat_words(&["you", "own", "card", "in", "exile"]) {
            return Ok(StaticCond::OwnCardInExile);
        }
        if self.eat_words(&["you", "have"]) {
            let at_least = self.number()?;
            let filter = self.filter()?;
            self.graveyard_cards_tail()?;
            return Ok(StaticCond::GraveyardCards {
                filter,
                at_least,
                wording: crate::ast::GraveyardWording::YouHave,
            });
        }
        if self.eat_words(&["total", "power"]) {
            let n = self.number()?;
            self.expect_words(&["or", "more"])?;
            return Ok(StaticCond::TotalPower(n));
        }
        if self.eat_word("attacking") {
            return Ok(StaticCond::Attacking);
        }
        if self.eat_words(&["attacked", "this", "turn"]) {
            return Ok(StaticCond::AttackedThisTurn);
        }
        if matches!(self.peek(), Some(Tok::Num(_)))
            && self.word_at(1).as_deref() == Some("unlocked")
            && self.word_at(2).as_deref() == Some("doors")
        {
            let n = self.number()?;
            self.pos += 2;
            return Ok(StaticCond::UnlockedDoors(n));
        }
        if self.eat_words(&["attacked", "a", "battle", "this", "turn"]) {
            return Ok(StaticCond::AttackedBattleThisTurn);
        }
        if self.eat_words(&["attacked", "with"]) {
            let f = self.filter()?;
            self.expect_words(&["this", "turn"])?;
            return Ok(StaticCond::AttackedWith(f));
        }
        if self.eat_word("entered") {
            let filter = self.filter()?;
            self.expect_words(&["this", "turn"])?;
            return Ok(StaticCond::EnteredWith { filter, had: false });
        }
        if self.word_at(0).as_deref() == Some("had") && self.word_at(1).as_deref() != Some("no") {
            self.pos += 1;
            let filter = self.filter()?;
            self.expect_words(&["enter", "this", "turn"])?;
            return Ok(StaticCond::EnteredWith { filter, had: true });
        }
        if self.eat_words(&["attacked", "last", "turn"]) {
            return Ok(StaticCond::AttackedLastTurn);
        }
        if self.eat_word("bargained") {
            return Ok(StaticCond::Bargained);
        }
        if self.eat_word("teamwork") {
            return Ok(StaticCond::Teamwork);
        }
        if self.eat_words(&["evidence", "collected"]) {
            return Ok(StaticCond::EvidenceCollected);
        }
        if self.eat_words(&["additional", "cost", "paid"]) {
            return Ok(StaticCond::AdditionalCostPaid);
        }
        if self.eat_word("beheld") {
            return Ok(StaticCond::Beheld);
        }
        if self.eat_words(&["no", "mana", "spent"]) {
            return Ok(StaticCond::NoManaSpent);
        }
        if self.eat_words(&["not", "kicked"]) {
            return Ok(StaticCond::NotKicked);
        }
        if self.eat_word("impending") {
            return Ok(StaticCond::Impending);
        }
        for (i, w) in ["web", "he", "she", "they"].iter().enumerate() {
            let words: &[&str] = if i == 0 {
                &["web", "slung"]
            } else {
                &[w, "web", "slung"]
            };
            if self.eat_words(words) {
                return Ok(StaticCond::WebSlung(u8::try_from(i).unwrap_or(0)));
            }
        }
        for (i, w) in ["sneak", "his", "her"].iter().enumerate() {
            let words: &[&str] = if i == 0 {
                &["sneak", "paid"]
            } else {
                &[w, "sneak", "paid"]
            };
            if self.eat_words(words) {
                return Ok(StaticCond::SneakPaid(u8::try_from(i).unwrap_or(0)));
            }
        }
        if self.eat_word("spent") {
            return self.spent_cond();
        }
        if self.eat_words(&["that", "spell", "spent"]) {
            let mut c = self.spent_cond()?;
            if let StaticCond::ManaSpent { that, .. } = &mut c {
                *that = true;
            }
            return Ok(c);
        }
        if self.eat_word("saddled") {
            return Ok(StaticCond::Saddled);
        }
        if self.eat_words(&["creature", "died", "this", "turn"]) {
            return Ok(StaticCond::CreatureDiedThisTurn);
        }
        if self.eat_words(&["opponent", "damaged", "this", "turn"]) {
            return Ok(StaticCond::OpponentDamagedThisTurn);
        }
        if self.at_words(&["opponent", "dealt", "noncombat"])
            || self.at_words(&["opponent", "was", "dealt", "noncombat"])
        {
            self.pos += 1;
            let was = self.eat_word("was");
            self.expect_words(&["dealt", "noncombat", "damage", "this", "turn"])?;
            return Ok(StaticCond::OpponentNoncombatDamagedThisTurn { was });
        }
        if self.eat_words(&["opponent", "lost", "life", "this", "turn"]) {
            return Ok(StaticCond::OpponentLostLifeThisTurn);
        }
        if self.eat_words(&["permanent", "left", "this", "turn"]) {
            return Ok(StaticCond::PermanentLeftThisTurn);
        }
        if self.eat_words(&["land", "entered", "this", "turn"]) {
            return Ok(StaticCond::LandEnteredThisTurn);
        }
        if self.eat_words(&["artifact", "entered", "this", "turn"]) {
            return Ok(StaticCond::ArtifactEnteredThisTurn);
        }
        if self.eat_words(&["this", "has"]) {
            return self.source_counters_cond();
        }
        if self.eat_words(&["there", "are"]) {
            let n = self.number()?;
            self.expect_words(&["or", "more"])?;
            let kind = self.counter_kind()?;
            self.expect_words(&["on", "this"])?;
            return Ok(StaticCond::SourceCountersThere { kind, n });
        }
        if self.eat_words(&["opponent", "poison"]) {
            let n = self.number()?;
            self.expect_words(&["or", "more"])?;
            return Ok(StaticCond::OpponentPoisonAtLeast(n));
        }
        if self.eat_words(&["player", "life"]) {
            let n = i32::try_from(self.number()?).unwrap_or(0);
            self.expect_words(&["or", "less"])?;
            return Ok(StaticCond::AnyPlayerLifeAtMost(n));
        }
        if self.eat_word("hand") {
            // `hand has no land card`: "you have no land cards in hand".
            if self.eat_words(&["has", "no"]) {
                let f = self.filter()?;
                self.cards_word()?;
                return Ok(StaticCond::NoCardInHand(f));
            }
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
        // "unless at least two other creatures attack" (CR 508.1c).
        if self.eat_words(&["at", "least"]) {
            let at_least = self.number()?;
            self.expect_words(&["other", "creatures"])?;
            let block = self.eat_word("block");
            if !block {
                self.expect_word("attack")?;
            }
            return Ok(StaticCond::OthersInCombat {
                filter: Filter {
                    types: vec![CardType::Creature],
                    ..Default::default()
                },
                at_least,
                also: false,
                block,
            });
        }
        // "unless a black or green creature also attacks".
        {
            let start = self.pos;
            if let Ok(filter) = self.filter()
                && self.eat_word("also")
            {
                let block = self.eat_word("blocks");
                if !block {
                    self.expect_word("attacks")?;
                }
                return Ok(StaticCond::OthersInCombat {
                    filter,
                    at_least: 1,
                    also: true,
                    block,
                });
            }
            self.pos = start;
        }
        if self.eat_words(&["defending", "player", "poisoned"]) {
            return Ok(StaticCond::DefenderPoisoned);
        }
        if self.eat_words(&["you", "control", "more", "creatures", "than"]) {
            let attacking = self.eat_word("attacking");
            if !attacking {
                self.expect_word("defending")?;
            }
            self.expect_word("player")?;
            return Ok(StaticCond::MoreCreaturesThanOpponent { attacking });
        }
        // "any player controls a black permanent": on the battlefield,
        // worded by controller.
        if self.eat_words(&["any", "player", "controls"]) {
            let mut filter = self.filter()?;
            filter.who = Some(Who::Any);
            return Ok(StaticCond::OnBattlefield {
                filter,
                at_least: 1,
            });
        }
        if self.eat_words(&["on", "battlefield"]) {
            let at_least = if matches!(self.peek(), Some(Tok::Num(_))) {
                self.number()?
            } else {
                1
            };
            let filter = self.filter()?;
            return Ok(StaticCond::OnBattlefield { filter, at_least });
        }
        if self.eat_words(&["defending", "player", "controls"]) {
            let at_least = if matches!(self.peek(), Some(Tok::Num(_))) {
                self.number()?
            } else {
                // `defending player controls no Glimmer creature`: 0.
                u32::from(!self.eat_word("no"))
            };
            let filter = self.filter()?;
            return Ok(StaticCond::DefenderControls { filter, at_least });
        }
        if self.eat_words(&["you", "control", "no"]) {
            return Ok(StaticCond::YouControlNone(self.filter()?));
        }
        if self.eat_words(&["you", "control", "at", "most"]) {
            let n = self.number()?;
            let filter = self.filter()?;
            return Ok(StaticCond::YouControlAtMost { filter, n });
        }
        if self.eat_word("enchanted") {
            return Ok(StaticCond::Enchanted);
        }
        if self.eat_word("equipped") {
            return Ok(StaticCond::Equipped);
        }
        if self.eat_word("monstrous") {
            return Ok(StaticCond::Monstrous);
        }
        if self.eat_word("suspected") {
            return Ok(StaticCond::Suspected(true));
        }
        if self.eat_words(&["not", "suspected"]) {
            return Ok(StaticCond::Suspected(false));
        }
        if self.eat_words(&["opponent", "life"]) {
            let n = i32::try_from(self.number()?).unwrap_or(0);
            self.expect_words(&["or", "less"])?;
            return Ok(StaticCond::OpponentLifeAtMost(n));
        }
        if self.eat_words(&["your", "life", "more", "than", "opponent"]) {
            return Ok(StaticCond::MoreLifeThanOpponent);
        }
        if self.eat_words(&["you", "don't", "control"]) {
            return Ok(StaticCond::YouDontControl(self.filter()?));
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
            // "you control an Urza's Mine and an Urza's Power-Plant".
            if at_least == 1 && self.at_word("and") {
                let mut all = vec![filter];
                while self.eat_word("and") {
                    all.push(self.filter()?);
                }
                return Ok(StaticCond::YouControlEach(all));
            }
            // "you control a Desert or there is a Desert card in your
            // graveyard".
            if self.at_words(&["or", "there"]) {
                self.pos += 1;
                let other = self.static_cond()?;
                return Ok(StaticCond::Or(
                    Box::new(StaticCond::YouControl { filter, at_least }),
                    Box::new(other),
                ));
            }
            return Ok(StaticCond::YouControl { filter, at_least });
        }
        let n = self.number()?;
        if self.eat_words(&["cards", "in", "your", "graveyard"]) {
            return Ok(StaticCond::CardsInGraveyard(n));
        }
        if self.eat_words(&["card", "types", "in", "your", "graveyard"]) {
            return Ok(StaticCond::CardTypesInGraveyard(n));
        }
        if self.eat_words(&["mana", "values", "in", "your", "graveyard"]) {
            return Ok(StaticCond::ManaValuesInGraveyard(n));
        }
        if self.eat_words(&["different", "powers"]) {
            return Ok(StaticCond::DifferentPowers(n));
        }
        if self.eat_words(&["basic", "land", "types"]) {
            return Ok(StaticCond::BasicLandTypes(n));
        }
        if self.word_at(0).is_some_and(|w| {
            card_type(&w).is_some()
                    || w.starts_with("non")
                    || color(&w).is_some()
                    // Descend: "four or more permanent cards".
                    || w == "permanent"
        }) || matches!(self.peek(), Some(Tok::Word(w)) if w.chars().next().is_some_and(char::is_uppercase))
        {
            let filter = self.filter()?;
            // "an artifact card is in your graveyard".
            let wording = if n == 1 && self.eat_words(&["card", "is", "in", "your", "graveyard"]) {
                crate::ast::GraveyardWording::IsIn
            } else {
                self.graveyard_cards_tail()?;
                crate::ast::GraveyardWording::ThereAre
            };
            return Ok(StaticCond::GraveyardCards {
                filter,
                at_least: n,
                wording,
            });
        }
        self.unexpected("a condition (threshold, 7 cards in your graveyard, you control 3 artifact, your turn, ...)")
    }

    /// `cards in your graveyard` after a filter (`card` is the filter's).
    fn graveyard_cards_tail(&mut self) -> R<()> {
        if !self.eat_word("cards") {
            self.eat_word("card");
        }
        self.expect_words(&["in", "your", "graveyard"])
    }

    /// A part of the turn (CR 602.5b): `during your upkeep`, `during
    /// opponent upkeep`, `during combat`, `before attackers`, `during your
    /// turn before attackers`, `during your turn`, `during declare
    /// blockers`, `during end of combat`.
    fn turn_time(&mut self) -> Option<StaticCond> {
        use mtg_core::card::TurnTime as T;
        if self.eat_words(&["before", "attackers"]) {
            return Some(StaticCond::During(T::BeforeAttackers));
        }
        if !self.at_word("during") {
            return None;
        }
        let save = self.pos;
        self.pos += 1;
        let t = if self.eat_words(&["your", "upkeep"]) {
            StaticCond::During(T::YourUpkeep)
        } else if self.eat_words(&["opponent", "upkeep"]) {
            StaticCond::During(T::OpponentUpkeep)
        } else if self.eat_words(&["combat", "before", "blockers"]) {
            StaticCond::During(T::BeforeBlockers)
        } else if self.eat_words(&["combat", "after", "blockers"]) {
            StaticCond::During(T::AfterBlockers)
        } else if self.eat_word("combat") {
            StaticCond::During(T::Combat)
        } else if self.eat_words(&["declare", "blockers"]) {
            StaticCond::During(T::DeclareBlockers)
        } else if self.eat_words(&["end", "of", "combat"]) {
            StaticCond::During(T::EndOfCombat)
        } else if self.eat_words(&["your", "main", "phase"]) {
            StaticCond::During(T::YourMain)
        } else if self.eat_words(&["declare", "attackers", "if", "attacked"]) {
            StaticCond::During(T::AttackedThisStep)
        } else if self.eat_words(&["declare", "attackers"]) {
            StaticCond::During(T::DeclareAttackers)
        } else if self.eat_words(&["your", "turn", "before", "attackers"]) {
            StaticCond::During(T::YourTurnBeforeAttackers)
        } else if self.eat_words(&["your", "turn"])
            && (!matches!(self.peek(), Some(Tok::Comma))
                // `during your turn, once:` (an activation restriction).
                || matches!(self.word_at(1).as_deref(), Some("once" | "sorcery")))
        {
            StaticCond::DuringYourTurn
        } else {
            self.pos = save;
            return None;
        };
        Some(t)
    }

    fn enters_outcome(&mut self) -> R<Outcome> {
        if self.eat_word("tapped") {
            return Ok(Outcome::EnterTapped);
        }
        if self.eat_word("prepared") {
            return Ok(Outcome::EnterPrepared);
        }
        if self.eat_word("with") {
            let n = self.num()?;
            let kind = self.counter_kind()?;
            return Ok(Outcome::EnterWithCounters { kind, n });
        }
        self.unexpected("`tapped`, `prepared`, or `with N counters`")
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
                    // `result 1 to 9: <effects>`: a row of the results
                    // table of the roll just before (CR 706.3a).
                    if self.at_word("result") {
                        let row = self.die_row()?;
                        match out.last_mut().and_then(|s| s.effects.last_mut()) {
                            Some(Effect::RollDie { rows, .. }) => rows.push(row),
                            _ => return self.fail("a `result` line needs a `roll` before it"),
                        }
                        self.end_stmt()?;
                        continue;
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

    /// `result 1 to 9: <effects>`, `result 20: ...`, `result 20 or more:
    /// ...`.
    fn die_row(&mut self) -> R<DieRow> {
        self.expect_word("result")?;
        let lo = self.number()?;
        let hi = if self.eat_word("to") {
            Some(self.number()?)
        } else if self.eat_words(&["or", "more"]) {
            None
        } else {
            Some(lo)
        };
        if hi.is_some_and(|h| h < lo) {
            return self.fail("a result range runs low to high");
        }
        self.expect(&Tok::Colon)?;
        let effects = self.sentences_inline()?;
        Ok(DieRow { lo, hi, effects })
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
        // `label "Hellbent" ...`: an ability word (CR 207.2c) before an
        // "instead" sentence.
        if self.at_word("label") {
            self.pos += 1;
            let l = self.string()?;
            let mut s = self.sentence()?;
            return match s.effects.as_mut_slice() {
                [Effect::Instead { label, .. }] if label.is_none() => {
                    *label = Some(l);
                    Ok(s)
                }
                // Any other sentence: printed "Spell mastery — If ...".
                _ if s.label.is_none() => {
                    s.label = Some(l);
                    Ok(s)
                }
                _ => self.fail("two `label`s on one sentence"),
            };
        }
        self.subject = None;
        // `label "Spell mastery" ...`: an ability word before the sentence.
        let label = if self.at_word("label") && matches!(self.peek_at(1), Some(Tok::Str(_))) {
            self.pos += 1;
            Some(self.string()?)
        } else {
            None
        };
        // "Then that player discards a card.": a sentence led by "then".
        let then_first = self.eat_word("then");
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
            } else if self.at_word("or") {
                // `sacrifice 1 artifact chosen or discard 1 card`: one of
                // these, chosen as it resolves.
                self.pos += 1;
                let c = self.clause()?;
                let last = clauses.pop().expect("a first clause");
                clauses.push(match last {
                    Effect::OneOf(mut v) => {
                        v.push(c);
                        Effect::OneOf(v)
                    }
                    other => Effect::OneOf(vec![other, c]),
                });
                continue;
            } else if self.peek() == Some(&Tok::Comma) {
                // `A, B and C`: a list of clauses, when a clause follows.
                let at = self.pos;
                let subject = self.subject;
                let declared = (self.targets.len(), self.groups.len(), self.counts.len());
                self.pos += 1;
                let Ok(c) = self.clause() else {
                    self.pos = at;
                    self.subject = subject;
                    self.targets.truncate(declared.0);
                    self.groups.truncate(declared.1);
                    self.counts.truncate(declared.2);
                    break;
                };
                joins.push(Join::Comma);
                clauses.push(self.unless_suffix(c)?);
                continue;
            } else {
                break;
            }
            let c = self.clause()?;
            clauses.push(self.unless_suffix(c)?);
        }
        // "Until end of turn, whenever ..., <effects>": a delayed trigger
        // that lasts the turn (CR 603.7c), as `whenever ... this turn`.
        if eot_first
            && let Some(Effect::Delayed {
                this_turn,
                trigger_first: true,
                ..
            }) = clauses.first_mut()
        {
            *this_turn = true;
        }
        let s = Sentence {
            effects: clauses,
            label,
            para: false,
            eot_first,
            then_first,
            joins,
        };
        self.condition_suffix(s)
    }

    /// A condition after a whole sentence: `<sentence> instead if <cond>`
    /// (CR 608.2c), or `<sentence> if <cond>` ("Draw a card if ...").
    fn condition_suffix(&mut self, s: Sentence) -> R<Sentence> {
        // `<sentence> instead if <cond>` (CR 608.2c).
        if self.at_words(&["instead", "if"]) {
            self.pos += 2;
            let cond = self.instead_cond()?;
            return Ok(Sentence::new(vec![Effect::Instead {
                cond,
                wording: InsteadWording::After,
                label: None,
                effects: vec![s],
            }]));
        }
        // `<sentence> if target 1 is <filter>`: "Put a +1/+1 counter on
        // target creature you control if it's legendary." (the target as
        // the sentence resolves).
        // `... if that target 1 is ...`: "Draw a card if that creature
        // has a +1/+1 counter on it."
        let that = self.at_words(&["if", "that", "target"])
            && matches!(self.peek_at(3), Some(Tok::Num(_)));
        if that
            || (self.at_words(&["if", "target"]) && matches!(self.peek_at(2), Some(Tok::Num(_))))
        {
            self.pos += if that { 3 } else { 2 };
            let n = self.number()? as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            self.expect_word("is")?;
            let filter = self.filter()?;
            let check = crate::ast::TargetCheck {
                tense: if that {
                    crate::ast::TargetTense::ThatIs
                } else {
                    crate::ast::TargetTense::Is
                },
                also: false,
                after: true,
            };
            return Ok(Sentence::new(vec![Effect::IfTarget(
                n - 1,
                filter,
                vec![s],
                check,
            )]));
        }
        // `<sentence> if <cond>`: "Draw a card if you control ...".
        if self.at_word("if") {
            self.pos += 1;
            let cond = self.static_cond()?;
            return Ok(Sentence::new(vec![Effect::IfCondition(
                cond,
                vec![s],
                crate::ast::IfWording::After,
            )]));
        }
        Ok(s)
    }

    /// `<clause> unless you pay {2}` / `unless they pay 2 life` / `unless
    /// <recipient> pays {2}`.
    fn unless_suffix(&mut self, e: Effect) -> R<Effect> {
        if !self.at_word("unless") {
            return Ok(e);
        }
        // `you lose 1 life unless opponent life 10 or less`: a static
        // condition, judged as it resolves.
        if !matches!(self.word_at(1).as_deref(), Some("you" | "they")) {
            let at = self.pos;
            self.pos += 1;
            if let Ok(cond) = self.static_cond()
                && (self.at_end()
                    || matches!(self.peek(), Some(Tok::Semi | Tok::RBrace | Tok::Newline)))
            {
                return Ok(Effect::IfCondition(
                    cond,
                    vec![Sentence::new(vec![e])],
                    crate::ast::IfWording::Unless,
                ));
            }
            self.pos = at;
        }
        // `unless you sacrifice 1 Forest chosen`: an act, not a payment.
        if self.at_words(&["unless", "you"])
            && !matches!(self.word_at(2).as_deref(), Some("pay" | "do"))
        {
            self.pos += 2;
            let act = self.clause()?;
            return Ok(Effect::UnlessYou {
                act: Box::new(act),
                effect: Box::new(e),
            });
        }
        self.pos += 1;
        // "unless that player discards a card": discarding is the payment.
        let discards = |p: &Self| {
            p.at_words(&["they", "discard"])
                || (p.pos + 1 < p.toks.len()
                    && (2..6).any(|k| {
                        matches!(p.peek_at(k), Some(Tok::Word(w)) if w == "discards")
                            && !(1..k)
                                .any(|j| matches!(p.peek_at(j), Some(Tok::Word(w)) if w == "pays"))
                    }))
        };
        if discards(self) {
            let payer = if self.eat_words(&["they", "discard"]) {
                match self.subject {
                    Some(s) => s,
                    None => {
                        return self.fail("`unless they discard` needs a clause with a subject");
                    }
                }
            } else {
                let r = self.recipient()?;
                self.expect_word("discards")?;
                r
            };
            let discard = self.number()?;
            if !self.eat_word("cards") {
                self.expect_word("card")?;
            }
            return Ok(Effect::UnlessPays {
                payer,
                mana: ManaCost::default(),
                life: 0,
                discard,
                effect: Box::new(e),
            });
        }
        let payer = if self.eat_words(&["you", "pay"]) {
            Recipient::You
        } else if self.eat_words(&["they", "pay"]) {
            match self.subject {
                Some(s) => s,
                None => return self.fail("`unless they pay` needs a clause with a subject"),
            }
        } else if self.eat_words(&["any", "player", "pays"]) {
            // "unless any player pays {2}": each player may.
            Recipient::EachPlayer
        } else {
            let r = self.recipient()?;
            self.expect_word("pays")?;
            r
        };
        let (mana, mut life) = self.pay_amount()?;
        // "unless that player pays {1} or 1 life": either.
        if life == 0 && self.at_word("or") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            life = self.number()?;
            self.expect_word("life")?;
        }
        Ok(Effect::UnlessPays {
            payer,
            mana,
            life,
            discard: 0,
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
        if self.at_many() {
            return true;
        }
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
        if w == "it"
            || w == "target"
            || w == "another"
            || self.at_words(&["creature", "dealt", "damage", "this", "way"])
        {
            return true;
        }
        if w == "targets" && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            return true;
        }
        if self.at_words(&["those", "targets"]) {
            return true;
        }
        if self.group_named && self.at_those().is_some() {
            return true;
        }
        if self.at_words(&["controller", "of", "that", "target"]) {
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
        if self.at_words(&["triggering", "card"])
            || self.at_words(&["controller", "of", "that", "creature"])
            || self.at_words(&["controller", "of", "enchanted"])
        {
            return true;
        }
        if w == "any" && self.word_at(1).as_deref() == Some("target") {
            return true;
        }
        if w == "that" && matches!(self.word_at(1).as_deref(), Some("card" | "target")) {
            return true;
        }
        if w == "each" || w == "all" || w == "every" {
            return true;
        }
        RECIPIENTS.iter().any(|(ws, _)| self.at_words(ws))
    }

    /// Verbs that take the previous clause's subject when none is written
    /// ("you draw a card and lose 1 life").
    fn at_subject_verb(&self) -> bool {
        if self.revealed && self.at_word("puts") {
            return true;
        }
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
                    | "can"
                    | "must"
                    | "blocks"
                    | "attacks"
                    | "attack"
                    | "sacrifices"
                    | "sacrifice"
                    | "reveals"
                    | "reveal"
                    | "returns"
            )
        )
    }

    /// Whether the cards revealed before are named here: `it`, `that
    /// card`, `all revealed`, `those revealed`, `the revealed`, `all other
    /// revealed`.
    fn at_card_words(&self) -> bool {
        self.at_word("it")
            || self.at_words(&["that", "card"])
            || self.at_words(&["all", "revealed"])
            || self.at_words(&["all", "other", "revealed"])
            || self.at_words(&["those", "revealed"])
            || self.at_words(&["the", "revealed"])
    }

    /// `put it into hand` (after `put`): the cards revealed before and
    /// where they go.
    fn put_card(&mut self, who: Option<Recipient>) -> R<Effect> {
        use crate::ast::CardWords;
        use mtg_core::card::CardDest;
        let what = if self.eat_word("it") {
            CardWords::It
        } else if self.eat_words(&["that", "card"]) {
            CardWords::ThatCard
        } else if self.eat_words(&["all", "revealed"]) {
            CardWords::AllRevealed
        } else if self.eat_words(&["all", "other", "revealed"]) {
            CardWords::OtherRevealed
        } else if self.eat_words(&["those", "revealed"]) {
            CardWords::Those
        } else {
            self.expect_words(&["the", "revealed"])?;
            CardWords::TheRevealed
        };
        let mut that_player = false;
        let to = if self.eat_words(&["into", "hand"]) {
            CardDest::Hand
        } else if self.eat_words(&["into", "graveyard"]) {
            that_player = self.eat_words(&["of", "that", "player"]);
            CardDest::Graveyard
        } else if self.eat_words(&["onto", "battlefield"]) {
            CardDest::Battlefield {
                tapped: self.eat_word("tapped"),
            }
        } else {
            self.expect_words(&["on", "bottom", "of", "library"])?;
            CardDest::LibraryBottom
        };
        let random = to == CardDest::LibraryBottom && self.eat_word("random");
        // `and the rest on bottom of library random`.
        let rest = if self.eat_words(&["and", "the", "rest"]) {
            let dest = if self.eat_words(&["into", "hand"]) {
                CardDest::Hand
            } else if self.eat_words(&["into", "graveyard"]) {
                CardDest::Graveyard
            } else {
                self.expect_words(&["on", "bottom", "of", "library"])?;
                CardDest::LibraryBottom
            };
            Some((
                dest,
                dest == CardDest::LibraryBottom && self.eat_word("random"),
            ))
        } else {
            None
        };
        Ok(Effect::PutCard {
            what,
            to,
            who,
            that_player,
            random,
            rest,
        })
    }

    /// `if it is land card, <sentence>` / `if it isnt ...` / `if that card
    /// is ...`: the card revealed before.
    fn if_card(&mut self) -> R<Effect> {
        self.pos += 1;
        let that = !self.eat_word("it");
        if that {
            self.expect_words(&["that", "card"])?;
        }
        if !that && self.eat_words(&["shares", "creature", "type", "with", "this"]) {
            self.expect(&Tok::Comma)?;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfCard {
                filter: Filter::default(),
                negated: false,
                card: false,
                that: false,
                shares_type: true,
                then: vec![s],
            });
        }
        let negated = if self.eat_word("isnt") {
            true
        } else {
            self.expect_word("is")?;
            false
        };
        let filter = self.filter()?;
        let card = self.eat_word("card");
        self.expect(&Tok::Comma)?;
        let saved = self.subject.take();
        let s = self.sentence()?;
        self.subject = saved;
        Ok(Effect::IfCard {
            filter,
            negated,
            card,
            that,
            shares_type: false,
            then: vec![s],
        })
    }

    /// `if target 1 was Human, ...`: "If that creature was a Human, ...",
    /// judged on the target's last state.
    fn if_target(&mut self) -> R<Effect> {
        self.pos += 1;
        let that = self.eat_word("that");
        self.pos += 1;
        let n = self.number()? as usize;
        if n == 0 || n > self.targets.len() {
            return self.fail(format!("target {n} has not been declared"));
        }
        if self.eat_word("is") {
            // `if target 1 is Human, ... instead`: an "instead"; otherwise
            // the target as it resolves ("If it's a Vampire, it also gains
            // lifelink until end of turn").
            let filter = self.filter()?;
            self.expect(&Tok::Comma)?;
            let tense = if that {
                crate::ast::TargetTense::ThatIs
            } else {
                crate::ast::TargetTense::Is
            };
            if self.eat_word("also") {
                let saved = self.subject.take();
                let s = self.sentence()?;
                self.subject = saved;
                let check = crate::ast::TargetCheck {
                    tense,
                    also: true,
                    after: false,
                };
                return Ok(Effect::IfTarget(n - 1, filter, vec![s], check));
            }
            let cond = InsteadCond::Target {
                n: n - 1,
                that,
                filter: filter.clone(),
            };
            return match self.instead_body(cond)? {
                Ok(e) => Ok(e),
                Err(s) => {
                    let check = crate::ast::TargetCheck {
                        tense,
                        also: false,
                        after: false,
                    };
                    Ok(Effect::IfTarget(n - 1, filter, vec![s], check))
                }
            };
        }
        let tense = if that {
            self.expect_words(&["was"])?;
            crate::ast::TargetTense::ThatWas
        } else if self.eat_word("had") {
            crate::ast::TargetTense::Had
        } else {
            self.expect_words(&["was"])?;
            crate::ast::TargetTense::Was
        };
        let f = self.filter()?;
        self.expect(&Tok::Comma)?;
        let saved = self.subject.take();
        let s = self.sentence()?;
        self.subject = saved;
        Ok(Effect::IfTarget(
            n - 1,
            f,
            vec![s],
            crate::ast::TargetCheck {
                tense,
                also: false,
                after: false,
            },
        ))
    }

    /// Whether an `if` here is a "this way" condition: `this way`, or
    /// `the sacrificed creature was ...`, before the comma.
    fn this_way_ahead(&self) -> bool {
        if self.word_at(1).as_deref() == Some("the")
            && self
                .word_at(2)
                .is_some_and(|w| crate::ast::this_way_verb(&w).is_some())
        {
            return true;
        }
        let mut k = 1;
        loop {
            match self.peek_at(k) {
                None | Some(Tok::Comma | Tok::Semi | Tok::Newline) => return false,
                _ => {}
            }
            if self.word_at(k).as_deref() == Some("this")
                && self.word_at(k + 1).as_deref() == Some("way")
            {
                return true;
            }
            k += 1;
        }
    }

    /// After `if`: `<filter> [card] was <verb> this way`, `<filter> is
    /// <verb> this way`, `you discarded <filter> card this way`, `you put
    /// <filter> card into hand this way`, `the sacrificed creature was[n't]
    /// <filter> [card]`.
    fn this_way_cond(&mut self) -> R<crate::ast::ThisWayCond> {
        use crate::ast::{ThisWayCond, ThisWayWording, this_way_verb};
        let verb_at = |p: &Self| p.word_at(0).and_then(|w| this_way_verb(&w));
        if self.eat_word("the") {
            let Some(verb) = verb_at(self) else {
                return self.unexpected("`sacrificed`, `discarded`, ...");
            };
            self.pos += 1;
            let noun = self.word()?;
            let negated = if self.eat_word("wasn't") {
                true
            } else {
                self.expect_word("was")?;
                false
            };
            let (filter, card) = self.this_way_what()?;
            return Ok(ThisWayCond {
                verb,
                filter,
                card,
                negated,
                wording: ThisWayWording::The(noun),
            });
        }
        if self.eat_word("you") {
            let verb = if self.eat_word("put") {
                mtg_core::card::ThisWay::PutInHand
            } else {
                let Some(v) = verb_at(self) else {
                    return self.unexpected("`discarded` or `put`");
                };
                self.pos += 1;
                v
            };
            let (filter, card) = self.this_way_what()?;
            if verb == mtg_core::card::ThisWay::PutInHand {
                self.expect_words(&["into", "hand"])?;
            }
            self.expect_words(&["this", "way"])?;
            return Ok(ThisWayCond {
                verb,
                filter,
                card,
                negated: false,
                wording: ThisWayWording::You,
            });
        }
        let (filter, card) = self.this_way_what()?;
        let wording = if self.eat_word("is") {
            ThisWayWording::Is
        } else {
            self.expect_word("was")?;
            ThisWayWording::Was
        };
        let Some(verb) = verb_at(self) else {
            return self.unexpected("`exiled`, `sacrificed`, ...");
        };
        self.pos += 1;
        self.expect_words(&["this", "way"])?;
        Ok(ThisWayCond {
            verb,
            filter,
            card,
            negated: false,
            wording,
        })
    }

    /// `card`, `<filter> card`, or `<filter>`.
    fn this_way_what(&mut self) -> R<(Option<Filter>, bool)> {
        if self.eat_word("card") {
            return Ok((None, true));
        }
        let f = self.filter()?;
        let card = self.eat_word("card");
        Ok((Some(f), card))
    }

    /// After `if <condition>,`: `instead <sentence>` or `<sentence>
    /// instead` (CR 608.2c), or else the plain sentence (`Err`).
    fn instead_body(&mut self, cond: InsteadCond) -> R<Result<Effect, Sentence>> {
        let lead = self.eat_word("instead");
        let saved = self.subject.take();
        let s = self.sentence()?;
        self.subject = saved;
        let wording = if lead {
            InsteadWording::Lead
        } else if self.eat_word("instead") {
            InsteadWording::Trail
        } else {
            return Ok(Err(s));
        };
        Ok(Ok(Effect::Instead {
            cond,
            wording,
            label: None,
            effects: vec![s],
        }))
    }

    /// The condition of `... instead if <cond>`: `kicked`, `target 1 is
    /// Human`, `that target 1 is Human`, or a static condition.
    fn instead_cond(&mut self) -> R<InsteadCond> {
        if self.eat_word("kicked") {
            return Ok(InsteadCond::Kicked);
        }
        if self.at_word("the")
            && self
                .word_at(1)
                .is_some_and(|w| crate::ast::this_way_verb(&w).is_some())
        {
            return Ok(InsteadCond::ThisWay(self.this_way_cond()?));
        }
        if self.at_word("target") || self.at_words(&["that", "target"]) {
            let that = self.eat_word("that");
            self.pos += 1;
            let n = self.number()? as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            self.expect_word("is")?;
            let filter = self.filter()?;
            return Ok(InsteadCond::Target {
                n: n - 1,
                that,
                filter,
            });
        }
        Ok(InsteadCond::Static(self.static_cond()?))
    }

    /// After `if kicked`: "If this spell was kicked, ..." (CR 702.33c).
    fn if_kicked(&mut self) -> R<Effect> {
        if self.peek() == Some(&Tok::Comma) {
            self.pos += 1;
        }
        // "If this spell was kicked, that creature also gains hexproof
        // until end of turn."
        if self.eat_word("also") {
            let s = self.sentence()?;
            return Ok(Effect::IfCondition(
                StaticCond::Kicked,
                vec![s],
                crate::ast::IfWording::Also,
            ));
        }
        let inner = if self.peek() == Some(&Tok::LBrace) {
            self.effects_body()?
        } else {
            match self.instead_body(InsteadCond::Kicked)? {
                Ok(e) => return Ok(e),
                Err(s) => vec![s],
            }
        };
        Ok(Effect::IfKicked(inner))
    }

    /// "if you control a Treefolk, ...": a static condition. `None` (and
    /// nothing consumed) when what follows `if` is not one.
    fn if_condition(&mut self) -> R<Option<Effect>> {
        let save = self.pos;
        self.pos += 1;
        if let Ok(cond) = self.static_cond()
            && self.peek() == Some(&Tok::Comma)
        {
            self.pos += 1;
            let also = self.eat_word("also");
            let inner = if self.peek() == Some(&Tok::LBrace) {
                self.effects_body()?
            } else if also {
                vec![self.sentence()?]
            } else {
                match self.instead_body(InsteadCond::Static(cond.clone()))? {
                    Ok(e) => return Ok(Some(e)),
                    Err(s) => vec![s],
                }
            };
            let wording = if also {
                crate::ast::IfWording::Also
            } else {
                crate::ast::IfWording::Lead
            };
            return Ok(Some(Effect::IfCondition(cond, inner, wording)));
        }
        self.pos = save;
        Ok(None)
    }

    #[allow(clippy::too_many_lines)] // one branch per clause form
    fn clause(&mut self) -> R<Effect> {
        // Designations (CR 701.54, 725, 726, 701.49).
        if self.at_words(&["the", "ring", "tempts", "you"]) {
            self.pos += 4;
            return Ok(Effect::RingTempts);
        }
        if self.at_words(&["become", "monarch"]) {
            self.pos += 2;
            return Ok(Effect::BecomeMonarch);
        }
        if self.at_words(&["take", "initiative"]) {
            self.pos += 2;
            return Ok(Effect::TakeInitiative);
        }
        if self.at_words(&["venture", "into", "undercity"]) {
            self.pos += 3;
            return Ok(Effect::Venture { undercity: true });
        }
        if self.at_word("venture") {
            self.pos += 1;
            return Ok(Effect::Venture { undercity: false });
        }
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
        // "choose two { mode: ... }": two different modes (CR 700.2d).
        if self.at_words(&["choose", "two"]) && self.peek_at(2) == Some(&Tok::LBrace) {
            self.pos += 2;
            return Ok(Effect::ModesTwo(self.modes_block()?));
        }
        // "when you do, ...": a reflexive trigger (CR 603.12) after an
        // optional action, with targets of its own.
        if self.at_words(&["when", "you", "do"]) {
            return self.when_you_do();
        }
        // "if you do, ...": after an optional action.
        // `if they do`: the same, after "unless its controller pays".
        if self.at_words(&["if", "you", "do"]) || self.at_words(&["if", "they", "do"]) {
            self.pos += 3;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfYouDo(vec![s]));
        }
        // "If you can't, ..." after an action that may do nothing.
        if self.at_words(&["if", "you", "can't"]) {
            self.pos += 3;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfYouDont {
                cant: true,
                then: vec![s],
            });
        }
        // "If you don't, ...": after an optional action that didn't happen.
        if self.at_words(&["if", "you", "don't"]) && self.peek_at(3) == Some(&Tok::Comma) {
            self.pos += 4;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfYouDont {
                cant: false,
                then: vec![s],
            });
        }
        // "Otherwise, ..." after a conditional sentence (CR 608.2c).
        if self.at_word("otherwise") && self.peek_at(1) == Some(&Tok::Comma) {
            self.pos += 2;
            let s = self.sentence()?;
            return Ok(Effect::Otherwise(vec![s]));
        }
        // "If you win the flip, ..." after "flip a coin" (CR 705.2).
        if self.at_words(&["if", "you", "win", "the", "flip"]) {
            self.pos += 5;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfWonFlip(vec![s]));
        }
        // "If you win, ..." after "clash with an opponent" (CR 701.23).
        if self.at_words(&["if", "you", "win"]) && self.peek_at(3) == Some(&Tok::Comma) {
            self.pos += 4;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfWonClash(vec![s]));
        }
        if self.at_words(&["if", "target"]) || self.at_words(&["if", "that", "target"]) {
            return self.if_target();
        }
        // `if you dont, ...` / `if you cant, ...` after an action.
        if (self.at_words(&["if", "you", "dont"]) || self.at_words(&["if", "you", "cant"]))
            && self.peek_at(3) == Some(&Tok::Comma)
        {
            let cant = self.word_at(2).as_deref() == Some("cant");
            self.pos += 4;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfYouDont {
                cant,
                then: vec![s],
            });
        }
        // `if you dont put it into hand, ...` after `if it is ...`.
        if self.revealed && self.at_words(&["if", "you", "dont", "put", "it"]) {
            use mtg_core::card::CardDest;
            self.pos += 5;
            let to = if self.eat_words(&["into", "hand"]) {
                CardDest::Hand
            } else if self.eat_words(&["into", "graveyard"]) {
                CardDest::Graveyard
            } else if self.eat_words(&["onto", "battlefield"]) {
                CardDest::Battlefield { tapped: false }
            } else {
                self.expect_words(&["on", "bottom", "of", "library"])?;
                CardDest::LibraryBottom
            };
            self.expect(&Tok::Comma)?;
            let s = self.sentence()?;
            return Ok(Effect::IfYouDontPut { to, then: vec![s] });
        }
        // `if it is land card, ...` after a card is revealed.
        if self.revealed
            && (self.at_words(&["if", "it"]) || self.at_words(&["if", "that", "card"]))
            && matches!(
                self.word_at(if self.word_at(1).as_deref() == Some("it") {
                    2
                } else {
                    3
                })
                .as_deref(),
                Some("is" | "isnt" | "shares")
            )
        {
            return self.if_card();
        }
        // `if you controlled target 1, ...`: "If you controlled that
        // permanent, ...", its controller as it last existed (CR 608.2h).
        if self.at_words(&["if", "you", "controlled", "target"]) {
            self.pos += 4;
            let n = self.number()? as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            self.expect(&Tok::Comma)?;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfTarget(
                n - 1,
                Filter::controlled_by_you(),
                vec![s],
                crate::ast::TargetCheck::default(),
            ));
        }
        // "If excess damage was dealt this way, ..." (CR 120.4a).
        if self.at_words(&["if", "excess", "damage", "dealt", "this", "way"]) {
            self.pos += 6;
            self.expect(&Tok::Comma)?;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfExcessDamage(vec![s]));
        }
        if self.at_word("if") && self.this_way_ahead() {
            self.pos += 1;
            let cond = self.this_way_cond()?;
            self.expect(&Tok::Comma)?;
            return match self.instead_body(InsteadCond::ThisWay(cond.clone()))? {
                Ok(e) => Ok(e),
                Err(s) => Ok(Effect::IfThisWay(cond, vec![s])),
            };
        }
        if self.at_word("if")
            && !self.at_words(&["if", "kicked"])
            && let Some(e) = self.if_condition()?
        {
            return Ok(e);
        }
        // "For each opponent who can't, you draw a card."
        if self.at_words(&["for", "each"])
            && matches!(self.word_at(2).as_deref(), Some("opponent" | "player"))
            && self.word_at(3).as_deref() == Some("who")
        {
            self.pos += 2;
            let opponent = self.word()? == "opponent";
            self.expect_words(&["who", "can't"])?;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            let inner = vec![self.sentence()?];
            return Ok(Effect::ForEachCouldnt { opponent, inner });
        }
        // "if kicked, ..." (CR 702.33c).
        if self.at_words(&["if", "kicked"]) {
            self.pos += 2;
            let before = self.targets.len();
            let e = self.if_kicked()?;
            // Not for an "instead" (Blood Beckoning's "instead return two
            // target creature cards"): its own targets replace the others.
            if self.targets.len() > before
                && self.kicked_targets.is_none()
                && !matches!(e, Effect::Instead { .. })
            {
                self.kicked_targets = Some(before);
            }
            // "If this spell was kicked, instead any number of target
            // creatures you control gain indestructible": the kicked spell
            // has these targets in place of the others.
            if self.targets.len() > before
                && before > 0
                && self.kicked_instead.is_none()
                && matches!(e, Effect::Instead { .. })
            {
                self.kicked_instead = Some(before);
            }
            return Ok(e);
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
            if group.is_some() || self.is_many_subject(&subjects) {
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
            // `this creature deals 3 damage to that player`, `it deals 2
            // damage to target creature`: the source named as written.
            "deal" | "deals"
                if matches!(
                    subject,
                    Recipient::Source
                        | Recipient::SourceNamed
                        | Recipient::SourceIt
                        | Recipient::TriggeringIt
                        | Recipient::TriggeringObject
                        | Recipient::Attached
                ) =>
            {
                self.pos -= 1;
                match self.verb_first()? {
                    Effect::DealDamage { amount, to, .. } => Ok(Effect::DealDamage {
                        amount,
                        to,
                        by: subject,
                    }),
                    _ => self.fail("`deals ... divided` takes no subject"),
                }
            }
            "gain" | "gains" if self.at_word("ability") || self.granted_mechanic_ahead() => {
                let ability = Box::new(if let Some(a) = self.granted_mechanic()? {
                    if let Ability::Protection(filter) = &a
                        && self.eat_words(&["or", "from", "color", "of", "choice"])
                    {
                        self.until_end_of_turn();
                        return Ok(Effect::ProtectionOr {
                            to: subject,
                            filter: filter.clone(),
                        });
                    }
                    a
                } else {
                    self.pos += 1;
                    self.nested_ability()?
                });
                self.until_end_of_turn();
                Ok(Effect::GrantAbility {
                    ability,
                    to: subject,
                })
            }
            "gain" | "gains" if self.at_word("flashback") => {
                self.pos += 1;
                let that_card = self.eat_words(&["that", "card"]);
                self.until_end_of_turn();
                Ok(Effect::GrantFlashback {
                    to: subject,
                    that_card,
                })
            }
            "gain" | "gains" => {
                if self.at_word("control") {
                    self.pos += 1;
                    self.expect_word("of")?;
                    let of = self.recipient()?;
                    self.until_end_of_turn();
                    return Ok(Effect::GainControl { of, forever: false });
                }
                if self.eat_words(&["choice", "of"]) {
                    let mut keywords = Vec::new();
                    loop {
                        let Some(k) = self.keyword() else {
                            return self.unexpected("a keyword");
                        };
                        keywords.push(k);
                        if !self.eat_word("or") {
                            break;
                        }
                    }
                    self.until_end_of_turn();
                    return Ok(Effect::GrantChoice {
                        to: subject,
                        keywords,
                    });
                }
                if self.eat_words(&["all", "creature", "types"]) {
                    self.until_end_of_turn();
                    return Ok(Effect::CreatureTypes {
                        to: subject,
                        all: true,
                    });
                }
                if self.eat_words(&["protection", "from", "chosen", "color"]) {
                    self.until_end_of_turn();
                    return Ok(Effect::ProtectionFromChosen { to: subject });
                }
                if self.eat_words(&["protection", "from", "color", "of", "choice"]) {
                    self.until_end_of_turn();
                    return Ok(Effect::OfChoice {
                        to: subject,
                        kind: ChoiceKind::Color { except: None },
                        protection: true,
                        in_addition: false,
                    });
                }
                if let Some(k) = self.keyword() {
                    let mut keywords = vec![k];
                    loop {
                        if self.peek() == Some(&Tok::Comma) {
                            self.pos += 1;
                        }
                        // "gains vigilance and gets +X/+X": the `and` joins
                        // the next clause instead.
                        if self.at_word("and") {
                            self.pos += 1;
                            if self.keyword_ahead() {
                                keywords.push(self.keyword().expect("keyword ahead"));
                                continue;
                            }
                            self.pos -= 1;
                            break;
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
            "sacrifices" | "sacrifice"
                if self.at_word("that")
                    || self.at_word("it")
                    || self.at_word("this")
                    || self.at_word("attached") =>
            {
                let what = self.recipient()?;
                Ok(Effect::SacrificeBy { who: subject, what })
            }
            "sacrifices" | "sacrifice" => {
                let count = if self.eat_word("a") || self.eat_word("an") {
                    Num::N(1)
                } else {
                    self.num()?
                };
                let mut filter = self.filter()?;
                let of_choice = self.eat_words(&["of", "their", "choice"]);
                // "a creature of their choice with flying".
                let mut choice_first = false;
                if of_choice && self.at_word("with") {
                    self.pos += 1;
                    let Some(k) = self.keyword() else {
                        return self.unexpected("a keyword after `with`");
                    };
                    filter.keywords.push(k);
                    while self.eat_word("and") {
                        let Some(k) = self.keyword() else {
                            return self.unexpected("a keyword after `and`");
                        };
                        filter.keywords.push(k);
                    }
                    choice_first = true;
                }
                // "an artifact and a land of their choice".
                let and = if self.at_word("and")
                    && (matches!(self.peek_at(1), Some(Tok::Num(_)))
                        || matches!(self.word_at(1).as_deref(), Some("a" | "an")))
                {
                    self.pos += 1;
                    let n = if self.eat_word("a") || self.eat_word("an") {
                        Num::N(1)
                    } else {
                        self.num()?
                    };
                    let f = self.filter()?;
                    self.eat_words(&["of", "their", "choice"]);
                    Some(Box::new((n, f)))
                } else {
                    None
                };
                Ok(Effect::SacrificeChosen {
                    who: subject,
                    count,
                    filter,
                    and,
                    choice_first,
                })
            }
            // "That player looks at the top three cards of your library,
            // then puts them back in any order."
            "looks" if self.at_words(&["at", "top"]) => {
                self.pos += 2;
                self.look_at_top(Some(subject))
            }
            "reveal" | "reveals" if self.at_words(&["top", "card"]) => {
                self.pos += 2;
                self.revealed = true;
                Ok(Effect::RevealTop {
                    who: Some(subject),
                    look: false,
                    of: false,
                })
            }
            // "Then that player shuffles" (CR 701.24).
            "shuffle" | "shuffles" if !matches!(self.peek(), Some(Tok::Word(_))) => {
                Ok(Effect::Shuffle { who: subject })
            }
            "reveal" | "reveals" if self.at_word("until") => {
                self.pos += 1;
                self.revealed = true;
                let filter = self.filter()?;
                self.expect_word("card")?;
                Ok(Effect::RevealUntil {
                    who: Some(subject),
                    filter,
                })
            }
            "put" | "puts" if self.revealed && self.at_card_words() => self.put_card(Some(subject)),
            // "reveals a card at random from their hand".
            "reveal" | "reveals"
                if matches!(self.peek(), Some(Tok::Num(1)))
                    && self.word_at(1).as_deref() == Some("card") =>
            {
                self.pos += 1;
                self.expect_words(&["card", "at", "random", "from", "hand"])?;
                Ok(Effect::RevealRandom { who: subject })
            }
            "reveal" | "reveals" => {
                self.eat_word("their");
                self.expect_word("hand")?;
                if self.eat_word("discarding") {
                    return self.discard_chosen(subject, false);
                }
                // "You choose a nonland card from that player's graveyard
                // or hand and exile it."
                if self.eat_word("exiling") {
                    let filter = if self.at_word("card") {
                        Filter::default()
                    } else {
                        self.filter()?
                    };
                    self.expect_word("card")?;
                    // "You choose a nonland card from it and exile that
                    // card."
                    if !self.at_word("from") {
                        let joined = !self.eat_word("split");
                        return Ok(Effect::DiscardChosen {
                            who: subject,
                            filter: (filter != Filter::default()).then_some(filter),
                            may: false,
                            look: false,
                            exile: Some(joined),
                        });
                    }
                    self.expect_words(&["from", "graveyard", "or", "hand"])?;
                    return Ok(Effect::ExileFromGraveyardOrHand {
                        who: subject,
                        filter,
                    });
                }
                Ok(Effect::RevealHand { who: subject })
            }
            // "loses all abilities and becomes a green Snake with base
            // power and toughness 1/1" (Snakeform): one effect.
            "lose" | "loses" if self.eat_words(&["all", "creature", "types"]) => {
                self.until_end_of_turn();
                Ok(Effect::CreatureTypes {
                    to: subject,
                    all: false,
                })
            }
            "lose" | "loses" if self.at_words(&["all", "abilities", "and"]) => {
                self.pos += 3;
                // "loses all abilities and has base power and toughness
                // 0/1" (Humble): one effect, layers 6 and 7b.
                if self.at_words(&["has", "base"]) || self.at_words(&["have", "base"]) {
                    let Effect::SetBasePt {
                        to,
                        power,
                        toughness,
                    } = self.subject_verb(subject, explicit)?
                    else {
                        return self.fail("expected `has base power and toughness N/N`");
                    };
                    return Ok(Effect::Animate {
                        to,
                        types: Vec::new(),
                        subtypes: Vec::new(),
                        pt: Some((power, toughness)),
                        base: true,
                        colors: Vec::new(),
                        keywords: Vec::new(),
                        indefinite: false,
                        loses_abilities: true,
                        in_addition: false,
                    });
                }
                if !(self.at_word("becomes") || self.at_word("become")) {
                    return self.unexpected("`becomes` after `loses all abilities and`");
                }
                match self.subject_verb(subject, explicit)? {
                    Effect::Animate {
                        to,
                        types,
                        subtypes,
                        pt,
                        base,
                        colors,
                        keywords,
                        indefinite,
                        in_addition,
                        ..
                    } => Ok(Effect::Animate {
                        to,
                        types,
                        subtypes,
                        pt,
                        base,
                        colors,
                        keywords,
                        indefinite,
                        loses_abilities: true,
                        in_addition,
                    }),
                    _ => self.fail("`loses all abilities and becomes` takes what it becomes"),
                }
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
            // "Each opponent blights 1" (CR 701.68a).
            "blights" => Ok(Effect::BlightBy {
                who: subject,
                n: self.number()?,
            }),
            // "All suspected creatures are no longer suspected." (CR
            // 701.60d).
            "no" if self.eat_words(&["longer", "suspected"]) => Ok(Effect::Suspect {
                what: subject,
                on: false,
            }),
            "put" | "puts"
                if matches!(self.peek(), Some(Tok::Num(_))) && self.hand_to_library_ahead() =>
            {
                let count = self.num()?;
                let bottom = self.hand_to_library_tail()?;
                Ok(Effect::HandToLibrary {
                    count,
                    who: (explicit || subject != Recipient::You).then_some(subject),
                    bottom,
                    shuffle: false,
                })
            }
            "shuffle" | "shuffles" if self.shuffle_zone_ahead() => {
                self.shuffle_zone((explicit || subject != Recipient::You).then_some(subject))
            }
            // "Each player puts a creature card from their graveyard onto
            // the battlefield."
            "puts"
                if matches!(self.peek(), Some(Tok::Num(1)))
                    && (2..6).any(|k| {
                        self.word_at(k).as_deref() == Some("from")
                            && self.word_at(k + 1).as_deref() == Some("graveyard")
                    }) =>
            {
                self.pos += 1;
                let filter = self.filter()?;
                self.expect_word("card")?;
                self.expect_words(&["from", "graveyard", "onto", "battlefield"])?;
                Ok(Effect::EachPutsFromGraveyard {
                    who: subject,
                    filter,
                })
            }
            "discard" | "discards" => {
                if let Some(e) = self.discard_all(Some(subject))? {
                    return Ok(e);
                }
                // "Then that player discards another card at random".
                if self.eat_words(&["another", "card", "at", "random"]) {
                    return Ok(Effect::DiscardRandom {
                        count: Num::N(1),
                        who: Some(subject),
                        another: true,
                    });
                }
                let count = self.num()?;
                self.cards_word()?;
                if let Some(filter) = self.discard_unless()? {
                    return Ok(Effect::DiscardUnless {
                        who: Some(subject),
                        count,
                        filter,
                    });
                }
                if self.eat_words(&["at", "random"]) {
                    return Ok(Effect::DiscardRandom {
                        count,
                        who: Some(subject),
                        another: false,
                    });
                }
                Ok(Effect::Discard {
                    count,
                    who: (explicit || subject != Recipient::You).then_some(subject),
                })
            }
            "investigates" => Ok(Effect::Investigates(subject)),
            // "Each player returns a creature they control to its owner's
            // hand": each chooses one of their own.
            "returns" => {
                let count = self.num()?;
                let filter = self.filter()?;
                Ok(Effect::ReturnChosen {
                    count,
                    filter,
                    who: Some(subject),
                })
            }
            // "Each player may scry 1", "each opponent scries 1".
            "may" if self.at_word("scry") => {
                self.pos += 1;
                let count = self.num()?;
                Ok(Effect::ScryEach {
                    who: subject,
                    count,
                    may: true,
                })
            }
            // "..., you recruit" (CR 701.70a).
            "recruit" if subject == Recipient::You => Ok(Effect::Recruit { you: true }),
            // "... and you scry 2": an explicit "you".
            "scries" | "scry" if w == "scries" || subject == Recipient::You => {
                let count = self.num()?;
                Ok(Effect::ScryEach {
                    who: subject,
                    count,
                    may: false,
                })
            }
            // "That player may pay {4}", "that player may discard a card
            // at random": the player decides and acts (`PlayerMay`).
            "may"
                if subject != Recipient::You
                    && (self.at_word("pay") || self.at_word("discard")) =>
            {
                let saved = self.subject.take();
                let s = self.sentence()?;
                self.subject = saved;
                Ok(Effect::PlayerMay {
                    who: subject,
                    inner: vec![s],
                })
            }
            // "Its controller may search their library for ...": the
            // player searches their own library (CR 701.19).
            "may" | "searches" if subject != Recipient::You => {
                let may = w == "may";
                if !may {
                    self.pos -= 1;
                }
                if !self.at_word("search") && !self.at_word("searches") {
                    return self.unexpected("`search` after `may`");
                }
                match self.verb_first()? {
                    Effect::Search {
                        filter,
                        count,
                        up_to,
                        to,
                        reveal,
                        that_card,
                        or_card,
                        ..
                    } => Ok(Effect::Search {
                        filter,
                        count,
                        up_to,
                        to,
                        reveal,
                        that_card,
                        or_card,
                        who: Some(subject),
                        may,
                    }),
                    _ => self.fail("expected a search"),
                }
            }
            // "Target opponent exiles two cards from their hand", "exiles
            // a creature they control".
            "exiles" if self.at_word("until") => {
                self.pos += 1;
                self.revealed = true;
                let filter = self.filter()?;
                self.expect_word("card")?;
                Ok(Effect::ExileUntilCard {
                    who: Some(subject),
                    filter,
                })
            }
            "exiles" if self.at_word("top") => {
                self.pos += 1;
                let count = self.num()?;
                self.cards_word()?;
                Ok(Effect::ExileTop {
                    count,
                    who: Some(subject),
                })
            }
            "exiles" => {
                let count = if self.eat_word("a") || self.eat_word("an") {
                    Num::N(1)
                } else {
                    self.num()?
                };
                if (self.at_word("card") || self.at_word("cards"))
                    && self.word_at(1).as_deref() == Some("from")
                {
                    self.pos += 2;
                    let graveyard = self.eat_word("graveyard");
                    if !graveyard {
                        self.expect_word("hand")?;
                    }
                    return Ok(Effect::ExileFromHand {
                        count,
                        who: subject,
                        graveyard,
                    });
                }
                let mut filter = self.filter()?;
                if filter.they_control {
                    filter.they_control = false;
                    filter.who = None;
                } else {
                    self.expect_words(&["they", "control"])?;
                }
                let graveyard = self.eat_words(&["and", "their", "graveyard"]);
                Ok(Effect::ExileChosen {
                    who: subject,
                    count,
                    filter,
                    graveyard,
                })
            }
            "fights" => {
                let b = self.recipient()?;
                Ok(Effect::Fight { a: subject, b })
            }
            "bites" => {
                let twice = self.eat_word("twice");
                let to = if !twice && self.eat_word("itself") {
                    subject
                } else {
                    self.recipient()?
                };
                Ok(Effect::Bite {
                    source: subject,
                    to,
                    twice,
                })
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
            // Blocking requirements and allowances (CR 509.1a, 509.1c):
            // "blocks if able", "must be blocked if able", "can block an
            // additional creature", "can block any number of creatures".
            // "Target creature attacks this turn if able" (CR 508.1d).
            "attacks" | "attack" => {
                let _ = self.eat_words(&["this", "turn"]);
                self.expect_words(&["if", "able"])?;
                self.until_end_of_turn();
                Ok(Effect::Grant {
                    keywords: vec![Keyword::MustAttack],
                    to: subject,
                })
            }
            // "Target creature blocks this creature this turn if able".
            "blocks" | "block"
                if !self.at_word("if")
                    && !self.at_words(&["this", "turn"])
                    && !self.at_words(&["each", "combat"]) =>
            {
                let attacker = self.recipient()?;
                let _ = self.eat_words(&["this", "turn"]);
                self.expect_words(&["if", "able"])?;
                Ok(Effect::BlocksIfAble {
                    blocker: subject,
                    attacker,
                })
            }
            "blocks" | "block" | "must" | "can" => {
                let keyword = match w.as_str() {
                    "must" => {
                        self.expect_words(&["be", "blocked"])?;
                        Keyword::MustBeBlocked
                    }
                    "can" if self.eat_word("attack") => {
                        // CR 702.3b: "can attack this turn as though it
                        // didn't have defender".
                        let _ = self.eat_words(&["this", "turn"]);
                        self.expect_words(&["as", "though"])?;
                        let _ = self.eat_word("it") || self.eat_word("they");
                        self.expect_words(&["didn't", "have", "defender"])?;
                        Keyword::AttackDespiteDefender
                    }
                    "can" => {
                        self.expect_word("block")?;
                        if self.eat_words(&["an", "additional", "creature"]) {
                            Keyword::BlockAdditional
                        } else {
                            self.expect_words(&["any", "number", "of", "creatures"])?;
                            Keyword::BlockAny
                        }
                    }
                    _ => Keyword::MustBlock,
                };
                let _ = self.eat_words(&["this", "turn"]) || self.eat_words(&["each", "combat"]);
                if matches!(keyword, Keyword::MustBlock | Keyword::MustBeBlocked) {
                    self.expect_words(&["if", "able"])?;
                }
                self.until_end_of_turn();
                Ok(Effect::Grant {
                    keywords: vec![keyword],
                    to: subject,
                })
            }
            // "Target creature can't block this creature this turn".
            "can't"
                if self.at_word("block")
                    && matches!(self.word_at(1).as_deref(), Some("this" | "it"))
                    && !self.at_words(&["block", "this", "turn"]) =>
            {
                self.pos += 1;
                let attacker = self.recipient()?;
                let _ = self.eat_words(&["this", "turn"]);
                Ok(Effect::CantBlockThis {
                    blocker: subject,
                    attacker,
                })
            }
            "can't"
                if self.at_words(&["play", "lands"])
                    || self.at_words(&["cast", "creature", "spells"]) =>
            {
                let kind = if self.eat_words(&["play", "lands"]) {
                    mtg_core::card::PlayerRuleKind::CantPlayLands
                } else {
                    self.expect_words(&["cast", "creature", "spells"])?;
                    mtg_core::card::PlayerRuleKind::CantCastCreatureSpells
                };
                self.expect_words(&["this", "turn"])?;
                Ok(Effect::PlayerRule { who: subject, kind })
            }
            // "This creature assigns no combat damage this turn" (CR
            // 510.1a).
            "assigns" if self.at_words(&["no", "combat", "damage", "this", "turn"]) => {
                self.pos += 5;
                Ok(Effect::AssignsNoCombatDamage { to: subject })
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
                } else if self.eat_words(&["be", "regenerated"]) {
                    self.eat_words(&["this", "turn"]);
                    return Ok(Effect::CantBeRegenerated { to: subject });
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
            "endures" => Ok(Effect::Endure {
                what: subject,
                count: self.num()?,
            }),
            "explores" => Ok(Effect::Explore {
                what: subject,
                twice: self.eat_word("twice"),
            }),
            "phases" => {
                self.expect_word("out")?;
                Ok(Effect::PhaseOut { what: subject })
            }
            "connives" => Ok(Effect::Connive { what: subject }),
            "becomes" if self.at_word("prepared") => {
                self.pos += 1;
                Ok(Effect::BecomePrepared { what: subject })
            }
            "skips" | "skip" if self.at_word("next") => {
                use mtg_core::card::SkipWhat;
                self.pos += 1;
                let what = if self.eat_words(&["draw", "step"]) {
                    SkipWhat::DrawStep
                } else {
                    self.expect_words(&["combat", "phase"])?;
                    if self.eat_words(&["this", "turn"]) {
                        SkipWhat::CombatPhaseThisTurn
                    } else {
                        SkipWhat::CombatPhase
                    }
                };
                Ok(Effect::SkipNext { who: subject, what })
            }
            // CR 500.7.
            "take" | "takes"
                if self.at_word("extra") || self.word_at(1).as_deref() == Some("extra") =>
            {
                let n = if self.eat_word("extra") {
                    1
                } else {
                    let n = self.number()?;
                    self.expect_word("extra")?;
                    u8::try_from(n).unwrap_or(u8::MAX)
                };
                if !self.eat_word("turn") {
                    self.expect_word("turns")?;
                }
                Ok(Effect::ExtraTurns { who: subject, n })
            }
            "doesn't" => {
                self.expect_words(&["untap", "next", "untap"])?;
                let steps = self.eat_word("steps");
                if !steps {
                    self.expect_word("step")?;
                }
                Ok(Effect::SkipNextUntap { to: subject, steps })
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
                // "becomes that type": chosen earlier in the resolution.
                if self.eat_words(&["that", "type"]) {
                    self.until_end_of_turn();
                    return Ok(Effect::BecomeChosen { to: subject });
                }
                // "becomes the color of your choice" and kin.
                if self.at_words(&["color", "of", "choice"])
                    || self.at_words(&["colors", "of", "choice"])
                    || self.at_words(&["land", "type", "of", "choice"])
                    || self.at_words(&["creature", "type", "of", "choice"])
                {
                    let kind = self.choice_kind(true)?;
                    let in_addition = self.eat_words(&["in", "addition"]);
                    self.until_end_of_turn();
                    return Ok(Effect::OfChoice {
                        to: subject,
                        kind,
                        protection: false,
                        in_addition,
                    });
                }
                // "becomes all colors" (Scrapbasket).
                if self.eat_words(&["all", "colors"]) {
                    self.until_end_of_turn();
                    return Ok(Effect::BecomeColor {
                        to: subject,
                        colors: Color::ALL.to_vec(),
                        indefinite: self.eat_word("indefinitely"),
                    });
                }
                // "becomes blue", "becomes colorless"; not "becomes a blue
                // Serpent", which `Animate` takes.
                let mut k = 0;
                while self.word_at(k).as_deref().and_then(color).is_some()
                    || (k > 0 && self.word_at(k).as_deref() == Some("and"))
                {
                    k += 1;
                }
                let animates = k > 0
                    && (matches!(self.peek_at(k), Some(Tok::Pt { .. }))
                        || matches!(self.peek_at(k), Some(Tok::Word(w))
                            if card_type(w).is_some() || w.chars().next().is_some_and(char::is_uppercase)));
                if self.at_word("colorless")
                    || (!animates && self.word_at(0).as_deref().and_then(color).is_some())
                {
                    let mut colors = Vec::new();
                    if !self.eat_word("colorless") {
                        while let Some(c) = self.word_at(0).as_deref().and_then(color) {
                            colors.push(c);
                            self.pos += 1;
                            self.eat_and_before_color();
                        }
                    }
                    self.until_end_of_turn();
                    return Ok(Effect::BecomeColor {
                        to: subject,
                        colors,
                        indefinite: self.eat_word("indefinitely"),
                    });
                }
                // "becomes an Island".
                if let Some(Tok::Word(w)) = self.peek()
                    && let Some(land) = basic_land_type(w)
                {
                    self.pos += 1;
                    // "becomes a Plains or an Island": the controller's choice.
                    if self.at_word("or") {
                        let mut lands = vec![land];
                        while self.eat_word("or") {
                            let Some(Tok::Word(w)) = self.next() else {
                                return self.fail("expected a basic land type after `or`");
                            };
                            let Some(l) = basic_land_type(&w) else {
                                return self.fail("expected a basic land type after `or`");
                            };
                            lands.push(l);
                        }
                        self.until_end_of_turn();
                        return Ok(Effect::OfChoice {
                            to: subject,
                            kind: ChoiceKind::LandType(lands),
                            protection: false,
                            in_addition: false,
                        });
                    }
                    self.until_end_of_turn();
                    let indefinite = self.eat_word("indefinitely");
                    return Ok(Effect::BecomeLandType {
                        to: subject,
                        land,
                        indefinite,
                    });
                }
                let mut pt = match self.peek() {
                    Some(&Tok::Pt {
                        power,
                        toughness,
                        signed: false,
                    }) => {
                        self.pos += 1;
                        Some((power, toughness))
                    }
                    _ => None,
                };
                let mut types = Vec::new();
                let mut subtypes = Vec::new();
                let mut colors = Vec::new();
                while let Some(Tok::Word(w)) = self.peek() {
                    if let Some(c) = color(w) {
                        colors.push(c);
                    } else if let Some(t) = card_type(w) {
                        types.push(t);
                    } else if w.chars().next().is_some_and(char::is_uppercase) {
                        subtypes.push(w.clone());
                    } else {
                        break;
                    }
                    self.pos += 1;
                }
                if types.is_empty() && subtypes.is_empty() {
                    return self.fail("expected what `becomes` makes it (`4/4 artifact creature`)");
                }
                // "a Shapeshifter artifact creature with base power and
                // toughness 5/5".
                let mut base = false;
                if pt.is_none() && self.eat_word("base") {
                    let Some(Tok::Pt {
                        power,
                        toughness,
                        signed: false,
                    }) = self.next()
                    else {
                        return self.fail("expected P/T after `base`");
                    };
                    pt = Some((power, toughness));
                    base = true;
                }
                let mut keywords = Vec::new();
                if self.eat_word("with") {
                    loop {
                        let Some(k) = self.keyword() else {
                            return self.fail("expected a keyword after `with`");
                        };
                        keywords.push(k);
                        if self.peek() == Some(&Tok::Comma) {
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                }
                // "a black Demon in addition to its other colors and types".
                let in_addition = self.eat_words(&["in", "addition"]);
                let indefinite = self.eat_word("indefinitely");
                if !indefinite {
                    self.until_end_of_turn();
                }
                Ok(Effect::Animate {
                    to: subject,
                    types,
                    subtypes,
                    pt,
                    base,
                    colors,
                    keywords,
                    indefinite,
                    loses_abilities: false,
                    in_addition,
                })
            }
            // "and you create a 1/1 ... token", "Its controller creates a
            // Treasure token", "each player creates ...".
            "create" | "creates" => {
                self.pos -= 1;
                match self.verb_first()? {
                    Effect::CreateToken {
                        token,
                        count,
                        tapped,
                        it_has,
                        attached,
                        ..
                    } => Ok(Effect::CreateToken {
                        token,
                        count,
                        tapped,
                        who: Some(subject),
                        it_has,
                        attached,
                    }),
                    _ => self.fail("expected a token after `create`"),
                }
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

    /// Whether a chosen action (`tap 1 other untapped creature you
    /// control chosen`) starts here: one of its verbs, with `chosen`
    /// before the clause ends or another verb starts. `return N <filter>
    /// chosen` alone is `ReturnChosen`.
    fn chosen_act_ahead(&self) -> bool {
        const VERBS: [&str; 11] = [
            "tap",
            "untap",
            "sacrifice",
            "discard",
            "exile",
            "return",
            "put",
            "attach",
            "create",
            "draw",
            "destroy",
        ];
        let Some(verb) = self.word_at(0) else {
            return false;
        };
        if !VERBS[..8].contains(&verb.as_str()) {
            return false;
        }
        let mut hand = false;
        let mut k = 1;
        loop {
            match self.peek_at(k) {
                None | Some(Tok::Semi | Tok::Newline) => return false,
                _ => {}
            }
            match self.word_at(k).as_deref() {
                Some("chosen") => {
                    return verb != "return" || hand || self.word_at(1).as_deref() == Some("up");
                }
                // `return 1 Zombie card from your graveyard to hand at
                // random`.
                Some("random") if verb == "return" && hand => return true,
                Some("hand") => hand = true,
                Some(w) if VERBS.contains(&w) => return false,
                _ => {}
            }
            k += 1;
        }
    }

    /// `tap [up to] N <filter> chosen`, `sacrifice ...`, `discard N
    /// <filter> card chosen`, `exile N <filter> card from your graveyard
    /// chosen`, `return N <filter> card from your graveyard to hand
    /// chosen`, `return up to N <filter> to hand chosen`, `put N <filter>
    /// card from among milled cards into hand chosen` (or `from among
    /// them`).
    #[allow(clippy::too_many_lines)] // one arm per act
    fn chosen_act(&mut self) -> R<Effect> {
        use mtg_core::card::ChooseAct;
        let verb = self.word()?;
        // `discard any number cards chosen`: "discard any number of
        // cards".
        let (up_to, count) = if self.eat_words(&["any", "number"]) {
            (true, Num::N(crate::ast::ANY_NUMBER))
        } else {
            (self.eat_words(&["up", "to"]), self.num()?)
        };
        let card_filter = |p: &mut Self| -> R<Filter> {
            let f = if p.at_word("card") || p.at_word("cards") {
                Filter::default()
            } else {
                p.filter()?
            };
            p.cards_word()?;
            Ok(f)
        };
        let mut among_them = false;
        let mut among_those = false;
        let mut this_way = false;
        let mut returns = false;
        let (act, filter) = match verb.as_str() {
            "tap" => (ChooseAct::Tap, self.filter()?),
            "untap" => (ChooseAct::Untap, self.filter()?),
            "sacrifice" => (ChooseAct::Sacrifice, self.filter()?),
            "discard" => (ChooseAct::Discard, card_filter(self)?),
            // `attach 1 Equipment you control to target 1 chosen`.
            "attach" => {
                let f = self.filter()?;
                self.expect_words(&["to", "target"])?;
                let Some(Tok::Num(n)) = self.next() else {
                    return self.fail("expected a target number after `to target`");
                };
                let n = u8::try_from(n.saturating_sub(1)).unwrap_or(0);
                (ChooseAct::AttachToTarget(n), f)
            }
            "exile" => {
                let f = card_filter(self)?;
                self.expect_words(&["from", "your", "graveyard"])?;
                (ChooseAct::ExileFromGraveyard, f)
            }
            "return" => {
                let f = if self.at_word("card") || self.at_word("cards") {
                    Filter::default()
                } else {
                    self.filter()?
                };
                if self.eat_word("card") || self.eat_word("cards") {
                    if self.eat_words(&["from", "among", "them", "to", "hand"]) {
                        // "return up to two permanent cards from among them
                        // to your hand" after a mill.
                        among_them = true;
                        returns = true;
                        (ChooseAct::MilledToHand, f)
                    } else {
                        self.expect_words(&["from", "your", "graveyard", "to", "hand"])?;
                        (ChooseAct::ReturnFromGraveyard, f)
                    }
                } else {
                    self.expect_words(&["to", "hand"])?;
                    (ChooseAct::Bounce, f)
                }
            }
            // `put 1 card from hand on bottom of library chosen`.
            "put"
                if {
                    let at = self.pos;
                    let hand = card_filter(self).is_ok()
                        && self.at_words(&["from", "hand", "on", "bottom", "of", "library"]);
                    self.pos = at;
                    hand
                } =>
            {
                let f = card_filter(self)?;
                self.pos += 6;
                (ChooseAct::HandToBottom, f)
            }
            // `put 1 instant or sorcery card from your graveyard on top of
            // library chosen`.
            "put"
                if {
                    let at = self.pos;
                    let gy = card_filter(self).is_ok()
                        && self.at_words(&[
                            "from",
                            "your",
                            "graveyard",
                            "on",
                            "top",
                            "of",
                            "library",
                        ]);
                    self.pos = at;
                    gy
                } =>
            {
                let f = card_filter(self)?;
                self.pos += 7;
                (ChooseAct::GraveyardToTop, f)
            }
            _ => {
                let f = card_filter(self)?;
                self.expect_words(&["from", "among"])?;
                if self.eat_word("them") {
                    among_them = true;
                } else if self.eat_words(&["those", "cards"]) {
                    among_those = true;
                } else if self.eat_words(&["milled", "this", "way", "cards"]) {
                    this_way = true;
                } else {
                    self.expect_words(&["milled", "cards"])?;
                }
                self.expect_words(&["into", "hand"])?;
                (ChooseAct::MilledToHand, f)
            }
        };
        let act = if act == ChooseAct::ReturnFromGraveyard && self.eat_words(&["at", "random"]) {
            ChooseAct::ReturnRandomFromGraveyard
        } else {
            self.expect_word("chosen")?;
            act
        };
        Ok(Effect::ChooseAct {
            act,
            count,
            up_to,
            filter,
            among_them,
            among_those,
            this_way,
            returns,
        })
    }

    /// How long an impulse permission lasts: `this turn`, `next turn`,
    /// `next end step`, `forever`.
    fn play_until(&mut self) -> R<mtg_core::card::PlayUntil> {
        Ok(if self.eat_words(&["this", "turn"]) {
            mtg_core::card::PlayUntil::ThisTurn
        } else if self.eat_words(&["next", "turn"]) {
            mtg_core::card::PlayUntil::EndOfYourNextTurn
        } else if self.eat_words(&["next", "end", "step"]) {
            mtg_core::card::PlayUntil::YourNextEndStep
        } else if self.eat_word("forever") {
            mtg_core::card::PlayUntil::Forever
        } else {
            return self.unexpected("`this turn`, `next turn`, `next end step`, or `forever`");
        })
    }

    /// `unless you discard <filter> card` / `unless they discard ...` after
    /// a discard: `None` (nothing consumed) if that doesn't follow.
    fn discard_unless(&mut self) -> R<Option<Filter>> {
        if !(self.at_words(&["unless", "you", "discard"])
            || self.at_words(&["unless", "they", "discard"]))
        {
            return Ok(None);
        }
        self.pos += 3;
        let f = self.filter()?;
        self.expect_word("card")?;
        Ok(Some(f))
    }

    /// After `discard`/`discards`: `hand`, `all cards in hand`, `all
    /// <filter> card`; `None` (nothing consumed) otherwise.
    fn discard_all(&mut self, who: Option<Recipient>) -> R<Option<Effect>> {
        if self.eat_word("hand") {
            return Ok(Some(Effect::DiscardAll {
                who,
                filter: None,
                all_cards: false,
            }));
        }
        if !self.eat_word("all") {
            return Ok(None);
        }
        if self.eat_words(&["cards", "in", "hand"]) {
            return Ok(Some(Effect::DiscardAll {
                who,
                filter: None,
                all_cards: true,
            }));
        }
        let f = self.filter()?;
        self.cards_word()?;
        Ok(Some(Effect::DiscardAll {
            who,
            filter: Some(f),
            all_cards: false,
        }))
    }

    /// After `look at top` / `<who> looks at top`: `3 [of <who>] [only]`.
    fn look_at_top(&mut self, by: Option<Recipient>) -> R<Effect> {
        let count = self.num()?;
        let of = if self.eat_word("of") {
            Some(self.recipient()?)
        } else {
            None
        };
        let only = self.eat_word("only");
        Ok(Effect::LookAtTop {
            count,
            of,
            by,
            only,
        })
    }

    /// After `discarding`: `[may] card` or `[may] <filter> card`.
    fn discard_chosen(&mut self, who: Recipient, look: bool) -> R<Effect> {
        let may = self.eat_word("may");
        let filter = if self.eat_word("card") {
            None
        } else {
            let f = self.filter()?;
            self.expect_word("card")?;
            Some(f)
        };
        Ok(Effect::DiscardChosen {
            who,
            filter,
            may,
            look,
            exile: None,
        })
    }

    /// After the count: `cards from hand on ...`.
    fn hand_to_library_ahead(&self) -> bool {
        matches!(self.word_at(1).as_deref(), Some("card" | "cards"))
            && self.word_at(2).as_deref() == Some("from")
            && self.word_at(3).as_deref() == Some("hand")
            && self.word_at(4).as_deref() == Some("on")
    }

    /// After `shuffle` / `shuffles`: `graveyard into`, `hand into`, `cards
    /// from hand`, `all <filter> card from graveyard`, `1 card from hand
    /// into`.
    fn shuffle_zone_ahead(&self) -> bool {
        let w = |k: usize| self.word_at(k);
        let zone = |k: usize| matches!(w(k).as_deref(), Some("graveyard" | "hand"));
        (zone(0) && w(1).as_deref() == Some("into"))
            || (w(0).as_deref() == Some("cards") && w(1).as_deref() == Some("from") && zone(2))
            || (w(0).as_deref() == Some("all")
                && (1..7).any(|k| w(k).as_deref() == Some("from") && zone(k + 1)))
            || (matches!(self.peek(), Some(Tok::Num(_)))
                && matches!(w(1).as_deref(), Some("card" | "cards"))
                && w(2).as_deref() == Some("from")
                && w(3).as_deref() == Some("hand")
                && w(4).as_deref() == Some("into"))
    }

    /// The rest of a `shuffle_zone_ahead` sentence.
    fn shuffle_zone(&mut self, who: Option<Recipient>) -> R<Effect> {
        if matches!(self.peek(), Some(Tok::Num(_))) {
            let count = self.num()?;
            self.cards_word()?;
            self.expect_words(&["from", "hand", "into", "library"])?;
            return Ok(Effect::HandToLibrary {
                count,
                who,
                bottom: false,
                shuffle: true,
            });
        }
        let mut filter = None;
        let mut cards = false;
        if self.eat_word("all") {
            filter = Some(self.filter()?);
            self.expect_word("card")?;
            self.expect_word("from")?;
        } else if self.eat_word("cards") {
            self.expect_word("from")?;
            cards = true;
        }
        let hand = match self.word()?.as_str() {
            "hand" => true,
            "graveyard" => false,
            _ => return self.unexpected("`hand` or `graveyard`"),
        };
        let of = if self.eat_word("of") {
            Some(self.recipient()?)
        } else {
            None
        };
        self.expect_words(&["into", "library"])?;
        Ok(Effect::ShuffleZone {
            who,
            of,
            hand,
            filter,
            cards,
        })
    }

    /// `cards from hand on top of library` / `on bottom of library`
    /// after the count: whether it's the bottom.
    fn hand_to_library_tail(&mut self) -> R<bool> {
        self.cards_word()?;
        self.expect_words(&["from", "hand", "on"])?;
        let bottom = self.eat_word("bottom");
        if !bottom {
            self.expect_word("top")?;
        }
        self.expect_words(&["of", "library"])?;
        Ok(bottom)
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

    /// After `return <what> to` or `put <what> onto`: `battlefield
    /// [tapped] [under your control] [with 1 +1/+1 counter] [attached to
    /// <what>]`.
    fn battlefield_tail(&mut self, what: Recipient, put: bool) -> R<Effect> {
        self.eat_word("the");
        self.expect_word("battlefield")?;
        let tapped = self.eat_word("tapped");
        self.eat_words(&["under", "its", "owner's", "control"]);
        let under_you = self.eat_words(&["under", "your", "control"]);
        let counters = if self.eat_word("with") {
            let n = self.count()?;
            let kind = self.counter_kind()?;
            self.eat_words(&["on", "it"]);
            Some((kind, n))
        } else {
            None
        };
        let attached = if self.eat_words(&["attached", "to"]) {
            Some(self.recipient()?)
        } else {
            None
        };
        Ok(Effect::ReturnToBattlefield {
            what,
            counters,
            tapped,
            under_you,
            attached,
            put,
        })
    }

    /// `[2] additional combat phase[s] [after this phase] [then main
    /// phase]` (CR 500.8): how many combat phases, and whether a main
    /// phase follows them.
    fn additional_phases(&mut self, trailing: bool) -> R<(u8, bool)> {
        let combats = if matches!(self.peek(), Some(Tok::Num(_))) {
            u8::try_from(self.number()?).unwrap_or(u8::MAX)
        } else {
            1
        };
        self.expect_words(&["additional", "combat"])?;
        if !self.eat_word("phase") {
            self.expect_word("phases")?;
        }
        if trailing {
            self.expect_words(&["after", "this", "phase"])?;
        }
        let main = self.eat_words(&["then", "main", "phase"]);
        Ok((combats, main))
    }

    #[allow(clippy::too_many_lines)]
    fn verb_first(&mut self) -> R<Effect> {
        if self.chosen_act_ahead() {
            return self.chosen_act();
        }
        let w = self.word()?;
        match w.as_str() {
            // "Choose a creature type." as the effect resolves.
            "choose"
                if matches!(
                    self.word_at(0).as_deref(),
                    Some("color" | "creature" | "land")
                ) =>
            {
                Ok(Effect::Choose(self.choice_kind(false)?))
            }
            // `deal damage equal <expr> to <what>`: "deals damage equal to
            // ... to <what>".
            "deal" | "deals" if self.at_words(&["damage", "equal"]) => {
                self.pos += 2;
                let expr = self.count_expr()?;
                let amount = self.push_count(crate::ast::Count {
                    expr,
                    style: crate::ast::CountStyle::EqualBefore,
                })?;
                self.expect_word("to")?;
                let to = self.recipients()?;
                Ok(Effect::DealDamage {
                    amount,
                    to,
                    by: Recipient::Source,
                })
            }
            // "Choose target creature, then roll a d20.": the target
            // named before what acts on it.
            "choose" if self.at_word("target") || self.at_words(&["any", "target"]) => {
                Ok(Effect::ChooseTarget(self.recipient()?))
            }
            // "Roll a d20." (CR 706); its results table follows on the
            // next lines (`result 1 to 9: ...`, see `effects_body`).
            "flip" => {
                self.expect_word("coin")?;
                Ok(Effect::FlipCoin)
            }
            // CR 701.23: "clash with an opponent".
            "clash" => Ok(Effect::Clash),
            // CR 500.8: "After this phase, there is an additional combat
            // phase."
            "after" if self.at_word("this") => {
                self.pos += 1;
                let main_phase = self.eat_word("main");
                self.expect_word("phase")?;
                self.expect(&Tok::Comma)?;
                let (combats, main) = self.additional_phases(false)?;
                Ok(Effect::AdditionalPhases {
                    combats,
                    main,
                    leading: true,
                    main_phase,
                })
            }
            "additional" if self.at_words(&["combat", "phase", "after"]) => {
                self.pos -= 1;
                let (combats, main) = self.additional_phases(true)?;
                Ok(Effect::AdditionalPhases {
                    combats,
                    main,
                    leading: false,
                    main_phase: false,
                })
            }
            // CR 701.38: "starting with you, each player votes for ...".
            "vote" => {
                if self.eat_word("return") {
                    let f = self.filter()?;
                    self.cards_word()?;
                    return Ok(Effect::VoteReturn(f));
                }
                let mut words = vec![self.word()?];
                while self.eat_word("or") {
                    words.push(self.word()?);
                }
                if words.len() != 2 {
                    return self.fail("a vote has two choices (`vote sprout or harvest`)");
                }
                self.votes.clone_from(&words);
                Ok(Effect::Vote(words))
            }
            "roll" => {
                let die = self.word()?;
                let sides = die
                    .strip_prefix('d')
                    .and_then(|n| n.parse::<u32>().ok())
                    .filter(|n| (1..=255).contains(n));
                match sides {
                    Some(sides) => Ok(Effect::RollDie {
                        sides,
                        rows: Vec::new(),
                    }),
                    None => self.fail(format!("`roll d<N>`, not `roll {die}`")),
                }
            }
            "deal" | "deals" => {
                let amount = self.num()?;
                self.expect_word("damage")?;
                // CR 601.2d: "divided as you choose among ...".
                if self.eat_words(&["divided", "among"]) {
                    let to = self.divided_group()?;
                    return Ok(Effect::DealDamageDivided { amount, to });
                }
                // `deal 4 damage instead`: to what the sentence before
                // dealt damage to.
                if !self.at_word("to") {
                    return Ok(Effect::DealDamage {
                        amount,
                        to: Vec::new(),
                        by: Recipient::Source,
                    });
                }
                self.expect_word("to")?;
                let to = self.recipients()?;
                Ok(Effect::DealDamage {
                    amount,
                    to,
                    by: Recipient::Source,
                })
            }
            "destroy" => {
                let to = self.recipients()?;
                let no_regen = self.eat_words(&["without", "regeneration"]);
                let joined = no_regen && self.eat_word("joined");
                Ok(Effect::Destroy {
                    to,
                    no_regen,
                    joined,
                })
            }
            // `put 2 cards from hand on top of library`.
            "put" if self.revealed && self.at_card_words() => self.put_card(None),
            "reveal" if self.revealed && self.at_word("it") => {
                self.pos += 1;
                Ok(Effect::RevealIt)
            }
            "discard" if self.revealed && self.at_word("it") => {
                self.pos += 1;
                Ok(Effect::DiscardIt)
            }
            "draw"
                if matches!(self.peek(), Some(Tok::Num(1)))
                    && (1..5).all(|k| {
                        self.word_at(k).as_deref() == Some(["", "card", "and", "reveal", "it"][k])
                    }) =>
            {
                self.pos += 5;
                self.revealed = true;
                Ok(Effect::DrawReveal)
            }
            // "put its counters on target creature you control".
            "put" if self.at_words(&["its", "counters", "on"]) => {
                self.pos += 3;
                let to = self.recipient()?;
                // "put its counters on this creature" in a triggered
                // ability: the triggering object's (Buzzard-Wasp Colony).
                let from = if self.in_trigger && to == Recipient::Source {
                    Recipient::TriggeringObject
                } else {
                    Recipient::Source
                };
                Ok(Effect::MoveCounters {
                    from,
                    to,
                    kind: None,
                    count: None,
                })
            }
            // "Move all counters from target creature onto another target
            // creature" (CR 122.5).
            "move" => {
                let (kind, count) = if self.eat_words(&["all", "counters"]) {
                    (None, None)
                } else {
                    let n = self.number()?;
                    let k = self.counter_kind()?;
                    (Some(k), Some(n))
                };
                self.expect_word("from")?;
                let from = self.recipient()?;
                self.expect_word("onto")?;
                let to = self.recipient()?;
                Ok(Effect::MoveCounters {
                    from,
                    to,
                    kind,
                    count,
                })
            }
            // `put +1/+1 counters equal <expr> on <what>`: "put a number
            // of +1/+1 counters equal to ... on <what>", the amount before
            // the recipient.
            "put"
                if matches!(
                    self.peek(),
                    Some(Tok::Pt { signed: true, .. } | Tok::Word(_))
                ) && self.word_at(1).as_deref() == Some("counters")
                    && self.word_at(2).as_deref() == Some("equal") =>
            {
                let kind = self.counter_kind()?;
                self.expect_word("equal")?;
                let expr = self.count_expr()?;
                let count = self.push_count(crate::ast::Count {
                    expr,
                    style: crate::ast::CountStyle::EqualBefore,
                })?;
                self.expect_word("on")?;
                let to = self.recipient()?;
                Ok(Effect::AddCounters { kind, count, to })
            }
            "put" if matches!(self.peek(), Some(Tok::Num(_))) && self.hand_to_library_ahead() => {
                let count = self.num()?;
                let bottom = self.hand_to_library_tail()?;
                Ok(Effect::HandToLibrary {
                    count,
                    who: None,
                    bottom,
                    shuffle: false,
                })
            }
            "shuffle" if self.shuffle_zone_ahead() => self.shuffle_zone(None),
            // `put that target 1 onto battlefield tapped`: "put that card
            // onto the battlefield tapped" (`return ... to battlefield`
            // otherwise).
            "put"
                if (0..5).any(|k| {
                    self.word_at(k).as_deref() == Some("onto")
                        && self.word_at(k + 1).as_deref() == Some("battlefield")
                }) && !(0..8).any(|k| {
                    self.word_at(k).as_deref() == Some("from")
                        && self.word_at(k + 1).as_deref() == Some("hand")
                }) =>
            {
                let what = self.recipient()?;
                self.expect_word("onto")?;
                self.battlefield_tail(what, true)
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
                self.expect_word("card")?;
                let also = if self.eat_word("and") {
                    let f = self.filter()?;
                    self.expect_word("card")?;
                    Some(f)
                } else {
                    None
                };
                self.expect_words(&["from", "hand", "onto", "battlefield"])?;
                let tapped = self.eat_word("tapped");
                Ok(Effect::PutFromHand {
                    filter,
                    tapped,
                    also,
                })
            }
            "put"
                if !matches!(self.peek(), Some(Tok::Num(_)))
                    && !self.word_at(0).is_some_and(|w| {
                        NUMBER_WORDS.contains(&w.as_str())
                            || w == "x"
                            || (w == "count"
                                && matches!(
                                    self.word_at(1).as_deref(),
                                    Some("where" | "equal" | "each")
                                ))
                    }) =>
            {
                let what = self.recipient()?;
                let place = self.library_place(false)?;
                if matches!(
                    place,
                    LibraryPlace::TopOrBottom | LibraryPlace::FromTopOrBottom(_)
                ) {
                    return self.fail("the owner chooses: `owner puts ...`");
                }
                Ok(Effect::PutInLibrary {
                    what,
                    place,
                    wording: LibraryWording::Put,
                })
            }
            // "You may shuffle." (CR 701.24): your library.
            "shuffle" if !matches!(self.peek(), Some(Tok::Word(_))) => Ok(Effect::Shuffle {
                who: Recipient::You,
            }),
            "shuffle" if !self.at_word("library") && !self.at_word("your") => {
                let what = self.recipient()?;
                let place = self.library_place(true)?;
                Ok(Effect::PutInLibrary {
                    what,
                    place,
                    wording: LibraryWording::Put,
                })
            }
            // `owner puts target creature on top or bottom of library`,
            // `owner of target creature puts it ...`, `owner shuffles this
            // into library`.
            "owner" => {
                let of = self.eat_word("of");
                let what = if of { Some(self.recipient()?) } else { None };
                let shuffle = match self.word()?.as_str() {
                    "puts" => false,
                    "shuffles" => true,
                    _ => return self.unexpected("`puts` or `shuffles`"),
                };
                let what = match what {
                    Some(w) => {
                        self.expect_word("it")?;
                        w
                    }
                    None => self.recipient()?,
                };
                let place = self.library_place(shuffle)?;
                Ok(Effect::PutInLibrary {
                    what,
                    place,
                    wording: if of {
                        LibraryWording::OwnerOf
                    } else {
                        LibraryWording::Owner
                    },
                })
            }

            "put" => {
                let count = self.num()?;
                let kind = self.counter_kind()?;
                let mut more = Vec::new();
                while self.at_word("and") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
                    self.pos += 1;
                    let n = self.num()?;
                    more.push((self.counter_kind()?, n));
                }
                self.expect_word("on")?;
                let to = self.recipient()?;
                if more.is_empty() {
                    return Ok(Effect::AddCounters { kind, count, to });
                }
                more.insert(0, (kind, count));
                Ok(Effect::AddCounterList { counters: more, to })
            }
            "distribute" => {
                let count = self.num()?;
                let kind = self.counter_kind()?;
                self.expect_word("among")?;
                let to = self.divided_group()?;
                Ok(Effect::DistributeCounters { kind, count, to })
            }
            "remove" => {
                if self.eat_words(&["all", "counters", "from"]) {
                    let to = self.recipient()?;
                    return Ok(Effect::RemoveAllCounters { to });
                }
                if matches!(self.peek(), Some(Tok::Num(1)))
                    && self.word_at(1).as_deref() == Some("counter")
                    && self.word_at(2).as_deref() == Some("from")
                {
                    self.pos += 3;
                    let to = self.recipient()?;
                    return Ok(Effect::RemoveCounterChosen { to });
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
                Ok(Effect::ReturnChosen {
                    count,
                    filter,
                    who: None,
                })
            }
            // "Return all creature cards from your graveyard to the
            // battlefield."
            "return"
                if self.at_word("all")
                    && (1..8).any(|k| {
                        self.word_at(k).as_deref() == Some("card")
                            && self.word_at(k + 1).as_deref() == Some("from")
                            && self.word_at(k + 2).as_deref() == Some("your")
                            && self.word_at(k + 3).as_deref() == Some("graveyard")
                    }) =>
            {
                self.pos += 1;
                let filter = self.filter()?;
                self.expect_words(&["card", "from", "your", "graveyard", "to", "battlefield"])?;
                Ok(Effect::GraveyardToBattlefield { filter })
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
                    if let [Recipient::Target(i)] = all.as_slice() {
                        self.returned = Some(*i);
                    }
                    return Ok(Effect::ReturnToHand { what: all });
                }
                if all.len() > 1 {
                    return self.fail("only one recipient returns to the battlefield");
                }
                self.battlefield_tail(what, false)
            }
            "regenerate" => {
                let to = self.recipient()?;
                Ok(Effect::Regenerate { to })
            }
            "exile" => {
                if self.eat_words(&["all", "graveyards"]) {
                    return Ok(Effect::ExileAllGraveyards);
                }
                // `exile until nonland card`.
                if self.eat_word("until") {
                    self.revealed = true;
                    let filter = self.filter()?;
                    self.expect_word("card")?;
                    return Ok(Effect::ExileUntilCard { who: None, filter });
                }
                // `exile top 3 cards`: from the top of your library.
                if self.at_word("top")
                    && (matches!(self.peek_at(1), Some(Tok::Num(_)))
                        || self.word_at(1).as_deref() == Some("count")
                        || self.word_at(1).as_deref() == Some("X"))
                {
                    self.pos += 1;
                    let count = self.num()?;
                    self.cards_word()?;
                    return Ok(Effect::ExileTop { count, who: None });
                }
                if self.eat_words(&["graveyard", "of"]) {
                    let who = self.recipient()?;
                    return Ok(Effect::ExileGraveyard { who });
                }
                let what = self.recipients()?;
                // "until this enchantment leaves the battlefield" (CR 610.3).
                if self.eat_words(&["until", "this", "leaves"]) {
                    return match what.as_slice() {
                        [one] => Ok(Effect::ExileUntil { what: *one }),
                        _ => self.fail("`until this leaves` takes one recipient"),
                    };
                }
                // "Exile target card from an opponent's graveyard. If it was
                // a creature card, that player loses 1 life.": the card's
                // owner.
                if let [Recipient::Target(i)] = what.as_slice()
                    && matches!(
                        self.targets.get(*i).map(Target::base),
                        Some(Target::GraveyardCard { .. })
                    )
                {
                    self.returned = Some(*i);
                }
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
            "bolster" => Ok(Effect::Bolster(self.num()?)),
            "monstrosity" => Ok(Effect::Monstrosity(self.num()?)),
            // CR 701.60a.
            "suspect" => Ok(Effect::Suspect {
                what: self.recipient()?,
                on: true,
            }),
            // CR 701.66a: "Target land you control becomes ...": the
            // keyword action declares its target.
            "earthbend" => {
                let count = self.num()?;
                self.targets.push(Target::Filter(Filter {
                    types: vec![mtg_core::card::CardType::Land],
                    who: Some(Who::You),
                    ..Filter::default()
                }));
                Ok(Effect::Earthbend {
                    count,
                    to: Recipient::Target(self.targets.len() - 1),
                })
            }
            "recruit" => Ok(Effect::Recruit { you: false }),
            "cloak" if self.eat_word("top") => Ok(Effect::CloakTop),
            // "airbend any number of other target nonland permanents you
            // control" (CR 601.2c).
            "airbend" => Ok(Effect::Airbend {
                what: if self.at_many() {
                    vec![self.many_target()?]
                } else {
                    self.recipients()?
                },
            }),
            "populate" => Ok(Effect::Populate),
            "incubate" => Ok(Effect::Incubate(self.num()?)),
            // CR 701.41a: "up to N other target creatures" on a permanent
            // (its triggered ability), "up to N target creatures" on a
            // spell.
            "support" => {
                let n = self.count()?;
                if n == 0 {
                    return self.fail("`support N`: N is at least 1");
                }
                let spec = Target::UpTo(Box::new(Target::Spec(if self.in_trigger {
                    TargetSpec::OtherCreature
                } else {
                    TargetSpec::Creature
                })));
                let mut to = Vec::new();
                for _ in 0..n {
                    to.push(Recipient::Target(self.targets.len()));
                    self.targets.push(spec.clone());
                }
                Ok(Effect::Support(to))
            }
            "discover" => Ok(Effect::Discover(self.count()?)),
            "impulse" => {
                let count = self.num()?;
                let until = self.play_until()?;
                let first = self.eat_word("first");
                let it = self.eat_word("it");
                let choose = self.eat_word("choose");
                let this_way = self.eat_words(&["this", "way"]);
                let from = if self.eat_word("of") {
                    Some(self.recipient()?)
                } else {
                    None
                };
                Ok(Effect::ExilePlayable {
                    count,
                    until,
                    first,
                    it,
                    choose,
                    from,
                    this_way,
                })
            }
            "dig" => {
                let count = self.num()?;
                let then = self.eat_word("then");
                let reveal_top = self.eat_word("reveal");
                self.expect_word("take")?;
                let all = self.eat_word("all");
                let take = if matches!(self.peek(), Some(Tok::Num(_))) {
                    self.number()?
                } else if self.eat_word("any") {
                    crate::ast::ANY_NUMBER
                } else {
                    1
                };
                let filter = if self.eat_word("card") {
                    None
                } else {
                    self.card_filter = true;
                    let f = self.filter();
                    self.card_filter = false;
                    let f = f?;
                    self.expect_word("card")?;
                    Some(f)
                };
                let also = if filter.is_some() && self.eat_word("and") {
                    let f = self.filter()?;
                    self.expect_word("card")?;
                    Some(f)
                } else {
                    None
                };
                let optional = self.eat_word("may");
                let revealing = self.eat_word("revealing");
                let that_card = revealing && self.eat_words(&["that", "card"]);
                // `take card may top`: back on top of the library.
                let top = self.eat_word("top");
                // `onto battlefield [tapped]`: onto the battlefield instead
                // of into the hand.
                let onto = if self.eat_words(&["onto", "battlefield"]) {
                    Some(self.eat_word("tapped"))
                } else {
                    None
                };
                self.expect_word("rest")?;
                let rest_graveyard = if self.eat_word("graveyard") {
                    true
                } else {
                    self.expect_word("bottom")?;
                    false
                };
                let random = !rest_graveyard && self.eat_word("random");
                let reword = self.eat_word("reword");
                let short = self.eat_word("short");
                Ok(Effect::Dig {
                    count,
                    filter,
                    optional,
                    all,
                    rest_graveyard,
                    reveal_top,
                    revealing,
                    take,
                    also,
                    random,
                    top,
                    onto,
                    that_card,
                    short,
                    then,
                    reword,
                })
            }
            "look" if self.at_words(&["at", "top", "card"]) => {
                self.pos += 3;
                self.revealed = true;
                // "look at the top card of target player's library".
                let who = if self.eat_word("of") {
                    Some(self.recipient()?)
                } else {
                    None
                };
                Ok(Effect::RevealTop {
                    of: who.is_some(),
                    who,
                    look: true,
                })
            }
            "reveal" if self.at_words(&["top", "card"]) => {
                self.pos += 2;
                self.revealed = true;
                let who = if self.eat_word("of") {
                    Some(self.recipient()?)
                } else {
                    None
                };
                Ok(Effect::RevealTop {
                    of: who.is_some(),
                    who,
                    look: false,
                })
            }
            "reveal" if self.at_word("until") => {
                self.pos += 1;
                self.revealed = true;
                let filter = self.filter()?;
                self.expect_word("card")?;
                Ok(Effect::RevealUntil { who: None, filter })
            }
            // `may cast that card this turn [first] [free]`.
            "may"
                if (self.at_word("play") || self.at_word("cast"))
                    && (self.word_at(1).as_deref() == Some("it")
                        || self.word_at(1).as_deref() == Some("that")) =>
            {
                let cast = self.word()? == "cast";
                let it = self.eat_word("it");
                if !it {
                    self.expect_words(&["that", "card"])?;
                }
                let until = self.play_until()?;
                let first = self.eat_word("first");
                let free = self.eat_word("free");
                Ok(Effect::MayPlayCard {
                    until,
                    cast,
                    free,
                    first,
                    it,
                })
            }
            "face-down" if self.at_words(&["spells", "cost"]) => {
                self.pos += 2;
                let n = self.mana_cost()?.mana_value();
                self.expect_words(&["less", "this", "turn"])?;
                Ok(Effect::FaceDownSpellsLess(n))
            }
            "look" if self.at_words(&["at", "hand", "top", "and", "face-down", "of"]) => {
                self.pos += 6;
                Ok(Effect::LookAtPlayer {
                    who: self.recipient()?,
                })
            }
            "look" if self.at_words(&["at", "face-down"]) => {
                self.pos += 2;
                Ok(Effect::LookAtFaceDown {
                    what: self.recipient()?,
                })
            }
            "turn" if self.at_words(&["face", "up"]) || self.at_words(&["face", "down"]) => {
                self.pos += 1;
                let up = self.word()? == "up";
                let what = self.recipient()?;
                Ok(if up {
                    Effect::TurnFaceUp { what }
                } else {
                    Effect::TurnFaceDown { what }
                })
            }
            "look" if self.at_words(&["at", "hand", "of"]) => {
                self.pos += 3;
                let who = self.recipient()?;
                if self.eat_word("discarding") {
                    return self.discard_chosen(who, true);
                }
                Ok(Effect::LookAtHand { who })
            }
            "look" if self.at_words(&["at", "top"]) => {
                self.pos += 2;
                self.look_at_top(None)
            }
            "manifest" if self.eat_word("dread") => Ok(Effect::ManifestDread),
            "blight" => Ok(Effect::Blight(self.count()?)),
            "collect" if self.eat_word("evidence") => Ok(Effect::CollectEvidence(self.count()?)),
            "forage" => Ok(Effect::Forage),
            "waterbend" => Ok(Effect::Waterbend(self.mana_cost()?)),
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

            "empower" => {
                self.expect_words(&["jace"])?;
                let count = self.num()?;
                Ok(Effect::EmpowerJace { count })
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
            "play"
                if self.at_word("additional")
                    || matches!(self.peek(), Some(Tok::Num(_)))
                        && self.word_at(1).as_deref() == Some("additional") =>
            {
                let n = if matches!(self.peek(), Some(Tok::Num(_))) {
                    self.number()?
                } else {
                    1
                };
                self.expect_words(&["additional", "land", "this", "turn"])?;
                Ok(Effect::ExtraLandPlays(n))
            }
            "next" if self.at_word("spell") => {
                self.expect_words(&["spell", "costs"])?;
                let n = self.mana_cost()?.mana_value();
                self.expect_word("less")?;
                Ok(Effect::NextSpellLess(n))
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
            "goad" => Ok(Effect::Goad {
                what: self.recipient()?,
            }),
            "detain" => Ok(Effect::Detain {
                what: self.recipient()?,
            }),
            "untap" => {
                let to = self.recipients()?;
                Ok(Effect::Untap { to })
            }
            "gain" => {
                self.expect_word("control")?;
                self.expect_word("of")?;
                let of = self.recipient()?;
                let forever = self.eat_word("indefinitely");
                if !forever {
                    self.until_end_of_turn();
                }
                Ok(Effect::GainControl { of, forever })
            }
            "draw" => {
                let count = self.num()?;
                self.cards_word()?;
                // "Draw two cards, then discard one of them."
                if self.at_words(&["then", "discard"])
                    && self.word_at(3).as_deref() == Some("of")
                    && self.word_at(4).as_deref() == Some("them")
                {
                    self.pos += 2;
                    let discard = self.num()?;
                    self.expect_words(&["of", "them"])?;
                    return Ok(Effect::DrawDiscardDrawn {
                        draw: count,
                        discard,
                    });
                }
                Ok(Effect::DrawCards { count, who: None })
            }
            "mill" => {
                let count = self.num()?;
                self.cards_word()?;
                Ok(Effect::Mill { count, who: None })
            }
            "discard" => {
                if let Some(e) = self.discard_all(None)? {
                    return Ok(e);
                }
                let count = self.num()?;
                self.cards_word()?;
                if let Some(filter) = self.discard_unless()? {
                    return Ok(Effect::DiscardUnless {
                        who: None,
                        count,
                        filter,
                    });
                }
                // "discard a card at random": the controller.
                if self.eat_words(&["at", "random"]) {
                    return Ok(Effect::DiscardRandom {
                        count,
                        who: None,
                        another: false,
                    });
                }
                Ok(Effect::Discard { count, who: None })
            }
            "scry" => Ok(Effect::Scry(self.num()?)),
            "investigate"
                if self.at_words(&["count", "each"]) || self.at_words(&["once", "count"]) =>
            {
                let once = self.eat_word("once");
                Ok(Effect::InvestigateCount {
                    count: self.num()?,
                    once,
                })
            }
            "investigate" => Ok(Effect::Investigate(if self.eat_word("twice") {
                2
            } else if matches!(self.peek(), Some(Tok::Num(_))) {
                let n = self.number()?;
                self.expect_word("times")?;
                n
            } else {
                1
            })),
            "transform" => Ok(Effect::Transform {
                what: self.recipient()?,
            }),
            "search" | "searches" => {
                let up_to = self.eat_words(&["up", "to"]);
                let count = if matches!(self.peek(), Some(Tok::Num(_))) {
                    self.num()?
                } else if self.eat_words(&["any", "number"]) {
                    Num::N(crate::ast::ANY_NUMBER)
                } else {
                    Num::N(1)
                };
                let mut filter = if self.eat_word("card") || self.eat_word("cards") {
                    Filter::default()
                } else {
                    let f = self.filter()?;
                    if !(self.eat_word("card") || self.eat_word("cards")) {
                        return self.unexpected("`card`");
                    }
                    f
                };
                // "a card named ~".
                if self.eat_words(&["named", "this"]) {
                    filter.same = Some(SameRef::NamedThis);
                }
                // "a creature card with mana value 6 or greater": the mana
                // value after the noun.
                if filter.mana_value.is_none() && self.at_words(&["with", "mana", "value"]) {
                    let g = self.filter()?;
                    filter.mana_value = g.mana_value;
                    filter.status.mana_value_x = g.status.mana_value_x;
                }
                // "a basic land card or a Gate card": the alternative as its
                // own noun.
                let mut or_card = crate::ast::OrCard::No;
                if self.at_word("or") && filter.or.is_none() && filter != Filter::default() {
                    self.pos += 1;
                    or_card = if self.eat_word("a") || self.eat_word("an") {
                        crate::ast::OrCard::Article
                    } else {
                        crate::ast::OrCard::Bare
                    };
                    let alt = self.filter()?;
                    if !(self.eat_word("card") || self.eat_word("cards")) {
                        return self.unexpected("`card`");
                    }
                    filter.or = Some(Box::new(alt));
                }
                let reveal = self.eat_word("reveal");
                self.expect_word("to")?;
                let to = if self.eat_word("hand") {
                    mtg_core::card::SearchTo::Hand
                } else if self.eat_word("battlefield") {
                    let tapped = self.eat_word("tapped");
                    if tapped && self.eat_words(&["and", "hand"]) {
                        mtg_core::card::SearchTo::BattlefieldAndHand
                    } else {
                        mtg_core::card::SearchTo::Battlefield { tapped }
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
                    or_card,
                    who: None,
                    may: false,
                })
            }
            "add" => {
                if self.eat_word("chosen") {
                    return Ok(Effect::AddMana(vec![Mana::Chosen]));
                }
                // "any color that a land an opponent controls could
                // produce" (CR 106.7).
                if self.eat_words(&["any", "that"]) {
                    let basic = self.eat_word("basic");
                    self.expect_word("land")?;
                    let whose = if self.eat_words(&["you", "control"]) {
                        Who::You
                    } else {
                        self.expect_words(&["opponent", "controls"])?;
                        Who::Opponents
                    };
                    self.expect_words(&["could", "produce"])?;
                    return Ok(Effect::AddManaLandCould { whose, basic });
                }
                if self.eat_word("any") {
                    if matches!(self.peek(), Some(Tok::Num(_))) {
                        return Ok(Effect::AddManaTimes {
                            mana: Mana::Any,
                            count: self.num()?,
                        });
                    }
                    return Ok(Effect::AddMana(vec![Mana::Any]));
                }
                let x = self.at_word("x") && self.word_at(1).as_deref() == Some("mana");
                if x || matches!(self.peek(), Some(Tok::Num(_))) {
                    let mut count = if x {
                        self.pos += 1;
                        Num::X
                    } else {
                        Num::N(self.number()?)
                    };
                    self.expect_word("mana")?;
                    if self.eat_words(&["different", "colors"]) {
                        return Ok(Effect::AddManaCombination {
                            count,
                            colors: mtg_core::mana::ColorSet::ALL,
                            different: true,
                        });
                    }
                    self.expect_words(&["any", "combination"])?;
                    let colors = if self.eat_word("of") {
                        let cost = self.mana_cost()?;
                        let mut set = mtg_core::mana::ColorSet::COLORLESS;
                        for s in cost.symbols() {
                            match s {
                                mtg_core::mana::ManaSymbol::Colored(c) => set = set.with(*c),
                                _ => return self.unexpected("colored mana symbols"),
                            }
                        }
                        set
                    } else {
                        mtg_core::mana::ColorSet::ALL
                    };
                    // "Add X mana ..., where X is the number of ...".
                    if x && self.at_word("count") {
                        count = self.num()?;
                    }
                    return Ok(Effect::AddManaCombination {
                        count,
                        colors,
                        different: false,
                    });
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
            "learn" => Ok(Effect::Learn),
            // CR 701.10.
            "double" => {
                if self.eat_words(&["the", "number", "of"]) {
                    let kind = if self.eat_words(&["each", "kind", "of", "counter"]) {
                        None
                    } else {
                        let k = self.counter_kind()?;
                        Some(k)
                    };
                    self.expect_word("on")?;
                    let to = self.recipient()?;
                    return Ok(Effect::DoubleCounters { kind, to });
                }
                if self.eat_words(&["your", "life", "total"]) {
                    return Ok(Effect::DoubleLife {
                        who: Recipient::You,
                    });
                }
                let of = self.eat_words(&["the", "power"]);
                let (to, toughness) = if of {
                    let toughness = self.eat_words(&["and", "toughness"]);
                    self.expect_word("of")?;
                    (self.recipient()?, toughness)
                } else {
                    let to = self.recipient()?;
                    if self.eat_words(&["life", "total"]) {
                        return Ok(Effect::DoubleLife { who: to });
                    }
                    self.expect_word("power")?;
                    (to, self.eat_words(&["and", "toughness"]))
                };
                self.until_end_of_turn();
                Ok(Effect::DoublePt { to, toughness, of })
            }
            // `copy target instant or sorcery spell you control [times
            // N] [new targets] [with exceptions]` (CR 707.10).
            "copy" => {
                let what = self.recipient()?;
                let times = if self.eat_word("times") {
                    self.num()?
                } else {
                    Num::N(1)
                };
                let new_targets = self.eat_words(&["new", "targets"]);
                let except = self.except_clause();
                Ok(Effect::CopySpells {
                    what,
                    times,
                    new_targets,
                    except,
                })
            }
            "counter" => {
                let to = self.recipient()?;
                if self.eat_word("unless") {
                    self.expect_words(&["its", "controller", "pays"])?;
                    let mana = self.mana_cost()?;
                    // `{X} count each hand`: "{1} for each card in your
                    // hand"; a bare {X} is the spell's X.
                    let x = if self.at_word("count") {
                        self.num()?
                    } else {
                        Num::X
                    };
                    let dest = self.countered_to();
                    return Ok(Effect::CounterUnlessPays { to, mana, x, dest });
                }
                let dest = self.countered_to();
                Ok(Effect::Counter { to, dest })
            }
            // `creates` after a subject (`its controller creates ...`).
            "create" | "creates" => {
                if self.eat_word("token") || self.eat_words(&["a", "token"]) {
                    self.expect_word("copy")?;
                    self.expect_word("of")?;
                    let of = self.recipient()?;
                    let except = self.except_clause();
                    return Ok(Effect::CreateTokenCopy { of, except });
                }
                let count = self.num()?;
                let tapped = self.eat_word("tapped");
                let token = self.string()?;
                self.eat_word("token");
                self.eat_word("tokens");
                let it_has = self.eat_words(&["it", "has"]);
                let attached = if self.eat_words(&["attached", "to"]) {
                    Some(self.recipient()?)
                } else {
                    None
                };
                Ok(Effect::CreateToken {
                    token,
                    count,
                    tapped,
                    who: None,
                    it_has,
                    attached,
                })
            }
            "attach" => {
                let what = if self.eat_word("to") || self.eat_words(&["this", "to"]) {
                    Recipient::Source
                } else {
                    let what = self.recipient()?;
                    self.expect_word("to")?;
                    what
                };
                let to = self.recipient()?;
                Ok(Effect::Attach { what, to })
            }
            "unattach" => Ok(Effect::Unattach {
                what: self.recipient()?,
            }),
            "emblem" => {
                let emblem = self.string()?;
                Ok(Effect::CreateEmblem { emblem })
            }
            "delayed" => {
                let about = self.recipient()?;
                self.expect(&Tok::Colon)?;
                let saved_subject = self.subject.take();
                let was = (self.in_delayed, self.delayed_that);
                self.in_delayed = true;
                self.delayed_that = about == Recipient::TriggeringObject;
                // "When that creature dies this turn, ..." (CR 603.7c).
                let mut it = false;
                let this_turn =
                    if self.at_words(&["when", "that", "creature", "dies", "this", "turn"]) {
                        self.pos += 6;
                        Some(Trigger::Dies(TriggerObj::This))
                    } else if self.at_words(&["when", "it", "dies", "this", "turn"]) {
                        // "When it dies this turn, manifest dread."
                        self.pos += 5;
                        it = true;
                        Some(Trigger::Dies(TriggerObj::This))
                    } else if self.at_words(&[
                        "when",
                        "that",
                        "creature",
                        "leaves",
                        "the",
                        "battlefield",
                        "this",
                        "turn",
                    ]) {
                        self.pos += 8;
                        Some(Trigger::Leaves(TriggerObj::This))
                    } else {
                        None
                    };
                // "Whenever a creature blocks this turn, ...": a delayed
                // trigger that triggers each time until the turn ends.
                let mut turn_long = false;
                let (trigger, effects, trigger_first) = if let Some(t) = this_turn.clone() {
                    if self.peek() == Some(&Tok::Comma) {
                        self.pos += 1;
                    }
                    let was_dies = self.delayed_dies;
                    self.delayed_dies = true;
                    let e = self.sentences_inline();
                    self.delayed_dies = was_dies;
                    (t, e?, true)
                } else if self.at_word("at") || self.at_word("when") || self.at_word("whenever") {
                    let t = self.trigger()?;
                    turn_long = self.eat_words(&["this", "turn"]);
                    if self.peek() == Some(&Tok::Comma) {
                        self.pos += 1;
                    }
                    // "its controller": the triggering object's.
                    let was_dies = self.delayed_dies;
                    self.delayed_dies |= turn_long;
                    let e = self.sentences_inline();
                    self.delayed_dies = was_dies;
                    (t, e?, true)
                } else {
                    let e = self.delayed_effects()?;
                    let t = self.trigger()?;
                    (t, e, false)
                };
                (self.in_delayed, self.delayed_that) = was;
                self.subject = saved_subject;
                Ok(Effect::Delayed {
                    about,
                    trigger,
                    effects,
                    trigger_first,
                    this_turn: this_turn.is_some() || turn_long,
                    it,
                })
            }
            "prevent" => {
                let next = if self.eat_word("all") {
                    None
                } else {
                    self.eat_word("the");
                    self.expect_word("next")?;
                    Some(self.num()?)
                };
                let combat = if self.eat_word("combat") {
                    Some(true)
                } else if self.eat_word("noncombat") {
                    Some(false)
                } else {
                    None
                };
                self.expect_word("damage")?;
                // `damage this turn to A [from S]`: "that would be dealt this
                // turn to A [by S]".
                let turn_first = self.at_words(&["this", "turn", "to"]);
                if turn_first {
                    self.pos += 2;
                }
                let event = if turn_first {
                    match self.damage_event(combat)? {
                        Replaces::Damage {
                            from, to, combat, ..
                        } => Replaces::Damage {
                            from,
                            to,
                            combat,
                            wording: DamageWording::TurnFirst,
                        },
                        _ => return self.fail("`this turn to` names a recipient".to_string()),
                    }
                } else if next.is_none() {
                    let event = self.damage_event(combat)?;
                    self.expect_words(&["this", "turn"])?;
                    match event {
                        // `... this turn from <source>`: "dealt this turn by"
                        // (Vine Snare).
                        Replaces::Damage {
                            from: None,
                            to,
                            combat,
                            ..
                        } if self.eat_word("from") => Replaces::Damage {
                            from: Some(self.source()?),
                            to,
                            combat,
                            wording: DamageWording::DealtLate,
                        },
                        e => e,
                    }
                } else {
                    let to = if self.eat_word("to") {
                        self.affects()?
                    } else {
                        Affects::Any
                    };
                    self.expect_words(&["this", "turn"])?;
                    // `... this turn from <source>`: "dealt to A this turn by
                    // S" (Healing Grace).
                    let from = if self.eat_word("from") {
                        Some(self.source()?)
                    } else {
                        None
                    };
                    let wording = if from.is_some() {
                        DamageWording::DealtLate
                    } else {
                        DamageWording::Dealt
                    };
                    Replaces::Damage {
                        from,
                        to,
                        combat,
                        wording,
                    }
                };
                Ok(Effect::CreateReplacement(Replacement {
                    event,
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
            // "Excess damage is dealt to that creature's controller
            // instead." (Pigment Storm).
            "excess" if self.at_words(&["damage", "to", "its", "controller", "instead"]) => {
                self.pos += 5;
                Ok(Effect::ExcessToController)
            }
            // "The damage can't be prevented." (Pinpoint Avalanche).
            "the" if self.at_words(&["damage", "can't", "be", "prevented"]) => {
                self.pos += 4;
                Ok(Effect::DamageUnpreventable)
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

    /// After a counter: `to exile` ("If that spell is countered this way,
    /// exile it instead ..."), `to library top`, or nothing.
    fn countered_to(&mut self) -> crate::ast::CounteredTo {
        use crate::ast::CounteredTo;
        if self.eat_words(&["to", "exile"]) {
            return CounteredTo::Exile;
        }
        if self.eat_words(&["to", "library", "top"]) {
            return CounteredTo::LibraryTop;
        }
        CounteredTo::Graveyard
    }

    /// Where in a library: `on top of library`, `on bottom of library`,
    /// `on top or bottom of library`, `into library 2 from top [or
    /// bottom]`, or (for a shuffle) `into library`.
    fn library_place(&mut self, shuffle: bool) -> R<crate::ast::LibraryPlace> {
        use crate::ast::LibraryPlace;
        if shuffle {
            self.expect_words(&["into", "library"])?;
            return Ok(LibraryPlace::Shuffle);
        }
        if self.eat_words(&["into", "library"]) {
            let n = self.number()?;
            if n < 2 {
                return self.fail("`into library N from top` needs N of 2 or more");
            }
            let n = u8::try_from(n).map_err(|_| ParseError {
                line: self.line(),
                message: "too deep".to_string(),
            })?;
            self.expect_words(&["from", "top"])?;
            return Ok(if self.eat_words(&["or", "bottom"]) {
                LibraryPlace::FromTopOrBottom(n)
            } else {
                LibraryPlace::FromTop(n)
            });
        }
        self.expect_word("on")?;
        let place = if self.eat_words(&["top", "or", "bottom"]) {
            LibraryPlace::TopOrBottom
        } else if self.eat_word("top") {
            LibraryPlace::Top
        } else {
            self.expect_word("bottom")?;
            LibraryPlace::Bottom
        };
        self.expect_word("of")?;
        self.eat_words(&["its", "owner's"]);
        self.expect_word("library")?;
        Ok(place)
    }

    fn num(&mut self) -> R<Num> {
        if self.at_words(&["that", "much"]) {
            self.pos += 2;
            return Ok(Num::ThatMuch);
        }
        if self.at_word("count")
            && (matches!(self.word_at(1).as_deref(), Some("where" | "equal" | "each"))
                || self.at_words(&["count", "that", "many"]))
        {
            self.pos += 1;
            let style = match self.word()?.as_str() {
                "where" => crate::ast::CountStyle::Where,
                "equal" => crate::ast::CountStyle::Equal,
                "that" => {
                    self.expect_word("many")?;
                    crate::ast::CountStyle::ThatMany
                }
                _ => crate::ast::CountStyle::Each,
            };
            let expr = self.count_expr()?;
            return self.push_count(crate::ast::Count { expr, style });
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
                    // `X plus 1`: "X plus 1 damage".
                    if self.at_word("plus") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
                        self.pos += 1;
                        return Ok(Num::XPlus(self.number()?));
                    }
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

    /// The card's count for `c`, shared with an equal one already parsed.
    fn push_count(&mut self, c: crate::ast::Count) -> R<Num> {
        let i = if let Some(i) = self.counts.iter().position(|x| *x == c) {
            i
        } else {
            self.counts.push(c);
            self.counts.len() - 1
        };
        Ok(Num::Count(u8::try_from(i).map_err(|_| ParseError {
            line: self.line(),
            message: "too many counts".to_string(),
        })?))
    }

    fn count(&mut self) -> R<u32> {
        match self.num()? {
            Num::N(n) => Ok(n),
            Num::X | Num::XPlus(_) | Num::Count(_) | Num::ThatMuch => {
                self.fail("X is not allowed here")
            }
        }
    }

    /// `life lost this way`, `discarded this way`, `enchantment destroyed
    /// this way`: what the resolution did (CR 608.2c); `None` (nothing
    /// consumed) otherwise.
    fn this_way_count(&mut self) -> R<Option<crate::ast::CountExpr>> {
        use crate::ast::CountExpr;
        // `plus 1 discarded this way`: the prefix form, read by the caller.
        if self.at_word("plus") {
            return Ok(None);
        }
        if self.eat_words(&["life", "lost", "this", "way"]) {
            return Ok(Some(CountExpr::LifeLostThisWay));
        }
        if let Some(verb) = self
            .word_at(0)
            .and_then(|w| crate::ast::this_way_verb(&w))
            .filter(|_| self.word_at(1).as_deref() == Some("this"))
        {
            self.pos += 1;
            self.expect_words(&["this", "way"])?;
            return Ok(Some(CountExpr::ThisWay { verb, filter: None }));
        }
        if (1..10).any(|k| {
            self.word_at(k).as_deref() == Some("this")
                && self.word_at(k + 1).as_deref() == Some("way")
        }) {
            let f = self.filter()?;
            let Some(verb) = self.word_at(0).and_then(|w| crate::ast::this_way_verb(&w)) else {
                return self.unexpected("`destroyed`, `exiled`, ...");
            };
            self.pos += 1;
            self.expect_words(&["this", "way"])?;
            return Ok(Some(CountExpr::ThisWay {
                verb,
                filter: Some(f),
            }));
        }
        Ok(None)
    }

    /// `domain`, `hand`, `graveyard [<filter>]`, `power`, `power of target
    /// N`, `life`, or a filter (`creature you control`, `Elf on
    /// battlefield`).
    fn count_expr(&mut self) -> R<crate::ast::CountExpr> {
        let first = self.count_expr_one()?;
        // `<expr> plus <expr>`; `plus 2 <expr>` is the prefix form.
        if self.at_word("plus")
            && !matches!(self.peek_at(1), Some(Tok::Num(_)))
            && !self
                .word_at(1)
                .is_some_and(|w| NUMBER_WORDS.contains(&w.to_ascii_lowercase().as_str()))
        {
            self.pos += 1;
            let second = self.count_expr_one()?;
            return Ok(crate::ast::CountExpr::Sum(
                Box::new(first),
                Box::new(second),
            ));
        }
        Ok(first)
    }

    /// `power of it`, `toughness of that creature` (the triggering
    /// object), `toughness of the sacrificed creature`, `mana value of that
    /// sacrificed creature` (what the resolution moved); `None`, nothing
    /// consumed, otherwise.
    fn stat_of(&mut self) -> Option<crate::ast::CountExpr> {
        use crate::ast::CountExpr;
        use mtg_core::card::Stat;
        let (stat, len) = if self.at_words(&["power", "of"]) {
            (Stat::Power, 2)
        } else if self.at_words(&["toughness", "of"]) {
            (Stat::Toughness, 2)
        } else if self.at_words(&["mana", "value", "of"]) {
            (Stat::ManaValue, 3)
        } else {
            return None;
        };
        let at = |k: usize| self.word_at(len + k);
        if at(0).as_deref() == Some("attached") {
            self.pos += len + 1;
            return Some(CountExpr::AttachedStat(stat));
        }
        if self.in_trigger && at(0).as_deref() == Some("it") {
            self.pos += len + 1;
            return Some(CountExpr::TriggerStat { stat, that: false });
        }
        if self.in_trigger
            && at(0).as_deref() == Some("that")
            && at(1).as_deref() == Some("creature")
        {
            self.pos += len + 2;
            return Some(CountExpr::TriggerStat { stat, that: true });
        }
        let that = match at(0).as_deref() {
            Some("the") => false,
            Some("that") => true,
            _ => return None,
        };
        let verb = at(1).and_then(|w| crate::ast::this_way_verb(&w))?;
        let noun = at(2).filter(|n| n.chars().all(|c| c.is_ascii_lowercase()))?;
        self.pos += len + 3;
        Some(CountExpr::MovedStat {
            verb,
            stat,
            noun,
            that,
        })
    }

    #[allow(clippy::too_many_lines)] // one branch per count
    fn count_expr_one(&mut self) -> R<crate::ast::CountExpr> {
        use crate::ast::CountExpr;
        // The card revealed before: "the card's mana value", "its mana
        // value".
        if self.revealed && self.eat_words(&["mana", "value", "of", "the", "card"]) {
            return Ok(CountExpr::CardManaValue(crate::ast::CardWord::TheCard));
        }
        if self.revealed && self.eat_words(&["its", "mana", "value"]) {
            return Ok(CountExpr::CardManaValue(crate::ast::CardWord::Its));
        }
        if let Some(c) = self.stat_of() {
            return Ok(c);
        }
        if self.eat_words(&["life", "you", "lost", "this", "turn"]) {
            return Ok(CountExpr::LifeLostThisTurn);
        }
        if self.eat_words(&["damage", "dealt", "this", "way"]) {
            return Ok(CountExpr::DamageThisWay { excess: false });
        }
        if self.eat_words(&["card", "types", "in", "graveyards"]) {
            return Ok(CountExpr::CardTypesInGraveyards);
        }
        if self.eat_words(&["card", "types", "in", "your", "graveyard"]) {
            return Ok(CountExpr::CardTypesInGraveyard);
        }
        if self.eat_words(&["life", "you", "gained", "this", "turn"]) {
            return Ok(CountExpr::LifeGainedThisTurn { ve: false });
        }
        if self.eat_words(&["life", "you've", "gained", "this", "turn"]) {
            return Ok(CountExpr::LifeGainedThisTurn { ve: true });
        }
        if self.eat_words(&["permanents", "sacrificed", "this", "turn"]) {
            return Ok(CountExpr::SacrificedThisTurn);
        }
        if self.eat_words(&["two", "to", "x"]) {
            return Ok(CountExpr::TwoToX);
        }
        if self.eat_words(&["creature", "attacked", "this", "turn"]) {
            return Ok(CountExpr::AttackedThisTurn);
        }
        if self.eat_words(&["opponents", "lost", "life", "this", "turn"]) {
            return Ok(CountExpr::OpponentsLostLife);
        }
        if self.eat_words(&["opponents", "more", "cards", "in", "hand"]) {
            return Ok(CountExpr::OpponentsMoreCards);
        }
        // `counters on this`, `counters on it`: every kind.
        if self.eat_words(&["counters", "on"]) {
            if self.eat_word("this") {
                return Ok(CountExpr::SourceAllCounters { it: false });
            }
            self.expect_word("it")?;
            // In a triggered ability `it` is the triggering object, which
            // this count doesn't read.
            if self.in_trigger && !self.it_source && !self.in_delayed {
                return self.fail("`counters on it` counts the source's; write `counters on this`");
            }
            return Ok(CountExpr::SourceAllCounters { it: true });
        }
        // `total power creature you control`.
        if self.eat_word("total") {
            use mtg_core::card::Stat;
            let stat = if self.eat_word("power") {
                Stat::Power
            } else if self.eat_word("toughness") {
                Stat::Toughness
            } else {
                self.expect_words(&["mana", "value"])?;
                Stat::ManaValue
            };
            return Ok(CountExpr::Total(stat, self.filter()?));
        }
        // `+1/+1 counters on this`, `age counters on it`.
        let at = self.pos;
        if let Ok(kind) = self.counter_kind()
            && self.eat_word("on")
        {
            if self.eat_word("this") {
                return Ok(CountExpr::SourceCounters { kind, it: false });
            }
            if self.eat_word("it") {
                // In a triggered ability `it` is the triggering object
                // ("for each +1/+1 counter on it" of the creature that
                // left), as a recipient is.
                if self.in_trigger && !self.it_source && !self.in_delayed {
                    return Ok(CountExpr::TriggerCounters(kind));
                }
                return Ok(CountExpr::SourceCounters { kind, it: true });
            }
        }
        self.pos = at;
        // "for each of those creatures": a group named before.
        if self.group_named
            && let Some((t, n)) = self.at_those()
        {
            self.pos += n;
            return Ok(CountExpr::Those(t));
        }
        if self.eat_words(&["excess", "damage", "dealt", "this", "way"]) {
            return Ok(CountExpr::DamageThisWay { excess: true });
        }
        if self.eat_words(&["greatest", "toughness"]) {
            return Ok(CountExpr::GreatestToughness(self.filter()?));
        }
        if self.eat_words(&["hand", "of", "target"]) {
            let n = self.number()? as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(CountExpr::TargetHand(n - 1));
        }
        if let Some(Tok::Num(n)) = self.peek() {
            let n = *n;
            self.pos += 1;
            let inner = self.count_expr()?;
            return Ok(CountExpr::Times(n, Box::new(inner)));
        }
        if let Some(c) = self.this_way_count()? {
            return Ok(c);
        }
        // `votes for sprout`: "for each sprout vote".
        if self.eat_words(&["votes", "for"]) {
            let word = self.word()?;
            let Some(index) = self.votes.iter().position(|w| *w == word) else {
                return self.fail(format!("`{word}` is not a choice of the ability's vote"));
            };
            return Ok(CountExpr::Votes {
                index: u8::try_from(index).unwrap_or(u8::MAX),
                word,
            });
        }
        // `greatest power creature you control`; bare `greatest power` is
        // among creatures you control.
        if self.eat_words(&["differently", "named"]) {
            return Ok(CountExpr::DifferentlyNamed(self.filter()?));
        }
        if self.eat_words(&["greatest", "power", "graveyard"]) {
            return Ok(CountExpr::GreatestPowerInGraveyard(self.filter()?));
        }
        if self.eat_words(&["greatest", "power"]) {
            let f = if self.word_at(0).is_some_and(|w| card_type(&w).is_some()) {
                self.filter()?
            } else {
                Filter {
                    types: vec![CardType::Creature],
                    who: Some(Who::You),
                    ..Filter::default()
                }
            };
            return Ok(CountExpr::GreatestPower(f));
        }
        if self.eat_word("domain") {
            return Ok(CountExpr::Domain);
        }
        if self.eat_word("hand") {
            return Ok(CountExpr::CardsInHand);
        }
        if self.eat_word("life") {
            return Ok(CountExpr::Life);
        }
        // "for each creature blocking it [beyond the first]".
        if self.eat_words(&["creature", "blocking", "it"]) {
            let beyond = if self.eat_word("beyond") {
                if self.eat_word("first") {
                    1
                } else {
                    self.number()?
                }
            } else {
                0
            };
            return Ok(CountExpr::BlockingIt { beyond });
        }
        if self.eat_word("party") {
            return Ok(CountExpr::Party);
        }
        // `devotion green`, `devotion white and blue` (CR 700.5); `red
        // mana symbols` (chroma).
        let chroma = self.word_at(0).as_deref().and_then(color).is_some()
            && self.word_at(1).as_deref() == Some("mana")
            && self.word_at(2).as_deref() == Some("symbols");
        if chroma || self.eat_word("devotion") {
            let mut colors = Vec::new();
            loop {
                let Some(c) = self.word_at(0).as_deref().and_then(color) else {
                    return self.unexpected("a color");
                };
                self.pos += 1;
                colors.push(c);
                if !self.eat_word("and") {
                    break;
                }
            }
            if chroma {
                self.expect_word("mana")?;
                self.expect_word("symbols")?;
            }
            return Ok(CountExpr::Devotion { colors, chroma });
        }
        if self.eat_words(&["greatest", "mana", "value"]) {
            return Ok(CountExpr::GreatestManaValue);
        }
        if self.eat_words(&["drawn", "this", "turn"]) {
            return Ok(CountExpr::CardsDrawnThisTurn);
        }
        if self.eat_words(&["spells", "cast", "this", "turn"]) {
            return Ok(CountExpr::SpellsCastThisTurn { other: false });
        }
        if self.eat_words(&["other", "spells", "cast", "this", "turn"]) {
            return Ok(CountExpr::SpellsCastThisTurn { other: true });
        }
        if self.eat_words(&["other", "instant", "and", "sorcery", "spells", "cast"]) {
            let before = self.eat_words(&["before", "it"]);
            self.expect_words(&["this", "turn"])?;
            return Ok(CountExpr::OtherInstantSorceryCast { before });
        }
        if self.eat_words(&["opponents", "attacked"]) {
            return Ok(CountExpr::OpponentsAttacked);
        }
        if self.eat_words(&["attacked", "with", "this", "turn"]) {
            return Ok(CountExpr::AttackedWithThisTurn);
        }
        if self.eat_words(&["colors", "among", "permanents"]) {
            return Ok(CountExpr::ColorsAmongPermanents);
        }
        if self.eat_words(&["colors", "spent"]) {
            let of = if self.eat_word("spell") {
                crate::ast::SpentOf::Spell
            } else if self.eat_word("creature") {
                crate::ast::SpentOf::Creature
            } else {
                crate::ast::SpentOf::Default
            };
            return Ok(CountExpr::SpentColors(of));
        }
        if self.eat_words(&["times", "kicked"]) {
            return Ok(CountExpr::TimesKicked);
        }
        if self.eat_words(&["treasure", "spent"]) {
            return Ok(CountExpr::SpentTreasure);
        }
        if self.eat_word("andor") {
            return Ok(CountExpr::AndOr(Box::new(self.count_expr()?)));
        }
        if self.eat_words(&["mana", "value", "of", "that", "card"]) {
            return Ok(CountExpr::CardManaValue(crate::ast::CardWord::ThatCard));
        }
        if self.eat_words(&["mana", "value", "of"]) {
            let the_spell = self.eat_words(&["the", "spell"]);
            let that = !the_spell && self.eat_word("that");
            self.expect_word("target")?;
            let n = self.number()? as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(CountExpr::TargetManaValue {
                n: n - 1,
                that,
                the_spell,
            });
        }
        if self.at_word("toughness") && !self.at_words(&["toughness", "of"]) {
            self.pos += 1;
            return Ok(CountExpr::SourceToughness);
        }
        if self.eat_words(&["opponents", "graveyards"]) {
            let f = if self.at_stmt_end()
                || !self.word_at(0).is_some_and(|w| {
                    card_type(&w).is_some() || w.starts_with("non") || color(&w).is_some()
                }) {
                None
            } else {
                Some(self.filter()?)
            };
            return Ok(CountExpr::CardsInOpponentsGraveyards(f));
        }
        let stat = self.word_at(0);
        let toughness = stat.as_deref() == Some("toughness");
        if (toughness
            && (self.at_words(&["toughness", "of", "target"])
                || self.at_words(&["toughness", "of", "that", "target"])))
            || (!toughness && stat.as_deref() == Some("power"))
        {
            self.pos += 1;
            let that = self.eat_words(&["of", "that", "target"]);
            if that || self.eat_words(&["of", "target"]) {
                let n = self.number()? as usize;
                if n == 0 || n > self.targets.len() {
                    return self.fail(format!("target {n} has not been declared"));
                }
                return Ok(CountExpr::TargetStat {
                    n: n - 1,
                    toughness,
                    that,
                });
            }
            return Ok(CountExpr::SourcePower);
        }
        // `plus 2 <expr>`: "2 plus the number of ...".
        if self.eat_word("plus") {
            let (n, word) = match self.next() {
                Some(Tok::Num(n)) => (n, false),
                Some(Tok::Word(w)) if NUMBER_WORDS.contains(&w.to_ascii_lowercase().as_str()) => {
                    let i = NUMBER_WORDS
                        .iter()
                        .position(|x| x.eq_ignore_ascii_case(&w))
                        .expect("number word");
                    (u32::try_from(i + 1).expect("small"), true)
                }
                _ => return self.fail("expected a number after `plus`"),
            };
            let inner = self.count_expr()?;
            return Ok(CountExpr::Plus {
                n,
                word,
                inner: Box::new(inner),
            });
        }
        let in_all = self.at_words(&["all", "graveyards"]);
        let in_each = self.at_words(&["each", "graveyard"]);
        // `attached controller graveyard creature`: "creature cards in its
        // controller's graveyard".
        let host = self.at_words(&["attached", "controller", "graveyard"]);
        if host {
            self.pos += 2;
        }
        if in_all || in_each || self.at_word("graveyard") {
            self.pos += if self.at_word("graveyard") { 1 } else { 2 };
            let f = if self.at_stmt_end()
                || !self.word_at(0).is_some_and(|w| {
                    card_type(&w).is_some()
                        || w.starts_with("non")
                        || color(&w).is_some()
                        || w == "named"
                        || w == "permanent"
                }) && !matches!(self.peek(), Some(Tok::Word(w))
                    if w.chars().next().is_some_and(char::is_uppercase))
            {
                None
            } else {
                let f = self.filter()?;
                // `graveyard permanent card` ("permanent card": CR 110.4a).
                if f.not_types == [CardType::Instant, CardType::Sorcery] {
                    self.eat_word("card");
                }
                Some(f)
            };
            return Ok(if host {
                CountExpr::HostGraveyard(f)
            } else if in_all || in_each {
                CountExpr::CardsInGraveyards {
                    filter: f,
                    each: in_each,
                }
            } else {
                CountExpr::CardsInGraveyard(f)
            });
        }
        let mut f = self.filter()?;
        if self.eat_words(&["died", "this", "turn"]) {
            return Ok(CountExpr::DiedThisTurn(f));
        }
        if self.eat_words(&["on", "battlefield"]) && f.who.is_none() {
            if f.same.is_some() {
                // "creatures named ~ on the battlefield".
                f.named_first = true;
            } else if self.eat_words(&["named", "this"]) {
                // "creature on the battlefield named ~".
                f.same = Some(SameRef::NamedThis);
            }
        }
        Ok(CountExpr::Permanents(f))
    }

    /// At `enters with N <kind> counter[s] if`: the counters are
    /// conditional (`Ability::EntersWithIf`).
    fn counter_then_if(&self) -> bool {
        let mut i = 3;
        while let Some(t) = self.peek_at(i) {
            match t {
                Tok::Word(w) if w == "if" => return true,
                Tok::Word(w) if w == "counter" || w == "counters" => {
                    return self.word_at(i + 1).as_deref() == Some("if");
                }
                _ => i += 1,
            }
            if i > 6 {
                return false;
            }
        }
        false
    }

    /// At `with counter` or `with +1/+1 counter` (a filter's counters).
    fn at_counter_qual(&self) -> bool {
        self.word_at(0).as_deref() == Some("with")
            && (self.word_at(1).as_deref() == Some("counter")
                || (matches!(self.peek_at(1), Some(Tok::Pt { signed: true, .. }))
                    && self.word_at(2).as_deref() == Some("counter"))
                // "with an oil counter on it": a named kind.
                || (self.word_at(2).as_deref() == Some("counter")
                    && self.word_at(1).is_some_and(|w| {
                        w != "loyalty" && CounterKind::NAMED.iter().any(|(n, _)| *n == w)
                    })))
    }

    fn counter_kind(&mut self) -> R<CounterKind> {
        let kind = match self.next() {
            // "+1/+1", "-0/-1": power/toughness counters (CR 122.1a).
            Some(Tok::Pt {
                power,
                toughness,
                signed: true,
            }) => match CounterKind::from_pt(power, toughness) {
                Some(k) => k,
                None => return self.fail(format!("unknown counter kind {power:+}/{toughness:+}")),
            },
            Some(Tok::Word(w)) if w == "loyalty" => CounterKind::Loyalty,
            Some(Tok::Word(w)) if w == "defense" => CounterKind::Defense,
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

    /// Whether a subject is one variable target group ("one or two target
    /// creatures each get ...").
    fn is_many_subject(&self, subjects: &[Recipient]) -> bool {
        matches!(subjects, [Recipient::Target(i)]
            if matches!(self.targets.get(*i), Some(Target::Many { .. })))
    }

    /// What a divided amount is divided among: a variable target group.
    fn divided_group(&mut self) -> R<Recipient> {
        if !self.at_many() {
            return self.unexpected("`1 to 2 targets` or `any number of targets`");
        }
        self.many_target()
    }

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
            delayed_that: self.delayed_that,
            delayed_dies: self.delayed_dies,
            in_trigger: self.in_trigger,
            it_source: self.it_source,
            in_spell: self.in_spell,
            group_named: self.group_named,
            subject: None,
            votes: self.votes.clone(),
            returned: self.returned,
            revealed: self.revealed,
            card_filter: self.card_filter,
            kicked_targets: None,
            kicked_instead: None,
        };
        if !probe.at_recipient() {
            return false;
        }
        if probe.recipient_group().is_err() {
            return false;
        }
        !probe.at_subject_verb()
    }

    /// Whether a variable target group starts here: `X target`, `1 to 2
    /// target(s)`, `any number of target(s)` (CR 601.2c).
    fn at_many(&self) -> bool {
        let target = |k: usize| matches!(self.word_at(k).as_deref(), Some("target" | "targets"));
        (self.at_word("x") && target(1))
            || (self.at_words(&["any", "number", "of"])
                && (target(3) || (self.word_at(3).as_deref() == Some("another") && target(4))))
            || (matches!(self.peek(), Some(Tok::Num(_)))
                && self.word_at(1).as_deref() == Some("to")
                && matches!(self.peek_at(2), Some(Tok::Num(_)))
                && target(3))
    }

    /// A variable target group (see `at_many`), declared as one target.
    fn many_target(&mut self) -> R<Recipient> {
        let (min, max) = if self.eat_word("x") {
            (Num::X, Some(Num::X))
        } else if self.eat_words(&["any", "number", "of"]) {
            (Num::N(0), None)
        } else {
            let a = self.number()?;
            self.expect_word("to")?;
            let b = self.number()?;
            if b <= a {
                return self.fail("`a to b target`: b must be more than a");
            }
            (Num::N(a), Some(Num::N(b)))
        };
        // "any number of other target nonland permanents you control".
        let another = self.eat_word("another");
        let target = if !another && self.eat_word("targets") {
            Target::Spec(TargetSpec::Any)
        } else {
            self.expect_word("target")?;
            let spec = self.target_spec()?;
            if another { another_target(spec) } else { spec }
        };
        let i = self.targets.len();
        self.targets.push(Target::Many {
            target: Box::new(target),
            min,
            max,
        });
        Ok(Recipient::Target(i))
    }

    /// A recipient, or `N target <spec>` declaring several.
    fn recipient_group(&mut self) -> R<Vec<Recipient>> {
        if self.at_many() {
            return Ok(vec![self.many_target()?]);
        }
        // "up to two target creatures": optional targets (CR 601.2c).
        let up_to = self.at_words(&["up", "to"])
            && (matches!(self.peek_at(2), Some(Tok::Num(_)))
                || self
                    .word_at(2)
                    .is_some_and(|w| NUMBER_WORDS.contains(&w.to_ascii_lowercase().as_str())))
            && (self.word_at(3).as_deref() == Some("target")
                || self.word_at(3).as_deref() == Some("another")
                    && self.word_at(4).as_deref() == Some("target"));
        if up_to {
            self.pos += 2;
        }
        // "up to two other target creatures you control".
        // "two other target permanents" (`2 another target permanent`).
        let counted = self.word_at(1).as_deref() == Some("target")
            || self.word_at(1).as_deref() == Some("another")
                && (up_to || self.word_at(2).as_deref() == Some("target"));
        let count = match self.peek() {
            Some(Tok::Num(n)) if counted => {
                let n = *n;
                self.pos += 1;
                n
            }
            Some(Tok::Word(w))
                if NUMBER_WORDS.contains(&w.to_ascii_lowercase().as_str()) && counted =>
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
            // "up to one other target permanent you control".
            let another = self.eat_word("another");
            self.expect_word("target")?;
            let mut spec = self.target_spec()?;
            if another {
                spec = another_target(spec);
            }
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

    /// `them`, `they`, `those creatures`, `those permanents`: a group
    /// named earlier in the ability, if here.
    fn at_those(&self) -> Option<(crate::ast::Those, usize)> {
        use crate::ast::Those;
        if self.at_words(&["those", "creatures"]) {
            Some((Those::Creatures, 2))
        } else if self.at_words(&["those", "permanents"]) {
            Some((Those::Permanents, 2))
        } else if self.at_word("them")
            || self.at_word("they") && !self.at_words(&["they", "control"])
        {
            Some((Those::Them, 1))
        } else {
            None
        }
    }

    fn recipient(&mut self) -> R<Recipient> {
        // `them`, `those creatures`: the objects a group named earlier in
        // the ability denoted (CR 608.2c).
        if let Some((t, n)) = self.at_those() {
            if self.group_named {
                self.pos += n;
                return Ok(Recipient::Those(t));
            }
            if n == 2 {
                return self.fail(
                    "`those creatures` needs a group (`each creature you control`) before it",
                );
            }
        }
        let r = self.recipient_inner()?;
        if matches!(
            r,
            Recipient::Group(_)
                | Recipient::EachCreature
                | Recipient::EachCreatureYouControl
                | Recipient::EachPermanentYouControl
        ) {
            self.group_named = true;
        }
        Ok(r)
    }

    #[allow(clippy::too_many_lines)] // one check per recipient form
    fn recipient_inner(&mut self) -> R<Recipient> {
        // "A creature dealt damage this way can't be regenerated this turn."
        if self.eat_words(&["creature", "dealt", "damage", "this", "way"]) {
            return Ok(Recipient::DamagedThisWay);
        }
        // "each opponent who doesn't control an Elf".
        if self.eat_words(&["each", "opponent", "who", "doesn't", "control"]) {
            let f = self.filter()?;
            self.groups.push(f);
            return Ok(Recipient::OpponentsWithout(
                u16::try_from(self.groups.len() - 1).map_err(|_| ParseError {
                    line: self.line(),
                    message: "too many groups".to_string(),
                })?,
            ));
        }
        if self.at_many() {
            return self.many_target();
        }
        if self.eat_words(&["each", "creature", "blocking", "it"]) {
            return Ok(Recipient::BlockingIt);
        }
        // "up to 1 target creature", "up to 2 target creatures".
        if self.at_words(&["up", "to"])
            && (matches!(self.peek_at(2), Some(Tok::Num(_)))
                || self
                    .word_at(2)
                    .is_some_and(|w| NUMBER_WORDS.contains(&w.to_ascii_lowercase().as_str())))
            && (self.word_at(3).as_deref() == Some("target")
                || self.word_at(3).as_deref() == Some("another")
                    && self.word_at(4).as_deref() == Some("target"))
        {
            let g = self.recipient_group()?;
            return match g.as_slice() {
                [one] => Ok(*one),
                [Recipient::Target(a), ..] => Ok(Recipient::Targets(*a, g.len())),
                _ => self.fail("expected targets"),
            };
        }
        // `controller of that target 1`: "that creature's controller";
        // `controller of the target 1`: "the creature's controller".
        if self.at_words(&["controller", "of", "that", "target"])
            || self.at_words(&["controller", "of", "the", "target"])
        {
            let the = self.word_at(2).as_deref() == Some("the");
            self.pos += 4;
            let n = self.number()? as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(if the {
                Recipient::TheControllerOf(n - 1)
            } else {
                Recipient::ThatControllerOf(n - 1)
            });
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
            if self.in_trigger && !self.it_source {
                return Ok(Recipient::TriggeringIt);
            }
            // In an instant or sorcery, `it` after a target is that
            // target ("... gains flying. Untap it."), unless it deals the
            // damage.
            if self.in_spell && !self.targets.is_empty() && !self.at_word("deals") {
                return Ok(Recipient::Target(self.targets.len() - 1));
            }
            // In a spell or an activated ability, `it` is the source,
            // printed "it" ("If this spell was kicked, it deals 2 damage").
            return Ok(Recipient::SourceIt);
        }
        // `this card`: the source's card ("Exile this card.").
        if self.at_words(&["this", "card"]) {
            self.pos += 2;
            return Ok(Recipient::SourceCard);
        }
        // `this creature`: the source named in full even after a mention.
        if self.at_word("this")
            && self
                .word_at(1)
                .is_some_and(|w| SELF_NOUNS.contains(&w.as_str()))
        {
            self.pos += 2;
            return Ok(Recipient::SourceNamed);
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
        if self.eat_words(&["triggering", "card"]) {
            return Ok(Recipient::TriggeringCard);
        }
        // `controller of enchanted`: "enchanted creature's controller".
        if self.eat_words(&["controller", "of", "enchanted"]) {
            return Ok(Recipient::AttachedController);
        }
        // In a triggered ability with no targets, `its controller` is the
        // triggering object's; `controller of that creature` names it.
        if (self.in_trigger && self.targets.is_empty() || self.delayed_dies)
            && self.eat_words(&["its", "controller"])
        {
            return Ok(Recipient::TriggeringController { that: false });
        }
        if self.in_trigger && self.eat_words(&["controller", "of", "that", "creature"]) {
            return Ok(Recipient::TriggeringController { that: true });
        }
        if self.in_trigger && self.eat_words(&["controller", "of", "the", "creature"]) {
            return Ok(Recipient::TheTriggeringController);
        }
        if self.in_trigger && !self.in_delayed && self.eat_words(&["the", "creature"]) {
            return Ok(Recipient::TheTriggeringObject);
        }
        // "Exile them at the beginning of the next end step." (the
        // tokens a delayed trigger is about).
        if self.in_delayed && self.eat_word("them") {
            return Ok(Recipient::SameThem);
        }
        // "Sacrifice those tokens at the beginning of the next end step."
        if self.eat_words(&["those", "tokens"]) {
            return Ok(if self.in_delayed {
                Recipient::SameTokens
            } else {
                Recipient::CreatedTokens
            });
        }
        // "Put X +1/+1 counters on each of them" after creating tokens.
        if !self.in_delayed && self.eat_words(&["each", "of", "them"]) {
            return Ok(Recipient::CreatedThem);
        }
        if self.eat_words(&["those", "cards"]) {
            if self.targets.is_empty() {
                return self.fail("`those cards` needs targets declared before it");
            }
            return Ok(Recipient::ThoseCards(0, self.targets.len()));
        }
        if self.eat_words(&["those", "targets"]) {
            if self.targets.is_empty() {
                return self.fail("`those targets` needs targets declared before it");
            }
            return Ok(Recipient::ThoseTargets(0, self.targets.len()));
        }
        if self.eat_words(&["exiled", "card"]) {
            return Ok(Recipient::ExiledCard);
        }
        if self.at_words(&["that", "card"]) {
            self.pos += 2;
            return Ok(if self.in_delayed {
                Recipient::SameCard
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
        if self.at_words(&["the", "target"]) && matches!(self.peek_at(2), Some(Tok::Num(_))) {
            self.pos += 2;
            let Some(Tok::Num(n)) = self.next() else {
                return self.fail("expected a target number after `the target`");
            };
            let n = n as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(Recipient::TheTarget(n - 1));
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
                spec = another_target(spec);
            }
            self.targets.push(spec);
            let t = self.targets.len() - 1;
            // `target creature and all other creature with same name as
            // target 1`: the target and a group related to it.
            if self.at_words(&["and", "all", "other"]) || self.at_words(&["and", "each", "other"]) {
                let save = (self.pos, self.groups.len());
                self.pos += 1;
                // `target creature card from your graveyard and all other
                // card with same name as target 1`: cards in its graveyard.
                let cards = self.at_words(&["all", "other", "card"])
                    && matches!(
                        self.targets[t],
                        Target::Spec(TargetSpec::CreatureCardInYourGraveyard)
                            | Target::GraveyardCard { .. }
                    );
                let group = if cards {
                    self.pos += 3;
                    self.filter().map(|mut f| {
                        f.all = true;
                        f.relation = Relation::Other;
                        self.groups.push(f);
                        Recipient::Group(u16::try_from(self.groups.len() - 1).unwrap_or(0))
                    })
                } else {
                    self.recipient()
                };
                if let Ok(Recipient::Group(g)) = group
                    && matches!(
                        self.groups[usize::from(g)].same,
                        Some(SameRef::SameNameAs(n) | SameRef::SharesColorWith(n)) if n == t
                    )
                {
                    return Ok(Recipient::TargetAnd(t, g));
                }
                self.pos = save.0;
                self.groups.truncate(save.1);
            }
            return Ok(Recipient::Target(t));
        }
        if another {
            return self.unexpected("`target`");
        }
        // "destroy that creature at end of combat": the object the delayed
        // trigger is about, only while it is the same object (CR 603.7c).
        if self.delayed_that && self.at_words(&["that", "creature"]) {
            self.pos += 2;
            return Ok(Recipient::SameObject);
        }
        // "If this spell was kicked, sacrifice that creature at the
        // beginning of the next end step": the target the delayed trigger
        // is about; its own ability has no targets.
        if self.in_delayed && self.at_words(&["that", "creature"]) {
            self.pos += 2;
            return Ok(Recipient::SameThat);
        }
        if self.in_delayed && self.at_words(&["that", "token"]) {
            self.pos += 2;
            return Ok(Recipient::SameToken);
        }
        // In a spell or an activated ability there is no triggering object:
        // "that creature" is the target named before.
        if !self.in_trigger
            && !self.targets.is_empty()
            && (self.at_words(&["that", "creature"]) || self.at_words(&["that", "permanent"]))
        {
            self.pos += 2;
            return Ok(Recipient::ThatTarget(self.targets.len() - 1));
        }
        // "Return target permanent to its owner's hand. Then that player
        // discards a card.": the returned card's owner.
        if let Some(i) = self.returned
            && self.at_words(&["that", "player"])
        {
            self.pos += 2;
            return Ok(Recipient::ThatOwner(i));
        }
        // Likewise "that player" after a target player ("Target player
        // discards two cards. ... it deals 3 damage to that player").
        if !self.in_trigger
            && self.at_words(&["that", "player"])
            && self.targets.last().is_some_and(|t| {
                matches!(
                    t,
                    Target::Spec(
                        TargetSpec::Player
                            | TargetSpec::Opponent
                            | TargetSpec::PlayerOrPlaneswalker
                            | TargetSpec::OpponentOrPlaneswalker
                    )
                )
            })
        {
            self.pos += 2;
            return Ok(Recipient::ThatTarget(self.targets.len() - 1));
        }
        for (ws, r) in RECIPIENTS {
            if self.at_words(ws) {
                // "each creature with flying" is a group, not "each creature";
                // so are "each creature you control of chosen type" and
                // "each creature your opponents control".
                let continues = (ws[0] == "each" || ws[0] == "all")
                    && (matches!(
                        self.word_at(ws.len()).as_deref(),
                        Some(
                            "with"
                                | "without"
                                | "of"
                                | "named"
                                | "your"
                                | "you"
                                | "defending"
                                | "dealt"
                                | "blocked"
                                | "token"
                        )
                    ) || self.word_at(ws.len()).as_deref() == Some("or")
                        && self.word_at(ws.len() + 1).as_deref() == Some("planeswalker")
                        || self.word_at(ws.len()).as_deref() == Some("they")
                        && self.word_at(ws.len() + 1).as_deref() == Some("control")
                        || (self.word_at(ws.len()).as_deref() == Some("blocking")
                        && matches!(self.word_at(ws.len() + 1).as_deref(), Some("this" | "it" | "or")))
                        // "each creature target player controls".
                        || (self.word_at(ws.len()).as_deref() == Some("target")
                            && matches!(
                                self.word_at(ws.len() + 1).as_deref(),
                                Some("player" | "opponent")
                            ))
                        || (self.word_at(ws.len()).as_deref() == Some("that")
                            && matches!(
                                self.word_at(ws.len() + 1).as_deref(),
                                Some("entered" | "dealt" | "blocked" | "was" | "player")
                            )));
                if !continues {
                    self.pos += ws.len();
                    return Ok(*r);
                }
                break;
            }
        }
        // "a creature an opponent controls chosen at random".
        if self.eat_word("random") {
            let f = self.filter()?;
            let i = if let Some(i) = self.groups.iter().position(|g| *g == f) {
                i
            } else {
                self.groups.push(f);
                self.groups.len() - 1
            };
            return Ok(Recipient::RandomOf(u16::try_from(i).map_err(|_| {
                ParseError {
                    line: self.line(),
                    message: "too many groups".to_string(),
                }
            })?));
        }
        // "each <filter>" / "all <filter>": every matching permanent.
        if self.at_word("each") || self.at_word("all") || self.at_word("every") {
            // "Return all permanents ... to their owners' hands" against
            // "Return each ... to its owner's hand"; `every`: "all" as a
            // subject too.
            let every = self.eat_word("every");
            let all = every || self.eat_word("all");
            self.eat_word("each");
            // `each single other creature you control with toxic gains
            // toxic 1`: printed one at a time, "Each other creature ...
            // gains".
            let single = !all && self.eat_word("single");
            let mut f = self.filter()?;
            f.all |= all;
            f.all_word |= every;
            f.each = single;
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

    #[allow(clippy::too_many_lines)] // one check per target form
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
                        delayed_that: false,
                        delayed_dies: false,
                        in_trigger: false,
                        it_source: false,
                        in_spell: false,
                        group_named: false,
                        subject: None,
                        votes: Vec::new(),
                        returned: None,
                        revealed: false,
                        card_filter: false,
                        kicked_targets: None,
                        kicked_instead: None,
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
                        | (Some("target"), Some("player" | "opponent"))
                        | (Some("defending" | "your"), Some("player" | "opponents"))
                        | (Some("that"), Some("was" | "entered" | "dealt" | "blocked" | "player"))
                        | (Some("other"), Some("than"))
                        | (Some("controller"), Some("of"))
                        | (Some("with" | "without"), Some("aura"))
                        | (Some("with"), Some("counter" | "toxic" | "morph"))
                        | (Some("blocked"), Some("by"))
                        | (Some("blocking"), Some("this" | "it" | "or"))
                ) || comma_type
                    || (next.as_deref() == Some("or")
                        && !matches!(
                            after.as_deref(),
                            Some("ability" | "abilities" | "player" | "players")
                        ))
                    || keyword_after
                    // "target artifact token".
                    || matches!(next.as_deref(), Some("token" | "tokens"))
                    || (matches!(next.as_deref(), Some("with"))
                        && matches!(self.peek_at(ws.len() + 1), Some(Tok::Pt { signed: true, .. })))
                    // "permanent you control with oil counter".
                    || (matches!(next.as_deref(), Some("with"))
                        && after
                            .as_deref()
                            .is_some_and(|w| CounterKind::NAMED.iter().any(|(n, _)| *n == w))
                        && self.word_at(ws.len() + 2).as_deref() == Some("counter"))
                    || (matches!(next.as_deref(), Some("with"))
                        && matches!(after.as_deref(), Some("power" | "toughness" | "mana")))
                    || next.as_deref().and_then(card_type).is_some()
                    || (matches!(next.as_deref(), Some("spell" | "spells"))
                        && !ws.ends_with(&["spell"]))
                    || (matches!(next.as_deref(), Some("card" | "cards"))
                        && matches!(after.as_deref(), Some("from" | "with" | "each")))
                    // "target permanent not named ~".
                    || (matches!(next.as_deref(), Some("named" | "not"))
                        && matches!(after.as_deref(), Some("named" | "this")));
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
            // "Beast creature you control" goes on as a filter.
            && !matches!(
                self.word_at(2).as_deref(),
                Some("you" | "an" | "your" | "defending" | "with" | "without" | "or" | "that")
            )
        {
            let sub = sub.clone();
            self.pos += 2;
            return Ok(Target::CreatureOfSubtype(sub));
        }
        // "card from a graveyard" / "instant card from your graveyard";
        // "card named ~ from your graveyard".
        let mut f = if self.at_word("card") {
            Filter::default()
        } else {
            self.filter()?
        };
        if self.eat_word("spell") || self.eat_word("spells") {
            // "target instant or sorcery spell you control".
            if f.who.is_none() && self.eat_words(&["you", "control"]) {
                f.who = Some(Who::You);
            }
            f.targeting = self.spell_targeting()?;
            return Ok(Target::Spell(f));
        }
        let named = self.at_words(&["card", "named", "this", "from"]);
        if named {
            self.pos += 2;
            f.same = Some(SameRef::NamedThis);
        }
        // "creature card with mana value X or less from your graveyard":
        // the mana value after the noun.
        // `card each with mana value 3 or less`: "one or two target
        // creature cards each with mana value 3 or less".
        let each = !named
            && f.mana_value.is_none()
            && self.at_words(&["card", "each", "with", "mana", "value"]);
        if each {
            self.pos += 2;
            f.each = true;
        }
        let mv_after = !named
            && f.mana_value.is_none()
            && (each || self.at_words(&["card", "with", "mana", "value"]));
        if mv_after {
            if !each {
                self.pos += 1;
            }
            let g = self.filter()?;
            f.mana_value = g.mana_value;
            f.status.mana_value_x = g.status.mana_value_x;
            if !self.at_word("from") {
                return self.unexpected("`from` after the card's mana value");
            }
        }
        // "creature card with infect from your graveyard": a keyword after
        // the noun.
        let mut kw_after = false;
        if !named && !mv_after && self.at_words(&["card", "with"]) {
            let at = self.pos;
            self.pos += 2;
            match self.keyword() {
                Some(k) if self.at_word("from") => {
                    f.keywords.push(k);
                    kw_after = true;
                }
                _ => self.pos = at,
            }
        }
        if named
            || mv_after
            || kw_after
            || self.at_words(&["card", "from"])
            || self.at_words(&["cards", "from"])
        {
            self.pos += if mv_after || kw_after { 1 } else { 2 };
            // `from graveyards`: several targets from any graveyards.
            if self.eat_word("graveyards") {
                return Ok(Target::GraveyardCard {
                    filter: f,
                    who: Who::Any,
                    graveyards: true,
                });
            }
            let who = if self.eat_word("your") {
                Who::You
            } else if self.eat_words(&["an", "opponent's"]) {
                Who::Opponents
            } else if self.eat_words(&["that", "player's"]) {
                // "from that player's graveyard": the triggering player's.
                f.that_player = Some(mtg_core::card::ThatPlayer::Triggering);
                Who::Any
            } else {
                self.expect_word("a")?;
                Who::Any
            };
            self.expect_word("graveyard")?;
            return Ok(Target::GraveyardCard {
                filter: f,
                who,
                graveyards: false,
            });
        }
        Ok(Target::Filter(f))
    }

    // ---- filters ----

    #[allow(clippy::too_many_lines)]
    fn filter(&mut self) -> R<Filter> {
        let mut f = Filter::default();
        let mut consumed = 0;
        let mut or_blocking = false;
        let mut named_before_who = false;
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
                    f.control_late |= self.eat_word("late");
                }
                "all" => {
                    f.all = true;
                    self.pos += 1;
                }
                "every" => {
                    f.all = true;
                    f.all_word = true;
                    self.pos += 1;
                }
                // "a permanent card": not an instant or sorcery (CR
                // 110.4a).
                "permanent"
                    if (matches!(self.word_at(1).as_deref(), Some("card" | "cards"))
                        || (self.card_filter && self.word_at(1).as_deref() == Some("with")))
                        && (f == Filter::default()
                            || (f.types.is_empty()
                                && f.not_types.is_empty()
                                && f.any_types.is_empty()
                                && !f.subtypes.is_empty())) =>
                {
                    f.not_types = vec![CardType::Instant, CardType::Sorcery];
                    self.pos += 1;
                }
                // A dig's "noncreature, nonland permanent card".
                "permanent"
                    if self.card_filter
                        && self.word_at(1).as_deref() == Some("with")
                        && f.types.is_empty()
                        && f.any_types.is_empty()
                        && !f.not_types.is_empty()
                        && !f.not_types.contains(&CardType::Instant) =>
                {
                    f.not_types.extend([CardType::Instant, CardType::Sorcery]);
                    self.pos += 1;
                }
                // "a nonland permanent card" (Restoration Seminar).
                "permanent"
                    if matches!(self.word_at(1).as_deref(), Some("card" | "cards"))
                        && f.types.is_empty()
                        && f.any_types.is_empty()
                        && f.not_types == [CardType::Land] =>
                {
                    f.not_types = vec![CardType::Land, CardType::Instant, CardType::Sorcery];
                    self.pos += 1;
                }
                "a" | "an" | "each" | "permanent" | "permanents" => self.pos += 1,
                "andor" if consumed == 0 && !f.and_or => {
                    f.and_or = true;
                    self.pos += 1;
                }
                "andlist" if consumed == 0 && !f.and_list => {
                    f.and_list = true;
                    self.pos += 1;
                }
                // "other than enchanted creature": not the source's host.
                "other" if self.at_words(&["other", "than", "enchanted"]) => {
                    f.not_attached = true;
                    self.pos += 3;
                }
                // "creature other than this creature".
                "other" if self.at_words(&["other", "than", "this"]) => {
                    f.relation = Relation::Other;
                    f.other_than_this = true;
                    self.pos += 3;
                }
                "other" | "another" => {
                    f.relation = Relation::Other;
                    self.pos += 1;
                }
                // "creatures target player controls": declares the target.
                "target"
                    if f.that_player.is_none()
                        && (self.at_words(&["target", "player", "controls"])
                            || self.at_words(&["target", "opponent", "controls"])) =>
                {
                    let opponent = self.word_at(1).as_deref() == Some("opponent");
                    self.pos += 3;
                    self.targets.push(Target::Spec(if opponent {
                        TargetSpec::Opponent
                    } else {
                        TargetSpec::Player
                    }));
                    f.that_player = Some(mtg_core::card::ThatPlayer::Target(
                        u8::try_from(self.targets.len() - 1).unwrap_or(u8::MAX),
                    ));
                    f.that_opponent = opponent;
                }
                // `that player 1 controls`, `that player or planeswalker 1
                // controls`: the 1st target, a player, or its controller.
                "that"
                    if self.at_words(&["that", "player"])
                        && (matches!(self.peek_at(2), Some(Tok::Num(_)))
                            || self.at_words(&["that", "player", "or", "planeswalker"])) =>
                {
                    self.pos += 2;
                    let planeswalker = self.eat_words(&["or", "planeswalker"]);
                    let n = self.declared_target()?;
                    self.expect_word("controls")?;
                    f.that_player = Some(mtg_core::card::ThatPlayer::TargetOrController {
                        n: u8::try_from(n).unwrap_or(u8::MAX),
                        planeswalker,
                    });
                }
                // "that player controls": the triggering player.
                "that" if self.at_words(&["that", "player", "controls"]) => {
                    f.that_player = Some(mtg_core::card::ThatPlayer::Triggering);
                    self.pos += 3;
                }
                // `controller of target 1 controls`: "that player controls",
                // the 1st target's controller.
                "controller" if self.at_words(&["controller", "of", "target"]) => {
                    self.pos += 3;
                    let n = self.declared_target()?;
                    self.expect_word("controls")?;
                    f.that_player = Some(mtg_core::card::ThatPlayer::ControllerOf(
                        u8::try_from(n).unwrap_or(u8::MAX),
                    ));
                }
                "this" if self.word_at(1).as_deref() != Some("turn") => {
                    f.relation = Relation::This;
                    self.pos += 1;
                }
                "enchanted" | "equipped" => {
                    f.enchanted = true;
                    f.enchanted_permanent =
                        w == "enchanted" && self.word_at(1).as_deref() == Some("permanent");
                    self.pos += 1;
                }
                "paired" if f.paired.is_none() => {
                    self.pos += 1;
                    f.paired = Some(self.eat_word("named"));
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
                    f.control_late |= self.eat_word("late");
                }
                "defending" if self.at_words(&["defending", "player", "controls"]) => {
                    f.who = Some(Who::Opponents);
                    f.defending = true;
                    self.pos += 3;
                    f.control_late |= self.eat_word("late");
                }
                "your" if self.at_words(&["your", "opponents", "control"]) => {
                    f.who = Some(Who::Opponents);
                    f.your_opponents = true;
                    self.pos += 3;
                    f.control_late |= self.eat_word("late");
                }
                "you" if self.at_words(&["you", "don't", "control"]) => {
                    f.who = Some(Who::Opponents);
                    f.dont_control = true;
                    self.pos += 3;
                    f.control_late |= self.eat_word("late");
                }
                "they" if self.at_words(&["they", "control"]) => {
                    f.who = Some(Who::Opponents);
                    f.they_control = true;
                    self.pos += 2;
                }
                "with" if self.at_words(&["with", "no", "abilities"]) => {
                    f.no_abilities = true;
                    self.pos += 3;
                }
                // "with mana value X": X of the spell.
                "with"
                    if self.at_words(&["with", "mana", "value", "x"]) && f.mana_value.is_none() =>
                {
                    self.pos += 4;
                    let cmp = if self.eat_words(&["or", "less"]) {
                        Cmp::AtMost
                    } else if self.eat_words(&["or", "greater"]) {
                        Cmp::AtLeast
                    } else {
                        Cmp::Exactly
                    };
                    f.mana_value = Some((cmp, 0));
                    f.status.mana_value_x = true;
                }
                // Converge: "with mana value less than or equal to the
                // number of colors of mana spent to cast this spell".
                "with"
                    if !f.status.spent_colors
                        && (self.at_words(&["with", "mana", "value", "colors", "spent"])
                            || self.at_words(&["with", "power", "colors", "spent"])) =>
                {
                    self.pos += 1;
                    if self.eat_word("power") {
                        f.power = Some((Cmp::AtMost, 0));
                    } else {
                        self.expect_words(&["mana", "value"])?;
                        f.mana_value = Some((Cmp::AtMost, 0));
                    }
                    self.expect_words(&["colors", "spent", "or", "less"])?;
                    f.status.spent_colors = true;
                }
                // "with three or more level counters on it".
                "with"
                    if f.status.counter.is_none()
                        && matches!(self.peek_at(1), Some(Tok::Num(_)))
                        && self.word_at(2).as_deref() == Some("or")
                        && self.word_at(3).as_deref() == Some("more") =>
                {
                    self.pos += 1;
                    let n = self.count()?;
                    self.expect_words(&["or", "more"])?;
                    let k = self.counter_kind()?;
                    f.status.counter = Some(mtg_core::card::CounterQual::AtLeast(k, n));
                }
                // "with a +1/+1 counter on it", "with a counter on it".
                "with" if f.status.counter.is_none() && self.at_counter_qual() => {
                    self.pos += 1;
                    f.status.counter = Some(if self.eat_word("counter") {
                        mtg_core::card::CounterQual::Any
                    } else {
                        mtg_core::card::CounterQual::Kind(self.counter_kind()?)
                    });
                }
                // "with toxic" (CR 702.164), not "with toxic 1".
                "with"
                    if self.word_at(1).as_deref() == Some("toxic")
                        && !matches!(self.peek_at(2), Some(Tok::Num(_)))
                        && !f.status.toxic =>
                {
                    f.status.toxic = true;
                    self.pos += 2;
                }
                // "with a morph ability" (CR 702.37).
                "with" if self.word_at(1).as_deref() == Some("morph") && !f.status.morph => {
                    f.status.morph = true;
                    self.pos += 2;
                }
                "with" | "without"
                    if self.word_at(1).as_deref() == Some("aura")
                        && f.status.enchanted.is_none() =>
                {
                    f.status.enchanted = Some(w == "with");
                    self.pos += 2;
                    if w == "with" && self.eat_word("late") {
                        f.enchanted_late = true;
                    }
                }
                // "an equipped creature" (`creature with Equipment`).
                "with" if self.word_at(1).as_deref() == Some("equipment") && !f.status.equipped => {
                    f.status.equipped = true;
                    self.pos += 2;
                    if self.eat_word("late") {
                        f.equipped_late = true;
                    }
                }
                "blocked" | "unblocked"
                    if self.word_at(1).as_deref() != Some("by") && f.status.blocked.is_none() =>
                {
                    f.status.blocked = Some(w == "blocked");
                    self.pos += 1;
                }
                "dealt" if self.at_words(&["dealt", "damage", "by", "this", "this", "turn"]) => {
                    f.status.damaged_by_source = true;
                    self.pos += 6;
                }
                "dealt" if self.at_words(&["dealt", "damage", "this", "turn"]) => {
                    f.damaged = true;
                    f.damaged_short = true;
                    self.pos += 4;
                }
                "that" if self.at_words(&["that", "entered", "this", "turn"]) => {
                    f.status.entered_this_turn = true;
                    self.pos += 4;
                }
                "that" if self.at_words(&["that", "dealt", "damage", "this", "turn"]) => {
                    f.status.dealt_damage = true;
                    self.pos += 5;
                }
                "that" if self.at_words(&["that", "blocked", "this", "turn"]) => {
                    f.status.blocked_this_turn = true;
                    self.pos += 4;
                }
                "that" if self.at_words(&["that", "attacked", "this", "turn"]) => {
                    f.status.attacked_this_turn = true;
                    self.pos += 4;
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
                "modified" if !f.status.modified => {
                    f.status.modified = true;
                    self.pos += 1;
                }
                // CR 708.2: "face-down creature".
                "face-down" if !f.status.face_down => {
                    f.status.face_down = true;
                    self.pos += 1;
                }
                // CR 701.60: "suspected creature".
                "suspected" if f.status.suspected.is_none() => {
                    f.status.suspected = Some(true);
                    self.pos += 1;
                }
                // "a token", "artifact token", "nontoken creature" (CR
                // 111.1).
                "token" | "tokens" | "nontoken" if f.status.token.is_none() => {
                    f.status.token = Some(w != "nontoken");
                    self.pos += 1;
                }
                // "blocking this creature", "this creature is blocking",
                // "it's blocking", "blocking or blocked by this creature".
                "blocking" | "blocked"
                    if f.combat_with.is_none()
                        && (self.at_words(&["blocking", "this"])
                            || self.at_words(&["blocking", "it"])
                            || self.at_words(&["blocked", "by", "this"])
                            || self.at_words(&["blocked", "by", "it"])
                            || self.at_words(&["blocking", "or", "blocked", "by", "this"])) =>
                {
                    let (r, n) = if self.at_words(&["blocking", "this"]) {
                        (crate::ast::CombatRef::BlockingThis, 2)
                    } else if self.at_words(&["blocking", "it"]) {
                        (crate::ast::CombatRef::BlockingIt, 2)
                    } else if self.at_words(&["blocked", "by", "this"]) {
                        (crate::ast::CombatRef::BlockedByThis, 3)
                    } else if self.at_words(&["blocked", "by", "it"]) {
                        (crate::ast::CombatRef::BlockedByIt, 3)
                    } else {
                        (crate::ast::CombatRef::BlockingOrBlockedByThis, 5)
                    };
                    f.combat_with = Some(r);
                    self.pos += n;
                }
                "attacking" if self.at_words(&["attacking", "or", "blocking"]) => {
                    f.attacking = true;
                    or_blocking = true;
                    self.pos += 3;
                    // "face-down creature that's attacking or blocking".
                    if self.eat_word("late") {
                        f.attacking_late = true;
                    }
                }
                "attacking" => {
                    f.attacking = true;
                    self.pos += 1;
                    if self.eat_word("late") {
                        f.attacking_late = true;
                    }
                }
                "blocking" => {
                    f.blocking = true;
                    self.pos += 1;
                }
                "damaged" => {
                    f.damaged = true;
                    self.pos += 1;
                }
                // "of the chosen type", "of that type", "of the color of
                // your choice".
                "of" if f.chosen.is_none() => {
                    let of = [
                        (&["of", "chosen", "color"][..], OfChosen::ChosenColor),
                        (&["of", "chosen", "type"][..], OfChosen::ChosenType),
                        (&["of", "that", "type"][..], OfChosen::ThatType),
                        (&["of", "that", "color"][..], OfChosen::ThatColor),
                        (
                            &["of", "color", "of", "choice"][..],
                            OfChosen::ColorOfChoice,
                        ),
                        (
                            &["of", "creature", "type", "of", "choice"][..],
                            OfChosen::CreatureTypeOfChoice,
                        ),
                    ]
                    .into_iter()
                    .find(|(words, _)| self.at_words(words));
                    let Some((words, c)) = of else { break };
                    self.pos += words.len();
                    f.chosen = Some(c);
                }
                // "named ~", "not named ~": the card's own name.
                "named" if self.at_words(&["named", "this"]) && f.same.is_none() => {
                    f.same = Some(SameRef::NamedThis);
                    named_before_who = f.who.is_none();
                    self.pos += 2;
                }
                "not" if self.at_words(&["not", "named", "this"]) && f.same.is_none() => {
                    f.same = Some(SameRef::NotNamedThis);
                    named_before_who = f.who.is_none();
                    self.pos += 3;
                }
                // "with the same name as that creature", "that shares a
                // color with it".
                "with"
                    if self.at_words(&["with", "same", "name", "as", "target"])
                        && f.same.is_none() =>
                {
                    self.pos += 5;
                    let n = self.declared_target()?;
                    f.same = Some(SameRef::SameNameAs(n));
                }
                "that"
                    if (self.at_words(&["that", "shares", "color", "with", "target"])
                        || self.at_words(&["that", "share", "color", "with", "target"]))
                        && f.same.is_none() =>
                {
                    self.pos += 5;
                    let n = self.declared_target()?;
                    f.same = Some(SameRef::SharesColorWith(n));
                }
                // "with lesser power" (mentor, CR 702.134a).
                "with" if self.at_words(&["with", "lesser", "power"]) && f.same.is_none() => {
                    self.pos += 3;
                    f.same = Some(SameRef::LesserPower);
                }
                "that" if self.at_words(&["that", "was", "dealt", "damage", "this", "turn"]) => {
                    f.damaged = true;
                    self.pos += 6;
                }
                // `attached to this`, `attached to attached`, `attached to
                // target creature`, `attached to that target 1`.
                // `Aura attached to creature`: "an Aura attached to a
                // creature" (any creature, CR 303.4).
                "attached"
                    if (self.at_words(&["attached", "to", "creature"])
                        || self.at_words(&["attached", "to", "land"]))
                        && f.status.on.is_none() =>
                {
                    f.status.on = Some(if self.word_at(2).as_deref() == Some("land") {
                        mtg_core::card::OnHost::Land
                    } else {
                        mtg_core::card::OnHost::Creature
                    });
                    self.pos += 3;
                }
                "attached" if self.at_words(&["attached", "to"]) && f.attached_to.is_none() => {
                    self.pos += 2;
                    f.attached_to = Some(if self.eat_word("this") {
                        AttachedRef::Source
                    } else if self.in_trigger && self.eat_words(&["that", "creature"]) {
                        AttachedRef::TriggeringObject
                    } else if self.eat_word("attached")
                        || self.eat_words(&["enchanted", "creature"])
                        || self.eat_words(&["equipped", "creature"])
                    {
                        AttachedRef::Host
                    } else {
                        match self.recipient()? {
                            Recipient::Target(n) => AttachedRef::Target(n),
                            Recipient::ThatTarget(n) => AttachedRef::ThatTarget(n),
                            _ => return self.fail("expected `this`, `attached`, or a target"),
                        }
                    });
                }
                "multicolored" | "monocolored" | "colorless" => {
                    f.color_count = Some(match w.as_str() {
                        "multicolored" => ColorCount::Multicolored,
                        "monocolored" => ColorCount::Monocolored,
                        _ => ColorCount::Colorless,
                    });
                    self.pos += 1;
                }
                // "green or white creature"; "green creature or white
                // creature" is two alternatives.
                "or" if self.word_at(1).as_deref().and_then(color).is_some()
                    && f.colors.is_some()
                    && f.types.is_empty()
                    && f.subtypes.is_empty() =>
                {
                    self.pos += 1;
                }
                // "enchanted creature or enchantment creature": a type
                // followed by another starts a filter of its own.
                "or" if self.word_at(1).as_deref().and_then(card_type).is_some()
                    && self.word_at(2).as_deref().and_then(card_type).is_none()
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
                    // "another creature or an artifact": the alternative
                    // with an article of its own doesn't take "another".
                    let own_article = matches!(self.word_at(0).as_deref(), Some("a" | "an"));
                    match self.filter() {
                        Ok(mut alt) if f.or.is_none() => {
                            crate::format::share_qualifiers(&mut f, &mut alt);
                            if own_article && f.relation == Relation::Other {
                                alt.relation = Relation::Any;
                            } else if own_article {
                                f.alt_article = true;
                            }
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
                // `... you control with exceptions`: the copy's, not the
                // filter's.
                "with" if self.word_at(1).as_deref() == Some("exceptions") => break,
                "with" => {
                    self.pos += 1;
                    let Some(k) = self.keyword() else {
                        return self.unexpected("a keyword after `with`");
                    };
                    f.keywords.push(k);
                    // "each creature with flying and each player": an
                    // `and` not followed by a keyword ends the filter.
                    while self.at_word("and") && self.word_at(1).as_deref() != Some("each") {
                        self.pos += 1;
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
                        } else if rest == "snow" {
                            // "nonsnow" (CR 205.4g).
                            f.not_supertypes.push(Supertype::Snow);
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
                    } else if w == "outlaw" && f.subtypes.is_empty() {
                        // "an outlaw": an Assassin, Mercenary, Pirate, Rogue,
                        // or Warlock (a batch of subtypes, CR 700.12).
                        f.subtypes.push(crate::ast::OUTLAW.to_string());
                        self.pos += 1;
                    } else if w == "historic" && f.subtypes.is_empty() {
                        // "a historic spell": an artifact, a legendary, or a
                        // Saga (CR 700.6).
                        f.subtypes.push(crate::ast::HISTORIC.to_string());
                        self.pos += 1;
                    } else if raw.chars().next().is_some_and(char::is_uppercase) {
                        // Plural subtypes are singular in a filter; a few
                        // end in "s" in the singular (Plains, Elves are
                        // "Elf" though).
                        let keep = matches!(
                            raw.as_str(),
                            "Plains"
                                | "Cyclops"
                                | "Locus"
                                | "Homunculus"
                                | "Fungus"
                                | "Octopus"
                                | "Platypus"
                                | "Walrus"
                                | "Aurochs"
                                | "Bolas"
                        ) || raw.ends_with("'s");
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
        // "creature named ~ you control": the name before the controller.
        f.named_first = named_before_who && f.who.is_some();
        // "Aura or Equipment attached to this": attached to whichever.
        if f.attached_to.is_none()
            && let Some(alt) = f.or.as_mut()
        {
            f.attached_to = alt.attached_to.take();
        }
        Ok(f)
    }

    /// `<n>` after `target`: a target declared before, 0-based.
    fn declared_target(&mut self) -> R<usize> {
        let n = self.number()? as usize;
        if n == 0 || n > self.targets.len() {
            return self.fail(format!("target {n} has not been declared"));
        }
        Ok(n - 1)
    }

    fn affects(&mut self) -> R<Affects> {
        if self.at_words(&["any", "target"]) {
            return match self.recipient()? {
                Recipient::Target(i) => Ok(Affects::Target(i)),
                _ => unreachable!(),
            };
        }
        // `1 to 2 target creature`, `any number of targets`, `up to 2
        // target creature`.
        if self.at_many()
            || (self.at_words(&["up", "to"]) && self.word_at(3).as_deref() == Some("target"))
        {
            return match self.recipient()? {
                Recipient::Target(i) => Ok(Affects::Target(i)),
                Recipient::Targets(a, n) => Ok(Affects::Targets(a, n)),
                _ => self.fail("expected targets".to_string()),
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
        if self.at_word("target") || self.at_words(&["that", "target"]) {
            return match self.recipient()? {
                Recipient::Target(i) => Ok(Affects::Target(i)),
                Recipient::ThatTarget(i) => Ok(Affects::ThatTarget(i)),
                _ => unreachable!(),
            };
        }
        // `chosen red source`: "a red source of your choice" (CR 609.7a).
        if self.eat_word("chosen") {
            if self.eat_word("source") {
                return Ok(Affects::Chosen(Filter::default()));
            }
            let f = self.filter()?;
            self.eat_word("source");
            return Ok(Affects::Chosen(f));
        }
        let f = self.filter()?;
        // `creature except target creature`: "creatures other than target
        // creature".
        if self.at_words(&["except", "target"]) {
            self.pos += 1;
            return match self.recipient()? {
                Recipient::Target(i) => Ok(Affects::OtherThanTarget(f, i)),
                _ => unreachable!(),
            };
        }
        Ok(Affects::Objects(f))
    }

    // ---- triggers, conditions, costs, replacements, modifications ----

    fn trigger_obj(&mut self) -> R<TriggerObj> {
        if self.at_word("this")
            && !matches!(self.word_at(1).as_deref(), Some("creature" | "permanent"))
        {
            self.pos += 1;
            // "this creature or another Ally you control".
            if self.eat_word("or") {
                return Ok(TriggerObj::ThisOr(self.filter()?));
            }
            return Ok(TriggerObj::This);
        }
        // `whenever a creature dies`: the article is the older wording.
        let article = matches!(self.word_at(0).as_deref(), Some("a" | "an"));
        let mut f = self.filter()?;
        if article && self.at_word("dies") {
            f.article = true;
        }
        Ok(TriggerObj::Filter(f))
    }

    /// The rest of `blocks a creature`, `becomes blocked by a creature`:
    /// the other creature of the pair.
    fn block_pair(&mut self, obj: TriggerObj, blocks: bool, blocked: bool) -> R<Trigger> {
        let any = self.eat_words(&["one", "or", "more"]);
        if !(any || self.eat_word("a") || self.eat_word("an")) {
            return self.unexpected("`a`, `an`, or `one or more`");
        }
        let other = Box::new(self.filter()?);
        Ok(Trigger::BlockPair {
            obj,
            other,
            blocks,
            blocked,
            any,
        })
    }

    /// A trigger, or two joined by `and` ("When this creature enters and
    /// whenever you cast ...").
    fn trigger(&mut self) -> R<Trigger> {
        let first = self.trigger_one()?;
        if self.at_words(&["and", "when"]) || self.at_words(&["and", "whenever"]) {
            self.pos += 1;
            let when = self.at_word("when");
            let mut second = self.trigger_one()?;
            if when && !crate::format::natural_when(&second) {
                second = Trigger::When(Box::new(second));
            }
            return Ok(Trigger::Either(Box::new(first), Box::new(second)));
        }
        Ok(first)
    }

    /// The ordinal of `your second spell each turn` and kin, after `you
    /// cast` / `an opponent casts`.
    fn spell_ordinal(&mut self) -> R<Option<mtg_core::card::SpellOrdinal>> {
        use mtg_core::card::SpellOrdinal;
        if self.eat_words(&["a", "spell", "during", "an", "opponent's", "turn"]) {
            return Ok(Some(SpellOrdinal::OnOpponentsTurn));
        }
        if self.eat_words(&["a", "spell", "from", "exile"]) {
            return Ok(Some(SpellOrdinal::FromExile));
        }
        if self.eat_words(&["a", "spell", "other", "than"]) {
            if !(self.eat_word("your") || self.eat_word("their")) {
                return self.unexpected("`your` or `their`");
            }
            self.expect_words(&["first", "spell", "each", "turn"])?;
            return Ok(Some(SpellOrdinal::NotFirst));
        }
        if !(self.at_word("your") || self.at_word("their")) {
            return Ok(None);
        }
        self.pos += 1;
        if self.eat_words(&["first", "noncreature", "spell", "each", "turn"]) {
            return Ok(Some(SpellOrdinal::FirstNoncreature));
        }
        let w = self.word()?;
        self.expect_word("spell")?;
        let nth = match w.as_str() {
            "first" if self.eat_words(&["during", "each", "opponent's", "turn"]) => {
                return Ok(Some(SpellOrdinal::FirstOnOpponentsTurn));
            }
            "first" => SpellOrdinal::First,
            "second" => SpellOrdinal::Second,
            _ => return self.fail(format!("expected `first` or `second`, found `{w}`")),
        };
        self.expect_words(&["each", "turn"])?;
        Ok(Some(nth))
    }

    #[allow(clippy::too_many_lines)]
    fn trigger_one(&mut self) -> R<Trigger> {
        // CR 714.2: `chapter 1, 2`.
        if self.eat_word("chapter") {
            let mut chapters = Vec::new();
            loop {
                let n = self.number()?;
                let Some(n) = u8::try_from(n).ok().filter(|n| (1..=15).contains(n)) else {
                    return self.unexpected("a chapter number from 1 to 15");
                };
                chapters.push(n);
                if self.peek() != Some(&Tok::Comma) {
                    break;
                }
                self.pos += 1;
            }
            return Ok(Trigger::Chapter(chapters));
        }
        if self.eat_word("at") {
            // CR 511.2: "at end of combat" triggers as the end of combat
            // step begins.
            if self.eat_words(&["end", "of", "combat"]) {
                return Ok(Trigger::BeginningOfStep {
                    step: Step::EndOfCombat,
                    whose: Who::Any,
                });
            }
            self.eat_word("the");
            self.expect_word("beginning")?;
            self.expect_word("of")?;
            if self.eat_word("every") {
                let w = self.word()?;
                let Some(step) = step_by_name(&w) else {
                    return self.fail(format!("unknown step `{w}`"));
                };
                self.eat_word("step");
                self.eat_word("main");
                return Ok(Trigger::EveryStep(step));
            }
            if self.eat_words(&["enchanted", "controller's"]) {
                let w = self.word()?;
                let Some(step) = step_by_name(&w) else {
                    return self.fail(format!("unknown step `{w}`"));
                };
                self.eat_word("step");
                return Ok(Trigger::AttachedStep(step));
            }
            // `each opponent's` before `each`, which would take its first
            // word (the formatter writes `each opponent's upkeep step`).
            let whose = if self.eat_word("your") {
                Who::You
            } else if self.eat_words(&["each", "opponent's"])
                || self.eat_words(&["an", "opponent's"])
            {
                Who::Opponents
            } else if self.eat_words(&["each", "player's"])
                || self.eat_word("each")
                || self.eat_word("the")
                || self.eat_words(&["the", "next"])
            {
                self.eat_word("next");
                Who::Any
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
            self.eat_word("main");
            self.eat_word("phase");
            return Ok(Trigger::BeginningOfStep { step, whose });
        }
        let whenever = self.at_word("whenever");
        if !(self.eat_word("when") || self.eat_word("whenever")) {
            return self.unexpected("`when`, `whenever`, or `at`");
        }
        // `when next you cast instant or sorcery spell`: "When you next
        // cast an instant or sorcery spell" (a delayed trigger, once).
        if !whenever && self.at_words(&["next", "you", "cast"]) {
            // Read the rest as `whenever you cast ...`.
            self.toks[self.pos].tok = Tok::Word("whenever".to_string());
            let inner = self.trigger_one()?;
            if !matches!(
                inner,
                Trigger::CastsSpell { .. } | Trigger::CastsFiltered { .. }
            ) {
                return self.fail("`when next` takes a `you cast` trigger");
            }
            return Ok(Trigger::Next(Box::new(inner)));
        }
        // "When you control no Islands", "When you control a Dwarf": a
        // state trigger (CR 603.8).
        if !whenever && self.at_words(&["you", "control"]) {
            return Ok(Trigger::State(self.static_cond()?));
        }
        if self.at_words(&["you", "gain", "life"]) {
            self.pos += 3;
            if self.eat_words(&["first", "time", "each", "turn"]) {
                return Ok(Trigger::GainsLifeFirst { whose: Who::You });
            }
            return Ok(Trigger::GainsLife { whose: Who::You });
        }
        if self.eat_words(&["you", "unlock", "this", "door"]) {
            return Ok(Trigger::UnlocksDoor);
        }
        if self.eat_words(&["this", "becomes", "level"]) {
            let n = self.number()?;
            let Some(level) = u8::try_from(n).ok().filter(|n| (2..=9).contains(n)) else {
                return self.fail("a Class level is 2 to 9");
            };
            return Ok(Trigger::BecomesLevel(level));
        }
        if self.eat_words(&["you", "fully", "unlock", "a", "Room"]) {
            return Ok(Trigger::FullyUnlocks);
        }
        if self.at_words(&["you", "win", "a", "coin", "flip"]) {
            self.pos += 5;
            return Ok(Trigger::WinsFlip { whose: Who::You });
        }
        if self.at_words(&["you", "roll", "dice"]) {
            self.pos += 3;
            return Ok(Trigger::RollsDice { whose: Who::You });
        }
        if self.at_words(&["day", "becomes", "night", "or", "night", "becomes", "day"]) {
            self.pos += 7;
            return Ok(Trigger::DayNight);
        }
        if self.at_words(&["a", "player", "rolls", "dice"]) {
            self.pos += 4;
            return Ok(Trigger::RollsDice { whose: Who::Any });
        }
        // CR 701.21: "you sacrifice another permanent", "a player
        // sacrifices a creature", "you sacrifice this".
        if self.at_words(&["you", "sacrifice"])
            || self.at_words(&["a", "player", "sacrifices"])
            || self.at_words(&["an", "opponent", "sacrifices"])
        {
            let whose = if self.eat_words(&["you", "sacrifice"]) {
                Who::You
            } else if self.eat_words(&["a", "player", "sacrifices"]) {
                Who::Any
            } else {
                self.pos += 3;
                Who::Opponents
            };
            let obj = self.trigger_obj()?;
            return Ok(Trigger::Sacrificed { obj, whose });
        }
        if self.at_words(&["a", "player", "gains", "life"]) {
            self.pos += 4;
            return Ok(Trigger::GainsLife { whose: Who::Any });
        }
        // "you draw your second card each turn".
        if self.at_words(&["you", "draw", "your"])
            || self.at_words(&["an", "opponent", "draws", "their"])
        {
            let whose = if self.eat_words(&["you", "draw", "your"]) {
                Who::You
            } else {
                self.pos += 4;
                Who::Opponents
            };
            let w = self.word()?;
            let n = match w.as_str() {
                "first" => 1,
                "second" => 2,
                "third" => 3,
                _ => return self.fail(format!("expected an ordinal, found `{w}`")),
            };
            self.expect_word("card")?;
            let in_a_turn = self.eat_words(&["in", "a", "turn"]);
            if !in_a_turn {
                self.expect_words(&["each", "turn"])?;
            }
            return Ok(Trigger::DrawsNth {
                whose,
                n,
                in_a_turn,
            });
        }
        // "you draw a card", "a player draws a card" (CR 121.2).
        for (words, whose) in [
            (&["you", "draw", "a", "card"][..], Who::You),
            (&["a", "player", "draws", "a", "card"][..], Who::Any),
            (
                &["an", "opponent", "draws", "a", "card"][..],
                Who::Opponents,
            ),
        ] {
            if self.eat_words(words) {
                return Ok(Trigger::Draws { whose });
            }
        }
        // "a player discards a card", "you discard creature card" (CR
        // 701.9).
        for (words, whose) in [
            (&["you", "discard"][..], Who::You),
            (&["a", "player", "discards"][..], Who::Any),
            (&["an", "opponent", "discards"][..], Who::Opponents),
        ] {
            if self.eat_words(words) {
                let filter = if self.eat_words(&["a", "card"]) {
                    Filter::default()
                } else {
                    let f = self.filter()?;
                    self.expect_word("card")?;
                    f
                };
                return Ok(Trigger::Discards { whose, filter });
            }
        }
        if self.eat_words(&["you", "collect", "evidence"]) {
            return Ok(Trigger::CollectsEvidence);
        }
        if self.eat_words(&["you", "forage"]) {
            return Ok(Trigger::Forages);
        }
        // CR 122.6: "you put one or more +1/+1 counters on a creature you
        // control".
        if self.eat_words(&["you", "put", "one", "or", "more"]) {
            let kind = if self.eat_word("counters") {
                None
            } else {
                let k = self.counter_kind()?;
                self.eat_word("counters");
                Some(k)
            };
            self.expect_word("on")?;
            let obj = if self.eat_word("this") {
                TriggerObj::This
            } else {
                TriggerObj::Filter(self.filter()?)
            };
            return Ok(Trigger::YouPutCounters { obj, kind });
        }
        if self.eat_words(&[
            "a",
            "source",
            "you",
            "control",
            "deals",
            "noncombat",
            "damage",
            "to",
            "an",
            "opponent",
        ]) {
            return Ok(Trigger::NoncombatDamageToOpponent {
                yours: true,
                batch: false,
            });
        }
        if self.eat_words(&["an", "opponent", "is", "dealt", "noncombat", "damage"]) {
            return Ok(Trigger::NoncombatDamageToOpponent {
                yours: false,
                batch: false,
            });
        }
        if self.eat_words(&[
            "one",
            "or",
            "more",
            "opponents",
            "are",
            "dealt",
            "noncombat",
            "damage",
        ]) {
            return Ok(Trigger::NoncombatDamageToOpponent {
                yours: false,
                batch: true,
            });
        }
        // CR 700.13.
        if self.eat_words(&["you", "commit", "a", "crime"]) {
            let your_turn = self.eat_words(&["during", "your", "turn"]);
            return Ok(Trigger::CommitsCrime { your_turn });
        }
        if self.eat_words(&["you", "turn", "a", "permanent", "face", "up"]) {
            return Ok(Trigger::YouTurnFaceUp);
        }
        // CR 700.14: "you expend 4".
        if self.eat_words(&["you", "expend"]) {
            let n = self.number()?;
            return Ok(Trigger::Expends(u8::try_from(n).unwrap_or(u8::MAX)));
        }
        // CR 122.1: "one or more +1/+1 counters are put on this creature".
        if self.eat_words(&["one", "or", "more", "creatures", "you", "control", "attack"]) {
            return Ok(Trigger::CreaturesYouControlAttack);
        }
        if self.at_words(&["one", "or", "more"]) && matches!(self.peek_at(3), Some(Tok::Pt { .. }))
        {
            self.pos += 3;
            let kind = self.counter_kind()?;
            self.eat_word("counters");
            self.expect_words(&["put", "on"])?;
            let obj = if self.eat_word("this") {
                TriggerObj::This
            } else {
                TriggerObj::Filter(self.filter()?)
            };
            return Ok(Trigger::CountersPut { obj, kind });
        }
        // "one or more cards leave your graveyard", "one or more creature
        // cards leave your graveyard" (CR 603.2c).
        if self.at_words(&["one", "or", "more"]) {
            let before = self.pos;
            self.pos += 3;
            let filter = if self.at_word("cards") {
                Filter::default()
            } else {
                self.filter()?
            };
            if self.eat_words(&["cards", "leave", "your", "graveyard"]) {
                return Ok(Trigger::LeavesGraveyard(filter));
            }
            self.pos = before;
        }
        // CR 603.2c: "one or more artifacts you control enter".
        if self.eat_words(&["one", "or", "more"]) {
            let f = self.filter()?;
            // "one or more other creatures die", "... leave the
            // battlefield" (CR 603.10a).
            if self.eat_word("die") {
                return Ok(Trigger::LeavesBatch {
                    filter: f,
                    dies: true,
                });
            }
            if self.eat_words(&["leave", "the", "battlefield"]) {
                return Ok(Trigger::LeavesBatch {
                    filter: f,
                    dies: false,
                });
            }
            // "one or more creatures you control deal combat damage to a
            // player" (CR 603.2c).
            if self.eat_words(&["deal", "combat", "damage", "to", "a", "player"]) {
                return Ok(Trigger::CreaturesDealCombatDamage(f));
            }
            self.expect_word("enter")?;
            return Ok(Trigger::EntersBatch(f));
        }
        // CR 603.2: "When you cast this spell", "When you cast or cycle ~".
        if self.eat_words(&["you", "cast", "this"]) {
            self.eat_word("spell");
            return Ok(Trigger::CastThis { or_cycle: false });
        }
        if self.eat_words(&["you", "cast", "or", "cycle", "this"]) {
            return Ok(Trigger::CastThis { or_cycle: true });
        }
        // CR 701.21: "Whenever you sacrifice another Eldrazi".
        if self.eat_words(&["you", "sacrifice"]) {
            return Ok(Trigger::Sacrifices(self.filter()?));
        }
        // CR 702.29: "you cycle this card", "you cycle or discard a card".
        if self.eat_words(&["you", "cycle"]) {
            if self.eat_word("this") {
                self.eat_word("card");
                return Ok(Trigger::Cycles {
                    this: true,
                    another: false,
                    or_discard: false,
                });
            }
            let or_discard = self.eat_words(&["or", "discard"]);
            let another = if self.eat_word("another") {
                true
            } else {
                self.expect_word("a")?;
                false
            };
            self.expect_word("card")?;
            return Ok(Trigger::Cycles {
                this: false,
                another,
                or_discard,
            });
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
            // Magecraft: `you cast or copy instant or sorcery spell`.
            let or_copy = whose == Who::You && self.eat_words(&["or", "copy"]);
            if or_copy {
                self.eat_word("an");
            }
            if or_copy && !self.at_words(&["instant", "or", "sorcery", "spell"]) {
                return self.unexpected("`instant or sorcery spell` after `cast or copy`");
            }
            if let Some(nth) = self.spell_ordinal()? {
                return Ok(Trigger::CastsNth { whose, nth });
            }
            self.eat_word("a");
            self.eat_word("an");
            // Heroic: "a spell that targets this creature" (a filter of
            // the spell's targets).
            if self.at_words(&["spell", "that", "targets"]) {
                self.pos += 1;
                let filter = Filter {
                    targeting: self.spell_targeting()?,
                    ..Filter::default()
                };
                return Ok(Trigger::CastsFiltered { whose, filter });
            }
            // Repartee: "an instant or sorcery spell that targets a
            // creature" is a filter with targets.
            let targets_after = |p: &Self, n: usize| {
                p.word_at(n).as_deref() == Some("that")
                    && p.word_at(n + 1).as_deref() == Some("targets")
            };
            let simple = (self.at_word("spell") && !targets_after(self, 1))
                || self.at_words(&["noncreature", "spell"])
                || (self.at_words(&["instant", "or", "sorcery", "spell"])
                    && !targets_after(self, 4));
            if !simple {
                let mut filter = self.filter()?;
                self.expect_word("spell")?;
                filter.targeting = self.spell_targeting()?;
                return Ok(Trigger::CastsFiltered { whose, filter });
            }
            let noncreature = self.eat_word("noncreature");
            let instant_or_sorcery = self.eat_words(&["instant", "or", "sorcery"]);
            self.expect_word("spell")?;
            return Ok(Trigger::CastsSpell {
                whose,
                noncreature,
                instant_or_sorcery,
                or_copy,
            });
        }
        // "Whenever you attack [with three or more creatures]" (CR 508.1m).
        if self.eat_words(&["you", "attack"]) {
            let min = if self.eat_word("with") {
                let n = self.number()?;
                self.expect_words(&["or", "more", "creatures"])?;
                n
            } else {
                1
            };
            return Ok(Trigger::AttacksWith {
                this: false,
                min: u8::try_from(min).unwrap_or(u8::MAX),
            });
        }
        // Battalion: "this and at least 2 other creatures attack".
        if self.eat_words(&["this", "and", "at", "least"]) {
            let n = self.number()?;
            self.expect_words(&["other", "creatures", "attack"])?;
            return Ok(Trigger::AttacksWith {
                this: true,
                min: u8::try_from(n + 1).unwrap_or(u8::MAX),
            });
        }
        let obj = self.trigger_obj()?;
        // "a land card is put into your graveyard from anywhere".
        if let TriggerObj::Filter(f) = &obj
            && self.eat_words(&[
                "card",
                "is",
                "put",
                "into",
                "your",
                "graveyard",
                "from",
                "anywhere",
            ])
        {
            return Ok(Trigger::PutIntoGraveyard(f.clone()));
        }
        // "a creature enters from your graveyard".
        if let TriggerObj::Filter(f) = &obj
            && self.eat_words(&["enters", "from", "your", "graveyard"])
        {
            return Ok(Trigger::EntersFromGraveyard(f.clone()));
        }
        let first = self.trigger_verb(obj.clone(), whenever)?;
        // "this enters or dies", "this enters or attacks": one ability,
        // two events of the same object.
        if first == Trigger::Enters(TriggerObj::This) && self.eat_word("or") {
            let second = self.trigger_verb(obj, whenever)?;
            if !matches!(
                second,
                Trigger::Dies(TriggerObj::This)
                    | Trigger::Leaves(TriggerObj::This)
                    | Trigger::Attacks(TriggerObj::This)
                    | Trigger::TurnedFaceUp(TriggerObj::This)
            ) {
                return self.fail("`this enters or` takes `dies`, `leaves the battlefield`, `attacks`, or `is turned face up`");
            }
            return Ok(Trigger::Or {
                first: Box::new(first),
                second: Box::new(second),
                whenever,
            });
        }
        Ok(first)
    }

    /// The event after a trigger's subject: `enters`, `dies`, `attacks`, ...
    #[allow(clippy::too_many_lines)] // one arm per trigger event
    fn trigger_verb(&mut self, obj: TriggerObj, whenever: bool) -> R<Trigger> {
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
            "becomes" if obj == TriggerObj::This && self.at_words(&["plotted"]) => {
                self.pos += 1;
                Ok(Trigger::BecomesPlotted)
            }
            "is" if self.at_words(&["turned", "face", "up"]) => {
                self.pos += 3;
                Ok(Trigger::TurnedFaceUp(obj))
            }
            "is" if self.at_words(&["dealt", "combat", "damage"]) => {
                self.pos += 3;
                Ok(Trigger::DealtCombatDamage(obj))
            }
            "is" if self.at_words(&["tapped", "for", "mana"]) => {
                self.pos += 3;
                Ok(Trigger::TappedForMana(obj))
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
            "leaves" if self.at_words(&["the", "battlefield"]) => {
                self.pos += 2;
                Ok(Trigger::Leaves(obj))
            }
            "attacks" if self.at_words(&["or", "blocks"]) => {
                self.pos += 2;
                Ok(Trigger::AttacksOrBlocks(obj))
            }
            "attacks" => {
                if self.eat_word("alone") {
                    return Ok(Trigger::AttacksAlone(obj));
                }
                if self.eat_words(&["for", "the", "first", "time", "each", "turn"]) {
                    return Ok(Trigger::AttacksFirstTime(obj));
                }
                if self.eat_words(&["and", "isn't", "blocked"]) {
                    return Ok(Trigger::AttacksUnblocked(obj));
                }
                if self.eat_words(&["a", "battle"]) {
                    return Ok(Trigger::AttacksBattle(obj));
                }
                Ok(Trigger::Attacks(obj))
            }
            "blocks" if self.at_words(&["or", "becomes", "blocked"]) => {
                self.pos += 3;
                if self.eat_word("by") {
                    return self.block_pair(obj, true, true);
                }
                Ok(Trigger::BlocksOrBlocked(obj))
            }
            "blocks" if self.at_word("a") || self.at_word("an") => {
                self.block_pair(obj, true, false)
            }
            "blocks" => Ok(Trigger::Blocks(obj)),
            "deals" => {
                if self.at_words(&["combat", "damage", "to", "a", "player", "or", "battle"]) {
                    self.pos += 7;
                    return Ok(Trigger::DealsCombatDamageToPlayerOrBattle(obj));
                }
                if self.at_words(&["combat", "damage", "to", "a", "player"]) {
                    self.pos += 5;
                    return Ok(Trigger::DealsCombatDamageToPlayer(obj));
                }
                if self.at_words(&["combat", "damage", "to", "defending", "player"]) {
                    self.pos += 5;
                    return Ok(Trigger::DealsCombatDamageToDefending(obj));
                }
                let combat = self.eat_word("combat");
                self.expect_word("damage")?;
                let to = if self.eat_word("to") {
                    self.eat_word("a");
                    self.eat_word("an");
                    // "deals damage to a Vampire": any other filter.
                    let simple = matches!(
                        self.word_at(0).as_deref(),
                        Some("creature" | "player" | "opponent")
                    ) && !matches!(self.peek_at(1), Some(Tok::Word(w)) if w != "if" && w != "while");
                    if !combat && !simple {
                        let filter = Box::new(self.filter()?);
                        return Ok(Trigger::DealsDamageTo { obj, filter });
                    }
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
            "becomes" if self.at_words(&["attached", "to", "this"]) => {
                self.pos += 3;
                let TriggerObj::Filter(f) = obj else {
                    return self.fail("expected what becomes attached (`Aura`)");
                };
                Ok(Trigger::BecomesAttachedTo(f))
            }
            "becomes" if self.at_word("untapped") => {
                self.pos += 1;
                Ok(Trigger::BecomesUntapped(obj))
            }
            "becomes" if self.at_word("blocked") => {
                self.pos += 1;
                if self.eat_word("by") {
                    return self.block_pair(obj, false, true);
                }
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
                if obj == TriggerObj::This
                    && self.eat_words(&["by", "you", "first", "time", "each", "turn"])
                {
                    return Ok(Trigger::TargetedByYouFirstTime);
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
        if self.eat_words(&["this", "is", "tapped"]) {
            return Ok(Condition::SourceTapped);
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
        if self.eat_words(&["not", "renowned"]) {
            return Ok(Condition::NotRenowned);
        }
        if self.eat_words(&["this", "isn't", "prepared"]) {
            return Ok(Condition::NotPrepared);
        }
        if self.eat_word("evolves") {
            return Ok(Condition::Evolves);
        }
        if self.eat_word("increments") {
            return Ok(Condition::Increments);
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
        if self.eat_words(&["it", "had", "counters"]) {
            return Ok(Condition::TriggerHadCounters);
        }
        if self.eat_words(&["it", "had", "no"]) {
            let k = self.counter_kind()?;
            self.eat_word("counters");
            return Ok(Condition::HadNoCounters(k));
        }
        if self.eat_words(&["it", "had"]) {
            let k = self.counter_kind()?;
            if !self.eat_word("counter") {
                self.eat_word("counters");
            }
            return Ok(Condition::HadCounters(k));
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
                // `2 times per turn`: "Activate no more than twice each
                // turn."
                Some(Tok::Num(n)) if self.word_at(1).as_deref() == Some("times") => {
                    cost.per_turn = u8::try_from(*n).unwrap_or(u8::MAX);
                    self.pos += 2;
                    self.eat_words(&["per", "turn"]);
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
                            // `sacrifice 2 Goblin`.
                            if let Some(Tok::Num(n)) = self.peek() {
                                cost.count = *n;
                                self.pos += 1;
                            }
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
                    // `put 1 -1/-1 counter on this` (CR 118.3).
                    "put"
                        if (1..5).any(|k| {
                            self.word_at(k).as_deref() == Some("on")
                                && self.word_at(k + 1).as_deref() == Some("this")
                        }) =>
                    {
                        self.pos += 1;
                        let n = self.count()?;
                        let k = self.counter_kind()?;
                        self.expect_words(&["on", "this"])?;
                        cost.put_counters = Some((k, n));
                        cost.order.push(CostItem::PutCounters);
                    }
                    "return" | "tap" if !matches!(self.peek_at(1), Some(Tok::Sym(_))) => {
                        cost.chosen = if w.eq_ignore_ascii_case("return") {
                            mtg_core::card::ChosenCost::Return
                        } else {
                            mtg_core::card::ChosenCost::Tap
                        };
                        self.pos += 1;
                        // `tap 2 untapped creature you control`.
                        if let Some(Tok::Num(n)) = self.peek() {
                            cost.count = *n;
                            self.pos += 1;
                        }
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
                    // "Exile this artifact" (CR 602.2b).
                    "exile" if self.at_words(&["exile", "this"]) => {
                        self.pos += 2;
                        cost.exile_this = true;
                        cost.order.push(CostItem::Exile);
                    }
                    // `exile 2 cards from graveyard`, `exile creature card
                    // from graveyard`, `exile X creature cards from
                    // graveyard`.
                    "exile" => {
                        self.pos += 1;
                        let top = self.eat_word("top");
                        let (mut n, mut x) = (1, false);
                        if let Some(Tok::Num(k)) = self.peek() {
                            n = *k;
                            self.pos += 1;
                        } else if self.at_word("x") {
                            x = true;
                            n = 0;
                            self.pos += 1;
                        }
                        let filter = if self.at_word("card") || self.at_word("cards") {
                            None
                        } else {
                            Some(self.filter()?)
                        };
                        self.cards_word()?;
                        self.expect_words(&["from", "graveyard"])?;
                        cost.exile_cards =
                            Some(Box::new(crate::ast::ExileCards { n, x, filter, top }));
                        cost.order.push(CostItem::ExileCards);
                    }
                    "discard" => {
                        self.pos += 1;
                        if self.eat_word("this") {
                            self.eat_word("card");
                            cost.discard_self = true;
                        } else if self.at_word("x") {
                            // "Discard X cards", "Discard X creature cards"
                            // (CR 601.2b: X announced).
                            self.pos += 1;
                            if !self.at_word("cards") {
                                cost.discard_filter = Some(self.filter()?);
                            }
                            self.cards_word()?;
                            cost.discard_x = true;
                        } else if matches!(self.peek(), Some(Tok::Num(_))) {
                            cost.discard = self.count()?;
                            self.cards_word()?;
                            cost.discard_random = self.eat_words(&["at", "random"]);
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
                    "once" if self.word_at(1).as_deref() == Some("ever") => {
                        self.pos += 2;
                        cost.once_ever = true;
                        cost.restrict.push(Restrict::OnceEver);
                    }
                    "once" => {
                        self.pos += 1;
                        self.eat_words(&["per", "turn"]);
                        self.eat_words(&["each", "turn"]);
                        cost.per_turn = 1;
                        cost.restrict.push(Restrict::Once);
                    }
                    // CR 602.5b: `if <condition>`, `during your upkeep`.
                    "if" | "during" | "before" => {
                        let c = if w.eq_ignore_ascii_case("if") {
                            self.pos += 1;
                            let c = self.static_cond()?;
                            // "Activate only if this land entered this turn
                            // or if you control a basic land."
                            if self.eat_words(&["or", "if"]) {
                                StaticCond::OrIf(Box::new(c), Box::new(self.static_cond()?))
                            } else {
                                c
                            }
                        } else if self.at_words(&["during", "your", "turn"])
                            && self.peek_at(3) == Some(&Tok::Comma)
                        {
                            // `during your turn, if ...`: "Activate only
                            // during your turn and only if ...".
                            self.pos += 3;
                            StaticCond::DuringYourTurn
                        } else {
                            match self.turn_time() {
                                Some(c) => c,
                                None => return self.unexpected("a part of the turn"),
                            }
                        };
                        cost.restrict.push(Restrict::Cond(cost.conditions.len()));
                        cost.conditions.push(c);
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
                    "behold" | "may"
                        if self.word_at(0).as_deref() == Some("behold")
                            || self.word_at(1).as_deref() == Some("behold") =>
                    {
                        let optional = self.eat_word("may");
                        self.pos += 1;
                        let exile = self.eat_words(&["and", "exile"]);
                        let n = if matches!(self.peek(), Some(Tok::Num(_))) {
                            self.count()?
                        } else {
                            1
                        };
                        let filter = self.filter()?;
                        cost.behold = Some(Box::new(crate::ast::Behold {
                            filter,
                            n,
                            exile,
                            optional,
                        }));
                        cost.order.push(CostItem::Behold);
                    }
                    // CR 701.67a: `waterbend {3}`, the mana that follows.
                    "waterbend" if matches!(self.peek_at(1), Some(Tok::Sym(_))) => {
                        self.pos += 1;
                        cost.waterbend = true;
                        while let Some(Tok::Sym(s)) = self.peek() {
                            mana.push('{');
                            mana.push_str(s);
                            mana.push('}');
                            self.pos += 1;
                        }
                        cost.order.push(CostItem::Mana);
                    }
                    "blight" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                        self.pos += 1;
                        cost.blight = self.count()?;
                        cost.order.push(CostItem::Blight);
                    }
                    // CR 701.61a: `forage`.
                    "forage" => {
                        self.pos += 1;
                        cost.forage = true;
                        cost.order.push(CostItem::Forage);
                    }
                    "collect"
                        if self.word_at(1).as_deref() == Some("evidence")
                            && matches!(self.peek_at(2), Some(Tok::Num(_))) =>
                    {
                        self.pos += 2;
                        cost.evidence = self.count()?;
                        cost.order.push(CostItem::Evidence);
                    }
                    "reveal" if self.word_at(1).as_deref() == Some("hand") => {
                        self.pos += 2;
                        cost.reveal_hand = true;
                        cost.order.push(CostItem::RevealHand);
                    }
                    "sorcery" => {
                        self.pos += 1;
                        cost.sorcery = true;
                        cost.restrict.push(Restrict::Sorcery);
                    }
                    // `reduced {1} each Town you control`, `reduced {4}{B}
                    // if drew 3 cards this turn`.
                    "reduced" => {
                        self.pos += 1;
                        let mut by = String::new();
                        while let Some(Tok::Sym(s)) = self.peek() {
                            by.push('{');
                            by.push_str(s);
                            by.push('}');
                            self.pos += 1;
                        }
                        let by = ManaCost::parse(&by).map_err(|e| ParseError {
                            line: self.line(),
                            message: e.to_string(),
                        })?;
                        let how = if self.eat_word("each") {
                            crate::ast::ReduceBy::Each(self.count_expr()?)
                        } else {
                            self.expect_word("if")?;
                            crate::ast::ReduceBy::If(self.static_cond()?)
                        };
                        cost.reduce = Some(Box::new((by, how)));
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
        // `this dies`: "If this creature would die, exile it instead."
        if self.eat_words(&["this", "dies"]) {
            return Ok(Replaces::Dies { to: Affects::This });
        }
        // `creature dealt damage this way dies` (CR 608.2c).
        if self.eat_words(&["creature", "dealt", "damage", "this", "way", "dies"]) {
            return Ok(Replaces::Dies {
                to: Affects::DamagedThisWay,
            });
        }
        // `creature dealt damage by this this turn dies` (Frostwielder),
        // `<affects> dies` in general.
        if !self.at_word("damage") && !self.at_word("combat") && !self.at_word("noncombat") {
            let at = self.pos;
            if let Ok(to) = self.affects()
                && self.eat_word("dies")
            {
                return Ok(Replaces::Dies { to });
            }
            self.pos = at;
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
        self.damage_event(combat)
    }

    /// The rest of a damage event after `damage`: `to and from <affects>`,
    /// `<source> would deal [to <affects>]`, or `[from <source>] [to
    /// <affects>]` (either order). Without `to`, any recipient.
    fn damage_event(&mut self, combat: Option<bool>) -> R<Replaces> {
        if self.eat_words(&["to", "and", "from"]) {
            return Ok(Replaces::DamageToAndBy {
                what: self.affects()?,
                combat,
            });
        }
        let ends = |p: &Self| p.at_words(&["this", "turn"]) || p.at_end_of_event();
        let mut from = None;
        let mut wording = DamageWording::Dealt;
        if !self.at_word("to") && !self.at_word("from") && !ends(self) {
            from = Some(self.source()?);
            self.expect_words(&["would", "deal"])?;
            wording = DamageWording::WouldDeal;
        }
        let mut to = Affects::Any;
        loop {
            if from.is_none() && self.eat_word("from") {
                from = Some(self.source()?);
            } else if self.eat_word("to") {
                to = self.affects()?;
            } else {
                break;
            }
        }
        Ok(Replaces::Damage {
            from,
            to,
            combat,
            wording,
        })
    }

    /// Whether the next token ends a replacement's event (`:` before the
    /// outcome).
    fn at_end_of_event(&self) -> bool {
        matches!(self.peek(), Some(Tok::Colon) | None)
    }

    /// A damage source: what `affects` reads, as an object (no `from any`:
    /// leave `from` out).
    fn source(&mut self) -> R<Affects> {
        let a = self.affects()?;
        if matches!(
            a,
            Affects::Any | Affects::You | Affects::Players(_) | Affects::YouAnd(_)
        ) {
            return self.fail("a damage source is an object, not a player".to_string());
        }
        Ok(a)
    }

    fn outcome(&mut self) -> R<Outcome> {
        let w = self.word()?;
        let out = match w.as_str() {
            "prevent" => {
                if self.eat_words(&["next", "time"]) {
                    Outcome::PreventNextTime
                } else if self.eat_word("next") || self.eat_words(&["the", "next"]) {
                    Outcome::PreventNext(Num::N(self.number()?))
                } else {
                    self.eat_word("it");
                    self.eat_words(&["that", "damage"]);
                    Outcome::Prevent
                }
            }
            "double" => Outcome::Double,
            // "return it to its owner's hand instead".
            "hand" => Outcome::Instead(Zone::Hand),
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
            "umbra" => {
                self.expect_word("armor")?;
                Outcome::UmbraArmor
            }
            "redirect" => {
                let next = if self.eat_word("next") {
                    Some(self.number()?)
                } else {
                    None
                };
                self.expect_word("to")?;
                if next.is_none() && (self.eat_word("attached") || self.eat_word("enchanted")) {
                    Outcome::RedirectToAttached
                } else if self.eat_word("this") {
                    Outcome::Redirect {
                        to: RedirectTo::This,
                        next,
                    }
                } else if self.at_word("target") || self.at_words(&["another", "target"]) {
                    match self.recipient()? {
                        Recipient::Target(i) => Outcome::Redirect {
                            to: RedirectTo::Target(i),
                            next,
                        },
                        _ => return self.unexpected("`target N`"),
                    }
                } else {
                    return self.unexpected("`attached`, `this`, or `target N`");
                }
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
            "has" | "have" if self.granted_mechanic_ahead() => Ok(Mod::AddAbility(Box::new(
                self.granted_mechanic()?.expect("checked"),
            ))),
            "has" | "have" if self.eat_words(&["protection", "from", "chosen", "color"]) => {
                Ok(Mod::ProtectionFromChosen)
            }
            "has" | "have" => Ok(Mod::AddKeywords(self.keyword_list()?)),
            "loses" | "lose" if self.keyword_ahead() => {
                Ok(Mod::LosesKeywords(self.keyword_list()?))
            }
            "loses" | "lose" if self.eat_words(&["all", "creature", "types"]) => {
                Ok(Mod::NoCreatureTypes)
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
            "goaded" => Ok(Mod::Goaded),
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
                        if types.is_empty() {
                            return self.fail("expected `add type <types>`");
                        }
                        Ok(if set {
                            Mod::SetTypes(types)
                        } else {
                            Mod::AddTypes(types)
                        })
                    }
                    "land" if set && self.eat_word("chosen") => Ok(Mod::ChosenLandType),
                    "land" if !set && self.eat_word("all") => Ok(Mod::AddAllLandTypes),
                    "land" => {
                        let name = self.word()?;
                        let Some(c) = basic_land_type(&name) else {
                            return self.fail(format!("not a basic land type: {name}"));
                        };
                        // `set land Mountain Forest Plains`: all of them.
                        let mut more = vec![c];
                        while set
                            && let Some(Tok::Word(w)) = self.peek()
                            && let Some(c) = basic_land_type(w)
                        {
                            more.push(c);
                            self.pos += 1;
                        }
                        Ok(if more.len() > 1 {
                            Mod::SetLandTypes(more)
                        } else if set {
                            Mod::SetLandType(c)
                        } else {
                            Mod::AddLandType(c)
                        })
                    }
                    "chosen" if !set && self.eat_word("type") => Ok(Mod::AddChosenType),
                    "subtype" if !set && self.eat_word("every") => {
                        Ok(Mod::AllCreatureTypes { every: true })
                    }
                    "subtype" if !set && self.eat_word("all") => {
                        Ok(Mod::AllCreatureTypes { every: false })
                    }
                    "subtype" => {
                        let s = self.word()?;
                        Ok(if set {
                            Mod::SetSubtype(s)
                        } else {
                            Mod::AddSubtype(s)
                        })
                    }
                    "color" if set && self.eat_word("chosen") => Ok(Mod::ChosenColor),
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
                let noun = self.eat_word("noun");
                let what = self.word()?;
                match what.as_str() {
                    "color" | "colors" => {
                        if self.eat_word("all") {
                            return Ok(Mod::CdaColors(Color::ALL.to_vec()));
                        }
                        // "Ghostfire is colorless." (CR 604.3).
                        if self.eat_word("colorless") {
                            return Ok(Mod::CdaColors(Vec::new()));
                        }
                        Ok(Mod::CdaColors(self.color_list()?))
                    }
                    "pt" | "power" | "toughness" => {
                        use crate::ast::CountExpr;
                        use mtg_core::card::CdaSets;
                        let sets = match what.as_str() {
                            "power" => CdaSets::Power,
                            "toughness" => CdaSets::Toughness,
                            _ => CdaSets::Both,
                        };
                        let count = if self.eat_words(&["creature", "cards", "in", "graveyards"]) {
                            CountExpr::CardsInGraveyards {
                                filter: Some(Filter {
                                    types: vec![CardType::Creature],
                                    ..Filter::default()
                                }),
                                each: false,
                            }
                        } else if self.eat_words(&["your", "life"]) {
                            self.eat_word("total");
                            CountExpr::Life
                        } else if self.eat_word("count") {
                            let f = self.filter()?;
                            // "the number of creatures named ~ on the
                            // battlefield".
                            if f.who.is_none() {
                                self.eat_words(&["on", "battlefield"]);
                            }
                            CountExpr::Permanents(f)
                        } else {
                            self.count_expr()?
                        };
                        let plus_toughness = match self.peek() {
                            Some(Tok::Signed(n)) if sets == CdaSets::Both => {
                                let n = *n;
                                self.pos += 1;
                                n
                            }
                            _ => 0,
                        };
                        Ok(Mod::CdaPt {
                            count,
                            sets,
                            plus_toughness,
                            noun,
                        })
                    }
                    _ => self.fail(format!("unknown `cda {what}`")),
                }
            }
            _ => self.fail(format!("unknown modification `{w}`")),
        }
    }

    /// `and` inside a list of colors ("white and blue"), not the `and`
    /// that starts the next clause ("becomes black and gains shadow").
    fn eat_and_before_color(&mut self) {
        if self.at_word("and") && self.word_at(1).as_deref().and_then(color).is_some() {
            self.pos += 1;
        }
    }

    fn color_list(&mut self) -> R<Vec<Color>> {
        let mut out = Vec::new();
        while let Some(w) = self.word_at(0) {
            let Some(c) = color(&w) else { break };
            out.push(c);
            self.pos += 1;
            self.eat_and_before_color();
        }
        if out.is_empty() {
            return self.unexpected("a color");
        }
        Ok(out)
    }
}

/// "another target creature": the target spec, excluding the source.
fn another_target(spec: Target) -> Target {
    match spec {
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
        // "up to one other target artifact or enchantment".
        Target::Spec(TargetSpec::ArtifactOrEnchantment) => Target::Filter(Filter {
            any_types: vec![CardType::Artifact, CardType::Enchantment],
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
        Target::GraveyardCard {
            mut filter,
            who,
            graveyards,
        } => {
            filter.relation = Relation::Other;
            Target::GraveyardCard {
                filter,
                who,
                graveyards,
            }
        }
        Target::Spec(TargetSpec::CreatureCardInYourGraveyard) => Target::GraveyardCard {
            filter: Filter {
                types: vec![CardType::Creature],
                relation: Relation::Other,
                ..Default::default()
            },
            who: Who::You,
            graveyards: false,
        },
        other => other,
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
                        to: vec![Recipient::Target(0)],
                        by: Recipient::Source,
                    }])]
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_spell_that_is_only_a_modes_block_is_modal() {
        let cards = parse(
            r#"
card "Test Charm" {
  cost {B}
  type Instant
  spell: choose one {
    mode: tap target creature
    mode: you gain 1 life
  }
}
"#,
        )
        .unwrap();
        match &cards[0].abilities[0] {
            Ability::Modal(modes) => assert_eq!(modes.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_spell_that_is_only_a_choose_two_block_chooses_two() {
        let cards = parse(
            r#"
card "Test Command" {
  cost {B}
  type Instant
  spell: choose two {
    mode: tap target creature
    mode: you gain 1 life
    mode: draw 1 card
  }
}
"#,
        )
        .unwrap();
        match &cards[0].abilities[0] {
            Ability::ModalTwo(modes) => assert_eq!(modes.len(), 3),
            other => panic!("{other:?}"),
        }
        let text = crate::format::format(&cards[0]);
        assert!(text.contains("choose two {"), "{text}");
        assert_eq!(parse(&text).unwrap()[0].abilities, cards[0].abilities);
    }

    #[test]
    fn them_after_a_group_is_that_group() {
        let cards = parse(
            r#"
card "Test Takeover" {
  cost {R}
  type Sorcery
  spell: gain control of all artifact your opponents control until end of turn; untap them; they gain haste
}
"#,
        )
        .unwrap();
        let Ability::Spell { effects, .. } = &cards[0].abilities[0] else {
            panic!("{:?}", cards[0].abilities[0]);
        };
        let text = format!("{effects:?}");
        assert_eq!(text.matches("Those(Them)").count(), 2, "{text}");
        // Nothing named before: an error, not every creature you control.
        assert!(
            parse(
                r#"
card "Test Untap" {
  cost {W}
  type Instant
  spell: untap those creatures
}
"#
            )
            .is_err()
        );
    }

    #[test]
    fn it_in_a_spell_is_the_last_target_unless_it_deals_damage() {
        let cards = parse(
            r#"
card "Test Pounce" {
  cost {W}
  type Instant
  spell: target creature gains flying; untap it
}

card "Test Thunder" {
  cost {R}
  type Instant
  spell: destroy target artifact; it deals 2 damage to target creature
}
"#,
        )
        .unwrap();
        let effects = |i: usize| match &cards[i].abilities[0] {
            Ability::Spell { effects, .. } => format!("{effects:?}"),
            other => panic!("{other:?}"),
        };
        assert!(effects(0).contains("Target(0)") && !effects(0).contains("SourceIt"));
        assert!(effects(1).contains("by: SourceIt"), "{}", effects(1));
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
