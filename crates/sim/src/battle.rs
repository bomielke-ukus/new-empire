//! Fighting: the attack, attack-move, patrol, flee and garrison orders,
//! target acquisition by stance, hits and projectiles, death, corpses and
//! rubble, gates, and the villagers' alarm (`docs/02` §6, §8, §8.1;
//! `docs/03` `UX-CMD-02`, `-03`, `-07`, `-09`).
//!
//! The damage numbers come from [`crate::combat`]; this module decides who
//! swings at whom, when, and what happens to what is hit.

use crate::combat;
use crate::command::PlayerId;
use crate::entity::{EntityId, KindId, Slot};
use crate::fx::Fx;
use crate::hash::{HashState, StateHasher};
use crate::kinds::{self, DamageType, GAIA};
use crate::nav;
use crate::orders::{Nav, NavState, Order, Stance, Then};
use crate::simulation::{Simulation, REACH, REACH_SLACK, TICKS_PER_SECOND};
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};

/// Ticks a corpse stays on the ground: thirty seconds.
pub const DECAY_TICKS: u16 = 600;
/// Ticks rubble stays on the ground: sixty seconds (`docs/03` §6.2). The
/// footprint is open from the first of them.
pub const RUBBLE_TICKS: u16 = 1200;
/// A projectile's flight speed in tiles per second.
pub const PROJECTILE_SPEED: Fx = Fx::from_int(8);
/// Ticks between one player's alarms, so a raid raises one cry, not fifty.
pub const ALARM_TICKS: u64 = 200;
/// A unit looks for enemies this often; slots are staggered so the load
/// spreads over the ticks.
const ACQUIRE_EVERY: u64 = 4;
/// How far a fleeing villager runs when there is no Town Center to run to.
const FLEE_TILES: i32 = 8;
/// A gate shuts when an enemy comes this near (`docs/02` §6: "allies pass,
/// enemies do not"), and opens again once none is within [`GATE_OPEN`].
/// Nothing walks the gap between the two in a tick, so no enemy is ever
/// standing on a gate when it shuts.
const GATE_CLOSE: Fx = Fx::from_int(2);
const GATE_OPEN: Fx = Fx::from_int(3);
/// Where the arrows of a building's volley start, in quarter tiles around
/// its centre, so a full tower visibly fires more than one.
const VOLLEY_OFFSETS: [(i32, i32); 8] = [
    (0, 0),
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (-1, -1),
    (1, -1),
];

/// Something in flight from a ranged unit to its target.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Projectile {
    /// Where it is.
    pub pos: Vec2Fx,
    /// What it was shot at; it homes while the target lives.
    pub target: EntityId,
    /// Where it is heading: the target's last known position.
    pub aim: Vec2Fx,
    /// Damage on impact, settled when it was loosed.
    pub damage: i32,
    /// Who shot it.
    pub owner: PlayerId,
    /// What kind of hit it lands.
    pub kind: DamageType,
}

impl HashState for Projectile {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&self.pos);
        h.write(&self.target);
        h.write(&self.aim);
        h.write_i32(self.damage);
        h.write_u8(self.owner);
        h.write_u8(self.kind as u8);
    }
}

/// Something the presentation layer may want to react to. Cleared every
/// tick; not state, so not hashed or saved.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// A player's unit was hit and their side had not been told lately.
    Alarm {
        /// Whose unit.
        player: PlayerId,
        /// Where.
        pos: Vec2Fx,
    },
    /// A hit landed.
    Hit {
        /// What was hit.
        target: EntityId,
        /// Where.
        pos: Vec2Fx,
        /// How much.
        damage: i32,
    },
    /// Something died. For a building, it fell: rubble is where it stood.
    Death {
        /// What.
        kind: KindId,
        /// Whose.
        owner: PlayerId,
        /// Where.
        pos: Vec2Fx,
    },
}

impl Simulation {
    /// A live, fightable target: alive, not a corpse or rubble, not
    /// sheltering inside a building, and not something a hit means nothing
    /// to.
    pub(crate) fn target_slot(&self, id: EntityId) -> Option<Slot> {
        let s = self.world.slot(id)?;
        let i = s.index();
        let k = kinds::info(self.world.kind[i]);
        (self.world.dying[i] == 0
            && self.world.inside[i].is_none()
            && k.class != kinds::Class::Other)
            .then_some(s)
    }

