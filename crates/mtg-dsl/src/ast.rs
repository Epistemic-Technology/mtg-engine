//! The card DSL's syntax tree. It mirrors `mtg_core::card` closely but keeps
//! names where the engine keeps interned ids (subtypes, tokens, emblems),
//! keeps the author's order and sentence structure for the Oracle-text
//! printer, and knows nothing about a card pool.

use mtg_core::card::{
    CardType, Condition, CounterKind, Keyword, Num, PtValue, Relation, Replacement as Outcome,
    RuleKind, Supertype, TargetSpec, Who,
};
pub use mtg_core::card::{CounteredTo, LibraryPlace};

/// How `Effect::IfTarget` words its check.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct TargetCheck {
    pub tense: TargetTense,
    /// `also` after the comma: "it also gains lifelink".
    pub also: bool,
    /// Written after the sentence: `put 1 +1/+1 counter on target creature
    /// you control if target 1 is legendary` ("... if it's legendary").
    pub after: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum TargetTense {
    /// `if target 1 was`: "If that creature was ...".
    #[default]
    Was,
    /// `if target 1 is`: "If it's ...".
    Is,
    /// `if that target 1 is`: "If that creature is ...".
    ThatIs,
    /// `if target 1 had with mana value 3 or less`: "If it had mana value 3
    /// or less", as it last existed, like `Was`.
    Had,
}

/// How `Effect::PutCard` names the cards it moves.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CardWords {
    /// `it`: "it".
    It,
    /// `that card`: "that card".
    ThatCard,
    /// `all revealed`: "all cards revealed this way".
    AllRevealed,
    /// `those revealed`: "those cards".
    Those,
    /// `the revealed`: "the revealed cards".
    TheRevealed,
    /// `all other revealed`: "all other cards revealed this way".
    OtherRevealed,
}

/// How a library placement is worded (`Effect::PutInLibrary`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LibraryWording {
    /// `put ...`: "Put target creature on top of its owner's library",
    /// "Shuffle enchanted creature into its owner's library".
    Put,
    /// `owner puts ...`: "Target creature's owner puts it ...".
    Owner,
    /// `owner of ... puts it ...`: "The owner of target nonland permanent
    /// puts it ...".
    OwnerOf,
}
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
    /// `activate of artifact`: "activate an ability of an artifact
    /// source".
    pub ability_source: Option<Filter>,
    /// `only to foretell or cast ...`: "foretell a card from your hand".
    pub foretell: bool,
    /// `only to pay disturb or cast ...`: "pay a disturb cost".
    pub disturb: bool,
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
    /// A battle's printed defense (CR 310.4a): `defense 5`.
    pub defense: Option<u32>,
    /// In the order written, which is the order printed.
    pub keywords: Vec<Keyword>,
    /// "As an additional cost to cast this spell, ..." (CR 601.2b).
    pub additional: Option<Cost>,
    /// `additional cost or {3}`: the other of two additional costs, after
    /// "or" ("sacrifice a creature or pay {3}").
    pub additional_or: Option<Cost>,
    /// `alternative cost sacrifice Mountain`: "You may sacrifice a
    /// Mountain rather than pay this spell's mana cost." (CR 118.9).
    pub alt_cost: Option<AltCost>,
    /// `flash as long as you control Merfolk`: "This spell has flash as
    /// long as you control a Merfolk." (CR 702.8).
    pub flash_if: Option<StaticCond>,
    /// `flash with cleanup sacrifice`: "You may cast this spell as though
    /// it had flash. If you cast it any time a sorcery couldn't have been
    /// cast, the controller of the permanent it becomes sacrifices it at
    /// the beginning of the next cleanup step."
    pub flash_cleanup: bool,
    /// Derived as a spell is parsed: how many of its targets come before
    /// those an "if kicked" sentence declares (Orim's Thunder's "target
    /// creature" is chosen only if the spell is kicked, CR 601.2c).
    pub unkicked_targets: Option<usize>,
    /// `cast only during combat`, `cast only if you control creature`:
    /// "Cast this spell only during combat." (CR 601.3).
    pub cast_only: Option<StaticCond>,
    /// `kicker {R}` (CR 702.33).
    pub kicker: Option<ManaCost>,
    /// The spell's targets from this index on are declared in its `if
    /// kicked, ...` and chosen only if it is kicked (CR 601.2c); set by
    /// the parser, never written.
    pub kicker_targets: Option<u8>,
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
            defense: None,
            keywords: Vec::new(),
            additional: None,
            additional_or: None,
            alt_cost: None,
            flash_if: None,
            flash_cleanup: false,
            unkicked_targets: None,
            cast_only: None,
            kicker: None,
            kicker_targets: None,
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
    /// `deal damage equal <expr> to <what>`: "deals damage equal to ... to
    /// <what>", the amount before the recipient.
    EqualBefore,
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
    /// `votes for sprout`: the votes the ability's vote gave that choice
    /// ("for each sprout vote"), `index` its place among the choices.
    Votes {
        index: u8,
        word: String,
    },
    /// `creature blocking it [beyond first]`: "for each creature blocking
    /// it [beyond the first]".
    BlockingIt {
        beyond: u32,
    },
    /// `greatest power creature you control`: "the greatest power among
    /// creatures you control".
    GreatestPower(Filter),
    /// `differently named land you control`: "the number of differently
    /// named lands you control".
    DifferentlyNamed(Filter),
    /// `party`: "creatures in your party" (CR 700.8).
    Party,
    /// `greatest mana value`: "the greatest mana value among permanents you
    /// control".
    GreatestManaValue,
    /// `drawn this turn`: "cards you've drawn this turn".
    CardsDrawnThisTurn,
    /// `spells cast this turn` ("spells you've cast this turn"), `other
    /// spells cast this turn` ("other spells you've cast this turn").
    SpellsCastThisTurn {
        other: bool,
    },
    /// `toughness`: the source's, "its toughness".
    SourceToughness,
    /// `mana value of target 1` ("its mana value"), `mana value of that
    /// target 1` ("that creature's mana value"), `mana value of the
    /// spell target 1` ("the spell's mana value").
    TargetManaValue {
        n: usize,
        that: bool,
        the_spell: bool,
    },
    /// `mana value of that card`: "that card's mana value", the card
    /// revealed before.
    CardManaValue,
    /// `colors among permanents`: "colors among permanents you control".
    ColorsAmongPermanents,
    /// `colors spent` (converge): "colors of mana spent to cast this
    /// spell" (CR 601.2h).
    SpentColors,
    /// `treasure spent`: "mana from a Treasure spent to cast it".
    SpentTreasure,
    /// `opponents graveyards [<filter>]`: "cards in your opponents'
    /// graveyards".
    CardsInOpponentsGraveyards(Option<Filter>),
    /// `opponents attacked`: "opponents you're attacking".
    OpponentsAttacked,
    /// `attacked with this turn`: "creatures you attacked with this turn".
    AttackedWithThisTurn,
    /// `andor <expr>`: the same count, its two types joined "and/or"
    /// ("artifact and/or creature card", "Merfolk and/or Faeries").
    AndOr(Box<CountExpr>),
    /// `Zubera died this turn`: "Zubera that died this turn" (creatures
    /// put into a graveyard from the battlefield this turn).
    DiedThisTurn(Filter),
    /// `greatest power graveyard creature`: "the greatest power among
    /// creature cards in your graveyard".
    GreatestPowerInGraveyard(Filter),
    /// `toughness of it` ("its toughness"), `power of that creature`
    /// ("that creature's power"): the triggering object's, as it last
    /// existed.
    TriggerStat {
        stat: mtg_core::card::Stat,
        that: bool,
    },
    /// `power of attached`: what the source enchants or equips, "its
    /// power".
    AttachedStat(mtg_core::card::Stat),
    /// `toughness of the sacrificed creature` ("the sacrificed creature's
    /// toughness"), `mana value of the sacrificed permanent`, `toughness
    /// of that sacrificed creature` ("that creature's toughness" after
    /// "sacrifices a creature"): the object moved this way, `noun` its
    /// printed noun.
    MovedStat {
        verb: mtg_core::card::ThisWay,
        stat: mtg_core::card::Stat,
        noun: String,
        that: bool,
    },
    /// `hand of target 1`: "the number of cards in that player's hand".
    TargetHand(usize),
    /// `greatest toughness other creature you control`: "the greatest
    /// toughness among other creatures you control".
    GreatestToughness(Filter),
    /// `<expr> plus <expr>`: "the number of creatures on the battlefield
    /// plus the number of artifacts on the battlefield".
    Sum(Box<CountExpr>, Box<CountExpr>),
    /// `life you lost this turn`: "the life you've lost this turn".
    LifeLostThisTurn,
    /// `damage dealt this way`, `excess damage dealt this way`.
    DamageThisWay {
        excess: bool,
    },
    /// `card types in graveyards`: "the number of card types among cards
    /// in all graveyards".
    CardTypesInGraveyards,
    /// `+1/+1 counters on this` ("the number of +1/+1 counters on this
    /// creature", "for each -1/-1 counter on this creature"), `age
    /// counters on it` ("on it").
    SourceCounters {
        kind: mtg_core::card::CounterKind,
        it: bool,
    },
    /// `attached controller graveyard [<filter>]`: "the number of creature
    /// cards in its controller's graveyard" (of what the source enchants).
    HostGraveyard(Option<Filter>),
    /// `devotion green`: "your devotion to green" (CR 700.5); `devotion
    /// white and blue`: "your devotion to white and blue"; `red mana
    /// symbols`: "the number of red mana symbols in the mana costs of
    /// permanents you control" (chroma).
    Devotion {
        colors: Vec<mtg_core::mana::Color>,
        chroma: bool,
    },
}

/// What a target-dependent cost's filter names: a permanent, a spell, or a
/// card (in a graveyard).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TargetNoun {
    #[default]
    Permanent,
    Spell,
    Card,
}

/// A search's count meaning "any number of cards" (`search any number`).
pub const ANY_NUMBER: u32 = u32::MAX;

#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct Except {
    pub subtypes: Vec<String>,
    pub keywords: Vec<Keyword>,
    pub abilities: Vec<Ability>,
}

/// What takes an activated ability's cost down (`reduced {1} each Town you
/// control`, `reduced {4}{B} if drew 3 cards this turn`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ReduceBy {
    Each(CountExpr),
    If(StaticCond),
}

