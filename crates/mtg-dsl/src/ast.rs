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
    /// A token's in-game name when its block name is a key that tells
    /// apart tokens of the same name (`token "1/1 white Soldier creature"
    /// { name "Soldier" ... }`).
    pub display_name: Option<String>,
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
    /// `kicker {R}` (CR 702.33).
    pub kicker: Option<ManaCost>,
    /// `flashback {4}{R}` or `flashback {1}{U}, pay 3 life` (CR 702.34).
    pub flashback: Option<Cost>,
    /// Filters behind `Recipient::Group`, in order of first mention.
    pub groups: Vec<Filter>,
    pub abilities: Vec<Ability>,
    /// What this card's copy effects add to what they copy (CR 707.9b).
    pub except: Option<Except>,
    /// Why the rendered text is allowed to differ from the Oracle text.
    pub mismatch: Option<String>,
    /// Variable amounts (`count ...` in a number's place), indexed by
    /// `Num::Count`.
    pub counts: Vec<Count>,
    /// `layout adventure` / `omen` / `transform` / `split`: how `faces`
    /// relate to this block (CR 709, 712, 715).
    pub layout: mtg_core::card::Layout,
    /// `face "Stomp" { ... }`: the card's other faces, each its own block
    /// of header and ability lines. A split card's two halves are both
    /// faces and the block itself has no characteristics.
    pub faces: Vec<Card>,
    /// Line of the block's opening, for error messages.
    pub line: usize,
}

impl Card {
    /// A block with this one's kind and nothing else, for a face.
    #[must_use]
    pub fn blank(&self) -> Card {
        Card {
            kind: self.kind,
            name: String::new(),
            display_name: None,
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
            line: self.line,
            layout: mtg_core::card::Layout::Normal,
            faces: Vec::new(),
            counts: Vec::new(),
        }
    }
}

