# Scenario file syntax

Scenario files (`*.scn`) hold rules tests: set up a board, script actions at
points in the turn structure, run the engine, and assert on the result. The
runner drives the engine through its ordinary decision interface, so every
scripted action is checked against the engine's own list of legal options.
An action that is not legal fails the scenario; so does an action whose point
in the game is never reached.

Players are `A` (index 0) and `B` (index 1). Comments start with `#` or
`//`. Card names are quoted. One statement per line.

```
scenario "name" {
  <setup lines>
  <scripted actions, run_to, and assert lines in any order>
}
```

## Setup

| Line | Meaning |
|---|---|
| `life A 15` | Starting life (default 20). |
| `poison B: 2`, `energy A: 3` | Starting poison or energy counters (`assert poison B 2`, `assert energy A 2` check them). |
| `battlefield A: "Grizzly Bears", 2 "Forest", "Hill Giant" tapped, "Flight" on "Grizzly Bears", "Phantom Tiger" with 2 +1/+1` | Permanents A controls. `N "name"` repeats; `tapped` taps; `on "name"` attaches an Aura to that permanent (either player's); `with N +1/+1` (or any counter kind: `-1/-1`, `+1/+0`, `-0/-1`, `oil`, `arrowhead`, ...) puts counters on it. Permanents count as controlled since the turn began, so they may attack. Timestamps follow listing order, player A first. |
| `hand A: "Lightning Bolt"` | Cards in hand. |
| `graveyard A: ...`, `exile A: ...` | Likewise. |
| `library A: "Top Card", "Next Card"` | The library, top card first, in exactly this order. Without this line the player gets 40 Plains so draws succeed. `library A:` with nothing gives an empty library. |
| `start turn 2 B precombat_main` | Where play begins: turn number, active player, and step (default `untap`). Default `start turn 1 A`. There are no mulligans; the starting player skips the first draw as usual. |
| `seed 7` | RNG seed (only matters if something random happens: shuffles, die rolls, coin flips, cards chosen at random). The same seed gives the same outcomes, so a scenario picks one for the result it tests. |
| `skip "reason"` | Parse but do not run (for adjudicated disagreements). |

The same zone may be listed more than once; the lines accumulate.

## Scripted actions

```
turn <N> [<step>] <player>: <action>
illegal turn <N> [<step>] <player>: <action>
```

An action fires the first time its player is offered a matching decision
during that turn and step. `illegal` inverts the check: the action must
*not* be among the legal options when the point is reached; the default
choice is taken instead. Either way the point must be reached, or the
scenario fails. An `illegal` cast or activation with `targeting` also
passes when the action is legal but one of the scripted targets is not,
or is never asked for (checked by taking the action on a copy of the
game, with the scripted X).

| Action | Default step | Notes |
|---|---|---|
| `play "Forest"` | `precombat_main` | Play a land from hand. |
| `cast "Shock" targeting "Hill Giant"` | `precombat_main` | Targets are comma-separated: `"name"`, `B "name"` (controller-qualified), or `player B`. Exactly as many as the spell needs (any number within a variable group's bounds); `targeting auto` takes the engine's first legal option for each (what converted XMage tests use when the test let XMage pick). Add `in response to "Spell"` to fire only while an object of that name is on the stack. Sorcery-speed spells wait for an empty stack and retry at the player's next priority in the same step. |
| `cast "Abrade" mode 2 targeting "Rod of Ruin"` | `precombat_main` | A modal spell (CR 700.2): `mode N` (1-based) chooses the mode before targets; the targets are the mode's. Unscripted, the first mode with legal targets is chosen. |
| `cast "Twin Bolt" targeting "Grizzly Bears", player B dividing 1, 1` | `precombat_main` | A spell that divides damage or counters among its targets (CR 601.2d): `dividing` gives each target's share, in target order, after `targeting`. Unscripted, the engine's first division. A variable number of targets ("X target creatures", "any number of target ...", CR 601.2c) takes as many as are scripted; the rest are declined. |
| `cast "Blaze" x 3 targeting player B` | `precombat_main` | `x N` chooses X for a spell with {X} in its cost (CR 601.2b); the scenario fails if the player cannot pay for it. Unscripted, X is 0. `replicated N` (in the same place) pays a replicate cost N times (CR 702.56a); unscripted, none; `illegal ... replicated N targeting ...` checks that N times can't be paid. |
| `cast "Bone Splinters" sacrificing "Goblin" targeting "Hill Giant"`, `cast "Tormenting Voice" discarding "Forest"` | `precombat_main` | Additional costs (CR 601.2h): what to sacrifice or discard. The order is `from graveyard`, `kicked`, `mode`, `x`, `sacrificing`, `discarding`, `targeting`. Unscripted, the engine's first option. `other_cost` (after `alternative`) pays the second of two additional costs ("sacrifice a creature or pay {3}", CR 601.2b): `cast "Lash of the Balrog" other_cost targeting "Hill Giant"`; otherwise the first that can be paid. |
| `cast "Mardu Scout" dashed` | `precombat_main` | Cast for the dash cost (CR 702.109). (`evoked` likewise casts for the evoke cost, CR 702.74; `warped` for the warp cost, CR 702.185; `alternative` for the alternative cost the card's own text offers, CR 118.9, with `sacrificing "Mountain"` naming the permanent it sacrifices or returns.) A bargained spell (CR 702.166) names what it sacrifices with `sacrificing "Ornithopter"`; unscripted, it isn't bargained. Casualty (CR 702.153) likewise: `sacrificing "Grizzly Bears"` pays it; unscripted, it isn't paid. |
| `cast "Nimbus Naiad" bestowed targeting "Grizzly Bears"` | `precombat_main` | Cast for the bestow cost as an Aura spell (CR 702.103); the target is the creature it enchants. |
| `cast "Electrickery" overloaded` | `precombat_main` | Cast for the overload cost (CR 702.96), without targets. |
| `cast "Clutch of Currents" awakened targeting "Hill Giant", "Forest"` | `precombat_main` | Cast for the awaken cost (CR 702.113); the land it awakens is the last target. |
| `cast "Burn Trail" conspired targeting player B` | `precombat_main` | Conspire the spell (CR 702.78), the engine tapping two creatures that share a color with it; unscripted, it isn't conspired. As the copy is made, `choose "Hill Giant"` gives it a new target (CR 707.10c); unscripted, it keeps the spell's. `teamwork` likewise casts a spell using teamwork, the engine tapping the creatures. |
| `cast "Kodama's Might" splicing "Glacial Ray" targeting "Grizzly Bears", player B` | `precombat_main` | Splice cards from hand onto an Arcane spell (CR 702.47), in order (comma-separated), after `discarding` and before `targeting`; the targets are the spell's own, then each spliced card's. Unscripted, nothing is spliced. `illegal cast ... splicing "X"` casts the spell and checks that X is not offered for splicing. |
| `cast "Goblin Bushwhacker" kicked` | `precombat_main` | Pay the kicker (CR 702.33), and a buyback (CR 702.27) or offspring (CR 702.175) cost. The engine offers the choice only when the kicker can be paid on top of the cost; unscripted (or unaffordable), the spell is not kicked. "Buyback—Sacrifice a land." takes `kicked sacrificing "Forest"`. |
| `cast "Atarka Efreet" face_down`, `unmorph "Atarka Efreet"` | `precombat_main`; any step | Cast a morph or disguise card face down for {3} (CR 702.37c); turn the face-down permanent that is that card face up (CR 116.2b). A face-down permanent is named "face-down creature" in assertions. `illegal ... unmorph` checks it can't be turned face up there (its morph cost can't be paid). A morph cost that discards or returns takes the engine's first option. |
| `unlock "Widow's Walk"` | `precombat_main` | Special action: unlock that locked door of a Room the player controls by paying its half's mana cost (CR 709.5e). |
| `suspend "X"`, `foretell "X"` | Special actions: suspend or foretell card X from the hand. A foretold card is then cast with `cast "X"` on a later turn. |
| `turn 3 upkeep A: cast "X" targeting ...` | A card cast by a trigger as it resolves (suspend, rebound, madness, cascade) is always cast when the trigger offers it; a scripted `cast "X"` there supplies its targets. |
| `cast "Firebolt" from graveyard targeting player B` | `precombat_main` | Cast with flashback (CR 702.34) from the player's graveyard, for the flashback cost; the card is exiled as it leaves the stack. |
| `attack "X", "Y"` | `declare_attackers` | Several `attack` lines for the same turn combine into one declaration. `attack "X" at "Jace Beleren"` (or `at player B`) says what the creature attacks when the defender controls a planeswalker (CR 508.1a); unscripted, the player. `illegal attack "X" at "Jace Beleren"` attacks with X and checks it can't attack that planeswalker (a goaded creature, CR 701.15b). |
| `choose "Grizzly Bears"` (step `declare_attackers`) | `declare_attackers` | With banding (CR 702.22c), after attackers are declared each attacker with banding in turn leads a band: each `choose` adds the named attacker to the band being formed; unscripted (or `choose done`), the band is closed. A creature blocking one creature of a band blocks the band. |
| `block "Attacker" with "Blocker"` | `declare_blockers` | Several lines combine. The same attacker may be named twice to double-block it, and the same blocker twice for a creature that can block several attackers (CR 509.1a). |
| `assign "Attacker" to "Blocker" 2, player B 4` | any combat damage step | Divide combat damage among blockers (and the player, with trample). A blocker blocking several attackers divides its damage among them the same way: `assign "Palace Guard" to "Grizzly Bears" 1` (a recipient left out gets 0). Unscripted divisions take the engine's first option. |
| `discard "Card"` | `cleanup` | Choice when discarding to hand size. Also the card chosen from another player's revealed hand to discard or exile ("You choose a nonland card from it"): `turn 1 precombat_main A: discard "Shock"`. |
| `trigger "Blood Artist" targeting player B` | any step | A triggered ability of that source. When the player controls several triggers waiting to go on the stack, the scripted ones go first, in file order (the first put on the stack resolves last). `targeting` gives the ability's targets, chosen as it goes on the stack (CR 603.3d). Unscripted triggers take the engine's first option. |
| `apply "Furnace of Rath"` | any step | When several replacement or prevention effects apply to one event, the affected player chooses which applies first (CR 616.1): apply the effect from that card. Unscripted choices take the engine's first option. |
| `activate "Prodigal Sorcerer" targeting player B` | `precombat_main` | Activate an ability of a permanent the player controls (CR 602). `x N` (after `ability N`) chooses X for an ability with {X} in its cost (unscripted, 0). `ability N` picks the Nth of the permanent's activated abilities (a planeswalker's loyalty abilities, CR 606); `sacrificing "Goblin"` names the permanent paid for a "sacrifice a ..." cost and `discarding "Forest"` the card for a "discard a card" cost; `targeting` gives the ability's targets. A permanent with several activated abilities offers the first whose costs can be paid. Sorcery-speed abilities (equip) wait for an empty stack like sorceries. Mana abilities are not activated this way; the engine taps for mana as part of paying costs. A card's ninjutsu (CR 702.49) is activated from the hand the same way, `sacrificing` naming the unblocked attacker returned. |
| `keep oldest "Isamaru, Hound of Konda"` / `keep newest ...` | any step | Under the legend rule (CR 704.5j), which of the player's legendary permanents of that name to keep. Unscripted, the oldest is kept. |
| `accept "Gravedigger"` / `decline "Gravedigger"` | any step | The answer to that card's "you may" as its spell or ability resolves. Unscripted, yes. Also where an owner puts that card "on their choice of the top or bottom" of their library (or "second from the top or on the bottom", or the card revealed by a clash, CR 701.23): `accept` is the top; and whether to pay for "counter it unless that player pays" of that card (ward): `decline "Owlin Shieldmage"` doesn't pay. |
| `wait` | `precombat_main` | Pass priority until the stack is empty before any later action of this player at the same point fires (so a second spell is cast after the first resolves rather than in response to it). |
| `copy "Serra Angel"` / `copy B "Serra Angel"` / `copy nothing` | any step | What a Clone enters as a copy of (CR 707), or nothing. Unscripted, the engine's first option (a copy). |
| `choose red`, `choose Island`, `choose Merfolk` (or quoted) | any step | A color or type chosen as a permanent enters ("As this land enters, choose a color") or as an effect resolves ("the color of your choice"); also the color of mana of any color. "The color or colors of your choice" asks a color at a time: `choose blue`, `choose red`, `choose done`. `illegal ... choose Wall` checks that it is not offered. Also what an effect acts on as it resolves ("tap another untapped creature you control", "return up to two lands": `choose "Forest"`, `choose done`), and which alternative of "A or B" (`choose "2"`, 1-based). Unscripted, the engine's first option (white; the first type). A card to dredge instead of drawing (CR 702.52): `choose "Greater Mossdog"`; for the draw step's draw, script it in the upkeep (the step whose passing leads to the draw); unscripted, the card is drawn. |

Step names: `untap`, `upkeep`, `draw`, `precombat_main` (`main1`),
`begin_combat`, `declare_attackers`, `declare_blockers`,
`first_strike_damage`, `combat_damage`, `end_combat`, `postcombat_main`
(`main2`), `end`, `cleanup`.

Unscripted decisions: keep the hand, pass priority, declare no attackers or
blockers, take the first option otherwise (including trigger order, trigger
targets, the order of replacement effects, the legend rule, where the
first option keeps the oldest permanent, modes, "you may" clauses, what
to copy, colors and types chosen, and what to sacrifice or discard for a
cost; X is 0).

Names resolve against the current state when the action fires. A copy is
named after what it copies (a Clone copying Serra Angel is `"Serra Angel"`
until it leaves the battlefield). Where a player controls several objects
of the same name, `attack` and `block` lines pick distinct ones in order;
targets and assertions must be unambiguous: qualify with `A`/`B` if both
players control one, and with `oldest`/`newest` if one player controls
several (`newest A "Serra Angel"`). A name that is not on the battlefield
is looked up in graveyards, for "target creature card in your graveyard".
A target that names an object that is not a legal choice falls back to
an ability on the stack from a source of that name, for "target
activated ability" (`cast "Rust" targeting "Aeolipile"`).

## Running

| Line | Meaning |
|---|---|
| `run_to turn 2 postcombat_main` | Run until the first decision at or after that point (default step `untap`), or the game ends. |
| `run_to end` | Run until the game is over. |

Statements execute in file order; actions are registered up front, so an
action placed after a `run_to` that has already passed its point is an
error.

## Assertions

```
assert life B 17
assert battlefield A contains "Grizzly Bears"      # at least one
assert battlefield A contains 2 "Grizzly Bears"    # exactly two
assert graveyard B lacks "Hill Giant"
assert hand A count 7
assert power "Grizzly Bears" 5          assert toughness B "Hill Giant" 3
assert damage "Hill Giant" 2
assert tapped "Mountain"                assert untapped "Serra Angel"
assert winner A      assert draw      assert game_over      assert not game_over
assert turn 3        assert step precombat_main        assert stack count 0
assert stack top "Blood Artist"         # a spell by name, or an ability by its source's name
assert has "Grizzly Bears" flying       assert has not "Serra Angel" vigilance
assert is "Humility" creature           assert is not "Urborg, Tomb of Yawgmoth" swamp
assert is "Dryad Arbor" green           # types, supertypes, land/creature subtypes, colors
assert is "Mishra's Factory" "Assembly-Worker"   # quote a subtype with punctuation
assert counters "Grizzly Bears" +1/+1 3     assert counters "Jace Beleren" loyalty 5
assert attached "Holy Strength" to "Grizzly Bears"
assert no_abilities "Serra Angel"
assert has newest A "Grizzly Bears" haste   # the most recent of several
assert known A "Island"     assert unknown B "Island"   # a card in a library or hand revealed to / looked at by that player
```

Zones: `battlefield` (by controller), `hand`, `graveyard`, `library`,
`exile` (by owner). Tokens are named after their definition ("Soldier",
"Goblin") and assert like any permanent; qualify with `A`/`B` when both
players have some.

## Example

```
scenario "deathtouch kills regardless of toughness" {
  battlefield A: "Serra Angel"
  battlefield B: "Typhoid Rats"
  start turn 2 B
  turn 2 B: attack "Typhoid Rats"
  turn 2 A: block "Typhoid Rats" with "Serra Angel"
  run_to turn 2 postcombat_main
  assert life A 20
  assert graveyard A contains "Serra Angel"
  assert graveyard B contains "Typhoid Rats"
}
```

Run the corpus with `cargo test -p mtg-scenario` or
`cargo run -p mtg-cli -- scenario [paths] [--verbose]`.

## Designations

Setup: `speed A: 3` gives a starting speed (CR 702.179). Assertions:
`assert monarch A` (or `none`), `assert initiative B`, `assert daynight
day` (`night`, `neither`), `assert speed A 2`, `assert tempted A 1` (times
the Ring has tempted A), `assert ring_bearer A "Grizzly Bears"` (or
`none`), `assert blessing A yes` (or `no`), `assert dungeon A "Lost Mine
of Phandelver" "Goblin Lair"` (or `none`), `assert completed A 1`. A
dungeon, an arrow out of a room, and the choice between "lose life" and
"unless they ..." are `choose "2"` (1-based) choices; the Ring-bearer is
`choose "Grizzly Bears"`.
