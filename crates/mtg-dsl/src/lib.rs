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

/// The embedded card files: (path relative to `cards/`, contents).
pub fn card_files() -> &'static [(&'static str, &'static str)] {
    embedded::CARD_FILES
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
        let cards = parse(text).map_err(|source| LoadError::Parse {
            path: (*path).to_string(),
            source,
        })?;
        for c in cards {
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
