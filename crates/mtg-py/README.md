# mtg-engine (Python)

Python bindings for the MTG rules engine: an `Engine` class stepped one
decision at a time, built-in agents, `simulate_batch` with the GIL released,
and `MtgEnv`, a PettingZoo AEC environment with exact action masks.

```
uv venv && uv pip install maturin pytest
uvx maturin develop --release -m crates/mtg-py/Cargo.toml
python -c "import mtg_engine as me; print(me.simulate_batch('mono-red', 'mono-green', games=200))"
pytest crates/mtg-py/python/tests
```
