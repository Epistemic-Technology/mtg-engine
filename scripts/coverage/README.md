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
   session model unless told otherwise. Feature, card, and retry agents
   all run on Opus. A matched trial on 2026-11-20 (40 uncovered Standard
   cards each) had Opus accept 18 for 192k tokens and Sonnet 11 for
   200k; an Opus token costs twice a Sonnet token, so cost per accepted
   card is about equal and Opus leaves fewer cards to retry. Tell each agent
   which `Co-Authored-By` line to use.
5. **Merge, then verify behaviour.** After each merge check that both
   sides' scenarios still exercise their feature. Run the random-deck
   fuzz and the bench once per wave, with no agents running. Run the
   fuzz with a debug build as well (`cargo build -p mtg-cli`, then
   `scripts/fuzz-random.sh target/debug/mtg 150`): only a debug build
   checks the characteristics cache against a fresh pass, the board
   counts against the battlefield, and re-entry into the layer pass.
6. **Retry.** Give card agents only the cards whose reported missing
   feature has landed. Stop fresh card batches when yield falls to about
   5%; that means the engine, not the authoring, is the limit.
7. **Clean up.** Delete `target/` in finished worktrees before the next
   wave, and kill wait loops finished agents left behind (`until !
   pgrep -f "<command>"` matches its own shell and never ends).
8. **Keep `KNOWN-ISSUES.md` current.** It is the open-issue list. Read
   it when partitioning (wrong behaviour in accepted cards goes to an
   agent before new features do), tell agents to delete the entries
   they fix, and after merging add the engine bugs, wrong-meaning
   forms, and missing features the reports name.

## Quick cross-pool backlog (no census inputs needed)

    for f in pauper standard modern; do
      mtg-cards author --format $f --verbose > $DIR/skips-$f.txt 2>&1
    done
    python3 scripts/coverage/rank-skips.py $DIR    # backlog-ranked.tsv
    python3 scripts/coverage/wave-lists.py $DIR    # wave1/<agent>.txt

`rank-skips.py` ranks the template author's skip reasons by cards
blocked across the three pools. The author reports only the first
reason per card, so a count is an upper bound on what one feature
unlocks. `wave-lists.py` holds the assignment of reasons to agents
(edit the table at its top per wave) and writes one card list per
agent, plus a catch-up list of every unassigned reason shared by three
or more cards.
