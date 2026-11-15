//! The engine: a resumable step function over `GameState` (design §4.6–4.7).
//! `Engine::new` runs until the first decision; `Engine::step` applies a
//! choice and runs until the next one or the end of the game.

use std::sync::Arc;

use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;
use smallvec::SmallVec;
use thiserror::Error;

use crate::card::{AbilityDef, CardPool, CardRef, Effect, Keyword, PlayerRef, TargetSpec};
use crate::decision::{Choice, ChoiceId, Decision, DecisionKind, StepResult};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::mana::{Mana, ManaRequirement};
use crate::state::{
    Attacker, CombatState, ContinuousEffect, DamageAssignment, Duration, Flow, GameObject,
    GameResult, GameState, LastKnown, Layer, LossReason, Modification, ObjKind, PlayerState,
    Target, Zone,
};
use crate::turn::Step;

/// Above this many enumerated options, attacker and blocker declarations
/// fall back to incremental mode (design §4.7, open question 2).
pub const MAX_SET_OPTIONS: usize = 1024;
/// Above this many combat damage divisions, only canonical divisions are
/// offered.
pub const MAX_DIVISION_OPTIONS: usize = 512;
const MAX_MULLIGANS: u8 = 7;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameConfig {
    /// One deck per player, as card references into the pool.
    pub decks: Vec<Vec<CardRef>>,
    pub starting_life: i32,
    pub seed: u64,
    /// Record events in `GameState::log`.
    pub logging: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EngineError {
    #[error("game is over")]
    GameOver,
    #[error("choice {choice} out of range (decision has {options} options)")]
    BadChoice { choice: ChoiceId, options: usize },
}

/// A prepared game position for scenario tests. See `Engine::from_setup`.
#[derive(Clone, Debug)]
pub struct Setup {
    pub players: Vec<PlayerSetup>,
    pub turn_number: u32,
    /// The active player; also treated as the starting player.
    pub active_player: PlayerId,
    /// Play begins with this step's turn-based actions.
    pub step: Step,
    pub seed: u64,
    pub logging: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PlayerSetup {
    pub life: i32,
    /// Bottom first; the last card is on top. Not shuffled.
    pub library: Vec<CardRef>,
    pub hand: Vec<CardRef>,
    /// (card, tapped).
    pub battlefield: Vec<(CardRef, bool)>,
    pub graveyard: Vec<CardRef>,
    pub exile: Vec<CardRef>,
}

pub struct Engine {
    state: GameState,
    current: StepResult,
    history: Vec<ChoiceId>,
}

impl Engine {
    /// Start a game and run until the first decision.
    pub fn new(pool: Arc<CardPool>, config: &GameConfig) -> Engine {
        assert_eq!(config.decks.len(), 2, "M1 supports exactly two players");
        let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
        let starting_player = PlayerId(u8::from(rand::Rng::random_bool(&mut rng, 0.5)));
        let mut state = GameState::blank(
            pool,
            starting_player,
            &[config.starting_life; 2],
            rng,
            config.logging,
        );
        state.flow = Flow::Mulligan {
            kept: [false; 2],
            mulled: [false; 2],
            decided: [false; 2],
            current: starting_player,
        };
        for (i, deck) in config.decks.iter().enumerate() {
            let p = PlayerId(u8::try_from(i).unwrap_or(0));
            for def in deck {
                let id = state.new_object(*def, p, Zone::Library);
                state.player_mut(p).library.push(id);
            }
            state.shuffle_library(p);
        }
        state.emit(Event::GameStart { starting_player });
        for p in [PlayerId(0), PlayerId(1)] {
            for _ in 0..7 {
                state.draw(p);
            }
        }
        let current = state.run();
        Engine {
            state,
            current,
            history: Vec::new(),
        }
    }

    /// Start from a prepared position (design §7.1): objects are placed
    /// directly in their zones, there are no mulligans, and play begins with
    /// the turn-based actions of `setup.step`. Permanents placed on the
    /// battlefield count as controlled since the turn began.
    pub fn from_setup(pool: Arc<CardPool>, setup: &Setup) -> Engine {
        assert_eq!(setup.players.len(), 2, "M1 supports exactly two players");
        let lives: Vec<i32> = setup.players.iter().map(|p| p.life).collect();
        let rng = ChaCha8Rng::seed_from_u64(setup.seed);
        let mut state = GameState::blank(pool, setup.active_player, &lives, rng, setup.logging);
        for (i, ps) in setup.players.iter().enumerate() {
            let p = PlayerId(u8::try_from(i).unwrap_or(0));
            for def in &ps.library {
                let id = state.new_object(*def, p, Zone::Library);
                state.player_mut(p).library.push(id);
            }
            for def in &ps.hand {
                let id = state.new_object(*def, p, Zone::Hand);
                state.player_mut(p).hand.push(id);
            }
            for def in &ps.graveyard {
                let id = state.new_object(*def, p, Zone::Graveyard);
                state.player_mut(p).graveyard.push(id);
            }
            for def in &ps.exile {
                let id = state.new_object(*def, p, Zone::Exile);
                state.player_mut(p).exile.push(id);
            }
            for (def, tapped) in &ps.battlefield {
                let id = state.new_object(*def, p, Zone::Battlefield);
                let o = &mut state.objects[id];
                o.tapped = *tapped;
                o.controlled_since_turn_start = true;
                state.battlefield.push(id);
            }
        }
        state.turn_number = setup.turn_number;
        state.step = setup.step;
        state.emit(Event::GameStart {
            starting_player: setup.active_player,
        });
        state.emit(Event::TurnBegin {
            turn: setup.turn_number,
            player: setup.active_player,
        });
        state.flow = Flow::BeginStep;
        let current = state.run();
        Engine {
            state,
            current,
            history: Vec::new(),
        }
    }

    pub fn current(&self) -> &StepResult {
        &self.current
    }

    pub fn state(&self) -> &GameState {
        &self.state
    }

    pub fn history(&self) -> &[ChoiceId] {
        &self.history
    }

    pub fn is_over(&self) -> bool {
        self.current.is_over()
    }

    /// Apply the chosen option of the current decision and run to the next.
    pub fn step(&mut self, choice: ChoiceId) -> Result<&StepResult, EngineError> {
        let decision = match &self.current {
            StepResult::Decision(d) => d,
            StepResult::Over(_) => return Err(EngineError::GameOver),
        };
        let Some(option) = decision.options.get(choice as usize) else {
            return Err(EngineError::BadChoice {
                choice,
                options: decision.options.len(),
            });
        };
        let player = decision.player;
        let option = option.clone();
        self.history.push(choice);
        self.state.apply_choice(player, option);
        self.current = self.state.run();
        Ok(&self.current)
    }
}

impl GameState {
    /// An empty game: players with the given life totals, no objects, turn 0
    /// at the untap step, flow unset (`BeginStep`).
    fn blank(
        pool: Arc<CardPool>,
        starting_player: PlayerId,
        lives: &[i32],
        rng: ChaCha8Rng,
        logging: bool,
    ) -> GameState {
        GameState {
            pool,
            version: 0,
            turn_number: 0,
            active_player: starting_player,
            starting_player,
            step: Step::Untap,
            priority: starting_player,
            consecutive_passes: 0,
            players: lives
                .iter()
                .map(|life| PlayerState {
                    life: *life,
                    library: Vec::new(),
                    hand: Vec::new(),
                    graveyard: Vec::new(),
                    exile: Vec::new(),
                    mana_pool: crate::mana::ManaPool::default(),
                    lands_played_this_turn: 0,
                    max_hand_size: 7,
                    drew_from_empty_library: false,
                    lost: None,
                    mulligans_taken: 0,
                })
                .collect(),
            objects: SlotMap::with_key(),
            battlefield: Vec::new(),
            stack: Vec::new(),
            combat: None,
            effects: Vec::new(),
            lki: std::collections::HashMap::default(),
            rng,
            next_timestamp: 1,
            flow: Flow::BeginStep,
            cleanup_repeat: false,
            result: None,
            log: Vec::new(),
            logging,
        }
    }

    // ----- objects and zones -------------------------------------------------

    fn new_object(&mut self, def: CardRef, owner: PlayerId, zone: Zone) -> ObjId {
        let timestamp = self.tick();
        self.objects.insert(GameObject {
            def,
            kind: ObjKind::Card,
            owner,
            controller: owner,
            zone,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
            timestamp,
            targets: SmallVec::new(),
        })
    }

    fn remove_from_zone_list(&mut self, id: ObjId) {
        let obj = &self.objects[id];
        let (zone, owner) = (obj.zone, obj.owner);
        let list: &mut Vec<ObjId> = match zone {
            Zone::Battlefield => &mut self.battlefield,
            Zone::Stack => &mut self.stack,
            Zone::Command => return,
            z => self.player_mut(owner).zone_mut(z),
        };
        if let Some(i) = list.iter().position(|x| *x == id) {
            list.remove(i);
        }
    }

    /// Move an object to another zone. It becomes a new object with a new id
    /// (CR 400.7); the old id is remembered in `lki`. `to_top` places it on
    /// top of a library or stack; otherwise at the bottom.
    fn move_object(&mut self, id: ObjId, to: Zone, to_top: bool) -> ObjId {
        let characteristics = self
            .characteristics(id)
            .expect("moving an object that exists");
        self.remove_from_zone_list(id);
        let old = self.objects.remove(id).expect("object exists");
        self.lki.insert(
            id,
            LastKnown {
                object: old.clone(),
                characteristics,
            },
        );
        self.effects.retain(|e| e.affected != id);
        let timestamp = self.tick();
        let new = self.objects.insert(GameObject {
            def: old.def,
            kind: old.kind,
            owner: old.owner,
            controller: old.owner,
            zone: to,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
            timestamp,
            targets: SmallVec::new(),
        });
        match to {
            Zone::Battlefield => self.battlefield.push(new),
            Zone::Stack => self.stack.push(new),
            Zone::Command => {}
            Zone::Library => {
                let lib = &mut self.player_mut(old.owner).library;
                if to_top {
                    lib.push(new);
                } else {
                    lib.insert(0, new);
                }
            }
            z => self.player_mut(old.owner).zone_mut(z).push(new),
        }
        self.bump();
        self.emit(Event::ZoneChange {
            old: id,
            new,
            def: old.def,
            from: old.zone,
            to,
        });
        new
    }

    fn shuffle_library(&mut self, p: PlayerId) {
        let mut lib = std::mem::take(&mut self.player_mut(p).library);
        lib.shuffle(&mut self.rng);
        self.player_mut(p).library = lib;
        self.bump();
    }

    fn draw(&mut self, p: PlayerId) {
        if let Some(top) = self.player(p).library.last().copied() {
            let new = self.move_object(top, Zone::Hand, false);
            self.emit(Event::Draw {
                player: p,
                card: new,
            });
        } else {
            self.player_mut(p).drew_from_empty_library = true;
            self.emit(Event::DrawFromEmptyLibrary { player: p });
        }
    }

    fn tap(&mut self, id: ObjId) {
        if let Some(o) = self.objects.get_mut(id)
            && !o.tapped
        {
            o.tapped = true;
            self.bump();
            self.emit(Event::Tapped { obj: id });
        }
    }

    fn untap(&mut self, id: ObjId) {
        if let Some(o) = self.objects.get_mut(id)
            && o.tapped
        {
            o.tapped = false;
            self.bump();
            self.emit(Event::Untapped { obj: id });
        }
    }

    fn change_life(&mut self, p: PlayerId, delta: i32) {
        if delta == 0 {
            return;
        }
        let ps = self.player_mut(p);
        ps.life += delta;
        let life = ps.life;
        self.bump();
        self.emit(Event::LifeChange {
            player: p,
            delta,
            life,
        });
    }

    /// Deal damage from `source` to `target` (CR 120.3). Lifelink applies
    /// (CR 702.15); deathtouch is remembered for the next SBA check.
    fn deal_damage(&mut self, source: ObjId, target: Target, amount: u32, combat: bool) {
        if amount == 0 {
            return;
        }
        let src = self.characteristics_or_lki(source);
        let lifelink = src.as_ref().is_some_and(|c| c.has(Keyword::Lifelink));
        let deathtouch = src.as_ref().is_some_and(|c| c.has(Keyword::Deathtouch));
        let controller = self
            .objects
            .get(source)
            .map(|o| o.controller)
            .or_else(|| self.lki.get(&source).map(|l| l.object.controller));
        match target {
            Target::Player(p) => {
                self.emit(Event::Damage {
                    source,
                    target,
                    amount,
                    combat,
                });
                let delta = -i32::try_from(amount).unwrap_or(i32::MAX);
                self.change_life(p, delta);
            }
            Target::Object(id) => {
                let Some(o) = self.objects.get_mut(id) else {
                    return;
                };
                if o.zone != Zone::Battlefield {
                    return;
                }
                o.damage += amount;
                if deathtouch {
                    o.deathtouch_damage = true;
                }
                self.bump();
                self.emit(Event::Damage {
                    source,
                    target,
                    amount,
                    combat,
                });
            }
        }
        if lifelink && let Some(c) = controller {
            self.change_life(c, i32::try_from(amount).unwrap_or(i32::MAX));
        }
    }

    fn destroy(&mut self, id: ObjId) {
        let Some(c) = self.characteristics(id) else {
            return;
        };
        if c.has(Keyword::Indestructible) {
            return;
        }
        let def = c.def;
        self.emit(Event::Destroyed { obj: id, def });
        self.move_object(id, Zone::Graveyard, true);
    }

    // ----- the main loop -------------------------------------------------------

    /// Advance until a decision is needed or the game ends.
    #[allow(clippy::too_many_lines)] // one arm per `Flow` variant
    fn run(&mut self) -> StepResult {
        loop {
            if let Some(result) = self.check_game_over() {
                return StepResult::Over(result);
            }
            match self.flow.clone() {
                Flow::GameOver => {
                    return StepResult::Over(self.result.unwrap_or(GameResult::Draw));
                }
                Flow::Mulligan { current, .. } => {
                    let mut options = vec![Choice::Keep];
                    if self.player(current).mulligans_taken < MAX_MULLIGANS {
                        options.push(Choice::Mulligan);
                    }
                    return StepResult::Decision(Decision {
                        player: current,
                        kind: DecisionKind::Mulligan,
                        options,
                    });
                }
                Flow::Bottoming { current, remaining } => {
                    if remaining == 0 || self.player(current).hand.is_empty() {
                        let bottomed = self.player(current).mulligans_taken;
                        self.emit(Event::HandKept {
                            player: current,
                            bottomed,
                        });
                        let next = self.next_player(current);
                        if next == self.starting_player {
                            self.begin_game();
                        } else {
                            self.flow = Flow::Bottoming {
                                current: next,
                                remaining: self.player(next).mulligans_taken,
                            };
                        }
                        continue;
                    }
                    let options = self
                        .player(current)
                        .hand
                        .iter()
                        .map(|id| Choice::Bottom(*id))
                        .collect();
                    return StepResult::Decision(Decision {
                        player: current,
                        kind: DecisionKind::Bottom,
                        options,
                    });
                }
                Flow::BeginStep => self.begin_step(),
                Flow::Priority => {
                    if self.sba_loop() {
                        continue; // game-over check first, then re-enter
                    }
                    let player = self.priority;
                    let options = self.priority_options(player);
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::PriorityAction,
                        options,
                    });
                }
                Flow::Casting { spell, next_target } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let caster = obj.controller;
                    let def = self.pool.get(obj.def);
                    let specs: Vec<TargetSpec> = def
                        .spell_ability()
                        .map(|(t, _)| t.to_vec())
                        .unwrap_or_default();
                    if next_target >= specs.len() {
                        self.finish_cast(spell);
                        continue;
                    }
                    let options: Vec<Choice> = self
                        .legal_targets(caster, specs[next_target])
                        .into_iter()
                        .map(Choice::Target)
                        .collect();
                    if options.is_empty() {
                        unreachable!("cast offered without legal targets");
                    }
                    return StepResult::Decision(Decision {
                        player: caster,
                        kind: DecisionKind::ChooseTarget {
                            spell,
                            index: next_target,
                        },
                        options,
                    });
                }
                Flow::DeclareAttackers {
                    chosen,
                    incremental,
                } => {
                    let player = self.active_player;
                    let eligible: Vec<ObjId> = self
                        .eligible_attackers(player)
                        .filter(|id| !chosen.contains(id))
                        .collect();
                    if incremental {
                        let mut options: Vec<Choice> =
                            eligible.into_iter().map(Choice::AddAttacker).collect();
                        options.push(Choice::Done);
                        return StepResult::Decision(Decision {
                            player,
                            kind: DecisionKind::DeclareAttackers,
                            options,
                        });
                    }
                    if eligible.len() > 10 || (1usize << eligible.len()) > MAX_SET_OPTIONS {
                        self.flow = Flow::DeclareAttackers {
                            chosen,
                            incremental: true,
                        };
                        continue;
                    }
                    let options: Vec<Choice> = subsets(&eligible)
                        .into_iter()
                        .map(Choice::Attackers)
                        .collect();
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::DeclareAttackers,
                        options,
                    });
                }
                Flow::DeclareBlockers {
                    chosen,
                    incremental,
                } => {
                    let player = self.opponent(self.active_player);
                    let attackers: Vec<ObjId> = self
                        .combat
                        .as_ref()
                        .map(|c| c.attackers.iter().map(|a| a.id).collect())
                        .unwrap_or_default();
                    let blockers: Vec<ObjId> = self
                        .eligible_blockers(player)
                        .filter(|b| !chosen.iter().any(|(bl, _)| bl == b))
                        .collect();
                    // Per blocker, the attackers it may block.
                    let can_block: Vec<Vec<ObjId>> = blockers
                        .iter()
                        .map(|b| {
                            attackers
                                .iter()
                                .copied()
                                .filter(|a| self.can_block(*b, *a))
                                .collect()
                        })
                        .collect();
                    if incremental {
                        let mut options = Vec::new();
                        for (b, attackers_for_b) in blockers.iter().zip(&can_block) {
                            for a in attackers_for_b {
                                let mut trial = chosen.clone();
                                trial.push((*b, *a));
                                if self.block_set_completable(&trial, &blockers, &can_block) {
                                    options.push(Choice::AddBlock {
                                        blocker: *b,
                                        attacker: *a,
                                    });
                                }
                            }
                        }
                        if self.block_set_legal(&chosen) {
                            options.push(Choice::Done);
                        }
                        if options.is_empty() {
                            // Painted into a corner (menace); drop illegal blocks and finish.
                            let fixed: Vec<(ObjId, ObjId)> = chosen
                                .iter()
                                .copied()
                                .filter(|(_, a)| {
                                    chosen.iter().filter(|(_, a2)| a2 == a).count() >= 2
                                        || !self.has_menace(*a)
                                })
                                .collect();
                            self.finish_blocks(fixed);
                            continue;
                        }
                        return StepResult::Decision(Decision {
                            player,
                            kind: DecisionKind::DeclareBlockers,
                            options,
                        });
                    }
                    let combos: usize = can_block.iter().map(|v| v.len() + 1).product();
                    if combos > MAX_SET_OPTIONS {
                        self.flow = Flow::DeclareBlockers {
                            chosen,
                            incremental: true,
                        };
                        continue;
                    }
                    let mut options = Vec::new();
                    for assignment in block_assignments(&blockers, &can_block) {
                        if self.block_set_legal(&assignment) {
                            options.push(Choice::Blocks(assignment));
                        }
                    }
                    if options.is_empty() {
                        unreachable!("no legal block declaration");
                    }
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::DeclareBlockers,
                        options,
                    });
                }
                Flow::AssignDamage { queue, assigned } => {
                    let Some(attacker) = queue.first().copied() else {
                        self.deal_combat_damage(assigned);
                        continue;
                    };
                    let player = self.objects[attacker].controller;
                    let options: Vec<Choice> = self
                        .damage_divisions(attacker)
                        .into_iter()
                        .map(Choice::Division)
                        .collect();
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::AssignCombatDamage { attacker },
                        options,
                    });
                }
                Flow::Discarding => {
                    let p = self.active_player;
                    let ps = self.player(p);
                    if ps.hand.len() <= ps.max_hand_size {
                        self.finish_cleanup();
                        continue;
                    }
                    let options = ps.hand.iter().map(|id| Choice::Discard(*id)).collect();
                    return StepResult::Decision(Decision {
                        player: p,
                        kind: DecisionKind::Discard,
                        options,
                    });
                }
                Flow::EndStep => self.end_step(),
            }
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per (`Flow`, `Choice`) pair
    fn apply_choice(&mut self, player: PlayerId, choice: Choice) {
        match (self.flow.clone(), choice) {
            (
                Flow::Mulligan {
                    mut kept,
                    mut mulled,
                    mut decided,
                    current,
                },
                c @ (Choice::Keep | Choice::Mulligan),
            ) => {
                debug_assert_eq!(current, player);
                let i = current.index();
                decided[i] = true;
                if c == Choice::Keep {
                    kept[i] = true;
                } else {
                    mulled[i] = true;
                }
                // Next undecided player this round, in turn order.
                let mut next = self.next_player(current);
                let mut found = None;
                for _ in 0..self.num_players() {
                    if !kept[next.index()] && !decided[next.index()] {
                        found = Some(next);
                        break;
                    }
                    next = self.next_player(next);
                }
                if let Some(next) = found {
                    self.flow = Flow::Mulligan {
                        kept,
                        mulled,
                        decided,
                        current: next,
                    };
                    return;
                }
                // Round over: everyone who mulliganed does so simultaneously.
                for (i, _) in mulled.iter().enumerate().filter(|(_, m)| **m) {
                    {
                        let p = PlayerId(u8::try_from(i).unwrap_or(0));
                        let hand = std::mem::take(&mut self.player_mut(p).hand);
                        for id in hand {
                            self.player_mut(p).hand.push(id);
                            self.move_object(id, Zone::Library, true);
                        }
                        self.shuffle_library(p);
                        for _ in 0..7 {
                            self.draw(p);
                        }
                        self.player_mut(p).mulligans_taken += 1;
                        let count = self.player(p).mulligans_taken;
                        self.emit(Event::MulliganTaken { player: p, count });
                    }
                }
                if kept.iter().all(|k| *k) {
                    let s = self.starting_player;
                    self.flow = Flow::Bottoming {
                        current: s,
                        remaining: self.player(s).mulligans_taken,
                    };
                } else {
                    let mut first = self.starting_player;
                    while kept[first.index()] {
                        first = self.next_player(first);
                    }
                    self.flow = Flow::Mulligan {
                        kept,
                        mulled: [false; 2],
                        decided: [false; 2],
                        current: first,
                    };
                }
            }
            (Flow::Bottoming { current, remaining }, Choice::Bottom(id)) => {
                self.move_object(id, Zone::Library, false);
                self.flow = Flow::Bottoming {
                    current,
                    remaining: remaining.saturating_sub(1),
                };
            }
            (Flow::Priority, Choice::Pass) => {
                self.emit(Event::Pass { player });
                self.consecutive_passes += 1;
                if usize::from(self.consecutive_passes) >= self.players_in_game().count() {
                    self.consecutive_passes = 0;
                    if self.stack.is_empty() {
                        self.flow = Flow::EndStep;
                    } else {
                        self.resolve_top();
                        self.priority = self.active_player;
                        self.flow = Flow::Priority;
                    }
                } else {
                    self.priority = self.next_player_in_game(player);
                }
            }
            (Flow::Priority, Choice::PlayLand(id)) => {
                self.consecutive_passes = 0;
                let new = self.move_object(id, Zone::Battlefield, true);
                self.player_mut(player).lands_played_this_turn += 1;
                self.emit(Event::LandPlayed { player, land: new });
            }
            (Flow::Priority, Choice::Cast(id)) => {
                self.consecutive_passes = 0;
                let spell = self.move_object(id, Zone::Stack, true);
                self.objects[spell].controller = player;
                self.flow = Flow::Casting {
                    spell,
                    next_target: 0,
                };
            }
            (Flow::Casting { spell, next_target }, Choice::Target(t)) => {
                self.objects[spell].targets.push(t);
                self.flow = Flow::Casting {
                    spell,
                    next_target: next_target + 1,
                };
            }
            (Flow::DeclareAttackers { .. }, Choice::Attackers(set)) => {
                self.finish_attackers(set);
            }
            (
                Flow::DeclareAttackers {
                    mut chosen,
                    incremental,
                },
                Choice::AddAttacker(id),
            ) => {
                chosen.push(id);
                self.flow = Flow::DeclareAttackers {
                    chosen,
                    incremental,
                };
            }
            (Flow::DeclareAttackers { chosen, .. }, Choice::Done) => {
                self.finish_attackers(chosen);
            }
            (Flow::DeclareBlockers { .. }, Choice::Blocks(set)) => {
                self.finish_blocks(set);
            }
            (
                Flow::DeclareBlockers {
                    mut chosen,
                    incremental,
                },
                Choice::AddBlock { blocker, attacker },
            ) => {
                chosen.push((blocker, attacker));
                self.flow = Flow::DeclareBlockers {
                    chosen,
                    incremental,
                };
            }
            (Flow::DeclareBlockers { chosen, .. }, Choice::Done) => {
                self.finish_blocks(chosen);
            }
            (
                Flow::AssignDamage {
                    mut queue,
                    mut assigned,
                },
                Choice::Division(div),
            ) => {
                let source = queue.remove(0);
                for (target, amount) in div {
                    assigned.push(DamageAssignment {
                        source,
                        target,
                        amount,
                    });
                }
                self.flow = Flow::AssignDamage { queue, assigned };
            }
            (Flow::Discarding, Choice::Discard(id)) => {
                self.emit(Event::Discard { player, card: id });
                self.move_object(id, Zone::Graveyard, true);
            }
            (flow, choice) => unreachable!("choice {choice:?} does not fit flow {flow:?}"),
        }
    }

    fn next_player_in_game(&self, p: PlayerId) -> PlayerId {
        let mut next = self.next_player(p);
        for _ in 0..self.num_players() {
            if self.player(next).lost.is_none() {
                return next;
            }
            next = self.next_player(next);
        }
        p
    }

    // ----- game start and turn structure -----------------------------------------

    fn begin_game(&mut self) {
        self.turn_number = 1;
        self.active_player = self.starting_player;
        self.step = Step::Untap;
        self.emit(Event::TurnBegin {
            turn: 1,
            player: self.starting_player,
        });
        self.flow = Flow::BeginStep;
    }

    /// Turn-based actions at the start of the current step (CR 500.1 and the
    /// step rules), then priority or straight to the end of the step.
    fn begin_step(&mut self) {
        self.emit(Event::StepBegin { step: self.step });
        let active = self.active_player;
        match self.step {
            Step::Untap => {
                // CR 502.2–502.3: untap all permanents of the active player.
                let perms: Vec<ObjId> = self.permanents_of(active).collect();
                for id in perms {
                    self.objects[id].controlled_since_turn_start = true;
                    self.untap(id);
                }
                self.player_mut(active).lands_played_this_turn = 0;
                self.flow = Flow::EndStep;
            }
            Step::Draw => {
                // CR 103.8a: the starting player skips their first draw.
                if !(self.turn_number == 1 && active == self.starting_player) {
                    self.draw(active);
                }
                self.give_priority(active);
            }
            Step::BeginningOfCombat => {
                self.combat = Some(CombatState::default());
                self.give_priority(active);
            }
            Step::DeclareAttackers => {
                if self.eligible_attackers(active).next().is_some() {
                    self.flow = Flow::DeclareAttackers {
                        chosen: Vec::new(),
                        incremental: false,
                    };
                } else {
                    // CR 508.8: no attackers, skip blockers and damage steps.
                    self.flow = Flow::EndStep;
                }
            }
            Step::DeclareBlockers => {
                let defender = self.opponent(active);
                let any_attackers = self
                    .combat
                    .as_ref()
                    .is_some_and(|c| !c.attackers.is_empty());
                if !any_attackers {
                    self.flow = Flow::EndStep;
                } else if self.eligible_blockers(defender).next().is_some() {
                    self.flow = Flow::DeclareBlockers {
                        chosen: Vec::new(),
                        incremental: false,
                    };
                } else {
                    self.finish_blocks(Vec::new());
                }
            }
            Step::FirstStrikeDamage => {
                let any_attackers = self
                    .combat
                    .as_ref()
                    .is_some_and(|c| !c.attackers.is_empty());
                if any_attackers && self.any_first_strike_in_combat() {
                    if let Some(c) = self.combat.as_mut() {
                        c.had_first_strike_step = true;
                    }
                    self.start_damage_assignment(true);
                } else {
                    self.flow = Flow::EndStep;
                }
            }
            Step::CombatDamage => {
                let any_attackers = self
                    .combat
                    .as_ref()
                    .is_some_and(|c| !c.attackers.is_empty());
                if any_attackers {
                    self.start_damage_assignment(false);
                } else {
                    self.flow = Flow::EndStep;
                }
            }
            Step::Cleanup => {
                self.flow = Flow::Discarding;
            }
            Step::Upkeep
            | Step::PrecombatMain
            | Step::EndOfCombat
            | Step::PostcombatMain
            | Step::End => self.give_priority(active),
        }
    }

    fn give_priority(&mut self, p: PlayerId) {
        self.priority = p;
        self.consecutive_passes = 0;
        self.flow = Flow::Priority;
    }

    /// CR 514.2 onward: remove damage, end "until end of turn" effects, then
    /// the 514.3a check.
    fn finish_cleanup(&mut self) {
        for (_, o) in &mut self.objects {
            o.damage = 0;
            o.deathtouch_damage = false;
        }
        let ended: Vec<ObjId> = self
            .effects
            .iter()
            .filter(|e| e.duration == Duration::UntilEndOfTurn)
            .map(|e| e.affected)
            .collect();
        self.effects
            .retain(|e| e.duration != Duration::UntilEndOfTurn);
        for affected in ended {
            self.emit(Event::EffectEnded { affected });
        }
        self.bump();
        if self.sba_loop() {
            // CR 514.3a: players get priority, then another cleanup step.
            self.cleanup_repeat = true;
            self.give_priority(self.active_player);
        } else {
            self.flow = Flow::EndStep;
        }
    }

    /// The current step ends (CR 500.2): mana empties, combat clears, and the
    /// next step or turn begins.
    fn end_step(&mut self) {
        for p in &mut self.players {
            p.mana_pool.empty();
        }
        if self.step == Step::EndOfCombat {
            self.combat = None;
        }
        if self.step == Step::Cleanup && self.cleanup_repeat {
            self.cleanup_repeat = false;
            self.flow = Flow::BeginStep;
            return;
        }
        if let Some(next) = self.step.next() {
            self.step = next;
        } else {
            self.lki.clear();
            self.turn_number += 1;
            self.active_player = self.next_player_in_game(self.active_player);
            self.step = Step::Untap;
            self.emit(Event::TurnBegin {
                turn: self.turn_number,
                player: self.active_player,
            });
        }
        self.flow = Flow::BeginStep;
    }

    // ----- priority actions --------------------------------------------------------

    fn priority_options(&self, p: PlayerId) -> Vec<Choice> {
        let mut options = Vec::new();
        let sorcery_timing =
            p == self.active_player && self.step.is_main() && self.stack.is_empty();
        let ps = self.player(p);
        for id in &ps.hand {
            let def = self.pool.get(self.objects[*id].def);
            if def.is_land() {
                if sorcery_timing && ps.lands_played_this_turn < 1 {
                    options.push(Choice::PlayLand(*id));
                }
                continue;
            }
            let instant_speed = def.is_instant() || def.keywords.contains(Keyword::Flash);
            if !(instant_speed || sorcery_timing) {
                continue;
            }
            if !self.can_pay(p, &def.mana_cost.requirements(0)) {
                continue;
            }
            let targets_ok = def.spell_ability().is_none_or(|(specs, _)| {
                specs.iter().all(|s| !self.legal_targets(p, *s).is_empty())
            });
            if targets_ok {
                options.push(Choice::Cast(*id));
            }
        }
        options.push(Choice::Pass);
        options
    }

    fn legal_targets(&self, caster: PlayerId, spec: TargetSpec) -> Vec<Target> {
        let mut out = Vec::new();
        for id in &self.battlefield {
            let t = Target::Object(*id);
            if self.is_legal_target(caster, spec, t) {
                out.push(t);
            }
        }
        for p in self.players_in_game() {
            let t = Target::Player(p);
            if self.is_legal_target(caster, spec, t) {
                out.push(t);
            }
        }
        out
    }

    /// Untapped mana sources `p` controls, as (object, mana produced). M1:
    /// basic lands only.
    fn mana_sources(&self, p: PlayerId) -> Vec<(ObjId, Mana)> {
        self.permanents_of(p)
            .filter(|id| !self.objects[*id].tapped)
            .filter_map(|id| {
                let def = self.pool.get(self.objects[id].def);
                def.abilities.iter().find_map(|a| match a {
                    AbilityDef::BasicLandMana(c) => Some((id, Mana::Colored(*c))),
                    AbilityDef::Spell { .. } => None,
                })
            })
            .collect()
    }

    /// A canonical payment plan: which sources to tap so that pool plus their
    /// mana covers `req`. With single-color sources a greedy assignment
    /// (colored symbols first, then generic from whatever is left) is exact.
    fn payment_plan(&self, p: PlayerId, req: &ManaRequirement) -> Option<Vec<ObjId>> {
        let mut pool = self.player(p).mana_pool;
        let mut sources = self.mana_sources(p);
        let mut tapped = Vec::new();
        for (ci, need) in req.colored.iter().enumerate() {
            let mut need = *need;
            let from_pool = need.min(pool.colored[ci]);
            pool.colored[ci] -= from_pool;
            need -= from_pool;
            while need > 0 {
                let pos = sources
                    .iter()
                    .position(|(_, m)| matches!(m, Mana::Colored(c) if *c as usize == ci))?;
                tapped.push(sources.remove(pos).0);
                need -= 1;
            }
        }
        let mut need = req.colorless;
        let from_pool = need.min(pool.colorless);
        pool.colorless -= from_pool;
        need -= from_pool;
        while need > 0 {
            let pos = sources.iter().position(|(_, m)| *m == Mana::Colorless)?;
            tapped.push(sources.remove(pos).0);
            need -= 1;
        }
        let mut generic = req.generic;
        let from_pool = generic.min(pool.total());
        generic -= from_pool;
        while generic > 0 {
            if sources.is_empty() {
                return None;
            }
            tapped.push(sources.remove(0).0);
            generic -= 1;
        }
        Some(tapped)
    }

    fn can_pay(&self, p: PlayerId, req: &ManaRequirement) -> bool {
        self.payment_plan(p, req).is_some()
    }

    /// CR 601.2g–h: activate mana abilities per the plan, then pay from the
    /// pool.
    fn pay(&mut self, p: PlayerId, req: &ManaRequirement) {
        let plan = self
            .payment_plan(p, req)
            .expect("cast was offered only when payable");
        for id in plan {
            let mana = self
                .mana_sources(p)
                .into_iter()
                .find(|(s, _)| *s == id)
                .map(|(_, m)| m)
                .expect("planned source produces mana");
            self.tap(id);
            self.player_mut(p).mana_pool.add(mana);
        }
        let pool = self.player(p).mana_pool;
        let left = pool
            .payment_plan(req)
            .expect("pool covers the cost after activating mana abilities");
        self.player_mut(p).mana_pool = left;
        self.bump();
    }

    /// CR 601.2f–i: pay and finish casting; caster gets priority.
    fn finish_cast(&mut self, spell: ObjId) {
        let obj = &self.objects[spell];
        let (caster, def_ref) = (obj.controller, obj.def);
        let targets: Vec<Target> = obj.targets.iter().copied().collect();
        let req = self.pool.get(def_ref).mana_cost.requirements(0);
        self.pay(caster, &req);
        self.emit(Event::Cast {
            player: caster,
            spell,
            def: def_ref,
            targets,
        });
        self.give_priority(caster);
    }

    // ----- resolution -------------------------------------------------------------

    /// CR 608: resolve the top object of the stack.
    fn resolve_top(&mut self) {
        let Some(spell) = self.stack.last().copied() else {
            return;
        };
        let obj = self.objects[spell].clone();
        let def = self.pool.get(obj.def).clone();
        if let Some((specs, effects)) = def.spell_ability() {
            // CR 608.2b: targets that are no longer legal are ignored; if all
            // are illegal the spell does not resolve.
            let legal: Vec<bool> = specs
                .iter()
                .zip(obj.targets.iter())
                .map(|(spec, t)| self.is_legal_target(obj.controller, *spec, *t))
                .collect();
            if !specs.is_empty() && legal.iter().all(|l| !l) {
                self.emit(Event::Fizzle {
                    spell,
                    def: obj.def,
                });
                self.move_object(spell, Zone::Graveyard, true);
                return;
            }
            self.emit(Event::Resolve {
                spell,
                def: obj.def,
            });
            for effect in effects {
                self.apply_effect(spell, obj.controller, &obj.targets, &legal, effect);
            }
        } else {
            self.emit(Event::Resolve {
                spell,
                def: obj.def,
            });
        }
        if let Some(o) = self.objects.get(spell)
            && o.zone == Zone::Stack
        {
            if def.is_permanent_card() {
                let new = self.move_object(spell, Zone::Battlefield, true);
                self.objects[new].controller = obj.controller;
            } else {
                self.move_object(spell, Zone::Graveyard, true);
            }
        }
    }

    fn apply_effect(
        &mut self,
        source: ObjId,
        controller: PlayerId,
        targets: &[Target],
        legal: &[bool],
        effect: &Effect,
    ) {
        let target_of = |i: usize| -> Option<Target> {
            if legal.get(i).copied().unwrap_or(false) {
                targets.get(i).copied()
            } else {
                None
            }
        };
        match effect {
            Effect::DealDamage { amount, target } => {
                if let Some(t) = target_of(*target) {
                    self.deal_damage(source, t, *amount, false);
                }
            }
            Effect::Pump {
                power,
                toughness,
                target,
            } => {
                if let Some(Target::Object(id)) = target_of(*target) {
                    let timestamp = self.tick();
                    self.effects.push(ContinuousEffect {
                        layer: Layer::L7cModifyPt,
                        timestamp,
                        affected: id,
                        modification: Modification::ModifyPt {
                            power: *power,
                            toughness: *toughness,
                        },
                        duration: Duration::UntilEndOfTurn,
                    });
                    self.bump();
                }
            }
            Effect::Destroy { target } => {
                if let Some(Target::Object(id)) = target_of(*target) {
                    self.destroy(id);
                }
            }
            Effect::GainLife { amount, player } => {
                let PlayerRef::You = player;
                self.change_life(controller, i32::try_from(*amount).unwrap_or(i32::MAX));
            }
            Effect::DrawCards { count, player } => {
                let PlayerRef::You = player;
                for _ in 0..*count {
                    self.draw(controller);
                }
            }
        }
    }

    // ----- state-based actions and game end ------------------------------------------

    /// CR 704.3: perform all applicable state-based actions simultaneously,
    /// repeating until none apply. Returns whether any were performed.
    fn sba_loop(&mut self) -> bool {
        let mut any = false;
        loop {
            let mut acted = false;
            // 704.5a, 704.5b
            for i in 0..self.num_players() {
                let p = PlayerId(u8::try_from(i).unwrap_or(0));
                let ps = self.player(p);
                if ps.lost.is_some() {
                    continue;
                }
                let reason = if ps.life <= 0 {
                    Some(LossReason::LifeZeroOrLess)
                } else if ps.drew_from_empty_library {
                    Some(LossReason::DrewFromEmptyLibrary)
                } else {
                    None
                };
                if let Some(reason) = reason {
                    self.player_mut(p).lost = Some(reason);
                    self.emit(Event::PlayerLost { player: p, reason });
                    acted = true;
                }
            }
            // 704.5d, 704.5f, 704.5g, 704.5h: collect first, act simultaneously.
            let mut to_graveyard = Vec::new();
            let mut to_destroy = Vec::new();
            let mut tokens_gone = Vec::new();
            for id in self.objects.keys().collect::<Vec<_>>() {
                let o = &self.objects[id];
                if o.kind == ObjKind::Token && o.zone != Zone::Battlefield {
                    tokens_gone.push(id);
                    continue;
                }
                if o.zone != Zone::Battlefield {
                    continue;
                }
                let Some(c) = self.characteristics(id) else {
                    continue;
                };
                if !c.is_creature() {
                    continue;
                }
                if c.toughness <= 0 {
                    to_graveyard.push(id);
                } else if o.damage >= u32::try_from(c.toughness).unwrap_or(0) || o.deathtouch_damage
                {
                    to_destroy.push(id);
                }
            }
            for id in tokens_gone {
                self.remove_from_zone_list(id);
                self.objects.remove(id);
                acted = true;
            }
            for id in to_graveyard {
                self.move_object(id, Zone::Graveyard, true);
                acted = true;
            }
            for id in to_destroy {
                let before = self.objects.contains_key(id);
                self.destroy(id);
                // Indestructible creatures stay; clear the deathtouch flag so
                // this does not loop forever.
                if before && self.objects.contains_key(id) {
                    self.objects[id].deathtouch_damage = false;
                }
                acted = true;
            }
            if !acted {
                break;
            }
            any = true;
        }
        any
    }

    fn check_game_over(&mut self) -> Option<GameResult> {
        if let Some(r) = self.result {
            return Some(r);
        }
        let alive: Vec<PlayerId> = self.players_in_game().collect();
        let result = match alive.len() {
            0 => GameResult::Draw,
            1 => GameResult::Win { winner: alive[0] },
            _ => return None,
        };
        self.result = Some(result);
        self.flow = Flow::GameOver;
        self.emit(Event::GameOver { result });
        Some(result)
    }

    // ----- combat -----------------------------------------------------------------------

    /// CR 508.1a: untapped creatures controlled since the turn began or with
    /// haste, without defender (CR 702.3b).
    fn eligible_attackers(&self, p: PlayerId) -> impl Iterator<Item = ObjId> + '_ {
        self.creatures_of(p).filter(move |id| {
            let o = &self.objects[*id];
            let c = self.characteristics(*id).expect("creature exists");
            !o.tapped
                && (o.controlled_since_turn_start || c.has(Keyword::Haste))
                && !c.has(Keyword::Defender)
        })
    }

    /// CR 509.1a: untapped creatures of the defending player.
    fn eligible_blockers(&self, p: PlayerId) -> impl Iterator<Item = ObjId> + '_ {
        self.creatures_of(p)
            .filter(move |id| !self.objects[*id].tapped)
    }

    /// Evasion: flying (CR 702.9b).
    fn can_block(&self, blocker: ObjId, attacker: ObjId) -> bool {
        let (Some(b), Some(a)) = (
            self.characteristics(blocker),
            self.characteristics(attacker),
        ) else {
            return false;
        };
        if a.has(Keyword::Flying) && !(b.has(Keyword::Flying) || b.has(Keyword::Reach)) {
            return false;
        }
        true
    }

    fn has_menace(&self, attacker: ObjId) -> bool {
        self.characteristics(attacker)
            .is_some_and(|c| c.has(Keyword::Menace))
    }

    /// Menace (CR 702.110b): an attacker is blocked by zero or at least two.
    fn block_set_legal(&self, blocks: &[(ObjId, ObjId)]) -> bool {
        let attackers: Vec<ObjId> = self
            .combat
            .as_ref()
            .map(|c| c.attackers.iter().map(|a| a.id).collect())
            .unwrap_or_default();
        attackers.iter().all(|a| {
            let n = blocks.iter().filter(|(_, x)| x == a).count();
            !(self.has_menace(*a) && n == 1)
        })
    }

    /// Whether a partial incremental block declaration can still be
    /// completed legally with the remaining free blockers.
    fn block_set_completable(
        &self,
        blocks: &[(ObjId, ObjId)],
        all_blockers: &[ObjId],
        can_block: &[Vec<ObjId>],
    ) -> bool {
        if self.block_set_legal(blocks) {
            return true;
        }
        let free: Vec<usize> = (0..all_blockers.len())
            .filter(|i| !blocks.iter().any(|(b, _)| *b == all_blockers[*i]))
            .collect();
        let attackers: Vec<ObjId> = self
            .combat
            .as_ref()
            .map(|c| c.attackers.iter().map(|a| a.id).collect())
            .unwrap_or_default();
        // Each menace attacker with exactly one blocker needs a distinct free
        // blocker able to block it. Small numbers; check greedily.
        let mut used = Vec::new();
        for a in attackers {
            let n = blocks.iter().filter(|(_, x)| *x == a).count();
            if self.has_menace(a) && n == 1 {
                let Some(i) = free
                    .iter()
                    .find(|i| !used.contains(*i) && can_block[**i].contains(&a))
                else {
                    return false;
                };
                used.push(*i);
            }
        }
        true
    }

    fn finish_attackers(&mut self, set: Vec<ObjId>) {
        let defender = self.opponent(self.active_player);
        let mut attackers = Vec::new();
        for id in set {
            // CR 508.1f: attacking taps the creature unless it has vigilance.
            if !self
                .characteristics(id)
                .is_some_and(|c| c.has(Keyword::Vigilance))
            {
                self.tap(id);
            }
            let target = Target::Player(defender);
            self.emit(Event::Attack {
                attacker: id,
                target,
            });
            attackers.push(Attacker {
                id,
                attacking: target,
                blockers: SmallVec::new(),
                blocked: false,
            });
        }
        if let Some(c) = self.combat.as_mut() {
            c.attackers = attackers;
        }
        self.bump();
        self.give_priority(self.active_player);
    }

    fn finish_blocks(&mut self, blocks: Vec<(ObjId, ObjId)>) {
        for (blocker, attacker) in blocks {
            self.emit(Event::Block { blocker, attacker });
            if let Some(a) = self.combat.as_mut().and_then(|c| c.attacker_mut(attacker)) {
                a.blockers.push(blocker);
                a.blocked = true;
            }
        }
        self.bump();
        self.give_priority(self.active_player);
    }

    fn any_first_strike_in_combat(&self) -> bool {
        let Some(c) = &self.combat else {
            return false;
        };
        c.attackers
            .iter()
            .flat_map(|a| std::iter::once(a.id).chain(a.blockers.iter().copied()))
            .filter_map(|id| self.characteristics(id))
            .any(|ch| ch.has(Keyword::FirstStrike) || ch.has(Keyword::DoubleStrike))
    }

    /// Whether `id` assigns combat damage in the current damage step (CR 510.4).
    fn deals_damage_this_step(&self, id: ObjId, first_strike_step: bool) -> bool {
        let Some(c) = self.characteristics(id) else {
            return false;
        };
        let fs = c.has(Keyword::FirstStrike);
        let ds = c.has(Keyword::DoubleStrike);
        let had_fs_step = self
            .combat
            .as_ref()
            .is_some_and(|c| c.had_first_strike_step);
        if first_strike_step {
            fs || ds
        } else if had_fs_step {
            ds || !fs
        } else {
            true
        }
    }

    /// CR 510.1: compute all automatic assignments, queue the ones needing a
    /// player's division, then deal damage.
    fn start_damage_assignment(&mut self, first_strike_step: bool) {
        let combat = self.combat.clone().unwrap_or_default();
        let mut assigned = Vec::new();
        let mut queue = Vec::new();
        for a in &combat.attackers {
            if !self.objects.contains_key(a.id) || !self.is_creature_on_battlefield(a.id) {
                continue;
            }
            if !self.deals_damage_this_step(a.id, first_strike_step) {
                continue;
            }
            let c = self.characteristics(a.id).expect("attacker exists");
            let power = u32::try_from(c.power).unwrap_or(0);
            if power == 0 {
                continue;
            }
            let live_blockers: Vec<ObjId> = a
                .blockers
                .iter()
                .copied()
                .filter(|b| self.is_creature_on_battlefield(*b))
                .collect();
            if !a.blocked {
                assigned.push(DamageAssignment {
                    source: a.id,
                    target: a.attacking,
                    amount: power,
                });
            } else if live_blockers.is_empty() {
                // CR 510.1c: no damage; trample assigns it all to the player (CR 702.19e).
                if c.has(Keyword::Trample) {
                    assigned.push(DamageAssignment {
                        source: a.id,
                        target: a.attacking,
                        amount: power,
                    });
                }
            } else if live_blockers.len() == 1 && !c.has(Keyword::Trample) {
                assigned.push(DamageAssignment {
                    source: a.id,
                    target: Target::Object(live_blockers[0]),
                    amount: power,
                });
            } else {
                queue.push(a.id);
            }
        }
        // Blockers (CR 510.1d): each blocks exactly one creature in M1.
        for a in &combat.attackers {
            for b in &a.blockers {
                if !self.is_creature_on_battlefield(*b)
                    || !self.deals_damage_this_step(*b, first_strike_step)
                {
                    continue;
                }
                let power = self
                    .characteristics(*b)
                    .map_or(0, |c| u32::try_from(c.power).unwrap_or(0));
                if power > 0 && self.is_creature_on_battlefield(a.id) {
                    assigned.push(DamageAssignment {
                        source: *b,
                        target: Target::Object(a.id),
                        amount: power,
                    });
                }
            }
        }
        self.flow = Flow::AssignDamage { queue, assigned };
    }

    /// Lethal damage for a blocker from this source (CR 702.19b, 702.2c).
    fn lethal_for(&self, source: ObjId, blocker: ObjId) -> u32 {
        if self
            .characteristics(source)
            .is_some_and(|c| c.has(Keyword::Deathtouch))
        {
            return 1;
        }
        let o = &self.objects[blocker];
        let t = self
            .characteristics(blocker)
            .map_or(0, |c| u32::try_from(c.toughness).unwrap_or(0));
        t.saturating_sub(o.damage).max(1)
    }

    /// All legal divisions of an attacker's damage (CR 510.1c, 702.19b).
    fn damage_divisions(&self, attacker: ObjId) -> Vec<Vec<(Target, u32)>> {
        let a = self
            .combat
            .as_ref()
            .and_then(|c| c.attacker(attacker))
            .cloned()
            .expect("attacker in combat");
        let c = self.characteristics(attacker).expect("attacker exists");
        let power = u32::try_from(c.power).unwrap_or(0);
        let trample = c.has(Keyword::Trample);
        let blockers: Vec<ObjId> = a
            .blockers
            .iter()
            .copied()
            .filter(|b| self.is_creature_on_battlefield(*b))
            .collect();
        let mut recipients: Vec<Target> = blockers.iter().map(|b| Target::Object(*b)).collect();
        let lethal: Vec<u32> = blockers
            .iter()
            .map(|b| self.lethal_for(attacker, *b))
            .collect();
        if trample {
            recipients.push(a.attacking);
        }
        let n = recipients.len();
        // Enumerate compositions of `power` into n parts, filtering trample's
        // lethal rule: the player slot may be nonzero only if every blocker
        // has at least lethal.
        let mut out = Vec::new();
        let mut current = vec![0u32; n];
        compose(power, 0, &mut current, &mut |parts| {
            if trample {
                let to_player = parts[n - 1];
                if to_player > 0 && parts[..n - 1].iter().zip(&lethal).any(|(p, l)| p < l) {
                    return true;
                }
            }
            out.push(parts.to_vec());
            out.len() < MAX_DIVISION_OPTIONS
        });
        if out.len() >= MAX_DIVISION_OPTIONS {
            // Too many: offer the canonical divisions only.
            out.clear();
            let mut canonical = vec![0u32; n];
            let mut left = power;
            for (i, l) in lethal.iter().enumerate() {
                let give = (*l).min(left);
                canonical[i] = give;
                left -= give;
            }
            if n > 0 {
                // Remainder to the player (trample) or the last blocker.
                canonical[n - 1] += left;
            }
            out.push(canonical);
        }
        out.into_iter()
            .map(|parts| {
                recipients
                    .iter()
                    .copied()
                    .zip(parts)
                    .filter(|(_, amt)| *amt > 0)
                    .collect()
            })
            .collect()
    }

    /// CR 510.2: all combat damage is dealt simultaneously.
    fn deal_combat_damage(&mut self, assigned: Vec<DamageAssignment>) {
        for DamageAssignment {
            source,
            target,
            amount,
        } in assigned
        {
            self.deal_damage(source, target, amount, true);
        }
        self.give_priority(self.active_player);
    }
}

