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
    pub const ALL: [Keyword; 15] = [
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
        if c.subtypes.iter().any(|s| self.not_subtypes.contains(s))
            || self.subtypes.iter().any(|s| !c.subtypes.contains(s))
        {
            return false;
        }
        !self.no_abilities || c.has_no_abilities()
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
    /// "loses all abilities" (layer 6).
    RemoveAllAbilities,
    /// "has base power and toughness 1/1" (layer 7b).
    SetPt { power: PtValue, toughness: PtValue },
    /// "+1/+1" (layer 7c).
    ModifyPt { power: i32, toughness: i32 },
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
            Modification::AddKeywords(_) | Modification::RemoveAllAbilities => Layer::L6Ability,
            Modification::CdaPt { .. } => Layer::L7aCda,
            Modification::SetPt { .. } => Layer::L7bSetPt,
            Modification::ModifyPt { .. } => Layer::L7cModifyPt,
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
}

/// Who or what an effect acts on.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Recipient {
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
}

/// Effects of spells and abilities.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Effect {
    DealDamage {
        amount: u32,
        to: Recipient,
    },
    /// +N/+N until end of turn (CR 611.2a; layer 7c).
    Pump {
        power: i32,
        toughness: i32,
        to: Recipient,
    },
    Destroy {
        to: Recipient,
    },
    GainLife {
        amount: u32,
        who: Recipient,
    },
    LoseLife {
        amount: u32,
        who: Recipient,
    },
    DrawCards {
        count: u32,
        who: Recipient,
    },
    /// Put counters on a permanent (CR 122.6).
    AddCounters {
        kind: CounterKind,
        count: u32,
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
        count: u32,
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
        count: u32,
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
    EnterWithCounters { kind: CounterKind, n: u32 },
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
            TriggerSpec::Attacks(_) => WATCH_ATTACK,
            TriggerSpec::Blocks(_) => WATCH_BLOCK,
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
            tap: false,
            sacrifice: None,
        }
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
    /// The ability's text, for naming the choice.
    pub text: String,
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
            if let Effect::YouMay(inner) = e {
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
    /// A token definition (CR 111): created by effects, never in a deck.
    pub token: bool,
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
}

/// Precomputed facts about a definition (see `CardDef::has_triggers` and
/// friends).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct DefFlags(u8);

impl DefFlags {
    const TRIGGERS: u8 = 1;
    const REPLACEMENTS: u8 = 2;
    const MODIFIES_ENTERING: u8 = 4;
    const ACTIVATED: u8 = 8;
    const CONTROL_STATIC: u8 = 16;
    const EQUIPMENT: u8 = 32;
    const MID_CHOICE: u8 = 64;
    const CDA: u8 = 128;
    /// `MODIFIES_ENTERING`, for callers outside this module.
    pub const MODIFIES_ENTERING_FLAG: u8 = DefFlags::MODIFIES_ENTERING;

    pub const fn has(self, f: u8) -> bool {
        self.0 & f != 0
    }

    fn set(&mut self, f: u8, on: bool) {
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

    /// Flags describing a list of abilities (and trigger keywords).
    fn of(abilities: &[AbilityDef], keywords: KeywordSet) -> DefFlags {
        let mut flags = DefFlags::default();
        flags.set(
            DefFlags::TRIGGERS,
            keywords.contains(Keyword::Prowess)
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
            DefFlags::MID_CHOICE,
            abilities.iter().any(|a| {
                matches!(a, AbilityDef::EntersAsCopy(_))
                    || a.all_effects()
                        .iter()
                        .any(|e| matches!(e, Effect::YouMay(_)))
            }),
        );
        flags
    }
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
    /// Per ability: whether resolving it may ask its controller a
    /// question ("you may").
    asks: Vec<bool>,
    /// Per ability: whether resolving it may put a permanent onto the
    /// battlefield whose own entering replacement could meet another.
    enters: Vec<bool>,
    /// The triggered ability prowess stands for (CR 702.108a).
    prowess: AbilityId,
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
            asks: Vec::new(),
            enters: Vec::new(),
            prowess: AbilityId(0),
        };
        for c in Color::ALL {
            let id = pool.subtype(c.basic_land_type());
            debug_assert_eq!(id, Subtype::basic(c));
        }
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
        pool
    }

    fn register(&mut self, a: AbilityDef) -> AbilityId {
        let id = AbilityId(u32::try_from(self.abilities.len()).expect("too many abilities"));
        let effects = a.all_effects();
        self.asks
            .push(effects.iter().any(|e| matches!(e, Effect::YouMay(_))));
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

    /// The triggered ability a keyword stands for, for keywords that are
    /// triggered abilities (CR 702.108 prowess).
    pub fn keyword_trigger(&self, k: Keyword) -> Option<AbilityId> {
        match k {
            Keyword::Prowess => Some(self.prowess),
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
            token: false,
            copy_except: None,
            subtype_ids: SubtypeSet::new(),
            ability_ids: AbilitySet::new(),
            except_subtype_ids: SubtypeSet::new(),
            except_ability_ids: AbilitySet::new(),
            except_flags: DefFlags::default(),
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
                    to: Recipient::Target(0),
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
                    to: Recipient::Target(0),
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
                    to: Recipient::Target(0),
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
                    to: Recipient::Target(0),
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
                effects: vec![Effect::Destroy {
                    to: Recipient::Target(0),
                }],
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
                    who: Recipient::You,
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
                    who: Recipient::You,
                }],
            })
            .text("You gain 3 life.")
            .build(),
    );
    add_m2_cards(&mut pool);
    add_trigger_cards(&mut pool);
    add_replacement_cards(&mut pool);
    add_activated_cards(&mut pool);
    add_copy_cards(&mut pool);
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
            relation: Relation::Other,
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
                    to: Recipient::Target(0),
                }],
            })
            .text("Put three +1/+1 counters on target creature.")
            .build(),
    );
}

