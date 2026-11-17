#!/bin/bash
# A/B throughput: best of five, one thread, pinned to one core, binaries
# interleaved so load on a shared machine hits both alike (design.md §8).
# usage: scripts/bench.sh <deck1> <deck2> <binary>...   e.g.
#   scripts/bench.sh mono-red mono-red /tmp/mtg-before target/release/mtg
d1=$1; d2=$2; shift 2
CORE=${CORE:-14}
declare -A best
for i in 1 2 3 4 5; do
  for b in "$@"; do
    v=$(RAYON_NUM_THREADS=1 taskset -c $CORE $b sim --deck1 $d1 --deck2 $d2 --games 2000 --seed 11 2>&1 | grep throughput | sed -E 's/.*, ([0-9]+) games\/s per core/\1/')
    if [ -z "${best[$b]}" ] || [ "$v" -gt "${best[$b]}" ]; then best[$b]=$v; fi
  done
done
for b in "$@"; do echo "$d1 vs $d2  $b: ${best[$b]} games/s (best of 5)"; done
