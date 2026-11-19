//! Printed card definitions (CR 200–212): the immutable characteristics a
//! card has in the card database, before any effect applies. Definitions
//! are built by the `mtg-dsl` compiler from the card files under `cards/`;
//! the [`Builder`] here is what it (and tests) construct them with.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::mana::{Color, ColorSet, Mana, ManaCost};

/// Index of a definition in a [`CardPool`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct CardRef(pub u32);

/// Card types (CR 205.2a).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum CardType {
    Artifact,
    Battle,
    Creature,
    Enchantment,
    Instant,
    Kindred,
    Land,
    Planeswalker,
    Sorcery,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub struct TypeSet(u16);

impl TypeSet {
    pub fn of(types: impl IntoIterator<Item = CardType>) -> Self {
        types.into_iter().fold(TypeSet(0), TypeSet::with)
    }

    #[must_use]
    pub const fn with(self, t: CardType) -> Self {
        TypeSet(self.0 | (1 << (t as u16)))
    }

    #[must_use]
    pub const fn without(self, t: CardType) -> Self {
        TypeSet(self.0 & !(1 << (t as u16)))
    }

    pub const fn contains(self, t: CardType) -> bool {
        self.0 & (1 << (t as u16)) != 0
    }

    pub const fn contains_all(self, other: TypeSet) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: TypeSet) -> bool {
        self.0 & other.0 != 0
    }

    #[must_use]
    pub const fn union(self, other: TypeSet) -> Self {
        TypeSet(self.0 | other.0)
    }

    /// Number of types in the set.
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn is_permanent_type(self) -> bool {
        self.contains(CardType::Artifact)
            || self.contains(CardType::Battle)
            || self.contains(CardType::Creature)
            || self.contains(CardType::Enchantment)
            || self.contains(CardType::Land)
            || self.contains(CardType::Planeswalker)
    }
}

impl fmt::Debug for TypeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for t in [
            CardType::Artifact,
            CardType::Battle,
            CardType::Creature,
            CardType::Enchantment,
            CardType::Instant,
            CardType::Kindred,
            CardType::Land,
            CardType::Planeswalker,
            CardType::Sorcery,
        ] {
            if self.contains(t) {
                if !first {
                    write!(f, " ")?;
                }
                write!(f, "{t:?}")?;
                first = false;
            }
        }
        Ok(())
    }
}

/// Supertypes (CR 205.4a).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum Supertype {
    Basic,
    Legendary,
    Snow,
    World,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub struct SupertypeSet(u8);

impl SupertypeSet {
    pub fn of(types: impl IntoIterator<Item = Supertype>) -> Self {
        types.into_iter().fold(SupertypeSet(0), SupertypeSet::with)
    }

    #[must_use]
    pub const fn with(self, t: Supertype) -> Self {
        SupertypeSet(self.0 | (1 << (t as u8)))
    }

    pub const fn contains(self, t: Supertype) -> bool {
        self.0 & (1 << (t as u8)) != 0
    }

    pub const fn contains_all(self, other: SupertypeSet) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: SupertypeSet) -> bool {
        self.0 & other.0 != 0
    }
}

impl fmt::Debug for SupertypeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for t in [
            Supertype::Basic,
            Supertype::Legendary,
            Supertype::Snow,
            Supertype::World,
        ] {
            if self.contains(t) {
                if !first {
                    write!(f, " ")?;
                }
                write!(f, "{t:?}")?;
                first = false;
            }
        }
        Ok(())
    }
}

/// Keyword abilities implemented in M1 (CR 702). Each is desugared by the
/// engine into the rules that define it; the keyword itself is only what
/// other effects refer to ("has flying").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum Keyword {
    Deathtouch,
    Defender,
    DoubleStrike,
    FirstStrike,
    Flash,
    Flying,
    Haste,
    Hexproof,
    Indestructible,
    Lifelink,
    Menace,
    /// CR 702.108: a triggered ability; see `CardPool::keyword_trigger`.
    Prowess,
    Reach,
    Trample,
    Vigilance,
    /// "This creature can't attack." Not a CR keyword; a restriction the
    /// engine keeps with the keywords so effects can grant it.
    CantAttack,
    /// "This creature can't block."
    CantBlock,
    /// "This creature attacks each combat if able."
    MustAttack,
    /// "This creature can't be blocked."
    Unblockable,
    /// Landwalk (CR 702.14): can't be blocked as long as the defending
    /// player controls a land with that basic land type.
    Plainswalk,
    Islandwalk,
    Swampwalk,
    Mountainwalk,
    Forestwalk,
    /// Protection from a color (CR 702.16): can't be damaged, enchanted or
    /// equipped, blocked, or targeted by anything of that color (DEBT).
    ProtectionFromWhite,
    ProtectionFromBlue,
    ProtectionFromBlack,
    ProtectionFromRed,
    ProtectionFromGreen,
    /// CR 702.114: this object is colorless (a characteristic-defining
    /// ability applied to the printed values in every zone).
    Devoid,
    /// CR 702.73: this object is every creature type, in every zone.
    Changeling,
    /// CR 702.18: can't be the target of spells or abilities.
    Shroud,
    /// CR 702.36: can't be blocked except by artifact and/or black creatures.
    Fear,
    /// CR 702.13: can't be blocked except by artifact creatures and/or
    /// creatures that share a color with it.
    Intimidate,
    /// CR 702.28: can block or be blocked by only creatures with shadow.
    Shadow,
    /// CR 702.31: can't be blocked except by creatures with horsemanship.
    Horsemanship,
    /// CR 702.118: can't be blocked by creatures with greater power.
    Skulk,
    /// CR 702.80: damage to creatures is dealt as -1/-1 counters.
    Wither,
    /// CR 702.90: damage to creatures as -1/-1 counters, to players as
    /// poison counters.
    Infect,
    /// CR 702.83: a triggered ability; see `CardPool::keyword_trigger`.
    Exalted,
    /// CR 702.79: a triggered ability (return with a -1/-1 counter).
    Persist,
    /// CR 702.93: a triggered ability (return with a +1/+1 counter).
    Undying,
    /// Protection from a quality other than a color ("protection from
    /// artifacts", CR 702.16): a marker that one of the object's
    /// abilities is an `AbilityDef::Protection`. Not printed.
    ProtectionFromFilter,
    /// "This creature can't attack alone." (CR 506.5)
    CantAttackAlone,
    /// "This creature can't block alone."
    CantBlockAlone,
    /// CR 702.25: a triggered ability; see `CardPool::keyword_trigger`.
    Flanking,
    /// "Doesn't untap during its controller's untap step" (CR 502.3): a
    /// restriction kept with the keywords.
    DoesntUntap,
    /// "Can't become untapped": no effect or rule untaps it (the untap
    /// step included), so a stun counter on it stays (CR 122.1d needs it
    /// to be about to become untapped).
    CantBecomeUntapped,
    /// CR 702.51: creatures tapped while casting pay for {1} or one mana of
    /// their color.
    Convoke,
    /// CR 702.126: artifacts tapped while casting pay for {1}.
    Improvise,
    /// Marks a card with evoke (CR 702.74): its "sacrifice it as it enters
    /// if evoked" trigger. Printed as "Evoke {cost}", not as a keyword.
    Evoke,
    /// CR 702.85: as it's cast, exile cards from the top of the library
    /// until a cheaper nonland card, which may be cast free.
    Cascade,
    /// CR 702.88: cast from hand, it's exiled as it resolves and may be
    /// cast free at its owner's next upkeep.
    Rebound,
    /// CR 702.110: "When this creature enters, you may sacrifice a
    /// creature."
    Exploit,
    /// CR 702.66: each card exiled from the graveyard while casting pays
    /// for {1}.
    Delve,
    /// "This creature blocks each combat if able." A blocking requirement
    /// (CR 509.1c).
    MustBlock,
    /// "This creature must be blocked if able." A blocking requirement on
    /// the defending player's creatures (CR 509.1c).
    MustBeBlocked,
    /// "All creatures able to block this creature do so." One requirement
    /// per creature able to block it (CR 509.1c).
    Lure,
    /// "This creature can block an additional creature each combat."
    /// (CR 509.1a; instances don't add up here.)
    BlockAdditional,
    /// "This creature can block any number of creatures."
    BlockAny,
    /// "This creature can attack as though it didn't have defender." (CR
    /// 702.3b): defender doesn't stop it attacking.
    AttackDespiteDefender,
    /// "Its activated abilities can't be activated" (Arrest): a
    /// restriction kept with the keywords, covering mana abilities too
    /// (CR 605.1a, 602.5).
    CantActivate,
    /// "Its activated abilities can't be activated unless they're mana
    /// abilities" (Realmbreaker's Grasp).
    CantActivateNonMana,
}

/// The pool's built-in triggered abilities that aren't keywords on a
/// permanent: they're queued by the engine directly.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Builtin {
    /// CR 702.35a: "When this card is exiled this way, its owner may cast
    /// it by paying its madness cost. If that player doesn't, they put
    /// this card into their graveyard."
    Madness,
    /// CR 702.62a: "At the beginning of your upkeep, if this card is
    /// suspended, remove a time counter from it", and as the last is
    /// removed, its owner may cast it without paying its mana cost.
    SuspendUpkeep,
    /// CR 702.88a: "At the beginning of your next upkeep, you may cast
    /// this card from exile without paying its mana cost."
    Rebound,
    /// CR 702.85a: "When you cast this spell, exile cards from the top of
    /// your library until you exile a nonland card whose mana value is
    /// less than this spell's. You may cast it without paying its mana
    /// cost. Put the exiled cards on the bottom in a random order."
    Cascade,
    /// CR 702.109a: "At the beginning of the next end step, return this
    /// permanent to its owner's hand" (a dashed permanent's delayed
    /// trigger).
    DashReturn,
}

/// How an exiled card is cast by a built-in trigger.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ExileCast {
    /// For its madness cost; if not cast, to the graveyard.
    Madness,
    /// Without paying its mana cost.
    Free,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub struct KeywordSet(u64);

impl KeywordSet {
    pub const NONE: KeywordSet = KeywordSet(0);

    pub fn of(keywords: impl IntoIterator<Item = Keyword>) -> Self {
        keywords.into_iter().fold(KeywordSet(0), KeywordSet::with)
    }

    #[must_use]
    pub const fn with(self, k: Keyword) -> Self {
        KeywordSet(self.0 | (1 << (k as u64)))
    }

    #[must_use]
    pub const fn union(self, other: KeywordSet) -> Self {
        KeywordSet(self.0 | other.0)
    }

    #[must_use]
    pub const fn difference(self, other: KeywordSet) -> Self {
        KeywordSet(self.0 & !other.0)
    }

    pub const fn contains(self, k: Keyword) -> bool {
        self.0 & (1 << (k as u64)) != 0
    }

    pub const fn intersects(self, other: KeywordSet) -> bool {
        self.0 & other.0 != 0
    }

    /// The five landwalk keywords (CR 702.14).
    pub const LANDWALK: KeywordSet = KeywordSet::NONE
        .with(Keyword::Plainswalk)
        .with(Keyword::Islandwalk)
        .with(Keyword::Swampwalk)
        .with(Keyword::Mountainwalk)
        .with(Keyword::Forestwalk);

    /// Evasion keywords `can_block` checks beyond flying and landwalk.
    pub const EVASION: KeywordSet = KeywordSet::NONE
        .with(Keyword::Fear)
        .with(Keyword::Intimidate)
        .with(Keyword::Shadow)
        .with(Keyword::Horsemanship)
        .with(Keyword::Skulk);

    /// Keywords that stand for triggered abilities.
    pub const TRIGGERED: KeywordSet = KeywordSet::NONE
        .with(Keyword::Prowess)
        .with(Keyword::Exalted)
        .with(Keyword::Persist)
        .with(Keyword::Undying)
        .with(Keyword::Flanking)
        .with(Keyword::Evoke)
        .with(Keyword::Exploit);

    /// The five protection-from-a-color keywords (CR 702.16).
    pub const PROTECTION: KeywordSet = KeywordSet::NONE
        .with(Keyword::ProtectionFromWhite)
        .with(Keyword::ProtectionFromBlue)
        .with(Keyword::ProtectionFromBlack)
        .with(Keyword::ProtectionFromRed)
        .with(Keyword::ProtectionFromGreen)
        .with(Keyword::ProtectionFromFilter);

    pub const fn contains_all(self, other: KeywordSet) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Keyword> {
        Keyword::ALL.into_iter().filter(move |k| self.contains(*k))
    }
}

impl Keyword {
    pub const ALL: [Keyword; 63] = [
        Keyword::Deathtouch,
        Keyword::Defender,
        Keyword::DoubleStrike,
        Keyword::FirstStrike,
        Keyword::Flash,
        Keyword::Flying,
        Keyword::Haste,
        Keyword::Hexproof,
        Keyword::Indestructible,
        Keyword::Lifelink,
        Keyword::Menace,
        Keyword::Prowess,
        Keyword::Reach,
        Keyword::Trample,
        Keyword::Vigilance,
        Keyword::CantAttack,
        Keyword::CantBlock,
        Keyword::MustAttack,
        Keyword::Unblockable,
        Keyword::Plainswalk,
        Keyword::Islandwalk,
        Keyword::Swampwalk,
        Keyword::Mountainwalk,
        Keyword::Forestwalk,
        Keyword::ProtectionFromWhite,
        Keyword::ProtectionFromBlue,
        Keyword::ProtectionFromBlack,
        Keyword::ProtectionFromRed,
        Keyword::ProtectionFromGreen,
        Keyword::Devoid,
        Keyword::Changeling,
        Keyword::Shroud,
        Keyword::Fear,
        Keyword::Intimidate,
        Keyword::Shadow,
        Keyword::Horsemanship,
        Keyword::Skulk,
        Keyword::Wither,
        Keyword::Infect,
        Keyword::Exalted,
        Keyword::Persist,
        Keyword::Undying,
        Keyword::ProtectionFromFilter,
        Keyword::CantAttackAlone,
        Keyword::CantBlockAlone,
        Keyword::Flanking,
        Keyword::DoesntUntap,
        Keyword::Convoke,
        Keyword::Improvise,
        Keyword::Evoke,
        Keyword::Cascade,
        Keyword::Rebound,
        Keyword::Exploit,
        Keyword::Delve,
        Keyword::MustBlock,
        Keyword::MustBeBlocked,
        Keyword::Lure,
        Keyword::BlockAdditional,
        Keyword::BlockAny,
        Keyword::AttackDespiteDefender,
        Keyword::CantActivate,
        Keyword::CantActivateNonMana,
        Keyword::CantBecomeUntapped,
    ];

    /// The keyword as Oracle prints it.
    pub const fn name(self) -> &'static str {
        match self {
            Keyword::Deathtouch => "deathtouch",
            Keyword::Defender => "defender",
            Keyword::DoubleStrike => "double strike",
            Keyword::FirstStrike => "first strike",
            Keyword::Flash => "flash",
            Keyword::Flying => "flying",
            Keyword::Haste => "haste",
            Keyword::Hexproof => "hexproof",
            Keyword::Indestructible => "indestructible",
            Keyword::Lifelink => "lifelink",
            Keyword::Menace => "menace",
            Keyword::Prowess => "prowess",
            Keyword::Reach => "reach",
            Keyword::Trample => "trample",
            Keyword::Vigilance => "vigilance",
            Keyword::CantAttack => "can't attack",
            Keyword::CantBlock => "can't block",
            Keyword::MustAttack => "attacks each combat if able",
            Keyword::Unblockable => "can't be blocked",
            Keyword::Plainswalk => "plainswalk",
            Keyword::Islandwalk => "islandwalk",
            Keyword::Swampwalk => "swampwalk",
            Keyword::Mountainwalk => "mountainwalk",
            Keyword::Forestwalk => "forestwalk",
            Keyword::ProtectionFromWhite => "protection from white",
            Keyword::ProtectionFromBlue => "protection from blue",
            Keyword::ProtectionFromBlack => "protection from black",
            Keyword::ProtectionFromRed => "protection from red",
            Keyword::ProtectionFromGreen => "protection from green",
            Keyword::Devoid => "devoid",
            Keyword::Changeling => "changeling",
            Keyword::Shroud => "shroud",
            Keyword::Fear => "fear",
            Keyword::Intimidate => "intimidate",
            Keyword::Shadow => "shadow",
            Keyword::Horsemanship => "horsemanship",
            Keyword::Skulk => "skulk",
            Keyword::Wither => "wither",
            Keyword::Infect => "infect",
            Keyword::Exalted => "exalted",
            Keyword::Persist => "persist",
            Keyword::Undying => "undying",
            Keyword::ProtectionFromFilter => "protection",
            Keyword::CantAttackAlone => "can't attack alone",
            Keyword::CantBlockAlone => "can't block alone",
            Keyword::Flanking => "flanking",
            Keyword::DoesntUntap => "doesn't untap during its controller's untap step",
            Keyword::CantBecomeUntapped => "can't become untapped",
            Keyword::Convoke => "convoke",
            Keyword::Improvise => "improvise",
            Keyword::Evoke => "evoke",
            Keyword::Cascade => "cascade",
            Keyword::Rebound => "rebound",
            Keyword::Exploit => "exploit",
            Keyword::Delve => "delve",
            Keyword::MustBlock => "blocks each combat if able",
            Keyword::MustBeBlocked => "must be blocked if able",
            Keyword::Lure => "lure",
            Keyword::BlockAdditional => "can block an additional creature each combat",
            Keyword::BlockAny => "can block any number of creatures",
            Keyword::AttackDespiteDefender => "can attack as though it didn't have defender",
            Keyword::CantActivate => "its activated abilities can't be activated",
            Keyword::CantActivateNonMana => {
                "its activated abilities can't be activated unless they're mana abilities"
            }
        }
    }

    /// The color a protection keyword protects from (CR 702.16).
    pub const fn protection(self) -> Option<Color> {
        match self {
            Keyword::ProtectionFromWhite => Some(Color::White),
            Keyword::ProtectionFromBlue => Some(Color::Blue),
            Keyword::ProtectionFromBlack => Some(Color::Black),
            Keyword::ProtectionFromRed => Some(Color::Red),
            Keyword::ProtectionFromGreen => Some(Color::Green),
            _ => None,
        }
    }

    /// The protection keyword for a color.
    pub const fn protection_from(c: Color) -> Keyword {
        match c {
            Color::White => Keyword::ProtectionFromWhite,
            Color::Blue => Keyword::ProtectionFromBlue,
            Color::Black => Keyword::ProtectionFromBlack,
            Color::Red => Keyword::ProtectionFromRed,
            Color::Green => Keyword::ProtectionFromGreen,
        }
    }

    /// The basic land type a landwalk keyword looks for (CR 702.14).
    pub const fn landwalk(self) -> Option<Color> {
        match self {
            Keyword::Plainswalk => Some(Color::White),
            Keyword::Islandwalk => Some(Color::Blue),
            Keyword::Swampwalk => Some(Color::Black),
            Keyword::Mountainwalk => Some(Color::Red),
            Keyword::Forestwalk => Some(Color::Green),
            _ => None,
        }
    }

    /// The landwalk keyword for a basic land type.
    pub const fn landwalk_for(c: Color) -> Keyword {
        match c {
            Color::White => Keyword::Plainswalk,
            Color::Blue => Keyword::Islandwalk,
            Color::Black => Keyword::Swampwalk,
            Color::Red => Keyword::Mountainwalk,
            Color::Green => Keyword::Forestwalk,
        }
    }
}

impl fmt::Debug for KeywordSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// An interned subtype (creature type, land type, ...). The five basic land
/// types are interned first, in WUBRG order, so `Subtype::basic(color)` is
/// constant.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct Subtype(pub u16);

impl Subtype {
    pub const fn basic(c: Color) -> Subtype {
        Subtype(c as u16)
    }

    /// Interned in `NONCREATURE_SUBTYPES` order after the basic types.
    pub const EQUIPMENT: Subtype = Subtype(5 + 18);
    pub const AURA: Subtype = Subtype(5 + 30);
    pub const TREASURE: Subtype = Subtype(5 + 27);

    /// The color a basic land type produces (CR 305.6), if this is one.
    pub fn basic_color(self) -> Option<Color> {
        Color::ALL.get(self.0 as usize).copied()
    }

    /// Whether this is a creature type (CR 205.3m), for changeling. The
    /// pool interns the basic land types and then `NONCREATURE_SUBTYPES`
    /// first, so every later index is a creature type (planeswalker types
    /// are counted as creature types; no filter tells them apart).
    pub const fn is_creature_type(self) -> bool {
        self.0 as usize >= 5 + NONCREATURE_SUBTYPES.len()
    }
}

/// Land, artifact, enchantment, spell, and battle subtypes (CR 205.3g–k),
/// other than the basic land types: interned right after them.
pub const NONCREATURE_SUBTYPES: [&str; 52] = [
    "Cave",
    "Cloud",
    "Desert",
    "Gate",
    "Lair",
    "Locus",
    "Mine",
    "Power-Plant",
    "Sphere",
    "Tower",
    "Town",
    "Urza's",
    "Planet",
    "Attraction",
    "Blood",
    "Bobblehead",
    "Clue",
    "Contraption",
    "Equipment",
    "Food",
    "Fortification",
    "Gold",
    "Incubator",
    "Junk",
    "Lander",
    "Map",
    "Powerstone",
    "Treasure",
    "Vehicle",
    "Spacecraft",
    "Aura",
    "Background",
    "Cartouche",
    "Case",
    "Class",
    "Curse",
    "Role",
    "Room",
    "Rune",
    "Saga",
    "Shard",
    "Shrine",
    "Adventure",
    "Arcane",
    "Lesson",
    "Trap",
    "Omen",
    "Siege",
    "Infinity",
    "Stone",
    "Mutagen",
    "Chorus",
];

/// A small fixed-capacity list of `Copy` items. `Characteristics` is copied
/// on every rules query, so its lists must be plain data, not heap vectors.
/// Items beyond the capacity are dropped (with a debug assertion); the DSL
/// gate rejects cards that would need more.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct FixedVec<T: Copy + Default, const N: usize> {
    len: u8,
    items: [T; N],
}

impl<T: Copy + Default, const N: usize> Default for FixedVec<T, N> {
    fn default() -> Self {
        FixedVec {
            len: 0,
            items: [T::default(); N],
        }
    }
}

impl<T: Copy + Default + PartialEq, const N: usize> FixedVec<T, N> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_slice(items: &[T]) -> Self {
        let mut v = Self::new();
        for i in items {
            v.push(*i);
        }
        v
    }

    pub fn push(&mut self, item: T) {
        let i = usize::from(self.len);
        if i < N {
            self.items[i] = item;
            self.len += 1;
        } else {
            debug_assert!(false, "FixedVec capacity {N} exceeded");
        }
    }

    pub fn as_slice(&self) -> &[T] {
        &self.items[..usize::from(self.len)]
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    pub fn len(&self) -> usize {
        usize::from(self.len)
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn contains(&self, item: &T) -> bool {
        self.as_slice().contains(item)
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        let mut out = Self::new();
        for i in self.as_slice() {
            if keep(i) {
                out.push(*i);
            }
        }
        *self = out;
    }
}

impl<T: Copy + Default + PartialEq, const N: usize> FromIterator<T> for FixedVec<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = Self::new();
        for i in iter {
            v.push(i);
        }
        v
    }
}

impl<T: Copy + Default + fmt::Debug, const N: usize> fmt::Debug for FixedVec<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(&self.items[..usize::from(self.len)])
            .finish()
    }
}

impl<T: Copy + Default + Serialize, const N: usize> Serialize for FixedVec<T, N> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.items[..usize::from(self.len)].serialize(serializer)
    }
}

