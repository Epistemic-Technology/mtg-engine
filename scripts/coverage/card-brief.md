# Card-authoring brief

You are writing Magic: The Gathering cards for a Rust rules engine at
`/home/mikethicke/projects/et/mtg-engine` (your working directory). Cards
are written in a small closed DSL; the engine renders each card file back
to Oracle text, and a card counts only if that rendering matches the
card's Oracle text exactly. Each card also gets rules tests (scenarios)
that must pass.

Paths below use `$S` for the scratch directory named in your
instructions (the lead fills it in per session). Shell state does not persist between commands, so write the full path or
re-set the variable in each command.

## Rules

- Only CREATE files: `cards/<x>/<slug>.card` and
  `scenarios/agents/<slug>.scn`, plus appending to your report file.
  Never edit or delete any existing file, never touch Rust code, docs, or
  git, and never run `cargo`. Another engineer is changing the engine in
  the same tree at the same time.
- If `cards/<x>/<slug>.card` already exists, skip that card (report it as
  `exists`).
- Never use `mismatch` / `oracle_mismatch` lines to force a pass.
- Draft privately. Write each card first to your draft directory
  `$S/agents/drafts-<batch>/<slug>.card` (create it), and run every tool
  with `MTG_CARDS_DIR=$S/agents/drafts-<batch>:cards`: cards in the draft
  directory take precedence, and unparsable files anywhere are skipped
  with a warning, so a half-written card never blocks anyone. Only when
  a card gates `Match` and its scenarios pass, copy it to
  `cards/<x>/<slug>.card` and delete the draft. Never put an unfinished
  card in `cards/`.
- Use only the frozen binaries in `$S/bin/` (below, `MTG_CARDS_DIR=cards`
  stands for the two-directory value above):
  - `MTG_CARDS_DIR=cards $S/bin/mtg-cards model-author --prompt-only "Card Name"`
    prints the card's exact header lines (name, oracle_id, cost, type,
    pt...), its Oracle text, rulings, and several similar corpus cards
    in the DSL. Start every card with this. (It makes no API call.)
  - `MTG_CARDS_DIR=cards $S/bin/mtg-cards gate --diff "Card Name"`
    checks your card file: `Match` means accepted; otherwise it shows the
    rendered text against the Oracle text, or a parse/compile error.
  - `MTG_CARDS_DIR=cards $S/bin/mtg scenario scenarios/agents/<slug>.scn`
    runs your scenarios (`-v` prints the event log of failures).
- References: `crates/mtg-dsl/SYNTAX.md` (card DSL) and
  `crates/mtg-scenario/SYNTAX.md` (scenario DSL). Read both first. The
  ~8,000 files under `cards/` are examples: `grep -rl` them for Oracle
  phrasings close to yours (the `oracle` line in each card file holds an
  id, not text, so grep the DSL statements, or use the similar cards from
  `--prompt-only`). Scenarios under `scenarios/` are examples too.

## Tokens

A card that creates a token needs the token's definition: look for it in
`cards/tokens/` (grep the description). If none exists, write one there
(`cards/tokens/<slug>.card`, copying the form of the existing ones) and
mention it in your report. The gate prints `ERROR ... unknown token` when
a card names a token nothing defines.

## Per card

1. Get the prompt pack. Write the draft `<slug>.card` with the header
   lines exactly as given, then the abilities. The slug is the name
   lowercased with every run of characters other than a–z and 0–9
   replaced by `-`, trimmed of leading and trailing `-` ("Doc Ock's
   Henchmen" → `doc-ock-s-henchmen`); `<x>` is the slug's first character.
2. Run the gate until it says `Match`. Iterate on wording: the renderer
   is strict about Oracle phrasing, and small DSL variations change the
   output. Spend a reasonable number of attempts (about 8); if it still
   doesn't match, or the DSL cannot express the card at all, delete the
   draft and report it.
3. Write 1–3 scenarios in `scenarios/agents/<slug>.scn` that exercise
   what the card does (its main ability; a restriction or condition if it
   has one, including an `illegal` line where useful). Use only the card
   itself, basic lands, and other cards already in `cards/`. Assert the
   outcome (life, zones, power, tapped, counters...). Run them until they
   pass. Then copy the card into `cards/` and delete the draft. If a scenario fails because the ENGINE behaves wrongly (not your
   script), keep the card only if you are confident the card file is
   right, drop that scenario, and report the engine bug.
4. Append one line to your report file
   (`$S/agents/report-<batch>.tsv`), tab-separated:
   `name<TAB>accepted|unsupported|failed|exists<TAB>detail`. For
   `unsupported`, the detail names the missing rule, effect, or phrasing
   in a few words ("mana spent only on X", "choose a creature type",
   "'where X is' arithmetic"); this list decides what gets built next, so
   be specific and consistent. For an engine bug, use `accepted` or
   `failed` and start the detail with `ENGINE BUG:`.

Work through every card in your batch file, in order, then stop. When
done, reply with a short summary: counts per verdict, the most common
unsupported reasons, and any engine bugs.
