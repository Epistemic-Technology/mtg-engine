//! Game state: players, objects, zones, combat, continuous effects, and
//! derived characteristics. A flat, clonable value (design §4.2).

use std::cell::RefCell;
use std::sync::Arc;

use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;
use smallvec::SmallVec;

pub use crate::card::Layer;
use crate::card::{
    AbilityId, AbilitySet, CardDef, CardPool, CardRef, CardType, CounterKind, Keyword, KeywordSet,
    Modification, ObjFilter, PlayerRuleKind, ReplacementAbility, RuleKind, SubtypeSet,
    SupertypeSet, TargetSpec, TypeSet,
};
use crate::decision::{ChoiceId, Decision};
use crate::event::Event;
use crate::ids::{IdMap, ObjId, PlayerId, Timestamp};
use crate::mana::{ColorSet, ManaPool};
use crate::turn::Step;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Zone {
    Library,
    Hand,
    Battlefield,
    Graveyard,
    Stack,
    Exile,
    Command,
    /// Not a zone: a phased-out permanent (CR 702.26b), still on the
    /// battlefield by the rules but treated as though it doesn't exist. It
    /// keeps its object id and is kept out of `GameState::battlefield`.
    PhasedOut,
}

impl Zone {
    /// Whether every player may look at the zone's contents (CR 400.2).
    pub const fn is_public(self) -> bool {
        matches!(
            self,
            Zone::Battlefield
                | Zone::Graveyard
                | Zone::Stack
                | Zone::Exile
                | Zone::Command
                | Zone::PhasedOut
        )
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ObjKind {
    Card,
    Token,
    /// An emblem in the command zone (CR 114).
    Emblem,
    /// An ability on the stack (CR 113.2b). Ceases to exist when it leaves
    /// the stack.
    Ability,
}

/// What an ability object on the stack is an instance of.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct StackAbility {
    pub ability: AbilityId,
    /// The object the ability came from, by the id it had when it triggered
    /// (possibly no longer on the battlefield; see `GameState::lki`).
    pub source: ObjId,
    /// The source's definition at that time (a copy's copied card), for
    /// naming the ability once the source is gone.
    pub def: CardRef,
    /// The object whose action triggered it, if any.
    pub trigger_obj: Option<ObjId>,
    /// The player whose action triggered it, if any.
    pub trigger_player: Option<PlayerId>,
    /// The amount of the triggering event (damage dealt), for "that much".
    pub trigger_amount: u32,
}

/// A player's untapped mana sources, as `mana_sources` lists them.
pub type ManaSources = smallvec::SmallVec<[(ObjId, crate::mana::Mana); 12]>;

/// A triggered ability that has triggered but is not yet on the stack
/// (CR 603.3).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PendingTrigger {
    pub controller: PlayerId,
    pub ability: StackAbility,
    pub timestamp: Timestamp,
}

/// A permanent as it was just before a batch of simultaneous zone changes,
/// for leaves-the-battlefield triggers (CR 603.10a).
#[derive(Clone, Debug)]
pub struct Lookback {
    pub id: ObjId,
    pub controller: PlayerId,
    pub tapped: bool,
    pub characteristics: Characteristics,
    pub except: Option<CardRef>,
}

/// A delayed triggered ability (CR 603.7): created by an effect, triggers
/// once at the next matching step or event, then ceases to exist.
#[derive(Copy, Clone, Debug)]
pub struct DelayedTrigger {
    pub controller: PlayerId,
    pub ability: StackAbility,
    /// "When that creature dies this turn": ends at cleanup if it hasn't
    /// triggered (CR 603.7c).
    pub this_turn: bool,
    /// "... for the rest of the game" (paradigm, CR 702.192a): it stays
    /// after it triggers.
    pub forever: bool,
}

/// Something a spell or ability can target or an effect can act on.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Target {
    Object(ObjId),
    Player(PlayerId),
}

impl Target {
    /// A declined optional target ("up to one target artifact"): it holds
    /// the slot so the later targets keep their places (CR 601.2c). It
    /// names no object (a null key), is never legal, and doesn't count
    /// toward "all its targets are illegal" (CR 608.2b).
    #[must_use]
    pub fn none() -> Self {
        Target::Object(ObjId::default())
    }

    /// Whether this is a declined target's placeholder.
    #[must_use]
    pub fn is_none(self) -> bool {
        use slotmap::Key as _;
        matches!(self, Target::Object(id) if id.is_null())
    }
}

/// `GameObject::marks` bits.
pub const OBJ_RENOWNED: u8 = 1;
pub const OBJ_MONSTROUS: u8 = 2;
pub const OBJ_RIOT_HASTE: u8 = 4;
/// The prepared designation (CR 722.3a).
pub const OBJ_PREPARED: u8 = 8;
/// Suspected (CR 701.60): it has menace and "can't block" for as long as
/// it is suspected.
pub const OBJ_SUSPECTED: u8 = 16;

impl GameObject {
    pub const fn renowned(&self) -> bool {
        self.marks & OBJ_RENOWNED != 0
    }

    pub const fn monstrous(&self) -> bool {
        self.marks & OBJ_MONSTROUS != 0
    }

    pub const fn riot_haste(&self) -> bool {
        self.marks & OBJ_RIOT_HASTE != 0
    }

    pub const fn prepared(&self) -> bool {
        self.marks & OBJ_PREPARED != 0
    }
    pub const fn suspected(&self) -> bool {
        self.marks & OBJ_SUSPECTED != 0
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] // independent facts
pub struct GameObject {
    /// The definition whose copiable values this object has (CR 707.2):
    /// the card itself, or what a copy effect made it a copy of.
    pub def: CardRef,
    /// The physical card (what it is again when it leaves the battlefield,
    /// CR 400.7). Equal to `def` unless a copy effect applies.
    pub card: CardRef,
    /// The definition whose `copy_except` is part of this copy's copiable
    /// values (CR 707.9b).
    pub except: Option<CardRef>,
    /// The chosen mode of a modal spell (CR 700.2) and how it was cast
    /// (kicked, with flashback), packed in one byte.
    pub cast: CastInfo,
    pub kind: ObjKind,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub zone: Zone,
    pub tapped: bool,
    pub damage: u32,
    /// Dealt damage by a deathtouch source since the last SBA check (CR 704.5h).
    pub deathtouch_damage: bool,
    /// Under its controller's control continuously since their most recent
    /// turn began (CR 302.6).
    pub controlled_since_turn_start: bool,
    /// Came onto the battlefield since its echo was last dealt with (CR
    /// 702.30a): set as it enters, cleared by its echo trigger.
    pub echo_due: bool,
    /// Per-object marks (`OBJ_RENOWNED`, `OBJ_MONSTROUS`, `OBJ_RIOT_HASTE`,
    /// `OBJ_PREPARED`): renowned (CR 702.112b), monstrous (CR 701.37b),
    /// riot's haste (CR 702.136a), and prepared (CR 722.3a). One byte, so
    /// `GameObject` stays small.
    pub marks: u8,
    /// Skips its controller's next untap step ("doesn't untap during its
    /// controller's next untap step").
    pub skip_untap: bool,
    /// What its controller chose as it entered ("As this land enters,
    /// choose a color", CR 614.12), for the abilities that refer to it
    /// (CR 607.2a). A new object has none (CR 400.7).
    pub chosen: Chosen,
    /// How the spell was cast, or what an exiled card is waiting for
    /// (madness, foretell, suspend, rebound).
    pub alt: AltCast,
    pub timestamp: Timestamp,
    /// Chosen targets, for a spell on the stack.
    pub targets: SmallVec<[Target; 2]>,
    /// Counters on the object (CR 122).
    pub counters: SmallVec<[(CounterKind, u32); 2]>,
    /// For Auras and Equipment: what this is attached to (CR 301.5, 303.4).
    pub attached_to: Option<ObjId>,
    /// For `ObjKind::Ability` objects.
    pub ability: Option<StackAbility>,
    /// The players (bit `i` for player `i`) who have seen this card while
    /// it is in a hidden zone: it was revealed from a hand ("Target player
    /// reveals their hand", CR 701.20) or revealed as it was put there
    /// ("reveal that card, put it into your hand"). A zone change makes a
    /// new object (CR 400.7) that nobody has seen, so the knowledge lasts
    /// until the card moves; `determinize` keeps a card the viewer has
    /// seen where it is.
    pub revealed: u8,
}

/// A choice an object remembers (`GameObject::chosen`): small, so the
/// object stays small.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Chosen {
    #[default]
    None,
    Color(crate::mana::Color),
    /// A creature type or basic land type.
    Subtype(crate::card::Subtype),
    /// A resolution's vote (CR 701.38): the votes for each of its two
    /// choices.
    Votes([u8; 2]),
}

impl Chosen {
    pub const fn color(self) -> Option<crate::mana::Color> {
        match self {
            Chosen::Color(c) => Some(c),
            _ => None,
        }
    }
}

/// Cards waiting to be cast from exile, and foretold ones.
#[derive(Clone, Debug, Default)]
pub struct ExileCasts {
    /// Cards to be cast as the current resolution finishes ("you may cast
    /// it without paying its mana cost"): the caster, the card, how.
    pub pending: Vec<(PlayerId, ObjId, AltCast)>,
    /// Pending cards to be cast transformed, with the face they are cast
    /// as (CR 712.11a: a defeated Siege).
    pub transformed: Vec<(ObjId, CardRef)>,
    /// Cards foretold or plotted this turn (CR 702.143a, 702.170d:
    /// castable on a later turn).
    pub foretold_this_turn: Vec<ObjId>,
    /// Cards exiled by impulse effects that a player may play.
    pub playable: Vec<PlayableExile>,
}

/// `PlayableExile::own_ends` for a warped card exiled this turn: castable
/// from the next turn on (CR 702.185a).
pub const WARP_LATER: u8 = u8::MAX;

/// An exiled card a player may play (impulse draw), and until when.
#[derive(Copy, Clone, Debug)]
pub struct PlayableExile {
    pub card: ObjId,
    pub player: PlayerId,
    pub until: crate::card::PlayUntil,
    /// For `EndOfYourNextTurn`: how many more of the player's turns must
    /// end before the permission does. `WARP_LATER` with `Forever`: not
    /// until a later turn (CR 702.185a).
    pub own_ends: u8,
    /// "you may cast that card": a spell only, not a land play.
    pub cast_only: bool,
    /// "... without paying its mana cost".
    pub free: bool,
}

/// How a spell was cast (on the stack; `Suspend` stays with the permanent
/// for its haste), or what an exiled card waits for.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum AltCast {
    #[default]
    None,
    /// Cast from its owner's hand (rebound, CR 702.88a).
    Hand,
    /// Cast for its madness cost (CR 702.35).
    Madness,
    /// Cast for its foretell cost (CR 702.143).
    Foretell,
    /// Cast without paying its mana cost (cascade, rebound).
    Free,
    /// Cast without paying its mana cost as the last time counter was
    /// removed; a creature has haste (CR 702.62a).
    Suspend,
    /// Cast from the graveyard with escape (CR 702.138); kept by the
    /// permanent ("escapes with").
    Escape,
    /// Cast from the graveyard for its mayhem cost, the turn it was
    /// discarded.
    Mayhem,
    /// In exile: discarded with madness, waiting for its trigger.
    MadnessExiled,
    /// In exile: foretold, castable on a later turn.
    Foretold,
    /// In exile: plotted (CR 702.170a), castable free as a sorcery on a
    /// later turn.
    Plotted,
    /// In exile: suspended, with time counters.
    Suspended,
    /// In exile: rebounding, castable at its owner's next upkeep.
    Rebounding,
    /// Cast for its dash cost (CR 702.109a); kept by the permanent, which
    /// has haste.
    Dash,
    /// Cast for its warp cost (CR 702.185a).
    Warp,
    /// Cast for its sneak cost (CR 702.190a); kept by the permanent, which
    /// enters tapped and attacking.
    Sneak,
    /// Cast for its impending cost (CR 702.176a); kept by the permanent,
    /// which enters with time counters and isn't a creature while it has
    /// one.
    Impending,
    /// Cast for its web-slinging cost (CR 702.188a); kept by the
    /// permanent ("if it was cast using web-slinging").
    WebSlinging,
    /// Cast from the graveyard for its harmonize cost (CR 702.180a); the
    /// spell is exiled as it leaves the stack (`CastInfo::flashback`).
    Harmonize,
    /// Cast for the alternative cost its own text offers (CR 118.9, "rather
    /// than pay this spell's mana cost").
    AltCost,
    /// Cast for its bestow cost (CR 702.103a): an Aura spell.
    Bestow,
    /// Cast for its overload cost (CR 702.96a): "each" for "target".
    Overload,
    /// Cast for its awaken cost (CR 702.113a).
    Awaken,
    /// Cast transformed from the graveyard for its disturb cost (CR
    /// 702.146a): the spell is the back face.
    Disturb,
    /// In exile: airbent (CR 701.65a), castable by its owner for {2}
    /// rather than its mana cost for as long as it remains exiled.
    Airbent,
    /// Cast for {2} rather than its mana cost after it was airbent.
    Airbend,
    /// Cast without paying its mana cost, and exiled instead of put into
    /// its owner's graveyard ("If that spell would be put into your
    /// graveyard, exile it instead", CR 614.1a).
    FreeExile,
}

/// The mana spent to cast a spell (CR 601.2h), as much as cards ask
/// about: how much of each color, colorless, and how much came from
/// Treasures.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct SpentMana {
    /// Indexed by `Color as usize`.
    pub colored: [u8; 5],
    pub colorless: u8,
    pub treasure: u8,
}

impl SpentMana {
    /// How much of the spent mana was of this kind.
    pub fn count(&self, kind: crate::card::SpentKind) -> u32 {
        use crate::card::SpentKind;
        match kind {
            SpentKind::Color(c) => u32::from(self.colored[c as usize]),
            SpentKind::Treasure => u32::from(self.treasure),
            SpentKind::Total => {
                self.colored.iter().map(|n| u32::from(*n)).sum::<u32>() + u32::from(self.colorless)
            }
        }
    }

    /// The number of colors among the spent mana (converge).
    pub fn colors(&self) -> u32 {
        u32::try_from(self.colored.iter().filter(|n| **n > 0).count()).unwrap_or(0)
    }

    /// Add what a payment took from the pool: `before` less `after`.
    pub fn add_paid(&mut self, before: &crate::mana::ManaPool, after: &crate::mana::ManaPool) {
        for (i, slot) in self.colored.iter_mut().enumerate() {
            let n = before.colored[i].saturating_sub(after.colored[i]);
            *slot = slot.saturating_add(u8::try_from(n).unwrap_or(u8::MAX));
        }
        let n = before.colorless.saturating_sub(after.colorless);
        self.colorless = self
            .colorless
            .saturating_add(u8::try_from(n).unwrap_or(u8::MAX));
    }
}

/// A spell's chosen mode (CR 700.2, the low six bits) and cast flags:
/// kicked (CR 702.33c; kept by the permanent it becomes, for "if it was
/// kicked") and cast with flashback (CR 702.34a; exiled as it leaves the
/// stack). Four bytes; `GameObject` stays within its size bound.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct CastInfo(u32);

impl CastInfo {
    /// Every flag and field, for the test that no two share a bit
    /// (parallel branches have twice picked the same free bit).
    #[cfg(test)]
    const ALL: [u32; 21] = [
        Self::KICKS_MASK,
        Self::OFFSPRING,
        Self::BARGAINED,
        Self::OTHER_COST,
        Self::CONSPIRED,
        Self::COPY,
        Self::INSTANT_TIMING,
        Self::AS_CAST,
        Self::CASUALTY,
        Self::TEAMWORK,
        Self::OPTIONAL_PAID,
        Self::REPLICATE_MASK,
        Self::BEHELD,
        Self::KICKED,
        Self::FLASHBACK,
        Self::ADVENTURE,
        Self::BUYBACK,
        Self::EVOKED,
        Self::MODE,
        Self::MODE_HIGH,
        Self::CAST_MASK,
    ];

    /// Cast with its offspring cost paid (CR 702.175a); kept by the
    /// permanent, for its trigger.
    const OFFSPRING: u32 = 0x100;
    /// Bargained (CR 702.166b); kept by the permanent ("if it was
    /// bargained").
    const BARGAINED: u32 = 0x200;
    /// Cast paying the second of its two additional costs ("sacrifice a
    /// creature or pay {3}", CR 601.2b).
    const OTHER_COST: u32 = 0x8000;
    /// Conspired (CR 702.78a): two creatures are tapped with its costs,
    /// and it is copied.
    const CONSPIRED: u32 = 0x2000;
    /// A copy of a spell (CR 707.10): not cast, and ceases to exist as it
    /// leaves the stack (CR 704.5e).
    const COPY: u32 = 0x4000;
    /// Cast any time a sorcery couldn't have been (CR 307.1), by a card
    /// whose permanent is then sacrificed at the next cleanup step (Spider
    /// Climb).
    const INSTANT_TIMING: u32 = 0x1_0000;
    /// The definition's `cast_snapshot` condition held as it was cast.
    const AS_CAST: u32 = 0x2_0000;
    /// Its casualty cost paid (CR 702.153a): a creature sacrificed with
    /// its costs, and it is copied.
    const CASUALTY: u32 = 0x4_0000;
    /// Cast using teamwork: creatures with enough total power tapped with
    /// its costs (an optional additional cost, CR 601.2b).
    const TEAMWORK: u32 = 0x8_0000;
    /// Its optional additional cost (`CardDef::optional_cost`) paid; kept
    /// by the permanent ("if evidence was collected").
    const OPTIONAL_PAID: u32 = 0x1000_0000;
    /// How many times its replicate cost was paid (CR 702.56a), up to 15.
    const REPLICATE_MASK: u32 = 0xF0_0000;
    const REPLICATE_SHIFT: u32 = 20;
    /// Its behold cost was paid ("If a Dragon was beheld", CR 701.69).
    const BEHELD: u32 = 0x2000_0000;
    /// How many times its multikicker cost was paid (CR 702.33c), up to
    /// 15; kept by the permanent ("for each time it was kicked").
    const KICKS_MASK: u32 = 0x0F00_0000;
    const KICKS_SHIFT: u32 = 24;
    const KICKED: u32 = 0x40;
    const FLASHBACK: u32 = 0x80;
    /// Cast as an adventure (CR 715.3) on the stack; "on an adventure" in
    /// exile.
    const ADVENTURE: u32 = 0x20;
    /// Cast with its buyback cost paid (CR 702.27): to its owner's hand as
    /// it resolves.
    const BUYBACK: u32 = 0x10;
    /// Cast for its evoke cost (CR 702.74); kept by the permanent.
    const EVOKED: u32 = 0x08;
    const MODE: u32 = 0x07;
    /// The mode's fourth bit (modes 8 to 15: pawprint combinations, CR
    /// 700.2i).
    const MODE_HIGH: u32 = 0x4000_0000;
    /// Cast at all, and from where (CR 601.2a); kept by the permanent ("if
    /// you cast it from your hand").
    const CAST_MASK: u32 = 0x1C00;
    const CAST_HAND: u32 = 0x0400;
    const CAST_GRAVEYARD: u32 = 0x0800;
    const CAST_ELSEWHERE: u32 = 0x1000;

    /// The zone the spell was cast from, `None` if it wasn't cast (a copy,
    /// a permanent put onto the battlefield); exile and the library read
    /// as `Zone::Exile`.
    pub const fn cast_from(self) -> Option<Zone> {
        match self.0 & Self::CAST_MASK {
            Self::CAST_HAND => Some(Zone::Hand),
            Self::CAST_GRAVEYARD => Some(Zone::Graveyard),
            Self::CAST_ELSEWHERE => Some(Zone::Exile),
            _ => None,
        }
    }

