//! What a player is entitled to see (CR 400.2, 401.2): the information set
//! for that seat. Agents receive views, never the full state.

use serde::{Deserialize, Serialize};

use crate::card::CardRef;
use crate::ids::{ObjId, PlayerId};
use crate::mana::ManaPool;
use crate::state::{GameState, Target, Zone};
use crate::turn::Step;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectView {
    pub id: ObjId,
    pub def: CardRef,
    pub name: String,
    pub controller: PlayerId,
    pub tapped: bool,
    pub damage: u32,
    pub power: i32,
    pub toughness: i32,
    pub summoning_sick: bool,
    pub attacking: Option<Target>,
    pub blocking: Option<ObjId>,
    /// For spells on the stack.
    pub targets: Vec<Target>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerView {
    pub id: PlayerId,
    pub life: i32,
    pub library_count: usize,
    pub hand_count: usize,
    /// Cards in hand, present only when the viewer may see this hand.
    pub hand: Option<Vec<ObjectView>>,
    pub graveyard: Vec<ObjectView>,
    pub exile: Vec<ObjectView>,
    pub mana_pool: ManaPool,
    pub lands_played_this_turn: u32,
    pub lost: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameView {
    pub viewer: PlayerId,
    pub turn: u32,
    pub active_player: PlayerId,
    pub step: Step,
    pub priority: PlayerId,
    pub players: Vec<PlayerView>,
    /// Battlefield in timestamp order.
    pub battlefield: Vec<ObjectView>,
    /// Bottom to top.
    pub stack: Vec<ObjectView>,
}

impl GameState {
    /// Whether `viewer` may see the identity of `obj` (CR 400.2, 401.2).
    /// Hidden zones are the library and other players' hands; M1 has no
    /// reveal or look effects.
    pub fn visible_to(&self, viewer: PlayerId, obj: ObjId) -> bool {
        self.objects
            .get(obj)
            .is_some_and(|o| o.zone.is_public() || (o.zone == Zone::Hand && o.owner == viewer))
    }

    fn object_view(&self, id: ObjId) -> ObjectView {
        let o = &self.objects[id];
        let c = self.characteristics(id).expect("object exists");
        let combat = self.combat.as_ref();
        let mut name = self.pool.get(o.def).name.clone();
        if o.kind == crate::state::ObjKind::Ability {
            name.push_str(" (ability)");
        }
        ObjectView {
            id,
            def: o.def,
            name,
            controller: o.controller,
            tapped: o.tapped,
            damage: o.damage,
            power: c.power,
            toughness: c.toughness,
            summoning_sick: o.zone == Zone::Battlefield
                && c.is_creature()
                && !o.controlled_since_turn_start,
            attacking: combat.and_then(|cb| cb.attacker(id)).map(|a| a.attacking),
            blocking: combat.and_then(|cb| cb.blocked_by(id)),
            targets: o.targets.iter().copied().collect(),
        }
    }

    /// The game as `viewer` sees it.
    pub fn observe(&self, viewer: PlayerId) -> GameView {
        let players = (0..self.num_players())
            .map(|i| {
                let id = PlayerId(u8::try_from(i).unwrap_or(0));
                let p = self.player(id);
                PlayerView {
                    id,
                    life: p.life,
                    library_count: p.library.len(),
                    hand_count: p.hand.len(),
                    hand: (id == viewer)
                        .then(|| p.hand.iter().map(|o| self.object_view(*o)).collect()),
                    graveyard: p.graveyard.iter().map(|o| self.object_view(*o)).collect(),
                    exile: p.exile.iter().map(|o| self.object_view(*o)).collect(),
                    mana_pool: p.mana_pool,
                    lands_played_this_turn: p.lands_played_this_turn,
                    lost: p.lost.is_some(),
                }
            })
            .collect();
        GameView {
            viewer,
            turn: self.turn_number,
            active_player: self.active_player,
            step: self.step,
            priority: self.priority,
            players,
            battlefield: self
                .battlefield
                .iter()
                .map(|o| self.object_view(*o))
                .collect(),
            stack: self.stack.iter().map(|o| self.object_view(*o)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::card::m1_pool;
    use crate::engine::{Engine, GameConfig};

    #[test]
    fn view_hides_opponent_hand_and_libraries() {
        let pool = Arc::new(m1_pool());
        let forest = pool.lookup("Forest").unwrap();
        let cfg = GameConfig {
            decks: vec![vec![forest; 20], vec![forest; 20]],
            starting_life: 20,
            seed: 1,
            logging: false,
        };
        let e = Engine::new(pool, &cfg);
        let v = e.state().observe(PlayerId(0));
        assert_eq!(v.players[0].hand.as_ref().map(Vec::len), Some(7));
        assert!(v.players[1].hand.is_none());
        assert_eq!(v.players[1].hand_count, 7);
        assert_eq!(v.players[1].library_count, 13);
        let own = e.state().player(PlayerId(0)).hand[0];
        let theirs = e.state().player(PlayerId(1)).hand[0];
        assert!(e.state().visible_to(PlayerId(0), own));
        assert!(!e.state().visible_to(PlayerId(0), theirs));
        assert!(!e.state().visible_to(PlayerId(1), own));
    }
}
