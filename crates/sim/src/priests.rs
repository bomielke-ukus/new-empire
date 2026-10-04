//! Priests: conversion and healing (`docs/02` §5.5).
//!
//! A priest's faith is its reload counter. It is full at 0; a conversion
//! sets it to [`FAITH_TICKS`], and it runs down a tick at a time in
//! [`Simulation::strike`] like any weapon's. A priest is no fighter: it
//! has no attack, so nothing in the fighting code ever swings with it.

use crate::battle::Event;
use crate::command::PlayerId;
use crate::entity::{EntityId, Slot};
use crate::fx::Fx;
use crate::kinds::{self, Class, GAIA};
use crate::nav;
use crate::orders::{Nav, NavState, Order, Stance};
use crate::simulation::{Simulation, REACH};

/// How far a priest's chant reaches, in tiles (`GD-PRIEST-01`).
pub const CONVERT_RANGE: i32 = 7;

/// Ticks for spent faith to come back: 40 seconds (`GD-PRIEST-02`).
pub const FAITH_TICKS: u16 = 800;

/// The shortest chant, in ticks: four seconds.
pub const CHANT_MIN: u16 = 80;

/// The longest chant, in ticks: ten seconds. Where between the two falls
/// is the simulation's draw (`GD-PRIEST-01`: "a variable interval").
pub const CHANT_MAX: u16 = 200;

/// How far a priest reaches to heal, in tiles.
pub const HEAL_RANGE: i32 = 4;

/// Hit points a priest gives back each second (`GD-PRIEST-04`). They are
/// given whole, once a second, so a wounded unit's health climbs in
/// steps a player can read.
pub const HEAL_HP: i32 = 3;

/// Ticks between a priest's heals: one second.
const HEAL_EVERY: u64 = crate::simulation::TICKS_PER_SECOND as u64;

impl Simulation {
    /// `target` as something `owner`'s priest may convert: a live unit out
    /// in the open, someone else's and not nature's. Buildings cannot be
    /// converted; siege can (`GD-PRIEST-03`).
    pub fn convertible(&self, target: EntityId, owner: PlayerId) -> Option<Slot> {
        let ts = self.target_slot(target)?;
        let t = ts.index();
        let k = kinds::info(self.world.kind[t]);
        let theirs = self.world.owner[t];
        (k.mobile && k.class != Class::Animal && theirs != owner && theirs != GAIA).then_some(ts)
    }

    /// Puts priest `i` on converting `target`.
    pub(crate) fn begin_conversion(&mut self, i: usize, target: EntityId) {
        self.world.order[i] = Order::Convert { target, chant: 0 };
        self.world.nav[i] = None;
    }

    /// One tick of a conversion: close to within [`CONVERT_RANGE`], wait
    /// there for the faith to be full, chant, and at the chant's end take
    /// the unit (`GD-PRIEST-01`). A target that steps out of reach holds
    /// the chant where it is while the priest follows.
    pub(crate) fn tick_convert(&mut self, slot: Slot, target: EntityId, chant: u16) {
        let i = slot.index();
        let me = self.world.owner[i];
        let Some(ts) = self.convertible(target, me) else {
            self.world.nav[i] = None;
            self.world.order[i] = Order::Idle;
            return;
        };
        let extra = self.civ(me).map_or(0, |c| c.convert_range());
        let reach = Fx::from_int(CONVERT_RANGE + extra) + REACH;
        if self.within(i, ts, reach) {
            self.world.nav[i] = None;
            self.face(i, ts);
            match chant {
                // Spent: wait here for the faith to come back.
                0 if self.world.reload[i] > 0 => {}
                0 => {
                    let ticks = self
                        .rng
                        .range_i32(i32::from(CHANT_MIN), i32::from(CHANT_MAX) + 1)
                        as u16;
                    self.world.order[i] = Order::Convert {
                        target,
                        chant: ticks,
                    };
                    self.events.push(Event::Chant {
                        owner: me,
                        pos: self.world.pos[i],
                        at: self.world.pos[ts.index()],
                    });
                }
                1 => self.convert(i, ts),
                n => {
                    self.world.order[i] = Order::Convert {
                        target,
                        chant: n - 1,
                    };
                }
            }
            return;
        }
        if self.nav_failed(i) {
            self.world.nav[i] = None;
            self.world.order[i] = Order::Idle;
            return;
        }
        // Walk toward it, re-aiming when it has moved a tile or the trip
        // has ended short, as an attacker does.
        let tpos = self.world.pos[ts.index()];
        let stale = match &self.world.nav[i] {
            None => true,
            Some(n) => {
                n.goal.distance(tpos) > Fx::ONE
                    || matches!(n.state, NavState::Arrived | NavState::Failed)
            }
        };
        if stale {
            let t = nav::tile_of(tpos);
            let arrive = (reach - Fx::HALF).max(Fx::HALF);
            self.world.nav[i] = Some(Nav::along(tpos, arrive, (t.0, t.1, 0)));
        }
    }