impl<'de, T: Copy + Default + PartialEq + Deserialize<'de>, const N: usize> Deserialize<'de>
    for FixedVec<T, N>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let items: Vec<T> = Vec::deserialize(deserializer)?;
        if items.len() > N {
            return Err(serde::de::Error::custom(format!(
                "list of {} exceeds capacity {N}",
                items.len()
            )));
        }
        Ok(FixedVec::from_slice(&items))
    }
}

impl<'a, T: Copy + Default, const N: usize> IntoIterator for &'a FixedVec<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items[..usize::from(self.len)].iter()
    }
}

pub type SubtypeSet = FixedVec<Subtype, 6>;
pub type AbilitySet = FixedVec<AbilityId, 8>;

/// Reference to an ability definition in the pool.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct AbilityId(pub u32);

/// CR 613 layers.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub enum Layer {
    L1Copy,
    L2Control,
    L3Text,
    L4Type,
    L5Color,
    L6Ability,
    L7aCda,
    L7bSetPt,
    L7cModifyPt,
    L7dSwitchPt,
}

impl Layer {
    pub const ALL: [Layer; 10] = [
        Layer::L1Copy,
        Layer::L2Control,
        Layer::L3Text,
        Layer::L4Type,
        Layer::L5Color,
        Layer::L6Ability,
        Layer::L7aCda,
        Layer::L7bSetPt,
        Layer::L7cModifyPt,
        Layer::L7dSwitchPt,
    ];
}

/// Whose objects a filter matches, relative to the effect's controller.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Who {
    Any,
    You,
    Opponents,
}

/// How a filter relates to the effect's own source.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Relation {
    /// The source may match like any other object.
    #[default]
    Any,
    /// Excludes the source ("each other ...").
    Other,
    /// Only the source itself ("~", "this creature").
    This,
}

/// Which battlefield objects a static ability's effect applies to
/// (CR 611.3: re-evaluated as the layers are applied).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
#[allow(clippy::struct_excessive_bools)] // independent conditions
pub struct ObjFilter {
    pub who: Option<Who>,
    /// Must have every one of these types.
    pub types: TypeSet,
    /// Must have none of these types.
    pub not_types: TypeSet,
    pub supertypes: SupertypeSet,
    pub not_supertypes: SupertypeSet,
    /// Must share a color with this set.
    pub colors: Option<ColorSet>,
    /// Must share no color with this set ("nonblack").
    pub not_colors: ColorSet,
    /// Must have every one of these keywords ("creature with flying").
    pub keywords: KeywordSet,
    /// Must have none of these keywords ("creature without flying").
    pub not_keywords: KeywordSet,
    /// Must have every one of these subtypes ("Goblin").
    pub subtypes: SubtypeSet,
    /// Must have none of these subtypes ("non-Aura").
    pub not_subtypes: SubtypeSet,
    /// Whether the source itself is excluded or required.
    pub relation: Relation,
    /// Only the object the source is attached to ("enchanted creature").
    pub enchanted: bool,
    /// Only objects with no abilities.
    pub no_abilities: bool,
    /// Must have at least one of these types ("artifact or enchantment").
    pub any_types: TypeSet,
    /// "with power 2 or less" and the like.
    pub power: Option<(Cmp, i16)>,
    pub toughness: Option<(Cmp, i16)>,
    pub mana_value: Option<(Cmp, u8)>,
    /// "multicolored", "monocolored", "colorless".
    pub color_count: Option<ColorCount>,
    /// Game-state conditions `matches` leaves to the caller
    /// (`GameState::status_ok`): tapped or untapped, attacking, blocking.
    pub tapped: Option<bool>,
    pub attacking: bool,
    pub blocking: bool,
    /// "that was dealt damage this turn" (damage is marked until the
    /// cleanup step, CR 514.2).
    pub damaged: bool,
    /// More game-state qualifiers ("blocked", "enchanted", "that entered
    /// this turn", "with mana value X"), judged by `GameState::status_ok`.
    pub status: StatusQuals,
    /// An alternative: matches this filter or that one ("creature or
    /// Vehicle", "attacking or blocking creature").
    pub or: Option<Box<ObjFilter>>,
    /// A spell "that targets ..." (judged with the spell's targets, where a
    /// spell is targeted).
    pub targeting: Option<Box<SpellTargets>>,
    /// "of the chosen color", "of the chosen type", "of that type": of the
    /// color or type chosen as the source entered (CR 607.2a) or as the
    /// effect resolves (CR 608.2d). A placeholder the engine fills in
    /// (`with_chosen`) where the choice is known; unfilled, nothing
    /// matches.
    pub chosen: bool,
    /// "named ~", "not named ~", "with the same name as that creature",
    /// "that shares a color with it": a relation to the source or a target.
    /// A placeholder the engine fills in (`with_same`) where the object is
    /// known; unfilled, nothing matches.
    pub same: Option<Same>,
    /// "named ~" filled in: the object must (or, with `false`, must not)
    /// have this definition's name. The pool holds one definition per card
    /// name (CR 201.2), so the definition stands for the name; tokens of
    /// different definitions that share a name are not recognized as such.
    pub named: Option<(CardRef, bool)>,
    /// "attached to it" (CR 301.5, 303.4): the object must be attached to
    /// this. Judged where the source and targets are known (a count, a
    /// group of an effect); elsewhere it doesn't restrict.
    pub attached_to: Option<AttachedTo>,
    /// "blocking this creature", "it's blocking", "blocking or blocked by
    /// this creature" (CR 509.1g): a combat relation to the source. Judged
    /// where the source is known (a group of an effect, a target);
    /// elsewhere, or once the source has left the battlefield (its last
    /// known information, CR 113.7a), it doesn't restrict.
    pub combat_with: Option<CombatWith>,
}

/// How an object a filter matches must be in combat with the source.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CombatWith {
    /// It is blocking the source.
    Blocking,
    /// The source is blocking it.
    BlockedBy,
    /// Either.
    BlockingOrBlockedBy,
}

/// Game-state qualifiers of a filter beyond tapped, attacking, blocking,
/// and damage marked (`ObjFilter::status`); all are judged by
/// `GameState::status_ok`, so a filter without them costs nothing more.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // independent qualifiers
pub struct StatusQuals {
    /// "blocked creature" (`true`) or "unblocked creature" (`false`): an
    /// attacking creature once blockers are declared (CR 509.1h); before
    /// then it is neither.
    pub blocked: Option<bool>,
    /// "enchanted permanent" (`true`, CR 303.4: an Aura is attached to
    /// it) or "that isn't enchanted" (`false`).
    pub enchanted: Option<bool>,
    /// "that entered this turn": its timestamp, the time it entered (CR
    /// 613.7e), is from this turn.
    pub entered_this_turn: bool,
    /// "that dealt damage this turn": it was the source of damage dealt
    /// this turn (CR 120.1).
    pub dealt_damage: bool,
    /// "that blocked this turn": it was declared as a blocker this turn
    /// (CR 509.1).
    pub blocked_this_turn: bool,
    /// "with mana value X": `ObjFilter::mana_value`'s number is the X of
    /// the spell (CR 107.3, 601.2b: X is announced before targets are
    /// chosen), filled in by the engine where X is known; elsewhere the
    /// mana value doesn't restrict.
    pub mana_value_x: bool,
}

impl StatusQuals {
    /// Whether any qualifier is set.
    pub const fn any(&self) -> bool {
        self.blocked.is_some()
            || self.enchanted.is_some()
            || self.entered_this_turn
            || self.dealt_damage
            || self.blocked_this_turn
            || self.mana_value_x
    }
}

/// What an object a filter matches must be attached to.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum AttachedTo {
    /// The source ("for each Aura and Equipment attached to it" on a
    /// creature).
    Source,
    /// What the source is attached to ("for each Equipment attached to
    /// it" on an Equipment: the equipped creature).
    SourceHost,
    /// The `n`th target ("Destroy all Equipment attached to that
    /// creature").
    Target(u8),
}

/// What a filter's `same` relates the object to (CR 201.2: "named ~" is
/// the object's name, CR 201.4b; CR 105.4: sharing a color).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct Same {
    pub of: SameOf,
    pub what: SameWhat,
}

/// The object a `Same` relation is about.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum SameOf {
    /// The source ("named ~").
    Source,
    /// The `n`th target ("with the same name as that creature").
    Target(u8),
}

/// What a `Same` relation compares.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum SameWhat {
    /// Has the same name.
    Name,
    /// Doesn't have the same name ("not named ~").
    NotName,
    /// Shares a color ("that shares a color with it").
    Color,
}

/// What a spell must target ("that targets a creature", "that targets you
/// or a permanent you control").
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct SpellTargets {
    /// "you" (the player judging).
    pub you: bool,
    /// "a player".
    pub player: bool,
    /// "a creature", "a permanent you control".
    pub filter: Option<ObjFilter>,
}

/// A comparison in a filter.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Cmp {
    AtMost,
    AtLeast,
    Exactly,
}

impl Cmp {
    pub fn holds<T: PartialOrd + Copy>(self, have: T, n: T) -> bool {
        match self {
            Cmp::AtMost => have <= n,
            Cmp::AtLeast => have >= n,
            Cmp::Exactly => have == n,
        }
    }
}

/// How many colors.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ColorCount {
    Colorless,
    Monocolored,
    Multicolored,
}

impl ObjFilter {
    pub fn creatures() -> Self {
        ObjFilter {
            types: TypeSet::of([CardType::Creature]),
            ..Default::default()
        }
    }

    pub fn enchanted() -> Self {
        ObjFilter {
            enchanted: true,
            ..Default::default()
        }
    }

    /// The source itself.
    pub fn this() -> Self {
        ObjFilter {
            relation: Relation::This,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn who(mut self, w: Who) -> Self {
        self.who = Some(w);
        self
    }

    #[must_use]
    pub fn colors(mut self, c: ColorSet) -> Self {
        self.colors = Some(c);
        self
    }

    #[must_use]
    pub fn subtype(mut self, s: Subtype) -> Self {
        self.subtypes.push(s);
        self
    }

    #[must_use]
    pub fn other(mut self) -> Self {
        self.relation = Relation::Other;
        self
    }

    /// The filter with "the chosen color/type" filled in: that color or
    /// that subtype; with nothing chosen, a filter nothing matches.
    #[must_use]
    pub fn with_chosen(&self, chosen: crate::state::Chosen) -> Self {
        let mut f = self.clone();
        f.chosen = false;
        match chosen {
            crate::state::Chosen::Color(c) => f.colors = Some(ColorSet::of([c])),
            crate::state::Chosen::Subtype(s) => f.subtypes.push(s),
            crate::state::Chosen::None | crate::state::Chosen::Votes(_) => {
                f.colors = Some(ColorSet::default());
            }
        }
        f
    }

    /// The filter with its `same` relation filled in from the object it
    /// relates to: that object's definition (`None` for an object with no
    /// name, CR 708.2: nothing has the same name as it) and colors (sharing
    /// a color with a colorless object is impossible, CR 105.2c).
    #[must_use]
    pub fn with_same(&self, def: Option<CardRef>, colors: ColorSet) -> Self {
        let mut f = self.clone();
        let Some(same) = f.same.take() else {
            return f;
        };
        match same.what {
            // No name: a definition no object has.
            SameWhat::Name => f.named = Some((def.unwrap_or(CardRef(u32::MAX)), true)),
            SameWhat::NotName => f.named = def.map(|d| (d, false)),
            SameWhat::Color => f.colors = Some(colors),
        }
        f
    }

    /// Whether an object with characteristics `c` controlled by
    /// `obj_controller` matches, judged for an effect or ability of `source`
    /// controlled by `controller`. The `enchanted` field needs game state and
    /// is the caller's job.
    pub fn matches(
        &self,
        obj: crate::ids::ObjId,
        obj_controller: crate::ids::PlayerId,
        c: &crate::state::Characteristics,
        source: Option<crate::ids::ObjId>,
        controller: Option<crate::ids::PlayerId>,
    ) -> bool {
        self.matches_one(obj, obj_controller, c, source, controller)
            || self
                .or
                .as_ref()
                .is_some_and(|a| a.matches(obj, obj_controller, c, source, controller))
    }

    fn matches_one(
        &self,
        obj: crate::ids::ObjId,
        obj_controller: crate::ids::PlayerId,
        c: &crate::state::Characteristics,
        source: Option<crate::ids::ObjId>,
        controller: Option<crate::ids::PlayerId>,
    ) -> bool {
        // An unfilled "of the chosen type" (`with_chosen`) or "named ~"
        // (`with_same`) matches nothing.
        if self.chosen || self.same.is_some() {
            return false;
        }
        if let Some((def, named)) = self.named
            && (c.def == def) != named
        {
            return false;
        }
        let is_source = source == Some(obj);
        let relation_ok = match self.relation {
            Relation::Any => true,
            Relation::Other => !is_source,
            Relation::This => is_source,
        };
        if !relation_ok {
            return false;
        }
        if let Some(w) = self.who
            && let Some(ctl) = controller
        {
            let ok = match w {
                Who::Any => true,
                Who::You => obj_controller == ctl,
                Who::Opponents => obj_controller != ctl,
            };
            if !ok {
                return false;
            }
        }
        if !c.types.contains_all(self.types) || c.types.intersects(self.not_types) {
            return false;
        }
        if !c.supertypes.contains_all(self.supertypes)
            || c.supertypes.intersects(self.not_supertypes)
        {
            return false;
        }
        if let Some(cs) = self.colors
            && !c.colors.intersects(cs)
        {
            return false;
        }
        if c.colors.intersects(self.not_colors) {
            return false;
        }
        if !c.keywords.contains_all(self.keywords) || c.keywords.intersects(self.not_keywords) {
            return false;
        }
        if c.subtypes.iter().any(|s| self.not_subtypes.contains(s))
            || self.subtypes.iter().any(|s| !c.has_subtype(*s))
            || (c.has(Keyword::Changeling)
                && self.not_subtypes.iter().any(|s| s.is_creature_type()))
        {
            return false;
        }
        if !self.any_types.is_empty() && !c.types.intersects(self.any_types) {
            return false;
        }
        if let Some((cmp, n)) = self.power
            && !cmp.holds(c.power, n)
        {
            return false;
        }
        if let Some((cmp, n)) = self.toughness
            && !cmp.holds(c.toughness, n)
        {
            return false;
        }
        if let Some(cc) = self.color_count {
            let n = c.colors.iter().count();
            let ok = match cc {
                ColorCount::Colorless => n == 0,
                ColorCount::Monocolored => n == 1,
                ColorCount::Multicolored => n >= 2,
            };
            if !ok {
                return false;
            }
        }
        !self.no_abilities || c.has_no_abilities()
    }

    /// Whether characteristics alone match (no relation, controller, or
    /// state parts): protection's qualities (CR 702.16a).
    pub fn matches_characteristics(&self, c: &crate::state::Characteristics) -> bool {
        let mut f = self.clone();
        f.relation = Relation::Any;
        f.who = None;
        f.matches(
            crate::ids::ObjId::default(),
            crate::ids::PlayerId(0),
            c,
            None,
            None,
        )
    }

    /// Whether the filter has conditions on game state beyond
    /// characteristics (see `GameState::status_ok`) or the mana value.
    pub const fn needs_state(&self) -> bool {
        self.or.is_some()
            || self.tapped.is_some()
            || self.attacking
            || self.blocking
            || self.damaged
            || self.mana_value.is_some()
            || self.status.any()
    }
}

/// A power or toughness value set by a layer 7b effect.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum PtValue {
    Fixed(i32),
    /// The object's mana value (CR 202.3).
    ManaValue,
}

/// One part of a continuous effect. Each part applies in exactly one layer.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Modification {
    /// "is a creature in addition to its other types" (layer 4).
    AddTypes(TypeSet),
    /// "is a Swamp in addition to its other land types" (layer 4; CR 305.7
    /// does not apply).
    AddLandType(Color),
    /// "is a Mountain" (layer 4): loses its other land types and, per CR
    /// 305.7, the abilities from its rules text (applied in layer 6).
    SetLandType(Color),
    /// "is an Assembly-Worker" (layer 4): gains a creature type.
    AddSubtype(Subtype),
    /// "becomes a Merfolk" (layer 4): the creature type replaces its
    /// others (CR 205.1b), changeling's included.
    SetCreatureType(Subtype),
    /// "Enchanted land is the chosen type" (layer 4): `SetLandType` of the
    /// basic land type the source chose as it entered (CR 607.2a).
    ChosenLandType,
    /// "is the chosen type in addition to its other types" (layer 4): the
    /// creature or basic land type the source chose as it entered.
    AddChosenType,
    /// "is the chosen color" (layer 5): the color the static ability's
    /// source chose as it entered (CR 607.2a).
    ChosenColor,
    /// "has protection from the chosen color" (layer 6).
    ProtectionFromChosen,
    /// The effect's controller controls the object (layer 2, CR 613.1b).
    /// Applied by `GameState::sync_control`, not through characteristics.
    Control,
    /// "is white in addition to its other colors" (layer 5).
    AddColors(ColorSet),
    /// "is blue" (layer 5).
    SetColors(ColorSet),
    /// "has flying" (layer 6).
    AddKeywords(KeywordSet),
    /// "has '{T}: Add {G}.'": an ability granted (layer 6), registered in
    /// the pool.
    AddAbility(AbilityId),
    /// "loses flying" (layer 6).
    RemoveKeywords(KeywordSet),
    /// "loses all abilities" (layer 6).
    RemoveAllAbilities,
    /// "has base power and toughness 1/1" (layer 7b).
    SetPt { power: PtValue, toughness: PtValue },
    /// "+1/+1" (layer 7c).
    ModifyPt { power: i32, toughness: i32 },
    /// "+1/+1 for each creature you control" (layer 7c), the count judged
    /// from the static ability's controller as the layer applies. Counts
    /// that the characteristics cache does not track (hand, life) are not
    /// allowed here.
    ModifyPtPer {
        power: i32,
        toughness: i32,
        count: CountExpr,
    },
    /// "switch power and toughness" (layer 7d).
    SwitchPt,
    /// A characteristic-defining ability setting colors (CR 604.3;
    /// Transguild Courier): layer 5, applied before other layer 5 effects
    /// (CR 613.2).
    CdaColors(ColorSet),
    /// A characteristic-defining ability setting power and toughness
    /// (CR 604.3, layer 7a): power is the count, toughness the count plus
    /// `plus_toughness`.
    CdaPt {
        count: CdaCount,
        plus_toughness: i32,
    },
}

/// What a power/toughness-defining ability counts.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CdaCount {
    /// Card types among cards in all graveyards (Tarmogoyf).
    CardTypesInGraveyards,
    /// Creature cards in all graveyards (Lhurgoyf).
    CreatureCardsInGraveyards,
    /// Permanents matching a filter, judged from the controller (Nightmare:
    /// "Swamps you control").
    Permanents(ObjFilter),
    /// The controller's life total (Ajani Goldmane's Avatar).
    ControllerLife,
}

impl Modification {
    pub const fn layer(&self) -> Layer {
        match self {
            Modification::Control => Layer::L2Control,
            Modification::AddTypes(_)
            | Modification::AddLandType(_)
            | Modification::SetLandType(_)
            | Modification::AddSubtype(_)
            | Modification::SetCreatureType(_)
            | Modification::ChosenLandType
            | Modification::AddChosenType => Layer::L4Type,
            Modification::AddColors(_)
            | Modification::SetColors(_)
            | Modification::CdaColors(_)
            | Modification::ChosenColor => Layer::L5Color,
            Modification::AddKeywords(_)
            | Modification::ProtectionFromChosen
            | Modification::AddAbility(_)
            | Modification::RemoveKeywords(_)
            | Modification::RemoveAllAbilities => Layer::L6Ability,
            Modification::CdaPt { .. } => Layer::L7aCda,
            Modification::SetPt { .. } => Layer::L7bSetPt,
            Modification::ModifyPt { .. } | Modification::ModifyPtPer { .. } => Layer::L7cModifyPt,
            Modification::SwitchPt => Layer::L7dSwitchPt,
        }
    }

    /// Whether this is a characteristic-defining ability's effect (CR
    /// 604.3): it applies before other effects in its layer (CR 613.2).
    pub const fn is_cda(&self) -> bool {
        matches!(
            self,
            Modification::CdaColors(_) | Modification::CdaPt { .. }
        )
    }
}

/// What a variable amount counts, judged from the effect's controller.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CountExpr {
    /// Battlefield permanents matching the filter ("creatures you
    /// control", "Elves on the battlefield").
    Permanents(ObjFilter),
    /// Basic land types among lands you control (domain).
    Domain,
    /// Cards in your hand.
    CardsInHand,
    /// Cards in your graveyard, all or matching the filter.
    CardsInGraveyard(Option<ObjFilter>),
    /// The source's power ("equal to its power" on a creature's ability).
    SourcePower,
    /// The `n`th target's power, or toughness (last known information
    /// once it has left: CR 608.2h).
    TargetStat { n: usize, toughness: bool },
    /// Your life total.
    Life,
    /// N times another count ("2 life for each creature you control").
    Times(u32, Box<CountExpr>),
    /// Cards in all graveyards, all or matching the filter ("the number of
    /// cards named ~ in all graveyards").
    CardsInAllGraveyards(Option<ObjFilter>),
    /// N plus another count ("2 plus the number of cards named ~ in your
    /// graveyard").
    Plus(u32, Box<CountExpr>),
    /// Objects the resolution moved `verb`'s way that match the filter
    /// ("for each enchantment destroyed this way", "draw that many
    /// cards" after a discard), judged as they last existed.
    ThisWay(ThisWay, Option<ObjFilter>),
    /// Life the resolution made players lose ("You gain life equal to the
    /// life lost this way").
    LifeLostThisWay,
    /// Creatures blocking the triggering object, or else the source ("for
    /// each creature blocking it"), less `beyond` ("beyond the first").
    BlockingIt { beyond: u32 },
    /// The `n`th target's mana value ("where X is its mana value"), last
    /// known information once it has left (CR 608.2h).
    TargetManaValue(usize),
    /// The greatest power among battlefield permanents matching the
    /// filter ("the greatest power among creatures you control"); 0 with
    /// none.
    GreatestPower(ObjFilter),
    /// The number of creatures in your party (CR 700.8): up to one each
    /// matching these four filters (Cleric, Rogue, Warrior, Wizard
    /// creatures you control), as many as can be assigned.
    Party(Box<[ObjFilter; 4]>),
    /// The greatest mana value among permanents you control.
    GreatestManaValue,
    /// Cards you've drawn this turn.
    CardsDrawnThisTurn,
    /// The source's toughness ("where X is its toughness").
    SourceToughness,
    /// Colors among permanents you control (vivid).
    ColorsAmongPermanents,
    /// Cards in your opponents' graveyards, all or matching the filter.
    CardsInOpponentsGraveyards(Option<ObjFilter>),
    /// Opponents you're attacking: those a creature you control is
    /// attacking (CR 506.2).
    OpponentsAttacked,
    /// Creatures you attacked with this turn (declared as attackers, CR
    /// 508.1).
    AttackedWithThisTurn,
    /// Converge: the number of colors of mana spent to cast the source
    /// (CR 601.2h).
    SpentColors,
    /// The mana from Treasures spent to cast the source ("for each mana
    /// from a Treasure spent to cast it").
    SpentTreasure,
    /// The votes the resolution's vote (`Effect::Vote`) gave its `n`th
    /// choice ("for each sprout vote").
    Votes(u8),
}

/// Where a spell countered this way goes (CR 701.5a: its owner's
/// graveyard unless the effect says otherwise).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum CounteredTo {
    #[default]
    Graveyard,
    /// "exile it instead of putting it into its owner's graveyard".
    Exile,
    /// "put it on top of its owner's library instead of into that
    /// player's graveyard".
    LibraryTop,
}

