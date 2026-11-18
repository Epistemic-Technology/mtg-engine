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
        .with(Keyword::Undying);

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
    pub const ALL: [Keyword; 45] = [
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
        self.tapped.is_some() || self.attacking || self.blocking || self.mana_value.is_some()
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
    /// The effect's controller controls the object (layer 2, CR 613.1b).
    /// Applied by `GameState::sync_control`, not through characteristics.
    Control,
    /// "is white in addition to its other colors" (layer 5).
    AddColors(ColorSet),
    /// "is blue" (layer 5).
    SetColors(ColorSet),
    /// "has flying" (layer 6).
    AddKeywords(KeywordSet),
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
            | Modification::AddSubtype(_) => Layer::L4Type,
            Modification::AddColors(_)
            | Modification::SetColors(_)
            | Modification::CdaColors(_) => Layer::L5Color,
            Modification::AddKeywords(_)
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
    /// The `n`th target's power.
    TargetPower(usize),
    /// Your life total.
    Life,
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
}

/// Kinds of counters (CR 122).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CounterKind {
    PlusOnePlusOne,
    MinusOneMinusOne,
    /// Loyalty counters on a planeswalker (CR 306.5).
    Loyalty,
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
    /// "target opponent".
    Opponent,
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
    /// The spell's or ability's controller ("you").
    You,
    /// Every opponent of the controller.
    EachOpponent,
    /// The ability's source ("this creature", "~"); for an ability of an
    /// object that changed zones, the object it became (CR 400.7).
    Source,
    /// The object whose action triggered the ability ("that creature").
    TriggeringObject,
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
}

impl Num {
    /// The value without game state: a count is 0 here (see
    /// `GameState::eval_num`).
    pub const fn eval(self, x: u32) -> u32 {
        match self {
            Num::N(n) => n,
            Num::X => x,
            Num::Count(_) => 0,
        }
    }

