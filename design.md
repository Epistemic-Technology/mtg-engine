# MTG Engine: Design Document

Status: draft v0.1, 2026-10-24
Target: Linux x86-64. Rust 2024 edition, MSRV 1.88.

## 1. Purpose

A Magic: The Gathering rules engine whose two non-negotiable properties are
**rules fidelity** (behaviour matches the Comprehensive Rules, not a
simplification of them) and **speed** (thousands of complete games per
second per core with cheap agents). It exists so that AI systems can play
Magic:

- LLM agents, through tool use over the Model Context Protocol (MCP).
- RL agents, through an in-process Python environment (PettingZoo AEC API)
  and a state representation suitable for search (MCTS, IS-MCTS, PIMC).

Everything else is secondary: no GUI, no matchmaking, no deck builder,
no images.

### Non-goals (for now)

- Multiplayer formats beyond two players. The design must not preclude
  Commander, but v1 is 1v1.
- Un-sets, silver-border, Alchemy digital-only mechanics, Conspiracy,
  Planechase, Vanguard, Archenemy.
- Complete card coverage. Coverage grows behind a verification gate (§6).
- A competitive AI. We ship baseline agents (random, greedy, flat MC) for
  testing and as opponents; strong agents are a consumer of the engine.

## 2. Landscape (what the research found)

Facts here were verified on 2026-10-24; full source list in §12.

**Mature Java engines.**
- *Forge* (GPL-3.0): ~33,800 card scripts in a text `Field:Value` DSL with
  206 effect API types; a CR 613 layer enum with 7a/7b/7c sublayers; a
  headless `sim` CLI but no step/observe API; a small test suite (roughly
  800 TestNG tests, mostly AI tests, with CR-section tests only for 103
  and 104).
- *XMage* (MIT): ~32,600 one-class-per-card Java implementations; a `Layer`
  enum plus a `SubLayer` enum; a headless server with a test mode; about
  7,200 JUnit tests, 1,900 of which are scripted `CardTestPlayerBase`
  scenarios (set up board, script actions, run to a phase, assert).

**Rust engines, all 2026 and all alpha.** `phase` (34k cards parsed from
MTGJSON, thousands unimplemented), `arcana` (claims 20k cards, documents
its RL results as negative), `rustmage` (RL-focused, 4 sets complete,
Standard only), `MTG-Ichor` (the only one documenting CR 613 status
precisely: complete except layers 1b and 3), `cantrip-rs` (cube-focused,
ledger state). None is both broad and verified-correct. `MTG-Ichor` and
`rustmage` are worth reading before we commit to internal representations.

**Oracle-text parsing.** The strongest prior art is Argentum Assay
(Kotlin): a *bidirectional* grammar where correctness is
`print(parse(text)) == text`, gated over ~34,900 cards. Wizards' own Arena
pipeline is a Python parser that converts English card text into CLIPS
rules and handles "80% or so" of new cards automatically. We adopt the
round-trip idea (§6.3).

**LLM play today.** mage-bench runs LLMs against XMage over MCP; its
creator reports ~$1+ per game and that LLMs do not yet beat basic
decision-tree AIs. It is bottlenecked on the JVM bridge. There is no
reproducible, published full-MTG LLM or RL benchmark. That is the gap.

**Why build rather than wrap.** Forge and XMage were built around a GUI
and a mutable, GUI-driven game loop. Neither exposes clone/step semantics,
neither is fast enough for RL-scale simulation, and Forge's GPL licence
would govern anything linked to it. We treat both as references and as
sources of test cases, not as dependencies.

## 3. Design principles

1. **The Comprehensive Rules are the spec.** Every rules-bearing function
   cites the CR section it implements. A disagreement between the engine
   and Forge or XMage is settled by the CR text, not by majority vote.
2. **The engine never calls out.** It is a resumable step function:
   run until a decision is needed, return the decision and its complete
   legal option set, wait. Chance (shuffles, coin flips) is a decision
   made by a distinguished chance player, so a game is fully determined
   by its action history. This is the OpenSpiel model.
3. **Characteristics are derived, never stored.** Power, toughness,
   types, colours, abilities and controller are computed from printed
   values plus the set of active continuous effects via the CR 613
   layer algorithm, with caching.
4. **Everything is an event.** Replacement and prevention effects
   intercept events before they happen; triggered abilities observe them
   after; the event log is the replay, the LLM-facing game log, and the
   debugging trace.
5. **State is a flat, clonable value.** No reference graphs, no `Rc`.
   `clone()` is the search primitive.
6. **Coverage is gated by verification.** A card is "supported" only when
   its DSL implementation round-trips to its Oracle text and passes its
   scenario tests. Unsupported cards are rejected at deck-load time.

## 4. Architecture

