//! The military manager and scouting (`docs/02` §12).
//!
//! With what the economy leaves over, it trains soldiers to a composition,
//! sends the scout round the map in widening rings, answers an alarm with
//! whatever soldiers are at home, and once enough are gathered sends them
//! at the nearest enemy building it knows of, as an attack-move in line.
//! Hard puts a Watch Tower by the Town Center. Everything it knows comes
//! from the view: an enemy it has not seen is an enemy it cannot attack,
//! which is what the scout is for.

use fogged::kinds::{self, Cost, Resource};
use fogged::{
    Age, CommandKind, EntityId, Event, FoggedView, Item, Job, KindId, Rng, Sighting, Stance, Vec2Fx,
};

use crate::economy::{afford, builder, place, spend, BuildOrder};

/// How long an alarm is answered for, in ticks.
const THREAT_TICKS: u64 = 600;
/// How far from the Town Center an alarm still counts as home.
const HOME_RADIUS: i32 = 30;
/// Rings the scout rides, in tiles from the Town Center.
const SCOUT_RINGS: [i32; 5] = [14, 22, 30, 40, 50];
/// Food and wood kept for the economy before a soldier is paid for.
const RESERVE: Cost = [150, 50, 0, 0];

/// The manager's memory between thoughts.
#[derive(Clone, Debug, Default)]
pub struct Military {
    /// Where the side was hit lately, and when.
    threats: Vec<(Vec2Fx, u64)>,
    /// The scout's next leg: ring, then point on the ring.
    scout_leg: usize,
    /// The scout has been told to run from trouble.
    scout_set: bool,
    /// The army is out: where it was sent and when.
    attack: Option<(Vec2Fx, u64)>,
    /// The tick of the last alarm answered.
    answered: Option<u64>,
}

/// A soldier: mobile, armed, and not a villager or the scout.
fn is_soldier(s: &Sighting) -> bool {
    let info = kinds::info(s.kind);
    info.mobile && info.combat.attack > 0 && s.kind != kinds::VILLAGER && s.kind != kinds::SCOUT
}

impl Military {
    /// Every tick, before thinking: what the bell says.
    pub fn observe(&mut self, view: &FoggedView<'_>) {
        let tick = view.tick();
        for e in view.events() {
            if let Event::Alarm { pos } = e {
                self.threats.push((pos, tick));
            }
        }
        self.threats
            .retain(|(_, t)| tick.saturating_sub(*t) < THREAT_TICKS);
    }