    /// A value for heuristics that have no X in hand.
    pub const fn hint(self) -> u32 {
        match self {
            Num::N(n) => n,
            Num::X | Num::Count(_) => 3,
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
    /// Counter a spell (CR 701.5): it goes to its owner's graveyard.
    Counter {
        to: Recipient,
    },
    /// "Counter target spell unless its controller pays {N}" (CR 701.5?):
    /// the controller may pay; if they do not, it is countered.
    CounterUnlessPays {
        to: Recipient,
        mana: ManaCost,
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
    /// "Put target creature on top of its owner's library" / "on the
    /// bottom".
    PutInLibrary {
        what: Recipient,
        top: bool,
    },
    /// "Each opponent sacrifices a creature of their choice" (an edict):
    /// each player chooses `count` permanents matching the filter.
    SacrificeChosen {
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
    /// "discards a card at random".
    DiscardRandom {
        count: Num,
        who: Recipient,
    },
    /// "You choose a nonland card from it. That player discards that
    /// card.": the effect's controller picks from the player's hand.
    DiscardChosen {
        who: Recipient,
        filter: Option<ObjFilter>,
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
    /// "has base power and toughness N/N until end of turn" (layer 7b).
    SetBasePt {
        to: Recipient,
        power: i32,
        toughness: i32,
    },
    /// Create a delayed triggered ability (CR 603.7) about the recipient:
    /// `ability` is a registered triggered ability (see
    /// `CardPool::register_ability`) whose effects see the recipient as
    /// `Recipient::SameObject`.
    Delayed {
        ability: AbilityId,
        about: Recipient,
    },
    /// Exile a permanent.
    Exile {
        what: Recipient,
    },
    /// Return a card from exile or a graveyard to the battlefield under its
    /// owner's control, optionally with counters on it.
    ReturnToBattlefield {
        what: Recipient,
        counters: Option<(CounterKind, u32)>,
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
                | Effect::UnlessPays { .. }
                | Effect::Scry { .. }
                | Effect::Surveil { .. }
                | Effect::Discard { .. }
                | Effect::CounterUnlessPays { .. }
                | Effect::Search { .. }
                | Effect::AddMana(_)
                | Effect::AddManaOneOf(_)
                | Effect::AddManaTimes { .. }
                | Effect::SacrificeChosen { .. }
                | Effect::DiscardRandom { .. }
                | Effect::DiscardChosen { .. }
        )
    }
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
}

/// The event a replacement or prevention effect watches for (CR 614.1,
/// 615.1).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Replaces {
    /// Damage a source matching `from` (any source if `None`) would deal to
    /// `to`; `combat` restricts to combat or noncombat damage.
    Damage {
        from: Option<ObjFilter>,
        to: Affects,
        combat: Option<bool>,
    },
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
        ) && matches!(self.replaces, Replaces::Damage { .. })
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
pub const WATCH_ZONE_CHANGE: u8 = 1;
pub const WATCH_STEP_BEGIN: u8 = 2;
pub const WATCH_ATTACK: u8 = 4;
pub const WATCH_BLOCK: u8 = 8;
pub const WATCH_DAMAGE: u8 = 16;
pub const WATCH_LIFE_CHANGE: u8 = 32;
pub const WATCH_CAST: u8 = 64;
pub const WATCH_TARGETED: u8 = 128;

/// Trigger conditions (CR 603.1). Each names the event and, where the
/// ability's text restricts it, which objects or players it watches.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum TriggerSpec {
    /// "When ~ enters" / "Whenever another creature enters".
    Enters(TriggerObj),
    /// "When ~ dies" / "Whenever a creature you control dies" (CR 700.4);
    /// also "is put into a graveyard from the battlefield" for
    /// noncreature permanents. A leaves-the-battlefield trigger: it looks
    /// back in time (CR 603.10).
    Dies(TriggerObj),
    /// "At the beginning of [whose] <step>".
    BeginningOfStep { step: crate::turn::Step, whose: Who },
    /// "Whenever ~ attacks" / "Whenever a creature you control attacks".
    Attacks(TriggerObj),
    /// "Whenever ~ blocks".
    Blocks(TriggerObj),
    /// "Whenever a creature you control attacks alone" (exalted, CR
    /// 506.5).
    AttacksAlone(TriggerObj),
    /// "Whenever this creature becomes blocked" (once per combat).
    BecomesBlocked(TriggerObj),
    /// "Whenever this creature blocks or becomes blocked".
    BlocksOrBlocked(TriggerObj),
    /// "Whenever ~ deals combat damage to a player".
    DealsCombatDamageToPlayer(TriggerObj),
    /// "Whenever you gain life".
    GainsLife { whose: Who },
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
}

impl TriggerSpec {
    /// Whether this kind of trigger can see `event` at all, judged from the
    /// event's kind alone. A cheap filter before the full check.
    pub fn watches(&self, event: &crate::event::Event) -> bool {
        self.watch_mask() & event.watch_bit() != 0
    }

    /// The kinds of event this trigger can fire on, as bits matching
    /// `Event::watch_bit`. A definition's mask is the union over its
    /// triggers, so "could anything here care about this event" is one
    /// `&` instead of a walk over abilities.
    pub fn watch_mask(&self) -> u8 {
        match self {
            TriggerSpec::Enters(_) | TriggerSpec::Dies(_) => WATCH_ZONE_CHANGE,
            TriggerSpec::BeginningOfStep { .. } => WATCH_STEP_BEGIN,
            TriggerSpec::Attacks(_) | TriggerSpec::AttacksAlone(_) => WATCH_ATTACK,
            TriggerSpec::Blocks(_)
            | TriggerSpec::BecomesBlocked(_)
            | TriggerSpec::BlocksOrBlocked(_) => WATCH_BLOCK,
            TriggerSpec::DealsCombatDamageToPlayer(_) => WATCH_DAMAGE,
            TriggerSpec::GainsLife { .. } => WATCH_LIFE_CHANGE,
            TriggerSpec::CastsSpell { .. } => WATCH_CAST,
            TriggerSpec::BecomesTarget(_) => WATCH_TARGETED,
        }
    }
}

/// An intervening "if" clause (CR 603.4), checked when the ability would
/// trigger and again on resolution.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Condition {
    SourceUntapped,
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
pub struct Cost {
    pub mana: ManaCost,
    /// "Discard this card" (cycling, CR 702.29): the source leaves the hand
    /// as the cost is paid.
    pub discard_self: bool,
    /// "Pay N life" (CR 119.4: payable only with at least that much life).
    pub life: u32,
    /// "Discard N cards" (CR 701.8); the cards are chosen as the cost is paid.
    pub discard: u32,
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
            life: 0,
            discard: 0,
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
        matches!(self.sacrifice, Some(SacCost::Filter(_))) || self.discard > 0
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

/// An activated ability (CR 602): "[cost]: [effect]". Uses the stack like a
/// spell; mana abilities are `AbilityDef::TapForMana` instead (CR 605).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ActivatedAbility {
    pub cost: Cost,
    pub targets: Vec<TargetSpec>,
    pub effects: Vec<Effect>,
    /// "Activate only as a sorcery" (CR 602.5a; equip, CR 702.6a).
    pub sorcery_speed: bool,
    /// "Activate only once each turn."
    pub once_per_turn: bool,
    /// Activated from the hand rather than the battlefield (cycling, CR
    /// 702.29).
    pub from_hand: bool,
    /// The ability's text, for naming the choice.
    pub text: String,
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
                    Effect::AddMana(_) | Effect::AddManaOneOf(_) | Effect::AddManaTimes { .. }
                )
            })
    }
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
    /// "Enchant <spec>" (CR 702.5): an Aura's targeting requirement and
    /// attachment legality.
    Enchant(TargetSpec),
    /// "{T}: Add <mana>" (CR 605). Basic land types grant theirs
    /// intrinsically (CR 305.6) and do not list this.
    TapForMana(Mana),
    /// "{T}, Sacrifice this artifact: Add <mana>" (Treasure, Lotus
    /// Petal): a mana ability that sacrifices its source.
    SacForMana(Mana),
    /// A blocking restriction on this creature (CR 509.1b–c): "can't be
    /// blocked by Walls", "can block only creatures with flying".
    BlockRule(BlockRule),
    /// "Artifact spells you cast cost {1} less to cast" (CR 601.2f).
    CostChange(CostChange),
    /// Protection from what matches the filter ("protection from
    /// artifacts", "from Goblins", "from everything"), judged on a source's
    /// characteristics (CR 702.16); the object also has
    /// `Keyword::ProtectionFromFilter`.
    Protection(ObjFilter),
    /// "You have no maximum hand size" (`delta` None), "Your maximum hand
    /// size is increased by one" (CR 402.2); `whose` from the controller.
    HandSize {
        whose: Who,
        delta: Option<i32>,
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
            | AbilityDef::BlockRule(_)
            | AbilityDef::CostChange(_)
            | AbilityDef::Protection(_)
            | AbilityDef::HandSize { .. }
            | AbilityDef::Static(_)
            | AbilityDef::Replacement(_)
            | AbilityDef::Modal(_)
            | AbilityDef::EntersAsCopy(_) => &[],
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
            | AbilityDef::BlockRule(_)
            | AbilityDef::CostChange(_)
            | AbilityDef::Protection(_)
            | AbilityDef::HandSize { .. }
            | AbilityDef::Static(_)
            | AbilityDef::Replacement(_)
            | AbilityDef::Modal(_)
            | AbilityDef::EntersAsCopy(_) => &[],
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
            | Effect::UnlessPays { effects: inner, .. } = e
            {
                for i in inner {
                    walk(i, out);
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
    pub kicker: Option<ManaCost>,
    /// What this spell's own text takes off its cost (affinity).
    pub self_cost: Option<SelfCost>,
    /// Flashback (CR 702.34): the spell may be cast from the graveyard for
    /// this cost instead of its mana cost, and is exiled as it leaves the
    /// stack.
    pub flashback: Option<Cost>,
    /// Event kinds any of this definition's triggers (keyword triggers
    /// included) can fire on; see `TriggerSpec::watch_mask`.
    watch_mask: u8,
    /// The same for the abilities a copy exception adds.
    except_watch_mask: u8,
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
}

/// Precomputed facts about a definition (see `CardDef::has_triggers` and
/// friends).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct DefFlags(u16);

impl DefFlags {
    const TRIGGERS: u16 = 1;
    const REPLACEMENTS: u16 = 2;
    const MODIFIES_ENTERING: u16 = 4;
    const ACTIVATED: u16 = 8;
    const CONTROL_STATIC: u16 = 16;
    const EQUIPMENT: u16 = 32;
    const MID_CHOICE: u16 = 64;
    /// A characteristic-defining ability that reads a life total (Ajani's
    /// Avatar): life changes must then invalidate the characteristics cache.
    const LIFE_CDA: u16 = 128;
    const CDA: u16 = 256;
    /// A static ability that changes what spells cost (CR 601.2f).
    const COST_CHANGE: u16 = 512;
    /// `MODIFIES_ENTERING`, for callers outside this module.
    pub const MODIFIES_ENTERING_FLAG: u16 = DefFlags::MODIFIES_ENTERING;

    pub const fn has(self, f: u16) -> bool {
        self.0 & f != 0
    }

    fn set(&mut self, f: u16, on: bool) {
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

    pub const fn has_cda(self) -> bool {
        self.has(DefFlags::CDA)
    }

    pub const fn has_life_cda(self) -> bool {
        self.has(DefFlags::LIFE_CDA)
    }

    pub const fn has_cost_change(self) -> bool {
        self.has(DefFlags::COST_CHANGE)
    }

    /// Flags describing a list of abilities (and trigger keywords).
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
                    StaticCondition::OpponentLifeAtMost(_) | StaticCondition::YourLife { .. }
                )))
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
                matches!(a, AbilityDef::EntersAsCopy(_)) || a.all_effects().iter().any(|e| e.asks())
            }),
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
}

