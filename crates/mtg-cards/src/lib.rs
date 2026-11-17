//! Card database: ingest of Scryfall and MTGJSON data, the pinned snapshot
//! lockfile, and pool statistics. See `design.md` §5.
//!
//! Scryfall is a development-time dependency only: `ingest` fetches bulk
//! files into a gitignored directory and records what it fetched in
//! `data/sources.lock`, which is committed. Nothing here runs at build time.

pub mod gate;
pub mod http;
pub mod ingest;
pub mod lock;
pub mod scryfall;
pub mod stats;

use std::path::{Path, PathBuf};

/// Where fetched data and the lockfile live, relative to the repository root.
#[derive(Debug, Clone)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lockfile(&self) -> PathBuf {
        self.root.join("sources.lock")
    }

    pub fn scryfall(&self) -> PathBuf {
        self.root.join("scryfall")
    }

    pub fn mtgjson(&self) -> PathBuf {
        self.root.join("mtgjson")
    }

    /// Path of a Scryfall bulk file by its bulk `type` (e.g. `oracle_cards`).
    pub fn scryfall_bulk(&self, bulk_type: &str) -> PathBuf {
        self.scryfall().join(format!("{bulk_type}.jsonl.gz"))
    }

    /// Path of an MTGJSON file by its file name (e.g. `Keywords.json`).
    pub fn mtgjson_file(&self, name: &str) -> PathBuf {
        self.mtgjson().join(name)
    }
}
