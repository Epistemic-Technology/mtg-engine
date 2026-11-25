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
- **Found in the fifteenth wave, in cards that are in:**
  - Flanking granted to a creature that already has flanking triggers
    once, not twice (Cavalry Master on Benalish Cavalry): keyword sets
    don't count instances (CR 702.25b).
  - The aftermath half of Reduce // Rubble cast from a graveyard takes
    no targets, scripted or unscripted; the same text cast from the hand
    scripts fine.
  - A noncreature artifact animated in the declare attackers step, after
    attackers are declared, is not offered a block (Foriysian Totem,
    Guardian Idol); animated at the beginning of combat it is. Reported
    by a card agent; the bugs branch did not reproduce it.
  - Doubling Season with Oath of Gideon: the printed loyalty now joins
    the event's loyalty counter entry in `entering_counters`, so the
    intrinsic doubling (`skip(given)`) skips it, and a planeswalker
    entering under both gets twice 1 plus its printed loyalty instead of
    twice (printed + 1).
  - `they` after an attack trigger is the attackers as the ability
    resolves, as "those creatures" is (above), not as declared.
  - `that player` after a destroy in a spell or activated ability is the
    destroyed permanent's owner (`ThatOwner`), not its controller; only a
    stolen permanent differs (Volatile Fault).
  - A third "This spell costs ... less to cast if ..." line on one card
    would overwrite the second (`CardDef::cond_cost2`). No card known.
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
    trigger or an activated ability prints "They gain haste until end of
    turn." but `they` is a group named before or the triggering spell
    (`TriggeringThey`), not the created tokens, so they don't gain haste.
    `those tokens gains haste` works (Ovika, Enigma Goliath), and the
    template writes it after a `create` since the fifteenth wave; no
    corpus card has `create ...; they gain`. A refusal of `they` there
    would be safer.
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
- **Found in the fifteenth wave** (each card is left out unless noted):
  - Siege Dragon: in `whenever this attacks if defending player controls
    no Wall: it deals 2 damage to each creature without flying that
    player controls`, `that player` is the triggering player, so the
    attacker's controller's creatures take the damage; `... defending
    player controls` renders "defending player controls". Needs a "that
    player" form for the defending player in an attack trigger.
  - Sepulchral Primordial: `that player's graveyard` inside `for each
    opponent, you may put up to 1 target creature card from that
    player's graveyard onto battlefield` is the controller's own
    graveyard.
  - Marsh Casualties: `if kicked, those creatures gets -2/-2 until end of
    turn instead` after `each creature target player controls gets
    -1/-1` binds nothing. `each creature that player 1 controls` works
    but prints "creatures that player controls".
  - Ib Halfheart, Goblin Tactician: in `sacrifice it; if you do, it
    deals 4 damage to each creature blocking it` the sacrificed creature
    has no blockers left; it needs the blockers as they last were.
  - Hostile Hostel // Creeping Inn: a chosen graveyard exile (`you may
    exile 1 creature card from your graveyard chosen`) is not linked to
    the source, so "exiled with this creature" counts nothing.
  - Psychic Intrusion (`exiling nonland card from graveyard or hand; may
    cast that card remains spend any color`) and Kheru Spellsnatcher
    (`counter target spell to exile; may cast that card remains free`):
    no permission to cast the exiled card is granted. Psychic Intrusion
    works with the `split` form.
  - Basri's Lieutenant, Grakmaw, Skyclave Ravager: `whenever ... creature
    you control dies if it had +1/+1 counter` reads the source's
    counters, not the dying creature's. `if it had counters` (any kind)
    is the only form about the dying creature.
  - Radiant Grace // Radiant Restraints: `return this card to battlefield
    transformed under your control attached to target opponent` in a
    dies trigger goes on the stack with no target, and the Aura stays in
    the graveyard.
  - Sanctum Guardian: `this turn, replace damage from chosen source to
    any target: prevent next time` gives the ability a real target;
    without `to any target` it renders "would deal damage this turn".
  - Fear, Fire, Foes!: `each other creature with same controller` after
    `deal X damage to target creature and deal 1 damage to` is not bound
    to the first target's controller, so the caster's creatures are
    damaged too; `controller of target 1 controls` prints "that player
    controls".
  - Vengeful Strangler // Strangling Grasp: in `controller of enchanted
    sacrifices 1 nonland permanent of their choice then that player loses
    1 life`, `that player` is the Aura's controller. `then controller of
    enchanted loses 1 life` renders "then loses 1 life".
  - Vanish into Memory: a target's power or toughness read inside a
    delayed trigger is 0 (`discard count equal toughness of that target 1
    cards`, also `toughness of that creature`), so nothing is discarded.
  - Cyclical Evolution: `exile this; put 3 time counters on this` in a
    spell puts no counters on the exiled card (`this` doesn't follow the
    card into exile), so it is never suspended. It prints as two
    sentences and gates Mismatch; a rendering joining them would make
    it a wrong-meaning form. It needs an "exile this with N time
    counters" effect (Arc Blade and Reality Strobe, section 3).
  - Relentless Dead: `you may pay {X}; if you do, return target ... card
    with mana value X` in a triggered ability returns the target whatever
    X is paid: the target is chosen as the trigger goes on the stack and
    the bound isn't judged again with the paid X. Searches (`with mana
    value X or less`, Taj-Nar Swordsmith) are judged as they resolve and
    work.
  - Silence the Believers: `all Aura attached to target 1` after `any
    number of target creature` prints "and all Auras attached to them"
    but takes only the first target's Auras. The template doesn't write
    it.
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
    creatures to cast this spell" (Imperiosaur, Myr Superion); mana
    equal to mana spent on a target spell, at the next main phase (Mana
    Sculpt); "it gains rebound" and a land's "Remove a time counter from
    this land" (Ojer Pakpatiq).
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
    sentences not retried (Slimefoot's Survey);
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
    the dying creature's controller (Meathook Massacre II); "Each
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
    hand and graveyard and a count of them (Identity Crisis;
    Ashiok, Nightmare Weaver; Worldfire; Bottled Cloister),
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
    one group (Fortune, Loyal Steed); the noncombat damage replacement
    up to its power (Ojer Axonil); a variable mode count (Riku of Many
    Paths); tiered modes that are only a chosen base power and toughness
    (Vincent's Limit Break); transform returns held by other parts: a
    Saga gaining a quoted mana ability (Huatli, Poet of Unity), "That
    land becomes an Island" after earthbend (The Legend of Kyoshi),
    exile and put onto the battlefield transformed if cast from a
    graveyard (Esper Origins).
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
    your last turn ended" (Premature Burial), "that doesn't have a +1/+1
    counter on it" (Coiling Stalker), "that aren't legendary" (Urza's
    Ruinous Blast), "that has an activated ability with {T} in its cost"
    (Magewright's Stone), "that shares a card type with it" (Daring
    Thief), "is snow or could produce {C}" (Break the Ice), "that was
    activated this turn" (Cut Short); "that's ...": an artifact,
    creature, or enchantment (Bounty Agent), attacking you or a
    planeswalker you control (Soul Snare), blocking it (Arc Spitter),
    the second spell cast this turn (Second Guess), a Wolf or a Werewolf
    (Pallid Mycoderm), attacking alone (Crowd of True Believers); return
    and search filters ("card with awaken", cycling, a kicker ability,
    flashback or disturb, a mana ability, "of an opponent's choice", one
    target per card type: Reconstruct History; 21 cards); spells by
    controller (Drown in the Loch); divided X with a "where X is"
    (Monstrous Onslaught, Magmatic Core, Undercity Upheaval, Quirion
    Beastcaller) and three targets with three amounts (Incremental
    Blight); target lists of kinds (Tale's End, Spider-Sense, Consign to
    Memory, Thraben Exorcism); "if it ..." checks on a countered target
    (Hisoka, Minamo Sensei, Equinox, Wretched Banquet, Corrupted
    Resolve, Nix, Unified Will); "unless its controller" does something
    other than pay (Grip of Amnesia, Perplex, Protect the Negotiators,
    Concerted Defense, Thassa's Intervention) or "has ~ deal N damage to
    them" (Blazing Salvo, Combustion Man, Dwarven Scorcher); "except
    for" in a group (Flame Sweep, Elspeth Tirel, Scourglass); "who"
    clauses on players (Creeping Dread, Bonder's Ornament, Rix Maadi
    Guildmage); "whose" clauses (Gilt-Leaf Winnower, Lullmage's
    Domination); "with the same controller / mana value as" (Fear, Fire,
    Foes!, Sanguine Praetor, Culling Scales); "1 plus the sacrificed
    ...'s mana value" (Enigmatic Incarnation, Prime Speaker Vannifar);
    the power of the creature that entered or died as a count (Kresh the
    Bloodbraided, Hamletback Goliath); the rest are single cards.
  - Flavor-word cards still skipped for their other sentences (each
    names its own feature in `mtg-cards author --verbose`): "if at least
    four mana was spent to cast it" (The Emperor of Palamecia, Prompto
    Argentum); dealt 6 or more combat damage this turn, then transform
    and attach (Sidequest: Play Blitzball); flash as long as an opponent
    cast a spell this turn (Captain Mar-Vell); "When you discard this
    card" (Edgar's Awakening); "for each opponent, tap up to one target
    nonland permanent that opponent controls" (Omega); "choose a
    creature an opponent controls", then "creatures other than ~ and the
    chosen creature" (Zenos yae Galvus); "the fourth time this ability
    has resolved this turn" (Sephiroth); "one plus the number of
    creature cards in your graveyard" cards revealed (Klaw); "you lose
    that much life" after a damage trigger and half the starting life
    total (Cecil); "up to one enchantment card milled this way" (Terra,
    Magical Adept, also lore counters on a token copy and two-mana
    adds); "look at that many cards" now renders, "It gains this card's
    other abilities" doesn't (Symbiote Spider-Man); "you may exile the
    top card of your library" and play it (Daredevil); enter as a copy
    of a creature card in a graveyard with exceptions (Superior
    Spider-Man); "Put a stun counter on one of them" (Aerith Rescue
    Mission); one subject with three verbs (Malboro); a legendary token
    with a quoted ability (Falcon); double damage to a player and their
    permanents until your next turn (Lightning, Army of One); a token
    and a bloodline counter, transform (Edgar, Charmed Groom); coin
    flips come up heads (Edgar, King of Figaro); cloak (Expose the
    Culprit, Etrata, Deadly Fugitive, Hide in Plain Sight, Vannifar).
  - Not in the Scryfall snapshot: Ultron, Nick Fury, Tony Stark,
    Seasoned Cryomancer, Master's Guide-Mural, Seek Thrills, Gleam of
    Death.
- **Modern, from the fifteenth wave's card batches** (the 576 cards
  reported unsupported without a `RENDER:` line that still have no card
  file, of about 1,230 tried; mostly Modern, some Pauper and Standard;
  each card is listed once, under the form that blocks it, commonest
  forms first with their count; the agents' full lines are in the wave's
  reports):
  - Triggers: keyword actions and other events with no trigger (17):
    "whenever you surveil" (Blood Operative, Whispering Snitch),
    "whenever you proliferate" (Ezuri, Stalker of Spheres), "whenever
    you investigate" (Erdwal Illuminator), "whenever you get one or more
    {E}" (Fabrication Module, Territorial Gorger), "whenever you create
    a token" (Rosie Cotton of South Lane), "becomes monstrous" (Ember
    Swallower), "trains" (Savior of Ollenbock), "complete a dungeon"
    (Varis, Silverymoon Ranger), "the Ring tempts you" (Sméagol, Helpful
    Guide), "phases in" (King of the Oathbreakers), "becomes unattached"
    (Captain's Hook), "whenever you pay its cumulative upkeep"
    (Hibernation's End, Balduvian Fallen), "mutates" (Essence Symbiote),
    "players finish voting" (Grudge Keeper); damage to you by a filtered
    source (8): "a source an opponent controls deals damage to you"
    (Elesh Norn, Retaliator Griffin; as a prevention or replacement
    source: Guardian Seraph, Channel Harm), "a creature deals combat
    damage to you" (Hixus, Prison Warden, Witch-king of Angmar), "a
    source deals damage to this" with "that source's controller"
    (Belltower Sphinx, Phyrexian Obliterator); other damage sources: a
    spell (Flame Channeler), "a noncreature source you control"
    (Tamanoa), "a source you control other than this" to an opponent,
    combat or not (Talon of Pain), "to one or more blocking creatures"
    (Arashin War Beast), "to a player or planeswalker" (Vraska, Swarm's
    Eminence); becoming a target (6): you as the target (Unsettled
    Mariner, Leyline of Combustion), an opponent's creature targeted by
    your spell or ability (Willbreaker, Shay Cormac, also "loses
    hexproof, indestructible, protection, shroud, and ward"), "a spell
    you control" (Illuminator Virtuoso), "a spell" without abilities and
    "if it entered from your library" (Fblthp, the Lost); attacks and
    blocks: "a creature attacks you or a planeswalker you control"
    (Sarkhan the Masterless; per creature, "without flying": Circle of
    Flame), "an opponent attacks a planeswalker you control" (Oath of
    Kaya), "two or more creatures attack a player" (Horn of the Mark),
    "this and another creature attack different players" (Canal
    Courier), "one or more creatures you control become blocked"
    (Hezrou), "blocks two or more creatures" (Lairwatch Giant); casts:
    "the fourth spell of a turn" by any player (Erayo, Soratami
    Ascendant), "the first multicolored spell each turn" (Zenith
    Chronicler), "a spell they don't own" (Gonti, Night Minister), "with
    the same name as a card in their graveyard" (Dragonlord Kolaghan),
    "a Dragon creature spell from your graveyard" (Rivaz of the Claw),
    "enchanted player casts a spell other than the first each turn or
    copies a spell" (Curse of Shaken Faith); zones: "into an opponent's
    graveyard from the battlefield" (Magus of the Bridge), "into your
    graveyard from your library" (Gaea's Blessing; Grolnok, the
    Omnivore, also croak counters on exiled cards), "into a graveyard or
    exile from the battlefield" (Slagstone Refinery; granted, Ashnod's
    Intervention), "if it was a creature" on leaving (Weatherseed
    Totem), "a player other than its owner controls it" (Bronze
    Bombshell); "lose life for the first time each turn" (Vengeful
    Warchief); "one or more opponents each lose exactly 1 life" (Ob
    Nixilis, Captive Kingpin); a counter kind on any creature
    (Flourishing Defenses); "two or fewer loyalty counters" (Garruk
    Relentless); "When you cast this spell and whenever this creature
    attacks" (Titans' Vanguard); activations: an ability of the
    enchanted artifact without {T} (Artifact Possession), an opponent's
    non-mana ability of an artifact, creature, or land (Immolation
    Shaman), a loyalty ability of a Chandra (Keral Keep Disciples); "a
    spell or ability an opponent controls causes you to discard this"
    (Quagnoth); from the battlefield or the graveyard with a condition
    (Quakebringer); from exile while suspended (Nihilith) and "the last
    time counter is removed" (Riftmarked Knight); "until your next turn,
    whenever either of those creatures deals combat damage" (Tamiyo,
    Field Researcher); "when you next cast an instant, cast a sorcery,
    or activate a loyalty ability this turn" (Repeated Reverberation); a
    spell as the source of "triggers an additional time" (Echoes of
    Eternity); an opening-hand reveal with a delayed trigger on an
    opponent's first spell of the game (Chancellor of the Annex).
  - Counts: another player's zones (7): cards in that player's graveyard
    (Roiling Terrain, Keening Stone), in opponents' hands (Enemy of
    Enlightenment), card types in opponents' graveyards (Nighthawk
    Scavenger), cards a target opponent discarded this turn (Dream
    Salvage), cards opponents drew this turn (Heliod, the Radiant Dawn),
    Curses attached to a player (Curse of Surveillance); arithmetic (9):
    a difference over counts (Skull Raid, Balance of Power), "minus one"
    (Dark Deal), "three times" (Rootwire Amalgam), "20 minus the highest
    life total" (Scourge of the Skyclaves), "your life total minus the
    highest opponent's" with suspend X (Roiling Horror), "3 minus the
    number of cards in their hand" (The Rack), "3 plus the number of
    charge counters" (Lightning Storm), "1 plus the number of counters"
    in a mana-value bound (Obscura Ascendancy); things done this way
    (8): counters removed (Simic Manipulator, Give // Take, Hankyu),
    creatures tapped (Urge to Feed, Siege Striker), cards looked at
    while scrying (Celeborn the Wise), the milled card's mana value
    (Mindshrieker), the colors of the sacrificed creature (Bound //
    Determined); a mana-value bound from a count (Brain in a Jar, Nissa
    of Shadowed Boughs, Sisay, Weatherlight Captain, "less than Sisay's
    power"); a power bound from a count (Ensnaring Bridge); a sacrifice
    count from a count (Din of the Fireherd); the mana value of the
    sacrificed creature plus one in a search (Neoform, Evolving Door);
    poison counters (Mycosynth Fiend; "fewer than nine ... equal to the
    difference", Vraska, Betrayal's Sting); "the number of times this
    ability has resolved this turn" (Bronze Cudgels); green symbols
    among revealed cards (Phosphorescent Feast) and in each creature's
    own cost (Light from Within); creatures that convoked it (Ancient
    Imperiosaur); opponents who control an artifact (Kellan, Daring
    Traveler); each player's own destroyed artifacts (Builder's Bane);
    the greatest number a player discarded (Whispering Madness); cards
    in your library (Enter the Infinite); a count as its own sentence
    with "that number" (Rumbling Ruin); differently named dungeons
    completed (Ellywick Tumblestrum); variable keyword amounts:
    soulshift X (Kodama of the Center Tree), bloodthirst X (Petrified
    Wood-Kin), "an additional creature for each Equipment" (Kemba's
    Legion); "+X/+1" (Cranial Ram); an X/1 token (Urabrask's Forge); a
    variable prevention amount (Cover of Winter, Shield of the Avatar);
    "distribute X counters, where X is" (Undercity Upheaval).
  - Turn tallies: life lost with an amount or another player (6): "an
    opponent lost 2 or more life" (Bloodchief Ascension), "a player lost
    4 or more life" (Knight of the Ebon Legion), "you lost life last
    turn" (First Response), all players' life lost (Tainted Sigil),
    damage dealt to a player this turn (Final Punishment), noncombat
    damage dealt to opponents as a cost reduction (Chandra's
    Incinerator); "a player discarded a card" (The Raven Man); another
    player's entered permanents: an artifact (Baloth Cage Trap), another
    land (Confounding Conundrum); "you played a land" (River of Tears);
    "you didn't play a card from exile" (Visions of Phyrexia); "you
    didn't attack with a creature" (See Red); a named creature attacked
    (Insubordination); "dealt damage by two or more creatures" (Inferno
    Trap); spells cast by name (Sift Through Sands); "an opponent has
    cast a spell" with flash for your spells (Captain Mar-Vell,
    Space-Born); "an opponent lost life" on a play permission (Theater
    of Horrors); "a turn you attacked with a Rogue" (Robber of the
    Rich); "for each opponent who was dealt damage" and "each creature
    blocks this turn" (You've Been Caught Stealing); cards "put there
    from the battlefield this turn" (Faith's Reward, No Rest for the
    Wicked) and "cycled or discarded this turn" (Shadow of the Grave).
  - Conditions: a devotion condition with "isn't a creature" (5: Nylea,
    Keen-Eyed; Nylea, God of the Hunt; Purphoros, Bronze-Blooded;
    Karametra, God of Harvests; Athreos, Shroud-Veiled); exact and upper
    bounds (6): "exactly one creature" (Deadly Wanderings), "exactly
    thirteen permanents" (Stensia Uprising), "If X is 1 / 2 / 3" and "do
    all of the above" (Multiple Choice), "fewer than four +1/+1
    counters" (Ayara's Oathsworn), "library 200 or more" (Battle of
    Wits), half the starting life total (Cecil, Dark Knight); a
    graveyard with twenty cards (Jace, the Perfected Mind); a full party
    (Coveted Prize); two conditions on one static (Grond, the
    Gatebreaker); "enchanted or equipped" (Warchanter Skald, Koll, the
    Forgemaster); "isn't a creature or Vehicle" (Lifecraft Awakening); a
    triggering or dying object's characteristics in an "instead":
    last-known power (Anax, Hardened in the Forge), legendary (Andúril,
    Flame of the West), a Mountain (Akoum Hellkite); its power against
    this creature's (Yorvo, Lord of Garenbrig); the greatest toughness
    among all creatures (Abzan Beastmaster); graveyard order (Death
    Spark); "{U}{B} was spent" as a first sentence (Mythos of Brokkos);
    "madness cost was paid" (From Under the Floorboards); "if you lose
    the flip" (Goblin Kaboomist); raid on "enters as a copy" (Protean
    Raider); "flash if X is 3 or less" (Silver Scrutiny); extra turns
    (Medomai the Ageless); a target spell's controller poisoned
    (Corrupted Resolve); four historic cards in your graveyard (Havi,
    the All-Father); "seven or more lands with different names" with
    dredge granted to lands (The Necrobloom); the triggering player or
    target controller's poison (Anoint with Affliction).
  - Costs: "pay any amount of mana, life, or {E}" (8: Power Leak, Karn,
    Living Legacy, Plunge into Darkness, Chorus of the Conclave, Plague
    of Vermin, Suppression Ray, Chthonian Nightmare); tapping any number
    or X of your creatures (4: Explosive Singularity, Burn at the Stake,
    Mossbridge Troll with total power, Aryel, Knight of Windgrace); "pay
    {..} any number of times" or "up to three times" (4: Spectral
    Adversary, Tainted Adversary, Intrepid Adversary, Hawkeye, Master
    Marksman); several sacrifices or exiles in one cost (5): named
    creatures (Urborg Panther), three colors (Angel's Herald), "this
    land and a creature" (Great Hall of Starnheim), this and four other
    artifacts (Mechtitan Core), "two creature cards from a single
    graveyard" (Night Soil); "Sacrifice X" (Springjack Pasture,
    Chatterfang, Squirrel General); "as this enters, sacrifice any
    number" (Shimatsu the Bloodcloaked, Devouring Hellion); half your
    life (Murderous Betrayal, Temporal Extortion); colored cost
    reductions (3: Even the Score, Stormwing Entity, Morophon, the
    Boundless); a mana tax on spells that target this (Callaphe, Beloved
    of the Sea, Pursued Whale); "exile any number of <color> cards, {2}
    less for each" (March of Otherworldly Light, March of Reckless Joy);
    counters removed from among several permanents (Light Up the Night,
    Galloping Lizrog); non-mana echo, evoke and cumulative-upkeep
    choices (Skizzik Surger; echo {X}, Volcano Hellion; Endurance; Jötun
    Owl Keeper, Arctic Nishoba); other cost kinds: alternative costs in
    one ability (Granite Shard), "discard two artifact cards" (Psychic
    Overload), "unless you discard a noncreature card" (Drekavac), a
    spell from the stack (Nivmagus Elemental), this on the bottom of its
    library (Timestream Navigator), a hand card on top (Leashling), a
    card exiled with this to a graveyard (Void Maw), crew by loyalty
    (Heart of Kiran), an attack cost of two Islands (Leviathan), a block
    tax by power (Hipparion), "remove a counter from a permanent you
    control" as an unless (Chisei, Heart of Oceans), the green card of
    mana value X (Nourishing Shoal), "choose a creature you control or
    reveal a creature card" (Monstrous Emergence), an as-enters return
    with a counter (Rescuer Sphinx); two kickers (Ana Battlemage) and a
    target per kick (Comet Storm); "unless" with something other than
    mana: "sacrifices a creature or pays 3 life" (Indulgent Tormentor),
    "discards two cards or sacrifices" with a repeat (Remorseless
    Punishment), "pays {X}, where X is its mana value" on a sacrifice
    (Soul Tithe), "discards their hand" (Perplex), "has it deal N damage
    to them" (Blazing Salvo, Combustion Man, Risk Factor), "unless that
    player pays {3}" for each object of a group (Cut the Tethers); cost
    changes: disguise (Fugitive Codebreaker), dash (Warbringer), a floor
    of three (Trinisphere), "except during its controller's turn"
    (Defense Grid), "the next Giant spell" (Invasion of the Giants),
    face-down spells (Obscuring Aether), "spells they cast this turn
    cost {2} more" with "that player may draw" (Academy Loremaster).
  - Mana: {S} spent (3: Boreal Outrider, Tundra Fumarole, Blessing of
    Frost); a two-kind restricted ability (The Emperor of Palamecia) and
    a cumulative-upkeep restriction (Adarkar Unicorn); the commander
    rider (Study Hall); colors among cards in a graveyard (The Grey
    Havens) and the source's colors (Katilda, Dawnhart Prime); "that
    player adds" (Magus of the Vineyard); keeping green mana (Fangorn,
    Tree Shepherd); "produces {C} instead" (Damping Sphere); "spend only
    mana produced by Treasures" (Security Rhox); mana at the next main
    phase (Scattering Stroke, Mana Sculpt); "spend mana as though any
    color" for activations (Drana and Linvala).
  - Digs: searching or digging another player's library (7: Sadistic
    Sacrament, Praetor's Grasp, Haunting Echoes, Surgical Extraction,
    Gonti, Lord of Luxury, Black Cat, Cunning Thief, Nashi, Moon Sage's
    Scion); reveal-until forms (7): "or X cards" (Thought Dissector), a
    mana-value filter (Treasure Keeper), "until X land cards" (Mind
    Grind), "lesser mana value" than the cast spell (Jodah, the
    Unifier), for each player (Plargg and Nassari), in place of a cast
    (Possibility Storm), by another player (Spellshift); takes: all
    lands onto the battlefield (Animist's Awakening), any number onto
    the battlefield and the rest into the hand (Genesis Ultimatum), from
    among exiled cards (Tezzeret, Master of the Bridge), an "and/or"
    filter (Kamahl's Druidic Vow), a double-faced filter (Overgrown
    Pest), "{X} in its mana cost" (Paradox Surveyor), the looked-at
    cards to the bottom with "if you do" (Petals of Insight), one exiled
    and played (Florian, Voldaren Scion), an opponent's choice among
    them (Dubious Challenge, Allure of the Unknown), each player's own
    dig with two takes (Wandering Archaic); fateseal (Mesmeric Sliver);
    "exile the top X cards" with a leading "for each" (Culmination of
    Studies); any number with the same name in your library (Secret
    Salvage).
  - Casting from exile and graveyards: flash granted for one spell or
    one turn (3: Scout's Warning, Quicken, Emergence Zone); "you may
    cast spells from your graveyard this turn" (Walk-In Closet); one per
    opponent's graveyard (Diluvian Primordial); an untargeted graveyard
    card cast free (Sword of Once and Future); type-limited permissions:
    "an instant or sorcery spell from among them" (Chandra, Hope's
    Beacon), until your next turn (Nivix, Aerie of the Firemind),
    Equipment free (Nahiri, Forged in Fury); "until your next end step"
    (Riveteers Charm); a quoted static granted to an exiled card (Monk
    Class); a revealed card cast for {1} (Jadzi, Oracle of Arcavios); a
    random or exiled card cast by its owner (Wild Evocation, Omen
    Machine); "when you next cast a spell this turn" with an exile-until
    (Codie, Vociferous Codex); abilities used from exile (Greater
    Gargadon, Torrent Elemental) and statics from the graveyard (Valor);
    splice onto instant or sorcery (Splicer's Skill).
  - Linked exile: copies of cards exiled with this cast free (3: Arcane
    Bombardment, Isochron Scepter, Elite Arcanist); counters on exiled
    cards (Kianne, Dean of Substance; Altaïr Ibn-La'Ahad); still exiled
    at the end step (Dragonhawk, Fate's Tempest); a copy while attached
    (Assimilation Aegis); "until this leaves" for any number (Lumbering
    Battlement); total power of the exiled cards (Sutured Ghoul);
    different mana values among them (Azor's Gateway); a name match
    against them (Mindreaver, Extraplanar Lens, Godsend); protection
    from their card types (Mirror Golem); a token's leaves trigger
    returning one (Ugin, the Ineffable); the cast replacement
    (Quintorius, Loremaster); "each player may play cards exiled with
    this" (Uba Mask).
  - Copies, types and names: "choose a card type" (5: Cloud Key, Alrund,
    God of the Cosmos, Serra's Emissary, Stenn, Paranoid Partisan,
    Vigean Intuition); manifest (5): from looked-at cards (Write into
    Being), from hands (Kozilek, the Broken Reality), by another player
    (Reality Shift), plain (Jeskai Infiltrator), of exiled cards
    (Ghastly Conscription); chosen-name checks (5): "if a card with the
    chosen name is revealed" (Isperia the Inscrutable, Magus of the
    Scroll, Dementia Sliver), a count of them (Mindblaze), "nonartifact,
    nonland card name" (Lost Legacy); a copy for each other creature it
    could target (3: Zada, Hedron Grinder, Mirrorwing Dragon,
    Ink-Treader Nephilim); base power and toughness from another
    creature (3: Shape Stealer, Exuberant Wolfbear, Eldrazi Mimic); a
    group becoming copies (Deceiver of Form, Augmenter Pugilist); a
    chosen opponent and name with a draw trigger (Booby Trap); a search
    of three zones by name (Kassandra, Eagle Bearer); "named this" in
    each graveyard (Myr Servitor) and inside a token (Sound the Call); a
    cast check by name in the graveyard (Pyromancer Ascension); a copy
    of a chosen enchantment (Calix, Guided by Fate); a conditional
    counter in a copy exception (Undercover Operative); "When you do,
    exile that card" after entering as a copy of a graveyard card
    (Superior Spider-Man); this card's other abilities granted (Symbiote
    Spider-Man); an X/X becomes with "X can't be 0" (Lair of the Hydra);
    noting a creature type (Long List of the Ents); a copy made by the
    spell's controller (Meletis Charlatan); partner with and copying an
    exiled card (Jacob Frye); "are the chosen type", replacing types
    (Conspiracy); "isn't legendary if it's a token" (Aeve, Progenitor
    Ooze); a two-color land creature (Emergent Sequence); "ward" inside
    a becomes and "isn't an Equipment" (Dancing Sword); becoming an Aura
    (Cloudform; Bronzehide Lion, returned as one); "an enchantment in
    addition" with a counter (Arbiter of the Ideal); equipping a
    planeswalker that becomes a creature (Luxior, Giada's Gift);
    awakening counters and "8/8 for as long as" (Liege of the Tangle);
    characteristics by coin flip (Molten Sentry); a face-down return and
    "+1/-1 or -1/+1" (Shorecrasher Elemental); face-down 5/5 artifact
    creatures (Tezzeret, Cruel Machinist); the meld partner's static
    "can't cast spells with mana value 3 or less" (Gisela, the Broken
    Blade; Brisela, Voice of Nightmares).
  - Targets and filters: suspended cards (3: Venser's Diffusion, Fury
    Charm, Dust of Moments); relative clauses (10): "that's attacking
    alone" (Crowd of True Believers), "attacking one of your opponents"
    (Martial Impetus), "attacking you or a planeswalker you control"
    (Soul Snare), "that doesn't have a +1/+1 counter on it" (Coiling
    Stalker), "that was activated this turn" (Cut Short), "that dealt
    damage to you this turn" (Giltspire Avenger), "entered since your
    last turn ended" (Premature Burial), a player dealt damage by this
    creature (Wicked Akuba), "blocking enchanted creature" (Coils of the
    Medusa), "that targets only this creature" (Torchling); keyword
    filters (6): eternalize or embalm (Vizier of the Anointed), modular
    (Arcbound Overseer), flashback or disturb (Phantom Carriage), a
    kicker ability (Coralhelm Chronicler), "has an Adventure"
    (Mysterious Pathlighter), "a card with flash" (Check for Traps);
    comparisons between targets or with a sacrificed card (5): "shares a
    card type" (Daring Thief, Fatal Grudge, Spirit-Sister's Call), "mana
    value equal or lesser" (Puca's Mischief), "share no creature types"
    (Rivals' Duel); superlatives: lowest mana value (Culling Scales),
    greatest power you control (Triumph of Gerrard), greatest power in a
    graveyard (Desecrator Hag); "nonhistoric" (Desynchronization); equip
    "legendary creature" (Blackblade Reforged, Brotherhood Regalia);
    "enchant creature with another Aura attached" (Daybreak Coronet);
    target kinds: "instant spell, sorcery spell, or triggered ability"
    (Spider-Sense), "triggered ability or colorless spell" (Consign to
    Memory), "activated or triggered ability you don't control" (Nimble
    Obstructionist), the countered ability's source (Ouphe Vandals), an
    opponent as a counter target (Etched Slith), opponent, creature or
    planeswalker, with player counters (All Will Be One); changing a
    target to this (Muck Drubb); "another target" and "a third target"
    (Cone of Flame); a counter on one of several targets (Aerith Rescue
    Mission, Wicked Slumber); cards in several graveyards (Turn the
    Earth); one target per player (Vaevictis Asmadi, the Dire); targets
    with a total mana value of X from that player's graveyard (The Rise
    of Sozin); "as this enters, choose another creature you control"
    (Dauntless Bodyguard); "each creature except for creatures you
    control with flying" (Flame Sweep); ignoring hexproof (Kaya, Bane of
    the Dead) and "can't have or gain hexproof" (Archetype of
    Endurance); "hexproof from that color" granted (Skrelv, Defector
    Mite); "can't be countered by" and "can't be the target of" colored
    spells (Autumn's Veil); "can't be prevented or dealt instead" (Lava
    Burst).
  - Players and turns: "your life total becomes N" (7: Shaman of
    Forgotten Ways, Angel of Grace, Worldfire, Providence, Repay in
    Kind, Eternity Vessel, Enduring Angel); cards from outside the game
    (7: Turtles Forever, Extrapolate the Impossible, Wish, The Raven's
    Warning, Fae of Wishes, Vivien, Arkbow Ranger, Karn, the Great
    Creator); "you win the game" as an effect (6: Barren Glory,
    Darksteel Reactor, Near-Death Experience with "exactly 1 life",
    Coalition Victory, Revel in Riches; Zenos yae Galvus, when the
    chosen player loses); player statics (12): graveyard locks
    (Grafdigger's Cage, Kunoros, Hound of Athreos, Ashes of the
    Abhorrent), "players can't search libraries" with a payment (Leonin
    Arbiter), "can't pay life or sacrifice creatures to cast" (Angel of
    Jubilation), "you can't play lands" (Aggressive Mining), "you can't
    cast noncreature spells" (Nullhide Ferox), "one nonartifact spell
    each turn" (Ethersworn Canonist), the defending player can't cast
    spells (Wardscale Dragon), one artifact untapped (Imi Statue),
    "Counters can't be put" and "Players can't get counters"
    (Solemnity), lands' activated abilities (Sharkey, Tyrant of the
    Shire); other players acting (12): "target opponent may draw"
    (Soldevi Heretic), "that creature's controller may draw"
    (Fecundity), "you and target opponent each create" (Oath of the Grey
    Host), "each player who controls" (Shatter the Sky, Depopulate),
    "chooses a card and discards the rest" (Monomania), two chosen
    discards (Gruesome Discovery), each player exiles permanents and
    hand cards (Descent into Madness), a chosen order of sacrifices
    (Rite of Ruin), "each creature deals 1 damage to its controller"
    (Rakdos Charm), the triggering creature's controller choosing the
    damage (Pandemonium), reveal and exile by name from a hand and
    graveyard (Legion's End); controlling another player (Worst Fears,
    Emrakul, the Promised End); day and night (Tovolar, Dire Overlord,
    Unnatural Moonrise); skipping steps (Dragon Appeasement, Gibbering
    Descent); piles of permanents (Liliana of the Veil); exchanging
    control of all creatures (Twist Allegiance); random choices (6):
    among targets (Capricious Efreet), a graveyard card by type
    (Deadbridge Chant), a returned card (Kheru Lich Lord), a sacrifice
    with a repeat (Tyrant of Discord), a counter kind (Crystalline
    Giant), coin flips until both win (Mana Clash); repeats: "repeat
    this process" (Professor Onyx, Sphinx's Tutelage), "X times"
    (Exponential Growth); a guess (Gollum, Scheming Guide); extra turns
    by mana value (Timesifter); team play (Azra Bladeseeker); the
    command zone (Campfire); a mulligan action (Serum Powder); loyalty
    abilities twice (The Chain Veil, Urza Assembles the Titans) or
    shared (Kasmina, Enigma Sage); a dynamic cast ban by the triggering
    spell's mana value (Inventive Iteration); "each opponent who
    attacked you" (Unstable Glyphbridge); for each 1 life lost, a choice
    across zones (Lich's Mastery); comparing card types discarded by
    different players (Creeping Dread); an extra turn ending in a loss
    (Alchemist's Gambit); time travel (Wibbly-wobbly, Timey-wimey).
  - Put and return: from every graveyard (Infernal Caretaker; Pyrrhic
    Revival; untargeted, Liliana, the Necromancer; another player's,
    Realmbreaker, the Invasion Tree); "under an opponent's control"
    (Sol'Kanar the Tainted); search onto the battlefield under a target
    player's control (Yavimaya Dryad); Auras attached: from the
    graveyard (Storm Herald), from the hand (Academy Researchers),
    exiled and returned with the creature (Flickerform); onto the
    battlefield blocking (Aetherplasm); "any number of cards from your
    hand on the bottom" (Valakut Awakening); a player's permanents
    shuffled (Teferi, Temporal Pilgrim); Warp World; exchanging zones
    (Harness Infinity, Mordenkainen); the sacrificed card in a delayed
    return (Rescue from the Underworld); "you may put it on top instead"
    (Pulmonic Sliver); a counter on an untargeted creature (Choking
    Miasma); an "and/or" return "each with" mana value X and "If X is 6
    or more, those permanents are 4/4 creatures" (Dance of the Manse); a
    countered spell to its owner's hand (Remand).
  - Other effects: new counter kinds (12: unity, Call for Unity; prey,
    Tetzimoc, Primal Death; mannequin, Makeshift Mannequin; doom,
    Imminent Doom; rejection, Tolarian Contempt; eon, Magosi, the
    Waterveil; ember, Smoldering Egg; study, Grimoire of the Dead;
    enlightened, The Book of Exalted Deeds; influence, Palantír of
    Orthanc; ice, Rimescale Dragon); counters of any kind moved, copied
    or removed (7): moving (Leech Bonder, Goldberry, River-Daughter),
    "for each kind of counter" (Maulfist Revolutionary, Exotic Pets),
    all of one kind (Hapatra's Mark), doubling on players and halving
    (Vorinclex, Monstrous Raider), "that many minus one" -1/-1 counters
    (Vizier of Remedies); keyword grants (7): madness (Falkenrath
    Gorger), scavenge (Varolz, the Scar-Striped), ripple (Thrumming
    Stone), bloodthirst to a spell (Bloodlord of Vaasgoth), battle cry
    (Reckless Pyrosurfer), riot (Rhythm of the Wild), shared keywords
    (Concerted Effort); replacements: an entering creature that wasn't
    cast (Mistcaller, Hallowed Moonlight), life gained becoming life
    lost (Rain of Gore, Tainted Remedy), "this turn, ... enters with an
    additional counter" (Zameck Guildmage), double damage to a player
    and their permanents (Lightning, Army of One), noncombat damage up
    to a power (Ojer Axonil, Deepest Might), "instant or sorcery source"
    (Pyromancer's Swath), coin flips (Edgar, King of Figaro); combat:
    "as though it weren't blocked" (Tornado Elemental), "as though it
    had reach" (Dragon Hunter), "attack <a player> each combat" (Public
    Enemy), "this combat" (Boros Battleshaper), blocked by the monarch's
    creatures (Azure Fleet Admiral); groups dealt damage this way
    (Aurelia's Fury); a group phasing out and "they can't phase in"
    (Teferi, Timeless Voyager); "can't be enchanted by other Auras"
    (Consecrate Land); you and your permanents as one subject gaining
    protection (Faith's Shield); untapping the chosen permanents you
    control and tapping the others (Teferi, Who Slows the Sunset); "the
    next time target creature adapts" (Biomancer's Familiar); "Exile
    this with three time counters" as an effect (Arc Blade, Reality
    Strobe; see Cyclical Evolution in section 2); "that player
    sacrifices" on a destroyed creature's controller with toughness
    (Shriveling Rot); a gain-control delayed unattach (Ogre
    Geargrabber); "for as long as this remains tapped" (Giant Oyster).
  - Not in the Scryfall snapshot: Summon, Sidequest (name ambiguous).
- **Wording near-misses left after the fifteenth wave: 114 cards.** The
  list is regenerated from the batch reports' `RENDER:` lines each wave.
- **Mechanics not yet in:** harness, cloak, epic,
  offering, companion, mutate, the Everywhere land token;
  "Each Sliver card in each player's hand has slivercycling {3}" (an
  ability granted to cards in hands, Homing Sliver); "has flying and
  trample if it devoured a creature" (Hellkite Hatchling).
- **"This creature or equipped creature" as a leaves or dies trigger's
  subject** still sees only the equipped creature, not the source (the
  attack, block, and damage forms see both). No card known.
- **Scenario runner, fifteenth wave:** `illegal turn 1 A: cast "Choose
  Your Weapon" mode 2 targeting "Hill Giant"` passes as legal though the
  mode's target must have flying and the plain cast is refused: with a
  `mode`, `illegal` doesn't test that mode's targets.
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

