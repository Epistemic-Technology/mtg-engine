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
- **Engine picks and approximations, ninth wave:** attack and block
  taxes (Propaganda, Ghostly Prison) are paid automatically, a
  declaration being offered when each attacker could be paid for at its
  cheapest target; "pay {0} for the first equip each turn" is always
  taken (Kíli); "any player may" asks from the active player on, though
  some rulings start with the controller; only a single target can be
  changed (Bolt Bend); a reveal land reveals the first matching card;
  "Tap three untapped Elves you control: Add {G}{G}{G}" (Heritage Druid)
  and a granted `activated {T}: add {G}{G}` are activated by hand, not
  used by payment; "the first creature spell each turn costs less"
  judges earlier spells by their printed characteristics; "until end of
  combat" made outside combat lasts to the end of the turn's next
  combat.
- **Found in the eleventh wave and still open** (the twelfth fixed the
  rest):
  - The controller of a hideaway permanent can't decline to play the
    exiled card once the condition holds.
  - A spell limit ("can't cast more than one spell each turn") counts
    spells cast earlier in the turn, before the limiter arrived (the
    rule; recorded for review, no known discrepancy).
  - A creature put onto the battlefield attacking always attacks the
    player; its controller should choose the player or a planeswalker
    (CR 508.4). That it ignores "can't attack you" is the rule (CR
    508.4c).
  - Under "Players can't draw cards" a draw is skipped silently; an
    effect that counts "cards drawn this way" counts 0. No card known
    to combine them.
  - "Each opponent may ... or ..." and "unless that player sacrifices"
    act on every opponent inside each payer's turn: right for two
    players only.