    pub fn set_cast_from(&mut self, zone: Zone) {
        let bit = match zone {
            Zone::Hand => Self::CAST_HAND,
            Zone::Graveyard => Self::CAST_GRAVEYARD,
            _ => Self::CAST_ELSEWHERE,
        };
        self.0 = (self.0 & !Self::CAST_MASK) | bit;
    }

    pub const fn as_cast(self) -> bool {
        self.0 & Self::AS_CAST != 0
    }

    pub fn set_as_cast(&mut self) {
        self.0 |= Self::AS_CAST;
    }

    pub const fn instant_timing(self) -> bool {
        self.0 & Self::INSTANT_TIMING != 0
    }

    pub fn set_instant_timing(&mut self) {
        self.0 |= Self::INSTANT_TIMING;
    }

    pub const fn offspring(self) -> bool {
        self.0 & Self::OFFSPRING != 0
    }

    pub fn set_offspring(&mut self, on: bool) {
        self.0 = if on {
            self.0 | Self::OFFSPRING
        } else {
            self.0 & !Self::OFFSPRING
        };
    }

    pub const fn other_cost(self) -> bool {
        self.0 & Self::OTHER_COST != 0
    }

    pub fn set_other_cost(&mut self, on: bool) {
        self.0 = if on {
            self.0 | Self::OTHER_COST
        } else {
            self.0 & !Self::OTHER_COST
        };
    }

    pub const fn bargained(self) -> bool {
        self.0 & Self::BARGAINED != 0
    }

    pub fn set_bargained(&mut self, on: bool) {
        self.0 = if on {
            self.0 | Self::BARGAINED
        } else {
            self.0 & !Self::BARGAINED
        };
    }

    pub const fn optional_paid(self) -> bool {
        self.0 & Self::OPTIONAL_PAID != 0
    }

    pub fn set_optional_paid(&mut self, on: bool) {
        self.0 = if on {
            self.0 | Self::OPTIONAL_PAID
        } else {
            self.0 & !Self::OPTIONAL_PAID
        };
    }

    pub const fn teamwork(self) -> bool {
        self.0 & Self::TEAMWORK != 0
    }

    pub fn set_teamwork(&mut self, on: bool) {
        self.0 = if on {
            self.0 | Self::TEAMWORK
        } else {
            self.0 & !Self::TEAMWORK
        };
    }

    /// How many times its replicate cost was paid (CR 702.56a).
    pub const fn replicated(self) -> u32 {
        (self.0 & Self::REPLICATE_MASK) >> Self::REPLICATE_SHIFT
    }

    pub fn set_replicated(&mut self, n: u32) {
        self.0 = (self.0 & !Self::REPLICATE_MASK)
            | ((n.min(15) << Self::REPLICATE_SHIFT) & Self::REPLICATE_MASK);
    }

    /// How many times it was kicked with its multikicker (CR 702.33d).
    pub const fn kicks(self) -> u32 {
        (self.0 & Self::KICKS_MASK) >> Self::KICKS_SHIFT
    }

    /// Kicked `n` times with its multikicker; kicked at all if `n > 0`
    /// (CR 702.33d).
    pub fn set_kicks(&mut self, n: u32) {
        self.0 =
            (self.0 & !Self::KICKS_MASK) | ((n.min(15) << Self::KICKS_SHIFT) & Self::KICKS_MASK);
        self.set_kicked(n > 0);
    }

    pub const fn casualty(self) -> bool {
        self.0 & Self::CASUALTY != 0
    }

    pub fn set_casualty(&mut self, on: bool) {
        self.0 = if on {
            self.0 | Self::CASUALTY
        } else {
            self.0 & !Self::CASUALTY
        };
    }

    pub const fn conspired(self) -> bool {
        self.0 & Self::CONSPIRED != 0
    }

    pub fn set_conspired(&mut self, on: bool) {
        self.0 = if on {
            self.0 | Self::CONSPIRED
        } else {
            self.0 & !Self::CONSPIRED
        };
    }

    pub const fn copy(self) -> bool {
        self.0 & Self::COPY != 0
    }

    /// The cast info of a copy of a spell with this one (CR 707.10: the
    /// choices made casting it, but not cast).
    #[must_use]
    pub const fn copied(self) -> CastInfo {
        // (Also marks a copy of a card cast from exile, cipher's.)
        CastInfo((self.0 & !(Self::CAST_MASK | Self::CONSPIRED)) | Self::COPY)
    }

    pub const fn evoked(self) -> bool {
        self.0 & Self::EVOKED != 0
    }

    pub fn set_evoked(&mut self, on: bool) {
        self.0 = if on {
            self.0 | Self::EVOKED
        } else {
            self.0 & !Self::EVOKED
        };
    }

    pub const fn bought_back(self) -> bool {
        self.0 & Self::BUYBACK != 0
    }

    pub fn set_bought_back(&mut self, on: bool) {
        self.0 = if on {
            self.0 | Self::BUYBACK
        } else {
            self.0 & !Self::BUYBACK
        };
    }

    pub const fn mode(self) -> u8 {
        let high = if self.0 & Self::MODE_HIGH != 0 { 8 } else { 0 };
        (self.0 & Self::MODE) as u8 | high
    }

    /// The chosen mode, 0 to 15.
    pub fn set_mode(&mut self, mode: u8) {
        debug_assert!(mode < 16, "at most sixteen modes");
        let high = if mode & 8 != 0 { Self::MODE_HIGH } else { 0 };
        self.0 = (self.0 & !(Self::MODE | Self::MODE_HIGH)) | (u32::from(mode) & Self::MODE) | high;
    }

    pub const fn beheld(self) -> bool {
        self.0 & Self::BEHELD != 0
    }

    pub fn set_beheld(&mut self) {
        self.0 |= Self::BEHELD;
    }

    pub const fn kicked(self) -> bool {
        self.0 & Self::KICKED != 0
    }

    pub fn set_kicked(&mut self, kicked: bool) {
        self.0 = if kicked {
            self.0 | Self::KICKED
        } else {
            self.0 & !Self::KICKED
        };
    }

    pub const fn adventure(self) -> bool {
        self.0 & Self::ADVENTURE != 0
    }

    pub fn set_adventure(&mut self, adventure: bool) {
        self.0 = if adventure {
            self.0 | Self::ADVENTURE
        } else {
            self.0 & !Self::ADVENTURE
        };
    }

    pub const fn flashback(self) -> bool {
        self.0 & Self::FLASHBACK != 0
    }

    pub fn set_flashback(&mut self, flashback: bool) {
        self.0 = if flashback {
            self.0 | Self::FLASHBACK
        } else {
            self.0 & !Self::FLASHBACK
        };
    }

    /// What a permanent keeps of its spell's cast info (CR 702.33c).
    #[must_use]
    pub const fn kept_by_permanent(self) -> CastInfo {
        CastInfo(
            self.0
                & (Self::KICKED
                    | Self::KICKS_MASK
                    | Self::EVOKED
                    | Self::OFFSPRING
                    | Self::BARGAINED
                    | Self::OPTIONAL_PAID
                    | Self::CAST_MASK),
        )
    }
}

impl GameObject {
    /// Whether a copy effect applies (CR 707).
    pub fn is_copy(&self) -> bool {
        self.def != self.card || self.except.is_some()
    }

    pub fn counters(&self, kind: CounterKind) -> u32 {
        if kind == CounterKind::Any {
            return self.counters.iter().map(|(_, n)| *n).sum();
        }
        self.counters
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(0, |(_, n)| *n)
    }

    pub fn add_counters(&mut self, kind: CounterKind, n: u32) {
        if let Some(slot) = self.counters.iter_mut().find(|(k, _)| *k == kind) {
            slot.1 += n;
        } else {
            self.counters.push((kind, n));
        }
    }

    /// Whether its counters satisfy "with a +1/+1 counter on it", "with
    /// three or more level counters on it" (CR 122.1).
    pub fn counters_match(&self, q: crate::card::CounterQual) -> bool {
        match q {
            crate::card::CounterQual::Any => self.counters.iter().any(|(_, n)| *n > 0),
            crate::card::CounterQual::Kind(k) => self.counters(k) > 0,
            crate::card::CounterQual::AtLeast(k, n) => self.counters(k) >= n,
        }
    }

    pub fn remove_counters(&mut self, kind: CounterKind, n: u32) {
        if let Some(i) = self.counters.iter().position(|(k, _)| *k == kind) {
            self.counters[i].1 = self.counters[i].1.saturating_sub(n);
            if self.counters[i].1 == 0 {
                self.counters.remove(i);
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum LossReason {
    LifeZeroOrLess,
    DrewFromEmptyLibrary,
    /// Ten or more poison counters (CR 704.5c).
    Poison,
    Conceded,
    /// An effect made an opponent win the game (CR 104.2b).
    OpponentWon,
}

#[derive(Clone, Debug)]
pub struct PlayerState {
    pub life: i32,
    /// Index 0 is the bottom; the last element is the top.
    pub library: Vec<ObjId>,
    pub hand: Vec<ObjId>,
    pub graveyard: Vec<ObjId>,
    pub exile: Vec<ObjId>,
    pub mana_pool: ManaPool,
    pub lands_played_this_turn: u32,
    /// Additional land plays granted this turn by resolved effects ("You
    /// may play an additional land this turn", CR 305.2).
    pub extra_land_plays: u32,
    /// Cards in the graveyard with flashback, so priority options skip the
    /// graveyard scan when there are none.
    pub flashback_in_graveyard: u32,
    /// Cards this player owns that are on an adventure in exile (CR
    /// 715.3d), so priority options skip the exile scan when there are none.
    pub adventures_in_exile: u32,
    pub max_hand_size: usize,
    pub drew_from_empty_library: bool,
    /// Poison counters (CR 122.1f); ten or more lose the game (704.5c).
    pub poison: u32,
    /// Energy counters (CR 122.1, 107.14).
    pub energy: u32,
    pub lost: Option<LossReason>,
    pub mulligans_taken: u8,
    /// Draw steps this player skips (CR 500.11): "Target player skips
    /// their next draw step."
    pub skip_draw: u8,
    /// Combat phases this player skips: "Target opponent skips their next
    /// combat phase."
    pub skip_combat: u8,
    /// Combat phases this player skips this turn only ("... skips their
    /// next combat phase this turn"); cleared as the turn ends.
    pub skip_combat_this_turn: u8,
    /// Red mana in the pool that lasts until end of combat (firebending,
    /// CR 702.189a): at most this much red stays as each combat step
    /// ends (the player spends their other mana first).
    pub combat_mana: u32,
}

impl PlayerState {
    pub fn zone_mut(&mut self, zone: Zone) -> &mut Vec<ObjId> {
        match zone {
            Zone::Library => &mut self.library,
            Zone::Hand => &mut self.hand,
            Zone::Graveyard => &mut self.graveyard,
            Zone::Exile => &mut self.exile,
            Zone::Battlefield | Zone::Stack | Zone::Command | Zone::PhasedOut => {
                unreachable!("{zone:?} is not a per-player zone")
            }
        }
    }
}

/// A phased-out permanent (CR 702.26b).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PhasedOut {
    pub id: ObjId,
    /// The player it phased out under: it phases in during their untap
    /// step (CR 702.26a).
    pub controller: PlayerId,
    /// It phased out with the permanent it's attached to and phases in
    /// with it (CR 702.26g).
    pub indirect: bool,
}

#[derive(Clone, Debug)]
pub struct Attacker {
    pub id: ObjId,
    pub attacking: Target,
    pub blockers: SmallVec<[ObjId; 4]>,
    /// Set once blockers are declared; stays true even if blockers leave
    /// combat (CR 509.1h).
    pub blocked: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CombatState {
    pub attackers: Vec<Attacker>,
    /// Attacking bands (CR 702.22c), two or more creatures each.
    pub bands: Vec<Vec<ObjId>>,
    /// Whether a first-strike combat damage step occurred this combat (CR 510.4).
    pub had_first_strike_step: bool,
}

impl CombatState {
    pub fn attacker(&self, id: ObjId) -> Option<&Attacker> {
        self.attackers.iter().find(|a| a.id == id)
    }

    /// The other creatures in `id`'s attacking band (CR 702.22).
    pub fn band_of(&self, id: ObjId) -> &[ObjId] {
        self.bands
            .iter()
            .find(|b| b.contains(&id))
            .map_or(&[], Vec::as_slice)
    }

    pub fn attacker_mut(&mut self, id: ObjId) -> Option<&mut Attacker> {
        self.attackers.iter_mut().find(|a| a.id == id)
    }

    pub fn blocked_by(&self, blocker: ObjId) -> Option<ObjId> {
        self.attackers
            .iter()
            .find(|a| a.blockers.contains(&blocker))
            .map(|a| a.id)
    }

    pub fn is_in_combat(&self, id: ObjId) -> bool {
        self.attackers
            .iter()
            .any(|a| a.id == id || a.blockers.contains(&id))
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Duration {
    UntilEndOfTurn,
    Permanent,
    /// Until the given player's next turn begins (CR 611.2b): "until your
    /// next turn".
    UntilYourNextTurn(PlayerId),
    /// "For as long as this creature remains on the battlefield" (CR
    /// 611.2b): while that object is on the battlefield.
    WhileOnBattlefield(ObjId),
    /// "For as long as it remains tapped" (CR 611.2b): while that object
    /// is a tapped permanent.
    WhileTapped(ObjId),
    /// "For as long as you control this creature" (CR 611.2b): while that
    /// object is on the battlefield under that player's control.
    WhileControlled(ObjId, PlayerId),
}

impl Duration {
    /// Durations that end when an object leaves, untaps, or changes
    /// control: checked with the state-based actions.
    pub const fn linked(self) -> bool {
        matches!(
            self,
            Duration::WhileOnBattlefield(_)
                | Duration::WhileTapped(_)
                | Duration::WhileControlled(..)
        )
    }
}

/// What a stored continuous effect applies to.
#[derive(Clone, Debug)]
pub enum Affected {
    /// A set locked in when the effect was created (CR 611.2c).
    Objects(SmallVec<[ObjId; 2]>),
    /// A filter re-evaluated as the layers are applied (CR 611.3), judged
    /// from `controller`'s point of view.
    Filter {
        filter: ObjFilter,
        controller: PlayerId,
    },
}

/// A continuous effect created by a resolving spell or ability (CR 611.2).
/// Effects of static abilities are not stored; the layer algorithm derives
/// them from the battlefield each time (CR 611.3).
#[derive(Clone, Debug)]
pub struct ContinuousEffect {
    pub source: Option<ObjId>,
    /// The player who controlled the spell or ability that created it
    /// (CR 611.2: the effect's controller), for "you" filters and control
    /// changes.
    pub controller: PlayerId,
    pub timestamp: Timestamp,
    pub duration: Duration,
    pub affected: Affected,
    pub parts: SmallVec<[Modification; 2]>,
}

/// A replacement or prevention effect created by a resolving spell or
/// ability (CR 614.3, 615.3): Fog's "this turn" effect, a damage shield.
/// Effects of static abilities are not stored; the pipeline reads them from
/// the battlefield each time.
#[derive(Clone, Debug)]
pub struct ReplacementEffect {
    pub source: Option<ObjId>,
    /// The card the effect came from, for naming it.
    pub def: CardRef,
    pub controller: PlayerId,
    pub timestamp: Timestamp,
    pub duration: Duration,
    pub ability: ReplacementAbility,
    /// The spell's chosen target this effect is locked to (CR 611.2c).
    pub target: Option<Target>,
    /// Damage left on a shield (CR 615.7); 1 for an unused "next time"
    /// shield.
    pub remaining: u32,
    /// The damage source the controller chose as the effect was created
    /// (CR 609.7a).
    pub chosen: Option<ObjId>,
    /// Where a redirection effect sends the damage (CR 614.9), locked in
    /// when the effect was created.
    pub redirect: Option<Target>,
}

/// A "can't" effect from a resolved spell (CR 614.17), until end of turn.
#[derive(Copy, Clone, Debug)]
pub struct RuleEffect {
    pub kind: RuleKind,
    pub duration: Duration,
}

/// A "can't" effect on one player from a resolved spell (CR 614.17),
/// until end of turn: "Target player can't play lands this turn."
#[derive(Copy, Clone, Debug)]
pub struct PlayerRuleEffect {
    pub player: PlayerId,
    pub kind: PlayerRuleKind,
}

/// A decision taken in the middle of applying an event (CR 616.1: which
/// of several replacement effects applies first). The engine cannot pause
/// there, so it records the question, lets a default stand, and the
/// `Engine` rolls the action back and replays it with the answer.
#[derive(Clone, Debug, Default)]
pub struct MidDecision {
    /// Answers to consume on a replay, in order.
    pub answers: Vec<ChoiceId>,
    pub next: usize,
    /// The first unanswered question met during this action.
    pub pending: Option<Decision>,
}

/// An object's current characteristics (CR 109.3), derived from printed
/// values plus continuous effects.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Characteristics {
    pub def: CardRef,
    pub colors: ColorSet,
    pub supertypes: SupertypeSet,
    pub types: TypeSet,
    pub subtypes: SubtypeSet,
    /// Stored narrow to keep `Characteristics` small (it is copied on every
    /// rules query); arithmetic saturates at the `i16` range.
    pub power: i16,
    pub toughness: i16,
    pub keywords: KeywordSet,
    pub abilities: AbilitySet,
}

/// `GameState::targets_of` for an object, from any handle on the pool (so
/// a resolving spell can read its definition while the state changes).
pub fn targets_in<'p>(pool: &'p CardPool, o: &GameObject) -> &'p [TargetSpec] {
    if let Some(a) = o.ability {
        let ab = pool.ability(a.ability);
        if let [crate::card::Effect::Modes(modes)] = ab.effects() {
            return modes
                .get(usize::from(o.cast.mode()))
                .map_or(&[], |m| &m.targets);
        }
        return ab.targets();
    }
    let def = pool.get(o.def);
    match def.modes() {
        Some(modes) => modes
            .get(usize::from(o.cast.mode()))
            .map_or(&[], |m| &m.targets),
        // CR 601.2c: targets only a kicked spell has (or one cast using
        // teamwork: "If this spell was cast using teamwork, instead ...").
        None if o.cast.kicked() || o.cast.teamwork() => def.kicked_cast_targets(),
        None => def.unkicked_cast_targets(),
    }
}

/// `GameState::effects_of` for an object, from any handle on the pool.
pub fn effects_in<'p>(pool: &'p CardPool, o: &GameObject) -> &'p [crate::card::Effect] {
    if let Some(a) = o.ability {
        let ab = pool.ability(a.ability);
        if let [crate::card::Effect::Modes(modes)] = ab.effects() {
            return modes
                .get(usize::from(o.cast.mode()))
                .map_or(&[], |m| &m.effects);
        }
        return ab.effects();
    }
    let def = pool.get(o.def);
    match def.modes() {
        Some(modes) => modes
            .get(usize::from(o.cast.mode()))
            .map_or(&[], |m| &m.effects),
        None => def.spell_ability().map_or(&[], |(_, e)| e),
    }
}

/// Power or toughness into the stored range (saturating).
#[inline]
pub fn pt16(x: i32) -> i16 {
    i16::try_from(x.clamp(i32::from(i16::MIN), i32::from(i16::MAX))).unwrap_or(0)
}

impl Characteristics {
    /// Power as a plain integer.
    #[inline]
    pub fn power(&self) -> i32 {
        i32::from(self.power)
    }

    /// Toughness as a plain integer.
    #[inline]
    pub fn toughness(&self) -> i32 {
        i32::from(self.toughness)
    }

    pub fn is_creature(&self) -> bool {
        self.types.contains(CardType::Creature)
    }

    pub fn is_land(&self) -> bool {
        self.types.contains(CardType::Land)
    }

    pub fn is_planeswalker(&self) -> bool {
        self.types.contains(CardType::Planeswalker)
    }

    pub fn is_battle(&self) -> bool {
        self.types.contains(CardType::Battle)
    }

    pub fn has(&self, k: Keyword) -> bool {
        self.keywords.contains(k)
    }