### 4.1 Crate layout (Cargo workspace)

```
mtg-engine/
  crates/
    mtg-core/      rules engine: state, events, layers, turn structure, decisions
    mtg-dsl/       card DSL: lexer, parser, AST, pretty-printer (round-trip)
    mtg-cards/     card database: Scryfall/MTGJSON ingest, DSL corpus, loader
    mtg-agents/    baseline agents: random, greedy, flat MC, IS-MCTS
    mtg-sim/       batch simulation, fuzzing harness, benchmarks
    mtg-scenario/  scenario test DSL and runner
    mtg-py/        PyO3 bindings, PettingZoo AEC env
    mtg-mcp/       rmcp server (stdio and streamable HTTP)
    mtg-cli/       human/debug CLI, replay viewer
  data/
    cr/            Comprehensive Rules txt (pinned version)
    cards/         DSL card files, one per Oracle name, by first letter
    scenarios/     scenario tests
```

`mtg-core` has no async runtime, no I/O, no card-text knowledge. It
depends only on `mtg-dsl`'s AST types (effects, costs, conditions). Card
text lives entirely in `mtg-cards` as data.

### 4.2 Game state

```rust
pub struct GameState {
    version: u64,                 // bumped on every mutation; cache key
    rng: ChaCha8 state,           // seeded; only consulted at chance nodes
    players: [PlayerState; N],
    objects: SlotMap<ObjId, GameObject>,   // every card, token, copy, ability-on-stack, emblem
    zones: Zones,                 // per-zone ordered SmallVec<ObjId>; library/graveyard order matters
    stack: SmallVec<[ObjId; 8]>,
    effects: Vec<ContinuousEffect>,        // active continuous effects with layer+timestamp
    delayed_triggers: Vec<DelayedTrigger>,
    turn: TurnState,              // active player, phase/step (CR 500), priority holder, passes
    combat: Option<CombatState>,  // attackers, blockers, damage assignment order
    pending: Pending,             // what the engine is waiting on (decision, triggers to order, SBA loop)
    lki: LkiStore,                // last-known information for objects that left zones (CR 113.7a, 608.2h)
    timestamp_counter: u64,
    log: EventLog,                // append-only; optionally truncated for speed
}
```

Decisions taken:

- **Arena with generational keys** (`slotmap`). Object identity is an
  `ObjId`; an object that changes zones becomes a *new object* with a new
  id (CR 400.7), and the old id is retained in `lki` so effects that need
  last-known information can find it.
- **Full clone, not undo.** State is a handful of `Vec`s of small structs;
  cloning is a memcpy-scale operation. We measure this early (§8). If
  clone cost dominates we can move specific large structures to
  `Arc::make_mut` copy-on-write without changing the API. Undo stacks are
  not worth the complexity given SBA cascades and layer recomputation.
- **Deterministic RNG at chance nodes only.** Shuffling, coin flips and
  random choices are exposed as `Step::Chance { outcomes }`. Agents, test
  harnesses and determinizers may override the outcome. A default
  `Engine::step_auto` samples from the seeded RNG so simple callers never
  see chance nodes.

### 4.3 Game objects and characteristics

```rust
pub struct GameObject {
    printed: CardRef,             // index into the card database (immutable)
    kind: ObjKind,                // Card | Token | Copy | SpellCopy | AbilityOnStack | Emblem
    owner: PlayerId,
    zone: Zone,
    // mutable per-object facts that are inputs to layers, not outputs:
    counters: Counters,
    damage: u32,
    tapped: bool, flipped: bool, face_down: bool, phased_out: bool,
    attached_to: Option<ObjId>,
    controller_base: PlayerId,
    timestamp: u64,
    chosen: ChosenValues,         // X, chosen colour, chosen creature type, modes, etc.
}
```

Current characteristics are produced by
`fn characteristics(&self, id: ObjId) -> &Characteristics`, which computes
on a cache miss (keyed by `state.version`) using the CR 613 algorithm:

1. Start from the printed characteristics (or the copiable values when a
   copy effect applies: layer 1, CR 707).
2. For each layer 1 through 7 (with 7a–7d sublayers), collect the active
   effects for that layer, order them by timestamp, then apply the CR
   613.8 dependency rule: an effect that would change what another effect
   applies to, or does, is applied after it. Dependency is computed per
   layer, per recomputation, with a cycle fallback to timestamp order
   (CR 613.8c).
3. Characteristic-defining abilities (CDAs) apply in layer 7a and are
   evaluated against the state as it exists after layer 6.

Because every effect-bearing decision in the engine goes through this one
function, Humility plus Opalescence and other textbook layer puzzles are
handled by construction. We will encode the CR's own worked examples and
the well-known judge puzzles as tests on day one of the layer work.

