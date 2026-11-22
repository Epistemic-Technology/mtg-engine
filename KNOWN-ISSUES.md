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
- **Thousand-Year Storm:** the "cast before it this turn" count has an
  edge case with spells cast while it is on the stack.
- **Token-creation replacements apply only to `create` and `create
  token copy` effects.** Amass, fabricate, incubate, populate, offspring,
  embalm and other token makers ignore Parallel Lives, Quina, and the
  like; the replacements' order is fixed (added tokens, then doubling)
  rather than chosen (CR 616.1).
- **Reflexive trigger on a token:** "it" may read the token where the
  text means the triggering object.
- **A player's protection from a color** (Seht's Tiger) stops targeting
  and damage but not an Aura of that color enchanting the player (a
  Curse, CR 702.16c). No card in the corpus combines them yet.
- **"Whenever you activate an ability of a creature" misses automatic
  mana payment:** the engine taps a creature's mana ability to pay a
  cost without activating it as an ability, so it doesn't trigger (CR
  605.3a says it should). No card in the corpus uses the trigger yet.
- **A copy effect with a duration replaces the definition** (Mirrorweave,
  Mirage Mirror, Shapesharer): it is undone by restoring the old values
  when it ends, not as a layer-1 effect with a timestamp (CR 613.2a); a
  second copy effect on the same permanent before the first ends keeps
  the first from being undone.
- **"Exiled with" links whatever its ability moves to exile**: a card
  exiled by a replacement effect (Rest in Peace) while the ability
  resolves is linked to the permanent too. No card known to care.

- **"Each Dragon you control enters with an additional +1/+1 counter"**
  (`Replaces::OthersEnter`) judges the entering permanent by its printed
  card, not as it would exist on the battlefield (CR 614.12), and an
  entering permanent carries only one counter kind, so another kind it
  would enter with drops the additional counters (Dragonstorm Globe,
  Gev, Giada).
- **Mana kept "until end of turn" and firebending's mana** assume the
  player's other mana is spent first (`PlayerState::turn_mana`,
  `combat_mana`): spending the kept mana in an earlier step leaves other
  mana in the pool to be kept instead.

## 2. Forms that pass the gate with the wrong meaning

The renderer prints the right Oracle text for these, but the compiled
card does something else. Scenarios catch them; the gate does not. Fix
by rejecting the form at compile time or by binding it correctly.

- **`that player controls` in a spell** (no triggering player) matches
  nothing: `deal 2 damage to target player and each creature that player
  controls` prints Chandra's Flame Wave's text but damages only the
  player; `that player 1 controls` is the target. Breaking of the
  Fellowship ("another target creature that player controls") may be
  affected. Reject the bare form outside triggered abilities.
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
  blocking, or tapped creature", Sonar Strike; a parse error now); "any
  other target" other than the enchanted creature dealing the damage
  (Pain for All; `enchanted creature bites another target any` is a
  parse error now).
- **Other effects:** "exile those tokens when this leaves the
  battlefield" (Mysterio; the old form is a compile error now); return all land cards from your graveyard to the
  battlefield tapped; put a card from among milled cards onto the
  battlefield; pile splitting (Fact or Fiction); choose a card name / a
  number / odd or even; win-the-game effects with a condition; a variable
  base power and toughness inside "becomes" (Fractalize) or from another
  creature (Sita Varma), base power alone (Turtle-Duck); attack tax;
  "must be blocked"; "unless they sacrifice another ..." (Torment of
  Venom).
- **Most often named in the fourth wave** (299 unsupported of 400):
  wording near-misses (about 45); legendary named tokens ("create Mowu,
  a legendary ... token"); "transforms into" triggers; token-creation
  replacement; trigger doubling ("triggers an additional time"); cards
  exiled and linked to their source for later casting; giving control
  of a permanent to an opponent; "cards you own in exile"; a choice
  among three keywords or counter kinds printed with commas; void;
  tiered; "X is your speed"; non-mana kicker; teamwork "choose both
  instead".
- **Mechanics not yet in:** tiered, devour, battle cry, harness, hideaway,
  meld, the Everywhere land token, "when this land enters untapped".
- **Scenario runner:** a trigger cannot be answered while its source
  spell is still on the stack.

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