/// Where in its owner's library an object is put.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum LibraryPlace {
    Top,
    Bottom,
    /// "second from the top" is `FromTop(2)` (CR 401.7 for a library
    /// with fewer cards: it goes on the bottom).
    FromTop(u8),
    /// "on their choice of the top or bottom of their library": the
    /// owner chooses.
    TopOrBottom,
    /// "into their library second from the top or on the bottom": the
    /// owner chooses.
    FromTopOrBottom(u8),
    /// "shuffles it into their library" (CR 701.24).
    Shuffle,
}

/// A card's physical layout (CR 709 split, 712 double-faced, 715
/// adventurer). `Omen` is an adventurer whose spell is shuffled into its
/// owner's library as it resolves instead of exiled.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Layout {
    #[default]
    Normal,
    Adventure,
    Omen,
    Transform,
    Split,
}

/// A restriction on how this creature blocks or is blocked.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum BlockRule {
    /// "can't be blocked by Walls".
    NotBy(ObjFilter),
    /// "can't be blocked except by creatures with flying".
    OnlyBy(ObjFilter),
    /// "can't be blocked by more than one creature".
    AtMostOne,
    /// "can block only creatures with flying".
    BlocksOnly(ObjFilter),
    /// "can't block creatures with flying".
    CantBlock(ObjFilter),
}

/// Where searched-for cards go.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum SearchTo {
    Hand,
    Battlefield {
        tapped: bool,
    },
    /// "put that card on top" (after shuffling).
    TopOfLibrary,
}

/// A static ability (CR 604) that generates a continuous effect while its
/// source is on the battlefield (CR 611.3).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct StaticAbility {
    pub affected: ObjFilter,
    pub parts: Vec<Modification>,
    /// "As long as ..." (CR 611.3a): the effect exists only while this
    /// holds, judged from the source's controller.
    pub condition: Option<StaticCondition>,
}

/// What a conditional static ability checks (threshold, metalcraft,
/// delirium, "during your turn", "as long as you control an Island").
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum StaticCondition {
    /// "seven or more cards are in your graveyard" (threshold).
    CardsInGraveyard(u32),
    /// "there are four or more card types among cards in your graveyard"
    /// (delirium).
    CardTypesInGraveyard(u32),
    /// "you control three or more artifacts" (metalcraft); `at_least` 1 is
    /// "you control an Island".
    YouControl {
        filter: ObjFilter,
        at_least: u32,
    },
    /// "it's your turn" ("during your turn"). The turn change bumps the
    /// characteristics cache.
    YourTurn,
    /// "as long as it's attacking" (the source).
    SourceAttacking,
    /// "as long as it's enchanted" / "as long as it's equipped".
    SourceEnchanted,
    SourceEquipped,
    /// "as long as an opponent has 10 or less life"; "as long as you have
    /// 30 or more life" (`at_least`), "you have 5 or less life".
    OpponentLifeAtMost(i32),
    YourLife {
        n: i32,
        at_least: bool,
    },
    /// "a player has 13 or less life".
    AnyPlayerLifeAtMost(i32),
    /// "an opponent has three or more poison counters" (corrupted).
    OpponentPoisonAtLeast(u32),
    /// "this creature has four or more oil counters on it" (`at_least`);
    /// "there are no arrowhead counters on this artifact" (`n` 0).
    SourceCounters {
        kind: CounterKind,
        n: u32,
        at_least: bool,
    },
    /// "defending player controls an Island" (the opponent, judged as
    /// attackers are declared).
    DefenderControls {
        filter: ObjFilter,
        at_least: u32,
    },
    /// "there is a Mountain on the battlefield", "there are five or more
    /// Islands on the battlefield": permanents of any controller.
    OnBattlefield {
        filter: ObjFilter,
        at_least: u32,
    },
    /// "defending player is poisoned" (an opponent, judged as attackers
    /// are declared).
    DefenderPoisoned,
    /// "at least two other creatures attack" (`at_least` 2), "a black or
    /// green creature also attacks" (`also`), "... block" (`block`): of
    /// "can't attack unless" / "can't block unless", judged on the whole
    /// declaration (CR 508.1c, 509.1c).
    OthersInCombat {
        filter: ObjFilter,
        at_least: u32,
        also: bool,
        block: bool,
    },
    /// "you control more creatures than defending player" (`attacking`
    /// false) / "than attacking player": than an opponent.
    MoreCreaturesThanOpponent {
        attacking: bool,
    },
    /// "you control no untapped lands": fewer than one.
    YouControlNone(ObjFilter),
    /// Raid: "you attacked this turn".
    AttackedThisTurn,
    /// The entering object was cast with escape ("escapes with").
    Escaped,
    /// Bloodthirst: "an opponent was dealt damage this turn".
    OpponentDamagedThisTurn,
    /// Morbid: "a creature died this turn".
    CreatureDiedThisTurn,
    /// Hellbent-style: "you have N or fewer cards in hand" (`at_most`) /
    /// "N or more".
    CardsInHand {
        n: u32,
        at_most: bool,
    },
    /// The entering or resolving object was kicked (CR 702.33c): "If this
    /// creature was kicked, it enters with ..." and the abilities it then
    /// has.
    Kicked,
    /// The spell or permanent was bargained (CR 702.166b): "If this spell
    /// was bargained", "if it was bargained". A card in hand counts as
    /// bargained if it could be ("This spell costs {2} less to cast if
    /// it's bargained", judged as the spell is offered).
    Bargained,
    /// Its offspring cost was paid (CR 702.175a).
    OffspringPaid,
    /// The source is saddled (CR 702.171b): "whenever this creature
    /// attacks while saddled".
    Saddled,
    /// Landfall: "you had a land enter the battlefield under your control
    /// this turn".
    LandEnteredThisTurn,
    /// "an artifact entered the battlefield under your control this turn".
    ArtifactEnteredThisTurn,
    /// "you've drawn two or more cards this turn".
    CardsDrawnThisTurn(u32),
    /// "there are two or more instant and/or sorcery cards in your
    /// graveyard" (spell mastery), "three or more creature cards".
    GraveyardCards {
        filter: ObjFilter,
        at_least: u32,
    },
    /// "an opponent has eight or more cards in their graveyard".
    OpponentGraveyard(u32),
    /// "there are five or more mana values among cards in your graveyard".
    ManaValuesInGraveyard(u32),
    /// "an opponent controls a Human" (judged now, unlike
    /// `DefenderControls`).
    OpponentControls {
        filter: ObjFilter,
        at_least: u32,
    },
    /// "it's untapped", "it's a Wall", "this permanent is a creature": the
    /// source matches (characteristics and status).
    SourceIs(ObjFilter),
    /// "it's modified" (CR 700.9): it has a counter, is equipped, or is
    /// enchanted by an Aura its controller controls.
    SourceModified,
    /// "it's attacking alone" (CR 506.5).
    SourceAttackingAlone,
    /// "this creature is blocked" (CR 509.1h).
    SourceBlocked,
    /// "at least one creature is blocking this creature": blocked, by a
    /// creature still blocking it.
    SourceBeingBlocked,
    /// "equipped creature is legendary": the permanent the source is
    /// attached to matches.
    AttachedIs(ObjFilter),
    /// "you've drawn two or more cards this turn".
    DrawnThisTurn(u32),
    /// "you've cast an instant or sorcery spell this turn", judged on the
    /// spells' characteristics as they were cast.
    CastThisTurn(ObjFilter),
    /// "you've discarded a card this turn".
    DiscardedThisTurn,
    /// "you have more cards in hand than each opponent".
    MoreCardsInHand,
    /// "an opponent is poisoned" (1), "an opponent has three or more poison
    /// counters".
    OpponentPoison(u32),
    /// "you own a card in exile".
    OwnCardInExile,
    /// Formidable: "creatures you control have total power 8 or greater".
    TotalPower(u32),
    /// Coven: "you control three or more creatures with different powers".
    DifferentPowers(u32),
    /// A part of the turn (CR 602.5b timing restrictions): "during your
    /// upkeep", "before attackers are declared".
    During(TurnTime),
    /// "If {B} was spent to cast this spell", "if at least three red mana
    /// was spent to cast it" (adamant), "if mana from a Treasure was spent
    /// to cast it": at least `at_least` of the mana paid to cast the
    /// source (CR 601.2h) was of that kind. A spell not cast (a copy, a
    /// permanent put onto the battlefield) had no mana spent on it. With
    /// `that`, the spell is the one that triggered the ability ("Whenever
    /// you cast a noncreature spell, if at least four mana was spent to
    /// cast it"; "If five or more mana was spent to cast that spell").
    ManaSpent {
        kind: SpentKind,
        at_least: u8,
        that: bool,
    },
}

/// What a "mana spent to cast" condition or count looks at (CR 601.2h).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum SpentKind {
    /// Mana of this color.
    Color(Color),
    /// Mana a Treasure made (CR 111.10a): its mana ability was activated
    /// while the spell was being paid for.
    Treasure,
    /// Any mana.
    Total,
}

/// What a definition's own text asks about the mana spent to cast it
/// (filled in by `CardPool::add`): whether casting it records what was
/// spent, and what the canonical payment then prefers to spend, so the
/// rider happens when the player can afford it. All zero for ordinary
/// cards, whose casts record nothing.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct SpentUse {
    /// Mana of each color (`Color as usize`) a condition wants.
    pub colors: [u8; 5],
    /// Treasures to spend first: 0, 1 ("if mana from a Treasure was
    /// spent"), or `u8::MAX` ("for each mana from a Treasure spent").
    pub treasure: u8,
    /// Converge: as many colors as the payment can include.
    pub converge: bool,
    /// Anything is read, so the cast records what was spent.
    pub record: bool,
}

/// A part of the turn an activation restriction names (CR 602.5b).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum TurnTime {
    /// "during your upkeep".
    YourUpkeep,
    /// "during an opponent's upkeep".
    OpponentUpkeep,
    /// "during combat" (CR 506.1: the combat phase's steps).
    Combat,
    /// "before attackers are declared": before the declare attackers step
    /// (of the turn's first combat).
    BeforeAttackers,
    /// "during your turn, before attackers are declared".
    YourTurnBeforeAttackers,
    /// "during the declare blockers step".
    DeclareBlockers,
    /// "during the end of combat step".
    EndOfCombat,
}

impl TurnTime {
    /// Whether it is that part of the turn, for `you`.
    pub fn now(
        self,
        step: crate::turn::Step,
        active: crate::ids::PlayerId,
        you: crate::ids::PlayerId,
    ) -> bool {
        use crate::turn::Step;
        let before = matches!(
            step,
            Step::Untap | Step::Upkeep | Step::Draw | Step::PrecombatMain | Step::BeginningOfCombat
        );
        match self {
            TurnTime::YourUpkeep => step == Step::Upkeep && active == you,
            TurnTime::OpponentUpkeep => step == Step::Upkeep && active != you,
            TurnTime::Combat => matches!(
                step,
                Step::BeginningOfCombat
                    | Step::DeclareAttackers
                    | Step::DeclareBlockers
                    | Step::FirstStrikeDamage
                    | Step::CombatDamage
                    | Step::EndOfCombat
            ),
            TurnTime::BeforeAttackers => before,
            TurnTime::YourTurnBeforeAttackers => before && active == you,
            TurnTime::DeclareBlockers => step == Step::DeclareBlockers,
            TurnTime::EndOfCombat => step == Step::EndOfCombat,
        }
    }
}

/// Kinds of counters (CR 122).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CounterKind {
    PlusOnePlusOne,
    MinusOneMinusOne,
    /// Loyalty counters on a planeswalker (CR 306.5).
    Loyalty,
    /// CR 122.1d: as the permanent would untap, a stun counter is removed
    /// instead.
    Stun,
    /// Named counters with no rules of their own (CR 122.1): what cards
    /// count and remove.
    Charge,
    Oil,
    Spore,
    Depletion,
    Quest,
    Ki,
    Time,
    Tide,
    Shield,
    Age,
    Fade,
    Ice,
    Level,
    Lore,
    Page,
    Arrowhead,
    Javelin,
    Polyp,
    Healing,
    /// Power/toughness counters other than +1/+1 and -1/-1 (CR 122.1a):
    /// each modifies power and toughness in layer 7c, and none annihilates
    /// with another (CR 704.5q is about +1/+1 and -1/-1 only).
    PlusOnePlusZero,
    PlusZeroPlusOne,
    PlusZeroPlusTwo,
    PlusOnePlusTwo,
    PlusTwoPlusTwo,
    MinusZeroMinusOne,
    MinusZeroMinusTwo,
    MinusOneMinusZero,
    MinusTwoMinusOne,
    MinusTwoMinusTwo,
    /// Keyword counters (CR 122.1b): the permanent has the keyword.
    Flying,
    FirstStrike,
    Trample,
    Lifelink,
    Deathtouch,
    Reach,
    Menace,
    Vigilance,
    Hexproof,
    Indestructible,
}

impl Keyword {
    /// Keywords that add payment helpers while casting (convoke,
    /// improvise, delve).
    pub fn helps_pay(set: KeywordSet) -> bool {
        set.contains(Keyword::Convoke)
            || set.contains(Keyword::Improvise)
            || set.contains(Keyword::Delve)
    }
}

impl CounterKind {
    /// Named kinds by their Oracle word ("stun", "first strike").
    pub const NAMED: [(&'static str, CounterKind); 30] = [
        ("stun", CounterKind::Stun),
        ("charge", CounterKind::Charge),
        ("oil", CounterKind::Oil),
        ("spore", CounterKind::Spore),
        ("depletion", CounterKind::Depletion),
        ("quest", CounterKind::Quest),
        ("ki", CounterKind::Ki),
        ("time", CounterKind::Time),
        ("tide", CounterKind::Tide),
        ("shield", CounterKind::Shield),
        ("age", CounterKind::Age),
        ("fade", CounterKind::Fade),
        ("ice", CounterKind::Ice),
        ("level", CounterKind::Level),
        ("lore", CounterKind::Lore),
        ("page", CounterKind::Page),
        ("arrowhead", CounterKind::Arrowhead),
        ("javelin", CounterKind::Javelin),
        ("polyp", CounterKind::Polyp),
        ("healing", CounterKind::Healing),
        ("flying", CounterKind::Flying),
        ("first strike", CounterKind::FirstStrike),
        ("trample", CounterKind::Trample),
        ("lifelink", CounterKind::Lifelink),
        ("deathtouch", CounterKind::Deathtouch),
        ("reach", CounterKind::Reach),
        ("menace", CounterKind::Menace),
        ("vigilance", CounterKind::Vigilance),
        ("hexproof", CounterKind::Hexproof),
        ("indestructible", CounterKind::Indestructible),
    ];

    /// Power/toughness counters by their Oracle name, with what each adds
    /// to power and toughness (CR 122.1a). "-0/-1" and "+0/+1" differ in
    /// sign only where the change is not zero, so the pair of numbers
    /// identifies the kind.
    pub const PT: [(&'static str, CounterKind, i32, i32); 12] = [
        ("+1/+1", CounterKind::PlusOnePlusOne, 1, 1),
        ("-1/-1", CounterKind::MinusOneMinusOne, -1, -1),
        ("+1/+0", CounterKind::PlusOnePlusZero, 1, 0),
        ("+0/+1", CounterKind::PlusZeroPlusOne, 0, 1),
        ("+0/+2", CounterKind::PlusZeroPlusTwo, 0, 2),
        ("+1/+2", CounterKind::PlusOnePlusTwo, 1, 2),
        ("+2/+2", CounterKind::PlusTwoPlusTwo, 2, 2),
        ("-0/-1", CounterKind::MinusZeroMinusOne, 0, -1),
        ("-0/-2", CounterKind::MinusZeroMinusTwo, 0, -2),
        ("-1/-0", CounterKind::MinusOneMinusZero, -1, 0),
        ("-2/-1", CounterKind::MinusTwoMinusOne, -2, -1),
        ("-2/-2", CounterKind::MinusTwoMinusTwo, -2, -2),
    ];

    /// The power/toughness counter with this change, if any.
    pub fn from_pt(power: i32, toughness: i32) -> Option<CounterKind> {
        CounterKind::PT
            .iter()
            .find(|(_, _, p, t)| *p == power && *t == toughness)
            .map(|(_, k, _, _)| *k)
    }

    /// What one counter of this kind adds to power and toughness (CR
    /// 613.4c), if it is a power/toughness counter.
    pub const fn pt(self) -> Option<(i32, i32)> {
        Some(match self {
            CounterKind::PlusOnePlusOne => (1, 1),
            CounterKind::MinusOneMinusOne => (-1, -1),
            CounterKind::PlusOnePlusZero => (1, 0),
            CounterKind::PlusZeroPlusOne => (0, 1),
            CounterKind::PlusZeroPlusTwo => (0, 2),
            CounterKind::PlusOnePlusTwo => (1, 2),
            CounterKind::PlusTwoPlusTwo => (2, 2),
            CounterKind::MinusZeroMinusOne => (0, -1),
            CounterKind::MinusZeroMinusTwo => (0, -2),
            CounterKind::MinusOneMinusZero => (-1, 0),
            CounterKind::MinusTwoMinusOne => (-2, -1),
            CounterKind::MinusTwoMinusTwo => (-2, -2),
            _ => return None,
        })
    }

    /// The Oracle word ("+1/+1", "stun").
    pub fn name(self) -> &'static str {
        if self == CounterKind::Loyalty {
            return "loyalty";
        }
        if let Some((n, ..)) = CounterKind::PT.iter().find(|(_, k, ..)| *k == self) {
            return n;
        }
        CounterKind::NAMED
            .iter()
            .find(|(_, k)| *k == self)
            .map_or("?", |(n, _)| n)
    }

    /// A kind by its Oracle word ("+1/+0", "loyalty", "first strike").
    pub fn by_name(name: &str) -> Option<CounterKind> {
        if name == "loyalty" {
            return Some(CounterKind::Loyalty);
        }
        CounterKind::PT
            .iter()
            .find(|(n, ..)| *n == name)
            .map(|(_, k, ..)| *k)
            .or_else(|| {
                CounterKind::NAMED
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, k)| *k)
            })
    }

    /// The keyword a keyword counter grants (CR 122.1b).
    pub const fn keyword(self) -> Option<Keyword> {
        Some(match self {
            CounterKind::Flying => Keyword::Flying,
            CounterKind::FirstStrike => Keyword::FirstStrike,
            CounterKind::Trample => Keyword::Trample,
            CounterKind::Lifelink => Keyword::Lifelink,
            CounterKind::Deathtouch => Keyword::Deathtouch,
            CounterKind::Reach => Keyword::Reach,
            CounterKind::Menace => Keyword::Menace,
            CounterKind::Vigilance => Keyword::Vigilance,
            CounterKind::Hexproof => Keyword::Hexproof,
            CounterKind::Indestructible => Keyword::Indestructible,
            _ => return None,
        })
    }
}

/// What a spell's target must be. One entry per "target" word in the text.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum TargetSpec {
    /// "any target": a creature, player, planeswalker, or battle (CR 115.4).
    Any,
    Creature,
    /// "target player or planeswalker".
    PlayerOrPlaneswalker,
    /// "target creature or planeswalker".
    CreatureOrPlaneswalker,
    /// "target land".
    Land,
    Player,
    /// "target tapped creature".
    TappedCreature,
    /// "target creature you control" (equip, CR 702.6a).
    CreatureYouControl,
    /// "target artifact, creature, or land".
    ArtifactCreatureOrLand,
    /// "target artifact or enchantment".
    ArtifactOrEnchantment,
    /// "target artifact".
    Artifact,
    /// "target enchantment".
    Enchantment,
    /// "target permanent".
    Permanent,
    /// "another target creature": a creature other than the ability's
    /// source.
    OtherCreature,
    /// "another target permanent".
    OtherPermanent,
    /// "target nonlegendary creature you control" (Kiki-Jiki).
    NonlegendaryCreatureYouControl,
    /// "target creature card in your graveyard" (Gravedigger).
    CreatureCardInYourGraveyard,
    /// "target Assembly-Worker creature" (Mishra's Factory): a creature with
    /// this subtype.
    CreatureWithSubtype(Subtype),
    /// "target spell": a spell on the stack (CR 115.1).
    Spell,
    /// "target creature spell".
    CreatureSpell,
    /// "target noncreature spell".
    NoncreatureSpell,
    /// An object on the stack (CR 115.1): "target spell or ability",
    /// "target activated or triggered ability", "target activated ability
    /// from an artifact source" (`source`: the ability's source has that
    /// card type, as it last existed if it has left). Mana abilities never
    /// use the stack, so are never targeted (CR 605.3b).
    OnStack {
        spells: bool,
        activated: bool,
        triggered: bool,
        source: Option<CardType>,
    },
    /// "target opponent".
    Opponent,
    /// "target opponent or planeswalker".
    OpponentOrPlaneswalker,
    /// "target artifact or creature".
    ArtifactOrCreature,
    /// "target attacking or blocking creature".
    AttackingOrBlockingCreature,
    /// "target attacking creature".
    AttackingCreature,
    /// "target blocking creature".
    BlockingCreature,
    /// A battlefield object matching the pool's `n`th interned filter
    /// ("target nonblack creature", "target creature an opponent controls").
    Filtered(u16),
    /// A spell matching the filter ("target instant spell", "target
    /// artifact or enchantment spell"); judged on its characteristics.
    SpellFiltered(u16),
    /// A card in a graveyard matching the filter ("target card from a
    /// graveyard", "target instant card from your graveyard"); `who` is
    /// whose graveyard, judged from the caster.
    GraveyardCard {
        filter: u16,
        who: Who,
    },
    /// "up to two target creatures" (CR 601.2c, 115.1d): the pool's `n`th
    /// optional spec (`CardPool::up_to`). The caster may stop choosing at
    /// an optional target; the optional targets come last.
    UpTo(u16),
    /// "X target creatures", "one or two target creatures", "any number of
    /// target cards" (CR 601.2c): the pool's `n`th variable group
    /// (`CardPool::many`). One spec for the whole group, which is an
    /// ability's last; its targets fill the rest of the target list.
    Many(u16),
}

/// A variable number of targets of one kind (CR 601.2c): at least `min`,
/// at most `max` (any number if `None`); X is the spell's X.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ManyTargets {
    pub spec: TargetSpec,
    pub min: Num,
    pub max: Option<Num>,
}

/// The spec the `i`th chosen target answers: a variable group's spec
/// (`TargetSpec::Many`, always last) repeats to the end of the list.
pub fn spec_for(specs: &[TargetSpec], i: usize) -> Option<TargetSpec> {
    specs.get(i).copied().or_else(|| {
        specs
            .last()
            .copied()
            .filter(|s| matches!(s, TargetSpec::Many(_)))
    })
}

impl TargetSpec {
    /// Targets on the stack.
    pub const fn is_spell(self) -> bool {
        matches!(
            self,
            TargetSpec::Spell
                | TargetSpec::CreatureSpell
                | TargetSpec::NoncreatureSpell
                | TargetSpec::SpellFiltered(_)
                | TargetSpec::OnStack { .. }
        )
    }

    /// Targets in graveyards.
    pub const fn is_graveyard(self) -> bool {
        matches!(
            self,
            TargetSpec::CreatureCardInYourGraveyard | TargetSpec::GraveyardCard { .. }
        )
    }
}

