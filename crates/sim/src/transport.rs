//! Transports and trade boats (`docs/02` §5.1, `GD-NAVAL-03`,
//! `GD-NAVAL-04`; `docs/07` D33): a boat that carries ten units over the
//! water, and one that takes wood to another side's Dock and brings gold
//! home.
//!
//! Boarding is garrisoning into something that moves. A walker told to
//! board makes for the land nearest the boat and waits there; the boat,
//! told it has passengers coming, comes in to the water nearest them, and
//! each steps aboard once within reach. Aboard, a unit is inside the boat
//! (the `inside` column a garrison uses) and rides where it goes. Told to
//! unload somewhere, the boat sails to the water nearest that place and
//! puts everyone ashore on the land beside it. A boat that sinks takes
//! everyone aboard down with it; a boat converted brings them over.

use crate::battle::Event;
use crate::entity::{EntityId, Slot};
use crate::fx::Fx;
use crate::kinds::{self, Resource};
use crate::nav::{self, Tile};
use crate::orders::{Nav, NavState, Order};
use crate::simulation::Simulation;
use crate::vec2::Vec2Fx;

/// How far from a boat, in tiles, a walker looks for land to wait on.
const SHORE_LOOK: i32 = 8;

/// How far from the boarding party, in tiles, a boat looks for water to
/// come in to.
const COME_IN_LOOK: i32 = 24;

/// How far from the place asked, in tiles, a boat looks for water to put
/// in at.
const PUT_IN_LOOK: i32 = 16;

/// How far from the hull, in tiles, the land may be for everyone to step
/// ashore.
const ASHORE_REACH: i32 = 3;

/// Wood a trade boat takes out on each trip (`GD-NAVAL-04`).
pub const TRADE_LOAD: i32 = 20;

/// The gold a load fetches at a Dock `tiles` from its side's nearest:
/// more the further it was carried.
pub fn trade_gold(tiles: i32) -> i32 {
    10 + tiles.max(0) * 3 / 4
}

impl Simulation {
    /// One tick of a walker boarding the transport in `bs`: to the land
    /// nearest the boat, re-aimed as the boat comes in, and waiting there.
    /// It steps aboard when within reach, which the caller checks first.
    pub(crate) fn board(&mut self, i: usize, bs: Slot) {
        let Some(from) = self.standing_tile(i) else {
            self.world.order[i] = Order::Idle;
            self.world.nav[i] = None;
            return;
        };
        let bt = nav::tile_of(self.world.pos[bs.index()]);
        let Some(shore) = self
            .nav
            .nearest_passable(bt.0, bt.1, SHORE_LOOK, Some(from))
        else {
            // No land of this walker's anywhere near the boat.
            self.world.order[i] = Order::Idle;
            self.world.nav[i] = None;
            return;
        };
        let goal = nav::centre(shore);
        let stale = match &self.world.nav[i] {
            None => true,
            Some(n) => n.goal.distance(goal) > Fx::ONE || n.state == NavState::Failed,
        };
        if stale {
            self.world.nav[i] = Some(Nav::to(goal, Fx::from_ratio(2, 10)));
        }
    }

    /// The transport in `bs` comes in to the water nearest the units in
    /// `boarding`, to take them aboard.
    pub(crate) fn come_alongside(&mut self, bs: Slot, boarding: &[Slot]) {
        let b = bs.index();
        let mut centroid = Vec2Fx::ZERO;
        for s in boarding {
            centroid += self.world.pos[s.index()];
        }
        let centroid = centroid.scale_ratio(Fx::ONE, Fx::from_int(boarding.len() as i32));
        let Some(from) = self.standing_tile(b) else {
            return;
        };
        let ct = nav::tile_of(centroid);
        if let Some(w) = self
            .water
            .nearest_passable(ct.0, ct.1, COME_IN_LOOK, Some(from))
        {
            let goal = nav::centre(w);
            self.world.order[b] = Order::Move { target: goal };
            self.world.nav[b] = Some(Nav::to(goal, Fx::HALF));
        }
    }

    /// One tick of a transport's unloading: to the water nearest `at`,
    /// then everyone ashore on the land beside it.
    pub(crate) fn tick_unload(&mut self, slot: Slot, at: Vec2Fx) {
        let i = slot.index();
        let id = self.world.id_at(slot);
        let idle = |sim: &mut Simulation| {
            sim.world.order[i] = Order::Idle;
            sim.world.nav[i] = None;
        };
        if self.garrison_count(id) == 0 {
            return idle(self);
        }
        let Some(from) = self.standing_tile(i) else {
            return idle(self);
        };
        let t = nav::tile_of(at);
        let Some(water) = self
            .water
            .nearest_passable(t.0, t.1, PUT_IN_LOOK, Some(from))
        else {
            return idle(self);
        };
        // Land the side of that water nearest the place asked.
        let Some(land) = self
            .nav
            .nearest_passable(water.0, water.1, ASHORE_REACH, None)
        else {
            return idle(self);
        };
        let reach = Fx::from_int(ASHORE_REACH);
        if self.world.pos[i].distance(nav::centre(land)) <= reach {
            self.put_ashore(slot, land);
            return idle(self);
        }
        match &self.world.nav[i] {
            None => self.world.nav[i] = Some(Nav::to(nav::centre(water), Fx::from_ratio(3, 10))),
            Some(n) if matches!(n.state, NavState::Arrived | NavState::Failed) => {
                // As near as the water goes, and the land out of reach.
                idle(self);
            }
            Some(_) => {}
        }
    }

