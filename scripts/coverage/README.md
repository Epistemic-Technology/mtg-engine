# Coverage waves

How card coverage is raised (design.md §11, M5). The acceptance bar never
moves: a card counts only when the gate says `Match` and its scenarios
pass. Everything here is about reaching that at less cost.

## Files

- `feature-brief.md`: the brief for a feature agent (one rules feature,
  in its own worktree, on a branch).
- `card-brief.md`: the brief for a card agent (a list of card names,
  written against frozen binaries; no Rust changes).
- `census/`: scripts that bucket uncovered cards by the feature they are
  missing. Set `CENSUS_ROOT` to a scratch directory and run `build.py`
  from `$CENSUS_ROOT/census/work`. The inputs (`pauper-oracle.tsv`,
  `uncovered-oracle.tsv`, `backlog.tsv` in the root; `meta.json`,
  `kwcov.json`, `subtypes.json`, `corpus.json`, `newly_covered.json` in
  `census/work`) are rebuilt per census from the snapshot and `mtg-cards
  author --verbose`; the scripts were written for Pauper and still name
  it. Not yet done: one census over the union of the target pools with a
  count per format.

## One wave

1. **Templates first.** Run `mtg-cards author --format <f> --write` for
   every target format (pauper, standard, modern). It needs no model, and
   each new primitive makes more cards expressible in all pools. On
   2026-11-20 a dry run found 16 Pauper, 196 Standard, and 1,421 Modern
   cards waiting. These cards pass the Oracle round trip but have no
   scenarios.
2. **Census over all target pools.** Rank buckets by cards unlocked
   across Pauper, Standard, and Modern together, not by one format.
   Standard's buckets are several times fatter than Pauper's tail, and
   most of them count for Modern too.
3. **Partition before launching.** Assign each primitive to exactly one
   agent. Where two buckets need the same primitive, land it first (or
   give both buckets to one agent). Put the full assignment list in every
   agent's instructions so nobody builds a second version.
4. **Launch with an explicit model per agent.** Subagents inherit the
   session model unless told otherwise. Feature agents run on Opus; card
   agents run on Sonnet. Two matched trials: on 2026-11-20 (40 Standard
   cards each) Opus accepted 18 for 192k tokens and Sonnet 11 for 200k;
   on 2026-11-22 (two batches each, a random split of 138 never-tried
   Standard cards) Opus accepted 26 of 70 for 415k and Sonnet 24 of 68
   for 374k, with scenarios of similar depth. A Sonnet token costs about
   half an Opus token. Tell each agent which `Co-Authored-By` line to
   use. About six feature agents is enough: launch one for cards only
   when the lists show about 30 or more it could unlock.
5. **Merge all branches, then verify once.** Merge each branch on main
   in turn with a release build, the gate and the scenario corpus
   (about two minutes); run clippy, `cargo test --workspace --release`
   and the debug-build scenarios once, after the last merge. Feature
   agents do not edit `KNOWN-ISSUES.md` (see `feature-brief.md`): fold
   their notes files in after merging. Run the random-deck fuzz and the
   bench once per wave, with no agents running. Run the fuzz with a
   debug build as well (`cargo build -p mtg-cli`, then
   `scripts/fuzz-random.sh target/debug/mtg 150`): only a debug build
   checks the characteristics cache against a fresh pass, the board
   counts against the battlefield, and re-entry into the layer pass.
6. **Card batches.** `wave-lists.py` builds them from never-tried cards
   of one format, leaving out cards a feature agent owns and cards whose
   Oracle text names a feature known to be missing. Stop batches for a
   format when yield falls below about 15% (Pauper reached 14.5% in the
   sixth wave, at three times the tokens per card of any other source):
   that means the engine, not the authoring, is the limit. The template
   author is the cheapest source of all; give one feature agent the
   template author itself every wave.
7. **Clean up.** Delete `target/` in finished worktrees before the next
   wave, and kill wait loops finished agents left behind (`until !
   pgrep -f "<command>"` matches its own shell and never ends).
8. **Keep `KNOWN-ISSUES.md` current.** It is the open-issue list. Read
   it when partitioning (wrong behaviour in accepted cards goes to an
   agent before new features do), and after merging delete the entries
   the agents' notes list as fixed and add the engine bugs,
   wrong-meaning forms, and missing features the notes and reports
   name.

## Quick cross-pool backlog (no census inputs needed)

    for f in pauper standard modern; do
      mtg-cards author --format $f --verbose > $DIR/skips-$f.txt 2>&1
    done
    python3 scripts/coverage/rank-skips.py $DIR    # backlog-ranked.tsv
    python3 scripts/coverage/wave-lists.py $DIR $REPORTS   # lists/, batches/
    python3 scripts/coverage/wave-prompts.py $DIR 8       # prompt-<agent>.txt

`rank-skips.py` ranks the template author's skip reasons by cards
blocked across the three pools. The author reports only the first
reason per card, so a count is an upper bound on what one feature
unlocks. `wave-lists.py <dir> <reports-dir>` holds the assignment of
skip reasons and report patterns to agents (edit the tables at its top
per wave) and writes one card list per agent and the card batches.
`wave-prompts.py <dir> <suffix>` writes the feature agents' prompts.
