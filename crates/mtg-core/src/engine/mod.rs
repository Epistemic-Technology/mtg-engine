//! The engine: a resumable step function over `GameState` (design §4.6–4.7).
//! `Engine::new` runs until the first decision; `Engine::step` applies a
//! choice and runs until the next one or the end of the game.

mod ability_mana;
mod casting;
mod combat;
mod control;
mod designations;
mod objects;
mod replace;
mod sba;
mod triggers;

use combat::{block_assignments, block_choices, subsets};

use std::sync::Arc;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;
use thiserror::Error;

use crate::card::{CardPool, CardRef, Keyword, SacCost, TargetSpec};
use crate::decision::{Choice, ChoiceId, Decision, DecisionKind, StepResult};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::ObjKind;
use crate::state::{
    CombatState, DamageAssignment, Duration, Flow, GameResult, GameState, PlayerState, Target, Zone,
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
    /// Ask about "If this card is in your opening hand, ..." cards in the
    /// hands set up before play begins (CR 103.6a).
    pub opening_hand: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PlayerSetup {
    pub life: i32,
    /// Starting poison counters (scenarios).
    pub poison: u32,
    /// Starting energy counters (scenarios).
    pub energy: u32,
    /// Starting speed (scenarios; CR 702.179).
    pub speed: u8,
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
#[derive(Clone)]
struct Snapshot {
    state: GameState,
    log_len: usize,
    action: Option<(PlayerId, Choice)>,
    answers: Vec<ChoiceId>,
}

#[derive(Clone)]
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
    #[allow(clippy::too_many_lines)] // one line per setup field
    pub fn from_setup(pool: Arc<CardPool>, setup: &Setup) -> Engine {
        assert_eq!(setup.players.len(), 2, "M1 supports exactly two players");
        let lives: Vec<i32> = setup.players.iter().map(|p| p.life).collect();
        let rng = ChaCha8Rng::seed_from_u64(setup.seed);
        let mut state = GameState::blank(pool, setup.active_player, &lives, rng, setup.logging);
        for (i, p) in setup.players.iter().enumerate() {
            state.players[i].poison = p.poison;
            state.players[i].energy = p.energy;
            if p.speed > 0 {
                state.desig.speed[i.min(3)] = p.speed.min(4);
                state.desig.live = true;
            }
        }
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
                // A Room set up by a half's name is that card with the
                // half's door unlocked (CR 709.5).
                if let Some(d) = state.pool.get(perm.def).doors.as_deref() {
                    let root = d.root;
                    state.objects[id].card = root;
                }
                let printed_loyalty = {
                    let d = state.pool.get(perm.def);
                    d.types
                        .contains(crate::card::CardType::Planeswalker)
                        .then_some(d.loyalty)
                };
                // A token definition set up on the battlefield is a token
                // (CR 111.1), not a card.
                let token = state.pool.get(perm.def).token;
                let o = &mut state.objects[id];
                if token {
                    o.kind = crate::state::ObjKind::Token;
                }
                o.tapped = perm.tapped;
                o.controlled_since_turn_start = true;
                for (kind, n) in &perm.counters {
                    o.add_counters(*kind, *n);
                }
                // CR 306.5b: a planeswalker set up on the battlefield has its
                // printed loyalty unless the scenario gives it loyalty.
                if let Some(l) = printed_loyalty
                    && !perm
                        .counters
                        .iter()
                        .any(|(k, _)| *k == crate::card::CounterKind::Loyalty)
                {
                    o.add_counters(crate::card::CounterKind::Loyalty, l);
                }
                // CR 310.4b: a battle likewise has its printed defense.
                let printed_defense = state.pool.get(perm.def).defense;
                let o = &mut state.objects[id];
                if printed_defense > 0
                    && !perm
                        .counters
                        .iter()
                        .any(|(k, _)| *k == crate::card::CounterKind::Defense)
                {
                    o.add_counters(crate::card::CounterKind::Defense, printed_defense);
                }
                state.battlefield.push(id);
                let def = state.pool.get(perm.def);
                state.trigger_sources += u32::from(def.has_triggers());
                state.replacement_sources += u32::from(def.has_replacements());
                state.control_sources += u32::from(def.has_control_static());
                state.legendary_count += u32::from(def.is_legendary());
                state.activated_sources += u32::from(def.has_activated());
                state.life_cda_sources += u32::from(def.flags().has_life_cda());
                state.tap_static_sources += u32::from(def.flags().has_tap_static());
                state.cost_change_sources += u32::from(def.flags().has_cost_change());
                state.restricted_mana_sources += u32::from(def.flags().has_restricted_mana());
                state.mana_trigger_sources += u32::from(def.flags().has_mana_trigger());
                state.turn_static_sources += u32::from(def.flags().has_turn_static());
                state.attack_rule_sources += u32::from(def.flags().has_attack_rule());
                state.grant_sources += u32::from(def.flags().grants_abilities());
                state.spent_watchers += u32::from(def.flags().watches_spent());
                state.valiant_watchers += u32::from(def.flags().has_valiant());
                state.state_trigger_sources += u32::from(def.flags().has_state_trigger());
                state.mark_sources += u32::from(def.flags().has_marks());
                state.banding_sources += u32::from(def.flags().has_banding());
                state.room_count += u32::from(def.flags().has_room());
                state.saga_count += u32::from(def.flags().has_saga());
                state.player_static_sources += u32::from(def.flags().has_player_static());
                state.top_play_sources += u32::from(def.flags().has_top_play());
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
        // Permanents set up count as controlled since the turn began, so
        // they did not enter this turn either (power-up, CR 702.193a).
        state.turn_started_at = state.next_timestamp;
        state.emit(Event::GameStart {
            starting_player: setup.active_player,
        });
        state.emit(Event::TurnBegin {
            turn: setup.turn_number,
            player: setup.active_player,
        });
        state.flow = if setup.opening_hand {
            Flow::OpeningHand { asked: Vec::new() }
        } else {
            Flow::BeginStep
        };
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

    /// Whether the current decision was met in the middle of an event (its
    /// answer replays the event; a search should not determinize here).
    pub fn mid_decision(&self) -> bool {
        self.snapshot.is_some()
    }

    pub fn history(&self) -> &[ChoiceId] {
        &self.history
    }

    pub fn is_over(&self) -> bool {
        self.current.is_over()
    }

    /// Resample the information hidden from `viewer` (design §4.8) and
    /// re-enumerate the current decision. Meant for search at a point
    /// where `viewer` is to act; a pending mid-event snapshot is dropped.
    pub fn determinize(&mut self, viewer: PlayerId, seed: u64) {
        self.snapshot = None;
        self.state.determinize(viewer, seed);
        if self.state.mid.pending.is_none() {
            self.current = self.state.run();
        }
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
        if !self
            .state
            .may_need_mid_decision(action.as_ref().map(|(_, c)| c))
            && answers.is_empty()
        {
            if let Some((p, c)) = action {
                self.state.apply_choice(p, c);
            }
            self.current = self.state.run();
            debug_assert!(
                self.state.mid.pending.is_none(),
                "mid-event decision without a snapshot: {:?}",
                self.state.mid.pending
            );
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
    #[allow(clippy::too_many_lines)] // one line per state field
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
            cversion: 0,
            sba_dirty: true,
            stray_tokens: Vec::new(),
            emblems: Vec::new(),
            loyalty_activated: Vec::new(),
            activated_once: Vec::new(),
            resolved_this_turn: Vec::new(),
            resolution_number: 0,
            limited_this_turn: Vec::new(),
            doing_once: None,
            token_pt: None,
            activated_ever: Vec::new(),
            modes_chosen: Vec::new(),
            attackers_this_turn: Vec::new(),
            battle_attackers_this_turn: Vec::new(),
            attacked_last_turn: Vec::new(),
            blockers_this_turn: Vec::new(),
            dealt_damage_this_turn: Vec::new(),
            entered_this_turn: Vec::new(),
            damaged_by_this_turn: Vec::new(),
            saddled: Vec::new(),
            phased_out: Vec::new(),
            encoded: Vec::new(),
            turn_started_at: 0,
            x_values: Vec::new(),
            entered_x: Vec::new(),
            reflexive_x: Vec::new(),
            reflexive_moved: Vec::new(),
            ninjutsu: Vec::new(),
            harmonized: Vec::new(),
            bargains: Vec::new(),
            divisions: Vec::new(),
            pairs: Vec::new(),
            splices: Vec::new(),
            goads: Vec::new(),
            turn_number: 0,
            spells_this_turn: [0; 4],
            next_spell_less: [0; 4],
            mana_spent_this_turn: [0; 4],
            mana_spent_before_cast: [0; 4],
            drawn_this_turn: [0; 4],
            life_lost_this_turn: [0; 4],
            attacked_this_turn: 0,
            block_demands: Vec::new(),
            block_bans: Vec::new(),
            no_regen: Vec::new(),
            no_combat_damage: Vec::new(),
            lands_entered_this_turn: 0,
            artifacts_entered_this_turn: 0,
            creatures_died_this_turn: 0,
            died_this_turn: Vec::new(),
            damaged_this_turn: 0,
            assassin_damage: 0,
            prowl_damage: Vec::new(),
            enter_notes: Vec::new(),
            lost_life_this_turn: 0,
            left_this_turn: 0,
            discarded_this_turn: 0,
            mayhem_discarded: Vec::new(),
            granted_flashback: Vec::new(),
            granted_harmonize: Vec::new(),
            cast_this_turn: smallvec::SmallVec::new(),
            turn_log: crate::state::TurnLog::default(),
            desig: crate::state::Designations::default(),
            exile_casts: None,
            suspended_count: 0,
            graveyard_trigger_cards: 0,
            dredge_cards: 0,
            enters_choice_cards: 0,
            prepared_ever: 0,
            spells_last_turn: [0; 4],
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
                    poison: 0,
                    energy: 0,
                    adventures_in_exile: 0,
                    lands_played_this_turn: 0,
                    extra_land_plays: 0,
                    flashback_in_graveyard: 0,
                    max_hand_size: 7,
                    drew_from_empty_library: false,
                    lost: None,
                    mulligans_taken: 0,
                    skip_draw: 0,
                    skip_combat: 0,
                    skip_combat_this_turn: 0,
                    combat_mana: 0,
                    turn_mana: crate::mana::ManaPool::default(),
                })
                .collect(),
            objects: SlotMap::with_key(),
            battlefield: Vec::new(),
            stack: Vec::new(),
            combat: None,
            effects: Vec::new(),
            replacements: Vec::new(),
            rules: Vec::new(),
            player_rules: Vec::new(),
            player_grants: Vec::new(),
            replacement_sources: 0,
            control_sources: 0,
            control_effects: 0,
            control_dirty: false,
            legendary_count: 0,
            activated_sources: 0,
            life_cda_sources: 0,
            tap_static_sources: 0,
            cost_change_sources: 0,
            restricted_mana_sources: 0,
            restricted_pool: Vec::new(),
            mana_trigger_sources: 0,
            turn_static_sources: 0,
            state_trigger_sources: 0,
            mark_sources: 0,
            banding_sources: 0,
            attack_rule_sources: 0,
            grant_sources: 0,
            eot_grants: 0,
            lasting_grants: 0,
            lasting: None,
            linked_durations: 0,
            undone_cast: None,
            face_down_count: 0,
            room_count: 0,
            top_play_sources: 0,
            saga_count: 0,
            player_static_sources: 0,
            mid: crate::state::MidDecision::default(),
            lki: std::collections::HashMap::default(),
            this_way: None,
            paid_costs: Vec::new(),
            linked_exiles: Vec::new(),
            melded: Vec::new(),
            temp_copies: Vec::new(),
            linking: None,
            spent_mana: Vec::new(),
            spending: None,
            spent_watchers: 0,
            valiant_watchers: 0,
            targeted_by_controller: Vec::new(),
            pending_triggers: Vec::new(),
            delayed_triggers: Vec::new(),
            paradigms: Vec::new(),
            trigger_sources: 0,
            lookback: None,
            rng,
            next_timestamp: 1,
            batch_from: 0,
            flow: Flow::BeginStep,
            cleanup_repeat: false,
            end_turn: false,
            extra_turns: Vec::new(),
            effect_controller: None,
            resume_player: None,
            extra_phases: Vec::new(),
            resume_step: None,
            combats_this_turn: 0,
            result: None,
            log: Vec::new(),
            logging,
            cache: crate::state::DerivedCache::default(),
            mana_cache: std::cell::RefCell::new(Default::default()),
            tap_epoch: 0,
        }
    }

    // ----- the main loop -------------------------------------------------------

    /// A copy of `flow` to match on. The common steps carry no data and
    /// are made in place; the rest go through the derived `clone` (an
    /// out-of-line match over every variant, 2% of a game when it took
    /// them all).
    #[inline]
    fn flow_now(&self) -> Flow {
        match self.flow {
            Flow::Priority => Flow::Priority,
            Flow::BeginStep => Flow::BeginStep,
            Flow::EndStep => Flow::EndStep,
            _ => self.flow.clone(),
        }
    }

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
            match self.flow_now() {
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
                            self.flow = Flow::OpeningHand { asked: Vec::new() };
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
                Flow::OpeningHand { asked } => {
                    // CR 103.6a: after mulligans, starting with the starting
                    // player, each player may put cards with such an
                    // ability from their opening hand onto the battlefield.
                    if let Some((player, card)) = self.next_opening_hand_card(&asked) {
                        return StepResult::Decision(Decision {
                            player,
                            kind: DecisionKind::YouMay {
                                def: self.objects[card].def,
                            },
                            options: vec![Choice::Accept, Choice::Decline],
                        });
                    }
                    if self.turn_number == 0 {
                        self.begin_game();
                    } else {
                        // A set-up position: play begins at its step.
                        self.flow = Flow::BeginStep;
                    }
                }
                Flow::BeginStep => self.begin_step(),
                Flow::Priority => {
                    // Spells cast while an effect resolved ("you may cast
                    // it without paying its mana cost") are cast now,
                    // before anything else happens.
                    if let Some((p, card, alt)) =
                        self.exile_casts.as_mut().and_then(|e| e.pending.pop())
                    {
                        // From exile, the hand (Mad Wizard's Lair), or a
                        // graveyard (The Dawning Archaic).
                        if self.objects.get(card).is_some_and(|o| {
                            matches!(o.zone, Zone::Exile | Zone::Hand | Zone::Graveyard)
                        }) {
                            // CR 712.11a: cast transformed, back face up.
                            let face = self.exile_casts.as_mut().and_then(|e| {
                                let i = e.transformed.iter().position(|(c, _)| *c == card)?;
                                Some(e.transformed.swap_remove(i).1)
                            });
                            self.begin_cast_as(p, card, alt, face);
                        }
                        continue;
                    }
                    if self.sba_loop() {
                        continue; // game-over check first, then re-enter
                    }
                    // CR 603.8: state triggers check as a player would
                    // receive priority.
                    if self.state_trigger_sources > 0 {
                        self.check_state_triggers();
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
                Flow::ChoosingAdditional { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    let options = self.additional_cost_options(spell);
                    match options.as_slice() {
                        [] => {
                            // Neither can be paid any more: the cast is
                            // undone (CR 726).
                            self.undo_cast(spell, false);
                        }
                        [Choice::Mode(i)] => {
                            self.objects[spell].cast.set_other_cost(*i == 1);
                            self.flow = self.after_additional(spell);
                        }
                        _ => {
                            return StepResult::Decision(Decision {
                                player,
                                kind: DecisionKind::ChooseAdditionalCost { spell },
                                options,
                            });
                        }
                    }
                }
                Flow::TurningFaceUp { obj } => {
                    // The card discarded or permanent returned for a
                    // morph cost (CR 702.37e, 116.2b).
                    let Some(cost) = self.face_up_cost(obj) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = self.objects[obj].controller;
                    let (kind, options): (DecisionKind, Vec<Choice>) = if cost.discard > 0 {
                        (
                            DecisionKind::ChooseDiscardCost { spell: obj },
                            self.discard_candidates(player, cost.discard_filter.as_ref(), obj)
                                .into_iter()
                                .map(|id| Choice::Discard {
                                    card: id,
                                    def: self.objects[id].def,
                                })
                                .collect(),
                        )
                    } else {
                        let Some(SacCost::Filter(f)) = &cost.sacrifice else {
                            unreachable!("a morph cost choosing nothing");
                        };
                        (
                            DecisionKind::ChooseSacrifice { spell: obj },
                            self.sacrifice_candidates(player, f, obj)
                                .into_iter()
                                .map(Choice::Sacrifice)
                                .collect(),
                        )
                    };
                    if options.is_empty() {
                        self.flow = Flow::Priority;
                        continue;
                    }
                    return StepResult::Decision(Decision {
                        player,
                        kind,
                        options,
                    });
                }
                Flow::ChoosingKicker { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::Kicker { spell },
                        options: self.linked_cost_options(spell),
                    });
                }
                Flow::ChoosingBuyback { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    // "Buyback—Sacrifice a land.": the land, or declining.
                    let def = self.pool.get(obj.def);
                    let options = match def.buyback_sacrifice.as_deref() {
                        Some(f) => self
                            .sacrifice_candidates(player, f, spell)
                            .into_iter()
                            .map(Choice::Sacrifice)
                            .chain(std::iter::once(Choice::Decline))
                            .collect(),
                        None => vec![Choice::Accept, Choice::Decline],
                    };
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::Buyback { spell },
                        options,
                    });
                }
                Flow::ChoosingOffspring { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::Offspring { spell },
                        options: vec![Choice::Accept, Choice::Decline],
                    });
                }
                Flow::ChoosingMultikicker { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    let options = (0..=self.multikicker_max(spell)).map(Choice::X).collect();
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::Multikicker { spell },
                        options,
                    });
                }
                Flow::ChoosingReplicate { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    let options = (0..=self.replicate_max(spell)).map(Choice::X).collect();
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::Replicate { spell },
                        options,
                    });
                }
                Flow::ChoosingBargain { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    let mut options: Vec<Choice> = self
                        .bargain_candidates(player)
                        .into_iter()
                        .map(Choice::Sacrifice)
                        .collect();
                    // Not bargaining may cost more ("costs {2} less to
                    // cast if it's bargained"): only if still payable.
                    if self.spell_payable(player, spell, &self.spell_requirements(spell, 0)) {
                        options.push(Choice::Decline);
                    }
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::Bargain { spell },
                        options,
                    });
                }
                Flow::ChoosingCasualty { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    let n = self.pool.get(obj.def).casualty;
                    let mut options: Vec<Choice> = self
                        .casualty_candidates(player, n)
                        .into_iter()
                        .map(Choice::Sacrifice)
                        .collect();
                    options.push(Choice::Decline);
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::Casualty { spell },
                        options,
                    });
                }
                Flow::ChoosingGift { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    // "If the gift was promised, instead ... target ...":
                    // without targets for the spell unpromised, the gift
                    // must be promised (CR 601.2c, 702.174m).
                    let def = self.pool.get(obj.def);
                    let unpromised = def.kicked_drops.is_none()
                        || self.targets_available(
                            obj.controller,
                            def.unkicked_cast_targets(),
                            Some(spell),
                        );
                    let mut options = vec![Choice::Accept];
                    if unpromised {
                        options.push(Choice::Decline);
                    }
                    return StepResult::Decision(Decision {
                        player: obj.controller,
                        kind: DecisionKind::Gift { spell },
                        options,
                    });
                }
                Flow::ChoosingTeamwork { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    // "If this spell was cast using teamwork, instead ...
                    // target ...": without targets for the spell cast
                    // without it, teamwork must be used (CR 601.2c).
                    let def = self.pool.get(obj.def);
                    let without = def.kicked_drops.is_none()
                        || self.targets_available(
                            obj.controller,
                            def.unkicked_cast_targets(),
                            Some(spell),
                        );
                    let mut options = vec![Choice::Accept];
                    if without {
                        options.push(Choice::Decline);
                    }
                    // "If this spell was cast using teamwork, choose both
                    // instead" (CR 700.2): the modes chose it.
                    match self.linked_cost_answer(spell) {
                        Some(true) => options = vec![Choice::Accept],
                        Some(false) => options = vec![Choice::Decline],
                        None => {}
                    }
                    return StepResult::Decision(Decision {
                        player: obj.controller,
                        kind: DecisionKind::Teamwork { spell },
                        options,
                    });
                }
                Flow::ChoosingOptionalCost { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    return StepResult::Decision(Decision {
                        player: obj.controller,
                        kind: DecisionKind::OptionalCost { spell },
                        options: self.linked_cost_options(spell),
                    });
                }
                Flow::ChoosingConspire { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    return StepResult::Decision(Decision {
                        player: obj.controller,
                        kind: DecisionKind::Conspire { spell },
                        options: vec![Choice::Accept, Choice::Decline],
                    });
                }
                Flow::ChoosingSplice { spell } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    let mut options: Vec<Choice> = self
                        .splice_candidates(spell)
                        .into_iter()
                        .map(|id| Choice::Discard {
                            card: id,
                            def: self.objects[id].def,
                        })
                        .collect();
                    if options.is_empty() {
                        self.flow = self.after_splice(spell);
                        continue;
                    }
                    options.push(Choice::Done);
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::Splice { spell },
                        options,
                    });
                }
                Flow::ChoosingX { spell, .. } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let player = obj.controller;
                    let is_ability = obj.kind == ObjKind::Ability;
                    let options = self.x_options(spell);
                    if options.is_empty() {
                        // No affordable X has a target its mana value fits
                        // ("with mana value X"): the cast or activation is
                        // undone (CR 726).
                        self.undo_cast(spell, is_ability);
                        continue;
                    }
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::ChooseX { spell },
                        options,
                    });
                }
                Flow::Casting {
                    spell,
                    next_target,
                    sacrifice,
                    discard,
                } => {
                    let Some(obj) = self.objects.get(spell) else {
                        self.flow = Flow::Priority;
                        continue;
                    };
                    let (caster, def) = (obj.controller, obj.def);
                    let is_ability = obj.kind == ObjKind::Ability;
                    let specs: Vec<TargetSpec> = self.targets_of(spell).to_vec();
                    if next_target >= specs.len() {
                        // CR 601.2d: a division is announced with the
                        // targets.
                        if let Some(d) = self.division_decision(spell) {
                            return StepResult::Decision(d);
                        }
                        if is_ability {
                            // Targets chosen: an activated ability pays its
                            // costs; a triggered one is already on the stack.
                            self.finish_ability_targets(spell, sacrifice, discard);
                            continue;
                        }
                        // CR 601.2h: additional costs that need a choice are
                        // settled after targets, then everything is paid;
                        // an alternative cost's permanent likewise (CR
                        // 118.9).
                        let d = self.pool.get(def);
                        // CR 702.190a: the unblocked attacker sneak returns.
                        if sacrifice.is_none()
                            && self.objects[spell].alt == crate::state::AltCast::Sneak
                        {
                            let options: Vec<Choice> = self
                                .unblocked_attackers(caster)
                                .into_iter()
                                .map(Choice::Sacrifice)
                                .collect();
                            if !options.is_empty() {
                                return StepResult::Decision(Decision {
                                    player: caster,
                                    kind: DecisionKind::ChooseSacrifice { spell },
                                    options,
                                });
                            }
                        }
                        // CR 702.188a: the tapped creature web-slinging
                        // returns.
                        if sacrifice.is_none()
                            && self.objects[spell].alt == crate::state::AltCast::WebSlinging
                        {
                            let options: Vec<Choice> = self
                                .web_slinging_returns(caster)
                                .into_iter()
                                .map(Choice::Sacrifice)
                                .collect();
                            if !options.is_empty() {
                                return StepResult::Decision(Decision {
                                    player: caster,
                                    kind: DecisionKind::ChooseSacrifice { spell },
                                    options,
                                });
                            }
                        }
                        let add = match &d.alt_cost {
                            Some(alt)
                                if self.objects[spell].alt == crate::state::AltCast::AltCost =>
                            {
                                &alt.cost
                            }
                            _ => self.additional_of(spell),
                        };
                        if sacrifice.is_none()
                            && let Some(SacCost::Filter(f)) = &add.sacrifice
                        {
                            let emerge = self.pool.get(self.objects[spell].def).emerge
                                && self.objects[spell].alt == crate::state::AltCast::AltCost;
                            let options: Vec<Choice> = if add.taps_chosen() {
                                self.tap_first_options(spell, add)
                            } else if emerge {
                                // Only those that leave the rest payable.
                                self.emerge_sacrifices(spell, f)
                            } else {
                                self.sacrifice_candidates(caster, f, spell)
                            }
                            .into_iter()
                            .map(Choice::Sacrifice)
                            .collect();
                            if options.is_empty() {
                                unreachable!("cast offered without a permanent to sacrifice");
                            }
                            return StepResult::Decision(Decision {
                                player: caster,
                                kind: DecisionKind::ChooseSacrifice { spell },
                                options,
                            });
                        }
                        if discard.is_none()
                            && (add.discard > 0 || (add.discard_x && self.x_of(spell) > 0))
                        {
                            let options: Vec<Choice> = self
                                .discard_candidates(caster, add.discard_filter.as_ref(), spell)
                                .into_iter()
                                .map(|id| Choice::Discard {
                                    card: id,
                                    def: self.objects[id].def,
                                })
                                .collect();
                            if options.is_empty() {
                                unreachable!("cast offered without a card to discard");
                            }
                            return StepResult::Decision(Decision {
                                player: caster,
                                kind: DecisionKind::ChooseDiscardCost { spell },
                                options,
                            });
                        }
                        self.finish_cast(spell, sacrifice, discard);
                        continue;
                    }
                    // A spell is its own source (protection judges its
                    // colors, CR 702.16b).
                    let source = Some(if is_ability {
                        self.objects[spell].ability.map_or(spell, |a| a.source)
                    } else {
                        spell
                    });
                    let spec = specs[next_target];
                    let optional = matches!(spec, TargetSpec::UpTo(_));
                    let chosen = &self.objects[spell].targets;
                    // A variable group (CR 601.2c) takes targets until its
                    // maximum, and may stop once it has its minimum.
                    let many = match spec {
                        TargetSpec::Many(i) => {
                            let (min, max) = self.many_bounds(spell, i);
                            Some((chosen.len().saturating_sub(next_target), min, max))
                        }
                        _ => None,
                    };
                    // Strive: no more targets than the caster can pay for.
                    let strive_capped = !is_ability
                        && many.is_some_and(|(have, ..)| have >= 1)
                        && self.pool.get(def).strive.is_some()
                        && !self.strive_payable(spell);
                    if strive_capped || many.is_some_and(|(have, _, max)| have >= max) {
                        self.flow = Flow::Casting {
                            spell,
                            next_target: specs.len(),
                            sacrifice,
                            discard,
                        };
                        continue;
                    }
                    // "another target permanent", "up to one other target
                    // artifact": also not one chosen before.
                    let another = match self.pool.base_spec(spec) {
                        TargetSpec::OtherCreature
                        | TargetSpec::OtherPermanent
                        | TargetSpec::AnyOther => true,
                        TargetSpec::Filtered(f) => {
                            self.pool.filter(f).relation == crate::card::Relation::Other
                        }
                        _ => false,
                    };
                    // CR 115.3: one "target" word names different objects
                    // ("two target lands"); a spliced card's targets are
                    // its own words (CR 702.47a).
                    let seg = self
                        .splice_segment(spell, next_target)
                        .max(self.mode_segment(spell, next_target));
                    let mut options: Vec<Choice> = self
                        .legal_targets(caster, spec, source)
                        .into_iter()
                        .filter(|t| {
                            // CR 115.5: an ability can't target itself.
                            *t != Target::Object(spell)
                                && !chosen.iter().enumerate().any(|(i, c)| {
                                    i >= seg
                                        && c == t
                                        && crate::card::spec_for(&specs, i)
                                            .is_some_and(|s| s == spec || another)
                                })
                        })
                        .map(Choice::Target)
                        .collect();
                    self.retain_that_player(spell, &specs, spec, &mut options);
                    // An ability's "with mana value X" is judged with the X
                    // announced for it (its source is the permanent).
                    if is_ability
                        && !self.x_values.is_empty()
                        && let Some(x) = self.x_chosen(spell)
                    {
                        options.retain(|c| match c {
                            Choice::Target(t) => {
                                self.is_legal_target_x(caster, spec, *t, source, Some(x))
                            }
                            _ => true,
                        });
                    }
                    // "X target cards from a single graveyard", "up to two
                    // target cards from a single graveyard": the rest come
                    // from the first one's graveyard. A variable group's
                    // first is at its own index; optional targets of one
                    // phrase are consecutive copies of one spec.
                    let first_of_group = match many {
                        Some((have, ..)) if have > 0 => Some(next_target),
                        Some(_) => None,
                        None => (seg..next_target).find(|&i| {
                            specs[i] == spec && chosen.get(i).is_some_and(|t| !t.is_none())
                        }),
                    };
                    if let Some(first) = first_of_group
                        && matches!(
                            self.pool.base_spec(spec),
                            TargetSpec::GraveyardCard {
                                who: crate::card::Who::Any,
                                graveyards: false,
                                ..
                            }
                        )
                        && let Some(Target::Object(first)) =
                            self.objects[spell].targets.get(first).copied()
                    {
                        let owner = self.objects.get(first).map(|o| o.owner);
                        options.retain(|c| match c {
                            Choice::Target(Target::Object(id)) => {
                                self.objects.get(*id).map(|o| o.owner) == owner
                            }
                            _ => true,
                        });
                    }
                    // A cost that depends on the first target: only targets
                    // the caster can pay for (CR 601.2f comes after 601.2c,
                    // but an unpayable choice would be illegal to finish).
                    let cost_checked =
                        next_target == 0 && !is_ability && self.pool.get(def).target_cost.is_some();
                    if cost_checked {
                        options.retain(|c| {
                            let Choice::Target(t) = c else { return true };
                            self.target_payable(spell, *t)
                        });
                    }
                    if optional {
                        if options.is_empty() {
                            // Nothing to choose: declined, and the later
                            // targets are still chosen (CR 601.2c).
                            self.objects[spell].targets.push(Target::none());
                            self.flow = Flow::Casting {
                                spell,
                                next_target: next_target + 1,
                                sacrifice,
                                discard,
                            };
                            continue;
                        }
                        options.push(Choice::Done);
                    }
                    if let Some((have, min, _)) = many
                        && have >= min
                    {
                        if options.is_empty() {
                            self.flow = Flow::Casting {
                                spell,
                                next_target: specs.len(),
                                sacrifice,
                                discard,
                            };
                            continue;
                        }
                        options.push(Choice::Done);
                    }
                    if options.is_empty() && many.is_none() && !cost_checked {
                        // Checked when offered (`targets_available`); keep
                        // going rather than stall if that ever falls short.
                        options = self
                            .legal_targets(caster, spec, source)
                            .into_iter()
                            .map(Choice::Target)
                            .collect();
                        self.retain_that_player(spell, &specs, spec, &mut options);
                    }
                    if options.is_empty() {
                        // Moving the card to the stack changed what is
                        // legal (a power that counts cards in hand): the
                        // cast is undone (CR 726).
                        let _ = def;
                        self.undo_cast(spell, is_ability);
                        continue;
                    }
                    return StepResult::Decision(Decision {
                        player: caster,
                        kind: DecisionKind::ChooseTarget {
                            spell,
                            index: self.objects[spell].targets.len(),
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
                    // CR 508.1d: creatures that attack each combat if able.
                    let must: Vec<ObjId> = eligible
                        .iter()
                        .copied()
                        .filter(|id| {
                            self.characteristics(*id)
                                .is_some_and(|c| c.has(Keyword::MustAttack))
                        })
                        .collect();
                    if incremental {
                        let mut options: Vec<Choice> = if must.is_empty() {
                            eligible.into_iter().map(Choice::AddAttacker).collect()
                        } else {
                            must.iter().copied().map(Choice::AddAttacker).collect()
                        };
                        if must.is_empty() {
                            options.push(Choice::Done);
                        }
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
                        .filter(|set| must.iter().all(|m| set.contains(m)))
                        .filter(|set| self.set_legal(set, false))
                        .map(Choice::Attackers)
                        .collect();
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::DeclareAttackers,
                        options,
                    });
                }
                Flow::FormBands { band, led } => {
                    if band.is_empty() {
                        match self.band_leader(&led) {
                            Some(l) => {
                                self.flow = Flow::FormBands { band: vec![l], led };
                            }
                            None => self.announce_attackers(),
                        }
                        continue;
                    }
                    let candidates = self.band_candidates(&band);
                    if candidates.is_empty() {
                        self.close_band(band, led);
                        continue;
                    }
                    // `Done` first: unscripted, no band.
                    let mut options = vec![Choice::Done];
                    options.extend(candidates.into_iter().map(Choice::AddAttacker));
                    return StepResult::Decision(Decision {
                        player: self.active_player,
                        kind: DecisionKind::FormBand { leader: band[0] },
                        options,
                    });
                }
                Flow::AttackTargets { chosen, remaining } => {
                    let Some(attacker) = remaining.first().copied() else {
                        self.finish_attackers_with(chosen);
                        continue;
                    };
                    let player = self.active_player;
                    let defender = self.opponent(player);
                    let mut options = vec![Choice::AttackTarget(Target::Player(defender))];
                    // CR 701.15b: a goaded creature attacks a player other
                    // than those who goaded it if able; it can always
                    // attack the defending player unless that player goaded
                    // it, so then only that player.
                    let goaders = self.goaders(attacker);
                    if goaders.is_empty() || goaders.contains(&defender) {
                        options.extend(
                            self.planeswalkers_of(defender)
                                .map(|pw| Choice::AttackTarget(Target::Object(pw))),
                        );
                    }
                    options.extend(
                        self.attackable_battles(defender)
                            .map(|b| Choice::AttackTarget(Target::Object(b))),
                    );
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::ChooseAttackTarget { attacker },
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
                    let all_blockers: Vec<ObjId> = self.eligible_blockers(player).collect();
                    // CR 509.1c: requirements; usually none.
                    let reqs = self.block_requirements(&attackers, &all_blockers);
                    // Blockers that can still block another attacker, with
                    // how many more (CR 509.1a).
                    let mut blockers = Vec::new();
                    let mut caps = Vec::new();
                    for b in all_blockers {
                        let used = chosen.iter().filter(|(bl, _)| *bl == b).count();
                        let cap = self.block_capacity(b);
                        if used < cap {
                            blockers.push(b);
                            caps.push(cap - used);
                        }
                    }
                    // Per blocker, the attackers it may (still) block.
                    let can_block: Vec<Vec<ObjId>> = blockers
                        .iter()
                        .map(|b| {
                            attackers
                                .iter()
                                .copied()
                                .filter(|a| !chosen.contains(&(*b, *a)) && self.can_block(*b, *a))
                                .collect()
                        })
                        .collect();
                    if incremental {
                        let mut options = Vec::new();
                        // CR 509.1c, greedily: while some block obeys another
                        // requirement, only such blocks are offered and the
                        // declaration can't stop. (Exact for lures and
                        // creatures that block if able; "must be blocked"
                        // shared among blockers is left to the player.)
                        let mut improving = Vec::new();
                        let obeyed = (!reqs.is_empty()).then(|| reqs.obeyed(&chosen));
                        for (b, attackers_for_b) in blockers.iter().zip(&can_block) {
                            for a in attackers_for_b {
                                let mut trial = chosen.clone();
                                trial.push((*b, *a));
                                if self.block_set_completable(&trial, &blockers, &can_block) {
                                    let choice = Choice::AddBlock {
                                        blocker: *b,
                                        attacker: *a,
                                    };
                                    if obeyed.is_some_and(|n| reqs.obeyed(&trial) > n) {
                                        improving.push(choice.clone());
                                    }
                                    options.push(choice);
                                }
                            }
                        }
                        if !improving.is_empty() {
                            options = improving;
                        } else if self.block_set_legal(&chosen) {
                            options.push(Choice::Done);
                        }
                        if options.is_empty() {
                            // Painted into a corner (menace); drop illegal blocks and finish.
                            let fixed: Vec<(ObjId, ObjId)> = chosen
                                .iter()
                                .copied()
                                .filter(|(_, a)| {
                                    chosen.iter().filter(|(_, a2)| a2 == a).count()
                                        >= self.min_blockers(*a)
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
                    // Saturating: with many blockers the product overflows
                    // and would wrap to a small number.
                    let combos: usize = can_block.iter().zip(&caps).fold(1usize, |n, (v, cap)| {
                        n.saturating_mul(block_choices(v.len(), *cap))
                    });
                    if combos > MAX_SET_OPTIONS {
                        self.flow = Flow::DeclareBlockers {
                            chosen,
                            incremental: true,
                        };
                        continue;
                    }
                    let mut sets: Vec<Vec<(ObjId, ObjId)>> =
                        block_assignments(&blockers, &can_block, &caps)
                            .into_iter()
                            .filter(|s| self.block_set_legal(s))
                            .collect();
                    if !reqs.is_empty() {
                        // CR 509.1c: only declarations obeying the most
                        // requirements any restriction-abiding one can.
                        sets.retain(|s| !self.blocks_alone_illegally(s));
                        let best = sets.iter().map(|s| reqs.obeyed(s)).max().unwrap_or(0);
                        sets.retain(|s| reqs.obeyed(s) == best);
                    }
                    if sets.is_empty() {
                        unreachable!("no legal block declaration");
                    }
                    let options = sets.into_iter().map(Choice::Blocks).collect();
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
                    let player = self.damage_assigner(attacker);
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
                    let max = self.max_hand_size(p);
                    let ps = self.player(p);
                    if ps.hand.len() <= max {
                        self.finish_cleanup();
                        continue;
                    }
                    let options = ps
                        .hand
                        .iter()
                        .map(|id| Choice::Discard {
                            card: *id,
                            def: self.objects[*id].def,
                        })
                        .collect();
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
                Flow::ChoosingMode { spell, .. } => {
                    let player = self.objects[spell].controller;
                    let options = self.mode_options(spell);
                    if options.is_empty() {
                        unreachable!("modal cast offered without a mode with targets");
                    }
                    return StepResult::Decision(Decision {
                        player,
                        kind: DecisionKind::ChooseMode { spell },
                        options,
                    });
                }
                Flow::EndStep => self.end_step(),
            }
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per (`Flow`, `Choice`) pair
    fn apply_choice(&mut self, player: PlayerId, choice: Choice) {
        match (self.flow_now(), choice) {
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
            (Flow::OpeningHand { mut asked }, c @ (Choice::Accept | Choice::Decline)) => {
                if let Some((_, card)) = self.next_opening_hand_card(&asked) {
                    asked.push(card);
                    if c == Choice::Accept {
                        self.move_object(card, Zone::Battlefield, true);
                    }
                }
                self.flow = Flow::OpeningHand { asked };
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
                        if self.end_turn {
                            self.end_turn = false;
                            self.skip_to_cleanup();
                        }
                    }
                } else {
                    self.priority = self.next_player_in_game(player);
                }
            }
            (Flow::Priority, Choice::PlayLand(id)) => {
                self.consecutive_passes = 0;
                // CR 305.1, 110.2a: under the control of the player who
                // played it, even a card they don't own (played from
                // exile).
                let new = self.put_onto_battlefield(id, player);
                self.player_mut(player).lands_played_this_turn += 1;
                self.emit(Event::LandPlayed { player, land: new });
            }
            (Flow::Priority, Choice::CastEvoke(card)) => {
                self.consecutive_passes = 0;
                let spell = self.move_object(card, Zone::Stack, true);
                self.objects[spell].controller = player;
                self.objects[spell].cast.set_evoked(true);
                self.flow = self.after_mode(spell);
            }
            (Flow::Priority, Choice::CastDash(card)) => {
                self.begin_cast(player, card, crate::state::AltCast::Dash);
            }
            (Flow::Priority, Choice::CastSneak(card)) => {
                self.begin_cast(player, card, crate::state::AltCast::Sneak);
            }
            (Flow::Priority, Choice::CastImpending(card)) => {
                self.begin_cast(player, card, crate::state::AltCast::Impending);
            }
            (Flow::Priority, Choice::CastWebSlinging(card)) => {
                self.begin_cast(player, card, crate::state::AltCast::WebSlinging);
            }
            // CR 702.180a: from the graveyard, exiled as it leaves the
            // stack (as with flashback); the creature to tap is chosen now
            // (CR 702.180b) and its power comes off the cost.
            (Flow::Priority, Choice::CastHarmonize { card, tap }) => {
                let power = tap
                    .and_then(|c| self.characteristics(c))
                    .map_or(0, |c| u32::try_from(c.power.max(0)).unwrap_or(0));
                let spell = self.begin_cast(player, card, crate::state::AltCast::Harmonize);
                self.objects[spell].cast.set_flashback(true);
                if let Some(c) = tap {
                    self.harmonized.push((spell, c, power));
                }
            }
            (Flow::Priority, Choice::CastWarp(card)) => {
                self.begin_cast(player, card, crate::state::AltCast::Warp);
            }
            (Flow::Priority, Choice::CastBestow(card)) => {
                // CR 702.103b: on the stack it is the Aura, enchant
                // creature, from the start.
                let face = self.pool.get(self.objects[card].def).bestow_face;
                self.begin_cast_as(player, card, crate::state::AltCast::Bestow, face);
            }
            (Flow::Priority, Choice::CastOverload(card)) => {
                // CR 702.96b: on the stack it is the spell without targets.
                let face = self.pool.get(self.objects[card].def).overload_face;
                self.begin_cast_as(player, card, crate::state::AltCast::Overload, face);
            }
            (Flow::Priority, Choice::CastAwaken(card)) => {
                // CR 702.113a: the spell with the land target and the
                // awaken effects after its own.
                let face = self.pool.get(self.objects[card].def).awaken_face;
                self.begin_cast_as(player, card, crate::state::AltCast::Awaken, face);
            }
            (Flow::Priority, Choice::CastDisturb(card)) => {
                // CR 702.146a, 712.8c: cast transformed, the back face's
                // characteristics on the stack.
                let face = self.pool.get(self.objects[card].def).faces.first().copied();
                self.begin_cast_as(player, card, crate::state::AltCast::Disturb, face);
            }
            (Flow::Priority, Choice::CastAlt(card)) => {
                // CR 718.3: a prototype card's alternative cost casts it
                // prototyped, the spell its prototype face.
                // CR 702.148a: a cleaved spell is its cleave face.
                let def = self.pool.get(self.objects[card].def);
                match def.prototype_face.or(def.cleave_face) {
                    Some(face) => {
                        self.begin_cast_as(
                            player,
                            card,
                            crate::state::AltCast::AltCost,
                            Some(face),
                        );
                    }
                    None => {
                        self.begin_cast(player, card, crate::state::AltCast::AltCost);
                    }
                }
            }
            (Flow::Priority, Choice::CastFaceDown(card)) => {
                self.consecutive_passes = 0;
                let ward = matches!(
                    self.pool.get(self.objects[card].def).morph.as_deref(),
                    Some((_, crate::card::MorphKind::Disguise))
                );
                let spell = self.move_object(card, Zone::Stack, true);
                self.objects[spell].controller = player;
                // CR 708.2a: the spell has only face-down characteristics.
                self.objects[spell].def = self.pool.face_down(ward);
                self.cbump();
                self.flow = self.after_mode(spell);
            }
            (Flow::Priority, Choice::Unlock { obj, door }) => {
                // CR 709.5e: a special action; the player keeps priority.
                let half = self
                    .pool
                    .get(self.objects[obj].def)
                    .doors
                    .as_deref()
                    .map(|d| d.half(door));
                if let Some(half) = half {
                    let req = self.pool.get(half).mana_cost.requirements(0);
                    self.pay(player, &req);
                    self.unlock(obj, door);
                }
                self.give_priority(player);
            }
            (Flow::Priority, Choice::TurnFaceUp(id)) => {
                // CR 116.2b: a special action; the player keeps priority.
                if let Some(cost) = self.face_up_cost(id) {
                    // "Morph—Discard a card.", "Morph—Return a Bird you
                    // control to its owner's hand.": the player chooses.
                    if cost.discard > 0 || matches!(cost.sacrifice, Some(SacCost::Filter(_))) {
                        self.flow = Flow::TurningFaceUp { obj: id };
                        return;
                    }
                    self.pay_face_up(player, id, &cost, None);
                }
                self.give_priority(player);
            }
            (Flow::Priority, Choice::CastFace { card, .. })
                if self.objects[card].zone == Zone::Battlefield =>
            {
                self.cast_prepared(player, card);
            }
            (Flow::Priority, Choice::CastFace { card, face })
                if self.land_face(card, face).is_some() =>
            {
                // CR 712.12: a modal double-faced card played as its back
                // face enters with that face up.
                self.consecutive_passes = 0;
                if let Some(f) = self.land_face(card, face) {
                    self.objects[card].def = f;
                }
                let new = self.move_object(card, Zone::Battlefield, true);
                self.player_mut(player).lands_played_this_turn += 1;
                self.emit(Event::LandPlayed { player, land: new });
            }
            (Flow::Priority, Choice::CastFace { card, face }) => {
                self.cast_face(player, card, face, false);
            }
            // CR 702.127a: a split card in the graveyard is cast as its
            // aftermath half, and exiled as it leaves the stack (as with
            // flashback).
            (Flow::Priority, Choice::Cast(id))
                if self.objects[id].zone == Zone::Graveyard
                    && self.aftermath_index(id).is_some() =>
            {
                let face = self.aftermath_index(id).unwrap_or(0);
                self.cast_face(player, id, face, true);
            }
            (Flow::Priority, Choice::Cast(id)) => {
                // CR 702.34a: a card cast from the graveyard is cast with
                // flashback.
                let o = &self.objects[id];
                let gdef = self.pool.get(o.def);
                // Flashback granted until end of turn: for its mana cost.
                let escape = gdef.flashback.is_none() && !self.granted_flashback.contains(&id);
                // Mayhem: a card discarded this turn, cast for its mayhem
                // cost (a card with escape too uses escape only when it
                // wasn't discarded this turn).
                let mayhem = escape && gdef.mayhem.is_some() && self.mayhem_discarded.contains(&id);
                let flashback = o.zone == Zone::Graveyard && !escape;
                let alt = match (o.zone, o.alt) {
                    (Zone::Hand, _) => crate::state::AltCast::Hand,
                    (Zone::Graveyard, _) if mayhem => crate::state::AltCast::Mayhem,
                    (Zone::Graveyard, _) if escape => crate::state::AltCast::Escape,
                    (Zone::Exile, crate::state::AltCast::Foretold) => {
                        crate::state::AltCast::Foretell
                    }
                    // CR 702.170d: without paying its mana cost.
                    (Zone::Exile, crate::state::AltCast::Plotted) => crate::state::AltCast::Free,
                    // "You may cast that card without paying its mana
                    // cost."
                    (Zone::Exile, _)
                        if self.exile_casts.as_ref().is_some_and(|e| {
                            e.playable
                                .iter()
                                .any(|pe| pe.card == id && pe.player == player && pe.free)
                        }) =>
                    {
                        crate::state::AltCast::Free
                    }
                    // CR 701.65a: for {2}.
                    (Zone::Exile, crate::state::AltCast::Airbent) => crate::state::AltCast::Airbend,
                    _ => crate::state::AltCast::None,
                };
                // "Once each turn, you may cast ... from the top of your
                // library": that permission is used unless another lets
                // the card be cast.
                if self.objects[id].zone == Zone::Library {
                    let perms = self.top_permissions(player, id);
                    if !perms.iter().any(|(_, _, once)| !*once)
                        && let Some((src, a, _)) = perms.first()
                    {
                        self.activated_once.push((*src, *a));
                    }
                }
                let spell = self.begin_cast(player, id, alt);
                self.objects[spell].cast.set_flashback(flashback);
            }
            (Flow::Priority, Choice::Suspend(id)) => {
                // CR 702.62a, 116.2f: a special action; the player keeps
                // priority.
                let def = self.pool.get(self.objects[id].def);
                if let Some((n, cost)) = def.suspend.as_deref().cloned() {
                    self.pay(player, &cost.requirements(0));
                    let new = self.move_object(id, Zone::Exile, true);
                    let o = &mut self.objects[new];
                    o.alt = crate::state::AltCast::Suspended;
                    o.add_counters(crate::card::CounterKind::Time, n);
                    self.suspended_count += 1;
                }
                self.give_priority(player);
            }
            (Flow::Priority, Choice::Foretell(id)) => {
                // CR 702.143a, 116.2h: a special action for {2}, which
                // mana restricted to foretelling may pay (CR 106.6).
                let sources = self.foretell_sources(player);
                self.pay_from(
                    player,
                    &crate::mana::ManaCost::parse("{2}")
                        .expect("a mana cost")
                        .requirements(0),
                    &sources,
                    None,
                );
                let new = self.move_object(id, Zone::Exile, true);
                self.objects[new].alt = crate::state::AltCast::Foretold;
                self.player_mut(player).adventures_in_exile += 1;
                self.exile_casts
                    .get_or_insert_with(Box::default)
                    .foretold_this_turn
                    .push(new);
                self.give_priority(player);
            }
            (Flow::Priority, Choice::Plot(id)) => {
                // CR 702.170a-b: a special action; the player keeps
                // priority.
                let def = self.pool.get(self.objects[id].def);
                if let Some(cost) = def.plot.as_deref().cloned() {
                    self.pay(player, &cost.requirements(0));
                    self.plot(id);
                }
                self.give_priority(player);
            }
            (
                Flow::ChoosingMode {
                    spell,
                    sacrifice,
                    discard,
                },
                Choice::Mode(i),
            ) => {
                self.objects[spell]
                    .cast
                    .set_mode(u8::try_from(i).unwrap_or(0));
                // "Choose one that hasn't been chosen [this turn]" (CR
                // 700.2): remembered for the source's ability.
                if let Some(a) = self.objects[spell].ability
                    && let Some(once) = self.modes_of(spell).and_then(|m| m.get(i)).map(|m| m.once)
                    && once != crate::card::ModeOnce::Any
                {
                    self.modes_chosen.push((
                        a.source,
                        a.ability,
                        u8::try_from(i).unwrap_or(0),
                        once == crate::card::ModeOnce::ThisTurn,
                    ));
                }
                // An activated ability goes on to X and its mode's targets
                // with what its cost chose (CR 602.2b).
                let activated = self.objects[spell].ability.is_some_and(|a| {
                    matches!(
                        self.pool.ability(a.ability),
                        crate::card::AbilityDef::Activated(_)
                    )
                });
                self.flow = if activated {
                    self.after_activated_mode(spell, sacrifice, discard)
                } else {
                    self.after_mode(spell)
                };
            }
            (
                Flow::ChoosingX {
                    spell,
                    sacrifice,
                    discard,
                },
                Choice::X(x),
            ) => {
                self.x_values.push((spell, x));
                self.flow = Flow::Casting {
                    spell,
                    next_target: 0,
                    sacrifice,
                    discard,
                };
            }
            (Flow::ChoosingAdditional { spell }, Choice::Mode(i)) => {
                self.objects[spell].cast.set_other_cost(i == 1);
                self.flow = self.after_additional(spell);
            }
            (Flow::ChoosingKicker { spell }, Choice::Accept) => {
                self.objects[spell].cast.set_kicked(true);
                self.flow = self.after_kicker(spell);
            }
            (Flow::ChoosingKicker { spell }, Choice::Decline) => {
                self.flow = self.after_kicker(spell);
            }
            (Flow::ChoosingBuyback { spell }, Choice::Accept) => {
                self.objects[spell].cast.set_bought_back(true);
                self.flow = self.after_buyback(spell);
            }
            (Flow::ChoosingBuyback { spell }, Choice::Sacrifice(s)) => {
                self.objects[spell].cast.set_bought_back(true);
                self.bargains.push((spell, s));
                self.flow = self.after_buyback(spell);
            }
            (Flow::ChoosingBuyback { spell }, Choice::Decline) => {
                self.flow = self.after_buyback(spell);
            }
            (Flow::ChoosingOffspring { spell }, Choice::Accept) => {
                self.objects[spell].cast.set_offspring(true);
                self.flow = self.after_offspring(spell);
            }
            (Flow::ChoosingOffspring { spell }, Choice::Decline) => {
                self.flow = self.after_offspring(spell);
            }
            // CR 702.56a: the replicate cost, paid this many times.
            // CR 702.33c: the multikicker cost, paid this many times.
            (Flow::ChoosingMultikicker { spell }, Choice::X(n)) => {
                self.objects[spell].cast.set_kicks(n);
                self.flow = self.after_kicker(spell);
            }
            (Flow::ChoosingReplicate { spell }, Choice::X(n)) => {
                self.objects[spell].cast.set_replicated(n);
                self.flow = self.after_replicate(spell);
            }
            // CR 702.166a: the sacrifice is an additional cost, paid with
            // the others; the spell is bargained from here on, for its
            // cost and text (CR 601.2f).
            (Flow::ChoosingBargain { spell }, Choice::Sacrifice(s)) => {
                self.objects[spell].cast.set_bargained(true);
                self.bargains.push((spell, s));
                self.flow = self.after_bargain(spell);
            }
            (Flow::ChoosingBargain { spell }, Choice::Decline) => {
                self.flow = self.after_bargain(spell);
            }
            // CR 702.153a: the sacrifice is an additional cost, paid with
            // the others (kept with bargain's).
            (Flow::ChoosingCasualty { spell }, Choice::Sacrifice(s)) => {
                self.objects[spell].cast.set_casualty(true);
                self.bargains.push((spell, s));
                self.flow = self.after_casualty(spell);
            }
            (Flow::ChoosingCasualty { spell }, Choice::Decline) => {
                self.flow = self.after_casualty(spell);
            }
            // CR 702.174k: the gift was promised (kept as the kicker flag).
            (Flow::ChoosingGift { spell }, Choice::Accept) => {
                self.objects[spell].cast.set_kicked(true);
                self.flow = self.after_gift(spell);
            }
            (Flow::ChoosingGift { spell }, Choice::Decline) => {
                self.flow = self.after_gift(spell);
            }
            // Teamwork: the creatures are tapped as the costs are paid.
            (Flow::ChoosingTeamwork { spell }, Choice::Accept) => {
                self.objects[spell].cast.set_teamwork(true);
                self.flow = self.after_teamwork(spell);
            }
            (Flow::ChoosingTeamwork { spell }, Choice::Decline) => {
                self.flow = self.after_teamwork(spell);
            }
            // CR 601.2b: the optional additional cost, paid with the others.
            (Flow::ChoosingOptionalCost { spell }, Choice::Accept) => {
                self.objects[spell].cast.set_optional_paid(true);
                self.flow = self.after_optional_cost(spell);
            }
            (Flow::ChoosingOptionalCost { spell }, Choice::Decline) => {
                self.flow = self.after_optional_cost(spell);
            }
            // CR 702.78a: the creatures are tapped as the costs are paid.
            (Flow::ChoosingConspire { spell }, Choice::Accept) => {
                self.objects[spell].cast.set_conspired(true);
                self.flow = self.after_conspire(spell);
            }
            (Flow::ChoosingConspire { spell }, Choice::Decline) => {
                self.flow = self.after_conspire(spell);
            }
            // CR 702.47a: the card is revealed and stays in hand; the spell
            // has its effects and targets, its splice cost paid with the
            // spell's (CR 601.2f).
            (Flow::ChoosingSplice { spell }, Choice::Discard { card, def }) => {
                let i = if let Some(i) = self.splices.iter().position(|s| s.spell == spell) {
                    i
                } else {
                    let targets = self.targets_of(spell).to_vec();
                    self.splices.push(crate::state::Spliced {
                        spell,
                        cards: smallvec::SmallVec::new(),
                        targets,
                    });
                    self.splices.len() - 1
                };
                let added = self.pool.get(def).cast_targets().to_vec();
                let s = &mut self.splices[i];
                s.cards.push((card, def));
                s.targets.extend(added);
                self.emit(Event::Revealed { obj: card, def });
            }
            (Flow::ChoosingSplice { spell }, Choice::Done) => {
                self.flow = self.after_splice(spell);
            }
            (
                Flow::Priority,
                Choice::Activate {
                    source,
                    ability,
                    sacrifice,
                    discard,
                },
            ) => {
                self.activate(player, source, ability, sacrifice, discard);
            }
            (
                Flow::Casting {
                    spell,
                    next_target,
                    sacrifice,
                    discard,
                },
                Choice::Done,
            ) => {
                // A variable group stops (it is the last spec); a declined
                // "up to one" target keeps its slot so the later targets
                // are still chosen (CR 601.2c).
                let many = matches!(
                    self.targets_of(spell).get(next_target),
                    Some(TargetSpec::Many(_)) | None
                );
                if !many {
                    self.objects[spell].targets.push(Target::none());
                }
                self.flow = Flow::Casting {
                    spell,
                    next_target: if many { usize::MAX } else { next_target + 1 },
                    sacrifice,
                    discard,
                };
            }
            (
                Flow::Casting {
                    spell,
                    next_target,
                    sacrifice,
                    discard,
                },
                Choice::Target(t),
            ) => {
                self.objects[spell].targets.push(t);
                // A variable group stays at its spec for more targets.
                let many = matches!(
                    self.targets_of(spell).get(next_target),
                    Some(TargetSpec::Many(_))
                );
                self.flow = Flow::Casting {
                    spell,
                    next_target: next_target + usize::from(!many),
                    sacrifice,
                    discard,
                };
            }
            (Flow::Casting { spell, .. }, Choice::Divide(parts)) => {
                self.divisions.push((spell, parts));
            }
            (
                Flow::Casting {
                    spell,
                    next_target,
                    discard,
                    ..
                },
                Choice::Sacrifice(id),
            ) => {
                self.flow = Flow::Casting {
                    spell,
                    next_target,
                    sacrifice: Some(id),
                    discard,
                };
            }
            (
                Flow::Casting {
                    spell,
                    next_target,
                    sacrifice,
                    ..
                },
                Choice::Discard { card: id, .. },
            ) => {
                self.flow = Flow::Casting {
                    spell,
                    next_target,
                    sacrifice,
                    discard: Some(id),
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
            (Flow::FormBands { mut band, led }, Choice::AddAttacker(id)) => {
                band.push(id);
                self.flow = Flow::FormBands { band, led };
            }
            (Flow::FormBands { band, led }, Choice::Done) => self.close_band(band, led),
            (Flow::DeclareAttackers { chosen, .. }, Choice::Done) => {
                self.finish_attackers(chosen);
            }
            (
                Flow::AttackTargets {
                    mut chosen,
                    mut remaining,
                },
                Choice::AttackTarget(t),
            ) => {
                let attacker = remaining.remove(0);
                chosen.push((attacker, t));
                self.flow = Flow::AttackTargets { chosen, remaining };
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
            (
                Flow::TurningFaceUp { obj },
                Choice::Sacrifice(pick) | Choice::Discard { card: pick, .. },
            ) => {
                self.flow = Flow::Priority;
                if let Some(cost) = self.face_up_cost(obj) {
                    self.pay_face_up(player, obj, &cost, Some(pick));
                }
                self.give_priority(player);
            }
            (Flow::Discarding, Choice::Discard { card: id, .. }) => {
                self.discard_card(player, id);
            }
            (Flow::OrderTriggers { .. }, Choice::Trigger(i)) => {
                self.put_trigger(i);
            }
            (flow, choice) => unreachable!("choice {choice:?} does not fit flow {flow:?}"),
        }
    }

    /// The casting state after the mode (if any) is chosen: X if the cost
    /// has one, else targets.
    /// CR 601.2a: the card moves to the stack, cast `alt`'s way; modes come
    /// before X, X before targets (CR 601.2b).
    fn begin_cast(&mut self, player: PlayerId, card: ObjId, alt: crate::state::AltCast) -> ObjId {
        self.begin_cast_as(player, card, alt, None)
    }

    /// `begin_cast`, the spell having the characteristics of `def` if
    /// given (a bestowed Aura, CR 702.103b).
    fn begin_cast_as(
        &mut self,
        player: PlayerId,
        card: ObjId,
        alt: crate::state::AltCast,
        def: Option<crate::card::CardRef>,
    ) -> ObjId {
        self.consecutive_passes = 0;
        let from = self.objects[card].zone;
        // CR 307.1: whether a sorcery could have been cast now.
        let sorcery_timing =
            player == self.active_player && self.step.is_main() && self.stack.is_empty();
        // A copy of a card cast from exile (cipher, CR 702.99a) is a copy
        // on the stack too (CR 707.12).
        let copy = self.objects[card].cast.copy();
        let spell = self.move_object(card, Zone::Stack, true);
        if copy {
            self.objects[spell].cast = self.objects[spell].cast.copied();
        }
        self.objects[spell].controller = player;
        self.objects[spell].alt = alt;
        if let Some(d) = def {
            self.objects[spell].def = d;
            self.cbump();
        }
        // CR 601.2a: "if you cast it from your hand".
        self.objects[spell].cast.set_cast_from(from);
        let def = self.pool.get(self.objects[spell].def);
        if def.flash_cleanup && !sorcery_timing {
            self.objects[spell].cast.set_instant_timing();
        }
        // "If you cast this spell during your main phase": judged now.
        if let Some(c) = def.cast_snapshot
            && self.condition_now(self.pool.condition(c), player, spell)
        {
            self.objects[spell].cast.set_as_cast();
        }
        self.flow = if def.modes().is_some() {
            Flow::ChoosingMode {
                spell,
                sacrifice: None,
                discard: None,
            }
        } else {
            self.after_mode(spell)
        };
        spell
    }

    /// CR 702.22c: the band being formed is finished; on to the next
    /// leader.
    fn close_band(&mut self, band: Vec<ObjId>, mut led: Vec<ObjId>) {
        led.push(band[0]);
        if band.len() >= 2
            && let Some(c) = self.combat.as_mut()
        {
            c.bands.push(band);
        }
        self.flow = Flow::FormBands {
            band: Vec::new(),
            led,
        };
    }

    /// The players who goaded `creature` (CR 701.15a): by an effect, or
    /// by the static ability of a permanent attached to it ("enchanted
    /// creature is goaded").
    fn goaders(&self, creature: ObjId) -> smallvec::SmallVec<[PlayerId; 2]> {
        let mut out: smallvec::SmallVec<[PlayerId; 2]> = self
            .goads
            .iter()
            .filter(|(g, _)| *g == creature)
            .map(|(_, by)| *by)
            .collect();
        for id in &self.battlefield {
            let o = &self.objects[*id];
            if o.attached_to == Some(creature) && self.pool.get(o.def).goads {
                out.push(o.controller);
            }
        }
        out
    }

    fn after_activated_mode(
        &self,
        spell: ObjId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    ) -> Flow {
        let has_x = self.objects[spell].ability.is_some_and(|a| {
            matches!(
                self.pool.ability(a.ability),
                crate::card::AbilityDef::Activated(ab) if ab.cost.mana.has_x()
                    || ab.cost.exile_cards.as_ref().is_some_and(|g| g.x)
            )
        });
        if has_x {
            Flow::ChoosingX {
                spell,
                sacrifice,
                discard,
            }
        } else {
            Flow::Casting {
                spell,
                next_target: 0,
                sacrifice,
                discard,
            }
        }
    }

    fn after_mode(&self, spell: ObjId) -> Flow {
        // A modal triggered ability goes on to its mode's targets.
        if self.objects[spell].kind == ObjKind::Ability {
            return if self.targets_of(spell).is_empty() {
                Flow::Priority
            } else {
                Flow::Casting {
                    spell,
                    next_target: 0,
                    sacrifice: None,
                    discard: None,
                }
            };
        }
        // CR 601.2b: which additional cost, then the kicker, before X.
        if self
            .pool
            .get(self.objects[spell].def)
            .additional_or
            .is_some()
        {
            return Flow::ChoosingAdditional { spell };
        }
        self.after_additional(spell)
    }

    fn after_additional(&self, spell: ObjId) -> Flow {
        // A kicked spell needs the targets only kicking gives it (CR
        // 601.2c).
        let def = self.pool.get(self.objects[spell].def);
        let kicked_targets = def.kicker_targets.is_none_or(|_| {
            let p = self.objects[spell].controller;
            def.spell_ability()
                .is_none_or(|(t, _)| self.targets_available(p, t, Some(spell)))
        }) && (def.kicked_drops.is_none()
            || self.targets_available(
                self.objects[spell].controller,
                def.kicked_cast_targets(),
                Some(spell),
            ));
        if kicked_targets && self.kicker_payable(spell) {
            Flow::ChoosingKicker { spell }
        } else if self.multikicker_max(spell) > 0 {
            // CR 702.33c: multikicker, payable at least once.
            Flow::ChoosingMultikicker { spell }
        } else {
            self.after_kicker(spell)
        }
    }

    /// Accept or decline an optional cost, or only the answer the chosen
    /// modes require (`linked_cost_answer`).
    fn linked_cost_options(&self, spell: ObjId) -> Vec<Choice> {
        match self.linked_cost_answer(spell) {
            Some(true) => vec![Choice::Accept],
            Some(false) => vec![Choice::Decline],
            None => vec![Choice::Accept, Choice::Decline],
        }
    }

    fn after_kicker(&self, spell: ObjId) -> Flow {
        // CR 702.174a: a gift, with an opponent to promise it to and the
        // targets only a promised gift gives (CR 702.174m).
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        if def.gift.is_some() && !o.cast.kicked() {
            let p = o.controller;
            let targets = def.kicker_targets.is_none_or(|_| {
                def.spell_ability()
                    .is_none_or(|(t, _)| self.targets_available(p, t, Some(spell)))
            }) && (def.kicked_drops.is_none()
                || self.targets_available(p, def.kicked_cast_targets(), Some(spell)));
            if targets && self.players_in_game().any(|q| q != p) {
                return Flow::ChoosingGift { spell };
            }
        }
        self.after_gift(spell)
    }

    fn after_gift(&self, spell: ObjId) -> Flow {
        if self.buyback_payable(spell) {
            Flow::ChoosingBuyback { spell }
        } else {
            self.after_buyback(spell)
        }
    }

    fn after_buyback(&self, spell: ObjId) -> Flow {
        if self.offspring_payable(spell) {
            Flow::ChoosingOffspring { spell }
        } else {
            self.after_offspring(spell)
        }
    }

    fn after_offspring(&self, spell: ObjId) -> Flow {
        // CR 702.56a: replicate, payable at least once.
        if self.replicate_max(spell) > 0 {
            Flow::ChoosingReplicate { spell }
        } else {
            self.after_replicate(spell)
        }
    }

    fn after_replicate(&self, spell: ObjId) -> Flow {
        let o = &self.objects[spell];
        if self.pool.get(o.def).bargain
            && !o.cast.flashback()
            && !self.bargain_candidates(o.controller).is_empty()
        {
            Flow::ChoosingBargain { spell }
        } else {
            self.after_bargain(spell)
        }
    }

    fn after_bargain(&self, spell: ObjId) -> Flow {
        // CR 702.153a: casualty, with a creature to sacrifice.
        let o = &self.objects[spell];
        let n = self.pool.get(o.def).casualty;
        if n > 0 && !self.casualty_candidates(o.controller, n).is_empty() {
            return Flow::ChoosingCasualty { spell };
        }
        self.after_casualty(spell)
    }

    fn after_casualty(&self, spell: ObjId) -> Flow {
        // Teamwork, with creatures to tap.
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let n = def.teamwork;
        if n > 0
            && self.teamwork_tappers(o.controller, n).is_some()
            && (def.kicked_drops.is_none()
                || self.targets_available(o.controller, def.kicked_cast_targets(), Some(spell)))
        {
            return Flow::ChoosingTeamwork { spell };
        }
        self.after_teamwork(spell)
    }

    fn after_teamwork(&self, spell: ObjId) -> Flow {
        // "As an additional cost to cast this spell, you may collect
        // evidence 6", when it could be paid.
        let o = &self.objects[spell];
        if let Some(c) = self.pool.get(o.def).optional_cost.as_deref()
            && self.other_costs_payable(o.controller, c, spell)
        {
            return Flow::ChoosingOptionalCost { spell };
        }
        self.after_optional_cost(spell)
    }

    fn after_optional_cost(&self, spell: ObjId) -> Flow {
        // CR 702.78a: conspire, with two creatures to tap.
        let o = &self.objects[spell];
        if self.pool.get(o.def).conspire && self.conspirators(spell).len() >= 2 {
            return Flow::ChoosingConspire { spell };
        }
        self.after_conspire(spell)
    }

    /// CR 702.78a: the untapped creatures `spell`'s caster controls that
    /// share a color with it, the ones to tap first first (least power,
    /// those that don't make mana last).
    pub(super) fn conspirators(&self, spell: ObjId) -> Vec<ObjId> {
        let Some(colors) = self.characteristics(spell).map(|c| c.colors) else {
            return Vec::new();
        };
        let p = self.objects[spell].controller;
        let mut out: Vec<(bool, i16, ObjId)> = self
            .creatures_of(p)
            .filter(|id| !self.objects[*id].tapped)
            .filter_map(|id| {
                let c = self.characteristics(id)?;
                c.colors.intersects(colors).then(|| {
                    let mana = self
                        .pool
                        .get(self.objects[id].def)
                        .abilities
                        .iter()
                        .any(|a| matches!(a, crate::card::AbilityDef::TapForMana(_)));
                    (mana, c.power, id)
                })
            })
            .collect();
        out.sort_by_key(|(mana, power, _)| (*mana, *power));
        out.into_iter().map(|(_, _, id)| id).collect()
    }

    fn after_conspire(&self, spell: ObjId) -> Flow {
        // CR 702.47a: cards to splice onto an Arcane spell, revealed as
        // costs are announced.
        if !self.splice_candidates(spell).is_empty() {
            return Flow::ChoosingSplice { spell };
        }
        self.after_splice(spell)
    }

    /// Cards in the caster's hand that could be spliced onto `spell` now
    /// (CR 702.47a): with splice onto Arcane, onto an Arcane spell, not
    /// spliced onto it already (CR 702.47c), their cost payable on top of
    /// the spell's and their targets available.
    fn splice_candidates(&self, spell: ObjId) -> Vec<ObjId> {
        let o = &self.objects[spell];
        let p = o.controller;
        if !self
            .characteristics(spell)
            .is_some_and(|c| c.has_subtype(crate::card::Subtype::ARCANE))
        {
            return Vec::new();
        }
        let done = self.splices.iter().find(|s| s.spell == spell);
        let base = self.spell_requirements(spell, 0);
        self.player(p)
            .hand
            .iter()
            .copied()
            .filter(|id| {
                let def = self.pool.get(self.objects[*id].def);
                let Some(cost) = &def.splice else {
                    return false;
                };
                if done.is_some_and(|s| s.cards.iter().any(|(c, _)| c == id)) {
                    return false;
                }
                let mut req = base;
                req.add(&cost.mana.requirements(0));
                self.spell_payable(p, spell, &req)
                    && self.additional_payable(p, cost, spell)
                    && self.targets_available(p, def.unkicked_cast_targets(), Some(spell))
            })
            .collect()
    }

    /// Where the part of `spell`'s targets that target `index` belongs to
    /// begins: the spell's own (0), or a spliced card's (CR 702.47a).
    /// Where the part of a combined mode ("choose two") holding target
    /// `index` begins: each mode's "target" words are its own (CR 115.3).
    fn mode_segment(&self, spell: ObjId, index: usize) -> usize {
        self.mode_splits_of(spell)
            .into_iter()
            .map(|(_, t)| t)
            .filter(|&t| t <= index)
            .max()
            .unwrap_or(0)
    }

    fn splice_segment(&self, spell: ObjId, index: usize) -> usize {
        if self.splices.is_empty() {
            return 0;
        }
        let Some(sp) = self.splices.iter().find(|s| s.spell == spell) else {
            return 0;
        };
        let mut start = crate::state::targets_in(&self.pool, &self.objects[spell]).len();
        let mut seg = 0;
        for (_, d) in &sp.cards {
            if index < start {
                break;
            }
            seg = start;
            start += self.pool.get(*d).cast_targets().len();
        }
        seg
    }

    fn after_splice(&self, spell: ObjId) -> Flow {
        let def = self.pool.get(self.objects[spell].def);
        let add = self.additional_of(spell);
        if def.mana_cost.has_x()
            || add.discard_x
            || add.life_x
            || add.exile_cards.as_ref().is_some_and(|g| g.x)
        {
            Flow::ChoosingX {
                spell,
                sacrifice: None,
                discard: None,
            }
        } else {
            Flow::Casting {
                spell,
                next_target: 0,
                sacrifice: None,
                discard: None,
            }
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

    /// The next card a player may begin the game with on the battlefield
    /// (CR 103.6a), in turn order from the starting player, skipping those
    /// already asked about.
    fn next_opening_hand_card(&self, asked: &[ObjId]) -> Option<(PlayerId, ObjId)> {
        let mut p = self.starting_player;
        for _ in 0..self.num_players() {
            for id in &self.player(p).hand {
                if !asked.contains(id)
                    && self
                        .pool
                        .get(self.objects[*id].def)
                        .abilities
                        .iter()
                        .any(|a| matches!(a, crate::card::AbilityDef::OpeningHand))
                {
                    return Some((p, *id));
                }
            }
            p = self.next_player(p);
        }
        None
    }

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

    /// CR 502.1, 502.3: the active player's permanents phase (CR 702.26a),
    /// then untap.
    fn untap_all(&mut self, active: PlayerId) {
        let phased_in = if self.phased_out.is_empty() {
            smallvec::SmallVec::new()
        } else {
            self.phase_in_all(active)
        };
        let perms: Vec<ObjId> = self.permanents_of(active).collect();
        let mut phasing: smallvec::SmallVec<[ObjId; 2]> = smallvec::SmallVec::new();
        for id in perms {
            let (phases, doesnt_untap) = self.characteristics(id).map_or((false, false), |c| {
                (c.has(Keyword::Phasing), c.has(Keyword::DoesntUntap))
            });
            // Those with phasing phase out instead of untapping, all at
            // once with those phasing in (which don't phase out again).
            if phases && !phased_in.contains(&id) {
                phasing.push(id);
                continue;
            }
            let o = &mut self.objects[id];
            o.controlled_since_turn_start = true;
            if std::mem::take(&mut o.skip_untap) || doesnt_untap {
                continue;
            }
            self.untap(id);
        }
        for id in phasing {
            self.phase_out(id, false);
        }
    }

    /// "Untap this creature during each other player's untap step" (CR
    /// 502.3): the other players' permanents with it untap too.
    fn untap_others(&mut self, active: PlayerId) {
        let ids: smallvec::SmallVec<[ObjId; 4]> = self
            .battlefield
            .iter()
            .copied()
            .filter(|id| {
                let o = &self.objects[*id];
                o.controller != active
                    && o.tapped
                    && self.characteristics(*id).is_some_and(|c| {
                        c.has(Keyword::UntapsDuringOthers) && !c.has(Keyword::DoesntUntap)
                    })
            })
            .collect();
        for id in ids {
            self.untap(id);
        }
    }

    /// Turn-based actions at the start of the current step (CR 500.1 and the
    /// step rules), then priority or straight to the end of the step.
    fn begin_step(&mut self) {
        self.emit(Event::StepBegin { step: self.step });
        let active = self.active_player;
        match self.step {
            Step::Untap => {
                // CR 502.2, 731.2: day may become night, or night day.
                if self.desig.day_night != crate::state::DayNight::Neither {
                    self.day_night_check();
                }
                self.untap_all(active);
                self.untap_others(active);
                self.player_mut(active).lands_played_this_turn = 0;
                for p in 0..self.num_players() {
                    self.players[p].extra_land_plays = 0;
                }
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
                self.combats_this_turn = self.combats_this_turn.saturating_add(1);
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
                self.entered_x.clear();
                self.reflexive_x.clear();
                self.reflexive_moved.clear();
                self.flow = Flow::Discarding;
            }
            // CR 714.3b: as the precombat main phase begins, the active
            // player puts a lore counter on each Saga they control.
            Step::PrecombatMain if self.saga_count > 0 => {
                let sagas: Vec<ObjId> = self
                    .permanents_of(active)
                    .filter(|id| self.pool.get(self.objects[*id].def).final_chapter > 0)
                    .collect();
                for id in sagas {
                    self.add_counters(id, crate::card::CounterKind::Lore, 1);
                }
                self.give_priority(active);
            }
            Step::Upkeep
            | Step::PrecombatMain
            | Step::EndOfCombat
            | Step::PostcombatMain
            | Step::End => self.give_priority(active),
        }
    }

    /// CR 722.3c: the copy of a prepared permanent's prepare spell is
    /// made in exile and cast from there; the permanent stops being
    /// prepared as it is cast (CR 601.2i). The copy remembers the
    /// permanent in `attached_to` (unused on the stack) in case the cast
    /// is rewound.
    fn cast_prepared(&mut self, player: PlayerId, perm: ObjId) {
        let Some(face) = self.prepare_spell(perm) else {
            return;
        };
        self.objects[perm].marks &= !crate::state::OBJ_PREPARED;
        let copy = self.new_object(face, player, Zone::Exile);
        self.objects[copy].cast = self.objects[copy].cast.copied();
        self.player_mut(player).exile.push(copy);
        let spell = self.begin_cast(player, copy, crate::state::AltCast::None);
        self.objects[spell].attached_to = Some(perm);
    }

    /// CR 726: a spell or ability that turns out to have no legal target
    /// as it is put on the stack is rewound; nothing has been paid yet.
    fn undo_cast(&mut self, spell: ObjId, is_ability: bool) {
        let o = &self.objects[spell];
        let (controller, source, def) = (o.controller, o.ability.map(|a| a.source), o.def);
        if is_ability {
            self.emit(Event::TriggerRemoved {
                source: source.unwrap_or(spell),
                def,
            });
            self.remove_from_game(spell);
        } else if o.cast.copy() {
            // A copy cast from exile (CR 722.3c, 702.99a) goes back to
            // being an uncast copy: none remains; a prepared permanent is
            // prepared again.
            if let Some(perm) = self.objects[spell].attached_to
                && self
                    .objects
                    .get(perm)
                    .is_some_and(|p| p.zone == Zone::Battlefield)
            {
                self.objects[perm].marks |= crate::state::OBJ_PREPARED;
            }
            self.x_values.retain(|(id, _)| *id != spell);
            self.remove_from_game(spell);
        } else {
            let back = if o.cast.flashback() {
                Zone::Graveyard
            } else {
                Zone::Hand
            };
            self.x_values.retain(|(id, _)| *id != spell);
            self.divisions.retain(|(id, _)| *id != spell);
            self.bargains.retain(|(id, _)| *id != spell);
            self.harmonized.retain(|(id, ..)| *id != spell);
            self.splices.retain(|s| s.spell != spell);
            let card = self.move_object(spell, back, false);
            self.give_priority(controller);
            self.undone_cast = Some((card, self.version));
            return;
        }
        self.give_priority(controller);
    }

    fn give_priority(&mut self, p: PlayerId) {
        self.priority = p;
        self.consecutive_passes = 0;
        self.flow = Flow::Priority;
    }

    /// CR 514.2 onward: remove damage, end "until end of turn" effects, then
    /// the 514.3a check.
    /// CR 402.2, 514.1: the hand size `p` discards down to, after static
    /// abilities that change it (`usize::MAX` for none).
    fn max_hand_size(&self, p: PlayerId) -> usize {
        let mut n = i64::try_from(self.player(p).max_hand_size).unwrap_or(7);
        for id in &self.battlefield {
            let o = &self.objects[*id];
            for a in &self.pool.get(o.def).ability_ids {
                if let crate::card::AbilityDef::HandSize { whose, delta, set } =
                    self.pool.ability(*a)
                {
                    let applies = match whose {
                        crate::card::Who::Any => true,
                        crate::card::Who::You => o.controller == p,
                        crate::card::Who::Opponents => o.controller != p,
                    };
                    if applies {
                        match delta {
                            None => return usize::MAX,
                            // "Your maximum hand size is eight."
                            Some(d) if *set => n = i64::from(*d),
                            Some(d) => n += i64::from(*d),
                        }
                    }
                }
            }
        }
        usize::try_from(n.max(0)).unwrap_or(0)
    }

    /// CR 724.1d: after "End the turn." the current step ends and the game
    /// skips straight to the cleanup step, creatures leaving combat; the
    /// end step (and its triggers, CR 724.1e) is skipped.
    fn skip_to_cleanup(&mut self) {
        for p in &mut self.players {
            p.mana_pool.empty();
            p.combat_mana = 0;
            p.turn_mana.empty();
        }
        self.restricted_pool.clear();
        if self.combat.take().is_some() {
            self.combat_bump();
        }
        self.extra_phases.clear();
        self.resume_step = None;
        self.cleanup_repeat = false;
        self.step = Step::Cleanup;
        self.flow = Flow::BeginStep;
    }

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
        self.player_rules.clear();
        if !self.temp_copies.is_empty() {
            self.end_temp_copies(None);
        }
        if !self.player_grants.is_empty() {
            self.player_grants
                .retain(|g| g.duration != Duration::UntilEndOfTurn);
        }
        self.granted_flashback.clear();
        self.granted_harmonize.clear();
        // CR 603.7c: "when that creature dies this turn" ends unfired.
        if !self.delayed_triggers.is_empty() {
            self.delayed_triggers.retain(|d| !d.this_turn);
        }
        for affected in ended {
            self.emit(Event::EffectEnded { affected });
        }
        self.cbump();
        // Control effects that ended: permanents go back to their owners
        // (or to whatever effect still applies).
        let control_left = u32::try_from(
            self.effects
                .iter()
                .filter(|e| e.parts.contains(&crate::card::Modification::Control))
                .count(),
        )
        .unwrap_or(u32::MAX);
        if control_left != self.control_effects {
            self.control_effects = control_left;
            self.control_dirty = true;
        }
        self.sync_control();
        let acted = self.sba_loop();
        if acted || !self.pending_triggers.is_empty() {
            // CR 514.3a: players get priority, then another cleanup step.
            self.cleanup_repeat = true;
            self.give_priority(self.active_player);
        } else {
            self.flow = Flow::EndStep;
        }
    }

    /// "If it attacked during your last turn": the turn ending is now the
    /// active player's last turn.
    fn record_attacked_last_turn(&mut self) {
        let ending = self.active_player;
        self.attacked_last_turn.retain(|(p, _)| *p != ending);
        self.attacked_last_turn
            .extend(self.attackers_this_turn.drain(..).map(|id| (ending, id)));
        if !self.battle_attackers_this_turn.is_empty() {
            self.battle_attackers_this_turn.clear();
            self.cbump();
        }
    }

    /// The step after the current one: the next in the turn's order,
    /// except that additional phases (CR 500.8) begin as the phase they
    /// were added after ends, and the turn goes on after them with what
    /// would have followed it. `None` after cleanup.
    fn next_step(&self) -> Option<Step> {
        let phase_ends = matches!(
            self.step,
            Step::Draw | Step::PrecombatMain | Step::EndOfCombat | Step::PostcombatMain
        );
        if phase_ends && (!self.extra_phases.is_empty() || self.resume_step.is_some()) {
            return self.extra_phases.first().copied().or(self.resume_step);
        }
        self.step.next()
    }

    /// Takes the next additional phase off the queue as it begins (or the
    /// step the turn resumes with after the last).
    fn take_extra_phase(&mut self, next: Step) {
        let phase_ends = matches!(
            self.step,
            Step::Draw | Step::PrecombatMain | Step::EndOfCombat | Step::PostcombatMain
        );
        if !phase_ends {
            return;
        }
        if self.extra_phases.first() == Some(&next) {
            self.extra_phases.remove(0);
            if self.resume_step.is_none() {
                self.resume_step = self.step.next();
            }
        } else if self.resume_step == Some(next) {
            self.resume_step = None;
        }
    }

    /// The player whose extra turn comes next, if one is in the game.
    fn next_extra_turn(&mut self) -> Option<PlayerId> {
        while let Some(p) = self.extra_turns.pop() {
            if self.player(p).lost.is_none() {
                return Some(p);
            }
        }
        None
    }

    /// The current step ends (CR 500.2): mana empties, combat clears, and the
    /// next step or turn begins.
    #[allow(clippy::too_many_lines)] // one line per turn record
    fn end_step(&mut self) {
        // CR 702.189a: firebending's mana lasts until end of combat.
        let combat = matches!(
            self.step,
            Step::BeginningOfCombat
                | Step::DeclareAttackers
                | Step::DeclareBlockers
                | Step::FirstStrikeDamage
                | Step::CombatDamage
        );
        let cleanup = self.step == Step::Cleanup;
        for p in &mut self.players {
            let red = crate::mana::Color::Red as usize;
            let keep = if combat {
                p.combat_mana.min(p.mana_pool.colored[red])
            } else {
                0
            };
            // "Until end of turn, you don't lose this mana": what is left of
            // it, through the cleanup step (CR 500.4).
            let mut kept = crate::mana::ManaPool::default();
            if !cleanup && !p.turn_mana.is_empty() {
                for c in 0..5 {
                    let left = if c == red {
                        p.mana_pool.colored[c] - keep
                    } else {
                        p.mana_pool.colored[c]
                    };
                    kept.colored[c] = p.turn_mana.colored[c].min(left);
                }
                kept.colorless = p.turn_mana.colorless.min(p.mana_pool.colorless);
            }
            p.mana_pool = kept;
            p.turn_mana = kept;
            p.combat_mana = keep;
            p.mana_pool.colored[red] += keep;
        }
        // Restricted mana empties too (CR 500.4).
        self.restricted_pool.clear();
        if self.step == Step::EndOfCombat && self.combat.take().is_some() {
            // Filters on attacking and blocking creatures see the change.
            self.combat_bump();
        }
        if self.step == Step::Cleanup && self.cleanup_repeat {
            self.cleanup_repeat = false;
            self.flow = Flow::BeginStep;
            return;
        }
        if let Some(mut next) = self.next_step() {
            self.take_extra_phase(next);
            // CR 500.11: a skipped step or phase doesn't happen; the game
            // proceeds to the one after it.
            let ap = self.active_player.index();
            if next == Step::Draw && self.players[ap].skip_draw > 0 {
                self.players[ap].skip_draw -= 1;
                next = Step::PrecombatMain;
            } else if next == Step::BeginningOfCombat
                && (self.players[ap].skip_combat > 0 || self.players[ap].skip_combat_this_turn > 0)
            {
                let p = &mut self.players[ap];
                if p.skip_combat_this_turn > 0 {
                    p.skip_combat_this_turn -= 1;
                } else {
                    p.skip_combat -= 1;
                }
                next = Step::PostcombatMain;
            }
            self.step = next;
            // "Until your next end step": the permission ends as it begins.
            if next == Step::End
                && let Some(e) = &mut self.exile_casts
                && !e.playable.is_empty()
            {
                let active = self.active_player;
                e.playable.retain(|pe| {
                    pe.until != crate::card::PlayUntil::YourNextEndStep || pe.player != active
                });
            }
            // A static ability conditioned on a part of the turn (CR
            // 611.3a) changes with the step.
            if self.turn_static_sources > 0
                && self
                    .battlefield
                    .iter()
                    .any(|id| self.pool.get(self.objects[*id].def).step_static)
            {
                self.cbump();
            }
        } else {
            self.lki.clear();
            // Costs of spells and abilities that never resolved.
            self.paid_costs.clear();
            if !self.spent_mana.is_empty() {
                let objects = &self.objects;
                self.spent_mana
                    .retain(|(id, _)| objects.get(*id).is_some_and(|o| o.zone == Zone::Stack));
            }
            self.loyalty_activated.clear();
            self.activated_once.clear();
            self.limited_this_turn.clear();
            self.resolved_this_turn.clear();
            self.modes_chosen.retain(|e| !e.3);
            self.record_attacked_last_turn();
            self.blockers_this_turn.clear();
            self.targeted_by_controller.clear();
            self.dealt_damage_this_turn.clear();
            self.entered_this_turn.clear();
            self.damaged_by_this_turn.clear();
            self.saddled.clear();
            self.turn_started_at = self.next_timestamp;
            self.turn_number += 1;
            self.extra_phases.clear();
            self.resume_step = None;
            self.combats_this_turn = 0;
            for p in &mut self.players {
                p.skip_combat_this_turn = 0;
            }
            self.spells_last_turn = std::mem::take(&mut self.spells_this_turn);
            self.next_spell_less = [0; 4];
            self.drawn_this_turn = [0; 4];
            // "As long as you've drawn two or more cards this turn" and
            // the other this-turn statics start over (CR 611.3a).
            if self.turn_static_sources > 0 {
                self.cbump();
            }
            self.mana_spent_this_turn = [0; 4];
            self.mana_spent_before_cast = [0; 4];
            self.life_lost_this_turn = [0; 4];
            self.attacked_this_turn = 0;
            self.assassin_damage = 0;
            self.prowl_damage.clear();
            if !self.enter_notes.is_empty() {
                let objects = &self.objects;
                self.enter_notes.retain(|(o, _)| {
                    objects
                        .get(*o)
                        .is_some_and(|o| o.zone == crate::state::Zone::Battlefield)
                });
            }
            self.lands_entered_this_turn = 0;
            self.block_demands.clear();
            self.block_bans.clear();
            self.no_regen.clear();
            self.no_combat_damage.clear();
            self.artifacts_entered_this_turn = 0;
            self.creatures_died_this_turn = 0;
            self.died_this_turn.clear();
            self.damaged_this_turn = 0;
            self.lost_life_this_turn = 0;
            self.left_this_turn = 0;
            self.discarded_this_turn = 0;
            self.mayhem_discarded.clear();
            self.cast_this_turn.clear();
            self.turn_log = crate::state::TurnLog::default();
            if let Some(e) = &mut self.exile_casts {
                e.foretold_this_turn.clear();
                if !e.playable.is_empty() {
                    // Impulse permissions end with the turn ("this turn") or
                    // with the player's next turn.
                    let (objects, active) = (&self.objects, self.active_player);
                    e.playable.retain_mut(|pe| {
                        if !objects.get(pe.card).is_some_and(|o| o.zone == Zone::Exile) {
                            return false;
                        }
                        match pe.until {
                            crate::card::PlayUntil::ThisTurn => false,
                            crate::card::PlayUntil::EndOfYourNextTurn => {
                                if pe.player == active {
                                    pe.own_ends = pe.own_ends.saturating_sub(1);
                                }
                                pe.own_ends > 0
                            }
                            // CR 702.185a: a warped card, castable from
                            // the next turn on.
                            crate::card::PlayUntil::Forever => {
                                if pe.own_ends == crate::state::WARP_LATER {
                                    pe.own_ends = 0;
                                }
                                true
                            }
                            crate::card::PlayUntil::YourNextEndStep => true,
                            // Ends when its source leaves or changes
                            // control (CR 400.7).
                            crate::card::PlayUntil::WhileYouControlSource => {
                                objects.get(pe.source).is_some_and(|s| {
                                    s.zone == Zone::Battlefield && s.controller == pe.player
                                })
                            }
                        }
                    });
                }
            }
            self.eot_grants = self.lasting_grants;
            self.desig.last_active = Some(self.active_player);
            self.desig.sped = 0;
            // CR 500.7: an extra turn comes directly after this one, the
            // most recently created first.
            // The turn order then goes on from the player whose regular
            // turn this would have been.
            self.active_player = match self.next_extra_turn() {
                Some(p) => {
                    if self.resume_player.is_none() {
                        self.resume_player = Some(self.next_player_in_game(self.active_player));
                    }
                    p
                }
                None => match self.resume_player.take() {
                    Some(p) if self.player(p).lost.is_none() => p,
                    Some(p) => self.next_player_in_game(p),
                    None => self.next_player_in_game(self.active_player),
                },
            };
            // CR 611.2b: "until your next turn" ends as that turn begins.
            let ends = Duration::UntilYourNextTurn(self.active_player);
            if self.effects.iter().any(|e| e.duration == ends) {
                self.effects.retain(|e| e.duration != ends);
            }
            if self.replacements.iter().any(|e| e.duration == ends) {
                self.replacements.retain(|e| e.duration != ends);
            }
            if self.rules.iter().any(|e| e.duration == ends) {
                self.rules.retain(|e| e.duration != ends);
            }
            if !self.player_grants.is_empty() {
                self.player_grants.retain(|g| g.duration != ends);
            }
            if !self.goads.is_empty() {
                let p = self.active_player;
                self.goads.retain(|(_, by)| *by != p);
            }
            if !self.temp_copies.is_empty() {
                self.end_temp_copies(Some(self.active_player));
            }
            // "During your turn" static abilities (CR 611.3a) change with
            // it; nothing a state-based action looks at does.
            self.version += 1;
            self.cversion += 1;
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
    use crate::card::test_pool;
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
        let pool = Arc::new(test_pool());
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
        let pool = Arc::new(test_pool());
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
        let pool = Arc::new(test_pool());
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
        let pool = Arc::new(test_pool());
        let mut e = Engine::new(pool.clone(), &config(&pool, 1));
        let n = e.current().decision().unwrap().options.len();
        assert!(matches!(
            e.step(u32::try_from(n).unwrap()),
            Err(EngineError::BadChoice { .. })
        ));
    }

    #[test]
    fn divisions_give_each_target_at_least_one() {
        // CR 601.2d.
        assert_eq!(casting::divisions(3, 2), vec![vec![1, 2], vec![2, 1]]);
        assert_eq!(casting::divisions(3, 3), vec![vec![1, 1, 1]]);
        assert!(casting::divisions(2, 3).is_empty());
        assert_eq!(casting::divisions(5, 2).len(), 4);
        // Past the option cap: one heavy target each, and the even split.
        let many = casting::divisions(30, 6);
        assert_eq!(many.len(), 7);
        assert!(
            many.iter()
                .all(|d| d.iter().sum::<u32>() == 30 && d.iter().all(|n| *n >= 1))
        );
        assert!(many.contains(&vec![5; 6]));
    }
}