/// `alternative cost {1}, return basic land you control`, `alternative
/// cost sacrifice 2 Mountain`, `alternative cost sacrifice Mountain, if your
/// turn` (CR 118.9): mana, and `count` permanents sacrificed or returned
/// (the cost's `sacrifice` filter); the cost's condition says when it's
/// offered.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct AltCost {
    pub cost: Cost,
    pub count: u32,
    /// `alternative cost each other player gains 6 life, if you control
    /// Forest`: "you may have each other player gain 6 life rather than
    /// pay this spell's mana cost".
    pub others_gain: u32,
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
    /// `discard 1 card at random`: "Discard a card at random".
    pub discard_random: bool,
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
    /// `exile this`: "Exile this artifact" (from the battlefield).
    pub exile_this: bool,
    /// Crew's cost (from its expansion).
    pub crew: u32,
    /// `pay 2 energy`: "Pay {E}{E}".
    pub energy: u32,
    /// `remove 1 charge counter from this`: "Remove a charge counter from
    /// this artifact".
    pub remove_counters: Option<(CounterKind, u32)>,
    /// `put 1 -1/-1 counter on this`: "Put a -1/-1 counter on this
    /// creature".
    pub put_counters: Option<(CounterKind, u32)>,
    /// `return land you control` / `tap untapped creature you control`:
    /// what the `sacrifice` filter's chosen permanent pays.
    pub chosen: mtg_core::card::ChosenCost,
    /// `tap 2 untapped Beast you control`: how many permanents the
    /// `sacrifice` filter chooses; 0 and 1 are one.
    pub count: u32,
    /// `exile 2 cards from graveyard`, `exile creature card from
    /// graveyard`, `exile X creature cards from graveyard`.
    pub exile_cards: Option<Box<ExileCards>>,
    /// "Activate only once each turn."
    pub per_turn: u8,
    /// `reduced {1} each Town you control`: "This ability costs {1} less to
    /// activate for each Town you control."
    pub reduce: Option<Box<(ManaCost, ReduceBy)>>,
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
    /// `reveal hand`: "reveal your hand" (an alternative cost).
    pub reveal_hand: bool,
    /// `blight 1`: "Blight 1" (CR 701.68a).
    pub blight: u32,
}

/// A cost exiling cards from the graveyard: `n` (or X) cards matching
/// `filter`.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct ExileCards {
    pub n: u32,
    pub x: bool,
    pub filter: Option<Filter>,
    /// `exile top 3 black cards from graveyard`: "the top three black
    /// cards of your graveyard".
    pub top: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CostItem {
    /// `exile 2 cards from graveyard`.
    ExileCards,
    RemoveCounters,
    PutCounters,
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
    /// `reveal hand`: "reveal your hand".
    RevealHand,
    /// `blight 1`.
    Blight,
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

/// The subtype a filter names for "outlaw" (CR 700.12): it compiles to
/// "Assassin, Mercenary, Pirate, Rogue, or Warlock".
pub const OUTLAW: &str = "outlaw";

/// The subtype a filter names for "historic" (CR 700.6): it compiles to
/// "artifact, legendary, or Saga".
pub const HISTORIC: &str = "historic";

/// The creature types "outlaw" stands for (CR 700.12).
pub const OUTLAW_TYPES: [&str; 5] = ["Assassin", "Mercenary", "Pirate", "Rogue", "Warlock"];

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
    /// `who` is `Opponents`, written and printed "they control" after a
    /// recipient that is the opponents ("each opponent and each creature
    /// they control").
    pub they_control: bool,
    /// `who` is `Opponents`, written and printed "defending player
    /// controls" (two players: the attacker's opponent).
    pub defending: bool,
    /// Written `all ...`; a static ability's subject then prints "All
    /// Sliver creatures" rather than "Sliver creatures".
    pub all: bool,
    /// Written `each ...` as a static ability's subject: it prints one at
    /// a time, "Each creature you control with a +1/+1 counter on it has
    /// trample".
    pub each: bool,
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
    /// `damaged` written `dealt damage this turn`: printed "creature dealt
    /// damage this turn" rather than "creature that was dealt damage this
    /// turn".
    pub damaged_short: bool,
    /// `blocked` / `unblocked`, `with aura` / `without aura` ("enchanted
    /// permanent", "that isn't enchanted"), `that entered this turn`,
    /// `that dealt damage this turn`, `that blocked this turn`, `with mana
    /// value X` (`ObjFilter::status`).
    pub status: mtg_core::card::StatusQuals,
    /// `creature or Vehicle`: an alternative filter.
    pub or: Option<Box<Filter>>,
    /// `Knight or a Soldier`: the alternative written with its own article,
    /// printed so in a condition ("you control a Knight or a Soldier").
    /// Wording only.
    pub alt_article: bool,
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
    /// `with aura late`: "creatures you control that are enchanted"
    /// (`status.enchanted`, printed after the controller).
    pub enchanted_late: bool,
    /// `with Equipment late`: "attacking equipped creature" (`equipped`
    /// printed after "attacking"/"blocking" rather than before).
    pub equipped_late: bool,
    /// `attacking late`: "nonwhite attacking creature" (`attacking`
    /// printed after colors and negations rather than before).
    pub attacking_late: bool,
    /// `attached to this`, `attached to attached`, `attached to target 1`
    /// (`ObjFilter::attached_to`).
    pub attached_to: Option<AttachedRef>,
    /// `blocking this`, `blocked by this`, `blocked by it`, `blocking or
    /// blocked by this` (`ObjFilter::combat_with`).
    pub combat_with: Option<CombatRef>,
    /// `that player controls` / `from that player's graveyard` (the
    /// triggering player), `that target 1 player controls` (the 1st
    /// target's controller), all printed "that player" (`ObjFilter::that_player`).
    pub that_player: Option<mtg_core::card::ThatPlayer>,
    /// With `that_player` a target (`target player controls`, `target
    /// opponent controls`): the target is an opponent.
    pub that_opponent: bool,
    /// `other than enchanted`: "other than enchanted creature"
    /// (`ObjFilter::not_attached`).
    pub not_attached: bool,
    /// `other than this`: "other than this creature", the relation
    /// `Other` worded after the noun (Weed-Pruner Poplar).
    pub other_than_this: bool,
    /// `andor Villain or artifact`: its alternatives joined "and/or" as a
    /// trigger's subject ("Whenever another Villain and/or artifact you
    /// control enters"); wording only.
    pub and_or: bool,
}

impl Filter {
    /// Anything you control: `if you controlled target 1, ...`.
    pub fn controlled_by_you() -> Filter {
        Filter {
            who: Some(Who::You),
            ..Filter::default()
        }
    }
}

/// How a search prints its filter's alternative.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum OrCard {
    /// One noun: "a basic Forest, Plains, or Island card".
    #[default]
    No,
    /// `card or Gate card`: "a basic land card or Gate card" (plural
    /// "basic land cards and/or Gate cards").
    Bare,
    /// `card or a Gate card`: "a basic land card or a Gate card".
    Article,
}

/// A filter's combat relation to the source, as printed.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CombatRef {
    /// `blocking this`: "blocking this creature".
    BlockingThis,
    /// `blocking it`: "blocking it" (the source, named before).
    BlockingIt,
    /// `blocked by this`: "this creature is blocking".
    BlockedByThis,
    /// `blocked by it`: "it's blocking".
    BlockedByIt,
    /// `blocking or blocked by this`: "blocking or blocked by this
    /// creature".
    BlockingOrBlockedByThis,
}

