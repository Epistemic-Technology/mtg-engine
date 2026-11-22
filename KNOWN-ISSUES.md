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

- **A variable target group is assumed to come last.** `spec_for`
  and the target loop treat a `TargetSpec::Many` as the final spec; a
  "choose two" pair whose first mode has "any number of target ..."
  would misplace the second mode's targets. No card known.
- **Finality counters: the exile is not a replacement choice.** It is
  applied when the zone change is performed, so another effect that
  moves a dying permanent elsewhere always wins (CR 616.1 gives the
  affected player the choice).
- **Destroying several groups at once is ordered, not simultaneous.**
  "All permanents attached to creatures" is destroyed first so the
  Equipment is found; other combinations may still miss objects.
- **Engine-picks-for-player approximations.** Forage as a cost (exiles
  the three oldest graveyard cards, else sacrifices a Food); "Remove a
  counter from this creature" of any kind (-1/-1 counters first, then
  kinds that don't change power or toughness, then the rest); several
  mechanics from the first wave have the engine choose where the player
  should.
- **Token copy of a creature that has died** (Sphinx of False
  Conclusions, Gruff Triplets): not supported; neither card is in.
- **Thousand-Year Storm:** the "cast before it this turn" count has an
  edge case with spells cast while it is on the stack.
- **Token-creation replacements apply only to `create` and `create
  token copy` effects.** Amass, fabricate, incubate, populate, offspring,
  embalm and other token makers ignore Parallel Lives, Quina, and the
  like; the replacements' order is fixed (added tokens, then doubling)
  rather than chosen (CR 616.1).
- **Reflexive trigger on a token:** "it" may read the token where the
  text means the triggering object.
- **"Triggers an additional time" looks at the battlefield as it is.**
  Teysa Karlov or Drivnod dying together with the creature whose death
  triggers doesn't double it (they should, looking back, CR 603.10a).
- **Batched triggers batch until a player gets priority.** Two separate
  discards in one resolution ("discard a card, then ... discard a card")
  make one "whenever you discard one or more cards" trigger counting
  both, where they are two events.
- **"Whenever you activate an ability of a creature" misses automatic
  mana payment:** the engine taps a creature's mana ability to pay a
  cost without activating it as an ability, so it doesn't trigger (CR
  605.3a says it should). No card in the corpus uses the trigger yet.
