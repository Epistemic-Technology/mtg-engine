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
    /// The cards of another player's hand the viewer has seen revealed
    /// and that haven't moved since (CR 701.20).
    #[serde(default)]
    pub revealed: Vec<ObjectView>,
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
    /// Hidden zones are the library and other players' hands; a card
    /// revealed from a hand stays known until it moves.
    pub fn visible_to(&self, viewer: PlayerId, obj: ObjId) -> bool {
        self.objects.get(obj).is_some_and(|o| {
            o.zone.is_public()
                || (o.zone == Zone::Hand && (o.owner == viewer || Self::seen_by(o, viewer)))
        })
    }

    /// Whether `viewer` has seen `o` while it is in a hidden zone
    /// (`GameObject::revealed`).
    fn seen_by(o: &crate::state::GameObject, viewer: PlayerId) -> bool {
        o.revealed & (1 << viewer.index().min(7)) != 0
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
            power: c.power(),
            toughness: c.toughness(),
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
                    revealed: if id == viewer {
                        Vec::new()
                    } else {
                        p.hand
                            .iter()
                            .filter(|o| Self::seen_by(&self.objects[**o], viewer))
                            .map(|o| self.object_view(*o))
                            .collect()
                    },
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

impl GameState {
    /// A stable label for an object: `#<n>` from its arena index. The label
    /// changes only when the object changes zones (it is a new object then,
    /// CR 400.7).
    pub fn label(id: ObjId) -> String {
        use slotmap::Key as _;
        format!("#{}", id.data().as_ffi() & 0xffff_ffff)
    }

    /// Every object whose label is `label`, in any zone the viewer may see.
    pub fn by_label(&self, viewer: PlayerId, label: &str) -> Option<ObjId> {
        let n: u64 = label.trim_start_matches('#').parse().ok()?;
        self.objects.keys().find(|id| {
            use slotmap::Key as _;
            id.data().as_ffi() & 0xffff_ffff == n && self.visible_to(viewer, *id)
        })
    }

    fn describe_object(&self, id: ObjId, viewer: PlayerId) -> String {
        use std::fmt::Write as _;
        let o = &self.objects[id];
        let mut s = format!("{} {}", Self::label(id), self.name_of(id));
        let Some(c) = self.characteristics(id) else {
            return s;
        };
        if o.zone == Zone::Battlefield {
            if c.is_creature() {
                let _ = write!(s, " {}/{}", c.power, c.toughness);
                if o.damage > 0 {
                    let _ = write!(s, " ({} damage)", o.damage);
                }
                if !o.controlled_since_turn_start {
                    s.push_str(" (summoning sick)");
                }
            }
            let kws: Vec<&str> = c.keywords.iter().map(crate::card::Keyword::name).collect();
            if !kws.is_empty() {
                let _ = write!(s, " [{}]", kws.join(", "));
            }
            for (kind, n) in &o.counters {
                let _ = write!(s, " [{n} {kind:?} counters]");
            }
            if o.tapped {
                s.push_str(" [tapped]");
            }
            if let Some(cb) = &self.combat {
                if let Some(a) = cb.attacker(id) {
                    match a.attacking {
                        Target::Player(p) => {
                            let _ = write!(s, " [attacking P{}]", p.0 + 1);
                        }
                        Target::Object(t) => {
                            let _ = write!(s, " [attacking {}]", Self::label(t));
                        }
                    }
                } else if let Some(a) = cb.blocked_by(id) {
                    let _ = write!(s, " [blocking {}]", Self::label(a));
                }
            }
            if let Some(t) = o.attached_to {
                let _ = write!(s, " [attached to {}]", Self::label(t));
            }
        }
        if o.zone == Zone::Stack && !o.targets.is_empty() {
            let targets: Vec<String> = o
                .targets
                .iter()
                .map(|t| match t {
                    Target::Player(p) => format!("P{}", p.0 + 1),
                    Target::Object(id) => Self::label(*id),
                })
                .collect();
            let _ = write!(s, " -> {}", targets.join(", "));
        }
        let _ = viewer;
        s
    }

    /// The game as dense text for `viewer`, with `#n` labels that `act`
    /// calls can name (design §9.2).
    pub fn describe(&self, viewer: PlayerId) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Turn {}, {:?}, P{}'s turn, priority P{}. You are P{}.",
            self.turn_number,
            self.step,
            self.active_player.0 + 1,
            self.priority.0 + 1,
            viewer.0 + 1
        );
        for i in 0..self.num_players() {
            let p = PlayerId(u8::try_from(i).unwrap_or(0));
            let ps = self.player(p);
            let who = if p == viewer { "You" } else { "Opponent" };
            let _ = write!(
                out,
                "{who} (P{}): {} life, {} in hand, {} in library, {} in graveyard, {} in exile",
                p.0 + 1,
                ps.life,
                ps.hand.len(),
                ps.library.len(),
                ps.graveyard.len(),
                ps.exile.len()
            );
            if ps.mana_pool.total() > 0 {
                let _ = write!(out, ", mana pool {:?}", ps.mana_pool);
            }
            if ps.lost.is_some() {
                out.push_str(", has lost");
            }
            out.push('\n');
            let perms: Vec<String> = self
                .permanents_of(p)
                .map(|id| self.describe_object(id, viewer))
                .collect();
            let _ = writeln!(
                out,
                "  battlefield: {}",
                if perms.is_empty() {
                    "(empty)".to_string()
                } else {
                    perms.join("; ")
                }
            );
            let seen: Vec<String> = ps
                .hand
                .iter()
                .filter(|id| p != viewer && Self::seen_by(&self.objects[**id], viewer))
                .map(|id| self.describe_object(*id, viewer))
                .collect();
            if !seen.is_empty() {
                let _ = writeln!(out, "  revealed in hand: {}", seen.join(", "));
            }
            if p == viewer {
                let hand: Vec<String> = ps
                    .hand
                    .iter()
                    .map(|id| self.describe_object(*id, viewer))
                    .collect();
                let _ = writeln!(
                    out,
                    "  hand: {}",
                    if hand.is_empty() {
                        "(empty)".to_string()
                    } else {
                        hand.join(", ")
                    }
                );
            }
            if !ps.graveyard.is_empty() {
                let gy: Vec<String> = ps
                    .graveyard
                    .iter()
                    .map(|id| self.describe_object(*id, viewer))
                    .collect();
                let _ = writeln!(out, "  graveyard: {}", gy.join(", "));
            }
        }
        if !self.stack.is_empty() {
            let items: Vec<String> = self
                .stack
                .iter()
                .rev()
                .map(|id| self.describe_object(*id, viewer))
                .collect();
            let _ = writeln!(out, "Stack (top first): {}", items.join("; "));
        }
        out
    }

    /// Resample everything hidden from `viewer` consistently with what the
    /// viewer knows (design §4.8): the opponent's hand is redrawn from
    /// their hand and library together, except the cards of it the viewer
    /// has seen revealed (CR 701.20), which stay; every library is
    /// shuffled, and the random number generator is reseeded.
    /// Object identities are kept; only zones and orders change. Valid at a
    /// point where the viewer is to act.
    pub fn determinize(&mut self, viewer: PlayerId, seed: u64) {
        use rand::SeedableRng as _;
        use rand::seq::SliceRandom as _;
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        for i in 0..self.num_players() {
            let p = PlayerId(u8::try_from(i).unwrap_or(0));
            if p == viewer {
                self.players[i].library.shuffle(&mut rng);
                continue;
            }
            // Seen cards keep their places; the rest of the hand is
            // redrawn into the other slots.
            let hand = self.players[i].hand.clone();
            let seen: Vec<bool> = hand
                .iter()
                .map(|id| Self::seen_by(&self.objects[*id], viewer))
                .collect();
            let mut unknown: Vec<ObjId> = hand
                .iter()
                .zip(&seen)
                .filter(|(_, s)| !**s)
                .map(|(id, _)| *id)
                .collect();
            let hidden = unknown.len();
            unknown.extend(self.players[i].library.iter().copied());
            unknown.shuffle(&mut rng);
            let library = unknown.split_off(hidden);
            let mut drawn = unknown.into_iter();
            let hand: Vec<ObjId> = hand
                .iter()
                .zip(&seen)
                .map(|(id, s)| if *s { *id } else { drawn.next().unwrap_or(*id) })
                .collect();
            for id in &hand {
                self.objects[*id].zone = Zone::Hand;
            }
            for id in &library {
                self.objects[*id].zone = Zone::Library;
                // A card that was revealed in the hand and is now
                // resampled into the library is no longer known there.
                self.objects[*id].revealed = 0;
            }
            self.players[i].hand = hand;
            self.players[i].library = library;
        }
        // The viewer can't know what later shuffles, die rolls, or random
        // choices will come up either: the generator they draw on is
        // resampled too, so a search sees each chance outcome as a draw
        // from its distribution rather than the real game's next number.
        self.rng = rand_chacha::ChaCha8Rng::seed_from_u64(rand::Rng::random(&mut rng));
        self.bump();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::card::test_pool;
    use crate::engine::{Engine, GameConfig};

    #[test]
    fn view_hides_opponent_hand_and_libraries() {
        let pool = Arc::new(test_pool());
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

    /// A search mustn't know the real game's next die roll or coin flip:
    /// determinizing resamples the random number generator too.
    #[test]
    fn determinize_resamples_the_generator() {
        let pool = Arc::new(test_pool());
        let forest = pool.lookup("Forest").unwrap();
        let cfg = GameConfig {
            decks: vec![vec![forest; 20], vec![forest; 20]],
            starting_life: 20,
            seed: 1,
            logging: false,
        };
        let real = Engine::new(pool, &cfg);
        let mut a = real.clone();
        let mut b = real.clone();
        a.determinize(PlayerId(0), 7);
        b.determinize(PlayerId(0), 7);
        assert!(
            a.state().rng == b.state().rng,
            "the same seed resamples alike"
        );
        assert_ne!(a.state().rng, real.state().rng);
        let mut c = real.clone();
        c.determinize(PlayerId(0), 8);
        assert_ne!(a.state().rng, c.state().rng);
    }
}
