//! The opponent at sea (`docs/02` §5.6, `docs/07` D33): a Dock where
//! there is water near home, fishing boats on the fish it knows of,
//! warships to meet the enemy's and to find it across the water, and
//! transports to carry the army over when the enemy cannot be walked to.
//!
//! Like everything the opponent does it works from its view: water it
//! has not seen is water it does not know is there, and an enemy it has
//! not found across the sea is one it looks for with a ship.

use fogged::kinds::{self, Cost};
use fogged::{
    CommandKind, EntityId, FoggedView, Job, KindId, Sighting, Terrain, TrainError, Vec2Fx,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::economy::{afford, builder, spend, BuildOrder};

/// How far from the Town Center, in tiles, a Dock site is looked for.
const DOCK_LOOK: i32 = 24;
/// Ticks between looks for a Dock site when none was found.
const DOCK_SEARCH_EVERY: u64 = 600;
/// Ticks an ordered Dock has to become a site before it is ordered again.
const DOCK_ORDER_TICKS: u64 = 400;
/// How far from home, in tiles, fish are worth a boat.
const FISH_LOOK: i32 = 30;
/// Kept back before a boat is paid for.
const RESERVE: Cost = [100, 50, 0, 0];
/// How far from the Town Center a soldier still counts as at home.
const HOME_RADIUS: i32 = 20;
/// Ticks the army has to get aboard before the boats sail with whoever
/// is aboard.
const BOARD_PATIENCE: u64 = 1200;
/// Ticks a ferry has to land its passengers before it is given up.
const SAIL_PATIENCE: u64 = 3000;
/// How far from the enemy Town Center, in tiles, a landing is looked for.
const LANDING_LOOK: i32 = 32;
/// Ticks after which an enemy not yet found is looked for by sea.
const SEA_SCOUT_AFTER: u64 = 6000;
/// Ticks between workings-out of whether the enemy can be walked to.
const BY_LAND_EVERY: u64 = 600;

/// Units a transport holds.
const HOLD: usize = 10;

/// The army on its way across, in transports.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Ferry {
    /// The transports.
    boats: Vec<EntityId>,
    /// The soldiers told to board them.
    #[serde(default)]
    party: Vec<EntityId>,
    /// Where to land them: a tile of the enemy's shore.
    landing: (i32, i32),
    /// Under way (else boarding).
    sailing: bool,
    /// The tick the phase began.
    since: u64,
}

/// The manager's memory between thoughts.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Navy {
    /// The tick no Dock site was found near home: not looked for again
    /// until a while after.
    #[serde(default)]
    dock_searched: Option<u64>,
    /// The tick a Dock was ordered, until it is a site.
    #[serde(default)]
    dock_ordered: Option<u64>,
    /// A ferry under way.
    #[serde(default)]
    ferry: Option<Ferry>,
    /// The sea scout's next leg.
    #[serde(default)]
    scout_leg: usize,
    /// Whether the enemy can be walked to, and when that was worked out.
    #[serde(default)]
    by_land: Option<(bool, u64)>,
}

/// A soldier of the land army.
fn is_soldier(s: &Sighting) -> bool {
    let k = kinds::info(s.kind);
    k.mobile
        && k.combat.attack > 0
        && !k.naval
        && s.kind != kinds::VILLAGER
        && s.kind != kinds::SCOUT
        && s.kind != kinds::PRIEST
}

/// A warship: a boat that fights.
fn is_warship(kind: KindId) -> bool {
    let k = kinds::info(kind);
    k.naval && k.mobile && k.combat.attack > 0
}

fn tile(p: Vec2Fx) -> (i32, i32) {
    (p.x.floor(), p.y.floor())
}