Caching: a per-state `HashMap<ObjId, (version, Characteristics)>`. A
batch recompute of all battlefield objects is done once per
`version` change in practice, since SBA checks touch everything.

### 4.4 Continuous effects

```rust
pub struct ContinuousEffect {
    source: ObjId,
    layer: Layer,                 // L1Copy, L2Control, L3Text, L4Type, L5Color, L6Ability, L7a..L7d
    timestamp: u64,
    duration: Duration,           // UntilEndOfTurn, WhileOnBattlefield(src), UntilEvent(..), Permanent
    affected: AffectedSet,        // Static(SmallVec<ObjId>) for "locked in" sets (CR 611.2c), or Predicate(Filter)
    modification: Modification,   // SetPT, ModifyPT, AddAbility, RemoveAbilities, AddType, SetColor, ChangeController, ...
    condition: Option<Condition>,
}
```

Effects from resolving spells lock their affected set at resolution
(CR 611.2c); static abilities use a predicate re-evaluated at each
recomputation (CR 611.3). Both are first-class so the distinction is in
the data, not in special cases.

### 4.5 Event pipeline

```rust
pub enum Event {
    MoveZone { obj, from, to, cause },
    DealDamage { source, target, amount, combat: bool },
    Tap(ObjId), Untap(ObjId),
    DrawCard { player, n },
    GainLife { player, n }, LoseLife { player, n },
    AddCounters { obj, kind, n },
    Cast { spell }, Resolve { obj }, Counter { obj },
    BeginStep(Step), EndStep(Step),
    Attack { attacker, target }, Block { blocker, attacker },
    // ...
}
```

Applying an event is a three-stage pipeline:

1. **Replacement and prevention** (CR 614, 615). Collect applicable
   replacement effects; if more than one, the affected player (or the
   controller of the affected object) chooses which to apply first
   (CR 616.1) as a `Decision`. Each replacement effect applies at most
   once per event (CR 614.5). Self-replacement effects apply first
   (CR 614.15). The result is zero or more replacement events, which
   re-enter stage 1.
2. **Apply.** Mutate state, bump `version`, append to log.
3. **Trigger detection** (CR 603). Every triggered ability whose trigger
   condition matches the applied event is marked as triggered, using
   "look back in time" semantics for leaves-the-battlefield triggers
   (CR 603.10), which read from `lki`.

Triggered abilities are put on the stack the next time a player would
receive priority, after SBAs (CR 117.5, 603.3). When one player controls
several, they choose the order (CR 603.3b) as a `Decision`.

### 4.6 State-based actions and the priority loop

```
loop {
  loop {
    let sbas = check_sba(&state);          // CR 704.5 a..z, all simultaneously
    if sbas.is_empty() { break }
    apply_all(sbas);
  }
  put_triggered_abilities_on_stack();       // may yield ordering Decisions
  if any triggered { continue }             // CR 117.5: repeat SBA check
  break
}
give_priority(player)
```

Priority, passing, stack resolution (CR 608) and turn structure
(CR 500–514) are a straightforward state machine inside `TurnState`.
Special actions (playing a land, turning a face-down creature face up,
etc., CR 116) are enumerated alongside spells and abilities when a player
has priority.

### 4.7 The decision interface

This is the only interface agents see.

```rust
pub enum Step {
    Decision(Decision),
    Chance { player: ChancePlayer, outcomes: Vec<(ChoiceId, f64)> },
    Terminal(GameResult),
}

pub struct Decision {
    pub player: PlayerId,
    pub kind: DecisionKind,
    pub options: Vec<Option>,          // exhaustive and all legal
    pub context: DecisionContext,       // what prompted it (source object, event)
}

pub enum DecisionKind {
    Mulligan, PriorityAction, ChooseTargets, ChooseModes, ChooseX,
    PayCost, DistributeMana, OrderTriggers, ChooseReplacement,
    DeclareAttackers, DeclareBlockers, OrderBlockers, AssignDamage,
    Discard, ChooseObjects, ChoosePlayer, ChooseColor, ChooseNumber,
    ChooseCardName, Scry, Surveil, Explore, ...
}

impl Engine {
    pub fn start(cfg: GameConfig, decks: [Deck; N], seed: u64) -> (Engine, Step);
    pub fn step(&mut self, choice: ChoiceId) -> Step;
    pub fn state(&self) -> &GameState;
    pub fn view(&self, player: PlayerId) -> PlayerView;   // hidden info redacted
    pub fn clone(&self) -> Engine;
}
```

Rules for the option list:

- It is **exhaustive**: every legal choice appears, nothing illegal does.
  This is what makes action masking exact for RL and what stops LLMs from
  hallucinating illegal plays.
