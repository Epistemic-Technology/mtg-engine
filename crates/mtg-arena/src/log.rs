//! Reading Arena's `Player.log`: pulling the JSON messages out of the Unity
//! log text, and following the file as Arena appends to it.
//!
//! With "Detailed Logs (Plugin Support)" enabled, Arena logs every message
//! exchanged with the match service. Each is preceded by a logger line such
//! as `[UnityCrossThreadLogger]11/16/2026 9:25:13 PM: Match to <user>:
//! GreToClientEvent`; the JSON follows either on one line (server to client)
//! or pretty-printed over many (client to server).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

const LOGGER_PREFIX: &str = "[UnityCrossThreadLogger]";
/// A JSON blob that grows past this is assumed to be a parsing mistake.
const MAX_BLOB: usize = 64 << 20;

/// One JSON message from the log, with the logger line that introduced it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub header: Option<String>,
    pub json: Value,
}

/// Incremental line-oriented extractor of JSON messages.
#[derive(Default)]
pub struct Parser {
    header: Option<String>,
    blob: Option<String>,
    /// Whether the log said detailed logging is on (`None` until it says).
    detailed: Option<bool>,
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Arena prints `DETAILED LOGS: ENABLED` or `DISABLED` near the top of
    /// each log. Without them there are no match messages to record.
    pub fn detailed_logs(&self) -> Option<bool> {
        self.detailed
    }

    /// Feed one line (without its newline). Returns a message if the line
    /// completed one.
    pub fn feed(&mut self, line: &str) -> Option<Entry> {
        if let Some(blob) = &mut self.blob {
            if line.starts_with(LOGGER_PREFIX) || blob.len() > MAX_BLOB {
                // The blob never closed; give up on it and treat this line
                // as an ordinary one.
                self.blob = None;
            } else {
                blob.push('\n');
                blob.push_str(line);
                if line == "}"
                    && let Ok(json) = serde_json::from_str(blob)
                {
                    self.blob = None;
                    return Some(self.emit(json));
                }
                return None;
            }
        }
        if line.starts_with('{') {
            match serde_json::from_str(line) {
                Ok(json) => return Some(self.emit(json)),
                Err(e) if e.is_eof() => self.blob = Some(line.to_string()),
                Err(_) => {}
            }
        } else if line.starts_with(LOGGER_PREFIX) {
            self.header = Some(line.to_string());
        } else if let Some(rest) = line.strip_prefix("DETAILED LOGS: ") {
            self.detailed = Some(rest.trim() == "ENABLED");
        }
        None
    }

    fn emit(&mut self, json: Value) -> Entry {
        Entry {
            header: self.header.take(),
            json,
        }
    }
}

/// What a [`Tailer::poll`] found.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Poll {
    /// Read whatever was appended (possibly nothing).
    Appended,
    /// The log was replaced or truncated: Arena restarted. Lines delivered
    /// by this poll belong to the old file; the next poll reads the new one
    /// from its start.
    Rotated,
    /// The log file does not exist (yet).
    Missing,
}

/// Follows a log file by polling, surviving Arena's rotation on launch
/// (`Player.log` is renamed to `Player-prev.log` and a new file started).
pub struct Tailer {
    path: PathBuf,
    file: Option<File>,
    id: u64,
    pos: u64,
    partial: Vec<u8>,
}

impl Tailer {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Tailer {
            path: path.into(),
            file: None,
            id: 0,
            pos: 0,
            partial: Vec::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read newly appended complete lines, passing each to `on_line`.
    pub fn poll(&mut self, mut on_line: impl FnMut(&str)) -> io::Result<Poll> {
        let meta = match std::fs::metadata(&self.path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(if self.reset(&mut on_line)? {
                    Poll::Rotated
                } else {
                    Poll::Missing
                });
            }
            Err(e) => return Err(e),
        };
        if self.file.is_some() && (file_id(&meta) != self.id || meta.len() < self.pos) {
            self.reset(&mut on_line)?;
            return Ok(Poll::Rotated);
        }
        if self.file.is_none() {
            self.file = Some(File::open(&self.path)?);
            self.id = file_id(&meta);
            self.pos = 0;
        }
        self.drain(&mut on_line)?;
        Ok(Poll::Appended)
    }

    /// Drop the current file after reading what is left of it (a renamed
    /// file is still readable through the open handle). Returns whether a
    /// file was open.
    fn reset(&mut self, on_line: &mut impl FnMut(&str)) -> io::Result<bool> {
        if self.file.is_none() {
            return Ok(false);
        }
        let drained = self.drain(on_line);
        self.file = None;
        self.pos = 0;
        self.partial.clear();
        drained.map(|()| true)
    }