    /// True if `i` is a live combatant: not a corpse, not inside a
    /// building, finished if it is one, and with something to shoot.
    fn can_fight(&self, i: usize) -> bool {
        self.world.dying[i] == 0
            && self.world.inside[i].is_none()
            && self.world.construction[i].is_none()
            && kinds::info(self.world.kind[i]).combat.attack > 0
            && self.volley(i) > 0
    }

    /// Projectiles `i` looses per swing: its own, plus one per unit
    /// garrisoned inside it if it is a building (`UX-CMD-09`).
    pub(crate) fn volley(&self, i: usize) -> u32 {
        let k = kinds::info(self.world.kind[i]);
        let own = k.combat.arrows as u32;
        if k.mobile || k.garrison == 0 {
            own
        } else {
            own + self.garrison_count(self.world.id_at(Slot::new(i)))
        }
    }

    /// How many units shelter inside `building`.
    pub(crate) fn garrison_count(&self, building: EntityId) -> u32 {
        self.world
            .slots()
            .filter(|s| self.world.inside[s.index()] == Some(building))
            .count() as u32
    }

    /// The units sheltering inside `building`, in slot order.
    pub fn garrison_of(&self, building: EntityId) -> Vec<EntityId> {
        self.world
            .slots()
            .filter(|s| self.world.inside[s.index()] == Some(building))
            .map(|s| self.world.id_at(s))
            .collect()
    }

    /// True if a finished gate stands open: its tile is passable. A gate
    /// shuts while an enemy is near and opens again when none is.
    pub fn gate_open(&self, i: usize) -> bool {
        let t = nav::tile_of(self.world.pos[i]);
        self.world.kind[i] == kinds::GATE && self.nav.passable(t.0, t.1)
    }

    /// The reach of unit `i`'s weapon as a distance from its position to a
    /// footprint tile of the target: hand-to-hand is [`REACH`], a ranged
    /// weapon its range plus the same allowance.
    fn reach_of(&self, i: usize) -> Fx {
        let k = kinds::info(self.world.kind[i]);
        Fx::from_int(combat::range_of(k, &self.modifiers(self.world.owner[i]))) + REACH
    }

    /// True if `i` can hit `target` from where it stands.
    fn in_range(&self, i: usize, target: Slot) -> bool {
        let reach = self.reach_of(i);
        self.within(i, target, reach)
    }

    /// How far unit `i` sees, in tiles.
    fn sight_of(&self, i: usize) -> Fx {
        Fx::from_int(kinds::info(self.world.kind[i]).combat.line_of_sight)
    }

