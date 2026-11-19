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
    Modification, ObjFilter, ReplacementAbility, RuleKind, SubtypeSet, SupertypeSet, TargetSpec,
    TypeSet,
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
}

impl Zone {
    /// Whether every player may look at the zone's contents (CR 400.2).
    pub const fn is_public(self) -> bool {
        matches!(
            self,
            Zone::Battlefield | Zone::Graveyard | Zone::Stack | Zone::Exile | Zone::Command
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
/// once at the next matching step, then ceases to exist.
#[derive(Copy, Clone, Debug)]
pub struct DelayedTrigger {
    pub controller: PlayerId,
    pub ability: StackAbility,
}

/// Something a spell or ability can target or an effect can act on.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Target {
    Object(ObjId),
    Player(PlayerId),
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
    /// Cards foretold this turn (CR 702.143a: castable on a later turn).
    pub foretold_this_turn: Vec<ObjId>,
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
    /// In exile: discarded with madness, waiting for its trigger.
    MadnessExiled,
    /// In exile: foretold, castable on a later turn.
    Foretold,
    /// In exile: suspended, with time counters.
    Suspended,
    /// In exile: rebounding, castable at its owner's next upkeep.
    Rebounding,
}

/// A spell's chosen mode (CR 700.2, the low six bits) and cast flags:
/// kicked (CR 702.33c; kept by the permanent it becomes, for "if it was
/// kicked") and cast with flashback (CR 702.34a; exiled as it leaves the
/// stack). One byte, so `GameObject` stays small.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct CastInfo(u8);

impl CastInfo {
    const KICKED: u8 = 0x40;
    const FLASHBACK: u8 = 0x80;
    /// Cast as an adventure (CR 715.3) on the stack; "on an adventure" in
    /// exile.
    const ADVENTURE: u8 = 0x20;
    /// Cast with its buyback cost paid (CR 702.27): to its owner's hand as
    /// it resolves.
    const BUYBACK: u8 = 0x10;
    /// Cast for its evoke cost (CR 702.74); kept by the permanent.
    const EVOKED: u8 = 0x08;
    const MODE: u8 = 0x07;

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
        self.0 & Self::MODE
    }

    pub fn set_mode(&mut self, mode: u8) {
        self.0 = (self.0 & !Self::MODE) | (mode & Self::MODE);
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
        CastInfo(self.0 & (Self::KICKED | Self::EVOKED))
    }
}

impl GameObject {
    /// Whether a copy effect applies (CR 707).
    pub fn is_copy(&self) -> bool {
        self.def != self.card || self.except.is_some()
    }

    pub fn counters(&self, kind: CounterKind) -> u32 {
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
}

impl PlayerState {
    pub fn zone_mut(&mut self, zone: Zone) -> &mut Vec<ObjId> {
        match zone {
            Zone::Library => &mut self.library,
            Zone::Hand => &mut self.hand,
            Zone::Graveyard => &mut self.graveyard,
            Zone::Exile => &mut self.exile,
            Zone::Battlefield | Zone::Stack | Zone::Command => {
                unreachable!("{zone:?} is not a per-player zone")
            }
        }
    }
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
    /// Whether a first-strike combat damage step occurred this combat (CR 510.4).
    pub had_first_strike_step: bool,
}

impl CombatState {
    pub fn attacker(&self, id: ObjId) -> Option<&Attacker> {
        self.attackers.iter().find(|a| a.id == id)
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
    /// Damage left on a shield (CR 615.7).
    pub remaining: u32,
}

/// A "can't" effect from a resolved spell (CR 614.17), until end of turn.
#[derive(Copy, Clone, Debug)]
pub struct RuleEffect {
    pub kind: RuleKind,
    pub duration: Duration,
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
        None => def.cast_targets(),
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

    pub fn has(&self, k: Keyword) -> bool {
        self.keywords.contains(k)
    }

    /// Has this subtype; a changeling has every creature type (CR 702.73a).
    pub fn has_subtype(&self, s: crate::card::Subtype) -> bool {
        self.subtypes.contains(&s) || (s.is_creature_type() && self.has(Keyword::Changeling))
    }

