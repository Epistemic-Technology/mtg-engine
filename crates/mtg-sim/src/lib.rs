//! Batch simulation and fuzzing (design §7.2 item 6, §8).

pub mod decks;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mtg_agents::{Agent, RandomAgent};
use mtg_core::card::{CardPool, CardRef};
use mtg_core::decision::StepResult;
use mtg_core::engine::{Engine, GameConfig};
use mtg_core::record::GameRecord;
use mtg_core::state::GameResult;
use rayon::prelude::*;

/// Safety valve for agents that never end a game. Not a rule.
pub const MAX_DECISIONS: u32 = 50_000;

#[derive(Clone, Debug)]
pub enum Outcome {
    Finished(GameResult),
    /// Hit `MAX_DECISIONS`.
    Unfinished,
    /// The engine panicked; the message and the record up to that point.
    Panicked {
        message: String,
        record: GameRecord,
    },
}

#[derive(Clone, Debug)]
pub struct GameSummary {
    pub seed: u64,
    pub outcome: Outcome,
    pub turns: u32,
    pub decisions: u32,
    pub elapsed: Duration,
}

/// Play one game between two agents.
pub fn play(
    pool: &Arc<CardPool>,
    config: &GameConfig,
    agents: &mut [Box<dyn Agent>; 2],
) -> GameSummary {
    let start = Instant::now();
    let mut engine = Engine::new(pool.clone(), config);
    let mut decisions = 0u32;
    let result = catch_unwind(AssertUnwindSafe(|| {
        loop {
            match engine.current() {
                StepResult::Over(r) => return Outcome::Finished(*r),
                StepResult::Decision(d) => {
                    if decisions >= MAX_DECISIONS {
                        return Outcome::Unfinished;
                    }
                    let choice = agents[d.player.index()].choose(engine.state(), d);
                    engine.step(choice).expect("agent chose a legal option");
                    decisions += 1;
                }
            }
        }
    }));
    let outcome = match result {
        Ok(o) => o,
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "non-string panic".to_string());
            Outcome::Panicked {
                message,
                record: GameRecord::from_engine(&engine, pool, config),
            }
        }
    };
    GameSummary {
        seed: config.seed,
        outcome,
        turns: engine.state().turn_number,
        decisions,
        elapsed: start.elapsed(),
    }
}

#[derive(Clone, Debug, Default)]
pub struct BatchStats {
    pub games: usize,
    pub wins: [usize; 2],
    pub draws: usize,
    pub unfinished: usize,
    pub panics: Vec<(u64, String)>,
    pub total_turns: u64,
    pub total_decisions: u64,
    pub wall: Duration,
    pub cpu: Duration,
}

#[allow(clippy::cast_precision_loss)] // statistics over counts far below 2^52
impl BatchStats {
    pub fn games_per_second_wall(&self) -> f64 {
        self.games as f64 / self.wall.as_secs_f64().max(1e-9)
    }

    pub fn games_per_second_cpu(&self) -> f64 {
        self.games as f64 / self.cpu.as_secs_f64().max(1e-9)
    }

    pub fn mean_turns(&self) -> f64 {
        self.total_turns as f64 / (self.games.max(1)) as f64
    }

    pub fn mean_decisions(&self) -> f64 {
        self.total_decisions as f64 / (self.games.max(1)) as f64
    }
}

/// Play `games` random-agent games between two decks, in parallel, seeds
/// `base_seed..base_seed+games`.
pub fn random_batch(
    pool: &Arc<CardPool>,
    decks: &[Vec<CardRef>; 2],
    games: usize,
    base_seed: u64,
    logging: bool,
) -> BatchStats {
    batch(pool, decks, games, base_seed, logging, ["random", "random"])
}

/// Play `games` games between two named baseline agents (see
/// `mtg_agents::by_name`), in parallel, seeds `base_seed..base_seed+games`.
///
/// # Panics
/// If an agent name is unknown.
pub fn batch(
    pool: &Arc<CardPool>,
    decks: &[Vec<CardRef>; 2],
    games: usize,
    base_seed: u64,
    logging: bool,
    agent_names: [&str; 2],
) -> BatchStats {
    for n in agent_names {
        assert!(mtg_agents::by_name(n, 0).is_some(), "unknown agent {n}");
    }
    let start = Instant::now();
    let summaries: Vec<GameSummary> = (0..games)
        .into_par_iter()
        .map(|i| {
            let seed = base_seed + i as u64;
            let config = GameConfig {
                decks: decks.to_vec(),
                starting_life: 20,
                seed,
                logging,
            };
            let mut agents: [Box<dyn Agent>; 2] = [
                mtg_agents::by_name(agent_names[0], seed.wrapping_mul(31).wrapping_add(1))
                    .expect("checked above"),
                mtg_agents::by_name(agent_names[1], seed.wrapping_mul(31).wrapping_add(2))
                    .expect("checked above"),
            ];
            play(pool, &config, &mut agents)
        })
        .collect();
    let mut stats = BatchStats {
        games,
        wall: start.elapsed(),
        ..Default::default()
    };
    for s in summaries {
        stats.total_turns += u64::from(s.turns);
        stats.total_decisions += u64::from(s.decisions);
        stats.cpu += s.elapsed;
        match s.outcome {
            Outcome::Finished(GameResult::Win { winner }) => stats.wins[winner.index()] += 1,
            Outcome::Finished(GameResult::Draw) => stats.draws += 1,
            Outcome::Unfinished => stats.unfinished += 1,
            Outcome::Panicked { message, .. } => stats.panics.push((s.seed, message)),
        }
    }
    stats
}