/// Who or what an effect acts on.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Recipient {
    /// The controller of the `n`th target ("its controller"), as it last
    /// existed if it has left its zone.
    ControllerOf(usize),
    /// The owner of the `n`th target ("its owner").
    OwnerOf(usize),
    /// The `n`th chosen target.
    Target(usize),
    /// Chosen targets `first..first + count` together ("up to two target
    /// creatures each get +1/+0").
    Targets(u8, u8),
    /// The spell's or ability's controller ("you").
    You,
    /// Every opponent of the controller.
    EachOpponent,
    /// The ability's source ("this creature", "~"); for an ability of an
    /// object that changed zones, the object it became (CR 400.7).
    Source,
    /// The object whose action triggered the ability ("that creature").
    TriggeringObject,
    /// The card the triggering object became by the zone change that
    /// triggered the ability ("When enchanted creature dies, return that
    /// card to its owner's hand"): only that new object, while it exists
    /// (CR 400.7).
    TriggeringCard,
    /// The controller of the triggering object, as it last existed on the
    /// battlefield if it has left ("When enchanted creature dies, its
    /// controller draws a card", "that creature's controller").
    TriggeringController,
    /// The player whose action triggered the ability ("that player", or the
    /// active player for "at the beginning of each player's ..." triggers).
    TriggeringPlayer,
    /// The player the triggering creature is attacking.
    DefendingPlayer,
    /// Every creature on the battlefield.
    EachCreature,
    /// Every player in the game.
    EachPlayer,
    /// Every creature the controller controls.
    EachCreatureYouControl,
    /// Every permanent the controller controls.
    EachPermanentYouControl,
    /// The object a delayed trigger is about, only if it is still the same
    /// object (CR 400.7): "sacrifice it at the beginning of the next end
    /// step" does nothing once it has changed zones.
    SameObject,
    /// The object the previous effect of the same resolution created or
    /// moved ("that token", "that card").
    CreatedObject,
    /// Every battlefield object matching the pool's `n`th interned filter
    /// ("all artifacts", "each creature with flying", "white creatures you
    /// control").
    Group(u16),
    /// One battlefield object matching the pool's `n`th interned filter,
    /// picked at random as the effect happens ("a creature an opponent
    /// controls chosen at random"), from the game's random number
    /// generator.
    RandomOf(u16),
    /// What the source (an Aura or Equipment) is attached to ("enchanted
    /// creature", "equipped creature").
    Attached,
    /// Each creature blocking the triggering object, or else the source
    /// ("each creature blocking it").
    BlockingIt,
    /// The `n`th target and every other battlefield object matching the
    /// pool's `g`th filter, whose `same` relation is to that target
    /// ("target creature and all other creatures with the same name as
    /// that creature", "target creature and each other creature that
    /// shares a color with it").
    TargetAndGroup(usize, u16),
}

/// A number in an effect's text: a constant, or X as chosen when the spell
/// was cast (CR 107.3).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Num {
    N(u32),
    X,
    /// The `i`th of the source card's `CardDef::counts` ("the number of
    /// creatures you control"), evaluated as the effect happens
    /// (`GameState::eval_num`).
    Count(u8),
    /// "that much": the triggering event's amount (damage dealt).
    ThatMuch,
}

impl Num {
    /// The value without game state: a count is 0 here (see
    /// `GameState::eval_num`).
    pub const fn eval(self, x: u32) -> u32 {
        match self {
            Num::N(n) => n,
            Num::X => x,
            Num::Count(_) | Num::ThatMuch => 0,
        }
    }

    /// A value for heuristics that have no X in hand.
    pub const fn hint(self) -> u32 {
        match self {
            Num::N(n) => n,
            Num::X | Num::Count(_) | Num::ThatMuch => 3,
        }
    }
}

impl From<u32> for Num {
    fn from(n: u32) -> Self {
        Num::N(n)
    }
}

/// Effects of spells and abilities.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Effect {
    DealDamage {
        amount: Num,
        to: Recipient,
    },
    /// +N/+N until end of turn (CR 611.2a; layer 7c).
    Pump {
        power: i32,
        toughness: i32,
        to: Recipient,
    },
    /// "+1/+1 for each creature you control", "+X/+0": `power` and
    /// `toughness` times `times`, until end of turn.
    PumpVar {
        power: i32,
        toughness: i32,
        times: Num,
        to: Recipient,
    },
    Destroy {
        to: Recipient,
    },
    GainLife {
        amount: Num,
        who: Recipient,
    },
    LoseLife {
        amount: Num,
        who: Recipient,
    },
    DrawCards {
        count: Num,
        who: Recipient,
    },
    /// Put counters on a permanent (CR 122.6).
    AddCounters {
        kind: CounterKind,
        count: Num,
        to: Recipient,
    },
    /// "deals 3 damage divided as you choose among one, two, or three
    /// targets" (CR 601.2d): `to` is a variable target group
    /// (`Recipient::Targets`); each target is dealt what the caster
    /// assigned it when casting.
    DealDamageDivided {
        amount: Num,
        to: Recipient,
    },
    /// "Distribute two +1/+1 counters among one or two target creatures"
    /// (CR 601.2d), divided like `DealDamageDivided`.
    DistributeCounters {
        kind: CounterKind,
        count: Num,
        to: Recipient,
    },
    /// Return a card to its owner's hand (from the battlefield or a
    /// graveyard).
    ReturnToHand {
        what: Recipient,
    },
    /// Remove counters from a permanent.
    RemoveCounters {
        kind: CounterKind,
        count: Num,
        to: Recipient,
    },
    /// Remove all counters from the recipient (Vampire Hexmage).
    RemoveAllCounters {
        to: Recipient,
    },
    /// Give the recipient a regeneration shield until end of turn (CR
    /// 701.15).
    Regenerate {
        to: Recipient,
    },
    /// Destroy; "it can't be regenerated" (CR 701.7c).
    DestroyNoRegen {
        to: Recipient,
    },
    /// The recipient player puts the top `count` cards of their library into
    /// their graveyard (CR 701.13).
    Mill {
        count: Num,
        who: Recipient,
    },
    /// The controller gets an emblem (CR 114) with this definition's
    /// abilities, in the command zone.
    CreateEmblem {
        emblem: CardRef,
    },
    /// Counter a spell or ability (CR 701.5): a spell goes to its owner's
    /// graveyard, or where `dest` says ("If that spell is countered this
    /// way, exile it instead of putting it into its owner's graveyard"); an
    /// ability just leaves the stack (CR 701.5a).
    Counter {
        to: Recipient,
        dest: CounteredTo,
    },
    /// "Counter target spell unless its controller pays {N}" (CR 701.5?):
    /// the controller may pay; if they do not, it is countered. An {X} in
    /// `mana` is `x` as the effect resolves ("unless its controller pays
    /// {1} for each card in your hand", "{X}, where X is its mana value").
    CounterUnlessPays {
        to: Recipient,
        mana: ManaCost,
        x: Num,
        dest: CounteredTo,
    },
    /// Scry N (CR 701.18): the controller puts any of the top N cards of
    /// their library on the bottom, the rest back on top.
    Scry {
        count: Num,
    },
    /// Surveil N (CR 701.42): like scry, into the graveyard instead.
    Surveil {
        count: Num,
    },
    /// "Search your library for a basic land card, reveal it, put it into
    /// your hand, then shuffle" (CR 701.19): up to `count` cards matching
    /// `filter` (judged on their printed characteristics); the searcher
    /// may fail to find (CR 701.19b).
    Search {
        filter: ObjFilter,
        count: Num,
        to: SearchTo,
    },
    /// "Put target creature on top of its owner's library", "on the
    /// bottom", "second from the top", "Its owner puts it on their choice
    /// of the top or bottom of their library", "shuffles it into their
    /// library" (the owner makes any choice).
    PutInLibrary {
        what: Recipient,
        place: LibraryPlace,
    },
    /// "Each opponent sacrifices a creature of their choice" (an edict):
    /// each player chooses `count` permanents matching the filter.
    SacrificeChosen {
        who: Recipient,
        filter: ObjFilter,
        count: Num,
    },
    /// "Return a land you control to its owner's hand": the recipient
    /// player chooses `count` of their permanents matching the filter.
    ReturnChosen {
        who: Recipient,
        filter: ObjFilter,
        count: Num,
    },
    /// "Target creature loses flying until end of turn".
    Strip {
        keywords: KeywordSet,
        to: Recipient,
    },
    /// "Target player reveals their hand" / "Look at target player's
    /// hand": information only.
    RevealHand {
        who: Recipient,
    },
    /// "reveals a card at random from their hand": one card of each
    /// player's hand, picked at random, is revealed.
    RevealRandom {
        who: Recipient,
    },
    /// "discards a card at random".
    DiscardRandom {
        count: Num,
        who: Recipient,
    },
    /// "Roll a d20." (CR 706): the controller rolls a `sides`-sided die,
    /// then the row of the results table whose range holds the result
    /// happens (CR 706.3a), if any. The roll draws on the game's random
    /// number generator like a shuffle, so a game replays from its seed.
    RollDie {
        sides: u8,
        rows: Vec<DieRow>,
    },
    /// "Starting with you, each player votes for sprout or harvest." (CR
    /// 701.38a): each player in turn order from the controller votes for
    /// one of `options` choices, words with no rules meaning (CR
    /// 701.38b). The tally is the resolution's (`CountExpr::Votes`: "for
    /// each sprout vote").
    Vote {
        options: u8,
    },
    /// "Starting with you, each player votes for an artifact, creature, or
    /// enchantment card in your graveyard. Return each card with the most
    /// votes or tied for most votes to your hand." (CR 701.38): the votes
    /// are for cards in the controller's graveyard matching `filter`.
    VoteReturn {
        filter: ObjFilter,
    },
    /// "Flip a coin. If you win the flip, ..." (CR 705): the controller
    /// calls it and wins half the time (CR 705.2), drawing on the game's
    /// random number generator; `won` happens if they win.
    FlipCoin {
        won: Vec<Effect>,
    },
    /// "You choose a nonland card from it. That player discards that
    /// card.": the effect's controller picks from the player's hand.
    DiscardChosen {
        who: Recipient,
        filter: Option<ObjFilter>,
        /// "You may choose a nonland card from it": the controller may
        /// choose none.
        optional: bool,
    },
    /// "Transform this creature" (CR 701.27): a double-faced permanent
    /// turns to its other face; it stays the same object.
    Transform {
        what: Recipient,
    },
    /// "Add {B}{B}" (CR 106.4): mana into the controller's pool; `Any` is
    /// one mana of a color the controller chooses. An activated ability
    /// whose only effects add mana and that has no targets is a mana
    /// ability (CR 605.1a) and resolves on activation.
    AddMana(Vec<crate::mana::Mana>),
    /// "Add {R} or {W}", "Add {B}, {G}, or {U}": one of these, chosen.
    AddManaOneOf(Vec<crate::mana::Mana>),
    /// "Add {G} for each Elf you control": `count` of this mana (`Any`:
    /// of one color, chosen once).
    AddManaTimes {
        mana: crate::mana::Mana,
        count: Num,
    },
    /// "Add two mana in any combination of colors", "Add three mana in any
    /// combination of {R} and/or {G}" (each mana one of `colors`, chosen
    /// one at a time), "Add two mana of different colors" (`different`:
    /// no color twice).
    AddManaCombination {
        count: u32,
        colors: ColorSet,
        different: bool,
    },
    /// The recipient player discards N cards of their choice (CR 701.8).
    Discard {
        count: Num,
        who: Recipient,
    },
    /// `a` fights `b` (CR 701.12): each deals damage equal to its power to
    /// the other.
    Fight {
        a: Recipient,
        b: Recipient,
    },
    /// `source` deals damage equal to its power to `to` ("Target creature
    /// you control deals damage equal to its power to target creature you
    /// don't control"; with `to == source`, "deals damage to itself equal
    /// to its power"). The creature is the damage's source (CR 120.3), so
    /// its deathtouch, lifelink, infect, and wither apply. One-sided,
    /// unlike fight (CR 701.12).
    Bite {
        source: Recipient,
        to: Recipient,
    },
    /// `source` (an object other than the ability's source) deals `amount`
    /// damage to `to` (CR 120.3): "Whenever equipped creature becomes
    /// blocked, it deals 1 damage to defending player."
    DamageFrom {
        source: Recipient,
        amount: Num,
        to: Recipient,
    },
    /// Exile every card in every graveyard.
    ExileAllGraveyards,
    /// Create a replacement or prevention effect that lasts until end of
    /// turn (CR 614.3, 615.3): "Prevent all combat damage that would be
    /// dealt this turn", "Prevent the next 4 damage that would be dealt to
    /// any target this turn" (a shield, CR 615.7).
    CreateReplacement(ReplacementAbility),
    /// A "can't" effect that lasts until end of turn (CR 614.17).
    Rule(RuleKind),
    Tap {
        to: Recipient,
    },
    /// "Tap or untap target permanent": its controller (the ability's)
    /// chooses as it resolves.
    TapOrUntap {
        to: Recipient,
    },
    Untap {
        to: Recipient,
    },
    /// "gains haste until end of turn" (layer 6).
    Grant {
        keywords: KeywordSet,
        to: Recipient,
    },
    /// "Gain control of target creature until end of turn" (layer 2,
    /// CR 613.1b).
    GainControl {
        of: Recipient,
    },
    /// Sacrifice a permanent (CR 701.21): its controller puts it into its
    /// owner's graveyard; indestructible does not help.
    Sacrifice {
        what: Recipient,
    },
    /// Create `count` tokens of a token definition under the recipient's
    /// control (CR 111).
    CreateToken {
        token: CardRef,
        count: Num,
        who: Recipient,
        /// "create a tapped Powerstone token": it enters tapped (CR
        /// 111.1, 614.1c).
        tapped: bool,
    },
    /// Attach the ability's source to the recipient (equip, CR 701.3).
    Attach {
        to: Recipient,
    },
    /// "becomes a 2/2 Assembly-Worker artifact creature until end of turn.
    /// It's still a land" (layers 4 and 7b).
    Animate {
        to: Recipient,
        types: TypeSet,
        subtypes: SubtypeSet,
        power: i32,
        toughness: i32,
        /// "becomes a 4/4 green Bear artifact creature": its colors.
        colors: Option<ColorSet>,
    },
    /// "becomes blue until end of turn", "becomes colorless" (layer 5).
    BecomeColor {
        to: Recipient,
        colors: ColorSet,
    },
    /// "Target land becomes an Island until end of turn" (layer 4);
    /// `indefinite`, with no duration ("Target land becomes a Forest. (This
    /// effect lasts indefinitely.)").
    BecomeLandType {
        to: Recipient,
        land: Color,
        indefinite: bool,
    },
    /// "Choose a creature type." (CR 608.2d): the controller chooses as
    /// the effect resolves, for the rest of the resolution's "of that
    /// type", "of the chosen color" filters and "becomes that type".
    Choose(ChoiceKind),
    /// "Each land you control becomes that type until end of turn": the
    /// color, creature type, or basic land type chosen earlier in the
    /// resolution (`Choose`).
    BecomeChosen {
        to: Recipient,
    },
    /// "Creatures you control gain protection from the chosen color until
    /// end of turn": the color chosen earlier in the resolution.
    ProtectionFromChosen {
        to: Recipient,
    },
    /// "becomes the color of your choice", "gains protection from the
    /// color of your choice", "becomes the basic land type of your
    /// choice", "becomes the creature type of your choice" until end of
    /// turn: the controller chooses as the effect resolves (CR 608.2d),
    /// once for all recipients. `protection` grants protection from the
    /// chosen color (a `ChoiceKind::Color`) instead of becoming it.
    OfChoice {
        to: Recipient,
        kind: ChoiceKind,
        protection: bool,
    },
    /// "You may [do these]": the controller decides on resolution.
    YouMay(Vec<Effect>),
    /// "pay {2}" / "pay 2 life", inside "you may": it happens only if the
    /// controller can pay; "If you do" then sees whether it did.
    Pay {
        mana: ManaCost,
        life: u32,
    },
    /// "If you do, [these]": only if the optional action before happened
    /// (CR 608.2c).
    IfYouDo(Vec<Effect>),
    /// "When you do, [this]": a reflexive triggered ability (CR 603.12),
    /// registered in the pool (`CardPool::register_ability`, trigger
    /// `TriggerSpec::Reflexive`), that triggers if the optional action
    /// before happened; its targets are chosen as it is put on the stack.
    Reflexive(AbilityId),
    /// "[these] unless [payer] pays {2}": the payer may pay; if they
    /// don't, the effects happen.
    UnlessPays {
        payer: Recipient,
        mana: ManaCost,
        life: u32,
        effects: Vec<Effect>,
    },
    /// "If this spell was kicked, [do these]" (CR 702.33c).
    IfKicked(Vec<Effect>),
    /// "If that creature was a Human, [do these]": the `n`th target,
    /// judged as it last existed if it has left (CR 608.2h).
    IfTarget {
        n: u8,
        filter: Box<ObjFilter>,
        then: Vec<Effect>,
    },
    /// "If you control a Treefolk, [these]": the pool's `n`th static
    /// condition, judged for the controller as the effect resolves.
    IfCondition(u16, Vec<Effect>),
    /// "[base]. If this spell was kicked, [alt] instead." (CR 608.2c:
    /// later text modifies the earlier instruction; not a replacement
    /// effect). Exactly one list happens, chosen as it resolves.
    Instead {
        cond: EffectCond,
        base: Vec<Effect>,
        alt: Vec<Effect>,
    },
    /// "has base power and toughness N/N until end of turn" (layer 7b).
    SetBasePt {
        to: Recipient,
        power: i32,
        toughness: i32,
    },
    /// Create a delayed triggered ability (CR 603.7) about the recipient:
    /// `ability` is a registered triggered ability (see
    /// `CardPool::register_ability`) whose effects see the recipient as
    /// `Recipient::SameObject`. A delayed trigger on an event rather than
    /// a step ("When that creature dies this turn, ...") watches for the
    /// recipient leaving the battlefield and, if `this_turn`, ends with the
    /// turn (CR 603.7b, 603.7c).
    Delayed {
        ability: AbilityId,
        about: Recipient,
        this_turn: bool,
    },
    /// Exile a permanent.
    Exile {
        what: Recipient,
    },
    /// Return a card from exile or a graveyard to the battlefield under its
    /// owner's control (or, `under_you`, the ability's controller's:
    /// "return that card to the battlefield under your control"),
    /// optionally tapped or with counters on it.
    ReturnToBattlefield {
        what: Recipient,
        counters: Option<(CounterKind, u32)>,
        tapped: bool,
        under_you: bool,
    },
    /// The ability's source becomes a copy of the recipient (CR 707),
    /// keeping its own definition's `copy_except` if `except`.
    BecomeCopy {
        of: Recipient,
        except: bool,
    },
    /// Create a token that is a copy of the recipient (CR 707.2, 111.4),
    /// with the source definition's `copy_except` if `except`.
    CreateTokenCopy {
        of: Recipient,
        except: bool,
    },
    /// "Each opponent gets a poison counter" (CR 122.1f).
    AddPoison {
        count: Num,
        who: Recipient,
    },
    /// "You get {E}{E}" (CR 107.14).
    AddEnergy {
        count: Num,
        who: Recipient,
    },
    /// "Pay {E}{E}", inside "you may": like `Pay`.
    PayEnergy(u32),
    /// Proliferate (CR 701.27): the controller adds one of each kind of
    /// counter already there to the permanents and players they choose;
    /// the engine chooses their own permanents' and players' beneficial
    /// counters and opponents' harmful ones (+1/+1 and loyalty on yours,
    /// -1/-1 on theirs, poison on opponents).
    Proliferate,
    /// The recipient creature explores (CR 701.44): its controller reveals
    /// the top card of their library; a land goes to their hand, otherwise
    /// the creature gets a +1/+1 counter and the card may go to the
    /// graveyard.
    Explore {
        what: Recipient,
    },
    /// Amass N (CR 701.47): put N +1/+1 counters on an Army the
    /// controller controls, creating `token` (a 0/0 Army) first if they
    /// control none.
    Amass {
        count: Num,
        token: CardRef,
    },
    /// The source's echo has been dealt with (CR 702.30a).
    ClearEcho,
    /// "Look at the top four cards of your library. You may reveal a
    /// creature card from among them and put it into your hand. Put the
    /// rest on the bottom of your library in any order." `filter` None is
    /// any card ("Put one of them into your hand"); `all` takes every
    /// match ("Put all creature cards revealed this way into your hand");
    /// the rest go to the bottom or, with `rest_graveyard`, the graveyard.
    Dig {
        count: Num,
        filter: Option<ObjFilter>,
        optional: bool,
        all: bool,
        rest_graveyard: bool,
    },
    /// "Look at the top three cards of your library, then put them back in
    /// any order" (the engine keeps the order).
    LookAtTop {
        count: Num,
    },
    /// Adapt N (CR 701.46): if the source has no +1/+1 counters, it gets
    /// N.
    Adapt(u32),
    /// The recipient creature connives (CR 701.50): its controller draws
    /// and discards a card; a nonland discard puts a +1/+1 counter on it.
    Connive {
        what: Recipient,
    },
    /// The source card, in exile, may be cast (madness, rebound, suspend).
    CastExiled(ExileCast),
    /// Remove a time counter from the suspended source card; as the last
    /// is removed, it may be cast without paying its mana cost.
    SuspendTick,
    /// Cascade from the source spell (CR 702.85a).
    Cascade,
    /// Discover N (CR 701.57): like cascade, mana value N or less; the card
    /// is cast free or put into the hand.
    Discover(u32),
    /// Exploit (CR 702.110): the controller may sacrifice a creature; if
    /// they do, the source's "when this exploits a creature" triggers.
    Exploit,
    /// Modular's dies trigger (CR 702.43a): the source's +1/+1 counters as
    /// it last existed, put on the recipient.
    MoveSourceCounters {
        to: Recipient,
    },
    /// Cumulative upkeep (CR 702.24a): an age counter on the source, then
    /// sacrifice it unless its controller pays the cost once per age
    /// counter.
    CumulativeUpkeep(ManaCost),
    /// Manifest (CR 701.34): the top card of the controller's library
    /// onto the battlefield face down as a 2/2 creature.
    Manifest,
    /// "Target creature gains '...' until end of turn": a registered
    /// ability granted until end of turn (layer 6).
    GrantAbility {
        ability: AbilityId,
        to: Recipient,
    },
    /// A modal triggered ability's modes ("When this creature enters,
    /// choose one —"), as its only effect: the mode is chosen as the
    /// ability is put on the stack (CR 700.2, 603.3c), and the ability's
    /// targets are the chosen mode's.
    Modes(Vec<Mode>),
    /// "Switch target creature's power and toughness until end of turn"
    /// (layer 7d).
    SwitchPt {
        to: Recipient,
    },
    /// "It doesn't untap during its controller's next untap step".
    SkipNextUntap {
        to: Recipient,
    },
    /// "Target creature blocks this creature this turn if able" (CR
    /// 509.1c): a blocking requirement on `blocker` for one attacker.
    BlocksIfAble {
        blocker: Recipient,
        attacker: Recipient,
    },
    /// "You may put a land card from your hand onto the battlefield": the
    /// controller chooses a matching card from their hand (or none).
    PutFromHand {
        filter: ObjFilter,
        tapped: bool,
    },
    /// "If a creature card was exiled this way, [these]" (CR 608.2c).
    IfThisWay {
        cond: ThisWayCond,
        then: Vec<Effect>,
    },
    /// "you may tap two other untapped creatures you control", "return up
    /// to two lands you control to their owner's hand": the controller
    /// chooses `count` objects matching `filter` (up to `count` with
    /// `up_to`) and `act`s on each. Without `up_to` it happens only if
    /// they can choose all of them (CR 101.3 doesn't let a partial cost
    /// count as done); "If you do" then sees whether it happened.
    ChooseAct {
        act: ChooseAct,
        filter: ObjFilter,
        count: Num,
        up_to: bool,
    },
    /// "sacrifice an artifact or discard a card", "create a Food token or
    /// a Treasure token", "gains your choice of lifelink or indestructible":
    /// the controller chooses one of these it can do (CR 608.2d); with
    /// none possible nothing happens and "If you do" sees that.
    OneOf(Vec<Effect>),
    /// "Then that player discards two cards unless they discard a land
    /// card": the player discards one card matching `filter` or else
    /// `count` cards (CR 608.2d: they choose as it resolves).
    DiscardUnless {
        who: Recipient,
        count: Num,
        filter: ObjFilter,
    },
    /// "Discard your hand", "discards all Trap cards": every card in the
    /// player's hand, or every one matching `filter`.
    DiscardAll {
        who: Recipient,
        filter: Option<ObjFilter>,
    },
    /// Ninjutsu's effect (CR 702.49a, c): the source card, if still in its
    /// owner's hand, onto the battlefield tapped and attacking the player or
    /// planeswalker the returned creature was attacking.
    EnterAttacking,
    /// Saddle's effect (CR 702.171a): the source becomes saddled until end
    /// of turn.
    BecomeSaddled,
}

