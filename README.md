# mtg-engine

A Magic: The Gathering rules engine, written in Rust, built so that AI
systems can play Magic: LLM agents through tool use over the Model Context
Protocol, and RL agents through an in-process Python environment. Its two
non-negotiable properties are rules fidelity (behaviour matches the
Comprehensive Rules) and speed (thousands of complete games per second per
core with cheap agents).

Status: milestones M0–M4 are complete (core loop, continuous effects and
the event pipeline, the card DSL and authoring pipeline, and the Python,
MCP, and review tools). M5 (coverage and hardening) is under way: cards
are text files under `cards/` that compile to the engine's definitions
and render back to Oracle text, checked against Scryfall by a gate; the
corpus holds 10,711 cards (81% of Pauper over every layout, 37% of
Modern), multi-face cards (adventures, omens, split and transforming
double-faced cards) included. Random agents play complete games at about
4,200 games per second per core on effect-free boards. A 4,900-scenario
rules corpus passes. An IS-MCTS agent (`--agent1 mcts`) beats the greedy
agent 62% to 38% over both seats, and `mtg-bench` (see
[BENCHMARK.md](BENCHMARK.md)) rates language models against the agents.
See [design.md](design.md) for the
architecture, roadmap, and open questions.

```
cargo run --release -p mtg-cli -- sim --deck1 mono-red --deck2 mono-green --games 1000
cargo run --release -p mtg-cli -- sim --agent1 greedy --agent2 random --games 1000
cargo run --release -p mtg-cli -- play --deck mono-red --opponent mono-green
cargo run --release -p mtg-cli -- bench
cargo run -p mtg-cli -- scenario              # run the rules-test corpus in scenarios/
```

Rules tests are written as scenarios (set up a board, script actions,
assert the result); see `scenarios/` and
[crates/mtg-scenario/SYNTAX.md](crates/mtg-scenario/SYNTAX.md).

Cards are written in a small DSL, one file per card under `cards/`; see
[crates/mtg-dsl/SYNTAX.md](crates/mtg-dsl/SYNTAX.md). A card file carries
no English: the engine renders its statements to Oracle text, and
`mtg-cards gate` checks that rendering against the Scryfall snapshot.

```
cargo run -p mtg-cards -- gate                # every card against its Oracle text
cargo run -p mtg-cards -- gate --diff "Pestilence"
cargo run -p mtg-cards -- author --format pauper --write   # author what the templates cover
cargo run -p mtg-cards -- coverage --format pauper --scenarios
cargo run -p mtg-cards -- model-author --write "Sorin's Thirst"   # ask Claude (ANTHROPIC_API_KEY), gate, write
cargo run -p mtg-cards -- model-author --write --list names.txt --jobs 6 --report report.tsv   # a batch
```

`model-author` asks a model for the card file and scenarios, then accepts
them only if the card renders to its Oracle text exactly and the scenarios
pass; `--prompt-only` prints the prompt instead of calling the API.

Python: the engine, batch simulation, and a PettingZoo environment live
in `crates/mtg-py` (see its README):

```
uv venv && uv pip install maturin pytest pettingzoo gymnasium numpy
.venv/bin/maturin develop --release -m crates/mtg-py/Cargo.toml
.venv/bin/python -c "import mtg_engine as me; print(me.simulate_batch('mono-red', 'mono-green', games=200))"
.venv/bin/python scripts/py_throughput.py     # env steps/s, one process and all cores
uv pip install -p .venv/bin/python torch --index-url https://download.pytorch.org/whl/cpu
.venv/bin/python scripts/ppo_selfplay.py --iters 4   # masked-PPO self-play, win rate vs greedy
```

MCP: `mtg-mcp` serves the engine to language models over stdio (the
default) or streamable HTTP; `.mcp.json` registers the built binary for
Claude Code in this repo (`cargo build --release -p mtg-mcp` first), so a
session can `new_game` and play with `act`, and finished games land in
`data/games` for the review tool:

```
cargo run --release -p mtg-mcp                      # stdio
cargo run --release -p mtg-mcp -- --http 127.0.0.1:8765   # http://127.0.0.1:8765/mcp
claude -p --mcp-config .mcp.json "play a game of mono-red vs mono-green with the mtg-engine tools"
```

Review: `mtg-review` indexes game records and serves a playback page
(board, decision and options, events, timeline) at
http://127.0.0.1:8780/ by re-simulating each record:

