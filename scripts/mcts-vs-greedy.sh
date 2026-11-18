#!/bin/bash
# M5 acceptance: the IS-MCTS agent against the greedy agent over a fixed
# matchup set, each matchup played with the agents in both seats.
# usage: scripts/mcts-vs-greedy.sh [binary] [games per side] [agent]
B=${1:-target/release/mtg}; G=${2:-100}; A=${3:-mcts}
total=0; wins=0
for m in "mono-red mono-green" "mono-white mono-black" "mono-blue mono-red" "mono-green mono-white" "slice6 activated" "triggers layers"; do
  set -- $m
  w1=$($B sim --deck1 $1 --deck2 $2 --agent1 $A --agent2 greedy --games $G --seed 500 2>&1 | awk '/^wins:/ {print $3}')
  w2=$($B sim --deck1 $1 --deck2 $2 --agent1 greedy --agent2 $A --games $G --seed 900 2>&1 | awk '/^wins:/ {print $5}')
  echo "$1 vs $2: $A won $w1/$G as P1 ($1), $w2/$G as P2 ($2)"
  wins=$((wins + w1 + w2)); total=$((total + 2 * G))
done
echo "overall: $A won $wins of $total ($((100 * wins / total))%)"
