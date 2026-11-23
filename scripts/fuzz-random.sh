#!/bin/bash
# Fuzz with random two-color decks drawn from the whole card corpus, so
# every card gets played. usage: scripts/fuzz-random.sh [binary] [games]
# Each run is killed after FUZZ_TIMEOUT seconds (default 600) and prints a
# TIMEOUT line: a run that long holds a game that does not end. The bar is
# twenty `wins` lines, "panics 0" on each, and no TIMEOUT.
B=${1:-target/release/mtg}; G=${2:-1000}; T=${FUZZ_TIMEOUT:-600}
run() {
  timeout "$T" "$B" sim "$@" 2>&1 | grep -E "^wins|^panic \(" | head -4
  [ "${PIPESTATUS[0]}" = 124 ] && echo "TIMEOUT after ${T}s: sim $*"
}
for seed in 201 202 203 204 205 206 207 208 209 210; do
  echo "== random:$seed vs random:$((seed+50)) seed $seed"
  run --deck1 random:$seed --deck2 random:$((seed+50)) --games $G --seed $seed
  run --deck1 random:$seed --deck2 random:$((seed+50)) --agent1 greedy --agent2 greedy --games $((G/2)) --seed $((seed+1000))
done
