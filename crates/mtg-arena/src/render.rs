//! A readable play-by-play of a trace, for checking a recording by eye.

use std::fmt::Write;

use serde_json::Value;

use crate::state::Snapshot;
use crate::trace::{Decision, Event, Frame, GameTrace, MatchTrace, Ref};

/// Render the match, or only game `only` of it.
pub fn render(trace: &MatchTrace, only: Option<u32>) -> String {
    let mut out = String::new();
    let names: Vec<String> = trace
        .players
        .iter()
        .map(|p| {
            let me = if Some(p.seat) == trace.local_seat {
                " (recording)"
            } else {
                ""
            };
            format!("{}{me}", p.name)
        })
        .collect();
    let _ = writeln!(out, "Match {}", trace.match_id);
    let _ = writeln!(out, "  {}", names.join(" vs "));
    if let Some(event) = &trace.event {
        let _ = writeln!(out, "  event: {event}");
    }
    match (trace.end, trace.winner) {
        (None, _) => {
            let _ = writeln!(out, "  still being recorded");
        }
        (Some(end), winner) => {
            let who = winner.map_or_else(|| "nobody".to_string(), |s| trace.player_name(s));
            let _ = writeln!(out, "  recording {end:?}; match winner: {who}");
        }
    }
    for w in &trace.warnings {
        let _ = writeln!(out, "  warning: {w}");
    }
    for game in &trace.games {
        if only.is_none_or(|n| n == game.number) {
            render_game(&mut out, trace, game);
        }
    }
    out
}

fn render_game(out: &mut String, trace: &MatchTrace, game: &GameTrace) {
    let _ = writeln!(out, "\n=== Game {} ===", game.number);
    if let Some(deck) = &game.deck {
        let main: u32 = deck.main.iter().map(|c| c.count).sum();
        let side: u32 = deck.sideboard.iter().map(|c| c.count).sum();
        let _ = writeln!(out, "deck: {main} cards, {side} sideboard");
    }
    for w in &game.warnings {
        let _ = writeln!(out, "warning: {w}");
    }
    for frame in &game.frames {
        for event in &frame.events {
            if event.kind == "NewTurnStarted" {
                let t = &frame.state.turn;
                let who = t.active.map(|s| trace.player_name(s)).unwrap_or_default();
                let life: Vec<String> = frame
                    .state
                    .players
                    .iter()
                    .map(|p| format!("{} {}", trace.player_name(p.seat), p.life))
                    .collect();
                // The first turn is announced before Arena numbers it.
                let _ = writeln!(
                    out,
                    "\nTurn {} — {who}   [{}]",
                    t.number.max(1),
                    life.join(", ")
                );
            } else if let Some(text) = describe(trace, frame, event) {
                let _ = writeln!(out, "  {:<17} {text}", step_label(&frame.state));
            }
        }
        for decision in &frame.decisions {
            for line in answers(trace, decision) {
                let _ = writeln!(out, "  {:<17} > {line}", step_label(&frame.state));
            }
        }
    }
    if let Some(r) = &game.result {
        let who = r
            .winner
            .map_or_else(|| "draw".to_string(), |s| trace.player_name(s));
        let _ = writeln!(out, "\nGame {} result: {who} ({})", game.number, r.reason);
    }
}

fn step_label(state: &Snapshot) -> &str {
    state
        .turn
        .step
        .as_deref()
        .or(state.turn.phase.as_deref())
        .unwrap_or("-")
}

fn label(r: &Ref) -> String {
    r.name.clone().unwrap_or_else(|| format!("#{}", r.id))
}

fn first(event: &Event) -> String {
    event.targets.first().map_or_else(|| "?".to_string(), label)
}

fn detail<'a>(event: &'a Event, key: &str) -> &'a Value {
    event.details.get(key).unwrap_or(&Value::Null)
}

