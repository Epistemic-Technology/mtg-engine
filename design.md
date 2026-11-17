# MTG Engine: Design Document

Status: draft v0.3, 2026-10-30 (v0.1 2026-10-24, v0.2 2026-10-29)
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

Humans need to be able to watch what those agents did. A web-based game
review tool (§9.4) that lists recorded games and plays them back visually
is part of the deliverable, because it is how rules bugs and agent
behaviour get inspected.

The intended public face of the project is a benchmark site in the style
of chessbench.org (§9.5): leaderboards of LLMs playing Magic on this
engine, with replays of their games. It is a consumer of the engine, the
MCP server, and the review tool, not part of the engine itself.

Everything else is secondary: no play GUI, no matchmaking, no deck
builder, no card images.

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
    mtg-review/    game record store and web playback server (§9.4)
    mtg-cli/       human/debug CLI
  data/
    sources.lock   pinned Scryfall and MTGJSON snapshot (committed)
    scryfall/      fetched bulk data (gitignored)
    mtgjson/       fetched enumerations (gitignored)
    cr/            Comprehensive Rules txt (pinned version)
    cards/         DSL card files, one per Oracle name, by first letter
    scenarios/     scenario tests
```

M0 scaffolds `mtg-core`, `mtg-dsl`, `mtg-cards`, `mtg-scenario`,
`mtg-agents`, `mtg-sim`, and `mtg-cli`. `mtg-py`, `mtg-mcp`, and
`mtg-review` are added in M4 when they have something to bind to.

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

Since M2 slice 5 the cache is keyed on a second counter, `cversion`,
bumped only by changes that can alter derived characteristics (the
battlefield's membership, counters, stored continuous effects,
attachments, control, copy effects, and graveyards, which feed
Tarmogoyf's defining ability); damage, life, tapping, mana, and draws
leave it alone, so a trigger-heavy board no longer recomputes every
object on every event. The discipline this needs (every such change
goes through `cbump`) is checked in debug builds: a sample of cache
hits is recomputed from scratch and compared, and the corpus and the
debug fuzz runs exercise it. It caught one miss on the first run (Aura
attachment on resolution).

Copy effects (layer 1, CR 707) are not stored as effects at all. A
`GameObject` carries `def`, the definition whose copiable values it has,
alongside `card`, the physical card; they differ only while a copy
effect applies, and the layers start from `def`. Everything that reads
the object's name, abilities, or precomputed flags reads `def`, so a
Clone copying Siege-Gang Commander triggers, counts as a trigger source,
and meets the legend rule as the card it copies, with no special cases;
leaving the battlefield it is its card again (CR 400.7, 707.?). "Except"
clauses (CR 707.9b) are a third field naming the definition whose
`copy_except` (subtypes, keywords, abilities) joins the copiable values,
so a Clone copying a Phantasmal Image inherits the Illusion type and the
sacrifice trigger. "Enters as a copy of" is chosen as the spell resolves,
set on the stack object, and carried through the zone change, so the
copied card's own "enters with" effects apply (CR 614.12). Characteristic-
defining abilities (CR 604.3) are static abilities whose parts are
`CdaPt` (layer 7a, evaluated against the working characteristics after
layer 6, so Urborg's Swamps count for Nightmare) or `CdaColors` (layer 5);
they take timestamp 0 within their layer (CR 613.2), which is also why
Transguild Courier stays all colors under Humility.

Planeswalkers (CR 306, M2 slice 6) are permanents like any other: their
loyalty is a counter kind (`CounterKind::Loyalty`) put on them as they
enter (CR 306.5b, in `move_object_now`), removed by damage (CR 120.3c, in
`deal_damage_now`, which marks damage only on creatures), and checked by
the state-based action of CR 704.5i. A loyalty ability is an activated
ability whose `Cost::loyalty` is nonzero: sorcery speed, one per
planeswalker per turn (`GameState::loyalty_activated`, cleared at turn
start), payable only with enough loyalty for a −N cost (CR 606.5).
Emblems (CR 114) are objects of `ObjKind::Emblem` in the command zone,
listed in `GameState::emblems`; the layer pass takes them as sources of
static abilities but never as affected objects. X (CR 107.3) is chosen as
part of casting and kept on the spell object (`GameObject::x`); effect
amounts are `Num`, a constant or X, and the entering-with-counters
replacement reads X from the spell as it enters (Endless One).

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

Control (layer 2) is the one layer not derived into `Characteristics`:
the rest of the engine reads `GameObject::controller` directly, so
`sync_control` (M2 slice 4) writes layer 2 into that field from the owner
plus every control-changing effect in timestamp order. It runs when such
an effect is created, when "until end of turn" effects end, and before
each state-based-action check, and is gated to nothing by counts of
control sources and effects on boards without any. A permanent whose
controller changes leaves combat (CR 506.4) and loses "controlled since
the turn began" (CR 302.6).

Delayed triggered abilities (CR 603.7, M2 slice 5) are a list on the
state of (controller, `StackAbility`): the ability is a triggered ability
registered in the pool (not printed on any card) and the object it is
about travels as the ability's triggering object. The ability carries
the definition of its source, so it still has a name once Flickerwisp
has died or Otherworldly Journey is in the graveyard. Each fires once at
its step and is removed; its effects refer to the object through
`Recipient::SameObject`, which finds it only if it has not changed zones
since (CR 400.7), so a token copy that died before the end step is not
sacrificed twice.

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

**Status 2026-11-15 (M2 slice 3):** all three stages are implemented.
Stage 1 lives in `mtg-core/src/engine/replace.rs`: an event that can be
replaced (damage, life gain, a draw, a zone change, counters) is a
`Proposed` value that goes through `GameState::propose`, which collects
every applicable replacement or prevention effect, applies them one at a
time (each at most once per event, CR 614.5), and performs what is left.
Effects come from the static abilities of battlefield permanents
(`AbilityDef::Replacement`, read from current characteristics so Humility
removes Phantom Tiger's shield), from the entering permanent's own
"enters with" abilities (CR 614.1c; the CR 614.12 look-ahead through
continuous effects is not modelled), and from effects stored by resolved
spells (`GameState::replacements`: Fog's turn-long prevention, damage
shields with a remaining amount per CR 615.7). When one event becomes
several (draw two instead of one) each happens in turn with its own
replacement check (CR 614.11a), so effects that become applicable only
after another applied get their chance (CR 616.2). "Can't" effects
(Skullcrack: players can't gain life, damage can't be prevented) are
stored rules, not replacement effects (CR 614.17); unpreventable damage
still runs prevention effects for their side effects without reducing a
shield (CR 615.12). When two or more effects apply to one event the
affected player (or the affected object's controller) chooses which
applies first (CR 616.1) through `DecisionKind::ChooseReplacement`. That
choice arises in the middle of applying an event, where the state machine
cannot pause; rather than restructure every effect into a resumable queue,
the engine records the question, lets the first option stand, and
`Engine::advance` rolls the state back to a snapshot taken before the
action and replays the action with the answer. The snapshot (a
`GameState` clone, about 3 µs) is taken only when two or more replacement
sources exist, so ordinary boards pay nothing; the replay is deterministic
because the RNG is part of the state. Since M2 slice 5 the same mechanism
serves "you may" clauses and "enters as a copy of" (CR 707), gated by a
precomputed per-ability "asks a question" flag on whatever is on the
stack; several questions in one action (a Clone copying a legend, then
the legend rule) are answered in sequence by the recorded answers. Modes,
by contrast, are chosen as the spell is cast (CR 601.2b) through an
ordinary `Flow::ChoosingMode` state before targets, since there the
engine can pause.

Stage 3: `GameState::emit` is the single chokepoint: every event runs
`detect_triggers` before it is logged, so detection works with logging
off. A card's triggered abilities are
`AbilityDef::Triggered { trigger: TriggerSpec, condition, targets,
effects }`; `TriggerSpec` names the event kind and what it watches (`This`
or an `ObjFilter`). Detection reads abilities from *current*
characteristics, so Humility stops triggers. Leaves-the-battlefield
triggers look back (CR 603.10): the object that left is read from `lki`,
and during a batch of simultaneous zone changes (the SBA loop) the whole
battlefield is snapshotted into `GameState::lookback` first, so Blood
Artist sees a creature that dies alongside it. Triggered abilities wait in
`pending_triggers`; `Flow::Priority` puts them on the stack in APNAP order
before anyone receives priority, with `Flow::OrderTriggers` /
`DecisionKind::OrderTriggers` when a player has several (each choice puts
one on the stack; the first chosen resolves last). An ability on the stack
is a `GameObject` of `ObjKind::Ability` carrying a `StackAbility` (ability
id, source id, triggering object and player); its targets are chosen
through the same `Flow::Casting` path as a spell's (CR 603.3d; an ability
with no legal target is removed instead), and it resolves through
`resolve_ability`, which re-checks intervening "if" clauses (CR 603.4) and
target legality. Effects name who they act on with `Recipient` (`Target(n)`,
`You`, `EachOpponent`, `Source`, `TriggeringObject`, `TriggeringPlayer`,
`DefendingPlayer`); `Recipient::Source` follows `LastKnown::moved_to`, so
Rancor's trigger finds the graveyard object it became (CR 400.7). Keywords
that are triggered abilities (prowess) are registered once in the pool and
looked up by keyword during detection. Trigger-free boards pay nothing: a
count of battlefield objects with printed triggers gates detection, and
event kinds are matched against printed abilities before characteristics
are computed.

Destruction is an event of the pipeline since slice 6 (`Proposed::Destroy`),
so regeneration (CR 701.15) is a stored replacement effect that applies to
it once: remove all damage, tap, remove from combat, and the shield is
spent. "Can't be regenerated" destruction carries a flag the shield does not
match. Damage redirection (Pariah) is a replacement that returns a new
damage event with the attached creature as target, which other effects may
then replace in turn (CR 616.2). The CR 614.12 look-ahead is real: to decide
which "enters" replacement effects of a permanent apply, `candidates` asks
the layer pass for the characteristics the permanent would have on the
battlefield under the effects now in play, its own static abilities
included (`layers::compute_entering`), so a Phantom Tiger under Humility
enters without counters and a Guildgate under Blood Moon enters untapped.

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

Casting a spell can involve several decisions in order (CR 601.2):
`Flow::ChoosingMode`, then `Flow::ChoosingX` (the options are the values of
X the player can pay for), then targets, then the choices an additional
cost needs, a permanent to sacrifice or a card to discard
(`DecisionKind::ChooseSacrifice`, `ChooseDiscardCost`), before everything
is paid at once in `finish_cast`. Activated abilities carry those cost
choices on the `Choice::Activate` option itself, one option per way to
pay. When the defending player controls a planeswalker, declaring attackers
is followed by one `ChooseAttackTarget` decision per attacker (CR 508.1a).

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
- **Game records.** Because a game is fully determined by its
  configuration, decks, seed, and action history, a `GameRecord` is just
  those four things plus metadata (agents, result, duration, tags,
  engine and card-corpus versions). It is the unit of storage for the
  review tool (§9.4), for RL replay, and for regression tests: a record
  can be re-simulated to produce every intermediate state. Records are
  small (a few KB) and written by `mtg-sim`, the Python env, and the MCP
  server whenever recording is enabled. The optional full event log is
  stored alongside only when the caller asks for it, since it can be
  regenerated.

### 4.9 Lessons taken from MTG-Ichor and rustmage (read 2026-10-30)

Before locking M1's representation we read the two most relevant alpha
Rust engines. They sit at opposite ends of one axis: MTG-Ichor is
callback-driven and rules-complete-first (the engine *calls* a decision
provider), rustmage is a synchronous `legal_actions()` / `apply(action)`
state machine tuned for two million decisions per second. What we took:

- **Step function, with every in-flight decision on the state.** Ichor's
  callback design left only about a third of its prompts forkable and
  "pausing at a decision" is its largest pending redesign. Our `Flow`
  enum holds partial casts, declarations, and damage assignment, so a
  clone at any decision resumes. Keep it that way: no decision site may
  hold outcome-affecting state outside `GameState`.
- **Never-reused object identity.** Both engines mint a new identity per
  zone change (CR 400.7). We use `slotmap` generational keys, so a stale
  id can never alias a live object; the mover writes last-known
  information, consumers never do.
- **Plain cloned state, no undo journal, no per-action checkpoint.**
  rustmage's full-state checkpoint plus `catch_unwind` on every action
  costs it 27–32% of throughput and forces `panic=unwind`. We validate
  before mutating. Copy-on-write (`Arc::make_mut`) is reserved for the
  object arena and libraries if clone cost ever matters; at 2.8 µs it
  does not yet.
- **Board-wide layer passes, one coarse cache epoch.** Ichor computes
  characteristics one board-wide pass per layer so later applications
  in a layer see earlier ones, and caches with a single `layer_epoch`
  audited in debug builds. rustmage's hand-ordered stat code cannot grow
  a CR 613.8 dependency algorithm. M2 follows Ichor.
- **One mutation chokepoint.** Ichor's proposed-action → replacement
  pipeline → performed-event design is what let replacement effects,
  triggers and the log plug in later; rustmage's per-family events forced
  a panic-unwind hack for zone replacements. M2 introduces the chokepoint
  before any replacement effect exists.
- **SBA/trigger loop at every priority point**, including after each
  resolution and the cleanup 514.3a probe, looping while triggers were
  placed. rustmage defers the re-check to the next action, which is
  subtly late. M1 already runs SBAs at every priority entry.
- **Combat declarations: incremental with completion-preserving
  filters** is rustmage's answer to open question 2 and avoids handing an
  agent a 2^n option list. M1 enumerates full sets up to 1,024 options
  and falls back to incremental above that; M4's action encoding decides
  which becomes the default.
- **Hidden information and chance are nobody's solved problem.** Neither
  engine has determinization; Ichor calls its missing visibility model a
  v1 blocker. M1 therefore ships `visible_to` and `observe(viewer)` from
  the start. Shuffles are still sampled from the in-state RNG; exposing
  chance nodes or a branch-reseed hook is an M2 item so search does not
  replay the same future draws.
- **Measured floors in CI.** Ichor enforces decisions-per-core-second
  and clone-cost bounds in CI with a counting allocator. We add the same
  once the numbers below have a few weeks of history.

## 5. Card database and ingest

- **Primary corpus:** Scryfall `oracle_cards` bulk file (gzipped JSONL,
  ~25 MB compressed, ~39k Oracle objects of which ~34k are real cards).
  One object per `oracle_id`; we key everything by `oracle_id`. Required:
  `User-Agent` header; `data.scryfall.io` has no rate limit. Rulings come
  from the `rulings` bulk file (~5 MB), keyed by `oracle_id`.
- **Scryfall is a development-time dependency only.** `mtg-cards ingest`
  is an explicit command that downloads the bulk files into a gitignored
  `data/scryfall/` directory and writes a committed lockfile
  (`data/sources.lock`: per file, the bulk `updated_at`, download URI,
  SHA-256 and size; MTGJSON build version likewise). CI
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

Text, one file per Oracle name under `cards/`, machine-written and
human-readable. Decided 2026-11-17 (M3 slice 1) after porting the 138-card
M2 pool: one statement per line inside a `card "Name" { }` block, header
lines for the printed characteristics, then one line per ability whose
effects are a controlled English of verb phrases. Full syntax in
`crates/mtg-dsl/SYNTAX.md`. Real cards from the corpus:

```
card "Lightning Bolt" {
  oracle_id 4457ed35-7c10-48c8-9776-456485fdf070
  cost {R}
  type Instant
  spell: deal 3 damage to any target
}

