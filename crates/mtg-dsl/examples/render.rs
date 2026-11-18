//! Print the Oracle text a card file renders to, and whether it compiles
//! against the built-in corpus: `cargo run -p mtg-dsl --example render --
//! file.card`.

use std::collections::HashMap;

fn main() {
    let corpus = mtg_dsl::builtin_cards().expect("the built-in corpus parses");
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).expect("readable file");
        let cards = match mtg_dsl::parse(&text) {
            Ok(c) => c,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        let mut all = corpus.clone();
        all.extend(cards.iter().cloned());
        let by_name: HashMap<&str, &mtg_dsl::ast::Card> =
            all.iter().map(|c| (c.name.as_str(), c)).collect();
        for c in &cards {
            println!("== {}", c.name);
            println!("{}", mtg_dsl::render::oracle_text(c, &by_name));
            for f in &c.faces {
                println!("// {}", f.name);
                println!("{}", mtg_dsl::render::oracle_text(f, &by_name));
            }
        }
        mtg_dsl::compile::compile_order(&mut all);
        if let Err(e) = mtg_dsl::compile::compile_all(&all) {
            println!("compile error: {e}");
        }
    }
}