    /// Whether this object has protection from something with `source`'s
    /// colors (CR 702.16b–e).
    #[inline]
    pub fn protected_from(&self, source: &Characteristics, pool: &crate::card::CardPool) -> bool {
        self.keywords
            .intersects(crate::card::KeywordSet::PROTECTION)
            && (crate::mana::Color::ALL
                .into_iter()
                .any(|col| self.has(Keyword::protection_from(col)) && source.colors.contains(col))
                || self.has(Keyword::ProtectionFromFilter)
                    && self.abilities.iter().any(|a| {
                        matches!(pool.ability(*a),
                            crate::card::AbilityDef::Protection(f) if f.matches_characteristics(source))
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

/// Last-known information about an object that left its zone (CR 113.7a).
#[derive(Clone, Debug)]
pub struct LastKnown {
    pub object: GameObject,
    pub characteristics: Characteristics,
    /// The object it became in its new zone (CR 400.7), if it moved rather
    /// than ceased to exist.
    pub moved_to: Option<ObjId>,
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
    },
    /// Choosing X for a spell being cast (CR 601.2b), after the mode and
    /// before targets.
    ChoosingX {
        spell: ObjId,
    },
    /// Choosing whether to pay a spell's kicker (CR 702.33), after the mode
    /// and before X.
    ChoosingKicker {
        spell: ObjId,
    },
    /// Choosing whether to pay a spell's buyback cost (CR 702.27), after
    /// kicker and before X.
    ChoosingBuyback {
        spell: ObjId,
    },
    /// Attackers declared; each chooses the player or planeswalker it
    /// attacks (CR 508.1a) when the defender controls a planeswalker.
    AttackTargets {
        chosen: Vec<(ObjId, Target)>,
        remaining: Vec<ObjId>,
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
    /// "Activate only once" abilities used (exhaust, CR 702.177a;
    /// power-up, CR 702.193a): never cleared, since the object a new
    /// zone makes is a new object with a new id (CR 400.7).
    pub activated_ever: Vec<(ObjId, crate::card::AbilityId)>,
    /// Creatures declared as attackers this turn (boast, CR 702.142a);
    /// one put onto the battlefield attacking was never declared.
    pub attackers_this_turn: Vec<ObjId>,
    /// The first timestamp of this turn: a permanent whose timestamp is
    /// not older entered this turn (power-up, CR 702.193a), as a
    /// permanent's timestamp is the time it entered (CR 613.7e).
    pub turn_started_at: Timestamp,
    /// X as chosen for spells on the stack (CR 107.3), kept off
    /// `GameObject` so the hot struct stays at 120 bytes.
    pub x_values: Vec<(ObjId, u32)>,
    /// How spells and abilities on the stack divide damage or counters
    /// among their targets (CR 601.2d), in target order from the variable
    /// group's first target; kept off `GameObject` like `x_values`.
    pub divisions: Vec<(ObjId, Vec<u32>)>,
    pub turn_number: u32,
    /// Spells each player has cast this turn, and the most any one player
    /// cast last turn ("if no spells were cast last turn", werewolves).
    pub spells_this_turn: [u8; 4],
    /// Cards each player drew this turn ("your second card each turn").
    pub drawn_this_turn: [u8; 4],
    /// Players who declared an attacker this turn (raid), one bit each.
    pub attacked_this_turn: u8,
    /// Players who had a land enter under their control this turn
    /// (landfall), one bit each.
    pub lands_entered_this_turn: u8,
    /// Creatures put into a graveyard from the battlefield this turn
    /// (morbid).
    pub creatures_died_this_turn: u16,
    /// Players dealt damage this turn (bloodthirst), one bit each.
    pub damaged_this_turn: u8,
    /// Players who discarded a card this turn, one bit each.
    pub discarded_this_turn: u8,
    /// The spells cast this turn, by caster ("if you've cast a noncreature
    /// spell this turn"); inline, so copying the state rarely allocates.
    pub cast_this_turn: SmallVec<[(PlayerId, crate::card::CardRef); 4]>,
    /// Casting from exile's bookkeeping, boxed: most games never need it,
    /// and the state is copied for every mid-event snapshot.
    pub exile_casts: Option<Box<ExileCasts>>,
    /// Suspended cards in exile, so upkeeps skip the scan without any.
    pub suspended_count: u32,
    /// Cards in the game with an "As this enters, choose ..." ability:
    /// with any, every way onto the battlefield may ask a question.
    pub enters_choice_cards: u32,
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
    /// Battlefield permanents with a static ability reading this turn's
    /// history or step (`DefFlags::TURN_STATIC`).
    pub turn_static_sources: u32,
    /// Face-down permanents on the battlefield, so listing options skips
    /// the turn-face-up check without any.
    pub face_down_count: u32,
    /// A cast just undone (CR 726) and the state version then: it is not
    /// offered again until something changes, so an agent can't loop on it.
    pub undone_cast: Option<(ObjId, u64)>,
    /// Battlefield permanents with a static ability granting abilities,
    /// plus grants made this turn by effects: while nonzero, any permanent
    /// may have activated or triggered abilities its definition lacks.
    pub grant_sources: u32,
    pub eot_grants: u32,
    pub mid: MidDecision,
    pub lki: IdMap<LastKnown>,
    /// Triggered abilities waiting to be put on the stack (CR 603.3).
    pub pending_triggers: Vec<PendingTrigger>,
    /// Delayed triggered abilities waiting for their event (CR 603.7).
    pub delayed_triggers: Vec<DelayedTrigger>,
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
                || self.lookback.is_some()
                || self.grant_sources > 0
                || self.eot_grants > 0)
            || ((!self.delayed_triggers.is_empty() || self.suspended_count > 0)
                && matches!(event, Event::StepBegin { .. }))
            || matches!(&event, Event::ZoneChange { def, .. } if self.pool.get(*def).has_triggers())
            // "When you cycle this card": the card itself watches.
            || matches!(&event, Event::Cycled { def, .. } if self.pool.get(*def).has_triggers());
        if may_trigger {
            self.detect_triggers(&event);
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
            // A mana ability resolves as it is activated and may ask for a
            // color (CR 605.3).
            Some(crate::decision::Choice::Activate { ability, .. }) => {
                self.pool.ability_asks(*ability)
            }
            _ => false,
        };
        land_play
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
                            || (choosers
                                && d.ability_ids.iter().any(|a| self.pool.ability_enters(*a)))
                    }
                }
            })
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
        if let Some((cmp, n)) = f.mana_value {
            let mv = self.pool.get(o.def).mana_cost.mana_value();
            if !cmp.holds(mv, u32::from(n)) {
                return false;
            }
        }
        true
    }

