//! The engine: a resumable step function over `GameState` (design §4.6–4.7).
//! `Engine::new` runs until the first decision; `Engine::step` applies a
//! choice and runs until the next one or the end of the game.

mod casting;
mod combat;
mod objects;
mod replace;
mod sba;
mod triggers;

use combat::{block_assignments, subsets};

use std::sync::Arc;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;
use thiserror::Error;

use crate::card::{CardPool, CardRef, TargetSpec};
use crate::decision::{Choice, ChoiceId, Decision, DecisionKind, StepResult};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::ObjKind;
use crate::state::{
    CombatState, DamageAssignment, Duration, Flow, GameResult, GameState, PlayerState, Zone,
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
    pub battlefield: Vec<SetupPermanent>,
    pub graveyard: Vec<CardRef>,
    pub exile: Vec<CardRef>,
}

#[derive(Clone, Debug)]
pub struct SetupPermanent {
    pub def: CardRef,
    pub tapped: bool,
    /// For Auras: the name of a permanent (either player's) to attach to.
    /// Resolved after every permanent is placed; the first match wins.
    pub attached_to: Option<String>,
    /// Counters already on the permanent.
    pub counters: Vec<(crate::card::CounterKind, u32)>,
}

impl SetupPermanent {
    pub fn new(def: CardRef) -> Self {
        SetupPermanent {
            def,
            tapped: false,
            attached_to: None,
            counters: Vec::new(),
        }
    }
}

/// The state before an action that met a decision in the middle of an
/// event (CR 616.1), kept so the action can be replayed with the answer.
struct Snapshot {
    state: GameState,
    log_len: usize,
    action: Option<(PlayerId, Choice)>,
    answers: Vec<ChoiceId>,
}

