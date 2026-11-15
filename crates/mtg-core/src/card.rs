//! Printed card definitions (CR 200–212): the immutable characteristics a
//! card has in the card database, before any effect applies. In M1 the
//! definitions are Rust data; M3 replaces this with the compiled DSL.

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
    Reach,
    Trample,
    Vigilance,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub struct KeywordSet(u32);

impl KeywordSet {
    pub const NONE: KeywordSet = KeywordSet(0);

    pub fn of(keywords: impl IntoIterator<Item = Keyword>) -> Self {
        keywords.into_iter().fold(KeywordSet(0), KeywordSet::with)
    }

    #[must_use]
    pub const fn with(self, k: Keyword) -> Self {
        KeywordSet(self.0 | (1 << (k as u32)))
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
        self.0 & (1 << (k as u32)) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Keyword> {
        Keyword::ALL.into_iter().filter(move |k| self.contains(*k))
    }
}

impl Keyword {
    pub const ALL: [Keyword; 14] = [
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
        Keyword::Reach,
        Keyword::Trample,
        Keyword::Vigilance,
    ];
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

    /// The color a basic land type produces (CR 305.6), if this is one.
    pub fn basic_color(self) -> Option<Color> {
        Color::ALL.get(self.0 as usize).copied()
    }
}

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

/// Which battlefield objects a static ability's effect applies to
/// (CR 611.3: re-evaluated as the layers are applied).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
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
    /// Must have none of these subtypes ("non-Aura").
    pub not_subtypes: SubtypeSet,
    /// Excludes the effect's source ("each other ...").
    pub other: bool,
    /// Only the object the source is attached to ("enchanted creature").
    pub enchanted: bool,
    /// Only objects with no abilities.
    pub no_abilities: bool,
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
    pub fn other(mut self) -> Self {
        self.other = true;
        self
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
    /// "is white in addition to its other colors" (layer 5).
    AddColors(ColorSet),
    /// "is blue" (layer 5).
    SetColors(ColorSet),
    /// "has flying" (layer 6).
    AddKeywords(KeywordSet),
    /// "loses all abilities" (layer 6).
    RemoveAllAbilities,
    /// "has base power and toughness 1/1" (layer 7b).
    SetPt { power: PtValue, toughness: PtValue },
    /// "+1/+1" (layer 7c).
    ModifyPt { power: i32, toughness: i32 },
    /// "switch power and toughness" (layer 7d).
    SwitchPt,
}

impl Modification {
    pub const fn layer(&self) -> Layer {
        match self {
            Modification::AddTypes(_)
            | Modification::AddLandType(_)
            | Modification::SetLandType(_) => Layer::L4Type,
            Modification::AddColors(_) | Modification::SetColors(_) => Layer::L5Color,
            Modification::AddKeywords(_) | Modification::RemoveAllAbilities => Layer::L6Ability,
            Modification::SetPt { .. } => Layer::L7bSetPt,
            Modification::ModifyPt { .. } => Layer::L7cModifyPt,
            Modification::SwitchPt => Layer::L7dSwitchPt,
        }
    }
}

/// A static ability (CR 604) that generates a continuous effect while its
/// source is on the battlefield (CR 611.3).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct StaticAbility {
    pub affected: ObjFilter,
    pub parts: Vec<Modification>,
}

/// Kinds of counters (CR 122).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CounterKind {
    PlusOnePlusOne,
    MinusOneMinusOne,
}

/// What a spell's target must be. One entry per "target" word in the text.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum TargetSpec {
    /// "any target": a creature, player, planeswalker, or battle (CR 115.4).
    Any,
    Creature,
    /// "target player or planeswalker".
    PlayerOrPlaneswalker,
    Player,
}

/// Who an effect applies to when it does not say "target".
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum PlayerRef {
    /// The spell's or ability's controller ("you").
    You,
}

/// Effects implemented in M1. `target` fields index into the ability's
/// `targets` list.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Effect {
    DealDamage {
        amount: u32,
        target: usize,
    },
    /// +N/+N until end of turn (CR 611.2a; layer 7c).
    Pump {
        power: i32,
        toughness: i32,
        target: usize,
    },
    Destroy {
        target: usize,
    },
    GainLife {
        amount: u32,
        player: PlayerRef,
    },
    DrawCards {
        count: u32,
        player: PlayerRef,
    },
    /// Put counters on the target (CR 122.6).
    AddCounters {
        kind: CounterKind,
        count: u32,
        target: usize,
    },
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum AbilityDef {
    /// The effect of an instant or sorcery spell when it resolves.
    Spell {
        targets: Vec<TargetSpec>,
        effects: Vec<Effect>,
    },
    /// "Enchant <spec>" (CR 702.5): an Aura's targeting requirement and
    /// attachment legality.
    Enchant(TargetSpec),
    /// "{T}: Add <mana>" (CR 605). Basic land types grant theirs
    /// intrinsically (CR 305.6) and do not list this.
    TapForMana(Mana),
    Static(StaticAbility),
}

