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
  (Canyon Vaulter, Reckless Velocitaur) fires; a
  processor's cost ("Put a card an opponent owns from exile into that
  player's graveyard", Oracle of Dust: the longest exiled); "Remove a
  +1/+1 counter from a creature you control" as a cost (the creature with
  the most); several
  mechanics from the first wave have the engine choose where the player
  should.
- **Thousand-Year Storm:** the "cast before it this turn" count has an
  edge case with spells cast while it is on the stack.
- **Token-creation replacements apply in a fixed order** (tokens
  instead of others, then added tokens, then doubling) rather than the
  order the player chooses (CR 616.1).
- **Reflexive trigger on a token:** "it" may read the token where the
  text means the triggering object.
- **A chosen-name cast ban is judged at priority only** (Meddling Mage,
  Nevermore, Voidstone Gargoyle): a spell of that name cast by an effect
  (cascade, "cast it without paying its mana cost") isn't stopped, as with
  "Your opponents can't cast spells during your turn."
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
  entering permanent carries counters of at most two kinds
  (`Enter::more`), so a third kind is dropped (Dragonstorm Globe, Gev,
  Giada).
- **Life lost by exchanging life totals** (CR 701.12c) changes the
  total directly, so a life-loss replacement (Bloodletter of Aclazotz)
  doesn't see it. No card in the corpus combines them.
- **Choosing the order of a permanent spell's own "enters" replacements
  panics** ("choice Replacement ... does not fit flow Priority") as it
  resolves. "Enters with counters" replacements commute and skip the
  choice; two that don't (none known in the corpus) would hit it.
