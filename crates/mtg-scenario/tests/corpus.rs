//! Runs every scenario file under `scenarios/` against the M1 pool.

use std::sync::Arc;

use mtg_dsl::builtin_pool;
use mtg_scenario::{corpus_dir, find_files, run_file};

#[test]
fn corpus_passes() {
    let pool = Arc::new(builtin_pool());
    let files = find_files(&corpus_dir());
    assert!(
        !files.is_empty(),
        "no scenario files under {}",
        corpus_dir().display()
    );
    let mut failures = Vec::new();
    let mut total = 0;
    let mut skipped = 0;
    for path in &files {
        let reports = match run_file(&pool, path) {
            Ok(r) => r,
            Err(e) => {
                failures.push(e.to_string());
                continue;
            }
        };
        for r in reports {
            total += 1;
            if r.skipped.is_some() {
                skipped += 1;
            }
            if let Some(f) = &r.failure {
                failures.push(format!(
                    "{}:{}: \"{}\": {}",
                    path.display(),
                    f.line,
                    r.name,
                    f.message
                ));
            }
        }
    }
    eprintln!(
        "{total} scenarios in {} files, {skipped} skipped",
        files.len()
    );
    assert!(
        failures.is_empty(),
        "{} scenario failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