- **Found in the twelfth wave, in cards that are in:**
  - A mana ability's Phyrexian symbols are still paid without asking.
    "Spend mana as though it were mana of any type" leaves Phyrexian
    symbols as they are, and does not reach a kicker or buyback cost
    added during the cast; Chromatic Orrery covers spells and activated
    abilities only (not ward, echo or "unless its controller pays").
  - A filter's mana value still sees a face-down exiled card's printed
    value (CR 406.3a: 0). Left open: Beseech the Mirror and Discover the
    Impossible go through the same check and need the printed value (CR
    406.3b); a fix has to tell those casts apart.
  - A card with a mandatory additional cost of its own cast by a
    granted retrace or jump-start skips its own additional cost.
  - Sanctum Prelate's ban judges the printed mana value with X as 0;
    name bans until your next turn are not checked for a free cast made
    during a resolution (as Meddling Mage's).
  - "the rest" and "not chosen this way" read the resolution's last
    choice only. "a card exiled with this" copies every linked card
    (right while one is linked).
  - A modal double-faced card's land face played as a land is not
    offered "enters as a copy"; a permanent spell with "enters tapped
    as a copy" would enter untapped. No such card known.
  - Mana abilities with a rider the payment planner doesn't know
    (`rest_ok`) are usable only by activating them by hand; cost
    changes to mana abilities with a mana cost apply only where the
    planner uses them. Not audited.
- **Engine picks, twelfth wave:** "an opponent chooses" among searched
  cards, and one target per player, assume two players; "choose a
  number" offers 0 to 20; of several granted graveyard ways one is
  offered (flashback, then jump-start, retrace, mayhem), a card's own
  sneak cost is used ahead of a granted one, and a top card castable
  for life or for mana is cast for mana; a granted foretell's cost is
  always the mana cost less {2}; looking at a new top card while a
  spell is being cast is not held back.
- **Found in the thirteenth wave, in cards that are in:**
  - Several spells cast "from among" exiled cards (`Effect::CastAmong`:
    Etali, Improvisation Capstone, Doom Reigns Supreme) are offered one
    card at a time and cast in exile order, not the player's order;
    castability is judged for all before the first is cast, so one that
    loses its only target to an earlier one is dropped as its cast
    begins.
  - "Do this only once each turn" (Planetarium of Wan Shi Tong) counts
    only a cast taken: declining leaves the limit unused, as for any
    `TurnLimit::Do` "you may".
  - "Lands you control enter untapped" always wins (Spelunking, The
    Wandering Minstrel): the player can't order the replacements (CR
    616.1) to have a land enter tapped. No card known to want that.
  - Mill and "one or more" draw replacements (The Water Crystal, Quantum
    Riddler) see only `mill` and `draw_n`; mulligan draws don't use
    `draw_n`.
  - Bumi's "Only land creatures can attack during that combat phase"
    applies to the next additional combat phase to begin
    (`GameState::pending_combat_attackers`), which may be another
    effect's queued before it.
  - The Kingpin of Crime's group is fixed as the ability resolves (a
    granted keyword until end of turn): a creature that later has
    toughness greater than its power isn't covered (CR 611.2c).
  - "You have protection from each of your opponents" (Absolute Virtue)
    assumes two players; an opponent's Aura falls off the player as a
    state-based action but can still be put onto the battlefield
    attached to them.
  - "Each player who does" after "each player may" (Step Between Worlds,
    Turtles in Time) splits a following draw off the `PlayerMay`;
    another verb there would run inside each player's "may".
  - "Except for tokens you control" assumes two players: it is filled in
    as "nontoken, or controlled by the other player" (Ajani
    Unrelenting).
  - A granted mana ability with a spend restriction (Clement, the
    Worrywort's Frogs) isn't used by automatic payment; the player
    activates it by hand first.
  - "You may put a ... card from your hand onto the battlefield" now
    counts as not done when nothing is put (Origin of the Avengers);
    other "if you don't" cards were not audited against the old
    behaviour.
  - "Card types among spells you've cast this turn" (April O'Neil,
    Hacktivist) reads the cast cards' printed types, not the spells as
    they were on the stack.
  - Kid Loki's "that you've put one or more +1/+1 counters on this turn"
    is judged by the permanent's controller, not the static ability's
    controller (the same for "creature you control").
  - Several cards put onto the battlefield at once enter one at a time,
    each seeing the earlier ones enter (CR 603.6a; Ghalta, Stampede
    Tyrant, Rise of the Dark Realms, Push the Limit, "return all").
  - One Last Job with no creature to attach to returns nothing: an
    Equipment should enter unattached; an Aura stays (CR 303.4g).
  - "Choose new targets" doesn't refuse a change that makes a kept
    target illegal (CR 115.7d). No corpus card known.
  - Granted spell keywords ("Instant and sorcery spells you control have
    lifelink") read the spell's printed types, and "cards that have an
    Adventure" a face-down exiled card's printed layout. No corpus card
    known.
  - A linked permission's finality rider (Intrepid Paleontologist) and
    "mana of any type can be spent" attach to the linked card, not the
    permission: any cast of it from exile while its permanent is under
    the caster's control gets them.
  - A copy that keeps its own name (`except { named this }`) is named so
    by `GameState::obj_name`, but `ObjFilter::named` compares the copied
    definition, so "named" filters still match the copied card's name.
    No card known to care.
  - Follow the Lumarets' "You may instead reveal two creature and/or
    land cards" lets the player take fewer than two.
  - Rowan's Grim Search puts the two cards back on top in the order they
    were; the player doesn't choose (as the other "any order" digs).
  - Reveal-until digs (`dig N until`) don't count for "cards revealed
    this way". No card asks.
  - A dig's "instead" sentence is placed by string search
    (`dig_alt_clause`); a base text with neither ". " nor "the rest"
    would print it last. None known.
  - "Deals 4 or more damage" (Spinneret and Spiderling) is judged per
    recipient: combat damage split among several recipients doesn't add
    up.
  - "Whenever one or more creatures attack you"
    (`TriggerSpec::PlayersAttacked`) sees attacks at players only, not
    at their planeswalkers.
  - King T'Challa is written `transform this` on a `modal_dfc` card; its
    scenario tests only the draw trigger.
  - `if player is damaged this way` prints "they" by rewriting a leading
    "It <verb>s" (Sonic Shrieker); a body starting otherwise keeps "it".
  - An optional waterbend cost is offered by checking mana sources and
    tappers, not restricted mana (as kicker's check).
  - "Exile a creature you control" as an activation cost (The Soul
    Stone) links nothing to the source.
- **Engine picks, thirteenth wave:** `add produced` ("one mana of any
  type that land produced") adds the kind the tapped permanent made most
  of, and payment foresees that mana only when the permanent makes one
  kind; of several permissions to cast a graveyard card
  (`GrantedWay::Cast`, `Effect::GraveCastTurn`) the one with the fewest
  riders is used (no added cost first, then no finality counter), and a
  card's own graveyard way or a granted flashback, jump-start, retrace
  or mayhem is used ahead of them (CR 601.3 lets the player choose);
  foraging as the added cost is the engine's pick.
- **Engine picks, eleventh wave:** the cards exiled by a "search their
  graveyard, hand, and library for cards with that name" (hand first);
  the
  creature copied by "a copy of one of them" (Twilight Diviner); the
  cheapest of several granted flashback costs; an optional cost whose
  discount the offer assumed can't be declined unless the full cost is
  payable.
- **Engine picks and approximations, the tenth wave's follow-ups:**
  collect evidence and blight paid as a ward cost are chosen by the
  engine; the Flagbearer rule forces only the last eligible target, a
  group of "any number of" targets counts as one, and a forced "up to
  one" target can't be declined; "can't be the target of spells" counts
  anything on the stack that isn't an ability as a spell (no corpus card
  uses it yet).
- **Approximations, tenth wave:** a static free cast (Omniscience,
  Dracogenesis, Weftwalking) is offered from the hand only, and not for
  split, Adventure or modal double-faced halves; a spell cast free from
  the hand does not rebound; "damage dealt to it this turn" reads the
  damage still marked; in scenarios, "you may put a creature card from
  your hand onto the battlefield" takes `choose "X"` / `choose done`
  (unscripted it takes the first card).
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

- **Found in the eighth wave and still open:**
  - `return that card to battlefield` in "Whenever a creature is put
    into your graveyard" returns nothing; `triggering card` works
    (Angelic Renewal, accepted with that form). A fix needs to know
    which of about thirty effects set the created object.
  - "you may exile it and collect evidence 4" in a dies trigger exiles
    the card even when the evidence can't be collected, and `return this
    card` after `exile it` returns nothing (Lamplight Phoenix): needs a
    joint optional cost.
  - An "If you do" after a target check (`if target 1 is ...`) reads the
    check, not an earlier optional action. No card has that order.
- **Found in the twelfth wave:** `discard that much cards` after "you
  may draw cards equal to its power. If you do," renders right; whether
  "that many" is the number drawn is not checked (Horrid Shadowspinner
  left out).
- **Found in the thirteenth wave:**
  - `you may exile this and collect evidence 4; if you do, ...`
    (Lamplight Phoenix) exiles even when no evidence can be collected,
    and "if you do" then follows the exile alone: the two need to be one
    optional cost paid in full or not at all. The card is left out.
  - `target creature or Spacecraft card with mana value 5 or less`
    (Scrounge for Eternity) parses "or less" as a third alternative: a
    mana value after an alternative filter isn't read as one qualifier
    for both. The card is left out.
  - "exile all other Nightmare tokens you control" after creating a
    token (Abyssal Harvester): `other` excludes the source, not the
    token just created, so the new token is exiled too. The card is
    left out.
  - A mana rider's effect on "it" (`spent on <filter> spell: ...`,
    `Effect::SpendRider`) acts on the spell, so "it gains haste until
    end of turn" ends when the spell becomes a permanent (CR 400.7):
    Carnelian Orb of Dragonkind can't be written.
  - `count N <expr>` (`StaticCondition::CountAtLeast`) accepts any
    count, but a resolution-only count (`CountExpr::in_static` false,
    such as `different powers among`) reads 0 in a static. The cards
    using it read static counts; a compile-time refusal would be safer.
  - `this attacked this combat` (`SourceHistory::AttackedThisCombat`) is
    "declared as an attacker this turn, during a combat": with two
    combat phases, an attack in the first counts in the second. No
    accepted card uses it yet.
  - `StaticCondition::NotTheirTurn`, `Attacking` and `TriggerIs` never
    hold in a static ability; only effects, costs and trigger conditions
    read them. `decompile` of `TriggerIs` is `unreachable!`.
  - Station by toughness reads the tapped creature's keyword for any
    tapped stat: a later count of "the tapped creature's power" by
    another route (`CountExpr::MovedStat { verb: Tapped, stat: Power }`)
    would count toughness for a creature with `stations using
    toughness`.
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
  (Iroh's flashback, Green Goblin's mayhem), embalm until end of turn
  (Cursecloth Wrappings), and "flashback {0} instead if saddled"
  (Archmage's Newt).
- **Tapped and attacking, what is left:** "It enters tapped and
  attacking that player" as its own sentence (Shark Shredder), a card
  with mana value X or less (Kinscaer Sentry), token copies tapped and
  attacking (Grub, Sauron), one token per opponent attacking that player
  (Adeline), "that many" attacking tokens (Basri Ket).
- **Triggers, what is left:** "a loyalty ability
  of a Chandra planeswalker" (a source filter on `activates`); "whenever
  you attack with one or more creatures, ... that many" (the attackers'
  number); "If you gaining life (drawing a card, ...) causes a triggered
  ability to trigger" (other `TriggerCause`s); "a Shaman or another
  Wizard" prints "other" (Harmonic Prodigy); "this permanent transforms
  into" on a card whose faces have different types (Sidequest: Raise a
  Chocobo: the face doesn't know the other face's types); "whenever you
  give a gift" (a permanent's gift is given by its own enters trigger);
  "whenever you discover" and "discover again for the same value"; "the
  first time that creature has become tapped this turn"
  (Captain America); "whenever one or more creatures attack you, those
  creatures ..." (Sabotage Strategist).
- **Counts:** a count per player ("Each player loses 1 life for each
  creature they control", Stronghold Discipline: `they control` there is
  an error, since it compiled to "an opponent controls"); counters of
  every kind on the triggering object or a target ("counters on it" in a
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
- **Replacement effects:** doubling counters of any kind (Doubling
  Season, Vorinclex, Loading Zone: loyalty paid as a cost isn't put by
  an effect, and a planeswalker's entering loyalty is added outside the
  replacement pipeline); "that much damage plus N" from a filter the
  renderer can't name as a source ("a red or artifact source", "a
  Lizard, Mouse, Otter, or Raccoon you control": `replace damage from a
  Lizard, ...` doesn't parse the list); "that much damage minus 1" from a
  spell (Benevolent Unicorn: no "a spell" damage source); "except the
  first one you draw in each of your draw steps"
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
  the wave's reports; the ninth wave built the combat durations, "any
  player may", "becomes blocked", attack and block taxes, non-targeting
  "choose a creature you control", "pay any amount of {E}", the storage
  lands, variable target counts, partial hand reveals, counted
  searches, opening-hand reveals, changing a target, and "target spell,
  activated ability, or triggered ability"): wording near-misses (about 45: "your team
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
- **Most often named in the ninth wave** (338 unsupported of 360
  Standard cards retried; the full lists are in the reports): one-off
  conditions and counts ("half their library", "if X is 10 or more",
  "the number of times you descended", a spell's number of colors,
  "three different kinds of counters", "cards put into exile this
  turn"); bare "if you cast it" on a permanent other than the source;
  temporary or static permissions to cast or play (Omniscience, free
  casts from exile with a mana-value cap, "look at the top card any
  time", flashback or warp granted to cards in a hand or graveyard);
  face-down exile with "look at and play"; static grants of other
  cards' activated abilities; ward costs other than mana and life
  (sacrifice, poison, life equal to power, collect evidence); cost
  reductions by a power or other count on one spell; piles; "choose a
  number", "choose a card name" as a spell effect; plural and personal
  pronouns for legends ("him", "she", "they"); searches for several
  differently-filtered cards or "with different names"; controlling
  another player's turn; "can't be sacrificed"; a payment window before
  a future step; "target opponent chooses a creature they control";
  "two target creatures controlled by the same player"; the Flagbearer
  rule.
- **Left by the twelfth wave's branches** (the eleventh's list is
  built; each of these is named by several cards): "as this enters, exile
  cards" with a copy of them (The Mimeoplasm, Dermotaxi) and imprint
  forms (Mimic Vat, Soul Foundry, Prototype Portal); "If damage is
  prevented this way, <source> deals that much damage to ..." and "that
  source's controller" (Deflecting Palm, Vengeful Archon, Refraction
  Trap, about nine cards); "becomes a Treasure artifact and loses all other card
  types and abilities" (Vraska, Kitesail Larcenist); "you win the
  game" on an empty library (Jace, Wielder of Mysteries); "Skip your
  draw step"; a cost filter and trigger for "spells you cast but don't
  own" (Gonti, Canny Acquisitor; Nita); gaining the activated abilities
  of opponents' permanents (Drana and Linvala, Sharkey); a cost change
  by a count for a player static (Agatha of the Vile Cauldron,
  Dragonkin Berserker); "the first activated ability you activate each
  turn"; "Only you may activate this ability"; choosing one card among
  those exiled with a permanent (Bane Alley Broker, Lazav, Wearer of
  Faces); "all other cards exiled with this"; a delayed ability with
  targets of its own (Fatal Fissure) and one naming its granter; a
  trigger on a time counter removed in exile; a free cast from the top
  of the library and a filter on lands played from it; per-player
  splits into three piles (Whims of the Fates); exactly one target per
  player (Vaevictis Asmadi); companion (Zirda); "assign combat damage
  as though it weren't blocked"; the amount of life gained in a
  "whenever you gain life" trigger; "Equipped creatures you control".
- **Left by the thirteenth wave's branches** (merged from the branches'
  notes; each card is listed once, under the form that blocks it):
  - Triggers: "those creatures" after an attack trigger (Sabotage
    Strategist; Garruk, Curse Breaker and Jace, Reality Sculptor, which
    also need delayed triggers until your next turn); a delayed trigger
    lasting "until your next turn" (Season of the Bold); a delayed
    trigger's own target (Roads Go Ever, Ever On); "when you next attack
    this turn" (All-Out Assault); a delayed trigger made by a leaves
    trigger (Aang, at the Crossroads); per-object "the first time this
    turn" (Captain America, Living Legend; Stalwart Successor); one
    trigger on two events, "When this enters and at the beginning of
    your upkeep" (Gathering Stone, Obsessive Pursuit); "an opponent
    searches their library" (Wan Shi Tong, Librarian); "your first or
    second card each turn" (Lady Octopus); "a spell you don't own"
    (Vaan, Street Thief; Nita, Forum Conciliator); "one or more land
    cards are put into your graveyard from your library" (Hedge
    Shredder); "you play a land from exile or cast a spell from exile"
    (Gwen Stacy // Ghost-Spider); "you play a land" (The Endstone, also
    "half your starting life total"); "enchanted player is dealt damage"
    (Grievous Wound); "enters or deals combat damage to a player"
    (Kitsune, Dragon's Daughter); any bending action and "all four this
    turn" (Avatar Aang // Aang, Master of Elements, also a colored cost
    reduction); attacks by total power (Desert Were-Worm), by a lone
    creature at a player (Jiang Yanggu, Alone), and by "a Vehicle
    crewed by this creature this turn" (Balthier and Fran, also "the
    first combat phase" and an additional combat phase); combat damage
    to a chosen player this turn (Great Train Heist) or from a delayed
    or quoted ability (Flitterwing Nuisance; Sygg, with "protection from
    each color"); "becomes the target of an ability you control" (Loki,
    Surrak); "activate an ability that targets" and copying it (Ertha
    Jo); "when you next activate an exhaust ability", copying it (Pit
    Automaton, also "Spend this mana only to activate abilities");
    "untap one or more permanents during your untap step" (The
    Millennium Calendar); "you're dealt combat damage" and point
    counters (Contested Game Ball); a spell's damage trigger filtered by
    its targets (Imodane, the Pyrohammer); "while this card is in your
    graveyard" (Furious Forebear); a filtered reflexive "when you
    discard a nonland card this way" (Glacial Dragonhunt, Miasma Demon)
    and one on what a dig put onto the battlefield (Gilgamesh,
    Master-at-Arms); "whenever you gain life, put that many" (Ageless
    Entity, Light of Promise, Sunbond); a cast trigger's filter compared
    with the source's power (Jackal, Genius Geneticist; Loki Laufeyson);
    transform-into triggers on faces held by other parts (Ultimecia's
    joint pay-and-exile cost); Kaya, Spirits' Justice, Fblthp, Way of
    the Mind Sculptor, Nicanzil, Stolen Uniform, Taii Wakeen, Market
    Gnome, Rhino's Rampage, Thorin, Prishe's Wanderings, Wilt-Leaf
    Liege, Feral Encounter, Dalkovan Encampment and Tomik each need a
    one-off trigger form.
  - Counts: a group's total power (Kylox, Visionary Inventor; The
    Skullspore Nexus, also "double target creature's power"); a
    per-recipient amount (Sunspine Lynx); the difference between two
    chosen creatures' powers (Spry and Mighty); base power as a
    characteristic (Sovereign Okinec Ahau; Kutzil, Malamet Exemplar); a
    per-type count to a maximum in a static (Diligent Zookeeper); color
    pairs (Niv-Mizzet, Guildpact); planeswalker types and "proliferate X
    times" (Tam, the Possibility); "different mana values among" (Lunar
    Insight), "with different powers" (Rip, Spawn Hunter), "eight
    different names among unlocked doors" (Central Elevator // Promising
    Stairs); graveyards with seven or more cards (The Master of
    Lake-town); seven or more cards in exile (Ketramose, also its
    trigger); the sacrificed object's mana value (Repurposing Bay,
    Sidisi) or power, putting up to X lands from the hand (The Gitrog,
    Ravenous Ride); the power of the creature that died or the spell's
    mana value (Death's Presence, Dancing from Dark to Dawn); the
    enchanted creature's last-known power in a dies trigger (Avatar
    Destiny, also "put up to one creature card milled this way onto the
    battlefield"); the triggering spell's X (Geometer's Arthropod) and
    blue symbols in its cost (Namor the Sub-Mariner); spells cast this
    turn other than the first (Outlaw Stitcher) and noncreature spells
    the triggering player cast (Magebane Lizard); permanents sacrificed
    this turn as a damage amount (Sawblade Skinripper); "exile cards
    equal to its mana value from target player's library" (Rakdos, the
    Muscle); the web-slinging cost's returned creature (Scarlet Spider,
    Ben Reilly); "where X is the amount of mana spent to cast this
    creature" after a target (Astelli Reclaimer); counters given to a
    spell being cast, by colors of mana spent (Wildgrowth Archaic) or
    "for each" on "your first Human creature spell each turn" (Torgal,
    A Fine Hound); cards put on top after surveil (Starving Revenant);
    "Add {R} for each card exiled this way" with a play permission
    (Quintorius Kand).
  - Turn tallies: a filtered died tally (Undead Sprinter) and a
    sacrificed Food (Bonecache Overseer); exhaust activations (Elvish
    Refueler); cards put into a target player's graveyard from their
    library (Cruel Calculations); players dealt combat damage by this
    creature (Steel Hellkite); "if a Pirate and a Vehicle attacked this
    combat" (Fearless Swashbuckler); Zombies entered and "other than
    your first spell that turn" (Geralf, the Fleshwright); a filtered
    `Entered` count's wording and "as though its power were 2 greater"
    (Cloudspire Coordinator); three kinds of "first spell this turn"
    and "have target opponent draw a card. If you do, copy that spell"
    (Alania, Divergent Storm); the wording "You can't cast ~ during your
    first, second, or third turns of the game" (the condition exists:
    Jace Reawakened, also "it becomes plotted"; Spider-Man 2099).
  - Conditions: joined by "or" with a target check, an "If you do" or a
    "this way" check (Reptilian Recruiter, Armored Kincaller, Cache
    Grab); a target first named inside the condition (Hidetsugu's
    Second Rite); "if they lost life this turn" of a target player
    (Thought-Stalker Warlock); the most life or tied for most life
    (Preacher of the Schism); "if it's not their turn" of the triggering
    player (Adrenaline Jockey); "this way" checks: a Cave put onto the
    battlefield (Spelunking), an artifact put (Oviya, Automech Artisan,
    also "attacking one of your opponents"), cards drawn (Transcendent
    Archaic), a discarded land, then "it" as that card (Silvan Reveler),
    a surveilled noncreature, nonland card (Chandra, Chill of
    Compliance); a condition on the created token (Yenna, Redtooth
    Regent), on an impulse-exiled card (Bruse Tarl, Daredevil), and on
    a revealed, chosen or countered card's mana value (Part in
    Friendship, Quag Feast, Soul Search, Unravel, Whiskervale
    Forerunner, Break Out); comparisons with other creatures (Getaway
    Glamer, Evil's Thrall); "if this creature isn't saddled. Otherwise"
    in a trigger (Caustic Bronco); "if a player controls no creatures"
    and "each opponent chooses a creature they control and exiles it"
    (Sothera, the Supervoid); "as long as you own a card in exile that
    has an Adventure" (Howling Galefang); excess damage from a fight or
    noncombat damage (The Last Agni Kai, Magmatic Galleon); "if the gift
    was promised and that creature isn't legendary" with "except it's
    1/1" (Coiling Rebirth).
  - Costs: removing counters from among several permanents (Iron
    Spider, Quilled Greatwurm, Dawnhand Dissident; lore counters from
    any number of Sagas, Garnet); "Tap X untapped artifacts" (Secluded
    Starforge); sacrificing one or more artifacts for mana (Radiant
    Lotus) or exiling them with total mana value X (Fabrication
    Foundry); "Sacrifice an artifact or discard a nonland card"
    (Bullseye); non-mana kicker with "twice that much damage instead"
    (Chocobo Kick); "exile a creature you control" as an additional
    cost (Fear of Abduction); flash "if you pay {2} more" (Asinine
    Antics) or if you behold a Dragon (Molten Exhale); "choose a
    creature type and behold two creatures of that type" (Celestial
    Reunion); behold's older "choose a creature you control or reveal a
    creature card" with the chosen creature's power (Monstrous
    Emergence); X in an additional waterbend cost (Foggy Swamp Visions)
    and waterbend as a permission's cost (Hama); convoke for an
    activated ability (Heirloom Epic); "cost an additional 3 life" for
    opponents' spells targeting it (Terror of the Peaks); warp with a
    life cost and warp from a graveyard (Timeline Culler); a cost
    change by equipped creature's power (Glamdring, Foe-hammer), colored
    and per count (Eluge, the Shoreless Sea, also "for as long as it has
    a flood counter"), or for plotting (Doc Aurlock); an alternative
    cost granted to all spells (Leyline of Mutation, Conspiracy
    Unraveler); "pay {0} rather than pay the mana cost" once each turn
    for a spell from exile (Charred Foyer // Warped Space).
  - Mana: "whenever you cast a spell using mana produced by this"
    (Tecutlan, The Myriad Pools, Barracks of the Thousand); a trigger on
    the kind of mana made (Ultima); mana replacement (Virtue of
    Strength, Ozai); a spell-made "whenever you tap a Mountain for mana"
    until end of turn (Molten Tide); mana from a Cave spent (Bat
    Colony); mana equal to mana spent, at the next main phase (Mana
    Sculpt); "can't be spent to cast spells from your hand" (Heartwood
    Crafter); "Put any number of permanent cards from your hand onto the
    battlefield" (Redshift); "one mana of any of the exiled cards'
    colors" (Pit of Offerings); restricted mana from a triggered add
    (Ashling, Rekindled); "it gains rebound" and a land's "Remove a time
    counter from this land" (Ojer Pakpatiq).
  - Digs: a take that is a number expression (Stargaze); two takes
    (Choco, Seeker of Paradise); one card of each of two kinds (Ojer
    Kaslem); a total mana value over the cards taken (Michelangelo's
    Technique; without targets, Lively Dirge); "put all ... from among
    them into your hand" (Seek Thrills, Gleam of Death); "exile a
    nonland card from among them" and one card per card type (The Key to
    the Vault, Portent of Calamity); an impulse's `instead` (Burning
    Curiosity); a conditional destination (Back for Seconds); "they" for
    the group a chosen act moved (Random Encounter); a card exiled and
    plotted (Make Your Own Luck); alternating picks (Memories
    Returning); two target players each revealing (Parker Luck); "You
    may reveal a creature card ... and put it onto the battlefield"
    (Loot, Exuberant Explorer).
  - Casting from exile and graveyards: casting from among cards looked
    at or exiled (Cosmic Cube, Kotis, the Fangkeeper, Black Cat, Cunning
    Thief), one exile per color then casting them (Sanar, Innovative
    First-Year), exile three, choose one, play it this turn (Case of the
    Burning Masks), exiling a hand and playing the cards (Hex Magic);
    "the discarded card" cast (Malcolm, Alluring Scoundrel); "the
    exiled card" permissions after an optional action (Reno and Rude,
    Black Widow, Super Spy); one-card or turn-long permissions
    (Raphael, Superior Foes, Maralen), each opponent's own permission
    with a tax (Lightstall Inquisitor), a cast paying from the pile
    (Boiling Rock Rioter, Kylox's Voltstrider); casting a graveyard card
    as an effect resolves, paying its costs or free from an opponent's
    graveyard (Bilbo, Thief in the Night; Tinybones, the Pickpocket;
    Flotsam // Jetsam); one permanent spell of each type each turn
    (Muldrotha, the Gravetide); one creature spell from the graveyard
    this turn with a finality counter and an added type (Tarrian's
    Journal // The Tomb of Aclazotz); land mayhem and "enters from a
    graveyard" (Oscorp Industries); "a permanent spell with equal or
    lesser mana value" free from the hand (Kellan, the Kid); plot on
    the top card of the library (Fblthp, Lost on the Range); "If a
    permanent spell is countered this way, exile it instead" (Thranduil's
    Decree); one target per card type and copies cast under a total mana
    value cap (Uldaros Theorix, The Tale of Tamiyo); comparisons between
    cards exiled this way (Triple Triad); an until-end-of-turn "exile it
    instead" replacement made by a trigger (Walk-In Closet // Forgotten
    Cellar).
  - Linked exile: moving cards exiled with this to graveyards, counted
    "this way" (Pinnacle Starcage); "for each of those cards that are
    still exiled" (Dragonhawk, Fate's Tempest); "the exiled card's owner
    manifests dread" (Unidentified Hovership); an X/X token for the
    exiled card's owner (Severance Priest); copies of a linked card
    attacking each opponent (Mardu Siegebreaker), until an Aura leaves
    (Secret Invasion), or Shards becoming copies until the next end step
    (Niko, Light of Hope); exiled creature cards' activated abilities
    granted (Agatha's Soul Cauldron); "the last chosen card"'s abilities
    (Koh); the craft card's mana value (Jade Seedstones); a copy "for as
    long as this Equipment remains attached" (Assimilation Aegis);
    "that have different names" with exiled-with (Omenpath Journey);
    stash counters and cards you don't own (Tinybones).
  - Copies, types and names: a name set by a static or an effect
    (Witness Protection, Honest Work, Tenth District Hero, The
    Irencrag); a supertype from `Effect::Animate` (Origin of
    Spider-Man); "in addition to its other types" on a returned
    permanent or group (Infernal Vessel, Ghost Vacuum; Relive the Past,
    also a shared "from your graveyard"); a returned card that becomes
    another type and loses its others (Vraska, the Silencer; Supper for
    Spiders; Brilliance Unleashed); "becomes the chosen color" (Puca's
    Eye); a chosen type in other zones (Leyline of Transformation); a
    once-a-turn replacement of token creation by copies (Mirrormind
    Crown, Moonlit Meditation); "for each of them" over a batch of
    entering tokens (Kambal, Profiteering Mayor); copy exceptions per
    mode (Ember Island Production) or changing equip costs (Firion,
    Wild Rose Warrior, also "Equipped creatures you control"); "a copy
    of the blighted creature" (Grub, Storied Matriarch); "repeat this
    process" (Sin, Spira's Punishment; Calamity, Galloping Inferno);
    "enter as a copy ... with mana value less than or equal to the
    amount of mana spent" (Mockingbird); the Mutavault token (Mutable
    Explorer); transform-on-pay and a back-face emblem with a chosen
    type (Oko, Lorwyn Liege); transforming a modal double-faced card and
    web-slinging granted to spells (Peter Parker // Amazing Spider-Man);
    "This spell can't be copied" (Choreographed Sparks); "those spells
    gain wither" for a spell and its copy (Spinerock Tyrant); a
    multi-face card with no prompt pack (Eddie Brock, not tried).
  - Targets and filters: one target across zones (Taskmaster,
    Mercenary Mimic; "target permanent or suspended card": Fury Charm,
    Shivan Sand-Mage, Timebender, Timecrafting); a target card in exile
    (The Darkness Crystal, Blade of the Swarm, Sentinel of Lost Lore); a
    card from the defending player's graveyard (Chorale of the Void) or
    any graveyard (Victor, Valgavoth's Seneschal); `target permanent of
    chosen color` (Pentarch Paladin); "has a basic land type" (Perilous
    Forays, Nervous Gardener, Sprouting Goblin, Boseiju, Slimefoot's
    Survey); "has an Adventure" (Edgewall Inn; Beluna Grandsquall,
    whose cost change works but has no wording); "kicked spell"
    (Lullmage's Familiar, Merfolk Falconer, Roost of Drakes, Elfhame
    Druid, Throne of Makindi, Volshe Tideturner); spells by controller
    (Counterflux, Hope-Ender Coatl, Hullbreaker Horror); target
    "activated ability, triggered ability, or noncreature spell"
    (Louisoix's Sacrifice); all opponents' spells and abilities and a
    count of them (Glen Elendra's Answer); "creatures you control but
    don't own" (Laughing Jasper Flint); "creatures that don't have a
    name" (Pompous Gadabout); an or-filter of a subtype and a name
    (Slime Against Humanity); "enchanted by an Aura you control" in a
    static (Archon of the Wild Rose, Eriette of the Charmed Apple); a
    filter compared with life gained this turn (Enlightened Confidant);
    "greatest power" in a dies trigger (Kraven the Hunter); "that Wolf"
    for the triggering creature (Tolsimir, Midnight's Light); a group
    as the damage source (Case of the Gateway Express); "each player who
    doesn't have max speed" (Outpace Oblivion).
  - Players and turns: controlling another player (Construct a Cosmic
    Cube, The Dominion Bracelet, Secret of Bloodbending); skipped turns,
    additional upkeeps and end steps (Ral Zarek, Obeka, Y'shtola); "don't
    lose the game for having 0 or less life", "you lose the game" and
    "Whenever you lose life" (Marina Vendrell's Grimoire); a maximum hand
    size from a count (Winter, Misanthropic Guide); another player
    amassing (Azog); protection from each of your opponents on a
    creature (Figure of Fable); "they may pay 3 life" for the dying
    creature's controller (Meathook Massacre II); "that player" for a
    destroyed target's controller (Volatile Fault); "Each player may put
    any number of creature cards from their hand" (Worlds Within
    Worlds); "For each opponent who can't" (Aclazotz, Deepest Betrayal);
    "Starting with you, each player chooses up to one permanent" and
    airbend (The Legend of Yangchen); "target opponent reveals X cards
    from their hand" (Taster of Wares); an opponent gaining control of
    your permanents (Coveted Falcon); "each creature attacks each combat
    if able and attacks a player other than you" until your next turn
    (Maximum Carnage).
  - Put and return: a mass return to the hand (Wisdom of Ages); "Choose
    up to two" with target-only modes and a shared return (Call Damage
    Control); a same-name search that draws for each card exiled from
    the hand (The End, Deadly Cover-Up); "library and/or graveyard"
    with X additional counters (Vision Quest) and "hand and/or library"
    (Last Light of Durin's Day); "If that card is an enchantment card, it
    enters tapped and attacking" (Summoner's Grimoire); a sacrificed
    source's counters and attachments (Zack Fair); "return it" for what
    a destroyed target became (Come Back Wrong); "one that hasn't been
    chosen" and a return under an opponent's control (Zuko,
    Conflicted); "exile all cards from <zone>" with no target (Leveler,
    Identity Crisis, Lantern of the Lost, Bottled Cloister).
  - Other effects: "exile those tokens when this leaves" (Mysterio,
    Master of Illusion); hexproof from a chosen color (Mondo Gecko); an
    indefinite quoted "attacks each combat if able" (Carnage, Crimson
    Chaos; Terminal Velocity); "put an additional +1/+1 counter" (Toph,
    Hardheaded Teacher); "put that many plus one" made until end of turn
    (Prairie Dog) and counter doubling on players (Innkeeper's Talent);
    "loses all abilities for as long as" after countering (Tishana's
    Tidebinder); granted riot and "abilities can't be countered"
    (Spider-Punk); a chosen face-up turn (Hustle // Bustle); "Manifest
    dread X times" (Valgavoth's Onslaught); the discovered card's mana
    value and another player discovering (Hit the Mother Lode, Zoyowa's
    Justice); suspect forms (Airtight Alibi, Eliminate the Impossible,
    Frantic Scapegoat); the source and a chosen creature as one group
    (Fortune, Loyal Steed); "Lock or unlock a door" (Marina Vendrell);
    the noncombat damage replacement up to its power (Ojer Axonil); a
    variable mode count (Riku of Many Paths); tiered modes that are only
    a chosen base power and toughness (Vincent's Limit Break);
    transform returns held by other parts: lore counters on a token copy
    and two-mana adds (Terra, Magical Adept), a Saga gaining a quoted
    mana ability (Huatli, Poet of Unity), "That land becomes an Island"
    after earthbend (The Legend of Kyoshi), exile and put onto the
    battlefield transformed if cast from a graveyard (Esper Origins).
  - Wording: "Return all artifact or creature cards" for "each artifact
    and creature card" (Fix What's Broken); "that spell's mana value"
    (Ovika, Enigma Goliath); "in hand" (Resonating Lute); a token's
    first quoted ability and a token key that is a card's name (Roxanne,
    Starfall Savant); a quote in a three-part list (The Pride of Hull
    Clade); the graveyard-cast rider's sentence order (Hundred-Battle
    Veteran); "Return to the battlefield target ..." (Squirming
    Emergence); two target checks as one sentence (Alacrian Armory);
    "a 4/4 Gorgon creature with deathtouch and lifelink in addition"
    (Case of the Gorgon's Kiss); a permanent static and a spell grant in
    one sentence (Lo and Li, Twin Tutors); "Other Ninja and Rogue
    creatures" (Silver-Fur Master, Narfi, Betrayer King); the name
    repeated in an attack trigger (Drakuseth, Maw of Flames).
  - Flavor-word cards still skipped for their other sentences (each
    names its own feature in `mtg-cards author --verbose`): Spider-Ham,
    The Emperor of Palamecia, G'raha Tia, Magitek Scythe, Aerith Rescue
    Mission, Klaw, Edgar, Omega, Emet-Selch, Diamond Weapon, Dion,
    Terra, Shocker, Lightning, Superior Spider-Man, Malboro, Hawkeye,
    Sephiroth, Blitzball, The Unbeatable Squirrel Girl, Quistis Trepe,
    Daredevil, Zenos yae Galvus, Falcon, Sidequest: Play Blitzball,
    Captain Mar-Vell, Symbiote Spider-Man, Prompto Argentum, Cecil, Dark
    Knight's Greatsword, Shadow of the Goblin, Flash Thompson, Tifa's
    Limit Break, and the cloak cards (Expose the Culprit, Etrata, Hide
    in Plain Sight, Vannifar).
  - Not in the Scryfall snapshot: Ultron, Nick Fury, Tony Stark, Tokka &
    Rahzar, Teo, Seasoned Cryomancer, Ojer Taq, Master's Guide-Mural.
- **Mechanics not yet in:** harness, cloak, epic,
  offering, companion, mutate, the Everywhere land token;
  "Each Sliver card in each player's hand has slivercycling {3}" (an
  ability granted to cards in hands, Homing Sliver); "has flying and
  trample if it devoured a creature" (Hellkite Hatchling).
- **"This creature or equipped creature" as a leaves or dies trigger's
  subject** still sees only the equipped creature, not the source (the
  attack, block, and damage forms see both). No card known.
- **Left by the thirteenth wave's wording branch:** "return another
  permanent card" other than the one sacrificed this way (Deadly Brew);
  a filter on `those targets` (Lost in the Maze); "any other target"
  than the enchanted creature (Pain for All); a bite to each of two
  other targets (Betrayal at the Vault); a discard printed before its
  "if you do" (Oildeep Gearhulk); "a land card milled this way" and a
  trailing "if a Lesson card is milled this way" (Sparring Dummy);
  "with one or more counters on it" on a dies filter with "that many"
  (Shadow Urchin); the source and a target shuffled into libraries in
  one clause (Floodpits Drowner); "is all colors" for a group (Leyline
  of the Guildpact); "you may have <face name> deal that much damage"
  (Jennifer Walters); The Spot, Unexpected Request, Torch the Tower.
  Not attempted: Relive the Past, Rat King, Repulsive Mutation,
  End-Blaze Epiphany, Saw, Choreographed Sparks, Archaic's Agony,
  Alacrian Armory, and the Pauper and Modern report lists.
- **Duplicated this wave:** "no maximum hand size for the rest of the
  game" exists twice (`PlayerRuleKind::NoMaxHandSize` from players13,
  `Effect::NoMaxHandSize` from universes13): collapse to one.
- **Scenario runner, thirteenth wave:** `activate "X"` on a land with several
  abilities silently picks the first, its mana ability (Restless Spire
  needed `ability 2`); `battlefield A: "Derelict Attic
  // Widow's Walk"` places the Room's root split card, which has no
  doors and isn't a legal "target Room you control" (unlock in the
  scenario instead; whether a Room put onto the battlefield by an effect
  gets the both-locked definition is untested); a `choose "X"` line
  doesn't steer manifest dread (Threats Around Every Corner manifested
  the other card; Conductive Machete's `choose` works); "your first,
  second, or third turn" (`PlayerState::turns_begun`) from a set-up
  position counts turns as if the players had alternated from turn 1
  (Starting Town).
- **Scenario runner, twelfth wave:** `illegal ... choose "X"` never
  fires at a library search's decision; `illegal activate ... x N
  targeting` passes when X has no legal target; a "cast from exile"
  offer is always accepted; a bare token name takes the first token of
  that name ("Spirit" is the 0/0).
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

- **Throughput after the thirteenth wave.** Twenty branches and no
  perf branch: the mono-red mirror fell 4.4% (4,148 to 3,967 games/s,
  same run), and random:3 v random:4 went from 612 to 454 games/s a
  core. The random decks changed with the corpus (725 decisions a game
  against 630), so that is about 15% a decision, not 26%. Not profiled.
  Suspects: `ObjFilter` grew to 208 bytes (four new `StatusQuals`
  fields and `name_unlike`; clippy's large-variant lint is silenced on
  `TriggerObj`, `SacCost` and `Replaces`), per-object histories and six
  more tally kinds, the spell-keyword and graveyard-grant checks on the
  priority listing, and whatever new card makes `may_need_mid_decision`
  snapshot. Measure merge by merge as the twelfth wave's perf branch
  did (`git log --merges 613ed113..`).
- **Throughput after the twelfth wave.** The eleventh period (two
  refactorings and fourteen branches) added 6.4% instructions to a
  mono-red mirror game; measured merge by merge, the largest parts were
  a state-based mark at the start of every turn, the shared cast check
  on the priority listing, and player-level statics checked on casts
  and draws. The twelfth wave's perf branch took back 8.2% on the
  mirror and 36% on random decks (a linked exile had made every action
  snapshot the whole state: `GameState::clone` was 41% of a random
  game). The hazard stays: `may_need_mid_decision` reads global counts,
  so one card anywhere can make every action snapshot. Left:
  `priority_options` is 29% of a mirror game (`mana_sources` copies its
  cached list on every hit, `castable` builds a whole `CostBasis` per
  hand card: a per-card "plain cast" bit would skip most); in random
  games snapshots are still about 20%, layer passes and trigger
  detection 20% each; `eot_grants` still counts activated-ability
  grants for trigger detection.
- **Throughput after the tenth wave.** Waves seven to nine cost 5% on
  the mono-red mirror without adding instructions (callgrind counts in
  `mtg_sim::play` were flat from 9dfb5c71 to c537ba2b): the loss was
  instruction-cache and data-cache misses and branch mispredicts, from
  hot functions carrying more rarely used code, and a start-up heap of
  434 MB against 179 MB. The tenth wave's perf branch cut allocations
  and instructions on the paths every game runs (17% fewer on the
  mirror, 8% fewer for random:3 v random:4), which put the mirror above
  the pre-first-wave binary. Left: random-deck games spend a third of
  their instructions in layer passes, most of it trigger detection
  asking for every permanent's characteristics while abilities are
  granted, right after a `cbump`; mid-event snapshots are another 10%
  there; `characteristics` is 13% of a mirror game (an 80-byte copy per
  hit); the state-based-action pass scans each permanent's abilities
  twice (`CardDef::enchant()`); `mana_sources` misses its cache a third
  of the time. Start-up is 0.70 s (0.36 s before the first wave): a
  quarter page faults, a quarter malloc; the fix is a lower peak
  (compile each card as it is parsed), which `compile_order` and the
  by-name map stand in the way of.
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
  The twelfth wave had one collision (two branches took `DefFlags` bit
  29). Both words have since been widened: `DefFlags` is a u64 and
  `GameObject::marks` a u16 (bits 10 and 12 newly taken, `skip_untap`
  moved to bit 8).
- **The template author copies statements from corpus cards whose
  rendered line matches**, so a wrong-meaning card (section 2) spreads
  its form to every card with the same line: when a section-2 entry is
  fixed, rerun `mtg-cards author` for the cards written from it.
  Legendary sorceries had no legend check until the thirteenth wave;
  other per-type casting rules may be missing the same way.
- **The census scripts** (`scripts/coverage/census/`) still name Pauper
  and have not been run over the union of the three pools.