    /// Has this subtype; a changeling has every creature type (CR 702.73a).
    pub fn has_subtype(&self, s: crate::card::Subtype) -> bool {
        self.subtypes.contains(&s)
            || (s.is_creature_type()
                && (self.has(Keyword::Changeling)
                    || self
                        .subtypes
                        .contains(&crate::card::Subtype::ALL_CREATURE_TYPES)))
    }

    /// Whether this object has protection from something with `source`'s
    /// colors (CR 702.16b–e). `opposing`: the source's controller is an
    /// opponent of this object's ("protection from creatures your opponents
    /// control").
    #[inline]
    pub fn protected_from(
        &self,
        source: &Characteristics,
        opposing: bool,
        pool: &crate::card::CardPool,
    ) -> bool {
        self.keywords
            .intersects(crate::card::KeywordSet::PROTECTION)
            && (crate::mana::Color::ALL
                .into_iter()
                .any(|col| self.has(Keyword::protection_from(col)) && source.colors.contains(col))
                || self.has(Keyword::ProtectionFromFilter)
                    && self.abilities.iter().any(|a| {
                        matches!(pool.ability(*a),
                            crate::card::AbilityDef::Protection(f) if f.matches_characteristics(source)
                                && f.who.is_none_or(|w| (w == crate::card::Who::Opponents) == opposing))
                    }))
    }

    /// No keyword or other abilities (Muraganda Petroglyphs).
    pub fn has_no_abilities(&self) -> bool {
        self.keywords.is_empty() && self.abilities.is_empty()
    }
}

/// Derived characteristics for one `GameState::version`.
pub enum Derived {
    /// Nothing modifies anything: every object has its printed characteristics.
    Printed,
    /// Characteristics of battlefield objects; others are printed.
    Battlefield(IdMap<Characteristics>),
}

/// Cache of derived characteristics, valid for one `GameState::cversion`.
/// Cloning a state yields an empty cache; it refills on first use.
#[derive(Default)]
pub struct DerivedCache(
    pub(crate) RefCell<Option<(u64, Derived)>>,
    /// Cache hits, for sampling the debug consistency check.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(crate) std::cell::Cell<u32>,
);

impl Clone for DerivedCache {
    fn clone(&self) -> Self {
        DerivedCache::default()
    }
}

impl std::fmt::Debug for DerivedCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DerivedCache")
    }
}

/// A zone change made by a resolving spell or ability, or by paying its
/// costs: the object's id before (its last known information, CR 608.2h)
/// and after.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Moved {
    pub old: ObjId,
    pub new: ObjId,
    pub from: Zone,
    pub to: Zone,
    pub owner: PlayerId,
}

/// What a resolution did "this way" (CR 608.2c).
#[derive(Clone, Debug, Default)]
pub struct ThisWayLog {
    pub moved: Vec<Moved>,
    /// Life players lost.
    pub life_lost: u32,
    /// Damage dealt (CR 120.3), and of it the excess (CR 120.4a).
    pub damage: u32,
    pub excess: u32,
    /// "The damage can't be prevented" (CR 615.12): this resolution's
    /// damage.
    pub unpreventable: bool,
    /// "Excess damage is dealt to that creature's controller instead."
    pub excess_to_controller: bool,
    /// The permanents dealt damage, in order ("a creature dealt damage
    /// this way").
    pub damaged: Vec<ObjId>,
    /// The objects the most recent group recipient denoted ("Untap
    /// them", `Recipient::ThoseObjects`).
    pub group: Vec<ObjId>,
}

/// A card a permanent exiled (`GameState::linked_exiles`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LinkedExile {
    /// The permanent whose ability exiled it, as the object it was on the
    /// battlefield (its leaves-the-battlefield trigger has the same id).
    pub source: ObjId,
    /// The card in exile.
    pub card: ObjId,
    /// Exiled "until" the source leaves the battlefield (CR 610.3).
    pub until: bool,
}

impl Moved {
    /// Whether this zone change was `verb` ("exiled", "discarded").
    pub fn is(&self, verb: crate::card::ThisWay) -> bool {
        use crate::card::ThisWay;
        match verb {
            ThisWay::Exiled => self.to == Zone::Exile && self.from != Zone::Exile,
            ThisWay::Sacrificed | ThisWay::Destroyed => {
                self.from == Zone::Battlefield && self.to != Zone::Battlefield
            }
            ThisWay::Discarded => self.from == Zone::Hand && self.to != Zone::Hand,
            ThisWay::Milled => self.from == Zone::Library && self.to == Zone::Graveyard,
            ThisWay::PutInHand => self.to == Zone::Hand && self.from != Zone::Hand,
            ThisWay::Tapped => self.from == Zone::Battlefield && self.to == Zone::Battlefield,
            ThisWay::Shuffled => self.to == Zone::Library && self.from != Zone::Library,
        }
    }
}

/// Last-known information about an object that left its zone (CR 113.7a).
#[derive(Clone, Debug)]
pub struct LastKnown {
    pub object: GameObject,
    pub characteristics: Characteristics,
    /// The object it became in its new zone (CR 400.7), if it moved rather
    /// than ceased to exist.
    pub moved_to: Option<ObjId>,
    /// It was an attacking or a blocking creature as it left the
    /// battlefield ("If it was attacking", CR 608.2h).
    pub attacking: bool,
    pub blocking: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum GameResult {
    Win { winner: PlayerId },
    Draw,
}

/// Where the engine is between decisions.
#[derive(Clone, Debug)]
pub enum Flow {
    /// Mulligan round: each player who has not kept decides in turn order.
    Mulligan {
        /// Players who have kept (permanently).
        kept: [bool; 2],
        /// Players who chose to mulligan this round.
        mulled: [bool; 2],
        /// Players who have decided this round.
        decided: [bool; 2],
        current: PlayerId,
    },
    /// Players who mulliganed put cards on the bottom, one decision each.
    Bottoming {
        current: PlayerId,
        remaining: u8,
    },
    /// Before the first turn (CR 103.6a): each player in turn may begin the
    /// game with "If this card is in your opening hand" cards on the
    /// battlefield, one question per card; `asked` are those decided.
    OpeningHand {
        asked: Vec<ObjId>,
    },
    /// Perform the turn-based actions of the current step.
    BeginStep,
    /// `priority` holds priority and must act.
    Priority,
    /// Choosing targets for a spell being cast (CR 601.2c) or an ability
    /// being activated (CR 602.2b); `sacrifice` is the permanent chosen for
    /// a "sacrifice a Goblin" cost, paid once targets are chosen.
    Casting {
        spell: ObjId,
        next_target: usize,
        sacrifice: Option<ObjId>,
        /// The card chosen for a "discard a card" cost, once chosen.
        discard: Option<ObjId>,
    },
    DeclareAttackers {
        chosen: Vec<ObjId>,
        incremental: bool,
    },
    DeclareBlockers {
        chosen: Vec<(ObjId, ObjId)>,
        incremental: bool,
    },
    /// Combat damage assignments still needing a player's division.
    AssignDamage {
        queue: Vec<ObjId>,
        assigned: Vec<DamageAssignment>,
    },
    /// Cleanup discard to maximum hand size (CR 514.1).
    Discarding,
    /// `player` controls several triggered abilities waiting to go on the
    /// stack and chooses their order (CR 603.3b).
    OrderTriggers {
        player: PlayerId,
    },
    /// Choosing the mode of a modal spell being cast (CR 601.2b), before
    /// its targets.
    ChoosingMode {
        spell: ObjId,
        /// An activated ability's: what its cost chose (as `ChoosingX`).
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    },
    /// Choosing X for a spell being cast (CR 601.2b), after the mode and
    /// before targets, or for an activated ability with {X} in its cost
    /// (CR 602.2b), with the permanent or card its cost chose.
    ChoosingX {
        spell: ObjId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    },
    /// Turning a face-down permanent face up for a morph cost that
    /// discards a card or returns a permanent (CR 702.37e): the player
    /// chooses which.
    TurningFaceUp {
        obj: ObjId,
    },
    /// Choosing whether to pay a spell's kicker (CR 702.33), after the mode
    /// and before X.
    ChoosingKicker {
        spell: ObjId,
    },
    /// Choosing whether to promise a gift (CR 702.174a), after the kicker.
    ChoosingGift {
        spell: ObjId,
    },
    /// Choosing which of a spell's two additional costs to pay
    /// ("sacrifice a creature or pay {3}", CR 601.2b), after the mode.
    ChoosingAdditional {
        spell: ObjId,
    },
    /// Choosing whether to pay a spell's buyback cost (CR 702.27), after
    /// kicker and before X.
    ChoosingBuyback {
        spell: ObjId,
    },
    /// Choosing whether to pay a spell's offspring cost (CR 702.175a),
    /// after buyback and before X.
    ChoosingOffspring {
        spell: ObjId,
    },
    /// Choosing whether to bargain a spell, and what to sacrifice (CR
    /// 702.166a), after offspring and before X.
    ChoosingBargain {
        spell: ObjId,
    },
    /// Choosing whether to pay a spell's casualty cost, and what to
    /// sacrifice (CR 702.153a), after bargain.
    ChoosingCasualty {
        spell: ObjId,
    },
    /// Choosing how many times to pay a spell's multikicker cost (CR
    /// 702.33c), where a kicker is chosen.
    ChoosingMultikicker {
        spell: ObjId,
    },
    /// Choosing how many times to pay a spell's replicate cost (CR
    /// 702.56a), after offspring.
    ChoosingReplicate {
        spell: ObjId,
    },
    /// Choosing whether to cast a spell using teamwork, after casualty.
    ChoosingTeamwork {
        spell: ObjId,
    },
    /// Choosing whether to pay a spell's optional additional cost, after
    /// teamwork.
    ChoosingOptionalCost {
        spell: ObjId,
    },
    /// Choosing whether to conspire a spell (CR 702.78a), after bargain.
    ChoosingConspire {
        spell: ObjId,
    },
    /// Choosing cards to splice onto an Arcane spell (CR 702.47a), one at
    /// a time, after bargain and before X.
    ChoosingSplice {
        spell: ObjId,
    },
    /// Attackers declared; each chooses the player or planeswalker it
    /// attacks (CR 508.1a) when the defender controls a planeswalker.
    AttackTargets {
        chosen: Vec<(ObjId, Target)>,
        remaining: Vec<ObjId>,
    },
    /// Attackers declared; the attacking player forms bands (CR 702.22c):
    /// `band` is the one being formed, its leader (with banding) first;
    /// `led` the attackers that have led one.
    FormBands {
        band: Vec<ObjId>,
        led: Vec<ObjId>,
    },
    /// All players passed with an empty stack: the step ends.
    EndStep,
    GameOver,
}

#[derive(Clone, Debug)]
pub struct DamageAssignment {
    pub source: ObjId,
    pub target: Target,
    pub amount: u32,
}

/// Records of the turn so far that static conditions read, one bit per
/// player unless noted; `TurnLog::default()` as each turn begins.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub struct TurnLog {
    /// Players who gained life ("if you gained life this turn").
    pub gained_life: u8,
    /// Players who descended (CR 700.11): a permanent card was put into
    /// their graveyard from anywhere.
    pub descended: u8,
    /// Players who surveilled (CR 701.25).
    pub surveilled: u8,
    /// Players who had a +1/+1 counter put on a permanent they control.
    pub counter_put: u8,
    /// Players who put a +1/+1 counter on a creature ("if you've put one
    /// or more +1/+1 counters on a creature this turn").
    pub counter_put_by: u8,
    /// Players dealt noncombat damage.
    pub noncombat_damaged: u8,
    /// Players under whose control a face-down creature entered the
    /// battlefield (CR 708.3).
    pub face_down_entered: u8,
    /// Generic mana face-down spells each player casts this turn cost less
    /// ("face-down spells you cast this turn cost {1} less to cast").
    pub face_down_less: [u8; 4],
    /// Creatures that entered the battlefield under each player's control.
    pub creatures_entered: [u8; 4],
    /// Nonland permanents that did (celebration).
    pub nonland_entered: [u8; 4],
    /// The card types among permanents put into a graveyard from the
    /// battlefield, as they last existed.
    pub to_graveyard: crate::card::TypeSet,
    /// Life each player gained in all.
    pub life_gained: [u32; 4],
    /// Players who committed a crime (CR 700.13).
    pub crime: u8,
    /// Players a card left the graveyard of.
    pub left_graveyard: u8,
    /// The card types among cards put into each player's graveyard from
    /// anywhere (tokens are no cards).
    pub cards_to_graveyard: [crate::card::TypeSet; 4],
    /// The card types among permanents each player sacrificed.
    pub sacrificed: [crate::card::TypeSet; 4],
    /// How many permanents each player sacrificed.
    pub sacrificed_n: [u16; 4],
    /// Players who put counters of any kind on a creature.
    pub any_counter_put_by: u8,
    /// Void: a nonland permanent left the battlefield (any player's).
    pub nonland_left: bool,
    /// Void: a spell was cast for its warp cost (CR 702.185a).
    pub warped: bool,
}

/// Day and night (CR 731): neither until something makes it day.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DayNight {
    #[default]
    Neither,
    Day,
    Night,
}

/// A player's dungeon in the command zone (CR 309): the dungeon card's
/// object and the room their venture marker is in (an index into the
/// definition's `rooms`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DungeonMarker {
    pub obj: ObjId,
    pub room: u8,
}

/// Player designations and progress tracks: the monarch (CR 725), the
/// initiative (CR 726), day and night (CR 731), the city's blessing (CR
/// 702.131), speed (CR 702.179), the Ring (CR 701.54), and dungeons (CR
/// 309). Inline and `Copy`, since the state is cloned constantly; `live`
/// stays false until any designation that has triggered abilities exists,
/// so event emission pays one test without them.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub struct Designations {
    /// Any of the monarch, the initiative, speed, or the Ring exists:
    /// their inherent triggered abilities may trigger.
    pub live: bool,
    pub monarch: Option<PlayerId>,
    pub initiative: Option<PlayerId>,
    pub day_night: DayNight,
    /// The previous turn's active player, for the day/night check at the
    /// untap step (CR 502.2).
    pub last_active: Option<PlayerId>,
    /// Players with the city's blessing, one bit each.
    pub blessed: u8,
    /// Players with an enduring story (CR 702.195b), one bit each.
    pub storied: u8,
    /// Each player's speed (CR 702.179b: 0 is "no speed").
    pub speed: [u8; 4],
    /// Players whose speed-increasing ability triggered this turn (CR
    /// 702.179d: "only once each turn"), one bit each.
    pub sped: u8,
    /// How many times the Ring has tempted each player (CR 701.54c).
    pub tempted: [u8; 4],
    /// Each player's Ring-bearer, as chosen (CR 701.54a); it stops being
    /// one as it leaves the battlefield or changes control.
    pub ring_bearer: [Option<ObjId>; 4],
    /// Each player's emblem named The Ring (CR 701.54c), the source of
    /// its triggered abilities.
    pub ring: [Option<ObjId>; 4],
    pub dungeon: [Option<DungeonMarker>; 4],
    /// Dungeons each player has completed (CR 309.7).
    pub completed: [u8; 4],
    /// The command-zone objects standing for the monarch's, the
    /// initiative's, and speed's sourceless triggered abilities (CR 725.2,
    /// 726.2, 702.179d), each made the first time one is needed.
    pub inherent: [Option<ObjId>; 3],
}

impl Designations {
    pub fn blessed(&self, p: PlayerId) -> bool {
        self.blessed & (1 << p.index().min(7)) != 0
    }

    pub fn storied(&self, p: PlayerId) -> bool {
        self.storied & (1 << p.index().min(7)) != 0
    }

    pub fn speed(&self, p: PlayerId) -> u8 {
        self.speed[p.index().min(3)]
    }

    pub fn tempted(&self, p: PlayerId) -> u8 {
        self.tempted[p.index().min(3)]
    }

    pub fn completed(&self, p: PlayerId) -> u8 {
        self.completed[p.index().min(3)]
    }

    pub fn dungeon(&self, p: PlayerId) -> Option<DungeonMarker> {
        self.dungeon[p.index().min(3)]
    }
}

/// An Arcane spell with cards spliced onto it (CR 702.47a): the cards
/// revealed from hand (and their definitions, whose effects the spell
/// has even if the cards leave the hand, CR 702.47b), and all the
/// spell's targets, its own first, then each spliced card's in order.
#[derive(Clone, Debug)]
pub struct Spliced {
    pub spell: ObjId,
    pub cards: SmallVec<[(ObjId, crate::card::CardRef); 2]>,
    pub targets: Vec<crate::card::TargetSpec>,
}

