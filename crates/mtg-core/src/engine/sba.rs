//! State-based actions (CR 704) and the game-over check.

use crate::event::Event;
use crate::ids::PlayerId;
use crate::state::{Flow, GameResult, GameState, LossReason, ObjKind, Zone};

impl GameState {
    // ----- state-based actions and game end ------------------------------------------

    /// CR 704.3: perform all applicable state-based actions simultaneously,
    /// repeating until none apply. Returns whether any were performed.
    pub(super) fn sba_loop(&mut self) -> bool {
        let mut any = false;
        loop {
            let mut acted = false;
            // 704.5a, 704.5b
            for i in 0..self.num_players() {
                let p = PlayerId(u8::try_from(i).unwrap_or(0));
                let ps = self.player(p);
                if ps.lost.is_some() {
                    continue;
                }
                let reason = if ps.life <= 0 {
                    Some(LossReason::LifeZeroOrLess)
                } else if ps.drew_from_empty_library {
                    Some(LossReason::DrewFromEmptyLibrary)
                } else {
                    None
                };
                if let Some(reason) = reason {
                    self.player_mut(p).lost = Some(reason);
                    self.emit(Event::PlayerLost { player: p, reason });
                    acted = true;
                }
            }
            // 704.5d, 704.5f, 704.5g, 704.5h: collect first, act simultaneously.
            let mut to_graveyard = Vec::new();
            let mut to_destroy = Vec::new();
            let mut tokens_gone = Vec::new();
            for id in self.objects.keys().collect::<Vec<_>>() {
                let o = &self.objects[id];
                if o.kind == ObjKind::Token && o.zone != Zone::Battlefield {
                    tokens_gone.push(id);
                    continue;
                }
                if o.zone != Zone::Battlefield {
                    continue;
                }
                let Some(c) = self.characteristics(id) else {
                    continue;
                };
                if !c.is_creature() {
                    continue;
                }
                if c.toughness <= 0 {
                    to_graveyard.push(id);
                } else if o.damage >= u32::try_from(c.toughness).unwrap_or(0) || o.deathtouch_damage
                {
                    to_destroy.push(id);
                }
            }
            for id in tokens_gone {
                self.remove_from_zone_list(id);
                self.objects.remove(id);
                acted = true;
            }
            for id in to_graveyard {
                self.move_object(id, Zone::Graveyard, true);
                acted = true;
            }
            for id in to_destroy {
                let before = self.objects.contains_key(id);
                self.destroy(id);
                // Indestructible creatures stay; clear the deathtouch flag so
                // this does not loop forever.
                if before && self.objects.contains_key(id) {
                    self.objects[id].deathtouch_damage = false;
                }
                acted = true;
            }
            if !acted {
                break;
            }
            any = true;
        }
        any
    }

    pub(super) fn check_game_over(&mut self) -> Option<GameResult> {
        if let Some(r) = self.result {
            return Some(r);
        }
        let alive: Vec<PlayerId> = self.players_in_game().collect();
        let result = match alive.len() {
            0 => GameResult::Draw,
            1 => GameResult::Win { winner: alive[0] },
            _ => return None,
        };
        self.result = Some(result);
        self.flow = Flow::GameOver;
        self.emit(Event::GameOver { result });
        Some(result)
    }
}
