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

/// What restricted mana may be spent on (CR 106.6); see
/// `mtg_core::card::ManaRestriction`.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct ManaRestriction {
    pub spells: Option<Filter>,
    pub abilities: bool,
    pub wording: mtg_core::card::SpendWording,
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
    /// `count that many ...`: "draw that many cards".
    ThatMany,
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
    /// `power of target 1` ("its power"), `toughness of that target 1`
    /// ("that creature's toughness").
    TargetStat {
        n: usize,
        toughness: bool,
        that: bool,
    },
    Life,
    /// `count each 2 creature you control`: "2 life for each creature
    /// you control".
    Times(u32, Box<CountExpr>),
    /// `all graveyards [<filter>]` / `each graveyard [<filter>]`: cards in
    /// all graveyards, "in all graveyards" or "in each graveyard".
    CardsInGraveyards {
        filter: Option<Filter>,
        each: bool,
    },
    /// `plus 2 <expr>`: "2 plus the number of ..."; `plus one <expr>`
    /// spells the number ("one plus ...").
    Plus {
        n: u32,
        word: bool,
        inner: Box<CountExpr>,
    },
    /// `enchantment destroyed this way`, `discarded this way`: what the
    /// resolution did (CR 608.2c).
    ThisWay {
        verb: mtg_core::card::ThisWay,
        filter: Option<Filter>,
    },
    /// `life lost this way`: "the life lost this way".
    LifeLostThisWay,
}

/// A search's count meaning "any number of cards" (`search any number`).
pub const ANY_NUMBER: u32 = u32::MAX;

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
    /// `discard X cards` (a spell's additional cost): "Discard X cards",
    /// X announced as the spell is cast (CR 601.2b).
    pub discard_x: bool,
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
    /// `if <condition>`, `during your upkeep`: "Activate only if ..."
    /// (CR 602.5b).
    pub conditions: Vec<StaticCond>,
    /// `once ever`: "Activate only once." (for as long as the source is the
    /// same object).
    pub once_ever: bool,
    /// The restrictions (`sorcery`, `once`, conditions) in the order
    /// written; printed as one sentence in that order. Empty: sorcery,
    /// then once.
    pub restrict: Vec<Restrict>,
    /// `boast {1}{W}: ...`, `exhaust`, `power-up`: the keyword printed
    /// before the cost ("Boast — {1}{W}: ..."), which adds its rules.
    pub keyword: Option<mtg_core::card::ActivatedKeyword>,
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
    /// `of chosen type`, `of that type`, `of color of choice`: "of the
    /// chosen type" and kin (`ObjFilter::chosen`).
    pub chosen: Option<OfChosen>,
    /// `named this`, `not named this`, `with same name as target 1`,
    /// `that shares color with target 1` (`ObjFilter::same`).
    pub same: Option<SameRef>,
    /// `named this` written before the controller or place ("creatures
    /// named ~ on the battlefield", "creature named ~ you control") rather
    /// than after it ("creature you control named ~").
    pub named_first: bool,
}

/// A filter's relation to the source or a target.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SameRef {
    /// `named this`: "named ~" (the card's own name, CR 201.4b).
    NamedThis,
    /// `not named this`: "not named ~".
    NotNamedThis,
    /// `with same name as target 1`: "with the same name as that creature".
    SameNameAs(usize),
    /// `that shares color with target 1`: "that shares a color with it".
    SharesColorWith(usize),
}

/// How a filter refers to a chosen color or type, as printed after the
/// rest of the filter.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum OfChosen {
    /// `of chosen color`: "of the chosen color".
    ChosenColor,
    /// `of chosen type`: "of the chosen type".
    ChosenType,
    /// `of that type`: "of that type" (after "Choose a creature type.").
    ThatType,
    /// `of that color`: "of that color".
    ThatColor,
    /// `of color of choice`: "of the color of your choice", chosen as the
    /// effect resolves.
    ColorOfChoice,
    /// `of creature type of choice`: "of the creature type of your
    /// choice".
    CreatureTypeOfChoice,
}

impl OfChosen {
    /// What an implied choice ("of the color of your choice") asks for.
    pub fn implied(self) -> Option<ChoiceKind> {
        match self {
            OfChosen::ColorOfChoice => Some(ChoiceKind::Color { except: None }),
            OfChosen::CreatureTypeOfChoice => Some(ChoiceKind::CreatureType { except: None }),
            _ => None,
        }
    }
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
    /// `X target creatures`, `1 to 2 target creatures`, `any number of
    /// target creatures`: one target word naming a variable number of
    /// objects (CR 601.2c), at least `min` and at most `max` (any number
    /// if `None`); the last of an ability's targets.
    Many {
        target: Box<Target>,
        min: Num,
        max: Option<Num>,
    },
}