impl Effect {
    /// Whether applying this effect may ask its controller (or another
    /// player) a question in the middle of resolution.
    pub const fn asks(&self) -> bool {
        matches!(
            self,
            Effect::YouMay(_)
                | Effect::IfKicked(_)
                | Effect::IfYouDo(_)
                | Effect::IfCondition(..)
                | Effect::IfTarget { .. }
                | Effect::Instead { .. }
                | Effect::UnlessPays { .. }
                | Effect::Scry { .. }
                | Effect::Surveil { .. }
                | Effect::Discard { .. }
                | Effect::CounterUnlessPays { .. }
                | Effect::Search { .. }
                | Effect::AddMana(_)
                | Effect::AddManaOneOf(_)
                | Effect::AddManaTimes { .. }
                | Effect::AddManaCombination { .. }
                | Effect::SacrificeChosen { .. }
                | Effect::ReturnChosen { .. }
                | Effect::DiscardRandom { .. }
                | Effect::DiscardChosen { .. }
                | Effect::Explore { .. }
                | Effect::PutFromHand { .. }
                | Effect::Dig { .. }
                | Effect::Connive { .. }
                | Effect::CastExiled(_)
                | Effect::SuspendTick
                | Effect::Cascade
                | Effect::PutInLibrary {
                    place: LibraryPlace::TopOrBottom | LibraryPlace::FromTopOrBottom(_),
                    ..
                }
                | Effect::Discover(_)
                | Effect::Exploit
                | Effect::CumulativeUpkeep(_)
                | Effect::TapOrUntap { .. }
                | Effect::OfChoice { .. }
                | Effect::Choose(_)
                | Effect::IfThisWay { .. }
                | Effect::ChooseAct { .. }
                | Effect::OneOf(_)
                | Effect::DiscardUnless { .. }
                | Effect::Vote { .. }
                | Effect::VoteReturn { .. }
        )
    }
}

/// One row of a die roll's results table (CR 706.3a): "1—9 | ...",
/// "20+ | ..." (`hi` is `u8::MAX`).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct DieRow {
    pub lo: u8,
    pub hi: u8,
    pub effects: Vec<Effect>,
}

/// What an `Effect::Instead` checks as it resolves.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum EffectCond {
    /// "If this spell was kicked" (CR 702.33c).
    Kicked,
    /// The pool's `n`th static condition, judged for the controller.
    Static(u16),
    /// "If it's a Human": the `n`th target, or as it last existed if it
    /// has left (CR 608.2h).
    Target { n: u8, filter: Box<ObjFilter> },
    /// "If the sacrificed creature was legendary": what the spell or
    /// ability did, or paid, this way.
    ThisWay(ThisWayCond),
}

/// What a resolving spell or ability did to objects "this way" (CR
/// 608.2c): each kind is a zone change the resolution, or the costs paid
/// for it (CR 601.2h, 602.2b), made.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ThisWay {
    /// Put into exile.
    Exiled,
    /// Sacrificed (CR 701.21): left the battlefield. Within one
    /// resolution only the effect that says so moves permanents off it.
    Sacrificed,
    /// Destroyed (CR 701.7): likewise.
    Destroyed,
    /// Discarded (CR 701.9): from a hand.
    Discarded,
    /// Milled (CR 701.13): from a library to a graveyard.
    Milled,
    /// Put into a hand ("returned this way", "If you put an artifact card
    /// into your hand this way").
    PutInHand,
}

/// "If a creature card was exiled this way", "If you discarded a card this
/// way", "If the discarded card wasn't a land card": some object the
/// resolution moved `verb`'s way matches `filter` (judged as it last
/// existed, CR 608.2h), or with `negated` some such object doesn't; with
/// `by_you`, only cards the controller owned.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ThisWayCond {
    pub verb: ThisWay,
    pub filter: Option<Box<ObjFilter>>,
    pub negated: bool,
    pub by_you: bool,
}

/// What `Effect::ChooseAct` does to each object its controller chooses.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ChooseAct {
    /// "tap another untapped Citizen you control" (permanents you control).
    Tap,
    /// "sacrifice a creature" (permanents you control).
    Sacrifice,
    /// "return up to two lands you control to their owner's hand".
    Bounce,
    /// "discard a creature card" (cards in your hand).
    Discard,
    /// "exile two creature cards from your graveyard".
    ExileFromGraveyard,
    /// "return a creature card from your graveyard to your hand".
    ReturnFromGraveyard,
    /// "return a Zombie card at random from your graveyard to your hand":
    /// the cards are picked at random, not by the controller.
    ReturnRandomFromGraveyard,
    /// "put a permanent card from among the milled cards into your hand":
    /// cards this resolution milled, still in the graveyard.
    MilledToHand,
}

/// What a player chooses as a permanent enters ("As this land enters,
/// choose a color", CR 614.12) or as an effect resolves ("the color of
/// your choice").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ChoiceKind {
    /// A color ("choose a color other than white").
    Color { except: Option<Color> },
    /// One or more colors ("the color or colors of your choice"): only
    /// `Effect::OfChoice` asks this, a color at a time.
    Colors,
    /// A creature type ("choose a creature type other than Wall").
    CreatureType { except: Option<Subtype> },
    /// One of these basic land types: all five for "the basic land type
    /// of your choice", or "a Plains or an Island".
    LandType(ColorSet),
}

/// Effects that modify the rules rather than objects (CR 613.11, 614.17).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum RuleKind {
    /// "Players can't gain life."
    NoLifeGain,
    /// "Damage can't be prevented." Prevention effects still apply but
    /// prevent nothing (CR 615.12).
    DamageCantBePrevented,
}

/// Who or what a replacement or prevention effect's event may affect.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Affects {
    /// Any permanent or player.
    Any,
    /// The ability's source only ("if damage would be dealt to ~").
    This,
    /// The controller ("you").
    You,
    /// Players, relative to the controller.
    Players(Who),
    /// Battlefield objects matching a filter (judged for the source and
    /// its controller).
    Objects(ObjFilter),
    /// The controller and objects matching a filter ("you and creatures
    /// you control").
    YouAnd(ObjFilter),
    /// The spell's `n`th chosen target, locked in when the effect is
    /// created (stored effects only).
    Target(usize),
    /// Objects matching a filter other than the spell's `n`th chosen
    /// target, locked in likewise ("creatures other than target creature",
    /// Terrifying Presence).
    OtherThanTarget(ObjFilter, usize),
}

/// The event a replacement or prevention effect watches for (CR 614.1,
/// 615.1).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Replaces {
    /// Damage a source matching `from` (any source if `None`) would deal to
    /// `to`; `combat` restricts to combat or noncombat damage. `from` is
    /// judged as an object: `This`, `Objects`, `Target`, `OtherThanTarget`
    /// (CR 609.7: the source of the damage).
    Damage {
        from: Option<Affects>,
        to: Affects,
        combat: Option<bool>,
    },
    /// Damage that would be dealt to `what` or dealt by it ("Prevent all
    /// combat damage that would be dealt to and dealt by enchanted
    /// creature", Gaseous Form). One effect: it applies once to damage a
    /// creature would deal to itself (CR 614.5).
    DamageToAndBy { what: Affects, combat: Option<bool> },
    /// A player would gain life.
    GainLife { whose: Who },
    /// A player would draw a card; `library_empty` requires an empty
    /// library (Laboratory Maniac).
    Draw { whose: Who, library_empty: bool },
    /// A card would be put into a graveyard owned by `whose`, from anywhere
    /// or only from the battlefield.
    ToGraveyard { whose: Who, from_battlefield: bool },
    /// +1/+1 counters would be put on a permanent matching `on`, including
    /// as it enters (CR 122.6).
    PlusCounters { on: ObjFilter },
    /// A permanent would be destroyed (CR 701.7); regeneration shields
    /// (CR 701.15) replace this.
    Destroy { to: Affects },
    /// A creature would die ("If that creature would die this turn, exile
    /// it instead").
    Dies { to: Affects },
    /// How this permanent itself enters the battlefield (CR 614.1c).
    Enters,
}

/// What happens instead.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Replacement {
    /// Prevent all of the damage, or the player gains no life.
    Prevent,
    /// A shield: prevent the next N damage (CR 615.7).
    PreventNext(u32),
    /// Double the damage or life.
    Double,
    /// That many plus one counters.
    PlusOne,
    /// Put the card into this zone instead.
    Instead(crate::state::Zone),
    /// Draw two cards instead of one.
    DrawTwo,
    /// The player wins the game instead.
    WinGame,
    /// Enters tapped.
    EnterTapped,
    /// Enters tapped unless the pool's `n`th condition holds for its
    /// controller ("unless you control a planeswalker", CR 614.1d).
    EnterTappedUnless(u16),
    /// Enters with counters if the pool's `cond`th condition holds
    /// (bloodthirst, CR 702.54a).
    EnterWithCountersIf {
        cond: u16,
        kind: CounterKind,
        n: u32,
    },
    /// Enters with counters.
    EnterWithCounters { kind: CounterKind, n: Num },
    /// CR 701.15a: instead of being destroyed, the permanent has all damage
    /// removed, is tapped, and leaves combat. A stored shield; used once.
    Regenerate,
    /// Damage that would be dealt to the controller is dealt to the
    /// permanent this Aura is attached to instead (Pariah).
    RedirectToAttached,
}

/// A replacement or prevention effect (CR 614, 615), either a static
/// ability of a permanent or created by a resolving spell.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ReplacementAbility {
    pub replaces: Replaces,
    pub replacement: Replacement,
    /// Effects that happen when this applies (CR 615.5): Phantom Tiger's
    /// "Remove a +1/+1 counter from it".
    pub then: Vec<Effect>,
}

impl ReplacementAbility {
    pub fn is_prevention(&self) -> bool {
        matches!(
            self.replacement,
            Replacement::Prevent | Replacement::PreventNext(_)
        ) && matches!(
            self.replaces,
            Replaces::Damage { .. } | Replaces::DamageToAndBy { .. }
        )
    }
}

/// What a triggered ability watches: its own source, or any object matching
/// a filter (judged relative to the source and its controller).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum TriggerObj {
    This,
    Filter(ObjFilter),
}

/// Event classes a trigger can watch (`TriggerSpec::watch_mask`, `Event::watch_bit`).
pub const WATCH_ZONE_CHANGE: u16 = 1;
pub const WATCH_STEP_BEGIN: u16 = 2;
pub const WATCH_ATTACK: u16 = 4;
pub const WATCH_BLOCK: u16 = 8;
pub const WATCH_DAMAGE: u16 = 16;
pub const WATCH_LIFE_CHANGE: u16 = 32;
pub const WATCH_CAST: u16 = 64;
pub const WATCH_TARGETED: u16 = 128;
pub const WATCH_TAP: u16 = 256;
pub const WATCH_FACE_UP: u16 = 512;
pub const WATCH_DRAW: u16 = 1024;
pub const WATCH_CYCLE: u16 = 2048;
pub const WATCH_DISCARD: u16 = 4096;
pub const WATCH_ROLL: u16 = 8192;

/// Trigger conditions (CR 603.1). Each names the event and, where the
/// ability's text restricts it, which objects or players it watches.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum TriggerSpec {
    /// "When ~ enters" / "Whenever another creature enters".
    Enters(TriggerObj),
    /// A reflexive trigger's (CR 603.12, `Effect::Reflexive`): created by
    /// the effect itself, never by an event.
    Reflexive,
    /// "When ~ dies" / "Whenever a creature you control dies" (CR 700.4);
    /// also "is put into a graveyard from the battlefield" for
    /// noncreature permanents. A leaves-the-battlefield trigger: it looks
    /// back in time (CR 603.10).
    Dies(TriggerObj),
    /// "At the beginning of [whose] <step>".
    BeginningOfStep { step: crate::turn::Step, whose: Who },
    /// "At the beginning of the upkeep of enchanted creature's controller"
    /// (Curses and their kin): the step of the turn of whoever controls
    /// what the source is attached to, judged as the step begins. That
    /// permanent is the triggering object ("that creature"), its
    /// controller the triggering player ("that player").
    AttachedControllersStep(crate::turn::Step),
    /// "Whenever ~ attacks" / "Whenever a creature you control attacks".
    Attacks(TriggerObj),
    /// "Whenever ~ blocks".
    Blocks(TriggerObj),
    /// "Whenever a creature you control attacks alone" (exalted, CR
    /// 506.5).
    AttacksAlone(TriggerObj),
    /// "Whenever this creature becomes blocked" (once per combat).
    BecomesBlocked(TriggerObj),
    /// "Whenever you attack" (`min` 1), "Whenever you attack with three or
    /// more creatures" (`min` 3), and battalion's "Whenever this creature
    /// and at least two other creatures attack" (`this`, `min` 3): once
    /// per declaration of attackers by the controller (CR 508.1m), when
    /// at least `min` creatures (with `this`, the source among them)
    /// attack.
    AttacksWith { this: bool, min: u8 },
    /// "Whenever this creature attacks and isn't blocked" (CR 509.1h): as
    /// blockers are declared, an attacking creature no creature blocks.
    AttacksUnblocked(TriggerObj),
    /// "Whenever this creature blocks or becomes blocked".
    BlocksOrBlocked(TriggerObj),
    /// "Whenever this creature attacks or blocks".
    AttacksOrBlocks(TriggerObj),
    /// "When this creature leaves the battlefield" (to any zone).
    Leaves(TriggerObj),
    /// "Whenever ~ deals combat damage to a player".
    DealsCombatDamageToPlayer(TriggerObj),
    /// "Whenever you gain life".
    GainsLife { whose: Who },
    /// "Whenever you roll one or more dice" (CR 706): once per
    /// instruction to roll, not per die.
    RollsDice { whose: Who },
    /// "Whenever you win a coin flip" (CR 705.2).
    WinsFlip { whose: Who },
    /// "Whenever you cast a spell"; `noncreature` or `instant_or_sorcery`
    /// restrict it.
    CastsSpell {
        whose: Who,
        noncreature: bool,
        instant_or_sorcery: bool,
    },
    /// "When ~ becomes the target of a spell or ability" (CR 115.4?; Phantasmal
    /// Image).
    BecomesTarget(TriggerObj),
    /// "Whenever a creature without flanking blocks this creature": the
    /// blocker is the triggering object.
    BlockedBy(ObjFilter),
    /// "Whenever this creature blocks a creature" (CR 509.3b), "becomes
    /// blocked by a creature" (CR 509.3d), or "blocks or becomes blocked
    /// by a creature": once per blocking pair, so once for each attacker
    /// it blocks or each creature blocking it. The other creature of the
    /// pair, which must match `other`, is the triggering object ("that
    /// creature").
    BlockPair {
        what: TriggerObj,
        other: ObjFilter,
        blocks: bool,
        blocked: bool,
    },
    /// Ward (CR 702.21a): "Whenever this permanent becomes the target of a
    /// spell or ability an opponent controls"; the spell or ability is the
    /// triggering object.
    TargetedByOpponent,
    /// "Whenever this creature becomes tapped".
    BecomesTapped(TriggerObj),
    /// "Whenever this creature becomes untapped" (inspired).
    BecomesUntapped(TriggerObj),
    /// "Whenever this creature is dealt damage" (enrage); the damaged
    /// object is the triggering object.
    DealtDamage(TriggerObj),
    /// "Whenever this creature deals damage to a creature / an opponent /
    /// a player": the damaged creature (or player) is the triggering
    /// object (player); `combat` only for combat damage.
    DealsDamage {
        what: TriggerObj,
        to: DamageTo,
        combat: bool,
    },
    /// "Whenever you cast an artifact spell", "a multicolored spell", "a
    /// Spirit or Arcane spell": the spell matches the filter.
    CastsFiltered { whose: Who, filter: ObjFilter },
    /// "When this creature is turned face up", "Whenever a permanent is
    /// turned face up" (CR 708.8), the permanent judged face up.
    TurnedFaceUp(TriggerObj),
    /// "Whenever you draw your second card each turn": the `n`th card a
    /// player draws in a turn.
    DrawsNth { whose: Who, n: u8 },
    /// "When this creature exploits a creature" (CR 702.110): queued by
    /// the exploit trigger itself, never by an event.
    Exploits,
    /// "When you cycle this card" (`this`; CR 702.29d: it triggers from
    /// the graveyard the card was discarded to), "Whenever you cycle a
    /// card", "Whenever you cycle or discard a card" (`or_discard`: once
    /// per card discarded, cycling being a discard, CR 702.29a).
    /// `another` is wording only ("another card"): a permanent on the
    /// battlefield can't be the card cycled.
    Cycles {
        this: bool,
        another: bool,
        or_discard: bool,
    },
    /// "Whenever you cast your second spell each turn" and kin: the
    /// spell's place among the spells its caster cast this turn.
    CastsNth { whose: Who, nth: SpellOrdinal },
    /// "When this creature enters and whenever you cast ...", "When you
    /// cycle this card and when this creature dies": one ability with two
    /// trigger events.
    Either(Box<(TriggerSpec, TriggerSpec)>),
    /// "Whenever one or more artifacts you control enter": once for all
    /// the matching objects that enter together (CR 603.2c), the number
    /// of them being the trigger's amount ("that much"). Objects entering
    /// before the next time a player would receive priority count as
    /// together.
    EntersBatch(ObjFilter),
}

/// Which of a player's spells this turn a cast trigger wants.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum SpellOrdinal {
    /// "your first spell each turn".
    First,
    /// "your second spell each turn".
    Second,
    /// "a spell other than your first spell each turn".
    NotFirst,
    /// "your first spell during each opponent's turn".
    FirstOnOpponentsTurn,
}

impl SpellOrdinal {
    /// Whether the `n`th spell its caster cast this turn qualifies, cast
    /// on its caster's turn or not.
    pub fn holds(self, n: u8, own_turn: bool) -> bool {
        match self {
            SpellOrdinal::First => n == 1,
            SpellOrdinal::Second => n == 2,
            SpellOrdinal::NotFirst => n >= 2,
            SpellOrdinal::FirstOnOpponentsTurn => n == 1 && !own_turn,
        }
    }
}

/// What a damage trigger's damage must be dealt to.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum DamageTo {
    Any,
    Creature,
    Player,
    Opponent,
}

impl TriggerSpec {
    /// "When you cycle this card", alone or as one of two events.
    pub fn cycles_this(&self) -> bool {
        match self {
            TriggerSpec::Cycles { this, .. } => *this,
            TriggerSpec::Either(pair) => pair.0.cycles_this() || pair.1.cycles_this(),
            _ => false,
        }
    }

    /// Whether this kind of trigger can see `event` at all, judged from the
    /// event's kind alone. A cheap filter before the full check.
    pub fn watches(&self, event: &crate::event::Event) -> bool {
        self.watch_mask() & event.watch_bit() != 0
    }

    /// The kinds of event this trigger can fire on, as bits matching
    /// `Event::watch_bit`. A definition's mask is the union over its
    /// triggers, so "could anything here care about this event" is one
    /// `&` instead of a walk over abilities.
    pub fn watch_mask(&self) -> u16 {
        match self {
            TriggerSpec::Enters(_)
            | TriggerSpec::Dies(_)
            | TriggerSpec::Leaves(_)
            | TriggerSpec::EntersBatch(_) => WATCH_ZONE_CHANGE,
            TriggerSpec::AttacksOrBlocks(_) => WATCH_ATTACK | WATCH_BLOCK,
            TriggerSpec::BeginningOfStep { .. } | TriggerSpec::AttachedControllersStep(_) => {
                WATCH_STEP_BEGIN
            }
            TriggerSpec::Attacks(_)
            | TriggerSpec::AttacksAlone(_)
            | TriggerSpec::AttacksWith { .. } => WATCH_ATTACK,
            TriggerSpec::Blocks(_)
            | TriggerSpec::BecomesBlocked(_)
            | TriggerSpec::BlocksOrBlocked(_)
            | TriggerSpec::BlockedBy(_)
            | TriggerSpec::BlockPair { .. }
            | TriggerSpec::AttacksUnblocked(_) => WATCH_BLOCK,
            TriggerSpec::DealsCombatDamageToPlayer(_)
            | TriggerSpec::DealtDamage(_)
            | TriggerSpec::DealsDamage { .. } => WATCH_DAMAGE,
            TriggerSpec::BecomesTapped(_) | TriggerSpec::BecomesUntapped(_) => WATCH_TAP,
            TriggerSpec::TurnedFaceUp(_) => WATCH_FACE_UP,
            TriggerSpec::DrawsNth { .. } => WATCH_DRAW,
            TriggerSpec::Exploits | TriggerSpec::Reflexive => 0,
            TriggerSpec::GainsLife { .. } => WATCH_LIFE_CHANGE,
            TriggerSpec::RollsDice { .. } | TriggerSpec::WinsFlip { .. } => WATCH_ROLL,
            TriggerSpec::CastsSpell { .. }
            | TriggerSpec::CastsFiltered { .. }
            | TriggerSpec::CastsNth { .. } => WATCH_CAST,
            TriggerSpec::BecomesTarget(_) | TriggerSpec::TargetedByOpponent => WATCH_TARGETED,
            TriggerSpec::Cycles {
                or_discard: true, ..
            } => WATCH_DISCARD,
            TriggerSpec::Cycles { .. } => WATCH_CYCLE,
            TriggerSpec::Either(pair) => pair.0.watch_mask() | pair.1.watch_mask(),
        }
    }
}

/// An intervening "if" clause (CR 603.4), checked when the ability would
/// trigger and again on resolution.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Condition {
    SourceUntapped,
    /// "if this creature is tapped" (survival).
    SourceTapped,
    /// "if no creatures are on the battlefield" (Pestilence).
    NoCreatures,
    /// "if it was kicked" (CR 702.33c): the source was cast with its kicker
    /// paid.
    Kicked,
    /// "if it had no -1/-1 counters on it" (persist), judged from the
    /// source as it last existed on the battlefield.
    HadNoCounters(CounterKind),
    /// "if no spells were cast last turn" (werewolves).
    NoSpellsLastTurn,
    /// "if a player cast two or more spells last turn".
    TwoSpellsLastTurn,
    /// Echo (CR 702.30a): the source came under its controller's control
    /// since their last upkeep.
    EchoDue,
    /// The pool's `n`th static condition ("if you control an artifact",
    /// "if you have 5 or less life").
    Static(u16),
    /// "if it was evoked" (CR 702.74a).
    Evoked,
}

