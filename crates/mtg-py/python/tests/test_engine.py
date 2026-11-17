import json
import random

import numpy as np
import pytest

import mtg_engine as me


def play_out(engine, seed=0, limit=20000):
    rng = random.Random(seed)
    steps = 0
    while not engine.is_over() and steps < limit:
        engine.step(rng.randrange(engine.num_options()))
        steps += 1
    return steps


def test_engine_runs_a_game_to_the_end():
    e = me.Engine("mono-red", "mono-green", seed=3)
    assert e.player in (0, 1)
    assert e.decision_kind == "mulligan"
    steps = play_out(e, seed=3)
    assert e.is_over()
    assert e.result() in ("P1", "P2", "draw")
    assert steps == len(e.history())


def test_observation_and_options_have_the_declared_shapes():
    e = me.Engine("mono-red", "mono-green", seed=1)
    obs = e.observation(0)
    assert obs.shape == (me.OBS_SIZE,)
    assert obs.dtype == np.float32
    feats = e.option_features(16)
    assert feats.shape == (16, me.OPT_SIZE)
    mask = e.action_mask(16)
    assert mask.sum() == min(16, e.num_options())
    assert len(e.options()) == e.num_options()
    # The mulligan decision: keep or mulligan, as text.
    assert e.options()[0] == "keep"


def test_record_round_trips():
    e = me.Engine("mono-red", "mono-green", seed=11)
    play_out(e, seed=11)
    text = e.record_json()
    rec = json.loads(text)
    assert rec["actions"] == e.history()
    again = me.Engine.from_record_json(text, False)
    assert again.is_over() == e.is_over()
    assert again.result() == e.result()
    assert again.turn == e.turn


def test_determinize_keeps_zone_sizes_and_the_game_playable():
    e = me.Engine("mono-red", "mono-green", seed=5)
    for _ in range(40):
        if e.is_over():
            break
        e.step(0)
    before = json.loads(e.view_json(0))
    p = e.player
    e.determinize(p, 99)
    after = json.loads(e.view_json(0))
    for b, a in zip(before["players"], after["players"]):
        assert b["hand_count"] == a["hand_count"]
        assert b["library_count"] == a["library_count"]
    play_out(e, seed=5)
    assert e.is_over()


def test_clone_is_independent():
    e = me.Engine("mono-red", "mono-green", seed=2)
    e.step(0)
    e.step(0)
    c = e.clone()
    play_out(c, seed=2)
    assert c.is_over()
    assert not e.is_over()
    assert len(e.history()) == 2


def test_describe_mentions_both_players():
    e = me.Engine("mono-red", "mono-green", seed=4)
    text = e.describe(0)
    assert "You (P1)" in text
    assert "Opponent (P2)" in text
    assert "hand:" in text


def test_agents_and_batch():
    e = me.Engine("mono-red", "mono-green", seed=8)
    greedy = me.Agent("greedy")
    while not e.is_over():
        e.step(greedy.choose(e))
    stats = me.simulate_batch("mono-red", "mono-green", games=20, seed=1)
    assert stats["games"] == 20
    assert stats["panics"] == 0
    assert sum(stats["wins"]) + stats["draws"] + stats["unfinished"] == 20


def test_card_table():
    n = me.card_count()
    assert n > 4000
    i = me.card_id("Lightning Bolt")
    assert i is not None and me.card_name(i) == "Lightning Bolt"
    assert "3 damage" in me.card_text("Lightning Bolt")
    with pytest.raises(IndexError):
        me.card_name(n + 1)
