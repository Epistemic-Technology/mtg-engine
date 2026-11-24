//! Baseline agents (design §1, §4.8): a uniformly random agent, used for
//! fuzzing and as the performance baseline, and a one-ply greedy agent.

mod greedy;
mod mcts;

pub use greedy::GreedyAgent;
pub use mcts::MctsAgent;

use std::collections::HashMap;

use mtg_core::card::{AbilityDef, AbilityId};
use mtg_core::decision::{Choice, ChoiceId, Decision};
use mtg_core::ids::ObjId;
use mtg_core::state::GameState;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub trait Agent {
    fn choose(&mut self, state: &GameState, decision: &Decision) -> ChoiceId;

    /// Choose with the whole engine at hand, for agents that search by
    /// simulating ahead. Callers that have the engine call this.
    fn choose_in(&mut self, engine: &mtg_core::engine::Engine, decision: &Decision) -> ChoiceId {
        self.choose(engine.state(), decision)
    }
}

/// Keeps an agent from activating a free ability ("{0}: ...", a cost with
/// no mana, tap, sacrifice, discard, or life) more than a few times a
/// turn. The rules allow it without end, and a random agent's stack of
/// activations is a symmetric random walk whose excursions are unbounded;
/// every activation adds an until-end-of-turn effect, so a game could
/// spend minutes in one step. Not a rule: a policy for the baseline agents.
#[derive(Default)]
pub struct ActivationGuard {
    turn: u32,
    counts: HashMap<(ObjId, AbilityId), u32>,
}

impl ActivationGuard {
    pub const LIMIT: u32 = 4;
    /// An ability with a cost can still be repeated without end when
    /// something pays the cost back (a sacrifice outlet beside a token
    /// maker), so every ability has a ceiling.
    pub const PAID_LIMIT: u32 = 50;

    /// Whether the agent may still pick this option.
    pub fn allows(&mut self, state: &GameState, choice: &Choice) -> bool {
        if state.turn_number != self.turn {
            self.turn = state.turn_number;
            self.counts.clear();
        }
        match choice {
            Choice::Activate {
                source, ability, ..
            } => {
                let limit = if Self::is_free(state, *ability) {
                    Self::LIMIT
                } else {
                    Self::PAID_LIMIT
                };
                self.counts.get(&(*source, *ability)).copied().unwrap_or(0) < limit
            }
            _ => true,
        }
    }

    /// Record the option the agent picked.
    pub fn note(&mut self, choice: &Choice) {
        if let Choice::Activate {
            source, ability, ..
        } = choice
        {
            *self.counts.entry((*source, *ability)).or_insert(0) += 1;
        }
    }

    fn is_free(state: &GameState, ability: AbilityId) -> bool {
        match state.pool.ability(ability) {
            AbilityDef::Activated(a) => {
                a.cost.mana.requirements(0).total() == 0
                    && !a.cost.tap
                    && a.cost.sacrifice.is_none()
                    && a.cost.discard == 0
                    && a.cost.life == 0
                    && !a.is_loyalty()
            }
            _ => false,
        }
    }
}

/// Picks uniformly among the legal options, except that it keeps opening
/// hands (so games are about play, not mulligan noise) and stops repeating
/// a free ability (see [`ActivationGuard`]).
pub struct RandomAgent {
    rng: ChaCha8Rng,
    pub keep_hands: bool,
    guard: ActivationGuard,
}

impl RandomAgent {
    pub fn new(seed: u64) -> Self {
        RandomAgent {
            rng: ChaCha8Rng::seed_from_u64(seed),
            keep_hands: true,
            guard: ActivationGuard::default(),
        }
    }
}

impl Agent for RandomAgent {
    fn choose(&mut self, state: &GameState, decision: &Decision) -> ChoiceId {
        if self.keep_hands
            && let Some(i) = decision
                .options
                .iter()
                .position(|c| matches!(c, Choice::Keep))
        {
            return u32::try_from(i).unwrap_or(0);
        }
        let n = decision.options.len();
        // The guard only ever excludes activations; most decisions have
        // none, and the uniform draw must stay allocation-free.
        if !decision
            .options
            .iter()
            .any(|c| matches!(c, Choice::Activate { .. }))
        {
            return u32::try_from(self.rng.random_range(0..n)).unwrap_or(0);
        }
        let allowed = decision
            .options
            .iter()
            .filter(|c| self.guard.allows(state, c))
            .count();
        let i = if allowed == 0 {
            self.rng.random_range(0..n)
        } else {
            let k = self.rng.random_range(0..allowed);
            decision
                .options
                .iter()
                .enumerate()
                .filter(|(_, c)| self.guard.allows(state, c))
                .nth(k)
                .map_or(0, |(i, _)| i)
        };
        self.guard.note(&decision.options[i]);
        u32::try_from(i).unwrap_or(0)
    }
}

/// Always takes the first option. Deterministic and never attacks; useful as
/// a control.
pub struct FirstOptionAgent;

impl Agent for FirstOptionAgent {
    fn choose(&mut self, _state: &GameState, _decision: &Decision) -> ChoiceId {
        0
    }
}

/// Construct an agent by name: `random`, `greedy`, `first`, `mcts`, or
/// `mcts:N` (N iterations per searched decision).
pub fn by_name(name: &str, seed: u64) -> Option<Box<dyn Agent + Send>> {
    Some(match name {
        "random" => Box::new(RandomAgent::new(seed)),
        "greedy" => Box::new(GreedyAgent::default()),
        "first" => Box::new(FirstOptionAgent),
        "mcts" => Box::new(MctsAgent::new(seed, MctsAgent::DEFAULT_ITERATIONS)),
        n if n.starts_with("mcts:") => {
            let iters = n["mcts:".len()..].parse().ok()?;
            Box::new(MctsAgent::new(seed, iters))
        }
        _ => return None,
    })
}
