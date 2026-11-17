//! The on-disk store of recorded matches.
//!
//! A match is archived as the raw messages Arena logged, untouched, one JSON
//! object per line (`*.jsonl`, gzipped once the match is over). Translation
//! into engine-facing traces happens later from these files, so a translator
//! fix applies to every game already recorded.
//!
//! Archives hold the opponent's screen name and both players' account ids.
//! They live outside the repository and should not be committed.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FORMAT: u32 = 1;

/// One line of an archive.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Record {
    Header(Header),
    /// A message exactly as logged, with the logger line that preceded it.
    Msg {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hdr: Option<String>,
        data: Value,
    },
    End {
        reason: EndReason,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header {
    pub format: u32,
    pub recorder: String,
    pub match_id: String,
    /// Server timestamp of the message that opened the match, in
    /// milliseconds since the Unix epoch.
    pub started_ms: u64,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EndReason {
    /// The match service reported the match complete.
    Completed,
    /// The log ended or was replaced first (Arena quit or crashed).
    Interrupted,
}

/// A directory of match archives.
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

/// An archive file in the store.
#[derive(Clone, Debug)]
pub struct ArchiveFile {
    pub path: PathBuf,
    /// File name without `.jsonl` / `.jsonl.gz`: `<UTC start>_<match id>`.
    pub stem: String,
    /// Still being written (not yet gzipped).
    pub in_progress: bool,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Store { root: root.into() }
    }

    /// `~/Library/Application Support/mtg-engine/arena` on macOS, or
    /// `$XDG_DATA_HOME/mtg-engine/arena` elsewhere.
    pub fn default_root() -> Option<PathBuf> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let base = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
            PathBuf::from(x)
        } else {
            home.join(".local/share")
        };
        Some(base.join("mtg-engine/arena"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn matches_dir(&self) -> PathBuf {
        self.root.join("matches")
    }

    fn stem(match_id: &str, started_ms: u64) -> String {
        format!("{}_{match_id}", utc_stamp(started_ms))
    }

    fn open_path(&self, stem: &str) -> PathBuf {
        self.matches_dir().join(format!("{stem}.jsonl"))
    }

    fn done_path(&self, stem: &str) -> PathBuf {
        self.matches_dir().join(format!("{stem}.jsonl.gz"))
    }

    /// Whether a finished archive exists for this match segment.
    pub fn is_archived(&self, match_id: &str, started_ms: u64) -> bool {
        self.done_path(&Self::stem(match_id, started_ms)).exists()
    }

    /// Start (or restart) the archive for a match segment.
    pub fn create(&self, match_id: &str, started_ms: u64) -> io::Result<Writer> {
        fs::create_dir_all(self.matches_dir())?;
        let stem = Self::stem(match_id, started_ms);
        let path = self.open_path(&stem);
        let mut w = Writer {
            out: BufWriter::new(File::create(&path)?),
            path,
            done: self.done_path(&stem),
            stem,
            messages: 0,
        };
        w.write(&Record::Header(Header {
            format: FORMAT,
            recorder: env!("CARGO_PKG_VERSION").to_string(),
            match_id: match_id.to_string(),
            started_ms,
        }))?;
        Ok(w)
    }

    /// All archives, oldest first.
    pub fn list(&self) -> io::Result<Vec<ArchiveFile>> {
        let mut out = Vec::new();
        let dir = match fs::read_dir(self.matches_dir()) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e),
        };
        for entry in dir {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let (stem, in_progress) = if let Some(s) = name.strip_suffix(".jsonl.gz") {
                (s, false)
            } else if let Some(s) = name.strip_suffix(".jsonl") {
                (s, true)
            } else {
                continue;
            };
            out.push(ArchiveFile {
                stem: stem.to_string(),
                in_progress,
                path,
            });
        }
        out.sort_by(|a, b| a.stem.cmp(&b.stem));
        Ok(out)
    }

    /// Find an archive by a 1-based index into [`Store::list`] (negative
    /// counts from the newest: `-1` is the latest), by a substring of its
    /// file stem, or by path.
    pub fn find(&self, key: &str) -> io::Result<Option<ArchiveFile>> {
        let as_path = Path::new(key);
        if as_path.is_file() {
            let name = as_path.file_name().and_then(|n| n.to_str()).unwrap_or(key);
            let stem = name.trim_end_matches(".gz").trim_end_matches(".jsonl");
            return Ok(Some(ArchiveFile {
                path: as_path.to_path_buf(),
                stem: stem.to_string(),
                in_progress: as_path.extension().is_none_or(|e| e != "gz"),
            }));
        }
        let mut all = self.list()?;
        if let Ok(n) = key.parse::<isize>() {
            let idx = if n < 0 {
                all.len().checked_add_signed(n)
            } else {
                n.unsigned_abs().checked_sub(1)
            };
            return Ok(idx.filter(|i| *i < all.len()).map(|i| all.swap_remove(i)));
        }
        Ok(all.into_iter().rev().find(|a| a.stem.contains(key)))
    }
}