/// A triggered ability (CR 603).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct TriggeredAbility {
    pub trigger: TriggerSpec,
    pub condition: Option<Condition>,
    /// Targets chosen when the ability is put on the stack (CR 603.3d).
    pub targets: Vec<TargetSpec>,
    pub effects: Vec<Effect>,
}

/// What a cost does to the permanent chosen for it.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
pub enum ChosenCost {
    #[default]
    Sacrifice,
    Return,
    Tap,
}

/// What an activated ability's cost sacrifices (CR 602.2b, 701.21).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum SacCost {
    /// "Sacrifice ~".
    This,
    /// "Sacrifice a Goblin": the player chooses a matching permanent.
    Filter(ObjFilter),
}

/// The cost of an activated ability (CR 118, 602.2b).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
#[allow(clippy::struct_excessive_bools)] // each is one independent cost part
pub struct Cost {
    pub mana: ManaCost,
    /// "Discard this card" (cycling, CR 702.29): the source leaves the hand
    /// as the cost is paid.
    pub discard_self: bool,
    /// "Exile this card from your graveyard" (an ability activated from
    /// the graveyard).
    pub exile_self: bool,
    /// Crew N (CR 702.122): tap any number of other untapped creatures you
    /// control with total power N or more.
    pub crew: u32,
    /// "Remove a charge counter from this artifact" (CR 122.8).
    pub remove_counters: Option<(CounterKind, u32)>,
    /// "Pay {E}{E}" (CR 107.14).
    pub energy: u32,
    /// What a `SacCost::Filter` cost does to the chosen permanent:
    /// sacrifice it, return it to its owner's hand ("Return a land you
    /// control to its owner's hand"), or tap it ("Tap an untapped creature
    /// you control").
    pub chosen: ChosenCost,
    /// "Pay N life" (CR 119.4: payable only with at least that much life).
    pub life: u32,
    /// "Discard N cards" (CR 701.8); the cards are chosen as the cost is paid.
    pub discard: u32,
    /// "Discard X cards" (a spell's additional cost): X is announced as the
    /// spell is cast (CR 601.2b, 107.3) and that many cards are discarded.
    pub discard_x: bool,
    /// "Discard a card at random": the `discard` cards are chosen at
    /// random as the cost is paid, not by the player.
    pub discard_random: bool,
    /// "Discard a creature card": what the discarded card must match.
    pub discard_filter: Option<ObjFilter>,
    /// A loyalty ability's cost (CR 606): +N puts, −N removes that many
    /// loyalty counters; nonzero marks the ability as a loyalty ability
    /// (sorcery speed, one per planeswalker per turn, CR 606.3).
    pub loyalty: i32,
    /// {T}: tap the source. A creature's {T} ability needs the source to
    /// have been under its controller's control since their most recent
    /// turn began (CR 302.6).
    pub tap: bool,
    pub sacrifice: Option<SacCost>,
}

impl Cost {
    pub fn parse(mana: &str) -> Cost {
        Cost {
            mana: ManaCost::parse(mana).unwrap_or_else(|e| panic!("{mana}: {e}")),
            discard_self: false,
            exile_self: false,
            crew: 0,
            remove_counters: None,
            energy: 0,
            chosen: ChosenCost::Sacrifice,
            life: 0,
            discard: 0,
            discard_x: false,
            discard_random: false,
            discard_filter: None,
            loyalty: 0,
            tap: false,
            sacrifice: None,
        }
    }

    #[must_use]
    pub fn loyalty(mut self, n: i32) -> Cost {
        self.loyalty = n;
        self
    }

    #[must_use]
    pub fn life(mut self, n: u32) -> Cost {
        self.life = n;
        self
    }

    #[must_use]
    pub fn discard(mut self, n: u32) -> Cost {
        self.discard = n;
        self
    }

    /// Whether paying needs a choice from the player (what to sacrifice or
    /// discard) beyond mana.
    pub fn needs_choice(&self) -> bool {
        matches!(self.sacrifice, Some(SacCost::Filter(_))) || self.discard > 0 || self.discard_x
    }

    #[must_use]
    pub fn tap(mut self) -> Cost {
        self.tap = true;
        self
    }

    #[must_use]
    pub fn sacrifice(mut self, what: SacCost) -> Cost {
        self.sacrifice = Some(what);
        self
    }
}

/// What restricted mana may be spent on (CR 106.6): "Spend this mana only
/// to cast an artifact spell or activate an ability." Mana spent on
/// anything else (a cost an effect asks for, a special action) is never
/// allowed.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ManaRestriction {
    /// The spells it may be spent to cast; `None` for none. With
    /// `SpendWording::CantCast`, the spells it may *not* be spent on.
    pub spells: Option<ObjFilter>,
    /// It may be spent to activate abilities (any activated ability, mana
    /// abilities and abilities of objects off the battlefield included).
    pub abilities: bool,
    /// Only abilities of sources matching this ("activate an ability of
    /// an artifact source"); `None` for any source.
    #[serde(default)]
    pub ability_source: Option<ObjFilter>,
    /// It may be spent to foretell a card (CR 702.143a): "Spend this mana
    /// only to foretell a card from your hand or cast an instant or
    /// sorcery spell."
    #[serde(default)]
    pub foretell: bool,
    /// How the sentence reads; `CantCast` also turns `spells` around.
    pub wording: SpendWording,
}

/// The sentence of a `ManaRestriction`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum SpendWording {
    /// "Spend this mana only to cast an artifact spell [or activate an
    /// ability]."
    #[default]
    CastFirst,
    /// "Spend this mana only to cast an artifact spell or to activate an
    /// ability."
    CastFirstTo,
    /// "Spend this mana only to activate an ability or cast an artifact
    /// spell."
    ActivateFirst,
    /// "This mana can't be spent to cast a nonartifact spell.": anything
    /// but casting a spell `spells` matches.
    CantCast,
}

impl ManaRestriction {
    /// Whether the mana may be spent to cast a spell with these
    /// characteristics.
    pub fn allows_spell(&self, c: &crate::state::Characteristics) -> bool {
        let matched = self
            .spells
            .as_ref()
            .is_some_and(|f| f.matches_characteristics(c));
        if self.wording == SpendWording::CantCast {
            !matched
        } else {
            matched
        }
    }

    /// Whether the mana may be spent to activate an ability of a source
    /// with these characteristics (`None`: a source not known, which
    /// passes only a restriction that names none).
    pub fn allows_abilities(&self, source: Option<&crate::state::Characteristics>) -> bool {
        (self.abilities || self.wording == SpendWording::CantCast)
            && match (&self.ability_source, source) {
                (None, _) => true,
                (Some(f), Some(c)) => f.matches_characteristics(c),
                (Some(_), None) => false,
            }
    }
}

/// An activated ability (CR 602): "[cost]: [effect]". Uses the stack like a
/// spell; mana abilities are `AbilityDef::TapForMana` instead (CR 605).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // independent restrictions
pub struct ActivatedAbility {
    pub cost: Cost,
    pub targets: Vec<TargetSpec>,
    pub effects: Vec<Effect>,
    /// "Activate only as a sorcery" (CR 602.5a; equip, CR 702.6a).
    pub sorcery_speed: bool,
    /// "Activate only once each turn" (1), "no more than twice" (2); 0
    /// is no limit.
    pub per_turn: u8,
    /// "Activate only if ..." / "Activate only during your upkeep" (CR
    /// 602.5b): each must hold, judged from the activating player.
    #[serde(default)]
    pub conditions: Vec<StaticCondition>,
    /// "Activate only once": once for as long as the source stays the same
    /// object (CR 400.7), not once each turn.
    #[serde(default)]
    pub once_ever: bool,
    /// Activated from the hand rather than the battlefield (cycling, CR
    /// 702.29).
    pub from_hand: bool,
    /// A cycling or typecycling ability (CR 702.29): activating it is
    /// "cycling" the card, for "when you cycle" triggers.
    #[serde(default)]
    pub cycling: bool,
    /// Activated from the graveyard (unearth, CR 702.84).
    pub from_graveyard: bool,
    /// A keyword that adds rules to the ability ("Boast — {1}{W}: ...").
    pub keyword: Option<ActivatedKeyword>,
    /// The ability's text, for naming the choice.
    pub text: String,
    /// "This ability costs {1} less to activate for each Town you
    /// control" (CR 601.2f, by way of 602.2b).
    #[serde(default)]
    pub reduction: Option<Box<ActivationReduction>>,
}

/// What an activated ability's own text takes off its cost: `by` once for
/// each of `each`, or once if `condition` holds ("This ability costs
/// {4}{B} less to activate if you've drawn three or more cards this
/// turn"), judged from the activating player.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ActivationReduction {
    pub by: ManaCost,
    pub each: Option<CountExpr>,
    pub condition: Option<StaticCondition>,
}

/// A keyword that adds rules to the activated ability after it.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ActivatedKeyword {
    /// CR 702.142a: "Activate only if this creature attacked this turn
    /// and only once each turn."
    Boast,
    /// CR 702.177a: "Activate only once."
    Exhaust,
    /// CR 702.193a: "If this permanent entered this turn, this ability's
    /// cost is reduced by this permanent's mana cost. Activate this
    /// ability only once."
    PowerUp,
    /// CR 702.49a: "[Cost], Reveal this card from your hand, Return an
    /// unblocked attacking creature you control to its owner's hand: Put
    /// this card onto the battlefield from your hand tapped and attacking."
    /// Activated from the hand, once blockers are declared (CR 509.1h: only
    /// then is an attacker unblocked); the creature returned is chosen as
    /// the ability's `sacrifice`.
    Ninjutsu,
}

impl ActivatedKeyword {
    /// The keyword as printed before the cost.
    pub fn name(self) -> &'static str {
        match self {
            ActivatedKeyword::Boast => "Boast",
            ActivatedKeyword::Exhaust => "Exhaust",
            ActivatedKeyword::PowerUp => "Power-up",
            ActivatedKeyword::Ninjutsu => "Ninjutsu",
        }
    }

    /// Whether the ability may be activated only once for the object's
    /// whole existence (CR 702.177a, 702.193a), not once each turn.
    pub fn once_ever(self) -> bool {
        matches!(self, ActivatedKeyword::Exhaust | ActivatedKeyword::PowerUp)
    }
}

impl ActivatedAbility {
    /// CR 605.1a: no targets, not a loyalty ability, and it adds mana.
    pub fn is_mana_ability(&self) -> bool {
        self.targets.is_empty()
            && self.cost.loyalty == 0
            && !self.effects.is_empty()
            && self.effects.iter().all(|e| {
                matches!(
                    e,
                    Effect::AddMana(_)
                        | Effect::AddManaOneOf(_)
                        | Effect::AddManaTimes { .. }
                        | Effect::AddManaCombination { .. }
                )
            })
    }
}

/// How a face-down card turns face up (CR 702.37, 702.168).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum MorphKind {
    Morph,
    /// Also gets a +1/+1 counter as it turns face up (CR 702.37b).
    Megamorph,
    /// Face down, it has ward {2} (CR 702.168).
    Disguise,
}

/// A static ability changing the generic part of spells' costs (CR
/// 601.2f): `generic` below zero is "cost {N} less", above is "more".
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct CostChange {
    /// What the spell must be, judged on its characteristics.
    pub spells: ObjFilter,
    /// Whose spells, from the source's controller.
    pub whose: Who,
    pub generic: i32,
}

/// "This spell costs {1} less to cast for each artifact you control"
/// (affinity, CR 702.41): `per` generic mana less, `times` times.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct SelfCost {
    pub per: u32,
    pub times: Num,
}

/// One mode of a "choose one" spell (CR 700.2).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct Mode {
    pub targets: Vec<TargetSpec>,
    pub effects: Vec<Effect>,
    pub text: String,
    /// "Choose one or both" (CR 700.2): the mode that is both, made of two
    /// modes' targets and effects one after the other. Effects from index
    /// `.0` on see the targets from index `.1` on as their own.
    pub split: Option<(u16, u16)>,
    /// A named mode's name ("• Smash the Chest — Destroy target
    /// artifact."), printed before its effect; it has no rules meaning.
    pub name: Option<String>,
}

/// Modifications a copy effect makes as part of copying (CR 707.9b):
/// "except it's an Illusion and has ..." (Phantasmal Image), "except it has
/// haste" (Kiki-Jiki). They become part of the copiable values.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
pub struct CopyExcept {
    pub subtypes: Vec<String>,
    pub keywords: KeywordSet,
    pub abilities: Vec<AbilityDef>,
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum AbilityDef {
    /// The effect of an instant or sorcery spell when it resolves.
    Spell {
        targets: Vec<TargetSpec>,
        effects: Vec<Effect>,
    },
    /// A "choose one" instant or sorcery (CR 700.2): the mode is chosen as
    /// the spell is cast (CR 601.2b), before targets.
    Modal(Vec<Mode>),
    /// "You may have ~ enter as a copy of any creature on the battlefield"
    /// (CR 707.? Clone): the choice is made as the permanent spell
    /// resolves, among battlefield objects matching the filter.
    EntersAsCopy(ObjFilter),
    /// "As this land enters, choose a color" (CR 614.12): the controller
    /// chooses as the permanent enters; the object remembers the choice
    /// (`GameObject::chosen`) for the abilities that refer to it ("the
    /// chosen color", CR 607.2a) until it leaves the battlefield.
    AsEntersChoose(ChoiceKind),
    /// "Enchant <spec>" (CR 702.5): an Aura's targeting requirement and
    /// attachment legality.
    Enchant(TargetSpec),
    /// "{T}: Add <mana>" (CR 605). Basic land types grant theirs
    /// intrinsically (CR 305.6) and do not list this.
    TapForMana(Mana),
    /// "{T}, Sacrifice this artifact: Add <mana>" (Treasure, Lotus
    /// Petal): a mana ability that sacrifices its source.
    SacForMana(Mana),
    /// "{T}: Add {C}. Spend this mana only to cast an instant or sorcery
    /// spell." (CR 106.6): a mana ability whose mana may be spent only as
    /// `restriction` allows. Payment uses it only where it is allowed.
    RestrictedMana {
        mana: Mana,
        restriction: ManaRestriction,
    },
    /// "Spend only black mana on X." (CR 107.3): the X in this card's cost
    /// is paid with mana of this color only.
    XManaColor(crate::mana::Color),
    /// A blocking restriction on this creature (CR 509.1b–c): "can't be
    /// blocked by Walls", "can block only creatures with flying".
    BlockRule(BlockRule),
    /// "Artifact spells you cast cost {1} less to cast" (CR 601.2f).
    CostChange(CostChange),
    /// "This creature can't attack unless defending player controls an
    /// Island", "can't block unless you control another Goblin", "can't
    /// attack or block unless ...": the pool's `condition`, judged for its
    /// controller (CR 508.1c, 509.1c).
    AttackUnless {
        condition: u16,
        attack: bool,
        block: bool,
    },
    /// Protection from what matches the filter ("protection from
    /// artifacts", "from Goblins", "from everything"), judged on a source's
    /// characteristics (CR 702.16); the object also has
    /// `Keyword::ProtectionFromFilter`.
    Protection(ObjFilter),
    /// Toxic N (CR 702.164): combat damage this deals to a player also
    /// gives them N poison counters.
    Toxic(u32),
    /// "You have no maximum hand size" (`delta` None), "Your maximum hand
    /// size is increased by one" (CR 402.2); `whose` from the controller.
    HandSize {
        whose: Who,
        delta: Option<i32>,
    },
    /// "While voting, you get an additional vote." (`optional` false) or
    /// "While voting, you may vote an additional time." (CR 701.38d: the
    /// votes happen when the controller would otherwise vote).
    ExtraVote {
        optional: bool,
    },
    Activated(ActivatedAbility),
    Static(StaticAbility),
    Triggered(TriggeredAbility),
    /// A replacement or prevention effect generated while the source is on
    /// the battlefield (CR 614.1a, 615.1a), or, for `Replaces::Enters`, as
    /// it enters (CR 614.1c).
    Replacement(ReplacementAbility),
}

impl AbilityDef {
    /// Targets chosen when this ability goes on the stack.
    pub fn targets(&self) -> &[TargetSpec] {
        match self {
            AbilityDef::Spell { targets, .. } => targets,
            AbilityDef::Triggered(t) => &t.targets,
            AbilityDef::Activated(a) => &a.targets,
            AbilityDef::Enchant(_)
            | AbilityDef::TapForMana(_)
            | AbilityDef::SacForMana(_)
            | AbilityDef::RestrictedMana { .. }
            | AbilityDef::XManaColor(_)
            | AbilityDef::BlockRule(_)
            | AbilityDef::CostChange(_)
            | AbilityDef::AttackUnless { .. }
            | AbilityDef::Protection(_)
            | AbilityDef::HandSize { .. }
            | AbilityDef::ExtraVote { .. }
            | AbilityDef::Toxic(_)
            | AbilityDef::Static(_)
            | AbilityDef::Replacement(_)
            | AbilityDef::Modal(_)
            | AbilityDef::EntersAsCopy(_)
            | AbilityDef::AsEntersChoose(_) => &[],
        }
    }

    /// Effects applied when this ability resolves.
    pub fn effects(&self) -> &[Effect] {
        match self {
            AbilityDef::Spell { effects, .. } => effects,
            AbilityDef::Triggered(t) => &t.effects,
            AbilityDef::Activated(a) => &a.effects,
            AbilityDef::Enchant(_)
            | AbilityDef::TapForMana(_)
            | AbilityDef::SacForMana(_)
            | AbilityDef::RestrictedMana { .. }
            | AbilityDef::XManaColor(_)
            | AbilityDef::BlockRule(_)
            | AbilityDef::CostChange(_)
            | AbilityDef::AttackUnless { .. }
            | AbilityDef::Protection(_)
            | AbilityDef::HandSize { .. }
            | AbilityDef::ExtraVote { .. }
            | AbilityDef::Toxic(_)
            | AbilityDef::Static(_)
            | AbilityDef::Replacement(_)
            | AbilityDef::Modal(_)
            | AbilityDef::EntersAsCopy(_)
            | AbilityDef::AsEntersChoose(_) => &[],
        }
    }

