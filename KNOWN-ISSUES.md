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
  kinds that don't change power or toughness, then the rest); the second
  land of "Kicker—Sacrifice two lands" (Bog Down), the
  first other matching permanent, which may be an untapped land; the
  creatures tapped for crew and saddle (the fewest with enough power),
  which decides which "saddles a Mount or crews a Vehicle" trigger
  (Canyon Vaulter, Reckless Velocitaur) fires; several
  mechanics from the first wave have the engine choose where the player
  should.
- **Token copy of a creature that has died before its enters trigger
  resolves** (Gruff Triplets, in; Sphinx of False Conclusions, not in):
  made from last known information, but no scenario covers it (the
  runner can't answer the trigger while the creature spell is on the
  stack).
- **Thousand-Year Storm:** the "cast before it this turn" count has an
  edge case with spells cast while it is on the stack.
- **Token-creation replacements apply in a fixed order** (tokens
  instead of others, then added tokens, then doubling) rather than the
  order the player chooses (CR 616.1).
- **A copy of an X/X token is 0/0.** "Create an X/X ... token, where X
  is ..." (`sized`, Tumbleweed Rising, Slime Molding) gives the token base
  power and toughness X/X with a layer-7b effect, not as a copiable value
  (CR 111.3, 707.2), so a Clone or populate copying it gets the
  definition's 0/0.
- **Reflexive trigger on a token:** "it" may read the token where the
  text means the triggering object.
- **A player's protection from a color** (Seht's Tiger) stops targeting
  and damage but not an Aura of that color enchanting the player (a
  Curse, CR 702.16c). No card in the corpus combines them yet.
- **"Whenever you activate an ability of a creature" misses automatic
  mana payment:** the engine taps a creature's mana ability to pay a
  cost without activating it as an ability, so it doesn't trigger (CR
  605.3a says it should). Elrond, Moon-Reader uses it.
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
- **Life lost by exchanging life totals** (CR 701.12c) changes the
  total directly, so a life-loss replacement (Bloodletter of Aclazotz)
  doesn't see it. No card in the corpus combines them.
- **Mana kept "until end of turn" and firebending's mana** assume the
  player's other mana is spent first (`PlayerState::turn_mana`,
  `combat_mana`): spending the kept mana in an earlier step leaves other
  mana in the pool to be kept instead.
- **A melded permanent's mana value is 0** (Chittering Host, Hanweir, the
  Writhing Township): its definition is the back face, which has no mana
  cost; it should be the sum of its front faces' (CR 202.3c, 712.8g). The
  other card that represents it waits in exile (`GameState::melded`), so
  "cards in exile" counts it.

