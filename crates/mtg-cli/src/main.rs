//! Human and debugging CLI. See `design.md` §9.3.
//!
//! M0 scaffold: only `version` exists.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mtg", about = "MTG engine command line")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the engine version.
    Version,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => println!("mtg-engine {}", mtg_core::ENGINE_VERSION),
    }
}
