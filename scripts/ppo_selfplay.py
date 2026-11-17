#!/usr/bin/env python3
"""Masked-PPO self-play on a fixed matchup (design §11, M4 acceptance):
one policy plays both seats of `MtgEnv`, rollouts come from one worker
process per core, and the policy scores each legal option from its
feature row, so the same network applies to any deck.

usage: scripts/ppo_selfplay.py [--iters 8] [--workers 16] [--steps 1024]
                               [--envs 8] [--deck-a mono-red] [--deck-b mono-green]
                               [--eval-games 48] [--max-options 64]

Prints collection throughput (environment steps per second including the
policy's forward passes, which is the figure the acceptance target names),
the PPO losses, and the policy's win rate as seat 0 against the greedy
agent after each iteration.
"""

from __future__ import annotations

import argparse
import multiprocessing as mp
import os
import time

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

import mtg_engine as me

GLOBAL = me.OBS_SIZE
OPT = me.OPT_SIZE


class Policy(nn.Module):
    """A global encoder over the view, an option encoder over each option's
    feature row, a scorer over their concatenation, and a value head."""

    def __init__(self, max_options: int, hidden: int = 256, opt_hidden: int = 64):
        super().__init__()
        self.max_options = max_options
        self.global_net = nn.Sequential(
            nn.Linear(GLOBAL, hidden), nn.ReLU(), nn.Linear(hidden, 128), nn.ReLU()
        )
        self.option_net = nn.Sequential(
            nn.Linear(OPT, opt_hidden), nn.ReLU(), nn.Linear(opt_hidden, opt_hidden), nn.ReLU()
        )
        self.scorer = nn.Sequential(nn.Linear(128 + opt_hidden, 64), nn.ReLU(), nn.Linear(64, 1))
        self.value = nn.Sequential(nn.Linear(128, 64), nn.ReLU(), nn.Linear(64, 1))

    def forward(self, obs: torch.Tensor, mask: torch.Tensor):
        """obs: [B, GLOBAL + M*OPT]; mask: [B, M] (1 = legal)."""
        b = obs.shape[0]
        g = self.global_net(obs[:, :GLOBAL])
        opts = obs[:, GLOBAL:].reshape(b, self.max_options, OPT)
        o = self.option_net(opts)
        ge = g.unsqueeze(1).expand(-1, self.max_options, -1)
        logits = self.scorer(torch.cat([ge, o], dim=-1)).squeeze(-1)
        logits = logits.masked_fill(mask == 0, float("-inf"))
        return logits, self.value(g).squeeze(-1)


def sample_batch(policy: Policy, obs: np.ndarray, mask: np.ndarray):
    """Actions, log-probabilities, and values for a batch of observations."""
    with torch.no_grad():
        o = torch.from_numpy(obs)
        m = torch.from_numpy(mask.astype(np.float32))
        logits, value = policy(o, m)
        dist = torch.distributions.Categorical(logits=logits)
        a = dist.sample()
        return a.numpy(), dist.log_prob(a).numpy(), value.numpy()


def sample(policy: Policy, obs: np.ndarray, mask: np.ndarray):
    a, logp, v = sample_batch(policy, obs[None], mask[None])
    return int(a[0]), float(logp[0]), float(v[0])


def worker_rollout(args):
    """Collect `steps` self-play steps with the given weights over `envs`
    games in lockstep (one batched forward pass per step of all of them);
    returns arrays plus a per-seat terminal reward assigned to each step."""
    (seed, steps, state, deck_a, deck_b, max_options, n_envs) = args
    torch.set_num_threads(1)
    policy = Policy(max_options)
    policy.load_state_dict(state)
    policy.eval()
    envs = [
        me.raw_env(deck_a=deck_a, deck_b=deck_b, seed=seed * 64 + e, max_options=max_options)
        for e in range(n_envs)
    ]
    for env in envs:
        env.reset()
    rounds = steps // n_envs
    total = rounds * n_envs
    obs_buf = np.zeros((total, GLOBAL + max_options * OPT), np.float32)
    mask_buf = np.zeros((total, max_options), np.int8)
    act_buf = np.zeros(total, np.int64)
    logp_buf = np.zeros(total, np.float32)
    val_buf = np.zeros(total, np.float32)
    seat_buf = np.zeros(total, np.int8)
    ret_buf = np.zeros(total, np.float32)
    # Per env: the buffer rows of its current game, credited when it ends.
    open_rows = [[] for _ in envs]
    games = 0
    t0 = time.perf_counter()
    for r in range(rounds):
        obs = np.zeros((n_envs, GLOBAL + max_options * OPT), np.float32)
        mask = np.zeros((n_envs, max_options), np.int8)
        for e, env in enumerate(envs):
            agent = env.agent_selection
            if env.terminations[agent] or env.truncations[agent]:
                winner = env.engine.winner()
                for k in open_rows[e]:
                    ret_buf[k] = 0.0 if winner is None else (1.0 if seat_buf[k] == winner else -1.0)
                open_rows[e] = []
                games += 1
                env.reset()
                agent = env.agent_selection
            ob = env.observe(agent)
            obs[e] = ob["observation"]
            mask[e] = ob["action_mask"]
            seat_buf[r * n_envs + e] = int(agent[-1])
        a, logp, v = sample_batch(policy, obs, mask)
        for e, env in enumerate(envs):
            i = r * n_envs + e
            obs_buf[i] = obs[e]
            mask_buf[i] = mask[e]
            act_buf[i] = a[e]
            logp_buf[i] = logp[e]
            val_buf[i] = v[e]
            open_rows[e].append(i)
            env.step(int(a[e]))
    # Unfinished games at the end: bootstrap from the value estimates.
    for rows in open_rows:
        for k in rows:
            ret_buf[k] = val_buf[k]
    elapsed = time.perf_counter() - t0
    overflow = sum(env.overflow for env in envs)
    return obs_buf, mask_buf, act_buf, logp_buf, val_buf, ret_buf, games, elapsed, overflow


