#!/usr/bin/env python3
"""Environment step throughput with a masked random policy: one process,
then one process per core (design §11 M4 acceptance: ≥10k steps/s on 16
cores). usage: scripts/py_throughput.py [steps_per_worker] [workers]"""

import multiprocessing as mp
import os
import sys
import time

import numpy as np


def run(args):
    seed, steps = args
    import mtg_engine as me

    env = me.raw_env(seed=seed, opponent="random")
    env.reset()
    rng = np.random.default_rng(seed)
    done = 0
    while done < steps:
        obs, reward, term, trunc, info = env.last()
        if term or trunc:
            env.reset()
            continue
        live = np.flatnonzero(obs["action_mask"])
        env.step(int(rng.choice(live)))
        done += 1
    return done, env.overflow


def main():
    steps = int(sys.argv[1]) if len(sys.argv) > 1 else 20000
    workers = int(sys.argv[2]) if len(sys.argv) > 2 else os.cpu_count()
    t = time.perf_counter()
    done, overflow = run((1, steps))
    dt = time.perf_counter() - t
    print(f"1 process: {done / dt:,.0f} steps/s ({overflow} overflowed decisions)")
    t = time.perf_counter()
    with mp.get_context("spawn").Pool(workers) as pool:
        results = pool.map(run, [(100 + i, steps) for i in range(workers)])
    dt = time.perf_counter() - t
    total = sum(d for d, _ in results)
    print(f"{workers} processes: {total / dt:,.0f} steps/s ({sum(o for _, o in results)} overflowed)")


if __name__ == "__main__":
    main()
