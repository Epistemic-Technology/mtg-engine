//! Game state: players, objects, zones, combat, continuous effects, and
//! derived characteristics. A flat, clonable value (design §4.2).

use std::cell::RefCell;
use std::collections::HashMap;
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
use crate::ids::{ObjId, PlayerId, Timestamp};
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
    /// The object whose action triggered it, if any.
    pub trigger_obj: Option<ObjId>,
    /// The player whose action triggered it, if any.
    pub trigger_player: Option<PlayerId>,
}

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
}

/// Something a spell or ability can target or an effect can act on.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Target {
    Object(ObjId),
    Player(PlayerId),
}

#[derive(Clone, Debug)]
pub struct GameObject {
    pub def: CardRef,
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

impl GameObject {
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
    pub max_hand_size: usize,
    pub drew_from_empty_library: bool,
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
    pub power: i32,
    pub toughness: i32,
    pub keywords: KeywordSet,
    pub abilities: AbilitySet,
}

impl Characteristics {
    pub fn is_creature(&self) -> bool {
        self.types.contains(CardType::Creature)
    }

    pub fn is_land(&self) -> bool {
        self.types.contains(CardType::Land)
    }

    pub fn has(&self, k: Keyword) -> bool {
        self.keywords.contains(k)
    }

    pub fn has_subtype(&self, s: crate::card::Subtype) -> bool {
        self.subtypes.contains(&s)
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
    Battlefield(HashMap<ObjId, Characteristics>),
}

/// Cache of derived characteristics, valid for one `GameState::version`.
/// Cloning a state yields an empty cache; it refills on first use.
#[derive(Default)]
pub struct DerivedCache(pub(crate) RefCell<Option<(u64, Derived)>>);

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
pub struct GameState {
    pub pool: Arc<CardPool>,
    pub version: u64,
    pub turn_number: u32,
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
    pub mid: MidDecision,
    pub lki: HashMap<ObjId, LastKnown>,
    /// Triggered abilities waiting to be put on the stack (CR 603.3).
    pub pending_triggers: Vec<PendingTrigger>,
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

    /// Record an event: triggered abilities that watch it trigger (CR 603.2),
    /// then it is logged.
    pub fn emit(&mut self, event: Event) {
        // Cheap test first: nothing on the battlefield (or just leaving it)
        // can have a triggered ability on most boards.
        let may_trigger = self.trigger_sources > 0
            || self.lookback.is_some()
            || matches!(&event, Event::ZoneChange { def, .. } if self.pool.get(*def).has_triggers());
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

    /// Targets a stack object needs: a spell's cast targets or an ability's.
    pub fn targets_of(&self, id: ObjId) -> &[TargetSpec] {
        let Some(o) = self.objects.get(id) else {
            return &[];
        };
        match o.ability {
            Some(a) => self.pool.ability(a.ability).targets(),
            None => self.pool.get(o.def).cast_targets(),
        }
    }

    /// Effects a stack object applies on resolution.
    pub fn effects_of(&self, id: ObjId) -> &[crate::card::Effect] {
        let Some(o) = self.objects.get(id) else {
            return &[];
        };
        match o.ability {
            Some(a) => self.pool.ability(a.ability).effects(),
            None => self.pool.get(o.def).spell_ability().map_or(&[], |(_, e)| e),
        }
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
    pub fn may_need_mid_decision(&self) -> bool {
        self.replacement_sources as usize + self.replacements.len() >= 2
            || self.legendary_count >= 2
            || (self.legendary_count == 1
                && self
                    .stack
                    .iter()
                    .any(|id| self.pool.get(self.objects[*id].def).is_legendary()))
    }

    /// Current characteristics of an object, computed through the layers
    /// (CR 613). Cached per `version`. Objects off the battlefield have their
    /// printed characteristics (no effect in the engine yet applies
    /// elsewhere).
    pub fn characteristics(&self, id: ObjId) -> Option<Characteristics> {
        let obj = self.objects.get(id)?;
        if obj.zone != Zone::Battlefield {
            return Some(self.printed(obj.def));
        }
        {
            let cache = self.cache.0.borrow();
            if let Some((v, derived)) = cache.as_ref()
                && *v == self.version
            {
                return match derived {
                    Derived::Printed => Some(self.printed(obj.def)),
                    Derived::Battlefield(map) => map.get(&id).copied(),
                };
            }
        }
        let derived = crate::layers::compute_battlefield(self);
        let out = match &derived {
            Derived::Printed => Some(self.printed(obj.def)),
            Derived::Battlefield(map) => map.get(&id).copied(),
        };
        *self.cache.0.borrow_mut() = Some((self.version, derived));
        out
    }

    /// Printed characteristics of a card, before any effect.
    pub fn printed(&self, def: CardRef) -> Characteristics {
        self.pool
            .get(def)
            .printed
            .expect("definitions in a pool have printed characteristics")
    }

    /// Whether an Aura with enchant `spec`, or an Equipment (CR 301.5c,
    /// `TargetSpec::Creature`), may be attached to `target` (CR 303.4c,
    /// 704.5n). Unlike targeting, hexproof and the "you control" of equip
    /// do not matter here.
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
            TargetSpec::Any | TargetSpec::Creature | TargetSpec::CreatureYouControl => {
                c.is_creature()
            }
            TargetSpec::TappedCreature => c.is_creature() && obj.tapped,
            TargetSpec::ArtifactCreatureOrLand => {
                c.is_creature() || c.is_land() || c.types.contains(CardType::Artifact)
            }
            TargetSpec::ArtifactOrEnchantment => {
                c.types.contains(CardType::Artifact) || c.types.contains(CardType::Enchantment)
            }
            TargetSpec::Player | TargetSpec::PlayerOrPlaneswalker => false,
        }
    }

    /// Characteristics of an object that may have left the game, falling
    /// back to last-known information.
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
    /// 115.5, 702.11 hexproof).
    pub fn is_legal_target(
        &self,
        caster: PlayerId,
        spec: crate::card::TargetSpec,
        target: Target,
    ) -> bool {
        use crate::card::TargetSpec;
        match target {
            Target::Player(p) => {
                p.index() < self.num_players()
                    && self.player(p).lost.is_none()
                    && matches!(
                        spec,
                        TargetSpec::Any | TargetSpec::Player | TargetSpec::PlayerOrPlaneswalker
                    )
            }
            Target::Object(id) => {
                let Some(obj) = self.objects.get(id) else {
                    return false;
                };
                if obj.zone != Zone::Battlefield {
                    return false;
                }
                let Some(c) = self.characteristics(id) else {
                    return false;
                };
                let type_ok = match spec {
                    TargetSpec::Any | TargetSpec::Creature => c.is_creature(),
                    TargetSpec::TappedCreature => c.is_creature() && obj.tapped,
                    TargetSpec::CreatureYouControl => c.is_creature() && obj.controller == caster,
                    TargetSpec::ArtifactCreatureOrLand => {
                        c.is_creature() || c.is_land() || c.types.contains(CardType::Artifact)
                    }
                    TargetSpec::ArtifactOrEnchantment => {
                        c.types.contains(CardType::Artifact)
                            || c.types.contains(CardType::Enchantment)
                    }
                    TargetSpec::Player | TargetSpec::PlayerOrPlaneswalker => false,
                };
                let hexproof_ok = !(c.has(Keyword::Hexproof) && obj.controller != caster);
                type_ok && hexproof_ok
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
        eprintln!("GameObject {o} bytes, Characteristics {c} bytes");
        assert!(o <= 136, "GameObject is {o} bytes");
        assert!(c <= 72, "Characteristics is {c} bytes");
    }
}