    /// The nearest enemy within `radius` of `i`, ties by slot: mobile units,
    /// or with `buildings` the enemy's buildings and sites instead.
    fn nearest_enemy(&self, i: usize, radius: Fx, buildings: bool) -> Option<EntityId> {
        let me = self.world.owner[i];
        let pos = self.world.pos[i];
        let limit = radius.raw() as u64 * radius.raw() as u64;
        let mut best: Option<(u64, usize)> = None;
        for s in self.world.slots() {
            let j = s.index();
            let owner = self.world.owner[j];
            if owner == me
                || owner == GAIA
                || self.world.dying[j] != 0
                || self.world.inside[j].is_some()
            {
                continue;
            }
            let k = kinds::info(self.world.kind[j]);
            if k.mobile == buildings || k.class == kinds::Class::Other {
                continue;
            }
            let d = pos.distance_sq_raw(self.world.pos[j]);
            if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, j));
            }
        }
        best.map(|(_, j)| self.world.id_at(Slot::new(j)))
    }

    /// Turns `i` to face `target`. Buildings have one face.
    fn face(&mut self, i: usize, target: Slot) {
        if !kinds::info(self.world.kind[i]).mobile {
            return;
        }
        let here = self.world.pos[i];
        let there = self.world.pos[target.index()];
        if there != here {
            self.world.facing[i] = (there - here).angle().facing8();
        }
    }

    /// Puts `i` on an attack, keeping what it should do afterwards.
    pub(crate) fn engage(
        &mut self,
        i: usize,
        target: EntityId,
        then: Then,
        leash: Option<(Vec2Fx, Fx)>,
    ) {
        self.world.order[i] = Order::Attack {
            target,
            then,
            leash,
        };
        self.world.nav[i] = None;
    }

    /// The fight is over, or off: do what was planned for afterwards.
    pub(crate) fn finish_fight(&mut self, i: usize, then: Then) {
        self.world.nav[i] = None;
        self.world.order[i] = match then {
            Then::Idle => Order::Idle,
            Then::Return(origin) => {
                if self.world.pos[i].distance(origin) <= Fx::HALF {
                    Order::Idle
                } else {
                    self.world.nav[i] = Some(Nav::to(origin, Fx::HALF));
                    Order::Move { target: origin }
                }
            }
            Then::AttackMove(target) => Order::AttackMove { target },
            Then::Patrol(from, to, leg) => Order::Patrol { from, to, leg },
        };
    }

    /// One tick of an attack order: close with the target and hold in
    /// range. The hit itself lands in [`Simulation::strike`].
    pub(crate) fn tick_attack(
        &mut self,
        slot: Slot,
        target: EntityId,
        then: Then,
        leash: Option<(Vec2Fx, Fx)>,
    ) {
        let i = slot.index();
        let Some(ts) = self.target_slot(target) else {
            self.finish_fight(i, then);
            return;
        };
        if !self.can_fight(i) {
            self.finish_fight(i, then);
            return;
        }
        let tpos = self.world.pos[ts.index()];
        if let Some((origin, radius)) = leash {
            // Past the leash: let it go and go back.
            if tpos.distance(origin) > radius + self.reach_of(i) {
                self.finish_fight(i, then);
                return;
            }
        }
        if self.in_range(i, ts) {
            // Hold here; face the target.
            self.world.nav[i] = None;
            self.face(i, ts);
            return;
        }
        if matches!(leash, Some((_, r)) if r.is_zero()) {
            // Standing ground: out of range is out of the fight.
            self.finish_fight(i, then);
            return;
        }
        if self.nav_failed(i) {
            self.finish_fight(i, then);
            return;
        }
        // Walk toward it, re-aiming when it has moved a tile or the trip
        // has ended short.
        let arrive = (self.reach_of(i) - Fx::HALF).max(Fx::HALF);
        let stale = match &self.world.nav[i] {
            None => true,
            Some(n) => {
                n.goal.distance(tpos) > Fx::ONE
                    || matches!(n.state, NavState::Arrived | NavState::Failed)
            }
        };
        if stale {
            let key = if kinds::info(self.world.kind[ts.index()]).footprint > 0 {
                self.field_key_of(ts)
            } else {
                let t = nav::tile_of(tpos);
                (t.0, t.1, 0)
            };
            self.world.nav[i] = Some(Nav::along(tpos, arrive, key));
        }
    }

    /// One tick of an attack-move: walk to the point; acquisition breaks
    /// in whenever an enemy unit comes into sight. Where the walk ends, at
    /// the point or as near as the way allows, the nearest enemy building
    /// in sight is taken next: a column walled out breaks in, and a column
    /// that arrives razes what stands there.
    pub(crate) fn tick_attack_move(&mut self, slot: Slot, target: Vec2Fx) {
        let i = slot.index();
        match &self.world.nav[i] {
            None => self.world.nav[i] = Some(Nav::to(target, Fx::from_ratio(15, 100))),
            Some(n) if matches!(n.state, NavState::Arrived | NavState::Failed) => {
                self.world.nav[i] = None;
                let sight = self.sight_of(i);
                match self.nearest_enemy(i, sight, true) {
                    Some(b) => self.engage(i, b, Then::AttackMove(target), None),
                    None => self.world.order[i] = Order::Idle,
                }
            }
            Some(_) => {}
        }
    }

    /// One tick of a patrol: walk the current leg; turn round at its end.
    pub(crate) fn tick_patrol(&mut self, slot: Slot, from: Vec2Fx, to: Vec2Fx, leg: u8) {
        let i = slot.index();
        let goal = if leg == 0 { to } else { from };
        match &self.world.nav[i] {
            None => self.world.nav[i] = Some(Nav::to(goal, Fx::HALF)),
            Some(n) if matches!(n.state, NavState::Arrived | NavState::Failed) => {
                self.world.nav[i] = None;
                self.world.order[i] = Order::Patrol {
                    from,
                    to,
                    leg: 1 - leg,
                };
            }
            Some(_) => {}
        }
    }

    /// One tick of a flight: run; shelter in the Town Center on arrival if
    /// it has room, else stop there.
    pub(crate) fn tick_flee(&mut self, slot: Slot, target: Vec2Fx, into: Option<EntityId>) {
        let i = slot.index();
        let shelter = into.and_then(|b| self.shelter_slot(b, self.world.owner[i]));
        if let Some(bs) = shelter {
            if self.within(i, bs, REACH_SLACK) {
                self.enter(i, bs);
                return;
            }
        }
        match &self.world.nav[i] {
            None => {
                // To a tile beside the door, or to the point.
                let door = shelter.and_then(|bs| self.approach(i, bs).map(|g| (g, bs)));
                self.world.nav[i] = Some(match door {
                    Some((goal, bs)) => {
                        Nav::along(goal, Fx::from_ratio(2, 10), self.field_key_of(bs))
                    }
                    None => Nav::to(target, Fx::HALF),
                });
            }
            Some(n) if matches!(n.state, NavState::Arrived | NavState::Failed) => {
                self.world.nav[i] = None;
                self.world.order[i] = Order::Idle;
            }
            Some(_) => {}
        }
    }

    /// One tick of a garrison order: walk to the building and go inside
    /// (`UX-CMD-09`). Gone, full, unfinished or someone else's: stop.
    pub(crate) fn tick_garrison(&mut self, slot: Slot, building: EntityId) {
        let i = slot.index();
        let Some(bs) = self.shelter_slot(building, self.world.owner[i]) else {
            self.world.nav[i] = None;
            self.world.order[i] = Order::Idle;
            return;
        };
        // Beside the door is near enough: the approach tile's centre plus
        // the stopping tolerance can be a little past [`REACH`].
        if self.within(i, bs, REACH_SLACK) {
            self.enter(i, bs);
            return;
        }
        match &self.world.nav[i] {
            None => match self.approach(i, bs) {
                // To a tile beside the door, as a builder walks to a site.
                Some(goal) => {
                    self.world.nav[i] = Some(Nav::along(
                        goal,
                        Fx::from_ratio(2, 10),
                        self.field_key_of(bs),
                    ));
                }
                None => self.world.order[i] = Order::Idle,
            },
            Some(n) if matches!(n.state, NavState::Arrived | NavState::Failed) => {
                // As close as it gets, and not close enough.
                self.world.nav[i] = None;
                self.world.order[i] = Order::Idle;
            }
            Some(_) => {}
        }
    }

    /// `building` as a place `owner`'s units may shelter right now: theirs,
    /// finished, standing, built to hold units, and with room.
    pub(crate) fn shelter_slot(&self, building: EntityId, owner: PlayerId) -> Option<Slot> {
        let bs = self.world.slot(building)?;
        let b = bs.index();
        let k = kinds::info(self.world.kind[b]);
        (self.world.owner[b] == owner
            && k.garrison > 0
            && self.world.dying[b] == 0
            && self.world.construction[b].is_none()
            && self.garrison_count(building) < k.garrison as u32)
            .then_some(bs)
    }

    /// Unit `i` steps inside `bs`.
    fn enter(&mut self, i: usize, bs: Slot) {
        self.world.inside[i] = Some(self.world.id_at(bs));
        self.world.pos[i] = self.world.pos[bs.index()];
        self.world.nav[i] = None;
        self.world.move_target[i] = None;
        self.world.order[i] = Order::Idle;
        self.world.reload[i] = 0;
    }

    /// Everything inside `bs` steps out onto the nearest open tiles around
    /// its footprint, nearest first.
    pub(crate) fn eject(&mut self, bs: Slot) {
        let id = self.world.id_at(bs);
        let units: Vec<usize> = self
            .world
            .slots()
            .map(|s| s.index())
            .filter(|&j| self.world.inside[j] == Some(id))
            .collect();
        if units.is_empty() {
            return;
        }
        let b = bs.index();
        let fp = kinds::info(self.world.kind[b]).footprint as i32;
        let (ax, ay) = nav::anchor_tile(self.world.pos[b], fp);
        let tiles = self.nav.spread(ax, ay, units.len(), None);
        let centre = self.world.pos[b];
        for (n, &j) in units.iter().enumerate() {
            self.world.inside[j] = None;
            // Walled in completely: stand on the spot and let the next
            // tick's nudge find a tile.
            self.world.pos[j] = tiles.get(n).map_or(centre, |t| nav::centre(*t));
            self.world.order[j] = Order::Idle;
            self.world.nav[j] = None;
            self.world.move_target[j] = None;
        }
    }

    /// Gates shut while an enemy is near and open again once none is. The
    /// gate's tile is a blocker in the one grid everyone walks, so the
    /// flow fields route the owner through an open gate and route an enemy
    /// round a shut one, or up to it to break it down.
    pub(crate) fn gates(&mut self) {
        let gates: Vec<Slot> = self
            .world
            .slots()
            .filter(|s| {
                let i = s.index();
                self.world.kind[i] == kinds::GATE
                    && self.world.dying[i] == 0
                    && self.world.construction[i].is_none()
            })
            .collect();
        if gates.is_empty() {
            return;
        }
        let mut changed = false;
        for g in gates {
            let i = g.index();
            let me = self.world.owner[i];
            let pos = self.world.pos[i];
            let t = nav::tile_of(pos);
            let open = self.nav.passable(t.0, t.1);
            let radius = if open { GATE_CLOSE } else { GATE_OPEN };
            let limit = radius.raw() as u64 * radius.raw() as u64;
            let threatened = self.world.slots().any(|s| {
                let j = s.index();
                let owner = self.world.owner[j];
                owner != me
                    && owner != GAIA
                    && kinds::info(self.world.kind[j]).mobile
                    && self.world.dying[j] == 0
                    && self.world.inside[j].is_none()
                    && pos.distance_sq_raw(self.world.pos[j]) <= limit
            });
            if open && threatened {
                self.nav.block(t.0, t.1);
                changed = true;
            } else if !open && !threatened {
                self.nav.unblock(t.0, t.1);
                changed = true;
            }
        }
        if changed {
            self.nav.refresh();
        }
    }

    /// Units and towers that answer enemies on their own pick a target:
    /// what their stance allows, from where they stand, keeping what they
    /// were doing for afterwards (`GD-STANCE-01`). Only units are taken
    /// this way; buildings are taken where an attack-move ends.
    pub(crate) fn acquire(&mut self) {
        let tick = self.tick;
        let slots: Vec<Slot> = self
            .world
            .slots()
            .filter(|s| {
                let i = s.index();
                self.world.owner[i] != GAIA
                    && self.can_fight(i)
                    && self.world.stance[i] != Stance::Passive
                    && (i as u64 + tick).is_multiple_of(ACQUIRE_EVERY)
            })
            .collect();
        for slot in slots {
            let i = slot.index();
            let then = match self.world.order[i] {
                Order::Idle => Then::Return(self.world.pos[i]),
                Order::AttackMove { target } => Then::AttackMove(target),
                Order::Patrol { from, to, leg } => Then::Patrol(from, to, leg),
                // Everything else is busy: working, fighting, running.
                _ => continue,
            };
            let sight = self.sight_of(i);
            let Some(target) = self.nearest_enemy(i, sight, false) else {
                continue;
            };
            let here = self.world.pos[i];
            let leash = match (self.world.stance[i], then) {
                // On the march, anything seen is fair game, within reason.
                (_, Then::AttackMove(_) | Then::Patrol(..)) => Some((here, sight * 2)),
                (Stance::Aggressive, _) => Some((here, sight * 2)),
                (Stance::Defensive, _) => Some((here, sight)),
                (Stance::StandGround, _) => Some((here, Fx::ZERO)),
                (Stance::Passive, _) => continue,
            };
            // Standing ground only takes what is already in reach.
            if matches!(leash, Some((_, r)) if r.is_zero()) {
                let Some(ts) = self.target_slot(target) else {
                    continue;
                };
                if !self.in_range(i, ts) {
                    continue;
                }
            }
            self.engage(i, target, then, leash);
        }
    }

    /// Every unit on an attack and in range swings when its reload allows:
    /// a melee hit lands now, a ranged one loosens its volley.
    pub(crate) fn strike(&mut self) {
        let slots: Vec<Slot> = self.world.slots().collect();
        for slot in slots {
            let i = slot.index();
            if self.world.reload[i] > 0 {
                self.world.reload[i] -= 1;
            }
            let Order::Attack { target, .. } = self.world.order[i] else {
                continue;
            };
            if self.world.reload[i] > 0 || !self.can_fight(i) {
                continue;
            }
            let Some(ts) = self.target_slot(target) else {
                continue;
            };
            if !self.in_range(i, ts) {
                continue;
            }
            let attacker = self.world.id_at(slot);
            let Some(damage) = self.damage_between(attacker, target) else {
                continue;
            };
            let k = kinds::info(self.world.kind[i]);
            self.world.reload[i] = k.combat.reload_ticks as u16;
            self.face(i, ts);
            let from = self.world.pos[i];
            if k.combat.range > 0 {
                let aim = self.world.pos[ts.index()];
                let owner = self.world.owner[i];
                for n in 0..self.volley(i) as usize {
                    let (dx, dy) = VOLLEY_OFFSETS[n % VOLLEY_OFFSETS.len()];
                    let start = from + Vec2Fx::new(Fx::from_ratio(dx, 4), Fx::from_ratio(dy, 4));
                    self.projectiles.push(Projectile {
                        pos: self.clamp_to_map(start),
                        target,
                        aim,
                        damage,
                        owner,
                        kind: k.combat.damage,
                    });
                }
            } else {
                self.hit(ts, damage, from, self.world.owner[i]);
            }
        }
    }

    /// Projectiles fly and land.
    pub(crate) fn fly(&mut self) {
        let step = PROJECTILE_SPEED / TICKS_PER_SECOND as i32;
        let mut landed = Vec::new();
        for (n, p) in self.projectiles.iter_mut().enumerate() {
            if let Some(ts) = self.world.slot(p.target) {
                let t = ts.index();
                if self.world.dying[t] == 0 && self.world.inside[t].is_none() {
                    p.aim = self.world.pos[t];
                }
            }
            if p.pos.distance(p.aim) <= step {
                p.pos = p.aim;
                landed.push(n);
            } else {
                p.pos = p.pos.move_toward(p.aim, step);
            }
        }
        for n in landed.into_iter().rev() {
            let p = self.projectiles.remove(n);
            if let Some(ts) = self.target_slot(p.target) {
                self.hit(ts, p.damage, p.pos, p.owner);
            }
        }
    }

    /// A hit lands on `target`: health comes off, the victim's side is
    /// told, and a passive unit runs for it (`GD-STANCE-02`).
    fn hit(&mut self, target: Slot, damage: i32, from: Vec2Fx, by: PlayerId) {
        let t = target.index();
        let before = self.world.health[t];
        self.world.health[t] = (before - Fx::from_int(damage)).max(Fx::ZERO);
        self.scratch.stats.hits += 1;
        let victim = self.world.id_at(target);
        let pos = self.world.pos[t];
        self.events.push(Event::Hit {
            target: victim,
            pos,
            damage,
        });
        let owner = self.world.owner[t];
        if owner == GAIA || owner == by {
            return;
        }
        // Raise the alarm, once in a while.
        if let Some(last) = self.scratch.last_alarm.get_mut(owner as usize) {
            if *last == 0 || self.tick >= *last + ALARM_TICKS {
                *last = self.tick.max(1);
                self.events.push(Event::Alarm { player: owner, pos });
            }
        }
        // A passive unit that is not already running runs.
        let mobile = kinds::info(self.world.kind[t]).mobile;
        if mobile
            && self.world.stance[t] == Stance::Passive
            && self.world.health[t] > Fx::ZERO
            && !matches!(self.world.order[t], Order::Flee { .. })
        {
            let (safety, into) = self.safety_for(t, from);
            self.world.order[t] = Order::Flee {
                target: safety,
                into,
            };
            self.world.nav[t] = None;
        }
    }

    /// Where a frightened unit runs: the nearest standing, finished Town
    /// Center of its owner (and which one, to shelter inside), else
    /// straight away from the attacker.
    fn safety_for(&self, i: usize, from: Vec2Fx) -> (Vec2Fx, Option<EntityId>) {
        let me = self.world.owner[i];
        let pos = self.world.pos[i];
        let mut best: Option<(u64, Slot)> = None;
        for s in self.world.slots() {
            let j = s.index();
            if self.world.owner[j] != me
                || self.world.kind[j] != kinds::TOWN_CENTER
                || self.world.construction[j].is_some()
                || self.world.dying[j] > 0
            {
                continue;
            }
            let d = pos.distance_sq_raw(self.world.pos[j]);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, s));
            }
        }
        if let Some((_, tc)) = best {
            return (self.world.pos[tc.index()], Some(self.world.id_at(tc)));
        }
        let away = (pos - from).normalized_or_zero();
        let away = if away.length().is_zero() {
            Vec2Fx::new(Fx::ONE, Fx::ZERO)
        } else {
            away
        };
        (
            self.clamp_to_map(pos + away * Fx::from_int(FLEE_TILES)),
            None,
        )
    }

    /// The dead fall and the fallen go: a unit at zero health becomes a
    /// corpse for [`DECAY_TICKS`], a building rubble for [`RUBBLE_TICKS`],
    /// both out of every system but the renderer.
    pub(crate) fn deaths(&mut self) {
        let slots: Vec<Slot> = self.world.slots().collect();
        for slot in slots {
            let i = slot.index();
            if self.world.dying[i] > 0 {
                self.world.dying[i] -= 1;
                if self.world.dying[i] == 0 {
                    self.remove(self.world.id_at(slot));
                }
                continue;
            }
            if self.world.health[i] > Fx::ZERO {
                continue;
            }
            let k = kinds::info(self.world.kind[i]);
            if k.class == kinds::Class::Other {
                // Trees and veins die by being used up, not by hits.
                continue;
            }
            self.scratch.stats.kills += 1;
            self.events.push(Event::Death {
                kind: self.world.kind[i],
                owner: self.world.owner[i],
                pos: self.world.pos[i],
            });
            if k.mobile {
                self.world.dying[i] = DECAY_TICKS;
                self.world.order[i] = Order::Idle;
                self.world.nav[i] = None;
                self.world.move_target[i] = None;
                self.world.carry[i] = None;
                self.world.reload[i] = 0;
            } else {
                self.demolish(slot);
            }
        }
    }

    /// A building falls: its garrison steps out, its footprint opens (a
    /// breach, if it was a wall), its queue is lost, its builders stop, and
    /// rubble lies where it stood for [`RUBBLE_TICKS`]. A site destroyed
    /// refunds nothing.
    fn demolish(&mut self, slot: Slot) {
        let i = slot.index();
        let id = self.world.id_at(slot);
        let info = kinds::info(self.world.kind[i]);
        // Out before the footprint opens, so they land round it, not on it.
        self.eject(slot);
        let (ax, ay) = nav::anchor_tile(self.world.pos[i], info.footprint as i32);
        // An open gate's tile is already clear; the counts saturate.
        self.nav.unblock_footprint(ax, ay, info.footprint as i32);
        self.nav.refresh();
        self.world.production[i] = None;
        self.world.construction[i] = None;
        self.world.resource[i] = 0;
        self.world.health[i] = Fx::ZERO;
        self.world.order[i] = Order::Idle;
        self.world.reload[i] = 0;
        self.world.dying[i] = RUBBLE_TICKS;
        for s in self.world.slots().collect::<Vec<_>>() {
            let j = s.index();
            if matches!(self.world.order[j], Order::Build { site, .. } if site == id) {
                self.world.order[j] = Order::Idle;
                self.world.nav[j] = None;
            }
        }
    }
}