/// What a filter's objects must be attached to.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AttachedRef {
    /// `attached to this`: "attached to it" (the source).
    Source,
    /// `attached to attached` (or `enchanted creature`, `equipped
    /// creature`): "attached to it", the source's host.
    Host,
    /// `attached to target 1`: the target, "it" after its first mention.
    Target(usize),
    /// `attached to that target 1`: "attached to that creature".
    ThatTarget(usize),
    /// `attached to that creature`: the triggering object.
    TriggeringObject,
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
    /// `with lesser power`: "with lesser power" (than the source's power,
    /// as it is or last existed; mentor, CR 702.134a).
    LesserPower,
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
    /// `graveyards`: `from graveyards` ("up to two target cards from
    /// graveyards"), which need not share a graveyard.
    GraveyardCard {
        filter: Filter,
        who: Who,
        graveyards: bool,
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
    /// `spree { mode {2}: ... mode {1}: ... }` (CR 702.172): one or more
    /// modes, each with an additional cost (at most three).
    Spree(Vec<(ManaCost, Mode)>),
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
        /// `triggered from graveyard ...`: works only while the card is
        /// in its owner's graveyard (CR 113.6k).
        from_graveyard: bool,
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
        /// Written `... if <cond>`: printed "<text> if <cond>." ("This
        /// creature can't be blocked if you've cast a historic spell this
        /// turn.") rather than "as long as".
        cond_if: bool,
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
        /// `static during turns other than yours, spells you cast cost {1}
        /// less`: only while the condition holds, printed first.
        cond: Option<StaticCond>,
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
        /// `if it targets spell creature`: "a creature spell"; `if it
        /// targets card creature with mana value 3 or less`: "a creature
        /// card with mana value 3 or less".
        noun: TargetNoun,
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
    /// `enters with 1 +1/+1 counter if attacked this turn`: "This
    /// creature enters with a +1/+1 counter on it if you attacked this
    /// turn." (raid; CR 614.1c); `if spent 3 white, enters with 1 +1/+1
    /// counter` (`first`): "If at least three white mana was spent to cast
    /// this spell, this creature enters with a +1/+1 counter on it."
    /// (adamant). The condition is judged as it enters.
    EntersWithIf {
        n: u32,
        kind: CounterKind,
        cond: StaticCond,
        first: bool,
    },
    /// `can't attack unless defending player controls Island`, `can't
    /// block unless ...`, `can't attack or block unless ...`.
    AttackUnless {
        cond: StaticCond,
        attack: bool,
        block: bool,
        /// `can't attack if ...`: "This creature can't attack if defending
        /// player controls an untapped land."
        negated: bool,
    },
    /// `hand size none`, `hand size +1`, `hand size opponents -1`: "You
    /// have no maximum hand size.", "Your maximum hand size is increased
    /// by one.", "Each opponent's maximum hand size is reduced by one."
    HandSize {
        whose: Who,
        delta: Option<i32>,
    },
    /// `extra vote`: "While voting, you get an additional vote."; `extra
    /// vote may`: "While voting, you may vote an additional time." (CR
    /// 701.38d).
    ExtraVote {
        optional: bool,
    },
    /// `may look top`: "You may look at the top card of your library any
    /// time."; `may look face-down`: "You may look at face-down creatures
    /// you don't control any time." (`may look face-down opponents`:
    /// "... your opponents control ..."); `may look top face-down`, both
    /// in one sentence. Information only.
    MayLook {
        top: bool,
        face_down: bool,
        opponents: bool,
    },
    /// `as turned face up put 4 +1/+1 counters`: "As this creature is
    /// turned face up, put four +1/+1 counters on it." (CR 708.11);
    /// `as enters or turned face up put count where other creature you
    /// control +1/+1 counters` (`enters`): "As this creature enters or is
    /// turned face up, put X +1/+1 counters on it, where X is ...".
    AsTurnedFaceUp {
        enters: bool,
        kind: CounterKind,
        n: Num,
    },
    /// `crews using toughness`: "This creature crews Vehicles using its
    /// toughness rather than its power."; `saddles and crews plus 2`:
    /// "This creature saddles Mounts and crews Vehicles as though its
    /// power were 2 greater." (CR 702.122a, 702.171a).
    CrewsAs {
        saddles: bool,
        toughness: bool,
        plus: u32,
    },
    /// `station 4: flying` (CR 721.2): "4+ | Flying": as long as it has
    /// four or more charge counters it has the keywords, and with a P/T
    /// box (the card's `pt`) it is a creature (CR 721.2b).
    StationLevel {
        n: u32,
        keywords: Vec<Keyword>,
    },
    /// `protection doesn't remove this`: "This effect doesn't remove this
    /// Aura.", printed after the static ability before it.
    AuraStays,
    /// `deck any number`: "A deck can have any number of cards named ~."
    /// (CR 100.2a). A deck-building rule with no meaning in a game.
    DeckAnyNumber,
    /// `this spell can't be countered`: "This spell can't be countered."
    Uncounterable,
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
    /// `while <condition>` after an attack or block trigger: judged only as
    /// the event happens ("Whenever this creature attacks while you control
    /// two or more artifacts, ...").
    While(StaticCond),
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
    /// CR 702.70: combat damage to a player gives that player N poison
    /// counters.
    Poisonous(u32),
    /// CR 702.68: attacks and isn't blocked, +N/+0 until end of turn.
    Frenzy(u32),
    /// CR 702.64: prevent N of each damage dealt to it.
    Absorb(u32),
    /// CR 702.39: attacks, you may have target creature defending player
    /// controls untap and block it if able.
    Provoke,
    /// CR 702.164: combat damage to a player also gives N poison counters.
    Toxic(u32),
    /// CR 702.21: becomes the target of an opponent's spell or ability,
    /// counter it unless its controller pays.
    Ward(ManaCost),
    /// `ward pay 3 life`: "Ward—Pay 3 life." (CR 702.21a): counter it
    /// unless that player pays the life.
    WardCost(Cost),
    /// CR 702.30: at your upkeep, if it came under your control since your
    /// last upkeep, sacrifice it unless you pay.
    Echo(ManaCost),
    /// `strive {2}{W}`: "Strive — This spell costs {2}{W} more to cast for
    /// each target beyond the first."
    Strive(ManaCost),
    /// `echo discard 1 card`: "Echo—Discard a card." (CR 702.30a).
    EchoCost(Cost),
    /// CR 702.84: from the graveyard as a sorcery: return it with haste,
    /// exile it at the next end step.
    Unearth(ManaCost),
    /// CR 702.27: an optional additional cost; the spell returns to its
    /// owner's hand as it resolves.
    Buyback(ManaCost),
    /// `buyback sacrifice land`: "Buyback—Sacrifice a land." (CR 702.27a).
    BuybackCost(Cost),
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
    /// CR 702.190: `sneak {2}{W}`: during your declare blockers step, cast
    /// for this cost and an unblocked attacker returned to hand; the
    /// permanent enters tapped and attacking.
    Sneak(ManaCost),
    /// CR 702.174: `gift card`, `gift food`, `gift tapped fish`, `gift
    /// treasure`: "Gift a card" and kin.
    Gift(GiftKind),
    /// CR 702.170: `plot {2}{B}`: exiled from hand for this cost as a
    /// sorcery, cast free as a sorcery on a later turn.
    Plot(ManaCost),
    /// CR 702.62: `suspend 4 {1}{U}`.
    Suspend(u32, ManaCost),
    /// CR 702.42: `entwine {2}`: every mode of the "choose one" spell for
    /// this cost on top.
    Entwine(ManaCost),
    /// CR 702.138: `escape {W} exile 2`.
    Escape(ManaCost, u32),
    /// `escapes with 2 +1/+1 counters`.
    EscapesWith(u32),
    /// `enters with choice flying counter or first strike counter`: "This
    /// creature enters with your choice of a flying counter or a first
    /// strike counter on it." (CR 614.1c); `true`: "escapes with ...".
    CounterChoice([CounterKind; 2], bool),
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
    /// `morph pay 5 life`, `morph discard 1 card`, `morph return Bird you
    /// control`: "Morph—Pay 5 life." and kin (CR 702.37a: any cost).
    MorphCost(Cost),
    /// CR 702.37b: morph, and a +1/+1 counter as it turns face up.
    Megamorph(ManaCost),
    /// CR 702.168: morph with ward {2} while face down.
    Disguise(ManaCost),
    /// CR 702.49: `ninjutsu {1}{U}`: from the hand, return an unblocked
    /// attacker you control; this card enters tapped and attacking.
    Ninjutsu(ManaCost),
    /// CR 702.97: `scavenge {5}{G}`: exile it from your graveyard as a
    /// sorcery; +1/+1 counters equal to its power on target creature.
    Scavenge(ManaCost),
    /// CR 702.109: `dash {2}{R}`: an alternative cost; haste, and back to
    /// its owner's hand at the next end step.
    Dash(ManaCost),
    /// CR 702.185: `warp {1}{U}`: an alternative cost from the hand; the
    /// permanent is exiled at the next end step, and may be cast from
    /// exile on a later turn.
    Warp(ManaCost),
    /// CR 702.103: `bestow {3}{W}`: an alternative cost; the spell is an
    /// Aura with enchant creature, a creature again once unattached.
    Bestow(ManaCost),
    /// Overload (CR 702.96): `overload {1}{U}`.
    Overload(ManaCost),
    /// CR 702.47: `splice {1}{R}`: splice onto Arcane; revealed from hand
    /// as an Arcane spell is cast, this paid too, its effects added.
    Splice(ManaCost),
    /// `splice sacrifice 2 Mountain`: splice onto Arcane with a cost
    /// other than mana ("Splice onto Arcane—Sacrifice two Mountains.").
    SpliceCost(Cost),
    /// CR 702.128: `embalm {W}`: exile it from your graveyard as a
    /// sorcery; a token copy that's a white Zombie with no mana cost.
    Embalm(ManaCost),
    /// CR 702.129: `eternalize {4}{U}{U}`: the same, a 4/4 black Zombie.
    Eternalize(ManaCost),
    /// CR 702.141: `encore {4}{B}`: exile it from your graveyard as a
    /// sorcery; for each opponent, a token copy that attacks that opponent
    /// this turn if able, with haste, sacrificed at the next end step.
    Encore(ManaCost),
    /// CR 702.165: `backup 1`: as it enters, N +1/+1 counters on target
    /// creature; another creature also gains the abilities written after
    /// this line until end of turn.
    Backup(u32),
    /// CR 702.175: `offspring {1}`: an optional additional cost; if paid,
    /// a 1/1 token copy as it enters.
    Offspring(ManaCost),
    /// CR 702.171: `saddle 2`: "Tap any number of other creatures you
    /// control with total power N or more: This Mount becomes saddled until
    /// end of turn. Saddle only as a sorcery."
    Saddle(u32),
    /// CR 702.166: `bargain`: "You may sacrifice an artifact, enchantment,
    /// or token as you cast this spell."
    Bargain,
    /// Conspire (CR 702.78).
    Conspire,
    /// `casualty 2` (CR 702.153): "As you cast this spell, you may
    /// sacrifice a creature with power N or greater. When you do, copy
    /// this spell."
    Casualty(u32),
    /// `dredge 2` (CR 702.52): "If you would draw a card, you may mill N
    /// cards instead. If you do, return this card from your graveyard to
    /// your hand."
    Dredge(u32),
    /// `storm` (CR 702.40): "When you cast this spell, copy it for each
    /// spell cast before it this turn."
    Storm,
    /// `awaken 3 {5}{B}` (CR 702.113): "Awaken 3—{5}{B}", an alternative
    /// cost; cast for it, also N +1/+1 counters on target land you
    /// control, which becomes a 0/0 Elemental creature with haste.
    Awaken(u32, ManaCost),
    /// `assist` (CR 702.132): "Another player can pay up to {N} of this
    /// spell's cost." The game's only other player is an opponent, who
    /// here never pays: the line has no effect.
    Assist,
    /// `enlist` (CR 702.154): as this creature attacks, you may tap a
    /// nonattacking creature you control without summoning sickness; it
    /// gets +X/+0 until end of turn, X that creature's power.
    Enlist,
    /// `replicate {U}` (CR 702.56): an additional cost paid any number of
    /// times; copied once for each.
    Replicate(ManaCost),
    /// `teamwork 2`: "Teamwork 2 (As an additional cost to cast this
    /// spell, you may tap any number of creatures you control with total
    /// power 2 or more.)"
    Teamwork(u32),
    /// `ripple 4` (CR 702.60).
    Ripple(u32),
    /// CR 702.184: `station`: "Tap another untapped creature you control:
    /// Put a number of charge counters on this permanent equal to the
    /// tapped creature's power. Activate only as a sorcery."
    Station,
    /// CR 702.131: `ascend`: the city's blessing once you control ten or
    /// more permanents (a permanent's static ability; an instant's or
    /// sorcery's first effect).
    Ascend,
    /// CR 702.179: `start your engines`: "Start your engines!", a speed of
    /// 1 for a player who has none.
    StartYourEngines,
    /// `becomes day as this enters`: "If it's neither day nor night, it
    /// becomes day as this creature enters." (CR 731.1).
    BecomesDay,
    /// `fabricate 1` (CR 702.123): when this enters, N +1/+1 counters on
    /// it or N 1/1 Servos.
    Fabricate(u32),
    /// `evolve` (CR 702.100).
    Evolve,
    /// `renown 1` (CR 702.112).
    Renown(u32),
    /// `unleash` (CR 702.98): may enter with a +1/+1 counter; can't block
    /// as long as it has one.
    Unleash,
    /// `afterlife 1` (CR 702.135): when this dies, N 1/1 white and black
    /// Spirit tokens with flying.
    Afterlife(u32),
    /// `extort` (CR 702.101): whenever you cast a spell, you may pay
    /// {W/B}; if you do, each opponent loses 1 life and you gain that
    /// much life.
    Extort,
    /// `mentor` (CR 702.134): whenever this attacks, a +1/+1 counter on
    /// target attacking creature with lesser power.
    Mentor,
    /// `riot` (CR 702.136): enters with your choice of a +1/+1 counter or
    /// haste.
    Riot,
    /// `living weapon` (CR 702.92): when this Equipment enters, a 0/0
    /// black Phyrexian Germ token, then attach this to it.
    LivingWeapon,
    /// `retrace` (CR 702.81): cast from your graveyard by discarding a land
    /// card in addition to its other costs.
    Retrace,
    /// `jump-start` (CR 702.133): cast from your graveyard by discarding a
    /// card in addition to its other costs; then exiled.
    JumpStart,
    /// `surge {1}{R}` (CR 702.117): an alternative cost if you've cast
    /// another spell this turn.
    Surge(ManaCost),
    /// `freerunning {1}{B}` (CR 702.173): an alternative cost if an
    /// Assassin you controlled dealt combat damage to a player this turn.
    Freerunning(ManaCost),
    /// `spectacle {R}` (CR 702.137): an alternative cost if an opponent
    /// lost life this turn.
    Spectacle(ManaCost),
    /// `ingest` (CR 702.115): whenever this deals combat damage to a
    /// player, that player exiles the top card of their library.
    Ingest,
    /// `reinforce 1 {1}{R}` (CR 702.77): "Reinforce 1—{1}{R}", "{1}{R},
    /// Discard this card: Put a +1/+1 counter on target creature."
    Reinforce(u32, ManaCost),
}

