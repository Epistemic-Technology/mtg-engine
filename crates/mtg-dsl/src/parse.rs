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
    Ability, ActivationSources, Affects, AttachedRef, BlockRule, Card, ChoiceKind, Cost, CostItem,
    DamageWording, DieRow, Effect, Except, Filter, InsteadCond, InsteadWording, Join, Kind,
    LibraryPlace, LibraryWording, Mechanic, Mod, Mode, ModeHead, OfChosen, Recipient, Replacement,
    Replaces, Restrict, SacCost, SameRef, Sentence, StaticCond, Target, Trigger, TriggerObj,
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
        delayed_player: None,
        delayed_dies: false,
        in_trigger: false,
        per_opponent: false,
        it_source: false,
        no_trigger_obj: false,
        self_trigger: false,
        no_trigger_player: false,
        created_at: None,
        no_trigger_amount: false,
        in_spell: false,
        group_named: false,
        subject: None,
        subject_each: false,
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
    (&["can't", "transform"], Keyword::CantTransform),
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
    (&["split", "second"], Keyword::SplitSecond),
    (&["aftermath"], Keyword::Aftermath),
    (&["fuse"], Keyword::Fuse),
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
    (
        &["may", "choose", "not", "to", "untap"],
        Keyword::MayNotUntap,
    ),
    (&["can't", "be", "equipped"], Keyword::CantBeEquipped),
    (
        &[
            "assigns",
            "combat",
            "damage",
            "equal",
            "to",
            "its",
            "toughness",
            "rather",
            "than",
            "its",
            "power",
        ],
        Keyword::AssignsToughness,
    ),
    (
        &["assigns", "combat", "damage", "equal", "to", "toughness"],
        Keyword::AssignsToughness,
    ),
    (
        &[
            "untaps", "during", "each", "other", "player's", "untap", "step",
        ],
        Keyword::UntapsDuringOthers,
    ),
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
            single: false,
            instant_sorcery: false,
            listed: false,
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
            single: false,
            instant_sorcery: false,
            listed: false,
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
            single: false,
            instant_sorcery: false,
            listed: false,
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
            single: false,
            instant_sorcery: false,
            listed: false,
        },
    ),
    (
        &[
            "instant",
            "spell",
            ",",
            "sorcery",
            "spell",
            ",",
            "activated",
            "ability",
            ",",
            "or",
            "triggered",
            "ability",
        ],
        TargetSpec::OnStack {
            spells: true,
            activated: true,
            triggered: true,
            source: None,
            yours: false,
            single: false,
            instant_sorcery: true,
            listed: true,
        },
    ),
    (
        &[
            "spell",
            ",",
            "activated",
            "ability",
            ",",
            "or",
            "triggered",
            "ability",
        ],
        TargetSpec::OnStack {
            spells: true,
            activated: true,
            triggered: true,
            source: None,
            yours: false,
            single: false,
            instant_sorcery: false,
            listed: true,
        },
    ),
    (
        &["activated", "ability", "with", "a", "single", "target"],
        TargetSpec::OnStack {
            spells: false,
            activated: true,
            triggered: false,
            source: None,
            yours: false,
            single: true,
            instant_sorcery: false,
            listed: false,
        },
    ),
    (
        &["spell", "with", "a", "single", "target"],
        TargetSpec::OnStack {
            spells: true,
            activated: false,
            triggered: false,
            source: None,
            yours: false,
            single: true,
            instant_sorcery: false,
            listed: false,
        },
    ),
    (
        &["spell", "or", "ability", "with", "a", "single", "target"],
        TargetSpec::OnStack {
            spells: true,
            activated: true,
            triggered: true,
            source: None,
            yours: false,
            single: true,
            instant_sorcery: false,
            listed: false,
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
            single: false,
            instant_sorcery: false,
            listed: false,
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
            single: false,
            instant_sorcery: false,
            listed: false,
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
            single: false,
            instant_sorcery: false,
            listed: false,
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
            single: false,
            instant_sorcery: false,
            listed: false,
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
            single: false,
            instant_sorcery: false,
            listed: false,
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
    (
        &["each", "of", "your", "opponents"],
        Recipient::EachOfYourOpponents,
    ),
    (&["each", "other", "player"], Recipient::EachOtherPlayer),
    (&["each", "player"], Recipient::EachPlayer),
    (&["each", "creature"], Recipient::EachCreature),
    (&["all", "creatures"], Recipient::EachCreature),
    (&["that", "creature"], Recipient::TriggeringObject),
    (&["that", "permanent"], Recipient::TriggeringObject),
    (&["that", "Vehicle"], Recipient::TriggeringObject),
    (&["that", "artifact"], Recipient::TriggeringObject),
    (&["that", "object"], Recipient::TriggeringObject),
    (&["that", "spell"], Recipient::TriggeringObject),
    (&["that", "player"], Recipient::TriggeringPlayer),
    (&["they"], Recipient::TriggeringThey),
    (
        &["that", "defending", "player"],
        Recipient::DefendingPlayerWord,
    ),
    (&["defending", "player"], Recipient::DefendingPlayer),
    (&["that", "token"], Recipient::CreatedObject),
    (&["that", "created", "creature"], Recipient::CreatedThat),
    (&["that", "created", "token"], Recipient::CreatedThatToken),
    (&["that", "created", "land"], Recipient::CreatedThatLand),
    (&["that", "created", "card"], Recipient::CreatedThatCard),
    (&["that", "exiled", "card"], Recipient::CreatedExiledCard),
    (
        &["a", "card", "exiled", "with", "this"],
        Recipient::ACardExiledWith,
    ),
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
    /// Inside a delayed trigger about a target player (`delayed target
    /// player: that player draws 1 card ...`): that target's index, "that
    /// player" being the player it is about.
    delayed_player: Option<usize>,
    /// Inside "When that creature dies (leaves the battlefield) this
    /// turn": "its controller" is the controller that object had.
    delayed_dies: bool,
    /// Inside a triggered ability: "that creature" is the triggering
    /// object there, the last target elsewhere.
    in_trigger: bool,
    /// Inside a `for each opponent,` sentence: `that opponent controls`.
    per_opponent: bool,
    /// Inside a trigger on casting "a spell that targets this creature"
    /// (heroic): `it` is the source, as Oracle has it ("put a +1/+1
    /// counter on it").
    it_source: bool,
    /// Inside a triggered ability whose event has no object ("at the
    /// beginning of your upkeep", "whenever you gain life"): there is no
    /// triggering object for `it` to be.
    no_trigger_obj: bool,
    /// Inside "When this enters", "Whenever this attacks", "When this is
    /// turned face up" (the triggering object is the source):
    /// "that creature" after an object target is that target ("attach it
    /// to target creature you control. That creature gains hexproof").
    self_trigger: bool,
    /// Inside "When this enters", "When this dies", "When this leaves the
    /// battlefield", "When you cast this spell": the event names no player,
    /// so `that player controls` would match nothing.
    no_trigger_player: bool,
    /// An effect of this ability created one token, with this many
    /// targets declared then: `it` after it is that token ("Create a 1/1
    /// ... token. Put X +1/+1 counters on it.") until a target is named.
    created_at: Option<usize>,
    /// Inside a triggered ability whose event has no amount (a cast, a
    /// step, life gain): `that much` would be 0.
    no_trigger_amount: bool,
    /// Inside an instant or sorcery's own effects (`spell:`, or the modes
    /// of a modal spell): `it` after a target is that target, since the
    /// spell itself can only be the "it" that deals damage.
    in_spell: bool,
    /// A group (`each <filter>`, `creatures you control`) has been named
    /// in this ability, for `them` and `those creatures` after it.
    group_named: bool,
    /// The subject of the previous clause of the sentence being parsed.
    subject: Option<Recipient>,
    /// The sentence's subject was followed by `each` ("one or two target
    /// creatures each gain haste").
    subject_each: bool,
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

    /// The last target declared in this ability if it can be an object
    /// (not "target player" or "target opponent"): what `it` after it is.
    fn last_object_target(&self) -> Option<usize> {
        use mtg_core::card::TargetSpec as S;
        let n = self.targets.len().checked_sub(1)?;
        let player = matches!(
            self.targets[n].base(),
            Target::Spec(
                S::Player
                    | S::Opponent
                    | S::Any
                    | S::PlayerOrPlaneswalker
                    | S::OpponentOrPlaneswalker
            )
        );
        (!player).then_some(n)
    }

    fn another_target(&self, spec: Target) -> R<Target> {
        match other_target(spec) {
            Some(t) => Ok(t),
            None => self.fail("`another` can't qualify this target"),
        }
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

    /// Whether the next token is the word `w` in any case (`w` itself is
    /// lowercase; one with a capital never matches, as when this compared
    /// a lowercased copy of the token).
    fn at_word(&self, w: &str) -> bool {
        match self.peek_at(0) {
            Some(Tok::Word(x)) => {
                x.eq_ignore_ascii_case(w) && !w.bytes().any(|b| b.is_ascii_uppercase())
            }
            _ => false,
        }
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
    #[allow(clippy::too_many_lines)] // one choice kind after another
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
        if self.eat_words(&["creature", "type", "among"]) {
            let mut types = vec![self.word()?];
            while self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
                types.push(self.word()?);
            }
            if types.len() < 2 || types.len() > 8 {
                return self.fail("`creature type among`: two to eight types");
            }
            return Ok(ChoiceKind::CreatureTypeIn(types));
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
        // "choose odd or even".
        if self.eat_words(&["odd", "or", "even"]) {
            return Ok(ChoiceKind::Parity);
        }
        // "choose a number between 0 and 10"; "choose a number." (any).
        if self.at_word("number") && !self.at_words(&["number", "between"]) {
            self.pos += 1;
            return Ok(ChoiceKind::Number {
                min: 0,
                max: u8::MAX,
            });
        }
        if self.eat_words(&["number", "between"]) {
            let min = u8::try_from(self.number()?).unwrap_or(u8::MAX);
            self.expect_word("and")?;
            let max = u8::try_from(self.number()?).unwrap_or(u8::MAX);
            if min > max {
                return self.fail("`choose number between`: the least first");
            }
            return Ok(ChoiceKind::Number { min, max });
        }
        // "choose a card name", "choose a nonland card name".
        if self.at_words(&["card", "name", "except", "basic", "land"]) {
            self.pos += 5;
            return Ok(ChoiceKind::CardNameOf(
                mtg_core::card::NameKind::NotBasicLand,
            ));
        }
        if self.eat_words(&["card", "name"]) {
            return Ok(ChoiceKind::CardName {
                nonland: false,
                look: false,
            });
        }
        if self.eat_words(&["creature", "card", "name"]) {
            return Ok(ChoiceKind::CardNameOf(mtg_core::card::NameKind::Creature));
        }
        if self.eat_words(&["nonland", "card", "name"]) {
            return Ok(ChoiceKind::CardName {
                nonland: true,
                look: false,
            });
        }
        // `"Khans" or "Dragons"`.
        if let Some(Tok::Str(_)) = self.peek() {
            let mut names = Vec::new();
            loop {
                match self.peek() {
                    Some(Tok::Str(s)) => {
                        names.push(s.clone());
                        self.pos += 1;
                    }
                    _ => return self.fail("expected a quoted option name"),
                }
                if !self.eat_word("or") {
                    break;
                }
            }
            if names.len() < 2 {
                return self.fail("a choice of named options takes two or more");
            }
            return Ok(ChoiceKind::Named(names));
        }
        self.unexpected("`color`, `creature type`, `land type` or quoted option names")
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
            flash_if_named: false,
            flash_cleanup: false,
            pronoun: None,
            short_name: None,
            unkicked_targets: None,
            kicked_drops: None,
            cast_only: None,
            cast_only_cant: false,
            kicker: None,
            kicker_cost: None,
            kicker_targets: None,
            flashback: None,
            graveyard_cast: None,
            graveyard_alias: None,
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
                    "flip" => mtg_core::card::Layout::Flip,
                    w => {
                        return self.fail(format!(
                            "unknown layout `{w}` (adventure, omen, transform, split, prepare, modal_dfc, flip)"
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
                } else if !self.at_words(&["may", "behold"])
                    && !self.at_words(&["may", "reveal"])
                    && !self.at_words(&["may", "sacrifice", "any"])
                    && self.eat_word("may")
                {
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
                            cond_late: false,
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
                let cond_late = !cost.conditions.is_empty() && self.eat_word("late");
                card.alt_cost = Some(crate::ast::AltCost {
                    cost,
                    count,
                    others_gain,
                    cond_late,
                });
                self.end_stmt()
            }
            "kicker" if matches!(self.peek_at(1), Some(Tok::Word(_))) => {
                self.pos += 1;
                card.kicker_cost = Some(self.cost()?);
                self.end_stmt()
            }
            "kicker" => {
                self.pos += 1;
                let at = self.pos;
                let k = self.mana_cost()?;
                // "Kicker—{2}{R}, Sacrifice a land."
                if self.peek() == Some(&Tok::Comma) {
                    self.pos = at;
                    card.kicker_cost = Some(self.cost()?);
                } else {
                    card.kicker = Some(k);
                }
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
                    "they" => crate::ast::Pronoun::They,
                    w => return self.fail(format!("unknown pronoun `{w}` (he, she, they)")),
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
            "flash"
                if self.at_words(&["flash", "as", "long", "as"])
                    || self.at_words(&["flash", "named", "as", "long", "as"]) =>
            {
                self.pos += 1;
                card.flash_if_named = self.eat_word("named");
                self.pos += 3;
                card.flash_if = Some(self.static_cond()?);
                self.end_stmt()
            }
            // `cast from graveyard while you control Zombie`, `cast from
            // graveyard for {2}{W}`, `cast from graveyard with discard 1
            // card` (CR 601.3).
            // `counts as named "Flame Burst" in graveyard`.
            "counts" if self.at_words(&["counts", "as", "named"]) => {
                self.pos += 3;
                card.graveyard_alias = Some(self.string()?);
                self.expect_words(&["in", "graveyard"])?;
                self.end_stmt()
            }
            "cast" if self.at_words(&["cast", "from", "graveyard"]) => {
                self.pos += 3;
                let mut g = crate::ast::GraveyardCast::default();
                if self.eat_words(&["at", "max", "speed"]) {
                    g.max_speed = true;
                } else if self.eat_word("while") {
                    g.as_long_as = true;
                    g.cond = Some(self.static_cond()?);
                } else if self.eat_word("if") {
                    g.cond = Some(self.static_cond()?);
                }
                if self.eat_word("for") {
                    g.mana = Some(self.mana_cost()?);
                } else if self.eat_word("with") {
                    g.extra = Some(self.cost()?);
                }
                card.graveyard_cast = Some(g);
                self.end_stmt()
            }
            "cast" if self.at_words(&["cast", "only"]) => {
                self.pos += 2;
                card.cast_only_cant = self.eat_word("unless");
                if !card.cast_only_cant {
                    self.eat_word("if");
                }
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
                        // "it's a 1/1 ..." (CR 707.9b).
                        Some("pt") => {
                            self.pos += 1;
                            match self.next() {
                                Some(Tok::Pt {
                                    power,
                                    toughness,
                                    signed: false,
                                }) => except.pt = Some((power, toughness)),
                                _ => return self.fail("expected power/toughness such as 1/1"),
                            }
                            self.end_stmt()?;
                        }
                        Some("color" | "colors") => {
                            self.pos += 1;
                            except.colors = self.color_list()?;
                            self.end_stmt()?;
                        }
                        // "it isn't legendary" (CR 707.9b).
                        Some("not") if self.word_at(1).as_deref() == Some("legendary") => {
                            self.pos += 2;
                            except.not_legendary = true;
                            self.end_stmt()?;
                        }
                        // "it's legendary".
                        Some("legendary") => {
                            self.pos += 1;
                            except.legendary = true;
                            self.end_stmt()?;
                        }
                        // "it's a 1/1 green Frog": not in addition.
                        Some("exactly") => {
                            self.pos += 1;
                            except.exactly = true;
                            self.end_stmt()?;
                        }
                        // "it isn't legendary and is a Mutant ...".
                        Some("and") if self.word_at(1).as_deref() == Some("is") => {
                            self.pos += 2;
                            except.and_is = true;
                            self.end_stmt()?;
                        }
                        // "except the token isn't legendary".
                        Some("the") if self.word_at(1).as_deref() == Some("token") => {
                            self.pos += 2;
                            except.the_token = true;
                            self.end_stmt()?;
                        }
                        // "except it's not legendary" (Ran and Shaw).
                        Some("its")
                            if self.word_at(1).as_deref() == Some("not")
                                && self.word_at(2).as_deref() == Some("legendary") =>
                        {
                            self.pos += 3;
                            except.not_legendary = true;
                            except.its_not = true;
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

    /// The rest of `static [each] <filter> card [or <filter> card] in your
    /// graveyard: has <grant>` after the first filter (`Ability::CardGrant`).
    fn card_grant(
        &mut self,
        mut filter: Filter,
        each: bool,
        cond: Option<StaticCond>,
    ) -> R<Ability> {
        filter.each = false;
        self.expect_word("card")?;
        if self.eat_word("or") {
            let alt = self.filter()?;
            self.expect_word("card")?;
            filter.or = Some(Box::new(alt));
            filter.alt_card = true;
        }
        self.expect_word("in")?;
        let players = self.eat_words(&["each", "player's"]);
        if !players {
            self.expect_word("your")?;
        }
        let zone = if self.eat_word("graveyard") {
            mtg_core::state::Zone::Graveyard
        } else {
            self.expect_word("hand")?;
            mtg_core::state::Zone::Hand
        };
        self.expect(&Tok::Colon)?;
        self.expect_word("has")?;
        let cost = |p: &mut Self| -> R<Option<ManaCost>> {
            if matches!(p.peek(), Some(Tok::Sym(_))) {
                Ok(Some(p.mana_cost()?))
            } else {
                Ok(None)
            }
        };
        let grant = match self.word_at(0).as_deref() {
            Some("flashback") => {
                self.pos += 1;
                crate::ast::Granted::Flashback(cost(self)?)
            }
            Some("mayhem") => {
                self.pos += 1;
                crate::ast::Granted::Mayhem(cost(self)?)
            }
            Some("warp") => {
                self.pos += 1;
                crate::ast::Granted::Warp(self.mana_cost()?)
            }
            Some("miracle") => {
                self.pos += 1;
                crate::ast::Granted::Miracle(self.mana_cost()?)
            }
            Some("retrace") => {
                self.pos += 1;
                crate::ast::Granted::Retrace
            }
            Some("foretell") => {
                self.pos += 1;
                crate::ast::Granted::Foretell
            }
            Some("sneak") => {
                self.pos += 1;
                crate::ast::Granted::Sneak(self.mana_cost()?)
            }
            Some("jump") if self.word_at(1).as_deref() == Some("start") => {
                self.pos += 2;
                crate::ast::Granted::JumpStart
            }
            _ => {
                let a = self.ability()?;
                if !matches!(a, Ability::Activated { .. } | Ability::Mechanic(_)) {
                    return self.fail("a card grant gives a keyword ability");
                }
                return Ok(Ability::CardGrant {
                    filter,
                    zone,
                    players,
                    each,
                    grant: crate::ast::Granted::Ability(Box::new(a)),
                    cond,
                });
            }
        };
        self.end_stmt()?;
        Ok(Ability::CardGrant {
            filter,
            zone,
            players,
            each,
            grant,
            cond,
        })
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

    /// After `add`: the mana to add (CR 106.4).
    #[allow(clippy::too_many_lines)] // one arm per form of `add`
    fn add_effect(&mut self) -> R<Effect> {
        if self.eat_word("chosen") {
            // `add chosen 2`: "Add two mana of the chosen color."
            if let Some(Tok::Num(n)) = self.peek() {
                let n = *n;
                self.pos += 1;
                return Ok(Effect::AddManaTimes {
                    mana: Mana::Chosen,
                    count: Num::N(n),
                });
            }
            return Ok(Effect::AddMana(vec![Mana::Chosen]));
        }
        // CR 903.4f: "one mana of any color in your commander's color
        // identity".
        if self.eat_word("commander") {
            let rider = if self.eat_word("rider") {
                if self.eat_word("counters") {
                    1
                } else {
                    self.expect_word("scry")?;
                    2
                }
            } else {
                0
            };
            return Ok(Effect::AddManaCommander { rider });
        }
        // "any color that a land an opponent controls could
        // produce" (CR 106.7).
        if self.at_words(&["any", "that"]) || self.at_words(&["any", "type", "that"]) {
            self.pos += 1;
            let any_type = self.eat_word("type");
            self.expect_word("that")?;
            let basic = self.eat_word("basic");
            self.expect_word("land")?;
            let whose = if self.eat_words(&["you", "control"]) {
                Who::You
            } else {
                self.expect_words(&["opponent", "controls"])?;
                Who::Opponents
            };
            self.expect_words(&["could", "produce"])?;
            return Ok(Effect::AddManaLandCould {
                whose,
                basic,
                any_type,
            });
        }
        if self.eat_word("any") {
            // `add any count where power`: "Add X mana of any one color,
            // where X is this creature's power."
            if let (Some(Tok::Num(n)), Some("and"), Some("any")) = (
                self.peek().cloned(),
                self.word_at(1).as_deref(),
                self.word_at(2).as_deref(),
            ) {
                // `add any 2 and any 2 other`.
                self.pos += 3;
                let m = self.number()?;
                self.expect_word("other")?;
                if m != n {
                    return self.fail("`add any N and any N other` takes the same N twice");
                }
                return Ok(Effect::AddManaTwoColors(u8::try_from(n).unwrap_or(u8::MAX)));
            }
            if matches!(self.peek(), Some(Tok::Num(_))) || self.at_word("count") {
                return Ok(Effect::AddManaTimes {
                    mana: Mana::Any,
                    count: self.num()?,
                });
            }
            // `add any X`: "Add X mana of any one color." (the ability's
            // X: Petalmane Baku's counters removed).
            if self.eat_word("x") {
                return Ok(Effect::AddManaTimes {
                    mana: Mana::Any,
                    count: Num::X,
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
                    listed: false,
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
                listed: false,
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
        // `add {W}{W} or {W}{U} or {U}{U}` (the filter lands): each
        // combination of two mana of two colors, listed.
        if out.len() == 2 && self.at_word("or") {
            let mut options = vec![out.clone()];
            while self.eat_word("or") {
                let c = self.mana_cost()?;
                let mut o = Vec::new();
                for s in c.symbols() {
                    match s {
                        mtg_core::mana::ManaSymbol::Colored(c) => o.push(Mana::Colored(*c)),
                        _ => return self.unexpected("two colored mana symbols after `or`"),
                    }
                }
                options.push(o);
            }
            let mut set = mtg_core::mana::ColorSet::COLORLESS;
            for m in options.iter().flatten() {
                if let Mana::Colored(c) = m {
                    set = set.with(*c);
                }
            }
            let pair: Vec<mtg_core::mana::Color> = set.iter().collect();
            let key = |o: &Vec<Mana>| {
                let mut k: Vec<String> = o.iter().map(|m| format!("{m:?}")).collect();
                k.sort();
                k
            };
            let mut got: Vec<Vec<String>> = options.iter().map(key).collect();
            got.sort();
            let fits = pair.len() == 2 && {
                let (a, b) = (Mana::Colored(pair[0]), Mana::Colored(pair[1]));
                let mut want: Vec<Vec<String>> = [vec![a, a], vec![a, b], vec![b, b]]
                    .iter()
                    .map(key)
                    .collect();
                want.sort();
                got == want
            };
            if !fits {
                return self.fail(
                    "`add {W}{W} or {W}{U} or {U}{U}` lists the three combinations of two mana of two colors",
                );
            }
            return Ok(Effect::AddManaCombination {
                count: Num::N(2),
                colors: set,
                different: false,
                listed: true,
            });
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
        // `add {R} that much` (Raphael's "add that much {R}").
        if out.len() == 1
            && (self.at_word("count")
                || self.at_words(&["that", "much"])
                || (matches!(self.peek(), Some(Tok::Num(_)))
                    && matches!(out[0], Mana::Colored(_) | Mana::Colorless)))
        {
            let count = self.num()?;
            return Ok(Effect::AddManaTimes {
                mana: out[0],
                count,
            });
        }
        Ok(Effect::AddMana(out))
    }

    /// `... only to cast creature spells` after an `add` effect: "Spend
    /// this mana only to ..." (CR 106.6).
    fn spend_only_suffix(&mut self, e: Effect) -> R<Effect> {
        // `spent on Dragon creature spell: you gain 2 life`: "When you
        // spend this mana to cast a Dragon creature spell, ...".
        if self.eat_words(&["spent", "on"]) {
            let spells = self.filter()?;
            self.expect_word("spell")?;
            self.expect(&Tok::Colon)?;
            let saved = self.subject.take();
            let then = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::SpendRider {
                inner: vec![Sentence::new(vec![e])],
                spells,
                then: vec![then],
            });
        }
        if self.at_words(&["only", "to"])
            || self.at_words(&["not", "to"])
            || self.at_words(&["on", "costs"])
        {
            let restriction = self.mana_restriction()?;
            return Ok(Effect::SpendOnly {
                inner: vec![Sentence::new(vec![e])],
                restriction,
            });
        }
        Ok(e)
    }

    /// `only to cast instant or sorcery spell`, `only to cast artifact
    /// spell or [to] activate`, `only to activate [or cast artifact
    /// spell]`, `not to cast nonartifact spell` (CR 106.6).
    fn mana_restriction(&mut self) -> R<crate::ast::ManaRestriction> {
        use mtg_core::card::{SpendPlural, SpendWays, SpendWording};
        let none = || crate::ast::ManaRestriction {
            spells: None,
            abilities: false,
            ability_source: None,
            foretell: false,
            disturb: false,
            wording: SpendWording::CastFirst,
            plural: SpendPlural::One,
            bare: false,
            ways: SpendWays::default(),
            x_spells: None,
        };
        // `on costs with x`: "Spend this mana only on costs that contain
        // {X}."
        if self.eat_words(&["on", "costs", "with", "x"]) {
            let mut r = none();
            r.ways = SpendWays(SpendWays::ON_COSTS | SpendWays::COST_X);
            return Ok(r);
        }
        // `only to power-up abilities`, `only to face up permanents`: only
        // the other ways, no spells or abilities.
        if self.at_words(&["only", "to"]) && self.at_spend_way(2) {
            self.pos += 2;
            let mut r = none();
            self.spend_ways(&mut r, true)?;
            while self.eat_word("plus") {
                if self.eat_word("to") {
                    r.ways.0 |= SpendWays::OR_TO;
                }
                self.spend_ways(&mut r, false)?;
            }
            return Ok(r);
        }
        let mut r = self.mana_restriction_base()?;
        // `plus unlock`, `plus face up`, `plus to class level`: further
        // alternatives; `uncounterable`.
        while self.eat_word("plus") {
            if self.eat_word("to") {
                r.ways.0 |= SpendWays::OR_TO;
            } else if self.eat_word("and") {
                r.wording = SpendWording::CastAnd;
            }
            self.spend_ways(&mut r, false)?;
        }
        if self.eat_word("uncounterable") {
            r.ways.0 |= SpendWays::UNCOUNTERABLE;
        }
        Ok(r)
    }

    /// Whether the word `at` words on starts one of a restriction's other
    /// ways (`spend_ways`).
    fn at_spend_way(&self, at: usize) -> bool {
        matches!(
            self.word_at(at).as_deref(),
            Some("power-up" | "equip" | "class" | "unlock" | "face" | "cost")
        )
    }

    /// One of a restriction's other ways: `power-up [abilities]`, `equip
    /// [abilities]`, `class level`, `unlock [doors]`, `face up
    /// [permanents | creatures | morph]`, `cost with c`. The plural words
    /// set the plural wording when `alone` (no spells before them).
    fn spend_ways(&mut self, r: &mut crate::ast::ManaRestriction, alone: bool) -> R<()> {
        use mtg_core::card::{SpendPlural, SpendWays};
        let mut plural = false;
        let w = self.word_at(0).unwrap_or_default();
        match w.as_str() {
            "power-up" | "equip" => {
                self.pos += 1;
                r.ways.0 |= if w == "equip" {
                    SpendWays::EQUIP
                } else {
                    SpendWays::POWER_UP
                };
                plural = self.eat_word("abilities");
            }
            "class" => {
                self.pos += 1;
                self.expect_word("level")?;
                r.ways.0 |= SpendWays::CLASS_LEVEL;
            }
            "unlock" => {
                self.pos += 1;
                r.ways.0 |= SpendWays::UNLOCK;
                plural = self.eat_word("doors");
            }
            "face" => {
                self.pos += 1;
                self.expect_word("up")?;
                r.ways.0 |= SpendWays::FACE_UP;
                if self.eat_word("permanents") {
                    plural = true;
                } else if self.eat_word("creatures") {
                    plural = true;
                    r.ways.0 |= SpendWays::FACE_UP_CREATURES;
                } else if self.eat_word("morph") {
                    r.ways.0 |= SpendWays::FACE_UP_MORPH;
                }
            }
            "cost" => {
                self.pos += 1;
                self.expect_words(&["with", "c"])?;
                r.ways.0 |= SpendWays::COST_C;
            }
            _ => return self.fail("expected a way to spend mana"),
        }
        if alone && plural {
            r.plural = SpendPlural::Or;
        }
        Ok(())
    }

    /// After a restriction's spell noun: `from exile`, `from graveyard`,
    /// `not from hand`, `with flashback from graveyard`, `face down` (what
    /// the spell must be cast as).
    fn spend_qualifiers(&mut self) -> mtg_core::card::SpendWays {
        use mtg_core::card::SpendWays;
        let mut w = 0;
        loop {
            if self.eat_words(&["from", "exile"]) {
                w |= SpendWays::FROM_EXILE;
            } else if self.eat_words(&["from", "graveyard"]) {
                w |= SpendWays::FROM_GRAVEYARD;
            } else if self.eat_words(&["not", "from", "hand"]) {
                w |= SpendWays::NOT_FROM_HAND;
            } else if self.eat_words(&["with", "flashback", "from", "graveyard"]) {
                w |= SpendWays::FLASHBACK;
            } else if self.eat_words(&["face", "down"]) {
                w |= SpendWays::FACE_DOWN;
            } else {
                return SpendWays(w);
            }
        }
    }

    #[allow(clippy::too_many_lines)] // one branch per wording
    fn mana_restriction_base(&mut self) -> R<crate::ast::ManaRestriction> {
        use mtg_core::card::{SpendPlural, SpendWays, SpendWording};
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
                plural: SpendPlural::One,
                bare: false,
                ways: SpendWays::default(),
                x_spells: None,
            });
        }
        self.expect_words(&["only", "to"])?;
        if self.eat_word("activate") {
            let plural = if self.eat_word("abilities") {
                SpendPlural::Or
            } else {
                SpendPlural::One
            };
            let (ability_source, bare) = self.activate_of()?;
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
                plural,
                bare,
                ways: SpendWays::default(),
                x_spells: None,
            });
        }
        // "foretell a card from your hand or cast an instant or sorcery
        // spell" (CR 702.143a).
        let foretell = self.eat_words(&["foretell", "or"]);
        let disturb = self.eat_words(&["pay", "disturb", "or"]);
        self.expect_word("cast")?;
        let mut f = self.spend_filter()?;
        let mut plural = self.spell_noun()?;
        let ways = self.spend_qualifiers();
        // `or x spells`: "or spells with {X} in their mana costs".
        let x_spells = if self.eat_words(&["or", "x"]) {
            let g = self.spend_filter()?;
            self.spell_noun()?;
            Some(Box::new(g))
        } else {
            None
        };
        // "an Elemental spell or a Chandra planeswalker spell": each side
        // whole.
        if self.at_word("or") && !matches!(self.word_at(1).as_deref(), Some("to" | "activate")) {
            self.pos += 1;
            let alt = self.filter()?;
            plural = self.spell_noun()?;
            f.or = Some(Box::new(alt));
            // Each side with its own article ("a Dragon spell or an Omen
            // spell"), against `Lesson or Shrine spell` ("a Lesson or
            // Shrine spell").
            f.alt_article = true;
        }
        // `spells joined and`: "instant and sorcery spells".
        if plural != SpendPlural::One && self.eat_word("joined") {
            plural = if self.eat_word("and") {
                SpendPlural::And
            } else {
                self.expect_word("andor")?;
                SpendPlural::AndOr
            };
        }
        let (abilities, wording) = if self.eat_words(&["or", "to", "activate"]) {
            (true, SpendWording::CastFirstTo)
        } else if self.eat_words(&["or", "activate"]) {
            (true, SpendWording::CastFirst)
        } else if self.eat_words(&["and", "activate"]) {
            (true, SpendWording::CastAnd)
        } else {
            (false, SpendWording::CastFirst)
        };
        let (ability_source, bare) = if abilities {
            self.activate_of()?
        } else {
            (None, false)
        };
        Ok(crate::ast::ManaRestriction {
            spells: Some(f),
            abilities,
            ability_source,
            foretell,
            disturb,
            wording,
            plural,
            bare,
            ways,
            x_spells,
        })
    }

    /// A restriction's spell filter, empty before `spells` ("cast spells
    /// from exile").
    fn spend_filter(&mut self) -> R<Filter> {
        if self.at_word("spells") || self.at_word("spell") {
            return Ok(Filter::default());
        }
        self.filter()
    }

    /// `spell` or `spells` (the plural wording) after a restriction's
    /// filter.
    fn spell_noun(&mut self) -> R<mtg_core::card::SpendPlural> {
        if self.eat_word("spells") {
            return Ok(mtg_core::card::SpendPlural::Or);
        }
        self.expect_word("spell")?;
        Ok(mtg_core::card::SpendPlural::One)
    }

    /// After `activate`: `of artifact` ("an ability of an artifact
    /// source"), or nothing for any source.
    fn activate_of(&mut self) -> R<(Option<Filter>, bool)> {
        if self.eat_word("of") {
            let f = self.filter()?;
            Ok((Some(f), self.eat_word("bare")))
        } else {
            Ok((None, false))
        }
    }

    /// `each player can't`, `you can't`, `opponents can't`, `enchanted
    /// player can't` before `cast more than one spell each turn`, `draw
    /// more than one card each turn`, or `gain life`: whom the
    /// player-level "can't" binds, and the words that say so.
    fn rule_players_ahead(&self) -> Option<(mtg_core::card::RulePlayers, usize)> {
        use mtg_core::card::RulePlayers as P;
        let (players, n) = if self.at_words(&["each", "player", "can't"]) {
            (P::Each, 3)
        } else if self.at_words(&["enchanted", "player", "can't"]) {
            (P::Enchanted, 3)
        } else if self.at_words(&["you", "can't"]) {
            (P::You, 2)
        } else if self.at_words(&["opponents", "can't"]) {
            (P::Opponents, 2)
        } else {
            return None;
        };
        let next = self.word_at(n)?;
        let ok = match next.as_str() {
            "cast" | "draw" => self.word_at(n + 1).as_deref() == Some("more"),
            "gain" => {
                players != P::Each
                    && self.word_at(n + 1).as_deref() == Some("life")
                    && self.word_at(n + 2).as_deref() != Some("this")
            }
            _ => false,
        };
        ok.then_some((players, n))
    }

    /// After `play from top [once]` (or the effect's `play from top this
    /// turn`): `[look] (cards | card | lands [and spells [<filter>]] |
    /// spells [<filter>])`, then any of `as though flash`, `pay life`, `as
    /// long as <condition>`.
    fn top_play_body(&mut self) -> R<(bool, Option<Filter>, crate::ast::TopMore)> {
        use crate::ast::{TopMore, TopWording};
        let mut more = TopMore {
            look: self.eat_word("look"),
            ..TopMore::default()
        };
        let at_end = |p: &Self| {
            p.at_stmt_end()
                || matches!(p.peek(), Some(Tok::Semi))
                || p.at_words(&["as", "though", "flash"])
                || p.at_words(&["pay", "life"])
                || p.at_words(&["as", "long", "as"])
        };
        let (lands, spells) = if self.eat_word("cards") {
            more.wording = TopWording::Cards;
            (true, Some(Filter::default()))
        } else if self.eat_word("card") {
            more.wording = TopWording::TopCard;
            (true, Some(Filter::default()))
        } else {
            let lands = self.eat_word("lands");
            let spells = if (!lands || self.eat_word("and")) && self.eat_word("spells") {
                if at_end(self) {
                    Some(Filter::default())
                } else {
                    Some(self.filter()?)
                }
            } else {
                None
            };
            (lands, spells)
        };
        if !lands && spells.is_none() {
            return self.fail("expected `lands`, `spells`, `cards`, or `card`".to_string());
        }
        loop {
            if self.eat_words(&["as", "though", "flash"]) {
                more.flash = true;
            } else if self.eat_words(&["pay", "life"]) {
                more.life = true;
            } else if self.eat_words(&["as", "long", "as"]) {
                more.cond = Some(self.static_cond()?);
            } else {
                break;
            }
        }
        Ok((lands, spells, more))
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

    /// The rest of `<class> abilities [you activate | your opponents
    /// activate] [of <filter> | of sources with chosen name] cost {N}
    /// less|more [unless mana] [min one]` (`Ability::ActivationCost`),
    /// after `abilities`.
    fn activation_cost(&mut self, class: mtg_core::card::AbilityClass) -> R<Ability> {
        use mtg_core::card::RulePlayers;
        let activator = if self.eat_words(&["you", "activate"]) {
            Some(RulePlayers::You)
        } else if self.eat_words(&["your", "opponents", "activate"]) {
            Some(RulePlayers::Opponents)
        } else {
            None
        };
        let sources = if self.eat_words(&["of", "sources", "with", "chosen", "name"]) {
            ActivationSources::ChosenName
        } else if self.eat_word("of") {
            ActivationSources::Filter(Box::new(self.filter()?))
        } else {
            ActivationSources::Any
        };
        self.expect_word("cost")?;
        let by = u8::try_from(self.mana_cost()?.mana_value()).unwrap_or(u8::MAX);
        let more = if self.eat_word("more") {
            true
        } else {
            self.expect_word("less")?;
            false
        };
        let class = if self.eat_words(&["unless", "mana"]) {
            if class != mtg_core::card::AbilityClass::All {
                return self.fail("`unless mana` needs `activated abilities`");
            }
            mtg_core::card::AbilityClass::NonMana
        } else {
            class
        };
        let min_one = self.eat_words(&["min", "one"]);
        self.end_stmt()?;
        Ok(Ability::ActivationCost {
            sources,
            by,
            more,
            min_one,
            class,
            activator,
        })
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
        self.no_trigger_obj = false;
        self.self_trigger = false;
        self.no_trigger_player = false;
        self.created_at = None;
        self.no_trigger_amount = false;
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
            "spells"
                if self.at_words(&["spells", "with", "chosen", "name", "can't", "be", "cast"]) =>
            {
                self.pos += 7;
                self.player_static(mtg_core::card::PlayerStaticKind::ChosenName(
                    mtg_core::card::NameBan::Cast,
                ))
            }
            "spells" => self.cost_change(None),
            "hexproof" if self.word_at(1).as_deref() == Some("from") => {
                self.pos += 2;
                let filter = self.filter()?;
                self.end_stmt()?;
                Ok(Ability::HexproofFrom(filter))
            }
            // `opening hand reveal first upkeep: ...`.
            "opening" if self.at_words(&["opening", "hand", "reveal"]) => {
                self.pos += 3;
                let whose = if self.eat_word("your") {
                    Who::You
                } else {
                    Who::Any
                };
                self.expect_word("first")?;
                let step = if self.eat_word("upkeep") {
                    Step::Upkeep
                } else if whose == Who::You && self.eat_word("main") {
                    Step::PrecombatMain
                } else {
                    return self.unexpected("`upkeep` or `main`");
                };
                let after = self.eat_word("after");
                let effects = self.effects_body()?;
                if !self.targets.is_empty() {
                    return self.fail("an opening hand reveal has no targets");
                }
                Ok(Ability::OpeningHandReveal {
                    step,
                    whose,
                    after,
                    effects,
                })
            }
            "opening" if self.at_words(&["opening", "hand"]) => {
                self.pos += 2;
                self.end_stmt()?;
                Ok(Ability::OpeningHand)
            }
            // CR 603.2d: `trigger again Ninja creature you control`,
            // `trigger again entering permanent you control`.
            "trigger"
                if self.at_words(&["trigger", "again"]) || self.at_words(&["trigger", "never"]) =>
            {
                let suppress = self.word_at(1).as_deref() == Some("never");
                self.pos += 2;
                let filter = self.filter()?;
                let (mut entering, mut battlefield) = (None, false);
                let mut cause = mtg_core::card::TriggerCause::Enters;
                if self.eat_word("when") {
                    entering = Some(self.filter()?);
                    if self.eat_word("dies") {
                        cause = mtg_core::card::TriggerCause::Dies;
                    } else if self.eat_word("attacks") {
                        cause = mtg_core::card::TriggerCause::Attacks;
                    } else {
                        self.expect_word("enters")?;
                        battlefield = self.eat_words(&["the", "battlefield"]);
                    }
                }
                self.end_stmt()?;
                if suppress && entering.is_none() {
                    return self.fail("`trigger never` needs `when <filter> enters`".to_string());
                }
                Ok(Ability::TriggerAgain {
                    filter,
                    entering,
                    battlefield,
                    cause,
                    suppress,
                })
            }
            // "Activated abilities of artifacts you control cost {1} less
            // to activate."
            "activated"
                if self.at_words(&[
                    "activated",
                    "abilities",
                    "of",
                    "sources",
                    "with",
                    "chosen",
                    "name",
                    "can't",
                    "be",
                    "activated",
                ]) =>
            {
                self.pos += 10;
                let ban = if self.eat_words(&["unless", "mana"]) {
                    mtg_core::card::NameBan::ActivateNonMana
                } else {
                    mtg_core::card::NameBan::Activate
                };
                self.player_static(mtg_core::card::PlayerStaticKind::ChosenName(ban))
            }
            "activated" | "power-up" | "exhaust" | "boast" | "ninjutsu" | "equip" | "loyalty"
                if self.word_at(1).as_deref() == Some("abilities")
                    && matches!(
                        self.word_at(2).as_deref(),
                        Some("of" | "you" | "your" | "cost")
                    ) =>
            {
                use mtg_core::card::AbilityClass as C;
                use mtg_core::card::ActivatedKeyword as K;
                let class = match w.as_str() {
                    "power-up" => C::Keyword(K::PowerUp),
                    "exhaust" => C::Keyword(K::Exhaust),
                    "boast" => C::Keyword(K::Boast),
                    "ninjutsu" => C::Keyword(K::Ninjutsu),
                    "equip" => C::Equip,
                    "loyalty" => C::Loyalty,
                    _ => C::All,
                };
                self.pos += 2;
                self.activation_cost(class)
            }
            // "Abilities you activate that aren't mana abilities cost {2}
            // less to activate." (Zirda)
            "abilities"
                if matches!(self.word_at(1).as_deref(), Some("you" | "your"))
                    && self.word_at(2).as_deref().is_some_and(|w| {
                        w == "activate"
                            || (w == "opponents" && self.word_at(3).as_deref() == Some("activate"))
                    }) =>
            {
                self.pos += 1;
                self.activation_cost(mtg_core::card::AbilityClass::All)
            }
            // "Enchanted creature can't be the target of spells or
            // abilities your opponents control.", "... of spells."
            "enchanted" if self.at_words(&["enchanted", "can't", "be", "targeted", "by"]) => {
                self.pos += 5;
                let ban = if self.eat_word("opponents") {
                    mtg_core::card::TargetBan::Opponents
                } else {
                    self.expect_word("spells")?;
                    mtg_core::card::TargetBan::Spells
                };
                self.player_static(mtg_core::card::PlayerStaticKind::EnchantedUntargetable(ban))
            }
            // The Flagbearer rule (Coalition Honor Guard).
            "flagbearer" if self.at_words(&["flagbearer", "rule"]) => {
                self.pos += 2;
                self.player_static(mtg_core::card::PlayerStaticKind::Flagbearer)
            }
            "you" if self.at_words(&["you", "have", "hexproof"]) => {
                self.pos += 3;
                self.player_static(mtg_core::card::PlayerStaticKind::Hexproof)
            }
            "you" if self.at_words(&["you", "can't", "lose", "the", "game"]) => {
                self.pos += 5;
                self.player_static(mtg_core::card::PlayerStaticKind::CantLose)
            }
            "you" if self.at_words(&["you", "have", "shroud"]) => {
                self.pos += 3;
                self.player_static(mtg_core::card::PlayerStaticKind::Shroud)
            }
            // Chosen-name bans (CR 201.3): Meddling Mage, Pithing Needle.
            "you"
                if self.at_words(&["you", "may", "play", "lands", "from", "your", "graveyard"]) =>
            {
                self.pos += 7;
                self.player_static(mtg_core::card::PlayerStaticKind::LandsFromGraveyard)
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
            // "You may pay {0} rather than pay the equip cost of the first
            // equip ability you activate each turn."
            "first" if self.at_words(&["first", "equip", "free"]) => {
                self.pos += 3;
                let your_turns = self.eat_words(&["during", "your", "turns"]);
                self.player_static(mtg_core::card::PlayerStaticKind::FirstEquipFree { your_turns })
            }
            // "Players can't draw cards [or gain life]." (CR 121.1).
            "players" if self.at_words(&["players", "can't", "draw", "cards"]) => {
                self.pos += 4;
                let kind = if self.eat_words(&["or", "gain", "life"]) {
                    mtg_core::card::RuleKind::NoDrawOrLifeGain
                } else {
                    mtg_core::card::RuleKind::NoDraw
                };
                self.player_static(mtg_core::card::PlayerStaticKind::Rule(kind))
            }
            // "Each player can't cast more than one spell each turn.", "Your
            // opponents can't gain life.", "Enchanted player can't gain
            // life.", "Each opponent can't draw more than one card each
            // turn." (CR 101.2).
            "each" | "you" | "opponents" | "enchanted" if self.rule_players_ahead().is_some() => {
                use mtg_core::card::PlayerStaticKind as K;
                let (players, n) = self.rule_players_ahead().expect("checked");
                self.pos += n;
                let kind = if self
                    .eat_words(&["cast", "more", "than", "one", "spell", "each", "turn"])
                {
                    K::CastLimit(players)
                } else if self.eat_words(&["draw", "more", "than", "one", "card", "each", "turn"]) {
                    K::DrawLimit(players)
                } else {
                    self.expect_words(&["gain", "life"])?;
                    K::CantGainLife(players)
                };
                self.player_static(kind)
            }
            // The "legend rule" (CR 704.5j).
            "legend" if self.at_words(&["legend", "rule", "doesn't", "apply"]) => {
                self.pos += 4;
                let yours = self.eat_words(&["to", "permanents", "you", "control"]);
                self.player_static(mtg_core::card::PlayerStaticKind::NoLegendRule { yours })
            }
            // "As long as enchanted creature is face down, it can't be
            // turned face up.", "Permanents your opponents control can't be
            // turned face up during your turn." (CR 702.37e).
            "enchanted" if self.at_words(&["enchanted", "can't", "be", "turned", "face", "up"]) => {
                self.pos += 6;
                self.player_static(mtg_core::card::PlayerStaticKind::FaceUpBan(
                    mtg_core::card::FaceUpBan::Enchanted,
                ))
            }
            "opponents"
                if self.at_words(&[
                    "opponents",
                    "permanents",
                    "can't",
                    "be",
                    "turned",
                    "face",
                    "up",
                    "during",
                    "your",
                    "turn",
                ]) =>
            {
                self.pos += 10;
                self.player_static(mtg_core::card::PlayerStaticKind::FaceUpBan(
                    mtg_core::card::FaceUpBan::OpponentsYourTurn,
                ))
            }
            // "Creatures with flying can't attack you [or planeswalkers you
            // control]." (CR 508.1c).
            // "Noncreature spells with mana value equal to the chosen
            // number can't be cast."
            "ban" if self.at_words(&["ban", "casting"]) => {
                self.pos += 2;
                let filter = self.filter()?;
                self.end_stmt()?;
                Ok(Ability::CantBeCast(filter))
            }
            "ban" if self.at_words(&["ban", "attacking", "you"]) => {
                self.pos += 3;
                let planeswalkers = self.eat_words(&["or", "planeswalkers"]);
                let filter = self.filter()?;
                self.end_stmt()?;
                Ok(Ability::CantAttackYou {
                    filter,
                    planeswalkers,
                })
            }
            "players" if self.at_words(&["players", "can't", "gain", "life"]) => {
                self.pos += 4;
                self.player_static(mtg_core::card::PlayerStaticKind::Rule(
                    mtg_core::card::RuleKind::NoLifeGain,
                ))
            }
            // "Cards in graveyards can't be the targets of spells or
            // abilities."
            "graveyard" if self.at_words(&["graveyard", "cards", "can't", "be", "targeted"]) => {
                self.pos += 5;
                self.player_static(mtg_core::card::PlayerStaticKind::Rule(
                    mtg_core::card::RuleKind::GraveyardUntargetable,
                ))
            }
            "damage" if self.at_words(&["damage", "can't", "be", "prevented"]) => {
                self.pos += 4;
                self.player_static(mtg_core::card::PlayerStaticKind::Rule(
                    mtg_core::card::RuleKind::DamageCantBePrevented,
                ))
            }
            "combat" if self.at_words(&["combat", "damage", "can't", "be", "prevented"]) => {
                self.pos += 5;
                self.player_static(mtg_core::card::PlayerStaticKind::Rule(
                    mtg_core::card::RuleKind::CombatDamageCantBePrevented,
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
            // CR 702.59a: recover; CR 702.120a: escalate.
            "recover" | "escalate" | "prowl" | "miracle" | "emerge"
                if matches!(self.peek_at(1), Some(Tok::Sym(_))) =>
            {
                self.pos += 1;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(match w.as_str() {
                    "recover" => Mechanic::Recover(c),
                    "miracle" => Mechanic::Miracle(c),
                    "emerge" => Mechanic::Emerge(c),
                    "prowl" => Mechanic::Prowl(c),
                    _ => Mechanic::Escalate(c),
                }))
            }
            // CR 702.152a: blitz.
            "blitz" if matches!(self.peek_at(1), Some(Tok::Sym(_))) => {
                self.pos += 1;
                let c = self.cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Blitz(c)))
            }
            // CR 702.160a: prototype.
            "prototype" if matches!(self.peek_at(1), Some(Tok::Sym(_))) => {
                self.pos += 1;
                let c = self.mana_cost()?;
                let Some(Tok::Pt {
                    power,
                    toughness,
                    signed: false,
                }) = self.next()
                else {
                    return self.fail("expected power/toughness such as 3/2");
                };
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Prototype(c, power, toughness)))
            }
            // CR 702.87a: level up.
            "level" if self.word_at(1).as_deref() == Some("up") => {
                self.pos += 2;
                let c = self.mana_cost()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::LevelUp(c)))
            }
            // CR 711.2: a leveler's level symbol and P/T box.
            "levels" if matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let min = self.number()?;
                let max = if self.eat_word("to") {
                    Some(self.number()?)
                } else {
                    self.expect_word("or")?;
                    self.expect_word("more")?;
                    None
                };
                if max.is_some_and(|m| m < min) {
                    return self.fail("a level range runs from the lower number");
                }
                self.expect_word("pt")?;
                let pt = match self.next() {
                    Some(Tok::Pt {
                        power,
                        toughness,
                        signed: false,
                    }) => (power, toughness),
                    _ => return self.fail("expected power/toughness such as 2/2"),
                };
                self.end_stmt()?;
                Ok(Ability::LevelBand { min, max, pt })
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
            // "Equip abilities you activate that target this creature cost
            // {2} less to activate."
            "equip" | "equipment"
                if self.at_words(&[w.as_str(), "abilities", "targeting", "this", "cost"]) =>
            {
                let equipment = w == "equipment";
                self.pos += 5;
                let cost = self.mana_cost()?;
                self.expect_word("less")?;
                self.end_stmt()?;
                let n = u8::try_from(cost.mana_value()).unwrap_or(u8::MAX);
                if n == 0 || cost.to_string() != format!("{{{n}}}") {
                    return self.fail("an equip discount is generic mana");
                }
                Ok(Ability::EquipDiscount { n, equipment })
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
                let as_long_as = self.at_words(&["less", "as", "long", "as"])
                    || self.at_words(&["more", "as", "long", "as"]);
                if self.at_words(&["less", "if"]) || self.at_words(&["more", "if"]) || as_long_as {
                    let less = self.at_word("less");
                    self.pos += if as_long_as { 4 } else { 2 };
                    let cond = self.static_cond()?;
                    self.end_stmt()?;
                    let n = i32::try_from(per).unwrap_or(0);
                    return Ok(Ability::CondCost {
                        delta: if less { -n } else { n },
                        cond,
                        as_long_as,
                    });
                }
                self.expect_word("less")?;
                let times = self.num()?;
                self.end_stmt()?;
                Ok(Ability::SelfCost { per, times })
            }
            // CR 702.14a: `landwalk snow Forest` ("Snow forestwalk").
            "landwalk" => {
                self.pos += 1;
                let filter = self.filter()?;
                self.end_stmt()?;
                Ok(Ability::Landwalk(filter))
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
                let (lands, spells, more) = self.top_play_body()?;
                self.end_stmt()?;
                Ok(Ability::PlayFromTop {
                    lands,
                    spells,
                    once,
                    more,
                })
            }
            // "You may cast spells from your hand without paying their mana
            // costs." (Omniscience).
            "cast" if self.at_words(&["cast", "free"]) => {
                use mtg_core::card::FreeCastLimit;
                self.pos += 2;
                let limit = if self.eat_words(&["once", "your", "turns"]) {
                    FreeCastLimit::OnceYourTurns
                } else if self.eat_words(&["first", "each", "player"]) {
                    FreeCastLimit::FirstEachPlayer
                } else {
                    FreeCastLimit::Always
                };
                let from_hand = self.eat_words(&["from", "hand"]);
                self.expect_word("spells")?;
                let spells = if self.at_stmt_end() || self.at_word("max") {
                    Filter::default()
                } else {
                    self.filter()?
                };
                let max = if self.eat_word("max") {
                    Some(if self.eat_word("count") {
                        crate::ast::CountExpr::Permanents(self.filter()?)
                    } else {
                        self.count_expr()?
                    })
                } else {
                    None
                };
                self.end_stmt()?;
                Ok(Ability::CastFree {
                    spells,
                    from_hand,
                    max,
                    limit,
                })
            }
            "may" if self.at_words(&["may", "look", "exiled"]) => {
                self.pos += 3;
                self.end_stmt()?;
                Ok(Ability::PlayExiled {
                    lands: false,
                    spells: None,
                    your_turn: false,
                    owned: false,
                    look: true,
                })
            }
            "play" if self.at_words(&["play", "exiled"]) => {
                self.pos += 2;
                let look = self.eat_word("look");
                let your_turn = self.eat_words(&["your", "turn"]);
                let owned = self.eat_word("owned");
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
                Ok(Ability::PlayExiled {
                    lands,
                    spells,
                    your_turn,
                    owned,
                    look,
                })
            }
            "top" if self.at_words(&["top", "revealed"]) => {
                self.pos += 2;
                let players = self.eat_word("players");
                self.end_stmt()?;
                Ok(Ability::TopRevealed {
                    players,
                    hands: false,
                })
            }
            "hands" if self.at_words(&["hands", "revealed", "players"]) => {
                self.pos += 3;
                self.end_stmt()?;
                Ok(Ability::TopRevealed {
                    players: true,
                    hands: true,
                })
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
            "damage" if self.at_words(&["damage", "isn't", "removed", "during", "cleanup"]) => {
                self.pos += 5;
                self.end_stmt()?;
                Ok(Ability::KeepsDamage)
            }
            "deck" if self.at_words(&["deck", "any", "number"]) => {
                self.pos += 3;
                self.end_stmt()?;
                Ok(Ability::DeckAnyNumber)
            }
            "companion" if matches!(self.peek_at(1), Some(Tok::Str(_))) => {
                self.pos += 1;
                let text = self.string()?;
                self.end_stmt()?;
                Ok(Ability::Companion(text))
            }
            // An attack or block tax (CR 508.1d, 509.1d).
            "tax" => {
                self.pos += 1;
                let scope = if self.eat_words(&["attacking", "you"]) {
                    let planeswalkers = self.eat_words(&["or", "planeswalkers"]);
                    mtg_core::card::TaxScope::AttackingYou {
                        planeswalkers,
                        those: planeswalkers,
                    }
                } else {
                    self.expect_word("enchanted")?;
                    if self.eat_word("blocked") {
                        mtg_core::card::TaxScope::EnchantedBlocked
                    } else if self.eat_words(&["attack", "or", "block"]) {
                        mtg_core::card::TaxScope::Enchanted {
                            attack: true,
                            block: true,
                        }
                    } else if self.eat_word("attack") {
                        mtg_core::card::TaxScope::Enchanted {
                            attack: true,
                            block: false,
                        }
                    } else {
                        self.expect_word("block")?;
                        mtg_core::card::TaxScope::Enchanted {
                            attack: false,
                            block: true,
                        }
                    }
                };
                let mana = self.mana_cost()?;
                let req = mana.requirements(0);
                let only_generic = req.colored == [0; 5]
                    && req.colorless == 0
                    && req.hybrid == [0; 10]
                    && req.two_hybrid == [0; 5];
                if !only_generic {
                    return self.fail("a combat tax is generic mana only");
                }
                let x = mana.has_x();
                let (generic, per) = if x {
                    if !self.at_word("count") {
                        return self.fail("`{X}` in a combat tax needs a count");
                    }
                    (1, Some(self.num()?))
                } else {
                    (req.generic, None)
                };
                // `tax attacking you {1} those`: "for each of those creatures".
                let scope = match scope {
                    mtg_core::card::TaxScope::AttackingYou {
                        planeswalkers: false,
                        ..
                    } if self.eat_word("those") => mtg_core::card::TaxScope::AttackingYou {
                        planeswalkers: false,
                        those: true,
                    },
                    s => s,
                };
                let cond = if self.eat_words(&["as", "long", "as"]) {
                    Some(self.static_cond()?)
                } else {
                    None
                };
                self.end_stmt()?;
                Ok(Ability::CombatTax {
                    scope,
                    generic,
                    per,
                    cond,
                })
            }
            "hand" if self.word_at(1).as_deref() == Some("size") => {
                self.pos += 2;
                let whose = if self.eat_word("opponents") {
                    Who::Opponents
                } else {
                    Who::You
                };
                let mut set = false;
                let delta = if self.eat_word("none") {
                    None
                } else {
                    match self.next() {
                        Some(Tok::Signed(n)) => Some(n),
                        // `hand size 8`: "Your maximum hand size is eight."
                        Some(Tok::Num(n)) => {
                            set = true;
                            Some(i32::try_from(n).unwrap_or(i32::MAX))
                        }
                        _ => {
                            return self
                                .fail("expected `none`, a size, or a signed number such as +1");
                        }
                    }
                };
                self.end_stmt()?;
                Ok(Ability::HandSize { whose, delta, set })
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
                // `and 1 trample counter`: a second kind.
                let more = if self.at_word("and") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
                    self.pos += 1;
                    let m = self.number()?;
                    Some((m, self.counter_kind()?))
                } else {
                    None
                };
                let keywords = if self.eat_word("and") {
                    self.keyword_list()?
                } else {
                    Vec::new()
                };
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::KickedWith(n, more, keywords)))
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
                if self.targets.is_empty() {
                    return Ok(Ability::Exert(effects));
                }
                // "When you do, target creature can't block this turn": a
                // reflexive trigger with the targets (CR 603.12).
                let targets = std::mem::take(&mut self.targets);
                Ok(Ability::Exert(vec![Sentence::new(vec![
                    Effect::WhenYouDo { targets, effects },
                ])]))
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
            "fading" | "annihilator" | "rampage"
                if matches!(self.peek_at(1), Some(Tok::Num(_))) =>
            {
                self.pos += 1;
                let n = self.number()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(match w.as_str() {
                    "fading" => Mechanic::Fading(n),
                    "annihilator" => Mechanic::Annihilator(n),
                    _ => Mechanic::Rampage(n),
                }))
            }
            "battle" if self.at_words(&["battle", "cry"]) => {
                self.pos += 2;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::BattleCry))
            }
            "for" if self.at_words(&["for", "mirrodin"]) => {
                self.pos += 2;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::ForMirrodin))
            }
            "training" | "melee" | "dethrone" | "myriad"
                if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) =>
            {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(match w.as_str() {
                    "training" => Mechanic::Training,
                    "melee" => Mechanic::Melee,
                    "dethrone" => Mechanic::Dethrone,
                    _ => Mechanic::Myriad,
                }))
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
            // CR 702.72a: `champion Goblin or Shaman`.
            "champion" if matches!(self.peek_at(1), Some(Tok::Word(_))) => {
                self.pos += 1;
                let f = self.filter()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Champion(f)))
            }
            // CR 702.155a.
            "read" if self.at_words(&["read", "ahead"]) => {
                self.pos += 2;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::ReadAhead))
            }
            "haunt" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Haunt))
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
            | "afterlife" | "casualty" | "teamwork" | "ripple" | "dredge" | "devour"
            | "amplify" | "tribute" | "hideaway"
                if matches!(self.peek_at(1), Some(Tok::Num(_))) =>
            {
                self.pos += 1;
                let n = self.number()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(match w.as_str() {
                    "devour" => Mechanic::Devour(None, n),
                    "amplify" => Mechanic::Amplify(n),
                    "tribute" => Mechanic::Tribute(n),
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
                    "hideaway" => Mechanic::Hideaway(n),
                    "renown" => Mechanic::Renown(n),
                    "afterlife" => Mechanic::Afterlife(n),
                    "casualty" => Mechanic::Casualty(n),
                    "dredge" => Mechanic::Dredge(n),
                    "teamwork" => Mechanic::Teamwork(n),
                    "ripple" => Mechanic::Ripple(n),
                    _ => Mechanic::Toxic(n),
                }))
            }
            // CR 702.82b: "devour artifact 1", "devour land 3".
            "devour"
                if matches!(self.word_at(1).as_deref(), Some("artifact" | "land"))
                    && matches!(self.peek_at(2), Some(Tok::Num(_))) =>
            {
                self.pos += 1;
                let kind = if self.eat_word("artifact") {
                    mtg_core::card::CardType::Artifact
                } else {
                    self.expect_word("land")?;
                    mtg_core::card::CardType::Land
                };
                let n = self.number()?;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Devour(Some(kind), n)))
            }
            "ward" | "echo" | "unearth" | "buyback" | "outlast" | "morph" | "megamorph"
            | "disguise" | "evoke" | "madness" | "mayhem" | "foretell" | "plot" | "sneak"
            | "harmonize" | "entwine" | "ninjutsu" | "scavenge" | "dash" | "warp" | "embalm"
            | "eternalize" | "offspring" | "bestow" | "splice" | "strive" | "overload"
            | "surge" | "freerunning" | "spectacle" | "replicate" | "encore" | "multikicker"
            | "transmute" | "disturb" | "reconfigure"
                if matches!(self.peek_at(1), Some(Tok::Sym(_))) =>
            {
                self.pos += 1;
                // "Ward—{2}, Pay 2 life.": mana and another part, a cost
                // paid as the trigger resolves (CR 702.21a).
                if w == "ward" {
                    let mut i = 0;
                    while matches!(self.peek_at(i), Some(Tok::Sym(_))) {
                        i += 1;
                    }
                    if self.peek_at(i) == Some(&Tok::Comma) {
                        let c = self.cost()?;
                        self.end_stmt()?;
                        return Ok(Ability::Mechanic(Mechanic::WardCost(c)));
                    }
                }
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
                    "reconfigure" => Mechanic::Reconfigure(c),
                    _ => Mechanic::Outlast(c),
                }))
            }
            // "Ward—Pay 3 life.", "Morph—Discard a card.", "Buyback—
            // Sacrifice a land.": a keyword whose cost isn't mana.
            "ward" | "morph" | "buyback" | "splice" | "echo"
                if matches!(self.peek_at(1), Some(Tok::Word(_))) =>
            {
                self.pos += 1;
                if w == "ward" {
                    let m = self.ward_cost()?;
                    self.end_stmt()?;
                    return Ok(Ability::Mechanic(m));
                }
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
            "compleated" if self.peek_at(1).is_none_or(|t| *t == Tok::Newline) => {
                self.pos += 1;
                self.end_stmt()?;
                Ok(Ability::Mechanic(Mechanic::Compleated))
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
                        graveyard: false,
                        name: None,
                        old: false,
                        those: false,
                        more: None,
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
                if self.at_word("only") || self.at_word("not") || self.at_words(&["on", "costs"]) {
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
            // CR 702.148a: `cleave {1}{U}: <effects>`, the spell as cast
            // for its cleave cost.
            "cleave" if matches!(self.peek_at(1), Some(Tok::Sym(_))) => {
                self.pos += 1;
                let c = self.mana_cost()?;
                self.kicked_targets = None;
                self.kicked_instead = None;
                self.in_spell = true;
                let effects = self.effects_body()?;
                Ok(Ability::Mechanic(Mechanic::Cleave(
                    c,
                    Box::new(Ability::Spell {
                        targets: std::mem::take(&mut self.targets),
                        effects,
                    }),
                )))
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
                        && matches!(s.effects.as_slice(), [Effect::Modes(_) | Effect::ModesTwo(_)
                            | Effect::ModesOf(ModeHead::OneOrMore | ModeHead::UpTo(_), _)]));
                if lone_modes {
                    match effects.pop().and_then(|mut s| s.effects.pop()) {
                        Some(Effect::Modes(modes)) => return Ok(Ability::Modal(modes)),
                        Some(Effect::ModesOf(head, modes)) => {
                            return Ok(Ability::ModalOf(head, modes));
                        }
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
            "tiered" if self.peek_at(1) == Some(&Tok::LBrace) => {
                self.pos += 1;
                let modes = self.tiered_block()?;
                self.end_stmt()?;
                Ok(Ability::Tiered(modes))
            }
            "choose" => {
                if let Some((head, n)) = self.peek_mode_head() {
                    self.pos += n;
                    let modes = self.modes_block()?;
                    self.end_stmt()?;
                    if matches!(head, ModeHead::Unchosen { .. }) {
                        return self.fail(
                            "`choose one unchosen` is a triggered or activated ability's effect",
                        );
                    }
                    return Ok(Ability::ModalOf(head, modes));
                }
                self.pos += 1;
                if self.eat_word("two") {
                    let modes = self.modes_block()?;
                    self.end_stmt()?;
                    return Ok(Ability::ModalTwo(modes));
                }
                self.expect_word("one")?;
                let both = self.eat_words(&["or", "both"]);
                let any_number = !both && self.eat_words(&["or", "any", "number"]);
                let more = any_number || !both && self.eat_words(&["or", "more"]);
                if (both || more) && self.eat_word("if") {
                    let may = self.eat_word("may");
                    let cond = self.static_cond()?;
                    let modes = self.modes_block()?;
                    self.end_stmt()?;
                    return Ok(Ability::ModalIf {
                        more,
                        any_number,
                        may,
                        cond,
                        modes,
                    });
                }
                if more {
                    return self.fail("expected `if` after `choose one or more`");
                }
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
                // "Equip—{2}, Pay 2 life.", "Equip—Sacrifice a creature.":
                // a cost other than mana alone (CR 702.6a).
                let full = matches!(self.peek(), Some(Tok::Word(_)))
                    || (self.peek_at(1) == Some(&Tok::Comma)
                        && self.word_at(2).as_deref() != Some("once"));
                let cost = if full {
                    Cost {
                        sorcery: true,
                        ..self.cost()?
                    }
                } else if self.word_at(1).as_deref() == Some("or") {
                    // `equip {3} or discard 1 card`: two equip abilities.
                    let mana = self.mana_cost()?;
                    self.expect_word("or")?;
                    let other = Box::new(self.cost()?);
                    self.end_stmt()?;
                    return Ok(Ability::EquipOr {
                        mana,
                        other,
                        targets: vec![target],
                    });
                } else {
                    let mana = self.mana_cost()?;
                    // "Equip {0}. Activate only once each turn."
                    let per_turn = if self.peek() == Some(&Tok::Comma) {
                        self.pos += 1;
                        self.expect_word("once")?;
                        1
                    } else {
                        0
                    };
                    Cost {
                        mana,
                        sorcery: true,
                        per_turn,
                        ..Default::default()
                    }
                };
                self.end_stmt()?;
                Ok(Ability::Activated {
                    cost,
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
                } else if self.eat_words(&["once", "ever"]) {
                    mtg_core::card::TurnLimit::Ever
                } else {
                    mtg_core::card::TurnLimit::None
                };
                let when = self.at_word("when");
                let mut trigger = self.trigger()?;
                // "Whenever this creature deals combat damage, untap it":
                // with no recipient named, "it" is the creature dealing the
                // damage, not the object dealt it.
                self.it_source = matches!(&trigger, Trigger::CastsFiltered { filter, .. }
                    if filter.targeting.as_ref().and_then(|t| t.filter.as_ref())
                        .is_some_and(|f| f.relation == Relation::This))
                    || matches!(
                        &trigger,
                        Trigger::DealsDamage {
                            obj: TriggerObj::This,
                            to: mtg_core::card::DamageTo::Any,
                            ..
                        }
                    )
                    // "When there are nine or more incarnation counters on
                    // this enchantment, exile it." (Nine Lives): a state
                    // trigger has no triggering object; "it" is the source.
                    || matches!(&trigger, Trigger::State(_));
                self.no_trigger_obj = no_trigger_object(&trigger);
                // Also a cast trigger: the triggering object is a spell,
                // which "that creature" doesn't name.
                self.self_trigger = matches!(
                    trigger,
                    Trigger::Enters(TriggerObj::This)
                        | Trigger::EntersFrom {
                            obj: TriggerObj::This,
                            ..
                        }
                        | Trigger::Attacks(TriggerObj::This)
                        | Trigger::TurnedFaceUp(TriggerObj::This)
                        | Trigger::CastsSpell { .. }
                        | Trigger::CastsFiltered { .. }
                        | Trigger::CastThis { .. }
                        | Trigger::CastsNth { .. }
                );
                self.no_trigger_player = matches!(
                    trigger,
                    Trigger::Enters(TriggerObj::This)
                        | Trigger::EntersFrom {
                            obj: TriggerObj::This,
                            ..
                        }
                        | Trigger::Dies(TriggerObj::This)
                        | Trigger::DiesOrExiled(TriggerObj::This)
                        | Trigger::Leaves(TriggerObj::This)
                        | Trigger::TurnedFaceUp(TriggerObj::This)
                        | Trigger::CastThis { .. }
                );
                self.no_trigger_amount = no_trigger_amount(&trigger);
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
                    if self.eat_words(&["it", "had", "counter"]) {
                        Some(crate::ast::TrigCond::HadCounter { one_or_more: false })
                    } else if self.eat_words(&["it", "had", "one", "or", "more", "counters"]) {
                        Some(crate::ast::TrigCond::HadCounter { one_or_more: true })
                    } else if let Ok(c) = self.condition() {
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
            "during" if self.at_words(&["during", "your", "turn", "replace"]) => {
                self.pos += 3;
                let ability = self.ability()?;
                Ok(Ability::DuringYourTurn(Box::new(ability)))
            }
            // CR 614.12: "As this creature enters, it becomes your choice
            // of a 3/3 creature, a 2/2 creature with flying, or ...".
            "as" if self.at_words(&["as", "this", "enters", "becomes", "choice"]) => {
                self.pos += 5;
                let mut forms = Vec::new();
                loop {
                    let Some(Tok::Pt {
                        power,
                        toughness,
                        signed: false,
                    }) = self.next()
                    else {
                        return self.fail("expected a power and toughness in `becomes choice`");
                    };
                    let mut form = crate::ast::ChoiceForm {
                        power,
                        toughness,
                        subtypes: Vec::new(),
                        types: Vec::new(),
                        keywords: Vec::new(),
                    };
                    loop {
                        let w = self.word()?;
                        if let Some(t) = card_type(&w) {
                            form.types.push(t);
                            if t == CardType::Creature {
                                break;
                            }
                        } else if w.chars().next().is_some_and(char::is_uppercase) {
                            form.subtypes.push(w);
                        } else {
                            return self
                                .fail(format!("expected a type in `becomes choice`, found {w}"));
                        }
                    }
                    if self.eat_word("with") {
                        form.keywords = self.keyword_list()?;
                    }
                    forms.push(form);
                    if !self.eat_word("or") {
                        break;
                    }
                }
                let in_addition = self.eat_words(&["in", "addition"]);
                if forms.len() < 2 {
                    return self.fail("`becomes choice` needs two or more choices");
                }
                self.end_stmt()?;
                Ok(Ability::BecomesChoice { forms, in_addition })
            }
            "as" if self.at_words(&["as", "this", "enters", "choose"])
                || self.at_words(&["as", "it", "enters", "choose"])
                || self.at_words(&["as", "this", "enters", "look", "at", "opponent's"]) =>
            {
                let joined = self.at_words(&["as", "it"]);
                self.pos += 3;
                // "look at an opponent's hand, then choose any card name"
                // (Sorcerous Spyglass).
                let look = self.eat_words(&["look", "at", "opponent's", "hand", "then"]);
                self.expect_word("choose")?;
                let mut kind = self.choice_kind(false)?;
                if look {
                    let ChoiceKind::CardName { look: l, .. } = &mut kind else {
                        return self
                            .fail("`look at opponent's hand then` needs `choose card name`");
                    };
                    *l = true;
                }
                self.end_stmt()?;
                Ok(Ability::AsEntersChoose { kind, joined })
            }
            "enters"
                if self.at_words(&["enters", "as"])
                    || self.at_words(&["enters", "tapped", "as", "copy"]) =>
            {
                self.pos += 1;
                let tapped = self.eat_word("tapped");
                self.expect_word("as")?;
                self.expect_word("copy")?;
                self.expect_word("of")?;
                let f = self.filter()?;
                let graveyard = self.eat_words(&["card", "in", "graveyard"]);
                self.end_stmt()?;
                Ok(Ability::EntersAsCopy(
                    f,
                    mtg_core::card::EntersCopy { tapped, graveyard },
                ))
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
                    token: None,
                    wording: crate::ast::ThenWording::Sentence,
                    cond: None,
                }))
            }
            // "As this land enters, you may reveal a Mountain or Forest
            // card from your hand. If you don't, this land enters tapped."
            "enters" if self.at_words(&["enters", "tapped", "unless", "reveal"]) => {
                self.pos += 4;
                let filter = self.filter()?;
                if !(self.eat_word("card") || self.eat_word("cards")) {
                    return self.unexpected("`card`");
                }
                let or = if self.eat_word("or") {
                    Some(self.static_cond()?)
                } else {
                    None
                };
                self.end_stmt()?;
                Ok(Ability::EntersTappedUnlessReveal { filter, or })
            }
            "enters" if self.at_words(&["enters", "tapped", "if", "not", "your", "turn"]) => {
                self.pos += 6;
                self.end_stmt()?;
                Ok(Ability::EntersTappedIfNotYourTurn)
            }
            "enters" if self.at_words(&["enters", "tapped", "if"]) => {
                self.pos += 3;
                let c = self.static_cond()?;
                self.end_stmt()?;
                Ok(Ability::EntersTappedIf(c))
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
                if self.at_words(&["this", "spell", "can't", "be", "countered"]) {
                    self.pos += 5;
                    let unpreventable =
                        self.at_words(&["and", "the", "damage", "can't", "be", "prevented"]);
                    if unpreventable {
                        self.pos += 6;
                    }
                    self.end_stmt()?;
                    return Ok(Ability::UncounterableIf {
                        cond,
                        unpreventable,
                    });
                }
                self.expect_word("enters")?;
                self.expect_word("with")?;
                let n = self.number()?;
                let kind = self.counter_kind()?;
                let more = if self.eat_word("and") {
                    let m = self.number()?;
                    Some((m, self.counter_kind()?))
                } else {
                    None
                };
                self.end_stmt()?;
                Ok(Ability::EntersWithIf {
                    n,
                    kind,
                    cond,
                    first: true,
                    more,
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
                    more: None,
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
                        more: None,
                    });
                }
                self.end_stmt()?;
                Ok(Ability::Replacement(Replacement {
                    event: Replaces::Enters,
                    outcome,
                    then: Vec::new(),
                    token: None,
                    wording: crate::ast::ThenWording::Sentence,
                    cond: None,
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
                let plural = !each && self.eat_word("plural");
                let mut filter = self.filter()?;
                // "Each instant and sorcery card in your graveyard has
                // flashback." (CR 113.6b).
                if !plural
                    && self.at_word("card")
                    && matches!(self.word_at(1).as_deref(), Some("in" | "or"))
                {
                    return self.card_grant(filter, each, condition);
                }
                filter.each = each;
                filter.plural = plural;
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
                } else if condition.is_none() && self.eat_words(&["during", "your", "turn"]) {
                    // "Equipped creature has deathtouch during your turn."
                    condition = Some(StaticCond::DuringYourTurn);
                } else if condition.is_none() && self.eat_word("if") {
                    condition = Some(self.static_cond()?);
                    cond_if = true;
                }
                // "Otherwise, it gets -2/-2." (CR 611.3a).
                let mut otherwise = Vec::new();
                if condition.is_some() && self.eat_word("otherwise") {
                    loop {
                        otherwise.push(self.modification()?);
                        if self.peek() == Some(&Tok::Comma) {
                            self.pos += 1;
                            continue;
                        }
                        if self.eat_word("and") {
                            continue;
                        }
                        break;
                    }
                }
                self.end_stmt()?;
                Ok(Ability::Static {
                    filter,
                    mods,
                    condition,
                    cond_first,
                    cond_if,
                    otherwise,
                })
            }
            _ => self.unexpected("a statement"),
        }
    }

    /// `spells artifact you cast cost {1} less` (at `spells`), with the
    /// static condition written before it, if any.
    fn cost_change(&mut self, cond: Option<StaticCond>) -> R<Ability> {
        self.pos += 1;
        // `spells each red or green you cast cost {1} less`: "Each spell
        // you cast that's red or green costs {1} less to cast."
        let each = self.eat_word("each");
        let first = !each && self.eat_word("first");
        let mut filter = if self.at_word("you")
            || self.at_word("your")
            || self.at_word("cost")
            || self.at_word("can't")
        {
            Filter::default()
        } else {
            self.filter()?
        };
        filter.each = each;
        let whose = if self.eat_words(&["you", "cast"]) {
            Who::You
        } else if self.eat_words(&["your", "opponents", "cast"]) {
            Who::Opponents
        } else {
            Who::Any
        };
        // "Spells you control can't be countered." (the filter may have
        // read "you control"); "Spells can't be countered." (Lier).
        let mine = filter.who == Some(Who::You);
        if cond.is_none()
            && (self.eat_words(&["you", "control", "can't", "be", "countered"])
                || (mine && self.eat_words(&["can't", "be", "countered"])))
        {
            self.end_stmt()?;
            let mut filter = filter;
            filter.who = None;
            return Ok(Ability::SpellsHave {
                filter,
                from: mtg_core::card::SpellFrom::Anywhere,
                whose: Who::You,
                keywords: Vec::new(),
                storm: false,
                affinity: None,
                uncounterable: true,
                spend: None,
            });
        }
        if cond.is_none()
            && whose == Who::Any
            && filter.who.is_none()
            && self.eat_words(&["can't", "be", "countered"])
        {
            self.end_stmt()?;
            return Ok(Ability::SpellsHave {
                filter,
                from: mtg_core::card::SpellFrom::Anywhere,
                whose: Who::Any,
                keywords: Vec::new(),
                storm: false,
                affinity: None,
                uncounterable: true,
                spend: None,
            });
        }
        // "Instant and sorcery spells you cast [from your hand] have
        // storm", "Spells you cast from your graveyard cost {2} less".
        let from = if whose == Who::You {
            self.spell_from()
        } else {
            mtg_core::card::SpellFrom::Anywhere
        };
        if whose == Who::You && cond.is_none() && self.eat_word("have") {
            return self.spells_have(filter, from);
        }
        self.expect_word("cost")?;
        let n = i32::try_from(self.mana_cost()?.mana_value()).unwrap_or(0);
        let generic = if self.eat_word("less") {
            -n
        } else {
            self.expect_word("more")?;
            n
        };
        let per = if self.at_word("count") {
            Some(self.num()?)
        } else {
            None
        };
        let flash = first && self.eat_words(&["and", "flash"]);
        self.end_stmt()?;
        Ok(Ability::CostChange {
            filter,
            whose,
            generic,
            cond,
            per,
            first,
            flash,
            from,
        })
    }

    /// `from hand`, `from graveyard`, `from exile`, `from other than hand`
    /// after `spells <filter> you cast` (`SpellFrom`).
    fn spell_from(&mut self) -> mtg_core::card::SpellFrom {
        use mtg_core::card::SpellFrom;
        if self.eat_words(&["from", "hand"]) {
            SpellFrom::Hand
        } else if self.eat_words(&["from", "graveyard"]) {
            SpellFrom::Graveyard
        } else if self.eat_words(&["from", "exile"]) {
            SpellFrom::Exile
        } else if self.eat_words(&["from", "other", "than", "hand"]) {
            SpellFrom::NotHand
        } else {
            SpellFrom::Anywhere
        }
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
        // "When you do, Bolg deals damage ... . If excess damage was dealt
        // this way, amass Goblins X ...": every later sentence is the
        // reflexive ability's (CR 603.12).
        let s = self.sentences_inline();
        self.subject = saved;
        let targets = std::mem::replace(&mut self.targets, outer);
        Ok(Effect::WhenYouDo {
            targets,
            effects: s?,
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
        // `has hexproof from monocolored` (CR 702.11d).
        if self.at_words(&["hexproof", "from"]) {
            self.pos += 2;
            return Ok(Some(Ability::HexproofFrom(self.filter()?)));
        }
        // `has ward pay 2 life`: 'have "Ward—Pay 2 life."'.
        if self.at_words(&["ward", "pay"])
            || self.at_words(&["ward", "sacrifice"])
            || self.at_words(&["ward", "discard"])
        {
            self.pos += 1;
            return Ok(Some(Ability::Mechanic(self.ward_cost()?)));
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
        // `gains firebending 4`: "gains firebending 4 until end of turn"
        // (Fire Nation Palace).
        if self.at_word("firebending") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            return Ok(Some(Ability::Mechanic(Mechanic::Firebending(Num::N(
                self.number()?,
            )))));
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

    /// A ward cost other than mana alone (CR 702.21a): a cost (`ward pay
    /// 3 life`, `ward sacrifice 3 permanent`, `ward collect evidence 4`),
    /// `pay count equal power life`, `get 5 poison counters`, `you get 2
    /// poison counters`, and alternatives joined with `or` (`ward discard
    /// 1 card or {2}`).
    fn ward_cost(&mut self) -> R<Mechanic> {
        let mut alts = Vec::new();
        loop {
            let alt = if self.at_words(&["pay", "count"]) {
                self.pos += 1;
                let n = self.num()?;
                self.expect_word("life")?;
                crate::ast::WardAlt {
                    life: Some(n),
                    ..Default::default()
                }
            } else if self.at_word("get") || self.at_words(&["you", "get"]) {
                let you = self.eat_word("you");
                self.expect_word("get")?;
                let poison = self.number()?;
                self.expect_word("poison")?;
                if !self.eat_word("counters") {
                    self.expect_word("counter")?;
                }
                crate::ast::WardAlt {
                    poison,
                    you,
                    ..Default::default()
                }
            } else {
                crate::ast::WardAlt {
                    cost: self.cost()?,
                    ..Default::default()
                }
            };
            alts.push(alt);
            if !self.eat_word("or") {
                break;
            }
        }
        if let [a] = alts.as_slice()
            && a.life.is_none()
            && a.poison == 0
        {
            return Ok(Mechanic::WardCost(a.cost.clone()));
        }
        Ok(Mechanic::WardAlts(alts))
    }

    fn granted_mechanic_ahead(&self) -> bool {
        (self.at_word("ward") && matches!(self.peek_at(1), Some(Tok::Sym(_))))
            || self.at_words(&["ward", "pay"])
            || self.at_words(&["ward", "sacrifice"])
            || self.at_words(&["ward", "discard"])
            || (self.at_word("toxic") && matches!(self.peek_at(1), Some(Tok::Num(_))))
            || (self.at_word("firebending") && matches!(self.peek_at(1), Some(Tok::Num(_))))
            || self.granted_protection_ahead()
            || (self.at_words(&["hexproof", "from"])
                && self
                    .word_at(2)
                    .as_deref()
                    .is_some_and(|w| color(w).is_none()))
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
    /// `choose one or more {`, `choose up to 2 {`, `choose one unchosen
    /// [this turn] {` at the current position: the head and how many
    /// words it takes, up to the brace.
    fn peek_mode_head(&self) -> Option<(ModeHead, usize)> {
        let w = |i: usize, s: &str| matches!(self.peek_at(i), Some(Tok::Word(x)) if x == s);
        if !w(0, "choose") {
            return None;
        }
        let (head, n) = if w(1, "one") && w(2, "or") && w(3, "more") {
            (ModeHead::OneOrMore, 4)
        } else if w(1, "up") && w(2, "to") {
            match self.peek_at(3) {
                Some(Tok::Num(k)) if *k > 0 => (ModeHead::UpTo(u8::try_from(*k).ok()?), 4),
                _ => return None,
            }
        } else if w(1, "one") && w(2, "unchosen") {
            let this_turn = w(3, "this") && w(4, "turn");
            let n = if this_turn { 5 } else { 3 };
            let stop = w(n, "stop");
            (
                ModeHead::Unchosen { this_turn, stop },
                n + usize::from(stop),
            )
        } else {
            return None;
        };
        (self.peek_at(n) == Some(&Tok::LBrace)).then_some((head, n))
    }

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

    /// `{ mode "Thunder" {0}: ... }` after `tiered`: each mode's name, its
    /// cost, then its effects (CR 702.183).
    fn tiered_block(&mut self) -> R<Vec<(ManaCost, Mode)>> {
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
            let Some(Tok::Str(name)) = self.next() else {
                return self.fail("a tiered mode has a name: `mode \"Thunder\" {0}: ...`");
            };
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
                    name: Some(name),
                },
            ));
        }
        self.targets = outer;
        if modes.len() < 2 {
            return self.fail("`tiered` takes two or more modes");
        }
        Ok(modes)
    }

    /// After `this has`: `4 or more oil counters`, `no arrowhead counters`.
    fn source_counters_cond(&mut self) -> R<StaticCond> {
        if self.eat_words(&["one", "or", "more"]) {
            return Ok(StaticCond::SourceCountersOneOrMore(self.counter_kind()?));
        }
        if self.eat_word("no") {
            let kind = self.counter_kind()?;
            return Ok(StaticCond::SourceCounters {
                kind,
                n: 0,
                at_least: false,
            });
        }
        let n = self.number()?;
        // "as long as it has two or fewer oil counters on it".
        let at_least = !self.eat_words(&["or", "fewer"]);
        if at_least {
            self.expect_words(&["or", "more"])?;
        }
        // `this has 3 or more counters`: of every kind together.
        if at_least && n > 1 && self.eat_word("counters") {
            return Ok(StaticCond::SourceCounters {
                kind: CounterKind::Any,
                n,
                at_least,
            });
        }
        let kind = self.counter_kind()?;
        Ok(StaticCond::SourceCounters { kind, n, at_least })
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

    /// A condition, or two joined by `and` (both must hold): "if there is
    /// an Elf card in your graveyard and this creature has a -1/-1
    /// counter on it". The `and` binds only when a condition follows it.
    fn static_cond(&mut self) -> R<StaticCond> {
        let first = self.static_cond_one()?;
        if self.at_word("and") {
            let at = self.pos;
            self.pos += 1;
            if let Ok(second) = self.static_cond() {
                return Ok(StaticCond::And(Box::new(first), Box::new(second)));
            }
            self.pos = at;
        }
        Ok(first)
    }

    #[allow(clippy::too_many_lines)] // one branch per condition
    fn static_cond_one(&mut self) -> R<StaticCond> {
        // `either you control land creature else land entered this turn`:
        // either holds (`else`, where a filter's own `or` would run on).
        if self.eat_word("either") {
            let a = self.static_cond_one()?;
            self.expect_word("else")?;
            let b = self.static_cond()?;
            return Ok(StaticCond::Or(Box::new(a), Box::new(b)));
        }
        // `top card is Goblin card`: "the top card of your library is a
        // Goblin card".
        if self.eat_words(&["top", "card", "is"]) {
            return Ok(StaticCond::TopCard(self.filter()?));
        }
        // `flash if pay {2} more` (Mystical Tether).
        if self.at_word("pay")
            && matches!(self.peek_at(1), Some(Tok::Sym(_)))
            && self.word_at(2).as_deref() == Some("more")
        {
            self.pos += 1;
            let c = self.mana_cost()?;
            self.expect_word("more")?;
            let n = u8::try_from(c.mana_value()).unwrap_or(u8::MAX);
            return Ok(StaticCond::FlashPay(n));
        }
        if self.eat_word("void") {
            return Ok(StaticCond::Void);
        }
        if self.eat_words(&["not", "your", "main", "phase"]) {
            return Ok(StaticCond::NotYourMainPhase);
        }
        if self.eat_words(&["cast", "from", "anywhere", "other", "than", "hand"]) {
            return Ok(StaticCond::NotCastFromHand);
        }
        if self.eat_words(&["you", "cast", "it"]) {
            return Ok(StaticCond::WasCast);
        }
        // "if that creature entered from a graveyard or you cast it from a
        // graveyard", "if they entered or were cast from a graveyard", "if
        // one or more of them entered from exile or was cast from exile".
        if let Some(wording) = if self.eat_words(&["that", "creature", "entered", "from"]) {
            Some(0)
        } else if self.eat_words(&["they", "entered", "or", "were", "cast", "from"]) {
            Some(1)
        } else if self.eat_words(&["one", "or", "more", "of", "them", "entered", "from"]) {
            Some(2)
        } else {
            None
        } {
            let zone = self.came_from_zone()?;
            match wording {
                0 => self.expect_words(&["or", "you", "cast", "it", "from"])?,
                2 => self.expect_words(&["or", "was", "cast", "from"])?,
                _ => {}
            }
            if wording != 1 && self.came_from_zone()? != zone {
                return self.unexpected("the same zone twice");
            }
            return Ok(StaticCond::CameFrom { zone, wording });
        }
        if self.eat_words(&["it", "was", "cast"])
            || self.eat_words(&["he", "was", "cast"])
            || self.eat_words(&["she", "was", "cast"])
            || self.eat_words(&["they", "were", "cast"])
        {
            return Ok(StaticCond::WasCastPassive);
        }
        if self.eat_words(&["put", "any", "counter", "this", "turn"]) {
            return Ok(StaticCond::PutAnyCounterThisTurn);
        }
        if self.eat_words(&["opponent", "controls", "more", "lands"]) {
            return Ok(StaticCond::OpponentMoreLands);
        }
        // `that player hand 2 or less`, `defending player hand 2 or less`,
        // `that player more cards in hand than you`.
        // `opponent hand 0 or less`, `any player hand 0 or less`.
        let who = if self.at_words(&["that", "player"]) {
            Some((mtg_core::card::HandPlayer::That, 2))
        } else if self.at_words(&["defending", "player"]) {
            Some((mtg_core::card::HandPlayer::Defending, 2))
        } else if self.at_words(&["any", "player"]) {
            Some((mtg_core::card::HandPlayer::Anyone, 2))
        } else if self.at_word("opponent") {
            Some((mtg_core::card::HandPlayer::Opponent, 1))
        } else {
            None
        };
        if let Some((who, words)) = who
            && ((self.word_at(words).as_deref() == Some("hand")
                && matches!(self.peek_at(words + 1), Some(Tok::Num(_))))
                // "an opponent has more cards in hand than you" is
                // `opponent has more cards in hand`.
                || (words == 2
                    && self.word_at(words).as_deref() == Some("more")
                    && self.word_at(words + 1).as_deref() == Some("cards")))
        {
            self.pos += words;
            if self.eat_words(&["more", "cards", "in", "hand", "than", "you"]) {
                return Ok(StaticCond::PlayerMoreHand(who));
            }
            self.expect_word("hand")?;
            let n = self.number()?;
            self.expect_word("or")?;
            let at_most = if self.eat_word("less") {
                true
            } else {
                self.expect_word("more")?;
                false
            };
            return Ok(StaticCond::PlayerHand { who, n, at_most });
        }
        if self.eat_words(&["library", "empty"]) {
            return Ok(StaticCond::LibraryEmpty);
        }
        if self.eat_words(&["opponent", "controls", "more", "creatures"]) {
            return Ok(StaticCond::OpponentHasMore(
                mtg_core::card::MoreWhat::Creatures,
            ));
        }
        if self.eat_words(&["opponent", "has", "more", "life"]) {
            return Ok(StaticCond::OpponentHasMore(mtg_core::card::MoreWhat::Life));
        }
        if self.eat_words(&["opponent", "has", "more", "cards", "in", "hand"]) {
            return Ok(StaticCond::OpponentHasMore(
                mtg_core::card::MoreWhat::CardsInHand,
            ));
        }
        if self.eat_words(&[
            "creature", "died", "under", "your", "control", "this", "turn",
        ]) {
            return Ok(StaticCond::CreaturesDied { n: 1, yours: true });
        }
        if matches!(self.peek(), Some(Tok::Num(_)))
            && self.word_at(1).as_deref() == Some("creatures")
            && self.word_at(2).as_deref() == Some("died")
        {
            let n = self.number()?;
            self.expect_words(&["creatures", "died", "this", "turn"])?;
            return Ok(StaticCond::CreaturesDied { n, yours: false });
        }
        if matches!(self.peek(), Some(Tok::Num(_)))
            && self.word_at(1).as_deref() == Some("creatures")
            && self.word_at(2).as_deref() == Some("entered")
        {
            let n = self.number()?;
            self.expect_words(&["creatures", "entered", "this", "turn"])?;
            return Ok(StaticCond::CreaturesEnteredThisTurn {
                n: u8::try_from(n).unwrap_or(u8::MAX),
                opponent: false,
            });
        }
        if self.eat_words(&["opponent", "had"]) {
            let n = self.number()?;
            self.expect_words(&["creatures", "enter", "this", "turn"])?;
            return Ok(StaticCond::CreaturesEnteredThisTurn {
                n: u8::try_from(n).unwrap_or(u8::MAX),
                opponent: true,
            });
        }
        if self.eat_words(&["scried", "or", "surveilled", "this", "turn"]) {
            return Ok(StaticCond::ScriedOrSurveilledThisTurn);
        }
        if self.eat_words(&["total", "toughness"]) {
            let n = self.number()?;
            self.expect_words(&["or", "more"])?;
            return Ok(StaticCond::TotalToughness(n));
        }
        if self.eat_words(&["cast", "both"]) {
            let a = self.filter()?;
            self.expect_words(&["spell", "and"])?;
            let b = self.filter()?;
            self.expect_words(&["spell", "this", "turn"])?;
            return Ok(StaticCond::CastBoth(Box::new((a, b))));
        }
        if self.at_words(&["attacked", "with"]) && matches!(self.peek_at(2), Some(Tok::Num(_))) {
            self.pos += 2;
            let n = self.number()?;
            let f = self.filter()?;
            self.expect_words(&["this", "turn"])?;
            return Ok(StaticCond::AttackedWithAtLeast(f, n));
        }
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
        if self.eat_words(&["this", "isnt", "token"]) {
            return Ok(StaticCond::IsntToken);
        }
        // `this isnt creature`: card types only.
        if self.eat_words(&["this", "isnt"]) {
            let f = self.filter()?;
            if f.types.is_empty()
                || f != (Filter {
                    types: f.types.clone(),
                    ..Filter::default()
                })
            {
                return self.fail("`this isnt` takes card types only");
            }
            return Ok(StaticCond::Isnt(f));
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
        if self.eat_words(&["attacked", "or", "blocked", "this", "combat"]) {
            return Ok(StaticCond::AttackedOrBlocked);
        }
        if self.eat_words(&["attacked", "total", "power"]) {
            let n = self.number()?;
            return Ok(StaticCond::AttackedTotalPower(
                u16::try_from(n).unwrap_or(u16::MAX),
            ));
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
        if self.eat_words(&["lost", "life", "this", "turn"]) {
            return Ok(StaticCond::LostLifeThisTurn { gained: false });
        }
        if self.eat_words(&["gained", "and", "lost", "life", "this", "turn"]) {
            return Ok(StaticCond::LostLifeThisTurn { gained: true });
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
        // `permanent to graveyard this turn`: "a permanent was put into a
        // graveyard from the battlefield this turn" (any types).
        if self.eat_words(&["permanent", "to", "graveyard", "this", "turn"]) {
            return Ok(StaticCond::PutIntoGraveyardThisTurn(Vec::new()));
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
        // `sacrificed permanent this turn`: "you sacrificed a permanent
        // this turn" (no types: any permanent).
        if self.eat_words(&["sacrificed", "permanent", "this", "turn"]) {
            return Ok(StaticCond::SacrificedThisTurn(Vec::new()));
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
        if self.eat_words(&["didn't", "cast", "this", "turn"]) {
            return Ok(StaticCond::DidntCastThisTurn);
        }
        if self.eat_words(&["not", "cast", "this", "turn"]) {
            return Ok(StaticCond::NoSpellsCastThisTurn { from_hand: false });
        }
        if self.eat_words(&["not", "cast", "from", "hand", "this", "turn"]) {
            return Ok(StaticCond::NoSpellsCastThisTurn { from_hand: true });
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
        // `x 10 or more`: "X is 10 or more" (CR 107.3).
        if self.word_at(0).is_some_and(|w| w.eq_ignore_ascii_case("x"))
            && matches!(self.peek_at(1), Some(Tok::Num(_)))
        {
            self.pos += 1;
            let n = self.number()?;
            self.expect_word("or")?;
            let at_least = if self.eat_word("more") {
                true
            } else {
                self.expect_word("less")?;
                false
            };
            return Ok(StaticCond::XIs { n, at_least });
        }
        if self.eat_words(&["not", "kicked"]) {
            return Ok(StaticCond::NotKicked);
        }
        if self.eat_words(&["that", "spell", "kicked"]) {
            return Ok(StaticCond::ThatKicked);
        }
        if self.at_word("kicked") && self.word_at(1).as_deref() != Some("with") {
            self.pos += 1;
            return Ok(StaticCond::Kicked);
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
        // `this sneak paid`: "this creature's sneak cost was paid"
        // (Turncoat Kunoichi).
        if self.eat_words(&["this", "sneak", "paid"]) {
            return Ok(StaticCond::SneakPaid(3));
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
        if self.eat_words(&["prowl", "paid"]) {
            return Ok(StaticCond::ProwlPaid);
        }
        // CR 701.38: `more votes for death [or tied]`.
        if self.eat_words(&["more", "votes", "for"]) {
            let word = self.word()?;
            let Some(index) = self.votes.iter().position(|w| *w == word) else {
                return self.fail(format!("`{word}` is not a choice of the ability's vote"));
            };
            let ties = self.eat_words(&["or", "tied"]);
            return Ok(StaticCond::MoreVotes {
                word,
                index: u8::try_from(index).unwrap_or(u8::MAX),
                ties,
            });
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
        if self.at_words(&["opponent", "was", "dealt"])
            && matches!(self.peek_at(3), Some(Tok::Num(_)))
        {
            self.pos += 3;
            let n = self.number()?;
            self.expect_words(&["damage", "this", "turn"])?;
            return Ok(StaticCond::OpponentDamageThisTurn(n));
        }
        if self.eat_words(&["no", "cards", "in", "any", "hand"]) {
            return Ok(StaticCond::EachHandEmpty);
        }
        if self.at_word("library") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            let n = self.number()?;
            self.expect_words(&["or", "less"])?;
            return Ok(StaticCond::LibraryAtMost(n));
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
        // "this creature doesn't have a flying counter on it".
        if self.eat_words(&["this", "doesn't", "have"]) {
            return Ok(StaticCond::SourceLacksCounter(self.counter_kind()?));
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
        // `activated 4 times this turn`: "this ability has been activated
        // four or more times this turn".
        if self.at_word("activated") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            let n = self.number()?;
            self.expect_words(&["times", "this", "turn"])?;
            return Ok(StaticCond::ActivatedThisTurn(n));
        }
        if self.eat_word("hand") {
            // `hand has no land card`: "you have no land cards in hand".
            if self.eat_words(&["has", "no"]) {
                let f = self.filter()?;
                self.cards_word()?;
                return Ok(StaticCond::NoCardInHand(f));
            }
            // `hand fewer than 10`: "you have fewer than ten cards in hand".
            if self.eat_words(&["fewer", "than"]) {
                return Ok(StaticCond::HandFewerThan(self.number()?));
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
        // `your opponents control 3 creature`: "your opponents control
        // three or more creatures", counted together (Lashwhip Predator).
        if self.eat_words(&["your", "opponents", "control"]) {
            let at_least = if matches!(self.peek(), Some(Tok::Num(_))) {
                self.number()?
            } else {
                1
            };
            let mut filter = self.filter()?;
            filter.who = Some(Who::Opponents);
            filter.your_opponents = true;
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
        if self.eat_words(&["you", "control", "at", "least"]) {
            let n = self.number()?;
            let filter = self.filter()?;
            return Ok(StaticCond::YouControlAtLeast { filter, n });
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
        // CR 103.4: against the starting life total.
        if self.at_word("life")
            && (self.at_words(&["life", "above", "starting"])
                || self.at_words(&["life", "at", "least", "starting"])
                || self.at_words(&["life", "below", "starting"])
                || (matches!(self.peek_at(1), Some(Tok::Num(_)))
                    && (self.word_at(2).as_deref() == Some("above")
                        || (self.word_at(2).as_deref() == Some("more")
                            && self.word_at(3).as_deref() == Some("than")))))
        {
            self.pos += 1;
            let (at_least, below, wording) = if self.eat_words(&["above", "starting"]) {
                (1, false, 0)
            } else if self.eat_words(&["at", "least", "starting"]) {
                (0, false, 1)
            } else if self.eat_words(&["below", "starting"]) {
                (0, true, 4)
            } else {
                let n = i32::try_from(self.number()?).unwrap_or(0);
                if self.eat_word("above") {
                    self.expect_word("starting")?;
                    (n, false, 2)
                } else {
                    self.expect_words(&["more", "than", "starting"])?;
                    (n, false, 3)
                }
            };
            return Ok(StaticCond::LifeVsStarting {
                at_least,
                below,
                wording,
            });
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
        // "As long as you control enchanted creature" (Mishra's
        // Domination): the attached object is yours.
        if self.eat_words(&["you", "control", "attached"]) {
            return Ok(StaticCond::AttachedIs(Filter {
                who: Some(Who::You),
                ..Default::default()
            }));
        }
        if self.eat_words(&["team", "controls"]) {
            return Ok(StaticCond::TeamControls(self.filter()?));
        }
        if self.eat_words(&["you", "control"]) {
            if matches!(self.peek(), Some(Tok::Num(1)))
                && self.word_at(1).as_deref() == Some("or")
                && self.word_at(2).as_deref() == Some("more")
            {
                self.pos += 3;
                return Ok(StaticCond::YouControlOneOrMore(self.filter()?));
            }
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
        if self.eat_words(&["cards", "exiled", "with", "this"]) {
            return Ok(StaticCond::ExiledWith(n));
        }
        if self.eat_words(&["card", "types", "exiled", "with", "this"]) {
            return Ok(StaticCond::TypesExiledWith(n));
        }
        if self.eat_words(&["cards", "are", "in", "your", "graveyard"]) {
            return Ok(StaticCond::GraveyardCards {
                filter: Filter::default(),
                at_least: n,
                wording: crate::ast::GraveyardWording::AreIn,
            });
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
    #[allow(clippy::too_many_lines)] // one branch per join and duration
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
        // "For each opponent, tap up to one target creature that player
        // controls." (one opponent in a two-player game).
        let per_opponent =
            self.at_words(&["for", "each", "opponent"]) && self.peek_at(3) == Some(&Tok::Comma);
        if per_opponent {
            self.pos += 4;
        }
        let was_per_opponent = std::mem::replace(&mut self.per_opponent, per_opponent);
        // "until end of turn, <clauses>": the duration printed first.
        let mut lasting = None;
        let eot_first = if self.eat_words(&["until", "your", "next", "turn"]) {
            // "Until your next turn, <clauses>" (CR 611.2b).
            lasting = Some(mtg_core::card::Lasting::YourNextTurn);
            true
        } else if self.eat_words(&["until", "end", "of", "combat"]) {
            // "Until end of combat, <clauses>" (CR 500.5a).
            lasting = Some(mtg_core::card::Lasting::EndOfCombat);
            true
        } else {
            self.eat_words(&["until", "end", "of", "turn"])
        };
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
        // `<clauses> until your next turn`, `... for as long as this
        // remains` (CR 611.2b).
        if lasting.is_none() {
            lasting = self.lasting_suffix();
        }
        // "Until end of turn, whenever ..., <effects>": a delayed trigger
        // that lasts the turn (CR 603.7c), as `whenever ... this turn`.
        if eot_first
            && lasting.is_none()
            && let Some(Effect::Delayed {
                this_turn,
                trigger_first: true,
                ..
            }) = clauses.first_mut()
        {
            *this_turn = true;
        }
        self.per_opponent = was_per_opponent;
        let s = Sentence {
            effects: clauses,
            label,
            para: false,
            eot_first,
            lasting,
            then_first,
            per_opponent,
            joins,
        };
        self.condition_suffix(s)
    }

    /// After `cast free`: `target ...` (a card), or `[<filter>] spell from
    /// hand [with mana value up to <n>]`; then `exile instead`.
    fn cast_free(&mut self) -> R<Effect> {
        // `cast free card exiled with this`: "you may cast a card exiled
        // with Krang & Shredder without paying its mana cost" (CR 607.2a).
        let exiled = (0..8).any(|k| {
            self.word_at(k).as_deref() == Some("exiled")
                && self.word_at(k + 1).as_deref() == Some("with")
                && self.word_at(k + 2).as_deref() == Some("this")
        }) && !self.at_word("target");
        let (what, filter, max) = if exiled {
            let f = if self.eat_word("card") {
                Filter::default()
            } else {
                let f = self.filter()?;
                self.expect_word("card")?;
                f
            };
            self.expect_words(&["exiled", "with", "this"])?;
            let i = if let Some(i) = self.groups.iter().position(|g| *g == f) {
                i
            } else {
                self.groups.push(f);
                self.groups.len() - 1
            };
            let i = u16::try_from(i).map_err(|_| ParseError {
                line: self.line(),
                message: "too many groups".to_string(),
            })?;
            (Some(Recipient::ExiledWith(i)), None, None)
        } else if self.at_word("target") || self.at_words(&["exiled", "card"]) {
            let what = self.recipient()?;
            // `cast free exiled card that is instant spell with mana value
            // up to 2`: "if it's an instant spell with mana value 2 or
            // less" (Discover the Impossible; CR 406.3b, the spell's
            // qualities).
            let filter = if self.eat_words(&["that", "is"]) {
                let f = self.filter()?;
                self.expect_word("spell")?;
                Some(f)
            } else {
                None
            };
            // `cast free exiled card with mana value up to 4`: "if that
            // spell's mana value is 4 or less" (Beseech the Mirror).
            let max = if self.eat_words(&["with", "mana", "value", "up", "to"]) {
                Some(self.num()?)
            } else {
                None
            };
            (Some(what), filter, max)
        } else {
            let filter = if self.at_word("spell") {
                None
            } else {
                Some(self.filter()?)
            };
            self.expect_words(&["spell", "from", "hand"])?;
            let max = if self.eat_words(&["with", "mana", "value", "up", "to"]) {
                Some(self.num()?)
            } else {
                None
            };
            (None, filter, max)
        };
        let exile = self.eat_words(&["exile", "instead"]);
        Ok(Effect::CastFree {
            what,
            filter,
            max,
            exile,
            play: false,
        })
    }

    /// After `spells <filter> you cast [from hand] have`: convoke,
    /// improvise, delve, cascade, `storm`, `affinity for <filter>`.
    fn spells_have(&mut self, filter: Filter, from: mtg_core::card::SpellFrom) -> R<Ability> {
        // CR 609.4b: "You may spend mana as though it were mana of any
        // color to cast planeswalker spells."
        if self.eat_word("spend") {
            let kind = if self.eat_words(&["any", "type"]) {
                mtg_core::card::SpendAs::AnyType
            } else if self.eat_words(&["any", "color"]) {
                mtg_core::card::SpendAs::AnyColor
            } else {
                return self.unexpected("`any type` or `any color`");
            };
            let can = self.eat_word("can");
            let abilities = self.eat_words(&["and", "abilities"]);
            self.end_stmt()?;
            return Ok(Ability::SpellsHave {
                filter,
                from,
                whose: Who::You,
                keywords: Vec::new(),
                storm: false,
                affinity: None,
                uncounterable: false,
                spend: Some((kind, can, abilities)),
            });
        }
        let mut keywords = Vec::new();
        let mut storm = false;
        let mut affinity = None;
        loop {
            if self.eat_word("storm") {
                storm = true;
            } else if self.eat_words(&["affinity", "for"]) {
                affinity = Some(self.filter()?);
            } else {
                match self.keyword() {
                    Some(
                        k @ (Keyword::Convoke
                        | Keyword::Improvise
                        | Keyword::Delve
                        | Keyword::Cascade
                        | Keyword::Flash),
                    ) => keywords.push(k),
                    _ => return self.fail(
                        "spells can have convoke, improvise, delve, cascade, storm, or affinity",
                    ),
                }
            }
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
                continue;
            }
            if !self.eat_word("and") {
                break;
            }
        }
        self.end_stmt()?;
        Ok(Ability::SpellsHave {
            filter,
            from,
            whose: Who::You,
            keywords,
            storm,
            affinity,
            uncounterable: false,
            spend: None,
        })
    }

    /// A sentence's duration after its clauses (CR 611.2b): `until your
    /// next turn`, `for as long as this remains`, `for as long as it
    /// remains tapped`, `for as long as you control this`.
    fn lasting_suffix(&mut self) -> Option<mtg_core::card::Lasting> {
        use mtg_core::card::Lasting;
        if self.eat_words(&["until", "your", "next", "turn"]) {
            return Some(Lasting::YourNextTurn);
        }
        if self.eat_words(&["for", "as", "long", "as", "this", "remains", "tapped"]) {
            return Some(Lasting::SourceTapped);
        }
        if self.eat_words(&["for", "as", "long", "as", "this", "remains"]) {
            return Some(Lasting::SourceOnBattlefield);
        }
        if self.eat_words(&["for", "as", "long", "as", "it", "remains", "tapped"]) {
            return Some(Lasting::WhileTapped);
        }
        if self.eat_words(&["for", "as", "long", "as", "you", "control", "this"]) {
            return Some(Lasting::YouControlSource);
        }
        if self.eat_words(&["until", "this", "is", "cast", "from", "exile"]) {
            return Some(Lasting::UntilCastFromExile);
        }
        if self.eat_words(&["until", "end", "of", "combat"]) {
            return Some(Lasting::EndOfCombat);
        }
        // `that token gains haste indefinitely`: no duration ("That token
        // gains haste.").
        if self.eat_word("indefinitely") {
            return Some(Lasting::Forever);
        }
        None
    }

    /// A condition after a whole sentence: `<sentence> instead if <cond>`
    /// (CR 608.2c), or `<sentence> if <cond>` ("Draw a card if ...").
    fn condition_suffix(&mut self, s: Sentence) -> R<Sentence> {
        // `<sentence> instead if <cond>` (CR 608.2c).
        if self.at_words(&["instead", "if"]) || self.at_words(&["instead", "bare", "if"]) {
            let bare = self.at_words(&["instead", "bare"]);
            self.pos += if bare { 3 } else { 2 };
            let cond = self.instead_cond()?;
            return Ok(Sentence::new(vec![Effect::Instead {
                cond,
                wording: if bare {
                    InsteadWording::AfterBare
                } else {
                    InsteadWording::After
                },
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
                not: None,
            };
            return Ok(Sentence::new(vec![Effect::IfTarget(
                n - 1,
                filter,
                vec![s],
                check,
            )]));
        }
        // `<sentence> also if <cond>`: "It also gets +1/+0 until end of
        // turn if you control a Mouse."
        if self.at_words(&["also", "if"]) {
            self.pos += 2;
            let cond = self.static_cond()?;
            return Ok(Sentence::new(vec![Effect::IfCondition(
                cond,
                vec![s],
                crate::ast::IfWording::AlsoAfter,
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
    #[allow(clippy::too_many_lines)] // one block per kind of payment
    fn unless_suffix(&mut self, e: Effect) -> R<Effect> {
        if !self.at_word("unless") {
            return Ok(e);
        }
        // `you lose 1 life unless opponent life 10 or less`: a static
        // condition, judged as it resolves.
        // `unless you control Villain` too: "unless you control a
        // Villain" (The Spot's Portal).
        if !matches!(self.word_at(1).as_deref(), Some("you" | "they"))
            || self.at_words(&["unless", "you", "control"])
        {
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
        // `unless any player may this deals 4 damage to you`: "unless a
        // player has this creature deal 4 damage to them".
        if self.at_words(&["unless", "any", "player", "may"]) {
            self.pos += 4;
            let act = self.clause()?;
            return Ok(Effect::UnlessAnyPlayer {
                act: Box::new(act),
                effect: Box::new(e),
            });
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
        // "unless they sacrifice a nonland permanent of their choice or
        // discard a card" (CR 608.2d).
        let sacrifices = |p: &Self| {
            p.at_words(&["they", "sacrifice"])
                || (1..5).any(|k| {
                    matches!(p.peek_at(k), Some(Tok::Word(w)) if w == "sacrifices")
                        && !(0..k).any(|j| {
                            matches!(p.peek_at(j), Some(Tok::Word(w)) if w == "pays" || w == "discards")
                        })
                })
        };
        if sacrifices(self) {
            let payer = if self.eat_words(&["they", "sacrifice"]) {
                match self.subject {
                    Some(s) => s,
                    None => {
                        return self.fail("`unless they sacrifice` needs a clause with a subject");
                    }
                }
            } else {
                let r = self.recipient()?;
                self.expect_word("sacrifices")?;
                r
            };
            let filter = self.filter()?;
            // "unless they sacrifice another nonland permanent" (Torment of
            // Venom): other than the target, but `another` is other than
            // the source, and a spell isn't on the battlefield.
            if self.in_spell && filter.relation == Relation::Other {
                return self.fail(
                    "`unless they sacrifice another` in a spell: `another` excludes the source, not the target; this form isn't supported",
                );
            }
            // `else discard 1 card` (`or` would start another clause).
            let or_discard = self.at_word("else")
                && matches!(self.word_at(1).as_deref(), Some("discard" | "discards"))
                && matches!(self.peek_at(2), Some(Tok::Num(1)))
                && self.word_at(3).as_deref() == Some("card");
            if or_discard {
                self.pos += 4;
            }
            return Ok(Effect::UnlessSacrifices {
                payer,
                filter,
                or_discard,
                effect: Box::new(e),
            });
        }
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
            let discard_random = self.eat_words(&["at", "random"]);
            return Ok(Effect::UnlessPays {
                payer,
                mana: ManaCost::default(),
                life: 0,
                discard,
                waterbend: false,
                discard_random,
                both: false,
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
        let waterbend = self.eat_word("waterbend");
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
            waterbend,
            discard_random: false,
            both: false,
            effect: Box::new(e),
        })
    }

    /// `{2}` or `2 life`.
    fn pay_amount(&mut self) -> R<(ManaCost, u32)> {
        if matches!(self.peek(), Some(Tok::Sym(_))) {
            let mana = self.mana_cost()?;
            // "you may pay {W}{B} and 2 life" (Zoraline, Cosmos Caller).
            if self.at_word("and")
                && matches!(self.peek_at(1), Some(Tok::Num(_)))
                && self.word_at(2).as_deref() == Some("life")
            {
                self.pos += 1;
                let n = self.number()?;
                self.expect_word("life")?;
                return Ok((mana, n));
            }
            return Ok((mana, 0));
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
        if self.at_words(&["those", "targets"])
            || self.at_words(&["the", "chosen", "targets"])
            || (self.at_words(&["the", "chosen", "target"])
                && matches!(self.peek_at(3), Some(Tok::Num(_))))
            || self.at_words(&["the", "full", "target"])
            || self.at_words(&["the", "chosen", "full", "target"])
            || (self.at_words(&["the", "other", "target"])
                && matches!(self.peek_at(3), Some(Tok::Num(_))))
        {
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
            || w == "granter"
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
            || self.at_words(&["triggering", "it"])
            || self.at_words(&["controller", "of", "that", "creature"])
            || self.at_words(&["controller", "of", "enchanted"])
            || self.at_words(&["controller", "of", "granter"])
        {
            return true;
        }
        if w == "any" && self.word_at(1).as_deref() == Some("target") {
            return true;
        }
        // "They gain haste until end of turn." after creating tokens
        // (Salt Road Skirmish).
        if (self.at_words(&["those", "tokens"]) || self.at_words(&["those", "copies"]))
            && matches!(
                self.word_at(2).as_deref(),
                Some("gain" | "gains" | "get" | "gets")
            )
        {
            return true;
        }
        if w == "that" && matches!(self.word_at(1).as_deref(), Some("card" | "target")) {
            return true;
        }
        if w == "each" || w == "all" || w == "every" {
            return true;
        }
        self.at_words(&["an", "opponent", "gains", "control"])
            || RECIPIENTS.iter().any(|(ws, _)| self.at_words(ws))
    }

    /// Verbs that take the previous clause's subject when none is written
    /// ("you draw a card and lose 1 life").
    /// After `remove`: a recipient followed by `from combat` ("remove it
    /// from combat"), not counters.
    fn peek_recipient_from_combat(&self) -> bool {
        (1..8).any(|k| {
            self.word_at(k).as_deref() == Some("from")
                && self.word_at(k + 1).as_deref() == Some("combat")
                && (0..k).all(|j| !matches!(self.peek_at(j), Some(Tok::Comma | Tok::Semi)))
        })
    }

    fn at_subject_verb(&self) -> bool {
        if self.revealed && self.at_word("puts") {
            return true;
        }
        // "That creature can't be blocked this turn and has base power
        // and toughness 1/1 until end of turn."
        if self.at_words(&["has", "base"]) {
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
                revealed: false,
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
            revealed: false,
            then: vec![s],
        })
    }

    /// `split top 3 [of <who>] [reveal] [opponent] into hand, top, bottom
    /// [rest <dest>] [then]` (`Effect::DigSplit`).
    fn dig_split(&mut self) -> R<Effect> {
        let count = self.num()?;
        let of = if self.eat_word("of") {
            Some(self.recipient()?)
        } else {
            None
        };
        let reveal = self.eat_word("reveal");
        let opponent = self.eat_word("opponent");
        self.expect_word("into")?;
        let mut piles = vec![self.split_dest()?];
        while self.peek() == Some(&Tok::Comma) {
            self.pos += 1;
            piles.push(self.split_dest()?);
        }
        let rest = if self.eat_word("rest") {
            Some(self.split_dest()?)
        } else {
            None
        };
        let then = self.eat_word("then");
        Ok(Effect::DigSplit {
            count,
            of,
            reveal,
            opponent,
            piles,
            rest,
            then,
        })
    }

    /// `piles look|reveal N [face-down] [separated by opponent|<who>]
    /// [chosen by opponent] [the piles]` (`Effect::Piles`).
    #[inline(never)]
    fn piles(&mut self) -> R<Effect> {
        use mtg_core::card::PileForm;
        let verb = self.word()?;
        let reveal = verb == "reveal";
        // `piles exiled ...`: the cards the resolution exiled.
        let count = if verb == "exiled" {
            Num::N(0)
        } else {
            self.num()?
        };
        let mut form = match verb.as_str() {
            "exiled" => PileForm::ExiledToBattlefield,
            "exile" => PileForm::PlayFree,
            _ => PileForm::HandGraveyard,
        };
        if verb == "exile" && self.eat_word("twice") {
            form = PileForm::TwoFaceDown;
        }
        let face_down = self.eat_word("face-down");
        let separator = if self.eat_words(&["separated", "by"]) {
            if self.eat_word("opponent") {
                Some(Recipient::EachOpponent)
            } else {
                Some(self.recipient()?)
            }
        } else {
            None
        };
        let opponent_chooses = self.eat_words(&["chosen", "by", "opponent"]);
        if separator.is_some() == opponent_chooses {
            return self.fail("`piles`: an opponent either separates or chooses");
        }
        let the_piles = self.eat_words(&["the", "piles"]);
        let tail = if self.eat_words(&["one", "to", "hand", "rest", "bottom"]) {
            Some(PileForm::OneToHandRestBottom)
        } else if self.eat_words(&["onto", "battlefield"]) {
            Some(PileForm::ExiledToBattlefield)
        } else if self.eat_words(&["play", "free"]) {
            Some(PileForm::PlayFree)
        } else {
            None
        };
        // The form's words must agree with how the cards were taken.
        let ok = matches!(
            (form, tail),
            (
                PileForm::HandGraveyard,
                None | Some(PileForm::OneToHandRestBottom)
            ) | (
                PileForm::ExiledToBattlefield,
                Some(PileForm::ExiledToBattlefield)
            ) | (PileForm::PlayFree, Some(PileForm::PlayFree))
                | (PileForm::TwoFaceDown, None)
        );
        if !ok {
            return self.fail(
                "`piles`: `exiled ... onto battlefield`, `exile N ... play free`, `exile N twice`, or `look|reveal N [one to hand rest bottom]`",
            );
        }
        if let Some(t) = tail {
            form = t;
        }
        Ok(Effect::Piles {
            count,
            reveal,
            face_down,
            separator,
            opponent_chooses,
            the_piles,
            form,
        })
    }

    fn split_dest(&mut self) -> R<mtg_core::card::SplitDest> {
        use mtg_core::card::SplitDest;
        Ok(match self.word()?.as_str() {
            "hand" => SplitDest::Hand,
            "graveyard" => SplitDest::Graveyard,
            "top" => SplitDest::Top,
            "bottom" => SplitDest::Bottom,
            "exile" => SplitDest::Exile,
            w => return self.fail(format!("`split`: unknown destination `{w}`")),
        })
    }

    /// `if revealed creature card, <sentence>`: "If a creature card is
    /// revealed this way, ...".
    fn if_revealed_card(&mut self) -> R<Effect> {
        self.pos += 2;
        let filter = self.filter()?;
        self.expect_word("card")?;
        self.expect(&Tok::Comma)?;
        let saved = self.subject.take();
        let s = self.sentence()?;
        self.subject = saved;
        Ok(Effect::IfCard {
            filter,
            negated: false,
            card: true,
            that: false,
            shares_type: false,
            revealed: true,
            then: vec![s],
        })
    }

    /// A target check can't ask whether the target is what the source
    /// enchants: `enchanted creature` there would mean that, and print
    /// "an enchanted creature" (Karametra's Blessing), which is `creature
    /// with aura`.
    fn no_host_check(f: &Filter) -> Result<(), String> {
        if f.enchanted || f.or.as_ref().is_some_and(|g| g.enchanted) {
            return Err(
                "`if target N is enchanted ...`: write `creature with aura` (an Aura on it) or `creature with equipment`".to_string(),
            );
        }
        Ok(())
    }

    /// `if target 1 was Human, ...`: "If that creature was a Human, ...",
    /// judged on the target's last state.
    fn if_target(&mut self) -> R<Effect> {
        self.pos += 1;
        let full = self.eat_words(&["the", "full"]);
        let that = !full && self.eat_word("that");
        self.pos += 1;
        let n = self.number()? as usize;
        if n == 0 || n > self.targets.len() {
            return self.fail(format!("target {n} has not been declared"));
        }
        // `if target 1 isnt creature`: "If it isn't a creature", the
        // check `noncreature`.
        let not = if self.eat_word("isnt") {
            Some(crate::ast::NotWording::Isnt)
        } else if self.eat_words(&["is", "not"]) {
            Some(crate::ast::NotWording::IsNot)
        } else {
            None
        };
        if not.is_some() || self.eat_word("is") {
            // `if target 1 is Human, ... instead`: an "instead"; otherwise
            // the target as it resolves ("If it's a Vampire, it also gains
            // lifelink until end of turn").
            let mut filter = self.filter()?;
            if not.is_some() {
                if filter.types.is_empty()
                    || filter
                        != (Filter {
                            types: filter.types.clone(),
                            ..Filter::default()
                        })
                {
                    return self.fail("`isnt` takes card types only");
                }
                filter = Filter {
                    not_types: std::mem::take(&mut filter.types),
                    ..Filter::default()
                };
            }
            Self::no_host_check(&filter).or_else(|m| self.fail(m))?;
            self.expect(&Tok::Comma)?;
            let tense = if full {
                crate::ast::TargetTense::TheFullIs
            } else if that {
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
                    not,
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
                        not,
                    };
                    Ok(Effect::IfTarget(n - 1, filter, vec![s], check))
                }
            };
        }
        if full {
            return self.fail("`if the full target N` takes `is`");
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
        Self::no_host_check(&f).or_else(|m| self.fail(m))?;
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
                not: None,
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
        let at_least_one = self.eat_words(&["at", "least", "one"]);
        let (filter, card) = self.this_way_what()?;
        let wording = if at_least_one {
            self.expect_word("was")?;
            ThisWayWording::AtLeastOne
        } else if self.eat_word("is") {
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

    /// A search's destination after `to`.
    fn search_to(&mut self) -> R<mtg_core::card::SearchTo> {
        use mtg_core::card::SearchTo;
        Ok(if self.eat_words(&["hand", "or", "graveyard"]) {
            SearchTo::HandOrGraveyard
        } else if self.eat_words(&["hand", "or", "battlefield"]) {
            SearchTo::HandOrBattlefield
        } else if self.eat_words(&["hand", "and", "graveyard"]) {
            SearchTo::HandAndGraveyard
        } else if self.eat_word("hand") {
            SearchTo::Hand
        } else if self.eat_word("battlefield") {
            let tapped = self.eat_word("tapped");
            if tapped && self.eat_words(&["and", "hand"]) {
                SearchTo::BattlefieldAndHand
            } else {
                SearchTo::Battlefield { tapped }
            }
        } else if self.eat_word("top") {
            SearchTo::TopOfLibrary
        } else if self.eat_word("graveyard") {
            SearchTo::Graveyard
        } else if self.eat_words(&["exile", "face", "down"]) {
            SearchTo::ExileFaceDown
        } else if self.eat_word("exile") {
            SearchTo::Exile
        } else {
            return self.unexpected("`hand`, `battlefield`, `graveyard`, or `top`");
        })
    }

    /// After `if <condition>,`: `instead <sentence>` or `<sentence>
    /// instead` (CR 608.2c), or else the plain sentence (`Err`).
    fn instead_body(&mut self, cond: InsteadCond) -> R<Result<Effect, Sentence>> {
        let lead = self.eat_word("instead");
        let saved = self.subject.take();
        let s = self.sentence()?;
        self.subject = saved;
        // `search up to 3 ... instead of 2`: "search your library for up
        // to three basic Forest cards instead of two" is the trailing form.
        let instead_of = matches!(s.effects.last(), Some(Effect::Search { more: Some(m), .. })
            if matches!(m.wording, crate::ast::SearchWording::InsteadOf(_)
                | crate::ast::SearchWording::PutInstead { .. }));
        let wording = if lead {
            InsteadWording::Lead
        } else if instead_of || self.eat_word("instead") {
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
                let before = self.targets.len();
                match self.instead_body(InsteadCond::Static(cond.clone()))? {
                    // "If this spell was cast using teamwork, instead exile
                    // target creature": the spell cast using teamwork has
                    // these targets in place of the others, as a kicked
                    // spell's "instead" (`Card::kicked_drops`).
                    Ok(e) => {
                        if cond == StaticCond::Teamwork
                            && matches!(e, Effect::Instead { .. })
                            && self.targets.len() > before
                            && before > 0
                            && self.kicked_instead.is_none()
                        {
                            self.kicked_instead = Some(before);
                        }
                        return Ok(Some(e));
                    }
                    Err(s) => {
                        // "If this spell was bargained, target land you
                        // control becomes ...": targets only a bargained
                        // spell has, as for "if kicked" (CR 601.2c).
                        if cond == StaticCond::Bargained
                            && self.targets.len() > before
                            && self.kicked_targets.is_none()
                        {
                            self.kicked_targets = Some(before);
                        }
                        vec![s]
                    }
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
        // `noalso each Warrior creature you control gains lifelink`.
        if self.eat_word("noalso") {
            let mut e = self.clause()?;
            let Effect::Grant { no_also, .. } = &mut e else {
                return self.fail("`noalso` takes `<who> gains <keywords>`");
            };
            *no_also = true;
            return Ok(e);
        }
        // "You may cast ... without paying its mana cost" (CR 608.2g).
        if self.eat_words(&["cast", "free"]) {
            return self.cast_free();
        }
        // "You may play the exiled card without paying its mana cost"
        // (hideaway, CR 702.75a).
        // "put the exiled card into its owner's hand" (CR 607.2a).
        // "Put all cards exiled with this creature into their owners'
        // hands." (`put all card exiled with this into hand`).
        if self.at_words(&["put", "exiled", "card", "into", "hand"])
            || self.at_words(&["put", "all", "card", "exiled", "with"])
        {
            self.pos += 1;
            let what = vec![self.recipient()?];
            self.expect_words(&["into", "hand"])?;
            // "Put the exiled card into your hand if it wasn't cast this
            // way." (Beseech the Mirror).
            let not_cast = what == [Recipient::ExiledCard] && self.at_words(&["if", "not", "cast"]);
            if not_cast {
                self.pos += 3;
            }
            if not_cast || what == [Recipient::ExiledCard] && self.eat_words(&["unless", "cast"]) {
                return Ok(Effect::PutCard {
                    what: if not_cast {
                        crate::ast::CardWords::ExiledIfNotCast
                    } else {
                        crate::ast::CardWords::ExiledUnlessCast
                    },
                    to: mtg_core::card::CardDest::Hand,
                    who: None,
                    that_player: false,
                    random: false,
                    rest: None,
                });
            }
            return Ok(Effect::ReturnToHand {
                what,
                owner_hand: false,
                put: true,
            });
        }
        if self.eat_words(&["play", "free"]) {
            let mut e = self.cast_free()?;
            if let Effect::CastFree { play, .. } = &mut e {
                *play = true;
            }
            return Ok(e);
        }
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
        // "Add {R}. Until end of turn, you don't lose this mana as steps
        // and phases end."
        if self.at_words(&["keep", "mana"]) {
            self.pos += 2;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::KeepMana(vec![s]));
        }
        // "Any player may pay 5 life", "any opponent may sacrifice a
        // creature of their choice".
        if self.at_words(&["any", "player", "may"]) || self.at_words(&["any", "opponent", "may"]) {
            let opponents = self.word_at(1).as_deref() == Some("opponent");
            self.pos += 3;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::AnyPlayerMay {
                opponents,
                inner: vec![s],
            });
        }
        // "if a player does, ..." / "if no one does, ..." after it.
        if self.at_words(&["if", "a", "player", "does"])
            || self.at_words(&["if", "no", "one", "does"])
        {
            let done = self.word_at(1).as_deref() == Some("a");
            self.pos += 4;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(if done {
                Effect::IfYouDo(vec![s])
            } else {
                Effect::Otherwise(vec![s])
            });
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
        // "choose one or more { ... }", "choose up to 1 { ... }", "choose
        // one unchosen { ... }".
        if let Some((head, n)) = self.peek_mode_head() {
            self.pos += n;
            return Ok(Effect::ModesOf(head, self.modes_block()?));
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
        // "When you discard a card this way, ...": the same after "discard
        // a card".
        if self.at_words(&["when", "you", "discard", "a", "card", "this", "way"]) {
            self.pos += 4;
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
                words: crate::ast::DontWords::Plain,
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
                words: crate::ast::DontWords::Plain,
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
        if self.at_words(&["if", "target"])
            || self.at_words(&["if", "that", "target"])
            || self.at_words(&["if", "the", "full", "target"])
        {
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
                words: crate::ast::DontWords::Plain,
            });
        }
        // `if you dont draw this way, ...`, `if you dont put into hand this
        // way, ...`, `if you didnt put into hand this way, ...`: wording.
        {
            use crate::ast::DontWords;
            let words = if self.at_words(&["if", "you", "dont", "draw", "this", "way"]) {
                self.pos += 6;
                Some(DontWords::Draw)
            } else if self.at_words(&["if", "you", "dont", "put", "into", "hand", "this", "way"]) {
                self.pos += 8;
                Some(DontWords::PutHand)
            } else if self.at_words(&["if", "you", "didnt", "put", "into", "hand", "this", "way"]) {
                self.pos += 8;
                Some(DontWords::DidntPutHand)
            } else {
                None
            };
            if let Some(words) = words {
                self.expect(&Tok::Comma)?;
                let saved = self.subject.take();
                let s = self.sentence()?;
                self.subject = saved;
                return Ok(Effect::IfYouDont {
                    cant: false,
                    then: vec![s],
                    words,
                });
            }
        }
        // `if you dont put it into hand, ...` after `if it is ...`.
        if self.revealed && self.at_words(&["if", "you", "dont", "put", "it"]) {
            use mtg_core::card::CardDest;
            self.pos += 5;
            let it = self.eat_word("itself");
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
            return Ok(Effect::IfYouDontPut {
                to,
                then: vec![s],
                it,
            });
        }
        if self.revealed && self.at_words(&["if", "revealed"]) {
            return self.if_revealed_card();
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
        // `if you controlled that target 1, ...`: "If you controlled that
        // permanent" whatever the target's noun (Kellan, Inquisitive
        // Prodigy).
        // `if you controlled it, ...`: "If you controlled it, ..." about the
        // last target (Hotshot Investigators).
        if self.at_words(&["if", "you", "controlled", "it"])
            && self.peek_at(4) == Some(&Tok::Comma)
            && let Some(n) = self.last_object_target()
        {
            self.pos += 5;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfTarget(
                n,
                Filter::controlled_by_you(),
                vec![s],
                crate::ast::TargetCheck {
                    tense: crate::ast::TargetTense::Is,
                    ..crate::ast::TargetCheck::default()
                },
            ));
        }
        // `if an opponent protects target 1, ...`, `if you protect target
        // 1, ...`: "If an opponent protects it" (CR 310.8, Portent
        // Tracker).
        let opp = self.at_words(&["if", "an", "opponent", "protects", "target"]);
        if opp || self.at_words(&["if", "you", "protect", "target"]) {
            self.pos += if opp { 5 } else { 4 };
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
                Filter::protected_by(if opp { Who::Opponents } else { Who::You }),
                vec![s],
                crate::ast::TargetCheck {
                    tense: crate::ast::TargetTense::Is,
                    ..crate::ast::TargetCheck::default()
                },
            ));
        }
        let permanent = self.at_words(&["if", "you", "controlled", "that", "target"]);
        if permanent || self.at_words(&["if", "you", "controlled", "target"]) {
            self.pos += if permanent { 5 } else { 4 };
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
                crate::ast::TargetCheck {
                    tense: if permanent {
                        crate::ast::TargetTense::ThatWas
                    } else {
                        crate::ast::TargetCheck::default().tense
                    },
                    ..crate::ast::TargetCheck::default()
                },
            ));
        }
        // "If excess damage was dealt this way, ..." (CR 120.4a).
        // "... to that creature this way" (Torch the Witness): the noun is
        // the target's, as printed.
        let to_that = self.at_words(&["if", "excess", "damage", "dealt", "to", "that"])
            && matches!(self.word_at(6).as_deref(), Some("permanent" | "creature"))
            && self.word_at(7).as_deref() == Some("this")
            && self.word_at(8).as_deref() == Some("way");
        if to_that || self.at_words(&["if", "excess", "damage", "dealt", "this", "way"]) {
            self.pos += if to_that { 9 } else { 6 };
            self.expect(&Tok::Comma)?;
            let saved = self.subject.take();
            let s = self.sentence()?;
            self.subject = saved;
            return Ok(Effect::IfExcessDamage(vec![s], to_that));
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
        // "If they can't, you draw a card." (Brain Pry).
        if self.at_words(&["if", "they", "can't"]) {
            self.pos += 3;
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            }
            let inner = vec![self.sentence()?];
            return Ok(Effect::IfTheyCant(inner));
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
            // An "instead" after a sentence with no targets ("Wildfire
            // Howl deals 2 damage to each creature. If the gift was
            // promised, instead ... 1 damage to any target ...") has
            // nothing to replace: its targets are the kicked spell's only.
            if self.targets.len() > before
                && self.kicked_targets.is_none()
                && (!matches!(e, Effect::Instead { .. }) || before == 0)
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
            let mut r = self.replacement_inline()?;
            r.wording = self.then_wording();
            r.then = self.created_then()?;
            return Ok(Effect::CreateReplacement(r));
        }
        // "That player sacrifices one of them of their choice.": one of
        // the last two targets picked (`Effect::PickTarget`).
        {
            use mtg_core::card::{PickAct, PickWording};
            const FORMS: [(&[&str], bool, PickAct, PickWording); 6] = [
                (
                    &[
                        "that",
                        "player",
                        "sacrifices",
                        "one",
                        "of",
                        "them",
                        "of",
                        "their",
                        "choice",
                    ],
                    true,
                    PickAct::Sacrifice,
                    PickWording::OfTheirChoice,
                ),
                (
                    &[
                        "their",
                        "controller",
                        "chooses",
                        "and",
                        "sacrifices",
                        "one",
                        "of",
                        "them",
                    ],
                    true,
                    PickAct::Sacrifice,
                    PickWording::TheirController,
                ),
                (
                    &[
                        "that",
                        "player",
                        "chooses",
                        "and",
                        "sacrifices",
                        "one",
                        "of",
                        "those",
                        "targets",
                    ],
                    true,
                    PickAct::Sacrifice,
                    PickWording::ChoosesAnd,
                ),
                (
                    &["that", "player", "chooses", "one", "of", "those", "targets"],
                    true,
                    PickAct::Choose,
                    PickWording::Those,
                ),
                (
                    &["exile", "one", "of", "those", "targets"],
                    false,
                    PickAct::Exile,
                    PickWording::Those,
                ),
                (
                    &["choose", "one", "of", "those", "targets"],
                    false,
                    PickAct::Choose,
                    PickWording::Those,
                ),
            ];
            for (words, by_controller, act, wording) in FORMS {
                if self.at_words(words) {
                    if self.targets.len() < 2 {
                        return self.fail("`one of those targets` needs two targets before it");
                    }
                    self.pos += words.len();
                    return Ok(Effect::PickTarget {
                        first: self.targets.len() - 2,
                        by_controller,
                        act,
                        wording,
                    });
                }
            }
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
            self.subject_each = false;
            if group.is_some() || self.is_many_subject(&subjects) {
                self.subject_each = self.eat_word("each");
            }
            // "You and target opponent each draw a card", "This creature
            // and up to one other target creature each get +3/+3".
            let pair = match subjects.as_slice() {
                [Recipient::You, Recipient::Target(a)] => Some(Recipient::YouAndTarget(*a)),
                [Recipient::Source, Recipient::Target(a)] => Some(Recipient::SourceAndTarget(*a)),
                _ => None,
            };
            if pair.is_some() {
                self.subject_each = self.eat_word("each");
            }
            let subject = if let Some(g) = group.or(pair) {
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
    fn subject_verb(&mut self, subject: Recipient, explicit: bool) -> R<Effect> {
        let counts = self.counts.len();
        let e = self.subject_verb_inner(subject, explicit)?;
        // "Each player loses 1 life for each creature they control":
        // `they control` compiles to "an opponent controls", not the
        // player losing life, so it would count the wrong creatures.
        // "Each player loses 1 life for each creature they control"
        // compiles to each player's own count (Stronghold Discipline).
        let own_count = subject == Recipient::EachPlayer
            && matches!(
                e,
                Effect::LoseLife {
                    amount: Num::Count(_),
                    ..
                }
            );
        // "Each player sacrifices a creature for each creature they
        // control", "any number of target players each draw a card for each
        // creature they control": the player the effect is acting on
        // (`ThatPlayer::Acting`), for the effects that act for each of
        // their players in turn.
        let acting = matches!(subject, Recipient::EachPlayer | Recipient::Targets(..))
            && matches!(
                e,
                Effect::LoseLife { .. }
                    | Effect::DrawCards { .. }
                    | Effect::Mill { .. }
                    | Effect::Discard { .. }
                    | Effect::SacrificeChosen { .. }
            );
        if acting {
            for c in &mut self.counts[counts..] {
                if let crate::ast::CountExpr::Permanents(f) = &mut c.expr
                    && f.they_control
                    && f.that_player.is_none()
                {
                    f.who = None;
                    f.that_player = Some(mtg_core::card::ThatPlayer::Acting);
                }
            }
        }
        if matches!(subject, Recipient::EachPlayer | Recipient::You)
            && !own_count
            && !acting
            && self.counts[counts..]
                .iter()
                .any(|c| matches!(&c.expr, crate::ast::CountExpr::Permanents(f) if f.they_control))
        {
            return self.fail(
                "`they control` in a count for each player or you: it counts an opponent's permanents, not that player's",
            );
        }
        // "Target player loses 1 life for each tapped artifact they
        // control." (Burden of Greed): that player's permanents, not an
        // opponent's.
        if let Recipient::Target(n) = subject
            && self.targets.get(n).is_some_and(|t| {
                matches!(
                    t.base(),
                    Target::Spec(TargetSpec::Player | TargetSpec::Opponent)
                )
            })
        {
            for c in &mut self.counts[counts..] {
                if let crate::ast::CountExpr::Permanents(f) = &mut c.expr
                    && f.they_control
                    && f.that_player.is_none()
                {
                    f.who = None;
                    f.that_player = Some(mtg_core::card::ThatPlayer::Target(
                        u8::try_from(n).unwrap_or(u8::MAX),
                    ));
                }
            }
        }
        Ok(e)
    }

    #[allow(clippy::too_many_lines)]
    fn subject_verb_inner(&mut self, subject: Recipient, explicit: bool) -> R<Effect> {
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
                        | Recipient::Granter
                        | Recipient::GranterIt
                ) =>
            {
                self.pos -= 1;
                match self.verb_first()? {
                    Effect::DealDamage {
                        amount,
                        to,
                        that_amount,
                        ..
                    } => Ok(Effect::DealDamage {
                        amount,
                        to,
                        by: subject,
                        that_amount,
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
            "gain" | "gains" if self.at_word("flashback") || self.at_word("harmonize") => {
                let harmonize = self.at_word("harmonize");
                self.pos += 1;
                let that_card = self.eat_words(&["that", "card"]);
                self.until_end_of_turn();
                Ok(Effect::GrantFlashback {
                    to: subject,
                    that_card,
                    harmonize,
                })
            }
            "gain" | "gains" => {
                if self.at_word("control") {
                    self.pos += 1;
                    self.expect_word("of")?;
                    let of = self.recipient()?;
                    // "Target opponent gains control of target permanent
                    // you control.": another player, indefinitely.
                    if subject != Recipient::You {
                        return Ok(Effect::GiveControl { of, to: subject });
                    }
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
                        each: explicit && self.subject_each,
                        no_also: false,
                        each_combat: false,
                    });
                }
                let amount = self.num()?;
                self.expect_word("life")?;
                Ok(Effect::GainLife {
                    amount,
                    who: subject,
                })
            }
            // `target 1 is Spirit in addition`: "It's a Spirit in addition
            // to its other types" (indefinitely).
            "is" if matches!(self.peek(), Some(Tok::Word(w))
                if w.chars().next().is_some_and(char::is_uppercase)) =>
            {
                let mut subtypes = Vec::new();
                while let Some(Tok::Word(w)) = self.peek() {
                    if !w.chars().next().is_some_and(char::is_uppercase) {
                        break;
                    }
                    subtypes.push(w.clone());
                    self.pos += 1;
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
                self.expect_words(&["in", "addition"])?;
                let loses_abilities = self.eat_words(&["and", "has", "no", "abilities"]);
                Ok(Effect::Animate {
                    to: subject,
                    types: Vec::new(),
                    subtypes,
                    pt: None,
                    base: false,
                    colors: Vec::new(),
                    keywords,
                    indefinite: true,
                    loses_abilities,
                    in_addition: true,
                    is_word: true,
                })
            }
            // "It's an enchantment." (indefinitely, layer 4).
            "is" if self.at_word("now") => {
                self.pos += 1;
                let mut types = Vec::new();
                while let Some(Tok::Word(w)) = self.peek() {
                    let Some(t) = card_type(w) else { break };
                    types.push(t);
                    self.pos += 1;
                }
                if types.is_empty() {
                    return self.fail("expected a card type after `is now`");
                }
                Ok(Effect::IsNow { to: subject, types })
            }
            // "loses all abilities" alone (layer 6).
            "lose" | "loses"
                if self.at_words(&["all", "abilities"])
                    && !self.at_words(&["all", "abilities", "and"]) =>
            {
                self.pos += 2;
                let indefinite = self.eat_word("indefinitely");
                if !indefinite {
                    self.until_end_of_turn();
                }
                Ok(Effect::LoseAllAbilities {
                    to: subject,
                    indefinite,
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
                    || self.at_word("attached")
                    || self.at_word("granter") =>
            {
                let what = self.recipient()?;
                Ok(Effect::SacrificeBy { who: subject, what })
            }
            // "sacrifices half the creatures they control of their
            // choice, rounded up".
            "sacrifices" | "sacrifice" if self.half_ahead() => {
                let at = self.pos;
                let filter = {
                    self.pos += 2;
                    self.filter()?
                };
                if filter.who.is_some() || filter.that_player.is_some() {
                    return self.fail(
                        "`sacrifices half up <filter>`: the filter is of the permanents they control; leave out who controls them",
                    );
                }
                let mut theirs = filter.clone();
                if subject == Recipient::You {
                    // "sacrifice half the non-Demon permanents you
                    // control, rounded up" (Rakdos the Defiler).
                    theirs.who = Some(Who::You);
                } else {
                    theirs.they_control = true;
                    theirs.that_player = Some(mtg_core::card::ThatPlayer::Acting);
                }
                self.pos = at;
                let count = self.half_count(crate::ast::CountExpr::Permanents(theirs))?;
                self.filter()?;
                Ok(Effect::SacrificeChosen {
                    who: subject,
                    count,
                    filter,
                    and: None,
                    choice_first: false,
                })
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
                // "a creature of their choice other than enchanted
                // creature" (Sporogenic Infection).
                if of_choice
                    && !choice_first
                    && !filter.not_attached
                    && self.eat_words(&["other", "than", "enchanted"])
                {
                    filter.not_attached = true;
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
                Ok(Effect::Shuffle {
                    who: subject,
                    library: false,
                    have: false,
                })
            }
            // "Target player shuffles their library."
            "shuffle" | "shuffles"
                if self.at_word("library") && !matches!(self.peek_at(1), Some(Tok::Word(_))) =>
            {
                self.pos += 1;
                Ok(Effect::Shuffle {
                    who: subject,
                    library: true,
                    have: false,
                })
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
            // "Target player reveals three cards from their hand and you
            // choose one of them. That player discards that card."
            "reveal" | "reveals"
                if ((matches!(self.peek(), Some(Tok::Num(_)))
                    || self.word_at(0).is_some_and(|w| w.eq_ignore_ascii_case("x")))
                    && matches!(self.word_at(1).as_deref(), Some("card" | "cards"))
                    && self.word_at(2).as_deref() == Some("discarding"))
                    || self.at_words(&["count", "where"]) =>
            {
                let n = self.num()?;
                // `reveals count where Faerie you control, cards
                // discarding card`: the comma ends the count.
                if matches!(n, Num::Count(_)) {
                    if self.peek() != Some(&Tok::Comma) {
                        return self.unexpected("`,` after the count");
                    }
                    self.pos += 1;
                }
                if !(self.eat_word("cards") || self.eat_word("card")) {
                    return self.unexpected("`cards`");
                }
                self.expect_word("discarding")?;
                let take = match self.peek() {
                    Some(Tok::Num(k)) if self.word_at(1).is_some() => {
                        let k = u8::try_from(*k).map_err(|_| ParseError {
                            line: self.line(),
                            message: "too many cards".to_string(),
                        })?;
                        self.pos += 1;
                        k
                    }
                    _ => 1,
                };
                let mut e = self.discard_chosen(subject, false)?;
                if let Effect::DiscardChosen { reveal, .. } = &mut e {
                    *reveal = Some((n, take));
                }
                Ok(e)
            }
            "reveal" | "reveals" => {
                self.eat_word("their");
                self.expect_word("hand")?;
                if self.eat_word("discarding") {
                    return self.discard_chosen(subject, false);
                }
                // "reveals their hand and you choose a card of that color
                // from it. That player discards that card." (Addle).
                if self.eat_words(&["and", "discarding"]) {
                    return self.discard_chosen_with(subject, false, true, false);
                }
                // "reveals their hand and you choose a nonland card from
                // it. Exile that card until ..." (Brain Maggot).
                let and_choose = self.eat_words(&["and", "exiling"]);
                // "You choose a nonland card from that player's graveyard
                // or hand and exile it."
                if and_choose || self.eat_word("exiling") {
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
                        let until = self.eat_words(&["until", "this", "leaves"]);
                        return Ok(Effect::DiscardChosen {
                            reveal: None,
                            who: subject,
                            filter: (filter != Filter::default()).then_some(filter),
                            may: false,
                            look: false,
                            exile: Some(joined),
                            until,
                            and_choose,
                            top: false,
                        });
                    }
                    if and_choose {
                        return self.unexpected("`split` or `until this leaves`");
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
                        is_word: false,
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
                        is_word: false,
                    }),
                    _ => self.fail("`loses all abilities and becomes` takes what it becomes"),
                }
            }
            "lose" | "loses" if self.eat_word("game") => Ok(Effect::LoseGame { who: subject }),
            // "loses half their life, rounded down".
            "lose" | "loses" if self.half_ahead() => {
                let amount = self.half_count(crate::ast::CountExpr::Their(
                    mtg_core::card::PlayerCount::Life,
                ))?;
                self.expect_word("life")?;
                Ok(Effect::LoseLife {
                    amount,
                    who: subject,
                })
            }
            "lose" | "loses" if self.at_words(&["half", "life"]) => {
                self.pos += 2;
                Ok(Effect::LoseHalfLife { who: subject })
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
            // "mills half their library, rounded down".
            "mill" | "mills" if self.half_ahead() => {
                let count = self.half_count(crate::ast::CountExpr::Their(
                    mtg_core::card::PlayerCount::Library,
                ))?;
                self.expect_word("library")?;
                Ok(Effect::Mill {
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
            // `target player shuffles up to 3 target card from their
            // graveyard into library`.
            "shuffles" if matches!(subject, Recipient::Target(_)) => {
                let what = self.recipient()?;
                self.expect_words(&["into", "library"])?;
                Ok(Effect::PutInLibrary {
                    what,
                    place: crate::ast::LibraryPlace::Shuffle,
                    wording: LibraryWording::By(subject),
                })
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
            // "discards half the cards in their hand, rounded up".
            "discard" | "discards" if self.half_ahead() => {
                let count = self.half_count(crate::ast::CountExpr::Their(
                    mtg_core::card::PlayerCount::Hand,
                ))?;
                self.expect_word("hand")?;
                Ok(Effect::Discard {
                    count,
                    who: (explicit || subject != Recipient::You).then_some(subject),
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
                // `discards 1 with that name card`: "discards a card with
                // that name".
                if count == Num::N(1)
                    && !matches!(self.word_at(0).as_deref(), Some("card" | "cards"))
                {
                    let filter = self.filter()?;
                    self.cards_word()?;
                    return Ok(Effect::DiscardAll {
                        who: Some(subject),
                        filter: Some(filter),
                        all_cards: false,
                        one: true,
                    });
                }
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
            // "If they do, you incubate 2.": an explicit "you".
            "incubate" if subject == Recipient::You => Ok(Effect::Incubate {
                count: self.num()?,
                you: true,
            }),
            // "... and you scry 2": an explicit "you".
            "scries" | "scry" if w == "scries" || subject == Recipient::You => {
                let count = self.num()?;
                Ok(Effect::ScryEach {
                    who: subject,
                    count,
                    may: false,
                })
            }
            // "Each opponent may discard a card or sacrifice a permanent of
            // their choice."
            "may"
                if subject != Recipient::You
                    && ((self.at_word("discard")
                        && matches!(self.peek_at(1), Some(Tok::Num(1)))
                        && self.word_at(3).as_deref() == Some("or")
                        && self.word_at(4).as_deref() == Some("sacrifice"))
                        || (self.at_word("sacrifice")
                            && (2..8).any(|k| {
                                self.word_at(k).as_deref() == Some("or")
                                    && self.word_at(k + 1).as_deref() == Some("discard")
                            }))) =>
            {
                let sacrifice_first = self.eat_word("sacrifice");
                if !sacrifice_first {
                    self.expect_word("discard")?;
                    self.number()?;
                    self.cards_word()?;
                    self.expect_words(&["or", "sacrifice"])?;
                }
                let sacrifice = self.filter()?;
                if sacrifice_first {
                    self.expect_words(&["or", "discard"])?;
                    if self.number()? != 1 {
                        return self.fail("`may sacrifice ... or discard 1 card`: one card");
                    }
                    self.cards_word()?;
                }
                Ok(Effect::MayOr {
                    who: subject,
                    sacrifice,
                    sacrifice_first,
                })
            }
            // "That player may pay {4}", "that player may discard a card
            // at random": the player decides and acts (`PlayerMay`).
            // "If the player does, they may copy this spell" (Chain
            // Lightning): the copy is that player's.
            "may"
                if subject != Recipient::You
                    && (self.at_word("pay")
                        || self.at_word("discard")
                        || self.at_words(&["copy", "this"])) =>
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
                        graveyard,
                        name,
                        old,
                        those,
                        more,
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
                        graveyard,
                        name,
                        old,
                        those,
                        more,
                    }),
                    _ => self.fail("expected a search"),
                }
            }
            // "Target opponent exiles two cards from their hand", "exiles
            // a creature they control".
            // "Target opponent chooses a creature they control."
            "chooses" if !self.at_word("and") && !self.at_word("one") => {
                let mut filter = self.filter()?;
                if filter.they_control {
                    filter.they_control = false;
                    filter.who = None;
                } else {
                    self.expect_words(&["they", "control"])?;
                }
                Ok(Effect::PlayerChooses {
                    who: subject,
                    filter,
                })
            }
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
                let face = self.exile_face();
                Ok(Effect::ExileTop {
                    count,
                    who: Some(subject),
                    face,
                })
            }
            // `each player exiles library but bottom 6 face down`.
            "exiles" if self.at_word("library") => {
                self.pos += 1;
                let keep = if self.eat_words(&["but", "bottom"]) {
                    self.number()?
                } else {
                    0
                };
                let face = self.exile_face();
                Ok(Effect::ExileLibrary {
                    who: Some(subject),
                    keep,
                    face,
                })
            }
            // `target player exiles hand face down`.
            "exiles" if self.at_word("hand") || self.at_words(&["graveyard", "face"]) => {
                let graveyard = self.at_word("graveyard");
                self.pos += 1;
                let face = self.exile_face();
                Ok(Effect::ExileFromHand {
                    count: Num::N(1),
                    who: subject,
                    graveyard,
                    face,
                    all: true,
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
                    let face = self.exile_face();
                    return Ok(Effect::ExileFromHand {
                        count,
                        who: subject,
                        graveyard,
                        face,
                        all: false,
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
            // "Then those creatures fight each other."
            "fight"
                if matches!(
                    subject,
                    Recipient::ThoseTargets(..) | Recipient::ChosenTargets(..)
                ) && self.eat_words(&["each", "other"]) =>
            {
                Ok(Effect::Fight {
                    a: subject,
                    b: subject,
                })
            }
            "bites" => {
                let twice = self.eat_word("twice");
                let to = if !twice && self.eat_word("itself") {
                    subject
                } else {
                    self.recipient()?
                };
                // "Enchanted creature deals damage equal to its power to
                // any other target" (Pain for All): `another target any`
                // excludes the source, the Aura, not the creature dealing
                // the damage, so it would mean something else.
                if subject == Recipient::Attached
                    && let Recipient::Target(i) = to
                    && self.targets.get(i) == Some(&Target::Spec(TargetSpec::AnyOther))
                {
                    return self.fail(
                        "`enchanted creature bites another target any`: \"other\" would exclude the Aura, not the creature",
                    );
                }
                Ok(Effect::Bite {
                    source: subject,
                    to,
                    twice,
                })
            }
            // "this creature's base power becomes equal to ..." (CR
            // 613.4b).
            "base" if self.at_words(&["power", "becomes"]) => {
                self.pos += 2;
                let n = self.num()?;
                self.until_end_of_turn();
                Ok(Effect::SetBasePower {
                    to: subject,
                    n,
                    becomes: true,
                })
            }
            "has" | "have"
                if self.at_words(&["base", "power"])
                    && !self.at_words(&["base", "power", "and"]) =>
            {
                self.pos += 2;
                let n = self.num()?;
                self.until_end_of_turn();
                Ok(Effect::SetBasePower {
                    to: subject,
                    n,
                    becomes: false,
                })
            }
            "has" | "have" => {
                self.expect_words(&["base", "power", "and", "toughness"])?;
                if !matches!(self.peek(), Some(Tok::Pt { .. })) {
                    let n = self.num()?;
                    self.until_end_of_turn();
                    return Ok(Effect::SetBasePtVar { to: subject, n });
                }
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
                    each: false,
                    no_also: false,
                    each_combat: false,
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
                // "Anzrag must be blocked each combat this turn if able".
                let each_combat = keyword == Keyword::MustBeBlocked
                    && self.eat_words(&["each", "combat", "this", "turn"]);
                let _ = self.eat_words(&["this", "turn"]) || self.eat_words(&["each", "combat"]);
                if matches!(keyword, Keyword::MustBlock | Keyword::MustBeBlocked) {
                    self.expect_words(&["if", "able"])?;
                }
                self.until_end_of_turn();
                Ok(Effect::Grant {
                    keywords: vec![keyword],
                    to: subject,
                    each: false,
                    no_also: false,
                    each_combat,
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
            // "The "legend rule" doesn't apply to permanents you control
            // this turn." (CR 704.5j; Hall of Echoes).
            "ignore" if self.at_words(&["legend", "rule", "this", "turn"]) => {
                self.pos += 4;
                Ok(Effect::PlayerRule {
                    who: subject,
                    kind: mtg_core::card::PlayerRuleKind::NoLegendRule,
                })
            }
            "can't"
                if self.at_words(&["play", "lands"])
                    || self.at_words(&["cast", "creature", "spells"])
                    || self.at_words(&["gain", "life", "this", "turn"]) =>
            {
                let kind = if self.eat_words(&["play", "lands"]) {
                    mtg_core::card::PlayerRuleKind::CantPlayLands
                } else if self.eat_words(&["gain", "life"]) {
                    mtg_core::card::PlayerRuleKind::CantGainLife
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
            // "can't be the target of spells or abilities your opponents
            // control this turn" (Vines of Vastwood).
            "can't" if self.at_words(&["be", "targeted", "by"]) => {
                self.pos += 3;
                let ban = if self.eat_word("opponents") {
                    mtg_core::card::TargetBan::Opponents
                } else {
                    self.expect_word("spells")?;
                    mtg_core::card::TargetBan::Spells
                };
                self.expect_words(&["this", "turn"])?;
                Ok(Effect::CantBeTargeted { what: subject, ban })
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
                    each: explicit && self.subject_each,
                    no_also: false,
                    each_combat: false,
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
            "explores" => {
                let twice = self.eat_word("twice");
                // `explores X times`.
                let times = if !twice
                    && self
                        .word_at(0)
                        .is_some_and(|w| w.eq_ignore_ascii_case("x") || w == "count")
                {
                    let n = self.num()?;
                    self.expect_word("times")?;
                    Some(n)
                } else {
                    None
                };
                Ok(Effect::Explore {
                    what: subject,
                    twice,
                    times,
                })
            }
            "phases" => {
                self.expect_word("out")?;
                Ok(Effect::PhaseOut { what: subject })
            }
            "connives" => Ok(Effect::Connive { what: subject }),
            "becomes" if self.at_word("prepared") => {
                self.pos += 1;
                Ok(Effect::BecomePrepared { what: subject })
            }
            "becomes" if self.at_word("saddled") => {
                self.pos += 1;
                self.until_end_of_turn();
                Ok(Effect::BecomeSaddledTarget { what: subject })
            }
            "skips" | "skip" if self.at_word("next") => {
                use mtg_core::card::SkipWhat;
                self.pos += 1;
                let what = if self.eat_words(&["draw", "step"]) {
                    SkipWhat::DrawStep
                } else if self.eat_words(&["creature", "untap"]) {
                    SkipWhat::CreatureUntap
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
            // "Target unblocked attacking creature becomes blocked."
            "become" | "becomes" if self.at_word("blocked") => {
                self.pos += 1;
                Ok(Effect::BecomeBlocked { what: subject })
            }
            "become" | "becomes" => {
                self.eat_word("a");
                if self.eat_word("copy") {
                    // The subject becomes the copy (`Effect::BecomeCopy`):
                    // the source, a target, or a group.
                    self.expect_word("of")?;
                    // `chosen other creature you control`: not targeted,
                    // chosen as it resolves (CR 608.2d).
                    let of = if self.eat_word("chosen") {
                        let f = self.filter()?;
                        let i = if let Some(i) = self.groups.iter().position(|g| *g == f) {
                            i
                        } else {
                            self.groups.push(f);
                            self.groups.len() - 1
                        };
                        Recipient::ChosenOf(u16::try_from(i).map_err(|_| ParseError {
                            line: self.line(),
                            message: "too many groups".to_string(),
                        })?)
                    } else {
                        self.recipient()?
                    };
                    let except = self.except_clause();
                    let until = if self.eat_words(&["until", "end", "of", "turn"]) {
                        mtg_core::card::CopyUntil::EndOfTurn
                    } else if self.eat_words(&["until", "your", "next", "turn"]) {
                        mtg_core::card::CopyUntil::YourNextTurn
                    } else {
                        mtg_core::card::CopyUntil::Permanent
                    };
                    return Ok(Effect::BecomeCopy {
                        of,
                        except,
                        who: subject,
                        until,
                    });
                }
                // "becomes that type": chosen earlier in the resolution.
                if self.at_words(&["that", "type"]) || self.at_words(&["that", "color"]) {
                    let color = self.word_at(1).as_deref() == Some("color");
                    self.pos += 2;
                    self.until_end_of_turn();
                    return Ok(Effect::BecomeChosen { to: subject, color });
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
                // "a 3/3 creature with vigilance and all creature types"
                // (Soulstone Sanctuary): every creature type (CR 205.3m).
                if self.eat_words(&["and", "all", "creature", "types"])
                    || (keywords.is_empty()
                        && self.eat_words(&["with", "all", "creature", "types"]))
                {
                    subtypes.push(crate::ast::EVERY_CREATURE_TYPE.to_string());
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
                    is_word: false,
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
                        attacking,
                        it_has,
                        attached,
                        sized,
                        ..
                    } => Ok(Effect::CreateToken {
                        token,
                        count,
                        tapped,
                        attacking,
                        who: Some(subject),
                        it_has,
                        attached,
                        sized,
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
        const VERBS: [&str; 14] = [
            "choose",
            "tap",
            "untap",
            "transform",
            "sacrifice",
            "discard",
            "exile",
            "return",
            "put",
            "attach",
            "reveal",
            "create",
            "draw",
            "destroy",
        ];
        let Some(verb) = self.word_at(0) else {
            return false;
        };
        if !VERBS[..11].contains(&verb.as_str()) {
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
                // `the chosen target 1` names a target.
                Some("chosen") if self.word_at(k - 1).as_deref() == Some("the") => {}
                // `exile up to 4 with chosen name from zones of ...`.
                Some("chosen") if self.word_at(k + 1).as_deref() == Some("name") => {}
                Some("chosen") => {
                    return verb != "return" || hand || self.word_at(1).as_deref() == Some("up");
                }
                // `return 1 Zombie card from your graveyard to hand at
                // random`.
                Some("random") if verb == "return" && hand => return true,
                Some("hand" | "battlefield") => hand = true,
                // `put 1 card an opponent owns from exile into graveyard
                // chosen`: a zone, not a verb.
                Some("exile") if self.word_at(k - 1).as_deref() == Some("from") => {}
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
        // `exile top 1 creature card from your graveyard chosen`: "exile
        // the top creature card of your graveyard".
        let top = verb == "exile" && self.eat_word("top");
        // `exile this spell and 4 card named this from your graveyard
        // chosen`.
        let with_spell = verb == "exile" && !top && self.eat_words(&["this", "spell", "and"]);
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
            // `choose 1 Giant creature you control chosen; it deals damage
            // ...`: "Choose a Giant creature you control. It ..." (`it` is
            // the chosen permanent).
            "choose" => {
                self.created_at = Some(self.targets.len());
                (ChooseAct::Choose, self.filter()?)
            }
            "tap" => (ChooseAct::Tap, self.filter()?),
            "untap" => (ChooseAct::Untap, self.filter()?),
            "transform" => (ChooseAct::Transform, self.filter()?),
            "sacrifice" => (ChooseAct::Sacrifice, self.filter()?),
            "discard" => (ChooseAct::Discard, card_filter(self)?),
            // `reveal any number green card in hand chosen`: "Reveal any
            // number of green cards in your hand".
            "reveal" => {
                let f = card_filter(self)?;
                self.expect_words(&["in", "hand"])?;
                (ChooseAct::RevealFromHand, f)
            }
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
                let at = self.pos;
                let graveyard =
                    card_filter(self).is_ok() && self.at_words(&["from", "your", "graveyard"]);
                self.pos = at;
                let any_graveyard = !graveyard
                    && !top
                    && !with_spell
                    && card_filter(self).is_ok()
                    && self.at_words(&["from", "a", "graveyard"]);
                self.pos = at;
                let hand = !graveyard
                    && !top
                    && !with_spell
                    && card_filter(self).is_ok()
                    && self.at_words(&["from", "your", "hand"]);
                self.pos = at;
                if hand {
                    let f = card_filter(self)?;
                    self.expect_words(&["from", "your", "hand"])?;
                    (ChooseAct::ExileFromHand, f)
                } else if any_graveyard {
                    let f = card_filter(self)?;
                    self.expect_words(&["from", "a", "graveyard"])?;
                    (ChooseAct::ExileFromAnyGraveyard, f)
                } else if graveyard || top || with_spell {
                    let f = card_filter(self)?;
                    self.expect_words(&["from", "your", "graveyard"])?;
                    if top {
                        (ChooseAct::ExileTopOfGraveyard, f)
                    } else if with_spell {
                        (ChooseAct::ExileWithThisSpell, f)
                    } else {
                        (ChooseAct::ExileFromGraveyard, f)
                    }
                } else {
                    // `exile 1 land you control chosen`.
                    (ChooseAct::Exile, self.filter()?)
                }
            }
            "return" => {
                let f = if self.at_word("card") || self.at_word("cards") {
                    Filter::default()
                } else {
                    self.filter()?
                };
                let mut f = f;
                if self.eat_word("card") || self.eat_word("cards") {
                    // `land card or Elf card`: "a land card or Elf card".
                    if f.or.is_none() && f != Filter::default() && self.at_word("or") {
                        self.pos += 1;
                        let alt = self.filter()?;
                        if !self.eat_word("card") {
                            return self.unexpected("`card`");
                        }
                        f.or = Some(Box::new(alt));
                        f.alt_card = true;
                    }
                    if self.eat_words(&["from", "among", "them", "to", "hand"]) {
                        // "return up to two permanent cards from among them
                        // to your hand" after a mill.
                        among_them = true;
                        returns = true;
                        (ChooseAct::MilledToHand, f)
                    } else if self.eat_words(&["from", "your", "graveyard", "to", "battlefield"]) {
                        // `return 1 land card from your graveyard to
                        // battlefield tapped chosen`.
                        let tapped = self.eat_word("tapped");
                        (ChooseAct::GraveyardToBattlefield { tapped }, f)
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
            // `put 1 card an opponent owns from exile into graveyard
            // chosen` (a processor).
            "put"
                if {
                    let at = self.pos;
                    let ex = card_filter(self).is_ok()
                        && self.at_words(&[
                            "an",
                            "opponent",
                            "owns",
                            "from",
                            "exile",
                            "into",
                            "graveyard",
                        ]);
                    self.pos = at;
                    ex
                } =>
            {
                let mut f = card_filter(self)?;
                self.pos += 7;
                f.who = Some(mtg_core::card::Who::Opponents);
                (ChooseAct::ExileToGraveyard, f)
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
                // `put 1 creature card from among them onto battlefield
                // chosen`.
                if self.eat_words(&["onto", "battlefield"]) {
                    (ChooseAct::MilledToBattlefield, f)
                } else {
                    self.expect_words(&["into", "hand"])?;
                    (ChooseAct::MilledToHand, f)
                }
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
        } else if self.eat_words(&["while", "you", "control", "this"]) {
            mtg_core::card::PlayUntil::WhileYouControlSource
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
                one: false,
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
                one: false,
            }));
        }
        let f = self.filter()?;
        self.cards_word()?;
        Ok(Some(Effect::DiscardAll {
            who,
            filter: Some(f),
            all_cards: false,
            one: false,
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
        self.discard_chosen_with(who, look, false, false)
    }

    /// `discard_chosen` with "reveals their hand and you choose"
    /// (`and_choose`) or "Put that card on top of that player's library"
    /// (`top`).
    fn discard_chosen_with(
        &mut self,
        who: Recipient,
        look: bool,
        and_choose: bool,
        top: bool,
    ) -> R<Effect> {
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
            reveal: None,
            may,
            look,
            exile: None,
            until: false,
            and_choose,
            top,
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
            || (w(0).as_deref() == Some("hand")
                && w(1).as_deref() == Some("and")
                && w(2).as_deref() == Some("graveyard")
                && w(3).as_deref() == Some("into"))
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
        if self.eat_words(&["all", "card", "from"]) {
            filter = Some(Filter::default());
        } else if self.eat_word("all") {
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
        // "their hand and graveyard into their library".
        let both = hand && filter.is_none() && !cards && self.eat_words(&["and", "graveyard"]);
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
            both,
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

    /// `face down` / `face down look` after an exile (CR 406.3).
    fn exile_face(&mut self) -> mtg_core::card::ExileFace {
        if !self.eat_words(&["face", "down"]) {
            mtg_core::card::ExileFace::Up
        } else if self.eat_word("look") {
            mtg_core::card::ExileFace::Look
        } else {
            mtg_core::card::ExileFace::Down
        }
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
        let mut tapped = self.eat_word("tapped");
        let mut attacking = tapped && self.eat_words(&["and", "attacking"]);
        // CR 712.14a: `transformed`, `tapped and transformed`.
        let transformed = if tapped && !attacking {
            self.eat_words(&["and", "transformed"])
        } else {
            !tapped && self.eat_word("transformed")
        };
        let front_up = !transformed && self.eat_words(&["front", "face", "up"]);
        self.eat_words(&["under", "its", "owner's", "control"]);
        let under_you = self.eat_words(&["under", "your", "control"]);
        // "onto the battlefield under your control tapped and attacking".
        if !tapped && self.eat_words(&["tapped", "and", "attacking"]) {
            tapped = true;
            attacking = true;
        }
        let mut additional = false;
        let mut more = None;
        let mut if_creature = false;
        let counters = if self.eat_word("with") {
            let n = self.count()?;
            additional = self.eat_word("additional");
            let kind = self.counter_kind()?;
            // `with 1 hexproof counter and 1 indestructible counter`.
            if self.at_word("and") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
                self.pos += 1;
                let m = self.count()?;
                more = Some((self.counter_kind()?, m));
            }
            self.eat_words(&["on", "it"]);
            if_creature = self.eat_words(&["if", "creature"]);
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
            attacking,
            additional,
            more,
            if_creature,
            transformed,
            front_up,
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

    /// Whether `exile <limit> with chosen name` follows.
    fn chosen_name_ahead(&self) -> bool {
        let skip = match self.word_at(0).as_deref() {
            Some("all") => 1,
            Some("any") => 2,
            Some("up") => 3,
            _ => return false,
        };
        (0..3).all(|i| self.word_at(skip + i).as_deref() == Some(["with", "chosen", "name"][i]))
    }

    /// `exile up to 4 with chosen name from zones of <who>`
    /// (`Effect::ExileChosenName`).
    #[inline(never)]
    fn exile_chosen_name(&mut self) -> R<Effect> {
        use mtg_core::card::NameSearch;
        let limit = if self.eat_word("all") {
            NameSearch::All
        } else if self.eat_words(&["any", "number"]) {
            NameSearch::AnyNumber
        } else {
            self.expect_words(&["up", "to"])?;
            NameSearch::UpTo(u8::try_from(self.number()?).unwrap_or(u8::MAX))
        };
        self.expect_words(&["with", "chosen", "name", "from", "zones", "of"])?;
        let who = self.recipient()?;
        let per_hand = if self.eat_words(&["then", "draws"]) {
            Some(crate::ast::PerHand::Draw)
        } else if self.eat_words(&["then", "creates"]) {
            let Some(Tok::Str(token)) = self.peek().cloned() else {
                return self.unexpected("a token name");
            };
            self.pos += 1;
            Some(crate::ast::PerHand::Create(token))
        } else {
            None
        };
        if per_hand.is_some() {
            self.expect_words(&["for", "each", "from", "hand"])?;
        }
        Ok(Effect::ExileChosenName {
            who,
            limit,
            per_hand,
        })
    }

    /// `exile same name as target 1 from its controller zones`, out of
    /// `verb_first` (whose frame is large in a debug build).
    #[inline(never)]
    fn exile_same_name(&mut self) -> R<Effect> {
        self.pos += 3;
        let what = self.recipient()?;
        self.expect_words(&["from", "its", "controller", "zones"])?;
        Ok(Effect::ExileSameName { what })
    }

    /// After `may` (`owner may`): `play target ...` / `cast that card ...`
    /// with a duration and a `CastRider`.
    fn may_play(&mut self, owner: bool) -> R<Effect> {
        let cast = self.word()? == "cast";
        // `may cast target Zombie creature card from your graveyard this
        // turn`.
        if self.at_word("target") || self.at_words(&["this", "card"]) {
            let what = self.recipient()?;
            let adventure = self.eat_words(&["from", "graveyard", "as", "adventure"]);
            let remains = self.eat_word("remains");
            let until = if remains {
                mtg_core::card::PlayUntil::Forever
            } else {
                self.play_until()?
            };
            let rider = self.cast_rider()?;
            return Ok(Effect::MayPlayTarget {
                what,
                until,
                cast,
                adventure,
                owner,
                remains,
                rider,
            });
        }
        let it = self.eat_word("it");
        if !it {
            self.expect_words(&["that", "card"])?;
        }
        let from_exile = self.eat_words(&["from", "exile"]);
        let remains = self.eat_word("remains");
        let until = if remains {
            mtg_core::card::PlayUntil::Forever
        } else {
            self.play_until()?
        };
        let first = self.eat_word("first");
        let free = self.eat_word("free");
        let rider = self.cast_rider()?;
        Ok(Effect::MayPlayCard {
            until,
            cast,
            free,
            first,
            it,
            from_exile,
            remains,
            owner,
            rider,
        })
    }

    /// A play permission's `Rider`, if one follows: `any type [it]`,
    /// `spend any color`, `spend any type this way`, then `costs {2} more
    /// [opponent]`.
    fn cast_rider(&mut self) -> R<Option<crate::ast::Rider>> {
        use crate::ast::SpendWording;
        use mtg_core::card::SpendAs;
        let mut rider = crate::ast::Rider::default();
        let kind = |p: &mut Self| {
            if p.eat_words(&["any", "type"]) {
                Some(SpendAs::AnyType)
            } else if p.eat_words(&["any", "color"]) {
                Some(SpendAs::AnyColor)
            } else {
                None
            }
        };
        if self.at_words(&["spend", "any"]) {
            self.pos += 1;
            let Some(k) = kind(self) else {
                return self.unexpected("`any type` or `any color`");
            };
            rider.spend = k;
            rider.wording = if self.eat_words(&["this", "way"]) {
                SpendWording::ThisWay
            } else {
                SpendWording::AsThough
            };
        } else if let Some(k) = kind(self) {
            rider.spend = k;
            if self.eat_word("it") {
                rider.wording = SpendWording::CanBeSpentIt;
            }
        }
        if self.eat_word("costs") {
            let n = self.mana_cost()?.mana_value();
            self.expect_word("more")?;
            rider.more = u8::try_from(n).unwrap_or(u8::MAX);
            rider.more_opponents = self.eat_word("opponent");
        }
        rider.exile = self.eat_words(&["exile", "instead"]);
        Ok((rider != crate::ast::Rider::default()).then_some(rider))
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
                ) || self.at_words(&["odd", "or", "even"])
                    || self.at_words(&["card", "name"])
                    || self.at_word("number")
                    || self.at_words(&["nonland", "card", "name"]) =>
            {
                Ok(Effect::Choose(self.choice_kind(false)?))
            }
            // `ban chosen name until your next turn [your opponents|
            // opponents] [and lands] [late]` (`Effect::BanChosenName`).
            "ban" if self.at_words(&["chosen", "name", "until", "your", "next", "turn"]) => {
                self.pos += 6;
                let opponents = if self.eat_words(&["your", "opponents"]) {
                    Some(true)
                } else if self.eat_word("opponents") {
                    Some(false)
                } else {
                    None
                };
                let lands = self.eat_words(&["and", "lands"]);
                let late = self.eat_word("late");
                Ok(Effect::BanChosenName {
                    opponents,
                    lands,
                    late,
                })
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
                    that_amount: false,
                })
            }
            // "Choose target creature, then roll a d20.": the target
            // named before what acts on it.
            // "Choose two target creatures controlled by different
            // players."
            "choose"
                if self.at_word("target")
                    || self.at_words(&["any", "target"])
                    // "Choose up to one target creature."
                    || self.at_words(&["up", "to"])
                    // "Choose any number of target creatures." (Nature's
                    // Panoply).
                    || self.at_words(&["any", "number", "of", "target"])
                    || (matches!(self.peek(), Some(Tok::Num(2)))
                        && self.word_at(1).as_deref() == Some("target")) =>
            {
                if matches!(self.peek(), Some(Tok::Num(_))) {
                    let rs = self.recipient_group()?;
                    return match rs.as_slice() {
                        [Recipient::Target(a), Recipient::Target(b)] if *b == a + 1 => {
                            Ok(Effect::ChooseTarget(Recipient::Targets(*a, 2)))
                        }
                        _ => self.fail("`choose 2 target ...` takes two new targets"),
                    };
                }
                let first = self.recipient()?;
                // "Choose target creature you control and target creature
                // you don't control.": two targets named at once.
                if let Recipient::Target(i) = first
                    && self.at_words(&["and", "target"])
                {
                    self.pos += 1;
                    let second = self.recipient()?;
                    if second != Recipient::Target(i + 1) {
                        return self.fail("expected a second new target after `and`");
                    }
                    return Ok(Effect::ChooseTarget(Recipient::Targets(i, 2)));
                }
                Ok(Effect::ChooseTarget(first))
            }
            // "Roll a d20." (CR 706); its results table follows on the
            // next lines (`result 1 to 9: ...`, see `effects_body`).
            "flip" if self.eat_word("coin") => Ok(Effect::FlipCoin),
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
            // "deals that amount of damage to target creature that player
            // controls" (Fear of Burning Alive): `that much`, worded so.
            "deal" | "deals" if self.eat_words(&["that", "amount", "of", "damage", "to"]) => {
                let to = self.recipients()?;
                Ok(Effect::DealDamage {
                    amount: Num::ThatMuch,
                    to,
                    by: Recipient::Source,
                    that_amount: true,
                })
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
                        that_amount: false,
                    });
                }
                self.expect_word("to")?;
                let to = self.recipients()?;
                Ok(Effect::DealDamage {
                    amount,
                    to,
                    by: Recipient::Source,
                    that_amount: false,
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
            // `put up to x +1/+0 counters on this capped 4`.
            "put"
                if self.at_words(&["up", "to"]) && {
                    let at = self.pos;
                    self.pos += 2;
                    let ok = self.num().is_ok() && self.counter_kind().is_ok();
                    self.pos = at;
                    ok
                } =>
            {
                self.pos += 2;
                let count = self.num()?;
                let kind = self.counter_kind()?;
                self.expect_word("on")?;
                let to = self.recipient()?;
                self.expect_word("capped")?;
                let cap = self.number()?;
                Ok(Effect::AddCountersCapped {
                    kind,
                    count,
                    to,
                    cap,
                })
            }
            "put" if self.revealed && self.at_card_words() => self.put_card(None),
            "reveal" if self.revealed && self.at_word("it") => {
                self.pos += 1;
                Ok(Effect::RevealIt { that: false })
            }
            "reveal" if self.revealed && self.at_words(&["that", "card"]) => {
                self.pos += 2;
                Ok(Effect::RevealIt { that: true })
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
            // "if it had counters on it, put those counters on up to one
            // target creature": the triggering object's (Donatello).
            "put" if self.in_trigger && self.at_words(&["those", "counters", "on"]) => {
                self.pos += 3;
                let to = self.recipient()?;
                Ok(Effect::MoveCounters {
                    from: Recipient::TriggeringObject,
                    to,
                    kind: None,
                    count: None,
                })
            }
            // "put its counters on target creature you control".
            "put" if self.at_words(&["its", "counters", "on"]) => {
                self.pos += 3;
                let to = self.recipient()?;
                // "put its counters on this creature" in a triggered
                // ability: the triggering object's (Buzzard-Wasp Colony).
                let from = if self.its_counters_triggering(to) {
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
            // "put its +1/+1 counters on target creature you control"
            // (Selfless Police Captain): every counter of that kind.
            "put"
                if self.at_word("its")
                    && self.word_at(2).as_deref() == Some("counters")
                    && self.word_at(3).as_deref() == Some("on") =>
            {
                self.pos += 1;
                let kind = self.counter_kind()?;
                self.eat_word("counters");
                self.expect_word("on")?;
                let to = self.recipient()?;
                let from = if self.its_counters_triggering(to) {
                    Recipient::TriggeringObject
                } else {
                    Recipient::Source
                };
                Ok(Effect::MoveCounters {
                    from,
                    to,
                    kind: Some(kind),
                    count: None,
                })
            }
            // "Move all counters from target creature onto another target
            // creature" (CR 122.5).
            "move" => {
                let (kind, count) = if self.eat_words(&["all", "counters"]) {
                    (None, None)
                } else if self.eat_words(&["any", "number"]) {
                    // `move any number +1/+1 counters from ...`.
                    let k = self.counter_kind()?;
                    (Some(k), Some(mtg_core::card::MOVE_ANY_NUMBER))
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
                if (0..10).any(|k| {
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
                let attacking = tapped && self.eat_words(&["and", "attacking"]);
                Ok(Effect::PutFromHand {
                    filter,
                    tapped,
                    also,
                    attacking,
                })
            }
            "put"
                if !matches!(self.peek(), Some(Tok::Num(_)))
                    && !self.at_words(&["that", "much"])
                    && !self.word_at(0).is_some_and(|w| {
                        NUMBER_WORDS.contains(&w.as_str())
                            || w == "x"
                            || (w == "count"
                                && matches!(
                                    self.word_at(1).as_deref(),
                                    Some("where" | "equal" | "each" | "foreach")
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
            // "You may have that player shuffle." (Portent).
            "have"
                if self.at_words(&["that", "player", "shuffle"])
                    || (self.at_words(&["that", "target"])
                        && self.word_at(3).as_deref() == Some("shuffle")) =>
            {
                let who = self.recipient()?;
                self.expect_word("shuffle")?;
                Ok(Effect::Shuffle {
                    who,
                    library: false,
                    have: true,
                })
            }
            "shuffle" if !matches!(self.peek(), Some(Tok::Word(_))) => Ok(Effect::Shuffle {
                who: Recipient::You,
                library: false,
                have: false,
            }),
            "shuffle"
                if self.at_word("library") && !matches!(self.peek_at(1), Some(Tok::Word(_))) =>
            {
                self.pos += 1;
                Ok(Effect::Shuffle {
                    who: Recipient::You,
                    library: true,
                    have: false,
                })
            }
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
            // `owner of this draws 3 cards`: the source's owner as a
            // subject.
            "owner"
                if self.at_words(&["of", "this"])
                    && !matches!(self.word_at(2).as_deref(), Some("puts" | "shuffles")) =>
            {
                self.pos += 2;
                self.subject = Some(Recipient::SourceOwner);
                self.subject_verb(Recipient::SourceOwner, true)
            }
            "owner"
                if self.at_word("may")
                    && matches!(self.word_at(1).as_deref(), Some("play" | "cast")) =>
            {
                self.pos += 1;
                self.may_play(true)
            }
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
                // "Gandalf's owner shuffles him into their library and
                // draws three cards": the owner stays the subject.
                if !of && what == Recipient::Source {
                    self.subject = Some(Recipient::SourceOwner);
                }
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
            // CR 506.4: "remove it from combat".
            "remove" if self.peek_recipient_from_combat() => {
                let what = self.recipient()?;
                self.expect_words(&["from", "combat"])?;
                Ok(Effect::RemoveFromCombat { what })
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
                    return Ok(Effect::RemoveCounterChosen { to, up_to: 0 });
                }
                // `remove up to 3 counters from target creature`.
                if self.at_words(&["up", "to"])
                    && matches!(self.peek_at(2), Some(Tok::Num(_)))
                    && self.word_at(3).as_deref() == Some("counters")
                {
                    self.pos += 2;
                    let n = self.number()?;
                    self.expect_words(&["counters", "from"])?;
                    let to = self.recipient()?;
                    return Ok(Effect::RemoveCounterChosen {
                        to,
                        up_to: u8::try_from(n).unwrap_or(u8::MAX).max(1),
                    });
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
                    // A long filter ("all creature with mana value 2 or
                    // less card from your graveyard") is still one.
                    && (1..24).any(|k| {
                        self.word_at(k).as_deref() == Some("card")
                            && self.word_at(k + 1).as_deref() == Some("from")
                            && self.word_at(k + 2).as_deref() == Some("your")
                            && self.word_at(k + 3).as_deref() == Some("graveyard")
                    }) =>
            {
                self.pos += 1;
                let filter = self.filter()?;
                self.expect_words(&["card", "from", "your", "graveyard", "to", "battlefield"])?;
                let tapped = self.eat_word("tapped");
                Ok(Effect::GraveyardToBattlefield { filter, tapped })
            }
            "return" if self.at_words(&["those", "exiled", "cards", "to", "hand"]) => {
                self.pos += 5;
                Ok(Effect::ReturnThoseExiled)
            }
            "return" => {
                let all = self.recipients()?;
                let what = all[0];
                self.expect_word("to")?;
                // `to owner's hand`: "to their owner's hand".
                let owner_hand = self.eat_words(&["owner's", "hand"]);
                if owner_hand
                    || self.eat_words(&["its", "owner's", "hand"])
                    || self.eat_words(&["their", "owners'", "hands"])
                    || self.eat_words(&["your", "hand"])
                    || self.eat_word("hand")
                {
                    if let [Recipient::Target(i)] = all.as_slice() {
                        self.returned = Some(*i);
                    }
                    return Ok(Effect::ReturnToHand {
                        what: all,
                        owner_hand,
                        put: false,
                    });
                }
                // "Return up to two target creature cards ... to the
                // battlefield": the targets of one phrase, together.
                let what = match all.as_slice() {
                    [Recipient::Target(a), rest @ ..]
                        if !rest.is_empty()
                            && rest
                                .iter()
                                .enumerate()
                                .all(|(k, r)| *r == Recipient::Target(a + k + 1)) =>
                    {
                        Recipient::Targets(*a, all.len())
                    }
                    [_] => what,
                    _ => return self.fail("only one recipient returns to the battlefield"),
                };
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
                // `exile library face down`, `exile hand face down`, `exile
                // 1 card from hand face down` (CR 406.3).
                if self.at_word("library") && self.word_at(1).as_deref() != Some("from") {
                    self.pos += 1;
                    let face = self.exile_face();
                    return Ok(Effect::ExileLibrary {
                        who: None,
                        keep: 0,
                        face,
                    });
                }
                if self.at_words(&["hand", "face", "down"]) {
                    self.pos += 1;
                    let face = self.exile_face();
                    return Ok(Effect::ExileFromHand {
                        count: Num::N(1),
                        who: Recipient::You,
                        graveyard: false,
                        face,
                        all: true,
                    });
                }
                if matches!(self.peek(), Some(Tok::Num(_)))
                    && matches!(self.word_at(1).as_deref(), Some("card" | "cards"))
                    && self.word_at(2).as_deref() == Some("from")
                    && self.word_at(3).as_deref() == Some("hand")
                    && self.word_at(4).as_deref() == Some("face")
                {
                    let count = self.num()?;
                    self.pos += 3;
                    let face = self.exile_face();
                    return Ok(Effect::ExileFromHand {
                        count,
                        who: Recipient::You,
                        graveyard: false,
                        face,
                        all: false,
                    });
                }
                // `exile same name as target 1 from its controller zones`:
                // "Search its controller's graveyard, hand, and library for
                // all cards with the same name as that spell and exile
                // them. Then that player shuffles."
                if self.at_words(&["same", "name", "as"]) {
                    return self.exile_same_name();
                }
                // `exile up to 4 with chosen name from zones of target
                // opponent`, `exile all with chosen name ...`, `exile any
                // number with chosen name ...`.
                if self.chosen_name_ahead() {
                    return self.exile_chosen_name();
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
                        || self.word_at(1).as_deref() == Some("X")
                        || (self.word_at(1).as_deref() == Some("that")
                            && self.word_at(2).as_deref() == Some("much")))
                {
                    self.pos += 1;
                    let count = self.num()?;
                    self.cards_word()?;
                    let face = self.exile_face();
                    return Ok(Effect::ExileTop {
                        count,
                        who: None,
                        face,
                    });
                }
                if self.eat_words(&["graveyard", "of"]) {
                    let who = self.recipient()?;
                    return Ok(Effect::ExileGraveyard { who, filter: None });
                }
                // `exile all creature card from graveyard of target player`.
                if self.at_word("all")
                    && (2..6).any(|k| {
                        matches!(self.word_at(k).as_deref(), Some("card" | "cards"))
                            && self.word_at(k + 1).as_deref() == Some("from")
                            && self.word_at(k + 2).as_deref() == Some("graveyard")
                            && self.word_at(k + 3).as_deref() == Some("of")
                    })
                {
                    self.pos += 1;
                    let filter = self.filter()?;
                    self.cards_word()?;
                    self.expect_words(&["from", "graveyard", "of"])?;
                    let who = self.recipient()?;
                    return Ok(Effect::ExileGraveyard {
                        who,
                        filter: Some(filter),
                    });
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
            // "you may pay any amount of {E}".
            "pay" if self.at_words(&["any", "energy"]) => {
                self.pos += 2;
                Ok(Effect::PayEnergy(mtg_core::card::PAY_ANY_ENERGY))
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
            "proliferate" => {
                // `proliferate X times`.
                let times = if self
                    .word_at(0)
                    .is_some_and(|w| w.eq_ignore_ascii_case("x") || w == "count")
                {
                    let n = self.num()?;
                    self.expect_word("times")?;
                    Some(n)
                } else {
                    None
                };
                Ok(Effect::Proliferate { times })
            }
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
            // CR 701.66a: "Target land you control becomes ...": the
            // keyword action declares its target.
            "earthbend" => {
                // A delayed ability's targets would be the spell's (Fatal
                // Fissure's "When that creature dies this turn, you
                // earthbend 4" needs targets of its own).
                if self.in_delayed {
                    return self.fail("`earthbend` in a delayed ability: its target would be chosen as the spell is cast");
                }
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
            "exchange" if self.eat_words(&["life", "totals", "with"]) => Ok(Effect::ExchangeLife {
                with: self.recipient()?,
            }),
            "exchange" if self.eat_words(&["control", "of"]) => Ok(Effect::ExchangeControl {
                what: if self.at_many() {
                    vec![self.many_target()?]
                } else {
                    self.recipients()?
                },
            }),
            "end" if self.eat_words(&["the", "turn"]) => Ok(Effect::EndTurn),
            "incubate" => Ok(Effect::Incubate {
                count: self.num()?,
                you: false,
            }),
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
                // `remains`: `forever`, printed "for as long as they remain
                // exiled".
                let remains = self.eat_word("remains");
                let until = if remains {
                    mtg_core::card::PlayUntil::Forever
                } else {
                    self.play_until()?
                };
                let first = self.eat_word("first");
                let it = self.eat_word("it");
                let choose = self.eat_word("choose");
                let choose_one = choose && self.eat_word("one");
                let this_way = self.eat_words(&["this", "way"]);
                let look_those = self.at_words(&["look", "those", "face", "down"]);
                let look_first = look_those || self.at_words(&["look", "face", "down"]);
                if look_first {
                    self.pos += if look_those { 2 } else { 1 };
                }
                let face_down = self.eat_words(&["face", "down"]);
                let possessive = self.at_word("from");
                let from = if self.eat_word("of") || self.eat_word("from") {
                    Some(self.recipient()?)
                } else {
                    None
                };
                // "you may play it if you control a Kavu": the permission's
                // own condition, not the effect's.
                let cond = if self.eat_word("if") {
                    Some(self.static_cond()?)
                } else {
                    None
                };
                let rider = self.cast_rider()?;
                Ok(Effect::ExilePlayable {
                    count,
                    until,
                    first,
                    it,
                    choose,
                    choose_one,
                    from,
                    possessive,
                    this_way,
                    remains,
                    face_down,
                    look_first,
                    look_those,
                    cond,
                    rider,
                })
            }
            "dig" if self.eat_words(&["face", "down"]) => {
                let count = self.number()?;
                let then = self.eat_word("then");
                let any_order = self.eat_words(&["any", "order"]);
                let of_them = self.eat_word("them");
                Ok(Effect::DigFaceDown {
                    count,
                    then,
                    them: of_them,
                    any_order,
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
                let rest_revealed = self.eat_word("revealed");
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
                    rest_revealed,
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
            // `may cast this while exiled`.
            "may" if self.at_words(&["cast", "this", "while", "exiled"]) => {
                self.pos += 4;
                Ok(Effect::MayCastThisFromExile)
            }
            // `may cast target Zombie creature card from your graveyard
            // this turn`, `may cast that card this turn [first] [free]`;
            // `owner may play ...`: the card's owner's permission.
            "may"
                if (self.at_word("play") || self.at_word("cast"))
                    && (matches!(self.word_at(1).as_deref(), Some("target" | "it" | "that"))
                        || (self.word_at(1).as_deref() == Some("this")
                            && self.word_at(2).as_deref() == Some("card"))) =>
            {
                self.may_play(false)
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
            "turn"
                if self.at_words(&[
                    "face",
                    "up",
                    "exiled",
                    "card",
                    "put",
                    "creature",
                    "onto",
                    "battlefield",
                ]) =>
            {
                self.pos += 8;
                Ok(Effect::TurnFaceUpExiled)
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
                // "Look at target opponent's hand and choose a card from
                // it. Put that card on top of that player's library."
                if self.eat_word("topping") {
                    return self.discard_chosen_with(who, true, false, true);
                }
                // "Look at target opponent's hand. You may exile a nonland
                // card from it until this creature leaves the battlefield."
                if self.eat_word("exiling") {
                    let may = self.eat_word("may");
                    let filter = if self.eat_word("card") {
                        None
                    } else {
                        let f = self.filter()?;
                        self.expect_word("card")?;
                        Some(f)
                    };
                    let until = self.eat_words(&["until", "this", "leaves"]);
                    return Ok(Effect::DiscardChosen {
                        reveal: None,
                        who,
                        filter,
                        may,
                        look: true,
                        exile: Some(false),
                        until,
                        and_choose: false,
                        top: false,
                    });
                }
                Ok(Effect::LookAtHand { who })
            }
            "look" if self.at_words(&["at", "top"]) => {
                self.pos += 2;
                self.look_at_top(None)
            }
            "split" if self.at_word("top") => {
                self.pos += 1;
                self.dig_split()
            }
            "piles"
                if matches!(
                    self.word_at(0).as_deref(),
                    Some("look" | "reveal" | "exile" | "exiled")
                ) =>
            {
                self.piles()
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
            // "sacrifice half the non-Demon permanents you control,
            // rounded up".
            "sacrifice" if self.half_ahead() => {
                self.pos -= 1;
                self.subject_verb_inner(Recipient::You, false)
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
            "next" if self.at_words(&["spell", "can't", "be", "countered"]) => {
                self.pos += 4;
                Ok(Effect::NextSpellUncounterable)
            }
            // "The next red or green creature spell you cast this turn can
            // be cast without paying its mana cost."
            "next"
                if (1..8).any(|k| {
                    self.word_at(k).as_deref() == Some("spell")
                        && self.word_at(k + 1).as_deref() == Some("free")
                }) && !self.at_word("spell") =>
            {
                let f = self.filter()?;
                self.expect_words(&["spell", "free"])?;
                Ok(Effect::NextSpellFree(f))
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
            "leaving" => {
                self.expect_word("exiles")?;
                Ok(Effect::ExileOnLeave {
                    what: self.recipient()?,
                })
            }
            "goad" => Ok(Effect::Goad {
                what: self.recipient()?,
            }),
            "detain" => {
                let spelled = self.eat_word("spelled");
                Ok(Effect::Detain {
                    what: self.recipient()?,
                    spelled,
                })
            }
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
            // "draw another card" (Hymn of the Faller).
            "draw" if self.eat_words(&["another", "card"]) => Ok(Effect::DrawAnother),
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
            // "Until end of turn, you may look at the top card of your
            // library any time, and you may play lands and cast spells from
            // the top of your library."
            "play" if self.at_words(&["from", "top", "this", "turn"]) => {
                self.pos += 4;
                let (lands, spells, more) = self.top_play_body()?;
                Ok(Effect::TopPermission(Box::new(crate::ast::TopPlayAst {
                    lands,
                    spells,
                    more,
                })))
            }
            // "Creature cards in your graveyard gain \"You may cast this
            // card from your graveyard\" until end of turn."
            "grant" if self.at_words(&["graveyard", "cast", "to"]) => {
                self.pos += 3;
                let f = self.filter()?;
                self.expect_word("cards")?;
                Ok(Effect::GrantGraveyardCast(f))
            }
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
            // A flip card flips only itself (CR 710.1a): `flip it` is the
            // source, printed "it", even in a trigger with no triggering
            // object ("..., if there are two or more ki counters on this
            // creature, you may flip it.").
            "meld" => {
                self.expect_word("with")?;
                let partner = self.string()?;
                self.expect_word("into")?;
                let into = self.string()?;
                Ok(Effect::Meld { partner, into })
            }
            "flip" if self.eat_word("it") => Ok(Effect::Flip {
                what: Recipient::SourceIt,
            }),
            "flip" => Ok(Effect::Flip {
                what: self.recipient()?,
            }),
            "search" | "searches" => {
                let mut more = crate::ast::SearchMore::default();
                // `search graveyard, hand, or library ...`: "your
                // graveyard, hand, and/or library" (`graveyard, hand or
                // library` without the serial comma); `graveyard, hand,
                // and library`: all three.
                if self.at_word("graveyard")
                    && self.peek_at(1) == Some(&Tok::Comma)
                    && self.word_at(2).as_deref() == Some("hand")
                {
                    self.pos += 3;
                    let comma = self.peek() == Some(&Tok::Comma);
                    if comma {
                        self.pos += 1;
                    }
                    more.zones = if self.eat_words(&["or", "library"]) {
                        crate::ast::SearchZones::HandOr { comma }
                    } else if comma && self.eat_words(&["and", "library"]) {
                        crate::ast::SearchZones::HandAnd
                    } else {
                        return self.unexpected("`or library` or `, and library`");
                    };
                }
                let graveyard = more.zones != crate::ast::SearchZones::Library
                    || self.eat_words(&["library", "or", "graveyard"]);
                let up_to = self.eat_words(&["up", "to"]);
                // `search exactly 2 card ...`: "for exactly two cards".
                more.exactly = !up_to && self.eat_word("exactly");
                // `search up to X ...`, `search up to count where ...`,
                // `search up to that much ...`: "for up to X basic land
                // cards, where X is ...", "for up to that many land cards".
                let count = if matches!(self.peek(), Some(Tok::Num(_)))
                    || self.word_at(0).is_some_and(|w| w.eq_ignore_ascii_case("x"))
                    || (self.at_word("count")
                        && matches!(self.word_at(1).as_deref(), Some("where" | "that")))
                    || self.at_words(&["that", "much"])
                {
                    let n = self.num()?;
                    // `search up to count where land you control, basic
                    // land card ...`: the comma ends the count's filter.
                    if matches!(n, Num::Count(_)) {
                        if self.peek() != Some(&Tok::Comma) {
                            return self.unexpected("`,` after the count");
                        }
                        self.pos += 1;
                    }
                    n
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
                // "a card named ~", "cards not named ~".
                if self.eat_words(&["named", "this"]) {
                    filter.same = Some(SameRef::NamedThis);
                } else if filter.same.is_none() && self.eat_words(&["not", "named", "this"]) {
                    filter.same = Some(SameRef::NotNamedThis);
                }
                // "a card named Ajani, Valiant Protector".
                let mut name = None;
                if self.at_word("named")
                    && let Some(Tok::Str(s)) = self.peek_at(1)
                {
                    name = Some(s.clone());
                    self.pos += 2;
                    // "a card named Heart-Piercer Bow or Vial of Dragonfire".
                    while self.at_words(&["or", "named"])
                        && let Some(Tok::Str(s)) = self.peek_at(2)
                    {
                        let s = s.clone();
                        if let Some(n) = name.as_mut() {
                            n.push('|');
                            n.push_str(&s);
                        }
                        self.pos += 3;
                    }
                }
                // "a creature card with mana value 6 or greater": the mana
                // value after the noun.
                if filter.mana_value.is_none() && self.at_words(&["with", "mana", "value"]) {
                    let g = self.filter()?;
                    filter.mana_value = g.mana_value;
                    filter.status.mana_value_x = g.status.mana_value_x;
                    if g.same == Some(SameRef::ManaValueAtMostPower) {
                        filter.same = g.same;
                    }
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
                // `and green card and blue card`, `and card named "Thinking
                // Cap"`: one card more for each.
                while self.eat_word("and") {
                    let mut f = if self.eat_word("card") {
                        Filter::default()
                    } else {
                        let f = self.filter()?;
                        if !self.eat_word("card") {
                            return self.unexpected("`card`");
                        }
                        f
                    };
                    // `and instant card with mana value 2`: after the noun.
                    if f.mana_value.is_none() && self.at_words(&["with", "mana", "value"]) {
                        let g = self.filter()?;
                        f.mana_value = g.mana_value;
                        f.status.mana_value_x = g.status.mana_value_x;
                    }
                    let mut n = None;
                    if self.at_word("named")
                        && let Some(Tok::Str(s)) = self.peek_at(1)
                    {
                        n = Some(s.clone());
                        self.pos += 2;
                    }
                    more.slots.push((f, n));
                }
                // "with different names" (CR 201.2), "that have different
                // names", "that each have different names".
                if self.eat_words(&["with", "different", "names"]) {
                    more.different_names = true;
                } else if self.eat_words(&["that", "have", "different", "names"]) {
                    more.different_names = true;
                    more.names = crate::ast::NamesWording::ThatHave;
                } else if self.eat_words(&["that", "each", "have", "different", "names"]) {
                    more.different_names = true;
                    more.names = crate::ast::NamesWording::ThatEachHave;
                }
                // `with different names that each have mana value X`
                // (Uncage the Menagerie): the mana value after the names.
                if more.different_names
                    && filter.mana_value.is_none()
                    && self.eat_words(&["that", "each", "have", "mana", "value"])
                {
                    if self.word_at(0).is_some_and(|w| w.eq_ignore_ascii_case("x")) {
                        self.pos += 1;
                        filter.mana_value = Some((Cmp::Exactly, 0));
                        filter.status.mana_value_x = true;
                    } else {
                        filter.mana_value = Some((
                            Cmp::Exactly,
                            u8::try_from(self.number()?).unwrap_or(u8::MAX),
                        ));
                    }
                    more.mana_value_last = true;
                }
                let reveal = self.eat_word("reveal");
                self.expect_word("to")?;
                let mut to = self.search_to()?;
                // `to battlefield tapped with 1 stun counter`, `with X
                // additional +1/+1 counters` (CR 122.6).
                if matches!(to, mtg_core::card::SearchTo::Battlefield { .. })
                    && self.at_word("with")
                    && (matches!(self.peek_at(1), Some(Tok::Num(_)))
                        || self.word_at(1).is_some_and(|w| w.eq_ignore_ascii_case("x")))
                {
                    self.pos += 1;
                    let n = self.num()?;
                    let additional = self.eat_word("additional");
                    let kind = self.counter_kind()?;
                    more.counters = Some((kind, n, additional));
                }
                // `to hand except 2 chosen by target opponent to
                // graveyard`: another player chooses some of the cards.
                if self.eat_word("except") {
                    let n = self.number()?;
                    self.expect_words(&["chosen", "by"])?;
                    let by = if self.eat_word("opponent") {
                        None
                    } else {
                        Some(self.recipient()?)
                    };
                    self.expect_word("to")?;
                    let chosen = self.search_to()?;
                    more.pick = Some((n, by, chosen));
                }
                // `to battlefield tapped if land else hand`: "Put it onto
                // the battlefield tapped if it's a land card. Otherwise, put
                // it into your hand."
                if self.eat_word("if") {
                    let f = self.filter()?;
                    self.expect_word("else")?;
                    let other = self.search_to()?;
                    more.split = Some((f, to));
                    to = other;
                }
                let that_card = self.eat_words(&["that", "card"]);
                // A final `then`: the older wording (not `then <clause>`).
                let old = self.at_word("then") && !matches!(self.peek_at(1), Some(Tok::Word(_)));
                if old {
                    self.pos += 1;
                }
                let those = self.eat_word("those");
                more.wording = if self.eat_word("short") {
                    crate::ast::SearchWording::Short
                } else if self.eat_word("sentence") {
                    crate::ast::SearchWording::Sentence
                } else if self.eat_words(&["of", "them"]) {
                    crate::ast::SearchWording::OfThem
                } else if self.eat_word("rest") {
                    crate::ast::SearchWording::Rest
                } else if self.eat_words(&["instead", "of"]) {
                    crate::ast::SearchWording::InsteadOf(self.number()?)
                } else if self.eat_word("unshuffled") {
                    crate::ast::SearchWording::Unshuffled
                } else if self.eat_words(&["put", "instead"]) {
                    let of = if self.at_word("of") && !self.at_words(&["of", "them"]) {
                        self.pos += 1;
                        Some(self.search_to()?)
                    } else {
                        None
                    };
                    crate::ast::SearchWording::PutInstead {
                        then: self.eat_word("then"),
                        of,
                    }
                } else if self.eat_word("repeat") {
                    crate::ast::SearchWording::Repeat
                } else {
                    crate::ast::SearchWording::Plain
                };
                let more = (more != crate::ast::SearchMore::default()).then(|| Box::new(more));
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
                    graveyard,
                    name,
                    old,
                    those,
                    more,
                })
            }
            "add" => {
                let e = self.add_effect()?;
                self.spend_only_suffix(e)
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
                // `copy target 1 [then] may cast [free]` (CR 707.12).
                let then = self.at_words(&["then", "may", "cast"]);
                if then || self.at_words(&["may", "cast"]) {
                    self.pos += if then { 3 } else { 2 };
                    let free = self.eat_word("free");
                    return Ok(Effect::CastCopy {
                        what,
                        free,
                        wording: if then {
                            crate::ast::CopyCastWording::Then
                        } else {
                            crate::ast::CopyCastWording::Sentences
                        },
                    });
                }
                let times = if self.eat_word("times") {
                    self.num()?
                } else {
                    Num::N(1)
                };
                let and_may = self.eat_words(&["and", "new", "targets"]);
                let new_targets = and_may || self.eat_words(&["new", "targets"]);
                let except = self.except_clause();
                Ok(Effect::CopySpells {
                    what,
                    times,
                    new_targets,
                    except,
                    and_may,
                })
            }
            // "Change the target of target spell or ability with a single
            // target" (CR 115.7a).
            "change" if self.at_words(&["target", "of"]) => {
                self.pos += 2;
                let what = self.recipient()?;
                Ok(Effect::ChangeTarget { what })
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
                // `create tapped [and attacking] token copy of ...`.
                let entry = |p: &Self, at: usize| -> Option<(usize, bool)> {
                    match (p.word_at(at).as_deref(), p.word_at(at + 1).as_deref()) {
                        (Some("tapped"), Some("and"))
                            if p.word_at(at + 2).as_deref() == Some("attacking") =>
                        {
                            Some((3, true))
                        }
                        (Some("tapped"), _) => Some((1, false)),
                        _ => None,
                    }
                };
                let (skip, attack) = entry(self, 0).unwrap_or((0, false));
                if matches!(self.word_at(skip).as_deref(), Some("token"))
                    && matches!(self.word_at(skip + 1).as_deref(), Some("copy"))
                    || skip == 0 && self.at_words(&["a", "token", "copy"])
                {
                    self.pos += skip;
                    if !self.eat_word("token") {
                        self.expect_words(&["a", "token"])?;
                    }
                    self.expect_word("copy")?;
                    self.expect_word("of")?;
                    let of = self.recipient()?;
                    let except = self.except_clause();
                    let trailing =
                        skip == 0 && self.eat_words(&["entering", "tapped", "and", "attacking"]);
                    self.created_at = Some(self.targets.len());
                    return Ok(Effect::CreateTokenCopy {
                        of,
                        except,
                        count: Num::N(1),
                        tapped: skip > 0 || trailing,
                        attacking: attack || trailing,
                        trailing,
                    });
                }
                // `create 2 token copies of this`, `create x token copies
                // of target ...`, `create 2 tapped token copies of this`.
                if (matches!(self.peek(), Some(Tok::Num(_))) || self.at_word("x"))
                    && let (skip, attack) = entry(self, 1).unwrap_or((0, false))
                    && self.word_at(1 + skip).as_deref() == Some("token")
                    && self.word_at(2 + skip).as_deref() == Some("copies")
                {
                    let count = self.num()?;
                    self.pos += 2 + skip;
                    self.expect_word("of")?;
                    let of = self.recipient()?;
                    let except = self.except_clause();
                    let trailing =
                        skip == 0 && self.eat_words(&["entering", "tapped", "and", "attacking"]);
                    self.created_at = Some(self.targets.len());
                    return Ok(Effect::CreateTokenCopy {
                        of,
                        except,
                        count,
                        tapped: skip > 0 || trailing,
                        attacking: attack || trailing,
                        trailing,
                    });
                }
                let count = self.num()?;
                self.created_at = (count == Num::N(1)).then_some(self.targets.len());
                let tapped = self.eat_word("tapped");
                let attacking = tapped && self.eat_words(&["and", "attacking"]);
                let token = self.string()?;
                self.eat_word("token");
                self.eat_word("tokens");
                let it_has = self.eat_words(&["it", "has"]);
                let attached = if self.eat_words(&["attached", "to"]) {
                    Some(self.recipient()?)
                } else {
                    None
                };
                // `sized count where land you control`: "an X/X ...
                // token, where X is ...".
                let sized = if self.eat_word("sized") {
                    Some(self.num()?)
                } else {
                    None
                };
                Ok(Effect::CreateToken {
                    token,
                    count,
                    tapped,
                    attacking,
                    who: None,
                    it_has,
                    attached,
                    sized,
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
                let was_player = self.delayed_player;
                self.in_delayed = true;
                self.delayed_that = about == Recipient::TriggeringObject;
                self.delayed_player = match about {
                    Recipient::Target(i) | Recipient::ThatTarget(i)
                        if matches!(
                            self.targets.get(i),
                            Some(Target::Spec(TargetSpec::Player | TargetSpec::Opponent))
                        ) =>
                    {
                        Some(i)
                    }
                    _ => None,
                };
                // `{ when ..., <effects> }`: the body ends at the brace.
                let closed = self.peek() == Some(&Tok::LBrace);
                if closed {
                    self.pos += 1;
                }
                let targets_before = self.targets.len();
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
                } else if closed {
                    return self.fail("`delayed ...: {` takes a trigger first");
                } else {
                    let e = self.delayed_effects()?;
                    let t = self.trigger()?;
                    (t, e, false)
                };
                if closed {
                    self.expect(&Tok::RBrace)?;
                } else if trigger_first && effects.len() > 1 && self.targets.len() > targets_before
                {
                    // "When that creature dies this turn, you gain 3 life.
                    // Target creature you control fights that creature.":
                    // the later sentence is the spell's, which the open
                    // form would put in the delayed trigger.
                    return self.fail(
                        "a delayed trigger's later sentence declares a target: close the trigger with `{ ... }` if the sentence is not the trigger's",
                    );
                }
                (self.in_delayed, self.delayed_that) = was;
                self.delayed_player = was_player;
                self.subject = saved_subject;
                Ok(Effect::Delayed {
                    about,
                    trigger,
                    effects,
                    trigger_first,
                    this_turn: this_turn.is_some() || turn_long,
                    it,
                    closed,
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
                        // `... this turn except from <filter>` (Inspire
                        // Awe).
                        Replaces::Damage {
                            from: None,
                            to,
                            combat,
                            ..
                        } if self.eat_words(&["except", "from"]) => Replaces::Damage {
                            from: Some(Affects::Except(self.filter()?)),
                            to,
                            combat,
                            wording: DamageWording::Dealt,
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
                let wording = self.then_wording();
                let then = self.created_then()?;
                Ok(Effect::CreateReplacement(Replacement {
                    event,
                    outcome: match next {
                        Some(n) => Outcome::PreventNext(n),
                        None => Outcome::Prevent,
                    },
                    then,
                    token: None,
                    wording,
                    cond: None,
                }))
            }
            "players" => {
                self.expect_word("can't")?;
                let kind = if self.eat_words(&["draw", "cards"]) {
                    if self.eat_words(&["or", "gain", "life"]) {
                        RuleKind::NoDrawOrLifeGain
                    } else {
                        RuleKind::NoDraw
                    }
                } else {
                    self.expect_words(&["gain", "life"])?;
                    RuleKind::NoLifeGain
                };
                self.expect_words(&["this", "turn"])?;
                Ok(Effect::Rule(kind))
            }
            "damage" => {
                self.expect_words(&["can't", "be", "prevented", "this", "turn"])?;
                Ok(Effect::Rule(RuleKind::DamageCantBePrevented))
            }
            "combat" if self.at_words(&["damage", "can't", "be", "prevented", "this", "turn"]) => {
                self.pos += 6;
                Ok(Effect::Rule(RuleKind::CombatDamageCantBePrevented))
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

    /// `graveyard` or `exile`: where a permanent came from.
    fn came_from_zone(&mut self) -> R<mtg_core::state::Zone> {
        if self.eat_word("graveyard") {
            Ok(mtg_core::state::Zone::Graveyard)
        } else if self.eat_word("exile") {
            Ok(mtg_core::state::Zone::Exile)
        } else {
            self.unexpected("`graveyard` or `exile`")
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
            // The triggering event's amount: none for a cast, a step, or
            // life gain ("If eight or more mana was spent to cast that
            // spell, ... deals that much damage" would deal 0).
            if self.in_trigger && !self.in_delayed && self.no_trigger_amount {
                return self.fail("`that much`: this trigger's event has no amount");
            }
            self.pos += 2;
            return Ok(Num::ThatMuch);
        }
        if self.at_word("count")
            && (matches!(
                self.word_at(1).as_deref(),
                Some("where" | "equal" | "each" | "foreach")
            ) || self.at_words(&["count", "that", "many"]))
        {
            self.pos += 1;
            let style = match self.word()?.as_str() {
                "where" => crate::ast::CountStyle::Where,
                "equal" => crate::ast::CountStyle::Equal,
                "foreach" => crate::ast::CountStyle::ForEach,
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
            // `2 times that much`: "twice that many" (Angel of Suffering).
            Some(Tok::Num(n))
                if self.word_at(1).as_deref() == Some("times")
                    && self.word_at(2).as_deref() == Some("that")
                    && self.word_at(3).as_deref() == Some("much") =>
            {
                let n = *n;
                if self.in_trigger && !self.in_delayed && self.no_trigger_amount {
                    return self.fail("`that much`: this trigger's event has no amount");
                }
                self.pos += 4;
                Ok(Num::ThatMuchTimes(n))
            }
            // `2 times X`: "twice X".
            Some(Tok::Num(n))
                if self.word_at(1).as_deref() == Some("times")
                    && self.word_at(2).is_some_and(|w| w.eq_ignore_ascii_case("x")) =>
            {
                let n = *n;
                self.pos += 3;
                Ok(Num::XTimes(n))
            }
            // `3 minus X`: "three minus X".
            Some(Tok::Num(n))
                if self.word_at(1).as_deref() == Some("minus")
                    && self.word_at(2).is_some_and(|w| w.eq_ignore_ascii_case("x")) =>
            {
                let n = *n;
                self.pos += 3;
                Ok(Num::NMinusX(n))
            }
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
                // `2 times X`: "twice X".
                if lw == "twice" && self.word_at(1).as_deref() == Some("x") {
                    self.pos += 2;
                    return Ok(Num::XTimes(2));
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
    /// `half up`, `half down` next (`CountStyle::Half`).
    fn half_ahead(&self) -> bool {
        self.at_word("half") && matches!(self.word_at(1).as_deref(), Some("up" | "down"))
    }

    /// `half up`/`half down` of the acting player's count `inner`, as
    /// written right after a per-player verb.
    fn half_count(&mut self, inner: crate::ast::CountExpr) -> R<Num> {
        let up = self.word_at(1).as_deref() == Some("up");
        self.pos += 2;
        self.push_count(crate::ast::Count {
            expr: crate::ast::CountExpr::Divided {
                by: 2,
                up,
                inner: Box::new(inner),
            },
            style: crate::ast::CountStyle::Half,
        })
    }

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
            Num::X
            | Num::XPlus(_)
            | Num::XTimes(_)
            | Num::ThatMuchTimes(_)
            | Num::NMinusX(_)
            | Num::Count(_)
            | Num::ThatMuch => self.fail("X is not allowed here"),
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
        // `chosen number`: "the chosen number".
        if self.eat_words(&["chosen", "number"]) {
            return Ok(Some(CountExpr::ChosenNumber));
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
            // `nonland card revealed this way` (parley): cards in a library.
            // `creature card milled this way`: milled cards are cards.
            if self.at_words(&["card", "revealed"]) || self.at_words(&["card", "milled"]) {
                self.pos += 1;
            }
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
        // `half down <count>`, `half up <count>`, `every 7 <count>`.
        if self.at_word("half") && matches!(self.word_at(1).as_deref(), Some("down" | "up")) {
            let up = self.word_at(1).as_deref() == Some("up");
            self.pos += 2;
            let inner = self.count_expr()?;
            return Ok(crate::ast::CountExpr::Divided {
                by: 2,
                up,
                inner: Box::new(inner),
            });
        }
        if self.at_word("every") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            let by = self.number()?;
            let inner = self.count_expr()?;
            return Ok(crate::ast::CountExpr::Divided {
                by,
                up: false,
                inner: Box::new(inner),
            });
        }
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
            // After a target, its stat ("you lose life equal to its mana
            // value" of the returned card).
            if let Some(n) = self.last_object_target() {
                self.pos += len + 1;
                return Some(match stat {
                    Stat::ManaValue => CountExpr::TargetManaValue {
                        n,
                        that: false,
                        the_spell: false,
                    },
                    Stat::Power | Stat::Toughness => CountExpr::TargetStat {
                        n,
                        toughness: stat == Stat::Toughness,
                        that: false,
                    },
                });
            }
            if self.no_trigger_obj {
                return None;
            }
            self.pos += len + 1;
            return Some(CountExpr::TriggerStat {
                stat,
                that: false,
                spell: false,
            });
        }
        if self.in_trigger
            && at(0).as_deref() == Some("that")
            && matches!(at(1).as_deref(), Some("creature" | "spell"))
            && !self.no_trigger_obj
        {
            let spell = at(1).as_deref() == Some("spell");
            self.pos += len + 2;
            return Some(CountExpr::TriggerStat {
                stat,
                that: true,
                spell,
            });
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
        if self.eat_words(&["loyalty", "of", "this"]) {
            return Ok(CountExpr::SourceLoyalty);
        }
        if self.eat_words(&["damage", "dealt", "this", "way"]) {
            return Ok(CountExpr::DamageThisWay {
                excess: false,
                to_that: false,
            });
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
        if self.eat_words(&["creatures", "entered", "this", "turn"]) {
            return Ok(CountExpr::CreaturesEnteredThisTurn);
        }
        if self.eat_words(&["discarded", "this", "turn"]) {
            return Ok(CountExpr::DiscardedThisTurn);
        }

        if self.eat_words(&["opponents", "more", "cards", "in", "hand"]) {
            return Ok(CountExpr::OpponentsMoreCards);
        }
        if self.at_words(&["opponents", "hand"]) && matches!(self.peek_at(2), Some(Tok::Num(_))) {
            self.pos += 2;
            let n = self.number()?;
            self.expect_words(&["or", "less"])?;
            return Ok(CountExpr::OpponentsHandAtMost(n));
        }
        // `their hand`, `their library`, `their life`: the acting
        // player's.
        if self.at_word("their")
            && matches!(
                self.word_at(1).as_deref(),
                Some("hand" | "library" | "life")
            )
        {
            use mtg_core::card::PlayerCount;
            let what = match self.word_at(1).as_deref() {
                Some("hand") => PlayerCount::Hand,
                Some("library") => PlayerCount::Library,
                _ => PlayerCount::Life,
            };
            self.pos += 2;
            return Ok(CountExpr::Their(what));
        }
        // `opponents didn't discard 2 nonland card`, `those opponents
        // didn't discard 1 card with mana value 4 or greater`.
        if self.at_words(&["opponents", "didn't", "discard"])
            || self.at_words(&["those", "opponents", "didn't", "discard"])
        {
            let those = self.eat_word("those");
            self.pos += 3;
            let n = self.number()?;
            let filter = if self.at_word("card") || self.at_word("cards") {
                self.pos += 1;
                // `1 card with mana value 4 or greater`.
                if self.at_word("with") {
                    Some(self.filter()?)
                } else {
                    None
                }
            } else {
                let f = self.filter()?;
                self.cards_word()?;
                Some(f)
            };
            return Ok(CountExpr::PlayersDidntDiscard { n, filter, those });
        }
        // `counters on this`, `counters on it`: every kind.
        if self.eat_words(&["counters", "on"]) {
            if self.eat_word("this") {
                return Ok(CountExpr::SourceAllCounters { it: false });
            }
            self.expect_word("it")?;
            // In a triggered ability `it` is the triggering object: its
            // counters of every kind ("the number of counters on it",
            // Spider-Man Noir).
            if self.in_trigger && !self.it_source && !self.in_delayed {
                return Ok(CountExpr::TriggerCounters(CounterKind::Any));
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
            return Ok(CountExpr::DamageThisWay {
                excess: true,
                to_that: false,
            });
        }
        if self.eat_words(&[
            "excess", "damage", "dealt", "to", "that", "creature", "this", "way",
        ]) {
            return Ok(CountExpr::DamageThisWay {
                excess: true,
                to_that: true,
            });
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
        if self.eat_words(&["colors", "of", "target"]) {
            let n = self.number()? as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(CountExpr::TargetColors(n - 1));
        }
        if self.eat_words(&["library", "of", "target"]) {
            let n = self.number()? as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(CountExpr::TargetLibrary(n - 1));
        }
        // `max 2 <count>`: "2 or ..., whichever is greater".
        if self.at_word("max") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            let n = self.number()?;
            let inner = self.count_expr()?;
            return Ok(CountExpr::AtLeast(n, Box::new(inner)));
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
        // `creature it devoured`, `Goblin it devoured` (CR 702.82c).
        if (1..5).any(|k| {
            self.word_at(k).as_deref() == Some("it")
                && self.word_at(k + 1).as_deref() == Some("devoured")
        }) {
            let f = self.filter()?;
            self.expect_words(&["it", "devoured"])?;
            return Ok(CountExpr::Devoured(f));
        }
        if self.eat_words(&["greatest", "power", "graveyard"]) {
            return Ok(CountExpr::GreatestPowerInGraveyard(self.filter()?));
        }
        if self.eat_words(&["greatest", "power"]) {
            // "the greatest power among Giants you control" (Pummeler for
            // Hire): a creature type alone.
            let subtype = matches!(self.peek(), Some(Tok::Word(w))
                if w.chars().next().is_some_and(char::is_uppercase));
            let f = if subtype || self.word_at(0).is_some_and(|w| card_type(&w).is_some()) {
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
        if self.eat_word("speed") {
            return Ok(CountExpr::Speed);
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
        if self.eat_words(&["greatest", "mana", "value", "among"]) {
            return Ok(CountExpr::GreatestManaValueOf(self.filter()?));
        }
        if self.eat_words(&["greatest", "mana", "value", "graveyard"]) {
            let f = if self.at_stmt_end()
                || !self.word_at(0).is_some_and(|w| {
                    card_type(&w).is_some() || w.starts_with("non") || color(&w).is_some()
                }) {
                None
            } else {
                Some(self.filter()?)
            };
            return Ok(CountExpr::GreatestManaValueInGraveyard(f));
        }
        if self.eat_words(&["greatest", "mana", "value"]) {
            return Ok(CountExpr::GreatestManaValue);
        }
        if self.eat_words(&["colors", "of", "that", "spell"]) {
            return Ok(CountExpr::ThatSpellColors);
        }
        if self.eat_words(&["damage", "dealt", "to", "it", "this", "turn"]) {
            return Ok(CountExpr::SourceDamage);
        }
        if self.eat_words(&["creatures", "left", "this", "turn"]) {
            return Ok(CountExpr::CreaturesLeftThisTurn);
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
        if self.eat_words(&["mana", "spent"]) {
            let that = self.eat_words(&["that", "spell"]);
            return Ok(CountExpr::ManaSpent { that });
        }
        if self.eat_words(&["colors", "spent"]) {
            let of = if self.eat_words(&["that", "spell"]) {
                crate::ast::SpentOf::That
            } else if self.eat_word("spell") {
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
        if self.eat_words(&["energy", "paid"]) {
            return Ok(CountExpr::EnergyPaid);
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
            if n == 0 || (n > self.targets.len() && !self.counters_on_next_target(n)) {
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
        // "cards exiled with this enchantment" (CR 607.2a), "cards you
        // own in exile [and in your graveyard]".
        let exiled_with = self.eat_words(&["exiled", "with", "this"]);
        let in_exile = !exiled_with && self.eat_word("exile");
        if exiled_with || in_exile {
            let graveyard = in_exile && self.eat_words(&["and", "graveyard"]);
            let f = if self.eat_word("card")
                || self.at_stmt_end()
                || !self.word_at(0).is_some_and(|w| {
                    card_type(&w).is_some() || w.starts_with("non") || color(&w).is_some()
                }) {
                None
            } else {
                let f = self.filter()?;
                self.eat_word("card");
                Some(f)
            };
            return Ok(if exiled_with {
                CountExpr::ExiledWith(f)
            } else {
                CountExpr::CardsInExile {
                    filter: f,
                    graveyard,
                }
            });
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
                if n == 0 || (n > self.targets.len() && !self.counters_on_next_target(n)) {
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
        // `difference 10 hand`: "the difference".
        if self.at_word("difference") && matches!(self.peek_at(1), Some(Tok::Num(_))) {
            self.pos += 1;
            let n = self.number()?;
            let inner = self.count_expr()?;
            return Ok(CountExpr::Difference {
                n,
                inner: Box::new(inner),
            });
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
            delayed_player: self.delayed_player,
            delayed_dies: self.delayed_dies,
            in_trigger: self.in_trigger,
            per_opponent: self.per_opponent,
            it_source: self.it_source,
            no_trigger_obj: self.no_trigger_obj,
            self_trigger: self.self_trigger,
            no_trigger_player: self.no_trigger_player,
            created_at: self.created_at,
            no_trigger_amount: self.no_trigger_amount,
            in_spell: self.in_spell,
            group_named: self.group_named,
            subject: None,
            subject_each: false,
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
            || (self.at_words(&["up", "to", "x"]) && target(3))
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
        } else if self.eat_words(&["up", "to", "x"]) {
            // "up to X target cards" (CR 601.2c).
            (Num::N(0), Some(Num::X))
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
            if another {
                self.another_target(spec)?
            } else {
                spec
            }
        };
        let total = self.total_mana_value()?;
        let different = self.eat_words(&["with", "different", "names"]);
        let i = self.targets.len();
        self.targets.push(Target::Many {
            target: Box::new(target),
            min,
            max,
            total,
            different,
        });
        Ok(Recipient::Target(i))
    }

    /// After a group's target: `with total mana value 3 or less` (CR
    /// 601.2c), or nothing.
    fn total_mana_value(&mut self) -> R<Option<Num>> {
        if !self.eat_words(&["with", "total", "mana", "value"]) {
            return Ok(None);
        }
        let n = self.num()?;
        self.expect_words(&["or", "less"])?;
        Ok(Some(n))
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
                spec = self.another_target(spec)?;
            }
            // "up to three target creature cards with total mana value 3
            // or less": one variable group (CR 601.2c).
            // "up to six target creature cards with different names".
            let total = if up_to {
                self.total_mana_value()?
            } else {
                None
            };
            let different = up_to && self.eat_words(&["with", "different", "names"]);
            if total.is_some() || different {
                let i = self.targets.len();
                self.targets.push(Target::Many {
                    target: Box::new(spec),
                    min: Num::N(0),
                    max: Some(Num::N(count)),
                    total,
                    different,
                });
                return Ok(vec![Recipient::Target(i)]);
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
    /// Whose counters "put its counters on ..." moves: in a triggered
    /// ability, the triggering object's ("put its counters on this
    /// creature", Buzzard-Wasp Colony; "Whenever another creature you
    /// control dies, put its counters on target creature"), unless the
    /// trigger is about the source itself.
    fn its_counters_triggering(&self, to: Recipient) -> bool {
        self.in_trigger
            && !self.in_delayed
            && (to == Recipient::Source
                || !(self.self_trigger || self.no_trigger_player || self.no_trigger_obj))
    }

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
        // `each card exiled this way`, `each creature card exiled this
        // way`: the targets the ability exiled, each in turn ("For each
        // card exiled this way, create a token that's a copy of that
        // card", Hour of Eternity).
        for (words, creature) in [
            (&["each", "card", "exiled", "this", "way"][..], false),
            (
                &["each", "creature", "card", "exiled", "this", "way"][..],
                true,
            ),
        ] {
            if self.at_words(words) {
                if self.targets.is_empty() {
                    return self
                        .fail("`each card exiled this way` needs targets declared before it");
                }
                self.pos += words.len();
                return Ok(Recipient::EachExiledThisWay(
                    0,
                    self.targets.len(),
                    creature,
                ));
            }
        }
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
        // "each opponent who didn't", "each player who doesn't", "each
        // opponent who can't": those who didn't do what the last
        // per-player act asked.
        if self.at_word("each")
            && matches!(self.word_at(1).as_deref(), Some("opponent" | "player"))
            && self.word_at(2).as_deref() == Some("who")
            && matches!(
                self.word_at(3).as_deref(),
                Some("didn't" | "doesn't" | "can't")
            )
            && self.word_at(4).as_deref() != Some("control")
        {
            use crate::ast::DeclinedTense;
            let player = self.word_at(1).as_deref() == Some("player");
            let tense = match self.word_at(3).as_deref() {
                Some("didn't") => DeclinedTense::Didnt,
                Some("doesn't") => DeclinedTense::Doesnt,
                _ => DeclinedTense::Cant,
            };
            self.pos += 4;
            let tail =
                tense == DeclinedTense::Didnt && self.eat_words(&["sacrifice", "or", "discard"]);
            return Ok(Recipient::Declined(crate::ast::Declined {
                player,
                tense,
                tail,
            }));
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
            // After "create a ... token", the token.
            if self.created_at == Some(self.targets.len()) && !self.at_word("deals") {
                return Ok(Recipient::CreatedObject);
            }
            if self.in_trigger && !self.it_source {
                // After a target the ability names, `it` is that target
                // ("put a +1/+1 counter on target creature; it gains
                // flying"), as in a spell; `it deals` stays the triggering
                // object.
                if let Some(n) = self.last_object_target()
                    && !self.at_word("deals")
                {
                    return Ok(Recipient::Target(n));
                }
                if self.no_trigger_obj {
                    return self.fail(
                        "`it`: this trigger has no triggering object; write `this` or `target N`",
                    );
                }
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
            // `enchanted creature and each other creature that share
            // creature type`: the host and the creatures sharing a
            // creature type with it.
            if self.at_words(&["and", "each", "other"]) {
                let save = (self.pos, self.groups.len());
                self.pos += 1;
                if let Ok(Recipient::Group(g)) = self.recipient()
                    && self.groups[usize::from(g)].same == Some(SameRef::ShareCreatureType)
                {
                    return Ok(Recipient::AttachedAnd(g));
                }
                self.pos = save.0;
                self.groups.truncate(save.1);
            }
            return Ok(Recipient::Attached);
        }
        if self.eat_words(&["triggering", "card"]) {
            return Ok(Recipient::TriggeringCard);
        }
        // `triggering it`: the triggering object, printed "it", even after
        // a target (Super-Soldier Serum's "attach any number of target
        // Equipment you control to it").
        if self.at_words(&["triggering", "it"]) {
            if !self.in_trigger || self.no_trigger_obj {
                return self.fail("`triggering it`: this ability has no triggering object");
            }
            self.pos += 2;
            return Ok(Recipient::TriggeringIt);
        }
        // `controller of enchanted`: "enchanted creature's controller".
        if self.eat_words(&["controller", "of", "enchanted"]) {
            return Ok(Recipient::AttachedController);
        }
        // `controller of granter`: "Hold for Ransom's controller".
        if self.eat_words(&["controller", "of", "granter"]) {
            return Ok(Recipient::GranterController);
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
        if self.in_trigger && !self.in_delayed && self.eat_words(&["one", "of", "them"]) {
            return Ok(Recipient::TriggeringOneOfThem);
        }
        // "Exile them at the beginning of the next end step." (the
        // tokens a delayed trigger is about).
        if self.in_delayed && self.eat_word("them") {
            return Ok(Recipient::SameThem);
        }
        // "Those tokens gain haste until end of turn." as a subject.
        if !self.in_delayed && self.eat_words(&["those", "copies"]) {
            return Ok(Recipient::CreatedThose);
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
        if self.in_delayed && self.eat_words(&["those", "cards"]) {
            return Ok(Recipient::SameCards);
        }
        if self.in_delayed && self.eat_words(&["the", "exiled", "cards"]) {
            return Ok(Recipient::SameExiledCards);
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
        if self.eat_word("granter") {
            if self.eat_words(&["as", "it"]) {
                return Ok(Recipient::GranterIt);
            }
            return Ok(Recipient::Granter);
        }
        if self.at_words(&["that", "card"]) {
            self.pos += 2;
            return Ok(if self.in_delayed {
                Recipient::SameCard
            } else {
                Recipient::CreatedObject
            });
        }
        // "any target that was dealt damage this turn".
        if self.eat_words(&[
            "any", "target", "that", "was", "dealt", "damage", "this", "turn",
        ]) {
            self.targets.push(Target::Spec(TargetSpec::AnyDamaged));
            return Ok(Recipient::Target(self.targets.len() - 1));
        }
        if self.eat_words(&["any", "target"]) {
            self.targets.push(Target::Spec(TargetSpec::Any));
            return Ok(Recipient::Target(self.targets.len() - 1));
        }
        // "any other target" (CR 115.3): not the source nor another of
        // the ability's targets.
        if self.eat_words(&["any", "other", "target"]) {
            self.targets.push(Target::Spec(TargetSpec::AnyOther));
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
        // `the full target 1`, `the chosen full target 1`: "the creature
        // you control", "the chosen creature you control".
        if (self.at_words(&["the", "full", "target"])
            && matches!(self.peek_at(3), Some(Tok::Num(_))))
            || (self.at_words(&["the", "chosen", "full", "target"])
                && matches!(self.peek_at(4), Some(Tok::Num(_))))
        {
            let chosen = self.word_at(1).as_deref() == Some("chosen");
            self.pos += if chosen { 4 } else { 3 };
            let Some(Tok::Num(n)) = self.next() else {
                return self.fail("expected a target number after `the full target`");
            };
            let n = n as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(Recipient::TheTargetFull(n - 1, chosen));
        }
        if self.eat_words(&["the", "chosen", "targets"]) {
            if self.targets.len() < 2 {
                return self.fail("`the chosen targets` needs two targets declared before it");
            }
            return Ok(Recipient::ChosenTargets(0, self.targets.len()));
        }
        if self.at_words(&["the", "chosen", "target"])
            && matches!(self.peek_at(3), Some(Tok::Num(_)))
        {
            self.pos += 3;
            let Some(Tok::Num(n)) = self.next() else {
                return self.fail("expected a target number after `the chosen target`");
            };
            let n = n as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(Recipient::ChosenTarget(n - 1));
        }
        // `the other target 2`: "the other" (after `one of those
        // targets`).
        if self.at_words(&["the", "other", "target"])
            && matches!(self.peek_at(3), Some(Tok::Num(_)))
        {
            self.pos += 3;
            let Some(Tok::Num(n)) = self.next() else {
                return self.fail("expected a target number after `the other target`");
            };
            let n = n as usize;
            if n == 0 || n > self.targets.len() {
                return self.fail(format!("target {n} has not been declared"));
            }
            return Ok(Recipient::OtherTarget(n - 1));
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
                spec = self.another_target(spec)?;
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
        if self.in_delayed && self.at_words(&["the", "token"]) {
            self.pos += 2;
            return Ok(Recipient::SameTheToken);
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
        // "When this Equipment enters, attach it to target creature you
        // control. That creature gains ...": the triggering object is the
        // source, which "that creature" never names; it is the target. So
        // in a trigger with no triggering object ("At the beginning of
        // combat on your turn, ... target creature. That creature
        // becomes a Mutant ...").
        if (self.self_trigger || self.in_trigger && self.no_trigger_obj)
            && (self.at_words(&["that", "creature"]) || self.at_words(&["that", "permanent"]))
            && let Some(n) = self.last_object_target()
        {
            self.pos += 2;
            return Ok(Recipient::ThatTarget(n));
        }
        // "When this Aura enters, ... untap that creature" (Howl of the
        // Hunt): the triggering object is the source, which "that
        // creature" doesn't name, and there is no target for it to be.
        if self.self_trigger
            && !self.in_delayed
            && self.at_words(&["that", "creature"])
            && self.last_object_target().is_none()
        {
            return self.fail(
                "`that creature` in a trigger on this object's own event names no creature: write `this`, `attached`, or a target",
            );
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
        // discards two cards. ... it deals 3 damage to that player"), in
        // a triggered ability too ("target player mills three cards ...
        // that player mills ten cards instead").
        // In a delayed trigger about that target player, the player it is
        // about (CR 603.7).
        if self.in_delayed
            && self.at_words(&["that", "player"])
            && self
                .delayed_player
                .is_some_and(|i| i + 1 == self.targets.len())
        {
            self.pos += 2;
            return Ok(Recipient::TriggeringPlayer);
        }
        if self.at_words(&["that", "player"])
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
        // "An opponent gains control of this artifact."
        if self.at_words(&["an", "opponent", "gains", "control"]) {
            self.pos += 2;
            return Ok(Recipient::AnOpponent);
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
                                | "card"
                                | "exiled"
                        )
                    ) || self.word_at(ws.len()).as_deref() == Some("or")
                        && self.word_at(ws.len() + 1).as_deref() == Some("planeswalker")
                        || self.word_at(ws.len()).as_deref() == Some("they")
                        && self.word_at(ws.len() + 1).as_deref() == Some("control")
                        // "each creature you control other than this
                        // creature" (Dawnstrike Vanguard).
                        || self.word_at(ws.len()).as_deref() == Some("other")
                        && self.word_at(ws.len() + 1).as_deref() == Some("than")
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
            // `all card exiled with this`: any card.
            let mut f = if self.at_words(&["card", "exiled", "with", "this"]) {
                self.pos += 1;
                Filter::default()
            } else {
                let f = self.filter()?;
                if self.at_words(&["card", "exiled", "with", "this"]) {
                    self.pos += 1;
                }
                f
            };
            f.all |= all;
            f.all_word |= every;
            f.each = single;
            // `all card exiled with this`: the cards this permanent's
            // abilities exiled (CR 607.2a).
            let exiled = self.eat_words(&["exiled", "with", "this"]);
            // "Zombies you control get +2/+1 ... Other creatures you
            // control get +1/+1" (In Oketra's Name): in a spell, "other"
            // after another group means not that group, but `other`
            // excludes only the source.
            if self.in_spell && f.relation == Relation::Other && self.groups.iter().any(|g| *g != f)
            {
                return self.fail(
                    "`other` in a spell's group after another group: it excludes the spell, not the earlier group; this form isn't supported",
                );
            }
            let i = if let Some(i) = self.groups.iter().position(|g| *g == f) {
                i
            } else {
                self.groups.push(f);
                self.groups.len() - 1
            };
            if exiled {
                return Ok(Recipient::ExiledWith(u16::try_from(i).map_err(|_| {
                    ParseError {
                        line: self.line(),
                        message: "too many groups".to_string(),
                    }
                })?));
            }
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
        // "target spell or creature" (CR 115.1).
        if self.at_words(&["spell", "or"])
            && !matches!(
                self.word_at(2).as_deref(),
                Some("ability" | "abilities" | "activated" | "triggered")
            )
        {
            self.pos += 2;
            return Ok(Target::SpellOr(self.filter()?));
        }
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
                        delayed_player: None,
                        delayed_dies: false,
                        in_trigger: false,
                        per_opponent: false,
                        it_source: false,
                        no_trigger_obj: false,
                        self_trigger: false,
                        no_trigger_player: false,
                        created_at: None,
                        no_trigger_amount: false,
                        in_spell: false,
                        group_named: false,
                        subject: None,
                        subject_each: false,
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
                    (Some("you"), Some("don't" | "control" | "own"))
                        | (Some("an"), Some("opponent"))
                        | (Some("target"), Some("player" | "opponent"))
                        | (Some("defending" | "your"), Some("player" | "opponents"))
                        | (
                            Some("that"),
                            Some("was" | "entered" | "dealt" | "blocked" | "player" | "share" | "opponent")
                        )
                        | (Some("other"), Some("than"))
                        | (Some("controller"), Some("of"))
                        | (Some("with" | "without"), Some("aura"))
                        | (Some("with"), Some("counter" | "toxic" | "morph" | "no" | "same"))
                        | (Some("blocked" | "controlled"), Some("by"))
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
                        && matches!(after.as_deref(), Some("power" | "toughness" | "mana" | "total")))
                    // "target creature that's one or more colors".
                    || (matches!(next.as_deref(), Some("that's"))
                        && matches!(after.as_deref(), Some("one")))
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
                Some(
                    "you" | "an" | "your" | "defending" | "with" | "without" | "or" | "that"
                        | "card" | "cards"
                )
            )
        {
            let sub = sub.clone();
            self.pos += 2;
            return Ok(Target::CreatureOfSubtype(sub));
        }
        // "card from a graveyard" / "instant card from your graveyard";
        // "card named ~ from your graveyard".
        // "Copy target spell you control." (Slick Imitator): a spell of
        // any kind with only its controller named.
        let mut f = if self.at_word("card") || self.at_words(&["spell", "you", "control"]) {
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
        // "creature card with power 2 or less from your graveyard"
        // (Slumbering Walker): the power after the noun.
        let power_after = !named
            && !each
            && f.power.is_none()
            && f.mana_value.is_none()
            && self.at_words(&["card", "with", "power"]);
        let mv_after = power_after
            || (!named
                && f.mana_value.is_none()
                && (each || self.at_words(&["card", "with", "mana", "value"])));
        if mv_after {
            if !each {
                self.pos += 1;
            }
            let g = self.filter()?;
            if power_after {
                f.power = g.power;
            } else {
                f.mana_value = g.mana_value;
                f.status.mana_value_x = g.status.mana_value_x;
                // "with mana value at most count equal power".
                f.status.bound = g.status.bound;
                if g.same == Some(SameRef::ManaValueAtMostPower) {
                    f.same = g.same;
                }
            }
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
            } else if self.at_word("their") {
                // "target player shuffles up to three target cards from
                // their graveyard": the last player target's.
                let Some(i) = self.targets.iter().rposition(|t| {
                    matches!(
                        t.base(),
                        Target::Spec(TargetSpec::Player | TargetSpec::Opponent)
                    )
                }) else {
                    return self.fail("`their graveyard` needs a player target before it");
                };
                self.pos += 1;
                f.that_player = Some(mtg_core::card::ThatPlayer::Target(
                    u8::try_from(i).unwrap_or(u8::MAX),
                ));
                f.their = true;
                Who::Any
            } else if self.eat_words(&["target", "player's"]) {
                // "any number of target artifact cards from target
                // player's graveyard": declares the player target.
                self.targets.push(Target::Spec(TargetSpec::Player));
                f.that_player = Some(mtg_core::card::ThatPlayer::Target(
                    u8::try_from(self.targets.len() - 1).unwrap_or(u8::MAX),
                ));
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
        let mut or_tapped = false;
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
                // "Insect, Rat, Spider, or Squirrel": any number of
                // one-word items before the `or`.
                let list_to_or = (1..8).any(|k| {
                    (1..=k).all(|j| self.peek_at(2 * j) == Some(&Tok::Comma))
                        && self.word_at(2 * k + 1).as_deref() == Some("or")
                });
                let alternatives =
                    !listed_type && f != Filter::default() && (next == "or" || list_to_or);
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
                "thatare" if !f.that_are => {
                    f.that_are = true;
                    self.pos += 1;
                }
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
                // "an artifact other than Dire Blunderbuss": not the
                // granter.
                "other" if self.at_words(&["other", "than", "granter"]) => {
                    f.not_granter = true;
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
                    // A spell or an activated ability has no triggering
                    // player, so it would match nothing ("each creature
                    // that player controls" after a target player is
                    // `that player 1 controls`).
                    if self.in_trigger && self.no_trigger_player && !self.in_delayed {
                        return self.fail(
                            "`that player controls` in a trigger on this object's own event: there is no triggering player; write `controller of target N controls` or `that player N controls` for the Nth target",
                        );
                    }
                    if !self.in_trigger && !self.in_delayed {
                        return self.fail(
                            "`that player controls` outside a triggered ability: there is no triggering player; write `that player N controls` for the Nth target",
                        );
                    }
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
                "this" if !matches!(self.word_at(1).as_deref(), Some("turn" | "way")) => {
                    f.relation = Relation::This;
                    self.pos += 1;
                }
                "enchanted" if self.at_words(&["enchanted", "player", "controls"]) => {
                    f.enchanted_player = true;
                    self.pos += 3;
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
                // "for each opponent, ... that player controls".
                "that" if self.per_opponent && self.at_words(&["that", "opponent", "controls"]) => {
                    f.who = Some(Who::Opponents);
                    f.per_opponent = true;
                    self.pos += 3;
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
                // `with power at most count equal creature you control`:
                // "with power less than or equal to the number of creatures
                // you control"; `less than` is strict, `count where` prints
                // "X or less, where X is ...".
                // "with mana value [, power, or toughness] equal to the
                // chosen number".
                "with"
                    if f.status.equal_chosen.is_none()
                        && self
                            .at_words(&["with", "mana", "value", "equal", "chosen", "number"]) =>
                {
                    self.pos += 6;
                    f.status.equal_chosen = Some(mtg_core::card::EqualChosen::ManaValue);
                }
                "with"
                    if f.status.equal_chosen.is_none()
                        && self.at_words(&[
                            "with",
                            "mana",
                            "value",
                            "or",
                            "power",
                            "or",
                            "toughness",
                            "equal",
                            "chosen",
                            "number",
                        ]) =>
                {
                    self.pos += 10;
                    f.status.equal_chosen =
                        Some(mtg_core::card::EqualChosen::ManaValuePowerToughness);
                }
                "with" if f.status.bound.is_none() && self.stat_bound_ahead().is_some() => {
                    let (stat, len) = self.stat_bound_ahead().expect("checked");
                    self.pos += 1 + len;
                    let exactly = self.eat_words(&["equal", "to"]);
                    // `with power at least count equal chosen number`:
                    // "with power greater than or equal to the chosen
                    // number".
                    let at_least = !exactly && self.eat_words(&["at", "least"]);
                    let strict = if exactly || at_least || self.eat_words(&["at", "most"]) {
                        false
                    } else {
                        self.expect_words(&["less", "than"])?;
                        true
                    };
                    let n = self.num()?;
                    // `with power at most X`: "with power X or less", X of
                    // the spell or ability (CR 107.3).
                    let i = match n {
                        Num::Count(i) => i,
                        Num::X => u8::MAX,
                        _ => {
                            return self
                                .fail("expected a count (`count equal ...`) or X after `at most`");
                        }
                    };
                    // The count is judged where the targets may not be
                    // chosen yet ("with power less than or equal to that
                    // creature's power" isn't supported).
                    if matches!(
                        self.counts.get(usize::from(i)).map(|c| &c.expr),
                        Some(crate::ast::CountExpr::TargetStat { .. })
                    ) {
                        return self.fail("a target's stat can't bound a filter");
                    }
                    f.status.bound = Some(mtg_core::card::StatBound {
                        stat,
                        cmp: if exactly {
                            Cmp::Exactly
                        } else if at_least {
                            Cmp::AtLeast
                        } else {
                            Cmp::AtMost
                        },
                        n,
                        strict,
                    });
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
                // "with no counters on it".
                "with"
                    if f.status.counter.is_none()
                        && self.word_at(1).as_deref() == Some("no")
                        && self.word_at(2).as_deref() == Some("counters") =>
                {
                    self.pos += 3;
                    f.status.counter = Some(mtg_core::card::CounterQual::NoCounters);
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
                    // "enchanted by Auras you control" (Archon of the Wild
                    // Rose): who controls the Aura isn't checked, and "you
                    // control" would be read as the creature's controller.
                    // (`creature with aura you control` is the formatter's
                    // order for "creatures you control that are enchanted":
                    // the controller not yet given is the creature's.)
                    if f.who.is_some()
                        && (self.at_words(&["you", "control"])
                            || self.at_words(&["an", "opponent", "controls"])
                            || self.at_words(&["your", "opponents", "control"]))
                    {
                        return self.fail(
                            "`with aura <controller>`: who controls the Aura isn't supported",
                        );
                    }
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
                // In a delayed trigger about a creature (Touch of
                // Moonglove): "a creature dealt damage by that creature
                // this turn", that creature being what it is about.
                "dealt"
                    if self.in_delayed
                        && self.at_words(&[
                            "dealt", "damage", "by", "that", "creature", "this", "turn",
                        ]) =>
                {
                    f.status.damaged_by_source = true;
                    self.pos += 7;
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
                // "with toughness greater than its power".
                "with" if self.at_words(&["with", "toughness", "greater", "than", "power"]) => {
                    f.status.toughness_over_power = true;
                    self.pos += 5;
                }
                "with"
                    if matches!(self.word_at(1).as_deref(), Some("power" | "toughness"))
                        || (self.at_words(&["with", "mana", "value"])
                            && !self.at_words(&["with", "mana", "value", "at", "most"])) =>
                {
                    self.pos += 1;
                    let what = self.word()?;
                    if what == "mana" {
                        self.expect_word("value")?;
                    }
                    // "with power or toughness 1 or less": either.
                    let either = what == "power" && self.eat_words(&["or", "toughness"]);
                    let n = self.number()?;
                    let cmp = if self.eat_words(&["or", "less"]) {
                        Cmp::AtMost
                    } else if self.eat_words(&["or", "greater"]) {
                        Cmp::AtLeast
                    } else {
                        Cmp::Exactly
                    };
                    // "with mana value 4 or 5": a range, which a filter
                    // can't hold; read as 4 it would mean something else.
                    if cmp == Cmp::Exactly
                        && self.word_at(0).as_deref() == Some("or")
                        && (matches!(self.peek_at(1), Some(Tok::Num(_)))
                            || self.word_at(1).is_some_and(|w| {
                                NUMBER_WORDS.contains(&w.to_ascii_lowercase().as_str())
                            }))
                    {
                        return self.fail(format!(
                            "`with {what} {n} or ...`: a range of values is not supported"
                        ));
                    }
                    match what.as_str() {
                        "power" if either => {
                            let n = i16::try_from(n).unwrap_or(i16::MAX);
                            f.power = Some((cmp, n));
                            f.toughness = Some((cmp, n));
                            f.status.pt_either = true;
                        }
                        "power" => f.power = Some((cmp, i16::try_from(n).unwrap_or(i16::MAX))),
                        "toughness" => {
                            f.toughness = Some((cmp, i16::try_from(n).unwrap_or(i16::MAX)));
                        }
                        _ => f.mana_value = Some((cmp, u8::try_from(n).unwrap_or(u8::MAX))),
                    }
                }
                // `create count each multicolored permanent you control
                // tapped "Robot"`: `tapped` before the token is the token's
                // ("create a tapped ... token for each ..."), not the
                // filter's.
                "tapped" | "untapped" if !matches!(self.peek_at(1), Some(Tok::Str(_))) => {
                    f.tapped = Some(w == "tapped");
                    self.pos += 1;
                }
                "modified" if !f.status.modified => {
                    f.status.modified = true;
                    self.pos += 1;
                }
                // CR 903.3: "commander creatures you own", "a commander".
                "commander" if f.status.commander.is_none() => {
                    f.status.commander = Some(false);
                    self.pos += 1;
                }
                "you" if f.status.commander == Some(false) && self.at_words(&["you", "own"]) => {
                    f.status.commander = Some(true);
                    self.pos += 2;
                }
                // "target creature you own" (CR 108.3), not "cards you own
                // in exile".
                "you"
                    if self.at_words(&["you", "own"])
                        && !f.status.you_own
                        && f.who.is_none()
                        && !matches!(self.word_at(2).as_deref(), Some("in" | "exiled")) =>
                {
                    f.status.you_own = true;
                    self.pos += 2;
                }
                // CR 708.2: "face-down creature".
                "face-down" if !f.status.face_down => {
                    f.status.face_down = true;
                    self.pos += 1;
                }
                // "a spell that's both white and black": `both white or
                // black spell`.
                "both"
                    if !f.status.all_colors
                        && self
                            .word_at(1)
                            .as_deref()
                            .is_some_and(|w| color(w).is_some()) =>
                {
                    f.status.all_colors = true;
                    self.pos += 1;
                }
                // CR 701.27g: "transformed permanent".
                "transformed" if !f.status.transformed => {
                    f.status.transformed = true;
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
                    // "attacking, blocking, or tapped creature".
                    if self.at_words(&["or", "tapped"]) {
                        or_tapped = true;
                        self.pos += 2;
                    }
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
                "of" | "with"
                    if f.chosen.is_none()
                        && (w == "of"
                            || self.at_words(&["with", "that", "name"])
                            || self.at_words(&["with", "chosen", "name"])) =>
                {
                    let of = [
                        (&["of", "chosen", "color"][..], OfChosen::ChosenColor),
                        (&["of", "chosen", "type"][..], OfChosen::ChosenType),
                        (&["of", "chosen", "quality"][..], OfChosen::ChosenQuality),
                        (&["with", "that", "name"][..], OfChosen::ThatName),
                        (&["with", "chosen", "name"][..], OfChosen::ChosenName),
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
                    // "Whenever this creature becomes blocked by a creature
                    // of that color this turn" (Zombie Boa): a delayed
                    // trigger doesn't carry the color chosen as its ability
                    // resolved, so it would never fire.
                    if c == OfChosen::ThatColor && self.in_delayed {
                        return self.fail(
                            "`of that color` in a delayed trigger: the chosen color isn't kept for it",
                        );
                    }
                    self.pos += words.len();
                    f.chosen = Some(c);
                }
                // "named ~", "not named ~": the card's own name.
                "named" if self.at_words(&["named", "this"]) && f.same.is_none() => {
                    f.same = Some(SameRef::NamedThis);
                    named_before_who = f.who.is_none();
                    self.pos += 2;
                }
                // `named "Keimi"`: a token's name.
                "named"
                    if matches!(self.peek_at(1), Some(Tok::Str(_))) && f.named_card.is_none() =>
                {
                    self.pos += 1;
                    f.named_card = Some(self.string()?);
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
                "that" if self.at_words(&["that", "share", "card", "type"]) && f.same.is_none() => {
                    self.pos += 4;
                    f.same = Some(SameRef::ShareCardType);
                }
                "that"
                    if self.at_words(&["that", "share", "creature", "type"])
                        && f.same.is_none() =>
                {
                    self.pos += 4;
                    f.same = Some(SameRef::ShareCreatureType);
                }
                // "two target creatures controlled by different players".
                "controlled"
                    if self.at_words(&["controlled", "by", "different", "players"])
                        && f.same.is_none() =>
                {
                    self.pos += 4;
                    f.same = Some(SameRef::DifferentControllers);
                }
                // "two target creatures controlled by the same player",
                // "... the same opponent".
                "controlled"
                    if (self.at_words(&["controlled", "by", "the", "same", "player"])
                        || self.at_words(&["controlled", "by", "the", "same", "opponent"]))
                        && f.same.is_none() =>
                {
                    self.pos += 4;
                    if self.eat_word("opponent") {
                        f.same = Some(SameRef::SameOpponent);
                        f.who = Some(mtg_core::card::Who::Opponents);
                    } else {
                        self.pos += 1;
                        f.same = Some(SameRef::SameController);
                    }
                }
                // "another target creature with the same controller".
                "with" if self.at_words(&["with", "same", "controller"]) && f.same.is_none() => {
                    self.pos += 3;
                    f.same = Some(SameRef::WithSameController);
                }
                // "with lesser power" (mentor, CR 702.134a).
                "with" if self.at_words(&["with", "lesser", "power"]) && f.same.is_none() => {
                    self.pos += 3;
                    f.same = Some(SameRef::LesserPower);
                }
                // "with mana value less than or equal to this creature's
                // power".
                "with"
                    if self.at_words(&["with", "mana", "value", "at", "most", "this", "power"])
                        && f.same.is_none() =>
                {
                    self.pos += 7;
                    f.same = Some(SameRef::ManaValueAtMostPower);
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
                "multicolored" | "monocolored" | "colorless" | "colored" | "two-colored" => {
                    f.color_count = Some(match w.as_str() {
                        "multicolored" => ColorCount::Multicolored,
                        "monocolored" => ColorCount::Monocolored,
                        "colored" => ColorCount::Colored,
                        // "that's exactly two colors".
                        "two-colored" => ColorCount::TwoColors,
                        _ => ColorCount::Colorless,
                    });
                    self.pos += 1;
                }
                // "that's one or more colors": `colored`.
                "that's"
                    if self.at_words(&["that's", "one", "or", "more", "colors"])
                        && f.color_count.is_none() =>
                {
                    f.color_count = Some(ColorCount::Colored);
                    self.pos += 5;
                }
                // "with total power and toughness 5 or less" (Cut Down).
                "with"
                    if self.at_words(&["with", "total", "power", "and", "toughness"])
                        && f.status.pt_total.is_none() =>
                {
                    self.pos += 5;
                    let n = self.number()?;
                    let cmp = if self.eat_words(&["or", "less"]) {
                        Cmp::AtMost
                    } else if self.eat_words(&["or", "greater"]) {
                        Cmp::AtLeast
                    } else {
                        Cmp::Exactly
                    };
                    f.status.pt_total = Some((cmp, i16::try_from(n).unwrap_or(i16::MAX)));
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
                // `any number of target nonland permanent with total mana
                // value 6 or less`: the group's bound, not the filter's.
                "with" if self.word_at(1).as_deref() == Some("total") => break,
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
                            f.subtype_neg_first |=
                                f.not_types.is_empty() && !f.not_subtypes.is_empty();
                            f.not_types.push(t);
                        } else if let Some(c) = color(rest) {
                            f.not_colors.push(c);
                        } else if rest == "snow" {
                            // "nonsnow" (CR 205.4g).
                            f.not_supertypes.push(Supertype::Snow);
                        } else if rest == "attacking" && !f.status.nonattacking {
                            // "target nonattacking creature".
                            f.status.nonattacking = true;
                        } else if rest_raw == crate::ast::OUTLAW {
                            // "non-outlaw" (CR 700.12): none of the five.
                            f.not_subtypes.push(crate::ast::OUTLAW.to_string());
                        } else if rest_raw.chars().next().is_some_and(char::is_uppercase) {
                            f.not_subtypes.push(rest_raw.to_string());
                        } else {
                            break;
                        }
                        self.pos += 1;
                    } else if let Some(c) = color(&w) {
                        f.neg_before_color |= f.colors.is_none()
                            && (f.status.token == Some(false) || !f.not_supertypes.is_empty());
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
                                | "Pegasus"
                                | "Nautilus"
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
        // "attacking or blocking" is itself an alternative: with another
        // ("attacking, blocking, or tapped creature") one would be lost.
        if or_blocking && f.or.is_some() {
            return self
                .fail("a filter can't have three alternatives (`attacking or blocking or ...`)");
        }
        if or_blocking && f.or.is_none() {
            let mut alt = f.clone();
            alt.attacking = false;
            alt.blocking = true;
            // "attacking, blocking, or tapped": a third side, tapped.
            if or_tapped {
                let mut tapped = f.clone();
                tapped.attacking = false;
                tapped.tapped = Some(true);
                alt.or = Some(Box::new(tapped));
            }
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

    /// At `with power at most count ...` (or `toughness`, `mana value`,
    /// `less than`): the stat and how many words name it.
    fn stat_bound_ahead(&self) -> Option<(mtg_core::card::Stat, usize)> {
        use mtg_core::card::Stat;
        if self.word_at(0).as_deref() != Some("with") {
            return None;
        }
        let (stat, len) = match self.word_at(1).as_deref() {
            Some("power") => (Stat::Power, 1),
            Some("toughness") => (Stat::Toughness, 1),
            Some("mana") if self.word_at(2).as_deref() == Some("value") => (Stat::ManaValue, 2),
            _ => return None,
        };
        let at = 1 + len;
        let cmp_ok = (self.word_at(at).as_deref() == Some("at")
            && self.word_at(at + 1).as_deref() == Some("most"))
            || (self.word_at(at).as_deref() == Some("less")
                && self.word_at(at + 1).as_deref() == Some("than"))
            || (self.word_at(at).as_deref() == Some("equal")
                && self.word_at(at + 1).as_deref() == Some("to"))
            || (self.word_at(at).as_deref() == Some("at")
                && self.word_at(at + 1).as_deref() == Some("least"));
        let n_ok = self.word_at(at + 2).as_deref() == Some("count")
            || (self
                .word_at(at + 2)
                .is_some_and(|w| w.eq_ignore_ascii_case("x"))
                && self.word_at(at).as_deref() == Some("at"));
        (cmp_ok && n_ok).then_some((stat, len))
    }

    /// `put count where power of that target 1 +1/+1 counters on target
    /// creature`: "Put X +1/+1 counters on target creature, where X is
    /// that creature's power" names the target the counters phrase declares
    /// next (the count is judged as the effect happens, after targets are
    /// chosen).
    fn counters_on_next_target(&self, n: usize) -> bool {
        if n != self.targets.len() + 1 {
            return false;
        }
        // After the counter kind (`+1/+1`, `stun`).
        let at = 1;
        matches!(self.word_at(at).as_deref(), Some("counter" | "counters"))
            && self.word_at(at + 1).as_deref() == Some("on")
            && self.word_at(at + 2).as_deref() == Some("target")
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
        if self.at_words(&["you", "andor"]) {
            self.pos += 2;
            return Ok(Affects::YouAndOr(self.filter()?));
        }
        if self.at_words(&["opponent", "and"]) {
            self.pos += 2;
            return Ok(Affects::OpponentsAnd(self.filter()?));
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
        // `another target creature` is how the formatter writes a second
        // target (Falling Timber's kicked half).
        if self.at_word("target")
            || self.at_words(&["that", "target"])
            || self.at_words(&["another", "target"])
        {
            return match self.recipient()? {
                Recipient::Target(i) => Ok(Affects::Target(i)),
                Recipient::ThatTarget(i) => Ok(Affects::ThatTarget(i)),
                _ => self.fail("expected a target".to_string()),
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
        // In a delayed trigger, "that creature" is what it is about (the
        // trigger's own object, as `this` is for `delayed this:`).
        if self.in_delayed && self.eat_words(&["that", "creature"]) {
            return Ok(TriggerObj::This);
        }
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
        if self.eat_words(&["a", "spell", "from", "a", "graveyard"]) {
            return Ok(Some(SpellOrdinal::FromGraveyard));
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
        if self.eat_words(&["first", "instant", "or", "sorcery", "spell", "each", "turn"]) {
            return Ok(Some(SpellOrdinal::FirstInstantOrSorcery));
        }
        let w = self.word()?;
        self.expect_word("spell")?;
        let nth = match w.as_str() {
            "first" if self.eat_words(&["during", "each", "opponent's", "turn"]) => {
                return Ok(Some(SpellOrdinal::FirstOnOpponentsTurn));
            }
            "first" => SpellOrdinal::First,
            "second" => SpellOrdinal::Second,
            "third" => SpellOrdinal::Third,
            _ => {
                return self.fail(format!(
                    "expected `first`, `second`, or `third`, found `{w}`"
                ));
            }
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
            if self.at_words(&["each", "player's", "end"]) {
                self.pos += 3;
                self.eat_word("step");
                return Ok(Trigger::EachPlayersEndStep);
            }
            let whose = if self.eat_word("your") {
                // "your next upkeep" (a delayed trigger).
                self.eat_word("next");
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
            // "the end step on your next turn".
            if self.eat_words(&["on", "your", "next", "turn"]) {
                return Ok(Trigger::OnNextTurn(Box::new(Trigger::BeginningOfStep {
                    step,
                    whose: Who::You,
                })));
            }
            return Ok(Trigger::BeginningOfStep { step, whose });
        }
        let whenever = self.at_word("whenever");
        if !(self.eat_word("when") || self.eat_word("whenever")) {
            return self.unexpected("`when`, `whenever`, or `at`");
        }
        // CR 702.55b: "When the creature this card haunts dies".
        if !whenever && self.eat_words(&["haunted", "dies"]) {
            return Ok(Trigger::HauntedDies);
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
        // "When there are nine or more incarnation counters on this
        // enchantment" (Nine Lives).
        if !whenever && (self.at_words(&["you", "control"]) || self.at_words(&["there", "are"])) {
            return Ok(Trigger::State(self.static_cond()?));
        }
        if self.eat_words(&["you", "gain", "or", "lose", "life"]) {
            let your_turn = self.eat_words(&["during", "your", "turn"]);
            return Ok(Trigger::GainsOrLosesLife { your_turn });
        }
        if self.eat_words(&["you", "scry", "or", "surveil"]) {
            return Ok(Trigger::ScriesOrSurveils);
        }
        if self.eat_words(&["you", "scry"]) {
            return Ok(Trigger::Scries);
        }
        // CR 106.11a: "a player taps a land for mana", "an opponent taps
        // a Mountain for mana".
        for (words, whose) in [
            (&["a", "player", "taps"][..], Who::Any),
            (&["an", "opponent", "taps"][..], Who::Opponents),
        ] {
            if self.eat_words(words) {
                let f = self.filter()?;
                self.expect_words(&["for", "mana"])?;
                return Ok(Trigger::TapsForMana {
                    what: TriggerObj::Filter(f),
                    whose,
                });
            }
        }
        // "you tap an untapped creature an opponent controls".
        if self.eat_words(&["you", "tap"]) {
            // "you tap this creature for mana" (CR 106.11a).
            if self.eat_words(&["this", "for", "mana"]) {
                return Ok(Trigger::TapsForMana {
                    what: TriggerObj::This,
                    whose: Who::You,
                });
            }
            // "you tap one or more untapped creatures your opponents
            // control" (CR 603.2c).
            if self.eat_words(&["one", "or", "more"]) {
                return Ok(Trigger::Batch(Box::new(Trigger::YouTap(self.filter()?))));
            }
            let f = self.filter()?;
            if self.eat_words(&["for", "mana"]) {
                return Ok(Trigger::TapsForMana {
                    what: TriggerObj::Filter(f),
                    whose: Who::You,
                });
            }
            return Ok(Trigger::YouTap(f));
        }
        // "Whenever you foretell a card" (CR 702.143a).
        if self.eat_words(&["you", "foretell", "a", "card"]) {
            return Ok(Trigger::Foretells);
        }
        // CR 602.2: "you activate an ability of a creature", "you activate
        // an exhaust ability", "an opponent activates a loyalty ability".
        if self.at_words(&["you", "activate"]) || self.at_words(&["an", "opponent", "activates"]) {
            use mtg_core::card::ActivationKind;
            let whose = if self.eat_words(&["you", "activate"]) {
                Who::You
            } else {
                self.pos += 3;
                Who::Opponents
            };
            let kind = if self.eat_words(&["an", "ability", "of", "a", "creature"]) {
                ActivationKind::OfCreature
            } else if self.eat_words(&["an", "exhaust", "ability"]) {
                ActivationKind::Exhaust
            } else if self.eat_words(&["a", "loyalty", "ability"]) {
                ActivationKind::Loyalty
            } else if self.eat_words(&["a", "ninjutsu", "ability"]) {
                ActivationKind::Ninjutsu
            } else {
                return self.unexpected(
                    "`an ability of a creature`, `an exhaust ability`, `a loyalty ability`, or `a ninjutsu ability`",
                );
            };
            return Ok(Trigger::Activates { whose, kind });
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
        // CR 603.2c: "you sacrifice one or more Foods".
        if self.eat_words(&["you", "sacrifice", "one", "or", "more"]) {
            return Ok(Trigger::SacrificesBatch(self.filter()?));
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
        // CR 119.3: "an opponent loses life".
        for (words, whose) in [
            (&["an", "opponent", "loses", "life"][..], Who::Opponents),
            (&["a", "player", "loses", "life"][..], Who::Any),
            (&["you", "lose", "life"][..], Who::You),
        ] {
            if self.eat_words(words) {
                return Ok(Trigger::LosesLife { whose });
            }
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
        // CR 603.2c: "you discard one or more cards", "a player discards
        // one or more cards".
        for (words, whose) in [
            (&["you", "discard"][..], Who::You),
            (&["a", "player", "discards"][..], Who::Any),
            (&["an", "opponent", "discards"][..], Who::Opponents),
        ] {
            let n = words.len();
            if self.at_words(words)
                && (0..4).all(|k| {
                    self.word_at(n + k).as_deref() == Some(["one", "or", "more", "cards"][k])
                })
            {
                self.pos += n + 4;
                return Ok(Trigger::DiscardsBatch { whose });
            }
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
        // "When you discard this card" (CR 701.9).
        if self.eat_words(&["you", "discard", "this"]) {
            self.eat_word("card");
            return Ok(Trigger::DiscardsThis);
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
        // "you put a +1/+1 counter on another creature", "you put a
        // counter on a creature you control": the older wording.
        if self.at_words(&["you", "put", "a"])
            && (self.word_at(3).as_deref() == Some("counter")
                || matches!(self.peek_at(3), Some(Tok::Pt { .. })))
        {
            self.pos += 3;
            let kind = if self.eat_word("counter") {
                None
            } else {
                Some(self.counter_kind()?)
            };
            self.expect_word("on")?;
            let obj = if self.eat_word("this") {
                TriggerObj::This
            } else {
                TriggerObj::Filter(self.filter()?)
            };
            return Ok(Trigger::Each(Box::new(Trigger::YouPutCounters {
                obj,
                kind,
            })));
        }
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
        if self.eat_words(&[
            "an",
            "opponent",
            "gains",
            "control",
            "of",
            "a",
            "permanent",
            "from",
            "you",
        ]) {
            return Ok(Trigger::OpponentGainsControl);
        }
        if self.eat_words(&["you", "turn", "a", "permanent", "face", "up"]) {
            return Ok(Trigger::YouTurnFaceUp);
        }
        // CR 700.14: "you expend 4".
        if self.eat_words(&["you", "expend"]) {
            let n = self.number()?;
            return Ok(Trigger::Expends(u8::try_from(n).unwrap_or(u8::MAX)));
        }
        // "a +1/+1 counter is removed from this creature" (Protean Hydra);
        // `counter removed from this` for any kind; `one or more loyalty
        // counters removed from this` (Chandra, Fire Artisan).
        let removed_at = |p: &Self, k: usize| {
            matches!(p.word_at(k).as_deref(), Some("counter" | "counters"))
                && p.word_at(k + 1).as_deref() == Some("removed")
        };
        let one_or_more =
            self.at_words(&["one", "or", "more"]) && (removed_at(self, 3) || removed_at(self, 4));
        if one_or_more
            || self.at_words(&["counter", "removed", "from"])
            || (matches!(self.peek(), Some(Tok::Pt { .. })) && removed_at(self, 1))
            || (self.word_at(1).as_deref() == Some("counter") && removed_at(self, 1))
        {
            if one_or_more {
                self.pos += 3;
            }
            let kind = if self.eat_word("counter") || self.eat_word("counters") {
                CounterKind::Any
            } else {
                let k = self.counter_kind()?;
                if !self.eat_word("counter") {
                    self.eat_word("counters");
                }
                k
            };
            self.expect_words(&["removed", "from"])?;
            let obj = if self.eat_word("this") {
                TriggerObj::This
            } else {
                TriggerObj::Filter(self.filter()?)
            };
            return Ok(Trigger::CountersRemoved {
                obj,
                kind,
                one_or_more,
            });
        }
        // "the fourth plan counter is put on this enchantment".
        if self.eat_word("nth") {
            let n = self.number()?;
            let kind = self.counter_kind()?;
            self.eat_word("counter");
            self.expect_words(&["put", "on", "this"])?;
            return Ok(Trigger::NthCounterPut { kind, n });
        }
        // CR 122.1: "one or more +1/+1 counters are put on this creature".
        if self.eat_words(&["one", "or", "more", "creatures", "you", "control", "attack"]) {
            return Ok(Trigger::CreaturesYouControlAttack);
        }
        // "Whenever two or more creatures your opponents control attack".
        if let Some(&Tok::Num(n)) = self.peek()
            && self.word_at(1).as_deref() == Some("or")
        {
            let at = self.pos;
            self.pos += 1;
            if self.eat_words(&[
                "or",
                "more",
                "creatures",
                "your",
                "opponents",
                "control",
                "attack",
            ]) {
                return Ok(Trigger::OpponentCreaturesAttack(
                    u8::try_from(n).unwrap_or(u8::MAX),
                ));
            }
            self.pos = at;
        }
        // "Whenever one or more suspected creatures you control attack".
        if self.at_words(&["one", "or", "more"]) {
            let before = self.pos;
            self.pos += 3;
            if let Ok(f) = self.filter()
                && self.eat_word("attack")
            {
                return Ok(Trigger::OneOrMoreAttack(f));
            }
            self.pos = before;
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
        // "a card leaves your graveyard": once for each card.
        if self.at_words(&["a", "card", "leaves", "your", "graveyard"]) {
            self.pos += 5;
            return Ok(Trigger::Each(Box::new(Trigger::LeavesGraveyard(
                Filter::default(),
            ))));
        }
        // "one or more players discard one or more cards".
        if self.eat_words(&[
            "one", "or", "more", "players", "discard", "one", "or", "more", "cards",
        ]) {
            return Ok(Trigger::Batch(Box::new(Trigger::DiscardsBatch {
                whose: Who::Any,
            })));
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
            // "one or more creature cards are put into your graveyard from
            // anywhere", "one or more creatures you control become tapped".
            if self.eat_words(&[
                "card",
                "put",
                "into",
                "your",
                "graveyard",
                "from",
                "anywhere",
            ]) {
                return Ok(Trigger::Batch(Box::new(Trigger::PutIntoGraveyard(f))));
            }
            if self.eat_words(&["become", "tapped"]) {
                return Ok(Trigger::Batch(Box::new(Trigger::BecomesTapped(
                    TriggerObj::Filter(f),
                ))));
            }
            // "one or more other creatures die", "... leave the
            // battlefield" (CR 603.10a).
            if self.eat_word("die") {
                return Ok(Trigger::LeavesBatch {
                    filter: f,
                    dies: true,
                });
            }
            if self.eat_words(&["leave", "the", "battlefield", "without", "dying"]) {
                return Ok(Trigger::Batch(Box::new(Trigger::LeavesWithoutDying(
                    TriggerObj::Filter(f),
                ))));
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
            if self.eat_words(&["deal", "damage", "to", "a", "player"]) {
                return Ok(Trigger::CreaturesDealDamage(f));
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
            // "you attack with one or more Faeries", "you attack a player
            // with one or more creatures with power 4 or greater".
            let player = self.eat_words(&["a", "player"]);
            if player || self.at_words(&["with", "one", "or", "more"]) {
                self.expect_words(&["with", "one", "or", "more"])?;
                let filter = self.filter()?;
                return Ok(Trigger::AttacksWithFilter { filter, player });
            }
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
        // "Whenever a player attacks with three or more creatures".
        for (words, whose) in [
            (&["a", "player", "attacks", "with"][..], Who::Any),
            (&["an", "opponent", "attacks", "with"][..], Who::Opponents),
        ] {
            if self.at_words(words) && matches!(self.peek_at(4), Some(Tok::Num(_))) {
                self.pos += 4;
                let n = self.number()?;
                self.expect_words(&["or", "more", "creatures"])?;
                return Ok(Trigger::PlayerAttacksWith {
                    whose,
                    min: u8::try_from(n).unwrap_or(u8::MAX),
                });
            }
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
        // "a creature is put into your graveyard from the battlefield".
        if let TriggerObj::Filter(f) = &obj
            && self.eat_words(&[
                "is",
                "put",
                "into",
                "your",
                "graveyard",
                "from",
                "battlefield",
            ])
        {
            return Ok(Trigger::PutIntoGraveyardFromBattlefield(f.clone()));
        }
        // "a creature enters from your graveyard".
        if let TriggerObj::Filter(f) = &obj
            && self.eat_words(&["enters", "from", "your", "graveyard"])
        {
            return Ok(Trigger::EntersFromGraveyard(f.clone()));
        }
        // "this creature enters from a graveyard", "a permanent you
        // control enters from exile".
        if self.eat_words(&["enters", "from"]) {
            let yours = self.eat_word("your");
            let zone = self.came_from_zone()?;
            if yours && zone != mtg_core::state::Zone::Graveyard {
                return self.unexpected("`graveyard` after `your`");
            }
            return Ok(Trigger::EntersFrom { obj, zone, yours });
        }
        let first = self.trigger_verb(obj.clone(), whenever)?;
        // "this enters or dies", "this enters or attacks": one ability,
        // two events of the same object.
        if first == Trigger::Enters(TriggerObj::This) && self.eat_word("or") {
            // CR 702.55b: "When this creature enters or the creature it
            // haunts dies".
            if !whenever && self.eat_words(&["haunted", "dies"]) {
                return Ok(Trigger::Or {
                    first: Box::new(first),
                    second: Box::new(Trigger::HauntedDies),
                    whenever,
                });
            }
            let second = self.trigger_verb(obj, whenever)?;
            if !matches!(
                second,
                Trigger::Dies(TriggerObj::This)
                    | Trigger::Leaves(TriggerObj::This)
                    | Trigger::Attacks(TriggerObj::This)
                    | Trigger::TurnedFaceUp(TriggerObj::This)
                    | Trigger::BecomesTapped(TriggerObj::This)
                    | Trigger::Transforms
            ) {
                return self.fail("`this enters or` takes `dies`, `leaves the battlefield`, `attacks`, `becomes tapped`, `transforms`, or `is turned face up`");
            }
            return Ok(Trigger::Or {
                first: Box::new(first),
                second: Box::new(second),
                whenever,
            });
        }
        // "When enchanted creature becomes tapped or is dealt damage": two
        // events of the same object.
        if matches!(first, Trigger::BecomesTapped(_))
            && self.at_words(&["or", "is", "dealt", "damage"])
        {
            self.pos += 1;
            let second = self.trigger_verb(obj, whenever)?;
            return Ok(Trigger::Or {
                first: Box::new(first),
                second: Box::new(second),
                whenever,
            });
        }
        // "Whenever a Detective you control enters or is turned face up".
        if let Trigger::Enters(TriggerObj::Filter(_)) = &first
            && self.at_words(&["or", "is", "turned", "face", "up"])
        {
            self.pos += 1;
            let second = self.trigger_verb(obj, whenever)?;
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
            "dies" if self.at_words(&["or", "is", "put", "into", "exile"]) => {
                self.pos += 5;
                Ok(Trigger::DiesOrExiled(obj))
            }
            "dies" => Ok(Trigger::Dies(obj)),
            // CR 701.27: "transforms into <this face>".
            "transforms" if obj == TriggerObj::This => Ok(Trigger::Transforms),
            "explores" => Ok(Trigger::Explores(obj)),
            "saddles" if self.at_words(&["a", "Mount", "or", "crews", "a", "Vehicle"]) => {
                self.pos += 6;
                Ok(Trigger::CrewsOrSaddles(obj))
            }
            "becomes" if self.at_words(&["saddled", "first", "time", "each", "turn"]) => {
                self.pos += 5;
                Ok(Trigger::BecomesSaddled(obj))
            }
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
            "is" if self.at_words(&["returned", "to", "hand"]) => {
                self.pos += 3;
                Ok(Trigger::ReturnedToHand(obj))
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
            "is" if self.at_word("dealt") && matches!(self.peek_at(1), Some(Tok::Num(_))) => {
                self.pos += 1;
                let n = self.number()?;
                self.expect_words(&["or", "more", "damage"])?;
                Ok(Trigger::DealtDamageAtLeast(obj, n))
            }
            "is" => {
                self.expect_words(&["put", "into", "a", "graveyard"])?;
                if obj == TriggerObj::This && self.eat_words(&["from", "anywhere"]) {
                    return Ok(Trigger::ThisToGraveyard);
                }
                let from_battlefield = self.eat_words(&["from", "the", "battlefield"]);
                // A filter keeps the wording ("Whenever another creature
                // or artifact you control is put into a graveyard ...").
                if matches!(obj, TriggerObj::Filter(_)) {
                    return Ok(Trigger::PutInGraveyard {
                        obj,
                        from_battlefield,
                    });
                }
                if !from_battlefield {
                    return self.unexpected("`from the battlefield`");
                }
                Ok(Trigger::Dies(obj))
            }
            "leaves" if self.at_words(&["the", "battlefield", "without", "dying"]) => {
                self.pos += 4;
                Ok(Trigger::LeavesWithoutDying(obj))
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
                if self.at_words(&[
                    "combat",
                    "damage",
                    "to",
                    "a",
                    "player",
                    "or",
                    "planeswalker",
                ]) {
                    self.pos += 7;
                    return Ok(Trigger::DealsCombatDamageToPlayerOrPlaneswalker(obj));
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
                // "the target of a spell", "... of a spell or ability an
                // opponent controls" (a filter's, or a spell's only).
                if self.at_words(&["by", "spell"])
                    || self.at_words(&["by", "opponent", "spell"])
                    || (obj != TriggerObj::This && self.at_words(&["by", "opponent"]))
                {
                    self.pos += 1;
                    let whose = if self.eat_word("opponent") {
                        Who::Opponents
                    } else {
                        Who::Any
                    };
                    let spells = self.eat_word("spell");
                    return Ok(Trigger::TargetedBy {
                        obj,
                        spells,
                        whose,
                        whenever,
                    });
                }
                if obj == TriggerObj::This && self.eat_words(&["by", "opponent"]) {
                    return Ok(Trigger::TargetedByOpponent);
                }
                if obj == TriggerObj::This
                    && self.eat_words(&["by", "you", "first", "time", "each", "turn"])
                {
                    return Ok(Trigger::TargetedByYouFirstTime);
                }
                if obj == TriggerObj::This && self.eat_words(&["first", "time", "each", "turn"]) {
                    return Ok(Trigger::TargetedFirstTime);
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
        if self.eat_words(&["it", "was", "blocking"]) {
            return Ok(Condition::WasBlocking(true));
        }
        if self.eat_words(&["it", "wasn't", "blocking"]) {
            return Ok(Condition::WasBlocking(false));
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
        // Keyword triggers' conditions (training, dethrone, melee), judged
        // as the creature attacks.
        if self.eat_word("trains") {
            return Ok(Condition::Trains);
        }
        if self.eat_word("blitzed") {
            return Ok(Condition::Blitzed);
        }
        // CR 702.104b: "if tribute wasn't paid".
        if self.eat_words(&["tribute", "not", "paid"]) {
            return Ok(Condition::TributeNotPaid);
        }
        if self.eat_word("dethrones") {
            return Ok(Condition::Dethrones);
        }
        if self.eat_words(&["attacked", "opponent"]) {
            return Ok(Condition::AttackedOpponent);
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
        // "if it had no counters on it": of any kind.
        if self.eat_words(&["it", "had", "no", "counters"]) {
            return Ok(Condition::HadNoCounters(CounterKind::Any));
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
                        } else if s == "Q" {
                            // CR 107.6: the untap symbol.
                            cost.untap = true;
                            if saw_mana {
                                cost.order.push(CostItem::Mana);
                                saw_mana = false;
                            }
                            cost.order.push(CostItem::Untap);
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
                    // `sacrifice any number creature`, `may sacrifice any
                    // number Spirit` (a spell's additional cost, CR 601.2b).
                    "sacrifice" | "may"
                        if (self.word_at(0).as_deref() == Some("sacrifice")
                            && self.word_at(1).as_deref() == Some("any"))
                            || (self.word_at(1).as_deref() == Some("sacrifice")
                                && self.word_at(2).as_deref() == Some("any")) =>
                    {
                        cost.sacrifice_any = if self.eat_word("may") { 2 } else { 1 };
                        self.pos += 1;
                        self.expect_words(&["any", "number"])?;
                        cost.sacrifice = Some(SacCost::Filter(self.filter()?));
                        cost.order.push(CostItem::Sacrifice);
                    }
                    "sacrifice" => {
                        self.pos += 1;
                        if self.eat_word("this") {
                            cost.sacrifice = Some(SacCost::This);
                        } else if self.eat_word("granter") {
                            cost.sacrifice = Some(SacCost::Granter);
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
                        // `remove X ki counters from this` (X the ability's).
                        let n = if self.eat_word("x") {
                            cost.remove_x = true;
                            0
                        } else {
                            self.count()?
                        };
                        // `remove 1 counter from this`: any kind.
                        let k = if self.eat_word("counter") || self.eat_word("counters") {
                            CounterKind::Any
                        } else {
                            self.counter_kind()?
                        };
                        self.expect_words(&["from", "this"])?;
                        cost.remove_counters = Some((k, n));
                        cost.order.push(CostItem::RemoveCounters);
                    }
                    // `remove 1 +1/+1 counter from creature you control`.
                    "remove"
                        if (2..5).any(|k| {
                            self.word_at(k).as_deref() == Some("from")
                                && self.word_at(k + 1).as_deref() == Some("creature")
                                && self.word_at(k + 2).as_deref() == Some("you")
                                && self.word_at(k + 3).as_deref() == Some("control")
                        }) =>
                    {
                        self.pos += 1;
                        let n = self.count()?;
                        let k = self.counter_kind()?;
                        self.expect_words(&["from", "creature", "you", "control"])?;
                        cost.remove_counters = Some((k, n));
                        cost.remove_from_creature = true;
                        cost.order.push(CostItem::RemoveCounters);
                    }
                    // `put 1 card an opponent owns from exile into
                    // graveyard` (a processor).
                    "put"
                        if matches!(self.peek_at(1), Some(Tok::Num(_)))
                            && matches!(self.word_at(2).as_deref(), Some("card" | "cards"))
                            && self.word_at(3).as_deref() == Some("an") =>
                    {
                        self.pos += 1;
                        let n = self.count()?;
                        self.pos += 1;
                        self.expect_words(&[
                            "an",
                            "opponent",
                            "owns",
                            "from",
                            "exile",
                            "into",
                            "graveyard",
                        ])?;
                        cost.process = n;
                        cost.order.push(CostItem::Process);
                    }
                    // `put 1 -1/-1 counter on creature you control`:
                    // blight's older wording (CR 701.68a).
                    "put"
                        if matches!(self.peek_at(1), Some(Tok::Num(_)))
                            && (2..5).any(|k| {
                                self.word_at(k).as_deref() == Some("on")
                                    && self.word_at(k + 1).as_deref() == Some("creature")
                                    && self.word_at(k + 2).as_deref() == Some("you")
                                    && self.word_at(k + 3).as_deref() == Some("control")
                            }) =>
                    {
                        self.pos += 1;
                        let n = self.count()?;
                        if self.counter_kind()? != CounterKind::MinusOneMinusOne {
                            return self.fail("only -1/-1 counters on a creature you control");
                        }
                        self.expect_words(&["on", "creature", "you", "control"])?;
                        cost.blight = n;
                        cost.blight_put = true;
                        cost.order.push(CostItem::Blight);
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
                    // `tap granter`, `unattach granter`: "Tap Fishing Pole",
                    // "Unattach Leonin Bola" (the Equipment granting the
                    // ability).
                    "tap" | "unattach" if self.word_at(1).as_deref() == Some("granter") => {
                        cost.chosen = if w.eq_ignore_ascii_case("tap") {
                            mtg_core::card::ChosenCost::Tap
                        } else {
                            mtg_core::card::ChosenCost::Unattach
                        };
                        self.pos += 2;
                        cost.sacrifice = Some(SacCost::Granter);
                        cost.order.push(CostItem::Sacrifice);
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
                    "exile" if self.at_words(&["exile", "this", "from", "hand"]) => {
                        self.pos += 4;
                        cost.exile_hand = true;
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
                        // `exile blue card from hand` (CR 118.9: a pitch
                        // spell's cost): cards from the hand, chosen.
                        if !top && !x && self.eat_words(&["from", "hand"]) {
                            cost.discard = n;
                            cost.discard_filter = filter;
                            cost.discard_exile = true;
                            cost.order.push(CostItem::Discard);
                        } else {
                            self.expect_words(&["from", "graveyard"])?;
                            cost.exile_cards =
                                Some(Box::new(crate::ast::ExileCards { n, x, filter, top }));
                            cost.order.push(CostItem::ExileCards);
                            // `and this`: "... and this creature".
                            if self.eat_words(&["and", "this"]) {
                                cost.exile_this = true;
                                cost.exile_this_joined = true;
                            }
                        }
                    }
                    "discard" if self.word_at(1).as_deref() == Some("hand") => {
                        self.pos += 2;
                        cost.discard_hand = true;
                        cost.order.push(CostItem::DiscardHand);
                    }
                    // CR 701.43a: `exert this`.
                    "exert" if self.word_at(1).as_deref() == Some("this") => {
                        self.pos += 2;
                        cost.exert = true;
                        cost.order.push(CostItem::Exert);
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
                    // CR 602.1: "Any player may activate this ability."
                    "any" if self.at_words(&["any", "player"]) => {
                        self.pos += 2;
                        cost.activators = Some(Who::Any);
                    }
                    "opponents" if self.at_words(&["opponents", "only"]) => {
                        self.pos += 2;
                        cost.activators = Some(Who::Opponents);
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

                    // "pay X life" (CR 107.3: X announced as the spell is
                    // cast).
                    "pay"
                        if self.word_at(1).as_deref() == Some("x")
                            && self.word_at(2).as_deref() == Some("life") =>
                    {
                        self.pos += 3;
                        cost.life_x = true;
                        cost.order.push(CostItem::Life);
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
                            reveal: false,
                            x: false,
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
                    "reveal" if self.word_at(1).as_deref() == Some("this") => {
                        self.pos += 2;
                        cost.reveal_this = true;
                        cost.order.push(CostItem::RevealThis);
                    }
                    // `reveal Kithkin card`, `may reveal Dragon card`: "reveal
                    // a Kithkin card from your hand" (a behold from the hand
                    // only).
                    "reveal" | "may"
                        if self.word_at(0).as_deref() == Some("reveal")
                            || self.word_at(1).as_deref() == Some("reveal") =>
                    {
                        let optional = self.eat_word("may");
                        self.pos += 1;
                        // `reveal X red cards` (X the ability's).
                        let x = self.eat_word("x");
                        let filter = self.filter()?;
                        self.cards_word()?;
                        cost.behold = Some(Box::new(crate::ast::Behold {
                            filter,
                            n: u32::from(!x),
                            exile: false,
                            optional,
                            reveal: true,
                            x,
                        }));
                        cost.order.push(CostItem::Behold);
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
                        let how = if self.eat_words(&["if", "targets"]) {
                            crate::ast::ReduceBy::Targets(self.filter()?)
                        } else if self.eat_words(&["each", "color", "of", "target"]) {
                            crate::ast::ReduceBy::PerTarget(mtg_core::card::PerTarget::Colors)
                        } else if self.at_word("each")
                            && self.word_at(2).as_deref() == Some("counter")
                            && self.word_at(3).as_deref() == Some("on")
                            && self.word_at(4).as_deref() == Some("target")
                        {
                            self.pos += 1;
                            let kind = self.counter_kind()?;
                            self.eat_word("counter");
                            self.expect_words(&["on", "target"])?;
                            crate::ast::ReduceBy::PerTarget(mtg_core::card::PerTarget::Counters(
                                kind,
                            ))
                        } else if self.eat_word("each") {
                            crate::ast::ReduceBy::Each(self.count_expr()?)
                        } else if self.eat_words(&["during", "your", "turn"]) {
                            // "This ability costs {1} less to activate
                            // during your turn." (Hylda's Crown of Winter).
                            crate::ast::ReduceBy::If(StaticCond::DuringYourTurn)
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

    /// `{ <effects> }` after an effect a spell creates (CR 615.5): what
    /// happens as it applies, `that much` being the damage prevented this
    /// way ("Prevent all damage target spell would deal this turn. You gain
    /// life equal to the damage prevented this way.").
    fn created_then(&mut self) -> R<Vec<Sentence>> {
        if self.peek() != Some(&Tok::LBrace) {
            return Ok(Vec::new());
        }
        self.pos += 1;
        let s = self.sentences_inline()?;
        self.expect(&Tok::RBrace)?;
        Ok(s)
    }

    /// `and` or `for each` before a prevention's `{ ... }`: how its
    /// effects are printed (`ThenWording`).
    fn then_wording(&mut self) -> crate::ast::ThenWording {
        if self.at_word("and") && self.peek_at(1) == Some(&Tok::LBrace) {
            self.pos += 1;
            crate::ast::ThenWording::Joined
        } else if self.at_words(&["for", "each"]) && self.peek_at(2) == Some(&Tok::LBrace) {
            self.pos += 2;
            crate::ast::ThenWording::ForEachFirst
        } else {
            crate::ast::ThenWording::Sentence
        }
    }

    fn replacement(&mut self) -> R<Replacement> {
        let mut r = self.replacement_inline()?;
        r.wording = self.then_wording();
        if self.peek() == Some(&Tok::LBrace) {
            r.then = self.effects_body()?;
        } else {
            self.end_stmt()?;
        }
        Ok(r)
    }

    fn replacement_inline(&mut self) -> R<Replacement> {
        let event = self.replaces()?;
        // `while this has 1 or more +1/+1 counters` (Ugin's Conjurant).
        let cond = if self.eat_word("while") {
            Some(Box::new(self.static_cond()?))
        } else {
            None
        };
        self.expect(&Tok::Colon)?;
        // `plus "1/1 green Frog creature"`, `plus that many "..."`: tokens
        // created as well (Quina, Chatterfang).
        let mut token = None;
        let outcome = if self.at_word("plus")
            && (matches!(self.peek_at(1), Some(Tok::Str(_)))
                || self.at_words(&["plus", "that", "many"]))
        {
            self.pos += 1;
            let that_many = self.eat_words(&["that", "many"]);
            token = Some(self.string()?);
            self.eat_word("instead");
            Outcome::PlusToken {
                token: mtg_core::card::CardRef::default(),
                that_many,
            }
        } else if self.at_words(&["that", "many"]) && matches!(self.peek_at(2), Some(Tok::Str(_))) {
            // `that many "5/5 red Dragon creature with flying"`: those
            // tokens become as many of this one (Draconic Visitor).
            self.pos += 2;
            token = Some(self.string()?);
            self.eat_word("instead");
            Outcome::TokensInstead {
                token: mtg_core::card::CardRef::default(),
            }
        } else {
            self.outcome()?
        };
        Ok(Replacement {
            event,
            outcome,
            then: Vec::new(),
            token,
            wording: crate::ast::ThenWording::Sentence,
            cond,
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
        // `tokens created under your control` (CR 111.1); `by an effect`
        // for the older wording.
        if self.at_words(&["tokens", "created"])
            || self.at_words(&["creature", "tokens", "created"])
            || self.at_words(&["artifact", "tokens", "created"])
        {
            let creature = self.eat_word("creature");
            let artifact = self.eat_word("artifact");
            self.pos += 2;
            self.expect_words(&["under", "your", "control"])?;
            let effect = self.eat_words(&["by", "an", "effect"]);
            return Ok(Replaces::CreateTokens {
                creature,
                effect,
                artifact,
                you: false,
            });
        }
        // `artifact tokens you create`: "If you would create one or more
        // artifact tokens".
        if self.at_words(&["tokens", "you", "create"])
            || self.at_words(&["artifact", "tokens", "you", "create"])
        {
            let artifact = self.eat_word("artifact");
            self.pos += 3;
            return Ok(Replaces::CreateTokens {
                creature: false,
                effect: false,
                artifact,
                you: true,
            });
        }
        // `this put into a graveyard` (CR 113.6b: from anywhere).
        if self.eat_words(&["this", "put", "into", "a", "graveyard"]) {
            self.eat_words(&["from", "anywhere"]);
            return Ok(Replaces::ThisToGraveyard);
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
        // "Each Dragon you control enters with ...", "Other creatures you
        // control enter with ..." (CR 614.1c).
        if self.eat_word("each") {
            let what = self.filter()?;
            self.expect_word("enters")?;
            return Ok(Replaces::OthersEnter { what, each: true });
        }
        {
            let at = self.pos;
            if let Ok(what) = self.filter()
                && self.eat_word("enter")
            {
                return Ok(Replaces::OthersEnter { what, each: false });
            }
            self.pos = at;
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
        for (words, whose) in [
            (&["you", "lose", "life"][..], Who::You),
            (&["a", "player", "loses", "life"][..], Who::Any),
            (&["an", "opponent", "loses", "life"][..], Who::Opponents),
        ] {
            if self.eat_words(words) {
                let your_turn = self.eat_words(&["during", "your", "turn"]);
                return Ok(Replaces::LoseLife { whose, your_turn });
            }
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
        // "If an instant or sorcery card would be put into a graveyard
        // from anywhere".
        if !self.at_word("card") && !self.at_words(&["a", "card"]) {
            let start = self.pos;
            if let Ok(f) = self.filter()
                && self.eat_words(&["card", "put", "into", "a", "graveyard"])
            {
                self.eat_words(&["from", "anywhere"]);
                return Ok(Replaces::FilteredToGraveyard(f));
            }
            self.pos = start;
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
        // Counters of any kind (CR 122.6).
        if self.at_words(&["counters", "put", "on"])
            || self.at_words(&["counters", "put", "by", "effect", "on"])
            || self.at_words(&["counters", "you", "put", "on"])
        {
            self.pos += 1;
            let by = if self.eat_words(&["put", "by", "effect", "on"]) {
                mtg_core::card::CountersBy::Effect
            } else if self.eat_words(&["you", "put", "on"]) {
                mtg_core::card::CountersBy::You
            } else {
                self.expect_words(&["put", "on"])?;
                mtg_core::card::CountersBy::Any
            };
            return Ok(Replaces::AnyCounters {
                on: self.filter()?,
                by,
            });
        }
        let combat = if self.eat_word("combat") {
            Some(true)
        } else if self.eat_word("noncombat") {
            Some(false)
        } else {
            None
        };
        self.expect_word("damage")?;
        // `damage this turn to A [from S]`: "that would be dealt this turn
        // to A [by S]"; `damage to A this turn from S`: "... to A this turn
        // by S" (in an effect's `this turn, replace`; rendering only).
        let turn_first = self.at_words(&["this", "turn", "to"]);
        if turn_first {
            self.pos += 2;
        }
        let mut event = self.damage_event(combat)?;
        if let Replaces::Damage { from, wording, .. } = &mut event {
            if turn_first {
                *wording = DamageWording::TurnFirst;
            } else if from.is_none() && self.at_words(&["this", "turn", "from"]) {
                self.pos += 3;
                *from = Some(self.source()?);
                *wording = DamageWording::DealtLate;
            }
        }
        Ok(event)
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
        // `damage source would deal to you`: any source, printed "If a
        // source would deal damage to you, ..." (Nine Lives).
        if self.eat_words(&["source", "would", "deal"]) {
            wording = DamageWording::WouldDeal;
        } else if self.eat_words(&["would", "be", "dealt", "to"]) {
            // `damage would be dealt to this`: "If this creature would be
            // dealt damage, ..." (Unbreathing Horde).
            return Ok(Replaces::Damage {
                from: None,
                to: self.affects()?,
                combat,
                wording: DamageWording::WouldBeDealt,
            });
        } else if !self.at_word("to") && !self.at_word("from") && !ends(self) {
            let bare = self.eat_word("bare");
            from = Some(self.source()?);
            self.expect_words(&["would", "deal"])?;
            wording = if bare {
                DamageWording::WouldDealBare
            } else {
                DamageWording::WouldDeal
            };
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
        // `a Wizard you control`: "If a Wizard you control would deal
        // damage ..., it deals double that damage instead." (Trance Kuja),
        // not "a Wizard source you control".
        let article = self.eat_word("a");
        let mut a = self.affects()?;
        if article && let Affects::Objects(f) = &mut a {
            f.article = true;
        }
        if matches!(
            a,
            Affects::Any
                | Affects::You
                | Affects::Players(_)
                | Affects::YouAnd(_)
                | Affects::YouAndOr(_)
        ) {
            return self.fail("a damage source is an object, not a player".to_string());
        }
        Ok(a)
    }

    #[allow(clippy::too_many_lines)] // one arm per outcome
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
            // "reveal it and shuffle it into its owner's library instead".
            "shuffle" => {
                self.eat_words(&["into", "library"]);
                Outcome::ShuffleIntoLibrary
            }
            // "return it to its owner's hand instead".
            "hand" => Outcome::Instead(Zone::Hand),
            // "put it on top of its owner's library instead" (`library
            // top`), "on the bottom ..." (`library bottom`).
            "library" => {
                if self.eat_word("top") {
                    Outcome::IntoLibrary { top: true }
                } else {
                    self.expect_word("bottom")?;
                    Outcome::IntoLibrary { top: false }
                }
            }
            "plus" => {
                if self.eat_word("one") {
                    Outcome::PlusOne
                } else if self.at_word("count") {
                    // `plus count where power`: "instead it deals that much
                    // damage plus X, where X is Hawkeye's power".
                    Outcome::PlusCount(self.num()?)
                } else {
                    // `plus 2`, `plus 1 to that`: damage (Torbran, Jaya).
                    let n = self.number()?;
                    let to_that = self.eat_words(&["to", "that"]);
                    Outcome::PlusN { n, to_that }
                }
            }
            "exile" => {
                self.eat_word("it");
                Outcome::Instead(Zone::Exile)
            }
            "destroy" if self.at_words(&["next", "time"]) => {
                self.pos += 2;
                Outcome::DestroyNextTime
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
                let next = if self.eat_words(&["next", "time"]) {
                    Some(mtg_core::card::REDIRECT_NEXT_TIME)
                } else if self.eat_word("next") {
                    Some(self.number()?)
                } else {
                    None
                };
                self.expect_word("to")?;
                if next.is_none()
                    && (self.eat_word("attached")
                        || self.eat_word("enchanted")
                        || self.eat_word("equipped"))
                {
                    Outcome::RedirectToAttached
                } else if self.eat_word("you") {
                    Outcome::Redirect {
                        to: RedirectTo::You,
                        next,
                    }
                } else if self.eat_words(&[
                    "chosen",
                    "creature",
                    "or",
                    "planeswalker",
                    "you",
                    "control",
                ]) {
                    Outcome::Redirect {
                        to: RedirectTo::ChosenCreatureOrPlaneswalker,
                        next,
                    }
                } else if self.eat_word("this") {
                    Outcome::Redirect {
                        to: RedirectTo::This,
                        next,
                    }
                } else if self.at_word("target")
                    || self.at_words(&["another", "target"])
                    || self.at_words(&["any", "target"])
                {
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
            "has" | "have" if self.at_words(&["all", "activated", "abilities"]) => {
                use crate::ast::{CountExpr, GroupWord};
                self.pos += 3;
                let while_here = self.eat_words(&["as", "long", "as", "on", "battlefield"]);
                self.expect_word("of")?;
                if self.eat_words(&["the", "exiled", "card"]) {
                    return Ok(Mod::ActivatedOf {
                        from: CountExpr::ExiledWith(None),
                        word: GroupWord::The,
                        while_here,
                    });
                }
                // "... of that card": the top card of your library.
                if self.eat_words(&["that", "card"]) {
                    return Ok(Mod::ActivatedOf {
                        from: CountExpr::LibraryTop,
                        word: GroupWord::The,
                        while_here,
                    });
                }
                let word = if self.at_word("all") && !self.at_words(&["all", "graveyards"]) {
                    self.pos += 1;
                    GroupWord::All
                } else if self.at_word("each") && !self.at_words(&["each", "graveyard"]) {
                    self.pos += 1;
                    GroupWord::Each
                } else {
                    GroupWord::Bare
                };
                let from = self.count_expr()?;
                if !matches!(
                    from,
                    CountExpr::Permanents(_)
                        | CountExpr::CardsInGraveyard(_)
                        | CountExpr::CardsInGraveyards { each: false, .. }
                        | CountExpr::ExiledWith(_)
                ) {
                    return self.fail(
                        "`has all activated abilities of` takes permanents, `graveyard`, \
                         `all graveyards`, or `exiled with this`",
                    );
                }
                Ok(Mod::ActivatedOf {
                    from,
                    word,
                    while_here,
                })
            }
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
                    "chosen" if !set && self.eat_word("type") => {
                        Ok(Mod::AddChosenType { creature: false })
                    }
                    "chosen" if !set && self.eat_words(&["creature", "type"]) => {
                        Ok(Mod::AddChosenType { creature: true })
                    }
                    // `add supertype snow`: "is snow".
                    "supertype" if !set => {
                        let w = self.word()?;
                        let Some(s) = supertype(&w) else {
                            return self.fail(format!("not a supertype: {w}"));
                        };
                        Ok(Mod::AddSupertype(s))
                    }
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
/// A trigger whose event has no amount for `that much`
/// (`trigger_amount` is the damage, the counters, or a batch's size).
fn no_trigger_amount(t: &Trigger) -> bool {
    match t {
        Trigger::When(t) | Trigger::Next(t) | Trigger::Each(t) | Trigger::OnNextTurn(t) => {
            no_trigger_amount(t)
        }
        Trigger::Batch(_) => false,
        Trigger::Either(a, b) => no_trigger_amount(a) && no_trigger_amount(b),
        Trigger::CastsSpell { .. }
        | Trigger::CastsFiltered { .. }
        | Trigger::CastThis { .. }
        | Trigger::CastsNth { .. }
        | Trigger::GainsLife { .. }
        | Trigger::GainsLifeFirst { .. } => true,
        t => {
            no_trigger_object(t)
                && !matches!(
                    t,
                    Trigger::CreaturesDealCombatDamage(_)
                        | Trigger::CreaturesDealDamage(_)
                        | Trigger::LosesLife { .. }
                )
        }
    }
}

/// A trigger whose event names no object (a step, a player's life gain,
/// day and night): its ability has no triggering object, so `it` there
/// would denote nothing.
fn no_trigger_object(t: &Trigger) -> bool {
    match t {
        Trigger::When(t)
        | Trigger::Next(t)
        | Trigger::Batch(t)
        | Trigger::Each(t)
        | Trigger::OnNextTurn(t) => no_trigger_object(t),
        Trigger::Either(a, b) => no_trigger_object(a) && no_trigger_object(b),
        Trigger::State(_)
        | Trigger::EveryStep(_)
        | Trigger::EachPlayersEndStep
        | Trigger::BeginningOfStep { .. }
        | Trigger::CreaturesYouControlAttack
        | Trigger::OpponentCreaturesAttack(_)
        | Trigger::ThisToGraveyard
        | Trigger::OneOrMoreAttack(_)
        | Trigger::GainsLife { .. }
        | Trigger::GainsLifeFirst { .. }
        | Trigger::LosesLife { .. }
        | Trigger::DayNight
        | Trigger::RollsDice { .. }
        | Trigger::WinsFlip { .. }
        | Trigger::DrawsNth { .. }
        | Trigger::CollectsEvidence
        | Trigger::Forages
        | Trigger::CreaturesDealCombatDamage(_)
        | Trigger::CreaturesDealDamage(_)
        | Trigger::CommitsCrime { .. } => true,
        _ => false,
    }
}

/// `another target <spec>`: the spec with "other" (not the source, nor
/// another of the ability's targets); `None` where the spec has no such
/// form, so `another` is not silently dropped.
fn other_target(spec: Target) -> Option<Target> {
    Some(match spec {
        Target::Spec(TargetSpec::Creature) => Target::Spec(TargetSpec::OtherCreature),
        // `another target any`: "any other target".
        Target::Spec(TargetSpec::Any) => Target::Spec(TargetSpec::AnyOther),
        Target::Spec(TargetSpec::Permanent) => Target::Spec(TargetSpec::OtherPermanent),
        Target::Spec(TargetSpec::AttackingCreature) => Target::Filter(Filter {
            types: vec![CardType::Creature],
            attacking: true,
            relation: Relation::Other,
            ..Default::default()
        }),
        // "another target blocking creature".
        Target::Spec(TargetSpec::BlockingCreature) => Target::Filter(Filter {
            types: vec![CardType::Creature],
            blocking: true,
            relation: Relation::Other,
            ..Default::default()
        }),
        Target::Spec(TargetSpec::Artifact) => Target::Filter(Filter {
            types: vec![CardType::Artifact],
            relation: Relation::Other,
            ..Default::default()
        }),
        // "If this spell was kicked, destroy another target land."
        Target::Spec(TargetSpec::Land) => Target::Filter(Filter {
            types: vec![CardType::Land],
            relation: Relation::Other,
            ..Default::default()
        }),
        Target::Spec(TargetSpec::Enchantment) => Target::Filter(Filter {
            types: vec![CardType::Enchantment],
            relation: Relation::Other,
            ..Default::default()
        }),
        Target::Spec(TargetSpec::CreatureOrPlaneswalker) => Target::Filter(Filter {
            any_types: vec![CardType::Creature, CardType::Planeswalker],
            relation: Relation::Other,
            ..Default::default()
        }),
        // "up to one other target artifact or creature" (Metalhead).
        Target::Spec(TargetSpec::ArtifactOrCreature) => Target::Filter(Filter {
            any_types: vec![CardType::Artifact, CardType::Creature],
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
        _ => return None,
    })
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
                        that_amount: false,
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
