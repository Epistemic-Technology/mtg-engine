//! Splitting the stream of log messages into matches and archiving them.
//!
//! A match opens at the match service's "room state: playing" event and
//! closes at its "match completed" event. Everything exchanged with the
//! match service in between is written to the archive verbatim.
//!
//! Recording is idempotent: a log can be fed through any number of times.
//! Matches already archived are skipped, and a half-written archive is
//! rebuilt from the start. That is what lets the watcher simply re-read
//! `Player-prev.log` and `Player.log` from the top whenever it starts,
//! picking up games played while it was not running.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::archive::{EndReason, Store, Writer};
use crate::log::{Entry, Parser, Poll, Tailer};

/// Something the recorder did, for the caller to report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Started {
        match_id: String,
    },
    /// The match is already in the store.
    AlreadyArchived {
        match_id: String,
    },
    Finished {
        match_id: String,
        path: PathBuf,
        reason: EndReason,
        messages: usize,
    },
    /// The log says detailed logging is off, so it has no match messages.
    DetailedLogsDisabled,
    /// Arena restarted and began a new log.
    LogRotated,
}

struct Open {
    match_id: String,
    writer: Writer,
}

pub struct Recorder {
    store: Store,
    open: Option<Open>,
}

impl Recorder {
    pub fn new(store: Store) -> Self {
        Recorder { store, open: None }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The match being recorded, if any.
    pub fn recording(&self) -> Option<&str> {
        self.open.as_ref().map(|o| o.match_id.as_str())
    }

    /// Process one log message.
    pub fn handle(&mut self, entry: Entry, events: &mut Vec<Event>) -> io::Result<()> {
        let Entry { header, json } = entry;
        if let Some(room) = json
            .get("matchGameRoomStateChangedEvent")
            .and_then(|e| e.get("gameRoomInfo"))
        {
            let match_id = room["gameRoomConfig"]["matchId"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let match_id = match_id.as_str();
            let playing = room["stateType"] == "MatchGameRoomStateType_Playing";
            let completed = room["stateType"] == "MatchGameRoomStateType_MatchCompleted";
            if self.recording().is_some_and(|id| id != match_id) {
                self.interrupt(events)?;
            }
            if self.open.is_none() && playing {
                let started_ms = timestamp_ms(&json);
                if self.store.is_archived(match_id, started_ms) {
                    events.push(Event::AlreadyArchived {
                        match_id: match_id.to_string(),
                    });
                } else {
                    self.open = Some(Open {
                        match_id: match_id.to_string(),
                        writer: self.store.create(match_id, started_ms)?,
                    });
                    events.push(Event::Started {
                        match_id: match_id.to_string(),
                    });
                }
            }
            if let Some(open) = &mut self.open {
                open.writer.message(header, json)?;
                if completed {
                    self.close(EndReason::Completed, events)?;
                }
            }
        } else if is_match_traffic(&json)
            && let Some(open) = &mut self.open
        {
            open.writer.message(header, json)?;
        }
        Ok(())
    }

    /// The log ended or was replaced: close any open match as interrupted.
    pub fn interrupt(&mut self, events: &mut Vec<Event>) -> io::Result<()> {
        self.close(EndReason::Interrupted, events)
    }

    fn close(&mut self, reason: EndReason, events: &mut Vec<Event>) -> io::Result<()> {
        if let Some(open) = self.open.take() {
            let messages = open.writer.messages();
            let path = open.writer.finish(reason)?;
            events.push(Event::Finished {
                match_id: open.match_id,
                path,
                reason,
                messages,
            });
        }
        Ok(())
    }
}

/// Messages to or from the rules engine (GRE), in either direction.
fn is_match_traffic(json: &Value) -> bool {
    json.get("greToClientEvent").is_some()
        || (json.get("clientToMatchServiceMessageType").is_some() && json.get("payload").is_some())
}

/// The server's millisecond timestamp on a match-service event.
fn timestamp_ms(json: &Value) -> u64 {
    json["timestamp"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| json["timestamp"].as_u64())
        .unwrap_or(0)
}

/// Record every match in a finished log file. A match still open when the
/// file ends is archived as interrupted.
pub fn import(rec: &mut Recorder, path: &Path, events: &mut Vec<Event>) -> io::Result<()> {
    let mut parser = Parser::new();
    let mut reader = BufReader::new(File::open(path)?);
    let mut raw = Vec::new();
    loop {
        raw.clear();
        if reader.read_until(b'\n', &mut raw)? == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&raw);
        if let Some(entry) = parser.feed(line.trim_end_matches(['\n', '\r'])) {
            rec.handle(entry, events)?;
        }
    }
    if parser.detailed_logs() == Some(false) {
        events.push(Event::DetailedLogsDisabled);
    }
    rec.interrupt(events)
}

/// Follows Arena's live log, recording matches as they are played.
pub struct Watcher {
    rec: Recorder,
    tailer: Tailer,
    parser: Parser,
    warned: bool,
}

impl Watcher {
    /// Catch up on `Player-prev.log` in `log_dir`, then follow `Player.log`.
    pub fn start(store: Store, log_dir: &Path, events: &mut Vec<Event>) -> io::Result<Watcher> {
        let mut rec = Recorder::new(store);
        let prev = log_dir.join("Player-prev.log");
        if prev.is_file() {
            import(&mut rec, &prev, events)?;
            // An old log without detailed logging is not worth a warning.
            events.retain(|e| *e != Event::DetailedLogsDisabled);
        }
        Ok(Watcher {
            rec,
            tailer: Tailer::new(log_dir.join("Player.log")),
            parser: Parser::new(),
            warned: false,
        })
    }