/// Printed characteristics plus abilities (CR 109.3, 200.1).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct CardDef {
    pub name: String,
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
    /// Filled in by `CardPool::add`: `subtypes` interned.
    #[serde(skip)]
    pub subtype_ids: SubtypeSet,
    /// Filled in by `CardPool::add`: `abilities` registered in the pool.
    #[serde(skip)]
    pub ability_ids: AbilitySet,
    /// Filled in by `CardPool::add`: the printed characteristics, ready to
    /// clone (CR 109.3 before any effect).
    #[serde(skip)]
    pub printed: Option<crate::state::Characteristics>,
    /// Filled in by `CardPool::add`; see `cast_targets()`.
    #[serde(skip)]
    cast_targets: Vec<TargetSpec>,
}

impl CardDef {
    /// Printed color (CR 202.2): the color indicator if any, else the mana
    /// cost's colors.
    pub fn colors(&self) -> ColorSet {
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
        };
        for c in Color::ALL {
            let id = pool.subtype(c.basic_land_type());
            debug_assert_eq!(id, Subtype::basic(c));
        }
        pool
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
            .iter()
            .map(|a| {
                let id =
                    AbilityId(u32::try_from(self.abilities.len()).expect("too many abilities"));
                self.abilities.push(a.clone());
                id
            })
            .collect();
        def.cast_targets = match def.spell_ability() {
            Some((t, _)) => t.to_vec(),
            None => def.enchant().into_iter().collect(),
        };
        def.printed = Some(crate::state::Characteristics {
            def: r,
            colors: def.colors(),
            supertypes: def.supertypes,
            types: def.types,
            subtypes: def.subtype_ids,
            power: def.power.unwrap_or(0),
            toughness: def.toughness.unwrap_or(0),
            keywords: def.keywords,
            abilities: def.ability_ids,
        });
        self.by_name.insert(def.name.clone(), r);
        self.defs.push(def);
        r
    }

    pub fn get(&self, r: CardRef) -> &CardDef {
        &self.defs[r.0 as usize]
    }

    pub fn lookup(&self, name: &str) -> Option<CardRef> {
        self.by_name.get(name).copied()
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
            subtype_ids: SubtypeSet::new(),
            ability_ids: AbilitySet::new(),
            printed: None,
            cast_targets: Vec::new(),
        })
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