- **Casting is one decision, costs are later decisions.** `PriorityAction`
  options are "cast X", "activate Y", "play land Z", "special action",
  "pass". Choosing one leads into the CR 601.2 sub-steps (modes, targets,
  X, additional costs, payment) as further decisions. Each of those is
  small. This keeps the branching factor sane and matches how the rules
  describe casting.
- **Mana payment is canonicalized.** The engine computes whether the cost
  is payable (CR 601.2h) before offering "cast X". If exactly one
  payment is possible up to symmetry (e.g. which of three identical
  Mountains to tap), the engine auto-pays. Otherwise a `PayCost` decision
  offers distinct payment plans, not individual tap actions. Agents can
  opt into fine-grained payment for the rare cases where it matters
  (e.g. which land to leave untapped).
- Declaring attackers and blockers are **set decisions**: the options are
  the legal sets, pruned by requirement/restriction rules (CR 508.1d,
  509.1c), not one creature at a time. For large boards we fall back to
  incremental declaration with a "done" option; the engine still
  guarantees that "done" is only offered when the partial declaration is
  legal.
- `PlayerView` carries everything the player is entitled to know
  (CR 400.2, 401.2): own hand, revealed cards, public zones, the stack,
  known information about face-down and library-top cards, and the full
  event log filtered for hidden information.

### 4.8 Hidden information and search support

- `Engine::view(player)` produces the information set; `PlayerView` has a
  canonical string and tensor encoding for search algorithms that hash
  information sets.
- `Engine::determinize(player, rng)` resamples all information hidden
  from `player` consistently with that player's knowledge (opponent's
  hand drawn from their unknown library; library order reshuffled except
  known positions). This is OpenSpiel's `ResampleFromInfostate`, and it
  is what IS-MCTS and PIMC need (Cowling, Ward & Powley 2012).
- Serialization: `postcard` for compact binary snapshots and replay
  buffers, `serde_json` for the MCP and debugging surfaces. `bincode` is
  unmaintained as of its 3.0.0 tombstone release and is not used.

## 5. Card database and ingest

- **Primary corpus:** Scryfall `oracle_cards` bulk file (gzipped JSONL,
  ~25 MB compressed, ~39k Oracle objects of which ~34k are real cards).
  One object per `oracle_id`; we key everything by `oracle_id`. Required:
  `User-Agent` header; `data.scryfall.io` has no rate limit. Rulings come
  from the `rulings` bulk file (~5 MB), keyed by `oracle_id`.
- **Scryfall is a development-time dependency only.** `mtg-cards ingest`
  is an explicit command that downloads the bulk files into a gitignored
  `data/scryfall/` directory and writes a committed lockfile
  (`data/scryfall.lock`: bulk `updated_at`, download URI, SHA-256). CI
  fetches the exact snapshot named in the lockfile. `cargo build` never
  touches the network. The DSL card files (§6) carry the printed
  characteristics the engine needs, plus the `oracle_id` they were
  authored against, so the engine builds and runs offline. The snapshot
  is needed only to author new cards, to run the round-trip and rulings
  gates, and to regenerate the rulings text served by the MCP
  `card_info` tool, which is emitted as a generated file during the gate
  run. Refresh cadence: on demand and per set release, in line with
  Scryfall's guidance that weekly fetches suffice.
- **Enumerations:** MTGJSON (MIT) `Keywords.json` (keyword abilities,
  keyword actions, ability words), `CardTypes.json` (sub/supertypes per
  type), `EnumValues.json`. Scryfall gives only a `type_line` string;
  MTGJSON gives pre-split types.
- **Comprehensive Rules:** the WotC txt, pinned by effective date
  (currently 2026-09-25), vendored in `data/cr/`. The Academy Ruins API
  provides the same text as JSON keyed by rule number, which we use to
  generate the test index of CR examples (§7.2).
- **Multi-face cards.** Scryfall `layout` drives how a card is loaded:
  `split`, `flip`, `transform`, `modal_dfc`, `adventure`, `meld`,
  `prototype`, `leveler`, `class`, `saga`, `case`, `mutate`, `prepare`.
  Each face is a `CardFace`; the engine's object model has a
  `current_face` and layer 1 copy rules know about copiable values of
  each face (CR 707.8, 712).
- **Format pools** (Oracle-unique, 2026-10-24): Standard 5,164; Pauper
  10,881; Pioneer 15,086; Modern 22,718; Vintage 32,028.

## 6. Card implementation strategy

### 6.1 Why a DSL

Hand-writing 32k cards in Rust is infeasible and, more importantly,
unreviewable. A declarative DSL that compiles to an AST interpreted by
`mtg-core` gives us: a corpus that LLMs can write and humans can review;
a round-trip check against Oracle text; and a fixed, testable set of
effect primitives (Forge gets by with 206 effect API types; we expect a
similar order of magnitude).

