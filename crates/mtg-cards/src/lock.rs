//! The committed lockfile that pins exactly which Scryfall and MTGJSON
//! snapshot the corpus was authored and verified against.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Lockfile {
    /// Lockfile schema version.
    pub version: u32,
    /// When `ingest` last wrote this file (RFC 3339, UTC).
    pub fetched_at: String,
    /// Scryfall bulk files, keyed by bulk `type` (`oracle_cards`, `rulings`).
    pub scryfall: BTreeMap<String, ScryfallEntry>,
    /// MTGJSON files, keyed by file name (`Keywords.json`).
    pub mtgjson: Mtgjson,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScryfallEntry {
    /// Scryfall's `updated_at` for the bulk file.
    pub updated_at: String,
    /// The `jsonl_download_uri` that was fetched.
    pub uri: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Mtgjson {
    /// MTGJSON build version from `Meta.json`, e.g. `5.3.0+20261028`.
    pub version: String,
    pub files: BTreeMap<String, MtgjsonEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MtgjsonEntry {
    pub uri: String,
    pub sha256: String,
    pub size: u64,
}

impl Lockfile {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn read(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let lock: Self =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        anyhow::ensure!(
            lock.version == Self::SCHEMA_VERSION,
            "{}: unsupported lockfile version {} (expected {})",
            path.display(),
            lock.version,
            Self::SCHEMA_VERSION
        );
        Ok(lock)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }
}