#[derive(Clone)]
// Independent dirty flags and switches, not a state machine.
#[allow(clippy::struct_excessive_bools)]
pub struct GameState {
    pub pool: Arc<CardPool>,
    /// Bumped on every change to the state.
    pub version: u64,
    /// Bumped only on changes that can alter derived characteristics (the
    /// battlefield, counters, continuous effects, attachments, control,
    /// copy effects, graveyards for characteristic-defining abilities).
    /// The characteristics cache is keyed on it, so damage, life, tapping,
    /// and drawing do not invalidate it.
    pub cversion: u64,
    /// Whether anything a state-based action looks at (CR 704.5) may have
    /// changed since the last check: set by `cbump`, damage, life changes,
    /// and drawing from an empty library; cleared by a check that found
    /// nothing to do. Tapping and mana leave it alone, so most priority
    /// passes skip the check.
    pub sba_dirty: bool,
    /// Tokens that left the battlefield and are not yet removed (CR 704.5d).
    pub stray_tokens: Vec<ObjId>,
    /// Emblems in the command zone (CR 114), sources of static abilities.
    pub emblems: Vec<ObjId>,
    /// Planeswalkers whose loyalty ability was activated this turn (CR
    /// 606.3: one per planeswalker per turn).
    pub loyalty_activated: Vec<ObjId>,
    /// "Activate only once each turn" abilities used this turn.
    pub activated_once: Vec<(ObjId, crate::card::AbilityId)>,
    /// Triggered abilities limited each turn (`TurnLimit`): one entry each
    /// time one triggered (`TurnLimit::Triggers`) or its controller did
    /// what it limits (`TurnLimit::Do`) this turn. A new object has a new
    /// id, so it starts fresh (CR 400.7).
    pub limited_this_turn: Vec<(ObjId, crate::card::AbilityId)>,
    /// Abilities of each source resolved this turn, once per resolution
    /// ("if this is the second time this ability has resolved this turn").
    pub resolved_this_turn: Vec<(ObjId, crate::card::AbilityId)>,
    /// While an ability resolves: how many times its source's ability has
    /// resolved this turn, counting this time (0 otherwise).
    pub resolution_number: u8,
    /// While a `TurnLimit::Do` ability resolves: its source and ability,
    /// for its first "you may".
    pub doing_once: Option<(ObjId, crate::card::AbilityId)>,
    /// "Activate only once" abilities used (exhaust, CR 702.177a;
    /// power-up, CR 702.193a): never cleared, since the object a new
    /// zone makes is a new object with a new id (CR 400.7).
    pub activated_ever: Vec<(ObjId, crate::card::AbilityId)>,
    /// Creatures declared as attackers this turn (boast, CR 702.142a);
    /// one put onto the battlefield attacking was never declared.
    pub attackers_this_turn: Vec<ObjId>,
    /// Creatures that attacked a battle this turn (CR 310.5: "as long as
    /// it attacked a battle this turn").
    pub battle_attackers_this_turn: Vec<ObjId>,
    /// The creatures each player attacked with during their last turn
    /// ("if it attacked during your last turn").
    pub attacked_last_turn: Vec<(PlayerId, ObjId)>,
    /// Creatures declared as blockers this turn ("that blocked this
    /// turn").
    pub blockers_this_turn: Vec<ObjId>,
    /// Sources that dealt damage this turn ("that dealt damage this
    /// turn"), each once.
    pub dealt_damage_this_turn: Vec<ObjId>,
    /// Permanents that entered the battlefield this turn, with the player
    /// they entered under the control of ("another Knight entered the
    /// battlefield under your control this turn").
    pub entered_this_turn: Vec<(PlayerId, ObjId)>,
    /// (source, creature) pairs: the source dealt damage to the creature
    /// this turn ("a creature dealt damage by this creature this turn"),
    /// each once.
    pub damaged_by_this_turn: Vec<(ObjId, ObjId)>,
    /// Cards encoded on creatures (CR 702.99b): the exiled card and the
    /// creature, each as the object it was when encoded; an entry lapses
    /// once either has moved (702.99c).
    pub encoded: Vec<(ObjId, ObjId)>,
    /// Phased-out permanents (CR 702.26), with the player they phased out
    /// under and whether they phased out with what they're attached to.
    pub phased_out: Vec<PhasedOut>,
    /// Mounts saddled this turn (CR 702.171b: until end of turn, or until
    /// it leaves the battlefield, which makes it a new object).
    pub saddled: Vec<ObjId>,
    /// The first timestamp of this turn: a permanent whose timestamp is
    /// not older entered this turn (power-up, CR 702.193a), as a
    /// permanent's timestamp is the time it entered (CR 613.7e).
    pub turn_started_at: Timestamp,
    /// X as chosen for spells on the stack (CR 107.3), kept off
    /// `GameObject` so the hot struct stays at 120 bytes.
    pub x_values: Vec<(ObjId, u32)>,
    /// X of the spell each permanent was as it resolved, for its own
    /// enters triggers (CR 107.3m; the permanent's own X is 0). Cleared
    /// at cleanup: such a trigger resolves the turn it triggered.
    pub entered_x: Vec<(ObjId, u32)>,
    /// Ninjutsu abilities on the stack (CR 702.49c): the card to enter
    /// attacking, and what the creature returned for the cost was
    /// attacking.
    pub ninjutsu: Vec<(ObjId, Target)>,
    /// Spells being cast for their harmonize cost (CR 702.180a), the
    /// creature chosen to tap, and its power then: the generic mana the
    /// cost is reduced by.
    pub harmonized: Vec<(ObjId, ObjId, u32)>,
    /// Bargained spells being cast (CR 702.166a) and the permanent to
    /// sacrifice as their costs are paid.
    pub bargains: Vec<(ObjId, ObjId)>,
    /// How spells and abilities on the stack divide damage or counters
    /// among their targets (CR 601.2d), in target order from the variable
    /// group's first target; kept off `GameObject` like `x_values`.
    pub divisions: Vec<(ObjId, Vec<u32>)>,
    /// Creatures paired by soulbond (CR 702.95b), each pair once.
    pub pairs: Vec<(ObjId, ObjId)>,
    /// Spells with cards spliced onto them (CR 702.47a), kept off
    /// `GameObject` like `x_values`.
    pub splices: Vec<Spliced>,
    /// Goaded creatures and who goaded them (CR 701.15a), until that
    /// player's next turn; the creature's "attacks each combat if able"
    /// is an ordinary until-your-next-turn effect, this list the
    /// requirement to attack a player other than the goader (701.15b).
    pub goads: Vec<(ObjId, PlayerId)>,
    pub turn_number: u32,
    /// Spells each player has cast this turn, and the most any one player
    /// cast last turn ("if no spells were cast last turn", werewolves).
    pub spells_this_turn: [u8; 4],
    /// Generic mana the next spell each player casts this turn costs less
    /// ("the next spell you cast this turn costs {1} less to cast"), by
    /// player index like `spells_this_turn`; zero in most games.
    pub next_spell_less: [u16; 4],
    /// Cards each player drew this turn ("your second card each turn").
    pub drawn_this_turn: [u8; 4],
    /// Total mana each player has spent to cast spells this turn (expend,
    /// CR 700.14), and what it was before their latest spell was paid
    /// for: "Whenever you expend 4" triggers on the spell that crosses 4.
    pub mana_spent_this_turn: [u16; 4],
    pub mana_spent_before_cast: [u16; 4],
    /// Life each player has lost this turn ("the life you've lost this
    /// turn"; CR 119.3).
    pub life_lost_this_turn: [u32; 4],
    /// Players who declared an attacker this turn (raid), one bit each.
    pub attacked_this_turn: u8,
    /// "Target creature blocks this creature this turn if able": (blocker,
    /// attacker) requirements for this turn (CR 509.1c).
    pub block_demands: Vec<(ObjId, ObjId)>,
    /// "Target creature can't block this creature this turn": (blocker,
    /// attacker) restrictions for this turn (CR 509.1b).
    pub block_bans: Vec<(ObjId, ObjId)>,
    /// Permanents that can't be regenerated this turn (CR 701.15).
    pub no_regen: Vec<ObjId>,
    /// Creatures that assign no combat damage this turn (CR 510.1a).
    pub no_combat_damage: Vec<ObjId>,
    /// Players who had a land enter under their control this turn
    /// (landfall), one bit each.
    pub lands_entered_this_turn: u8,
    /// Players who had an artifact enter under their control this turn,
    /// one bit each.
    pub artifacts_entered_this_turn: u8,
    /// Creatures put into a graveyard from the battlefield this turn
    /// (morbid).
    pub creatures_died_this_turn: u16,
    /// Those creatures as they last existed (CR 700.4): the object they
    /// were, their controller, and their characteristics, for "for each
    /// Zubera that died this turn". Empty, unallocated, on most turns.
    pub died_this_turn: Vec<(ObjId, PlayerId, Characteristics)>,
    /// Players dealt damage this turn (bloodthirst), one bit each.
    pub damaged_this_turn: u8,
    /// Players who controlled an Assassin creature as it dealt combat
    /// damage to a player this turn (freerunning, CR 702.173a), one bit
    /// each; kept only with a freerunning card in the pool.
    pub assassin_damage: u8,
    /// Players who lost life this turn ("if an opponent lost life this
    /// turn"), one bit each.
    pub lost_life_this_turn: u8,
    /// Players a permanent left the battlefield under the control of this
    /// turn (revolt), one bit each.
    pub left_this_turn: u8,
    /// Players who discarded a card this turn, one bit each.
    pub discarded_this_turn: u8,
    /// Cards with mayhem discarded this turn, as the graveyard objects
    /// they became (castable for their mayhem cost until the turn ends).
    pub mayhem_discarded: Vec<ObjId>,
    /// Graveyard cards given flashback until end of turn
    /// (`Effect::GrantFlashback`), castable for their mana cost while
    /// they stay there.
    pub granted_flashback: Vec<ObjId>,
    /// Graveyard cards given harmonize until end of turn, castable for
    /// their mana cost (as their harmonize cost) while they stay there.
    pub granted_harmonize: Vec<ObjId>,
    /// The spells cast this turn, by caster ("if you've cast a noncreature
    /// spell this turn"); inline, so copying the state rarely allocates.
    pub cast_this_turn: SmallVec<[(PlayerId, crate::card::CardRef); 4]>,
    /// More of what happened this turn, reset as each turn begins.
    pub turn_log: TurnLog,
    /// The monarch, the initiative, day and night, speed, the Ring, and
    /// dungeons.
    pub desig: Designations,
    /// Casting from exile's bookkeeping, boxed: most games never need it,
    /// and the state is copied for every mid-event snapshot.
    pub exile_casts: Option<Box<ExileCasts>>,
    /// Suspended cards in exile, so upkeeps skip the scan without any.
    pub suspended_count: u32,
    /// Cards in graveyards with a triggered ability that works there
    /// (CR 113.6k), so trigger detection skips the graveyards without any.
    pub graveyard_trigger_cards: u32,
    /// Cards with dredge in graveyards (CR 702.52): with any, a draw may
    /// ask whether to dredge instead.
    pub dredge_cards: u32,
    /// Cards in the game with an "As this enters, choose ..." ability:
    /// with any, every way onto the battlefield may ask a question.
    pub enters_choice_cards: u32,
    /// How many times a permanent has become prepared this game (CR
    /// 722.3a), never decreased: with none, priority skips looking for
    /// prepared permanents.
    pub prepared_ever: u32,
    pub spells_last_turn: [u8; 4],
    pub active_player: PlayerId,
    pub starting_player: PlayerId,
    pub step: Step,
    pub priority: PlayerId,
    pub consecutive_passes: u8,
    pub players: Vec<PlayerState>,
    pub objects: SlotMap<ObjId, GameObject>,
    pub battlefield: Vec<ObjId>,
    /// Index 0 is the bottom; the last element is the top.
    pub stack: Vec<ObjId>,
    pub combat: Option<CombatState>,
    pub effects: Vec<ContinuousEffect>,
    /// Replacement and prevention effects from resolved spells (CR 614.3).
    pub replacements: Vec<ReplacementEffect>,
    /// "Can't" effects from resolved spells (CR 614.17).
    pub rules: Vec<RuleEffect>,
    /// "Target player can't play lands this turn": until end of turn.
    pub player_rules: Vec<PlayerRuleEffect>,
    /// Battlefield objects whose definition has a replacement or
    /// prevention ability, so that events on plain boards cost nothing.
    pub replacement_sources: u32,
    /// Battlefield objects whose definition has a control-changing static
    /// ability (Control Magic), and stored effects with a control part;
    /// `sync_control` does nothing while both are zero.
    pub control_sources: u32,
    pub control_effects: u32,
    /// A control source or effect just went away: the next `sync_control`
    /// must run even though the counts say nothing applies.
    pub control_dirty: bool,
    /// Legendary permanents on the battlefield, for the legend rule's
    /// snapshot gate (see `may_need_mid_decision`).
    pub legendary_count: u32,
    /// Battlefield objects whose definition has an activated ability, so
    /// that listing a player's options costs nothing extra without any.
    pub activated_sources: u32,
    /// Battlefield objects whose definition has a life-dependent CDA, so
    /// that life changes bump `cversion` only when one is in play.
    pub life_cda_sources: u32,
    /// Battlefield permanents with a static ability whose condition looks
    /// at tapped permanents (see `DefFlags::TAP_STATIC`).
    pub tap_static_sources: u32,
    /// Battlefield permanents with a static ability changing spell costs.
    pub cost_change_sources: u32,
    /// Battlefield permanents with a restricted mana ability (CR 106.6),
    /// so payment looks for such sources only when one is in play.
    pub restricted_mana_sources: u32,
    /// Battlefield permanents with a triggered mana ability (CR 605.1b,
    /// `DefFlags::MANA_TRIGGER`): tapping for mana and payment look for
    /// them only then.
    pub mana_trigger_sources: u32,
    /// Battlefield permanents with a static ability reading this turn's
    /// history or step (`DefFlags::TURN_STATIC`).
    pub turn_static_sources: u32,
    /// Battlefield permanents with a state trigger
    /// (`DefFlags::STATE_TRIGGER`, CR 603.8).
    pub state_trigger_sources: u32,
    /// Battlefield permanents with ascend, start your engines!, or
    /// becoming day as they enter (`DefFlags::MARKS`): state-based
    /// actions look for them only then.
    pub mark_sources: u32,
    /// Permanents with banding (CR 702.22): combat looks for bands only
    /// while there are some.
    pub banding_sources: u32,
    /// Battlefield permanents with an attack or block condition
    /// (`DefFlags::ATTACK_RULE`): declarations check them only then.
    pub attack_rule_sources: u32,
    /// Face-down permanents on the battlefield, so listing options skips
    /// the turn-face-up check without any.
    pub face_down_count: u32,
    /// Room permanents on the battlefield (CR 709.5): priority looks for
    /// locked doors to unlock only when there are any.
    pub room_count: u32,
    /// Permanents on the battlefield that let their controller play from
    /// the top of the library or reveal it (`DefFlags::TOP_PLAY`):
    /// priority and the views look only when there are any.
    pub top_play_sources: u32,
    /// Sagas with chapter abilities on the battlefield (CR 714): the
    /// precombat main phase and state-based actions look for them only
    /// when there are any.
    pub saga_count: u32,
    /// Battlefield permanents with a player-level static ability
    /// (`AbilityDef::PlayerStatic`): land plays, player targeting, and
    /// casting look for them only when there are any.
    pub player_static_sources: u32,
    /// A cast just undone (CR 726) and the state version then: it is not
    /// offered again until something changes, so an agent can't loop on it.
    pub undone_cast: Option<(ObjId, u64)>,
    /// Battlefield permanents with a static ability granting abilities,
    /// plus grants made this turn by effects: while nonzero, any permanent
    /// may have activated or triggered abilities its definition lacks.
    pub grant_sources: u32,
    pub eot_grants: u32,
    /// Grants made with a duration longer than the turn ("until your next
    /// turn", "for as long as ..."): `eot_grants` starts each turn here.
    pub lasting_grants: u32,
    /// The duration a resolving `Effect::Lasting` gives the effects it
    /// creates in place of "until end of turn" (CR 611.2b).
    pub lasting: Option<crate::card::Lasting>,
    /// Effects whose duration is linked to an object
    /// (`Duration::linked`): while nonzero the state-based actions end
    /// those whose object left, untapped, or changed control.
    pub linked_durations: u32,
    pub mid: MidDecision,
    pub lki: IdMap<LastKnown>,
    /// What the resolving spell or ability has done so far, for "this
    /// way" (`ThisWayCond`); `None` outside a resolution, so zone changes
    /// then record nothing.
    pub this_way: Option<ThisWayLog>,
    /// The sacrifices and discards paid as costs of spells and abilities
    /// on the stack (CR 601.2h, 602.2b), keyed by the stack object: what
    /// "the sacrificed creature" refers to as it resolves.
    pub paid_costs: Vec<(ObjId, Vec<Moved>)>,
    /// Cards exiled by a permanent's ability for its linked ability's "the
    /// exiled card" (CR 607.2a), or until it leaves the battlefield (CR
    /// 610.3); see `Effect::ExileLinked`. Empty on most boards: zone
    /// changes check it only when it isn't.
    pub linked_exiles: Vec<LinkedExile>,
    /// The mana spent to cast spells whose text asks about it (CR
    /// 601.2h; `CardDef::spent`), keyed by the spell and then by the
    /// permanent it became ("if {B} was spent to cast it"). Kept until the
    /// turn ends, for intervening "if"s that resolve after the permanent
    /// left.
    pub spent_mana: Vec<(ObjId, SpentMana)>,
    /// While a payment is being recorded, what it has spent so far.
    pub spending: Option<SpentMana>,
    /// Battlefield permanents with a condition on the mana spent to cast
    /// the spell that triggers them (`DefFlags::watches_spent`): while
    /// nonzero, every cast records what was spent.
    pub spent_watchers: u32,
    /// Battlefield permanents with valiant (`DefFlags::has_valiant`):
    /// while nonzero, targeting records `targeted_by_controller`.
    pub valiant_watchers: u32,
    /// Permanents that became the target of a spell or ability their
    /// controller controlled this turn (valiant's "for the first time
    /// each turn"), recorded only while `valiant_watchers` is nonzero.
    pub targeted_by_controller: Vec<ObjId>,
    /// Triggered abilities waiting to be put on the stack (CR 603.3).
    pub pending_triggers: Vec<PendingTrigger>,
    /// Delayed triggered abilities waiting for their event (CR 603.7).
    pub delayed_triggers: Vec<DelayedTrigger>,
    /// Paradigm spells that have resolved (CR 702.192a): their controller,
    /// the card, and the exiled card that is the source of the delayed
    /// trigger copying it (the copy is made even if that card has left
    /// exile).
    pub paradigms: Vec<(PlayerId, crate::card::CardRef, ObjId)>,
    /// Battlefield objects whose definition has a triggered ability, so
    /// that events on trigger-free boards cost nothing to detect over.
    pub trigger_sources: u32,
    /// Set during a batch of simultaneous zone changes: the battlefield as
    /// it was before the batch (CR 603.10a).
    pub lookback: Option<Vec<Lookback>>,
    pub rng: ChaCha8Rng,
    pub next_timestamp: Timestamp,
    pub flow: Flow,
    /// Set when a cleanup step must repeat (CR 514.3a).
    pub cleanup_repeat: bool,
    /// Set by a resolving "End the turn." (CR 724.1): once the resolution
    /// is over, the game skips to the cleanup step.
    pub end_turn: bool,
    /// Extra turns to come (CR 500.7), the next last: each player takes
    /// them directly after the current turn, the most recently created
    /// first.
    pub extra_turns: Vec<PlayerId>,
    /// The controller of the spell or ability whose effect is being
    /// applied: who puts counters it puts ("Whenever you put one or more
    /// +1/+1 counters on a creature", CR 122.6).
    pub effect_controller: Option<PlayerId>,
    /// While extra turns run: whose regular turn comes after them.
    pub resume_player: Option<PlayerId>,
    /// Additional phases added after the current phase (CR 500.8), each
    /// by the step that begins it (`BeginningOfCombat` or
    /// `PostcombatMain`, CR 505.1a), in the order they come.
    pub extra_phases: Vec<Step>,
    /// While additional phases run: the step the turn goes on with after
    /// them (what would have followed the phase they were added after).
    pub resume_step: Option<Step>,
    /// Combat phases begun this turn ("if it's the first combat phase of
    /// the turn").
    pub combats_this_turn: u8,
    pub result: Option<GameResult>,
    pub log: Vec<Event>,
    pub logging: bool,
    pub cache: DerivedCache,
    /// Each player's untapped mana sources, keyed by (`cversion`,
    /// `tap_epoch`, turn): priority listings ask for them again and again
    /// with nothing changed.
    pub mana_cache: std::cell::RefCell<[(u64, u32, u32, ManaSources); 2]>,
    /// Bumped on every tap and untap (which leave `cversion` alone).
    pub tap_epoch: u32,
}

impl GameState {
    pub fn num_players(&self) -> usize {
        self.players.len()
    }

    pub fn player(&self, p: PlayerId) -> &PlayerState {
        &self.players[p.index()]
    }

    pub fn player_mut(&mut self, p: PlayerId) -> &mut PlayerState {
        &mut self.players[p.index()]
    }

    /// The next player in turn order (CR 101.4).
    pub fn next_player(&self, p: PlayerId) -> PlayerId {
        PlayerId(u8::try_from((p.index() + 1) % self.num_players()).unwrap_or(0))
    }

    /// The single opponent in a two-player game.
    pub fn opponent(&self, p: PlayerId) -> PlayerId {
        self.next_player(p)
    }

    /// A battle's protector (CR 310.9): a Siege's is its controller's
    /// opponent (CR 310.12a), another battle's its controller (CR
    /// 310.9a). With two players the choice is forced, and a change of
    /// control re-chooses it the same way (CR 310.11), so it is derived.
    pub fn protector(&self, id: ObjId) -> Option<PlayerId> {
        let o = self.objects.get(id)?;
        let c = self.characteristics(id)?;
        if !c.is_battle() {
            return None;
        }
        Some(if c.has_subtype(crate::card::Subtype::SIEGE) {
            self.opponent(o.controller)
        } else {
            o.controller
        })
    }

    /// Whether a creature matching `f` attacked under `p`'s control this
    /// turn ("if you attacked with a Spacecraft this turn"), judged as it
    /// is or as it last existed.
    pub fn attacked_with(&self, p: PlayerId, f: &crate::card::ObjFilter) -> bool {
        self.attackers_this_turn.iter().any(|id| {
            let (controller, c) = match self.objects.get(*id) {
                Some(o) => (o.controller, self.characteristics(*id)),
                None => match self.lki.get(id) {
                    Some(l) => (l.object.controller, Some(l.characteristics)),
                    None => return false,
                },
            };
            controller == p && c.is_some_and(|c| f.matches(*id, controller, &c, None, Some(p)))
        })
    }

