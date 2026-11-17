# Learning agent: plan

Status: proposal, 2026-11-17. Nothing here is built.

## 1. Goal

An agent that improves from its own games. It estimates the probability of
winning from a state, looks a short way ahead with the engine, and samples
its move in proportion to how good each option looks. Every legal option
keeps a non-zero chance, so the agent keeps exploring.

The design has three parts that can be developed and swapped separately:

1. **Features**: a fixed-length vector describing a state from one
   player's point of view.
2. **Value function**: features in, win probability out. Two
   implementations behind one trait: a nearest-neighbour memory of past
   states, and a fitted model.
3. **Search**: apply each legal option on a copy of the game, optionally
   play a few turns forward, and score where it lands with the value
   function.

Relation to `design.md`: §1 lists a competitive AI as a non-goal. This
agent is a stronger rung on the baseline ladder (random, greedy, this,
IS-MCTS) and a consumer of the search support in §4.8, not an attempt at
strong play. If it grows past that, the non-goal needs revisiting.

## 2. What exists and what is missing

Exists:

- `Agent::choose(&GameState, &Decision) -> ChoiceId` (`mtg-agents`), with
  random and greedy agents.
- `mtg-sim::play` and `batch` for running games, and `GameRecord` (seed,
  decks, action list) with `replay`, so any past game can be re-simulated
  to recover every intermediate state.
- `GameState::observe(viewer) -> GameView`, the hidden-information view.
- Throughput with random agents: about 945 games/s/core on random decks
  (840 decisions per game) and about 4,900 on mono-red (§8.1).

Missing, and needed by this plan:

- **A way to look ahead.** `Engine` is not `Clone`, and agents receive a
  `&GameState`, not an engine they can step. Phase 2 needs either
  `Engine: Clone` plus access to it from the agent, or
  `Engine::from_state`.
- **Determinization.** `Engine::determinize` (§4.8) is designed but not
  implemented. Without it, any lookahead sees the opponent's real hand
  and the real library order.
- **Agents see everything.** `choose` is handed the full `GameState`. The
  feature extractor must read only what `observe` exposes for the acting
  player, or the value function learns from information it should not have.
- **No dataset writer.** Nothing logs per-state features yet.

## 3. Features

Version 1 is hand-built, a few dozen numbers, all from the acting
player's view and expressed as mine/theirs:

- Life totals; cards in hand; cards in library; cards in graveyard.
- Lands on the battlefield; untapped lands; mana available by colour.
- Creatures: count, total power, total toughness, untapped count, count
  with evasion (flying, menace, unblockable), count with deathtouch,
  first strike, lifelink.
- Non-creature permanents by type.
- Own hand: count by type, castable count, total mana value.
- Turn number, phase and step, whose turn, whether the stack is empty.
- Derived: damage I can present next attack against their untapped
  blockers, and the reverse (a crude clock).

Each feature is scaled to a comparable range, since both nearest-neighbour
distance and linear models are sensitive to scale. The feature list is
versioned; a dataset records the version it was written with.

Later versions, only if version 1 plateaus: per-zone card-count vectors;
then per-object vectors derived from each card's DSL definition (cost,
types, keywords, effect kinds), pooled per zone, so the model generalizes
to cards it has not seen.

## 4. Value function

```rust
pub trait Evaluator {
    /// Probability that `player` wins from this state, in [0, 1].
    fn value(&self, view: &GameView, player: PlayerId) -> f32;
}
```

Terminal states are scored 1, 0, or 0.5 (draw) by the caller, not the
evaluator.

**Memory (nearest neighbour).** Stores (features, outcome) rows. A query
takes the k nearest rows, weights them by distance, and adds a prior of
one imaginary win and one imaginary loss. With little data the estimate
sits near 0.5 and moves as evidence accumulates. Brute-force search is
fine to roughly a million rows; an approximate index comes after that.

**Fitted model.** Logistic regression first, then boosted trees, trained
offline in Python on the same rows and exported as weights. Inference in
Rust is a dot product (or a tree walk). A small neural network over the
same features is the next step up and not planned yet.

**Blend.** The model's output can replace the fixed prior in the memory
estimate, so the memory acts as a local correction. Optional.

## 5. Search and move selection

For each legal option: copy the game, apply the option, and score the
resulting position. Three depths, in order of cost:

