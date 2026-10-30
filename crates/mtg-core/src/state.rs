//! Game state: players, objects, zones, combat, continuous effects, and
//! derived characteristics. A flat, clonable value (design §4.2).

use std::collections::HashMap;
use std::sync::Arc;

use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;
use smallvec::SmallVec;

use crate::card::{
    CardDef, CardPool, CardRef, CardType, Keyword, KeywordSet, SupertypeSet, TypeSet,
};
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
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum LossReason {
    LifeZeroOrLess,
    DrewFromEmptyLibrary,
    Conceded,
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

/// CR 613 layers. M1 uses only 7c; the rest are declared so effects can be
/// placed correctly as they are added.
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

#[derive(Clone, Debug)]
pub enum Modification {
    ModifyPt { power: i32, toughness: i32 },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Duration {
    UntilEndOfTurn,
}

#[derive(Clone, Debug)]
pub struct ContinuousEffect {
    pub layer: Layer,
    pub timestamp: Timestamp,
    pub affected: ObjId,
    pub modification: Modification,
    pub duration: Duration,
}

/// An object's current characteristics (CR 109.3), derived from printed
/// values plus continuous effects.
#[derive(Clone, Debug)]
pub struct Characteristics {
    pub def: CardRef,
    pub colors: ColorSet,
    pub supertypes: SupertypeSet,
    pub types: TypeSet,
    pub power: i32,
    pub toughness: i32,
    pub keywords: KeywordSet,
}

impl Characteristics {
    pub fn is_creature(&self) -> bool {
        self.types.contains(CardType::Creature)
    }

    pub fn has(&self, k: Keyword) -> bool {
        self.keywords.contains(k)
    }
}

/// Last-known information about an object that left its zone (CR 113.7a).
#[derive(Clone, Debug)]
pub struct LastKnown {
    pub object: GameObject,
    pub characteristics: Characteristics,
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
    /// Choosing targets for a spell being cast (CR 601.2c).
    Casting {
        spell: ObjId,
        next_target: usize,
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
    pub lki: HashMap<ObjId, LastKnown>,
    pub rng: ChaCha8Rng,
    pub next_timestamp: Timestamp,
    pub flow: Flow,
    /// Set when a cleanup step must repeat (CR 514.3a).
    pub cleanup_repeat: bool,
    pub result: Option<GameResult>,
    pub log: Vec<Event>,
    pub logging: bool,
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

    pub fn emit(&mut self, event: Event) {
        if self.logging {
            self.log.push(event);
        }
    }

    /// Current characteristics of an object, computed through the layers
    /// (CR 613). M1 applies printed values, then layer 7c modifications in
    /// timestamp order. Objects not on the battlefield have no effects
    /// applied to them except those that explicitly apply elsewhere (none
    /// in M1).
    pub fn characteristics(&self, id: ObjId) -> Option<Characteristics> {
        let obj = self.objects.get(id)?;
        let def = self.pool.get(obj.def);
        let mut c = Characteristics {
            def: obj.def,
            colors: def.colors(),
            supertypes: def.supertypes,
            types: def.types,
            power: def.power.unwrap_or(0),
            toughness: def.toughness.unwrap_or(0),
            keywords: def.keywords,
        };
        if obj.zone == Zone::Battlefield {
            let mut applicable: SmallVec<[&ContinuousEffect; 8]> =
                self.effects.iter().filter(|e| e.affected == id).collect();
            applicable.sort_by_key(|e| (e.layer, e.timestamp));
            for e in applicable {
                match e.modification {
                    Modification::ModifyPt { power, toughness } => {
                        c.power += power;
                        c.toughness += toughness;
                    }
                }
            }
        }
        Some(c)
    }

    /// Characteristics of an object that may have left the game, falling
    /// back to last-known information.
    pub fn characteristics_or_lki(&self, id: ObjId) -> Option<Characteristics> {
        self.characteristics(id)
            .or_else(|| self.lki.get(&id).map(|l| l.characteristics.clone()))
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