card "Humility" {
  oracle_id ed7bdb3e-5c51-4547-9266-76a791e0b2b0
  cost {2}{W}{W}
  type Enchantment
  static creature: loses all abilities, base 1/1
}

card "Siege-Gang Commander" {
  oracle_id ddc7f59a-bbb1-4ba1-82c8-6813fd191940
  cost {3}{R}{R}
  type Creature — Goblin
  pt 2/2
  triggered when this enters: create 3 "Goblin"
  activated {1}{R}, sacrifice Goblin: deal 2 damage to any target
}

card "Elspeth, Knight-Errant" {
  oracle_id eab0d8b9-c66f-402f-b259-f46f15cefd78
  cost {2}{W}{W}
  type Legendary Planeswalker — Elspeth
  loyalty 4
  loyalty +1: create 1 "Soldier"
  loyalty +1: target creature gets +3/+3 and gains flying
  loyalty -8: emblem "Elspeth, Knight-Errant emblem"
}
```

The file carries no English: the printer renders the statements to Oracle
text ("When this creature enters, create three 1/1 red Goblin creature
tokens." / "{1}{R}, Sacrifice a Goblin: This creature deals 2 damage to
any target."), and that rendering is what the engine shows players and
what the gate compares. Sentence structure is part of the statement
(`;` separates sentences, `and` joins clauses, a block line starts a
paragraph, a delayed trigger is written trigger-first or effect-first),
so the printer can reproduce Oracle's wording exactly instead of
approximating it.

Pipeline (`mtg-dsl`): a hand-written line-oriented lexer and recursive-
descent parser (the same style as the scenario runner; `logos`/`chumsky`
were not needed and would have added two dependencies for a grammar this
small) produce a syntax tree (`ast.rs`) that mirrors `mtg_core::card`
but keeps names where the engine interns ids and keeps the author's
sentence structure. `compile.rs` turns it into `CardDef`s in a
`CardPool` (tokens and emblems first, so `create "Goblin"` resolves);
`render.rs` prints Oracle text; `format.rs` prints canonical DSL text
(`parse(format(x)) == x`, tested over the corpus); `decompile.rs` goes
from a `CardDef` back to the tree, which is how the M2 pool was ported
(it round-tripped 138 of 138 definitions on the first run) and is a
standing check that the language can express everything the engine
holds. `build.rs` embeds `cards/` so `mtg_dsl::builtin_pool()` needs no
files at run time; a pool builds in about 1 ms (release). Keywords stay keywords
(the engine implements CR 702 directly); multi-face cards are not yet
in the grammar (M5).

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

Status 2026-11-17: gate 1 is implemented (`mtg-cards gate [--diff]
[names]`, and a test that runs when the Scryfall snapshot is present).
The comparison normalizes both sides: reminder text in parentheses is
dropped and a keyword-only line is split one keyword per line (Oracle
prints "Flying, vigilance" on one card and "Flying\nDeathtouch" on
another). The first run over the ported M2 pool matched 111 of 131 and
exposed four cards the engine had been implementing as something other
than their Oracle text (Healing Salve is modal, Dromoka's Gift is
"Bolster 4", Benalish Cavalry has flanking, Mishra's Factory has a third
ability) and a dozen smaller divergences in costs, type lines, and
wording ("Human Wizard Sorcerer", Regenerate costs {1}{G}, Ravenous
Bloodseeker is 1/3); after fixes 131 of 131 match and one card carries an
`oracle_mismatch` tag (Leyline of the Void's opening-hand ability, not
implemented). Gate 4 (fuzz participation) is served by random decks
drawn from the whole pool (`mtg sim --deck1 random:<seed>`,
`scripts/fuzz-random.sh`), so every card in the corpus gets played.
Gate 2 is measured, not enforced: `mtg-cards coverage --scenarios`
reports which corpus cards no scenario names (2026-11-17, after slice 3:
145 of 2,717; 1,841 cards with abilities have none, nearly all
template-authored commons whose abilities are the same few primitives
the hand-written cards' and `scenarios/m3/primitives.scn` exercise). Gate 3 (rulings) is not started. The
corpus therefore has two support levels: the 131 hand-written cards are
*verified* (gates 1, 2, 4); the template-authored cards are *supported*
(gates 1 and 4). Deck loading does not yet distinguish them.

### 6.4 Authoring pipeline

Status 2026-11-17 (M3 slice 2): the part of the pipeline that needs no
model is in. `mtg-cards author --format <f> [--write]` reads the Scryfall
snapshot and, for every card of a normal layout legal in the format,
builds a card file from a fixed set of sentence templates (the inverse of
the printer's: "~ deals N damage to <target>", "Target creature gets
+N/+N until end of turn", "Enchanted creature gets ...", "{T}: Add ...",
"This land enters tapped.", "When this creature enters, ...", "<cost>:
<effects>", and so on), skipping any card with a keyword the engine does
not implement, a sentence no template matches, a quoted ability, or a
hybrid/Phyrexian mana symbol. Every candidate is parsed, rendered, and
compared to its Oracle text before it is accepted, so the written files
pass gate 1 by construction (the first run over Pauper found 54 that did
not, all printer wording: "-3/-0", "Untap it", cost order, a keyword
printed after "enters tapped"; fixed by making those part of the DSL).
The report's skip reasons, by count, are the backlog: Scry, Cycling,
Surveil, Kicker, Flashback, "Counter target spell", "Add one mana of any
color", "can't block", "Activate only once each turn". Hand-written files
are never overwritten. Result: 2,090 Pauper cards authored in one run,
every one matching its Oracle text; the corpus went from 138 definitions
to 2,229 (Pauper coverage 20%, Modern 8%). Fuzzing draws random two-color
decks from the whole pool (`mtg sim --deck1 random:<seed>`), which is how
these cards get played.

Slice 3 (2026-11-17) worked the top of that backlog into the engine and
the templates: counterspells (`TargetSpec::Spell`, `Effect::Counter`,
"unless its controller pays" as a mid-resolution decision for the
spell's controller), scry and surveil (a decision over which of the top
cards go down; the rest keep their order, a simplification of CR
701.18), cycling (an activated ability from the hand whose cost discards
its source), discard effects, fight, "activate only once each turn",
any-color mana in the payment planner, the combat restrictions "can't
attack", "can't block", "attacks each combat if able", and "can't be
blocked" (kept with the keywords so effects can grant them), and targets
given by any filter (`TargetSpec::Filtered` over filters the pool
interns: "target nonblack creature", "target creature an opponent
controls"). That authored 563 more Pauper cards. It also exposed a bug in
slice 2's output: a spell with two paragraphs ("Scry 1.\nDraw a card.")
had been written as two `spell` statements, of which the engine ran only
the first; the compiler now rejects that, the author writes one `spell`
block, and the 68 affected files were regenerated.

Slice 5 (2026-11-17) built the model-driven part: `mtg-cards
model-author [--model claude-opus-5-5] [--attempts 3] [--write]
<names...>` (`llm.rs`). For each card it assembles a prompt pack (the
two syntax references as a cached system prompt; the card's Oracle text,
rulings from the snapshot, the header lines pre-filled from Scryfall,
the six corpus cards whose Oracle text shares the most words, and the
common cards scenarios may use), asks the model for a card block and
two to four scenarios, and runs the same gate the templates face: parse,
compile into a copy of the pool, render back to Oracle text and compare,
parse and run the scenarios. Any failure goes back to the model as the
next turn, up to the attempt limit; `oracle_mismatch` is forbidden and
the model may answer `unsupported: <reason>` instead, which is the
signal for the next engine primitive. Only an accepted card is written
(`cards/`, with its scenarios under `scenarios/llm/`), so model-authored
cards are *verified* in the §6.3 sense from the start. `--prompt-only`
prints the pack without calling the API. First live run (2026-11-17,
five Pauper cards the templates had skipped, `claude-opus-5-5`): one
accepted on the first attempt (Sorin's Thirst, whose third scenario
tests its ruling that an illegal target means no life gain, which is
gate 3 in practice); four declared unsupported with exact reasons that
are the engine backlog: "that card" after an immediate return from
exile (Justiciar's Portal), "target attacking creature" (Second
Thoughts), base power and toughness until end of turn (Water Wings), a
color-restricted discard cost (Krovikan Sorcerer). One attempt was
rejected for a parse error and the model answered correctly on the
second. 7,315 input tokens uncached, 44,425 from cache, 3,907 out.

Slice 6 (2026-11-17) did what that run asked for and then ran the model
author at scale. Engine and DSL: kicker (CR 702.33: `kicker {R}`, a
`ChoosingKicker` flow step after modes and before X, offered only when
the kicker is payable on top of the cost; `if kicked, ...` in a spell
and `if kicked` as a trigger condition, the permanent keeping its
spell's kicked flag per 702.33c), flashback (CR 702.34: `flashback
{4}{R}` or `flashback {1}{U}, pay 3 life`; a graveyard card is offered
as `Choice::Cast` like a hand card, pays the flashback cost, and is
exiled wherever it would leave the stack, with a per-player count so
priority enumeration skips graveyards that hold none), the five landwalk
keywords (CR 702.14, checked in block legality behind one bitmask test),
`target attacking creature` and `target blocking creature`, "has base
power and toughness N/N until end of turn" with the `until end of turn,`
sentence prefix Oracle uses for it, and filtered discard costs ("Discard
a creature card"). The scenario runner learned `cast ... kicked` and
`cast ... from graveyard`. The templates follow (Kicker and Flashback
lines, "if it was kicked", "If this spell was kicked", landwalk,
filtered discards), which authored 170 more cards (123 Pauper). Of the
first run's four unsupported cards, three are now in (Second Thoughts,
Water Wings, Vampire Hounds for the discard cost); Krovikan Sorcerer
also needs "discard one of them".

`model-author` gained batch mode: `--list <file>` of names, `--jobs N`
workers over one pass of the snapshot for records and rulings, a
`--report` file of one line per card, retries with backoff on 429 and
5xx, and cards already in the corpus skipped. First batch (2026-11-17,
`claude-opus-5-5`, six workers): the 240 Pauper cards with the shortest
Oracle text among those the templates skip for an unrecognized sentence,
after filtering out text that names things the engine plainly lacks
(library search, tokens, Treasure, extra turns, ...). 67 accepted (28%),
each with two to four passing scenarios; 172 unsupported; 1 rejected
after three attempts (Ajani's Mantra). 93 cards took a second attempt
and 19 a third. 517,532 input tokens uncached, 3,339,938 from cache,
262,418 out. The unsupported reasons, clustered: 89 want a group
recipient the DSL lacks ("tap all artifacts", "destroy all enchantments",
"white creatures you control get +2/+2"), 12 a conditional static ("as
long as", "during your turn", "can't block black creatures"), 10 "up to
N targets", 4 triggers on being dealt damage or blocked, 4 "its
controller" for a targeted spell, 2 one-shot color changes, and the rest
assorted (an imperative "Mill N cards", legendary short names, the
Ring). The group recipient is the next primitive; it alone would have
raised this batch's acceptance to about 65%.

Slice 7 (2026-11-17) added it: `Recipient::Group` over the pool's
interned filters (`each artifact`, `each blue creature`, `each creature
without flying`, `each white creature you control`), printed as Oracle
does ("Tap all artifacts", "deals 1 damage to each creature without
flying", "White creatures you control get +2/+2"), with `without
<keyword>` in filters, the imperative "Mill N cards", and `all` on a
static ability's filter for the old Sliver wording ("All Sliver
creatures get +1/+1"). The templates follow ("all artifacts", plural
subjects, Mill, the Sliver statics): 138 more cards. Seven hand-written
cards and scenarios (`scenarios/m3/groups.scn`) cover each form. Second
model-author batch (249 cards: the 89 batch-1 cards whose reason named a
group, plus the next 160 shortest candidates): 45 accepted (one of them
a Scryfall token record, Assembly-Worker, dropped afterwards; the model
author now checks the header fields and skips token records), 182
unsupported, 2 rejected; 466,675 input tokens uncached, 3,075,316 from
cache, 217,814 out. The retries disappointed (6 of 89): the regex that
picked them had caught "All Sliver creatures" statics (31, now fixed
after the batch) and "attacking creatures" filters (31), neither of
which the group recipient covers. The fresh cards accepted at 24%. The
reasons now cluster as: filters and statics over attacking or blocking
creatures (31, needs combat state in `ObjFilter::matches`), conditional
statics and evasion restrictions ("as long as", "can't be blocked by
Walls", 19), triggers and designations outside the engine (monarch, the
initiative, 16), "can't attack/block alone" and "must be blocked" (6),
"multicolored" (6), "up to N" targets (4), and a long tail. Attacking
and blocking filters are the next primitive.

The plan as first written: `mtg-cards author <oracle_id>` runs: fetch Oracle text and rulings;
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
of precedent. Implemented in `mtg-scenario` (pulled forward from M3 on
2026-11-14 so that M1 rules claims rest on more than fuzzing; full syntax
in `crates/mtg-scenario/SYNTAX.md`):

```
scenario "deathtouch kills regardless of toughness" {
  battlefield A: "Serra Angel"
  battlefield B: "Typhoid Rats"
  start turn 2 B
  turn 2 B: attack "Typhoid Rats"
  turn 2 A: block "Typhoid Rats" with "Serra Angel"
  run_to turn 2 postcombat_main
  assert life A 20
  assert graveyard A contains "Serra Angel"
  assert graveyard B contains "Typhoid Rats"
}
```

The runner starts the engine from a prepared position
(`Engine::from_setup`: objects placed directly in zones, no mulligans) and
drives the ordinary decision interface. A scripted action that is not
among the legal options fails the scenario, so the corpus checks legality
as well as effects; `illegal turn 1 B: block ...` asserts the opposite.
Unscripted decisions keep, pass, declare nothing, or take the first option.
Spells can be scripted `in response to "X"`; sorcery-speed actions wait
for an empty stack. The corpus lives in `scenarios/` and runs under
`cargo test -p mtg-scenario`; `mtg scenario --verbose` prints the event
log of a failing scenario.

Status 2026-11-17 (M3 slice 4): the XMage converter is in
(`mtg-cards convert-xmage [--out scenarios/xmage]`, source `xmage.rs` in
`mtg-cards`). It reads a sparse checkout of XMage's test directory pinned
in `data/sources.lock`, extracts each `@Test` method of a
`CardTestPlayerBase` subclass, and maps the scripting API onto the
scenario DSL: `addCard` to setup lines (library cards on top of thirty
Plains, since XMage's test players draw from a real deck and ours from
Plains), `castSpell`/`activateAbility`/`attack`/`block`/`playLand` to
scripted actions (an `activateAbility` text prefix is matched against the
rendered texts of the player's permanents' abilities), `waitStackResolved`
to the new `wait` action, `setStopAt` to `run_to`, and the `assert*`
family to assertions; XMage's implicit targeting became `targeting auto`.
Each converted test is run; failures are written with a `skip` line
naming the failure, for adjudication. The result is sobering: of 6,802
tests, 28 convert and 27 pass. 5,895 name a card outside the corpus
(XMage's tests are overwhelmingly per-card tests of complex cards; the
2,565 distinct missing cards are each named by a handful of tests) and
836 use custom harness code. Authoring every format's template-coverable
cards (958 more, corpus 3,675) unblocked four tests. The 1,000-scenario
acceptance target therefore waits on coverage, not on the converter: it
needs the model-driven authoring of §6.4 and the engine primitives it will
ask for. The converter's missing-card histogram is the ordering for that
work.

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

**M1 baseline (2026-10-30, release build, random agents, 60-card
mono-colored decks, logging off):**

| Metric | Measured |
|---|---|
| Games/s, one thread (mono-red vs mono-green, 2,000 games) | 2,620 |
| Games/s, 16 threads (SMT) | 11,800 |
| Mean decisions per game | ~490 |
| Decisions/s, one thread | ~1.3 M |
| `GameState::clone()` after 200 decisions | 2.8 µs |
| 10,000 games across five deck pairs | 0 panics, 0 unfinished |

The games/s target is met before any optimization; characteristics are
recomputed on every query with no cache, so the first real cost will
appear when M2 adds continuous effects at scale.

**M2 slice 1 (2026-11-14, release build, random agents, one thread,
rustc 1.100).** Baseline for the same binary before the layer system was
2,965 games/s on mono-red vs mono-red.

| Matchup | games/s/core | Note |
|---|---|---|
| mono-red vs mono-red (no effects) | 2,830 | fast path: printed characteristics |
| mono-green vs mono-green (counters) | 1,950 | counters force the layer pass |
| mono-white vs mono-black (anthems, auras) | 850 | |
| layers vs mono-green | 1,130 | |
| layers vs layers (Humility, Opalescence, Blood Moon...) | 770 | O(n²) dependency checks per layer |
| `GameState::clone`, 200 decisions in | 3.1 µs | was 2.0 µs; counters and attachment fields |

**M2 slice 2 (2026-11-15, release build, random agents, one thread, 3,000
games, seed 11).** Same binary measured against the slice 1 commit on the
same morning: mono-red vs mono-red 2,842 → 2,758 (−3%), layers vs layers
767 → 740, mono-white vs mono-black 845 → 832.

| Matchup | games/s/core | Note |
|---|---|---|
| mono-red vs mono-red (no effects, no triggers) | 2,758 | detection gated off by the trigger-source count |
| mono-green vs mono-green (counters) | 1,899 | |
| mono-white vs mono-black (anthems, auras) | 832 | |
| layers vs layers | 740 | |
| triggers vs mono-red | 802 | |
| triggers vs triggers (every trigger card at once) | 351 | 691 decisions/game; counters and Rancor force a layer pass per event |
| `GameState::clone`, 200 decisions in | 3.0 µs | |

**M2 slice 3 (2026-11-15, release build, random agents, one thread pinned
with `taskset`, best of five runs of 2,000 games, seed 11).** The machine
was shared with an unrelated eight-core job, hence best-of-five rather
than one run. Same method on the slice 2 binary: mono-red vs mono-red
2,679.

| Matchup | games/s/core | Note |
|---|---|---|
| mono-red vs mono-red (no effects) | 2,579 | −3.7% against slice 2 |
| mono-green vs mono-green (counters) | 1,820 | |
| mono-white vs mono-black (anthems, auras) | 806 | |
| layers vs layers | 723 | |
| triggers vs triggers | 349 | |
| replacement vs mono-red | 852 | |
| replacement vs replacement (every replacement card at once) | 487 | 518 decisions/game; snapshots on most steps |

**M2 slice 4 (2026-11-15, same method as slice 3, measured against the
slice 3 binary built the same evening).** Enumerating activated abilities
at every priority decision first cost 4–6% on effect-free boards (the
stub build without it ran faster than slice 3); gating the enumeration
and the mana-source ordering behind a maintained count of permanents
with activated abilities brought the cost back to noise.

| Matchup | games/s/core | slice 3 binary | Note |
|---|---|---|---|
| mono-red vs mono-red (no effects) | 2,595 | 2,606 | −0.4% |
| mono-green vs mono-green (counters) | 1,840 | 1,853 | |
| mono-white vs mono-black (anthems, auras) | 808 | | |
| layers vs layers | 736 | | |
| triggers vs triggers | 344 | 348 | |
| replacement vs replacement | 487 | 495 | |
| activated vs mono-red | 933 | | |
| activated vs activated (every slice 4 card at once) | 267 | | 1,023 decisions/game; tokens, Equipment, Pestilence |

The effect-free cost was chased with stub builds: removing the pipeline
gate entirely, or the engine's snapshot bookkeeping, each recovered under
1%, so most of the 3.7% is code layout (the crate grew by 700 lines under
thin LTO with one codegen unit) rather than work done. One real cost was
found and fixed on the way: an inline `Option<GameState>` snapshot field
made `Option::take` copy 1.5 KB on every step; it is boxed.

The trigger-heavy board is slow for the known reason: each event that a
trigger could see asks for current characteristics, and with counters or
statics on the board that is a full layer pass per event. Pre-filtering
watchers by event kind (a `Tapped` event never wakes an enters trigger)
took that matchup from 266 to 351 games/s; incremental recomputation of
characteristics is still the real fix. Two cheaper lessons: a per-event
scan of the battlefield for "does anything here have a trigger" cost 13%
on trigger-free boards and was replaced by a maintained count, and boxing
the new `GameObject::ability` field to shrink the struct made no
measurable difference, so it stays inline. Variation between builds from
code layout alone was about ±3% on this benchmark, which bounds what
single-run comparisons can show.

**M2 slice 5 (2026-11-16, same method, measured against the slice 4
binary built the same evening).** The characteristics cache is now keyed
on `cversion`, bumped only by changes that can alter derived
characteristics, instead of on `version`, which every tap and mana
payment bumped; that is the "incremental recomputation" the slice 4 note
asked for, in its cheapest form, and it roughly doubles throughput on
every board with statics, counters, or triggers. The effect-free board
first lost 14%: `characteristics()` had started returning
`base_characteristics(obj)`, which merges a copy's `except` abilities into
the printed values, and although the merge never ran on these boards its
presence (growable vectors, a drop path) kept the common case from being
the plain 72-byte copy it was before. Stub builds found it (bypassing the
call recovered 13 of the 14 points; fat LTO changed nothing, so it was not
layout), and the fix was structural: the cached `Printed` result is only
derived when no battlefield object has an `except`, so that arm copies the
printed values directly, and the merge lives in a `#[cold]` function. The
lesson from slice 1 holds in a sharper form: a hot accessor must not even
*contain* a slow path.