/// Where the enemy is, as far as the opponent knows: its Town Center in
/// sight or remembered, else the enemy building nearest `home`.
fn enemy_tc(view: &FoggedView<'_>, seen: &[Sighting], home: Vec2Fx) -> Option<Vec2Fx> {
    let enemy = |owner: u8| owner != view.player() && owner != kinds::GAIA;
    let remembered = view.remembered();
    seen.iter()
        .find(|s| enemy(s.owner) && s.kind == kinds::TOWN_CENTER && !s.site)
        .map(|s| s.pos)
        .or_else(|| {
            remembered
                .iter()
                .find(|r| enemy(r.owner) && r.kind == kinds::TOWN_CENTER && !r.site)
                .map(|r| fogged::nav::centre(r.tile))
        })
        .or_else(|| {
            seen.iter()
                .filter(|s| enemy(s.owner) && kinds::info(s.kind).footprint > 0)
                .map(|s| s.pos)
                .chain(
                    remembered
                        .iter()
                        .filter(|r| enemy(r.owner))
                        .map(|r| fogged::nav::centre(r.tile)),
                )
                .min_by_key(|p| (p.distance_sq_raw(home), p.x.raw(), p.y.raw()))
        })
}

/// Whether land the player has seen joins `from` to within two tiles of
/// `to`. Ground never seen counts as no way: an enemy across water the
/// opponent has not looked at is one it cannot be sure of walking to.
fn walkable_between(view: &FoggedView<'_>, from: (i32, i32), to: (i32, i32)) -> bool {
    let (w, _) = view.size();
    let land = land_of(view, from, false);
    (-2..=2).any(|dy| {
        (-2..=2).any(|dx| {
            let (x, y) = (to.0 + dx, to.1 + dy);
            x >= 0 && y >= 0 && x < w && land.get((y * w + x) as usize) == Some(&true)
        })
    })
}

/// Every tile of land the player has seen joined to within two tiles of
/// `from`, by index: by the lie of the land, or with `as_seen` only over
/// ground known open (trees and buildings in the way).
fn land_of(view: &FoggedView<'_>, from: (i32, i32), as_seen: bool) -> Vec<bool> {
    let (w, h) = view.size();
    let open = |x: i32, y: i32| {
        x >= 0
            && y >= 0
            && x < w
            && y < h
            && if as_seen {
                view.passable(x, y) == Some(true)
            } else {
                view.terrain(x, y).is_some_and(|t| t.walkable())
            }
    };
    let mut seen = vec![false; (w * h) as usize];
    let mut queue = VecDeque::new();
    for dy in -2..=2 {
        for dx in -2..=2 {
            let (x, y) = (from.0 + dx, from.1 + dy);
            if open(x, y) && !seen[(y * w + x) as usize] {
                seen[(y * w + x) as usize] = true;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if open(nx, ny) && !seen[(ny * w + nx) as usize] {
                seen[(ny * w + nx) as usize] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    seen
}

/// A tile of the shore nearest `to`: land the player has seen with water
/// it has seen beside it, where a transport can put soldiers ashore.
fn landing_near(view: &FoggedView<'_>, to: (i32, i32)) -> Option<(i32, i32)> {
    let wet = |x: i32, y: i32| view.terrain(x, y).is_some_and(Terrain::is_water);
    let dry = |x: i32, y: i32| view.terrain(x, y).is_some_and(|t| t.walkable());
    for r in 1..=LANDING_LOOK {
        let mut best: Option<(i32, (i32, i32))> = None;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) != r {
                    continue;
                }
                let (x, y) = (to.0 + dx, to.1 + dy);
                let shore = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|&(ex, ey)| wet(x + ex, y + ey));
                if dry(x, y) && shore {
                    let d = dx * dx + dy * dy;
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, (x, y)));
                    }
                }
            }
        }
        if let Some((_, t)) = best {
            return Some(t);
        }
    }
    None
}

impl Navy {
    /// Whether the side has a Dock, built or building: without one there
    /// is no going by sea, and the army walks whatever the map.
    pub fn has_dock(&self, view: &FoggedView<'_>) -> bool {
        view.sightings()
            .iter()
            .any(|s| s.owner == view.player() && s.kind == kinds::DOCK)
    }

