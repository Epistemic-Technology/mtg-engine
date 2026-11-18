//! Game records: a game is fully determined by its configuration, decks,
//! seed, and action history (design §4.8).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::card::CardPool;
use crate::decision::ChoiceId;
use crate::engine::{Engine, EngineError, GameConfig};
use crate::state::GameResult;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameRecord {
    pub engine_version: String,
    pub seed: u64,
    pub starting_life: i32,
    /// Decks as card names, one list per player.
    pub decks: Vec<Vec<String>>,
    pub actions: Vec<ChoiceId>,
    pub result: Option<GameResult>,
    pub turns: u32,
    /// Free-form labels: agent names, tags.
    #[serde(default)]
    pub meta: Vec<(String, String)>,
    /// What a model said and spent at each decision it made (the
    /// benchmark harness, design §9.5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<DecisionNote>,
}

/// One model decision, kept with the record.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DecisionNote {
    /// Index into `actions`.
    pub action: usize,
    pub player: u8,
    /// The model's reasoning text, as it replied.
    pub reasoning: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub latency_ms: u64,
    /// Replies that named no legal option before this one was accepted.
    pub invalid: u32,
    /// No legal reply came; the fallback agent chose.
    pub fallback: bool,
}

impl GameRecord {
    pub fn from_engine(engine: &Engine, pool: &CardPool, config: &GameConfig) -> GameRecord {
        GameRecord {
            engine_version: crate::ENGINE_VERSION.to_string(),
            seed: config.seed,
            starting_life: config.starting_life,
            decks: config
                .decks
                .iter()
                .map(|d| d.iter().map(|r| pool.get(*r).name.clone()).collect())
                .collect(),
            actions: engine.history().to_vec(),
            result: engine.state().result,
            turns: engine.state().turn_number,
            meta: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn config(&self, pool: &CardPool, logging: bool) -> Option<GameConfig> {
        let decks = self
            .decks
            .iter()
            .map(|d| d.iter().map(|n| pool.lookup(n)).collect::<Option<Vec<_>>>())
            .collect::<Option<Vec<_>>>()?;
        Some(GameConfig {
            decks,
            starting_life: self.starting_life,
            seed: self.seed,
            logging,
        })
    }

    /// Re-simulate the record. Returns the engine positioned after the last
    /// recorded action.
    pub fn replay(&self, pool: Arc<CardPool>, logging: bool) -> Result<Engine, ReplayError> {
        let config = self
            .config(&pool, logging)
            .ok_or(ReplayError::UnknownCard)?;
        let mut engine = Engine::new(pool, &config);
        for (i, a) in self.actions.iter().enumerate() {
            engine
                .step(*a)
                .map_err(|e| ReplayError::Engine { at: i, source: e })?;
        }
        Ok(engine)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("record names a card that is not in the pool")]
    UnknownCard,
    #[error("action {at} rejected: {source}")]
    Engine { at: usize, source: EngineError },
}