- **"When this land enters untapped"** (Idyllic Grange) is the
  intervening-if trigger `when this enters if this is untapped`: tapping
  the land with its trigger on the stack stops it (CR 603.4); the printed
  trigger event is judged only as the land enters.

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
- **`attach to target creature you control; that creature gains ...`**
  (Celestial Armor: "When this Equipment enters, attach it to target
  creature you control. That creature gains hexproof and indestructible
  until end of turn."): the Equipment attaches but the creature gains
  nothing. Celestial Armor is not in (the template author writes it).
- **Found by the parse-forms branch; not used by a card in the corpus**
  (the template author writes both; each card was rejected):
  - `exile target nonland permanent ... and all other nonland permanents
    that player controls with same name as target 1 until this leaves`
    exiles only the target (Deputy of Detention).
  - `triggered when this dies: put count each -1/-1 counters on this
    -1/-1 counters on target creature` puts none: the count doesn't read
    the counters it had as it died (Grief Tyrant).
- **Found by the fifth wave's card batches; none is used by a card in
  the corpus** (each card named was rejected):
  - `each player loses count each creature they control life`: every
    player loses for the opponent's creatures (Stronghold Discipline).
  - A sentence after `delayed target 1: when ...` is taken into the
    delayed trigger, so it never happens on resolution (Time to Feed).
  - `trigger again other creature you control of chosen type` never
    doubles; the same with a named type does (Roaming Throne).
  - A status word in a target check is ignored: `if target 1 is
    enchanted creature ...` and `creature with aura` are true for a
    creature with no Aura (Karametra's Blessing).
  - `whenever modified creature you control leaves the battlefield`
    fires for an unmodified creature (Costume Closet).
  - A counter condition on the enchanted creature in a static (`if
    attached is with sleep counter`) is always true, and `tap enchanted
    creature and put X sleep counter on it` puts none (Venarian Gold).
  - `that creature becomes Mutant ...` in a trigger doesn't bind to the
    target; `that target 1 becomes ...` does (Jenova uses the second).
  - `static enchanted: has ... as long as this is with power 3 or less`
    reads the Equipment's power, and the `attached` form prints
    "equipped creature's power" for "its power" (Thieves' Tools).
- **`whenever you cast of chosen color spell`** renders "Whenever you
  cast a spell of the chosen color" but never triggered (Diamond Mare,
  not added).
- **The sacrificed creature in a reflexive trigger:** `you may sacrifice
  1 other creature chosen; when you do, deal damage equal power of the
  sacrificed creature to ...` deals 0 (Bolg of the North, not added).
- **`put its counters on <target>` in a trigger about another object**
  moves the source's counters, not the triggering object's (`put its
  counters on this` and `put those counters on ...` take the triggering
  object's). No card in the corpus writes it.
- **Template-author cards written before the first wave's catch-up**
  may read "it" as the source where the text means another object. Not
  audited.

## 3. Missing features, by cards reported

From the third wave's card batches (Standard). Counts are cards that
named the feature; a card may need more than one.

- **Wording near-misses, about 50 cards.** The DSL expresses the card
  and the renderer is a few words off. Listed with the differing words
  in the batch reports (`RENDER:` lines).
- **Targets only a bargained spell has** (Brave the Wilds: "If this
  spell was bargained, target land you control becomes ..."): the target
  is required unbargained too (kicker targets are handled; bargain isn't).
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
  ability to trigger" (other `TriggerCause`s); "a Shaman or another
  Wizard" prints "other" (Harmonic Prodigy); "this permanent transforms
  into" on a card whose faces have different types (Sidequest: Raise a
  Chocobo: the face doesn't know the other face's types); "whenever you
  give a gift" (a permanent's gift is given by its own enters trigger);
  "whenever you discover" and "discover again for the same value";
  "whenever you attack with two or more Spiders" and "deals 4 or more
  damage"; "the first time that creature has become tapped this turn"
  (Captain America); "whenever one or more creatures attack you, those
  creatures ..." (Sabotage Strategist).
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
- **Replacement effects:** doubling counters of any kind (Doubling
  Season, Vorinclex, Loading Zone: loyalty paid as a cost isn't put by
  an effect, and a planeswalker's entering loyalty is added outside the
  replacement pipeline); "that much damage plus N" from a filter the
  renderer can't name as a source ("a red or artifact source", "a
  Lizard, Mouse, Otter, or Raccoon you control": `replace damage from a
  Lizard, ...` doesn't parse the list); "that much damage minus 1" from a
  spell (Benevolent Unicorn: no "a spell" damage source); "draw that many
  cards plus one" (Quantum Riddler: draws are replaced one card at a
  time); "except the first one you draw in each of your draw steps"
  (Bard, King of Dale); "an additional Food token" (Peregrin Took);
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
  creature (Sita Varma); attack tax;
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
- **Most often named in the fifth wave** (363 unsupported or failed of
  480; nearly every card names a different gap): wording near-misses
  (about 60); a ward cost that is a sacrifice (Vein Ripper and Ygra,
  Eater of All, removed: the cost was never enforced), collect evidence,
  or another act; a kicker cost that isn't mana (four cards); "assigns
  combat damage equal to its toughness" (five); "Otherwise" after a
  conditional static (four); "Choose target X and target Y" as one
  sentence with "those creatures fight each other" (four); "Remove X
  counters" as a cost; "enchant player" Curses; "exile this, then return
  it to the battlefield transformed"; restricted mana of more than one
  symbol or added by an effect; "if that spell was kicked"; a combined subject ("you and other Heroes you control have
  hexproof"); "flash if you pay {2} more"; copying a card and casting
  the copy; choose a number, a card name, odd or even; piles; haunt,
  amplify, sweep, devour, tribute, cleave, champion, reconfigure.
- **Left by the sixth wave's counts branch:** restricted mana of more
  than one symbol or added by an effect (about 40 cards: Great Hall of
  the Citadel, Resonating Lute, Eldrazi Temple, Boommobile; restricted
  mana is spent as its source is tapped and never sits in the pool, so a
  two-mana restricted ability needs the pool to keep restricted mana);
  "half their library", "half the cards in their hand", "half the
  permanents they control" (no count of a player's library; Cut Your
  Losses, Curse of the Cabal, Lord Xander); "deals twice that much damage
  instead" (Chocobo Kick) and "it deals 3 damage to another target"
  (Magma Burst) after kicker, wording only; "Then if it has no oil
  counters on it" (Evolved Spinoderm, which the `otherwise` form
  otherwise covers).
- **A cost of "sacrifice six creatures named this"** is never offered
  (Shadowborn Apostle, not added).
- **Mechanics not yet in:** tiered, devour, harness, hideaway, mutate,
  meld, the Everywhere land token.
- **Scenario runner:** a trigger cannot be answered while its source
  spell is still on the stack.
  An edict-style choice ("return another permanent you control to its
  owner's hand", Aviary Mechanic) always takes the first option; an
  `illegal ... trigger` line followed by a legal `trigger` in the same
  step reports the legal one unreached; a mana ability can't be scripted
  as an action; no way to give X to a spell cast by a trigger, to choose
  a library-search card, or to say `prototyped`/`blitzed`/`prowled`
  (all use `alternative`).

## 4. Process

- **Throughput: the five-wave drop is recovered; what is left.** Binaries
  built at each wave boundary had lost about 2% a wave (mono-red mirror;
  callgrind: 731.7M instructions in `mtg_sim::play` for 300 games at
  9dfb5c71, 775.5M at 27cbd09a). The growth was spread thin: a
  priority listing that checked fourteen alternative casts per hand card
  and called `castable` for off-timing cards, cached mana sources cloned
  entry by entry, an options vector regrown push by push, a large frame
  on every cached `characteristics` call, an `IdMap` probe per battlefield
  hit, and the derived `Flow::clone` per loop step. After the perf branch
  the mirror is 712.2M (below the oldest wave) and random:3 v random:4
  (100 games) 1,768.0M -> 1,241.1M: `KeywordSet::iter` tested all 72
  keywords for every trigger watcher, the layer pass collected a vector
  per object per layer, and snapshots cloned objects' small vectors
  element by element. Start-up (`mtg sim --games 0`): 2,013M -> 1,745M
  instructions (lexer and `at_word` allocation). Left, by size: in
  effect-heavy games the layer pass (21% of a game: any `cbump` makes
  the next `characteristics` call recompute every object), trigger
  watchers (`watchers` copies characteristics for every permanent whose
  printed abilities might see the event), and state snapshots before
  actions that may ask midway (`Engine::advance` clones the whole state,
  and twice when the question comes). In the mirror: `characteristics`
  still returns an 88-byte copy (9%), `mana_sources` misses its cache on
  37% of calls, malloc and free about 15%. At start-up, rendering every
  card's Oracle text for `CardDef::oracle_text` is 14% (lazy rendering
  would need the AST kept; `link_exiles` reads the text), and dropping
  the pool at exit 2.5%. The "30 ms fixed cost inside the timed region"
  did not reproduce in instruction counts: the first game costs about
  2M more instructions than later ones (malloc working through the heap
  the corpus load left), about 1 ms; the rest is cold caches or noise on
  a loaded machine. Wall-clock A/B on this machine was unreliable at load
  20+; use callgrind.
- **The optional additional cost was built twice:** behold's "you may
  behold" and the path for collect evidence and blight are separate.
- **Cost reductions on a prepared spell copy** have no scenario.
- **Merges can collide silently** on free bits (`DefFlags`,
  `GameObject::marks`; `CastInfo` has a test) and on the length of
  `Keyword::ALL`. Only `CastInfo` is checked automatically.
- **The census scripts** (`scripts/coverage/census/`) still name Pauper
  and have not been run over the union of the three pools.
