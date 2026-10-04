//! Relics (`docs/02` §10 `GD-WIN-03`; `docs/07` D31): objects on the map
//! that only a priest can carry. Carried to a Temple of its side's, a relic
//! earns that side gold for as long as the Temple stands; a side holding
//! every relic in its Temples for ten minutes wins ([`crate::victory`]).
//!
//! A relic is an entity of its own. On the ground it stands on its tile
//! like a bush, nature's, blocking it; carried or held, it is inside its
//! priest or its Temple (the `inside` column a garrison uses) and its
//! owner is theirs. A priest that falls drops it where it fell, and a
//! Temple that falls drops all it held round its rubble.

use crate::entity::{EntityId, Slot};
use crate::fx::Fx;
use crate::kinds::{self, GAIA};
use crate::nav;
use crate::orders::{Nav, NavState, Order};
use crate::simulation::{Simulation, REACH_SLACK};
use crate::vec2::Vec2Fx;

/// Relics on a generated map with starts on it.
pub const RELICS_PER_MAP: usize = 5;

/// Ticks between a held relic's gold: one every two seconds.
pub const RELIC_GOLD_TICKS: u64 = 40;

impl Simulation {
    /// The relic `carrier` holds, if any: a priest's one, or the first of a
    /// Temple's.
    pub fn carried_relic(&self, carrier: EntityId) -> Option<EntityId> {
        self.world
            .slots()
            .find(|s| {
                let i = s.index();
                self.world.kind[i] == kinds::RELIC && self.world.inside[i] == Some(carrier)
            })
            .map(|s| self.world.id_at(s))
    }

    /// How many relics `carrier` holds: a Temple's count, a priest's one.
    pub fn relics_held_by(&self, carrier: EntityId) -> u32 {
        self.world
            .slots()
            .filter(|s| {
                let i = s.index();
                self.world.kind[i] == kinds::RELIC && self.world.inside[i] == Some(carrier)
            })
            .count() as u32
    }

    /// How many relics the Temples of `owner` hold.
    pub fn relics_held(&self, owner: u8) -> u32 {
        self.world
            .slots()
            .filter(|s| {
                let i = s.index();
                self.world.kind[i] == kinds::RELIC
                    && self.world.owner[i] == owner
                    && self.world.inside[i].is_some_and(|c| {
                        self.world
                            .slot(c)
                            .is_some_and(|cs| self.world.kind[cs.index()] == kinds::TEMPLE)
                    })
            })
            .count() as u32
    }

    /// `id` as a relic lying on the ground for the taking.
    pub fn ground_relic(&self, id: EntityId) -> Option<Slot> {
        let s = self.world.slot(id)?;
        let i = s.index();
        (self.world.kind[i] == kinds::RELIC
            && self.world.inside[i].is_none()
            && self.world.dying[i] == 0)
            .then_some(s)
    }

    /// `id` as a Temple of `owner`'s that can take a relic: finished and
    /// standing.
    pub fn relic_temple(&self, id: EntityId, owner: u8) -> Option<Slot> {
        let s = self.world.slot(id)?;
        let i = s.index();
        (self.world.kind[i] == kinds::TEMPLE
            && self.world.owner[i] == owner
            && self.world.dying[i] == 0
            && self.world.construction[i].is_none())
        .then_some(s)
    }

    /// The nearest Temple of priest `i`'s side's that can take a relic,
    /// ties to the lowest slot.
    fn nearest_temple(&self, i: usize) -> Option<Slot> {
        let owner = self.world.owner[i];
        let here = self.world.pos[i];
        self.world
            .slots()
            .filter(|s| self.relic_temple(self.world.id_at(*s), owner).is_some())
            .min_by_key(|s| (here.distance_sq_raw(self.world.pos[s.index()]), s.index()))
    }

    /// Puts priests on fetching `relic`, or on taking theirs to `temple`.
    pub(crate) fn send_for_relic(&mut self, i: usize, target: EntityId) {
        let me = self.world.id_at(Slot::new(i));
        let owner = self.world.owner[i];
        let order = if self.ground_relic(target).is_some() {
            Order::Relic {
                relic: target,
                temple: None,
            }
        } else if self.relic_temple(target, owner).is_some() {
            let Some(relic) = self.carried_relic(me) else {
                return;
            };
            Order::Relic {
                relic,
                temple: Some(target),
            }
        } else {
            return;
        };
        self.world.order[i] = order;
        self.world.nav[i] = None;
    }