    /// One thought, with `stock` being what the economy has not spent.
    pub fn think(
        &mut self,
        view: &FoggedView<'_>,
        order: &BuildOrder,
        rng: &mut Rng,
        stock: &mut Cost,
    ) -> Vec<CommandKind> {
        let mut out = Vec::new();
        let Some(me) = view.me() else {
            return out;
        };
        let seen = view.sightings();
        let mine: Vec<&Sighting> = seen.iter().filter(|s| s.owner == view.player()).collect();
        let Some(tc) = mine
            .iter()
            .find(|s| s.kind == kinds::TOWN_CENTER && !s.site)
            .copied()
        else {
            return out;
        };
        let tick = view.tick();
        let age = me.age.index().min(3);
        let soldiers: Vec<&Sighting> = mine.iter().filter(|s| is_soldier(s)).copied().collect();
        let (w, h) = view.size();

        // ----- The scout rides the rings, from the point nearest its last
        // leg, to ground not yet seen.
        if order.scouts {
            if let Some(scout) = mine.iter().find(|s| s.kind == kinds::SCOUT && !s.inside) {
                if !self.scout_set {
                    out.push(CommandKind::SetStance {
                        ids: vec![scout.id],
                        stance: Stance::Passive,
                    });
                    self.scout_set = true;
                }
                if scout.job == Job::Idle {
                    let tc_tile = (tc.pos.x.floor(), tc.pos.y.floor());
                    let legs = SCOUT_RINGS.len() * 8;
                    let mut sent = false;
                    for _ in 0..legs {
                        let leg = self.scout_leg % legs;
                        self.scout_leg += 1;
                        let (ring, point) = (SCOUT_RINGS[leg / 8], leg % 8);
                        let (dx, dy) = COMPASS[point];
                        let x = (tc_tile.0 + dx * ring).clamp(1, w - 2);
                        let y = (tc_tile.1 + dy * ring).clamp(1, h - 2);
                        if !view.explored(x, y) {
                            out.push(CommandKind::Move {
                                ids: vec![scout.id],
                                target: fogged::nav::centre((x, y)),
                            });
                            sent = true;
                            break;
                        }
                    }
                    if !sent {
                        // Everything on the rings is seen: ride them again
                        // for what has changed since.
                        self.scout_leg = 0;
                    }
                }
            }
        }

        // ----- The alarm: soldiers at home go to the last place hit.
        let home = |p: Vec2Fx| {
            (p.x.floor() - tc.pos.x.floor()).abs() <= HOME_RADIUS
                && (p.y.floor() - tc.pos.y.floor()).abs() <= HOME_RADIUS
        };
        if let Some(&(pos, when)) = self.threats.iter().rfind(|(p, _)| home(*p)) {
            if self.answered != Some(when) {
                self.answered = Some(when);
                let defenders: Vec<EntityId> = soldiers
                    .iter()
                    .filter(|s| home(s.pos))
                    .map(|s| s.id)
                    .collect();
                if !defenders.is_empty() {
                    out.push(CommandKind::AttackMove {
                        ids: defenders,
                        target: pos,
                    });
                }
            }
        }

        // ----- The attack: enough soldiers idle at home, and somewhere
        // known to send them.
        let idle_home: Vec<EntityId> = soldiers
            .iter()
            .filter(|s| s.job == Job::Idle && home(s.pos))
            .map(|s| s.id)
            .collect();
        let out_already = self
            .attack
            .is_some_and(|(_, when)| tick.saturating_sub(when) < 1200);
        // Every enemy building known of, in sight or remembered, with
        // whether it shoots back: a raid goes for the houses, farms and
        // stores; only a full army walks into the Town Center's arrows.
        let enemy = |owner: u8| owner != view.player() && owner != kinds::GAIA;
        let shoots =
            |kind: KindId| kinds::info(kind).combat.attack > 0 || kind == kinds::TOWN_CENTER;
        let mut targets: Vec<(Vec2Fx, bool)> = seen
            .iter()
            .filter(|s| enemy(s.owner) && kinds::info(s.kind).footprint > 0)
            .map(|s| (s.pos, shoots(s.kind)))
            .collect();
        targets.extend(
            view.remembered()
                .into_iter()
                .filter(|r| enemy(r.owner))
                .map(|r| (fogged::nav::centre(r.tile), shoots(r.kind))),
        );
        let nearest_target = |to: Vec2Fx, full: bool| {
            targets
                .iter()
                .filter(|(_, shoots)| full || !*shoots)
                .map(|(p, _)| *p)
                .min_by_key(|p| (p.distance_sq_raw(to), p.x.raw(), p.y.raw()))
        };
        // A raid goes out with the attack size once the order's hour has
        // come; the Town Center's arrows wait for twice that.
        let n_home = idle_home.len() as u32;
        let full = n_home >= order.attack_size * 2;
        let raid = tick >= order.attack_by && n_home >= order.attack_size;
        if (full || raid) && !out_already && self.threats.is_empty() {
            if let Some(target) = nearest_target(tc.pos, full) {
                out.push(CommandKind::AttackMove {
                    ids: idle_home,
                    target,
                });
                self.attack = Some((target, tick));
            }
        }
        // Soldiers idle away from home carry on to the next enemy building
        // they know of, the Town Center included once they are enough, or
        // come home when there is nothing left to go for.
        let idle_away: Vec<&Sighting> = soldiers
            .iter()
            .filter(|s| s.job == Job::Idle && !home(s.pos))
            .copied()
            .collect();
        if let Some(lead) = idle_away.first() {
            let ids: Vec<EntityId> = idle_away.iter().map(|s| s.id).collect();
            let full = ids.len() as u32 >= order.attack_size;
            match nearest_target(lead.pos, full) {
                Some(target) => out.push(CommandKind::AttackMove { ids, target }),
                None => out.push(CommandKind::Move {
                    ids,
                    target: tc.pos,
                }),
            }
        }

        // ----- Training, to the composition, with what is left after the
        // reserve.
        let want = order.army[age];
        let queued_soldiers: u32 = mine
            .iter()
            .filter(|s| kinds::info(s.kind).trains && !s.site)
            .map(|b| {
                view.queue(b.id)
                    .iter()
                    .filter(|i| matches!(i, Item::Unit(k) if *k != kinds::VILLAGER))
                    .count() as u32
            })
            .sum();
        if soldiers.len() as u32 + queued_soldiers < want {
            // The kind furthest below its share of the army, among those a
            // finished building of ours can train.
            let mut have = std::collections::BTreeMap::new();
            for s in &soldiers {
                *have.entry(s.kind).or_insert(0u32) += 1;
            }
            let composition = COMPOSITION[age];
            let mut best: Option<(i32, KindId, EntityId)> = None;
            for b in mine
                .iter()
                .filter(|s| kinds::info(s.kind).trains && !s.site)
            {
                if view.queue(b.id).len() >= 2 {
                    continue;
                }
                for kind in view.roster(b.kind) {
                    let Some(&(_, share)) = composition.iter().find(|(k, _)| *k == kind) else {
                        continue;
                    };
                    let target = (want * share).div_ceil(100) as i32;
                    let deficit = target - *have.get(&kind).unwrap_or(&0) as i32;
                    let cost = kinds::info(kind).cost;
                    let mut with_reserve = cost;
                    for (c, r) in with_reserve.iter_mut().zip(RESERVE) {
                        *c += r;
                    }
                    if deficit <= 0
                        || !afford(stock, &with_reserve)
                        || view.can_train(b.id, kind).is_err()
                    {
                        continue;
                    }
                    if best.is_none_or(|(d, _, _)| deficit > d) {
                        best = Some((deficit, kind, b.id));
                    }
                }
            }
            if let Some((_, kind, building)) = best {
                out.push(CommandKind::Train { building, kind });
                spend(stock, &kinds::info(kind).cost);
            }
        }

        // ----- A Watch Tower by the Town Center, for the orders that want
        // one.
        if order.towers > 0
            && age >= Age::Tool.index()
            && (mine.iter().filter(|s| s.kind == kinds::WATCH_TOWER).count() as u32) < order.towers
            && view.can_build(kinds::WATCH_TOWER).is_ok()
        {
            let cost = kinds::info(kinds::WATCH_TOWER).cost;
            if afford(stock, &cost) {
                let villagers: Vec<&Sighting> = mine
                    .iter()
                    .filter(|s| s.kind == kinds::VILLAGER && !s.inside)
                    .copied()
                    .collect();
                let tc_tile = (tc.pos.x.floor(), tc.pos.y.floor());
                if let Some((x, y)) = place(view, kinds::WATCH_TOWER, tc_tile, 4, 8, rng) {
                    if let Some(b) = builder(
                        &villagers,
                        fogged::nav::centre((x, y)),
                        Some(Resource::Stone),
                        &[],
                    ) {
                        out.push(CommandKind::Build {
                            kind: kinds::WATCH_TOWER,
                            x,
                            y,
                            ids: vec![b],
                        });
                        spend(stock, &cost);
                    }
                }
            }
        }
        out
    }
}

