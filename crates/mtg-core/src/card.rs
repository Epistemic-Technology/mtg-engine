//! Printed card definitions (CR 200–212): the immutable characteristics a
//! card has in the card database, before any effect applies. In M1 the
//! definitions are Rust data; M3 replaces this with the compiled DSL.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::mana::{Color, ColorSet, ManaCost};

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
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum AbilityDef {
    /// Intrinsic mana ability of a basic land type (CR 305.6).
    BasicLandMana(Color),
    /// The effect of an instant or sorcery spell when it resolves.
    Spell {
        targets: Vec<TargetSpec>,
        effects: Vec<Effect>,
    },
}

/// Printed characteristics plus abilities (CR 109.3, 200.1).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct CardDef {
    pub name: String,
    pub mana_cost: ManaCost,
    pub supertypes: SupertypeSet,
    pub types: TypeSet,
    pub subtypes: Vec<String>,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub keywords: KeywordSet,
    pub abilities: Vec<AbilityDef>,
    pub oracle_text: String,
}

impl CardDef {
    /// Printed color (CR 202.2): derived from the mana cost. Color indicators
    /// and CDAs come later.
    pub fn colors(&self) -> ColorSet {
        self.mana_cost.colors()
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
            AbilityDef::BasicLandMana(_) => None,
        })
    }
}

/// All card definitions a game can use, with name lookup.
#[derive(Clone, Debug, Default)]
pub struct CardPool {
    defs: Vec<CardDef>,
    by_name: HashMap<String, CardRef>,
}

impl CardPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, def: CardDef) -> CardRef {
        let r = CardRef(u32::try_from(self.defs.len()).expect("card pool too large"));
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
            supertypes: SupertypeSet::default(),
            types: TypeSet::default(),
            subtypes: Vec::new(),
            power: None,
            toughness: None,
            keywords: KeywordSet::NONE,
            abilities: Vec::new(),
            oracle_text: String::new(),
        })
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
    let basic = |name: &str, sub: &str, color: Color| {
        Builder::new(name, "")
            .supertypes([Supertype::Basic])
            .types([Land])
            .subtypes(&[sub])
            .ability(AbilityDef::BasicLandMana(color))
            .text(&format!("({{T}}: Add {{{}}}.)", color.letter()))
            .build()
    };
    pool.add(basic("Plains", "Plains", Color::White));
    pool.add(basic("Island", "Island", Color::Blue));
    pool.add(basic("Swamp", "Swamp", Color::Black));
    pool.add(basic("Mountain", "Mountain", Color::Red));
    pool.add(basic("Forest", "Forest", Color::Green));

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
    pool
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