/// A blocking restriction on this creature.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum BlockRule {
    NotBy(Filter),
    OnlyBy(Filter),
    AtMostOne,
    BlocksOnly(Filter),
    CantBlock(Filter),
    /// `can't be blocked by creature with lesser power`: "Creatures with
    /// power less than this creature's power can't block it."
    NotByLesserPower,
    /// `can't be blocked except by 3 or more creatures`.
    MinBlockers(u8),
    /// `can block shadow as though had shadow` ("can block creatures with
    /// shadow as though it had shadow"), `... as though didn't have
    /// shadow` ("... as though they didn't have shadow").
    BlocksShadow {
        had: bool,
    },
    /// `can block landwalk as though didn't have`: "can block creatures
    /// with landwalk abilities as though they didn't have those abilities".
    BlocksLandwalk,
    /// `can block an additional 7 creatures each combat`.
    BlocksAdditional(u8),
}

/// How a graveyard-card condition is worded.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum GraveyardWording {
    /// "there are two or more creature cards in your graveyard".
    ThereAre,
    /// "you have four or more creature cards in your graveyard".
    YouHave,
    /// "there is a land card in your graveyard".
    ThereIs,
    /// "there's a Lesson card in your graveyard".
    Theres,
    /// "an artifact card is in your graveyard".
    IsIn,
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
    /// `not your turn`: "it's not your turn"; `static during turns other
    /// than yours, ...` prints "During turns other than yours, ...".
    NotYourTurn,
    /// `cast during main phase`: "you cast this spell during your main
    /// phase" (judged as it was cast).
    CastDuringMain,
    /// `controlled Mount as cast`: "you controlled a Mount as you cast this
    /// spell".
    ControlledAsCast(Filter),
    /// Written `static during your turn, <filter>: ...`.
    DuringYourTurn,
    /// `attacking`, `enchanted`, `equipped`: "it's attacking" (the source).
    Attacking,
    Enchanted,
    Equipped,
    /// `monstrous`: "this creature is monstrous" (CR 701.37b).
    Monstrous,
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
    /// `there are 3 or more brick counters on this`: "there are three or
    /// more brick counters on this artifact" (the same condition, worded
    /// from the counters).
    SourceCountersThere {
        kind: CounterKind,
        n: u32,
    },
    /// `defending player controls Island`: "defending player controls an
    /// Island"; `at_least` 0, `defending player controls no Glimmer
    /// creature`: "... controls no Glimmer creatures".
    DefenderControls {
        filter: Filter,
        at_least: u32,
    },
    /// `on battlefield Mountain`, `on battlefield 5 Island`: "there is a
    /// Mountain on the battlefield", "there are five or more Islands on the
    /// battlefield".
    OnBattlefield {
        filter: Filter,
        at_least: u32,
    },
    /// `defending player poisoned`: "defending player is poisoned".
    DefenderPoisoned,
    /// `your life more than opponent`: "you have more life than an
    /// opponent".
    MoreLifeThanOpponent,
    /// `3 basic land types`: "there are three or more basic land types
    /// among lands you control" (domain, CR 305.6).
    BasicLandTypes(u32),
    /// `you don't control legendary creature`: "you don't control a
    /// legendary creature" (`YouControlNone` worded so).
    YouDontControl(Filter),
    /// `at least 2 other creatures attack` ("... block"), `black or green
    /// creature also attacks`: of "can't attack unless".
    OthersInCombat {
        filter: Filter,
        at_least: u32,
        also: bool,
        block: bool,
    },
    /// `you control more creatures than defending player` (or `attacking
    /// player`).
    MoreCreaturesThanOpponent {
        attacking: bool,
    },
    /// `you control no untapped land`.
    YouControlNone(Filter),
    /// `you control Urza's Mine and Urza's Power-Plant`: "you control an
    /// Urza's Mine and an Urza's Power-Plant".
    YouControlEach(Vec<Filter>),
    /// `attacked this turn` (raid): "you attacked this turn".
    AttackedThisTurn,
    /// `2 unlocked doors`: "there are two or more unlocked doors among
    /// Rooms you control" (CR 709.5j).
    UnlockedDoors(u32),
    /// `attacked with Spacecraft this turn`: "you attacked with a
    /// Spacecraft this turn".
    AttackedWith(Filter),
    /// `entered another Knight this turn`: "another Knight entered the
    /// battlefield under your control this turn" (judged as it is or as it
    /// last existed); `had another creature enter this turn`: "you had
    /// another creature enter the battlefield under your control this
    /// turn".
    EnteredWith {
        filter: Filter,
        had: bool,
    },
    /// `attacked a battle this turn`: "it attacked a battle this turn"
    /// (the source).
    AttackedBattleThisTurn,
    /// `attacked last turn`: "it attacked during your last turn".
    AttackedLastTurn,
    /// `creature died this turn` (morbid).
    CreatureDiedThisTurn,
    /// `opponent damaged this turn`: "an opponent was dealt damage this
    /// turn".
    OpponentDamagedThisTurn,
    /// `opponent lost life this turn`: "an opponent lost life this turn".
    OpponentLostLifeThisTurn,
    /// `permanent left this turn` (revolt): "a permanent left the
    /// battlefield under your control this turn".
    PermanentLeftThisTurn,
    /// Cast with escape (internal: "escapes with").
    Escaped,
    /// Cast with kicker (internal: `kicked with`).
    Kicked,
    /// `bargained`: "this spell was bargained" (an instant or sorcery),
    /// "it was bargained", "it's bargained" (a cost) (CR 702.166b).
    Bargained,
    /// `teamwork`: "this spell was cast using teamwork".
    Teamwork,
    /// `saddled`: "it's saddled"; after a trigger, `while saddled`
    /// ("Whenever this creature attacks while saddled, ...", CR 702.171b).
    Saddled,
    /// `land entered this turn` (landfall): "you had a land enter the
    /// battlefield under your control this turn".
    LandEnteredThisTurn,
    /// `artifact entered this turn`: "an artifact entered the battlefield
    /// under your control this turn".
    ArtifactEnteredThisTurn,
    /// `drew 2 cards this turn`: "you've drawn two or more cards this
    /// turn".
    CardsDrawnThisTurn(u32),
    /// `hand 1 or less`: "you have one or fewer cards in hand"; `hand 0 or
    /// less`: "you have no cards in hand".
    CardsInHand {
        n: u32,
        at_most: bool,
    },
    /// `hand has no land card`: "you have no land cards in hand".
    NoCardInHand(Filter),
    /// `2 instant or sorcery cards in your graveyard`: "there are two or
    /// more instant and/or sorcery cards in your graveyard"; `you have 4
    /// creature cards in your graveyard`: "you have four or more creature
    /// cards in your graveyard"; `there is land card in your graveyard`:
    /// "there is a land card in your graveyard" (`there's` prints
    /// "there's a"); `1 artifact card is in your graveyard`: "an artifact
    /// card is in your graveyard".
    GraveyardCards {
        filter: Filter,
        at_least: u32,
        wording: GraveyardWording,
    },
    /// `you control Desert or there is Desert card in your graveyard`:
    /// either holds.
    Or(Box<StaticCond>, Box<StaticCond>),
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
    /// or sorcery spell this turn"; `cast 2 spell this turn` ("you've cast
    /// two or more spells this turn"), `cast another blue spell this turn`
    /// ("you've cast another blue spell this turn").
    CastThisTurn {
        filter: Filter,
        at_least: u32,
        other: bool,
    },
    /// `cast from hand` ("you cast it from your hand"), `cast from
    /// graveyard` ("this spell was cast from a graveyard").
    CastFrom(mtg_core::state::Zone),
    /// `gained life this turn`, `gained or lost life this turn`.
    GainedLifeThisTurn {
        or_lost: bool,
    },
    /// `descended this turn`: "you descended this turn".
    DescendedThisTurn,
    /// `spell number 2 this turn`: the spell being cast would be your
    /// second this turn ("The second spell you cast each turn costs {1}
    /// less to cast.").
    SpellNumber(u32),
    /// `surveilled this turn`: "you've surveilled this turn".
    SurveilledThisTurn,
    /// `counter put this turn`: "a +1/+1 counter was put on a permanent
    /// under your control this turn".
    CounterPutThisTurn,
    /// `face-down creature entered this turn`: "a face-down creature
    /// entered the battlefield under your control this turn".
    FaceDownEnteredThisTurn,
    /// `creature entered this turn`, `another creature entered this turn`
    /// ("another creature entered the battlefield under your control this
    /// turn").
    CreatureEnteredThisTurn {
        other: bool,
    },
    /// `2 nonland entered this turn` (celebration): "two or more nonland
    /// permanents entered the battlefield under your control this turn".
    NonlandEnteredThisTurn(u32),
    /// `artifact or creature to graveyard this turn`: "an artifact or
    /// creature was put
    /// into a graveyard from the battlefield this turn".
    PutIntoGraveyardThisTurn(Vec<mtg_core::card::CardType>),
    /// `discarded this turn`: "you've discarded a card this turn".
    DiscardedThisTurn,
    /// `night`: "it's night".
    Night,
    /// `gained 3 life this turn`: "you've gained 3 or more life this turn".
    LifeGainedThisTurn(u32),
    /// `committed crime this turn`: "you've committed a crime this turn".
    CrimeThisTurn,
    /// `card left your graveyard this turn`: "one or more cards left your
    /// graveyard this turn".
    LeftGraveyardThisTurn,
    /// `creature card to your graveyard this turn`: "a creature card was
    /// put into your graveyard from anywhere this turn".
    CardToGraveyardThisTurn(Vec<mtg_core::card::CardType>),
    /// `sacrificed artifact this turn`: "you've sacrificed an artifact this
    /// turn".
    SacrificedThisTurn(Vec<mtg_core::card::CardType>),
    /// `more cards in hand than each opponent`.
    MoreCardsInHand,
    /// `opponent poisoned` ("an opponent is poisoned"), `opponent poison 3
    /// or more` ("an opponent has three or more poison counters").
    OpponentPoison(u32),
    /// `you own card in exile`: "you own a card in exile".
    OwnCardInExile,
    /// `you are monarch`: "you're the monarch" (CR 725).
    Monarch,
    /// `completed dungeon`: "you've completed a dungeon" (CR 309.7).
    CompletedDungeon,
    /// `city's blessing`: "you have the city's blessing" (CR 702.131c).
    CityBlessing,
    /// `max speed`: "you have max speed" (CR 702.179e). Usually written
    /// as the ability word `label "Max speed"` instead.
    MaxSpeed,
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
    /// `spent {B}`: "{B} was spent to cast this spell" (an instant or
    /// sorcery) or "... to cast it"; `spent {R}{R}`; `spent 3 red`: "at
    /// least three red mana was spent ..." (adamant); `spent treasure`:
    /// "mana from a Treasure was spent ..." (CR 601.2h). `that spell
    /// spent 5 mana`: the spell that triggered the ability, "five or more
    /// mana was spent to cast that spell" (as a trigger's condition, "at
    /// least five mana was spent to cast it").
    ManaSpent {
        kind: mtg_core::card::SpentKind,
        at_least: u8,
        that: bool,
    },
    /// `no mana spent`: "it wasn't cast or no mana was spent to cast it".
    NoManaSpent,
    /// `not kicked`: "this spell wasn't kicked"; on a card with gift, "the
    /// gift wasn't promised" (CR 702.174).
    NotKicked,
    /// `sneak paid`, `his sneak paid`, `her sneak paid`: "this spell's
    /// sneak cost was paid" (an instant or sorcery), "its (his, her) sneak
    /// cost was paid" (CR 702.190). The pronoun: 0 its, 1 his, 2 her.
    SneakPaid(u8),
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