/// Cards with triggered abilities for the event pipeline (M2 slice 2). All
/// are real cards with simple Oracle text.
#[allow(clippy::too_many_lines)]
fn add_trigger_cards(pool: &mut CardPool) {
    use CardType::{Artifact, Creature, Enchantment};
    use Recipient::{DefendingPlayer, EachOpponent, Source, Target, TriggeringPlayer, You};
    use TriggerObj::This;

    let triggered = |trigger: TriggerSpec, targets: &[TargetSpec], effects: &[Effect]| {
        AbilityDef::Triggered(TriggeredAbility {
            trigger,
            condition: None,
            targets: targets.to_vec(),
            effects: effects.to_vec(),
        })
    };
    let creature = |name: &str, cost: &str, sub: &[&str], p: i32, t: i32| {
        Builder::new(name, cost)
            .types([Creature])
            .subtypes(sub)
            .pt(p, t)
    };
    let another_creature = || TriggerObj::Filter(ObjFilter::creatures().other());
    let draw = |who: Recipient| Effect::DrawCards { count: 1, who };
    let gain = |amount: u32, who: Recipient| Effect::GainLife { amount, who };
    let lose = |amount: u32, who: Recipient| Effect::LoseLife { amount, who };
    let damage = |amount: u32, to: Recipient| Effect::DealDamage { amount, to };
    let pump = |power: i32, toughness: i32, to: Recipient| Effect::Pump {
        power,
        toughness,
        to,
    };
    let counter = |to: Recipient| Effect::AddCounters {
        kind: CounterKind::PlusOnePlusOne,
        count: 1,
        to,
    };

    // ---- enters the battlefield ----
    pool.add(
        creature("Elvish Visionary", "{1}{G}", &["Elf", "Shaman"], 1, 1)
            .ability(triggered(TriggerSpec::Enters(This), &[], &[draw(You)]))
            .text("When Elvish Visionary enters, draw a card.")
            .build(),
    );
    pool.add(
        creature("Wall of Omens", "{1}{W}", &["Wall"], 0, 4)
            .keywords([Keyword::Defender])
            .ability(triggered(TriggerSpec::Enters(This), &[], &[draw(You)]))
            .text("Defender\nWhen Wall of Omens enters, draw a card.")
            .build(),
    );
    pool.add(
        creature("Soul Warden", "{W}", &["Human", "Cleric"], 1, 1)
            .ability(triggered(
                TriggerSpec::Enters(another_creature()),
                &[],
                &[gain(1, You)],
            ))
            .text("Whenever another creature enters, you gain 1 life.")
            .build(),
    );
    pool.add(
        creature("Flametongue Kavu", "{3}{R}", &["Kavu"], 4, 2)
            .ability(triggered(
                TriggerSpec::Enters(This),
                &[TargetSpec::Creature],
                &[damage(4, Target(0))],
            ))
            .text("When Flametongue Kavu enters, it deals 4 damage to target creature.")
            .build(),
    );
    pool.add(
        creature("Ajani's Pridemate", "{1}{W}", &["Cat", "Soldier"], 2, 2)
            .ability(triggered(
                TriggerSpec::GainsLife { whose: Who::You },
                &[],
                &[counter(Source)],
            ))
            .text("Whenever you gain life, put a +1/+1 counter on Ajani's Pridemate.")
            .build(),
    );

    // ---- dies ----
    pool.add(
        creature("Blood Artist", "{1}{B}", &["Vampire"], 0, 1)
            .ability(triggered(
                TriggerSpec::Dies(TriggerObj::Filter(ObjFilter::creatures())),
                &[TargetSpec::Player],
                &[lose(1, Target(0)), gain(1, You)],
            ))
            .text("Whenever Blood Artist or another creature dies, target player loses 1 life and you gain 1 life.")
            .build(),
    );
    pool.add(
        creature("Zulaport Cutthroat", "{1}{B}", &["Human", "Rogue", "Ally"], 1, 1)
            .ability(triggered(
                TriggerSpec::Dies(TriggerObj::Filter(ObjFilter::creatures().who(Who::You))),
                &[],
                &[lose(1, EachOpponent), gain(1, You)],
            ))
            .text("Whenever Zulaport Cutthroat or another creature you control dies, each opponent loses 1 life and you gain 1 life.")
            .build(),
    );
    pool.add(
        creature("Festering Goblin", "{B}", &["Zombie", "Goblin"], 1, 1)
            .ability(triggered(
                TriggerSpec::Dies(This),
                &[TargetSpec::Creature],
                &[pump(-1, -1, Target(0))],
            ))
            .text("When Festering Goblin dies, target creature gets -1/-1 until end of turn.")
            .build(),
    );
    pool.add(
        Builder::new("Perilous Myr", "{2}")
            .types([Artifact, Creature])
            .subtypes(&["Myr"])
            .pt(1, 1)
            .ability(triggered(
                TriggerSpec::Dies(This),
                &[TargetSpec::Any],
                &[damage(2, Target(0))],
            ))
            .text("When Perilous Myr dies, it deals 2 damage to any target.")
            .build(),
    );
    pool.add(
        Builder::new("Rancor", "{G}")
            .types([Enchantment])
            .subtypes(&["Aura"])
            .ability(AbilityDef::Enchant(TargetSpec::Creature))
            .ability(AbilityDef::Static(StaticAbility {
                affected: ObjFilter::enchanted(),
                parts: vec![
                    Modification::ModifyPt {
                        power: 2,
                        toughness: 0,
                    },
                    Modification::AddKeywords(KeywordSet::of([Keyword::Trample])),
                ],
            }))
            .ability(triggered(
                TriggerSpec::Dies(This),
                &[],
                &[Effect::ReturnToHand { what: Source }],
            ))
            .text("Enchant creature\nEnchanted creature gets +2/+0 and has trample.\nWhen Rancor is put into a graveyard from the battlefield, return Rancor to its owner's hand.")
            .build(),
    );

    // ---- beginning of step ----
    pool.add(
        Builder::new("Phyrexian Arena", "{1}{B}{B}")
            .types([Enchantment])
            .ability(triggered(
                TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::Upkeep,
                    whose: Who::You,
                },
                &[],
                &[draw(You), lose(1, You)],
            ))
            .text("At the beginning of your upkeep, you draw a card and you lose 1 life.")
            .build(),
    );
    pool.add(
        creature("Juzám Djinn", "{2}{B}{B}", &["Djinn"], 5, 5)
            .ability(triggered(
                TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::Upkeep,
                    whose: Who::You,
                },
                &[],
                &[damage(1, You)],
            ))
            .text("At the beginning of your upkeep, Juzám Djinn deals 1 damage to you.")
            .build(),
    );
    pool.add(
        Builder::new("Howling Mine", "{2}")
            .types([Artifact])
            .ability(AbilityDef::Triggered(TriggeredAbility {
                trigger: TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::Draw,
                    whose: Who::Any,
                },
                condition: Some(Condition::SourceUntapped),
                targets: Vec::new(),
                effects: vec![draw(TriggeringPlayer)],
            }))
            .text("At the beginning of each player's draw step, if Howling Mine is untapped, that player draws an additional card.")
            .build(),
    );

    // ---- combat ----
    pool.add(
        creature("Benalish Cavalry", "{1}{W}", &["Human", "Knight"], 2, 2)
            .ability(triggered(
                TriggerSpec::Attacks(This),
                &[],
                &[pump(1, 1, Source)],
            ))
            .ability(triggered(
                TriggerSpec::Blocks(This),
                &[],
                &[pump(1, 1, Source)],
            ))
            .text("Whenever Benalish Cavalry attacks or blocks, it gets +1/+1 until end of turn.")
            .build(),
    );
    pool.add(
        creature("Hellrider", "{2}{R}{R}", &["Devil"], 3, 3)
            .keywords([Keyword::Haste])
            .ability(triggered(
                TriggerSpec::Attacks(TriggerObj::Filter(ObjFilter::creatures().who(Who::You))),
                &[],
                &[damage(1, DefendingPlayer)],
            ))
            .text("Haste\nWhenever a creature you control attacks, Hellrider deals 1 damage to the player or planeswalker it's attacking.")
            .build(),
    );
    pool.add(
        creature("Markov Blademaster", "{1}{R}{R}", &["Vampire", "Warrior"], 1, 1)
            .keywords([Keyword::DoubleStrike])
            .ability(triggered(
                TriggerSpec::DealsCombatDamageToPlayer(This),
                &[],
                &[counter(Source)],
            ))
            .text("Double strike\nWhenever Markov Blademaster deals combat damage to a player, put a +1/+1 counter on it.")
            .build(),
    );

    // ---- keyword triggers ----
    pool.add(
        creature("Monastery Swiftspear", "{R}", &["Human", "Monk"], 1, 2)
            .keywords([Keyword::Haste, Keyword::Prowess])
            .text("Haste\nProwess")
            .build(),
    );
}