    /// Every effect this ability can apply, across modes and "you may"
    /// clauses, for static inspection.
    pub fn all_effects(&self) -> Vec<&Effect> {
        fn walk<'a>(e: &'a Effect, out: &mut Vec<&'a Effect>) {
            out.push(e);
            if let Effect::YouMay(inner)
            | Effect::IfKicked(inner)
            | Effect::IfYouDo(inner)
            | Effect::IfCondition(_, inner)
            | Effect::IfTarget { then: inner, .. }
            | Effect::IfThisWay { then: inner, .. }
            | Effect::OneOf(inner)
            | Effect::UnlessPays { effects: inner, .. } = e
            {
                for i in inner {
                    walk(i, out);
                }
            }
            if let Effect::FlipCoin { won } = e {
                for i in won {
                    walk(i, out);
                }
            }
            if let Effect::RollDie { rows, .. } = e {
                for i in rows.iter().flat_map(|r| &r.effects) {
                    walk(i, out);
                }
            }
            if let Effect::Instead { base, alt, .. } = e {
                for i in base.iter().chain(alt) {
                    walk(i, out);
                }
            }
            if let Effect::Modes(modes) = e {
                for m in modes {
                    for i in &m.effects {
                        walk(i, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        match self {
            AbilityDef::Modal(modes) => {
                for m in modes {
                    for e in &m.effects {
                        walk(e, &mut out);
                    }
                }
            }
            other => {
                for e in other.effects() {
                    walk(e, &mut out);
                }
            }
        }
        out
    }
}

/// Printed characteristics plus abilities (CR 109.3, 200.1).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // independent facts
pub struct CardDef {
    pub name: String,
    /// How the card's faces relate (CR 709, 712, 715); see `faces`.
    pub layout: Layout,
    /// Variable amounts the card's effects use (`Num::Count`).
    pub counts: Vec<CountExpr>,
    /// The card's other faces, registered as their own definitions: an
    /// adventurer's or omen's spell (CR 715), a double-faced card's other
    /// face (CR 712), a split card's halves (CR 709).
    pub faces: Vec<CardRef>,
    pub mana_cost: ManaCost,
    /// Color indicator (CR 204), for cards whose color is not their cost's.
    pub color_indicator: Option<ColorSet>,
    pub supertypes: SupertypeSet,
    pub types: TypeSet,
    pub subtypes: Vec<String>,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub keywords: KeywordSet,
    pub abilities: Vec<AbilityDef>,
    pub oracle_text: String,
    /// A token definition (CR 111): created by effects, never in a deck.
    pub token: bool,
    /// An emblem definition (CR 114): not a card, lives in the command zone.
    pub emblem: bool,
    /// A planeswalker's printed loyalty (CR 306.5b).
    pub loyalty: u32,
    /// What this card's copy effects add to the copiable values of what
    /// they copy (CR 707.9b).
    pub copy_except: Option<CopyExcept>,
    /// Filled in by `CardPool::add`: `subtypes` interned.
    #[serde(skip)]
    pub subtype_ids: SubtypeSet,
    /// Filled in by `CardPool::add`: `abilities` registered in the pool.
    #[serde(skip)]
    pub ability_ids: AbilitySet,
    /// Filled in by `CardPool::add`: `copy_except` interned and registered.
    #[serde(skip)]
    pub except_subtype_ids: SubtypeSet,
    #[serde(skip)]
    pub except_ability_ids: AbilitySet,
    #[serde(skip)]
    pub except_flags: DefFlags,
    /// "As an additional cost to cast this spell, ..." (CR 601.2b, 118.8):
    /// sacrifice, discard, or life paid alongside the mana cost.
    pub additional: Cost,
    /// Kicker (CR 702.33): an optional additional mana cost chosen as the
    /// spell is cast.
    pub kicker: Option<Box<ManaCost>>,
    /// What this spell's own text takes off its cost (affinity).
    pub self_cost: Option<SelfCost>,
    /// "This spell costs {3} less to cast if it targets a tapped
    /// creature" (negative) or "{2} more ... a Dragon": generic mana, and
    /// what the first target must match.
    pub target_cost: Option<Box<(i32, ObjFilter)>>,
    /// "This spell costs {3} less to cast if a creature died this turn":
    /// generic mana, and the pool's condition index.
    pub cond_cost: Option<(i32, u16)>,
    /// Buyback (CR 702.27): an optional additional cost; if paid, the
    /// spell goes to its owner's hand as it resolves.
    pub buyback: Option<Box<ManaCost>>,
    /// Morph, megamorph, or disguise (CR 702.37, 702.168): cast face down
    /// for {3}, turned face up for this cost.
    pub morph: Option<Box<(ManaCost, MorphKind)>>,
    /// Evoke (CR 702.74): an alternative cost; an evoked permanent is
    /// sacrificed as it enters.
    pub evoke: Option<Box<ManaCost>>,
    /// Dash (CR 702.109): an alternative cost; a dashed permanent has
    /// haste and returns to its owner's hand at the next end step.
    pub dash: Option<Box<ManaCost>>,
    /// Offspring (CR 702.175): an optional additional cost; if paid, a 1/1
    /// token copy is created as the permanent enters.
    pub offspring: Option<Box<ManaCost>>,
    /// Bargain (CR 702.166): "You may sacrifice an artifact, enchantment,
    /// or token as you cast this spell."
    pub bargain: bool,
    /// Madness (CR 702.35): cast for this cost as it's discarded.
    pub madness: Option<Box<ManaCost>>,
    /// Foretell (CR 702.143): exiled from hand for {2}, cast later for
    /// this cost.
    pub foretell: Option<Box<ManaCost>>,
    /// Suspend N—cost (CR 702.62).
    pub suspend: Option<Box<(u32, ManaCost)>>,
    /// Entwine (CR 702.42): every mode of a "choose one" spell for this
    /// cost on top.
    pub entwine: Option<Box<ManaCost>>,
    /// Escape—cost, exile N other cards from your graveyard (CR 702.138).
    pub escape: Option<Box<(ManaCost, u32)>>,
    /// Modular N (CR 702.43): enters with N +1/+1 counters (its dies
    /// trigger is an ordinary ability).
    pub modular: u32,
    /// Flashback (CR 702.34): the spell may be cast from the graveyard for
    /// this cost instead of its mana cost, and is exiled as it leaves the
    /// stack.
    pub flashback: Option<Box<Cost>>,
    /// Event kinds any of this definition's triggers (keyword triggers
    /// included) can fire on; see `TriggerSpec::watch_mask`.
    watch_mask: u16,
    /// The same for the abilities a copy exception adds.
    except_watch_mask: u16,
    /// Filled in by `CardPool::add`: the printed characteristics, ready to
    /// clone (CR 109.3 before any effect).
    #[serde(skip)]
    pub printed: Option<crate::state::Characteristics>,
    /// Filled in by `CardPool::add`; see `cast_targets()`.
    #[serde(skip)]
    cast_targets: Vec<TargetSpec>,
    /// Filled in by `CardPool::add`: facts the engine checks on hot paths.
    #[serde(skip)]
    flags: DefFlags,
    /// Filled in by `CardPool::add`: a static ability whose filter looks at
    /// attacking or blocking ("attacking creatures get +1/+0"), so combat
    /// changes must refresh the characteristics cache.
    #[serde(skip)]
    pub combat_static: bool,
    /// Filled in by `CardPool::add`: a static that counts cards in hand
    /// (Empyrial Armor), so hand changes refresh the cache.
    #[serde(skip)]
    pub hand_static: bool,
    /// Filled in by `CardPool::add`: a static whose condition names a part
    /// of the turn, so step changes refresh the cache.
    #[serde(skip)]
    pub step_static: bool,
    /// Filled in by `CardPool::add`: what its text asks about the mana
    /// spent to cast it.
    #[serde(skip)]
    pub spent: SpentUse,
}

/// Precomputed facts about a definition (see `CardDef::has_triggers` and
/// friends).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct DefFlags(u32);

impl DefFlags {
    const TRIGGERS: u32 = 1;
    const REPLACEMENTS: u32 = 2;
    const MODIFIES_ENTERING: u32 = 4;
    const ACTIVATED: u32 = 8;
    const CONTROL_STATIC: u32 = 16;
    const EQUIPMENT: u32 = 32;
    const MID_CHOICE: u32 = 64;
    /// A characteristic-defining ability that reads a life total (Ajani's
    /// Avatar): life changes must then invalidate the characteristics cache.
    const LIFE_CDA: u32 = 128;
    const CDA: u32 = 256;
    /// A static ability that changes what spells cost (CR 601.2f).
    const COST_CHANGE: u32 = 512;
    /// "Can't attack unless ..." (`AbilityDef::AttackUnless`).
    const ATTACK_RULE: u32 = 1024;
    /// A static ability that grants an ability ("Enchanted creature has
    /// '{T}: ...'"), so other objects may have abilities their definitions
    /// lack.
    const GRANTS: u32 = 2048;
    /// A static ability whose condition looks at tapped permanents ("as
    /// long as you control no untapped lands"): tapping must then
    /// invalidate the characteristics cache.
    const TAP_STATIC: u32 = 4096;
    /// "As this enters, choose ..." (`AbilityDef::AsEntersChoose`): a
    /// question as the permanent enters, however it does.
    const ENTERS_CHOICE: u32 = 8192;
    /// A mana ability whose mana is restricted (`AbilityDef::RestrictedMana`):
    /// payment then looks for such sources.
    const RESTRICTED_MANA: u32 = 16384;
    /// A static ability that reads this turn's history or step
    /// (`CardDef::hand_static`, `step_static`): casts and step changes then
    /// look for such permanents.
    const TURN_STATIC: u32 = 32768;
    /// An activated mana ability (CR 605.1a) that payment may activate
    /// (`AbilityDef::Activated` adding mana, not `TapForMana`): payment
    /// then looks for such abilities when plain sources fall short.
    const MANA_ABILITY: u32 = 65536;
    /// A condition on the mana spent to cast the spell that triggered an
    /// ability (`StaticCondition::ManaSpent` with `that`): casts then
    /// record what was spent.
    const SPENT_WATCH: u32 = 131_072;
    /// `MODIFIES_ENTERING`, for callers outside this module.
    pub const MODIFIES_ENTERING_FLAG: u32 = DefFlags::MODIFIES_ENTERING;

    pub const fn has(self, f: u32) -> bool {
        self.0 & f != 0
    }

    fn set(&mut self, f: u32, on: bool) {
        if on {
            self.0 |= f;
        }
    }

    #[must_use]
    pub const fn union(self, other: DefFlags) -> DefFlags {
        DefFlags(self.0 | other.0)
    }

    pub const fn has_triggers(self) -> bool {
        self.has(DefFlags::TRIGGERS)
    }

    pub const fn has_replacements(self) -> bool {
        self.has(DefFlags::REPLACEMENTS)
    }

    pub const fn has_activated(self) -> bool {
        self.has(DefFlags::ACTIVATED)
    }

    pub const fn has_control_static(self) -> bool {
        self.has(DefFlags::CONTROL_STATIC)
    }

    pub const fn has_mid_choice(self) -> bool {
        self.has(DefFlags::MID_CHOICE)
    }

    pub const fn has_enters_choice(self) -> bool {
        self.has(DefFlags::ENTERS_CHOICE)
    }

    pub const fn has_cda(self) -> bool {
        self.has(DefFlags::CDA)
    }

    pub const fn has_life_cda(self) -> bool {
        self.has(DefFlags::LIFE_CDA)
    }

    pub const fn has_turn_static(self) -> bool {
        self.has(DefFlags::TURN_STATIC)
    }

    pub const fn has_tap_static(self) -> bool {
        self.has(DefFlags::TAP_STATIC)
    }

    pub const fn has_cost_change(self) -> bool {
        self.has(DefFlags::COST_CHANGE)
    }

    pub const fn has_attack_rule(self) -> bool {
        self.has(DefFlags::ATTACK_RULE)
    }

    pub const fn grants_abilities(self) -> bool {
        self.has(DefFlags::GRANTS)
    }

    pub const fn has_restricted_mana(self) -> bool {
        self.has(DefFlags::RESTRICTED_MANA)
    }

    pub const fn has_mana_ability(self) -> bool {
        self.has(DefFlags::MANA_ABILITY)
    }

    pub const fn watches_spent(self) -> bool {
        self.has(DefFlags::SPENT_WATCH)
    }

    /// Flags describing a list of abilities (and trigger keywords).
    #[allow(clippy::too_many_lines)] // one flag per hot-path fact
    fn of(abilities: &[AbilityDef], keywords: KeywordSet) -> DefFlags {
        let mut flags = DefFlags::default();
        flags.set(
            DefFlags::TRIGGERS,
            keywords.intersects(KeywordSet::TRIGGERED)
                || abilities
                    .iter()
                    .any(|a| matches!(a, AbilityDef::Triggered(_))),
        );
        flags.set(
            DefFlags::REPLACEMENTS,
            abilities.iter().any(
                |a| matches!(a, AbilityDef::Replacement(r) if !matches!(r.replaces, Replaces::Enters)),
            ),
        );
        flags.set(
            DefFlags::MODIFIES_ENTERING,
            abilities.iter().any(
                |a| matches!(a, AbilityDef::Replacement(r) if matches!(r.replaces, Replaces::Enters)),
            ),
        );
        flags.set(
            DefFlags::ACTIVATED,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::Activated(_))),
        );
        flags.set(
            DefFlags::CONTROL_STATIC,
            abilities.iter().any(
                |a| matches!(a, AbilityDef::Static(s) if s.parts.contains(&Modification::Control)),
            ),
        );
        flags.set(
            DefFlags::CDA,
            abilities.iter().any(
                |a| matches!(a, AbilityDef::Static(s) if s.parts.iter().any(Modification::is_cda)),
            ),
        );
        flags.set(
            DefFlags::LIFE_CDA,
            abilities.iter().any(|a| {
                matches!(a, AbilityDef::Static(s) if s.parts.iter().any(|p| matches!(
                    p,
                    Modification::CdaPt { count: CdaCount::ControllerLife, .. }
                        | Modification::ModifyPtPer { count: CountExpr::Life, .. }
                )) || matches!(s.condition, Some(
                    StaticCondition::OpponentLifeAtMost(_)
                        | StaticCondition::YourLife { .. }
                        | StaticCondition::AnyPlayerLifeAtMost(_)
                        | StaticCondition::OpponentPoison(_)
                )))
            }),
        );
        flags.set(
            DefFlags::GRANTS,
            abilities.iter().any(|a| {
                matches!(a, AbilityDef::Static(s) if s.parts.iter().any(|m| matches!(m, Modification::AddAbility(_))))
            }),
        );
        flags.set(
            DefFlags::ATTACK_RULE,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::AttackUnless { .. })),
        );
        flags.set(
            DefFlags::TAP_STATIC,
            abilities.iter().any(|a| {
                matches!(a, AbilityDef::Static(s) if matches!(&s.condition,
                    Some(StaticCondition::YouControlNone(f)
                        | StaticCondition::SourceIs(f)
                        | StaticCondition::AttachedIs(f)) if f.tapped.is_some()))
            }),
        );
        flags.set(
            DefFlags::COST_CHANGE,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::CostChange(_))),
        );
        flags.set(
            DefFlags::MID_CHOICE,
            abilities.iter().any(|a| {
                matches!(
                    a,
                    AbilityDef::EntersAsCopy(_) | AbilityDef::AsEntersChoose(_)
                ) || a.all_effects().iter().any(|e| e.asks())
            }),
        );
        flags.set(
            DefFlags::MANA_ABILITY,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::Activated(ab) if ab.is_mana_ability())),
        );
        flags.set(
            DefFlags::RESTRICTED_MANA,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::RestrictedMana { .. })),
        );
        flags.set(
            DefFlags::ENTERS_CHOICE,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::AsEntersChoose(_))),
        );
        flags
    }
}

impl CardDef {
    /// Printed color (CR 202.2): the color indicator if any, else the mana
    /// cost's colors; none for devoid (CR 702.114a).
    pub fn colors(&self) -> ColorSet {
        if self.keywords.contains(Keyword::Devoid) {
            return ColorSet::default();
        }
        self.color_indicator
            .unwrap_or_else(|| self.mana_cost.colors())
    }

    pub fn is_aura(&self) -> bool {
        self.enchant().is_some()
    }

    /// The Aura's enchant requirement, if it has one.
    pub fn enchant(&self) -> Option<TargetSpec> {
        self.abilities.iter().find_map(|a| match a {
            AbilityDef::Enchant(t) => Some(*t),
            _ => None,
        })
    }

    /// Targets chosen when casting this card (CR 601.2c): a spell's
    /// targets, or an Aura's enchant target.
    pub fn cast_targets(&self) -> &[TargetSpec] {
        &self.cast_targets
    }

    /// Whether the card does something from its owner's graveyard
    /// (flashback, unearth): such cards are counted per player
    /// (`flashback_in_graveyard`) so listing options skips the graveyard
    /// otherwise.
    pub fn graveyard_active(&self) -> bool {
        self.flashback.is_some()
            || self.escape.is_some()
            || self
                .abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::Activated(ab) if ab.from_graveyard))
    }

    pub fn has_static(&self) -> bool {
        self.abilities
            .iter()
            .any(|a| matches!(a, AbilityDef::Static(_)))
    }

    /// Has a printed triggered ability or a keyword that is one. Checked on
    /// every event, so precomputed.
    pub fn has_triggers(&self) -> bool {
        self.flags.has(DefFlags::TRIGGERS)
    }

    /// Has a printed replacement or prevention ability that works from the
    /// battlefield. Checked on every replaceable event, so precomputed.
    pub fn has_replacements(&self) -> bool {
        self.flags.has(DefFlags::REPLACEMENTS)
    }

    /// Has a printed "enters with" / "enters tapped" ability (CR 614.1c).
    pub fn modifies_entering(&self) -> bool {
        self.flags.has(DefFlags::MODIFIES_ENTERING)
    }

    /// Has a printed activated ability other than a mana ability.
    pub fn has_activated(&self) -> bool {
        self.flags.has(DefFlags::ACTIVATED)
    }

    /// Has a printed static ability that changes control of something
    /// (Control Magic).
    pub fn has_control_static(&self) -> bool {
        self.flags.has(DefFlags::CONTROL_STATIC)
    }

    /// Has the Equipment subtype (CR 301.5).
    pub fn is_equipment(&self) -> bool {
        self.flags.has(DefFlags::EQUIPMENT)
    }

    /// Resolving this (or an ability of it) may ask its controller a
    /// question in the middle of resolution: "you may", or which object
    /// to enter as a copy of.
    pub fn has_mid_choice(&self) -> bool {
        self.flags.has(DefFlags::MID_CHOICE)
    }

    /// Has a characteristic-defining ability (CR 604.3).
    pub fn has_cda(&self) -> bool {
        self.flags.has(DefFlags::CDA)
    }

    /// The precomputed flags of the printed abilities.
    pub fn flags(&self) -> DefFlags {
        self.flags
    }

    /// The modes of a "choose one" spell, if it is one.
    pub fn modes(&self) -> Option<&[Mode]> {
        self.abilities.iter().find_map(|a| match a {
            AbilityDef::Modal(m) => Some(m.as_slice()),
            _ => None,
        })
    }

    /// What an "As this enters, choose ..." ability chooses, if it has
    /// one.
    pub fn enters_choice(&self) -> Option<ChoiceKind> {
        if !self.flags.has(DefFlags::ENTERS_CHOICE) {
            return None;
        }
        self.abilities.iter().find_map(|a| match a {
            AbilityDef::AsEntersChoose(k) => Some(*k),
            _ => None,
        })
    }

    /// The filter of an "enters as a copy" ability, if it has one.
    pub fn enters_as_copy(&self) -> Option<&ObjFilter> {
        self.abilities.iter().find_map(|a| match a {
            AbilityDef::EntersAsCopy(f) => Some(f),
            _ => None,
        })
    }

    /// Each way of casting this as an instant or sorcery: the single spell
    /// ability, or one entry per mode.
    pub fn spell_variants(&self) -> Vec<(&[TargetSpec], &[Effect])> {
        if let Some(modes) = self.modes() {
            return modes
                .iter()
                .map(|m| (m.targets.as_slice(), m.effects.as_slice()))
                .collect();
        }
        self.spell_ability().into_iter().collect()
    }

    /// An Aura or Equipment: attaches to another permanent.
    pub fn attaches(&self) -> bool {
        self.is_aura() || self.is_equipment()
    }

    pub fn is_legendary(&self) -> bool {
        self.supertypes.contains(Supertype::Legendary)
    }

    /// Whether any printed triggered ability (or trigger keyword) could see
    /// `event`, by event kind. Current characteristics decide for real.
    pub fn may_watch(&self, event: &crate::event::Event) -> bool {
        self.watch_mask & event.watch_bit() != 0
    }

    /// The same for the abilities this card's copy effects add (CR 707.9b).
    pub fn except_may_watch(&self, event: &crate::event::Event) -> bool {
        self.except_watch_mask & event.watch_bit() != 0
    }

    pub fn is_creature(&self) -> bool {
        self.types.contains(CardType::Creature)
    }

    pub fn is_land(&self) -> bool {
        self.types.contains(CardType::Land)
    }

    pub fn is_instant(&self) -> bool {
        self.types.contains(CardType::Instant)
    }

    pub fn is_sorcery(&self) -> bool {
        self.types.contains(CardType::Sorcery)
    }

    pub fn is_permanent_card(&self) -> bool {
        self.types.is_permanent_type()
    }

    pub fn spell_ability(&self) -> Option<(&[TargetSpec], &[Effect])> {
        self.abilities.iter().find_map(|a| match a {
            AbilityDef::Spell { targets, effects } => {
                Some((targets.as_slice(), effects.as_slice()))
            }
            _ => None,
        })
    }
}

/// All card definitions a game can use, with name lookup, plus the interned
/// subtypes and the registered abilities they refer to.
#[derive(Clone, Debug)]
pub struct CardPool {
    defs: Vec<CardDef>,
    by_name: HashMap<String, CardRef>,
    subtypes: Vec<String>,
    subtype_ids: HashMap<String, Subtype>,
    abilities: Vec<AbilityDef>,
    /// Filters behind `TargetSpec::Filtered`.
    filters: Vec<ObjFilter>,
    /// Specs behind `TargetSpec::UpTo`.
    up_to: Vec<TargetSpec>,
    /// Groups behind `TargetSpec::Many`.
    many: Vec<ManyTargets>,
    /// Conditions behind `Replacement::EnterTappedUnless`,
    /// `AbilityDef::AttackUnless`, and `Condition::Static`.
    conditions: Vec<StaticCondition>,
    /// Per ability: whether resolving it may ask its controller a
    /// question ("you may").
    asks: Vec<bool>,
    /// Per ability: whether resolving it may put a permanent onto the
    /// battlefield whose own entering replacement could meet another.
    enters: Vec<bool>,
    /// The triggered ability prowess stands for (CR 702.108a).
    prowess: AbilityId,
    /// Exalted, persist, and undying (CR 702.83a, 702.79a, 702.93a).
    exalted: AbilityId,
    persist: AbilityId,
    undying: AbilityId,
    flanking: AbilityId,
    evoke: AbilityId,
    exploit: AbilityId,
    builtins: [AbilityId; 5],
    /// A face-down permanent's or spell's characteristics (CR 708.2a): a
    /// nameless, colorless 2/2 creature with no abilities; and the same
    /// with ward {2}, for disguise (CR 702.168b).
    face_down: CardRef,
    face_down_ward: CardRef,
}

impl Default for CardPool {
    fn default() -> Self {
        Self::new()
    }
}

