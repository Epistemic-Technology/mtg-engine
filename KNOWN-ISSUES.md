# Known issues

The open-issue list for the engine and the card language. One entry per
issue, one or two lines, naming the affected cards and the scenario that
reproduces it where there is one.

- Fixing an issue: delete its entry in the same commit, and un-skip or
  add its scenario.
- Finding one: add an entry in the same commit as whatever exposed it.
- Before a coverage wave, read this file when partitioning work; after
  merging, add what the agents reported (`scripts/coverage/README.md`).

Section 3 is a backlog, not a bug list: it records what card agents
reported missing, so the next wave's features can be chosen by count.

## 1. Wrong behaviour in accepted cards

Cards in the corpus that pass the gate and their scenarios but do the
wrong thing in some case.

- **A declined "up to N" target skips every later target.** A spell
  keeps its targets in one flat list, so a declined slot has no place
  and target choice stops there. Explosive Entry with no artifact puts
  no counter on the creature. Repro: `scenarios/m5/declined-up-to.scn`
  (skipped). The "choose two" pairing code also assumes a variable-size
  target group comes last.
- **A choose-two pair whose modes share a target description is not
  offered with one legal object.** Brigid's Command: "+3/+3" and
  "fights" both say "target creature you control"; with one creature
  the pair is missing, though the same creature may be chosen for each
  (CR 115.3). Its scenario uses two creatures.
- **Most counts read 0 in a static ability.** `per_count` in
  `crates/mtg-core/src/layers.rs` returns 0 for a list of
  "resolution-only" counts, and the gate still matches. Greatest mana
  value and devotion are fixed; the rest of the list (greatest power,
  party, differently named, cards drawn this turn, ...) has not been
  checked against the corpus for statics that use them.
- **A delayed "whenever" trigger written without "this turn" does not
  fire on later events.** The `until end of turn, delayed this:` form
  (Mistway Spy) is fixed; the general case is not.
- **Finality counters: the exile is not a replacement choice.** It is
  applied when the zone change is performed, so another effect that
  moves a dying permanent elsewhere always wins (CR 616.1 gives the
  affected player the choice).
- **Destroying several groups at once is ordered, not simultaneous.**
  "All permanents attached to creatures" is destroyed first so the
  Equipment is found; other combinations may still miss objects.
- **Automatic mana payment may skip a mana ability's non-mana cost.**
  It knows mana, tap, life, and counter costs; forage is excluded, but
  blight, collect evidence, and behold costs on a mana ability have not
  been checked. Reported, not reproduced.
- **Engine-picks-for-player approximations.** Forage as a cost (exiles
  the three oldest graveyard cards, else sacrifices a Food); several
  mechanics from the first wave have the engine choose where the player
  should.
- **Token copy of a creature that has died** (Sphinx of False
  Conclusions, Gruff Triplets): not supported; neither card is in.
- **Thousand-Year Storm:** the "cast before it this turn" count has an
  edge case with spells cast while it is on the stack.
- **Reflexive trigger on a token:** "it" may read the token where the
  text means the triggering object.

## 2. Forms that pass the gate with the wrong meaning

The renderer prints the right Oracle text for these, but the compiled
card does something else. Scenarios catch them; the gate does not. Fix
by rejecting the form at compile time or by binding it correctly.

- **`it` in a triggered ability is the triggering object**, also after a
  target was named ("put a counter on target creature; it gains
  flying") and in a trigger with no triggering object (an upkeep
  trigger's `it deals 2 damage` deals 0; an end-step trigger's `exile
  target creature then return it` leaves it in exile). Authors must
  write `target 1` or `this`. Seen on Spider-Man India, Eye of Jace,
  Airbender Ascension, Abuelo, Salvation Swan.