/// What a gift gives the chosen opponent (CR 702.174d-h).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum GiftKind {
    /// "Gift a card": they draw a card.
    Card,
    /// "Gift a Food": they create a Food token.
    Food,
    /// "Gift a tapped Fish": they create a tapped 1/1 blue Fish creature
    /// token.
    TappedFish,
    /// "Gift a Treasure": they create a Treasure token.
    Treasure,
}

impl GiftKind {
    /// The DSL words after `gift`.
    pub const fn words(self) -> &'static str {
        match self {
            GiftKind::Card => "card",
            GiftKind::Food => "food",
            GiftKind::TappedFish => "tapped fish",
            GiftKind::Treasure => "treasure",
        }
    }

    /// The Oracle words after "Gift".
    pub const fn oracle(self) -> &'static str {
        match self {
            GiftKind::Card => "a card",
            GiftKind::Food => "a Food",
            GiftKind::TappedFish => "a tapped Fish",
            GiftKind::Treasure => "a Treasure",
        }
    }
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

/// A row of a results table (CR 706.3a): `result 1 to 9`, `result 20`,
/// `result 20 or more` ("20+", `hi` `None`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct DieRow {
    pub lo: u32,
    pub hi: Option<u32>,
    pub effects: Vec<Sentence>,
}

