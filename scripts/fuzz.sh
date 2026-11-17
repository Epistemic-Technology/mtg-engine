#!/bin/bash
# Fuzz the engine with random and greedy agents over the built-in stress
# decks. usage: scripts/fuzz.sh [binary]   (default target/release/mtg)
# Prints the wins/panics line of each matchup; "panics 0" everywhere is the
# bar (design.md §6.3, gate 4).
B=${1:-target/release/mtg}
for args in "--deck1 copy --deck2 copy --games 3000 --seed 101" \
            "--deck1 copy --deck2 mono-red --games 3000 --seed 102" \
            "--deck1 copy --deck2 activated --games 3000 --seed 103" \
            "--deck1 copy --deck2 triggers --games 2000 --seed 104" \
            "--deck1 copy --deck2 copy --agent1 greedy --agent2 greedy --games 2000 --seed 105" \
            "--deck1 copy --deck2 copy --agent1 greedy --agent2 random --games 2000 --seed 106" \
            "--deck1 mono-white --deck2 copy --agent1 greedy --agent2 greedy --games 2000 --seed 107" \
            "--deck1 replacement --deck2 copy --games 3000 --seed 108" \
            "--deck1 replacement --deck2 copy --agent1 greedy --agent2 greedy --games 2000 --seed 109" \
            "--deck1 slice6 --deck2 slice6 --games 3000 --seed 110" \
            "--deck1 slice6 --deck2 mono-red --games 3000 --seed 111" \
            "--deck1 slice6 --deck2 copy --games 2000 --seed 112" \
            "--deck1 slice6 --deck2 replacement --games 2000 --seed 113" \
            "--deck1 slice6 --deck2 slice6 --agent1 greedy --agent2 greedy --games 2000 --seed 114" \
            "--deck1 slice6 --deck2 activated --agent1 greedy --agent2 random --games 2000 --seed 115"; do
  echo "== $args"
  $B sim $args 2>&1 | grep -iE "panic|unfinished|wins" | head -4
done
