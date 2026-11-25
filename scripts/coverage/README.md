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
   that means the engine, not the authoring, is the limit. That figure
   was for retries: the 253 Pauper cards no batch had tried still
   yielded 23% in the eighth wave, so check for never-tried cards
   before stopping a format. Retry batches (`RETRY` in `wave-lists.py`)
   of cards with old reports did not pay: 7 of 72 Pauper cards right
   after the eighth wave's merges, and 22 of 360 Standard cards in the
   ninth (6%, about 80k Sonnet tokens a card). What is left in a format
   at that point needs a rule of its own per card or small group; give
   those cards to feature agents. The template
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

## When a format's tail is one rule a card (thirteenth wave)

The skip ranking goes flat near the end of a format (after the twelfth
wave no reason blocked more than five Standard cards). What worked for
Standard at 82.6%: join the uncovered cards with their latest report
line (name, author's skip, what the card agent found missing), read
all of it, and group the cards by the kind of rule they lack (counts,
conditions on "it", filters that compare, dig forms, costs and mana,
players, ...). Twenty agents from such a partition added 341 Standard
cards where seventeen from the leftovers list had added 43. Each agent
still leaves more than half its list, with reasons.

- Give every agent the joined list and tell it the notes may be stale.
- Assign flag bits in the prompt (`DefFlags` is a u64, `marks` a u16).
- Name shared forms in the assignment list, not only areas: two pairs
  of agents built the same form, and two cards were written twice.
- A build tree is 8 to 10 GB: start as many agents as the disk holds
  and the rest as finished ones delete `target/`.
- Agents run `cargo test` in debug; the lead's runs must too (a debug
  assertion failed every agent's run for a fault the release runs
  never saw).
- `wordmerge.sh <file>` merges a conflicted file word by word: the
  long table lines of `SYNTAX.md` conflict in almost every merge and
  resolve cleanly that way.
- No perf agent ran and throughput fell (KNOWN-ISSUES section 4): run
  one every wave, or at least bench merge by merge.

## Card batches first for a format nobody has tried (fifteenth wave)

Before partitioning a format for feature agents, count how many of its
uncovered cards have a report line. Modern had 5,770 uncovered cards
and 513 reports: 32 Sonnet batches of 40 never-tried cards accepted
31% (10 to 19 a batch) at about a third of a feature agent's cost a
card, and their reports are the partition for what follows.

- At most 20 agents run at once: queue the rest and start one as each
  finishes.
- A fifth of the cards come back as `RENDER:` near-misses and a few
  percent as `MEANING:`. Start wording agents (about 55 lines each)
  and one bug-fix agent on those lines as soon as the first twenty
  batches are in; they yielded 25 to 34 cards each.
- Integration: build, gate, scenarios and `cargo test` on the main
  checkout before committing. Expect leftovers: scenario files for
  rejected cards, a card that no longer gates, a card that fails the
  format round trip (list them all at once with a throwaway test, not
  one failure a run), a file name that breaks the slug rule (accents
  are kept).
- The template agent, working in a worktree, writes some of the same
  cards: keep the main checkout's file at the merge.

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