```
cargo run --release -p mtg-cli -- sim --games 10 --agent1 greedy --record-dir data/games
cargo run --release -p mtg-review -- --store data/games serve
```

Benchmark: `mtg-bench` plays a model against the agent ladder in
mirrored pairs and on win-this-turn puzzles, logs its reasoning, tokens,
and latency per decision, and fits Elo ratings anchored on the greedy
agent ([BENCHMARK.md](BENCHMARK.md)):

```
cargo run --release -p mtg-bench -- ladder --pairs 10 --out results.jsonl
cargo run --release -p mtg-bench -- rated --model claude-haiku-5-5 --out results.jsonl
cargo run --release -p mtg-bench -- report results.jsonl
```

Fuzzing draws random two-color decks from the whole corpus:

```
cargo run --release -p mtg-cli -- sim --deck1 random:7 --deck2 random:8 --games 1000
cargo run --release -p mtg-cli -- deck random:7
scripts/fuzz-random.sh                       # and scripts/fuzz.sh, scripts/bench.sh
```

## Layout

```
crates/
  mtg-core/      rules engine (state, events, layers, turn structure, decisions)
  mtg-dsl/       card DSL: parser, compiler, Oracle-text printer, built-in corpus
  mtg-cards/     card database: Scryfall/MTGJSON ingest, round-trip gate
  mtg-scenario/  scenario test DSL and runner
  mtg-agents/    agents: random, greedy, IS-MCTS
  mtg-sim/       batch simulation, fuzzing, benchmarks
  mtg-cli/       human/debug CLI
  mtg-py/        Python binding and PettingZoo environment
  mtg-mcp/       MCP server
  mtg-review/    game-record store and playback page
  mtg-bench/     benchmark harness for language models
cards/           the card corpus (*.card, one per card under cards/<letter>/), embedded at build time
scenarios/       rules-test corpus (*.scn); scenarios/puzzles/ holds the benchmark puzzles
scripts/         fuzz and A/B benchmark scripts
data/
  sources.lock   pinned Scryfall and MTGJSON snapshot (committed)
  scryfall/      fetched bulk data (gitignored)
  mtgjson/       fetched enumerations (gitignored)
```

## Card data

Card data is a development-time dependency. It is fetched on demand and
never at build time:

```
cargo run -p mtg-cards -- ingest            # fetch latest, update data/sources.lock
cargo run -p mtg-cards -- ingest --locked   # fetch exactly what the lockfile names
cargo run -p mtg-cards -- stats             # pool sizes from the local snapshot
cargo run -p mtg-cards -- stats --check     # compare with live Scryfall (within 1%)
```

Data comes from [Scryfall](https://scryfall.com/docs/api) (card objects
and rulings) and [MTGJSON](https://mtgjson.com) (keyword and type
enumerations, MIT). The rules tests under `scenarios/xmage/` are converted
from [XMage](https://github.com/magefree/mage)'s card tests (MIT, copyright
its contributors) by `mtg-cards convert-xmage`, which reads a sparse
checkout of XMage's test directory at the commit named in
`data/sources.lock`:

```
git clone --depth 1 --filter=blob:none --sparse https://github.com/magefree/mage.git data/xmage
git -C data/xmage sparse-checkout set Mage.Tests/src/test/java/org/mage/test
git -C data/xmage checkout <commit from data/sources.lock>
cargo run -p mtg-cards -- convert-xmage --out scenarios/xmage
```

A test converts when every call in it is one the converter knows and every
card it names is in the corpus; converted tests that the engine fails are
written with a `skip` line naming the failure, for adjudication. Scryfall data is used under its API terms: this project
is free, does not gate access to card data, and does not repackage or proxy
Scryfall's database.

## Building

Rust 1.88 or later, edition 2024.

```
cargo build --workspace
cargo test --workspace
```

## Licence

Code, card DSL files, scenario tests, and documentation are MIT licensed;
see [LICENSE](LICENSE). Card names, Oracle text, rulings, and the
Comprehensive Rules are the property of Wizards of the Coast and are not
covered by that licence.

mtg-engine is unofficial Fan Content permitted under the Fan Content
Policy. Not approved/endorsed by Wizards. Portions of the materials used
are property of Wizards of the Coast. ©Wizards of the Coast LLC.
