# Benchmark harness

`mtg-bench` measures how well a language model plays Magic on this engine
(design.md §9.5). It is the harness the public benchmark site (M7) will run;
everything here works from a checkout.

## Tracks

**Rated games.** A model plays the built-in agents (`random`, `greedy`, and
`mcts`, the IS-MCTS agent) over a fixed set of deck matchups. Games come in
mirrored pairs: the same seed and the same decks in the same seats, played
twice with the controllers swapped, so shuffles, deck strength, and seat
cancel within the pair.

**Ladder.** The agents play each other the same way. These games are free
and anchor the ratings.

**Puzzles.** Win-this-turn positions written as scenarios in
`scenarios/puzzles/`. Each file includes the scripted solution, which the
scenario suite (`mtg scenario`) checks, so every puzzle is known to be
solvable. The harness gives the player seat A at the starting position
without the script, and counts the puzzle solved if A wins before the turn
ends. The opponent is the greedy agent.

## What the model sees and answers

One request per decision: the game from the model's seat as text (the same
rendering `mtg-mcp` serves, with `#n` labels), the events since its last
decision, and the numbered legal options. The model ends its reply with
`ANSWER: <number>`. A reply that names no legal option is counted as
invalid and answered with a correction, up to two retries; after that the
greedy agent chooses and the decision is recorded as a fallback. Requests
are stateless, so a long game does not grow the context, and the system
prompt is cached.

**Auto-pass policy**, fixed for every model: a decision with a single
option is taken without asking, and opening hands are kept. Everything else
goes to the model. `--max-model-decisions` caps the model decisions per
game (a cost limit); the greedy agent plays the seat after the cap, and the
fallbacks are counted.

Every model decision's reply text, input, output, and cache-read tokens,
latency, and invalid-reply count are stored in the game's `GameRecord`
(`notes`), written with `--records DIR`, and the records replay in
`mtg-review`.

## Ratings

`mtg-bench report` fits Bradley–Terry ratings on the Elo scale over every
rated and ladder game in the given files. The greedy agent is fixed at 1500:
Magic has no external engine to calibrate against, so the built-in agents
are the anchors. Each player also gets one virtual draw against the anchor,
a weak prior that keeps a rating finite after a few decisive games.
Intervals are 95% from the fit's information. The report
also shows score, invalid-reply rate, output tokens per game, seconds per
decision, puzzles solved, and cost per game when `--price-in` and
`--price-out` (USD per million tokens) are given. The harness stores no
prices of its own. Every line records the engine version and corpus size it
was played on.

## Running

```
cargo build --release -p mtg-bench
B=target/release/mtg-bench

# Anchor games (free): every agent pair, each matchup, 10 mirrored pairs.
$B ladder --pairs 10 --out results.jsonl

# A model (needs ANTHROPIC_API_KEY); costs real money.
$B rated --model claude-haiku-5-5 --opponents greedy,mcts --pairs 2 \
    --out results.jsonl --records records/
$B puzzles --model claude-haiku-5-5 --out results.jsonl

# Check the puzzle track with an agent instead of a model.
$B puzzles --agent greedy --out results.jsonl

$B report results.jsonl --price-in <usd> --price-out <usd>
```

Defaults: matchups `mono-red:mono-green,mono-white:mono-black,
mono-blue:mono-red,mono-green:mono-white` (the built-in decks of
`mtg-sim`), model `claude-haiku-5-5`, 4,000 output tokens per reply, and
300 model decisions per game.

## First results (2026-11-18)

Ladder, 25 mirrored pairs per agent pair over the four default matchups
(1,200 games): greedy 1500, `mcts` 1568 ± 41, `random` 1218 ± 49. Puzzles:
greedy 3 of 4, `mcts` 2 of 4, `random` 1 of 4. A first live check, one
mirrored pair with `claude-haiku-5-5` against greedy on mono-red against
mono-green: Haiku won both games, with 87 model decisions, no invalid
replies, about 30,000 input and 18,500 output tokens per game, and 2.5 s per
decision. Two games rate it 1780 ± 527; the interval is the point.

## Not yet

These come with the site (M7): decks drawn only from scenario-tested cards
beyond the built-in ones, model-against-model games, more puzzles, a
leaderboard page, and public replays.