    /// Whether the enemy's Town Center can be walked to from home, as far
    /// as the opponent has seen; true while it knows of none, so the army
    /// keeps to its land plans until there is something across the water
    /// to go for. Worked out once in a while, not every thought.
    pub fn by_land(&mut self, view: &FoggedView<'_>) -> bool {
        let tick = view.tick();
        if let Some((answer, when)) = self.by_land {
            if tick.saturating_sub(when) < BY_LAND_EVERY {
                return answer;
            }
        }
        let seen = view.sightings();
        let home = seen
            .iter()
            .find(|s| s.owner == view.player() && s.kind == kinds::TOWN_CENTER && !s.site)
            .map(|s| s.pos);
        let answer = match home.and_then(|h| enemy_tc(view, &seen, h).map(|e| (h, e))) {
            Some((from, to)) => walkable_between(view, tile(from), tile(to)),
            None => true,
        };
        self.by_land = Some((answer, tick));
        answer
    }

    /// One thought: the commands for the Dock, the boats and a ferry,
    /// with `stock` being what the economy and the army have not spent.
    pub fn think(
        &mut self,
        view: &FoggedView<'_>,
        order: &BuildOrder,
        stock: &mut Cost,
    ) -> Vec<CommandKind> {
        let mut out = Vec::new();
        if order.fishers == 0 {
            return out;
        }
        let tick = view.tick();
        let by_land = self.by_land(view);
        let seen = view.sightings();
        let mine: Vec<&Sighting> = seen.iter().filter(|s| s.owner == view.player()).collect();
        let Some(tc) = mine
            .iter()
            .find(|s| s.kind == kinds::TOWN_CENTER && !s.site)
            .copied()
        else {
            return out;
        };
        let home_tile = tile(tc.pos);
        let near = |p: Vec2Fx, r: i32| {
            let (x, y) = tile(p);
            (x - home_tile.0).abs() <= r && (y - home_tile.1).abs() <= r
        };
        let enemy = |owner: u8| owner != view.player() && owner != kinds::GAIA;
        let docks: Vec<&Sighting> = mine
            .iter()
            .filter(|s| s.kind == kinds::DOCK)
            .copied()
            .collect();
        let dock = docks.iter().find(|d| !d.site).copied();
        let fish: Vec<&Sighting> = seen
            .iter()
            .filter(|s| s.kind == kinds::FISH && s.resource.is_some_and(|(_, left)| left > 0))
            .filter(|s| near(s.pos, FISH_LOOK))
            .collect();
        let enemy_tc = enemy_tc(view, &seen, tc.pos);

        // ----- A Dock, once the Stone Age's buildings stand, where there
        // is water near home worth one: fish to catch, or an enemy over
        // the water.
        if self
            .dock_ordered
            .is_some_and(|t| tick.saturating_sub(t) > DOCK_ORDER_TICKS)
            || !docks.is_empty()
        {
            self.dock_ordered = None;
        }
        let basics = [kinds::STOREHOUSE, kinds::BARRACKS]
            .iter()
            .all(|k| mine.iter().any(|s| s.kind == *k && !s.site));
        let wanted = !fish.is_empty() || !by_land;
        let searched_lately = self
            .dock_searched
            .is_some_and(|t| tick.saturating_sub(t) < DOCK_SEARCH_EVERY);
        if docks.is_empty()
            && self.dock_ordered.is_none()
            && basics
            && wanted
            && !searched_lately
            && view.can_build(kinds::DOCK).is_ok()
        {
            let cost = view.cost_of(kinds::DOCK);
            if afford(stock, &cost) {
                match dock_site(view, home_tile) {
                    Some((x, y)) => {
                        let villagers: Vec<&Sighting> = mine
                            .iter()
                            .filter(|s| s.kind == kinds::VILLAGER && !s.inside)
                            .copied()
                            .collect();
                        let at = fogged::nav::building_centre(x, y, 3);
                        if let Some(b) = builder(&villagers, at, None, &[]) {
                            out.push(CommandKind::Build {
                                kind: kinds::DOCK,
                                x,
                                y,
                                ids: vec![b],
                            });
                            spend(stock, &cost);
                            self.dock_ordered = Some(tick);
                        }
                    }
                    None => self.dock_searched = Some(tick),
                }
            }
        }
        let Some(dock) = dock else {
            return out;
        };
        let queue = view.queue(dock.id);
        let train = |kind: KindId, stock: &mut Cost, out: &mut Vec<CommandKind>| -> bool {
            let mut with_reserve = view.cost_of(kind);
            for (c, r) in with_reserve.iter_mut().zip(RESERVE) {
                *c += r;
            }
            if queue.len() < 2
                && afford(stock, &with_reserve)
                && view.can_train(dock.id, kind).is_ok()
            {
                out.push(CommandKind::Train {
                    building: dock.id,
                    kind,
                });
                spend(stock, &view.cost_of(kind));
                true
            } else {
                false
            }
        };

        // ----- Fishing boats, up to the order's number while there are
        // fish; an idle one to the nearest fish it knows.
        let fishers: Vec<&Sighting> = mine
            .iter()
            .filter(|s| s.kind == kinds::FISHING_BOAT)
            .copied()
            .collect();
        if (fishers.len() as u32) < order.fishers && !fish.is_empty() {
            train(kinds::FISHING_BOAT, stock, &mut out);
        }
        for boat in fishers.iter().filter(|b| b.job == Job::Idle) {
            if let Some(f) = fish
                .iter()
                .min_by_key(|f| (f.pos.distance_sq_raw(boat.pos), f.id.index()))
            {
                out.push(CommandKind::Gather {
                    ids: vec![boat.id],
                    node: f.id,
                });
            }
        }

        // ----- Warships: to meet the enemy's at sea, or when the enemy is
        // over the water; one to look for an enemy not yet found.
        let enemy_naval: Vec<Vec2Fx> = seen
            .iter()
            .filter(|s| enemy(s.owner) && kinds::info(s.kind).naval)
            .map(|s| s.pos)
            .chain(
                view.remembered()
                    .into_iter()
                    .filter(|r| enemy(r.owner) && r.kind == kinds::DOCK)
                    .map(|r| fogged::nav::centre(r.tile)),
            )
            .collect();
        let warships: Vec<&Sighting> = mine
            .iter()
            .filter(|s| is_warship(s.kind))
            .copied()
            .collect();
        let lost = enemy_tc.is_none() && tick >= SEA_SCOUT_AFTER;
        let want = if !by_land || !enemy_naval.is_empty() {
            order.warships
        } else if lost {
            1
        } else {
            0
        };
        if (warships.len() as u32) < want {
            // The best warship the side has: waited for if it is only
            // short of the price.
            let best = [kinds::CATAPULT_SHIP, kinds::WAR_GALLEY, kinds::ARCHER_SHIP]
                .into_iter()
                .find(|&k| {
                    matches!(
                        view.can_train(dock.id, k),
                        Ok(()) | Err(TrainError::Unaffordable | TrainError::QueueFull)
                    )
                });
            if let Some(kind) = best {
                train(kind, stock, &mut out);
            }
        }
        let idle_ships: Vec<&Sighting> = warships
            .iter()
            .filter(|s| s.job == Job::Idle)
            .copied()
            .collect();
        if let Some(target) = enemy_naval
            .iter()
            .copied()
            .min_by_key(|p| (p.distance_sq_raw(tc.pos), p.x.raw(), p.y.raw()))
        {
            if !idle_ships.is_empty() {
                out.push(CommandKind::AttackMove {
                    ids: idle_ships.iter().map(|s| s.id).collect(),
                    target,
                });
            }
        } else if lost {
            // Sail to the edge of the water seen, nearest first, until the
            // enemy is found; with none left, round the map's rim.
            if let Some(scout) = idle_ships.first() {
                let to = sea_frontier(view, scout.pos, self.scout_leg).unwrap_or_else(|| {
                    let (w, h) = view.size();
                    let k = self.scout_leg as i32 % 8;
                    let rim = [
                        (w / 2, 2),
                        (w - 3, 2),
                        (w - 3, h / 2),
                        (w - 3, h - 3),
                        (w / 2, h - 3),
                        (2, h - 3),
                        (2, h / 2),
                        (2, 2),
                    ];
                    rim[k as usize]
                });
                self.scout_leg += 1;
                out.push(CommandKind::Move {
                    ids: vec![scout.id],
                    target: fogged::nav::centre(to),
                });
            }
        }

        // ----- The ferry: over the water to an enemy that cannot be walked
        // to, the army aboard transports once it has gathered at home, put
        // ashore on the enemy's shore nearest its Town Center; the army
        // manager takes it from there.
        if by_land || !order.ferries {
            self.ferry = None;
            return out;
        }
        let Some(target) = enemy_tc else {
            return out;
        };
        let soldiers: Vec<&Sighting> = mine
            .iter()
            .filter(|s| is_soldier(s) && !s.inside)
            .copied()
            .collect();
        let idle_home: Vec<&Sighting> = soldiers
            .iter()
            .filter(|s| s.job == Job::Idle && near(s.pos, HOME_RADIUS))
            .copied()
            .collect();
        let threshold = if tick >= order.attack_by {
            (order.attack_size * 3).div_ceil(4)
        } else {
            order.attack_size
        } as usize;
        let transports: Vec<&Sighting> = mine
            .iter()
            .filter(|s| s.kind == kinds::TRANSPORT)
            .copied()
            .collect();
        let want = idle_home.len().max(threshold).div_ceil(HOLD).clamp(1, 3);
        if transports.len() < want && self.ferry.is_none() {
            train(kinds::TRANSPORT, stock, &mut out);
        }
        match self.ferry.take() {
            None => {
                let free: Vec<&Sighting> = transports
                    .iter()
                    .filter(|t| t.job == Job::Idle)
                    .copied()
                    .collect();
                if idle_home.len() >= threshold && !free.is_empty() {
                    if let Some(landing) = landing_near(view, tile(target)) {
                        let mut boats = Vec::new();
                        let mut all = Vec::new();
                        for (boat, party) in free.iter().zip(idle_home.chunks(HOLD)) {
                            let ids: Vec<EntityId> = party.iter().map(|s| s.id).collect();
                            all.extend(&ids);
                            out.push(CommandKind::Garrison {
                                ids,
                                building: boat.id,
                            });
                            boats.push(boat.id);
                        }
                        self.ferry = Some(Ferry {
                            boats,
                            party: all,
                            landing,
                            sailing: false,
                            since: tick,
                        });
                    }
                }
            }
            Some(mut ferry) => {
                ferry
                    .boats
                    .retain(|b| transports.iter().any(|t| t.id == *b));
                if ferry.boats.is_empty() {
                    return out;
                }
                let aboard: usize = ferry.boats.iter().map(|b| view.aboard(*b)).sum();
                if !ferry.sailing {
                    // The party not yet aboard, and still coming.
                    let ashore: Vec<EntityId> = soldiers
                        .iter()
                        .filter(|s| ferry.party.contains(&s.id))
                        .map(|s| s.id)
                        .collect();
                    let late = tick.saturating_sub(ferry.since) >= BOARD_PATIENCE;
                    if aboard > 0 && (ashore.is_empty() || late) {
                        out.push(CommandKind::Unload {
                            ids: ferry.boats.clone(),
                            target: fogged::nav::centre(ferry.landing),
                        });
                        ferry.sailing = true;
                        ferry.since = tick;
                    }
                    if late || ferry.sailing {
                        // Whoever missed the boat stands down, to go with
                        // the next.
                        if !ashore.is_empty() {
                            out.push(CommandKind::Stop { ids: ashore });
                        }
                        if !ferry.sailing {
                            return out;
                        }
                    }
                    self.ferry = Some(ferry);
                } else if aboard == 0 || tick.saturating_sub(ferry.since) >= SAIL_PATIENCE {
                    // Landed, or given up: home to the Dock.
                    out.push(CommandKind::Move {
                        ids: ferry.boats.clone(),
                        target: dock.pos,
                    });
                } else {
                    let idle = ferry.boats.iter().any(|b| {
                        transports
                            .iter()
                            .any(|t| t.id == *b && t.job == Job::Idle && view.aboard(*b) > 0)
                    });
                    if idle {
                        // Stopped short with people aboard: once more.
                        out.push(CommandKind::Unload {
                            ids: ferry.boats.clone(),
                            target: fogged::nav::centre(ferry.landing),
                        });
                    }
                    self.ferry = Some(ferry);
                }
            }
        }
        out
    }
}

