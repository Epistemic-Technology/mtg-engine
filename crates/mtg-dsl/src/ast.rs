//! The card DSL's syntax tree. It mirrors `mtg_core::card` closely but keeps
//! names where the engine keeps interned ids (subtypes, tokens, emblems),
//! keeps the author's order and sentence structure for the Oracle-text
//! printer, and knows nothing about a card pool.

use mtg_core::card::{
    CardType, Condition, CounterKind, Keyword, Num, PtValue, Relation, Replacement as Outcome,
    RuleKind, Supertype, TargetSpec, Who,
};
pub use mtg_core::card::{CounteredTo, LibraryPlace};

/// One choice of `Ability::BecomesChoice`: "a 1/6 Wall artifact creature
/// with defender".
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct ChoiceForm {
    pub power: i32,
    pub toughness: i32,
    pub subtypes: Vec<String>,
    pub types: Vec<mtg_core::card::CardType>,
    pub keywords: Vec<mtg_core::card::Keyword>,
}

/// What a permission to play or cast a card adds (`mtg_core::card::CastRider`),
/// with its wording: `any type` (", and mana of any type can be spent to
/// cast that spell"), `any type it` ("... to cast it"), `spend any color`
/// (", and you may spend mana as though it were mana of any color to cast
/// that spell"), `spend any type this way` (". If you cast a spell this
/// way, you may spend mana as though it were mana of any type to cast
/// it"), `costs {2} more` (". A spell cast this way costs {2} more to
/// cast"; `costs {2} more opponent`: "A spell cast by an opponent this way
/// ...", only for an owner who is an opponent).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct Rider {
    pub spend: mtg_core::card::SpendAs,
    pub wording: SpendWording,
    pub more: u8,
    pub more_opponents: bool,
    /// `exile instead`: ". If that spell would be put into a graveyard,
    /// exile it instead".
    pub exile: bool,
    /// `pay life`: ". If you cast a spell this way, pay life equal to its
    /// mana value rather than pay its mana cost".
    pub life: bool,
}

/// How a `Rider` words its spending.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum SpendWording {
    /// "mana of any type can be spent to cast that spell" (or "them").
    #[default]
    CanBeSpent,
    /// "... can be spent to cast it".
    CanBeSpentIt,
    /// "you may spend mana as though it were mana of any color to cast
    /// that spell".
    AsThough,
    /// "If you cast a spell this way, you may spend mana as though it
    /// were mana of any type to cast it."
    ThisWay,
}

/// How `Effect::IfTarget` words its check.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // each is one printed wording
pub struct TargetCheck {
    pub tense: TargetTense,
    /// `also` after the comma: "it also gains lifelink".
    pub also: bool,
    /// Written after the sentence: `put 1 +1/+1 counter on target creature
    /// you control if target 1 is legendary` ("... if it's legendary").
    pub after: bool,
    /// `if target 1 isnt creature` ("If it isn't a creature") and `if
    /// target 1 is not creature` ("If it's not a creature"): the check is
    /// the filter with those types negated (`noncreature`). Wording only.
    pub not: Option<NotWording>,
    /// `if target 1 wasnt attacking` ("If it wasn't attacking"), `if that
    /// target 1 wasnt damaged` ("If that creature wasn't dealt damage this
    /// turn"): the sentence happens if the target doesn't match.
    pub negated: bool,
    /// `if that land is Mountain` about the triggering object
    /// (`IT_TRIGGER`): the noun printed after "that".
    pub noun: Option<&'static str>,
    /// `if it was Soldier` after a target: "If it was a Soldier" (Zephyr
    /// Sentinel) for `if target 1 was`'s "If that creature was". Wording
    /// only; the check is `Was` on the last object target.
    pub it: bool,
}

/// `Effect::IfTarget`'s index for a check of the triggering object, not a
/// target: `if it is Spider, ...` ("If it's a Spider, ..."), `if that land
/// is Mountain, ...`, `draw 1 card if it was attacking`.
pub const IT_TRIGGER: usize = usize::MAX;

/// How a negated target check prints its "not".
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum NotWording {
    /// `isnt`: "isn't".
    Isnt,
    /// `is not`: "'s not".
    IsNot,
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
    /// `if that target 1 was with mana value 2 or less`: "If that spell's
    /// mana value was 2 or less", as it last existed, like `Was`.
    ThatWas,
    /// `if target 1 had with mana value 3 or less`: "If it had mana value 3
    /// or less", as it last existed, like `Was`.
    Had,
    /// `if the full target 1 is that entered this turn`: "If the creature
    /// you control entered this turn", as it is now, like `Is`.
    TheFullIs,
}

/// How `Effect::IfYouDont` words what didn't happen.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum DontWords {
    /// `if you dont, ...`: "If you don't, ...".
    #[default]
    Plain,
    /// `if you dont draw this way, ...`: "If you don't draw a card this
    /// way, ..." (Trade Route Envoy).
    Draw,
    /// `if you dont put into hand this way, ...`: "If you don't put a card
    /// into your hand this way, ..." (Courier of Comestibles).
    PutHand,
    /// `if you didnt put into hand this way, ...`: "If you didn't put a
    /// card into your hand this way, ..." (Contagious Vorrac).
    DidntPutHand,
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
    /// `put exiled card into hand unless cast`: "Put the exiled card into
    /// your hand if it wasn't cast this way."
    ExiledUnlessCast,
    /// `put exiled card into hand if not cast`: "If you don't, put that
    /// card into your hand." (Discover the Impossible), the same.
    ExiledIfNotCast,
    /// `put exiled card into hand otherwise`: "Otherwise, put it into
    /// your hand." (Bre of Clan Stoutarm), the same.
    ExiledOtherwise,
    /// `put exiled card into hand if not cast this way`: "If you don't
    /// cast that card this way, put it into your hand." (Solstice
    /// Revelations), the same.
    ExiledIfNotCastThisWay,
}

/// How `Effect::CastFree`'s "exile it instead" is worded (wording only).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum CastExileWords {
    /// `exile instead`: "If that spell would be put into your graveyard,
    /// exile it instead."
    #[default]
    Your,
    /// `exile instead a`: "If that spell would be put into a graveyard,
    /// exile it instead."
    A,
    /// `exile instead this way`: "If a spell cast this way would be put
    /// into a graveyard, exile it instead." (Jetsam).
    ThisWay,
}

/// How `Effect::CastAmong` names the cards it may cast.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AmongWords {
    /// `it`: "it" (the resolution's card).
    It,
    /// `that card`: "that card".
    ThatCard,
    /// `exiled card`: "the exiled card".
    ExiledCard,
    /// `from among them`: "from among them" (the cards exiled).
    Them,
    /// `from among those cards`: "from among those cards".
    Those,
    /// `from among exiled cards`: "from among the exiled cards".
    TheExiled,
}

impl AmongWords {
    /// Whether these words name the cards exiled, not the one card.
    pub const fn many(self) -> bool {
        matches!(self, Self::Them | Self::Those | Self::TheExiled)
    }
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
    /// `target player shuffles up to 3 target card from their graveyard
    /// into library`: "Target player shuffles up to three target cards
    /// from their graveyard into their library" (the player names the
    /// cards' owner; wording only).
    By(Recipient),
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
#[allow(clippy::struct_excessive_bools)] // independent permissions and wordings
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
    /// `spells`, `abilities`: the plural wording; `joined and`.
    pub plural: mtg_core::card::SpendPlural,
    /// `of artifact bare`: "abilities of artifacts", without "sources".
    pub bare: bool,
    /// `plus unlock`, `plus face up`, `from exile`, `uncounterable`, ...:
    /// see `mtg_core::card::SpendWays`.
    pub ways: mtg_core::card::SpendWays,
    /// `or x spells`: "or spells with {X} in their mana costs".
    pub x_spells: Option<Box<Filter>>,
}

/// The pronoun a character's text uses for it after its name: `pronoun
/// he` ("He fights up to one target creature", "shuffles him into their
/// library"), `pronoun she`. Wording only.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Pronoun {
    He,
    She,
    /// `pronoun they`: a plural name ("Whenever Bebop & Rocksteady
    /// attack or block").
    They,
}

/// One definition: a `card`, `token`, or `emblem` block.
#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::struct_excessive_bools)] // each is one printed wording
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
    /// `additional cost may collect evidence 6`: "As an additional cost to
    /// cast this spell, you may collect evidence 6." (CR 601.2b).
    pub optional_cost: Option<Cost>,
    /// `alternative cost sacrifice Mountain`: "You may sacrifice a
    /// Mountain rather than pay this spell's mana cost." (CR 118.9).
    pub alt_cost: Option<AltCost>,
    /// `flash as long as you control Merfolk`: "This spell has flash as
    /// long as you control a Merfolk." (CR 702.8).
    pub flash_if: Option<StaticCond>,
    /// `flash if you control Human`: the same, printed "You may cast this
    /// spell as though it had flash if you control a Human."
    /// `flash named as long as you control Desert`: "Colossal Rattlewurm
    /// has flash as long as you control a Desert." (the card's name, not
    /// "This spell"); wording only. With `flash_if_may` (`flash first if
    /// ...`): the condition first, "If ..., you may cast this spell as
    /// though it had flash." (Serpent of the Pass).
    pub flash_if_named: bool,
    pub flash_if_may: bool,
    /// `flash with cleanup sacrifice`: "You may cast this spell as though
    /// it had flash. If you cast it any time a sorcery couldn't have been
    /// cast, the controller of the permanent it becomes sacrifices it at
    /// the beginning of the next cleanup step."
    pub flash_cleanup: bool,
    /// `pronoun he`: how the text refers to the card after naming it.
    pub pronoun: Option<Pronoun>,
    /// `short_name "Ajani"`: the name a legendary card's text uses for
    /// itself when it isn't the part before a comma ("put a loyalty
    /// counter on Ajani" on Ajani Resolute; CR 201.5).
    pub short_name: Option<String>,
    /// Derived as a spell is parsed: how many of its targets come before
    /// those an "if kicked" sentence declares (Orim's Thunder's "target
    /// creature" is chosen only if the spell is kicked, CR 601.2c).
    pub unkicked_targets: Option<usize>,
    /// Derived likewise: how many targets come before those an "if kicked,
    /// instead ..." sentence declares, which a kicked spell has in their
    /// place (Divine Resilience, CR 601.2c).
    pub kicked_drops: Option<usize>,
    /// `cast only during combat`, `cast only if you control creature`:
    /// "Cast this spell only during combat." (CR 601.3).
    pub cast_only: Option<StaticCond>,
    /// `cast only unless threshold`: "Threshold — You can't cast this
    /// spell unless there are seven or more cards in your graveyard."
    /// (Proft, Sinister Mastermind); wording only.
    pub cast_only_cant: bool,
    /// `kicker {R}` (CR 702.33).
    pub kicker: Option<ManaCost>,
    /// `kicker sacrifice 2 land`: "Kicker—Sacrifice two lands." (CR
    /// 702.33a), a kicker cost other than mana.
    pub kicker_cost: Option<Cost>,
    /// The spell's targets from this index on are declared in its `if
    /// kicked, ...` and chosen only if it is kicked (CR 601.2c); set by
    /// the parser, never written.
    pub kicker_targets: Option<u8>,
    /// `flashback {4}{R}` or `flashback {1}{U}, pay 3 life` (CR 702.34).
    pub flashback: Option<Cost>,
    /// `cast from graveyard ...`: "You may cast this card from your
    /// graveyard ..." (CR 601.3).
    pub graveyard_cast: Option<GraveyardCast>,
    /// `counts as named "Flame Burst" in graveyard`: "If this card is in a
    /// graveyard, effects from spells named Flame Burst count it as a card
    /// named Flame Burst."
    pub graveyard_alias: Option<String>,
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
    /// `count foreach ...`: "For each ..., <clause>" (Brass's Bounty:
    /// "For each land you control, create a Treasure token.").
    ForEach,
    /// `count that many ...`: "draw that many cards".
    ThatMany,
    /// `deal damage equal <expr> to <what>`: "deals damage equal to ... to
    /// <what>", the amount before the recipient.
    EqualBefore,
    /// `sacrifices half up creature`, `discards half up hand`, `mills half
    /// down library`, `loses half down life`: "half the creatures they
    /// control of their choice, rounded up", "half the cards in their
    /// hand", "half their library", "half their life", written right after
    /// a per-player verb (a `Divided` by 2 of the acting player's count).
    Half,
}

/// How a count names the card revealed before (`CountExpr::CardManaValue`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CardWord {
    /// "that card's".
    ThatCard,
    /// "the card's".
    TheCard,
    /// "its".
    Its,
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
    /// `speed`: "your speed" (CR 702.179).
    Speed,
    /// `count each 2 creature you control`: "2 life for each creature
    /// you control".
    Times(u32, Box<CountExpr>),
    /// `half down <count>`, `half up <count>`: "half the number of
    /// artifacts you control, rounded down"; `every 7 <count>`: "for every
    /// seven cards in your graveyard" (rounded down).
    Divided {
        by: u32,
        up: bool,
        inner: Box<CountExpr>,
    },
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
    /// `difference 10 hand`: `n` minus the count, at least 0, printed
    /// "the difference" (after "if you have fewer than ten cards in hand").
    Difference {
        n: u32,
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
    /// `chosen number`: "the chosen number" (`choose number between 0
    /// and 10`).
    ChosenNumber,
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
    /// `creature it devoured`: "for each creature it devoured", "the
    /// number of Goblins it devoured" (CR 702.82c).
    Devoured(Filter),
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
    /// `other instant and sorcery spells cast this turn` ("other instant
    /// and sorcery spell you've cast this turn"); `other instant and
    /// sorcery spells cast before it this turn` ("... you've cast before
    /// it this turn").
    OtherInstantSorceryCast {
        before: bool,
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
    /// revealed before; `mana value of the card`: "the card's mana
    /// value"; `its mana value`: "its mana value".
    CardManaValue(CardWord),
    /// `mana value of exiled card`: "the mana value of the exiled card"
    /// (the source's linked exiled card, CR 607.2a).
    ExiledCardManaValue,
    /// `colors among permanents`: "colors among permanents you control".
    ColorsAmongPermanents,
    /// `colors spent` (converge): "colors of mana spent to cast this
    /// spell" (CR 601.2h). `colors spent spell` and `colors spent
    /// creature` print "this spell" and "this creature" where the default
    /// would be "it" (wording only).
    SpentColors(SpentOf),
    /// `times kicked`: "time(s) it was kicked" (multikicker, CR 702.33d).
    TimesKicked,
    /// `energy paid`: "{E} paid this way" (after `pay any energy`).
    EnergyPaid,
    /// `treasure spent`: "mana from a Treasure spent to cast it".
    SpentTreasure,
    /// `cave spent`: "mana from a Cave spent to cast it".
    SpentCave,
    /// `mana spent`: "the amount of mana spent to cast it"; `mana spent
    /// that spell`: "... to cast that spell", the spell that triggered the
    /// ability (CR 601.2h).
    ManaSpent {
        that: bool,
    },
    /// `opponents graveyards [<filter>]`: "cards in your opponents'
    /// graveyards".
    CardsInOpponentsGraveyards(Option<Filter>),
    /// `exiled with this [<filter>]`: "cards exiled with this
    /// enchantment" (CR 607.2a).
    ExiledWith(Option<Filter>),
    /// `exile [<filter>]`: "cards you own in exile"; `exile and graveyard
    /// [<filter>]`: "... you own in exile and in your graveyard" ("the
    /// total number of ..." as a number).
    CardsInExile {
        filter: Option<Filter>,
        graveyard: bool,
    },
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
    /// existed. `mana value of that spell` (`spell`): "that spell's mana
    /// value", the spell whose casting triggered (Thor, God of Thunder).
    TriggerStat {
        stat: mtg_core::card::Stat,
        that: bool,
        spell: bool,
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
    /// `loyalty of this`: "Nissa's loyalty", the loyalty counters on
    /// this planeswalker (CR 306.5b; `loyalty counters on this` prints
    /// "the number of loyalty counters on Nissa").
    SourceLoyalty,
    /// `damage dealt this way`, `excess damage dealt this way`;
    /// `excess damage dealt to that creature this way` prints "the amount
    /// of excess damage dealt to that creature this way" (wording only).
    DamageThisWay {
        excess: bool,
        to_that: bool,
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
    /// `+1/+1 counters on it` in a triggered ability whose `it` is the
    /// triggering object: that object's counters ("create a Mutagen token
    /// for each +1/+1 counter on it" of the creature that left).
    TriggerCounters(mtg_core::card::CounterKind),
    /// `those creatures` / `them` after a group: how many objects that
    /// group denoted ("You gain 1 life for each of those creatures").
    Those(Those),
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
    /// `life you gained this turn`: "the amount of life you gained this
    /// turn"; `life you've gained this turn`: "... you've gained ...".
    LifeGainedThisTurn {
        ve: bool,
    },
    /// `card types in your graveyard`: "the number of card types among
    /// cards in your graveyard".
    CardTypesInGraveyard,
    /// `counters on this` ("the number of counters on this creature"),
    /// `counters on it`: every kind.
    SourceAllCounters {
        it: bool,
    },
    /// `permanents sacrificed this turn`: "the number of permanents
    /// you've sacrificed this turn".
    SacrificedThisTurn,
    /// `two to x`: "2ˣ".
    TwoToX,
    /// `creature attacked this turn`: "for each creature that attacked this
    /// turn" (any player's).
    AttackedThisTurn,
    /// `opponents lost life this turn`: "for each opponent who lost life
    /// this turn".
    OpponentsLostLife,
    /// `creatures entered this turn`: "the number of creatures that
    /// entered the battlefield under your control this turn".
    CreaturesEnteredThisTurn,
    /// `discarded this turn`: "for each card you've discarded this turn".
    DiscardedThisTurn,
    /// `times descended this turn`, `lands entered this turn`, ...: a
    /// turn-log count worded by its own phrase (`Tally`).
    Tally(Tally),
    /// `opponents more cards in hand`: "for each opponent who has more
    /// cards in hand than you".
    OpponentsMoreCards,
    /// `opponents hand 1 or less`: opponents who have one or fewer cards
    /// in hand.
    OpponentsHandAtMost(u32),
    /// `their hand`, `their library`, `their life`: the count of the
    /// player a per-player effect is acting on ("each player discards half
    /// the cards in their hand").
    Their(mtg_core::card::PlayerCount),
    /// `opponents didn't discard 2 nonland card`: "opponent who didn't
    /// discard two nonland cards this way"; `those opponents didn't
    /// discard 1 card with mana value 4 or greater`: "of those opponents
    /// who didn't discard a card with mana value 4 or greater" (after a
    /// discard by a group of target opponents).
    PlayersDidntDiscard {
        n: u32,
        filter: Option<Filter>,
        those: bool,
    },
    /// `that card` in `has all activated abilities of that card`: the top
    /// card of your library ("As long as the top card of your library is
    /// a Goblin card, this creature has all activated abilities of that
    /// card.", Conspicuous Snoop).
    LibraryTop,
    /// `total power creature you control`: "the total power of creatures
    /// you control" (also `total toughness`, `total mana value`).
    Total(mtg_core::card::Stat, Filter),
    /// `library of target 1`: "the number of cards in that player's
    /// library"; halved, "half their library".
    TargetLibrary(usize),
    /// `greatest mana value among artifact you control`: "the greatest
    /// mana value among artifacts you control".
    GreatestManaValueOf(Filter),
    /// `greatest mana value graveyard [creature]`: "the greatest mana
    /// value among [creature] cards in your graveyard".
    GreatestManaValueInGraveyard(Option<Filter>),
    /// `max 2 <count>`: "2 or <count>, whichever is greater".
    AtLeast(u32, Box<CountExpr>),
    /// `colors of that spell`: "for each of that spell's colors".
    ThatSpellColors,
    /// `damage dealt to it this turn`: "the amount of damage dealt to it
    /// this turn" (the source's).
    SourceDamage,
    /// `creatures left this turn`: "for each creature that left the
    /// battlefield under your control this turn".
    CreaturesLeftThisTurn,
    /// `colors of target 1`: "for each of its colors".
    TargetColors(usize),
    /// `colors among Ally you control`: "for each color among Allies you
    /// control", "the number of colors among ...".
    ColorsAmong(Filter),
    /// `card types among other nonland permanent you control`: "the number
    /// of card types among other nonland permanents you control".
    CardTypesAmong(Filter),
    /// `card types among spells cast this turn`: "for each card type among
    /// spells you've cast this turn".
    CardTypesCastThisTurn,
    /// `card types among discarded this way`: "for each card type among
    /// cards discarded this way".
    CardTypesThisWay(mtg_core::card::ThisWay),
    /// `permanent types in your graveyard`: "permanent types among cards in
    /// your graveyard".
    PermanentTypesInGraveyard,
    /// `graveyards with 7 cards`: "for each graveyard with seven or more
    /// cards in it".
    GraveyardsWithAtLeast(u32),
    /// `+1/+1 counters on land you control`: "the number of +1/+1 counters
    /// on lands you control".
    CountersAmong(mtg_core::card::CounterKind, Filter),
    /// `counter kinds among creature you control`: "different kinds of
    /// counters among creatures you control".
    CounterKindsAmong(Filter),
    /// `different powers among creature you control`: "for each different
    /// power among creatures you control".
    DifferentPowers(Filter),
    /// `most sharing type creature you control`: "the greatest number of
    /// creatures you control that have a creature type in common".
    SharingCreatureType(Filter),
    /// `difference of power and toughness`: "the difference between its
    /// power and toughness" (the triggering object's).
    TriggerPtDifference,
    /// `coins heads`: "the number of coins that came up heads" (after
    /// `flip 5 coins`).
    CoinsHeads,
    /// `x`: X, under a count built on it (`half up x`: "half X, rounded
    /// up").
    X,
    /// `hand of target opponent`: "the number of cards in target
    /// opponent's hand", declaring the target (the `n`th).
    TargetHandDeclared {
        n: usize,
        opponent: bool,
    },
    /// `different mana values among nonland permanent you control`: "for
    /// each different mana value among nonland permanents you control";
    /// `different mana values among graveyard [nonland card]`: "... among
    /// [nonland] cards in your graveyard" (`graveyard`, no filter: every
    /// card).
    DifferentManaValues {
        filter: Option<Filter>,
        graveyard: bool,
    },
    /// `noncreature spells they cast this turn`: "the number of noncreature
    /// spells they've cast this turn", the player the ability triggered
    /// for.
    TheyCast(Filter),
    /// `library to graveyard of target player this turn`: "the number of
    /// cards that were put into target player's graveyard from their
    /// library this turn", declaring the target (the `n`th).
    TargetMilled(usize),
    /// `spells cast this turn but first`: "for each spell you've cast this
    /// turn other than the first".
    SpellsCastButFirst,
    /// `entered Goblin this turn`: "the number of Goblins that entered the
    /// battlefield under your control this turn"; `entered other Zombie
    /// this turn`: "for each other Zombie that entered ...", other than the
    /// one the ability triggered for (that one less).
    EnteredCount {
        filter: Filter,
        other: bool,
    },
    /// `power of the creature that died`: "the power of the creature that
    /// died", the triggering creature's as it last existed (a dies
    /// trigger).
    DiedStat(mtg_core::card::Stat),
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
#[allow(clippy::struct_excessive_bools)] // one per exception line
pub struct Except {
    pub subtypes: Vec<String>,
    pub keywords: Vec<Keyword>,
    pub abilities: Vec<Ability>,
    /// `not legendary`: "it isn't legendary" (CR 707.9b).
    pub not_legendary: bool,
    /// `its not legendary`: the same, worded "it's not legendary" (Ran
    /// and Shaw). Wording only.
    pub its_not: bool,
    /// `types Artifact`: "it's an artifact in addition to its other types".
    pub types: Vec<mtg_core::card::CardType>,
    /// `pt 1/1`: "it's a 1/1 ..." (its base power and toughness).
    pub pt: Option<(i32, i32)>,
    /// `colors red`: "a ... red ... in addition to its other colors and
    /// types".
    pub colors: Vec<mtg_core::mana::Color>,
    /// `legendary`: "it's legendary".
    pub legendary: bool,
    /// `exactly`: "it's a 1/1 green Frog" (its colors and creature types
    /// are these, not added to its own).
    pub exactly: bool,
    /// `the token`: "except the token isn't legendary" (wording only).
    pub the_token: bool,
    /// `and is`: "except it isn't legendary and is a Mutant in addition
    /// to its other types" (The Cloning of Shredder), not "and it's"
    /// (wording only).
    pub and_is: bool,
    /// `named this`: "except its name is <this card's name>" (CR 707.9b).
    pub own_name: bool,
    /// `noun creature`: a spell's token copies' abilities call the token
    /// "this creature" (Face Yourself), not "this token". Wording only.
    pub creature_noun: bool,
}

/// What takes an activated ability's cost down (`reduced {1} each Town you
/// control`, `reduced {4}{B} if drew 3 cards this turn`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ReduceBy {
    Each(CountExpr),
    /// `reduced {1} where differently named land you control`: the same
    /// reduction for each, printed "This ability costs {X} less to
    /// activate, where X is the number of ..." (Survey Mechan).
    Where(CountExpr),
    If(StaticCond),
    /// `reduced {1} if targets creature with power 3 or less`: "This
    /// ability costs {1} less to activate if it targets a creature with
    /// power 3 or less." (the first target).
    Targets(Filter),
    /// `reduced {1} each +1/+1 counter on target`, `reduced {1} each color
    /// of target`: "... for each +1/+1 counter on the creature it
    /// targets", "... for each color of the creature it targets".
    PerTarget(mtg_core::card::PerTarget),
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
    /// `alternative cost {B}, if on battlefield 13 creature late`: the
    /// condition printed last ("You may pay {B} rather than pay this
    /// spell's mana cost if there are thirteen or more creatures on the
    /// battlefield."); wording only.
    pub cond_late: bool,
}

/// `cast from graveyard [while <cond> | if <cond> | at max speed] [for
/// <mana>] [with <cost>]`: "[Max speed — ]You may cast this card from your
/// graveyard [as long as <cond> | if <cond>] [by paying <mana> rather than
/// paying its mana cost | by <cost> in addition to paying its other
/// costs]." (CR 601.3): not exiled as it leaves the stack.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct GraveyardCast {
    pub cond: Option<StaticCond>,
    /// `while`: printed "as long as".
    pub as_long_as: bool,
    /// `at max speed`: "Max speed — " first.
    pub max_speed: bool,
    /// `for {2}{W}`: rather than its mana cost.
    pub mana: Option<ManaCost>,
    /// `with discard 1 card`: in addition to its other costs.
    pub extra: Option<Cost>,
}

/// One alternative of a ward cost (`Mechanic::WardAlts`): a cost, or
/// `pay count ... life` ("Pay life equal to ..."), or `get N poison
/// counters` (`you get`: "You get N poison counters").
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct WardAlt {
    pub cost: Cost,
    pub life: Option<Num>,
    pub poison: u32,
    /// `you get 2 poison counters`: "You get two poison counters".
    pub you: bool,
}

/// A cost (CR 118): of an activated ability or the additional cost of a
/// spell.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // each is one printed cost part
pub struct Cost {
    pub mana: ManaCost,
    /// `x can't be 0` (with the restrictions): "X can't be 0.", printed
    /// after the effects, before "Activate only ...".
    pub x_not_zero: bool,
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
    /// `exile blue card from hand`, `exile 2 white cards from hand`: the
    /// `discard` cards are exiled from the hand instead ("Exile a blue
    /// card from your hand", a pitch spell's alternative cost).
    pub discard_exile: bool,
    /// A loyalty ability's cost (CR 606).
    pub loyalty: Option<i32>,
    /// `loyalty -X`: "−X" (with `loyalty` `Some(0)`), X announced as the
    /// ability is activated.
    pub loyalty_x: bool,
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
    /// `exile this from hand`: "Exile this card from your hand" (an
    /// ability activated from the hand).
    pub exile_hand: bool,
    /// `exile 2 cards from graveyard and this`: "Exile two cards from
    /// your graveyard and this creature" (with `exile_this`), one cost
    /// part.
    pub exile_this_joined: bool,
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
    /// `any player`: "Any player may activate this ability."; `opponents
    /// only`: "Only your opponents may activate this ability." (CR 602.1).
    pub activators: Option<Who>,
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
    /// `reveal this`: "Reveal this card from your hand" (forecast, CR
    /// 702.57a): the ability is activated from the hand.
    pub reveal_this: bool,
    /// `blight 1`: "Blight 1" (CR 701.68a).
    pub blight: u32,
    /// `behold Kithkin`, `behold and exile Elf`, `behold 3 Elemental`,
    /// `may behold Dragon` (CR 701.69a).
    pub behold: Option<Box<Behold>>,
    /// `collect evidence 3`: "Collect evidence 3" (CR 701.59a).
    pub evidence: u32,
    /// `forage`: "Forage" (CR 701.61a).
    pub forage: bool,
    /// `waterbend {3}`: "Waterbend {3}" (CR 701.67a): the mana's generic
    /// part may be paid by tapping artifacts and creatures.
    pub waterbend: bool,
    /// `exert this`: "Exert this creature" (CR 701.43a).
    pub exert: bool,
    /// `discard hand`: "Discard your hand".
    pub discard_hand: bool,
    /// `pay x life`: "pay X life" (a spell's additional cost, X announced
    /// as it is cast).
    pub life_x: bool,
    /// `put 1 -1/-1 counter on creature you control`: "Put a -1/-1 counter
    /// on a creature you control", blight's older wording.
    pub blight_put: bool,
    /// `put 1 card an opponent owns from exile into graveyard`: "Put a
    /// card an opponent owns from exile into that player's graveyard".
    pub process: u32,
    /// `remove 1 +1/+1 counter from creature you control`: the
    /// `remove_counters` from a creature the payer controls.
    pub remove_from_creature: bool,
    /// `remove 2 +1/+1 counters from among artifact you control`,
    /// `remove 6 counters from among creature you control`: the
    /// `remove_counters` from among the payer's matching permanents.
    pub remove_among: Option<Filter>,
    /// `sacrifice any number creature` (1): "sacrifice any number of
    /// creatures"; `may sacrifice any number Spirit` (2): "you may
    /// sacrifice any number of Spirits".
    pub sacrifice_any: u8,
    /// `remove X ki counters from this`: "Remove X ki counters from this
    /// creature", X the ability's.
    pub remove_x: bool,
    /// `remove any number +1/+1 counters from this` (with `remove_x`):
    /// "Remove any number of +1/+1 counters from ~", the ability's X
    /// called "that many" in its effects. Wording only.
    pub remove_any: bool,
    /// `{Q}`: the untap symbol.
    pub untap: bool,
}

/// `behold Kithkin`: "behold a Kithkin" (CR 701.69a); `and exile`: "and
/// exile it"; `may` (a spell's additional cost): "you may behold a
/// Dragon", an optional additional cost.
#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::struct_excessive_bools)] // independent wording flags
pub struct Behold {
    pub filter: Filter,
    pub n: u32,
    pub exile: bool,
    pub optional: bool,
    /// `reveal Kithkin card`: "reveal a Kithkin card from your hand", a
    /// card from the hand only.
    pub reveal: bool,
    /// `reveal X red cards`: "Reveal X red cards from your hand", X the
    /// ability's.
    pub x: bool,
    /// `reveal blue card in hand`: "Reveal a blue card in your hand"
    /// (Dragon's Eye Savants) for "from your hand". Wording only.
    pub in_hand: bool,
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

/// How "copy it" and "you may cast the copy" join (`Effect::CastCopy`).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum CopyCastWording {
    /// "Copy it. You may cast the copy."
    #[default]
    Sentences,
    /// "Copy it, then you may cast the copy ..."
    Then,
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
    /// `{Q}`.
    Untap,
    Sacrifice,
    Discard,
    Life,
    /// `reveal hand`: "reveal your hand".
    RevealHand,
    /// `reveal this`: "Reveal this card from your hand".
    RevealThis,
    /// `reveal this card`: "Reveal this card from your hand" on a creature
    /// card, for `reveal this`'s "this creature" (Sky Hussar).
    RevealThisCard,
    /// `blight 1`.
    Blight,
    /// `behold Kithkin`.
    Behold,
    /// `collect evidence 3`.
    Evidence,
    /// `forage`.
    Forage,
    /// `exert this`.
    Exert,
    /// `discard hand`.
    DiscardHand,
    /// `put 1 card an opponent owns from exile into graveyard`.
    Process,
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::large_enum_variant)] // a syntax tree, not a hot structure
pub enum SacCost {
    This,
    Filter(Filter),
    /// `sacrifice granter`: "Sacrifice Blazing Torch", the Aura or
    /// Equipment granting the ability.
    Granter,
}

/// What a spell targets ("that targets you or a creature").
#[derive(Clone, Eq, PartialEq, Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // each is one printed qualifier
pub struct Targeting {
    pub you: bool,
    pub player: bool,
    pub filter: Option<Filter>,
    /// `that targets an opponent [or ...]`: "an opponent".
    pub opponent: bool,
    /// `with single target`: "with a single target" (nothing else set);
    /// `that targets only single creature you control`: "that targets
    /// only a single creature you control".
    pub single: bool,
    /// `that targets one or more permanent`: "that targets one or more
    /// permanents" (Tiller of Flesh), the same as "a permanent". Wording
    /// only.
    pub one_or_more: bool,
}

/// The subtype name standing for every creature type in a `becomes`
/// (`Subtype::ALL_CREATURE_TYPES`, as `CardPool::subtype_name` names it):
/// "with vigilance and all creature types".
pub const EVERY_CREATURE_TYPE: &str = "(every creature type)";

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
    /// `enchanted permanent`: printed "enchanted permanent" whatever the
    /// Aura enchants (Song of Stupefaction, "Enchant creature or Vehicle").
    pub enchanted_permanent: bool,
    /// `paired` (`paired named`): "as long as this creature (~) is paired
    /// with another creature, both creatures ..." (soulbond, CR 702.95b);
    /// `Some(true)` names the card.
    pub paired: Option<bool>,
    pub no_abilities: bool,
    /// `who` is `Opponents`, written and printed "you don't control".
    pub dont_control: bool,
    /// `who` is `Opponents`, written and printed "your opponents control".
    pub your_opponents: bool,
    /// `who` is `Opponents`, written and printed "they control" after a
    /// recipient that is the opponents ("each opponent and each creature
    /// they control").
    pub they_control: bool,
    /// `who` is `Opponents`, written `that opponent controls` and printed
    /// "that player controls" in a `for each opponent,` sentence ("for
    /// each opponent, tap up to one target creature that player
    /// controls"): the game has one opponent.
    pub per_opponent: bool,
    /// Written `that player controls` and printed so in a `for each
    /// player,` sentence ("for each player, destroy up to one target
    /// creature that player controls"): targets controlled by different
    /// players, one for each player (the game has two).
    pub per_player: bool,
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
    /// `that entered the battlefield this turn`: `status.entered_this_turn`
    /// printed in the long form (Oakhollow Village). Wording only.
    pub entered_long: bool,
    /// `put there from anywhere this turn`: `status.put_there_this_turn`
    /// printed "that was put there from anywhere this turn" rather than
    /// "that was put there this turn".
    pub put_there_anywhere: bool,
    /// `blocked` / `unblocked`, `with aura` / `without aura` ("enchanted
    /// permanent", "that isn't enchanted"), `that entered this turn`,
    /// `that dealt damage this turn`, `that blocked this turn`, `with mana
    /// value X` (`ObjFilter::status`).
    pub status: mtg_core::card::StatusQuals,
    /// `with counters`: `status.counter` any kind, printed "with one or
    /// more counters on it" (Shadow Urchin) rather than "with a counter on
    /// it"; wording only.
    pub counter_many: bool,
    /// `creature or Vehicle`: an alternative filter.
    pub or: Option<Box<Filter>>,
    /// `Knight or a Soldier`: the alternative written with its own article,
    /// printed so in a condition ("you control a Knight or a Soldier").
    /// Wording only.
    pub alt_article: bool,
    /// `land card or Elf card`: each side its own "card" noun ("a land
    /// card or Elf card"). Wording only.
    pub alt_card: bool,
    /// `that targets you or a permanent you control` (spells).
    pub targeting: Option<Box<Targeting>>,
    /// `of chosen type`, `of that type`, `of color of choice`: "of the
    /// chosen type" and kin (`ObjFilter::chosen`).
    pub chosen: Option<OfChosen>,
    /// `named this`, `not named this`, `with same name as target 1`,
    /// `that shares color with target 1` (`ObjFilter::same`).
    pub same: Option<SameRef>,
    /// `named "Keimi"`: "named Keimi", a token's name (CR 201.2; the
    /// token definition keyed so).
    pub named_card: Option<String>,
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
    /// With `that_player` a target: `from their graveyard`, the player
    /// targeted before ("their graveyard"), rather than `from target
    /// player's graveyard`, which declares it ("target player's
    /// graveyard").
    pub their: bool,
    /// `target card in a graveyard`: "card in a graveyard" rather than
    /// "from a graveyard" (Reenact the Crime). Wording only.
    pub in_zone: bool,
    /// `other than enchanted`: "other than enchanted creature"
    /// (`ObjFilter::not_attached`).
    pub not_attached: bool,
    /// `enchanted player controls`: "creatures enchanted player controls"
    /// on a Curse (`ObjFilter::enchanted_player`).
    pub enchanted_player: bool,
    /// `other than this`: "other than this creature", the relation
    /// `Other` worded after the noun (Weed-Pruner Poplar).
    pub other_than_this: bool,
    /// `other than granter`: "an artifact other than Dire Blunderbuss",
    /// not the Equipment granting the quoted ability.
    pub not_granter: bool,
    /// `with basic land type`: "a land card with a basic land type"
    /// (Nervous Gardener): compiled as a Plains, Island, Swamp, Mountain,
    /// or Forest.
    pub basic_land_type: bool,

    /// `that doesn't have same name as token you control`: "that doesn't
    /// have the same name as a token you control" (`ObjFilter::name_unlike`).
    pub name_unlike: Option<Box<Filter>>,
    /// How `status.unchosen` is worded: `unchosen` ("all other
    /// creatures"), `not chosen this way` ("all nonland permanents not
    /// chosen this way"), `except chosen this way` ("all creatures except
    /// creatures chosen this way").
    pub unchosen_wording: UnchosenWording,
    /// `static you and other Hero you control: has hexproof`: "you and
    /// other Heroes you control have hexproof" (Captain America,
    /// Super-Soldier): the controller has the hexproof or shroud too
    /// (`PlayerStaticKind::Hexproof`).
    pub and_you: bool,
    /// `static this and enchantment creature you control: has trample`:
    /// "Yuna and enchantment creatures you control have trample": the
    /// source too, as a second static ability.
    pub and_this: bool,
    /// `andor Villain or artifact`: its alternatives joined "and/or" as a
    /// trigger's subject ("Whenever another Villain and/or artifact you
    /// control enters"); wording only.
    pub and_or: bool,
    /// `andlist Pest or Bat or Spider`: its alternatives joined "and"
    /// after "each" ("put a +1/+1 counter on each Pest, Bat, and Spider
    /// you control"); wording only.
    pub and_list: bool,
    /// `thatare andor creature or Vehicle you control` in a count: "the
    /// number of permanents you control that are creatures and/or
    /// Vehicles" (Collision Course); wording only.
    pub that_are: bool,
    /// `every other creature`: a group printed with "all" as an effect's
    /// subject ("all other creatures get -2/-2"); otherwise as `all`.
    /// Wording only.
    pub all_word: bool,
    /// `whenever a creature dies`: written with its article, a trigger's
    /// subject that could be the source prints so ("Whenever a creature
    /// dies") rather than "this creature or another creature". Wording
    /// only.
    pub article: bool,
    /// `creature with flying an opponent controls late`: who controls it
    /// printed after the qualities ("target creature with flying an
    /// opponent controls") rather than before them ("target creature an
    /// opponent controls with flying"). Wording only.
    pub control_late: bool,
    /// `1/1 creature`: exactly that power and toughness (`power` and
    /// `toughness` both `Exactly`), printed before the noun ("Target 1/1
    /// creature gets +1/+2"). Wording only.
    pub pt_word: bool,
    /// `non-Faerie, nonland permanent`: a negated subtype written before a
    /// negated type prints first ("non-Faerie, nonland permanent",
    /// Nurturing Pixie) rather than after it. Wording only.
    pub subtype_neg_first: bool,
    /// `nontoken green creature`: "nontoken" or a negated supertype
    /// written before a color prints before it ("a nontoken green
    /// creature", "a nonlegendary green creature card") rather than after
    /// it ("a green nontoken creature"). Wording only.
    pub neg_before_color: bool,
    /// `static plural artifact you control: add subtype Food`: a static
    /// ability's subject printed plural even with a type it adds
    /// ("Artifacts you control are Foods in addition to their other
    /// types", Ragost) rather than one at a time ("Each artifact you
    /// control is a Food ..."). Wording only.
    pub plural: bool,
}

