//! `stats`: pool sizes from the local snapshot, optionally checked against
//! live Scryfall search totals. The M0 acceptance test is that the local
//! counts match Scryfall within 1%.

use std::collections::BTreeMap;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::DataDir;
use crate::http::Client;
use crate::scryfall::{FORMATS, SearchPage, read_cards, search_url};

#[derive(Debug, Default, Clone)]
pub struct PoolStats {
    /// Every object in the `oracle_cards` file (includes tokens, emblems, etc.).
    pub objects: u64,
    /// Objects with the `card` object type and an `oracle_id`.
    pub cards: u64,
    pub by_layout: BTreeMap<String, u64>,
    pub by_format: BTreeMap<String, u64>,
}

pub fn compute(data: &DataDir) -> Result<PoolStats> {
    let path = data.scryfall_bulk("oracle_cards");
    let mut stats = PoolStats::default();
    for card in read_cards(&path)? {
        let card = card?;
        stats.objects += 1;
        if card.object != "card" || card.oracle_id.is_none() {
            continue;
        }
        stats.cards += 1;
        *stats.by_layout.entry(card.layout.clone()).or_default() += 1;
        for format in FORMATS {
            if card.playable_in(format) {
                *stats.by_format.entry((*format).to_string()).or_default() += 1;
            }
        }
    }
    Ok(stats)
}

/// Live `total_cards` for `f:<format>` for each format, respecting
/// Scryfall's 2 req/s limit on `/cards/search`.
pub fn live_format_totals() -> Result<BTreeMap<String, u64>> {
    let client = Client::new()?;
    let mut out = BTreeMap::new();
    for (i, format) in FORMATS.iter().enumerate() {
        if i > 0 {
            sleep(Duration::from_millis(550));
        }
        let url = search_url(&format!("f:{format}"));
        let text = client.get_text(&url)?;
        let page: SearchPage = serde_json::from_str(&text)
            .with_context(|| format!("parsing search page for {format}"))?;
        out.insert((*format).to_string(), page.total_cards);
    }
    Ok(out)
}

/// One row of a local-vs-live comparison.
#[derive(Debug, Clone)]
pub struct Comparison {
    pub format: String,
    pub local: u64,
    pub live: u64,
    pub rel_diff: f64,
    pub ok: bool,
}

pub const TOLERANCE: f64 = 0.01;

/// `|local - live| / live`, or infinity when `live` is zero.
#[allow(clippy::cast_precision_loss)]
fn relative_difference(local: u64, live: u64) -> f64 {
    if live == 0 {
        f64::INFINITY
    } else {
        (local as f64 - live as f64).abs() / live as f64
    }
}

pub fn compare(local: &PoolStats, live: &BTreeMap<String, u64>) -> Vec<Comparison> {
    FORMATS
        .iter()
        .map(|format| {
            let l = local.by_format.get(*format).copied().unwrap_or(0);
            let r = live.get(*format).copied().unwrap_or(0);
            let rel_diff = relative_difference(l, r);
            Comparison {
                format: (*format).to_string(),
                local: l,
                live: r,
                rel_diff,
                ok: rel_diff <= TOLERANCE,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_flags_out_of_tolerance() {
        let mut local = PoolStats::default();
        local.by_format.insert("standard".into(), 5000);
        local.by_format.insert("pauper".into(), 10_000);
        let mut live = BTreeMap::new();
        live.insert("standard".to_string(), 5040); // 0.8% off
        live.insert("pauper".to_string(), 10_200); // 2% off
        let rows = compare(&local, &live);
        let std = rows.iter().find(|r| r.format == "standard").unwrap();
        let pau = rows.iter().find(|r| r.format == "pauper").unwrap();
        assert!(std.ok);
        assert!(!pau.ok);
    }
}
