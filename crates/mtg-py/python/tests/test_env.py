import numpy as np
from pettingzoo.test import api_test

import mtg_engine as me


def masked_random(rng, obs):
    mask = obs["action_mask"]
    live = np.flatnonzero(mask)
    return int(rng.choice(live))


def test_env_passes_pettingzoo_api_test():
    api_test(me.env(seed=1), num_cycles=200, verbose_progress=False)


def test_two_seat_game_runs_to_the_end():
    env = me.raw_env(seed=2)
    env.reset()
    rng = np.random.default_rng(2)
    steps = 0
    for agent in env.agent_iter(max_iter=50000):
        obs, reward, term, trunc, info = env.last()
        if term or trunc:
            env.step(None)
            continue
        env.step(masked_random(rng, obs))
        steps += 1
    assert env.engine.is_over()
    assert steps > 10
    rewards = env._cumulative_rewards
    assert env.overflow == 0


def test_single_seat_against_builtin_opponent():
    env = me.raw_env(seed=3, opponent="greedy")
    env.reset()
    rng = np.random.default_rng(3)
    seats = set()
    for agent in env.agent_iter(max_iter=50000):
        obs, reward, term, trunc, info = env.last()
        if term or trunc:
            env.step(None)
            continue
        seats.add(agent)
        env.step(masked_random(rng, obs))
    assert seats == {"player_0"}
    assert env.engine.is_over()


def test_observation_matches_space():
    env = me.raw_env(seed=4, max_options=64)
    env.reset()
    obs = env.observe("player_0")
    assert obs["observation"].shape == env.observation_space("player_0")["observation"].shape
    assert obs["action_mask"].shape == (64,)
    assert obs["action_mask"].sum() == min(64, env.engine.num_options())
