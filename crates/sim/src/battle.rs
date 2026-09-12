//! Fighting: the attack, attack-move, patrol and flee orders, target
//! acquisition by stance, hits and projectiles, death and corpses, and the
//! villagers' alarm (`docs/02` §8, §8.1; `docs/03` `UX-CMD-02`, `-03`,
//! `-07`).
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
use crate::simulation::{Simulation, REACH, TICKS_PER_SECOND};
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};

/// Ticks a corpse stays on the ground: thirty seconds.
pub const DECAY_TICKS: u16 = 600;
/// A projectile's flight speed in tiles per second.
pub const PROJECTILE_SPEED: Fx = Fx::from_int(8);
/// Ticks between one player's alarms, so a raid raises one cry, not fifty.
pub const ALARM_TICKS: u64 = 200;
/// A unit looks for enemies this often; slots are staggered so the load
/// spreads over the ticks.
const ACQUIRE_EVERY: u64 = 4;
/// How far a fleeing villager runs when there is no Town Center to run to.
const FLEE_TILES: i32 = 8;

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
    /// Something died.
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
    /// A live, fightable target: alive, not a corpse, and not something a
    /// hit means nothing to.
    pub(crate) fn target_slot(&self, id: EntityId) -> Option<Slot> {
        let s = self.world.slot(id)?;
        let i = s.index();
        let k = kinds::info(self.world.kind[i]);
        (self.world.dying[i] == 0 && k.class != kinds::Class::Other).then_some(s)
    }

    /// True if `i` is a live combatant: not a corpse and able to hit.
    fn can_fight(&self, i: usize) -> bool {
        self.world.dying[i] == 0 && kinds::info(self.world.kind[i]).combat.attack > 0
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

    /// The nearest enemy mobile unit within `radius` of `i`, ties by slot.
    fn nearest_enemy_unit(&self, i: usize, radius: Fx) -> Option<EntityId> {
        let me = self.world.owner[i];
        let pos = self.world.pos[i];
        let limit = radius.raw() as u64 * radius.raw() as u64;
        let mut best: Option<(u64, usize)> = None;
        for s in self.world.slots() {
            let j = s.index();
            let owner = self.world.owner[j];
            if owner == me || owner == GAIA || self.world.dying[j] != 0 {
                continue;
            }
            if !kinds::info(self.world.kind[j]).mobile {
                continue;
            }
            let d = pos.distance_sq_raw(self.world.pos[j]);
            if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, j));
            }
        }
        best.map(|(_, j)| self.world.id_at(Slot::new(j)))
    }

    /// Turns `i` to face `target`.
    fn face(&mut self, i: usize, target: Slot) {
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
    /// in whenever an enemy comes into sight.
    pub(crate) fn tick_attack_move(&mut self, slot: Slot, target: Vec2Fx) {
        let i = slot.index();
        match &self.world.nav[i] {
            None => self.world.nav[i] = Some(Nav::to(target, Fx::from_ratio(15, 100))),
            Some(n) if matches!(n.state, NavState::Arrived | NavState::Failed) => {
                self.world.nav[i] = None;
                self.world.order[i] = Order::Idle;
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

    /// One tick of a flight: run; stop when there.
    pub(crate) fn tick_flee(&mut self, slot: Slot, target: Vec2Fx) {
        let i = slot.index();
        match &self.world.nav[i] {
            None => self.world.nav[i] = Some(Nav::to(target, Fx::HALF)),
            Some(n) if matches!(n.state, NavState::Arrived | NavState::Failed) => {
                self.world.nav[i] = None;
                self.world.order[i] = Order::Idle;
            }
            Some(_) => {}
        }
    }

    /// Units that answer enemies on their own pick a target: what their
    /// stance allows, from where they stand, keeping what they were doing
    /// for afterwards (`GD-STANCE-01`).
    pub(crate) fn acquire(&mut self) {
        let tick = self.tick;
        let slots: Vec<Slot> = self
            .world
            .slots()
            .filter(|s| {
                let i = s.index();
                self.world.owner[i] != GAIA
                    && kinds::info(self.world.kind[i]).mobile
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
            let Some(target) = self.nearest_enemy_unit(i, sight) else {
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
    /// a melee hit lands now, a ranged one loosens a projectile.
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
            if k.combat.range > 0 {
                let from = self.world.pos[i];
                self.projectiles.push(Projectile {
                    pos: from,
                    target,
                    aim: self.world.pos[ts.index()],
                    damage,
                    owner: self.world.owner[i],
                    kind: k.combat.damage,
                });
            } else {
                let from = self.world.pos[i];
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
                if self.world.dying[ts.index()] == 0 {
                    p.aim = self.world.pos[ts.index()];
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
            let safety = self.safety_for(t, from);
            self.world.order[t] = Order::Flee { target: safety };
            self.world.nav[t] = None;
        }
    }

    /// Where a frightened unit runs: the nearest finished Town Center of
    /// its owner, else straight away from the attacker.
    fn safety_for(&self, i: usize, from: Vec2Fx) -> Vec2Fx {
        let me = self.world.owner[i];
        let pos = self.world.pos[i];
        let mut best: Option<(u64, Vec2Fx)> = None;
        for s in self.world.slots() {
            let j = s.index();
            if self.world.owner[j] != me
                || self.world.kind[j] != kinds::TOWN_CENTER
                || self.world.construction[j].is_some()
            {
                continue;
            }
            let d = pos.distance_sq_raw(self.world.pos[j]);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, self.world.pos[j]));
            }
        }
        if let Some((_, tc)) = best {
            return tc;
        }
        let away = (pos - from).normalized_or_zero();
        let away = if away.length().is_zero() {
            Vec2Fx::new(Fx::ONE, Fx::ZERO)
        } else {
            away
        };
        self.clamp_to_map(pos + away * Fx::from_int(FLEE_TILES))
    }

    /// The dead fall and the fallen go: a unit at zero health becomes a
    /// corpse for [`DECAY_TICKS`], out of every system but the renderer;
    /// a building at zero is removed outright (rubble is M4's next chunk).
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
                self.remove(self.world.id_at(slot));
            }
        }
    }
}
