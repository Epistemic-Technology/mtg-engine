//! The round-trip gate over the built-in corpus, when the Scryfall snapshot
//! is present (`mtg-cards ingest --locked`); skipped otherwise so builds
//! never need the network.

use mtg_cards::{DataDir, gate};

#[test]
fn corpus_round_trips_to_oracle_text() {
    let data = DataDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"));
    if !gate::snapshot_present(&data) {
        eprintln!("no Scryfall snapshot; skipping the round-trip gate");
        return;
    }
    let cards = mtg_dsl::builtin_cards().unwrap();
    let rows = gate::run(&data, &cards).unwrap();
    let bad: Vec<String> = rows
        .iter()
        .filter(|r| {
            matches!(
                r.status,
                gate::Status::Mismatch
                    | gate::Status::NotInSnapshot
                    | gate::Status::HeaderMismatch(_)
            )
        })
        .map(|r| {
            format!(
                "{}: {:?}\n  rendered: {:?}\n  oracle:   {:?}",
                r.name, r.status, r.rendered, r.oracle
            )
        })
        .collect();
    assert!(
        bad.is_empty(),
        "{} card(s) fail the gate:\n{}",
        bad.len(),
        bad.join("\n")
    );
    let matched = rows
        .iter()
        .filter(|r| r.status == gate::Status::Match)
        .count();
    assert!(matched >= 100, "only {matched} matched");
}
