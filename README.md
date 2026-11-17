# mtg-engine

A Magic: The Gathering rules engine, written in Rust, built so that AI
systems can play Magic: LLM agents through tool use over the Model Context
Protocol, and RL agents through an in-process Python environment. Its two
non-negotiable properties are rules fidelity (behaviour matches the
Comprehensive Rules) and speed (thousands of complete games per second per
core with cheap agents).

Status: milestone M2 (continuous effects and the event pipeline) is
complete: the CR 613 layer system, Auras and Equipment, counters,
triggered abilities (CR 603) including delayed ones, replacement and
prevention effects (CR 614–616), activated abilities (CR 602), tokens,
control-changing effects, the legend rule, copy effects (CR 707),
characteristic-defining abilities, modal spells, "you may" clauses, X and
non-mana costs, regeneration, damage redirection, the CR 614.12
look-ahead, planeswalkers (CR 306, 606), and emblems are in. M3 (the card
DSL and authoring pipeline) is under way: cards are text files under
`cards/` that compile to the engine's definitions and render back to
Oracle text, and a gate checks that rendering against Scryfall. The
corpus holds 2,222 cards (131 hand-written and scenario-tested, 2,090
authored from Oracle text by templates; 20% of Pauper), all matching
their Oracle text. Random agents play complete games at about 5,000
games per second per core on effect-free boards. A 378-scenario rules
corpus passes. See [design.md](design.md) for the
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
  mtg-agents/    baseline agents
  mtg-sim/       batch simulation, fuzzing, benchmarks
  mtg-cli/       human/debug CLI
cards/           the card corpus (*.card, one per card under cards/<letter>/), embedded at build time
scenarios/       rules-test corpus (*.scn)
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
enumerations, MIT). Scryfall data is used under its API terms: this project
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
