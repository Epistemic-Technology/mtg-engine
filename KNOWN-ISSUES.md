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
  "cards in exile" counts it. Left open: the fix needs the front faces'
  sum without giving the back face a mana cost.

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
- **Found in the thirteenth wave, in cards that are in** (the fourteenth
  wave's bugs branch left these open: no corpus card does the wrong
  thing, or the fix is a mechanism of its own):
  - Several spells cast "from among" exiled cards (`Effect::CastAmong`:
    Etali, Improvisation Capstone, Doom Reigns Supreme) are offered one
    card at a time and cast in exile order, not the player's order;
    castability is judged for all before the first is cast, so one that
    loses its only target to an earlier one is dropped as its cast
    begins. The fix is a decision loop in the middle of the resolution.
  - "Lands you control enter untapped" always wins (The Wandering
    Minstrel): the player can't order the replacements (CR 616.1) to
    have a land enter tapped. No card known to want that; replacement
    ordering by the player is the missing mechanism.
  - Mill and "one or more" draw replacements (The Water Crystal, Quantum
    Riddler) see only `mill` and `draw_n`; other draw paths are not
    audited. Mulligan draws don't use `draw_n`, but no permanent exists
    to see them.
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
  - "Card types among spells you've cast this turn" (April O'Neil,
    Hacktivist) reads the cast cards' printed types, not the spells as
    they were on the stack. Only an effect changing a spell's types
    would differ.
  - Several cards put onto the battlefield at once enter one at a time,
    each seeing the earlier ones enter (CR 603.6a; Ghalta, Stampede
    Tyrant, Rise of the Dark Realms, Push the Limit, "return all"). The
    fix is a simultaneous-entry mechanism.
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
  - Rowan's Grim Search puts the two cards back on top in the order they
    were; the player doesn't choose (as the other "any order" digs).
  - Reveal-until digs (`dig N until`) don't count for "cards revealed
    this way". No card asks.
  - A dig's "instead" sentence is placed by string search
    (`dig_alt_clause`); a base text with neither ". " nor "the rest"
    would print it last. None known.
  - "Deals 4 or more damage" (Spinneret and Spiderling) is judged per
    recipient: combat damage split among several recipients doesn't add
    up. The fix is a batched damage trigger summing one combat damage
    event across recipients.
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
- **Found in the fourteenth wave, in cards that are in:**
  - A mana multiplier ("it produces three times as much of that mana
    instead", `Replaces::TappedForMana`: Nyxbloom Ancient, Mana
    Reflection, Virtue of Strength) is foreseen by payment only when each
    mana the permanent makes is of one known kind. A dual land is tapped
    as one mana and the extra goes into the pool afterwards, so a spell
    that needs the extra mana can't be cast with it; an activated mana
    ability making one of several kinds, tapped while paying, gets no
    extra mana; restricted mana from a `mana ... only to` ability is not
    multiplied.
  - "Whenever you cast a spell using mana produced by this"
    (`SpellFrom::ManaFromThis`: Tecutlan, Barracks of the Thousand) sees
    only the permanents payment tapped for the spell: mana made with it
    earlier (by hand, or kept in the pool) and then spent doesn't count.
  - "Those creatures" after an attack trigger (Sabotage Strategist,
    Garruk, Curse Breaker; `GameState::note_attackers`) are the
    creatures attacking that player as the ability resolves, so one
    removed from combat in response isn't pumped; they should be the
    attackers as declared.
  - Per-object "the first time this turn" (Captain America, Living
    Legend; Stalwart Successor; `GameState::object_events`) counts only
    while its card is on the battlefield: a creature tapped before
    Captain America entered, then tapped again, counts that tap as its
    first.
  - Geralf, the Fleshwright's "for each other Zombie that entered this
    turn" is the filtered entered count less one: if the triggering
    Zombie stopped being a Zombie before resolution, one too few is
    counted.
  - A bare `pay {..}` outside `you may` (the Pacts: "pay {2}{B}. If you
    don't, you lose the game") is paid automatically whenever the
    player can; the player is never asked (`Effect::Pay` outside
    `YouMay`).
  - Token copies from Moonlit Meditation and Mirrormind Crown are made
    from the permanent's copiable definition (`GameObject::def`): a copy
    exception it has (`except`) or an X/X size (`sized`) is not carried
    over (`tokens_to_create` returns definitions only).
  - "Same name" filters (`SameWhat::Name`) compare definitions, not
    names: two permanents an effect named the same (two Humble
    Merchants) don't share a name, and a set name equal to a real card's
    doesn't match that card's "named" filter. The legend rule and the log
    use the set name. No card known to care.
  - Once a renaming or "is legendary" effect has been in the game
    (`GameState::renamed`, sticky), `name_of` reads characteristics: a
    caller asking a name between a change and its `cbump` caches stale
    characteristics (the debug build's stale-cache check catches it).
  - `enchanted by aura you control` (`StatusQuals::aura_yours`, Archon of
    the Wild Rose, Eriette of the Charmed Apple) matches nothing outside
    a static ability's affected set and an attack ban: a target or group
    with it would need the judging player passed to `status_ok`.
- **Engine picks, fourteenth wave:** a card with an alternative cost of
  its own is offered only that one, not a permanent's granted one too
  (Force of Will under Leyline of Mutation), and of several permanents
  granting one (Leyline of Mutation, Conspiracy Unraveler) the first is
  used; a card cast "from your graveyard" or "from each opponent's
  graveyard" by an effect paying its costs (Bilbo, Thief in the Night;
  Flotsam // Jetsam) is chosen from the cards castable now, and its rider
  is combined with any other permission's for that card (the most
  generous wins); a once-each-turn graveyard permission
  (`GraveRider::once`) ranks after one without a limit, so a second
  source's unlimited permission is used first (CR 601.3 lets the player choose); "Then
  choose an opponent" (Memories Returning) takes the one opponent.
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
  - An "If you do" after a target check (`if target 1 is ...`) reads the
    check, not an earlier optional action. No card has that order.
- **Found in the twelfth wave:** `discard that much cards` after "you
  may draw cards equal to its power. If you do," renders right; whether
  "that many" is the number drawn is not checked (Horrid Shadowspinner
  left out).
- **Found in the fourteenth wave:**
  - `deals combat damage` with no recipient (`TriggerSpec::DealsDamage`
    with `DamageTo::Any`) makes "it" the object dealt the damage (none
    when a player was), not the creature that dealt it: Quilled
    Greatwurm's `whenever creature you control deals combat damage
    during your turn: put that much +1/+1 counters on it` gates Match
    and puts no counters. The card is left out.
  - `they gain haste until end of turn` after `create N "..."` in a cast
    trigger prints "They gain haste until end of turn." but `they` is
    `TriggeringThey`, the triggering spell, so the tokens don't gain
    haste. `those tokens gains haste` works (Ovika, Enigma Goliath). A
    refusal of `they` as an object in a trigger about one spell would be
    safer; corpus cards with `; they gain` after a create in a trigger
    are not audited.
  - The imperative `sacrifice N <filter> chosen` offers "done", and
    unscripted the first option sacrifices nothing even when the
    sacrifice is mandatory ("sacrifice that much permanent chosen" in a
    lose-life trigger). `you sacrifice N <filter>` (`SacrificeChosen`)
    works. Corpus cards using the `chosen` form for a mandatory
    sacrifice are not audited.
  - A group of `other` tokens (`group_into`) excludes every token the
    resolution created, not only the one the sentence names: "Create two
    tokens. Then exile all other tokens" would keep both. No card known.
  - `it is <types> ... and loses other card types` uses `BecomeTypes`,
    which keeps Land: a returned Dryad Arbor stays a land. No test card.
  - A trigger's object filter with a count bound ("with mana value equal
    to this creature's power") now restricts in a cast trigger only;
    other trigger kinds don't fill the bound. None known in the corpus.
  - `StaticCondition::ThatLife` (`if target player life ...`) never
    holds in a static ability, as `NotTheirTurn` didn't before it was
    refused there. `EffectCond::Did` means something only inside
    `Effect::IfAny`; `decompile` of an `Instead` with it, and of a
    `StaticCondition::TriggerIs` from a hand-built definition, is
    `unreachable!`.
  - The corpus line index replaced every digit of a statement through
    the amount map, so a line with amount 1 written into a card with
    amount 2 turned `that target 1` into `that target 2` (fixed in the
    fourteenth wave). Cards the twelfth and thirteenth waves wrote
    through the index with two targets and a 1 among their amounts are
    not audited; most would render differently and fail the gate.
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
  Enemy), and "once each turn" permissions (Zaffai, One with the
  Multiverse).
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
  "whenever you discover" and "discover again for the same value".
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
- **Other effects:** return all land cards from your graveyard to the
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
  own" (Gonti, Canny Acquisitor); gaining the activated abilities
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
- **Left by the fourteenth wave's branches** (merged from the thirteenth
  and fourteenth waves' notes, less the cards now in the corpus; each
  card is listed once, under the form that blocks it):
  - Triggers: "whenever a creature attacks you or a planeswalker you
    control" (Jace, Reality Sculptor, also empower, "exile all but the
    bottom card of each opponent's library" and a condition on loyalty
    among Jaces; its "until your next turn" delayed trigger is in); one
    trigger on two events, "When this enters and at the beginning of
    your upkeep", is a one-line parse addition (`and at beginning` in
    `Parser::trigger`) left for want of a card: Ashling, Rekindled is
    held by it alone; the per-object crew history "a Vehicle crewed by
    this creature this turn", "if it's the first combat phase of the
    turn" and an additional combat phase after a payment (Balthier and
    Fran); "it" for the creature that dealt damage in a trigger with no
    recipient (Quilled Greatwurm, section 2); a damage trigger from "an
    instant or sorcery spell you control that targets only a single
    creature" (Imodane, the Pyrohammer); "when you next activate an
    exhaust ability", copying it, and "Spend this mana only to activate
    abilities" (Pit Automaton); a batched trigger's cards as "them"
    (Hedge Shredder: `join_batch` only counts); "When you search your
    library this way" (Prishe's Wanderings: `Effect::Search` sets no
    `did` when it finds nothing); filtered reflexive triggers of other
    verbs: "When you put one or more Equipment onto the battlefield this
    way", attaching one to a Samurai (Gilgamesh, Master-at-Arms), "When
    excess damage is dealt ... this way" (Rhino's Rampage), "When one or
    more Equipment become attached to that creature this way" (Thorin),
    "When you exile a nonland card this way" (Hidetsugu, Devouring
    Chaos); "deals damage to one or more creatures" (Briarbridge Patrol,
    whose condition is in); "At the beginning of enchanted player's
    first upkeep each turn" (Paradox Haze) and "each of enchanted
    player's postcombat main phases" (Shadow of the Second Sun: the
    engine has the extra phase, no DSL form); a delayed trigger at "that
    turn's end step" for an extra turn (Chance for Glory, Alchemist's
    Gambit); a fully unlocked Room (Victor, Valgavoth's Seneschal); any
    bending action and "all four this turn" (Avatar Aang // Aang, Master
    of Elements, also a colored cost reduction); single-card events,
    each its own kind: Kaya, Spirits' Justice (batched exile over
    battlefield and graveyard, a temporary copy), Fblthp, Impossibly
    Lost (opponents dealt combat damage during your turn, win the game),
    Way of the Mind Sculptor (loyalty removed to activate), Nicanzil
    ("explores a land card"), Stolen Uniform (a lose-control delayed
    trigger), Taii Wakeen (damage equal to toughness, a "plus X"
    replacement), Market Gnome (exiled while activating craft),
    Wilt-Leaf Liege (a discard by an opponent's effect), Feral Encounter
    (a next-combat delayed trigger with targets), Dalkovan Encampment
    (tapped and attacking tokens from a delayed trigger), Tomik
    (attackers counted at you and your planeswalkers).
  - Counts: a group's total power (Kylox, Visionary Inventor; The
    Skullspore Nexus, also "double target creature's power"); a
    per-recipient amount (Sunspine Lynx); the difference between two
    chosen creatures' powers (Spry and Mighty); base power as a
    characteristic (Sovereign Okinec Ahau; Kutzil, Malamet Exemplar); a
    per-type count to a maximum in a static (Diligent Zookeeper); color
    pairs (Niv-Mizzet, Guildpact); planeswalker types and "proliferate X
    times" (Tam, the Possibility); "with different powers" (Rip, Spawn
    Hunter), "eight different names among unlocked doors" (Central
    Elevator // Promising Stairs) and "unlocked doors among Rooms you
    control" (Smoky Lounge // Misty Salon); seven or more cards in exile
    (Ketramose, also its trigger); "eight or more permanents named ~"
    (Phoenix Fleet Airship); the sacrificed object's mana value
    (Repurposing Bay, Sidisi) or power, putting up to X lands from the
    hand (The Gitrog, Ravenous Ride); the enchanted creature's
    last-known power in a dies trigger (Avatar Destiny, also "put up to
    one creature card milled this way onto the battlefield"); the
    source's own mana value inside a quoted ability (Terminal Velocity);
    the triggering spell's X (Geometer's Arthropod) and blue symbols in
    its cost (Namor the Sub-Mariner); "exile cards equal to its mana
    value from target player's library" (Rakdos, the Muscle); the
    web-slinging cost's returned creature (Scarlet Spider, Ben Reilly);
    "where X is the amount of mana spent to cast this creature" after a
    target (Astelli Reclaimer); counters given to a spell being cast, by
    colors of mana spent (Wildgrowth Archaic) or "for each" on "your
    first Human creature spell each turn" (Torgal, A Fine Hound); cards
    put on top after surveil (Starving Revenant); "Add {R} for each card
    exiled this way" with a play permission (Quintorius Kand); a count
    over exile and graveyards with an or-filter of a subtype and a name
    (Slime Against Humanity).
  - Turn tallies: "a non-Zombie creature died this turn" as a condition
    (the filtered died count works) and a graveyard-cast rider "If you
    do, this creature enters with a +1/+1 counter" (Undead Sprinter);
    exhaust activations (Elvish Refueler); players dealt combat damage
    by this creature (Steel Hellkite); "if a Pirate and a Vehicle
    attacked this combat" (Fearless Swashbuckler); "saddles Mounts and
    crews Vehicles as though its power were 2 greater" on a token
    (Cloudspire Coordinator; its count is in); three kinds of "first
    spell this turn" and "have target opponent draw a card. If you do,
    copy that spell" (Alania, Divergent Storm); the wording "You can't
    cast ~ during your first, second, or third turns of the game" (the
    condition exists: Jace Reawakened, also "it becomes plotted";
    Spider-Man 2099).
  - Conditions: "if they lost life this turn" of a target player (a
    "that player" `TurnWho`) and "they reveal their hand, you choose a
    nonland card from it, and they discard that card" (Thought-Stalker
    Warlock); the most life or tied for most life, as an attack trigger
    and a static (Preacher of the Schism); "If X is three or more" about
    a count X (Obsessive Pursuit: `if x 3 or more` is the cast X; its
    two-event trigger is the parse addition above); "if its controller
    has three or more poison counters" of a target's controller (Anoint
    with Affliction, Bring the Ending); a trailing "or if {G}{W} was
    spent to cast this spell" (Mythos of Nethroi: `either` has only the
    leading form); "three or more different kinds of counters"
    (Hundred-Battle Veteran, also its finality-counter sentence order);
    "Then if it has no oil counters on it" (Archfiend of the Dross:
    renders "there are no oil counters on this creature"); "If that
    spell is a Lesson" (Toph, Hardheaded Teacher); "this way" checks: a
    Cave put onto the battlefield (Spelunking), an artifact put (Oviya,
    Automech Artisan, also "attacking one of your opponents"); a
    condition on the created token (Yenna, Redtooth Regent), on an
    impulse-exiled card (Bruse Tarl, Daredevil), and on a revealed,
    chosen or countered card's mana value (Part in Friendship, Quag
    Feast, Soul Search, Unravel); comparisons with other creatures
    (Getaway Glamer, Evil's Thrall); "if this creature isn't saddled" in
    an effect and "that much life" naming the sentence before's amount
    (Caustic Bronco); "if a player controls no creatures" and "each
    opponent chooses a creature they control and exiles it" (Sothera,
    the Supervoid); excess damage from a fight or noncombat damage (The
    Last Agni Kai, Magmatic Galleon); "if the gift was promised and that
    creature isn't legendary" with "except it's 1/1" (Coiling Rebirth).
  - Costs: removing counters from among several permanents for a cast
    from exile with this (Dawnhand Dissident: the cost part is `remove 3
    counters from among creature you control`, the permission is
    missing); "Tap X untapped artifacts" (Secluded Starforge);
    sacrificing one or more artifacts for mana with "Target player adds"
    (Radiant Lotus) or exiling them with total mana value X (Fabrication
    Foundry); a -X loyalty cost (Chandra, Chill of Compliance); a joint
    optional "pay {4}{U}{U}{B}{B} and exile eight cards from your
    graveyard" (Ultimecia, whose transform-into trigger exists); flash
    only when an optional behold is paid (Molten Exhale); "choose a
    creature type and behold two creatures of that type" (Celestial
    Reunion); behold's older "choose a creature you control or reveal a
    creature card" with the chosen creature's power (Monstrous
    Emergence); "you may pay {1} up to three times" (Hawkeye, a
    flavor-word card); convoke for an activated ability (Heirloom Epic);
    warp with a life cost and warp from a graveyard (Timeline Culler); a
    cost change by total toughness (The Pride of Hull Clade), colored
    and per count (Eluge, the Shoreless Sea, also "for as long as it has
    a flood counter"); "Spells you cast of the chosen type cost {1}
    less" and "a card of the chosen type" (Gathering Stone).
  - Mana: "for as long as that land has a blight counter on it, it loses
    all land types and abilities and has ..." (Ultima, Origin of
    Oblivion; its {C} trigger is in); mana replacement and keeping or
    recoloring unspent mana, and counts on it (Ozai, the Phoenix King;
    Omnath, Locus of All; Omnath, Locus of Mana; Upwelling); "it
    produces {U} instead of any other type" (Deep Water, Harvest Mage,
    Damping Sphere: `Replaces::TappedForMana` only multiplies); "If that
    mana is spent on a creature spell, it gains haste" with no duration,
    or "it gains riot" (Hall of the Bandit Lord, Domri, Chaos Bringer,
    Arena of Glory); "Spend only mana produced by basic lands /
    creatures to cast this spell" (Imperiosaur, Myr Superion); "Spend
    this mana only to cast kicked spells" (Elfhame Druid, Throne of
    Makindi, Volshe Tideturner: `restriction_allows_spell` ignores
    status qualifiers, and payment would need to know the spell is
    kicked); mana equal to mana spent on a target spell, at the next
    main phase (Mana Sculpt); "it gains rebound" and a land's "Remove a
    time counter from this land" (Ojer Pakpatiq).
  - Digs: "put all ... from among them into your hand" (Glamdring,
    Foe-hammer; Seek Thrills and Gleam of Death are not in the
    snapshot); one card matching either of two filters (Garruk's
    Harbinger: `and` is "and/or", one of each); a per-card `instead` on
    a target group's return (Back for Seconds); two target players each
    revealing (Parker Luck).
  - Casting from exile and graveyards: casting from among cards looked
    at or exiled (Cosmic Cube; Kotis, the Fangkeeper, also "where X is
    the amount of damage dealt"; Black Cat, Cunning Thief, with "exile
    two of them face down"; The Key to the Vault, Portent of Calamity),
    one exile per color then casting them (Sanar, Innovative
    First-Year), exile three, choose one, play it this turn (Case of the
    Burning Masks, whose solve condition is a count), exiling a hand and
    playing the cards until the end of your next turn (Hex Magic:
    `MayCastExiled` is cast-only, this turn); "the discarded card" cast
    (Malcolm, Alluring Scoundrel); "the exiled card's owner may cast it"
    (Spell Queller); "the exiled nonland card" with a postpositive
    "until end of turn" (Black Widow, Super Spy); one-card or turn-long
    permissions (Raphael, Most Attitude; Superior Foes of Spider-Man;
    Maralen, Fae Ascendant), each opponent's own permission with "Each
    land played this way enters tapped" (Lightstall Inquisitor); collect
    evidence 6 as an activation cost linking the cards, and "put it on
    the bottom of its owner's library instead" (Kylox's Voltstrider);
    casting the exiled card by waterbending (Hama); an until-end-of-turn
    Omniscience (Chandra, Flame's Catalyst); a spell cast from the
    graveyard gaining a quoted ability (Rivaz of the Claw); "by
    sacrificing a creature in addition to paying its other costs" on a
    once-each-turn graveyard permission (Maestros Ascendancy:
    `GraveRider` has life and forage only); a cast from exile with
    "creatures you control but don't own" (Laughing Jasper Flint); one
    permanent spell of each type each turn (Muldrotha, the Gravetide);
    one creature spell from the graveyard this turn with a finality
    counter and an added type (Tarrian's Journal // The Tomb of
    Aclazotz); land mayhem and "enters from a graveyard" (Oscorp
    Industries); "a permanent spell with equal or lesser mana value"
    free from the hand (Kellan, the Kid); plot on the top card of the
    library (Fblthp, Lost on the Range); "If a permanent spell is
    countered this way, exile it instead" (Thranduil's Decree); one
    target per card type and copies cast under a total mana value cap
    (Uldaros Theorix, The Tale of Tamiyo); comparisons between cards
    exiled this way (Triple Triad); an until-end-of-turn "exile it
    instead" replacement made by a trigger (Walk-In Closet // Forgotten
    Cellar).
  - Linked exile: "for each of those cards that are still exiled" in a
    delayed end-step trigger, with "where X is the number of creatures
    you control with power 4 or greater" (Dragonhawk, Fate's Tempest);
    putting the creature cards exiled with this onto the battlefield
    with a flying counter, "Each of them is a 1/1 Spirit in addition to
    its other types" (Ghost Vacuum); "Otherwise, you may put a face-up
    exiled card they own into their graveyard" (Binding Negotiation);
    exiled creature cards' activated abilities granted (Agatha's Soul
    Cauldron); "the last chosen card"'s abilities (Koh); the craft
    card's mana value (Jade Seedstones; also Saheeli's Lattice, Sunbird
    Standard, Throne of the Grim Captain, which need more: craft records
    no exiled cards); stash counters and cards you don't own (Tinybones,
    Bauble Burglar).
  - Copies, types and names: copies of a linked card attacking each
    opponent (Mardu Siegebreaker), until an Aura leaves (Secret
    Invasion), Shards becoming copies until the next end step (Niko,
    Light of Hope), a copy "for as long as this Equipment remains
    attached" (Assimilation Aegis), "becomes a copy of target creature
    card exiled with it, except it's 0/0 and has this ability" with an
    as-enters exile from a graveyard (Mimeoplasm, Revered One; its
    target exists); token copies of exiled cards (Foggy Swamp Visions);
    "becomes a copy of that spell until end of turn" (The Everflowing
    Well // The Myriad Pools; its "using mana from this" trigger is in);
    copies until your next turn (Taskmaster, Mercenary Mimic); "gains
    <two abilities, one of them equip> and loses all other abilities"
    (The Irencrag; the name and supertype work); "They are Food
    artifacts" with its reminder and "creature cards in your opponents'
    graveyards that were put there from the battlefield this turn"
    (Supper for Spiders); "Return it to the battlefield if it's an
    artifact creature card. Otherwise, ..." (Brilliance Unleashed); a
    chosen type in other zones (Leyline of Transformation); "for each of
    them" over a batch of entering tokens (Kambal, Profiteering Mayor);
    copy exceptions per mode (Ember Island Production) or changing equip
    costs (Firion, Wild Rose Warrior, also "Equipped creatures you
    control"); "a copy of the blighted creature" (Grub, Storied
    Matriarch); "repeat this process" (Sin, Spira's Punishment;
    Calamity, Galloping Inferno); "enter as a copy ... with mana value
    less than or equal to the amount of mana spent" (Mockingbird); the
    Mutavault token (Mutable Explorer); transform-on-pay and a back-face
    emblem with a chosen type (Oko, Lorwyn Liege); transforming a modal
    double-faced card and web-slinging granted to spells (Peter Parker
    // Amazing Spider-Man); "those spells gain wither" for a spell and
    its copy (Spinerock Tyrant); a multi-face card with no prompt pack
    (Eddie Brock, not tried).
  - Targets and filters: one target across zones ("target permanent or
    suspended card": Fury Charm, Shivan Sand-Mage, Timebender,
    Timecrafting, Venser's Diffusion, Dust of Moments; graveyard or
    exile: Sorceress's Schemes; battlefield and graveyards: Angel of
    Serenity, Aurelia's Vindicator); a card from the defending player's
    graveyard put tapped and attacking (Chorale of the Void, also the
    Void "sacrifice ... unless"); `target permanent of chosen color`
    (Pentarch Paladin); "has a basic land type" works now, other
    sentences not retried (Boseiju, Who Endures; Slimefoot's Survey);
    "has an Adventure" in a cost change (Beluna Grandsquall, which has
    no wording); "any other target" than the enchanted creature (Pain
    for All: `another target any` excludes the source and is refused);
    "up to that many target creatures" in a reflexive trigger after
    "discard any number of cards" (Miasma Demon); a `renowned` qualifier
    (Enshrouding Mist); "if it shares a color with a creature you
    control" (Jaded Response); "each permanent you control that's a
    creature or Vehicle" (Ao, the Dawn Sky: `or` is read as a second
    effect); all opponents' spells and abilities countered and a count
    of them (Glen Elendra's Answer); a filter compared with life gained
    this turn (Enlightened Confidant); "greatest power" in a dies
    trigger (Kraven the Hunter); "that Wolf" for the triggering creature
    and "if Tolsimir attacked this combat" on another creature's trigger
    (Tolsimir, Midnight's Light); a group as the damage source (Case of
    the Gateway Express).
  - Players and turns: controlling another player (Construct a Cosmic
    Cube, The Dominion Bracelet, Secret of Bloodbending, Mindslaver,
    Sorin Markov, Worst Fears); other "lose the game" forms (Immortal
    Coil, Phage the Untouchable, Lich's Mastery) and "all damage is
    dealt to you as though its source had infect" (Phyrexian Unlife);
    "The next time a source of your choice would deal damage to you this
    turn, prevent that damage" (Intervention Pact); an `eon` counter
    (Magosi, the Waterveil: `CounterKind::NAMED` is fixed-length); "the
    attacking player gains control of this artifact and untaps it"
    (Contested Game Ball, whose trigger and point counters are in);
    another player amassing (Azog); protection from each of your
    opponents on a creature (Figure of Fable); "they may pay 3 life" for
    the dying creature's controller (Meathook Massacre II); "that
    player" for a destroyed target's controller (Volatile Fault); "Each
    player may put any number of creature cards from their hand" (Worlds
    Within Worlds); "For each opponent who can't" (Aclazotz, Deepest
    Betrayal); "Starting with you, each player chooses up to one
    permanent" and airbend (The Legend of Yangchen); "target opponent
    reveals X cards from their hand" (Taster of Wares); an opponent
    gaining control of your permanents (Coveted Falcon); "each creature
    attacks each combat if able and attacks a player other than you"
    until your next turn (Maximum Carnage).
  - Put and return: "Choose up to two" with target-only modes and a
    shared return (Call Damage Control); a same-name search that draws
    for each card exiled from the hand (The End, Deadly Cover-Up);
    "library and/or graveyard" with X additional counters (Vision Quest)
    and "hand and/or library" (Last Light of Durin's Day); "If that card
    is an enchantment card, it enters tapped and attacking" (Summoner's
    Grimoire); a sacrificed source's counters and attachments (Zack
    Fair); "return it" for the card a destroyed target became and "If a
    creature card is put into a graveyard this way" (Come Back Wrong);
    "one that hasn't been chosen" and a return under an opponent's
    control (Zuko, Conflicted); "exile all cards from" a target player's
    hand and graveyard and a count of them (Identity Crisis; Kaya, Geist
    Hunter; Ashiok, Nightmare Weaver; Worldfire; Bottled Cloister),
    "return all cards named ~" (Demigod of Revenge); "Put any number of
    permanent cards from your hand onto the battlefield" (Redshift,
    Rocketeer Chief); "Choose a permanent type." before a mass return
    (Creeping Renaissance); Wrenn and Seven's +1 and Treefolk token.
  - Other effects: "the creature you control deals twice that much
    damage instead" (Chocobo Kick, whose non-mana kicker is in); "For
    each creature your opponents control, create a Cursed Role token
    attached to that creature" (Asinine Antics, whose "flash if you pay
    {2} more" is in); "remove a lore counter from each of any number of
    Sagas you control" with "for each lore counter removed this way"
    (Garnet, Princess of Alexandria); a granted quoted ability with
    "protection from each color" (Sygg, Wanderwine Wisdom); counter
    doubling on players (Innkeeper's Talent); "loses all abilities for
    as long as" after countering (Tishana's Tidebinder); granted riot
    and "abilities can't be countered" (Spider-Punk); the discovered
    card's mana value and another player discovering (Hit the Mother
    Lode, Zoyowa's Justice); "can't become suspected" (Airtight Alibi: a
    `Keyword` would be a merge hazard) and "suspect one of the other
    creatures" (Frantic Scapegoat); the source and a chosen creature as
    one group (Fortune, Loyal Steed); "Lock or unlock a door" (Marina
    Vendrell); the noncombat damage replacement up to its power (Ojer
    Axonil); a variable mode count (Riku of Many Paths); tiered modes
    that are only a chosen base power and toughness (Vincent's Limit
    Break); transform returns held by other parts: a Saga gaining a
    quoted mana ability (Huatli, Poet of Unity), "That land becomes an
    Island" after earthbend (The Legend of Kyoshi), exile and put onto
    the battlefield transformed if cast from a graveyard (Esper
    Origins).
  - Wording: a token's first quoted ability printed without its final
    period (Roxanne, Starfall Savant; its mana trigger and token are
    in); "Return to the battlefield target ... with mana value less than
    or equal to ..." (Squirming Emergence); two type checks in one
    sentence (Alacrian Armory); "a 4/4 Gorgon creature with deathtouch
    and lifelink in addition" (Case of the Gorgon's Kiss); "If you
    sacrificed a permanent this way" and "another permanent card" other
    than the sacrificed one (Deadly Brew); a filter on `those targets`
    (Lost in the Maze); "a land card milled this way" without "from
    among" and a trailing "You gain 2 life if a Lesson card is milled
    this way" (Sparring Dummy); "you may have <face> deal that much
    damage" with "Do this only once each turn" (Jennifer Walters); "put
    each card exiled with it into its owner's hand" (Fear of Abduction,
    whose additional cost gates); "for as long as you control this
    creature" on a legendary card's impulse (Gwen Stacy // Ghost-Spider,
    whose trigger is built); "Surveil 2. Then draw a card for each ..."
    as two sentences, with a turn-limited creature static and an emblem
    (Kaito, Bane of Nightmares); "this artifact" as the self-noun of
    "{0}: This creature gets +3/+3" (Chronatog Totem); "another target"
    for `another target any` (Magma Burst), "where X is this creature's
    power" after the source is named (Nested Shambler), "If a creature
    enters this way" for `if creature` (Recommission); "Tap ... and it
    doesn't untap during its controller's next untap step" without a
    semicolon (Quiet Contemplation) or with a comma before "and"
    (Winterthorn Blessing); creature lands' "until end of turn ... It's
    still a land." with a quoted trigger (Hall of Storm Giants, Lair of
    the Hydra, Cave of the Frost Dragon, Hive of the Eye Tyrant, Den of
    the Bugbear: their enters-tapped sentence is templated now);
    Unexpected Request, Torch the Tower (other forms, as before). Not
    attempted: Rat King, Repulsive Mutation, End-Blaze Epiphany, Saw,
    Archaic's Agony, about 40 more Pauper and 69 Modern `RENDER:` lines.
  - Sentences that don't parse (249 cards over the three pools, grouped
    by phrase in the template branch's notes): relative clauses on a
    target or group: "that dealt damage to you this turn" (Spear of
    Heliod, Giltspire Avenger, Reciprocate, Retaliate), "that convoked
    it" (Venerated Loxodon, Zephyr Singer), "that targets you or a
    creature you control" (Siren Stormtamer, Not of This World), "that
    wasn't cast from its owner's hand" (Wash Away), "that blocked or was
    blocked by" (Time to Reflect, You Cannot Pass!), "that entered since
    your last turn ended" (Premature Burial), "that has a -1/-1 counter
    on it" (Banewhip Punisher), "that doesn't have a +1/+1 counter on
    it" (Coiling Stalker), "that aren't legendary" (Urza's Ruinous
    Blast), "that has an activated ability with {T} in its cost"
    (Magewright's Stone), "that shares a card type with it" (Daring
    Thief), "that entered the battlefield this turn" on a subtype list
    (Oakhollow Village), "is snow or could produce {C}" (Break the Ice),
    "that was activated this turn" (Cut Short), "that were dealt damage
    this turn" (Death-Rattle Oni); "that's ...": an artifact, creature,
    or enchantment (Bounty Agent), attacking you or a planeswalker you
    control (Soul Snare), attacking or blocking (Surge of
    Righteousness), blocking it (Arc Spitter), the second spell cast
    this turn (Second Guess), a Wolf or a Werewolf (Spirit of the Hunt,
    Pallid Mycoderm), attacking alone (Crowd of True Believers); return
    and search filters ("card with awaken", cycling, a kicker ability,
    flashback or disturb, a mana ability, "of an opponent's choice", one
    target per card type: Reconstruct History; 21 cards); spells by
    controller (Drown in the Loch); divided X with a "where X is"
    (Monstrous Onslaught, Magmatic Core, Undercity Upheaval, Quirion
    Beastcaller) and three targets with three amounts (Incremental
    Growth, Incremental Blight); target lists of kinds (Tale's End,
    Spider-Sense, Consign to Memory, Thraben Exorcism); "if it ..."
    checks on a countered target (Hisoka, Minamo Sensei, Equinox,
    Wretched Banquet, Corrupted Resolve, Nix, Unified Will); "unless its
    controller" does something other than pay (Grip of Amnesia, Perplex,
    Protect the Negotiators, Concerted Defense, Thassa's Intervention)
    or "has ~ deal N damage to them" (Blazing Salvo, Combustion Man,
    Dwarven Scorcher); "except for" in a group (Flame Sweep, Elspeth
    Tirel, Scourglass); "who" clauses on players (Creeping Dread,
    Bonder's Ornament, Rix Maadi Guildmage); "whose" clauses (Gilt-Leaf
    Winnower, Lullmage's Domination); "with the same controller / mana
    value as" (Fear, Fire, Foes!, Sanguine Praetor, Culling Scales); "1
    plus the sacrificed ...'s mana value" (Enigmatic Incarnation, Prime
    Speaker Vannifar); the power of the creature that entered or died as
    a count (Kresh the Bloodbraided, Hamletback Goliath); the rest are
    single cards.
  - Flavor-word cards still skipped for their other sentences (each
    names its own feature in `mtg-cards author --verbose`): "if at least
    four mana was spent to cast it" (The Emperor of Palamecia, Prompto
    Argentum); dealt 6 or more combat damage this turn, then transform
    and attach (Sidequest: Play Blitzball); flash as long as an opponent
    cast a spell this turn (Captain Mar-Vell); "one or more other
    creatures and/or artifacts you control die", once each turn (G'raha
    Tia); "When you discard this card" (Edgar's Awakening); "for each
    opponent, tap up to one target nonland permanent that opponent
    controls" (Omega); "choose a creature an opponent controls", then
    "creatures other than ~ and the chosen creature" (Zenos yae Galvus);
    "the fourth time this ability has resolved this turn" (Sephiroth);
    "one plus the number of creature cards in your graveyard" cards
    revealed (Klaw); "you lose that much life" after a damage trigger
    and half the starting life total (Cecil); "up to one enchantment
    card milled this way" (Terra, Magical Adept, also lore counters on a
    token copy and two-mana adds); "look at that many cards" now
    renders, "It gains this card's other abilities" doesn't (Symbiote
    Spider-Man); "you may exile the top card of your library" and play
    it (Daredevil); enter as a copy of a creature card in a graveyard
    with exceptions (Superior Spider-Man); "Put a stun counter on one of
    them" (Aerith Rescue Mission); one subject with three verbs
    (Malboro); "that creature ... must be blocked this turn if able"
    after an optional attach (Magitek Scythe); a legendary token with a
    quoted ability (Falcon); double damage to a player and their
    permanents until your next turn (Lightning, Army of One); a token
    and a bloodline counter, transform (Edgar, Charmed Groom); coin
    flips come up heads (Edgar, King of Figaro); cloak (Expose the
    Culprit, Etrata, Deadly Fugitive, Hide in Plain Sight, Vannifar).
  - Not in the Scryfall snapshot: Ultron, Nick Fury, Tony Stark,
    Seasoned Cryomancer, Master's Guide-Mural, Seek Thrills, Gleam of
    Death.
- **Mechanics not yet in:** harness, cloak, epic,
  offering, companion, mutate, the Everywhere land token;
  "Each Sliver card in each player's hand has slivercycling {3}" (an
  ability granted to cards in hands, Homing Sliver); "has flying and
  trample if it devoured a creature" (Hellkite Hatchling).
- **"This creature or equipped creature" as a leaves or dies trigger's
  subject** still sees only the equipped creature, not the source (the
  attack, block, and damage forms see both). No card known.
- **Scenario runner, fourteenth wave:** `illegal activate ... x N
  targeting ...` passes for a wrong X when the ability's targets are
  chosen before X (`scripted_target_refused`): a scripted X the engine
  doesn't offer falls back to the first option instead of counting as
  refused, so a scenario can't show "with mana value X" refusing a
  target (Gelatinous Cube; the engine filters X through `x_fits`).
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

- **Throughput after the fourteenth wave.** Measured merge by merge in
  game-only instructions (`perf stat -e instructions:u` of `--games
  N+1` less `--games 1`, steady to 0.05% under load; games/s on this
  machine with sixteen agents compiling meant nothing), the thirteenth
  wave cost the mono-red mirror 2.5% (2,116,055 to 2,169,673
  instructions a game); the 4.4% throughput drop was that plus layout
  noise. Two merges were most of it: costs-mana13's red-mana loop at
  every step end (+35.7k) and turn-tallies13's characteristics bump on
  every first damage (+16.3k: `pool.history` is a pool flag, so on for
  the whole corpus). The fourteenth wave's perf branch fixed both, gated
  the target listing behind activated abilities' cost discounts
  (`pool.equip_discounts` is on for the corpus, so every targeted
  ability listed its targets at every priority: 12% of a random game,
  from wording12), gated snapshots, gave statics layer bits, borrowed
  cached mana sources instead of copying them, and stopped cloning a
  filter alternative. Against main at 01bb8644 the mirror runs 2.0%
  fewer instructions (2,169,634 to 2,126,651, still above the 2,116,055
  before the thirteenth wave) and random:3 v random:4 29.7% fewer
  (18,502,576 to 13,004,839). It also fixed a cache bug: the mana-source
  cache shared one slot among every player beyond the first two, so in
  a game of three or more `mana_sources(2)` right after
  `mana_sources(1)` returned player 1's sources (players beyond two are
  now not cached, `mana_key`). Since copies-names14 a static "is
  legendary" or a set name makes `GameState::renamed` sticky, and from
  then on every action snapshots for the legend rule's question. Left
  for later: pool-level flags (`equip_discounts`, `card_grants`,
  `spend_grants`, `stack_grants`, `history`) are on for the whole
  corpus and want game-level counts (a `DefFlags` bit or a per-game scan
  at setup); about 19k snapshots per 500 random games remain ("you may"
  triggers at the resolving pass, inherent, and
  `own_graveyard_replacers`, about 4.5k); in random games the priority
  listing is 37% inclusive, the layer pass 22% (`characteristics_miss`
  12.5%, `battlefield_chars` 10%, `gather_statics` 7%: a per-object
  layer mask per pass would skip more), `detect_triggers` 16%,
  `GameState::clone` 11% (about 50 snapshots a game of 124 objects),
  `ability_plan` and `ability_plan_tapping` 5% each, with
  `activation_allowed` and `ObjFilter::matches` behind; `run` is 10%
  exclusive in the mirror; `mana_sources` misses 39% of calls in the
  mirror (its key has the global `tap_epoch` and `cversion`; a
  per-player tap epoch would keep the other player's list); `castable`
  per hand card; `CardPool::intern_filter` compares filters linearly at
  load (35M instructions, not per game). Re-bench games/s on a quiet
  machine.
  Quiet-machine games/s after the wave's merges (pinned, best of five):
  mirror 4,037, against 3,985 after the thirteenth wave and 4,158
  before it; random:3 v random:4 680 a core against 457 and 615 (the
  random decks differ between the three binaries).
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
  `GameObject::marks` a u16. After the fourteenth wave `DefFlags` bits
  32, 33, 38, 42, 50 and 51 are taken, `GameObject::marks` bits 0-8, 10
  and 12, and `SpendWays` bit 17.
- **The template author copies statements from corpus cards whose
  rendered line matches**, so a wrong-meaning card (section 2) spreads
  its form to every card with the same line: when a section-2 entry is
  fixed, rerun `mtg-cards author` for the cards written from it. Since
  the fourteenth wave the corpus line index also substitutes colors,
  card types, subtypes, keywords and counter kinds one to one, so a
  wrong-meaning form spreads across its word shape, not only its number
  shape: rerun the author for cards written from the same line with
  other words too.
  Legendary sorceries had no legend check until the thirteenth wave;
  other per-type casting rules may be missing the same way.
- **The census scripts** (`scripts/coverage/census/`) still name Pauper
  and have not been run over the union of the three pools.
