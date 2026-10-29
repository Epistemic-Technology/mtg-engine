//! Scryfall API types: the bulk-data index and the subset of the card object
//! this crate needs. Field names follow <https://scryfall.com/docs/api/cards>.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde::Deserialize;

pub const API_BASE: &str = "https://api.scryfall.com";

/// Bulk file types this project consumes.
pub const BULK_TYPES: &[&str] = &["oracle_cards", "rulings"];

/// Formats whose pool sizes `stats` reports. Order is the reporting order.
pub const FORMATS: &[&str] = &[
    "standard",
    "pioneer",
    "modern",
    "legacy",
    "vintage",
    "pauper",
    "commander",
];

#[derive(Debug, Deserialize)]
pub struct BulkIndex {
    pub data: Vec<BulkEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BulkEntry {
    #[serde(rename = "type")]
    pub bulk_type: String,
    pub updated_at: String,
    pub jsonl_download_uri: String,
    pub compressed_size: u64,
}

/// The subset of a Scryfall card object needed for statistics and the
/// corpus loader. Unknown fields are ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct Card {
    pub object: String,
    pub oracle_id: Option<String>,
    pub name: String,
    pub layout: String,
    #[serde(default)]
    pub set_type: String,
    #[serde(default)]
    pub type_line: Option<String>,
    #[serde(default)]
    pub legalities: BTreeMap<String, String>,
    #[serde(default)]
    pub games: Vec<String>,
}

impl Card {
    /// True if the card is `legal` or `restricted` in `format`, which is what
    /// Scryfall's `f:<format>` search counts.
    pub fn playable_in(&self, format: &str) -> bool {
        matches!(
            self.legalities.get(format).map(String::as_str),
            Some("legal" | "restricted")
        )
    }
}

/// Iterate the card objects in a gzipped JSONL bulk file.
pub fn read_cards(path: &Path) -> Result<impl Iterator<Item = Result<Card>>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(GzDecoder::new(file));
    Ok(reader.lines().enumerate().map(|(i, line)| {
        let line = line.with_context(|| format!("reading line {}", i + 1))?;
        serde_json::from_str::<Card>(&line)
            .with_context(|| format!("parsing card on line {}", i + 1))
    }))
}

#[derive(Debug, Deserialize)]
pub struct SearchPage {
    pub total_cards: u64,
}

pub fn search_url(query: &str) -> String {
    // Scryfall defaults to unique=cards (one result per Oracle name).
    let q: String = query
        .chars()
        .map(|c| match c {
            ':' => "%3A".to_string(),
            ' ' => "+".to_string(),
            c => c.to_string(),
        })
        .collect();
    format!("{API_BASE}/cards/search?q={q}")
}