/// The M1 card pool: basic lands, vanilla and keyword creatures, and a few
/// simple spells. Enough to exercise every part of the core loop.
#[allow(clippy::too_many_lines)]
pub fn m1_pool() -> CardPool {
    use CardType::{Creature, Instant, Land, Sorcery};
    use Keyword::{
        Deathtouch, Defender, DoubleStrike, FirstStrike, Flying, Haste, Lifelink, Menace, Reach,
        Trample, Vigilance,
    };

    let mut pool = CardPool::new();
    for c in Color::ALL {
        pool.add(
            Builder::new(c.basic_land_type(), "")
                .supertypes([Supertype::Basic])
                .types([Land])
                .subtypes(&[c.basic_land_type()])
                .text(&format!("({{T}}: Add {{{}}}.)", c.letter()))
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
    pool.add(creature("Savannah Lions", "{W}", &["Cat"], 2, 1, &[]));
    pool.add(creature("Grizzly Bears", "{1}{G}", &["Bear"], 2, 2, &[]));
    pool.add(creature(
        "Raging Goblin",
        "{R}",
        &["Goblin", "Berserker"],
        1,
        1,
        &[Haste],
    ));
    pool.add(creature(
        "Wind Drake",
        "{2}{U}",
        &["Drake"],
        2,
        2,
        &[Flying],
    ));
    pool.add(creature("Hill Giant", "{3}{R}", &["Giant"], 3, 3, &[]));
    pool.add(creature(
        "Giant Spider",
        "{3}{G}",
        &["Spider"],
        2,
        4,
        &[Reach],
    ));
    pool.add(creature(
        "Trained Armodon",
        "{1}{G}{G}",
        &["Elephant"],
        3,
        3,
        &[],
    ));
    pool.add(creature(
        "Typhoid Rats",
        "{1}{B}",
        &["Rat"],
        1,
        1,
        &[Deathtouch],
    ));
    pool.add(creature(
        "Vampire Nighthawk",
        "{1}{B}{B}",
        &["Vampire", "Shaman"],
        2,
        3,
        &[Flying, Deathtouch, Lifelink],
    ));
    pool.add(creature(
        "Youthful Knight",
        "{1}{W}",
        &["Human", "Knight"],
        2,
        1,
        &[FirstStrike],
    ));
    pool.add(creature(
        "Fencing Ace",
        "{1}{W}",
        &["Human", "Soldier"],
        1,
        1,
        &[DoubleStrike],
    ));
    pool.add(creature(
        "Wall of Stone",
        "{1}{R}{R}",
        &["Wall"],
        0,
        8,
        &[Defender],
    ));
    pool.add(creature(
        "Boggart Brute",
        "{2}{R}",
        &["Goblin", "Warrior"],
        3,
        2,
        &[Menace],
    ));
    pool.add(creature(
        "Serra Angel",
        "{3}{W}{W}",
        &["Angel"],
        4,
        4,
        &[Flying, Vigilance],
    ));
    pool.add(creature(
        "Air Elemental",
        "{3}{U}{U}",
        &["Elemental"],
        4,
        4,
        &[Flying],
    ));
    pool.add(creature("Craw Wurm", "{4}{G}{G}", &["Wurm"], 6, 4, &[]));
    pool.add(creature(
        "Colossal Dreadmaw",
        "{4}{G}{G}",
        &["Dinosaur"],
        6,
        6,
        &[Trample],
    ));

    pool.add(
        Builder::new("Lightning Bolt", "{R}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Any],
                effects: vec![Effect::DealDamage {
                    amount: 3,
                    target: 0,
                }],
            })
            .text("Lightning Bolt deals 3 damage to any target.")
            .build(),
    );
    pool.add(
        Builder::new("Shock", "{R}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Any],
                effects: vec![Effect::DealDamage {
                    amount: 2,
                    target: 0,
                }],
            })
            .text("Shock deals 2 damage to any target.")
            .build(),
    );
    pool.add(
        Builder::new("Lava Axe", "{4}{R}")
            .types([Sorcery])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::PlayerOrPlaneswalker],
                effects: vec![Effect::DealDamage {
                    amount: 5,
                    target: 0,
                }],
            })
            .text("Lava Axe deals 5 damage to target player or planeswalker.")
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
                    target: 0,
                }],
            })
            .text("Target creature gets +3/+3 until end of turn.")
            .build(),
    );
    pool.add(
        Builder::new("Murder", "{1}{B}{B}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Creature],
                effects: vec![Effect::Destroy { target: 0 }],
            })
            .text("Destroy target creature.")
            .build(),
    );
    pool.add(
        Builder::new("Divination", "{2}{U}")
            .types([Sorcery])
            .ability(AbilityDef::Spell {
                targets: vec![],
                effects: vec![Effect::DrawCards {
                    count: 2,
                    player: PlayerRef::You,
                }],
            })
            .text("Draw two cards.")
            .build(),
    );
    pool.add(
        Builder::new("Healing Salve", "{W}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![],
                effects: vec![Effect::GainLife {
                    amount: 3,
                    player: PlayerRef::You,
                }],
            })
            .text("You gain 3 life.")
            .build(),
    );
    add_m2_cards(&mut pool);
    pool
}