impl Target {
    /// The target without its "up to" or count.
    pub fn base(&self) -> &Target {
        match self {
            Target::UpTo(t) | Target::Many { target: t, .. } => t.base(),
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
    /// `colors of choice`: "the color or colors of your choice".
    Colors,
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
    /// `mana {C} only to cast instant or sorcery spell`: "{T}: Add {C}.
    /// Spend this mana only to cast an instant or sorcery spell." (CR
    /// 106.6).
    RestrictedMana {
        mana: Mana,
        restriction: ManaRestriction,
    },
    /// `spend only black mana on x`: "Spend only black mana on X."
    XManaColor(Color),
    Enchant(Target),
    Spell {
        targets: Vec<Target>,
        effects: Vec<Sentence>,
    },
    Modal(Vec<Mode>),
    /// `choose one or both { mode: ... mode: ... }` (CR 700.2): two modes,
    /// either or both.
    ModalBoth(Vec<Mode>),
    /// `choose two { mode: ... }` (CR 700.2): two different modes.
    ModalTwo(Vec<Mode>),
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
    /// `deck any number`: "A deck can have any number of cards named ~."
    /// (CR 100.2a). A deck-building rule with no meaning in a game.
    DeckAnyNumber,
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
    /// `kicked with 2 +1/+1 counters [and first strike]`: "If this
    /// creature was kicked, it enters with two +1/+1 counters on it [and
    /// with first strike]." (CR 702.33c, 614.1c).
    KickedWith(u32, Vec<Keyword>),
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
    /// `opponent poison 3 or more`: "an opponent has three or more poison
    /// counters" (corrupted).
    OpponentPoisonAtLeast(u32),
    /// `this has 4 or more oil counters`: "this creature has four or more
    /// oil counters on it"; `this has no arrowhead counters`: "there are
    /// no arrowhead counters on this artifact".
    SourceCounters {
        kind: CounterKind,
        n: u32,
        at_least: bool,
    },
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
    /// Cast with kicker (internal: `kicked with`).
    Kicked,
    /// `land entered this turn` (landfall): "you had a land enter the
    /// battlefield under your control this turn".
    LandEnteredThisTurn,
    /// `drew 2 cards this turn`: "you've drawn two or more cards this
    /// turn".
    CardsDrawnThisTurn(u32),
    /// `hand 1 or less`: "you have one or fewer cards in hand"; `hand 0 or
    /// less`: "you have no cards in hand".
    CardsInHand {
        n: u32,
        at_most: bool,
    },
    /// `2 instant or sorcery cards in your graveyard`: "there are two or
    /// more instant and/or sorcery cards in your graveyard"; `you have 4
    /// creature cards in your graveyard`: "you have four or more creature
    /// cards in your graveyard".
    GraveyardCards {
        filter: Filter,
        at_least: u32,
        have: bool,
    },
    /// `opponent graveyard 8 or more`: "an opponent has eight or more
    /// cards in their graveyard".
    OpponentGraveyard(u32),
    /// `5 mana values in your graveyard`: "there are five or more mana
    /// values among cards in your graveyard".
    ManaValuesInGraveyard(u32),
    /// `opponent controls Human`: "an opponent controls a Human".
    OpponentControls {
        filter: Filter,
        at_least: u32,
    },
    /// `this is untapped`, `this is Wall`, `this is creature`: "it's
    /// untapped", "it's a Wall", "this permanent is a creature".
    Is(Filter),
    /// `modified`: "it's modified" (CR 700.9).
    Modified,
    /// `attacking alone`: "it's attacking alone".
    AttackingAlone,
    /// `blocked`: "this creature is blocked".
    Blocked,
    /// `being blocked`: "at least one creature is blocking this creature".
    BeingBlocked,
    /// `attached is legendary`: "equipped creature is legendary" (or
    /// "enchanted creature").
    AttachedIs(Filter),
    /// `drew 2 this turn`: "you've drawn two or more cards this turn".
    DrawnThisTurn(u32),
    /// `cast instant or sorcery spell this turn`: "you've cast an instant
    /// or sorcery spell this turn".
    CastThisTurn(Filter),
    /// `discarded this turn`: "you've discarded a card this turn".
    DiscardedThisTurn,
    /// `more cards in hand than each opponent`.
    MoreCardsInHand,
    /// `opponent poisoned` ("an opponent is poisoned"), `opponent poison 3
    /// or more` ("an opponent has three or more poison counters").
    OpponentPoison(u32),
    /// `you own card in exile`: "you own a card in exile".
    OwnCardInExile,
    /// `total power 8 or more` (formidable): "creatures you control have
    /// total power 8 or greater".
    TotalPower(u32),
    /// `3 different powers` (coven): "you control three or more creatures
    /// with different powers".
    DifferentPowers(u32),
    /// `during your upkeep`, `during opponent upkeep`, `during combat`,
    /// `before attackers`, `during your turn before attackers`, `during
    /// declare blockers`, `during end of combat` (CR 602.5b).
    During(mtg_core::card::TurnTime),
}

/// An activation restriction, in the order written and printed ("Activate
/// only if ... and only as a sorcery").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Restrict {
    Sorcery,
    Once,
    /// `once ever`: "only once" (not each turn).
    OnceEver,
    /// `Cost::conditions[i]`.
    Cond(usize),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Mode {
    pub targets: Vec<Target>,
    pub effects: Vec<Sentence>,
    /// `mode "Smash the Chest": ...`: a named mode, "• Smash the Chest —
    /// Destroy target artifact."
    pub name: Option<String>,
}

/// What an `instead` sentence checks.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum InsteadCond {
    /// `kicked`: "this spell was kicked".
    Kicked,
    Static(StaticCond),
    /// `target 1 is Human` ("it's a Human"), `that target 1 is Human`
    /// ("that creature is a Human"): the target as it resolves.
    Target {
        n: usize,
        that: bool,
        filter: Filter,
    },
    /// `the sacrificed creature was legendary`: "If the sacrificed
    /// creature was legendary, draw three cards instead."
    ThisWay(ThisWayCond),
}

/// Where "instead" and the condition print.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum InsteadWording {
    /// `if kicked, instead ...`: "If this spell was kicked, instead ...".
    Lead,
    /// `if kicked, ... instead`: "If this spell was kicked, ... instead."
    Trail,
    /// `... instead if hand 0 or less`: "... instead if you have no cards
    /// in hand."
    After,
}

