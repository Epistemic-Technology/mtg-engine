//! Baseline agents (design §1, §4.8): a uniformly random agent, used for
//! fuzzing and as the performance baseline, and a one-ply greedy agent.

mod greedy;

pub use greedy::GreedyAgent;

use mtg_core::decision::{Choice, ChoiceId, Decision};
use mtg_core::state::GameState;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub trait Agent {
    fn choose(&mut self, state: &GameState, decision: &Decision) -> ChoiceId;
}

/// Picks uniformly among the legal options, except that it keeps opening
/// hands (so games are about play, not mulligan noise).
pub struct RandomAgent {
    rng: ChaCha8Rng,
    pub keep_hands: bool,
}

impl RandomAgent {
    pub fn new(seed: u64) -> Self {
        RandomAgent {
            rng: ChaCha8Rng::seed_from_u64(seed),
            keep_hands: true,
        }
    }
}

impl Agent for RandomAgent {
    fn choose(&mut self, _state: &GameState, decision: &Decision) -> ChoiceId {
        if self.keep_hands
            && let Some(i) = decision.options.iter().position(|c| *c == Choice::Keep)
        {
            return u32::try_from(i).unwrap_or(0);
        }
        let n = decision.options.len();
        u32::try_from(self.rng.random_range(0..n)).unwrap_or(0)
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

/// Construct a baseline agent by name: `random` or `greedy`.
pub fn by_name(name: &str, seed: u64) -> Option<Box<dyn Agent>> {
    Some(match name {
        "random" => Box::new(RandomAgent::new(seed)),
        "greedy" => Box::new(GreedyAgent),
        "first" => Box::new(FirstOptionAgent),
        _ => return None,
    })
}