- **Throughput after the fifteenth wave.** Measured merge by merge as
  before (game-only instructions, `perf stat -e instructions:u` of
  `--games N+1` less `--games 1`; mono-red mirror, 2,000 games, seed
  11), the fourteenth wave cost the mirror 1.3% on top of its own perf
  branch (2,128,313 instructions a game after perf14 to 2,155,994 at
  c8e3f478; 2,169,507 before the wave). costs14 was most of it (+22.7k:
  once `pool.targeted_life` was set, true for the corpus, every spell's
  target choice ran `target_payable` per offered target and
  `targets_life` built a Vec per offered target); players-turns14 added
  5.9k (step and turn bookkeeping in `run` and `sba_loop_inner`),
  copies-names14 3.3k (`may_need_mid_decision`); perf14 took back 41.9k
  and targets-filters14 7.1k. The fifteenth wave's perf branch made
  seven changes: random:3 and random:4 frozen into deck files (`--deck1
  file:<path>`) so the corpus can grow under them; the snapshot gate
  passing a linked return to hand and Rooms and adventurers without a
  legend; targeted life running no `target_payable` without a target
  cost, and no Vec; `GameObject::clone` skipping the copy of empty
  SmallVecs; the snapshot gate checking replacements only when two share
  an event kind; per-object static layer masks in the layer pass; a
  per-player tap epoch in the mana cache. Against main at c8e3f478 the
  mirror runs 1.7% fewer instructions (2,155,891 to 2,119,011, below the
  2,169,507 before the fourteenth wave), random:3 v random:4 6.0% fewer
  (12,632,681 to 11,870,859; the frozen deck files 12,186,887 to
  11,164,458) and mono-red v mono-green 2.0% fewer (2,337,074 to
  2,289,358). Snapshots in 20 random games halved (1,761 to 940;
  `GameState::clone` from 19% of a game to 10%). Wins lines and fuzz
  output were byte-identical to main for every commit. Games/s at load
  19 meant nothing (3,322 main, 3,247 branch); the last quiet-machine
  figures are after the fourteenth wave (mirror 4,037, random:3 v
  random:4 680 a core). Left for later: snapshots are still 10% of a
  random game (about 25k instructions each: "you may" resolutions,
  inherent; land plays with a replacer in play; two replacers of one
  kind); `GameObject` is 136 bytes, and under 128 the clone's copies
  might inline, which means boxing `ability: Option<StackAbility>` (32
  bytes) across many files; the layer pass is about 26% of a random game
  inclusive (`static_masks` computed twice a pass, layer 6 scanning
  every object, `Work::matching` about 2.2k a call, `CharMap::new` and
  malloc each pass); `targets_life` still runs per offered target (about
  2.4k a mirror game: a game-level "some permanent has TargetedLife"
  gate would remove it; `pool.targeted_life` must stay for
  `cost_checked`); `priority_options` grew about 21 instructions a call
  in costs14; `next_may_not_untap` looks up characteristics for every
  tapped permanent at each untap step; the pool flags `card_grants`,
  `spend_grants`, `stack_grants`, `alt_grants` and `plot_discounts` are
  still on for the corpus (paired with state counts, none hot);
  `detect_triggers` (5% exclusive in random games) and
  `ObjFilter::matches` (4%) are untouched; checking only the top stack
  object saved 0.07% and was left out. Cache and branch misses were not
  measured. Re-bench games/s on a quiet machine.
  Quiet-machine games/s after the wave's merges (pinned, best of five):
  mirror 4,120, against 4,046 at the wave's start and 4,186 before the
  thirteenth wave. random:3 v random:4 reads 594 a core against 684,
  but the random decks changed with 600 more cards in the corpus; on
  the frozen deck files the perf branch measured 6.0% fewer
  instructions.
- **Card batches on never-tried cards.** The fifteenth wave's 32 Sonnet
  batches tried about 1,230 cards no agent had tried (127 Pauper and
  Standard, the rest Modern) against frozen binaries: 385 accepted
  (31%), from 1 of 32 (3%) to 19 of 40 (48%) a batch. 6 were held back
  at integration (3 no longer matched the gate, 3 failed the format
  round trip; wording-e fixed the three and they are in). 249 were
  wording near-misses (`RENDER:`), a fifth of all tried and about a
  quarter of each batch's unaccepted cards; 135 of them are in now. 20
  failed (19 wrong-meaning forms or engine bugs, sections 1 and 2; one
  name not in the snapshot), and the rest were unsupported (section 3).
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
