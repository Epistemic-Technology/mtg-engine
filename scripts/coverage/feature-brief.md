# Feature-agent brief

You are adding one rules feature to a Rust Magic: The Gathering rules
engine. You work in your own git worktree (your current directory); the
lead engineer merges your branch afterwards, while others change `main`
in parallel. Keep your change focused on the feature you were given.

## The codebase

- `crates/mtg-core`: the rules engine. `src/card.rs` (card definitions:
  `Effect`, `AbilityDef`, `TriggerSpec`, `StaticCondition`, `Cost`,
  `ObjFilter`, `Keyword`), `src/state.rs` (`GameState`, `GameObject`),
  `src/engine/` (`casting.rs`: priority options, casting, costs, effect
  resolution in `apply_effect`; `triggers.rs`; `combat.rs`; `layers.rs`
  for continuous effects; `replace.rs`; `objects.rs`; `mod.rs`: the
  decision flow).
- `crates/mtg-dsl`: the card language. `src/ast.rs`, `src/parse.rs`,
  `src/compile.rs` (AST to core), `src/decompile.rs` (core to AST),
  `src/format.rs` (AST to DSL text; must round-trip through parse),
  `src/render.rs` (AST to Oracle text; must match Oracle exactly).
  `SYNTAX.md` documents the language: update it for what you add.
- `crates/mtg-cards`: `src/author.rs` (templates that turn Oracle text
  into DSL; add a template if the phrasing is regular), the gate.
- `crates/mtg-scenario`: rules tests (`SYNTAX.md`); scenarios live in
  `scenarios/`.
- `cards/`: the card corpus (`cards/<x>/<slug>.card`), embedded at build
  time.

Do not read `design.md` (it is 2,500 lines of history). To see how a
feature is threaded through every layer (core type, engine behavior, AST,
parse, compile, decompile, format, render, SYNTAX.md, scenarios), read
the diff of one recent feature commit close to yours (`git log --oneline
-30`, then `git show <hash>`), or grep for `Effect::TapOrUntap`,
`Effect::BecomeColor`, `Trigger::Leaves`, or `CardDef::target_cost`.
Cite rules (CR numbers) in comments the way the code does.

Your worktree's `data/` holds only the tracked files (`cr/`,
`sources.lock`): symlink the gitignored subdirectories (`games`,
`mtgjson`, `scryfall`, `xmage`) from the main checkout's `data/` into it
before running anything that reads the snapshot.

Keep scratch files in a directory of your own (`mktemp -d`); a shared
scratch directory is written by every agent, and same-named files
overwrite each other.

Other agents are working in parallel. Your instructions list what each
of them is building. If you find you need a primitive on that list, do
not build your own version: skip the cards that need it and say so in
your reply.

## Requirements

1. Implement the feature in the engine and the DSL, rendering the exact
   Oracle wording of the cards listed for you.
2. Write the listed cards whose Oracle text your feature completes
   (header lines from `cargo run -q --release -p mtg-cards --
   model-author --prompt-only "Name"`), each passing
   `cargo run -q --release -p mtg-cards -- gate --diff "Name"` (`Match`).
   Skip any that need something else too.
3. Add scenarios in `scenarios/m5/<feature>.scn` exercising the rules
   (positive and negative cases; `illegal` lines check refusals).
4. All of this must pass before you finish, each wrapped in `timeout`:
   `cargo fmt --all`, `cargo clippy -q --workspace` (no warnings),
   `timeout 1800 cargo test -q --workspace --no-fail-fast`, and
   `cargo run -q --release -p mtg-cli -- scenario` (the whole scenario
   corpus).
5. Performance matters: keep hot paths (priority options, characteristics,
   trigger detection) free of new per-call work; gate new checks on a
   cheap flag or counter the way existing features do.
6. A question asked in the middle of an event needs a snapshot: the
   engine saves the state before an action only when
   `GameState::may_need_mid_decision` says the action could ask (which
   replacement applies first, which legend to keep, whether to dredge,
   a "you may" as an effect resolves). If your feature can ask a
   player something while an action is being applied, or makes an
   existing question arise in a new place (a static that makes
   permanents legendary, a replacement created as a spell resolves, a
   card moved and then drawn over by one effect), teach that function
   about it, with the check gated on a count so games without the
   feature pay nothing. A debug build asserts on a missed one
   ("mid-event decision without a snapshot"); a release build does
   not, so run your scenarios with `target/debug/mtg scenario` too.
   Likewise `cbump()` when anything a static ability reads changes
   (a debug build asserts on a stale characteristics cache).
7. Commit on your branch as you go (one commit per mechanic is fine),
   each with a message in the repository's style (a summary line, then
   what and why), ending with the `Co-Authored-By` line given in your
   instructions. Write the message to a file first and use `git commit
   -F <file>`. Do not push, do not touch `main`, never force anything.
   When everything is committed, delete your worktree's `target/`.
8. Reply with: the branch name, the commit hash, what you implemented,
   the cards you added, and anything you left out or are unsure of.

## Known issues

`KNOWN-ISSUES.md` at the top of the repository is the open-issue list.
Read sections 1 and 2 before you start: they name forms that pass the
gate and mean the wrong thing. Do not edit that file (every branch
editing it makes every merge conflict). Keep a notes file instead, at
the path your instructions give, with three headings: `Fixed` (the
title of each entry you fixed, one per line), `New` (each engine bug or
wrong-meaning form you found and did not fix, written as the entry
should read), and `Missing` (features your cards still need). The lead
folds the notes into `KNOWN-ISSUES.md` after merging.