/// How a conditional sentence prints its condition.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum IfWording {
    /// "If <condition>, <effect>."
    #[default]
    Lead,
    /// "If <condition>, it also <effect>."
    Also,
    /// "<Effect> if <condition>."
    After,
    /// `<clause> unless <condition>`: "<Effect> unless <condition>." (the
    /// effect happens only if the condition doesn't hold).
    Unless,
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
    /// `damage this turn to A [from S]`: "that would be dealt this turn to
    /// A [by S]" (Divine Light).
    TurnFirst,
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
    /// `up to 2 target creature`: several targets, each locked by an
    /// effect of its own.
    Targets(usize, usize),
    /// `creature except target creature`: "creatures other than target
    /// creature" (Terrifying Presence).
    OtherThanTarget(Filter, usize),
    /// `chosen red source`: "a red source of your choice", a damage source
    /// chosen as the effect is created (CR 609.7a).
    Chosen(Filter),
    /// `creature dealt damage this way`: "a creature dealt damage this
    /// way", each permanent the resolution dealt damage to (CR 608.2c).
    DamagedThisWay,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Trigger {
    /// `when you control no Island`: a state trigger (CR 603.8), "When you
    /// control no Islands".
    State(StaticCond),
    Enters(TriggerObj),
    Dies(TriggerObj),
    /// `at beginning of every upkeep`: "At the beginning of each upkeep"
    /// (each player's, worded without "player's", as werewolves are).
    EveryStep(Step),
    BeginningOfStep {
        step: Step,
        whose: Who,
    },
    /// `at beginning of enchanted controller's upkeep`: "At the beginning
    /// of the upkeep of enchanted creature's controller".
    AttachedStep(Step),
    Attacks(TriggerObj),
    /// "Whenever a creature you control attacks alone".
    AttacksAlone(TriggerObj),
    /// `whenever you attack` (`min` 1), `whenever you attack with 3 or
    /// more creatures`, `whenever this and at least 2 other creatures
    /// attack` (battalion: `this`, `min` 3).
    AttacksWith {
        this: bool,
        min: u8,
    },
    /// `whenever this attacks and isn't blocked`.
    AttacksUnblocked(TriggerObj),
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
        /// `... by one or more Orc`: once, not once per pair.
        any: bool,
    },
    /// `whenever this attacks or blocks`.
    AttacksOrBlocks(TriggerObj),
    /// `when this leaves the battlefield`.
    Leaves(TriggerObj),
    /// `whenever you sacrifice other permanent`, `whenever a player
    /// sacrifices creature`, `when you sacrifice this` (CR 701.21).
    Sacrificed {
        obj: TriggerObj,
        whose: Who,
    },
    Blocks(TriggerObj),
    DealsCombatDamageToPlayer(TriggerObj),
    /// `deals combat damage to a player or battle` (CR 310.6).
    DealsCombatDamageToPlayerOrBattle(TriggerObj),
    /// `whenever this attacks a battle` (CR 310.5).
    AttacksBattle(TriggerObj),
    /// `when you unlock this door` (CR 709.5h).
    UnlocksDoor,
    /// `whenever you fully unlock a Room` (CR 709.5i).
    FullyUnlocks,
    /// `deals combat damage to defending player`: the same trigger, worded
    /// with "defending player" (combat damage to a player is dealt to the
    /// defending player, CR 510.1).
    DealsCombatDamageToDefending(TriggerObj),
    GainsLife {
        whose: Who,
    },
    /// `whenever you gain life first time each turn`: "Whenever you gain
    /// life for the first time each turn".
    GainsLifeFirst {
        whose: Who,
    },
    /// `whenever day becomes night or night becomes day` (CR 731.1a).
    DayNight,
    /// `whenever you roll dice`: "Whenever you roll one or more dice".
    RollsDice {
        whose: Who,
    },
    /// `whenever you win a coin flip`.
    WinsFlip {
        whose: Who,
    },
    /// `whenever you cast instant or sorcery spell`; `whenever you cast or
    /// copy instant or sorcery spell` (`or_copy`, magecraft).
    CastsSpell {
        whose: Who,
        noncreature: bool,
        instant_or_sorcery: bool,
        or_copy: bool,
    },
    BecomesTarget(TriggerObj),
    /// Ward's trigger: "becomes the target of a spell or ability an
    /// opponent controls".
    TargetedByOpponent,
    /// Valiant's trigger, `whenever this becomes targeted by you first time
    /// each turn`: "becomes the target of a spell or ability you control
    /// for the first time each turn".
    TargetedByYouFirstTime,
    /// `when this is turned face up`, `whenever permanent is turned face
    /// up`, `whenever this or Detective you control is turned face up`
    /// (CR 708.8).
    TurnedFaceUp(TriggerObj),
    /// `whenever this becomes targeted`: printed "Whenever" (most
    /// creatures) where `when ...` prints "When".
    BecomesTargetWhenever(TriggerObj),
    BecomesTapped(TriggerObj),
    BecomesUntapped(TriggerObj),
    /// `whenever one or more +1/+1 counters put on this`: "Whenever one or
    /// more +1/+1 counters are put on this creature" (CR 122.1).
    CountersPut {
        obj: TriggerObj,
        kind: CounterKind,
    },
    /// `whenever Aura becomes attached to this`: "Whenever an Aura becomes
    /// attached to this creature".
    BecomesAttachedTo(Filter),
    /// `whenever enchanted land is tapped for mana: add {G}`: "Whenever
    /// enchanted land is tapped for mana, its controller adds an
    /// additional {G}." A triggered mana ability (CR 605.1b); its effects
    /// are mana for the tapped permanent's controller, nothing else.
    TappedForMana(TriggerObj),
    /// `whenever this is dealt damage`.
    DealtDamage(TriggerObj),
    /// `whenever this is dealt combat damage`.
    DealtCombatDamage(TriggerObj),
    /// `whenever this deals damage to creature`, `... to player`, `... to
    /// opponent`, `... deals combat damage to creature`.
    DealsDamage {
        obj: TriggerObj,
        to: mtg_core::card::DamageTo,
        combat: bool,
    },
    /// `whenever this deals damage to Vampire`, `... to Goblin or Orc`:
    /// the damaged permanent, matching the filter, is "that creature".
    DealsDamageTo {
        obj: TriggerObj,
        filter: Box<Filter>,
    },
    /// `whenever you cast artifact spell`, `whenever a player casts
    /// multicolored spell`.
    CastsFiltered {
        whose: Who,
        filter: Filter,
    },
    /// `when this exploits a creature` (CR 702.110).
    Exploits,
    /// `when this becomes plotted` (CR 702.170): "When this card becomes
    /// plotted".
    BecomesPlotted,
    /// `whenever you draw your second card each turn`; `... card in a
    /// turn` prints that way ("When you draw your third card in a turn").
    DrawsNth {
        whose: Who,
        n: u8,
        in_a_turn: bool,
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
    /// `when you cast this` ("When you cast this spell", CR 603.2) and
    /// `when you cast or cycle this` ("When you cast or cycle ~").
    CastThis {
        or_cycle: bool,
    },
    /// `whenever you sacrifice other Eldrazi` ("Whenever you sacrifice
    /// another Eldrazi").
    Sacrifices(Filter),
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
    /// `when this enters or dies`, `whenever this enters or attacks`,
    /// `when this enters or leaves the battlefield`, `when this enters or
    /// is turned face up`: one ability, two events of the source, printed
    /// with the subject once. `whenever` is the word written (Oracle
    /// mostly says "When", "Whenever" with `attacks`).
    Or {
        first: Box<Trigger>,
        second: Box<Trigger>,
        whenever: bool,
    },
    /// `whenever one or more artifact you control enter`: once for the
    /// objects entering together (CR 603.2c); `that much` is how many.
    EntersBatch(Filter),
    /// `whenever one or more other creature you control die`, `whenever
    /// one or more permanent you control leave the battlefield` (CR
    /// 603.2c, 603.10a).
    LeavesBatch {
        filter: Filter,
        dies: bool,
    },
    /// `whenever you draw a card`, `whenever a player draws a card`,
    /// `whenever an opponent draws a card` (once per card, CR 121.2).
    Draws {
        whose: Who,
    },
    /// `whenever a player discards a card`, `whenever you discard creature
    /// card` (CR 701.9): the discarded card is the triggering object.
    Discards {
        whose: Who,
        filter: Filter,
    },
    /// `whenever one or more cards leave your graveyard`, `... creature
    /// cards leave your graveyard`: once for the cards leaving together
    /// (CR 603.2c).
    LeavesGraveyard(Filter),
    /// `whenever land card is put into your graveyard from anywhere`.
    PutIntoGraveyard(Filter),
    /// `whenever creature enters from your graveyard`.
    EntersFromGraveyard(Filter),
    /// `whenever you expend 4` (CR 700.14).
    Expends(u8),
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
    /// `set type creature`: "is a creature" (its other card types go,
    /// layer 4).
    SetTypes(Vec<CardType>),
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
    /// `set subtype Spirit`: "is a Spirit" (it loses its other creature
    /// types, CR 205.1b).
    SetSubtype(String),
    /// `add land all`: "is every basic land type in addition to its other
    /// types".
    AddAllLandTypes,
    Control,
    AddColors(Vec<Color>),
    SetColors(Vec<Color>),
    AddKeywords(Vec<Keyword>),
    /// `has ability { <ability line> }`: "has \"{T}: Add {G}.\"" (a quoted
    /// ability, layer 6).
    AddAbility(Box<Ability>),
    /// `loses flying`.
    LosesKeywords(Vec<Keyword>),
    /// `goaded`: "is goaded" (CR 701.15): attacks each combat if able,
    /// and a player other than the source's controller if able.
    Goaded,
    RemoveAllAbilities,
    /// `add subtype every`: "is every creature type"; `add subtype all`:
    /// "is all creature types" (`every` false). Layer 4.
    AllCreatureTypes {
        every: bool,
    },
    /// `loses all creature types` (layer 4).
    NoCreatureTypes,
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
    /// `cda pt <count> [+N]`, `cda power <count>`, `cda toughness
    /// <count>`: a characteristic-defining ability (CR 604.3).
    CdaPt {
        count: CountExpr,
        sets: mtg_core::card::CdaSets,
        plus_toughness: i32,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Recipient {
    /// The `n`th target (0-based), declared by its first mention; printed
    /// as "it" after that.
    Target(usize),
    /// `each creature blocking it`: the creatures blocking the triggering
    /// creature (or this one).
    BlockingIt,
    /// Targets `first..first + count` as one subject ("up to two target
    /// creatures each get +1/+0").
    Targets(usize, usize),
    /// `controller of that target 1`: "that creature's controller".
    ThatControllerOf(usize),
    /// "that player" after "Return target permanent to its owner's hand":
    /// the owner of the `n`th target, printed "that player" (Recoil's
    /// "Then that player discards a card").
    ThatOwner(usize),
    /// `controller of the target 1`: "the creature's controller".
    TheControllerOf(usize),
    /// `the target 1`: the `n`th target, "the creature" (Dark Temper's
    /// "destroy the creature instead").
    TheTarget(usize),
    /// `controller of the creature` in a triggered ability: the triggering
    /// object's controller, "the creature's controller" (Creature Bond).
    TheTriggeringController,
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
    /// `each opponent who doesn't control Elf`: "each opponent who doesn't
    /// control an Elf" (the group's filter).
    OpponentsWithout(u16),
    /// `creature dealt damage this way`: "a creature dealt damage this
    /// way" (each permanent the resolution has dealt damage to).
    DamagedThisWay,
    /// `each other player`: "each other player", the same players as
    /// each opponent.
    EachOtherPlayer,
    /// `this`: the source, "this creature" (a spell's or a legendary
    /// permanent's name, CR 201.5) at its first mention, "it" after.
    Source,
    /// `this creature` (the noun the card uses for itself): the source
    /// named in full even after a mention ("..., you may sacrifice it. If
    /// you do, this creature deals 3 damage to that player.").
    SourceNamed,
    /// `this card`: the source printed "this card" (its card in a
    /// graveyard: "When this creature dies, ... Exile this card.").
    SourceCard,
    /// `it` in a spell or an activated ability: the source as "it" even at
    /// its first mention ("If this spell was kicked, it deals 2 damage").
    SourceIt,
    TriggeringObject,
    /// `triggering card`: the card the triggering object became, "that
    /// card" ("When enchanted creature dies, return that card to its
    /// owner's hand").
    TriggeringCard,
    /// `its controller` in a triggered ability with no targets: the
    /// triggering object's controller ("When enchanted creature dies, its
    /// controller draws a card"); `controller of that creature` (`that`)
    /// prints "that creature's controller" (or "that land's").
    TriggeringController {
        that: bool,
    },
    /// The triggering object written `it` in a triggered ability ("Whenever
    /// another creature you control attacks, it gets +0/+1").
    TriggeringIt,
    /// The triggering object written `the creature` in a triggered
    /// ability: "If the player does, untap the creature." (Paralyze).
    TheTriggeringObject,
    TriggeringPlayer,
    DefendingPlayer,
    /// `that defending player`: the defending player, printed "defending
    /// player" where `defending player` as an object prints "the player
    /// or planeswalker it's attacking" (Tormentor's Helm's older wording).
    DefendingPlayerWord,
    EachCreature,
    EachPlayer,
    EachCreatureYouControl,
    EachPermanentYouControl,
    SameObject,
    /// `that creature` inside a delayed trigger about a target: the object
    /// the trigger is about (`SameObject`), printed "that creature".
    SameThat,
    CreatedObject,
    /// `those tokens`: every token the last token-creating sentence
    /// created; inside a delayed trigger about them, each in turn
    /// (`SameTokens`, the core `SameObject`).
    CreatedTokens,
    SameTokens,
    /// `each <filter>` / `all <filter>`: the `n`th of the card's `groups`.
    Group(u16),
    /// `random <filter>`: one of the `n`th group's permanents, picked at
    /// random ("a creature an opponent controls chosen at random").
    RandomOf(u16),
    /// `enchanted creature`, `equipped creature`: what the source is
    /// attached to.
    Attached,
    /// `controller of enchanted`: "enchanted creature's controller".
    AttachedController,
    /// `target creature and all other creature with same name as target
    /// 1`: the `n`th target and the `g`th group, whose filter relates to
    /// that target.
    TargetAnd(usize, u16),
    /// `exiled card`: "the exiled card", what this permanent's linked
    /// ability exiled (CR 607.2a).
    ExiledCard,
    /// `those cards`: the targets declared so far (`first`, `count`),
    /// after an effect moved them ("Exile two target creatures you
    /// control, then return those cards to the battlefield").
    ThoseCards(usize, usize),
    /// `those targets`: every target declared so far, named again by
    /// their kind ("If this spell was kicked, put a stun counter on each
    /// of those creatures").
    ThoseTargets(usize, usize),
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
        /// `without regeneration joined`: "Destroy target Knight and it
        /// can't be regenerated." in one sentence.
        joined: bool,
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
    /// `remove 1 counter from target permanent`: "remove a counter from
    /// target permanent" (of a kind the controller chooses).
    RemoveCounterChosen {
        to: Recipient,
    },
    ReturnToHand {
        what: Vec<Recipient>,
    },
    /// `return that card to battlefield tapped under your control`;
    /// `return this to battlefield attached to triggering object`: "...
    /// to the battlefield attached to that creature" (an Aura, CR 303.4f).
    ReturnToBattlefield {
        what: Recipient,
        counters: Option<(CounterKind, u32)>,
        tapped: bool,
        under_you: bool,
        attached: Option<Recipient>,
    },
    Regenerate {
        to: Recipient,
    },
    Exile {
        what: Vec<Recipient>,
    },
    /// `exile target creature until this leaves`: "... until this
    /// enchantment leaves the battlefield" (CR 610.3).
    ExileUntil {
        what: Recipient,
    },
    ExileAllGraveyards,
    /// `exile graveyard of target player`: "Exile target player's
    /// graveyard."
    ExileGraveyard {
        who: Recipient,
    },
    Sacrifice {
        what: Recipient,
    },
    /// `that player sacrifices that creature`: a sacrifice of one named
    /// permanent, worded with its controller as the subject (the same
    /// effect, CR 701.21a: only its controller can sacrifice it).
    SacrificeBy {
        who: Recipient,
        what: Recipient,
    },
    Tap {
        to: Vec<Recipient>,
    },
    /// `tap or untap target creature`.
    /// `next spell costs {1} less`: "the next spell you cast this turn
    /// costs {1} less to cast".
    NextSpellLess(u32),
    /// `face-down spells cost {1} less this turn`: "face-down spells you
    /// cast this turn cost {1} less to cast".
    FaceDownSpellsLess(u32),
    TapOrUntap {
        to: Recipient,
    },
    /// `turn face up target face-down creature opponent controls` (CR
    /// 708.8): "Turn target face-down creature an opponent controls face
    /// up."
    TurnFaceUp {
        what: Recipient,
    },
    /// `turn face down target creature with morph` (CR 708.2a): "Turn
    /// target creature with a morph ability face down."
    TurnFaceDown {
        what: Recipient,
    },
    /// `look at face-down target face-down creature` (CR 708.5): "Look at
    /// target face-down creature."
    LookAtFaceDown {
        what: Recipient,
    },
    /// `look at hand top and face-down of target player`: "Look at target
    /// player's hand, the top card of that player's library, and any
    /// face-down creatures they control."
    LookAtPlayer {
        who: Recipient,
    },
    /// `goad target creature` (CR 701.15).
    Goad {
        what: Recipient,
    },
    /// `detain target creature an opponent controls` (CR 701.35).
    Detain {
        what: Recipient,
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
    /// `attach to <to>` (this), `attach <what> to <to>`.
    Attach {
        what: Recipient,
        to: Recipient,
    },
    /// `unattach all Equipment attached to target creature`: "Unattach all
    /// Equipment from target creature".
    Unattach {
        what: Recipient,
    },
    /// `create 2 "Soldier"`; `create 1 tapped "Powerstone"`.
    CreateToken {
        token: String,
        count: Num,
        tapped: bool,
        /// Who creates them, worded with its subject: `you create` ("...
        /// sacrifices it and you create a 1/1 ... token"), `its controller
        /// creates` ("Its controller creates a Treasure token"), `each
        /// player creates`. `None` is "create", by the ability's
        /// controller.
        who: Option<Recipient>,
        /// `it has`: the token's abilities print after the sentence ("...
        /// creature token. It has \"Sacrifice this token: Add {C}.\"";
        /// "They have" for more than one) instead of "with ...".
        it_has: bool,
        /// `attached to up to 1 target creature you control`: an Aura token
        /// (a Role, CR 303.7) enters attached to it.
        attached: Option<Recipient>,
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
    /// `counter target spell [to exile | to library top]`.
    Counter {
        to: Recipient,
        dest: CounteredTo,
    },
    /// `counter target spell unless its controller pays {X} [count ...]
    /// [to exile]`: an {X} is `x` (the spell's X when no count is given).
    CounterUnlessPays {
        to: Recipient,
        mana: ManaCost,
        x: Num,
        dest: CounteredTo,
    },
    Scry(Num),
    /// `each player may scry 1`, `each opponent may scry 1`: each in turn
    /// order scries their own library. `may` is wording: declining to scry
    /// leaves the library as scrying and keeping every card on top does.
    ScryEach {
        who: Recipient,
        count: Num,
        may: bool,
    },
    /// "Investigate" (CR 701.16): create a Clue token; `investigate
    /// twice` ("investigate twice") does it two times.
    Investigate(u32),
    /// `target player investigates`: another player creates the Clue
    /// ("Target player investigates.").
    Investigates(Recipient),
    /// `put target creature on top of library`, `on bottom of library`,
    /// `into library 2 from top`; `owner puts target creature on top or
    /// bottom of library` ("Target creature's owner puts it on their
    /// choice ..."); `owner of target creature puts it into library 2
    /// from top or bottom`; `shuffle enchanted creature into library`.
    PutInLibrary {
        what: Recipient,
        place: LibraryPlace,
        wording: LibraryWording,
    },
    /// `each opponent sacrifices 1 creature`: "Each opponent sacrifices a
    /// creature of their choice."
    SacrificeChosen {
        who: Recipient,
        count: Num,
        filter: Filter,
        /// `target player sacrifices 1 artifact and 1 land`: "sacrifices an
        /// artifact and a land of their choice" (two choices, one
        /// sentence).
        and: Option<Box<(Num, Filter)>>,
        /// `sacrifices 1 creature of their choice with flying`: the filter's
        /// "with" qualities print after "of their choice".
        choice_first: bool,
    },
    /// `return 1 land you control`: "return a land you control to its
    /// owner's hand" (the controller chooses).
    ReturnChosen {
        count: Num,
        filter: Filter,
        /// `each player returns 1 creature they control`: "Each player
        /// returns a creature they control to its owner's hand", each
        /// choosing one of their own (CR 101.4).
        who: Option<Recipient>,
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
    /// `target opponent reveals 1 card at random from hand`: "reveals a
    /// card at random from their hand".
    RevealRandom {
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
        /// `None`: `discard 1 card at random` ("discard a card at random",
        /// the controller).
        who: Option<Recipient>,
        /// `discards another card at random`: wording for a second one.
        another: bool,
    },
    /// `roll d20`, then a `result 1 to 9: <effects>` line per row of its
    /// results table: "Roll a d20." and "1—9 | ..." paragraphs (CR 706.3).
    /// The rows are the ability's last paragraphs.
    RollDie {
        sides: u32,
        rows: Vec<DieRow>,
    },
    /// `choose target creature an opponent controls`: "Choose target
    /// creature ..., then roll a d20.", the target named before the roll
    /// whose results act on it. Wording only: the target is the ability's.
    ChooseTarget(Recipient),
    /// `vote sprout or harvest`: "starting with you, each player votes for
    /// sprout or harvest" (CR 701.38); `count each votes for sprout` counts
    /// the votes.
    Vote(Vec<String>),
    /// `vote return artifact or creature or enchantment card`: "starting
    /// with you, each player votes for an artifact, creature, or
    /// enchantment card in your graveyard. Return each card with the most
    /// votes or tied for most votes to your hand."
    VoteReturn(Filter),
    /// `flip coin`: "flip a coin" (CR 705); what happens if the flip is won
    /// is the next sentence, `if you win the flip, <sentence>`.
    FlipCoin,
    /// `if you win the flip, <sentence>`, after `flip coin`.
    IfWonFlip(Vec<Sentence>),
    /// `clash`: "clash with an opponent" (CR 701.23); what happens if the
    /// clash is won is the next sentence, `if you win, <sentence>`.
    Clash,
    /// `if you win, <sentence>`, after `clash`.
    IfWonClash(Vec<Sentence>),
    /// `draw 2 cards then discard 1 of them`: "Draw two cards, then
    /// discard one of them." (only cards drawn this way).
    DrawDiscardDrawn {
        draw: Num,
        discard: Num,
    },
    /// `target opponent exiles 1 creature they control`: "Target opponent
    /// exiles a creature they control" (the player chooses).
    ExileChosen {
        who: Recipient,
        count: Num,
        filter: Filter,
    },
    /// `target opponent exiles 2 cards from hand`: "Target opponent exiles
    /// two cards from their hand" (the player chooses); `... from
    /// graveyard`: "from their graveyard".
    ExileFromHand {
        count: Num,
        who: Recipient,
        graveyard: bool,
    },
    /// `target opponent reveals hand exiling nonland card from graveyard
    /// or hand`: "Target opponent reveals their hand. You choose a nonland
    /// card from that player's graveyard or hand and exile it."
    ExileFromGraveyardOrHand {
        who: Recipient,
        filter: Filter,
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
        /// `reveals hand exiling nonland card`: "You choose a nonland card
        /// from it and exile that card." (`Some(true)`); with `split`
        /// after it, "You choose a nonland card from it. Exile that card."
        /// (`Some(false)`).
        exile: Option<bool>,
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
        /// `search basic land card or Gate card ...`: the filter's
        /// alternative printed as its own card noun ("a basic land card or
        /// a Gate card").
        or_card: OrCard,
        /// `its controller may search ...`: another player searches their
        /// own library ("Its controller may search their library for
        /// ..."); `None` is you.
        who: Option<Recipient>,
        /// `<who> may search`: the searcher may choose not to.
        may: bool,
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
        count: Num,
        colors: mtg_core::mana::ColorSet,
        different: bool,
    },
    /// `add any that land opponent controls could produce`, `add any that
    /// basic land you control could produce`: "Add one mana of any color
    /// that a land an opponent controls could produce." (CR 106.7).
    AddManaLandCould {
        whose: Who,
        basic: bool,
    },
    Surveil(Num),
    /// `learn` (CR 701.48): there is no sideboard here, so "you may
    /// discard a card; if you do, draw a card" (CR 701.48a's other
    /// choice; no Lesson card is ever outside the game).
    Learn,
    /// `double this power`: "double its power until end of turn" (CR
    /// 701.10f: it gets +X/+0, X its power); `times` is the count of its
    /// power.
    DoublePower {
        to: Recipient,
        times: Num,
    },
    /// `who` is `None` for the imperative "discard a card".
    Discard {
        count: Num,
        who: Option<Recipient>,
    },
    /// `put 1 card from hand on bottom of library`: "put a card from your
    /// hand on the bottom of your library"; `target opponent puts 1 card
    /// from hand on top of library`: "Target opponent puts a card from
    /// their hand on top of their library". Two or more on top print "in
    /// any order". `who` is `None` for the imperative. With `shuffle`,
    /// `shuffle 1 card from hand into library`: "shuffle a card from your
    /// hand into your library".
    HandToLibrary {
        count: Num,
        who: Option<Recipient>,
        bottom: bool,
        shuffle: bool,
    },
    /// `shuffle graveyard into library` ("Shuffle your graveyard into your
    /// library"), `target player shuffles graveyard into library` ("Target
    /// player shuffles their graveyard into their library"), `shuffle all
    /// creature card from graveyard of target player into library`
    /// ("Shuffle all creature cards from target player's graveyard into
    /// that player's library"), `shuffle cards from hand into library`
    /// ("shuffle the cards from your hand into your library"). `who` is the
    /// subject, `of` the player whose zone it is in the imperative.
    ShuffleZone {
        who: Option<Recipient>,
        of: Option<Recipient>,
        hand: bool,
        filter: Option<Filter>,
        cards: bool,
    },
    Fight {
        a: Recipient,
        b: Recipient,
    },
    /// `<what> bites <what>`: "deals damage equal to its power to"; `bites
    /// itself` when `to == source`; `bites twice <what>` is "deals damage
    /// equal to twice its power to".
    Bite {
        source: Recipient,
        to: Recipient,
        twice: bool,
    },
    /// `this becomes 4/4 green Bear artifact creature`, `target creature
    /// becomes Dragon base 4/4`, `this becomes artifact creature`, `this
    /// becomes 2/2 Gargoyle creature with flying indefinitely`.
    Animate {
        to: Recipient,
        types: Vec<CardType>,
        subtypes: Vec<String>,
        /// Its base power and toughness, if it gets new ones.
        pt: Option<(i32, i32)>,
        /// The P/T written `base 5/5`: "with base power and toughness 5/5"
        /// after the types.
        base: bool,
        /// Colors it becomes, if any ("4/4 green Bear artifact creature").
        colors: Vec<Color>,
        /// `with flying`: "a 2/2 Gargoyle creature with flying".
        keywords: Vec<Keyword>,
        /// `indefinitely`: no duration (Opal Caryatid).
        indefinite: bool,
        /// `loses all abilities and becomes green Snake base 1/1`
        /// (Snakeform).
        loses_abilities: bool,
        /// `becomes black Demon in addition`: "in addition to its other
        /// colors and types".
        in_addition: bool,
    },
    /// `this becomes blue`, `becomes colorless` (until end of turn).
    BecomeColor {
        to: Recipient,
        colors: Vec<Color>,
        /// `indefinitely`: no duration.
        indefinite: bool,
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
        /// `becomes creature type of choice in addition`: "... in addition
        /// to its other types".
        in_addition: bool,
    },
    YouMay(Vec<Sentence>),
    /// `that player may pay {4}`: "that player may pay {4}" (the player
    /// decides and acts); a following `if they do` prints "If the player
    /// does".
    PlayerMay {
        who: Recipient,
        inner: Vec<Sentence>,
    },
    /// "If this spell was kicked, ..." (CR 702.33c).
    IfKicked(Vec<Sentence>),
    /// `each player puts 1 creature card from graveyard onto battlefield`:
    /// "Each player puts a creature card from their graveyard onto the
    /// battlefield."
    EachPutsFromGraveyard {
        who: Recipient,
        filter: Filter,
    },
    /// `for each opponent who can't, you gain 4 life` (after `each
    /// opponent discards 1 card`): the sentence once for each player who
    /// couldn't. `opponent` is the word printed ("opponent" or "player").
    ForEachCouldnt {
        opponent: bool,
        inner: Vec<Sentence>,
    },
    /// `if target 1 was Human, ...`: "If that creature was a Human, ...";
    /// `if target 1 is Vampire, also ...`: "If it's a Vampire, it also
    /// ..." (the target as it resolves).
    IfTarget(usize, Filter, Vec<Sentence>, TargetCheck),
    /// `if you control Treefolk, draw 1 card`: a static condition,
    /// judged as it resolves. With `also` after the comma (`if that spell
    /// spent 5 mana, also this gains double strike`), "this creature also
    /// gains double strike". `<sentence> if <condition>` prints the
    /// condition last ("Draw a card if you control a creature with a +1/+1
    /// counter on it.").
    IfCondition(StaticCond, Vec<Sentence>, IfWording),
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
    /// `otherwise, ...`: only if the conditional sentence just before (`if
    /// <condition>, ...`, `if you win, ...`) didn't happen (CR 608.2c).
    Otherwise(Vec<Sentence>),
    /// `when you do, <sentence>`: a reflexive triggered ability (CR
    /// 603.12) with targets of its own, chosen as it triggers.
    WhenYouDo {
        targets: Vec<Target>,
        effects: Vec<Sentence>,
    },
    /// `<clause> unless you pay {2}`, `... unless they pay {2}`: the payer
    /// may pay to stop it.
    UnlessPays {
        payer: Recipient,
        mana: ManaCost,
        life: u32,
        /// `unless that player discards 1 card`: cards the payer
        /// discards, of their choice, as the payment.
        discard: u32,
        effect: Box<Effect>,
    },
    /// `<clause> unless you <clause>`: "sacrifice it unless you discard
    /// a card at random", "... unless you return two Forests you control
    /// to their owner's hand" (the controller may act to stop it).
    UnlessYou {
        act: Box<Effect>,
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
        /// `when that creature dies this turn` / `when that creature leaves
        /// the battlefield this turn` (CR 603.7c): the trigger is
        /// `Dies(This)` / `Leaves(This)` about the recipient, and ends with
        /// the turn.
        this_turn: bool,
    },
    /// A replacement or prevention effect that lasts until end of turn.
    CreateReplacement(Replacement),
    Rule(RuleKind),
    /// `target player can't play lands this turn`, `... can't cast
    /// creature spells this turn`.
    PlayerRule {
        who: Recipient,
        kind: mtg_core::card::PlayerRuleKind,
    },
    /// `proliferate` (CR 701.27).
    Proliferate,
    /// `the ring tempts you` (CR 701.54): "the Ring tempts you".
    RingTempts,
    /// `become monarch` (CR 725): "you become the monarch".
    BecomeMonarch,
    /// `take initiative` (CR 726): "you take the initiative".
    TakeInitiative,
    /// `venture` (CR 701.49): "venture into the dungeon"; `venture into
    /// undercity`: "venture into Undercity".
    Venture {
        undercity: bool,
    },
    /// `this explores` (CR 701.44).
    Explore {
        what: Recipient,
    },
    /// `target creature phases out` (CR 702.26).
    PhaseOut {
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
    /// `manifest dread` (CR 701.62).
    ManifestDread,
    /// `blight 2` (CR 701.68).
    Blight(u32),
    /// `adapt 2` (CR 701.46).
    Adapt(u32),
    /// `bolster 2` (CR 701.39).
    Bolster(Num),
    /// `monstrosity 3` (CR 701.37): "Monstrosity 3."
    Monstrosity(Num),
    /// `support 2` (CR 701.41a): a +1/+1 counter on each of up to N target
    /// creatures, other than the source in a triggered ability (support on
    /// a permanent); the recipients are those targets.
    Support(Vec<Recipient>),
    /// `populate` (CR 701.36).
    Populate,
    /// `incubate 2` (CR 701.53): the token is "Incubator".
    Incubate(Num),
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
        /// `take 2`, `take any`: up to that many ("Put up to two instant
        /// and/or sorcery cards", "any number of creature cards";
        /// `ANY_NUMBER`); without a filter, exactly that many ("Put two of
        /// those cards into your hand").
        take: u32,
        /// `take creature card and land card`: "a creature card and/or a
        /// land card".
        also: Option<Filter>,
        /// `rest bottom random`: "in a random order".
        random: bool,
        /// `dig 3 then take card rest graveyard`: "look at the top three
        /// cards of your library, then put one of them into your hand and
        /// the rest into your graveyard" (one sentence).
        then: bool,
        /// `take card may top`: "You may put one of those cards back on
        /// top of your library" (it stays there; the rest go).
        top: bool,
        /// `reword`, wording only: "two of them" for "two of those cards",
        /// "one of those cards" for "one of them", and "Then put the rest"
        /// after a card revealed and put into your hand.
        reword: bool,
    },
    /// `look at top 3`: "look at the top three cards of your library, then
    /// put them back in any order"; `look at top 3 of target player`:
    /// "... of target player's library, then put them back in any order";
    /// `only` ends after the look ("Look at the top three cards of target
    /// player's library."); with a subject (`by`), that player looks:
    /// `that player looks at top 3 of you` ("That player looks at the top
    /// three cards of your library, then puts them back in any order").
    LookAtTop {
        count: Num,
        of: Option<Recipient>,
        by: Option<Recipient>,
        only: bool,
    },
    /// `reveal top card` ("reveal the top card of your library"), `look at
    /// top card` ("look at the top card of your library"), `<who> reveals
    /// top card` ("target player reveals the top card of their library"),
    /// `reveal top card of <who>` ("Reveal the top card of target
    /// opponent's library"): the card is `it` / `that card` after it.
    RevealTop {
        who: Option<Recipient>,
        look: bool,
        of: bool,
    },
    /// `reveal until nonland card` ("reveal cards from the top of your
    /// library until you reveal a nonland card"), `<who> reveals until
    /// land card` ("... until they reveal a land card"): the matching card
    /// is `that card` after it, the revealed cards `all revealed`.
    RevealUntil {
        who: Option<Recipient>,
        filter: Filter,
    },
    /// `put its counters on <what>` ("put its counters on target creature
    /// you control": the source's, as it last existed), `move all counters
    /// from <what> onto <what>`, `move 1 +1/+1 counter from this onto
    /// <what>` (CR 122.5).
    MoveCounters {
        from: Recipient,
        to: Recipient,
        kind: Option<CounterKind>,
        count: Option<u32>,
    },
    /// `<who> shuffles` ("Then that player shuffles", CR 701.24).
    Shuffle {
        who: Recipient,
    },
    /// `if it is land card, <sentence>` ("If it's a land card, ..."), `if
    /// it isnt land card, ...` ("If it isn't a land card"), `if that card
    /// is Goblin card, ...` ("If that card is a Goblin card"), after a
    /// card is revealed or looked at; `card` is whether "card" is written
    /// ("If it's a land, ...").
    IfCard {
        filter: Filter,
        negated: bool,
        card: bool,
        that: bool,
        /// `if it shares creature type with this, ...`: "If it shares a
        /// creature type with this creature, ..." (kinship).
        shares_type: bool,
        then: Vec<Sentence>,
    },
    /// `if you dont, <sentence>` / `if you cant, <sentence>`: "If you
    /// don't, ..." after an optional action ("You may pay {U}. If you
    /// don't, ..."), "If you can't, ..." after one that must happen if it
    /// can ("return a land card from your graveyard to your hand. If you
    /// can't, ..."): only if it didn't happen (CR 608.2c).
    IfYouDont {
        cant: bool,
        then: Vec<Sentence>,
    },
    /// `if you dont put it into hand, <sentence>`: "If you don't put the
    /// card into your hand, ..." after `if it is ...` (an "otherwise").
    IfYouDontPut {
        to: mtg_core::card::CardDest,
        then: Vec<Sentence>,
    },
    /// `reveal it`: "reveal it", the card looked at before.
    RevealIt,
    /// `draw 1 card and reveal it`: "Draw a card and reveal it." (it is
    /// `it` after).
    DrawReveal,
    /// `discard it`: "discard it", the card drawn and revealed before.
    DiscardIt,
    /// `put it into hand`, `put that card onto battlefield tapped`, `put
    /// all revealed into hand`, `<who> puts those revealed into
    /// graveyard`, `put the revealed on bottom of library`: the card or
    /// cards revealed or looked at before.
    PutCard {
        what: CardWords,
        to: mtg_core::card::CardDest,
        who: Option<Recipient>,
        /// `into graveyard of that player`: "into that player's
        /// graveyard" (the card's owner's, where it goes anyway).
        that_player: bool,
    },
    /// `impulse 2 next turn first`: "Exile the top two cards of your
    /// library. Until the end of your next turn, you may play those
    /// cards." `impulse 1 this turn`: "... You may play that card this
    /// turn."; `it` prints "it" for "that card"; `next end step` is "until
    /// your next end step".
    ExilePlayable {
        count: Num,
        until: mtg_core::card::PlayUntil,
        /// The duration before "you may play".
        first: bool,
        it: bool,
    },
    /// `choose one { mode: ... }` as a triggered ability's effect: "When
    /// this creature enters, choose one —" (CR 603.3c).
    Modes(Vec<Mode>),
    /// `target 1 doesn't untap next untap step`: "It doesn't untap during
    /// its controller's next untap step."; `... next untap steps`: "Those
    /// creatures don't untap during their controllers' next untap steps."
    /// `target player skips next draw step` ("Target player skips their
    /// next draw step"), `target opponent skips next combat phase`, `...
    /// combat phase this turn`.
    SkipNext {
        who: Recipient,
        what: mtg_core::card::SkipWhat,
    },
    SkipNextUntap {
        to: Recipient,
        steps: bool,
    },
    /// `that creature can't be regenerated`: "That creature can't be
    /// regenerated this turn."
    CantBeRegenerated {
        to: Recipient,
    },
    /// `the damage can't be prevented`: "The damage can't be prevented."
    /// (this spell's damage, CR 615.12).
    DamageUnpreventable,
    /// `excess damage to its controller instead`: "Excess damage is dealt
    /// to that creature's controller instead." (CR 120.4a).
    ExcessToController,
    /// `this assigns no combat damage this turn`: "This creature assigns
    /// no combat damage this turn."
    AssignsNoCombatDamage {
        to: Recipient,
    },
    /// `target creature blocks this if able`: "Target creature blocks this
    /// creature this turn if able."
    BlocksIfAble {
        blocker: Recipient,
        attacker: Recipient,
    },
    /// `target creature can't block this`: "Target creature can't block
    /// this creature this turn."
    CantBlockThis {
        blocker: Recipient,
        attacker: Recipient,
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
    /// `target permanent you control gains protection from artifact or
    /// from color of choice`: "gains protection from artifacts or from the
    /// color of your choice until end of turn" (CR 702.16): the controller
    /// chooses one as it resolves.
    ProtectionOr {
        to: Recipient,
        filter: Filter,
    },
    /// `target creature gains all creature types` (`all`), `target
    /// creature loses all creature types`, until end of turn (layer 4).
    CreatureTypes {
        to: Recipient,
        all: bool,
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
    /// `if excess damage dealt this way, <sentence>`: "If excess damage
    /// was dealt this way, ..." (CR 120.4a).
    IfExcessDamage(Vec<Sentence>),
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
        /// `MilledToHand` only: `from among milled this way cards`, "from
        /// among the cards milled this way".
        this_way: bool,
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
        ThisWay::Tapped => "tapped",
        ThisWay::Shuffled => "shuffled",
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
        ThisWay::Shuffled,
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