/// Cards with static abilities, Auras, and counters for the layer system
/// (M2). All are real cards with simple Oracle text.
#[allow(clippy::too_many_lines)]
fn add_m2_cards(pool: &mut CardPool) {
    use CardType::{Artifact, Creature, Enchantment, Instant, Land};
    use Modification::{
        AddKeywords, AddLandType, AddTypes, ModifyPt, RemoveAllAbilities, SetLandType, SetPt,
    };

    let aura_subtype = pool.subtype("Aura");
    let enchantment =
        |name: &str, cost: &str, text: &str, affected: ObjFilter, parts: &[Modification]| {
            Builder::new(name, cost)
                .types([Enchantment])
                .ability(AbilityDef::Static(StaticAbility {
                    affected,
                    parts: parts.to_vec(),
                }))
                .text(text)
                .build()
        };
    let aura = |name: &str, cost: &str, text: &str, parts: &[Modification]| {
        Builder::new(name, cost)
            .types([Enchantment])
            .subtypes(&["Aura"])
            .ability(AbilityDef::Enchant(TargetSpec::Creature))
            .ability(AbilityDef::Static(StaticAbility {
                affected: ObjFilter::enchanted(),
                parts: parts.to_vec(),
            }))
            .text(text)
            .build()
    };
    let pump = |p: i32, t: i32| ModifyPt {
        power: p,
        toughness: t,
    };

    pool.add(enchantment(
        "Glorious Anthem",
        "{1}{W}{W}",
        "Creatures you control get +1/+1.",
        ObjFilter::creatures().who(Who::You),
        &[pump(1, 1)],
    ));
    pool.add(enchantment(
        "Crusade",
        "{W}{W}",
        "White creatures get +1/+1.",
        ObjFilter::creatures().colors(ColorSet::of([Color::White])),
        &[pump(1, 1)],
    ));
    pool.add(enchantment(
        "Humility",
        "{2}{W}{W}",
        "All creatures lose all abilities and have base power and toughness 1/1.",
        ObjFilter::creatures(),
        &[
            RemoveAllAbilities,
            SetPt {
                power: PtValue::Fixed(1),
                toughness: PtValue::Fixed(1),
            },
        ],
    ));
    pool.add(enchantment(
        "Opalescence",
        "{2}{W}{W}",
        "Each other non-Aura enchantment is a creature in addition to its other types and has base power and toughness each equal to its mana value.",
        ObjFilter {
            types: TypeSet::of([Enchantment]),
            not_subtypes: SubtypeSet::from_slice(&[aura_subtype]),
            other: true,
            ..Default::default()
        },
        &[
            AddTypes(TypeSet::of([Creature])),
            SetPt {
                power: PtValue::ManaValue,
                toughness: PtValue::ManaValue,
            },
        ],
    ));
    pool.add(enchantment(
        "Muraganda Petroglyphs",
        "{3}{G}",
        "Creatures with no abilities get +2/+2.",
        ObjFilter {
            types: TypeSet::of([Creature]),
            no_abilities: true,
            ..Default::default()
        },
        &[pump(2, 2)],
    ));
    pool.add(enchantment(
        "Levitation",
        "{2}{U}{U}",
        "Creatures you control have flying.",
        ObjFilter::creatures().who(Who::You),
        &[AddKeywords(KeywordSet::of([Keyword::Flying]))],
    ));
    pool.add(enchantment(
        "Blood Moon",
        "{2}{R}",
        "Nonbasic lands are Mountains.",
        ObjFilter {
            types: TypeSet::of([Land]),
            not_supertypes: SupertypeSet::of([Supertype::Basic]),
            ..Default::default()
        },
        &[SetLandType(Color::Red)],
    ));
    pool.add(
        Builder::new("Urborg, Tomb of Yawgmoth", "")
            .supertypes([Supertype::Legendary])
            .types([Land])
            .ability(AbilityDef::Static(StaticAbility {
                affected: ObjFilter {
                    types: TypeSet::of([Land]),
                    ..Default::default()
                },
                parts: vec![AddLandType(Color::Black)],
            }))
            .text("Each land is a Swamp in addition to its other land types.")
            .build(),
    );
    pool.add(
        Builder::new("Darksteel Citadel", "")
            .types([Artifact, Land])
            .keywords([Keyword::Indestructible])
            .ability(AbilityDef::TapForMana(Mana::Colorless))
            .text("Indestructible\n{T}: Add {C}.")
            .build(),
    );
    pool.add(
        Builder::new("Dryad Arbor", "")
            .color_indicator([Color::Green])
            .types([Land, Creature])
            .subtypes(&["Forest", "Dryad"])
            .pt(1, 1)
            .text("(Dryad Arbor isn't a spell, it's affected by summoning sickness, and it has \"{T}: Add {G}.\")")
            .build(),
    );

    pool.add(aura(
        "Holy Strength",
        "{W}",
        "Enchant creature\nEnchanted creature gets +1/+2.",
        &[pump(1, 2)],
    ));
    pool.add(aura(
        "Unholy Strength",
        "{B}",
        "Enchant creature\nEnchanted creature gets +2/+1.",
        &[pump(2, 1)],
    ));
    pool.add(aura(
        "Dead Weight",
        "{B}",
        "Enchant creature\nEnchanted creature gets -2/-2.",
        &[pump(-2, -2)],
    ));
    pool.add(aura(
        "Flight",
        "{U}",
        "Enchant creature\nEnchanted creature has flying.",
        &[AddKeywords(KeywordSet::of([Keyword::Flying]))],
    ));

    pool.add(
        Builder::new("Dromoka's Gift", "{4}{G}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Creature],
                effects: vec![Effect::AddCounters {
                    kind: CounterKind::PlusOnePlusOne,
                    count: 3,
                    target: 0,
                }],
            })
            .text("Put three +1/+1 counters on target creature.")
            .build(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_lookup_and_characteristics() {
        let pool = m1_pool();
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
