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
creature token named Cadet"); so is a noncreature token with no subtype
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
\"...\"").

## Multi-face cards

A split card, adventurer, omen, or double-faced card is one block named as
Scryfall names it (`card "Curious Pair // Treats to Share"`) with a
`layout` line (`adventure`, `omen`, `transform`, `split`, `prepare`,
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

## Header

| Line | Meaning |
|---|---|
| `oracle_id 2b93787d-...` | The Scryfall `oracle_id` the card was authored against. Required for cards. |
| `cost {1}{G}` | Mana cost. Omit for lands and tokens. Hybrid `{R/G}`, monocolored hybrid `{2/W}`, and Phyrexian `{R/P}` symbols are accepted (CR 107.4e–f); the engine pays them in a canonical order and pays 2 life for a Phyrexian symbol only when mana can't. Snow `{S}` (CR 107.4h, in any cost) is paid by tapping a different snow permanent for each symbol (the rest of the cost from the other sources and the pool); mana already floating in the pool pays no `{S}`, since the pool doesn't remember where its mana came from. `protection from snow` prints "Protection from snow". |
| `type Legendary Creature — Goblin Shaman` | The type line: supertypes, types, then subtypes after `—` (or `-`). |
| `pt 2/2` | Power and toughness. Omit when a characteristic-defining ability sets them (Scryfall prints `*`). When it sets one of them, write the other and `*` for it: `pt */1` with `cda power ...`, `pt 2/*` with `cda toughness ...` (`*` is 0 outside the battlefield, CR 208.2a). |
| `loyalty 3` | A planeswalker's printed loyalty. |
| `defense 5` | A battle's printed defense (CR 310.4): it enters with that many defense counters. |
| `short_name "Ajani"` | The name a legendary card's text calls itself by when it isn't the part before a comma (CR 201.5): "put a loyalty counter on Ajani" on Ajani Resolute. The template author adds it when the text uses the name's first word alone. |
| `pronoun he`, `pronoun she`, `pronoun they` | Wording only: after naming it, the card's text calls itself "he"/"him"/"his", "she"/"her", or "they"/"them"/"their" instead of "it" (and `they` makes a trigger about it plural: "Whenever Bebop & Rocksteady attack or block") ("Put a +1/+1 counter on Abomination. He fights up to one target creature an opponent controls."; "Morlun enters with X +1/+1 counters on him."). The template author adds it for a legendary card whose Oracle text uses those words. |
| `color green` | Color indicator (CR 204): for cards and tokens whose color is not their cost's. |
| `keywords flying, deathtouch, lifelink` | Keyword abilities, in printed order: `deathtouch`, `defender`, `double strike`, `first strike`, `flash`, `flying`, `haste`, `hexproof`, `indestructible`, `lifelink`, `menace`, `prowess`, `reach`, `trample`, `vigilance`, and the landwalks `plainswalk`, `islandwalk`, `swampwalk`, `mountainwalk`, `forestwalk` (CR 702.14), `protection from white` (and the other colors, CR 702.16), `devoid` (CR 702.114), `changeling` (CR 702.73), `shroud`, `fear`, `intimidate`, `shadow`, `horsemanship`, `skulk`, `wither`, `infect` (damage to players as poison counters; ten lose, CR 704.5c), `exalted`, `persist`, `undying`, `flanking`, `convoke` (CR 702.51: untapped creatures may be tapped to pay {1} or one mana of their color), and `improvise` (CR 702.126: artifacts pay {1}), and `banding` (CR 702.22: attacking bands, blocked as a group, the banding player dividing the combat damage; "bands with other" is not supported), `decayed` (CR 702.147: can't block; when it attacks, it's sacrificed at end of combat), and `phasing` (CR 702.26: phases in or out before its controller untaps). The restrictions `can't attack`, `can't block`, `attacks each combat if able`, `can't be blocked`, `can't attack alone`, and `can't block alone` (both together print "can't attack or block alone", CR 506.5), `doesn't untap` ("This creature doesn't untap during your untap step."; granted, "Enchanted creature doesn't untap during its controller's untap step."), `can't become untapped` (no effect or untap step untaps it, and a stun counter stays on it; granted, "Enchanted creature loses all abilities and can't become untapped."), and the blocking requirements and allowances (CR 509.1a, 509.1c) `blocks each combat if able`, `must be blocked if able`, `lure` ("All creatures able to block this creature do so."), `can block an additional creature each combat`, and `can block any number of creatures`, and `can attack as though it didn't have defender` (CR 702.3b; granted to a group, "... as though they didn't have defender"; with `can't be blocked` after it, "... and it can't be blocked") are written as keywords too and print as sentences ("This creature can't block."). A `keyword` line after an ability line prints in that place instead of first (a bridge land's "Indestructible" after "enters tapped"). |
| `additional cost sacrifice creature` | "As an additional cost to cast this spell, ..." (CR 601.2b). Same syntax as an activated ability's cost. |
| `additional cost sacrifice creature` then `additional cost or {3}{B}` | "As an additional cost to cast this spell, sacrifice a creature or pay {3}{B}." (CR 601.2b): two additional costs, of which the caster pays one, chosen as the spell is cast (before the kicker and X); only those that can be paid are offered. A mana part prints "pay {3}{B}", and the total cost includes it (CR 601.2f). |
| `alternative cost sacrifice Mountain`, `alternative cost sacrifice 2 Mountain`, `alternative cost sacrifice Mountain, if your turn`, `alternative cost {1}, return basic land you control` | An alternative cost of the card's own (CR 118.9): "You may sacrifice a Mountain rather than pay this spell's mana cost.", "You may sacrifice two Mountains ...", "If it's your turn, you may ...", "You may pay {1} and return a basic land you control to its owner's hand ...". Mana and one kind of permanent (sacrificed, returned with `return`, or tapped with `tap untapped creature you control`), offered while the condition holds; the chosen permanent is the first and the engine picks the rest. Not combined with an additional cost that also chooses a permanent. `alternative cost pay 4 life, if you control Swamp` pays life (offered only with that much life, CR 119.4); `alternative cost if cast another white spell this turn`, with no cost at all, prints "If you've cast another white spell this turn, you may cast this spell without paying its mana cost."; `alternative cost reveal hand, if hand has no land card` prints "If you have no land cards in hand, you may reveal your hand rather than pay this spell's mana cost." (every card left in the hand is then known to every player, CR 701.20). `alternative cost each other player gains 6 life, if you control Forest` prints the older wording "If you control a Forest, rather than pay this spell's mana cost, you may have each other player gain 6 life." (the cost is paid by each other player gaining that life). |
| `alternative cost sacrifice Mountain`, `alternative cost sacrifice 2 Mountain`, `alternative cost sacrifice Mountain, if your turn`, `alternative cost {1}, return basic land you control` | An alternative cost of the card's own (CR 118.9): "You may sacrifice a Mountain rather than pay this spell's mana cost.", "You may sacrifice two Mountains ...", "If it's your turn, you may ...", "You may pay {1} and return a basic land you control to its owner's hand ...". Mana and one kind of permanent (sacrificed, returned with `return`, or tapped with `tap untapped creature you control`), offered while the condition holds; the chosen permanent is the first and the engine picks the rest. Not combined with an additional cost that also chooses a permanent. `alternative cost pay 4 life, if you control Swamp` pays life (offered only with that much life, CR 119.4); `alternative cost if cast another white spell this turn`, with no cost at all, prints "If you've cast another white spell this turn, you may cast this spell without paying its mana cost."; `alternative cost reveal hand, if hand has no land card` prints "If you have no land cards in hand, you may reveal your hand rather than pay this spell's mana cost." (every card left in the hand is then known to every player, CR 701.20). `alternative cost {1}{W}, exile creature card from graveyard` prints "You may pay {1}{W} and exile a creature card from your graveyard rather than pay this spell's mana cost."; `alternative cost exile top 3 black cards from graveyard` "You may exile the top three black cards of your graveyard rather than ..." (the most recent matching cards; without `top`, the oldest). An alternative cost prints before the keywords. |
| `flash as long as you control Merfolk`, `flash if you control Human` | "This spell has flash as long as you control a Merfolk." (CR 702.8): the card may be cast as though it had flash while the condition holds. The `if` form prints "You may cast this spell as though it had flash if you control a Human." |
| `flash with cleanup sacrifice` | "You may cast this spell as though it had flash. If you cast it any time a sorcery couldn't have been cast, the controller of the permanent it becomes sacrifices it at the beginning of the next cleanup step." (Spider Climb): cast other than in its controller's main phase with an empty stack (CR 307.1), the permanent gets a delayed trigger at the next cleanup step (CR 514.3a). |
| `cast only during combat`, `cast only during opponent upkeep`, `cast only during combat before blockers`, `cast only during combat after blockers`, `cast only during declare attackers`, `cast only during declare attackers if attacked`, `cast only if you control creature` | "Cast this spell only during combat." (CR 601.3), printed first: the card is offered for casting from the hand only while the condition holds for its caster (any static condition, or a part of the turn as for activation restrictions). `if attacked` prints "during the declare attackers step and only if you've been attacked this step" (a creature attacks the caster; not one of their planeswalkers). |
| `kicker {1}{U}` | Kicker (CR 702.33): an optional additional mana cost, chosen as the spell is cast. Prints "Kicker {1}{U}" after the keywords. Effects that depend on it: `if kicked, <clause>` in a spell, `if kicked` as a trigger condition. |
| `flashback {4}{R}` / `flashback {1}{U}, pay 3 life` / `flashback sacrifice 3 creature` / `flashback tap 3 untapped white creature you control` | Flashback (CR 702.34): the card may be cast from the graveyard for this cost and is exiled as it leaves the stack. The cost is mana, life, and a chosen permanent to sacrifice, return, or tap (chosen after targets like an additional cost's, so not on a card that also has an additional cost). Prints last, as "Flashback {4}{R}" or "Flashback—Sacrifice three creatures." |
| `oracle_mismatch "reason"` | The rendered text is allowed to differ from Oracle; the gate reports it as allowed. Use it for cards whose text includes an ability the engine does not implement. |

## Abilities

| Line | Meaning |
|---|---|
| `mana {G}` / `mana {W} or {U}` / `mana any` / `mana {U} or chosen` | "{T}: Add {G}." A mana ability (CR 605); `any` is "one mana of any color", `chosen` "one mana of the chosen color" (the color chosen as the permanent entered; none chosen, no mana). Basic land types grant theirs intrinsically. |
| `mana any, sacrifice this` | "{T}, Sacrifice this artifact: Add one mana of any color." (Treasure, Lotus Petal). Payment uses such sources after all others. |
| `mana {C} only to cast instant or sorcery spell`, `mana any only to cast creature spell`, `mana {U} only to cast artifact spell or activate`, `mana {R} only to cast artifact spell or to activate`, `mana {C} only to activate or cast artifact spell`, `mana {R} only to cast Elemental spell or Chandra planeswalker spell`, `mana any only to cast Lesson or Shrine spell`, `mana {U} not to cast nonartifact spell` | "{T}: Add {C}. Spend this mana only to cast an instant or sorcery spell.", "... only to cast an artifact spell or activate an ability.", "... or to activate an ability.", "... only to activate an ability or cast an artifact spell.", "This mana can't be spent to cast a nonartifact spell." Two alternatives each written with `spell` print whole ("an Elemental spell or a Chandra planeswalker spell"); two types alone with one `spell`, `Lesson or Shrine spell`, share it ("a Lesson or Shrine spell"). Restricted mana (CR 106.6): one kind of mana; the engine taps the source only to pay for what the restriction allows (any activated ability for `activate`; for `not to cast`, other spells and any activated ability, though not costs an effect asks for), so its mana is spent at once and never sits in the pool. |
| `mana {U} only to cast artifact spell or activate of artifact`, `mana {U} only to foretell or cast instant or sorcery spell` | "Spend this mana only to cast an artifact spell or activate an ability of an artifact source." (`of <filter>` after `activate`: only abilities of matching sources, cards in hand and graveyard included); "Spend this mana only to foretell a card from your hand or cast an instant or sorcery spell." (the foretell special action's {2}, CR 702.143a). |
| `spend only black mana on x` | "Spend only black mana on X." (CR 107.3): the X in the spell's cost is paid with that color only. After an activated ability with {X} (`activated {X}: deal X damage to each creature and each player` then `spend only black mana on x`), it is that ability's X, printed in its paragraph (Crypt Rats). |
| `cycling {2}`, `cycling pay 2 life` | Cycling (CR 702.29): "{2}, Discard this card: Draw a card.", activated from the hand. A cost other than mana prints "Cycling—Pay 2 life." (activated only with that much life, CR 119.4). |
| `plainscycling {2}`, `basic landcycling {1}{G}`, `landcycling {2}` | Typecycling (CR 702.29e): discard to search for a card of that type. Two of them, one per line (`islandcycling {2}` then `swampcycling {2}`), match Oracle's one line "Islandcycling {2}, swampcycling {2}"; each is its own ability. |
| `enchant creature` | An Aura's "Enchant" (CR 702.5), with a target specification. |
| `spell: <effects>` | What an instant or sorcery does on resolution. |
| `gift card`, `gift food`, `gift tapped fish`, `gift treasure` | Gift (CR 702.174): "Gift a card" ("a Food", "a tapped Fish", "a Treasure"), printed first. As the spell is cast its caster may promise the gift to the opponent; an instant or sorcery gives it (they draw a card, or create the token) before its other effects as it resolves, a permanent by its own "When this permanent enters, if its gift cost was paid" trigger. The engine keeps a promised gift as the kicker flag, so the card's "if the gift was promised" is written with the kicker conditions: `if kicked, <clause>` ("If the gift was promised, ..."; a target first named there is chosen only if it was, CR 702.174m; the spell names itself again), `if kicked, also ...`, `triggered when this enters if kicked: ...`, `... if kicked` after a sentence, and `if not kicked, <clause>` ("If the gift wasn't promised, ..."). |
| `spree { mode {2}{B}: <effects> mode {1}: <effects> }` | Spree (CR 702.172): one to three modes, each with an additional cost ("Spree", then "+ {2}{B} — Destroy target creature."). The caster chooses one or more of them and pays their costs on top; each chosen mode keeps its own targets. The engine's modes are the sets of the printed modes, the set with bits N being mode N (in scenarios, `mode 3` is the first two). |
| `pawprint 5 { mode 1: <effects> mode 2: <effects> mode 3: <effects> }` | "Choose up to five {P} worth of modes. You may choose the same mode more than once." (CR 700.2d, 700.2i), then "{P} — ...", "{P}{P} — ...": each mode with its number of pawprints. The engine's modes are every nonempty combination, a mode repeated allowed, whose pawprints total at most the limit (at most sixteen), each instance with its own targets, in printed order; they are listed in dictionary order of their modes' numbers (for 1, 2, 3 pawprints and five: 1, 1+1, 1+1+1, 1+1+1+1, 1+1+1+1+1, 1+1+1+2, 1+1+2, 1+1+3, 1+2, 1+2+2, 1+3, 2, 2+2, 2+3, 3; in scenarios `mode 13` is 2+2). |
| `choose one { mode: <effects> ... }` | A modal spell (CR 700.2), one `mode` line per mode. `choose one or both { ... }` takes two modes, either or both ("Choose one or both —"). As a triggered ability's effect (`triggered when this enters: choose one { ... }`) the mode is chosen as the ability goes on the stack (CR 603.3c), each mode with its own targets. `choose two { ... }` (three or four unnamed modes) chooses two different modes (CR 700.2d; "Choose two —"), as a spell (`choose two { ... }` or `spell: choose two { ... }`) or as a triggered ability's effect (`triggered when this enters: choose two { ... }`: "When this creature enters, choose two —"); the engine offers the pairs of modes, in order. A named mode, `mode "Smash the Chest": destroy target artifact`, prints its name first ("• Smash the Chest — Destroy target artifact."); the name has no rules meaning. |
| `enters tapped unless you control planeswalker`, `enters tapped unless player life 13 or less` | "This land enters tapped unless ..." with a static condition (CR 614.1d). |
| `enters tapped if not your turn` | "This creature enters tapped if it's not your turn." (the same replacement as `enters tapped unless your turn`, CR 614.1d) |
| `enters tapped unless pay 3 life` | "As this land enters, you may pay 3 life. If you don't, it enters tapped." (CR 614.12; only with that much life, CR 119.4). |
| `can't attack unless defending player controls Island`, `can't block unless you control other Goblin`, `can't attack or block unless ...` | Attack and block restrictions with a condition (CR 508.1c, 509.1c). Conditions only these use: `defending player poisoned` ("defending player is poisoned"), `on battlefield Mountain` / `on battlefield 5 Island` ("there is a Mountain on the battlefield", "there are five or more Islands on the battlefield"), `you control more creatures than defending player` (or `attacking player`), and, judged on the whole declaration, `at least 2 other creatures attack` (or `block`) and `black or green creature also attacks` ("a black or green creature also attacks"): a creature whose condition the declared attackers (blockers) don't meet can't be among them. `opponent damaged this turn` prints "an opponent has been dealt damage this turn" here. `can't attack if defending player controls untapped land` (`if`: the restriction applies while the condition holds: "This creature can't attack if ..."); `can't attack if creature also attacks` prints "can only attack alone" (granted: "Enchanted creature ... can only attack alone."). `attacked last turn` ("it attacked during your last turn": the source was declared as an attacker during its controller's most recent turn) also works in `static this: has doesn't untap if attacked last turn`. `defending player controls no Glimmer creature` prints "controls no Glimmer creatures". A condition's alternative written with its own article (`you control Knight or a Soldier`) prints it ("a Knight or a Soldier"). |
| `activated <cost>: <effects>` | An activated ability (CR 602). With {X} in its cost (`activated {X}, sacrifice this: destroy target artifact with mana value X`), X is announced as it is activated, before targets (CR 602.2b, 601.2b), only as high as can be paid and some target fits; the effects read it. A mana ability's X is not asked. |
| `boast {1}{W}: <effects>`, `exhaust {3}: <effects>`, `power-up {5}{R}: <effects>` | An activated ability after a keyword, printed "Boast — {1}{W}: ..." (the keyword's rules are reminder text). Boast (CR 702.142): only if this creature was declared as an attacker this turn, and only once each turn. Exhaust (CR 702.177) and power-up (CR 702.193): only once for the object (a new object after a zone change may again, CR 400.7); a power-up ability of a permanent that entered this turn costs its mana cost less (colored mana reduces that color, any excess generic; CR 118.7). |
| `loyalty +1: <effects>` | A loyalty ability (CR 606); `-6` likewise, and `loyalty 0:` (or `+0`) prints "0:". |
| `equip {1}`, `equip Knight {1}`, `equip {0}, once`, `equip {2}, pay 2 life`, `equip sacrifice creature`, `equip {3} or discard 1 card` | Equip (CR 702.6): "Equip {1}"; with a creature type, only onto a creature of that type you control ("Equip Knight {1}", CR 702.6c); `once` prints "Equip {0}. Activate only once each turn." Any activation cost other than mana alone prints after a dash: "Equip—{2}, Pay 2 life.", "Equip—Sacrifice a creature." (`, once` adds "Activate only once each turn."). `equip {3} or discard 1 card` prints "Equip—Pay {3} or discard a card.": two equip abilities, one for each cost (`activate "X" ability 2` in scenarios takes the second). |
| `triggered <trigger> [if <condition>]: <effects>` | A triggered ability (CR 603). `triggered once each turn <trigger>: ...` ("This ability triggers only once each turn."), `twice each turn`, and `do once each turn` ("Do this only once each turn.") limit it each turn (below). |
| `replace <event>: <outcome>` | A replacement or prevention effect while on the battlefield (CR 614, 615). Add `{ <effects> }` for effects that happen when it applies (Phantom Tiger). |
| `enters tapped` / `enters with 2 +1/+1 counters` / `enters prepared` | How this permanent enters (CR 614.1c; prepared, CR 722.3a). `enters with 2 +1/+1 counters if creature died this turn` (any static condition, judged as it enters) prints "This creature enters with two +1/+1 counters on it if a creature died this turn."; `enters with count equal graveyard creature +1/+1 counters` prints "... enters with a number of +1/+1 counters on it equal to the number of creature cards in your graveyard" (counted as it enters). Both, in that order, print as one sentence: "This land enters tapped with two depletion counters on it." (the order they apply in changes nothing, so it isn't asked). |
| `static <filter>: <modifications>` | A static ability generating a continuous effect (CR 604, 611.3). `static each creature you control with +1/+1 counter: has trample` prints the subject one at a time: "Each creature you control with a +1/+1 counter on it has trample." |
| `static as long as <condition>, <filter>: <modifications>` | A conditional static ability (CR 611.3a), printed "As long as <condition>, ...". Write the condition last (`static this: +2/+2 as long as metalcraft`) to print it last; `static this: has can't be blocked if cast historic spell this turn` prints "if" ("This creature can't be blocked if ..."). `static during your turn, this: has first strike` prints "During your turn, ..."; `static during turns other than yours, this: +0/+2` prints "During turns other than yours, ..." (it isn't the controller's turn). |
| `label "Landfall" <ability line>` | An ability word (CR 207.2c) printed before the ability: "Landfall — Whenever a land you control enters, ...". It has no rules meaning. |
| `can't be blocked by Wall`, `can't be blocked except by creature with flying`, `can't be blocked by more than one creature`, `can block only creature with flying`, `can't block creature with flying` | Blocking restrictions on this creature (CR 509.1b–c). Alternatives print with "and/or" ("can't be blocked except by artifact creatures and/or white creatures", `black or red creature`: "black and/or red creatures"). Granted, they print unquoted: `static enchanted: has ability { can't be blocked except by Wall }` is "Enchanted creature can't be blocked except by Walls."; `this gains ability { can't be blocked by creature with power 2 or less }` is "This creature can't be blocked by creatures with power 2 or less this turn." ("can't be blocked this turn except by ..."). `can't be blocked by creature with lesser power` prints "Creatures with power less than this creature's power can't block it." Alternatives in `can't be blocked by` print with "or" (`can't be blocked by creature with aura or enchantment creature`: "can't be blocked by enchanted creatures or enchantment creatures"). `can't be blocked except by 3 or more creatures` (CR 509.1b: blocked by none or by at least that many, like menace's two). Allowances (CR 509.1a): `can block shadow as though had shadow` ("can block creatures with shadow as though it had shadow"; `as though didn't have shadow`: "... as though they didn't have shadow", CR 702.28b), `can block landwalk as though didn't have` ("can block creatures with landwalk abilities as though they didn't have those abilities", CR 702.14b), `can block an additional 7 creatures each combat` (eight in all). |
| `enters as copy of creature` | "You may have ~ enter as a copy of ..." (CR 707). |
| `as this enters choose color`, `as this enters choose color except white`, `as this enters choose creature type`, `as this enters choose land type`, `as it enters choose color` | "As this land enters, choose a color [other than white]." (CR 614.12; `land type` is "a basic land type"): the controller chooses as the permanent enters, however it enters; the permanent remembers it until it leaves (CR 400.7) for `mana chosen`, `set color chosen`, `has protection from chosen color`, `set land chosen`, `add chosen type`, and filters `of chosen color` / `of chosen type`. `as it enters` prints on the line before ("This land enters tapped. As it enters, choose a color."). |
| `spells artifact you cast cost {1} less`, `spells noncreature cost {1} more`, `spells Dragon your opponents cast cost {2} more` | "Artifact spells you cast cost {1} less to cast." (CR 601.2f): the generic part only, never below zero. Types joined with `or` print "Instant and sorcery spells". `static during turns other than yours, spells you cast cost {1} less` (or `during your turn`) applies only then: "During turns other than yours, spells you cast cost {1} less to cast." A mana value prints after who casts them: `spells creature with mana value 6 or greater you cast cost {2} less` is "Creature spells you cast with mana value 6 or greater cost {2} less to cast." `static as long as spell number 2 this turn, spells you cast cost {1} less` is "The second spell you cast each turn costs {1} less to cast." (the spell would be your second this turn: you've cast exactly one before it). A keyword quality prints after "spells": `spells creature with flying you cast cost {1} less` is "Creature spells with flying you cast cost {1} less to cast." |
| `spells you cast have delve`, `spells instant or sorcery you cast have storm`, `spells instant or sorcery you cast from hand have cascade`, `spells you cast have affinity for artifact`, `spells creature you cast have convoke`, `spells noncreature you cast have improvise` | "Spells you cast have delve.", "Instant and sorcery spells you cast have storm.", "... you cast from your hand have cascade.", "Spells you cast have affinity for artifacts.": the controller's matching spells have the keyword while they are cast (convoke, improvise, delve, cascade, storm; affinity for <filter> is {1} less for each such permanent the caster controls). Several join with `and`. |
| `this spell costs {1} less count each artifact you control` | "This spell costs {1} less to cast for each artifact you control." |
| `affinity for artifact` | "Affinity for artifacts" (CR 702.41). |
| `protection from artifact`, `protection from Goblin`, `protection from multicolored`, `protection from everything`, `protection from creature your opponents control` | Protection from a quality other than a color (CR 702.16); colors are keywords. A controller in the quality is judged from the protected object's controller ("protection from creatures your opponents control": its own creatures still may block and target it). Consecutive protections print as one line, "Protection from Spirits and from Arcane" (the gate splits such a line into one protection each, CR 702.16g); in a list of granted keywords, "has protection from green and from blue". |
| `fabricate 1`, `renown 1`, `evolve`, `unleash` | Fabricate (CR 702.123): when it enters, you may put N +1/+1 counters on it; if you don't (or it has left the battlefield), you create N "1/1 colorless Servo artifact creature" tokens. Renown (CR 702.112): when it deals combat damage to a player, if it isn't renowned, N +1/+1 counters and it becomes renowned (a designation that lasts until it leaves the battlefield). Evolve (CR 702.100): whenever a creature you control enters, if that creature has greater power or greater toughness (judged as it triggers and as it resolves, the entering creature by last known information if it has left), a +1/+1 counter on this. Unleash (CR 702.98): you may have it enter with a +1/+1 counter; it can't block as long as it has a +1/+1 counter. |
| `storied` | Storied (CR 702.195): any time its controller controls three or more permanents that are artifacts, Sagas, and/or legendary, they get an enduring story for the rest of the game (checked with the state-based actions). Prints `Storied`. The static conditions `enduring story` ("you have an enduring story") and `no enduring story` ("you don't have an enduring story"; `static this: has doesn't untap if no enduring story` prints "... doesn't untap during your untap step unless you have an enduring story."). |
| `increment` | Increment (CR 702.191): whenever you cast a spell, if this is a creature and the amount of mana spent to cast that spell (X included) is greater than its power or its toughness, a +1/+1 counter on it; judged as it triggers and as it resolves. Prints `Increment`. |
| `afterlife 1`, `extort`, `mentor` | Afterlife (CR 702.135): when it dies, N "1/1 white and black Spirit creature with flying" tokens. Extort (CR 702.101): whenever you cast a spell, you may pay {W/B}; if you do, each opponent loses 1 life and you gain the life lost this way. Mentor (CR 702.134): whenever it attacks, a +1/+1 counter on target attacking creature `with lesser power` (a filter word: power less than the source's, as it is or last existed); `has mentor` grants it. |
| `mobilize 2`, `mobilize count where graveyard creature` | Mobilize (CR 702.181): whenever it attacks, N "1/1 red Warrior creature" tokens enter tapped and attacking what it attacks (never declared as attackers, CR 508.4), sacrificed at the beginning of the next end step. Prints "Mobilize 2", or "Mobilize X, where X is the number of creature cards in your graveyard." with a count. |
| `riot`, `living weapon`, `job select`, `activated {5}{R}: monstrosity 3`, `static as long as monstrous, this: has flying` | Riot (CR 702.136): as it enters, its controller may have it enter with an additional +1/+1 counter; if they don't, it has haste for as long as it is that object (scenarios: `accept`/`decline`). Living weapon (CR 702.92): when this Equipment enters, a "0/0 black Phyrexian Germ creature" token, then attach this to it. Job select (CR 702.182) likewise, with a "1/1 colorless Hero creature" token. Monstrosity N (CR 701.37), an effect, `X` allowed: if the source isn't monstrous, N +1/+1 counters on it and it becomes monstrous (a designation kept until it leaves the battlefield); `monstrous` is the condition "this creature is monstrous". |
| `ingest`, `reinforce 1 {1}{R}` | Ingest (CR 702.115): whenever it deals combat damage to a player, that player exiles the top card of their library. Reinforce N—cost (CR 702.77): "{cost}, Discard this card: Put N +1/+1 counters on target creature." (activated from the hand). |
| `retrace`, `jump start`, `surge {1}{R}`, `spectacle {R}` | Retrace (CR 702.81): the card may be cast from your graveyard for its mana cost and a land card discarded (scenarios: `from graveyard discarding "Forest"`); it isn't exiled. Jump-start (CR 702.133): the same with any card discarded, exiled as it leaves the stack (flashback's casting, for the mana cost and a discard). Surge (CR 702.117) and spectacle (CR 702.137): the card's alternative cost (`alternative` in scenarios), offered if you've cast another spell this turn, or if an opponent lost life this turn. Printed "Retrace", "Jump-start", "Surge {1}{R}", "Spectacle {R}" where written. `freerunning {1}{B}` (CR 702.173) likewise, offered once a creature that was an Assassin you controlled dealt combat damage to a player this turn (the game has no commanders); prints "Freerunning {1}{B}". |
| `mayhem {1}{R}` | Mayhem: a card its owner discarded this turn may be cast from their graveyard for this cost (scenarios: `from graveyard`), timing rules still applying; the discarded card is remembered as the graveyard object it became, until the turn ends. Prints `Mayhem {1}{R}` in place. The land form ("You may play this card from your graveyard ...") is not supported. |
| `poisonous 1`, `frenzy 1`, `absorb 1`, `provoke` | Poisonous (CR 702.70): combat damage to a player gives them N poison counters. Frenzy (CR 702.68): attacks and isn't blocked, +N/+0 until end of turn. Absorb (CR 702.64): prevent N of each damage dealt to it (a static `prevent next N` prevents N of every event, not a shield). Provoke (CR 702.39): attacks, you may have target creature defending player controls untap and block it if able. All four, and `outlast {2}`, may be granted bare: `static all Sliver creature: has poisonous 1` ("All Sliver creatures have poisonous 1."). |
| `modular 2`, `cumulative upkeep {1}`, `exert: this gets +2/+2` | Modular (CR 702.43): enters with N +1/+1 counters; as it dies, they may move to target artifact creature. Cumulative upkeep (CR 702.24): each upkeep an age counter, then pay the cost per counter or sacrifice it. Exert (CR 701.43): as it attacks, its controller may exert it (it doesn't untap during their next untap step) for the effects. The keyword `delve` (CR 702.66) lets graveyard cards pay {1} each. |
| `graft 2`, `vanishing 3` | Graft (CR 702.58): enters with N +1/+1 counters; whenever another creature enters, if it has a +1/+1 counter, its controller may move one onto that creature. Vanishing (CR 702.63): enters with N time counters; at the beginning of its controller's upkeep, if it has a time counter, one is removed, and as the last is removed this way it is sacrificed (part of the same trigger here). `vanishing` without a number (CR 702.63b) has no entering counters. Print `Graft 2`, `Vanishing 3`, `Vanishing`. |
| `sunburst` | Sunburst (CR 702.44): enters with a +1/+1 counter (a creature card) or a charge counter (any other card) for each color of mana spent to cast it (`colors spent`); the printed card decides which, ignoring type-changing effects. Prints `Sunburst`. |
| `soulbond`, `static paired: has flying` | Soulbond (CR 702.95): as this creature enters, its controller may pair it with another unpaired creature they control; as another creature they control enters, they may pair it with this one if both are unpaired. A pair breaks when either leaves, stops being a creature, or changes controller (702.95e). The filter `paired` (`paired named`) is this creature and the one it's paired with, while paired: "As long as this creature (~) is paired with another creature, both creatures have flying." (keywords) or "... each of those creatures gets +1/+1." Prints `Soulbond`. |
| `daybound`, `nightbound` | Daybound (CR 702.145b), on a transforming card's front face, and nightbound (CR 702.145e), on its back face: with either on the battlefield it becomes day (daybound) or night (nightbound) if it's neither; at night a daybound permanent transforms, and by day a nightbound one; at night a daybound card enters transformed; no other effect transforms them. Print `Daybound`, `Nightbound`. |
| `disturb {1}{U}`, `exiled instead of graveyard` | Disturb (CR 702.146), on a transforming card's front face: the card may be cast transformed from its owner's graveyard for this cost, the spell being the back face, which enters as the permanent (CR 702.146b). The back face's "If [name] would be put into a graveyard from anywhere, exile it instead." is `exiled instead of graveyard` (from the stack too). Print `Disturb {1}{U}` and that sentence. |
| `transmute {1}{B}{B}` | Transmute (CR 702.53): an activated ability of the card in hand, "{1}{B}{B}, Discard this card: Search your library for a card with the same mana value as the discarded card (the printed card's), reveal it, put it into your hand, then shuffle. Activate only as a sorcery." Prints `Transmute {1}{B}{B}`. |
| `entwine {2}`, `escape {3}{B} exile 3`, `escapes with 2 +1/+1 counters` | Entwine (CR 702.42): a `choose one` spell may choose every mode for this cost on top. Escape (CR 702.138): cast from the graveyard for this cost, exiling N other cards from it (the oldest); a permanent cast this way keeps that, for `escapes with`. The keyword `exploit` (CR 702.110) sacrifices a creature as it enters, if its controller wants, and `triggered when this exploits a creature:` abilities follow. `discover 4` (CR 701.57) is an effect. |
| `enters with 1 +1/+1 counter if attacked this turn`, `if spent 3 white, enters with 1 +1/+1 counter` | "This creature enters with a +1/+1 counter on it if you attacked this turn." (raid; any static condition: `opponent lost life this turn`, `permanent left this turn` (revolt: "a permanent left the battlefield under your control this turn"), `you control creature with power 4 or greater` (ferocious), `you control modified creature`), and with the condition first, "If at least three white mana was spent to cast this spell, this creature enters with a +1/+1 counter on it." (adamant). Judged as the permanent enters (CR 614.1c). |
| `enters with count each other creature you control +1/+1 counters`, `enters with count each colors spent +1/+1 counters`, `enters with count equal all graveyards creature +1/+1 counters` | "This creature enters with a +1/+1 counter on it for each other creature you control." / "... for each color of mana spent to cast it." (converge) / "This creature enters with a number of +1/+1 counters on it equal to the number of creature cards in all graveyards.": the count judged as it enters. |
| `enters with choice flying counter or first strike counter`, `escapes with choice +1/+1 counter or flying counter` | "This creature enters with your choice of a flying counter or a first strike counter on it." (CR 614.1c, 122.1b): its controller chooses one as it enters; "escapes with ..." only when cast with escape. |
| `kicked with 2 +1/+1 counters`, `kicked with 2 +1/+1 counters and first strike` | "If this creature was kicked, it enters with two +1/+1 counters on it [and with first strike]." (CR 702.33c): the counters as it enters, and the keywords for as long as it stays, only if it was cast with its kicker paid. |
| `madness {1}{R}`, `foretell {1}{U}`, `suspend 4 {1}{U}` | Madness (CR 702.35): discarded into exile, then may be cast for this cost (else to the graveyard). Foretell (CR 702.143): on its owner's turn, exiled from hand for {2}; cast on a later turn for this cost. Suspend N (CR 702.62): exiled from hand for this cost with N time counters; one comes off each upkeep, and as the last does it may be cast free (a creature has haste). The keywords `cascade` and `rebound` need no cost. `keywords cipher` (CR 702.99, printed "Cipher" after the spell's text): as the spell resolves, its controller may exile it encoded on a creature they control (`decline "X"` in scenarios; `choose "Y"` picks the creature); whenever that creature deals combat damage to a player, its controller may cast a copy of the card free. The encoding ends when the creature or the card moves. `keywords paradigm` (CR 702.192, printed "Paradigm" after the spell's text): the spell is exiled as it resolves; the first time a spell its controller controls with that name resolves, at the beginning of each of their precombat main phases for the rest of the game a copy is made in exile, which they may cast free (scenarios: `turn 3 precombat_main A: cast "X" targeting ...`; it is always cast). |
| `evoke {1}{U}{U}`, `bloodthirst 2` | Evoke (CR 702.74): cast for this cost instead, and the permanent is sacrificed as it enters (`if evoked` is the trigger condition). Bloodthirst (CR 702.54): enters with N +1/+1 counters if an opponent was dealt damage this turn (`opponent damaged this turn` is also a static condition). |
| `crew 3` | Crew (CR 702.122): "Tap any number of other untapped creatures you control with total power 3 or more: this Vehicle becomes an artifact creature until end of turn." The engine taps the weakest creature strong enough alone, else the strongest until the power suffices. |
| `morph {2}{U}`, `megamorph {G}`, `disguise {1}{W}` | Cast face down as a nameless colorless 2/2 creature for {3}, turned face up any time its controller has priority for the cost (a special action; CR 702.37, 708); megamorph adds a +1/+1 counter as it turns up, and a disguised permanent has ward {2} while face down (CR 702.168). `triggered when this is turned face up: ...`. `morph pay 5 life`, `morph discard 1 card`, `morph return Bird you control` print "Morph—Pay 5 life." and kin (CR 702.37a: any cost): turning it face up pays the life, or asks the player which card to discard or which permanent to return. `manifest` is an effect: the top card of your library onto the battlefield face down (CR 701.34); a manifested creature card turns face up for its mana cost. `manifest dread` (CR 701.62): look at the top two cards of your library, manifest the one you choose, and put the other into your graveyard. |
| `bushido 2`, `soulshift 3`, `afflict 1`, `toxic 1`, `ward {2}`, `echo {2}{R}`, `unearth {B}`, `buyback {3}`, `outlast {1}{W}` | Keyword abilities with a number or cost (CR 702.45, .46, .130, .164, .21, .30, .84, .27, .107), printed as the keyword ("Bushido 2") in place and compiled to what they stand for: a blocks-or-blocked pump; a dies trigger that may return a Spirit card with that mana value or less; a becomes-blocked life loss; poison counters with combat damage to a player; a trigger countering an opponent's spell or ability that targets it unless they pay; an upkeep sacrifice unless paid, once after it comes under your control; an ability activated from the graveyard as a sorcery that returns it with haste and exiles it at the next end step; an optional additional cost that returns the spell to its owner's hand; and "{cost}, {T}: Put a +1/+1 counter on this creature. Activate only as a sorcery." `flanking` is a plain keyword. `ward pay 3 life` prints "Ward—Pay 3 life." (CR 702.21a: counter it unless that player pays 3 life); `buyback sacrifice land` prints "Buyback—Sacrifice a land." (CR 702.27a: the land is chosen as buyback is decided and sacrificed as the costs are paid). `ward discard 1 card` and `echo discard 1 card` print "Ward—Discard a card." and "Echo—Discard a card.": the payer discards a card of their choice to pay (none in hand, it can't be paid). |
| `firebending 2`, `firebending count where power`, `firebending count where creature you control` | Firebending (CR 702.189): "Whenever this creature attacks, add N {R}. Until end of combat, you don't lose this mana as steps and phases end." The red mana stays in the pool as each combat step ends, through the end of combat step (the player's other mana is taken to be spent first); X is counted as the trigger resolves. Prints "Firebending 2" (in a keyword line with others: "Trample, firebending 4, haste") or "Firebending X, where X is this creature's power." |
| `strive {2}{W}` | "Strive — This spell costs {2}{W} more to cast for each target beyond the first." (CR 207.2c): the cost is added once per target beyond the first as the total cost is determined (CR 601.2f); a variable target group stops offering targets once another could not be paid for. Printed first. |
| `protection doesn't remove this` | "This effect doesn't remove this Aura.", printed after the static ability before it (`static enchanted: has protection from chosen color`): the Aura's state-based check (CR 704.5m) ignores protection. |
| `ninjutsu {1}{U}` | Ninjutsu (CR 702.49): activated from the hand once blockers are declared, returning an unblocked attacker you control to its owner's hand as a cost; the card enters tapped and attacking what that creature attacked. It was never declared as an attacker. |
| `scavenge {5}{G}`, `embalm {W}`, `eternalize {4}{U}{U}` | Abilities activated from the graveyard as a sorcery, exiling the card as a cost (CR 702.97, .128, .129). Scavenge: +1/+1 counters equal to the card's power on target creature. Embalm: a token copy of the card that's a white Zombie with no mana cost; eternalize: a 4/4 black Zombie with no mana cost. The token is a definition of its own, made from the card's block. |
| `encore {4}{B}` | Encore (CR 702.141): activated from the graveyard as a sorcery, exiling the card as a cost; creates a token copy of the card (one: the game has one opponent) that gains haste and attacks this turn if able, sacrificed at the beginning of the next end step. Prints `Encore {4}{B}`. |
| `dash {2}{R}` | Dash (CR 702.109): cast for this cost instead; the permanent has haste and returns to its owner's hand at the beginning of the next end step. |
| `plot {3}{U}`, `triggered when this becomes plotted: ...` | Plot (CR 702.170): during its owner's main phase with the stack empty, a special action exiles the card from the hand for this cost; it is plotted, and from a later turn on its owner may cast it from exile without paying its mana cost, as a sorcery. Prints `Plot {3}{U}`. "When this card becomes plotted, it deals 2 damage to any target." triggers from exile as it is plotted. `enters with 2 +1/+1 counters if no mana spent` prints "... if it wasn't cast or no mana was spent to cast it." |
| `sneak {2}{W}{B}`, `if sneak paid`, `if his sneak paid` | Sneak (CR 702.190): any time its controller could cast an instant during their declare blockers step, the card may be cast from the hand for this cost and an unblocked attacker they control returned to its owner's hand (chosen with the costs), instead of its mana cost, whatever its type; a permanent enters tapped and attacking what that creature attacked. Prints `Sneak {2}{W}{B}`. The static condition `sneak paid` reads "this spell's sneak cost was paid" on an instant or sorcery, "its sneak cost was paid" on a permanent (kept by it); `his sneak paid` and `her sneak paid` print "his"/"her". |
| `harmonize {4}{R}` | Harmonize (CR 702.180): the card may be cast from its owner's graveyard for this cost instead of its mana cost (timing as usual), tapping up to one untapped creature they control, chosen as the cost is announced (CR 702.180b), to reduce the generic mana by that creature's power; the spell is exiled as it leaves the stack. Prints `Harmonize {4}{R}`. |
| `web slinging {W}`, `if web slung`, `if they web slung` | Web-slinging (CR 702.188): the card may be cast for this cost instead of its mana cost (timing as usual), returning a tapped creature its caster controls to its owner's hand (chosen with the costs). Prints `Web-slinging {W}`. The permanent keeps how it was cast: the static condition `web slung` prints "it was cast using web-slinging"; `he web slung`, `she web slung`, `they web slung` print "he was", "she was", "they were". |
| `impending 4 {1}{U}{U}` | Impending (CR 702.176): the card may be cast for this cost instead of its mana cost; the permanent then enters with that many time counters, isn't a creature while it has one (its other card types replace its own, a layer 4 static ability with the condition `impending`: "this permanent's impending cost was paid and it has a time counter on it"), and at the beginning of its controller's end step loses a time counter while that holds. Prints `Impending 4—{1}{U}{U}`; the card then calls itself "this permanent". |
| `warp {1}{U}` | Warp (CR 702.185): cast from the hand for this cost instead; the permanent is exiled at the beginning of the next end step, and its owner may cast it from exile on a later turn (paying its costs as usual). Prints `Warp {1}{U}`. |
| `bestow {3}{W}` | Bestow (CR 702.103): cast for this cost instead as an Aura spell with enchant creature (not a creature, no creature types; the engine derives that Aura from the card). It becomes a creature again when it isn't attached, and resolves as a creature if its target is illegal. Write the creature's own abilities as usual, with `static enchanted: ...` for what the Aura grants. |
| `overload {2}{U}` | Overload (CR 702.96): an instant or sorcery with one target may be cast for this cost instead; it then has no targets and its effects happen once for each object or player the target spec describes ("target" reads "each"), regardless of hexproof or protection. Prints `Overload {2}{U}`. |
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
| `bargain` | Bargain (CR 702.166): as the spell is cast, its controller may sacrifice an artifact, enchantment, or token. Printed first. `bargained` is a static condition: "If this spell was bargained, ..." (`if bargained, ...` in a spell, with `instead` like kicker), "if it was bargained" (`if bargained` after a trigger), "This spell costs {2} less to cast if it's bargained." (`this spell costs {2} less if bargained`; a card in hand counts as bargained when its controller has something to sacrifice). |
| `saddle 2` | Saddle N (CR 702.171): "Tap any number of other creatures you control with total power N or more: This Mount becomes saddled until end of turn. Saddle only as a sorcery." (chosen like crew). `triggered whenever this attacks while saddled: ...` ("Whenever this creature attacks while saddled, ..."); `saddled` is also a static condition ("it's saddled"). |
| `deck any number` | "A deck can have any number of cards named ~." (CR 100.2a): a deck-building rule; nothing in a game. |
| `extra vote`, `extra vote may` | "While voting, you get an additional vote.", "While voting, you may vote an additional time." (CR 701.38d: the controller's extra votes are cast when they would vote; the optional one may be declined). |
| `as turned face up put 4 +1/+1 counters`, `as enters or turned face up put count where other creature you control +1/+1 counters` | "As this creature is turned face up, put four +1/+1 counters on it.", "As this creature enters or is turned face up, put X +1/+1 counters on it, where X is the number of other creatures you control." CR 708.11: the counters are put on while it turns face up (by its cost or by an effect), before "turned face up" triggers look; with `enters or` it also enters with them (CR 614.1c, the count judged as it enters). |
| `may look face-down`, `may look face-down opponents`, `may look top`, `may look top face-down` | "You may look at face-down creatures you don't control any time." ("... your opponents control ...", the same in a two-player game), "You may look at the top card of your library any time.", "You may look at the top card of your library and at face-down creatures you don't control any time." Information only (CR 401.2, 708.5): the controller's view shows those cards. |
| `additional land`, `additional land 2`, `additional land each player`, `additional land as long as you control other Elf` | "You may play an additional land on each of your turns.", "You may play two additional lands on each of your turns.", "Each player may play an additional land on each of their turns.", "As long as you control another Elf, you may play an additional land on each of your turns." (CR 305.2): more land plays on its controller's turns (each player's own turns with `each player`), while the condition holds. The effect `play additional land this turn` ("You may play an additional land this turn.") and `play 2 additional land this turn` ("You may play up to two additional lands this turn.") add plays for the rest of the turn. |
| `you have hexproof`, `you have shroud`, `opponents can't cast spells during your turn`, `players can't gain life`, `damage can't be prevented` | Static abilities about players: "You have hexproof." (CR 702.11c: spells and abilities your opponents control can't target you), "You have shroud." (CR 702.18c: no spell or ability can target you), "Your opponents can't cast spells during your turn.", "Players can't gain life.", "Damage can't be prevented." (CR 614.17). `you can't lose the game` prints "You can't lose the game and your opponents can't win the game.": the controller doesn't lose for the state-based actions of CR 704.5a-c while it applies, and an effect that would make an opponent win does nothing. `as long as <condition>` may follow any of them. |
| `hexproof from black`, `hexproof from monocolored`, `hexproof from instant` | "Hexproof from black" (CR 702.11d): this permanent can't be the target of spells or abilities your opponents control from sources matching the filter (a color, `multicolored`, `monocolored`, a card type: "instants", "planeswalkers"). In a keyword line ("Flying, hexproof from multicolored") write the keywords first, then this line. |
| `trigger again Ninja creature you control`, `trigger again other Elemental you control`, `trigger again permanent you control when permanent enters the battlefield`, `trigger again permanent you control when artifact or creature enters`, `trigger again permanent you control when creature dies`, `trigger again permanent you control when creature attacks` | CR 603.2d: "If a triggered ability of a Ninja creature you control triggers, that ability triggers an additional time." ("it triggers" after `other`, as Twinflame Travelers prints); with `when ... enters`, only when that permanent entering caused it ("If a permanent entering the battlefield causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time.", `the battlefield` printed where written); `when creature dies` ("If a creature dying causes ...", the creature judged as it last existed) and `when creature attacks` ("If a creature attacking causes ...") likewise. Only such abilities on the battlefield as the event happens count (one dying with the creature doesn't). Each such ability adds one more instance; each instance is put on the stack with its own targets and choices |
| `opening hand` | "If this card is in your opening hand, you may begin the game with it on the battlefield." (CR 103.6a; a legendary card names itself: "If Quicksilver is in your opening hand, ..."). After mulligans, starting with the starting player, each player is asked about each such card in their hand. |
| `play from top lands`, `play from top spells creature`, `play from top lands and spells`, `play from top once spells instant or sorcery` | "You may play lands from the top of your library.", "You may cast creature spells from the top of your library.", "You may play lands and cast spells from the top of your library.", "Once each turn, you may cast an instant or sorcery spell from the top of your library." (CR 601.3, 305.1): the controller may play the top card of their library, paying its costs, with the usual timing and a land only with a land play left. The spells' filter is any filter (`Spider or noncreature`: "Spider spells and noncreature spells"; `with mana value 4 or greater`: "spells with mana value 4 or greater"; `with flash or flying`); `once` allows one spell a turn by that permanent's permission. |
| `top revealed`, `top revealed players` | "Play with the top card of your library revealed.", "Players play with the top card of their libraries revealed.": every player knows that top card (CR 401.2). |
| `hand size none`, `hand size +1`, `hand size opponents -1` | "You have no maximum hand size.", "Your maximum hand size is increased by one.", "Each opponent's maximum hand size is reduced by one." (CR 402.2) |
| `except { ... }` | What this card's copy effects add (CR 707.9b): `not legendary` ("it isn't legendary"; "the copy isn't legendary" for a spell copy), `types Artifact` and `subtypes Illusion` ("it's an artifact in addition to its other types", "it's a Vehicle artifact ..." together), `keywords haste`, and ability lines ("it has haste and \"At the beginning of the end step, sacrifice this token.\"": on an instant or sorcery, `this` in them is the token). They are part of the copy's copiable values; the legend rule reads its supertypes. A token copy of a creature that has left the battlefield ("When this creature dies, if it's not a token, create a token that's a copy of it"; `if this is nontoken`; `if this isnt token` prints "if it isn't a token") is made as it last existed. |

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
a matching permanent they control), `discard 1 card`, `discard 1 card
at random` ("Discard a card at random": the card is picked at random as
the cost is paid, CR 701.9b), `discard creature
card` (a filter: discard one matching card, printed "Discard a creature
card"), `discard X cards` (a spell's additional cost: "As an additional
cost to cast this spell, discard X cards."; X is announced as the spell
is cast, at most the other cards in hand, CR 601.2b; `discard X creature
cards` counts only matching cards), `discard this`, `exile this from graveyard` ("Exile this card from
your graveyard"; the ability is activated from the graveyard), `exile
this` ("Exile this artifact": the permanent exiles itself), `return
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
than the source, sparing what the spell or ability targets), `pay 3
life`, `pay x life` (a spell's additional cost: "As an additional cost to
cast this spell, pay X life."; X is announced as the spell is cast, at most
the caster's life, CR 119.4), `exert this` ("Exert this creature", "Exert
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
`reduced {1} each <count>` ("This ability costs {1} less to activate for
each Town you control.") and `reduced {4}{B} if <static condition>` ("...
less to activate if you've drawn three or more cards this turn.") take
mana off the ability's cost as it is activated (CR 601.2f by way of
602.2b), printed after the effect and before the restrictions. An ability whose cost
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
| `when you control no Island`, `when you control no other creature`, `when you control Dwarf` | a state trigger (CR 603.8): "When you control no Islands, sacrifice this creature."; any static condition after `when you control` ("When you control a Dwarf", "When you control a creature with toughness 2 or less"). Checked whenever a player would receive priority; it doesn't trigger again while its ability waits to go on the stack or is on it. A face-down permanent has no abilities, so no state trigger |
| `whenever you gain life` | |
| `whenever you gain or lose life`, `whenever you gain or lose life during your turn` | once per event that changes your life total (CR 119.9: a gain or a loss of any amount is one event), only on your turn with `during your turn` |
| `whenever you scry or surveil` | once per instruction to scry or surveil (CR 701.22, 701.25) |
| `whenever creature you control explores`, `whenever this explores` | as a creature explores (CR 701.44), whatever it revealed and even if it has left the battlefield (then judged as it last existed); it is the triggering object |
| `whenever you tap untapped creature an opponent controls` | "Whenever you tap an untapped creature an opponent controls": a spell or ability you control (an effect, or a cost such as `tap 1 untapped creature ... chosen` in an effect) taps an untapped permanent matching the filter; it is `that creature`. Not when an opponent taps their own creature for your effect |
| `whenever you activate an ability of a creature`, `whenever you activate an exhaust ability`, `whenever an opponent activates a loyalty ability` | as an activated ability is activated (CR 602.2), before it resolves: of a creature (mana abilities included, CR 605.3a; mana the engine taps for automatically while paying a cost is not an activation), an exhaust ability (CR 702.177), a loyalty ability (CR 606). The activating player is `that player` |
| `whenever you discard one or more cards` | once for the cards you discard together (CR 603.2c), counted as `that much` ("put that many +1/+1 counters", "deals that much damage"); cards discarded before the next time a player gets priority count as together. Cycling a card discards it |
| `whenever you sacrifice one or more Food` | once for the matching permanents you sacrifice together (CR 603.2c), counted as `that much` |
| `whenever you attack with one or more Faerie`, `whenever you attack a player with one or more creature with power 4 or greater` | once as you declare attackers if a matching creature attacks (CR 508.1m); with `a player`, once for each player a matching creature attacks, that player being `that player` |
| `whenever this transforms`, `whenever this enters or transforms` | "Whenever this creature transforms into Ashling, Rimebound" (CR 701.27): the ability is on that face, so it sees only the transformation to it; the face's name prints |
| `whenever one or more creature you control deal combat damage to a player`, `whenever one or more Ally you control deal combat damage to a player` | once for each player dealt combat damage by matching creatures in one combat damage step (CR 603.2c; a first-strike step and the regular one trigger separately); that player is `that player` |
| `whenever a source you control deals noncombat damage to an opponent`, `whenever an opponent is dealt noncombat damage`, `whenever one or more opponents are dealt noncombat damage` | damage other than combat damage (CR 120.2) to an opponent, from a source you control (a spell as it resolves, as it last existed) or any source; the last once for the damage dealt to each opponent at once. The opponent is `that player`, `that much` the damage |
| `whenever you commit a crime`, `whenever you commit a crime during your turn` | "Whenever you commit a crime" (CR 700.13): as you finish casting a spell, activating an ability, or putting a triggered ability on the stack that targets an opponent, something an opponent controls, or a card in an opponent's graveyard; once per spell or ability. A copy put on the stack commits none. `during your turn` (Overzealous Muscle) only on your turn |
| `whenever you gain life first time each turn` | "Whenever you gain life for the first time each turn": the gain that starts the life you gained this turn (one of several simultaneous lifelink gains) |
| `whenever you roll dice`, `whenever you win a coin flip` | "Whenever you roll one or more dice" (once per roll, CR 706), "Whenever you win a coin flip" (CR 705.2) |
| `whenever creature you control attacks alone` | attacks alone (CR 506.5) |
| `whenever this attacks for the first time each turn` | the attacking creature hadn't attacked yet this turn (with additional combat phases, CR 500.8) |
| `whenever you attack`, `whenever you attack with 3 or more creatures`, `label "Battalion" triggered whenever this and at least 2 other creatures attack` | once as the controller declares attackers (CR 508.1m), if at least that many attack (battalion: this creature among them); "Whenever this creature and at least two other creatures attack"; `whenever one or more creatures you control attack` is the same trigger in the older wording |
| `whenever this attacks and isn't blocked` | as blockers are declared, for an attacking creature no creature blocks (CR 509.1h) |
| `whenever this becomes blocked`, `whenever this blocks or becomes blocked` | |
| `whenever this blocks a creature with flying`, `whenever this becomes blocked by a creature`, `whenever this blocks or becomes blocked by a nonblack creature` | once for each attacker it blocks (CR 509.3b) or each creature blocking it (CR 509.3d); the other creature, which must match the filter, is "that creature"; `a creature with Equipment` prints "an equipped creature", `a creature that was dealt damage this turn` "... that has been dealt damage this turn". `... by one or more blue or black creature` ("by one or more blue and/or black creatures", CR 509.3c, 509.3e) triggers once, on the first matching pair, and "it" is the blocking or blocked creature itself (`whenever equipped creature becomes blocked by one or more colorless creature: it gains deathtouch until end of turn`) |
| `at end of combat` | as the end of combat step begins (CR 511.2); also a delayed trigger: `delayed that creature: destroy that creature at end of combat` |
| `whenever you cast a spell`, `... a noncreature spell`, `... an instant or sorcery spell`, `whenever a player casts a spell` | `whenever you cast noncreature or Dragon spell` prints "a noncreature or Dragon spell"; with the alternative's own article, `noncreature or a Dragon spell`, "a noncreature spell or a Dragon spell" (the same spells) |
| `label "Magecraft" triggered whenever you cast or copy instant or sorcery spell: ...` | "Magecraft — Whenever you cast or copy an instant or sorcery spell, ..." (a copy triggers it too, CR 707.10) |
| `label "Heroic" triggered whenever you cast a spell that targets this: ...` | "Heroic — Whenever you cast a spell that targets this creature, ..." (any spell filter: `whenever you cast red spell that targets this`): the spell's targets as it is cast (CR 601.2c), so copies and abilities don't trigger it. The spell is the triggering object, but `it` in the effects is this creature, as Oracle has it ("put a +1/+1 counter on it"). |
| `label "Repartee" triggered whenever you cast instant or sorcery spell that targets creature: ...` | "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, ...": any creature among the spell's targets as it is cast (CR 601.2c). |
| `label "Valiant" triggered whenever this becomes targeted by you first time each turn: ...` | "Valiant — Whenever this creature becomes the target of a spell or ability you control for the first time each turn, ...": once a turn, the first time its controller's spell or ability targets it. |
| `when this becomes targeted` | becomes the target of a spell or ability |
| `whenever this becomes targeted` | "Whenever this creature becomes the target of a spell or ability" (`when` prints "When") |
| `whenever this becomes tapped`, `whenever this becomes untapped` | |
| `whenever one or more +1/+1 counters put on this` | "Whenever one or more +1/+1 counters are put on this creature" (CR 122.1): once per instruction putting counters of that kind on it (adapt, a spell), however many |
| `whenever you put one or more +1/+1 counters on creature you control`, `whenever you put one or more -1/-1 counters on this`, `whenever you put one or more counters on Goblin, Orc, or Army you control` | "Whenever you put one or more +1/+1 counters on a creature you control" (CR 122.6): counters a spell or ability you control puts (or that a permanent you control enters with, or a cost you pay puts), once for each permanent they go on; without a kind, counters of any kind. `that much` / `that many` is how many were put |
| `whenever Aura becomes attached to this` | "Whenever an Aura becomes attached to this creature" (CR 701.3a; an Aura entering attached too); the object attached is the triggering object |
| `whenever enchanted land is tapped for mana: add {G}` | "Whenever enchanted land is tapped for mana, its controller adds an additional {G}." (`enchanted Forest` for an Aura with `enchant Forest`). A triggered mana ability (CR 605.1b): its effects may only add mana (`add any`, `add {G}{G}`, `add 2 mana any combination`, `add chosen`, `add {G} count each Elf on battlefield`), for the tapped permanent's controller, and it resolves at once when the permanent is tapped for mana by a mana ability (CR 605.4a), whether paying a cost or activated by hand. Payment counts the extra mana as part of what the permanent makes. `whenever land you control is tapped for mana: add {G}` prints "Whenever you tap a land for mana, add {G}." |
| `whenever this is dealt damage` | (enrage) |
| `whenever this deals damage`, `... deals damage to creature`, `... to player`, `... to opponent`, `... deals combat damage to creature` | the damaged creature is "that creature", the damaged player "that player"; `whenever this deals damage to Vampire`, `... to Goblin or Orc` (any filter): the damaged permanent, if it matches, is "that creature". On an Aura, the creature dealt damage by, or blocking or blocked by, the enchanted creature prints "the other creature" |
| `whenever you cast artifact spell`, `whenever a player casts Goblin spell`, `whenever an opponent casts multicolored spell` | a cast trigger with any filter |
| `whenever this becomes targeted by opponent` | "... becomes the target of a spell or ability an opponent controls" (ward's trigger) |
| `whenever you cast with mana value 4 or greater spell` | "Whenever you cast a spell with mana value 4 or greater" (a filter's "with" qualities print after "spell"; likewise `counter target with mana value 4 or less spell`) |
| `whenever you cast your first spell each turn`, `... your second spell each turn`, `... a spell other than your first spell each turn`, `... your first spell during each opponent's turn`, `whenever an opponent casts their second spell each turn`, `whenever you cast a spell during an opponent's turn`, `whenever an opponent casts their first noncreature spell each turn`, `whenever you cast a spell from exile`, `whenever you cast your third spell each turn` | the spell's place among those its caster cast this turn (or only that it was cast on another player's turn; the first noncreature one among their noncreature spells; or that it was cast from exile) |
| `when you cycle this` | "When you cycle this card" (CR 702.29d): triggers from the card itself as its cycling ability is activated, so it resolves before the draw; the card is "it" ("it deals 6 damage to any target") |
| `whenever you cycle a card`, `whenever you cycle another card`, `whenever you cycle or discard a card` | cycling a card (CR 702.29); `or discard` triggers once per card discarded, cycling included |
| `whenever this or Ally you control enters`, `whenever this or creature you control with power 4 or greater dies` | "Whenever this creature or another Ally you control enters"; the filter includes the source, and Oracle names the source again ("this creature") |
| `whenever one or more artifact you control enter` | once for all the matching objects entering together (CR 603.2c), counted as `that much`; objects entering before the next time a player gets priority count as together |
| `whenever one or more other creature you control die`, `whenever one or more permanent you control leave the battlefield` | the same batching for objects leaving the battlefield (dying), judged as they last existed (CR 603.10a) |
| `when you cast this`, `when you cast or cycle this` | "When you cast this spell" (CR 603.2): triggers from the spell itself as it is cast, controlled by its caster, so it resolves before the spell and even if the spell is countered; "When you cast or cycle Warped Tusker" (the card's name) adds the cycle trigger. `when you cast this and when this dies` is one ability with both |
| `whenever you sacrifice other Eldrazi` | "Whenever you sacrifice another Eldrazi" (CR 701.21a): the sacrificed permanent, matching as it is sacrificed, is the triggering object |
| `when this enters and whenever you cast ...`, `when you cycle this and when this dies` | one ability with two trigger events, joined with "and" |
| `when this enters or dies`, `whenever this enters or attacks`, `when this enters or leaves the battlefield`, `when this enters or is turned face up` | one ability with two events of the source, the subject printed once: "When this creature enters or dies" ("When this artifact enters or is put into a graveyard from the battlefield" on a noncreature), "Whenever this creature enters or attacks"; the word written (`when` / `whenever`) is the word printed ("Whenever this creature enters or dies"); a filter's two events likewise: `whenever Detective you control enters or is turned face up` ("Whenever a Detective you control enters or is turned face up", each Detective the triggering object). A face-down permanent has no abilities, so one cast face down triggers only as it turns face up (CR 708.8) |
| `whenever you sacrifice other permanent`, `whenever you sacrifice token`, `whenever a player sacrifices creature`, `whenever an opponent sacrifices ...`, `when this enters and when you sacrifice this` | a permanent is sacrificed (CR 701.21) by its controller, the player named: "Whenever you sacrifice another permanent", "Whenever a player sacrifices a creature", "When this artifact enters and when you sacrifice it". It triggers as the permanent is sacrificed, judged while it is still on the battlefield (so a permanent sacrificing itself sees it), whether by an effect, a cost, or a mana ability (a Treasure); a creature that dies otherwise doesn't. The sacrificed permanent is the triggering object |
| `whenever permanent is turned face up`, `whenever this or permanent is turned face up`, `whenever Detective you control is turned face up` | "Whenever a permanent is turned face up", "Whenever this creature or another permanent is turned face up": the permanent judged as it is face up (CR 708.8) |
| `whenever you draw a card`, `whenever a player draws a card`, `whenever an opponent draws a card` | once per card drawn (CR 121.2); the card is the triggering object |
| `whenever a player discards a card`, `whenever you discard a card`, `whenever you discard creature card` | once per card discarded (CR 701.9), judged as it is discarded; the card is the triggering object |
| `whenever one or more cards leave your graveyard`, `whenever one or more creature cards leave your graveyard` | once for the matching cards that leave the controller's graveyard together, however they go (returned, exiled, cast with flashback; CR 603.2c); their number is `that much` |
| `whenever land card is put into your graveyard from anywhere` | "Whenever a land card is put into your graveyard from anywhere": a card the controller owns, matching as it is in the graveyard (milled, discarded, dying, ...) |
| `whenever creature enters from your graveyard` | "Whenever a creature enters from your graveyard": a permanent the controller owns entering from their graveyard is "it" |
| `whenever you expend 4` | "Whenever you expend 4" (CR 700.14): as the controller spends their fourth total mana to cast spells this turn, judged as the spell that spent it becomes cast (the mana counted as its cost's mana symbols) |
| `whenever andor other Villain or artifact you control enters` | "Whenever another Villain and/or artifact you control enters": `andor` before a filter with an alternative prints "and/or" in a trigger's subject, and between plural alternatives anywhere (`up to 2 target andor Mount or Vehicle`: "up to two target Mounts and/or Vehicles"; wording only); `andlist` likewise joins a singular list with "and" (`each andlist Pest or Bat or Spider you control`: "each Pest, Bat, and Spider you control"), and plural types (`andlist artifact or creature you control`: "artifacts and creatures you control") |

A trigger's subject prints its controller before its "with" qualities
("Whenever a creature you control with power 5 or greater enters").
`you may this has base power and toughness 4/2 until end of turn` prints
"you may have this creature's base power and toughness become 4/2 until
end of turn".
`you may deal 1 damage to any target` prints "you may have this creature
deal ..." and `you may each Ally creature you control gets +1/+0` "you may
have Ally creatures you control get +1/+0 ...".

Conditions (intervening "if", CR 603.4): `if this is untapped`, `if
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
"... enters while this creature has a -1/-1 counter on it"), and `during
your turn` is one too: "Whenever a land enters during your turn".

Static conditions: `7 cards in your graveyard` ("there are seven or more
cards in your graveyard"), `4 card types in your graveyard`, `you control 3
artifact` ("you control three or more artifacts"), `you control Island`
("you control an Island"), `you control 1 or more Egg` ("you control one
or more Eggs", the same condition), `your turn` ("it's your turn"), `not your turn` ("it's not your turn"),
`cast during main phase` ("you cast this spell during your main phase")
and `controlled Mount as cast` ("you controlled a Mount as you cast this
spell"), both judged as the spell is cast and remembered by it, `attacking`,
`enchanted`, `equipped` ("it's attacking", or "this creature is
attacking" when the subject is something else), `opponent life 10 or
less` ("an opponent has 10 or less life"), `your life 30 or more`,
`player life 13 or less` ("a player has 13 or less life"), `defending
player controls Island`, `you control no untapped land`, `you control
at most 2 other land` ("you control two or fewer other lands": the fast
lands), `you control Mountain or a Plains` ("you control a Mountain or a
Plains"), `you control 2 basic land`, `any player controls black
permanent` ("any player controls a black permanent"), `you control
other Goblin` ("you control another Goblin"), `you control Urza's Mine and
Urza's Power-Plant` ("you control an Urza's Mine and an Urza's
Power-Plant": one of each), `attacked this turn` (raid),
`creature died this turn` (morbid), `hand 0 or less` ("you have no cards
in hand"), `hand 1 or less` ("you have one or fewer cards in hand"),
`hand has no land card` ("you have no land cards in hand"), and
the shorthands `threshold`, `delirium`, `metalcraft`; also `land entered
this turn` (landfall: "you had a land enter the battlefield under your
control this turn") and `drew 2 cards this turn` ("you've drawn two or
more cards this turn").
in hand"), `hand 1 or less` ("you have one or fewer cards in hand"),
`2 instant or sorcery cards in your graveyard` ("there are two or more
instant and/or sorcery cards in your graveyard"; any card filter, `4 permanent
cards in your graveyard` for descend), `you
have 4 creature cards in your graveyard`, `there is land card in your
graveyard` ("there is a land card in your graveyard"; `there's Lesson card
in your graveyard` prints "there's a"), `1 artifact card is in your
graveyard` ("an artifact card is in your graveyard"), `you control Desert
or there is Desert card in your graveyard` (either holds), `5 mana values in your
graveyard`, `opponent graveyard 8 or more` ("an opponent has eight or more
cards in their graveyard"), `opponent controls Human`, `this is untapped`,
`this is Wall` ("it's untapped", "it's a Wall"), `modified` ("it's
modified", CR 700.9), `attacking alone`, `blocked`, `being blocked` ("at
least one creature is blocking this creature"), `attached is legendary`
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
graveyard` ("this spell was cast from a graveyard"; CR 601.2a), `gained
life this turn`, `gained or lost life this turn`, `descended this turn`
("you descended this turn", CR 700.11: a permanent card was put into your
graveyard from anywhere), `surveilled this turn` ("you've surveilled this
turn"), `counter put this turn` ("a +1/+1 counter was put on a permanent
under your control this turn"), `opponent dealt noncombat damage this turn` ("an opponent has been dealt
noncombat damage this turn"; `opponent was dealt ...`: "an opponent was
dealt ..."), `resolution 2` ("this is the second time
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
from the battlefield this turn"; any card types joined by `or`),
`discarded this turn`, `more cards in hand than each
opponent`, `opponent poisoned` ("an opponent is poisoned"), `opponent poison
3 or more`, `you own card in exile`, `total power 8 or more` ("creatures you
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
its spell), `put any counter this turn` ("you put a counter on a creature
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
..."), `this has no arrowhead counters` ("there are no arrowhead counters
on this artifact"), `there are 3 or more brick counters on this` ("there
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
`net`, `flood`, `glyph`, `crystal`, `unlock`, `loot`, `supply`, `divinity`, `finality` (CR
122.1h: a permanent with one that would be put into a graveyard from the
battlefield is exiled instead, so it doesn't die; `return target creature
card from your graveyard to battlefield with 1 finality counter` prints
"... with a finality counter on it"; `with 2 additional +1/+1 counters`
prints "with two additional +1/+1 counters on it", the same counters), and the keyword counters `flying`, `first strike`, `trample`,
`lifelink`, `deathtouch`, `reach`, `menace`, `vigilance`, `hexproof`,
`indestructible`, `double strike` (CR 122.1b: the permanent has the
keyword; `double_strike` in scenarios). A cost may
`remove 1 charge counter from this`, `remove 1 counter from this` ("Remove
a counter from this creature": any kind; the engine removes -1/-1 counters
first, then kinds that don't change power or toughness, then the rest), or
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
`with`; `indefinitely`: no duration, Opal Caryatid), `until end of turn,
target creature becomes Dragon base 4/4 and target 1 gains flying`;
a trigger condition `if this is enchantment` prints "if this permanent is
an enchantment" and the source is "it" after it, `target creature becomes blue`, `this becomes colorless`, `that creature becomes green indefinitely` (no duration: "That creature becomes green."),
`target land becomes Island` (all until end of turn), `this becomes all
colors` (CR 105.2). Colors before a creature type make an animation:
`until end of turn, target creature becomes blue Serpent base 5/5` ("...
becomes a blue Serpent with base power and toughness 5/5"; the color and
creature type replace its others, layers 4, 5, and 7b), `loses all
abilities and becomes green Snake base 1/1` ("loses all abilities and
becomes a green Snake ...", layer 6 too), `target creature loses all
abilities and has base power and toughness 0/1` (Humble: one effect,
layers 6 and 7b), and `in addition` adds instead of
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
time, then `done`), all until end of turn. A choice may also be its own
sentence, used by what follows in the same resolution (CR 608.2d):
`choose creature type` ("Choose a creature type."), `choose color`,
`choose land type` ("Choose a basic land type."), `choose creature type
except Wall`; then `each land you control becomes that type` ("Each land
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
creature". More conditions: `night` ("it's night", CR 731), `gained 3
life this turn` ("you've gained 3 or more life this turn", in all),
`committed crime this turn` ("you've committed a crime this turn", CR
700.13: a spell or ability you control targeted an opponent, a permanent
or spell an opponent controls, or a card in an opponent's graveyard),
`card left your graveyard this turn` ("one or more cards left your
graveyard this turn"), `sacrificed artifact this turn` ("you've
sacrificed an artifact this turn"), and `creature card to your graveyard this turn` ("a
creature card was put into your graveyard from anywhere this turn"). A
count `differently named land you control`: "the number of differently
named lands you control" (`this spell costs {1} less count where
differently named land you control`).
`this spell can't be countered`: "This spell can't be countered." (CR
101.2: a spell or ability that would counter it, ward included, does
nothing to it). Triggers `when
this leaves the battlefield`, `whenever this attacks or blocks`.
In a triggered ability, `it` is the triggering object printed "it"
("Whenever another creature you control attacks, it gets +0/+1 until end
of turn."); `that creature` prints "that creature".
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
Trigger `whenever you draw your second card each turn` (or `an opponent
draws their second card each turn`); `card in a turn` prints "When you
draw your third card in a turn".
Steps: `at beginning of your combat` ("At the beginning of combat on your
turn"), `your first main`, `your second main` ("... your second main
phase"). Recipient `controller of that target 1` ("that artifact's
controller"; `controller of the target 1` is "the creature's controller"); `controller of enchanted` ("enchanted creature's
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
player who had too few cards to discard. `target player can't play lands
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
triggering object. In a trigger with no triggering object (`at beginning
of ...`, `whenever you gain life`, `whenever you attack`) `it` is an
error: write `this` or `target N`. In any ability, `it` after `create 1
"..."` (or a token copy), before another target, is that token ("Create a
0/0 ... token. Put X +1/+1 counters on it."), as `that token` is; and
`that player` after a target player is that player, in a triggered
ability too ("Target player mills three cards. ... that player mills ten
cards instead"). In a triggered ability `toughness of
it`, `power of it`, `mana value of it` ("its toughness") and `power of
that creature` ("that creature's power") otherwise read the triggering
object, as it last existed once it has left (CR 608.2h): `triggered when enchanted creature dies: its
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
attacker's opponent).
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
permanent" whatever the target is (Kellan, Inquisitive Prodigy). `then if <cond>, <clause>` prints "Then if ...". `you lose 1 life
unless opponent life 10 or less` prints "you lose 1 life unless an
opponent has 10 or less life": the clause happens only if the condition
doesn't hold as it resolves. Conditions
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
`tap or untap target creature` (the controller chooses on resolution).
`next spell costs {1} less` prints "the next spell you cast this turn costs {1}
less to cast" (CR 601.2f): generic mana off the controller's next spell
this turn; several add up, and that spell uses them all up.
`face-down spells cost {1} less this turn` prints "face-down spells you
cast this turn cost {1} less to cast": off the {3} of each face-down spell
(CR 702.37c) the controller casts for the rest of the turn.
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
activated abilities can't be activated.
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
article, `replace damage from a Wizard you control to any: double`
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
life plus 1 instead."; for counters, `replace +1/+1 counters put on
creature you control: double` prints "If one or more +1/+1 counters would
be put on a creature you control, twice that many +1/+1 counters are put
on that creature instead." (also the counters it enters with, CR 122.6).
Token creation (CR 111.1, 614.1a): `replace tokens created under your
control: double` prints "If one or more tokens would be created under your
control, twice that many of those tokens are created instead." (`... by
an effect: double` is the older "If an effect would create one or more
tokens under your control, it creates twice that many of those tokens
instead."); `replace creature tokens created under your control: plus
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
counters.
`replace this put into a graveyard: shuffle into library` prints "If
Darksteel Colossus would be put into a graveyard from anywhere, reveal
Darksteel Colossus and shuffle it into its owner's library instead.": the
card names itself, and the ability works in every zone (CR 113.6b: hand,
library, stack, battlefield); the card goes into the library, which is
then shuffled.
`target opponent loses 3 life unless they sacrifice nonland permanent
else discard 1 card` prints "... unless they sacrifice a nonland permanent
of their choice or discard a card" (CR 608.2d; `else`, since `or` would
start another clause); `each opponent loses 3 life unless each opponent
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
chosen and as the spell resolves); `exchange control of this and target
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
control.
`recruit` (CR 701.70): the controller draws a card, then discards a
card; if the discarded card was a nonland card, they create a 1/1 white
Human Soldier creature token. Prints "Recruit."; `you recruit` prints
"you recruit" (The Queen of Dale's wording after another player's
action).
`spells red or white you cast cost {1} less` prints "Red spells and white
spells"; `spells Goblin or Rogue ...` prints "Goblin spells and Rogue
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
at all. `triggered do once each turn <trigger>: you may ...` prints "Do
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
energy` inside `may` and as a cost ("Pay {E}{E}").

Blight (CR 701.68): `blight 2` is an effect ("blight 2": the controller
puts two -1/-1 counters on a creature they control, chosen as it
resolves; with none, nothing happens and `if you do` after `you may
blight 2` fails, 701.68b) and a cost (`{T}, blight 1: ...` prints "{T},
Blight 1: ..."; `additional cost blight 1` with `additional cost or {3}`
prints "As an additional cost to cast this spell, blight 1 or pay {3}.").
As a cost the engine puts the counters on the toughest creature its payer
controls, and it can't be paid without a creature.

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
with the card's behold filter: the behold cost was paid.

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
Trigger `whenever you turn a permanent face up` (a permanent you
control; "it" is that permanent). `return up to 2 permanent card from
among them to hand chosen` after a mill prints "return ... from among
them to your hand", and `put 1 Hero or enchantment card from among those
cards into hand chosen` "... from among those cards into your hand" (Rick
Jones). In a triggered ability, `put its counters on this` moves the
triggering object's counters, as it last existed ("Whenever another
creature you control dies, if it had counters on it, put its counters on
this creature."; the condition `if it had counters` is any kind on the
triggering object). `impulse 2 next turn it` prints "You may play them
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
Wolf you control`. A spell target may say what the spell targets:
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
`destroy <affects>`, `enters`.

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

Outcomes: `prevent`, `prevent next 4`, `double`, `plus one`, `exile`,
`draw two`, `win`, `tapped`, `with 2 +1/+1 counters`, `regenerate`,
`redirect to attached`, `umbra armor` (`replace destroy enchanted: umbra
armor` prints "Umbra armor", CR 702.89a: if the enchanted permanent would
be destroyed, all damage is removed from it and the Aura is destroyed
instead; it applies even if the permanent can't be regenerated); in a `this turn, replace` effect also `prevent
next time` ("The next time a red source of your choice would deal damage
to you this turn, prevent that damage.": a shield for one damage event,
CR 615.7) and redirection (CR 614.9): `redirect to this`, `redirect to
target creature` (a new target, declared after the event's), and
`redirect next 1 to ...` ("The next 1 damage that would be dealt to this
creature this turn is dealt to target creature you control instead."). A
redirection with a source prints "All damage that would be dealt this
turn to A by S is dealt to B instead." (Kor Chant). It does nothing if
the new recipient has left the battlefield or is no longer a creature or
planeswalker.

### Filters

A filter names a kind of permanent, in the singular: `creature`,
`creature you control`, `creature an opponent controls`, `creature you
don't control`, `white creature`, `nonblack creature`, `nonbasic land`,
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

More filter words: `token` ("a token", "artifact token", CR 111.1),
`nontoken`, `tapped`, `untapped`, `attacking`, `blocking`,
`damaged` (also written `that was dealt damage this turn`),
`multicolored`, `monocolored`, `colorless`, `basic`, `legendary`, `snow`,
`nonsnow`, `modified` (CR 700.9; `nontoken modified creature` prints
"nontoken modified creature", and a dies trigger judges `tapped`,
`token`/`nontoken`, counters, and `modified` as the creature last existed,
CR 603.10a),
colors joined with `or` (`red or green creature`, sharing a color), types
joined with `or` or listed (`artifact or enchantment`, `artifact,
enchantment, or land`: at least one), negations listed with commas
(`nonartifact, nonblack creature`), and `with
power 2 or less`, `with toughness 4 or greater`, `with mana value 3`.
A chosen color or type comes last: `creature you control of chosen color`
("creatures you control of the chosen color", the color the source chose
as it entered, CR 607.2a), `creature of chosen type`, `permanent you
control of that type` (chosen earlier as the effect resolved), `enchantment
of color of choice`, `creature of creature type of choice` (chosen as the
effect resolves). With nothing chosen, nothing matches. `named this` is
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
"creature that isn't enchanted": an Aura is attached to it, CR 303.4; the
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
fits is offered; on a card, the mana value prints after the noun: `target
creature card with mana value X or less from your graveyard`; several targets print "creature cards with mana value 2 or less", and `card each with mana value 3 or less` "creature cards each with mana value 3 or less"), `with +1/+1
counter` / `with counter` ("creature you control with a +1/+1 counter on
it", "with a counter on it", CR 122.1; plural "with +1/+1 counters on
them"; printed after the controller; any kind: `permanent you control with
oil counter`, "permanents you control with oil counters on them"), `with
3 or more level counters` ("a creature with three or more level counters
on it"), and `modified` ("a modified
creature", CR 700.9: it has a counter, is equipped, or is enchanted by an
Aura its controller controls), `attached to creature` / `attached to land`
("target Aura attached to a creature": what it is attached to, CR
303.4), `face-down` ("target face-down creature", CR 708.2), and `with toxic` ("each other creature you
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
`add subtype Saproling`, `goaded` ("is goaded", CR 701.15: attacks each
combat if able, and a player other than the source's controller if able),
`control`
(you control it), `add color white`, `set color blue`, `set color chosen`,
`has protection from chosen color`, `set land chosen` ("Enchanted land is
the chosen type") and `add chosen type` ("is the chosen type in addition
to its other types") (the color or type the source chose as it
entered), `switch pt`,
`cda color all`, `cda color colorless` ("Ghostfire is colorless.": a
color-defining ability, CR 604.3, so the card is colorless in every zone,
the stack included), `cda pt card types in graveyards +1`,
`cda pt creature cards in graveyards +1`, `cda pt count Swamp you control`,
`cda pt your life`, `cda pt domain` (a power/toughness-defining ability,
layer 7a, counting any count a static ability can (a count only a resolving effect has, such as `drawn this turn`, `greatest power ...`, `party`, or a target's or the triggering object's, is a compile error in a static ability: it would read 0): "~'s power and
toughness are each equal to ..."; the count is judged from its controller
as the layer applies), `cda power count Rat you control` ("~'s power is
equal to the number of Rats you control"; the toughness is printed, `pt
*/1`), `cda toughness domain` ("~'s toughness is equal to ..."; `cda noun power
...` names the card's noun instead, "This creature's power is equal to
...", "This Vehicle's power", where Oracle does), and `has ability { <ability line> }`: a quoted ability
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
all its card types, land included, keeping its supertypes (layer 4);
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

In a triggered ability, `triggering card` is the card the triggering
object became ("When enchanted creature dies, return that card to its
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
Conviction). `it deals 1 damage to each opponent` makes the
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
number of ..."), or `count each <expr>` ("... for each ..."), with
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
(the source's), `life`, `party` ("creatures in your party", CR 700.8: one
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
opponents' graveyards"), `opponents attacked` ("opponents you're
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
`+1/+1 counters on this`), `permanents sacrificed this turn` ("the number of
permanents you've sacrificed this turn"), `two to x` (`count where two to x`
prints "2ˣ": "Target player draws 2ˣ cards."), `creature attacked this
turn` ("for each creature that attacked this turn": any player's
attackers), `opponents lost life this turn` ("for each opponent who lost
life this turn"), `opponents more cards in hand` ("... who has more cards in
hand than you"), `total power creature you control` ("the total power of
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
its mana cost"). `exile instead` after either adds "If that spell would
be put into your graveyard, exile it instead." (it is exiled as it
resolves or is countered).

Other durations (CR 611.2b) end a sentence, and whatever in it would last
until end of turn (or "this turn") lasts that long instead: `until your
next turn` (it ends as the controller's next turn begins: `each creature
your opponents control gets -3/-0 until your next turn`, "Creatures your
opponents control get -3/-0 until your next turn"; `until your next turn,
up to 1 target creature gets -5/-0` prints it first), `for as long as this
remains` ("for as long as this creature remains on the battlefield", "...
The Wondrous Wasp remains ..."), `for as long as you control this` ("for as
long as you control this creature"), and `for as long as it remains tapped`
(each affected permanent, while it stays tapped). The last three end when
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

Subject-first clauses: `<who> gains 3 life`, `<who> loses 1 life`, `<who> loses half life` ("that player loses half their life, rounded up", CR 107.1a; `you lose half life`: "your life"),
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
`<what> becomes 2/2 Assembly-Worker artifact creature`, `this becomes a
copy of <what> [with exceptions]`, `<what> bites <what>` ("deals damage
equal to its power to"; the creature is the damage's source, so its
deathtouch, lifelink, infect, and wither apply, and it deals none if
either target is illegal as the spell resolves), `<what> bites itself`
("deals damage to itself equal to its power"), `<what> bites twice <what>`
("deals damage equal to twice its power to"); `up to two target creatures
you control each bites <what>` prints "each deal damage equal to their
power to", each creature dealing its own damage. `deal damage equal <expr>
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
on target creature"),
`remove 1 +1/+1 counter from <what>`, `remove 1 counter from <what>` ("remove a counter from target permanent": a counter of a kind the controller chooses), `remove all counters from <what>`,
`return <what> to hand`, `return <what> to battlefield [with 1 +1/+1 counter]`,
`regenerate <what>`, `exile <what> [and <what>]` (a permanent or a graveyard card),
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
graveyard"), `put <what> on top of library` / `on bottom of
library` / `into library 2 from top` ("Put target creature into its
owner's library second from the top"; fewer cards, the bottom, CR 401.7),
`shuffle <what> into library` ("Shuffle enchanted creature into its
owner's library"; `you may shuffle up to 1 target card from your
graveyard into library`: "You may shuffle up to one target card from
your graveyard into your library"), `shuffle graveyard into library`
("Shuffle your graveyard into your library"; `target player shuffles
graveyard into library`: "Target player shuffles their graveyard into
their library"), `shuffle all creature card from graveyard of target
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
loses flying`, `<who> reveals hand`, `<who> reveals hand discarding
nonland card` (Duress: you choose the card), `<who> discards 1 card at
random` (`discard 1 card at random` for you; `that player discards
another card at random`), `<who> reveals 1 card at random from hand`
("reveals a card at random from their hand"; the card is `that card`
after it: `then loses count equal mana value of that card life`, "then
loses life equal to that card's mana value"), `<who> skips next draw
step` ("Target player skips their next draw step", CR 500.11), `<who>
skips next combat phase` ("Target opponent skips their next combat
phase"; `... combat phase this turn`: only a combat phase of this turn;
the main phase after it still happens), `<who> takes extra turn` ("Take
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
"it's your combat phase", "it's the first combat phase of the turn"),
`sacrifice <what>`, `tap <what>`, `untap <what> [and <what>]`,
`gain control of <what>`, `attach to <what>` (this: "attach this Equipment to ..."),
`attach <what> to <what>` ("Attach target Equipment you control to target
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
tapped and attacking"),
`create 1 "0/0 green Elemental creature" sized count where land you
control` ("Create an X/X green Elemental creature token, where X is the
number of lands you control."; `sized x` is the spell's or ability's X,
"Create an X/X green Ooze creature token."): X is fixed as the effect
resolves, before the token exists (CR 608.2h), and the token has base
power and toughness X/X from its creation on (CR 111.3; a layer-7b effect,
so a copy of the token is 0/0); the definition's power and toughness print
as "X/X",
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
`create token copy of <what> [with exceptions]` (of a group, `create
token copy of each nontoken creature you control` prints "For each
nontoken creature you control, create a token that's a copy of that
creature"; `create 2 token copies of it` prints "create two tokens that
are copies of it", `create x token copies of x target permanent` "For
each of X target permanents, create X tokens that are copies of that
permanent."; `those copies gain haste` then prints "Those tokens gain
haste until end of turn." where `those tokens` prints "They"), `copy <what> [times
<num>] [new targets] [with exceptions]` (CR 707.10: `copy target instant
or sorcery spell you control new targets` prints "Copy target instant or
sorcery spell you control. You may choose new targets for the copy.";
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
controller's), `emblem "Name"`,
`draw 2 cards` (you), `discard 1 card` (you), `mill 2 cards` (you),
`scry 2`, `surveil 1`, `investigate` (create a Clue token; `investigate
twice`; `investigate count each opponents lost life this turn`:
"investigate for each opponent who lost life this turn", `investigate once
count each ...`: "investigate once for each ..."; `target player
investigates`: that player creates it), `<who>
exiles 2 cards from hand` ("Target opponent exiles two cards from their
hand"; `from graveyard`: "a card from their graveyard"; the player
chooses), `<who> exiles 1 creature they control` ("Target opponent
exiles a creature they control", their choice; `... they control and
their graveyard` then exiles their graveyard: "Target opponent exiles a
creature they control and their graveyard"), `<who> reveals hand
exiling nonland card from graveyard or hand` ("Target opponent reveals
their hand. You choose a nonland card from that player's graveyard or
hand and exile it."), `transform
this`,
`add {B}{B}` / `add any` ("Add {B}{B}.", "Add one mana of any color."; an
activated ability that only adds mana is a mana ability and resolves at
once, CR 605), `add any 2` ("Add two mana of any one color."), `add 2 mana
any combination` / `add 3 mana any combination of {R}{G}` / `add 2 mana
different colors` ("Add two mana in any combination of colors.", "Add
three mana in any combination of {R} and/or {G}.", "Add two mana of
different colors."; each mana's color chosen as it is added; `add X mana
any combination` with the ability's X, `add X mana any combination count
where creature you control with defender` "Add X mana in any combination
of colors, where X is the number of ..."), `add chosen` ("Add one mana of
the chosen color."), `add {G} count equal power` ("Add an amount of {G}
equal to this creature's power."), `add any that land opponent controls
could produce`, `add any that land you control could produce`, `add any
that basic land you control could produce` ("Add one mana of any color
that a land an opponent controls could produce.": the colors those lands'
mana abilities could make as it resolves, CR 106.7, tapped or not; none,
no mana). An activated ability that adds mana and has no targets is a mana
ability even if it does more (CR 605.1a): `activated {T}, remove 1
depletion counter from this: add {G}{G}; if this has no depletion
counters, sacrifice this` resolves at once, and payment activates it
(removing the counter, then the rest) when plain sources fall short,
`add {C}; if you control Urza's Mine and Urza's Tower, add {C}{C}
instead` counting as the mana it would make now. `search basic land card reveal to hand`, `search up to 2
Forest card to battlefield tapped`, `search basic land card reveal to
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
or a card with flash"; `search Rebel permanent card with mana value 3 or
less to battlefield`: the mana value after the noun, `permanent card`
not an instant or sorcery (CR 110.4a); `search card to graveyard that
card` ("Search your library for a card, put that card into your
graveyard, then shuffle."); `search creature card reveal to hand or
graveyard` ("..., reveal it, put it into your hand or graveyard, then
shuffle.": the searcher chooses, `choose "1"` hand or `choose "2"`
graveyard in a scenario); `reveal to hand that card`:
"reveal that card, put it into your hand"; `search up to 2 basic land
card reveal to battlefield tapped and hand that card`: "reveal those
cards, put one onto the battlefield tapped and the other into your
hand" (the searcher chooses which; one found goes where they choose);
another player searches their own library with a subject: `its
controller may search basic land card to battlefield tapped` ("Its
controller may search their library for ..., put it onto the battlefield
tapped, then shuffle": declining before finding anything is not
searching, so nothing is shuffled), `target player searches basic land
card to battlefield tapped` ("Target player searches their library for
..., puts it onto the battlefield tapped, then shuffles"). A card
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
is "until your next end step", which ends as that step begins; `choose`
adds "Choose one of them." and only that card may be played, `this way`
prints "cards exiled this way", `of target opponent` takes the cards from
that player's library ("Target opponent exiles the top card of their
library. You may play that card this turn.", the permission still yours),
a count `count where ...` prints "the top X cards of your library, where
X is ...", and `that much` "exile that many cards from the top of your
library"), `exile top 3 cards` ("Exile the top three cards of your
library."; `exile top 1 card`: "the top card"; `each opponent exiles top 1
card`: "Each opponent exiles the top card of their library."),
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
`you may <clause>`, `pay {2}` / `pay 2 life` (after `you may`),
`that player may pay {4}` / `that player may discard 1 card at random`
(another player decides and acts as it resolves, CR 608.2d; an act they
can't complete isn't offered; a following `if they do,` prints "If the
player does,"), `the creature` (in a triggered ability, the triggering
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
target`; after `draw 1 card then discard 1 card` it prints "When you discard
a card this way, ..."; `you may you sacrifice a creature` prints "you may sacrifice a
creature", and an empty choice is no action), `<clause> unless you <clause>` (an act, not a payment: `sacrifice this
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
entering with three +1/+1 counters), `<what> connives` (CR 701.50), `<what> becomes prepared` (CR 722.3a), `dig 4 take creature
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
rest" after `revealing`; `dig 3 then take card rest graveyard`: "look at the top three
cards of your library, then put one of them into your hand and the rest
into your graveyard"; `dig 3 take card may top rest graveyard`: "You may
put one of those cards back on top of your library. Put the rest into
your graveyard.", the card kept stays on top; `revealing that card`
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
with mana value 5 or less` is a permanent card there), `look at top 3` ("look at
the top three cards of your library, then put them back in any order";
`look at top count where hand` puts "where X is the number of cards in
your hand" before ", then put them back"; `look at top 3 of target
player`: "... of target player's library, then put them back in any
order"; `look at top 3 of target player only` ends at "Look at the top
three cards of target player's library."; `that player looks at top 3 of
you`: "That player looks at the top three cards of your library, then puts
them back in any order": the looker knows the cards where they are),
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
tapping up to one creature to reduce it, CR 702.180a), `put land card from hand onto battlefield [tapped]` ("put a land
card from your hand onto the battlefield"; `put creature card and land
card from hand onto battlefield`: "put a creature card and/or a land card
from your hand onto the battlefield", a choice of each in turn), `<what> doesn't untap next
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
loyalty counters, opponents' -1/-1 counters and poison), `<what>
explores` (CR 701.44; `<what> explores twice` prints "explores, then it explores again"; a creature card returned from a graveyard is "that creature" afterwards), `<what> endures 2` (CR 701.63: its controller puts that many +1/+1 counters on it or creates an N/N white Spirit token, needing the token "N/N white Spirit creature"; `endures X` makes a 0/0 Spirit that size; one that has left the battlefield makes the token; endure 0 does nothing; `this endures 1` prints "it endures 1" after the source is named, "this creature endures 1" after another clause, "Anafenza endures 2" for a legendary source; scenarios `choose "1"` counters, `choose "2"` the token), `<what> phases out` (CR 702.26: treated as though
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
"... that creatures would deal to players this turn"), `prevent all combat
damage to and from target 1 this turn` ("dealt to and dealt by it"),
`prevent all damage this turn to <affects> [from <source>]` ("Prevent
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
also works ("Draw a card at the beginning of the next turn's upkeep"), and
`at end of combat` ("destroy it at end of combat"). Inside a delayed ability `it` is the object
the ability is about, and so is `that card`, which prints so where `it`
prints "it" (`delayed target 1: return that card to battlefield at
beginning of the next end step`: "Return that card to the battlefield
under its owner's control ..."; `return it`: "Return it ..."); in one about `that creature` (the triggering object),
`that creature` is that object too, only while it is the same object
(CR 603.7c), and prints "that creature"; in one about a target, `that
creature` is the object too. `delayed target 1: when that creature dies
this turn, <effects>` (and `when that creature leaves the battlefield this
turn`) triggers once if that object dies (leaves) and ends unfired at the
cleanup step (CR 603.7c). A target declared by the delayed ability
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
step."); both are the token. `delayed this: whenever <trigger> this turn, <effects>` triggers on
every such event until the turn ends (CR 603.7c), as its controller's
ability: "Whenever a creature blocks this turn, its controller loses 1
life." (`its controller` is the triggering object's), "Whenever a
nontoken creature you control dies this turn, create ...". Led by
`until end of turn,` it is the same ability printed with the duration
first: `until end of turn, delayed this: whenever creature you control
deals combat damage to a player, investigate` ("Until end of turn,
whenever a creature you control deals combat damage to a player,
investigate."). A delayed trigger printed "Whenever" with neither is a
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
creature you control; they gain trample`. As a count, `count each those
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
colorless, CR 105.2c), and targets.
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
X, which can't exceed what there is to target), `1 to 2 target creature`
("one or two target creatures"; `1 to 3` prints "one, two, or three"),
`any number of target player`, and `1 to 2 targets` / `any number of
targets` for "any target". Later mentions (`target 1`) print "them" or
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
any`, Boulder Dash). `another target blocking creature` prints "another
target blocking creature". `another` before a target with no "other"
form is an error rather than dropped.
`deal X plus 1 damage divided among any number of targets` ("X plus 1
damage"; also wherever a number goes). The same damage to several targets
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
`artifact or creature`, `attacking or blocking creature`, `attacking
creature`, `blocking creature`, `<Subtype> creature` ("target Assembly-Worker creature"), and any filter
("target nonblack creature", "target creature an opponent controls").
Targets relative to a player: `target creature that player controls` (the
triggering player: "Whenever this creature deals combat damage to a
player, ... target creature that player controls"), `another target
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
other than this creature"). A group or count of what a target player controls declares that
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

A kicked "instead" that declares its own targets replaces the targets
before it (CR 601.2c): `target creature you control gains
indestructible; if kicked, instead any number of target creature you
control gains indestructible` (Divine Resilience) has one target unkicked
and only the group kicked.

The condition is `kicked`, any static condition (`if you control 3
creature, ...`, `... instead if hand 0 or less`), or the target as it
resolves: `if target 1 is Human, ...` ("If it's a Human, ..."), `if that
target 1 is other Hero, ...` ("If that creature is another Hero, ..."),
or as it last existed: `if that target 1 was with mana value 2 or less,
...` ("If that spell's mana value was 2 or less, ...", after countering
it).
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
- `tap 1 other untapped Citizen you control chosen` ("tap another untapped
  Citizen you control"), `tap 2 other untapped creature you control chosen`
- `sacrifice 1 creature chosen` ("sacrifice a creature")
- `discard 1 creature card chosen` ("discard a creature card")
- `exile 2 creature card from your graveyard chosen`
- `attach 1 Equipment you control to target 1 chosen` ("attach an Equipment
  you control to it": permanents you control, attached to that target)
- `return 1 creature or land card from your graveyard to hand chosen`
  ("return a creature or land card from your graveyard to your hand"),
  `return up to 2 creature card from your graveyard to hand chosen`
- `return up to 2 land you control to hand chosen` ("return up to two
  lands you control to their owner's hand")
- `put 1 permanent card from among milled cards into hand chosen` ("put a
  permanent card from among the milled cards into your hand"; `from among
  them`; `from among milled this way cards`: "from among the cards milled
  this way"): cards this resolution milled that are still in the
  graveyard.

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
...` (cards you owned), `if you put Town card into hand this way, ...`.
Verbs: `exiled`, `sacrificed`, `destroyed`, `discarded`, `milled`,
`returned` (put into a hand), `shuffled` (put into a library). What a spell's or ability's costs sacrificed
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
1` ("the number of cards in that player's hand"), `greatest toughness
other creature you control` ("the greatest toughness among other
creatures you control"), `life you lost this turn` ("the life you've lost
this turn"), and `<expr> plus <expr>` ("the number of creatures on the
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
player discards one such card or else two). `<who> reveals hand discarding
may nonland card` ("You may choose a nonland card from it."), `look at hand
of target player discarding may nonland card` ("look at target player's
hand. You may choose ..."), `look at hand of target player discarding card`
("Look at target player's hand and choose a card from it."). `<who>
reveals hand exiling nonland card` ("Target opponent reveals their hand.
You choose a nonland card from it and exile that card."; `exiling
nonland card split`: "You choose a nonland card from it. Exile that
card."). `nonland noninstant nonsorcery card` prints "a nonland
permanent card" (CR 110.4a). A sentence
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
`that card`. `may cast that card this turn` ("You may cast that card this
turn."; `may play that card next turn first`: "Until the end of your next
turn, you may play that card."; `free`: "... without paying its mana
cost"; `it` for "it"; the durations are impulse's) lets the controller
play or cast the resolution's card from exile, also after `exile target
... card from your graveyard`. `put that card into hand and the rest on
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
shuffles." (CR 701.24).

After `if it is ...`, `if you dont put it into hand, <sentence>` is "If
you don't put the card into your hand, ..." (`onto battlefield`, `into
graveyard`, `on bottom of library` likewise): it happens when the card
didn't match or the optional put was declined. `if it shares creature type with this, ...` is kinship's "If it shares a
creature type with this creature, ..." (changeling's every type
included); `reveal it` reveals a looked-at card where it is ("you may
reveal it. If you do, ..."). `draw 1 card and reveal it` is "Draw a card
and reveal it." with the drawn card `it` after, and `discard it` discards
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
if the second is gone. `put its counters on target creature you control`
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
double strike"). A spell names itself again after such a condition
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
- `label "Max speed" <static, activated, or triggered ability>`, `label
  "Max speed" spells you cast cost {1} less`: "Max speed — ..." (CR
  702.179e): the ability works only while its controller's speed is 4 (a
  static ability's or cost reduction's condition, an activated ability's
  activation condition, graveyard and mana abilities included; a
  triggered ability triggers only at max speed). A dies trigger under it
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

Cases (CR 719) are `type Enchantment — Case` cards with `to solve
<condition>` ("To solve — You control three or more artifacts.", any
static condition, printed capitalized) and `solved <ability>` ("Solved —
<ability>", any ability line after `solved`). "this Case" is the
self-noun. Like a Class, a Case has a definition per state
(`CardDef::stages`): unsolved, the card itself, with its to-solve
ability, "At the beginning of your end step, if <condition>, this Case
becomes solved" (judged as it triggers and as it resolves, CR 719.2a),
and not its solved abilities; solved, the reverse (CR 719.2b). It stays
solved until it leaves the battlefield.

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
Vehicles." In a trigger on `Vehicle you control attacks`, `that
Vehicle` is the attacking Vehicle.

`replace this dies: exile`: "If this creature would die, exile it
instead."