/// Cards with replacement and prevention effects (M2 slice 3). All are real
/// cards with simple Oracle text.
#[allow(clippy::too_many_lines)]
fn add_replacement_cards(pool: &mut CardPool) {
    use CardType::{Creature, Enchantment, Instant, Land};
    use Recipient::{Source, Target, You};
    use Replacement::{Double, DrawTwo, EnterTapped, Instead, PlusOne, Prevent, PreventNext};

    let replacement = |replaces: Replaces, replacement: Replacement| {
        AbilityDef::Replacement(ReplacementAbility {
            replaces,
            replacement,
            then: Vec::new(),
        })
    };
    let triggered = |trigger: TriggerSpec, effects: &[Effect]| {
        AbilityDef::Triggered(TriggeredAbility {
            trigger,
            condition: None,
            targets: Vec::new(),
            effects: effects.to_vec(),
        })
    };
    let creature = |name: &str, cost: &str, sub: &[&str], p: i32, t: i32| {
        Builder::new(name, cost)
            .types([Creature])
            .subtypes(sub)
            .pt(p, t)
    };
    let enchantment = |name: &str, cost: &str| Builder::new(name, cost).types([Enchantment]);
    let instant = |name: &str, cost: &str, text: &str, effects: &[Effect]| {
        Builder::new(name, cost)
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: Vec::new(),
                effects: effects.to_vec(),
            })
            .text(text)
            .build()
    };
    let any_damage = |to: Affects, combat: Option<bool>| Replaces::Damage {
        from: None,
        to,
        combat,
    };
    let prevent = |replaces: Replaces| {
        Effect::CreateReplacement(ReplacementAbility {
            replaces,
            replacement: Prevent,
            then: Vec::new(),
        })
    };
    let shield = |n: u32| {
        Effect::CreateReplacement(ReplacementAbility {
            replaces: any_damage(Affects::Target(0), None),
            replacement: PreventNext(n),
            then: Vec::new(),
        })
    };

    // ---- prevention spells ----
    pool.add(instant(
        "Fog",
        "{G}",
        "Prevent all combat damage that would be dealt this turn.",
        &[prevent(any_damage(Affects::Any, Some(true)))],
    ));
    pool.add(instant(
        "Holy Day",
        "{W}",
        "Prevent all combat damage that would be dealt this turn.",
        &[prevent(any_damage(Affects::Any, Some(true)))],
    ));
    pool.add(instant(
        "Safe Passage",
        "{2}{W}",
        "Prevent all damage that would be dealt to you and creatures you control this turn.",
        &[prevent(any_damage(
            Affects::YouAnd(ObjFilter::creatures().who(Who::You)),
            None,
        ))],
    ));
    pool.add(
        Builder::new("Mending Hands", "{W}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Any],
                effects: vec![shield(4)],
            })
            .text("Prevent the next 4 damage that would be dealt to any target this turn.")
            .build(),
    );
    pool.add(
        Builder::new("Bandage", "{W}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Any],
                effects: vec![
                    shield(1),
                    Effect::DrawCards {
                        count: 1,
                        who: You,
                    },
                ],
            })
            .text("Prevent the next 1 damage that would be dealt to any target this turn.\nDraw a card.")
            .build(),
    );
    pool.add(
        Builder::new("Skullcrack", "{1}{R}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::PlayerOrPlaneswalker],
                effects: vec![
                    Effect::DealDamage {
                        amount: 3,
                        to: Target(0),
                    },
                    Effect::Rule(RuleKind::NoLifeGain),
                    Effect::Rule(RuleKind::DamageCantBePrevented),
                ],
            })
            .text("Skullcrack deals 3 damage to target player or planeswalker. Players can't gain life this turn. Damage can't be prevented this turn.")
            .build(),
    );

    // ---- damage replacement ----
    pool.add(
        enchantment("Furnace of Rath", "{1}{R}{R}{R}")
            .ability(replacement(any_damage(Affects::Any, None), Double))
            .text("If a source would deal damage to a permanent or player, it deals double that damage to that permanent or player instead.")
            .build(),
    );
    pool.add(
        enchantment("Gratuitous Violence", "{2}{R}{R}{R}")
            .ability(replacement(
                Replaces::Damage {
                    from: Some(ObjFilter::creatures().who(Who::You)),
                    to: Affects::Any,
                    combat: None,
                },
                Double,
            ))
            .text("If a creature you control would deal damage to a permanent or player, it deals double that damage to that permanent or player instead.")
            .build(),
    );
    pool.add(
        creature("Phantom Tiger", "{2}{G}", &["Cat", "Spirit"], 1, 0)
            .ability(replacement(
                Replaces::Enters,
                Replacement::EnterWithCounters {
                    kind: CounterKind::PlusOnePlusOne,
                    n: 2,
                },
            ))
            .ability(AbilityDef::Replacement(ReplacementAbility {
                replaces: any_damage(Affects::This, None),
                replacement: Prevent,
                then: vec![Effect::RemoveCounters {
                    kind: CounterKind::PlusOnePlusOne,
                    count: 1,
                    to: Source,
                }],
            }))
            .text("Phantom Tiger enters the battlefield with two +1/+1 counters on it.\nIf damage would be dealt to Phantom Tiger, prevent that damage. Remove a +1/+1 counter from Phantom Tiger.")
            .build(),
    );

    // ---- life ----
    pool.add(
        enchantment("Sulfuric Vortex", "{1}{R}{R}")
            .ability(triggered(
                TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::Upkeep,
                    whose: Who::Any,
                },
                &[Effect::DealDamage {
                    amount: 2,
                    to: Recipient::TriggeringPlayer,
                }],
            ))
            .ability(replacement(Replaces::GainLife { whose: Who::Any }, Prevent))
            .text("At the beginning of each player's upkeep, Sulfuric Vortex deals 2 damage to that player.\nIf a player would gain life, that player gains no life instead.")
            .build(),
    );
    pool.add(
        creature("Rhox Faithmender", "{3}{W}", &["Rhino", "Monk"], 1, 5)
            .keywords([Keyword::Lifelink])
            .ability(replacement(Replaces::GainLife { whose: Who::You }, Double))
            .text("Lifelink\nIf you would gain life, you gain twice that much life instead.")
            .build(),
    );

    // ---- draws ----
    pool.add(
        creature("Laboratory Maniac", "{2}{U}", &["Human", "Wizard"], 2, 2)
            .ability(replacement(
                Replaces::Draw {
                    whose: Who::You,
                    library_empty: true,
                },
                Replacement::WinGame,
            ))
            .text("If you would draw a card while your library has no cards in it, you win the game instead.")
            .build(),
    );
    pool.add(
        enchantment("Thought Reflection", "{4}{U}{U}{U}")
            .ability(replacement(
                Replaces::Draw {
                    whose: Who::You,
                    library_empty: false,
                },
                DrawTwo,
            ))
            .text("If you would draw a card, draw two cards instead.")
            .build(),
    );

    // ---- graveyards ----
    pool.add(
        enchantment("Rest in Peace", "{1}{W}")
            .ability(triggered(
                TriggerSpec::Enters(TriggerObj::This),
                &[Effect::ExileAllGraveyards],
            ))
            .ability(replacement(
                Replaces::ToGraveyard {
                    whose: Who::Any,
                    from_battlefield: false,
                },
                Instead(crate::state::Zone::Exile),
            ))
            .text("When Rest in Peace enters, exile all cards from all graveyards.\nIf a card or token would be put into a graveyard from anywhere, exile it instead.")
            .build(),
    );
    pool.add(
        enchantment("Leyline of the Void", "{2}{B}{B}")
            .ability(replacement(
                Replaces::ToGraveyard {
                    whose: Who::Opponents,
                    from_battlefield: false,
                },
                Instead(crate::state::Zone::Exile),
            ))
            .text("If Leyline of the Void is in your opening hand, you may begin the game with it on the battlefield.\nIf a card would be put into an opponent's graveyard from anywhere, exile it instead.")
            .build(),
    );

    // ---- counters ----
    pool.add(
        enchantment("Hardened Scales", "{G}")
            .ability(replacement(
                Replaces::PlusCounters {
                    on: ObjFilter::creatures().who(Who::You),
                },
                PlusOne,
            ))
            .text("If one or more +1/+1 counters would be put on a creature you control, that many plus one +1/+1 counters are put on it instead.")
            .build(),
    );

    // ---- enters tapped ----
    pool.add(
        Builder::new("Azorius Guildgate", "")
            .types([Land])
            .subtypes(&["Gate"])
            .ability(replacement(Replaces::Enters, EnterTapped))
            .ability(AbilityDef::TapForMana(Mana::Colored(Color::White)))
            .ability(AbilityDef::TapForMana(Mana::Colored(Color::Blue)))
            .text("Azorius Guildgate enters the battlefield tapped.\n{T}: Add {W} or {U}.")
            .build(),
    );
}