    /// Whether a permanent matching `f` entered the battlefield under
    /// `p`'s control this turn, judged as it is or as it last existed
    /// ("another Knight entered the battlefield under your control this
    /// turn"; `Relation::Other` in `f` excludes `source`).
    pub fn entered_with(&self, p: PlayerId, f: &crate::card::ObjFilter, source: ObjId) -> bool {
        self.entered_this_turn.iter().any(|&(controller, id)| {
            let c = match self.objects.get(id) {
                Some(_) => self.characteristics(id),
                None => self.lki.get(&id).map(|l| l.characteristics),
            };
            controller == p
                && c.is_some_and(|c| f.matches(id, controller, &c, Some(source), Some(p)))
        })
    }

    /// Unlocked doors among Rooms `p` controls (CR 709.5j).
    pub fn unlocked_doors(&self, p: PlayerId) -> u32 {
        if self.room_count == 0 {
            return 0;
        }
        self.battlefield
            .iter()
            .filter_map(|id| {
                let o = &self.objects[*id];
                (o.controller == p).then_some(())?;
                let doors = self.pool.get(o.def).doors.as_deref()?;
                Some(u32::from(doors.unlocked(o.def)[0]) + u32::from(doors.unlocked(o.def)[1]))
            })
            .sum()
    }

    pub fn obj(&self, id: ObjId) -> Option<&GameObject> {
        self.objects.get(id)
    }

    pub fn def_of(&self, id: ObjId) -> Option<&CardDef> {
        self.objects.get(id).map(|o| self.pool.get(o.def))
    }

    pub fn name_of(&self, id: ObjId) -> &str {
        self.def_of(id).map_or("?", |d| d.name.as_str())
    }

    pub fn tick(&mut self) -> Timestamp {
        let t = self.next_timestamp;
        self.next_timestamp += 1;
        t
    }

    pub fn bump(&mut self) {
        self.version += 1;
    }

    /// Combat status changed: refresh characteristics only if a static
    /// ability on the battlefield looks at attacking or blocking.
    pub fn combat_bump(&mut self) {
        self.version += 1;
        if self
            .battlefield
            .iter()
            .any(|id| self.pool.get(self.objects[*id].def).combat_static)
        {
            self.cversion += 1;
        }
    }

    /// A change that can alter derived characteristics (see `cversion`).
    pub fn cbump(&mut self) {
        self.version += 1;
        self.cversion += 1;
        self.sba_dirty = true;
    }

    /// A change a state-based action may act on that does not alter
    /// characteristics (damage, life, a draw from an empty library).
    pub fn sba_touch(&mut self) {
        self.sba_dirty = true;
    }

    /// X of a spell on the stack (CR 107.3); 0 if none was chosen.
    pub fn x_of(&self, spell: ObjId) -> u32 {
        self.x_values
            .iter()
            .find(|(id, _)| *id == spell)
            .map_or(0, |(_, x)| *x)
    }

    /// The division a stack object announced (CR 601.2d); empty if none.
    pub fn division_of(&self, id: ObjId) -> Vec<u32> {
        if self.divisions.is_empty() {
            return Vec::new();
        }
        self.divisions
            .iter()
            .find(|(s, _)| *s == id)
            .map(|(_, d)| d.clone())
            .unwrap_or_default()
    }

    /// Record an event: triggered abilities that watch it trigger (CR 603.2),
    /// then it is logged.
    pub fn emit(&mut self, event: Event) {
        // Cheap test first: nothing on the battlefield (or just leaving it)
        // can have a triggered ability on most boards.
        let may_trigger = event.watch_bit() != 0
            && (self.trigger_sources > 0
                || self.graveyard_trigger_cards > 0
                || self.lookback.is_some()
                || self.grant_sources > 0
                || self.eot_grants > 0)
            || ((!self.delayed_triggers.is_empty() || self.suspended_count > 0)
                && matches!(event, Event::StepBegin { .. }))
            // "Whenever a creature blocks this turn": a delayed trigger
            // may watch any event (rare: only while one waits).
            || (!self.delayed_triggers.is_empty() && event.watch_bit() != 0)
            || matches!(&event, Event::ZoneChange { def, .. } if self.pool.get(*def).has_triggers())
            // "When you cycle this card": the card itself watches.
            || matches!(&event, Event::Cycled { def, .. } if self.pool.get(*def).has_triggers())
            // "When you cast this spell": the spell itself watches.
            || matches!(&event, Event::Cast { def, .. } if self.pool.get(*def).may_watch(&event));
        if may_trigger {
            self.detect_triggers(&event);
        }
        // The monarch's, the initiative's, speed's, and the Ring's own
        // triggered abilities, once any of them exists.
        if self.desig.live {
            self.designation_triggers(&event);
        }
        if self.logging {
            self.log.push(event);
        }
    }

    /// The object `id` is now, following zone changes (CR 400.7), if it
    /// still exists.
    pub fn current_object(&self, mut id: ObjId) -> Option<ObjId> {
        for _ in 0..8 {
            if self.objects.contains_key(id) {
                return Some(id);
            }
            id = self.lki.get(&id)?.moved_to?;
        }
        None
    }