- **An until-end-of-turn grant of a triggered keyword** ("gains prowess
  until end of turn") may not trigger: only static grants are counted as
  granting abilities. Not reproduced; no such card yet.

## 2. Forms that pass the gate with the wrong meaning

The renderer prints the right Oracle text for these, but the compiled
card does something else. Scenarios catch them; the gate does not. Fix
by rejecting the form at compile time or by binding it correctly.

- **"any other target" meaning other than the enchanted creature**
  (Pain for All: "enchanted creature deals damage ... to any other
  target"): `another target any` excludes the source (the Aura), not
  the creature dealing the damage. Pain for All is not in.
- **`you gains shroud` / `you gains hexproof` / `you gains protection
  from ...`** render "You gain shroud until end of turn" but do nothing:
  `Effect::Grant` applies only to permanents (Seht's Tiger, Gilded
  Light). Blossoming Calm ("You gain hexproof until your next turn") is
  left out for this.
- **`when this enters and whenever this attacks while saddled`**
  applies `while saddled` to both events, so the enters half needs the
  Mount saddled (Autarch Mammoth; not added). A trigger condition
  belongs to one event of an `Either`.
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
- **Free casts still missing** (`cast free` from the hand or a target
  card is in): the discarded card (Malcolm), cards exiled by the effect
  or "with this" (Ugin's -11, Maralen, Collector's Cage's hideaway),
  "equal to" or "lesser" mana-value bounds (Brain in a Jar, Press the
  Enemy), cards in an opponent's graveyard (Memory Plunder), and "once
  each turn" permissions (Zaffai, One with the Multiverse).
- **Graveyard keyword grants still missing** (`gains harmonize` and the
  spells-you-cast grants are in): static grants to graveyard cards
  (Iroh's flashback, Green Goblin's mayhem, Festival of Embers), embalm
  until end of turn (Cursecloth Wrappings), "flashback {0} instead if
  saddled" (Archmage's Newt), and cost changes for spells cast from a
  graveyard.
- **Tapped and attacking, what is left:** "It enters tapped and
  attacking that player" as its own sentence (Shark Shredder), a card
  with mana value X or less (Kinscaer Sentry), token copies tapped and
  attacking (Grub, Sauron), one token per opponent attacking that player
  (Adeline), "that many" attacking tokens (Basri Ket).
- **Triggers, what is left:** "whenever a player taps a basic land for
  mana, that player adds one mana of any type that land produced"
  (Lavaleaper: what the land produced isn't recorded); "a loyalty ability
  of a Chandra planeswalker" (a source filter on `activates`); "whenever
  you attack with one or more creatures, ... that many" (the attackers'
  number); "If you gaining life (drawing a card, ...) causes a triggered
  ability to trigger" (other `TriggerCause`s); "whenever you scry"
  alone; "a Shaman or another Wizard" prints "other" (Harmonic Prodigy).
  Elrond, Moon-Reader's trigger is in; its "return those cards" delayed
  return of a group of targets is not.
- **Counts:** mana spent on the triggering spell; counters of every
  kind on the triggering object or a target ("counters on it" in a
  trigger); total power or toughness in a static ability; "if X is
  three or more" (a condition on a count, Obsessive Pursuit); cards in
  exile and graveyard together (Huskburster Swarm); the difference
  between power and toughness (Doran); "pay life equal to its power"
  (Madame Null); "add X mana of any one color" with a count (Mona Lisa,
  Accomplished Alchemist).
- **Conditions:** a target check joined by "or if" ("if that creature's
  power is 2 or less or if you control another Lizard", Reptilian
  Recruiter; "If you do or if you control another Dinosaur", Armored
  Kincaller); "if they lost life this turn" of a target player
  (Thought-Stalker Warlock); "it had no counters on it" (any kind,
  Unstoppable Slasher).
- **Costs:** a mana ability with exert, "discard your hand", or another
  cost automatic payment doesn't use (Oasis Ritualist, Diamond Lion).
- **Mass graveyard return of two types:** "Return each artifact and
  creature card with mana value X from your graveyard" (Fix What's
  Broken) doesn't parse (`artifact or creature card`).
- **Replacement effects:** doubling life loss (Bloodletter of
  Aclazotz) or counters of any kind (Doubling Season, Vorinclex, Loading
  Zone); "that much damage plus N" from a filter the renderer can't name
  as a source ("a red or artifact source", "a Lizard, Mouse, Otter, or
  Raccoon you control"); "an additional Food token" (Peregrin Took);
  replacing a token with another (Divine Visitation). Progenitus needs
  protection from everything.
- **Targets:** one target per opponent ("for each opponent, ... target
  ... that player controls": Lich's Relic, the Primordials); targets with
  a total mana value; "up to X targets" in a reflexive trigger; mixed
  battlefield and graveyard targets; targets that share a card type or
  compare with another target (Shifting Loyalties, Puca's Mischief,
  Daring Thief); a filter with three alternatives ("attacking,
  blocking, or tapped creature", Sonar Strike; a parse error now).
- **Other effects:** "exile those tokens when this leaves the
  battlefield" (Mysterio; the old form is a compile error now); return all land cards from your graveyard to the
  battlefield tapped; put a card from among milled cards onto the
  battlefield; pile splitting (Fact or Fiction); choose a card name / a
  number / odd or even; win-the-game effects with a condition; a variable
  base power and toughness inside "becomes" (Fractalize) or from another
  creature (Sita Varma), base power alone (Turtle-Duck); attack tax;
  "must be blocked"; "unless they sacrifice another ..." (Torment of
  Venom).
- **Mechanics not yet in:** tiered, devour, battle cry, harness, hideaway,
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

## 5. Reported in the fourth wave's card batches (to be sorted)

Wrong behaviour that reaches accepted cards:

- **Prowess granted by a static ability never triggers.** `static other
  Ally you control: has menace, prowess` (Sokka, Tenacious Tactician)
  grants the keyword but a noncreature spell does not pump. Bria,
  Riptide Rogue has the same fault. Sokka's scenario leaves that
  assertion out.
- **X in a cycling trigger is 0.** `cycling {X}{G}{G}` with `triggered
  when you cycle this: ... X` (Webstrike Elite, accepted; Valor's
  Flagship, rejected): the X paid for cycling does not reach the
  trigger. Not yet reproduced on Webstrike Elite.

Forms that pass the gate with the wrong meaning:

- **`<creature> bites each other creature`** damages only the first
  creature of the group (Waltz of Rage, Nibelheim Aflame). `bites each
  opponent` is used by accepted cards and is not known to be wrong.
- **`you may pay {X}; when you do, ... X`**: X is never chosen and is 0
  (`Effect::Pay` uses `requirements(0)`; Wildborn Preserver).
- **`ward waterbend {4}`** is paid as plain {4}: creatures and
  artifacts cannot be tapped toward it (The Unagi of Kyoshi Island).
- **`flashback {1}{R}, behold 3 Elemental`** parses and drops the
  behold part (Kindle the Inner Flame).
- **Gift with `if kicked/gift, instead <targeted effect>`**: cast
  without the gift, the spell still takes the gifted side's target and
  can fizzle on it (Wildfire Howl). The promised side was fixed in the
  third wave.
- **`it` in a granted `has ability { triggered ...: untap it }`** does
  not untap the creature; `untap this` does (Quintessential Katana).
  Same family as the first entry of section 2.
- **`tapped` after `create count each <filter>`** is read into the
  filter ("for each tapped multicolored permanent", Infinite Guideline
  Station).
- **`return all <filter> card from your graveyard`** finds `card` only
  within seven words of `all` (`parse.rs`), so a long filter fails to
  parse (Raise the Past).

Scenario runner: surveil's choice still cannot be scripted (Umbral
Collar Zealot, Arid Archway); a token named in a battlefield setup line
is still not a token.

Missing features most often named (fourth wave, 299 unsupported of 400):
wording near-misses (about 45); legendary named tokens ("create Mowu, a
legendary ... token"); "transforms into" triggers; token-creation
replacement; trigger doubling ("triggers an additional time"); cards
exiled and linked to their source for later casting; giving control of
a permanent to an opponent; "cards you own in exile"; a choice among
three keywords or counter kinds printed with commas; void; tiered;
"X is your speed"; non-mana kicker; teamwork "choose both instead".
