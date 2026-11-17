"""A Magic: The Gathering rules engine for AI play.

`Engine` steps one game decision at a time; `simulate_batch` plays many
games between built-in agents with the GIL released; `MtgEnv` wraps a game
as a PettingZoo AEC environment with exact action masks.
"""

from ._mtg_engine import (
    CARD_FEATURES,
    CHOICE_KINDS,
    DECISION_KINDS,
    ENGINE_VERSION,
    OBS_SIZE,
    OPT_SIZE,
    Agent,
    Engine,
    card_count,
    card_id,
    card_name,
    card_text,
    simulate_batch,
)
from .env import MtgEnv, env, raw_env

__all__ = [
    "Agent",
    "CARD_FEATURES",
    "CHOICE_KINDS",
    "DECISION_KINDS",
    "ENGINE_VERSION",
    "Engine",
    "MtgEnv",
    "OBS_SIZE",
    "OPT_SIZE",
    "card_count",
    "card_id",
    "card_name",
    "card_text",
    "env",
    "raw_env",
    "simulate_batch",
]
