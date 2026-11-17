#![allow(clippy::doc_markdown)] // PettingZoo, NumPy
//! Python bindings (design §9.1): a thin `Engine` class over `mtg_core`,
//! baseline agents, batch simulation with the GIL released, and the
//! fixed-size encoders the PettingZoo environment in `python/` uses.

mod encode;

use std::sync::{Arc, OnceLock};

use mtg_core::card::{CardPool, CardRef};
use mtg_core::decision::{Decision, StepResult};
use mtg_core::engine::{Engine, GameConfig};
use mtg_core::ids::PlayerId;
use mtg_core::record::GameRecord;
use mtg_core::state::GameResult;
use numpy::{IntoPyArray, PyArray1, PyArray2, PyArrayMethods as _};
use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// The built-in card corpus, compiled once per process.
fn pool() -> &'static Arc<CardPool> {
    static POOL: OnceLock<Arc<CardPool>> = OnceLock::new();
    POOL.get_or_init(|| Arc::new(mtg_dsl::builtin_pool()))
}

/// A deck from a built-in deck name (`mono-red`, `random:7`, ...) or a
/// list of card names.
fn deck_from(obj: &Bound<'_, PyAny>, seed: u64) -> PyResult<Vec<CardRef>> {
    if let Ok(name) = obj.extract::<String>() {
        return mtg_sim::decks::by_name_seeded(pool(), &name, seed)
            .ok_or_else(|| PyValueError::new_err(format!("unknown deck {name}")));
    }
    let names: Vec<String> = obj
        .extract()
        .map_err(|_| PyValueError::new_err("a deck is a deck name or a list of card names"))?;
    names
        .iter()
        .map(|n| {
            pool()
                .lookup(n)
                .ok_or_else(|| PyValueError::new_err(format!("unknown card {n}")))
        })
        .collect()
}

fn player(p: u8) -> PyResult<PlayerId> {
    if p < 2 {
        Ok(PlayerId(p))
    } else {
        Err(PyValueError::new_err("player must be 0 or 1"))
    }
}

/// One game, stepped one decision at a time.
#[pyclass(name = "Engine", unsendable)]
struct PyEngine {
    engine: Engine,
    config: GameConfig,
}

impl PyEngine {
    fn decision(&self) -> PyResult<&Decision> {
        match self.engine.current() {
            StepResult::Decision(d) => Ok(d),
            StepResult::Over(_) => Err(PyRuntimeError::new_err("the game is over")),
        }
    }
}

#[pymethods]
impl PyEngine {
    /// Start a game between two decks (deck names or lists of card names).
    #[new]
    #[pyo3(signature = (deck_a, deck_b, seed = 0, logging = false, starting_life = 20))]
    fn new(
        deck_a: &Bound<'_, PyAny>,
        deck_b: &Bound<'_, PyAny>,
        seed: u64,
        logging: bool,
        starting_life: i32,
    ) -> PyResult<Self> {
        let config = GameConfig {
            decks: vec![
                deck_from(deck_a, seed)?,
                deck_from(deck_b, seed ^ 0x9e37_79b9)?,
            ],
            starting_life,
            seed,
            logging,
        };
        let engine = Engine::new(pool().clone(), &config);
        Ok(PyEngine { engine, config })
    }

    /// Re-simulate a game record (JSON) to its last recorded action.
    #[staticmethod]
    fn from_record_json(text: &str, logging: bool) -> PyResult<Self> {
        let record: GameRecord =
            serde_json::from_str(text).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let config = record
            .config(pool(), logging)
            .ok_or_else(|| PyValueError::new_err("the record names a card not in the corpus"))?;
        let engine = record
            .replay(pool().clone(), logging)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(PyEngine { engine, config })
    }

    fn is_over(&self) -> bool {
        self.engine.is_over()
    }

    /// `"P1"`, `"P2"`, `"draw"`, or `None` while the game runs.
    fn result(&self) -> Option<String> {
        match self.engine.current() {
            StepResult::Over(GameResult::Win { winner }) => Some(format!("P{}", winner.0 + 1)),
            StepResult::Over(GameResult::Draw) => Some("draw".to_string()),
            StepResult::Decision(_) => None,
        }
    }

    /// The winner's seat (0 or 1), or `None` for a draw or a running game.
    fn winner(&self) -> Option<u8> {
        match self.engine.current() {
            StepResult::Over(GameResult::Win { winner }) => Some(winner.0),
            _ => None,
        }
    }

