//! Translating an archived match into a trace: for each game, the sequence
//! of states the rules engine reported, the events that led from one to the
//! next, and the decisions the recording player was offered and took.
//!
//! The trace is the engine-facing form of a recording. It is derived, never
//! stored as the source of truth: rebuild it from the archive whenever the
//! translation improves.
//!
//! What a trace can and cannot say:
//!
//! - It is one player's view. The opponent's hand and both libraries are
//!   hidden; the opponent's cards are identified when they become visible.
//! - Only the recording player's decisions are recorded as decisions. The
//!   opponent's show up as events (`UserActionTaken`, `ZoneTransfer`, ...).
//! - Arena gives an object a new id whenever it changes zones; an
//!   `ObjectIdChanged` event links the old id to the new.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::archive::{EndReason, Record};
use crate::cards::Names;
use crate::state::{Annotation, GameObject, GameStateMessage, Snapshot, Tracker, bare};

pub const FORMAT: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MatchTrace {
    pub format: u32,
    pub match_id: String,
    pub started_ms: u64,
    /// Arena's name for the queue or event the match was played in.
    pub event: Option<String>,
    pub players: Vec<PlayerInfo>,
    /// Seat of the player whose client made the recording.
    pub local_seat: Option<u32>,
    /// How the recording ended; `None` if the match is still being recorded.
    pub end: Option<EndReason>,
    /// Seat of the match winner.
    pub winner: Option<u32>,
    pub games: Vec<GameTrace>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub seat: u32,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GameTrace {
    pub number: u32,
    /// The recording player's deck for this game (after sideboarding).
    pub deck: Option<Deck>,
    pub result: Option<GameResult>,
    pub frames: Vec<Frame>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Deck {
    pub main: Vec<DeckCard>,
    pub sideboard: Vec<DeckCard>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeckCard {
    pub count: u32,
    pub grp: u32,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameResult {
    /// Seat of the winner; `None` for a draw.
    pub winner: Option<u32>,
    /// Arena's reason: `Game` (played out), `Concede`, `Timeout`, ...
    pub reason: String,
}

/// One reported game state and how the game got there.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    /// Arena's `gameStateId`; restarts at 1 each game.
    pub state_id: u32,
    /// What happened since the previous frame, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    /// Questions put to the recording player in this state, and the answers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decision>,
    pub state: Snapshot,
}

/// Something the rules engine reported happening (an Arena "annotation").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Arena's annotation type: `ZoneTransfer`, `DamageDealt`, ...
    pub kind: String,
    /// What caused it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Ref>,
    /// What it happened to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<Ref>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, Value>,
}

/// A reference to an object, a player (by seat), or one of Arena's internal
/// ids, named where it can be: card name for objects, `@name` for players.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ref {
    pub id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Decision {
    pub seat: u32,
    /// Arena's request type without the `Req`: `ActionsAvailable`,
    /// `SelectTargets`, `DeclareAttackers`, ... `Unprompted` for something
    /// the player did without being asked (conceding).
    pub kind: String,
    pub msg_id: u64,
    /// The request body: the options offered.
    pub request: Value,
    /// The player's answers. Usually one; none if the game moved on first.
    pub responses: Vec<Response>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub kind: String,
    pub body: Value,
}

impl MatchTrace {
    /// Every distinct card and token name seen in the match.
    pub fn card_names(&self) -> BTreeSet<(String, bool)> {
        let mut out = BTreeSet::new();
        for game in &self.games {
            for card in game
                .deck
                .iter()
                .flat_map(|d| d.main.iter().chain(&d.sideboard))
            {
                if let Some(name) = &card.name {
                    out.insert((name.clone(), false));
                }
            }
            for frame in &game.frames {
                let s = &frame.state;
                let zones = [&s.battlefield, &s.stack, &s.exile, &s.command];
                let per_player = s.players.iter().flat_map(|p| [&p.hand, &p.graveyard]);
                for o in zones.into_iter().chain(per_player).flatten() {
                    if let (Some(name), "Card" | "Token") = (&o.name, o.kind.as_str()) {
                        out.insert((name.clone(), o.kind == "Token"));
                    }
                }
            }
        }
        out
    }

    pub fn player_name(&self, seat: u32) -> String {
        self.players
            .iter()
            .find(|p| p.seat == seat)
            .map_or_else(|| format!("seat {seat}"), |p| p.name.clone())
    }
}

/// Keys of request and response bodies that describe the client's display
/// or its automatic mana payment rather than the game.
const NOISE_KEYS: &[&str] = &[
    "autoTapSolution",
    "autoTapActionsReq",
    "manaPaymentOptions",
    "manaSelections",
    "facetId",
    "shouldStop",
    "highlight",
    "uniqueAbilityId",
    "isBatchable",
];

fn is_enum_value(s: &str) -> bool {
    s.split_once('_').is_some_and(|(prefix, rest)| {
        prefix.len() >= 3
            && prefix.starts_with(|c: char| c.is_ascii_uppercase())
            && prefix.chars().all(|c| c.is_ascii_alphanumeric())
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Tidy a request or response body: strip enum prefixes, drop display-only
/// keys, and name cards and abilities next to their ids.
fn normalize(v: &Value, names: &dyn Names) -> Value {
    match v {
        Value::String(s) if is_enum_value(s) => Value::String(bare(s).to_string()),
        Value::Array(a) => Value::Array(a.iter().map(|x| normalize(x, names)).collect()),
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, x) in o {
                if NOISE_KEYS.contains(&k.as_str()) {
                    continue;
                }
                out.insert(k.clone(), normalize(x, names));
                let id = x.as_u64().and_then(|n| u32::try_from(n).ok());
                if let (Some(id), "grpId") = (id, k.as_str())
                    && let Some(name) = names.card(id)
                {
                    out.insert("name".to_string(), Value::String(name));
                }
                if let (Some(id), "abilityGrpId") = (id, k.as_str())
                    && id != 0
                    && let Some(text) = names.ability(id)
                {
                    out.insert("ability".to_string(), Value::String(text));
                }
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn u32_of(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// Arena's `ActionType` numbering, as used in `UserActionTaken`.
fn action_type(n: u64) -> Option<&'static str> {
    Some(match n {
        1 => "Cast",
        2 => "Activate",
        3 => "Play",
        4 => "Activate_Mana",
        5 => "Pass",
        _ => return None,
    })
}

struct Builder<'a> {
    names: &'a dyn Names,
    trace: MatchTrace,
    tracker: Tracker,
    /// The deck for the next game to start.
    deck: Option<Deck>,
    /// Objects the state message in hand deleted, which its events may
    /// still refer to (a token leaving the battlefield).
    gone: BTreeMap<u32, GameObject>,
    /// New id -> old id for the objects that message moved between zones.
    renamed: BTreeMap<u32, u32>,
}

impl Builder<'_> {
    fn deck(&self, v: &Value) -> Option<Deck> {
        let pile = |key: &str| {
            let mut counts: Vec<DeckCard> = Vec::new();
            for grp in v[key].as_array().into_iter().flatten().filter_map(u32_of) {
                match counts.iter_mut().find(|c| c.grp == grp) {
                    Some(c) => c.count += 1,
                    None => counts.push(DeckCard {
                        count: 1,
                        grp,
                        name: self.names.card(grp),
                    }),
                }
            }
            counts
        };
        v.get("deckCards").map(|_| Deck {
            main: pile("deckCards"),
            sideboard: pile("sideboardCards"),
        })
    }

    fn reference(&self, id: u32) -> Ref {
        let find = |id: &u32| self.tracker.objects.get(id).or_else(|| self.gone.get(id));
        // A card that moved somewhere the recording player cannot see it
        // (or a token that ceased to exist) is named by what it was.
        let object = find(&id)
            .filter(|o| o.grp_id != 0)
            .or_else(|| self.renamed.get(&id).and_then(find));
        let name = if let Some(o) = object {
            let grp = if o.kind == "GameObjectType_Ability" {
                o.object_source_grp_id.unwrap_or(0)
            } else {
                o.grp_id
            };
            self.names.card(grp)
        } else {
            self.trace
                .players
                .iter()
                .find(|p| p.seat == id)
                .map(|p| format!("@{}", p.name))
        };
        Ref { id, name }
    }

    fn event(&self, a: &Annotation, kind: &str) -> Event {
        let mut details = a.detail_map();
        for key in ["zone_src", "zone_dest", "source_zone"] {
            if let Some(label) = details
                .get(key)
                .and_then(u32_of)
                .and_then(|z| self.tracker.zone_label(z))
            {
                details.insert(key.to_string(), Value::String(label));
            }
        }
        if let Some(name) = details
            .get("grpid")
            .and_then(u32_of)
            .and_then(|g| self.names.card(g))
        {
            details.insert("name".to_string(), Value::String(name));
        }
        if let Some(name) = details
            .get("counter_type")
            .and_then(u32_of)
            .and_then(|c| self.names.counter(c))
        {
            details.insert("counter_type".to_string(), Value::String(name));
        }
        if kind == "UserActionTaken"
            && let Some(name) = details
                .get("actionType")
                .and_then(Value::as_u64)
                .and_then(action_type)
        {
            details.insert("actionType".to_string(), Value::String(name.to_string()));
        }
        if let Some(text) = details
            .get("abilityGrpId")
            .and_then(u32_of)
            .filter(|g| *g != 0)
            .and_then(|g| self.names.ability(g))
        {
            details.insert("ability".to_string(), Value::String(text));
        }
        Event {
            kind: kind.to_string(),
            source: a.affector_id.map(|id| self.reference(id)),
            targets: a
                .affected_ids
                .iter()
                .map(|id| self.reference(*id))
                .collect(),
            details,
        }
    }

    fn state(&mut self, body: &Value) {
        let msg: GameStateMessage = match serde_json::from_value(body.clone()) {
            Ok(m) => m,
            Err(e) => {
                self.trace
                    .warnings
                    .push(format!("unreadable game state: {e}"));
                return;
            }
        };
        let number = msg
            .game_info
            .as_ref()
            .and_then(|i| u32_of(&i["gameNumber"]))
            .unwrap_or(1);
        if msg.is_full() {
            if self.trace.games.last().is_some_and(|g| g.number == number) {
                self.warn(format!(
                    "full state resent at state {} (reconnect)",
                    msg.game_state_id
                ));
            } else {
                self.trace.games.push(GameTrace {
                    number,
                    deck: self.deck.clone(),
                    ..GameTrace::default()
                });
            }
        } else if self.trace.games.is_empty() {
            self.trace
                .warnings
                .push("recording starts mid-game: no full state to build on".to_string());
            self.trace.games.push(GameTrace {
                number,
                deck: self.deck.clone(),
                ..GameTrace::default()
            });
        }
        self.gone = msg
            .diff_deleted_instance_ids
            .iter()
            .filter_map(|id| Some((*id, self.tracker.objects.get(id)?.clone())))
            .collect();
        self.renamed = msg
            .annotations
            .iter()
            .filter(|a| a.is("ObjectIdChanged"))
            .filter_map(|a| Some((a.detail_u32("new_id")?, a.detail_u32("orig_id")?)))
            .collect();
        if let Some(w) = self.tracker.apply(&msg) {
            self.warn(w);
        }

        let mut events = Vec::new();
        for a in &msg.annotations {
            for kind in &a.kinds {
                events.push(self.event(a, bare(kind)));
            }
        }
        let state = self.tracker.snapshot(self.names);
        let result = self.result();
        let game = self
            .trace
            .games
            .last_mut()
            .expect("a game was just ensured");
        if result.is_some() {
            game.result = result;
        }
        if events.is_empty() && game.frames.last().is_some_and(|f| f.state == state) {
            return;
        }
        game.frames.push(Frame {
            state_id: msg.game_state_id,
            events,
            decisions: Vec::new(),
            state,
        });
    }

    fn result(&self) -> Option<GameResult> {
        let info = &self.tracker.game_info;
        if info["stage"] != "GameStage_GameOver" {
            return None;
        }
        let r = info["results"]
            .as_array()?
            .iter()
            .rfind(|r| r["scope"] == "MatchScope_Game")?;
        Some(GameResult {
            winner: u32_of(&r["winningTeamId"]).and_then(|t| self.tracker.seat_of_team(t)),
            reason: bare(r["reason"].as_str().unwrap_or("")).to_string(),
        })
    }

    fn warn(&mut self, w: String) {
        match self.trace.games.last_mut() {
            Some(g) => g.warnings.push(w),
            None => self.trace.warnings.push(w),
        }
    }

    fn last_frame(&mut self) -> Option<&mut Frame> {
        self.trace.games.last_mut()?.frames.last_mut()
    }

    fn gre(&mut self, m: &Value) {
        let kind = bare(m["type"].as_str().unwrap_or(""));
        let seats = m["systemSeatIds"].as_array().map_or(&[][..], Vec::as_slice);
        match kind {
            "GameStateMessage" | "QueuedGameStateMessage" => self.state(&m["gameStateMessage"]),
            "ConnectResp" => {
                self.trace.local_seat = seats.first().and_then(u32_of);
                self.deck = self
                    .deck(&m["connectResp"]["deckMessage"])
                    .or(self.deck.take());
            }
            // A notice to both players, not a question.
            "PromptReq" => {}
            _ if kind.ends_with("Req") || kind == "OptionalActionMessage" => {
                let seat = seats
                    .first()
                    .and_then(u32_of)
                    .or(self.trace.local_seat)
                    .unwrap_or(0);
                // The body sits under the camel-cased type name; anything
                // else on the message besides routing is part of the ask.
                let mut request = Map::new();
                for (k, v) in m.as_object().into_iter().flatten() {
                    if !matches!(
                        k.as_str(),
                        "type" | "systemSeatIds" | "msgId" | "gameStateId"
                    ) {
                        request.insert(k.clone(), normalize(v, self.names));
                    }
                }
                let decision = Decision {
                    seat,
                    kind: kind
                        .trim_end_matches("Req")
                        .trim_end_matches("Message")
                        .to_string(),
                    msg_id: m["msgId"].as_u64().unwrap_or(0),
                    request: Value::Object(request),
                    responses: Vec::new(),
                };
                match self.last_frame() {
                    Some(f) => f.decisions.push(decision),
                    None => self.warn(format!("{kind} before any game state")),
                }
            }
            _ => {}
        }
    }

    fn client(&mut self, payload: &Value) {
        let kind = bare(payload["type"].as_str().unwrap_or(""));
        if matches!(kind, "UIMessage" | "SetSettingsReq") {
            return;
        }
        let mut body = Map::new();
        for (k, v) in payload.as_object().into_iter().flatten() {
            if !matches!(k.as_str(), "type" | "respId" | "gameStateId") {
                body.insert(k.clone(), normalize(v, self.names));
            }
        }
        if kind == "SubmitDeckResp"
            && let Some(deck) = self.deck(&payload["submitDeckResp"]["deck"])
        {
            self.deck = Some(deck);
        }
        let response = Response {
            kind: kind.to_string(),
            body: Value::Object(body),
        };
        let seat = self.trace.local_seat.unwrap_or(0);
        let Some(game) = self.trace.games.last_mut() else {
            return;
        };
        let asked = payload["respId"].as_u64().and_then(|id| {
            game.frames
                .iter_mut()
                .rev()
                .flat_map(|f| f.decisions.iter_mut().rev())
                .find(|d| d.msg_id == id)
        });
        if let Some(decision) = asked {
            decision.responses.push(response);
        } else if let Some(frame) = game.frames.last_mut() {
            frame.decisions.push(Decision {
                seat,
                kind: "Unprompted".to_string(),
                msg_id: 0,
                request: Value::Null,
                responses: vec![response],
            });
        }
    }

    fn room(&mut self, room: &Value) {
        let config = &room["gameRoomConfig"];
        if self.trace.players.is_empty() {
            for p in config["reservedPlayers"].as_array().into_iter().flatten() {
                self.trace.players.push(PlayerInfo {
                    seat: u32_of(&p["systemSeatId"]).unwrap_or(0),
                    name: p["playerName"].as_str().unwrap_or("?").to_string(),
                });
                if self.trace.event.is_none() {
                    self.trace.event = p["eventId"].as_str().map(str::to_string);
                }
            }
            self.trace.players.sort_by_key(|p| p.seat);
        }
        let results = room["finalMatchResult"]["resultList"].as_array();
        if let Some(r) = results
            .into_iter()
            .flatten()
            .find(|r| r["scope"] == "MatchScope_Match")
        {
            // Teams are single players in a duel, numbered like the seats.
            self.trace.winner =
                u32_of(&r["winningTeamId"]).map(|t| self.tracker.seat_of_team(t).unwrap_or(t));
        }
    }
}

/// Build the trace of an archived match.
pub fn build(records: &[Record], names: &dyn Names) -> MatchTrace {
    let mut b = Builder {
        names,
        trace: MatchTrace {
            format: FORMAT,
            ..MatchTrace::default()
        },
        tracker: Tracker::default(),
        deck: None,
        gone: BTreeMap::new(),
        renamed: BTreeMap::new(),
    };
    for record in records {
        match record {
            Record::Header(h) => {
                b.trace.match_id.clone_from(&h.match_id);
                b.trace.started_ms = h.started_ms;
            }
            Record::End { reason } => b.trace.end = Some(*reason),
            Record::Msg { data, .. } => {
                if let Some(room) = data
                    .get("matchGameRoomStateChangedEvent")
                    .and_then(|e| e.get("gameRoomInfo"))
                {
                    b.room(room);
                } else if let Some(messages) = data
                    .get("greToClientEvent")
                    .and_then(|e| e["greToClientMessages"].as_array())
                {
                    for m in messages {
                        b.gre(m);
                    }
                } else if let Some(payload) = data.get("payload").filter(|p| p.is_object()) {
                    b.client(payload);
                }
            }
        }
    }
    b.trace
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::archive::Header;

    struct TestNames;
    impl Names for TestNames {
        fn card(&self, grp: u32) -> Option<String> {
            match grp {
                7 => Some("Grizzly Bears".into()),
                9 => Some("Forest".into()),
                _ => None,
            }
        }
        fn ability(&self, grp: u32) -> Option<String> {
            (grp == 1005).then(|| "{T}: Add {G}.".into())
        }
        fn counter(&self, _: u32) -> Option<String> {
            None
        }
    }

    fn msg(data: Value) -> Record {
        Record::Msg { hdr: None, data }
    }

    fn gre(messages: &Value) -> Record {
        msg(json!({"greToClientEvent": {"greToClientMessages": messages}}))
    }

    fn client(payload: &Value) -> Record {
        msg(json!({"clientToMatchServiceMessageType": "x", "payload": payload}))
    }

    fn mini_match() -> Vec<Record> {
        vec![
            Record::Header(Header {
                format: 1,
                recorder: "t".into(),
                match_id: "m".into(),
                started_ms: 5,
            }),
            msg(json!({"matchGameRoomStateChangedEvent": {"gameRoomInfo": {
                "gameRoomConfig": {"matchId": "m", "reservedPlayers": [
                    {"playerName": "Opp", "systemSeatId": 1, "eventId": "Play"},
                    {"playerName": "Me", "systemSeatId": 2, "eventId": "Play"}]},
                "stateType": "MatchGameRoomStateType_Playing"}}})),
            gre(&json!([
                {"type": "GREMessageType_ConnectResp", "systemSeatIds": [2], "msgId": 1,
                 "connectResp": {"deckMessage": {"deckCards": [9, 9, 7], "sideboardCards": [7]}}},
                {"type": "GREMessageType_GameStateMessage", "systemSeatIds": [2], "msgId": 2,
                 "gameStateMessage": {"type": "GameStateType_Full", "gameStateId": 1,
                    "gameInfo": {"gameNumber": 1, "stage": "GameStage_Play"},
                    "players": [{"systemSeatNumber": 1, "lifeTotal": 20, "teamId": 1},
                                {"systemSeatNumber": 2, "lifeTotal": 20, "teamId": 2}],
                    "turnInfo": {"phase": "Phase_Main1", "turnNumber": 1, "activePlayer": 2, "priorityPlayer": 2},
                    "zones": [{"zoneId": 28, "type": "ZoneType_Battlefield"},
                              {"zoneId": 35, "type": "ZoneType_Hand", "ownerSeatId": 2, "objectInstanceIds": [100]}],
                    "gameObjects": [{"instanceId": 100, "grpId": 9, "type": "GameObjectType_Card", "zoneId": 35,
                                     "ownerSeatId": 2, "controllerSeatId": 2, "cardTypes": ["CardType_Land"]}]}},
                {"type": "GREMessageType_ActionsAvailableReq", "systemSeatIds": [2], "msgId": 3, "gameStateId": 1,
                 "prompt": {"promptId": 2},
                 "actionsAvailableReq": {"actions": [
                    {"actionType": "ActionType_Play", "grpId": 9, "instanceId": 100, "facetId": 100, "shouldStop": true},
                    {"actionType": "ActionType_Pass"}]}},
            ])),
            // Hover noise, then the answer.
            client(&json!({"type": "ClientMessageType_UIMessage", "uiMessage": {}})),
            client(
                &json!({"type": "ClientMessageType_PerformActionResp", "gameStateId": 1, "respId": 3,
                "performActionResp": {"actions": [{"actionType": "ActionType_Play", "grpId": 9, "instanceId": 100}]}}),
            ),
            gre(&json!([
                {"type": "GREMessageType_GameStateMessage", "systemSeatIds": [2], "msgId": 4,
                 "gameStateMessage": {"type": "GameStateType_Diff", "gameStateId": 2, "prevGameStateId": 1,
                    "zones": [{"zoneId": 28, "type": "ZoneType_Battlefield", "objectInstanceIds": [101]},
                              {"zoneId": 35, "type": "ZoneType_Hand", "ownerSeatId": 2}],
                    "gameObjects": [{"instanceId": 101, "grpId": 9, "type": "GameObjectType_Card", "zoneId": 28,
                                     "ownerSeatId": 2, "controllerSeatId": 2, "cardTypes": ["CardType_Land"]}],
                    "diffDeletedInstanceIds": [100],
                    "annotations": [
                        {"id": 1, "affectedIds": [100], "type": ["AnnotationType_ObjectIdChanged"],
                         "details": [{"key": "orig_id", "valueInt32": [100]}, {"key": "new_id", "valueInt32": [101]}]},
                        {"id": 2, "affectedIds": [101], "type": ["AnnotationType_ZoneTransfer"],
                         "details": [{"key": "zone_src", "valueInt32": [35]}, {"key": "zone_dest", "valueInt32": [28]},
                                     {"key": "category", "valueString": ["PlayLand"]}]},
                        {"id": 3, "affectorId": 2, "affectedIds": [101], "type": ["AnnotationType_UserActionTaken"],
                         "details": [{"key": "actionType", "valueInt32": [3]}, {"key": "abilityGrpId", "valueInt32": [0]}]}]}},
                // Timers only: no new frame.
                {"type": "GREMessageType_GameStateMessage", "systemSeatIds": [2], "msgId": 5,
                 "gameStateMessage": {"type": "GameStateType_Diff", "gameStateId": 3, "prevGameStateId": 2}},
                {"type": "GREMessageType_GameStateMessage", "systemSeatIds": [2], "msgId": 6,
                 "gameStateMessage": {"type": "GameStateType_Diff", "gameStateId": 4, "prevGameStateId": 3,
                    "gameInfo": {"gameNumber": 1, "stage": "GameStage_GameOver", "results": [
                        {"scope": "MatchScope_Game", "result": "ResultType_WinLoss", "winningTeamId": 2,
                         "reason": "ResultReason_Concede"}]},
                    "players": [{"systemSeatNumber": 1, "lifeTotal": 20, "teamId": 1, "status": "PlayerStatus_PendingLoss"}]}},
            ])),
            msg(json!({"matchGameRoomStateChangedEvent": {"gameRoomInfo": {
                "gameRoomConfig": {"matchId": "m"}, "stateType": "MatchGameRoomStateType_MatchCompleted",
                "finalMatchResult": {"resultList": [
                    {"scope": "MatchScope_Game", "winningTeamId": 2},
                    {"scope": "MatchScope_Match", "winningTeamId": 2}]}}}})),
            Record::End {
                reason: EndReason::Completed,
            },
        ]
    }

    #[test]
    fn builds_a_trace() {
        let t = build(&mini_match(), &TestNames);
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!((t.match_id.as_str(), t.started_ms), ("m", 5));
        assert_eq!(t.event.as_deref(), Some("Play"));
        assert_eq!(t.local_seat, Some(2));
        assert_eq!(t.winner, Some(2));
        assert_eq!(t.end, Some(EndReason::Completed));
        assert_eq!(t.player_name(2), "Me");

        let g = &t.games[0];
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
        let deck = g.deck.as_ref().unwrap();
        assert_eq!(
            (deck.main[0].count, deck.main[0].name.as_deref()),
            (2, Some("Forest"))
        );
        assert_eq!(deck.sideboard.len(), 1);
        assert_eq!(
            g.result,
            Some(GameResult {
                winner: Some(2),
                reason: "Concede".into()
            })
        );
        // Full state, the land drop, and the game-over state; the
        // timers-only diff adds nothing.
        assert_eq!(
            g.frames.iter().map(|f| f.state_id).collect::<Vec<_>>(),
            [1, 2, 4]
        );

        let d = &g.frames[0].decisions[0];
        assert_eq!((d.kind.as_str(), d.seat), ("ActionsAvailable", 2));
        let offered = &d.request["actionsAvailableReq"]["actions"];
        assert_eq!(
            offered[0],
            json!({"actionType": "Play", "grpId": 9, "name": "Forest", "instanceId": 100})
        );
        assert_eq!(offered[1]["actionType"], "Pass");
        assert_eq!(d.responses.len(), 1);
        assert_eq!(d.responses[0].kind, "PerformActionResp");
        assert_eq!(
            d.responses[0].body["performActionResp"]["actions"][0]["name"],
            "Forest"
        );

        let f = &g.frames[1];
        assert_eq!(f.state.battlefield[0].name.as_deref(), Some("Forest"));
        let transfer = &f.events[1];
        assert_eq!(transfer.kind, "ZoneTransfer");
        assert_eq!(
            transfer.targets[0],
            Ref {
                id: 101,
                name: Some("Forest".into())
            }
        );
        assert_eq!(transfer.details["zone_src"], "hand:2");
        assert_eq!(transfer.details["zone_dest"], "battlefield");
        let action = &f.events[2];
        assert_eq!(
            action.source,
            Some(Ref {
                id: 2,
                name: Some("@Me".into())
            })
        );
        assert_eq!(action.details["actionType"], "Play");

        assert_eq!(
            t.card_names().into_iter().collect::<Vec<_>>(),
            [
                ("Forest".to_string(), false),
                ("Grizzly Bears".to_string(), false)
            ]
        );
    }

    #[test]
    fn enum_values() {
        assert!(is_enum_value("ActionType_Activate_Mana"));
        assert!(is_enum_value("Phase_Main1"));
        assert!(!is_enum_value("oUoU"));
        assert!(!is_enum_value("Hallowed Fountain"));
        assert!(!is_enum_value("a_b"));
    }
}