**Provenance.** The DSL is our own design. It borrows the one-file-per-
card, closed-primitive-set approach that Forge proved scales to the full
card pool, and the round-trip-to-Oracle-text correctness criterion from
Argentum Assay. It does not borrow syntax, vocabulary, or scripts from
Forge, which is GPL-3.0 and would govern any derived work. Rejected
alternatives: adopting Forge's format outright (licence); relicensing the
project as GPL-3.0 to permit that (would also govern the Python bindings
and anything linking them); designing an isomorphic re-encoding of
Forge's ability vocabulary with our own syntax (defensible as interface
re-implementation, but it blurs the licence boundary with every card
translated rather than authored, for modest gain). The cost of this
decision is that we re-answer "how do I express this card" questions
Forge has already answered; the authoring pipeline (§6.4) and the
Pauper-first ordering are meant to keep that cost bounded.

### 6.2 Shape of the DSL

Text, one file per Oracle name, machine-written and human-readable.
Sketch, not final syntax:

```
card "Lightning Bolt" {
  cost {R}
  type Instant
  spell { deal_damage 3 to target any }
}

card "Humility" {
  cost {2}{W}{W}
  type Enchantment
  static { affects creature all: lose_all_abilities; set_pt 1/1 }
}

card "Wrenn and Six" {
  cost {R}{G}
  type Legendary Planeswalker — Wrenn
  loyalty 3
  loyalty +1 { return target card: land in your graveyard to your hand }
  loyalty -1 { deal_damage 1 to target any }
  loyalty -7 { emblem { static { instants_and_sorceries you own have retrace } } }
}
```

Compiled form: `CardDef { faces: Vec<FaceDef> }`, where a `FaceDef` holds
printed characteristics and a list of `AbilityDef`s (spell, activated,
triggered, static, keyword) whose bodies are `Effect` ASTs over a closed
set of primitives with typed `Filter`, `Target`, `Zone`, `Player`,
`Number` (literal, X, count-of) expressions. Keywords are desugared by the
compiler into their CR 702 definitions (flying becomes an evasion static,
deathtouch a damage-resolution rule, etc.), so the core sees no keyword
names except where the rules themselves refer to them (e.g. "has flying").

Parser: `logos` lexer + `chumsky` combinators, for good error messages on
machine-generated scripts. A pretty-printer renders the AST back to
English for the round-trip check.

### 6.3 The verification gate

A card is *supported* when all of the following hold:

1. **Round trip.** `render(parse(dsl))` normalizes to the card's Oracle
   text (modulo reminder text, which Scryfall includes in parentheses).
   This is Argentum Assay's correctness criterion. Exact-match is the
   goal; a curated allowlist handles Oracle text that is deliberately
   non-templated.
2. **Scenario tests.** At least one scenario test exercising each ability
   on the card, auto-generated by the authoring pipeline and reviewed.
3. **Rulings coverage.** Each official ruling that describes a concrete
   interaction has a corresponding scenario test, or an explicit
   "not testable in isolation" tag.
4. **Fuzz survival.** The card has participated in N random-agent games
   without panics or invariant violations.

Deck loading rejects unsupported cards unless `allow_unsupported` is set,
in which case the engine warns and the game is tagged as non-authoritative.

### 6.4 Authoring pipeline

`mtg-cards author <oracle_id>` runs: fetch Oracle text and rulings;
retrieve the DSL for the k most similar already-supported cards
(embedding search over Oracle text); prompt an LLM for the DSL and
scenario tests; compile; run the gate; iterate up to M times on compiler
or test failures; emit a PR-ready file plus a report. Humans review the
diff. Coverage expands in order of format demand: Pauper first (10,881
cards, few complex mechanics), then a Standard snapshot, then Modern.

## 7. Testing

Fidelity is a testing problem. The test corpus is the asset; the engine
is what passes it.

### 7.1 Scenario DSL

Modelled on XMage's `CardTestPlayerBase` pattern, which has 1,900 files
of precedent:

```
scenario "deathtouch kills regardless of toughness" {
  battlefield A: "Archangel of Thune"
  battlefield B: "Typhoid Rats"
  turn 2 B: attack "Typhoid Rats"
  turn 2 A: block "Archangel of Thune" with "Typhoid Rats"
  run_to turn 2 postcombat_main
  assert life A 23
  assert graveyard A contains "Archangel of Thune"
  assert graveyard B contains "Typhoid Rats"
}
```

The runner drives the decision interface with a scripted agent that
fails loudly if a scripted action is not among the legal options (this
catches legality bugs as well as effect bugs). Unscripted decisions
default to "pass" or the first option, and the scenario can pin them.