    #[getter]
    fn turn(&self) -> u32 {
        self.engine.state().turn_number
    }

    #[getter]
    fn step_name(&self) -> String {
        format!("{:?}", self.engine.state().step)
    }

    #[getter]
    fn active_player(&self) -> u8 {
        self.engine.state().active_player.0
    }

    /// The seat that must act now.
    #[getter]
    fn player(&self) -> PyResult<u8> {
        Ok(self.decision()?.player.0)
    }

    #[getter]
    fn decision_kind(&self) -> PyResult<&'static str> {
        Ok(encode::decision_kind_name(self.decision()?.kind))
    }

    fn num_options(&self) -> PyResult<usize> {
        Ok(self.decision()?.options.len())
    }

    /// The legal options, described.
    fn options(&self) -> PyResult<Vec<String>> {
        let d = self.decision()?;
        Ok(d.options
            .iter()
            .map(|c| c.describe(self.engine.state()))
            .collect())
    }

    /// Apply option `choice` of the current decision.
    fn step(&mut self, choice: u32) -> PyResult<()> {
        self.engine
            .step(choice)
            .map(|_| ())
            .map_err(|e| PyIndexError::new_err(e.to_string()))
    }

    /// The game as text for `player`, with `#n` labels.
    fn describe(&self, player: u8) -> PyResult<String> {
        Ok(self.engine.state().describe(self::player(player)?))
    }

    /// The player's view as JSON.
    fn view_json(&self, player: u8) -> PyResult<String> {
        let v = self.engine.state().observe(self::player(player)?);
        serde_json::to_string(&v).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Fixed-size observation for `player` (see `OBS_SIZE`).
    fn observation<'py>(&self, py: Python<'py>, player: u8) -> PyResult<Bound<'py, PyArray1<f32>>> {
        let d = match self.engine.current() {
            StepResult::Decision(d) => Some(d),
            StepResult::Over(_) => None,
        };
        let v = encode::observation(self.engine.state(), d, self::player(player)?);
        Ok(v.into_pyarray(py))
    }

    /// Per-option features, `max_options × OPT_SIZE`; rows past the live
    /// options are zero.
    fn option_features<'py>(
        &self,
        py: Python<'py>,
        max_options: usize,
    ) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let (v, _) = match self.engine.current() {
            StepResult::Decision(d) => encode::option_features(self.engine.state(), d, max_options),
            StepResult::Over(_) => (vec![0.0; max_options * encode::OPT_F], vec![0; max_options]),
        };
        v.into_pyarray(py)
            .reshape([max_options, encode::OPT_F])
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// 1 for each live option among the first `max_options`.
    fn action_mask<'py>(&self, py: Python<'py>, max_options: usize) -> Bound<'py, PyArray1<i8>> {
        let mask = match self.engine.current() {
            StepResult::Decision(d) => {
                let mut m = vec![0i8; max_options];
                for x in m.iter_mut().take(d.options.len()) {
                    *x = 1;
                }
                m
            }
            StepResult::Over(_) => vec![0; max_options],
        };
        mask.into_pyarray(py)
    }

    /// An independent copy of the game.
    #[allow(clippy::should_implement_trait)]
    fn clone(&self) -> PyEngine {
        PyEngine {
            engine: self.engine.clone(),
            config: self.config.clone(),
        }
    }

    /// Resample what `player` cannot see (design §4.8).
    fn determinize(&mut self, player: u8, seed: u64) -> PyResult<()> {
        self.engine.determinize(self::player(player)?, seed);
        Ok(())
    }

    /// The game record (JSON): decks, seed, and the actions so far.
    fn record_json(&self) -> PyResult<String> {
        let r = GameRecord::from_engine(&self.engine, pool(), &self.config);
        serde_json::to_string(&r).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn history(&self) -> Vec<u32> {
        self.engine.history().to_vec()
    }

    /// Events since index `since` (needs `logging=True`), as text, and the
    /// new cursor.
    fn events(&self, since: usize) -> (Vec<String>, usize) {
        let log = &self.engine.state().log;
        let from = since.min(log.len());
        (
            log[from..].iter().map(|e| format!("{e:?}")).collect(),
            log.len(),
        )
    }
}