/// A variable amount and how Oracle words it.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Count {
    pub expr: CountExpr,
    pub style: CountStyle,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CountStyle {
    /// `count where ...`: "X ..., where X is the number of ...".
    Where,
    /// `count equal ...`: "... equal to the number of ...".
    Equal,
    /// `count each ...`: "... for each ...".
    Each,
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)] // a syntax tree, not a hot structure
pub enum CountExpr {
    /// A filter: "creatures you control"; written with `on battlefield`
    /// when it names no controller.
    Permanents(Filter),
    Domain,
    CardsInHand,
    CardsInGraveyard(Option<Filter>),
    SourcePower,
    TargetPower(usize),
    Life,
    /// `count each 2 creature you control`: "2 life for each creature
    /// you control".
    Times(u32, Box<CountExpr>),
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
#[allow(clippy::struct_excessive_bools)] // each is one printed cost part
pub struct Cost {
    pub mana: ManaCost,
    pub tap: bool,
    pub life: u32,
    pub discard: u32,
    /// `discard a creature card`: what the discarded card must match.
    pub discard_filter: Option<Filter>,
    /// A loyalty ability's cost (CR 606).
    pub loyalty: Option<i32>,
    pub sacrifice: Option<SacCost>,
    /// "Activate only as a sorcery."
    pub sorcery: bool,
    /// "Discard this card" (cycling): the source leaves the hand as the
    /// cost is paid.
    pub discard_self: bool,
    /// Activated from the graveyard (unearth's expansion).
    pub from_graveyard: bool,
    /// `exile this from graveyard`: "Exile this card from your graveyard"
    /// (activated from the graveyard).
    pub exile_self: bool,
    /// Crew's cost (from its expansion).
    pub crew: u32,
    /// `pay 2 energy`: "Pay {E}{E}".
    pub energy: u32,
    /// `remove 1 charge counter from this`: "Remove a charge counter from
    /// this artifact".
    pub remove_counters: Option<(CounterKind, u32)>,
    /// `return land you control` / `tap untapped creature you control`:
    /// what the `sacrifice` filter's chosen permanent pays.
    pub chosen: mtg_core::card::ChosenCost,
    /// "Activate only once each turn."
    pub per_turn: u8,
    /// The order the parts were written in, which is the order printed
    /// (Oracle has no fixed one); empty means mana, tap, sacrifice,
    /// discard, life.
    pub order: Vec<CostItem>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CostItem {
    RemoveCounters,
    /// `from graveyard`: activated from the graveyard (printed nowhere;
    /// the effect says "this card from your graveyard").
    FromGraveyard,
    Energy,
    Exile,
    Mana,
    Tap,
    Sacrifice,
    Discard,
    Life,
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)] // a syntax tree, not a hot structure
pub enum SacCost {
    This,
    Filter(Filter),
}

/// What a spell targets ("that targets you or a creature").
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct Targeting {
    pub you: bool,
    pub player: bool,
    pub filter: Option<Filter>,
}

/// Which objects something applies to; see `mtg_core::card::ObjFilter`.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // each is one printed qualifier
pub struct Filter {
    pub who: Option<Who>,
    pub types: Vec<CardType>,
    pub not_types: Vec<CardType>,
    pub supertypes: Vec<Supertype>,
    pub not_supertypes: Vec<Supertype>,
    pub colors: Option<Vec<Color>>,
    /// "nonblack": shares no color with these.
    pub not_colors: Vec<Color>,
    pub subtypes: Vec<String>,
    pub not_subtypes: Vec<String>,
    /// "with flying": has every one of these.
    pub keywords: Vec<Keyword>,
    /// "without flying": has none of these.
    pub not_keywords: Vec<Keyword>,
    pub relation: Relation,
    pub enchanted: bool,
    pub no_abilities: bool,
    /// `who` is `Opponents`, written and printed "you don't control".
    pub dont_control: bool,
    /// `who` is `Opponents`, written and printed "your opponents control".
    pub your_opponents: bool,
    /// `who` is `Opponents`, written and printed "defending player
    /// controls" (two players: the attacker's opponent).
    pub defending: bool,
    /// Written `all ...`; a static ability's subject then prints "All
    /// Sliver creatures" rather than "Sliver creatures".
    pub all: bool,
    /// "artifact or enchantment": at least one of these types (written
    /// with `or`).
    pub any_types: Vec<CardType>,
    /// `with power 2 or less`, `with toughness 4 or greater`, `with mana
    /// value 3 or less`.
    pub power: Option<(mtg_core::card::Cmp, i16)>,
    pub toughness: Option<(mtg_core::card::Cmp, i16)>,
    pub mana_value: Option<(mtg_core::card::Cmp, u8)>,
    /// `multicolored`, `monocolored`, `colorless`.
    pub color_count: Option<mtg_core::card::ColorCount>,
    /// `tapped` / `untapped`, `attacking`, `blocking`.
    pub tapped: Option<bool>,
    pub attacking: bool,
    pub blocking: bool,
    /// `damaged`: "that was dealt damage this turn".
    pub damaged: bool,
    /// `creature or Vehicle`: an alternative filter.
    pub or: Option<Box<Filter>>,
    /// `that targets you or a permanent you control` (spells).
    pub targeting: Option<Box<Targeting>>,
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
#[allow(clippy::large_enum_variant)] // a syntax tree, not a hot structure
pub enum Target {
    Spec(TargetSpec),
    /// "target Assembly-Worker creature".
    CreatureOfSubtype(String),
    /// Any filter phrase: "target nonblack creature", "target creature an
    /// opponent controls".
    Filter(Filter),
    /// `target instant spell`: a spell matching the filter.
    Spell(Filter),
    /// `target card from a graveyard`, `target instant card from your
    /// graveyard`: a card in a graveyard (whose: `Any`, `You`, `Opponents`).
    GraveyardCard {
        filter: Filter,
        who: Who,
    },
    /// `up to 2 target creatures`: an optional target (CR 601.2c); the
    /// optional targets of an ability come last.
    UpTo(Box<Target>),
}

impl Target {
    /// The target without its "up to".
    pub fn base(&self) -> &Target {
        match self {
            Target::UpTo(t) => t.base(),
            t => t,
        }
    }
}

/// What a player chooses: `color [except blue]`, `creature type [except
/// Wall]`, `land type` (all five basic land types) or `Plains or Island`.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ChoiceKind {
    Color {
        except: Option<Color>,
    },
    CreatureType {
        except: Option<String>,
    },
    /// Basic land types, by the color they make; all five for "the basic
    /// land type of your choice".
    LandType(Vec<Color>),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Ability {
    /// A keyword printed among the abilities rather than first (a bridge
    /// land's "Indestructible" after "enters tapped").
    Keyword(Keyword),
    /// "{T}: Add {W} or {U}." One entry per choice.
    Mana(Vec<Mana>),
    /// "{T}, Sacrifice this artifact: Add one mana of any color."
    /// (`mana any, sacrifice this`).
    SacMana(Mana),
    Enchant(Target),
    Spell {
        targets: Vec<Target>,
        effects: Vec<Sentence>,
    },
    Modal(Vec<Mode>),
    /// `choose one or both { mode: ... mode: ... }` (CR 700.2): two modes,
    /// either or both.
    ModalBoth(Vec<Mode>),
    Activated {
        cost: Cost,
        targets: Vec<Target>,
        effects: Vec<Sentence>,
        /// Written as `equip {N}`; printed as "Equip {N}".
        equip: bool,
        /// Written as `cycling {N}`; printed as "Cycling {N}"; activated
        /// from the hand.
        cycling: bool,
    },
    Triggered {
        trigger: Trigger,
        condition: Option<TrigCond>,
        targets: Vec<Target>,
        effects: Vec<Sentence>,
    },
    Replacement(Replacement),
    Static {
        filter: Filter,
        mods: Vec<Mod>,
        /// "As long as ..." (CR 611.3a).
        condition: Option<StaticCond>,
        /// The condition prints first ("As long as seven or more cards are
        /// in your graveyard, this creature gets +1/+1.") rather than last.
        cond_first: bool,
    },
    EntersAsCopy(Filter),
    /// `as this enters choose color`: "As this land enters, choose a
    /// color." (CR 614.12). `as it enters choose color except blue` is
    /// printed on the line before: "This land enters tapped. As it
    /// enters, choose a color other than blue."
    AsEntersChoose {
        kind: ChoiceKind,
        joined: bool,
    },
    /// `can't be blocked by Wall`, `can't be blocked except by creature
    /// with flying`, `can't be blocked by more than one creature`, `can
    /// block only creature with flying`, `can't block creature with
    /// flying`.
    BlockRule(BlockRule),
    /// `spells artifact you cast cost {1} less`: "Artifact spells you
    /// cast cost {1} less to cast." (CR 601.2f); `generic` above zero is
    /// "more". `whose` is `Any` for "Noncreature spells cost {1} more".
    CostChange {
        filter: Filter,
        whose: Who,
        generic: i32,
    },
    /// `this spell costs {1} less count each artifact you control`: "This
    /// spell costs {1} less to cast for each artifact you control."
    SelfCost {
        per: u32,
        times: Num,
    },
    /// `this spell costs {3} less if it targets tapped creature`, `... {2}
    /// more if it targets Dragon`.
    TargetCost {
        delta: i32,
        filter: Filter,
    },
    /// `this spell costs {3} less if creature died this turn`.
    CondCost {
        delta: i32,
        cond: StaticCond,
    },
    /// `affinity for artifact`: "Affinity for artifacts" (CR 702.41), this
    /// spell costs {1} less for each such permanent you control.
    Affinity(Filter),
    /// `protection from artifact`, `protection from everything`:
    /// "Protection from artifacts" (CR 702.16), a quality other than a
    /// color (those are keywords).
    Protection(Filter),
    /// A keyword ability with a number or cost, printed as the keyword
    /// ("Bushido 2", "Echo {2}{R}") and compiled to what it stands for.
    Mechanic(Mechanic),
    /// `exert: effects`: "You may exert this creature as it attacks. When
    /// you do, [effects]." (CR 701.43).
    Exert(Vec<Sentence>),
    /// `enters tapped unless you control planeswalker`: "This land enters
    /// tapped unless you control a planeswalker." (CR 614.1d)
    EntersTappedUnless(StaticCond),
    /// `can't attack unless defending player controls Island`, `can't
    /// block unless ...`, `can't attack or block unless ...`.
    AttackUnless {
        cond: StaticCond,
        attack: bool,
        block: bool,
    },
    /// `hand size none`, `hand size +1`, `hand size opponents -1`: "You
    /// have no maximum hand size.", "Your maximum hand size is increased
    /// by one.", "Each opponent's maximum hand size is reduced by one."
    HandSize {
        whose: Who,
        delta: Option<i32>,
    },
    /// An ability word (CR 207.2c) printed before the ability: "Landfall —
    /// Whenever a land you control enters, ...". No rules meaning.
    Labeled {
        label: String,
        ability: Box<Ability>,
    },
}

/// An intervening "if" (CR 603.4): one of the engine's trigger conditions,
/// or a static condition ("if you control an artifact").
#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TrigCond {
    Core(Condition),
    Static(StaticCond),
}

/// Keyword abilities with a parameter (CR 702).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Mechanic {
    /// CR 702.45: blocks or becomes blocked, +N/+N until end of turn.
    Bushido(u32),
    /// CR 702.46: dies, you may return a Spirit card with mana value N or
    /// less from your graveyard to your hand.
    Soulshift(u32),
    /// CR 702.130: becomes blocked, defending player loses N life.
    Afflict(u32),
    /// CR 702.164: combat damage to a player also gives N poison counters.
    Toxic(u32),
    /// CR 702.21: becomes the target of an opponent's spell or ability,
    /// counter it unless its controller pays.
    Ward(ManaCost),
    /// CR 702.30: at your upkeep, if it came under your control since your
    /// last upkeep, sacrifice it unless you pay.
    Echo(ManaCost),
    /// CR 702.84: from the graveyard as a sorcery: return it with haste,
    /// exile it at the next end step.
    Unearth(ManaCost),
    /// CR 702.27: an optional additional cost; the spell returns to its
    /// owner's hand as it resolves.
    Buyback(ManaCost),
    /// CR 702.107: "{cost}, {T}: Put a +1/+1 counter on this creature.
    /// Activate only as a sorcery."
    Outlast(ManaCost),
    /// CR 702.122: "Tap any number of other untapped creatures you control
    /// with total power N or more: This Vehicle becomes an artifact
    /// creature until end of turn."
    Crew(u32),
    /// CR 702.74: an alternative cost; sacrificed as it enters if evoked.
    Evoke(ManaCost),
    /// CR 702.35: cast for this cost as it's discarded.
    Madness(ManaCost),
    /// CR 702.143: exiled for {2} on its owner's turn, cast later for this.
    Foretell(ManaCost),
    /// CR 702.62: `suspend 4 {1}{U}`.
    Suspend(u32, ManaCost),
    /// CR 702.42: `entwine {2}`: every mode of the "choose one" spell for
    /// this cost on top.
    Entwine(ManaCost),
    /// CR 702.138: `escape {W} exile 2`.
    Escape(ManaCost, u32),
    /// `escapes with 2 +1/+1 counters`.
    EscapesWith(u32),
    /// CR 702.43: `modular 2`.
    Modular(u32),
    /// CR 702.24: `cumulative upkeep {1}`.
    CumulativeUpkeep(ManaCost),
    /// CR 702.54: enters with N +1/+1 counters if an opponent was dealt
    /// damage this turn.
    Bloodthirst(u32),
    /// CR 702.37: cast face down for {3}; turn face up for the cost.
    Morph(ManaCost),
    /// CR 702.37b: morph, and a +1/+1 counter as it turns face up.
    Megamorph(ManaCost),
    /// CR 702.168: morph with ward {2} while face down.
    Disguise(ManaCost),
}