| Matchup | games/s/core | slice 4 binary | Note |
|---|---|---|---|
| mono-red vs mono-red (no effects) | 2,605 | 2,625 | −0.8%, within run-to-run noise |
| mono-green vs mono-green (counters) | 2,232 | 1,838 | +21% |
| mono-white vs mono-black (anthems, auras) | 1,216 | 812 | +50% |
| layers vs layers | 1,328 | 729 | +82% |
| triggers vs triggers | 604 | 340 | +78% |
| replacement vs replacement | 560 | 482 | +16% |
| activated vs mono-red | 1,281 | 933 | +37% |
| activated vs activated | 424 | 262 | +62% |
| copy vs mono-red | 1,437 | | |
| copy vs copy (every slice 5 card at once) | 891 | | 609 decisions/game |

**Perf pass after slice 5 (2026-11-16, same method, measured against the
slice 5 binary).** The first profile ever taken of the engine (`perf` was
enabled on the machine that evening) showed the state-based check at 44%
of an effect-free game: it ran after every action and walked every object
in the game, library cards included, collecting their keys into a vector
first. Two changes: it now walks the battlefield plus a list of tokens
that have left it (`stray_tokens`, filled by `move_object_now`), and it is
skipped unless `sba_dirty` is set, which `cbump`, damage, life changes,
and a draw from an empty library do and tapping and mana do not. In debug
builds a skipped check still runs and asserts it would have done nothing,
the same discipline as the characteristics cache. The effect-heavy
profiles then showed `SipHash` at 12–21% (the derived-characteristics map
and the layer pass's index are keyed by `ObjId`) and `CardDef::may_watch`
at 12% on triggers (a walk over abilities per watcher per event). Maps
keyed by `ObjId` now use a one-multiply hasher (`ids::IdMap`), and each
definition carries a bitmask of the event kinds its triggers can fire on,
so "could this care about this event" is one `&`. Throughput roughly
doubled everywhere:

| Matchup | games/s/core | slice 5 binary | Note |
|---|---|---|---|
| mono-red vs mono-red (no effects) | 5,119 | 2,600 | +97% |
| mono-green vs mono-green (counters) | 4,784 | 2,229 | +115% |
| mono-white vs mono-black (anthems, auras) | 2,813 | 1,217 | +131% |
| layers vs layers | 2,732 | 1,327 | +106% |
| triggers vs triggers | 1,166 | 603 | +93% |
| replacement vs replacement | 703 | 560 | +26% |
| activated vs mono-red | 2,522 | 1,274 | +98% |
| activated vs activated | 745 | 423 | +76% |
| copy vs mono-red | 2,723 | 1,432 | +90% |
| copy vs copy | 1,560 | 891 | +75% |

Fuzz after the pass: 72,000 release games across the deck pairs and
400 debug-assert games, 0 panics. What the profiles show now: the
effect-free game is spread thin (`run` 12%, `mana_sources` 12%,
`characteristics` 11%, malloc/free about 14%, mostly the per-decision
option and target vectors); the trigger and layer boards spend 18–21% in
`compute_battlefield`, the full layer pass that still runs once per
`cversion` change; and the replacement board spends 63% cloning the state
for the mid-decision snapshot, because the gate (§4.5) fires on every
action once two replacement sources are in play, whether or not the action
can cause a replaceable event. That snapshot is the next perf target: a
finer gate (does this action lead to an event the effects in play
replace?) or a cheaper snapshot (copy on write for the object arena).

**M2 slice 6 (2026-11-16, same method, measured against the perf-pass
binary built the same evening).** The slice's rules cost 2–4% on most
boards and nothing on the replacement board. The first build cost 5%: the
new `x` field pushed `GameObject` from 120 to 128 bytes, which the size
test allows but every object copy and battlefield scan paid for; X now
lives in a small list on the state keyed by the spell on the stack, and the
object is 120 bytes again. The rest is diffuse: a differential profile
(same games, absolute sample counts) shows no function grown by more than
1% of the total, small increases in `run`, `apply_choice`, `emit`, and
malloc against decreases in `mana_sources` and `characteristics`, which is
inlining and layout shifting under a crate that grew by 2,500 lines. The
size test now prints `Flow` and `Event` too (56 and 40 bytes).

| Matchup | games/s/core | perf-pass binary | Note |
|---|---|---|---|
| mono-red vs mono-red (no effects) | 4,894 | 5,079 | −3.6% |
| mono-green vs mono-green (counters) | 4,619 | 4,800 | −3.8% |
| layers vs layers | 2,684 | 2,742 | −2.1% |
| triggers vs triggers | 1,169 | 1,179 | −0.8% |
| replacement vs replacement | 719 | 709 | +1.4% |
| activated vs activated | 740 | 756 | −2.1% |
| copy vs copy | 1,494 | 1,561 | −4.3% |
| slice6 vs mono-red | 2,408 | | |
| slice6 vs slice6 (every slice 6 card at once) | 590 | | 801 decisions/game, 36 turns |

The first version of `characteristics()` returned a struct holding two
`SmallVec`s and cost 42% of throughput even on effect-free boards; making
`Characteristics` `Copy` (fixed-capacity lists) recovered all but 5%. The
lesson generalises: anything the engine returns per rules query must be
plain data. The slow path recomputes every battlefield object on each
state version; incremental recomputation and memoised dependency checks
are the obvious next steps once the effect model stops changing.


**M3 slice 1 (2026-11-17, same method, measured against the slice 6
binary).** The pool now comes from the DSL (`mtg_dsl::builtin_pool`,
1.1 ms to build, once per process), card definitions are the same
structures as before, and no engine hot path changed except one new
`TargetSpec` arm, so throughput is unchanged where the cards are
unchanged: mono-red 4,892→4,898 games/s/core, copy deck 1,481→1,495.
The slice 6 deck went from 582 to 854 because its cards changed under
the gate (Regenerate costs {1}{G}, Ravenous Bloodseeker is 1/3, Mishra's
Factory has its pump): games are shorter (mean 31 turns and 673
decisions, from 37 and 823) and the per-decision rate is about 20%
higher, which the changed card mix explains well enough not to
profile. Fuzz: 36,000 release games over 15 matchups, 0 panics.


**M3 slice 2 (2026-11-17, same method, measured against the slice 1
binary).** The pool grew from 138 to 2,229 definitions and the agents
gained the free-activation guard; the stress decks are unchanged, so
throughput should be too, and is: mono-red 4,907→4,891 games/s/core,
copy deck 1,493→1,483, slice6 857→836. The guard's first version
allocated a `Vec` of allowed options per decision and cost 5% on the
effect-free deck (the random agent is part of the measured loop); it
now takes an allocation-free path when no activation is on offer. A new
baseline: two random decks drawn from the whole pool (`random:208` vs
`random:258`, 2,000 games) run at 945 games/s/core with 840 decisions
per game. Fuzz: 15,000 random-deck games (half with greedy agents) and
36,000 stress-deck games, 0 panics, 0 unfinished.


**M3 slice 3 (2026-11-17, same method, measured against the slice 2
binary).** New per-step checks (a blocker's "can't block", an attacker's
"attacks each combat if able", hand cards' cycling abilities, spells as
targets) cost 1–2% on every stress deck, inside the ±3% build-to-build
band: mono-red 4,821→4,716 then 4,818→4,764 on a repeat, copy
1,475→1,445, triggers 1,144→1,129, slice6 837→818. Random decks over the
2,717-card pool run at 1,195 games/s/core (737 decisions per game; the
slice 2 figure of 945 was over a 2,229-card pool, so the decks differ).
Fuzz: 15,000 random-deck games (half greedy) and 36,000 stress-deck
games, 0 panics, 0 unfinished; the random-deck run first caught the
greedy agent panicking on a mid-resolution discard whose options named
a card drawn by the same resolution, absent from the state the decision
is shown against (§4.9's self-describing-options rule), fixed by giving
`Choice::Discard` the card's definition.

**M3 slice 6 (2026-11-17, same method, measured against the slice 5
binary, which has slice 3's engine).** The first build carried the
kicked and flashback flags as two bools on `GameObject`, which grew it
from 120 to 128 bytes, and every stress deck lost 4–5% (mono-red
4,763→4,535, copy 1,440→1,389, slice6 831→792, triggers 1,136→1,116);
the profiles showed no new hot function, only `characteristics` and the
allocator up a point each, which is what a fatter object looks like.
Packing the mode and both flags into one byte (`CastInfo`) put the size
back to 120 and the numbers inside the band: mono-red 4,763→4,661
(−2.1%), copy 1,467→1,444, slice6 831→821. The per-step additions
(kicker check only for cards with one, graveyard scan only while a
flashback card is there, landwalk behind one bitmask test) cost nothing
measurable. The random-deck figure is not comparable across pools (the
decks differ), so it is not reported here. Fuzz: 36,000 stress-deck and
10,000 random-deck games, 0 panics, 0 unfinished.

**M3 slice 7 (2026-11-17, same method, measured against the slice 6
binary).** Group recipients add nothing to the hot path (a `Group`
recipient is matched only when such an effect resolves): mono-red
4,525→4,577, copy 1,388→1,399, slice6 813→812. Fuzz: 36,000 stress-deck
and 10,000 random-deck games over the 4,124-card pool, 0 panics, 0
unfinished.

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

Status 2026-11-17 (M4 slice 1): `crates/mtg-py` is in, built with
maturin into the repo's `.venv` (`uv venv`; see the crate README). The
thin layer: `Engine(deck_a, deck_b, seed)` over deck names or card
lists, `step`, `options` (described), `observation(player)`,
`option_features(max)`, `action_mask(max)`, `describe(player)` (the
dense text with `#n` labels the MCP server will use too),
`view_json`, `clone`, `determinize(player, seed)` (new in `mtg-core`:
the opponent's hand is redrawn from hand and library together and
libraries are shuffled, identities kept), `record_json` and
`from_record_json`, `Agent(name)` for the built-in agents, and
`simulate_batch` on rayon with the GIL released (19,300 games/s and
9.4M decisions/s over 16 cores on the effect-free decks). The encoders
live in Rust (`encode.rs`): an observation of 1,890 floats (global
scalars, decision kind, 10 hand slots, 24 battlefield slots, 6 stack
slots, 46 features per card: pool id, mana value, types, P/T, tapped,
sick, damage, combat, controller, colors, keywords) and 37 features per
option (kind, source and target card ids, target player, a count, a
cost). `MtgEnv` is the AEC environment, `Discrete(max_options)` with
the mask; it passes PettingZoo's `api_test`. `opponent="greedy"` plays
seat 1 internally for single-agent training. Throughput with a masked
random policy: 40,000 steps/s in one process and 132,000 steps/s on
16 processes, against the M4 target of 10,000. Option lists longer than
`max_options` (128) are rare (none in 20,000 random-deck decisions;
the engine falls back to incremental declaration for large attack and
block sets) and are counted in `env.overflow`. The chance flag is not
implemented: the engine's only chance node is the opening shuffle.

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

Status 2026-11-17 (M4 slice 2): `crates/mtg-mcp` is in, on rmcp 3.11
(spec 2026-07-28), serving stdio by default and streamable HTTP with
`--http <addr>` (axum, endpoint `/mcp`). Games live in a table keyed by
a server-minted `game_id`; the caller takes one seat and a built-in
agent (`greedy` by default, or `random`) plays the other between the
caller's decisions, or `opponent: "none"` gives the caller both seats.
Tools: `new_game`, `get_state` (the §9.2 dense text with `#n` labels,
plus the view as JSON in `structuredContent`), `get_decision`
(numbered options, every one legal; also structured), `act` (a number,
the exact option text, or a phrase that picks exactly one option, such
as a `#n` label, a card name, or "pass"; replies with the events the
seat saw and the next decision), `get_log` (events since a cursor,
with hidden information withheld: an opponent's draw is "a card"),
`card_info`, `legal_summary` (options grouped by kind), `get_record`
(the §4.8 record, for the review tool), `end_game`, `list_games`. The
server sends instructions at initialize telling the model the protocol:
new_game, then act with the numbers from the decision it is handed. An
in-process rmcp client plays a whole game through the tools in the
crate's test; a raw JSON-RPC exchange over the stdio binary is the
smoke test. `.mcp.json` at the repo root registers the server for
Claude Code as `cargo run -q --release -p mtg-mcp`, so the M4
acceptance game can be played from a fresh Claude Code session in the
repo. Rulings are not served (the snapshot lives in `mtg-cards`'s data
directory, outside the engine); `card_info` gives Oracle text.

### 9.3 CLI (`mtg-cli`)

Play as a human against a baseline agent, replay a game from its action
history, step through a scenario, and dump state. Primarily a debugging
tool.

### 9.4 Game review (`mtg-review`)

A web application for humans to find and watch recorded games. It is how
we inspect what an RL policy learned, read an LLM's reasoning next to the
board it was looking at, and diagnose a rules bug reported as "game X,
turn 7".

**Store.** A directory of `GameRecord` files (§4.8) plus a SQLite index
over their metadata: timestamp, format, deck names, agent names and
versions, result, turn count, tags, and any free-text notes. `mtg-review
import <dir>` indexes records produced by the other crates; `mtg-sim` can
also write straight into the store.

**Server.** A Rust binary (axum) serving a JSON API and static assets.
Endpoints: list and filter games; fetch a game's metadata; fetch the
sequence of step snapshots for a game. Snapshots are produced by
re-simulating the record with `mtg-core`, which is deterministic, so
nothing but the record is persisted. Each snapshot is the full public
state plus, for the review view, both players' hidden information (a
reviewer is allowed to see everything; a `redact` flag hides one side for
reviewing play from a single seat). Snapshots for a game are cached in
memory after first request.

**Playback UI.** A single-page app served by the same binary:

- Game list with filters (format, decks, agents, result, date, tags) and
  search by game id or note text.
- Board view per step: both players' life, mana pool, battlefield grouped
  by controller with tapped/counters/attachments shown, hands, graveyards,
  exile, libraries (count and known top cards), the stack with targets
  drawn as arrows, and the current phase and priority holder.
- A timeline: step forward and back, jump to a turn or phase, jump to the
  next decision by a chosen player, scrub.
- The event log alongside, synchronized with the timeline, with the
  decision taken at each step and the full legal option list that was
  available, so a reviewer can see what the agent could have done.
- An agent-commentary panel: for LLM games, the model's reasoning text
  for each decision, recorded by the MCP server; for RL games, the
  policy's option scores if the trainer attached them.
- Card rendering is text-only (name, cost, type line, Oracle text,
  current characteristics), per the no-images policy in §10.

**Technology.** Backend is Rust so it shares `mtg-core` types directly.
Frontend is TypeScript with SolidJS (decided 2026-10-30), built with Vite
to static files that the Rust binary serves; the built assets are
checked in so the server has no Node dependency at run time. Solid's
fine-grained reactivity suits a playback view where a timeline scrub
changes one step's worth of board state at a time. The server runs
locally during development and can be deployed as a service later for
sharing games.

**Milestones.** The `GameRecord` format and deterministic re-simulation
land in M1, since that is when determinism exists and when the CLI
replay needs it. `mtg-review` v1 (store, list, playback with event log)
is part of M4 alongside the Python and MCP surfaces, because those are
what produce games worth reviewing. The agent-commentary panel follows
once the MCP server records reasoning.

### 9.5 Public benchmark site (`mtg-bench`, working name)

Intent recorded 2026-11-17; not yet scheduled (see M6 in §11). A public
website modelled on chessbench.org and chessbench.ai: a leaderboard of
language models ranked by how well they play Magic on this engine, with
every rated game available as a replay. It fills the gap named in §2:
there is no reproducible, published full-MTG LLM benchmark.

**Tracks.**

- *Rated games.* Models play a fixed gauntlet of decks against a ladder
  of built-in agents (random, greedy, IS-MCTS once it exists) and against
  each other. Decks are built only from scenario-tested cards. Games are
  played in mirrored pairs (same seed, decks and seats swapped) to reduce
  the variance that shuffling and hidden information add. Ratings are Elo
  within the benchmark pool only; the built-in agents are the fixed
  anchors, since Magic has no equivalent of Stockfish to calibrate
  against.
- *Puzzles.* Positions written in the scenario DSL (§7.1) with an
  engine-verified solution, e.g. "win this turn". Cheap, low-variance,
  and scored exactly; the natural first track to ship.

**Harness.** A model sees its `PlayerView` as text and the numbered
legal options for the current decision (§4.7), through `mtg-mcp` (§9.2)
or a thinner loop over the same rendering. A response that does not
resolve to a legal option is rejected, counted, and retried, as
chessbench.org does with illegal moves. An auto-pass policy (when
priority is passed without asking the model) is fixed by the benchmark
and applied identically to every model, because priority passes dominate
the decision count and so the cost. Each decision's reasoning text,
token counts, cost, and latency are stored with the `GameRecord`.

**Leaderboard.** Per model and thinking level: Elo with an interval,
win/loss rate, invalid-response rate, puzzle score, cost per game,
output tokens per game, and time per decision. Every result names the
engine and card-corpus versions it was played on. A rules fix that
could change outcomes bumps the version; affected games are flagged and
replayed rather than silently kept.

**Replays.** The review tool (§9.4) deployed read-only, with the
agent-commentary panel showing the model's reasoning beside the board.
This resolves open question 10 in favour of a deployed service once the
site exists.

**Constraints.** The whole site is free and needs no login (§10).
Running costs may be subsidized by donations or sponsors but nothing is
gated on them. Text-only cards, no images. Human play against bots is
out of scope (decided 2026-11-17): model-backed opponents are too
expensive to offer, and a hosted place to play Magic carries more
licensing exposure than a benchmark does (§10).

## 10. Data, licensing, and attribution

- **Our code:** MIT (decided 2026-10-29), with an explicit statement in
  `LICENSE` and `README.md` that card names, Oracle text, rulings, and
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
- **Benchmark site (§9.5).** The Fan Content Policy requires content to
  be free: no payment, subscription, or registration to access it.
  Donations (the FAQ names Patreon), ad revenue, and clearly labelled
  sponsors are allowed so long as they do not restrict access, so no
  patron-only results or replays. Read 2026-11-17, the policy's FAQ also
  appears to exclude "game mechanics" from what it licenses and its
  rules say not to use Wizards' IP "in other games". If so, the engine
  and anything playable built on it rest on general IP law and on the
  tolerance Forge and XMage have had, not on the policy, and the notice
  above overstates our footing. To be verified against the full policy
  text before the site launches (open question 13).

## 11. Roadmap

Each milestone has an acceptance test. Nothing is "done" without it.

**M0: Skeleton and data (1–2 weeks)**
- Workspace, CI, `mtg-cards ingest` (Scryfall + MTGJSON), CR vendored,
  Academy Ruins example extraction.
- Accept: `cargo test` green; `mtg-cards stats` prints pool sizes
  matching Scryfall within 1%.

**M1: Core loop with vanilla cards (4–6 weeks; started 2026-10-30)**

Status 2026-11-14: scenario runner (§7.1) added with a 72-scenario corpus
covering combat keywords, the stack, timing, lands and mana, cleanup, and
game end; all pass. Authoring the corpus found no engine bugs; it found
three mis-authored tests (Murder is an instant; an instant scripted after
a response fires in response). `engine.rs` split into `engine/{mod,
objects, casting, sba, combat}.rs`. Greedy one-ply agent added
(`mtg-agents`; beats random 95% of games with the stronger deck, 72% with
the weaker). `mtg play` gives terminal play against a baseline agent.
Remaining for M1: chance nodes (deferred to M2, where the first random
effects appear).

Status 2026-10-30: first cut complete and green. Zones, turn structure,
priority, stack, casting with targets and canonical mana payment, combat
(attack and block declaration as sets with incremental fallback,
first/double strike, trample divisions, menace, flying/reach, deathtouch,
lifelink, vigilance, haste, defender, indestructible, hexproof), SBAs,
London mulligan, decision interface, event log, `GameRecord` with replay,
`observe(viewer)`, random agent, parallel batch simulation, 28-card M1
pool with five 60-card decks. Not yet: `mtg-cli replay` prints the log but
there is no human play mode; no scenario DSL (M3), so rules tests beyond
invariants are thin; no chance nodes; the engine source is one large
file and wants splitting.
- Zones, turn structure (CR 500–514), priority (CR 117), stack and
  resolution (CR 608), casting steps (CR 601), mana and payment
  canonicalization, combat (CR 506–511) including damage assignment
  order, SBA loop (CR 704), mulligans (CR 103.5), decision interface,
  chance nodes, seeded determinism, event log, `GameRecord` format with
  re-simulation (`mtg-cli replay`).
- Cards: basic lands, vanilla and French-vanilla creatures with evergreen
  evasion keywords, burn, simple pump, simple removal.
- Agents: random, greedy.
- Accept: 10k random-agent games with zero panics; CR 500–514 and 704
  examples pass; performance targets measured and recorded.

**M2: Continuous effects and the event pipeline (6–8 weeks; started 2026-11-14)**

Status 2026-11-16, perf pass after slice 5, done the evening `perf` became
usable on the development machine. The first profile put the state-based
check at 44% of an effect-free game (a walk over every object after every
action); it now walks the battlefield and a list of stray tokens and is
skipped unless something it checks has changed, with a debug-build check
that a skipped pass would have done nothing. Object-keyed maps use a cheap
hasher and trigger watching is a per-definition bitmask of event kinds.
Throughput roughly doubled on every board (§8): 5,119 games/s/core
effect-free, 2,732 on the layers deck, 1,166 on triggers, 1,560 on the
copy deck. Next perf target: the full-state clone the mid-decision
snapshot takes on replacement-heavy boards (63% of that profile).

Status 2026-11-16, slice 6 (X and non-mana costs, regeneration, damage
redirection, the CR 614.12 look-ahead, planeswalkers, emblems) done. X
(CR 107.3) is a casting decision (`Flow::ChoosingX`, options the player can
pay for) kept on the spell object; effect amounts are `Num::{N, X}`, and
Endless One's "enters with X counters" reads X from the spell as it
enters. `Cost` gained life, discard, and loyalty parts, and `CardDef` an
`additional` cost (CR 601.2h) whose sacrifice or discard is chosen after
targets and paid with the mana in `finish_cast`; activated abilities list
one `Choice::Activate` per way to pay (sacrifice × discard). Destruction
goes through the event pipeline (`Proposed::Destroy`), so regeneration
(CR 701.15) is a stored replacement used once that removes damage, taps,
and leaves combat, and "can't be regenerated" is a flag the shield does
not match; Pariah redirects damage by returning a new damage event, open
to further replacement (CR 616.2). The CR 614.12 look-ahead runs the layer
pass with the entering permanent included (`layers::compute_entering`), so
Phantom Tiger under Humility enters without counters and a Guildgate
under Blood Moon enters untapped. Planeswalkers (CR 306) have loyalty as a
counter kind put on them as they enter, removed by damage (CR 120.3c), and
checked by CR 704.5i; loyalty abilities are activated abilities with a
`loyalty` cost, sorcery speed and once per planeswalker per turn (CR
606.3), needing enough loyalty for −N (CR 606.5); "any target" and
"player or planeswalker" include them (CR 115.4); when the defender
controls one, each attacker chooses what it attacks (CR 508.1a,
`Flow::AttackTargets`), and an attacker whose planeswalker left deals no
combat damage. Emblems (CR 114) are command-zone objects the layer pass
takes as static sources and never as affected objects. Twenty definitions
join the pool (Blaze, Mind Spring, Endless One, Bone Splinters, Tormenting
Voice, Ravenous Bloodseeker, Souldrinker, Vampire Hexmage, Drudge
Skeletons, Regenerate, Wrath of God, Pariah, Ajani Goldmane, Jace Beleren,
Elspeth Knight-Errant, Garruk Wildspeaker, Hero's Downfall, Beast and
Avatar tokens, Elspeth's emblem), bringing it to 138. The scenario DSL
gained `cast ... x N`, `sacrificing`/`discarding` on casts and
activations, `attack ... at`, `activate ... ability N`, and `loyalty`
counters. Fifty-nine scenarios in `costs.scn`, `regen.scn`,
`lookahead.scn`, and `planeswalkers.scn` cover Bone Splinters' sacrifice
triggering Blood Artist before the spell resolves, paying exactly all your
life, a regenerated attacker leaving combat, Wrath ignoring a shield,
redirected damage then prevented, the Blood Moon Guildgate example,
Elspeth dying to her own ultimate while the emblem lives on, and a
planeswalker's attacker dealing no damage once it is gone. Authoring found
no rules bugs; fuzzing with the debug cache check found one: Ajani's
Avatar, whose power is its controller's life, went stale because life
changes do not bump `cversion` (fixed with a counted `LIFE_CDA` flag so
they do while such a CDA is in play). Fuzz: 51,000 release games on the slice 6 deck against itself, mono-red,
copy, replacement, and activated, with random and greedy agents, and 1,500
debug-assert games, 0 panics; records with attack-target choices replay
identically. Perf (§8): 2–4% on most boards, diffuse, after the one real
cost (an 8-byte growth of `GameObject`) was found by the size test and
removed. Not yet: battles,
X in activated-ability costs, "target player discards" (a decision for
another player mid-resolution), triggered abilities on emblems, and
distinct-target enforcement for multi-target spells (CR 115.3).

Status 2026-11-16, slice 5 (copy effects, characteristic-defining
abilities, delayed triggers, modal spells, "you may") done. Copy effects
(CR 707) are not effects: `GameObject::def` is the definition whose
copiable values the object has, next to `card`, the physical card, and an
optional `except` definition whose `copy_except` (subtypes, keywords,
abilities) joins the copiable values (CR 707.9b) so a Clone copying a
Phantasmal Image is an Illusion that sacrifices itself when targeted.
Everything that reads a name, abilities, or hot-path flags reads `def`,
so a copy triggers, counts toward the per-kind source counts, and meets
the legend rule as what it copies; `move_object_now` keeps `(def,
except)` only when the object enters the battlefield and resets them
otherwise. "Enters as a copy" is chosen through the mid-event mechanism
before the permanent is put onto the battlefield; token copies and
"becomes a copy" (Cryptoplasm) adjust the source counts in place. CDAs
(CR 604.3) are static abilities with timestamp 0 in their layer (CR
613.2): `CdaPt` in layer 7a counts against the working characteristics
after layer 6 (Urborg's Swamps count for Nightmare), `CdaColors` in layer
5 (Transguild Courier stays all colors under Humility); tokens in
graveyards are not cards for Tarmogoyf and Lhurgoyf. Delayed triggered
abilities (CR 603.7) are a list on the state carrying their source's
definition, fired once from `detect_triggers` at the step they name, with
`Recipient::SameObject` meaning the object only while it is the same one
(CR 400.7) and `CreatedObject` the new object an exile or return made.
Modal spells (CR 700.2) choose a mode in a casting state before targets
(CR 601.2b); `GameObject::mode` selects the mode's targets and effects.
"You may" is `Effect::YouMay`, asked mid-resolution; the snapshot gate
judges abilities by themselves (`CardPool::ability_asks`), since a copy's
exception can ask when its definition cannot, and also snapshots when a
spell or ability about to resolve could put a permanent with its own
"enters with" replacement onto the battlefield while another replacement
effect is in play (Otherworldly Journey returning Phantom Tiger under
Hardened Scales). Derived characteristics are cached on `cversion`, a
counter bumped only by changes that can alter them (§4.3), with a
sampled from-scratch comparison in debug builds. Fourteen cards join the
pool (Clone, Phantasmal Image, Cryptoplasm, Kiki-Jiki, Mirror Breaker,
Tarmogoyf, Lhurgoyf, Nightmare, Transguild Courier, Flickerwisp,
Otherworldly Journey, Abrade, Boros Charm, Grixis Charm, Gravedigger),
bringing it to 118. The scenario DSL gained `cast ... mode N`,
`accept`/`decline`, `copy`, and `oldest`/`newest` object references with
a graveyard fallback. The 66-scenario suite in `scenarios/m2/copy.scn`
covers a Clone copying a Phantasmal Image and inheriting its exceptions,
Lightning Bolt saving a Tarmogoyf by landing in the graveyard, Kiki-Jiki
tokens that died before the end step, Flickerwisp returning a token that
is gone, Nightmare dying when its last Swamp leaves, modes with no legal
targets, and Boros Charm's indestructible mode in response to Murder.
Authoring found one rules bug (the mid-choice gate judged a copy's
trigger by the copied card) and the debug cache check found a missed
bump (Aura attachment) and a count that changed without one (a token
leaving a graveyard). Fuzzing the new deck found two more: the
state-based loop never ended once an indestructible creature had lethal
damage (CR 702.12b; Boros Charm made it reachable), and the gate above
did not anticipate a delayed trigger returning a permanent: Phantom
Tiger's own entering replacement meeting Hardened Scales, and a flickered
Kiki-Jiki meeting the one cast in the meantime. Fuzz: 32,000 release games on the new deck against itself, mono-red,
activated, triggers, and replacement, with random and greedy agents, and
400 debug-assert games with the sampled cache check, 0 panics; records
with copy choices replay identically. Perf (§8): the characteristics cache
keyed on `cversion` lifts every board with effects by 16–82%, and the
effect-free board is within noise of slice 4 after a 14% regression from
a slow path inside the hot accessor was found and removed. Not yet (at
the time): planeswalkers, emblems, costs other than mana/{T}/sacrifice, X
costs, the CR 614.12 look-ahead, regeneration, and damage redirection,
all done in slice 6.

Status 2026-11-15, slice 4 (activated abilities, tokens, Equipment,
control changes, the legend rule) done. Activated abilities (CR 602) are
`AbilityDef::Activated` with a `Cost` of mana, {T}, and an optional
sacrifice ("sacrifice ~" or "sacrifice a Goblin", the latter offered once
per permanent that could be sacrificed, so the option list stays
exhaustive); they are announced like spells, choose targets through the
same `Flow::Casting`, then pay costs (source tapped first so it is not
also tapped for mana, sacrifices last) and resolve through the same path
as triggered abilities. A creature's {T} abilities and mana abilities
respect summoning sickness (CR 302.6). Tokens (CR 111) are definitions
flagged `token`, created in the command zone and put onto the battlefield
through the pipeline so "enters" effects and triggers see them; the
existing SBA removes them from other zones. Equipment (CR 301.5) reuses
the attachment machinery of Auras with equip as a sorcery-speed activated
ability and CR 704.5n unattaching instead of 704.5m's graveyard. Control
(layer 2) is written into `GameObject::controller` by `sync_control`
(§4.4) from "until end of turn" effects (Act of Treason) and static
abilities (Control Magic), with Glorious Anthem following the stolen
creature. The legend rule (CR 704.5j) is a state-based action that asks
its question through the mid-event mechanism of slice 3; its options carry
the card and the permanent's rank by age because the state shown is the
one before the action. Twenty-two cards and four token definitions join
the pool (Prodigal Sorcerer, Llanowar Elves, Mogg Fanatic, Bottle Gnomes,
Shivan Dragon, Rod of Ruin, Pestilence, Royal Assassin, Icy Manipulator,
Seal of Fire, Mishra's Factory, Bonesplitter, Loxodon Warhammer, Raise
the Alarm, Dragon Fodder, Siege-Gang Commander, Bitterblossom, Young
Pyromancer, Disenchant, Act of Treason, Control Magic, Isamaru, Hound of
Konda), bringing it to 104. The scenario DSL gained `activate` (with
`sacrificing` and `targeting`) and `keep oldest|newest`. The 56-scenario
suite in `scenarios/m2/activated.scn` covers Mogg Fanatic dying to its
own cost before Blood Artist's trigger, an ability resolving after its
source is Shocked away, Pestilence's intervening "if" at the end step, a
Mishra's Factory animated under Humility being 2/2 (and 1/1 when Humility
comes later), Equipment surviving its creature, tokens vanishing from the
graveyard but still seen by dies triggers, a stolen legend meeting the
legend rule from both choices, and Disenchant on Control Magic giving the
creature back. Authoring found no bugs in the slice's rules code; it
found that the canonical payment plan tapped Mishra's Factory to pay for
its own animation (plain lands are now tapped first), that the
legend-rule choice named objects that did not exist in the rolled-back
state (fixed by making the choice self-describing), and a latent greedy-
agent crash on attackers that had left the battlefield (CR 506.4 removal
from combat is now done by the engine too). Fuzz: 11,000 release games
on the new deck with random and greedy agents and 500 debug-assert games,
0 panics; records with legend-rule decisions replay identically. Not yet
(at the time): copy effects, CDAs, delayed triggers, "you may" and modal
choices, and the characteristics cache keyed on its own counter, all done
in slice 5; planeswalkers, emblems, costs other than mana/{T}/sacrifice,
X costs, and the CR 614.12 look-ahead remain.

Status 2026-11-15, slice 3 (replacement and prevention effects) done:
`mtg-core/src/engine/replace.rs` implements CR 614–616 as stage 1 of the
pipeline described in §4.5: damage, life gain, draws, zone changes, and
counters are proposed, modified by every applicable effect once each, and
performed, with the affected player choosing the order when several apply
(a new `ChooseReplacement` decision, served by rolling the action back to
a snapshot and replaying it with the answer). Also in this slice: "can't"
effects (no life gain, damage can't be prevented), "enters tapped" and
"enters with counters", damage shields, and winning the game as an
effect. Seventeen cards join the pool (Fog, Holy Day, Safe Passage,
Mending Hands, Bandage, Skullcrack, Furnace of Rath, Gratuitous Violence,
Phantom Tiger, Sulfuric Vortex, Rhox Faithmender, Laboratory Maniac,
Thought Reflection, Rest in Peace, Leyline of the Void, Hardened Scales,
Azorius Guildgate), bringing it to 78. The scenario DSL gained `apply
"Source"` for the CR 616.1 choice and `with N +1/+1` in setup. The
47-scenario suite in `scenarios/m2/replacement.scn` covers shields used
up across several damage events, Phantom Tiger losing a counter to
unpreventable Skullcrack damage, two Furnaces quadrupling, the shield-
before-or-after-doubling choice from both sides, Thought Reflection
feeding Laboratory Maniac, Rest in Peace stopping dies triggers and
Rancor's return, and Hardened Scales on counters a creature enters with.
Authoring found one engine bug (both halves of "draw two instead" were
checked for replacement before either happened, against CR 614.11a; fixed)
and two mis-authored tests. Fuzz: 14,000 games with the replacement deck
(random and greedy agents), 0 panics; records of games with mid-event
decisions replay identically. Not yet: the CR 614.12 look-ahead for
"enters" effects under Humility, redirection and regeneration, CR 615.7's
choice of which simultaneous damage a shield prevents, activated
abilities, delayed triggers, copy effects, CDAs, control-changing
effects, tokens, planeswalkers, emblems, the legend rule (slice 4, see
above).

Status 2026-11-15, slice 2 (triggered abilities and the event pipeline)
done: `mtg-core/src/engine/triggers.rs` implements CR 603 as described in
§4.5: detection from every emitted event, look-back for leaves-the-
battlefield triggers including simultaneous deaths, APNAP placement with
an ordering decision, targets chosen on placement with CR 603.3d removal,
intervening "if" clauses, and ability objects on the stack that resolve
like spells. Seventeen trigger cards join the pool (Elvish Visionary, Wall
of Omens, Soul Warden, Flametongue Kavu, Ajani's Pridemate, Blood Artist,
Zulaport Cutthroat, Festering Goblin, Perilous Myr, Rancor, Phyrexian
Arena, Juzám Djinn, Howling Mine, Benalish Cavalry, Hellrider, Markov
Blademaster, Monastery Swiftspear), prowess is the first keyword
implemented as a triggered ability, and `Recipient` replaces positional
targets in `Effect` so abilities can act on "you", "each opponent", their
source, or the triggering object or player. The scenario DSL gained
`trigger "Source" [targeting ...]` for ordering and targeting triggers and
`assert stack top`. The 41-scenario suite in `scenarios/m2/triggers.scn`
covers Soul Warden + Pridemate chains, Flametongue Kavu forced to target
itself, Blood Artist seeing a simultaneous death, Rancor returning after
SBA 704.5m, Howling Mine's intervening if, prowess stack order against a
response, and APNAP ordering. Authoring found two mis-authored tests
(Boggart Brute has menace; a response resolves before a prowess trigger)
and no engine bugs. Fuzz: 9,000 trigger-deck games, 0 panics. Not yet:
replacement and prevention effects (stage 1 of §4.5), activated abilities,
delayed triggers, copy effects, CDAs, control-changing effects, tokens,
planeswalkers, emblems, the legend rule (slice 3).

Status 2026-11-14, slice 1 (layers) done: `mtg-core/src/layers.rs`
implements CR 613 over all battlefield objects at once: effects apply in
timestamp order within a layer, with CR 613.8 dependency ordering (an
effect whose affected set would change, or whose ability would be removed,
applies after the effect that would do that; cycles fall back to
timestamps), CR 613.6 persistence (an effect that has started applying
keeps applying in later layers even if its ability is removed), and CR
305.7 for "is a Mountain" effects. Static abilities are derived from the
battlefield each computation (CR 611.3); spell effects are stored with
locked-in affected sets (CR 611.2c). Characteristics are cached per state
version and are a `Copy` type (fixed-capacity subtype and ability lists)
because the engine queries them dozens of times per decision. Also in
this slice: Auras (enchant targeting, attachment, CR 303.4f, CR 704.5m),
+1/+1 and -1/-1 counters (layer 7c, CR 704.5q), color indicators, land
subtypes as the source of intrinsic mana abilities (CR 305.6), and
sixteen cards for the puzzle suite (Humility, Opalescence, Blood Moon,
Urborg, Darksteel Citadel, Dryad Arbor, Glorious Anthem, Crusade,
Muraganda Petroglyphs, Levitation, Holy/Unholy Strength, Dead Weight,
Flight, Dromoka's Gift). The 37-scenario suite in `scenarios/m2/`
includes Humility+Opalescence in both timestamp orders and Blood Moon
beating Urborg in both orders. Not yet: triggered abilities and the event
pipeline (slice 2), replacement and prevention effects, copy effects,
CDAs, control-changing effects, tokens, planeswalkers, emblems, the
legend rule (slice 3). Performance: effect-free boards cost 5% against
M1; boards with static abilities run at 770–1,130 games/s (see §8), to be
optimized with incremental recomputation once slice 2 settles the shape.
- CR 613 layers with dependency, CDAs, copy effects (CR 707), LKI,
  replacement and prevention (CR 614–616), triggered abilities including
  ordering and "look back" (CR 603), delayed triggers, auras and
  equipment, counters, planeswalkers, tokens, emblems.
- Accept: layer puzzle suite passes; CR 603, 608, 611–616, 700–707
  examples pass; fuzz clean.

**M3: DSL, authoring pipeline, scenario DSL (6–8 weeks; started 2026-11-17)**

Status 2026-11-17, slice 1 (the card DSL) done: `mtg-dsl` parses,
compiles, prints Oracle text for, and formats card files; the 138
definitions of the M2 pool (131 cards, 6 tokens, 1 emblem) moved from
2,150 lines of Rust in `mtg-core` to one file each under `cards/`, and
`mtg_dsl::builtin_pool()` replaced `m1_pool()` everywhere (the engine
crate keeps a six-card pool for its own unit tests). The round-trip gate
(§6.3) is in `mtg-cards`; it immediately found four cards whose
implementation did not match their Oracle text, which were replaced by
cards the engine does implement faithfully (Reckless Pangolin and Giant
Badger for Benalish Cavalry, Dragonscale Boon for Dromoka's Gift) or
corrected (Healing Salve is modal; Mishra's Factory gained its pump
ability and the engine a subtype-restricted creature target). Syntax
decided (§6.2, open question 4). Scenario corpus: 378 pass, with the
counter and life-gain numbers of a dozen scenarios updated for the
corrected cards. Not yet: gates 2–4, `mtg-cards author`, the XMage
converter, any Pauper coverage beyond the M2 pool.
Status 2026-11-17, slice 2 (template authoring and fuzz participation)
done: `mtg-cards author --format pauper --write` authored 2,090 Pauper
cards from the Scryfall snapshot through sentence templates, each
verified against its Oracle text before being written (§6.4); the corpus
is 2,229 definitions in `cards/<letter>/` (20.1% of Pauper, 8.1% of
Modern), and the gate matches 2,221 of 2,221 cards with one declared
exception. Random two-color decks drawn from the whole pool
(`mtg sim --deck1 random:<seed>`, `mtg deck random:<seed>`) are the fuzz
input; the first run found two things. A legendary land (Urborg) played
onto a battlefield holding its namesake hit the legend rule with no
snapshot: the mid-decision gate only looked at the stack, and a land play
has none; it now looks at the action too (a legendary land or one with
an "enters" replacement when a namesake or replacement source is out).
And Hyalopterous Lemure's "{0}: ..." let the random agent's stack of
activations random-walk for thousands of steps, each adding an
until-end-of-turn effect, so one game could run for minutes: not a rules
bug, a baseline-agent policy, fixed with an `ActivationGuard` in
`mtg-agents` that stops repeating a free ability after four uses a turn.
`mtg-cards coverage --scenarios` measures gate 2 (§6.3). The fuzz and
A/B scripts moved into `scripts/`. Fuzz: 15,000 random-deck games (half
greedy) and 36,000 stress-deck games, 0 panics, 0 unfinished. The
acceptance target of 500 Pauper cards through the gate is met for gate 1
and gate 4; the XMage converter and model-driven authoring remain.

Status 2026-11-17, M4 slice 2 (MCP) done: `mtg-mcp` on rmcp 3.11 with
the nine tools of §9.2 over stdio and streamable HTTP, a game table,
seat-filtered text and events, and option resolution by number, text,
or label; an in-process client plays a full game in the test, and
`.mcp.json` registers the server for Claude Code. `mtg-agents::by_name`
now returns `Box<dyn Agent + Send>` so an agent can sit behind a mutex.

Status 2026-11-17, M4 slice 1 (Python) done: `mtg-py` with the thin
`Engine` binding, `simulate_batch` on rayon with the GIL released, the
tensor encoders, and the PettingZoo AEC environment (§9.1); 12 Python
tests including PettingZoo's `api_test`. The env runs a masked random
policy at 40,000 steps/s per process and 132,000 steps/s across 16
processes, so the ≥10k steps/s acceptance figure is met before any RL
code exists; the masked-PPO run itself is slice 4. `mtg-core` gained
`determinize` and the labelled text view.

Status 2026-11-17, slice 7 (group recipients, then the second model
batch) done: `each <filter>` / `all <filter>` recipients, `without
<keyword>` filters, the imperative "Mill N cards", and the "All Sliver
creatures" static subject are in the engine, DSL, and templates (§6.4);
7 scenarios in `scenarios/m3/groups.scn`. Template authoring added 138
cards and the second model-author batch 45 (of 249). Corpus 4,124 cards
(2,959 Pauper, 27.6%; 3,083 Modern, 14.2%; gate 4,116 of 4,116 matching,
1 allowed), 147 hand-written and 112 model-authored, all
scenario-tested. Scenarios: 821 pass (1 skipped). The reasons the model
gives for the rest put filters over attacking and blocking creatures
next.

Status 2026-11-17, slice 6 (the first run's backlog, then the model
author at scale) done: kicker, flashback, landwalk, attacking and
blocking targets, base power and toughness until end of turn, and
filtered discard costs are in the engine, the DSL, the scenario runner,
and the templates (§6.4); 17 scenarios in `scenarios/m3/primitives-2.scn`
cover them with real cards. Template authoring added 170 cards and the
first 240-card model-author batch 67 more (28% accepted, with the
unsupported reasons recorded as the next backlog: group recipients
first). Corpus 3,929 cards (2,848 Pauper, 26.6%; 2,949 Modern, 13.6%;
gate 3,921 of 3,921 matching, 1 allowed), 140 hand-written and 68
model-authored, all scenario-tested. Scenarios: 670 pass (410
hand-written and primitive, 231 model-authored, 29 converted from XMage
of which 28 pass; 1 skipped for adjudication). Fuzz: 36,000 stress-deck
and 10,000 random-deck games, 0 panics, 0 unfinished.

Status 2026-11-17, slice 5 (model-driven authoring) built: `mtg-cards
model-author` with the prompt pack, the gate-and-feedback loop, and
verified-on-acceptance writing (§6.4). Not yet run against the API (a
spend the user makes); the `--prompt-only` pack is checked. With this,
every part of the M3 plan exists; what remains is running it to grow
coverage, which is where the 500-card and 1,000-scenario targets now
stand or fall.

Status 2026-11-17, slice 4 (XMage converter) done, with the honest
result recorded in §7.1: the converter and its `wait`/`targeting auto`
runner support are in and 27 of 28 convertible tests pass as
`scenarios/xmage/`, but 5,895 of XMage's 6,802 tests name cards outside
the corpus, so the 1,000-scenario target is a coverage problem. Authoring
every format's template-coverable cards added 958 (corpus 3,675 cards,
2,649 Pauper, 2,801 Modern; gate 3,674 of 3,674). Scenarios: 421 (393
hand-written and primitive, 28 converted; 1 skipped for adjudication).

Status 2026-11-17, slice 3 (engine primitives from the authoring
backlog) done: counterspells including "unless its controller pays",
scry and surveil, cycling, discard effects, fight, once-per-turn
activations, any-color mana, the four combat restrictions, and
filter-based targets (§6.4). The DSL gained `then` joins, `cycling`,
`mana any`, `once`, `discard this`, restriction keywords, and filter
targets. The author's templates follow, and the corpus is 2,717 cards
(24.7% of Pauper, 9.9% of Modern), 2,716 matching Oracle text. 15 new
scenarios in `scenarios/m3/primitives.scn` cover each primitive with
real Pauper cards (393 pass); the runner learned to name a spell on the
stack as a target and to send a creature that must attack. Fixed on the
way: multi-paragraph spells authored as two `spell` statements ran only
their first paragraph (68 cards regenerated; the compiler now rejects
it).
- `mtg-dsl` with round-trip printer; `mtg-scenario` runner; XMage test
  converter; `mtg-cards author`.
- Accept: 500 Pauper cards supported through the gate; 1,000 ported
  XMage scenarios passing or adjudicated.

**M4: Python, MCP, and review (4–6 weeks)**
- `mtg-py` with PettingZoo AEC env and batch simulation; `mtg-mcp`
  stdio and HTTP; `mtg-cli`; `mtg-review` v1 with store, game list, and
  step-by-step playback with the event log and legal option list.
- Accept: a masked-PPO self-play run on a fixed Pauper matchup trains
  without env errors at ≥ 10k steps/s on 16 cores; Claude Code plays a
  complete game via MCP; that game is opened in the review UI and
  stepped through from a recorded `GameRecord`.

**M5: Coverage and hardening (ongoing)**
- Pauper to ≥ 95% supported; Standard snapshot; modal and multi-face
  cards; remaining CR 702 keywords; IS-MCTS baseline agent; published
  benchmark harness.

**M6: Public benchmark site (after M4; not scheduled)**
- `mtg-bench` (§9.5): LLM harness with cost and token logging, the
  puzzle track, rated games against the agent ladder, rating pipeline,
  leaderboard, and public read-only replays.
- A command-line prototype of the harness against the built-in decks
  can precede M4; its purpose is to measure cost per game and decisions
  per game, the main unknowns.
- Accept: at least five models rated on both tracks with published
  methodology; every rated game opens as a replay; a second run of the
  same model lands within the stated rating interval.

## 12. Open questions

1. **Flat clone vs. structural sharing.** Decided for flat clone and
   confirmed by M1 measurement (2.8 µs per clone mid-game, under the
   5 µs target). Revisit only if M2's effect rows change the picture.
2. **Attack/block declaration as sets vs. incremental.** Sets are
   cleaner for RL masking; combinatorial on wide boards. Proposed hybrid
   in §4.7; validate with the fuzzer on token decks.
3. **Mana payment granularity.** Canonical plans by default; is the
   fine-grained mode needed for correct play in Pauper? Probably rarely.
4. **DSL surface syntax.** Resolved 2026-11-17 (M3 slice 1): block
   per card, one statement per line, effects as a controlled English of
   verb phrases whose sentence structure the printer reproduces (§6.2).
   Chosen over Forge-like `Field:Value` for reviewability and over
   free-form English because every construct must compile to a closed
   primitive set; the 138-card port and the round-trip gate validated it.
5. **Round-trip strictness.** Exact Oracle match may be unachievable for
   older or oddly templated cards. Measure the hit rate on Pauper before
   deciding the allowlist policy.
6. **Observation encoding ownership.** Engine-side default vs.
   Python-side only. Default engine-side (fast, consistent), with a raw
   escape hatch.
7. **Commander and multiplayer.** APNAP ordering (CR 101.4) and
   multiplayer SBAs are designed in but untested until a later milestone.
8. **Licence choice.** Resolved: MIT (2026-10-29).
9. **Review UI frontend stack.** Resolved: TypeScript + SolidJS
   (2026-10-30). Whether to render the board as DOM or canvas is decided
   when the M4 playback view is built.
10. **Review tool hosting.** Local-only in v1. If games are to be shared
   with people outside the machine, it becomes a deployed service and
   needs a decision on access control and on where game records live.
   The benchmark site (§9.5) makes it a public read-only service.
11. **Benchmark rating method.** How many mirrored pairs give a stable
   Elo given Magic's variance, and whether model-vs-model games are
   affordable or the ladder of built-in agents has to carry the rating.
   Needs the cost-per-game measurement from the M6 prototype.
12. **Benchmark auto-pass policy.** Which priority passes are taken
   without asking the model. Too few and games cost too much; too many
   and the benchmark makes decisions the model should have made (holding
   up an instant, responding on the stack).
13. **Licensing footing for the site.** Whether the Fan Content Policy
   covers a rules engine and a benchmark built on it (§10), whether the
   README notice should change, and whether a name containing "MTG" is
   acceptable. Decide whether to ask Wizards for written permission
   before launch.

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
https://github.com/GregorStocks/mage-bench. ChessBench (models for the
benchmark site, §9.5): https://chessbench.org, https://chessbench.ai.
MageZero:
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
