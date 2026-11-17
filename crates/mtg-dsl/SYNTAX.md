# Card file syntax

Card files (`*.card`, one per Oracle name, under `cards/<first letter>/`
in the repository) define the cards the engine knows. A file is parsed into a
syntax tree, compiled into the engine's definitions, and *rendered back to
Oracle text* for the round-trip gate (`mtg-cards gate`): the rendered text
must equal the card's Oracle text in the Scryfall snapshot, modulo reminder
text. The English the printer produces follows from the statements below, so
the statements are the single source of truth; nothing in a card file is
free text.

Comments start with `#`. One statement per line. Mana and tap symbols are
written in braces as Scryfall prints them (`{1}{G}`, `{T}`, `{X}`).

```
card "Name" {
  <header lines>
  <ability lines>
}
```

`token "Name" { ... }` and `emblem "Name" { ... }` define the tokens and
emblems that effects create (CR 111, 114). They have no `oracle_id` and are
not gated.

## Header

| Line | Meaning |
|---|---|
| `oracle_id 2b93787d-...` | The Scryfall `oracle_id` the card was authored against. Required for cards. |
| `cost {1}{G}` | Mana cost. Omit for lands and tokens. |
| `type Legendary Creature — Goblin Shaman` | The type line: supertypes, types, then subtypes after `—` (or `-`). |
| `pt 2/2` | Power and toughness. Omit when a characteristic-defining ability sets them (Scryfall prints `*`). |
| `loyalty 3` | A planeswalker's printed loyalty. |
| `color green` | Color indicator (CR 204): for cards and tokens whose color is not their cost's. |
| `keywords flying, deathtouch, lifelink` | Keyword abilities, in printed order: `deathtouch`, `defender`, `double strike`, `first strike`, `flash`, `flying`, `haste`, `hexproof`, `indestructible`, `lifelink`, `menace`, `prowess`, `reach`, `trample`, `vigilance`, and the landwalks `plainswalk`, `islandwalk`, `swampwalk`, `mountainwalk`, `forestwalk` (CR 702.14). The restrictions `can't attack`, `can't block`, `attacks each combat if able`, and `can't be blocked` are written as keywords too and print as sentences ("This creature can't block."). A `keyword` line after an ability line prints in that place instead of first (a bridge land's "Indestructible" after "enters tapped"). |
| `additional cost sacrifice creature` | "As an additional cost to cast this spell, ..." (CR 601.2b). Same syntax as an activated ability's cost. |
| `kicker {1}{U}` | Kicker (CR 702.33): an optional additional mana cost, chosen as the spell is cast. Prints "Kicker {1}{U}" after the keywords. Effects that depend on it: `if kicked, <clause>` in a spell, `if kicked` as a trigger condition. |
| `flashback {4}{R}` / `flashback {1}{U}, pay 3 life` | Flashback (CR 702.34): the card may be cast from the graveyard for this cost (mana and life only) and is exiled as it leaves the stack. Prints last, as "Flashback {4}{R}" or "Flashback—{1}{U}, Pay 3 life." |
| `oracle_mismatch "reason"` | The rendered text is allowed to differ from Oracle; the gate reports it as allowed. Use it for cards whose text includes an ability the engine does not implement. |

## Abilities

| Line | Meaning |
|---|---|
| `mana {G}` / `mana {W} or {U}` / `mana any` | "{T}: Add {G}." A mana ability (CR 605); `any` is "one mana of any color". Basic land types grant theirs intrinsically. |
| `cycling {2}` | Cycling (CR 702.29): "{2}, Discard this card: Draw a card.", activated from the hand. |
| `enchant creature` | An Aura's "Enchant" (CR 702.5), with a target specification. |
| `spell: <effects>` | What an instant or sorcery does on resolution. |
| `choose one { mode: <effects> ... }` | A modal spell (CR 700.2), one `mode` line per mode. |
| `activated <cost>: <effects>` | An activated ability (CR 602). |
| `loyalty +1: <effects>` | A loyalty ability (CR 606); `-6` likewise. |
| `equip {1}` | Equip (CR 702.6). |
| `triggered <trigger> [if <condition>]: <effects>` | A triggered ability (CR 603). |
| `replace <event>: <outcome>` | A replacement or prevention effect while on the battlefield (CR 614, 615). Add `{ <effects> }` for effects that happen when it applies (Phantom Tiger). |
| `enters tapped` / `enters with 2 +1/+1 counters` | How this permanent enters (CR 614.1c). |
| `static <filter>: <modifications>` | A static ability generating a continuous effect (CR 604, 611.3). |
| `enters as copy of creature` | "You may have ~ enter as a copy of ..." (CR 707). |
| `except { ... }` | What this card's copy effects add (CR 707.9b): `subtypes Illusion`, `keywords haste`, and ability lines. |

Effects written after `:` sit on one line; write `{` instead of `:` and put
one line per paragraph for text that spans paragraphs:

```
  spell {
    prevent next 1 damage to any target this turn
    draw 1 card
  }
```

### Costs

Comma-separated, printed in the order written: mana symbols (`{1}{R}`),
`{T}`, `sacrifice this`, `sacrifice Goblin` (a filter: the player chooses
a matching permanent they control), `discard 1 card`, `discard creature
card` (a filter: discard one matching card, printed "Discard a creature
card"), `discard this`, `pay 3 life`, `sorcery` (activate only as a
sorcery), `once` (activate only once each turn). `none` for a free ability.

### Triggers

| Trigger | Oracle |
|---|---|
| `when this enters`, `whenever other creature enters` | enters the battlefield |
| `when this dies`, `whenever creature dies`, `whenever creature you control dies` | dies; for a noncreature, "is put into a graveyard from the battlefield". A filter without `other` includes the source ("this creature or another creature"). |
| `at beginning of your upkeep`, `at beginning of each draw step`, `at beginning of the end step` | beginning of a step; steps: `untap`, `upkeep`, `draw`, `combat`, `end` |
| `whenever this attacks`, `whenever creature you control attacks`, `whenever this blocks` | attacks or blocks |
| `whenever this deals combat damage to a player` | |
| `whenever you gain life` | |
| `whenever you cast a spell`, `... a noncreature spell`, `... an instant or sorcery spell` | |
| `when this becomes targeted` | becomes the target of a spell or ability |

Conditions (intervening "if", CR 603.4): `if this is untapped`,
`if no creatures are on the battlefield`, `if kicked` ("if it was kicked",
CR 702.33c, for a permanent with `kicker`).

### Replacement events and outcomes

Events: `damage to <affects>`, `combat damage to <affects>`,
`damage from creature you control to <affects>`, `you gain life`,
`a player gains life`, `you draw`, `you draw with empty library`,
`card put into a graveyard`, `card put into an opponent's graveyard`,
`... from the battlefield`, `+1/+1 counters put on creature you control`,
`destroy <affects>`, `enters`.

Affects: `any` (a permanent or player), `this`, `you`, `each opponent`,
`each player`, `you and creature you control`, a filter, or a target.

Outcomes: `prevent`, `prevent next 4`, `double`, `plus one`, `exile`,
`draw two`, `win`, `tapped`, `with 2 +1/+1 counters`, `regenerate`,
`redirect to attached`.

### Filters

A filter names a kind of permanent, in the singular: `creature`,
`creature you control`, `creature an opponent controls`, `creature you
don't control`, `white creature`, `nonblack creature`, `nonbasic land`,
`other non-Aura enchantment`, `creature with flying`, `creature without
flying`, `creature with no abilities`, `all Sliver creature` (a static
ability's subject then prints "All Sliver creatures", as old Slivers do), `Goblin`, `Swamp you control`, `nonplaneswalker permanent you
control`, `enchanted` (the permanent this is attached to), `this` (the
source itself, for characteristic-defining abilities).

### Modifications (static abilities)

Comma-separated, each in one layer of CR 613: `+1/+1`, `base 1/1`,
`base mv` (base power and toughness each equal to its mana value),
`has flying, trample`, `loses all abilities`, `add type creature`,
`set land Mountain`, `add land Swamp`, `add subtype Saproling`, `control`
(you control it), `add color white`, `set color blue`, `switch pt`,
`cda color all`, `cda pt card types in graveyards +1`,
`cda pt creature cards in graveyards +1`, `cda pt count Swamp you control`,
`cda pt your life`.

### Effects

Effects are clauses. `;` separates sentences; `and` joins clauses into one
sentence ("target player loses 1 life and you gain 1 life") and `then`
joins them with ", then" ("draw 1 card then discard 1 card"); a clause
after `and` may leave its subject implied ("you draw 1 card and lose 1
life").
Numbers are digits or `X`; "until end of turn" is implied by `gets`,
`gains`, `becomes`, `has base power and toughness`, and `gain control of`
and may be written or not. A sentence that starts `until end of turn,`
prints the duration first instead ("Until end of turn, target creature you
control has base power and toughness 4/4 and gains flying").

Subject-first clauses: `<who> gains 3 life`, `<who> loses 1 life`,
`<who> draws 2 cards`, `<who> discards 2 cards`, `<who> mills 20 cards`,
`<what> gets +3/+3`, `<what> gains flying`, `<what> can't block` (this
turn), `<what> fights <what>`, `<what> has base power and toughness 4/4`,
`<what> becomes 2/2 Assembly-Worker artifact creature`, `this becomes a
copy of <what> [with exceptions]`.

Verb-first clauses: `deal 3 damage to <what> [and <what>]`,
`destroy <what> [without regeneration]`, `put 2 +1/+1 counters on <what>`,
`remove 1 +1/+1 counter from <what>`, `remove all counters from <what>`,
`return <what> to hand`, `return <what> to battlefield [with 1 +1/+1 counter]`,
`regenerate <what>`, `exile <what>`, `exile all graveyards`,
`sacrifice <what>`, `tap <what>`, `untap <what> [and <what>]`,
`gain control of <what>`, `attach to <what>`, `create 2 "Soldier"`,
`create token copy of <what> [with exceptions]`, `emblem "Name"`,
`draw 2 cards` (you), `discard 1 card` (you), `mill 2 cards` (you),
`scry 2`, `surveil 1`,
`counter target spell`, `counter target spell unless its controller pays {3}`,
`you may <clause>`, `if kicked, <clause>` ("If this spell was kicked, ..."
for a spell with `kicker`), `prevent all [combat] damage
[to <affects>] this turn`, `prevent next 4 damage to <affects> this turn`,
`this turn, replace <event>: <outcome>`, `players can't gain life this turn`,
`damage can't be prevented this turn`, and
`delayed <what>: <effects> at beginning of the next end step` (a delayed
triggered ability, CR 603.7; write the trigger first, then a comma, then the
effects, to print it that way). Inside a delayed ability `it` is the object
the ability is about.

Who and what (`<who>`, `<what>`): `you`, `each opponent`, `each player`,
`this` (the source), `that player` / `that creature` (what triggered the
ability), `defending player`, `each creature`, `each creature you control`,
`those creatures` (the same, mentioned again), `each permanent you control`,
`that token` (what the previous effect created), `each <filter>` or
`all <filter>` (every matching permanent: `each artifact`, `each blue
creature`, `each creature without flying`, `each white creature you
control`; written singular, printed "all artifacts" after tap, destroy,
and exile, "each creature without flying" after deal damage, and "White
creatures you control get +2/+2" as a subject), and targets.

### Targets

`target creature` declares a target; the first mention names it, and later
mentions refer to it by number: `target 1`, printed as "it", or
`that target 1`, printed as "that creature". `2 target lands` declares two
targets of one kind. `another target creature` excludes the source.
Specifications: `any target`, `creature`, `player`, `player or planeswalker`,
`creature or planeswalker`, `land`, `tapped creature`, `creature you control`,
`nonlegendary creature you control`, `artifact`, `enchantment`,
`artifact or enchantment`, `artifact, creature, or land`, `permanent`,
`another creature`, `another permanent`, `creature card from your graveyard`,
`spell`, `creature spell`, `noncreature spell`, `opponent`,
`artifact or creature`, `attacking or blocking creature`, `attacking
creature`, `blocking creature`, `<Subtype> creature` ("target Assembly-Worker creature"), and any filter
("target nonblack creature", "target creature an opponent controls").

## Example

```
card "Siege-Gang Commander" {
  oracle_id 7b8b4b5a-...
  cost {3}{R}{R}
  type Creature — Goblin
  pt 2/2
  triggered when this enters: create 3 "Goblin"
  activated {1}{R}, sacrifice Goblin: deal 2 damage to any target
}
```

renders as

```
When this creature enters, create three 1/1 red Goblin creature tokens.
{1}{R}, Sacrifice a Goblin: This creature deals 2 damage to any target.
```

Check a file with `cargo run -p mtg-cards -- gate --diff "Siege-Gang Commander"`
(needs the Scryfall snapshot: `cargo run -p mtg-cards -- ingest --locked`).

## Authoring from templates

`cargo run -p mtg-cards -- author --format pauper [--write]` builds card
files for every card in a format whose Oracle text consists of keywords
the engine implements and sentences a fixed set of templates recognizes
(vanilla and keyword creatures, Auras, mana lands, burn, pump, cantrips,
removal, simple triggers and activated abilities). Each candidate is
parsed, rendered, and compared to its Oracle text before it is written,
so it passes the gate by construction; the report lists what was skipped
and why (unsupported keyword, unrecognized sentence), which is the backlog
for new templates and engine primitives. `--write` never overwrites a
file that exists, so hand-written cards win. `mtg-cards coverage --format
pauper` counts how much of a format the corpus holds.
