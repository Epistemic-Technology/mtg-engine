//! Rebuilding the game state from Arena's messages.
//!
//! The rules engine sends one full state at the start of each game and
//! diffs afterwards. [`Tracker`] applies them in order and can produce a
//! normalized [`Snapshot`] of the whole game at any point: the form an
//! engine state is compared against.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::cards::Names;

/// Strip the enum-type prefix Arena puts on every enum value:
/// `CardType_Creature` -> `Creature`.
pub fn bare(s: &str) -> &str {
    s.split_once('_').map_or(s, |(_, rest)| rest)
}

/// `gameStateMessage` as sent by the rules engine.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameStateMessage {
    #[serde(rename = "type")]
    pub kind: String,
    pub game_state_id: u32,
    pub prev_game_state_id: Option<u32>,
    pub game_info: Option<Value>,
    pub players: Vec<Player>,
    pub turn_info: Option<TurnInfo>,
    pub zones: Vec<Zone>,
    pub game_objects: Vec<GameObject>,
    pub annotations: Vec<Annotation>,
    pub persistent_annotations: Vec<Annotation>,
    pub diff_deleted_instance_ids: Vec<u32>,
    pub diff_deleted_persistent_annotation_ids: Vec<u32>,
}

impl GameStateMessage {
    pub fn is_full(&self) -> bool {
        self.kind == "GameStateType_Full"
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Player {
    pub system_seat_number: u32,
    pub life_total: i32,
    pub team_id: u32,
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TurnInfo {
    pub phase: Option<String>,
    pub step: Option<String>,
    pub turn_number: u32,
    pub active_player: Option<u32>,
    pub priority_player: Option<u32>,
    pub decision_player: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Zone {
    pub zone_id: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub owner_seat_id: Option<u32>,
    pub object_instance_ids: Vec<u32>,
}

impl Zone {
    /// `battlefield`, `stack`, or `hand:2` for per-player zones.
    pub fn label(&self) -> String {
        let name = bare(&self.kind).to_lowercase();
        match self.owner_seat_id {
            Some(seat) => format!("{name}:{seat}"),
            None => name,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[allow(clippy::struct_excessive_bools)] // mirrors Arena's message
pub struct GameObject {
    pub instance_id: u32,
    pub grp_id: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub zone_id: u32,
    pub owner_seat_id: u32,
    pub controller_seat_id: u32,
    pub card_types: Vec<String>,
    pub subtypes: Vec<String>,
    pub super_types: Vec<String>,
    pub color: Vec<String>,
    #[serde(deserialize_with = "int_value")]
    pub power: Option<i32>,
    #[serde(deserialize_with = "int_value")]
    pub toughness: Option<i32>,
    #[serde(deserialize_with = "int_value")]
    pub loyalty: Option<i32>,
    #[serde(deserialize_with = "int_value")]
    pub defense: Option<i32>,
    pub damage: u32,
    pub is_tapped: bool,
    pub has_summoning_sickness: bool,
    pub is_facedown: bool,
    pub is_copy: bool,
    pub attack_state: Option<String>,
    pub block_state: Option<String>,
    pub attack_info: Option<Value>,
    pub block_info: Option<Value>,
    /// For an ability on the stack, the object it came from.
    pub parent_id: Option<u32>,
    /// For abilities and tokens, the card that made them.
    pub object_source_grp_id: Option<u32>,
}

/// Arena writes numbers that can be modified as `{"value": 3}`.
fn int_value<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Plain(i32),
        Wrapped {
            #[serde(default)]
            value: i32,
        },
    }
    Ok(Option::<Raw>::deserialize(d)?.map(|r| match r {
        Raw::Plain(v) | Raw::Wrapped { value: v } => v,
    }))
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Annotation {
    pub id: u32,
    pub affector_id: Option<u32>,
    pub affected_ids: Vec<u32>,
    #[serde(rename = "type")]
    pub kinds: Vec<String>,
    pub details: Vec<Value>,
}

impl Annotation {
    pub fn is(&self, kind: &str) -> bool {
        self.kinds.iter().any(|k| bare(k) == kind)
    }

    /// Details as a map; each value is a scalar, or an array if the detail
    /// had several values.
    pub fn detail_map(&self) -> BTreeMap<String, Value> {
        let mut out = BTreeMap::new();
        for d in &self.details {
            let Some(key) = d.get("key").and_then(Value::as_str) else {
                continue;
            };
            let value = d
                .as_object()
                .into_iter()
                .flatten()
                .find(|(k, _)| k.starts_with("value"))
                .map_or(Value::Null, |(_, v)| match v.as_array() {
                    Some(a) if a.len() == 1 => a[0].clone(),
                    _ => v.clone(),
                });
            out.insert(key.to_string(), value);
        }
        out
    }

    pub fn detail_u32(&self, key: &str) -> Option<u32> {
        let v = self.detail_map().remove(key)?;
        u32::try_from(v.as_i64()?).ok()
    }
}

/// The accumulated state of one game.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    pub state_id: u32,
    pub game_info: Value,
    pub players: BTreeMap<u32, Player>,
    pub turn: TurnInfo,
    pub zones: BTreeMap<u32, Zone>,
    pub objects: BTreeMap<u32, GameObject>,
    pub persistent: BTreeMap<u32, Annotation>,
}

impl Tracker {
    /// Apply a state message. Returns a warning if a diff does not follow
    /// from the state held (messages were lost); it is applied regardless.
    pub fn apply(&mut self, msg: &GameStateMessage) -> Option<String> {
        let mut warning = None;
        if msg.is_full() {
            *self = Tracker::default();
        } else if let Some(prev) = msg.prev_game_state_id
            && prev != self.state_id
        {
            warning = Some(format!(
                "state {} is a diff from {prev}, but the last state seen was {}",
                msg.game_state_id, self.state_id
            ));
        }
        self.state_id = msg.game_state_id;
        if let Some(info) = &msg.game_info {
            self.game_info = info.clone();
        }
        for p in &msg.players {
            self.players.insert(p.system_seat_number, p.clone());
        }
        if let Some(turn) = &msg.turn_info {
            self.turn = turn.clone();
        }
        for z in &msg.zones {
            self.zones.insert(z.zone_id, z.clone());
        }
        for id in &msg.diff_deleted_instance_ids {
            self.objects.remove(id);
        }
        for o in &msg.game_objects {
            self.objects.insert(o.instance_id, o.clone());
        }
        for id in &msg.diff_deleted_persistent_annotation_ids {
            self.persistent.remove(id);
        }
        for a in &msg.persistent_annotations {
            self.persistent.insert(a.id, a.clone());
        }
        warning
    }

    pub fn zone_label(&self, zone_id: u32) -> Option<String> {
        self.zones.get(&zone_id).map(Zone::label)
    }

    /// The seat on a team (teams are single players in a duel).
    pub fn seat_of_team(&self, team_id: u32) -> Option<u32> {
        self.players
            .values()
            .find(|p| p.team_id == team_id)
            .map(|p| p.system_seat_number)
    }

    fn zone_objects(&self, zone: &Zone, names: &dyn Names) -> Vec<Object> {
        zone.object_instance_ids
            .iter()
            .map(|id| self.object(*id, zone.owner_seat_id, names))
            .collect()
    }

    /// The normalized form of one object. Ids with no object data are cards
    /// the recording player cannot see.
    pub fn object(&self, id: u32, zone_owner: Option<u32>, names: &dyn Names) -> Object {
        let Some(o) = self.objects.get(&id) else {
            return Object {
                id,
                kind: "Hidden".to_string(),
                owner: zone_owner.unwrap_or(0),
                controller: zone_owner.unwrap_or(0),
                ..Object::default()
            };
        };
        let strip = |v: &[String]| v.iter().map(|s| bare(s).to_string()).collect::<Vec<_>>();
        let mut out = Object {
            id,
            grp: (o.grp_id != 0).then_some(o.grp_id),
            kind: bare(&o.kind).to_string(),
            owner: o.owner_seat_id,
            controller: o.controller_seat_id,
            types: strip(&o.card_types),
            subtypes: strip(&o.subtypes),
            supertypes: strip(&o.super_types),
            colors: strip(&o.color),
            power: o.power,
            toughness: o.toughness,
            loyalty: o.loyalty,
            defense: o.defense,
            damage: o.damage,
            tapped: o.is_tapped,
            summoning_sick: o.has_summoning_sickness,
            face_down: o.is_facedown,
            copy: o.is_copy,
            ..Object::default()
        };
        if o.kind == "GameObjectType_Ability" {
            out.source = o.parent_id;
            out.name = o.object_source_grp_id.and_then(|g| names.card(g));
            out.text = names.ability(o.grp_id);
        } else {
            out.name = names.card(o.grp_id);
        }
        if o.attack_state.as_deref() == Some("AttackState_Attacking") {
            out.attacking = Some(
                o.attack_info
                    .as_ref()
                    .and_then(|i| i["targetId"].as_u64())
                    .and_then(|t| u32::try_from(t).ok())
                    .unwrap_or(0),
            );
        }
        if o.block_state.as_deref() == Some("BlockState_Blocking") {
            out.blocking = o
                .block_info
                .as_ref()
                .and_then(|i| i["attackerIds"].as_array())
                .into_iter()
                .flatten()
                .filter_map(|v| u32::try_from(v.as_u64()?).ok())
                .collect();
        }
        for a in self.persistent.values() {
            if a.is("Counter") && a.affected_ids.contains(&id) {
                let d = a.detail_map();
                let kind = d.get("counter_type").and_then(Value::as_u64).unwrap_or(0);
                let kind = u32::try_from(kind).unwrap_or(0);
                let name = names.counter(kind).unwrap_or_else(|| format!("#{kind}"));
                let count = d.get("count").and_then(Value::as_i64).unwrap_or(0);
                *out.counters.entry(name).or_insert(0) += count;
            } else if a.is("TargetSpec") && a.affector_id == Some(id) {
                out.targets.extend(&a.affected_ids);
            } else if a.is("Attachment") && a.affector_id == Some(id) {
                out.attached_to = a.affected_ids.first().copied();
            }
        }
        out
    }

    /// The whole game as the recording player sees it.
    pub fn snapshot(&self, names: &dyn Names) -> Snapshot {
        let mut snap = Snapshot {
            turn: Turn {
                number: self.turn.turn_number,
                active: self.turn.active_player,
                phase: self.turn.phase.as_deref().map(|s| bare(s).to_string()),
                step: self.turn.step.as_deref().map(|s| bare(s).to_string()),
                priority: self.turn.priority_player,
                decision: self.turn.decision_player,
            },
            players: self
                .players
                .values()
                .map(|p| PlayerState {
                    seat: p.system_seat_number,
                    life: p.life_total,
                    status: bare(&p.status).to_string(),
                    ..PlayerState::default()
                })
                .collect(),
            ..Snapshot::default()
        };
        for zone in self.zones.values() {
            let player = zone
                .owner_seat_id
                .and_then(|seat| snap.players.iter_mut().find(|p| p.seat == seat));
            match (bare(&zone.kind), player) {
                ("Battlefield", _) => snap.battlefield = self.zone_objects(zone, names),
                ("Stack", _) => snap.stack = self.zone_objects(zone, names),
                ("Exile", _) => snap.exile = self.zone_objects(zone, names),
                ("Command", _) => snap.command = self.zone_objects(zone, names),
                ("Hand", Some(p)) => p.hand = self.zone_objects(zone, names),
                ("Graveyard", Some(p)) => p.graveyard = self.zone_objects(zone, names),
                ("Library", Some(p)) => p.library = zone.object_instance_ids.len(),
                _ => {}
            }
        }
        snap
    }
}

/// A complete game state, from the recording player's point of view.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub turn: Turn,
    pub players: Vec<PlayerState>,
    pub battlefield: Vec<Object>,
    /// Top of the stack first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<Object>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exile: Vec<Object>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<Object>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub number: u32,
    /// Seat of the active player.
    pub active: Option<u32>,
    pub phase: Option<String>,
    pub step: Option<String>,
    /// Seat of the player with priority.
    pub priority: Option<u32>,
    /// Seat of the player the game is waiting on.
    pub decision: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub seat: u32,
    pub life: i32,
    pub status: String,
    /// Cards in hand; the opponent's are `Hidden` unless revealed.
    pub hand: Vec<Object>,
    /// Number of cards in the library.
    pub library: usize,
    pub graveyard: Vec<Object>,
}

// `skip_serializing_if` passes a reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// A card, token, or ability. Arena gives an object a new `id` each time it
/// changes zones.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // independent flags of a permanent
pub struct Object {
    pub id: u32,
    /// `Card`, `Token`, `Ability`, or `Hidden` for a card the recording
    /// player cannot see.
    pub kind: String,
    /// Arena's id for the card printing (or, for an ability, the ability).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grp: Option<u32>,
    /// English card name; for an ability, the name of its source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub owner: u32,
    pub controller: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtypes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supertypes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defense: Option<i32>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub damage: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub tapped: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub summoning_sick: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub face_down: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub copy: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub counters: BTreeMap<String, i64>,
    /// The id of the player (seat) or permanent this creature attacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacking: Option<u32>,
    /// Ids of the attackers this creature blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocking: Vec<u32>,
    /// What an Aura or Equipment is attached to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_to: Option<u32>,
    /// For an ability on the stack, the object it came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<u32>,
    /// For an ability, its rules text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Ids of the objects or players (seats) a spell or ability targets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cards::NoNames;

    fn msg(v: Value) -> GameStateMessage {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn full_then_diffs() {
        let mut t = Tracker::default();
        assert!(t.apply(&msg(json!({
            "type": "GameStateType_Full", "gameStateId": 1,
            "players": [
                {"systemSeatNumber": 1, "lifeTotal": 20, "teamId": 1},
                {"systemSeatNumber": 2, "lifeTotal": 20, "teamId": 2}],
            "zones": [
                {"zoneId": 28, "type": "ZoneType_Battlefield"},
                {"zoneId": 31, "type": "ZoneType_Hand", "ownerSeatId": 1, "objectInstanceIds": [100, 101]},
                {"zoneId": 35, "type": "ZoneType_Hand", "ownerSeatId": 2, "objectInstanceIds": [200]},
                {"zoneId": 32, "type": "ZoneType_Library", "ownerSeatId": 1, "objectInstanceIds": [102, 103, 104]}],
            "gameObjects": [
                {"instanceId": 200, "grpId": 7, "type": "GameObjectType_Card", "zoneId": 35,
                 "ownerSeatId": 2, "controllerSeatId": 2, "cardTypes": ["CardType_Creature"],
                 "power": {"value": 2}, "toughness": {"value": 2}}],
        }))).is_none());

        // The creature moves to the battlefield under a new id and gets a
        // counter; the opponent loses life.
        assert!(t.apply(&msg(json!({
            "type": "GameStateType_Diff", "gameStateId": 2, "prevGameStateId": 1,
            "turnInfo": {"phase": "Phase_Main1", "turnNumber": 1, "activePlayer": 2, "priorityPlayer": 2},
            "players": [{"systemSeatNumber": 1, "lifeTotal": 17, "teamId": 1}],
            "zones": [
                {"zoneId": 28, "type": "ZoneType_Battlefield", "objectInstanceIds": [201]},
                {"zoneId": 35, "type": "ZoneType_Hand", "ownerSeatId": 2}],
            "gameObjects": [
                {"instanceId": 201, "grpId": 7, "type": "GameObjectType_Card", "zoneId": 28,
                 "ownerSeatId": 2, "controllerSeatId": 2, "cardTypes": ["CardType_Creature"],
                 "power": {"value": 3}, "toughness": {"value": 3}, "isTapped": true}],
            "diffDeletedInstanceIds": [200],
            "persistentAnnotations": [
                {"id": 5, "affectorId": 4002, "affectedIds": [201], "type": ["AnnotationType_Counter"],
                 "details": [{"key": "count", "type": "KeyValuePairValueType_int32", "valueInt32": [1]},
                             {"key": "counter_type", "type": "KeyValuePairValueType_int32", "valueInt32": [1]}]}],
        }))).is_none());

        let s = t.snapshot(&NoNames);
        assert_eq!(s.turn.phase.as_deref(), Some("Main1"));
        assert_eq!(s.turn.active, Some(2));
        assert_eq!(s.players[0].life, 17);
        assert_eq!(s.players[0].library, 3);
        assert_eq!(s.players[0].hand.len(), 2);
        assert_eq!(s.players[0].hand[0].kind, "Hidden");
        assert_eq!(s.players[0].hand[0].owner, 1);
        assert!(s.players[1].hand.is_empty());
        let c = &s.battlefield[0];
        assert_eq!(
            (c.id, c.power, c.toughness, c.tapped),
            (201, Some(3), Some(3), true)
        );
        assert_eq!(c.types, ["Creature"]);
        assert_eq!(c.counters["#1"], 1);

        // Counter annotation deleted; a gap in state ids is reported.
        let warning = t.apply(&msg(json!({
            "type": "GameStateType_Diff", "gameStateId": 5, "prevGameStateId": 4,
            "diffDeletedPersistentAnnotationIds": [5],
        })));
        assert!(warning.unwrap().contains("diff from 4"));
        assert!(t.snapshot(&NoNames).battlefield[0].counters.is_empty());
    }

    #[test]
    fn detail_values() {
        let a: Annotation = serde_json::from_value(json!({
            "id": 1, "type": ["AnnotationType_ZoneTransfer"],
            "details": [
                {"key": "zone_src", "type": "KeyValuePairValueType_int32", "valueInt32": [27]},
                {"key": "category", "type": "KeyValuePairValueType_string", "valueString": ["Resolve"]},
                {"key": "OldIds", "type": "KeyValuePairValueType_int32", "valueInt32": [1, 2]},
                {"key": "empty", "type": "KeyValuePairValueType_int32"}],
        }))
        .unwrap();
        let d = a.detail_map();
        assert_eq!(d["zone_src"], 27);
        assert_eq!(d["category"], "Resolve");
        assert_eq!(d["OldIds"], json!([1, 2]));
        assert_eq!(d["empty"], Value::Null);
        assert!(a.is("ZoneTransfer"));
        assert_eq!(a.detail_u32("zone_src"), Some(27));
    }
}