/// Effects that print as one sentence ("target player loses 1 life and you
/// gain 1 life").
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Sentence {
    pub effects: Vec<Effect>,
    /// `label "Spell mastery"`: an ability word printed before the
    /// sentence ("Spell mastery — If ...", CR 207.2c).
    pub label: Option<String>,
    /// Starts a new paragraph (a new line of Oracle text).
    pub para: bool,
    /// Printed "Until end of turn, ..." with the duration first (Water
    /// Wings) rather than after each clause.
    pub eot_first: bool,
    /// Printed "Then ..." (`then` before the sentence): "Then that player
    /// discards a card."
    pub then_first: bool,
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
    /// `,`: a list of three or more clauses, "A, B, and C" (the last `and`
    /// takes the serial comma) or "A, B, then C".
    Comma,
}

impl Sentence {
    pub fn new(effects: Vec<Effect>) -> Self {
        let joins = vec![Join::And; effects.len().saturating_sub(1)];
        Sentence {
            effects,
            label: None,
            para: false,
            eot_first: false,
            then_first: false,
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
    /// `damage [from <source>] to <affects>`; `from` is an object (`this`,
    /// a filter, a target, `creature other than target creature`).
    Damage {
        from: Option<Affects>,
        to: Affects,
        combat: Option<bool>,
        /// How "Prevent all ..." names the source (rendering only).
        wording: DamageWording,
    },
    /// `damage to and from <affects>`: "dealt to and dealt by enchanted
    /// creature".
    DamageToAndBy {
        what: Affects,
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

/// Where "Prevent all damage" names a damage source (CR 609.7).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum DamageWording {
    /// `damage from S to A this turn`: "that would be dealt to A by S this
    /// turn".
    #[default]
    Dealt,
    /// `damage to A this turn from S`: "that would be dealt to A this turn
    /// by S" (Vine Snare).
    DealtLate,
    /// `damage S would deal to A this turn`: "[that] S would deal to A this
    /// turn" (Goblin Furrier; no "that" after a target).
    WouldDeal,
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
    /// `that target 1`: the same target named again, "that creature".
    ThatTarget(usize),
    /// `creature except target creature`: "creatures other than target
    /// creature" (Terrifying Presence).
    OtherThanTarget(Filter, usize),
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
    /// `whenever this blocks a creature with flying`, `whenever this
    /// becomes blocked by a creature`, `whenever this blocks or becomes
    /// blocked by a nonblack creature`: once per blocking pair, the other
    /// creature being "that creature" (CR 509.3b, 509.3d).
    BlockPair {
        obj: TriggerObj,
        other: Box<Filter>,
        blocks: bool,
        blocked: bool,
    },
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
    /// `when you cycle this` ("When you cycle this card"), `whenever you
    /// cycle a card`, `whenever you cycle another card`, `whenever you
    /// cycle or discard a card` (CR 702.29).
    Cycles {
        this: bool,
        another: bool,
        or_discard: bool,
    },
    /// `whenever you cast your second spell each turn`, `... your first
    /// spell each turn`, `... a spell other than your first spell each
    /// turn`, `... your first spell during each opponent's turn`.
    CastsNth {
        whose: Who,
        nth: mtg_core::card::SpellOrdinal,
    },
    /// `when this enters and whenever you cast ...`: one ability, two
    /// trigger events.
    Either(Box<Trigger>, Box<Trigger>),
    /// `whenever one or more artifact you control enter`: once for the
    /// objects entering together (CR 603.2c); `that much` is how many.
    EntersBatch(Filter),
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TriggerObj {
    This,
    Filter(Filter),
    /// `this or Ally you control`: "this creature or another Ally you
    /// control" (the filter, the source included).
    ThisOr(Filter),
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Mod {
    AddTypes(Vec<CardType>),
    AddLandType(Color),
    SetLandType(Color),
    /// `set land Mountain Forest Plains`: "is a Mountain, Forest, and
    /// Plains" (it has all of them and no others).
    SetLandTypes(Vec<Color>),
    /// `set land chosen`: "is the chosen type" (the basic land type the
    /// source chose as it entered).
    ChosenLandType,
    /// `add chosen type`: "is the chosen type in addition to its other
    /// types".
    AddChosenType,
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
    /// `this`: the source, "this creature" (a spell's or a legendary
    /// permanent's name, CR 201.5) at its first mention, "it" after.
    Source,
    /// `this creature` (the noun the card uses for itself): the source
    /// named in full even after a mention ("..., you may sacrifice it. If
    /// you do, this creature deals 3 damage to that player.").
    SourceNamed,
    /// `it` in a spell or an activated ability: the source as "it" even at
    /// its first mention ("If this spell was kicked, it deals 2 damage").
    SourceIt,
    TriggeringObject,
    /// The triggering object written `it` in a triggered ability ("Whenever
    /// another creature you control attacks, it gets +0/+1").
    TriggeringIt,
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
    /// `target creature and all other creature with same name as target
    /// 1`: the `n`th target and the `g`th group, whose filter relates to
    /// that target.
    TargetAnd(usize, u16),
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Effect {
    /// "deals N damage to A and B" (one effect per recipient once compiled).
    DealDamage {
        amount: Num,
        to: Vec<Recipient>,
        /// How the source is named as the subject: `deal` is `Source`
        /// ("Shock deals", then "it deals"); `this creature deals` and
        /// `it deals` choose the form (`SourceNamed`, `SourceIt`).
        by: Recipient,
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
    /// `put 1 +1/+1 counter and 1 reach counter on target creature`:
    /// several kinds at once ("Put a +1/+1 counter, a reach counter, and a
    /// deathtouch counter on target creature"), at least two.
    AddCounterList {
        counters: Vec<(CounterKind, Num)>,
        to: Recipient,
    },
    /// `deal 3 damage divided among 1 to 3 targets`: "deals 3 damage
    /// divided as you choose among one, two, or three targets" (CR
    /// 601.2d); `to` is a variable target group.
    DealDamageDivided {
        amount: Num,
        to: Recipient,
    },
    /// `distribute 2 +1/+1 counters among 1 to 2 target creatures`.
    DistributeCounters {
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
    /// `create 2 "Soldier"`; `create 1 tapped "Powerstone"`.
    CreateToken {
        token: String,
        count: Num,
        tapped: bool,
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
        /// `discarding may ...`: "You may choose ...".
        may: bool,
        /// `look at hand of target player discarding ...`: "Look at
        /// target player's hand" rather than "reveals their hand".
        look: bool,
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
    /// `add 2 mana any combination`, `add 3 mana any combination of
    /// {R}{G}`, `add 2 mana different colors`: "Add two mana in any
    /// combination of colors.", "... of {R} and/or {G}.", "Add two mana of
    /// different colors."
    AddManaCombination {
        count: u32,
        colors: mtg_core::mana::ColorSet,
        different: bool,
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
    /// `target land becomes Island` (until end of turn); `target land
    /// becomes Forest indefinitely`, with no duration.
    BecomeLandType {
        to: Recipient,
        land: Color,
        indefinite: bool,
    },
    /// `choose creature type`, `choose color`, `choose land type`, `choose
    /// creature type except Wall`: "Choose a creature type." as the effect
    /// resolves, for `of that type` and `becomes that type` after it.
    Choose(ChoiceKind),
    /// `each land you control becomes that type`: the type chosen earlier
    /// (until end of turn).
    BecomeChosen {
        to: Recipient,
    },
    /// `each creature you control gains protection from chosen color`
    /// (until end of turn).
    ProtectionFromChosen {
        to: Recipient,
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
    /// `if target 1 was Human, ...`: "If that creature was a Human, ...".
    IfTarget(usize, Filter, Vec<Sentence>),
    /// `if you control Treefolk, draw 1 card`: a static condition,
    /// judged as it resolves.
    IfCondition(StaticCond, Vec<Sentence>),
    /// `if kicked, deal 4 damage instead`: the sentence before it happens
    /// unless the condition holds, and these effects happen instead
    /// (CR 608.2c). Always a sentence of its own, after the one it
    /// replaces; an ability word (`label "Hellbent"`) may precede it.
    Instead {
        cond: InsteadCond,
        wording: InsteadWording,
        label: Option<String>,
        effects: Vec<Sentence>,
    },
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
    /// `if creature card was exiled this way, <sentence>`, `if the
    /// sacrificed creature was Human, <sentence>`: what the resolution, or
    /// the costs paid for it, did (CR 608.2c).
    IfThisWay(ThisWayCond, Vec<Sentence>),
    /// `tap 1 other untapped Citizen you control chosen`, `exile 2
    /// creature card from your graveyard chosen`, `return up to 2 land you
    /// control chosen`: the controller chooses what to act on.
    ChooseAct {
        act: mtg_core::card::ChooseAct,
        count: Num,
        up_to: bool,
        filter: Filter,
        /// `MilledToHand` only: "from among them" rather than "from among
        /// the milled cards".
        among_them: bool,
    },
    /// `sacrifice 1 artifact chosen or discard 1 card`: one of these, as
    /// the controller chooses.
    OneOf(Vec<Effect>),
    /// `target creature gains choice of lifelink or indestructible`:
    /// "gains your choice of lifelink or indestructible until end of
    /// turn".
    GrantChoice {
        to: Recipient,
        keywords: Vec<Keyword>,
    },
    /// `discard 2 cards unless you discard creature card`, `that player
    /// discards 2 cards unless they discard land card`.
    DiscardUnless {
        who: Option<Recipient>,
        count: Num,
        filter: Filter,
    },
    /// `discard hand` ("discard your hand"), `discard all cards in hand`
    /// ("discard all the cards in your hand"), `<who> discards all Trap
    /// card` ("discards all Trap cards").
    DiscardAll {
        who: Option<Recipient>,
        filter: Option<Filter>,
        /// With no filter: "all the cards in your hand" rather than "your
        /// hand".
        all_cards: bool,
    },
}

/// `<filter> was exiled this way`, `you discarded card this way`, `the
/// sacrificed creature was Human`.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct ThisWayCond {
    pub verb: mtg_core::card::ThisWay,
    pub filter: Option<Filter>,
    /// The filter names cards ("a creature card"), not permanents.
    pub card: bool,
    /// "wasn't".
    pub negated: bool,
    pub wording: ThisWayWording,
}

/// How a `ThisWay` is written and printed: "exiled", "sacrificed", ...;
/// a card put into a hand was "returned".
pub fn this_way_word(v: mtg_core::card::ThisWay) -> &'static str {
    use mtg_core::card::ThisWay;
    match v {
        ThisWay::Exiled => "exiled",
        ThisWay::Sacrificed => "sacrificed",
        ThisWay::Destroyed => "destroyed",
        ThisWay::Discarded => "discarded",
        ThisWay::Milled => "milled",
        ThisWay::PutInHand => "returned",
    }
}

/// The `ThisWay` a word names.
pub fn this_way_verb(w: &str) -> Option<mtg_core::card::ThisWay> {
    use mtg_core::card::ThisWay;
    [
        ThisWay::Exiled,
        ThisWay::Sacrificed,
        ThisWay::Destroyed,
        ThisWay::Discarded,
        ThisWay::Milled,
        ThisWay::PutInHand,
    ]
    .into_iter()
    .find(|v| this_way_word(*v) == w)
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ThisWayWording {
    /// "If a creature card was exiled this way".
    Was,
    /// "If an artifact is destroyed this way".
    Is,
    /// "If you discarded a card this way", "If you put an artifact card
    /// into your hand this way".
    You,
    /// "If the sacrificed creature was a Human": the noun.
    The(String),
}
