//! The round-trip gate (design.md §6.3): the DSL's rendered text must match
//! the card's Oracle text from the Scryfall snapshot, modulo reminder text
//! and keyword-line layout.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use mtg_dsl::ast::{Card, Kind};
use mtg_dsl::render::{self, Lookup};

use crate::DataDir;
use crate::scryfall::{self, Card as ScryCard};

/// One card's round-trip result.
#[derive(Debug, Clone)]
pub struct Row {
    pub name: String,
    pub status: Status,
    /// What the DSL renders.
    pub rendered: String,
    /// Normalized Oracle text from the snapshot, if found.
    pub oracle: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Rendered text equals the normalized Oracle text.
    Match,
    /// Differs, and the card declares why (`oracle_mismatch`).
    Allowed(String),
    /// Differs with no declared reason.
    Mismatch,
    /// Not a card (token or emblem), or no `oracle_id`.
    Skipped,
    /// The `oracle_id` is not in the snapshot.
    NotInSnapshot,
    /// Header fields (cost, type line, P/T, loyalty) differ.
    HeaderMismatch(String),
}

/// Oracle records by `oracle_id` from the snapshot, for the ids asked for.
pub fn oracle_records(data: &DataDir, ids: &[&str]) -> Result<HashMap<String, ScryCard>> {
    let wanted: std::collections::HashSet<&str> = ids.iter().copied().collect();
    let path = data.scryfall_bulk("oracle_cards");
    let mut out = HashMap::new();
    for card in scryfall::read_cards(&path)? {
        let card = card?;
        if let Some(id) = &card.oracle_id
            && wanted.contains(id.as_str())
            && card.layout != "art_series"
        {
            out.entry(id.clone()).or_insert(card);
        }
    }
    Ok(out)
}

fn type_line(card: &Card) -> String {
    let mut parts: Vec<String> = card
        .supertypes
        .iter()
        .map(|s| format!("{s:?}"))
        .chain(card.types.iter().map(|t| format!("{t:?}")))
        .collect();
    if !card.subtypes.is_empty() {
        parts.push("—".to_string());
        parts.extend(card.subtypes.iter().cloned());
    }
    parts.join(" ")
}

pub fn header_diff(card: &Card, s: &ScryCard) -> Option<String> {
    let mut diffs = Vec::new();
    let cost = card.cost.to_string();
    if s.mana_cost.as_deref().unwrap_or("") != cost {
        diffs.push(format!(
            "cost {cost} vs {}",
            s.mana_cost.as_deref().unwrap_or("")
        ));
    }
    let tl = type_line(card);
    if s.type_line.as_deref().unwrap_or("") != tl {
        diffs.push(format!(
            "type line {tl:?} vs {:?}",
            s.type_line.as_deref().unwrap_or("")
        ));
    }
    let pt = card.pt.map(|(p, t)| (p.to_string(), t.to_string()));
    let spt = match (&s.power, &s.toughness) {
        (Some(p), Some(t)) => Some((p.clone(), t.clone())),
        _ => None,
    };
    // A characteristic-defining power/toughness prints as `*`; the DSL
    // leaves `pt` out and the ability defines it.
    let cda = spt.as_ref().is_some_and(|(p, _)| p.contains('*')) && pt.is_none();
    // One half defined (`pt */1`, `pt 2/*`): the `*` reads as 0.
    let star = |s: &str| {
        if s == "*" {
            "0".to_string()
        } else {
            s.to_string()
        }
    };
    let half = pt
        .as_ref()
        .zip(spt.as_ref())
        .is_some_and(|((p, t), (sp, st))| {
            (sp == "*") != (st == "*") && *p == star(sp) && *t == star(st)
        });
    if pt != spt && !cda && !half {
        diffs.push(format!("P/T {pt:?} vs {spt:?}"));
    }
    let loyalty = card.loyalty.map(|n| n.to_string());
    if loyalty != s.loyalty {
        diffs.push(format!("loyalty {loyalty:?} vs {:?}", s.loyalty));
    }
    let defense = card.defense.map(|n| n.to_string());
    if defense != s.defense {
        diffs.push(format!("defense {defense:?} vs {:?}", s.defense));
    }
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

/// The DSL faces of a multi-face card, paired with the snapshot's: the
/// block itself is the first face unless it is a split card.
pub fn face_pairs<'a>(card: &'a Card, s: &ScryCard) -> Option<Vec<(&'a Card, ScryCard)>> {
    let mut dsl: Vec<&Card> = Vec::new();
    if card.layout != mtg_core::card::Layout::Split {
        dsl.push(card);
    }
    dsl.extend(card.faces.iter());
    if dsl.len() != s.card_faces.len() {
        return None;
    }
    Some(
        dsl.into_iter()
            .zip(s.card_faces.iter().map(|f| s.face_record(f)))
            .collect(),
    )
}

