//! Events as one-line text for a viewer, hiding what the viewer may not
//! see (a card drawn by the opponent is "a card"). Used by the MCP server
//! and the review tool.

use std::fmt::Write as _;

use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{GameState, Target};

fn who(p: PlayerId, viewer: PlayerId) -> String {
    if p == viewer {
        "You".to_string()
    } else {
        format!("P{}", p.0 + 1)
    }
}

/// "You keep" / "P2 keeps".
fn verb(p: PlayerId, viewer: PlayerId, you: &str, they: &str) -> String {
    if p == viewer {
        format!("You {you}")
    } else {
        format!("P{} {they}", p.0 + 1)
    }
}

fn obj(state: &GameState, id: ObjId) -> String {
    format!("{} ({})", state.name_of_or_lki(id), GameState::label(id))
}

fn target(state: &GameState, t: Target, viewer: PlayerId) -> String {
    match t {
        Target::Player(p) => who(p, viewer),
        Target::Object(id) => obj(state, id),
    }
}

/// The event as text, or `None` for events not worth showing a player
/// (bookkeeping, priority passes).
#[allow(clippy::too_many_lines)]
pub fn describe(state: &GameState, e: &Event, viewer: PlayerId) -> Option<String> {
    let s = match e {
        Event::GameStart { starting_player } => {
            format!("Game start; {} plays first.", who(*starting_player, viewer))
        }
        Event::MulliganTaken { player, count } => {
            format!(
                "{} (to {}).",
                verb(*player, viewer, "mulligan", "mulligans"),
                7 - count
            )
        }
        Event::HandKept { player, .. } => format!("{}.", verb(*player, viewer, "keep", "keeps")),
        Event::TurnBegin { turn, player } => {
            format!("--- Turn {turn}, {}'s turn ---", who(*player, viewer))
        }
        Event::StepBegin { step } => format!("[{step:?}]"),
        Event::RevealedHand { player } => {
            format!("{} reveals their hand.", who(*player, viewer))
        }
        Event::Transformed { obj } => format!("{} transforms.", state.name_of_or_lki(*obj)),
        Event::CollectedEvidence { player } => {
            format!("{} collects evidence.", who(*player, viewer))
        }
        Event::Foraged { player } => format!("{} forages.", who(*player, viewer)),
        Event::Unlocked { obj, door, .. } => {
            let half = state
                .objects
                .get(*obj)
                .and_then(|o| state.pool.get(o.def).doors.as_deref())
                .map_or("a door", |d| state.pool.get(d.half(*door)).name.as_str());
            format!("{half} is unlocked.")
        }
        Event::StageChanged { obj, stage } => {
            let name = state.name_of_or_lki(*obj);
            match state.objects.get(*obj).map(|o| state.pool.get(o.def)) {
                Some(d) if d.subtypes.iter().any(|s| s == "Case") => format!("{name} is solved."),
                _ => format!("{name} becomes level {}.", stage + 1),
            }
        }
        Event::TurnedFaceUp { obj } => format!("{} is turned face up.", state.name_of_or_lki(*obj)),
        Event::Revealed { def, .. } => format!("{} is revealed.", state.pool.get(*def).name),
        Event::Draw { player, card } => {
            if *player == viewer {
                format!("You draw {}.", obj(state, *card))
            } else {
                format!("P{} draws a card.", player.0 + 1)
            }
        }
        Event::DrawFromEmptyLibrary { player } => {
            format!("{} would draw from an empty library.", who(*player, viewer))
        }
        // Draws, casts, resolutions, and destruction have their own events;
        // show the zone changes those do not cover (bounces, exiles,
        // returns from the graveyard).
        Event::ZoneChange { old, from, to, .. } => {
            use crate::state::Zone;
            let shown = matches!(
                (from, to),
                (Zone::Battlefield, Zone::Hand | Zone::Exile | Zone::Library)
                    | (
                        Zone::Graveyard,
                        Zone::Hand | Zone::Battlefield | Zone::Exile
                    )
                    | (Zone::Stack, Zone::Exile)
                    | (Zone::Hand, Zone::Exile | Zone::Library)
            );
            if !shown {
                return None;
            }
            let name = state.name_of_or_lki(*old);
            format!("{name} moves from {from:?} to {to:?}.")
        }
        Event::LandPlayed { player, land } => {
            format!(
                "{} {}.",
                verb(*player, viewer, "play", "plays"),
                obj(state, *land)
            )
        }
        Event::Cast {
            player,
            spell,
            targets,
            ..
        } => {
            let mut s = format!(
                "{} {}",
                verb(*player, viewer, "cast", "casts"),
                obj(state, *spell)
            );
            if !targets.is_empty() {
                let ts: Vec<String> = targets.iter().map(|t| target(state, *t, viewer)).collect();
                let _ = write!(s, " targeting {}", ts.join(", "));
            }
            s.push('.');
            s
        }
        Event::SpellCopied { player, spell, .. } => {
            format!(
                "{} {}.",
                verb(*player, viewer, "copy", "copies"),
                obj(state, *spell)
            )
        }
        Event::Tapped { .. }
        | Event::Untapped { .. }
        | Event::Pass { .. }
        | Event::Blocked { .. }
        | Event::Unblocked { .. } => return None,
        Event::Resolve { spell, .. } => format!("{} resolves.", obj(state, *spell)),
        Event::Fizzle { spell, .. } => {
            format!("{} fizzles (its targets are illegal).", obj(state, *spell))
        }
        Event::Countered { spell, .. } => format!("{} is countered.", obj(state, *spell)),
        Event::Scried {
            player,
            looked,
            down,
        } => format!(
            "{} at {looked} cards and puts {down} down.",
            verb(*player, viewer, "look", "looks")
        ),
        Event::Attack {
            attacker,
            target: t,
        } => format!(
            "{} attacks {}.",
            obj(state, *attacker),
            target(state, *t, viewer)
        ),
        Event::Block { blocker, attacker } => {
            format!("{} blocks {}.", obj(state, *blocker), obj(state, *attacker))
        }
        Event::Damage {
            source,
            target: t,
            amount,
            combat,
        } => format!(
            "{} deals {amount}{} damage to {}.",
            obj(state, *source),
            if *combat { " combat" } else { "" },
            target(state, *t, viewer)
        ),
        Event::LifeChange {
            player,
            delta,
            life,
        } => format!(
            "{} {} life (now {life}).",
            if *delta < 0 {
                verb(*player, viewer, "lose", "loses")
            } else {
                verb(*player, viewer, "gain", "gains")
            },
            delta.abs()
        ),
        Event::FlippedCoin { player, won } => format!(
            "{} a coin and {} the flip.",
            verb(*player, viewer, "flip", "flips"),
            if *won { "wins" } else { "loses" }
        ),
        Event::RolledDie {
            player,
            sides,
            result,
        } => format!(
            "{} a d{sides}: {result}.",
            verb(*player, viewer, "roll", "rolls")
        ),
        Event::Destroyed { obj: o, .. } => format!("{} is destroyed.", obj(state, *o)),
        Event::Cycled { player, card, .. } => {
            format!(
                "{} {}.",
                verb(*player, viewer, "cycle", "cycles"),
                obj(state, *card)
            )
        }
        Event::Discard { player, card } => {
            format!(
                "{} {}.",
                verb(*player, viewer, "discard", "discards"),
                obj(state, *card)
            )
        }
        Event::EffectEnded { .. } | Event::Replaced { .. } | Event::Targeted { .. } => {
            return None;
        }
        Event::CountersAdded {
            obj: o, kind, n, ..
        } => {
            format!("{n} {kind:?} counter(s) put on {}.", obj(state, *o))
        }
        Event::CountersRemoved { obj: o, kind, n } => {
            format!("{n} {kind:?} counter(s) removed from {}.", obj(state, *o))
        }
        Event::Attached { obj: o, to } => {
            format!("{} is attached to {}.", obj(state, *o), obj(state, *to))
        }
        Event::Triggered { source, .. } => format!("{}'s ability triggers.", obj(state, *source)),
        Event::TriggerOnStack { source, .. } => {
            format!("{}'s trigger goes on the stack.", obj(state, *source))
        }
        Event::TriggerRemoved { source, .. } => {
            format!("{}'s trigger is removed.", obj(state, *source))
        }
        Event::Prevented {
            target: t, amount, ..
        } => format!(
            "{amount} damage to {} is prevented.",
            target(state, *t, viewer)
        ),
        Event::PlayerWon { player } => format!("{}.", verb(*player, viewer, "win", "wins")),
        Event::Activated { source, .. } => {
            format!("{}'s ability is activated.", obj(state, *source))
        }
        Event::Sacrificed { obj: o, .. } => format!("{} is sacrificed.", obj(state, *o)),
        Event::TokenCreated {
            obj: o, controller, ..
        } => format!(
            "{} {}.",
            verb(*controller, viewer, "create", "creates"),
            obj(state, *o)
        ),
        Event::ControlChanged { obj: o, to, .. } => {
            format!(
                "{} control of {}.",
                verb(*to, viewer, "gain", "gains"),
                obj(state, *o)
            )
        }
        Event::Unattached { obj: o } => format!("{} becomes unattached.", obj(state, *o)),
        Event::LegendRule { obj: o, .. } => {
            format!("{} goes to the graveyard (legend rule).", obj(state, *o))
        }
        Event::DelayedTriggerCreated { source, .. } => {
            format!("{} sets up a delayed trigger.", obj(state, *source))
        }
        Event::BecameCopy { obj: o, of } => format!(
            "{} becomes a copy of {}.",
            obj(state, *o),
            state.pool.get(*of).name
        ),
        Event::DayNight { day, .. } => {
            format!("It becomes {}.", if *day { "day" } else { "night" })
        }
        Event::BecameMonarch { player } => {
            format!(
                "{} the monarch.",
                verb(*player, viewer, "become", "becomes")
            )
        }
        Event::TookInitiative { player } => {
            format!("{} the initiative.", verb(*player, viewer, "take", "takes"))
        }
        Event::RingTempted { player, bearer } => {
            let tempts = if *player == viewer {
                "The Ring tempts you".to_string()
            } else {
                format!("The Ring tempts P{}", player.0 + 1)
            };
            match bearer {
                Some(b) => format!("{tempts}; {} is the Ring-bearer.", obj(state, *b)),
                None => format!("{tempts}."),
            }
        }
        Event::Ventured {
            player,
            dungeon,
            room,
        } => {
            let d = state.pool.get(*dungeon);
            let name = d
                .rooms
                .get(usize::from(*room))
                .map_or("?", |r| r.name.as_str());
            format!(
                "{} {name} ({}).",
                verb(*player, viewer, "venture into", "ventures into"),
                d.name
            )
        }
        Event::CompletedDungeon { player, dungeon } => format!(
            "{} {}.",
            verb(*player, viewer, "complete", "completes"),
            state.pool.get(*dungeon).name
        ),
        Event::Speed { player, speed } => {
            let whose = if *player == viewer {
                "Your".to_string()
            } else {
                format!("P{}'s", player.0 + 1)
            };
            format!("{whose} speed is {speed}.")
        }
        Event::CityBlessing { player } => format!(
            "{} the city's blessing.",
            verb(*player, viewer, "get", "gets")
        ),
        Event::PlayerLost { player, reason } => {
            format!("{} ({reason:?}).", verb(*player, viewer, "lose", "loses"))
        }
        Event::EmblemCreated {
            controller, def, ..
        } => format!(
            "{} an emblem ({}).",
            verb(*controller, viewer, "get", "gets"),
            state.pool.get(*def).name
        ),
        Event::Regenerated { obj: o, .. } => format!("{} regenerates.", obj(state, *o)),
        Event::Redirected {
            from, to, amount, ..
        } => format!(
            "{amount} damage is redirected from {} to {}.",
            target(state, *from, viewer),
            target(state, *to, viewer)
        ),
        Event::GameOver { result } => format!("Game over: {result:?}."),
    };
    Some(s)
}
