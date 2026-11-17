"""PettingZoo AEC environment over the engine (design §9.1).

The action space is ``Discrete(max_options)``: an index into the current
decision's option list, masked to the live options. The observation is the
PettingZoo Classic convention, ``{"observation": ..., "action_mask": ...}``,
where the observation vector is the player's view (``OBS_SIZE`` floats)
followed by one feature row per option (``max_options × OPT_SIZE``), so a
policy can score options by content rather than by position.

A game is sequential: whichever seat the engine asks next is
``agent_selection``. With ``opponent`` set to a built-in agent name
(``"random"``, ``"greedy"``), seat 1 is played internally and only
``player_0`` ever acts, which is the single-agent training setup.
"""

from __future__ import annotations

import numpy as np
from gymnasium import spaces
from pettingzoo import AECEnv
from pettingzoo.utils import wrappers

from . import _mtg_engine as core

AGENTS = ("player_0", "player_1")


def env(**kwargs):
    """The environment with PettingZoo's standard wrappers."""
    e = raw_env(**kwargs)
    e = wrappers.AssertOutOfBoundsWrapper(e)
    e = wrappers.OrderEnforcingWrapper(e)
    return e


def raw_env(**kwargs):
    return MtgEnv(**kwargs)


class MtgEnv(AECEnv):
    metadata = {
        "render_modes": ["ansi"],
        "name": "mtg_engine_v0",
        "is_parallelizable": False,
    }

    def __init__(
        self,
        deck_a="mono-red",
        deck_b="mono-green",
        seed=0,
        opponent=None,
        max_options=128,
        logging=False,
        render_mode=None,
    ):
        super().__init__()
        self.deck_a = deck_a
        self.deck_b = deck_b
        self.base_seed = seed
        self.opponent_name = opponent
        self.max_options = max_options
        self.logging = logging
        self.render_mode = render_mode
        self.possible_agents = list(AGENTS)
        self.agents = []
        self.obs_size = core.OBS_SIZE + max_options * core.OPT_SIZE
        self._obs_space = spaces.Dict(
            {
                "observation": spaces.Box(-np.inf, np.inf, (self.obs_size,), np.float32),
                "action_mask": spaces.Box(0, 1, (max_options,), np.int8),
            }
        )
        self._act_space = spaces.Discrete(max_options)
        self.engine = None
        self._opponent = None
        self._games = 0
        # Decisions whose option list exceeded max_options; the mask then
        # covers only the first max_options.
        self.overflow = 0

    def observation_space(self, agent):
        return self._obs_space

    def action_space(self, agent):
        return self._act_space

    # ----- lifecycle -------------------------------------------------------

    def reset(self, seed=None, options=None):
        if seed is None:
            seed = self.base_seed + self._games
        self._games += 1
        self.engine = core.Engine(self.deck_a, self.deck_b, seed=seed, logging=self.logging)
        self._opponent = (
            core.Agent(self.opponent_name, seed) if self.opponent_name else None
        )
        self.agents = list(self.possible_agents)
        self.rewards = {a: 0.0 for a in self.agents}
        self._cumulative_rewards = {a: 0.0 for a in self.agents}
        self.terminations = {a: False for a in self.agents}
        self.truncations = {a: False for a in self.agents}
        self.infos = {a: {} for a in self.agents}
        self._advance()

    def _advance(self):
        """Let the built-in opponent act until a seat the caller plays must
        decide, or the game ends."""
        engine = self.engine
        while not engine.is_over():
            if self._opponent is not None and engine.player == 1:
                engine.step(self._opponent.choose(engine))
            else:
                break
        if engine.is_over():
            self._finish()
        else:
            if engine.num_options() > self.max_options:
                self.overflow += 1
            self.agent_selection = self.possible_agents[engine.player]

    def _finish(self):
        winner = self.engine.winner()
        for i, a in enumerate(self.possible_agents):
            if a not in self.agents:
                continue
            self.terminations[a] = True
            self.rewards[a] = 0.0 if winner is None else (1.0 if winner == i else -1.0)
        self.agent_selection = self.agents[0]

    def observe(self, agent):
        p = self.possible_agents.index(agent)
        view = self.engine.observation(p)
        opts = self.engine.option_features(self.max_options).ravel()
        return {
            "observation": np.concatenate([view, opts]).astype(np.float32, copy=False),
            "action_mask": self.engine.action_mask(self.max_options),
        }

    def step(self, action):
        agent = self.agent_selection
        if self.terminations[agent] or self.truncations[agent]:
            self._was_dead_step(action)
            return
        self._cumulative_rewards[agent] = 0.0
        self.rewards = {a: 0.0 for a in self.agents}
        n = self.engine.num_options()
        action = int(action)
        if action < 0 or action >= n:
            raise ValueError(f"action {action} is not among the {n} legal options")
        self.engine.step(action)
        self._advance()
        self._accumulate_rewards()

    def render(self):
        if self.engine is None:
            return ""
        p = 0 if self.engine.is_over() else self.engine.player
        return self.engine.describe(p)

    def close(self):
        self.engine = None

    # ----- extras ----------------------------------------------------------

    def legal_options(self):
        """The current decision's options as text, in action-index order."""
        return self.engine.options()

    def record_json(self):
        return self.engine.record_json()