### 7.2 Test sources, in priority order

1. **CR worked examples.** The CR contains ~1,200 `NNN.N` rules, many
   with "Example:" text. Every example in sections 1–7 becomes a scenario
   or an explicit "untestable" tag. Tracked as a coverage percentage.
2. **Card rulings.** Scryfall's `rulings` file: each concrete ruling for a
   supported card becomes a scenario (§6.3).
3. **Ported XMage scenarios.** XMage is MIT. We port its scenario tests
   mechanically (a converter from `CardTestPlayerBase` Java to our
   scenario DSL) for cards we support, with attribution. Where our engine
   disagrees with a ported test, we adjudicate against the CR and record
   the verdict in the test file. Forge is GPL-3.0; we do not copy its
   scripts or tests.
4. **Layer puzzles.** A hand-written suite of the canonical CR 613
   interactions: Humility + Opalescence, Blood Moon + Urborg, Conspiracy
   + Xenograft, copy of a modified permanent, Painter's Servant with
   dependency, CDA interactions, text-changing effects, timestamp vs
   dependency ordering.
5. **Property tests** (`proptest`): object conservation across zone
   changes; SBA loop reaches a fixpoint; stack never contains an object
   not in `objects`; `clone()` then replay produces an identical state;
   `view(player)` never leaks hidden information (checked by
   determinizing twice and diffing the view).
6. **Fuzzing** (`cargo-fuzz`): random agents on random decks from the
   supported pool, checking for panics, infinite loops (CR 726 draws), and
   invariant violations. Runs continuously in CI.
7. **Differential testing against XMage** via its test mode, periodically,
   as a source of candidate bugs rather than as a gate.
8. **Snapshot tests** (`insta`) of serialized states and event logs for
   regression on a fixed set of seeded games.

## 8. Performance

### 8.1 Targets (to be validated in milestone 1)

| Metric | Target |
|---|---|
| Random-agent games/s/core, 60-card mono-red vs mono-green | ≥ 2,000 |
| `GameState::clone()` on a 20-permanent board | ≤ 5 µs |
| Characteristics recompute, 20 permanents, 10 effects | ≤ 20 µs |
| Decision enumeration at priority, typical board | ≤ 10 µs |
| Python `step()` round trip (no detach) | ≤ 20 µs |

Targets are per core on the current dev box (16 cores). If milestone 1
measurements miss by more than 3x, we revisit state layout before adding
features.

### 8.2 Techniques

- Enums over trait objects in hot paths; `SmallVec` for zones and option
  lists; no heap allocation in `step()` on the common path (measured with
  an allocation-counting test).
- `version`-keyed caches for characteristics and legal-action
  enumeration.
- Event log is optional and bounded; RL rollouts run with logging off.
- `rayon` for batch simulation; in Python, `py.detach()` around rollouts
  so the GIL is released and free-threaded 3.14t+ builds scale.
- `criterion` benchmarks in CI with regression thresholds.

## 9. Interfaces

### 9.1 Python (`mtg-py`)

PyO3 0.30 + maturin 1.15; wheels `abi3-py310` plus `cp314t` and `abi3t`
for free-threaded Python; Python 3.10–3.15.

Two layers:

1. **Thin engine binding.** `Engine`, `Step`, `Decision`, `PlayerView`
   exposed directly, plus `clone`, `determinize`, `serialize`, and a
   `simulate_batch(decks, n, agent, seed)` that runs on rayon with the
   GIL released. This is what MCTS/IS-MCTS implementations use.
2. **PettingZoo AEC environment.** AEC, not Parallel: PettingZoo's own
   docs warn that modelling sequential games with the simultaneous API
   causes bugs. Observation is `Dict({"observation": ..., "action_mask":
   ...})`, the PettingZoo Classic convention.

**Action space.** A fixed `Discrete(MAX_OPTIONS)` whose index selects an
entry in the current `Decision.options` list, masked to the live count.
The observation includes a per-option feature block (option kind, source
card id, target ids, cost summary) so a policy can score options by
content rather than by position. This avoids the deck-specific
fixed-action-table approach (the arXiv 2605.06066 benchmark used 478
hand-defined actions) and generalizes across decks and card pools.

**Observation.** Default encoding: per-zone card lists (card id,
characteristics, counters, tapped, summoning sick) padded to fixed
sizes; stack contents; life, mana pool, turn/phase, priority; the last K
events. Card identity is an integer id into the supported-card table;
embedding is the model's job. A `raw_view()` escape hatch returns the
`PlayerView` as a Python object for custom encoders.

**Chance.** The env resolves chance nodes internally with the seeded RNG
by default. A flag exposes them as decisions for the chance agent, which
OpenSpiel-style algorithms want.

### 9.2 MCP server (`mtg-mcp`)

