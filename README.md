# mtg-engine

A Magic: The Gathering rules engine, written in Rust, built so that AI
systems can play Magic: LLM agents through tool use over the Model Context
Protocol, and RL agents through an in-process Python environment. Its two
non-negotiable properties are rules fidelity (behaviour matches the
Comprehensive Rules) and speed (thousands of complete games per second per
core with cheap agents).

Status: milestone M2 (continuous effects and the event pipeline) in
progress; the CR 613 layer system, Auras and Equipment, counters,
triggered abilities (CR 603) including delayed ones, replacement and
prevention effects (CR 614–616), activated abilities (CR 602), tokens,
control-changing effects, the legend rule, copy effects (CR 707),
characteristic-defining abilities, modal spells, and "you may" clauses
are in. Random agents play complete games with a 118-card pool at about
2,600 games per second per core on effect-free boards. A 319-scenario
rules corpus passes. See [design.md](design.md) for the architecture,
roadmap, and open questions.

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

## Layout

```
crates/
  mtg-core/      rules engine (state, events, layers, turn structure, decisions)
  mtg-dsl/       card DSL: parser, AST, pretty-printer
  mtg-cards/     card database: Scryfall/MTGJSON ingest, DSL corpus, loader
  mtg-scenario/  scenario test DSL and runner
  mtg-agents/    baseline agents
  mtg-sim/       batch simulation, fuzzing, benchmarks
  mtg-cli/       human/debug CLI
scenarios/       rules-test corpus (*.scn)
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
