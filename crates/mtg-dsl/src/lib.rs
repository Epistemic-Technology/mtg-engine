//! Card DSL: lexer, parser, syntax tree, compiler to `mtg_core` definitions,
//! and the Oracle-text printer used by the round-trip check. See
//! `design.md` §6 and `SYNTAX.md` in this crate.
//!
//! The repository's `cards/` directory is embedded at build time;
//! [`builtin_pool`] compiles it.

pub mod ast;
pub mod compile;
pub mod decompile;
pub mod format;
pub mod lex;
pub mod parse;
pub mod render;

use std::collections::HashMap;

use mtg_core::card::CardPool;
use thiserror::Error;

pub use compile::CompileError;
pub use parse::{ParseError, parse};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/cards.rs"));
}

/// The file name a card goes by: lowercase, non-alphanumerics collapsed to
/// hyphens ("Siege-Gang Commander" → "siege-gang-commander").
pub fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// The directory under `cards/` a card file lives in: its slug's first
/// character, so no directory grows past a few hundred entries.
pub fn shard(slug: &str) -> String {
    slug.chars()
        .next()
        .map_or_else(|| "_".to_string(), |c| c.to_string())
}

/// The embedded card files: (path relative to `cards/`, contents).
pub fn card_files() -> &'static [(&'static str, &'static str)] {
    static FROM_DIR: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    // `MTG_CARDS_DIR=drafts:cards`: read the card files at run time instead,
    // so a built binary checks cards written since (card authoring). With
    // several directories, a card in an earlier one wins.
    match std::env::var_os("MTG_CARDS_DIR") {
        Some(dirs) => FROM_DIR.get_or_init(|| {
            std::env::split_paths(&dirs)
                .flat_map(|d| read_card_dir(&d))
                .collect()
        }),
        None => embedded::CARD_FILES,
    }
}

fn read_card_dir(root: &std::path::Path) -> Vec<(&'static str, &'static str)> {
    fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else if p.extension().is_some_and(|x| x == "card") {
                out.push(p);
            }
        }
    }
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    files
        .into_iter()
        .filter_map(|f| {
            let text = std::fs::read_to_string(&f).ok()?;
            let rel = f.strip_prefix(&root).unwrap_or(&f).display().to_string();
            Some((
                &*Box::leak(rel.into_boxed_str()),
                &*Box::leak(text.into_boxed_str()),
            ))
        })
        .collect()
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("{path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: ParseError,
    },
    #[error("{path}: {source}")]
    Compile {
        path: String,
        #[source]
        source: CompileError,
    },
    #[error("{name} is defined in both {first} and {second}")]
    Duplicate {
        name: String,
        first: String,
        second: String,
    },
}

/// Parse every embedded card file, in compile order (tokens, emblems,
/// cards; by name within each).
pub fn builtin_cards() -> Result<Vec<ast::Card>, LoadError> {
    let mut all = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();
    for (path, text) in card_files() {
        // Read at run time (card authoring), a file being written or a
        // second copy of a card is skipped, not fatal.
        let lenient = std::env::var_os("MTG_CARDS_DIR").is_some();
        let cards = match parse(text) {
            Ok(c) => c,
            Err(e) if lenient => {
                eprintln!("skipping {path}: {e}");
                continue;
            }
            Err(source) => {
                return Err(LoadError::Parse {
                    path: (*path).to_string(),
                    source,
                });
            }
        };
        for c in cards {
            if lenient && seen.contains_key(&c.name) {
                continue;
            }
            if let Some(first) = seen.insert(c.name.clone(), (*path).to_string()) {
                return Err(LoadError::Duplicate {
                    name: c.name,
                    first,
                    second: (*path).to_string(),
                });
            }
            all.push(c);
        }
    }
    compile::compile_order(&mut all);
    for c in &mut all {
        compile::split_mana(c);
    }
    Ok(all)
}

/// The built-in card pool: every card in `cards/`, compiled.
pub fn builtin_pool() -> CardPool {
    try_builtin_pool().unwrap_or_else(|e| panic!("built-in card corpus: {e}"))
}

pub fn try_builtin_pool() -> Result<CardPool, LoadError> {
    let cards = builtin_cards()?;
    compile::compile_all(&cards).map_err(|source| LoadError::Compile {
        path: source.card.clone(),
        source,
    })
}

/// Parse a file's text and compile it against (and into) `pool`, with
/// tokens and emblems the file defines resolved first.
pub fn load_into(
    pool: &mut CardPool,
    text: &str,
) -> Result<Vec<mtg_core::card::CardRef>, LoadError> {
    let mut cards = parse(text).map_err(|source| LoadError::Parse {
        path: "<text>".to_string(),
        source,
    })?;
    compile::compile_order(&mut cards);
    for c in &mut cards {
        compile::split_mana(c);
    }
    let by_name: HashMap<&str, &ast::Card> = cards.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut refs = Vec::new();
    for c in &cards {
        refs.push(
            compile::add_card(pool, c, &by_name).map_err(|source| LoadError::Compile {
                path: source.card.clone(),
                source,
            })?,
        );
    }
    Ok(refs)
}