/// Appends records to an in-progress archive.
pub struct Writer {
    out: BufWriter<File>,
    path: PathBuf,
    done: PathBuf,
    stem: String,
    messages: usize,
}

impl Writer {
    fn write(&mut self, record: &Record) -> io::Result<()> {
        serde_json::to_writer(&mut self.out, record)?;
        self.out.write_all(b"\n")?;
        // Flushed per record so a crash loses at most the message in hand.
        self.out.flush()
    }

    pub fn message(&mut self, hdr: Option<String>, data: Value) -> io::Result<()> {
        self.messages += 1;
        self.write(&Record::Msg { hdr, data })
    }

    pub fn messages(&self) -> usize {
        self.messages
    }

    pub fn stem(&self) -> &str {
        &self.stem
    }

    /// Close the archive and compress it. Returns the final path.
    pub fn finish(mut self, reason: EndReason) -> io::Result<PathBuf> {
        self.write(&Record::End { reason })?;
        drop(self.out);
        let tmp = self.done.with_extension("gz.part");
        let mut gz = GzEncoder::new(File::create(&tmp)?, Compression::default());
        io::copy(&mut File::open(&self.path)?, &mut gz)?;
        gz.finish()?.sync_all()?;
        fs::rename(&tmp, &self.done)?;
        fs::remove_file(&self.path)?;
        Ok(self.done)
    }
}

/// Read every record of an archive (gzipped or not).
pub fn read(path: &Path) -> io::Result<Vec<Record>> {
    let file = File::open(path)?;
    let reader: Box<dyn BufRead> = if path.extension().is_some_and(|e| e == "gz") {
        Box::new(BufReader::new(GzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str(&line) {
            Ok(r) => out.push(r),
            // A torn final line in an in-progress file is expected.
            Err(e) if e.is_eof() => break,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(out)
}

/// `YYYYMMDD-HHMMSS` in UTC for a millisecond Unix timestamp.
pub fn utc_stamp(ms: u64) -> String {
    let secs = ms / 1000;
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    // Civil-from-days (Howard Hinnant's algorithm), valid for the Unix era.
    let z = secs / 86400 + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}-{h:02}{m:02}{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps() {
        assert_eq!(utc_stamp(0), "19700101-000000");
        assert_eq!(utc_stamp(951_782_400_000), "20000229-000000");
        assert_eq!(utc_stamp(1_794_882_313_630), "20261117-022513");
    }

    #[test]
    fn write_finish_read_find() {
        let root = std::env::temp_dir().join(format!("mtg-arena-store-{}", std::process::id()));
        let store = Store::new(&root);
        let mut w = store.create("abc", 1000).unwrap();
        w.message(Some("h".into()), serde_json::json!({"x": 1}))
            .unwrap();
        assert!(store.list().unwrap()[0].in_progress);
        assert!(!store.is_archived("abc", 1000));
        let path = w.finish(EndReason::Completed).unwrap();
        assert!(store.is_archived("abc", 1000));
        let records = read(&path).unwrap();
        assert_eq!(records.len(), 3);
        assert!(matches!(&records[1], Record::Msg { data, .. } if data["x"] == 1));
        assert!(matches!(
            records[2],
            Record::End {
                reason: EndReason::Completed
            }
        ));
        assert_eq!(
            store.find("-1").unwrap().unwrap().stem,
            "19700101-000001_abc"
        );
        assert_eq!(
            store.find("1").unwrap().unwrap().stem,
            "19700101-000001_abc"
        );
        assert!(store.find("2").unwrap().is_none());
        assert!(store.find("ab").unwrap().is_some());
        fs::remove_dir_all(&root).unwrap();
    }
}