    /// The chant ends: unit `ts` is priest `i`'s side's now, standing idle
    /// where it stands, and the priest's faith is spent.
    fn convert(&mut self, i: usize, ts: Slot) {
        let t = ts.index();
        let to = self.world.owner[i];
        let from = self.world.owner[t];
        let kind = self.world.kind[t];
        self.world.owner[t] = to;
        // No more health than its new side's kind has (`docs/02` §11).
        let full = Fx::from_int(self.max_health_of(to, kind));
        if self.world.health[t] > full {
            self.world.health[t] = full;
        }
        self.world.order[t] = Order::Idle;
        self.world.nav[t] = None;
        self.world.move_target[t] = None;
        self.world.stance[t] = Stance::default_for(kind);
        self.world.queue_mut(t).clear();
        // A priest turned brings its relic.
        let id = self.world.id_at(ts);
        if let Some(relic) = self.carried_relic(id) {
            if let Some(rs) = self.world.slot(relic) {
                self.world.owner[rs.index()] = to;
            }
        }
        self.world.reload[i] = FAITH_TICKS;
        self.world.order[i] = Order::Idle;
        self.world.nav[i] = None;
        self.events.push(Event::Converted {
            target: self.world.id_at(ts),
            kind,
            from,
            to,
            pos: self.world.pos[t],
        });
    }

    /// Every priest not converting gives [`HEAL_HP`] to the most wounded
    /// unit of its side within [`HEAL_RANGE`], once a second
    /// (`GD-PRIEST-04`): not itself, and not a siege engine, which is
    /// timber and rope. The priests take their turns spread over the
    /// second, by slot.
    pub(crate) fn heal(&mut self) {
        let tick = self.tick;
        let priests: Vec<usize> = self
            .world
            .slots()
            .map(|s| s.index())
            .filter(|&i| {
                self.world.kind[i] == kinds::PRIEST
                    && (i as u64 + tick).is_multiple_of(HEAL_EVERY)
                    && self.world.owner[i] != GAIA
                    && self.world.dying[i] == 0
                    && self.world.inside[i].is_none()
                    && !matches!(self.world.order[i], Order::Convert { .. })
            })
            .collect();
        let range = Fx::from_int(HEAL_RANGE);
        let limit = range.raw() as u64 * range.raw() as u64;
        for i in priests {
            let me = self.world.owner[i];
            let here = self.world.pos[i];
            // The most hit points missing, ties to the lowest slot.
            let mut best: Option<(Fx, usize)> = None;
            for s in self.world.slots() {
                let j = s.index();
                if j == i || self.world.owner[j] != me {
                    continue;
                }
                let k = kinds::info(self.world.kind[j]);
                if !k.mobile
                    || k.class == Class::Siege
                    || self.world.dying[j] > 0
                    || self.world.inside[j].is_some()
                    || here.distance_sq_raw(self.world.pos[j]) > limit
                {
                    continue;
                }
                let missing =
                    Fx::from_int(self.max_health_of(me, self.world.kind[j])) - self.world.health[j];
                if missing > Fx::ZERO && best.is_none_or(|(m, _)| missing > m) {
                    best = Some((missing, j));
                }
            }
            if let Some((missing, j)) = best {
                let gain = Fx::from_int(HEAL_HP).min(missing);
                self.world.health[j] += gain;
                self.events.push(Event::Healed {
                    target: self.world.id_at(Slot::new(j)),
                    pos: self.world.pos[j],
                });
            }
        }
    }
}