Built on `rmcp` 3.7 (spec 2026-07-28, which is stateless: no session
handshake, so every tool call carries an explicit `game_id` handle minted
by the server). Transports: stdio for local Claude Code / Agent SDK use,
streamable HTTP for hosted benchmarks.

Tools (JSON schemas derived with `schemars`):

| Tool | Purpose |
|---|---|
| `new_game(deck_a, deck_b, seed, opponent)` | Start a game; `opponent` may be a built-in agent or another MCP seat |
| `get_state(game_id, player)` | `PlayerView` as compact text plus structured JSON |
| `get_decision(game_id)` | Current decision, with numbered, described options |
| `act(game_id, option)` | Apply a choice; returns the next decision or the events that occurred and the next decision |
| `get_log(game_id, since)` | Event log since a cursor, filtered to the player's knowledge |
| `card_info(name_or_id)` | Oracle text, rulings, current characteristics if on battlefield |
| `legal_summary(game_id)` | Short natural-language summary of what the player can do |

Text rendering matters for LLM play quality and cost. The default
`get_state` text is a dense board description with stable object labels
(e.g. `A3: Grizzly Bears 2/2, tapped`) that the LLM can reference in
`act`. The server accepts option indices, object labels, or a short
natural-language form that it resolves against the legal option list
and rejects if ambiguous.

### 9.3 CLI (`mtg-cli`)

Play as a human against a baseline agent, replay a game from its action
history, step through a scenario, and dump state. Primarily a debugging
tool.

## 10. Data, licensing, and attribution

- **Our code:** MIT or Apache-2.0 (decide before first public commit),
  with an explicit statement that card names, Oracle text, rulings, and
  the Comprehensive Rules are property of Wizards of the Coast and are
  not covered by the code licence. This is the practice of arcana,
  Majik, MageZero and others.
- **Fan Content Policy** notice, verbatim, in README and the MCP server's
  `server/discover` metadata: "[Title] is unofficial Fan Content
  permitted under the Fan Content Policy. Not approved/endorsed by
  Wizards. Portions of the materials used are property of Wizards of the
  Coast. ©Wizards of the Coast LLC."
- **No card images, set symbols, mana symbols as trademarks, or WotC
  logos** are distributed. Historical enforcement (Cockatrice 2013, Card
  Conjurer 2022, proxy sites 2023) has targeted images, art, and
  trademarks; Forge and XMage have shipped Oracle text for over a decade
  without challenge.
- **Scryfall terms:** the project must stay free with no paywall or
  registration gate; must add value beyond repackaging; must not be used
  "to create new games" (we implement Magic, not a new game); must set
  `User-Agent`. Scryfall and MTGJSON data are fetched on demand by
  `mtg-cards ingest` into a gitignored directory pinned by a committed
  lockfile (§5); they are not vendored, which keeps the repository small
  and mirrors Cockatrice's post-2013 posture. The DSL corpus (our own
  work product) is vendored and is what the engine actually ships.
- **MTGJSON** data is MIT. **XMage** tests are MIT and may be ported with
  attribution. **Forge** is GPL-3.0 and is read for understanding only.
- The IP status of Oracle text itself is unaddressed by any source we
  found; we treat it as WotC copyright used under the Fan Content Policy.

## 11. Roadmap

Each milestone has an acceptance test. Nothing is "done" without it.

**M0: Skeleton and data (1–2 weeks)**
- Workspace, CI, `mtg-cards ingest` (Scryfall + MTGJSON), CR vendored,
  Academy Ruins example extraction.
- Accept: `cargo test` green; `mtg-cards stats` prints pool sizes
  matching Scryfall within 1%.

**M1: Core loop with vanilla cards (4–6 weeks)**
- Zones, turn structure (CR 500–514), priority (CR 117), stack and
  resolution (CR 608), casting steps (CR 601), mana and payment
  canonicalization, combat (CR 506–511) including damage assignment
  order, SBA loop (CR 704), mulligans (CR 103.5), decision interface,
  chance nodes, seeded determinism, event log.
- Cards: basic lands, vanilla and French-vanilla creatures with evergreen
  evasion keywords, burn, simple pump, simple removal.
- Agents: random, greedy.
- Accept: 10k random-agent games with zero panics; CR 500–514 and 704
  examples pass; performance targets measured and recorded.

**M2: Continuous effects and the event pipeline (6–8 weeks)**
- CR 613 layers with dependency, CDAs, copy effects (CR 707), LKI,
  replacement and prevention (CR 614–616), triggered abilities including
  ordering and "look back" (CR 603), delayed triggers, auras and
  equipment, counters, planeswalkers, tokens, emblems.
- Accept: layer puzzle suite passes; CR 603, 608, 611–616, 700–707
  examples pass; fuzz clean.

