//! Single-observer information-set Monte Carlo tree search (Cowling,
//! Powley, and Whitehouse 2012), the M5 search baseline (design §1, §4.8).
//!
//! Each iteration samples the hidden information the searching player
//! can't see (`Engine::determinize`: the opponent's hand and both
//! libraries), then descends a tree of the searcher's own decisions by
//! UCB1, counting a child as available only when its option is legal in
//! this sample. The opponent's decisions, and the searcher's minor ones,
//! follow the greedy policy. A new child is expanded once per iteration,
//! and the game is played out by greedy agents for both seats up to a
//! decision cap, then scored: a win 1, a loss 0, otherwise a life-and-board
//! estimate. The most visited root option is chosen.

use mtg_core::decision::{Choice, ChoiceId, Decision, DecisionKind, StepResult};
use mtg_core::engine::Engine;
use mtg_core::ids::PlayerId;
use mtg_core::state::{GameResult, GameState};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::{Agent, GreedyAgent};

struct Node {
    visits: u32,
    /// Sum of results from the searcher's point of view.
    value: f64,
    /// Times a child was legal when this node was reached (the
    /// availability count of IS-MCTS's UCB).
    children: Vec<Child>,
}

struct Child {
    choice: Choice,
    node: usize,
    available: u32,
}

pub struct MctsAgent {
    rng: ChaCha8Rng,
    iterations: u32,
    /// Decisions a playout may take before it is scored by estimate.
    pub playout_cap: u32,
    /// UCB1 exploration constant.
    pub exploration: f64,
    fallback: GreedyAgent,
    guard: crate::ActivationGuard,
}

impl MctsAgent {
    pub const DEFAULT_ITERATIONS: u32 = 64;

    pub fn new(seed: u64, iterations: u32) -> Self {
        MctsAgent {
            rng: ChaCha8Rng::seed_from_u64(seed ^ 0x5eed_4d43),
            iterations: iterations.max(1),
            playout_cap: 400,
            exploration: 0.7,
            fallback: GreedyAgent::default(),
            guard: crate::ActivationGuard::default(),
        }
    }

    /// Decisions worth a search; the rest follow the greedy policy.
    fn searched(kind: DecisionKind) -> bool {
        matches!(
            kind,
            DecisionKind::PriorityAction
                | DecisionKind::DeclareAttackers
                | DecisionKind::DeclareBlockers
                | DecisionKind::ChooseTarget { .. }
                | DecisionKind::ChooseMode { .. }
        )
    }

    fn search(&mut self, engine: &Engine, d: &Decision) -> ChoiceId {
        let me = d.player;
        let mut nodes = vec![Node {
            visits: 0,
            value: 0.0,
            children: Vec::new(),
        }];
        for _ in 0..self.iterations {
            let mut sim = engine.clone();
            sim.determinize(me, self.rng.random());
            let mut agents = [GreedyAgent::default(), GreedyAgent::default()];
            let mut path = vec![0usize];
            let mut node = 0usize;
            let mut expanded = false;
            let mut steps = 0u32;
            let result = loop {
                let decision = match sim.current() {
                    StepResult::Over(r) => break Some(*r),
                    StepResult::Decision(x) => x.clone(),
                };
                if steps >= self.playout_cap {
                    break None;
                }
                steps += 1;
                let mine = decision.player == me
                    && Self::searched(decision.kind)
                    && decision.options.len() > 1;
                let choice = if mine && !expanded {
                    let (c, next, new) = self.select(&mut nodes, node, &decision);
                    node = next;
                    path.push(node);
                    expanded = new;
                    c
                } else {
                    agents[decision.player.index()].choose_in(&sim, &decision)
                };
                if sim.step(choice).is_err() {
                    break None;
                }
            };
            let v = match result {
                Some(r) => outcome_value(r, me),
                None => estimate(sim.state(), me),
            };
            for n in path {
                nodes[n].visits += 1;
                nodes[n].value += v;
            }
        }
        let root = &nodes[0];
        let best = root
            .children
            .iter()
            .max_by_key(|c| nodes[c.node].visits)
            .map(|c| c.choice.clone());
        best.and_then(|c| d.options.iter().position(|o| *o == c))
            .map_or_else(
                || self.fallback.choose_in(engine, d),
                |i| u32::try_from(i).unwrap_or(0),
            )
    }

