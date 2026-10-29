//! `ingest`: fetch Scryfall bulk files and MTGJSON enumerations into the data
//! directory and record them in the lockfile.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::DataDir;
use crate::http::{Client, sha256_file};
use crate::lock::{Lockfile, Mtgjson, MtgjsonEntry, ScryfallEntry};
use crate::scryfall::{API_BASE, BULK_TYPES, BulkIndex};

pub const MTGJSON_BASE: &str = "https://mtgjson.com/api/v5";
pub const MTGJSON_FILES: &[&str] = &["Keywords.json", "CardTypes.json", "EnumValues.json"];

#[derive(Debug, Deserialize)]
struct MtgjsonMeta {
    meta: MtgjsonMetaInner,
}

#[derive(Debug, Deserialize)]
struct MtgjsonMetaInner {
    version: String,
}

/// Fetch the latest data and rewrite the lockfile.
pub fn ingest_latest(data: &DataDir) -> Result<Lockfile> {
    let client = Client::new()?;
    let mut lock = Lockfile {
        version: Lockfile::SCHEMA_VERSION,
        fetched_at: now_rfc3339(),
        ..Lockfile::default()
    };

    let index_text = client.get_text(&format!("{API_BASE}/bulk-data"))?;
    let index: BulkIndex = serde_json::from_str(&index_text).context("parsing bulk-data index")?;
    for bulk_type in BULK_TYPES {
        let entry = index
            .data
            .iter()
            .find(|e| e.bulk_type == *bulk_type)
            .with_context(|| format!("bulk-data index has no `{bulk_type}` entry"))?;
        let dest = data.scryfall_bulk(bulk_type);
        eprintln!(
            "fetching scryfall {bulk_type} ({:.1} MB) -> {}",
            megabytes(entry.compressed_size),
            dest.display()
        );
        let dl = client.download(&entry.jsonl_download_uri, &dest)?;
        lock.scryfall.insert(
            (*bulk_type).to_string(),
            ScryfallEntry {
                updated_at: entry.updated_at.clone(),
                uri: entry.jsonl_download_uri.clone(),
                sha256: dl.sha256,
                size: dl.size,
            },
        );
    }

    let meta_text = client.get_text(&format!("{MTGJSON_BASE}/Meta.json"))?;
    let meta: MtgjsonMeta =
        serde_json::from_str(&meta_text).context("parsing MTGJSON Meta.json")?;
    let mut mtgjson = Mtgjson {
        version: meta.meta.version,
        files: BTreeMap::default(),
    };
    for name in MTGJSON_FILES {
        let uri = format!("{MTGJSON_BASE}/{name}");
        let dest = data.mtgjson_file(name);
        eprintln!("fetching mtgjson {name} -> {}", dest.display());
        let dl = client.download(&uri, &dest)?;
        mtgjson.files.insert(
            (*name).to_string(),
            MtgjsonEntry {
                uri,
                sha256: dl.sha256,
                size: dl.size,
            },
        );
    }
    lock.mtgjson = mtgjson;

    lock.write(&data.lockfile())?;
    eprintln!("wrote {}", data.lockfile().display());
    Ok(lock)
}

/// Fetch exactly what the lockfile names and verify hashes. Files already
/// present with the right hash are not re-downloaded.
pub fn ingest_locked(data: &DataDir) -> Result<Lockfile> {
    let lock = Lockfile::read(&data.lockfile())?;
    let client = Client::new()?;

    for (bulk_type, entry) in &lock.scryfall {
        let dest = data.scryfall_bulk(bulk_type);
        fetch_verified(&client, &entry.uri, &dest, &entry.sha256)?;
    }
    for (name, entry) in &lock.mtgjson.files {
        let dest = data.mtgjson_file(name);
        fetch_verified(&client, &entry.uri, &dest, &entry.sha256)?;
    }
    Ok(lock)
}

/// Verify that every file in the lockfile is present on disk with the right
/// hash. No network.
pub fn verify(data: &DataDir) -> Result<Lockfile> {
    let lock = Lockfile::read(&data.lockfile())?;
    for (bulk_type, entry) in &lock.scryfall {
        check_hash(&data.scryfall_bulk(bulk_type), &entry.sha256)?;
    }
    for (name, entry) in &lock.mtgjson.files {
        check_hash(&data.mtgjson_file(name), &entry.sha256)?;
    }
    Ok(lock)
}

fn fetch_verified(client: &Client, uri: &str, dest: &std::path::Path, want: &str) -> Result<()> {
    if dest.exists() && sha256_file(dest)?.sha256 == want {
        eprintln!("ok      {}", dest.display());
        return Ok(());
    }
    eprintln!("fetching {uri} -> {}", dest.display());
    let dl = client.download(uri, dest)?;
    if dl.sha256 != want {
        bail!(
            "{}: sha256 mismatch after download (lockfile {want}, got {})",
            dest.display(),
            dl.sha256
        );
    }
    Ok(())
}

fn check_hash(path: &std::path::Path, want: &str) -> Result<()> {
    if !path.exists() {
        bail!(
            "{} is missing; run `mtg-cards ingest --locked`",
            path.display()
        );
    }
    let got = sha256_file(path)?.sha256;
    if got != want {
        bail!(
            "{}: sha256 mismatch (lockfile {want}, on disk {got}); run `mtg-cards ingest --locked`",
            path.display()
        );
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn megabytes(bytes: u64) -> f64 {
    bytes as f64 / 1e6
}

fn now_rfc3339() -> String {
    // Avoid a chrono dependency for one timestamp: civil-from-days algorithm
    // (Howard Hinnant), UTC, second precision.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::now_rfc3339;

    #[test]
    fn timestamp_shape() {
        let t = now_rfc3339();
        assert_eq!(t.len(), 20, "{t}");
        assert!(t.ends_with('Z'));
        assert_eq!(&t[4..5], "-");
        assert_eq!(&t[10..11], "T");
    }
}
