//! Embeds every `.card` file under the repository's `cards/` directory so
//! the built-in pool needs no files at run time.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
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

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cards");
    let root = root.canonicalize().unwrap_or(root);
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    let mut src = String::from(
        "/// Every card file in the corpus: (path relative to `cards/`, contents).\npub const CARD_FILES: &[(&str, &str)] = &[\n",
    );
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
        println!("cargo:rerun-if-changed={}", f.display());
        writeln!(src, "    ({rel:?}, include_str!({:?})),", f.display()).unwrap();
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("cards.rs");
    std::fs::write(out, src).unwrap();
}