impl Filter {
    /// "Equip worthy {1}" (Mjölnir, Hammer of Thor): "A creature is worthy
    /// if it's a legendary non-Villain that's red and/or white", one you
    /// control.
    pub fn worthy() -> Filter {
        Filter {
            who: Some(Who::You),
            types: vec![CardType::Creature],
            supertypes: vec![Supertype::Legendary],
            not_subtypes: vec!["Villain".to_string()],
            colors: Some(vec![Color::Red, Color::White]),
            ..Filter::default()
        }
    }

    /// Anything you control: `if you controlled target 1, ...`.
    pub fn controlled_by_you() -> Filter {
        Filter {
            who: Some(Who::You),
            ..Filter::default()
        }
    }

    /// A permanent cast for its warp cost (CR 702.185): `if target 1
    /// warped, ...`.
    pub fn warped() -> Filter {
        Filter {
            status: mtg_core::card::StatusQuals {
                warped: true,
                ..Default::default()
            },
            ..Filter::default()
        }
    }

    /// A battle its protector is `who` of (CR 310.8): `if an opponent
    /// protects target 1, ...`, `if you protect target 1, ...`.
    pub fn protected_by(who: Who) -> Filter {
        Filter {
            status: mtg_core::card::StatusQuals {
                protector: Some(who),
                ..Default::default()
            },
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

/// The rarer parts of a search (`Effect::Search::more`).
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct SearchMore {
    /// `search black card and green card and blue card ...`, `card named
    /// "X" and card named "Y"`: one card more for each, by its own filter
    /// and name ("a black card, a green card, and a blue card"; named
    /// cards join with "and/or").
    pub slots: Vec<(Filter, Option<String>)>,
    /// Which zones are searched beyond the library.
    pub zones: SearchZones,
    /// `with different names` (CR 201.2).
    pub different_names: bool,
    /// `to battlefield if land else hand`: "Put it onto the battlefield
    /// tapped if it's a land card. Otherwise, put it into your hand."
    pub split: Option<(Filter, mtg_core::card::SearchTo)>,
    pub wording: SearchWording,
    /// How `different_names` prints: `with different names`, `that have
    /// different names`, `that each have different names`.
    pub names: NamesWording,
    /// `exactly 2 card`: "for exactly two cards" (the count, worded so).
    pub exactly: bool,
    /// `with different names that each have mana value X`: the filter's
    /// mana value printed after the names (Uncage the Menagerie).
    pub mana_value_last: bool,
    /// `to hand except 2 chosen by target opponent to graveyard`:
    /// "Target opponent chooses two of those cards. Put the chosen cards
    /// into your graveyard and the rest into your hand." (`chosen by
    /// opponent`: "An opponent chooses").
    pub pick: Option<(u32, Option<Recipient>, mtg_core::card::SearchTo)>,
    /// `to battlefield tapped with 1 stun counter`: "puts it onto the
    /// battlefield tapped with a stun counter on it"; `with X additional
    /// +1/+1 counters` prints "additional" (the bool).
    pub counters: Option<(mtg_core::card::CounterKind, Num, bool)>,
}

/// How a search's "different names" prints.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum NamesWording {
    /// "with different names".
    #[default]
    With,
    /// "that have different names".
    ThatHave,
    /// "that each have different names".
    ThatEachHave,
}

/// The zones a search names beyond the library.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum SearchZones {
    /// The library (or `library or graveyard`, `Effect::Search::graveyard`).
    #[default]
    Library,
    /// `graveyard, hand, or library`: "your graveyard, hand, and/or
    /// library" (`graveyard, hand or library`: without the serial comma);
    /// the library is shuffled if it was searched.
    HandOr { comma: bool },
    /// `graveyard, hand, and library`: "your graveyard, hand, and library
    /// ..., then shuffle".
    HandAnd,
}

/// How a search's destination prints (wording only).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum SearchWording {
    #[default]
    Plain,
    /// `short`: only "Search your library for a basic land card." The
    /// `instead` search after it prints the destination for both (Analyze
    /// the Pollen).
    Short,
    /// `sentence`: the destination as a sentence of its own, "Reveal that
    /// card, put it into your hand, then shuffle." (after an `instead`
    /// search; a search for several differently-filtered cards always
    /// prints so).
    Sentence,
    /// `of them`: "... and reveal them. Put one of them onto the
    /// battlefield tapped and the other into your hand, then shuffle."
    OfThem,
    /// `rest`: "..., reveal those cards, and put one onto the battlefield
    /// tapped and the rest into your hand. Then shuffle."
    Rest,
    /// `instead of 2`: in a trailing `instead` sentence, "search your
    /// library for up to three basic Forest cards instead of two" (the
    /// rest as the search before).
    InsteadOf(u32),
    /// `unshuffled`: no shuffle printed, "Search your library for a card
    /// and put that card into your hand." (an `instead` sentence after it
    /// ends "Then shuffle."; Increasing Ambition, From Father to Son).
    Unshuffled,
    /// `put instead`: in a trailing `instead` sentence, only the new
    /// destination, "put that card onto the battlefield instead"
    /// (Embermouth Sentinel); `put instead then` adds "Then shuffle.";
    /// `put instead of hand`: "put that card onto the battlefield instead
    /// of putting it into your hand" (before `instead if`, Caravan
    /// Vigil).
    PutInstead {
        then: bool,
        of: Option<mtg_core::card::SearchTo>,
    },
    /// `repeat`: slots that differ from the first card only by mana value,
    /// "Search your library for an instant card with mana value 3, reveal
    /// it, and put it into your hand. Then repeat this process for instant
    /// cards with mana values 2 and 1. Then shuffle." (Firemind's
    /// Foresight).
    Repeat,
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
    /// `crewed it`: "that crewed it this turn" (CR 702.122).
    CrewedIt,
    /// `saddled it`: "that saddled it this turn" (CR 702.171).
    SaddledIt,
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
    /// `that share card type`, in the spec of a spell's first targets:
    /// "two target nonland permanents that share a card type" (each
    /// shares one with the first).
    ShareCardType,
    /// `that share creature type`: "that share a creature type" (with the
    /// first target).
    ShareCreatureType,
    /// `controlled by different players`: "two target creatures
    /// controlled by different players" (each controlled by a player
    /// other than the other's controller).
    DifferentControllers,
    /// `controlled by the same player`: "two target creatures controlled
    /// by the same player"; `with the same controller`: "another target
    /// creature with the same controller" (Bioshift).
    SameController,
    /// `controlled by the same opponent`: "two target creatures
    /// controlled by the same opponent" (the filter's `who` is opponent;
    /// not printed apart).
    SameOpponent,
    /// `with same controller`: "with the same controller" (as
    /// `SameController`, worded after a single target).
    WithSameController,
    /// `with lesser power`: "with lesser power" (than the source's power,
    /// as it is or last existed; mentor, CR 702.134a).
    LesserPower,
    /// `with mana value at most this power`: "with mana value less than
    /// or equal to this creature's power" (the source's, as it is or last
    /// existed).
    ManaValueAtMostPower,
    /// `with lesser mana value`: "with lesser mana value" (than the object
    /// that triggered the ability, as it is or last existed).
    LesserManaValue,
    /// `other than that creature`: "other than that creature" (the object
    /// that triggered the ability).
    NotTriggering,
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
    /// `of chosen quality`: "with mana value of the chosen quality" (odd
    /// or even, chosen earlier).
    ChosenQuality,
    /// `with that name`: "with that name" (after "Choose a card name.").
    ThatName,
    /// `with chosen name`: "with the chosen name".
    ChosenName,
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
    /// `target spell or creature`, `target spell or permanent`: one target
    /// that is a spell or a permanent matching the filter (CR 115.1).
    SpellOr(Filter),
    /// `target card from a graveyard`, `target instant card from your
    /// graveyard`: a card in a graveyard (whose: `Any`, `You`, `Opponents`).
    /// `graveyards`: `from graveyards` ("up to two target cards from
    /// graveyards"), which need not share a graveyard.
    GraveyardCard {
        filter: Filter,
        who: Who,
        graveyards: bool,
    },
    /// `target creature card exiled with this` (`linked`, CR 607.2a),
    /// `target exiled card with warp`, `target card you own in exile that
    /// has adventure`: a face-up card in exile (whose owner: `Any`, `You`,
    /// `Opponents`).
    ExileCard {
        filter: Filter,
        who: Who,
        linked: bool,
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
        /// `... with total mana value 3 or less`: "up to three target
        /// creature cards with total mana value 3 or less" (CR 601.2c).
        total: Option<Num>,
        /// `... with different names`: "up to six target creature cards
        /// with different names" (CR 201.2).
        different: bool,
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

/// What the searched player does for each card exiled from their hand
/// (`Effect::ExileChosenName`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum PerHand {
    Draw,
    /// The token's description, as `create` names it.
    Create(String),
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
    /// `"Khans" or "Dragons"`: one of these named options (the Sieges);
    /// abilities labeled with an option's name are the permanent's only
    /// while it was chosen.
    Named(Vec<String>),
    /// `card name`, `nonland card name`: "a card name", "a nonland card
    /// name" (CR 201.3).
    /// `look at opponent's hand then choose card name`: "look at an
    /// opponent's hand, then choose any card name" (`look`).
    CardName {
        nonland: bool,
        look: bool,
    },
    /// `creature card name`, `card name except basic land`: "a creature
    /// card name", "a card name other than a basic land card name".
    CardNameOf(mtg_core::card::NameKind),
    /// `odd or even`: "odd or even" (Zero is even.), for `of chosen
    /// quality`.
    Parity,
    /// `creature type among Elemental, Elf, Faerie`: "Elemental, Elf, or
    /// Faerie" (at most eight).
    CreatureTypeIn(Vec<String>),
    /// `number between 0 and 10`: "a number between 0 and 10", for the
    /// count `chosen number`.
    Number {
        min: u8,
        max: u8,
    },
}

/// What `Ability::CardGrant` gives its cards (`GrantedWay`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Granted {
    /// `flashback` ("has flashback. The flashback cost is equal to that
    /// card's mana cost."), `flashback {1}`.
    Flashback(Option<ManaCost>),
    /// `mayhem` ("has mayhem. The mayhem cost is equal to its mana
    /// cost."), `mayhem {1}{R}`.
    Mayhem(Option<ManaCost>),
    /// `warp {2}{R}`.
    Warp(ManaCost),
    /// `retrace` ("have retrace", in a graveyard).
    Retrace,
    /// `jump start` ("has jump-start", in a graveyard).
    JumpStart,
    /// `foretell`, in a hand: "Each nonland card in your hand without
    /// foretell has foretell. Its foretell cost is equal to its mana cost
    /// reduced by {2}."
    Foretell,
    /// `sneak {3}{B}`, in a graveyard: "have sneak {3}{B}. You may cast
    /// creature spells from your graveyard using their sneak abilities."
    Sneak(ManaCost),
    /// `miracle {2}`.
    Miracle(ManaCost),
    /// An activated keyword ability, activated from the cards' zone:
    /// `slivercycling {3}`, `cycling {2}`, `unearth {2}`.
    Ability(Box<Ability>),
    /// `cast`, `cast by pay 3 life finality`, `cast by pay 1 life brief`,
    /// `play`, in a graveyard: "You may cast artifact spells from your
    /// graveyard by paying 3 life in addition to paying their other
    /// costs. If you cast a spell this way, that artifact enters with a
    /// finality counter on it.", "You may play cards from your graveyard."
    Cast(mtg_core::card::GraveRider),
}