impl Default for CardPool {
    fn default() -> Self {
        Self::new()
    }
}

impl CardPool {
    pub fn new() -> Self {
        let mut pool = CardPool {
            defs: Vec::new(),
            by_name: HashMap::new(),
            subtypes: Vec::new(),
            subtype_ids: HashMap::new(),
            abilities: Vec::new(),
            filters: Vec::new(),
            up_to: Vec::new(),
            asks: Vec::new(),
            enters: Vec::new(),
            prowess: AbilityId(0),
            exalted: AbilityId(0),
            persist: AbilityId(0),
            undying: AbilityId(0),
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

    /// The spec an optional target must meet; other specs are themselves.
    pub fn base_spec(&self, spec: TargetSpec) -> TargetSpec {
        match spec {
            TargetSpec::UpTo(i) => self.up_to[i as usize],
            s => s,
        }
    }

    /// Every interned filter, by index.
    pub fn filters(&self) -> &[ObjFilter] {
        &self.filters
    }

    /// The triggered ability a keyword stands for, for keywords that are
    /// triggered abilities (CR 702.108 prowess).
    pub fn keyword_trigger(&self, k: Keyword) -> Option<AbilityId> {
        match k {
            Keyword::Prowess => Some(self.prowess),
            Keyword::Exalted => Some(self.exalted),
            Keyword::Persist => Some(self.persist),
            Keyword::Undying => Some(self.undying),
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
            abilities.iter().fold(0u8, |m, a| match a {
                AbilityDef::Triggered(t) => m | t.trigger.watch_mask(),
                _ => m,
            })
        };
        def.watch_mask = mask(&def.abilities)
            | def.keywords.iter().fold(0u8, |m, k| {
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
                || sa.condition == Some(StaticCondition::SourceAttacking))
        });
        def.hand_static = def.abilities.iter().any(|a| {
            matches!(a, AbilityDef::Static(sa) if sa.parts.iter().any(|m| matches!(
                m,
                Modification::ModifyPtPer { count: CountExpr::CardsInHand, .. }
            )))
        });
        def.cast_targets = match def.spell_ability() {
            Some((t, _)) => t.to_vec(),
            None => def.enchant().into_iter().collect(),
        };
        let mut flags = DefFlags::of(&def.abilities, def.keywords);
        flags.set(
            DefFlags::EQUIPMENT,
            def.subtypes.iter().any(|s| s == "Equipment"),
        );
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
        self.0.kicker = Some(cost);
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
        self.0.flashback = Some(cost);
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