    /// One tick of a relic order: with the relic in hand, to the Temple
    /// and in; without it, to the relic and up. A priest with a relic and
    /// no Temple to take it to stands holding it.
    pub(crate) fn tick_relic(&mut self, slot: Slot, relic: EntityId, temple: Option<EntityId>) {
        let i = slot.index();
        let me = self.world.id_at(slot);
        let owner = self.world.owner[i];
        if let Some(held) = self.carried_relic(me) {
            let to = temple
                .and_then(|t| self.relic_temple(t, owner))
                .or_else(|| self.nearest_temple(i));
            let Some(ts) = to else {
                self.stand(i);
                return;
            };
            if self.within(i, ts, REACH_SLACK) {
                self.deposit(held, ts);
                self.stand(i);
                return;
            }
            self.walk_beside(i, ts);
            return;
        }
        let Some(rs) = self.ground_relic(relic) else {
            self.stand(i);
            return;
        };
        if self.within(i, rs, REACH_SLACK) {
            self.pick_up(i, rs);
            self.world.nav[i] = None;
            return;
        }
        self.walk_beside(i, rs);
    }

    fn stand(&mut self, i: usize) {
        self.world.nav[i] = None;
        self.world.order[i] = Order::Idle;
    }

    /// A step toward a tile beside `target`, as a garrison walks to a
    /// door; as close as it gets and not close enough, it stops.
    fn walk_beside(&mut self, i: usize, target: Slot) {
        match &self.world.nav[i] {
            None => match self.approach(i, target) {
                Some(goal) => {
                    let key = self.field_key_of(target);
                    self.world.nav[i] = Some(Nav::along(goal, Fx::from_ratio(2, 10), key));
                }
                None => self.stand(i),
            },
            Some(n) if matches!(n.state, NavState::Arrived | NavState::Failed) => self.stand(i),
            Some(_) => {}
        }
    }

    /// Priest `i` takes up the relic at `rs`: its tile opens.
    fn pick_up(&mut self, i: usize, rs: Slot) {
        let r = rs.index();
        let (ax, ay) = nav::anchor_tile(self.world.pos[r], 1);
        self.nav.unblock_footprint(ax, ay, 1);
        self.nav.refresh();
        let (w, h) = (self.map.width(), self.map.height());
        self.scratch.vacate(ax, ay, 1, r, w, h);
        self.world.inside[r] = Some(self.world.id_at(Slot::new(i)));
        self.world.owner[r] = self.world.owner[i];
        self.world.pos[r] = self.world.pos[i];
    }

    /// The relic goes into the Temple at `ts`, and is its side's.
    fn deposit(&mut self, relic: EntityId, ts: Slot) {
        let Some(rs) = self.world.slot(relic) else {
            return;
        };
        let r = rs.index();
        self.world.inside[r] = Some(self.world.id_at(ts));
        self.world.owner[r] = self.world.owner[ts.index()];
        self.world.pos[r] = self.world.pos[ts.index()];
    }

    /// Every relic `carrier` holds goes back on the ground round `near`,
    /// nature's again, each on an open tile of its own.
    pub(crate) fn drop_relics_of(&mut self, carrier: EntityId, near: Vec2Fx) {
        while let Some(relic) = self.carried_relic(carrier) {
            let Some(rs) = self.world.slot(relic) else {
                return;
            };
            let r = rs.index();
            let t = nav::tile_of(near);
            let tile = self
                .nav
                .spread(t.0, t.1, 1, None)
                .first()
                .copied()
                .unwrap_or(t);
            self.world.inside[r] = None;
            self.world.owner[r] = GAIA;
            self.world.pos[r] = nav::centre(tile);
            self.nav.block_footprint(tile.0, tile.1, 1);
            self.nav.refresh();
            let (w, h) = (self.map.width(), self.map.height());
            self.scratch.place(tile.0, tile.1, 1, r, w, h);
        }
    }

    /// A held relic earns its side gold, once every [`RELIC_GOLD_TICKS`],
    /// while its Temple stands finished.
    pub(crate) fn relic_gold(&mut self) {
        if !self.tick.is_multiple_of(RELIC_GOLD_TICKS) {
            return;
        }
        let earners: Vec<u8> = self
            .world
            .slots()
            .filter_map(|s| {
                let i = s.index();
                if self.world.kind[i] != kinds::RELIC {
                    return None;
                }
                let owner = self.world.owner[i];
                let temple = self.world.inside[i]?;
                self.relic_temple(temple, owner)?;
                Some(owner)
            })
            .collect();
        for owner in earners {
            if let Some(p) = self.players.get_mut(owner as usize) {
                let gold = kinds::Resource::Gold.index();
                p.stockpile[gold] += 1;
                p.gathered[gold] += 1;
            }
        }
    }
}
