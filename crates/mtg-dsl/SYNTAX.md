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
not gated. A token's block name is the key `create` uses; tokens the
author generates are keyed by their description (`token "1/1 white Soldier
creature with flying"`) and give their in-game name with a `name "Soldier"`
line. A noncreature token whose one subtype is its name (Treasure, Food,
Clue: CR 111.10) prints by name: "create a Treasure token".

## Multi-face cards

A split card, adventurer, omen, or double-faced card is one block named as
Scryfall names it (`card "Curious Pair // Treats to Share"`) with a
`layout` line (`adventure`, `omen`, `transform`, `split`) and a `face
"Name" { ... }` block per other face, holding that face's header and
ability lines. The block's own lines are the front face (a split card's
block has none: both halves are faces). The gate compares each face with
the snapshot's. In the engine each face is its own definition: an
adventure or omen is cast from the hand as its spell face (CR 715.3) and
goes on an adventure in exile as it resolves (an omen is shuffled into
the library instead), whence the creature may be cast; a split card's
halves are cast separately (CR 709.3); `transform this` turns a
double-faced permanent to its other face (CR 701.27).

## Header

| Line | Meaning |
|---|---|
| `oracle_id 2b93787d-...` | The Scryfall `oracle_id` the card was authored against. Required for cards. |
| `cost {1}{G}` | Mana cost. Omit for lands and tokens. Hybrid `{R/G}`, monocolored hybrid `{2/W}`, and Phyrexian `{R/P}` symbols are accepted (CR 107.4e–f); the engine pays them in a canonical order and pays 2 life for a Phyrexian symbol only when mana can't. |
| `type Legendary Creature — Goblin Shaman` | The type line: supertypes, types, then subtypes after `—` (or `-`). |
| `pt 2/2` | Power and toughness. Omit when a characteristic-defining ability sets them (Scryfall prints `*`). |
| `loyalty 3` | A planeswalker's printed loyalty. |
| `color green` | Color indicator (CR 204): for cards and tokens whose color is not their cost's. |
| `keywords flying, deathtouch, lifelink` | Keyword abilities, in printed order: `deathtouch`, `defender`, `double strike`, `first strike`, `flash`, `flying`, `haste`, `hexproof`, `indestructible`, `lifelink`, `menace`, `prowess`, `reach`, `trample`, `vigilance`, and the landwalks `plainswalk`, `islandwalk`, `swampwalk`, `mountainwalk`, `forestwalk` (CR 702.14), `protection from white` (and the other colors, CR 702.16), `devoid` (CR 702.114), `changeling` (CR 702.73), `shroud`, `fear`, `intimidate`, `shadow`, `horsemanship`, `skulk`, `wither`, `infect` (damage to players as poison counters; ten lose, CR 704.5c), `exalted`, `persist`, `undying`, `flanking`, `convoke` (CR 702.51: untapped creatures may be tapped to pay {1} or one mana of their color), and `improvise` (CR 702.126: artifacts pay {1}). The restrictions `can't attack`, `can't block`, `attacks each combat if able`, `can't be blocked`, `can't attack alone`, and `can't block alone` (both together print "can't attack or block alone", CR 506.5), and `doesn't untap` ("This creature doesn't untap during your untap step."; granted, "Enchanted creature doesn't untap during its controller's untap step.") are written as keywords too and print as sentences ("This creature can't block."). A `keyword` line after an ability line prints in that place instead of first (a bridge land's "Indestructible" after "enters tapped"). |
| `additional cost sacrifice creature` | "As an additional cost to cast this spell, ..." (CR 601.2b). Same syntax as an activated ability's cost. |
| `kicker {1}{U}` | Kicker (CR 702.33): an optional additional mana cost, chosen as the spell is cast. Prints "Kicker {1}{U}" after the keywords. Effects that depend on it: `if kicked, <clause>` in a spell, `if kicked` as a trigger condition. |
| `flashback {4}{R}` / `flashback {1}{U}, pay 3 life` | Flashback (CR 702.34): the card may be cast from the graveyard for this cost (mana and life only) and is exiled as it leaves the stack. Prints last, as "Flashback {4}{R}" or "Flashback—{1}{U}, Pay 3 life." |
| `oracle_mismatch "reason"` | The rendered text is allowed to differ from Oracle; the gate reports it as allowed. Use it for cards whose text includes an ability the engine does not implement. |

## Abilities

| Line | Meaning |
|---|---|
| `mana {G}` / `mana {W} or {U}` / `mana any` / `mana {U} or chosen` | "{T}: Add {G}." A mana ability (CR 605); `any` is "one mana of any color", `chosen` "one mana of the chosen color" (the color chosen as the permanent entered; none chosen, no mana). Basic land types grant theirs intrinsically. |
| `mana any, sacrifice this` | "{T}, Sacrifice this artifact: Add one mana of any color." (Treasure, Lotus Petal). Payment uses such sources after all others. |
| `cycling {2}` | Cycling (CR 702.29): "{2}, Discard this card: Draw a card.", activated from the hand. |
| `plainscycling {2}`, `basic landcycling {1}{G}`, `landcycling {2}` | Typecycling (CR 702.29e): discard to search for a card of that type. |
| `enchant creature` | An Aura's "Enchant" (CR 702.5), with a target specification. |
| `spell: <effects>` | What an instant or sorcery does on resolution. |
| `choose one { mode: <effects> ... }` | A modal spell (CR 700.2), one `mode` line per mode. `choose one or both { ... }` takes two modes, either or both ("Choose one or both —"). As a triggered ability's effect (`triggered when this enters: choose one { ... }`) the mode is chosen as the ability goes on the stack (CR 603.3c), each mode with its own targets. |
| `enters tapped unless you control planeswalker`, `enters tapped unless player life 13 or less` | "This land enters tapped unless ..." with a static condition (CR 614.1d). |
| `can't attack unless defending player controls Island`, `can't block unless you control other Goblin`, `can't attack or block unless ...` | Attack and block restrictions with a condition (CR 508.1c, 509.1c). |
| `activated <cost>: <effects>` | An activated ability (CR 602). |
| `loyalty +1: <effects>` | A loyalty ability (CR 606); `-6` likewise. |
| `equip {1}` | Equip (CR 702.6). |
| `triggered <trigger> [if <condition>]: <effects>` | A triggered ability (CR 603). |
| `replace <event>: <outcome>` | A replacement or prevention effect while on the battlefield (CR 614, 615). Add `{ <effects> }` for effects that happen when it applies (Phantom Tiger). |
| `enters tapped` / `enters with 2 +1/+1 counters` | How this permanent enters (CR 614.1c). |
| `static <filter>: <modifications>` | A static ability generating a continuous effect (CR 604, 611.3). |
| `static as long as <condition>, <filter>: <modifications>` | A conditional static ability (CR 611.3a), printed "As long as <condition>, ...". Write the condition last (`static this: +2/+2 as long as metalcraft`) to print it last. `static during your turn, this: has first strike` prints "During your turn, ...". |
| `label "Landfall" <ability line>` | An ability word (CR 207.2c) printed before the ability: "Landfall — Whenever a land you control enters, ...". It has no rules meaning. |
| `can't be blocked by Wall`, `can't be blocked except by creature with flying`, `can't be blocked by more than one creature`, `can block only creature with flying`, `can't block creature with flying` | Blocking restrictions on this creature (CR 509.1b–c). |
| `enters as copy of creature` | "You may have ~ enter as a copy of ..." (CR 707). |
| `as this enters choose color`, `as this enters choose color except white`, `as this enters choose creature type`, `as it enters choose color` | "As this land enters, choose a color [other than white]." (CR 614.12): the controller chooses as the permanent enters, however it enters; the permanent remembers it until it leaves (CR 400.7) for `mana chosen`, `set color chosen` and `has protection from chosen color`. `as it enters` prints on the line before ("This land enters tapped. As it enters, choose a color."). |
| `spells artifact you cast cost {1} less`, `spells noncreature cost {1} more`, `spells Dragon your opponents cast cost {2} more` | "Artifact spells you cast cost {1} less to cast." (CR 601.2f): the generic part only, never below zero. Types joined with `or` print "Instant and sorcery spells". |
| `this spell costs {1} less count each artifact you control` | "This spell costs {1} less to cast for each artifact you control." |
| `affinity for artifact` | "Affinity for artifacts" (CR 702.41). |
| `protection from artifact`, `protection from Goblin`, `protection from multicolored`, `protection from everything` | Protection from a quality other than a color (CR 702.16); colors are keywords. |
| `modular 2`, `cumulative upkeep {1}`, `exert: this gets +2/+2` | Modular (CR 702.43): enters with N +1/+1 counters; as it dies, they may move to target artifact creature. Cumulative upkeep (CR 702.24): each upkeep an age counter, then pay the cost per counter or sacrifice it. Exert (CR 701.43): as it attacks, its controller may exert it (it doesn't untap during their next untap step) for the effects. The keyword `delve` (CR 702.66) lets graveyard cards pay {1} each. |
| `entwine {2}`, `escape {3}{B} exile 3`, `escapes with 2 +1/+1 counters` | Entwine (CR 702.42): a `choose one` spell may choose every mode for this cost on top. Escape (CR 702.138): cast from the graveyard for this cost, exiling N other cards from it (the oldest); a permanent cast this way keeps that, for `escapes with`. The keyword `exploit` (CR 702.110) sacrifices a creature as it enters, if its controller wants, and `triggered when this exploits a creature:` abilities follow. `discover 4` (CR 701.57) is an effect. |
| `madness {1}{R}`, `foretell {1}{U}`, `suspend 4 {1}{U}` | Madness (CR 702.35): discarded into exile, then may be cast for this cost (else to the graveyard). Foretell (CR 702.143): on its owner's turn, exiled from hand for {2}; cast on a later turn for this cost. Suspend N (CR 702.62): exiled from hand for this cost with N time counters; one comes off each upkeep, and as the last does it may be cast free (a creature has haste). The keywords `cascade` and `rebound` need no cost. |
| `evoke {1}{U}{U}`, `bloodthirst 2` | Evoke (CR 702.74): cast for this cost instead, and the permanent is sacrificed as it enters (`if evoked` is the trigger condition). Bloodthirst (CR 702.54): enters with N +1/+1 counters if an opponent was dealt damage this turn (`opponent damaged this turn` is also a static condition). |
| `crew 3` | Crew (CR 702.122): "Tap any number of other untapped creatures you control with total power 3 or more: this Vehicle becomes an artifact creature until end of turn." The engine taps the weakest creature strong enough alone, else the strongest until the power suffices. |
| `morph {2}{U}`, `megamorph {G}`, `disguise {1}{W}` | Cast face down as a nameless colorless 2/2 creature for {3}, turned face up any time its controller has priority for the cost (a special action; CR 702.37, 708); megamorph adds a +1/+1 counter as it turns up, and a disguised permanent has ward {2} while face down (CR 702.168). `triggered when this is turned face up: ...`. `manifest` is an effect: the top card of your library onto the battlefield face down (CR 701.34); a manifested creature card turns face up for its mana cost. |
| `bushido 2`, `soulshift 3`, `afflict 1`, `toxic 1`, `ward {2}`, `echo {2}{R}`, `unearth {B}`, `buyback {3}`, `outlast {1}{W}` | Keyword abilities with a number or cost (CR 702.45, .46, .130, .164, .21, .30, .84, .27, .107), printed as the keyword ("Bushido 2") in place and compiled to what they stand for: a blocks-or-blocked pump; a dies trigger that may return a Spirit card with that mana value or less; a becomes-blocked life loss; poison counters with combat damage to a player; a trigger countering an opponent's spell or ability that targets it unless they pay; an upkeep sacrifice unless paid, once after it comes under your control; an ability activated from the graveyard as a sorcery that returns it with haste and exiles it at the next end step; an optional additional cost that returns the spell to its owner's hand; and "{cost}, {T}: Put a +1/+1 counter on this creature. Activate only as a sorcery." `flanking` is a plain keyword. |
| `hand size none`, `hand size +1`, `hand size opponents -1` | "You have no maximum hand size.", "Your maximum hand size is increased by one.", "Each opponent's maximum hand size is reduced by one." (CR 402.2) |
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
card"), `discard this`, `exile this from graveyard` ("Exile this card from
your graveyard"; the ability is activated from the graveyard), `return
land you control` ("Return a land you control to its owner's hand") and
`tap untapped creature you control` ("Tap an untapped creature you
control"), chosen like a sacrifice, `pay 3
life`, `sorcery` (printed "Activate only as a sorcery."), `once` (activate
only once each turn). `none` for a free ability. An ability whose cost
includes `discard this` is activated from the hand (cycling, channel).

### Triggers

| Trigger | Oracle |
|---|---|
| `when this enters`, `whenever other creature enters` | enters the battlefield |
| `when this dies`, `whenever creature dies`, `whenever creature you control dies` | dies; for a noncreature, "is put into a graveyard from the battlefield". A filter without `other` includes the source ("this creature or another creature"). |
| `at beginning of your upkeep`, `at beginning of each draw step`, `at beginning of the end step` | beginning of a step; steps: `untap`, `upkeep`, `draw`, `combat`, `end` |
| `at beginning of every upkeep` | "At the beginning of each upkeep" (werewolves' wording) |
| `whenever this attacks`, `whenever creature you control attacks`, `whenever this blocks` | attacks or blocks |
| `whenever this deals combat damage to a player` | |
| `whenever you gain life` | |
| `whenever creature you control attacks alone` | attacks alone (CR 506.5) |
| `whenever this becomes blocked`, `whenever this blocks or becomes blocked` | |
| `whenever you cast a spell`, `... a noncreature spell`, `... an instant or sorcery spell` | |
| `when this becomes targeted` | becomes the target of a spell or ability |
| `whenever this becomes targeted` | "Whenever this creature becomes the target of a spell or ability" (`when` prints "When") |
| `whenever this becomes tapped`, `whenever this becomes untapped` | |
| `whenever this is dealt damage` | (enrage) |
| `whenever this deals damage`, `... deals damage to creature`, `... to player`, `... to opponent`, `... deals combat damage to creature` | the damaged creature is "that creature", the damaged player "that player" |
| `whenever you cast artifact spell`, `whenever a player casts Goblin spell`, `whenever an opponent casts multicolored spell` | a cast trigger with any filter |
| `whenever this becomes targeted by opponent` | "... becomes the target of a spell or ability an opponent controls" (ward's trigger) |

Conditions (intervening "if", CR 603.4): `if this is untapped`,
`if no creatures are on the battlefield`, `if kicked` ("if it was kicked",
CR 702.33c, for a permanent with `kicker`), `if it had no -1/-1 counters`
("if it had no -1/-1 counters on it", judged as the source last existed),
any static condition below (`if you control artifact`, `if attacked this
turn`),
`if no spells were cast last turn`, `if a player cast two or more spells
last turn`.

Static conditions: `7 cards in your graveyard` ("there are seven or more
cards in your graveyard"), `4 card types in your graveyard`, `you control 3
artifact` ("you control three or more artifacts"), `you control Island`
("you control an Island"), `your turn` ("it's your turn"), `attacking`,
`enchanted`, `equipped` ("it's attacking", or "this creature is
attacking" when the subject is something else), `opponent life 10 or
less` ("an opponent has 10 or less life"), `your life 30 or more`,
`player life 13 or less` ("a player has 13 or less life"), `defending
player controls Island`, `you control no untapped land`, `you control
other Goblin` ("you control another Goblin"), `attacked this turn` (raid),
`creature died this turn` (morbid), `hand 0 or less` ("you have no cards
in hand"), `hand 1 or less` ("you have one or fewer cards in hand"), and
the shorthands `threshold`, `delirium`, `metalcraft`.

### Replacement events and outcomes

Counter kinds: `+1/+1`, `-1/-1`, `loyalty`, `stun` (CR 122.1d: removed
instead of untapping), `charge`, `oil`, `spore`, `depletion`, `quest`,
`ki`, `time`, `tide`, `shield`, `age`, `fade`, `ice`, `level`, `lore`,
`page`, and the keyword counters `flying`, `first strike`, `trample`,
`lifelink`, `deathtouch`, `reach`, `menace`, `vigilance`, `hexproof`,
`indestructible` (CR 122.1b: the permanent has the keyword). A cost may
`remove 1 charge counter from this`.

`this becomes 4/4 green Bear artifact creature` (colors before
subtypes), `target creature becomes blue`, `this becomes colorless`,
`target land becomes Island` (all until end of turn). Chosen as the
effect resolves (CR 608.2d), once for all its recipients: `target creature
becomes color of choice`, `target creature gains protection from color of
choice`, `target land becomes land type of choice` ("the basic land type of
your choice"), `target land becomes Plains or Island` (one of those),
`enchanted creature becomes creature type of choice` (it loses its other
creature types, CR 205.1b) and `target creature becomes creature type of
choice except Wall` ("Choose a creature type other than Wall. Target
creature becomes that type until end of turn."), all until end of turn.
`this spell costs {3} less if it targets tapped creature` (or `more`):
"This spell costs {3} less to cast if it targets a tapped creature." Its
targets are then held to ones the caster can pay for. Triggers `when
this leaves the battlefield`, `whenever this attacks or blocks`.
Cost item `2 times per turn`: "Activate no more than twice each turn."
`that much` is the triggering event's amount ("Whenever this creature is
dealt damage, you gain that much life": `you gain that much life`).
`count each 2 creature you control` ("2 life for each creature you
control"); filter `defending player controls` (two players: the
attacker's opponent).
`if you control Treefolk, draw 1 card`: a static condition judged as the
effect resolves ("If you control a Treefolk, draw a card."). `that target
1` prints "that creature" even after the first mention.
`tap or untap target creature` (the controller chooses on resolution).
`spells red or white you cast cost {1} less` prints "Red spells and white
spells"; `spells Goblin or Rogue ...` prints "Goblin spells and Rogue
spells". `defending player` as a subject prints "defending player".

Cost item `from graveyard`: activated from the graveyard ("Return this
card from your graveyard to your hand." for `return this to hand`).
`, and` between clauses repeats the subject ("you draw a card and you
lose 1 life"). Filters: `your opponents control`, `creature with flying
or reach`.

`triggered when ...` on a trigger Oracle words "Whenever" prints "When"
("When this creature blocks, ..."). `look at hand of target player`
("Look at target player's hand."). `gets +1/+1 X` is "+X/+X".

Energy (CR 107.14): `you get 2 energy` ("you get {E}{E}"), `pay 2
energy` inside `may` and as a cost ("Pay {E}{E}").

Filters may have an alternative: `creature or Vehicle`, `land or
nonblack creature`, `attacking or blocking creature`, `Bear, Spider, or
Wolf you control`. A spell target may say what the spell targets:
`target spell that targets you or a permanent you control`. A group of
targets can be one subject: `up to 2 target creature gets +2/+0`
("Up to two target creatures each get +2/+0"), later `targets 1 to 2`. A trailing
`you control`, a leading `another`, and a type after two subtypes
(`Elf or Soldier creature`) apply to both sides.

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

More filter words: `tapped`, `untapped`, `attacking`, `blocking`,
`damaged` (also written `that was dealt damage this turn`),
`multicolored`, `monocolored`, `colorless`, `basic`, `legendary`, `snow`,
colors joined with `or` (`red or green creature`, sharing a color), types
joined with `or` or listed (`artifact or enchantment`, `artifact,
enchantment, or land`: at least one), negations listed with commas
(`nonartifact, nonblack creature`), and `with
power 2 or less`, `with toughness 4 or greater`, `with mana value 3`.

### Modifications (static abilities)

Comma-separated, each in one layer of CR 613: `+1/+1`, `base 1/1`,
`base mv` (base power and toughness each equal to its mana value),
`has flying, trample`, `loses all abilities`, `add type creature`,
`loses flying`, `set land Mountain`, `add land Swamp`, `add subtype Saproling`, `control`
(you control it), `add color white`, `set color blue`, `set color chosen`
and `has protection from chosen color` (the color the source chose as it
entered), `switch pt`,
`cda color all`, `cda pt card types in graveyards +1`,
`cda pt creature cards in graveyards +1`, `cda pt count Swamp you control`,
`cda pt your life`, and `has ability { <ability line> }`: a quoted ability
granted in layer 6 ("Enchanted creature has \"{T}: This creature deals 1
damage to any target.\""), written as its own ability line (`activated
{T}: deal 1 damage to any target`), where `this` is the object that has
it.

### Effects

Effects are clauses. `;` separates sentences; `and` joins clauses into one
sentence ("target player loses 1 life and you gain 1 life") and `then`
joins them with ", then" ("draw 1 card then discard 1 card"); a clause
after `and` may leave its subject implied ("you draw 1 card and lose 1
life").
Numbers are digits, `X`, or a count: `count where <expr>` ("X ...,
where X is the number of ..."), `count equal <expr>` ("... equal to the
number of ..."), or `count each <expr>` ("... for each ..."), with
`<expr>` a filter (`creature you control`, `Elf on battlefield`),
`domain`, `hand`, `graveyard [<filter>]`, `power` (the source's), `power
of target 1`, or `life`; `this gets +1/+1 count each creature you
control` scales a pump, and in a static ability `+1/+1 count each
artifact you control` is "gets +1/+1 for each artifact you control"; "until end of turn" is implied by `gets`,
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
`regenerate <what>`, `exile <what> [and <what>]` (a permanent or a graveyard card),
`exile all graveyards`, `put <what> on top of library` / `on bottom of
library`, `<who> sacrifices 1 creature` ("of their choice"), `<what>
loses flying`, `<who> reveals hand`, `<who> reveals hand discarding
nonland card` (Duress: you choose the card), `<who> discards 1 card at
random`,
`sacrifice <what>`, `tap <what>`, `untap <what> [and <what>]`,
`gain control of <what>`, `attach to <what>`, `create 2 "Soldier"`,
`create token copy of <what> [with exceptions]`, `emblem "Name"`,
`draw 2 cards` (you), `discard 1 card` (you), `mill 2 cards` (you),
`scry 2`, `surveil 1`, `investigate` (create a Clue token), `transform
this`,
`add {B}{B}` / `add any` ("Add {B}{B}.", "Add one mana of any color."; an
activated ability that only adds mana is a mana ability and resolves at
once, CR 605), `search basic land card reveal to hand`, `search up to 2
Forest card to battlefield tapped`, `search basic land card reveal to
top` ("Search your library for ..., then shuffle"; add `that card` to
print "that card" for "it"),
`counter target spell`, `counter target spell unless its controller pays {3}`,
`you may <clause>`, `pay {2}` / `pay 2 life` (after `you may`), `if you do,
<clause>` (only if the optional action before it happened: "You may pay
{2}. If you do, draw a card."), `<clause> unless you pay {1}` / `unless
they pay 2 life` / `unless its controller pays {2}` (the payer may pay to
stop it), `add {R} or {G} or {W}` ("Add {R}, {G}, or {W}."), `adapt 2` (CR 701.46:
+1/+1 counters only if it has none), `<what> connives` (CR 701.50), `dig 4 take creature
card may revealing rest bottom` ("Look at the top four cards of your
library. You may reveal a creature card from among them and put it into
your hand. Put the rest on the bottom of your library in any order."; `dig
3 take card rest graveyard`: "Put one of them into your hand and the rest
into your graveyard"; `dig 4 reveal take all land card rest graveyard`:
"Reveal the top four cards ... Put all land cards revealed this way into
your hand and the rest into your graveyard"), `look at top 3` ("look at
the top three cards of your library, then put them back in any order"),
`return 1 land you control chosen` ("return a land you control to its
owner's hand"), `switch pt of target
creature` ("Switch target creature's power and toughness until end of
turn"), `put land card from hand onto battlefield [tapped]` ("put a land
card from your hand onto the battlefield"), `<what> doesn't untap next
untap step` ("It doesn't untap during its controller's next untap
step."), `<what> gains ability { <ability line> }` ("Target creature
gains \"...\" until end of turn."), `proliferate` (CR
701.27; the engine makes the usual choices: your permanents' +1/+1 and
loyalty counters, opponents' -1/-1 counters and poison), `<what>
explores` (CR 701.44), `amass Zombies 2` (CR 701.47; needs the token
"0/0 black Zombie Army creature"), `<who> gets 1 poison counters`, `add {G}
count each Elf on battlefield` ("Add {G} for each Elf on the
battlefield."), `if kicked, <clause>` ("If this spell was kicked, ..."
for a spell with `kicker`), `prevent all [combat] damage
[to <affects>] this turn`, `prevent next 4 damage to <affects> this turn`,
`this turn, replace <event>: <outcome>` (`this turn, replace target 1
dies: exile`: "If that creature would die this turn, exile it
instead."), `players can't gain life this turn`,
`damage can't be prevented this turn`, and
`delayed <what>: <effects> at beginning of the next end step` (a delayed
triggered ability, CR 603.7; write the trigger first, then a comma, then the
effects, to print it that way); `at beginning of the next turn's upkeep`
also works ("Draw a card at the beginning of the next turn's upkeep"). Inside a delayed ability `it` is the object
the ability is about.

Who and what (`<who>`, `<what>`): `you`, `each opponent`, `each player`,
`its controller` / `its owner` (of the last target declared),
`this` (the source), `that player` / `that creature` (what triggered the
ability), `defending player`, `each creature`, `each creature you control`,
`those creatures` (the same, mentioned again), `each permanent you control`,
`enchanted creature` / `equipped creature` / `attached` (what this Aura or
Equipment is attached to),
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
targets of one kind (two different objects, CR 115.3); `up to 2 target
creatures` declares optional ones (CR 601.2c), which come after an
ability's other targets. `another target creature` excludes the source.
Specifications: `any target`, `creature`, `player`, `player or planeswalker`,
`opponent or planeswalker`,
`creature or planeswalker`, `land`, `tapped creature`, `creature you control`,
`nonlegendary creature you control`, `artifact`, `enchantment`,
`artifact or enchantment`, `artifact, creature, or land`, `permanent`,
`another creature`, `another permanent`, `creature card from your graveyard`,
`spell`, `creature spell`, `noncreature spell`, `<filter> spell`
("target instant or sorcery spell"), `card from a graveyard`, `<filter>
card from your graveyard` / `from an opponent's graveyard`, `opponent`,
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
