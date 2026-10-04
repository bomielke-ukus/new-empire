//! The two victories that end a match on a clock (`docs/02` §10): a Wonder
//! held ten minutes (`GD-WIN-02`), and every relic held ten minutes
//! (`GD-WIN-03`). Conquest (`GD-WIN-01`) needs no clock; it is
//! [`Simulation::standing`].
//!
//! The clocks are read once a second. Everyone may see them: a Wonder is
//! announced to all the moment it is finished, and so is a side that holds
//! every relic.

use crate::battle::Event;
use crate::command::PlayerId;
use crate::entity::EntityId;
use crate::hash::{HashState, StateHasher};
use crate::kinds;
use crate::simulation::{Simulation, TICKS_PER_SECOND};
use serde::{Deserialize, Serialize};

/// How long a Wonder, or every relic, must be held: ten minutes.
pub const HOLD_TICKS: u64 = 10 * 60 * TICKS_PER_SECOND as u64;

/// How a match was won.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Victory {
    /// Every other side is out (`GD-WIN-01`).
    Conquest,
    /// A Wonder stood ten minutes (`GD-WIN-02`).
    Wonder,
    /// Every relic was held ten minutes (`GD-WIN-03`).
    Relics,
}

/// The clocks, and what they decided.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Clocks {
    /// Every finished Wonder standing, its owner, and the tick its clock
    /// started, oldest first.
    pub(crate) wonders: Vec<(EntityId, PlayerId, u64)>,
    /// The side holding every relic in its Temples, and since when.
    pub(crate) relics: Option<(PlayerId, u64)>,
    /// A side that won on a clock, and which.
    pub(crate) won: Option<(PlayerId, Victory)>,
}

impl Clocks {
    /// Nothing running and nothing decided: a match before any of this,
    /// whose hash it leaves alone.
    pub(crate) fn is_quiet(&self) -> bool {
        self.wonders.is_empty() && self.relics.is_none() && self.won.is_none()
    }
}

impl HashState for Clocks {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.wonders.len() as u32);
        for (id, owner, since) in &self.wonders {
            h.write(id);
            h.write_u8(*owner);
            h.write_u64(*since);
        }
        match self.relics {
            Some((p, since)) => {
                h.write_u8(1);
                h.write_u8(p);
                h.write_u64(since);
            }
            None => h.write_u8(0),
        }
        match self.won {
            Some((p, v)) => {
                h.write_u8(1);
                h.write_u8(p);
                h.write_u8(v as u8);
            }
            None => h.write_u8(0),
        }
    }
}

impl Simulation {
    /// How the match was won, and by whom, once it has been.
    pub fn victory(&self) -> Option<(PlayerId, Victory)> {
        if let Some(w) = self.clocks.won {
            return Some(w);
        }
        self.conquered().map(|p| (p, Victory::Conquest))
    }

    /// Every Wonder's clock: its owner, and the ticks it has still to
    /// stand, oldest first.
    pub fn wonder_clocks(&self) -> Vec<(EntityId, PlayerId, u64)> {
        self.clocks
            .wonders
            .iter()
            .map(|&(id, owner, since)| (id, owner, (since + HOLD_TICKS).saturating_sub(self.tick)))
            .collect()
    }

    /// The side holding every relic, and the ticks left on its clock.
    pub fn relic_clock(&self) -> Option<(PlayerId, u64)> {
        self.clocks
            .relics
            .map(|(p, since)| (p, (since + HOLD_TICKS).saturating_sub(self.tick)))
    }

    /// Reads the clocks, once a second; a clock run out decides the match.
    pub(crate) fn victory_clocks(&mut self) {
        if self.players.len() < 2
            || self.clocks.won.is_some()
            || !self.tick.is_multiple_of(TICKS_PER_SECOND as u64)
        {
            return;
        }
        let tick = self.tick;
        let in_play = |sim: &Simulation, p: PlayerId| {
            sim.players.get(p as usize).is_some_and(|pl| !pl.resigned)
        };

        // The Wonders: a fallen one's clock stops; a newly finished one's
        // starts, announced to everyone.
        let world = &self.world;
        self.clocks.wonders.retain(|&(id, _, _)| {
            world.slot(id).is_some_and(|s| {
                world.kind[s.index()] == kinds::WONDER && world.dying[s.index()] == 0
            })
        });
        let fresh: Vec<(EntityId, PlayerId)> = world
            .slots()
            .filter(|s| {
                let i = s.index();
                world.kind[i] == kinds::WONDER
                    && world.dying[i] == 0
                    && world.construction[i].is_none()
            })
            .map(|s| (world.id_at(s), world.owner[s.index()]))
            .filter(|(id, _)| !self.clocks.wonders.iter().any(|w| w.0 == *id))
            .collect();
        for (id, owner) in fresh {
            self.clocks.wonders.push((id, owner, tick));
            if let Some(s) = self.world.slot(id) {
                self.events.push(Event::WonderRaised {
                    owner,
                    pos: self.world.pos[s.index()],
                });
            }
        }

        // The relics: all of them in the Temples of one side.
        let mut holder: Option<PlayerId> = None;
        let mut all = true;
        let mut any = false;
        for s in self.world.slots() {
            let i = s.index();
            if self.world.kind[i] != kinds::RELIC {
                continue;
            }
            any = true;
            let owner = self.world.owner[i];
            let held = self.world.inside[i]
                .and_then(|c| self.relic_temple(c, owner))
                .is_some();
            if !held || holder.is_some_and(|h| h != owner) {
                all = false;
                break;
            }
            holder = Some(owner);
        }
        let holder = holder.filter(|&p| any && all && in_play(self, p));
        let was = self.clocks.relics;
        self.clocks.relics = match (holder, was) {
            (Some(p), Some((q, since))) if p == q => Some((q, since)),
            (Some(p), _) => Some((p, tick)),
            (None, _) => None,
        };
        let (before, after) = (was.map(|w| w.0), self.clocks.relics.map(|r| r.0));
        if before != after {
            if let Some(owner) = before {
                self.events.push(Event::RelicsHeld { owner, held: false });
            }
            if let Some(owner) = after {
                self.events.push(Event::RelicsHeld { owner, held: true });
            }
        }

        // A clock run out: the oldest Wonder first, then the relics.
        let wonder = self
            .clocks
            .wonders
            .iter()
            .find(|&&(_, owner, since)| tick >= since + HOLD_TICKS && in_play(self, owner))
            .map(|&(_, owner, _)| (owner, Victory::Wonder));
        let relics = self
            .clocks
            .relics
            .filter(|&(_, since)| tick >= since + HOLD_TICKS)
            .map(|(p, _)| (p, Victory::Relics));
        self.clocks.won = wonder.or(relics);
    }
}