**M3: DSL, authoring pipeline, scenario DSL (6–8 weeks)**
- `mtg-dsl` with round-trip printer; `mtg-scenario` runner; XMage test
  converter; `mtg-cards author`.
- Accept: 500 Pauper cards supported through the gate; 1,000 ported
  XMage scenarios passing or adjudicated.

**M4: Python and MCP (3–4 weeks)**
- `mtg-py` with PettingZoo AEC env and batch simulation; `mtg-mcp`
  stdio and HTTP; `mtg-cli`.
- Accept: a masked-PPO self-play run on a fixed Pauper matchup trains
  without env errors at ≥ 10k steps/s on 16 cores; Claude Code plays a
  complete game via MCP.

**M5: Coverage and hardening (ongoing)**
- Pauper to ≥ 95% supported; Standard snapshot; modal and multi-face
  cards; remaining CR 702 keywords; IS-MCTS baseline agent; published
  benchmark harness.

## 12. Open questions

1. **Flat clone vs. structural sharing.** Decided for flat clone; M1
   measurements may overturn it. The API does not change either way.
2. **Attack/block declaration as sets vs. incremental.** Sets are
   cleaner for RL masking; combinatorial on wide boards. Proposed hybrid
   in §4.7; validate with the fuzzer on token decks.
3. **Mana payment granularity.** Canonical plans by default; is the
   fine-grained mode needed for correct play in Pauper? Probably rarely.
4. **DSL surface syntax.** Forge-like `Field:Value` is proven but hard
   to read; the block syntax in §6.2 is a sketch. Decide during M3 after
   trying to write 50 cards by hand.
5. **Round-trip strictness.** Exact Oracle match may be unachievable for
   older or oddly templated cards. Measure the hit rate on Pauper before
   deciding the allowlist policy.
6. **Observation encoding ownership.** Engine-side default vs.
   Python-side only. Default engine-side (fast, consistent), with a raw
   escape hatch.
7. **Commander and multiplayer.** APNAP ordering (CR 101.4) and
   multiplayer SBAs are designed in but untested until a later milestone.
8. **Licence choice** between MIT and Apache-2.0.

## 13. Sources

Comprehensive Rules: https://magic.wizards.com/en/rules (txt effective
2026-09-25). Academy Ruins API: https://api.academyruins.com/docs.
Scryfall: https://scryfall.com/docs/api, /bulk-data, /rate-limits,
/cards, /layouts, /rulings. MTGJSON: https://mtgjson.com.
Fan Content Policy: https://company.wizards.com/en/legal/fancontentpolicy.

Forge: https://github.com/Card-Forge/forge and its Card-scripting-API
wiki. XMage: https://github.com/magefree/mage. phase:
https://github.com/phase-rs/phase. arcana:
https://github.com/levineuwirth/arcana. rustmage:
https://github.com/altf4/rustmage. MTG-Ichor:
https://github.com/lcmaier/MTG-Ichor. cantrip-rs:
https://github.com/cantrip-rs/cantrip-rs. Argentum Engine:
https://github.com/wingedsheep/argentum-engine. mage-bench:
https://github.com/GregorStocks/mage-bench. MageZero:
https://github.com/WillWroble/MageZero. WotC on Arena's rules parser:
https://magic.wizards.com/en/news/mtg-arena/on-whiteboards-naps-and-living-breakthrough.

Papers: Cowling, Ward & Powley, "Ensemble Determinization in MCTS for
MTG", IEEE TCIAIG 2012. Cowling, Powley & Whitehouse, "Information Set
MCTS", IEEE TCIAIG 2012. Churchill, Biderman & Herrick, "Magic: The
Gathering is Turing Complete", arXiv 1904.09828. da Costa Cunha et al.,
"Causal RL for Complex Card Games: An MTG Benchmark", arXiv 2605.06066.
Licato, Fields & Hollis, "Code Generation for Collectible Card Games with
Complex APIs", FLAIRS-36 2023.

Tooling: PyO3 0.30.1 https://pyo3.rs; maturin 1.15.0; PettingZoo 1.27.0
https://pettingzoo.farama.org/api/aec/; Gymnasium 1.4.0; OpenSpiel 2.0.2
`spiel.h`; MCP spec 2026-07-28
https://modelcontextprotocol.io/specification/2026-07-28; rmcp 3.7.0
https://github.com/modelcontextprotocol/rust-sdk; Rust 1.99.1, edition
2024. Crates: slotmap 1.1, smallvec 1.16, rayon 1.12, postcard 1.1,
rkyv 0.8, proptest 1.12, cargo-fuzz 0.13, criterion 0.8, insta 1.49,
logos 0.16, chumsky 0.13.