- **"If it enters as a creature"** (Teferi's Time Twist) is judged by the
  returned card's printed types, not as it would exist on the
  battlefield (CR 614.12).
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
- **Restricted mana in the pool** (CR 106.6; `GameState::restricted_pool`):
  payment spends unrestricted floating mana before restricted units,
  which is legal but not what a player would choose; restricted and
  rider mana can't pay a cost an effect asks for (ward, "unless its
  controller pays", cumulative upkeep); it always empties at the end of
  the step (`keep mana` doesn't combine with a restriction); and payment
  never activates an ability whose `add` carries `only to ...` or `spent
  on ...`, nor a mana ability with a rider (Twitching Doll, Pyramid of
  the Pantheon): the player activates it first.
- **Thirst for Identity, Sunscour, Fury of the Horde:** more
  engine-picks-for-player cases. For "exile two white cards from your
  hand" the caster chooses the first card and the engine the rest, as
  for "discard two cards".
- **"As this creature enters, it becomes your choice of ..." isn't a
  copiable value** (Primal Plasma, Corrupted Shapeshifter, Primal Clay):
  a copy doesn't get the chosen values (CR 707.2 says it should), and a
  copy entering doesn't get to choose.
- **"For each opponent, ..." assumes two players** (Hapatra, the Desert
  Frost, Lich's Relic, Riptide Gearhulk, Celebrate the Mountain-king,
  The Theorist, Jace Beleren): it happens once.
- **Conditions on the starting life total** read 20 in a scenario's
  prepared position, whatever `life A N` says.
- **A leaves-the-battlefield trigger granted by another permanent that
  leaves at the same time** may still be missed: the pre-check looks at
  the departing object's own grants, not at grants that left with it.
  The eighth wave could not reproduce it with an Aura granting its own
  creature a dies trigger (`scenarios/m5/granted-leaves-together.scn`);
  a permanent granting a leaves trigger to other permanents is untested
  (no corpus card).
- **Engine picks, eighth wave:** a reveal-from-hand cost reveals the
  first matching cards in hand, which matters where the revealed card's
  mana value or power is used (Living Destiny, Induce Despair, Disaster
  Radius, Charge of the Forever-Beast); "sacrifice any number of
  creatures" as a cost lets the caster choose the number and the first
  creature only (Vicious Betrayal), and as a trigger's effect sacrifices
  every matching permanent (Malevolent Witchkite); Cavern of Souls'
  spell is uncounterable only if payment happened to spend Cavern's
  mana; "an opponent chooses one of them" (Murmurs from Beyond) asks the
  next player.
- **Approximations, eighth wave:** "until this card is cast from exile"
  ends when the card leaves exile by any route (the Masked cycle); a
  card cast by "You may cast this card from your graveyard" counts as
  cast with flashback and as having a flashback cost (Gravecrawler,
  Skaab Ruinator); "counts as a card named X" applies to any source of
  that name, not only spells (Pardic Firecat); "one mana of any type"
  offers colors only when a color is possible (Reflecting Pool); "a cost
  that contains {C}" or "{X}" is judged on the printed cost; "if this
  ability has been activated four or more times this turn" counts every
  counted ability of the source; a mana ability activated from the hand
  or made by an effect is never used by automatic payment (Simian Spirit
  Guide); "if a creature card is revealed this way" counts a top card
  kept revealed by another effect even if the optional reveal was
  declined (Delver of Secrets).
- **Loops** end the game in a draw once 1,000 spells and abilities have
  resolved in one turn (`MAX_RESOLUTIONS_PER_TURN`). That is the rule for
  a loop of mandatory actions (CR 104.4b). A loop a player could leave
  should instead be given a number of repetitions and then stop (CR
  732.2a); the engine has no shortcuts, so a player who never stops draws
  too. The baseline agents stop themselves first (`ActivationGuard`: any
  one ability at most 50 times a turn). A loop that never uses the stack
  (state-based actions feeding each other) is not caught.

## 2. Forms that pass the gate with the wrong meaning

The renderer prints the right Oracle text for these, but the compiled
card does something else. Scenarios catch them; the gate does not. Fix
by rejecting the form at compile time or by binding it correctly.

- **Found in the seventh wave and still open; none is used by a card
  in the corpus** (each card named was rejected):
  - A dig's "with mana value X or less" reads the spell's X, not the
    dig's count (Hatchery Spider).
  - "cards revealed this way" after `reveal until ...; put that card
    onto battlefield` misses the card that was moved (Madcap
    Experiment).
  - A target check (`if target 1 is ...`) judges only the state
    qualifiers `live_state` lists (attacking, blocking, tapped, counters,
    suspected, enchanted, equipped, modified, attached-to, token,
    entered this turn); `face-down`, `that dealt damage this turn`, `that
    attacked this turn` and the rest pass for any target.
  - "As long as you control <filter>" in the layers ignores most status
    qualifiers (tapped, token, attacking): only counters and
    `transformed` are checked. Worth an audit.
- **Found in the eighth wave** (each card named was rejected unless
  noted):
  - `cycling sacrifice land` sacrifices nothing: the card is cycled for
    free (Edge of Autumn).
  - `each other creature you control` after a first group does not
    exclude that group: "other" means "not the Zombies" on In Oketra's
    Name, and the Zombies got both pumps.
  - A second `spell:` line with a label (`label "Revolt" spell: ...`)
    gates but never resolves; the block form `spell { ...; label "Revolt"
    if ... }` works (Decommission, accepted with that form).
  - `prevent all damage permanent of color of choice would deal this
    turn` asks for the color and prevents nothing (Prismatic Strands).
  - `if this is noncreature` in a trigger reads the triggering object
    (Answered Prayers).
  - `becomes land type of choice in addition` drops "in addition": the
    land loses its own types (Navigator's Compass).
  - `has first strike as long as this is blocking and you control snow
    land` ignores the blocking half on an Aura (Snow Devil).
  - `count each tapped artifact they control` with a target player
    counts the opponent's (Burden of Greed).
  - `return that card to battlefield` in "Whenever a creature is put
    into your graveyard" returns nothing; `triggering card` works
    (Angelic Renewal, accepted with that form).
  - `its controller` after "choose target ...; if kicked, instead choose
    target ..." binds to the kicked-only target; `controller of that
    target 1` works (Expel the Unworthy, accepted with that form).
  - "you may exile it and collect evidence 4" in a dies trigger exiles
    the card even when the evidence can't be collected, and `return this
    card` after `exile it` returns nothing (Lamplight Phoenix).
  - `you own` in a filter is judged only for targets and an effect's
    groups, not in a count or a static's subject.
  - An "If you do" after a target check (`if target 1 is ...`) now reads
    the check, not an earlier optional action. No card has that order.
  - The template author reads "attach it to" as the source (Sigarda's
    Aid), and `search up to X land card` takes X as a subtype (Nylea's
    Intervention).
- **`put its counters on <target>` in a trigger about another object**
  now moves the triggering object's counters but renders "put those
  counters on" (Host of the Hereafter does not gate).
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
- **Counts:** a count per player ("Each player loses 1 life for each
  creature they control", Stronghold Discipline: `they control` there is
  an error, since it compiled to "an opponent controls"); mana spent on
  the triggering spell (Tellah, Great Sage); counters of every
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
  cost automatic payment doesn't use (Diamond Lion; Oasis Ritualist is
  in, its exert ability activated by hand before the spell is cast).
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
  (about 60); a ward cost that is collect evidence (Axebane Ferox) or
  another act besides mana, life, a discard, or one sacrifice; a kicker cost that isn't mana (four cards); "assigns
  combat damage equal to its toughness" (five); "Otherwise" after a
  conditional static (four); "Choose target X and target Y" as one
  sentence with "those creatures fight each other" (four); "enchant player" Curses; "exile this, then return
  it to the battlefield transformed"; a combined subject ("you and other Heroes you control have
  hexproof"); "flash if you pay {2} more"; copying a card and casting
  the copy; choose a number, a card name, odd or even; piles.
- **Most often named in the sixth wave** (356 unsupported or failed of
  480): wording near-misses (about 55); "exile this, then return it to
  the battlefield transformed" and other returns transformed (eight
  cards); "a spell that's both white and black" (the four Mimics);
  "Enchanted land is a 6/4 creature. It's still a land." (four); "Any
  player may activate this ability" (three); "Remove X ki counters" as
  a cost (three); reveal cards until N of a kind (three); counter kinds
  impostor, bait, growth, fire, dread, invasion, devotion; ward with an
  alternative ("or pay {2}") or mana plus life; "only one recipient
  returns to the battlefield" (Sandman, Push // Pull); base power alone
  or base toughness alone (Withercrown, Maha); "would die" replaced by
  the top or bottom of the library; reveal X cards from a hand and
  choose one to discard (Thieving Sprite, Mire's Toll).
- **Left by the sixth wave's counts branch:** "half their library", "half the cards in their hand", "half the
  permanents they control" (no count of a player's library; Cut Your
  Losses, Curse of the Cabal, Lord Xander); "deals twice that much damage
  instead" (Chocobo Kick) and "it deals 3 damage to another target"
  (Magma Burst) after kicker, wording only; "Then if it has no oil
  counters on it" (Evolved Spinoderm, which the `otherwise` form
  otherwise covers).
- **Most often named in the eighth wave** (194 unsupported or failed of
  253 Pauper cards, and the eight branches' notes; the full lists are in
  the wave's reports): wording near-misses (about 45: "your team
  controls", "defending player may pay", "that player or that
  planeswalker's controller", "you may have enchanted creature fight",
  "gains flying and toxic 1", trailing "also ... if" clauses, "Choose
  target creature. You gain life equal to its power" for one sentence);
  "until end of combat" and "this combat" durations (five cards); an
  effect that makes an attacker blocked, and "cast only during the
  declare blockers step" (five); "any player may pay / sacrifice / have
  it deal damage to them" (six); a granted ability that sacrifices the
  granting Equipment (Blazing Torch, Ninja's Kunai, Deconstruction
  Hammer); non-targeting "choose a creature you control" as a recipient
  (five); a payment window before a future step (Quenchable Fire, Nafs
  Asp, Sabertooth Cobra); "pay any amount of {E}"; attack or block taxes
  on an enchanted creature (Brainwash, Oppressive Rays, Cowed by
  Wisdom); a mana ability whose cost has X or a counter removal (the
  storage lands, Petalmane Baku); a conditional "this spell can't be
  countered" (four); variable target counts ("up to X target creatures",
  "with total mana value 4 or less"); partial hand reveals; older
  multi-sentence search splits; the Flagbearer targeting rule; "that
  spell's controller" as a recipient; changing a spell's target; casting
  a card as an effect while paying its cost (Tinybones); "pay {0} for
  the first equip or exile cast each turn"; "if a Rat is attacking";
  opening-hand reveals; searches for up to X cards; two static abilities
  on one Oracle line; setting a life total; Phyrexian-hybrid activation
  costs; "target spell, activated ability, or triggered ability";
  total-mana-value target groups.
- **Mechanics not yet in:** harness, hideaway, cloak, epic,
  offering, compleated, companion, mutate, the Everywhere land token;
  "Each Sliver card in each player's hand has slivercycling {3}" (an
  ability granted to cards in hands, Homing Sliver); "has flying and
  trample if it devoured a creature" (Hellkite Hatchling).
- **"This creature or equipped creature" as a leaves or dies trigger's
  subject** still sees only the equipped creature, not the source (the
  attack, block, and damage forms see both). No card known.
- **Scenario runner:** a mana ability can't be scripted as an action
  (the engine offers none at priority; it taps for mana while paying);
  `x N` on a spell cast by a trigger isn't passed on (no corpus card
  pays X for such a cast: the free ones have X 0); a "you may cast the copy", a "you may choose a card from it" or an
  optional take (Pulsar Squadron Ace) can't be declined; `illegal cast
  ... targeting` with two "up to one" targets passes an illegal first
  target as declined (Pull from the Deep); `illegal ... cast "X" mode
  N` doesn't check that mode N is offered; `activate` can't tell two
  permanents of one name apart; a fused or aftermath spell's targets
  can't be scripted (`cast_needs` reads the split card's root
  definition; `crates/mtg-cli/tests/split_cards.rs` drives those).

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
  Measured after the sixth wave's merges with nothing else running
  (mono-red mirror, pinned best of five): 3,770 games/s before the first
  wave, 3,488 after the fifth, 3,685 after the sixth, so the wall-clock
  figure is still 2.3% under the first; the sixth wave's own features
  were not measured apart from the perf branch.
- **The optional additional cost was built twice:** behold's "you may
  behold" and the path for collect evidence and blight are separate.
- **Cost reductions on a prepared spell copy** have no scenario.
- **Merges can collide silently** on free bits (`DefFlags`,
  `GameObject::marks`; `CastInfo` has a test) and on the length of
  `Keyword::ALL`. Only `CastInfo` is checked automatically.
- **The census scripts** (`scripts/coverage/census/`) still name Pauper
  and have not been run over the union of the three pools.