/// Whose activated abilities an `Ability::ActivationCost` changes
/// (`mtg_core::card::AbilitySources`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ActivationSources {
    /// `of <filter>`.
    Filter(Box<Filter>),
    /// Nothing written: every activated ability.
    Any,
    /// `of sources with chosen name`.
    ChosenName,
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
    /// `choose one or more { ... }`, `choose up to 2 { ... }` (CR 700.2):
    /// a modal spell with another head.
    ModalOf(ModeHead, Vec<Mode>),
    /// `choose one or both if teamwork { ... }`, `choose one or both if
    /// may descended this turn { ... }`, `choose one or more if 4 card
    /// types in your graveyard { ... }` (CR 700.2): "Choose one. If this
    /// spell was cast using teamwork, choose both instead." (`may`: "you
    /// may choose both instead"), "... choose one or more instead.": a
    /// modal spell whose larger choice needs the condition as it is cast.
    /// `choose one or any number if kicked` is `more` printed "choose any
    /// number instead".
    ModalIf {
        more: bool,
        any_number: bool,
        may: bool,
        cond: StaticCond,
        modes: Vec<Mode>,
    },
    /// `spree { mode {2}: ... mode {1}: ... }` (CR 702.172): one or more
    /// modes, each with an additional cost (at most three).
    Spree(Vec<(ManaCost, Mode)>),
    /// `tiered { mode "Thunder" {0}: ... mode "Thundara" {3}: ... }` (CR
    /// 702.183): choose exactly one mode and pay its additional cost; each
    /// mode is named.
    Tiered(Vec<(ManaCost, Mode)>),
    /// `equip abilities targeting this cost {2} less`: "Equip abilities
    /// you activate that target this creature cost {2} less to activate."
    /// With `equipment`, `equipment abilities targeting this cost {1}
    /// less`: "Activated abilities of Equipment you control that target
    /// this creature cost {1} less to activate." (Bladegraft Aspirant).
    EquipDiscount {
        n: u8,
        equipment: bool,
    },
    /// `pawprint 5 { mode 1: ... mode 2: ... mode 3: ... }` (CR 700.2i):
    /// "Choose up to five {P} worth of modes. You may choose the same mode
    /// more than once." with each mode's number of pawprints.
    Pawprint {
        limit: u8,
        modes: Vec<(u8, Mode)>,
    },
    /// `equip {3} or discard 1 card`: "Equip—Pay {3} or discard a card."
    /// (CR 702.6a): an equip ability for each cost, the player choosing
    /// which to activate.
    EquipOr {
        mana: ManaCost,
        other: Box<Cost>,
        targets: Vec<Target>,
    },
    /// `activated {3}, {T}, sacrifice artifact or discard nonland card:
    /// <effects>`: "{3}, {T}, Sacrifice an artifact or discard a nonland
    /// card: ..." (CR 602.1): an activated ability for each alternative of
    /// the cost's last part (`cost`, then `cost` with `other` in place of
    /// that part), the player choosing which to activate. The last part
    /// and `other` are each a sacrifice, a discard, or life.
    ActivatedOr {
        cost: Cost,
        other: Box<Cost>,
        targets: Vec<Target>,
        effects: Vec<Sentence>,
    },
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

        /// `triggered once each turn ...` ("This ability triggers only
        /// once each turn."), `twice each turn`, `do once each turn` ("Do
        /// this only once each turn.").
        limit: mtg_core::card::TurnLimit,
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
        /// `... as long as <cond> otherwise -2/-2`: "Otherwise, it gets
        /// -2/-2." (CR 611.3a), what applies to the same objects while the
        /// condition doesn't hold. Empty without a condition.
        otherwise: Vec<Mod>,
    },
    /// `enters as copy of creature`: "You may have ~ enter as a copy of
    /// any creature on the battlefield"; `enters tapped as copy of land`
    /// "enter tapped as a copy of any land on the battlefield" (Vesuva);
    /// `... of land card in graveyard` "any land card in a graveyard"
    /// (Echoing Deeps).
    EntersAsCopy(Filter, mtg_core::card::EntersCopy),
    /// `as this enters choose color`: "As this land enters, choose a
    /// color." (CR 614.12). `as it enters choose color except blue` is
    /// printed on the line before: "This land enters tapped. As it
    /// enters, choose a color other than blue."
    AsEntersChoose {
        kind: ChoiceKind,
        joined: bool,
    },
    /// `as this enters becomes choice 3/3 creature or 2/2 creature with
    /// flying or 1/6 creature with defender` ("As this creature enters,
    /// it becomes your choice of a 3/3 creature, a 2/2 creature with
    /// flying, or a 1/6 creature with defender.", CR 614.12): the
    /// controller chooses one as it enters, and the permanent has that
    /// base power and toughness, those types, and those keywords for as
    /// long as it stays on the battlefield. `in addition` prints "in
    /// addition to its other types".
    BecomesChoice {
        forms: Vec<ChoiceForm>,
        in_addition: bool,
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
        /// `... cost {1} less count each creature you control with power 4
        /// or greater`: "... for each ...".
        per: Option<Num>,
        /// `spells first creature you cast cost {2} less`: "The first
        /// creature spell you cast each turn costs {2} less to cast."
        first: bool,
        /// `... cost {2} less and flash`: "... and can be cast as though it
        /// had flash" (with `first`).
        flash: bool,
        /// `spells you cast from graveyard cost {2} less`: "Spells you cast
        /// from your graveyard cost {2} less to cast." (`from hand`, `from
        /// exile`, `from other than hand`: "from anywhere other than your
        /// hand").
        from: mtg_core::card::SpellFrom,
    },
    /// `spells instant or sorcery you cast have storm`, `spells you cast
    /// have delve`, `spells instant or sorcery you cast from hand have
    /// cascade`, `spells you cast have affinity for artifact`: what the
    /// controller's matching spells have (convoke, improvise, delve,
    /// cascade, storm, affinity).
    SpellsHave {
        filter: Filter,
        /// `from hand`, `from exile`, ... (`SpellFrom`): "... you cast from
        /// exile have convoke."
        from: mtg_core::card::SpellFrom,
        /// `spells can't be countered`: everyone's spells ("Spells can't be
        /// countered."), with `uncounterable`; otherwise `You`.
        whose: Who,
        keywords: Vec<Keyword>,
        storm: bool,
        affinity: Option<Filter>,
        /// `spells you control can't be countered`: "Spells you control
        /// can't be countered." (no keywords then).
        uncounterable: bool,
        /// `spells planeswalker you cast have spend any color`: "You may
        /// spend mana as though it were mana of any color to cast
        /// planeswalker spells." (CR 609.4b; no keywords then); `spend any
        /// type can`: "You can spend mana of any type to cast creature
        /// spells." (`true`: that wording). `spells you cast have spend
        /// any color and abilities`: "You may spend mana as though it were
        /// mana of any color." (spells and activated abilities alike; the
        /// third field).
        spend: Option<(mtg_core::card::SpendAs, bool, bool)>,
        /// `spells instant or sorcery you control have lifelink`: written
        /// and printed "you control" rather than "you cast" (the spells on
        /// the stack: lifelink, deathtouch, wither, split second).
        control: bool,
        /// `... have conspire` (CR 702.78).
        conspire: bool,
        /// `... have casualty 1` (CR 702.153); 0 for none.
        casualty: u8,
        /// `spells Lesson you control have lifelink joined` after `static
        /// Noble creature you control: has lifelink`: one sentence, "Noble
        /// creatures you control and Lesson spells you control have
        /// lifelink." (Lo and Li, Twin Tutors); wording only.
        joined: bool,
    },
    /// `spells you cast have alternative cost {W}{U}{B}{R}{G}`: "You may
    /// pay {W}{U}{B}{R}{G} rather than pay the mana cost for spells you
    /// cast." (CR 118.9; `collect evidence 10`: "You may collect evidence
    /// 10 ..."); `spells you cast from exile have once alternative cost
    /// {0}`: "Once each turn, you may pay {0} rather than pay the mana cost
    /// for a spell you cast from exile." Mana and collect evidence only.
    SpellsAlt {
        filter: Filter,
        from: mtg_core::card::SpellFrom,
        cost: Box<Cost>,
        once: bool,
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
        /// `this spell costs {1}{U} less if it targets ...`: the colored
        /// symbols of the reduction (one mana of each color).
        colors: Vec<mtg_core::mana::Color>,
        filter: Filter,
        /// `if it targets spell creature`: "a creature spell"; `if it
        /// targets card creature with mana value 3 or less`: "a creature
        /// card with mana value 3 or less".
        noun: TargetNoun,
    },
    /// `this spell costs {3} less if creature died this turn`; `... as
    /// long as 4 card types in your graveyard` prints "as long as" (Drag
    /// to the Roots; wording only).
    CondCost {
        delta: i32,
        cond: StaticCond,
        as_long_as: bool,
    },
    /// `affinity for artifact`: "Affinity for artifacts" (CR 702.41), this
    /// spell costs {1} less for each such permanent you control.
    Affinity(Filter),
    /// `protection from artifact`, `protection from everything`:
    /// "Protection from artifacts" (CR 702.16), a quality other than a
    /// color (those are keywords).
    Protection(Filter),
    /// `landwalk snow Forest`, `landwalk snow land`, `landwalk nonbasic
    /// land`: "Snow forestwalk", "Snow landwalk", "Nonbasic landwalk"
    /// (CR 702.14a): can't be blocked as long as the defending player
    /// controls a land the filter matches.
    Landwalk(Filter),
    /// `hexproof from black`, `hexproof from multicolored`, `hexproof from
    /// instant`: "Hexproof from black" (CR 702.11d).
    HexproofFrom(Filter),
    /// `you have hexproof`, `you have shroud`, `opponents can't cast spells
    /// during your turn`, `additional land`, `additional land 2`,
    /// `additional land each player`: a static ability of the controller
    /// as a player; `as long as <condition>` after it is its condition
    /// ("As long as you control another Elf, you may play ...").
    PlayerStatic {
        kind: mtg_core::card::PlayerStaticKind,
        cond: Option<StaticCond>,
    },
    /// `ban attacking you creature with flying`, `ban attacking you or
    /// planeswalkers creature with flying`: "Creatures with flying can't
    /// attack you [or planeswalkers you control]." (CR 508.1c;
    /// `PlayerStaticKind::CantAttackYou`).
    CantAttackYou {
        filter: Filter,
        planeswalkers: bool,
    },
    /// `activated abilities of artifact you control cost {1} less min
    /// one`, `power-up abilities of other creature you control cost {3}
    /// less`: "Activated abilities of artifacts you control cost {1} less
    /// to activate. This effect can't reduce the mana in that cost to less
    /// than one mana." (`PlayerStaticKind::ActivationCost`);
    /// `activated abilities cost {2} more unless mana`, `abilities you
    /// activate cost {2} less unless mana min one`, `equip abilities you
    /// activate of other Equipment cost {1} less`, `activated abilities of
    /// sources with chosen name cost {2} more`, `loyalty abilities of
    /// planeswalker your opponents control cost {1} more`.
    ActivationCost {
        sources: ActivationSources,
        by: u8,
        more: bool,
        min_one: bool,
        class: mtg_core::card::AbilityClass,
        activator: Option<mtg_core::card::RulePlayers>,
    },
    /// `ban casting noncreature spell with mana value equal chosen
    /// number`: "Noncreature spells with mana value equal to the chosen
    /// number can't be cast." (`PlayerStaticKind::CantCast`).
    CantBeCast(Filter),
    /// `opening hand`: "If this card is in your opening hand, you may begin
    /// the game with it on the battlefield." (CR 103.6a).
    OpeningHand,
    /// `plotting costs {2} less`: "Plotting cards from your hand costs {2}
    /// less." (CR 702.170a).
    PlotLess(u32),
    /// `spells your opponents cast that target this cost 3 life more`:
    /// "Spells your opponents cast that target this creature cost an
    /// additional 3 life to cast." (CR 601.2f, 119.4).
    TargetedLife(u32),
    /// `opening hand reveal first upkeep: ...`: "You may reveal this card
    /// from your opening hand. If you do, at the beginning of the first
    /// upkeep, ..." (the Chancellors); `your first upkeep` ("at the
    /// beginning of your first upkeep"), `your first main` ("at the
    /// beginning of your first main phase of the game"); `after` puts the
    /// time last ("If you do, scry 3 at the beginning of your first
    /// upkeep.").
    OpeningHandReveal {
        step: Step,
        whose: Who,
        after: bool,
        effects: Vec<Sentence>,
    },
    /// `trigger again another Elemental you control`: "If a triggered
    /// ability of another Elemental you control triggers, it triggers an
    /// additional time." (CR 603.2d); `trigger again permanent you control
    /// when artifact or creature enters`: "If an artifact or creature
    /// entering causes a triggered ability of a permanent you control to
    /// trigger, that ability triggers an additional time." (`enters the
    /// battlefield`: worded "entering the battlefield").
    TriggerAgain {
        filter: Filter,
        entering: Option<Filter>,
        battlefield: bool,
        /// `when creature dies`, `when creature you control attacks`: "If
        /// a creature dying causes ...", "If a creature you control
        /// attacking causes ...".
        cause: mtg_core::card::TriggerCause,
        /// `trigger never permanent when creature enters`: "Creatures
        /// entering don't cause abilities to trigger." (Torpor Orb).
        suppress: bool,
    },
    /// A keyword ability with a number or cost, printed as the keyword
    /// ("Bushido 2", "Echo {2}{R}") and compiled to what it stands for.
    Mechanic(Mechanic),
    /// `exert: effects`: "You may exert this creature as it attacks. When
    /// you do, [effects]." (CR 701.43). With targets ("When you do, target
    /// creature can't block this turn"), the effects are one `WhenYouDo`
    /// sentence holding them (see `exert_parts`): a reflexive trigger, its
    /// targets chosen once the creature is exerted (CR 603.12).
    Exert(Vec<Sentence>),
    /// `enters tapped unless you control planeswalker`: "This land enters
    /// tapped unless you control a planeswalker." (CR 614.1d)
    EntersTappedUnless(StaticCond),
    /// `enters tapped unless reveal Mountain or Forest card`: "As this
    /// land enters, you may reveal a Mountain or Forest card from your
    /// hand. If you don't, this land enters tapped." (CR 614.12); `...
    /// card or you control Dragon`: "This land enters tapped unless you
    /// revealed a Dragon card this way or you control a Dragon."
    EntersTappedUnlessReveal {
        filter: Filter,
        or: Option<StaticCond>,
        /// `enters tapped unless behold Jace`: "As this land enters, you
        /// may behold a Jace. If you don't, this land enters tapped."
        behold: bool,
    },
    /// `enters tapped if not your turn`: "This creature enters tapped if
    /// it's not your turn." (the same replacement as `enters tapped unless
    /// your turn`, worded with "if").
    EntersTappedIfNotYourTurn,
    /// `enters tapped if x 2 or less`: "If X is 2 or less, this creature
    /// enters tapped." (the replacement "unless" the condition fails);
    /// after an `enters with` line, in its paragraph: "..., it enters
    /// tapped."
    EntersTappedIf(StaticCond),
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
        /// `and 1 lifelink counter`: counters of a second kind ("two +1/+1
        /// counters and a lifelink counter", Dust Animus).
        more: Option<(u32, CounterKind)>,
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
    /// `tax attacking you {2}`, `tax attacking you or planeswalkers {1}`,
    /// `tax enchanted attack or block {3}`, `tax enchanted blocked {3}`:
    /// an attack or block tax (CR 508.1d, 509.1d). `generic` mana for
    /// each creature, times `per` (`{X} count each hand`: "{1} for each
    /// card in your hand"), while `cond` holds (`... as long as <cond>`).
    CombatTax {
        scope: mtg_core::card::TaxScope,
        generic: u32,
        per: Option<Num>,
        cond: Option<StaticCond>,
    },
    /// `power-up again`: "Each power-up ability of permanents you control
    /// can be activated an additional time." (Wonder Man, Hollywood Hero).
    PowerUpAgain,
    /// `hand size none`, `hand size +1`, `hand size opponents -1`: "You
    /// have no maximum hand size.", "Your maximum hand size is increased
    /// by one.", "Each opponent's maximum hand size is reduced by one."
    HandSize {
        whose: Who,
        delta: Option<i32>,
        /// `hand size 8`: "Your maximum hand size is eight."
        set: bool,
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
    /// `play from top lands`: "You may play lands from the top of your
    /// library."; `play from top spells creature`: "You may cast creature
    /// spells from the top of your library."; `play from top lands and
    /// spells`: "You may play lands and cast spells ..."; `play from top
    /// once spells instant or sorcery`: "Once each turn, you may cast an
    /// instant or sorcery spell from the top of your library." (CR 601.3).
    PlayFromTop {
        lands: bool,
        spells: Option<Filter>,
        once: bool,
        /// The rest: `may look top and play from top ...`, `as long as
        /// <condition>`, `flash`, `pay life`, and the wording.
        more: TopMore,
    },
    /// `cast free from hand spells`: "You may cast spells from your hand
    /// without paying their mana costs." (Omniscience); `cast free spells
    /// Dragon`: "You may cast Dragon spells without paying their mana
    /// costs."; `... max count creature you control`: "spells with mana
    /// value less than or equal to the number of creatures you control";
    /// `cast free once your turns from hand spells instant or sorcery`:
    /// "Once during each of your turns, you may cast an instant or sorcery
    /// spell from your hand without paying its mana cost."; `cast free
    /// first each player spells`: "The first spell each player casts
    /// during each of their turns may be cast without paying its mana
    /// cost." (CR 118.9).
    CastFree {
        spells: Filter,
        from_hand: bool,
        max: Option<CountExpr>,
        limit: mtg_core::card::FreeCastLimit,
    },
    /// `static each instant or sorcery card in your graveyard: has
    /// flashback`, `static artifact card or red creature card in your hand:
    /// has warp {2}{R}`, `static each Sliver card in each player's hand:
    /// has slivercycling {3}`, `static during your turn, each Lesson card
    /// in your graveyard: has flashback {1}`: cards in a hand or graveyard
    /// have a way to be cast or an ability (`CardGrant`, CR 113.6b).
    CardGrant {
        filter: Filter,
        zone: mtg_core::state::Zone,
        players: bool,
        each: bool,
        grant: Granted,
        cond: Option<StaticCond>,
    },
    /// `play exiled spells`: "You may cast spells from among cards exiled
    /// with this creature."; `play exiled lands and spells` "You may play
    /// lands and cast spells from among ..."; `play exiled your turn lands
    /// and spells`: "During your turn, you may play cards exiled with
    /// this artifact."; `owned`: "cards you own exiled with ..." (CR
    /// 607.2a, 601.3).
    PlayExiled {
        lands: bool,
        spells: Option<Filter>,
        your_turn: bool,
        owned: bool,
        /// `may look exiled`: "You may look at cards exiled with this
        /// creature." (alone); `play exiled look lands and spells`: "You
        /// may look at the cards exiled with this enchantment, and you may
        /// play lands and cast spells from among those cards." (CR 406.3).
        look: bool,
        /// Riders, a condition, and wordings.
        more: PlayExiledMore,
    },
    /// `top revealed`: "Play with the top card of your library
    /// revealed."; `top revealed players`: "Players play with the top card
    /// of their libraries revealed."
    TopRevealed {
        players: bool,
        /// `hands revealed players`: "Players play with their hands
        /// revealed."
        hands: bool,
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
    /// `level 2 {R}{G}`: a Class's level ability, "{R}{G}: Level 2" (CR
    /// 716.2a: activate only as a sorcery while it is level 1). The
    /// abilities after it, up to the next, are the ones it gains at that
    /// level.
    ClassLevel {
        level: u8,
        cost: ManaCost,
    },
    /// `levels 1 to 4 pt 2/2`, `levels 5 or more pt 5/5` (`max` `None`):
    /// a leveler card's level symbol and P/T box, "LEVEL 1-4" over "2/2"
    /// (CR 711.2): as long as it has that many level counters, it has that
    /// base power and toughness and the abilities after this line, up to
    /// the next.
    LevelBand {
        min: u32,
        max: Option<u32>,
        pt: (i32, i32),
    },
    /// `to solve you control 3 artifact`: a Case's "To solve — You
    /// control three or more artifacts." (CR 719.2a: at the beginning of
    /// its controller's end step, if the condition holds and it isn't
    /// solved, it becomes solved).
    ToSolve(StaticCond),
    /// `solved <ability>`: "Solved — <ability>" (CR 719.2b), an ability
    /// the Case has only once solved.
    Solved(Box<Ability>),
    /// `station 2: static other creature you control: +1/+1` (CR 721.2a):
    /// "2+ | Other creatures you control get +1/+1.": the ability works
    /// only while the permanent has that many charge counters (a static
    /// ability's condition, an activated ability's activation condition;
    /// a triggered ability triggers only then). It never makes the
    /// permanent a creature.
    StationAbility {
        n: u32,
        ability: Box<Ability>,
    },
    /// `protection doesn't remove this`: "This effect doesn't remove this
    /// Aura.", printed after the static ability before it.
    AuraStays,
    /// `damage isn't removed during cleanup`: "Damage isn't removed from
    /// this creature during cleanup steps." (CR 514.2).
    KeepsDamage,
    /// `deck any number`: "A deck can have any number of cards named ~."
    /// (CR 100.2a). A deck-building rule with no meaning in a game.
    DeckAnyNumber,
    /// `companion "Each nonland card in your starting deck has a different
    /// name."`: "Companion — ..." (CR 702.139a). A deck-building condition
    /// for a card kept outside the game; here the card is simply a card
    /// in the deck, so the line has no meaning in a game.
    Companion(String),
    /// `this spell can't be countered`: "This spell can't be countered."
    Uncounterable,
    /// `this spell can't be copied`: "This spell can't be copied."
    Uncopyable,
    /// `if x 5 or more, this spell can't be countered and the damage can't
    /// be prevented`: "If X is 5 or more, this spell can't be countered and
    /// the damage can't be prevented." (Banefire); without the `and`
    /// clause, "this spell can't be countered" only (Exquisite Firecraft).
    UncounterableIf {
        cond: StaticCond,
        unpreventable: bool,
    },
    /// An ability word (CR 207.2c) printed before the ability: "Landfall —
    /// Whenever a land you control enters, ...". No rules meaning.
    Labeled {
        label: String,
        ability: Box<Ability>,
    },
    /// `during your turn replace damage to this: prevent`: "During your
    /// turn, prevent all damage that would be dealt to this creature." The
    /// ability applies only during its controller's turn.
    DuringYourTurn(Box<Ability>),
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
    /// `while in graveyard` after a `triggered from graveyard` ability's
    /// trigger: "while this card is in your graveyard" (Furious
    /// Forebear). Wording only: such an ability works only from there.
    InGraveyard,
    /// `if it had counter`, `if it had one or more counters`: the core
    /// `TriggerHadCounters` ("if it had counters on it") printed "if it had
    /// a counter on it" (Leader's Talent) or "if it had one or more
    /// counters on it" (Ambitious Augmenter).
    HadCounter {
        one_or_more: bool,
    },
    /// `if its power or toughness greater than this`: the core `Evolves`
    /// printed "if its power is greater than this creature's power or its
    /// toughness is greater than this creature's toughness" (Sharp-Eyed
    /// Rookie).
    Outstats,
    /// `if its toughness was less than 1` (also `power`, `mana value`; `N
    /// or less`, `N or greater`): the triggering object as it last existed
    /// (`StaticCondition::TriggerIs`, Massacre Girl, Known Killer).
    ItsStat {
        stat: ItStat,
        cmp: mtg_core::card::Cmp,
        /// `less than N`: printed so, the check "N-1 or less".
        less_than: bool,
        n: i16,
        /// `if its power is 2 or less` ("is"): the triggering object as it
        /// is, an attacker (Writ of Passage); wording only.
        present: bool,
    },
}

/// What `TrigCond::ItsStat` compares.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ItStat {
    Power,
    Toughness,
    ManaValue,
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
    /// CR 702.189: `firebending 2`, `firebending count where power`:
    /// whenever it attacks, add N {R} that lasts until end of combat.
    Firebending(Num),
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
    /// CR 702.119: `emerge {6}{B}`: an alternative cost, sacrificing a
    /// creature, reduced by its mana value.
    Emerge(ManaCost),
    /// CR 702.94: `miracle {R}`: as it's drawn, the first card drawn this
    /// turn, it may be cast for this cost.
    Miracle(ManaCost),
    /// CR 702.76: `prowl {2}{U}`: an alternative cost, offered if you dealt
    /// combat damage to a player this turn with a creature of one of the
    /// card's creature types.
    Prowl(ManaCost),
    /// CR 702.120: `escalate {1}{W}`: "Pay this cost for each mode chosen
    /// beyond the first." On a `choose one or both` spell, the cost of
    /// choosing both.
    Escalate(ManaCost),
    /// CR 702.59: `recover {2}{B}`: "When a creature is put into your
    /// graveyard from the battlefield, you may pay [cost]. If you do,
    /// return this card from your graveyard to your hand. Otherwise, exile
    /// this card." (from the graveyard).
    Recover(ManaCost),
    /// CR 702.152: `blitz {3}{G}`, `blitz {2}{B}{B}, pay 2 life`: the
    /// card's alternative cost; the permanent has haste and "When this
    /// creature dies, draw a card" and is sacrificed at the beginning of
    /// the next end step.
    Blitz(Cost),
    /// CR 702.160: `prototype {2}{R} 3/2`: "Prototype {2}{R} — 3/2", cast
    /// with that mana cost, power, and toughness (and the cost's colors).
    Prototype(ManaCost, i32, i32),
    /// CR 702.87: `level up {2}`: "{2}: Put a level counter on this.
    /// Activate only as a sorcery."
    LevelUp(ManaCost),
    /// CR 702.32: `fading 3`: enters with N fade counters; "At the
    /// beginning of your upkeep, remove a fade counter from this
    /// permanent. If you can't, sacrifice the permanent."
    Fading(u32),
    /// CR 702.86: `annihilator 2`: "Whenever this creature attacks,
    /// defending player sacrifices N permanents."
    Annihilator(u32),
    /// CR 702.91: `battle cry`: "Whenever this creature attacks, each
    /// other attacking creature gets +1/+0 until end of turn."
    BattleCry,
    /// CR 702.23: `rampage 2`: "Whenever this creature becomes blocked,
    /// it gets +N/+N until end of turn for each creature blocking it
    /// beyond the first."
    Rampage(u32),
    /// CR 702.149: `training`: "Whenever this creature and at least one
    /// other creature with power greater than this creature's power
    /// attack, put a +1/+1 counter on this creature."
    Training,
    /// CR 702.163: `for mirrodin`: "When this Equipment enters, create a
    /// 2/2 red Rebel creature token, then attach this Equipment to it."
    ForMirrodin,
    /// CR 702.121: `melee`: "Whenever this creature attacks, it gets
    /// +1/+1 until end of turn for each opponent you attacked with a
    /// creature this combat."
    Melee,
    /// CR 702.105: `dethrone`: "Whenever this creature attacks the player
    /// with the most life or tied for most life, put a +1/+1 counter on
    /// this creature."
    Dethrone,
    /// CR 702.116: `myriad`: token copies attacking each opponent other
    /// than the defending player. The game's only opponent is the
    /// defending player, so it does nothing.
    Myriad,
    /// CR 702.164: combat damage to a player also gives N poison counters.
    Toxic(u32),
    /// CR 702.21: becomes the target of an opponent's spell or ability,
    /// counter it unless its controller pays.
    Ward(ManaCost),
    /// `ward pay 3 life`: "Ward—Pay 3 life." (CR 702.21a): counter it
    /// unless that player pays the life.
    WardCost(Cost),
    /// `ward pay count equal power life`, `ward get 5 poison counters`,
    /// `ward you get 2 poison counters`, `ward discard 1 card or {2}`:
    /// "Ward—Pay life equal to this creature's power.", "Ward—Get five
    /// poison counters.", "Ward—Discard a card or pay {2}." (CR 702.21a):
    /// counter it unless that player pays one of the alternatives.
    WardAlts(Vec<WardAlt>),
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
    /// `crew 1, once`: "Crew 1. Activate only once each turn." (Luxurious
    /// Locomotive).
    CrewOnce(u32),
    /// CR 702.74: an alternative cost; sacrificed as it enters if evoked.
    Evoke(ManaCost),
    /// CR 702.35: cast for this cost as it's discarded.
    Madness(ManaCost),
    /// `mayhem {B}`: "Mayhem {B}": cast from the graveyard for this cost
    /// the turn it was discarded.
    Mayhem(ManaCost),
    /// CR 702.143: exiled for {2} on its owner's turn, cast later for this.
    Foretell(ManaCost),
    /// CR 702.190: `sneak {2}{W}`: during your declare blockers step, cast
    /// for this cost and an unblocked attacker returned to hand; the
    /// permanent enters tapped and attacking.
    Sneak(ManaCost),
    /// CR 702.180: `harmonize {4}{R}`: cast from the graveyard for this
    /// cost, tapping up to one untapped creature to reduce it by that
    /// creature's power; exiled as it leaves the stack.
    Harmonize(ManaCost),
    /// CR 702.188: `web slinging {W}`: cast for this cost, returning a
    /// tapped creature you control to its owner's hand.
    WebSlinging(ManaCost),
    /// CR 702.176: `impending 4 {1}{U}{U}`: cast for this cost, it enters
    /// with N time counters and isn't a creature while it has one; at
    /// the beginning of your end step a time counter is removed.
    Impending(u32, ManaCost),
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
    /// `kicked with 2 +1/+1 counters [and 1 trample counter] [and haste]`.
    KickedWith(u32, Option<(u32, CounterKind)>, Vec<Keyword>),
    /// CR 702.43: `modular 2`.
    Modular(u32),
    /// CR 702.58: `graft 2`: enters with N +1/+1 counters; "Whenever
    /// another creature enters, if this permanent has a +1/+1 counter on
    /// it, you may move a +1/+1 counter from this permanent onto that
    /// creature."
    Graft(u32),
    /// CR 702.63: `vanishing 3`: enters with N time counters; "At the
    /// beginning of your upkeep, if this permanent has a time counter on
    /// it, remove a time counter from it", and as the last is removed, it
    /// is sacrificed. `vanishing` (0) has no entering counters (CR
    /// 702.63b).
    Vanishing(u32),
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
    /// CR 702.167: `craft with artifact {2}{R}`: "{2}{R}, Exile this
    /// artifact, Exile another artifact you control or an artifact card
    /// from your graveyard: Return this card transformed under its owner's
    /// control. Craft only as a sorcery." One material matching the filter.
    Craft(Filter, ManaCost),
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
    /// CR 702.150: `compleated`: it enters with two fewer loyalty counters
    /// for each Phyrexian mana symbol paid with life.
    Compleated,
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
    /// `multikicker {1}{G}` (CR 702.33c): a kicker cost paid any number of
    /// times as the spell is cast.
    Multikicker(ManaCost),
    /// `soulbond` (CR 702.95): pairing as this creature or another
    /// creature its controller controls enters.
    Soulbond,
    /// `disturb {1}{U}` (CR 702.146): "You may cast this card transformed
    /// from your graveyard by paying [cost] rather than its mana cost."
    Disturb(ManaCost),
    /// `exiled instead of graveyard`: "If [this] would be put into a
    /// graveyard from anywhere, exile it instead." (a disturb card's back
    /// face).
    GraveyardExile,
    /// `transmute {1}{B}{B}` (CR 702.53): "{1}{B}{B}, Discard this card:
    /// Search your library for a card with the same mana value as the
    /// discarded card, reveal it, put it into your hand, then shuffle.
    /// Activate only as a sorcery."
    Transmute(ManaCost),
    /// `sunburst` (CR 702.44): enters with a +1/+1 counter (a creature
    /// card) or a charge counter (any other) for each color of mana spent
    /// to cast it.
    Sunburst,
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
    /// CR 702.195: `storied`: an enduring story for the rest of the game
    /// once you control three or more artifacts, legendaries, and/or
    /// Sagas.
    Storied,
    /// CR 702.179: `start your engines`: "Start your engines!", a speed of
    /// 1 for a player who has none.
    StartYourEngines,
    /// `becomes day as this enters`: "If it's neither day nor night, it
    /// becomes day as this creature enters." (CR 731.1).
    BecomesDay,
    /// `daybound` (CR 702.145b), on a transforming card's front face: it
    /// becomes day if it's neither, it enters transformed at night, it
    /// transforms as it becomes night, and nothing else transforms it.
    Daybound,
    /// `nightbound` (CR 702.145e), on the back face: it transforms as it
    /// becomes day.
    Nightbound,
    /// `fabricate 1` (CR 702.123): when this enters, N +1/+1 counters on
    /// it or N 1/1 Servos.
    Fabricate(u32),
    /// `hideaway 4` (CR 702.75): when this enters, look at the top N
    /// cards of your library, exile one face down (linked to this), and
    /// put the rest on the bottom in a random order.
    Hideaway(u32),
    /// `evolve` (CR 702.100).
    Evolve,
    /// `increment` (CR 702.191): whenever you cast a spell, if the mana
    /// spent on it is greater than this creature's power or toughness, a
    /// +1/+1 counter on it.
    Increment,
    /// `renown 1` (CR 702.112).
    Renown(u32),
    /// `unleash` (CR 702.98): may enter with a +1/+1 counter; can't block
    /// as long as it has one.
    Unleash,
    /// `afterlife 1` (CR 702.135): when this dies, N 1/1 white and black
    /// Spirit tokens with flying.
    Afterlife(u32),
    /// `mobilize 2`, `mobilize count where graveyard creature` (CR
    /// 702.181): whenever this attacks, N tapped and attacking 1/1 red
    /// Warrior tokens, sacrificed at the beginning of the next end step.
    Mobilize(Num),
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
    /// `cleave {1}{U}: <effects>` (CR 702.148): the spell's alternative
    /// cost, cast for which the spell is these effects: the `spell` text
    /// with the words in square brackets removed. The `spell` line prints
    /// with the words it has beyond these in brackets.
    Cleave(ManaCost, Box<Ability>),
    /// `read ahead` (CR 702.155): a Saga that enters with a chosen number
    /// of lore counters, the chapters below it not triggering.
    ReadAhead,
    /// `champion Goblin` (CR 702.72): "Champion a Goblin": when it enters,
    /// sacrifice it unless you exile another Goblin you control; when it
    /// leaves the battlefield, the exiled card returns.
    Champion(Filter),
    /// `reconfigure {2}` (CR 702.151): "{2}: Attach to target creature you
    /// control; or unattach from a creature. Reconfigure only as a
    /// sorcery. While attached, this isn't a creature."
    Reconfigure(ManaCost),
    /// `haunt` (CR 702.55): "When this creature dies [this spell card is
    /// put into a graveyard after resolving], exile it haunting target
    /// creature."; its `when haunted dies` ability then watches that
    /// creature from exile.
    Haunt,
    /// `devour 2`, `devour artifact 1`, `devour land 3` (CR 702.82): as it
    /// enters, sacrifice any number of creatures (artifacts, lands); it
    /// enters with N +1/+1 counters for each.
    Devour(Option<mtg_core::card::CardType>, u32),
    /// `amplify 1` (CR 702.38): as it enters, reveal any number of cards
    /// from your hand that share a creature type with it; N +1/+1 counters
    /// for each.
    Amplify(u32),
    /// `tribute 2` (CR 702.104): as it enters, an opponent of your choice
    /// may put N +1/+1 counters on it; `if tribute not paid` after.
    Tribute(u32),
    /// `living weapon` (CR 702.92): when this Equipment enters, a 0/0
    /// black Phyrexian Germ token, then attach this to it.
    LivingWeapon,
    /// `job select` (CR 702.182): when this Equipment enters, a 1/1
    /// colorless Hero token, then attach this to it.
    JobSelect,
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
    /// `can't be blocked by creature with greater power`: "This creature
    /// can't be blocked by creatures with greater power."
    NotByGreaterPower,
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
    /// `8 cards are in your graveyard`: "eight or more cards are in your
    /// graveyard" (Sidequest: Card Collection).
    AreIn,
}

