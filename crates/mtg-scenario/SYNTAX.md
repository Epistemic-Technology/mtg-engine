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
| `battlefield A: "Grizzly Bears", 2 "Forest", "Hill Giant" tapped, "Flight" on "Grizzly Bears"` | Permanents A controls. `N "name"` repeats; `tapped` taps; `on "name"` attaches an Aura to that permanent (either player's). Permanents count as controlled since the turn began, so they may attack. Timestamps follow listing order, player A first. |
| `hand A: "Lightning Bolt"` | Cards in hand. |
| `graveyard A: ...`, `exile A: ...` | Likewise. |
| `library A: "Top Card", "Next Card"` | The library, top card first, in exactly this order. Without this line the player gets 40 Plains so draws succeed. `library A:` with nothing gives an empty library. |
| `start turn 2 B precombat_main` | Where play begins: turn number, active player, and step (default `untap`). Default `start turn 1 A`. There are no mulligans; the starting player skips the first draw as usual. |
| `seed 7` | RNG seed (only matters if something random happens). |
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
scenario fails.

| Action | Default step | Notes |
|---|---|---|
| `play "Forest"` | `precombat_main` | Play a land from hand. |
| `cast "Shock" targeting "Hill Giant"` | `precombat_main` | Targets are comma-separated: `"name"`, `B "name"` (controller-qualified), or `player B`. Exactly as many as the spell needs. Add `in response to "Spell"` to fire only while an object of that name is on the stack. Sorcery-speed spells wait for an empty stack and retry at the player's next priority in the same step. |
| `attack "X", "Y"` | `declare_attackers` | Several `attack` lines for the same turn combine into one declaration. |
| `block "Attacker" with "Blocker"` | `declare_blockers` | Several lines combine. The same attacker may be named twice to double-block it. |
| `assign "Attacker" to "Blocker" 2, player B 4` | any combat damage step | Divide combat damage among blockers (and the player, with trample). Unscripted divisions take the engine's first option. |
| `discard "Card"` | `cleanup` | Choice when discarding to hand size. |
| `trigger "Blood Artist" targeting player B` | any step | A triggered ability of that source. When the player controls several triggers waiting to go on the stack, the scripted ones go first, in file order (the first put on the stack resolves last). `targeting` gives the ability's targets, chosen as it goes on the stack (CR 603.3d). Unscripted triggers take the engine's first option. |

Step names: `untap`, `upkeep`, `draw`, `precombat_main` (`main1`),
`begin_combat`, `declare_attackers`, `declare_blockers`,
`first_strike_damage`, `combat_damage`, `end_combat`, `postcombat_main`
(`main2`), `end`, `cleanup`.

Unscripted decisions: keep the hand, pass priority, declare no attackers or
blockers, take the first option otherwise (including trigger order and
trigger targets).

Names resolve against the current state when the action fires. Where a
player controls several objects of the same name, `attack` and `block`
lines pick distinct ones in order; targets and assertions must be
unambiguous (qualify with `A`/`B` if both players control one).

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
assert counters "Grizzly Bears" +1/+1 3
assert attached "Holy Strength" to "Grizzly Bears"
assert no_abilities "Serra Angel"
```

Zones: `battlefield` (by controller), `hand`, `graveyard`, `library`,
`exile` (by owner).

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