/// M2 slice 4: activated abilities (CR 602), tokens (CR 111), Equipment
/// (CR 301.5), control-changing effects (CR 613.1b), and the legend rule
/// (CR 704.5j).
#[allow(clippy::too_many_lines)]
fn add_activated_cards(pool: &mut CardPool) {
    use CardType::{Artifact, Creature, Enchantment, Instant, Kindred, Land, Sorcery};
    use Recipient::{Source, Target, You};

    let goblin = pool.subtype("Goblin");
    let assembly_worker = pool.subtype("Assembly-Worker");

    let creature = |name: &str, cost: &str, sub: &[&str], p: i32, t: i32| {
        Builder::new(name, cost)
            .types([Creature])
            .subtypes(sub)
            .pt(p, t)
    };
    let activated = |cost: Cost, targets: &[TargetSpec], effects: &[Effect], text: &str| {
        AbilityDef::Activated(ActivatedAbility {
            cost,
            targets: targets.to_vec(),
            effects: effects.to_vec(),
            sorcery_speed: false,
            text: text.to_string(),
        })
    };
    let triggered = |trigger: TriggerSpec, condition: Option<Condition>, effects: &[Effect]| {
        AbilityDef::Triggered(TriggeredAbility {
            trigger,
            condition,
            targets: Vec::new(),
            effects: effects.to_vec(),
        })
    };
    let damage = |amount: u32, to: Recipient| Effect::DealDamage { amount, to };
    let tap_cost = || Cost::parse("").tap();
    let sac_self = || Cost::parse("").sacrifice(SacCost::This);

    // ---- tokens (CR 111) ----
    let soldier = pool.add(
        creature("Soldier", "", &["Soldier"], 1, 1)
            .color_indicator([Color::White])
            .token()
            .build(),
    );
    let goblin_token = pool.add(
        creature("Goblin", "", &["Goblin"], 1, 1)
            .color_indicator([Color::Red])
            .token()
            .build(),
    );
    let faerie_rogue = pool.add(
        creature("Faerie Rogue", "", &["Faerie", "Rogue"], 1, 1)
            .color_indicator([Color::Black])
            .keywords([Keyword::Flying])
            .token()
            .build(),
    );
    let elemental = pool.add(
        creature("Elemental", "", &["Elemental"], 1, 1)
            .color_indicator([Color::Red])
            .token()
            .build(),
    );
    let tokens = |token: CardRef, count: u32| Effect::CreateToken {
        token,
        count,
        who: You,
    };

    // ---- activated abilities ----
    pool.add(
        creature("Prodigal Sorcerer", "{2}{U}", &["Human", "Wizard"], 1, 1)
            .ability(activated(
                tap_cost(),
                &[TargetSpec::Any],
                &[damage(1, Target(0))],
                "{T}: deal 1 damage to any target",
            ))
            .text("{T}: Prodigal Sorcerer deals 1 damage to any target.")
            .build(),
    );
    pool.add(
        creature("Llanowar Elves", "{G}", &["Elf", "Druid"], 1, 1)
            .ability(AbilityDef::TapForMana(Mana::Colored(Color::Green)))
            .text("{T}: Add {G}.")
            .build(),
    );
    pool.add(
        creature("Mogg Fanatic", "{R}", &["Goblin"], 1, 1)
            .ability(activated(
                sac_self(),
                &[TargetSpec::Any],
                &[damage(1, Target(0))],
                "Sacrifice: deal 1 damage to any target",
            ))
            .text("Sacrifice Mogg Fanatic: It deals 1 damage to any target.")
            .build(),
    );
    pool.add(
        Builder::new("Bottle Gnomes", "{3}")
            .types([Artifact, Creature])
            .subtypes(&["Gnome"])
            .pt(1, 3)
            .ability(activated(
                sac_self(),
                &[],
                &[Effect::GainLife {
                    amount: 3,
                    who: You,
                }],
                "Sacrifice: you gain 3 life",
            ))
            .text("Sacrifice Bottle Gnomes: You gain 3 life.")
            .build(),
    );
    pool.add(
        creature("Shivan Dragon", "{4}{R}{R}", &["Dragon"], 5, 5)
            .keywords([Keyword::Flying])
            .ability(activated(
                Cost::parse("{R}"),
                &[],
                &[Effect::Pump {
                    power: 1,
                    toughness: 0,
                    to: Source,
                }],
                "{R}: +1/+0 until end of turn",
            ))
            .text("Flying\n{R}: Shivan Dragon gets +1/+0 until end of turn.")
            .build(),
    );
    pool.add(
        Builder::new("Rod of Ruin", "{4}")
            .types([Artifact])
            .ability(activated(
                Cost::parse("{3}").tap(),
                &[TargetSpec::Any],
                &[damage(1, Target(0))],
                "{3}, {T}: deal 1 damage to any target",
            ))
            .text("{3}, {T}: Rod of Ruin deals 1 damage to any target.")
            .build(),
    );
    pool.add(
        Builder::new("Pestilence", "{2}{B}{B}")
            .types([Enchantment])
            .ability(activated(
                Cost::parse("{B}"),
                &[],
                &[
                    damage(1, Recipient::EachCreature),
                    damage(1, Recipient::EachPlayer),
                ],
                "{B}: deal 1 damage to each creature and each player",
            ))
            .ability(triggered(
                TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::End,
                    whose: Who::Any,
                },
                Some(Condition::NoCreatures),
                &[Effect::Sacrifice { what: Source }],
            ))
            .text(
                "{B}: Pestilence deals 1 damage to each creature and each player.\nAt the beginning of the end step, if no creatures are on the battlefield, sacrifice Pestilence.",
            )
            .build(),
    );
    pool.add(
        creature("Royal Assassin", "{1}{B}{B}", &["Human", "Assassin"], 1, 1)
            .ability(activated(
                tap_cost(),
                &[TargetSpec::TappedCreature],
                &[Effect::Destroy { to: Target(0) }],
                "{T}: destroy target tapped creature",
            ))
            .text("{T}: Destroy target tapped creature.")
            .build(),
    );
    pool.add(
        Builder::new("Icy Manipulator", "{4}")
            .types([Artifact])
            .ability(activated(
                Cost::parse("{1}").tap(),
                &[TargetSpec::ArtifactCreatureOrLand],
                &[Effect::Tap { to: Target(0) }],
                "{1}, {T}: tap target artifact, creature, or land",
            ))
            .text("{1}, {T}: Tap target artifact, creature, or land.")
            .build(),
    );
    pool.add(
        Builder::new("Seal of Fire", "{R}")
            .types([Enchantment])
            .ability(activated(
                sac_self(),
                &[TargetSpec::Any],
                &[damage(2, Target(0))],
                "Sacrifice: deal 2 damage to any target",
            ))
            .text("Sacrifice Seal of Fire: It deals 2 damage to any target.")
            .build(),
    );
    pool.add(
        Builder::new("Mishra's Factory", "")
            .types([Land])
            .ability(AbilityDef::TapForMana(Mana::Colorless))
            .ability(activated(
                Cost::parse("{1}"),
                &[],
                &[Effect::Animate {
                    to: Source,
                    types: TypeSet::of([Artifact, Creature]),
                    subtypes: SubtypeSet::from_slice(&[assembly_worker]),
                    power: 2,
                    toughness: 2,
                }],
                "{1}: becomes a 2/2 Assembly-Worker artifact creature until end of turn",
            ))
            .text(
                "{T}: Add {C}.\n{1}: Mishra's Factory becomes a 2/2 Assembly-Worker artifact creature until end of turn. It's still a land.",
            )
            .build(),
    );

    // ---- Equipment (CR 301.5) ----
    let equip = |cost: &str| {
        AbilityDef::Activated(ActivatedAbility {
            cost: Cost::parse(cost),
            targets: vec![TargetSpec::CreatureYouControl],
            effects: vec![Effect::Attach { to: Target(0) }],
            sorcery_speed: true,
            text: format!("Equip {cost}"),
        })
    };
    pool.add(
        Builder::new("Bonesplitter", "{1}")
            .types([Artifact])
            .subtypes(&["Equipment"])
            .ability(AbilityDef::Static(StaticAbility {
                affected: ObjFilter::enchanted(),
                parts: vec![Modification::ModifyPt {
                    power: 2,
                    toughness: 0,
                }],
            }))
            .ability(equip("{1}"))
            .text("Equipped creature gets +2/+0.\nEquip {1}")
            .build(),
    );
    pool.add(
        Builder::new("Loxodon Warhammer", "{3}")
            .types([Artifact])
            .subtypes(&["Equipment"])
            .ability(AbilityDef::Static(StaticAbility {
                affected: ObjFilter::enchanted(),
                parts: vec![
                    Modification::ModifyPt {
                        power: 3,
                        toughness: 0,
                    },
                    Modification::AddKeywords(KeywordSet::of([
                        Keyword::Trample,
                        Keyword::Lifelink,
                    ])),
                ],
            }))
            .ability(equip("{3}"))
            .text("Equipped creature gets +3/+0 and has trample and lifelink.\nEquip {3}")
            .build(),
    );

    // ---- tokens ----
    pool.add(
        Builder::new("Raise the Alarm", "{1}{W}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: Vec::new(),
                effects: vec![tokens(soldier, 2)],
            })
            .text("Create two 1/1 white Soldier creature tokens.")
            .build(),
    );
    pool.add(
        Builder::new("Dragon Fodder", "{1}{R}")
            .types([Sorcery])
            .ability(AbilityDef::Spell {
                targets: Vec::new(),
                effects: vec![tokens(goblin_token, 2)],
            })
            .text("Create two 1/1 red Goblin creature tokens.")
            .build(),
    );
    pool.add(
        creature("Siege-Gang Commander", "{3}{R}{R}", &["Goblin"], 2, 2)
            .ability(triggered(
                TriggerSpec::Enters(TriggerObj::This),
                None,
                &[tokens(goblin_token, 3)],
            ))
            .ability(activated(
                Cost::parse("{1}{R}").sacrifice(SacCost::Filter(ObjFilter {
                    who: Some(Who::You),
                    subtypes: SubtypeSet::from_slice(&[goblin]),
                    ..Default::default()
                })),
                &[TargetSpec::Any],
                &[damage(2, Target(0))],
                "{1}{R}, Sacrifice a Goblin: deal 2 damage to any target",
            ))
            .text(
                "When Siege-Gang Commander enters, create three 1/1 red Goblin creature tokens.\n{1}{R}, Sacrifice a Goblin: Siege-Gang Commander deals 2 damage to any target.",
            )
            .build(),
    );
    pool.add(
        Builder::new("Bitterblossom", "{1}{B}")
            .types([Kindred, Enchantment])
            .subtypes(&["Faerie"])
            .ability(triggered(
                TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::Upkeep,
                    whose: Who::You,
                },
                None,
                &[
                    Effect::LoseLife { amount: 1, who: You },
                    tokens(faerie_rogue, 1),
                ],
            ))
            .text(
                "At the beginning of your upkeep, you lose 1 life and create a 1/1 black Faerie Rogue creature token with flying.",
            )
            .build(),
    );
    pool.add(
        creature("Young Pyromancer", "{1}{R}", &["Human", "Shaman"], 2, 1)
            .ability(triggered(
                TriggerSpec::CastsSpell {
                    whose: Who::You,
                    noncreature: false,
                    instant_or_sorcery: true,
                },
                None,
                &[tokens(elemental, 1)],
            ))
            .text("Whenever you cast an instant or sorcery spell, create a 1/1 red Elemental creature token.")
            .build(),
    );

    pool.add(
        Builder::new("Disenchant", "{1}{W}")
            .types([Instant])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::ArtifactOrEnchantment],
                effects: vec![Effect::Destroy { to: Target(0) }],
            })
            .text("Destroy target artifact or enchantment.")
            .build(),
    );

    // ---- control (CR 613.1b) ----
    pool.add(
        Builder::new("Act of Treason", "{2}{R}")
            .types([Sorcery])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Creature],
                effects: vec![
                    Effect::GainControl { of: Target(0) },
                    Effect::Untap { to: Target(0) },
                    Effect::Grant {
                        keywords: KeywordSet::of([Keyword::Haste]),
                        to: Target(0),
                    },
                ],
            })
            .text("Gain control of target creature until end of turn. Untap that creature. It gains haste until end of turn.")
            .build(),
    );
    pool.add(
        Builder::new("Control Magic", "{2}{U}{U}")
            .types([Enchantment])
            .subtypes(&["Aura"])
            .ability(AbilityDef::Enchant(TargetSpec::Creature))
            .ability(AbilityDef::Static(StaticAbility {
                affected: ObjFilter::enchanted(),
                parts: vec![Modification::Control],
            }))
            .text("Enchant creature\nYou control enchanted creature.")
            .build(),
    );

    // ---- the legend rule (CR 704.5j) ----
    pool.add(
        creature("Isamaru, Hound of Konda", "{W}", &["Dog"], 2, 2)
            .supertypes([Supertype::Legendary])
            .build(),
    );
}

