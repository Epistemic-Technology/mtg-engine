//! Game review (design §9.4): a directory of `GameRecord` files with a
//! SQLite index over their metadata, and step snapshots produced by
//! re-simulating a record, which is deterministic, so nothing but the
//! record is stored.

#![allow(clippy::doc_markdown)] // SQLite, axum

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use mtg_core::card::CardPool;
use mtg_core::decision::StepResult;
use mtg_core::engine::Engine;
use mtg_core::ids::PlayerId;
use mtg_core::record::GameRecord;
use mtg_core::state::{GameResult, GameState};
use mtg_core::view::GameView;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

/// One row of the index.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameMeta {
    pub id: String,
    pub path: String,
    pub deck_a: String,
    pub deck_b: String,
    pub agent_a: String,
    pub agent_b: String,
    pub source: String,
    /// `P1`, `P2`, `draw`, or `unfinished`.
    pub result: String,
    pub turns: u32,
    pub actions: usize,
    pub seed: u64,
    /// Unix seconds when imported.
    pub imported: u64,
    pub tags: String,
}

/// The store: records on disk, metadata in `index.sqlite` beside them.
pub struct Store {
    dir: PathBuf,
    db: Mutex<Connection>,
}

fn meta_of(rec: &GameRecord, key: &str) -> String {
    rec.meta
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn result_text(r: Option<GameResult>) -> String {
    match r {
        Some(GameResult::Win { winner }) => format!("P{}", winner.0 + 1),
        Some(GameResult::Draw) => "draw".to_string(),
        None => "unfinished".to_string(),
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Store {
    /// Open (creating) the store in `dir`.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Store> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let db = Connection::open(dir.join("index.sqlite"))?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS games (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                deck_a TEXT, deck_b TEXT, agent_a TEXT, agent_b TEXT, source TEXT,
                result TEXT, turns INTEGER, actions INTEGER, seed INTEGER,
                imported INTEGER, tags TEXT
            );",
        )?;
        Ok(Store {
            dir,
            db: Mutex::new(db),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Index one record file (the id is the file stem).
    pub fn index_file(&self, path: &Path) -> Result<GameMeta> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let rec: GameRecord =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("game")
            .to_string();
        let meta = GameMeta {
            id,
            path: path.to_string_lossy().into_owned(),
            deck_a: meta_of(&rec, "deck_a"),
            deck_b: meta_of(&rec, "deck_b"),
            agent_a: meta_of(&rec, "agent_a"),
            agent_b: meta_of(&rec, "agent_b"),
            source: meta_of(&rec, "source"),
            result: result_text(rec.result),
            turns: rec.turns,
            actions: rec.actions.len(),
            seed: rec.seed,
            imported: now(),
            tags: meta_of(&rec, "tags"),
        };
        self.db.lock().expect("index").execute(
            "INSERT OR REPLACE INTO games
             (id, path, deck_a, deck_b, agent_a, agent_b, source, result, turns, actions, seed, imported, tags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                meta.id,
                meta.path,
                meta.deck_a,
                meta.deck_b,
                meta.agent_a,
                meta.agent_b,
                meta.source,
                meta.result,
                meta.turns,
                i64::try_from(meta.actions).unwrap_or(i64::MAX),
                i64::try_from(meta.seed).unwrap_or(i64::MAX),
                i64::try_from(meta.imported).unwrap_or(i64::MAX),
                meta.tags
            ],
        )?;
        Ok(meta)
    }

    /// Index every `*.json` under `dir` (not recursive). Returns how many.
    pub fn import(&self, dir: &Path) -> Result<usize> {
        let mut n = 0;
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        entries.sort();
        for p in entries {
            match self.index_file(&p) {
                Ok(_) => n += 1,
                Err(e) => tracing::warn!("skipping {}: {e:#}", p.display()),
            }
        }
        Ok(n)
    }

    /// Add a record to the store under `id` and index it.
    pub fn add(&self, id: &str, rec: &GameRecord) -> Result<GameMeta> {
        let path = self.dir.join(format!("{id}.json"));
        std::fs::write(&path, serde_json::to_string(rec)?)?;
        self.index_file(&path)
    }

    /// Games matching a substring filter on any text column, newest first.
    pub fn list(&self, filter: &str, limit: usize) -> Result<Vec<GameMeta>> {
        let db = self.db.lock().expect("index");
        let like = format!("%{filter}%");
        let mut stmt = db.prepare(
            "SELECT id, path, deck_a, deck_b, agent_a, agent_b, source, result, turns, actions, seed, imported, tags
             FROM games
             WHERE ?1 = '' OR id LIKE ?2 OR deck_a LIKE ?2 OR deck_b LIKE ?2 OR agent_a LIKE ?2
                OR agent_b LIKE ?2 OR source LIKE ?2 OR result LIKE ?2 OR tags LIKE ?2
             ORDER BY imported DESC, id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![filter, like, i64::try_from(limit).unwrap_or(i64::MAX)],
            |r| {
                Ok(GameMeta {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    deck_a: r.get(2)?,
                    deck_b: r.get(3)?,
                    agent_a: r.get(4)?,
                    agent_b: r.get(5)?,
                    source: r.get(6)?,
                    result: r.get(7)?,
                    turns: r.get(8)?,
                    actions: r
                        .get::<_, i64>(9)
                        .map(|n| usize::try_from(n).unwrap_or(0))?,
                    seed: r.get::<_, i64>(10).map(|n| u64::try_from(n).unwrap_or(0))?,
                    imported: r.get::<_, i64>(11).map(|n| u64::try_from(n).unwrap_or(0))?,
                    tags: r.get(12)?,
                })
            },
        )?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn get(&self, id: &str) -> Result<Option<(GameMeta, GameRecord)>> {
        let found = self.list("", usize::MAX)?.into_iter().find(|m| m.id == id);
        let Some(meta) = found else {
            return Ok(None);
        };
        let text = std::fs::read_to_string(&meta.path)?;
        let rec: GameRecord = serde_json::from_str(&text)?;
        Ok(Some((meta, rec)))
    }
}

/// What a reviewer sees at one step: the decision taken from this state
/// and the state itself (both hands unless redacted).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub index: usize,
    pub turn: u32,
    pub step: String,
    pub active_player: u8,
    pub priority: u8,
    /// The decision at this point, if the game is still running.
    pub decision: Option<SnapshotDecision>,
    /// Events that led to this state (since the previous snapshot).
    pub events: Vec<String>,
    pub view: GameView,
    /// The board as text for seat 1 (the §9.2 rendering).
    pub text: String,
    pub over: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotDecision {
    pub player: u8,
    pub kind: String,
    pub options: Vec<String>,
    /// The option the record took here (none at the last snapshot).
    pub chosen: Option<u32>,
}

fn full_view(state: &GameState, redact: Option<PlayerId>) -> GameView {
    let viewer = redact.unwrap_or(PlayerId(0));
    let mut view = state.observe(viewer);
    if redact.is_none() {
        let other = state.observe(PlayerId(1));
        if let (Some(p), Some(o)) = (view.players.get_mut(1), other.players.get(1)) {
            p.hand.clone_from(&o.hand);
        }
    }
    view
}

/// Re-simulate a record into one snapshot per recorded action plus the
/// final state. `redact` hides one seat's hidden information from the
/// other.
pub fn snapshots(
    pool: Arc<CardPool>,
    rec: &GameRecord,
    redact: Option<PlayerId>,
) -> Result<Vec<Snapshot>> {
    let config = rec
        .config(&pool, true)
        .context("the record names a card not in the corpus")?;
    let mut engine = Engine::new(pool, &config);
    let mut out = Vec::with_capacity(rec.actions.len() + 1);
    let mut cursor = 0usize;
    let capture = |engine: &Engine, index: usize, chosen: Option<u32>, cursor: &mut usize| {
        let state = engine.state();
        let viewer = redact.unwrap_or(PlayerId(0));
        let events: Vec<String> = state.log[*cursor..]
            .iter()
            .filter_map(|e| mtg_core::event_text::describe(state, e, viewer))
            .collect();
        *cursor = state.log.len();
        let (decision, over) = match engine.current() {
            StepResult::Decision(d) => (
                Some(SnapshotDecision {
                    player: d.player.0,
                    kind: format!("{:?}", d.kind),
                    options: d.options.iter().map(|c| c.describe(state)).collect(),
                    chosen,
                }),
                None,
            ),
            StepResult::Over(r) => (None, Some(result_text(Some(*r)))),
        };
        Snapshot {
            index,
            turn: state.turn_number,
            step: format!("{:?}", state.step),
            active_player: state.active_player.0,
            priority: state.priority.0,
            decision,
            events,
            view: full_view(state, redact),
            text: state.describe(viewer),
            over,
        }
    };
    for (i, a) in rec.actions.iter().enumerate() {
        out.push(capture(&engine, i, Some(*a), &mut cursor));
        engine
            .step(*a)
            .with_context(|| format!("action {i} rejected on replay"))?;
    }
    out.push(capture(&engine, rec.actions.len(), None, &mut cursor));
    Ok(out)
}

type CacheKey = (String, Option<u8>);

/// Snapshots cached per game id (and redaction).
#[derive(Default)]
pub struct SnapshotCache {
    inner: Mutex<HashMap<CacheKey, Arc<Vec<Snapshot>>>>,
}

impl SnapshotCache {
    pub fn get_or_build(
        &self,
        id: &str,
        redact: Option<PlayerId>,
        build: impl FnOnce() -> Result<Vec<Snapshot>>,
    ) -> Result<Arc<Vec<Snapshot>>> {
        let key = (id.to_string(), redact.map(|p| p.0));
        if let Some(v) = self.inner.lock().expect("cache").get(&key) {
            return Ok(v.clone());
        }
        let v = Arc::new(build()?);
        self.inner.lock().expect("cache").insert(key, v.clone());
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_indexes_and_snapshots_replay() {
        let pool = Arc::new(mtg_dsl::builtin_pool());
        let decks = [
            mtg_sim::decks::by_name(&pool, "mono-red").unwrap(),
            mtg_sim::decks::by_name(&pool, "mono-green").unwrap(),
        ];
        let config = mtg_core::engine::GameConfig {
            decks: decks.to_vec(),
            starting_life: 20,
            seed: 3,
            logging: false,
        };
        let mut agents: [Box<dyn mtg_agents::Agent>; 2] = [
            mtg_agents::by_name("random", 1).unwrap(),
            mtg_agents::by_name("greedy", 2).unwrap(),
        ];
        let summary = mtg_sim::play(&pool, &config, &mut agents);
        let dir = std::env::temp_dir().join(format!("mtg-review-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).unwrap();
        let mut rec = summary.record.clone();
        rec.meta
            .push(("deck_a".to_string(), "mono-red".to_string()));
        let meta = store.add("g1", &rec).unwrap();
        assert_eq!(meta.deck_a, "mono-red");
        assert_eq!(store.list("", 10).unwrap().len(), 1);
        assert_eq!(store.list("mono-red", 10).unwrap().len(), 1);
        assert_eq!(store.list("nothing", 10).unwrap().len(), 0);
        let (_, again) = store.get("g1").unwrap().unwrap();
        assert_eq!(again.actions, rec.actions);
        let snaps = snapshots(pool, &rec, None).unwrap();
        assert_eq!(snaps.len(), rec.actions.len() + 1);
        assert!(snaps.last().unwrap().over.is_some());
        assert_eq!(
            snaps[0].decision.as_ref().unwrap().chosen,
            Some(rec.actions[0])
        );
        // Both hands are visible to a reviewer.
        assert!(snaps[0].view.players[1].hand.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