- **`mana value of it` in a trigger** is the source's or the triggering
  object's, not the target's (Queen's Bay Paladin; use `mana value of
  target 1`).
- **`that player mills N` after a targeted player** mills the caster
  (Exhibition Tidecaller; use `that target 1 mills`).
- **`put counters on it` after `create`** puts none on the new token
  (Lasting Fayth; use `on that token`).
- **`<target> becomes a copy of <target>`** drops the first target and
  renders the source becoming the copy (Shuri, Wakandan Inventor).
- **`each player may discard hand and draw`** applies only to the
  caster (Raphael's Technique); "each player may" works only for scry
  and search.
- **`that much` after a mana-spent check** is 0 (Tellah, Great Sage).
- **`delayed those tokens: exile those tokens when this leaves`**
  watches the tokens, not the source (Mysterio, Master of Illusion).
- **`if teamwork, instead <other target>`** and kicker with `instead`
  on a single target always ask for both targets (Cruel Alliance, Long
  River's Pull). Gift is fixed; check whether these now work.
- **`you gains shroud` / `you gains hexproof` / `you gains protection
  from ...`** render "You gain shroud until end of turn" but do nothing:
  `Effect::Grant` applies only to permanents (Seht's Tiger, Gilded
  Light). Blossoming Calm ("You gain hexproof until your next turn") is
  left out for this.
- **`another target any`** drops "other" and renders "any target"
  (Boulder Dash, Red Hulk, Pain for All).
- **`another` on a blocking-creature target** is dropped (Dissension in
  the Ranks).
- **A target filter with three alternatives** drops the third
  ("attacking, blocking, or tapped creature", Sonar Strike).
- **`delayed this: whenever <land> is tapped for mana this turn, add
  ...`** panics in the compiler (`compile.rs`, "a triggered mana
  ability") where it should report an error.
- **Template-author cards written before the first wave's catch-up**
  may read "it" as the source where the text means another object. Not
  audited.

## 3. Missing features, by cards reported

From the third wave's card batches (Standard). Counts are cards that
named the feature; a card may need more than one.

- **Wording near-misses, about 50 cards.** The DSL expresses the card
  and the renderer is a few words off. Listed with the differing words
  in the batch reports (`RENDER:` lines).
- **Durations still missing** (the sentence forms are in): a delayed
  trigger lasting "until your next turn" (Garruk, Jace Architect of
  Thought, Vraska the Unseen), "becomes a copy of ... until your next
  turn", cost changes and "can't cast" rules until your next turn,
  "for as long as it has a flood counter on it", "for as long as this
  Equipment remains attached to it".
- **Casting a card without paying its mana cost as an effect**, also
  mid-resolution and with a mana-value bound (Malcolm, Ugin, Collector's
  Cage, Tinybones, Lady Octopus, Maralen, Chandra Torch of Defiance,
  Breaching Dragonstorm).
- **Granting a keyword to spells or cards in other zones:** delve,
  affinity, cascade, storm, convoke to spells you cast; mayhem,
  harmonize, embalm, jump-start to graveyard cards (Teval, Witherbloom,
  Quandrix, Prismari, Eirdu, Norman Osborn, Songcrafter Mage).
- **Tapped and attacking:** tokens, cards from hand, cards from a
  graveyard (Fire Navy Trebuchet, Kinscaer Sentry, Shark Shredder,
  Summoner's Grimoire).
- **Batched and new triggers:** "whenever you discard one or more
  cards", "whenever you gain or lose life", "whenever you scry or
  surveil", "whenever you tap an untapped creature an opponent
  controls", "whenever a creature you control explores", "whenever you
  sacrifice one or more Foods", "a triggered ability triggers an
  additional time", third spell each turn, "transforms into".
- **Counts:** total mana value / total power / total toughness, life
  gained this turn, creatures that died (under your control) this turn,
  counters of every kind on a permanent, mana spent on the triggering
  spell, card types in your graveyard, 2^X, permanents sacrificed this
  turn.
- **Conditions:** two conditions joined by "and" or "or if" in an
  intervening "if"; "an opponent controls more lands than you"; "cast
  from anywhere other than your hand"; void (a nonland permanent left
  or a spell was warped); "it isn't your main phase".
- **Costs:** remove a counter of any kind; equip with a non-mana or a
  choice of costs; "pay X life"; "discard your hand"; exert in an
  activated cost; mana abilities that cost life.
- **Replacement effects:** "that much plus 1 instead"; doubling life
  loss or counters of any kind; damage doubled to "an opponent or a
  permanent an opponent controls"; shuffle into library instead of the
  graveyard from anywhere (Progenitus, Darksteel Colossus, Nexus of
  Fate); token-creation replacement.
- **Targets:** one target that is "spell or creature" / "spell or
  permanent"; one target per opponent; targets with a total mana value;
  "up to X targets" in a reflexive trigger; mixed battlefield and
  graveyard targets.
- **Other effects:** "End the turn"; exchange control; return all
  land cards from your graveyard to the battlefield; search then put
  into hand or graveyard; put a card from among milled cards onto the
  battlefield; a player choosing to act instead of a penalty ("unless
  they sacrifice ..."); pile splitting (Fact or Fiction); choose a card
  name / a number / odd or even; "can't lose the game"; win-the-game
  effects; set base power and toughness to a variable; attack tax;
  "must be blocked".
- **Mechanics not yet in:** tiered, devour, battle cry, exhaust-ability
  triggers, harness, hideaway, double strike and divinity counters,
  meld, the Everywhere land token, "when this land enters untapped".
- **Scenario runner:** `trigger` cannot take `dividing`; surveil cannot
  be scripted; tokens named in a battlefield setup line are not tokens
  (Baylen's "tap two untapped tokens"); a trigger cannot be answered
  while its source spell is still on the stack.

## 4. Process

- **Per-game throughput has fallen about 6.7% over three waves** (2.8%
  in the first, 4.0% in the third; mono-red mirror, pinned best of
  five). Neither drop has been bisected.
- **The optional additional cost was built twice:** behold's "you may
  behold" and the path for collect evidence and blight are separate.
- **Cost reductions on a prepared spell copy** have no scenario.
- **Merges can collide silently** on free bits (`DefFlags`,
  `GameObject::marks`; `CastInfo` has a test) and on the length of
  `Keyword::ALL`. Only `CastInfo` is checked automatically.
- **The census scripts** (`scripts/coverage/census/`) still name Pauper
  and have not been run over the union of the three pools.