/// The rest of `Ability::PlayExiled`, written before `lands` / `spells`
/// (and the condition last).
#[derive(Clone, Eq, PartialEq, Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // one per clause of the text
pub struct PlayExiledMore {
    /// `any type`: "Mana of any type can be spent to cast those spells."
    /// (", and mana of any type can be spent to cast that spell" with
    /// `the card`).
    pub any_type: bool,
    /// `flash`: "During your turn, you may cast cards exiled with Azula and
    /// you may cast them as though they had flash."
    pub flash: bool,
    /// `finality`: "If you cast a spell this way, that creature enters with
    /// a finality counter on it."
    pub finality: bool,
    /// `the card`: "you may cast the exiled card" (spells only).
    pub the_card: bool,
    /// `if <condition>` last: "As long as <condition>, you may ...".
    pub condition: Option<Box<StaticCond>>,
}

/// A turn-log count (`TurnEvent`) the card language words by its own
/// phrase, as a condition (`StaticCond::Tally`: "three or more cards left
/// your graveyard this turn") and as a count (`CountExpr::Tally`: "the
/// number of times you descended this turn"). One row per kind: its words
/// in the card language, its noun phrase, and what it counts.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Tally {
    /// "cards left your graveyard this turn".
    LeftGraveyard,
    /// "times you descended this turn" (CR 700.11).
    Descended,
    /// "lands that entered the battlefield under your control this turn".
    LandsEntered,
    /// "cards were put into exile this turn" (any player's).
    Exiled,
    /// "creature cards were put into graveyards from anywhere this turn".
    CreatureCardsToGraveyards,
    /// "creatures that were exiled under your opponents' control this turn".
    CreaturesExiledOpponents,
    /// "sources you controlled dealt damage this turn".
    DamageSources,
    /// "you've activated a loyalty ability this turn".
    LoyaltyActivated,
    /// "creatures attacked this turn" (any player's).
    CreaturesAttacked,
    /// "an opponent was dealt combat damage by a legendary creature this
    /// turn".
    LegendaryCombatDamaged,
    /// "cards were put into your graveyard from your library this turn".
    LibraryToGraveyard,
}

impl Tally {
    pub const ALL: [Tally; 11] = [
        Tally::LegendaryCombatDamaged,
        Tally::LibraryToGraveyard,
        Tally::LeftGraveyard,
        Tally::Descended,
        Tally::LandsEntered,
        Tally::Exiled,
        Tally::CreatureCardsToGraveyards,
        Tally::CreaturesExiledOpponents,
        Tally::DamageSources,
        Tally::LoyaltyActivated,
        Tally::CreaturesAttacked,
    ];