/// The eight compass points, in the order the scout rides them.
const COMPASS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// The army's shape by age index, in percent of the army target: what to
/// train, and how much of it. A kind a building of ours cannot train yet
/// is skipped, so the Stone Age is all clubmen and the Tool Age is axemen
/// with bowmen and slingers once their buildings stand.
const COMPOSITION: [&[(KindId, u32)]; 4] = [
    &[(kinds::CLUBMAN, 100)],
    &[
        (kinds::AXEMAN, 40),
        (kinds::BOWMAN, 30),
        (kinds::SLINGER, 15),
        (kinds::CLUBMAN, 15),
    ],
    &[
        (kinds::AXEMAN, 35),
        (kinds::BOWMAN, 30),
        (kinds::LIGHT_CAVALRY, 20),
        (kinds::SLINGER, 15),
    ],
    &[
        (kinds::AXEMAN, 35),
        (kinds::BOWMAN, 30),
        (kinds::LIGHT_CAVALRY, 20),
        (kinds::SLINGER, 15),
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositions_sum_to_the_whole_army_and_the_compass_goes_round() {
        for c in COMPOSITION {
            assert_eq!(c.iter().map(|(_, s)| s).sum::<u32>(), 100);
        }
        assert_eq!(COMPASS.len(), 8);
        assert!(COMPASS.iter().all(|&(x, y)| x != 0 || y != 0));
        assert!(SCOUT_RINGS.windows(2).all(|w| w[0] < w[1]));
    }
}