fn text(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

/// Name whatever `id` refers to in this state: a player or an object.
fn name_in(trace: &MatchTrace, state: &Snapshot, id: u32) -> String {
    if trace.players.iter().any(|p| p.seat == id) {
        return trace.player_name(id);
    }
    let zones = [&state.battlefield, &state.stack, &state.exile];
    let per_player = state.players.iter().flat_map(|p| [&p.hand, &p.graveyard]);
    zones
        .into_iter()
        .chain(per_player)
        .flatten()
        .find(|o| o.id == id)
        .and_then(|o| o.name.clone())
        .unwrap_or_else(|| format!("#{id}"))
}

fn describe(trace: &MatchTrace, frame: &Frame, event: &Event) -> Option<String> {
    let source = || event.source.as_ref().map_or_else(|| "?".to_string(), label);
    Some(match event.kind.as_str() {
        "ZoneTransfer" => {
            let category = text(detail(event, "category"));
            if category == "Resolve" {
                return None; // reported by ResolutionStart
            }
            format!(
                "{category}: {} ({} -> {})",
                event
                    .targets
                    .first()
                    .and_then(|r| r.name.clone())
                    .unwrap_or_else(|| "a card".into()),
                text(detail(event, "zone_src")),
                text(detail(event, "zone_dest")),
            )
        }
        "ResolutionStart" => format!("resolves: {}", source()),
        "UserActionTaken" if detail(event, "actionType") == "Activate" => {
            format!(
                "{} activates {}: {}",
                source(),
                first(event),
                text(detail(event, "ability"))
            )
        }
        "PlayerSubmittedTargets" => {
            let spell = event.targets.first()?;
            let object = frame.state.stack.iter().find(|o| o.id == spell.id)?;
            if object.targets.is_empty() {
                return None;
            }
            let targets: Vec<String> = object
                .targets
                .iter()
                .map(|id| name_in(trace, &frame.state, *id))
                .collect();
            format!("{} targets {}", label(spell), targets.join(", "))
        }
        "DamageDealt" => format!(
            "{} deals {} damage to {}",
            source(),
            detail(event, "damage"),
            first(event)
        ),
        "ModifiedLife" => {
            let seat = event.targets.first()?.id;
            let now = frame.state.players.iter().find(|p| p.seat == seat)?.life;
            let delta = detail(event, "life").as_i64()?;
            format!("{} life {delta:+} (now {now})", trace.player_name(seat))
        }
        "CounterAdded" | "CounterRemoved" => format!(
            "{}: {}{} {} counter",
            first(event),
            if event.kind == "CounterAdded" {
                '+'
            } else {
                '-'
            },
            detail(event, "transaction_amount"),
            text(detail(event, "counter_type")),
        ),
        "TokenCreated" => format!("token created: {}", first(event)),
        _ => return None,
    })
}

/// One line per answer the player gave, for the answers worth reading.
fn answers(trace: &MatchTrace, decision: &Decision) -> Vec<String> {
    let who = trace.player_name(decision.seat);
    let mut out = Vec::new();
    for r in &decision.responses {
        let line = match r.kind.as_str() {
            "PerformActionResp" => {
                let actions = r.body["performActionResp"]["actions"].as_array();
                let parts: Vec<String> = actions
                    .into_iter()
                    .flatten()
                    .map(|a| {
                        let mut s = text(&a["actionType"]);
                        if let Some(name) = a["name"].as_str() {
                            let _ = write!(s, " {name}");
                        }
                        s
                    })
                    .collect();
                // Paying mana is reported by the taps that follow.
                if parts.iter().all(|p| p.starts_with("Activate_Mana")) {
                    continue;
                }
                parts.join(", ")
            }
            "MulliganResp" => text(&r.body["mulliganResp"]["decision"]),
            "ConcedeReq" => "concedes".to_string(),
            "UIMessage" | "SetSettingsReq" => continue,
            other => other
                .trim_end_matches("Resp")
                .trim_end_matches("Req")
                .to_string(),
        };
        out.push(format!("{who}: {line}"));
    }
    out
}