    /// Its words in the card language, after the number of a condition
    /// (`3 cards left your graveyard this turn`) or as a count (`count
    /// each times descended this turn`).
    #[must_use]
    pub const fn words(self) -> &'static [&'static str] {
        match self {
            Tally::LeftGraveyard => &["cards", "left", "your", "graveyard", "this", "turn"],
            Tally::Descended => &["times", "descended", "this", "turn"],
            Tally::LandsEntered => &["lands", "entered", "this", "turn"],
            Tally::Exiled => &["cards", "exiled", "this", "turn"],
            Tally::CreatureCardsToGraveyards => {
                &["creature", "cards", "to", "graveyards", "this", "turn"]
            }
            Tally::CreaturesExiledOpponents => {
                &["creatures", "exiled", "under", "opponents", "this", "turn"]
            }
            Tally::DamageSources => &["sources", "dealt", "damage", "this", "turn"],
            Tally::LoyaltyActivated => &["loyalty", "abilities", "activated", "this", "turn"],
            Tally::CreaturesAttacked => &["creatures", "attacked", "this", "turn"],
            Tally::LibraryToGraveyard => &[
                "cards",
                "from",
                "library",
                "to",
                "graveyard",
                "this",
                "turn",
            ],
            Tally::LegendaryCombatDamaged => &[
                "opponent",
                "dealt",
                "combat",
                "damage",
                "by",
                "legendary",
                "creature",
                "this",
                "turn",
            ],
        }
    }

    /// The turn count it reads.
    #[must_use]
    pub const fn count(self) -> mtg_core::card::TurnCount {
        use mtg_core::card::{TurnCount, TurnEvent as E, TurnWho as W};
        match self {
            Tally::LeftGraveyard => TurnCount::of(E::LeftGraveyard, W::You),
            Tally::Descended => TurnCount::of(E::Descended, W::You),
            Tally::LandsEntered => TurnCount::of(E::LandEntered, W::You),
            Tally::Exiled => TurnCount::of(E::Exiled, W::Anyone),
            Tally::CreatureCardsToGraveyards => {
                TurnCount::of(E::CreatureCardToGraveyard, W::Anyone)
            }
            Tally::CreaturesExiledOpponents => TurnCount::of(E::CreatureExiled, W::Opponents),
            Tally::DamageSources => TurnCount::of(E::DamageSource, W::You),
            Tally::LoyaltyActivated => TurnCount::of(E::LoyaltyActivated, W::You),
            Tally::CreaturesAttacked => TurnCount::of(E::Attacked, W::Anyone),
            Tally::LegendaryCombatDamaged => TurnCount::of(E::LegendaryCombatDamaged, W::Opponent),
            Tally::LibraryToGraveyard => TurnCount::of(E::LibraryToGraveyard, W::You),
        }
    }

    /// The tally that reads this turn count, if one does.
    #[must_use]
    pub fn of(c: &mtg_core::card::TurnCount) -> Option<Tally> {
        (c.types.is_empty() && !c.other && c.filter.is_none())
            .then(|| Tally::ALL.into_iter().find(|t| t.count() == *c))
            .flatten()
    }
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
    /// `team controls other Warrior`: "your team controls another
    /// Warrior" (Sickle Dancer). Outside a team game a player's team is
    /// that player alone (CR 810.1), so it is `you control`.
    TeamControls(Filter),
    /// `your turn`: "it's your turn"; `static during your turn, ...`
    /// prints "During your turn, ..." instead.
    YourTurn,
    /// `not your turn`: "it's not your turn"; `static during turns other
    /// than yours, ...` prints "During turns other than yours, ...".
    NotYourTurn,
    /// `not their turn`: "it's not their turn", the player the ability
    /// triggered for (Scytheclaw Raptor).
    NotTheirTurn,
    /// `target player life exactly 10` (after `if`): "target player has
    /// exactly 10 life", declaring the target (`n`); `that target 1 life
    /// 10 or less` checks a target already declared ("that player has 10
    /// or less life").
    TargetLife {
        n: usize,
        life: i32,
        cmp: mtg_core::card::Cmp,
    },
    /// `attacker Rat`: "a Rat is attacking"; `attacker creature at you`:
    /// "a creature is attacking you".
    AttackerIs {
        filter: Filter,
        you: bool,
    },
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
    /// `suspected`: "it's suspected"; `not suspected`: "it's not
    /// suspected" (CR 701.60).
    Suspected(bool),
    /// `opponent life 10 or less`: "an opponent has 10 or less life".
    OpponentLifeAtMost(i32),
    /// `your life 30 or more`: "you have 30 or more life".
    YourLife {
        n: i32,
        at_least: bool,
    },
    /// Your life total against your starting life total: `life above
    /// starting` ("your life total is greater than your starting life
    /// total"), `life at least starting` ("... greater than or equal to
    /// ..."), `life 10 above starting` ("your life total is at least 10
    /// greater than ..."), `life 7 more than starting` ("you have at least
    /// 7 life more than ..."), `life below starting` ("your life total is
    /// less than ..."). `wording` is 0 to 4 in that order.
    LifeVsStarting {
        at_least: i32,
        below: bool,
        wording: u8,
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
    /// `this has one or more +1/+1 counters`: "this creature has one or
    /// more +1/+1 counters on it" (Ingenious Prodigy; `1 or more` prints
    /// "a +1/+1 counter").
    SourceCountersOneOrMore(CounterKind),
    /// `this doesn't have flying counter`: "this creature doesn't have a
    /// flying counter on it" (Inventive Wingsmith; the same condition as
    /// `this has no flying counters`, worded from the source).
    SourceLacksCounter(CounterKind),
    /// `this has even counters`: "it has an even number of counters on
    /// it"; `odd`: "an odd number".
    SourceCounterParity(bool),
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
    /// `entered 3 artifact this turn`: "three or more artifacts entered the
    /// battlefield under your control this turn" (judged as they are or
    /// last existed).
    EnteredAtLeast {
        filter: Filter,
        n: u32,
    },
    /// `you control 1 or more Egg`: "you control one or more Eggs" (the
    /// same condition as `you control Egg`).
    YouControlOneOrMore(Filter),
    /// `you control at most 2 other land`: "you control two or fewer other
    /// lands".
    YouControlAtMost {
        filter: Filter,
        n: u32,
    },
    /// `you control at least 5 other Forest`: "you control at least five
    /// other Forests" (Roiling Canopy; the same condition as `you control
    /// 5 other Forest`).
    YouControlAtLeast {
        filter: Filter,
        n: u32,
    },
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
    /// `top card is Goblin card`, `top card is black`: "the top card of
    /// your library is a Goblin card", "... is black" (by its printed
    /// characteristics).
    TopCard(Filter),
    /// `count 5 colors among permanents`: the count is at least N ("there
    /// are five colors among permanents you control", "the number of ...
    /// plus the number of ... is three or greater").
    CountAtLeast(Box<CountExpr>, u32),
    /// `creature died this turn` (morbid).
    CreatureDiedThisTurn,
    /// `opponent damaged this turn`: "an opponent was dealt damage this
    /// turn".
    OpponentDamagedThisTurn,
    /// `opponent was dealt 7 damage this turn`: "an opponent was dealt 7
    /// or more damage this turn" (`TurnEvent::DamageTaken`).
    OpponentDamageThisTurn(u32),
    /// `no cards in any hand`: "each player has no cards in hand".
    EachHandEmpty,
    /// `library 20 or less`: "a library has twenty or fewer cards in it".
    LibraryAtMost(u32),
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
    /// `beheld`: "a Dragon was beheld" (its behold cost was paid; the
    /// filter is the card's behold cost's).
    Beheld,
    /// `evidence collected`: "evidence was collected" (the optional
    /// additional cost, `additional cost may collect evidence 6`, paid).
    EvidenceCollected,
    /// `additional cost paid`: "this spell's additional cost was paid"
    /// (the same condition, worded for blight).
    AdditionalCostPaid,
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
    /// `activated 4 times this turn`: "this ability has been activated
    /// four or more times this turn".
    ActivatedThisTurn(u32),
    CardsInHand {
        n: u32,
        at_most: bool,
        /// `hand 7 or more your`: "seven or more cards in your hand"
        /// (Resonating Lute); wording only.
        your: bool,
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
    /// The other doesn't hold: written only as a static ability's
    /// `otherwise` (`static enchanted: +2/+2 as long as attached is Vampire
    /// otherwise -2/-2`).
    Not(Box<StaticCond>),
    /// `flash if pay {2} more`: "You may cast this spell as though it had
    /// flash if you pay {2} more to cast it." (flash, and the cost, while
    /// a sorcery couldn't be cast).
    FlashPay(u8),
    /// `hand fewer than 10`: "you have fewer than ten cards in hand" (nine
    /// or fewer).
    HandFewerThan(u32),
    /// `if this is that entered this turn or if you control basic land`
    /// (an activation condition): either holds, printed "... or if ...".
    OrIf(Box<StaticCond>, Box<StaticCond>),
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
    /// `this isnt token`: "it isn't a token" (Sphinx of False
    /// Conclusions); `this is nontoken` prints "it's not a token".
    IsntToken,
    /// `this isnt creature`: "this enchantment isn't a creature" (Answered
    /// Prayers): the source has none of the filter's card types.
    Isnt(Filter),
    /// `modified`: "it's modified" (CR 700.9).
    Modified,
    /// `attacking alone`: "it's attacking alone".
    AttackingAlone,
    /// `attacked or blocked this combat`: "this creature attacked or
    /// blocked this combat".
    AttackedOrBlocked,
    /// `attacked total power 6`: "you attacked with creatures with total
    /// power 6 or greater this combat" (pack tactics).
    AttackedTotalPower(u16),
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
    /// `lost life this turn` ("you've lost life this turn"), `gained and
    /// lost life this turn` ("you gained and lost life this turn").
    LostLifeThisTurn {
        gained: bool,
    },
    /// `descended this turn`: "you descended this turn".
    DescendedThisTurn,
    /// `3 cards left your graveyard this turn`, `1 cards exiled this
    /// turn`: a turn-log count (`Tally`) of `n` or more.
    Tally {
        what: Tally,
        n: u32,
    },
    /// What the source did or had done to it (`SourceHistory`), or with
    /// `not` didn't: `this plus counters put this turn` ("you've put one
    /// or more +1/+1 counters on this creature this turn"), `this counters
    /// put this turn` ("you put a counter on this creature this turn"),
    /// `this not dealt damage` ("it hasn't dealt damage yet"), `this not
    /// dealt combat damage`, `this not entered this turn` ("this creature
    /// didn't enter the battlefield this turn"), `this attacked this
    /// combat` ("Tolsimir attacked this combat").
    SourceHistory {
        what: mtg_core::card::SourceHistory,
        not: bool,
    },
    /// `early turn`: "it's your first, second, or third turn of the game".
    EarlyTurn,
    /// `opponent dealt noncombat damage last turn`: "an opponent was dealt
    /// noncombat damage last turn".
    OpponentNoncombatLastTurn,
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
    /// `scried or surveilled this turn`: "you've scried or surveilled this
    /// turn".
    ScriedOrSurveilledThisTurn,
    /// `2 creatures entered this turn` ("two or more creatures entered the
    /// battlefield under your control this turn"), `opponent had 2
    /// creatures enter this turn` ("an opponent had two or more creatures
    /// enter the battlefield under their control this turn").
    CreaturesEnteredThisTurn {
        n: u8,
        opponent: bool,
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
    /// `your main phase`: "it's your main phase"; `your combat phase`:
    /// "it's your combat phase"; `first combat phase`: "it's the first
    /// combat phase of the turn".
    YourMainPhase,
    YourCombatPhase,
    FirstCombatPhase,
    /// `first end step`: "it's the first end step of the turn".
    FirstEndStep,
    /// `put counter this turn`: "you've put one or more +1/+1 counters on
    /// a creature this turn".
    PutCounterThisTurn,
    /// `opponent dealt noncombat damage this turn`: "an opponent has been
    /// dealt noncombat damage this turn"; `was`: `opponent was dealt
    /// noncombat damage this turn`, "an opponent was dealt ...".
    OpponentNoncombatDamagedThisTurn {
        was: bool,
    },
    /// `resolution 2`: "this is the second time this ability has resolved
    /// this turn"; `resolution 2 again`: "it's the second time" (after the
    /// first such condition).
    NthResolution {
        n: u8,
        again: bool,
    },
    /// `card left your graveyard this turn`: "one or more cards left your
    /// graveyard this turn".
    LeftGraveyardThisTurn,
    /// `creature card to your graveyard this turn`: "a creature card was
    /// put into your graveyard from anywhere this turn".
    CardToGraveyardThisTurn(Vec<mtg_core::card::CardType>),
    /// `sacrificed artifact this turn`: "you've sacrificed an artifact this
    /// turn".
    SacrificedThisTurn(Vec<mtg_core::card::CardType>),
    /// `sacrificed Food this turn`: "you sacrificed a Food this turn" (any
    /// filter, as the permanents last existed); `have sacrificed Food this
    /// turn`: "you've sacrificed a Food this turn"; with a number,
    /// `sacrificed 3 Clue this turn`: "you sacrificed three or more Clues
    /// this turn" (`sacrificed 1 permanent this turn`: "one or more
    /// permanents").
    SacrificedWith {
        filter: Filter,
        n: Option<u32>,
        have: bool,
    },
    /// `more cards in hand than each opponent`.
    MoreCardsInHand,
    /// `opponent poisoned` ("an opponent is poisoned"), `opponent poison 3
    /// or more` ("an opponent has three or more poison counters").
    OpponentPoison(u32),
    /// `you own card in exile`: "you own a card in exile".
    OwnCardInExile,
    /// `you own adventurer card in exile`: "you own a card in exile that
    /// has an Adventure" (`<filter> card`).
    OwnInExile(Filter),
    /// `3 cards exiled with this`: "there are three or more cards exiled
    /// with this creature" (CR 607.2a).
    ExiledWith(u32),
    /// `4 card types exiled with this`: "there are four or more card types
    /// among cards exiled with this creature".
    TypesExiledWith(u32),
    /// `you are monarch`: "you're the monarch" (CR 725).
    Monarch,
    /// `completed dungeon`: "you've completed a dungeon" (CR 309.7).
    CompletedDungeon,
    /// `city's blessing`: "you have the city's blessing" (CR 702.131c).
    CityBlessing,
    /// `enduring story`: "you have an enduring story" (CR 702.195b).
    EnduringStory,
    /// `no enduring story`: "you don't have an enduring story"; after a
    /// static ability's `if`, "... unless you have an enduring story".
    NoEnduringStory,
    /// `max speed`: "you have max speed" (CR 702.179e). Usually written
    /// as the ability word `label "Max speed"` instead.
    MaxSpeed,
    /// `not cast this turn`: "you haven't cast a spell this turn"; `not
    /// cast from hand this turn`: "... a spell from your hand this turn".
    NoSpellsCastThisTurn {
        from_hand: bool,
    },
    /// `didn't cast this turn`: "you didn't cast a spell this turn" (the
    /// same condition, worded in the past).
    DidntCastThisTurn,
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
    /// `x 10 or more`: "X is 10 or more"; `x 2 or less`: "X is 2 or
    /// less" (X of the resolving spell or ability, CR 107.3).
    XIs {
        n: u32,
        at_least: bool,
    },
    /// `not kicked`: "this spell wasn't kicked"; on a card with gift, "the
    /// gift wasn't promised" (CR 702.174).
    NotKicked,
    /// `that spell kicked`: "if that spell was kicked", the spell that
    /// triggered the ability (Bloodstone Goblin).
    ThatKicked,
    /// `sneak paid`, `his sneak paid`, `her sneak paid`: "this spell's
    /// sneak cost was paid" (an instant or sorcery), "its (his, her) sneak
    /// cost was paid" (CR 702.190). The pronoun: 0 its, 1 his, 2 her.
    SneakPaid(u8),
    /// `her sneak paid this turn`: "her sneak cost was paid this turn"
    /// (Karai, Future of the Foot); the pronoun as for `SneakPaid`.
    SneakPaidThisTurn(u8),
    /// `mayhem paid`: "this spell's mayhem cost was paid".
    MayhemPaid,
    /// `prowl paid`: "its prowl cost was paid", "this spell's prowl cost
    /// was paid" (CR 702.76a).
    ProwlPaid,
    /// `more votes for death`: "death gets more votes"; `more votes for
    /// torture or tied`: "torture gets more votes or the vote is tied"
    /// (CR 701.38), after the ability's vote.
    MoreVotes {
        word: String,
        index: u8,
        ties: bool,
    },
    /// `web slung`, `he web slung`, `she web slung`, `they web slung`:
    /// "it was (he was, she was, they were) cast using web-slinging" (CR
    /// 702.188). The pronoun: 0 it, 1 he, 2 she, 3 they.
    WebSlung(u8),
    /// `impending`: "this permanent's impending cost was paid and it has
    /// a time counter on it" (CR 702.176a).
    Impending,
    /// `<cond> and <cond>`: both hold ("there is an Elf card in your
    /// graveyard and this creature has a -1/-1 counter on it").
    And(Box<StaticCond>, Box<StaticCond>),
    /// `cast both creature spell and noncreature spell this turn`:
    /// "you've cast both a creature spell and a noncreature spell this
    /// turn" (each filter at least once).
    CastBoth(Box<(Filter, Filter)>),
    /// `2 creatures died this turn`: "two or more creatures died this
    /// turn"; `creature died under your control this turn`: "a creature
    /// died under your control this turn" (`yours`, `n` 1).
    CreaturesDied {
        n: u32,
        yours: bool,
    },
    /// `opponent controls more lands`: "an opponent controls more lands
    /// than you".
    OpponentMoreLands,
    /// `opponent controls more creatures`, `opponent has more life`,
    /// `opponent has more cards in hand`: "an opponent controls more
    /// creatures than you", ...
    OpponentHasMore(mtg_core::card::MoreWhat),
    /// `that player hand 2 or less`, `defending player hand 2 or less`:
    /// "that player has two or fewer cards in hand", "defending player has
    /// two or fewer cards in hand" (`or more`: "two or more").
    PlayerHand {
        who: mtg_core::card::HandPlayer,
        n: u32,
        at_most: bool,
    },
    /// `that player more cards in hand than you`: "that player has more
    /// cards in hand than you".
    PlayerMoreHand(mtg_core::card::HandPlayer),
    /// `library empty`: "your library has no cards in it".
    LibraryEmpty,
    /// `no cards in library`: the same, printed "there are no cards in
    /// your library" (Living Conundrum).
    NoCardsInLibrary,
    /// `void`: "a nonland permanent left the battlefield this turn or a
    /// spell was warped this turn".
    Void,
    /// `not your main phase`: "it isn't your main phase".
    NotYourMainPhase,
    /// `cast from anywhere other than hand`: "this spell was cast from
    /// anywhere other than your hand".
    NotCastFromHand,
    /// `not cast from hand`: "this spell wasn't cast from your hand" (a
    /// copy wasn't cast, so it qualifies).
    WasntCastFromHand,
    /// `you cast it`: "you cast it" (from any zone).
    WasCast,
    /// `that creature entered from graveyard or you cast it from
    /// graveyard` (0), `they entered or were cast from graveyard` (1),
    /// `one or more of them entered from exile or was cast from exile`
    /// (2): the triggering object was put onto the battlefield from the
    /// zone or cast from it.
    CameFrom {
        zone: mtg_core::state::Zone,
        wording: u8,
    },
    /// `it was cast` (also `he was cast`, `she was cast`, `they were
    /// cast`): "he was cast" by the card's pronoun (Anti-Venom); the same
    /// condition as `you cast it`.
    WasCastPassive,
    /// `put any counter this turn`: "you put a counter on a creature this
    /// turn" (any kind).
    PutAnyCounterThisTurn,
    /// `attacked with 3 Merfolk this turn`: "you attacked with three or
    /// more Merfolk this turn".
    AttackedWithAtLeast(Filter, u32),
    /// `total toughness 10 or more`: "creatures you control have total
    /// toughness 10 or greater".
    TotalToughness(u32),
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

/// The head of a modal ability other than "choose one", "choose one or
/// both" and "choose two" (CR 700.2).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ModeHead {
    /// `choose one or more`: "Choose one or more —", any nonempty set of
    /// the modes.
    OneOrMore,
    /// `choose up to 2`: "Choose up to two —", no mode or up to that many
    /// different modes.
    UpTo(u8),
    /// `choose one or both` as a triggered or activated ability's effect:
    /// "choose one or both —" over two modes, either or both (a spell's
    /// is `Ability::ModalBoth`).
    OneOrBoth,
    /// `choose one unchosen`, `choose one unchosen this turn`: "choose one
    /// that hasn't been chosen [this turn] —" (an ability's modes, each
    /// chosen at most once for the object, or once each turn). `stop`,
    /// `choose one unchosen this turn stop`: the head ends with a period
    /// rather than a dash ("... hasn't been chosen this turn." before the
    /// bullets, Lita, Little Orphan Amphibian; wording only).
    Unchosen { this_turn: bool, stop: bool },
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

/// What a dig's `instead ... if <cond>` changes (CR 608.2c: the later text
/// modifies the earlier instruction): `count 8` ("If this spell was
/// bargained, look at the top eight cards of your library instead."),
/// `take 2`, `take any`, `take all` ("Put each of those cards into your
/// hand instead if ..."), `otherwise onto battlefield` ("If this spell was
/// bargained, put the revealed cards onto the battlefield. Otherwise, put
/// the revealed cards into your hand."). `end` prints the sentence after
/// the one about the rest, `rest` repeats "and the rest ..." in it.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct DigAlt {
    pub cond: InsteadCond,
    /// `Lead`, `Trail`, or `After` (written `lead`, nothing, `after`).
    pub wording: InsteadWording,
    pub count: Option<Num>,
    pub take: Option<u32>,
    pub all: bool,
    /// `otherwise onto battlefield [tapped]`.
    pub onto: Option<bool>,
    pub end: bool,
    pub rest: bool,
}

/// A dig's `field ...` after `revealing`: the card revealed may go onto
/// the battlefield instead of into the hand. `field if your turn`: "You may
/// put it onto the battlefield if it's your turn. If you don't put it onto
/// the battlefield, put it into your hand." `field with mana value 2 or
/// less haste`: "If that card has mana value 2 or less, you may put it onto
/// the battlefield and it gains haste until end of turn. If you didn't put
/// the revealed card onto the battlefield this way, put it into your hand."
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct DigField {
    pub cond: Option<StaticCond>,
    pub max_mv: Option<u32>,
    pub haste: bool,
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
    /// `... instead bare if ...`: the same, the replaced clause without
    /// its "until end of turn" ("That creature gets -4/-4 instead if you
    /// control a creature named Bogbrew Witch", Festering Newt).
    AfterBare,
}

/// Effects that print as one sentence ("target player loses 1 life and you
/// gain 1 life").
#[derive(Clone, Eq, PartialEq, Debug)]
#[allow(clippy::struct_excessive_bools)] // independent wording flags
pub struct Sentence {
    pub effects: Vec<Effect>,
    /// `label "Spell mastery"`: an ability word printed before the
    /// sentence ("Spell mastery — If ...", CR 207.2c).
    pub label: Option<String>,
    /// Starts a new paragraph (a new line of Oracle text).
    pub para: bool,
    /// Printed "Until end of turn, ..." with the duration first (Water
    /// Wings) rather than after each clause; with `lasting`, that duration
    /// first ("Until your next turn, ...").
    pub eot_first: bool,
    /// `... until your next turn`, `... for as long as this remains`, `...
    /// for as long as it remains tapped`, `... for as long as you control
    /// this` (CR 611.2b): what lasts until end of turn lasts this long
    /// instead.
    pub lasting: Option<mtg_core::card::Lasting>,
    /// Printed "Then ..." (`then` before the sentence): "Then that player
    /// discards a card."
    pub then_first: bool,
    /// `for each opponent, ...`: printed "For each opponent, ..."; the
    /// game has one opponent, so it happens once (its targets are written
    /// `that opponent controls`).
    pub per_opponent: bool,
    /// `for each player, ...`: printed "For each player, ..."; its targets
    /// written `that player controls` are one for each player.
    pub per_player: bool,
    /// How each clause after the first joins the sentence
    /// (`joins.len() == effects.len() - 1`).
    pub joins: Vec<Join>,
}

/// How a filter's "not chosen this way" (`StatusQuals::unchosen`) is
/// printed.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum UnchosenWording {
    /// `unchosen`: "other".
    #[default]
    Other,
    /// `not chosen this way`: "... not chosen this way".
    NotChosen,
    /// `except chosen this way`: "... except <noun> chosen this way".
    Except,
}

/// Who makes an `Effect::ChooseFor` choice, as worded.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChooseBy {
    /// `each player chooses ...`: each player for their own.
    Each,
    /// `for each player, you choose ...`.
    You,
    /// `for each player, choose ...`: the controller, no "you".
    Bare,
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
    /// `<sentence> also if <condition>`: "It also gets +1/+0 until end of
    /// turn if you control a Mouse." (Might of the Meek).
    AlsoAfter,
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
            lasting: None,
            then_first: false,
            per_opponent: false,
            per_player: false,
            joins,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Replacement {
    pub event: Replaces,
    pub outcome: Outcome,
    pub then: Vec<Sentence>,
    /// The token an outcome `plus "<token>"` adds (`Outcome::PlusToken`,
    /// whose `CardRef` the compiler fills in).
    pub token: Option<String>,
    /// How the effects after a prevention are printed (rendering only).
    pub wording: ThenWording,
    /// `replace damage to this while this has 1 or more +1/+1 counters:
    /// ...`: "If damage would be dealt to this creature while it has a
    /// +1/+1 counter on it, ..." (Ugin's Conjurant). The ability applies
    /// only while the condition holds (a static ability only).
    pub cond: Option<Box<StaticCond>>,
}

/// How a prevention's effects (`{ ... }`) are printed.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum ThenWording {
    /// A sentence of their own: "... prevent that damage. Put a +1/+1
    /// counter on that creature for each 1 damage prevented this way."
    #[default]
    Sentence,
    /// `prevent and { ... }`: joined to the prevention, "prevent that
    /// damage and put that many +1/+1 counters on him" (Anti-Venom).
    Joined,
    /// `for each { ... }` after a created prevention: "For each 1 damage
    /// prevented this way, put a +1/+1 counter on that creature." (Test of
    /// Faith).
    ForEachFirst,
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
    /// `an opponent loses life during your turn`: "If an opponent would
    /// lose life during your turn, ..." (`your_turn` sets the ability's
    /// condition).
    LoseLife {
        whose: Who,
        your_turn: bool,
    },
    Draw {
        whose: Who,
        library_empty: bool,
        /// `except first in draw step`: "except the first one you draw in
        /// each of your draw steps" (Bard, King of Dale).
        except_first: bool,
    },
    /// `you draw cards`: "if you would draw one or more cards".
    DrawMany {
        whose: Who,
    },
    /// `opponent mills`: "If an opponent would mill one or more cards".
    Mill {
        whose: Who,
    },
    /// `creature you control connives`, `... explores`.
    Connive {
        what: Filter,
    },
    Explore {
        what: Filter,
    },
    /// `card put into your graveyard`; `card or token put into your
    /// graveyard` ("If a card or token would be put into your graveyard
    /// from anywhere").
    ToGraveyard {
        whose: Who,
        from_battlefield: bool,
        tokens: bool,
    },
    /// `instant or sorcery card put into a graveyard`: "If an instant or
    /// sorcery card would be put into a graveyard from anywhere".
    FilteredToGraveyard(Filter),
    /// `tokens created under your control`, `creature tokens created
    /// under your control`; `effect` (`... by an effect`) prints "If an
    /// effect would create one or more tokens under your control".
    CreateTokens {
        creature: bool,
        effect: bool,
        /// `artifact tokens created under your control`.
        artifact: bool,
        /// `artifact tokens you create`: "If you would create one or more
        /// artifact tokens, instead create ...".
        you: bool,
    },
    /// `this put into a graveyard`: "If ~ would be put into a graveyard
    /// from anywhere" (CR 113.6b).
    ThisToGraveyard,
    /// `tapped for mana permanent you control`: "If you tap a permanent for
    /// mana, it produces ... instead" (CR 106.12; `double`, `triple`).
    TappedForMana {
        what: Filter,
    },
    /// `+1/+1 counters put on creature you control`; `+1/+1 counters you
    /// put on ...` ("If you would put one or more +1/+1 counters on ...").
    PlusCounters {
        on: Filter,
        you: bool,
    },
    /// `counters put on permanent you control` ("If one or more counters
    /// would be put on ..."), `counters put by effect on ...` ("If an
    /// effect would put one or more counters on ..."), `counters you put
    /// on ...` ("If you would put one or more counters on ...").
    AnyCounters {
        on: Filter,
        by: mtg_core::card::CountersBy,
    },
    Destroy {
        to: Affects,
    },
    /// `target 1 dies`: "If that creature would die".
    Dies {
        to: Affects,
    },
    Enters,
    /// `each Dragon you control enters`: "Each Dragon you control enters
    /// with an additional +1/+1 counter on it"; without `each`, `other
    /// creature you control enter` ("Other creatures you control enter
    /// with an additional +1/+1 counter on them ...").
    OthersEnter {
        what: Filter,
        each: bool,
    },
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
    /// `damage bare S would deal ...`: the same without "that" ("prevent
    /// all combat damage non-Soldier creatures would deal this turn",
    /// Frontline Strategist).
    WouldDealBare,
    /// `damage this turn to A [from S]`: "that would be dealt this turn to
    /// A [by S]" (Divine Light).
    TurnFirst,
    /// `damage would be dealt to A` (a static replacement): "If A would be
    /// dealt damage, ..." (Unbreathing Horde).
    WouldBeDealt,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Affects {
    /// `except from <filter>` after an effect's `prevent all ... this turn`
    /// (a damage source only): any source but those ("Prevent all combat
    /// damage that would be dealt this turn except combat damage that
    /// would be dealt by enchanted creatures and enchantment creatures.").
    Except(Filter),
    Any,
    This,
    You,
    Players(Who),
    Objects(Filter),
    YouAnd(Filter),
    /// `you andor permanent you control`: the same as `you and`, printed
    /// "you and/or permanents you control" (Harm's Way).
    YouAndOr(Filter),
    /// `opponent and permanent an opponent controls`: "an opponent or a
    /// permanent an opponent controls".
    OpponentsAnd(Filter),
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
    /// `when enchanted dies or is put into exile`: leaves the battlefield
    /// for a graveyard or exile.
    DiesOrExiled(TriggerObj),
    /// `at beginning of every upkeep`: "At the beginning of each upkeep"
    /// (each player's, worded without "player's", as werewolves are).
    EveryStep(Step),
    /// `at beginning of each player's end step`: "At the beginning of each
    /// player's end step" (`each end step` prints "the end step"); the
    /// same trigger.
    EachPlayersEndStep,
    /// `whenever creature or artifact you control is put into a graveyard
    /// from the battlefield`, `when enchanted artifact is put into a
    /// graveyard`: the `Dies` trigger worded so whatever the filter
    /// ("dies" prints only for creatures). Wording only.
    PutInGraveyard {
        obj: TriggerObj,
        from_battlefield: bool,
    },
    BeginningOfStep {
        step: Step,
        whose: Who,
    },
    /// `at beginning of next upkeep`: a delayed trigger's "at the beginning
    /// of the next upkeep" (Aang, at the Crossroads), the same as `at
    /// beginning of the next turn's upkeep` printed without "turn's".
    NextUpkeep,
    /// `at beginning of enchanted controller's upkeep`: "At the beginning
    /// of the upkeep of enchanted creature's controller".
    AttachedStep(Step),
    Attacks(TriggerObj),
    /// "Whenever a creature you control attacks alone".
    AttacksAlone(TriggerObj),
    /// `whenever creature you control attacks a player alone`.
    AttacksPlayerAlone(TriggerObj),
    /// `whenever you attack` (`min` 1), `whenever you attack with 3 or
    /// more creatures`, `whenever this and at least 2 other creatures
    /// attack` (battalion: `this`, `min` 3).
    AttacksWith {
        this: bool,
        min: u8,
    },
    /// `whenever you attack with creatures with total power 12 or greater
    /// first time each turn` ("... for the first time each turn": only
    /// once each turn).
    AttacksWithPower {
        power: u16,
        first: bool,
    },
    /// `whenever a player attacks with 3 or more creatures`, `whenever an
    /// opponent attacks with 2 or more creatures`.
    PlayerAttacksWith {
        whose: Who,
        min: u8,
    },
    /// `whenever one or more creatures you control attack`: the older
    /// wording of `whenever you attack` (CR 508.1m), the same trigger.
    CreaturesYouControlAttack,
    /// `when this is put into a graveyard from anywhere`: "When Purity is
    /// put into a graveyard from anywhere" (CR 603.6a), triggering from
    /// the graveyard.
    ThisToGraveyard,
    /// `whenever 2 or more creatures your opponents control attack`: the
    /// older wording of `whenever an opponent attacks with 2 or more
    /// creatures` (one opponent), the same trigger.
    OpponentCreaturesAttack(u8),
    /// `whenever one or more suspected creature you control attack`: the
    /// older wording of `whenever you attack with one or more ...`;
    /// `... attack a player` (`true`: "Whenever one or more Merfolk you
    /// control attack a player", Attuma).
    OneOrMoreAttack(Filter, bool),
    /// `whenever this attacks and isn't blocked`.
    AttacksUnblocked(TriggerObj),
    /// `whenever this attacks for the first time each turn`.
    AttacksFirstTime(TriggerObj),
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
    /// `whenever this or creature you control leaves the battlefield
    /// without dying`.
    LeavesWithoutDying(TriggerObj),
    /// `whenever other nonland permanent you control is returned to hand`:
    /// "... is returned to its owner's hand".
    ReturnedToHand(TriggerObj),
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
    /// `deals combat damage to a player or planeswalker`.
    DealsCombatDamageToPlayerOrPlaneswalker(TriggerObj),
    /// `whenever this attacks a battle` (CR 310.5).
    AttacksBattle(TriggerObj),
    /// `when you unlock this door` (CR 709.5h).
    UnlocksDoor,
    /// `chapter 1, 2`: a Saga's chapter ability (CR 714.2), printed as
    /// its chapter numbers in Roman numerals, "I, II — ...".
    Chapter(Vec<u8>),
    /// `when this becomes level 2` (CR 716.2c).
    BecomesLevel(u8),
    /// `whenever you fully unlock a Room` (CR 709.5i).
    FullyUnlocks,
    /// `deals combat damage to defending player`: the same trigger, worded
    /// with "defending player" (combat damage to a player is dealt to the
    /// defending player, CR 510.1).
    DealsCombatDamageToDefending(TriggerObj),
    GainsLife {
        whose: Who,
    },
    /// `whenever an opponent loses life`, `whenever a player loses life`.
    LosesLife {
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
    /// `whenever a player or permanent becomes targeted by your ability`:
    /// "Whenever a player or permanent becomes the target of an ability
    /// you control" (Loki, God of Mischief).
    TargetedByYourAbility,
    /// `whenever this becomes targeted first time each turn`: "Whenever
    /// this creature becomes the target of a spell or ability for the first
    /// time each turn" (the spell or ability is `that spell`).
    TargetedFirstTime,
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
    /// `whenever this becomes tapped for teamwork`: "Whenever Agent Maria
    /// Hill becomes tapped to pay a teamwork cost".
    TappedForTeamwork(TriggerObj),
    BecomesUntapped(TriggerObj),
    /// `whenever one or more +1/+1 counters put on this`: "Whenever one or
    /// more +1/+1 counters are put on this creature" (CR 122.1).
    CountersPut {
        obj: TriggerObj,
        kind: CounterKind,
    },
    /// `when nth 4 plan counter put on this`: "When the fourth plan
    /// counter is put on this enchantment".
    NthCounterPut {
        kind: CounterKind,
        n: u32,
    },
    /// `whenever +1/+1 counter removed from this`: "Whenever a +1/+1
    /// counter is removed from this creature" (once per counter; `counter
    /// removed from this` for any kind); `whenever one or more loyalty
    /// counters removed from this`: once per removal, `that much` their
    /// number (Chandra, Fire Artisan).
    CountersRemoved {
        obj: TriggerObj,
        kind: CounterKind,
        one_or_more: bool,
    },
    /// `whenever you put one or more +1/+1 counters on creature you
    /// control`: "Whenever you put one or more +1/+1 counters on a creature
    /// you control"; `... one or more counters on ...` (any kind).
    YouPutCounters {
        obj: TriggerObj,
        kind: Option<CounterKind>,
        /// `... on one or more other Hero you control`: "on one or more
        /// other Heroes you control" (Invisible Woman; wording only).
        many: bool,
    },
    /// `whenever Aura becomes attached to this`: "Whenever an Aura becomes
    /// attached to this creature".
    BecomesAttachedTo(Filter),
    /// `whenever enchanted land is tapped for mana: add {G}`: "Whenever
    /// enchanted land is tapped for mana, its controller adds an
    /// additional {G}." A triggered mana ability (CR 605.1b); its effects
    /// are mana for the tapped permanent's controller, nothing else.
    /// `is tapped for {C}` / `is tapped for chosen` (the kind it must
    /// make), then `passive` ("Whenever a Forest is tapped for mana, its
    /// controller adds an additional {G}.") and `additional` ("Whenever you
    /// tap a Swamp for mana, add an additional {B}."): the form.
    TappedForMana(TriggerObj, mtg_core::card::ManaTriggerForm),
    /// `whenever you tap this for mana`, `whenever a player taps land for
    /// mana`, `whenever an opponent taps Mountain for mana`: not a mana
    /// ability (its effects use the stack, CR 605.1b).
    TapsForMana {
        what: TriggerObj,
        whose: Who,
    },
    /// `when enchanted creature becomes targeted by spell` ("When enchanted
    /// creature becomes the target of a spell"), `whenever permanent you
    /// control becomes targeted by opponent` ("... the target of a spell or
    /// ability an opponent controls"), `... by opponent spell`.
    TargetedBy {
        obj: TriggerObj,
        spells: bool,
        whose: Who,
        whenever: bool,
        /// `whenever creature you control or spell becomes targeted by
        /// opponent`: "Whenever a creature you control or a creature spell
        /// you control becomes the target of ..." (Surrak, Elusive Hunter).
        or_spell: bool,
    },
    /// `whenever this is dealt damage`.
    DealtDamage(TriggerObj),
    /// `when you discard this`: "When you discard this card".
    DiscardsThis,
    /// `whenever this is dealt 3 or more damage`.
    DealtDamageAtLeast(TriggerObj, u32),
    /// `whenever this deals 4 or more damage`: one damage event of at
    /// least that much.
    DealsDamageAtLeast(TriggerObj, u32),
    /// `when that creature dies under your control this turn` (a delayed
    /// trigger about an object).
    DiesUnderYourControl,
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
        /// `from hand`: "... spell from your hand".
        from: mtg_core::card::SpellFrom,
    },
    /// `when this exploits a creature` (CR 702.110).
    Exploits,
    /// `when this becomes plotted` (CR 702.170): "When this card becomes
    /// plotted".
    BecomesPlotted,
    /// `whenever you foretell a card` (CR 702.143a).
    Foretells,
    /// `whenever you draw your second card each turn`; `... card in a
    /// turn` prints that way ("When you draw your third card in a turn").
    DrawsNth {
        whose: Who,
        n: u8,
        in_a_turn: bool,
        /// `whenever you draw your first or second card each turn`: the
        /// `n`th card or the next.
        or_next: bool,
    },
    /// A trigger that would print "Whenever" printed "When" ("When this
    /// creature blocks, return it to its owner's hand"): written `when`.
    When(Box<Trigger>),
    /// `when you next cast instant or sorcery spell` (in a delayed
    /// trigger, followed by `this turn`): "When you next cast an instant
    /// or sorcery spell this turn" (CR 603.7c), once.
    Next(Box<Trigger>),
    /// The "one or more" form of a per-object trigger (CR 603.2c): `one or
    /// more creature card put into your graveyard from anywhere` ("one or
    /// more creature cards are put into your graveyard from anywhere"),
    /// `one or more creature you control become tapped`.
    Batch(Box<Trigger>),
    /// The per-object form of a "one or more" trigger: `a card leaves your
    /// graveyard` ("Whenever a card leaves your graveyard", once per card),
    /// `you put a +1/+1 counter on other creature` ("Whenever you put a
    /// +1/+1 counter on another creature", the older wording).
    Each(Box<Trigger>),
    /// `at beginning of the end step on your next turn` (a delayed
    /// trigger): "At the beginning of the end step on your next turn".
    OnNextTurn(Box<Trigger>),
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
    /// `when haunted dies`: "When the creature this card haunts dies"
    /// (CR 702.55b), a card with `haunt`; `when this enters or haunted
    /// dies`: "When this creature enters or the creature it haunts dies".
    HauntedDies,
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
    /// `whenever creature is put into your graveyard from battlefield`:
    /// "Whenever a creature is put into your graveyard from the
    /// battlefield" (a permanent you own; recover, CR 702.59a).
    PutIntoGraveyardFromBattlefield(Filter),
    /// `whenever creature enters from your graveyard`.
    EntersFromGraveyard(Filter),
    /// `when this enters from graveyard`, `whenever this or permanent
    /// enters from graveyard`, `whenever this or creature enters from your
    /// graveyard`, `whenever permanent you control enters from exile`.
    EntersFrom {
        obj: TriggerObj,
        zone: mtg_core::state::Zone,
        yours: bool,
    },
    /// `whenever you expend 4` (CR 700.14).
    Expends(u8),
    /// `whenever you collect evidence` (CR 701.59).
    CollectsEvidence,
    /// `whenever you manifest dread` (CR 701.62).
    ManifestsDread,
    /// `whenever you discover` (CR 701.57).
    Discovers,
    /// `whenever you give a gift` (CR 702.174).
    GivesGift,
    /// `whenever you solve a Case` (CR 719.3a).
    SolvesCase,
    /// `whenever you forage` (CR 701.61).
    Forages,
    /// `whenever an opponent searches their library` (CR 701.19; `you
    /// search your library`, `a player searches their library`).
    Searches {
        whose: Who,
    },
    /// `whenever you play a land` (CR 305.1), `... a land from exile`,
    /// `... or cast a spell` (The Endstone: one ability with both events),
    /// `whenever you play a land from exile or cast a spell from exile`
    /// (Ghost-Spider), `whenever you play a land or cast a spell from
    /// anywhere other than your hand` (Shadow of the Goblin: both).
    PlaysLand {
        whose: Who,
        from: mtg_core::card::SpellFrom,
        or_cast: bool,
    },
    /// `whenever you untap one or more permanents during your untap
    /// step` (CR 502.3): once for the permanents untapped together, their
    /// number `that much`.
    UntapsDuringUntapStep,
    /// `whenever you cast a spell you don't own` (CR 108.3).
    CastsNotOwned {
        whose: Who,
    },
    /// `whenever enchanted player is dealt damage` (a Curse's player),
    /// `whenever you're dealt combat damage`, `whenever you're dealt
    /// damage`: that player is `that player` ("they").
    PlayerDealtDamage {
        enchanted: bool,
        combat: bool,
    },
    /// `whenever one or more creature you control deal combat damage to a
    /// player`, `... one or more Ally you control ...` (once per player,
    /// CR 603.2c).
    CreaturesDealCombatDamage(Filter),
    /// `whenever one or more Hero you control deal damage to a player`.
    CreaturesDealDamage(Filter),
    /// `whenever a source you control deals noncombat damage to an
    /// opponent` (`yours`), `whenever an opponent is dealt noncombat
    /// damage`, `whenever one or more opponents are dealt noncombat damage`
    /// (`batch`).
    NoncombatDamageToOpponent {
        yours: bool,
        batch: bool,
    },
    /// `whenever you commit a crime` (CR 700.13), `... during your turn`.
    CommitsCrime {
        your_turn: bool,
    },
    /// `whenever an opponent gains control of a permanent from you`.
    OpponentGainsControl,
    /// `whenever you turn a permanent face up` (CR 708.8): a permanent you
    /// control is turned face up; it is "it".
    YouTurnFaceUp,
    /// `whenever you discard one or more cards` (CR 603.2c): their number
    /// is `that many`.
    DiscardsBatch {
        whose: Who,
    },
    /// `whenever you sacrifice one or more Food` (CR 603.2c).
    SacrificesBatch(Filter),
    /// `whenever you attack with one or more Faerie`, `whenever you attack
    /// a player with one or more creature with power 4 or greater`
    /// (`player`).
    AttacksWithFilter {
        filter: Filter,
        player: bool,
        /// `whenever you attack with 2 or more Spider` (0 for "one or
        /// more").
        min: u8,
    },
    /// `whenever one or more creatures attack you`, `whenever one or more
    /// of your opponents are attacked`, `whenever one or more creatures
    /// attack one of your opponents` (`each`).
    PlayersAttacked {
        whose: Who,
        each: bool,
    },
    /// `whenever you gain or lose life`, `... during your turn`.
    GainsOrLosesLife {
        your_turn: bool,
    },
    /// `whenever you scry or surveil`.
    ScriesOrSurveils,
    /// `whenever you scry` (CR 701.22).
    Scries,
    /// `whenever this saddles a Mount or crews a Vehicle`.
    CrewsOrSaddles(TriggerObj),
    /// `whenever this becomes saddled first time each turn`: "Whenever this
    /// creature becomes saddled for the first time each turn".
    BecomesSaddled(TriggerObj),
    /// `whenever creature you control explores` (CR 701.44).
    Explores(TriggerObj),
    /// `whenever you tap an untapped creature an opponent controls`: the
    /// filter after `tap` (`untapped` is wording: only an untapped
    /// permanent can be tapped).
    YouTap(Filter),
    /// `whenever you activate an ability of a creature`, `whenever you
    /// activate an exhaust ability`, `whenever an opponent activates a
    /// loyalty ability`.
    Activates {
        whose: Who,
        kind: mtg_core::card::ActivationKind,
    },
    /// `whenever this transforms` (CR 701.27): "Whenever this creature
    /// transforms into <the name of the face this ability is on>".
    Transforms,
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

/// How a permission to play from the top of a library words "lands and
/// spells" (`TopMore::wording`).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum TopWording {
    /// "play lands and cast spells from the top of your library".
    #[default]
    Split,
    /// `cards`: "play cards from the top of your library".
    Cards,
    /// `card`: "play the top card of your library".
    TopCard,
}

/// The rest of a permission to play from the top of a library
/// (`Ability::PlayFromTop`, `Effect::TopPermission`).
#[derive(Clone, Eq, PartialEq, Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // one per clause of the text
pub struct TopMore {
    /// `may look top and ...`: "You may look at the top card of your
    /// library any time, and you may ...".
    pub look: bool,
    /// `as long as <condition>` (after the permission): "As long as The
    /// Lunar Whale attacked this turn, you may ...".
    pub cond: Option<StaticCond>,
    /// `flash`: "If you cast a spell this way, you may cast it as though it
    /// had flash."
    pub flash: bool,
    /// `pay life`: "If you cast a spell this way, pay life equal to its
    /// mana value rather than pay its mana cost."
    pub life: bool,
    /// `cards`, `card`: how "lands and spells" is worded.
    pub wording: TopWording,
}

/// `Effect::TopPermission`: what is permitted until end of turn.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct TopPlayAst {
    pub lands: bool,
    pub spells: Option<Filter>,
    pub more: TopMore,
}