    /// Everyone aboard the boat in `bs` steps ashore round `land`, on the
    /// nearest open tiles of the same land.
    pub(crate) fn put_ashore(&mut self, bs: Slot, land: Tile) {
        let id = self.world.id_at(bs);
        let aboard: Vec<usize> = self
            .garrison_of(id)
            .into_iter()
            .filter_map(|p| self.world.slot(p).map(|s| s.index()))
            .collect();
        if aboard.is_empty() {
            return;
        }
        let tiles = self.nav.spread(land.0, land.1, aboard.len(), Some(land));
        for (n, &j) in aboard.iter().enumerate() {
            self.world.inside[j] = None;
            self.world.pos[j] = nav::centre(tiles.get(n).copied().unwrap_or(land));
            self.world.order[j] = Order::Idle;
            self.world.nav[j] = None;
            self.world.move_target[j] = None;
        }
    }

    /// Everyone aboard a boat at sea rides where it goes.
    pub(crate) fn carry_passengers(&mut self) {
        for slot in self.world.slots().collect::<Vec<_>>() {
            let i = slot.index();
            let Some(carrier) = self.world.inside[i] else {
                continue;
            };
            if let Some(cs) = self.world.slot(carrier) {
                let c = cs.index();
                let k = kinds::info(self.world.kind[c]);
                if k.mobile && k.naval {
                    self.world.pos[i] = self.world.pos[c];
                }
            }
        }
    }

    /// The boat in `bs` has sunk: everyone aboard goes down with it.
    pub(crate) fn drown_passengers(&mut self, bs: Slot) {
        let id = self.world.id_at(bs);
        for p in self.garrison_of(id) {
            if let Some(ps) = self.world.slot(p) {
                let j = ps.index();
                self.events.push(Event::Death {
                    kind: self.world.kind[j],
                    owner: self.world.owner[j],
                    pos: self.world.pos[j],
                });
            }
            self.remove(p);
        }
    }

    /// Whether `dock` is a Dock `p` may trade at: another side's,
    /// finished and standing.
    pub fn market_for(&self, dock: EntityId, p: u8) -> Option<Slot> {
        let ds = self.world.slot(dock)?;
        let d = ds.index();
        (self.world.kind[d] == kinds::DOCK
            && self.world.owner[d] != p
            && self.world.owner[d] != kinds::GAIA
            && self.world.construction[d].is_none()
            && self.world.dying[d] == 0)
            .then_some(ds)
    }

    /// The finished, standing Dock of `p`'s nearest `pos`.
    fn home_dock(&self, p: u8, pos: Vec2Fx) -> Option<Slot> {
        self.world
            .slots()
            .filter(|s| {
                let d = s.index();
                self.world.kind[d] == kinds::DOCK
                    && self.world.owner[d] == p
                    && self.world.construction[d].is_none()
                    && self.world.dying[d] == 0
            })
            .min_by_key(|s| (pos.distance_sq_raw(self.world.pos[s.index()]), s.index()))
    }

    /// Sails unit `i` to beside the Dock in `ds`; true once there.
    fn sail_to(&mut self, i: usize, ds: Slot) -> bool {
        if self.within_reach(i, ds) {
            self.world.nav[i] = None;
            return true;
        }
        let idle = match &self.world.nav[i] {
            None => true,
            Some(n) => matches!(n.state, NavState::Arrived | NavState::Failed),
        };
        if idle {
            match self.approach(i, ds) {
                Some(goal) => {
                    let field = self.field_key_of(ds);
                    self.world.nav[i] = Some(Nav::along(goal, Fx::from_ratio(2, 10), field));
                }
                None => {
                    self.world.order[i] = Order::Idle;
                    self.world.nav[i] = None;
                }
            }
        }
        false
    }

    /// One tick of a trade boat's round (`GD-NAVAL-04`): load wood at a
    /// Dock of its side's, sell it at `market` for gold, bring the gold
    /// home, and go again. With no wood to load it waits at home; with no
    /// market or no home it stops.
    pub(crate) fn tick_trade(&mut self, slot: Slot, market: EntityId, out: bool) {
        let i = slot.index();
        let me = self.world.owner[i];
        let stop = |sim: &mut Simulation| {
            sim.world.order[i] = Order::Idle;
            sim.world.nav[i] = None;
        };
        let Some(home) = self.home_dock(me, self.world.pos[i]) else {
            return stop(self);
        };
        if out {
            let Some(ms) = self.market_for(market, me) else {
                return stop(self);
            };
            if self.world.carry[i].is_none() {
                if !self.sail_to(i, home) {
                    return;
                }
                if !self.players[me as usize].pay(&[0, TRADE_LOAD, 0, 0]) {
                    // Nothing to sell: wait at home for wood.
                    return;
                }
                self.world.carry[i] = Some((Resource::Wood, TRADE_LOAD));
            }
            if !self.sail_to(i, ms) {
                return;
            }
            // Sold: the further from its side's nearest Dock, the dearer.
            let from = self
                .home_dock(me, self.world.pos[ms.index()])
                .map_or(self.world.pos[i], |h| self.world.pos[h.index()]);
            let tiles = from.distance(self.world.pos[ms.index()]).floor();
            self.world.carry[i] = Some((Resource::Gold, trade_gold(tiles)));
            self.world.order[i] = Order::Trade { market, out: false };
        } else {
            if !self.sail_to(i, home) {
                return;
            }
            if let Some((r, amount)) = self.world.carry[i].take() {
                if r == Resource::Gold {
                    self.players[me as usize].deposit(r, amount);
                    self.events.push(Event::Deposited {
                        owner: me,
                        resource: r,
                        amount,
                        pos: self.world.pos[i],
                    });
                }
            }
            self.world.order[i] = Order::Trade { market, out: true };
        }
    }
}