/// A built-in agent (`random`, `greedy`, `first`).
#[pyclass(name = "Agent", unsendable)]
struct PyAgent {
    inner: Box<dyn mtg_agents::Agent>,
}

#[pymethods]
impl PyAgent {
    #[new]
    #[pyo3(signature = (name = "random", seed = 0))]
    fn new(name: &str, seed: u64) -> PyResult<Self> {
        let inner = mtg_agents::by_name(name, seed)
            .ok_or_else(|| PyValueError::new_err(format!("unknown agent {name}")))?;
        Ok(PyAgent { inner })
    }

    /// The option this agent picks for the engine's current decision.
    fn choose(&mut self, engine: &PyEngine) -> PyResult<u32> {
        let d = engine.decision()?;
        Ok(self.inner.choose(engine.engine.state(), d))
    }
}

/// Play `games` games between two built-in agents on rayon threads with
/// the GIL released. Returns a dict of counts and rates.
#[pyfunction]
#[pyo3(signature = (deck_a, deck_b, games = 100, seed = 0, agent_a = "random", agent_b = "random"))]
fn simulate_batch<'py>(
    py: Python<'py>,
    deck_a: &Bound<'py, PyAny>,
    deck_b: &Bound<'py, PyAny>,
    games: usize,
    seed: u64,
    agent_a: &str,
    agent_b: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let decks = [
        deck_from(deck_a, seed)?,
        deck_from(deck_b, seed ^ 0x9e37_79b9)?,
    ];
    for a in [agent_a, agent_b] {
        if mtg_agents::by_name(a, 0).is_none() {
            return Err(PyValueError::new_err(format!("unknown agent {a}")));
        }
    }
    let (a, b) = (agent_a.to_string(), agent_b.to_string());
    let stats = py.detach(|| mtg_sim::batch(pool(), &decks, games, seed, false, [&a, &b]));
    let d = PyDict::new(py);
    d.set_item("games", stats.games)?;
    d.set_item("wins", (stats.wins[0], stats.wins[1]))?;
    d.set_item("draws", stats.draws)?;
    d.set_item("unfinished", stats.unfinished)?;
    d.set_item("panics", stats.panics.len())?;
    d.set_item("mean_turns", stats.mean_turns())?;
    d.set_item("mean_decisions", stats.mean_decisions())?;
    d.set_item("games_per_second", stats.games_per_second_wall())?;
    d.set_item(
        "decisions_per_second",
        stats.mean_decisions() * stats.games_per_second_wall(),
    )?;
    d.set_item("wall_seconds", stats.wall.as_secs_f64())?;
    Ok(d)
}

/// Number of cards in the corpus; card ids run `0..card_count()`.
#[pyfunction]
fn card_count() -> usize {
    pool().len()
}

#[pyfunction]
fn card_name(id: u32) -> PyResult<String> {
    if (id as usize) < pool().len() {
        Ok(pool().get(CardRef(id)).name.clone())
    } else {
        Err(PyIndexError::new_err("no such card"))
    }
}

#[pyfunction]
fn card_id(name: &str) -> Option<u32> {
    pool().lookup(name).map(|r| r.0)
}

/// The Oracle text of a card.
#[pyfunction]
fn card_text(name: &str) -> Option<String> {
    pool()
        .lookup(name)
        .map(|r| pool().get(r).oracle_text.clone())
}

#[pymodule]
#[allow(clippy::used_underscore_items)] // the module's Python name
fn _mtg_engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyEngine>()?;
    m.add_class::<PyAgent>()?;
    m.add_function(wrap_pyfunction!(simulate_batch, m)?)?;
    m.add_function(wrap_pyfunction!(card_count, m)?)?;
    m.add_function(wrap_pyfunction!(card_name, m)?)?;
    m.add_function(wrap_pyfunction!(card_id, m)?)?;
    m.add_function(wrap_pyfunction!(card_text, m)?)?;
    m.add("OBS_SIZE", encode::OBS_SIZE)?;
    m.add("OPT_SIZE", encode::OPT_F)?;
    m.add("CARD_FEATURES", encode::CARD_F)?;
    m.add("DECISION_KINDS", encode::DECISION_KIND_NAMES.to_vec())?;
    m.add("CHOICE_KINDS", encode::CHOICE_KIND_NAMES.to_vec())?;
    m.add("ENGINE_VERSION", mtg_core::ENGINE_VERSION)?;
    Ok(())
}