    /// Name of an object, including one that has left the game.
    pub fn name_of_or_lki(&self, id: ObjId) -> &str {
        let def = self
            .objects
            .get(id)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&id).map(|l| l.object.def));
        def.map_or("?", |d| self.pool.get(d).name.as_str())
    }

    /// Targets a stack object needs: a spell's cast targets (of its chosen
    /// mode) or an ability's.
    pub fn targets_of(&self, id: ObjId) -> &[TargetSpec] {
        // CR 702.47a: a spliced spell has the spliced cards' targets too.
        if !self.splices.is_empty()
            && let Some(s) = self.splices.iter().find(|s| s.spell == id)
        {
            return &s.targets;
        }
        self.objects
            .get(id)
            .map_or(&[], |o| targets_in(&self.pool, o))
    }

    /// The modes a stack object chooses among: a modal spell's, or a modal
    /// triggered ability's.
    pub fn modes_of(&self, id: ObjId) -> Option<&[crate::card::Mode]> {
        let o = self.objects.get(id)?;
        if let Some(a) = o.ability {
            return match self.pool.ability(a.ability).effects() {
                [crate::card::Effect::Modes(modes)] => Some(modes),
                _ => None,
            };
        }
        self.pool.get(o.def).modes()
    }

    /// For the chosen "both" mode (CR 700.2): from which effect, and
    /// which target, the second mode's part starts.
    pub fn mode_split_of(&self, id: ObjId) -> Option<(usize, usize)> {
        let o = self.objects.get(id)?;
        let m = self.modes_of(id)?.get(usize::from(o.cast.mode()))?;
        m.split.map(|(e, t)| (usize::from(e), usize::from(t)))
    }

    /// `mode_split_of`, with a spree mode's third part too (CR
    /// 702.172a): where each part after the first starts.
    pub fn mode_splits_of(&self, id: ObjId) -> SmallVec<[(usize, usize); 2]> {
        let mut out = SmallVec::new();
        let Some(o) = self.objects.get(id) else {
            return out;
        };
        let Some(m) = self
            .modes_of(id)
            .and_then(|ms| ms.get(usize::from(o.cast.mode())))
        else {
            return out;
        };
        for (e, t) in m
            .split
            .into_iter()
            .chain(m.split2)
            .chain(m.more.iter().copied())
        {
            out.push((usize::from(e), usize::from(t)));
        }
        out
    }

    /// Effects a stack object applies on resolution.
    pub fn effects_of(&self, id: ObjId) -> &[crate::card::Effect] {
        self.objects
            .get(id)
            .map_or(&[], |o| effects_in(&self.pool, o))
    }

    /// The precomputed flags of an object's definition, including what a
    /// copy effect's exceptions add (CR 707.9b).
    pub fn flags_of(&self, o: &GameObject) -> crate::card::DefFlags {
        let f = self.pool.get(o.def).flags();
        match o.except {
            Some(x) => f.union(self.pool.get(x).except_flags),
            None => f,
        }
    }

    /// Whether an object's triggered abilities (printed or added by a copy
    /// exception) could see `event`, by event kind.
    pub fn may_watch(&self, def: CardRef, except: Option<CardRef>, event: &Event) -> bool {
        self.pool.get(def).may_watch(event)
            || except.is_some_and(|x| self.pool.get(x).except_may_watch(event))
    }

    /// Whether a "can't" effect of this kind is in force (CR 614.17).
    pub fn rule_in_force(&self, kind: RuleKind) -> bool {
        self.rules.iter().any(|r| r.kind == kind)
            || self.player_static_sources > 0
                && self
                    .player_statics()
                    .any(|(_, k)| k == crate::card::PlayerStaticKind::Rule(kind))
    }

    /// Whether a "can't" effect of this kind applies to `p` this turn.
    pub fn player_rule(&self, p: PlayerId, kind: PlayerRuleKind) -> bool {
        self.player_rules
            .iter()
            .any(|r| r.player == p && r.kind == kind)
    }

    /// The player-level static abilities in force: each with its
    /// source's controller (CR 611.3a: a condition is judged for them).
    fn player_statics(
        &self,
    ) -> impl Iterator<Item = (PlayerId, crate::card::PlayerStaticKind)> + '_ {
        self.battlefield
            .iter()
            .filter(|id| {
                self.pool
                    .get(self.objects[**id].def)
                    .flags()
                    .has_player_static()
            })
            .flat_map(move |id| {
                let o = &self.objects[*id];
                self.pool
                    .get(o.def)
                    .ability_ids
                    .iter()
                    .filter_map(move |a| match self.pool.ability(*a) {
                        crate::card::AbilityDef::PlayerStatic { kind, condition }
                            if condition.is_none_or(|c| {
                                self.condition_now(self.pool.condition(c), o.controller, *id)
                            }) =>
                        {
                            Some((o.controller, *kind))
                        }
                        _ => None,
                    })
            })
    }

    /// Whether `p` may play another land now (CR 305.2): one a turn, plus
    /// additional plays from effects and static abilities.
    pub fn may_play_another_land(&self, p: PlayerId) -> bool {
        let ps = self.player(p);
        let played = ps.lands_played_this_turn;
        if played < 1 {
            return true;
        }
        let mut allowed = 1 + ps.extra_land_plays;
        if played < allowed {
            return true;
        }
        if self.player_static_sources > 0 {
            for (who, kind) in self.player_statics() {
                if let crate::card::PlayerStaticKind::ExtraLands { n, each_player } = kind
                    && (each_player || who == p)
                {
                    allowed += n;
                }
            }
        }
        played < allowed
    }

    /// Whether `caster`'s spells and abilities may target player `p`
    /// ("You have hexproof", "You have shroud": CR 702.11c, 702.18c).
    pub fn player_targetable(&self, p: PlayerId, caster: PlayerId) -> bool {
        self.player_static_sources == 0
            || !self.player_statics().any(|(who, kind)| {
                who == p
                    && match kind {
                        crate::card::PlayerStaticKind::Shroud => true,
                        crate::card::PlayerStaticKind::Hexproof => caster != p,
                        _ => false,
                    }
            })
    }

    /// Whether `p` can't lose the game, and their opponents can't win it
    /// ("You can't lose the game and your opponents can't win the game.").
    pub fn cant_lose(&self, p: PlayerId) -> bool {
        self.player_static_sources > 0
            && self
                .player_statics()
                .any(|(who, kind)| who == p && kind == crate::card::PlayerStaticKind::CantLose)
    }

    /// Whether `p` can't cast spells now: an opponent's "Your opponents
    /// can't cast spells during your turn." during that opponent's turn.
    pub fn cant_cast_spells(&self, p: PlayerId) -> bool {
        self.player_static_sources > 0
            && self.active_player != p
            && self.player_statics().any(|(who, kind)| {
                who == self.active_player
                    && kind == crate::card::PlayerStaticKind::OpponentsCantCastYourTurn
            })
    }

    /// Whether applying an action could ask a player a question in the
    /// middle of an event: which of several replacement effects applies
    /// first (CR 616.1; two or more effects must exist), or which legendary
    /// permanent to keep under the legend rule (CR 704.5j; two legendary
    /// permanents, or one and a legendary spell about to resolve). The
    /// `Engine` snapshots the state before an action when this holds.
    pub fn may_need_mid_decision(&self, action: Option<&crate::decision::Choice>) -> bool {
        let replacers = self.replacement_sources as usize + self.replacements.len();
        // A land play puts a permanent onto the battlefield with no spell
        // on the stack: a legendary land can meet its namesake (CR 704.5j)
        // and any land can meet a replacement effect as it enters.
        let land_play = match action {
            Some(crate::decision::Choice::PlayLand(id)) => self.objects.get(*id).is_some_and(|o| {
                let d = self.pool.get(o.def);
                (self.legendary_count >= 1 && d.is_legendary())
                    || (replacers >= 1 && d.modifies_entering())
                    || d.flags().has_enters_choice()
            }),
            // A modal double-faced card's land face (CR 712.12).
            Some(crate::decision::Choice::CastFace { card, face }) => self
                .objects
                .get(*card)
                .and_then(|o| self.pool.get(o.def).faces.get(usize::from(*face)))
                .is_some_and(|f| {
                    let d = self.pool.get(*f);
                    d.is_land()
                        && ((self.legendary_count >= 1 && d.is_legendary())
                            || (replacers >= 1 && d.modifies_entering())
                            || d.flags().has_enters_choice())
                }),
            // A mana ability resolves as it is activated and may ask for a
            // color (CR 605.3).
            Some(crate::decision::Choice::Activate { ability, .. }) => {
                self.pool.ability_asks(*ability)
            }
            _ => false,
        };
        land_play
            // A mana-boost trigger (CR 605.1b) resolves while mana is paid
            // and may ask for a color, during any payment.
            || self.mana_trigger_sources > 0
            || self.dredge_cards > 0
            || replacers >= 2
            || self.legendary_count >= 2
            || self.stack.iter().any(|id| {
                let o = &self.objects[*id];
                let d = self.pool.get(o.def);
                // A spell or ability that asks something as it resolves
                // ("you may", enter as a copy), a legendary spell that
                // could meet its namesake (CR 704.5j), or one that puts a
                // permanent onto the battlefield whose own "enters with"
                // replacement (CR 614.1c) could meet one already in play.
                // What an ability returns from exile or copies is not known
                // until it resolves, so it could also be a legend meeting
                // its namesake. A copy could become legendary, which
                // `has_mid_choice` covers. An ability is judged by itself:
                // a copy's exceptions can add one its definition does not
                // have.
                // With a card that chooses as it enters ("As this land
                // enters, choose a color", CR 614.12) in the game, so may
                // anything that puts a permanent onto the battlefield.
                let choosers = self.enters_choice_cards > 0;
                match o.ability {
                    Some(a) => {
                        self.pool.ability_asks(a.ability)
                            || ((replacers >= 1 || self.legendary_count >= 1 || choosers)
                                && self.pool.ability_enters(a.ability))
                    }
                    None => {
                        d.has_mid_choice()
                            || (self.legendary_count == 1 && d.is_legendary())
                            || (replacers >= 1 && d.modifies_entering())
                            // A spell that makes a token copy or returns
                            // a card to the battlefield (Self-Reflection
                            // copying a legendary creature) is judged like
                            // an ability that does.
                            || ((choosers || replacers >= 1 || self.legendary_count >= 1)
                                && d.ability_ids.iter().any(|a| self.pool.ability_enters(*a)))
                    }
                }
            })
    }

    /// The cards in `p`'s graveyard: a token there until the next
    /// state-based check is not a card (CR 111.8, 704.5d).
    pub fn cards_in_graveyard(&self, p: PlayerId) -> usize {
        self.player(p)
            .graveyard
            .iter()
            .filter(|id| self.objects[**id].kind != ObjKind::Token)
            .count()
    }

    /// Current characteristics of an object, computed through the layers
    /// (CR 613). Cached per `cversion`. Objects off the battlefield have
    /// their copiable values (no effect in the engine yet applies
    /// elsewhere).
    pub fn characteristics(&self, id: ObjId) -> Option<Characteristics> {
        let obj = self.objects.get(id)?;
        if obj.zone != Zone::Battlefield {
            return Some(self.base_characteristics(obj));
        }
        {
            let cache = self.cache.0.borrow();
            if let Some((v, derived)) = cache.as_ref()
                && *v == self.cversion
            {
                // `Printed` is only derived when no battlefield object has
                // an `except` (layers::compute_battlefield), so the printed
                // values are the copiable values here; skipping the check
                // keeps this hot path a plain copy.
                debug_assert!(!matches!(derived, Derived::Printed) || obj.except.is_none());
                let out = match derived {
                    Derived::Printed => Some(self.printed(obj.def)),
                    Derived::Battlefield(map) => map.get(&id).copied(),
                };
                #[cfg(debug_assertions)]
                if self
                    .cache
                    .1
                    .replace(self.cache.1.get().wrapping_add(1))
                    .is_multiple_of(16)
                {
                    // The cache is only as good as the `cbump` discipline:
                    // in debug builds, check a sample of hits against a
                    // fresh pass (a stale cache stays stale until the next
                    // bump, so sampling still catches it).
                    let fresh = match crate::layers::compute_battlefield(self) {
                        Derived::Printed => Some(self.base_characteristics(obj)),
                        Derived::Battlefield(map) => map.get(&id).copied(),
                    };
                    assert_eq!(
                        out,
                        fresh,
                        "stale characteristics cache for {}: a change that affects characteristics did not cbump",
                        self.pool.get(obj.def).name
                    );
                }
                return out;
            }
        }
        let derived = crate::layers::compute_battlefield(self);
        debug_assert!(!matches!(derived, Derived::Printed) || obj.except.is_none());
        let out = match &derived {
            Derived::Printed => Some(self.printed(obj.def)),
            Derived::Battlefield(map) => map.get(&id).copied(),
        };
        *self.cache.0.borrow_mut() = Some((self.cversion, derived));
        out
    }

    /// Printed characteristics of a card, before any effect.
    pub fn printed(&self, def: CardRef) -> Characteristics {
        self.pool
            .get(def)
            .printed
            .expect("definitions in a pool have printed characteristics")
    }

    /// An object's copiable values (CR 707.2): the printed characteristics
    /// of what it is (or is a copy of), plus what the copy effect's
    /// exceptions add (CR 707.9b). The starting point of the layers.
    #[inline]
    pub fn base_characteristics(&self, o: &GameObject) -> Characteristics {
        // The common case must stay a plain copy of the printed values: the
        // merge below, with its growable vectors, is kept out of line so it
        // does not weigh on every rules query (measured at 13% of an
        // effect-free game when it was inline).
        match o.except {
            None => self.printed(o.def),
            Some(x) => self.printed_with_except(o.def, x),
        }
    }

    #[cold]
    #[inline(never)]
    fn printed_with_except(&self, def: CardRef, except: CardRef) -> Characteristics {
        let mut c = self.printed(def);
        let xd = self.pool.get(except);
        for s in &xd.except_subtype_ids {
            if !c.subtypes.contains(s) {
                c.subtypes.push(*s);
            }
        }
        if let Some(ce) = &xd.copy_except {
            c.keywords = c.keywords.union(ce.keywords);
            c.types = c.types.union(ce.types);
            if ce.not_legendary {
                c.supertypes = c.supertypes.without(crate::card::Supertype::Legendary);
            }
        }
        for a in &xd.except_ability_ids {
            c.abilities.push(*a);
        }
        c
    }

    /// Whether an Aura with enchant `spec`, or an Equipment (CR 301.5c,
    /// `TargetSpec::Creature`), may be attached to `target` (CR 303.4c,
    /// 704.5n). Unlike targeting, hexproof and the "you control" of equip
    /// do not matter here.
    /// The game-state parts of a filter (`ObjFilter::needs_state`): tapped,
    /// attacking, blocking, and mana value.
    pub fn status_ok(&self, f: &crate::card::ObjFilter, id: ObjId) -> bool {
        if !f.needs_state() {
            return true;
        }
        // With an alternative, one side must match whole: its
        // characteristics and its state.
        if let Some(alt) = &f.or {
            let Some(o) = self.objects.get(id) else {
                return false;
            };
            let Some(c) = self.characteristics(id) else {
                return false;
            };
            let mut base = f.clone();
            base.or = None;
            let side = |g: &crate::card::ObjFilter| {
                g.matches(id, o.controller, &c, None, None) && self.status_ok(g, id)
            };
            return side(&base) || side(alt);
        }
        let Some(o) = self.objects.get(id) else {
            return false;
        };
        if let Some(t) = f.tapped
            && o.tapped != t
        {
            return false;
        }
        if f.damaged && o.damage == 0 && !o.deathtouch_damage {
            return false;
        }
        if f.attacking
            && !self
                .combat
                .as_ref()
                .is_some_and(|cb| cb.attackers.iter().any(|a| a.id == id))
        {
            return false;
        }
        if f.blocking
            && self
                .combat
                .as_ref()
                .is_none_or(|cb| cb.blocked_by(id).is_none())
        {
            return false;
        }
        // An unfilled "with mana value X" doesn't restrict (`source_filter`).
        if let Some((cmp, n)) = f.mana_value
            && !f.status.mana_value_x
            && !f.status.spent_colors
        {
            let mv = self.pool.get(o.def).mana_cost.mana_value();
            if !cmp.holds(mv, u32::from(n)) {
                return false;
            }
        }
        !f.status.any() || self.status_quals_ok(f.status, id, o)
    }

    /// The qualifiers of `StatusQuals` for `id` (`status_ok`).
    #[allow(clippy::too_many_lines)] // one check per qualifier
    fn status_quals_ok(&self, q: crate::card::StatusQuals, id: ObjId, o: &GameObject) -> bool {
        if let Some(b) = q.blocked {
            // CR 509.1h: blocked or unblocked only once blockers are
            // declared, and only while attacking.
            let declared = matches!(
                self.step,
                crate::turn::Step::DeclareBlockers
                    | crate::turn::Step::FirstStrikeDamage
                    | crate::turn::Step::CombatDamage
                    | crate::turn::Step::EndOfCombat
            );
            let ok = declared
                && self
                    .combat
                    .as_ref()
                    .and_then(|cb| cb.attacker(id))
                    .is_some_and(|a| a.blocked == b);
            if !ok {
                return false;
            }
        }
        if let Some(e) = q.enchanted {
            // CR 303.4: an Aura attached to it.
            let enchanted = o.zone == Zone::Battlefield
                && self.battlefield.iter().any(|a| {
                    self.objects[*a].attached_to == Some(id)
                        && self
                            .characteristics(*a)
                            .is_some_and(|c| c.has_subtype(crate::card::Subtype::AURA))
                });
            if enchanted != e {
                return false;
            }
        }
        // CR 301.5: an Equipment attached to it.
        if q.equipped
            && !(o.zone == Zone::Battlefield
                && self.battlefield.iter().any(|a| {
                    self.objects[*a].attached_to == Some(id)
                        && self
                            .characteristics(*a)
                            .is_some_and(|c| c.has_subtype(crate::card::Subtype::EQUIPMENT))
                }))
        {
            return false;
        }
        if q.entered_this_turn
            && (o.zone != Zone::Battlefield || o.timestamp < self.turn_started_at)
        {
            return false;
        }
        if q.dealt_damage && !self.dealt_damage_this_turn.contains(&id) {
            return false;
        }
        if q.blocked_this_turn && !self.blockers_this_turn.contains(&id) {
            return false;
        }
        if q.attacked_this_turn && !self.attackers_this_turn.contains(&id) {
            return false;
        }
        if q.modified
            && !self.modified(
                id,
                |a| {
                    self.characteristics(a)
                        .is_some_and(|c| c.has_subtype(crate::card::Subtype::EQUIPMENT))
                },
                |a| {
                    self.characteristics(a)
                        .is_some_and(|c| c.has_subtype(crate::card::Subtype::AURA))
                },
            )
        {
            return false;
        }
        // "Aura attached to a creature" (CR 303.4): what it is attached
        // to, as it is now.
        if let Some(on) = q.on {
            let ok = o.attached_to.is_some_and(|h| {
                self.characteristics(h).is_some_and(|c| match on {
                    crate::card::OnHost::Creature => c.is_creature(),
                    crate::card::OnHost::Land => c.is_land(),
                })
            });
            if !ok {
                return false;
            }
        }
        // CR 708.2: a face-down permanent.
        if q.face_down && !self.pool.is_face_down(o.def) {
            return false;
        }
        // CR 701.60: "suspected creature".
        if q.suspected
            .is_some_and(|s| s != (o.zone == Zone::Battlefield && o.suspected()))
        {
            return false;
        }
        // "with a morph ability" (CR 702.37): a face-down permanent has no
        // abilities (CR 708.2).
        if q.morph && !self.pool.get(o.def).has_morph() {
            return false;
        }
        // "with toxic" (CR 702.164): a toxic ability among its current ones.
        if q.toxic
            && !self.characteristics(id).is_some_and(|c| {
                c.abilities
                    .iter()
                    .any(|a| matches!(self.pool.ability(*a), crate::card::AbilityDef::Toxic(_)))
            })
        {
            return false;
        }
        // "with a +1/+1 counter on it" (CR 122.1).
        if q.counter.is_some_and(|c| !o.counters_match(c)) {
            return false;
        }
        q.token.is_none_or(|t| (o.kind == ObjKind::Token) == t)
    }

    /// Whether the spell `obj` targets what `t` asks for, judged by
    /// `you`.
    pub(crate) fn spell_targets(
        &self,
        obj: &GameObject,
        t: &crate::card::SpellTargets,
        you: PlayerId,
        source: Option<ObjId>,
    ) -> bool {
        obj.targets.iter().any(|x| match *x {
            Target::Player(p) => t.player || (t.you && p == you),
            Target::Object(o) => t.filter.as_ref().is_some_and(|f| {
                self.objects
                    .get(o)
                    .is_some_and(|ob| ob.zone == Zone::Battlefield)
                    && self.characteristics(o).is_some_and(|c| {
                        f.matches(o, self.objects[o].controller, &c, source, Some(you))
                    })
                    && self.status_ok(f, o)
            }),
        })
    }

    /// CR 702.16c: a permanent with protection can't be enchanted or
    /// equipped by something with the quality.
    pub fn protected_attachment(&self, attachment: ObjId, target: ObjId) -> bool {
        match (
            self.characteristics(target),
            self.characteristics(attachment),
        ) {
            (Some(t), Some(a)) => t.protected_from(
                &a,
                self.objects[attachment].controller != self.objects[target].controller,
                &self.pool,
            ),
            _ => false,
        }
    }

    pub fn is_legal_attachment(&self, spec: TargetSpec, target: ObjId) -> bool {
        let Some(obj) = self.objects.get(target) else {
            return false;
        };
        if obj.zone != Zone::Battlefield {
            return false;
        }
        let Some(c) = self.characteristics(target) else {
            return false;
        };
        match spec {
            TargetSpec::Any
            | TargetSpec::AnyOther
            | TargetSpec::Creature
            | TargetSpec::CreatureYouControl
            | TargetSpec::OtherCreature
            | TargetSpec::NonlegendaryCreatureYouControl
            | TargetSpec::AttackingOrBlockingCreature
            | TargetSpec::AttackingCreature
            | TargetSpec::BlockingCreature => c.is_creature(),
            TargetSpec::TappedCreature => c.is_creature() && obj.tapped,
            TargetSpec::CreatureWithSubtype(s) => c.is_creature() && c.has_subtype(s),
            TargetSpec::ArtifactCreatureOrLand => {
                c.is_creature() || c.is_land() || c.types.contains(CardType::Artifact)
            }
            TargetSpec::ArtifactOrEnchantment => {
                c.types.contains(CardType::Artifact) || c.types.contains(CardType::Enchantment)
            }
            TargetSpec::Artifact => c.types.contains(CardType::Artifact),
            TargetSpec::Enchantment => c.types.contains(CardType::Enchantment),
            TargetSpec::Permanent | TargetSpec::OtherPermanent => true,
            TargetSpec::Land => c.is_land(),
            TargetSpec::CreatureOrPlaneswalker => c.is_creature() || c.is_planeswalker(),
            TargetSpec::ArtifactOrCreature => {
                c.is_creature() || c.types.contains(CardType::Artifact)
            }
            TargetSpec::Filtered(f) => {
                self.pool
                    .filter(f)
                    .matches(target, obj.controller, &c, None, None)
            }
            TargetSpec::Player
            | TargetSpec::Opponent
            | TargetSpec::Spell
            | TargetSpec::CreatureSpell
            | TargetSpec::NoncreatureSpell
            | TargetSpec::OnStack { .. }
            | TargetSpec::PlayerOrPlaneswalker
            | TargetSpec::PlayerOrBattle
            | TargetSpec::OpponentOrPlaneswalker
            | TargetSpec::SpellFiltered(_)
            | TargetSpec::SpellOr(_)
            | TargetSpec::GraveyardCard { .. }
            | TargetSpec::UpTo(_)
            | TargetSpec::Many(_)
            | TargetSpec::CreatureCardInYourGraveyard => false,
        }
    }

    /// Teamwork: the untapped creatures the caster of `spell` would tap,
    /// with total power `n` or more, or `None` if they can't. The engine
    /// picks: the one with the least power that is enough alone, else the
    /// most powerful first; creatures that make mana last.
    pub fn teamwork_tappers(&self, p: PlayerId, n: u8) -> Option<Vec<ObjId>> {
        let n = i16::from(n);
        let mut all: Vec<(bool, i16, ObjId)> = self
            .creatures_of(p)
            .filter(|id| !self.objects[*id].tapped)
            .filter_map(|id| {
                let c = self.characteristics(id)?;
                let mana = self
                    .pool
                    .get(self.objects[id].def)
                    .abilities
                    .iter()
                    .any(|a| matches!(a, crate::card::AbilityDef::TapForMana(_)));
                Some((mana, c.power, id))
            })
            .collect();
        for mana in [false, true] {
            if let Some((_, _, id)) = all
                .iter()
                .filter(|(m, pw, _)| (!*m || mana) && *pw >= n)
                .min_by_key(|(m, pw, _)| (*m, *pw))
            {
                return Some(vec![*id]);
            }
        }
        all.sort_by_key(|(mana, power, _)| (*mana, -*power));
        let mut total = 0i16;
        let mut out = Vec::new();
        for (_, power, id) in all {
            if total >= n {
                break;
            }
            if power > 0 {
                total += power;
                out.push(id);
            }
        }
        (total >= n).then_some(out)
    }

    /// CR 702.153a: the creatures with power `n` or greater `p` could
    /// sacrifice for casualty.
    pub fn casualty_candidates(&self, p: PlayerId, n: u8) -> Vec<ObjId> {
        self.battlefield
            .iter()
            .copied()
            .filter(|id| {
                self.objects[*id].controller == p
                    && self.characteristics(*id).is_some_and(|c| {
                        c.types.contains(crate::card::CardType::Creature) && c.power >= i16::from(n)
                    })
            })
            .collect()
    }

    /// CR 702.166a: the artifacts, enchantments, and tokens `p` could
    /// sacrifice to bargain.
    pub fn bargain_candidates(&self, p: PlayerId) -> Vec<ObjId> {
        self.battlefield
            .iter()
            .copied()
            .filter(|id| {
                let o = &self.objects[*id];
                o.controller == p
                    && (o.kind == ObjKind::Token
                        || self.characteristics(*id).is_some_and(|c| {
                            c.types.contains(crate::card::CardType::Artifact)
                                || c.types.contains(crate::card::CardType::Enchantment)
                        }))
            })
            .collect()
    }

    /// The mana spent to cast `id` (or the spell it was), none if it
    /// wasn't cast or its text doesn't ask (CR 601.2h).
    /// The mana values of the cards in `p`'s graveyard, `except` aside: what
    /// collecting evidence may exile (CR 701.59a), by card.
    pub fn evidence_cards(&self, p: PlayerId, except: ObjId) -> Vec<(ObjId, u32)> {
        self.player(p)
            .graveyard
            .iter()
            .copied()
            .filter(|id| *id != except)
            .map(|id| {
                (
                    id,
                    self.pool.get(self.objects[id].def).mana_cost.mana_value(),
                )
            })
            .collect()
    }

    /// The cards in `p`'s graveyard a forage could exile (CR 701.61a),
    /// other than `except`, oldest first.
    pub fn forage_cards(&self, p: PlayerId, except: ObjId) -> Vec<ObjId> {
        self.player(p)
            .graveyard
            .iter()
            .copied()
            .filter(|id| *id != except)
            .collect()
    }

    /// The Foods `p` controls a forage could sacrifice (CR 701.61a).
    pub fn forage_foods(&self, p: PlayerId) -> Vec<ObjId> {
        let Some(food) = self.pool.subtype_lookup("Food") else {
            return Vec::new();
        };
        self.permanents_of(p)
            .filter(|id| {
                self.characteristics(*id)
                    .is_some_and(|c| c.has_subtype(food))
            })
            .collect()
    }

    /// Whether `p` can forage (CR 701.61a): three other cards in their
    /// graveyard, or a Food.
    pub fn forage_payable(&self, p: PlayerId, except: ObjId) -> bool {
        self.forage_cards(p, except).len() >= 3 || !self.forage_foods(p).is_empty()
    }

    /// Whether `p` can collect evidence `n` (CR 701.59b): the cards in
    /// their graveyard have total mana value `n` or greater.
    pub fn evidence_payable(&self, p: PlayerId, n: u32, except: ObjId) -> bool {
        n == 0
            || self
                .evidence_cards(p, except)
                .iter()
                .map(|(_, mv)| *mv)
                .sum::<u32>()
                >= n
    }

    /// Whether the non-mana parts of an optional additional cost could be
    /// paid by `p` (blight: a creature; collect evidence: enough mana
    /// value in the graveyard).
    pub fn other_costs_payable(&self, p: PlayerId, c: &crate::card::Cost, except: ObjId) -> bool {
        !(c.blight > 0 && self.creatures_of(p).next().is_none())
            && self.evidence_payable(p, c.evidence, except)
            && (!c.forage || self.forage_payable(p, except))
            && c.craft
                .as_deref()
                .is_none_or(|f| !self.craft_candidates(p, f, except).is_empty())
    }

    /// What craft may exile (CR 702.167a): cards in `p`'s graveyard, then
    /// permanents `p` controls, matching `f`, other than `except`.
    pub fn craft_candidates(
        &self,
        p: PlayerId,
        f: &crate::card::ObjFilter,
        except: ObjId,
    ) -> Vec<ObjId> {
        let mut out: Vec<ObjId> = self
            .player(p)
            .graveyard
            .iter()
            .copied()
            .filter(|id| *id != except)
            .filter(|id| {
                let o = &self.objects[*id];
                let c = self.base_characteristics(o);
                f.matches(*id, o.owner, &c, Some(except), Some(p))
            })
            .collect();
        out.extend(self.battlefield.iter().copied().filter(|id| {
            *id != except
                && self.objects[*id].controller == p
                && self
                    .characteristics(*id)
                    .is_some_and(|c| f.matches(*id, p, &c, Some(except), Some(p)))
        }));
        out
    }

    pub fn spent_on(&self, id: ObjId) -> SpentMana {
        if self.spent_mana.is_empty() {
            return SpentMana::default();
        }
        self.spent_mana
            .iter()
            .find(|(o, _)| *o == id)
            .map(|(_, s)| *s)
            .unwrap_or_default()
    }

    /// `condition_now` for an ability that `trigger_obj` triggered: "that
    /// spell" in a condition is that object (CR 603.4).
    pub fn condition_for(
        &self,
        cond: &crate::card::StaticCondition,
        you: PlayerId,
        source: ObjId,
        trigger_obj: Option<ObjId>,
    ) -> bool {
        if let crate::card::StaticCondition::ManaSpent {
            kind,
            at_least,
            that: true,
        } = cond
        {
            return trigger_obj
                .is_some_and(|t| self.spent_on(t).count(*kind) >= u32::from(*at_least));
        }
        self.condition_now(cond, you, source)
    }

    /// The number of basic land types among lands `p` controls (domain, CR
    /// 305.6).
    pub fn basic_land_types(&self, p: PlayerId) -> usize {
        let mut seen = crate::mana::ColorSet::default();
        for id in self.permanents_of(p) {
            if let Some(c) = self.characteristics(id)
                && c.is_land()
            {
                for s in &c.subtypes {
                    if let Some(col) = s.basic_color() {
                        seen = seen.with(col);
                    }
                }
            }
        }
        seen.iter().count()
    }

    /// Whether a condition holds now for `you`, about `source` (which may
    /// be entering, not yet on the battlefield): conditional "enters
    /// tapped", attack restrictions, and intervening "if" clauses.
    #[allow(clippy::too_many_lines)] // one arm per condition
    pub fn condition_now(
        &self,
        cond: &crate::card::StaticCondition,
        you: PlayerId,
        source: ObjId,
    ) -> bool {
        use crate::card::StaticCondition as C;
        let count = |filter: &crate::card::ObjFilter, whose: PlayerId| {
            let filter = &*self.source_filter(filter, Some(source), None);
            self.battlefield
                .iter()
                .filter(|id| {
                    let o = &self.objects[**id];
                    self.characteristics(**id).is_some_and(|c| {
                        filter.matches(**id, o.controller, &c, Some(source), Some(whose))
                    }) && self.status_ok(filter, **id)
                })
                .count()
        };
        match cond {
            C::CardsInGraveyard(n) => self.cards_in_graveyard(you) >= *n as usize,
            C::UnlockedDoors(n) => self.unlocked_doors(you) >= *n,
            C::AttackedWith(f) => self.attacked_with(you, f),
            C::EnteredWith { filter, .. } => self.entered_with(you, filter, source),
            C::CardTypesInGraveyard(n) => {
                let mut types = crate::card::TypeSet::default();
                for g in &self.player(you).graveyard {
                    types = types.union(self.pool.get(self.objects[*g].def).types);
                }
                types.len() >= *n
            }
            // "there are five or more Islands on the battlefield": the
            // filter names no controller.
            C::YouControl { filter, at_least } | C::OnBattlefield { filter, at_least } => {
                count(filter, you) >= *at_least as usize
            }
            C::YouControlNone(filter) => count(filter, you) == 0,
            C::YouControlAtMost { filter, n } => count(filter, you) <= *n as usize,
            C::YouControlEach(filters) => filters.iter().all(|f| count(f, you) > 0),
            C::DefenderControls {
                filter,
                at_least: 0,
            } => self
                .players_in_game()
                .filter(|p| *p != you)
                .any(|p| count(filter, p) == 0),
            C::Or(a, b) => self.condition_now(a, you, source) || self.condition_now(b, you, source),
            C::AsCast(_) => self.objects.get(source).is_some_and(|o| o.cast.as_cast()),
            C::DefenderControls { filter, at_least } => self
                .players_in_game()
                .filter(|p| *p != you)
                .any(|p| count(filter, p) >= *at_least as usize),
            // Judged on the whole declaration (`GameState::set_rules_allow`).
            C::OthersInCombat { .. } => true,
            C::DefenderPoisoned => self
                .players_in_game()
                .any(|p| p != you && self.player(p).poison > 0),
            C::MoreCreaturesThanOpponent { .. } => {
                let creatures = |p: PlayerId| self.creatures_of(p).count();
                let mine = creatures(you);
                self.players_in_game()
                    .filter(|p| *p != you)
                    .any(|p| mine > creatures(p))
            }
            C::YourTurn => self.active_player == you,
            C::SourceAttackedBattle => self.battle_attackers_this_turn.contains(&source),
            C::NotYourTurn => self.active_player != you,
            C::YourMainPhase => self.active_player == you && self.step.is_main(),
            C::YourCombatPhase => {
                self.active_player == you && self.step.phase() == crate::turn::Phase::Combat
            }
            C::FirstCombatPhase => {
                self.step.phase() == crate::turn::Phase::Combat && self.combats_this_turn <= 1
            }
            C::SourceAttacking => self
                .combat
                .as_ref()
                .is_some_and(|c| c.attackers.iter().any(|a| a.id == source)),
            C::SourceMonstrous => self.objects.get(source).is_some_and(GameObject::monstrous),
            C::SourceSuspected(want) => {
                self.objects.get(source).is_some_and(GameObject::suspected) == *want
            }
            C::SourceEnchanted | C::SourceEquipped => {
                let want = if *cond == C::SourceEnchanted {
                    crate::card::Subtype::AURA
                } else {
                    crate::card::Subtype::EQUIPMENT
                };
                self.battlefield.iter().any(|id| {
                    self.objects[*id].attached_to == Some(source)
                        && self
                            .characteristics(*id)
                            .is_some_and(|c| c.has_subtype(want))
                })
            }
            C::OpponentLifeAtMost(n) => self
                .players_in_game()
                .any(|p| p != you && self.player(p).life <= *n),
            C::YourLife { n, at_least } => {
                let l = self.player(you).life;
                if *at_least { l >= *n } else { l <= *n }
            }
            C::AnyPlayerLifeAtMost(n) => self.players_in_game().any(|p| self.player(p).life <= *n),
            C::MoreLifeThanOpponent => {
                let l = self.player(you).life;
                self.players_in_game()
                    .any(|p| p != you && self.player(p).life < l)
            }
            C::BasicLandTypes(n) => self.basic_land_types(you) >= *n as usize,
            C::SourceCounters { kind, n, at_least } => {
                let have = self.objects.get(source).map_or(0, |o| o.counters(*kind));
                if *at_least { have >= *n } else { have <= *n }
            }
            C::OpponentPoisonAtLeast(n) => self
                .players_in_game()
                .any(|p| p != you && self.player(p).poison >= *n),
            C::AttackedThisTurn => self.attacked_this_turn & (1 << you.index().min(7)) != 0,
            C::AttackedLastTurn => self.attacked_last_turn.contains(&(you, source)),
            C::CreatureDiedThisTurn => self.creatures_died_this_turn > 0,
            C::AssassinDealtCombatDamage => self.assassin_damage & (1 << you.index().min(7)) != 0,
            C::LandEnteredThisTurn => self.lands_entered_this_turn & (1 << you.index().min(7)) != 0,
            C::ArtifactEnteredThisTurn => {
                self.artifacts_entered_this_turn & (1 << you.index().min(7)) != 0
            }
            C::Kicked => self.objects.get(source).is_some_and(|o| o.cast.kicked()),
            C::NotKicked => self.objects.get(source).is_some_and(|o| !o.cast.kicked()),
            C::ManaSpent {
                kind,
                at_least,
                that: false,
            } => self.spent_on(source).count(*kind) >= u32::from(*at_least),
            // Judged by `condition_for`, which knows the triggering spell.
            C::ManaSpent { that: true, .. } => false,
            C::NoManaSpent => self.spent_on(source).count(crate::card::SpentKind::Total) == 0,
            C::Impending => self.objects.get(source).is_some_and(|o| {
                o.alt == AltCast::Impending && o.counters(crate::card::CounterKind::Time) > 0
            }),
            C::WebSlung(_) => match self.objects.get(source) {
                Some(o) => o.alt == AltCast::WebSlinging,
                None => self
                    .lki
                    .get(&source)
                    .is_some_and(|l| l.object.alt == AltCast::WebSlinging),
            },
            C::SneakPaid(_) => match self.objects.get(source) {
                Some(o) => o.alt == AltCast::Sneak,
                None => self
                    .lki
                    .get(&source)
                    .is_some_and(|l| l.object.alt == AltCast::Sneak),
            },
            C::Bargained => match self.objects.get(source) {
                Some(o) if o.zone == Zone::Hand => !self.bargain_candidates(you).is_empty(),
                Some(o) => o.cast.bargained(),
                None => self
                    .lki
                    .get(&source)
                    .is_some_and(|l| l.object.cast.bargained()),
            },
            C::Saddled => self.saddled.contains(&source),
            C::Beheld => self.objects.get(source).is_some_and(|o| o.cast.beheld()),
            C::Teamwork => match self.objects.get(source) {
                Some(o) => o.cast.teamwork(),
                None => self
                    .lki
                    .get(&source)
                    .is_some_and(|l| l.object.cast.teamwork()),
            },
            C::OptionalCostPaid => match self.objects.get(source) {
                Some(o) if o.zone == Zone::Hand => self
                    .pool
                    .get(o.def)
                    .optional_cost
                    .as_deref()
                    .is_some_and(|c| self.other_costs_payable(you, c, source)),
                Some(o) => o.cast.optional_paid(),
                None => self
                    .lki
                    .get(&source)
                    .is_some_and(|l| l.object.cast.optional_paid()),
            },
            C::OffspringPaid => self
                .objects
                .get(source)
                .map(|o| o.cast.offspring())
                .or_else(|| self.lki.get(&source).map(|l| l.object.cast.offspring()))
                .unwrap_or(false),
            C::CardsDrawnThisTurn(n) => u32::from(self.drawn_this_turn[you.index().min(3)]) >= *n,
            C::Escaped => self
                .objects
                .get(source)
                .is_some_and(|o| o.alt == AltCast::Escape),
            C::OpponentDamagedThisTurn => self
                .players_in_game()
                .any(|p| p != you && self.damaged_this_turn & (1 << p.index().min(7)) != 0),
            C::OpponentLostLifeThisTurn => self
                .players_in_game()
                .any(|p| p != you && self.lost_life_this_turn & (1 << p.index().min(7)) != 0),
            C::PermanentLeftThisTurn => self.left_this_turn & (1 << you.index().min(7)) != 0,
            C::CardsInHand { n, at_most } => {
                let h = self.player(you).hand.len();
                if *at_most {
                    h <= *n as usize
                } else {
                    h >= *n as usize
                }
            }
            C::OpponentControls { filter, at_least } => self
                .players_in_game()
                .filter(|p| *p != you)
                .any(|p| count(filter, p) >= *at_least as usize),
            C::SourceIs(filter) => match self.objects.get(source) {
                Some(o) => {
                    self.characteristics_or_lki(source).is_some_and(|c| {
                        filter.matches(source, o.controller, &c, Some(source), Some(you))
                    }) && self.status_ok(filter, source)
                }
                // A leaves-the-battlefield trigger's "if it isn't a token"
                // (Sphinx of False Conclusions): the permanent as it last
                // existed (CR 603.10a, 608.2h).
                None => self.lki.get(&source).is_some_and(|l| {
                    let o = &l.object;
                    self.characteristics_or_lki(source).is_some_and(|c| {
                        filter.matches(source, o.controller, &c, Some(source), Some(you))
                    }) && filter.tapped.is_none_or(|t| o.tapped == t)
                        && !filter.attacking
                        && !filter.blocking
                        && (!filter.status.any() || self.status_quals_ok(filter.status, source, o))
                        && filter
                            .status
                            .token
                            .is_none_or(|t| (o.kind == ObjKind::Token) == t)
                }),
            },
            C::AttachedIs(filter) => self
                .objects
                .get(source)
                .and_then(|o| o.attached_to)
                .is_some_and(|host| {
                    self.characteristics(host).is_some_and(|c| {
                        filter.matches(
                            host,
                            self.objects[host].controller,
                            &c,
                            Some(source),
                            Some(you),
                        )
                    }) && self.status_ok(filter, host)
                }),
            C::SourceModified => self.modified(
                source,
                |id| {
                    self.characteristics(id)
                        .is_some_and(|c| c.has_subtype(crate::card::Subtype::EQUIPMENT))
                },
                |id| {
                    self.characteristics(id)
                        .is_some_and(|c| c.has_subtype(crate::card::Subtype::AURA))
                },
            ),
            C::TotalPower(n) => {
                let total: i32 = self
                    .battlefield
                    .iter()
                    .filter(|id| self.objects[**id].controller == you)
                    .filter_map(|id| self.characteristics(*id))
                    .filter(Characteristics::is_creature)
                    .map(|c| c.power().max(0))
                    .sum();
                total >= i32::try_from(*n).unwrap_or(i32::MAX)
            }
            C::DifferentPowers(n) => {
                let mut powers: Vec<i32> = self
                    .battlefield
                    .iter()
                    .filter(|id| self.objects[**id].controller == you)
                    .filter_map(|id| self.characteristics(*id))
                    .filter(Characteristics::is_creature)
                    .map(|c| c.power())
                    .collect();
                powers.sort_unstable();
                powers.dedup();
                powers.len() >= *n as usize
            }
            C::And(a, b) => {
                self.condition_now(a, you, source) && self.condition_now(b, you, source)
            }
            C::TotalToughness(n) => {
                let total: i32 = self
                    .creatures_of(you)
                    .filter_map(|id| self.characteristics(id))
                    .map(|c| c.toughness().max(0))
                    .sum();
                total >= i32::try_from(*n).unwrap_or(i32::MAX)
            }
            C::OpponentMoreLands => {
                let lands = |p: PlayerId| {
                    self.battlefield
                        .iter()
                        .filter(|id| {
                            self.objects[**id].controller == p
                                && self.characteristics(**id).is_some_and(|c| c.is_land())
                        })
                        .count()
                };
                let mine = lands(you);
                self.players_in_game().any(|p| p != you && lands(p) > mine)
            }
            _ => self.condition_history(cond, you, source),
        }
    }

    /// The conditions that need no characteristics of permanents (zones,
    /// counts, what happened this turn, the step), shared by
    /// `condition_now` and the layer system; `false` for the rest.
    #[allow(clippy::too_many_lines)] // one arm per condition
    pub fn condition_history(
        &self,
        cond: &crate::card::StaticCondition,
        you: PlayerId,
        source: ObjId,
    ) -> bool {
        use crate::card::StaticCondition as C;
        match cond {
            C::GraveyardCards { filter, at_least } => {
                let n = self
                    .player(you)
                    .graveyard
                    .iter()
                    .filter(|id| {
                        let c = self.base_characteristics(&self.objects[**id]);
                        filter.matches(**id, you, &c, None, Some(you))
                    })
                    .count();
                n >= *at_least as usize
            }
            C::NoCardInHand(filter) => !self.player(you).hand.iter().any(|id| {
                let c = self.base_characteristics(&self.objects[*id]);
                filter.matches(*id, you, &c, None, Some(you))
            }),
            C::OpponentGraveyard(n) => self
                .players_in_game()
                .any(|p| p != you && self.player(p).graveyard.len() >= *n as usize),
            C::ManaValuesInGraveyard(n) => {
                let mut seen = 0u64;
                for g in &self.player(you).graveyard {
                    let mv = self.pool.get(self.objects[*g].def).mana_cost.mana_value();
                    seen |= 1 << mv.min(63);
                }
                seen.count_ones() >= *n
            }
            C::SourceAttackingAlone => self
                .combat
                .as_ref()
                .is_some_and(|c| c.attackers.len() == 1 && c.attackers[0].id == source),
            C::SourceBlocked => self
                .combat
                .as_ref()
                .and_then(|c| c.attacker(source))
                .is_some_and(|a| a.blocked),
            C::SourceBeingBlocked => self
                .combat
                .as_ref()
                .and_then(|c| c.attacker(source))
                .is_some_and(|a| !a.blockers.is_empty()),
            C::DrawnThisTurn(n) => u32::from(self.drawn_this_turn[you.index().min(3)]) >= *n,
            C::CastThisTurn {
                filter,
                at_least,
                other,
            } => {
                // "a spell with mana value 4 or greater": the mana value
                // `matches_characteristics` leaves to the caller, judged from
                // the card's mana cost.
                let matches = |def: crate::card::CardRef| {
                    filter.matches_characteristics(&self.printed(def))
                        && filter.mana_value.is_none_or(|(cmp, n)| {
                            cmp.holds(self.pool.get(def).mana_cost.mana_value(), u32::from(n))
                        })
                };
                let n = self
                    .cast_this_turn
                    .iter()
                    .filter(|(p, def)| *p == you && matches(*def))
                    .count();
                // "Another": the source's own cast (as a spell, or as the
                // permanent it became) is not counted.
                let own = *other
                    && self
                        .objects
                        .get(source)
                        .or_else(|| self.lki.get(&source).map(|l| &l.object))
                        .is_some_and(|o| {
                            o.cast.cast_from().is_some()
                                && o.controller == you
                                && (o.zone == Zone::Stack || o.timestamp >= self.turn_started_at)
                                && matches(o.def)
                        });
                n >= *at_least as usize + usize::from(own)
            }
            C::CastFrom(zone) => self
                .objects
                .get(source)
                .or_else(|| self.lki.get(&source).map(|l| &l.object))
                .is_some_and(|o| o.cast.cast_from() == Some(*zone)),
            C::GainedLifeThisTurn { or_lost } => {
                let bit = 1 << you.index().min(7);
                self.turn_log.gained_life & bit != 0
                    || (*or_lost && self.lost_life_this_turn & bit != 0)
            }
            C::DescendedThisTurn => self.turn_log.descended & (1 << you.index().min(7)) != 0,
            C::SpellNumber(n) => {
                let cast = self
                    .cast_this_turn
                    .iter()
                    .filter(|(p, _)| *p == you)
                    .count();
                cast + 1 == *n as usize
            }
            C::SurveilledThisTurn => self.turn_log.surveilled & (1 << you.index().min(7)) != 0,
            C::CounterPutThisTurn => self.turn_log.counter_put & (1 << you.index().min(7)) != 0,
            C::FaceDownEnteredThisTurn => {
                self.turn_log.face_down_entered & (1 << you.index().min(7)) != 0
            }
            C::CreatureEnteredThisTurn { other } => {
                let n = self.turn_log.creatures_entered[you.index().min(3)];
                // "Another creature": the source's own entry this turn is
                // not counted.
                let own = *other
                    && self.objects.get(source).is_some_and(|o| {
                        o.zone == Zone::Battlefield
                            && o.controller == you
                            && o.timestamp >= self.turn_started_at
                            && self.base_characteristics(o).is_creature()
                    });
                n > u8::from(own)
            }
            C::PutIntoGraveyardThisTurn(types) => self.turn_log.to_graveyard.intersects(*types),
            C::NonlandEnteredThisTurn(n) => {
                u32::from(self.turn_log.nonland_entered[you.index().min(3)]) >= *n
            }
            C::DiscardedThisTurn => self.discarded_this_turn & (1 << you.index().min(7)) != 0,
            C::Night => self.desig.day_night == DayNight::Night,
            C::LifeGainedThisTurn(n) => self.turn_log.life_gained[you.index().min(3)] >= *n,
            C::CrimeThisTurn => self.turn_log.crime & (1 << you.index().min(7)) != 0,
            C::PutCounterThisTurn => self.turn_log.counter_put_by & (1 << you.index().min(7)) != 0,
            C::NthResolution(n) => self.resolution_number == *n,
            C::OpponentNoncombatDamagedThisTurn => self.players_in_game().any(|p| {
                p != you && self.turn_log.noncombat_damaged & (1 << p.index().min(7)) != 0
            }),
            C::LeftGraveyardThisTurn => {
                self.turn_log.left_graveyard & (1 << you.index().min(7)) != 0
            }
            C::SacrificedThisTurn(types) => {
                self.turn_log.sacrificed[you.index().min(3)].intersects(*types)
            }
            C::CardToGraveyardThisTurn(types) => {
                self.turn_log.cards_to_graveyard[you.index().min(3)].intersects(*types)
            }
            C::MoreCardsInHand => {
                let mine = self.player(you).hand.len();
                self.players_in_game()
                    .all(|p| p == you || self.player(p).hand.len() < mine)
            }
            C::OpponentPoison(n) => self
                .players_in_game()
                .any(|p| p != you && self.player(p).poison >= *n),
            C::OwnCardInExile => !self.player(you).exile.is_empty(),
            C::Monarch => self.desig.monarch == Some(you),
            C::CompletedDungeon => self.desig.completed(you) > 0,
            C::CityBlessing => self.desig.blessed(you),
            C::EnduringStory => self.desig.storied(you),
            C::NoEnduringStory => !self.desig.storied(you),
            C::MaxSpeed => self.desig.speed(you) >= 4,
            C::During(crate::card::TurnTime::AttackedThisStep) => {
                self.step == crate::turn::Step::DeclareAttackers
                    && self.combat.as_ref().is_some_and(|c| {
                        c.attackers
                            .iter()
                            .any(|a| a.attacking == Target::Player(you))
                    })
            }
            C::During(t) => t.now(self.step, self.active_player, you),
            // CR 700.4: as they last existed, under whose control.
            C::CreaturesDied { n, yours } => {
                let died = if *yours {
                    self.died_this_turn
                        .iter()
                        .filter(|(_, controller, _)| *controller == you)
                        .count()
                } else {
                    usize::from(self.creatures_died_this_turn)
                };
                died >= *n as usize
            }
            C::Void => self.turn_log.nonland_left || self.turn_log.warped,
            C::NotYourMainPhase => !(self.active_player == you && self.step.is_main()),
            C::NotCastFrom(zone) => self
                .objects
                .get(source)
                .or_else(|| self.lki.get(&source).map(|l| &l.object))
                .and_then(|o| o.cast.cast_from())
                .is_some_and(|z| z != *zone),
            C::PutAnyCounterThisTurn => {
                self.turn_log.any_counter_put_by & (1 << you.index().min(7)) != 0
            }
            C::AttackedWithAtLeast(f, n) => {
                self.attackers_this_turn
                    .iter()
                    .filter(|id| {
                        let (controller, c) = match self.objects.get(**id) {
                            Some(o) => (o.controller, self.characteristics(**id)),
                            None => match self.lki.get(id) {
                                Some(l) => (l.object.controller, Some(l.characteristics)),
                                None => return false,
                            },
                        };
                        controller == you
                            && c.is_some_and(|c| f.matches(**id, controller, &c, None, Some(you)))
                    })
                    .count()
                    >= *n as usize
            }
            // The layer system and `condition_now` judge both parts their
            // own way.
            C::And(a, b) => {
                self.condition_history(a, you, source) && self.condition_history(b, you, source)
            }
            _ => false,
        }
    }

    /// CR 700.9: a permanent is modified if it has a counter, is equipped,
    /// or is enchanted by an Aura its controller controls; `equipment` and
    /// `aura` say what an attached object is.
    pub fn modified(
        &self,
        id: ObjId,
        equipment: impl Fn(ObjId) -> bool,
        aura: impl Fn(ObjId) -> bool,
    ) -> bool {
        let Some(o) = self.objects.get(id) else {
            return false;
        };
        if o.counters.iter().any(|(_, n)| *n > 0) {
            return true;
        }
        self.battlefield.iter().any(|a| {
            let att = &self.objects[*a];
            att.attached_to == Some(id)
                && (equipment(*a) || (att.controller == o.controller && aura(*a)))
        })
    }

    /// The state qualifiers of a filter that a permanent's last known
    /// information answers (CR 603.10a: leaves-the-battlefield abilities
    /// look back): tapped, token, counters, and modified (CR 700.9; its
    /// Auras and Equipment are still attached as the event is seen, the
    /// state-based actions not yet performed). Other qualifiers don't
    /// restrict here.
    pub fn lki_status_ok(&self, f: &crate::card::ObjFilter, old: ObjId, last: &LastKnown) -> bool {
        let obj = &last.object;
        // An alternative ("creature or Vehicle") isn't judged here.
        if f.or.is_some() {
            return true;
        }
        if f.tapped.is_some_and(|t| obj.tapped != t) {
            return false;
        }
        let quals = f.status;
        if quals
            .token
            .is_some_and(|t| (obj.kind == ObjKind::Token) != t)
        {
            return false;
        }
        if quals.counter.is_some_and(|c| !obj.counters_match(c)) {
            return false;
        }
        if quals.modified && !obj.counters.iter().any(|(_, n)| *n > 0) {
            let has = |a: ObjId, s: crate::card::Subtype| {
                self.characteristics(a).is_some_and(|c| c.has_subtype(s))
            };
            let attached = self.battlefield.iter().any(|a| {
                let att = &self.objects[*a];
                att.attached_to == Some(old)
                    && (has(*a, crate::card::Subtype::EQUIPMENT)
                        || (att.controller == obj.controller
                            && has(*a, crate::card::Subtype::AURA)))
            });
            if !attached {
                return false;
            }
        }
        true
    }

    /// Characteristics of an object that may have left the game, falling
    /// back to last-known information.
    /// `f` with its `same` relation ("named ~", "that shares a color with
    /// it") filled in from `of` as it is now, or as it last existed (CR
    /// 608.2h). A face-down object has no name (CR 708.2).
    pub fn with_same_of(
        &self,
        f: &crate::card::ObjFilter,
        of: Option<ObjId>,
    ) -> crate::card::ObjFilter {
        let c = of.and_then(|id| self.characteristics_or_lki(id));
        let def = c
            .as_ref()
            .map(|c| c.def)
            .filter(|d| *d != self.pool.face_down(false) && *d != self.pool.face_down(true));
        let power = c.as_ref().map(|c| c.power);
        f.with_same(def, c.map(|c| c.colors).unwrap_or_default(), power)
    }

    /// A filter with "named ~" filled in from the source ("target
    /// permanent not named ~"); other filters as they are.
    /// "with mana value X" is filled in from `x`, or else the X chosen for
    /// the source spell (CR 107.3), if known.
    fn source_filter<'f>(
        &self,
        f: &'f crate::card::ObjFilter,
        source: Option<ObjId>,
        x: Option<u32>,
    ) -> std::borrow::Cow<'f, crate::card::ObjFilter> {
        let x = if f.status.mana_value_x {
            x.or_else(|| source.and_then(|s| self.x_chosen(s)))
        } else {
            None
        };
        let mut out = if f.same.is_some() {
            std::borrow::Cow::Owned(self.with_same_of(f, source))
        } else {
            std::borrow::Cow::Borrowed(f)
        };
        if f.status.mana_value_x
            && let Some(x) = x
        {
            let g = out.to_mut();
            g.status.mana_value_x = false;
            g.mana_value = g
                .mana_value
                .map(|(cmp, _)| (cmp, u8::try_from(x).unwrap_or(u8::MAX)));
        }
        if f.status.spent_colors
            && let Some(s) = source
        {
            Self::fill_spent_colors(out.to_mut(), self.spent_on(s).colors());
        }
        out
    }

    /// Converge in a filter (`StatusQuals::spent_colors`): its mana value
    /// and power thresholds become the number of colors spent.
    pub fn fill_spent_colors(g: &mut crate::card::ObjFilter, colors: u32) {
        g.status.spent_colors = false;
        g.mana_value = g
            .mana_value
            .map(|(cmp, _)| (cmp, u8::try_from(colors).unwrap_or(u8::MAX)));
        g.power = g
            .power
            .map(|(cmp, _)| (cmp, i16::try_from(colors).unwrap_or(i16::MAX)));
    }

    /// Whether `id` is in combat with `source` as `with` asks (CR
    /// 509.1g); a source no longer on the battlefield as it last was, so
    /// the relation doesn't restrict (CR 113.7a).
    pub fn combat_with_ok(
        &self,
        with: Option<crate::card::CombatWith>,
        source: Option<ObjId>,
        id: ObjId,
    ) -> bool {
        use crate::card::CombatWith;
        let Some(with) = with else {
            return true;
        };
        let Some(source) = source else {
            return true;
        };
        if self
            .objects
            .get(source)
            .is_none_or(|o| o.zone != Zone::Battlefield)
        {
            return true;
        }
        let Some(cb) = self.combat.as_ref() else {
            return false;
        };
        let blocking = || {
            cb.attacker(source)
                .is_some_and(|a| a.blockers.contains(&id))
        };
        let blocked_by = || {
            cb.attacker(id)
                .is_some_and(|a| a.blockers.contains(&source))
        };
        match with {
            CombatWith::Blocking => blocking(),
            CombatWith::BlockedBy => blocked_by(),
            CombatWith::BlockingOrBlockedBy => blocking() || blocked_by(),
        }
    }

    /// X of a spell on the stack if it was chosen (CR 107.3, 601.2b).
    pub fn x_chosen(&self, spell: ObjId) -> Option<u32> {
        self.x_values
            .iter()
            .find(|(id, _)| *id == spell)
            .map(|(_, x)| *x)
    }

    pub fn characteristics_or_lki(&self, id: ObjId) -> Option<Characteristics> {
        self.characteristics(id)
            .or_else(|| self.lki.get(&id).map(|l| l.characteristics))
    }

    pub fn is_creature_on_battlefield(&self, id: ObjId) -> bool {
        self.objects
            .get(id)
            .is_some_and(|o| o.zone == Zone::Battlefield)
            && self.characteristics(id).is_some_and(|c| c.is_creature())
    }

    pub fn permanents_of(&self, p: PlayerId) -> impl Iterator<Item = ObjId> + '_ {
        self.battlefield
            .iter()
            .copied()
            .filter(move |id| self.objects[*id].controller == p)
    }

    /// The creature `id` is paired with (CR 702.95b).
    pub fn partner(&self, id: ObjId) -> Option<ObjId> {
        self.pairs.iter().find_map(|&(a, b)| {
            if a == id {
                Some(b)
            } else if b == id {
                Some(a)
            } else {
                None
            }
        })
    }

    pub fn creatures_of(&self, p: PlayerId) -> impl Iterator<Item = ObjId> + '_ {
        self.permanents_of(p)
            .filter(move |id| self.is_creature_on_battlefield(*id))
    }

    /// Whether `target` is a legal target for `spec` chosen by `caster` (CR
    /// 115.5, 702.11 hexproof), for a spell or an ability of `source`.
    pub fn is_legal_target(
        &self,
        caster: PlayerId,
        spec: crate::card::TargetSpec,
        target: Target,
    ) -> bool {
        self.is_legal_target_for(caster, spec, target, None)
    }

    /// `is_legal_target` for an ability of `source` ("another target
    /// creature" excludes it).
    pub fn is_legal_target_for(
        &self,
        caster: PlayerId,
        spec: crate::card::TargetSpec,
        target: Target,
        source: Option<ObjId>,
    ) -> bool {
        self.is_legal_target_x(caster, spec, target, source, None)
    }

    /// Whether target `t` for `spec` answers its filter's "that player"
    /// (`ObjFilter::that_player`): controlled by (a card in a graveyard,
    /// owned by) the triggering player or an earlier target's controller,
    /// as it last existed if it has left. With that player unknown, or no
    /// such relation, it doesn't restrict.
    pub fn that_player_ok(
        &self,
        spec: crate::card::TargetSpec,
        t: Target,
        trigger_player: Option<PlayerId>,
        targets: &[Target],
    ) -> bool {
        use crate::card::{TargetSpec, ThatPlayer};
        let (TargetSpec::Filtered(f) | TargetSpec::GraveyardCard { filter: f, .. }) =
            self.pool.base_spec(spec)
        else {
            return true;
        };
        let Some(rel) = self.pool.filter(f).that_player else {
            return true;
        };
        let player = match rel {
            ThatPlayer::Triggering => trigger_player,
            ThatPlayer::ControllerOf(n) => match targets.get(usize::from(n)) {
                Some(Target::Object(id)) => self
                    .objects
                    .get(*id)
                    .map(|o| o.controller)
                    .or_else(|| self.lki.get(id).map(|l| l.object.controller)),
                _ => None,
            },
            ThatPlayer::Target(n) => match targets.get(usize::from(n)) {
                Some(Target::Player(p)) => Some(*p),
                _ => None,
            },
            ThatPlayer::TargetOrController { .. } => None,
        };
        let Some(player) = player else {
            return true;
        };
        match t {
            Target::Object(id) => self.objects.get(id).is_some_and(|o| {
                if o.zone == Zone::Battlefield {
                    o.controller == player
                } else {
                    o.owner == player
                }
            }),
            Target::Player(_) => false,
        }
    }

    /// `is_legal_target_for` with X taken as `x` ("with mana value X"
    /// while X is being chosen) rather than the spell's chosen X.
    pub fn is_legal_target_x(
        &self,
        caster: PlayerId,
        spec: crate::card::TargetSpec,
        target: Target,
        source: Option<ObjId>,
        x: Option<u32>,
    ) -> bool {
        self.spec_matches(caster, spec, target, source, x, true)
    }

    /// Whether `target` answers `spec`'s description; `targeting`: also
    /// that it can be targeted (hexproof, shroud, protection, CR 115.5).
    /// Without, the objects "each" means for an overloaded spell (CR
    /// 702.96a).
    #[allow(clippy::too_many_lines)]
    pub fn spec_matches(
        &self,
        caster: PlayerId,
        spec: crate::card::TargetSpec,
        target: Target,
        source: Option<ObjId>,
        x: Option<u32>,
        targeting: bool,
    ) -> bool {
        use crate::card::TargetSpec;
        if let TargetSpec::UpTo(_) | TargetSpec::Many(_) = spec {
            return self.spec_matches(
                caster,
                self.pool.base_spec(spec),
                target,
                source,
                x,
                targeting,
            );
        }
        match target {
            Target::Player(p) => {
                p.index() < self.num_players()
                    && self.player(p).lost.is_none()
                    && match spec {
                        TargetSpec::Any
                        | TargetSpec::AnyOther
                        | TargetSpec::Player
                        | TargetSpec::PlayerOrPlaneswalker
                        | TargetSpec::PlayerOrBattle => true,
                        TargetSpec::Opponent | TargetSpec::OpponentOrPlaneswalker => p != caster,
                        _ => false,
                    }
                    && (!targeting || self.player_targetable(p, caster))
            }
            Target::Object(id) => {
                let Some(obj) = self.objects.get(id) else {
                    return false;
                };
                if spec == TargetSpec::CreatureCardInYourGraveyard {
                    return obj.zone == Zone::Graveyard
                        && obj.owner == caster
                        && self.pool.get(obj.def).is_creature();
                }
                if let TargetSpec::GraveyardCard { filter, who, .. } = spec {
                    let whose = match who {
                        crate::card::Who::Any => true,
                        crate::card::Who::You => obj.owner == caster,
                        crate::card::Who::Opponents => obj.owner != caster,
                    };
                    let c = self.base_characteristics(obj);
                    // "another target card": the source's card after it
                    // moved to the graveyard is the source still.
                    let source = source.map(|s| self.current_object(s).unwrap_or(s));
                    let f = self.source_filter(self.pool.filter(filter), source, x);
                    return obj.zone == Zone::Graveyard
                        && whose
                        && f.matches(id, obj.owner, &c, source, None)
                        && self.status_ok(&f, id);
                }
                if let TargetSpec::OnStack {
                    spells,
                    activated,
                    triggered,
                    source: source_type,
                    yours,
                } = spec
                {
                    if obj.zone != Zone::Stack || (yours && obj.controller != caster) {
                        return false;
                    }
                    let Some(sa) = obj.ability else {
                        return spells;
                    };
                    let kind_ok = match self.pool.ability(sa.ability) {
                        crate::card::AbilityDef::Activated(_) => activated,
                        crate::card::AbilityDef::Triggered(_) => triggered,
                        _ => false,
                    };
                    // "from an artifact source": the source as it is, or as
                    // it last existed (CR 113.7a).
                    return kind_ok
                        && source_type.is_none_or(|t| {
                            self.characteristics_or_lki(sa.source)
                                .is_some_and(|c| c.types.contains(t))
                        });
                }
                // "target spell or creature": a spell, or a permanent
                // judged like `Filtered` below.
                // Its qualities other than the permanent's card types apply
                // to the spell too ("target spell or permanent with mana
                // value 1 or greater").
                if let TargetSpec::SpellOr(f) = spec
                    && obj.zone == Zone::Stack
                {
                    if obj.kind == ObjKind::Ability {
                        return false;
                    }
                    let mut f = self.pool.filter(f).clone();
                    f.types = crate::card::TypeSet::default();
                    let c = self.base_characteristics(obj);
                    return f.matches(id, obj.controller, &c, source, Some(caster));
                }
                if spec.is_spell() {
                    if obj.zone != Zone::Stack || obj.kind == ObjKind::Ability {
                        return false;
                    }
                    let creature = self.pool.get(obj.def).is_creature();
                    return match spec {
                        TargetSpec::CreatureSpell => creature,
                        TargetSpec::NoncreatureSpell => !creature,
                        TargetSpec::SpellFiltered(f) => {
                            let c = self.base_characteristics(obj);
                            let f = self.pool.filter(f);
                            f.matches(id, obj.controller, &c, source, Some(caster))
                                && self.status_ok(f, id)
                                && f.targeting
                                    .as_ref()
                                    .is_none_or(|t| self.spell_targets(obj, t, caster, source))
                        }
                        _ => true,
                    };
                }
                if obj.zone != Zone::Battlefield {
                    return false;
                }
                let Some(c) = self.characteristics(id) else {
                    return false;
                };
                let type_ok = match spec {
                    // CR 115.4: "any target" is a creature, player,
                    // planeswalker, or battle.
                    TargetSpec::Any => c.is_creature() || c.is_planeswalker() || c.is_battle(),
                    TargetSpec::AnyOther => {
                        (c.is_creature() || c.is_planeswalker() || c.is_battle())
                            && Some(id) != source
                    }
                    TargetSpec::CreatureOrPlaneswalker => c.is_creature() || c.is_planeswalker(),
                    TargetSpec::PlayerOrBattle => c.is_battle(),
                    TargetSpec::PlayerOrPlaneswalker | TargetSpec::OpponentOrPlaneswalker => {
                        c.is_planeswalker()
                    }
                    TargetSpec::Land => c.is_land(),
                    TargetSpec::Creature => c.is_creature(),
                    TargetSpec::CreatureWithSubtype(s) => c.is_creature() && c.has_subtype(s),
                    TargetSpec::OtherCreature => c.is_creature() && Some(id) != source,
                    TargetSpec::TappedCreature => c.is_creature() && obj.tapped,
                    TargetSpec::CreatureYouControl => c.is_creature() && obj.controller == caster,
                    TargetSpec::NonlegendaryCreatureYouControl => {
                        c.is_creature()
                            && obj.controller == caster
                            && !c.supertypes.contains(crate::card::Supertype::Legendary)
                    }
                    TargetSpec::ArtifactCreatureOrLand => {
                        c.is_creature() || c.is_land() || c.types.contains(CardType::Artifact)
                    }
                    TargetSpec::ArtifactOrEnchantment => {
                        c.types.contains(CardType::Artifact)
                            || c.types.contains(CardType::Enchantment)
                    }
                    TargetSpec::Artifact => c.types.contains(CardType::Artifact),
                    TargetSpec::Enchantment => c.types.contains(CardType::Enchantment),
                    TargetSpec::Permanent => true,
                    TargetSpec::OtherPermanent => Some(id) != source,
                    TargetSpec::ArtifactOrCreature => {
                        c.is_creature() || c.types.contains(CardType::Artifact)
                    }
                    TargetSpec::AttackingOrBlockingCreature => {
                        c.is_creature()
                            && self.combat.as_ref().is_some_and(|cb| {
                                cb.attackers
                                    .iter()
                                    .any(|a| a.id == id || a.blockers.contains(&id))
                            })
                    }
                    TargetSpec::AttackingCreature => {
                        c.is_creature()
                            && self
                                .combat
                                .as_ref()
                                .is_some_and(|cb| cb.attackers.iter().any(|a| a.id == id))
                    }
                    TargetSpec::BlockingCreature => {
                        c.is_creature()
                            && self
                                .combat
                                .as_ref()
                                .is_some_and(|cb| cb.blocked_by(id).is_some())
                    }
                    TargetSpec::Filtered(f) | TargetSpec::SpellOr(f) => {
                        let f = self.source_filter(self.pool.filter(f), source, x);
                        f.matches(id, obj.controller, &c, source, Some(caster))
                            && self.status_ok(&f, id)
                            && self.combat_with_ok(f.combat_with, source, id)
                            // "other than enchanted creature".
                            && !(f.not_attached
                                && source
                                    .and_then(|s| self.objects.get(s))
                                    .and_then(|s| s.attached_to)
                                    == Some(id))
                    }
                    TargetSpec::SpellFiltered(_)
                    | TargetSpec::GraveyardCard { .. }
                    | TargetSpec::Player
                    | TargetSpec::Opponent
                    | TargetSpec::Spell
                    | TargetSpec::CreatureSpell
                    | TargetSpec::NoncreatureSpell
                    | TargetSpec::OnStack { .. }
                    | TargetSpec::UpTo(_)
                    | TargetSpec::Many(_)
                    | TargetSpec::CreatureCardInYourGraveyard => false,
                };
                if !targeting {
                    return type_ok;
                }
                let hexproof_ok = (!c.has(Keyword::Hexproof) || obj.controller == caster)
                    && !c.has(Keyword::Shroud)
                    // CR 702.11d: "hexproof from [quality]": not by spells
                    // or abilities an opponent controls from such sources.
                    && (!c.has(Keyword::HexproofFromFilter)
                        || obj.controller == caster
                        || !source
                            .and_then(|s| self.characteristics_or_lki(s))
                            .is_some_and(|sc| {
                                c.abilities.iter().any(|a| {
                                    matches!(self.pool.ability(*a),
                                        crate::card::AbilityDef::HexproofFrom(f)
                                            if f.matches_characteristics(&sc))
                                })
                            }));
                // CR 702.16b: protection from a quality forbids targeting by
                // spells and abilities from sources with it.
                let protection_ok = !c.keywords.intersects(crate::card::KeywordSet::PROTECTION)
                    || !source
                        .and_then(|s| self.characteristics_or_lki(s))
                        .is_some_and(|sc| {
                            c.protected_from(&sc, caster != obj.controller, &self.pool)
                        });
                type_ok && hexproof_ok && protection_ok
            }
        }
    }

    /// Players still in the game.
    pub fn players_in_game(&self) -> impl Iterator<Item = PlayerId> + '_ {
        (0..self.num_players())
            .map(|i| PlayerId(u8::try_from(i).unwrap_or(0)))
            .filter(move |p| self.player(*p).lost.is_none())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cast_info_flags_share_no_bit() {
        let mut seen = 0u32;
        for f in super::CastInfo::ALL {
            assert_eq!(seen & f, 0, "flag {f:#x} overlaps another");
            seen |= f;
        }
    }

    /// Each `CastInfo` fact has its own bit: casting from hand once shared
    /// a bit with paying the second additional cost.
    #[test]
    fn cast_info_facts_are_independent() {
        let mut c = CastInfo::default();
        c.set_cast_from(Zone::Hand);
        assert!(!c.other_cost());
        c.set_other_cost(true);
        assert_eq!(c.cast_from(), Some(Zone::Hand));
        c.set_other_cost(false);
        assert_eq!(c.cast_from(), Some(Zone::Hand));
    }

    use super::*;

    /// `GameObject` is cloned on every zone change and `Characteristics` on
    /// every rules query; keep them small (design §8).
    #[test]
    fn hot_structs_stay_small() {
        let (o, c) = (
            std::mem::size_of::<GameObject>(),
            std::mem::size_of::<Characteristics>(),
        );
        eprintln!(
            "GameObject {o} bytes, Characteristics {c} bytes, Flow {} bytes, Event {} bytes",
            std::mem::size_of::<Flow>(),
            std::mem::size_of::<Event>()
        );
        assert!(o <= 136, "GameObject is {o} bytes");
        // 80 since the keyword set outgrew 64 bits (banding and "can't
        // crew Vehicles" took the last two; 2026-11-19).
        assert!(c <= 80, "Characteristics is {c} bytes");
    }
}
