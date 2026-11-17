//! Card names from Arena's local card database.
//!
//! Arena identifies every card printing, token and ability by a number (its
//! "grpId"). The client ships an `SQLite` database that maps those to English
//! text; this reads it so traces can name cards the way the engine does.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

/// Resolves Arena's numeric ids to English text.
pub trait Names {
    /// The name of the card or token with this grpId.
    fn card(&self, grp_id: u32) -> Option<String>;
    /// The rules text of the ability with this grpId.
    fn ability(&self, grp_id: u32) -> Option<String>;
    /// The name of a counter type ("+1/+1", "Loyalty").
    fn counter(&self, counter_type: u32) -> Option<String>;
}

/// Arena writes a cost as one group with an `o` before each symbol:
/// `{o2oU}` for `{2}{U}`, `{oT}` for `{T}`.
fn standard_symbols(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{o") {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        for symbol in rest[start + 1..start + len].split('o').skip(1) {
            out.push('{');
            out.push_str(symbol);
            out.push('}');
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    out
}

/// Resolves nothing: traces carry ids only.
pub struct NoNames;

impl Names for NoNames {
    fn card(&self, _: u32) -> Option<String> {
        None
    }
    fn ability(&self, _: u32) -> Option<String> {
        None
    }
    fn counter(&self, _: u32) -> Option<String> {
        None
    }
}

/// Arena's `Raw_CardDatabase_*.mtga`.
pub struct CardDb {
    conn: Connection,
    cache: RefCell<HashMap<(u8, u32), Option<String>>>,
}

impl CardDb {
    pub fn open(path: &Path) -> Result<CardDb> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("opening {}", path.display()))?;
        // Fail now, not on first lookup, if this is not a card database.
        conn.prepare("SELECT GrpId, TitleId FROM Cards LIMIT 1")
            .with_context(|| format!("{} is not an Arena card database", path.display()))?;
        Ok(CardDb {
            conn,
            cache: RefCell::new(HashMap::new()),
        })
    }

    /// Find the card database of an installed Arena client, newest first.
    pub fn locate() -> Option<PathBuf> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let installs = [
            home.join("Library/Application Support/Steam/steamapps/common/MTGA/MTGA_Data"),
            home.join("Library/Application Support/com.wizards.mtga"),
            PathBuf::from("/Library/Application Support/com.wizards.mtga"),
            home.join(".steam/steam/steamapps/common/MTGA/MTGA_Data"),
        ];
        let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for install in installs {
            let Ok(dir) = std::fs::read_dir(install.join("Downloads/Raw")) else {
                continue;
            };
            for entry in dir.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("Raw_CardDatabase_") && name.ends_with(".mtga") {
                    let modified = entry.metadata().and_then(|m| m.modified()).ok()?;
                    found.push((modified, entry.path()));
                }
            }
        }
        found.sort();
        found.pop().map(|(_, p)| p)
    }

    fn lookup(&self, kind: u8, id: u32, sql: &str) -> Option<String> {
        if let Some(hit) = self.cache.borrow().get(&(kind, id)) {
            return hit.clone();
        }
        let text = self
            .conn
            .query_row(sql, [id], |row| row.get::<_, Option<String>>(0))
            .optional()
            .ok()
            .flatten()
            .flatten();
        self.cache.borrow_mut().insert((kind, id), text.clone());
        text
    }
}

impl Names for CardDb {
    fn card(&self, grp_id: u32) -> Option<String> {
        self.lookup(
            0,
            grp_id,
            "SELECT l.Loc FROM Cards c JOIN Localizations_enUS l ON l.LocId = c.TitleId \
             WHERE c.GrpId = ?1 ORDER BY l.Formatted DESC LIMIT 1",
        )
    }

    fn ability(&self, grp_id: u32) -> Option<String> {
        let text = self.lookup(
            1,
            grp_id,
            "SELECT l.Loc FROM Abilities a JOIN Localizations_enUS l ON l.LocId = a.TextId \
             WHERE a.Id = ?1 ORDER BY l.Formatted DESC LIMIT 1",
        )?;
        Some(standard_symbols(&text))
    }

    fn counter(&self, counter_type: u32) -> Option<String> {
        self.lookup(
            2,
            counter_type,
            "SELECT l.Loc FROM Enums e JOIN Localizations_enUS l ON l.LocId = e.LocId \
             WHERE e.Type = 'CounterType' AND e.Value = ?1 ORDER BY l.Formatted DESC LIMIT 1",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mana_symbols() {
        assert_eq!(standard_symbols("{oT}: Add {oW}."), "{T}: Add {W}.");
        assert_eq!(
            standard_symbols("{o2oU}, {oT}: Empower Jace 2."),
            "{2}{U}, {T}: Empower Jace 2."
        );
        assert_eq!(standard_symbols("Warp {o1oW}"), "Warp {1}{W}");
        assert_eq!(standard_symbols("Flying"), "Flying");
        assert_eq!(standard_symbols("broken {oT"), "broken {oT");
    }
}
