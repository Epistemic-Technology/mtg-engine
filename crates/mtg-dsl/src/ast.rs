//! The card DSL's syntax tree. It mirrors `mtg_core::card` closely but keeps
//! names where the engine keeps interned ids (subtypes, tokens, emblems),
//! keeps the author's order and sentence structure for the Oracle-text
//! printer, and knows nothing about a card pool.

use mtg_core::card::{
    CardType, Condition, CounterKind, Keyword, Num, PtValue, Relation, Replacement as Outcome,
    RuleKind, Supertype, TargetSpec, Who,
};
use mtg_core::mana::{Color, Mana, ManaCost};
use mtg_core::turn::Step;

/// What a definition is.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Kind {
    Card,
    /// A token definition (CR 111): created by effects, never in a deck.
    Token,
    /// An emblem definition (CR 114).
    Emblem,
}

/// One definition: a `card`, `token`, or `emblem` block.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Card {
    pub kind: Kind,
    pub name: String,
    /// Scryfall `oracle_id` the card was authored against.
    pub oracle_id: Option<String>,
    pub cost: ManaCost,
    pub color_indicator: Option<Vec<Color>>,
    pub supertypes: Vec<Supertype>,
    pub types: Vec<CardType>,
    pub subtypes: Vec<String>,
    pub pt: Option<(i32, i32)>,
    pub loyalty: Option<u32>,
    /// In the order written, which is the order printed.
    pub keywords: Vec<Keyword>,
    /// "As an additional cost to cast this spell, ..." (CR 601.2b).
    pub additional: Option<Cost>,
    pub abilities: Vec<Ability>,
    /// What this card's copy effects add to what they copy (CR 707.9b).
    pub except: Option<Except>,
    /// Why the rendered text is allowed to differ from the Oracle text.
    pub mismatch: Option<String>,
    /// Line of the block's opening, for error messages.
    pub line: usize,
}

#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct Except {
    pub subtypes: Vec<String>,
    pub keywords: Vec<Keyword>,
    pub abilities: Vec<Ability>,
}

/// A cost (CR 118): of an activated ability or the additional cost of a
/// spell.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct Cost {
    pub mana: ManaCost,
    pub tap: bool,
    pub life: u32,
    pub discard: u32,
    /// A loyalty ability's cost (CR 606).
    pub loyalty: Option<i32>,
    pub sacrifice: Option<SacCost>,
    /// "Activate only as a sorcery."
    pub sorcery: bool,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum SacCost {
    This,
    Filter(Filter),
}

/// Which objects something applies to; see `mtg_core::card::ObjFilter`.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct Filter {
    pub who: Option<Who>,
    pub types: Vec<CardType>,
    pub not_types: Vec<CardType>,
    pub supertypes: Vec<Supertype>,
    pub not_supertypes: Vec<Supertype>,
    pub colors: Option<Vec<Color>>,
    pub subtypes: Vec<String>,
    pub not_subtypes: Vec<String>,
    pub relation: Relation,
    pub enchanted: bool,
    pub no_abilities: bool,
}

impl Filter {
    pub fn this() -> Self {
        Filter {
            relation: Relation::This,
            ..Default::default()
        }
    }

    pub fn enchanted() -> Self {
        Filter {
            enchanted: true,
            ..Default::default()
        }
    }

    pub fn creatures() -> Self {
        Filter {
            types: vec![CardType::Creature],
            ..Default::default()
        }
    }
}