/// A face's text: the block's own abilities.
fn face_text(card: &Card, lookup: &dyn Lookup) -> String {
    render::oracle_text(card, lookup)
}

/// Gate a multi-face card face by face; the texts are joined with "//"
/// lines.
fn faces_status(card: &Card, s: &ScryCard, lookup: &dyn Lookup) -> (Status, String, String) {
    let Some(pairs) = face_pairs(card, s) else {
        return (
            Status::HeaderMismatch(format!(
                "{} faces in the DSL, {} in the snapshot",
                card.faces.len() + usize::from(card.layout != mtg_core::card::Layout::Split),
                s.card_faces.len()
            )),
            String::new(),
            String::new(),
        );
    };
    let mut rendered = Vec::new();
    let mut oracle = Vec::new();
    let mut header = Vec::new();
    for (face, rec) in &pairs {
        // The block's name is the whole card's; its face is the front.
        let mut f = (*face).clone();
        f.name.clone_from(&rec.name);
        if let Some(d) = header_diff(&f, rec) {
            header.push(format!("{}: {d}", rec.name));
        }
        rendered.push(render::normalize(&face_text(&f, lookup)));
        oracle.push(render::normalize(rec.oracle_text.as_deref().unwrap_or("")));
    }
    let (r, o) = (rendered.join("\n//\n"), oracle.join("\n//\n"));
    let status = if !header.is_empty() {
        Status::HeaderMismatch(header.join("; "))
    } else if r == o {
        Status::Match
    } else if let Some(reason) = &card.mismatch {
        Status::Allowed(reason.clone())
    } else {
        Status::Mismatch
    };
    (status, o, r)
}

/// Run the gate over `cards` against the snapshot in `data`.
pub fn run(data: &DataDir, cards: &[Card]) -> Result<Vec<Row>> {
    let ids: Vec<&str> = cards
        .iter()
        .filter_map(|c| c.oracle_id.as_deref())
        .collect();
    let records = oracle_records(data, &ids).context("reading the Scryfall snapshot")?;
    let by_name: HashMap<&str, &Card> = cards.iter().map(|c| (c.name.as_str(), c)).collect();
    let lookup: &dyn Lookup = &by_name;
    let mut rows = Vec::new();
    for card in cards {
        let rendered = render::oracle_text(card, lookup);
        let (status, oracle) = match (card.kind, &card.oracle_id) {
            (Kind::Card, Some(id)) => match records.get(id) {
                None => (Status::NotInSnapshot, None),
                Some(s) if !card.faces.is_empty() => {
                    let (status, oracle, text) = faces_status(card, s, lookup);
                    rows.push(Row {
                        name: card.name.clone(),
                        status,
                        rendered: text,
                        oracle: Some(oracle),
                    });
                    continue;
                }
                Some(s) => {
                    let oracle = render::normalize(s.oracle_text.as_deref().unwrap_or(""));
                    let status = if let Some(d) = header_diff(card, s) {
                        Status::HeaderMismatch(d)
                    } else if render::normalize(&rendered) == oracle {
                        Status::Match
                    } else if let Some(reason) = &card.mismatch {
                        Status::Allowed(reason.clone())
                    } else {
                        Status::Mismatch
                    };
                    (status, Some(oracle))
                }
            },
            _ => (Status::Skipped, None),
        };
        rows.push(Row {
            name: card.name.clone(),
            status,
            rendered,
            oracle,
        });
    }
    Ok(rows)
}

/// Whether the snapshot is available for the gate.
pub fn snapshot_present(data: &DataDir) -> bool {
    Path::new(&data.scryfall_bulk("oracle_cards")).exists()
}