/// The word before a group in "has all activated abilities of ...":
/// wording only.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum GroupWord {
    /// "of creatures you control ..." (Marvin).
    #[default]
    Bare,
    /// "of all Elf cards in your graveyard".
    All,
    /// "of each other creature with a +1/+1 counter on it".
    Each,
    /// "of the exiled card" (one card exiled with this, Territory Forge).
    The,
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
    /// types"; `add chosen creature type` (`creature`): "the chosen
    /// creature type" (Lifecraft Engine; wording only).
    AddChosenType {
        creature: bool,
    },
    /// `add supertype snow`: "is snow" (CR 205.4).
    AddSupertype(Supertype),
    /// `named "Legitimate Businessperson"`: "... named Legitimate
    /// Businessperson" after what it is (CR 201.1: its other names go).
    SetName(String),
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
    /// `additional +5/+5`: "gets an additional +5/+5" (Elenda, Saint of
    /// Dusk), the same modification.
    ModifyPtAdditional {
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
    /// `has all activated abilities of [all|each] <count>`: "has all
    /// activated abilities of all Elf cards in your graveyard" (layer 6):
    /// those of the objects the count counts (permanents, cards in a
    /// graveyard or all graveyards, cards exiled with this). `the exiled
    /// card` is `exiled with this` printed "the exiled card".
    /// `has all activated abilities as long as on battlefield of ...`
    /// prints "As long as this creature is on the battlefield, it has
    /// ..." (Necrotic Ooze; wording only).
    ActivatedOf {
        from: CountExpr,
        word: GroupWord,
        while_here: bool,
    },
    /// `cda pt <count> [+N]`, `cda power <count>`, `cda toughness
    /// <count>`: a characteristic-defining ability (CR 604.3).
    CdaPt {
        count: CountExpr,
        sets: mtg_core::card::CdaSets,
        plus_toughness: i32,
        /// `cda noun power ...`: "This creature's power is equal to ..."
        /// where Oracle names the card's noun rather than the card
        /// (wording only).
        noun: bool,
    },
}

