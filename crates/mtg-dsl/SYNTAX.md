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
`{Q}`, the untap symbol (CR 107.6), is a cost that untaps the source:
payable only while it is tapped, and for a creature only once it has been
under its controller's control since their most recent turn began (CR
302.6); automatic mana payment never uses a {Q} mana ability.

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
Clue, Blood, Map, Powerstone, Mutagen: CR 111.10) prints by name: "create
a Treasure token". Other tokens print described: their keywords and
keyword abilities bare, their restrictions and other abilities quoted ("a
1/1 colorless Phyrexian Mite artifact creature token with toxic 1 and
\"This token can't block.\""; "a 0/1 colorless Eldrazi Spawn creature
token with \"Sacrifice this token: Add {C}.\""; `can't attack alone, can't
block alone` is one quotation, "This token can't attack or block
alone.", and so are `can't block non-Spirit creature` and `can't be
blocked by non-Spirit creature`, "This token can't block or be blocked by
non-Spirit creatures."), and a creature token
whose name is not its subtypes is named ("a 2/2 colorless Wizard Soldier
creature token named Cadet"); a token of all five colors is "a 2/2
Elemental creature token that's all colors" (The Wandering Minstrel); so is a noncreature token with no subtype
("a colorless artifact token named Munitions with \"...\""), and an Equipment token, its abilities quoted without their final period and its equip ability last ("a colorless Equipment artifact token named Axe with \"Equipped creature gets +1/+0\" and equip {2}"; with several quoted abilities the list's commas go inside the quotation marks: "... token with \"Equipped creature gets +2/+0,\" \"Whenever equipped creature attacks, ...,\" and equip {2}"). A Role token (CR 111.10j–r: an `Enchantment
— Aura Role` with `enchant creature` and `name "Wicked"`, keyed `"Wicked
Role"`) prints by name too: "a Wicked Role token". If one player controls
more than one Role on a permanent, all but the newest go to the graveyard
(CR 303.7a, 704.5z). A legendary token (`type Legendary
Creature — Elder Alien`) with a name of its own is named first, and its
abilities call it by name: "Create Galactus, a legendary 16/16 black
Elder Alien creature token with flying, trample, and \"Whenever Galactus
attacks, destroy target land.\"" A token with a characteristic-defining
ability prints its abilities after the sentence ("... token. It has
\"...\""), except a legendary one named first ("Create Beau, a legendary
blue Ox creature token with \"Beau's power and toughness are each equal
to the number of lands you control.\"", Bonny Pall, Clearcutter).

## Multi-face cards

A split card, adventurer, omen, or double-faced card is one block named as
Scryfall names it (`card "Curious Pair // Treats to Share"`) with a
`layout` line (`adventure`, `omen`, `transform`, `split`, `prepare`, `flip`,
`modal_dfc`) and a `face
"Name" { ... }` block per other face, holding that face's header and
ability lines. The block's own lines are the front face (a split card's
block has none: both halves are faces). The gate compares each face with
the snapshot's. In the engine each face is its own definition: an
adventure or omen is cast from the hand as its spell face (CR 715.3) and
goes on an adventure in exile as it resolves (an omen is shuffled into
the library instead), whence the creature may be cast; a split card's
halves are cast separately (CR 709.3); `transform this` turns a
double-faced permanent to its other face (CR 701.27). A preparation card
(`layout prepare`, CR 722) has its prepare spell as its face: never cast
itself, but while the permanent is prepared (`enters prepared`, "This
creature enters prepared."; the effect `this becomes prepared`, "this
creature becomes prepared", CR 722.3a; the trigger condition `if this
isn't prepared`) its controller may cast a copy of the spell, with the
spell's timing and costs, which unprepares it (CR 722.3c). The copy ceases
to exist as it leaves the stack. A modal double-faced card (`layout
modal_dfc`, CR 712.3) has its back face as its face: from the hand (or
wherever it may be played) either face may be cast, or played if it is a
land (CR 712.11b, 712.12), and the permanent has the face it was cast or
played as; anywhere else it is its front face. `transform this` turns it
to its other face as well.

A flip card (`layout flip`, CR 710) has its top half as the block and its
bottom half as its face, without a `cost` line (the face keeps the
card's mana cost and so its color, CR 710.1c). The effect `flip this`
("flip this creature", "flip Kuon" for a legendary card; `flip it` is
the source too, "..., you may flip it.", even in a trigger with no
triggering object) turns an unflipped flip card on the battlefield to its
bottom half, the same object with its counters, until it leaves the
battlefield (CR 710.2, 710.4); it does nothing to anything else, and
`transform` does nothing to a flip card. In scenarios the permanent is
named by the half it shows ("Callow Jushi", then "Jaraku the
Interloper").

Meld cards (CR 712.4) are single-faced cards; the combined back face is a
card of its own with no `cost` (`card "Chittering Host" { type Creature
— Eldrazi Horror ... }`). The effect `meld with "Midnight Scavengers"
into "Chittering Host"` prints "if you both own and control this
creature and a creature named Midnight Scavengers, exile them, then meld
them into Chittering Host" (capitalized first in an activated ability,
Hanweir Battlements); in a triggered ability it is an intervening "if"
too (CR 603.4). Both are exiled; if both are still the pair's cards in
exile (not tokens, CR 701.42b-c), the source's card enters as one
permanent with the back face's characteristics (its "When this creature
enters" triggers), and the other card waits in exile; when the permanent
leaves the battlefield, both cards go to the new zone (CR 712.21).

`keywords aftermath` on a split card's half (CR 702.127a, printed
"Aftermath" first in the half's text): that half can be cast only from
its owner's graveyard (for its mana cost, with its own timing; in
scenarios `cast "Cut // Ribbons" from graveyard`, though the runner reads
the targets of that cast from the card, so an aftermath half with targets
can't be scripted yet), and the spell is exiled as it leaves the stack;
the other half is cast from the hand as usual and can't be cast from the
graveyard. `keywords fuse` on both halves (CR 702.102, printed "Fuse"
after each half's text): from the hand, both halves may be cast together
as a fused split spell, for both mana costs, with the left half's targets
and then the right half's (the same object may be chosen for one of each,
CR 115.3), doing the left half's instructions and then the right half's.
The fused spell is the card's third face (`CardPool::add_fused`), named
like the card; either half may still be cast alone.

## Header

| Line | Meaning |
|---|---|
| `oracle_id 2b93787d-...` | The Scryfall `oracle_id` the card was authored against. Required for cards. |
| `cost {1}{G}` | Mana cost. Omit for lands and tokens. Hybrid `{R/G}`, monocolored hybrid `{2/W}`, Phyrexian `{R/P}`, and hybrid Phyrexian `{G/U/P}` (either color or 2 life, both colors, CR 107.4f) symbols are accepted (CR 107.4e–f); the engine pays hybrid symbols in a canonical order. As a spell's total cost is paid its caster announces how many of its Phyrexian symbols they pay with 2 life each (CR 601.2b), offered only the numbers the rest of the cost can then be paid with and their life covers (CR 119.4); the choice is remembered for compleated. Scenarios: `cast "Dismember" paying_life 1 ...`; unscripted, the fewest. An activated ability's Phyrexian symbols are paid with mana where it can. Snow `{S}` (CR 107.4h, in any cost) is paid by tapping a different snow permanent for each symbol (the rest of the cost from the other sources and the pool); mana already floating in the pool pays no `{S}`, since the pool doesn't remember where its mana came from. `protection from snow` prints "Protection from snow". |
| `type Legendary Creature — Goblin Shaman` | The type line: supertypes, types, then subtypes after `—` (or `-`). |
| `pt 2/2` | Power and toughness (`pt -2/4`: a negative printed power, Loot, the Anomaly). Omit when a characteristic-defining ability sets them (Scryfall prints `*`). When it sets one of them, write the other and `*` for it: `pt */1` with `cda power ...`, `pt 2/*` with `cda toughness ...` (`*` is 0 outside the battlefield, CR 208.2a). |
| `loyalty 3` | A planeswalker's printed loyalty. |
| `defense 5` | A battle's printed defense (CR 310.4): it enters with that many defense counters. |
| `short_name "Ajani"` | The name a legendary card's text calls itself by when it isn't the part before a comma (CR 201.5): "put a loyalty counter on Ajani" on Ajani Resolute. The template author adds it when the text uses the name's first word alone. On a card that isn't legendary, the card names itself with it everywhere instead of "this creature" (`short_name "Enduring Innocence"`: "When Enduring Innocence dies", a card that may not stay a creature). |
| `pronoun he`, `pronoun she`, `pronoun they` | Wording only: after naming it, the card's text calls itself "he"/"him"/"his", "she"/"her", or "they"/"them"/"their" instead of "it" (and `they` makes a trigger about it plural: "Whenever Bebop & Rocksteady attack or block") ("Put a +1/+1 counter on Abomination. He fights up to one target creature an opponent controls."; "Morlun enters with X +1/+1 counters on him."; after naming it, `if this is attacking, ...` prints "If he's attacking, ..."; `pronoun they` gives the card plural verbs as a subject, "The Neutrinos get +1/+0", and "if you cast them"; a quoted ability it grants itself uses the pronoun too, "Iron Fist deals damage equal to his power"). A legendary card names itself where "it" would be ambiguous: "where X is Jessica Jones's power" after a cost naming it ("{T}, Put a stun counter on Jessica Jones: ..."), "Other creatures you control get +1/+0 for each Aura and Equipment attached to Kellan", "until end of turn, Moon Girl and Devil Dinosaur's base power and toughness become 6/6"; a granted `cda power` prints "base power" ("Ms. Marvel gains \"Ms. Marvel's base power is equal to ...\""). The template author adds it for a legendary card whose Oracle text uses those words. |
| `color green` | Color indicator (CR 204): for cards and tokens whose color is not their cost's. |
| `keywords flying, deathtouch, lifelink` | Keyword abilities, in printed order: `deathtouch`, `defender`, `double strike`, `first strike`, `flash`, `flying`, `haste`, `hexproof`, `indestructible`, `lifelink`, `menace`, `prowess`, `reach`, `trample`, `vigilance`, and the landwalks `plainswalk`, `islandwalk`, `swampwalk`, `mountainwalk`, `forestwalk` (CR 702.14), `protection from white` (and the other colors, CR 702.16), `devoid` (CR 702.114), `changeling` (CR 702.73), `shroud`, `fear`, `intimidate`, `shadow`, `horsemanship`, `skulk`, `wither`, `infect` (damage to players as poison counters; ten lose, CR 704.5c), `exalted`, `persist`, `undying`, `flanking`, `convoke` (CR 702.51: untapped creatures may be tapped to pay {1} or one mana of their color), and `improvise` (CR 702.126: artifacts pay {1}), and `banding` (CR 702.22: attacking bands, blocked as a group, the banding player dividing the combat damage; "bands with other" is not supported), `decayed` (CR 702.147: can't block; when it attacks, it's sacrificed at end of combat), and `phasing` (CR 702.26: phases in or out before its controller untaps). The restrictions `can't attack`, `can't block`, `attacks each combat if able`, `can't be blocked`, `can't attack alone`, and `can't block alone` (both together print "can't attack or block alone", CR 506.5), `doesn't untap` ("This creature doesn't untap during your untap step."; granted, "Enchanted creature doesn't untap during its controller's untap step."), `can't become untapped` (no effect or untap step untaps it, and a stun counter stays on it; granted, "Enchanted creature loses all abilities and can't become untapped."), `untaps during each other player's untap step` ("Untap this creature during each other player's untap step.", CR 502.3: it untaps in the other players' untap steps too; granted to a group, `static permanent you control: has untaps during each other player's untap step` prints "Untap all permanents you control during each other player's untap step.", `static andlist creature or land you control: ...` "... all creatures and lands you control ...", `static andor green or blue creature you control: ...` "... all green and/or blue creatures ...", and with `each` "Untap each creature you control with a +1/+1 counter on it during ..."; granted by an Aura, `static enchanted: has untaps during each other player's untap step` is quoted, 'Enchanted land has "Untap this land during each other player's untap step."'), `assigns combat damage equal to toughness` ("This creature assigns combat damage equal to its toughness rather than its power.", CR 510.1a; `static each creature you control: has ...` prints "Each creature you control assigns ..."; gained by a group, `until end of turn, each creature with toughness greater than power you control gains assigns combat damage equal to toughness` "until end of turn, creatures you control with toughness greater than their power assign combat damage equal to their toughness rather than their power", The Kingpin of Crime), `assigns combat damage as though its power were positive` ("If Loot's power is negative, he assigns combat damage as though his power were positive.": a negative power assigns its absolute value), and the blocking requirements and allowances (CR 509.1a, 509.1c) `blocks each combat if able`, `must be blocked if able`, `lure` ("All creatures able to block this creature do so."), `can block an additional creature each combat`, and `can block any number of creatures`, and `can attack as though it didn't have defender` (CR 702.3b; granted to a group, "... as though they didn't have defender"; with `can't be blocked` after it, "... and it can't be blocked") are written as keywords too and print as sentences ("This creature can't block."). A `keyword` line after an ability line prints in that place instead of first (a bridge land's "Indestructible" after "enters tapped"). |
| `keywords flying, deathtouch, lifelink` | Keyword abilities, in printed order: `deathtouch`, `defender`, `double strike`, `first strike`, `flash`, `flying`, `haste`, `hexproof`, `indestructible`, `lifelink`, `menace`, `prowess`, `reach`, `trample`, `vigilance`, and the landwalks `plainswalk`, `islandwalk`, `swampwalk`, `mountainwalk`, `forestwalk` (CR 702.14), `protection from white` (and the other colors, CR 702.16), `devoid` (CR 702.114), `changeling` (CR 702.73), `shroud`, `fear`, `intimidate`, `shadow`, `horsemanship`, `skulk`, `wither`, `infect` (damage to players as poison counters; ten lose, CR 704.5c), `exalted`, `persist`, `undying`, `flanking`, `convoke` (CR 702.51: untapped creatures may be tapped to pay {1} or one mana of their color), and `improvise` (CR 702.126: artifacts pay {1}), and `banding` (CR 702.22: attacking bands, blocked as a group, the banding player dividing the combat damage; "bands with other" is not supported), `decayed` (CR 702.147: can't block; when it attacks, it's sacrificed at end of combat), and `phasing` (CR 702.26: phases in or out before its controller untaps). The restrictions `can't attack`, `can't block`, `attacks each combat if able`, `can't be blocked`, `can't attack alone`, and `can't block alone` (both together print "can't attack or block alone", CR 506.5), `doesn't untap` ("This creature doesn't untap during your untap step."; granted, "Enchanted creature doesn't untap during its controller's untap step."), `can't become untapped` (no effect or untap step untaps it, and a stun counter stays on it; granted, "Enchanted creature loses all abilities and can't become untapped."), `untaps during each other player's untap step` ("Untap this creature during each other player's untap step.", CR 502.3: it untaps in the other players' untap steps too; granted to a group, `static permanent you control: has untaps during each other player's untap step` prints "Untap all permanents you control during each other player's untap step.", `static andlist creature or land you control: ...` "... all creatures and lands you control ...", `static andor green or blue creature you control: ...` "... all green and/or blue creatures ...", and with `each` "Untap each creature you control with a +1/+1 counter on it during ..."), `may choose not to untap` ("You may choose not to untap this creature during your untap step.", CR 502.3: as its controller's untap step untaps their permanents, they are asked about each such tapped permanent; scenarios: `turn 5 untap A: accept "Phyrexian Gremlins"` keeps it tapped, unscripted it untaps), `can't be equipped` ("This creature can't be equipped.", CR 301.5c: equip may target it, but no Equipment becomes attached to it, and one already attached becomes unattached, CR 704.5n), `assigns combat damage equal to toughness` ("This creature assigns combat damage equal to its toughness rather than its power.", CR 510.1a; `static each creature you control: has ...` prints "Each creature you control assigns ..."), and the blocking requirements and allowances (CR 509.1a, 509.1c) `blocks each combat if able`, `must be blocked if able`, `lure` ("All creatures able to block this creature do so."), `can block an additional creature each combat`, and `can block any number of creatures`, and `can attack as though it didn't have defender` (CR 702.3b; granted to a group, "... as though they didn't have defender"; with `can't be blocked` after it, "... and it can't be blocked") are written as keywords too and print as sentences ("This creature can't block."). A `keyword` line after an ability line prints in that place instead of first (a bridge land's "Indestructible" after "enters tapped"). |
| `additional cost sacrifice creature` | "As an additional cost to cast this spell, ..." (CR 601.2b). Same syntax as an activated ability's cost. |
| `additional cost sacrifice creature` then `additional cost or {3}{B}` | "As an additional cost to cast this spell, sacrifice a creature or pay {3}{B}." (CR 601.2b): two additional costs, of which the caster pays one, chosen as the spell is cast (before the kicker and X); only those that can be paid are offered. A mana part prints "pay {3}{B}", and the total cost includes it (CR 601.2f). |
| `alternative cost sacrifice Mountain`, `alternative cost sacrifice 2 Mountain`, `alternative cost sacrifice Mountain, if your turn`, `alternative cost {1}, return basic land you control` | An alternative cost of the card's own (CR 118.9): "You may sacrifice a Mountain rather than pay this spell's mana cost.", "You may sacrifice two Mountains ...", "If it's your turn, you may ...", "You may pay {1} and return a basic land you control to its owner's hand ...". Mana and one kind of permanent (sacrificed, returned with `return`, or tapped with `tap untapped creature you control`), offered while the condition holds; the chosen permanent is the first and the engine picks the rest. Not combined with an additional cost that also chooses a permanent. `alternative cost pay 4 life, if you control Swamp` pays life (offered only with that much life, CR 119.4); `alternative cost if cast another white spell this turn`, with no cost at all, prints "If you've cast another white spell this turn, you may cast this spell without paying its mana cost."; `alternative cost reveal hand, if hand has no land card` prints "If you have no land cards in hand, you may reveal your hand rather than pay this spell's mana cost." (every card left in the hand is then known to every player, CR 701.20). `alternative cost exile blue card from hand` (a pitch spell) prints "You may exile a blue card from your hand rather than pay this spell's mana cost." (`exile 2 white cards from hand`: "exile two white cards from your hand"; `, if not your turn` for "If it's not your turn, ..."): the caster chooses the card as a discard is chosen (scenarios: `cast "Snapback" alternative discarding "Wind Drake"`; the engine picks any others), never the spell itself, and it is exiled, not discarded. `alternative cost each other player gains 6 life, if you control Forest` prints the older wording "If you control a Forest, rather than pay this spell's mana cost, you may have each other player gain 6 life." (the cost is paid by each other player gaining that life). |
| `alternative cost sacrifice Mountain`, `alternative cost sacrifice 2 Mountain`, `alternative cost sacrifice Mountain, if your turn`, `alternative cost {1}, return basic land you control` | An alternative cost of the card's own (CR 118.9): "You may sacrifice a Mountain rather than pay this spell's mana cost.", "You may sacrifice two Mountains ...", "If it's your turn, you may ...", "You may pay {1} and return a basic land you control to its owner's hand ...". Mana and one kind of permanent (sacrificed, returned with `return`, or tapped with `tap untapped creature you control`), offered while the condition holds; the chosen permanent is the first and the engine picks the rest. Not combined with an additional cost that also chooses a permanent. `alternative cost pay 4 life, if you control Swamp` pays life (offered only with that much life, CR 119.4); `alternative cost if cast another white spell this turn`, with no cost at all, prints "If you've cast another white spell this turn, you may cast this spell without paying its mana cost."; `alternative cost reveal hand, if hand has no land card` prints "If you have no land cards in hand, you may reveal your hand rather than pay this spell's mana cost." (every card left in the hand is then known to every player, CR 701.20). `alternative cost {1}{W}, exile creature card from graveyard` prints "You may pay {1}{W} and exile a creature card from your graveyard rather than pay this spell's mana cost."; `alternative cost exile top 3 black cards from graveyard` "You may exile the top three black cards of your graveyard rather than ..." (the most recent matching cards; without `top`, the oldest). An alternative cost prints before the keywords. |
| `flash as long as you control Merfolk`, `flash if you control Human` | "This spell has flash as long as you control a Merfolk." (CR 702.8): the card may be cast as though it had flash while the condition holds. The `if` form prints "You may cast this spell as though it had flash if you control a Human." |
| `flash with cleanup sacrifice` | "You may cast this spell as though it had flash. If you cast it any time a sorcery couldn't have been cast, the controller of the permanent it becomes sacrifices it at the beginning of the next cleanup step." (Spider Climb): cast other than in its controller's main phase with an empty stack (CR 307.1), the permanent gets a delayed trigger at the next cleanup step (CR 514.3a). |
| `cast only during combat`, `cast only during opponent upkeep`, `cast only during combat before blockers`, `cast only during combat after blockers`, `cast only during declare attackers`, `cast only during declare attackers if attacked`, `cast only if you control creature` | "Cast this spell only during combat." (CR 601.3), printed first: the card is offered for casting from the hand only while the condition holds for its caster (any static condition, or a part of the turn as for activation restrictions). `if attacked` prints "during the declare attackers step and only if you've been attacked this step" (a creature attacks the caster; not one of their planeswalkers). |
| `kicker {1}{U}` | Kicker (CR 702.33): an optional additional mana cost, chosen as the spell is cast. Prints "Kicker {1}{U}" after the keywords. Effects that depend on it: `if kicked, <clause>` in a spell, `if kicked` as a trigger condition. `kicker sacrifice 2 land`, `kicker sacrifice artifact or creature`, `kicker return land you control` print "Kicker—Sacrifice two lands." and kin (CR 702.33a: a kicker cost other than mana): the permanent is chosen after targets, like an additional cost's (`kicked sacrificing "Forest"` in scenarios; the engine picks the rest of two), and kicking is offered only when there are enough of them. Such a card has no additional cost of its own. |
| `flashback {4}{R}` / `flashback {1}{U}, pay 3 life` / `flashback sacrifice 3 creature` / `flashback tap 3 untapped white creature you control` | Flashback (CR 702.34): the card may be cast from the graveyard for this cost and is exiled as it leaves the stack. The cost is mana, life, and a chosen permanent to sacrifice, return, or tap (chosen after targets like an additional cost's, so not on a card that also has an additional cost), or a behold (`flashback {1}{R}, behold 3 Elemental`, "Flashback—{1}{R}, Behold three Elementals."), or one card discarded (`flashback {1}{U/R}{U/R}, discard 1 card`, Twinned Vision: scenarios `from graveyard discarding "Mountain"`; not with a chosen permanent or an additional cost); the card can be cast this way only when every part can be paid. Prints last, as "Flashback {4}{R}" or "Flashback—Sacrifice three creatures." |
| `oracle_mismatch "reason"` | The rendered text is allowed to differ from Oracle; the gate reports it as allowed. Use it for cards whose text includes an ability the engine does not implement. |

## Abilities

| Line | Meaning |
|---|---|
| `mana {G}` / `mana {W} or {U}` / `mana any` / `mana {U} or chosen` | "{T}: Add {G}." A mana ability (CR 605); `any` is "one mana of any color", `chosen` "one mana of the chosen color" (the color chosen as the permanent entered; none chosen, no mana). Basic land types grant theirs intrinsically. |
| `mana any, sacrifice this` | "{T}, Sacrifice this artifact: Add one mana of any color." (Treasure, Lotus Petal). Payment uses such sources after all others. |
| `mana {C} only to cast instant or sorcery spell`, `mana any only to cast creature spell`, `mana {U} only to cast artifact spell or activate`, `mana {R} only to cast artifact spell or to activate`, `mana {C} only to activate or cast artifact spell`, `mana {R} only to cast Elemental spell or Chandra planeswalker spell`, `mana any only to cast Lesson or Shrine spell`, `mana {U} not to cast nonartifact spell` | "{T}: Add {C}. Spend this mana only to cast an instant or sorcery spell.", "... only to cast an artifact spell or activate an ability.", "... or to activate an ability.", "... only to activate an ability or cast an artifact spell.", "This mana can't be spent to cast a nonartifact spell." Two alternatives each written with `spell` print whole ("an Elemental spell or a Chandra planeswalker spell"); two types alone with one `spell`, `Lesson or Shrine spell`, share it ("a Lesson or Shrine spell"). Restricted mana (CR 106.6): one kind of mana; the engine taps the source only to pay for what the restriction allows (any activated ability for `activate`; for `not to cast`, other spells and any activated ability, though not costs an effect asks for), so its mana is spent at once and never sits in the pool. |
| `mana {U} only to cast artifact spell or activate of artifact`, `mana {U} only to foretell or cast instant or sorcery spell` | "Spend this mana only to cast an artifact spell or activate an ability of an artifact source." (`of <filter>` after `activate`: only abilities of matching sources, cards in hand and graveyard included); "Spend this mana only to foretell a card from your hand or cast an instant or sorcery spell." (the foretell special action's {2}, CR 702.143a). |
| `mana any only to cast creature spells`, `... only to cast instant or sorcery spells joined and`, `... spells joined andor`, `... only to cast artifact spells or activate of artifact bare`, `... or activate of Elemental`, `... only to activate abilities`, `... only to activate abilities of land`, `... only to activate of creature bare`, `... only to cast Elf spells and activate of Elf` | The plural wordings of a restriction: "Spend this mana only to cast creature spells.", "... instant and sorcery spells.", "... instant and/or sorcery spells.", "... artifact spells or activate abilities of artifacts." (`bare`: the source named without "source"), "... or activate abilities of Elemental sources.", "... only to activate abilities.", "... only to activate abilities of land sources.", "... only to activate an ability of a creature.", "... only to cast Elf spells and activate abilities of Elf sources." `spells` (or `abilities` after `activate`) makes the sentence plural; `joined and` / `joined andor` join two alternatives with "and" or "and/or" (else "or"). The rule is the same as the singular wording's. |
| `activated {1}, {T}: add 2 mana any combination only to cast legendary spells`, `spell: add {R}{R}{R}{R}{R}{R}{R} only to cast artifact or creature spells`, `triggered when this enters: add any 4 only to activate abilities` | "{1}, {T}: Add two mana in any combination of colors. Spend this mana only to cast legendary spells." and the like: a restriction (any of the forms above) after any `add` effect, in any ability or spell (CR 106.6). Unlike a `mana ... only to` line, the mana goes into the pool, each mana carrying the restriction: payment spends it only on what the restriction allows (after the unrestricted mana in the pool, before tapping permanents), never on a cost an effect asks for, and it empties with the pool as each step ends (CR 500.4). Payment doesn't activate such an ability on its own; a player activates it (a scenario's `activate`), then spends the mana. |
| `activated {U/B}, {T}: add {U}{U} or {U}{B} or {B}{B}` | "{U/B}, {T}: Add {U}{U}, {U}{B}, or {B}{B}." (the filter lands): the three combinations of two mana of two colors, listed in the color wheel's order ("{G}{G}, {G}{W}, or {W}{W}"); the same mana as `add 2 mana any combination of {U}{B}`, each mana's color chosen as it is added. Payment activates it, paying the hybrid symbol with mana of either color. |
| `only to power-up abilities`, `only to face up permanents`, `... only to cast enchantment spell plus unlock plus face up`, `... only to cast colorless spell or activate of colorless permanent bare plus cost with c`, `... only to cast Room spells plus and unlock doors`, `... only to cast spells face down plus to face up creatures`, `... only to cast Equipment spell plus equip`, `... only to cast instant or sorcery spell plus to class level`, `... only to cast creature spell face down plus face up morph`, `on costs with x` | Other things restricted mana may be spent on, after the spells and abilities (`plus`, each in printed order): "activate power-up abilities" (`power-up`; CR 702.193), "activate an equip ability" (`equip`), "gain a Class level" (`class level`: a Class's level-up ability), "unlock a door" (`unlock`, the special action, CR 709.5e), "turn a permanent face up" (`face up`, CR 116.2b; `face up creatures` prints "turn creatures face up", `face up morph` "pay a mana cost to turn a manifested creature face up, or pay a morph cost", the same rule), "pay a cost that contains {C}" (`cost with c`: any spell or ability whose mana cost has {C}). Alone after `only to` (no spells), `abilities`, `doors`, `permanents` give the plural wording ("Spend this mana only to turn permanents face up."). Three or more alternatives print as a list ("cast an enchantment spell, unlock a door, or turn a permanent face up"); `plus to` joins two with "or to", `plus and` with "and" ("cast Room spells and unlock doors"). `on costs with x` is "Spend this mana only on costs that contain {X}." (any spell or ability whose mana cost has {X}). |
| `... only to cast spells from exile`, `... only to cast spells from graveyard`, `... only to cast spell not from hand`, `... only to cast spells with flashback from graveyard`, `... only to cast spells face down`, `... only to cast with mana value 5 or greater spells or x spells`, `... only to cast creature of chosen type spell uncounterable` | What the spells must be: cast from exile ("spells from exile"), from the caster's graveyard ("from your graveyard"), from anywhere other than their hand ("a spell from anywhere other than your hand"), with flashback from a graveyard ("spells with flashback from a graveyard": the card has flashback, or was given it, and is cast from a graveyard), face down ("face-down spells", "a face-down creature spell", CR 708.4); an empty filter before `spells` is any spell. `or x <filter> spells` adds spells matching the filter that have {X} in their mana costs ("or spells with {X} in their mana costs", "or creature spells with {X} in their mana costs"). A filter's mana value is the card's, with X counted on the stack (CR 202.3e) and 0 before. `uncounterable` adds ", and that spell can't be countered" (Cavern of Souls): a spell this mana is spent to cast can't be countered. |
| `activated {T}: add {G} spent on Dragon creature spell: you gain 2 life` | "{T}: Add {G}. When you spend this mana to cast a Dragon creature spell, you gain 2 life.": the mana goes into the pool like restricted mana, spendable on any spell or ability payment offers it for (not on a cost an effect asks for); spent to cast a spell matching the filter, it triggers the sentence after the colon ("it" is that spell). Payment doesn't activate the ability on its own. `add {R} when spent on red instant or sorcery spell: copy that spell and new targets` prints "Add {R}. When that mana is spent to cast a red instant or sorcery spell, copy that spell and you may choose new targets for the copy." (the same rule). |
| `spend only black mana on x` | "Spend only black mana on X." (CR 107.3): the X in the spell's cost is paid with that color only. After an activated ability with {X} (`activated {X}: deal X damage to each creature and each player` then `spend only black mana on x`), it is that ability's X, printed in its paragraph (Crypt Rats). |
| `cycling {2}`, `cycling pay 2 life` | Cycling (CR 702.29): "{2}, Discard this card: Draw a card.", activated from the hand. A cost other than mana prints "Cycling—Pay 2 life." (activated only with that much life, CR 119.4). |
| `plainscycling {2}`, `basic landcycling {1}{G}`, `landcycling {2}` | Typecycling (CR 702.29e): discard to search for a card of that type. Two of them, one per line (`islandcycling {2}` then `swampcycling {2}`), match Oracle's one line "Islandcycling {2}, swampcycling {2}"; each is its own ability. |
| `enchant creature` | An Aura's "Enchant" (CR 702.5), with a target specification. `enchant player` (a Curse, CR 303.4): the Aura is cast targeting a player and enters attached to that player, the enchanted player; it goes to the graveyard if that player leaves the game (CR 704.5m). On such an Aura `enchanted` (as a recipient) and `controller of enchanted` are the enchanted player, `at beginning of enchanted controller's upkeep` prints "At the beginning of enchanted player's upkeep" (that player is `that player`), and the filter words `enchanted player controls` ("Creatures enchanted player controls get -1/-1", "Whenever a creature enchanted player controls enters") match objects that player controls, in a static ability's affected objects, a trigger's filter, and an effect's group or count. |
| `spell: <effects>` | What an instant or sorcery does on resolution. |
| `gift card`, `gift food`, `gift tapped fish`, `gift treasure` | Gift (CR 702.174): "Gift a card" ("a Food", "a tapped Fish", "a Treasure"), printed first. As the spell is cast its caster may promise the gift to the opponent; an instant or sorcery gives it (they draw a card, or create the token) before its other effects as it resolves, a permanent by its own "When this permanent enters, if its gift cost was paid" trigger. The engine keeps a promised gift as the kicker flag, so the card's "if the gift was promised" is written with the kicker conditions: `if kicked, <clause>` ("If the gift was promised, ..."; a target first named there is chosen only if it was, CR 702.174m, also in an `instead` after a sentence with no targets: `deal 2 damage to each creature; if kicked, instead deal 1 damage to any target and deal 2 damage to each creature`; the spell names itself again), `if kicked, also ...`, `triggered when this enters if kicked: ...`, `... if kicked` after a sentence, and `if not kicked, <clause>` ("If the gift wasn't promised, ..."). Trigger `whenever you give a gift` ("Whenever you give a gift, draw a card."): after a promised gift is given, as the spell resolves or as the permanent's gift trigger resolves. |
| `spree { mode {2}{B}: <effects> mode {1}: <effects> }` | Spree (CR 702.172): one to three modes, each with an additional cost ("Spree", then "+ {2}{B} — Destroy target creature."). The caster chooses one or more of them and pays their costs on top; each chosen mode keeps its own targets. The engine's modes are the sets of the printed modes, the set with bits N being mode N (in scenarios, `mode 3` is the first two). |
| `tiered { mode "Thunder" {0}: <effects> mode "Thundara" {3}: <effects> }` | Tiered (CR 702.183): two or more named modes, each with an additional cost; the caster chooses exactly one and pays its cost on top ("Tiered (Choose one additional cost.)", then "• Thunder — {0} — Thunder Magic deals 2 damage to target creature."). Only the modes whose cost can be paid are offered; a copy keeps the mode. In scenarios, `mode N` is the Nth mode. |
| `pawprint 5 { mode 1: <effects> mode 2: <effects> mode 3: <effects> }` | "Choose up to five {P} worth of modes. You may choose the same mode more than once." (CR 700.2d, 700.2i), then "{P} — ...", "{P}{P} — ...": each mode with its number of pawprints. The engine's modes are every nonempty combination, a mode repeated allowed, whose pawprints total at most the limit (at most sixteen), each instance with its own targets, in printed order; they are listed in dictionary order of their modes' numbers (for 1, 2, 3 pawprints and five: 1, 1+1, 1+1+1, 1+1+1+1, 1+1+1+1+1, 1+1+1+2, 1+1+2, 1+1+3, 1+2, 1+2+2, 1+3, 2, 2+2, 2+3, 3; in scenarios `mode 13` is 2+2). |
| `choose one { mode: <effects> ... }` | A modal spell (CR 700.2), one `mode` line per mode. `choose one or both { ... }` takes two modes, either or both ("Choose one or both —"). As a triggered ability's effect (`triggered when this enters: choose one { ... }`) the mode is chosen as the ability goes on the stack (CR 603.3c), each mode with its own targets. `choose two { ... }` (three or four unnamed modes) chooses two different modes (CR 700.2d; "Choose two —"), as a spell (`choose two { ... }` or `spell: choose two { ... }`) or as a triggered ability's effect (`triggered when this enters: choose two { ... }`: "When this creature enters, choose two —"); the engine offers the pairs of modes, in order. A named mode, `mode "Smash the Chest": destroy target artifact`, prints its name first ("• Smash the Chest — Destroy target artifact."); the name has no rules meaning. |
| `choose one or both if teamwork { mode: ... mode: ... }`, `choose one or both if may descended this turn { ... }`, `choose one or more if 4 card types in your graveyard { ... }`, `choose one or any number if kicked { ... }` | "Choose one. If this spell was cast using teamwork, choose both instead." (`may`: "you may choose both instead"; `one or more`: "choose one or more instead", two to four modes; `one or any number`: "choose any number instead") (CR 700.2): a modal spell whose sets of more than one mode may be chosen only while the condition holds as it is cast; without `may`, choosing both is then required. A condition on a cost announced with the modes (`kicked`, `teamwork`, `additional cost paid`, CR 601.2b) is chosen by the modes instead: the larger choice is offered while that cost can be paid and then pays it (scenarios: `kicked`/`teamwork` still name it), one mode under "choose both instead" declines it. `kicked` prints "this spell was kicked". |
| `choose one or more { mode: ... }`, `choose up to 2 { mode: ... }`, `choose one unchosen { mode: ... }`, `choose one unchosen this turn { mode: ... }` | Other modal heads (CR 700.2). `choose one or more` ("Choose one or more —", two to four modes) offers every nonempty set of the modes, numbered as bits like spree's (`mode 3` is the first two, `mode 7` the first three); `choose up to 2` ("Choose up to two —"; `choose up to 1`: "choose up to one —") the sets of at most that many in the same order, then no mode at all, last. Each as a spell or as a triggered or activated ability's effect. `choose one unchosen` ("choose one that hasn't been chosen —"), only as a triggered or activated ability's effect: each mode can be chosen once for that object's ability, by whoever controls it (a new object starts fresh, CR 400.7); a mode chosen for an ability that then does nothing still counts. `this turn` ("... that hasn't been chosen this turn —") makes the modes open again as the turn ends; `stop` after either (`choose one unchosen this turn stop {`) ends the head with a period instead of the dash ("choose one that hasn't been chosen this turn.", wording only). With no mode left, a triggered ability is removed and an activated one can't be activated. |
| `flash if pay {2} more` | "You may cast this spell as though it had flash if you pay {2} more to cast it." (CR 601.2f): the card may be cast any time its caster could cast an instant, costing {2} more whenever they couldn't cast a sorcery (outside their main phase, or with something on the stack). |
| `enters tapped unless you control planeswalker`, `enters tapped unless player life 13 or less` | "This land enters tapped unless ..." with a static condition (CR 614.1d). |
| `activated {1}: add {B}; if activated 4 times this turn, delayed this: sacrifice this at beginning of the next end step` | The condition `activated N times this turn` ("If this ability has been activated four or more times this turn", Initiates of the Ebon Hand): the source's activations of such an ability this turn, this one included, whether a player or payment activated it (the ability then records its activations, `ActivatedAbility::counted`). |
| `enters tapped if not your turn` | "This creature enters tapped if it's not your turn." (the same replacement as `enters tapped unless your turn`, CR 614.1d) |
| `enters tapped if x 2 or less` | "If X is 2 or less, this creature enters tapped." (CR 614.1d: X as it was cast, 0 if it wasn't, CR 107.3m); right after an `enters with` line it ends that paragraph: `enters with 3 minus X stun counters` then `enters tapped if x 2 or less` print "This creature enters with a number of stun counters on it equal to three minus X. If X is 2 or less, it enters tapped." Any static condition. |
| `enters with 3 minus X stun counters` | "... a number of stun counters on it equal to three minus X." `3 minus X` is a number wherever X is allowed: three less X, never below 0. |
| `enters tapped unless pay 3 life` | "As this land enters, you may pay 3 life. If you don't, it enters tapped." (CR 614.12; only with that much life, CR 119.4). |
| `enters tapped unless reveal Mountain or Forest card`, `enters tapped unless reveal Dragon card or you control Dragon` | "As this land enters, you may reveal a Mountain or Forest card from your hand. If you don't, this land enters tapped." (CR 614.12, 701.20a; the snarls and the Lorwyn tribal lands): its controller is asked only with a matching card in hand (judged as printed; the land itself doesn't count while it is played), and the engine reveals the first such card, known to every player until it moves. With `or <condition>`: "This land enters tapped unless you revealed a Dragon card this way or you control a Dragon." (Temple of the Dragon Queen). |
| `can't attack unless defending player controls Island`, `can't block unless you control other Goblin`, `can't attack or block unless ...` | Attack and block restrictions with a condition (CR 508.1c, 509.1c). Conditions only these use: `defending player poisoned` ("defending player is poisoned"), `on battlefield Mountain` / `on battlefield 5 Island` ("there is a Mountain on the battlefield", "there are five or more Islands on the battlefield"), `you control more creatures than defending player` (or `attacking player`), and, judged on the whole declaration, `at least 2 other creatures attack` (or `block`) and `black or green creature also attacks` ("a black or green creature also attacks"): a creature whose condition the declared attackers (blockers) don't meet can't be among them. `opponent damaged this turn` prints "an opponent has been dealt damage this turn" here. `can't attack if defending player controls untapped land` (`if`: the restriction applies while the condition holds: "This creature can't attack if ..."); `can't attack if creature also attacks` prints "can only attack alone" (granted: "Enchanted creature ... can only attack alone."). `attacked last turn` ("it attacked during your last turn": the source was declared as an attacker during its controller's most recent turn) also works in `static this: has doesn't untap if attacked last turn`. `defending player controls no Glimmer creature` prints "controls no Glimmer creatures". A condition's alternative written with its own article (`you control Knight or a Soldier`) prints it ("a Knight or a Soldier"). |
| `tax attacking you {2}`, `tax attacking you {1} those`, `tax attacking you or planeswalkers {1}`, `tax attacking you or planeswalkers {X} count where enchantment you control`, `tax enchanted attack {3}`, `tax enchanted attack or block {X} count each hand`, `tax enchanted blocked {3}`, `... as long as enduring story` | Attack and block taxes (CR 508.1d, 508.1g-h, 509.1d, 509.1f-g): "Creatures can't attack you unless their controller pays {2} for each creature they control that's attacking you." (`those`: "... for each of those creatures."), "Creatures can't attack you or planeswalkers you control unless their controller pays {1} for each of those creatures." (`{X} count where ...` adds ", where X is ..."), "Enchanted creature can't attack unless its controller pays {3}.", "... can't attack or block unless its controller pays {1} for each card in your hand." (`{X} count each ...`: {1} for each, counted for the tax's controller), "Enchanted creature can't be blocked unless defending player pays {3} for each creature they control that's blocking it."; `as long as <condition>` prints "As long as you have an enduring story, creatures can't attack you ...". Generic mana only. A declaration whose taxes its player can't pay isn't offered (each attacker counted at what it costs to attack the cheapest of the defending player, their planeswalkers, and battles); the taxes are paid, automatically, once the attackers are tapped (or the blockers declared). A creature that attacks each combat if able needn't when no declaration with it can be paid for. |
| `activated <cost>: <effects>` | An activated ability (CR 602). With {X} in its cost (`activated {X}, sacrifice this: destroy target artifact with mana value X`), X is announced as it is activated, before targets (CR 602.2b, 601.2b), only as high as can be paid and some target fits; the effects read it. A mana ability's X is not asked. |
| `boast {1}{W}: <effects>`, `exhaust {3}: <effects>`, `power-up {5}{R}: <effects>` | An activated ability after a keyword, printed "Boast — {1}{W}: ..." (the keyword's rules are reminder text). Boast (CR 702.142): only if this creature was declared as an attacker this turn, and only once each turn. Exhaust (CR 702.177) and power-up (CR 702.193): only once for the object (a new object after a zone change may again, CR 400.7); a power-up ability of a permanent that entered this turn costs its mana cost less (colored mana reduces that color, any excess generic; CR 118.7). |
| `loyalty +1: <effects>` | A loyalty ability (CR 606); `-6` likewise, and `loyalty 0:` (or `+0`) prints "0:". `loyalty -X: deal X damage to each creature` prints "−X: ..." (CR 107.3): X is announced as the ability is activated, from 0 to the planeswalker's loyalty, and that many loyalty counters are removed; `X` in its effects is that number. |
| `equip {1}`, `equip Knight {1}`, `equip {0}, once`, `equip {2}, pay 2 life`, `equip sacrifice creature`, `equip {3} or discard 1 card` | Equip (CR 702.6): "Equip {1}"; with a creature type, only onto a creature of that type you control ("Equip Knight {1}", CR 702.6c); `once` prints "Equip {0}. Activate only once each turn." Any activation cost other than mana alone prints after a dash: "Equip—{2}, Pay 2 life.", "Equip—Sacrifice a creature." (`, once` adds "Activate only once each turn."). `equip {3} or discard 1 card` prints "Equip—Pay {3} or discard a card.": two equip abilities, one for each cost (`activate "X" ability 2` in scenarios takes the second). |
| `triggered <trigger> [if <condition>]: <effects>` | A triggered ability (CR 603). `triggered once each turn <trigger>: ...` ("This ability triggers only once each turn."), `twice each turn`, and `do once each turn` ("Do this only once each turn.") limit it each turn (below). |
| `replace <event>: <outcome>` | A replacement or prevention effect while on the battlefield (CR 614, 615). Add `{ <effects> }` for effects that happen when it applies (Phantom Tiger); after a prevention, `that much` in them is the damage prevented this way (CR 615.5: none for damage that can't be prevented, the shield's share for a `prevent next N`) and `that creature` the damaged permanent, printed "You gain life equal to the damage prevented this way." (`you gain that much life`), "Put a +1/+1 counter on that creature for each 1 damage prevented this way." (`put that much +1/+1 counters on that creature`, Vigor). An effect a spell creates takes the block too: `prevent all damage target spell would deal this turn { you gain that much life }` (Hallow), `this turn, replace damage from chosen source to you: prevent next time { you gain that much life }` (Reverse Damage). `prevent and { ... }` joins them to the prevention, `that much` printed "that many": `replace damage to this: prevent and { put that much +1/+1 counters on this }` "If damage would be dealt to Anti-Venom, prevent that damage and put that many +1/+1 counters on him.", `replace combat damage from creature to this: prevent and { put 1 +1/+1 counters on this creature }` "If a creature would deal combat damage to this creature, prevent that damage and put a +1/+1 counter on this creature." (Ironscale Hydra). `for each { ... }` after a created prevention prints the count first: `prevent next 3 damage to target creature this turn for each { put that much +1/+1 counters on that creature }` "... For each 1 damage prevented this way, put a +1/+1 counter on that creature." (Test of Faith). `replace damage to this while this has 1 or more +1/+1 counters: prevent and { remove that much +1/+1 counters from this creature }` prints "If damage would be dealt to this creature while it has a +1/+1 counter on it, prevent that damage and remove that many +1/+1 counters from this creature." (Ugin's Conjurant): the replacement applies only while the static condition holds (static abilities only). The joined form is part of the replacement itself: it happens even where the damage can't be prevented (CR 615.12), and its `that much` is the damage the event would have dealt, not what was prevented (Protean Hydra's and Gloom Surgeon's rulings); `2 times that much` prints "twice that many" ("prevent that damage and mill twice that many cards", Angel of Suffering), `exile top that much cards` "exile that many cards from the top of your library" (Gloom Surgeon). `replace damage source would deal to you: prevent and { ... }` prints "If a source would deal damage to you, prevent that damage and ..." (Nine Lives: once for each source's damage), `replace damage would be dealt to this: prevent and { ... }` "If this creature would be dealt damage, prevent that damage and ..." (Unbreathing Horde). After `remove that much +1/+1 counters from this`, `when you do, <clause>` prints "When one or more counters are removed from this creature this way, ..." and triggers only if a counter was removed; `that much` in it is still the damage (Magma Pummeler: more damage than counters deals all of it). |
| `enters tapped` / `enters with 2 +1/+1 counters` / `enters prepared` | How this permanent enters (CR 614.1c; prepared, CR 722.3a). `enters with 2 +1/+1 counters if creature died this turn` (any static condition, judged as it enters) prints "This creature enters with two +1/+1 counters on it if a creature died this turn."; `enters with count equal graveyard creature +1/+1 counters` prints "... enters with a number of +1/+1 counters on it equal to the number of creature cards in your graveyard" (counted as it enters). Both, in that order, print as one sentence: "This land enters tapped with two depletion counters on it." (the order they apply in changes nothing, so it isn't asked). |
| `static <filter>: <modifications>` | A static ability generating a continuous effect (CR 604, 611.3). `static each creature you control with +1/+1 counter: has trample` prints the subject one at a time: "Each creature you control with a +1/+1 counter on it has trample." |
| `static as long as <condition>, <filter>: <modifications>` | A conditional static ability (CR 611.3a), printed "As long as <condition>, ...". Write the condition last (`static this: +2/+2 as long as metalcraft`) to print it last; `static this: has can't be blocked if cast historic spell this turn` prints "if" ("This creature can't be blocked if ..."). `static during your turn, this: has first strike` prints "During your turn, ..."; `static during turns other than yours, this: +0/+2` prints "During turns other than yours, ..." (it isn't the controller's turn). `otherwise <modifications>` after the condition is what applies to the same objects while it doesn't hold, printed "Otherwise, it ...": `static enchanted: +2/+2 as long as attached is Vampire otherwise -2/-2` ("Enchanted creature gets +2/+2 as long as it's a Vampire. Otherwise, it gets -2/-2."), `static equipped: has deathtouch during your turn otherwise has reach` ("Equipped creature has deathtouch during your turn. Otherwise, it has reach."; `during your turn` written last prints last), `static as long as you control attached, enchanted: +2/+2 otherwise has can't block` ("As long as you control enchanted creature, it gets +2/+2. Otherwise, it can't block."). The engine makes it a second static ability with the condition negated. `as long as this has 2 or fewer oil counters` prints "it has two or fewer oil counters on it". `static this: additional +5/+5 as long as life 10 above starting` prints "Elenda gets an additional +5/+5 as long as ..." (the same +5/+5, worded after another bonus; right after another `static` line it joins that paragraph). `static during your turn, this or other Knight you control: has flying` prints "During your turn, Dion and other Knights you control have flying." (the source or the others). |
| `label "Landfall" <ability line>` | An ability word (CR 207.2c) printed before the ability: "Landfall — Whenever a land you control enters, ...". It has no rules meaning. A flavor word (CR 207.2d) is the same: `label "Fire Cross" triggered whenever this deals combat damage to a player: ...`, `label "Intangibility" static this: has can't be blocked` ("Intangibility — Ghost can't be blocked."). |
| `can't be blocked by Wall`, `can't be blocked except by creature with flying`, `can't be blocked by more than one creature`, `can block only creature with flying`, `can't block creature with flying` | Blocking restrictions on this creature (CR 509.1b–c). Alternatives print with "and/or" ("can't be blocked except by artifact creatures and/or white creatures", `black or red creature`: "black and/or red creatures"). Granted, they print unquoted: `static enchanted: has ability { can't be blocked except by Wall }` is "Enchanted creature can't be blocked except by Walls."; `this gains ability { can't be blocked by creature with power 2 or less }` is "This creature can't be blocked by creatures with power 2 or less this turn." ("can't be blocked this turn except by ..."). `can't be blocked by creature with lesser power` prints "Creatures with power less than this creature's power can't block it."; `can't be blocked by creature with greater power` prints "This creature can't be blocked by creatures with greater power." (Prehistoric Pet: a blocker's power at most the attacker's, as blockers are declared). Alternatives in `can't be blocked by` print with "or" (`can't be blocked by creature with aura or enchantment creature`: "can't be blocked by enchanted creatures or enchantment creatures"). `can't be blocked except by 3 or more creatures` (CR 509.1b: blocked by none or by at least that many, like menace's two). Allowances (CR 509.1a): `can block shadow as though had shadow` ("can block creatures with shadow as though it had shadow"; `as though didn't have shadow`: "... as though they didn't have shadow", CR 702.28b), `can block landwalk as though didn't have` ("can block creatures with landwalk abilities as though they didn't have those abilities", CR 702.14b), `can block an additional 7 creatures each combat` (eight in all). |
| `enters as copy of creature`, `enters tapped as copy of land`, `enters tapped as copy of land card in graveyard`, `enters as copy of creature card in graveyard` | "You may have ~ enter as a copy of any creature on the battlefield." (CR 707, 614.1c): chosen as it enters, however it enters (cast, played, put onto the battlefield; `copy "X"` / `copy nothing` in scenarios). `tapped` prints "enter tapped as a copy of" and it enters tapped only if it copies something (Vesuva); `card in graveyard` prints "any land card in a graveyard": it copies a card in any graveyard instead of a permanent (Echoing Deeps; an `except` block adds to it, "except it's a Cave in addition to its other types"). |
| `as this enters choose color`, `as this enters choose color except white`, `as this enters choose creature type`, `as this enters choose land type`, `as it enters choose color` | "As this land enters, choose a color [other than white]." (CR 614.12; `land type` is "a basic land type"): the controller chooses as the permanent enters, however it enters; the permanent remembers it until it leaves (CR 400.7) for `mana chosen`, `set color chosen`, `has protection from chosen color`, `set land chosen`, `add chosen type`, and filters `of chosen color` / `of chosen type`. `as it enters` prints on the line before ("This land enters tapped. As it enters, choose a color."). `as this enters choose "Khans" or "Dragons"` ("As this enchantment enters, choose Khans or Dragons.", the Sieges): one of the named options (`choose "1"`, `"2"` in scenarios; unscripted, the first); `label "Khans" <ability>` then prints "• Khans — ..." and is an ability the permanent has only while Khans was chosen (a static, triggered, activated, or cost ability, or a player's permission such as `label "Sultai" you may play lands from your graveyard`, or `label "Mardu" trigger again permanent you control when creature attacks`; a triggered ability's own `if` still applies). `as this enters choose creature type among Elemental, Elf, Faerie, Giant, Goblin, Kithkin, Merfolk, Treefolk` ("As this artifact enters, choose Elemental, Elf, Faerie, Giant, Goblin, Kithkin, Merfolk, or Treefolk.", two to eight types): only those are offered, then read as any chosen type is; a card filter prints the chosen type after its noun ("target card of the chosen type", "all creature cards of the chosen type"). `as this enters choose odd or even` ("As Gollum enters, choose odd or even."; `choose "1"` is odd, `"2"` even in scenarios) and the effect `choose odd or even` ("Choose odd or even.", as it resolves) are read by the filter `of chosen quality`: "with mana value of the chosen quality" (`whenever an opponent casts of chosen quality spell`, `destroy each single other creature of chosen quality`; zero is even, a spell's X counts on the stack). |
| `as this enters becomes choice 3/3 creature or 2/2 creature with flying or 1/6 creature with defender`, `... or 1/6 Wall artifact creature with defender in addition` | "As this creature enters, it becomes your choice of a 3/3 creature, a 2/2 creature with flying, or a 1/6 creature with defender." (CR 614.12; "in addition to its other types"): the controller chooses one as it enters (`choose "2"` in scenarios, 1-based; unscripted, the first), and the permanent has that base power and toughness, those types and subtypes, and those keywords while it stays on the battlefield. A copy of it doesn't copy the choice (it isn't a copiable value here). |
| `spells artifact you cast cost {1} less`, `spells noncreature cost {1} more`, `spells Dragon your opponents cast cost {2} more`, `spells your opponents cast from graveyards or exile cost {2} more` | "Artifact spells you cast cost {1} less to cast." (CR 601.2f; "Spells your opponents cast from graveyards or from exile cost {2} more to cast."): the generic part only, never below zero. Types joined with `or` print "Instant and sorcery spells". `static during turns other than yours, spells you cast cost {1} less` (or `during your turn`) applies only then: "During turns other than yours, spells you cast cost {1} less to cast." A mana value prints after who casts them: `spells creature with mana value 6 or greater you cast cost {2} less` is "Creature spells you cast with mana value 6 or greater cost {2} less to cast." `static as long as spell number 2 this turn, spells you cast cost {1} less` is "The second spell you cast each turn costs {1} less to cast." (the spell would be your second this turn: you've cast exactly one before it). A keyword quality prints after "spells": `spells creature with flying you cast cost {1} less` is "Creature spells with flying you cast cost {1} less to cast." `... cost {1} less count each creature you control with power 4 or greater` is "... cost {1} less to cast for each creature you control with power 4 or greater." (counted for the source's controller as the cost is determined). `spells noncreature you cast cost {1} less count where speed` is "Noncreature spells you cast cost {X} less to cast, where X is your speed." `spells first creature you cast cost {2} less` is "The first creature spell you cast each turn costs {2} less to cast.": only while its caster hasn't cast a matching spell this turn (the spell being cast isn't one yet); `and flash` adds "and can be cast as though it had flash" (CR 702.8); `static during your turn, spells first non-Lemur creature with flying you cast cost {1} less` prints "The first non-Lemur creature spell with flying you cast during each of your turns costs {1} less to cast." |
| `static each instant or sorcery card in your graveyard: has flashback`, `static during your turn, each Lesson card in your graveyard: has flashback {1}`, `static each nonland card in your graveyard: has mayhem`, `static artifact card or red creature card in your hand: has warp {2}{R}`, `static each Sliver card in each player's hand: has slivercycling {3}`, `static each creature card in your graveyard: has unearth {2}{B}` | Cards in a hand or graveyard given a way to be cast or an ability (CR 113.6b): "Each instant and sorcery card in your graveyard has flashback. The flashback cost is equal to that card's mana cost.", "During your turn, each Lesson card in your graveyard has flashback {1}.", "Each nonland card in your graveyard has mayhem. The mayhem cost is equal to its mana cost.", "Artifact cards and red creature cards in your hand have warp {2}{R}.", "Each Sliver card in each player's hand has slivercycling {3}.". The cards are those the source's controller owns (`in each player's hand`: anyone's) matching the filter (`<filter> card or <filter> card`: either, printed "X cards and Y cards"; types joined with `or` print "instant and sorcery"), while the source is on the battlefield and the condition before them (`during your turn,`, `as long as <condition>,`) holds. `each` prints "Each ... card ... has" (without it, "... cards ... have"). In a graveyard: `flashback` (for the card's mana cost, or the cost written), `mayhem` (cast if its owner discarded it this turn, for its mana cost or the cost written), `retrace` (`static during your turn, nonland permanent card in your graveyard: has retrace`, "During your turn, nonland permanent cards in your graveyard have retrace.": cast for its mana cost and a land card discarded, with its usual timing; it isn't exiled, CR 702.81a), and `jump start` (`static each two-colored instant or sorcery card in your graveyard: has jump start`, "Each instant and sorcery card in your graveyard that's exactly two colors has jump-start.": its mana cost and any card discarded; exiled as it leaves the stack, CR 702.133a; with jump-start and retrace both, jump-start is used, and a granted flashback before either), `cast` (`static artifact card in your graveyard: has cast by pay 3 life finality`, Noctis: "You may cast artifact spells from your graveyard by paying 3 life in addition to paying their other costs. If you cast a spell this way, that artifact enters with a finality counter on it."; `static during your turn, instant or sorcery card in your graveyard: has cast by pay 1 life brief`, Festival of Embers: "During your turn, you may cast instant and sorcery spells from your graveyard by paying 1 life in addition to their other costs."; `by forage` "by foraging"; `brief` leaves out "paying"; `finality` names the filter's one card type, else "permanent": CR 601.3, the controller's matching cards may be cast from their graveyard for their costs and the rider's (life, foraging: the engine picks, as for forage as a cost), with their usual timing, and aren't exiled; a permanent cast this way enters with a finality counter, CR 122.1h; a card's own way from the graveyard, or a granted flashback, jump-start, retrace or mayhem, is used first; of several such permissions the one with the fewest riders) and `play` (`static during your turn, card in your graveyard: has play`, "During your turn, you may play cards from your graveyard.": land cards too, with a land play, CR 305.1), and `sneak <cost>` (`static creature card in your graveyard: has sneak {3}{B}`, Ninja Teen: "Creature cards in your graveyard have sneak {3}{B}." and the next sentence, "You may cast creature spells from your graveyard using their sneak abilities.": cast from the graveyard for that cost during its controller's declare blockers step, returning an unblocked attacker, as with the keyword; a card with sneak of its own uses its own cost); in a hand: `warp <cost>`, `foretell` (`static each nonland card in your hand: has foretell`, Dream Devourer: "Each nonland card in your hand without foretell has foretell. Its foretell cost is equal to its mana cost reduced by {2}.": a card without foretell of its own may be foretold for {2} during its owner's turn, and cast from exile on a later turn for its mana cost less {2} generic, the Devourer gone or not, CR 702.143a), and `miracle <cost>` (`static each instant or sorcery card in your hand: has miracle {2}`: the card is revealed as it is drawn if it is the first card its owner drew this turn, and may then be cast for that cost while the grant still applies); in either, an activated keyword ability of the cards' zone (cycling, typecycling, ninjutsu from a hand; unearth from a graveyard), activated as the card's own. A card with that way of its own uses its own; a spell cast this way is cast with flashback (exiled), for mayhem, or warped, as with the keyword. |
| `spells you cast have delve`, `spells instant or sorcery you cast have storm`, `spells instant or sorcery you cast from hand have cascade`, `spells you cast have affinity for artifact`, `spells creature you cast have convoke`, `spells noncreature you cast have improvise` | "Spells you cast have delve.", "Instant and sorcery spells you cast have storm.", "... you cast from your hand have cascade.", "Spells you cast have affinity for artifacts.": the controller's matching spells have the keyword while they are cast (convoke, improvise, delve, cascade, storm; affinity for <filter> is {1} less for each such permanent the caster controls). Several join with `and`. `spells noncreature you cast have flash` prints "You may cast noncreature spells as though they had flash." (`spells you cast have flash`: "You may cast spells as though they had flash."; `spells sorcery or Dragon you cast have flash`: "... sorcery spells and Dragon spells ..."): the controller may cast matching spells any time they could cast an instant (CR 702.8). `spells you control can't be countered`, `spells instant or sorcery you control can't be countered` ("Instant and sorcery spells you control can't be countered."): an effect that counters such a spell of the controller's does nothing to it (CR 701.5); `spells can't be countered` ("Spells can't be countered.", Lier) is every player's. After `you cast`, `from hand`, `from graveyard`, `from exile`, and `from other than hand` limit the spells to those cast from there ("Noncreature spells you cast from exile have convoke.", "... from anywhere other than your hand ..."; judged where the card is as it is offered, and on the stack by the zone it was cast from); they apply to a cost change too: `spells you cast from graveyard cost {1} less` is "Spells you cast from your graveyard cost {1} less to cast." `spells planeswalker you cast have spend any color` prints "You may spend mana as though it were mana of any color to cast planeswalker spells." (Oath of Nissa; `spend any type can`: "You can spend mana of any type to cast creature spells."): the controller pays each colored and hybrid symbol of such a spell's total cost with any mana (`any type`: {C} too; CR 609.4b). `spells you cast have spend any color and abilities` prints "You may spend mana as though it were mana of any color." (Chromatic Orrery): also the mana costs of the activated abilities of the controller's permanents (not other costs, such as ward's). `spells each noncreature you cast have conspire` ("Each noncreature spell you cast has conspire.", CR 702.78) and `spells each instant or sorcery you cast have casualty 1` ("Each instant and sorcery spell you cast has casualty 1.", CR 702.153) give the cast's optional cost and its copy (the greatest casualty given counts; without `each`, "... spells you cast have conspire."). `spells instant or sorcery you control have lifelink` ("Instant and sorcery spells you control have lifelink.") gives the controller's matching spells on the stack lifelink, deathtouch, wither, or split second (CR 702.15, 702.2, 702.80, 702.61: damage they deal, and responses to them). An emblem's (`emblem "..." { spells instant or sorcery you cast have storm }`) works like a permanent's (CR 114.4). |
| `this spell costs {1} less count each artifact you control` | "This spell costs {1} less to cast for each artifact you control." |
| `affinity for artifact` | "Affinity for artifacts" (CR 702.41). |
| `protection from artifact`, `protection from Goblin`, `protection from multicolored`, `protection from everything`, `protection from creature your opponents control` | Protection from a quality other than a color (CR 702.16); colors are keywords. A controller in the quality is judged from the protected object's controller ("protection from creatures your opponents control": its own creatures still may block and target it). Consecutive protections print as one line, "Protection from Spirits and from Arcane" (the gate splits such a line into one protection each, CR 702.16g); in a list of granted keywords, "has protection from green and from blue". |
| `landwalk snow Forest`, `landwalk snow land`, `landwalk legendary land`, `landwalk nonbasic land` | "Snow forestwalk", "Snow landwalk", "Legendary landwalk", "Nonbasic landwalk" (CR 702.14a): can't be blocked as long as the defending player controls a land the filter matches; "can block creatures with landwalk abilities as though they didn't have those abilities" stops it too. |
| `fabricate 1`, `renown 1`, `evolve`, `unleash` | Fabricate (CR 702.123): when it enters, you may put N +1/+1 counters on it; if you don't (or it has left the battlefield), you create N "1/1 colorless Servo artifact creature" tokens. Renown (CR 702.112): when it deals combat damage to a player, if it isn't renowned, N +1/+1 counters and it becomes renowned (a designation that lasts until it leaves the battlefield). Evolve (CR 702.100): whenever a creature you control enters, if that creature has greater power or greater toughness (judged as it triggers and as it resolves, the entering creature by last known information if it has left), a +1/+1 counter on this. Unleash (CR 702.98): you may have it enter with a +1/+1 counter; it can't block as long as it has a +1/+1 counter. |
| `hideaway 4`, `play free exiled card`, `cast free exiled card`, `put exiled card into hand` | Hideaway N (CR 702.75): "When this permanent enters, look at the top N cards of your library, exile one face down, then put the rest on the bottom in a random order." (the controller chooses, scenarios `choose "Hill Giant"`; the exiled card is linked to the permanent, CR 607.2a, and only its controller sees it: other players' views show a face-down card). Prints `Hideaway 4`. `play free exiled card` is "you may play the exiled card without paying its mana cost": a spell is cast as the ability resolves (timing ignored, CR 608.2g); a land is played, counting as the land play, only on the controller's own turn with a land play left (CR 305.1); `cast free exiled card` casts only. A trailing `if <condition>` prints "... if you control four or more legendary creatures" and is judged as the ability resolves. `put exiled card into hand` is "put the exiled card into its owner's hand" (Watcher for Tomorrow's leaves trigger). |
| `storied` | Storied (CR 702.195): any time its controller controls three or more permanents that are artifacts, Sagas, and/or legendary, they get an enduring story for the rest of the game (checked with the state-based actions). Prints `Storied`. The static conditions `enduring story` ("you have an enduring story") and `no enduring story` ("you don't have an enduring story"; `static this: has doesn't untap if no enduring story` prints "... doesn't untap during your untap step unless you have an enduring story."). |
| `increment` | Increment (CR 702.191): whenever you cast a spell, if this is a creature and the amount of mana spent to cast that spell (X included) is greater than its power or its toughness, a +1/+1 counter on it; judged as it triggers and as it resolves. Prints `Increment`. |
| `afterlife 1`, `extort`, `mentor` | Afterlife (CR 702.135): when it dies, N "1/1 white and black Spirit creature with flying" tokens. Extort (CR 702.101): whenever you cast a spell, you may pay {W/B}; if you do, each opponent loses 1 life and you gain the life lost this way. Mentor (CR 702.134): whenever it attacks, a +1/+1 counter on target attacking creature `with lesser power` (a filter word: power less than the source's, as it is or last existed); `has mentor` grants it. |
| `mobilize 2`, `mobilize count where graveyard creature` | Mobilize (CR 702.181): whenever it attacks, N "1/1 red Warrior creature" tokens enter tapped and attacking what it attacks (never declared as attackers, CR 508.4), sacrificed at the beginning of the next end step. Prints "Mobilize 2", or "Mobilize X, where X is the number of creature cards in your graveyard." with a count. |
| `devour 2`, `devour artifact 1`, `devour land 3`, `creature it devoured` | Devour (CR 702.82): as it enters, its controller sacrifices any number of other creatures (artifacts, lands) they control, chosen one at a time (scenarios: `choose "Grizzly Bears"`, `choose done`; unscripted, none), and it enters with N +1/+1 counters for each. Prints "Devour 2", "Devour artifact 1". The count `creature it devoured` (`Goblin it devoured`) is the permanents it devoured that match, as they last existed: "for each creature it devoured", "equal to twice the number of Goblins it devoured" (`count equal 2 Goblin it devoured`). |
| `haunt`, `triggered when this enters or haunted dies: ...`, `triggered when haunted dies: ...` | Haunt (CR 702.55): on a permanent, "When this creature dies, exile it haunting target creature."; on an instant or sorcery, the same as the resolved spell is put into its owner's graveyard ("When this spell card is put into a graveyard after resolving"). Exiled so, the card haunts that creature, and its `when haunted dies` ability ("When the creature this card haunts dies, ..."; with `this enters or`, "When this creature enters or the creature it haunts dies, ...", also the creature's own enters trigger) triggers from exile when that creature dies, controlled by the card's owner, as long as the card is still in exile; the creature leaving the battlefield any other way ends it. Prints "Haunt". |
| `reconfigure {2}`, `triggered whenever this or equipped creature attacks: it gets +2/+0` | Reconfigure (CR 702.151): two activated abilities, each only as a sorcery: "{2}: Attach this permanent to another target creature you control." (scenarios: `activate "Lizard Blades" ability 1 targeting ...`) and "{2}: Unattach this permanent." (`ability 2`; only while attached to a creature); attached to a creature, it isn't a creature (layer 4; an Equipment creature with reconfigure may be attached, CR 301.5c). Prints "Reconfigure {2}". The trigger subject `this or equipped creature` is "this creature or equipped creature": the source, or the creature it's attached to. |
| `cleave {1}{U}: return target nonland permanent to hand`, then `spell: return target nonland permanent you control to hand` | Cleave (CR 702.148): "Cleave {1}{U}", the card's alternative cost (`alternative` in scenarios); cast for it, the spell is the `cleave` line's effects (and targets) instead of the `spell` line's. The `spell` line prints with the words the cleaved text lacks in square brackets ("Return target nonland permanent [you control] to its owner's hand."); the cleaved text must be the spell's with words removed, or the brackets aren't printed. Paragraphs: `cleave {3}{U} { ... }` like a `spell { ... }` block. |
| `champion Goblin`, `champion creature`, `champion Goblin or Shaman` | Champion (CR 702.72): "When this enters, sacrifice it unless you exile another Goblin you control." (the controller chooses one, scenarios `choose "Goblin Piker"`, or `choose done` to sacrifice it; unscripted, the first) and "When this leaves the battlefield, return the exiled card to the battlefield under its owner's control." (the exile is linked to it). Prints "Champion a Goblin". |
| `read ahead` | Read ahead (CR 702.155), on a Saga: as it enters, its controller chooses a number from one to its final chapter (scenarios: `choose "3"`; unscripted, one) and it enters with that many lore counters; only the chosen chapter triggers as it enters. Prints "Read ahead". |
| `amplify 1` | Amplify (CR 702.38): as it enters, its controller reveals any number of other cards from their hand that share a creature type with it (scenarios: `choose "X"`, `choose done`; unscripted, none) and it enters with N +1/+1 counters for each. Prints "Amplify 1". |
| `tribute 2`, `if tribute not paid` | Tribute (CR 702.104): as it enters, its controller chooses an opponent, who may put N +1/+1 counters on it (scenarios: that opponent's `accept "Pharagax Giant"` / `decline`; unscripted, accepted). The trigger condition `triggered when this enters if tribute not paid: ...` prints "When this creature enters, if tribute wasn't paid, ...". Prints "Tribute 2". |
| `riot`, `living weapon`, `job select`, `activated {5}{R}: monstrosity 3`, `static as long as monstrous, this: has flying` | Riot (CR 702.136): as it enters, its controller may have it enter with an additional +1/+1 counter; if they don't, it has haste for as long as it is that object (scenarios: `accept`/`decline`). Living weapon (CR 702.92): when this Equipment enters, a "0/0 black Phyrexian Germ creature" token, then attach this to it. Job select (CR 702.182) likewise, with a "1/1 colorless Hero creature" token. Monstrosity N (CR 701.37), an effect, `X` allowed: if the source isn't monstrous, N +1/+1 counters on it and it becomes monstrous (a designation kept until it leaves the battlefield); `monstrous` is the condition "this creature is monstrous". |
| `ingest`, `reinforce 1 {1}{R}` | Ingest (CR 702.115): whenever it deals combat damage to a player, that player exiles the top card of their library. Reinforce N—cost (CR 702.77): "{cost}, Discard this card: Put N +1/+1 counters on target creature." (activated from the hand). |
| `retrace`, `jump start`, `surge {1}{R}`, `spectacle {R}` | Retrace (CR 702.81): the card may be cast from your graveyard for its mana cost and a land card discarded (scenarios: `from graveyard discarding "Forest"`); it isn't exiled. Jump-start (CR 702.133): the same with any card discarded, exiled as it leaves the stack (flashback's casting, for the mana cost and a discard). Surge (CR 702.117) and spectacle (CR 702.137): the card's alternative cost (`alternative` in scenarios), offered if you've cast another spell this turn, or if an opponent lost life this turn. Printed "Retrace", "Jump-start", "Surge {1}{R}", "Spectacle {R}" where written. `freerunning {1}{B}` (CR 702.173) likewise, offered once a creature that was an Assassin you controlled dealt combat damage to a player this turn (the game has no commanders); prints "Freerunning {1}{B}". |
| `cast from graveyard while you control Zombie`, `cast from graveyard if gained life this turn`, `cast from graveyard for {2}{W}`, `cast from graveyard with pay 3 life, discard 1 card`, `cast from graveyard at max speed` | "You may cast this card from your graveyard as long as you control a Zombie.", "... if you gained life this turn.", "... by paying {2}{W} rather than paying its mana cost.", "... by paying 3 life and discarding a card in addition to paying its other costs." (life, discards, and a sacrifice: "sacrificing an artifact or creature"), "Max speed — You may cast this card from your graveyard." (CR 601.3): the card may be cast from its owner's graveyard with its usual timing, while the condition holds, for its mana cost and the added parts or for that mana instead; it isn't exiled as it leaves the stack (scenarios: `from graveyard`, `discarding`, `sacrificing`). Printed after the other abilities. |
| `counts as named "Flame Burst" in graveyard` | "If this card is in a graveyard, effects from spells named Flame Burst count it as a card named Flame Burst.": a spell of that name counting the cards named ~ in graveyards counts this card too. Printed after the other abilities. |
| `mayhem {1}{R}` | Mayhem: a card its owner discarded this turn may be cast from their graveyard for this cost (scenarios: `from graveyard`), timing rules still applying; the discarded card is remembered as the graveyard object it became, until the turn ends. Prints `Mayhem {1}{R}` in place. The land form ("You may play this card from your graveyard ...") is not supported. |
| `poisonous 1`, `frenzy 1`, `absorb 1`, `provoke` | Poisonous (CR 702.70): combat damage to a player gives them N poison counters. Frenzy (CR 702.68): attacks and isn't blocked, +N/+0 until end of turn. Absorb (CR 702.64): prevent N of each damage dealt to it (a static `prevent next N` prevents N of every event, not a shield). Provoke (CR 702.39): attacks, you may have target creature defending player controls untap and block it if able. All four, and `outlast {2}`, may be granted bare: `static all Sliver creature: has poisonous 1` ("All Sliver creatures have poisonous 1."). |
| `modular 2`, `cumulative upkeep {1}`, `exert: this gets +2/+2` | Modular (CR 702.43): enters with N +1/+1 counters; as it dies, they may move to target artifact creature. Cumulative upkeep (CR 702.24): each upkeep an age counter, then pay the cost per counter or sacrifice it. Exert (CR 701.43): as it attacks, its controller may exert it (it doesn't untap during their next untap step) for the effects. With targets (`exert: target creature can't block`, "When you do, target creature can't block this turn."), the effects are a reflexive trigger (CR 603.12): the targets are chosen once the creature is exerted, and with none legal nothing happens. The keyword `delve` (CR 702.66) lets graveyard cards pay {1} each. |
| `graft 2`, `vanishing 3` | Graft (CR 702.58): enters with N +1/+1 counters; whenever another creature enters, if it has a +1/+1 counter, its controller may move one onto that creature. Vanishing (CR 702.63): enters with N time counters; at the beginning of its controller's upkeep, if it has a time counter, one is removed, and as the last is removed this way it is sacrificed (part of the same trigger here). `vanishing` without a number (CR 702.63b) has no entering counters. Print `Graft 2`, `Vanishing 3`, `Vanishing`. |
| `keywords split second` | Split second (CR 702.61): as long as the spell is on the stack, no player is offered casting a spell or activating an ability other than a mana ability; special actions (turning a face-down creature face up, unlocking a door) remain. |
| `fading 3`, `annihilator 2`, `rampage 2`, `battle cry`, `training`, `melee`, `dethrone`, `myriad`, `for mirrodin` | Fading (CR 702.32): enters with N fade counters; at the beginning of its controller's upkeep a fade counter is removed, or, with none to remove, it is sacrificed. Annihilator (CR 702.86): whenever it attacks, defending player sacrifices N permanents of their choice. Rampage (CR 702.23): whenever it becomes blocked, +N/+N until end of turn for each creature blocking it beyond the first (counted as the trigger resolves). Battle cry (CR 702.91): whenever it attacks, each other attacking creature gets +1/+0 until end of turn. Training (CR 702.149): whenever it attacks along with another attacking creature its controller controls with greater power (judged as attackers are declared), a +1/+1 counter on it. Melee (CR 702.121): whenever it attacks, +1/+1 until end of turn if a creature attacked the opponent this combat (the game's one opponent; attacking only a planeswalker doesn't count). Dethrone (CR 702.105): whenever it attacks a player with the most life or tied for the most, a +1/+1 counter on it (not when it attacks a planeswalker). Myriad (CR 702.116): its copies attack the opponents other than the defending player, of whom a two-player game has none, so it does nothing. For Mirrodin! (CR 702.163): when this Equipment enters, a 2/2 red Rebel creature token, then attach this to it. Printed "Fading 3", "Annihilator 2", "Rampage 2", "Battle cry", "Training", "Melee", "Dethrone", "Myriad", "For Mirrodin!". |
| `label "Forecast" activated {2}{W}, reveal this: tap target untapped creature` | Forecast (CR 702.57): "Forecast — {2}{W}, Reveal this card from your hand: Tap target untapped creature." The cost `reveal this` ("Reveal this card from your hand"; "this creature" on a creature card) makes the ability one activated from the hand, the card staying there, known to every player. Labeled "Forecast", it is activated only during its controller's upkeep and only once each turn (the reminder text). |
| `blitz {3}{G}`, `blitz {2}{B}{B}, pay 2 life` | Blitz (CR 702.152): "Blitz {3}{G}", "Blitz—{2}{B}{B}, Pay 2 life.". The card's alternative cost (`alternative` in scenarios); cast for it, the permanent has haste and "When this creature dies, draw a card." (a dies trigger of the card's that triggers only if its blitz cost was paid, `if blitzed`), and is sacrificed at the beginning of the next end step (a delayed trigger, `Builtin::BlitzSacrifice`). A card with blitz has no other alternative cost of its own. |
| `escalate {1}{W}` | Escalate (CR 702.120): "Escalate {1}{W}", on a `choose one or both` spell: choosing both modes costs the escalate cost on top (entwine's cost on the both-mode). On a `choose one or more` spell (Blessed Alliance), the escalate cost is paid once for each mode chosen beyond the first (CR 702.120a). |
| `prowl {2}{U}`, `prowl paid` | Prowl (CR 702.76): "Prowl {2}{U}", the card's alternative cost (`alternative` in scenarios), offered if its caster dealt combat damage to a player this turn with a creature of one of the card's creature types (a Kindred card's too). Condition `prowl paid`: "if its prowl cost was paid" (a permanent), "if this spell's prowl cost was paid" (an instant or sorcery). |
| `miracle {R}` | Miracle (CR 702.94): "Miracle {R}". The first card a player draws in a turn, if it has miracle, is revealed as it's drawn (always: revealing costs nothing) and its trigger (`Builtin::Miracle`) lets its owner cast it from the hand for this cost as the trigger resolves, whatever its type. In scenarios the cast is always taken; script it after `wait` in that step to give its targets (`turn 3 draw A: wait`, `turn 3 draw A: cast "Thunderous Wrath" targeting player B`). |
| `emerge {6}{B}` | Emerge (CR 702.119): "Emerge {6}{B}", the card's alternative cost (`alternative ... sacrificing "Hill Giant"` in scenarios): a creature is sacrificed and the cost is reduced by its mana value (generic mana only); offered when the best creature to sacrifice makes it payable. A card with emerge has no other alternative cost of its own. "Emerge from artifact" isn't supported. |
| `recover {2}{B}` | Recover (CR 702.59): "Recover {2}{B}", a triggered ability of the card in its owner's graveyard: "When a creature is put into your graveyard from the battlefield, you may pay {2}{B}. If you do, return this card from your graveyard to your hand. Otherwise, exile this card." (`accept`/`decline` in scenarios; unpayable, it is exiled). |
| `prototype {2}{R} 3/2` | Prototype (CR 702.160, 718): "Prototype {2}{R} — 3/2". The card's alternative cost (`alternative` in scenarios): cast so, the spell and the permanent it becomes have that mana cost (so that mana value), power, and toughness, and the cost's colors (CR 718.3b), the rest of the card unchanged (CR 718.5); copies of it are too (CR 718.3c-d). Off the stack and battlefield it is its card again (CR 718.4). A card with prototype has no other alternative cost of its own. |
| `sunburst` | Sunburst (CR 702.44): enters with a +1/+1 counter (a creature card) or a charge counter (any other card) for each color of mana spent to cast it (`colors spent`); the printed card decides which, ignoring type-changing effects. Prints `Sunburst`. |
| `soulbond`, `static paired: has flying` | Soulbond (CR 702.95): as this creature enters, its controller may pair it with another unpaired creature they control; as another creature they control enters, they may pair it with this one if both are unpaired. A pair breaks when either leaves, stops being a creature, or changes controller (702.95e). The filter `paired` (`paired named`) is this creature and the one it's paired with, while paired: "As long as this creature (~) is paired with another creature, both creatures have flying." (keywords and protection: "both creatures have protection from Zombies") or "... each of those creatures gets +1/+1." (and "each of those creatures has" for a quoted ability). Prints `Soulbond`. |
| `daybound`, `nightbound` | Daybound (CR 702.145b), on a transforming card's front face, and nightbound (CR 702.145e), on its back face: with either on the battlefield it becomes day (daybound) or night (nightbound) if it's neither; at night a daybound permanent transforms, and by day a nightbound one; at night a daybound card enters transformed; no other effect transforms them. Print `Daybound`, `Nightbound`. |
| `disturb {1}{U}`, `exiled instead of graveyard` | Disturb (CR 702.146), on a transforming card's front face: the card may be cast transformed from its owner's graveyard for this cost, the spell being the back face, which enters as the permanent (CR 702.146b). The back face's "If [name] would be put into a graveyard from anywhere, exile it instead." is `exiled instead of graveyard` (from the stack too). Print `Disturb {1}{U}` and that sentence. |
| `transmute {1}{B}{B}` | Transmute (CR 702.53): an activated ability of the card in hand, "{1}{B}{B}, Discard this card: Search your library for a card with the same mana value as the discarded card (the printed card's), reveal it, put it into your hand, then shuffle. Activate only as a sorcery." Prints `Transmute {1}{B}{B}`. |
| `entwine {2}`, `escape {3}{B} exile 3`, `escapes with 2 +1/+1 counters` | Entwine (CR 702.42): a `choose one` spell may choose every mode for this cost on top. Escape (CR 702.138): cast from the graveyard for this cost, exiling N other cards from it (the oldest); a permanent cast this way keeps that, for `escapes with`. The keyword `exploit` (CR 702.110) sacrifices a creature as it enters, if its controller wants, and `triggered when this exploits a creature:` abilities follow. `discover 4` (CR 701.57) is an effect; `discover count where mana value of that target 1` prints "Discover X, where X is that spell's mana value." The trigger `whenever you discover` has the value discovered as its amount: `discover again` prints "discover again for the same value". |
| `enters with 1 +1/+1 counter if attacked this turn`, `if spent 3 white, enters with 1 +1/+1 counter` | "This creature enters with a +1/+1 counter on it if you attacked this turn." (raid; any static condition: `opponent lost life this turn`, `permanent left this turn` (revolt: "a permanent left the battlefield under your control this turn"), `you control creature with power 4 or greater` (ferocious), `you control modified creature`), and with the condition first, "If at least three white mana was spent to cast this spell, this creature enters with a +1/+1 counter on it." (adamant); `if you control 5 untapped land, enters with 2 +1/+1 counters and 1 lifelink counter` "If you control five or more untapped lands, this creature enters with two +1/+1 counters and a lifelink counter on it." (two kinds). Judged as the permanent enters (CR 614.1c). |
| `enters with count each other creature you control +1/+1 counters`, `enters with count each colors spent +1/+1 counters`, `enters with count equal all graveyards creature +1/+1 counters` | "This creature enters with a +1/+1 counter on it for each other creature you control." / "... for each color of mana spent to cast it." (converge) / "This creature enters with a number of +1/+1 counters on it equal to the number of creature cards in all graveyards.": the count judged as it enters. |
| `enters with choice flying counter or first strike counter`, `escapes with choice +1/+1 counter or flying counter` | "This creature enters with your choice of a flying counter or a first strike counter on it." (CR 614.1c, 122.1b): its controller chooses one as it enters; "escapes with ..." only when cast with escape. |
| `kicked with 2 +1/+1 counters`, `kicked with 2 +1/+1 counters and first strike`, `kicked with 2 +1/+1 counters and 1 trample counter and haste` | "If this creature was kicked, it enters with two +1/+1 counters [and a trample counter] on it [and with first strike]." (CR 702.33c): the counters as it enters, and the keywords for as long as it stays, only if it was cast with its kicker paid. A permanent enters with counters of up to two kinds. |
| `madness {1}{R}`, `foretell {1}{U}`, `suspend 4 {1}{U}` | Madness (CR 702.35): discarded into exile, then may be cast for this cost (else to the graveyard). Foretell (CR 702.143): on its owner's turn, exiled from hand for {2}; cast on a later turn for this cost. Suspend N (CR 702.62): exiled from hand for this cost with N time counters; one comes off each upkeep, and as the last does it may be cast free (a creature has haste). The keywords `cascade` and `rebound` need no cost. `keywords cipher` (CR 702.99, printed "Cipher" after the spell's text): as the spell resolves, its controller may exile it encoded on a creature they control (`decline "X"` in scenarios; `choose "Y"` picks the creature); whenever that creature deals combat damage to a player, its controller may cast a copy of the card free. The encoding ends when the creature or the card moves. `keywords paradigm` (CR 702.192, printed "Paradigm" after the spell's text): the spell is exiled as it resolves; the first time a spell its controller controls with that name resolves, at the beginning of each of their precombat main phases for the rest of the game a copy is made in exile, which they may cast free (scenarios: `turn 3 precombat_main A: cast "X" targeting ...`; it is always cast). |
| `evoke {1}{U}{U}`, `bloodthirst 2` | Evoke (CR 702.74): cast for this cost instead, and the permanent is sacrificed as it enters (`if evoked` is the trigger condition). Bloodthirst (CR 702.54): enters with N +1/+1 counters if an opponent was dealt damage this turn (`opponent damaged this turn` is also a static condition). |
| `crew 3` | Crew (CR 702.122): "Tap any number of other untapped creatures you control with total power 3 or more: this Vehicle becomes an artifact creature until end of turn." The engine taps the weakest creature strong enough alone, else the strongest until the power suffices. `crew 1, once` prints "Crew 1. Activate only once each turn." The filter words `crewed it` ("creature that crewed it this turn") and `saddled it` ("creatures that saddled it this turn") hold the creatures tapped to pay the source's crew or saddle cost this turn, even once the source has left the battlefield; as a count (`count each creature crewed it`, "for each creature that crewed it this turn") those that have left the battlefield since count too. |
| `morph {2}{U}`, `megamorph {G}`, `disguise {1}{W}` | Cast face down as a nameless colorless 2/2 creature for {3}, turned face up any time its controller has priority for the cost (a special action; CR 702.37, 708); megamorph adds a +1/+1 counter as it turns up, and a disguised permanent has ward {2} while face down (CR 702.168). A land card with disguise (Branch of Vitu-Ghazi) may be cast face down too. `triggered when this is turned face up: ...`. `morph pay 5 life`, `morph discard 1 card`, `morph return Bird you control` print "Morph—Pay 5 life." and kin (CR 702.37a: any cost): turning it face up pays the life, or asks the player which card to discard or which permanent to return. `manifest` is an effect: the top card of your library onto the battlefield face down (CR 701.34); a manifested creature card turns face up for its mana cost. `manifest dread` (CR 701.62): look at the top two cards of your library, manifest the one you choose, and put the other into your graveyard. |
| `bushido 2`, `soulshift 3`, `afflict 1`, `toxic 1`, `ward {2}`, `echo {2}{R}`, `unearth {B}`, `buyback {3}`, `outlast {1}{W}` | Keyword abilities with a number or cost (CR 702.45, .46, .130, .164, .21, .30, .84, .27, .107), printed as the keyword ("Bushido 2") in place and compiled to what they stand for: a blocks-or-blocked pump; a dies trigger that may return a Spirit card with that mana value or less; a becomes-blocked life loss; poison counters with combat damage to a player; a trigger countering an opponent's spell or ability that targets it unless they pay; an upkeep sacrifice unless paid, once after it comes under your control; an ability activated from the graveyard as a sorcery that returns it with haste and exiles it at the next end step, and exiles it instead if it would leave the battlefield any other way (CR 702.84a: it doesn't die; `leaving exiles it` alone prints "If it would leave the battlefield, exile it instead of putting it anywhere else."); an optional additional cost that returns the spell to its owner's hand; and "{cost}, {T}: Put a +1/+1 counter on this creature. Activate only as a sorcery." `flanking` is a plain keyword. `ward pay 3 life` prints "Ward—Pay 3 life." (CR 702.21a: counter it unless that player pays 3 life); `ward {2}, pay 2 life` prints "Ward—{2}, Pay 2 life.": the player pays both, the mana and the life, or it is countered; `buyback sacrifice land` prints "Buyback—Sacrifice a land." (CR 702.27a: the land is chosen as buyback is decided and sacrificed as the costs are paid). `ward discard 1 card` and `echo discard 1 card` print "Ward—Discard a card." and "Echo—Discard a card.": the payer discards a card of their choice to pay (none in hand, it can't be paid); `ward discard 1 card at random` ("Ward—Discard a card at random.") discards one at random (CR 701.9b). `ward waterbend {4}` prints "Ward—Waterbend {4}.": the payer may tap their untapped artifacts and creatures for the generic mana (CR 701.67a). `ward sacrifice creature` prints "Ward—Sacrifice a creature." (any filter: `ward sacrifice Food`, "Ward—Sacrifice a Food."): the player who controls the spell or ability may sacrifice one matching permanent of their choice as the ward trigger resolves; with none, or if they don't, it is countered. Any other ward cost is paid in full or not at all by that player as the trigger resolves (CR 702.21a, 118.3): `ward sacrifice 3 permanent` ("Ward—Sacrifice three permanents."; `ward sacrifice 3 nonland permanent`: each permanent chosen by the payer), `ward collect evidence 4` ("Ward—Collect evidence 4."; the engine picks the cards, as for the cost elsewhere), `ward blight 2`, `ward discard enchantment or instant or sorcery card` ("Ward—Discard an enchantment, instant, or sorcery card."), `ward pay count equal power life` ("Ward—Pay life equal to this creature's power.", "... to Raubahn's power." on a legend: its power as the trigger resolves, or as it last existed), `ward get 5 poison counters` ("Ward—Get five poison counters."; `ward you get 2 poison counters`: "Ward—You get two poison counters."), and alternatives joined with `or`: `ward discard 1 card or {2}` ("Ward—Discard a card or pay {2}."): the player is asked whether to pay, then (with several they can pay) which. A ward cost with another part (tapping or exiling a permanent, behold, ...) is an error, as is an echo cost other than mana, life, and discarded cards. |
| `firebending 2`, `firebending count where power`, `firebending count where creature you control` | Firebending (CR 702.189): "Whenever this creature attacks, add N {R}. Until end of combat, you don't lose this mana as steps and phases end." The red mana stays in the pool as each combat step ends, through the end of combat step (the player's other mana is taken to be spent first); X is counted as the trigger resolves. Prints "Firebending 2" (in a keyword line with others: "Trample, firebending 4, haste") or "Firebending X, where X is this creature's power." |
| `strive {2}{W}` | "Strive — This spell costs {2}{W} more to cast for each target beyond the first." (CR 207.2c): the cost is added once per target beyond the first as the total cost is determined (CR 601.2f); a variable target group stops offering targets once another could not be paid for. Printed first. |
| `protection doesn't remove this` | "This effect doesn't remove this Aura.", printed after the static ability before it (`static enchanted: has protection from chosen color`): the Aura's state-based check (CR 704.5m) ignores protection. |
| `ninjutsu {1}{U}` | Ninjutsu (CR 702.49): activated from the hand once blockers are declared, returning an unblocked attacker you control to its owner's hand as a cost; the card enters tapped and attacking what that creature attacked. It was never declared as an attacker. |
| `scavenge {5}{G}`, `embalm {W}`, `eternalize {4}{U}{U}` | Abilities activated from the graveyard as a sorcery, exiling the card as a cost (CR 702.97, .128, .129). Scavenge: +1/+1 counters equal to the card's power on target creature. Embalm: a token copy of the card that's a white Zombie with no mana cost; eternalize: a 4/4 black Zombie with no mana cost. The token is a definition of its own, made from the card's block. |
| `encore {4}{B}` | Encore (CR 702.141): activated from the graveyard as a sorcery, exiling the card as a cost; creates a token copy of the card (one: the game has one opponent) that gains haste and attacks this turn if able, sacrificed at the beginning of the next end step. Prints `Encore {4}{B}`. |
| `dash {2}{R}` | Dash (CR 702.109): cast for this cost instead; the permanent has haste and returns to its owner's hand at the beginning of the next end step. |
| `plot {3}{U}`, `triggered when this becomes plotted: ...` | Plot (CR 702.170): during its owner's main phase with the stack empty, a special action exiles the card from the hand for this cost; it is plotted, and from a later turn on its owner may cast it from exile without paying its mana cost, as a sorcery. Prints `Plot {3}{U}`. "When this card becomes plotted, it deals 2 damage to any target." triggers from exile as it is plotted. As an effect, `it becomes plotted` after an exile ("exile target spell; it becomes plotted": "Exile target spell. It becomes plotted."; `you may exile 1 nonland with mana value 3 or less card from your hand chosen; if you do, that card becomes plotted`, printed "... If you do, it becomes plotted."): the exiled card becomes plotted (a spell exiled from the stack too). `plot that spell as it resolves` prints "exile that spell instead of putting it into your graveyard as it resolves. If you do, it becomes plotted." (a countered spell is not exiled). `enters with 2 +1/+1 counters if no mana spent` prints "... if it wasn't cast or no mana was spent to cast it." |
| `sneak {2}{W}{B}`, `if sneak paid`, `if his sneak paid` | Sneak (CR 702.190): any time its controller could cast an instant during their declare blockers step, the card may be cast from the hand for this cost and an unblocked attacker they control returned to its owner's hand (chosen with the costs), instead of its mana cost, whatever its type; a permanent enters tapped and attacking what that creature attacked. Prints `Sneak {2}{W}{B}`. The static condition `sneak paid` reads "this spell's sneak cost was paid" on an instant or sorcery, "its sneak cost was paid" on a permanent (kept by it); `his sneak paid` and `her sneak paid` print "his"/"her". |
| `harmonize {4}{R}` | Harmonize (CR 702.180): the card may be cast from its owner's graveyard for this cost instead of its mana cost (timing as usual), tapping up to one untapped creature they control, chosen as the cost is announced (CR 702.180b), to reduce the generic mana by that creature's power; the spell is exiled as it leaves the stack. Prints `Harmonize {4}{R}`. |
| `web slinging {W}`, `if web slung`, `if they web slung` | Web-slinging (CR 702.188): the card may be cast for this cost instead of its mana cost (timing as usual), returning a tapped creature its caster controls to its owner's hand (chosen with the costs). Prints `Web-slinging {W}`. The permanent keeps how it was cast: the static condition `web slung` prints "it was cast using web-slinging"; `he web slung`, `she web slung`, `they web slung` print "he was", "she was", "they were". |
| `impending 4 {1}{U}{U}` | Impending (CR 702.176): the card may be cast for this cost instead of its mana cost; the permanent then enters with that many time counters, isn't a creature while it has one (its other card types replace its own, a layer 4 static ability with the condition `impending`: "this permanent's impending cost was paid and it has a time counter on it"), and at the beginning of its controller's end step loses a time counter while that holds. Prints `Impending 4—{1}{U}{U}`; the card then calls itself "this permanent". |
| `warp {1}{U}` | Warp (CR 702.185): cast from the hand for this cost instead; the permanent is exiled at the beginning of the next end step, and its owner may cast it from exile on a later turn (paying its costs as usual). Prints `Warp {1}{U}`. |
| `bestow {3}{W}` | Bestow (CR 702.103): cast for this cost instead as an Aura spell with enchant creature (not a creature, no creature types; the engine derives that Aura from the card). It becomes a creature again when it isn't attached, and resolves as a creature if its target is illegal. Write the creature's own abilities as usual, with `static enchanted: ...` for what the Aura grants. |
| `overload {2}{U}` | Overload (CR 702.96): an instant or sorcery with one target may be cast for this cost instead; it then has no targets and its effects happen once for each object or player the target spec describes ("target" reads "each"), regardless of hexproof or protection. Prints `Overload {2}{U}`. |
| `compleated` | Compleated (CR 702.150a): a planeswalker whose caster paid 2 life for some of its Phyrexian mana symbols (CR 601.2b) enters with two fewer loyalty counters for each, none at all if that leaves none (a 0-loyalty planeswalker then goes to the graveyard, CR 704.5i). Prints `Compleated`. |
| `conspire` | Conspire (CR 702.78): as the spell is cast its caster may tap two untapped creatures they control that share a color with it (the engine picks which); a trigger then copies the spell (CR 707.10), and the copy's controller may choose new targets for it. Prints `Conspire`. |
| `enlist` | Enlist (CR 702.154): as this creature attacks, its controller may tap a nonattacking creature they control that has haste or has been under their control since the turn began; this creature gets +X/+0 until end of turn, X that creature's power. The engine makes it an attack trigger, the creature chosen as it resolves. Prints `Enlist`. |
| `assist` | Assist (CR 702.132): "Another player can pay up to {N} of this spell's cost." The only other player is an opponent, who here never pays, so the line has no effect. Prints `Assist`. |
| `dredge 3` | Dredge (CR 702.52): "If you would draw a card, you may mill N cards instead. If you do, return this card from your graveyard to your hand." Not with fewer than N cards in the library. Asked before the draw's other replacement effects (a `ChooseCard` decision: the dredge cards, or `Done` to draw). Prints `Dredge 3`. |
| `awaken 3 {5}{B}` | Awaken (CR 702.113): an alternative cost; cast for it, the spell also puts N +1/+1 counters on target land you control (a target after its own), which becomes a 0/0 Elemental creature with haste, still a land, indefinitely (the engine derives that spell from the card). Prints `Awaken 3—{5}{B}`. |
| `craft with artifact {2}{R}`, `craft with Cave {5}{G}` | Craft (CR 702.167): "{2}{R}, Exile this artifact, Exile another artifact you control or an artifact card from your graveyard: Return this card transformed under its owner's control. Craft only as a sorcery." on the front face of a transform card; one material matching the filter, other than the source (the engine picks one, a graveyard card first). Prints `Craft with artifact {2}{R}`. |
| `storm` | Storm (CR 702.40): as the spell is cast, a trigger copies it once for each spell cast before it this turn by any player (CR 707.10), and the copies' controller may choose new targets for them. Prints `Storm`. |
| `replicate {1}{R}` | Replicate (CR 702.56): an additional cost the caster may pay any number of times; as the spell is cast, it is copied once for each time, the copies' targets may change. Prints `Replicate {1}{R}`. |
| `multikicker {1}{G}` | Multikicker (CR 702.33c): a kicker cost the caster may pay any number of times (offered where a kicker is, up to as many times as the mana pays for, at most 15); paid at least once, the spell is kicked (CR 702.33d), and the count `times kicked` ("for each time it was kicked", `enters with count each times kicked +1/+1 counters`) counts the payments, kept by the permanent. Prints `Multikicker {1}{G}`. |
| `casualty 2` | Casualty (CR 702.153): as the spell is cast its caster may sacrifice a creature with power 2 or greater; when they do, a trigger copies the spell (CR 707.10), and the copy's controller may choose new targets for it. Prints `Casualty 2`. |
| `ripple 4` | Ripple (CR 702.60): as the spell is cast, a trigger reveals the top four cards of its caster's library; each with the same name may be cast without paying its mana cost (the caster chooses), and the rest go on the bottom. Prints `Ripple 4`. |
| `teamwork 2` | Teamwork: an optional additional cost, "tap any number of creatures you control with total power 2 or more" (the engine picks them: the weakest creature enough alone, else the strongest first). `if teamwork, ...` reads "If this spell was cast using teamwork, ...". `if teamwork, instead exile target creature and you gain 3 life` (Cruel Alliance): as with the kicker, the spell cast using teamwork has the "instead" sentence's targets in place of the others (CR 601.2c), and teamwork is offered only when they exist (and required when only they do). Prints `Teamwork 2`. |
| `splice {1}{R}` | Splice onto Arcane (CR 702.47): as its controller casts an Arcane spell, the card may be revealed from hand and this cost paid too; the spell gains the card's `spell:` effects and targets (after its own). A cost other than mana is a cost phrase: `splice sacrifice 2 Mountain`, `splice return blue creature you control`, `splice tap untapped white creature you control` ("Splice onto Arcane—Sacrifice two Mountains."); the engine picks the permanents, sparing the spell's targets. |
| `backup 1` | Backup N (CR 702.165): "When this creature enters, put N +1/+1 counters on target creature. If that's another creature, it gains the following abilities until end of turn." The following abilities are the lines written after `backup` (keywords with `keyword ...` on their own line). |
| `offspring {1}` | Offspring (CR 702.175): an optional additional cost (scripted like kicker); if paid, when the permanent enters, a 1/1 token copy of it (a definition of its own, keeping the mana cost). |
| `bargain` | Bargain (CR 702.166): as the spell is cast, its controller may sacrifice an artifact, enchantment, or token. Printed first. `bargained` is a static condition: "If this spell was bargained, ..." (`if bargained, ...` in a spell, with `instead` like kicker; a target first named there is one only a bargained spell has, as for "if kicked", CR 601.2c: Brave the Wilds is cast unbargained with no target), "if it was bargained" (`if bargained` after a trigger), "This spell costs {2} less to cast if it's bargained." (`this spell costs {2} less if bargained`; a card in hand counts as bargained when its controller has something to sacrifice). |
| `saddle 2` | Saddle N (CR 702.171): "Tap any number of other creatures you control with total power N or more: This Mount becomes saddled until end of turn. Saddle only as a sorcery." (chosen like crew). `triggered whenever this attacks while saddled: ...` ("Whenever this creature attacks while saddled, ..."); `saddled` is also a static condition ("it's saddled"). |
| `deck any number` | "A deck can have any number of cards named ~." (CR 100.2a): a deck-building rule; nothing in a game. |
| `companion "Each nonland card in your starting deck has a different name."` | "Companion — Each nonland card ..." (CR 702.139a), the text as written: a deck-building condition for a card chosen from outside the game. Here a companion is simply a card in the deck, so the line has no meaning in a game (no companion is chosen, and it is never put into the hand for {3}). |
| `extra vote`, `extra vote may` | "While voting, you get an additional vote.", "While voting, you may vote an additional time." (CR 701.38d: the controller's extra votes are cast when they would vote; the optional one may be declined). |
| `as turned face up put 4 +1/+1 counters`, `as enters or turned face up put count where other creature you control +1/+1 counters` | "As this creature is turned face up, put four +1/+1 counters on it.", "As this creature enters or is turned face up, put X +1/+1 counters on it, where X is the number of other creatures you control." CR 708.11: the counters are put on while it turns face up (by its cost or by an effect), before "turned face up" triggers look; with `enters or` it also enters with them (CR 614.1c, the count judged as it enters). |
| `may look face-down`, `may look face-down opponents`, `may look top`, `may look top face-down` | "You may look at face-down creatures you don't control any time." ("... your opponents control ...", the same in a two-player game), "You may look at the top card of your library any time.", "You may look at the top card of your library and at face-down creatures you don't control any time." Information only (CR 401.2, 708.5): the controller's view shows those cards. |
| `additional land`, `additional land 2`, `additional land each player`, `additional land as long as you control other Elf` | "You may play an additional land on each of your turns.", "You may play two additional lands on each of your turns.", "Each player may play an additional land on each of their turns.", "As long as you control another Elf, you may play an additional land on each of your turns." (CR 305.2): more land plays on its controller's turns (each player's own turns with `each player`), while the condition holds. The effect `play additional land this turn` ("You may play an additional land this turn.") and `play 2 additional land this turn` ("You may play up to two additional lands this turn.") add plays for the rest of the turn. |
| `as this enters choose number between 1 and 10`, `as this enters choose number`; `ban casting noncreature with mana value equal chosen number` | "As Talion enters, choose a number between 1 and 10." / "As this creature enters, choose a number." (any number: 0 to 20 are offered; `choose "3"` in scenarios). The filter qualifiers `with mana value equal chosen number` ("with mana value equal to the chosen number") and `with mana value or power or toughness equal chosen number` ("with mana value, power, or toughness equal to the chosen number": any of the three) read the source's choice; unfilled, they match nothing. `ban casting <filter>`: "Noncreature spells with mana value equal to the chosen number can't be cast." (Sanctum Prelate, CR 601.2): no player is offered casting a matching spell, judged from the card's printed characteristics (X as 0). In a cast trigger: `whenever an opponent casts with mana value or power or toughness equal chosen number spell` ("Whenever an opponent casts a spell with mana value, power, or toughness equal to the chosen number", Talion). |
| `as this enters choose card name`, `as this enters choose nonland card name`; `spells with chosen name can't be cast`, `activated abilities of sources with chosen name can't be activated [unless mana]` | "As this artifact enters, choose a [nonland] card name." (CR 201.3): the names offered are those of the cards in the game (any zone, either player's, the entering card's own included), in name order (`choose "Lightning Bolt"` in scenarios), since naming a card not in the game changes nothing. "Spells with the chosen name can't be cast." (Meddling Mage, Nevermore): no player is offered casting a spell of that name. "Activated abilities of sources with the chosen name can't be activated [unless they're mana abilities]." (Pithing Needle, Phyrexian Revoker): in any zone, and without `unless mana` not even its mana abilities to pay costs (Voidstone Gargoyle). `as this enters look at opponent's hand then choose card name`: "As this artifact enters, look at an opponent's hand, then choose any card name." (Sorcerous Spyglass): the next player's hand is revealed before the choice. |
| `each player can't cast more than one spell each turn`, `you can't ...`, `opponents can't ...`, `enchanted player can't ...` | "Each player can't cast more than one spell each turn." ("You can't ...", "Your opponents can't ...", "Enchanted player can't ...": a Curse's player; CR 101.2): once one of those players has cast a spell this turn, no other spell of theirs is offered, by any permission (an effect's "you may cast it" included). Copies aren't cast. Lands may still be played. |
| `each player can't draw more than one card each turn`, `opponents can't draw more than one card each turn` | "Each player can't draw more than one card each turn." ("Each opponent can't ..."): a draw after the player's first this turn doesn't happen (nor any replacement of it). |
| `players can't draw cards`, `players can't draw cards or gain life` | "Players can't draw cards." (CR 121.1), "Players can't draw cards or gain life.": no draw happens (the draw step's included, and no draw is replaced, dredge included), and with `or gain life` no life is gained. The effects `players can't draw cards this turn` and `players can't draw cards or gain life this turn` do the same until end of turn. |
| `opponents can't gain life`, `enchanted player can't gain life` | "Your opponents can't gain life.", "Enchanted player can't gain life." (CR 119.7): life those players would gain isn't gained. The effect `<who> can't gain life this turn` (`opponents can't gain life this turn`: "Your opponents can't gain life this turn.") does the same until end of turn. |
| `legend rule doesn't apply`, `legend rule doesn't apply to permanents you control` | 'The "legend rule" doesn't apply.' ('... to permanents you control.'; CR 704.5j): no player (only its controller) keeps just one of their legendary permanents of a name. The effect `you ignore legend rule this turn` prints 'The "legend rule" doesn't apply to permanents you control this turn.' (Hall of Echoes). |
| `ban attacking you <filter>`, `ban attacking you or planeswalkers <filter>` | "Creatures with flying can't attack you." (`ban attacking you creature with flying`), "Creatures can't attack you or planeswalkers you control." (CR 508.1c): a creature matching the filter can't attack its controller (nor their planeswalkers); one that can attack nothing else isn't offered as an attacker. |
| `enchanted can't be turned face up`, `opponents permanents can't be turned face up during your turn` | "As long as enchanted creature is face down, it can't be turned face up." (CR 702.37e, 708.7: the permanent this Aura enchants), "Permanents your opponents control can't be turned face up during your turn.": neither the special action nor an effect turns it face up. |
| `graveyard cards can't be targeted` | "Cards in graveyards can't be the targets of spells or abilities." (CR 115.4): while the source is on the battlefield, no card in a graveyard is a legal target (an untargeted choice still finds them). |
| `you don't lose unspent red mana` | "You don't lose unspent red mana as steps and phases end." (CR 500.4): its controller's red mana stays in their pool as each step and phase ends, the cleanup step included, while the source is on the battlefield. |
| `you may play lands from your graveyard` | "You may play lands from your graveyard." (CR 305.1): its controller may play land cards from their graveyard, with their usual land plays. |
| `you have hexproof`, `you have shroud`, `opponents can't cast spells during your turn`, `players can't gain life`, `damage can't be prevented`, `combat damage can't be prevented` | Static abilities about players: "You have hexproof." (CR 702.11c: spells and abilities your opponents control can't target you), "You have shroud." (CR 702.18c: no spell or ability can target you), "Your opponents can't cast spells during your turn.", "Players can't gain life.", "Damage can't be prevented." (CR 614.17), "Combat damage can't be prevented." (Frenzied Baloth: combat damage only; as an effect, `combat damage can't be prevented this turn`). Damage that can't be prevented isn't prevented by protection either (CR 615.12, 702.16d). `you can't lose the game` prints "You can't lose the game and your opponents can't win the game.": the controller doesn't lose for the state-based actions of CR 704.5a-c while it applies, and an effect that would make an opponent win does nothing. `as long as <condition>` may follow any of them. |
| `during combat players can't cast spells or activate nonmana abilities`, `during your turn opponents can't cast spells or activate abilities of artifacts creatures enchantments`, `during your turn opponents can't cast spells` | "During combat, players can't cast spells or activate abilities that aren't mana abilities." (Yuriko, Blade of the Mighty), "During your turn, your opponents can't cast spells or activate abilities of artifacts, creatures, or enchantments." (Grand Abolisher; CR 101.2): in the combat phase's steps, or during its controller's turn, those players (`players`: every player; `opponents`) can't cast spells by any route, and can't activate the abilities named: with `nonmana`, any ability that isn't a mana ability (CR 605.1a); with `of artifacts creatures enchantments`, any ability of such a permanent on the battlefield, mana abilities included (so automatic payment doesn't tap them either), but not cycling from a hand. The effect `each player can't cast spells this turn` prints "Players can't cast spells this turn." (Bilbo's Gambit). |
| `you have protection from opponents` | "You have protection from each of your opponents." (CR 702.16j; Absolute Virtue): its controller can't be the target of spells or abilities an opponent controls, isn't dealt damage by sources an opponent controls (unless the damage can't be prevented), and can't stay enchanted by an Aura an opponent controls (state-based action, CR 704.5m). `as long as <condition>` may follow. |
| `static you and other Hero you control: has hexproof`, `static during your turn, this and enchantment creature you control: has trample, lifelink, has ward {2}` | A static ability whose subject also names its controller or its source: "As long as ..., you and other Heroes you control have hexproof." (Captain America, Super-Soldier: the controller has hexproof or shroud too, the player-level `you have hexproof` with the same condition; other modifications are an error), "During your turn, Yuna and enchantment creatures you control have trample, lifelink, and ward {2}." (the source gets the same modifications as a second static ability). |
| `enchanted can't be targeted by opponents`, `enchanted can't be targeted by spells`, `flagbearer rule` | On an Aura: "Enchanted creature can't be the target of spells or abilities your opponents control." (CR 115.4; "your" is the Aura's controller, not the enchanted creature's, so unlike hexproof the creature's controller can be the one kept out), "Enchanted creature can't be the target of spells." (any player's spells; abilities still may). `flagbearer rule` prints "While an opponent is choosing targets as part of casting a spell they control or activating an ability they control, that player must choose at least one Flagbearer on the battlefield if able." (Standard Bearer): while none of the targets chosen so far is a Flagbearer, the last target of a spell or activated ability of an opponent's that could be a Flagbearer (any player's) must be one if one is a legal choice; triggered abilities are free (rulings). |
| `first equip free`, `first equip free during your turns`, `first equip free as long as enduring story` | "You may pay {0} rather than pay the equip cost of the first equip ability you activate each turn." (`during your turns`: "... during each of your turns."; CR 118.9): the first equip ability its controller activates in such a turn costs no mana (the engine takes the option, though a player might rather keep it for a later equip that turn); a player static ability, so `as long as <condition>` prints first ("As long as you have an enduring story, you may pay {0} ..."). |
| `hexproof from black`, `hexproof from monocolored`, `hexproof from instant` | "Hexproof from black" (CR 702.11d): this permanent can't be the target of spells or abilities your opponents control from sources matching the filter (a color, `multicolored`, `monocolored`, a card type: "instants", "planeswalkers"). In a keyword line ("Flying, hexproof from multicolored") write the keywords first, then this line. |
| `trigger again Ninja creature you control`, `trigger again other Elemental you control`, `trigger again permanent you control when permanent enters the battlefield`, `trigger again permanent you control when artifact or creature enters`, `trigger again permanent you control when creature dies`, `trigger again permanent you control when creature attacks` | CR 603.2d: "If a triggered ability of a Ninja creature you control triggers, that ability triggers an additional time." ("it triggers" after `other`, as Twinflame Travelers prints); with `when ... enters`, only when that permanent entering caused it ("If a permanent entering the battlefield causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time.", `the battlefield` printed where written); `when creature dies` ("If a creature dying causes ...", the creature judged as it last existed) and `when creature attacks` ("If a creature attacking causes ...") likewise. Only such abilities on the battlefield as the event happens count (one dying with the creature doesn't). Each such ability adds one more instance; each instance is put on the stack with its own targets and choices |
| `trigger never permanent when creature enters`, `trigger never permanent when andlist artifact or creature enters the battlefield` | CR 603.6a (Torpor Orb): "Creatures entering don't cause abilities to trigger.", "Artifacts and creatures entering the battlefield don't cause abilities to trigger.": an enters trigger (`when this enters`, `whenever other creature enters`, `whenever one or more ... enter`, either event of two) of a permanent matching the first filter (any player's) doesn't trigger at all on a permanent matching the second entering, whatever would make it trigger more. Replacement effects on entering (counters, tapped) still apply |
| `opening hand` | "If this card is in your opening hand, you may begin the game with it on the battlefield." (CR 103.6a; a legendary card names itself: "If Quicksilver is in your opening hand, ..."). After mulligans, starting with the starting player, each player is asked about each such card in their hand. |
| `opening hand reveal first upkeep: create 1 "Phyrexian Goblin"`, `opening hand reveal your first upkeep: ...`, `opening hand reveal your first main: add {G}`, `opening hand reveal your first upkeep after: scry 3` | "You may reveal this card from your opening hand. If you do, at the beginning of the first upkeep, create ..." (the Chancellors; "at the beginning of your first upkeep, ...", "at the beginning of your first main phase of the game, ..."; `after` puts the time last: "If you do, scry 3 at the beginning of your first upkeep."). Asked with the opening-hand cards; the revealed card stays in hand, known to every player, and leaves a delayed trigger for the next such step (CR 603.7), which happens once, controlled by the card's owner. |
| `play exiled spells`, `play exiled lands and spells`, `play exiled your turn lands and spells`, `play exiled owned spells Dinosaur creature` | "You may cast spells from among cards exiled with Rona.", "You may play lands and cast spells from among cards exiled with this creature.", "During your turn, you may play cards exiled with this artifact.", "You may cast Dinosaur creature spells from among cards you own exiled with this creature." (CR 607.2a, 601.3): the controller may play the cards this permanent's abilities exiled while they stay in exile, paying their costs with the usual timing and land plays. `may look exiled` is "You may look at cards exiled with this creature." and `play exiled look lands and spells` "You may look at the cards exiled with this enchantment, and you may play lands and cast spells from among those cards.": the permanent's controller may look at the face-down ones (CR 406.3), as with hideaway's card (CR 702.75a): a new controller of the permanent may look at them from then on, and a player who looked keeps knowing them while they stay exiled. Before `lands` / `spells`, riders: `play exiled your turn flash any type spells` is "During your turn, you may cast cards exiled with Azula and you may cast them as though they had flash. Mana of any type can be spent to cast those spells." (CR 702.8a, 609.4b: at any time the controller could cast an instant, with mana of any type); `any type` alone adds the second sentence; `play exiled owned finality spells Dinosaur creature` adds "If you cast a spell this way, that creature enters with a finality counter on it." (CR 122.1h); `play exiled the card any type spells if 7 cards in your graveyard` is "As long as there are seven or more cards in your graveyard, you may cast the exiled card, and mana of any type can be spent to cast that spell." (Null Summoner: the condition, written last, is judged whenever the card could be cast, CR 611.3a). A card with such a permission links what its abilities exile ("the exiled card") to it. |
| `cast free from hand spells`, `cast free spells Dragon`, `cast free from hand spells max count creature you control`, `cast free once your turns from hand spells instant or sorcery`, `cast free first each player spells` | "You may cast spells from your hand without paying their mana costs." (Omniscience), "You may cast Dragon spells without paying their mana costs." (no zone named: the hand is offered), "You may cast spells with mana value less than or equal to the number of creatures you control from your hand ...", "Once during each of your turns, you may cast an instant or sorcery spell from your hand without paying its mana cost.", "The first spell each player casts during each of their turns may be cast without paying its mana cost." (CR 118.9): the player may cast a matching card from their hand without paying its mana cost, with its usual timing (a separate option from paying; the scenario line is `cast "X" free`). `max` takes a count; `first each player` applies to every player, the first spell in each of their own turns. |
| `play from top lands`, `play from top spells creature`, `play from top lands and spells`, `play from top once spells instant or sorcery` | "You may play lands from the top of your library.", "You may cast creature spells from the top of your library.", "You may play lands and cast spells from the top of your library.", "Once each turn, you may cast an instant or sorcery spell from the top of your library." (CR 601.3, 305.1): the controller may play the top card of their library, paying its costs, with the usual timing and a land only with a land play left. The spells' filter is any filter (`Spider or noncreature`: "Spider spells and noncreature spells"; `with mana value 4 or greater`: "spells with mana value 4 or greater"; `with flash or flying`); `once` allows one spell a turn by that permanent's permission. `play from top card` prints "You may play the top card of your library." and `play from top cards` "... play cards from the top of your library" (both lands and spells). `look` first joins "You may look at the top card of your library any time, and you may play lands ..." (Case of the Locked Hothouse; with `cards`, "... any time and you may play cards ..."): the controller may look as with `may look top`. After the permission: `as though flash` ("If you cast a spell this way, you may cast it as though it had flash.", Elsha of the Infinite: such a spell may be cast at instant speed), `pay life` ("If you cast a spell this way, pay life equal to its mana value rather than pay its mana cost.", Bolas's Citadel: an alternative cost, X is 0, offered with that much life, CR 118.9, 119.4; additional costs and cost increases are still paid; where another permission lets the card be cast for its mana cost, the mana cost is paid), and `as long as <condition>` ("As long as The Lunar Whale attacked this turn, you may ...": judged for the source; `this is that attacked this turn` prints "<name> attacked this turn"). `play from top this turn ...` is the effect: "Until end of turn, you may look at the top card of your library any time, and you may play lands and cast spells from the top of your library." (`play from top this turn look lands and spells`, The Belligerent; `play from top this turn look cards pay life`, Gwenom, Remorseless), the same permission for the rest of the turn (no condition or flash). |
| `top revealed`, `top revealed players` | "Play with the top card of your library revealed.", "Players play with the top card of their libraries revealed.": every player knows that top card (CR 401.2). |
| `hands revealed players` | "Players play with their hands revealed.": every card in a hand is known to every player while it is there. |
| `activated abilities of artifact you control cost {1} less min one`, `activated abilities of land you control cost {1} less`, `power-up abilities of other creature you control cost {3} less`, `exhaust abilities of other permanent you control cost {2} less` | "Activated abilities of artifacts you control cost {1} less to activate. This effect can't reduce the mana in that cost to less than one mana." (`min one`), "Power-up abilities of other creatures you control cost {3} less to activate." (CR 601.2f by way of 602.2b): the generic mana of the matching permanents' activated abilities (of that keyword: `power-up`, `exhaust`, `boast`, `ninjutsu`) is reduced as they are activated; with `min one`, never below one mana in all. |
| `activated abilities of creature cost {1} less min one`, `activated abilities of enchanted creature cost {3} more`, `activated abilities cost {2} more unless mana`, `abilities you activate cost {2} less unless mana min one`, `abilities your opponents activate cost {1} more unless mana`, `equip abilities you activate cost {1} less`, `equip abilities you activate of other Equipment cost {1} less`, `loyalty abilities of planeswalker your opponents control cost {1} more`, `activated abilities of sources with chosen name cost {2} more` | The general form: `<class> abilities [you activate \| your opponents activate] [of <filter> \| of sources with chosen name] cost {N} less\|more [unless mana] [min one]`. "Activated abilities of creatures cost {1} less to activate." (Heartstone: any player's), "Activated abilities of enchanted creature cost {3} more to activate." (Oppressive Rays), "Activated abilities cost {2} more to activate unless they're mana abilities." (Suppression Field: with no `of`, every activated ability, in any zone: cycling and loyalty abilities too), "Abilities you activate that aren't mana abilities cost {2} less to activate." (`unless mana` after `you activate` prints so; `you activate`: only those the source's controller activates, whoever controls their source), "Equip abilities you activate cost {1} less to activate.", "Loyalty abilities of planeswalkers your opponents control cost {1} more to activate.", "Activated abilities of sources with the chosen name cost {2} more to activate." (Skyseer's Chariot, CR 201.3). Classes: `activated` (all), `power-up`, `exhaust`, `boast`, `ninjutsu`, `equip` (CR 702.6a), `loyalty` (CR 606.3). Increases apply before reductions (CR 601.2f); `min one` holds the reduction to leave at least one mana. A filter (`of ...`) matches only permanents. |
| `hand size none`, `hand size +1`, `hand size opponents -1`, `hand size 8` | "You have no maximum hand size.", "Your maximum hand size is increased by one.", "Each opponent's maximum hand size is reduced by one.", "Your maximum hand size is eight." (CR 402.2). The count `difference 10 hand` ("draw cards equal to the difference", after the condition `hand fewer than 10`: "if you have fewer than ten cards in hand") is ten minus the count, at least 0. |
| `except { ... }` | What this card's copy effects add (CR 707.9b): `not legendary` ("it isn't legendary"; "the copy isn't legendary" for a spell copy; `its not legendary`: "it's not legendary"), `types Artifact` and `subtypes Illusion` ("it's an artifact in addition to its other types", "it's a Vehicle artifact ..." together), `keywords haste`, `pt 1/1` and `colors red` ("it's a 1/1 red Balloon creature in addition to its other colors and types": base power and toughness, and colors added), and ability lines ("it has haste and \"At the beginning of the end step, sacrifice this token.\"": on an instant or sorcery, `this` in them is the token); `legendary` ("it's legendary": the legend rule then applies to it); `exactly` with `pt`, `colors`, and `subtypes` ("it's a 1/1 green Frog", "a 4/4 black Zombie", no "in addition": its colors and creature types are these instead, its card types stay); `pt 1/1` alone ("it's 1/1"); `the token` (wording: "except the token isn't legendary"; "the tokens aren't legendary" for two or more, and with `pronoun they` "if they're not a token ... copies of them", The Notary Hobbits); `and is` (wording: "it isn't legendary and is a Mutant in addition to its other types", not "and it's"; with keywords too, "..., is a Reflection in addition to its other types, and has haste"); a card with a
`pronoun` becoming a copy itself prints it ("Oko becomes a copy of ...,
except he has hexproof"; "his other types"); `named this` ("except his name
is Absorbing Man", first: the copy keeps the name of the card whose copy
effect made it, CR 707.9b, for the legend rule and wherever a name is
shown; "named" filters still compare the copied card), and with types or
subtypes `legendary` joins them ("he's a legendary 4/4 Human Villain
creature in addition to his other types"); `noun creature` (wording: a
spell's token copies' abilities say "sacrifice this creature", Face
Yourself, not "this token"). The template author writes the block from the
sentence ("Create a token that's a copy of target creature, except it's an
artifact in addition to its other types."). They are part of the copy's copiable values; the legend rule reads its supertypes. A token copy of a creature that has left the battlefield ("When this creature dies, if it's not a token, create a token that's a copy of it"; `if this is nontoken`; `if this isnt token` prints "if it isn't a token") is made as it last existed. |

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
a matching permanent they control), `sacrifice granter`, `tap granter`,
`unattach granter` (in a quoted ability an Aura or Equipment grants:
"Sacrifice Blazing Torch", "Tap Fishing Pole", "Unattach Leonin Bola",
see `has ability`), `discard 1 card`, `discard 1 card
at random` ("Discard a card at random": the card is picked at random as
the cost is paid, CR 701.9b), `discard creature
card` (a filter: discard one matching card, printed "Discard a creature
card"), `discard X cards` (a spell's additional cost: "As an additional
cost to cast this spell, discard X cards."; X is announced as the spell
is cast, at most the other cards in hand, CR 601.2b; `discard X creature
cards` counts only matching cards), `discard this`, `exile this from graveyard` ("Exile this card from
your graveyard"; the ability is activated from the graveyard), `exile
this from hand` ("Exile this card from your hand"; the ability is
activated from the hand, and the card goes to exile as the cost is
paid; an ability that only adds mana is a mana ability, resolving at
once, CR 605.3b, though payment doesn't activate it on its own: Simian
Spirit Guide),
`exile this` ("Exile this artifact": the permanent exiles itself), `return
land you control` ("Return a land you control to its owner's hand";
`return this`: "Return this enchantment to its owner's hand") and
`tap untapped creature you control` ("Tap an untapped creature you
control"), chosen like a sacrifice; a count after `sacrifice`, `return`,
or `tap` takes that many: `tap 2 untapped artifact or creature you
control` ("Tap two untapped artifacts and/or creatures you control"; the
player chooses the first and the engine the rest, and permanents tapped
for a cost are tapped before mana is paid, so they don't also pay it; a
cost with {T} never taps the source again for the rest), `exile 2 cards
from graveyard`, `exile creature card from graveyard`, `exile X creature
cards from graveyard` ("Exile two cards from your graveyard", "exile a
creature card from your graveyard"; X announced as a spell is cast, at
most the cards there; the engine exiles the oldest matching cards other
than the source, sparing what the spell or ability targets; `exile top
card from graveyard`, `exile top creature card from graveyard`: "Exile the
top card of your graveyard", "Exile the top creature card of your
graveyard", the most recent, CR 404.2; `exile 2 cards from graveyard and
this`: "Exile two cards from your graveyard and this creature", one
part), `pay 3
life`, `pay x life` (a spell's additional cost: "As an additional cost to
cast this spell, pay X life."; X is announced as the spell is cast, at most
the caster's life, CR 119.4; in an activated ability's cost, "{X}{B}, {T},
Pay X life: ...", the ability's X), `sacrifice any number creature` and
`may sacrifice any number Spirit` (a spell's additional cost: "As an
additional cost to cast this spell, sacrifice any number of creatures." /
"..., you may sacrifice any number of Spirits."; the number is X,
announced as the spell is cast, at most the matching permanents; the
caster chooses the first and the engine the rest; `count each creature
sacrificed this way` counts them), `exert this` ("Exert this creature", "Exert
Basri": it won't untap during its controller's next untap step, CR
701.43a), `discard hand` ("Discard your hand"; payable with no cards in
hand), `sorcery` (printed "Activate only as a sorcery."), `once` (activate
only once each turn). `none` for a free ability. Activation restrictions
(CR 602.5b): `if <static condition>` ("Activate only if there are seven or
more cards in your graveyard"; `if blocked` prints "only if this creature is
blocked", `if this is creature` "only if this permanent is a creature";
`if this is that entered this turn or if you control basic land`: "only
if this land entered this turn or if you control a basic land", either),
`during your upkeep`, `during opponent upkeep` ("during an opponent's
upkeep"), `during your turn`, `during combat`, `before attackers` ("before
attackers are declared"), `during your turn before attackers` ("during your
turn, before attackers are declared"), `during declare blockers`, `during
end of combat`, and `once ever` ("only once": once for as long as the
source stays the same object, CR 400.7). `sorcery`, `once`, and the
restrictions print as one
sentence in the order written: `if you control enchantment, sorcery` is
"Activate only if you control an enchantment and only as a sorcery".
`any player` ("Any player may activate this ability.") and `opponents
only` ("Only your opponents may activate this ability.") say who may
activate it (CR 602.1): the activating player pays the cost and controls
the ability (CR 602.2a). `replace destroy this: regenerate` prints "If this
creature would be destroyed, regenerate it." (a regeneration that is never
used up; "can't be regenerated" stops it, CR 701.15c).
`reduced {1} each <count>` ("This ability costs {1} less to activate for
each Town you control.") and `reduced {4}{B} if <static condition>` ("...
less to activate if you've drawn three or more cards this turn.") take
mana off the ability's cost as it is activated (CR 601.2f by way of
602.2b), printed after the effect and before the restrictions; `reduced
{1} if targets creature with power 3 or less` ("... if it targets a
creature with power 3 or less") once if the first target matches, so the
ability is offered if some target makes it payable and only such targets
are offered. `reduced {1} each +1/+1 counter on target` and `reduced {1} each
color of target` ("... for each +1/+1 counter on the creature it
targets", "... for each color of the creature it targets") take `by` off
once for each, judged on the first target the same way. On an equip
ability, `equip {3}, reduced {1} each other Equipment you control` prints
"Equip {3}. This ability costs {1} less to activate for each other
Equipment you control." `static equipped creature: +2/+2, has hexproof
from monocolored` grants "hexproof from monocolored" (CR 702.11d) bare. The card-level line `equip abilities targeting this cost {2}
less` prints "Equip abilities you activate that target this creature cost
{2} less to activate." (generic mana off any equip ability aimed at it);
`equipment abilities targeting this cost {1} less` prints "Activated
abilities of Equipment you control that target this creature cost {1}
less to activate." (off any activated ability of an Equipment you control
whose first target is it, equip or not).
An ability whose cost
includes `discard this` is activated from the hand (cycling, channel).
`waterbend {3}` (CR 701.67) is generic mana, printed "Waterbend {3}", of
which each {1} may be paid by tapping an untapped artifact or creature its
payer controls (the engine taps them after its mana sources; the source of
an ability with {T} doesn't help). As a spell's additional cost
(`additional cost waterbend {5}`: "As an additional cost to cast this
spell, waterbend {5}."), only that cost's own generic mana may be paid
this way (CR 701.67b). As an effect, `discard 1 card unless you waterbend
{2}` ("Then discard a card unless you waterbend {2}."): the controller
may pay it the same way (`accept`/`decline` in scenarios).

### Triggers

| Trigger | Oracle |
|---|---|
| `when this enters`, `whenever other creature enters` | enters the battlefield |
| `when this dies`, `whenever creature dies`, `whenever creature you control dies`, `whenever a creature dies` | dies; for a noncreature other than a Vehicle, "is put into a graveyard from the battlefield". A filter without `other` includes the source ("this creature or another creature"), unless the card can't match it (an enchantment's `whenever Angel you control dies`: "Whenever an Angel you control dies"); written with its article it prints the older wording, "Whenever a creature dies" (wording only). |
| `at beginning of your upkeep`, `at beginning of each draw step`, `at beginning of the end step` | beginning of a step; steps: `untap`, `upkeep`, `draw`, `combat`, `end` |
| `at beginning of every upkeep` | "At the beginning of each upkeep" (werewolves' wording) |
| `at beginning of enchanted controller's upkeep`, `at beginning of enchanted controller's end step` | "At the beginning of the upkeep of enchanted creature's controller" (the noun from the Aura's enchant restriction): the step of whoever controls what the source is attached to, judged as the step begins; that permanent is `that creature`, its controller `that player` |
| `whenever this attacks`, `whenever creature you control attacks`, `whenever this blocks` | attacks or blocks |
| `whenever this deals combat damage to a player`, `whenever enchanted creature deals combat damage to defending player` | the damaged player is "that player" (`target creature that player controls`); `defending player` is wording only (combat damage to a player is dealt to the defending player, CR 510.1) |
| `whenever this deals combat damage to a player or planeswalker` | Either attack target; the player dealt damage, or the planeswalker's controller, is the triggering player. Damage to a blocker doesn't trigger it. |
| `when you control no Island`, `when you control no other creature`, `when you control Dwarf` | a state trigger (CR 603.8): "When you control no Islands, sacrifice this creature."; any static condition after `when you control` ("When you control a Dwarf", "When you control a creature with toughness 2 or less"), or `when there are 9 or more incarnation counters on this` ("When there are nine or more incarnation counters on this enchantment, exile it.", Nine Lives: `it` in a state trigger is the source). Checked whenever a player would receive priority; it doesn't trigger again while its ability waits to go on the stack or is on it. A face-down permanent has no abilities, so no state trigger |
| `whenever you gain life` | |
| `whenever an opponent loses life`, `whenever a player loses life`, `whenever you lose life` | once per life-losing event (CR 119.3; damage too); the player is `that player`, the life lost `that much` ("you gain that much life") |
| `whenever you gain or lose life`, `whenever you gain or lose life during your turn` | once per event that changes your life total (CR 119.9: a gain or a loss of any amount is one event), only on your turn with `during your turn` |
| `whenever you scry or surveil` | once per instruction to scry or surveil (CR 701.22, 701.25) |
| `whenever you scry` | once per instruction to scry (CR 701.22; scry 0 is none, 701.22b); surveilling doesn't trigger it |
| `whenever this is dealt 3 or more damage` | "Whenever this creature is dealt 3 or more damage": one damage event of at least that much (damage dealt at different times doesn't add up) |
| `when this is put into a graveyard from anywhere` | "When Purity is put into a graveyard from anywhere" (CR 603.6a; the card names itself): the card going to a graveyard from any zone, the hand and library included; it triggers from the graveyard, controlled by the card's owner (`shuffle this into library`: "shuffle it into its owner's library") |
| `when enchanted dies or is put into exile` | "When enchanted permanent dies or is put into exile": leaves the battlefield for a graveyard or exile, judged as it last existed (`return triggering card to battlefield under your control`: "return that card to the battlefield under your control") |
| `when enchanted creature becomes tapped or is dealt damage`, `whenever this enters or becomes tapped` | two events of one subject, the shared words printed once ("When enchanted creature becomes tapped or is dealt damage, destroy it.") |
| `when this enters while this is untapped` | "When this land enters untapped": the land's state as it enters (a `while` condition, judged as the event happens) |
| `whenever creature you control explores`, `whenever this explores` | as a creature explores (CR 701.44), whatever it revealed and even if it has left the battlefield (then judged as it last existed); it is the triggering object |
| `whenever you tap untapped creature an opponent controls` | "Whenever you tap an untapped creature an opponent controls": a spell or ability you control (an effect, or a cost such as `tap 1 untapped creature ... chosen` in an effect) taps an untapped permanent matching the filter; it is `that creature`. Not when an opponent taps their own creature for your effect |
| `whenever you activate an ability of a creature`, `whenever you activate an exhaust ability`, `whenever an opponent activates a loyalty ability`, `whenever you activate a ninjutsu ability`, `whenever you foretell a card` | as an activated ability is activated (CR 602.2), before it resolves: of a creature (mana abilities included, CR 605.3a; mana the engine taps for automatically while paying a cost is not an activation), an exhaust ability (CR 702.177), a ninjutsu ability (CR 702.49, a card's own or one a permanent gives it, Satoru Umezawa), a card foretold (CR 702.143a: as it is exiled, not as it is cast later), a loyalty ability (CR 606). The activating player is `that player` |
| `whenever you discard one or more cards` | once for the cards you discard together (CR 603.2c), counted as `that much` ("put that many +1/+1 counters", "deals that much damage"); cards discarded before the next time a player gets priority count as together. Cycling a card discards it |
| `whenever you sacrifice one or more Food` | once for the matching permanents you sacrifice together (CR 603.2c), counted as `that much` |
| `they lose 2 life`, `deal 2 damage to they` | The triggering player written "they" ("Whenever a player casts a noncreature spell, they lose 2 life.") and "them" as an object ("... deals 2 damage to them"); the same player as `that player`. In a "when this enters" trigger with targets (whose triggering player is you) "they" is the first target's controller, or that player: `triggered whenever this enters: destroy target permanent an opponent controls; they create 1 "Treasure"` ("They create a Treasure token.", Vraska, the Cutting Glare) |
| `whenever you attack with one or more Faerie`, `whenever you attack a player with one or more creature with power 4 or greater` | once as you declare attackers if a matching creature attacks (CR 508.1m); with `a player`, once for each player a matching creature attacks, that player being `that player` |
| `whenever you tap this for mana`, `whenever a player taps land for mana`, `whenever an opponent taps Mountain for mana`, `whenever you tap creature for mana` | "Whenever you tap this creature for mana", "Whenever a player taps a land for mana" (CR 106.11a): a matching permanent, controlled by that player, is tapped for mana by a mana ability (activated, or tapped while paying a cost); not a mana ability (CR 605.1b: it adds no mana), so it uses the stack. The permanent is the triggering object and its controller `that player` |
| `whenever this transforms`, `whenever this enters or transforms` | "Whenever this creature transforms into Ashling, Rimebound" (CR 701.27): the ability is on that face, so it sees only the transformation to it; the face's name prints |
| `whenever one or more creature you control deal combat damage to a player`, `whenever one or more Ally you control deal combat damage to a player` | once for each player dealt combat damage by matching creatures in one combat damage step (CR 603.2c; a first-strike step and the regular one trigger separately); that player is `that player` |
| `whenever one or more Hero you control deal damage to a player` | "Whenever one or more Heroes you control deal damage to a player": any damage, once for each player dealt damage by matching creatures at once (CR 603.2c); that player is `that player` |
| `whenever a source you control deals noncombat damage to an opponent`, `whenever an opponent is dealt noncombat damage`, `whenever one or more opponents are dealt noncombat damage` | damage other than combat damage (CR 120.2) to an opponent, from a source you control (a spell as it resolves, as it last existed) or any source; the last once for the damage dealt to each opponent at once. The opponent is `that player`, `that much` the damage |
| `whenever you commit a crime`, `whenever you commit a crime during your turn` | "Whenever you commit a crime" (CR 700.13): as you finish casting a spell, activating an ability, or putting a triggered ability on the stack that targets an opponent, something an opponent controls, or a card in an opponent's graveyard; once per spell or ability. A copy put on the stack commits none. `during your turn` (Overzealous Muscle) only on your turn |
| `whenever you gain life first time each turn` | "Whenever you gain life for the first time each turn": the gain that starts the life you gained this turn (one of several simultaneous lifelink gains); with `during your turn` after it, "Whenever you gain life for the first time during each of your turns" |
| `whenever this or creature you control leaves the battlefield without dying`, `whenever one or more other creature you control leave the battlefield without dying` | to any zone but a graveyard (CR 700.4), judged as it last existed; the second batched (CR 603.2c) |
| `whenever other nonland permanent you control is returned to hand` | "Whenever another nonland permanent you control is returned to its owner's hand": leaves the battlefield for a hand, judged as it last existed |
| `whenever one or more suspected creature you control attack` | "Whenever one or more suspected creatures you control attack": the older wording of `whenever you attack with one or more ...` (the same trigger) |
| `when you discard this` | "When you discard this card" (CR 701.9): triggers from the card itself as it is discarded (by an effect or a cost, not cycling's own "when you cycle"), controlled by the discarding player |
| `whenever you roll dice`, `whenever you win a coin flip` | "Whenever you roll one or more dice" (once per roll, CR 706), "Whenever you win a coin flip" (CR 705.2) |
| `whenever creature you control attacks alone` | attacks alone (CR 506.5) |
| `whenever creature you control attacks a player alone` | "Whenever a creature you control attacks a player alone" (CR 506.5): the only attacker declared, attacking a player rather than a planeswalker or battle; the creature is `that creature`, the player attacked `that player` |
| `whenever one or more of your opponents are attacked`, `whenever one or more creatures attack you`, `whenever one or more creatures attack one of your opponents` | once per declaration of attackers (CR 508.1m) in which a creature, anyone's, attacks a matching player (not a planeswalker or battle); the last once for each opponent attacked. The player attacked is `that player`. "Those creatures" (the attackers) is not expressible yet |
| `whenever this attacks for the first time each turn` | the attacking creature hadn't attacked yet this turn (with additional combat phases, CR 500.8) |
| `whenever you attack`, `whenever you attack with 3 or more creatures`, `label "Battalion" triggered whenever this and at least 2 other creatures attack` | once as the controller declares attackers (CR 508.1m), if at least that many attack (battalion: this creature among them); "Whenever this creature and at least two other creatures attack"; `whenever one or more creatures you control attack` is the same trigger in the older wording |
| `whenever a player attacks with 3 or more creatures`, `whenever an opponent attacks with 2 or more creatures` | "Whenever a player attacks with three or more creatures": once per declaration of attackers by such a player (CR 508.1m) if at least that many attack; the attacking player is `that player`. `whenever 2 or more creatures your opponents control attack` is the opponent's form in the older wording ("Whenever two or more creatures your opponents control attack"), the same trigger |
| `whenever this saddles a Mount or crews a Vehicle while your main phase` | "Whenever this creature saddles a Mount or crews a Vehicle during your main phase" (CR 702.122b, 702.171c): it is tapped to pay a crew or saddle cost (the engine picks the creatures); the Mount or Vehicle is "that Mount or Vehicle". `while your main phase` after a trigger prints "during your main phase" |
| `whenever this becomes saddled first time each turn` | "Whenever this creature becomes saddled for the first time each turn" (CR 702.171b: saddled until the turn ends, so once); `target Mount you control becomes saddled` is the effect ("... becomes saddled until end of turn") |
| `whenever this attacks and isn't blocked` | as blockers are declared, for an attacking creature no creature blocks (CR 509.1h) |
| `whenever this becomes blocked`, `whenever this blocks or becomes blocked` | |
| `whenever this blocks a creature with flying`, `whenever this becomes blocked by a creature`, `whenever this blocks or becomes blocked by a nonblack creature` | once for each attacker it blocks (CR 509.3b) or each creature blocking it (CR 509.3d); the other creature, which must match the filter, is "that creature"; `a creature with Equipment` prints "an equipped creature", `a creature that was dealt damage this turn` "... that has been dealt damage this turn". `... by one or more blue or black creature` ("by one or more blue and/or black creatures", CR 509.3c, 509.3e) triggers once, on the first matching pair, and "it" is the blocking or blocked creature itself (`whenever equipped creature becomes blocked by one or more colorless creature: it gains deathtouch until end of turn`) |
| `at end of combat` | as the end of combat step begins (CR 511.2); also a delayed trigger: `delayed that creature: destroy that creature at end of combat` |
| `whenever you cast a spell`, `... a noncreature spell`, `... an instant or sorcery spell`, `whenever a player casts a spell` | `whenever you cast noncreature or Dragon spell` prints "a noncreature or Dragon spell"; with the alternative's own article, `noncreature or a Dragon spell`, "a noncreature spell or a Dragon spell" (the same spells) |
| `label "Magecraft" triggered whenever you cast or copy instant or sorcery spell: ...` | "Magecraft — Whenever you cast or copy an instant or sorcery spell, ..." (a copy triggers it too, CR 707.10) |
| `label "Heroic" triggered whenever you cast a spell that targets this: ...` | "Heroic — Whenever you cast a spell that targets this creature, ..." (any spell filter: `whenever you cast red spell that targets this`): the spell's targets as it is cast (CR 601.2c), so copies and abilities don't trigger it. The spell is the triggering object, but `it` in the effects is this creature, as Oracle has it ("put a +1/+1 counter on it"). |
| `label "Repartee" triggered whenever you cast instant or sorcery spell that targets creature: ...` | "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, ...": any creature among the spell's targets as it is cast (CR 601.2c). |
| `whenever you cast a spell with X cost`, `whenever you cast permanent spell that has adventure`, `whenever you cast instant or sorcery spell with a single target`, `whenever you cast instant or sorcery spell that targets only a single creature you control`, `whenever you cast a spell that targets an opponent or creature an opponent controls`, `whenever you cast Equipment spell or spell that targets creature you control` | "Whenever you cast a spell with {X} in its mana cost" (CR 107.3: the card's mana cost), "... that has an Adventure" (CR 715.2a; an Adventure spell on the stack doesn't), "... with a single target" (one target, or the same one for each "target", CR 115.3), "... that targets only a single creature you control" (that one target matches), "... that targets an opponent or a creature an opponent controls", "Whenever you cast an Equipment spell or a spell that targets a creature you control" (either side, each with its own targets). The targets are the spell's as it is cast (CR 601.2c). |
| `label "Valiant" triggered whenever this becomes targeted by you first time each turn: ...` | "Valiant — Whenever this creature becomes the target of a spell or ability you control for the first time each turn, ...": once a turn, the first time its controller's spell or ability targets it. |
| `when this becomes targeted` | becomes the target of a spell or ability |
| `whenever this becomes targeted` | "Whenever this creature becomes the target of a spell or ability" (`when` prints "When") |
| `whenever this becomes tapped`, `whenever this becomes untapped` | |
| `whenever one or more +1/+1 counters put on this` | "Whenever one or more +1/+1 counters are put on this creature" (CR 122.1): once per instruction putting counters of that kind on it (adapt, a spell), however many |
| `whenever +1/+1 counter removed from this`, `whenever counter removed from this` | "Whenever a +1/+1 counter is removed from this creature" (Protean Hydra), "Whenever a counter is removed from ...": once for each counter of that kind (any kind) removed from a matching permanent, however it goes: an effect, a cost, a prevention's follow-up, or a +1/+1 and a -1/-1 counter annihilating (CR 704.5q) |
| `when nth 4 plan counter put on this` | "When the fourth plan counter is put on this enchantment" (the Plans): counters of that kind put on the source take their number from below 4 to 4 or more, once. `put 1 +1/+1 counter on target creature you control and put 1 plan counter on this` prints "put a +1/+1 counter on target creature you control and a plan counter on this enchantment" |
| `whenever one or more creature card put into your graveyard from anywhere`, `whenever one or more creature you control become tapped` | "Whenever one or more creature cards are put into your graveyard from anywhere", "Whenever one or more creatures you control become tapped" (CR 603.2c): the per-object trigger batched, once for the matching objects of events before the next time a player would receive priority (a creature card dying with another, attackers tapped together); their number is `that much` |
| `whenever a card leaves your graveyard`, `whenever one or more players discard one or more cards`, `whenever you tap one or more untapped creature your opponents control`, `whenever you put a +1/+1 counter on other creature`, `whenever you put a counter on creature you control` | "Whenever a card leaves your graveyard": once for each card (not batched); "Whenever one or more players discard one or more cards": the cards every player discards together, once; "Whenever you tap one or more untapped creatures your opponents control": the batched form of `whenever you tap ...`; "Whenever you put a +1/+1 counter on another creature" is the older wording of `you put one or more ...` (the same trigger) |
| `whenever you put one or more +1/+1 counters on creature you control`, `whenever you put one or more -1/-1 counters on this`, `whenever you put one or more counters on Goblin, Orc, or Army you control` | "Whenever you put one or more +1/+1 counters on a creature you control" (CR 122.6): counters a spell or ability you control puts (or that a permanent you control enters with, or a cost you pay puts), once for each permanent they go on; without a kind, counters of any kind. `that much` / `that many` is how many were put |
| `whenever Aura becomes attached to this` | "Whenever an Aura becomes attached to this creature" (CR 701.3a; an Aura entering attached too); the object attached is the triggering object |
| `whenever enchanted land is tapped for mana: add {G}` | "Whenever enchanted land is tapped for mana, its controller adds an additional {G}." (`enchanted Forest` for an Aura with `enchant Forest`). A triggered mana ability (CR 605.1b): its effects may only add mana (`add any`, `add {G}{G}`, `add 2 mana any combination`, `add chosen`, `add {G} count each Elf on battlefield`), for the tapped permanent's controller, and it resolves at once when the permanent is tapped for mana by a mana ability (CR 605.4a), whether paying a cost or activated by hand. Payment counts the extra mana as part of what the permanent makes. `whenever land you control is tapped for mana: add {G}` prints "Whenever you tap a land for mana, add {G}." A filter with no controller, `whenever basic land is tapped for mana: add produced`, prints "Whenever a player taps a basic land for mana, that player adds one mana of any type that land produced." (Mana Flare): `add produced` (CR 106.12) adds one mana of a kind the tapped permanent just made, named after the filter's noun ("that land", "that artifact token": `whenever artifact token you control is tapped for mana: add produced`); payment counts it when the permanent makes one kind of mana. |
| `whenever Swamp you control is tapped for mana additional: add {B}`, `whenever Forest is tapped for mana passive: add {G}`, `whenever basic land is tapped for chosen passive: add chosen`, `whenever land you control is tapped for {C} additional: add {C}` | The same triggered mana ability, other wordings: `additional` prints "add an additional {B}" ("Whenever you tap a Swamp for mana, add an additional {B}.", Crypt Ghast; "that player adds an additional {U}" for a filter with no controller); `passive`, for a filter with no controller, "Whenever a Forest is tapped for mana, its controller adds an additional {G}." (Vernal Bloom; with `add produced`, "its controller adds one mana of any type that land produced"). `for chosen` ("for mana of the chosen color", with `add chosen` printed "one mana of that color": "Whenever a basic land is tapped for mana of the chosen color, its controller adds an additional one mana of that color.", Gauntlet of Power) and `for {C}` ("Whenever you tap a land for {C}") trigger only if the permanent made mana of that kind (the source's chosen color; payment foresees it when the permanent makes only that kind). A nonland noun loses its "non" word before "produced" ("Whenever you tap a nonland permanent for mana, add one mana of any type that permanent produced.", Kinnan). |
| `until end of turn, delayed this: whenever Island is tapped for mana additional, add {U}` | "Until end of turn, whenever a player taps an Island for mana, that player adds an additional {U}." (High Tide; `Mountain you control`: "whenever you tap a Mountain for mana, add an additional {R}", Molten Tide): a delayed triggered mana ability (CR 603.7c, 605.1b) made by a spell, its controller's ("you"), resolving at once as such a permanent is tapped for mana, until the turn ends; payment counts its mana as for a permanent's. Only `add` effects, about `this`, with the duration first. |

| `whenever this is dealt damage` | (enrage) |
| `whenever this deals damage`, `... deals damage to creature`, `... to player`, `... to opponent`, `... deals combat damage to creature` | the damaged creature is "that creature", the damaged player "that player"; `whenever this deals damage to Vampire`, `... to Goblin or Orc` (any filter): the damaged permanent, if it matches, is "that creature". On an Aura, the creature dealt damage by, or blocking or blocked by, the enchanted creature prints "the other creature" |
| `whenever you cast artifact spell`, `whenever a player casts Goblin spell`, `whenever an opponent casts multicolored spell`, `whenever you cast multicolored instant or sorcery spell from your hand` | a cast trigger with any filter; `from your hand` only a spell cast from there (Lilah, Undefeated Slickshot) |
| `whenever this becomes targeted first time each turn` | "Whenever this creature becomes the target of a spell or ability for the first time each turn": any controller's, once each turn; the spell or ability is `that spell` ("counter that spell or ability") |
| `whenever this becomes targeted by opponent` | "... becomes the target of a spell or ability an opponent controls" (ward's trigger) |
| `when enchanted creature becomes targeted by spell`, `whenever creature you control becomes targeted by spell`, `whenever permanent you control becomes targeted by opponent`, `... by opponent spell` | "When enchanted creature becomes the target of a spell" (a spell only, not an ability), "Whenever a permanent you control becomes the target of a spell or ability an opponent controls": any trigger subject; the targeted permanent is `that creature`, the spell's or ability's controller `that player` |
| `whenever you cast with mana value 4 or greater spell` | "Whenever you cast a spell with mana value 4 or greater" (a filter's "with" qualities print after "spell"; likewise `counter target with mana value 4 or less spell`) |
| `whenever you cast your first instant or sorcery spell each turn` | the first of the caster's instant and sorcery spells this turn (other spells cast before it don't count) |
| `whenever you cast your first spell each turn`, `... your second spell each turn`, `... a spell other than your first spell each turn`, `... your first spell during each opponent's turn`, `whenever an opponent casts their second spell each turn`, `whenever you cast a spell during an opponent's turn`, `whenever an opponent casts their first noncreature spell each turn`, `whenever you cast a spell from exile`, `whenever a player casts a spell from a graveyard`, `whenever you cast your third spell each turn` | the spell's place among those its caster cast this turn (or only that it was cast on another player's turn; the first noncreature one among their noncreature spells; or that it was cast from exile, or from any graveyard) |
| `when you cycle this` | "When you cycle this card" (CR 702.29d): triggers from the card itself as its cycling ability is activated, so it resolves before the draw; the card is "it" ("it deals 6 damage to any target") |
| `whenever you cycle a card`, `whenever you cycle another card`, `whenever you cycle or discard a card` | cycling a card (CR 702.29); `or discard` triggers once per card discarded, cycling included |
| `whenever this or Ally you control enters`, `whenever this or creature you control with power 4 or greater dies` | "Whenever this creature or another Ally you control enters"; the filter includes the source, and Oracle names the source again ("this creature") |
| `whenever one or more artifact you control enter` | once for all the matching objects entering together (CR 603.2c), counted as `that much`; objects entering before the next time a player gets priority count as together |
| `whenever one or more other creature you control die`, `whenever one or more permanent you control leave the battlefield` | the same batching for objects leaving the battlefield (dying), judged as they last existed (CR 603.10a) |
| `when you cast this`, `when you cast or cycle this` | "When you cast this spell" (CR 603.2): triggers from the spell itself as it is cast, controlled by its caster, so it resolves before the spell and even if the spell is countered; "When you cast or cycle Warped Tusker" (the card's name) adds the cycle trigger. `when you cast this and when this dies` is one ability with both |
| `whenever you sacrifice other Eldrazi` | "Whenever you sacrifice another Eldrazi" (CR 701.21a): the sacrificed permanent, matching as it is sacrificed, is the triggering object |
| `when this enters and whenever you cast ...`, `when you cycle this and when this dies` | one ability with two trigger events, joined with "and". A `while` condition after them is the second event's only: `when this enters and whenever this attacks while saddled` ("When this creature enters and whenever it attacks while saddled", CR 603.2). |
| `when this enters or dies`, `whenever this enters or attacks`, `when this enters or leaves the battlefield`, `when this enters or is turned face up` | one ability with two events of the source, the subject printed once: "When this creature enters or dies" ("When this artifact enters or is put into a graveyard from the battlefield" on a noncreature), "Whenever this creature enters or attacks"; the word written (`when` / `whenever`) is the word printed ("Whenever this creature enters or dies"); a filter's two events likewise: `whenever Detective you control enters or is turned face up` ("Whenever a Detective you control enters or is turned face up", each Detective the triggering object). A face-down permanent has no abilities, so one cast face down triggers only as it turns face up (CR 708.8) |
| `whenever you sacrifice other permanent`, `whenever you sacrifice token`, `whenever a player sacrifices creature`, `whenever an opponent sacrifices ...`, `when this enters and when you sacrifice this` | a permanent is sacrificed (CR 701.21) by its controller, the player named: "Whenever you sacrifice another permanent", "Whenever a player sacrifices a creature", "When this artifact enters and when you sacrifice it". It triggers as the permanent is sacrificed, judged while it is still on the battlefield (so a permanent sacrificing itself sees it), whether by an effect, a cost, or a mana ability (a Treasure); a creature that dies otherwise doesn't. The sacrificed permanent is the triggering object |
| `whenever permanent is turned face up`, `whenever this or permanent is turned face up`, `whenever Detective you control is turned face up` | "Whenever a permanent is turned face up", "Whenever this creature or another permanent is turned face up": the permanent judged as it is face up (CR 708.8) |
| `whenever you draw a card`, `whenever a player draws a card`, `whenever an opponent draws a card` | once per card drawn (CR 121.2); the card is the triggering object |
| `whenever a player discards a card`, `whenever you discard a card`, `whenever you discard creature card` | once per card discarded (CR 701.9), judged as it is discarded; the card is the triggering object |
| `whenever one or more cards leave your graveyard`, `whenever one or more creature cards leave your graveyard` | once for the matching cards that leave the controller's graveyard together, however they go (returned, exiled, cast with flashback; CR 603.2c); their number is `that much` |
| `whenever creature is put into your graveyard from battlefield` | "Whenever a creature is put into your graveyard from the battlefield": a matching permanent the controller owns dies, whoever controlled it, judged as it last existed on the battlefield; it is the triggering object |
| `whenever land card is put into your graveyard from anywhere` | "Whenever a land card is put into your graveyard from anywhere": a card the controller owns, matching as it is in the graveyard (milled, discarded, dying, ...) |
| `whenever creature enters from your graveyard` | "Whenever a creature enters from your graveyard": a permanent the controller owns entering from their graveyard is "it" |
| `when this enters from graveyard`, `whenever this or permanent enters from graveyard`, `whenever this or creature enters from your graveyard`, `whenever permanent you control enters from exile` | "When this creature enters from a graveyard" (any player's), "Whenever this creature or another permanent enters from a graveyard", "... enters from your graveyard" (one its controller owns), "Whenever a permanent you control enters from exile": a matching permanent put onto the battlefield from that zone (CR 400.7: returned by undying or persist, reanimated, flickered); one cast from there enters from the stack and doesn't count. It is the triggering object |
| `whenever a creature card leaves your graveyard` | "Whenever a creature card leaves your graveyard" (Along the Crooked Way): the per-card form of `one or more ... leave your graveyard` with any card filter, once for each matching card, however it leaves |
| `whenever a player draws their second card each turn` | any player's second card drawn in a turn (King T'Challa); the drawing player is `that player`. `you draw your second card each turn` and `an opponent draws their ...` are the controller's and the opponents' |
| `whenever you attack with 2 or more Spider` | "Whenever you attack with two or more Spiders": once as you declare attackers if at least that many matching creatures attack (CR 508.1m); `with 3 or more creatures` is the unfiltered form |
| `whenever this deals 4 or more damage` | "Whenever Spinneret and Spiderling deals 4 or more damage": one damage event of at least that much from the source (as it last existed, if it has left) to one creature, player, or other permanent; damage to different recipients at once doesn't add up. The recipient is `that creature` or `that player` |
| `whenever counter put on creature you control`, `whenever one or more counters put on creature you control` | "Whenever a counter is put on a creature you control", "Whenever one or more counters are put on ...": counters of any kind put on a matching permanent, by anyone; once per instruction putting counters on it (the first form is the older wording, the same trigger) |
| `whenever you expend 4` | "Whenever you expend 4" (CR 700.14): as the controller spends their fourth total mana to cast spells this turn, judged as the spell that spent it becomes cast (the mana counted as its cost's mana symbols) |
| `whenever andor other Villain or artifact you control enters` | "Whenever another Villain and/or artifact you control enters": `andor` before a filter with an alternative prints "and/or" in a trigger's subject, and between plural alternatives anywhere (`up to 2 target andor Mount or Vehicle`: "up to two target Mounts and/or Vehicles"; wording only); `andlist` likewise joins a singular list with "and" (`each andlist Pest or Bat or Spider you control`: "each Pest, Bat, and Spider you control"), and plural types (`andlist artifact or creature you control`: "artifacts and creatures you control"). In a count, `andor thatare creature or Vehicle you control` prints "permanents you control that are creatures and/or Vehicles" (Collision Course; `count andor thatare Fungus or Saproling creature you control`, alternatives of one creature type each, prints "creatures you control that are Fungi and/or Saprolings", The Mycotyrant; wording only) |

A trigger's subject prints its controller before its "with" qualities
("Whenever a creature you control with power 5 or greater enters").
`you may this has base power and toughness 4/2 until end of turn` prints
"you may have this creature's base power and toughness become 4/2 until
end of turn".
`you may deal 1 damage to any target` prints "you may have this creature
deal ..." and `you may each Ally creature you control gets +1/+0` "you may
have Ally creatures you control get +1/+0 ...".

Conditions (intervening "if", CR 603.4): `if its toughness was less than 1` (also `power`, `mana value`, and `was N or less` / `was N or greater`: the triggering object as it is or last existed, "Whenever a creature an opponent controls dies, if its toughness was less than 1, ..."), `if its power or toughness greater than this` (evolve's check, printed "if its power is greater than this creature's power or its toughness is greater than this creature's toughness"), `if it wasn't blocking` and `if it was blocking` (in a dies trigger, as it last existed), `if this is untapped`, `if
this is tapped` ("if this creature is tapped"),
`if no creatures are on the battlefield`, `if kicked` ("if it was kicked",
CR 702.33c, for a permanent with `kicker`), `if it had no -1/-1 counters`
("if it had no -1/-1 counters on it", judged as the source last existed),
`if it had +1/+1 counter` ("if it had a +1/+1 counter on it", the same way),
any static condition below (`if you control artifact`, `if attacked this
turn`),
`if no spells were cast last turn`, `if a player cast two or more spells
last turn`, `if this is damaged` ("if this creature was dealt damage this
turn", damage still marked on it), `if this isn't prepared` (CR 722.3a). After an attack or block trigger, `while <static condition>`
is a trigger condition instead (CR 603.2): "Whenever this creature attacks
while you control two or more artifacts, ..." (`triggered whenever this
attacks while you control 2 artifact: ...`, `while saddled`, `while
delirium`), judged only as the event happens, so the ability resolves even
if the condition no longer holds. Any trigger takes one (`whenever other
creature you control enters while this has 1 or more -1/-1 counters`:
"... enters while this creature has a -1/-1 counter on it"; with the
source not yet named, `whenever you cast a spell while this is attacking`
names it: "Whenever you cast a spell while Fire Lord Azula is
attacking"), and `during
your turn` is one too: "Whenever a land enters during your turn".

Static conditions: `7 cards in your graveyard` ("there are seven or more
cards in your graveyard"), `4 card types in your graveyard`, `you control 3
artifact` ("you control three or more artifacts"), `you control Island`
("you control an Island"), `you control 1 or more Egg` ("you control one
or more Eggs", the same condition), `your turn` ("it's your turn"), `not your turn` ("it's not your turn"), `not their turn` ("it's not their turn": the player the ability triggered for, or a spell's targeted player; "Whenever a player casts a spell, if it's not their turn, ..."), `attacker Rat` ("a Rat is attacking": any attacking creature that matches; `attacker creature at you`, "a creature is attacking you", attacking that player rather than a planeswalker or battle; not in a static ability),
`cast during main phase` ("you cast this spell during your main phase")
and `controlled Mount as cast` ("you controlled a Mount as you cast this
spell"), both judged as the spell is cast and remembered by it, `attacking`,
`enchanted`, `equipped` ("it's attacking", or "this creature is
attacking" when the subject is something else), `opponent life 10 or
less` ("an opponent has 10 or less life"), `your life 30 or more`,
`life above starting` ("your life total is greater than your starting
life total", CR 103.4: 20 here; the game's own in a configured game),
`x 10 or more` and `x 2 or less` ("X is 10 or more", CR 107.3: in a spell
or ability, `if x 5 or more, destroy each artifact`, also with `instead`,
its X as it resolves, the value its other X effects read; on a permanent,
X as it was cast, 0 if it wasn't),
`life at least starting` ("... greater than or equal to ..."), `life 10
above starting` ("your life total is at least 10 greater than ..."),
`life 7 more than starting` ("you have at least 7 life more than your
starting life total"), `life below starting` ("your life total is less
than ..."), `player life 13 or less` ("a player has 13 or less life"), `defending
player controls Island`, `you control no untapped land`, `you control
at most 2 other land` ("you control two or fewer other lands": the fast
lands), `you control Mountain or a Plains` ("you control a Mountain or a
Plains"), `you control 2 basic land`, `any player controls black
permanent` ("any player controls a black permanent"), `you control
other Goblin` ("you control another Goblin"), `team controls other
Warrior` ("your team controls another Warrior": outside a team game a
player's team is that player, CR 810.1, so it is `you control`), `you control Urza's Mine and
Urza's Power-Plant` ("you control an Urza's Mine and an Urza's
Power-Plant": one of each), `attacked this turn` (raid),
`top card is Goblin` ("the top card of your library is a Goblin
card"; `top card is black`: "... is black"; by the card's printed
characteristics, checked again whenever the top card changes),
`creature died this turn` (morbid), `hand 0 or less` ("you have no cards
in hand"), `hand 1 or less` ("you have one or fewer cards in hand"),
`hand has no land card` ("you have no land cards in hand"), and
the shorthands `threshold`, `delirium`, `metalcraft`; also `land entered
this turn` (landfall: "you had a land enter the battlefield under your
control this turn") and `drew 2 cards this turn` ("you've drawn two or
more cards this turn").
Another player's hand: `that player hand 2 or less` ("that player has
two or fewer cards in hand"; `or more` for "two or more"), read for the
player the ability triggered for (at the beginning of each opponent's
upkeep: the active player) or, in a spell, its targeted player;
`defending player hand 2 or less` ("defending player has two or fewer
cards in hand", the player the source is attacking); `opponent hand 0 or
less` ("an opponent has no cards in hand", any one opponent; also in a
static ability, Guul Draz Specter); `any player hand 0 or less` ("a
player has no cards in hand", Lupine Prototype); `that player more
cards in hand than you` ("then if that player has more cards in hand than
you", Pulse of the Dross); `library empty` ("your library has no cards in
it", Stillness in Motion). As intervening "if" clauses they are judged as
the ability triggers and again as it resolves (CR 603.4).
in hand"), `hand 1 or less` ("you have one or fewer cards in hand"),
`2 instant or sorcery cards in your graveyard` ("there are two or more
instant and/or sorcery cards in your graveyard"; any card filter, `4 permanent
cards in your graveyard` for descend), `you
have 4 creature cards in your graveyard`, `there is land card in your
graveyard` ("there is a land card in your graveyard"; `there's Lesson card
in your graveyard` prints "there's a"), `1 artifact card is in your
graveyard` ("an artifact card is in your graveyard"), `you control Desert
or there is Desert card in your graveyard` (either holds; any two
conditions: `either you control land creature else entered land this
turn`, "you control a land creature or a land entered the battlefield
under your control this turn", `else` where a filter's own `or` would run
on), `5 mana values in your
graveyard`, `opponent graveyard 8 or more` ("an opponent has eight or more
cards in their graveyard"), `opponent controls Human`, `this is untapped`,
`this is Wall` ("it's untapped", "it's a Wall"), `modified` ("it's
modified", CR 700.9), `attacking alone`, `blocked`, `being blocked` ("at
least one creature is blocking this creature"), `attacked or blocked this
combat` ("this creature attacked or blocked this combat", in an "at end
of combat" trigger; one removed from combat, or whose attacker was, still
counts; "it" follows), `attacked total power 6` ("you attacked with
creatures with total power 6 or greater this combat", pack tactics: the
total power of your attackers as they were declared), `attached is legendary`
("equipped creature is legendary"; "enchanted" on an Aura), `this is with
power 5 or greater` ("this creature's power is 5 or greater"), `this has 1
or more +1/+1 counters` ("this creature has a +1/+1 counter on it"),
`artifact entered this turn` ("an artifact entered the battlefield under
your control this turn"), `drew 2 this
turn` ("you've drawn two or more cards this turn"), `cast instant or sorcery
spell this turn`, `cast 2 spell this turn` ("you've cast two or more
spells this turn"), `cast another blue spell this turn` ("you've cast
another blue spell this turn": the source's own cast, as a spell or as the
permanent it became, doesn't count), `cast from hand` ("you cast it from
your hand", kept by the permanent the spell becomes), `cast from
graveyard` ("this spell was cast from a graveyard"; CR 601.2a), `you cast
it` ("When this creature enters, if you cast it, ...": cast from any zone,
remembered by the permanent its spell became; not when put onto the
battlefield otherwise; in a trigger with a triggering object, "it" is that
object: `whenever creature you control enters if you cast it`, The Sibsig
Ceremony; `cast from hand` and `cast from graveyard` likewise), `that
creature entered from graveyard or you cast it from graveyard` ("if that
creature entered from a graveyard or you cast it from a graveyard": the
triggering permanent was put onto the battlefield from a graveyard, or
was a spell cast from one; `they entered or were cast from graveyard`
prints "they entered or were cast from a graveyard", `one or more of them
entered from exile or was cast from exile` "one or more of them entered
from exile or was cast from exile"; `exile` for exile. In a batched
trigger, "one or more ... enter", each permanent is judged as it enters,
and the first that qualifies is the triggering object, `one of them`
("create a token that's a copy of one of them", Twilight Diviner: the
engine picks it), `gained
life this turn`, `gained or lost life this turn`, `lost life this turn`
("you've lost life this turn"), `gained and lost life this turn` ("you
gained and lost life this turn": both), `descended this turn`
("you descended this turn", CR 700.11: a permanent card was put into your
graveyard from anywhere), `surveilled this turn` ("you've surveilled this
turn"), `scried or surveilled this turn` ("you've scried or surveilled this
turn"; scry 0 and surveil 0 are neither, CR 701.22b, 701.25c), `2
creatures entered this turn` ("two or more creatures entered the
battlefield under your control this turn"), `opponent had 2 creatures
enter this turn` ("an opponent had two or more creatures enter the
battlefield under their control this turn"), `counter put this turn` ("a +1/+1 counter was put on a permanent
under your control this turn"), `opponent dealt noncombat damage this turn` ("an opponent has been dealt
noncombat damage this turn"; `opponent was dealt ...`: "an opponent was
dealt ..."), `opponent was dealt 7 damage this turn` ("an opponent was dealt 7 or more
damage this turn", any one opponent's total), `no cards in any hand` ("each
player has no cards in hand"), `library 20 or less` ("a library has twenty
or fewer cards in it": any player's), `resolution 2` ("this is the second time
this ability has resolved this turn": the resolving ability's source has
resolved it that many times this turn, this time included; `resolution 2
again` prints "it's the second time", for "you gain 1 life if resolution
1; if resolution 2 again, draw 1 card"), `put counter this turn` ("you've put
one or more +1/+1 counters on a creature this turn": a spell or ability
you control put them, CR 122.6), `face-down creature entered this turn`
("a face-down creature entered the battlefield under your control this
turn", CR 708.3), `another creature entered this turn`
("another creature entered the battlefield under your control this
turn"; `creature entered this turn` without "another"), `entered
another Knight this turn` ("another Knight entered the battlefield under
your control this turn": any filter, judged as the permanent is or as it
last existed, so one that has since left still counts; `had another
creature enter this turn` prints "you had another creature enter the
battlefield under your control this turn"), `2 nonland
entered this turn` (celebration: "two or more nonland permanents entered
the battlefield under your control this turn"), `artifact or creature to
graveyard this turn` ("an artifact or creature was put into a graveyard
from the battlefield this turn"; any card types joined by `or`;
`permanent to graveyard this turn`, "a permanent was put into a graveyard
from the battlefield this turn", any permanent),
`discarded this turn`, `opponent controls more creatures`, `opponent has more life`, `opponent
has more cards in hand` ("an opponent controls more creatures than you",
...), `not cast this turn` ("you haven't cast a
spell this turn"; `didn't cast this turn` prints "you didn't cast a spell
this turn"), `not cast from hand this turn` ("you haven't cast a
spell from your hand this turn": a spell cast from a graveyard or exile
doesn't count), `more cards in hand than each
opponent`, `opponent poisoned` ("an opponent is poisoned"), `opponent poison
3 or more`, `you own card in exile`, `3 cards exiled with this` ("there
are three or more cards exiled with this creature", CR 607.2a), `4 card
types exiled with this` ("there are four or more card types among cards
exiled with this creature"), `total power 8 or more` ("creatures you
control have total power 8 or greater", formidable), `total toughness 10
or more` ("creatures you control have total toughness 10 or greater"), `3
different powers` ("you control three or more creatures with different
powers", coven), `2 creatures died this turn` ("two or more creatures died
this turn"), `creature died under your control this turn` (as they last
existed, CR 700.4), `opponent controls more lands` ("an opponent controls
more lands than you"), `void` ("a nonland permanent left the battlefield
this turn or a spell was warped this turn", any player's; usually with
`label "Void"`), `not your main phase` ("it isn't your main phase"), `cast
from anywhere other than hand` ("this spell was cast from anywhere other
than your hand": cast, and not from the hand; a permanent remembers it from
its spell), `not cast from hand` ("this spell wasn't cast from your hand",
Twinned Vision: a copy, never cast, counts too), `put any counter this turn` ("you put a counter on a creature
this turn": any kind, by a spell or ability you controlled or a cost you
paid), `attacked with 3 Merfolk this turn` ("you attacked with three or
more Merfolk this turn"), `cast both creature spell and noncreature spell
this turn` ("you've cast both a creature spell and a noncreature spell
this turn"), the parts of the turn under Costs, and the shorthands
`threshold`, `delirium`, `metalcraft`. Two conditions joined by `and` must
both hold (`if there is Elf card in your graveyard and this has 1 or more
-1/-1 counters`: "if there is an Elf card in your graveyard and this
creature has a -1/-1 counter on it"); the `and` joins only when a condition
follows it. A conditional static ability that pumps what an earlier
static ability of the card pumps prints "gets an additional +2/+2".
in hand"), `hand 1 or less` ("you have one or fewer cards in hand"),
`opponent poison 3 or more` ("an opponent has three or more poison
counters", corrupted), `this has 4 or more oil counters` ("this creature
has four or more oil counters on it"; first, "As long as ..., it gets
..."; `this has 3 or more counters`: "this creature has three or more
counters on it", every kind together; `this has even counters`: "it has
an even number of counters on it" (zero is even), `this has odd counters`
"an odd number", every kind together (Sab-Sunen: `can't attack or block
unless this has even counters` prints "Sab-Sunen can't attack or block
unless it has an even number of counters on it."); `this has 1 or more +1/+1 counters` prints "a +1/+1 counter on it", `this has one or more +1/+1 counters` "one or more +1/+1 counters on it"), `this has no arrowhead counters` ("there are no arrowhead counters
on this artifact"), `this doesn't have flying counter` (the same
condition, "this creature doesn't have a flying counter on it"; after it
the source is "it": "..., put a flying counter on it"), `there are 3 or more brick counters on this` ("there
are three or more brick counters on this artifact"), and the shorthands `threshold`, `delirium`,
`metalcraft`.

### Replacement events and outcomes

Counter kinds: `+1/+1`, `-1/-1`, the other power/toughness counters
`+1/+0`, `+0/+1`, `+0/+2`, `+1/+2`, `+2/+2`, `-0/-1`, `-0/-2`, `-1/-0`,
`-2/-1`, `-2/-2` (CR 122.1a: each changes power and toughness in layer
7c; only +1/+1 and -1/-1 counters annihilate, CR 704.5q), `loyalty`,
`stun` (CR 122.1d: removed instead of untapping), `charge`, `oil`,
`spore`, `depletion`, `quest`, `ki`, `time`, `tide`, `shield`, `age`,
`fade`, `ice`, `level`, `lore`, `page`, `arrowhead`, `javelin`, `polyp`,
`healing`, `mining`, `scream`, `credit`, `brick`, `currency`, `sleep`,
`net`, `flood`, `glyph`, `crystal`, `unlock`, `loot`, `supply`, `incubation`, `divinity`, `plan`, `storage`, `verse`, `soul`, `blood`, `slime`, `stash`, `ingenuity`, `blaze`, `aim`, `void`, `omen`, `memory`, `luck`, `hour`, `growth`, `fire`, `fate`, `devotion`, `collection`, `point`, `carrion`, `landmark`, `nest`, `bloodstain`, `invasion`, `skewer`, `revival`, `rev`, `impostor`, `fellowship`, `dream`, `dread`, `coin`, `chorus`, `blight` (none with rules of its own), `hone` (each hone counter on an Equipment gives the creature it is attached to +1/+0, layer 7c: Dwalin, Weaponmaster), `decayed` (a keyword counter, CR 122.1b), `finality` (CR
122.1h: a permanent with one that would be put into a graveyard from the
battlefield is exiled instead, so it doesn't die; `return target creature
card from your graveyard to battlefield with 1 finality counter` prints
"... with a finality counter on it"; `with 2 additional +1/+1 counters`
prints "with two additional +1/+1 counters on it", the same counters;
`with 1 hexproof counter and 1 indestructible counter` "with a hexproof
counter and an indestructible counter on it"; in a delayed return,
`return that card to battlefield with 1 additional +1/+1 counter if
creature at beginning of the next end step` prints "Return that card to
the battlefield under its owner's control at the beginning of the next end
step. If it enters as a creature, it enters with an additional +1/+1
counter on it." (judged by its card's types)), and the keyword counters `flying`, `first strike`, `trample`,
`lifelink`, `deathtouch`, `reach`, `menace`, `vigilance`, `hexproof`,
`indestructible`, `double strike` (CR 122.1b: the permanent has the
keyword; `double_strike` in scenarios). A cost may
`remove 1 charge counter from this`, `remove 1 counter from this` ("Remove
a counter from this creature": any kind; the engine removes -1/-1 counters
first, then kinds that don't change power or toughness, then the rest),
`remove 1 +1/+1 counter from creature you control` ("Remove a +1/+1
counter from a creature you control": the engine takes it from the
creature with the most such counters, the source included),
`remove X ki counters from this` ("Remove X ki counters from this
creature": X is the ability's, announced as it is activated, at most the
counters of that kind on the source; with {X} in the mana cost too, the
same X; not in a mana ability; `remove any number +1/+1 counters from
this` prints "Remove any number of +1/+1 counters from ~", the same X,
and the first X in its effects prints "that many": `create X "1/1 green
Insect creature"` is "Create that many 1/1 green Insect creature
tokens."), or
`put 1 -1/-1 counter on this`
("Put a -1/-1 counter on this creature", CR 118.3: put on as the cost is
paid, so a creature it kills is gone before the ability resolves; a mana
ability with it pays for spells too).

Counters on a group of targets: `put 1 +1/+1 counter on up to 2 target
creature` ("put a +1/+1 counter on each of up to two target creatures";
`1 to 2 target creature` is "one or two", `X target creature` "X",
`up to 2 another target creature you control` "up to two other target
creatures you control"; `untap 2 another target permanent` is "Untap two
other target permanents"), `... and put 1 stun counter on them` ("on each
of them"), and `put 1 stun counter on those targets` ("on each of those
creatures": every target declared so far, named by their kind).

`this becomes 4/4 green Bear artifact creature` (colors before
subtypes; the card types replace the permanent's others except land, CR
205.1a-b, and a creature type replaces its creature types: "becomes a
Dragon ... (It loses all other creature types.)"), `this becomes
Shapeshifter artifact creature base 5/5` ("... with base power and
toughness 5/5"), `this becomes artifact creature` (no new P/T), `this
becomes 2/2 Gargoyle creature with flying indefinitely` (keywords after
`with`; `indefinitely`: no duration, Opal Caryatid), `this becomes 2/4 Wizard creature with ability { triggered whenever you cast instant or sorcery spell: this gets +1/+0 } indefinitely` ("it becomes a 2/4 Wizard creature with \"Whenever ...\" It's still a land.", Great Hall of the Biblioplex; `with flying and ability { ... }`: "with flying and \"This creature gets ...\"", Iron Man Armor: it gains the ability for as long as the rest lasts), `this becomes 3/3 creature with vigilance and all creature types indefinitely` (Soulstone Sanctuary: every creature type, CR 205.3m; `with all creature types` without keywords), `until end of turn,
target creature becomes Dragon base 4/4 and target 1 gains flying`;
`return this to battlefield; it is now enchantment` prints "return it to
the battlefield under its owner's control. It's an enchantment." (the
Enduring cycle: its card types become those indefinitely, and a permanent
that stops being a creature loses its creature types, CR 205.1a; with
`pronoun they`, "return them to the battlefield. They're an artifact.");
in a `when this dies` trigger, `if this is creature` prints "if it was a
creature" (as it last existed); a trigger condition `if this is enchantment` prints "if this permanent is
an enchantment" and the source is "it" after it, `target creature becomes blue`, `this becomes colorless`, `that creature becomes green indefinitely` (no duration: "That creature becomes green."),
`target land becomes Island` (all until end of turn), `this becomes all
colors` (CR 105.2). Colors before a creature type make an animation:
`until end of turn, target creature becomes blue Serpent base 5/5` ("...
becomes a blue Serpent with base power and toughness 5/5"; the color and
creature type replace its others, layers 4, 5, and 7b), `loses all
abilities and becomes green Snake base 1/1` ("loses all abilities and
becomes a green Snake ...", layer 6 too), `target creature loses all
abilities and has base power and toughness 0/1` (Humble: one effect,
layers 6 and 7b), `up to 1 another target creature loses all abilities
indefinitely` (no duration; without `indefinitely`, until end of turn),
and `in addition` adds instead of
replacing: `it becomes black Demon in addition indefinitely` ("it becomes
a black Demon in addition to its other colors and types"), `up to 2
target creature each has base power and toughness 3/3, gains flying, and
becomes blue Illusion in addition` ("... each have base power and
toughness 3/3, gain flying, and become blue Illusions in addition to
their other colors and types"). A number in place of N/N sets both:
`each creature you control has base power and toughness X` prints
"Creatures you control have base power and toughness X/X until end of
turn." (X fixed as the effect resolves, CR 608.2h). Chosen as the
effect resolves (CR 608.2d), once for all its recipients: `target creature
becomes color of choice`, `target creature gains protection from color of
choice`, `target land becomes land type of choice` ("the basic land type of
your choice"), `target land becomes Plains or Island` (one of those),
`enchanted creature becomes creature type of choice` (it loses its other
creature types, CR 205.1b) and `target creature becomes creature type of
choice except Wall` ("Choose a creature type other than Wall. Target
creature becomes that type until end of turn."), `target creature becomes
colors of choice` ("the color or colors of your choice": one color at a
time, then `done`), all until end of turn. A player gains keywords too
(CR 702.18c, 702.11c, 702.16): `you gains shroud` ("You gain shroud until
end of turn."), `you gains hexproof until your next turn`, `you gains
protection from color of choice` (spells and abilities from sources of
that color can't target the player, and damage from them is prevented);
other keywords do nothing on a player. A choice may also be its own
sentence, used by what follows in the same resolution (CR 608.2d):
`choose creature type` ("Choose a creature type."), `choose color`,
`choose land type` ("Choose a basic land type."), `choose creature type
except Wall`, `choose card name` ("Choose a card name.", CR 201.3; the
names offered are those of the cards in the game, as for `as this enters
choose card name`), `choose nonland card name`; after a card name, `exile
up to 4 with chosen name from zones of target opponent` ("Search target
opponent's graveyard, hand, and library for up to four cards with that
name and exile them. Then that player shuffles."; `any number` "any
number of cards", `all` "all cards": the engine finds every one it may,
from the hand first, then the library, then the graveyard, up to the
limit; CR 701.19b; `then draws for each from hand`: "That player
shuffles, then draws a card for each card exiled from their hand this
way."; `then creates "2/2 black Zombie creature" for each from hand`:
"... creates a 2/2 black Zombie creature token for each card exiled from
their hand this way."), `choose creature card name` ("Choose a creature
card name."), `choose card name except basic land` ("Choose a card name
other than a basic land card name."); after a card name, `ban chosen
name until your next turn` ("Until your next turn, spells with the
chosen name can't be cast", CR 611.2b: by any player, until the
controller's next turn begins; `and lands`: "... and lands with the
chosen name can't be played"; `your opponents`: "Until your next turn,
your opponents can't cast spells with the chosen name."; `opponents
late`: "Opponents can't cast spells with the chosen name until your next
turn."), `that player discards 1 with that name card` ("That player
discards a card with that name.", the hand revealed; the filter `with
that name` / `with chosen name` is "with that name" / "with the chosen
name", the name chosen earlier in the resolution) then `if they can't,
you draw 1 card` ("If they can't, you draw a card.": once if that player
had no such card), `<who> reveals hand and discards all with that name
card` ("... reveals their hand and discards all cards with that name");
`choose number between 0 and 10` ("Choose a number
between 0 and 10.", `choose "7"` in scenarios; unscripted, the least;
`choose number`: "Choose a number.", any number, 0 to 20 offered),
read by the count `chosen number` ("the chosen number": `destroy all
creature with power at least count equal chosen number`, "Destroy all
creatures with power greater than or equal to the chosen number."; `with
power at least <count>` is the filter "with power greater than or equal
to ..."); then `each land you control becomes that type` ("Each land
you control becomes that type until end of turn"), `each creature you
control gains protection from chosen color` (until end of turn), and
filters `of that type` / `of chosen type` / `of chosen color` (`draw count
each permanent you control of that type cards`). A filter `of creature
type of choice` or `of color of choice` makes the choice itself as its
effect resolves: `each creature of creature type of choice gets +0/+4`
("Creatures of the creature type of your choice get +0/+4 until end of
turn"), `destroy each enchantment of color of choice`. `target land
becomes Forest indefinitely` has no duration ("Target land becomes a
Forest.").
`this spell costs {3} less if creature died this turn` (any static
condition): "This spell costs {3} less to cast if a creature died this
turn." (printed first, before keywords).
`this spell costs {3} less if it targets tapped creature` (or `more`):
"This spell costs {3} less to cast if it targets a tapped creature." Its
targets are then held to ones the caster can pay for. `if it targets
spell creature` is "a creature spell", `if it targets card creature with
mana value 3 or less` "a creature card with mana value 3 or less", and
`if it targets attacking or tapped creature` "an attacking or tapped
creature". `this spell costs {1}{U} less if it targets spell instant or
sorcery` ("This spell costs {1}{U} less to cast if it targets an instant
or sorcery spell.") also takes one mana of each colored symbol off (CR
118.7b: generic mana where the cost has none of that color); only a
reduction by a target may have colored symbols. More conditions: `night` ("it's night", CR 731), `gained 3
life this turn` ("you've gained 3 or more life this turn", in all),
`committed crime this turn` ("you've committed a crime this turn", CR
700.13: a spell or ability you control targeted an opponent, a permanent
or spell an opponent controls, or a card in an opponent's graveyard),
`card left your graveyard this turn` ("one or more cards left your
graveyard this turn"), `sacrificed artifact this turn` ("you've
sacrificed an artifact this turn"; `sacrificed permanent this turn`, "you
sacrificed a permanent this turn", any permanent), and `creature card to your graveyard this turn` ("a
creature card was put into your graveyard from anywhere this turn").
A number before a turn-log phrase is "N or more" of it (`Tally`; the
same words without the number are a count, `count where times descended
this turn`, `count each lands entered this turn`): `3 cards left your
graveyard this turn` ("three or more cards left your graveyard this
turn"; a count, "cards that left your graveyard this turn"), `2 times
descended this turn` ("you descended two or more times this turn"; "the
number of times you descended this turn", CR 700.11), `2 lands entered
this turn` ("lands that entered the battlefield under your control this
turn"), `1 cards exiled this turn` ("one or more cards were put into
exile this turn", any player's cards; "cards that were put into exile
this turn"), `3 creature cards to graveyards this turn` ("three or more
creature cards were put into graveyards from anywhere this turn"),
`1 creatures exiled under opponents this turn` (from the battlefield, as
they last existed; as a count, "the number of creatures that were exiled
under your opponents' control this turn", all opponents' together),
`3 sources dealt damage this turn` ("three or more sources you
controlled dealt damage this turn", each source once, by its controller
as it dealt damage), `1 loyalty abilities activated this turn` ("you've
activated a loyalty ability this turn"; with 2 or more, "you've
activated two or more loyalty abilities this turn"), and `3 creatures
attacked this turn` ("three or more creatures attacked this turn", any
player's; only a condition, the count being `creature attacked this
turn`), and `1 opponent dealt combat damage by legendary creature this
turn` ("an opponent was dealt combat damage by a legendary creature this
turn"). About the source (negated with `not` after `this`): `this plus
counters put this turn` ("you've put one or more +1/+1 counters on Beast
this turn"; "As long as ..., he has flying" with the source as subject),
`this counters put this turn` ("you put a counter on this creature this
turn", any kind), both by the ability's controller; `this not dealt
damage` ("it hasn't dealt damage yet", since it became this object),
`this not dealt combat damage` ("they haven't dealt combat damage yet"
with `pronoun they`); `this not entered this turn` ("this creature didn't
enter the battlefield this turn"; "it" after); `this attacked this combat`
("Tolsimir attacked this combat"). `early turn`: "it's your first,
second, or third turn of the game" (CR 500.1; a set-up position counts
the turns as if the players had taken them in order from turn 1);
`opponent dealt noncombat damage last turn`: "an opponent was dealt
noncombat damage last turn". A filter's `with counters put this turn`
prints "that you've put one or more +1/+1 counters on this turn" (by the
permanent's controller: "Each creature you control that ..."), and a
graveyard card target's `... from a graveyard put there this turn`
(`put there from anywhere this turn`) prints "... from a graveyard that
was put there this turn" (Night Nurse: "target permanent card in your
graveyard that was put there from anywhere this turn"). A
count `differently named land you control`: "the number of differently
named lands you control" (`this spell costs {1} less count where
differently named land you control`).
`during your turn replace damage to this: prevent` prints "During your
turn, prevent all damage that would be dealt to this creature.": the
replacement or prevention effect applies only during its controller's
turn.
`until end of turn, this has base power 4 and gains trample` (CR
613.4b: the power only, in layer 7b; the toughness stays) and `this base
power becomes count equal Town you control` ("this creature's base power
becomes equal to the number of Towns you control", fixed as it resolves),
both until end of turn.
`triggered when this enters if he was cast: ...` prints "When Anti-Venom
enters, if he was cast, ..." (`it was cast`, `she was cast`, `they were
cast` are the same condition, printed by the card's pronoun): the
condition of `you cast it`.
`this spell can't be countered`: "This spell can't be countered." (CR
101.2: a spell or ability that would counter it, ward included, does
nothing to it). `if x 5 or more, this spell can't be countered and the
damage can't be prevented` prints "If X is 5 or more, this spell can't be
countered and the damage can't be prevented." (Banefire; any static
condition, `label "Hellbent" if hand 0 or less, ...`; without the `and`
clause, "this spell can't be countered" only): the condition is judged
for the spell's controller when an effect would counter it, and the
damage clause as the spell resolves (CR 615.12). Triggers `when
this leaves the battlefield`, `whenever this attacks or blocks`.
In a triggered ability, `it` is the triggering object printed "it"
("Whenever another creature you control attacks, it gets +0/+1 until end
of turn."); `that creature` prints "that creature". After `whenever
this deals combat damage` (or `deals damage`) with no recipient, `it` is
the source ("Whenever this creature deals combat damage, untap it").
`if target 1 was with mana value 3 or less, <sentence>` prints "If its
mana value was 3 or less, ..." and `if target 1 had with mana value 3
or less, ...` "If it had mana value 3 or less, ..." (as it last existed).
After a sentence, `... if target 1 is legendary` checks the target as
the sentence resolves: "Put a +1/+1 counter on target creature you
control if it's legendary.", "... if its power is 4 or greater", "...
if it has mana value 3 or less"; `... if that target 1 is with +1/+1
counter` prints "Draw a card if that creature has a +1/+1 counter on it."
(a target's counters are judged on the battlefield).
`otherwise, <sentence>` after `if <condition>, ...` or after `clash; if
you win, ...` happens only if that didn't (CR 608.2c): "Otherwise, that
player discards a card."; after `you may <act>; if you do, ...` it
happens if the act wasn't taken (an optional act that can't be done in
full, `you may tap 2 untapped creature you control chosen`, isn't
offered); creating the token the sentence before
created prints "Otherwise, create one of those tokens."
`<clause> unless that player discards 1 card` (or `unless they discard
1 card`): the payer may discard a card of their choice to stop it.
`if target 1 was <filter>, <sentence>`: "If that creature was a Human,
...", "If that creature was green or white, ...", "If that land was
nonbasic, ..." (the target as it last existed); a state alone prints "it":
`if target 1 was attacking` ("If it was attacking"), `if target 1 was
tapped` ("If it was tapped"), judged as it last existed on the
battlefield, so a target returned to its owner's hand still counts. Without `instead`, `if
target 1 is <filter>, <sentence>` is "If it's a Vampire, ..." and `if
that target 1 is <filter>, ...` is "If that creature is a Goat, ..." (the
target as it resolves, or last existed); `also` after the comma prints
"it also gains lifelink". Supertypes alone print bare ("If it's
legendary"), a keyword prints "has" ("If that creature has infect"), and
a graveyard card target adds "card" ("If it's a Zombie card").
`if target 1 wasnt attacking, ...` prints "If it wasn't attacking, ..."
and `if that target 1 wasnt damaged, ...` "If that creature wasn't dealt
damage this turn, ..." (the sentence happens if the target doesn't match,
as it last existed). Inside `if you controlled it, ...`, `return it to
battlefield tapped under your control` prints "return it to the
battlefield tapped" (Unyielding Gatekeeper).
In a triggered ability with a triggering object and no target named yet,
`if it is <filter>, ...`, `if it isnt ...`, `if it was ...`, `if it wasnt
...` and `if that <noun> is ...` (`creature`, `land`, `spell`,
`permanent`, `artifact`, `card`; also `was`) check the triggering object,
as it is or as it last existed (CR 608.2h): "If it's a Spider, put a
+1/+1 counter on it." (Aunt May), "If that land is a Mountain, add {R}."
(Koth, the Geomancer), "If that spell is a Lesson, ...". Written after the
sentence, `draw 1 card if it was attacking` prints "draw a card if it was
attacking"; `otherwise, ...` may follow either (Garna, Bloodfist of Keld).
After a reveal or a look at the top card, `if it is ...` is about that
card instead.
Trigger `whenever you draw your second card each turn` (or `an opponent
draws their second card each turn`); `card in a turn` prints "When you
draw your third card in a turn".
Steps: `at beginning of your combat` ("At the beginning of combat on your
turn"), `your first main`, `your second main` ("... your second main
phase"). Recipient `controller of that target 1` ("that artifact's
controller"; `controller of the target 1` is "the creature's controller";
after "target player or planeswalker" it is "that player or that
planeswalker's controller", the player or the planeswalker's controller,
Blightning; after the target is exiled, "that permanent's controller",
its last controller, Structural Distortion); `controller of enchanted` ("enchanted creature's
controller": `controller of enchanted sacrifices attached` prints
"enchanted creature's controller sacrifices it"); and `each other player` ("each other player": the opponents).

Mass and per-player effects. `deal 1 damage to each opponent and each
creature or planeswalker they control` ("... each opponent and each
creature and planeswalker they control"): `they control` after the
opponents is the opponents' control. `deal 3 damage to each creature with
flying and each player`. `each green creature or white creature can't
block` ("Green creatures and white creatures can't block this turn").
Each player named acts on their own, in turn order (CR 101.4): `each player
returns 1 creature they control` ("Each player returns a creature they
control to its owner's hand"), `each player may scry 1`, `then each
opponent may scry 1` (`scries` without `may`; declining to scry leaves the
library as keeping every card on top does; `you scry 2` prints an explicit
"you scry 2"), `each player may search basic
land card reveal to hand`, `each player puts 1 creature card from
graveyard onto battlefield` ("... from their graveyard onto the
battlefield": each chooses, then the cards enter together). `each opponent
who doesn't control Elf loses 1 life` ("each opponent who doesn't control
an Elf", judged as it resolves). After a discard, `for each opponent who
can't, you gain 4 life` ("For each opponent who can't, ..."): once for each
player who had too few cards to discard. Halves counted for each player
as the effect happens for them (CR 107.1a), written right after the verb:
`target opponent sacrifices half up creature` ("sacrifices half the
creatures they control of their choice, rounded up"; any filter, without
who controls it: `each player sacrifices half down non-God creature`),
`discards half up hand` ("discards half the cards in their hand, rounded
up"), `mills half down library` ("mills half their library, rounded
down"; `any number of target players each mills half down library`: each
their own), `loses half down life` ("loses half their life, rounded
down"; `loses half life` is rounded up). A sentence with two or more
halves, all rounded down, prints them bare and adds "Round down each
time." (Pox Plague: `each player loses half down life, then discards half
down hand, then sacrifices half down permanent`). The counts `their
hand`, `their library`, `their life` are the acting player's, and `they
control` in a count of `each player` or of a group of target players
(`each player loses count each creature they control life`) is each
player's own, for life loss, draws, mills, discards, and sacrifices.
`each opponent may discard 1 card or sacrifice permanent` ("Each opponent
may discard a card or sacrifice a permanent of their choice.", CR 608.2d;
`may sacrifice nonland permanent or discard 1 card` in the other order):
each in turn chooses (in a scenario `choose "1"` sacrifices, `"2"`
discards, `"3"` does neither). After it, after `each player may discard 1
card`, after a discard by each player, or after a counted sacrifice, the
recipient `each opponent who didn't` (`each player who doesn't`, `each
opponent who can't`; `each opponent who didn't sacrifice or discard`:
"... who didn't sacrifice a permanent or discard a card this way") is
those who didn't or couldn't: `this deals 3 damage to each opponent who
didn't`, `each opponent who can't loses 3 life` (after `each opponent
discards 1 card`), `each opponent who can't discards 1 card` (after `each
opponent sacrifices 1 creature or Vehicle`). The count `opponents didn't
discard 2 nonland card` ("for each opponent who didn't discard two
nonland cards this way") counts the players the last discard asked who
discarded fewer such cards; `those opponents didn't discard 1 card with
mana value 4 or greater` prints "For each of those opponents who didn't
discard a card with mana value 4 or greater" (after `any number of
target opponents each discards 1 card`). `target player can't play lands
this turn` and `target player can't cast creature spells this turn` (CR
614.17: until end of turn, the player isn't offered land plays or creature
spells, face-down ones included). `creature dealt damage this way can't be
regenerated` ("A creature dealt damage this way can't be regenerated this
turn": each permanent the resolution has dealt damage to so far).
`you create 1 "..."`
after `and` or `if they do,` prints "and you create a ... token".
Cost item `2 times per turn`: "Activate no more than twice each turn."
`that much` is the triggering event's amount ("Whenever this creature is
dealt damage, you gain that much life": `you gain that much life`).
Trigger `whenever this is dealt combat damage` (CR 510.2) counts only
combat damage. In a triggered ability `it` is the triggering object, but
after a target the ability names (other than a player) it is that target,
as in a spell: `put 2 +1/+1 counters on target creature you control; it
gains vigilance until end of turn`, `you lose count equal mana value of it
life` after `return up to 1 target Vampire card ...`; `it deals` stays the
triggering object. Where the triggering object is the source (`when this
enters`, `whenever this attacks`, `when this is turned face up`), a spell
(`whenever you cast ...`), or there is none (`at beginning of combat on
your turn`), `that creature` after an object target is that target too: `when this enters:
attach to target creature you control; that creature gains hexproof`
("When this Equipment enters, attach it to target creature you control.
That creature gains hexproof ..."); with no object target before it, there
it is an error ("When this Aura enters, ... untap that creature" names no
creature: write `this`, `attached`, or a target). In a trigger with no triggering object (`at beginning
of ...`, `whenever you gain life`, `whenever you attack`) `it` is an
error: write `this` or `target N`. In any ability, `it` after `create 1
"..."` (or a token copy), before another target, is that token ("Create a
0/0 ... token. Put X +1/+1 counters on it."), as `that token` is; and
`that player` after a target player is that player, in a triggered
ability too ("Target player mills three cards. ... that player mills ten
cards instead"). In a triggered ability `toughness of
it`, `power of it`, `mana value of it` ("its toughness") and `power of
that creature` ("that creature's power"; `mana value of that spell`:
"that spell's mana value", the spell a cast trigger is about) otherwise
read the triggering object, as it last existed once it has left (CR 608.2h): `triggered when enchanted creature dies: its
controller loses count equal power of it life and you gain count equal
toughness of it life`.
`count each creature blocking it` ("for each creature blocking it", the
triggering creature or this one; `creature blocking it beyond first`:
"beyond the first") and the recipient `each creature blocking it`
("Whenever this creature becomes blocked, it deals 1 damage to each
creature blocking it"). A count of attacking or blocking creatures omits
"on the battlefield" ("for each other attacking Aurochs").
`count each 2 creature you control` ("2 life for each creature you
control"); filter `defending player controls` (two players: the
attacker's opponent). `half down <count>`, `half up <count>`: "half the
number of Zombies you control, rounded down" (CR 107.1a; `create count
where half down Zombie you control "2/2 black Zombie creature"`);
`creatures entered this turn`: "the number of creatures that entered
the battlefield under your control this turn" (a static ability's count
too). `discarded this turn`: "for each card you've discarded this turn"
(a cost's discard included; not in a static ability). `every 7 <count>`: "for every seven cards in your graveyard", rounded
down (`static this: +2/+0 count each every 7 graveyard`). Both work in
static abilities.
`if you control Treefolk, draw 1 card`: a static condition judged as the
effect resolves ("If you control a Treefolk, draw a card."); written after
the sentence, it prints after it: `draw 1 card if you control creature
with +1/+1 counter` ("Draw a card if you control a creature with a +1/+1
counter on it."), `its controller loses 3 life if cast another black
spell this turn`. After any such condition a spell names itself again,
and `also` goes after its subject ("If you control an Equipment,
Misthios's Fury also deals 2 damage ...", "that creature also gains
trample"). `if you controlled target 1, draw 1 card`: "If you controlled
that permanent, draw a card.", its controller as it last existed (CR
608.2h); the target's own noun prints ("If you controlled that
artifact"), and `if you controlled that target 1, ...` prints "that
permanent" whatever the target is (Kellan, Inquisitive Prodigy).
`choose target battle; if an opponent protects target 1, remove 1
defense counter from target 1; otherwise, put 1 defense counter on
target 1`: "Choose target battle. If an opponent protects it, remove a
defense counter from it. Otherwise, put a defense counter on it."
(Portent Tracker); `if you protect target 1, ...` prints "If you protect
it". The battle's protector (CR 310.8; a Siege's is its controller's
opponent) is judged from the ability's controller as it resolves; a
target no longer on the battlefield matches neither. `then if <cond>, <clause>` prints "Then if ...". `you lose 1 life
unless opponent life 10 or less` prints "you lose 1 life unless an
opponent has 10 or less life": the clause happens only if the condition
doesn't hold as it resolves; `you lose 2 life unless you control Villain`
prints "unless you control a Villain" (The Spot's Portal). Conditions
also include `your life more than opponent` ("you have more life than an
opponent"), `3 basic land types` ("there are three or more basic land
types among lands you control"), `you don't control legendary creature`
("you don't control a legendary creature"), `there is Elf card in
your graveyard` ("there is an Elf card in your graveyard"), `you control
permanent with oil counter` (any named counter kind), and `you control
7 land or Treefolk` ("seven or more lands and/or Treefolk"). The filter
word `outlaw` is an Assassin, Mercenary, Pirate, Rogue, or Warlock (CR
700.12): `that target 1 gets -4/-0 instead if you control outlaw`, `if
outlaw was sacrificed this way, ...`. The filter word `historic` is an
artifact, a legendary, or a Saga (CR 700.6): `triggered whenever you cast
historic spell: scry 1` ("Whenever you cast a historic spell, scry 1."). `put 1 card from hand on bottom of
library chosen`: "put a card from your hand on the bottom of your
library". A graveyard target may name a keyword after the noun: `target
creature card with infect from your graveyard`. A target check may be
its combat state or its power: `if that target 1 is attacking, ...`
("If that creature is attacking"), `then if that target 1 is creature
with power 0 or less, destroy target 1` ("Then if that creature's power
is 0 or less, destroy it"), or its controller: `if that target 1 is an
opponent controls, ...` ("If an opponent controls that creature"). An "instead" that is optional (`if that
target 1 is attacking, you may put target 1 on top of library instead`)
does the earlier instruction when declined. A sentence may
start with an ability word: `label "Spell mastery" if 2 instant or sorcery
cards in your graveyard, scry 2` ("Spell mastery — If there are two or more
instant and/or sorcery cards in your graveyard, scry 2."). `that target
1` prints "that creature" even after the first mention.
`tap or untap target creature` (the controller chooses on resolution). `remove target attacking or blocking creature from combat`, `you may untap this and remove this from combat` (CR 506.4: an attacker removed from combat deals and is dealt no combat damage; an attacker whose blocker is removed stays blocked, CR 509.1h). `target unblocked attacking creature becomes blocked`, `X target attacking creature become blocked; this deals 1 damage to those targets` ("... deals 1 damage to each of those creatures": `those targets` after a variable group is all of it), `all attacking creature becomes blocked` ("Attacking creatures become blocked."): CR 509.1h, an attacking creature not yet blocked becomes blocked with no blocker (even one that can't be blocked), so it assigns no combat damage unless it has trample (CR 702.19e: all of it to the player), and "becomes blocked" triggers; with `cast only during declare blockers` or `cast only during combat after blockers`.
`next spell costs {1} less` prints "the next spell you cast this turn costs {1}
less to cast" (CR 601.2f): generic mana off the controller's next spell
this turn; several add up, and that spell uses them all up.
`next red or green creature spell free` prints "the next red or green
creature spell you cast this turn can be cast without paying its mana
cost" (CR 118.9; any filter before `spell`): the controller may cast the
next such spell from their hand without paying its mana cost (`cast "X"
free` in a scenario); the next such spell uses it up however it is cast,
and it ends with the turn.
`next spell can't be countered` prints "the next spell you cast this turn
can't be countered" (CR 101.2): the controller's next spell cast this
turn is uncounterable; the effect ends with the turn.
`face-down spells cost {1} less this turn` prints "face-down spells you
cast this turn cost {1} less to cast": off the {3} of each face-down spell
(CR 702.37c) the controller casts for the rest of the turn.
`spells artifact cost {1} less this turn` prints "Artifact spells you cast
this turn cost {1} less to cast." (any filter; `spells cost {1} less this
turn`: "Spells you cast this turn ..."); `spells black or red cost {X} less
this turn count where life you lost this turn` prints "Spells you cast
this turn that are black and/or red cost {X} less to cast, where X is the
amount of life you lost this turn" (a filter of colors alone prints "that
are ... and/or ..."): generic mana off each matching spell the controller
casts for the rest of the turn (CR 601.2f), the amount fixed as the effect
resolves (CR 611.2a). `next noncreature spell has affinity for artifact`
prints "the next noncreature spell you cast this turn has affinity for
artifacts" (CR 702.41a; any filter before `spell`): {1} less for each
artifact the caster controls, used up by the next matching spell cast this
turn. `that spell gains deathtouch and lifelink` prints "That spell gains
deathtouch and lifelink." (a cast trigger's spell): the spell has the
keywords for as long as it is on the stack (lifelink, deathtouch, wither
and split second act there, CR 702.15, 702.2, 702.80, 702.61).
`learn` (CR 701.48) prints "Learn."; there are no Lesson cards outside the
game here, so it is "you may discard a card; if you do, draw a card".
`goad target creature` (CR 701.15; `goad target 1` prints "Goad it.",
`goad targets 1 to 3` "Goad them."): until the controller's next turn the
creature attacks each combat if able, and attacks a player other than the
controller if able.
`turn face up target face-down creature an opponent controls` prints "Turn
target face-down creature an opponent controls face up." (CR 708.8: no
cost is paid, so megamorph adds no counter, CR 702.37b; "turned face up"
triggers fire; a face-down instant or sorcery card is revealed and stays
face down, CR 701.40g). `turn face down target creature with morph` prints
"Turn target creature with a morph ability face down." (CR 708.2a: a 2/2
with no text, no ward even for a disguise card; it can be turned face up
again for its morph cost; a face-down or double-faced permanent is
unaffected, CR 708.2b, 712.16). `with morph` is a filter word ("with a
morph ability", CR 702.37: morph or megamorph, not disguise).
`look at face-down target face-down creature` prints "Look at target
face-down creature." (CR 708.5): the controller learns the card;
`you may look at face-down each face-down attacking or blocking late
creature` prints "You may look at each face-down creature that's attacking
or blocking." (`attacking or blocking late` prints the combat words after
the noun). `look at hand top and face-down of target player` prints "Look
at target player's hand, the top card of that player's library, and any
face-down creatures they control." Information only: an agent's view shows a face-down permanent's
card to its controller and to whoever has looked at it.
`detain target creature an opponent controls` (CR 701.35): until the
controller's next turn the permanent can't attack or block and its
activated abilities can't be activated. `detain spelled target nonland
permanent` prints it out: "Choose target nonland permanent. Until your
next turn, it can't attack or block, and its activated abilities can't
be activated." (Academic Probation).
`earthbend 2` (CR 701.66), `earthbend count where Forest you control`
("Earthbend X, where X is the number of Forests you control"): declares
its own target, "target land you control", which becomes a 0/0 creature
with haste in addition to its other types (indefinitely) and gets N +1/+1
counters; a delayed trigger returns it to the battlefield tapped under the
earthbender's control when it dies or is put into exile (leaving for any
other zone ends it). Prints "Earthbend 2." In scenarios the land is the
spell's or ability's target like any other.
Double (CR 701.10): `double target creature power` ("Double target
creature's power until end of turn"), `double this power and toughness`,
`double the power of target creature` ("Double the power of target
creature until end of turn"), `double the power and toughness of each
creature you control`: +X/+0 (+X/+Y) until end of turn, X its power (Y
its toughness) as the effect resolves, negative values getting more
negative. `double the number of +1/+1 counters on this`, `double the
number of each kind of counter on target permanent` (each recipient gets
as many of those counters as it has; `on those targets` prints "on each
of those creatures"). `double your life total`, `double target player
life total`, `double its controller life total` ("Double target player's
life total": gain, or below zero lose, that much life). The damage
outcome `double` (`replace damage from permanent you control to any:
double`) prints "If a source you control would deal damage to a
permanent or player, it deals double that damage to that permanent or
player instead." (a filter without a card type names a source;
`replace damage to you: double` "... to you instead."; written with its
article, `replace damage from a Wizard you control to any: double`;
several creature types, `replace damage from Lizard or Mouse or Otter or
Raccoon you control to any: plus 1`, print without "source": "If a
Lizard, Mouse, Otter, or Raccoon you control would deal damage ...";
prints "If a Wizard you control would deal damage to a permanent or
player, it deals double that damage instead."). Likewise
`replace damage from permanent you control to each opponent: prevent {
each opponent mills that much cards }` prints "If a source you control
would deal damage to an opponent, prevent that damage and each opponent
mills that many cards." (`that much` is the prevented damage).
`replace damage to you: double` "... to you instead."; `replace damage permanent you control of chosen type would deal: double` is the newer "Double all damage that sources you control of the chosen type would deal.").
The recipient `opponent and <filter>` is "an opponent or <a filter>":
`replace damage from permanent you control to opponent and permanent an
opponent controls: double` prints "If a source you control would deal
damage to an opponent or a permanent an opponent controls, it deals double
that damage instead." (Twinflame Tyrant). The damage outcome `plus 2`
adds that much (CR 614.1a): `replace damage from red permanent you
control to opponent and permanent an opponent controls: plus 2` prints
"..., it deals that much damage plus 2 instead." (Torbran); `plus 1 to
that` adds "to that permanent or player" (`replace damage from other red
permanent you control to any: plus 1 to that`, Jaya). For life, `replace
you gain life: plus one` prints "If you would gain life, you gain that much
life plus 1 instead."; `plus count where power` adds a count of the source's, evaluated as the
damage would be dealt: `replace noncombat damage from permanent you
control to opponent and permanent an opponent controls: plus count where
power` prints "..., instead it deals that much damage plus X, where X is
Hawkeye's power." (Hawkeye, Young Avenger); for counters, `replace +1/+1 counters put on
creature you control: double` prints "If one or more +1/+1 counters would
be put on a creature you control, twice that many +1/+1 counters are put
on that creature instead." (also the counters it enters with, CR 122.6).
Counters of any kind: `replace counters put on creature or Spacecraft or
Planet you control: double` prints "If one or more counters would be put
on a creature, Spacecraft, or Planet you control, twice that many of each
of those kinds of counters are put on it instead." (Loading Zone; `plus
one`: "that many plus one of each ..."); `replace counters put by effect
on permanent you control: double` prints "If an effect would put one or
more counters on a permanent you control, it puts twice that many of those
counters on that permanent instead." (Doubling Season: not a cost's
counters); `replace counters you put on permanent you control: plus one`
prints "If you would put one or more counters on a permanent you control,
put that many plus one of each of those kinds of counters on that
permanent instead." (Doc Samson: counters the replacement's controller's
spells, abilities, and costs put). Each also changes the counters a
permanent enters with, its own included (a planeswalker's loyalty, a
Saga's lore counter, modular's, CR 122.6).
Token creation (CR 111.1, 614.1a): `replace tokens created under your
control: double` prints "If one or more tokens would be created under your
control, twice that many of those tokens are created instead." (`... by
an effect: double` is the older "If an effect would create one or more
tokens under your control, it creates twice that many of those tokens
instead."; `triple` prints "three times that many", Ojer Taq); `replace creature tokens created under your control: plus
"1/1 white Soldier creature"` prints "If one or more creature tokens would
be created under your control, those tokens plus a 1/1 white Soldier
creature token are created instead." (`plus that many "..."`: "plus that
many ... tokens"). `replace artifact tokens created under your control:
that many "5/5 red Dragon creature with flying"` prints "If one or more
artifact tokens would be created under your control, that many 5/5 red
Dragon creature tokens with flying are created instead." (the covered
tokens become as many of that token); `replace artifact tokens you create:
plus "Map"` prints "If you would create one or more artifact tokens,
instead create those tokens plus an additional Map token." Each applies
once to each token-making effect's tokens for one player (`create`, token
copies, amass, fabricate, incubate, populate, endure, ...): tokens made
instead come first, then tokens added, then doubling, so a doubler doubles
the added ones too (the order is the player's choice, CR 616.1, fixed
here). An amassed Army made twice is two Armies, one of which gets the
counters. `triple` in place of `double` prints "three times that many of
those tokens" (Ojer Taq).
Mana (CR 106.12): `replace tapped for mana permanent you control: triple`
prints "If you tap a permanent for mana, it produces three times as much of
that mana instead." (Nyxbloom Ancient; `double`: "twice as much", Mana
Reflection; `basic land you control`: "If you tap a basic land for mana,
..."): a permanent its controller taps for mana with a mana ability
({T} in its cost) makes that many times its own mana, not what triggered
mana abilities add; several multiply together. Payment counts the mana when
each mana the permanent makes is of one known kind ("Add {R}", "Add
{C}{C}"); a permanent making one of several kinds is tapped for its
mana, and the rest goes into the pool.
Whole events of one or more (CR 614.1a), once for each instruction:
`replace opponent mills: plus 4` prints "If an opponent would mill one or
more cards, they mill that many cards plus four instead." (The Water
Crystal; `double`: "twice that many cards", Bruvac; `you mill`, `a player
mills` too; the milling player orders several, doubling first).
`replace you draw cards while hand 1 or less: plus one` prints "As long as
you have one or fewer cards in hand, if you would draw one or more cards,
you draw that many cards plus one instead." (Quantum Riddler: a draw
instruction, the draw step's draw, a connive's draw). `replace you draw a
card except first in draw step: draw two` prints "If you would draw a card
except the first one you draw in each of your draw steps, draw two cards
instead." (Bard, King of Dale: each card; the first card the active player
draws in their draw step isn't replaced). `replace creature you control
connives: instead { you draw 1 card }` prints "If a creature you control
would connive, instead you draw a card, then that creature connives."
(Leader, Super-Genius), `replace creature you control explores: instead {
scry 1 }` "If a creature you control would explore, instead you scry 1,
then that creature explores." (Twists and Turns): the block's effects
(the replacement's controller's, "that creature" the one conniving), then
the creature connives or explores (CR 701.50, 701.44), each such ability
once.
`replace land you control enter: untapped` prints "Lands you control enter
untapped." (Spelunking, The Wandering Minstrel): a matching permanent
enters untapped however it would have entered tapped, its own "enters
tapped" included (its controller orders the replacements so, CR 616.1, an
engine pick) and one put onto the battlefield tapped (the card's ruling).
`replace an opponent loses life during your turn: double` prints "If an
opponent would lose life during your turn, they lose twice that much life
instead." (CR 119.3: damage to a player included, life paid as a cost
not; `you lose life`, `a player loses life` too). `replace card or token put into your graveyard: exile` prints "If a
card or token would be put into your graveyard from anywhere, exile it
instead." (Festival of Embers); `card put into your graveyard` and `card
put into an opponent's graveyard` ("If a card would be put into ...")
leave tokens alone (CR 108.2: a token isn't a card), `card put into a
graveyard` prints "a card or token" and covers both. `replace instant or
sorcery card put into a graveyard: exile` prints "If an instant or
sorcery card would be put into a graveyard from anywhere, exile it
instead." (any card filter, judged as the card is wherever it is; tokens
aren't cards).
`replace this put into a graveyard: shuffle into library` prints "If
Darksteel Colossus would be put into a graveyard from anywhere, reveal
Darksteel Colossus and shuffle it into its owner's library instead.": the
card names itself, and the ability works in every zone (CR 113.6b: hand,
library, stack, battlefield); the card goes into the library, which is
then shuffled.
`target opponent loses 3 life unless they sacrifice nonland permanent
else discard 1 card` prints "... unless they sacrifice a nonland permanent
of their choice or discard a card" (CR 608.2d; `else`, since `or` would
start another clause; in a spell, `sacrifice another ...` is an error:
`another` excludes the source, not the spell's target, Torment of Venom); `each opponent loses 3 life unless each opponent
sacrifices nonland permanent else discards 1 card` prints "... unless that
player sacrifices ... or discards a card"; `it deals 5 damage to target
opponent unless target 1 sacrifices creature` prints "... unless that
player sacrifices a creature of their choice". The player chooses as it
resolves (in a scenario `choose "1"` sacrifices, `"2"` discards, `"3"`
takes the penalty).
`exchange control of 2 target creature` (CR 701.12) prints "Exchange
control of two target creatures."; `exchange control of 2 target nonland
permanent that share card type` prints "... two target nonland permanents
that share a card type." (`that share card type`, in the spell's first
target spec: each target must share a card type with the first, as
chosen and as the spell resolves; `return 2 target creature that share
creature type card from your graveyard to hand`: "Return two target
creature cards that share a creature type from your graveyard to your
hand.", each sharing a creature type with the first, a changeling every
one, CR 702.73a; `choose 2 target creature controlled by different
players; return those targets to hand`: "Choose two target creatures
controlled by different players. Return those creatures to their owners'
hands.", each controlled by someone other than the other's controller, as
it is or last existed, as chosen and as the spell resolves, so once one
player controls both, neither is legal; `controlled by the same player`,
`controlled by the same opponent` likewise: "Choose two target creatures
controlled by the same player.", both illegal once their controllers
differ; `another target creature with same controller` prints "another
target creature with the same controller" (Bioshift). After such a pair,
one of them is picked as the spell resolves (CR 608.2d; a still-legal one
if the other isn't, CR 608.2b): `that player sacrifices one of them of
their choice` ("That player sacrifices one of them of their choice.", the
targets' controller), `that player chooses and sacrifices one of those
targets` ("... chooses and sacrifices one of those creatures."), `their
controller chooses and sacrifices one of them`, `that player chooses one
of those targets` ("That player chooses one of those creatures."), `exile
one of those targets` (the caster picks: "Exile one of those creatures");
then the picked one is `that target 1` ("that creature") and the other
`the other target 2` ("the other": `put 2 +1/+1 counters on the other
target 2`, `the other target 2 can't block this turn` joined by `and`
prints ", and the other can't block this turn")); `exchange control of this and target
nonland permanent`, `exchange control of target land you control and
target land an opponent controls` name the two. Nothing happens unless
both are still on the battlefield (CR 701.12a) under different
controllers (701.12b); then each controller gains control of the other's
permanent indefinitely.
`target opponent gains control of target permanent you control` (a
player subject other than `you`; CR 613.1b) prints "Target opponent gains
control of target permanent you control.": that player gains control
indefinitely (`an opponent gains control of this artifact`: "An opponent
gains control of this artifact.", the game's one opponent). A following
`if they do,` prints "If they do, ..." and holds
only if the permanent changed hands (Stiltzkin); `you may have` prints "you
may have target opponent gain control of ...". `whenever an opponent gains
control of a permanent from you` triggers on each permanent its
controller loses to an opponent, an "until end of turn" effect ending
included (the permanent is the triggering object, the opponent the
triggering player).
`exchange life totals with target opponent` (CR 701.12c): each player
gains or loses the life needed to reach the other's former total; a
following `if you do,` prints "If you lost life this way, ..." and holds
only if the controller lost life, and `count that many life lost this way`
is that loss ("draw that many cards", Mister Negative).
`end the turn` (CR 724.1) prints "End the turn.": triggered abilities
waiting to be put on the stack cease to exist, every object on the stack
(the resolving spell or ability too) is exiled (an ability or a copy of a
spell ceases to exist), and once the resolution is over the game skips to
the cleanup step (no end step; creatures leave combat; damage wears off,
"until end of turn" effects end, and the active player discards to seven
there).
`cloak top` (CR 701.58): "cloak the top card of your library": the card
enters face down under the controller's control as a nameless 2/2 creature
with ward {2} (`that token` is it afterwards: `cloak top, then attach to
that token` prints "..., then attach this Equipment to it"); its
controller may turn it face up any time they have priority for its mana
cost if it's a creature card (scenarios: `unmorph "Name"`).
`airbend target nonland permanent` (CR 701.65), `airbend up to one
another target creature`, `airbend any number of another target nonland
permanent you control` ("any number of other target nonland permanents you
control"): exiles the permanents (or spells); each card exiled this way
may be cast by its owner from exile for {2} rather than its mana cost, with
its usual timing, for as long as it stays exiled (a land can't be played
this way; a token or a copy of a spell ceases to exist). Prints "Airbend
target nonland permanent."
`return all creature card from your graveyard to battlefield` ("Return
all creature cards from your graveyard to the battlefield."): every card
in the controller's graveyard matching the filter enters under its owner's
control; `... to battlefield tapped` ("Return all land cards from your
graveyard to the battlefield tapped."), and `return all andlist artifact
or enchantment card ...` ("all artifact and enchantment cards"). A
mana value X is the spell's (CR 107.3). `put all creature card from all
graveyards onto battlefield under your control` ("Put all creature cards
from all graveyards onto the battlefield under your control.", Rise of
the Dark Realms): every graveyard's, under the controller's control.
After either, `them` is the permanents that entered (`delayed them:
sacrifice them at beginning of the next end step`: "Sacrifice them at the
beginning of the next end step.", one delayed trigger about each, Push
the Limit). After `return up to 2 target creature card ... to
battlefield`, `those targets` prints "those creatures" ("Put a +1/+1
counter on each of those creatures.", Smile at Death) and `targets 1 to 2`
as a subject "they" ("They gain haste until end of turn.", Pull). A group
that becomes a type without power and toughness prints in the plural:
`each Vehicle you control becomes artifact creature until end of turn`
("Vehicles you control become artifact creatures until end of turn.").
`put count where power of that target 1 +1/+1 counters on target
creature` ("Put X +1/+1 counters on target creature, where X is that
creature's power", also `mana value of that target 1`): the count may
name the target its counters phrase declares, judged as the effect
happens.
`recruit` (CR 701.70): the controller draws a card, then discards a
card; if the discarded card was a nonland card, they create a 1/1 white
Human Soldier creature token. Prints "Recruit."; `you recruit` prints
"you recruit" (The Queen of Dale's wording after another player's
action).
`spells red or white you cast cost {1} less` prints "Red spells and white
spells"; `spells each red or green you cast cost {1} less` prints "Each
spell you cast that's red or green costs {1} less to cast." (Goblin
Anarchomancer); `spells Goblin or Rogue ...` prints "Goblin spells and Rogue
spells". `defending player` as a subject prints "defending player".

Cost item `from graveyard`: activated from the graveyard ("Return this
card from your graveyard to your hand." for `return this to hand`).
`triggered from graveyard <trigger>: ...` is a triggered ability that
works only while the card is in its owner's graveyard (CR 113.6k),
controlled by its owner; `return this to hand` / `return this to
battlefield tapped` in it print "return this card from your graveyard to
...". A card in a graveyard sees no event that put it there (CR 400.7),
and once it leaves the graveyard the ability can't return it. `return this
to battlefield attached to that creature` (an Aura, CR 303.4f): "...
attached to that creature"; nothing happens if that creature is gone or
can't be enchanted by it (CR 303.4g).

`triggered once each turn <trigger>: ...` prints "This ability triggers
only once each turn." after the effects (`twice each turn`: "only twice
each turn"): past that many times in a turn the ability doesn't trigger
at all. `triggered once ever <trigger>: ...` prints "This ability
triggers only once.": once for the object, ever (an intervening "if"
that fails doesn't use it up). `triggered do once each turn <trigger>: you may ...` prints "Do
this only once each turn.": the ability triggers every time, but its
first `you may` is offered only until its controller has done it this
turn (declining doesn't count). Both count per object and ability and
reset each turn; a permanent that changes zones is a new object and
starts fresh (CR 400.7). Each goes after `from graveyard`, before the
trigger, and works in a granted ability (`has ability { triggered once
each turn ... }`), counted for the permanent that has it.
Cards from hand into a library: `put 1 card from hand on bottom of
library` ("put a card from your hand on the bottom of your library"),
`put 2 cards from hand on top of library` ("... on top of your library in
any order": chosen one at a time, the last on top), `target opponent puts
1 card from hand on top of library` ("Target opponent puts a card from
their hand on top of their library"), `each player puts ...`. With an
empty hand, `if you do` after it fails.
`, and` between clauses repeats the subject ("you draw a card and you
lose 1 life"). Filters: `your opponents control`, `creature with flying
or reach`.

`triggered when ...` on a trigger Oracle words "Whenever" prints "When"
("When this creature blocks, ..."). `look at hand of target player`
("Look at target player's hand."). `gets +1/+1 X` is "+X/+X".

Energy (CR 107.14): `you get 2 energy` ("you get {E}{E}"), `pay 2
energy` inside `may` and as a cost ("Pay {E}{E}"). `you may pay any energy` is
"you may pay any amount of {E}": the amount is chosen as it resolves (0
to all; all unscripted), and `that much` and X after it in the resolution
are the amount paid ("Galvanic Discharge deals that much damage to that
permanent"). The count `energy paid` is that amount: `counter that
target 1 unless its controller pays {X} count each energy paid` prints
"Counter that spell unless its controller pays {1} for each {E} paid this
way." (Aether Spike).

Blight (CR 701.68): `blight 2` is an effect ("blight 2": the controller
puts two -1/-1 counters on a creature they control, chosen as it
resolves; with none, nothing happens and `if you do` after `you may
blight 2` fails, 701.68b) and a cost (`{T}, blight 1: ...` prints "{T},
Blight 1: ..."; `additional cost blight 1` with `additional cost or {3}`
prints "As an additional cost to cast this spell, blight 1 or pay {3}.").
As a cost the engine puts the counters on the toughest creature its payer
controls, and it can't be paid without a creature. The older wording is
the same cost: `additional cost put 1 -1/-1 counter on creature you
control` ("As an additional cost to cast this spell, put a -1/-1 counter
on a creature you control.", Lethal Sting) and `{2}{R}, {T}, put 1 -1/-1
counter on creature you control: ...` (Hatchet Bully).

Processors (CR 400.3): `you may put 1 card an opponent owns from exile
into graveyard chosen; if you do, ...` prints "you may put a card an
opponent owns from exile into that player's graveyard. If you do, ..."
(the controller chooses among the cards in exile their opponents own; the
card goes to its owner's graveyard; with none, `if you do` fails), and
`put 2 cards an opponent owns ...` "put two cards your opponents own from
exile into their owners' graveyards" (all or nothing). As a cost, `{2},
put 1 card an opponent owns from exile into graveyard: ...` and `additional
cost put 1 card an opponent owns from exile into graveyard` print "Put a
card an opponent owns from exile into that player's graveyard" (payable
only with such a card in exile; the engine picks the longest exiled).
Devoid prints before an additional cost, as flash does.

Behold (CR 701.69a) is a cost: `additional cost behold Kithkin` with
`additional cost or {2}` prints "As an additional cost to cast this
spell, behold a Kithkin or pay {2}."; `behold and exile Elf` "behold an
Elf and exile it"; `behold 3 Elemental` "Behold three Elementals"; `may
behold Dragon` (a spell's additional cost) "you may behold a Dragon", an
optional additional cost (scenarios decline it with `other_cost`). The
payer chooses a matching permanent they control or reveals a matching
card from their hand, never the spell itself; the engine picks, a
permanent first, or with `and exile` a card in hand first. What a spell's
behold cost exiles is linked to the permanent the spell becomes (CR
607.2a): `triggered when this leaves the battlefield: return exiled card
to hand` ("return the exiled card to its owner's hand"). The condition
`beheld` (`if beheld, you gain 2 life`) prints "If a Dragon was beheld"
with the card's behold filter: the behold cost was paid. Behold is also
an effect: `you may behold Dragon; if you do, create 1 "Treasure"` ("you
may behold a Dragon. If you do, create a Treasure token."; `behold 2
Elf`, "behold two Elves"): with too few matching permanents and cards in
hand nothing happens and `if you do` sees that. `enters tapped unless
behold Jace` prints "As this land enters, you may behold a Jace. If you
don't, this land enters tapped." (a Jace card in hand or a Jace it
controls).

The older wording reveals from the hand only: `additional cost reveal
Kithkin card` with `additional cost or {3}` prints "As an additional cost
to cast this spell, reveal a Kithkin card from your hand or pay {3}.";
alone (`additional cost reveal creature card`) "..., reveal a creature
card from your hand." (castable only with such a card in hand, other
than the spell; a permanent doesn't pay it); `additional cost may reveal
Dragon card` "..., you may reveal a Dragon card from your hand." (an
optional cost, declined with `other_cost`). The revealed card is "the
revealed card": `count equal mana value of the revealed card` prints
"the revealed card's mana value" (`power of the revealed card`, "the
revealed card's power"), read as the spell resolves. With a reveal cost,
`beheld` prints "you revealed a Dragon card or controlled a Dragon as you
cast this spell": the card was revealed, or its caster controlled a
matching permanent as the spell was cast (CR 601.2); a permanent keeps
it for `enters with 1 +1/+1 counter if beheld` and `triggered when this
enters if beheld: ...`. In an activated ability's cost, `reveal X white
cards` prints "Reveal X white cards from your hand": X is the ability's,
announced as it is activated, at most the matching cards in hand (the
engine picks which); `{1}, reveal X white cards, sacrifice this: you gain
3 times X life` (Martyr of Sands).

Collect evidence (CR 701.59): `collect evidence 4` is an effect (the
controller exiles cards with total mana value 4 or greater from their
graveyard; with too little there, nothing is exiled and `if you do` after
`you may collect evidence 4` fails) and a cost (`{T}, collect evidence 3:
...` prints "{T}, Collect evidence 3: ..."). The engine picks the cards:
the greatest mana values first, then it drops the smallest picks the total
doesn't need, sparing what the spell or ability targets. Trigger `whenever
you collect evidence`.

Forage (CR 701.61): `forage` is an effect ("exile three cards from your
graveyard or sacrifice a Food": with both possible the controller chooses,
`choose "1"` the cards, `choose "2"` a Food; with neither, nothing happens
and `if you do` after `you may forage` fails), a cost (`{2}, forage: ...`
prints "{2}, Forage: ..."; `additional cost forage` / `additional cost or
{B}` prints "As an additional cost to cast this spell, forage or pay
{B}."), where the engine exiles the three oldest other cards if it can and
otherwise sacrifices a Food, and the trigger `whenever you forage`. A mana
ability with a forage cost is activated on its own, not while paying.

Suspect (CR 701.60): `suspect target creature`, `suspect this`, `suspect
enchanted creature` (a suspected permanent has menace and "This creature
can't block" for as long as it's suspected, CR 701.60c; it stops being
suspected as it leaves the battlefield), `all suspected creature no
longer suspected` ("All suspected creatures are no longer suspected."),
the filter word `suspected` ("Sacrifice a suspected creature"), and the
conditions `suspected` / `not suspected` ("if it's not suspected"; at the
start of an activated ability's sentence, "If this creature is suspected,
... it"). `each opponent blights 1` (CR 701.68a): each of those players
puts the counters on a creature of their own choice.

Manifest dread: after it, `manifested creature` is the creature
manifested, printed "that creature" ("manifest dread, then put three
+1/+1 counters on that creature"; `attach to manifested creature`:
"attach this Equipment to that creature"). `delayed target 1: when it
dies this turn, manifest dread` prints "When it dies this turn, ...".
Another player: `counter target spell; its controller manifests dread`
("Its controller manifests dread.", CR 701.62a: the face-down creature is
theirs). Trigger `whenever you manifest dread` (it triggers even when the
library was too small to manifest anything); in it, `dread card to hand`
prints "put a card you put into your graveyard this way into your hand"
(the card manifest dread put into the graveyard, if any).
Trigger `whenever you turn a permanent face up` (a permanent you
control; "it" is that permanent). `return up to 2 permanent card from
among them to hand chosen` after a mill prints "return ... from among
them to your hand", and `put 1 Hero or enchantment card from among those
cards into hand chosen` "... from among those cards into your hand" (Rick
Jones). In a triggered ability, `put its counters on this` moves the
triggering object's counters, as it last existed ("Whenever another
creature you control dies, if it had counters on it, put its counters on
this creature."; the condition `if it had counters` is any kind on the
triggering object). So does `put its counters on target creature` in a
trigger about another object ("Whenever another creature you control
dies, put its counters on target creature you control"); in a trigger
about the source itself (`when this dies`) they are the source's. `impulse 2 next turn it` prints "You may play them
until the end of your next turn."

An optional additional cost (CR 601.2b): `additional cost may collect
evidence 6` ("As an additional cost to cast this spell, you may collect
evidence 6.") or `additional cost may blight 1`, chosen as the spell is
cast (scenarios: `kicked`), offered only when it can be paid. The
conditions `evidence collected` ("evidence was collected") and
`additional cost paid` ("this spell's additional cost was paid") read it,
in a spell, an intervening "if", or `this spell costs {2} less if
evidence collected` (printed after the cost, in its paragraph); the
permanent a spell becomes keeps it.

The mana spent to cast a spell (CR 601.2h): `mana spent` counts all of
it ("the amount of mana spent to cast it", "this spell" on an instant or
sorcery), and `mana spent that spell` the triggering spell's ("Whenever
you cast an instant or sorcery spell, this creature gets +X/+0 until end
of turn, where X is the amount of mana spent to cast that spell":
`triggered whenever you cast an instant or sorcery spell: this gets
+1/+0 count where mana spent that spell until end of turn`); `colors
spent that spell` counts that spell's colors ("for each color of mana
spent to cast that spell"). A permanent with such a count makes every
cast record what was spent.

Converge (CR 601.2h): `colors spent` counts the colors of mana spent to
cast the source ("this spell" on an instant or sorcery, "it" otherwise;
`colors spent spell` and `colors spent creature` print "this spell" and
"this creature"). A filter `with mana value colors spent or less` (or
`with power colors spent or less`) prints "with mana value less than or
equal to the number of colors of mana spent to cast this creature", in a
target or a check (`exile target nonland permanent if target 1 is with
mana value colors spent or less`: "... if its mana value is less than or
equal to ..."). `enters with count each 2 colors spent +1/+1 counters`
prints "two +1/+1 counters on it for each color ...". After creating
tokens, `each of them` is those tokens ("Put X +1/+1 counters on each of
them"). `gain control of target creature indefinitely` prints "Gain
control of target creature." (no duration).

Filters may have an alternative: `creature or Vehicle`, `land or
nonblack creature`, `attacking or blocking creature`, `Bear, Spider, or
Wolf you control`, and longer lists (`target Insect, Rat, Spider, or
Squirrel`; a card filter prints them so too: "a Bat, Lizard, Rat, or
Squirrel card", "up to two Cleric, Rogue, Warrior, Wizard, and/or Ally
cards"). `nontoken green creature` and `nonlegendary green creature card`
print in that order ("a nontoken green creature", Flare of Cultivation);
`green nontoken creature` prints "a green nontoken creature" (wording
only). A spell target may say what the spell targets:
`target spell that targets you or a permanent you control`. One target
may be a spell or a permanent (CR 115.1): `target spell or creature`,
`target spell or permanent with mana value 1 or greater` (the filter's
other qualities apply to the spell too); `return target spell or
creature to hand` returns a spell from the stack to its owner's hand. A group of
targets can be one subject: `up to 2 target creature gets +2/+0`
("Up to two target creatures each get +2/+0"), later `targets 1 to 2`. A trailing
`you control`, a leading `another`, and a type after two subtypes
(`Elf or Soldier creature`) apply to both sides; an alternative written
with an article of its own doesn't take `other`: `other creature or an
artifact` ("another creature or an artifact"), and `artifact or other
creature` ("an artifact or another creature") keeps it on its side.
`up to 1 another target permanent you control` is "up to one other target
permanent you control" (CR 115.3: not one chosen for another target).

Events: `damage to <affects>`, `combat damage to <affects>`,
`damage from creature you control to <affects>` (without `to`, any
recipient: `damage from enchanted`, "Prevent all damage that would be
dealt by enchanted creature"), `damage this would deal to snow creature`
("Prevent all damage that this creature would deal to snow creatures"),
`combat damage to and from enchanted` ("dealt to and dealt by enchanted
creature": one effect, CR 615), `you gain life`,
`a player gains life`, `you draw`, `you draw with empty library`,
`card put into a graveyard`, `card put into an opponent's graveyard`,
`... from the battlefield`, `+1/+1 counters put on creature you control`,
`destroy <affects>`, `enters`, and another permanent entering (CR
614.1c): `replace each Dragon you control enters: with 1 +1/+1 counter`
("Each Dragon you control enters with an additional +1/+1 counter on
it."), `replace other creature you control enter: with count each
opponents lost life this turn +1/+1 counters` ("Other creatures you
control enter with an additional +1/+1 counter on them for each opponent
who lost life this turn."), `replace andlist creature or nonbasic land
your opponents control enter: tapped` ("Creatures and nonbasic lands your
opponents control enter tapped."); a count is the replacement source's, counted
as the permanent enters ("for each Angel you already control"), and the
permanent matches by its card under the controller it will have.

Affects: `any` (a permanent or player), `this`, `you`, `each opponent`,
`each player`, `you and creature you control`, a filter, a target (`target
creature`, `target 1`, or `that target 1` for "that creature"), a group
of targets (`up to 2 target creature`, `1 to 2 target creature`, `any
number of targets`: in an effect, one effect per target, locked to it),
or, as a damage source, `creature except target creature` ("creatures
other than target creature") and `chosen <filter> source` ("a red source
of your choice", `chosen source`, `chosen creature with shadow` ("a
creature of your choice with shadow"), `chosen permanent of chosen
color` ("a source of your choice of the chosen color"): chosen as the
effect is created, among permanents and spells on the stack, CR
609.7a). A damage source (`from`) is an object: `this`, a filter, or a
target. A filter that isn't of creatures prints as sources:
`from artifact` is "by artifact sources". A static prevention with no
effects after it prints "Prevent all damage that would be dealt to ...";
a static `prevent next 1` prevents 1 of every damage event (CR 615.1) and
prints "If a source would deal damage to a Cleric creature you control,
prevent 1 of that damage." (`replace damage to Cleric creature you
control: prevent next 1`; `from artifact to you` prints "If an artifact
would deal damage to you, ...");
with them (Phantom Tiger), "If damage would be dealt to ..., prevent that
damage.".

`this turn, replace damage to target creature: destroy next time` prints
"The next time damage would be dealt to target creature this turn, destroy
that creature instead." (a one-use replacement: the damage isn't dealt).
Outcomes: `prevent`, `prevent next 4`, `double`, `plus one`, `exile`,
`draw two`, `win`, `skip` (`replace you draw while your library has no cards in it: skip` prints "If you would draw a card while your library has no cards in it, skip that draw instead.", CR 614.10: the draw doesn't happen, so drawing from an empty library doesn't lose the game), `tapped`, `with 2 +1/+1 counters`, `regenerate`,
`redirect to attached`, `umbra armor` (`replace destroy enchanted: umbra
armor` prints "Umbra armor", CR 702.89a: if the enchanted permanent would
be destroyed, all damage is removed from it and the Aura is destroyed
instead; it applies even if the permanent can't be regenerated); in a `this turn, replace` effect also `prevent
next time` ("The next time a red source of your choice would deal damage
to you this turn, prevent that damage.": a shield for one damage event,
CR 615.7) and redirection (CR 614.9): `redirect to this`, `redirect to
target creature` (a new target, declared after the event's), and
`redirect next 1 to ...` ("The next 1 damage that would be dealt to this
creature this turn is dealt to target creature you control instead."),
`redirect next 1 to you` ("... is dealt to you instead.", Vassal's Duty),
`redirect next 1 to any target` (a new target), `redirect next time to
this` ("The next time a source of your choice would deal damage this
turn, that damage is dealt to Opal-Eye instead.": all of one damage
event, then it is used up; `this turn, replace damage from chosen source:
redirect next time to this`), `redirect to equipped`
(an Equipment's `redirect to attached`: "... is dealt to equipped
creature instead.", Pariah's Shield), and `redirect to chosen creature or
planeswalker you control` ("Choose a creature or planeswalker you control.
All damage that would be dealt this turn to you and permanents you control
is dealt to the chosen permanent instead (if it's still on the
battlefield).", Gideon's Sacrifice: chosen, not targeted, as the effect is
created). In a `this turn, replace` effect, where "this turn" prints
follows the event: `damage to A from S` "dealt to A by S this turn"
(Kjeldoran Royal Guard), `damage to A this turn from S` "dealt to A this
turn by S" (Oracle's Attendants), `damage this turn to A [from S]` "dealt
this turn to A [by S]" (Kor Chant, Saving Grace), and with `next N`,
`damage S would deal to A` "The next 2 damage that a source of your choice
would deal to you and/or permanents you control this turn ..." (`this
turn, replace damage chosen source would deal to you andor permanent you
control: redirect next 2 to any target`, Harm's Way; `you andor <filter>`
is `you and <filter>` printed "you and/or ..."). A static redirection
takes `while <condition>` after its event, printed first: `replace combat
damage from unblocked creature to you while this is untapped: redirect to
this` "As long as this creature is untapped, all combat damage that would
be dealt to you by unblocked creatures is dealt to this creature instead."
(Weathered Bodyguards). It does nothing if the
new recipient has left the battlefield or is no longer a creature or
planeswalker.

### Filters

A filter names a kind of permanent, in the singular: `creature`,
`creature you control`, `creature an opponent controls`, `creature you
don't control`, `white creature`, `nonblack creature`, `both white or
black spell` (every one of the colors, not just one: "Whenever you cast a
spell that's both white and black", the colors printed in the order
written), `nonbasic land`,
`other non-Aura enchantment`, `creature with flying`, `creature without
flying`, `creature with no abilities`, `all Sliver creature` (a static
ability's subject then prints "All Sliver creatures", as old Slivers do), `Goblin`, `Swamp you control`, `nonplaneswalker permanent you
control`, `enchanted` (the permanent this is attached to; `enchanted
permanent` prints "enchanted permanent" whatever the Aura enchants, as on
Song of Stupefaction's "Enchant creature or Vehicle"; on such an Aura the
recipient `enchanted permanent` prints so too, "tap enchanted
permanent"), `this` (the
source itself, for characteristic-defining abilities). Two alternatives
sharing two or more colors print the colors after the types: `blue or
red creature or blue or red planeswalker` is "creature or planeswalker
that's blue or red"; `other token creature or Rabbit creature you
control` is "other creature you control that's a token or a Rabbit"
(write `token creature`: a bare `token` is any token, a Treasure too).

`target creature you own` (CR 108.3: owned by the caster, whoever
controls it; `exile target creature you own then return it to
battlefield under your control`, Slip On the Ring) is judged for targets
and an effect's groups, by the caster or the effect's controller;
elsewhere it doesn't restrict.
`commander creature you own` ("commander creatures you own") and
`commander creature or planeswalker` ("a commander that's a creature or
planeswalker") match nothing: the game has no commanders (CR 903.3).

More filter words: `token` ("a token", "artifact token", CR 111.1),
`nontoken`, `tapped`, `untapped`, `attacking`, `blocking`, `nonattacking`
("target nonattacking creature": not an attacking creature),
`damaged` (also written `that was dealt damage this turn`),
`multicolored`, `monocolored`, `colorless`, `two-colored` ("that's
exactly two colors"), `basic`, `legendary`, `snow`,
`nonsnow`, `modified` (CR 700.9; `nontoken modified creature` prints
"nontoken modified creature", and a dies trigger judges `tapped`,
`token`/`nontoken`, counters, and `modified` as the creature last existed,
CR 603.10a), `adventurer` (CR 715.2a: a card with an Adventure, used or
not; `graveyard instant or sorcery or adventurer` counts "cards in your
graveyard that are instant cards, sorcery cards, and/or have an
Adventure"),
colors joined with `or` or listed (`red or green creature`, `red,
white, or black creature card`: sharing a color), types
joined with `or` or listed (`artifact or enchantment`, `artifact,
enchantment, or land`: at least one), negations listed with commas
(`nonartifact, nonblack creature`), and `with
power 2 or less`, `with toughness 4 or greater`, `with mana value 3`,
`with power or toughness 1 or less` (either is enough: "creatures you
control with power or toughness 1 or less"), `1/1 creature` (exactly that
power and toughness, printed before the noun: "Target 1/1 creature gets
+1/+2"), `with toughness greater
than power` ("with toughness greater than its power", "their power" in
the plural; `attached is with toughness greater than power` prints
"equipped creature's toughness is greater than its power"), `with mana value at most
this power` ("with mana value less than or equal to this creature's
power", "... to Alesha's power": the source's power as it is, or as it
last existed once it has left the battlefield, CR 608.2h; after `card`
too, `return target creature card with mana value at most this power
from your graveyard to battlefield`). A characteristic compared with a
count: `with power at most count equal creature you control` ("with power
less than or equal to the number of creatures you control"), `with
toughness less than count equal power` ("with toughness less than this
creature's power"), `with mana value equal to count equal charge
counters on this` ("with mana value equal to the number of charge
counters on this artifact", the counters it had once sacrificed),
`with mana value at most count where Faerie you
control` ("with mana value X or less, where X is the number of Faeries you
control", for a filter that ends the sentence), and `with power at most X`
("with power X or less", X of the spell or ability, CR 107.3); also on a
card (`another target creature card with mana value at most count equal
power from your graveyard`) and a spell (`counter target with mana value
at most count where Faerie you control spell`). The count is the source's,
for its controller, as the target is chosen and again as the effect
resolves (CR 608.2b); for a group, as the effect happens. A target's stat
(`power of that target 1`) can't be the count. As a target check, `if
target 1 is with toughness at most count equal graveyard` prints "if its
toughness is less than or equal to the number of cards in your
graveyard" (Ghastly Demise; `counter target spell if target 1 is with mana
value at most count equal greatest mana value`, a spell's mana value on
the stack), and `destroy target creature if target 1 is tapped` "if it's
tapped". `with total power and toughness 5 or less` ("target creature
with total power and toughness 5 or less", the sum of the two, Cut Down);
`except for tokens you control` ("Ajani deals 4 damage to each creature
except for tokens you control": `each single creature except for tokens
you control`; not a token the effect's controller controls, filled in
for an effect's groups, elsewhere matching nothing); `with basic land
type` ("search your library for a land card with a basic land type":
`search land card with basic land type reveal to hand`, a Plains, Island,
Swamp, Mountain, or Forest, CR 205.3i);
`that doesn't have same name as token you control` ("choose target
nontoken creature you control that doesn't have the same name as a token
you control", The Apprentice's Folly; `... as another permanent you
control`): no other permanent its controller controls that matches the
filter after `as` has its name (CR 201.2; a face-down permanent has
none); written last, the filter after it taking the rest.
In a triggered ability, a target can be related to the object that
triggered it: `with lesser mana value` ("Whenever this or creature you
control enters, return up to 1 target creature with lesser mana value you
control to hand": "... target creature you control with lesser mana
value ...", Clement; on a card, `return another target creature card with
lesser mana value from your graveyard to battlefield`, Jackdaw Savior: its
mana value less than the triggering object's, as it is or last existed)
and `other than that creature` ("put a +1/+1 counter on target creature
you control other than that creature", Pawpatch Recruit: neither it nor
the card it became). Both are judged as targets are chosen and again as
the ability resolves; elsewhere they don't restrict.
`with mana value 4 or 5` (either value: "search your library for an
artifact card with mana value 4 or 5", Transit Mage);
`with greatest power`, `with greatest toughness`, `with greatest mana
value` ("a creature with the greatest power among creatures they
control", "a creature or planeswalker with the greatest mana value among
creatures and planeswalkers they control": no other permanent its
controller controls that matches the rest of the filter has more, ties
all matching; on the battlefield only; in an edict, `target opponent
sacrifices 1 creature with greatest power` prints without "of their
choice", and `exiles 1 creature or planeswalker they control with
greatest mana value` prints the comparison after "they control"; with
`an opponent controls` it prints "among creatures that player
controls");
`colored` ("target creature that's one or more colors", not colorless,
CR 105.2; also written `that's one or more colors`).
A chosen color or type comes last: `creature you control of chosen color`
("creatures you control of the chosen color", the color the source chose
as it entered, CR 607.2a), `creature of chosen type`, `creature not of chosen type` ("Destroy all
creatures that aren't of the chosen type.", Kindred Judgment: the choice
excluded, a changeling being of it; `not of that type` likewise), `permanent you
control of that type` (chosen earlier as the effect resolved; with `late` after
the controller the choice prints first: `count each permanent you control
late of chosen type`, "for each permanent of the chosen type you control"), `enchantment
of color of choice`, `creature of creature type of choice` (chosen as the
effect resolves). With nothing chosen, nothing matches; a delayed trigger
doesn't keep what its ability chose, so `of that color` there is an error
(Zombie Boa). `named this` is
"named ~", the card's own name (CR 201.4b), printed after the controller
(`creature you control named this`: "creature you control named Timberpack
Wolf") unless written before it (`creature named this you control`); `not
named this` is "not named ~" (`target permanent not named this`). `named "Darkstar"` is a token's name ("you don't
control a creature named Darkstar"), the token definition keyed by it. Cards
take the name after the noun: `search card named this ...`, `target card
named this from your graveyard`, `graveyard named this` ("cards named ~ in
your graveyard"). A face-down permanent has no name (CR 708.2). As an effect's
recipient, `all permanent ...` prints "all permanents ... to their owners'
hands" where `each` prints "each permanent ... to its owner's hand"; so
do `sacrifice all non-Ogre creature you control`, `exile all artifact your
opponents control until this leaves`, and `remove all counters from all
permanent` ("all ...s" where `each` prints "each ..."). Who controls it
prints before the qualities ("target creature an opponent controls with
flying"); written with `late` after it, after them (`target creature
with flying an opponent controls late`: "target creature with flying an
opponent controls"); wording only. Negations print with commas, a
supertype's first ("nonlegendary, nonland card", "nonland, nontoken
permanent"); a historic target is "target historic permanent". A
nonland permanent card names what it isn't: `nonland noninstant
nonsorcery permanent card` ("a nonland permanent card"; `nonland
permanent card` is a nonland card).

Game-state qualifiers (`ObjFilter::status`, judged with the game state like
`tapped`): `blocked` and `unblocked` ("target blocked creature": an
attacking creature once blockers are declared, CR 509.1h; before then it is
neither), `with aura` and `without aura` ("target enchanted permanent",
"creature that isn't enchanted": an Aura is attached to it, CR 303.4; who
controls the Aura isn't checked, so `with aura you control` is an error; the
word `enchanted` is the permanent this is attached to), `that entered this
turn` (its timestamp is from this turn, CR 613.7e; `as long as this is that
entered this turn` prints "As long as this creature entered this turn"),
`that dealt damage this turn` (it was the source of damage this turn),
`that blocked this turn` (declared as a blocker this turn), `that
attacked this turn` (declared as an attacker this turn: "Untap all
creatures that attacked this turn."), `dealt damage
this turn` (`damaged` printed "creature dealt damage this turn"), `dealt
damage by this this turn` ("a creature dealt damage by this creature this
turn": the source dealt it damage this turn; only a dies trigger's subject,
`triggered whenever creature dealt damage by this this turn dies`, which
looks back, CR 603.10a, and a dies replacement's, `replace creature dealt
damage by this this turn dies: exile`: "If a creature dealt damage by this
creature this turn would die, exile it instead."), and
`with mana value X` / `with mana value X or less` (X of the spell, CR
107.3; X is chosen before targets, CR 601.2b, so only an X some object
fits is offered, a spell on the stack too: `counter target with mana
value X spell`, "Counter target spell with mana value X"; on a card, the mana value prints after the noun: `target
creature card with mana value X or less from your graveyard`; several targets print "creature cards with mana value 2 or less", and `card each with mana value 3 or less` "creature cards each with mana value 3 or less"), `with +1/+1
counter` / `with counter` ("creature you control with a +1/+1 counter on
it", "with a counter on it", CR 122.1; plural "with +1/+1 counters on
them"; printed after the controller; any kind: `permanent you control with
oil counter`, "permanents you control with oil counters on them"), `with
3 or more level counters` ("a creature with three or more level counters
on it"), `with no counters` ("target creature with no counters on it": none
of any kind, Heartless Act), and `modified` ("a modified
creature", CR 700.9: it has a counter, is equipped, or is enchanted by an
Aura its controller controls), `attached to creature` / `attached to land`
("target Aura attached to a creature": what it is attached to, CR
303.4), `face-down` ("target face-down creature", CR 708.2),
`transformed` ("for each transformed permanent you control", "As long as
you control a transformed permanent": a double-faced permanent with its
back face up, CR 701.27g; a permanent turned back to its front face isn't
one), and `with toxic` ("each other creature you
control with toxic", CR 702.164: one of its abilities is toxic N, printed
or granted; printed after the controller). As a check on a target or what
an Aura enchants it prints "has": `if that target 1 is with toxic, draw 1
card` ("If that creature has toxic, draw a card."), `deal 2 damage to
target creature; if that target 1 is with toxic, deal 4 damage to that
creature instead`, `triggered when this enters if attached is with toxic:
enchanted creature gains hexproof` ("When this Aura enters, if enchanted
creature has toxic, that creature gains hexproof until end of turn."; an
Aura spell enters attached to its target, CR 303.4f, so the condition
sees it). A group written `each single <filter>` prints one at a time
("Each other creature you control with toxic gains toxic 1"), also as
an object ("Destroy each creature with mana value 3 or less.", `untap
each single other creature you control`). A group written `every
<filter>` prints "all" as a subject too (`every other creature gets
-2/-2`: "all other creatures get -2/-2"; `all` alone prints the plural
without it); wording only. `creature
with aura or enchantment creature` is two alternatives: a type followed by
another starts a filter of its own. In a static ability's subject these
qualifiers are re-judged only when characteristics change, so they belong
in targets, groups, and conditions.

Combat relations to the source (`ObjFilter::combat_with`, CR 509.1g):
`blocking this` ("each creature blocking this creature"), `blocking it`
("target creature blocking it", the source named before), `blocked by this`
("target creature this creature is blocking"), `blocked by it` ("target
creature it's blocking"), `blocking or blocked by this`. Judged for targets
and an effect's groups; once the source has left the battlefield (a
sacrifice cost), as it last was, so the relation no longer restricts (CR
113.7a). A group with a combat relation is a subject one at a time: `each
creature without flanking blocking this gets -1/-1` prints "Each creature
without flanking blocking this creature gets -1/-1". `if target 1 was
damaged, ...` prints "If it was dealt damage this turn, ..." (the damage on
it as it last existed).

### Modifications (static abilities)

Comma-separated, printed in the order written ("gets +1/+1, has reach,
and is a Giant in addition to its other types"; `has can't be blocked,
has shroud` prints "can't be blocked and has shroud"), each in one layer
of CR 613: `+1/+1`, `base 1/1`,
`base mv` (base power and toughness each equal to its mana value),
`has flying, trample`, `loses all abilities`, `add type creature`,
`loses flying`, `set land Mountain`, `set land Mountain Forest Plains` ("is a
Mountain, Forest, and Plains": all of them and no other), `add land Swamp`,
`add subtype Saproling`, `add supertype snow` ("Enchanted land is snow.",
layer 4: a supertype added to its others, CR 205.4; `add supertype
legendary, add subtype Soldier` is one phrase, "is a legendary Soldier in
addition to its other types"), `goaded` ("is goaded", CR 701.15: attacks each
combat if able, and a player other than the source's controller if able),
`control`
(you control it), `add color white`, `set color blue`, `set color chosen`,
`has protection from chosen color`, `set land chosen` ("Enchanted land is
the chosen type") and `add chosen type` ("is the chosen type in addition
to its other types"; `add chosen creature type`: "the chosen creature
type", wording only; `static plural Vehicle creature you control: add
chosen creature type` prints "Vehicle creatures you control are the
chosen creature type in addition to their other types", the static
finding what it applies to after the crew effect it depends on, CR
613.8) (the color or type the source chose as it
entered), `switch pt`,
`cda color all`, `cda color colorless` ("Ghostfire is colorless.": a
color-defining ability, CR 604.3, so the card is colorless in every zone,
the stack included), `cda pt card types in graveyards +1`,
`cda pt creature cards in graveyards +1`, `cda pt count Swamp you control`,
`cda pt your life`, `cda pt domain`, `cda pt 2 hand` ("... each equal to
twice the number of cards in your hand") (a power/toughness-defining ability,
layer 7a, counting any count a static ability can (a count only a resolving effect has, such as `drawn this turn`, `greatest power ...`, `party`, or a target's or the triggering object's, is a compile error in a static ability: it would read 0): "~'s power and
toughness are each equal to ..."; the count is judged from its controller
as the layer applies), `cda power count Rat you control` ("~'s power is
equal to the number of Rats you control"; the toughness is printed, `pt
*/1`), `cda toughness domain` ("~'s toughness is equal to ..."; `cda noun power
...` names the card's noun instead, "This creature's power is equal to
...", "This Vehicle's power", where Oracle does), `has all activated
abilities of [all|each] <count>` (layer 6, CR 613.1f: the activated
abilities, mana abilities included, of the objects the count counts;
`all` and `each` are wording: "of all Elf cards in your graveyard",
"of each other creature with a +1/+1 counter on it", and without either
"of creatures you control ..."). The count is permanents (`creature you
control not named this`: "creatures you control that don't have the
same name as this creature"; their abilities as they have them,
granted ones included, CR 613.8a), `graveyard Elf` ("Elf cards in your
graveyard"), `all graveyards creature`, or `exiled with this creature
card` ("creature cards exiled with it"); cards in a graveyard or in
exile give their printed abilities, a land card the mana ability of
each of its basic land types (CR 305.6), and abilities that work only
from a hand or a graveyard (cycling, unearth) are left out. `has all
activated abilities of the exiled card` is `exiled with this` printed
"the exiled card" (Territory Forge); `has all activated abilities of
that card` reads the top card of your library (`static as long as top
card is Goblin, this: has all activated abilities of that card`,
Conspicuous Snoop: its printed abilities, while it is on top); either makes what the card's
abilities exile linked to it (CR 607.2a). `has all activated abilities
as long as on battlefield of ...` prints "As long as this creature is on
the battlefield, it has all activated abilities of ..." (Necrotic Ooze).
A granted ability's counts are its own card's ("-X/-X, where X is the
number of Elves you control" read for the permanent that has it), and a
permanent holds at most eight abilities in all; and `has ability { <ability line> }`: a quoted ability
granted in layer 6 ("Enchanted creature has \"{T}: This creature deals 1
damage to any target.\""), written as its own ability line (`activated
{T}: deal 1 damage to any target`), where `this` is the object that has
it; "this permanent" when the group names only a subtype (`static all
Sliver: has ability { activated {2}: regenerate this }` prints "All
Slivers have \"{2}: Regenerate this permanent.\""); "this land", "this
enchantment", "this artifact" when the group is only lands, enchantments,
or artifacts (`static artifact you control: has ability { activated {2},
sacrifice this: ... }`: "Sacrifice this artifact"). A granted loyalty
ability brackets its cost (`static planeswalker you control: has ability
{ loyalty -4: ... }`: "Planeswalkers you control have \"[−4]: ...\"").
A granted static ability applies after the grant, in layer 6 and later
(`static equipped: +0/+1, has ability { static this: has hexproof as
long as this is untapped }`, Giant's Amulet); one that would change
types, colors, or control (layers 1 to 5) is a compile error.
In a quoted ability an Aura or Equipment grants what it is attached to
(`static enchanted:` or `static equipped:`), `granter` is that Aura or
Equipment, printed by its name: the cost `sacrifice granter` ("{T},
Sacrifice Blazing Torch: ..."), `granter deals 2 damage to any target`
("Blazing Torch deals 2 damage to any target": the Equipment is the
damage's source, as it last existed once sacrificed), `return granter to
hand` ("Return Trusty Boomerang to its owner's hand"), `sacrifice
granter` and `destroy granter` as effects (`you may sacrifice granter;
when you do, this creature deals 2 damage to any target`: with the
Equipment gone or another player's, it can't be done, CR 701.21a). Each
grant is its own ability (CR 113.2c): two Blazing Torches give the
creature two, each sacrificing its own; an activation needs its granter
attached (and, to sacrifice it, controlled by the activating player).
The costs `unattach granter` ("{T}, Unattach Leonin Bola: Tap target
creature": the Equipment moves off the creature and stays on the
battlefield, CR 701.3d; whoever controls it) and `tap granter` ("{1},
{T}, Tap Fishing Pole": it must be untapped). `granter as it` is the
granter printed "it" ("{T}, Unattach Razor Boomerang: It deals 1 damage
to any target"); `controller of granter` its controller, as it last
existed once gone ("{7}: Hold for Ransom's controller sacrifices it and
draws a card": `controller of granter sacrifices granter as it and draws
1 card`, the Aura's controller sacrificing it and drawing, though the
creature's controller activates the ability). `put 1 bait counter on
granter` and `remove 1 aim counter from granter` act on it ("{1}, {T},
Tap Fishing Pole: Put a bait counter on Fishing Pole"). The filter word
`other than granter` excludes it from what a resolving ability chooses
(`you may sacrifice 1 artifact other than granter chosen`: "you may
sacrifice an artifact other than Dire Blunderbuss"). A reflexive ability
(`when you do, granter deals 1 damage to any target`) names the same
granter as the ability that creates it; a delayed one can't name it (an
error).
`granter` is an error elsewhere, beside a cost that chooses a permanent
or card, and in other effects.
A token a quoted ability creates prints its full description. A replacement or
prevention ability may be granted (absorb). `has protection from
multicolored`, `has protection from artifact` grant protection from a
quality (CR 702.16); as an effect, `target creature gains protection from
artifact`, and `target permanent you control gains protection from
artifact or from color of choice` ("... gains protection from artifacts or
from the color of your choice until end of turn": the controller chooses
one as it resolves, `choose "1"` or `choose "2"` in a scenario). `this becomes creature type of choice in addition` prints "...
becomes the creature type of your choice in addition to its other types"
(Mistform Sliver). A replacement outcome `hand` prints "return it to its
owner's hand instead" (`replace <affects> dies: hand`). `has
ward {2}` and `has toxic 1` grant those keyword abilities bare
("Enchanted creature gets +4/+4 and has trample and ward {1}."; CR 702.21,
702.164); `has ward pay 2 life` grants a ward whose cost isn't mana,
quoted ('Artifacts you control have "Ward—Pay 2 life."'); as an effect, `<what> gains toxic 1` ("gains toxic 1 until end
of turn"). Toxic values add up (CR 702.164c). `add color black, add
subtype Zombie` together print "is a black Zombie in addition to its other
colors and types". Lure among other modifications prints "all creatures
able to block it do so" ("As long as enchanted creature is green, it gets
+1/+1 and all creatures able to block it do so."). `as long as attached is
black, enchanted: ...` is "As long as enchanted creature is black, it
...".

#### Auras and Equipment

`loses all abilities, set color blue, set subtype Frog, set type
creature, base 1/1` prints "loses all abilities and is a blue Frog
creature with base power and toughness 1/1" (Frogify): `set type` replaces
all its card types, land included, keeping its supertypes (layer 4), and
it loses the subtypes of the types it lost (CR 205.3d: no creature types
unless it is still a creature, no basic land types unless still a land);
`set colorless` is "colorless" ("loses all abilities, set colorless, set
subtype Noggle, base 1/1": "... is a colorless Noggle with base power and
toughness 1/1", Noggle the Mind); `set colorless, set type land, has
ability { activated {T}: add {C} }, loses all abilities` prints "is a
colorless land with \"{T}: Add {C}\" and loses all other card types and
abilities" (Imprisoned in the Moon; `set subtype Food, set type artifact`
in the middle, "a colorless Food artifact with \"...\"", Sugar Coat: an
"Enchant creature or Food" Aura says "Enchanted permanent");
colors, a creature type, and card types set together are one noun
phrase, with the base P/T after it. `add subtype every` is "is every
creature type" and `add subtype all` "is all creature types" (layer 4,
not changeling: it has no ability, and "loses all abilities" leaves it);
`loses all creature types` removes them (a changeling's too). As effects,
`target creature gains all creature types` and `target creature loses all
creature types` (until end of turn). `this: add subtype Cleric, add
subtype Rogue, ...` (two or more) prints "~ is also a Cleric, Rogue, ..."
and the types are the card's in every zone. A group `creature you control
with aura late` prints "creatures you control that are enchanted"
(Magemarks; `with aura` prints "enchanted creatures"). Wording-only
order: `attacking with Equipment late creature` prints "attacking equipped
creature" (`with Equipment` alone, "equipped attacking"), and `attacking
late nonwhite creature` "nonwhite attacking creature". "You control"
prints before a group's qualities: "other creatures you control with
defender", "creatures you control with power 5 or greater".

`has defender, loses all abilities` prints "has defender, and loses all
other abilities" (Deep Freeze): abilities a static ability grants before
`loses all abilities` are kept (the loss applies first in layer 6).
`has can't activate` ("its activated abilities can't be activated", CR
602.5: mana abilities too) and `has can't activate except mana` ("...
unless they're mana abilities") are restrictions like `can't attack`;
they print last, after ", and" ("Enchanted creature can't attack or
block, and its activated abilities can't be activated"; "and" alone after
`doesn't untap`), or alone as "Enchanted creature's activated abilities
can't be activated."; on a group of several types, `static artifact,
creature, or planeswalker: has can't activate`, "Activated abilities of
artifacts, creatures, and planeswalkers can't be activated." `set subtype Treefolk` is "is a Treefolk" (it loses
its other creature types, CR 205.1b); `add land all` is "is every basic
land type in addition to its other types". A type followed by a base P/T
is one phrase: `set subtype Treefolk, base 0/4, loses all abilities`
("is a Treefolk with base power and toughness 0/4 and loses all
abilities"), `add type creature, base 4/4` ("is a creature with base power
and toughness 4/4 in addition to its other types"); `this: base 5/5, add
subtype Golem, add type artifact creature as long as metalcraft` is one
noun phrase ("This artifact is a 5/5 Golem artifact creature as long as
..."). `as long as attached
is creature, enchanted: +2/+1` prints "As long as enchanted permanent is a
creature, it gets +2/+1." (the noun is what the Aura enchants); `attached
is Human or Angel` is "a Human or an Angel"; in a trigger condition
`attached is with power 4 or greater` is "enchanted creature's power is 4
or greater".

A filter may say what its objects are attached to: `attached to this`
(the source: "for each Aura and Equipment attached to it", written `count
each Aura or Equipment attached to this`), `attached to attached` (what
the source is attached to: Golem-Skin Gauntlets' "for each Equipment
attached to it"), `attached to target creature` (declaring the target:
`destroy all Aura or Equipment attached to target creature`), `attached
to target 1` ("it": `return target creature and all Aura attached to
target 1 to hand`), `attached to that target 1` ("that creature"), and,
in a triggered ability, `attached to that creature` (the triggering object:
"destroy all Auras attached to that creature").

In a triggered ability, `triggering it` is the triggering object printed
"it" even after a target ("Whenever enchanted creature attacks or
blocks, attach any number of target Equipment you control to it":
`attach any number of target Equipment you control to triggering it`,
where `it` would be the last target). `triggering card` is the card the
triggering object became ("When enchanted creature dies, return that card to its
owner's hand": `return triggering card to hand`; only that card, CR
400.7). With no targets, `its controller` is the triggering object's
controller as it last existed ("When enchanted creature dies, its
controller draws a card"); `controller of that creature` prints "that
creature's controller" ("that land's controller" for an Aura that
enchants a land). `return triggering card to battlefield tapped under
your control` takes `tapped` and `under your control` (else "under its
owner's control"). A target card from a graveyard that isn't yours (`target
creature card from a graveyard`, `... from an opponent's graveyard`)
returned `under your control` prints "put ... onto the battlefield under
your control"; `put that target 1 onto battlefield tapped` is the same
effect worded "put that card onto the battlefield tapped" (Zuko's
Conviction). `return this to battlefield transformed under your control`
(and `tapped and transformed`; from the graveyard, `activated ..., from
graveyard: return this to battlefield transformed`) returns a transforming
double-faced card with its back face up (CR 712.14a): "Exile this Saga,
then return it to the battlefield transformed under your control" is
`exile this then return this to battlefield transformed under your
control`; "When this creature dies, return it to the battlefield tapped and
transformed under its owner's control". Any other card stays where it is.
The source is found as the card it became by being exiled or dying, but
not once that card has left the graveyard or exile again (CR 400.7: the
delayed `delayed this: return this to battlefield transformed ... at
beginning of the next end step` of Loyal Cathar misses a card exiled from
the graveyard first). `... front face up` is the same return worded "return
it to the battlefield (front face up)" (wording only: a back face's "Exile
Shiva, then return it to the battlefield (front face up).").
`it deals 1 damage to each opponent` makes the
triggering creature the damage's source (CR 120.3: Hawkeye's Bow), and
`that defending player` prints "defending player" where `defending
player` as an object prints "the player or planeswalker it's attacking".
`enchanted creature gains attacks each combat if able` prints "Enchanted
creature attacks this turn if able."

### Effects

Effects are clauses. `;` separates sentences; `and` joins clauses into one
sentence ("target player loses 1 life and you gain 1 life") and `then`
joins them with ", then" ("draw 1 card then discard 1 card"); a clause
after `and` may leave its subject implied ("you draw 1 card and lose 1
life"). A list of three or more clauses separates them with `,`: `target
opponent discards 2 cards, mills 1 card and loses 1 life` prints "Target
opponent discards two cards, mills a card, and loses 1 life." (the last
`and` takes the serial comma), `you draw 2 cards, lose 2 life, then mill 2
cards` prints "..., lose 2 life, then mill two cards." `draw 2 cards then discard
1 of them` is one effect: "Draw two cards, then discard one of them."
(only a card drawn this way can be the one).
Numbers are digits, `X`, or a count: `count where <expr>` ("X ...,
where X is the number of ..."), `count equal <expr>` ("... equal to the
number of ..."; `count equal 2 Mountain returned this way`: "equal to
twice the number of Mountains returned this way", the count doubled), or
`count each <expr>` ("... for each ..."), with
`<expr>` a filter (`creature you control`, `Elf on battlefield`,
`creature named this on battlefield`: "creatures named ~ on the
battlefield", `other creature on battlefield named this`: "other creature
on the battlefield named ~"),
`domain`, `hand`, `graveyard [<filter>]`, `all graveyards [<filter>]` /
`each graveyard [<filter>]` ("cards named ~ in all graveyards", "for each
card named ~ in each graveyard"), `plus 2 <expr>` ("2 plus the number of
..."; `plus one <expr>` spells the number), `power` (the source's), `power
of target 1` / `toughness of target 1` ("its power"; `power of that
target 1` is "that creature's power"; read as the target last existed if
it has left), `mana value of target 1` ("its mana value", X included on
the stack; `mana value of the spell target 1` is "the spell's mana
value"), `greatest power creature you control` ("the greatest power
among creatures you control"), or `life`; `this gets +1/+1 count each creature you
it has left), `mana value of target 1` ("its mana value"), `toughness`
(the source's), `life`, `speed` ("your speed", CR 702.179: 0 with no
speed; `search up to count where speed basic land card to battlefield
tapped` prints "Search your library for up to X basic land cards, where X
is your speed. Put those cards onto the battlefield tapped, then
shuffle."), `party` ("creatures in your party", CR 700.8: one
each of Cleric, Rogue, Warrior, and Wizard at most), `devotion green`
("your devotion to green", CR 700.5: the green mana symbols in the mana
costs of permanents you control, a hybrid symbol counting once;
`devotion white and blue` counts symbols of either), `red mana symbols`
(chroma: "the number of red mana symbols in the mana costs of permanents
you control", the same count), `greatest power`
("the greatest power among creatures you control"), `greatest mana value`
("... among permanents you control"), `drawn this turn` ("cards you've
drawn this turn"), `spells cast this turn`, `other spells cast this turn`, `other
instant and sorcery spells cast this turn` ("other instant and sorcery
spell you've cast this turn"), `other instant and sorcery spells cast
before it this turn` ("... you've cast before it this turn": before the
triggering spell)
("the number of other spells you've cast this turn": not the resolving
spell itself), `colors among permanents` ("colors among permanents
you control"; vivid, `label "Vivid"`; also a characteristic-defining
power, `static this: cda power colors among permanents`), `colors spent` (converge: "the number of colors of mana
spent to cast this spell"), `treasure spent` ("for each mana from a
Treasure spent to cast it"), `opponents graveyards [<filter>]` ("creature cards in your
opponents' graveyards"), `exiled with this [<filter> card]` ("cards
exiled with this enchantment", CR 607.2a), `exile [<filter> card]`
("cards you own in exile"; `static this: cda pt exile +1` is Cosmogoyf),
`exile and graveyard instant or sorcery card` ("instant and sorcery
cards you own in exile and in your graveyard", "the total number of
..." as a number), `opponents attacked` ("opponents you're
attacking"), `attacked with this turn` ("creatures you attacked with this
turn"), `Zubera died this turn` ("for each Zubera that died this turn",
"the number of Zubera that died this turn": creatures put into a graveyard
from the battlefield this turn, matched as they last existed, CR 700.4),
`greatest power graveyard creature` ("the greatest power among creature
cards in your graveyard"; 0 with none), `graveyard permanent card` ("for
each permanent card in your graveyard", CR 110.4a), `card types in
graveyards` ("card types among cards in all graveyards"), `+1/+1 counters
on this` ("the number of +1/+1 counters on this creature", "for each
scream counter on this Aura"; once the source has left the battlefield,
the counters it had there, CR 608.2h) and `age counters on it` ("... on
it"), `attached controller graveyard [<filter>]` ("the number of creature
cards in its controller's graveyard": the controller of what the source
is attached to), `creature target player controls` ("for each creature
target player controls": declares the target there; `target opponent
controls` likewise), `life you gained this turn` ("the amount of life you
gained this turn": all the life gained, whatever was lost; `life you've
gained this turn` prints "you've gained"), `card types in your graveyard`
("the number of card types among cards in your graveyard"), `counters on
this` ("the number of counters on Savanti Romero": every kind, as for
`+1/+1 counters on this`; in a triggered ability `counters on it` is the
triggering creature's, "the number of counters on it", Spider-Man Noir), `permanents sacrificed this turn` ("the number of
permanents you've sacrificed this turn"), `two to x` (`count where two to x`
prints "2ˣ": "Target player draws 2ˣ cards."), `creature attacked this
turn` ("for each creature that attacked this turn": any player's
attackers), `opponents lost life this turn` ("for each opponent who lost
life this turn"), `opponents more cards in hand` ("... who has more cards in
hand than you"), `opponents hand 1 or less` ("for each opponent who has one or fewer
cards in hand"; in a "your draw step" trigger `draw count each opponents
hand 1 or less cards` prints "draw an additional card for each ..."), `total power creature you control` ("the total power of
creatures you control"; also `total toughness` and `total mana value`; a
negative total is 0, CR 107.1b; in a static ability only `total mana value`
counts, power and toughness reading 0), or `andor <expr>` (the same count, printed "artifact and/or
creature card"); two clauses of one sentence with the same `count where`
share one X ("deals X damage to any target and you gain X life, where X
is ..."); in a static ability `-1/-1 count where hand` prints "gets -X/-X,
where X is ...", and `this spell costs {1} less count where greatest
power` prints "costs {X} less to cast, where X is ..."; `this gets +1/+1 count each creature you
control` scales a pump, and in a static ability `+1/+1 count each
artifact you control` is "gets +1/+1 for each artifact you control"; "until end of turn" is implied by `gets`,
`gains`, `becomes`, `has base power and toughness`, and `gain control of`
and may be written or not. A sentence that starts `until end of turn,`
prints the duration first instead ("Until end of turn, target creature you
control has base power and toughness 4/4 and gains flying").

Counts over a group of battlefield permanents (judged with their current
characteristics; in a static ability, as the layers have them): `colors
among Ally you control` ("for each color among Allies you control", "the
number of colors among other artifacts you control"; colorless adds
none), `card types among other nonland permanent you control` ("the
number of card types among other nonland permanents you control"),
`+1/+1 counters on land you control` ("the number of +1/+1 counters on
lands you control"; `counters on creature you control` counts every
kind), `counter kinds among creature you control` ("different kinds of
counters among creatures you control"), `different powers among creature
you control` ("for each different power among creatures you control";
resolving effects only), and `most sharing type creature you control`
("the greatest number of creatures you control that have a creature type
in common": a changeling shares every type). Others: `card types among
spells cast this turn` ("for each card type among spells you've cast this
turn", from the cards you cast), `card types among discarded this way`
("for each card type among cards discarded this way", as they last
existed), `permanent types in your graveyard` (artifact, battle,
creature, enchantment, land, planeswalker: "permanent types among cards
in your graveyard"), `graveyards with 7 cards` ("for each graveyard with
seven or more cards in it"), `difference of power and toughness` ("where X
is the difference between its power and toughness": the triggering
creature's), `2 land you control` with `count where` ("where X is twice
the number of lands you control"), `greatest power other creature you
control` ("the greatest power among other creatures you control"), and
`<count> plus <count>` with `count each` ("for each Artificer you control
and each Artificer card in your graveyard": `count each Artificer you
control plus graveyard Artificer`). `half up x` as a number is "half X,
rounded up" (`create half up x "Food"`: "create half X Food tokens,
rounded up"; `half down x`, "rounded down"). As a condition, `count 5
colors among permanents` is the count at least N: "there are five colors
among permanents you control" (`to solve count 5 ...`; an activation's
`if count 4 permanent types in your graveyard`: "Activate only if there
are four or more permanent types among cards in your graveyard"; `if count
3 other Cave you control plus graveyard Cave`: "if the number of other
Caves you control plus the number of Cave cards in your graveyard is three
or greater"), judged as a static ability's counts are. `add chosen count
each <count>` is "Choose a color. Add one mana of that color for each
..." (one color, as `add any count ...`). After a trigger with an amount,
`choose up to that much target creature you control; put 1 +1/+1 counter
on each of them` is "choose up to that many target creatures you control.
Put a +1/+1 counter on each of them." (the amount as the group's
maximum, known as the targets are chosen); "Whenever you gain life,
target opponent loses that much life" reads the life gained.

Casting as an effect (CR 608.2g: cast as the effect finishes, timing
restrictions ignored): `cast free noncreature spell from hand` ("you may
cast a noncreature spell from your hand without paying its mana cost";
the controller chooses the card, or none), `cast free spell from hand
with mana value up to 4` ("a spell with mana value 4 or less from your
hand"; `up to X` likewise), `cast free spell from hand with mana value up
to that much` ("a spell from your hand with mana value less than or equal
to that damage"), `cast free artifact spell from hand with mana value up
to count where ...` ("... less than or equal to the number of ..."), and
`cast free target instant or sorcery card from your graveyard` ("you may
cast target instant or sorcery card from your graveyard without paying
its mana cost"), `cast free card exiled with this` ("you may cast a card
exiled with Krang & Shredder without paying its mana cost": the
controller chooses one of the cards this permanent's abilities exiled,
CR 607.2a, or none). `exile instead` after either adds "If that spell would
be put into your graveyard, exile it instead." (it is exiled as it
resolves or is countered). `cast free exiled card with mana value up to 4`
is "you may cast the exiled card without paying its mana cost if that
spell's mana value is 4 or less" (hideaway's `cast free exiled card` with
a cap; `search card to exile face down` links its card to the spell, CR
607.2a, so it is "the exiled card"), and `put exiled card into hand unless
cast` "Put the exiled card into your hand if it wasn't cast this way."
(unless its controller chose to cast it; Beseech the Mirror). `cast free
exiled card that is instant spell with mana value up to 2` is "you may
cast the exiled card without paying its mana cost if it's an instant
spell with mana value 2 or less" (judged on the spell it would be, even
face down, CR 406.3b), and `put exiled card into hand if not cast` "If
you don't, put that card into your hand." (Discover the Impossible;
`put exiled card into hand otherwise`: "Otherwise, put it into your
hand.", `... if not cast this way`: "If you don't cast that card this
way, put it into your hand.").

Casting from among the cards the resolution just exiled or looked at
(CR 608.2g: as the effect finishes, timing ignored): `cast now that card`
("You may cast that card.": its mana cost and other costs paid as
usual, offered only if they can be; `cast now it` "You may cast it.")
after `exile top 1 card`, `exile until nonland card` or `look at top
card` (the card on top of the library is cast from there); `cast now
that card by discarding` ("You may cast that card by discarding a card
rather than paying its mana cost.": an alternative cost, X is 0, the
card discarded chosen as jump-start's is, `cast "X" discarding "Y"` in
scenarios; offered only with a card to discard); `cast free it` / `cast
free that card` ("You may cast it without paying its mana cost.") with
`with mana value up to 8` ("... if that spell's mana value is 8 or
less"; a count prints "if the spell's mana value is less than or equal
to the amount of life you gained this turn", Bre of Clan Stoutarm) or
`with mana value below count where Mountain you control` ("... if the
spell's mana value is less than the number of Mountains you control");
`cast free up to 2 spell from among exiled cards` ("You may cast up to
two spells from among the exiled cards without paying their mana
costs.", Doom Reigns Supreme), `cast free any number spell from among
them` ("You may cast any number of spells from among them ..."; `from
among those cards`: "from among those cards", Etali; a filter first:
`any number andor instant or sorcery spell`, "any number of instant
and/or sorcery spells"): each card the resolution exiled that is still
in exile and could be cast is offered in turn (scenarios: `decline "X"`
skips one), up to the number. `if you dont, ...` after any of these
happens if none will be cast ("If you don't, Chandra deals 2 damage to
each opponent.": a land can't be cast, so it counts as not cast).
`may cast cards exiled this way this turn free` is "Until end of turn,
you may cast cards exiled this way without paying their mana costs."
(`may cast those cards this turn free`: "... cast those cards ..."): every
card the resolution exiled, still in exile, may be cast by the
controller this turn with its usual timing (Dream Harvest).
`dig face down 5 them` is hideaway's body as an effect: "Look at the top
five cards of your library. Exile one of them face down and put the rest
on the bottom of your library in a random order." (the controller
chooses, scenarios `choose "Shock"`, and may look at it; it is "the
exiled card", linked to the source); `dig face down 4 then any order`
prints "look at the top four cards of your library, exile one face down,
then put the rest on the bottom of your library in any order" (Clone
Shell; a random order is one of its orders). `turn face up exiled card
put creature onto battlefield` is "turn the exiled card face up. If it's
a creature card, put it onto the battlefield under your control." (a
face-down exiled card turned face up stays in exile, known to every
player, CR 406.3).

Other durations (CR 611.2b) end a sentence, and whatever in it would last
until end of turn (or "this turn") lasts that long instead: `until your
next turn` (it ends as the controller's next turn begins: `each creature
your opponents control gets -3/-0 until your next turn`, "Creatures your
opponents control get -3/-0 until your next turn"; `until your next turn,
up to 1 target creature gets -5/-0` prints it first; `until your next turn,
prevent all damage to you this turn` prints "Until your next turn, prevent
all damage that would be dealt to you.", Morningtide's Light), `for as long as this
remains` ("for as long as this creature remains on the battlefield", "...
The Wondrous Wasp remains ..."), `for as long as you control this` ("for as
long as you control this creature"), `for as long as it remains tapped`
(each affected permanent, while it stays tapped), and `for as long as this
remains tapped` ("for as long as this creature remains tapped": while the
source stays tapped; Phyrexian Gremlins' `tap target artifact; target 1
gains doesn't untap for as long as this remains tapped`). The last four end when
the source leaves the battlefield or changes control, or the permanent (or
source) untaps (checked with the state-based actions), and do nothing if that has
long as you control this creature"), and `for as long as it remains tapped`
(each affected permanent, while it stays tapped), and `until this is cast
from exile` ("until this card is cast from exile", after an `exile this
from hand` cost: while that card stays in exile; with `may cast this while
exiled`, "You may cast this card for as long as it remains exiled.", its
controller may cast it from there, CR 601.3). The last three end when
the source leaves the battlefield or changes control, or the permanent
untaps (checked with the state-based actions), and do nothing if that has
already happened as the effect would begin. `gain control of target
creature for as long as you control this`, `that target 1 can't attack or
block for as long as you control this` ("That creature can't attack or
block for as long as ..."), `prevent all damage from up to 1 target
creature this turn for as long as this remains` ("Prevent all damage that
would be dealt by up to one target creature for as long as this Saga
remains on the battlefield"). Clauses for these: `target 1 loses all
abilities` ("It loses all abilities", layer 6), `that target 1 gains
doesn't untap` ("That creature doesn't untap during its controller's untap
step"; plural "Those creatures don't untap during their controllers' untap
steps"), and `target 1 gains can't activate` ("Its activated abilities
can't be activated").

`until end of combat` (CR 500.5a, 511.2) ends a sentence the same way,
or begins it (`until end of combat, ...`): what would last until end of
turn ends as the combat phase ends (at the cleanup step if no combat
phase ends first), so a later combat phase that turn is free of it. It
prints "until end of combat" in place of "until end of turn" and "this
combat" in place of "this turn": `this gets +2/+0 until end of combat`
("it gets +2/+0 until end of combat"), `that creature can't block until
end of combat` ("That creature can't block this combat."), `target
creature defending player controls blocks this if able until end of
combat` ("... blocks it this combat if able").

`indefinitely` after a sentence whose clauses would last until end of
turn makes them last for good, printing no duration (CR 611.2a): `target
1 gains haste indefinitely` ("It gains haste.", on a creature returned to
the battlefield, Whip of Erebos). `leaving exiles that creature` prints
"If that creature would leave the battlefield, exile it instead of
putting it anywhere else." (`leaving exiles target 1`: "If it would
...").

Subject-first clauses: `this must be blocked each combat this turn if able` ("Anzrag must be blocked each combat this turn if able": the same as `this gains must be blocked if able`, until end of turn, worded so), `this deals that amount of damage to target creature that player controls` ("deals that amount of damage", `that much` worded so: Fear of Burning Alive), `owner shuffles this into library and draws 3 cards` ("Gandalf's owner shuffles him into their library and draws three cards": the source's owner, as it last existed, stays the subject; `owner of this` names that player elsewhere), `<who> gains 3 life`, `<who> loses 1 life`, `<who> loses half life` ("that player loses half their life, rounded up", CR 107.1a; `you lose half life`: "your life"), `you lose game` ("You lose the game.", CR 104.3e: not while the player can't lose the game),
`<who> draws 2 cards`, `<who> discards 2 cards`, `<who> mills 20 cards`,
`<what> gets +3/+3`, `<what> gains flying`, `<what> can't block` (this
turn), `<what> blocks if able` ("blocks this turn if able"), `<what> must be
blocked if able` ("must be blocked this turn if able"), `<what> can block an
additional creature`, `<what> can block any number of creatures` ("... this
turn"), `<what> attacks if able` ("attacks this turn if able", CR 508.1d;
"Those creatures attack ..."), `<what> blocks <what> if able` ("Target
creature blocks this creature this turn if able": a requirement on that
blocker for that attacker only, CR 509.1c), `<what> can't block <what>`
("Target creature can't block this creature this turn", CR 509.1b),
`<what> assigns no combat damage this turn` ("If you do, this creature
assigns no combat damage this turn.", CR 510.1a: it assigns none in this
turn's combat damage steps; write `this creature` to name it again),
`<what> can't be regenerated` ("That creature can't be regenerated this
turn": no regeneration shield replaces its destruction this turn, CR
701.15), `<what> can attack as though it
didn't have defender` ("can attack this turn as though it didn't have
defender"), `<what> gains lure` ("All creatures able to block <what> this turn
do so."), `<what> fights <what>`, `<what> has base power and toughness 4/4`,
`<what> becomes 2/2 Assembly-Worker artifact creature` (a land stays a
land, CR 205.1b: on a target land it prints "that's still a land" when no
duration follows, `until end of turn, target land becomes 3/3 creature`
"Until end of turn, target land becomes a 3/3 creature that's still a
land.", else ". It's still a land."; an Aura's `static enchanted: base 4/2,
set color red, add subtype Beast, add type creature, has trample` prints
"Enchanted land is a 4/2 red Beast creature with trample. It's still a
land."), `this becomes a
copy of <what> [with exceptions]` (`you may this becomes a copy of chosen
other creature you control until end of turn`: "you may have it become a
copy of another creature you control until end of turn", the creature
chosen as the effect resolves, not targeted, CR 608.2d; scenarios `choose
"X"`; any subject: `target Shapeshifter
becomes a copy of target creature until your next turn`, `each single
other creature becomes a copy of target nonlegendary creature until end of
turn`, "Each other creature becomes ..."; `until end of turn` and `until
your next turn` end the copy effect (`until end of turn, target 1 becomes
a copy of up to 1 another target creature until end of turn` prints
"Until end of turn, it becomes a copy of up to one other target
creature."; the same target kind twice, `target artifact you control
becomes a copy of target artifact you control with exceptions until end of
turn`, prints "... a copy of a second target artifact you control ...",
two different artifacts), the permanent getting back what it
was unless it has become a copy of something else since; a card in a
graveyard or face up in exile can be copied too, with its printed
values, CR 707.2: `this becomes a copy of target creature card with mana
value X from your graveyard` prints "a copy of target creature card in
your graveyard with mana value X", Likeness Looter; `you may this
creature becomes a copy of that created card until end of turn` prints
"you may have Lazav become a copy of that card until end of turn", the
name in full), `<what> bites <what>` ("deals damage
equal to its power to"; the creature is the damage's source, so its
deathtouch, lifelink, infect, and wither apply, and it deals none if
either target is illegal as the spell resolves), `<what> bites itself`
("deals damage to itself equal to its power"), `<what> bites twice <what>`
("deals damage equal to twice its power to"); `up to two target creatures
you control each bites <what>` prints "each deal damage equal to their
power to", each creature dealing its own damage. A group recipient is
dealt damage by each creature: `target creature you control bites each
other creature` (Waltz of Rage), where "other" excludes the creature
dealing the damage. `deal damage equal <expr>
to <what>` puts the amount before the recipient ("deals damage equal to
that creature's power to the creature's controller", where `deal count
equal ... damage to <what>` prints "deals damage to <what> equal to ...").
Counters likewise: `put count equal <expr> +1/+1 counters on <what>` prints
"put a number of +1/+1 counters on <what> equal to ...", and `put +1/+1
counters equal <expr> on <what>` puts the amount first: "put a number of
+1/+1 counters equal to its power on up to one target creature".

Verb-first clauses: `deal 3 damage to <what> [and <what>]`,
`destroy <what> [without regeneration [joined]]` (`joined`: "destroy
target Knight and it can't be regenerated" in one sentence), `put 2 +1/+1 counters on <what>`,
`put 1 +1/+1 counter and 1 reach counter and 1 deathtouch counter on
<what>` ("Put a +1/+1 counter, a reach counter, and a deathtouch counter
on target creature"), `put up to x +1/+0 counters on this capped 4` ("Put
up to X +1/+0 counters on this creature. This ability can't cause the
total number of +1/+0 counters on this creature to be greater than
four.": the engine puts as many as it may),
`remove 1 +1/+1 counter from <what>`, `remove 1 counter from <what>` ("remove a counter from target permanent": a counter of a kind the controller chooses), `remove up to 3 counters from <what>` ("remove up to three counters from target permanent": one at a time, each of a kind the controller chooses, or stop; scenarios `choose "1"` the first kind on it, `choose done`; unscripted, all three), `remove all counters from <what>`,
`return <what> to hand`, `return <what> to battlefield [with 1 +1/+1 counter]`,
`regenerate <what>`, `exile <what> [and <what>]` (a permanent or a graveyard card),
`put all card exiled with this into hand` ("Put all cards exiled with
this creature into their owners' hands.", from exile, CR 607.2a),
`return all card exiled with this to battlefield under your control`
("Return all cards exiled with this artifact to the battlefield under
your control"; `each creature card exiled with this` "each creature card
exiled with this land"): the cards this permanent's abilities exiled,
still in exile, matching the filter (CR 607.2a). A card whose text says
"exiled with" links every card its triggered and activated abilities
exile to the permanent (by any exile effect: a target, the top of a
library, a graveyard card); a new object after a zone change links
nothing (CR 400.7).
`1 creature card exiled with this` (`1 card exiled with this`; `exiled
with it` prints "exiled with it") is "a creature card exiled with this
land": one of those cards, chosen as the effect resolves (CR 608.2d, not
targeted), for `return <it> to battlefield [under your control] [with 2
additional +1/+1 counters]` ("Return a creature card exiled with this
land to the battlefield under your control"; `put ... onto battlefield`
"Put ... onto the battlefield"), `put 1 card exiled with this into hand`
("Put a card exiled with this Saga into its owner's hand"), and `you may
this becomes a copy of 1 creature card exiled with it until end of turn`
("you may have Lazav become a copy of a creature card exiled with it
until end of turn"); with none, nothing happens. After it, `each other
card exiled with this` ("Return each other card exiled with this land to
its owner's hand") is the rest of them.
`exile <what> until this leaves` ("exile target creature an opponent
controls until this enchantment leaves the battlefield", CR 610.3: nothing
is exiled if the source has already left, and the card returns under its
owner's control as soon as it leaves), `return exiled card to battlefield`
("return the exiled card to the battlefield under its owner's control":
what this permanent's own exiling triggered ability exiled, CR 607.2a, as
in Journey to Nowhere; a flickered permanent is a new object and links
nothing), `return those cards to battlefield [under your control]` (after
`exile up to 2 target creature you control then`: "return those cards to
the battlefield under their owner's control", each target found as the
card it became, CR 400.7),
`exile all graveyards`, `exile graveyard of target player` ("Exile target
player's graveyard"; `of each opponent`: "Exile each opponent's
graveyard"; `exile all creature card from graveyard of target player`:
"Exile all creature cards from target player's graveyard"), `put <what> on top of library` / `on bottom of
library` / `into library 2 from top` ("Put target creature into its
owner's library second from the top"; fewer cards, the bottom, CR 401.7),
`shuffle <what> into library` ("Shuffle enchanted creature into its
owner's library"; `you may shuffle up to 1 target card from your
graveyard into library`: "You may shuffle up to one target card from
your graveyard into your library"), `shuffle graveyard into library`
("Shuffle your graveyard into your library"; `target player shuffles
graveyard into library`: "Target player shuffles their graveyard into
their library"; `each player shuffles hand and graveyard into library then draws 7
cards`: "Each player shuffles their hand and graveyard into their library,
then draws seven cards.", both zones), `target player shuffles up to 3 target card from their graveyard into
library` ("Target player shuffles up to three target cards from their
graveyard into their library": `from their graveyard` is the graveyard of
the player target before it; `put any number of target artifact card from
target player's graveyard on top of library`: "Put any number of target
artifact cards from target player's graveyard on top of their library in
any order", the player target declared by the filter), `shuffle all creature card from graveyard of target
player into library` ("Shuffle all creature cards from target player's
graveyard into that player's library"), `shuffle cards from hand into
library` ("shuffle the cards from your hand into your library"; `then
draw count that many shuffled this way cards`: "then draw that many
cards"), `shuffle 1 card from hand into library` ("shuffle a card from
your hand into your library", the player's choice); the library is
shuffled even when no card moved (CR 701.24), `you may shuffle` ("You
may shuffle.": your library), and with the owner as subject, who makes any choice:
`owner puts target creature on top or bottom of library` ("Target
creature's owner puts it on their choice of the top or bottom of their
library"), `owner of target nonland permanent puts it into library 2
from top or bottom` ("The owner of target nonland permanent puts it into
their library second from the top or on the bottom"), `owner shuffles
this into library` ("this creature's owner shuffles it into their
library"), `<who> sacrifices 1 creature` ("of their choice"; `sacrifices 1 creature
of their choice with flying` puts the "with" after it; `sacrifices 1
artifact and 1 land` is "an artifact and a land of their choice", one
choice after the other), `<what>
loses flying`, `<who> reveals hand`, `<who> reveals 3 cards discarding
card` ("Target player reveals three cards from their hand and you choose
one of them. That player discards that card.", Blackmail: the player
chooses which cards to reveal, all of them with no more in hand, and you
choose among those; `... 3 cards discarding 2 card`: "... You choose two
of them. That player discards those cards."; `<who> reveals count where
Faerie you control, cards discarding card`: "... reveals X cards from
their hand, where X is the number of Faeries you control. You choose one
of those cards. ...", the comma ending the count; scenarios: the
revealing player's `choose "Card"` lines, then your `discard "Card"`),
`<who> reveals hand discarding
nonland card` (Duress: you choose the card; `<who> reveals hand and
discarding of that color card` after `choose color`: "Target player
reveals their hand and you choose a card of that color from it. That
player discards that card.", the color chosen as it resolves; `look at
hand of target opponent topping card`: "Look at target opponent's hand
and choose a card from it. Put that card on top of that player's
library."), `<who> discards 1 card at
random` (`discard 1 card at random` for you; `that player discards
another card at random`), `<who> reveals 1 card at random from hand`
("reveals a card at random from their hand"; the card is `that card`
after it: `then loses count equal mana value of that card life`, "then
loses life equal to that card's mana value"), `<who> skips next draw
step` ("Target player skips their next draw step", CR 500.11), `<who>
skips next combat phase` ("Target opponent skips their next combat
phase"; `... combat phase this turn`: only a combat phase of this turn;
the main phase after it still happens), `<who> skips next creature
untap` ("Creatures don't untap during target player's next untap step.",
Blinding Beam: no creature untaps in that player's next untap step,
whoever controls it; their other permanents do), `<who> takes extra turn` ("Take
an extra turn after this one." for `you take extra turn`; `target player
takes 2 extra turns`: "... two extra turns after this one", CR 500.7: the
most recently created extra turn comes first), `after this phase,
additional combat phase` ("After this phase, there is an additional
combat phase.", CR 500.8: added directly after the current phase, before
phases added earlier; `after this main phase, additional combat phase
then main phase` adds "... followed by an additional main phase", `after
this main phase, 2 additional combat phases` "there are two additional
combat phases"; after an `if`, the trailing order `if your main phase,
additional combat phase after this phase then main phase` prints "If it's
your main phase, there is an additional combat phase after this phase
followed by an additional main phase"; the conditions `your main phase`,
`your combat phase`, and `first combat phase` are "it's your main phase",
"it's your combat phase", "it's the first combat phase of the turn";
`only land creature can attack during that combat phase` after it prints
"Only land creatures can attack during that combat phase." (Bumi,
Unleashed, CR 508.1c: in the next additional combat phase to begin, only
matching creatures can attack)),
`sacrifice <what>`, `tap <what>`, `untap <what> [and <what>]`,
`gain control of <what>`, `attach to <what>` (this: "attach this Equipment to ..."),
`attach it to <what>` (in a trigger about another object, that object:
"Whenever an Equipment you control enters, you may attach it to target
creature you control"), `attach <what> to <what>` ("Attach target Equipment you control to target
creature you control": an Aura only where its enchant ability allows, an
Equipment only onto a creature and not while it is a creature, nothing if
already attached there, CR 701.3, 301.5c), `unattach all Equipment attached to
target creature` ("Unattach all Equipment from target creature", CR
701.3d), `amass Goblins 1 then attach to amassed Army` ("then attach this
Equipment to the amassed Army"), `create 2 "Soldier"`,
`create 1 tapped "Powerstone"` ("Create a tapped Powerstone token"),
`create 1 tapped and attacking "..."` ("create a ... token that's tapped
and attacking", "... tokens that are tapped and attacking": CR 508.4, never
declared as attackers; they attack what the source is attacking, else an
opponent; likewise `put Soldier creature card from hand onto battlefield
tapped and attacking`, `return target creature card from your graveyard to
battlefield tapped and attacking`, `return this to battlefield tapped and
attacking` from the graveyard, and `put target 1 onto battlefield under your
control tapped and attacking` ("... onto the battlefield under your control
tapped and attacking"; `return it to battlefield under your control tapped
and attacking` likewise "return it to the battlefield under your control
tapped and attacking", The Neutrinos),
`create 1 "0/0 green Elemental creature" sized count where land you
control` ("Create an X/X green Elemental creature token, where X is the
number of lands you control."; `sized x` is the spell's or ability's X,
"Create an X/X green Ooze creature token."): X is fixed as the effect
resolves, before the token exists (CR 608.2h), and the token has base
power and toughness X/X from its creation on (CR 111.3), a copiable value
(CR 707.2: a Clone, a token copy, or populate copying it is X/X too); the
definition's power and toughness print as "X/X",
`create 1 "Eldrazi Scion" it has` (the token's abilities after the
sentence: "Create a 1/1 colorless Eldrazi Scion creature token. It has
\"Sacrifice this token: Add {C}.\""; "They have" for more than one),
`create 1 "Wicked Role" attached to up to 1 target creature you control`
(an Aura token, such as a Role, enters attached to it; if there is nothing
it could enchant, no token is created, CR 303.4f; attached to several
objects, one is created for each: `create 1 "Monster Role" attached to
target 1` after `1 to 2 target creature each gain haste` prints "For each
of those creatures, create a Monster Role token attached to it.", and
`attached to each Rat you control` "For each Rat you control, create a
Wicked Role token attached to that Rat."; `each` after a variable target
group before `gain` prints "One or two target creatures each gain haste"), `<who> creates 2
"Treasure"` (`its controller creates`, `each player creates`, `you
create`: "Its controller creates two Treasure tokens"),
`create token copy of <what> [with exceptions]` (a permanent, or a card
in a graveyard or face up in exile: `exile target creature card from a
graveyard; create token copy of target 1 with exceptions` "Exile target
creature card from a graveyard. Create a token that's a copy of it,
except ..." (The Scarab God), `you may exile 1 creature card from your
graveyard chosen; if you do, create token copy of that created card`
"... create a token that's a copy of that card" (God-Pharaoh's Gift;
`that created card` is the card the effect before it moved; `exile x
target creature card from your graveyard; create token copy of each card
exiled this way` "For each card exiled this way, create a token that's a
copy of that card" (Hour of Eternity: each target, found in exile; `each
creature card exiled this way` prints "For each creature card exiled this
way, ... a copy of it"); `create token copy of a card exiled with this`
"... a copy of a card exiled with this Saga": what the card's own
abilities exiled (CR 607.2a; its triggered abilities' `exile target ...`,
a card in a graveyard too, are then linked to it, as for `return exiled
card to battlefield`; The Cloning of Shredder); `create tapped token
copy of ...`, `create 2 tapped token copies of this artifact`: "create a
tapped token that's a copy of ...", "create two tapped tokens that are
copies of this artifact", entering tapped; `create tapped and attacking
token copy of ...` "create a tapped and attacking token that's a copy of
...", and the older `create token copy of <what> [with exceptions] entering tapped
and attacking` "create a token that's a copy of ... and that's tapped and
attacking": CR 508.4, attacking what the source attacks; in a delayed
trigger about it, `the token` prints "the token": `delayed that token:
exile the token at end of combat`; of a group, `create
token copy of each nontoken creature you control` prints "For each
nontoken creature you control, create a token that's a copy of that
creature"; `create 2 token copies of it` prints "create two tokens that
are copies of it", `create x token copies of x target permanent` "For
each of X target permanents, create X tokens that are copies of that
permanent."; after a copy of a target, `if kicked, create 5 token copies of
that target 1 instead` prints "If this spell was kicked, create five of
those tokens instead." (Rite of Replication); `target player creates token
copy of target creature you control` "Target player creates a token
that's a copy of ..." (Echocasting Symposium: that player creates and
controls it); `create token copy of each creature target player controls`
"For each creature target player controls, create a token that's a copy
of that creature"; `those copies gain haste` then prints "Those tokens gain
haste until end of turn." where `those tokens` prints "They"), `copy <what> [times
<num>] [new targets] [with exceptions]` (CR 707.10: `copy target instant
or sorcery spell you control new targets` prints "Copy target instant or
sorcery spell you control. You may choose new targets for the copy.";
`copy target spell you control new targets`, any spell of yours, a
permanent spell's copy becoming a token (Slick Imitator);
`copy that spell` in a cast trigger, `copy it` for "it" there or in
`when you cast this`, `copy this` "copy this spell"; `times 2` prints
"twice", `times x` "X times" (the copied spell's X), `times count each
<count>` "for each ..."; "copies" after more than one. The copy is the
ability's controller's, is not cast, keeps the spell's mode, X, and
targets, and with `new targets` its controller may change them (CR
707.10c; `choose "Hill Giant"` in scenarios); `with exceptions` applies
the card's `except` block to the copy, "except the copy isn't
legendary". A copy of a permanent spell becomes a token as it resolves,
CR 608.3f; a copy of a spell that has left the stack is made from it as
it last existed). Abilities on the stack are copied the same way (CR
707.10, 707.10b: the copy has the same source and isn't activated): `copy
target triggered ability you control new targets`, `copy target
activated or triggered ability you control [from a creature source | from
an artifact source]` (targets "you control" only the ability's
controller's; `copy target instant spell, sorcery spell, activated
ability, or triggered ability` a spell only if it is an instant or
sorcery), `change target of target spell or ability with a single target`
("Change the target of target spell or ability with a single target.",
CR 115.7a: the effect's controller changes it to another target legal for
that spell or ability, judged for its controller and source; with none,
it is unchanged; `choose "Hill Giant"` or `choose "player B"` in
scenarios; "with a single target" is one target chosen, CR 115.9a),
`choose new targets for that spell` ("you may choose new targets for that
spell", CR 115.7d: the effect's controller may keep each target or change
it to another legal for the spell, judged for its controller; `choose
"Hill Giant"` in scenarios, unscripted they stay; new ones become targeted),
`counter target spell, activated ability, or triggered ability` (the
listed wording of `spell or ability`), `copy target 1 may cast [free]` (CR 707.12: "Copy it. You
may cast the copy [without paying its mana cost]."; `copy target 1 then
may cast free` "Copy it, then you may cast the copy without paying its
mana cost."; after `exile up to one target black card from your
graveyard and`, Kaervek's "... and copy it. You may cast the copy."): a
copy of the card is made in exile and its controller may cast it as the
ability finishes, for its mana cost (offered only if they can pay it) or
free; not cast, it ceases to exist; a copy of a permanent card becomes a
token; `if you do` after it sees whether it will be cast. In scenarios,
`cast "Murder" targeting "Hill Giant"` gives the copy's targets; the
runner always casts it), `emblem "Name"`,
`draw 2 cards` (you), `discard 1 card` (you), `mill 2 cards` (you),
`scry 2`, `surveil 1`, `investigate` (create a Clue token; `investigate
twice`; `investigate count each opponents lost life this turn`:
"investigate for each opponent who lost life this turn", `investigate once
count each ...`: "investigate once for each ..."; `target player
investigates`: that player creates it), `<who>
exiles 2 cards from hand` ("Target opponent exiles two cards from their
hand"; `from graveyard`: "a card from their graveyard"; the player
chooses; `exile 1 card from hand face down` is "exile a card from your
hand face down" and `exile hand face down` "Exile all cards from your
hand face down." (`target player exiles hand face down`: "Target player
exiles all cards from their hand face down."): exiled face down, nobody
may look at the cards (CR 406.3) unless the source lets its controller
(`may look exiled`), and they are linked to the source (CR 607.2a), so
"the exiled card", "cards exiled with this", and in a delayed trigger
`return those exiled cards to hand` ("return those cards to your hand",
Ignorant Bliss) find them), `exile library face down` ("Exile all cards
from your library face down.") and `each player exiles library but bottom
6 face down` ("each player exiles all but the bottom six cards of their
library face down": a library of six or fewer loses none), `<who> chooses creature they control; destroy that created creature`
("Target opponent chooses a creature they control. Destroy that
creature.": not targeted, the player chooses as it resolves; the chosen
permanent is `that created creature`; it is also remembered for `the
rest` and `unchosen`, below), choices remembered for each player (not
targeted, CR 608.2d; each player in turn order from the active player):
`each player chooses 3 permanent they control` ("Each player chooses
three permanents they control"), `that player chooses up to 2 creature
they control`, `each player chooses each of artifact, creature,
enchantment, planeswalker from among nonland permanent they control`
("... an artifact, a creature, an enchantment, and a planeswalker from
among the nonland permanents they control"; one permanent may be chosen
for several types), `each opponent chooses permanent they control of
each permanent type`, `for each player, you choose creature that player
controls` (you choose for every player), `for each player, you choose
from among permanent that player controls each of artifact, ...` ("For
each player, you choose from among the permanents that player controls
an artifact, ..."), `for each player, choose creature with power 2 or
less that player controls` (no "you"); `choose up to 1 creature chosen`
("Choose up to one creature.", any player's) is remembered the same way.
After such a choice, the recipient `the rest` is what could have been
chosen and wasn't (`destroy the rest`, `... then sacrifices the rest`),
and a group filter's `unchosen` ("all other nonland permanents"), `not
chosen this way` ("all nonland permanents not chosen this way"), `except
chosen this way` ("all creatures except creatures chosen this way") leave
out what was chosen; `that doesn't share creature type with chosen` ("that
don't share a creature type with the chosen creature they control") is
judged against the permanent chosen for the object's controller. `each
player sacrifices all unchosen nonland permanent they control` ("Then each
player sacrifices all other nonland permanents they control"): each their
own. What was chosen is also `those creatures` after it: `each opponent
chooses creature they control; you gain control of those creatures
indefinitely` ("Each opponent chooses a creature they control. You gain
control of those creatures."; `you gain control of ... indefinitely` names
"you", for good). `<who> exiles 1 creature they control` ("Target opponent
exiles a creature they control", their choice; `... they control and
their graveyard` then exiles their graveyard: "Target opponent exiles a
creature they control and their graveyard"), `<who> reveals hand
exiling nonland card from graveyard or hand` ("Target opponent reveals
their hand. You choose a nonland card from that player's graveyard or
hand and exile it."), `transform
this`,
`add {B}{B}` / `add any` ("Add {B}{B}.", "Add one mana of any color."; an
activated ability that only adds mana is a mana ability and resolves at
once, CR 605), `keep mana add {R}{R}{R}{G}{G}{G}` ("Add {R}{R}{R}{G}{G}{G}.
Until end of turn, you don't lose this mana as steps and phases end.": the
mana that sentence adds stays in the pool as steps end, until the cleanup
step ends, the player's other mana taken to be spent first; CR 500.4),
`keep red mana this turn` ("Until end of turn, you don't lose unspent red
mana as steps and phases end.": all the controller's red mana stays as
each step ends this turn, until the cleanup step), `add each color among permanent you control` ("For each color among
permanents you control, add one mana of that color."; any filter:
`add each color among monocolored permanent you control`, "... among
monocolored permanents you control ..."; payment uses it), `add {G} 6` ("Add six {G}.", Castle Garenbrig), `add {R} that much` ("add that much {R}", the triggering event's amount:
"Whenever Raphael is dealt damage, add that much {R}"), `add any 2` ("Add
two mana of any one color."; `add any count where power`: "Add X mana of
any one color, where X is this creature's power.", the color chosen once;
`add any count equal creature you control of chosen type`: "Choose a
color. Add an amount of mana of that color equal to the number of
creatures you control of the chosen type."; `add {R} or {G} count each
Raccoon you control`: "add {R} or {G} for each Raccoon you control", each
mana's color chosen as it is added; `add x {G} or x {W} count where other
creature you control`: "Add X {G} or X {W}, where X is the number of other
creatures you control.", one of the colors chosen for all of it; `add any 2 and any 2 other`: "Add two mana of any one color and two mana of any other color.", a color chosen, then another),
`add 2 mana
any combination` / `add 3 mana any combination of {R}{G}` / `add 2 mana
different colors` ("Add two mana in any combination of colors.", "Add
three mana in any combination of {R} and/or {G}.", "Add two mana of
different colors."; each mana's color chosen as it is added; `add X mana
any combination` with the ability's X, `add X mana any combination count
where creature you control with defender` "Add X mana in any combination
of colors, where X is the number of ..."), `add chosen 2` ("Add two mana of the chosen color."), `add chosen` ("Add one mana of
the chosen color."), `add {G} count equal power` ("Add an amount of {G}
equal to this creature's power."), `add any that land opponent controls
could produce`, `add any that land you control could produce`, `add any
that basic land you control could produce` ("Add one mana of any color
that a land an opponent controls could produce.": the colors those lands'
mana abilities could make as it resolves, CR 106.7, tapped or not; none,
no mana; `add any type that land you control could produce` is "one mana
of any type": {C} when those lands could make colorless mana and no
color, a color otherwise), `add any; this becomes that color` ("Add one
mana of any color. This creature becomes that color until end of turn.":
the color of the mana just added, also when payment activates the
ability), `add commander` ("Add one mana of any color in your commander's
color identity.": no mana, since the game has no commanders, CR 903.4f;
`add commander rider counters` and `add commander rider scry` add Opal
Palace's "If you spend this mana to cast your commander, ..." and Path of
Ancestry's "When that mana is spent to cast a creature spell that shares a
creature type with your commander, scry 1." as wording). An activated ability that adds mana and has no targets is a mana
ability even if it does more (CR 605.1a): `activated {T}, remove 1
depletion counter from this: add {G}{G}; if this has no depletion
counters, sacrifice this` resolves at once, and payment activates it
(removing the counter, then the rest) when plain sources fall short,
`add {C}; if you control Urza's Mine and Urza's Tower, add {C}{C}
instead` counting as the mana it would make now. A mana ability whose cost removes X
counters asks for X as it is activated (a scenario's `activate ... x 2`;
payment doesn't activate it): `activated {1}, remove X storage counters
from this: add X mana any combination of {W}{U}` (storage lands; a pair
prints in the color wheel's order, "{G} and/or {W}"), `activated {1},
remove X ki counters from this: add any X` ("Add X mana of any one
color."). `search basic land card reveal to hand`, `search up to 2
Forest card to battlefield tapped`, `search up to X land card reveal to
hand` ("up to X land cards": X is the count, the spell's X), `search basic land card reveal to
top` ("Search your library for ..., then shuffle"; add `that card` to
print "that card" for "it"; `search up to 3 card named this reveal to hand`,
`search any number card named this reveal to hand`: "a card named ~",
"up to three cards named ~", "any number of cards named ~"; `search basic
Forest or basic Plains or basic Island card to battlefield tapped`: "a
basic Forest, Plains, or Island card" (a supertype every alternative
shares prints once); `search basic land card or a Gate card reveal to
hand` / `... or Gate card ...`: "a basic land card or a Gate card",
"... or Gate card", plural "up to two basic land cards and/or Gate
cards"; `search instant card or a with flash card ...`: "an instant card
or a card with flash"; `search library or graveyard card named "Ajani, Valiant Protector"
reveal to hand` ("Search your library and/or graveyard for a card named
Ajani, Valiant Protector, reveal it, and put it into your hand. If you
search your library this way, shuffle."; without `reveal`, "... for a card
named Sunlit Hoplite and put it onto the battlefield. If ..."; a player
subject prints "... reveals it, and puts it into their hand. If they
search their library this way, they shuffle."): one card, from either
zone, into the hand or onto the battlefield; the library is shuffled if
the card came from it or nothing was found (`search library or
graveyard creature with no abilities card reveal to hand`, `... artifact
card with mana value 2 or less ...`: "for a creature card with no
abilities", "for an artifact card with mana value 2 or less", by a
filter). A counted search: `search up to X card to hand that card` ("for
up to X cards, put those cards into your hand", the spell's X), `search
up to count where land you control, basic land card to battlefield
tapped` ("for up to X basic land cards, where X is the number of lands
you control, put them onto ..."; the comma ends the count, which is read
as the search begins), `search up to that much land card to battlefield
tapped` ("for up to that many land cards", the triggering amount).
`card named "X"` names
another card by its name (`card named "X" or named "Y"`: "a card named X
or Y", either name) (also without `library or graveyard`: "Search
your library for a card named X, put it onto the battlefield, then
shuffle."); `search Rebel permanent card with mana value 3 or
less to battlefield`: the mana value after the noun, `permanent card`
not an instant or sorcery (CR 110.4a); `search card to graveyard that
card` ("Search your library for a card, put that card into your
graveyard, then shuffle."); `search 3 cards to exile` ("Search your
library for three cards, exile them, then shuffle.", face up and linked to
nothing); `search card to exile face down` ("Search your library for a
card, exile it face down, then shuffle.": only the searcher knows it, CR
406.3, and it is the resolution's card, "the exiled card"); a final `then` prints the older wording "... and put that
card onto the battlefield. Then shuffle." (`search Plains or Island or
Swamp or Mountain or Forest card to battlefield that card then`, Spoils of
Victory; into the hand likewise); a final `those` on a count-sized search
prints the newer wording "..., where X is the number of Shrines you
control. Put those cards onto the battlefield tapped, then shuffle." (`search
up to count where Shrine you control, basic land card to battlefield
tapped those`; with `reveal to hand`, "... Reveal those cards, put them
into your hand, then shuffle."); `search creature card reveal to hand or
graveyard` ("..., reveal it, put it into your hand or graveyard, then
shuffle.": the searcher chooses, `choose "1"` hand or `choose "2"`
graveyard in a scenario); `reveal to hand that card`:
"reveal that card, put it into your hand"; `search up to 2 basic land
card reveal to battlefield tapped and hand that card`: "reveal those
cards, put one onto the battlefield tapped and the other into your
hand" (the searcher chooses which; one found goes where they choose);
several differently-filtered cards, one of each, joined with `and`:
`search black card and green card and blue card to graveyard` ("Search
your library for a black card, a green card, and a blue card. Put those
cards into your graveyard, then shuffle."; with `reveal to hand`,
"... Reveal those cards, put them into your hand, then shuffle."; two
print "a basic land card and a Shrine card"): each card is found for
one filter only, and any may be left unfound (`choose decline` in a
scenario skips that one); named cards join with "and/or": `search card
named "Alpine Watchdog" and card named "Igneous Cur" reveal to hand`
("for a card named Alpine Watchdog and/or a card named Igneous Cur,
reveal them, put them into your hand, then shuffle."); `with different
names` after the cards (`search up to 2 basic land card or Town card
with different names to battlefield tapped`: "... and/or Town cards with
different names, put them ..."): no two found cards share a name (CR
201.2); `search graveyard, hand, or library ...` ("Search your
graveyard, hand, and/or library for ... and put them onto the
battlefield. If you search your library this way, shuffle.";
`graveyard, hand or library` prints "your graveyard, hand and/or
library"; `graveyard, hand, and library` prints "your graveyard, hand,
and library for ..., put it onto the battlefield, then shuffle." and
always shuffles): the hand's and graveyard's cards may be found too; a
destination by the found card, `to battlefield if with mana value 2 or
less else hand` ("... and reveal it. Put it onto the battlefield if its
mana value is 2 or less. Otherwise, put it into your hand. Then
shuffle."; `to battlefield tapped if land else hand`: "Put it onto the
battlefield tapped if it's a land card."; inside `you may`, "If you
search your library this way, shuffle."), the card judged by its printed
characteristics; `search up to 2 basic land card reveal to hand and
graveyard` ("Search your library for up to two basic land cards and
reveal them. Put one into your hand and the other into your graveyard.
Then shuffle.": the searcher chooses the one for the hand). Older
wordings, after the destination: `of them` ("... for up to two Forest
cards and reveal them. Put one of them onto the battlefield tapped and
the other into your hand, then shuffle."), `rest` ("..., reveal those
cards, and put one onto the battlefield tapped and the rest into your
hand. Then shuffle."), `short` before an `instead` search that ends
`sentence` (`search basic land card reveal to hand short; if evidence
collected, instead search creature or land card reveal to hand
sentence`: "Search your library for a basic land card. If evidence was
collected, instead search your library for a creature or land card.
Reveal that card, put it into your hand, then shuffle."; each search
has its own destination, printed once), and `instead of 2` closing a
trailing `instead` sentence (`label "Spell mastery" if 2 instant or
sorcery cards in your graveyard, search up to 3 basic Forest card reveal
to battlefield tapped and hand instead of 2`: "... search your library
for up to three basic Forest cards instead of two."; `if kicked, search 2
basic land card reveal to hand instead of 1`: "If this spell was kicked,
search your library for two basic land cards instead of one."),
`unshuffled` on a search an `instead` sentence follows (`search card to
hand that card unshuffled; if cast from graveyard, instead search 2 card
to hand that card then`: "Search your library for a card and put that
card into your hand. If this spell was cast from a graveyard, instead
search your library for two cards and put those cards into your hand.
Then shuffle."; with `reveal`, "..., reveal it, and put it into your
hand."), and `put instead` closing a trailing `instead` search that only
changes where the card goes (`search Vehicle card reveal to hand
unshuffled; if cast from graveyard, search Vehicle card reveal to
battlefield put instead then`: "... If this spell was cast from a
graveyard, put that card onto the battlefield instead. Then shuffle.";
without `then`, nothing more: `you may search basic land card reveal to
top that card; if you control Dragon, you may search basic land card
reveal to battlefield tapped put instead`, "... then shuffle and put that
card on top. If you control a Dragon, put that card onto the battlefield
tapped instead."; the engine searches once, for the instead
destination when the condition holds; `put instead of hand` with the
destination `hand or battlefield` before `instead if`: `label "Morbid"
search basic land card reveal to hand or battlefield that card put
instead of hand instead if creature died this turn`, "Morbid — You may
put that card onto the battlefield instead of putting it into your hand
if a creature died this turn.", the searcher choosing `choose "1"` hand
or `choose "2"` battlefield in a scenario);
another player chooses among the found cards with `except N chosen by
<player> to <destination>` after the destination (`search up to 4 card
with different names reveal to hand except 2 chosen by target opponent
to graveyard then`: "Search your library for up to four cards with
different names and reveal them. Target opponent chooses two of those
cards. Put the chosen cards into your graveyard and the rest into your
hand. Then shuffle."; `chosen by opponent` prints "An opponent chooses",
the next opponent in turn order; without `then`, "... and the rest onto
the battlefield tapped, then shuffle."; inside `you may` with `exactly`:
`you may search exactly 2 card not named this that have different names
reveal to graveyard except 1 chosen by opponent to hand`, "you may search
your library for exactly two cards not named Burning-Rune Demon that have
different names. If you do, reveal those cards. An opponent chooses one
of them. Put the chosen card into your hand and the other into your
graveyard, then shuffle."): the found cards are revealed, the chooser
picks N of them (`choose "X"` by that player in a scenario), and with N
or fewer found all of them are chosen; `that have different names` and
`that each have different names` print so ("up to five Dragon cards not
named Tiamat that each have different names"), and `with different
names that each have mana value X` puts the mana value after the names
("up to X creature cards with different names that each have mana value
X"); a found card put onto the battlefield with counters, `to battlefield
tapped with 1 stun counter` ("puts it onto the battlefield tapped with a
stun counter on it", CR 122.6; `with X additional +1/+1 counters` prints
"additional"); `repeat` after slots that differ from the first card only
by mana value (`search instant card with mana value 3 and instant card
with mana value 2 and instant card with mana value 1 reveal to hand
repeat`: "Search your library for an instant card with mana value 3,
reveal it, and put it into your hand. Then repeat this process for
instant cards with mana values 2 and 1. Then shuffle.": one card for each
mana value, any may be left unfound);
another player searches their own library with a subject: `its
controller may search basic land card to battlefield tapped` ("Its
controller may search their library for ..., put it onto the battlefield
tapped, then shuffle": declining before finding anything is not
searching, so nothing is shuffled), `target player searches basic land
card to battlefield tapped` ("Target player searches their library for
..., puts it onto the battlefield tapped, then shuffles"), `exile same
name as target 1 from its controller zones` ("Search its controller's
graveyard, hand, and library for all cards with the same name as that
spell and exile them. Then that player shuffles.": the target's name and
controller as it last existed, CR 608.2h, after it was countered or
exiled; the engine finds every such card, CR 701.19b). A card
revealed and put into a hand (`reveal to hand`), like every card of a
revealed hand, is known to every player until it moves (CR 701.20):
`determinize` keeps it in place),
`impulse 2 next turn first` ("Exile the top two cards of your library.
Until the end of your next turn, you may play those cards."; impulse
draw: the controller may play the exiled cards, paying their costs, with
the usual timing permissions and a land only with a land play left, CR
305.2, 601.3; `impulse 1 this turn` is "You may play that card this
turn.", `first` puts the duration first ("Until end of turn, you may
play that card."), `it` prints "it" for "that card", and `next end step`
is "until your next end step", which ends as that step begins; `while you control this` is "You may play that card for as long as you control Lightning" (the source on the battlefield under its controller's control, the same object); `choose`
adds "Choose one of them." and only that card may be played (`choose
one`: "Choose one.", wording only), `this way`
prints "cards exiled this way", `of target opponent` takes the cards from
that player's library ("Target opponent exiles the top card of their
library. You may play that card this turn.", the permission still yours; `from target opponent`
instead prints "Exile the top three cards of target opponent's library.
You may play those cards this turn."); `remains` in place of the
duration is `forever` printed "for as long as they remain exiled"
("You may play that card for as long as it remains exiled.", with `first`
"For as long as that card remains exiled, you may play it" with `it`);
`face down` exiles the cards face down (CR 406.3: no other player knows
them, the controller may look at them; "Exile the top two cards of target
opponent's library face down. You may look at and play those cards for as
long as they remain exiled.", Expensive Taste), `look face down` prints
"Look at the top two cards of your library and exile them face down."
and "you may play" (Flameshape); a static condition last, `if you
control Kavu`, belongs to the permission: "you may play it if you control
a Kavu" is judged whenever the card could be played, not as the effect
resolves (Possibility Technician); `look those face down` prints "...
and exile those cards face down" (Petty Larceny); a rider (`any type`,
see `may play that card`) comes last,
a count `count where ...` prints "the top X cards of your library, where
X is ...", and `that much` "exile that many cards from the top of your
library"), `exile top 3 cards` ("Exile the top three cards of your
library."; `exile top 1 card`: "the top card"; `each opponent exiles top 1
card`: "Each opponent exiles the top card of their library."; `exile top
1 card of each player`: "exile the top card of each player's library",
Etali; `exile top 1 card face down` adds "face down": nobody may look at the card while it
stays in exile (CR 406.3, Bomat Courier; the reminder "(You can't look at
it.)" is not printed), `face down look` "face down and look at it": its
controller may),
`counter target spell`, `counter target spell unless its controller pays {3}`,
`... pays {X}` (the spell's X), `... pays {X} count each hand` ("{1} for
each card in your hand"; `count each 2 Wizard on battlefield` is "{2} for
each Wizard on the battlefield", `count each plus 2 Faerie you control`
is "{2} plus an additional {1} for each Faerie you control", `count each
plus 2 2 each graveyard named this` "{2} plus an additional {2} for each
card named ~ in each graveyard", `count where
mana value of target 1` is "{X}, where X is its mana value"), and after
either `to exile` / `to library top` ("If that spell is countered this
way, exile it instead of putting it into its owner's graveyard" / "put it
on top of its owner's library instead of into that player's graveyard").
Targets on the stack also include abilities (CR 701.5a: a countered
ability leaves the stack): `target spell or ability`, `target activated
or triggered ability`, `target activated ability`, `target triggered
ability`, `target activated ability from an artifact source`; `target spell that
targets this` ("that targets this
creature"),
`you may <clause>`, `pay {2}` / `pay 2 life` / `pay {W}{B} and 2 life` (after `you may`; the last "you may pay {W}{B} and 2 life", both or neither),
`target opponent may sacrifice 2 nonland nontoken permanent chosen` ("Target opponent may sacrifice two nonland, nontoken permanents of their choice."; a following `if they don't, <clause>` prints "If they don't, ...": only if the player didn't, or couldn't), `target opponent may have you create 2 "Lander"` ("Target opponent may have you create two Lander tokens.": the player decides, and the effect's controller acts), `each player may shuffle hand and graveyard into library` ("Each player may shuffle their hand and graveyard into their library.", each in turn order) and after it the recipient `each player who does` ("Each player who does draws seven cards.", also joined with `then` in the same sentence: those who did, once all have decided), `each player who controls most creature investigates` ("Each player who controls the most creatures investigates.": ties included, counted as it happens), `you and all permanent you control gain hexproof` ("You and permanents you control gain hexproof until end of turn.": the controller and the group), `you have no maximum hand size for rest of game` ("You have no maximum hand size for the rest of the game.", CR 402.2: for good, whatever leaves the battlefield),
`that player may pay {4}` / `that player may discard 1 card at random`
(another player decides and acts as it resolves, CR 608.2d; an act they
can't complete isn't offered; a following `if they do,` prints "If the
player does,"; `deal 3 damage to any target; then controller of that
target 1 may pay {R}{R}; if they do, controller of that target 1 may copy
this new targets` prints Chain Lightning's "Then that player or that
permanent's controller may pay {R}{R}. If the player does, they may copy
this spell and may choose a new target for that copy.": the controller of
an "any target" that is a player is that player, and the copy is the
paying player's, CR 707.10), `any player may <clause>` / `any opponent
may <clause>` ("Any player may pay 5 life", CR 608.2d: starting with the
active player and in turn order, CR 101.4, each player, or each
opponent, may do it as their own, until one does; an act they can't
complete isn't offered; the effect's controller has declining listed
first): `pay 5 life`, `pay {1}`, `sacrifice 2 creature chosen` ("sacrifice
two creatures of their choice"), `exile 1 card from your graveyard
chosen` ("exile a card from their graveyard"), `this deals 6 damage to
you` ("have Book Burning deal 6 damage to them"); a following `if a player
does, <clause>` (or `if you do,`) prints "If a player does, ...", and `if
no one does, <clause>` (or `otherwise,`) "If no one does, ..."; `counter
this` there in a "When you cast this spell" trigger names the spell
("counter Brain Gorgers"); `<clause> unless any player may this deals 4
damage to you` prints "... unless a player has this creature deal 4
damage to them" (the clause happens if no one does), `the creature` (in a triggered ability, the triggering
object printed "the creature": "untap the creature"), `if you do,
<clause>` (only if the optional action before it happened: "You may pay
{2}. If you do, draw a card."), `if you don't, <clause>` / `if you dont, <clause>` (only if it
didn't: "You may pay {U}. If you don't, destroy target Knight."; after
`reveals hand discarding may ...` the discard then prints "If you do,
that player discards that card."), `if you can't, <clause>` / `if you cant, <clause>` (after an act
that happens if it can, only if nothing was chosen: "Put a land card
from among the milled cards into your hand. If you can't, create a
Treasure token."), `when you do, <clause>` (a reflexive
triggered ability, CR 603.12: it triggers only if the optional action
before it happened, and its targets are its own, chosen as it goes on the
stack: `you may pay {2}{R}; when you do, this deals 3 damage to any
target`; with {X}, `you may pay {X}; when you do, put X +1/+1 counters
on this`, the player chooses X as they pay, any value they can pay, and
X after it, in the same resolution or the reflexive ability, is that
value; after `draw 1 card then discard 1 card` it prints "When you discard
a card this way, ..."; `you may you sacrifice a creature` prints "you may sacrifice a
creature", and an empty choice is no action; every sentence after `when
you do` is the reflexive ability's, and what the resolution sacrificed
is its `that sacrificed creature`: `you may sacrifice 1 another creature
chosen; when you do, this deals damage equal power of that sacrificed
creature to another target creature; if excess damage dealt this way,
...`; an `if` right after `when you do,` is an intervening "if" and
covers every later sentence of the reflexive ability, CR 603.4: `when you
do, if this has 4 or more quest counter, put 1 +1/+1 counter on target
creature you control; target 1 gains trample until end of turn`,
Earthbender Ascension), `<clause> unless you <clause>` (an act, not a payment: `sacrifice this
unless you discard 1 card at random`, `... unless you return 2 Forest you
control to hand chosen` ("... unless you return two Forests you control to
their owner's hand"), `... unless you sacrifice 1 land chosen`, `... unless
you tap 1 untapped permanent you control chosen`, `... unless you return 1
basic land card from your graveyard to hand chosen`: the controller may
act as it resolves; only an act they can complete in full counts, and with
none possible the clause happens without asking; `return 1 enchantment to
hand chosen` with no controller may return anyone's), `<clause> unless you pay {1}` / `unless
they pay 2 life` / `unless its controller pays {2}` (the payer may pay to
stop it), `unless any player pays {2}` / `{X}` (each player in turn from the
active player may pay, until one does; the effect's own controller is
asked too, with declining offered first; a second pump of the same
recipient there prints "gets an additional +4/+4", and of creatures you
control "They get an additional ..."), `unless that player pays {1} or 1
life` (either, the payer's choice), `counter target spell unless its
controller pays {2}; if they do, you create 1 "Lander"` ("If they do":
whether it was paid), `add {R} or {G} or {W}` ("Add {R}, {G}, or {W}."), `adapt 2` (CR 701.46:
+1/+1 counters only if it has none), `bolster 2` (CR 701.39: +1/+1 counters
on a creature you control with the least toughness, chosen if tied),
`support 2` (CR 701.41: a +1/+1 counter on each of up to two target
creatures; in a triggered ability, other target creatures, as support on a
permanent means), `populate` (CR 701.36: a token copy of a creature token
you control, chosen), `incubate 3` (CR 701.53: the token "Incubator", a
transforming token whose back face is the 0/0 Phyrexian artifact creature,
entering with three +1/+1 counters; `you incubate 2` prints "you
incubate 2" after another player's choice: "If they do, you incubate
2."), `<what> connives` (CR 701.50), `<what> becomes prepared` (CR 722.3a), `dig 4 take creature
card may revealing rest bottom` ("Look at the top four cards of your
library. You may reveal a creature card from among them and put it into
your hand. Put the rest on the bottom of your library in any order."; `dig
3 take card rest graveyard`: "Put one of them into your hand and the rest
into your graveyard"; `dig 4 reveal take all land card rest graveyard`:
"Reveal the top four cards ... Put all land cards revealed this way into
your hand and the rest into your graveyard"; `take 2 instant or sorcery
card may`: "up to two instant and/or sorcery cards", and without a
filter `dig 4 take 2 card rest graveyard`: "Put two of those cards into
your hand" (with one card left over, `dig 3 take 2 card rest graveyard`,
"Put two of them into your hand and the other into your graveyard"); `take any creature card may revealing`: "You may reveal any
number of creature cards from among them and put the revealed cards into
your hand"; `take creature card and land card may`: "a creature card
and/or a land card" (one of each); `rest bottom random`: "in a random
order"; a final `reword` (wording only) prints "two of them" for "two of
those cards", "one of those cards" for "one of them", and "Then put the
rest" after `revealing`, and a final `revealed` prints "the rest of the revealed
cards" for "the rest" (Kruphix's Insight); `dig 3 then take card rest graveyard`: "look at the top three
cards of your library, then put one of them into your hand and the rest
into your graveyard"; `dig 3 take card may top rest graveyard`: "You may
put one of those cards back on top of your library. Put the rest into
your graveyard.", the card kept stays on top (with `reword`, "You may put
one of those cards on top of your library."); `revealing that card`
prints "put that card into your hand" ("put them" for several), `short`
"Put one into your hand and the other ..."; `onto battlefield
[tapped]` after `may`: onto the battlefield under your control instead of
into the hand, `dig 5 take creature card may onto battlefield rest bottom
random` "You may put a creature card from among them onto the
battlefield. Put the rest ...", `dig 6 take 2 creature with mana value 3
or less card may onto battlefield rest bottom` "Put up to two creature
cards with mana value 3 or less from among them onto the battlefield. Put
the rest ..." (`reword`: "You may put up to two ..."), `dig 5 reveal take
land card onto battlefield rest graveyard` "Put a land card from among
them onto the battlefield and the rest into your graveyard"; `permanent
with mana value 5 or less` is a permanent card there; after a counted
dig, `dig count where graveyard creature reveal take green permanent with
mana value X or less card ...`, X is the dig's count: "reveal the top X
cards of your library, where X is .... You may put a green permanent card
with mana value X or less from among them ...", Hatchery Spider; `rest
shuffle` leaves the rest in the library and shuffles it, "Then shuffle."
(CR 701.24): `dig 20 then take any land card may onto battlefield tapped
rest shuffle` "look at the top twenty cards of your library, put any
number of land cards from among them onto the battlefield tapped, then
shuffle", without `then` "Put any number of ... tapped, then shuffle.",
with `reword` "You may put any number of ... onto the battlefield. Then
shuffle."; `take creature card at random onto battlefield`: "Put a random
creature card from among them onto the battlefield. Put the rest ...",
picked by the game's random number generator; `dig 5 take 2 creature or
land card may split land card creature card rest bottom random`: "You
may reveal up to two creature and/or land cards from among them, then put
the rest on the bottom of your library in a random order. Put all land
cards revealed this way onto the battlefield tapped and put all creature
cards revealed this way into your hand."; `take all enchantment card ...
among` prints "Put all enchantment cards from among them into your hand"
for "revealed this way"; `dig 4 then take 2 card may top rest graveyard`:
"look at the top four cards of your library, then put up to two of them
back on top of your library in any order and the rest into your
graveyard"; `dig 2 until take all land card onto battlefield tapped rest
bottom random`: "Reveal cards from the top of your library until you
reveal two land cards. Put those land cards onto the battlefield tapped
and the rest on the bottom of your library in a random order." (CR
701.20: the cards down to the second matching one, or the whole library,
are revealed; `take any ... may` "Put any number of those permanent cards
onto the battlefield, then put the rest ...", `revealed` "the rest of the
revealed cards"; a counted `dig count where colors among permanents until
...` prints "until you reveal X permanent cards, where X is ...").
A dig ending `instead [lead|after] [end] [rest] <change> if <condition>`
is changed by a later sentence when the condition holds as it resolves
(CR 608.2c; compiled to the dig and its changed copy, exactly one
happening; the condition is any of the `instead` sentence's): `count 8`
prints after the first sentence ("If this spell was bargained, look at
the top eight cards of your library instead."), `take 2`, `take any`,
`take all` before the sentence about the rest ("If this spell was kicked,
put two of those cards into your hand instead. Put the rest ..."; `take
all` without a filter "put each of those cards into your hand"; after a
revealing take "you may instead reveal two creature and/or land cards
from among them and put them into your hand"; after `onto battlefield`
"put any number of creature cards from among them onto the battlefield
instead"), or with `end` after it; `lead` prints "If ..., instead ...",
`after` "... instead if ...", the default "If ..., ... instead."; `rest`
adds "and the rest ..." to it (Flow State); `otherwise onto battlefield`
after a revealing take prints "You may reveal up to two creature cards
from among them. If this spell was bargained, put the revealed cards onto
the battlefield. Otherwise, put the revealed cards into your hand."
(Thunderous Debut). `dig 3 take card rest bottom reword instead after end
take all if 3 Lesson cards in your graveyard` is Accumulate Wisdom), `look at top 3` ("look at
the top three cards of your library, then put them back in any order";
`look at top count where hand` puts "where X is the number of cards in
your hand" before ", then put them back"; `look at top 3 of target
player`: "... of target player's library, then put them back in any
order"; `look at top 3 of target player only` ends at "Look at the top
three cards of target player's library."; `that player looks at top 3 of
you`: "That player looks at the top three cards of your library, then puts
them back in any order": the looker knows the cards where they are),
`split top 3 into hand, top, bottom` ("Look at the top three cards of your
library. Put one of those cards into your hand, one on top of your
library, and one on the bottom of your library."; `into hand, graveyard,
bottom`: "..., one into your graveyard, and ..."): one card to each
destination in turn, the controller choosing (`choose "X"` in scenarios),
a card put on top staying there; `split top 2 into graveyard` ("Look at
the top two cards of your library. Put one of them into your graveyard.",
the other left in place); `split top 2 of target opponent into exile
then` ("Look at the top two cards of target opponent's library, then
exile one of them."); `split top 3 reveal opponent into graveyard rest
hand` ("Reveal the top three cards of your library. An opponent chooses
one of them. Put that card into your graveyard and the rest into your
hand.": the opponent chooses); `split top 3 of target player into top
rest graveyard` ("Look at the top three cards of target player's library.
Put one back and the rest into that player's graveyard."). Destinations:
`hand`, `graveyard`, `top`, `bottom`, `exile`,
`piles reveal 5 chosen by opponent` ("Reveal the top five cards of your
library and separate them into two piles. An opponent chooses one of those
piles. Put that pile into your hand and the other into your graveyard.",
CR 700.3; `piles look 4 face-down chosen by opponent the piles`: "Look at
the top four cards of your library and separate them into a face-down pile
and a face-up pile. An opponent chooses one of the piles. ..."; `piles
reveal 5 separated by opponent`: "Reveal the top five cards of your
library. An opponent separates those cards into two piles. Put one pile
into your hand and the other into your graveyard."; `piles look 4
face-down separated by target opponent`: "Target opponent looks at the
top four cards of your library and separates them into a face-down pile
and a face-up pile. Put one pile ..."; `X plus 1` prints "X plus one"):
the separator puts cards into the first (face-down) pile one at a time
(`choose "X"`, then `choose done` in scenarios; unscripted, all of them),
the rest making the second, which is revealed to every player when face
up; a pile may be empty. Then the chooser takes `choose "1"` or `"2"`.
The count `graveyarded this way` ("for each card put into your graveyard
this way") counts the cards the resolution put into a graveyard,
`count each graveyarded this way +1/+1 counters on it` (Intrude on the
Mind). Other sources and destinations: `piles reveal 5 chosen by
opponent one to hand rest bottom` ("... An opponent chooses one of those
piles. Put a card from the chosen pile into your hand, then put all
other cards revealed this way on the bottom of your library in any
order.": the controller picks the card, then the order, a card at a
time); `piles exiled separated by opponent onto battlefield` after an
exile ("An opponent separates those cards into two piles. Put all cards
from the pile of your choice onto the battlefield under your control and
the rest into their owners' graveyards.": the cards the resolution
exiled); `piles exile 3 twice chosen by opponent` ("Exile the top three
cards of your library in a face-down pile, then exile the top three
cards of your library in another face-down pile. Look at the cards in
each pile, then turn a pile of your choice face up. An opponent chooses
one of those piles. Put that pile into your hand and the other into your
graveyard.": the controller turns a pile up with `choose "1"` or `"2"`
before the opponent chooses); `piles exile 5 separated by opponent play
free` ("Exile the top five cards of your library. An opponent separates
those cards into two piles. You may play lands and cast spells from one
of those piles. If you cast a spell this way, you cast it without paying
its mana cost.": each card of the chosen pile is offered in turn, a
spell cast as the effect finishes, CR 608.2g, a land played on its
controller's turn with a land play left, CR 305.1; the rest stay in
exile). `<who> reveals hand` also makes the hand's cards "revealed this
way": `deal count each 3 with chosen name card revealed this way damage
to that player` ("deals 3 damage to that player for each card with the
chosen name revealed this way", Thought Hemorrhage);
`return 1 land you control chosen` ("return a land you control to its
owner's hand"), `switch pt of target
creature` ("Switch target creature's power and toughness until end of
turn"), `target instant or sorcery card from your graveyard gains
flashback` ("Target instant or sorcery card in your graveyard gains
flashback until end of turn. The flashback cost is equal to its mana
cost."; `gains flashback that card`: "... equal to that card's mana
cost.": its owner may cast it from the graveyard for its mana cost this
turn, with its usual timing, and it is exiled as it leaves the stack, CR
702.34a; `gains harmonize`: "... gains harmonize until end of turn. Its
harmonize cost is equal to its mana cost.", cast that way this turn,
tapping up to one creature to reduce it, CR 702.180a), `grant graveyard cast to creature cards` ("Creature cards in your graveyard gain "You may cast this card from your graveyard" until end of turn.": the controller's graveyard cards matching the filter as it resolves, CR 611.2c, may be cast from there this turn for their costs with their usual timing, and aren't exiled; a card that leaves the graveyard loses it), `graveyard cast this turn creature cards by forage finality` (Osteomancer Adept: "Until end of turn, you may cast creature spells from your graveyard by foraging in addition to paying their other costs. If you cast a spell this way, that creature enters with a finality counter on it.": the controller's permission for the rest of the turn, cards put into the graveyard later included, with the rider words of a card grant's `cast`), `put land card from hand onto battlefield [tapped]` ("put a land
card from your hand onto the battlefield"; qualities after the noun print
there, `put Hero creature card with mana value 3 or less from hand onto
battlefield`, `with mana value X or less` (X of the spell), `with mana
value at most count where land you control` (counted as it resolves),
`with total power and toughness 5 or less`; nothing put is not done, so
`if you don't, draw 1 card` after `you may put ...` draws; in an
activated ability as in a spell, `that creature` after it is the creature
put there; `put creature card and land
card from hand onto battlefield`: "put a creature card and/or a land card
from your hand onto the battlefield", a choice of each in turn; in a
spell, `that creature` after it is the creature put there, also as what a
delayed trigger is about: `spell: you may put creature card from hand
onto battlefield; that creature gains haste indefinitely; delayed that
creature: sacrifice that creature at beginning of the next end step`,
Through the Breach), `<what> doesn't untap next
untap step` ("It doesn't untap during its controller's next untap
step."; a group of targets named again, `targets 1 to 2 doesn't untap
next untap step`, is "Those creatures don't untap during their
controller's next untap step", or with `next untap steps` "... during
their controllers' next untap steps", and a group `each attacking creature
doesn't untap next untap step` "Each attacking creature doesn't untap
..."), `<who> sacrifices that creature` ("that player sacrifices that
creature": a sacrifice of that one permanent, worded with its
controller), `<what> gains ability { <ability line> }` ("Target creature
gains \"...\" until end of turn."; after `gains deathtouch and`, the
second "gains" is dropped: "gains deathtouch and \"When this creature
dies, ...\""), `proliferate` (CR
701.27; the engine makes the usual choices: your permanents' +1/+1 and
loyalty counters, opponents' -1/-1 counters and poison; `proliferate X
times`: "Proliferate X times.", that many separate proliferates), `<what>
explores` (CR 701.44; `<what> explores twice` prints "explores, then it explores again"; `it explores X times` explores that many times, each a separate explore; a creature card returned from a graveyard is "that creature" afterwards), `<what> endures 2` (CR 701.63: its controller puts that many +1/+1 counters on it or creates an N/N white Spirit token, needing the token "N/N white Spirit creature"; `endures X` makes a 0/0 Spirit that size; one that has left the battlefield makes the token; endure 0 does nothing; `this endures 1` prints "it endures 1" after the source is named, "this creature endures 1" after another clause, "Anafenza endures 2" for a legendary source; scenarios `choose "1"` counters, `choose "2"` the token), `<what> phases out` (CR 702.26: treated as though
it doesn't exist, with what's attached to it, until it phases in as its
controller's next untap step begins; `keywords phasing` phases a
permanent out at every other such step), `amass Zombies 2` (CR 701.47; needs the token
"0/0 black Zombie Army creature"), `empower jace 2` / `empower jace count
where creature you control` ("Empower Jace 2.", "Empower Jace X, where X
is ...": N loyalty counters on a Jace planeswalker token you control,
the token "Jace planeswalker" (blue, 0 loyalty, "[−1]: Surveil 1",
"[−3]: Draw a card.") created first if you control none; a nontoken
Jace doesn't count), `<who> gets 1 poison counters`, `add {G}
count each Elf on battlefield` ("Add {G} for each Elf on the
battlefield."), `if kicked, <clause>` ("If this spell was kicked, ..."
for a spell with `kicker`; `if kicked, also <clause>` puts "also" after
the subject: "If this spell was kicked, that creature also gains hexproof
until end of turn."; a target first named in it, "return another target
permanent card from your graveyard to your hand", is chosen only if the
spell is kicked, CR 601.2c), `prevent all [combat] damage
[from <source>] [to <affects>] this turn` ("Prevent all combat damage that
would be dealt by target attacking creature this turn"; `to each player`
prints "to players"), `prevent all [combat] damage [to <affects>] this turn
from <source>` ("... dealt this turn by creatures with power 4 or less"),
`prevent all [combat] damage <source> would deal [to <affects>] this turn`
("Prevent all combat damage target creature would deal this turn",
"... that creatures would deal to players this turn"; `bare` before the
source drops the "that": `prevent all combat damage bare non-Soldier
creature would deal this turn`, "prevent all combat damage non-Soldier
creatures would deal this turn"; `prevent all damage permanent of color
of choice would deal this turn`, "Prevent all damage that sources of the
color of your choice would deal this turn.": the color is chosen as the
effect is created, CR 608.2d, and any source of it, a spell too, is
prevented), `prevent all combat
damage to and from target 1 this turn` ("dealt to and dealt by it"),
`prevent all combat damage this turn except from andlist creature with
aura or enchantment creature` ("Prevent all combat damage that would be
dealt this turn except combat damage that would be dealt by enchanted
creatures and enchantment creatures.", Inspire Awe: every source but
those), `prevent all damage this turn to <affects> [from <source>]` ("Prevent
all damage that would be dealt this turn to creatures you control"),
`prevent next 4 damage to <affects> this turn [from <source>]` ("... to
any target this turn by a source of your choice"), `prevent next 4 damage
this turn to any number of targets` ("..., divided as you choose", divided
as the spell is cast, CR 601.2d), `prevent next count where Cleric on
battlefield damage to target creature this turn` ("Prevent the next X
damage ..., where X is ...", counted as it resolves),
`this turn, replace <event>: <outcome>` (`this turn, replace target 1
dies: exile`: "If that creature would die this turn, exile it
instead."; a target "creature or planeswalker" prints "If that creature or
planeswalker would die this turn" and covers the planeswalker too, CR
700.4; `this turn, replace creature dealt damage this way dies:
exile`: "If a creature dealt damage this way would die this turn, exile
it instead.", one effect locked to each permanent the resolution has
dealt damage to so far, CR 608.2c), `players can't gain life this turn`,
`damage can't be prevented this turn`, `excess damage to its controller
instead` (after a spell's damage to a creature: "Excess damage is dealt to
that creature's controller instead.", CR 120.4a: beyond lethal damage, as
a second damage event), `the damage can't be prevented`
(after a spell's damage: "The damage can't be prevented.", that spell's
damage only, CR 615.12), and
`delayed <what>: <effects> at beginning of the next end step` (a delayed
triggered ability, CR 603.7; write the trigger first, then a comma, then the
effects, to print it that way); `at beginning of the next turn's upkeep`
also works ("Draw a card at the beginning of the next turn's upkeep";
about a target player, `delayed target player: that player draws 1 card
at beginning of the next turn's upkeep` prints "Target player draws a
card at the beginning of the next turn's upkeep.", `that player` in it
being the player it is about),
`delayed this: at beginning of your next upkeep, discard 3 cards at
random` ("At the beginning of your next upkeep, ..."), `delayed that
token: at beginning of the end step on your next turn, sacrifice that
token` ("At the beginning of the end step on your next turn, ...": the
controller's end step, but not in the turn it was created), and
`at end of combat` ("destroy it at end of combat"). A delayed trigger
about a triggering object that has left the battlefield whose effects make
a token copy of it is about that permanent as it last existed (`whenever
you sacrifice this or artifact: you may pay {2}; if you do, delayed that
creature: at beginning of the next end step, create token copy of it`:
"... create a token that's a copy of that artifact", printed so when the
sacrifice trigger's filter is artifacts; a token or a card works alike).
Inside a delayed ability `it` is the object
the ability is about, and so is `that card`, which prints so where `it`
prints "it" (`delayed target 1: return that card to battlefield at
beginning of the next end step`: "Return that card to the battlefield
under its owner's control ..."; `return it`: "Return it ..."; in one
about several targets, `delayed targets 1 to 2: return those cards to
battlefield at beginning of the next end step`: "Return those cards to the
battlefield under their owner's control ...", each in turn; about any
number of targets, `exile any number of target creature; delayed target 1:
at beginning of the next end step, return those cards to battlefield
tapped` covers them all, "... under their owners' control", Morningtide's
Light); in one about `that creature` (the triggering object),
`that creature` is that object too, only while it is the same object
(CR 603.7c), and prints "that creature"; in one about a target, `that
creature` is the object too. `delayed target 1: when that creature dies
this turn, <effects>` (and `when that creature leaves the battlefield this
turn`) triggers once if that object dies (leaves) and ends unfired at the
cleanup step (CR 603.7c); `when it dies under your control this turn`
("When it dies under your control this turn, draw two cards.", Desperate
Measures) only if its controller as it last existed is the delayed
trigger's controller. In a delayed trigger about an object, `it` after an
event with no object of its own ("Whenever that creature deals combat
damage this turn, you may exile it") is still that object, and after
`exile it; if you do,` `return it` finds the card this resolution
exiled. Every sentence after a trigger-first delayed
trigger is its own; braces close it, so the sentences after them are the
spell's: `delayed target 1: { when that creature dies this turn, you gain
3 life }; target creature you control fights that target 1` ("When that
creature dies this turn, you gain 3 life. Target creature you control
fights that creature."). An open one whose later sentence declares a
target is an error. A target declared by the delayed ability
itself prints in its trigger: `delayed target creature: when that
creature dies this turn, return triggering card to battlefield` ("When
target creature dies this turn, return that card to the battlefield under
its owner's control."; `triggering card` is the card it became). After
`create 3 "..."`, `those tokens` is every token that sentence created
(those still on the battlefield), and `delayed those tokens: sacrifice
those tokens at beginning of the next end step` is one delayed trigger
about each ("Sacrifice those tokens at the beginning of the next end
step."), and `them` prints "them" (`delayed those tokens: exile them at
beginning of the next end step`). `those tokens gains haste` right after
prints "They gain haste until end of turn.". In `delayed that token: ...`, `it` prints "it" and `that token`
"that token" ("Exile that token at the beginning of the next end
step."); both are the token. `delayed this: at beginning of each combat this turn, <effects>` ("At
the beginning of each combat this turn, untap all creatures that attacked
this turn.", Full Throttle) triggers at each such step until the turn
ends. `delayed this: whenever <trigger> this turn, <effects>` triggers on
every such event until the turn ends (CR 603.7c), as its controller's
ability: "Whenever a creature blocks this turn, its controller loses 1
life." (`its controller` is the triggering object's), "Whenever a
nontoken creature you control dies this turn, create ...". Led by
`until end of turn,` it is the same ability printed with the duration
first: `until end of turn, delayed this: whenever creature you control
deals combat damage to a player, investigate` ("Until end of turn,
whenever a creature you control deals combat damage to a player,
investigate."). In a delayed trigger about a target, `that creature` is that target, as the trigger's subject (`delayed target 1: whenever that creature deals combat damage this turn, ...`) or in `dealt damage by that creature this turn` (`delayed target 1: whenever creature dealt damage by that creature this turn dies this turn, its controller loses 2 life`: "Whenever a creature dealt damage by that creature dies this turn, its controller loses 2 life."); `this` there is the same object. A delayed trigger printed "Whenever" with neither is a
compile error (it would trigger only once, CR 603.7c). `delayed
this: when next you cast instant or sorcery spell this turn, copy that
spell new targets` ("When you next cast an instant or sorcery spell this
turn, copy that spell. ...", CR 603.7c) triggers only the first time,
then ends; any cast trigger may follow `when next` (`when next you cast
instant or sorcery with mana value 4 or less spell`).

Who and what (`<who>`, `<what>`): `you`, `each opponent` (`each of your
opponents` is the same players, printed "Each of your opponents"), `each player`,
`its controller` / `its owner` (of the last target declared),
`this` (the source), `that player` / `that creature` (what triggered the
ability; in a spell or an activated ability, the player or creature
targeted last), `defending player`, `each creature`, `each creature you control`,
`the target 1` ("the creature": "If you control a black permanent,
destroy the creature instead"), `controller of the creature` in a
triggered ability ("the creature's controller": the triggering object's),
`those creatures`, `those permanents`, and `them` (printed "they" as a
subject) after a group named earlier in the ability (`each <filter>`,
`all <filter>`, `each creature you control`): the objects that group
denoted when its effect happened, still on the battlefield, even those
that no longer match it (CR 608.2c): "Creatures you control get +2/+1
until end of turn. Untap those creatures.", `gain control of all artifact
your opponents control until end of turn; untap them; they gain haste`
("Gain control of all artifacts your opponents control until end of turn.
Untap them. They gain haste until end of turn.": the stolen artifacts, not
every permanent you now control), `put 1 +1/+1 counters on each token
creature you control; they gain trample`. After a variable number of
targets (`distribute 3 +1/+1 counters among 0 to 3 target creatures; they
gain vigilance until end of turn`), `they` is those targets, as `target
1` is. As a count, `count each those
creatures` is "for each of those creatures" (Bounding Felidar's "You gain
1 life for each of those creatures"). Without a group before it, `those
creatures` is an error. In a triggered ability, `+1/+1 counters on it`
counts the triggering object's counters, as it last existed (The Ooze's
"create a Mutagen token for each +1/+1 counter on it"); `on this` counts
the source's. `each permanent you control`,
`enchanted creature` / `equipped creature` / `attached` (what this Aura or
Equipment is attached to; printed "it" after its first mention in the
ability, or after a condition `if attached is untapped`: "tap enchanted
creature and remove all counters from it"),
`that token` (what the previous effect created; `that created creature`
is the same token, printed "that creature"), `each <filter>` or
`all <filter>` (every matching permanent: `each artifact`, `each blue
creature`, `each creature without flying`, `each white creature you
control`; written singular, printed "all artifacts" after tap, destroy,
and exile, "each creature without flying" after deal damage, and "White
creatures you control get +2/+2" as a subject), `target creature and all other creature with same name as
target 1` ("Target creature and all other creatures with the same name as
that creature": the target, if still legal, and every other permanent with
its name as the effect resolves; returned, "to their owners' hands";
`return target creature card from your graveyard and all other card with
same name as target 1 to hand` is "Return target creature card and all
other cards with the same name as that card from your graveyard to your
hand", the cards in its graveyard), `target enchantment and each other
enchantment that shares color with target 1` ("... and each other
enchantment that shares a color with it": none but the target if it is
colorless, CR 105.2c), and targets. After such a recipient, `those
creatures` is the target and the group together ("Untap target creature
and each other creature that shares a color with it. Those creatures get
+2/+0 until end of turn.", Rally the Righteous).
Two subjects, one of them a target, each doing the same thing: `you and
target opponent each draw 1 card` ("You and target opponent each draw a
card.", you first, then the target) and `this and up to 1 another target
creature each get +3/+3` ("This creature and up to one other target
creature each get +3/+3 until end of turn."; with the target declined, the
source still gets it; with the only target illegal, the ability does
nothing, CR 608.2b). Only `draw` and `get` take such a subject.
`enchanted creature and each other creature that share creature type`
("Enchanted creature and other creatures that share a creature type with
it", Crown of Vigor's `activated sacrifice this: enchanted creature and
each other creature that share creature type gets +1/+1`): what the
source is attached to (as it last existed, once the Aura is sacrificed)
and every other creature, any player's, sharing a creature type with it
(CR 205.3m; a changeling has them all, CR 702.73a).
creatures you control get +2/+2" as a subject; `each creature your
opponents control`), and targets. A group is "they" again within its
sentence and named in full in a later one ("... to each creature your
opponents control. Creatures your opponents control can't block this
turn."). Alternatives in the plural join with "and": `each Ninja or Rogue
you control gains menace` prints "Ninjas and Rogues you control gain
menace".

How the source names itself (CR 201.5): `this card` prints "this card" (the
source's card, as a dies trigger exiles it from the graveyard: "Exile this
card."). `this` prints "this creature" (the
card's own noun: "this land", "this Aura", "this Equipment"), or a spell's
name, at its first mention in an ability and "it" after. A legendary
permanent uses its name instead of "this creature", up to the first comma
of the name ("When Pol Jamaar enters", "Sacrifice Lunatic Pandora", "As
Hall of Triumph enters"). To choose the form rather than take the default,
write `this creature` (any of the nouns above; the card's own noun is what
prints) for the full name again after a mention ("..., you may sacrifice
it. If you do, this creature deals 3 damage to that player.", "return this
creature to its owner's hand"), or `it` in a spell or an activated ability
for "it" at the first mention ("If this spell was kicked, it deals 3
damage to that player."). Damage takes the source as a subject the same
way: `this creature deals 3 damage to ...`, `it deals 2 damage to ...`
(plain `deal` is `this deals`). Another object may be the source:
`enchanted creature deals 1 damage to target creature with flying`
(Dizzying Gaze), or the triggering object written `it` ("Whenever
equipped creature becomes blocked, it deals 1 damage to defending
player"); a source that has left the battlefield deals the damage as it
last existed (CR 608.2h: "When this creature dies, it deals 3 damage to
any target").

### Targets

`target creature` declares a target; the first mention names it, and later
mentions refer to it by number: `target 1`, printed as "it", or
`that target 1`, printed as "that creature". `2 target lands` declares two
targets of one kind (two different objects, CR 115.3); `up to 2 target
creatures` declares optional ones (CR 601.2c), which come after an
ability's other targets. A variable group is one target word naming any
number of objects within bounds (CR 601.2c), chosen after the ability's
other targets even when a target is written after it (`tap 1 to 2 target
untapped creature you control; target 1 bites target creature an opponent
controls`, Coordinated Clobbering: scenarios script the creature an
opponent controls first; in a `choose two` pair whose first mode ends
with a group, the second mode's targets come first): `X target creature` ("X target creatures": exactly X, the spell's
X, which can't exceed what there is to target), `up to X target card`
("up to X target cards": at most the spell's X, any number of them; as
the amount's only recipient, "deals five times X damage to each of up
to X targets"), `up to 3 target creature card from your graveyard with
total mana value 3 or less` ("up to three target creature cards with
total mana value 3 or less from your graveyard", Patch Up; also after
`any number of` and `up to X`: the printed mana values of the chosen
objects add up to at most the bound, X the spell's; after a return to
the battlefield, `put 1 +1/+1 counter on each of them` finds them there,
CR 400.7), `1 to 2 target creature`
("one or two target creatures"; `1 to 3` prints "one, two, or three"),
`any number of target player`, and `1 to 2 targets` / `any number of
targets` for "any target" (`deal 1 damage to 1 to 2 targets`: "deals 1
damage to each of one or two targets"; `0 to 2 targets`: "each of up
to two targets"); `2 target any` is two separate any-targets ("deals 2
damage to each of two targets"). Later mentions (`target 1`) print "them" or
"they", and a group as subject takes "each" where Oracle has it ("One or
two target creatures each get +2/+1", "Any number of target players each
gain 6 life"). "X target cards from a single graveyard" (`X target card
from a graveyard`) and "up to two target cards from a single graveyard"
(`up to 2 target card from a graveyard`) take the rest from the first
card's graveyard; `up to 2 target card from graveyards` ("up to two target
cards from graveyards") takes them from any.
Divided effects (CR 601.2d) take a group: `deal 2 damage divided among 1
to 2 targets` ("deals 2 damage divided as you choose among one or two
targets"), `deal X damage divided among any number of targets`, and
`distribute 2 +1/+1 counters among 1 to 2 target creature`. The caster
announces the division as the targets are chosen, each target getting at
least one (so no more targets than the amount, and at least one when
there is anything to divide); a target that is illegal on resolution
loses its share. `another target creature` excludes the source.
`another target any` (or `any other target`) prints "any other target":
not the source nor what the ability's other "target" words chose (CR
115.3; `deal 2 damage to any target and deal 1 damage to another target
any`, Boulder Dash). `any target that was dealt damage this turn` (Needle
Drop): a player, creature, planeswalker, or battle that was dealt damage
this turn, judged as it is chosen and as the spell resolves. `another target blocking creature` prints "another
target blocking creature". `another` before a target with no "other"
form is an error rather than dropped.
`deal X plus 1 damage divided among any number of targets` ("X plus 1
damage"; also wherever a number goes; `2 times X`, "twice X", likewise:
`deal 2 times X damage to target creature`, `you gain 2 times X life`,
`put 2 times X stun counters on it`). The same damage to several targets
of one phrase prints "each of": `deal 1 damage to up to 2 target creature`
("deals 1 damage to each of up to two target creatures"); `those targets`
names every target declared so far again ("Those creatures can't block
this turn"), and counters put on `targets 1 to 2` print "on each of
them". `deal 2 damage to up to 1 another target creature token`:
"up to one other target creature token" (not the first target). In a
filter, `that player 1 controls` is the 1st target, a player, and `that
player or planeswalker 1 controls` that player or that planeswalker's
controller ("deals 4 damage to target player or planeswalker and 1 damage
to each creature that player or that planeswalker's controller controls");
`its controller` after a target is "its controller", and `controller of
that target 1` of a creature-or-planeswalker target "that permanent's
controller".
Specifications: `any target`, `creature`, `player`, `player or planeswalker`,
`opponent or planeswalker`,
`creature or planeswalker`, `land`, `tapped creature`, `creature you control`,
`nonlegendary creature you control`, `artifact`, `enchantment`,
`artifact or enchantment`, `artifact, creature, or land`, `permanent`,
`another creature`, `another permanent`, `creature card from your graveyard`,
`spell`, `creature spell`, `noncreature spell`, `<filter> spell`
("target instant or sorcery spell"), `card from a graveyard`, `<filter>
card from your graveyard` / `from an opponent's graveyard` / `from that
player's graveyard` (the triggering player's), `opponent`,
`artifact or creature`, `attacking or blocking creature`, `attacking or
blocking or tapped creature` ("target attacking, blocking, or tapped
creature": any of the three), `attacking
creature`, `blocking creature`, `<Subtype> creature` ("target Assembly-Worker creature"), and any filter
("target nonblack creature", "target creature an opponent controls").
Targets relative to a player: `target creature that player controls` (the
triggering player: "Whenever this creature deals combat damage to a
player, ... target creature that player controls"; outside a triggered
ability there is no triggering player, and the bare `that player
controls` is an error; so it is in a trigger on the source's own event,
`when this enters`, `when this dies`, `when this leaves`, `when this is
turned face up`, `when you cast this`: "... target nonland permanent an
opponent controls and all other nonland permanents that player controls
with the same name as that permanent" is `controller of target 1
controls`, Deputy of Detention), `another target
creature controller of target 1 controls` ("Target creature an opponent
controls deals damage equal to its power to another target creature that
player controls": the first target's controller; a first target is offered
only if it leaves a second, and the spell can be cast only if some pair
exists), both checked as targets are chosen and again on resolution (CR
608.2b); a triggered ability with no target that player controls is
removed (CR 603.3d). `target creature other than enchanted` ("target
creature other than enchanted creature"): not what the source is attached
to. `target creature other than this` ("target creature other than this
creature") is `target other creature` worded after the noun; so is
`each creature you control other than this` ("each creature you control
other than this creature"; after a choice, "each creature you control of
the chosen type other than this Vehicle"). A group or count of what a target player controls declares that
target in its filter: `each creature target player controls gets -2/-2`
("Creatures target player controls get -2/-2 until end of turn"), `you
gain count each 2 creature target player controls life`; `target opponent
controls` targets an opponent. Nothing matches if that target is illegal
as the effect happens (CR 608.2b).

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

### "Instead"

A sentence may replace the sentence just before it when a condition
holds (CR 608.2c: later text modifies earlier text; this is not a
replacement effect). Exactly one of the two happens, chosen as the effect
resolves; only the sentence just before is replaced. Three wordings:

- `if kicked, deal 4 damage instead`: "If this spell was kicked, it deals
  4 damage instead."
- `if kicked, instead each creature you control gets +2/+2 and gains
  first strike`: "If this spell was kicked, instead creatures you control
  get +2/+2 and gain first strike until end of turn."
- `that target 1 gets +4/+4 instead if metalcraft`: "That creature gets
  +4/+4 until end of turn instead if you control three or more
  artifacts."
- `that target 1 gets -4/-4 instead bare if you control creature named
  "Bogbrew Witch"`: the same without the replaced clause's "until end of
  turn" ("That creature gets -4/-4 instead if ...", Festering Newt).

A kicked "instead" that declares its own targets replaces the targets
before it (CR 601.2c): `target creature you control gains
indestructible; if kicked, instead any number of target creature you
control gains indestructible` (Divine Resilience) has one target unkicked
and only the group kicked. After it, a reference to a kicked-only target
("Exile the chosen creature, then its controller gains life ...", where
`its` follows the kicked target) is the chosen target either way.

The condition is `kicked`, any static condition (`if you control 3
creature, ...`, `... instead if hand 0 or less`), or the target as it
resolves: `if target 1 is Human, ...` ("If it's a Human, ..."), `if that
target 1 is other Hero, ...` ("If that creature is another Hero, ..."),
or as it last existed: `if that target 1 was with mana value 2 or less,
...` ("If that spell's mana value was 2 or less, ...", after countering
it). State on the battlefield counts: `if target 1 is creature with aura
or enchantment creature, also ...` ("If it's an enchanted creature or
enchantment creature, it also ..."); `enchanted creature` (what the
source enchants) is an error in a target check.
`label "Hellbent"` before the sentence prints an ability word (CR 207.2c,
no rules meaning): "Hellbent — Cackling Flames deals 5 damage instead if
you have no cards in hand." In a `spell { ... }` block it starts its own
line. `deal 4 damage` without `to` deals it to what the sentence before
dealt damage to ("it deals 4 damage instead"). A spell names itself again
in the upgrade unless it is for kicker; creating the token the sentence
before created prints "create four of those tokens".

### Choosing what to act on, alternatives, and "this way"

Optional actions that choose what they act on (CR 608.2d) end in
`chosen`; the controller chooses as the effect resolves, and without `up
to` the action happens only if all of it can ("you may exile two creature
cards from your graveyard" does nothing with one), which is what `if you
do` then sees:

- `untap up to 2 land chosen` ("untap up to two lands": tapped permanents,
  anyone's unless the filter names a controller)
- `choose 1 creature you control chosen; it gains indestructible until end
  of turn` ("Choose a creature you control. It gains indestructible until
  end of turn.", CR 608.2d: not targeted; anyone's unless the filter names
  a controller): nothing is done to it, and `it` after it is the chosen
  permanent (`choose 1 Giant creature you control chosen; it bites target
  creature`: "It deals damage equal to its power to target creature.");
  with none, the sentences about it do nothing
- `tap 1 other untapped Citizen you control chosen` ("tap another untapped
  Citizen you control"), `tap 2 other untapped creature you control chosen`
- `sacrifice 1 creature chosen` ("sacrifice a creature")
- `transform 1 Incubator token you control chosen` ("transform an
  Incubator token you control", CR 701.27: permanents you control; one
  that can't transform is still chosen), `transform any number Human
  Werewolf you control chosen`
- `discard 1 creature card chosen` ("discard a creature card")
- `reveal any number green card in hand chosen` ("reveal any number of
  green cards in your hand", CR 701.20a): the cards stay in the hand,
  known to every player; `count where revealed this way` ("the number of
  cards revealed this way") and `count each revealed this way` ("for each
  card revealed this way") count them, also any the resolution has
  moved since (`reveal until artifact card; put that card onto
  battlefield and the rest on bottom of library random; deal count equal
  revealed this way damage to you`, Madcap Experiment, counts the
  artifact put onto the battlefield); `each player loses count each
  creature they control life` ("Each player loses 1 life for each creature
  they control.", Stronghold Discipline) counts each player's own (you,
  then your opponent; elsewhere `they control` in a count for each player
  is refused); `count each creature card milled
  this way` prints "for each creature card milled this way" (a milled
  filter always names cards)
- `exile 2 creature card from your graveyard chosen`; `exile top 1
  creature card from your graveyard chosen` ("exile the top creature card
  of your graveyard", CR 404.2: the most recent, nothing chosen); `exile
  this spell and 4 named this card from your graveyard chosen` ("exile
  this spell and four cards named Dragon's Approach from your
  graveyard": the cards, then the resolving spell, only if all four can
  be)
- `exile 1 card from a graveyard chosen` ("exile a card from a
  graveyard": a card in any graveyard, not targeted, so no crime; Lazav,
  Familiar Stranger); `exile 1 artifact or creature card from your hand
  chosen` ("exile an artifact or creature card from your hand", Nexus of
  Becoming). After either, `that created card` is the card exiled
  ("that card"), `that exiled card` the same printed "the exiled card"
- `exile 1 land you control chosen` ("exile a land you control"; `exile 1
  other creature you control chosen`: "exile a creature you control other
  than this creature"): on a card whose other ability says `return
  exiled card to battlefield`, what it exiles is linked to it (CR 607.2a,
  Wormfang Newt)
- `return 1 creature card from your graveyard to battlefield chosen`
  ("return a creature card from your graveyard to the battlefield"; `to
  battlefield tapped`: "... to the battlefield tapped"), under its owner's
  control; `... to battlefield with 1 finality counter chosen` ("... with
  a finality counter on it", one counter of the kind as it enters;
  Scavenger's Talent)
- `put any number creature card from hand onto battlefield chosen` ("put
  any number of creature cards from your hand onto the battlefield",
  Ghalta, Stampede Tyrant; `up to 2`: "up to two creature cards"); `from
  hand or graveyard` ("put a land card from your hand or graveyard onto
  the battlefield"; with more than one, "up to X land cards from your hand
  and/or graveyard", Worldsoul's Rage), `tapped` and `with 1 finality
  counter` after `onto battlefield` as above (Kastral, the Windcrested):
  each card the controller chooses enters under their control, one at a
  time; `them` after it is the cards put (`they gain haste`); a mana value
  after the noun, `put 1 permanent card with mana value X or less from
  hand onto battlefield tapped chosen` ("a permanent card with mana value
  X or less from your hand", Mind into Matter; X is the spell's)
- `return 1 creature card put into your graveyard this way to battlefield
  under your control chosen` ("return a creature card put into your
  graveyard this way to the battlefield under your control", Starfall
  Invocation; `put into a graveyard this way`: any graveyard, Zero Point
  Ballad): the cards this resolution put into a graveyard (CR 608.2c),
  still there, enter under the controller's control
- `put 1 creature card from among them onto battlefield chosen` ("put a
  creature card from among them onto the battlefield"; `from among milled
  cards`: "from among the milled cards"), as `into hand` below
- `attach 1 Equipment you control to target 1 chosen` ("attach an Equipment
  you control to it": permanents you control, attached to that target);
  `attach 1 Equipment you control to it chosen` (after `return this to
  battlefield ...`: "Then you may attach an Equipment you control to
  him", by the card's pronoun; what the resolution last returned, Winter
  Soldier)
- `destroy up to 1 Equipment attached to that target 1 chosen` ("Destroy
  up to one Equipment attached to that creature.", Light of Judgment):
  permanents of any controller unless the filter names one, not targeted
- `exile 1 card from your graveyard at random` ("exile a card at random
  from your graveyard"; `that card` after it is the card exiled: `may play
  that card this turn`, Tersa Lightshatter)
- `return target Aura or Equipment card from your graveyard to battlefield
  attached to chosen creature you control` ("... attached to a creature
  you control", One Last Job): the host is chosen as the card returns (CR
  303.4f), not targeted; with none, the card stays where it is
- `return this and target land card from your graveyard to battlefield
  tapped` in an ability that works from the graveyard ("Return this card
  and target land card from your graveyard to the battlefield tapped.",
  Sandman, Shifting Scoundrel): the source, then the target
- `put up to 1 target creature card from that player's graveyard onto
  battlefield under your control; it enters tapped and attacking that
  player` ("... under your control. It enters tapped and attacking that
  player.", Shark Shredder): the same as `... under your control tapped
  and attacking`, printed as a sentence of its own
- `reveal until creature card; put that card onto battlefield tapped and
  attacking and the rest on bottom of library random` ("Put that card onto
  the battlefield tapped and attacking and the rest ...", CR 508.4; Raph &
  Mikey, Troublemakers)
- `return 1 creature or land card from your graveyard to hand chosen`
  ("return a creature or land card from your graveyard to your hand"),
  `return up to 2 creature card from your graveyard to hand chosen`
- `return up to 2 land you control to hand chosen` ("return up to two
  lands you control to their owner's hand")
- `put 1 permanent card from among milled cards into hand chosen` ("put a
  permanent card from among the milled cards into your hand"; `from among
  them`; `from among milled this way cards`: "from among the cards milled
  this way"): cards this resolution milled that are still in the
  graveyard. `put all Elf card from among them into hand chosen`: "put
  all Elf cards from among them into your hand", every one, none chosen
  (`all` takes any chosen act's every candidate); `put 1 creature or land
  card from among milled cards on top of library chosen`: "put a creature
  or land card from among the milled cards on top of your library".
  With a player subject after their mill, `target player mills 5 cards
  then target 1 puts all Goblin card from among milled this way cards into
  hand chosen` prints "Target player mills five cards, then puts each
  Goblin card milled this way into their hand" (each card to its owner's
  hand).
- `put up to 1 land card discarded this way onto battlefield tapped
  chosen` ("Put up to one land card discarded this way onto the
  battlefield tapped under your control."): cards this resolution
  discarded that are still in a graveyard, under the controller's control
  (CR 110.2a).

`permanent card` is a card that is not an instant or sorcery (CR 110.4a);
`nonland permanent card` likewise, and not a land.

`A or B` joins alternatives the controller chooses between, among those it
can do: `you may sacrifice 1 artifact chosen or discard 1 card; if you do,
draw 1 card`, `create 1 "Food" or create 1 "Treasure"` ("create a Food
token or a Treasure token"; `destroy each enchantment or destroy each nonwhite
enchantment` likewise names "destroy" once). A subject is named in each
alternative: `deal 2 damage to each attacking creature or deal 2 damage to
each blocking creature` is "Lava Storm deals 2 damage to each attacking
creature or Lava Storm deals 2 damage to each blocking creature". `target creature gains choice of lifelink or
indestructible` is "gains your choice of lifelink or indestructible until
end of turn". With none possible nothing happens and `if you do` sees that.
After a choice (`return 1 creature card from your graveyard to hand chosen`,
`put 1 land card from among milled cards into hand chosen`), `if you can't,
...` happens only if it found nothing to act on: "Put a land card from among
the milled cards into your hand. If you can't, create a Treasure token."

Conditions on what the resolution did "this way" (CR 608.2c), judged on
the objects as they last existed (CR 608.2h): `if creature card was exiled
this way, ...`, `if artifact is destroyed this way, ...` ("If an artifact
is destroyed this way"), `if land card is discarded this way, ...`, `if
Saproling was sacrificed this way, ...`, `if you discarded card this way,
...` (cards you owned), `if you put Town card into hand this way, ...`,
`if you put no cards into hand this way, ...` ("If you put no cards into
your hand this way", Nashi), `if Dragon is damaged this way, ...` ("If a
Dragon is dealt damage this way": a permanent the resolution dealt damage
to, as it is or last existed, The Black Arrow) and `if player is damaged
this way, ...` ("If a player is dealt damage this way"; a target player
acting in the sentence prints "they": "they discard a card", Sonic
Shrieker).
Verbs: `exiled`, `sacrificed`, `destroyed`, `discarded`, `milled`,
`returned` (put into a hand), `shuffled` (put into a library), and in a
count only `revealed` (parley: `each player reveals top card; you create
count foreach nonland card revealed this way "1/1 white Spirit creature
with flying"` is "Each player reveals the top card of their library. For
each nonland card revealed this way, you create ..."; every card the
resolution revealed, still where it was). What a spell's or ability's costs sacrificed
or discarded counts too (CR 601.2h, 602.2b): `if the sacrificed creature was
Human, ...`, `if the sacrificed permanent was artifact, ...`, `if the
discarded card wasn't land card, ...`, and as an "instead": `if the
sacrificed creature was legendary, draw 3 cards instead`. Counts: `count
each 2 enchantment destroyed this way` ("2 life for each enchantment
destroyed this way"; `count each permanent returned this way` prints
"for each permanent returned to its owner's hand this way"), `count equal
life lost this way` ("life equal to the
life lost this way"), `count that many discarded this way` ("draw that
many cards"; `discard up to 2 cards chosen then draw count that many
discarded this way cards`: "discard up to two cards, then draw that many
cards", `discard any number cards chosen ...`: "discard any number of
cards, ..."; `count that many plus 1 discarded this way`: "draw that many
cards plus one"), `count equal damage dealt this way` ("life equal to the
damage dealt this way", CR 120.3), `count equal excess damage dealt this
way` (CR 120.4a; `count equal excess damage dealt to that creature this
way` prints "equal to the amount of excess damage dealt to that creature
this way", and `if excess damage dealt this way, create 1
"Lander"`: "If excess damage was dealt this way, ..."; beyond lethal damage, given damage already marked, or
beyond a planeswalker's loyalty). What the resolution or its costs moved,
as it last existed: `toughness of the sacrificed creature` ("the
sacrificed creature's toughness"), `mana value of the sacrificed
permanent`, `toughness of that sacrificed creature` ("that creature's
toughness", after "sacrifices a creature"). Other counts: `hand of target
1` ("the number of cards in that player's hand"), `library of target 1`
("the number of cards in that player's library"; `target opponent mills
count equal half down library of target 1 cards` prints "Target opponent
mills half their library, rounded down."), `greatest mana value among
other artifact you control` ("the greatest mana value among other
artifacts you control"; also in a static ability, from the mana costs as
the layers have them), `greatest mana value graveyard [<filter>]` ("the
greatest mana value among [creature] cards in your graveyard"; 0 with
none), `max 2 <expr>` ("2 or the greatest power among Dinosaurs you
control, whichever is greater"), `colors of that spell` ("for each of
that spell's colors": the triggering spell, as it last existed), `damage
dealt to it this turn` ("the amount of damage dealt to it this turn": the
damage marked on the source, as it last existed on the battlefield; in a
dies trigger, what it died with), `creatures left this turn` ("for each
creature that left the battlefield under your control this turn"; also in
a static ability), `colors of target 1` ("for each of its colors": the
target's, as it last existed), `greatest toughness
other creature you control` ("the greatest toughness among other
creatures you control"), `life you lost this turn` ("the life you've lost
this turn"), `loyalty of this` ("Nissa's loyalty": the loyalty counters
on this planeswalker, CR 306.5b; `loyalty counters on this` prints "the
number of loyalty counters on Nissa"), and `<expr> plus <expr>` ("the number of creatures on the
battlefield plus the number of artifacts on the battlefield"). `toughness
of that target 1` names a graveyard card target "that card's" and a
target of several permanent types "that permanent's". `power of attached` is
what the source enchants or equips ("tap enchanted creature; enchanted
creature deals count equal power of attached damage to you": "Tap
enchanted creature. It deals damage to you equal to its power.").

Discards: `discard hand` ("Discard your hand"), `discard all cards in hand`
("Discard all the cards in your hand"), `<who> discards all Trap card`
("discards all Trap cards"), `discard 2 cards unless you discard creature
card`, `that player discards 2 cards unless they discard land card` (the
player discards one such card or else two). `<who> exiles 1 nonland card from graveyard` (after `target opponent
exiles 1 nontoken creature they control then they ...`: "then they exile
a nonland card from their graveyard": a card of their choice matching
the filter). `<who> reveals hand discarding
may nonland card` ("You may choose a nonland card from it."), `look at hand
of target player discarding may nonland card` ("look at target player's
hand. You may choose ..."), `look at hand of target player discarding card`
("Look at target player's hand and choose a card from it."). `<who>
reveals hand exiling nonland card` ("Target opponent reveals their hand.
You choose a nonland card from it and exile that card."; `exiling
nonland card split`: "You choose a nonland card from it. Exile that
card."; `... split until this leaves`: "Exile that card until this
creature leaves the battlefield.", CR 610.3: linked to the creature, it
returns to its owner's hand when the creature leaves, and nothing is
exiled if the creature has already left; `reveals hand and exiling
nonland card until this leaves`: "reveals their hand and you choose a
nonland card from it. Exile that card until ..."; `look at hand of
target opponent exiling may nonland card until this leaves`: "look at
target opponent's hand. You may exile a nonland card from it until this
creature leaves the battlefield."). `nonland noninstant nonsorcery card` prints "a nonland
permanent card" (CR 110.4a). `discarding nonland with mana value 3 or less
card` prints "You choose a nonland card from it with mana value 3 or less."
(the qualifier after "from it"), and `discarding may nonland card; if you
don't, that player discards 2 cards` "You may choose a nonland card from it.
If you do, that player discards that card. If you don't, ...". A sentence
may start `then` ("Then discard two cards unless ..."). After `return
target permanent to hand`, `that player` is that permanent's owner:
`return target permanent to hand; then that player discards 1 card`
("Return target permanent to its owner's hand. Then that player discards
a card."). `return this to its
owner's hand` in a spell returns the resolving spell from the stack.

### Revealing a card and acting on what it is

`reveal top card` is "reveal the top card of your library", `look at top
card` "look at the top card of your library", `target player reveals top
card` "Target player reveals the top card of their library", and `reveal
top card of target opponent` "Reveal the top card of target opponent's
library" (`look at top card of target player`: "look at the top card of
target player's library"; `of each player`: "of each player's library").
The card stays where it is (CR 701.20b) and is `it` / `that
card` for the rest of the ability. `reveal until nonland card` is "reveal
cards from the top of your library until you reveal a nonland card" (`its
controller reveals until land card`: "Its controller reveals cards from
the top of their library until they reveal a land card"); the revealed
cards stay in the library, the last one (the card that matched, if any)
is `that card`, and with no match the whole library is revealed. `exile until
nonland card` is "exile cards from the top of your library until you
exile a nonland card" (`target opponent exiles until nonland card`:
"Target opponent exiles cards from the top of their library until they
exile a nonland card"): the cards stay exiled and the matching one is
`that card`. `exile until total mana value 4` is "Exile cards from the
top of your library until you exile cards with total mana value 4 or
greater." (`each opponent exiles until total mana value 5`: "Each
opponent exiles cards from the top of their library until they have
exiled cards with total mana value 5 or greater this way."): the
library's top cards are exiled one at a time until their mana values
add up to that much, or the library runs out. `may cast that card this turn` ("You may cast that card this
turn."; `may play that card next turn first`: "Until the end of your next
turn, you may play that card."; `free`: "... without paying its mana
cost"; `it` for "it"; the durations are impulse's; `may play that card
remains`: "You may play that card for as long as it remains exiled.") lets
the controller play or cast the resolution's card from exile, also after
`exile target ... card from your graveyard` and `that player exiles 1 card
from hand` (the card they chose; Fiend of the Shadows). `may cast target Zombie creature card from
your graveyard this turn` ("You may cast target Zombie creature card from
your graveyard this turn.", Zul Ashur; `may play target ...` for "play")
gives the same permission for the targeted cards where they are, in a
graveyard or in exile, paying their costs with the usual timing (CR
601.3); it ends with the duration or when the card leaves that zone.
`may cast this card from graveyard as adventure next turn` ("you may cast
it from your graveyard as an Adventure until the end of your next turn",
Mosswood Dreadknight's dies trigger) gives it for the dead card, and only
its Adventure half may be cast (CR 715.3).
`owner may cast it remains first free` ("For as long as that card remains
exiled, its owner may cast it without paying its mana cost.", Release to
the Wind) gives the permission to the card's owner instead (CR 108.3);
`owner may play target 1 remains` ("For each of those cards, its owner
may play it for as long as it remains exiled.", Extraordinary Journey;
`next turn` is "until the end of their next turn", the owner's) gives it
to each targeted card's owner, wherever the targets went.
A permission (`may play that card ...`, `may play target ...`, `impulse
...`) takes a rider last, seen by both the offer and the payment of the
spell cast by it (CR 601.3): `any type` (", and mana of any type can be
spent to cast that spell", "... to cast them" after an impulse of several
cards; `any type it`: "... to cast it"; CR 609.4b: each colored, hybrid,
and {C} symbol of the total cost is paid with any mana, after cost
changes; Phyrexian and snow symbols are unchanged), `spend any color`
(", and you may spend mana as though it were mana of any color to cast
that spell": {C} still needs colorless mana), `spend any type this way`
(". If you cast a spell this way, you may spend mana as though it were
mana of any type to cast it", Outrageous Robbery), then `costs {2} more`
(". A spell cast this way costs {2} more to cast", Elite Spellbinder;
`costs {2} more opponent`: "A spell cast by an opponent this way ...",
Soul Partition, only for an owner who is an opponent of the controller),
then `exile instead` (". If that spell would be put into a graveyard,
exile it instead", Dire Fleet Daredevil: exiled as it leaves the stack), then `pay life` (". If you cast a spell this way, pay life equal to its mana value rather than pay its mana cost", Inside Information: `impulse X this turn from target opponent pay life`; an alternative cost, X is 0, offered only with that much life, CR 118.9, 119.4; a land is played as usual).
Hostage Taker: `exile another target creature or artifact until this
leaves; may cast that card remains any type`.
`put that card into hand and the rest on
bottom of library random` is "Put that card into your hand and the rest
on the bottom of your library in a random order." (the other revealed
cards; `random` after any `on bottom of library` shuffles them first).

After either, `if it is land card, <sentence>` is "If it's a land card,
...", `if it isnt land card, ...` "If it isn't a land card, ...", `if
that card is Goblin card, ...` "If that card is a Goblin card, ...", and
without `card` (`if it is land, ...`) "If it's a land, ..."; with no card
revealed (an empty library) neither holds. `otherwise, <sentence>` after
it happens when it didn't. The card or cards move with `put it into
hand`, `put that card into hand`, `put it into graveyard` (`put that card
into graveyard of that player`: "into that player's graveyard", after
looking at another player's library), `put it onto
battlefield [tapped]` (under your control, CR 110.2a), `put it on bottom of
library`, and for every revealed card still in the library `put all
revealed into hand` ("put all cards revealed this way into your hand"),
`put all other revealed into graveyard` ("all other cards revealed this
way"), `put the revealed on bottom of library` ("Put the revealed cards on
the bottom of your library in any order": in the order revealed), and with
the revealing player as subject `then puts those revealed into graveyard`
("..., then puts those cards into their graveyard"). `mana value of that
card` counts "that card's mana value" (`mana value of the card`: "the
card's mana value"; `its mana value`: "its mana value"; `reveal top card
and put that card into hand; you lose count equal its mana value life` is
Dark Confidant's "reveal the top card of your library and put that card
into your hand. You lose life equal to its mana value."; `choose any target; ...; deal damage
equal mana value of that card to that target 1`: "Choose any target. ... deals
damage equal to that card's mana value to that permanent or player"),
and `its mana value` counts the same card printed "its mana value"
(`reveal top card and put that card into hand; you lose count equal its
mana value life`: "... put that card into your hand. You lose life equal
to its mana value.", as it last existed once moved, CR 608.2h; in a
triggered ability `mana value of it` is the triggering object's); `that player shuffles` is "Then that player
shuffles." (CR 701.24), and `target player shuffles library` "Target player
shuffles their library." (`shuffle library`: "shuffle your library"); `you may have that player
shuffle` is "You may have that player shuffle." (Portent: the controller
decides, that player shuffles).

After `if it is ...`, `if you dont put it into hand, <sentence>` is "If
you don't put the card into your hand, ..." (`onto battlefield`, `into
graveyard`, `on bottom of library` likewise): it happens when the card
didn't match or the optional put was declined. `if it shares creature type with this, ...` is kinship's "If it shares a
creature type with this creature, ..." (changeling's every type
included); `reveal it` reveals a looked-at card where it is ("you may
reveal it. If you do, ..."; `reveal that card`: "You may reveal that
card."), and `if revealed instant or sorcery card, <sentence>` is "If an
instant or sorcery card is revealed this way, ...": only if the card is
revealed to every player where it is, so declining "you may reveal that
card" stops it (Delver of Secrets). `draw 1 card and reveal it` is "Draw a card
and reveal it." with the drawn card `it` after (`draw another card` is
"draw another card", one card, after an earlier draw: Hymn of the
Faller), and `discard it` discards
it ("If it isn't a land card, discard it.").

A revealed card is known to every player and a looked-at one (also the
cards a player scries) to that player, until it moves or its library is
shuffled (`GameObject::revealed`); a search's `determinize` keeps known
cards where they are in a library, as it does revealed cards in a hand.

```
triggered when this enters: reveal top card; if it is land card, put it onto battlefield; otherwise, put that card into hand
spell: choose any target; reveal until nonland card; this deals damage equal mana value of that card to that target 1; put the revealed on bottom of library
activated {2}, once: reveal top card; if it is land card, this gets +1/+0 and gains first strike
```

### Moving counters

`move all counters from target creature onto another target creature`
("Move all counters from target creature onto another target creature")
and `move 1 +1/+1 counter from this onto target creature` ("Move a +1/+1
counter from this artifact onto target creature") take the counters off
the first permanent and put them on the second (CR 122.5); nothing moves
if the second is gone. `move any number +1/+1 counters from this onto
another target creature` ("Move any number of +1/+1 counters from this
creature onto another target creature", Scrounging Bandar): how many is
chosen as it resolves (scenarios: `choose "2"`; unscripted, all). `target 1 can't be targeted by opponents this turn` prints "Target creature
can't be the target of spells or abilities your opponents control this
turn." (Vines of Vastwood; `by spells`: "... of spells this turn."),
judged against the spell's controller, not the creature's. `put its counters on target creature you control`
in a dies trigger puts the counters the source had as it last existed on
the battlefield, of every kind (CR 608.2h); `put its +1/+1 counters on
target creature you control` only those of one kind (Selfless Police
Captain). After `exile target card from
an opponent's graveyard`, `that player` is the card's owner (Carrion
Locust's "If it was a creature card, that player loses 1 life"; `if target
1 was creature` prints "If it was a creature card" for a graveyard card).

### Dice, coins, votes, and "at random"

Every random outcome draws on the game's seeded random number generator,
the one shuffles use, so a game replays from its seed; a search that
determinizes the hidden state resamples it too.

`roll d20` is "Roll a d20." (CR 706). Its results table (CR 706.3) is the
`result` lines that follow it in the ability's `{ }` block, one row each,
printed as paragraphs after the roll; the roll is the ability's last
sentence:

```
  triggered when this enters {
    roll d20
    result 1 to 9: create 1 "1/1 red Goblin creature"
    result 10 to 19: create 2 "1/1 red Goblin creature"
    result 20: create 3 "1/1 red Goblin creature"
  }
```

prints "When this creature enters, roll a d20.", "1—9 | Create a 1/1 red
Goblin creature token.", "10—19 | Create two of those tokens.", "20 |
Create three of those tokens." (`result 20 or more` is "20+"). Only the
row holding the result happens. Each row names the source in full again
("This artifact deals 5 damage to that creature."), and a later row
creating the creature token the first row created prints "of those
tokens". `choose target creature then roll d20` ("Choose target creature,
then roll a d20.") names the target the rows act on (`that target 1`)
before the roll; it is the ability's target, chosen as usual.

`flip coin` is "flip a coin" (CR 705); the sentence after it, `if you win
the flip, <sentence>`, happens only if the controller wins it (half the
time: "Create two 1/1 red Elemental creature tokens, then flip a coin. If
you win the flip, return Molten Birth to its owner's hand.").

`clash` is "clash with an opponent" (CR 701.23): the controller and an
opponent each reveal the top card of their library, then each (the active
player first) puts it on the top or bottom; the sentence after it, `if you
win, <sentence>`, happens only if the controller's card had the greater
mana value (or the opponent revealed nothing). "If you win, that creature
gets an additional +2/+2" for a second pump of the same target. A
second pump of the same target scaled by `count each` prints "an
additional" too: `target creature gets +3/+3; target 1 gets +2/+2 count
each graveyard named this` ("It gets an additional +2/+2 until end of
turn for each card named Growth Cycle in your graveyard."). After a
condition, `also those creatures gain vigilance` prints "those creatures
also gain vigilance".

After a vote, `if more votes for death, <sentence>` is "If death gets
more votes, ..." and `if more votes for torture or tied, <sentence>` "If
torture gets more votes or the vote is tied, ..." (Tyrant's Choice): the
choice got more votes than any other (or at least as many).
`vote sprout or harvest` is "starting with you, each player votes for
sprout or harvest" (CR 701.38): each player in turn order from the
controller votes for one of the two words (as a `choose` among them), and
`count each votes for sprout` counts that word's votes in the rest of the
ability ("Put two +1/+1 counters on this creature for each sprout vote":
`put count each 2 votes for sprout +1/+1 counters on this creature`).
`vote return artifact or creature or enchantment card` is "starting with
you, each player votes for an artifact, creature, or enchantment card in
your graveyard. Return each card with the most votes or tied for most
votes to your hand."

At random: `random creature an opponent controls` is a recipient, "a
creature an opponent controls chosen at random", picked as the effect
happens (`deal 7 damage to random creature an opponent controls`);
`return 1 Zombie card from your graveyard to hand at random` ("return a
Zombie card at random from your graveyard to your hand"); and the discards
and reveals above.

### The mana spent to cast a spell

The static conditions `spent {B}` ("{B} was spent to cast this spell" for
an instant or sorcery, "... to cast it" otherwise), `spent {R}{R}`, `spent
3 red` ("at least three red mana was spent ...", adamant with `label
"Adamant"`) and `spent treasure` ("mana from a Treasure was spent ...")
read what paid for the card itself (CR 601.2h): `triggered when this enters
if spent {U}: return target creature to hand`, `if spent {W}, untap all
creature you control` in a spell. The permanent a spell becomes remembers
it; a card put onto the battlefield or a copy had nothing spent. `that
spell spent 4 mana` reads the spell that triggered the ability: after a
trigger it prints "if at least four mana was spent to cast it" (`triggered
whenever you cast a noncreature spell if that spell spent 4 mana: ...`),
in an effect "If five or more mana was spent to cast that spell" (`if that
spell spent 5 mana, put 2 +1/+1 counters on this creature instead`). After
the comma, `also` prints "also" after the subject (`if that spell spent 5
mana, also this creature gains double strike`: "this creature also gains
double strike"). After such a condition, `that much` in a cast trigger is
the mana spent to cast that spell (`if that spell spent 8 mana, sacrifice
this and it deals that much damage to each opponent`, Tellah, Great
Sage); without one it is refused there. `that spell kicked` likewise reads the triggering
spell's kicker (CR 702.33d): `triggered whenever you cast spell if that
spell kicked: ...` prints "Whenever you cast a spell, if that spell was
kicked, ..." (Bloodstone Goblin); plain `kicked` is the source's own
kicker. A spell names itself again after such a condition
("Adamant — If ..., Searing Barrage deals 3 damage ..."); a permanent's
trigger says "it" ("if {R} was spent to cast it, it gains haste").
The engine pays canonically and these riders are all bonuses, so for a
card whose text asks, payment spends what it wants where it can: generic
mana is paid with the named colors (as many colors as possible for
converge), and Treasures go first ("mana from a Treasure").

### Designations and progress tracks

Player designations (CR 725 the monarch, 726 the initiative, 731 day and
night, 702.131 the city's blessing, 702.179 speed, 701.54 the Ring) and
dungeons (CR 309):

- Effects: `become monarch` ("you become the monarch"), `take initiative`
  ("you take the initiative"; the initiative's own trigger then ventures
  into Undercity), `venture` ("venture into the dungeon"), `venture into
  undercity`, `the ring tempts you` ("the Ring tempts you": the emblem
  named The Ring, then a creature you control becomes your Ring-bearer,
  legendary and unblockable by creatures with greater power; tempted
  twice it loots as it attacks, three times a creature blocking it is
  sacrificed at end of combat, four times its combat damage to a player
  makes each opponent lose 3 life).
- Conditions (static conditions anywhere they are accepted): `you are
  monarch` ("you're the monarch"), `completed dungeon` ("you've completed
  a dungeon"), `city's blessing` ("you have the city's blessing"), `max
  speed` ("you have max speed").
- Ability lines: `ascend` ("Ascend": on a permanent, the city's blessing
  once its controller controls ten or more permanents; on an instant or
  sorcery, checked first as it resolves), `start your engines` ("Start
  your engines!": a speed of 1 for a controller with none; speed rises by
  one, at most once each turn, when an opponent loses life during your
  turn, to at most 4), `becomes day as this enters` ("If it's neither day
  nor night, it becomes day as this creature enters.").
- Effect `reduce speed of controller of that target 1`: "If that
  opponent's speed is greater than each other player's speed, reduce that
  opponent's speed by 1. This effect can't reduce their speed below 1."
- `label "Max speed" <static, activated, or triggered ability>`, `label
  "Max speed" spells you cast cost {1} less`: "Max speed — ..." (CR
  702.179e): the ability works only while its controller's speed is 4 (a
  static ability's or cost reduction's condition, an activated ability's
  activation condition, graveyard and mana abilities included; a
  triggered ability triggers only at max speed; a replacement effect, `label
  "Max speed" replace damage from permanent you control to opponent and
  permanent an opponent controls: plus 1`, applies only then). A dies trigger under it
  prints "Whenever a creature you control dies" without "this creature or
  another".
- Trigger: `whenever day becomes night or night becomes day` (CR 731.1a;
  not the first time it becomes day).
- `impulse 2 forever`: "Exile the top two cards of your library. You may
  play them." with no time limit.

The dungeons themselves (Lost Mine of Phandelver, Dungeon of the Mad
Mage, Tomb of Annihilation, Undercity) are data in
`mtg_core::dungeons`, added to the built-in pool after the corpus.

### Newer permanent types: battles, Rooms, Spacecraft, and Vehicles

Battles (CR 310) are written like any card with `type Battle — Siege`
and `defense 5`; a Siege is a `layout transform` card whose `face` is
its back. It enters with defense counters, damage removes them (CR
310.6), and at none it goes to the graveyard; a Siege's intrinsic
trigger (no line) exiles it as its last defense counter is removed and
offers to cast it transformed for free (CR 310.12b). Its protector is
its controller's opponent (another battle's, its controller), so its
controller can attack it and the opponent's creatures block for it
(CR 310.9). "this Siege" is its self-noun; "any target" includes
battles (CR 115.4).

- Triggers: `whenever this attacks a battle`, `whenever equipped
  creature deals combat damage to a player or battle` (the player, or
  the battle's protector, is "that player").
- Static condition: `attacked a battle this turn` ("it attacked a battle
  this turn": the source).
- Target: `target player or battle`; `target creature, planeswalker, or
  battle` is a filter.
- Effect: `double this power` ("double its power until end of turn",
  CR 701.10b: +X/+0 with X its power; see Double above).
- Effect: `you may put 1 instant, sorcery, or battle card from your
  graveyard on top of library chosen` ("you may put an instant,
  sorcery, or battle card from your graveyard on top of your library":
  the controller chooses, no target).

Rooms (CR 709.5) are `layout split` cards whose two `face`s are both
`type Enchantment — Room`; "this Room" is the self-noun. Casting a half
puts the Room onto the battlefield with that door unlocked; put onto the
battlefield otherwise, both doors are locked. The engine holds a
definition per set of unlocked doors (`CardDef::doors`), so a locked
door's name, cost, and abilities are absent. Unlocking a locked door by
paying its half's cost is a special action at sorcery speed (scenario
`unlock "Door"`).

- Triggers: `when you unlock this door` (on a half: as that door
  unlocks, whether as the Room enters or later, CR 709.5h), `whenever
  you fully unlock a Room` (CR 709.5i); eerie is `label "Eerie"
  triggered whenever enchantment you control enters and whenever you
  fully unlock a Room: ...`.
- Static condition: `2 unlocked doors` ("there are two or more unlocked
  doors among Rooms you control").
- Effects: `unlock a locked door of up to 1 target Room you control`
  ("Unlock a locked door of up to one target Room you control."),
  `unlock a locked door of a Room you control` (no target: any Room its
  controller controls), `lock or unlock a door of target Room you
  control`. The controller picks the Room and the door as it resolves
  (CR 608.2d); a door unlocked this way costs nothing and triggers `when
  you unlock this door` and `whenever you fully unlock a Room` as the
  special action does (CR 709.5f). A locked door triggers nothing.
- `unlock costs you pay cost {1} less` (a player static): "Unlock costs
  you pay cost {1} less." (generic mana only).

Sagas (CR 714) are `type Enchantment — Saga` cards whose chapter
abilities are `triggered chapter 1: ...` ("I — ..."), `triggered
chapter 1, 2: ...` ("I, II — ...", one ability triggering for each
number, CR 714.2b); `label "Wark" triggered chapter 1: ...` prints the
chapter's name after its numerals ("I — Wark — ..."). "this Saga" is the
self-noun (a Saga creature's is "this creature"). The engine reads the
final chapter from the abilities (`CardDef::final_chapter`): a Saga
enters with a lore counter (CR 714.3a), gets another as its
controller's precombat main phase begins (CR 714.3b), and is sacrificed
once its lore counters reach the final chapter and no chapter ability of
its own is waiting to leave the stack (CR 714.4). A chapter triggers as
lore counters put on the Saga take their number from below its chapter
number to at least it, however they are put there (proliferate too).

Classes (CR 716) are `type Enchantment — Class` cards whose abilities are
listed in order with `level 2 {R}{G}` lines between them ("{R}{G}: Level
2"): the abilities before the first are level 1's, those after a level
line that level's. `triggered when this becomes level 2: ...` ("When
this Class becomes level 2, ...", CR 716.2c). "this Class" is the
self-noun. The engine holds a definition per level (`CardDef::stages`,
the card itself level 1), each with the abilities the Class has at that
level and of the level lines only the next one, so "{R}{G}: Level 2" is
activated only as a sorcery while the Class is level 1 (CR 716.2a). The
level resolves as `Effect::BecomeStage`; the Class is the same object,
and leaves the battlefield as its card again, level 1.

Leveler cards (CR 711): `level up {2}` ("Level up {2}", CR 702.87a:
"{2}: Put a level counter on this. Activate only as a sorcery.") and
level symbols with their P/T boxes, `levels 1 to 4 pt 2/2` ("LEVEL 1-4"
over "2/2") and `levels 5 or more pt 5/5` ("LEVEL 5+" over "5/5"). The
abilities after a level line, up to the next, are that level's: as long
as the creature has that many level counters it has the box's base power
and toughness (layer 7b) and those abilities (CR 711.2a-b); below the
first range it has its printed `pt` (CR 711.5). A level's keyword is
written `keyword first strike` in place; a level's static, activated,
or triggered ability gets the level as a condition of its own, and any
other ability (a blocking restriction, protection) is granted to the
creature while the level holds.

Cases (CR 719) are `type Enchantment — Case` cards with `to solve
<condition>` ("To solve — You control three or more artifacts.", any
static condition, printed capitalized) and `solved <ability>` ("Solved —
<ability>", any ability line after `solved`). "this Case" is the
self-noun. Like a Class, a Case has a definition per state
(`CardDef::stages`): unsolved, the card itself, with its to-solve
ability, "At the beginning of your end step, if <condition>, this Case
becomes solved" (judged as it triggers and as it resolves, CR 719.2a),
and not its solved abilities; solved, the reverse (CR 719.2b). It stays
solved until it leaves the battlefield. Trigger `whenever you solve a
Case` (a Case you control becomes solved, CR 719.3a; the Case is "it").

Spacecraft: `station` ("Station": tap another untapped creature you
control, put charge counters equal to its power on this permanent,
only as a sorcery, CR 702.184) and `station 4: flying` ("4+ | Flying",
CR 721.2: with four or more charge counters it has the keywords and,
when the card has `pt`, is an artifact creature), and `station 2: <ability
line>` ("2+ | Other creatures you control get +1/+1."; any static,
activated, mana, or triggered ability, CR 721.2a: the permanent has it
only while it has that many charge counters, and it never makes the
permanent a creature). A Planet (a land with `station`) has no `pt` and
never becomes a creature. "this Spacecraft" is
the self-noun; the plural is "Spacecraft". A cost's tapped creature is
recorded for the resolution (`ThisWay::Tapped`). Static condition:
`attacked with Spacecraft this turn` ("you attacked with a Spacecraft
this turn"; any filter).

Vehicles (CR 702.122): `crews using toughness` ("This creature crews
Vehicles using its toughness rather than its power."), `saddles and
crews using toughness`, `saddles and crews plus 2` ("... saddles Mounts
and crews Vehicles as though its power were 2 greater."): what the
creature counts for when tapped for a crew (and, with `saddles`, a
saddle) cost. `can't crew Vehicles` is a restriction like `can't block`
(CR 702.122d): `static enchanted: has can't attack, can't block, can't
crew Vehicles` prints "Enchanted creature can't attack, block, or crew
Vehicles." `can't transform` is a restriction too (CR 701.27): no effect,
and no change of day or night, transforms the permanent; `static enchanted:
has can't attack, can't block, can't transform` prints "Enchanted creature
can't attack, block, or transform." (Bound by Moonsilver). In a trigger on `Vehicle you control attacks`, `that
Vehicle` is the attacking Vehicle.

`replace this dies: exile`: "If this creature would die, exile it
instead."; `replace this dies: library top`: "If this creature would die,
put it on top of its owner's library instead." (`library bottom`: "on the
bottom of its owner's library").

### Crossover-set pieces

Small forms the Avatar, Marvel, Final Fantasy, Turtles and Strixhaven
cards need:

| Form | Oracle and meaning |
|---|---|
| `additional cost may waterbend {4}` | "As an additional cost to cast this spell, you may waterbend {4}." (CR 601.2b, 701.67): an optional additional cost, chosen as the spell is cast (scenarios: `kicked`) and offered only when it can be paid; its generic mana may be paid by tapping artifacts and creatures, and only once it is chosen (the spell's own generic mana never). `if additional cost paid` reads it. |
| `you have no maximum hand size for the rest of the game` | The effect (CR 402.2): the player's maximum hand size is gone for good. |
| `triple target creature power and toughness` | "Triple target creature's power and toughness until end of turn" (CR 701.10): +2X/+2Y, X and Y as it resolves. |
| `if mayhem paid, ...` | "If this spell's mayhem cost was paid, ..." (the spell cast from its owner's graveyard for its mayhem cost). |
| `if her sneak paid this turn, instead ...` | "If her sneak cost was paid this turn, instead ..." (Karai): the source was cast for its sneak cost and entered this turn (`his`, `this`, or none for "its"). |
| `if target 1 warped, also target 1 gains trample and haste` | "If that creature was cast for its warp cost, it also gains trample and haste until end of turn." (Full Bore): the target, still on the battlefield, was a spell cast for its warp cost. |
| `create 3 "..."; if sneak paid, they enter tapped and attacking` | "Create three ... tokens. If this spell's sneak cost was paid, they enter tapped and attacking." (CR 508.4): only right after a sentence creating tokens; while the condition holds the tokens are created tapped and attacking (what the source attacks, else an opponent). |
| `target creature becomes unprepared`, `each single creature you control becomes prepared` | "... becomes unprepared" removes the prepared designation (CR 722.3a); `each single` prints "Each creature you control becomes prepared". |
| `whenever you cast prepared spell` | "Whenever you cast a prepared spell" (Codie, Ravenous Codex): a copy of a prepare spell cast from a prepared permanent (CR 722.3c); `copy it new targets` copies it. |
| `activated waterbend {X}, x can't be 0, during your turn: ...` | "X can't be 0." printed after the effects, before "Activate only ...": the activation's X is chosen from 1 up, and it is offered only when X 1 could be paid. |
| `activated {5}{W}, {T}: harness this`, `label "∞" triggered ... if harnessed: ...` | "Harness The Mind Stone." marks the permanent harnessed (until it leaves); an "∞ — ..." ability is the labeled trigger with the condition `if harnessed`, which prints nothing: it triggers only while the source is harnessed. |
| `activated {6}{B}, {T}, exile creature you control: ...` | "Exile a creature you control" as an activation cost: the payer chooses a matching permanent they control (`sacrificing "X"` in scenarios) and it is exiled. |
| `static each creature you control with toughness greater than power: has stations using toughness` | "... stations permanents using its toughness rather than its power." (Tapestry Warden): tapped for a station cost (CR 702.184a), the creature counts its toughness. |
| `power-up again` | "Each power-up ability of permanents you control can be activated an additional time." (Wonder Man): its controller's power-up abilities (CR 702.193a) may each be activated once more per such permanent. |
| `you take extra turn without power-up` | "Take an extra turn after this one. During that turn, power-up abilities can't be activated." (Kang the Conqueror): no player activates a power-up ability during that extra turn. |
| `whenever this becomes tapped for teamwork` | "Whenever Agent Maria Hill becomes tapped to pay a teamwork cost": the creature was one the engine tapped for a spell's teamwork cost. |
| `equip worthy {1}` | "Equip worthy {1}" (Mjölnir): equip onto a legendary non-Villain creature you control that's red and/or white. `replace damage equipped creature would deal: double` prints "Double all damage equipped creature would deal." |
| `delayed target 1: when that creature dies this turn, you earthbend 4` | "When that creature dies this turn, you earthbend 4." (Fatal Fissure): the earthbend's land is the delayed ability's own target, chosen as it triggers (`trigger "X" targeting "Forest"` in scenarios), not the spell's. |
| `activated {2}, {T}, sorcery: target creature that attacked this turn becomes prepared` | A target with "that attacked this turn" as an effect's subject. |
| `spell: choose target creature card with mana value 4 or less from your graveyard; ...` | "Choose target creature card in your graveyard with mana value 4 or less." (`choose` prints the graveyard right after the noun). |

### Wording variants

Forms that change only how Oracle words what an existing form does:

| Form | Oracle text |
|---|---|
| `return all attacking creature to owner's hand` | "Return all attacking creatures to their owner's hand." (the newer singular; `to hand` prints "their owners' hands") |
| `if you dont put it itself into hand, ...` | "If you don't put it into your hand, ..." (`put it` prints "the card") |
| `alternative cost {B}, if on battlefield 13 creature late` | "You may pay {B} rather than pay this spell's mana cost if there are thirteen or more creatures on the battlefield." (the condition last) |
| `cast only unless threshold` | "Threshold — You can't cast this spell unless there are seven or more cards in your graveyard." (other conditions print without the label) |
| `flash named as long as you control Desert` | "Colossal Rattlewurm has flash as long as you control a Desert." |
| `non-Faerie, nonland permanent` | A negated subtype written before a negated type prints first: "non-Faerie, nonland permanent". |
| `this isnt token` | "it isn't a token" (`this is nontoken` prints "it's not a token") |
| `if this isnt creature, this becomes 3/3 Angel creature with flying in addition` | "If this enchantment isn't a creature, it becomes a 3/3 Angel creature ..." (Answered Prayers; card types only, judged on the source as it resolves); `if this is Scout, this becomes ...` prints "If Kellan is a Scout, it becomes ..." |
| `target creature you control gains firebending 4` | "Target creature you control gains firebending 4 until end of turn." (Fire Nation Palace; a granted keyword with a number, like `toxic 1`) |
| `this spell costs {2} less as long as 4 card types in your graveyard` | "This spell costs {2} less to cast as long as there are four or more card types among cards in your graveyard." (Drag to the Roots; `if` prints "if") |
| `then the chosen target 1 loses ... life` | "Then the chosen player loses ... life" (Haunt the Network): `the chosen target N` as a sentence's subject, after `choose target opponent` |
| `you may exile it; may play that card from exile this turn` | "You may play that card from exile this turn." (Norin, Swift Survivalist; wording only) |
| `delayed target 1: return the exiled cards to battlefield under its owner's control at beginning of the next end step` | "Return the exiled cards to the battlefield under their owners' control at the beginning of the next end step." after `exile X target ...` (Hide on the Ceiling; each card in turn, as `those cards`) |
| `triggered whenever creature you control leaves the battlefield if it had counter: ...`, `... if it had one or more counters: ...` | "if it had a counter on it", "if it had one or more counters on it" (the trigger condition `if it had counters`, "if it had counters on it", in other words) |
| `triggered when this dies if it had no counters: ...` | "if it had no counters on it": of any kind, as it last existed (`if it had no time counters` names a kind) |
| `if you control at least 5 other Forest` | "if you control at least five other Forests" (the condition `you control 5 other Forest`; in a trigger about another Forest, when the source couldn't be one of them, as Roiling Canopy, a land that isn't a Forest, "other" excludes the triggering Forest instead) |
| `triggered whenever you cast Lesson or Saga or Shrine spell` | "Whenever you cast a Lesson, Saga, or Shrine spell" (three or more bare subtypes; two print "a Lesson or Shrine spell") |
| `that player gains control of this` | on a legendary card, "that player gains control of Kain" (the name again) |
| `you draw that much cards, create that much tapped "Treasure", then lose that much life` | "you draw that many cards, create that many tapped Treasure tokens, then lose that much life" (a list's subject isn't repeated after subjectless clauses; `then you lose` prints the same) |
| `you may target 1 no longer suspected` | "You may have it become no longer suspected." |
| `label "Descend 8" static as long as 8 permanent cards in your graveyard, this: +2/+2, ...` after a pump of the same subject | "... gets an additional +2/+2 ..." after any earlier pump static of the same subject, conditional or not (Akawalli) |
| `label "Revolt" spell: ...` beside another `spell:` line | rejected: one spell paragraph per card (it never resolved); write `spell { ...; label "Revolt" if ... }` |
| `if target 1 was token, draw 1 card` | "If it was a token, draw a card." (Sunpearl Kirin: as it last existed) |
| `exile target creature or planeswalker; if that target 1 was with mana value 3 or less, ...` | "If that permanent's mana value was 3 or less" (the noun it was targeted by, though it is a card in exile now) |
| `exile target creature you control then return it to battlefield ...; if that target 1 is Bird or Frog or Otter or Rat, ...` | "If that creature is a Bird, Frog, Otter, or Rat" (back on the battlefield, the target's own noun; three or more creature types as one list) |
| `cycling sacrifice land` | "Cycling—Sacrifice a land.": offered once per land that could be sacrificed, which is sacrificed as the cost (none: it can't be cycled) |
| a static ability's `as long as` condition on an object's state | Judged in the layers: tapped and untapped (of the source or what it's attached to), counters, `token`, `transformed` (of what you control), toxic; `attacking`, `blocking`, damaged and other states (`static enchanted: has first strike as long as this is blocking`) are rejected, since the layers wouldn't see them |
| `if target 1 is face-down, ...` and other state checks on a target | Every state qualifier is judged (`face-down`, `that attacked this turn`, ...), as it last existed if the target has left the battlefield |
| rejected | `prevent all damage ... of color of choice would deal this turn` (the prevention never saw the color); `each other creature you control` in a spell after another group (`other` excludes the spell, not the group: In Oketra's Name) |
| `that created token gains haste indefinitely`, `this gains ability { ... } indefinitely` | "That token gains haste.", "This creature gains \"...\"" (no duration, CR 611.2a: until the object leaves; `indefinitely` after a sentence makes everything in it that would last until end of turn last for good). A legendary card granting itself a quoted ability names itself in it ("Kellan ... gains \"Whenever Kellan deals combat damage ...\""). |
| `static plural artifact you control: add subtype Food, ...` | "Artifacts you control are Foods in addition to their other types and have ..." rather than "Each artifact you control is a Food ...". An added subtype and the card types added after it are one noun ("are Food artifacts", "is a Golem creature with base power and toughness 5/4"), and a quoted ability of a holder given a card type names "this permanent". |
| `reduced {1} during your turn` | "This ability costs {1} less to activate during your turn." |
| `the chosen target 1` | "the chosen creature", after "choose target creature" |
| `choose any number of target creature; put 1 +1/+1 counter on target 1` | "Choose any number of target creatures. Put a +1/+1 counter on each of them." (Nature's Panoply): the group named first, then acted on |
| `for each opponent, tap up to 1 target creature that opponent controls` | "For each opponent, tap up to one target creature that player controls." The game has two players, so it happens once, with one target an opponent controls (or none); `that opponent controls` is allowed only in such a sentence, and "those creatures" names its targets plainly ("Put a stun counter on each of those creatures."). |
| `for each player, destroy up to 1 target creature that player controls` | "For each player, destroy up to one target creature that player controls." One target for each player: with two players, up to two targets controlled by different players (CR 115.1); `that player controls` in such a sentence means this, and only `up to 1 target` is allowed. A sentence `for each player, [you] choose ...` is a choice instead (see `each player chooses`). |
| `choose up to 1 target creature`, `choose up to 1 another target creature` | "Choose up to one target creature.", "Choose up to one other target creature." (no effect of its own; the later sentences name it: `that target 1 can't be blocked and has base power and toughness 1/1`, "That creature can't be blocked this turn and has base power and toughness 1/1 until end of turn."; with none chosen they do nothing). `if target 1 is suspected, exile target 1; otherwise, suspect target 1` prints "If it's suspected, exile it. Otherwise, suspect it.": an `otherwise` may follow a target check. |
| `choose target creature you control and target creature you don't control` | "Choose target creature you control and target creature you don't control." (two targets named at once; no effect of its own) |
| `the full target 1`, `the chosen full target 1` | "the creature you control", "the chosen creature you control": the target named by its whole filter |
| `those targets fight each other`, `the chosen targets fight each other` | "Then those creatures fight each other.", "Then the chosen creatures fight each other." (CR 701.14a; the first two targets; "those creatures" is the plain noun when the targets differ) |
| `if the full target 1 is creature that entered this turn, ...` | "If the creature you control entered this turn, ..." (a target check as it resolves) |
| `if this sneak paid` | "If this creature's sneak cost was paid" |
| `count foreach land you control` | "For each land you control, create a Treasure token." (the count leads the sentence, ahead of a subject: `you create count foreach ...` is "For each ..., you create ..."; `put count foreach ... +1/+1 counters on this creature` is "For each ..., put a +1/+1 counter on this creature"; the template author writes it for "For each ..., create a ... token") |
| `your opponents control 3 creature` | "your opponents control three or more creatures" (a static condition: the opponents' permanents counted together) |
| `at beginning of your next end step` | In a delayed trigger: "at the beginning of your next end step". |
| `this is with counter` | "this creature has a counter on it" (any kind) |
| `if excess damage dealt to that permanent this way, ...` | "If excess damage was dealt to that permanent this way, ..." (`to that creature` likewise; either prints the damaged target's noun: "to that creature" after "target creature"); inside either form a `count where excess damage dealt this way` prints "where X is that excess damage" |
| `8 cards are in your graveyard` | "eight or more cards are in your graveyard" (`8 cards in your graveyard`: "there are eight or more cards in your graveyard") |
| `if you controlled it, ...` | "If you controlled it, ..." about the last target (`if you controlled target 1` prints "that creature") |
| `that created token` | "that token", the token just created (`that token` prints "it") |
| `that created land` | "that land", the land card just put onto the battlefield (Fabled Passage) |
| `target creature card with power 2 or less from your graveyard` | "target creature card with power 2 or less from your graveyard" (the power after the noun) |

| `noalso each Warrior creature you control gains lifelink` | "Warrior creatures you control gain lifelink until end of turn." after a pump of all your creatures (without `noalso`: "also gain", Rush of Battle) |
| `return 1 land card or Elf card from your graveyard to hand chosen` | "return a land card or Elf card from your graveyard to your hand" (each side its own "card") |
| `if at least one creature card was exiled this way, ...` | "If at least one creature card was exiled this way, ..." (the same check as `if creature card was exiled this way`) |
| `if target 1 isnt creature, ...`, `if target 1 is not creature, ...` | "If it isn't a creature, ...", "If it's not a creature, ..." (the check `if target 1 is noncreature`; card types only) |
| `at beginning of each player's end step` | "At the beginning of each player's end step" (`each end step` prints "the end step"; the same trigger) |
| `target 1 is Spirit in addition`, `it is Angel with flying in addition`, `target 1 is Skeleton in addition and has no abilities` | "It's a Spirit in addition to its other types", "It has flying and is an Angel in addition to its other types", "It's a Skeleton in addition to its other types and has no abilities" (indefinitely, like `becomes ... in addition indefinitely`; "That creature is a Hero ..." after `that creature`) |
| `if you dont draw this way, ...`, `if you dont put into hand this way, ...`, `if you didnt put into hand this way, ...` | "If you don't draw a card this way, ...", "If you don't put a card into your hand this way, ...", "If you didn't put a card into your hand this way, ..." (`dont draw` is `if you dont, ...`; the two "put" forms check that no card was put into your hand by the effects before, so a search that finds nothing or a dig that takes nothing counts; the source is named again after them) |
| `target 1 gets +1/+0 also if you control Mouse` | "It also gets +1/+0 until end of turn if you control a Mouse." (`... if <condition>` with "also" after the subject) |
| `target player sacrifices 1 creature of their choice other than enchanted` | "target player sacrifices a creature of their choice other than enchanted creature" (never the enchanted creature) |
| `put those counters on up to 1 target creature` | In a triggered ability: "put those counters on up to one target creature", the triggering object's counters (`put its counters on` names them too in a trigger about another object, printed "its": Host of the Hereafter; in `when this dies`, the source's) |
| `count where greatest power Giant you control` | "the greatest power among Giants you control" (a creature type alone after `greatest power`) |
| `copy it and new targets` | "copy it and you may choose new targets for the copy" (`copy it new targets` prints "copy it. You may choose new targets for the copy.") |
| `whenever other creature or artifact you control is put into a graveyard from the battlefield`, `when enchanted artifact is put into a graveyard` | The dies trigger worded so for a filter that includes creatures ("dies" otherwise), and the older wording without "from the battlefield" (only for `enchanted ...`, which can only be on the battlefield; for another filter it would mean from anywhere and is a compile error) |
| `create 1 "0/0 green and blue Fractal creature" then put its counters on that token` | "create a 0/0 green and blue Fractal creature token, then put this creature's counters on that token" (Ambitious Augmenter: the source's counters as it last existed) |
| `whenever one or more Merfolk you control attack a player` | "Whenever one or more Merfolk you control attack a player" (the same trigger as `whenever you attack a player with one or more Merfolk`) |
| `you may attach any number Equipment you control to target creature you control chosen` | "you may attach any number of Equipment you control to target creature you control" (Beatrix: the target declared by the attach itself; `to target 1` names one declared before) |
| `put 1 time counters on this; then if there are 7 or more time counters on this, remove count where time counters on this time counters from this and ...` | "put a time counter on this artifact. Then if there are seven or more time counters on it, remove those counters and ..." (Cursed Recording: the source named before is "it"; removing all of the counted kind is "those counters") |
| `each single creature target opponent controls loses all abilities and becomes Coward base 1/1 in addition indefinitely` | "each creature target opponent controls loses all abilities, becomes a Coward in addition to its other types, and has base power and toughness 1/1" (Curious Colossus) |
| `if you sacrificed permanent this way, ...` | "If you sacrificed a permanent this way, ..." (any permanent) |
| `count foreach creature card graveyarded this way` | "For each creature card put into a graveyard this way, ..." (Dread Summons: any player's graveyard; without a filter, `graveyarded this way` prints "put into your graveyard") |
| `replace damage from permanent you control to opponent and permanent an opponent controls: plus count equal fire counters on this` | "... it deals that much damage plus an amount of damage equal to the number of fire counters on this enchantment instead." (Fated Firepower; `plus count where ...` prints "instead it deals that much damage plus X, where X is ...") |
| `delayed this: copy that spell new targets when next you cast spell this turn` | "copy the next spell you cast this turn when you cast it. You may choose new targets for the copy." (Flamehold Grappler: the effect first; the same delayed trigger as `when next you cast spell this turn, copy that spell new targets`) |
| `dig X reveal take any permanent with mana value X or less card may onto battlefield rest graveyard reword` | "... You may put any number of permanent cards with mana value X or less from among them onto the battlefield. Then put all cards revealed this way that weren't put onto the battlefield into your graveyard." (Genesis Wave: a dig's mana value X is the spell's X when the dig's count isn't a count of its own) |
| `dig 6 take land card may onto battlefield tapped rest bottom random short` | "... Put the rest on the bottom in a random order." (Kaslem's Stonetree) |
| `whenever you put one or more +1/+1 counters on one or more other Hero you control` | "Whenever you put one or more +1/+1 counters on one or more other Heroes you control": once for the permanents counters were put on together (CR 603.2c; without `one or more` before the filter, once for each permanent) |
| `triggered at beginning of your draw step ...: you lose 1 life, and you draw 1 card` | "you lose 1 life and you draw an additional card" (in your own draw step; Lord Skitter's Blessing) |
| `return count that many sacrificed this way creature card from your graveyard to battlefield chosen` | "Return that many creature cards from your graveyard to the battlefield." (Lich-Knights' Conquest) |
| `impulse count where plus one mana value of the sacrificed artifact this turn` | "Exile the top X cards of your library, where X is one plus the mana value of the sacrificed artifact." (`plus 1 ...` with a digit prints "1 plus the sacrificed creature's mana value") |
| `triggered whenever you discard a card: you may exile triggering card; ...` | "you may exile that card from your graveyard" (Moonstone) |
| `as this enters choose land type` then `enters tapped unless pay 2 life` | "As this land enters, choose a basic land type. Then you may pay 2 life. If you don't, it enters tapped." (one line, Multiversal Passage) |
| `replace damage bare creature you control that entered this turn would deal: double` | "If a creature you control that entered this turn would deal damage, it deals twice that much damage instead." (Neriv) |
| `distribute 2 +1/+1 counters among 1 to 2 target creatures; then if ..., double the number of +1/+1 counters on that target 1` | "... double the number of +1/+1 counters on those creatures" (Omnivorous Flytrap: `that target N` of a variable group is all of its targets; `those targets` prints "each of those creatures") |
| `return 1 non-Avatar creature or a planeswalker card from your graveyard to hand chosen` | "return a non-Avatar creature card or a planeswalker card from your graveyard to your hand" (the alternative with an article of its own) |
| `each opponent loses 3 life unless they discard 1 card else sacrifice creature` | "Each opponent loses 3 life unless they discard a card or sacrifice a creature." (Polygraph Orb; the same choice as `unless they sacrifice creature else discard 1 card`) |
| `opening hand` on a legendary card | "If Quicksilver, Brash Blur is in your opening hand, you may begin the game with him on the battlefield." (the full name, the card's pronoun) |
| `until end of turn, creatures you control get +2/+2 and gain menace and gains ability { ... }` | "Until end of turn, creatures you control get +2/+2 and gain menace and \"...\"" (Root Manipulation) |
| `flash first if 3 Lesson cards in your graveyard` | "If there are three or more Lesson cards in your graveyard, you may cast this spell as though it had flash." (the same as `flash if ...`) |
| `enters with count each plus 1 other creature you control +1/+1 counters` | "This creature enters with a +1/+1 counter on it plus an additional +1/+1 counter on it for each other creature you control." (Sheriff of Safe Passage) |
| `triggered whenever you cast a spell if that spell spent treasure: ...` | "Whenever you cast a spell, if mana from a Treasure was spent to cast it, ..." (Smaug) |
| `activated {10}, sacrifice this, reduced {1} where differently named land you control: ...` | "This ability costs {X} less to activate, where X is the number of differently named lands you control." (Survey Mechan: {1} less for each, as `reduced {1} each ...`) |
| `return up to 1 target creature card with mana value at most count where life you gained this turn from your graveyard to battlefield` | "return up to one target creature card with mana value X or less from your graveyard to the battlefield, where X is the amount of life you gained this turn" (Moseo: the X explained at the end) |
| `this creature deals 4 damage to any target and deal 3 damage to up to 2 another target any` | "Drakuseth deals 4 damage to any target and 3 damage to each of up to two other targets." (the name again; "other": not the source nor the first target) |
| `deal damage equal +1/+1 counters on it to any other target` | in a trigger about the source on a card with a `pronoun`: "the number of +1/+1 counters on him" (Red Hulk) |
| `label "Descend 8" choose one or more if 8 permanent cards in your graveyard { ... }` | "Descend 8 — Choose one. If there are eight or more permanent cards in your graveyard as you cast this spell, choose one or more instead." (Wail of the Forgotten) |
| `whenever other creature you control or a land you control is put into a graveyard from the battlefield` | "Whenever another creature you control or a land you control is put into a graveyard from the battlefield" (Long Feng: an alternative with its own article names its controller too) |
| `this gets +1/+0 count where mana spent that spell until end of turn; if that spell spent 4 mana, draw 1 card` | "... where X is the amount of mana spent to cast that spell. If X is 4 or more, draw a card." (Shantotto: the X named before) |
| `discard 2 cards unless you discard instant or sorcery or a creature with flying card` | "discard two cards unless you discard an instant or sorcery card or a creature card with flying" (Steamcore Scholar) |
| `create 1 tapped "Everywhere"` | "create a tapped colorless land token named Everywhere that is every basic land type" (a land token of all five basic land types, `cards/tokens/everywhere.card`) |
| `distribute 2 +1/+1 counters among 1 to 2 another target andor Vehicle or creature you control` | "distribute two +1/+1 counters among one or two other target Vehicles and/or creatures you control" (Cloudspire Skycycle: `N to M another target`) |
| `until end of turn, this becomes 2/1 blue red Elemental creature with ability { static during your turn, this: has first strike }` | "Until end of turn, this land becomes a 2/1 blue and red Elemental creature with \"During your turn, this creature has first strike.\" It's still a land." (Restless Spire) |