/// All subsets of `items`, smallest first, each in the items' order.
fn subsets(items: &[ObjId]) -> Vec<Vec<ObjId>> {
    let n = items.len();
    let mut out: Vec<Vec<ObjId>> = (0..(1usize << n))
        .map(|mask| {
            (0..n)
                .filter(|i| mask & (1 << i) != 0)
                .map(|i| items[i])
                .collect()
        })
        .collect();
    out.sort_by_key(Vec::len);
    out
}

/// All block assignments: each blocker picks none or one of its allowed
/// attackers.
fn block_assignments(blockers: &[ObjId], can_block: &[Vec<ObjId>]) -> Vec<Vec<(ObjId, ObjId)>> {
    let mut out = vec![Vec::new()];
    for (b, allowed) in blockers.iter().zip(can_block) {
        let mut next = Vec::with_capacity(out.len() * (allowed.len() + 1));
        for partial in &out {
            next.push(partial.clone());
            for a in allowed {
                let mut p = partial.clone();
                p.push((*b, *a));
                next.push(p);
            }
        }
        out = next;
    }
    out
}

/// Visit every way of writing `total` as an ordered sum of `parts.len()`
/// non-negative parts. The visitor returns `false` to stop.
fn compose(
    total: u32,
    index: usize,
    parts: &mut Vec<u32>,
    visit: &mut dyn FnMut(&[u32]) -> bool,
) -> bool {
    if parts.is_empty() {
        return true;
    }
    if index == parts.len() - 1 {
        parts[index] = total;
        return visit(parts);
    }
    for v in (0..=total).rev() {
        parts[index] = v;
        if !compose(total - v, index + 1, parts, visit) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::m1_pool;

    fn deck(pool: &CardPool, spec: &[(&str, usize)]) -> Vec<CardRef> {
        spec.iter()
            .flat_map(|(name, n)| std::iter::repeat_n(pool.lookup(name).expect(name), *n))
            .collect()
    }

    fn config(pool: &CardPool, seed: u64) -> GameConfig {
        GameConfig {
            decks: vec![
                deck(
                    pool,
                    &[
                        ("Mountain", 12),
                        ("Raging Goblin", 8),
                        ("Lightning Bolt", 8),
                    ],
                ),
                deck(
                    pool,
                    &[("Forest", 12), ("Grizzly Bears", 8), ("Giant Growth", 8)],
                ),
            ],
            starting_life: 20,
            seed,
            logging: true,
        }
    }

    /// Always pick the first option: keep, pass, no attacks. Games end by
    /// decking.
    #[test]
    fn passing_game_ends_by_decking() {
        let pool = Arc::new(m1_pool());
        let mut engine = Engine::new(pool.clone(), &config(&pool, 1));
        let mut steps = 0;
        while !engine.is_over() {
            engine.step(0).unwrap();
            steps += 1;
            assert!(steps < 100_000, "game did not end");
        }
        let state = engine.state();
        let loser = state
            .players
            .iter()
            .position(|p| p.lost == Some(LossReason::DrewFromEmptyLibrary))
            .expect("someone decked");
        assert_eq!(
            state.result,
            Some(GameResult::Win {
                winner: state.opponent(PlayerId(u8::try_from(loser).unwrap()))
            })
        );
        // 28-card decks, 7 drawn: the first player to run out draws on their
        // 22nd draw step.
        assert!(state.turn_number >= 40, "turn {}", state.turn_number);
    }

    #[test]
    fn same_seed_and_actions_replay_identically() {
        let pool = Arc::new(m1_pool());
        let cfg = config(&pool, 7);
        let mut a = Engine::new(pool.clone(), &cfg);
        // Alternate between the last and first options to exercise casting
        // and attacking without an RNG.
        let mut i = 0u32;
        while !a.is_over() && i < 4000 {
            let n = u32::try_from(a.current().decision().unwrap().options.len()).unwrap();
            let choice = if i.is_multiple_of(3) { n - 1 } else { i % n };
            a.step(choice).unwrap();
            i += 1;
        }
        let record = crate::record::GameRecord::from_engine(&a, &pool, &cfg);
        let b = record.replay(pool.clone(), true).unwrap();
        assert_eq!(a.state().version, b.state().version);
        assert_eq!(a.state().turn_number, b.state().turn_number);
        assert_eq!(a.state().log.len(), b.state().log.len());
        for (p, q) in a.state().players.iter().zip(&b.state().players) {
            assert_eq!(p.life, q.life);
            assert_eq!(p.hand.len(), q.hand.len());
            assert_eq!(p.library.len(), q.library.len());
        }
    }

    #[test]
    fn objects_are_conserved() {
        let pool = Arc::new(m1_pool());
        let cfg = config(&pool, 3);
        let mut e = Engine::new(pool.clone(), &cfg);
        let total = cfg.decks.iter().map(Vec::len).sum::<usize>();
        let mut i = 0u32;
        while !e.is_over() && i < 3000 {
            let s = e.state();
            let in_zones: usize = s
                .players
                .iter()
                .map(|p| p.library.len() + p.hand.len() + p.graveyard.len() + p.exile.len())
                .sum::<usize>()
                + s.battlefield.len()
                + s.stack.len();
            assert_eq!(in_zones, total, "zone lists lost or duplicated objects");
            assert_eq!(s.objects.len(), total, "arena out of sync with zones");
            let n = u32::try_from(e.current().decision().unwrap().options.len()).unwrap();
            e.step((i * 7) % n).unwrap();
            i += 1;
        }
    }

    #[test]
    fn bad_choice_is_rejected() {
        let pool = Arc::new(m1_pool());
        let mut e = Engine::new(pool.clone(), &config(&pool, 1));
        let n = e.current().decision().unwrap().options.len();
        assert!(matches!(
            e.step(u32::try_from(n).unwrap()),
            Err(EngineError::BadChoice { .. })
        ));
    }

    #[test]
    fn compositions_and_subsets() {
        let mut seen = Vec::new();
        let mut parts = vec![0; 3];
        compose(2, 0, &mut parts, &mut |p| {
            seen.push(p.to_vec());
            true
        });
        assert_eq!(seen.len(), 6); // C(2+3-1, 3-1)
        assert!(seen.contains(&vec![2, 0, 0]));
        assert!(seen.contains(&vec![0, 1, 1]));

        let mut sm: SlotMap<ObjId, ()> = SlotMap::with_key();
        let ids: Vec<ObjId> = (0..3).map(|_| sm.insert(())).collect();
        let subs = subsets(&ids);
        assert_eq!(subs.len(), 8);
        assert_eq!(subs[0].len(), 0);
        assert_eq!(subs[7].len(), 3);
        let blocks = block_assignments(&ids[..2], &[vec![ids[2]], vec![ids[2]]]);
        assert_eq!(blocks.len(), 4);
    }
}