    fn drain(&mut self, on_line: &mut impl FnMut(&str)) -> io::Result<()> {
        let Some(file) = &mut self.file else {
            return Ok(());
        };
        let len = file.metadata()?.len();
        if len < self.pos {
            // Truncated in place; the old tail is gone.
            return Ok(());
        }
        file.seek(SeekFrom::Start(self.pos))?;
        let mut chunk = vec![0u8; 1 << 16];
        loop {
            let n = file.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            self.pos += n as u64;
            let mut rest = &chunk[..n];
            while let Some(i) = rest.iter().position(|b| *b == b'\n') {
                self.partial.extend_from_slice(&rest[..i]);
                if self.partial.last() == Some(&b'\r') {
                    self.partial.pop();
                }
                on_line(&String::from_utf8_lossy(&self.partial));
                self.partial.clear();
                rest = &rest[i + 1..];
            }
            self.partial.extend_from_slice(rest);
        }
        Ok(())
    }
}

#[cfg(unix)]
fn file_id(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.ino()
}

#[cfg(not(unix))]
fn file_id(_meta: &std::fs::Metadata) -> u64 {
    // Rotation is still caught when the new file is shorter than the old.
    0
}

/// Where Arena writes its log on this platform, if known.
pub fn default_log_dir() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    if cfg!(target_os = "macos") {
        Some(home.join("Library/Logs/Wizards Of The Coast/MTGA"))
    } else if cfg!(target_os = "windows") {
        let profile = PathBuf::from(std::env::var_os("USERPROFILE")?);
        Some(profile.join("AppData/LocalLow/Wizards Of The Coast/MTGA"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn feed_all(text: &str) -> Vec<Entry> {
        let mut p = Parser::new();
        text.lines().filter_map(|l| p.feed(l)).collect()
    }

    #[test]
    fn single_and_multi_line_messages() {
        let text = r#"DETAILED LOGS: ENABLED
[UnityCrossThreadLogger]11/16/2026 9:25:13 PM: Match to X: GreToClientEvent
{ "requestId": 2, "greToClientEvent": { "greToClientMessages": [] } }
noise
[UnityCrossThreadLogger]11/16/2026 9:25:15 PM: X to Match: ClientToGremessage
{
  "requestId": 3,
  "payload": {
    "type": "ClientMessageType_MulliganResp"
  }
}
UnityEngine.DebugLogHandler:Internal_Log(LogType, LogOption, String, Object)
{ "orphan": true }
"#;
        let entries = feed_all(text);
        assert_eq!(entries.len(), 3);
        assert!(
            entries[0]
                .header
                .as_deref()
                .unwrap()
                .ends_with("GreToClientEvent")
        );
        assert_eq!(
            entries[1].json["payload"]["type"],
            "ClientMessageType_MulliganResp"
        );
        assert!(entries[1].header.as_deref().unwrap().contains("to Match"));
        assert_eq!(entries[2].header, None);
    }

    #[test]
    fn unterminated_blob_is_abandoned() {
        let text = "{\n  \"a\": 1,\n[UnityCrossThreadLogger]next\n{ \"b\": 2 }\n";
        let entries = feed_all(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].json["b"], 2);
    }

    #[test]
    fn detailed_flag() {
        let mut p = Parser::new();
        assert_eq!(p.detailed_logs(), None);
        p.feed("DETAILED LOGS: DISABLED");
        assert_eq!(p.detailed_logs(), Some(false));
    }

    #[test]
    fn tailer_follows_appends_and_rotation() {
        let dir = std::env::temp_dir().join(format!("mtg-arena-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Player.log");
        let prev = dir.join("Player-prev.log");
        let mut lines = Vec::new();
        let mut tail = Tailer::new(&path);
        assert_eq!(
            tail.poll(|l| lines.push(l.to_string())).unwrap(),
            Poll::Missing
        );

        let mut f = File::create(&path).unwrap();
        write!(f, "one\r\ntwo\nthr").unwrap();
        f.flush().unwrap();
        assert_eq!(
            tail.poll(|l| lines.push(l.to_string())).unwrap(),
            Poll::Appended
        );
        assert_eq!(lines, ["one", "two"]);

        // Finish the line, then rotate: the tail of the old file is still
        // delivered, and the new file is read from its start.
        writeln!(f, "ee").unwrap();
        f.flush().unwrap();
        drop(f);
        std::fs::rename(&path, &prev).unwrap();
        std::fs::write(&path, "new\n").unwrap();
        assert_eq!(
            tail.poll(|l| lines.push(l.to_string())).unwrap(),
            Poll::Rotated
        );
        assert_eq!(
            tail.poll(|l| lines.push(l.to_string())).unwrap(),
            Poll::Appended
        );
        assert_eq!(lines, ["one", "two", "three", "new"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
