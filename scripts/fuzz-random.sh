#!/bin/bash
# Fuzz with random two-color decks drawn from the whole card corpus, so
# every card gets played. usage: scripts/fuzz-random.sh [binary] [games]
B=${1:-target/release/mtg}; G=${2:-1000}
for seed in 201 202 203 204 205 206 207 208 209 210; do
  echo "== random:$seed vs random:$((seed+50)) seed $seed"
  $B sim --deck1 random:$seed --deck2 random:$((seed+50)) --games $G --seed $seed 2>&1 | grep -E "^wins|^panic \(" | head -4
  $B sim --deck1 random:$seed --deck2 random:$((seed+50)) --agent1 greedy --agent2 greedy --games $((G/2)) --seed $((seed+1000)) 2>&1 | grep -E "^wins|^panic \(" | head -4
done
