//! Terminal play against a baseline agent (design §9.3). The human is
//! player 1 and sees only what that player may see.

use std::fmt::Write as _;
use std::io::{BufRead, Write};
use std::sync::Arc;

use anyhow::{Result, bail};
use mtg_core::card::m1_pool;
use mtg_core::decision::{Decision, StepResult};
use mtg_core::engine::{Engine, GameConfig};
use mtg_core::ids::PlayerId;
use mtg_core::state::{GameState, Zone};

const HUMAN: PlayerId = PlayerId(0);

pub fn run(deck: &str, opponent: &str, agent: &str, seed: u64) -> Result<()> {
    let pool = Arc::new(m1_pool());
    let Some(d1) = mtg_sim::decks::by_name(&pool, deck) else {
        bail!("unknown deck {deck}");
    };
    let Some(d2) = mtg_sim::decks::by_name(&pool, opponent) else {
        bail!("unknown deck {opponent}");
    };
    let Some(mut ai) = mtg_agents::by_name(agent, seed) else {
        bail!("unknown agent {agent} (greedy, random, first)");
    };
    let config = GameConfig {
        decks: vec![d1, d2],
        starting_life: 20,
        seed,
        logging: true,
    };
    let mut engine = Engine::new(pool, &config);
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut log_from = 0;
    println!("You are player 1 ({deck}); the opponent ({agent}) plays {opponent}.");
    loop {
        let decision = match engine.current() {
            StepResult::Over(r) => {
                print_events(engine.state(), &mut log_from);
                println!("\nGame over: {r:?}");
                return Ok(());
            }
            StepResult::Decision(d) => d.clone(),
        };
        if decision.player != HUMAN {
            let choice = ai.choose(engine.state(), &decision);
            engine.step(choice)?;
            continue;
        }
        print_events(engine.state(), &mut log_from);
        print_board(engine.state());
        print_decision(engine.state(), &decision);
        let choice = loop {
            print!("> ");
            std::io::stdout().flush()?;
            let Some(line) = lines.next() else {
                println!();
                return Ok(());
            };
            let line = line?;
            let line = line.trim();
            if line == "q" || line == "quit" {
                return Ok(());
            }
            match line.parse::<usize>() {
                Ok(n) if n >= 1 && n <= decision.options.len() => {
                    break u32::try_from(n - 1).unwrap_or(0);
                }
                _ => println!("enter a number from 1 to {}, or q", decision.options.len()),
            }
        };
        engine.step(choice)?;
    }
}

fn print_events(state: &GameState, from: &mut usize) {
    for e in &state.log[*from..] {
        println!("  · {e:?}");
    }
    *from = state.log.len();
}

fn describe_permanent(state: &GameState, id: mtg_core::ids::ObjId) -> String {
    let o = &state.objects[id];
    let c = state.characteristics(id).expect("permanent exists");
    let mut s = state.name_of(id).to_string();
    if c.is_creature() {
        let _ = write!(s, " {}/{}", c.power, c.toughness);
        if o.damage > 0 {
            let _ = write!(s, " ({} damage)", o.damage);
        }
        if !c.keywords.is_empty() {
            let _ = write!(s, " {:?}", c.keywords);
        }
    }
    if o.tapped {
        s.push_str(" [tapped]");
    }
    if let Some(cb) = &state.combat {
        if cb.attacker(id).is_some() {
            s.push_str(" [attacking]");
        } else if cb.blocked_by(id).is_some() {
            s.push_str(" [blocking]");
        }
    }
    s
}

fn print_board(state: &GameState) {
    println!(
        "\n=== Turn {} ({}), {:?}, priority: player {} ===",
        state.turn_number,
        if state.active_player == HUMAN {
            "your turn"
        } else {
            "opponent's turn"
        },
        state.step,
        state.priority.0 + 1
    );
    for p in [PlayerId(1), HUMAN] {
        let ps = state.player(p);
        let who = if p == HUMAN { "You" } else { "Opponent" };
        println!(
            "{who}: {} life, {} in hand, {} in library, {} in graveyard",
            ps.life,
            ps.hand.len(),
            ps.library.len(),
            ps.graveyard.len()
        );
        let perms: Vec<String> = state
            .permanents_of(p)
            .map(|id| describe_permanent(state, id))
            .collect();
        println!(
            "  battlefield: {}",
            if perms.is_empty() {
                "(empty)".to_string()
            } else {
                perms.join("; ")
            }
        );
    }
    if !state.stack.is_empty() {
        let items: Vec<String> = state
            .stack
            .iter()
            .rev()
            .map(|id| {
                let o = &state.objects[*id];
                let targets: Vec<String> = o
                    .targets
                    .iter()
                    .map(|t| match t {
                        mtg_core::state::Target::Player(p) => format!("player {}", p.0 + 1),
                        mtg_core::state::Target::Object(id) => state.name_of(*id).to_string(),
                    })
                    .collect();
                if targets.is_empty() {
                    state.name_of(*id).to_string()
                } else {
                    format!("{} -> {}", state.name_of(*id), targets.join(", "))
                }
            })
            .collect();
        println!("  stack (top first): {}", items.join("; "));
    }
    let hand: Vec<String> = state
        .player(HUMAN)
        .hand
        .iter()
        .filter(|id| state.objects[**id].zone == Zone::Hand)
        .map(|id| state.name_of(*id).to_string())
        .collect();
    println!("  your hand: {}", hand.join(", "));
}

fn print_decision(state: &GameState, d: &Decision) {
    println!("{:?}:", d.kind);
    for (i, c) in d.options.iter().enumerate() {
        println!("  {:>3}. {}", i + 1, c.describe(state));
    }
}