    /// Pick an option at `node` for this sample: an untried legal option
    /// first (expanding it), otherwise UCB1 over the legal children with
    /// availability counts. Returns the option, the child node, and
    /// whether it was just expanded.
    fn select(
        &mut self,
        nodes: &mut Vec<Node>,
        node: usize,
        d: &Decision,
    ) -> (ChoiceId, usize, bool) {
        let legal: Vec<usize> = (0..d.options.len()).collect();
        for c in &mut nodes[node].children {
            if d.options.contains(&c.choice) {
                c.available += 1;
            }
        }
        let untried: Vec<usize> = legal
            .iter()
            .copied()
            .filter(|i| {
                !nodes[node]
                    .children
                    .iter()
                    .any(|c| c.choice == d.options[*i])
            })
            .collect();
        if !untried.is_empty() {
            let i = untried[self.rng.random_range(0..untried.len())];
            let child = nodes.len();
            nodes.push(Node {
                visits: 0,
                value: 0.0,
                children: Vec::new(),
            });
            nodes[node].children.push(Child {
                choice: d.options[i].clone(),
                node: child,
                available: 1,
            });
            return (u32::try_from(i).unwrap_or(0), child, true);
        }
        let mut best = (0usize, 0usize, f64::NEG_INFINITY);
        for (i, opt) in d.options.iter().enumerate() {
            let Some(c) = nodes[node].children.iter().find(|c| c.choice == *opt) else {
                continue;
            };
            let n = &nodes[c.node];
            let visits = f64::from(n.visits.max(1));
            let ucb = n.value / visits
                + self.exploration * (f64::from(c.available.max(1)).ln() / visits).sqrt();
            if ucb > best.2 {
                best = (i, c.node, ucb);
            }
        }
        (u32::try_from(best.0).unwrap_or(0), best.1, false)
    }
}

impl Agent for MctsAgent {
    fn choose(&mut self, state: &GameState, d: &Decision) -> ChoiceId {
        self.fallback.choose(state, d)
    }

    fn choose_in(&mut self, engine: &Engine, d: &Decision) -> ChoiceId {
        if d.options.len() <= 1 || !Self::searched(d.kind) || engine.mid_decision() {
            return self.fallback.choose_in(engine, d);
        }
        let c = self.search(engine, d);
        // The baseline agents' guard against repeating a free ability
        // without end (see `ActivationGuard`).
        let chosen = &d.options[c as usize];
        if !self.guard.allows(engine.state(), chosen) {
            return self.fallback.choose_in(engine, d);
        }
        self.guard.note(chosen);
        c
    }
}

fn outcome_value(r: GameResult, me: PlayerId) -> f64 {
    match r {
        GameResult::Win { winner } if winner == me => 1.0,
        GameResult::Win { .. } => 0.0,
        GameResult::Draw => 0.5,
    }
}

/// A playout cut short: life totals and the creatures on board, squashed
/// into (0, 1).
#[allow(clippy::cast_precision_loss)]
fn estimate(state: &GameState, me: PlayerId) -> f64 {
    let opp = PlayerId(1 - me.0);
    let board = |p: PlayerId| -> f64 {
        state
            .permanents_of(p)
            .filter_map(|id| state.characteristics(id))
            .filter(mtg_core::state::Characteristics::is_creature)
            .map(|c| f64::from(c.power().max(0)) + 0.5 * f64::from(c.toughness().max(0)))
            .sum()
    };
    let hand = |p: PlayerId| state.player(p).hand.len() as f64;
    let score = f64::from(state.player(me).life - state.player(opp).life) * 0.15
        + (board(me) - board(opp)) * 0.1
        + (hand(me) - hand(opp)) * 0.1;
    1.0 / (1.0 + (-score).exp())
}