impl CardPool {
    #[allow(clippy::too_many_lines)] // the built-in definitions
    pub fn new() -> Self {
        let mut pool = CardPool {
            defs: Vec::new(),
            by_name: HashMap::new(),
            subtypes: Vec::new(),
            subtype_ids: HashMap::new(),
            abilities: Vec::new(),
            filters: Vec::new(),
            up_to: Vec::new(),
            many: Vec::new(),
            conditions: Vec::new(),
            asks: Vec::new(),
            enters: Vec::new(),
            prowess: AbilityId(0),
            exalted: AbilityId(0),
            persist: AbilityId(0),
            undying: AbilityId(0),
            flanking: AbilityId(0),
            evoke: AbilityId(0),
            exploit: AbilityId(0),
            builtins: [AbilityId(0); 5],
            face_down: CardRef(0),
            face_down_ward: CardRef(0),
        };
        for c in Color::ALL {
            let id = pool.subtype(c.basic_land_type());
            debug_assert_eq!(id, Subtype::basic(c));
        }
        for name in NONCREATURE_SUBTYPES {
            pool.subtype(name);
        }
        debug_assert_eq!(pool.subtype("Equipment"), Subtype::EQUIPMENT);
        debug_assert_eq!(pool.subtype("Aura"), Subtype::AURA);
        pool.prowess = pool.register(AbilityDef::Triggered(TriggeredAbility {
            trigger: TriggerSpec::CastsSpell {
                whose: Who::You,
                noncreature: true,
                instant_or_sorcery: false,
            },
            condition: None,
            targets: Vec::new(),
            effects: vec![Effect::Pump {
                power: 1,
                toughness: 1,
                to: Recipient::Source,
            }],
        }));
        pool.exalted = pool.register(AbilityDef::Triggered(TriggeredAbility {
            trigger: TriggerSpec::AttacksAlone(TriggerObj::Filter(
                ObjFilter::creatures().who(Who::You),
            )),
            condition: None,
            targets: Vec::new(),
            effects: vec![Effect::Pump {
                power: 1,
                toughness: 1,
                to: Recipient::TriggeringObject,
            }],
        }));
        // CR 702.25a: "Whenever a creature without flanking blocks this
        // creature, the blocking creature gets -1/-1 until end of turn."
        pool.flanking = pool.register(AbilityDef::Triggered(TriggeredAbility {
            trigger: TriggerSpec::BlockedBy(ObjFilter {
                not_keywords: KeywordSet::NONE.with(Keyword::Flanking),
                ..ObjFilter::creatures()
            }),
            condition: None,
            targets: Vec::new(),
            effects: vec![Effect::Pump {
                power: -1,
                toughness: -1,
                to: Recipient::TriggeringObject,
            }],
        }));
        // CR 702.74a: "When this permanent enters, if its evoke cost was
        // paid, its controller sacrifices it."
        pool.evoke = pool.register(AbilityDef::Triggered(TriggeredAbility {
            trigger: TriggerSpec::Enters(TriggerObj::This),
            condition: Some(Condition::Evoked),
            targets: Vec::new(),
            effects: vec![Effect::Sacrifice {
                what: Recipient::Source,
            }],
        }));
        pool.exploit = pool.register(AbilityDef::Triggered(TriggeredAbility {
            trigger: TriggerSpec::Enters(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: vec![Effect::Exploit],
        }));
        for (i, effect) in [
            Effect::CastExiled(ExileCast::Madness),
            Effect::SuspendTick,
            Effect::CastExiled(ExileCast::Free),
            Effect::Cascade,
        ]
        .into_iter()
        .enumerate()
        {
            let trigger = if i == 2 {
                // Delayed: matched at the next upkeep.
                TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::Upkeep,
                    whose: Who::You,
                }
            } else {
                TriggerSpec::Enters(TriggerObj::This)
            };
            pool.builtins[i] = pool.register(AbilityDef::Triggered(TriggeredAbility {
                trigger,
                condition: None,
                targets: Vec::new(),
                effects: vec![effect],
            }));
        }
        pool.builtins[Builtin::DashReturn as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                trigger: TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::End,
                    whose: Who::Any,
                },
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::ReturnToHand {
                    what: Recipient::SameObject,
                }],
            }));
        // Named only for logs: a face-down permanent has no name.
        pool.face_down = pool.add(
            Builder::new("face-down creature", "")
                .types([CardType::Creature])
                .pt(2, 2)
                .build(),
        );
        let ward = AbilityDef::Triggered(TriggeredAbility {
            trigger: TriggerSpec::TargetedByOpponent,
            condition: None,
            targets: Vec::new(),
            effects: vec![Effect::CounterUnlessPays {
                to: Recipient::TriggeringObject,
                mana: ManaCost::parse("{2}").expect("a mana cost"),
                x: Num::N(0),
                dest: CounteredTo::Graveyard,
            }],
        });
        pool.face_down_ward = pool.add(
            Builder::new("face-down creature", "")
                .types([CardType::Creature])
                .pt(2, 2)
                .ability(ward)
                .build(),
        );
        for (kind, slot) in [
            (CounterKind::MinusOneMinusOne, 0),
            (CounterKind::PlusOnePlusOne, 1),
        ] {
            let id = pool.register(AbilityDef::Triggered(TriggeredAbility {
                trigger: TriggerSpec::Dies(TriggerObj::This),
                condition: Some(Condition::HadNoCounters(kind)),
                targets: Vec::new(),
                effects: vec![Effect::ReturnToBattlefield {
                    what: Recipient::Source,
                    counters: Some((kind, 1)),
                    tapped: false,
                    under_you: false,
                }],
            }));
            if slot == 0 {
                pool.persist = id;
            } else {
                pool.undying = id;
            }
        }
        pool
    }

    fn register(&mut self, a: AbilityDef) -> AbilityId {
        let id = AbilityId(u32::try_from(self.abilities.len()).expect("too many abilities"));
        let effects = a.all_effects();
        self.asks.push(effects.iter().any(|e| e.asks()));
        // What a token is known; what returns from exile or is copied is not.
        self.enters.push(effects.iter().any(|e| {
            match e {
                Effect::ReturnToBattlefield { .. } | Effect::CreateTokenCopy { .. } => true,
                Effect::CreateToken { token, .. } => self
                    .defs
                    .get(token.0 as usize)
                    .is_none_or(CardDef::modifies_entering),
                _ => false,
            }
        }));
        self.abilities.push(a);
        id
    }

    /// Whether resolving this ability may put onto the battlefield a
    /// permanent with its own entering replacement (CR 614.1c), which could
    /// meet another replacement effect and need an order chosen (CR 616.1).
    pub fn ability_enters(&self, id: AbilityId) -> bool {
        self.enters[id.0 as usize]
    }

    /// Whether resolving this ability may ask its controller a question
    /// in the middle of resolution ("you may").
    pub fn ability_asks(&self, id: AbilityId) -> bool {
        self.asks[id.0 as usize]
    }

    /// Register an ability that is not printed on a card: a delayed
    /// triggered ability an effect creates (CR 603.7).
    pub fn register_ability(&mut self, a: AbilityDef) -> AbilityId {
        self.register(a)
    }

    /// Intern a filter for `TargetSpec::Filtered`.
    pub fn intern_filter(&mut self, f: ObjFilter) -> u16 {
        if let Some(i) = self.filters.iter().position(|x| *x == f) {
            return u16::try_from(i).expect("few filters");
        }
        self.filters.push(f);
        u16::try_from(self.filters.len() - 1).expect("too many target filters")
    }

    pub fn filter(&self, i: u16) -> &ObjFilter {
        &self.filters[i as usize]
    }

    /// Intern an optional target spec for `TargetSpec::UpTo`.
    pub fn intern_up_to(&mut self, spec: TargetSpec) -> TargetSpec {
        let i = if let Some(i) = self.up_to.iter().position(|x| *x == spec) {
            i
        } else {
            self.up_to.push(spec);
            self.up_to.len() - 1
        };
        TargetSpec::UpTo(u16::try_from(i).expect("few optional specs"))
    }

    /// Intern a variable target group for `TargetSpec::Many`.
    pub fn intern_many(&mut self, m: ManyTargets) -> TargetSpec {
        let i = if let Some(i) = self.many.iter().position(|x| *x == m) {
            i
        } else {
            self.many.push(m);
            self.many.len() - 1
        };
        TargetSpec::Many(u16::try_from(i).expect("few target groups"))
    }

    /// The variable target group behind `TargetSpec::Many(i)`.
    pub fn many(&self, i: u16) -> ManyTargets {
        self.many[i as usize]
    }

    /// Intern a condition for `Replacement::EnterTappedUnless` and friends.
    pub fn intern_condition(&mut self, c: StaticCondition) -> u16 {
        let i = if let Some(i) = self.conditions.iter().position(|x| *x == c) {
            i
        } else {
            self.conditions.push(c);
            self.conditions.len() - 1
        };
        u16::try_from(i).expect("few conditions")
    }

    pub fn condition(&self, i: u16) -> &StaticCondition {
        &self.conditions[i as usize]
    }

    /// Whether these abilities ask about the mana spent to cast the spell
    /// that triggered them.
    fn watches_spent(&self, abilities: &[AbilityDef]) -> bool {
        let that = |i: u16| {
            matches!(
                self.condition(i),
                StaticCondition::ManaSpent { that: true, .. }
            )
        };
        abilities.iter().any(|a| {
            let AbilityDef::Triggered(t) = a else {
                return false;
            };
            matches!(t.condition, Some(Condition::Static(i)) if that(i))
                || a.all_effects().iter().any(|e| {
                    matches!(
                        e,
                        Effect::IfCondition(i, _)
                            | Effect::Instead {
                                cond: EffectCond::Static(i),
                                ..
                            } if that(*i)
                    )
                })
        })
    }

    /// What `def`'s text asks about the mana spent to cast it (see
    /// `SpentUse`): its intervening "if"s, its "If {B} was spent ..."
    /// sentences, and its converge and Treasure counts.
    fn spent_use(&self, def: &CardDef) -> SpentUse {
        let mut out = SpentUse::default();
        let mut want = |c: &StaticCondition| {
            if let StaticCondition::ManaSpent {
                kind,
                at_least,
                that: false,
            } = *c
            {
                out.record = true;
                match kind {
                    SpentKind::Color(col) => {
                        let w = &mut out.colors[col as usize];
                        *w = (*w).max(at_least);
                    }
                    SpentKind::Treasure => out.treasure = out.treasure.max(1),
                    SpentKind::Total => {}
                }
            }
        };
        for a in &def.abilities {
            if let AbilityDef::Triggered(t) = a
                && let Some(Condition::Static(i)) = t.condition
            {
                want(self.condition(i));
            }
            for e in a.all_effects() {
                match e {
                    Effect::IfCondition(i, _)
                    | Effect::Instead {
                        cond: EffectCond::Static(i),
                        ..
                    } => want(self.condition(*i)),
                    _ => {}
                }
            }
        }
        for c in &def.counts {
            match c {
                CountExpr::SpentColors => {
                    out.record = true;
                    out.converge = true;
                }
                CountExpr::SpentTreasure => {
                    out.record = true;
                    out.treasure = u8::MAX;
                }
                _ => {}
            }
        }
        out
    }

    /// The spec an optional target must meet; other specs are themselves.
    pub fn base_spec(&self, spec: TargetSpec) -> TargetSpec {
        match spec {
            TargetSpec::UpTo(i) => self.up_to[i as usize],
            TargetSpec::Many(i) => self.many[i as usize].spec,
            s => s,
        }
    }

    /// Every interned filter, by index.
    pub fn filters(&self) -> &[ObjFilter] {
        &self.filters
    }

    /// The definition a face-down object has (CR 708.2a); with ward {2}
    /// for disguise.
    pub fn face_down(&self, ward: bool) -> CardRef {
        if ward {
            self.face_down_ward
        } else {
            self.face_down
        }
    }

    /// A built-in triggered ability (madness, suspend, rebound, cascade).
    pub const fn builtin(&self, b: Builtin) -> AbilityId {
        self.builtins[b as usize]
    }

    /// Whether a definition is a face-down one.
    pub fn is_face_down(&self, def: CardRef) -> bool {
        def == self.face_down || def == self.face_down_ward
    }

    /// The triggered ability a keyword stands for, for keywords that are
    /// triggered abilities (CR 702.108 prowess).
    pub fn keyword_trigger(&self, k: Keyword) -> Option<AbilityId> {
        match k {
            Keyword::Prowess => Some(self.prowess),
            Keyword::Exalted => Some(self.exalted),
            Keyword::Persist => Some(self.persist),
            Keyword::Undying => Some(self.undying),
            Keyword::Flanking => Some(self.flanking),
            Keyword::Evoke => Some(self.evoke),
            Keyword::Exploit => Some(self.exploit),
            _ => None,
        }
    }

    pub fn add(&mut self, mut def: CardDef) -> CardRef {
        let r = CardRef(u32::try_from(self.defs.len()).expect("card pool too large"));
        def.subtype_ids = def
            .subtypes
            .clone()
            .iter()
            .map(|s| self.subtype(s))
            .collect();
        def.ability_ids = def
            .abilities
            .clone()
            .into_iter()
            .map(|a| self.register(a))
            .collect();
        if let Some(x) = def.copy_except.clone() {
            def.except_subtype_ids = x.subtypes.iter().map(|s| self.subtype(s)).collect();
            def.except_ability_ids = x.abilities.into_iter().map(|a| self.register(a)).collect();
            def.except_flags = DefFlags::of(
                def.copy_except
                    .as_ref()
                    .map_or(&[], |x| x.abilities.as_slice()),
                x.keywords,
            );
        }
        let mask = |abilities: &[AbilityDef]| {
            abilities.iter().fold(0u16, |m, a| match a {
                AbilityDef::Triggered(t) => m | t.trigger.watch_mask(),
                _ => m,
            })
        };
        def.watch_mask = mask(&def.abilities)
            | def.keywords.iter().fold(0u16, |m, k| {
                m | self
                    .keyword_trigger(k)
                    .map_or(0, |id| match self.ability(id) {
                        AbilityDef::Triggered(t) => t.trigger.watch_mask(),
                        _ => 0,
                    })
            });
        def.except_watch_mask = def.copy_except.as_ref().map_or(0, |x| mask(&x.abilities));
        def.combat_static = def.abilities.iter().any(|a| {
            matches!(a, AbilityDef::Static(sa) if sa.affected.attacking
            || sa.affected.blocking
            || matches!(sa.condition, Some(
                StaticCondition::SourceAttacking
                    | StaticCondition::SourceAttackingAlone
                    | StaticCondition::SourceBlocked
                    | StaticCondition::SourceBeingBlocked
            )))
        });
        def.hand_static = def.abilities.iter().any(|a| {
            matches!(a, AbilityDef::Static(sa) if sa.parts.iter().any(|m| matches!(
                m,
                Modification::ModifyPtPer { count: CountExpr::CardsInHand, .. }
            )) || matches!(sa.condition, Some(
                StaticCondition::CardsInHand { .. }
                    | StaticCondition::MoreCardsInHand
                    | StaticCondition::DrawnThisTurn(_)
                    | StaticCondition::CastThisTurn(_)
                    | StaticCondition::DiscardedThisTurn
            )))
        });
        def.step_static = def.abilities.iter().any(|a| {
            matches!(a, AbilityDef::Static(sa) if matches!(sa.condition, Some(StaticCondition::During(_))))
        });
        def.spent = self.spent_use(&def);
        def.cast_targets = match def.spell_ability() {
            Some((t, _)) => t.to_vec(),
            None => def.enchant().into_iter().collect(),
        };
        let mut flags = DefFlags::of(&def.abilities, def.keywords);
        flags.set(
            DefFlags::EQUIPMENT,
            def.subtypes.iter().any(|s| s == "Equipment"),
        );
        flags.set(DefFlags::TURN_STATIC, def.hand_static || def.step_static);
        flags.set(DefFlags::SPENT_WATCH, self.watches_spent(&def.abilities));
        def.flags = flags;
        def.printed = Some(crate::state::Characteristics {
            def: r,
            colors: def.colors(),
            supertypes: def.supertypes,
            types: def.types,
            subtypes: def.subtype_ids,
            power: crate::state::pt16(def.power.unwrap_or(0)),
            toughness: crate::state::pt16(def.toughness.unwrap_or(0)),
            keywords: def.keywords,
            abilities: def.ability_ids,
        });
        self.by_name.insert(def.name.clone(), r);
        self.defs.push(def);
        r
    }

    /// Make `r` findable by `key` as well as its name.
    pub fn alias(&mut self, key: &str, r: CardRef) {
        self.by_name.insert(key.to_string(), r);
    }

    pub fn get(&self, r: CardRef) -> &CardDef {
        &self.defs[r.0 as usize]
    }

    pub fn lookup(&self, name: &str) -> Option<CardRef> {
        self.by_name.get(name).copied()
    }

    /// Link a definition to its other faces after both are in the pool (a
    /// double-faced card's faces name each other).
    pub fn set_faces(&mut self, r: CardRef, layout: Layout, faces: Vec<CardRef>) {
        let d = &mut self.defs[r.0 as usize];
        d.layout = layout;
        d.faces = faces;
    }

    /// Intern a subtype name.
    pub fn subtype(&mut self, name: &str) -> Subtype {
        if let Some(id) = self.subtype_ids.get(name) {
            return *id;
        }
        let id = Subtype(u16::try_from(self.subtypes.len()).expect("too many subtypes"));
        self.subtypes.push(name.to_string());
        self.subtype_ids.insert(name.to_string(), id);
        id
    }

    pub fn subtype_lookup(&self, name: &str) -> Option<Subtype> {
        self.subtype_ids.get(name).copied()
    }

    /// How many subtypes are interned (`Subtype(0)` up to this).
    pub fn subtype_count(&self) -> usize {
        self.subtypes.len()
    }

    pub fn subtype_name(&self, s: Subtype) -> &str {
        self.subtypes.get(s.0 as usize).map_or("?", String::as_str)
    }

    pub fn ability(&self, id: AbilityId) -> &AbilityDef {
        &self.abilities[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (CardRef, &CardDef)> {
        self.defs
            .iter()
            .enumerate()
            .map(|(i, d)| (CardRef(u32::try_from(i).unwrap_or(u32::MAX)), d))
    }
}

/// Builder for M1 card definitions.
pub struct Builder(CardDef);

impl Builder {
    pub fn new(name: &str, cost: &str) -> Self {
        Builder(CardDef {
            hand_static: false,
            spent: SpentUse::default(),
            step_static: false,
            counts: Vec::new(),
            combat_static: false,
            layout: Layout::Normal,
            faces: Vec::new(),
            name: name.to_string(),
            mana_cost: ManaCost::parse(cost).unwrap_or_else(|e| panic!("{name}: {e}")),
            color_indicator: None,
            supertypes: SupertypeSet::default(),
            types: TypeSet::default(),
            subtypes: Vec::new(),
            power: None,
            toughness: None,
            keywords: KeywordSet::NONE,
            abilities: Vec::new(),
            oracle_text: String::new(),
            token: false,
            emblem: false,
            loyalty: 0,
            copy_except: None,
            subtype_ids: SubtypeSet::new(),
            ability_ids: AbilitySet::new(),
            except_subtype_ids: SubtypeSet::new(),
            except_ability_ids: AbilitySet::new(),
            except_flags: DefFlags::default(),
            additional: Cost::default(),
            kicker: None,
            self_cost: None,
            buyback: None,
            morph: None,
            evoke: None,
            dash: None,
            offspring: None,
            bargain: false,
            target_cost: None,
            cond_cost: None,
            madness: None,
            foretell: None,
            suspend: None,
            entwine: None,
            escape: None,
            modular: 0,
            flashback: None,
            watch_mask: 0,
            except_watch_mask: 0,
            printed: None,
            cast_targets: Vec::new(),
            flags: DefFlags::default(),
        })
    }

    /// What this card's copy effects add to what they copy (CR 707.9b).
    #[must_use]
    pub fn copy_except(mut self, x: CopyExcept) -> Self {
        self.0.copy_except = Some(x);
        self
    }

    /// Mark this as a token definition (CR 111).
    #[must_use]
    pub fn token(mut self) -> Self {
        self.0.token = true;
        self
    }

    /// The card's variable amounts (`Num::Count`).
    #[must_use]
    pub fn counts(mut self, counts: Vec<CountExpr>) -> Self {
        self.0.counts = counts;
        self
    }

    /// The card's layout and its other faces (already in the pool).
    #[must_use]
    pub fn faces(mut self, layout: Layout, faces: Vec<CardRef>) -> Self {
        self.0.layout = layout;
        self.0.faces = faces;
        self
    }

    #[must_use]
    pub fn emblem(mut self) -> Self {
        self.0.emblem = true;
        self
    }

    #[must_use]
    pub fn loyalty(mut self, n: u32) -> Self {
        self.0.loyalty = n;
        self
    }

    #[must_use]
    pub fn color_indicator(mut self, c: impl IntoIterator<Item = Color>) -> Self {
        self.0.color_indicator = Some(ColorSet::of(c));
        self
    }

    #[must_use]
    pub fn types(mut self, types: impl IntoIterator<Item = CardType>) -> Self {
        self.0.types = TypeSet::of(types);
        self
    }

    #[must_use]
    pub fn supertypes(mut self, s: impl IntoIterator<Item = Supertype>) -> Self {
        self.0.supertypes = SupertypeSet::of(s);
        self
    }

    #[must_use]
    pub fn subtypes(mut self, s: &[&str]) -> Self {
        self.0.subtypes = s.iter().map(|x| (*x).to_string()).collect();
        self
    }

    #[must_use]
    pub fn pt(mut self, power: i32, toughness: i32) -> Self {
        self.0.power = Some(power);
        self.0.toughness = Some(toughness);
        self
    }

    #[must_use]
    pub fn keywords(mut self, k: impl IntoIterator<Item = Keyword>) -> Self {
        self.0.keywords = KeywordSet::of(k);
        self
    }

    #[must_use]
    pub fn additional_cost(mut self, cost: Cost) -> Self {
        self.0.additional = cost;
        self
    }

    /// Kicker (CR 702.33).
    #[must_use]
    pub fn kicker(mut self, cost: ManaCost) -> Self {
        self.0.kicker = Some(Box::new(cost));
        self
    }

    /// Morph, megamorph, or disguise (CR 702.37, 702.168).
    #[must_use]
    pub fn morph(mut self, cost: ManaCost, kind: MorphKind) -> Self {
        self.0.morph = Some(Box::new((cost, kind)));
        self
    }

    /// Evoke (CR 702.74).
    #[must_use]
    pub fn evoke(mut self, cost: ManaCost) -> Self {
        self.0.evoke = Some(Box::new(cost));
        self
    }

    /// Dash (CR 702.109).
    #[must_use]
    pub fn dash(mut self, cost: ManaCost) -> Self {
        self.0.dash = Some(Box::new(cost));
        self
    }

    /// Offspring (CR 702.175).
    #[must_use]
    pub fn offspring(mut self, cost: ManaCost) -> Self {
        self.0.offspring = Some(Box::new(cost));
        self
    }

    /// Bargain (CR 702.166).
    #[must_use]
    pub fn bargain(mut self) -> Self {
        self.0.bargain = true;
        self
    }

    /// A cost change by a condition (see `CardDef::cond_cost`).
    #[must_use]
    pub fn cond_cost(mut self, delta: i32, cond: u16) -> Self {
        self.0.cond_cost = Some((delta, cond));
        self
    }

    /// A cost change by the first target (see `CardDef::target_cost`).
    #[must_use]
    pub fn target_cost(mut self, delta: i32, filter: ObjFilter) -> Self {
        self.0.target_cost = Some(Box::new((delta, filter)));
        self
    }

    /// Madness (CR 702.35).
    #[must_use]
    pub fn madness(mut self, cost: ManaCost) -> Self {
        self.0.madness = Some(Box::new(cost));
        self
    }

    /// Foretell (CR 702.143).
    #[must_use]
    pub fn foretell(mut self, cost: ManaCost) -> Self {
        self.0.foretell = Some(Box::new(cost));
        self
    }

    /// Modular N's entering counters (CR 702.43).
    #[must_use]
    pub fn modular(mut self, n: u32) -> Self {
        self.0.modular = n;
        self
    }

    /// Entwine (CR 702.42).
    #[must_use]
    pub fn entwine(mut self, cost: ManaCost) -> Self {
        self.0.entwine = Some(Box::new(cost));
        self
    }

    /// Escape (CR 702.138).
    #[must_use]
    pub fn escape(mut self, cost: ManaCost, exile: u32) -> Self {
        self.0.escape = Some(Box::new((cost, exile)));
        self
    }

    /// Suspend N—cost (CR 702.62).
    #[must_use]
    pub fn suspend(mut self, n: u32, cost: ManaCost) -> Self {
        self.0.suspend = Some(Box::new((n, cost)));
        self
    }

    /// Buyback (CR 702.27).
    #[must_use]
    pub fn buyback(mut self, cost: ManaCost) -> Self {
        self.0.buyback = Some(Box::new(cost));
        self
    }

    /// "This spell costs {1} less to cast for each ..." (affinity).
    #[must_use]
    pub fn self_cost(mut self, c: SelfCost) -> Self {
        self.0.self_cost = Some(c);
        self
    }

    /// Flashback (CR 702.34).
    #[must_use]
    pub fn flashback(mut self, cost: Cost) -> Self {
        self.0.flashback = Some(Box::new(cost));
        self
    }

    #[must_use]
    pub fn ability(mut self, a: AbilityDef) -> Self {
        self.0.abilities.push(a);
        self
    }

    #[must_use]
    pub fn text(mut self, t: &str) -> Self {
        self.0.oracle_text = t.to_string();
        self
    }

    pub fn build(self) -> CardDef {
        self.0
    }
}

/// A small pool for this crate's own tests: basic lands, a few vanilla and
/// keyword creatures, and two spells. The real corpus is the DSL card files
/// compiled by `mtg_dsl::builtin_pool`.
#[cfg(test)]
pub(crate) fn test_pool() -> CardPool {
    use CardType::{Creature, Instant, Land};
    let mut pool = CardPool::new();
    for c in Color::ALL {
        pool.add(
            Builder::new(c.basic_land_type(), "")
                .supertypes([Supertype::Basic])
                .types([Land])
                .subtypes(&[c.basic_land_type()])
                .build(),
        );
    }
    let creature = |name: &str, cost: &str, sub: &[&str], p: i32, t: i32, kw: &[Keyword]| {
        Builder::new(name, cost)
            .types([Creature])
            .subtypes(sub)
            .pt(p, t)
            .keywords(kw.iter().copied())
            .build()
    };
    pool.add(creature("Grizzly Bears", "{1}{G}", &["Bear"], 2, 2, &[]));
    pool.add(creature(
        "Raging Goblin",
        "{R}",
        &["Goblin", "Berserker"],
        1,
        1,
        &[Keyword::Haste],
    ));
    pool.add(creature(
        "Vampire Nighthawk",
        "{1}{B}{B}",
        &["Vampire", "Shaman"],
        2,
        3,
        &[Keyword::Flying, Keyword::Deathtouch, Keyword::Lifelink],
    ));
    pool.add(
        Builder::new("Lightning Bolt", "{R}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Any],
                effects: vec![Effect::DealDamage {
                    amount: Num::N(3),
                    to: Recipient::Target(0),
                }],
            })
            .text("Lightning Bolt deals 3 damage to any target.")
            .build(),
    );
    pool.add(
        Builder::new("Giant Growth", "{G}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Creature],
                effects: vec![Effect::Pump {
                    power: 3,
                    toughness: 3,
                    to: Recipient::Target(0),
                }],
            })
            .text("Target creature gets +3/+3 until end of turn.")
            .build(),
    );
    pool
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_lookup_and_characteristics() {
        let pool = test_pool();
        let bears = pool.get(pool.lookup("Grizzly Bears").unwrap());
        assert!(bears.is_creature());
        assert_eq!((bears.power, bears.toughness), (Some(2), Some(2)));
        assert_eq!(bears.colors(), ColorSet::of([Color::Green]));
        assert_eq!(bears.mana_cost.mana_value(), 2);

        let forest = pool.get(pool.lookup("Forest").unwrap());
        assert!(forest.is_land());
        assert!(forest.supertypes.contains(Supertype::Basic));
        assert!(forest.colors().is_colorless());

        let nighthawk = pool.get(pool.lookup("Vampire Nighthawk").unwrap());
        assert!(nighthawk.keywords.contains(Keyword::Deathtouch));
        assert!(nighthawk.keywords.contains(Keyword::Lifelink));
        assert!(!nighthawk.keywords.contains(Keyword::Haste));

        let bolt = pool.get(pool.lookup("Lightning Bolt").unwrap());
        let (targets, effects) = bolt.spell_ability().unwrap();
        assert_eq!(targets, &[TargetSpec::Any]);
        assert_eq!(effects.len(), 1);
        assert!(pool.lookup("Black Lotus").is_none());
    }

    #[test]
    fn type_sets() {
        let t = TypeSet::of([CardType::Creature, CardType::Artifact]);
        assert!(t.contains(CardType::Creature));
        assert!(!t.contains(CardType::Land));
        assert!(t.is_permanent_type());
        assert!(!TypeSet::of([CardType::Instant]).is_permanent_type());
        assert_eq!(format!("{t:?}"), "Artifact Creature");
    }
}