    pub fn recorder(&self) -> &Recorder {
        &self.rec
    }

    /// Read whatever Arena has logged since the last call.
    pub fn tick(&mut self, events: &mut Vec<Event>) -> io::Result<()> {
        let mut entries = Vec::new();
        let parser = &mut self.parser;
        let poll = self.tailer.poll(|line| entries.extend(parser.feed(line)))?;
        for entry in entries {
            self.rec.handle(entry, events)?;
        }
        if !self.warned && self.parser.detailed_logs() == Some(false) {
            self.warned = true;
            events.push(Event::DetailedLogsDisabled);
        }
        if poll == Poll::Rotated {
            self.rec.interrupt(events)?;
            self.parser = Parser::new();
            self.warned = false;
            events.push(Event::LogRotated);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{self, Record};

    fn room(id: &str, state: &str, ts: u64) -> String {
        format!(
            r#"{{ "timestamp": "{ts}", "matchGameRoomStateChangedEvent": {{ "gameRoomInfo": {{ "gameRoomConfig": {{ "matchId": "{id}" }}, "stateType": "MatchGameRoomStateType_{state}" }} }} }}"#
        )
    }

    const GRE: &str = r#"{ "greToClientEvent": { "greToClientMessages": [] } }"#;
    const CLIENT: &str = "{\n  \"clientToMatchServiceMessageType\": \"x\",\n  \"payload\": {}\n}";

    fn log(parts: &[&str]) -> String {
        let mut s = String::from("DETAILED LOGS: ENABLED\n");
        for p in parts {
            s.push_str("[UnityCrossThreadLogger]hdr\n");
            s.push_str(p);
            s.push('\n');
        }
        s
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mtg-arena-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn import_is_idempotent_and_splits_matches() {
        let dir = temp("rec");
        let path = dir.join("Player.log");
        let text = log(&[
            GRE, // before any match: ignored
            &room("m1", "Playing", 1000),
            GRE,
            CLIENT,
            &room("m1", "MatchCompleted", 2000),
            GRE, // between matches: ignored
            &room("m2", "Playing", 3000),
            GRE,
        ]);
        std::fs::write(&path, text).unwrap();
        let store = Store::new(dir.join("store"));

        let mut events = Vec::new();
        import(&mut Recorder::new(store.clone()), &path, &mut events).unwrap();
        let finished: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Finished {
                    match_id,
                    reason,
                    messages,
                    ..
                } => Some((match_id.as_str(), *reason, *messages)),
                _ => None,
            })
            .collect();
        assert_eq!(
            finished,
            [
                ("m1", EndReason::Completed, 4),
                ("m2", EndReason::Interrupted, 2)
            ]
        );
        let files = store.list().unwrap();
        assert_eq!(files.len(), 2);
        let records = archive::read(&files[0].path).unwrap();
        assert_eq!(records.len(), 6);
        assert!(
            matches!(&records[0], Record::Header(h) if h.match_id == "m1" && h.started_ms == 1000)
        );

        // A second pass over the same log records nothing new.
        let mut events = Vec::new();
        import(&mut Recorder::new(store.clone()), &path, &mut events).unwrap();
        assert!(
            events
                .iter()
                .all(|e| matches!(e, Event::AlreadyArchived { .. }))
        );
        assert_eq!(events.len(), 2);
        assert_eq!(store.list().unwrap().len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn watcher_records_live_and_rebuilds_after_restart() {
        use std::io::Write;
        let dir = temp("watch");
        let path = dir.join("Player.log");
        let store = Store::new(dir.join("store"));
        let mut f = File::create(&path).unwrap();
        write!(f, "{}", log(&[&room("m1", "Playing", 1000), GRE])).unwrap();
        f.flush().unwrap();

        let mut events = Vec::new();
        let mut w = Watcher::start(store.clone(), &dir, &mut events).unwrap();
        w.tick(&mut events).unwrap();
        assert_eq!(w.recorder().recording(), Some("m1"));
        assert!(store.list().unwrap()[0].in_progress);

        // The watcher dies mid-match; a new one rebuilds the archive from
        // the top of the log and carries on.
        drop(w);
        let mut events = Vec::new();
        let mut w = Watcher::start(store.clone(), &dir, &mut events).unwrap();
        w.tick(&mut events).unwrap();
        write!(f, "{}", log(&[CLIENT, &room("m1", "MatchCompleted", 2000)])).unwrap();
        f.flush().unwrap();
        w.tick(&mut events).unwrap();
        assert_eq!(w.recorder().recording(), None);
        let files = store.list().unwrap();
        assert_eq!(files.len(), 1);
        assert!(!files[0].in_progress);
        assert_eq!(archive::read(&files[0].path).unwrap().len(), 6);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