    /// Whether the spell `obj` targets what `t` asks for, judged by
    /// `you`.
    fn spell_targets(
        &self,
        obj: &GameObject,
        t: &crate::card::SpellTargets,
        you: PlayerId,
    ) -> bool {
        obj.targets.iter().any(|x| match *x {
            Target::Player(p) => t.player || (t.you && p == you),
            Target::Object(o) => t.filter.as_ref().is_some_and(|f| {
                self.objects
                    .get(o)
                    .is_some_and(|ob| ob.zone == Zone::Battlefield)
                    && self.characteristics(o).is_some_and(|c| {
                        f.matches(o, self.objects[o].controller, &c, None, Some(you))
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
            (Some(t), Some(a)) => t.protected_from(&a, &self.pool),
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
            | TargetSpec::PlayerOrPlaneswalker
            | TargetSpec::OpponentOrPlaneswalker
            | TargetSpec::SpellFiltered(_)
            | TargetSpec::GraveyardCard { .. }
            | TargetSpec::UpTo(_)
            | TargetSpec::Many(_)
            | TargetSpec::CreatureCardInYourGraveyard => false,
        }
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
            let filter = &*self.source_filter(filter, Some(source));
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
            C::CardsInGraveyard(n) => self.player(you).graveyard.len() >= *n as usize,
            C::CardTypesInGraveyard(n) => {
                let mut types = crate::card::TypeSet::default();
                for g in &self.player(you).graveyard {
                    types = types.union(self.pool.get(self.objects[*g].def).types);
                }
                types.len() >= *n
            }
            C::YouControl { filter, at_least } => count(filter, you) >= *at_least as usize,
            C::YouControlNone(filter) => count(filter, you) == 0,
            C::DefenderControls { filter, at_least } => self
                .players_in_game()
                .filter(|p| *p != you)
                .any(|p| count(filter, p) >= *at_least as usize),
            C::YourTurn => self.active_player == you,
            C::SourceAttacking => self
                .combat
                .as_ref()
                .is_some_and(|c| c.attackers.iter().any(|a| a.id == source)),
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
            C::SourceCounters { kind, n, at_least } => {
                let have = self.objects.get(source).map_or(0, |o| o.counters(*kind));
                if *at_least { have >= *n } else { have <= *n }
            }
            C::OpponentPoisonAtLeast(n) => self
                .players_in_game()
                .any(|p| p != you && self.player(p).poison >= *n),
            C::AttackedThisTurn => self.attacked_this_turn & (1 << you.index().min(7)) != 0,
            C::CreatureDiedThisTurn => self.creatures_died_this_turn > 0,
            C::LandEnteredThisTurn => self.lands_entered_this_turn & (1 << you.index().min(7)) != 0,
            C::Kicked => self.objects.get(source).is_some_and(|o| o.cast.kicked()),
            C::CardsDrawnThisTurn(n) => u32::from(self.drawn_this_turn[you.index().min(3)]) >= *n,
            C::Escaped => self
                .objects
                .get(source)
                .is_some_and(|o| o.alt == AltCast::Escape),
            C::OpponentDamagedThisTurn => self
                .players_in_game()
                .any(|p| p != you && self.damaged_this_turn & (1 << p.index().min(7)) != 0),
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
            C::SourceIs(filter) => self.objects.get(source).is_some_and(|o| {
                self.characteristics_or_lki(source).is_some_and(|c| {
                    filter.matches(source, o.controller, &c, Some(source), Some(you))
                }) && self.status_ok(filter, source)
            }),
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
            _ => self.condition_history(cond, you, source),
        }
    }

    /// The conditions that need no characteristics of permanents (zones,
    /// counts, what happened this turn, the step), shared by
    /// `condition_now` and the layer system; `false` for the rest.
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
            C::CastThisTurn(filter) => self
                .cast_this_turn
                .iter()
                .any(|(p, def)| *p == you && filter.matches_characteristics(&self.printed(*def))),
            C::DiscardedThisTurn => self.discarded_this_turn & (1 << you.index().min(7)) != 0,
            C::MoreCardsInHand => {
                let mine = self.player(you).hand.len();
                self.players_in_game()
                    .all(|p| p == you || self.player(p).hand.len() < mine)
            }
            C::OpponentPoison(n) => self
                .players_in_game()
                .any(|p| p != you && self.player(p).poison >= *n),
            C::OwnCardInExile => !self.player(you).exile.is_empty(),
            C::During(t) => t.now(self.step, self.active_player, you),
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
        f.with_same(def, c.map(|c| c.colors).unwrap_or_default())
    }

    /// A filter with "named ~" filled in from the source ("target
    /// permanent not named ~"); other filters as they are.
    fn source_filter<'f>(
        &self,
        f: &'f crate::card::ObjFilter,
        source: Option<ObjId>,
    ) -> std::borrow::Cow<'f, crate::card::ObjFilter> {
        if f.same.is_some() {
            std::borrow::Cow::Owned(self.with_same_of(f, source))
        } else {
            std::borrow::Cow::Borrowed(f)
        }
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
    #[allow(clippy::too_many_lines)]
    pub fn is_legal_target_for(
        &self,
        caster: PlayerId,
        spec: crate::card::TargetSpec,
        target: Target,
        source: Option<ObjId>,
    ) -> bool {
        use crate::card::TargetSpec;
        if let TargetSpec::UpTo(_) | TargetSpec::Many(_) = spec {
            return self.is_legal_target_for(caster, self.pool.base_spec(spec), target, source);
        }
        match target {
            Target::Player(p) => {
                p.index() < self.num_players()
                    && self.player(p).lost.is_none()
                    && match spec {
                        TargetSpec::Any | TargetSpec::Player | TargetSpec::PlayerOrPlaneswalker => {
                            true
                        }
                        TargetSpec::Opponent | TargetSpec::OpponentOrPlaneswalker => p != caster,
                        _ => false,
                    }
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
                if let TargetSpec::GraveyardCard { filter, who } = spec {
                    let whose = match who {
                        crate::card::Who::Any => true,
                        crate::card::Who::You => obj.owner == caster,
                        crate::card::Who::Opponents => obj.owner != caster,
                    };
                    let c = self.base_characteristics(obj);
                    // "another target card": the source's card after it
                    // moved to the graveyard is the source still.
                    let source = source.map(|s| self.current_object(s).unwrap_or(s));
                    let f = self.source_filter(self.pool.filter(filter), source);
                    return obj.zone == Zone::Graveyard
                        && whose
                        && f.matches(id, obj.owner, &c, source, None)
                        && self.status_ok(&f, id);
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
                                    .is_none_or(|t| self.spell_targets(obj, t, caster))
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
                    // CR 115.4: "any target" is a creature, player, or
                    // planeswalker (battles are not modelled).
                    TargetSpec::Any | TargetSpec::CreatureOrPlaneswalker => {
                        c.is_creature() || c.is_planeswalker()
                    }
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
                    TargetSpec::Filtered(f) => {
                        let f = self.source_filter(self.pool.filter(f), source);
                        f.matches(id, obj.controller, &c, source, Some(caster))
                            && self.status_ok(&f, id)
                    }
                    TargetSpec::SpellFiltered(_)
                    | TargetSpec::GraveyardCard { .. }
                    | TargetSpec::Player
                    | TargetSpec::Opponent
                    | TargetSpec::Spell
                    | TargetSpec::CreatureSpell
                    | TargetSpec::NoncreatureSpell
                    | TargetSpec::UpTo(_)
                    | TargetSpec::Many(_)
                    | TargetSpec::CreatureCardInYourGraveyard => false,
                };
                let hexproof_ok = (!c.has(Keyword::Hexproof) || obj.controller == caster)
                    && !c.has(Keyword::Shroud);
                // CR 702.16b: protection from a quality forbids targeting by
                // spells and abilities from sources with it.
                let protection_ok = !c.keywords.intersects(crate::card::KeywordSet::PROTECTION)
                    || !source
                        .and_then(|s| self.characteristics_or_lki(s))
                        .is_some_and(|sc| c.protected_from(&sc, &self.pool));
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
        assert!(c <= 72, "Characteristics is {c} bytes");
    }
}