/// The nearest tile of water the player has seen with ground it has not
/// beside it: where a ship looking for the enemy sails next. Every
/// `skip`-th such tile in turn after the nearest is passed over, so a
/// ship that cannot get to one tries another.
fn sea_frontier(view: &FoggedView<'_>, from: Vec2Fx, skip: usize) -> Option<(i32, i32)> {
    let (w, h) = view.size();
    let wet = |x: i32, y: i32| view.terrain(x, y).is_some_and(Terrain::is_water);
    let mut edge: Vec<(u64, (i32, i32))> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if wet(x, y)
                && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    nx >= 0 && ny >= 0 && nx < w && ny < h && !view.explored(nx, ny)
                })
            {
                edge.push((from.distance_sq_raw(fogged::nav::centre((x, y))), (x, y)));
            }
        }
    }
    edge.sort_unstable();
    if edge.is_empty() {
        return None;
    }
    Some(edge[(skip % 4).min(edge.len() - 1)].1)
}

/// Whether the water at `from` is open sea as far as the player has seen:
/// it runs on for more than a pond's worth of tiles. A Dock on a pond, or
/// in a cove shut off by the land, trains boats that go nowhere.
fn open_sea(view: &FoggedView<'_>, from: (i32, i32)) -> bool {
    const POND: usize = 150;
    let wet = |x: i32, y: i32| view.terrain(x, y).is_some_and(Terrain::is_water);
    if !wet(from.0, from.1) {
        return false;
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut queue = VecDeque::new();
    seen.insert(from);
    queue.push_back(from);
    while let Some((x, y)) = queue.pop_front() {
        if seen.len() > POND {
            return true;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (x + dx, y + dy);
            if wet(n.0, n.1) && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    false
}

/// A Dock site near `home`: the nearest the opponent may place one, by
/// ring, with the land beside it joined to home's, so a villager can walk
/// there to build it (a sandbar off the shore will not do).
fn dock_site(view: &FoggedView<'_>, home: (i32, i32)) -> Option<(i32, i32)> {
    let (w, _) = view.size();
    let land = land_of(view, home, true);
    let ours =
        |x: i32, y: i32| x >= 0 && y >= 0 && x < w && land.get((y * w + x) as usize) == Some(&true);
    for r in 3..=DOCK_LOOK {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) != r {
                    continue;
                }
                let (x, y) = (home.0 + dx, home.1 + dy);
                if view.can_place(kinds::DOCK, x, y).is_ok() {
                    let ring = fogged::nav::footprint_ring(x, y, 3);
                    if ring.iter().any(|&(rx, ry)| ours(rx, ry))
                        && ring.iter().any(|&(rx, ry)| open_sea(view, (rx, ry)))
                    {
                        return Some((x, y));
                    }
                }
            }
        }
    }
    None
}
