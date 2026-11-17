# mtg-arena

Records MTG Arena matches from the client's log and translates them into
traces for testing the engine against the real game.

## Setup

Arena only logs game messages when asked: **Options → Account → Detailed
Logs (Plugin Support)**, then restart Arena. The recorder warns when the
setting is off.

## Use

```
mtg arena watch                 # follow Arena's log, record every match; runs until killed
mtg arena import Player.log     # record the matches in a saved log
mtg arena list                  # recorded matches
mtg arena show [MATCH]          # readable play-by-play (default: the latest match)
mtg arena trace [MATCH] -o t.json   # states, events and decisions as JSON
mtg arena cards [MATCH]         # which of the match's cards the engine has
mtg arena service install       # run `watch` in the background from login (macOS)
mtg arena service status | uninstall
```

`MATCH` is a number from `list` (negative counts from the newest), part of
an archive's name, or a path. `--store DIR` changes where matches are kept
(default `~/Library/Application Support/mtg-engine/arena`); `--card-db`
points at Arena's card database if it is not found in the installed client.

`watch` re-reads `Player-prev.log` and `Player.log` from the top each time
it starts and skips what it already has, so games played while it was not
running are picked up, as long as Arena has not been launched twice since
(Arena keeps one previous log).

`service install` registers a launchd agent
(`~/Library/LaunchAgents/com.mtg-engine.arena-recorder.plist`) that runs
the executable it was invoked from. Install the binary somewhere stable
first (`cargo install --path crates/mtg-cli`); its output goes to
`recorder.log` in the store.

## What is stored

One file per match in `<store>/matches/`, named
`<UTC start>_<match id>.jsonl.gz` (`.jsonl` while the match is in
progress). It holds the messages exchanged with the match service exactly
as Arena logged them, one JSON object per line:

```
{"kind":"header","format":1,"recorder":"0.1.0","match_id":"…","started_ms":1794882313630}
{"kind":"msg","hdr":"[UnityCrossThreadLogger]… GreToClientEvent","data":{…}}
…
{"kind":"end","reason":"completed"}        // or "interrupted": Arena quit first
```

The archive is the source of truth and is never rewritten. Everything
else is derived from it, so improvements to the translation apply to
games already recorded.

Archives contain the opponent's screen name and both players' account
ids. They are kept outside the repository; do not commit them.

## The trace

`mtg arena trace` turns an archive into a `MatchTrace` (see `trace.rs`):

```
match: id, players (seat, name), local_seat, winner
  games[]: number, deck (the recording player's, after sideboarding), result
    frames[]: one per game state Arena reported that changed something
      state_id   Arena's gameStateId (restarts each game)
      events[]   what happened since the last frame: ZoneTransfer, DamageDealt,
                 ModifiedLife, CounterAdded, ResolutionStart, UserActionTaken, …
                 each with its source, targets, and details
      decisions[] questions put to the recording player in this state
                 (ActionsAvailable, SelectTargets, DeclareAttackers, …):
                 the options offered and the answer given
      state      the whole game: turn/phase/step/priority, each player's life,
                 hand, graveyard and library size, battlefield, stack, exile.
                 Objects carry name, types, P/T, tapped, damage, counters,
                 attacking/blocking, targets.
```

Things to know when consuming it:

- **One player's view.** The opponent's hand and both libraries are
  hidden (`kind: "Hidden"` objects, and a count for libraries). The
  opponent's cards are identified when they become visible. There is no
  seed: draws and shuffles have to be forced from what the trace shows.
- **Only the recording player's decisions are decisions.** The opponent's
  appear as events (`UserActionTaken`, `ZoneTransfer` with a category such
  as `CastSpell` or `PlayLand`).
- **Object ids change.** Arena gives an object a new id whenever it
  changes zones; an `ObjectIdChanged` event (`orig_id`, `new_id`) links
  them. Players are referred to by seat number (1 or 2) in the same id
  space.
- **Arena automates.** It taps mana, passes priority, and orders
  triggers on the player's behalf, so decisions do not line up one-to-one
  with the engine's. Request bodies keep Arena's own shape (with enum
  prefixes stripped and card names added) rather than being forced into
  the engine's `Choice` type; the replay harness is where the two get
  matched.
- **Names are Arena's.** Card and ability text come from the client's
  card database (`Raw_CardDatabase_*.mtga`, an SQLite file). Ability text
  uses `CARDNAME` for the source's name.

## Not here yet

The replay harness: driving `mtg-core` with a trace's decisions, forcing
chance outcomes from the trace, and diffing engine state against each
frame. Real games use far more cards than the engine's pool (`mtg arena
cards` reports the overlap), so that work starts with either games played
with supported cards or single-step snapshot tests cut from frames.