1. **One step.** Score the state at the next decision. An option that
   wins or loses outright is scored 1 or 0 here with no special rule.
2. **Rollout.** From that state, play forward with the greedy agent on
   both sides to the end of the opponent's next turn (or game end), then
   score. Repeat over several determinizations and average. This is what
   catches a lethal counter-attack.
3. **IS-MCTS.** Already planned for M5; out of scope here, but it would
   use the same `Evaluator`.

Selection: softmax over the scores with a temperature, plus a small floor
so no option has zero probability. Temperature is high early in training
and lowered as the value function improves. Evaluation games use a low
temperature.

Not every decision deserves a search. Mana payment, trigger ordering and
similar low-stakes decisions fall through to the greedy agent's rule at
first; search covers priority actions, targets, attackers and blockers.

## 6. Training loop

1. **Generate.** Run games with the current agent against a pool of
   opponents (random, greedy, earlier versions of itself). Save
   `GameRecord`s.
2. **Extract.** Replay each record, sample 8 to 16 decision points per
   game, and write one row per sample: feature version, features, acting
   player, final outcome for that player.
3. **Fit.** Load rows into the memory, or train a model. Hold out whole
   games, never individual states.
4. **Evaluate.** Win rate over mirrored pairs (same decks and seed, seats
   swapped) against greedy and against the previous version. Also report
   log loss and calibration on the held-out games.
5. **Repeat.**

Deck strength confounds the outcome label when decks are random. Mirrored
pairs handle this in evaluation. In training, start with a small set of
fixed matchups, then widen to random decks once the loop works.

Later: train each state toward the value of the following state
(temporal-difference learning) instead of the final outcome, which reduces
label noise.

## 7. Phases

Each phase has an acceptance test, as in the main roadmap.

**Phase 0: measure.**
- Greedy vs greedy games/s/core on mono-red and on random decks; cost of
  cloning a mid-game state today (the 2.8 µs figure in §12 is from M1).
- Accept: both numbers recorded in this file.

**Phase 1: features and dataset.**
- Feature extractor over `GameView`; extractor that replays records and
  writes rows; a `mtg-sim` command to generate a dataset.
- Accept: 10,000 greedy-vs-greedy games produce a dataset; a logistic
  regression trained on it beats a constant 0.5 predictor on held-out
  log loss.

**Phase 2: lookahead.**
- Make the engine copyable from an agent; implement `determinize`.
- Accept: a determinized copy passes the engine's debug invariants and
  never exposes a card the viewer could not know; clone-and-step cost is
  recorded.

**Phase 3: one-step agent with memory.**
- `Evaluator` trait, nearest-neighbour memory, softmax selection.
- Accept: win rate against greedy rises with the number of training
  games and passes 50% on a fixed mirrored matchup.

**Phase 4: rollouts.**
- Greedy rollouts to the end of the opponent's next turn.
- Accept: beats the phase 3 agent over mirrored pairs; decision time
  recorded.

**Phase 5: fitted model.**
- Python training script, weight export, Rust inference.
- Accept: matches or beats the memory evaluator at lower decision time.

**Phase 6: iterate.**
- Self-play against an opponent pool; random decks; richer features or
  temporal-difference targets as the measurements suggest.

## 8. Costs

Rough, to be replaced by phase 0 numbers.

- **Storage.** 100,000 games at 16 samples of 200 floats is about 1.3 GB
  uncompressed.
- **Training.** Logistic regression and boosted trees fit in seconds to
  minutes on CPU. No GPU is needed before a neural network.
- **Data generation.** The dominant cost. One-step search multiplies the
  work per decision by the number of options; rollouts multiply it again
  by rollout length and count. The phase 0 and phase 2 measurements
  decide how many rollouts per option are affordable.

## 9. Open questions

1. Where does the agent get a steppable game? Options: change the `Agent`
   trait to pass an engine handle, or build an engine from a `GameState`.
   The first is simpler; the second keeps the trait stable.
2. How should set decisions (attackers, blockers) be searched when the
   option list is large? Possibly score a sample of options plus the
   greedy agent's pick.
3. Should the value function see the decklist? It reduces deck-strength
   noise but ties the model to known decks.
4. How many mirrored pairs give a stable win rate? Shared with open
   question 11 in `design.md`.
5. Does the memory need pruning or ageing, given that old rows come from
   weaker versions of the agent?