/// A blocking restriction on this creature.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum BlockRule {
    NotBy(Filter),
    OnlyBy(Filter),
    AtMostOne,
    BlocksOnly(Filter),
    CantBlock(Filter),
}

/// What a conditional static ability checks.
#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)] // a syntax tree, not a hot structure
pub enum StaticCond {
    /// `7 cards in your graveyard`: "seven or more cards are in your
    /// graveyard" (threshold).
    CardsInGraveyard(u32),
    /// `4 card types in your graveyard`: "there are four or more card
    /// types among cards in your graveyard" (delirium).
    CardTypesInGraveyard(u32),
    /// `you control 3 artifact`: "you control three or more artifacts";
    /// `you control Island`: "you control an Island".
    YouControl {
        filter: Filter,
        at_least: u32,
    },
    /// `your turn`: "it's your turn"; `static during your turn, ...`
    /// prints "During your turn, ..." instead.
    YourTurn,
    /// Written `static during your turn, <filter>: ...`.
    DuringYourTurn,
    /// `attacking`, `enchanted`, `equipped`: "it's attacking" (the source).
    Attacking,
    Enchanted,
    Equipped,
    /// `opponent life 10 or less`: "an opponent has 10 or less life".
    OpponentLifeAtMost(i32),
    /// `your life 30 or more`: "you have 30 or more life".
    YourLife {
        n: i32,
        at_least: bool,
    },
    /// `player life 13 or less`: "a player has 13 or less life".
    AnyPlayerLifeAtMost(i32),
    /// `defending player controls Island`: "defending player controls an
    /// Island".
    DefenderControls {
        filter: Filter,
        at_least: u32,
    },
    /// `you control no untapped land`.
    YouControlNone(Filter),
    /// `attacked this turn` (raid): "you attacked this turn".
    AttackedThisTurn,
    /// `creature died this turn` (morbid).
    CreatureDiedThisTurn,
    /// `opponent damaged this turn`: "an opponent was dealt damage this
    /// turn".
    OpponentDamagedThisTurn,
    /// Cast with escape (internal: "escapes with").
    Escaped,
    /// `hand 1 or less`: "you have one or fewer cards in hand"; `hand 0 or
    /// less`: "you have no cards in hand".
    CardsInHand {
        n: u32,
        at_most: bool,
    },
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
    /// Printed "Until end of turn, ..." with the duration first (Water
    /// Wings) rather than after each clause.
    pub eot_first: bool,
    /// How each clause after the first joins the sentence
    /// (`joins.len() == effects.len() - 1`).
    pub joins: Vec<Join>,
}

