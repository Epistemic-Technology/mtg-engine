//! Printed card definitions (CR 200–212): the immutable characteristics a
//! card has in the card database, before any effect applies. Definitions
//! are built by the `mtg-dsl` compiler from the card files under `cards/`;
//! the [`Builder`] here is what it (and tests) construct them with.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::mana::{Color, ColorSet, Mana, ManaCost};

/// Index of a definition in a [`CardPool`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
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
    pub const EMPTY: TypeSet = TypeSet(0);

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

    /// The types in the set, in `CardType` order.
    pub fn iter(self) -> impl Iterator<Item = CardType> {
        [
            CardType::Artifact,
            CardType::Battle,
            CardType::Creature,
            CardType::Enchantment,
            CardType::Instant,
            CardType::Kindred,
            CardType::Land,
            CardType::Planeswalker,
            CardType::Sorcery,
        ]
        .into_iter()
        .filter(move |t| self.contains(*t))
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

    #[must_use]
    pub const fn without(self, t: Supertype) -> Self {
        SupertypeSet(self.0 & !(1 << (t as u8)))
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
    /// Landwalk of a kind other than a basic land type ("snow
    /// forestwalk", "nonbasic landwalk", CR 702.14a): a marker that one of
    /// the object's abilities is an `AbilityDef::Landwalk`. Not printed.
    LandwalkFilter,
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
    /// CR 702.22: attacks in a band with other creatures with banding and
    /// up to one without; a band is blocked as a group, and its
    /// controller divides the combat damage of creatures blocking or
    /// blocked by one with banding.
    Banding,
    /// "can't crew Vehicles" (CR 702.122d): it can't be tapped to pay a
    /// crew cost.
    CantCrew,
    /// "Untap this creature during each other player's untap step." (CR
    /// 502.3): it untaps in every untap step, not only its controller's.
    UntapsDuringOthers,
    /// "You may choose not to untap this creature during your untap step."
    /// (CR 502.3): its controller is asked, as their untap step untaps
    /// permanents, whether it stays tapped.
    MayNotUntap,
    /// "This creature can't be equipped." (CR 301.5c): an Equipment
    /// doesn't become attached to it, and one attached to it becomes
    /// unattached (CR 704.5n); it can still be targeted by equip.
    CantBeEquipped,
    /// "assigns combat damage equal to its toughness rather than its
    /// power" (CR 510.1a): its combat damage is its toughness.
    AssignsToughness,
    /// CR 702.147: "This creature can't block. When this creature
    /// attacks, sacrifice it at end of combat." Its triggered half is
    /// `CardPool::keyword_trigger`.
    Decayed,
    /// CR 702.26: it phases in or out before its controller untaps during
    /// each of their untap steps.
    Phasing,
    /// CR 702.99: as the spell resolves, its card may be exiled encoded on
    /// a creature its controller controls (`GameState::encoded`).
    Cipher,
    /// Hexproof from a quality (CR 702.11d): a marker that one of the
    /// object's abilities is an `AbilityDef::HexproofFrom`. Not printed.
    HexproofFromFilter,
    /// CR 702.192a: as the spell resolves it is exiled; the first time a
    /// spell its controller controls with its name resolves, a copy of it
    /// may be cast free at the beginning of each of their precombat main
    /// phases for the rest of the game (`Builtin::Paradigm`).
    Paradigm,
    /// CR 702.61a: "As long as this spell is on the stack, players can't
    /// cast other spells or activate abilities that aren't mana
    /// abilities."
    SplitSecond,
    /// CR 702.127a: on a split card's half, "You may cast this half of
    /// this split card from your graveyard," "This half of this split
    /// card can't be cast from any zone other than a graveyard," and "If
    /// this spell was cast from a graveyard, exile it instead of putting
    /// it anywhere else any time it would leave the stack." The split card
    /// itself has it too, so the graveyard lists it (`graveyard_active`).
    Aftermath,
    /// CR 702.102a: on both halves of a split card, "you may cast one or
    /// both halves of this card from your hand": the fused split spell is
    /// the card's third face (CR 702.102b-d). The split card has it too.
    Fuse,
    /// "Enchanted creature can't attack, block, or transform." (CR
    /// 701.27): no effect transforms it. A restriction kept with the
    /// keywords.
    CantTransform,
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
    /// CR 702.78a: conspire's reflexive trigger, "When you do, copy it
    /// and you may choose a new target for the copy."
    Conspire,
    /// CR 310.12b: a Siege's "When the last defense counter is removed
    /// from this permanent, exile it, then you may cast it transformed
    /// without paying its mana cost."
    SiegeDefeated,
    /// "the controller of the permanent it becomes sacrifices it at the
    /// beginning of the next cleanup step" (Spider Climb cast any time a
    /// sorcery couldn't have been).
    CleanupSacrifice,
    /// CR 702.99a: the encoded creature's "Whenever this creature deals
    /// combat damage to a player, you may copy the encoded card and you
    /// may cast the copy without paying its mana cost."
    Cipher,
    /// CR 702.185a: "Exile this creature at the beginning of the next end
    /// step, then you may cast it from exile on a later turn." (a warped
    /// permanent's delayed trigger).
    WarpExile,
    /// CR 702.60a: "When you cast this spell, you may reveal the top N
    /// cards of your library. You may cast spells with the same name as
    /// this spell from among those cards without paying their mana costs.
    /// Put the rest on the bottom of your library."
    Ripple,
    /// CR 701.66a: earthbend's "When that land dies or is put into exile,
    /// return it to the battlefield tapped under your control." (a
    /// delayed trigger about the land).
    EarthbendReturn,
    /// CR 702.192a: paradigm's "at the beginning of each of your precombat
    /// main phases for the rest of the game, create a copy of this object
    /// in exile. You may cast the copy without paying its mana cost." (a
    /// delayed trigger that stays).
    Paradigm,
    /// CR 702.152a: "If this spell's blitz cost was paid, sacrifice the
    /// permanent this spell becomes at the beginning of the next end
    /// step."
    BlitzSacrifice,
    /// CR 702.94a: "When you reveal this card this way, you may cast it
    /// by paying [cost] rather than its mana cost."
    Miracle,
}

/// How an exiled card is cast by a built-in trigger.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ExileCast {
    /// For its madness cost; if not cast, to the graveyard.
    Madness,
    /// Without paying its mana cost.
    Free,
    /// Transformed (its back face, CR 712.11), without paying its mana
    /// cost: a defeated Siege (CR 310.12b).
    Transformed,
    /// From the hand, for its miracle cost (CR 702.94a).
    Miracle,
}

/// A set of keywords: two words, aligned as one (a `u128` would align
/// `Characteristics` to 16 bytes).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub struct KeywordSet([u64; 2]);

impl KeywordSet {
    pub const NONE: KeywordSet = KeywordSet([0; 2]);
    /// Detain (CR 701.35a): "until your next turn, that permanent can't
    /// attack or block and its activated abilities can't be activated"
    /// (`Effect::UntilYourNextTurn` granting these).
    pub const DETAINED: KeywordSet = KeywordSet::NONE
        .with(Keyword::CantAttack)
        .with(Keyword::CantBlock)
        .with(Keyword::CantActivate);

    pub fn of(keywords: impl IntoIterator<Item = Keyword>) -> Self {
        keywords
            .into_iter()
            .fold(KeywordSet::NONE, KeywordSet::with)
    }

    /// The word and bit of a keyword.
    const fn bit(k: Keyword) -> (usize, u64) {
        ((k as usize) >> 6, 1 << ((k as u64) & 63))
    }

    #[must_use]
    pub const fn with(self, k: Keyword) -> Self {
        let (w, b) = Self::bit(k);
        let mut out = self.0;
        out[w] |= b;
        KeywordSet(out)
    }

    #[must_use]
    pub const fn union(self, other: KeywordSet) -> Self {
        KeywordSet([self.0[0] | other.0[0], self.0[1] | other.0[1]])
    }

    #[must_use]
    pub const fn difference(self, other: KeywordSet) -> Self {
        KeywordSet([self.0[0] & !other.0[0], self.0[1] & !other.0[1]])
    }

    pub const fn contains(self, k: Keyword) -> bool {
        let (w, b) = Self::bit(k);
        self.0[w] & b != 0
    }

    pub const fn intersects(self, other: KeywordSet) -> bool {
        self.0[0] & other.0[0] != 0 || self.0[1] & other.0[1] != 0
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
        .with(Keyword::Exploit)
        .with(Keyword::Decayed);

    /// The five protection-from-a-color keywords (CR 702.16).
    pub const PROTECTION: KeywordSet = KeywordSet::NONE
        .with(Keyword::ProtectionFromWhite)
        .with(Keyword::ProtectionFromBlue)
        .with(Keyword::ProtectionFromBlack)
        .with(Keyword::ProtectionFromRed)
        .with(Keyword::ProtectionFromGreen)
        .with(Keyword::ProtectionFromFilter);

    pub const fn contains_all(self, other: KeywordSet) -> bool {
        self.0[0] & other.0[0] == other.0[0] && self.0[1] & other.0[1] == other.0[1]
    }

    pub const fn is_empty(self) -> bool {
        self.0[0] == 0 && self.0[1] == 0
    }

    /// The keywords in the set, in `Keyword::ALL` order. Walks the set
    /// bits only: trigger detection asks this of every watching permanent
    /// on every event, and testing all of `ALL` was 8% of an effect-heavy
    /// game.
    pub fn iter(self) -> impl Iterator<Item = Keyword> {
        // Each keyword's position in `ALL` (which is not declaration
        // order); `u8::MAX` for none.
        const RANK: [u8; 128] = {
            assert!(Keyword::ALL.len() < u8::MAX as usize);
            let mut r = [u8::MAX; 128];
            let mut i: u8 = 0;
            while (i as usize) < Keyword::ALL.len() {
                r[Keyword::ALL[i as usize] as usize] = i;
                i += 1;
            }
            r
        };
        // The set re-indexed by position in `ALL`: lowest bit first is
        // then `ALL` order.
        let mut ranks = [0u64; 2];
        for (w, word) in self.0.iter().enumerate() {
            let mut word = *word;
            while word != 0 {
                let r = RANK[w * 64 + word.trailing_zeros() as usize];
                word &= word - 1;
                if r != u8::MAX {
                    ranks[usize::from(r >> 6)] |= 1 << (r & 63);
                }
            }
        }
        std::iter::from_fn(move || {
            let w = ranks.iter().position(|w| *w != 0)?;
            let r = w * 64 + ranks[w].trailing_zeros() as usize;
            ranks[w] &= ranks[w] - 1;
            Some(Keyword::ALL[r])
        })
    }
}

impl Keyword {
    pub const ALL: [Keyword; 79] = [
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
        Keyword::Banding,
        Keyword::CantCrew,
        Keyword::Decayed,
        Keyword::Phasing,
        Keyword::Cipher,
        Keyword::HexproofFromFilter,
        Keyword::Paradigm,
        Keyword::SplitSecond,
        Keyword::UntapsDuringOthers,
        Keyword::AssignsToughness,
        Keyword::Aftermath,
        Keyword::Fuse,
        Keyword::CantTransform,
        Keyword::LandwalkFilter,
        Keyword::MayNotUntap,
        Keyword::CantBeEquipped,
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
            Keyword::LandwalkFilter => "landwalk",
            Keyword::CantAttackAlone => "can't attack alone",
            Keyword::CantBlockAlone => "can't block alone",
            Keyword::Flanking => "flanking",
            Keyword::DoesntUntap => "doesn't untap during its controller's untap step",
            Keyword::CantBecomeUntapped => "can't become untapped",
            Keyword::Banding => "banding",
            Keyword::Decayed => "decayed",
            Keyword::Phasing => "phasing",
            Keyword::Cipher => "cipher",
            Keyword::HexproofFromFilter => "hexproof from",
            Keyword::Paradigm => "paradigm",
            Keyword::SplitSecond => "split second",
            Keyword::Aftermath => "aftermath",
            Keyword::Fuse => "fuse",
            Keyword::CantTransform => "can't transform",
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
            Keyword::CantCrew => "can't crew Vehicles",
            Keyword::UntapsDuringOthers => "untaps during each other player's untap step",
            Keyword::MayNotUntap => "may choose not to untap",
            Keyword::CantBeEquipped => "can't be equipped",
            Keyword::AssignsToughness => {
                "assigns combat damage equal to its toughness rather than its power"
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
    pub const ROLE: Subtype = Subtype(5 + 36);
    pub const ARCANE: Subtype = Subtype(5 + 43);
    pub const TREASURE: Subtype = Subtype(5 + 27);
    pub const VEHICLE: Subtype = Subtype(5 + 28);
    pub const SPACECRAFT: Subtype = Subtype(5 + 29);
    pub const ROOM: Subtype = Subtype(5 + 37);
    pub const SAGA: Subtype = Subtype(5 + 39);
    pub const SIEGE: Subtype = Subtype(5 + 47);
    /// Not interned: "is every creature type" (layer 4), held among an
    /// object's subtypes in place of its creature types; `has_subtype`
    /// treats it as each of them, like changeling (CR 702.73a) but as a
    /// type-changing effect, not an ability.
    pub const ALL_CREATURE_TYPES: Subtype = Subtype(u16::MAX);

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

    /// Whether another `push` would be dropped.
    pub fn is_full(&self) -> bool {
        usize::from(self.len) >= N
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
    /// Only the source and the creature it's paired with, while paired
    /// (soulbond, CR 702.95b: "both creatures", "each of those
    /// creatures").
    #[serde(default)]
    pub paired: bool,
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
    /// "target creature that player controls", "target card from that
    /// player's graveyard": a target controlled (in a graveyard, owned) by
    /// the triggering player or by an earlier target's controller.
    /// Judged with the stack object's targets (`GameState::that_player_ok`);
    /// elsewhere it doesn't restrict.
    pub that_player: Option<ThatPlayer>,
    /// "target creature other than enchanted creature": not what the source
    /// is attached to. Judged for targets of an ability of the source.
    pub not_attached: bool,
    /// "creatures enchanted player controls" (CR 303.4): controlled by the
    /// player the source, a Curse, is attached to. A placeholder the engine
    /// fills in (`with_enchanted_player`) where the source is known: a
    /// static ability's affected objects, a trigger's filter, an effect's
    /// group; unfilled, nothing matches.
    #[serde(default)]
    pub enchanted_player: bool,
    /// Controlled by this player: `enchanted_player` filled in.
    #[serde(default)]
    pub controlled_by: Option<crate::ids::PlayerId>,
}

/// The player a filter's `that_player` means.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ThatPlayer {
    /// The triggering player ("Whenever this creature deals combat damage
    /// to a player, ... target creature that player controls").
    Triggering,
    /// The controller of the `n`th target ("Target creature an opponent
    /// controls deals damage ... to another target creature that player
    /// controls").
    ControllerOf(u8),
    /// The `n`th target, a player ("Creatures target player controls get
    /// -2/-2", "for each creature target opponent controls"). Judged for an
    /// effect's groups and counts; nothing matches if that target is
    /// illegal as the effect happens (CR 608.2b).
    Target(u8),
    /// The `n`th target if it's a player, else its controller ("deals 1
    /// damage to each creature that player or that planeswalker's
    /// controller controls"); `planeswalker` is only the wording. Judged
    /// for groups and counts like `Target`.
    TargetOrController { n: u8, planeswalker: bool },
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
    /// "equipped creature" (CR 301.5: an Equipment attached to it).
    pub equipped: bool,
    /// "that entered this turn": its timestamp, the time it entered (CR
    /// 613.7e), is from this turn.
    pub entered_this_turn: bool,
    /// "that dealt damage this turn": it was the source of damage dealt
    /// this turn (CR 120.1).
    pub dealt_damage: bool,
    /// "that blocked this turn": it was declared as a blocker this turn
    /// (CR 509.1).
    pub blocked_this_turn: bool,
    /// "that attacked this turn": it was declared as an attacker this turn
    /// (CR 508.1).
    #[serde(default)]
    pub attacked_this_turn: bool,
    /// "dealt damage by this creature this turn": the source dealt damage
    /// to it this turn (`GameState::damaged_by_this_turn`). Judged only by
    /// dies triggers ("Whenever a creature dealt damage by this creature
    /// this turn dies", looking back, CR 603.10a).
    pub damaged_by_source: bool,
    /// "with mana value X": `ObjFilter::mana_value`'s number is the X of
    /// the spell (CR 107.3, 601.2b: X is announced before targets are
    /// chosen), filled in by the engine where X is known; elsewhere the
    /// mana value doesn't restrict.
    pub mana_value_x: bool,
    /// Converge (CR 702.119): "with mana value less than or equal to the
    /// number of colors of mana spent to cast this spell" (and power
    /// likewise): `ObjFilter::mana_value`'s and `power`'s numbers are the
    /// colors spent to cast the source, filled in by the engine where the
    /// source is known; elsewhere they don't restrict.
    pub spent_colors: bool,
    /// "token" (`true`, CR 111.1) or "nontoken" (`false`).
    pub token: Option<bool>,
    /// "with a +1/+1 counter on it", "with a counter on it" (CR 122.1):
    /// at least one counter of the kind, or of any kind.
    pub counter: Option<CounterQual>,
    /// "modified creature" (CR 700.9): it has a counter, is equipped, or
    /// is enchanted by an Aura its controller controls.
    pub modified: bool,
    /// "with toxic", "if that creature has toxic" (CR 702.164): one of its
    /// abilities is toxic N, printed or granted.
    pub toxic: bool,
    /// "Aura attached to a creature", "Aura attached to a land": what it
    /// is attached to (CR 303.4) is of that kind.
    pub on: Option<OnHost>,
    /// "face-down creature" (CR 708.2): a face-down permanent.
    pub face_down: bool,
    /// "creature with a morph ability" (CR 702.37; megamorph is morph,
    /// CR 702.37b, disguise isn't).
    pub morph: bool,
    /// "suspected creature" (`true`, CR 701.60) or not suspected.
    pub suspected: Option<bool>,
    /// "creature you own" (CR 108.3): owned by the player judging (the
    /// caster of a spell choosing targets, an effect's controller for its
    /// groups), whoever controls it. Judged for targets and effect
    /// groups; elsewhere it doesn't restrict.
    #[serde(default)]
    pub you_own: bool,
    /// "nonattacking creature": not an attacking creature (CR 508.1).
    #[serde(default)]
    pub nonattacking: bool,
    /// "with power or toughness 1 or less": `ObjFilter::power` and
    /// `toughness` (the same threshold) are alternatives, either enough.
    /// A characteristic, not game state: `any` leaves it out.
    #[serde(default)]
    pub pt_either: bool,
    /// "with power less than or equal to the number of creatures you
    /// control", "if its toughness is less than or equal to the number of
    /// cards in your graveyard": a characteristic compared with a count of
    /// the source's, filled in by the engine where the source is known
    /// (`GameState::fill_bound`); elsewhere it doesn't restrict.
    #[serde(default)]
    pub bound: Option<StatBound>,
    /// "with total power and toughness 5 or less": the sum compared. A
    /// characteristic, not game state: `any` leaves it out.
    #[serde(default)]
    pub pt_total: Option<(Cmp, i16)>,
    /// "with toughness greater than its power": a characteristic, like
    /// `pt_either`.
    #[serde(default)]
    pub toughness_over_power: bool,
    /// "transformed permanent": a transforming double-faced permanent with
    /// its back face up (CR 701.27g).
    #[serde(default)]
    pub transformed: bool,
    /// "a spell that's both white and black": `ObjFilter::colors` must
    /// all be the object's, not just one (CR 105.2). A characteristic, not
    /// game state: `any` leaves it out.
    #[serde(default)]
    pub all_colors: bool,
    /// "commander creatures you own" (`Some(true)`: "you own" follows the
    /// noun), "a commander" (`Some(false)`): a commander (CR 903.3). The
    /// game has no commanders (it isn't a Commander game), so nothing
    /// matches.
    #[serde(default)]
    pub commander: Option<bool>,
}

/// A characteristic compared with an amount the source counts
/// (`StatusQuals::bound`): `stat` is at most `n` (less than `n` with
/// `strict`), or `n` exactly with `Cmp::Exactly`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct StatBound {
    pub stat: Stat,
    pub cmp: Cmp,
    pub n: Num,
    pub strict: bool,
}

/// The kind of permanent `StatusQuals::on` asks the object to be attached
/// to.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum OnHost {
    Creature,
    Land,
}

/// The counters `StatusQuals::counter` asks for.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CounterQual {
    /// "with a counter on it": any kind.
    Any,
    /// "with a +1/+1 counter on it".
    Kind(CounterKind),
    /// "with three or more level counters on it".
    AtLeast(CounterKind, u32),
    /// "with no counters on it": none of any kind (Heartless Act).
    NoCounters,
}

impl StatusQuals {
    /// Whether any qualifier is set.
    pub const fn any(&self) -> bool {
        self.blocked.is_some()
            || self.enchanted.is_some()
            || self.equipped
            || self.token.is_some()
            || self.entered_this_turn
            || self.dealt_damage
            || self.blocked_this_turn
            || self.attacked_this_turn
            || self.damaged_by_source
            || self.mana_value_x
            || self.spent_colors
            || self.counter.is_some()
            || self.modified
            || self.toxic
            || self.on.is_some()
            || self.face_down
            || self.transformed
            || self.morph
            || self.suspected.is_some()
            || self.you_own
            || self.nonattacking
            || self.bound.is_some()
            || self.commander.is_some()
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
    /// The triggering object ("destroy all Auras attached to that
    /// creature" after "Whenever this creature blocks or becomes blocked by
    /// a creature").
    TriggeringObject,
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
    /// Has lesser power ("target attacking creature with lesser power",
    /// mentor, CR 702.134a).
    LesserPower,
    /// Has mana value at most the source's power ("with mana value less
    /// than or equal to this creature's power"); no source, nothing.
    ManaValueAtMostPower,
    /// Shares a card type (CR 205.2a: "two target permanents that share a
    /// card type").
    SharesType,
    /// Shares a creature type (CR 205.3m; a changeling has every one, CR
    /// 702.73a): "two target creature cards that share a creature type".
    /// Only a relation between targets (`GameState::that_player_ok`).
    SharesCreatureType,
    /// Controlled by a player other than the other targets' controllers
    /// ("two target creatures controlled by different players"; as they
    /// last existed if they have left the battlefield). Only a relation
    /// between targets (`GameState::that_player_ok`).
    DifferentController,
    /// Controlled by the same player as the other targets ("two target
    /// creatures controlled by the same player", "... the same opponent";
    /// "another target creature with the same controller"), as they last
    /// existed if they have left the battlefield. Only a relation between
    /// targets (`GameState::that_player_ok`).
    SameController,
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
    /// "with mana value of the chosen quality" once odd was chosen (the
    /// number is ignored; filled in by `ObjFilter::with_chosen`).
    Odd,
    /// ... once even was chosen ("Zero is even.").
    Even,
}

impl Cmp {
    pub fn holds<T: PartialOrd + Copy + Into<i64>>(self, have: T, n: T) -> bool {
        match self {
            Cmp::AtMost => have <= n,
            Cmp::AtLeast => have >= n,
            Cmp::Exactly => have == n,
            Cmp::Odd => have.into().rem_euclid(2) == 1,
            Cmp::Even => have.into().rem_euclid(2) == 0,
        }
    }
}

/// How many colors.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ColorCount {
    Colorless,
    Monocolored,
    Multicolored,
    /// "that's one or more colors": not colorless (CR 105.2).
    Colored,
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

    /// The filter with "enchanted player controls" filled in: controlled
    /// by `player`, the player the source is attached to; with none, a
    /// filter nothing matches.
    #[must_use]
    pub fn with_enchanted_player(&self, player: Option<crate::ids::PlayerId>) -> Self {
        let mut f = self.clone();
        if player.is_some() {
            f.enchanted_player = false;
            f.controlled_by = player;
        }
        if let Some(alt) = &f.or {
            f.or = Some(Box::new(alt.with_enchanted_player(player)));
        }
        f
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
            // "spell with the chosen name".
            crate::state::Chosen::Card(_) => f.named = chosen.card_ref().map(|d| (d, true)),
            // "with mana value of the chosen quality" (odd or even).
            crate::state::Chosen::Parity(odd) => {
                f.mana_value = Some((if odd { Cmp::Odd } else { Cmp::Even }, 0));
            }
            crate::state::Chosen::None
            | crate::state::Chosen::Votes(_)
            | crate::state::Chosen::Option(_) => {
                f.colors = Some(ColorSet::default());
            }
        }
        f
    }

    /// The filter with its `same` relation filled in from the object it
    /// relates to: that object's definition (`None` for an object with no
    /// name, CR 708.2: nothing has the same name as it), colors (sharing
    /// a color with a colorless object is impossible, CR 105.2c), and
    /// power (`None`: no object, so nothing has lesser power).
    #[must_use]
    pub fn with_same(
        &self,
        def: Option<CardRef>,
        colors: ColorSet,
        power: Option<i16>,
        types: TypeSet,
    ) -> Self {
        let mut f = self.clone();
        let Some(same) = f.same.take() else {
            return f;
        };
        match same.what {
            // No name: a definition no object has.
            SameWhat::Name => f.named = Some((def.unwrap_or(CardRef(u32::MAX)), true)),
            SameWhat::NotName => f.named = def.map(|d| (d, false)),
            SameWhat::Color => f.colors = Some(colors),
            SameWhat::LesserPower => {
                f.power = Some((Cmp::AtMost, power.map_or(i16::MIN, |p| p.saturating_sub(1))));
            }
            // A negative power (or none): no mana value is that small.
            SameWhat::ManaValueAtMostPower => match power.and_then(|p| u8::try_from(p).ok()) {
                Some(p) => f.mana_value = Some((Cmp::AtMost, p)),
                None => f.named = Some((CardRef(u32::MAX), true)),
            },
            // No card types (or no object): nothing shares one.
            SameWhat::SharesType if types.is_empty() => {
                f.named = Some((CardRef(u32::MAX), true));
            }
            SameWhat::SharesType => f.any_types = types,
            // Judged against the other target (`that_player_ok`).
            SameWhat::SharesCreatureType
            | SameWhat::DifferentController
            | SameWhat::SameController => {}
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

    #[allow(clippy::too_many_lines)] // one check per qualifier
    fn matches_one(
        &self,
        obj: crate::ids::ObjId,
        obj_controller: crate::ids::PlayerId,
        c: &crate::state::Characteristics,
        source: Option<crate::ids::ObjId>,
        controller: Option<crate::ids::PlayerId>,
    ) -> bool {
        // An unfilled "of the chosen type" (`with_chosen`), "named ~"
        // (`with_same`), or "enchanted player controls"
        // (`with_enchanted_player`) matches nothing; nor does "commander"
        // (CR 903.3: the game has no commanders).
        if self.chosen
            || self.same.is_some()
            || self.enchanted_player
            || self.status.commander.is_some()
        {
            return false;
        }
        if self.controlled_by.is_some_and(|p| p != obj_controller) {
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
            && !(if self.status.all_colors {
                c.colors.union(cs) == c.colors
            } else {
                c.colors.intersects(cs)
            })
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
            || ((c.has(Keyword::Changeling) || c.subtypes.contains(&Subtype::ALL_CREATURE_TYPES))
                && self.not_subtypes.iter().any(|s| s.is_creature_type()))
        {
            return false;
        }
        if !self.any_types.is_empty() && !c.types.intersects(self.any_types) {
            return false;
        }
        if self.status.toughness_over_power && c.toughness <= c.power {
            return false;
        }
        // "with total power and toughness 5 or less" (Cut Down).
        if let Some((cmp, n)) = self.status.pt_total
            && !cmp.holds(i32::from(c.power) + i32::from(c.toughness), i32::from(n))
        {
            return false;
        }
        if self.status.pt_either {
            // "with power or toughness 1 or less": either is enough.
            let p = self.power.is_some_and(|(cmp, n)| cmp.holds(c.power, n));
            let t = self
                .toughness
                .is_some_and(|(cmp, n)| cmp.holds(c.toughness, n));
            if !p && !t {
                return false;
            }
        } else {
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
        }
        if let Some(cc) = self.color_count {
            let n = c.colors.iter().count();
            let ok = match cc {
                ColorCount::Colorless => n == 0,
                ColorCount::Monocolored => n == 1,
                ColorCount::Multicolored => n >= 2,
                ColorCount::Colored => n >= 1,
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
    /// Left as it is ("has base power 4": the toughness isn't set, CR
    /// 613.4b).
    Unchanged,
    /// The object's mana value (CR 202.3).
    ManaValue,
}

/// One part of a continuous effect. Each part applies in exactly one layer.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Modification {
    /// "is a creature in addition to its other types" (layer 4).
    AddTypes(TypeSet),
    /// "Enchanted land is snow." (layer 4, CR 205.4: a supertype added
    /// to its others).
    AddSupertype(Supertype),
    /// "becomes an artifact creature" (layer 4): these card types replace
    /// its others (CR 205.1a), except land ("It's still a land", CR
    /// 205.1b; see `Effect::Animate`).
    BecomeTypes(TypeSet),
    /// "Enchanted creature ... is a blue Frog creature" (layer 4): these
    /// card types replace all its others, land included ("It loses all
    /// other card types"); supertypes stay (CR 205.1a).
    SetTypes(TypeSet),
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
    /// "is every creature type", "gains all creature types" (layer 4):
    /// its creature types become all of them (`Subtype::ALL_CREATURE_TYPES`).
    AllCreatureTypes,
    /// "loses all creature types" (layer 4): no creature types, a
    /// changeling's included (as for `SetCreatureType`).
    NoCreatureTypes,
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
    /// A characteristic-defining ability setting power and/or toughness
    /// (CR 604.3, layer 7a): what `sets` names is the count (toughness the
    /// count plus `plus_toughness`); the other keeps its printed value. The
    /// count is judged from the source's controller as the layer applies
    /// (`CountExpr`, as for `ModifyPtPer`).
    CdaPt {
        count: CountExpr,
        sets: CdaSets,
        plus_toughness: i32,
    },
}

/// Which of power and toughness a characteristic-defining ability defines
/// ("~'s power is equal to ...", "~'s toughness is equal to ...", "~'s
/// power and toughness are each equal to ...").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum CdaSets {
    #[default]
    Both,
    Power,
    Toughness,
}

impl CdaSets {
    pub const fn power(self) -> bool {
        matches!(self, CdaSets::Both | CdaSets::Power)
    }
    pub const fn toughness(self) -> bool {
        matches!(self, CdaSets::Both | CdaSets::Toughness)
    }
}

impl Modification {
    pub const fn layer(&self) -> Layer {
        match self {
            Modification::Control => Layer::L2Control,
            Modification::AddTypes(_)
            | Modification::AddSupertype(_)
            | Modification::BecomeTypes(_)
            | Modification::SetTypes(_)
            | Modification::AddLandType(_)
            | Modification::SetLandType(_)
            | Modification::AddSubtype(_)
            | Modification::SetCreatureType(_)
            | Modification::ChosenLandType
            | Modification::AddChosenType
            | Modification::AllCreatureTypes
            | Modification::NoCreatureTypes => Layer::L4Type,
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
    /// Your speed (CR 702.179: "where X is your speed"; 0 with none).
    Speed,
    /// N times another count ("2 life for each creature you control").
    Times(u32, Box<CountExpr>),
    /// Another count divided by `by`, rounded down or (`up`) up: "half the
    /// number of artifacts you control, rounded down" (CR 107.1a), "for
    /// every seven cards in your graveyard".
    Divided {
        by: u32,
        up: bool,
        inner: Box<CountExpr>,
    },
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
    /// The objects the most recent group of this resolution denoted ("You
    /// gain 1 life for each of those creatures"; `Recipient::ThoseObjects`).
    ThoseObjects,
    /// Counters of a kind on the triggering object, as it last existed
    /// once it has left (CR 608.2h): "Whenever a creature you control with
    /// a +1/+1 counter on it leaves the battlefield, create a Mutagen token
    /// for each +1/+1 counter on it."
    TriggerCounters(CounterKind),
    /// Creatures blocking the triggering object, or else the source ("for
    /// each creature blocking it"), less `beyond` ("beyond the first").
    BlockingIt { beyond: u32 },
    /// The `n`th target's mana value ("where X is its mana value"), last
    /// known information once it has left (CR 608.2h).
    TargetManaValue(usize),
    /// The mana value of the resolution's card ("that card's mana value":
    /// a card revealed this way, `Effect::RevealTop`), last known
    /// information once it has moved; 0 with none.
    CardManaValue,
    /// The greatest power among battlefield permanents matching the
    /// filter ("the greatest power among creatures you control"); 0 with
    /// none.
    GreatestPower(ObjFilter),
    /// Different names among battlefield permanents matching the filter
    /// ("the number of differently named lands you control"); a nameless
    /// face-down permanent adds none.
    DifferentlyNamed(ObjFilter),
    /// Devour (CR 702.82c): the permanents the source devoured matching
    /// the filter, as they last existed ("for each creature it devoured",
    /// "the number of Goblins it devoured").
    Devoured(ObjFilter),
    /// The number of creatures in your party (CR 700.8): up to one each
    /// matching these four filters (Cleric, Rogue, Warrior, Wizard
    /// creatures you control), as many as can be assigned.
    Party(Box<[ObjFilter; 4]>),
    /// The greatest mana value among permanents you control.
    GreatestManaValue,
    /// The other instant and sorcery spells you've cast this turn (not
    /// the source or triggering spell itself); with `before`, only those
    /// cast before it ("copy it for each other instant and sorcery spell
    /// you've cast before it this turn").
    OtherInstantSorceryCast { before: bool },
    /// The source's toughness ("where X is its toughness").
    SourceToughness,
    /// Colors among permanents you control (vivid).
    ColorsAmongPermanents,
    /// Cards in your opponents' graveyards, all or matching the filter.
    CardsInOpponentsGraveyards(Option<ObjFilter>),
    /// Opponents you're attacking: those a creature you control is
    /// attacking (CR 506.2).
    OpponentsAttacked,
    /// Converge: the number of colors of mana spent to cast the source
    /// (CR 601.2h).
    SpentColors,
    /// "for each time it was kicked": how many times the source's
    /// multikicker cost was paid (CR 702.33d).
    TimesKicked,
    /// "for each {E} paid this way": the energy the resolution's "pay any
    /// amount of {E}" paid (its X).
    EnergyPaid,
    /// The mana from Treasures spent to cast the source ("for each mana
    /// from a Treasure spent to cast it").
    SpentTreasure,
    /// "the amount of mana spent to cast it" (CR 601.2h): all the mana
    /// paid to cast the source, X included.
    SpentTotal,
    /// "the amount of mana spent to cast that spell": the spell that
    /// triggered the ability ("Whenever you cast an instant or sorcery
    /// spell"); 0 for one not cast.
    ThatSpellSpent,
    /// "for each color of mana spent to cast that spell".
    ThatSpellSpentColors,
    /// The votes the resolution's vote (`Effect::Vote`) gave its `n`th
    /// choice ("for each sprout vote").
    Votes(u8),
    /// The greatest power among cards in your graveyard matching the
    /// filter ("the greatest power among creature cards in your
    /// graveyard"); 0 with none.
    GreatestPowerInGraveyard(ObjFilter),
    /// A characteristic of the triggering object ("When enchanted creature
    /// dies, its controller loses life equal to its toughness"), as it
    /// last existed once it has left (CR 608.2h).
    TriggerStat(Stat),
    /// A characteristic of what the source is attached to ("It deals
    /// damage to you equal to its power" of enchanted creature).
    AttachedStat(Stat),
    /// A characteristic of the first object the resolution, or its costs,
    /// moved `verb`'s way ("life equal to the sacrificed creature's
    /// toughness", "sacrifices a creature, then gains life equal to that
    /// creature's toughness"), as it last existed (CR 608.2h).
    MovedStat { verb: ThisWay, stat: Stat },
    /// Cards in the hand of the `n`th target player ("the number of cards
    /// in that player's hand").
    TargetHand(usize),
    /// The greatest toughness among battlefield permanents matching the
    /// filter; 0 with none.
    GreatestToughness(ObjFilter),
    /// One count plus another ("the number of creatures on the battlefield
    /// plus the number of artifacts on the battlefield").
    Sum(Box<CountExpr>, Box<CountExpr>),
    /// The damage the resolution has dealt (CR 120.3), or of it the
    /// excess (CR 120.4a: beyond lethal damage to a creature, beyond its
    /// loyalty to a planeswalker): "You gain life equal to the damage
    /// dealt this way", "the excess damage dealt this way".
    DamageThisWay { excess: bool },
    /// Card types among cards in all graveyards (Tarmogoyf; CR 205.2a).
    CardTypesInAllGraveyards,
    /// Counters of a kind on the source ("the number of +1/+1 counters on
    /// this creature", "for each scream counter on this Aura"), as it last
    /// existed on the battlefield once it has left (CR 608.2h: "When this
    /// creature dies, ... the number of +1/+1 counters on this creature").
    SourceCounters(CounterKind),
    /// Cards, all or matching the filter, in the graveyard of the
    /// controller of what the source is attached to ("Enchanted creature
    /// gets -X/-X, where X is the number of creature cards in its
    /// controller's graveyard"); 0 unattached.
    HostGraveyard(Option<ObjFilter>),
    /// Devotion (CR 700.5): the mana symbols of these colors among the
    /// mana costs of permanents you control ("your devotion to green");
    /// a hybrid symbol of two of them counts once (CR 700.5a). Also
    /// chroma's "the number of red mana symbols in the mana costs of
    /// permanents you control".
    Devotion(ColorSet),
    /// Card types among cards in your graveyard (CR 205.2a; Nethergoyf).
    CardTypesInGraveyard,
    /// Counters of every kind on the source ("the number of counters on
    /// Savanti Romero"), as it last existed on the battlefield once it
    /// has left (CR 608.2h).
    SourceAllCounters,
    /// 2 to the power of the source's X ("draws 2ˣ cards"), capped so it
    /// fits.
    TwoToX,
    /// Opponents who have more cards in hand than you.
    OpponentsMoreCards,
    /// The total power, toughness, or mana value of the battlefield
    /// permanents matching the filter ("the total power of creatures you
    /// control"): the values as they are, negative ones included, and 0
    /// if the sum is negative (CR 107.1b).
    Total(Stat, ObjFilter),
    /// Cards exiled with the source (CR 607.2a), all or matching the
    /// filter, still in exile ("for each card exiled with this
    /// enchantment").
    ExiledWith(Option<ObjFilter>),
    /// Cards you own in exile, all or matching the filter ("the number of
    /// cards you own in exile"); with `graveyard`, and in your graveyard
    /// too ("for each creature card you own in exile and in your
    /// graveyard").
    CardsInExile {
        filter: Option<ObjFilter>,
        graveyard: bool,
    },
    /// `n` minus the count, at least 0: "if you have fewer than ten cards
    /// in hand, draw cards equal to the difference".
    Difference(u32, Box<CountExpr>),
    /// Cards in the library of the `n`th target player ("Target opponent
    /// mills half their library, rounded down").
    TargetLibrary(usize),
    /// The greatest mana value among battlefield permanents matching the
    /// filter ("the greatest mana value among artifacts you control"); 0
    /// with none.
    GreatestManaValueOf(ObjFilter),
    /// The greatest mana value among cards in your graveyard, all or
    /// matching the filter ("the greatest mana value among cards in your
    /// graveyard"); 0 with none.
    GreatestManaValueInGraveyard(Option<ObjFilter>),
    /// The greater of `n` and the count: "equal to 2 or the greatest
    /// power among Dinosaurs you control, whichever is greater".
    AtLeast(u32, Box<CountExpr>),
    /// The colors of the spell that triggered the ability ("for each of
    /// that spell's colors"), as it last existed once it has left the
    /// stack (CR 608.2h).
    ThatSpellColors,
    /// The damage dealt to the source this turn ("where X is the amount of
    /// damage dealt to it this turn" of a creature that died): the damage
    /// marked on it, as it last existed on the battlefield (CR 120.6).
    SourceDamage,
    /// The colors of the `n`th target ("for each of its colors"), as it
    /// last existed once it has left (CR 608.2h).
    TargetColors(usize),
    /// The turn's count of a kind of event ("the life you've lost this
    /// turn", "the amount of life you gained this turn", "for each
    /// opponent who lost life this turn"; `GameState::turn_count`).
    Happened(TurnCount),
}

impl CountExpr {
    /// `n` divided by `by`, rounded down or (`up`) up (CR 107.1a).
    #[must_use]
    pub fn divide(n: u32, by: u32, up: bool) -> u32 {
        let by = by.max(1);
        if up { n.div_ceil(by) } else { n / by }
    }

    /// Whether the count is the cards in a hand, possibly multiplied: a
    /// static that reads it must be re-read as hands change.
    pub fn reads_hand(&self) -> bool {
        match self {
            CountExpr::CardsInHand => true,
            CountExpr::Times(_, inner)
            | CountExpr::Plus(_, inner)
            | CountExpr::AtLeast(_, inner) => inner.reads_hand(),
            _ => false,
        }
    }

    /// Whether the count reads the turn's history, so a static ability
    /// using it must be recomputed as that changes (`history_static`).
    pub fn reads_history(&self) -> bool {
        match self {
            CountExpr::Happened(_) => true,
            CountExpr::Times(_, c)
            | CountExpr::Plus(_, c)
            | CountExpr::Difference(_, c)
            | CountExpr::AtLeast(_, c) => c.reads_history(),
            CountExpr::Divided { inner, .. } => inner.reads_history(),
            CountExpr::Sum(a, b) => a.reads_history() || b.reads_history(),
            _ => false,
        }
    }
}

impl CountExpr {
    /// Whether a static ability can read this count (`layers::per_count`):
    /// the rest exist only while an effect resolves (its targets, what it
    /// did) and read 0 in a static ability, so the card language rejects
    /// them there.
    #[must_use]
    pub fn in_static(&self) -> bool {
        match self {
            // The card language keeps the rest (your draws, discards, life
            // lost, spells cast, attackers, deaths) for a resolving effect.
            CountExpr::Happened(c) => {
                matches!(
                    (c.kind, c.who),
                    (
                        TurnEvent::LifeGained
                            | TurnEvent::Sacrificed
                            | TurnEvent::CreatureEntered
                            | TurnEvent::CreatureLeft,
                        _
                    ) | (TurnEvent::Attacked, TurnWho::Anyone)
                        | (TurnEvent::LifeLost, TurnWho::Opponent)
                ) && c.filter.is_none()
            }
            CountExpr::Sum(a, b) => a.in_static() && b.in_static(),
            CountExpr::Plus(_, c)
            | CountExpr::Times(_, c)
            | CountExpr::Difference(_, c)
            | CountExpr::AtLeast(_, c) => c.in_static(),
            CountExpr::Divided { inner, .. } => inner.in_static(),
            CountExpr::Permanents(_)
            | CountExpr::Domain
            | CountExpr::ColorsAmongPermanents
            | CountExpr::CardsInGraveyard(_)
            | CountExpr::CardsInAllGraveyards(_)
            | CountExpr::CardsInOpponentsGraveyards(_)
            | CountExpr::SourceCounters(_)
            | CountExpr::HostGraveyard(_)
            | CountExpr::CardTypesInAllGraveyards
            | CountExpr::GreatestPowerInGraveyard(_)
            | CountExpr::GreatestManaValue
            | CountExpr::GreatestManaValueOf(_)
            | CountExpr::GreatestManaValueInGraveyard(_)
            | CountExpr::Devotion(_)
            | CountExpr::CardsInHand
            | CountExpr::CardTypesInGraveyard
            | CountExpr::SourceAllCounters
            | CountExpr::OpponentsMoreCards
            | CountExpr::ExiledWith(_)
            | CountExpr::CardsInExile { .. }
            | CountExpr::Speed
            | CountExpr::Life => true,

            // Total power and toughness would read the characteristics
            // being computed; total mana value does not.
            CountExpr::Total(stat, _) => *stat == Stat::ManaValue,
            CountExpr::TwoToX
            | CountExpr::SourcePower
            | CountExpr::TargetStat { .. }
            | CountExpr::ThisWay(..)
            | CountExpr::LifeLostThisWay
            | CountExpr::ThoseObjects
            | CountExpr::TriggerCounters(_)
            | CountExpr::Votes(_)
            | CountExpr::BlockingIt { .. }
            | CountExpr::TargetManaValue(_)
            | CountExpr::CardManaValue
            | CountExpr::GreatestPower(_)
            | CountExpr::DifferentlyNamed(_)
            | CountExpr::Devoured(_)
            | CountExpr::Party(_)
            | CountExpr::SourceToughness
            | CountExpr::OtherInstantSorceryCast { .. }
            | CountExpr::OpponentsAttacked
            | CountExpr::SpentColors
            | CountExpr::TimesKicked
            | CountExpr::EnergyPaid
            | CountExpr::SpentTreasure
            | CountExpr::SpentTotal
            | CountExpr::ThatSpellSpent
            | CountExpr::ThatSpellSpentColors
            | CountExpr::TriggerStat(_)
            | CountExpr::AttachedStat(_)
            | CountExpr::MovedStat { .. }
            | CountExpr::TargetHand(_)
            | CountExpr::GreatestToughness(_)
            | CountExpr::TargetLibrary(_)
            | CountExpr::ThatSpellColors
            | CountExpr::SourceDamage
            | CountExpr::TargetColors(_)
            | CountExpr::DamageThisWay { .. } => false,
        }
    }
}

/// A characteristic a count reads from an object.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Stat {
    Power,
    Toughness,
    /// CR 202.3: on the stack, X is its chosen value (CR 202.3e).
    ManaValue,
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

/// Which cards an `Effect::PutCard` moves.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CardSet {
    /// The resolution's card ("it", "that card").
    It,
    /// Every card revealed this way ("all cards revealed this way").
    Revealed,
    /// The revealed cards but the resolution's card ("all other cards
    /// revealed this way").
    OtherRevealed,
}

/// Where an `Effect::PutCard` moves cards.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CardDest {
    /// Its owner's hand.
    Hand,
    /// Its owner's graveyard.
    Graveyard,
    /// The battlefield under the effect's controller's control (CR
    /// 110.2a).
    Battlefield { tapped: bool },
    /// The bottom of its owner's library, in the order revealed ("in any
    /// order": the engine keeps it).
    LibraryBottom,
}

/// Where `Effect::DigSplit` puts one of the cards looked at: its owner's
/// hand, graveyard, the top or bottom of its library, or exile.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum SplitDest {
    Hand,
    Graveyard,
    Top,
    Bottom,
    Exile,
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
/// adventurer, 722 preparation). `ModalDfc`'s face is its back face (CR
/// 712.3), which may be cast or played instead of the front (CR 712.11b,
/// 712.12). `Omen` is an adventurer whose spell is
/// shuffled into its owner's library as it resolves instead of exiled.
/// `Prepare`'s face is its prepare spell (CR 722.2a): never cast itself,
/// only copies of it while the permanent is prepared (CR 722.3). `Flip`'s
/// face is its flipped half (CR 710.1b), with the card's mana cost (CR
/// 710.1c): `Effect::Flip` turns the permanent to it, for good.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Layout {
    #[default]
    Normal,
    Adventure,
    Omen,
    Transform,
    Split,
    Prepare,
    ModalDfc,
    Flip,
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
    /// "Creatures with power less than this creature's power can't block
    /// it." (judged as blockers are declared, CR 509.1b).
    NotByLesserPower,
    /// "can't be blocked except by three or more creatures" (CR 509.1b,
    /// like menace's two).
    MinBlockers(u8),
    /// "can block creatures with shadow as though it had shadow" (`had`)
    /// or "... as though they didn't have shadow" (CR 702.28b): shadow
    /// doesn't stop it blocking.
    BlocksShadow { had: bool },
    /// "can block creatures with landwalk abilities as though they didn't
    /// have those abilities" (CR 702.14b).
    BlocksLandwalk,
    /// "can block an additional seven creatures each combat" (CR 509.1a).
    BlocksAdditional(u8),
}

/// A duration other than "until end of turn" for the continuous effects
/// an `Effect::Lasting` creates (CR 611.2b).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Lasting {
    /// "until your next turn": ends as its controller's next turn begins.
    YourNextTurn,
    /// "for as long as this creature remains on the battlefield".
    SourceOnBattlefield,
    /// "for as long as it remains tapped": each affected object, while it
    /// stays tapped.
    WhileTapped,
    /// "for as long as you control this creature".
    YouControlSource,
    /// "for as long as this creature remains tapped": while the source
    /// stays a tapped permanent (Phyrexian Gremlins).
    SourceTapped,
    /// "until this card is cast from exile" (an ability activated from the
    /// hand that exiled its card as a cost): while that card stays in
    /// exile.
    UntilCastFromExile,
    /// No duration at all ("That token gains haste.", "It gains
    /// \"...\"" from a resolving spell or ability): the effect lasts as
    /// long as the object it affects (CR 611.2a, 400.7).
    Forever,
    /// "until end of combat", "this combat" (CR 500.5a, 511.2): ends as
    /// the combat phase ends (or with the turn, if it has no combat).
    EndOfCombat,
}

/// How long cards exiled by an impulse effect stay playable.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum PlayUntil {
    /// "this turn", "until end of turn" (CR 514.2).
    ThisTurn,
    /// "until the end of your next turn".
    EndOfYourNextTurn,
    /// "until your next end step": until the controller's next end step
    /// begins.
    YourNextEndStep,
    /// "You may play them." with no duration (Runestone Caverns): for as
    /// long as the card stays in exile.
    Forever,
    /// "You may play that card for as long as you control Lightning": while
    /// the source stays on the battlefield under the player's control
    /// (the same object, CR 400.7).
    WhileYouControlSource,
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
    /// "put one onto the battlefield tapped and the other into your hand"
    /// (Cultivate): the searcher chooses which.
    BattlefieldAndHand,
    /// "put that card into your graveyard" (Vile Entomber).
    Graveyard,
    /// "put it into your hand or graveyard" (Dina's Guidance): the
    /// searcher chooses, for each card.
    HandOrGraveyard,
    /// "exile them, then shuffle" (Foresight): face up, not linked to
    /// anything.
    Exile,
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

/// A kind of event the turn log counts for each player (`TurnLog::tally`),
/// what "this turn" conditions and counts read (`TurnCount`): "if you
/// gained life this turn", "an opponent lost life this turn".
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum TurnEvent {
    /// Life the player gained (CR 119.3), in life rather than in gains.
    LifeGained,
    /// Life the player lost (CR 119.3: damage to a player is life loss
    /// too), in life.
    LifeLost,
    /// Times the player was dealt damage (CR 120.3), after prevention.
    Damaged,
    /// Times the player was dealt noncombat damage.
    NoncombatDamaged,
    /// Permanents that entered the battlefield under the player's control,
    /// by their characteristics just before they entered: creatures,
    /// nonland permanents, lands, artifacts, and face-down permanents (CR
    /// 708.3).
    CreatureEntered,
    NonlandEntered,
    LandEntered,
    ArtifactEntered,
    FaceDownEntered,
    /// Permanents that left the battlefield under the player's control
    /// (revolt), as they last existed: any, creatures, nonland ones.
    PermanentLeft,
    CreatureLeft,
    NonlandLeft,
    /// Creatures put into a graveyard from the battlefield under the
    /// player's control (CR 700.4), as they last existed.
    Died,
    /// Permanents put into a graveyard from the battlefield under the
    /// player's control, with their card types as they last existed
    /// (`TurnLog::types`).
    ToGraveyard,
    /// Spells the player cast for their warp cost (CR 702.185a).
    Warped,
    /// Cards the player drew (CR 121.1).
    Drew,
    /// Cards the player discarded (CR 701.8).
    Discarded,
    /// Permanent cards put into the player's graveyard from anywhere: the
    /// player descended (CR 700.11).
    Descended,
    /// Cards put into the player's graveyard from anywhere (tokens are no
    /// cards), with their card types there (`TurnLog::types`).
    CardToGraveyard,
    /// Cards that left the player's graveyard.
    LeftGraveyard,
    /// Permanents the player sacrificed (CR 701.21), with their card types
    /// as they last existed (`TurnLog::types`).
    Sacrificed,
    /// Times the player scried (CR 701.22b: scry 0 is no scry) and
    /// surveilled (CR 701.25c).
    Scried,
    Surveilled,
    /// Crimes the player committed (CR 700.13), one per spell or ability.
    Crime,
    /// Times +1/+1 counters were put on a permanent the player controls.
    PlusCountersOn,
    /// Times the player put +1/+1 counters on a permanent (CR 122.6: as
    /// the controller of the effect, or of a permanent entering with
    /// them).
    PlusCountersBy,
    /// Whether the player put counters of any kind on a creature (CR
    /// 122.6): counted once a turn, since asking what the permanent is
    /// costs characteristics.
    CountersOnCreatureBy,
    /// Equip abilities the player activated ("the first equip ability you
    /// activate each turn").
    Equipped,
    /// Creatures the player declared as attackers (CR 508.1), counted as
    /// they are declared, before any is tapped.
    Attacked,
    /// Combat damage dealt to a player by an Assassin creature the player
    /// controlled (freerunning, CR 702.173a); counted only with a
    /// freerunning card in the pool.
    AssassinDamage,
    /// Permanents that entered the battlefield under the player's control.
    Entered,
    /// Spells the player cast (CR 601.2i), and those cast from their hand
    /// (CR 601.2a).
    Cast,
    CastFromHand,
}

impl TurnEvent {
    /// The number of kinds: the turn log's rows.
    pub const COUNT: usize = 33;

    /// Whether a static ability reading this kind must be refreshed as it
    /// is counted (`CardDef::history_static`), the kinds counted without a
    /// zone change that refreshes characteristics anyway; a count of any
    /// kind in a static ability refreshes (`CountExpr::reads_history`).
    #[must_use]
    pub const fn refreshes(self) -> bool {
        match self {
            TurnEvent::LifeGained
            | TurnEvent::LifeLost
            | TurnEvent::Damaged
            | TurnEvent::NoncombatDamaged
            | TurnEvent::NonlandLeft
            | TurnEvent::Warped
            | TurnEvent::CardToGraveyard
            | TurnEvent::LeftGraveyard
            | TurnEvent::Sacrificed
            | TurnEvent::Scried
            | TurnEvent::Surveilled
            | TurnEvent::Crime
            | TurnEvent::PlusCountersBy
            | TurnEvent::CountersOnCreatureBy
            | TurnEvent::Attacked => true,
            TurnEvent::CreatureEntered
            | TurnEvent::NonlandEntered
            | TurnEvent::LandEntered
            | TurnEvent::ArtifactEntered
            | TurnEvent::FaceDownEntered
            | TurnEvent::PermanentLeft
            | TurnEvent::CreatureLeft
            | TurnEvent::Died
            | TurnEvent::ToGraveyard
            | TurnEvent::Drew
            | TurnEvent::Discarded
            | TurnEvent::Descended
            | TurnEvent::PlusCountersOn
            | TurnEvent::Equipped
            | TurnEvent::AssassinDamage
            | TurnEvent::Entered
            | TurnEvent::Cast
            | TurnEvent::CastFromHand => false,
        }
    }

    /// Whether a static ability reading this kind must be refreshed as a
    /// card leaves a hand (`CardDef::hand_static`).
    #[must_use]
    pub const fn with_hand(self) -> bool {
        matches!(
            self,
            TurnEvent::Drew | TurnEvent::Discarded | TurnEvent::Cast | TurnEvent::CastFromHand
        )
    }
}

/// Whose events a turn count counts, judged from the ability's controller.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum TurnWho {
    You,
    /// As a condition, any one opponent's ("an opponent lost life this
    /// turn"); as a count, the opponents with one or more ("for each
    /// opponent who lost life this turn").
    Opponent,
    /// Every player's together ("two or more creatures died this turn").
    Anyone,
}

/// "Events of kind K by player P this turn": what a `StaticCondition::
/// Happened` compares and a `CountExpr::Happened` counts, read from the
/// turn log.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct TurnCount {
    pub kind: TurnEvent,
    pub who: TurnWho,
    /// Only events about objects with one of these card types ("an
    /// artifact or creature was put into a graveyard"); empty, all. The
    /// log keeps the types each player's events had, not each event's, so
    /// with types a count is 1 or 0: whether one or more happened.
    pub types: TypeSet,
    /// "Another": not counting the source's own event ("another creature
    /// entered the battlefield under your control this turn", "another
    /// blue spell").
    pub other: bool,
    /// Only events about objects matching the filter ("you attacked with
    /// three or more Merfolk this turn", "for each Zubera that died this
    /// turn"), read from the turn's list of those objects, for the kinds
    /// that keep one (`GameState::listed`): attackers, entered permanents,
    /// dead creatures as they last existed, spells as they were cast.
    pub filter: Option<Box<ObjFilter>>,
}

impl TurnCount {
    #[must_use]
    pub const fn of(kind: TurnEvent, who: TurnWho) -> Self {
        TurnCount {
            kind,
            who,
            types: TypeSet::EMPTY,
            other: false,
            filter: None,
        }
    }
}

/// What a conditional static ability checks (threshold, metalcraft,
/// delirium, "during your turn", "as long as you control an Island").
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum StaticCondition {
    /// "seven or more cards are in your graveyard" (threshold).
    CardsInGraveyard(u32),
    /// "there are two or more unlocked doors among Rooms you control"
    /// (CR 709.5j).
    UnlockedDoors(u32),
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
    /// "during turns other than yours": it isn't the controller's turn.
    /// The turn change bumps the characteristics cache, as for `YourTurn`.
    NotYourTurn,
    /// "it's your main phase" (CR 505.1), "it's your combat phase" (CR
    /// 506.1), and "it's the first combat phase of the turn": for effects
    /// that add phases (`Effect::AdditionalPhases`), judged as they
    /// resolve.
    YourMainPhase,
    YourCombatPhase,
    FirstCombatPhase,
    /// "this is the second time this ability has resolved this turn": the
    /// resolving ability's object has resolved it this many times this
    /// turn, counting this time.
    NthResolution(u8),
    /// "as long as it's attacking" (the source).
    SourceAttacking,
    /// "as long as it attacked a battle this turn" (the source, CR
    /// 310.5).
    SourceAttackedBattle,
    /// "as long as it's enchanted" / "as long as it's equipped".
    SourceEnchanted,
    SourceEquipped,
    /// "as long as this creature is monstrous" (CR 701.37b).
    SourceMonstrous,
    /// "this creature is suspected" (`true`) or "it's not suspected" (CR
    /// 701.60).
    SourceSuspected(bool),
    /// "as long as an opponent has 10 or less life"; "as long as you have
    /// 30 or more life" (`at_least`), "you have 5 or less life".
    OpponentLifeAtMost(i32),
    YourLife {
        n: i32,
        at_least: bool,
    },
    /// Your life total against your starting life total (CR 103.4): at
    /// least `at_least` more than it ("greater than": 1; "at least 10
    /// greater", "at least 7 life more": 10, 7; "greater than or equal
    /// to": 0), or with `below`, less than it.
    LifeVsStarting {
        at_least: i32,
        below: bool,
    },
    /// "a player has 13 or less life".
    AnyPlayerLifeAtMost(i32),
    /// "you have more life than an opponent".
    MoreLifeThanOpponent,
    /// "there are three or more basic land types among lands you control"
    /// (domain, CR 305.6).
    BasicLandTypes(u32),
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
    /// attackers are declared); `at_least` 0: "defending player controls
    /// no Glimmer creatures".
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
    /// "you control two or fewer other lands" (the fast lands): at most
    /// `n` permanents matching the filter.
    YouControlAtMost {
        filter: ObjFilter,
        n: u32,
    },
    /// "you control an Urza's Mine and an Urza's Power-Plant": at least
    /// one permanent matching each filter.
    YouControlEach(Vec<ObjFilter>),
    /// "if it attacked during your last turn" (Giant Turtle, Goblin Rock
    /// Sled): the source was declared as an attacker during its
    /// controller's most recent turn before this one (or this one, while
    /// it is not theirs).
    AttackedLastTurn,
    /// The entering object was cast with escape ("escapes with").
    Escaped,
    /// Hellbent-style: "you have N or fewer cards in hand" (`at_most`) /
    /// "N or more".
    CardsInHand {
        n: u32,
        at_most: bool,
    },
    /// "If this ability has been activated four or more times this turn"
    /// (Initiates of the Ebon Hand): the source's counted abilities
    /// (`ActivatedAbility::counted`) activated at least N times this turn
    /// (`GameState::activated_once`), this activation included.
    ActivatedThisTurn(u32),
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
    /// "If this spell was cast using teamwork": its teamwork cost was paid.
    Teamwork,
    /// "If a Dragon was beheld": the spell's behold cost was paid (CR
    /// 701.69).
    Beheld,
    /// "If evidence was collected", "if this spell's additional cost was
    /// paid": its optional additional cost (`CardDef::optional_cost`) was
    /// paid. A card in hand counts as paid when the cost could be ("This
    /// spell costs {2} less to cast if evidence was collected").
    OptionalCostPaid,
    /// The source is saddled (CR 702.171b): "whenever this creature
    /// attacks while saddled".
    Saddled,
    /// "there are two or more instant and/or sorcery cards in your
    /// graveyard" (spell mastery), "three or more creature cards".
    GraveyardCards {
        filter: ObjFilter,
        at_least: u32,
    },
    /// "you have no land cards in hand": no card in your hand matches.
    NoCardInHand(ObjFilter),
    /// Either holds ("you control a Desert or there is a Desert card in
    /// your graveyard").
    Or(Box<StaticCondition>, Box<StaticCondition>),
    /// The other doesn't hold: a conditional static ability's "Otherwise,
    /// it gets -2/-2." (CR 611.3a).
    Not(Box<StaticCondition>),
    /// You could cast a sorcery now (CR 307.1: your main phase, the stack
    /// empty but for the source): "You may cast this spell as though it had
    /// flash if you pay {2} more to cast it" is flash and {2} more while
    /// this doesn't hold.
    SorceryTiming,
    /// The condition as the spell was cast ("If you cast this spell during
    /// your main phase", "if you controlled a Mount as you cast this
    /// spell"): judged then and remembered by the spell (the definition's
    /// `cast_snapshot`).
    AsCast(Box<StaticCondition>),
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
    /// "if this creature attacked or blocked this combat" (the Clockwork
    /// creatures): it is an attacking or blocking creature of the combat
    /// now ending.
    SourceAttackedOrBlocked,
    /// Pack tactics: "if you attacked with creatures with total power 6 or
    /// greater this combat": the creatures you declared as attackers this
    /// combat, their total power as they were declared.
    AttackedTotalPower(u16),
    /// "this creature is blocked" (CR 509.1h).
    SourceBlocked,
    /// "at least one creature is blocking this creature": blocked, by a
    /// creature still blocking it.
    SourceBeingBlocked,
    /// "equipped creature is legendary": the permanent the source is
    /// attached to matches.
    AttachedIs(ObjFilter),
    /// "if you cast it from your hand", "if this spell was cast from a
    /// graveyard" (CR 601.2a): where the source was cast from; a
    /// permanent remembers it from its spell.
    CastFrom(crate::state::Zone),
    /// "it's night" (CR 731).
    Night,
    /// "you have more cards in hand than each opponent".
    MoreCardsInHand,
    /// "an opponent is poisoned" (1), "an opponent has three or more poison
    /// counters".
    OpponentPoison(u32),
    /// "you own a card in exile".
    OwnCardInExile,
    /// "there are three or more cards exiled with this creature" (CR
    /// 607.2a: what the source's abilities exiled, still in exile).
    ExiledWith(u32),
    /// "there are four or more card types among cards exiled with this
    /// creature".
    TypesExiledWith(u32),
    /// Formidable: "creatures you control have total power 8 or greater".
    TotalPower(u32),
    /// Coven: "you control three or more creatures with different powers".
    DifferentPowers(u32),
    /// A part of the turn (CR 602.5b timing restrictions): "during your
    /// upkeep", "before attackers are declared".
    During(TurnTime),
    /// "you're the monarch" (CR 725).
    Monarch,
    /// "you've completed a dungeon" (CR 309.7).
    CompletedDungeon,
    /// "you have the city's blessing" (CR 702.131c).
    CityBlessing,
    /// "you have an enduring story" (CR 702.195b).
    EnduringStory,
    /// "unless you have an enduring story": the controller doesn't have
    /// one.
    NoEnduringStory,
    /// Max speed (CR 702.179e): "you have max speed", speed 4.
    MaxSpeed,
    /// "• Khans — ...": the source's controller chose this one of its
    /// named options as it entered (`ChoiceKind::Named`; the Sieges).
    Chose(u8),
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
    /// "if it wasn't cast or no mana was spent to cast it": no mana was
    /// paid to cast the source (CR 601.2h; cast without paying its mana
    /// cost, plotted, or not cast at all).
    NoManaSpent,
    /// "If X is 10 or more", "if X is 2 or less" (`at_least` false): X of
    /// the resolving spell or ability (CR 107.3), as its other X effects
    /// read it; elsewhere (a permanent's "enters tapped" replacement), X
    /// of the source as it was cast (CR 107.3m; 0 if it wasn't).
    XIs {
        n: u32,
        at_least: bool,
    },
    /// "If the gift wasn't promised" (CR 702.174; a promised gift is the
    /// kicker flag): the source wasn't kicked.
    NotKicked,
    /// "Whenever you cast a spell, if that spell was kicked" (CR
    /// 702.33d): the spell that triggered the ability was kicked.
    ThatKicked,
    /// "If this spell's sneak cost was paid", "if its sneak cost was
    /// paid" (CR 702.190): the source was cast for its sneak cost (kept by
    /// the permanent). The number is wording only: the pronoun for a
    /// permanent (0 "its", 1 "his", 2 "her").
    SneakPaid(u8),
    /// "if its prowl cost was paid", "if this spell's prowl cost was paid"
    /// (CR 702.76a).
    ProwlPaid,
    /// "If death gets more votes" (CR 701.38): the resolution's vote gave
    /// its `index`th choice more votes than any other; with `ties`, "or
    /// the vote is tied", at least as many. Judged only by an
    /// `Effect::IfCondition` after the vote; elsewhere false.
    MoreVotes {
        index: u8,
        ties: bool,
    },
    /// Prowl's condition (CR 702.76a): "if you dealt combat damage to a
    /// player this turn with a [creature of one of these types]".
    ProwlDamage(Vec<Subtype>),
    /// "If it was cast using web-slinging" (CR 702.188): the source was
    /// cast for its web-slinging cost (kept by the permanent). The number
    /// is wording only: 0 "it was", 1 "he was", 2 "she was", 3 "they
    /// were".
    WebSlung(u8),
    /// "As long as this permanent's impending cost was paid and it has a
    /// time counter on it" (CR 702.176a): impending's static ability and
    /// end step trigger.
    Impending,
    /// Both hold ("if there is an Elf card in your graveyard and this
    /// creature has a -1/-1 counter on it").
    And(Box<StaticCondition>, Box<StaticCondition>),
    /// "an opponent controls more lands than you".
    OpponentMoreLands,
    /// "an opponent controls more creatures than you", "an opponent has
    /// more life than you", "an opponent has more cards in hand than you".
    OpponentHasMore(MoreWhat),
    /// "it isn't your main phase" (CR 505.1).
    NotYourMainPhase,
    /// "this spell was cast from anywhere other than your hand": the
    /// source was cast (CR 601.2a), from another zone than this one.
    NotCastFrom(crate::state::Zone),
    /// "if you cast it" (CR 601.2a): the source was cast, from any zone;
    /// a permanent remembers it from its spell.
    WasCast,
    /// "creatures you control have total toughness 10 or greater".
    TotalToughness(u32),
    /// "you gained life this turn", "an opponent lost life this turn",
    /// "you've gained 3 or more life this turn": the turn's count of a kind
    /// of event compared with `n` (CR 611.3a; `GameState::turn_count`).
    Happened {
        count: TurnCount,
        cmp: Cmp,
        n: u32,
    },
}

impl StaticCondition {
    /// "One or more events of this kind this turn".
    #[must_use]
    pub const fn happened(kind: TurnEvent, who: TurnWho) -> Self {
        StaticCondition::Happened {
            count: TurnCount::of(kind, who),
            cmp: Cmp::AtLeast,
            n: 1,
        }
    }

    /// Whether a static ability with this condition must be refreshed as
    /// the turn log counts an event (`CardDef::history_static`).
    #[must_use]
    pub fn refreshes_with_turn_log(&self) -> bool {
        match self {
            StaticCondition::Happened { count, .. } => count.kind.refreshes(),
            StaticCondition::And(a, b) | StaticCondition::Or(a, b) => {
                a.refreshes_with_turn_log() || b.refreshes_with_turn_log()
            }
            _ => false,
        }
    }
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

/// What `StaticCondition::OpponentHasMore` compares.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum MoreWhat {
    Creatures,
    Life,
    CardsInHand,
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
    /// "during your main phase" (CR 505.1).
    YourMain,
    /// "during the declare attackers step".
    DeclareAttackers,
    /// "during combat before blockers are declared".
    BeforeBlockers,
    /// "during combat after blockers are declared".
    AfterBlockers,
    /// "during the declare attackers step and only if you've been attacked
    /// this step": a creature attacks you (judged by the game state).
    AttackedThisStep,
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
            TurnTime::YourMain => step.is_main() && active == you,
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
            TurnTime::DeclareAttackers | TurnTime::AttackedThisStep => {
                step == Step::DeclareAttackers
            }
            TurnTime::BeforeBlockers => {
                matches!(step, Step::BeginningOfCombat | Step::DeclareAttackers)
            }
            TurnTime::AfterBlockers => matches!(
                step,
                Step::DeclareBlockers
                    | Step::FirstStrikeDamage
                    | Step::CombatDamage
                    | Step::EndOfCombat
            ),
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
    /// Defense counters on a battle (CR 310.4b).
    Defense,
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
    Mining,
    Scream,
    Credit,
    Brick,
    Currency,
    Sleep,
    Net,
    Flood,
    Glyph,
    Crystal,
    Unlock,
    Loot,
    Supply,
    Film,
    Possession,
    Wish,
    /// A named counter with no rules of its own (Drake Hatcher).
    Incubation,
    /// CR 122.1h: if a permanent with a finality counter on it would be
    /// put into a graveyard from the battlefield, it is exiled instead.
    Finality,
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
    DoubleStrike,
    /// A named counter with no rules of its own (Myojin of Night's Reach).
    Divinity,
    /// Plan counters on a Plan enchantment (no rules of their own).
    Plan,
    /// Named counters with no rules of their own (CR 122.1), added for
    /// the seventh wave's cards.
    Storage,
    Verse,
    Soul,
    Blood,
    Slime,
    Stash,
    Ingenuity,
    Blaze,
    Aim,
    Void,
    Omen,
    Memory,
    Luck,
    Hour,
    Hone,
    Growth,
    Fire,
    Fate,
    Devotion,
    Collection,
    Point,
    Carrion,
    Landmark,
    Nest,
    Bloodstain,
    Invasion,
    Skewer,
    Revival,
    Rev,
    Impostor,
    Fellowship,
    Dream,
    Dread,
    Coin,
    Chorus,
    Blight,
    Bait,
    /// Keyword counter (CR 122.1b): the permanent has decayed (Rot-Curse
    /// Rakshasa).
    Decayed,
    /// Not a kind: "remove a counter from this creature" (a cost), counters
    /// of every kind. `GameObject::counters` totals them; removing them
    /// takes -1/-1 counters first, then kinds that don't change power or
    /// toughness, then the rest (the engine chooses for the player).
    Any,
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

/// What an attack or block tax applies to (`AbilityDef::CombatTax`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaxScope {
    /// Creatures attacking the ability's controller ("Creatures can't
    /// attack you"), and with `planeswalkers` those attacking planeswalkers
    /// they control ("you or planeswalkers you control"). `those` is
    /// wording only: "for each of those creatures" rather than "for each
    /// creature they control that's attacking you".
    AttackingYou { planeswalkers: bool, those: bool },
    /// The creature this Aura enchants, attacking and/or blocking; its
    /// controller pays.
    Enchanted { attack: bool, block: bool },
    /// Each creature blocking the creature this Aura enchants ("can't be
    /// blocked unless defending player pays {3} for each creature they
    /// control that's blocking it").
    EnchantedBlocked,
}

/// An attack or block tax (CR 508.1d, 509.1d): `generic` mana for each
/// creature in its scope, times `per` if any ("{1} for each card in your
/// hand", counted for the ability's controller as the declaration is
/// made), while `condition` (a pool condition) holds.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CombatTax {
    pub scope: TaxScope,
    pub generic: u32,
    pub per: Option<Num>,
    pub condition: Option<u16>,
}

impl CounterKind {
    /// Named kinds by their Oracle word ("stun", "first strike").
    pub const NAMED: [(&'static str, CounterKind); 89] = [
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
        ("mining", CounterKind::Mining),
        ("scream", CounterKind::Scream),
        ("credit", CounterKind::Credit),
        ("brick", CounterKind::Brick),
        ("currency", CounterKind::Currency),
        ("sleep", CounterKind::Sleep),
        ("net", CounterKind::Net),
        ("flood", CounterKind::Flood),
        ("glyph", CounterKind::Glyph),
        ("crystal", CounterKind::Crystal),
        ("unlock", CounterKind::Unlock),
        ("loot", CounterKind::Loot),
        ("supply", CounterKind::Supply),
        ("film", CounterKind::Film),
        ("possession", CounterKind::Possession),
        ("wish", CounterKind::Wish),
        ("incubation", CounterKind::Incubation),
        ("finality", CounterKind::Finality),
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
        ("double strike", CounterKind::DoubleStrike),
        ("divinity", CounterKind::Divinity),
        ("plan", CounterKind::Plan),
        ("storage", CounterKind::Storage),
        ("verse", CounterKind::Verse),
        ("soul", CounterKind::Soul),
        ("blood", CounterKind::Blood),
        ("slime", CounterKind::Slime),
        ("stash", CounterKind::Stash),
        ("ingenuity", CounterKind::Ingenuity),
        ("blaze", CounterKind::Blaze),
        ("aim", CounterKind::Aim),
        ("void", CounterKind::Void),
        ("omen", CounterKind::Omen),
        ("memory", CounterKind::Memory),
        ("luck", CounterKind::Luck),
        ("hour", CounterKind::Hour),
        ("hone", CounterKind::Hone),
        ("growth", CounterKind::Growth),
        ("fire", CounterKind::Fire),
        ("fate", CounterKind::Fate),
        ("devotion", CounterKind::Devotion),
        ("collection", CounterKind::Collection),
        ("point", CounterKind::Point),
        ("carrion", CounterKind::Carrion),
        ("landmark", CounterKind::Landmark),
        ("nest", CounterKind::Nest),
        ("bloodstain", CounterKind::Bloodstain),
        ("invasion", CounterKind::Invasion),
        ("skewer", CounterKind::Skewer),
        ("revival", CounterKind::Revival),
        ("rev", CounterKind::Rev),
        ("impostor", CounterKind::Impostor),
        ("fellowship", CounterKind::Fellowship),
        ("dream", CounterKind::Dream),
        ("dread", CounterKind::Dread),
        ("coin", CounterKind::Coin),
        ("chorus", CounterKind::Chorus),
        ("blight", CounterKind::Blight),
        ("bait", CounterKind::Bait),
        ("decayed", CounterKind::Decayed),
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
        if self == CounterKind::Defense {
            return "defense";
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
        if name == "defense" {
            return Some(CounterKind::Defense);
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
            CounterKind::DoubleStrike => Keyword::DoubleStrike,
            CounterKind::Decayed => Keyword::Decayed,
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
    /// use the stack, so are never targeted (CR 605.3b). `yours`: "...
    /// you control".
    OnStack {
        spells: bool,
        activated: bool,
        triggered: bool,
        source: Option<CardType>,
        #[serde(default)]
        yours: bool,
        /// "... with a single target" (CR 115.9a): one target was chosen
        /// for it as it was put on the stack.
        #[serde(default)]
        single: bool,
        /// "target instant spell, sorcery spell, activated ability, or
        /// triggered ability": a spell only if it is an instant or sorcery.
        #[serde(default)]
        instant_sorcery: bool,
        /// Printed as a list: "target spell, activated ability, or
        /// triggered ability" (the same objects as "spell or ability").
        #[serde(default)]
        listed: bool,
    },
    /// "target opponent".
    Opponent,
    /// "target opponent or planeswalker".
    OpponentOrPlaneswalker,
    /// "target artifact or creature".
    ArtifactOrCreature,
    /// "target player or battle".
    PlayerOrBattle,
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
    /// One target that is a spell or a battlefield object matching the
    /// pool's `n`th filter ("target spell or creature", "target spell or
    /// permanent", CR 115.1).
    SpellOr(u16),
    /// A card in a graveyard matching the filter ("target card from a
    /// graveyard", "target instant card from your graveyard"); `who` is
    /// whose graveyard, judged from the caster. Several such targets of
    /// one ability with `who` `Any` come from a single graveyard ("up to
    /// two target cards from a single graveyard") unless `graveyards`
    /// ("up to two target cards from graveyards").
    GraveyardCard {
        filter: u16,
        who: Who,
        graveyards: bool,
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
    /// "any other target": as `Any`, but not the source ("he deals damage
    /// ... to any other target") nor an object or player another of the
    /// ability's "target" words chose (Boulder Dash's "2 damage to any
    /// target and 1 damage to any other target").
    AnyOther,
    /// "any target that was dealt damage this turn" (Needle Drop): as
    /// `Any`, but only a player or permanent dealt damage this turn.
    AnyDamaged,
}

/// `CardDef::modes_if`: when a modal spell's larger choice is allowed.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ModesIf {
    /// The pooled condition (`CardPool::condition`).
    pub cond: u16,
    /// "choose both instead": with the condition, only the sets of more
    /// than one mode ("you may choose both instead": any).
    pub must: bool,
}

/// A variable number of targets of one kind (CR 601.2c): at least `min`,
/// at most `max` (any number if `None`); X is the spell's X.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ManyTargets {
    pub spec: TargetSpec,
    pub min: Num,
    pub max: Option<Num>,
    /// "with total mana value 3 or less": the chosen objects' printed
    /// mana values add up to at most this (CR 601.2c).
    #[serde(default)]
    pub total_mv: Option<Num>,
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
    /// Every token the most recent token-creating effect of the same
    /// resolution created ("Sacrifice those tokens at the beginning of the
    /// next end step"), those still on the battlefield.
    CreatedTokens,
    /// Every battlefield object matching the pool's `n`th interned filter
    /// ("all artifacts", "each creature with flying", "white creatures you
    /// control").
    Group(u16),
    /// One battlefield object matching the pool's `n`th interned filter,
    /// picked at random as the effect happens ("a creature an opponent
    /// controls chosen at random"), from the game's random number
    /// generator.
    RandomOf(u16),
    /// Each opponent who controls no permanent matching the pool's `n`th
    /// interned filter ("each opponent who doesn't control an Elf"),
    /// judged as the effect happens.
    OpponentsWithout(u16),
    /// Each permanent the resolution has dealt damage to so far ("A
    /// creature dealt damage this way can't be regenerated this turn", CR
    /// 608.2c), still on the battlefield.
    DamagedThisWay,
    /// What the source (an Aura or Equipment) is attached to ("enchanted
    /// creature", "equipped creature").
    Attached,
    /// The controller of what the source is attached to ("enchanted
    /// creature's controller").
    AttachedController,
    /// Each creature blocking the triggering object, or else the source
    /// ("each creature blocking it").
    BlockingIt,
    /// The `n`th target and every other battlefield object matching the
    /// pool's `g`th filter, whose `same` relation is to that target
    /// ("target creature and all other creatures with the same name as
    /// that creature", "target creature and each other creature that
    /// shares a color with it").
    TargetAndGroup(usize, u16),
    /// What the source is attached to and every other battlefield object
    /// matching the pool's `g`th filter that shares a creature type with
    /// it (CR 205.3m): "Enchanted creature and other creatures that share
    /// a creature type with it get +1/+1 until end of turn." (Crown of
    /// Vigor). Judged against the host as it is, or as it last existed.
    AttachedAndSharing(u16),
    /// The cards the source's linked ability exiled ("the exiled card",
    /// CR 607.2a: Journey to Nowhere's "When this enchantment leaves the
    /// battlefield, return the exiled card ..."), still in exile; see
    /// `Effect::ExileLinked`.
    ExiledCard,
    /// The Aura or Equipment that granted the resolving ability to the
    /// permanent it is attached to, named in the quoted ability ("Equipped
    /// creature has '{1}, {T}: Tap target creature. Return Trusty
    /// Boomerang to its owner's hand.'"). Each grant is its own ability
    /// (CR 113.2c): two Boomerangs give two, each naming its own. Only in
    /// such an ability (`AbilityDef::names_granter`).
    Granter,
    /// The cards the source's abilities exiled (CR 607.2a), still in
    /// exile, that match the pool's `n`th filter: "Return all cards
    /// exiled with this artifact to their owner's hand", "each creature
    /// card exiled with this land".
    ExiledWith(u16),
    /// The objects the most recent group of this resolution denoted
    /// (`Group`, `EachCreature`, `EachCreatureYouControl`,
    /// `EachPermanentYouControl`), those still on the battlefield: "Gain
    /// control of all artifacts your opponents control until end of turn.
    /// Untap them.", "Put a +1/+1 counter on each creature token you
    /// control. They gain trample". The same objects even if they no
    /// longer match the group's filter (CR 608.2c).
    ThoseObjects,
}

/// How long a "becomes a copy" effect lasts (CR 611.2b).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum CopyUntil {
    #[default]
    Permanent,
    /// "until end of turn" (CR 514.2).
    EndOfTurn,
    /// "until your next turn": the effect's controller's.
    YourNextTurn,
}

/// A number in an effect's text: a constant, or X as chosen when the spell
/// was cast (CR 107.3).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Num {
    N(u32),
    X,
    /// "X plus 1": X and then some (Meteor Shower).
    XPlus(u32),
    /// "twice X": X times this (Torch the Witness).
    XTimes(u32),
    /// "three minus X": this less X, never below 0 (Slumbering Trudge,
    /// CR 107.1b).
    NMinusX(u32),
    /// The `i`th of the source card's `CardDef::counts` ("the number of
    /// creatures you control"), evaluated as the effect happens
    /// (`GameState::eval_num`).
    Count(u8),
    /// "that much": the triggering event's amount (damage dealt).
    ThatMuch,
}

impl Num {
    /// One (a serde default).
    pub const fn one() -> Num {
        Num::N(1)
    }

    /// The value without game state: a count is 0 here (see
    /// `GameState::eval_num`).
    pub const fn eval(self, x: u32) -> u32 {
        match self {
            Num::N(n) => n,
            Num::X => x,
            Num::XPlus(n) => x.saturating_add(n),
            Num::XTimes(n) => x.saturating_mul(n),
            Num::NMinusX(n) => n.saturating_sub(x),
            Num::Count(_) | Num::ThatMuch => 0,
        }
    }

    /// A value for heuristics that have no X in hand.
    pub const fn hint(self) -> u32 {
        match self {
            Num::N(n) => n,
            Num::X | Num::Count(_) | Num::ThatMuch => 3,
            Num::XPlus(n) => 3 + n,
            Num::XTimes(n) => 3 * n,
            Num::NMinusX(n) => n.saturating_sub(3),
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
    /// "loses half their life, rounded up" (CR 107.1a): each recipient
    /// player loses half their life total, rounded up (none at 0 or less).
    LoseHalfLife {
        who: Recipient,
    },
    /// "You lose the game." (CR 104.3e): each recipient player loses,
    /// unless they can't lose the game.
    LoseGame {
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
    /// "Put up to X +1/+0 counters on this creature. This ability can't
    /// cause the total number of +1/+0 counters on this creature to be
    /// greater than four." (the Clockwork creatures): `count` counters, but
    /// none past `cap` in all. "Up to" lets fewer be chosen; the engine
    /// always puts as many as it may.
    AddCountersCapped {
        kind: CounterKind,
        count: Num,
        to: Recipient,
        cap: u32,
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
    /// "Remove a counter from target permanent": one counter of a kind
    /// the controller chooses among those on it. With `up_to` N, "remove
    /// up to three counters from target creature": up to N counters, one
    /// at a time, each of a kind chosen (or stop).
    RemoveCounterChosen {
        to: Recipient,
        #[serde(default)]
        up_to: u8,
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
    /// The recipient player exiles the top card of their library (ingest,
    /// CR 702.115a), or the top `count` cards ("Exile the top three cards
    /// of your library.", "Each opponent exiles the top card of their
    /// library.").
    ExileTop {
        who: Recipient,
        #[serde(default = "Num::one")]
        count: Num,
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
    /// "Change the target of target spell or ability with a single
    /// target" (CR 115.7a): the effect's controller changes that object's
    /// target to another one legal for it, if there is one; otherwise it
    /// is unchanged.
    ChangeTarget {
        what: Recipient,
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
        /// Who scries: the controller, or each player or opponent in turn
        /// order ("each player may scry 1", CR 101.4), each looking at
        /// their own library.
        who: Recipient,
    },
    /// Surveil N (CR 701.42): like scry, into the graveyard instead.
    Surveil {
        count: Num,
    },
    /// "Search your library for a basic land card, reveal it, put it into
    /// your hand, then shuffle" (CR 701.19): up to `count` cards matching
    /// `filter` (judged on their printed characteristics); the searcher
    /// may fail to find (CR 701.19b). `who` searches their own library
    /// ("Its controller may search their library ...": with `may`, they
    /// may decline before finding anything, and then don't shuffle);
    /// `reveal`: what is found is revealed ("reveal it"), so a card put
    /// into a hand is known to every player until it moves.
    Search {
        filter: ObjFilter,
        count: Num,
        to: SearchTo,
        who: Recipient,
        may: bool,
        reveal: bool,
        /// "search your library and/or graveyard": the graveyard's cards
        /// may be found too; the library is shuffled only if a card was
        /// found there or none was found at all.
        #[serde(default)]
        graveyard: bool,
        /// "a card named Ajani, Valiant Protector": only cards with this
        /// name (another card's, so by name rather than definition); "a
        /// card named X or Y" joins the names with `|`.
        #[serde(default)]
        name: Option<Box<str>>,
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
    /// "Target opponent exiles a creature they control": the recipient
    /// player chooses `count` of their permanents matching the filter and
    /// exiles them.
    ExileChosen {
        who: Recipient,
        filter: ObjFilter,
        count: Num,
    },
    /// "Target opponent exiles two cards from their hand" (with
    /// `graveyard`, "a card from their graveyard"): each recipient player
    /// chooses `count` cards there and exiles them.
    ExileFromHand {
        count: Num,
        who: Recipient,
        graveyard: bool,
    },
    /// "Draw two cards, then discard one of them.": the controller draws
    /// `draw` cards, then discards `discard` of the cards drawn this way.
    DrawDiscardDrawn {
        draw: Num,
        discard: Num,
    },
    /// "Target opponent reveals their hand. You choose a nonland card from
    /// that player's graveyard or hand and exile it.": the recipient
    /// reveals their hand, then the controller picks one card matching
    /// `filter` among the recipient's graveyard and hand.
    ExileFromGraveyardOrHand {
        who: Recipient,
        filter: ObjFilter,
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
    /// "Clash with an opponent. If you win, ..." (CR 701.23): the
    /// controller and an opponent each reveal the top card of their
    /// library and put it on the top or bottom; the controller wins if
    /// their card's mana value is greater, and then `won` happens.
    Clash {
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
        /// "You choose a nonland card from it and exile that card": the
        /// chosen card is exiled rather than discarded.
        #[serde(default)]
        exile: bool,
        /// "Exile that card until this creature leaves the battlefield"
        /// (CR 610.3): exiled linked to the source, returning to its
        /// owner's hand when the source leaves; nothing is exiled if the
        /// source has already left (CR 610.3a).
        #[serde(default)]
        until: bool,
        /// "Put that card on top of that player's library." (Painful
        /// Memories): the chosen card goes there instead.
        #[serde(default)]
        top: bool,
        /// "Target player reveals three cards from their hand and you
        /// choose one of them" (Blackmail): the player reveals that many
        /// cards of their choice (all of them with no more in hand), and
        /// the controller chooses the second number of them from those.
        #[serde(default)]
        reveal: Option<(Num, u8)>,
    },
    /// "Transform this creature" (CR 701.27): a double-faced permanent
    /// turns to its other face; it stays the same object.
    Transform {
        what: Recipient,
    },
    /// "If you both own and control this creature and a creature named
    /// `partner`, exile them, then meld them into `into`." (CR 701.42a):
    /// both exiled, then, if both are cards of the pair, put onto the
    /// battlefield as one permanent with `into`'s characteristics
    /// (`GameState::melded`); otherwise they stay exiled (CR 701.42c).
    Meld {
        partner: Box<str>,
        into: Box<str>,
    },
    /// "Flip this creature" (CR 710): an unflipped flip card on the
    /// battlefield gets its flipped half's characteristics, for as long as
    /// it stays there (CR 710.4).
    Flip {
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
        count: Num,
        colors: ColorSet,
        different: bool,
    },
    /// "Add two mana of any one color and two mana of any other color":
    /// `.0` mana of a color chosen, then `.0` of another color chosen.
    AddManaTwoColors(u8),
    /// "Add one mana of any color that a land an opponent controls could
    /// produce" (CR 106.7): one mana of a kind the mana abilities of
    /// `whose` lands (`basic`: basic lands only) could make, judged as it
    /// resolves, ignoring costs and whether they could be activated.
    /// None, no mana.
    AddManaLandCould {
        whose: Who,
        basic: bool,
        /// "one mana of any type": colorless too, when no color is
        /// possible (Reflecting Pool, Naga Vitalist).
        #[serde(default)]
        any_type: bool,
    },
    /// "Add one mana of any color in your commander's color identity."
    /// The game has no commanders, so it adds no mana (CR 903.4f). `rider`
    /// is wording only, a rider about spending that mana (which never
    /// exists): 1, Opal Palace's "If you spend this mana to cast your
    /// commander, ..."; 2, Path of Ancestry's "When that mana is spent to
    /// cast a creature spell that shares a creature type with your
    /// commander, scry 1."
    AddManaCommander {
        rider: u8,
    },
    /// The recipient player discards N cards of their choice (CR 701.8).
    Discard {
        count: Num,
        who: Recipient,
    },
    /// "Put a card from your hand on the bottom of your library", "target
    /// opponent puts a card from their hand on top of their library",
    /// "put two cards from your hand on top of your library in any
    /// order": each recipient player chooses `count` cards from their hand
    /// one at a time, each put on top of (or under) the ones before, so
    /// the order is theirs (CR 401.4). With `shuffle`, the library is
    /// shuffled afterwards ("shuffle a card from your hand into your
    /// library", CR 701.24).
    HandToLibrary {
        count: Num,
        who: Recipient,
        bottom: bool,
        shuffle: bool,
    },
    /// "Shuffle your graveyard into your library", "Target player shuffles
    /// their graveyard into their library", "Shuffle all creature cards
    /// from target player's graveyard into that player's library",
    /// "shuffle the cards from your hand into your library": each
    /// recipient player's cards in that zone (from the hand with `hand`,
    /// else the graveyard), all or those matching `filter`, go into their
    /// library, which is then shuffled (CR 701.24), even with no card
    /// moved. "Shuffled this way" counts them (`ThisWay::Shuffled`).
    ShuffleIntoLibrary {
        who: Recipient,
        hand: bool,
        /// "their hand and graveyard": both zones (`hand` is then true).
        #[serde(default)]
        both: bool,
        filter: Option<ObjFilter>,
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
    /// unlike fight (CR 701.12). With `twice`, "damage equal to twice its
    /// power". Each object of `source` deals its own damage ("Up to two
    /// target creatures you control each deal damage equal to their power
    /// to another target creature").
    Bite {
        source: Recipient,
        to: Recipient,
        twice: bool,
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
    /// Exile every card in the recipient players' graveyards ("Exile target
    /// player's graveyard", "Exile each opponent's graveyard"); with a
    /// filter, those matching it ("Exile all creature cards from target
    /// player's graveyard").
    ExileGraveyard {
        who: Recipient,
        filter: Option<Box<ObjFilter>>,
    },
    /// Create a replacement or prevention effect that lasts until end of
    /// turn (CR 614.3, 615.3): "Prevent all combat damage that would be
    /// dealt this turn", "Prevent the next 4 damage that would be dealt to
    /// any target this turn" (a shield, CR 615.7).
    CreateReplacement(ReplacementAbility),
    /// A "can't" effect that lasts until end of turn (CR 614.17).
    Rule(RuleKind),
    /// "Each player puts a creature card from their graveyard onto the
    /// battlefield": each player `who` names, in turn order, chooses a card
    /// matching `filter` in their own graveyard (CR 101.4); then the
    /// chosen cards enter together under their owners' control.
    EachPutsFromGraveyard {
        who: Recipient,
        filter: ObjFilter,
    },
    /// "For each opponent who can't, you draw a card": these effects once
    /// for each player who couldn't do what the effect before asked
    /// (`Effect::Discard`: had too few cards to discard).
    ForEachCouldnt(Vec<Effect>),
    /// A "can't" effect on each player `who` names, until end of turn:
    /// "Target player can't play lands this turn."
    PlayerRule {
        who: Recipient,
        kind: PlayerRuleKind,
    },
    Tap {
        to: Recipient,
    },
    /// "The next spell you cast this turn costs {1} less to cast" (CR
    /// 601.2f): the controller's next spell this turn costs that much less
    /// generic mana; several such effects add up, and that one spell uses
    /// them all up.
    NextSpellLess(u32),
    /// "The next red or green creature spell you cast this turn can be
    /// cast without paying its mana cost." (CR 118.9): the controller's
    /// next spell this turn matching the filter may be cast from their
    /// hand without paying its mana cost (`Choice::CastFree`); the next
    /// such spell uses it up, however it is cast.
    NextSpellFree(ObjFilter),
    /// "The next spell you cast this turn can't be countered" (CR 101.2):
    /// the controller's next spell this turn is uncounterable.
    NextSpellUncounterable,
    /// "You may play an additional land this turn." (CR 305.2): the
    /// controller may play `n` more lands this turn ("up to two additional
    /// lands").
    ExtraLandPlays(u32),
    /// "Face-down spells you cast this turn cost {1} less to cast." (CR
    /// 601.2f): the controller's face-down spells (CR 702.37c) this turn.
    FaceDownSpellsLess(u32),
    /// "Turn target face-down creature an opponent controls face up" (CR
    /// 708.8): no cost is paid, so megamorph adds no counter (CR
    /// 702.37b); one whose card is an instant or sorcery is revealed and
    /// stays face down (CR 701.40g).
    TurnFaceUp {
        what: Recipient,
    },
    /// "Turn target creature with a morph ability face down" (CR 708.2a):
    /// a 2/2 face-down creature with no text; a face-down or double-faced
    /// permanent is unaffected (CR 708.2b, 712.16).
    TurnFaceDown {
        what: Recipient,
    },
    /// "Look at target face-down creature" (CR 708.5): the controller
    /// learns what it is; information only.
    LookAtFaceDown {
        what: Recipient,
    },
    /// "Look at target player's hand, the top card of that player's
    /// library, and any face-down creatures they control." (CR 708.5):
    /// the controller learns those cards; information only.
    LookAtPlayer {
        who: Recipient,
    },
    /// "Tap or untap target permanent": its controller (the ability's)
    /// chooses as it resolves.
    TapOrUntap {
        to: Recipient,
    },
    /// "Search its controller's graveyard, hand, and library for all
    /// cards with the same name as that spell and exile them. Then that
    /// player shuffles." (Counterbore, Eradicate): the object's controller
    /// and name as it last existed (CR 608.2h); every card of that name
    /// in those zones is exiled (CR 701.19b lets a searcher fail to find
    /// them; the engine finds them all), then that library is shuffled.
    ExileSameName {
        what: Recipient,
    },
    Untap {
        to: Recipient,
    },
    /// "Remove it from combat" (CR 506.4): an attacker or blocker stops
    /// being one; an attacker it was blocking stays blocked (CR 509.1h).
    RemoveFromCombat {
        what: Recipient,
    },
    /// "Target unblocked attacking creature becomes blocked" (CR 509.1h,
    /// 506.4): an attacking creature not yet blocked is blocked with no
    /// blocker, so it assigns no combat damage unless it has trample (CR
    /// 702.19e); "becomes blocked" triggers. Works on creatures that can't
    /// be blocked.
    BecomeBlocked {
        what: Recipient,
    },
    /// "gains haste until end of turn" (layer 6).
    Grant {
        keywords: KeywordSet,
        to: Recipient,
    },
    /// "Gain control of target creature until end of turn" (layer 2,
    /// CR 613.1b); with `forever`, indefinitely ("Gain control of target
    /// creature.").
    GainControl {
        of: Recipient,
        forever: bool,
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
        /// "Create a Wicked Role token attached to target creature you
        /// control": an Aura token enters attached to the first object
        /// of this recipient, and isn't created if there is none it could
        /// enchant (CR 303.4f).
        attached: Option<Recipient>,
    },
    /// Attach `what` (an Aura or Equipment; `Source` for the ability's
    /// source, as equip does) to the first object of `to` (CR 701.3).
    /// "Attach target Equipment you control to target creature you
    /// control" names another object.
    Attach {
        what: Recipient,
        to: Recipient,
    },
    /// "Unattach all Equipment from target creature" (CR 701.3d): each
    /// object stays on the battlefield, attached to nothing.
    Unattach {
        what: Recipient,
    },
    /// "becomes a 2/2 Assembly-Worker artifact creature until end of turn.
    /// It's still a land" (layers 4, 5, 6 and 7b). The card types it
    /// becomes replace its others (CR 205.1a; Opal Caryatid stops being an
    /// enchantment) except land: every Oracle text animating a land says
    /// "It's still a land" (CR 205.1b). The creature types replace its
    /// others (CR 205.1a): "becomes a Dragon ... (It loses all other
    /// creature types.)".
    Animate {
        to: Recipient,
        types: TypeSet,
        subtypes: SubtypeSet,
        /// Its new base power and toughness; `None` keeps them ("This
        /// Vehicle becomes an artifact creature until end of turn").
        pt: Option<(i32, i32)>,
        /// "becomes a 4/4 green Bear artifact creature": its colors.
        colors: Option<ColorSet>,
        /// "becomes a 2/2 Gargoyle creature with flying" (layer 6).
        keywords: KeywordSet,
        /// No duration: "it becomes a 2/2 Soldier creature" (Opal Caryatid)
        /// lasts indefinitely.
        indefinite: bool,
        /// "loses all abilities and becomes a green Snake" (Snakeform):
        /// layer 6 removes its abilities, before the effect's keywords.
        loses_abilities: bool,
        /// "becomes a black Demon in addition to its other colors and
        /// types": the types, subtypes, and colors are added (layers 4 and
        /// 5) instead of replacing its others.
        in_addition: bool,
    },
    /// "becomes blue until end of turn", "becomes colorless" (layer 5).
    BecomeColor {
        to: Recipient,
        colors: ColorSet,
        /// No duration ("That creature becomes green. (This effect lasts
        /// indefinitely.)").
        indefinite: bool,
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
        /// "becomes that color" (the color of the mana the ability just
        /// added: "Add one mana of any color. This creature becomes that
        /// color until end of turn."); wording only.
        #[serde(default)]
        color: bool,
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
        /// "becomes the creature type of your choice in addition to its
        /// other types": the chosen type is added rather than replacing.
        in_addition: bool,
    },
    /// "You may [do these]": the controller decides on resolution.
    YouMay(Vec<Effect>),
    /// "That player may pay {4}", "that player may discard a card at
    /// random": the player `who` names decides on resolution and does
    /// these as their own (CR 608.2d); "If the player does" then sees
    /// whether they did.
    PlayerMay {
        who: Recipient,
        effects: Vec<Effect>,
    },
    /// "Any player may pay 5 life", "any opponent may sacrifice a creature
    /// of their choice" (CR 608.2d): starting with the active player and in
    /// turn order (CR 101.4), each player (each opponent, with
    /// `opponents`) may do these as their own, until one does; "If a
    /// player does" / "If no one does" then see whether one did.
    AnyPlayerMay {
        opponents: bool,
        effects: Vec<Effect>,
    },
    /// "pay {2}" / "pay 2 life", inside "you may": it happens only if the
    /// controller can pay; "If you do" then sees whether it did.
    Pay {
        mana: ManaCost,
        life: u32,
    },
    /// "If you do, [these]": only if the optional action before happened
    /// (CR 608.2c).
    IfYouDo(Vec<Effect>),
    /// "Otherwise, [these]" (CR 608.2c): only if the conditional sentence
    /// just before ("If you control an Eldrazi Spawn, ...", "If you win,
    /// ...") didn't happen. Those set the resolution's `did` to whether
    /// they did.
    Otherwise(Vec<Effect>),
    /// "When you do, [this]": a reflexive triggered ability (CR 603.12),
    /// registered in the pool (`CardPool::register_ability`, trigger
    /// `TriggerSpec::Reflexive`), that triggers if the optional action
    /// before happened; its targets are chosen as it is put on the stack.
    Reflexive(AbilityId),
    /// "[these] unless [payer] pays {2}": the payer may pay; if they
    /// don't, the effects happen. `discard` cards of the payer's choice
    /// are part of the payment ("unless that player discards a card",
    /// ward's and echo's "—Discard a card.").
    UnlessPays {
        payer: Recipient,
        mana: ManaCost,
        life: u32,
        discard: u32,
        /// "Ward—Waterbend {4}" (CR 701.67a): the payer may tap their
        /// untapped artifacts and creatures for the generic part.
        waterbend: bool,
        /// "Ward—Discard a card at random." (CR 701.9b).
        discard_random: bool,
        /// "Ward—{2}, Pay 2 life.": the mana and the life together;
        /// otherwise mana with life is "{1} or 1 life", either.
        #[serde(default)]
        both: bool,
        effects: Vec<Effect>,
    },
    /// A ward cost of any kind (CR 702.21a: "counter it unless that player
    /// pays [cost]"): "Ward—Sacrifice three permanents.", "Ward—Pay life
    /// equal to this creature's power.", "Ward—Get five poison counters.",
    /// "Ward—Collect evidence 4.", "Ward—Discard a card or pay {2}.". The
    /// payer may pay one of the alternatives in full, chosen as it
    /// resolves (a cost can't be paid in part, CR 118.3); if they don't,
    /// the effects happen.
    UnlessPaysCost {
        payer: Recipient,
        alts: Vec<UnlessCost>,
        effects: Vec<Effect>,
    },
    /// "... unless they sacrifice a nonland permanent of their choice or
    /// discard a card" (a punisher, CR 608.2d): the payer may sacrifice a
    /// permanent matching `filter` (or, with `or_discard`, discard a
    /// card) to stop `effects`.
    UnlessSacrifices {
        payer: Recipient,
        filter: ObjFilter,
        or_discard: bool,
        effects: Vec<Effect>,
    },
    /// "Sacrifice it unless you discard a card at random", "... unless you
    /// return two Forests you control to their owner's hand": the
    /// controller may do `act` (CR 608.2d, chosen as it resolves); unless
    /// they do (and can do it all), `effects` happen.
    UnlessYou {
        act: Vec<Effect>,
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
    /// "have base power and toughness X/X until end of turn" (layer 7b):
    /// both `n`, evaluated as the effect resolves (Biomass Mutation).
    SetBasePtVar {
        to: Recipient,
        n: Num,
    },
    /// "has base power 4 until end of turn", "this creature's base power
    /// becomes equal to the number of Towns you control" (layer 7b, CR
    /// 613.4b): the power only, `n` fixed as the effect resolves (CR
    /// 608.2h).
    SetBasePower {
        to: Recipient,
        n: Num,
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
    /// Haunt (CR 702.55a): "exile it haunting target creature": the
    /// source card, from its owner's graveyard, is exiled and haunts the
    /// target: its `CardDef::haunted` ability becomes a delayed trigger
    /// about that creature (702.55b-c).
    ExileHaunting,
    /// Champion (CR 702.72a): "sacrifice it unless you exile another
    /// [object] you control": the controller may choose another permanent
    /// they control matching the filter; it is exiled, linked to the
    /// source (its "When this leaves the battlefield, return the exiled
    /// card" returns it, 702.72b), or else the source is sacrificed.
    Champion(ObjFilter),
    /// Exile a permanent.
    Exile {
        what: Recipient,
    },
    /// Exile a permanent, remembering it as exiled by the source (CR
    /// 607.2a): for its other ability's "the exiled card"
    /// (`Recipient::ExiledCard`, Journey to Nowhere), the compiler turns
    /// the card's `Exile`s into these. With `until`, "exile target
    /// creature until this enchantment leaves the battlefield" (CR 610.3):
    /// nothing is exiled if the source has already left (610.3a), and the
    /// card returns under its owner's control (610.3c) as soon as the
    /// source leaves.
    ExileLinked {
        what: Recipient,
        until: bool,
    },
    /// Return a card from exile or a graveyard to the battlefield under its
    /// owner's control (or, `under_you`, the ability's controller's:
    /// "return that card to the battlefield under your control"),
    /// optionally tapped or with counters on it. An Aura with `attached`
    /// enters attached to that permanent ("return this card from your
    /// graveyard to the battlefield attached to that creature", CR
    /// 303.4f), or stays put if it can't enchant it (CR 303.4g).
    ReturnToBattlefield {
        what: Recipient,
        counters: Option<(CounterKind, u32)>,
        tapped: bool,
        under_you: bool,
        #[serde(default)]
        attached: Option<Recipient>,
        /// "with a hexproof counter and an indestructible counter on it":
        /// counters of a second kind.
        #[serde(default)]
        more: Option<(CounterKind, u32)>,
        /// "If it enters as a creature, it enters with an additional +1/+1
        /// counter on it." (Teferi's Time Twist): the counters only on a
        /// card that will be a creature (judged by its card).
        #[serde(default)]
        if_creature: bool,
        /// "Return it to the battlefield transformed under your control"
        /// (CR 712.14a): a transforming double-faced card enters with its
        /// back face up; any other card stays where it is.
        #[serde(default)]
        transformed: bool,
    },
    /// The ability's source becomes a copy of the recipient (CR 707),
    /// keeping its own definition's `copy_except` if `except`. With `who`,
    /// those objects become copies instead ("target artifact becomes a
    /// copy of ...", "each other creature becomes a copy of ..."); `until`
    /// ends the copy effect ("until end of turn", "until your next turn").
    BecomeCopy {
        of: Recipient,
        except: bool,
        #[serde(default)]
        who: Option<Recipient>,
        #[serde(default)]
        until: CopyUntil,
    },
    /// Create a token that is a copy of the recipient (CR 707.2, 111.4),
    /// with the source definition's `copy_except` if `except`.
    CreateTokenCopy {
        of: Recipient,
        except: bool,
        /// "create two tokens that are copies of it": how many for each
        /// object (CR 111.4).
        count: Num,
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
    /// "Pay {E}{E}", inside "you may": like `Pay`. `PAY_ANY_ENERGY`: "you
    /// may pay any amount of {E}", the amount chosen as it resolves; later
    /// "X" and "that much" in the resolution are the amount paid.
    PayEnergy(u32),
    /// "Level 2" (a Class's level ability, CR 716.2a: the stage is the
    /// level less one) or "it becomes solved" (a Case's to-solve ability,
    /// CR 719.3a: stage 1): the source becomes that stage of its card
    /// (`CardDef::stages`) if it is at an earlier one, gaining the
    /// abilities the stage adds (CR 716.2b, 719.3b).
    BecomeStage(u8),
    /// "You become the monarch" (CR 725): `You`, or for the monarch's own
    /// combat-damage trigger `TriggeringController` (CR 725.2).
    BecomeMonarch {
        who: Recipient,
    },
    /// "You take the initiative" (CR 726): `You`, or for the initiative's
    /// combat-damage trigger `TriggeringPlayer`. Taking it triggers a
    /// venture into Undercity (CR 726.2), even for its holder (726.5).
    TakeInitiative {
        who: Recipient,
    },
    /// Venture into the dungeon (CR 701.49); `undercity`: venture into
    /// Undercity (CR 701.49d, the initiative's).
    Venture {
        undercity: bool,
    },
    /// The Ring tempts you (CR 701.54).
    RingTempts,
    /// The speed trigger's effect (CR 702.179d): speed increases by 1, to
    /// at most 4. Built-in only.
    IncreaseSpeed,
    /// Ascend on an instant or sorcery (CR 702.131a): the city's blessing
    /// if the controller controls ten or more permanents.
    Ascend,
    /// "Target creature gets -4/-0 until your next turn", "can't attack
    /// until your next turn", goad (CR 701.15): an effect on the recipient
    /// that ends as the controller's next turn begins (CR 611.2b).
    UntilYourNextTurn {
        to: Recipient,
        parts: Vec<Modification>,
    },
    /// "<effects> until your next turn", "... for as long as this creature
    /// remains on the battlefield" (CR 611.2b): the effects, each
    /// continuous effect they would create until end of turn lasting
    /// `lasting` instead. One whose duration is linked to the source does
    /// nothing if the source has already left (CR 611.2b).
    Lasting {
        lasting: Lasting,
        effects: Vec<Effect>,
    },
    /// Copy the source spell (CR 707.10), as it is on the stack or as it
    /// last existed there; its controller may choose new targets for the
    /// copy (CR 707.10c). Conspire's trigger (`Builtin::Conspire`).
    CopySpell,
    /// "Copy target instant or sorcery spell you control", "copy that
    /// spell", "copy it X times" (CR 707.10): `times` copies of each
    /// recipient spell (as it is on the stack, or as it last existed
    /// there), controlled by the ability's controller. With `new_targets`
    /// ("You may choose new targets for the copy"), the copy's controller
    /// may change its targets (CR 707.10c); with `except`, the source
    /// definition's `copy_except` applies to the copy ("except the copy
    /// isn't legendary", CR 707.9b). A copy of a permanent spell becomes a
    /// token as it resolves (CR 608.3f).
    CopySpells {
        what: Recipient,
        times: Num,
        new_targets: bool,
        except: bool,
    },
    /// Goad (CR 701.15): until the controller's next turn, each recipient
    /// creature attacks each combat if able and attacks a player other
    /// than the controller if able.
    Goad {
        what: Recipient,
    },
    /// "If it would leave the battlefield, exile it instead of putting it
    /// anywhere else" (unearth, CR 702.84a): each recipient on the
    /// battlefield is marked (`OBJ_EXILE_ON_LEAVE`) and exiled instead
    /// whenever it would leave for another zone.
    ExileOnLeave {
        what: Recipient,
    },
    /// "Each player loses 2 life unless they discard a card" (`unless`
    /// `None`) / "... unless they sacrifice a creature, artifact, or land
    /// of their choice": each player in turn order chooses (CR 101.4).
    PunishEach {
        life: u32,
        unless: Option<ObjFilter>,
    },
    /// "Draw three cards and reveal them. You may cast one of them without
    /// paying its mana cost." (Mad Wizard's Lair).
    DrawCastOne {
        count: u32,
    },
    /// "You may cast a spell from your hand with mana value less than or
    /// equal to that damage without paying its mana cost", "you may cast
    /// target instant or sorcery card from your graveyard without paying
    /// its mana cost" (CR 608.2g: cast as the effect finishes, timing
    /// ignored). `what` names the card (a target); without it, the
    /// controller chooses a card in their hand matching `filter` with mana
    /// value at most `max_mv`. With `exile`, "If that spell would be put
    /// into your graveyard, exile it instead."
    CastFree {
        what: Option<Recipient>,
        filter: Option<ObjFilter>,
        max_mv: Option<Num>,
        exile: bool,
    },
    /// "Reveal the top ten cards of your library. Put a creature card from
    /// among them onto the battlefield with three +1/+1 counters on it.
    /// ... Then shuffle." (Throne of the Dead Three): the creature is
    /// `Recipient::CreatedObject` afterwards.
    RevealPutCreature {
        count: u32,
        counters: u32,
    },
    /// Proliferate (CR 701.27): the controller adds one of each kind of
    /// counter already there to the permanents and players they choose;
    /// the engine chooses their own permanents' and players' beneficial
    /// counters and opponents' harmful ones (+1/+1 and loyalty on yours,
    /// -1/-1 on theirs, poison on opponents). "Proliferate X times":
    /// `times` separate proliferates, each its own event (CR 701.27).
    Proliferate {
        times: Num,
    },
    /// The recipient creature explores (CR 701.44): its controller reveals
    /// the top card of their library; a land goes to their hand, otherwise
    /// the creature gets a +1/+1 counter and the card may go to the
    /// graveyard. It explores `times` times ("it explores X times"),
    /// each a separate explore (CR 701.44).
    Explore {
        what: Recipient,
        times: Num,
    },
    /// The recipient permanent endures `count` (CR 701.63): its
    /// controller puts that many +1/+1 counters on it or creates a
    /// `count`/`count` white Spirit creature token (`token`; for a count
    /// not known until it resolves, a 0/0 Spirit given that base power
    /// and toughness). Endure 0 does nothing (CR 701.63b).
    Endure {
        what: Recipient,
        count: Num,
        token: CardRef,
    },
    /// "Target creature phases out" (CR 702.26b): it phases in before its
    /// controller untaps during their next untap step.
    PhaseOut {
        what: Recipient,
    },
    /// Amass N (CR 701.47): put N +1/+1 counters on an Army the
    /// controller controls, creating `token` (a 0/0 Army) first if they
    /// control none.
    Amass {
        count: Num,
        token: CardRef,
    },
    /// Empower Jace N: put N loyalty counters on a Jace planeswalker
    /// token the controller controls, creating `token` (the blue Jace
    /// with 0 loyalty, "[−1]: Surveil 1" and "[−3]: Draw a card.") first
    /// if they control none. A nontoken Jace doesn't count.
    EmpowerJace {
        count: Num,
        token: CardRef,
    },
    /// The source's echo has been dealt with (CR 702.30a).
    ClearEcho,
    /// "You may cast this card for as long as it remains exiled" (CR
    /// 601.3): the source's card, in exile (exiled as the ability's cost),
    /// may be cast by the ability's controller while it stays there.
    MayCastThisFromExile,
    /// Warp's delayed trigger (CR 702.185a, `Builtin::WarpExile`): exile
    /// the permanent; its owner may cast it from exile on a later turn.
    WarpExile,
    /// Ripple's trigger (CR 702.60a, `Builtin::Ripple`), N from the
    /// source's card.
    Ripple,
    /// "Look at the top four cards of your library. You may reveal a
    /// creature card from among them and put it into your hand. Put the
    /// rest on the bottom of your library in any order." `filter` None is
    /// any card ("Put one of them into your hand"); `all` takes every
    /// match ("Put all creature cards revealed this way into your hand");
    /// the rest go to the bottom or, with `rest_graveyard`, the graveyard.
    /// `take` is how many may be taken (`u32::MAX`: any number: "You may
    /// reveal any number of creature cards"); `also` takes one more
    /// matching it ("a creature card and/or a land card"); `random` puts
    /// the rest on the bottom in a random order. With `top` the card taken
    /// stays on top of the library instead of going to the hand ("You may
    /// put one of those cards back on top of your library. Put the rest
    /// into your graveyard."). With `onto` the cards taken go onto the
    /// battlefield under the controller's control instead, tapped with
    /// `Some(true)` ("You may put a creature card from among them onto the
    /// battlefield", "Put up to two land cards ... onto the battlefield
    /// tapped").
    Dig {
        count: Num,
        filter: Option<ObjFilter>,
        optional: bool,
        all: bool,
        rest_graveyard: bool,
        take: u32,
        also: Option<ObjFilter>,
        random: bool,
        top: bool,
        #[serde(default)]
        onto: Option<bool>,
    },
    /// "Look at the top three cards of your library. Put one of those cards
    /// into your hand, one on top of your library, and one on the bottom of
    /// your library." (Telling Time), "Look at the top two cards of target
    /// opponent's library, then exile one of them.", "Reveal the top three
    /// cards of your library. An opponent chooses one of them. Put that
    /// card into your graveyard and the rest into your hand.": the top
    /// `count` cards of `of`'s library (revealed with `reveal`); for each
    /// of `piles` in turn the chooser (the controller, or with `opponent`
    /// an opponent) picks one card still among them and it goes there; the
    /// cards left go to `rest`, or stay where they are with none. A card
    /// put on top stays on top once the others have gone.
    DigSplit {
        count: Num,
        of: Recipient,
        reveal: bool,
        opponent: bool,
        piles: Vec<SplitDest>,
        rest: Option<SplitDest>,
    },
    /// "Exile the top two cards of your library. Until the end of your next
    /// turn, you may play those cards." (impulse draw): the controller may
    /// play the exiled cards, paying their costs and following the usual
    /// timing rules and land-play allowance (CR 305.2, 601.3), for as long
    /// as `until` says.
    /// With `choose`, "Choose one of them.": only the card the
    /// controller chooses may be played. With `from`, the cards come from
    /// that player's library ("Target opponent exiles the top card of
    /// their library. You may play that card this turn.").
    ExilePlayable {
        count: Num,
        until: PlayUntil,
        #[serde(default)]
        choose: bool,
        #[serde(default)]
        from: Option<Recipient>,
    },
    /// "Look at the top three cards of your library, then put them back in
    /// any order" (the engine keeps the order). `who` owns the library
    /// ("target player's library") and `looker` looks ("That player looks
    /// at the top three cards of your library"): the looker knows those
    /// cards where they are (`GameObject::revealed`).
    LookAtTop {
        count: Num,
        who: Recipient,
        looker: Recipient,
    },
    /// "Reveal the top card of your library" (CR 701.20) or, with `look`,
    /// "look at the top card of your library": the card stays where it is
    /// and becomes the resolution's card ("it", "that card":
    /// `Recipient::CreatedObject`), for the `IfCard` and `PutCard` effects
    /// after it. Each recipient player's library is looked at in turn; the
    /// last one is the card. Every player knows a revealed card, only the
    /// controller a looked-at one, until it moves or its library is
    /// shuffled (`GameObject::revealed`).
    RevealTop {
        who: Recipient,
        look: bool,
    },
    /// "Exile cards from the top of your library until you exile a
    /// nonland card" ("Target opponent exiles cards from the top of their
    /// library until they exile a nonland card"): the recipient player
    /// exiles from the top until a card matching `filter` is exiled or the
    /// library runs out. The cards stay exiled; the matching card is the
    /// resolution's card ("that card").
    ExileUntilCard {
        who: Recipient,
        filter: ObjFilter,
    },
    /// "You may cast that card this turn.", "Until the end of your next
    /// turn, you may play that card.", "Until end of turn, you may cast
    /// that card without paying its mana cost.": the resolution's card, in
    /// exile, may be played by the controller for as long as `until`
    /// says, paying its costs with the usual timing (CR 601.3); with
    /// `cast`, only cast (not played as a land); with `free`, without
    /// paying its mana cost (CR 118.9).
    MayPlayCard {
        until: PlayUntil,
        cast: bool,
        free: bool,
        /// "You may cast target Zombie creature card from your graveyard
        /// this turn.": the targeted cards instead, where they are (a
        /// graveyard or exile).
        #[serde(default)]
        what: Option<Recipient>,
        /// "you may cast it from your graveyard as an Adventure" (CR
        /// 715.3): only its Adventure half.
        #[serde(default)]
        adventure: bool,
    },
    /// "Reveal cards from the top of your library until you reveal a land
    /// card" (CR 701.20): the recipient player reveals from the top until a
    /// card matching `filter` is revealed or the library runs out. The
    /// revealed cards stay in the library, known to every player, and are
    /// the resolution's revealed cards (`PutCard` with `CardSet::Revealed`);
    /// the matching card, if any, is the resolution's card ("that card").
    RevealUntil {
        who: Recipient,
        filter: ObjFilter,
    },
    /// Move counters from one permanent onto others (CR 122.5): "Move all
    /// counters from target creature onto another target creature", "Move
    /// a +1/+1 counter from this artifact onto target creature" (`kind`
    /// None is every kind, `count` None every counter). With the source
    /// gone from the battlefield, "put its counters on target creature you
    /// control" puts the counters it had as it last existed (CR 608.2h).
    /// Nothing moves if there is nowhere to put them.
    MoveCounters {
        from: Recipient,
        to: Recipient,
        kind: Option<CounterKind>,
        /// `None`: all of them; `Some(MOVE_ANY_NUMBER)`: "any number of",
        /// chosen as it resolves.
        count: Option<u32>,
    },
    /// "Then that player shuffles" (CR 701.24): each recipient player
    /// shuffles their library.
    ShuffleLibrary {
        who: Recipient,
    },
    /// "If it's a land card, [these]" / "If it isn't ...": whether the
    /// resolution's card (`RevealTop`, `RevealUntil`, or what the
    /// previous effect moved) matches `filter`, judged where it is; no
    /// card matches nothing either way. Sets the resolution's `did`, for
    /// an "Otherwise" after it.
    IfCard {
        filter: ObjFilter,
        negated: bool,
        /// Kinship's "If it shares a creature type with this creature"
        /// (CR 205.3m, changeling's every type included), as well as
        /// matching `filter`.
        #[serde(default)]
        shares_type: bool,
        /// "If a creature card is revealed this way, ...": the card must
        /// also be revealed to every player where it is (an optional
        /// "You may reveal that card." declined leaves it unrevealed).
        #[serde(default)]
        revealed: bool,
        then: Vec<Effect>,
    },
    /// "you may reveal it": every player knows the resolution's card
    /// where it is (CR 701.20).
    RevealCard,
    /// "Draw a card and reveal it": the card drawn is revealed, and is the
    /// resolution's card ("it") after.
    DrawReveal,
    /// "put it onto the battlefield", "put all cards revealed this way into
    /// your hand", "put them on the bottom of your library in any order":
    /// moves the resolution's card or revealed cards still in the library
    /// they were revealed from (CR 400.7: one that has moved is a new
    /// object). A card put into a hand keeps what players knew of it.
    /// With `random`, cards put on the bottom of a library go in a
    /// random order ("and the rest on the bottom of your library in a
    /// random order").
    PutCard {
        what: CardSet,
        to: CardDest,
        #[serde(default)]
        random: bool,
    },
    /// Adapt N (CR 701.46): if the source has no +1/+1 counters, it gets
    /// N.
    Adapt(u32),
    /// Bolster N (CR 701.39a): the controller chooses a creature they
    /// control with the least toughness among creatures they control and
    /// puts N +1/+1 counters on it.
    Bolster(Num),
    /// "Double target creature's power [and toughness] until end of turn"
    /// (CR 701.10a-c): it gets +X/+0 (+X/+Y), X its power (Y its
    /// toughness) as this resolves; a negative value gets more negative.
    DoublePt {
        to: Recipient,
        toughness: bool,
    },
    /// "Double the number of +1/+1 counters on ..." (`kind`), "... of each
    /// kind of counter on ..." (`None`) (CR 701.10e): each recipient gets
    /// as many of those counters as it already has.
    DoubleCounters {
        kind: Option<CounterKind>,
        to: Recipient,
    },
    /// "Double your life total" (CR 701.10d): each recipient player gains
    /// (or, below zero, loses) life equal to their life total.
    DoubleLife {
        who: Recipient,
    },
    /// Firebending's effect (CR 702.189a): "add N {R}. Until end of
    /// combat, you don't lose this mana as steps and phases end."
    Firebend(Num),
    /// "[add mana]. Until end of turn, you don't lose this mana as steps
    /// and phases end." (CR 106.4, 500.4): the mana these effects add to
    /// the controller's pool stays as steps end, until the cleanup step
    /// ends.
    KeepMana(Vec<Effect>),
    /// "[add mana]. Spend this mana only to cast creature spells." (CR
    /// 106.6): the mana these effects add goes into the controller's pool
    /// as restricted mana (`GameState::restricted_pool`), each unit
    /// spendable only as the restriction allows.
    SpendOnly {
        effects: Vec<Effect>,
        restriction: Box<ManaRestriction>,
    },
    /// "[add mana]. When you spend this mana to cast a Dragon creature
    /// spell, you gain 2 life.": the mana these effects add goes into the
    /// pool (`GameState::restricted_pool`, spendable on any spell or
    /// ability) carrying the rider; spent to cast a spell matching
    /// `spells`, it triggers `ability` (a `TriggerSpec::Reflexive`
    /// triggered ability; "it" is that spell).
    SpendRider {
        effects: Vec<Effect>,
        spells: Box<ObjFilter>,
        ability: AbilityId,
    },
    /// "unless you waterbend {2}" (CR 701.67a): pay this generic mana,
    /// each {1} by mana or by tapping an untapped artifact or creature the
    /// controller controls; inside "you may" or "unless", like `Pay`.
    Waterbend(ManaCost),
    /// Earthbend N (CR 701.66a): the target land the controller controls
    /// becomes a 0/0 creature with haste in addition to its other types
    /// (indefinitely), gets N +1/+1 counters, and a delayed trigger
    /// (`Builtin::EarthbendReturn`) returns it to the battlefield tapped
    /// under the controller's control when it dies or is put into exile.
    Earthbend {
        count: Num,
        to: Recipient,
    },
    /// "Return all creature cards from your graveyard to the battlefield":
    /// every card in the controller's graveyard matching `filter` enters
    /// under its owner's control.
    GraveyardToBattlefield {
        filter: ObjFilter,
        /// "... to the battlefield tapped" (Splendid Reclamation).
        #[serde(default)]
        tapped: bool,
    },
    /// Cloak (CR 701.58a): the top card of `of`'s library is put onto the
    /// battlefield face down under the controller's control as a 2/2
    /// creature with ward {2}; it may be turned face up for its mana cost
    /// if it's a creature card (CR 701.58b).
    CloakTop {
        of: Recipient,
    },
    /// Paradigm's delayed trigger (CR 702.192a): a copy of the paradigm
    /// card (`GameState::paradigms`, by the trigger's source) is made in
    /// exile, and its controller may cast it without paying its mana cost;
    /// not cast, it ceases to exist.
    CastParadigmCopy,
    /// Airbend (CR 701.65a): exile the permanent or spell; a card exiled
    /// this way may be cast by its owner for {2} rather than its mana cost
    /// for as long as it remains exiled.
    Airbend {
        what: Recipient,
    },
    /// "Create an X/X green Elemental creature token, where X is ...":
    /// fixes X as the effect resolves, before the token exists (CR
    /// 608.2h); the next `CreateToken`'s tokens of its own definition get
    /// base power and toughness X/X (CR 111.3: the values the creating
    /// effect defines; a layer-7b effect from their creation on).
    TokenPtX(Num),
    /// Exchange control of two permanents (CR 701.12a-b): `a` and `b`, or
    /// the two objects `a` names when `b` is `None` ("two target
    /// creatures"). Nothing happens unless both are still on the
    /// battlefield under different controllers; then each controller
    /// gains control of the other's permanent indefinitely.
    ExchangeControl {
        a: Recipient,
        b: Option<Recipient>,
    },
    /// "Target opponent gains control of target permanent you control."
    /// (CR 613.1b): the first player `to` names gains control of each
    /// object `of` names, indefinitely. Sets the "if they do" flag when
    /// any of them changed controller.
    GiveControl {
        of: Recipient,
        to: Recipient,
    },
    /// "Exchange life totals with target opponent" (CR 701.12c): the
    /// controller and the first player `with` names each gain or lose the
    /// life needed to reach the other's former total. Sets the "if you do"
    /// flag when the controller lost life ("If you lost life this way"),
    /// which the resolution's life-lost log then holds.
    ExchangeLife {
        with: Recipient,
    },
    /// Recruit (CR 701.70a): "Draw a card, then discard a card. If you
    /// discarded a nonland card this way, create a 1/1 white Human Soldier
    /// creature token." (`token`).
    Recruit {
        token: CardRef,
    },
    /// Populate (CR 701.36a): the controller chooses a creature token they
    /// control and creates a token that's a copy of it; with none, nothing
    /// happens (CR 701.36b).
    Populate,
    /// "End the turn." (CR 724.1): triggered abilities waiting to be put
    /// on the stack cease to exist, every object on the stack (this spell
    /// included) is exiled, and the game skips to the cleanup step.
    EndTurn,
    /// Incubate N (CR 701.53a): create `token` (an Incubator, a
    /// double-faced token, CR 111.10i) entering with N +1/+1 counters.
    Incubate {
        count: Num,
        token: CardRef,
    },
    /// Fabricate N (CR 702.123a): the controller may put N +1/+1 counters
    /// on the source; if they don't (or can't: it has left the
    /// battlefield), they create N `token`s (1/1 Servos).
    Fabricate {
        count: u32,
        token: CardRef,
    },
    /// The source becomes renowned (CR 702.112a, b), if it is still on the
    /// battlefield.
    BecomeRenowned,
    /// "Target Mount you control becomes saddled until end of turn" (CR
    /// 702.171b).
    BecomeSaddledTarget {
        what: Recipient,
    },
    /// The recipient permanent becomes prepared (CR 722.3a): only one with
    /// a prepare spell, and only if it isn't already.
    BecomePrepared {
        what: Recipient,
    },
    /// Monstrosity N (CR 701.37a): if the source isn't monstrous, N +1/+1
    /// counters on it and it becomes monstrous.
    Monstrosity(Num),
    /// Suspect (CR 701.60a): the recipient permanents become suspected
    /// (`on`), or are no longer suspected (CR 701.60d).
    Suspect {
        what: Recipient,
        on: bool,
    },
    /// "Each opponent blights 1", "target opponent blights 2" (CR
    /// 701.68a): each player of the recipient puts N -1/-1 counters on a
    /// creature they control, chosen as it resolves.
    BlightBy {
        who: Recipient,
        n: u32,
    },
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
    /// Soulbond's pairing (CR 702.95a): the source and an unpaired
    /// creature its controller controls, chosen (or, with `triggering`,
    /// the creature that entered), become paired if both are unpaired
    /// creatures that player controls (702.95c); a "may".
    Soulbond {
        triggering: bool,
    },
    /// Cumulative upkeep (CR 702.24a): an age counter on the source, then
    /// sacrifice it unless its controller pays the cost once per age
    /// counter.
    CumulativeUpkeep(ManaCost),
    /// Manifest (CR 701.34): the top card of the controller's library
    /// onto the battlefield face down as a 2/2 creature.
    Manifest,
    /// Cipher's trigger (CR 702.99a): a copy of the encoded card (the
    /// source, in exile) may be cast without paying its mana cost.
    CastCipherCopy,
    /// "Copy it. You may cast the copy [without paying its mana cost]."
    /// (CR 707.12): for each recipient card (one not on the battlefield or
    /// the stack), a copy is made in exile, and the controller may cast it
    /// as the effect finishes, for its mana cost or free; not cast, it
    /// ceases to exist. "If you do" sees whether one will be cast.
    CastCopy {
        what: Recipient,
        free: bool,
    },
    /// Blight N (CR 701.68a): the controller puts N -1/-1 counters on a
    /// creature they control; without one, nothing happens and it isn't
    /// done (701.68b).
    Blight(u32),
    /// Collect evidence N (CR 701.59a): the controller exiles cards with
    /// total mana value N or greater from their graveyard (the engine picks
    /// them); with too little there, nothing is exiled and it isn't done
    /// (701.59b).
    CollectEvidence(u32),
    /// "Forage" (CR 701.61a): exile three cards from your graveyard or
    /// sacrifice a Food, as the controller chooses; "If you do" sees
    /// whether they could.
    Forage,
    /// Craft's effect (CR 702.167a): "Return this card transformed under
    /// its owner's control": the source's card, exiled by the cost, onto
    /// the battlefield with its back face up (CR 712.14).
    ReturnTransformed,
    /// Manifest dread (CR 701.62a): look at the top two cards of the
    /// controller's library, manifest one, and put the rest into their
    /// graveyard.
    ManifestDread,
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
    /// "Target creature can't be the target of spells or abilities your
    /// opponents control this turn." (Vines of Vastwood): until end of
    /// turn, judged against this effect's controller (CR 611.2).
    CantBeTargeted {
        what: Recipient,
        ban: TargetBan,
    },
    /// "Target opponent chooses a creature they control. Destroy that
    /// creature.": each recipient player chooses a permanent they control
    /// matching the filter (not targeted, CR 608.2d); the chosen one is
    /// the created object for the rest of the resolution
    /// (`Recipient::CreatedObject`).
    PlayerChooses {
        who: Recipient,
        filter: ObjFilter,
    },
    /// "Choose two target creatures controlled by the same player. That
    /// player sacrifices one of them of their choice." (Incriminate,
    /// Cannibalize, Trial of Agony): targets `first` and `first + 1`; the
    /// controller of the first (`by_controller`) or the ability's
    /// controller picks one of them (CR 608.2d; only a still-legal one if
    /// the other isn't, CR 608.2b), which becomes target `first` for the
    /// rest of the resolution and the other target `first + 1` ("the
    /// other"); `act` is done to the picked one.
    PickTarget {
        first: u8,
        by_controller: bool,
        act: PickAct,
        wording: PickWording,
    },
    /// "Target instant or sorcery card in your graveyard gains flashback
    /// until end of turn. The flashback cost is equal to its mana cost."
    /// (CR 702.34a): its owner may cast it from the graveyard for its mana
    /// cost this turn, and it is exiled as it leaves the stack. The grant
    /// stays with the card only while it stays in the graveyard (CR
    /// 400.7).
    GrantFlashback {
        to: Recipient,
        /// "... gains harmonize until end of turn. Its harmonize cost is
        /// equal to its mana cost." (CR 702.180a) instead.
        harmonize: bool,
    },
    /// "gains all creature types until end of turn" (`all`), "loses all
    /// creature types until end of turn" (layer 4).
    CreatureTypes {
        to: Recipient,
        all: bool,
    },
    /// "It doesn't untap during its controller's next untap step".
    /// "Target player skips their next draw step", "Target opponent skips
    /// their next combat phase" (CR 500.11), `this_turn`: "... skips their
    /// next combat phase this turn" (only a combat phase of this turn).
    SkipNext {
        who: Recipient,
        what: SkipWhat,
    },
    SkipNextUntap {
        to: Recipient,
    },
    /// "Take an extra turn after this one", "Target player takes two extra
    /// turns after this one" (CR 500.7): each recipient player takes `n`
    /// extra turns directly after this one, the most recently created
    /// first.
    ExtraTurns {
        who: Recipient,
        n: u8,
    },
    /// "After this phase, there is an additional combat phase", "After
    /// this main phase, there is an additional combat phase followed by an
    /// additional main phase", "there are two additional combat phases"
    /// (CR 500.8): `combats` combat phases, then a main phase if `main`,
    /// added directly after the current phase, before phases added
    /// earlier.
    AdditionalPhases {
        combats: u8,
        main: bool,
    },
    /// "That creature can't be regenerated this turn" (CR 701.15): no
    /// regeneration shield replaces its destruction this turn.
    CantBeRegenerated {
        to: Recipient,
    },
    /// "The damage can't be prevented." (Pinpoint Avalanche, CR 615.12):
    /// no damage this resolution deals is prevented. Read before the
    /// spell's effects happen, wherever it is written among them.
    DamageUnpreventable,
    /// "Excess damage is dealt to that creature's controller instead."
    /// (Pigment Storm, CR 120.4a): read before the spell's effects
    /// happen; its damage to a creature beyond lethal goes to that
    /// creature's controller.
    ExcessToController,
    /// "This creature assigns no combat damage this turn" (CR 510.1a):
    /// it assigns none in this turn's combat damage steps.
    AssignsNoCombatDamage {
        to: Recipient,
    },
    /// "Target creature blocks this creature this turn if able" (CR
    /// 509.1c): a blocking requirement on `blocker` for one attacker.
    BlocksIfAble {
        blocker: Recipient,
        attacker: Recipient,
    },
    /// "Target creature can't block this creature this turn" (CR 509.1b):
    /// a blocking restriction on `blocker` for one attacker.
    CantBlockThis {
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
    /// "If excess damage was dealt this way, [these]" (CR 120.4a): the
    /// resolution has dealt damage beyond lethal so far.
    IfExcessDamage(Vec<Effect>),
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
    /// Mobilize's tokens (CR 702.181a): the tokens the resolution just
    /// created are attacking ("create two tapped and attacking ...
    /// tokens", CR 508.4: never declared as attackers), what the source
    /// is attacking, or else an opponent.
    CreatedAttack,
    /// "... onto the battlefield tapped and attacking" (CR 508.4): the
    /// permanent the resolution just put onto the battlefield (from a hand
    /// or a graveyard) is attacking what the source is attacking, or else
    /// an opponent.
    EnteredAttack,
    /// Saddle's effect (CR 702.171a): the source becomes saddled until end
    /// of turn.
    BecomeSaddled,
}

impl Effect {
    /// Whether applying this effect may ask its controller (or another
    /// player) a question in the middle of resolution.
    /// Whether this effect adds mana (CR 106.4), in either branch of an
    /// "instead" ("Add {C}. If you control an Urza's Mine and an Urza's
    /// Power-Plant, add {C}{C} instead.").
    pub fn adds_mana(&self) -> bool {
        match self {
            Effect::AddMana(_)
            | Effect::AddManaOneOf(_)
            | Effect::AddManaTimes { .. }
            | Effect::AddManaTwoColors(_)
            | Effect::AddManaCombination { .. }
            | Effect::AddManaLandCould { .. }
            | Effect::AddManaCommander { .. } => true,
            Effect::Instead { base, alt, .. } => base.iter().chain(alt).any(Effect::adds_mana),
            Effect::SpendOnly { effects, .. } | Effect::SpendRider { effects, .. } => {
                effects.iter().any(Effect::adds_mana)
            }
            _ => false,
        }
    }

    /// Whether this effect acts on or through `Recipient::Granter`.
    pub fn names_granter(&self) -> bool {
        let g = Recipient::Granter;
        match self {
            Effect::DamageFrom { source, to, .. } => *source == g || *to == g,
            Effect::DealDamage { to, .. } | Effect::Destroy { to } => *to == g,
            Effect::ReturnToHand { what } | Effect::Sacrifice { what } => *what == g,
            _ => false,
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per effect that asks
    pub const fn asks(&self) -> bool {
        matches!(
            self,
            Effect::YouMay(_)
                | Effect::PlayerMay { .. }
                | Effect::AnyPlayerMay { .. }
                | Effect::ForEachCouldnt(_)
                | Effect::EachPutsFromGraveyard { .. }
                | Effect::IfKicked(_)
                | Effect::IfYouDo(_)
                | Effect::Otherwise(_)
                | Effect::IfCondition(..)
                | Effect::IfTarget { .. }
                | Effect::IfCard { .. }
                // "Choose one of them."
                | Effect::ExilePlayable { choose: true, .. }
                // A card put onto the battlefield is not known until it
                // resolves: it can meet its namesake or a replacement.
                | Effect::PutCard {
                    to: CardDest::Battlefield { .. },
                    ..
                }
                | Effect::Instead { .. }
                | Effect::UnlessPays { .. }
                | Effect::UnlessPaysCost { .. }
                | Effect::UnlessSacrifices { .. }
                | Effect::UnlessYou { .. }
                | Effect::Scry { .. }
                | Effect::Surveil { .. }
                | Effect::RemoveCounterChosen { .. }
                | Effect::CopySpell
                | Effect::CopySpells { .. }
                | Effect::ChangeTarget { .. }
                | Effect::Discard { .. }
                | Effect::HandToLibrary { .. }
                | Effect::ShuffleIntoLibrary { .. }
                | Effect::CounterUnlessPays { .. }
                | Effect::Search { .. }
                | Effect::AddMana(_)
                | Effect::AddManaOneOf(_)
                | Effect::AddManaTimes { .. }
                | Effect::AddManaTwoColors(_)
                | Effect::AddManaCombination { .. }
                | Effect::AddManaLandCould { .. }
                | Effect::KeepMana(_)
                | Effect::SpendOnly { .. }
                | Effect::SpendRider { .. }
                | Effect::SacrificeChosen { .. }
                | Effect::ReturnChosen { .. }
                | Effect::ExileChosen { .. }
                | Effect::PlayerChooses { .. }
                | Effect::PickTarget { .. }
                // "Move any number of +1/+1 counters": how many is asked as
                // it resolves (Bioshift; it panicked outside a "you may").
                | Effect::MoveCounters {
                    count: Some(MOVE_ANY_NUMBER),
                    ..
                }
                | Effect::ExileFromHand { .. }
                | Effect::ExileFromGraveyardOrHand { .. }
                | Effect::DrawDiscardDrawn { .. }
                | Effect::DiscardRandom { .. }
                | Effect::DiscardChosen { .. }
                | Effect::Explore { .. }
                | Effect::Endure { .. }
                | Effect::PutFromHand { .. }
                | Effect::Dig { .. }
                | Effect::DigSplit { .. }
                | Effect::ManifestDread
                | Effect::Champion(_)
                | Effect::Blight(_)
                | Effect::BlightBy { .. }
                | Effect::Connive { .. }
                | Effect::CastExiled(_)
                | Effect::CastCipherCopy
                | Effect::CastCopy { .. }
                | Effect::SuspendTick
                | Effect::Cascade
                | Effect::Ripple
                | Effect::PutInLibrary {
                    place: LibraryPlace::TopOrBottom | LibraryPlace::FromTopOrBottom(_),
                    ..
                }
                | Effect::Discover(_)
                | Effect::Exploit
                | Effect::CumulativeUpkeep(_)
                | Effect::Soulbond { .. }
                | Effect::TapOrUntap { .. }
                | Effect::OfChoice { .. }
                | Effect::Choose(_)
                | Effect::IfThisWay { .. }
                | Effect::IfExcessDamage(_)
                | Effect::ChooseAct { .. }
                | Effect::Forage
                | Effect::OneOf(_)
                | Effect::DiscardUnless { .. }
                // A source of the controller's choice (CR 609.7a).
                | Effect::CreateReplacement(ReplacementAbility {
                    replaces: Replaces::Damage {
                        from: Some(Affects::Chosen(_)),
                        ..
                    },
                    ..
                })
                // "Choose a creature or planeswalker you control."
                | Effect::CreateReplacement(ReplacementAbility {
                    replacement: Replacement::Redirect {
                        to: RedirectTo::ChosenCreatureOrPlaneswalker,
                        ..
                    },
                    ..
                })
                | Effect::Vote { .. }
                | Effect::VoteReturn { .. }
                // Each clashing player puts their card on the top or
                // bottom (CR 701.23a).
                | Effect::Clash { .. }
                // A Ring-bearer, a dungeon, or a room to choose.
                | Effect::RingTempts
                | Effect::Venture { .. }
                | Effect::PunishEach { .. }
                | Effect::DrawCastOne { .. }
                | Effect::CastFree { .. }
                | Effect::RevealPutCreature { .. }
                // Which creature (tied for least toughness, or token) and
                // whether to take the counters.
                | Effect::Bolster(_)
                | Effect::Populate
                | Effect::Fabricate { .. }
                // The card to discard (CR 701.70a).
                | Effect::Recruit { .. }
                // What enters is not known until it resolves.
                | Effect::GraveyardToBattlefield { .. }
                | Effect::CloakTop { .. }
                | Effect::CastParadigmCopy
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
    /// Tapped to pay a cost ("the tapped creature's power", station's,
    /// CR 702.184a): recorded where it stays, on the battlefield.
    Tapped,
    /// Put into a library from another zone ("shuffle the cards from your
    /// hand into your library, then draw that many cards").
    Shuffled,
    /// Revealed from the top of a library without moving (parley, "For
    /// each nonland card revealed this way"): the resolution's revealed
    /// cards, not a zone change, so only a count reads it.
    Revealed,
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

/// What `Effect::SkipNext` skips (CR 500.11).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum SkipWhat {
    /// "their next draw step".
    DrawStep,
    /// "their next combat phase".
    CombatPhase,
    /// "their next combat phase this turn".
    CombatPhaseThisTurn,
    /// "Creatures don't untap during target player's next untap step."
    /// (Blinding Beam): no creature untaps in that player's next untap
    /// step, whoever controls it.
    CreatureUntap,
}

/// `Effect::MoveCounters::count` for "move any number of +1/+1 counters":
/// the controller chooses how many as it resolves.
pub const MOVE_ANY_NUMBER: u32 = u32::MAX;

/// What `Effect::ChooseAct` does to each object its controller chooses.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ChooseAct {
    /// "tap another untapped Citizen you control" (permanents you control).
    Tap,
    /// "sacrifice a creature" (permanents you control).
    Sacrifice,
    /// "untap up to two lands" (tapped permanents; anyone's unless the
    /// filter names a controller).
    Untap,
    /// "return up to two lands you control to their owner's hand".
    Bounce,
    /// "discard a creature card" (cards in your hand).
    Discard,
    /// "exile two creature cards from your graveyard".
    ExileFromGraveyard,
    /// "exile the top creature card of your graveyard" (CR 404.2: the
    /// most recent matching cards; nothing is chosen).
    ExileTopOfGraveyard,
    /// "exile this spell and four cards named Dragon's Approach from your
    /// graveyard": the cards from your graveyard, then the resolving spell
    /// itself, only if all of the cards can be.
    ExileWithThisSpell,
    /// "exile a land you control" (permanents you control). On a card
    /// whose leaves-the-battlefield ability returns "the exiled card", what
    /// it exiles is linked to it (CR 607.2a; Wormfang Newt).
    Exile,
    /// "return a creature card from your graveyard to your hand".
    ReturnFromGraveyard,
    /// "return a Zombie card at random from your graveyard to your hand":
    /// the cards are picked at random, not by the controller.
    ReturnRandomFromGraveyard,
    /// "put a permanent card from among the milled cards into your hand":
    /// cards this resolution milled, still in the graveyard.
    MilledToHand,
    /// "put a creature card from among them onto the battlefield": cards
    /// this resolution milled, still in the graveyard, onto the
    /// battlefield under their owner's control.
    MilledToBattlefield,
    /// "return a land card from your graveyard to the battlefield
    /// [tapped]" (cards in your graveyard).
    GraveyardToBattlefield { tapped: bool },
    /// "you may attach an Equipment you control to it" (permanents you
    /// control): attached to the `n`th target (CR 701.3).
    AttachToTarget(u8),
    /// "put a card from your hand on the bottom of your library" (cards in
    /// your hand).
    HandToBottom,
    /// "put an instant, sorcery, or battle card from your graveyard on top
    /// of your library" (cards in your graveyard).
    GraveyardToTop,
    /// Enlist (CR 702.154a): tap a nonattacking creature you control that
    /// could pay a {T} cost (CR 302.6); the source gets +X/+0 until end of
    /// turn, X that creature's power.
    Enlist,
    /// "put a card an opponent owns from exile into that player's
    /// graveyard" (a processor, CR 400.3: to its owner's graveyard): cards
    /// in exile whose owner the filter's `who` names.
    ExileToGraveyard,
    /// "Reveal any number of green cards in your hand" (CR 701.20a): cards
    /// in your hand, revealed where they are; "cards revealed this way"
    /// (`ThisWay::Revealed`) counts them.
    RevealFromHand,
    /// "transform an Incubator token you control", "transform any number
    /// of Human Werewolves you control" (CR 701.27; permanents you
    /// control).
    Transform,
    /// "Choose a creature you control. It gains indestructible until end
    /// of turn." (CR 608.2d: not targeted; any controller's unless the
    /// filter names one): nothing is done to it; the chosen permanent is
    /// "it" for the rest of the resolution (`Recipient::CreatedObject`).
    Choose,
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
    /// One of this many named options ("choose Khans or Dragons", CR
    /// 614.12): `Chosen::Option` with its index.
    Named(u8),
    /// "choose odd or even" (Zero is even.): `Chosen::Parity`, for
    /// "with mana value of the chosen quality".
    Parity,
    /// "choose a card name", "choose a nonland card name" (CR 201.3):
    /// `Chosen::Card`. The names offered are those of the cards in the
    /// game (any zone, either player's), since naming another card has no
    /// effect on play.
    CardName { nonland: bool },
}

/// What a chosen-name ban forbids (`PlayerStaticKind::ChosenName`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum NameBan {
    /// "Spells with the chosen name can't be cast."
    Cast,
    /// "Activated abilities of sources with the chosen name can't be
    /// activated unless they're mana abilities." (CR 605.1a).
    ActivateNonMana,
    /// "Activated abilities of sources with the chosen name can't be
    /// activated."
    Activate,
}

/// What a player-level static ability does (`AbilityDef::PlayerStatic`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum PlayerStaticKind {
    /// "You may play an additional land on each of your turns." (CR
    /// 305.2): `n` more land plays on the controller's turns; with
    /// `each_player`, on each player's own turns ("Each player may play an
    /// additional land on each of their turns.").
    ExtraLands { n: u32, each_player: bool },
    /// "You have hexproof." (CR 702.11c): the controller can't be the
    /// target of spells or abilities their opponents control.
    Hexproof,
    /// "You have shroud." (CR 702.18c): the controller can't be the target
    /// of spells or abilities.
    Shroud,
    /// A ban on the name the source chose as it entered (`Chosen::Card`,
    /// CR 201.3), for every player: Meddling Mage, Pithing Needle.
    ChosenName(NameBan),
    /// "You may play lands from your graveyard." (CR 305.1, 601.3): the
    /// controller may play land cards from their graveyard, using their
    /// land plays.
    LandsFromGraveyard,
    /// "Your opponents can't cast spells during your turn." (CR 101.2).
    OpponentsCantCastYourTurn,
    /// "You can't lose the game and your opponents can't win the game."
    /// (CR 104.3a-b, 104.2b; Platinum Angel).
    CantLose,
    /// A rule for the whole game while the source is on the battlefield:
    /// "Players can't gain life.", "Damage can't be prevented." (CR
    /// 614.17; `GameState::rule_in_force`).
    Rule(RuleKind),
    /// "Activated abilities of artifacts you control cost {1} less to
    /// activate." (CR 602.2b, 601.2f): the generic mana of the activated
    /// abilities (of that `keyword`, if any: "Power-up abilities of other
    /// creatures you control ...") of permanents matching the pooled
    /// filter `sources` (judged for this source) is reduced by `by`; with
    /// `min_one`, "This effect can't reduce the mana in that cost to less
    /// than one mana."
    ActivationCost {
        sources: u16,
        by: u8,
        min_one: bool,
        keyword: Option<ActivatedKeyword>,
    },
    /// "You may pay {0} rather than pay the equip cost of the first equip
    /// ability you activate each turn" (CR 118.9; `your_turns`: "during
    /// each of your turns"): the engine takes it for the first equip
    /// ability its controller activates in such a turn.
    FirstEquipFree { your_turns: bool },
    /// "Enchanted creature can't be the target of spells or abilities your
    /// opponents control.", "Enchanted creature can't be the target of
    /// spells." (CR 115.4): the permanent this is attached to, judged
    /// against this source's controller ("your"), not the enchanted
    /// permanent's (unlike hexproof, CR 702.11b).
    EnchantedUntargetable(TargetBan),
    /// The Flagbearer rule (Coalition Honor Guard): "While an opponent is
    /// choosing targets as part of casting a spell they control or
    /// activating an ability they control, that player must choose at
    /// least one Flagbearer on the battlefield if able." Triggered
    /// abilities and changed or copied targets are free (rulings).
    Flagbearer,
}

/// Which spells and abilities can't target a permanent
/// (`PlayerStaticKind::EnchantedUntargetable`, `Effect::CantBeTargeted`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum TargetBan {
    /// "spells or abilities your opponents control": those an opponent of
    /// the effect's controller controls.
    Opponents,
    /// "spells": any player's spells, no abilities.
    Spells,
}

/// What `Effect::PickTarget` does to the target picked.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum PickAct {
    /// Nothing: "That player chooses one of those creatures."
    Choose,
    /// Its controller sacrifices it: "That player sacrifices one of them
    /// of their choice."
    Sacrifice,
    /// "Exile one of those creatures".
    Exile,
}

/// How `Effect::PickTarget` is worded (wording only).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum PickWording {
    /// "That player chooses one of those creatures.", "Exile one of those
    /// creatures".
    #[default]
    Those,
    /// "That player sacrifices one of them of their choice."
    OfTheirChoice,
    /// "That player chooses and sacrifices one of those creatures."
    ChoosesAnd,
    /// "Their controller chooses and sacrifices one of them."
    TheirController,
}

/// How often a static free-cast permission may be used
/// (`AbilityDef::CastFree`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum FreeCastLimit {
    /// Any number of spells.
    #[default]
    Always,
    /// "Once during each of your turns, you may cast ...": one spell in
    /// each of the controller's turns.
    OnceYourTurns,
    /// "The first spell each player casts during each of their turns may
    /// be cast without paying its mana cost.": every player, the first
    /// spell in each of their own turns.
    FirstEachPlayer,
}

/// Effects that modify the rules rather than objects (CR 613.11, 614.17).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum RuleKind {
    /// "Players can't gain life."
    NoLifeGain,
    /// "Damage can't be prevented." Prevention effects still apply but
    /// prevent nothing (CR 615.12).
    DamageCantBePrevented,
    /// "Combat damage can't be prevented." (Frenzied Baloth): as
    /// `DamageCantBePrevented`, for combat damage only.
    CombatDamageCantBePrevented,
    /// "Cards in graveyards can't be the targets of spells or abilities."
    /// (CR 115.4): no graveyard card is a legal target.
    GraveyardUntargetable,
}

/// A "can't" effect on certain players until end of turn (CR 614.17):
/// "Target player can't play lands this turn" (CR 305.1), "... can't cast
/// creature spells this turn" (CR 601.2).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum PlayerRuleKind {
    CantPlayLands,
    CantCastCreatureSpells,
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
    /// An opponent of the controller or an object matching a filter ("an
    /// opponent or a permanent an opponent controls").
    OpponentsAnd(ObjFilter),
    /// The spell's `n`th chosen target, locked in when the effect is
    /// created (stored effects only).
    Target(usize),
    /// The spell's chosen targets `first..first + count` ("up to two target
    /// creatures"; a variable group, `count` `u8::MAX`, runs to the end of
    /// the targets): one effect is stored per target, locked to it. A
    /// shield over a variable group is divided as the spell is cast
    /// (CR 601.2d, Remedy).
    Targets(u8, u8),
    /// Objects matching a filter other than the spell's `n`th chosen
    /// target, locked in likewise ("creatures other than target creature",
    /// Terrifying Presence).
    OtherThanTarget(ObjFilter, usize),
    /// A damage source matching a filter that the controller chooses as
    /// the effect is created ("a red source of your choice", CR 609.7a;
    /// stored effects only).
    Chosen(ObjFilter),
    /// "a creature dealt damage this way" (CR 608.2c): each permanent the
    /// resolution has dealt damage to so far, one effect locked to each
    /// as it is created (stored effects only).
    DamagedThisWay,
    /// Anything but objects matching the filter: as a damage source,
    /// "Prevent all combat damage that would be dealt this turn except
    /// combat damage that would be dealt by enchanted creatures and
    /// enchantment creatures." (Inspire Awe).
    Except(ObjFilter),
}

/// Where a redirection effect sends the damage (CR 614.9).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum RedirectTo {
    /// The ability's source ("is dealt to this creature instead").
    This,
    /// The spell's `n`th chosen target, locked in when the effect is
    /// created.
    Target(usize),
    /// The effect's controller ("is dealt to you instead", Vassal's
    /// Duty).
    You,
    /// "Choose a creature or planeswalker you control. All damage ... is
    /// dealt to the chosen permanent instead (if it's still on the
    /// battlefield)." (Gideon's Sacrifice): chosen by the controller as
    /// the effect is created (stored effects only).
    ChosenCreatureOrPlaneswalker,
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
    /// A player would lose life (CR 119.3), damage included, but not life
    /// paid as a cost: "If an opponent would lose life during your turn,
    /// they lose twice that much life instead." (Bloodletter of
    /// Aclazotz).
    LoseLife { whose: Who },
    /// A player would draw a card; `library_empty` requires an empty
    /// library (Laboratory Maniac).
    Draw { whose: Who, library_empty: bool },
    /// A card would be put into a graveyard owned by `whose`, from anywhere
    /// or only from the battlefield.
    ToGraveyard { whose: Who, from_battlefield: bool },
    /// A card matching the filter (judged as it is, in whatever zone)
    /// would be put into a graveyard from anywhere: "If an instant or
    /// sorcery card would be put into a graveyard from anywhere, exile it
    /// instead." (Dryad Militant).
    FilteredToGraveyard { filter: ObjFilter },
    /// The ability's own card would be put into a graveyard from anywhere
    /// (Progenitus): the ability functions in every zone (CR 113.6b).
    ThisToGraveyard,
    /// +1/+1 counters would be put on a permanent matching `on`, including
    /// as it enters (CR 122.6).
    PlusCounters { on: ObjFilter },
    /// Counters of any kind would be put on a permanent matching `on`,
    /// including as it enters (CR 122.6): "If one or more counters would
    /// be put on ..." (`by` says whose putting counts).
    AnyCounters { on: ObjFilter, by: CountersBy },
    /// A permanent would be destroyed (CR 701.7); regeneration shields
    /// (CR 701.15) replace this.
    Destroy { to: Affects },
    /// A creature would die ("If that creature would die this turn, exile
    /// it instead").
    Dies { to: Affects },
    /// How this permanent itself enters the battlefield (CR 614.1c).
    Enters,
    /// How another permanent matching the filter enters ("Each Dragon you
    /// control enters with an additional +1/+1 counter on it", CR
    /// 614.1c), judged by its card as it enters, under the controller it
    /// will have.
    OthersEnter { what: ObjFilter },
    /// One or more tokens (`creature`: creature tokens; `artifact`:
    /// artifact tokens) would be created under the controller's control
    /// (CR 111.1, 614.1a). `effect` prints the older "If an effect would
    /// create ...", `you` "If you would create ..." (rendering only).
    CreateTokens {
        creature: bool,
        effect: bool,
        artifact: bool,
        you: bool,
    },
}

/// Whose putting counters a counter replacement applies to
/// (`Replaces::AnyCounters`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum CountersBy {
    /// "If one or more counters would be put on ...": any.
    Any,
    /// "If an effect would put one or more counters on ...": an effect's
    /// (not a cost's, CR 122.6).
    Effect,
    /// "If you would put one or more counters on ...": the replacement's
    /// controller's spells, abilities, and costs.
    You,
}

/// `Effect::PayEnergy`'s "any amount of {E}".
pub const PAY_ANY_ENERGY: u32 = u32::MAX;
impl Affects {
    fn fill_chosen(&mut self, chosen: crate::state::Chosen) {
        match self {
            Affects::Objects(f)
            | Affects::YouAnd(f)
            | Affects::OpponentsAnd(f)
            | Affects::OtherThanTarget(f, _)
            | Affects::Chosen(f)
            | Affects::Except(f)
                if f.chosen =>
            {
                *f = f.with_chosen(chosen);
            }
            _ => {}
        }
    }
}

impl Replaces {
    /// The damage event's filters with "of the color of your choice" filled
    /// in from the choice made as the effect was created (CR 608.2d).
    pub fn fill_chosen(&mut self, chosen: crate::state::Chosen) {
        match self {
            Replaces::Damage { from, to, .. } => {
                if let Some(f) = from {
                    f.fill_chosen(chosen);
                }
                to.fill_chosen(chosen);
            }
            Replaces::DamageToAndBy { what, .. } => what.fill_chosen(chosen),
            _ => {}
        }
    }
}

/// What happens instead.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Replacement {
    /// Prevent all of the damage, or the player gains no life.
    Prevent,
    /// A shield: prevent the next N damage (CR 615.7); N is evaluated as
    /// the effect is created ("where X is the number of Clerics on the
    /// battlefield").
    PreventNext(Num),
    /// Double the damage or life.
    Double,
    /// That many plus one counters; that much life plus 1.
    PlusOne,
    /// "those tokens plus a 1/1 green Frog creature token are created
    /// instead" (Quina); `that_many`: "plus that many ... tokens"
    /// (Chatterfang).
    PlusToken { token: CardRef, that_many: bool },
    /// "that many 5/5 red Dragon creature tokens with flying are created
    /// instead" (Draconic Visitor): the tokens the event covers become as
    /// many of this token.
    TokensInstead { token: CardRef },
    /// "it deals that much damage plus 2 instead" (Torbran); `to_that`
    /// prints "to that permanent or player" after it (rendering only).
    PlusN { n: u32, to_that: bool },
    /// "instead it deals that much damage plus X, where X is Hawkeye's
    /// power": `n` (the source card's count) evaluated as the damage would
    /// be dealt.
    PlusCount(Num),
    /// Put the card into this zone instead.
    Instead(crate::state::Zone),
    /// Reveal the card and shuffle it into its owner's library instead
    /// (Darksteel Colossus).
    ShuffleIntoLibrary,
    /// Put the card on top (`top`) or the bottom of its owner's library
    /// instead ("If this creature would die, put it on top of its owner's
    /// library instead.", Gravebane Zombie; Nissa's Chosen).
    IntoLibrary { top: bool },
    /// Draw two cards instead of one.
    DrawTwo,
    /// The player wins the game instead.
    WinGame,
    /// Enters tapped.
    EnterTapped,
    /// "This creature enters prepared." (CR 722.3a).
    EnterPrepared,
    /// "As this land enters, you may pay 3 life. If you don't, it enters
    /// tapped." (CR 614.12, 119.4): its controller is asked as it enters.
    EnterTappedUnlessPay(u32),
    /// "As this land enters, you may reveal a Mountain or Forest card
    /// from your hand. If you don't, this land enters tapped." (CR
    /// 614.12, 701.20a): its controller is asked as it enters if a card in
    /// their hand matches the pool's `filter`th filter (judged as printed).
    /// `or`: "This land enters tapped unless you revealed a Dragon card
    /// this way or you control a Dragon." (the pool's `or`th condition).
    EnterTappedUnlessReveal { filter: u16, or: Option<u16> },
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
    /// Its controller may have it enter with N more counters (unleash, CR
    /// 702.98a); with `haste`, if they don't, it has haste (riot, CR
    /// 702.136a).
    EnterWithCountersMay {
        kind: CounterKind,
        n: u32,
        haste: bool,
    },
    /// "This creature enters with your choice of a flying counter or a
    /// first strike counter on it" (CR 614.1c, 122.1b): its controller
    /// chooses one of `kinds` as it enters; with `cond` ("escapes with
    /// ..."), only while the pool's condition holds.
    EnterWithCounterChoice {
        kinds: [CounterKind; 2],
        cond: Option<u16>,
    },
    /// Devour N (CR 702.82a): "As this object enters, you may sacrifice
    /// any number of [kind]s. This permanent enters with N times that many
    /// +1/+1 counters on it." (CR 702.82b: "devour artifact", "devour
    /// land"). What it devoured is noted (`GameState::enter_notes`).
    EnterDevour { kind: CardType, n: u32 },
    /// Amplify N (CR 702.38a): "As this object enters, reveal any number
    /// of cards from your hand that share a creature type with it. This
    /// permanent enters with N +1/+1 counters on it for each card revealed
    /// this way."
    EnterAmplify(u32),
    /// Tribute N (CR 702.104a): "As this creature enters, choose an
    /// opponent. That player may put an additional N +1/+1 counters on it
    /// as it enters." Paid, it is noted (`EnterNote::TributePaid`).
    EnterTribute(u32),
    /// Read ahead (CR 702.155a): "As this Saga enters, choose a number
    /// from one to the number of chapter abilities it has. It enters with
    /// a number of lore counters equal to the chosen number." Chapters
    /// below it don't trigger as it enters (702.155c).
    EnterReadAhead,
    /// CR 701.15a: instead of being destroyed, the permanent has all damage
    /// removed, is tapped, and leaves combat. A stored shield; used once.
    Regenerate,
    /// Damage that would be dealt to the controller is dealt to the
    /// permanent this Aura is attached to instead (Pariah).
    RedirectToAttached,
    /// "The next time ... would deal damage ..., prevent that damage": a
    /// shield that prevents all of one damage event, then is used up
    /// (CR 615.7). Stored effects only.
    PreventNextTime,
    /// "The next time damage would be dealt to target creature this turn,
    /// destroy that creature instead." (Kill-Suit Cultist): a one-use
    /// replacement of one damage event to a permanent (CR 614.1a).
    DestroyNextTime,
    /// The damage is dealt to `to` instead (CR 614.9): all of it, or the
    /// next `next` damage ("The next 1 damage that would be dealt to this
    /// creature this turn is dealt to target creature you control
    /// instead"). Stored effects only.
    Redirect { to: RedirectTo, next: Option<u32> },
    /// Umbra armor (CR 702.89a): instead of the enchanted permanent being
    /// destroyed, all damage is removed from it and this Aura is
    /// destroyed.
    UmbraArmor,
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
    /// It applies only while the pool's condition holds for its
    /// controller ("Max speed — If a source you control would deal
    /// damage ...", CR 702.179f).
    #[serde(default)]
    pub condition: Option<u16>,
}

impl ReplacementAbility {
    pub fn is_prevention(&self) -> bool {
        matches!(
            self.replacement,
            Replacement::Prevent | Replacement::PreventNext(_) | Replacement::PreventNextTime
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
pub const WATCH_ZONE_CHANGE: u32 = 1;
pub const WATCH_STEP_BEGIN: u32 = 2;
pub const WATCH_ATTACK: u32 = 4;
pub const WATCH_BLOCK: u32 = 8;
pub const WATCH_DAMAGE: u32 = 16;
pub const WATCH_LIFE_CHANGE: u32 = 32;
pub const WATCH_CAST: u32 = 64;
pub const WATCH_TARGETED: u32 = 128;
pub const WATCH_TAP: u32 = 256;
pub const WATCH_FACE_UP: u32 = 512;
pub const WATCH_DRAW: u32 = 1024;
pub const WATCH_CYCLE: u32 = 2048;
pub const WATCH_DISCARD: u32 = 4096;
pub const WATCH_ATTACH: u32 = 8192;
pub const WATCH_ROLL: u32 = 16384;
/// Designation changes (day and night, CR 731).
pub const WATCH_DESIG: u32 = 32768;
/// Sacrifices (CR 701.21).
pub const WATCH_SACRIFICE: u32 = 65536;
/// Counters put on permanents (CR 122.1).
pub const WATCH_COUNTERS: u32 = 131_072;
/// Keyword actions and activations: scry and surveil, explore, transform,
/// activated abilities, tapping by an effect.
pub const WATCH_ACT: u32 = 262_144;

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
    /// "Whenever ~ attacks for the first time each turn": the attacking
    /// creature hadn't attacked before this turn (an additional combat
    /// phase, CR 500.8).
    AttacksFirstTime(TriggerObj),
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
    /// "Whenever a player attacks with three or more creatures" (CR
    /// 508.1m): once per declaration of attackers by a player matching
    /// `whose`, when at least `min` creatures attack; that player is the
    /// triggering player.
    PlayerAttacksWith { whose: Who, min: u8 },
    /// "Whenever this creature attacks and isn't blocked" (CR 509.1h): as
    /// blockers are declared, an attacking creature no creature blocks.
    AttacksUnblocked(TriggerObj),
    /// "Whenever this creature blocks or becomes blocked".
    BlocksOrBlocked(TriggerObj),
    /// "Whenever this creature attacks or blocks".
    AttacksOrBlocks(TriggerObj),
    /// "When this creature leaves the battlefield" (to any zone).
    Leaves(TriggerObj),
    /// "Whenever this creature or another creature you control leaves the
    /// battlefield without dying": to any zone but a graveyard (CR 700.4),
    /// judged as it last existed. `Batch` of it: "one or more ... leave
    /// the battlefield without dying".
    LeavesWithoutDying(TriggerObj),
    /// "Whenever another nonland permanent you control is returned to its
    /// owner's hand": leaves the battlefield for a hand, judged as it last
    /// existed.
    ReturnedToHand(TriggerObj),
    /// "When that land dies or is put into exile" (earthbend's delayed
    /// trigger, CR 701.66a), "When enchanted permanent dies or is put into
    /// exile" (Kaya's Ghostform): leaves the battlefield for a graveyard or
    /// exile, judged as it last existed.
    DiesOrExiled(TriggerObj),
    /// "Whenever ~ deals combat damage to a player".
    DealsCombatDamageToPlayer(TriggerObj),
    /// "Whenever one or more creatures you control deal combat damage to a
    /// player", "... one or more Allies you control ..." (CR 510.2,
    /// 603.2c): once for each player dealt combat damage by matching
    /// creatures in one combat damage step; that player is the triggering
    /// player.
    CreaturesDealCombatDamage(ObjFilter),
    /// "Whenever one or more Heroes you control deal damage to a player"
    /// (CR 603.2c): any damage, once for each player dealt damage by
    /// matching creatures at once; that player is the triggering player.
    CreaturesDealDamage(ObjFilter),
    /// "Whenever a source you control deals noncombat damage to an
    /// opponent" (`yours`), "Whenever an opponent is dealt noncombat
    /// damage", "Whenever one or more opponents are dealt noncombat damage"
    /// (`batch`: once for the damage dealt at once to each opponent). The
    /// opponent is the triggering player and the damage the trigger's
    /// amount ("that many").
    NoncombatDamageToOpponent { yours: bool, batch: bool },
    /// "Whenever ~ deals combat damage to a player or battle" (CR 310.6):
    /// the player dealt damage is the triggering player.
    DealsCombatDamageToPlayerOrBattle(TriggerObj),
    /// "Whenever ~ deals combat damage to a player or planeswalker": the
    /// player dealt damage, or the planeswalker's controller, is the
    /// triggering player.
    DealsCombatDamageToPlayerOrPlaneswalker(TriggerObj),
    /// "Whenever this creature attacks a battle" (CR 310.5, 508.1).
    AttacksBattle(TriggerObj),
    /// "When you unlock this door" (CR 709.5h): the door of the Room
    /// whose half has this ability (0 left, 1 right, set by
    /// `CardPool::add_room_doors`) is unlocked, as it enters or later.
    UnlocksDoor { door: u8 },
    /// "Whenever you fully unlock a Room" (CR 709.5i).
    FullyUnlocks { whose: Who },
    /// "Whenever you collect evidence" (CR 701.59).
    CollectsEvidence { whose: Who },
    /// "Whenever you forage" (CR 701.61).
    Forages { whose: Who },
    /// "Whenever you commit a crime" (CR 700.13), `your_turn`: "...
    /// during your turn". The controller is the triggering player.
    CommitsCrime { your_turn: bool },
    /// "Whenever an opponent gains control of a permanent from you" (CR
    /// 613.1b): a permanent the controller controlled comes under an
    /// opponent's control. It is the triggering object; the opponent is
    /// the triggering player.
    OpponentGainsControl,
    /// "Whenever you gain life".
    GainsLife { whose: Who },
    /// "Whenever an opponent loses life", "Whenever a player loses life"
    /// (CR 119.3): once per life-losing event, damage included; the player
    /// is the triggering player and the amount lost the trigger's amount
    /// ("that much").
    LosesLife { whose: Who },
    /// "Whenever you gain life for the first time each turn": the life
    /// gain event that starts the player's life gained this turn.
    GainsLifeFirst { whose: Who },
    /// "Whenever you roll one or more dice" (CR 706): once per
    /// instruction to roll, not per die.
    RollsDice { whose: Who },
    /// "Whenever you win a coin flip" (CR 705.2).
    WinsFlip { whose: Who },
    /// "Whenever you cast a spell"; `noncreature` or `instant_or_sorcery`
    /// restrict it. `or_copy` is magecraft's "Whenever you cast or copy an
    /// instant or sorcery spell" (CR 207.2c): a copy of a spell made on
    /// the stack triggers it too (CR 707.10, `Event::SpellCopied`).
    CastsSpell {
        whose: Who,
        noncreature: bool,
        instant_or_sorcery: bool,
        or_copy: bool,
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
        /// "blocks or becomes blocked by one or more Orcs": once for the
        /// declaration, on the first matching pair, not once per pair.
        #[serde(default)]
        any: bool,
    },
    /// Ward (CR 702.21a): "Whenever this permanent becomes the target of a
    /// spell or ability an opponent controls"; the spell or ability is the
    /// triggering object.
    TargetedByOpponent,
    /// "Whenever this creature becomes the target of a spell or ability
    /// for the first time each turn" (Shimmering Glasskite): any
    /// controller's; the spell or ability is the triggering object. The
    /// compiler limits it to once each turn (`TurnLimit::Triggers(1)`),
    /// which is the same thing.
    TargetedFirstTime,
    /// Valiant (CR 207.2c): "Whenever this creature becomes the target
    /// of a spell or ability you control for the first time each turn":
    /// the spell or ability is the triggering object.
    TargetedByYouFirstTime,
    /// "Whenever an Aura becomes attached to this creature" (CR 701.3a,
    /// 303.4): the object attached, which must match, is the triggering
    /// object.
    BecomesAttachedTo(ObjFilter),
    /// "Whenever this creature becomes tapped".
    BecomesTapped(TriggerObj),
    /// "Whenever this creature becomes untapped" (inspired).
    BecomesUntapped(TriggerObj),
    /// "Whenever this creature is dealt damage" (enrage); the damaged
    /// object is the triggering object.
    DealtDamage(TriggerObj),
    /// "Whenever this creature is dealt 3 or more damage": one damage event
    /// of at least that much (damage dealt at different times doesn't add
    /// up).
    DealtDamageAtLeast(TriggerObj, u32),
    /// "Whenever this creature is dealt combat damage" (CR 510.2).
    DealtCombatDamage(TriggerObj),
    /// "Whenever this creature deals damage to a creature / an opponent /
    /// a player": the damaged creature (or player) is the triggering
    /// object (player); `combat` only for combat damage.
    DealsDamage {
        what: TriggerObj,
        to: DamageTo,
        combat: bool,
    },
    /// "Whenever this creature deals damage to a Vampire", "to a Goblin or
    /// Orc": the damaged permanent, which must match `filter`, is the
    /// triggering object ("that creature").
    DealsDamageTo { what: TriggerObj, filter: ObjFilter },
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
    /// "When this card becomes plotted" (CR 702.170): queued as the card
    /// is plotted, from exile, never by an event.
    BecomesPlotted,
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
    /// "When you discard this card" (CR 701.9): triggers from the card
    /// itself as it is discarded, controlled by the discarding player.
    DiscardsThis,
    /// "When you cast this spell" (CR 603.2, 601.2i): triggers from the
    /// spell itself as it becomes cast, so it goes on the stack above the
    /// spell and resolves first; countering the spell doesn't stop it.
    /// "When you cast or cycle ~" is this and `Cycles { this }`.
    CastThis,
    /// "When you next cast an instant or sorcery spell this turn" (CR
    /// 603.7c): a delayed trigger on the inner event that triggers only
    /// the first time it happens, then ends.
    Next(Box<TriggerSpec>),
    /// "Whenever you sacrifice another Eldrazi" (CR 701.21a: only a
    /// permanent's controller sacrifices it): the sacrificed permanent,
    /// matching the filter as it is sacrificed, is the triggering object.
    Sacrifices(ObjFilter),
    /// "Whenever you cast your second spell each turn" and kin: the
    /// spell's place among the spells its caster cast this turn.
    CastsNth { whose: Who, nth: SpellOrdinal },
    /// "When this creature enters and whenever you cast ...", "When you
    /// cycle this card and when this creature dies": one ability with two
    /// trigger events.
    Either(Box<(TriggerSpec, TriggerSpec)>),
    /// "When you control no Islands" (a state trigger, CR 603.8): the
    /// pool's `n`th static condition, judged for the controller whenever a
    /// player would receive priority. It doesn't trigger again while the
    /// ability is waiting or on the stack.
    State(u16),
    /// "Whenever one or more artifacts you control enter": once for all
    /// the matching objects that enter together (CR 603.2c), the number
    /// of them being the trigger's amount ("that much"). Objects entering
    /// before the next time a player would receive priority count as
    /// together.
    EntersBatch(ObjFilter),
    /// "Whenever one or more other creatures you control die" (`dies`),
    /// "Whenever one or more permanents you control leave the
    /// battlefield": once for the matching objects leaving together (CR
    /// 603.2c), judged as they last existed (CR 603.10a); their number is
    /// the trigger's amount.
    LeavesBatch { filter: ObjFilter, dies: bool },
    /// "Whenever day becomes night or night becomes day" (CR 731.2): not
    /// as it first becomes day.
    DayNight,
    /// "Whenever you sacrifice another permanent", "Whenever a player
    /// sacrifices a creature", "when you sacrifice it" (CR 701.21): the
    /// sacrificed permanent, judged as it is sacrificed (still on the
    /// battlefield), is the triggering object; `whose` is who sacrificed
    /// it, its controller (CR 701.21a).
    Sacrificed { what: TriggerObj, whose: Who },
    /// "Whenever you draw a card", "Whenever a player draws a card" (CR
    /// 121.2): once per card drawn.
    Draws { whose: Who },
    /// "Whenever a player discards a card", "Whenever you discard a
    /// creature card" (CR 701.9): once per card, judged as it is
    /// discarded (still in the hand); the card is the triggering object.
    Discards { whose: Who, filter: ObjFilter },
    /// "Whenever one or more cards leave your graveyard", "... creature
    /// cards ..." (CR 603.2c, 603.10a): once for the matching cards that
    /// leave the controller's graveyard together, however they leave;
    /// their number is the trigger's amount.
    LeavesGraveyard(ObjFilter),
    /// "Whenever a land card is put into your graveyard from anywhere":
    /// once per matching card its controller owns going to a graveyard,
    /// the card (as it was) the triggering object.
    PutIntoGraveyard(ObjFilter),
    /// "When Purity is put into a graveyard from anywhere" (CR 603.6a): the
    /// card itself goes to a graveyard from any zone; it triggers from the
    /// graveyard, controlled by the card's owner.
    ThisToGraveyard,
    /// "Whenever a creature is put into your graveyard from the
    /// battlefield": a matching permanent (as it last existed) its
    /// controller owns dies, whoever controlled it; the permanent the
    /// triggering object.
    PutIntoGraveyardFromBattlefield(ObjFilter),
    /// "Whenever a creature enters from your graveyard": a matching
    /// permanent that entered from its controller's graveyard (the
    /// source's controller's), the triggering object.
    EntersFromGraveyard(ObjFilter),
    /// "Whenever one or more +1/+1 counters are put on this creature" (CR
    /// 122.1): once per instruction putting counters of that kind on it,
    /// however many.
    CountersPut { what: TriggerObj, kind: CounterKind },
    /// "When the fourth plan counter is put on this enchantment": counters
    /// of that kind put on the source take their number from below `n` to
    /// `n` or more.
    NthCounterPut { kind: CounterKind, n: u32 },
    /// The "one or more" form of a per-object trigger (CR 603.2c): "Whenever
    /// one or more creature cards are put into your graveyard from
    /// anywhere", "Whenever one or more creatures you control become
    /// tapped": once for the matching objects of events that happen
    /// together (before the next time a player would receive priority),
    /// their number the trigger's amount.
    Batch(Box<TriggerSpec>),
    /// The per-object form of a "one or more" trigger: "Whenever a card
    /// leaves your graveyard" (once for each card), "Whenever you put a
    /// +1/+1 counter on a creature you control" (the same as "one or more",
    /// in the older wording).
    Each(Box<TriggerSpec>),
    /// "At the beginning of the end step on your next turn" (a delayed
    /// trigger's step, `BeginningOfStep` of yours): not in the turn the
    /// delayed trigger was created.
    OnNextTurn(Box<TriggerSpec>),
    /// "Whenever you put one or more +1/+1 counters on a creature you
    /// control", "... counters on a Goblin, Orc, or Army you control"
    /// (`kind` `None`: any kind): once per permanent counters are put on
    /// by one instruction of a spell or ability its controller controls
    /// (CR 122.6); their number is the trigger's amount ("that many").
    YouPutCounters {
        what: TriggerObj,
        kind: Option<CounterKind>,
    },
    /// "Whenever you expend 4" (CR 700.14): as the controller spends their
    /// `n`th total mana to cast spells during a turn, judged as the spell
    /// that paid it becomes cast.
    Expends(u8),
    /// A Saga's chapter ability (CR 714.2): bit `n` set for chapter `n`
    /// ("I, II —" is bits 1 and 2, CR 714.2b). It triggers when lore
    /// counters put on the source take their number from below `n` to `n`
    /// or more (CR 714.2c), once for each chapter number so crossed.
    Chapter(u16),
    /// "When this Class becomes level 2" (CR 716.2c): the source becomes
    /// that stage of its card (`Effect::BecomeStage`).
    BecomesStage(u8),
    /// "Whenever you discard one or more cards" (CR 603.2c): once for the
    /// cards `whose` player discards together, their number the trigger's
    /// amount ("that many").
    DiscardsBatch { whose: Who },
    /// "Whenever you sacrifice one or more Foods" (CR 603.2c, 701.21):
    /// once for the matching permanents sacrificed together, their number
    /// the trigger's amount.
    SacrificesBatch(ObjFilter),
    /// "Whenever you attack with one or more Faeries" (CR 508.1m): once
    /// per declaration of attackers by the controller in which a matching
    /// creature attacks; with `player`, "Whenever you attack a player with
    /// one or more creatures with power 4 or greater", once for each
    /// player a matching creature attacks (that player is the triggering
    /// player).
    AttacksWithFilter { filter: ObjFilter, player: bool },
    /// "Whenever you gain or lose life" (CR 119.3, 119.9), `your_turn`:
    /// "... during your turn".
    GainsOrLosesLife { your_turn: bool },
    /// "Whenever you scry or surveil" (CR 701.22, 701.25): once per
    /// instruction to scry or surveil.
    ScriesOrSurveils,
    /// "Whenever you scry" (CR 701.22): once per instruction to scry, not
    /// surveil.
    Scries,
    /// "Whenever this creature saddles a Mount or crews a Vehicle" (CR
    /// 702.122b, 702.171c): it is tapped to pay a crew or saddle cost; the
    /// Mount or Vehicle is the triggering object ("that Mount or
    /// Vehicle").
    CrewsOrSaddles(TriggerObj),
    /// "Whenever this creature becomes saddled for the first time each
    /// turn" (CR 702.171b: it stays saddled until the turn ends, so the
    /// first time is the only time).
    BecomesSaddled(TriggerObj),
    /// "Whenever a creature you control explores" (CR 701.44): the
    /// exploring creature is the triggering object.
    Explores(TriggerObj),
    /// "Whenever you tap an untapped creature an opponent controls": a
    /// spell or ability the controller controls taps an untapped
    /// permanent matching the filter; it is the triggering object.
    YouTap(ObjFilter),
    /// "Whenever you activate an ability of a creature", "... an exhaust
    /// ability", "Whenever an opponent activates a loyalty ability" (CR
    /// 602.2): as an activated ability matching `kind` is activated by a
    /// player matching `whose` (the triggering player).
    Activates { whose: Who, kind: ActivationKind },
    /// "Whenever this creature transforms into Ashling, Rimebound" (CR
    /// 701.27): the source transformed and now shows this ability's face.
    Transforms,
    /// "Whenever you tap this creature for mana", "Whenever a player taps
    /// a land for mana", "Whenever an opponent taps a Mountain for mana"
    /// (CR 106.11a): a permanent matching `what`, controlled by a player
    /// matching `whose`, is tapped for mana by a mana ability (activated,
    /// or tapped by payment). Not a mana ability (CR 605.1b: it adds no
    /// mana), so it uses the stack. The permanent is the triggering
    /// object, its controller the triggering player ("that player").
    TapsForMana { what: TriggerObj, whose: Who },
    /// "When enchanted creature becomes the target of a spell" (`spells`),
    /// "Whenever a permanent you control becomes the target of a spell or
    /// ability an opponent controls" (`whose` Opponents): a matching
    /// permanent becomes a target of a spell (or, without `spells`, a spell
    /// or ability) whose controller matches `whose`. The targeted permanent
    /// is the triggering object, the spell's or ability's controller the
    /// triggering player.
    TargetedBy {
        what: TriggerObj,
        spells: bool,
        whose: Who,
    },
}

/// The event whose triggers an `AbilityDef::TriggerAgain` with a cause
/// filter doubles: a permanent entering, a creature dying, a creature
/// attacking (CR 603.2d).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
pub enum TriggerCause {
    #[default]
    Enters,
    Dies,
    Attacks,
}

/// Which activated abilities a `TriggerSpec::Activates` trigger sees.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ActivationKind {
    /// "an ability of a creature": the source is a creature (mana
    /// abilities included, CR 605.3a).
    OfCreature,
    /// "an exhaust ability" (CR 702.177).
    Exhaust,
    /// "a loyalty ability" (CR 606.3).
    Loyalty,
}

/// Which of a player's spells this turn a cast trigger wants.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum SpellOrdinal {
    /// "your first spell each turn".
    First,
    /// "your second spell each turn".
    Second,
    /// "your third spell each turn".
    Third,
    /// "a spell other than your first spell each turn".
    NotFirst,
    /// "your first spell during each opponent's turn".
    FirstOnOpponentsTurn,
    /// "a spell during an opponent's turn": any spell cast on another
    /// player's turn.
    OnOpponentsTurn,
    /// "their first noncreature spell each turn": a noncreature spell, the
    /// first of its caster's noncreature spells this turn (counted by the
    /// trigger, not by `holds`).
    FirstNoncreature,
    /// "your first instant or sorcery spell each turn": the first of its
    /// caster's instant and sorcery spells this turn (counted by the
    /// trigger, not by `holds`).
    FirstInstantOrSorcery,
    /// "a spell from exile": any spell cast from exile (checked by the
    /// trigger, not by `holds`).
    FromExile,
}

impl SpellOrdinal {
    /// Whether the `n`th spell its caster cast this turn qualifies, cast
    /// on its caster's turn or not.
    pub fn holds(self, n: u8, own_turn: bool) -> bool {
        match self {
            SpellOrdinal::First
            | SpellOrdinal::FirstNoncreature
            | SpellOrdinal::FirstInstantOrSorcery => n == 1,
            SpellOrdinal::Second => n == 2,
            SpellOrdinal::Third => n == 3,
            SpellOrdinal::NotFirst => n >= 2,
            SpellOrdinal::FirstOnOpponentsTurn => n == 1 && !own_turn,
            SpellOrdinal::OnOpponentsTurn => !own_turn,
            SpellOrdinal::FromExile => true,
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
    /// A trigger on a permanent entering the battlefield (alone or as one
    /// of two events): what Torpor Orb stops.
    pub fn is_enters(&self) -> bool {
        match self {
            TriggerSpec::Enters(_)
            | TriggerSpec::EntersBatch(_)
            | TriggerSpec::EntersFromGraveyard(_) => true,
            TriggerSpec::Either(pair) => pair.0.is_enters() || pair.1.is_enters(),
            _ => false,
        }
    }

    /// Valiant's trigger, alone or as one of two events.
    pub fn is_valiant(&self) -> bool {
        match self {
            TriggerSpec::TargetedByYouFirstTime => true,
            TriggerSpec::Either(pair) => pair.0.is_valiant() || pair.1.is_valiant(),
            _ => false,
        }
    }

    /// "When you cycle this card", alone or as one of two events.
    pub fn cycles_this(&self) -> bool {
        match self {
            TriggerSpec::Cycles { this, .. } => *this,
            TriggerSpec::Either(pair) => pair.0.cycles_this() || pair.1.cycles_this(),
            _ => false,
        }
    }

    /// "When you discard this card", alone or as one of two events.
    pub fn discards_this(&self) -> bool {
        match self {
            TriggerSpec::DiscardsThis => true,
            TriggerSpec::Either(pair) => pair.0.discards_this() || pair.1.discards_this(),
            _ => false,
        }
    }

    /// "Whenever you tap this creature for mana", alone or as one of two
    /// events.
    pub fn taps_for_mana(&self) -> bool {
        match self {
            TriggerSpec::TapsForMana { .. } => true,
            TriggerSpec::Either(pair) => pair.0.taps_for_mana() || pair.1.taps_for_mana(),
            _ => false,
        }
    }

    /// "When you cast this spell", alone or as one of two events.
    pub fn casts_this(&self) -> bool {
        match self {
            TriggerSpec::CastThis => true,
            TriggerSpec::Either(pair) => pair.0.casts_this() || pair.1.casts_this(),
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
    #[allow(clippy::too_many_lines)] // one arm per trigger kind
    pub fn watch_mask(&self) -> u32 {
        match self {
            TriggerSpec::Enters(_)
            | TriggerSpec::Dies(_)
            | TriggerSpec::Leaves(_)
            | TriggerSpec::LeavesWithoutDying(_)
            | TriggerSpec::ReturnedToHand(_)
            | TriggerSpec::DiesOrExiled(_)
            | TriggerSpec::EntersBatch(_)
            | TriggerSpec::LeavesBatch { .. }
            | TriggerSpec::LeavesGraveyard(_)
            | TriggerSpec::PutIntoGraveyard(_)
            | TriggerSpec::ThisToGraveyard
            | TriggerSpec::PutIntoGraveyardFromBattlefield(_)
            | TriggerSpec::EntersFromGraveyard(_) => WATCH_ZONE_CHANGE,
            TriggerSpec::AttacksOrBlocks(_) => WATCH_ATTACK | WATCH_BLOCK,
            TriggerSpec::BeginningOfStep { .. } | TriggerSpec::AttachedControllersStep(_) => {
                WATCH_STEP_BEGIN
            }
            TriggerSpec::Attacks(_)
            | TriggerSpec::AttacksFirstTime(_)
            | TriggerSpec::AttacksBattle(_)
            | TriggerSpec::AttacksAlone(_)
            | TriggerSpec::AttacksWith { .. }
            | TriggerSpec::PlayerAttacksWith { .. }
            | TriggerSpec::AttacksWithFilter { .. } => WATCH_ATTACK,
            TriggerSpec::Blocks(_)
            | TriggerSpec::BecomesBlocked(_)
            | TriggerSpec::BlocksOrBlocked(_)
            | TriggerSpec::BlockedBy(_)
            | TriggerSpec::BlockPair { .. }
            | TriggerSpec::AttacksUnblocked(_) => WATCH_BLOCK,
            TriggerSpec::DealsCombatDamageToPlayer(_)
            | TriggerSpec::CreaturesDealCombatDamage(_)
            | TriggerSpec::CreaturesDealDamage(_)
            | TriggerSpec::NoncombatDamageToOpponent { .. }
            | TriggerSpec::DealsCombatDamageToPlayerOrBattle(_)
            | TriggerSpec::DealsCombatDamageToPlayerOrPlaneswalker(_)
            | TriggerSpec::DealtDamage(_)
            | TriggerSpec::DealtDamageAtLeast(..)
            | TriggerSpec::DealtCombatDamage(_)
            | TriggerSpec::DealsDamage { .. }
            | TriggerSpec::DealsDamageTo { .. } => WATCH_DAMAGE,
            TriggerSpec::BecomesTapped(_)
            | TriggerSpec::BecomesUntapped(_)
            | TriggerSpec::TapsForMana { .. } => WATCH_TAP,
            TriggerSpec::TurnedFaceUp(_) => WATCH_FACE_UP,
            TriggerSpec::DrawsNth { .. } | TriggerSpec::Draws { .. } => WATCH_DRAW,
            TriggerSpec::Exploits
            | TriggerSpec::BecomesPlotted
            | TriggerSpec::Reflexive
            | TriggerSpec::State(_) => 0,
            TriggerSpec::GainsLife { .. }
            | TriggerSpec::LosesLife { .. }
            | TriggerSpec::GainsLifeFirst { .. }
            | TriggerSpec::GainsOrLosesLife { .. } => WATCH_LIFE_CHANGE,
            TriggerSpec::RollsDice { .. } | TriggerSpec::WinsFlip { .. } => WATCH_ROLL,
            TriggerSpec::CastsSpell { .. }
            | TriggerSpec::CastsFiltered { .. }
            | TriggerSpec::CastsNth { .. }
            | TriggerSpec::CastThis
            | TriggerSpec::Expends(_) => WATCH_CAST,
            TriggerSpec::BecomesTarget(_)
            | TriggerSpec::TargetedBy { .. }
            | TriggerSpec::TargetedByOpponent
            | TriggerSpec::TargetedFirstTime
            | TriggerSpec::TargetedByYouFirstTime
            | TriggerSpec::CommitsCrime { .. } => WATCH_TARGETED,
            TriggerSpec::BecomesAttachedTo(_) => WATCH_ATTACH,
            TriggerSpec::Discards { .. }
            | TriggerSpec::DiscardsThis
            | TriggerSpec::DiscardsBatch { .. }
            | TriggerSpec::Cycles {
                or_discard: true, ..
            } => WATCH_DISCARD,
            TriggerSpec::Cycles { .. } => WATCH_CYCLE,
            TriggerSpec::Either(pair) => pair.0.watch_mask() | pair.1.watch_mask(),
            TriggerSpec::Next(inner)
            | TriggerSpec::Batch(inner)
            | TriggerSpec::Each(inner)
            | TriggerSpec::OnNextTurn(inner) => inner.watch_mask(),
            TriggerSpec::UnlocksDoor { .. }
            | TriggerSpec::BecomesStage(_)
            | TriggerSpec::FullyUnlocks { .. }
            | TriggerSpec::CollectsEvidence { .. }
            | TriggerSpec::Forages { .. }
            | TriggerSpec::OpponentGainsControl
            | TriggerSpec::DayNight => WATCH_DESIG,
            TriggerSpec::Sacrifices(_)
            | TriggerSpec::Sacrificed { .. }
            | TriggerSpec::SacrificesBatch(_) => WATCH_SACRIFICE,
            TriggerSpec::CountersPut { .. }
            | TriggerSpec::NthCounterPut { .. }
            | TriggerSpec::YouPutCounters { .. }
            | TriggerSpec::Chapter(_) => WATCH_COUNTERS,
            TriggerSpec::ScriesOrSurveils
            | TriggerSpec::Scries
            | TriggerSpec::CrewsOrSaddles(_)
            | TriggerSpec::BecomesSaddled(_)
            | TriggerSpec::Explores(_)
            | TriggerSpec::Activates { .. }
            | TriggerSpec::Transforms
            | TriggerSpec::YouTap(_) => WATCH_ACT,
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
    /// "if it had a +1/+1 counter on it" (Promising Duskmage), judged the
    /// same way.
    HadCounters(CounterKind),
    /// "if it had counters on it" of the triggering object, any kind, as
    /// it last existed (Buzzard-Wasp Colony's "Whenever another creature
    /// you control dies, if it had counters on it, ...").
    TriggerHadCounters,
    /// "if no spells were cast last turn" (werewolves).
    NoSpellsLastTurn,
    /// "if a player cast two or more spells last turn".
    TwoSpellsLastTurn,
    /// Echo (CR 702.30a): the source came under its controller's control
    /// since their last upkeep.
    EchoDue,
    /// "if it isn't renowned" (renown, CR 702.112a).
    NotRenowned,
    /// "if it was blocking" (`true`), "if it wasn't blocking" (`false`) in
    /// a leaves-the-battlefield trigger: the source as it last existed on
    /// the battlefield (CR 603.10a).
    WasBlocking(bool),
    /// "if this creature isn't prepared" (CR 722.3a).
    NotPrepared,
    /// Evolve (CR 702.100a): the triggering creature's power is greater
    /// than the source's, or its toughness is greater than the source's.
    Evolves,
    /// Increment (CR 702.191a): the source is a creature and the amount of
    /// mana spent to cast the triggering spell is greater than its power or
    /// its toughness.
    Increments,
    /// The pool's `n`th static condition ("if you control an artifact",
    /// "if you have 5 or less life").
    Static(u16),
    /// "if it was evoked" (CR 702.74a).
    Evoked,
    /// The pool's `n`th static condition as a trigger condition, not an
    /// intervening "if": "Whenever this creature attacks while you control
    /// two or more artifacts" is judged only as the event happens, so the
    /// ability resolves even if the condition no longer holds (CR 603.2,
    /// unlike CR 603.4).
    While(u16),
    /// `While` on the second of two trigger events joined by "and" ("When
    /// this creature enters and whenever it attacks while saddled"): the
    /// first event triggers it regardless (CR 603.2).
    WhileSecond(u16),
    /// Training (CR 702.149a): "Whenever this creature and at least one
    /// other creature with power greater than this creature's power
    /// attack": judged only as attackers are declared (another attacking
    /// creature its controller controls has greater power).
    Trains,
    /// Dethrone (CR 702.105a): "Whenever this creature attacks the player
    /// with the most life or tied for most life": judged only as it
    /// attacks; attacking a planeswalker or battle doesn't count.
    Dethrones,
    /// Blitz (CR 702.152a): "When this creature dies, draw a card." is
    /// the source's only while its blitz cost was paid (as it last
    /// existed).
    Blitzed,
    /// Tribute (CR 702.104b): "if tribute wasn't paid", judged as the
    /// ability resolves (the permanent as it is, or last existed).
    TributeNotPaid,
    /// Melee (CR 702.121a) in a two-player game: an opponent was attacked
    /// with a creature this combat (a planeswalker doesn't count), so the
    /// bonus "for each opponent you attacked" is one. Judged only as the
    /// ability triggers: attackers are declared once each combat.
    AttackedOpponent,
    /// Meld's intervening "if" (CR 603.4, 701.42a): "if you both own and
    /// control this creature and a creature named <its pair>", the pair
    /// named by the ability's `Effect::Meld`. Set by the compiler on a
    /// triggered ability that melds; the effect prints the words.
    MeldPair,
}

/// A triggered ability (CR 603).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct TriggeredAbility {
    pub trigger: TriggerSpec,
    pub condition: Option<Condition>,
    /// Targets chosen when the ability is put on the stack (CR 603.3d).
    pub targets: Vec<TargetSpec>,
    pub effects: Vec<Effect>,
    /// Works only while its card is in its owner's graveyard, not on the
    /// battlefield (CR 113.6k: "Whenever you cast a multicolored spell, you
    /// may return this card from your graveyard to your hand").
    #[serde(default)]
    pub from_graveyard: bool,
    /// "This ability triggers only once each turn." and kin: a count kept
    /// per object and ability, so a new object after a zone change starts
    /// fresh (CR 400.7).
    #[serde(default)]
    pub limit: TurnLimit,
}

/// A triggered ability's limit each turn.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
pub enum TurnLimit {
    #[default]
    None,
    /// "This ability triggers only once each turn." (1), "only twice each
    /// turn" (2): past that many, it doesn't trigger at all this turn.
    Triggers(u8),
    /// "Do this only once each turn.": the ability triggers every time,
    /// but its first "you may" is offered only until it is done once this
    /// turn; declining doesn't count.
    Do,
    /// "This ability triggers only once.": once for the object, ever (a
    /// new object after a zone change starts fresh, CR 400.7).
    Ever,
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
    /// "Sacrifice Blazing Torch" in an ability an Aura or Equipment grants
    /// the permanent it is attached to ("Equipped creature has '{T},
    /// Sacrifice Blazing Torch: ...'"): the granting permanent, not the
    /// one with the ability (`Recipient::Granter`).
    Granter,
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
    /// "Exile this artifact": the permanent exiles itself from the
    /// battlefield as the cost is paid (CR 602.2b).
    #[serde(default)]
    pub exile_this: bool,
    /// "Exile this card from your hand" (Simian Spirit Guide): an ability
    /// activated from the hand, the card exiled as the cost is paid.
    #[serde(default)]
    pub exile_hand: bool,
    /// Crew N (CR 702.122): tap any number of other untapped creatures you
    /// control with total power N or more.
    pub crew: u32,
    /// "Remove a charge counter from this artifact" (CR 122.8).
    pub remove_counters: Option<(CounterKind, u32)>,
    /// "Put a -1/-1 counter on this creature" (CR 118.3, 602.2b): the
    /// counters go on the source as the cost is paid; it may die to state-
    /// based actions before the ability resolves.
    #[serde(default)]
    pub put_counters: Option<(CounterKind, u32)>,
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
    /// "Exile a blue card from your hand" (a pitch spell's alternative
    /// cost, CR 118.9): the `discard` cards are exiled from the hand
    /// rather than discarded.
    #[serde(default)]
    pub discard_exile: bool,
    /// A loyalty ability's cost (CR 606): +N puts, −N removes that many
    /// loyalty counters; nonzero marks the ability as a loyalty ability
    /// (sorcery speed, one per planeswalker per turn, CR 606.3).
    pub loyalty: i32,
    /// {T}: tap the source. A creature's {T} ability needs the source to
    /// have been under its controller's control since their most recent
    /// turn began (CR 302.6).
    pub tap: bool,
    pub sacrifice: Option<SacCost>,
    /// How many permanents a `SacCost::Filter` cost chooses ("Tap two
    /// untapped creatures you control"); 0 and 1 are one. The player
    /// chooses the first and the engine the rest.
    #[serde(default)]
    pub count: u32,
    /// "Exile two cards from your graveyard", "exile a creature card from
    /// your graveyard" (CR 118.3): cards other than the source, the oldest
    /// first.
    #[serde(default)]
    pub exile_cards: Option<Box<GraveExile>>,
    /// "Reveal your hand" (Land Grant's alternative cost, CR 701.20):
    /// every card left in the payer's hand becomes known.
    #[serde(default)]
    pub reveal_hand: bool,
    /// "Reveal this card from your hand" (forecast, CR 702.57a): the
    /// ability is activated from the hand and the card becomes known.
    #[serde(default)]
    pub reveal_self: bool,
    /// "Blight N" (CR 701.68a): put N -1/-1 counters on a creature the
    /// payer controls (the engine picks the toughest).
    #[serde(default)]
    pub blight: u32,
    /// "Behold a Kithkin" (CR 701.69a): choose a Kithkin you control or
    /// reveal a Kithkin card from your hand; "and exile it".
    #[serde(default)]
    pub behold: Option<Box<Behold>>,
    /// "Collect evidence N" (CR 701.59a): exile cards with total mana value
    /// N or greater from the payer's graveyard (the engine picks them).
    pub evidence: u32,
    /// "Forage" (CR 701.61a): exile three cards from the payer's graveyard
    /// or sacrifice a Food (the engine picks: the three oldest cards if
    /// there are three, else a Food).
    #[serde(default)]
    pub forage: bool,
    /// Craft's materials (CR 702.167a): "Exile another artifact you
    /// control or an artifact card from your graveyard": one permanent the
    /// payer controls or card in their graveyard matching the filter,
    /// other than the source (the engine picks, graveyard cards first).
    pub craft: Option<Box<ObjFilter>>,
    /// "Waterbend {5}" (CR 701.67a): the generic mana of `mana` may be
    /// paid, {1} each, by tapping untapped artifacts and creatures the
    /// payer controls. Every waterbend cost is generic mana only.
    #[serde(default)]
    pub waterbend: bool,
    /// "Exert this creature" (CR 701.43a): it won't untap during its
    /// controller's next untap step.
    #[serde(default)]
    pub exert: bool,
    /// "Discard your hand": every card in the payer's hand (none is
    /// payable too).
    #[serde(default)]
    pub discard_hand: bool,
    /// "As an additional cost to cast this spell, pay X life" (CR 107.3,
    /// 119.4): X announced as the spell is cast; payable only with at
    /// least that much life.
    #[serde(default)]
    pub life_x: bool,
    /// "Put a -1/-1 counter on a creature you control" (Lethal Sting): the
    /// older wording of blight (CR 701.68a), the `blight` counters.
    #[serde(default)]
    pub blight_put: bool,
    /// "Put a card an opponent owns from exile into that player's
    /// graveyard" (a processor's cost): that many cards in exile owned by
    /// the payer's opponents go to their owners' graveyards (the engine
    /// picks: the longest exiled first).
    #[serde(default)]
    pub process: u32,
    /// "Remove a +1/+1 counter from a creature you control": the
    /// `remove_counters` come off a creature the payer controls rather
    /// than the source (the engine picks the one with the most of them).
    #[serde(default)]
    pub remove_from_creature: bool,
    /// "As an additional cost to cast this spell, sacrifice any number of
    /// creatures" (1; "you may sacrifice any number of Spirits", 2): X
    /// announced as the spell is cast (CR 601.2b, 107.3), at most the
    /// matching permanents, and X permanents matching the `sacrifice`
    /// filter sacrificed (the caster chooses the first, the engine the
    /// rest). 0: none.
    #[serde(default)]
    pub sacrifice_any: u8,
    /// "Remove X ki counters from this creature" (CR 107.3, 602.2b): the
    /// `remove_counters` count is the ability's X, announced as it is
    /// activated, at most the source's counters of that kind.
    #[serde(default)]
    pub remove_x: bool,
    /// {Q}, the untap symbol (CR 107.6): untap the source. Payable only
    /// while it is tapped; a creature's needs it to have been controlled
    /// since the turn began (CR 302.6).
    #[serde(default)]
    pub untap: bool,
}

/// One alternative of `Effect::UnlessPaysCost`: a cost paid by a player
/// other than the source's controller as an effect resolves. Of `cost`,
/// mana, life, discarded cards (of the payer's choice, any filter, or at
/// random), waterbend, sacrificed permanents (the payer chooses each),
/// collect evidence and blight are paid; `life_count` is life equal to a
/// number counted as it resolves ("Pay life equal to this creature's
/// power", CR 119.4), and `poison` poison counters the payer gets.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
pub struct UnlessCost {
    pub cost: Cost,
    pub life_count: Option<Num>,
    pub poison: u32,
}

/// A behold cost (CR 701.69a): `n` objects matching `filter`, each a
/// permanent the payer controls or a card revealed from their hand; with
/// `exile`, "behold a Kithkin and exile it", they are exiled, linked to
/// the spell's permanent ("return the exiled card to its owner's hand",
/// CR 607.2a).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct Behold {
    pub filter: ObjFilter,
    pub n: u32,
    pub exile: bool,
    /// "Reveal a Kithkin card from your hand" (the older wording, CR
    /// 601.2b): a card from the hand only, never a permanent. As the
    /// spell is cast, controlling a matching permanent also counts as
    /// paid ("If you revealed a Dragon card or controlled a Dragon as you
    /// cast this spell").
    #[serde(default)]
    pub hand: bool,
    /// "Reveal X red cards from your hand" (an activated ability's cost):
    /// `n` is the ability's X, announced as it is activated, at most the
    /// matching cards.
    #[serde(default)]
    pub x: bool,
}

/// A cost that exiles cards from its payer's graveyard: `n` of them (X,
/// announced as the spell is cast, with `x`), each matching `filter`.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct GraveExile {
    pub n: u32,
    pub x: bool,
    pub filter: Option<ObjFilter>,
    /// "the top three black cards of your graveyard": the most recent
    /// matching cards rather than the oldest.
    #[serde(default)]
    pub top: bool,
}

/// A card's permission to be cast from its owner's graveyard
/// (`CardDef::graveyard_cast`).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
pub struct GraveyardCast {
    /// Offered only while this holds ("as long as you control a Zombie",
    /// "if you gained life this turn"; "Max speed —").
    pub condition: Option<u16>,
    /// Printed "as long as" rather than "if".
    pub as_long_as: bool,
    /// Printed "Max speed — " first (the condition is max speed).
    pub max_speed: bool,
    /// The cost's mana replaces the mana cost ("by paying {2}{W} rather
    /// than paying its mana cost"); otherwise it is the mana cost and the
    /// rest is paid in addition.
    pub alt_mana: bool,
}

/// "You may pay {1} and return a basic land you control to its owner's hand
/// rather than pay this spell's mana cost" (CR 118.9): mana and one chosen
/// permanent to sacrifice or return (`cost.sacrifice`, `cost.chosen`),
/// `count` of them ("sacrifice two Mountains"), offered only while the
/// pool's condition `condition` holds ("If it's your turn, ...").
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct AltCost {
    pub cost: Cost,
    pub count: u32,
    pub condition: Option<u16>,
    /// "you may have each other player gain 6 life rather than pay this
    /// spell's mana cost": paid by each other player gaining this much
    /// life (a cost the caster can always pay, CR 118.3; it is paid even
    /// if they can't gain life).
    pub others_gain: u32,
}

impl Cost {
    pub fn parse(mana: &str) -> Cost {
        Cost {
            mana: ManaCost::parse(mana).unwrap_or_else(|e| panic!("{mana}: {e}")),
            discard_self: false,
            exile_self: false,
            exile_this: false,
            exile_hand: false,
            crew: 0,
            remove_counters: None,
            put_counters: None,
            energy: 0,
            chosen: ChosenCost::Sacrifice,
            life: 0,
            discard: 0,
            discard_x: false,
            discard_random: false,
            discard_filter: None,
            discard_exile: false,
            loyalty: 0,
            tap: false,
            sacrifice: None,
            count: 0,
            exile_cards: None,
            reveal_hand: false,
            reveal_self: false,
            blight: 0,
            behold: None,
            evidence: 0,
            forage: false,
            craft: None,
            waterbend: false,
            exert: false,
            discard_hand: false,
            life_x: false,
            blight_put: false,
            process: 0,
            remove_from_creature: false,
            sacrifice_any: 0,
            remove_x: false,
            untap: false,
        }
    }

    /// How many permanents the `SacCost::Filter` part chooses.
    pub fn chosen_count(&self) -> usize {
        self.count.max(1) as usize
    }

    /// "Tap an untapped creature you control": the chosen permanents are
    /// tapped, before any mana is paid, so they don't also tap for mana.
    pub fn taps_chosen(&self) -> bool {
        self.chosen == ChosenCost::Tap && matches!(self.sacrifice, Some(SacCost::Filter(_)))
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
#[allow(clippy::struct_excessive_bools)] // independent permissions and wordings
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
    /// "Spend this mana only to pay a disturb cost or cast an instant or
    /// sorcery spell." Disturb (CR 702.146) isn't implemented, so this is
    /// the wording only.
    #[serde(default)]
    pub disturb: bool,
    /// How the sentence reads; `CantCast` also turns `spells` around.
    pub wording: SpendWording,
    /// "artifact spells or activate abilities of artifacts": the plural
    /// wording, and how alternatives join. Wording only.
    #[serde(default)]
    pub plural: SpendPlural,
    /// "abilities of Elementals", "an ability of a creature": the source
    /// named without "source". Wording only.
    #[serde(default)]
    pub bare: bool,
    /// The other things it may be spent on (power-up and equip abilities,
    /// a Class level, unlocking a door, turning a permanent face up, a
    /// cost with {C} or {X}), what `spells` must have been cast as (from
    /// exile, face down, ...), and the "that spell can't be countered"
    /// rider.
    #[serde(default)]
    pub ways: SpendWays,
    /// "or spells with {X} in their mana costs": also spells matching this
    /// whose mana cost has {X} (CR 107.3).
    #[serde(default)]
    pub x_spells: Option<ObjFilter>,
}

/// What restricted mana may further be spent on, and what a spell it pays
/// for must have been cast as (`ManaRestriction::ways`); also what a
/// payment is (`SpendWays::of_ability`, the engine's spell context), so
/// that a restriction allows a payment when they share an alternative.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpendWays(pub u32);

impl SpendWays {
    /// "activate power-up abilities" (CR 702.193).
    pub const POWER_UP: u32 = 1;
    /// "activate an equip ability" (CR 702.6).
    pub const EQUIP: u32 = 1 << 1;
    /// "gain a Class level" (CR 716.2a).
    pub const CLASS_LEVEL: u32 = 1 << 2;
    /// "unlock a door" (CR 709.5e).
    pub const UNLOCK: u32 = 1 << 3;
    /// "turn a permanent face up" (CR 116.2b).
    pub const FACE_UP: u32 = 1 << 4;
    /// "pay a cost that contains {C}": any spell or ability whose mana
    /// cost has {C}.
    pub const COST_C: u32 = 1 << 5;
    /// "on costs that contain {X}": any spell or ability whose mana cost
    /// has {X}.
    pub const COST_X: u32 = 1 << 6;
    /// The spell is cast from exile ("cast spells from exile").
    pub const FROM_EXILE: u32 = 1 << 7;
    /// The spell is cast from its owner's graveyard ("from your
    /// graveyard").
    pub const FROM_GRAVEYARD: u32 = 1 << 8;
    /// The spell is cast from anywhere other than its caster's hand.
    pub const NOT_FROM_HAND: u32 = 1 << 9;
    /// The spell has flashback and is cast from a graveyard ("spells with
    /// flashback from a graveyard").
    pub const FLASHBACK: u32 = 1 << 10;
    /// The spell is face down (CR 708.4: "face-down spells").
    pub const FACE_DOWN: u32 = 1 << 11;
    /// "..., and that spell can't be countered" (Cavern of Souls): a spell
    /// this mana is spent to cast can't be countered.
    pub const UNCOUNTERABLE: u32 = 1 << 12;
    /// Wording: "turn creatures face up" rather than "permanents".
    pub const FACE_UP_CREATURES: u32 = 1 << 13;
    /// Wording: "pay a mana cost to turn a manifested creature face up, or
    /// pay a morph cost" (the same rule as `FACE_UP`).
    pub const FACE_UP_MORPH: u32 = 1 << 14;
    /// Wording: "Spend this mana only on costs that contain {X}."
    pub const ON_COSTS: u32 = 1 << 15;
    /// Wording: an alternative after the first joined with "or to" ("cast
    /// face-down spells or to turn creatures face up").
    pub const OR_TO: u32 = 1 << 16;

    /// The spell properties: each must hold of a spell for `spells` to
    /// allow it.
    pub const SPELL_QUALIFIERS: u32 = Self::FROM_EXILE
        | Self::FROM_GRAVEYARD
        | Self::NOT_FROM_HAND
        | Self::FLASHBACK
        | Self::FACE_DOWN;
    /// The alternatives an activated ability's kind gives.
    pub const ABILITY_KINDS: u32 = Self::POWER_UP | Self::EQUIP | Self::CLASS_LEVEL;
    /// The alternatives a cost's symbols give.
    pub const COSTS: u32 = Self::COST_C | Self::COST_X;

    pub const fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    #[must_use]
    pub const fn with(self, bit: u32) -> SpendWays {
        SpendWays(self.0 | bit)
    }

    /// What paying for `ab` is: its kind, and the symbols of its mana
    /// cost.
    pub fn of_ability(ab: &ActivatedAbility) -> SpendWays {
        let mut w = Self::of_cost(&ab.cost.mana);
        if ab.keyword == Some(ActivatedKeyword::PowerUp) {
            w.0 |= Self::POWER_UP;
        }
        if ab.is_equip() {
            w.0 |= Self::EQUIP;
        }
        if ab.sorcery_speed && matches!(ab.effects.first(), Some(Effect::BecomeStage(_))) {
            w.0 |= Self::CLASS_LEVEL;
        }
        w
    }

    /// `COST_C` and `COST_X` as `cost` has those symbols.
    pub fn of_cost(cost: &crate::mana::ManaCost) -> SpendWays {
        let mut w = 0;
        if cost.has_x() {
            w |= Self::COST_X;
        }
        if cost.has_colorless_symbol() {
            w |= Self::COST_C;
        }
        SpendWays(w)
    }
}

/// Whether a `ManaRestriction` reads in the plural ("Spend this mana only
/// to cast instant and sorcery spells."), and how its alternatives join.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum SpendPlural {
    /// "an artifact spell", "an ability".
    #[default]
    One,
    /// "instant or sorcery spells", "abilities".
    Or,
    /// "instant and sorcery spells".
    And,
    /// "instant and/or sorcery spells".
    AndOr,
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
    /// "Spend this mana only to cast Elf spells and activate abilities of
    /// Elf sources." (the same rule as `CastFirst`).
    CastAnd,
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
    /// Its activations are recorded each turn (`GameState::activated_once`)
    /// for a condition on how often it has been activated
    /// (`StaticCondition::ActivatedThisTurn`).
    #[serde(default)]
    pub counted: bool,
    /// Who may activate it (CR 602.1): `None`, its controller; `Any`,
    /// "Any player may activate this ability."; `Opponents`, "Only your
    /// opponents may activate this ability." The activating player pays
    /// the cost and controls the ability on the stack (CR 602.2a).
    #[serde(default)]
    pub activators: Option<Who>,
}

impl ActivatedAbility {
    /// An equip ability (CR 702.6a): it attaches its source to its target.
    pub fn is_equip(&self) -> bool {
        matches!(
            self.effects.first(),
            Some(Effect::Attach {
                what: Recipient::Source,
                to: Recipient::Target(0),
            })
        ) && self.sorcery_speed
    }
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
    /// "This ability costs {1} less to activate if it targets a creature
    /// with power 3 or less": `by` once if the first target matches (CR
    /// 601.2f comes after the targets are chosen, 601.2c).
    #[serde(default)]
    pub target: Option<ObjFilter>,
    /// "This ability costs {1} less to activate for each +1/+1 counter on
    /// the creature it targets" / "for each color of the creature it
    /// targets": `by` once for each, judged on the first target.
    #[serde(default)]
    pub per_target: Option<PerTarget>,
}

/// What an activation reduction counts on its first target
/// (`ActivationReduction::per_target`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum PerTarget {
    /// Its counters of this kind.
    Counters(CounterKind),
    /// Its colors (CR 105.2).
    Colors,
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
    /// Saddle's ability (CR 702.171a), whose cost is a crew-like tap.
    pub fn saddles(&self) -> bool {
        self.effects.contains(&Effect::BecomeSaddled)
    }

    /// CR 605.1a: no targets, not a loyalty ability, and it could add
    /// mana; it may do other things too ("Add {G}{G}. If there are no
    /// depletion counters on this land, sacrifice it.").
    pub fn is_mana_ability(&self) -> bool {
        self.targets.is_empty()
            && self.cost.loyalty == 0
            && self.effects.iter().any(Effect::adds_mana)
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
    /// Only while this pooled condition holds for the source's controller
    /// ("During turns other than yours, spells you cast cost {1} less").
    #[serde(default)]
    pub cond: Option<u16>,
    /// "Spells you cast have delve" (`generic` 0): what the matching
    /// spells have besides.
    #[serde(default)]
    pub grant: Option<Box<SpellGrant>>,
    /// "... cost {1} less to cast for each creature you control with
    /// power 4 or greater": `generic` this many times, counted for the
    /// source's controller as the cost is determined.
    #[serde(default)]
    pub per: Option<Num>,
    /// "The first creature spell you cast each turn costs {2} less": only
    /// while its caster hasn't cast a matching spell this turn.
    #[serde(default)]
    pub first: bool,
}

/// What a static ability gives the spells its controller casts (CR
/// 702.51 convoke, 702.66 delve, 702.126 improvise, 702.85 cascade,
/// 702.40 storm, 702.41 affinity): "Instant and sorcery spells you cast
/// have storm", "... from your hand have cascade", "Spells you cast have
/// affinity for artifacts".
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct SpellGrant {
    /// Convoke, improvise, delve, cascade.
    pub keywords: KeywordSet,
    pub storm: bool,
    /// Affinity for these: {1} less for each such permanent the caster
    /// controls.
    pub affinity: Option<ObjFilter>,
    /// Only spells cast from their caster's hand.
    pub from_hand: bool,
    /// "Spells you control can't be countered" (CR 701.5): an effect that
    /// counters a matching spell does nothing to it.
    #[serde(default)]
    pub uncounterable: bool,
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
    /// A spree mode of three parts (CR 702.172a): where the third part
    /// starts, as `split` for the second.
    #[serde(default)]
    pub split2: Option<(u16, u16)>,
    /// A combination of more parts (pawprint modes chosen more than once,
    /// CR 700.2d, 700.2i): where each part after the third starts.
    #[serde(default)]
    pub more: Vec<(u16, u16)>,
    /// "Choose one that hasn't been chosen [this turn] —": the mode can be
    /// chosen only once for each object's ability (CR 700.2; a new object
    /// starts fresh, CR 400.7), or once each turn.
    #[serde(default)]
    pub once: ModeOnce,
    /// A named mode's name ("• Smash the Chest — Destroy target
    /// artifact."), printed before its effect; it has no rules meaning.
    pub name: Option<String>,
}

/// How often a mode of an ability may be chosen (`Mode::once`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum ModeOnce {
    /// Any number of times.
    #[default]
    Any,
    /// "choose one that hasn't been chosen —": once for each object.
    Ever,
    /// "choose one that hasn't been chosen this turn —".
    ThisTurn,
}

/// Modifications a copy effect makes as part of copying (CR 707.9b):
/// "except it's an Illusion and has ..." (Phantasmal Image), "except it has
/// haste" (Kiki-Jiki), "except it isn't legendary", "except it's an
/// artifact in addition to its other types". They become part of the
/// copiable values.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, Default)]
pub struct CopyExcept {
    pub subtypes: Vec<String>,
    pub keywords: KeywordSet,
    pub abilities: Vec<AbilityDef>,
    /// "except it isn't legendary": the copy has no legendary supertype.
    #[serde(default)]
    pub not_legendary: bool,
    /// "except it's an artifact in addition to its other types".
    #[serde(default)]
    pub types: TypeSet,
    /// "except it's a 1/1 ...": its base power and toughness.
    #[serde(default)]
    pub pt: Option<(i32, i32)>,
    /// "a ... red Balloon creature in addition to its other colors and
    /// types": colors added to its own.
    #[serde(default)]
    pub colors: ColorSet,
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
    /// "Whenever enchanted land is tapped for mana, its controller adds
    /// an additional {G}" (CR 605.1b): a triggered mana ability. It
    /// triggers when a permanent matching `what` (`enchanted`: the one
    /// this is attached to) is tapped for mana by a mana ability, and
    /// resolves at once, right after that ability (CR 605.4a), adding
    /// `effects`' mana for the tapped permanent's controller. Payment
    /// counts it as part of what that permanent makes.
    ManaTrigger {
        what: ObjFilter,
        effects: Vec<Effect>,
    },
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
        /// "can't attack if defending player controls an untapped land":
        /// the restriction applies while the condition holds.
        negated: bool,
    },
    /// An attack or block tax (CR 508.1d, 508.1g-h, 509.1d, 509.1f-g):
    /// "Creatures can't attack you unless their controller pays {2} for
    /// each creature they control that's attacking you", "Enchanted
    /// creature can't attack or block unless its controller pays {3}".
    CombatTax(CombatTax),
    /// Protection from what matches the filter ("protection from
    /// artifacts", "from Goblins", "from everything"), judged on a source's
    /// characteristics (CR 702.16); the object also has
    /// `Keyword::ProtectionFromFilter`.
    Protection(ObjFilter),
    /// Landwalk of the lands the filter matches ("snow forestwalk": a snow
    /// Forest; "nonbasic landwalk"; CR 702.14a): the creature can't be
    /// blocked as long as the defending player controls such a land. The
    /// object also has `Keyword::LandwalkFilter`.
    Landwalk(ObjFilter),
    /// Toxic N (CR 702.164): combat damage this deals to a player also
    /// gives them N poison counters.
    Toxic(u32),
    /// Hexproof from what matches the filter ("hexproof from black",
    /// "from monocolored", "from instants"), judged on the targeting
    /// source's characteristics (CR 702.11d): spells and abilities an
    /// opponent controls from such sources can't target it. The object
    /// also has `Keyword::HexproofFromFilter`.
    HexproofFrom(ObjFilter),
    /// A static ability of its controller as a player: "You have
    /// hexproof.", "You may play an additional land on each of your
    /// turns." `condition`: "As long as ..." (a pool condition, judged
    /// for the controller, CR 611.3a).
    PlayerStatic {
        kind: PlayerStaticKind,
        condition: Option<u16>,
    },
    /// "If a triggered ability of another Elemental you control triggers,
    /// it triggers an additional time." (CR 603.2d): a triggered ability
    /// of a permanent matching the filter (judged for this source) that
    /// triggers, triggers once more for each such ability; `entering`:
    /// "If an artifact or creature entering causes a triggered ability of
    /// a permanent you control to trigger, ...", only on a permanent
    /// matching it entering (`battlefield`: worded "entering the
    /// battlefield"); with `cause` `Dies` or `Attacks`, "If a creature
    /// dying causes ...", "If a creature attacking causes ...". Counted
    /// with the player-level statics. With `suppress`, "Creatures entering
    /// don't cause abilities to trigger." (Torpor Orb): an enters trigger
    /// of a permanent matching the filter doesn't trigger at all on a
    /// permanent matching `entering` entering (CR 603.2, 603.6a), whatever
    /// would add to it.
    TriggerAgain {
        filter: ObjFilter,
        entering: Option<ObjFilter>,
        battlefield: bool,
        #[serde(default)]
        cause: TriggerCause,
        #[serde(default)]
        suppress: bool,
    },
    /// "If this card is in your opening hand, you may begin the game with
    /// it on the battlefield." (CR 103.6a; the Leylines).
    OpeningHand,
    /// "You may reveal this card from your opening hand. If you do, at
    /// the beginning of the first upkeep, ..." (the Chancellors; CR
    /// 103.6a's moment): revealed, the card creates the delayed
    /// triggered ability `AbilityId` (a registered `BeginningOfStep`
    /// trigger), which happens once, at the next such step.
    OpeningHandReveal(AbilityId),
    /// "You have no maximum hand size" (`delta` None), "Your maximum hand
    /// size is increased by one" (CR 402.2); `whose` from the controller.
    HandSize {
        whose: Who,
        delta: Option<i32>,
        /// "Your maximum hand size is eight": `delta` is the size itself.
        #[serde(default)]
        set: bool,
    },
    /// "While voting, you get an additional vote." (`optional` false) or
    /// "While voting, you may vote an additional time." (CR 701.38d: the
    /// votes happen when the controller would otherwise vote).
    ExtraVote {
        optional: bool,
    },
    /// "You may look at the top card of your library any time." (`top`),
    /// "You may look at face-down creatures you don't control any time."
    /// (`face_down`; `opponents`: "... your opponents control ...", the
    /// same in a two-player game): information only (CR 401.2, 708.5),
    /// what the controller's view shows.
    MayLook {
        top: bool,
        face_down: bool,
        opponents: bool,
    },
    /// "You may play lands from the top of your library." (`lands`), "You
    /// may cast creature spells from the top of your library." (`spells`,
    /// the spells' filter), or both: the controller may play the top card
    /// of their library as though it were in their hand, paying its costs
    /// and following the usual timing and land-play rules (CR 601.3,
    /// 305.1). With `once`, "Once each turn, you may cast ...": one spell
    /// a turn by this permanent's permission.
    PlayFromTop {
        lands: bool,
        spells: Option<ObjFilter>,
        once: bool,
    },
    /// "You may cast spells from your hand without paying their mana
    /// costs." (Omniscience; CR 601.2b, 118.9): the controller may cast
    /// spells matching `spells` from their hand without paying their mana
    /// costs, with the usual timing (`Choice::CastFree`). `from_hand`
    /// false: the text names no zone ("You may cast Dragon spells without
    /// paying their mana costs."; only the hand is offered). `max`: "with
    /// mana value less than or equal to <count>" (judged for the source).
    /// `limit`: once during each of the controller's turns, or the first
    /// spell each player casts during each of their turns (for every
    /// player).
    CastFree {
        spells: ObjFilter,
        from_hand: bool,
        max: Option<CountExpr>,
        limit: FreeCastLimit,
    },
    /// "You may cast spells from among cards exiled with this creature.",
    /// "You may play lands and cast spells from among ...", "During your
    /// turn, you may play cards exiled with this artifact." (`your_turn`):
    /// the controller may play the cards the source's abilities exiled
    /// (CR 607.2a), paying their costs with the usual timing and land-play
    /// rules (CR 601.3, 305.1). `spells` filters the spells ("Dinosaur
    /// creature spells"); `owned`, "cards you own exiled with ...".
    PlayExiled {
        lands: bool,
        spells: Option<ObjFilter>,
        your_turn: bool,
        owned: bool,
    },
    /// "Play with the top card of your library revealed." (`players`:
    /// "Players play with the top card of their libraries revealed."):
    /// every player knows that top card while it stays there (CR 401.2).
    TopRevealed {
        players: bool,
        /// "Players play with their hands revealed." (Wandering Eye):
        /// every card in a hand is known to every player instead.
        #[serde(default)]
        hands: bool,
    },
    /// "As this creature is turned face up, put four +1/+1 counters on
    /// it." (CR 708.11: applied while it is being turned face up, before
    /// "turned face up" triggers look). "As this creature enters or is
    /// turned face up" is this and an enters-with replacement.
    AsTurnedFaceUp {
        kind: CounterKind,
        n: Num,
    },
    /// "This creature crews Vehicles using its toughness rather than its
    /// power." (`toughness`), "... saddles Mounts and crews Vehicles as
    /// though its power were 2 greater." (`plus` 2): what it counts for
    /// when tapped to pay a crew cost (CR 702.122a), and with `saddles` a
    /// saddle cost too (CR 702.171a).
    CrewsAs {
        saddles: bool,
        toughness: bool,
        plus: u32,
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
            | AbilityDef::ManaTrigger { .. }
            | AbilityDef::SacForMana(_)
            | AbilityDef::RestrictedMana { .. }
            | AbilityDef::XManaColor(_)
            | AbilityDef::BlockRule(_)
            | AbilityDef::CostChange(_)
            | AbilityDef::AttackUnless { .. }
            | AbilityDef::CombatTax(_)
            | AbilityDef::Protection(_)
            | AbilityDef::Landwalk(_)
            | AbilityDef::HandSize { .. }
            | AbilityDef::ExtraVote { .. }
            | AbilityDef::MayLook { .. }
            | AbilityDef::PlayFromTop { .. }
            | AbilityDef::CastFree { .. }
            | AbilityDef::PlayExiled { .. }
            | AbilityDef::TopRevealed { .. }
            | AbilityDef::AsTurnedFaceUp { .. }
            | AbilityDef::CrewsAs { .. }
            | AbilityDef::Toxic(_)
            | AbilityDef::HexproofFrom(_)
            | AbilityDef::PlayerStatic { .. }
            | AbilityDef::OpeningHand
            | AbilityDef::OpeningHandReveal(_)
            | AbilityDef::TriggerAgain { .. }
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
            AbilityDef::Spell { effects, .. } | AbilityDef::ManaTrigger { effects, .. } => effects,
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
            | AbilityDef::CombatTax(_)
            | AbilityDef::Protection(_)
            | AbilityDef::Landwalk(_)
            | AbilityDef::HandSize { .. }
            | AbilityDef::ExtraVote { .. }
            | AbilityDef::MayLook { .. }
            | AbilityDef::PlayFromTop { .. }
            | AbilityDef::CastFree { .. }
            | AbilityDef::PlayExiled { .. }
            | AbilityDef::TopRevealed { .. }
            | AbilityDef::AsTurnedFaceUp { .. }
            | AbilityDef::CrewsAs { .. }
            | AbilityDef::Toxic(_)
            | AbilityDef::HexproofFrom(_)
            | AbilityDef::PlayerStatic { .. }
            | AbilityDef::OpeningHand
            | AbilityDef::OpeningHandReveal(_)
            | AbilityDef::TriggerAgain { .. }
            | AbilityDef::Static(_)
            | AbilityDef::Replacement(_)
            | AbilityDef::Modal(_)
            | AbilityDef::EntersAsCopy(_)
            | AbilityDef::AsEntersChoose(_) => &[],
        }
    }

    /// "You may have this permanent enter with ..." (unleash, CR 702.98a)
    /// or "enters with your choice of ...": its controller is asked as it
    /// enters.
    pub const fn enters_may(&self) -> bool {
        matches!(
            self,
            AbilityDef::Replacement(ReplacementAbility {
                replacement: Replacement::EnterWithCountersMay { .. }
                    | Replacement::EnterWithCounterChoice { .. }
                    | Replacement::EnterDevour { .. }
                    | Replacement::EnterAmplify(_)
                    | Replacement::EnterTribute(_)
                    | Replacement::EnterReadAhead
                    | Replacement::EnterTappedUnlessPay(_)
                    | Replacement::EnterTappedUnlessReveal { .. },
                ..
            })
        )
    }

    /// Whether this ability names the Aura or Equipment that grants it
    /// (`Recipient::Granter`, `SacCost::Granter`): each grant of it is
    /// then its own ability, carrying its granter.
    pub fn names_granter(&self) -> bool {
        if let AbilityDef::Activated(a) = self
            && a.cost.sacrifice == Some(SacCost::Granter)
        {
            return true;
        }
        self.all_effects().iter().any(|e| e.names_granter())
    }

    /// Every effect this ability can apply, across modes and "you may"
    /// clauses, for static inspection.
    pub fn all_effects(&self) -> Vec<&Effect> {
        fn walk<'a>(e: &'a Effect, out: &mut Vec<&'a Effect>) {
            out.push(e);
            if let Effect::YouMay(inner)
            | Effect::PlayerMay { effects: inner, .. }
            | Effect::AnyPlayerMay { effects: inner, .. }
            | Effect::IfKicked(inner)
            | Effect::IfYouDo(inner)
            | Effect::Otherwise(inner)
            | Effect::IfCondition(_, inner)
            | Effect::IfTarget { then: inner, .. }
            | Effect::IfCard { then: inner, .. }
            | Effect::IfThisWay { then: inner, .. }
            | Effect::IfExcessDamage(inner)
            | Effect::OneOf(inner)
            | Effect::Lasting { effects: inner, .. }
            | Effect::UnlessPays { effects: inner, .. }
            | Effect::UnlessPaysCost { effects: inner, .. }
            | Effect::UnlessSacrifices { effects: inner, .. }
            | Effect::SpendOnly { effects: inner, .. }
            | Effect::SpendRider { effects: inner, .. } = e
            {
                for i in inner {
                    walk(i, out);
                }
            }
            if let Effect::FlipCoin { won } | Effect::Clash { won } = e {
                for i in won {
                    walk(i, out);
                }
            }
            if let Effect::UnlessYou { act, effects } = e {
                for i in act.iter().chain(effects) {
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
    /// A battle's printed defense (CR 310.4a).
    pub defense: u32,
    /// A dungeon card's rooms (CR 309.4), topmost first; empty for any
    /// other card.
    #[serde(default)]
    pub rooms: Vec<Room>,
    /// A Room's definitions, one per unlock state (CR 709.5): set by
    /// `CardPool::add_room_doors` on the split card, its halves, and the
    /// fully locked and fully unlocked states.
    #[serde(default)]
    pub doors: Option<Box<RoomDoors>>,
    /// A fused split spell (CR 702.102b-d, `CardPool::add_fused`): its
    /// spell ability is the left half's targets and effects followed by
    /// the right half's, which start at these indices (effect, target), as
    /// a combined mode's `Mode::split`.
    #[serde(default)]
    pub fused: Option<(u16, u16)>,
    /// A Class's levels (CR 716) or a Case's unsolved and solved states
    /// (CR 719), in order, the first the card itself: each a definition
    /// with the abilities it has then. Set by `CardPool::set_stages` on
    /// every one of them; empty for other cards.
    #[serde(default)]
    pub stages: Vec<CardRef>,
    /// Ascend, start your engines!, and becoming day as it enters
    /// (`Marks`).
    #[serde(default)]
    pub marks: Marks,
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
    /// "As an additional cost to cast this spell, sacrifice a creature or
    /// pay {3}{B}" (CR 601.2b): the other of two additional costs, of which
    /// the caster pays one, chosen as the spell is cast. `additional` is
    /// the one printed first.
    #[serde(default)]
    pub additional_or: Option<Box<Cost>>,
    /// Kicker (CR 702.33): an optional additional mana cost chosen as the
    /// spell is cast.
    pub kicker: Option<Box<ManaCost>>,
    /// A kicker cost that isn't mana ("Kicker—Sacrifice two lands.", CR
    /// 702.33a): paid with the other costs when the spell is kicked, a
    /// permanent it chooses settled after targets like an additional
    /// cost's (`kicker` is then its mana part, "Kicker—{2}{R}, Sacrifice
    /// a land."). Such a card has no
    /// additional cost of its own.
    #[serde(default)]
    pub kicker_cost: Option<Box<Cost>>,
    /// The spell's targets from this index on are chosen only if it was
    /// kicked ("If this spell was kicked, return another target permanent
    /// card ...", CR 601.2c).
    #[serde(default)]
    pub kicker_targets: Option<u8>,
    /// What this spell's own text takes off its cost (affinity).
    pub self_cost: Option<SelfCost>,
    /// "This spell costs {3} less to cast if it targets a tapped
    /// creature" (negative) or "{2} more ... a Dragon": generic mana, and
    /// what the first target must match.
    pub target_cost: Option<Box<(i32, ObjFilter)>>,
    /// "This spell costs {3} less to cast if a creature died this turn":
    /// generic mana, and the pool's condition index.
    pub cond_cost: Option<(i32, u16)>,
    /// "This spell can't be countered." (CR 101.2: a spell or ability
    /// that would counter it does nothing to it).
    #[serde(default)]
    pub uncounterable: bool,
    /// "If X is 5 or more, this spell can't be countered and the damage
    /// can't be prevented." (Banefire), "Spell mastery — If there are two
    /// or more instant and/or sorcery cards in your graveyard, this spell
    /// can't be countered." (Exquisite Firecraft): the pool's condition,
    /// judged for the spell's controller when an effect would counter it
    /// (CR 101.2) and, with `unpreventable`, as it resolves (CR 615.12).
    #[serde(default)]
    pub uncounterable_if: Option<(u16, bool)>,
    /// Buyback (CR 702.27): an optional additional cost; if paid, the
    /// spell goes to its owner's hand as it resolves.
    pub buyback: Option<Box<ManaCost>>,
    /// "Buyback—Sacrifice a land." (CR 702.27a): a buyback cost that
    /// sacrifices a matching permanent, chosen as buyback is decided.
    pub buyback_sacrifice: Option<Box<ObjFilter>>,
    /// Morph, megamorph, or disguise (CR 702.37, 702.168): cast face down
    /// for {3}, turned face up for this cost ("Morph—Pay 5 life.",
    /// "Morph—Discard a card.", "Morph—Return a Bird you control to its
    /// owner's hand.": CR 702.37a allows any cost).
    pub morph: Option<Box<(Cost, MorphKind)>>,
    /// Evoke (CR 702.74): an alternative cost; an evoked permanent is
    /// sacrificed as it enters.
    pub evoke: Option<Box<ManaCost>>,
    /// Dash (CR 702.109): an alternative cost; a dashed permanent has
    /// haste and returns to its owner's hand at the next end step.
    pub dash: Option<Box<ManaCost>>,
    /// Warp (CR 702.185): cast from the hand for this cost instead; the
    /// permanent is exiled at the beginning of the next end step, and may
    /// be cast from exile on a later turn.
    #[serde(default)]
    pub warp: Option<Box<ManaCost>>,
    /// Sneak (CR 702.190): during its controller's declare blockers step,
    /// cast for this cost, returning an unblocked attacker they control to
    /// its owner's hand, instead of its mana cost; the permanent enters
    /// tapped and attacking what that creature attacked.
    #[serde(default)]
    pub sneak: Option<Box<ManaCost>>,
    /// Web-slinging (CR 702.188): cast for this cost, returning a tapped
    /// creature its caster controls to its owner's hand, instead of its
    /// mana cost.
    #[serde(default)]
    pub web_slinging: Option<Box<ManaCost>>,
    /// Impending N—[cost] (CR 702.176): cast for this cost instead of its
    /// mana cost, the permanent enters with N time counters; it isn't a
    /// creature while it has one (an ordinary static ability with the
    /// condition `Impending`).
    #[serde(default)]
    pub impending: Option<Box<(u32, ManaCost)>>,
    /// Harmonize (CR 702.180): cast from the graveyard for this cost,
    /// tapping up to one untapped creature its caster controls to reduce
    /// the cost by that creature's power in generic mana; exiled as it
    /// leaves the stack.
    #[serde(default)]
    pub harmonize: Option<Box<ManaCost>>,
    /// Bestow (CR 702.103): an alternative cost; the spell is then an Aura
    /// with enchant creature (`bestow_face`), and the permanent becomes a
    /// creature again when it isn't attached.
    #[serde(default)]
    pub bestow: Option<Box<ManaCost>>,
    /// Filled in by `CardPool::add` for a creature with bestow: the Aura it
    /// is as a bestowed spell or permanent (CR 702.103b: no creature type
    /// or creature subtypes, Aura, enchant creature).
    #[serde(skip)]
    pub bestow_face: Option<CardRef>,
    /// Conspire (CR 702.78): as it is cast, its caster may tap two
    /// untapped creatures they control that share a color with it; when
    /// they do, the spell is copied.
    #[serde(default)]
    pub conspire: bool,
    /// Casualty N (CR 702.153): as it is cast, its caster may sacrifice a
    /// creature with power N or greater; when they do, the spell is
    /// copied. 0: none.
    #[serde(default)]
    pub casualty: u8,
    /// Storm (CR 702.40): "When you cast this spell, copy it for each spell
    /// cast before it this turn."
    #[serde(default)]
    pub storm: bool,
    /// Dredge N (CR 702.52): "If you would draw a card, you may mill N
    /// cards instead. If you do, return this card from your graveyard to
    /// your hand." 0: none.
    #[serde(default)]
    pub dredge: u8,
    /// Disturb (CR 702.146): on a transforming card's front face, the card
    /// may be cast transformed from its owner's graveyard for this cost.
    #[serde(default)]
    pub disturb: Option<Box<ManaCost>>,
    /// "If this would be put into a graveyard from anywhere, exile it
    /// instead." (a disturb card's back face, CR 614.1a).
    #[serde(default)]
    pub graveyard_exile: bool,
    /// CR 607.2a: an ability of this card refers to the cards exiled with
    /// it ("cards exiled with this artifact"), so what its triggered and
    /// activated abilities exile is linked to it
    /// (`GameState::linked_exiles`).
    #[serde(default)]
    pub links_exile: bool,
    /// Multikicker (CR 702.33c): a kicker cost paid any number of times.
    #[serde(default)]
    pub multikicker: Option<Box<ManaCost>>,
    /// Replicate (CR 702.56): an additional cost paid any number of times;
    /// as the spell is cast, it is copied once for each time.
    #[serde(default)]
    pub replicate: Option<ManaCost>,
    /// Teamwork N: "As an additional cost to cast this spell, you may tap
    /// any number of creatures you control with total power N or more."
    /// 0: none.
    #[serde(default)]
    pub teamwork: u8,
    /// "As an additional cost to cast this spell, you may collect evidence
    /// 6." / "... you may blight 1." (CR 601.2b): an optional additional
    /// cost without mana, chosen as the spell is cast; "if evidence was
    /// collected" and "if this spell's additional cost was paid" read it
    /// (`StaticCondition::OptionalCostPaid`).
    #[serde(default)]
    pub optional_cost: Option<Box<Cost>>,
    /// Ripple N (CR 702.60): as it is cast, its caster may reveal the top
    /// N cards of their library and cast those with the same name free.
    /// 0: none.
    #[serde(default)]
    pub ripple: u8,
    /// "Enchanted creature ... is goaded" (CR 701.15): the permanent's
    /// static ability grants "attacks each combat if able"; this marks
    /// that what it is attached to attacks a player other than the
    /// permanent's controller if able.
    #[serde(default)]
    pub goads: bool,
    /// Overload (CR 702.96): an alternative cost; an overloaded spell's
    /// text has "each" for "target" (`overload_face`).
    #[serde(default)]
    pub overload: Option<Box<ManaCost>>,
    /// Filled in by `CardPool::add` for a spell with overload: the spell
    /// as cast overloaded, without targets (CR 702.96b). It resolves once
    /// for each object or player the card's target spec matches.
    #[serde(skip)]
    pub overload_face: Option<CardRef>,
    /// Prototype (CR 702.160a, 718): the alternative mana cost, power,
    /// and toughness. Cast prototyped (the card's alternative cost, CR
    /// 118.9), the spell and its permanent are `prototype_face`: the card
    /// with those characteristics and the cost's colors (CR 718.3b).
    #[serde(default)]
    pub prototype: Option<Box<(ManaCost, i32, i32)>>,
    #[serde(default)]
    pub prototype_face: Option<CardRef>,
    /// Blitz (CR 702.152a): the card's alternative cost (`alt_cost`) is its
    /// blitz cost; cast for it, the permanent is `AltCast::Blitz`.
    #[serde(default)]
    pub blitz: bool,
    /// Haunt (CR 702.55): "When the creature it haunts dies, ...", the
    /// ability a card exiled haunting a creature has: a delayed trigger
    /// about that creature, created as the card is exiled
    /// (`Effect::ExileHaunting`), whose source is the card in exile.
    #[serde(default)]
    pub haunted: Option<AbilityId>,
    /// Reconfigure (CR 702.151): this Equipment creature may be attached
    /// to a creature (CR 301.5c), and isn't a creature while attached.
    #[serde(default)]
    pub reconfigure: bool,
    /// Read ahead (CR 702.155): a Saga entering with lore counters
    /// triggers only the chapter it starts at.
    #[serde(default)]
    pub read_ahead: bool,
    /// Haunt on an instant or sorcery (CR 702.55a): "When this spell is
    /// put into a graveyard during its resolution, exile it haunting
    /// target creature.", triggered as the resolved spell goes there.
    #[serde(default)]
    pub haunt_spell: Option<AbilityId>,
    /// Prowl (CR 702.76a): the card's alternative cost (`alt_cost`) is its
    /// prowl cost; cast for it, the permanent is `AltCast::Prowl`.
    #[serde(default)]
    pub prowl: bool,
    /// Emerge (CR 702.119a): the card's alternative cost (`alt_cost`) is
    /// its emerge cost with a creature sacrificed, reduced by that
    /// creature's mana value.
    #[serde(default)]
    pub emerge: bool,
    /// Awaken N (CR 702.113): an alternative cost; cast for it, the spell
    /// also puts N +1/+1 counters on target land you control, which
    /// becomes a 0/0 Elemental creature with haste (`awaken_face`).
    #[serde(default)]
    pub awaken: Option<(u8, Box<ManaCost>)>,
    /// Filled in by `CardPool::add` for a spell with awaken: the spell as
    /// cast for its awaken cost, with the land target and the effects
    /// after its own (CR 702.113a).
    #[serde(skip)]
    pub awaken_face: Option<CardRef>,
    /// Cleave (CR 702.148): the spell ability with the bracketed words
    /// removed, cast for the card's alternative cost (`alt_cost`).
    #[serde(default)]
    pub cleaved: Option<Box<AbilityDef>>,
    /// Filled in by `CardPool::add` for a spell with cleave: the spell as
    /// cast for its cleave cost (CR 702.148a).
    #[serde(skip)]
    pub cleave_face: Option<CardRef>,
    /// Splice onto Arcane (CR 702.47): revealed from hand as an Arcane
    /// spell is cast, this cost paid on top, the card's effects (and
    /// targets) are added to that spell. Mana, or mana and a permanent
    /// chosen (sacrificed, returned, tapped) or cards exiled from the
    /// graveyard ("Splice onto Arcane—Sacrifice two Mountains.").
    #[serde(default)]
    pub splice: Option<Box<Cost>>,
    /// "You may sacrifice a Mountain rather than pay this spell's mana
    /// cost." (CR 118.9): an alternative cost the card itself offers.
    pub alt_cost: Option<Box<AltCost>>,
    /// "This spell has flash as long as you control a Merfolk." (CR
    /// 702.8): the pool condition under which the card may be cast as
    /// though it had flash.
    pub flash_if: Option<u16>,
    /// "You may cast this spell as though it had flash. If you cast it any
    /// time a sorcery couldn't have been cast, the controller of the
    /// permanent it becomes sacrifices it at the beginning of the next
    /// cleanup step." (Spider Climb).
    pub flash_cleanup: bool,
    /// How many of a spell's targets it has unless it was kicked: those
    /// after are declared by "If this spell was kicked, ..." and chosen
    /// only if it was (CR 601.2c, 702.33).
    pub unkicked_targets: Option<u8>,
    /// How many of a spell's first targets a kicked spell doesn't have:
    /// "If this spell was kicked, instead any number of target creatures
    /// you control gain indestructible" names its own targets, which
    /// replace the others (CR 601.2c); the kicked spell's targets are
    /// those after, numbered from 0.
    #[serde(default)]
    pub kicked_drops: Option<u8>,
    /// The pool condition an `AsCast` condition of the spell judges as it
    /// is cast.
    pub cast_snapshot: Option<u16>,
    /// "Cast this spell only during combat." / "... only if you control a
    /// creature." (CR 601.3): the pool condition the caster must meet to
    /// cast the card.
    pub cast_only: Option<u16>,
    /// Offspring (CR 702.175): an optional additional cost; if paid, a 1/1
    /// token copy is created as the permanent enters.
    pub offspring: Option<Box<ManaCost>>,
    /// Bargain (CR 702.166): "You may sacrifice an artifact, enchantment,
    /// or token as you cast this spell."
    pub bargain: bool,
    /// "This effect doesn't remove this Aura.": protection the Aura grants
    /// doesn't make it fall off (CR 702.16; the state-based check of CR
    /// 704.5m ignores protection for this Aura).
    pub aura_stays: bool,
    /// Strive (CR 207.2c): "This spell costs {2}{W} more to cast for each
    /// target beyond the first." Added as the total cost is determined
    /// (CR 601.2f), after the targets.
    pub strive: Option<Box<ManaCost>>,
    /// Madness (CR 702.35): cast for this cost as it's discarded.
    pub madness: Option<Box<ManaCost>>,
    /// Miracle (CR 702.94): cast for this cost as it's drawn, the first
    /// card its owner drew this turn.
    #[serde(default)]
    pub miracle: Option<Box<ManaCost>>,
    /// Foretell (CR 702.143): exiled from hand for {2}, cast later for
    /// this cost.
    pub foretell: Option<Box<ManaCost>>,
    /// Plot (CR 702.170): exiled from hand for this cost as a special
    /// action at sorcery speed; cast free as a sorcery on a later turn.
    #[serde(default)]
    pub plot: Option<Box<ManaCost>>,
    /// Suspend N—cost (CR 702.62).
    pub suspend: Option<Box<(u32, ManaCost)>>,
    /// Gift (CR 702.174): "As an additional cost to cast this spell, you
    /// may choose an opponent" (the game's opponent). A promised gift is
    /// kept as the spell's kicker flag (`CastInfo::kicked`), so the
    /// card's "if the gift was promised" sentences are its kicker
    /// conditions. These effects give the gift (to `EachOpponent`): an
    /// instant or sorcery applies them before its other effects as it
    /// resolves (CR 702.174j); a permanent has them as its own triggered
    /// ability ("When this permanent enters, if its gift cost was paid").
    #[serde(default)]
    pub gift: Option<Box<Vec<Effect>>>,
    /// Spree (CR 702.172): the additional cost of each of the spell's
    /// modes, in order. The modal ability's modes are then every nonempty
    /// set of them, the set with bits `b` at index `b - 1` (at most three
    /// modes: seven sets); the caster chooses one and pays its costs.
    #[serde(default)]
    pub spree: Vec<ManaCost>,
    /// Tiered (CR 702.183): the additional cost of each mode of the
    /// spell's "choose one" modal ability, in order; the caster chooses
    /// exactly one mode and pays its cost.
    #[serde(default)]
    pub tiered: Vec<ManaCost>,
    /// "Equip abilities you activate that target this creature cost {2}
    /// less to activate." (CR 601.2f by way of 602.2b): generic mana off
    /// any equip ability whose target is this permanent.
    #[serde(default)]
    pub equip_discount: u8,
    /// Entwine (CR 702.42): every mode of a "choose one" spell for this
    /// cost on top.
    pub entwine: Option<Box<ManaCost>>,
    /// Escalate (CR 702.120a) on a "choose one or more" spell: this cost
    /// for each mode chosen beyond the first. (On a "choose one or both"
    /// spell it is `entwine`.) The modes are the sets of modes as bits,
    /// mode index `bits - 1`.
    #[serde(default)]
    pub escalate: Option<Box<ManaCost>>,
    /// "Choose one. If this spell was cast using teamwork, choose both
    /// instead." (CR 700.2): a modal spell whose sets of more than one mode
    /// may be chosen only while the pooled condition holds as it is cast
    /// (judged for the spell); with `must`, only those sets then.
    #[serde(default)]
    pub modes_if: Option<ModesIf>,
    /// Escape—cost, exile N other cards from your graveyard (CR 702.138).
    pub escape: Option<Box<(ManaCost, u32)>>,
    /// Mayhem—cost: cast from the graveyard for this cost if its owner
    /// discarded it this turn.
    #[serde(default)]
    pub mayhem: Option<Box<ManaCost>>,
    /// Modular N (CR 702.43): enters with N +1/+1 counters (its dies
    /// trigger is an ordinary ability).
    pub modular: u32,
    /// Graft N (CR 702.58a): enters with N +1/+1 counters (its trigger is
    /// an ordinary ability).
    #[serde(default)]
    pub graft: u32,
    /// Vanishing N (CR 702.63a): enters with N time counters (its upkeep
    /// trigger is an ordinary ability).
    #[serde(default)]
    pub vanishing: u32,
    /// Fading N's entering fade counters (CR 702.32a).
    #[serde(default)]
    pub fading: u32,
    /// Flashback (CR 702.34): the spell may be cast from the graveyard for
    /// this cost instead of its mana cost, and is exiled as it leaves the
    /// stack.
    pub flashback: Option<Box<Cost>>,
    /// Retrace (CR 702.81): `flashback` is the card's mana cost and a land
    /// card discarded, and the spell isn't exiled as it leaves the stack.
    #[serde(default)]
    pub retrace: bool,
    /// "You may cast this card from your graveyard [as long as ...] [by
    /// paying {2}{W} rather than paying its mana cost | by discarding a
    /// card in addition to paying its other costs]" (CR 601.3): `flashback`
    /// holds the cost and `retrace` is set (the card isn't exiled); this
    /// holds the condition and the wording.
    #[serde(default)]
    pub graveyard_cast: Option<Box<GraveyardCast>>,
    /// "If this card is in a graveyard, effects from spells named Flame
    /// Burst count it as a card named Flame Burst." (Pardic Firecat): a
    /// graveyard count of cards named this, for a spell of that name.
    #[serde(default)]
    pub graveyard_alias: Option<String>,
    /// Event kinds any of this definition's triggers (keyword triggers
    /// included) can fire on; see `TriggerSpec::watch_mask`.
    watch_mask: u32,
    /// The same for the abilities a copy exception adds.
    except_watch_mask: u32,
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
    /// Filled in by `CardPool::add`: a static whose condition reads a
    /// record of the turn that changes without a zone change ("as long as
    /// you gained life this turn", "an opponent was dealt damage this
    /// turn"), so the record's first entry each turn refreshes the cache.
    /// (Records kept as objects change zones or gain counters need no
    /// flag: those changes refresh it anyway.)
    #[serde(skip)]
    pub history_static: bool,
    /// Filled in by `CardPool::add`: any way to cast it from the hand
    /// besides its mana cost that the priority listing checks one by one
    /// (evoke, an alternative cost, dash, warp, sneak, web-slinging,
    /// impending, bestow, overload, awaken, face down, suspend, foretell,
    /// plot). Most cards have none, and the listing skips them all.
    #[serde(skip)]
    pub other_casts: bool,
    /// A nonland card with no mana cost (Ancestral Vision, Lotus Bloom):
    /// it can't be cast by paying its mana cost (CR 118.6, 202.1b), only
    /// by an alternative cost or without paying.
    #[serde(default)]
    pub no_mana_cost: bool,
    /// Filled in by `CardPool::add`: what its text asks about the mana
    /// spent to cast it.
    #[serde(skip)]
    pub spent: SpentUse,
    /// Filled in by `CardPool::add`: a Saga's final chapter number (CR
    /// 714.2d), the greatest among its chapter abilities; 0 without any.
    #[serde(skip)]
    pub final_chapter: u8,
}

/// A room of a dungeon card (CR 309.4): its name (flavor, CR 309.4b), the
/// rooms its arrows lead to by index (none for the bottommost room), and
/// its room ability, "When you move your venture marker into this room,
/// <effect>" (CR 309.4c), a triggered ability the engine queues itself.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct Room {
    pub name: String,
    pub next: Vec<u8>,
    pub ability: AbilityDef,
    /// Filled in by `CardPool::add`: `ability` registered.
    #[serde(skip)]
    pub id: AbilityId,
}

/// Abilities that act on a player's designations as the game goes:
/// ascend (CR 702.131b), start your engines! (CR 702.179a), "If it's
/// neither day nor night, it becomes day as this creature enters.", and
/// daybound and nightbound (CR 702.145).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct Marks(pub u8);

impl Marks {
    pub const NONE: Marks = Marks(0);
    pub const ASCEND: Marks = Marks(1);
    pub const ENGINES: Marks = Marks(2);
    pub const BECOMES_DAY: Marks = Marks(4);
    /// Daybound (CR 702.145b): on a transforming card's front face.
    pub const DAYBOUND: Marks = Marks(8);
    /// Nightbound (CR 702.145e): on its back face.
    pub const NIGHTBOUND: Marks = Marks(16);
    /// Storied (CR 702.195a): its controller gets an enduring story.
    pub const STORIED: Marks = Marks(32);
    /// "Damage isn't removed from this creature during cleanup steps."
    /// (Ancient Adamantoise, CR 514.2): kept by the card's definition,
    /// also while it has lost its abilities.
    pub const KEEPS_DAMAGE: Marks = Marks(64);

    pub const fn has(self, m: Marks) -> bool {
        self.0 & m.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A Room permanent's definitions (CR 709.5): a permanent with a shared
/// type line has the name, mana cost, and rules text of its unlocked
/// halves only, so each set of unlocked doors is a definition of its own:
/// the left half alone, the right half alone, both (`both`: the names,
/// costs, and abilities of the two), or neither (`none`: the shared type
/// line only). `root` is the split card itself.
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct RoomDoors {
    pub root: CardRef,
    pub left: CardRef,
    pub right: CardRef,
    pub both: CardRef,
    pub none: CardRef,
}

impl RoomDoors {
    /// Which doors (left, right) a permanent with definition `def` has
    /// unlocked (CR 709.5c).
    pub fn unlocked(&self, def: CardRef) -> [bool; 2] {
        [
            def == self.left || def == self.both,
            def == self.right || def == self.both,
        ]
    }

    /// The definition for a set of unlocked doors.
    pub fn def_for(&self, unlocked: [bool; 2]) -> CardRef {
        match unlocked {
            [true, true] => self.both,
            [true, false] => self.left,
            [false, true] => self.right,
            [false, false] => self.none,
        }
    }

    /// A door's half (CR 709.5j).
    pub fn half(&self, door: u8) -> CardRef {
        if door == 0 { self.left } else { self.right }
    }
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
    /// Valiant (`TriggerSpec::TargetedByYouFirstTime`): targeting then
    /// records which permanents their controllers targeted this turn.
    const VALIANT: u32 = 262_144;
    /// A triggered mana ability (`AbilityDef::ManaTrigger`, CR 605.1b):
    /// tapping for mana and payment then look for such sources.
    const MANA_TRIGGER: u32 = 524_288;
    /// A state trigger (`TriggerSpec::State`, CR 603.8): priority then
    /// checks its condition.
    const STATE_TRIGGER: u32 = 1_048_576;
    /// A triggered ability that works from the graveyard
    /// (`TriggeredAbility::from_graveyard`): cards with one are counted
    /// while there (`GameState::graveyard_trigger_cards`).
    const GRAVEYARD_TRIGGER: u32 = 2_097_152;
    /// Ascend, start your engines!, or becoming day as it enters
    /// (`CardDef::marks`): state-based actions and entering then look.
    const MARKS: u32 = 4_194_304;
    /// Banding (CR 702.22): combat then looks for bands.
    const BANDING: u32 = 8_388_608;
    /// A Room (`CardDef::doors`): priority then offers its locked doors.
    const ROOM: u32 = 16_777_216;
    /// A Saga with chapter abilities (`CardDef::final_chapter`, CR 714):
    /// the precombat main phase and state-based actions then look.
    const SAGA: u32 = 33_554_432;
    /// A player-level static ability (`AbilityDef::PlayerStatic`): land
    /// plays, player targeting, and casting then look for such sources.
    const PLAYER_STATIC: u32 = 67_108_864;
    /// Playing from or revealing the top of a library
    /// (`AbilityDef::PlayFromTop`, `TopRevealed`): priority then looks
    /// at the top card.
    const TOP_PLAY: u32 = 134_217_728;
    /// An activated ability another player may activate ("Any player may
    /// activate this ability."): priority then looks at other players'
    /// permanents.
    const FOREIGN_ACTIVATED: u32 = 268_435_456;
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

    pub const fn has_valiant(self) -> bool {
        self.has(DefFlags::VALIANT)
    }

    pub const fn has_mana_trigger(self) -> bool {
        self.has(DefFlags::MANA_TRIGGER)
    }

    pub const fn has_state_trigger(self) -> bool {
        self.has(DefFlags::STATE_TRIGGER)
    }

    pub const fn has_graveyard_trigger(self) -> bool {
        self.has(DefFlags::GRAVEYARD_TRIGGER)
    }

    pub const fn has_marks(self) -> bool {
        self.has(DefFlags::MARKS)
    }

    pub const fn has_banding(self) -> bool {
        self.has(DefFlags::BANDING)
    }

    pub const fn has_room(self) -> bool {
        self.has(DefFlags::ROOM)
    }

    pub const fn has_saga(self) -> bool {
        self.has(DefFlags::SAGA)
    }

    pub const fn has_player_static(self) -> bool {
        self.has(DefFlags::PLAYER_STATIC)
    }

    pub const fn has_top_play(self) -> bool {
        self.has(DefFlags::TOP_PLAY)
    }

    pub const fn has_foreign_activated(self) -> bool {
        self.has(DefFlags::FOREIGN_ACTIVATED)
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
                    .any(|a| matches!(a, AbilityDef::Triggered(t) if !t.from_graveyard)),
        );
        flags.set(
            DefFlags::GRAVEYARD_TRIGGER,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::Triggered(t) if t.from_graveyard)),
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
                    Modification::CdaPt { count: CountExpr::Life, .. }
                        | Modification::ModifyPtPer { count: CountExpr::Life, .. }
                )) || matches!(s.condition, Some(
                    StaticCondition::OpponentLifeAtMost(_)
                        | StaticCondition::YourLife { .. }
                        | StaticCondition::LifeVsStarting { .. }
                        | StaticCondition::MoreLifeThanOpponent
                        | StaticCondition::AnyPlayerLifeAtMost(_)
                        | StaticCondition::OpponentPoison(_)
                )))
            }),
        );
        flags.set(
            DefFlags::GRANTS,
            // A keyword with a triggered ability ("Equipped creature has
            // prowess") grants that ability too.
            abilities.iter().any(|a| {
                matches!(a, AbilityDef::Static(s) if s.parts.iter().any(|m| match m {
                    Modification::AddAbility(_) => true,
                    Modification::AddKeywords(ks) => [
                        Keyword::Prowess,
                        Keyword::Exalted,
                        Keyword::Persist,
                        Keyword::Undying,
                        Keyword::Flanking,
                        Keyword::Exploit,
                        Keyword::Decayed,
                    ]
                    .into_iter()
                    .any(|k| ks.contains(k))
                        || ks.intersects(KeywordSet::TRIGGERED),
                    _ => false,
                }))
            }),
        );
        flags.set(
            DefFlags::STATE_TRIGGER,
            abilities.iter().any(
                |a| matches!(a, AbilityDef::Triggered(t) if matches!(t.trigger, TriggerSpec::State(_))),
            ),
        );
        flags.set(
            DefFlags::ATTACK_RULE,
            abilities.iter().any(|a| {
                matches!(
                    a,
                    AbilityDef::AttackUnless { .. } | AbilityDef::CombatTax(_)
                )
            }),
        );
        flags.set(
            DefFlags::TAP_STATIC,
            abilities.iter().any(|a| {
                matches!(a, AbilityDef::Static(s) if matches!(&s.condition,
                    Some(StaticCondition::YouControlNone(f)
                        | StaticCondition::SourceIs(f)
                        | StaticCondition::AttachedIs(f)) if f.tapped.is_some())
                    // "Other tapped creatures you control have
                    // deathtouch" (Saryth, the Viper's Fang): whom the
                    // static affects changes as permanents tap.
                    || s.affected.tapped.is_some())
            }),
        );
        flags.set(
            DefFlags::COST_CHANGE,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::CostChange(_))),
        );
        flags.set(
            DefFlags::FOREIGN_ACTIVATED,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::Activated(ab) if ab.activators.is_some())),
        );
        flags.set(
            DefFlags::TOP_PLAY,
            abilities.iter().any(|a| {
                matches!(
                    a,
                    AbilityDef::PlayFromTop { .. } | AbilityDef::TopRevealed { .. }
                )
            }),
        );
        // Cipher asks whether to encode as the spell resolves (CR 702.99a).
        flags.set(
            DefFlags::MID_CHOICE,
            keywords.contains(Keyword::Cipher)
                || abilities.iter().any(|a| {
                    matches!(
                        a,
                        AbilityDef::EntersAsCopy(_) | AbilityDef::AsEntersChoose(_)
                    ) || a.enters_may()
                        || a.all_effects().iter().any(|e| e.asks())
                }),
        );
        flags.set(
            DefFlags::MANA_ABILITY,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::Activated(ab) if ab.is_mana_ability())),
        );
        // A "whenever you tap this creature for mana" trigger counts too:
        // tapping for mana is reported (`Event::TappedForMana`) only while
        // a permanent with either is on the battlefield.
        flags.set(
            DefFlags::MANA_TRIGGER,
            abilities.iter().any(|a| {
                matches!(a, AbilityDef::ManaTrigger { .. })
                    || matches!(a, AbilityDef::Triggered(t) if t.trigger.taps_for_mana())
            }),
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
                .any(|a| matches!(a, AbilityDef::AsEntersChoose(_)) || a.enters_may()),
        );
        flags.set(
            DefFlags::VALIANT,
            abilities
                .iter()
                .any(|a| matches!(a, AbilityDef::Triggered(t) if t.trigger.is_valiant())),
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

    /// The targets the spell needs to be cast at all: without those only
    /// a kicked spell has (CR 601.2c).
    /// The targets a kicked spell chooses (CR 601.2c): all of them, but
    /// for those an "instead" replaces (`kicked_drops`).
    pub fn kicked_cast_targets(&self) -> &[TargetSpec] {
        match self.kicked_drops {
            Some(n) => &self.cast_targets[usize::from(n).min(self.cast_targets.len())..],
            None => &self.cast_targets,
        }
    }

    pub fn unkicked_cast_targets(&self) -> &[TargetSpec] {
        match self.unkicked_targets {
            Some(n) => &self.cast_targets[..usize::from(n).min(self.cast_targets.len())],
            None => &self.cast_targets,
        }
    }

    /// Whether the card does something from its owner's graveyard
    /// (flashback, unearth): such cards are counted per player
    /// (`flashback_in_graveyard`) so listing options skips the graveyard
    /// otherwise.
    pub fn graveyard_active(&self) -> bool {
        self.flashback.is_some()
            || self.keywords.contains(Keyword::Aftermath)
            || self.harmonize.is_some()
            || self.disturb.is_some()
            || self.escape.is_some()
            || self.mayhem.is_some()
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

    /// `may_watch` and `except_may_watch` for an event's `watch_bit`
    /// worked out once (a scan over the battlefield asks for each object).
    pub fn may_watch_bit(&self, bit: u32, except: bool) -> bool {
        (if except {
            self.except_watch_mask
        } else {
            self.watch_mask
        }) & bit
            != 0
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

    /// "with a morph ability" (CR 702.37): morph or megamorph (CR
    /// 702.37b), not disguise.
    pub fn has_morph(&self) -> bool {
        matches!(
            self.morph.as_deref(),
            Some((_, MorphKind::Morph | MorphKind::Megamorph))
        )
    }
}

/// All card definitions a game can use, with name lookup, plus the interned
/// subtypes and the registered abilities they refer to.
#[derive(Clone, Debug)]
pub struct CardPool {
    defs: Vec<CardDef>,
    by_name: HashMap<String, CardRef>,
    /// Slots held for cards that an earlier card names and that are not
    /// compiled yet (`reserve`).
    reserved: HashMap<String, CardRef>,
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
    /// Per ability: the definition whose `counts` its `Num::Count`s
    /// index, when it was registered while compiling a card rather than
    /// printed on it (a granted ability, "has '...'", or a delayed
    /// trigger): the granting card's, not the permanent's that has it.
    counts_owner: Vec<Option<CardRef>>,
    /// Per ability: whether it names the Aura or Equipment granting it
    /// (`AbilityDef::names_granter`).
    names_granter: Vec<bool>,
    /// Whether any ability does: until one is registered, nothing looks
    /// for granters.
    pub granter_abilities: bool,
    /// Whether some card has `CardDef::equip_discount`, so equip
    /// abilities' costs depend on their targets.
    pub equip_discounts: bool,
    /// The triggered ability prowess stands for (CR 702.108a).
    prowess: AbilityId,
    /// Exalted, persist, and undying (CR 702.83a, 702.79a, 702.93a).
    exalted: AbilityId,
    persist: AbilityId,
    undying: AbilityId,
    flanking: AbilityId,
    evoke: AbilityId,
    exploit: AbilityId,
    /// Decayed's "When this creature attacks, sacrifice it at end of
    /// combat" (CR 702.147a).
    decayed: AbilityId,
    builtins: [AbilityId; 15],
    /// A face-down permanent's or spell's characteristics (CR 708.2a): a
    /// nameless, colorless 2/2 creature with no abilities; and the same
    /// with ward {2}, for disguise (CR 702.168b).
    face_down: CardRef,
    face_down_ward: CardRef,
    /// Dungeon cards (CR 309), in the order added.
    dungeons: Vec<CardRef>,
    /// The designations' built-in abilities and definitions.
    desig: DesigBuiltins,
    /// The Assassin creature type, once a card with freerunning is
    /// compiled (CR 702.173a): combat damage to players by Assassins is
    /// then recorded.
    assassin: Option<Subtype>,
    /// The creature types of the pool's prowl cards (CR 702.76a).
    prowl_types: Vec<Subtype>,
}

/// The inherent triggered abilities of the monarch (CR 725.2), the
/// initiative (CR 726.2), speed (CR 702.179d), and The Ring emblem (CR
/// 701.54c), queued by the engine, and the definitions naming their
/// sources.
#[derive(Copy, Clone, Debug, Default)]
pub struct DesigBuiltins {
    /// "At the beginning of the monarch's end step, that player draws a
    /// card."
    pub monarch_draw: AbilityId,
    /// "Whenever a creature deals combat damage to the monarch, its
    /// controller becomes the monarch."
    pub monarch_steal: AbilityId,
    /// "At the beginning of the upkeep of the player who has the
    /// initiative, that player ventures into Undercity."
    pub initiative_upkeep: AbilityId,
    /// "Whenever one or more creatures a player controls deal combat
    /// damage to the player who has the initiative, the controller of
    /// those creatures takes the initiative."
    pub initiative_steal: AbilityId,
    /// "Whenever a player takes the initiative, that player ventures into
    /// Undercity."
    pub initiative_taken: AbilityId,
    /// "Whenever one or more opponents lose life during your turn, if your
    /// speed is less than 4, your speed increases by 1."
    pub speed_up: AbilityId,
    /// "Whenever your Ring-bearer attacks, draw a card, then discard a
    /// card."
    pub ring_loot: AbilityId,
    /// "Whenever your Ring-bearer becomes blocked by a creature, the
    /// blocking creature's controller sacrifices it at end of combat."
    pub ring_block: AbilityId,
    /// "Whenever your Ring-bearer deals combat damage to a player, each
    /// opponent loses 3 life."
    pub ring_damage: AbilityId,
    pub monarch: CardRef,
    pub initiative: CardRef,
    pub speed: CardRef,
    pub ring: CardRef,
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
            reserved: HashMap::new(),
            subtypes: Vec::new(),
            subtype_ids: HashMap::new(),
            abilities: Vec::new(),
            filters: Vec::new(),
            up_to: Vec::new(),
            many: Vec::new(),
            conditions: Vec::new(),
            asks: Vec::new(),
            enters: Vec::new(),
            counts_owner: Vec::new(),
            names_granter: Vec::new(),
            granter_abilities: false,
            equip_discounts: false,
            prowess: AbilityId(0),
            exalted: AbilityId(0),
            persist: AbilityId(0),
            undying: AbilityId(0),
            flanking: AbilityId(0),
            evoke: AbilityId(0),
            exploit: AbilityId(0),
            decayed: AbilityId(0),
            builtins: [AbilityId(0); 15],
            face_down: CardRef(0),
            face_down_ward: CardRef(0),
            dungeons: Vec::new(),
            desig: DesigBuiltins::default(),
            assassin: None,
            prowl_types: Vec::new(),
        };
        for c in Color::ALL {
            let id = pool.subtype(c.basic_land_type());
            debug_assert_eq!(id, Subtype::basic(c));
        }
        for name in NONCREATURE_SUBTYPES {
            pool.subtype(name);
        }
        debug_assert_eq!(pool.subtype("Equipment"), Subtype::EQUIPMENT);
        debug_assert_eq!(pool.subtype("Role"), Subtype::ROLE);
        debug_assert_eq!(pool.subtype("Aura"), Subtype::AURA);
        debug_assert_eq!(pool.subtype("Saga"), Subtype::SAGA);
        pool.prowess = pool.register(AbilityDef::Triggered(TriggeredAbility {
            limit: crate::card::TurnLimit::None,
            from_graveyard: false,
            trigger: TriggerSpec::CastsSpell {
                whose: Who::You,
                noncreature: true,
                instant_or_sorcery: false,
                or_copy: false,
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
            limit: crate::card::TurnLimit::None,
            from_graveyard: false,
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
            limit: crate::card::TurnLimit::None,
            from_graveyard: false,
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
            limit: crate::card::TurnLimit::None,
            from_graveyard: false,
            trigger: TriggerSpec::Enters(TriggerObj::This),
            condition: Some(Condition::Evoked),
            targets: Vec::new(),
            effects: vec![Effect::Sacrifice {
                what: Recipient::Source,
            }],
        }));
        pool.exploit = pool.register(AbilityDef::Triggered(TriggeredAbility {
            limit: crate::card::TurnLimit::None,
            from_graveyard: false,
            trigger: TriggerSpec::Enters(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: vec![Effect::Exploit],
        }));
        // CR 702.147a: "When this creature attacks, sacrifice it at end of
        // combat."
        let sac = pool.register(AbilityDef::Triggered(TriggeredAbility {
            limit: crate::card::TurnLimit::None,
            from_graveyard: false,
            trigger: TriggerSpec::BeginningOfStep {
                step: crate::turn::Step::EndOfCombat,
                whose: Who::Any,
            },
            condition: None,
            targets: Vec::new(),
            effects: vec![Effect::Sacrifice {
                what: Recipient::SameObject,
            }],
        }));
        pool.decayed = pool.register(AbilityDef::Triggered(TriggeredAbility {
            limit: crate::card::TurnLimit::None,
            from_graveyard: false,
            trigger: TriggerSpec::Attacks(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: vec![Effect::Delayed {
                ability: sac,
                about: Recipient::Source,
                this_turn: false,
            }],
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
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger,
                condition: None,
                targets: Vec::new(),
                effects: vec![effect],
            }));
        }
        pool.builtins[Builtin::DashReturn as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
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
        pool.builtins[Builtin::Cipher as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::Enters(TriggerObj::This),
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::CastCipherCopy],
            }));
        pool.builtins[Builtin::WarpExile as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::End,
                    whose: Who::Any,
                },
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::WarpExile],
            }));
        pool.builtins[Builtin::Ripple as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::Reflexive,
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::Ripple],
            }));
        pool.builtins[Builtin::Conspire as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::Enters(TriggerObj::This),
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::CopySpell],
            }));
        pool.builtins[Builtin::SiegeDefeated as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::Reflexive,
                condition: None,
                targets: Vec::new(),
                effects: vec![
                    Effect::Exile {
                        what: Recipient::Source,
                    },
                    Effect::CastExiled(ExileCast::Transformed),
                ],
            }));
        pool.builtins[Builtin::Paradigm as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::PrecombatMain,
                    whose: Who::You,
                },
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::CastParadigmCopy],
            }));
        pool.builtins[Builtin::EarthbendReturn as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::DiesOrExiled(TriggerObj::This),
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::ReturnToBattlefield {
                    what: Recipient::TriggeringCard,
                    counters: None,
                    tapped: true,
                    under_you: true,
                    attached: None,
                    more: None,
                    if_creature: false,
                    transformed: false,
                }],
            }));
        pool.builtins[Builtin::Miracle as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::Reflexive,
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::CastExiled(ExileCast::Miracle)],
            }));
        pool.builtins[Builtin::BlitzSacrifice as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::End,
                    whose: Who::Any,
                },
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::Sacrifice {
                    what: Recipient::SameObject,
                }],
            }));
        pool.builtins[Builtin::CleanupSacrifice as usize] =
            pool.register(AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::BeginningOfStep {
                    step: crate::turn::Step::Cleanup,
                    whose: Who::Any,
                },
                condition: None,
                targets: Vec::new(),
                effects: vec![Effect::Sacrifice {
                    what: Recipient::SameObject,
                }],
            }));
        pool.add_designations();
        // Named only for logs: a face-down permanent has no name.
        pool.face_down = pool.add(
            Builder::new("face-down creature", "")
                .types([CardType::Creature])
                .pt(2, 2)
                .build(),
        );
        let ward = AbilityDef::Triggered(TriggeredAbility {
            limit: crate::card::TurnLimit::None,
            from_graveyard: false,
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
                limit: crate::card::TurnLimit::None,
                from_graveyard: false,
                trigger: TriggerSpec::Dies(TriggerObj::This),
                condition: Some(Condition::HadNoCounters(kind)),
                targets: Vec::new(),
                effects: vec![Effect::ReturnToBattlefield {
                    what: Recipient::Source,
                    counters: Some((kind, 1)),
                    tapped: false,
                    under_you: false,
                    attached: None,
                    more: None,
                    if_creature: false,
                    transformed: false,
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
                // What a permanent becomes a copy of may be legendary too
                // (Mirage Mirror copying a legend its controller has).
                Effect::ReturnToBattlefield { .. }
                | Effect::CreateTokenCopy { .. }
                | Effect::BecomeCopy { .. } => true,
                // A legendary token can meet its namesake (CR 704.5j).
                Effect::CreateToken { token, .. } => self
                    .defs
                    .get(token.0 as usize)
                    .is_none_or(|d| d.modifies_entering() || d.is_legendary()),
                _ => false,
            }
        }));
        let granter = a.names_granter();
        self.granter_abilities |= granter;
        self.names_granter.push(granter);
        self.abilities.push(a);
        self.counts_owner.push(None);
        id
    }

    /// Whether ability `id` names the Aura or Equipment granting it
    /// (`AbilityDef::names_granter`): each grant is its own instance.
    pub fn ability_names_granter(&self, id: AbilityId) -> bool {
        self.granter_abilities && self.names_granter[id.0 as usize]
    }

    /// The number of abilities registered so far, for
    /// `set_counts_owner`.
    pub fn ability_count(&self) -> u32 {
        u32::try_from(self.abilities.len()).expect("too many abilities")
    }

    /// Record that the abilities registered in `from..to` (while
    /// compiling card `r`) take their counts from `r`'s definition.
    pub fn set_counts_owner(&mut self, from: u32, to: u32, r: CardRef) {
        for o in &mut self.counts_owner[from as usize..to as usize] {
            *o = Some(r);
        }
    }

    /// The definition whose counts ability `id`'s `Num::Count`s index,
    /// when not its source's (see `counts_owner`).
    pub fn counts_owner(&self, id: AbilityId) -> Option<CardRef> {
        self.counts_owner.get(id.0 as usize).copied().flatten()
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
                || t.condition == Some(Condition::Increments)
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
        let mut converge = false;
        let mut want = |c: &StaticCondition| {
            if *c == StaticCondition::NoManaSpent {
                out.record = true;
            }
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
            // Adamant: "If at least three white mana was spent to cast this
            // spell, this creature enters with a +1/+1 counter on it."
            if let AbilityDef::Replacement(ReplacementAbility {
                replacement: Replacement::EnterWithCountersIf { cond, .. },
                ..
            }) = a
            {
                want(self.condition(*cond));
            }
            for e in a.all_effects() {
                match e {
                    Effect::IfCondition(i, _)
                    | Effect::Instead {
                        cond: EffectCond::Static(i),
                        ..
                    } => want(self.condition(*i)),
                    // Converge in a target check: "if its mana value is
                    // less than or equal to the number of colors ...".
                    Effect::IfTarget { filter, .. } if filter.status.spent_colors => {
                        converge = true;
                    }
                    _ => {}
                }
            }
            // Converge in a target: "target nonland permanent ... with mana
            // value less than or equal to the number of colors ...".
            if a.targets().iter().any(|t| {
                matches!(self.base_spec(*t), TargetSpec::Filtered(f)
                    if self.filter(f).status.spent_colors)
            }) {
                converge = true;
            }
        }
        if converge {
            out.record = true;
            out.converge = true;
        }
        for c in &def.counts {
            // "two +1/+1 counters on it for each color of mana spent".
            let c = match c {
                CountExpr::Times(_, inner) => &**inner,
                c => c,
            };
            match c {
                CountExpr::SpentColors => {
                    out.record = true;
                    out.converge = true;
                }
                CountExpr::SpentTreasure => {
                    out.record = true;
                    out.treasure = u8::MAX;
                }
                CountExpr::SpentTotal => out.record = true,
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

    /// The designations' built-in abilities and source definitions.
    pub const fn desig(&self) -> &DesigBuiltins {
        &self.desig
    }

    /// Dungeon cards (CR 309), in the order added.
    pub fn dungeons(&self) -> &[CardRef] {
        &self.dungeons
    }

    /// Register the designations' inherent abilities (`DesigBuiltins`).
    fn add_designations(&mut self) {
        let trig = |effects: Vec<Effect>| {
            AbilityDef::Triggered(TriggeredAbility {
                limit: crate::card::TurnLimit::None,
                trigger: TriggerSpec::Reflexive,
                condition: None,
                targets: Vec::new(),
                effects,
                from_graveyard: false,
            })
        };
        let one = Num::N(1);
        let mut d = DesigBuiltins {
            monarch_draw: self.register(trig(vec![Effect::DrawCards {
                count: one,
                who: Recipient::You,
            }])),
            monarch_steal: self.register(trig(vec![Effect::BecomeMonarch {
                who: Recipient::TriggeringController,
            }])),
            initiative_upkeep: self.register(trig(vec![Effect::Venture { undercity: true }])),
            initiative_steal: self.register(trig(vec![Effect::TakeInitiative {
                who: Recipient::TriggeringPlayer,
            }])),
            initiative_taken: self.register(trig(vec![Effect::Venture { undercity: true }])),
            speed_up: self.register(trig(vec![Effect::IncreaseSpeed])),
            ring_loot: self.register(trig(vec![
                Effect::DrawCards {
                    count: one,
                    who: Recipient::You,
                },
                Effect::Discard {
                    count: one,
                    who: Recipient::You,
                },
            ])),
            ring_block: AbilityId(0),
            ring_damage: self.register(trig(vec![Effect::LoseLife {
                amount: Num::N(3),
                who: Recipient::EachOpponent,
            }])),
            ..DesigBuiltins::default()
        };
        // CR 603.7: "... sacrifices it at end of combat", a delayed
        // trigger about the blocking creature.
        let sac = self.register(AbilityDef::Triggered(TriggeredAbility {
            limit: crate::card::TurnLimit::None,
            trigger: TriggerSpec::BeginningOfStep {
                step: crate::turn::Step::EndOfCombat,
                whose: Who::Any,
            },
            condition: None,
            targets: Vec::new(),
            effects: vec![Effect::Sacrifice {
                what: Recipient::SameObject,
            }],
            from_graveyard: false,
        }));
        d.ring_block = self.register(trig(vec![Effect::Delayed {
            ability: sac,
            about: Recipient::TriggeringObject,
            this_turn: false,
        }]));
        d.monarch = self.add(Builder::new("The Monarch", "").emblem().build());
        d.initiative = self.add(Builder::new("The Initiative", "").emblem().build());
        d.speed = self.add(Builder::new("Speed", "").emblem().build());
        d.ring = self.add(Builder::new("The Ring", "").emblem().build());
        self.desig = d;
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
            Keyword::Decayed => Some(self.decayed),
            _ => None,
        }
    }

    #[allow(clippy::too_many_lines)] // one flag per kind of definition
    pub fn add(&mut self, mut def: CardDef) -> CardRef {
        // Taken before any face of this card is added: the slot is the
        // card's own.
        let slot = self.reserved.remove(&def.name);
        self.equip_discounts |= def.equip_discount > 0;
        // CR 702.103b: the Aura a bestowed spell is, registered first so
        // the card's own definition keeps its name.
        if def.bestow.is_some() && def.types.contains(CardType::Creature) {
            let face = bestowed_aura(&def);
            def.bestow_face = Some(self.add(face));
        }
        // CR 718.3b: a prototyped spell and its permanent have only the
        // alternative mana cost, power, and toughness, and that cost's
        // colors. Casting it so is the card's alternative cost (CR 118.9),
        // whose spell is this face.
        if let Some(p) = def.prototype.clone() {
            let (cost, power, toughness) = *p;
            let mut face = def.clone();
            face.mana_cost = cost.clone();
            face.power = Some(power);
            face.toughness = Some(toughness);
            face.prototype = None;
            face.alt_cost = None;
            face.printed = None;
            def.prototype_face = Some(self.add(face));
            def.alt_cost = Some(Box::new(AltCost {
                cost: Cost {
                    mana: cost,
                    ..Cost::default()
                },
                count: 1,
                condition: None,
                others_gain: 0,
            }));
        }
        // CR 702.148a: cast for its cleave cost, the spell is its text with
        // the bracketed words removed.
        if let Some(cleaved) = def.cleaved.take() {
            let mut face = def.clone();
            face.alt_cost = None;
            for a in &mut face.abilities {
                if matches!(a, AbilityDef::Spell { .. }) {
                    *a = (*cleaved).clone();
                }
            }
            def.cleave_face = Some(self.add(face));
            def.cleaved = Some(cleaved);
        }
        // CR 702.96b: the overloaded spell has no targets (and so is
        // not registered again).
        if def.overload.is_some() && def.spell_ability().is_some_and(|(t, _)| !t.is_empty()) {
            let mut face = def.clone();
            for a in &mut face.abilities {
                if let AbilityDef::Spell { targets, .. } = a {
                    targets.clear();
                }
            }
            face.overload_face = None;
            def.overload_face = Some(self.add(face));
        }
        // CR 702.113a: "If you cast this spell for [cost], also put N +1/+1
        // counters on target land you control and it becomes a 0/0
        // Elemental creature with haste. It's still a land."
        if let Some((n, _)) = &def.awaken {
            let mut face = def.clone();
            // Its cost is read from the card (`GameObject::card`).
            face.awaken = None;
            let mut lands = ObjFilter::default().who(Who::You);
            lands.types = TypeSet::of([CardType::Land]);
            let spec = TargetSpec::Filtered(self.intern_filter(lands));
            let elemental = self.subtype("Elemental");
            for a in &mut face.abilities {
                if let AbilityDef::Spell {
                    targets, effects, ..
                } = a
                {
                    let i = targets.len();
                    targets.push(spec);
                    effects.push(Effect::AddCounters {
                        kind: CounterKind::PlusOnePlusOne,
                        count: Num::N(u32::from(*n)),
                        to: Recipient::Target(i),
                    });
                    effects.push(Effect::Animate {
                        to: Recipient::Target(i),
                        types: TypeSet::of([CardType::Creature]),
                        subtypes: SubtypeSet::from_slice(&[elemental]),
                        pt: Some((0, 0)),
                        colors: None,
                        keywords: KeywordSet::NONE.with(Keyword::Haste),
                        indefinite: true,
                        loses_abilities: false,
                        in_addition: false,
                    });
                    break;
                }
            }
            def.awaken_face = Some(self.add(face));
        }
        let r = slot.unwrap_or(CardRef(
            u32::try_from(self.defs.len()).expect("card pool too large"),
        ));
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
        for i in 0..def.rooms.len() {
            def.rooms[i].id = self.register(def.rooms[i].ability.clone());
        }
        if !def.rooms.is_empty() {
            self.dungeons.push(r);
        }
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
            abilities.iter().fold(0u32, |m, a| match a {
                AbilityDef::Triggered(t) => m | t.trigger.watch_mask(),
                _ => m,
            })
        };
        def.watch_mask = mask(&def.abilities)
            | def.keywords.iter().fold(0u32, |m, k| {
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
                    | StaticCondition::SourceAttackedBattle
                    | StaticCondition::SourceAttackingAlone
                    | StaticCondition::SourceAttackedOrBlocked
                    | StaticCondition::SourceBlocked
                    | StaticCondition::SourceBeingBlocked
            )))
        });
        def.hand_static = def.abilities.iter().any(|a| {
            matches!(a, AbilityDef::Static(sa) if sa.parts.iter().any(|m| matches!(
                m,
                // Through a multiple ("twice the number of cards in your
                // hand", Masumaro, First to Live).
                Modification::ModifyPtPer { count, .. } | Modification::CdaPt { count, .. }
                    if count.reads_hand()
            )) || matches!(sa.condition, Some(
                StaticCondition::CardsInHand { .. }
                    | StaticCondition::MoreCardsInHand
            )) || matches!(&sa.condition, Some(StaticCondition::Happened { count, .. })
                if count.kind.with_hand()))
        });
        def.step_static = def.abilities.iter().any(|a| {
            matches!(a, AbilityDef::Static(sa) if matches!(sa.condition, Some(StaticCondition::During(_))))
        });
        def.spent = self.spent_use(&def);
        def.cast_targets = match def.spell_ability() {
            // Targets only a kicked spell has aren't needed to cast it.
            Some((t, _)) => t[..def
                .kicker_targets
                .map_or(t.len(), |n| t.len().min(n.into()))]
                .to_vec(),
            None => def.enchant().into_iter().collect(),
        };
        let mut flags = DefFlags::of(&def.abilities, def.keywords);
        flags.set(
            DefFlags::EQUIPMENT,
            def.subtypes.iter().any(|s| s == "Equipment"),
        );
        // "Enchant tapped creature" (Glimmerdust Nap): whether the Aura
        // is still legally attached changes as its permanent taps or
        // untaps (CR 704.5m), so a tap must mark the state-based check.
        flags.set(
            DefFlags::TAP_STATIC,
            match def.enchant().map(|t| self.base_spec(t)) {
                Some(TargetSpec::TappedCreature) => true,
                Some(TargetSpec::Filtered(f)) => self.filter(f).tapped.is_some(),
                _ => false,
            },
        );
        def.history_static = def.abilities.iter().any(|a| {
            matches!(a, AbilityDef::Static(sa)
                if sa.condition.as_ref().is_some_and(StaticCondition::refreshes_with_turn_log)
                || sa.parts.iter().any(|m| matches!(
                m,
                Modification::ModifyPtPer { count, .. } | Modification::CdaPt { count, .. }
                    if count.reads_history()
            )))
        });
        flags.set(
            DefFlags::TURN_STATIC,
            def.hand_static || def.step_static || def.history_static,
        );
        // "the amount of mana spent to cast that spell".
        let reads_that = def.counts.iter().any(|c| {
            let c = match c {
                CountExpr::Times(_, inner) => &**inner,
                c => c,
            };
            matches!(
                c,
                CountExpr::ThatSpellSpent | CountExpr::ThatSpellSpentColors
            )
        });
        flags.set(
            DefFlags::SPENT_WATCH,
            reads_that || self.watches_spent(&def.abilities),
        );
        // "Enchanted creature ... can only attack alone" (Errantry): a
        // granted attack restriction counts its source in
        // `GameState::attack_rule_sources` too.
        if def.abilities.iter().any(|a| {
            matches!(a, AbilityDef::Static(s) if s.parts.iter().any(|m| matches!(m,
                Modification::AddAbility(id) if matches!(self.ability(*id), AbilityDef::AttackUnless { .. }))))
        }) {
            flags.set(DefFlags::ATTACK_RULE, true);
        }
        // "All Sliver creatures have absorb 1": a granted replacement or
        // prevention ability counts its granter in
        // `GameState::replacement_sources`, so replacements are looked for.
        if def.abilities.iter().any(|a| {
            matches!(a, AbilityDef::Static(s) if s.parts.iter().any(|m| matches!(m,
                Modification::AddAbility(id) if matches!(self.ability(*id), AbilityDef::Replacement(r) if !matches!(r.replaces, Replaces::Enters)))))
        }) {
            flags.set(DefFlags::REPLACEMENTS, true);
        }
        flags.set(DefFlags::MARKS, !def.marks.is_empty());
        flags.set(DefFlags::BANDING, def.keywords.contains(Keyword::Banding));
        // CR 714.2d: the final chapter number is the greatest among its
        // chapter abilities.
        def.final_chapter = def
            .abilities
            .iter()
            .filter_map(|a| match a {
                AbilityDef::Triggered(TriggeredAbility {
                    trigger: TriggerSpec::Chapter(mask),
                    ..
                }) if *mask != 0 => u8::try_from(15 - mask.leading_zeros()).ok(),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        flags.set(DefFlags::SAGA, def.final_chapter > 0);
        flags.set(
            DefFlags::PLAYER_STATIC,
            def.abilities.iter().any(|a| {
                matches!(
                    a,
                    AbilityDef::PlayerStatic { .. }
                        | AbilityDef::TriggerAgain { .. }
                        | AbilityDef::CastFree { .. }
                )
            }),
        );
        def.flags = flags;
        def.other_casts = def.evoke.is_some()
            || def.alt_cost.is_some()
            || def.dash.is_some()
            || def.warp.is_some()
            || def.sneak.is_some()
            || def.web_slinging.is_some()
            || def.impending.is_some()
            || def.bestow_face.is_some()
            || def.overload_face.is_some()
            || def.awaken_face.is_some()
            || def.morph.is_some()
            || def.suspend.is_some()
            || def.foretell.is_some()
            || def.plot.is_some();
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
        if slot.is_some() {
            self.defs[r.0 as usize] = def;
        } else {
            self.defs.push(def);
        }
        r
    }

    /// The definition a card not yet compiled will have, for a card that
    /// names it ("if you control a creature named Tower Worker" on Power
    /// Plant Worker, compiled first): a slot `add` fills when that card
    /// arrives. Without it the name stood for no card, and the condition
    /// was never true. Nothing while the pool is empty.
    pub fn reserve(&mut self, name: &str) -> Option<CardRef> {
        if let Some(r) = self.reserved.get(name) {
            return Some(*r);
        }
        let mut hold = self.defs.first()?.clone();
        hold.name = name.to_string();
        let r = CardRef(u32::try_from(self.defs.len()).expect("card pool too large"));
        self.defs.push(hold);
        self.reserved.insert(name.to_string(), r);
        Some(r)
    }

    /// A name reserved and never defined, if any: an error in the caller.
    pub fn unfilled_reservation(&self) -> Option<&str> {
        self.reserved.keys().next().map(String::as_str)
    }

    /// Link a Class's levels or a Case's states (`CardDef::stages`), the
    /// card itself first.
    pub fn set_stages(&mut self, stages: &[CardRef]) {
        for r in stages {
            self.defs[r.0 as usize].stages = stages.to_vec();
        }
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

    /// CR 702.102: the fused split spell of a split card whose halves
    /// `left` and `right` both have fuse: the halves' combined
    /// characteristics (CR 702.102b, c), and one spell ability that does
    /// the left half's instructions and then the right half's (702.102d),
    /// each half's targets its own (`CardDef::fused`). Nothing when a half
    /// isn't a plain spell, or both read counts of their own.
    pub fn add_fused(&mut self, name: &str, left: CardRef, right: CardRef) -> Option<CardRef> {
        let (l, r) = (self.get(left), self.get(right));
        if !l.counts.is_empty() && !r.counts.is_empty() {
            return None;
        }
        let spell = |d: &CardDef| match d.abilities.as_slice() {
            [AbilityDef::Spell { targets, effects }] => Some((targets.clone(), effects.clone())),
            _ => None,
        };
        let ((mut targets, mut effects), (rt, re)) = (spell(l)?, spell(r)?);
        let split = (
            u16::try_from(effects.len()).ok()?,
            u16::try_from(targets.len()).ok()?,
        );
        targets.extend(rt);
        effects.extend(re);
        let mut fused = l.clone();
        fused.name = name.to_string();
        fused.mana_cost =
            ManaCost::parse(&format!("{}{}", l.mana_cost, r.mana_cost)).unwrap_or_default();
        fused.types = l.types.union(r.types);
        fused.keywords = l
            .keywords
            .union(r.keywords)
            .difference(KeywordSet::NONE.with(Keyword::Fuse));
        fused.abilities = vec![AbilityDef::Spell { targets, effects }];
        fused.oracle_text = format!("{}\n{}", l.oracle_text, r.oracle_text);
        if fused.counts.is_empty() {
            fused.counts.clone_from(&r.counts);
        }
        fused.fused = Some(split);
        fused.faces = Vec::new();
        let keep = self.by_name.clone();
        let from = self.ability_count();
        let f = self.add(fused);
        let to = self.ability_count();
        self.set_counts_owner(from, to, f);
        self.by_name = keep;
        Some(f)
    }

    /// A split card whose halves are permanents (a Room, CR 709.5): adds
    /// the definitions of its fully locked and fully unlocked states and
    /// links all five (`CardDef::doors`). Each half's "When you unlock
    /// this door" learns which door it is. Nothing for other cards.
    pub fn add_room_doors(&mut self, root: CardRef) {
        let d = self.get(root);
        let (&[left, right], Layout::Split) = (d.faces.as_slice(), d.layout) else {
            return;
        };
        if !self.get(left).types.is_permanent_type() {
            return;
        }
        let root_name = d.name.clone();
        for (half, door) in [(left, 0u8), (right, 1u8)] {
            for id in self
                .get(half)
                .ability_ids
                .iter()
                .copied()
                .collect::<Vec<_>>()
            {
                if let AbilityDef::Triggered(t) = &mut self.abilities[id.0 as usize]
                    && let TriggerSpec::UnlocksDoor { door: d } = &mut t.trigger
                {
                    *d = door;
                }
            }
        }
        let (l, r) = (self.get(left).clone(), self.get(right).clone());
        // Both unlocked: both names, costs, and rules texts (CR 709.5).
        let mut both = l.clone();
        both.name.clone_from(&root_name);
        both.mana_cost =
            ManaCost::parse(&format!("{}{}", l.mana_cost, r.mana_cost)).unwrap_or_default();
        both.abilities.extend(r.abilities.iter().cloned());
        both.keywords = l.keywords.union(r.keywords);
        both.oracle_text = format!("{}\n{}", l.oracle_text, r.oracle_text);
        // Neither: the shared type line only.
        let mut none = l.clone();
        none.name = String::new();
        none.mana_cost = ManaCost::default();
        none.abilities.clear();
        none.keywords = KeywordSet::NONE;
        none.oracle_text = String::new();
        let keep = self.by_name.clone();
        let both = self.add(both);
        let none = self.add(none);
        self.by_name = keep;
        // The halves' abilities themselves, so a door's triggers are the
        // ones its half registered.
        let ids: AbilitySet = l
            .ability_ids
            .iter()
            .chain(r.ability_ids.iter())
            .copied()
            .collect();
        let b = &mut self.defs[both.0 as usize];
        b.ability_ids = ids;
        if let Some(p) = &mut b.printed {
            p.abilities = ids;
        }
        let doors = RoomDoors {
            root,
            left,
            right,
            both,
            none,
        };
        for x in [root, left, right, both, none] {
            let d = &mut self.defs[x.0 as usize];
            d.doors = Some(Box::new(doors));
            if x != root {
                d.flags.set(DefFlags::ROOM, true);
            }
        }
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

    /// A card with freerunning is in the pool (CR 702.173a): record
    /// combat damage that Assassins deal to players.
    pub fn note_freerunning(&mut self) {
        if self.assassin.is_none() {
            self.assassin = Some(self.subtype("Assassin"));
        }
    }

    /// A card with prowl is in the pool (CR 702.76a): record combat damage
    /// to players by creatures of these types.
    pub fn note_prowl(&mut self, types: &[Subtype]) {
        for t in types {
            if !self.prowl_types.contains(t) {
                self.prowl_types.push(*t);
            }
        }
    }

    /// The creature types prowl cards in the pool watch.
    pub fn prowl_types(&self) -> &[Subtype] {
        &self.prowl_types
    }

    /// The Assassin type, if a card with freerunning is in the pool.
    pub fn assassin(&self) -> Option<Subtype> {
        self.assassin
    }

    pub fn subtype_lookup(&self, name: &str) -> Option<Subtype> {
        self.subtype_ids.get(name).copied()
    }

    /// How many subtypes are interned (`Subtype(0)` up to this).
    pub fn subtype_count(&self) -> usize {
        self.subtypes.len()
    }

    pub fn subtype_name(&self, s: Subtype) -> &str {
        if s == Subtype::ALL_CREATURE_TYPES {
            return "(every creature type)";
        }
        self.subtypes.get(s.0 as usize).map_or("?", String::as_str)
    }

    /// Every registered ability's id.
    pub fn ability_ids(&self) -> impl Iterator<Item = AbilityId> + '_ {
        (0..self.abilities.len()).map(|i| AbilityId(u32::try_from(i).expect("few abilities")))
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

/// CR 702.103b: what a creature card with bestow is as a bestowed Aura
/// spell or permanent: an Aura enchantment with enchant creature, no
/// longer a creature, without its creature types (these are copiable
/// values); everything else (name, mana cost, abilities) as printed.
fn bestowed_aura(def: &CardDef) -> CardDef {
    let mut face = def.clone();
    face.types = def
        .types
        .without(CardType::Creature)
        .with(CardType::Enchantment);
    face.subtypes = vec!["Aura".to_string()];
    face.power = None;
    face.toughness = None;
    face.abilities
        .insert(0, AbilityDef::Enchant(TargetSpec::Creature));
    face.bestow_face = None;
    face
}

/// Builder for M1 card definitions.
pub struct Builder(CardDef);

impl Builder {
    #[allow(clippy::too_many_lines)] // one line per field
    pub fn new(name: &str, cost: &str) -> Self {
        Builder(CardDef {
            hand_static: false,
            spent: SpentUse::default(),
            step_static: false,
            history_static: false,
            other_casts: false,
            no_mana_cost: false,
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
            defense: 0,
            rooms: Vec::new(),
            doors: None,
            fused: None,
            stages: Vec::new(),
            marks: Marks::NONE,
            copy_except: None,
            subtype_ids: SubtypeSet::new(),
            ability_ids: AbilitySet::new(),
            except_subtype_ids: SubtypeSet::new(),
            except_ability_ids: AbilitySet::new(),
            except_flags: DefFlags::default(),
            additional: Cost::default(),
            kicker: None,
            kicker_cost: None,
            kicker_targets: None,
            self_cost: None,
            buyback: None,
            buyback_sacrifice: None,
            morph: None,
            evoke: None,
            dash: None,
            warp: None,
            sneak: None,
            web_slinging: None,
            impending: None,
            harmonize: None,
            bestow: None,
            bestow_face: None,
            overload: None,
            overload_face: None,
            prototype: None,
            prototype_face: None,
            blitz: false,
            haunted: None,
            haunt_spell: None,
            reconfigure: false,
            read_ahead: false,
            cleaved: None,
            cleave_face: None,
            prowl: false,
            emerge: false,
            miracle: None,
            awaken: None,
            awaken_face: None,
            goads: false,
            conspire: false,
            casualty: 0,
            storm: false,
            dredge: 0,
            replicate: None,
            multikicker: None,
            disturb: None,
            graveyard_exile: false,
            links_exile: false,
            teamwork: 0,
            optional_cost: None,
            ripple: 0,
            splice: None,
            alt_cost: None,
            additional_or: None,
            flash_if: None,
            flash_cleanup: false,
            unkicked_targets: None,
            kicked_drops: None,
            cast_snapshot: None,
            cast_only: None,
            offspring: None,
            bargain: false,
            aura_stays: false,
            strive: None,
            target_cost: None,
            uncounterable: false,
            uncounterable_if: None,
            cond_cost: None,
            madness: None,
            foretell: None,
            plot: None,
            suspend: None,
            spree: Vec::new(),
            tiered: Vec::new(),
            equip_discount: 0,
            gift: None,
            entwine: None,
            escalate: None,
            modes_if: None,
            escape: None,
            mayhem: None,
            modular: 0,
            graft: 0,
            vanishing: 0,
            fading: 0,
            flashback: None,
            retrace: false,
            graveyard_cast: None,
            graveyard_alias: None,
            watch_mask: 0,
            except_watch_mask: 0,
            printed: None,
            cast_targets: Vec::new(),
            flags: DefFlags::default(),
            final_chapter: 0,
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

    /// A dungeon card's next room (CR 309.4).
    #[must_use]
    pub fn room(mut self, room: Room) -> Self {
        self.0.rooms.push(room);
        self
    }

    #[must_use]
    pub fn marks(mut self, m: Marks) -> Self {
        self.0.marks = Marks(self.0.marks.0 | m.0);
        self
    }

    #[must_use]
    pub fn loyalty(mut self, n: u32) -> Self {
        self.0.loyalty = n;
        self
    }

    #[must_use]
    pub fn defense(mut self, n: u32) -> Self {
        self.0.defense = n;
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

    /// The other of two additional costs ("... or pay {3}").
    #[must_use]
    pub fn additional_or(mut self, cost: Cost) -> Self {
        self.0.additional_or = Some(Box::new(cost));
        self
    }

    /// Kicker (CR 702.33).
    #[must_use]
    pub fn kicker(mut self, cost: ManaCost) -> Self {
        self.0.kicker = Some(Box::new(cost));
        self
    }

    /// A kicker cost other than mana ("Kicker—Sacrifice a creature.").
    #[must_use]
    pub fn kicker_cost(mut self, cost: Cost) -> Self {
        self.0.kicker = Some(Box::new(cost.mana.clone()));
        self.0.kicker_cost = Some(Box::new(cost));
        self
    }

    /// Morph, megamorph, or disguise (CR 702.37, 702.168).
    #[must_use]
    pub fn morph(mut self, cost: ManaCost, kind: MorphKind) -> Self {
        self.0.morph = Some(Box::new((
            Cost {
                mana: cost,
                ..Cost::default()
            },
            kind,
        )));
        self
    }

    /// Morph with a cost other than mana ("Morph—Pay 5 life.").
    #[must_use]
    pub fn morph_cost(mut self, cost: Cost) -> Self {
        self.0.morph = Some(Box::new((cost, MorphKind::Morph)));
        self
    }

    /// Evoke (CR 702.74).
    #[must_use]
    pub fn evoke(mut self, cost: ManaCost) -> Self {
        self.0.evoke = Some(Box::new(cost));
        self
    }

    /// "You may cast this spell as though it had flash. If you cast it
    /// any time a sorcery couldn't have been cast, ..." (Spider Climb).
    #[must_use]
    pub fn flash_cleanup(mut self) -> Self {
        self.0.flash_cleanup = true;
        self
    }

    /// The condition judged as the spell is cast (`AsCast`).
    #[must_use]
    pub fn cast_snapshot(mut self, condition: u16) -> Self {
        self.0.cast_snapshot = Some(condition);
        self
    }

    /// A kicked spell doesn't have its first `n` targets (CR 601.2c).
    #[must_use]
    pub fn kicked_drops(mut self, n: u8) -> Self {
        self.0.kicked_drops = Some(n);
        self
    }

    /// Targets declared after this many only if kicked (CR 601.2c).
    #[must_use]
    pub fn unkicked_targets(mut self, n: u8) -> Self {
        self.0.unkicked_targets = Some(n);
        self
    }

    /// Sneak (CR 702.190).
    #[must_use]
    pub fn sneak(mut self, cost: ManaCost) -> Self {
        self.0.sneak = Some(Box::new(cost));
        self
    }

    /// Impending N (CR 702.176).
    #[must_use]
    pub fn impending(mut self, n: u32, cost: ManaCost) -> Self {
        self.0.impending = Some(Box::new((n, cost)));
        self
    }

    /// Web-slinging (CR 702.188).
    #[must_use]
    pub fn web_slinging(mut self, cost: ManaCost) -> Self {
        self.0.web_slinging = Some(Box::new(cost));
        self
    }

    /// Harmonize (CR 702.180).
    #[must_use]
    pub fn harmonize(mut self, cost: ManaCost) -> Self {
        self.0.harmonize = Some(Box::new(cost));
        self
    }

    /// Warp (CR 702.185).
    #[must_use]
    pub fn warp(mut self, cost: ManaCost) -> Self {
        self.0.warp = Some(Box::new(cost));
        self
    }

    /// Prowl (CR 702.76a): the alternative cost, and its condition.
    #[must_use]
    pub fn prowl(mut self, cost: ManaCost, condition: u16) -> Self {
        self.0.alt_cost = Some(Box::new(AltCost {
            cost: Cost {
                mana: cost,
                ..Cost::default()
            },
            count: 1,
            condition: Some(condition),
            others_gain: 0,
        }));
        self.0.prowl = true;
        self
    }

    /// Emerge (CR 702.119a): "You may cast this spell by sacrificing a
    /// creature and paying the emerge cost reduced by that creature's mana
    /// value."
    #[must_use]
    pub fn emerge(mut self, cost: ManaCost) -> Self {
        self.0.alt_cost = Some(Box::new(AltCost {
            cost: Cost {
                mana: cost,
                sacrifice: Some(SacCost::Filter(ObjFilter::creatures())),
                ..Cost::default()
            },
            count: 1,
            condition: None,
            others_gain: 0,
        }));
        self.0.emerge = true;
        self
    }

    /// Cleave (CR 702.148a): the alternative cost, and the spell ability
    /// without the bracketed words.
    #[must_use]
    pub fn cleave(mut self, cost: ManaCost, cleaved: AbilityDef) -> Self {
        self.0.alt_cost = Some(Box::new(AltCost {
            cost: Cost {
                mana: cost,
                ..Cost::default()
            },
            count: 1,
            condition: None,
            others_gain: 0,
        }));
        self.0.cleaved = Some(Box::new(cleaved));
        self
    }

    /// Read ahead (CR 702.155a): its entering choice, and its chapters.
    #[must_use]
    pub fn read_ahead(mut self) -> Self {
        self.0.read_ahead = true;
        self.0
            .abilities
            .push(AbilityDef::Replacement(ReplacementAbility {
                replaces: Replaces::Enters,
                replacement: Replacement::EnterReadAhead,
                then: Vec::new(),
                condition: None,
            }));
        self
    }

    /// Reconfigure (CR 702.151): an Equipment creature that may equip.
    #[must_use]
    pub fn reconfigure(mut self) -> Self {
        self.0.reconfigure = true;
        self
    }

    /// Haunt (CR 702.55): the haunted creature's dies ability, and an
    /// instant's or sorcery's trigger as it goes to the graveyard.
    #[must_use]
    pub fn haunt(mut self, haunted: Option<AbilityId>, spell: Option<AbilityId>) -> Self {
        self.0.haunted = haunted;
        self.0.haunt_spell = spell;
        self
    }

    /// Blitz (CR 702.152a): the alternative cost is a blitz cost.
    #[must_use]
    pub fn blitz(mut self, cost: Cost) -> Self {
        self.0.alt_cost = Some(Box::new(AltCost {
            cost,
            count: 1,
            condition: None,
            others_gain: 0,
        }));
        self.0.blitz = true;
        self
    }

    /// Prototype (CR 702.160a): the alternative mana cost, power, and
    /// toughness.
    #[must_use]
    pub fn prototype(mut self, cost: ManaCost, power: i32, toughness: i32) -> Self {
        self.0.prototype = Some(Box::new((cost, power, toughness)));
        self
    }

    /// Dash (CR 702.109).
    #[must_use]
    pub fn dash(mut self, cost: ManaCost) -> Self {
        self.0.dash = Some(Box::new(cost));
        self
    }

    /// Bestow (CR 702.103).
    #[must_use]
    pub fn bestow(mut self, cost: ManaCost) -> Self {
        self.0.bestow = Some(Box::new(cost));
        self
    }

    /// Conspire (CR 702.78).
    #[must_use]
    pub fn conspire(mut self) -> Self {
        self.0.conspire = true;
        self
    }

    /// Dredge N (CR 702.52).
    #[must_use]
    pub fn dredge(mut self, n: u8) -> Self {
        self.0.dredge = n;
        self
    }

    /// Awaken N (CR 702.113).
    #[must_use]
    pub fn awaken(mut self, n: u8, cost: ManaCost) -> Self {
        self.0.awaken = Some((n, Box::new(cost)));
        self
    }

    /// Storm (CR 702.40).
    #[must_use]
    pub fn storm(mut self) -> Self {
        self.0.storm = true;
        self
    }

    /// Disturb (CR 702.146).
    #[must_use]
    pub fn disturb(mut self, cost: ManaCost) -> Self {
        self.0.disturb = Some(Box::new(cost));
        self
    }

    /// "If this would be put into a graveyard from anywhere, exile it
    /// instead."
    #[must_use]
    pub fn graveyard_exile(mut self) -> Self {
        self.0.graveyard_exile = true;
        self
    }

    /// Multikicker (CR 702.33c).
    #[must_use]
    pub fn multikicker(mut self, cost: ManaCost) -> Self {
        self.0.multikicker = Some(Box::new(cost));
        self
    }

    /// Replicate (CR 702.56).
    #[must_use]
    pub fn replicate(mut self, cost: ManaCost) -> Self {
        self.0.replicate = Some(cost);
        self
    }

    /// Casualty N (CR 702.153).
    #[must_use]
    pub fn casualty(mut self, n: u8) -> Self {
        self.0.casualty = n;
        self
    }

    /// Ripple N (CR 702.60).
    #[must_use]
    pub fn ripple(mut self, n: u8) -> Self {
        self.0.ripple = n;
        self
    }

    /// Teamwork N.
    #[must_use]
    pub fn teamwork(mut self, n: u8) -> Self {
        self.0.teamwork = n;
        self
    }

    /// An optional additional cost ("you may collect evidence 6").
    #[must_use]
    pub fn optional_cost(mut self, c: Cost) -> Self {
        self.0.optional_cost = Some(Box::new(c));
        self
    }

    /// "Enchanted creature is goaded" (CR 701.15).
    #[must_use]
    pub fn goads(mut self) -> Self {
        self.0.goads = true;
        self
    }

    /// Overload (CR 702.96).
    #[must_use]
    pub fn overload(mut self, cost: ManaCost) -> Self {
        self.0.overload = Some(Box::new(cost));
        self
    }

    /// Splice onto Arcane (CR 702.47).
    #[must_use]
    pub fn splice(mut self, cost: Cost) -> Self {
        self.0.splice = Some(Box::new(cost));
        self
    }

    /// "This spell has flash as long as ..." (CR 702.8).
    #[must_use]
    pub fn flash_if(mut self, condition: u16) -> Self {
        self.0.flash_if = Some(condition);
        self
    }

    /// "Cast this spell only ..." (CR 601.3).
    #[must_use]
    pub fn cast_only(mut self, condition: u16) -> Self {
        self.0.cast_only = Some(condition);
        self
    }

    /// An alternative cost of the card's own (CR 118.9).
    #[must_use]
    pub fn alt_cost(mut self, alt: AltCost) -> Self {
        self.0.alt_cost = Some(Box::new(alt));
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
    pub fn strive(mut self, cost: ManaCost) -> Self {
        self.0.strive = Some(Box::new(cost));
        self
    }

    #[must_use]
    pub fn aura_stays(mut self) -> Self {
        self.0.aura_stays = true;
        self
    }

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

    /// "This spell can't be countered."
    #[must_use]
    pub fn uncounterable(mut self) -> Self {
        self.0.uncounterable = true;
        self
    }

    /// "If <condition>, this spell can't be countered [and the damage
    /// can't be prevented]." (see `CardDef::uncounterable_if`).
    #[must_use]
    pub fn uncounterable_if(mut self, cond: u16, unpreventable: bool) -> Self {
        self.0.uncounterable_if = Some((cond, unpreventable));
        self
    }

    /// Targets chosen only if the spell is kicked (see
    /// `CardDef::kicker_targets`).
    #[must_use]
    pub fn kicker_targets(mut self, from: u8) -> Self {
        self.0.kicker_targets = Some(from);
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

    /// Miracle (CR 702.94).
    #[must_use]
    pub fn miracle(mut self, cost: ManaCost) -> Self {
        self.0.miracle = Some(Box::new(cost));
        self
    }

    /// Foretell (CR 702.143).
    #[must_use]
    pub fn foretell(mut self, cost: ManaCost) -> Self {
        self.0.foretell = Some(Box::new(cost));
        self
    }

    /// Plot (CR 702.170).
    #[must_use]
    pub fn plot(mut self, cost: ManaCost) -> Self {
        self.0.plot = Some(Box::new(cost));
        self
    }

    /// Modular N's entering counters (CR 702.43).
    #[must_use]
    pub fn modular(mut self, n: u32) -> Self {
        self.0.modular = n;
        self
    }

    /// Gift (CR 702.174): the gift's effects.
    #[must_use]
    pub fn gift(mut self, effects: Vec<Effect>) -> Self {
        self.0.gift = Some(Box::new(effects));
        self
    }

    /// Spree (CR 702.172): each mode's additional cost.
    #[must_use]
    pub fn spree(mut self, costs: Vec<ManaCost>) -> Self {
        self.0.spree = costs;
        self
    }

    /// Tiered (CR 702.183): each mode's additional cost.
    #[must_use]
    pub fn tiered(mut self, costs: Vec<ManaCost>) -> Self {
        self.0.tiered = costs;
        self
    }

    /// "Equip abilities you activate that target this creature cost {N}
    /// less to activate."
    #[must_use]
    pub fn equip_discount(mut self, n: u8) -> Self {
        self.0.equip_discount = n;
        self
    }

    /// Graft N's entering counters (CR 702.58a).
    #[must_use]
    pub fn graft(mut self, n: u32) -> Self {
        self.0.graft = n;
        self
    }

    /// Vanishing N's entering time counters (CR 702.63a).
    #[must_use]
    pub fn vanishing(mut self, n: u32) -> Self {
        self.0.vanishing = n;
        self
    }

    /// Fading N's entering fade counters (CR 702.32a).
    #[must_use]
    pub fn fading(mut self, n: u32) -> Self {
        self.0.fading = n;
        self
    }

    /// Entwine (CR 702.42).
    #[must_use]
    pub fn entwine(mut self, cost: ManaCost) -> Self {
        self.0.entwine = Some(Box::new(cost));
        self
    }

    /// "Choose one. If ..., choose both instead." (`CardDef::modes_if`).
    #[must_use]
    pub fn modes_if(mut self, m: ModesIf) -> Self {
        self.0.modes_if = Some(m);
        self
    }

    /// Escalate (CR 702.120a) on a "choose one or more" spell.
    #[must_use]
    pub fn escalate(mut self, cost: ManaCost) -> Self {
        self.0.escalate = Some(Box::new(cost));
        self
    }

    /// Mayhem: cast from the graveyard for `cost` the turn it was
    /// discarded.
    #[must_use]
    pub fn mayhem(mut self, cost: ManaCost) -> Self {
        self.0.mayhem = Some(Box::new(cost));
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

    /// "Buyback—Sacrifice a land." (CR 702.27a).
    #[must_use]
    pub fn buyback_sacrifice(mut self, filter: ObjFilter) -> Self {
        self.0.buyback = Some(Box::default());
        self.0.buyback_sacrifice = Some(Box::new(filter));
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

    /// "If this card is in a graveyard, effects from spells named X count
    /// it as a card named X."
    #[must_use]
    pub fn graveyard_alias(mut self, name: String) -> Self {
        self.0.graveyard_alias = Some(name);
        self
    }

    /// "You may cast this card from your graveyard ..." (CR 601.3): for
    /// `cost` (the mana cost and other parts unless `alt_mana`), not
    /// exiled as it leaves the stack.
    #[must_use]
    pub fn graveyard_cast(mut self, cost: Cost, how: GraveyardCast) -> Self {
        self.0.flashback = Some(Box::new(cost));
        self.0.retrace = true;
        self.0.graveyard_cast = Some(Box::new(how));
        self
    }

    /// Retrace (CR 702.81a): cast from the graveyard for its mana cost and
    /// a land card discarded; it isn't exiled.
    #[must_use]
    pub fn retrace(mut self, land: ObjFilter) -> Self {
        let mana = self.0.mana_cost.clone();
        self.0.flashback = Some(Box::new(Cost {
            mana,
            discard: 1,
            discard_filter: Some(land),
            ..Cost::default()
        }));
        self.0.retrace = true;
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
    fn keyword_set_iterates_in_all_order() {
        // Sets of one, of each pair, and of everything: `iter` yields what
        // filtering `Keyword::ALL` by `contains` does, in that order.
        let check = |s: KeywordSet| {
            let want: Vec<Keyword> = Keyword::ALL
                .into_iter()
                .filter(|k| s.contains(*k))
                .collect();
            assert_eq!(s.iter().collect::<Vec<_>>(), want);
        };
        check(KeywordSet::NONE);
        check(KeywordSet::of(Keyword::ALL));
        for a in Keyword::ALL {
            for b in Keyword::ALL {
                check(KeywordSet::of([a, b]));
            }
        }
    }

    #[test]
    fn triggered_keywords_are_those_with_an_ability() {
        // Trigger detection walks a watcher's keywords only when they meet
        // `KeywordSet::TRIGGERED`.
        let pool = test_pool();
        for k in Keyword::ALL {
            assert_eq!(
                pool.keyword_trigger(k).is_some(),
                KeywordSet::TRIGGERED.contains(k),
                "{k:?}"
            );
        }
    }

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