/// M2 slice 5: copy effects (CR 707), characteristic-defining abilities
/// (CR 604.3), delayed triggered abilities (CR 603.7), modal spells (CR
/// 700.2), and "you may" clauses.
#[allow(clippy::too_many_lines)]
fn add_copy_cards(pool: &mut CardPool) {
    use CardType::{Artifact, Creature, Instant};
    use Recipient::{CreatedObject, SameObject, Source, Target};

    let creature = |name: &str, cost: &str, sub: &[&str], p: i32, t: i32| {
        Builder::new(name, cost)
            .types([Creature])
            .subtypes(sub)
            .pt(p, t)
    };
    let triggered = |trigger: TriggerSpec, targets: &[TargetSpec], effects: &[Effect]| {
        AbilityDef::Triggered(TriggeredAbility {
            trigger,
            condition: None,
            targets: targets.to_vec(),
            effects: effects.to_vec(),
        })
    };
    let at_next_end_step = |effects: &[Effect]| {
        triggered(
            TriggerSpec::BeginningOfStep {
                step: crate::turn::Step::End,
                whose: Who::Any,
            },
            &[],
            effects,
        )
    };
    let cda = |parts: Vec<Modification>| {
        AbilityDef::Static(StaticAbility {
            affected: ObjFilter::this(),
            parts,
        })
    };
    let mode = |targets: &[TargetSpec], effects: &[Effect], text: &str| Mode {
        targets: targets.to_vec(),
        effects: effects.to_vec(),
        text: text.to_string(),
    };

    // ---- copy effects (CR 707) ----
    pool.add(
        creature("Clone", "{3}{U}", &["Shapeshifter"], 0, 0)
            .ability(AbilityDef::EntersAsCopy(ObjFilter::creatures()))
            .text("You may have Clone enter as a copy of any creature on the battlefield.")
            .build(),
    );
    let image_trigger = triggered(
        TriggerSpec::BecomesTarget(TriggerObj::This),
        &[],
        &[Effect::Sacrifice { what: Source }],
    );
    pool.add(
        creature("Phantasmal Image", "{1}{U}", &["Illusion"], 0, 0)
            .ability(AbilityDef::EntersAsCopy(ObjFilter::creatures()))
            .ability(image_trigger.clone())
            .copy_except(CopyExcept {
                subtypes: vec!["Illusion".to_string()],
                keywords: KeywordSet::NONE,
                abilities: vec![image_trigger],
            })
            .text(
                "You may have Phantasmal Image enter as a copy of any creature on the battlefield, except it's an Illusion in addition to its other types and it has \"When this creature becomes the target of a spell or ability, sacrifice it.\"",
            )
            .build(),
    );
    let cryptoplasm_trigger = triggered(
        TriggerSpec::BeginningOfStep {
            step: crate::turn::Step::Upkeep,
            whose: Who::You,
        },
        &[TargetSpec::OtherCreature],
        &[Effect::YouMay(vec![Effect::BecomeCopy {
            of: Target(0),
            except: true,
        }])],
    );
    pool.add(
        creature("Cryptoplasm", "{1}{U}{U}", &["Shapeshifter"], 2, 2)
            .ability(cryptoplasm_trigger.clone())
            .copy_except(CopyExcept {
                subtypes: Vec::new(),
                keywords: KeywordSet::NONE,
                abilities: vec![cryptoplasm_trigger],
            })
            .text(
                "At the beginning of your upkeep, you may have Cryptoplasm become a copy of another target creature, except it has this ability.",
            )
            .build(),
    );
    let sacrifice_at_end =
        pool.register_ability(at_next_end_step(&[Effect::Sacrifice { what: SameObject }]));
    pool.add(
        creature("Kiki-Jiki, Mirror Breaker", "{2}{R}{R}{R}", &["Goblin", "Shaman"], 2, 2)
            .supertypes([Supertype::Legendary])
            .keywords([Keyword::Haste])
            .ability(AbilityDef::Activated(ActivatedAbility {
                cost: Cost::parse("").tap(),
                targets: vec![TargetSpec::NonlegendaryCreatureYouControl],
                effects: vec![
                    Effect::CreateTokenCopy {
                        of: Target(0),
                        except: true,
                    },
                    Effect::Delayed {
                        ability: sacrifice_at_end,
                        about: CreatedObject,
                    },
                ],
                sorcery_speed: false,
                text: "{T}: create a token copy of target nonlegendary creature you control with haste; sacrifice it at the beginning of the next end step".to_string(),
            }))
            .copy_except(CopyExcept {
                subtypes: Vec::new(),
                keywords: KeywordSet::of([Keyword::Haste]),
                abilities: Vec::new(),
            })
            .text(
                "Haste\n{T}: Create a token that's a copy of target nonlegendary creature you control, except it has haste. Sacrifice it at the beginning of the next end step.",
            )
            .build(),
    );

    // ---- characteristic-defining abilities (CR 604.3) ----
    pool.add(
        creature("Tarmogoyf", "{1}{G}", &["Lhurgoyf"], 0, 1)
            .ability(cda(vec![Modification::CdaPt {
                count: CdaCount::CardTypesInGraveyards,
                plus_toughness: 1,
            }]))
            .text(
                "Tarmogoyf's power is equal to the number of card types among cards in all graveyards and its toughness is equal to that number plus 1.",
            )
            .build(),
    );
    pool.add(
        creature("Lhurgoyf", "{2}{G}{G}", &["Lhurgoyf"], 0, 1)
            .ability(cda(vec![Modification::CdaPt {
                count: CdaCount::CreatureCardsInGraveyards,
                plus_toughness: 1,
            }]))
            .text(
                "Lhurgoyf's power is equal to the number of creature cards in all graveyards and its toughness is equal to that number plus 1.",
            )
            .build(),
    );
    pool.add(
        creature("Nightmare", "{5}{B}", &["Nightmare", "Horse"], 0, 0)
            .keywords([Keyword::Flying])
            .ability(cda(vec![Modification::CdaPt {
                count: CdaCount::Permanents(ObjFilter {
                    who: Some(Who::You),
                    subtypes: SubtypeSet::from_slice(&[Subtype::basic(Color::Black)]),
                    ..Default::default()
                }),
                plus_toughness: 0,
            }]))
            .text(
                "Flying\nNightmare's power and toughness are each equal to the number of Swamps you control.",
            )
            .build(),
    );
    pool.add(
        Builder::new("Transguild Courier", "{4}")
            .types([Artifact, Creature])
            .subtypes(&["Golem"])
            .pt(3, 3)
            .ability(cda(vec![Modification::CdaColors(ColorSet::of(Color::ALL))]))
            .text("Transguild Courier is all colors.")
            .build(),
    );

    // ---- delayed triggered abilities (CR 603.7) ----
    let return_at_end = pool.register_ability(at_next_end_step(&[Effect::ReturnToBattlefield {
        what: SameObject,
        counters: None,
    }]));
    pool.add(
        creature("Flickerwisp", "{1}{W}{W}", &["Elemental"], 3, 1)
            .keywords([Keyword::Flying])
            .ability(triggered(
                TriggerSpec::Enters(TriggerObj::This),
                &[TargetSpec::OtherPermanent],
                &[
                    Effect::Exile { what: Target(0) },
                    Effect::Delayed {
                        ability: return_at_end,
                        about: Target(0),
                    },
                ],
            ))
            .text(
                "Flying\nWhen Flickerwisp enters, exile another target permanent. Return that card to the battlefield under its owner's control at the beginning of the next end step.",
            )
            .build(),
    );
    let return_with_counter =
        pool.register_ability(at_next_end_step(&[Effect::ReturnToBattlefield {
            what: SameObject,
            counters: Some((CounterKind::PlusOnePlusOne, 1)),
        }]));
    pool.add(
        Builder::new("Otherworldly Journey", "{1}{W}")
            .types([Instant])
            .subtypes(&["Arcane"])
            .ability(AbilityDef::Spell {
                targets: vec![TargetSpec::Creature],
                effects: vec![
                    Effect::Exile { what: Target(0) },
                    Effect::Delayed {
                        ability: return_with_counter,
                        about: Target(0),
                    },
                ],
            })
            .text(
                "Exile target creature. At the beginning of the next end step, return that card to the battlefield under its owner's control with a +1/+1 counter on it.",
            )
            .build(),
    );

    // ---- modal spells (CR 700.2) ----
    pool.add(
        Builder::new("Abrade", "{1}{R}")
            .types([Instant])
            .ability(AbilityDef::Modal(vec![
                mode(
                    &[TargetSpec::Creature],
                    &[Effect::DealDamage {
                        amount: 3,
                        to: Target(0),
                    }],
                    "Abrade deals 3 damage to target creature",
                ),
                mode(
                    &[TargetSpec::Artifact],
                    &[Effect::Destroy { to: Target(0) }],
                    "Destroy target artifact",
                ),
            ]))
            .text("Choose one —\n• Abrade deals 3 damage to target creature.\n• Destroy target artifact.")
            .build(),
    );
    pool.add(
        Builder::new("Boros Charm", "{R}{W}")
            .types([Instant])
            .ability(AbilityDef::Modal(vec![
                mode(
                    &[TargetSpec::Player],
                    &[Effect::DealDamage {
                        amount: 4,
                        to: Target(0),
                    }],
                    "Boros Charm deals 4 damage to target player",
                ),
                mode(
                    &[],
                    &[Effect::Grant {
                        keywords: KeywordSet::of([Keyword::Indestructible]),
                        to: Recipient::EachPermanentYouControl,
                    }],
                    "Permanents you control gain indestructible until end of turn",
                ),
                mode(
                    &[TargetSpec::Creature],
                    &[Effect::Grant {
                        keywords: KeywordSet::of([Keyword::DoubleStrike]),
                        to: Target(0),
                    }],
                    "Target creature gains double strike until end of turn",
                ),
            ]))
            .text(
                "Choose one —\n• Boros Charm deals 4 damage to target player or planeswalker.\n• Permanents you control gain indestructible until end of turn.\n• Target creature gains double strike until end of turn.",
            )
            .build(),
    );
    pool.add(
        Builder::new("Grixis Charm", "{U}{B}{R}")
            .types([Instant])
            .ability(AbilityDef::Modal(vec![
                mode(
                    &[TargetSpec::Permanent],
                    &[Effect::ReturnToHand { what: Target(0) }],
                    "Return target permanent to its owner's hand",
                ),
                mode(
                    &[TargetSpec::Creature],
                    &[Effect::Pump {
                        power: -4,
                        toughness: -4,
                        to: Target(0),
                    }],
                    "Target creature gets -4/-4 until end of turn",
                ),
                mode(
                    &[],
                    &[Effect::Pump {
                        power: 2,
                        toughness: 0,
                        to: Recipient::EachCreatureYouControl,
                    }],
                    "Creatures you control get +2/+0 until end of turn",
                ),
            ]))
            .text(
                "Choose one —\n• Return target permanent to its owner's hand.\n• Target creature gets -4/-4 until end of turn.\n• Creatures you control get +2/+0 until end of turn.",
            )
            .build(),
    );

    // ---- "you may" ----
    pool.add(
        creature("Gravedigger", "{3}{B}", &["Zombie"], 2, 2)
            .ability(triggered(
                TriggerSpec::Enters(TriggerObj::This),
                &[TargetSpec::CreatureCardInYourGraveyard],
                &[Effect::YouMay(vec![Effect::ReturnToHand { what: Target(0) }])],
            ))
            .text(
                "When Gravedigger enters, you may return target creature card from your graveyard to your hand.",
            )
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