/// How clauses of one sentence connect: "A and B", "A, then B".
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Join {
    And,
    Then,
    /// `, and`: "and" with the subject printed again ("you draw a card and
    /// you lose 1 life").
    AndRepeat,
}

impl Sentence {
    pub fn new(effects: Vec<Effect>) -> Self {
        let joins = vec![Join::And; effects.len().saturating_sub(1)];
        Sentence {
            effects,
            para: false,
            eot_first: false,
            joins,
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
#[allow(clippy::large_enum_variant)]
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
    /// `target 1 dies`: "If that creature would die".
    Dies {
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
    /// `at beginning of every upkeep`: "At the beginning of each upkeep"
    /// (each player's, worded without "player's", as werewolves are).
    EveryStep(Step),
    BeginningOfStep {
        step: Step,
        whose: Who,
    },
    Attacks(TriggerObj),
    /// "Whenever a creature you control attacks alone".
    AttacksAlone(TriggerObj),
    /// `whenever this becomes blocked`.
    BecomesBlocked(TriggerObj),
    /// `whenever this blocks or becomes blocked`.
    BlocksOrBlocked(TriggerObj),
    /// `whenever this attacks or blocks`.
    AttacksOrBlocks(TriggerObj),
    /// `when this leaves the battlefield`.
    Leaves(TriggerObj),
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
    /// Ward's trigger: "becomes the target of a spell or ability an
    /// opponent controls".
    TargetedByOpponent,
    /// `when this is turned face up` (CR 708.8).
    TurnedFaceUp,
    /// `whenever this becomes targeted`: printed "Whenever" (most
    /// creatures) where `when ...` prints "When".
    BecomesTargetWhenever(TriggerObj),
    BecomesTapped(TriggerObj),
    BecomesUntapped(TriggerObj),
    /// `whenever this is dealt damage`.
    DealtDamage(TriggerObj),
    /// `whenever this deals damage to creature`, `... to player`, `... to
    /// opponent`, `... deals combat damage to creature`.
    DealsDamage {
        obj: TriggerObj,
        to: mtg_core::card::DamageTo,
        combat: bool,
    },
    /// `whenever you cast artifact spell`, `whenever a player casts
    /// multicolored spell`.
    CastsFiltered {
        whose: Who,
        filter: Filter,
    },
    /// `when this exploits a creature` (CR 702.110).
    Exploits,
    /// `whenever you draw your second card each turn`.
    DrawsNth {
        whose: Who,
        n: u8,
    },
    /// A trigger that would print "Whenever" printed "When" ("When this
    /// creature blocks, return it to its owner's hand"): written `when`.
    When(Box<Trigger>),
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TriggerObj {
    This,
    Filter(Filter),
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Mod {
    AddTypes(Vec<CardType>),
    AddLandType(Color),
    SetLandType(Color),
    AddSubtype(String),
    Control,
    AddColors(Vec<Color>),
    SetColors(Vec<Color>),
    AddKeywords(Vec<Keyword>),
    /// `has ability { <ability line> }`: "has \"{T}: Add {G}.\"" (a quoted
    /// ability, layer 6).
    AddAbility(Box<Ability>),
    /// `loses flying`.
    LosesKeywords(Vec<Keyword>),
    RemoveAllAbilities,
    SetPt {
        power: PtValue,
        toughness: PtValue,
    },
    ModifyPt {
        power: i32,
        toughness: i32,
    },
    /// `+1/+1 count each creature you control`: "gets +1/+1 for each
    /// creature you control".
    ModifyPtPer {
        power: i32,
        toughness: i32,
        count: Num,
    },
    SwitchPt,
    CdaColors(Vec<Color>),
    /// `set color chosen`: "is the chosen color" (chosen as the source
    /// entered).
    ChosenColor,
    /// `has protection from chosen color`.
    ProtectionFromChosen,
    CdaPt {
        count: CdaCount,
        plus_toughness: i32,
    },
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum CdaCount {
    CardTypesInGraveyards,
    CreatureCardsInGraveyards,
    Permanents(Filter),
    ControllerLife,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Recipient {
    /// The `n`th target (0-based), declared by its first mention; printed
    /// as "it" after that.
    Target(usize),
    /// Targets `first..first + count` as one subject ("up to two target
    /// creatures each get +1/+0").
    Targets(usize, usize),
    /// `controller of that target 1`: "that creature's controller".
    ThatControllerOf(usize),
    /// The `n`th target again, printed as "that creature" (Act of
    /// Treason's "Untap that creature").
    ThatTarget(usize),
    /// `its controller`: the controller of the `n`th target (the last one
    /// declared when written).
    ControllerOf(usize),
    /// `its owner`.
    OwnerOf(usize),
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
    /// `each <filter>` / `all <filter>`: the `n`th of the card's `groups`.
    Group(u16),
    /// `enchanted creature`, `equipped creature`: what the source is
    /// attached to.
    Attached,
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)]
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
    /// `this gets +1/+1 count each creature you control`.
    PumpVar {
        power: i32,
        toughness: i32,
        times: Num,
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
    /// `who` is `None` for the imperative "Mill two cards" (the controller).
    Mill {
        count: Num,
        who: Option<Recipient>,
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
        what: Vec<Recipient>,
    },
    ReturnToBattlefield {
        what: Recipient,
        counters: Option<(CounterKind, u32)>,
    },
    Regenerate {
        to: Recipient,
    },
    Exile {
        what: Vec<Recipient>,
    },
    ExileAllGraveyards,
    Sacrifice {
        what: Recipient,
    },
    Tap {
        to: Vec<Recipient>,
    },
    /// `tap or untap target creature`.
    TapOrUntap {
        to: Recipient,
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
    Counter {
        to: Recipient,
    },
    CounterUnlessPays {
        to: Recipient,
        mana: ManaCost,
    },
    Scry(Num),
    /// "Investigate" (CR 701.16): create a Clue token.
    Investigate,
    /// `put target creature on top of library` / `on bottom of library`.
    PutInLibrary {
        what: Recipient,
        top: bool,
    },
    /// `each opponent sacrifices 1 creature`: "Each opponent sacrifices a
    /// creature of their choice."
    SacrificeChosen {
        who: Recipient,
        count: Num,
        filter: Filter,
    },
    /// `return 1 land you control`: "return a land you control to its
    /// owner's hand" (the controller chooses).
    ReturnChosen {
        count: Num,
        filter: Filter,
    },
    /// `target creature loses flying`: until end of turn.
    Strip {
        keywords: Vec<Keyword>,
        to: Recipient,
    },
    /// `target player reveals hand`.
    RevealHand {
        who: Recipient,
    },
    /// `look at hand of target player`: "Look at target player's hand."
    /// (Information only: compiled as a reveal.)
    LookAtHand {
        who: Recipient,
    },
    /// `target player discards 1 card at random`.
    DiscardRandom {
        count: Num,
        who: Recipient,
    },
    /// `target opponent reveals hand discarding nonland card`: "Target
    /// opponent reveals their hand. You choose a nonland card from it.
    /// That player discards that card."
    DiscardChosen {
        who: Recipient,
        filter: Option<Filter>,
    },
    /// `transform this`: "transform this creature" (CR 701.27).
    Transform {
        what: Recipient,
    },
    /// `search basic land card to hand`, `search up to 2 basic land card
    /// to battlefield tapped`, `... to top`: "Search your library for a
    /// basic land card, reveal it, put it into your hand, then shuffle"
    /// (CR 701.19). `reveal` prints "reveal it"; `that card` names the
    /// card that way instead of "it".
    Search {
        filter: Filter,
        count: Num,
        up_to: bool,
        to: mtg_core::card::SearchTo,
        reveal: bool,
        that_card: bool,
    },
    /// `add {B}{B}`, `add any`: "Add {B}{B}.", "Add one mana of any
    /// color." (CR 106.4).
    AddMana(Vec<Mana>),
    /// `add {R} or {W}`: "Add {R} or {W}." (one, chosen).
    AddManaOneOf(Vec<Mana>),
    /// `add {G} count each Elf you control`: "Add {G} for each Elf you
    /// control."
    AddManaTimes {
        mana: Mana,
        count: Num,
    },
    Surveil(Num),
    /// `who` is `None` for the imperative "discard a card".
    Discard {
        count: Num,
        who: Option<Recipient>,
    },
    Fight {
        a: Recipient,
        b: Recipient,
    },
    /// `<what> bites <what>`: "deals damage equal to its power to"; `bites
    /// itself` when `to == source`.
    Bite {
        source: Recipient,
        to: Recipient,
    },
    Animate {
        to: Recipient,
        types: Vec<CardType>,
        subtypes: Vec<String>,
        power: i32,
        toughness: i32,
        /// Colors it becomes, if any ("4/4 green Bear artifact creature").
        colors: Vec<Color>,
    },
    /// `this becomes blue`, `becomes colorless` (until end of turn).
    BecomeColor {
        to: Recipient,
        colors: Vec<Color>,
    },
    /// `target land becomes Island` (until end of turn).
    BecomeLandType {
        to: Recipient,
        land: Color,
    },
    /// `target creature becomes color of choice`, `... gains protection
    /// from color of choice`, `target land becomes land type of choice`,
    /// `target land becomes Plains or Island`, `enchanted creature becomes
    /// creature type of choice [except Wall]` (until end of turn): chosen
    /// as the effect resolves.
    OfChoice {
        to: Recipient,
        kind: ChoiceKind,
        protection: bool,
    },
    YouMay(Vec<Sentence>),
    /// "If this spell was kicked, ..." (CR 702.33c).
    IfKicked(Vec<Sentence>),
    /// `if you control Treefolk, draw 1 card`: a static condition,
    /// judged as it resolves.
    IfCondition(StaticCond, Vec<Sentence>),
    /// `pay {2}`, `pay 2 life` (inside `you may`).
    Pay {
        mana: ManaCost,
        life: u32,
    },
    /// `if you do, ...`: only if the optional action before happened.
    IfYouDo(Vec<Sentence>),
    /// `<clause> unless you pay {2}`, `... unless they pay {2}`: the payer
    /// may pay to stop it.
    UnlessPays {
        payer: Recipient,
        mana: ManaCost,
        life: u32,
        effect: Box<Effect>,
    },
    /// "<what> has base power and toughness 4/4 until end of turn".
    SetBasePt {
        to: Recipient,
        power: i32,
        toughness: i32,
    },
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
    /// `proliferate` (CR 701.27).
    Proliferate,
    /// `this explores` (CR 701.44).
    Explore {
        what: Recipient,
    },
    /// `amass Zombies 2` (CR 701.47): the token is "0/0 black <kind> Army
    /// creature".
    Amass {
        kind: String,
        count: Num,
    },
    /// Echo's bookkeeping (from its expansion).
    ClearEcho,
    /// `manifest`: "manifest the top card of your library" (CR 701.34).
    Manifest,
    /// `adapt 2` (CR 701.46).
    Adapt(u32),
    /// `discover 4` (CR 701.57).
    Discover(u32),
    /// Modular's "put its +1/+1 counters on [recipient]" (internal).
    MoveSourceCounters {
        to: Recipient,
    },
    /// Cumulative upkeep's payment (internal).
    CumulativeUpkeep(ManaCost),
    /// `this connives` (CR 701.50).
    Connive {
        what: Recipient,
    },
    /// `dig 4 take creature card may revealing rest bottom`: "Look at the
    /// top four cards of your library. You may reveal a creature card from
    /// among them and put it into your hand. Put the rest on the bottom of
    /// your library in any order." `dig 3 reveal take all land card rest
    /// graveyard`: "Reveal the top three cards ... Put all land cards
    /// revealed this way into your hand and the rest into your
    /// graveyard." `take card` (no filter) is "Put one of them into your
    /// hand".
    Dig {
        count: Num,
        filter: Option<Filter>,
        optional: bool,
        all: bool,
        rest_graveyard: bool,
        /// "Reveal the top N cards" rather than "Look at".
        reveal_top: bool,
        /// "You may reveal a creature card from among them and put it".
        revealing: bool,
    },
    /// `look at top 3`: "look at the top three cards of your library, then
    /// put them back in any order".
    LookAtTop(Num),
    /// `choose one { mode: ... }` as a triggered ability's effect: "When
    /// this creature enters, choose one —" (CR 603.3c).
    Modes(Vec<Mode>),
    /// `target 1 doesn't untap next untap step`: "It doesn't untap during
    /// its controller's next untap step."
    SkipNextUntap {
        to: Recipient,
    },
    /// `<what> gains ability { <ability line> }`: "gains \"...\" until end of
    /// turn".
    GrantAbility {
        ability: Box<Ability>,
        to: Recipient,
    },
    /// `switch pt of target creature`: "Switch target creature's power
    /// and toughness until end of turn."
    SwitchPt {
        to: Recipient,
    },
    /// `put land card from hand onto battlefield [tapped]`.
    PutFromHand {
        filter: Filter,
        tapped: bool,
    },
    /// `each opponent gets 1 poison counter`.
    AddPoison {
        count: Num,
        who: Recipient,
    },
    /// `you get 2 energy`: "you get {E}{E}".
    AddEnergy {
        count: Num,
        who: Recipient,
    },
    /// `pay 2 energy`, inside `may`.
    PayEnergy(u32),
}