/// How a back-reference to a group named earlier in the ability is
/// printed (`Recipient::Those`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Those {
    /// `them`: "them" as an object, "they" as a subject.
    Them,
    /// `those creatures`.
    Creatures,
    /// `those permanents`.
    Permanents,
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
    /// "that player" after a target permanent "an opponent controls": the
    /// controller of the `n`th target, printed "that player" (Boseiju, Who
    /// Endures: "That player may search their library ...").
    ThatController(usize),
    /// `controller of the target 1`: "the creature's controller".
    TheControllerOf(usize),
    /// `the target 1`: the `n`th target, "the creature" (Dark Temper's
    /// "destroy the creature instead").
    TheTarget(usize),
    /// `the chosen target 1`: "the chosen creature", a target declared by
    /// "choose target creature" (Vengeful Villagers).
    ChosenTarget(usize),
    /// `the full target 1`, `the chosen full target 1`: the `n`th target
    /// named by its whole filter, "the creature you control", "the chosen
    /// creature you control" (after "Choose target creature you control
    /// and target creature you don't control.").
    TheTargetFull(usize, bool),
    /// `the chosen targets`: targets `first..first + count`, "the chosen
    /// creatures" ("Then the chosen creatures fight each other.").
    ChosenTargets(usize, usize),
    /// `controller of the creature` in a triggered ability: the triggering
    /// object's controller, "the creature's controller" (Creature Bond).
    TheTriggeringController,
    /// The `n`th target again, printed as "that creature" (Act of
    /// Treason's "Untap that creature").
    ThatTarget(usize),
    /// `the other target 2`: the `n`th target, printed "the other" (after
    /// `Effect::PickTarget`: "put two +1/+1 counters on the other").
    OtherTarget(usize),
    /// `its controller`: the controller of the `n`th target (the last one
    /// declared when written).
    ControllerOf(usize),
    /// `its owner`.
    OwnerOf(usize),
    /// The source's owner: the subject carried after `owner shuffles
    /// this into library` (`... and draws 3 cards`: "Gandalf's owner
    /// shuffles him into their library and draws three cards").
    SourceOwner,
    You,
    EachOpponent,
    /// `each opponent who doesn't control Elf`: "each opponent who doesn't
    /// control an Elf" (the group's filter).
    OpponentsWithout(u16),
    /// `each player who doesn't have max speed` (CR 702.179e).
    WithoutMaxSpeed,
    /// `each player who controls most creature`: "each player who controls
    /// the most creatures" (ties included), judged as it happens.
    MostControl(u16),
    /// `each opponent who didn't`, `each player who doesn't`, `each
    /// opponent who can't`: the players who didn't (or couldn't) do what
    /// the last per-player act asked ("Zoyowa deals 3 damage to each
    /// opponent who didn't", "Each opponent who can't loses 3 life.").
    Declined(Declined),
    /// `creature dealt damage this way`: "a creature dealt damage this
    /// way" (each permanent the resolution has dealt damage to).
    DamagedThisWay,
    /// `the rest`: after a choice, what wasn't chosen ("Destroy the
    /// rest.", "then sacrifices the rest").
    Rest,
    /// `each other player`: "each other player", the same players as
    /// each opponent.
    EachOtherPlayer,
    /// `each of your opponents`: "each of your opponents" (each opponent;
    /// wording only, Everything Pizza).
    EachOfYourOpponents,
    /// `an opponent` as the subject of `gains control of`: "An opponent
    /// gains control of this artifact." (the first opponent; the game
    /// has one).
    AnOpponent,
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
    /// The triggering player written `they` ("Whenever a player casts a
    /// noncreature spell, they lose 2 life"; "deals 2 damage to them").
    TriggeringThey,
    /// The triggering object written `the creature` in a triggered
    /// ability: "If the player does, untap the creature." (Paralyze).
    TheTriggeringObject,
    /// `one of them` in a batched trigger ("Whenever one or more other
    /// creatures you control enter, ..., create a token that's a copy of
    /// one of them"): the triggering object, the first of the batch that
    /// made it trigger (the engine picks it for the player).
    TriggeringOneOfThem,
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
    /// `that card` inside a delayed trigger: the object it is about
    /// (`SameObject`), returned as "that card" ("Return that card to the
    /// battlefield ..."), where `it` prints "it".
    SameCard,
    /// `that creature` inside a delayed trigger about a target: the object
    /// the trigger is about (`SameObject`), printed "that creature".
    SameThat,
    /// `that token` inside a delayed trigger about a created token: that
    /// token (`SameObject`), printed "that token" ("Exile that token at
    /// the beginning of the next end step").
    SameToken,
    /// `the token` inside a delayed trigger about a created token: that
    /// token (`SameObject`), printed "the token" ("Exile the token at end
    /// of combat.").
    SameTheToken,
    CreatedObject,
    /// `that created creature`: the created token (`CreatedObject`),
    /// printed "that creature" ("Create a 1/1 ... Otter creature token
    /// ... put a +1/+1 counter on that creature").
    CreatedThat,
    /// `that created token`: the token just created, printed "that token"
    /// ("Then if this spell was cast from a graveyard, put two +1/+1
    /// counters on that token.", Retrieve the Esper); `that token` prints
    /// "it".
    CreatedThatToken,
    /// `that created land`: the land card just put onto the battlefield,
    /// printed "that land" ("Then if you control four or more lands, untap
    /// that land.", Fabled Passage).
    CreatedThatLand,
    /// `that created card`: the card the effect before it moved (the
    /// core `CreatedObject`), printed "that card" ("you may exile a
    /// creature card from your graveyard. If you do, create a token that's
    /// a copy of that card", God-Pharaoh's Gift).
    CreatedThatCard,
    /// `that exiled card`: `CreatedThatCard` printed "the exiled card"
    /// ("Then you may exile an artifact or creature card from your hand.
    /// If you do, create a token that's a copy of the exiled card", Nexus
    /// of Becoming); `exiled card` is a linked ability's (CR 607.2a).
    CreatedExiledCard,
    /// `a card exiled with this`: the card the source's linked ability
    /// exiled (the core `ExiledCard`, CR 607.2a), printed "a card exiled
    /// with this Saga" ("Create a token that's a copy of a card exiled
    /// with this Saga", The Cloning of Shredder).
    ACardExiledWith,
    /// `each card exiled this way` (`each creature card exiled this
    /// way` when `.2`): the targets `.0` to `.0 + .1` (the core
    /// `Targets`), found in exile after the effect before it moved them;
    /// a token copy of them prints "For each card exiled this way, create
    /// a token that's a copy of that card" ("For each creature card
    /// exiled this way, ... a copy of it", Foggy Swamp Visions).
    EachExiledThisWay(usize, usize, bool),
    /// `those tokens`: every token the last token-creating sentence
    /// created; inside a delayed trigger about them, each in turn
    /// (`SameTokens`, the core `SameObject`).
    CreatedTokens,
    SameTokens,
    /// `those cards` inside a delayed trigger about several targets: each
    /// in turn (the core `SameObject`), printed "those cards" ("Return
    /// those cards to the battlefield under their owner's control at the
    /// beginning of the next end step.").
    SameCards,
    /// `the exiled cards` inside a delayed trigger about several targets:
    /// `SameCards` printed "the exiled cards" ("Return the exiled cards to
    /// the battlefield under their owners' control ...", Hide on the
    /// Ceiling).
    SameExiledCards,
    /// `the exiled card` inside a delayed trigger about one target:
    /// `SameCards` for it, printed "the exiled card" ("Return the exiled
    /// card to the battlefield under its owner's control at the beginning
    /// of the next end step.", Identity Thief).
    SameExiledCard,
    /// `them` inside a delayed trigger about created tokens: those tokens
    /// (`SameTokens`), printed "them" ("Exile them at the beginning of the
    /// next end step").
    SameThem,
    /// `those creatures` inside a delayed trigger about a group the
    /// resolution put onto the battlefield (`delayed them:`): each in turn
    /// (the core `SameObject`), printed "those creatures" ("At the
    /// beginning of the next end step, return those creatures to their
    /// owner's hand.", Random Encounter).
    SameCreatures,
    /// `them` after creating tokens: `those tokens` printed "them" ("Put
    /// X +1/+1 counters on each of them").
    CreatedThem,
    /// `those copies`: `those tokens` printed "those tokens" as a subject
    /// too ("Those tokens gain haste until end of turn.", where `those
    /// tokens` prints "They gain ...").
    CreatedThose,
    /// `those creatures` after creating creature tokens, with no group
    /// named: `those tokens` printed "those creatures" ("Create two 1/1
    /// green Elf Warrior creature tokens. Clash with an opponent. If you
    /// win, those creatures gain deathtouch until end of turn.", Gilt-Leaf
    /// Ambush).
    CreatedCreatures,
    /// `manifested creature`: the creature the resolution manifested
    /// (manifest dread, CR 701.62), printed "that creature" ("manifest
    /// dread, then put three +1/+1 counters on that creature").
    Manifested,
    /// `each <filter>` / `all <filter>`: the `n`th of the card's `groups`.
    Group(u16),
    /// `random <filter>`: one of the `n`th group's permanents, picked at
    /// random ("a creature an opponent controls chosen at random").
    RandomOf(u16),
    /// `chosen <filter>` after `becomes a copy of`: one of the `n`th
    /// group's permanents, chosen by the controller as the effect
    /// resolves, not targeted ("you may have it become a copy of another
    /// creature you control", CR 608.2d).
    ChosenOf(u16),
    /// `enchanted creature`, `equipped creature`: what the source is
    /// attached to.
    Attached,
    /// `controller of enchanted`: "enchanted creature's controller".
    AttachedController,
    /// `target creature and all other creature with same name as target
    /// 1`: the `n`th target and the `g`th group, whose filter relates to
    /// that target.
    TargetAnd(usize, u16),
    /// `enchanted creature and each other creature that share creature
    /// type`: what this is attached to and the `g`th group, the creatures
    /// that share a creature type with it, "Enchanted creature and other
    /// creatures that share a creature type with it" (Crown of Vigor).
    AttachedAnd(u16),
    /// `you and all permanent you control gain hexproof`: the controller
    /// and the `g`th group as one subject, "You and permanents you control
    /// gain hexproof until end of turn." (Dawn's Truce).
    YouAnd(u16),
    /// `you and target opponent each draw 1 card`: you and the `n`th
    /// target as one subject, "You and target opponent each draw a card."
    /// (Sky Crier); the effect happens to each, you first.
    YouAndTarget(usize),
    /// `this and up to 1 other target creature each get +3/+3`: the
    /// source and the `n`th target as one subject, "This creature and up
    /// to one other target creature each get +3/+3 until end of turn."
    /// (Razorgrass Invoker).
    SourceAndTarget(usize),
    /// `exiled card`: "the exiled card", what this permanent's linked
    /// ability exiled (CR 607.2a).
    ExiledCard,
    /// `exiled card owner`: "the exiled card's owner".
    ExiledCardOwner,
    /// `exiled cards`: "the exiled cards", `ExiledCard` where the linked
    /// ability may exile several (The Spot, Living Portal).
    ExiledCards,
    /// `granter`: in an ability an Aura or Equipment grants the permanent
    /// it is attached to, that Aura or Equipment, printed by its name
    /// ("Return Trusty Boomerang to its owner's hand").
    Granter,
    /// `granter as it`: the `Granter` printed "it" ("{T}, Unattach Razor
    /// Boomerang: It deals 1 damage to any target"; "Hold for Ransom's
    /// controller sacrifices it").
    GranterIt,
    /// `controller of granter`: "Hold for Ransom's controller".
    GranterController,
    /// `all card exiled with this`, `each creature card exiled with this`:
    /// "all cards exiled with this artifact", what this permanent's
    /// abilities exiled (CR 607.2a) matching the `n`th group's filter.
    ExiledWith(u16),
    /// `1 creature card exiled with this` (`exiled with it` when `.1`):
    /// "a creature card exiled with this land" ("... exiled with it"),
    /// one of the cards this permanent's abilities exiled matching the
    /// `n`th group's filter, chosen as the effect resolves (CR 607.2a,
    /// 608.2d; the core `ChooseAct::ExiledWith`, then `CreatedObject`).
    OneExiledWith(u16, bool),
    /// `1 card exiled with this at random`: as `OneExiledWith`, the card
    /// picked at random ("choose a card at random exiled with this
    /// enchantment and put it onto the battlefield tapped", Omenpath
    /// Journey; the core `ChooseAct::ExiledWithRandom`).
    RandomExiledWith(u16),
    /// `those cards`: the targets declared so far (`first`, `count`),
    /// after an effect moved them ("Exile two target creatures you
    /// control, then return those cards to the battlefield").
    ThoseCards(usize, usize),
    /// `those targets`: every target declared so far, named again by
    /// their kind ("If this spell was kicked, put a stun counter on each
    /// of those creatures").
    ThoseTargets(usize, usize),
    /// `them`, `those creatures`, `those permanents` after a group (`each
    /// <filter>`, `creatures you control`) named earlier in the ability:
    /// the objects that group denoted, even those that no longer match it
    /// ("Gain control of all artifacts your opponents control until end of
    /// turn. Untap them. They gain haste until end of turn.").
    Those(Those),
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
        /// `deal that amount of damage to ...`: "that amount of damage"
        /// for "that much damage" (Fear of Burning Alive). Wording only;
        /// the amount is `Num::ThatMuch`.
        that_amount: bool,
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
    /// `that player loses half life`: "that player loses half their life,
    /// rounded up" (`you lose half life`: "you lose half your life,
    /// rounded up").
    LoseHalfLife {
        who: Recipient,
    },
    /// `you lose game`: "You lose the game." (CR 104.3e).
    LoseGame {
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
    /// `put up to x +1/+0 counters on this capped 4`: "Put up to X +1/+0
    /// counters on this creature. This ability can't cause the total
    /// number of +1/+0 counters on this creature to be greater than
    /// four."
    AddCountersCapped {
        kind: CounterKind,
        count: Num,
        to: Recipient,
        cap: u32,
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
    /// target permanent" (of a kind the controller chooses); `remove up to
    /// 3 counters from target creature` (`up_to` 3).
    RemoveCounterChosen {
        to: Recipient,
        up_to: u8,
    },
    ReturnToHand {
        what: Vec<Recipient>,
        /// `to owner's hand`: a group returned "to their owner's hand"
        /// (Oracle's singular, River's Rebuke); wording only.
        owner_hand: bool,
        /// `put exiled card into hand`: "put the exiled card into its
        /// owner's hand" (Watcher for Tomorrow); `put all card exiled with
        /// this into hand`: "Put all cards exiled with this creature into
        /// their owners' hands."; wording only.
        put: bool,
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
        /// `put that target 1 onto battlefield`: "put that card onto the
        /// battlefield" (wording only).
        put: bool,
        /// `... to battlefield tapped and attacking` (CR 508.4).
        attacking: bool,
        /// `with 2 additional +1/+1 counters`: "with two additional
        /// +1/+1 counters on it" (wording only).
        additional: bool,
        /// `with 1 hexproof counter and 1 indestructible counter`.
        more: Option<(CounterKind, u32)>,
        /// `with 1 additional +1/+1 counter if creature`: "If it enters as
        /// a creature, it enters with an additional +1/+1 counter on it."
        if_creature: bool,
        /// `... if creature late`: the condition printed last, "It enters
        /// with two additional +1/+1 counters on it if it's a creature."
        /// (What Must Be Done). Wording only.
        if_creature_late: bool,
        /// `... to battlefield transformed`, `tapped and transformed`:
        /// "return it to the battlefield transformed under your control"
        /// (CR 712.14a).
        transformed: bool,
        /// `... to battlefield front face up`: "Exile Shiva, then return
        /// it to the battlefield (front face up)." (wording only: a card
        /// put onto the battlefield is front face up unless told
        /// otherwise, CR 712.14).
        front_up: bool,
        /// `...; it enters tapped and attacking that player`: "It enters
        /// tapped and attacking that player." as a sentence of its own
        /// (Shark Shredder; with `tapped` and `attacking`, wording only).
        enters_attacking: bool,
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
    /// `exile all cards from all graveyards`: "Exile all cards from all
    /// graveyards" (Lantern of the Lost): the same effect, worded so.
    ExileAllCardsFromAllGraveyards,
    /// `exile graveyard of target player`: "Exile target player's
    /// graveyard."
    /// `exile all creature card from graveyard of target player`: "Exile
    /// all creature cards from target player's graveyard."
    ExileGraveyard {
        who: Recipient,
        filter: Option<Filter>,
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
    /// `next creature spell free`: "The next creature spell you cast this
    /// turn can be cast without paying its mana cost." (any filter before
    /// `spell`).
    NextSpellFree(Filter),
    /// `next spell can't be countered`: "the next spell you cast this turn
    /// can't be countered".
    NextSpellUncounterable,
    /// `that spell gains deathtouch and lifelink`: "That spell gains
    /// deathtouch and lifelink." (the triggering spell, while it is on the
    /// stack).
    SpellGains {
        what: Recipient,
        keywords: Vec<Keyword>,
    },
    /// `spells artifact cost {1} less this turn`: "Artifact spells you
    /// cast this turn cost {1} less to cast."; `spells black or red cost
    /// {X} less this turn count where life you lost this turn`: "Spells
    /// you cast this turn that are black and/or red cost {X} less to cast,
    /// where X is the amount of life you lost this turn" (X fixed as it
    /// resolves); `next noncreature spell has affinity for artifact`
    /// (`next`): "the next noncreature spell you cast this turn has
    /// affinity for artifacts".
    SpellsThisTurn {
        filter: Filter,
        less: Num,
        affinity: Option<Filter>,
        next: bool,
    },
    /// `play additional land this turn`, `play 2 additional land this
    /// turn`: "You may play an additional land this turn.", "You may play
    /// up to two additional lands this turn." (CR 305.2).
    ExtraLandPlays(u32),
    /// `face-down spells cost {1} less this turn`: "face-down spells you
    /// cast this turn cost {1} less to cast".
    FaceDownSpellsLess(u32),
    TapOrUntap {
        to: Recipient,
    },
    /// `exile same name as target 1 from its controller zones`: "Search its
    /// controller's graveyard, hand, and library for all cards with the
    /// same name as that spell and exile them. Then that player
    /// shuffles."
    ExileSameName {
        what: Recipient,
    },
    /// `exile up to 4 with chosen name from zones of target opponent`
    /// (`any number`, `all`): "Search target opponent's graveyard, hand,
    /// and library for up to four cards with that name and exile them.
    /// Then that player shuffles." after `choose card name`.
    ExileChosenName {
        who: Recipient,
        limit: mtg_core::card::NameSearch,
        /// `then draws for each from hand`, `then creates "2/2 black
        /// Zombie creature" for each from hand`: "That player shuffles,
        /// then draws a card for each card exiled from their hand this
        /// way."
        per_hand: Option<PerHand>,
    },
    /// `ban chosen name until your next turn [opponents|your opponents]
    /// [and lands] [late]`: "Until your next turn, your opponents can't
    /// cast spells with the chosen name." (`late`: "Opponents can't cast
    /// spells with the chosen name until your next turn."), "Until your
    /// next turn, spells with the chosen name can't be cast and lands with
    /// the chosen name can't be played." (`and lands`).
    BanChosenName {
        /// `None`: every player; `Some(true)`: "your opponents";
        /// `Some(false)`: "opponents".
        opponents: Option<bool>,
        lands: bool,
        late: bool,
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
    /// `leaving exiles it`: "If it would leave the battlefield, exile it
    /// instead of putting it anywhere else." (CR 702.84a's replacement).
    ExileOnLeave {
        what: Recipient,
    },
    /// `detain target creature an opponent controls` (CR 701.35); `detain
    /// spelled target nonland permanent`: "Choose target nonland
    /// permanent. Until your next turn, it can't attack or block, and its
    /// activated abilities can't be activated." (Academic Probation).
    Detain {
        what: Recipient,
        spelled: bool,
    },
    Untap {
        to: Vec<Recipient>,
    },
    /// `remove target 1 from combat`: "remove it from combat" (CR 506.4).
    RemoveFromCombat {
        what: Recipient,
    },
    /// `target 1 becomes blocked`: "Target unblocked attacking creature
    /// becomes blocked." (CR 509.1h).
    BecomeBlocked {
        what: Recipient,
    },
    Grant {
        keywords: Vec<Keyword>,
        to: Recipient,
        /// `one or two target creature each gain haste`: "each" after a
        /// variable target group, as some Oracle texts print it.
        each: bool,
        /// `noalso each Warrior creature you control gains lifelink`: no
        /// "also" after a pump of a wider group ("Warrior creatures you
        /// control gain lifelink until end of turn", Rush of Battle).
        /// Wording only.
        no_also: bool,
        /// `this must be blocked each combat this turn if able`: "each
        /// combat this turn" for "this turn" (Anzrag, the Quake-Mole).
        /// Wording only.
        each_combat: bool,
    },
    /// `gain control of target creature` (until end of turn); with
    /// `indefinitely`, for good.
    GainControl {
        of: Recipient,
        forever: bool,
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
        /// `create 1 tapped and attacking "..."`: "create a ... token
        /// that's tapped and attacking" (CR 508.4: never declared as an
        /// attacker); it attacks what the source attacks, else an
        /// opponent.
        attacking: bool,
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
        /// `sized <num>`: the token is X/X, X fixed as it is created ("an
        /// X/X green Elemental creature token, where X is ..."); its
        /// definition's power and toughness print as "X/X".
        sized: Option<Num>,
    },
    /// `create token copy of <what>`, `create 2 token copies of this`
    /// ("create two tokens that are copies of it"), `create x token copies
    /// of target ...` ("Create X tokens that are copies of ...").
    CreateTokenCopy {
        of: Recipient,
        except: bool,
        count: Num,
        /// `create tapped token copy of ...`: "create a tapped token
        /// that's a copy of ..." (CR 614.1c).
        tapped: bool,
        /// `create tapped and attacking token copy of ...`: "create a
        /// tapped and attacking token that's a copy of ..." (CR 508.4).
        attacking: bool,
        /// `create token copy of <what> entering tapped and attacking`: the older
        /// wording "create a token that's a copy of ... and that's tapped
        /// and attacking". Wording only.
        trailing: bool,
        /// `target player creates token copy of ...`: "Target player
        /// creates a token that's a copy of ..." (that player's token).
        who: Option<Recipient>,
    },
    /// `copy target instant or sorcery spell you control [times N] [new
    /// targets] [with exceptions]` (CR 707.10): "Copy target instant or
    /// sorcery spell you control. You may choose new targets for the
    /// copy."; `copy this times x new targets`: "copy it X times. You may
    /// choose new targets for the copies."
    CopySpells {
        what: Recipient,
        times: Num,
        new_targets: bool,
        except: bool,
        /// `copy it and new targets`: "copy it and you may choose new
        /// targets for the copy" (one sentence). Wording only.
        and_may: bool,
    },
    /// `copy target 1 may cast [free]`: "Copy it. You may cast the copy
    /// [without paying its mana cost]." (CR 707.12); `then`: "Copy it,
    /// then you may cast the copy ..."; `may copy`: "You may copy the
    /// exiled card. If you do, you may cast the copy ...".
    CastCopy {
        what: Recipient,
        free: bool,
        wording: CopyCastWording,
    },
    /// `this becomes a copy of <what> [with exceptions]`; another subject
    /// (`target artifact becomes ...`, `each other creature becomes ...`)
    /// and `until end of turn` / `until your next turn` after it.
    BecomeCopy {
        of: Recipient,
        except: bool,
        who: Recipient,
        until: mtg_core::card::CopyUntil,
    },
    CreateEmblem {
        emblem: String,
    },
    /// `counter target spell [to exile | to library top]`.
    Counter {
        to: Recipient,
        dest: CounteredTo,
    },
    /// `change target of target spell or ability with a single target`:
    /// "Change the target of target spell or ability with a single
    /// target." (CR 115.7a).
    ChangeTarget {
        what: Recipient,
    },
    /// `choose new targets for that spell`: "you may choose new targets for
    /// that spell" (CR 115.7d).
    NewTargets {
        what: Recipient,
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
    /// `investigate count each opponents lost life this turn`
    /// ("investigate for each opponent who lost life this turn"); `once`,
    /// `investigate once count each ...`, prints "investigate once for
    /// each ...".
    InvestigateCount {
        count: Num,
        once: bool,
    },
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
    /// `put target creature card exiled with this into graveyard`, `put
    /// each card exiled with this into graveyard`: "Put ... into its
    /// owner's graveyard", from exile (CR 400.3).
    /// `return exiled cards to graveyard` (`.1`): "return the exiled cards
    /// to their owner's graveyard" (Gravegouger).
    PutInGraveyard(Recipient, bool),
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
    /// `cast free noncreature spell from hand`, `cast free spell from hand
    /// with mana value up to that much`, `cast free target instant or
    /// sorcery card from your graveyard exile instead` (CR 608.2g): "you
    /// may cast ... without paying its mana cost".
    CastFree {
        what: Option<Recipient>,
        filter: Option<Filter>,
        max: Option<Num>,
        exile: bool,
        /// `play free exiled card`: "you may play the exiled card without
        /// paying its mana cost" (a land may be played).
        play: bool,
        /// `cast now target instant card from your graveyard` (paid, no
        /// "without paying its mana cost"), `cast now artifact or instant
        /// or sorcery spell from graveyard` (`from`), `any type` (",
        /// and mana of any type can be spent to cast that spell"), `exile
        /// instant or sorcery instead` (Bilbo).
        more: mtg_core::card::CastFreeMore,
        /// How "exile it instead" is worded.
        exile_words: CastExileWords,
        /// `cast now Ally spell from among owned cards exiled with this`:
        /// "an Ally spell from among cards you own exiled with this
        /// creature" rather than "a card exiled with ..." (wording only;
        /// `owned` is `more.owned`).
        among: bool,
    },
    /// `it is now enchantment`: "It's an enchantment." (the Enduring
    /// cycle's returned permanent; "They're an artifact." with `pronoun
    /// they`): its card types become these, indefinitely (layer 4), so it
    /// loses its creature types.
    IsNow {
        to: Recipient,
        types: Vec<CardType>,
    },
    /// `target creature loses all abilities` (until end of turn; layer
    /// 6); `... indefinitely`: no duration ("up to one other target
    /// creature loses all abilities").
    LoseAllAbilities {
        to: Recipient,
        indefinite: bool,
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
    /// `flip 5 coins`: "Flip five coins." (CR 705; no call).
    FlipCoins(u8),
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
    /// exiles a creature they control" (the player chooses); `... and
    /// their graveyard`: "... and their graveyard", their whole graveyard
    /// after.
    ExileChosen {
        who: Recipient,
        count: Num,
        filter: Filter,
        graveyard: bool,
    },
    /// `target opponent chooses creature they control`: "Target opponent
    /// chooses a creature they control." (not targeted); the chosen one is
    /// `that created creature` after it ("Destroy that creature.").
    PlayerChooses {
        who: Recipient,
        filter: Filter,
    },
    /// Permanents chosen for each player and remembered
    /// (`mtg_core::card::ChooseFor`): `each player chooses each of
    /// artifact, creature, enchantment, planeswalker from among nonland
    /// permanent they control` ("Each player chooses an artifact, a
    /// creature, an enchantment, and a planeswalker from among the
    /// nonland permanents they control"), `each player chooses 3
    /// permanent they control`, `that player chooses up to 2 creature
    /// they control`, `each opponent chooses permanent they control of
    /// each permanent type`, `for each player, you choose creature that
    /// player controls`, `for each player, you choose from among
    /// permanent that player controls each of artifact, ...`, `for each
    /// player, choose creature with power 2 or less that player controls`.
    /// Followed by `the rest` (`Recipient::Rest`) or a filter's
    /// `unchosen`.
    ChooseFor {
        who: Recipient,
        by: ChooseBy,
        /// The one kind chosen, `count` of them (unused with `types`).
        filter: Filter,
        count: Num,
        up_to: bool,
        /// `each of artifact, creature, ...`: one of each card type.
        types: Vec<mtg_core::card::CardType>,
        /// `of each permanent type`: one `filter` of each permanent type
        /// (CR 110.4).
        each_type: bool,
        /// `from among <filter> they control`.
        among: Option<Filter>,
    },
    /// `target 1 can't be targeted by opponents this turn`: "Target
    /// creature can't be the target of spells or abilities your opponents
    /// control this turn."; `by spells`: "... can't be the target of
    /// spells this turn."
    CantBeTargeted {
        what: Recipient,
        ban: mtg_core::card::TargetBan,
    },
    /// After `choose 2 target ...` (the last two targets declared): `that
    /// player chooses one of those targets` ("That player chooses one of
    /// those creatures."), `that player sacrifices one of them of their
    /// choice`, `that player chooses and sacrifices one of those
    /// targets`, `their controller chooses and sacrifices one of them`,
    /// `exile one of those targets` (the controller picks). The picked
    /// one is target `first` after it (`that target 1`), the other `the
    /// other target 2`.
    PickTarget {
        first: usize,
        by_controller: bool,
        act: mtg_core::card::PickAct,
        wording: mtg_core::card::PickWording,
    },
    /// `target opponent exiles 2 cards from hand`: "Target opponent exiles
    /// two cards from their hand" (the player chooses); `... from
    /// graveyard`: "from their graveyard".
    ExileFromHand {
        count: Num,
        who: Recipient,
        graveyard: bool,
        /// `... face down` (CR 406.3): "exile a card from your hand face
        /// down" (`exile 1 card from hand face down`, the controller).
        face: mtg_core::card::ExileFace,
        /// `exile hand face down`: "Exile all cards from your hand face
        /// down."; `target player exiles hand face down`.
        all: bool,
        /// `that player exiles 1 nonland card from graveyard`: "they exile
        /// a nonland card from their graveyard".
        filter: Option<Filter>,
    },
    /// `exile library face down`: "Exile all cards from your library face
    /// down."; `each player exiles library but bottom 6 face down`: "each
    /// player exiles all but the bottom six cards of their library face
    /// down".
    ExileLibrary {
        who: Option<Recipient>,
        keep: u32,
        face: mtg_core::card::ExileFace,
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
        /// `target player reveals 3 cards discarding card`: "Target
        /// player reveals three cards from their hand and you choose one
        /// of them. That player discards that card." (Blackmail); `... 3
        /// cards discarding 2 card`: "... You choose two of them. That
        /// player discards those cards." (the count revealed, the number
        /// chosen).
        reveal: Option<(Num, u8)>,
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
        /// `... until this leaves`: "Exile that card until this creature
        /// leaves the battlefield." (CR 610.3; `exile` is `Some`).
        until: bool,
        /// `reveals hand and exiling ...`: "reveals their hand and you
        /// choose ... from it. Exile that card ..." (Brain Maggot); `and
        /// discarding` likewise, "That player discards that card." (Addle).
        and_choose: bool,
        /// `look at hand of target opponent topping card`: "Put that card
        /// on top of that player's library." (Painful Memories) instead of
        /// a discard.
        top: bool,
    },
    /// `transform this`: "transform this creature" (CR 701.27).
    Transform {
        what: Recipient,
    },
    /// `meld with "Midnight Scavengers" into "Chittering Host"`: "if you
    /// both own and control this creature and a creature named Midnight
    /// Scavengers, exile them, then meld them into Chittering Host." (CR
    /// 701.42a); in a triggered ability, an intervening "if" too.
    Meld {
        partner: String,
        into: String,
    },
    /// `flip this`: "flip this creature", "flip it" (CR 710).
    Flip {
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
        /// `search library or graveyard ...`: "search your library and/or
        /// graveyard for ... If you search your library this way,
        /// shuffle."
        graveyard: bool,
        /// `card named "Ajani, Valiant Protector"`: "a card named ...".
        name: Option<String>,
        /// `... to battlefield that card then`: the older wording "and put
        /// that card onto the battlefield. Then shuffle." (wording only).
        old: bool,
        /// `... to battlefield tapped those`: a count-sized search's
        /// newer wording, "..., where X is .... Put those cards onto the
        /// battlefield tapped" ("Reveal those cards, put them into your
        /// hand") (wording only).
        those: bool,
        /// Several differently-filtered cards, more zones, `with different
        /// names`, a destination by the found card, and more wordings.
        more: Option<Box<SearchMore>>,
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
        /// `add {W}{W} or {W}{U} or {U}{U}`: two mana of two colors, the
        /// combinations listed ("Add {W}{W}, {W}{U}, or {U}{U}.", the
        /// filter lands); the same mana as `add 2 mana any combination
        /// of {W}{U}`. Wording only.
        listed: bool,
    },
    /// `add any 2 and any 2 other`: "Add two mana of any one color and two
    /// mana of any other color."
    AddManaTwoColors(u8),
    /// `add any that land opponent controls could produce`, `add any that
    /// basic land you control could produce`: "Add one mana of any color
    /// that a land an opponent controls could produce." (CR 106.7).
    AddManaLandCould {
        whose: Who,
        basic: bool,
        /// `add any type that ...`: "one mana of any type".
        any_type: bool,
    },
    /// `add commander`, `add commander rider counters`, `add commander
    /// rider scry`: "Add one mana of any color in your commander's color
    /// identity." (no mana: the game has no commanders, CR 903.4f), with
    /// Opal Palace's or Path of Ancestry's rider as wording.
    AddManaCommander {
        rider: u8,
    },
    /// `add {R} or {G} count each Raccoon you control` (`each`): "add {R}
    /// or {G} for each Raccoon you control", each mana's color chosen;
    /// `add x {G} or x {W} count where ...`: "Add X {G} or X {W}, where X
    /// is ...", one color chosen for all of it.
    AddManaOneOfTimes {
        mana: Vec<Mana>,
        count: Num,
        each: bool,
    },
    /// `add produced`: in a triggered mana ability, "add one mana of any
    /// type that land produced" (Mana Flare, CR 106.12): a kind of the mana
    /// the tapped permanent made.
    AddManaProduced,
    /// `add exiled color`: "Add one mana of any of the exiled cards'
    /// colors."
    AddManaExiledColors,
    /// `add each color among permanent you control`: "For each color among
    /// permanents you control, add one mana of that color."
    AddManaColorsAmong(Filter),
    Surveil(Num),
    /// `learn` (CR 701.48): there is no sideboard here, so "you may
    /// discard a card; if you do, draw a card" (CR 701.48a's other
    /// choice; no Lesson card is ever outside the game).
    Learn,
    /// `double this power`, `double target creature power and toughness`:
    /// "double its power until end of turn", "Double target creature's
    /// power and toughness until end of turn" (CR 701.10a-c); with `of`,
    /// `double the power of target creature`: "Double the power of target
    /// creature until end of turn".
    DoublePt {
        to: Recipient,
        toughness: bool,
        of: bool,
        /// `triple ...`: "Triple target creature's power and toughness
        /// until end of turn" (Tifa's Limit Break).
        triple: bool,
    },
    /// `double the number of +1/+1 counters on this`, `double the number
    /// of each kind of counter on target permanent` (`kind` None) (CR
    /// 701.10e).
    DoubleCounters {
        kind: Option<CounterKind>,
        to: Recipient,
    },
    /// `double your life total`, `double target player life total`,
    /// `double its controller life total` (CR 701.10d).
    DoubleLife {
        who: Recipient,
    },
    /// `your life total becomes half starting`: "your life total becomes
    /// half your starting life total, rounded up" (CR 119.5, The Endstone).
    LifeBecomesHalfStarting {
        who: Recipient,
    },
    /// `create 3 "..."; if sneak paid, they enter tapped and attacking`
    /// (The Last Ronin's Technique, CR 508.4): only after a sentence
    /// creating tokens, inside `if <condition>,`: the tokens enter tapped
    /// and attacking while the condition holds (compiled as the creation
    /// with an `instead`).
    EnterTappedAttacking,
    /// `you have no maximum hand size for the rest of the game` (CR
    /// 402.2; Spirit Water Revival).
    NoMaxHandSize {
        who: Recipient,
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
        /// `hand and graveyard`: "shuffles their hand and graveyard into
        /// their library" (both zones).
        both: bool,
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
        /// `target 1 is Spirit in addition`: "It's a Spirit in addition to
        /// its other types" (indefinitely; `with flying` prints "It has
        /// flying and is an Angel ...", `and has no abilities` loses them
        /// last). Wording only.
        is_word: bool,
        /// `this becomes 2/4 Wizard creature with ability { ... }`: "it
        /// becomes a 2/4 Wizard creature with \"Whenever ...\"" (Great
        /// Hall of the Biblioplex): it gains the ability for as long as the
        /// rest lasts (layer 6).
        ability: Option<Box<Ability>>,
        /// `becomes legendary Spider Hero`: "a legendary Spider Hero" (CR
        /// 205.4: added to its supertypes).
        supertype: Option<Supertype>,
        /// `named "Mileva, the Stalwart"`: the name it has instead of its
        /// own (CR 201.1).
        name: Option<String>,
        /// `base 4/4 and gains vigilance`: "with base power and toughness
        /// 4/4 and gains vigilance" (Tenth District Hero); wording only.
        gains: bool,
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
        /// `becomes that color`.
        color: bool,
        /// `becomes chosen color indefinitely`: "becomes the chosen
        /// color" (Puca's Eye), with no duration.
        indefinite: bool,
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
        /// `target opponent may have you create 2 "Lander"`: "Target
        /// opponent may have you create two Lander tokens." The player
        /// decides; the effect's controller acts.
        have_you: bool,
    },
    /// `any player may pay 5 life`, `any opponent may sacrifice 1 creature
    /// chosen`, `any player may this deals 6 damage to you`: "Any player
    /// may pay 5 life", "any opponent may sacrifice a creature of their
    /// choice", "Any player may have ~ deal 6 damage to them" (each player
    /// in turn order until one does, acting as the one who does it); a
    /// following `if you do` prints "If a player does", `otherwise` "If no
    /// one does".
    AnyPlayerMay {
        opponents: bool,
        inner: Vec<Sentence>,
    },
    /// `<clause> unless any player may this deals 4 damage to you`: "...
    /// unless a player has this creature deal 4 damage to them" (the
    /// clause happens if no player does it; `AnyPlayerMay`).
    UnlessAnyPlayer {
        act: Box<Effect>,
        effect: Box<Effect>,
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
        /// `when you discard nonland card this way, ...`: "When you
        /// discard a nonland card this way, ..." (Glacial Dragonhunt): it
        /// triggers only if a card the resolution discarded matches.
        discarded: Option<Filter>,
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
        /// `unless that player pays waterbend {4}` ("Ward—Waterbend {4}",
        /// CR 701.67a): artifacts and creatures may be tapped for the
        /// generic part.
        waterbend: bool,
        /// `ward discard 1 card at random`: the cards are chosen at
        /// random (CR 701.9b).
        discard_random: bool,
        /// `ward {2}, pay 2 life`: the mana and the life together (else
        /// mana with life is "{1} or 1 life", either).
        both: bool,
        effect: Box<Effect>,
    },
    /// A ward cost of any kind, from `Mechanic::WardCost` and
    /// `Mechanic::WardAlts` only (no syntax of its own): "counter it
    /// unless that player pays" one of the alternatives (CR 702.21a).
    UnlessPaysCost {
        payer: Recipient,
        alts: Vec<WardAlt>,
        effect: Box<Effect>,
    },
    /// `<clause> unless they sacrifice nonland permanent or discard 1
    /// card`, `<clause> unless target 1 sacrifices creature`: "unless they
    /// sacrifice a nonland permanent of their choice or discard a card".
    UnlessSacrifices {
        payer: Recipient,
        filter: Filter,
        or_discard: bool,
        /// `unless they discard 1 card else sacrifice creature`: "unless
        /// they discard a card or sacrifice a creature" (wording only).
        discard_first: bool,
        effect: Box<Effect>,
    },
    /// `each opponent may discard 1 card or sacrifice permanent`: "Each
    /// opponent may discard a card or sacrifice a permanent of their
    /// choice."; `... may sacrifice nonland permanent or discard 1 card`
    /// (`sacrifice_first`). Those who do neither are `each opponent who
    /// didn't` after it.
    MayOr {
        who: Recipient,
        sacrifice: Filter,
        sacrifice_first: bool,
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
    /// `<what> has base power and toughness X`: "... have base power and
    /// toughness X/X until end of turn" (Biomass Mutation).
    SetBasePtVar {
        to: Recipient,
        n: Num,
    },
    /// `<what> has base power 4` ("has base power 4 until end of turn"),
    /// `<what> base power becomes count equal ...` (`becomes`: "this
    /// creature's base power becomes equal to ...").
    SetBasePower {
        to: Recipient,
        n: Num,
        becomes: bool,
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
        /// `when it dies this turn`: the same, printed "When it dies this
        /// turn" (wording only).
        it: bool,
        /// `delayed target 1: { when ..., <effects> }; <more>`: a
        /// trigger-first body closed by braces, so the sentences after it
        /// are the spell's, not the delayed trigger's (wording only: both
        /// print the same).
        closed: bool,
        /// The targets its own text declares ("Whenever you attack this
        /// turn, target creature you control gets +1/+1"): the first's
        /// index among the ability's targets and their number. The delayed
        /// ability chooses them as it triggers (CR 603.7), not the ability
        /// that creates it; they must be the ability's last targets.
        own_targets: Option<(usize, usize)>,
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
    /// `proliferate` (CR 701.27); `proliferate X times`: "Proliferate X
    /// times." (`count where ...` for X).
    Proliferate {
        times: Option<Num>,
    },
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
    /// `this explores` (CR 701.44); `explores twice`: "explores, then it
    /// explores again".
    Explore {
        what: Recipient,
        twice: bool,
        /// `explores X times`: "it explores X times".
        times: Option<Num>,
    },
    /// `this endures 2`, `that creature endures X` (CR 701.63): "endures
    /// N": N +1/+1 counters on it or an N/N white Spirit token, as its
    /// controller chooses.
    Endure {
        what: Recipient,
        count: Num,
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
    /// Mobilize's expansion only: the tokens just created are attacking.
    CreatedAttack,
    /// `empower jace 3`: "Empower Jace 3." (needs the token "Jace
    /// planeswalker").
    EmpowerJace {
        count: Num,
    },
    /// Echo's bookkeeping (from its expansion).
    ClearEcho,
    /// `manifest`: "manifest the top card of your library" (CR 701.34).
    Manifest,
    /// `manifest dread` (CR 701.62).
    ManifestDread,
    /// `its controller manifests dread` (CR 701.62a): a player other than
    /// the controller manifests dread.
    /// `manifest dread x times`: "Manifest dread X times" (each a separate
    /// manifest dread); `those creatures` after it are the manifested
    /// permanents.
    ManifestDreadTimes(Num),
    ManifestDreadBy {
        who: Recipient,
    },
    /// `behold Dragon`, `behold 2 Elf`: behold as an effect (CR 701.69a),
    /// "you may behold a Dragon. If you do, ...".
    Behold {
        filter: Filter,
        n: u32,
        /// `reveal Dinosaur card from hand`: "reveal a Dinosaur card from
        /// your hand" (the hand only, never a permanent).
        hand: bool,
    },
    /// `reduce speed of <player>`: "If that opponent's speed is greater
    /// than each other player's speed, reduce that opponent's speed by 1.
    /// This effect can't reduce their speed below 1." (CR 702.179).
    ReduceSpeed {
        who: Recipient,
    },
    /// `dread card to hand`: "put a card you put into your graveyard
    /// this way into your hand" in a "whenever you manifest dread" trigger
    /// (the triggering card).
    DreadCardToHand,
    /// `suspect target creature` (CR 701.60a); `<what> no longer
    /// suspected` (`on` false): "... are no longer suspected".
    Suspect {
        what: Recipient,
        on: bool,
    },
    /// `each opponent blights 1` (CR 701.68a).
    BlightBy {
        who: Recipient,
        n: u32,
    },
    /// `forage` (CR 701.61): exile three cards from your graveyard or
    /// sacrifice a Food.
    Forage,
    /// `collect evidence 4` (CR 701.59).
    CollectEvidence(u32),
    /// `blight 2` (CR 701.68).
    Blight(u32),
    /// `adapt 2` (CR 701.46).
    Adapt(u32),
    /// `bolster 2` (CR 701.39).
    Bolster(Num),
    /// `monstrosity 3` (CR 701.37): "Monstrosity 3."
    Monstrosity(Num),
    /// Firebending's effect (CR 702.189a), from its expansion: "add N {R}
    /// that lasts until end of combat".
    Firebend(Num),
    /// `keep mana add {R}`: "Add {R}. Until end of turn, you don't lose
    /// this mana as steps and phases end."
    KeepMana(Vec<Sentence>),
    /// `keep red mana this turn`: "Until end of turn, you don't lose
    /// unspent red mana as steps and phases end."
    KeepRedManaThisTurn,
    /// `add 2 mana any combination only to cast legendary spells`: "Add
    /// two mana in any combination of colors. Spend this mana only to
    /// cast legendary spells." (CR 106.6): the mana is restricted.
    SpendOnly {
        inner: Vec<Sentence>,
        restriction: ManaRestriction,
    },
    /// `add {G} spent on Dragon creature spell: you gain 2 life`: "Add
    /// {G}. When you spend this mana to cast a Dragon creature spell, you
    /// gain 2 life."
    SpendRider {
        inner: Vec<Sentence>,
        spells: Filter,
        then: Vec<Sentence>,
        /// Wording only: 0, `spent on`; 1, `when spent on`: "When that
        /// mana is spent to cast a red instant or sorcery spell, ...";
        /// 2, `if spent on`: "If that mana is spent on a creature spell,
        /// ..."; 3, `if any spent on`: "If any of that mana is spent on
        /// ...".
        wording: u8,
    },
    /// `waterbend {2}` (CR 701.67a), inside "unless" or "you may": pay
    /// the mana, artifacts and creatures tapped to help.
    Waterbend(ManaCost),
    /// `earthbend 2` (CR 701.66): "Earthbend 2." `to` is the implicit
    /// "target land you control" the parser declares.
    Earthbend {
        count: Num,
        to: Recipient,
        /// `you earthbend 4` in a delayed trigger ("When that creature
        /// dies this turn, you earthbend 4.", Fatal Fissure): the land is
        /// the delayed ability's own target, chosen as it triggers; `to`
        /// is unused.
        delayed: bool,
    },
    /// `cloak top` (CR 701.58a): "cloak the top card of your library".
    CloakTop,
    /// `airbend target nonland permanent` (CR 701.65a): "Airbend target
    /// nonland permanent."
    Airbend {
        what: Vec<Recipient>,
    },
    /// `exchange control of 2 target creature`, `exchange control of this
    /// and target nonland permanent` (CR 701.12): "Exchange control of two
    /// target creatures."
    ExchangeControl {
        what: Vec<Recipient>,
    },
    /// `target opponent gains control of target permanent you control`,
    /// `an opponent gains control of this` (CR 613.1b): another player
    /// gains control indefinitely.
    GiveControl {
        of: Recipient,
        to: Recipient,
    },
    /// `exchange life totals with target opponent` (CR 701.12c); a
    /// following `if you do,` prints "If you lost life this way, ...".
    ExchangeLife {
        with: Recipient,
    },
    /// `return all creature card from your graveyard to battlefield`:
    /// "Return all creature cards from your graveyard to the battlefield."
    GraveyardToBattlefield {
        filter: Filter,
        /// `... to battlefield tapped`: "... to the battlefield tapped".
        tapped: bool,
        /// `put all creature card from all graveyards onto battlefield
        /// under your control`: "Put all creature cards from all
        /// graveyards onto the battlefield under your control." (every
        /// graveyard, under the controller's control).
        all_graveyards: bool,
        /// `return each andlist artifact or creature card ...`: "Return
        /// each artifact and creature card ..." (Fix What's Broken);
        /// wording only.
        each: bool,
    },
    /// `return all instant or sorcery card from your graveyard to hand`:
    /// "Return all instant and sorcery cards from your graveyard to your
    /// hand."
    GraveyardToHand {
        filter: Filter,
    },
    /// `recruit` (CR 701.70a): "Recruit." `you recruit` prints "you
    /// recruit" (after a trigger naming another player).
    Recruit {
        you: bool,
    },
    /// `support 2` (CR 701.41a): a +1/+1 counter on each of up to N target
    /// creatures, other than the source in a triggered ability (support on
    /// a permanent); the recipients are those targets.
    Support(Vec<Recipient>),
    /// `populate` (CR 701.36).
    Populate,
    /// `harness this`: "Harness The Mind Stone." (its ∞ ability active).
    Harness,
    /// `turn face up exiled card put creature onto battlefield`: "turn the
    /// exiled card face up. If it's a creature card, put it onto the
    /// battlefield under your control." (Clone Shell, Summoner's Egg; CR
    /// 406.3, 607.2a).
    TurnFaceUpExiled,
    /// `turn face up 1 creature you control chosen`: "turn a creature you
    /// control face up" (Bustle): the controller chooses a permanent
    /// matching the filter as it resolves (not targeted) and turns it face
    /// up without paying a cost (CR 708.8).
    TurnFaceUpChosen {
        filter: Filter,
    },
    /// `this becomes color of choice and gains hexproof from that color`:
    /// "Until end of turn, Mondo Gecko becomes the color of your choice
    /// and gains hexproof from that color."
    ColorOfChoiceHexproof {
        to: Recipient,
    },
    /// `return those exiled cards to hand`: "return those cards to your
    /// hand" (Ignorant Bliss's delayed trigger): the cards the source
    /// exiled face down, still in exile (CR 607.2a).
    ReturnThoseExiled,
    /// `dig face down 5 them`: "Look at the top five cards of your library.
    /// Exile one of them face down and put the rest on the bottom of your
    /// library in a random order." (Discover the Impossible); `dig face
    /// down 4 then any order`: "look at the top four cards of your
    /// library, exile one face down, then put the rest on the bottom of
    /// your library in any order" (Clone Shell). Hideaway's body (CR
    /// 702.75a, 406.3): the controller chooses the card and may look at it;
    /// it is linked to the source, "the exiled card" (CR 607.2a). "In any
    /// order" is wording only (a random order is one of its orders).
    DigFaceDown {
        count: u32,
        then: bool,
        them: bool,
        any_order: bool,
    },
    /// `end the turn` (CR 724.1).
    EndTurn,
    /// `incubate 2` (CR 701.53): the token is "Incubator". `you incubate
    /// 2` (`you`): "you incubate 2" (Assimilate Essence; wording only).
    Incubate {
        count: Num,
        you: bool,
    },
    /// `discover 4` (CR 701.57).
    Discover(Num),
    /// `discover again`: "discover again for the same value" in a
    /// "whenever you discover" trigger (the trigger's amount).
    DiscoverAgain,
    /// Modular's "put its +1/+1 counters on [recipient]" (internal).
    /// Soulbond's pairing (internal; CR 702.95a).
    Soulbond {
        triggering: bool,
    },
    MoveSourceCounters {
        to: Recipient,
    },
    /// Cumulative upkeep's payment (internal).
    CumulativeUpkeep(ManaCost),
    /// `target Mount you control becomes saddled`: "... becomes saddled
    /// until end of turn" (CR 702.171b).
    BecomeSaddledTarget {
        what: Recipient,
    },
    /// `this becomes prepared`: "this creature becomes prepared" (CR
    /// 722.3a); not `on`, `target creature becomes unprepared`.
    BecomePrepared {
        what: Recipient,
        on: bool,
    },
    /// `it becomes plotted`: "It becomes plotted." after the card was
    /// exiled (CR 702.170).
    BecomePlotted {
        what: Recipient,
    },
    /// `plot that spell as it resolves`: "exile that spell instead of
    /// putting it into your graveyard as it resolves. If you do, it
    /// becomes plotted." (Lilah, Undefeated Slickshot).
    PlotOnResolve {
        what: Recipient,
    },
    /// `unlock a locked door of <recipient>`, `unlock a locked door of a
    /// Room you control` (`own`), `lock or unlock a door of <recipient>`
    /// (`lock`): a Room's doors as an effect (CR 709.5).
    Door {
        what: Recipient,
        own: bool,
        lock: bool,
    },
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
        /// `take X card`: exactly that many, a number expression ("Put X
        /// cards from among them into your hand", Stargaze); `take` unused.
        take_num: Option<Num>,
        /// `take 2 creature card with total mana value 6 or less`: the
        /// cards taken have printed mana values adding up to at most this
        /// ("Put up to two creature cards with total mana value 6 or less
        /// from among them onto the battlefield and the rest ...").
        total_mv: Option<Num>,
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
        /// `take creature card may onto battlefield [tapped]`: "You may
        /// put a creature card from among them onto the battlefield
        /// [tapped]"; `Some(tapped)`.
        onto: Option<bool>,
        /// `revealing that card`, wording only: "... and put that card
        /// into your hand".
        that_card: bool,
        /// `short`, wording only: "Put one into your hand and the other
        /// ..." (without "of them").
        short: bool,
        /// `reword`, wording only: "two of them" for "two of those cards",
        /// "one of those cards" for "one of them", and "Then put the rest"
        /// after a card revealed and put into your hand.
        reword: bool,
        /// `revealed`, wording only: "and the rest of the revealed cards
        /// into your graveyard" (Kruphix's Insight).
        rest_revealed: bool,
        /// `rest shuffle`: "Then shuffle." (the rest stay in the library).
        shuffle: bool,
        /// `take creature card at random`: "Put a random creature card
        /// from among them ...".
        at_random: bool,
        /// `split land card creature card`: the cards taken that are land
        /// cards go onto the battlefield tapped, the others into the hand
        /// ("Put all land cards revealed this way onto the battlefield
        /// tapped and put all creature cards revealed this way into your
        /// hand."); the second filter names the others.
        split: Option<(Filter, Filter)>,
        /// `among`, wording only, with `take all`: "Put all enchantment
        /// cards from among them into your hand" for "... revealed this
        /// way ...".
        among: bool,
        /// `instead ... if <cond>`: a later sentence changes the dig when
        /// the condition holds (CR 608.2c).
        alt: Option<Box<DigAlt>>,
        /// `dig 2 until take all land card ...`: "Reveal cards from the top
        /// of your library until you reveal two land cards." (the count is
        /// of cards matching the filter).
        until: bool,
        /// `revealing field ...`: the revealed card may go onto the
        /// battlefield instead.
        field: Option<Box<DigField>>,
        /// `then any land card onto battlefield tapped`: after the take, any
        /// number of the cards left matching this go onto the battlefield
        /// (tapped with `true`) before the rest go ("Then put any number of
        /// land cards from among them onto the battlefield tapped and the
        /// rest into your graveyard.").
        second: Option<Box<(Filter, bool)>>,
        /// `take nonland card may exile plotted rest hand`: "You may exile a
        /// nonland card from among them. If you do, it becomes plotted. Put
        /// the rest into your hand." (`rest_hand`: the rest into the hand).
        exile_plot: bool,
        rest_hand: bool,
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
    /// `piles reveal 5 chosen by opponent` ("Reveal the top five cards of
    /// your library and separate them into two piles. An opponent chooses
    /// one of those piles. Put that pile into your hand and the other into
    /// your graveyard."), `piles look 4 face-down chosen by opponent the
    /// piles` (Riddles in the Dark), `piles reveal 5 separated by
    /// opponent` (Fact or Fiction), `piles look 4 face-down separated by
    /// target opponent` (Fortune's Favor).
    Piles {
        count: Num,
        reveal: bool,
        face_down: bool,
        /// `separated by opponent` (`EachOpponent`: "an opponent") or by
        /// a target; none, the controller.
        separator: Option<Recipient>,
        opponent_chooses: bool,
        /// `the piles`: "one of the piles" rather than "of those piles".
        the_piles: bool,
        /// `one to hand rest bottom` (Truth or Tale), `piles exiled ...
        /// onto battlefield` (Boneyard Parley), `piles exile 3 twice`
        /// (Hostile Negotiations), `piles exile 5 ... play free` (Brilliant
        /// Ultimatum).
        form: mtg_core::card::PileForm,
    },
    /// `split top 3 into hand, top, bottom` ("Look at the top three cards
    /// of your library. Put one of those cards into your hand, one on top
    /// of your library, and one on the bottom of your library."), `split
    /// top 2 of target opponent into exile then` ("Look at the top two
    /// cards of target opponent's library, then exile one of them."),
    /// `split top 3 reveal opponent into graveyard rest hand` ("Reveal the
    /// top three cards of your library. An opponent chooses one of them.
    /// Put that card into your graveyard and the rest into your hand."):
    /// one card to each destination in turn, chosen; the rest to `rest`
    /// or left in place.
    DigSplit {
        count: Num,
        of: Option<Recipient>,
        reveal: bool,
        opponent: bool,
        piles: Vec<mtg_core::card::SplitDest>,
        rest: Option<mtg_core::card::SplitDest>,
        then: bool,
        /// `alternate`: you and a chosen opponent pick in turn ("Then
        /// choose an opponent. They put one on the bottom of your library.
        /// Then you put one into your hand. ...", Memories Returning).
        alternate: bool,
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
    /// `exile until nonland card`: "exile cards from the top of your
    /// library until you exile a nonland card"; `<who> exiles until
    /// nonland card`: "... until they exile a nonland card". The matching
    /// card is `that card` after it.
    ExileUntilCard {
        who: Option<Recipient>,
        filter: Filter,
    },
    /// `exile until total mana value 4`: "Exile cards from the top of
    /// your library until you exile cards with total mana value 4 or
    /// greater."; `each opponent exiles until total mana value 5`: "Each
    /// opponent exiles cards from the top of their library until they
    /// have exiled cards with total mana value 5 or greater this way."
    ExileUntilTotal {
        who: Option<Recipient>,
        total: Num,
    },
    /// `may cast cards exiled this way this turn free`: "Until end of
    /// turn, you may cast cards exiled this way without paying their mana
    /// costs." (`those`: `may cast those cards this turn free`, "... cast
    /// those cards ..."): every card the resolution exiled, still in
    /// exile, may be cast from there this turn (CR 601.3).
    MayCastExiled {
        those: bool,
        free: bool,
    },
    /// Casting from among the cards the resolution exiled or revealed
    /// (CR 608.2g): `cast now that card` ("You may cast that card."),
    /// `cast now that card by discarding` ("... by discarding a card
    /// rather than paying its mana cost"), `cast free it with mana value
    /// up to 8` ("You may cast it without paying its mana cost if that
    /// spell's mana value is 8 or less."; `below <count>`: "if the spell's
    /// mana value is less than ..."), `cast free up to 2 spells from among
    /// exiled cards` ("You may cast up to two spells from among the
    /// exiled cards without paying their mana costs."), `cast free any
    /// number instant or sorcery spells from among them`.
    CastAmong {
        words: AmongWords,
        count: Option<u8>,
        filter: Option<Filter>,
        mv: Option<Num>,
        below: bool,
        cost: mtg_core::card::AmongCost,
    },
    /// `may cast that card this turn`: "You may cast that card this
    /// turn."; `may play that card next turn first`: "Until the end of
    /// your next turn, you may play that card."; `free`: "... without
    /// paying its mana cost"; `it`: "it" for "that card".
    /// `may cast this while exiled`: "You may cast this card for as long
    /// as it remains exiled." (after `exile this from hand` paid).
    MayCastThisFromExile,
    MayPlayCard {
        until: mtg_core::card::PlayUntil,
        cast: bool,
        free: bool,
        first: bool,
        it: bool,
        /// `may play that card from exile this turn`: "You may play that
        /// card from exile this turn." (Norin, Swift Survivalist; wording
        /// only).
        from_exile: bool,
        /// `remains`: `forever` printed "for as long as it remains exiled"
        /// ("For as long as that card remains exiled, you may cast it"
        /// with `first`); wording only.
        remains: bool,
        /// `owner may play that card remains first`: "For as long as that
        /// card remains exiled, its owner may play it" (the permission is
        /// the card's owner's).
        owner: bool,
        /// `may play exiled card this turn`: "you may play the exiled card
        /// this turn" (Reno and Rude; wording only, the resolution's card
        /// as for "that card").
        exiled: bool,
        /// `any type`: "..., and mana of any type can be spent to cast that
        /// spell"; `costs {2} more`: "A spell cast this way costs {2} more
        /// to cast." (`CastRider`).
        rider: Option<Rider>,
    },
    /// `may cast target Zombie creature card from your graveyard this
    /// turn`: "You may cast target Zombie creature card from your
    /// graveyard this turn." (`play`: "You may play ..."), paying its
    /// costs with the usual timing (CR 601.3).
    MayPlayTarget {
        what: Recipient,
        until: mtg_core::card::PlayUntil,
        cast: bool,
        /// `may cast this card from graveyard as adventure next turn`:
        /// "you may cast it from your graveyard as an Adventure until the
        /// end of your next turn" (only the Adventure half, CR 715.3).
        adventure: bool,
        /// `owner may play target 1 remains`: "For each of those cards,
        /// its owner may play it for as long as it remains exiled." (the
        /// targets, wherever they went; each card's owner's permission).
        owner: bool,
        /// `remains` for `forever`: "for as long as it remains exiled".
        remains: bool,
        rider: Option<Rider>,
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
    /// `<who> shuffles` ("Then that player shuffles", CR 701.24); `<who>
    /// shuffles library` ("Target player shuffles their library").
    Shuffle {
        who: Recipient,
        library: bool,
        /// `you may have that player shuffle`: "You may have that player
        /// shuffle." (wording only).
        have: bool,
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
        /// `if revealed creature card, ...`: "If a creature card is
        /// revealed this way, ..." (the card must have been revealed).
        revealed: bool,
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
        /// What "don't" names: wording only.
        words: DontWords,
    },
    /// `if you dont put it into hand, <sentence>`: "If you don't put the
    /// card into your hand, ..." after `if it is ...` (an "otherwise").
    IfYouDontPut {
        to: mtg_core::card::CardDest,
        then: Vec<Sentence>,
        /// `if you dont put it itself into hand`: "If you don't put it
        /// into your hand" (Bucolic Ranch); wording only.
        it: bool,
    },
    /// `reveal it`: "reveal it", the card looked at before (`reveal that
    /// card`: "reveal that card").
    RevealIt {
        that: bool,
    },
    /// `draw 1 card and reveal it`: "Draw a card and reveal it." (it is
    /// `it` after).
    DrawReveal,
    /// `draw another card`: "draw another card", a second draw after an
    /// earlier one (Hymn of the Faller's void clause). One card for the
    /// controller, like `draw 1 card`.
    DrawAnother,
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
        /// `into graveyard of their`: "into their graveyard" (Draugr
        /// Thought-Thief), the same; wording only.
        their: bool,
        /// `on bottom of library random`: "in a random order".
        random: bool,
        /// `put that card into hand and the rest on bottom of library
        /// random`: "Put that card into your hand and the rest on the
        /// bottom of your library in a random order." (the other revealed
        /// cards still in the library; `true`: in a random order).
        rest: Option<(mtg_core::card::CardDest, bool)>,
        /// `onto battlefield tapped and attacking`: "... onto the
        /// battlefield tapped and attacking" (CR 508.4; Raph & Mikey).
        attacking: bool,
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
        /// `choose`: "Choose one of them. ... you may play that card."
        choose: bool,
        /// `choose one`: "Choose one." for "Choose one of them." (Chandra,
        /// Flameshaper; wording only).
        choose_one: bool,
        /// `of target opponent`: "Target opponent exiles the top card of
        /// their library. You may play that card this turn."
        from: Option<Recipient>,
        /// `from target opponent` rather than `of`: "Exile the top three
        /// cards of target opponent's library." (you exile them).
        possessive: bool,
        /// `this way`: "you may play cards exiled this way".
        this_way: bool,
        /// `remains` for `forever`: "for as long as they remain exiled"
        /// ("For as long as that card remains exiled, you may play it"
        /// with `first`); wording only.
        remains: bool,
        /// `face down`: "... library face down. You may look at and play
        /// those cards" (CR 406.3): only the controller knows them.
        face_down: bool,
        /// `look` with `face down`: "Look at the top two cards of your
        /// library and exile them face down." (the permission then says
        /// "play", not "look at and play"); wording only.
        look_first: bool,
        /// `look those face down`: "... and exile those cards face down"
        /// rather than "them"; wording only.
        look_those: bool,
        /// `if you control Kavu` at the end: "you may play it if you
        /// control a Kavu", judged whenever the card could be played.
        cond: Option<StaticCond>,
        /// `any type`: "..., and mana of any type can be spent to cast
        /// them".
        rider: Option<Rider>,
        /// `instead count 3 if additional cost paid`: "If this spell's
        /// additional cost was paid, exile the top three cards instead."
        /// after the first sentence (CR 608.2c; Burning Curiosity).
        alt: Option<Box<(InsteadCond, Num)>>,
    },
    /// `exile top 3 cards`: "Exile the top three cards of your library."
    /// (`exile top 1 card`: "the top card"); `each opponent exiles top 1
    /// card`: "Each opponent exiles the top card of their library."
    /// `face down` adds "face down" (CR 406.3: nobody may look at it),
    /// `face down look` "face down and look at it" (the controller may).
    ExileTop {
        count: Num,
        who: Option<Recipient>,
        face: mtg_core::card::ExileFace,
        /// `exile top 1 card of each player`: "exile the top card of each
        /// player's library" (`who` is the players, printed after).
        of: bool,
    },
    /// `choose one { mode: ... }` as a triggered ability's effect: "When
    /// this creature enters, choose one —" (CR 603.3c).
    Modes(Vec<Mode>),
    /// `choose two { mode: ... }` (three or four unnamed modes) as an
    /// effect: "When this creature enters, choose two —" (CR 700.2d), two
    /// different modes chosen as the ability goes on the stack.
    ModesTwo(Vec<Mode>),
    /// `choose one or more { ... }`, `choose up to 1 { ... }`, `choose one
    /// unchosen { ... }` as a triggered or activated ability's effect.
    ModesOf(ModeHead, Vec<Mode>),
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
    /// `you take extra turn`: "Take an extra turn after this one."; `target
    /// player takes 2 extra turns`: "Target player takes two extra turns
    /// after this one." (CR 500.7).
    ExtraTurns {
        who: Recipient,
        n: u8,
        /// `... without power-up`: "During that turn, power-up abilities
        /// can't be activated." (Kang the Conqueror).
        no_powerup: bool,
    },
    /// `after this phase, additional combat phase`: "After this phase,
    /// there is an additional combat phase."; `after this main phase,
    /// additional combat phase then main phase` ("... followed by an
    /// additional main phase"), `after this main phase, 2 additional
    /// combat phases` ("there are two additional combat phases"); the
    /// trailing order `additional combat phase after this phase then main
    /// phase` ("there is an additional combat phase after this phase
    /// followed by an additional main phase", after an `if`). CR 500.8.
    AdditionalPhases {
        combats: u8,
        main: bool,
        /// "After this phase, there is ..." rather than "there is ...
        /// after this phase".
        leading: bool,
        /// "this main phase" rather than "this phase" (wording only).
        main_phase: bool,
    },
    /// `additional end step after this step` ("There is an additional
    /// end step after this step.", CR 500.10), `you get that much additional upkeep
    /// steps after this phase` ("you get that many additional upkeep
    /// steps after this phase"), `that player gets additional upkeep step
    /// after this step`. `who` is wording only.
    ExtraSteps {
        step: Step,
        n: Num,
        after_phase: bool,
        who: Option<Recipient>,
    },
    /// `only land creature can attack during that combat phase` (Bumi,
    /// Unleashed): after `after this phase, additional combat phase`.
    AddedCombatAttackers(Filter),
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
    /// `target instant or sorcery card from your graveyard gains
    /// flashback`: "Target instant or sorcery card in your graveyard gains
    /// flashback until end of turn. The flashback cost is equal to its
    /// mana cost." (`that_card`, `gains flashback that card`: "... equal
    /// to that card's mana cost.").
    GrantFlashback {
        to: Recipient,
        that_card: bool,
        /// `gains harmonize`: "... gains harmonize until end of turn. Its
        /// harmonize cost is equal to its mana cost."
        harmonize: bool,
    },
    /// `grant graveyard cast to creature cards`: "Creature cards in your
    /// graveyard gain \"You may cast this card from your graveyard\" until
    /// end of turn." (CR 611.2c: the cards there as it resolves).
    GrantGraveyardCast(Filter),
    /// `graveyard cast this turn creature cards by forage finality`:
    /// "Until end of turn, you may cast creature spells from your graveyard
    /// by foraging in addition to paying their other costs. If you cast a
    /// spell this way, that creature enters with a finality counter on
    /// it." (`mtg_core::card::Effect::GraveCastTurn`).
    GraveCastTurn {
        cards: Filter,
        rider: mtg_core::card::GraveRider,
    },
    /// `until end of turn may look top and play from top lands and
    /// spells`: "Until end of turn, you may look at the top card of your
    /// library any time, and you may play lands and cast spells from the
    /// top of your library." (The Belligerent); `... play from top cards
    /// pay life` (Gwenom): "... and you may play cards from the top of your
    /// library. If you cast a spell this way, pay life equal to its mana
    /// value rather than pay its mana cost." The permission lasts the turn.
    TopPermission(Box<TopPlayAst>),
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
    /// `put land card from hand onto battlefield [tapped]`; `put creature
    /// card and land card from hand onto battlefield`: "a creature card
    /// and/or a land card", one of each (or neither).
    PutFromHand {
        filter: Filter,
        tapped: bool,
        also: Option<Filter>,
        /// `... onto battlefield tapped and attacking` (CR 508.4).
        attacking: bool,
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
    /// `if excess damage dealt this way, ...`; `if excess damage dealt to
    /// that permanent this way, ...` (`true`): "If excess damage was dealt
    /// to that permanent this way, ..." (Violent Echoes; wording only).
    IfExcessDamage(Vec<Sentence>, bool),
    /// `put it from graveyard onto battlefield tapped`, after `if you
    /// discard land card this way`: "put it from your graveyard onto the
    /// battlefield tapped" (Silvan Reveler): the card discarded this way,
    /// still in the graveyard (`ChooseAct::DiscardedToBattlefield`).
    PutDiscardedIt,
    /// `either if that target 1 is with power 2 or less, if you control
    /// other Lizard, <sentence>`: "If that creature's power is 2 or less or
    /// if you control another Lizard, ..."; `either same if you control
    /// Squirrel, if you returned Squirrel card to hand this way, ...`: "If
    /// you control a Squirrel or returned a Squirrel card to your hand this
    /// way, ..." (the subject said once). Each check is a conditional
    /// effect (`IfTarget`, `IfCondition`, `IfThisWay`, `IfYouDo`) with no
    /// sentence of its own; the sentence happens if any holds.
    Either {
        conds: Vec<Effect>,
        then: Vec<Sentence>,
        same: bool,
    },
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
        /// `MilledToHand` only: `from among those cards`, "from among
        /// those cards" (Rick Jones).
        among_those: bool,
        /// `MilledToHand` only: `from among milled this way cards`, "from
        /// among the cards milled this way".
        this_way: bool,
        /// `MilledToHand` only: written `return ... from among them to
        /// hand`, "return ... from among them to your hand".
        returns: bool,
        /// `target 1 puts all Goblin card from among milled this way cards
        /// into hand chosen`, wording only: "then puts each Goblin card
        /// milled this way into their hand" (every card goes to its
        /// owner's hand; with `all` no one chooses). With
        /// `ReturnFromGraveyard`, `you return up to 2 creature card from
        /// your graveyard to hand chosen`: "Then you return up to two
        /// creature cards ..." (Unseal the Necropolis), `Some(You)`.
        by: Option<Recipient>,
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
        /// `discards 1 with that name card`: "discards a card with that
        /// name" (one of them).
        one: bool,
    },
    /// `if they can't, you draw 1 card` (after `that player discards 1
    /// with that name card`): "If they can't, you draw a card."
    /// (`Effect::ForEachCouldnt` for one player). `if the player can't, ...`
    /// (`true`): "If the player can't, they lose 5 life." (Cruel Reality),
    /// wording only.
    IfTheyCant(Vec<Sentence>, bool),
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
        ThisWay::Revealed => "revealed",
        ThisWay::PutInGraveyard => "graveyarded",
        ThisWay::Damaged | ThisWay::DamagedPlayer => "damaged",
    }
}

/// The `ThisWay` a word names. Not `tapped`: "the tapped creature's
/// power" is station's count only, which the engine reads as toughness
/// for a creature that "stations permanents using its toughness"
/// (Tapestry Warden, CR 702.184a); written by hand it would too.
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
        ThisWay::Revealed,
        ThisWay::PutInGraveyard,
        ThisWay::Damaged,
    ]
    .into_iter()
    .find(|v| this_way_word(*v) == w)
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ThisWayWording {
    /// "If a creature card was exiled this way".
    Was,
    /// `at least one creature card was exiled this way`: "If at least one
    /// creature card was exiled this way" (Soul-Shackled Zombie).
    AtLeastOne,
    /// "If an artifact is destroyed this way".
    Is,
    /// "If you discarded a card this way", "If you put an artifact card
    /// into your hand this way".
    You,
    /// `you discard land card this way`: the present tense, "If you
    /// discard a land card this way" (Silvan Reveler).
    YouNow,
    /// `you returned Squirrel card to hand this way`: "returned a
    /// Squirrel card to your hand this way" (Cache Grab), put into the
    /// hand.
    Returned,
    /// `you drew cards this way`: "If you draw one or more cards this way"
    /// (Transcendent Archaic): a card put into the hand, from the library.
    Drew,
    /// "If the sacrificed creature was a Human": the noun.
    The(String),
}

/// How a converge count names the object it reads (`CountExpr::SpentColors`):
/// wording only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpentOf {
    /// "this spell" on an instant or sorcery, "it" otherwise.
    #[default]
    Default,
    /// "this spell".
    Spell,
    /// "this creature".
    Creature,
    /// `colors spent that spell`: "that spell", the spell that triggered
    /// the ability (not converge: it reads that spell, not the source).
    That,
}

/// An exert ability's targets and effects (`Ability::Exert`): a lone
/// `WhenYouDo` sentence holds the targets.
pub fn exert_parts(effects: &[Sentence]) -> (&[Target], &[Sentence]) {
    match effects {
        [s] if s.effects.len() == 1 => match &s.effects[0] {
            Effect::WhenYouDo {
                targets, effects, ..
            } => (targets, effects),
            _ => (&[], effects),
        },
        _ => (&[], effects),
    }
}

/// How `Recipient::Declined` names its players.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Declined {
    /// "each player who ..." rather than "each opponent who ...".
    pub player: bool,
    pub tense: DeclinedTense,
    /// `each opponent who didn't sacrifice or discard`: "...
    /// who didn't sacrifice a permanent or discard a card this way".
    pub tail: bool,
}

/// "didn't", "doesn't", or "can't".
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DeclinedTense {
    Didnt,
    Doesnt,
    Cant,
    /// "each player who does": those who did (`Recipient::Accepted`).
    Does,
}

impl Declined {
    /// The words after "who": "didn't", "doesn't", "can't", or "didn't
    /// sacrifice a permanent or discard a card this way".
    pub fn verb(self) -> &'static str {
        match (self.tense, self.tail) {
            (DeclinedTense::Didnt, true) => {
                "didn't sacrifice a permanent or discard a card this way"
            }
            (DeclinedTense::Didnt, false) => "didn't",
            (DeclinedTense::Doesnt, _) => "doesn't",
            (DeclinedTense::Cant, _) => "can't",
            (DeclinedTense::Does, _) => "does",
        }
    }
}

impl Effect {
    /// The sentence a check `Effect::Either` joins says, if this is one.
    pub fn either_body_mut(&mut self) -> Option<&mut Vec<Sentence>> {
        match self {
            Effect::IfTarget(_, _, inner, _)
            | Effect::IfCondition(_, inner, IfWording::Lead)
            | Effect::IfThisWay(_, inner)
            | Effect::IfYouDo(inner) => Some(inner),
            _ => None,
        }
    }
}

/// `Ability::ActivatedOr`'s second cost: `cost` with its last part (a
/// sacrifice, a discard, or life) replaced by `other` (one such part).
/// `None` when either isn't of that shape.
pub fn or_cost_second(cost: &Cost, other: &Cost) -> Option<Cost> {
    fn part_only(c: &Cost) -> Option<CostItem> {
        let [item] = c.order.as_slice() else {
            return None;
        };
        let bare = Cost {
            order: c.order.clone(),
            sacrifice: c.sacrifice.clone(),
            chosen: c.chosen,
            discard: c.discard,
            discard_filter: c.discard_filter.clone(),
            discard_random: c.discard_random,
            life: c.life,
            ..Cost::default()
        };
        (matches!(
            item,
            CostItem::Sacrifice | CostItem::Discard | CostItem::Life
        ) && bare == *c)
            .then_some(*item)
    }
    part_only(other)?;
    let mut out = cost.clone();
    match out.order.pop()? {
        CostItem::Sacrifice => {
            out.sacrifice = None;
            out.chosen = mtg_core::card::ChosenCost::default();
        }
        CostItem::Discard => {
            out.discard = 0;
            out.discard_filter = None;
            out.discard_random = false;
        }
        CostItem::Life => out.life = 0,
        _ => return None,
    }
    if other.sacrifice.is_some() {
        if out.sacrifice.is_some() {
            return None;
        }
        out.sacrifice.clone_from(&other.sacrifice);
        out.chosen = other.chosen;
    }
    if other.discard > 0 {
        if out.discard > 0 {
            return None;
        }
        out.discard = other.discard;
        out.discard_filter.clone_from(&other.discard_filter);
        out.discard_random = other.discard_random;
    }
    out.life += other.life;
    out.order.extend(other.order.iter().copied());
    Some(out)
}