/// What a spell or ability targets (one entry per "target").
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Target {
    Spec(TargetSpec),
    /// "target Assembly-Worker creature".
    CreatureOfSubtype(String),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Ability {
    /// "{T}: Add {W} or {U}." One entry per choice.
    Mana(Vec<Mana>),
    Enchant(Target),
    Spell {
        targets: Vec<Target>,
        effects: Vec<Sentence>,
    },
    Modal(Vec<Mode>),
    Activated {
        cost: Cost,
        targets: Vec<Target>,
        effects: Vec<Sentence>,
        /// Written as `equip {N}`; printed as "Equip {N}".
        equip: bool,
    },
    Triggered {
        trigger: Trigger,
        condition: Option<Condition>,
        targets: Vec<Target>,
        effects: Vec<Sentence>,
    },
    Replacement(Replacement),
    Static {
        filter: Filter,
        mods: Vec<Mod>,
    },
    EntersAsCopy(Filter),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Mode {
    pub targets: Vec<Target>,
    pub effects: Vec<Sentence>,
}

/// Effects that print as one sentence ("target player loses 1 life and you
/// gain 1 life").
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Sentence {
    pub effects: Vec<Effect>,
    /// Starts a new paragraph (a new line of Oracle text).
    pub para: bool,
}

impl Sentence {
    pub fn new(effects: Vec<Effect>) -> Self {
        Sentence {
            effects,
            para: false,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Replacement {
    pub event: Replaces,
    pub outcome: Outcome,
    pub then: Vec<Sentence>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Replaces {
    Damage {
        from: Option<Filter>,
        to: Affects,
        combat: Option<bool>,
    },
    GainLife {
        whose: Who,
    },
    Draw {
        whose: Who,
        library_empty: bool,
    },
    ToGraveyard {
        whose: Who,
        from_battlefield: bool,
    },
    PlusCounters {
        on: Filter,
    },
    Destroy {
        to: Affects,
    },
    Enters,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Affects {
    Any,
    This,
    You,
    Players(Who),
    Objects(Filter),
    YouAnd(Filter),
    Target(usize),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Trigger {
    Enters(TriggerObj),
    Dies(TriggerObj),
    BeginningOfStep {
        step: Step,
        whose: Who,
    },
    Attacks(TriggerObj),
    Blocks(TriggerObj),
    DealsCombatDamageToPlayer(TriggerObj),
    GainsLife {
        whose: Who,
    },
    CastsSpell {
        whose: Who,
        noncreature: bool,
        instant_or_sorcery: bool,
    },
    BecomesTarget(TriggerObj),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum TriggerObj {
    This,
    Filter(Filter),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Mod {
    AddTypes(Vec<CardType>),
    AddLandType(Color),
    SetLandType(Color),
    AddSubtype(String),
    Control,
    AddColors(Vec<Color>),
    SetColors(Vec<Color>),
    AddKeywords(Vec<Keyword>),
    RemoveAllAbilities,
    SetPt {
        power: PtValue,
        toughness: PtValue,
    },
    ModifyPt {
        power: i32,
        toughness: i32,
    },
    SwitchPt,
    CdaColors(Vec<Color>),
    CdaPt {
        count: CdaCount,
        plus_toughness: i32,
    },
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum CdaCount {
    CardTypesInGraveyards,
    CreatureCardsInGraveyards,
    Permanents(Filter),
    ControllerLife,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Recipient {
    /// The `n`th target (0-based), declared by its first mention.
    Target(usize),
    You,
    EachOpponent,
    Source,
    TriggeringObject,
    TriggeringPlayer,
    DefendingPlayer,
    EachCreature,
    EachPlayer,
    EachCreatureYouControl,
    EachPermanentYouControl,
    SameObject,
    CreatedObject,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Effect {
    /// "deals N damage to A and B" (one effect per recipient once compiled).
    DealDamage {
        amount: Num,
        to: Vec<Recipient>,
    },
    Pump {
        power: i32,
        toughness: i32,
        to: Recipient,
    },
    Destroy {
        to: Vec<Recipient>,
        no_regen: bool,
    },
    GainLife {
        amount: Num,
        who: Recipient,
    },
    LoseLife {
        amount: Num,
        who: Recipient,
    },
    /// `who` is `None` for the imperative "Draw a card" (the controller).
    DrawCards {
        count: Num,
        who: Option<Recipient>,
    },
    Mill {
        count: Num,
        who: Recipient,
    },
    AddCounters {
        kind: CounterKind,
        count: Num,
        to: Recipient,
    },
    RemoveCounters {
        kind: CounterKind,
        count: Num,
        to: Recipient,
    },
    RemoveAllCounters {
        to: Recipient,
    },
    ReturnToHand {
        what: Recipient,
    },
    ReturnToBattlefield {
        what: Recipient,
        counters: Option<(CounterKind, u32)>,
    },
    Regenerate {
        to: Recipient,
    },
    Exile {
        what: Recipient,
    },
    ExileAllGraveyards,
    Sacrifice {
        what: Recipient,
    },
    Tap {
        to: Vec<Recipient>,
    },
    Untap {
        to: Vec<Recipient>,
    },
    Grant {
        keywords: Vec<Keyword>,
        to: Recipient,
    },
    GainControl {
        of: Recipient,
    },
    Attach {
        to: Recipient,
    },
    CreateToken {
        token: String,
        count: Num,
    },
    CreateTokenCopy {
        of: Recipient,
        except: bool,
    },
    BecomeCopy {
        of: Recipient,
        except: bool,
    },
    CreateEmblem {
        emblem: String,
    },
    Animate {
        to: Recipient,
        types: Vec<CardType>,
        subtypes: Vec<String>,
        power: i32,
        toughness: i32,
    },
    YouMay(Vec<Sentence>),
    /// A delayed triggered ability (CR 603.7) about `about`; `trigger_first`
    /// records whether the text puts the trigger before the effects.
    Delayed {
        about: Recipient,
        trigger: Trigger,
        effects: Vec<Sentence>,
        trigger_first: bool,
    },
    /// A replacement or prevention effect that lasts until end of turn.
    CreateReplacement(Replacement),
    Rule(RuleKind),
}