/// Measure `GameState::clone` on a mid-game state: play `warmup` random
/// decisions, then clone `iterations` times. Returns mean time per clone.
pub fn clone_cost(
    pool: &Arc<CardPool>,
    decks: &[Vec<CardRef>; 2],
    seed: u64,
    warmup: u32,
    iterations: u32,
) -> Duration {
    let config = GameConfig {
        decks: decks.to_vec(),
        starting_life: 20,
        seed,
        logging: false,
    };
    let mut engine = Engine::new(pool.clone(), &config);
    let mut agent = RandomAgent::new(seed);
    for _ in 0..warmup {
        match engine.current() {
            StepResult::Over(_) => break,
            StepResult::Decision(d) => {
                let c = agent.choose(engine.state(), d);
                engine.step(c).expect("legal");
            }
        }
    }
    let state = engine.state();
    let start = Instant::now();
    let mut sink = 0usize;
    for _ in 0..iterations {
        let c = state.clone();
        sink = sink.wrapping_add(c.battlefield.len());
    }
    let elapsed = start.elapsed();
    std::hint::black_box(sink);
    elapsed / iterations.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::card::m1_pool;

    /// Fuzz: every built-in deck pair, a few dozen seeded random games each,
    /// must finish without panics. Release CI runs far more via `mtg sim`.
    #[test]
    fn random_games_finish_without_panics() {
        let pool = Arc::new(m1_pool());
        let games = if cfg!(debug_assertions) { 12 } else { 200 };
        for a in crate::decks::NAMES {
            for b in crate::decks::NAMES {
                let decks = [
                    crate::decks::by_name(&pool, a).unwrap(),
                    crate::decks::by_name(&pool, b).unwrap(),
                ];
                let stats = random_batch(&pool, &decks, games, 42, false);
                assert_eq!(stats.panics.len(), 0, "{a} vs {b}: {:?}", stats.panics);
                assert_eq!(stats.unfinished, 0, "{a} vs {b}");
                assert_eq!(stats.wins[0] + stats.wins[1] + stats.draws, games);
            }
        }
    }

    /// Decisions taken in the middle of an event (which of several
    /// replacement effects applies first, CR 616.1; which legend to keep,
    /// CR 704.5j) make the engine roll an action back and replay it. A
    /// record of such a game must re-simulate to the same state.
    #[test]
    fn games_with_mid_event_decisions_replay_identically() {
        use mtg_core::decision::{DecisionKind, StepResult};
        use mtg_core::engine::Engine;
        use mtg_core::ids::ObjId;
        use mtg_core::record::GameRecord;
        let pool = Arc::new(m1_pool());
        for (deck_name, kind) in [
            ("replacement", DecisionKind::ChooseReplacement),
            ("activated", DecisionKind::LegendRule),
            ("copy", DecisionKind::ChooseCopy),
            (
                "slice6",
                DecisionKind::ChooseAttackTarget {
                    attacker: ObjId::default(),
                },
            ),
        ] {
            let deck = crate::decks::by_name(&pool, deck_name).unwrap();
            let mut mid_decisions = 0;
            for seed in 0..12u64 {
                let config = GameConfig {
                    decks: vec![deck.clone(), deck.clone()],
                    starting_life: 20,
                    seed,
                    logging: true,
                };
                let mut agents = [RandomAgent::new(seed), RandomAgent::new(seed + 1)];
                let mut engine = Engine::new(pool.clone(), &config);
                while let StepResult::Decision(d) = engine.current() {
                    if std::mem::discriminant(&d.kind) == std::mem::discriminant(&kind) {
                        mid_decisions += 1;
                    }
                    let choice = agents[d.player.index()].choose(engine.state(), d);
                    engine.step(choice).unwrap();
                }
                let record = GameRecord::from_engine(&engine, &pool, &config);
                let again = record.replay(pool.clone(), true).unwrap();
                assert_eq!(
                    engine.state().version,
                    again.state().version,
                    "{deck_name} seed {seed}"
                );
                assert_eq!(
                    engine.state().log.len(),
                    again.state().log.len(),
                    "{deck_name} seed {seed}"
                );
                assert_eq!(
                    engine.state().result,
                    again.state().result,
                    "{deck_name} seed {seed}"
                );
                for (p, q) in engine.state().players.iter().zip(&again.state().players) {
                    assert_eq!(p.life, q.life, "{deck_name} seed {seed}");
                }
            }
            assert!(
                mid_decisions > 0,
                "no {deck_name} game asked a {kind:?} question"
            );
        }
    }

    /// The greedy agent must beat the random agent clearly; a regression here
    /// means a heuristic broke (or the engine started offering bad options).
    #[test]
    fn greedy_beats_random() {
        let pool = Arc::new(m1_pool());
        let games = if cfg!(debug_assertions) { 40 } else { 200 };
        let decks = [
            crate::decks::by_name(&pool, "mono-green").unwrap(),
            crate::decks::by_name(&pool, "mono-red").unwrap(),
        ];
        let stats = batch(&pool, &decks, games, 7, false, ["greedy", "random"]);
        assert_eq!(stats.panics.len(), 0, "{:?}", stats.panics);
        assert!(
            stats.wins[0] * 10 >= games * 8,
            "greedy won only {} of {games}",
            stats.wins[0]
        );
    }
}