def eval_worker(args):
    """Games of the policy as seat 0 against the greedy agent; returns wins."""
    (seed, games, state, deck_a, deck_b, max_options) = args
    torch.set_num_threads(1)
    policy = Policy(max_options)
    policy.load_state_dict(state)
    policy.eval()
    wins = 0
    for g in range(games):
        env = me.raw_env(deck_a=deck_a, deck_b=deck_b, seed=seed + g, opponent="greedy", max_options=max_options)
        env.reset()
        while True:
            agent = env.agent_selection
            if env.terminations[agent] or env.truncations[agent]:
                break
            ob = env.observe(agent)
            a, _, _ = sample(policy, ob["observation"], ob["action_mask"])
            env.step(a)
        if env.engine.winner() == 0:
            wins += 1
    return wins


def evaluate(pool, workers, policy, games, deck_a, deck_b, max_options, seed):
    """Win rate of the policy as seat 0 against the greedy agent, games
    spread over the worker pool."""
    state = {k: v.clone() for k, v in policy.state_dict().items()}
    per = max(1, games // workers)
    jobs = [(seed + w * 1000, per, state, deck_a, deck_b, max_options) for w in range(workers)]
    wins = sum(pool.map(eval_worker, jobs))
    return wins / (per * workers)


def ppo_update(policy, opt, obs, mask, act, logp_old, ret, val, epochs=2, minibatch=2048, clip=0.2):
    adv = ret - val
    adv = (adv - adv.mean()) / (adv.std() + 1e-8)
    n = obs.shape[0]
    stats = {"policy_loss": 0.0, "value_loss": 0.0, "entropy": 0.0, "batches": 0}
    for _ in range(epochs):
        perm = torch.randperm(n)
        for start in range(0, n, minibatch):
            idx = perm[start : start + minibatch]
            logits, value = policy(obs[idx], mask[idx])
            dist = torch.distributions.Categorical(logits=logits)
            logp = dist.log_prob(act[idx])
            ratio = torch.exp(logp - logp_old[idx])
            a = adv[idx]
            pl = -torch.min(ratio * a, torch.clamp(ratio, 1 - clip, 1 + clip) * a).mean()
            vl = F.mse_loss(value, ret[idx])
            ent = dist.entropy().mean()
            loss = pl + 0.5 * vl - 0.01 * ent
            opt.zero_grad()
            loss.backward()
            nn.utils.clip_grad_norm_(policy.parameters(), 0.5)
            opt.step()
            stats["policy_loss"] += pl.item()
            stats["value_loss"] += vl.item()
            stats["entropy"] += ent.item()
            stats["batches"] += 1
    b = max(stats["batches"], 1)
    return {k: v / b for k, v in stats.items() if k != "batches"}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--iters", type=int, default=8)
    ap.add_argument("--workers", type=int, default=os.cpu_count())
    ap.add_argument("--steps", type=int, default=1024, help="steps per worker per iteration")
    ap.add_argument("--deck-a", default="mono-red")
    ap.add_argument("--deck-b", default="mono-green")
    ap.add_argument("--eval-games", type=int, default=40)
    ap.add_argument("--max-options", type=int, default=64)
    ap.add_argument("--envs", type=int, default=8, help="games per worker, stepped in lockstep")
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()
    torch.manual_seed(args.seed)
    policy = Policy(args.max_options)
    opt = torch.optim.Adam(policy.parameters(), lr=3e-4)
    ctx = mp.get_context("spawn")
    total_steps = 0
    t_start = time.perf_counter()
    with ctx.Pool(args.workers) as pool:
        for it in range(args.iters):
            state = {k: v.clone() for k, v in policy.state_dict().items()}
            jobs = [
                (args.seed * 100_000 + it * 1000 + w, args.steps, state, args.deck_a, args.deck_b, args.max_options, args.envs)
                for w in range(args.workers)
            ]
            t0 = time.perf_counter()
            results = pool.map(worker_rollout, jobs)
            collect = time.perf_counter() - t0
            obs = torch.from_numpy(np.concatenate([r[0] for r in results]))
            mask = torch.from_numpy(np.concatenate([r[1] for r in results]).astype(np.float32))
            act = torch.from_numpy(np.concatenate([r[2] for r in results]))
            logp = torch.from_numpy(np.concatenate([r[3] for r in results]))
            val = torch.from_numpy(np.concatenate([r[4] for r in results]))
            ret = torch.from_numpy(np.concatenate([r[5] for r in results]))
            games = sum(r[6] for r in results)
            overflow = sum(r[8] for r in results)
            n = obs.shape[0]
            total_steps += n
            t1 = time.perf_counter()
            policy.train()
            stats = ppo_update(policy, opt, obs, mask, act, logp, ret, val)
            train = time.perf_counter() - t1
            wr = evaluate(pool, args.workers, policy, args.eval_games, args.deck_a, args.deck_b, args.max_options, seed=10_000 + it)
            print(
                f"iter {it}: {n} steps in {collect:.1f}s = {n / collect:,.0f} steps/s "
                f"({games} games finished, {overflow} overflowed); update {train:.1f}s "
                f"policy {stats['policy_loss']:.3f} value {stats['value_loss']:.3f} entropy {stats['entropy']:.2f}; "
                f"win rate vs greedy {wr:.2f}",
                flush=True,
            )
    wall = time.perf_counter() - t_start
    print(f"total {total_steps} steps in {wall:.0f}s = {total_steps / wall:,.0f} steps/s end to end")


if __name__ == "__main__":
    main()