pub struct Engine {
    state: GameState,
    current: StepResult,
    history: Vec<ChoiceId>,
    /// Boxed: taking an inline `Option<GameState>` would copy it on every
    /// step.
    snapshot: Option<Box<Snapshot>>,
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
        let mut engine = Engine {
            state,
            current: StepResult::Over(GameResult::Draw),
            history: Vec::new(),
            snapshot: None,
        };
        engine.advance(None, Vec::new());
        engine
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
        let mut attachments: Vec<(ObjId, String)> = Vec::new();
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
            for perm in &ps.battlefield {
                let id = state.new_object(perm.def, p, Zone::Battlefield);
                let o = &mut state.objects[id];
                o.tapped = perm.tapped;
                o.controlled_since_turn_start = true;
                for (kind, n) in &perm.counters {
                    o.add_counters(*kind, *n);
                }
                state.battlefield.push(id);
                let def = state.pool.get(perm.def);
                state.trigger_sources += u32::from(def.has_triggers());
                state.replacement_sources += u32::from(def.has_replacements());
                if let Some(name) = &perm.attached_to {
                    attachments.push((id, name.clone()));
                }
            }
        }
        for (aura, name) in attachments {
            let target = state
                .battlefield
                .iter()
                .copied()
                .find(|id| *id != aura && state.name_of(*id) == name)
                .unwrap_or_else(|| panic!("setup: no \"{name}\" on the battlefield to attach to"));
            state.objects[aura].attached_to = Some(target);
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
        let mut engine = Engine {
            state,
            current: StepResult::Over(GameResult::Draw),
            history: Vec::new(),
            snapshot: None,
        };
        engine.advance(None, Vec::new());
        engine
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
        if let Some(snap) = self.snapshot.take() {
            // Answering a question met in the middle of an event: replay the
            // action that raised it from the state before it, with the
            // answer appended.
            let snap = *snap;
            let mut answers = snap.answers;
            answers.push(choice);
            self.restore(snap.state, snap.log_len);
            self.advance(snap.action, answers);
        } else {
            self.advance(Some((player, option)), Vec::new());
        }
        Ok(&self.current)
    }

    /// Apply `action` (if any) and run to the next decision. If a decision
    /// is needed in the middle of an event (CR 616.1) the state is rolled
    /// back to before the action and that decision is returned; `step`
    /// replays the action once it is answered. The snapshot this needs is
    /// taken only when such a decision is possible.
    fn advance(&mut self, action: Option<(PlayerId, Choice)>, answers: Vec<ChoiceId>) {
        // The common case: no replacement effects in play, nothing to
        // snapshot or record.
        if !self.state.may_need_mid_decision() && answers.is_empty() {
            if let Some((p, c)) = action {
                self.state.apply_choice(p, c);
            }
            self.current = self.state.run();
            debug_assert!(self.state.mid.pending.is_none());
            return;
        }
        let (snapshot, log_len) = self.snapshot();
        let saved_action = action.clone();
        self.state.mid = crate::state::MidDecision {
            answers,
            next: 0,
            pending: None,
        };
        if let Some((p, c)) = action {
            self.state.apply_choice(p, c);
        }
        let result = self.state.run();
        if let Some(pending) = self.state.mid.pending.take() {
            let answers = std::mem::take(&mut self.state.mid.answers);
            self.restore(snapshot.clone(), log_len);
            self.snapshot = Some(Box::new(Snapshot {
                state: snapshot,
                log_len,
                action: saved_action,
                answers,
            }));
            self.current = StepResult::Decision(pending);
            return;
        }
        debug_assert_eq!(
            self.state.mid.next,
            self.state.mid.answers.len(),
            "recorded answers were not all consumed on replay"
        );
        self.state.mid = crate::state::MidDecision::default();
        self.current = result;
    }

    /// The state without its event log (restored separately).
    fn snapshot(&mut self) -> (GameState, usize) {
        let log = std::mem::take(&mut self.state.log);
        let copy = self.state.clone();
        let log_len = log.len();
        self.state.log = log;
        (copy, log_len)
    }

    fn restore(&mut self, state: GameState, log_len: usize) {
        let mut log = std::mem::take(&mut self.state.log);
        log.truncate(log_len);
        self.state = state;
        self.state.log = log;
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
            replacements: Vec::new(),
            rules: Vec::new(),
            replacement_sources: 0,
            mid: crate::state::MidDecision::default(),
            lki: std::collections::HashMap::default(),
            pending_triggers: Vec::new(),
            trigger_sources: 0,
            lookback: None,
            rng,
            next_timestamp: 1,
            flow: Flow::BeginStep,
            cleanup_repeat: false,
            result: None,
            log: Vec::new(),
            logging,
            cache: crate::state::DerivedCache::default(),
        }
    }

    // ----- the main loop -------------------------------------------------------

    /// Advance until a decision is needed or the game ends.
    #[allow(clippy::too_many_lines)] // one arm per `Flow` variant
    fn run(&mut self) -> StepResult {
        loop {
            // A decision was needed in the middle of an event: stop here;
            // the `Engine` rolls back and asks (see `Engine::advance`).
            if self.mid.pending.is_some() {
                return StepResult::Decision(self.mid.pending.clone().expect("checked"));
            }
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
                    // CR 603.3, 117.5: triggered abilities go on the stack
                    // before priority; then the SBA check repeats.
                    if self.put_triggers_on_stack() {
                        continue;
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
                    let is_ability = obj.kind == ObjKind::Ability;
                    let specs: Vec<TargetSpec> = self.targets_of(spell).to_vec();
                    if next_target >= specs.len() {
                        if is_ability {
                            // Targets chosen; the ability is on the stack.
                            self.flow = Flow::Priority;
                        } else {
                            self.finish_cast(spell);
                        }
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
                Flow::OrderTriggers { player } => {
                    let mine = self.triggers_of(player);
                    if mine.len() <= 1 {
                        // Nothing left to order.
                        self.flow = Flow::Priority;
                        continue;
                    }
                    let options: Vec<Choice> = mine.into_iter().map(Choice::Trigger).collect();
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::OrderTriggers,
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
            (Flow::OrderTriggers { .. }, Choice::Trigger(i)) => {
                self.put_trigger(i);
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
            .flat_map(|e| match &e.affected {
                crate::state::Affected::Objects(set) => set.to_vec(),
                crate::state::Affected::Filter { .. } => Vec::new(),
            })
            .collect();
        self.effects
            .retain(|e| e.duration != Duration::UntilEndOfTurn);
        self.replacements
            .retain(|e| e.duration != Duration::UntilEndOfTurn);
        self.rules
            .retain(|e| e.duration != Duration::UntilEndOfTurn);
        for affected in ended {
            self.emit(Event::EffectEnded { affected });
        }
        self.bump();
        let acted = self.sba_loop();
        if acted || !self.pending_triggers.is_empty() {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::m1_pool;
    use crate::state::LossReason;

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
            let abilities = s
                .stack
                .iter()
                .filter(|id| s.objects[**id].kind == ObjKind::Ability)
                .count();
            assert_eq!(
                in_zones - abilities,
                total,
                "zone lists lost or duplicated objects"
            );
            assert_eq!(
                s.objects.len() - abilities,
                total,
                "arena out of sync with zones"
            );
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
}
