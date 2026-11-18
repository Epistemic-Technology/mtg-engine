//! Scenario test DSL and runner. See `design.md` §7.1 and `SYNTAX.md` in
//! this crate for the file format.
//!
//! A scenario sets up a board, scripts actions at points in the turn
//! structure, runs the engine to a step, and asserts on the result. Scripted
//! actions that are not legal fail the scenario, so the corpus checks
//! legality as well as effects.

pub mod ast;
pub mod parse;
pub mod run;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mtg_core::card::CardPool;
use thiserror::Error;

pub use parse::{ParseError, parse};
pub use run::{Failure, Report, run, start_engine};

#[derive(Debug, Error)]
pub enum FileError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}:{}: {}", source.line, source.message)]
    Parse { path: PathBuf, source: ParseError },
}

/// Parse and run every scenario in a file.
pub fn run_file(pool: &Arc<CardPool>, path: &Path) -> Result<Vec<Report>, FileError> {
    let text = std::fs::read_to_string(path).map_err(|source| FileError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let scenarios = parse(&text).map_err(|source| FileError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(scenarios.iter().map(|sc| run(pool, sc)).collect())
}

/// All `.scn` files under `dir`, recursively, sorted.
pub fn find_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "scn") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Where the repository's scenario corpus lives, relative to this crate.
pub fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios")
}
