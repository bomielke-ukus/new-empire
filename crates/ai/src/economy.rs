//! The build-order planner and the economy manager (`docs/02` §12).
//!
//! Every few ticks the manager looks at the view and issues what a player
//! at the keyboard would: villagers to the resource furthest below its
//! share, a house ahead of the population cap, a villager from the Town
//! Center while under the target, a Storehouse by the wood and a Barracks
//! for the age gate, the age advance when the simulation allows it, and
//! farms once the bushes are gone. It knows only what the view tells it,
//! and every order goes through the same commands a player has.

use fogged::kinds::{self, Cost, Resource};
use fogged::tech;
use fogged::{
    Age, CommandKind, EntityId, FoggedView, Item, Job, KindId, Rally, Rng, Sighting, Vec2Fx,
};

use crate::Difficulty;

/// What a difficulty aims for, by age index (Stone, Tool, Bronze, Iron).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BuildOrder {
    /// Villagers to keep, by age.
    pub villagers: [u32; 4],
    /// Gatherers per resource in percent, by age: food, wood, stone, gold.
    pub shares: [[u32; 4]; 4],
    /// Ticks between thoughts: how fast it reacts.
    pub cadence: u64,
    /// Population headroom at or below which a house is started.
    pub headroom: u32,
    /// The furthest age this order advances to.
    pub last_age: Age,
    /// Whether one gatherer a thought is moved from the resource most over
    /// its share to the one most under it.
    pub rebalances: bool,
    /// Soldiers to keep, by age.
    pub army: [u32; 4],
    /// Soldiers idle at home before a raid goes out; twice this before
    /// the army walks into the Town Center's arrows.
    pub attack_size: u32,
    /// Not before this many ticks does a raid go out.
    pub attack_by: u64,
    /// Whether the scout rides the map.
    pub scouts: bool,
    /// Watch Towers wanted by the Town Center.
    pub towers: u32,
}

/// The buildings that count toward the next age, in the order they are
/// wanted, by age index. The Storehouse also drops off wood and stone and
/// gold; the Barracks is the first soldier's home.
const AGE_BUILDINGS: [&[KindId]; 4] = [
    &[kinds::STOREHOUSE, kinds::BARRACKS],
    &[kinds::ARCHERY_RANGE, kinds::MARKET],
    &[],
    &[],
];

/// Wood kept back for the next house.
const HOUSE_RESERVE: i32 = 30;

impl BuildOrder {
    /// The order a difficulty plays (`docs/02` §12's table).
    pub fn for_difficulty(d: Difficulty) -> BuildOrder {
        match d {
            Difficulty::Easy => BuildOrder {
                villagers: [6, 10, 12, 12],
                shares: [
                    [60, 40, 0, 0],
                    [50, 35, 5, 10],
                    [45, 30, 10, 15],
                    [45, 30, 10, 15],
                ],
                cadence: 40,
                headroom: 1,
                last_age: Age::Tool,
                rebalances: false,
                army: [3, 5, 6, 6],
                attack_size: 4,
                attack_by: 18_000,
                scouts: false,
                towers: 0,
            },
            Difficulty::Standard => BuildOrder {
                villagers: [8, 16, 22, 26],
                shares: [
                    [60, 40, 0, 0],
                    [45, 35, 5, 15],
                    [40, 30, 10, 20],
                    [40, 30, 10, 20],
                ],
                cadence: 20,
                headroom: 3,
                last_age: Age::Bronze,
                rebalances: true,
                army: [4, 10, 16, 20],
                attack_size: 6,
                attack_by: 12_000,
                scouts: true,
                towers: 0,
            },
            Difficulty::Hard | Difficulty::Hardest => BuildOrder {
                villagers: [10, 20, 28, 32],
                shares: [
                    [60, 40, 0, 0],
                    [45, 35, 5, 15],
                    [40, 30, 10, 20],
                    [40, 30, 10, 20],
                ],
                cadence: 10,
                headroom: 4,
                last_age: Age::Iron,
                rebalances: true,
                army: [6, 14, 24, 30],
                attack_size: 8,
                attack_by: 9_000,
                scouts: true,
                towers: 1,
            },
        }
    }
}

/// The manager's own memory between thoughts.
#[derive(Clone, Debug, Default)]
pub struct Economy {
    /// The node the Town Center's rally points at.
    rally: Option<EntityId>,
    /// Auto-reseed has been switched on.
    reseeding: bool,
    /// Buildings ordered and not yet seen as sites, with the tick ordered,
    /// so one is not ordered twice while the first is walking to its site.
    ordered: Vec<(KindId, u64)>,
    /// What is being saved for: the Tool Age's cost while only the cost
    /// stands in its way. The military keeps its hands off it.
    saving: Cost,
    /// Villagers sent to a node last thought, so a villager still idle now
    /// tells us the node cannot be gathered from.
    sent: Vec<(EntityId, EntityId)>,
    /// Nodes not to send anyone to, and until when.
    avoid: Vec<(EntityId, u64)>,
}

/// A resource node the manager knows of: in sight, with what is left of
/// it, or remembered, with nothing known of what is left.
#[derive(Clone, Copy, Debug)]
struct Node {
    id: EntityId,
    resource: Resource,
    pos: Vec2Fx,
    /// What is left, if in sight.
    left: Option<i32>,
}

fn tile(p: Vec2Fx) -> (i32, i32) {
    (p.x.floor(), p.y.floor())
}

pub(crate) fn afford(stock: &Cost, cost: &Cost) -> bool {
    stock.iter().zip(cost).all(|(have, need)| have >= need)
}

pub(crate) fn spend(stock: &mut Cost, cost: &Cost) {
    for (have, need) in stock.iter_mut().zip(cost) {
        *have -= need;
    }
}

impl Economy {
    /// What the Tool Age still needs, while only its cost stands in the
    /// way; nothing otherwise.
    pub fn saving(&self) -> Cost {
        self.saving
    }

    /// One thought: the orders for this tick, in order. `stock` is what
    /// there is to spend; what this leaves in it is the military's.
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
        // Homeless: nothing this manager can do yet.
        let Some(tc) = mine
            .iter()
            .find(|s| s.kind == kinds::TOWN_CENTER && !s.site)
            .copied()
        else {
            return out;
        };
        let tc_tile = tile(tc.pos);
        let age = me.age.index().min(3);
        let villagers: Vec<&Sighting> = mine
            .iter()
            .filter(|s| s.kind == kinds::VILLAGER && !s.inside)
            .copied()
            .collect();
        // Those sheltering count toward the target too: a raid is not a
        // reason to train a second workforce.
        let villagers_all = mine.iter().filter(|s| s.kind == kinds::VILLAGER).count() as u32;
        // `stock` is spent as orders go out, so two orders in one thought
        // do not both count the same wood.
        let queued = view.queue(tc.id);
        let queued_villagers = queued
            .iter()
            .filter(|i| **i == Item::Unit(kinds::VILLAGER))
            .count() as u32;
        let tick = view.tick();
        // An order that has not become a site in ten seconds was refused
        // or its builder died: forget it and try again.
        self.ordered.retain(|(k, t)| {
            tick.saturating_sub(*t) < 200 && !mine.iter().any(|s| s.kind == *k && s.site)
        });
        // Villagers given an order this thought, so a later order in the
        // same thought does not take them off it.
        let mut taken: Vec<EntityId> = Vec::new();
        let owned = |kind: KindId| mine.iter().any(|s| s.kind == kind);
        let pending = |ordered: &[(KindId, u64)], kind: KindId| {
            ordered.iter().any(|(k, _)| *k == kind) || mine.iter().any(|s| s.kind == kind && s.site)
        };

        // A villager sent to a node last thought and idle now could not
        // gather from it: nobody is sent there again for a while.
        self.avoid.retain(|(_, until)| *until > tick);
        for (v, node) in std::mem::take(&mut self.sent) {
            if villagers.iter().any(|s| s.id == v && s.job == Job::Idle) {
                self.avoid.push((node, tick + 2400));
            }
        }
        let avoided = |id: EntityId| self.avoid.iter().any(|(n, _)| *n == id);
        // A node can be gathered from only if there is ground to stand on
        // beside it: a tree inside a forest is not one to send anyone to.
        let approachable = |pos: Vec2Fx| {
            let (x, y) = tile(pos);
            (-1..=1).any(|dx| {
                (-1..=1)
                    .any(|dy| (dx != 0 || dy != 0) && view.passable(x + dx, y + dy) == Some(true))
            })
        };
        // What is known to gather from: nodes in sight, own finished farms,
        // and nodes remembered out of sight. Animals are food on the hoof,
        // and there is no hunting yet.
        let mut nodes: Vec<Node> = seen
            .iter()
            .filter_map(|s| {
                let info = kinds::info(s.kind);
                let (resource, left) = s.resource?;
                let gatherable = !info.mobile
                    && left > 0
                    && (s.owner == kinds::GAIA || (s.owner == view.player() && !s.site))
                    && !avoided(s.id)
                    && approachable(s.pos);
                gatherable.then_some(Node {
                    id: s.id,
                    resource,
                    pos: s.pos,
                    left: Some(left),
                })
            })
            .collect();
        for r in view.remembered() {
            if let Some((resource, _)) = kinds::info(r.kind).resource {
                let pos = fogged::nav::centre(r.tile);
                if r.owner == kinds::GAIA
                    && !nodes.iter().any(|n| n.id == r.id)
                    && !avoided(r.id)
                    && approachable(pos)
                {
                    nodes.push(Node {
                        id: r.id,
                        resource,
                        pos,
                        left: None,
                    });
                }
            }
        }
        // In sight and not empty first; a memory may be of something gone.
        let nearest = |resource: Resource, to: Vec2Fx| -> Option<Node> {
            nodes
                .iter()
                .filter(|n| n.resource == resource)
                .min_by_key(|n| (n.left.is_none(), n.pos.distance_sq_raw(to), n.id))
                .copied()
        };
        // The order that sends a villager to a node: to gather from one in
        // sight, or to walk to a remembered one and see whether it is still
        // there, since a memory cannot be gathered from.
        let go = |v: EntityId, node: &Node| -> CommandKind {
            match node.left {
                Some(_) => CommandKind::Gather {
                    ids: vec![v],
                    node: node.id,
                },
                None => CommandKind::Move {
                    ids: vec![v],
                    target: node.pos,
                },
            }
        };

        // ----- The count: who gathers what, and what each share wants.
        // Until food and wood are stocked, the age's stone and gold shares
        // go to them: nothing is bought with gold that is not first paid
        // for in houses and farms.
        let n = villagers.len() as u32;
        let mut shares = order.shares[age];
        let basics_short =
            stock[Resource::Food.index()] < 200 || stock[Resource::Wood.index()] < 150;
        if basics_short {
            let extra = shares[Resource::Stone.index()] + shares[Resource::Gold.index()];
            shares[Resource::Stone.index()] = 0;
            shares[Resource::Gold.index()] = 0;
            let (food, wood) = if stock[Resource::Food.index()] < stock[Resource::Wood.index()] {
                (extra.div_ceil(2), extra / 2)
            } else {
                (extra / 2, extra.div_ceil(2))
            };
            shares[Resource::Food.index()] += food;
            shares[Resource::Wood.index()] += wood;
        }
        let mut have = [0u32; 4];
        for v in &villagers {
            if let Job::Gathering(r) = v.job {
                have[r.index()] += 1;
            }
        }
        let want = |r: usize| (n * shares[r] + 50) / 100;
        let known = |r: Resource| nodes.iter().any(|n| n.resource == r);

        // ----- A site nobody is building: its builder gave up or died.
        // The nearest villager not on a site of its own takes it over.
        let orphans: Vec<&Sighting> = mine
            .iter()
            .filter(|s| s.site && !villagers.iter().any(|v| v.job == Job::Building(s.id)))
            .copied()
            .collect();
        for site in orphans {
            if let Some(b) = builder(&villagers, site.pos, None, &taken) {
                out.push(CommandKind::Assist {
                    ids: vec![b],
                    site: site.id,
                });
                taken.push(b);
            }
        }

        // ----- Houses ahead of the cap (`GD-POP`): one at a time.
        if me.pop_cap < view.pop_cap_max()
            && me.pop_cap.saturating_sub(me.pop) <= order.headroom + queued_villagers
            && !pending(&self.ordered, kinds::HOUSE)
        {
            let cost = kinds::info(kinds::HOUSE).cost;
            if afford(stock, &cost) {
                if let Some((x, y)) = place(view, kinds::HOUSE, tc_tile, 3, 9, rng) {
                    if let Some(b) = builder(&villagers, fogged::nav::centre((x, y)), None, &taken)
                    {
                        out.push(CommandKind::Build {
                            kind: kinds::HOUSE,
                            x,
                            y,
                            ids: vec![b],
                        });
                        spend(stock, &cost);
                        self.ordered.push((kinds::HOUSE, tick));
                        taken.push(b);
                    }
                }
            }
        }

        // ----- Farms once the bushes are gone (Tool Age and up): before
        // anything the next age wants, because a settlement with no food
        // coming in buys nothing.
        let food_nodes = nodes
            .iter()
            .filter(|n| n.resource == Resource::Food && n.left.is_some())
            .count() as u32;
        let farms_wanted = want(Resource::Food.index()).div_ceil(2);
        let farm_sites = mine
            .iter()
            .filter(|s| s.kind == kinds::FARM && s.site)
            .count()
            + self
                .ordered
                .iter()
                .filter(|(k, _)| *k == kinds::FARM)
                .count();
        let farms_short = age >= Age::Tool.index() && food_nodes < farms_wanted;
        if age >= Age::Tool.index() {
            if !self.reseeding {
                out.push(CommandKind::SetAutoReseed { enabled: true });
                self.reseeding = true;
            }
            let cost = kinds::info(kinds::FARM).cost;
            if farms_short
                && farm_sites < 2
                && afford(stock, &cost)
                && view.can_build(kinds::FARM).is_ok()
            {
                if let Some((x, y)) = place(view, kinds::FARM, tc_tile, 2, 7, rng) {
                    if let Some(b) = builder(
                        &villagers,
                        fogged::nav::centre((x, y)),
                        Some(Resource::Food),
                        &taken,
                    ) {
                        out.push(CommandKind::Build {
                            kind: kinds::FARM,
                            x,
                            y,
                            ids: vec![b],
                        });
                        spend(stock, &cost);
                        self.ordered.push((kinds::FARM, tick));
                        taken.push(b);
                    }
                }
            }
        }

        // ----- The age gate: the buildings the next age needs, then the
        // advance itself.
        self.saving = [0; 4];
        if me.age.index() < order.last_age.index() {
            if let Some(next) = tech::age_advance(me.age) {
                if !queued.contains(&Item::Tech(next.id)) {
                    match view.can_research(tc.id, next.id) {
                        Ok(()) => {
                            out.push(CommandKind::Research {
                                building: tc.id,
                                tech: next.id,
                            });
                            spend(stock, &next.cost);
                        }
                        // Saved for only while the food engine is not
                        // built: the Tool Age brings the farms. Later
                        // ages compete with the army for what comes in.
                        Err(fogged::ResearchError::Unaffordable) if me.age == Age::Stone => {
                            self.saving = next.cost;
                        }
                        Err(_) => {}
                    }
                }
            }
            for &kind in AGE_BUILDINGS[age] {
                if farms_short {
                    // Farms first.
                    break;
                }
                if owned(kind) || pending(&self.ordered, kind) || view.can_build(kind).is_err() {
                    continue;
                }
                let cost = kinds::info(kind).cost;
                let mut with_reserve = cost;
                with_reserve[Resource::Wood.index()] += HOUSE_RESERVE;
                if !afford(stock, &with_reserve) {
                    break;
                }
                // The Storehouse goes by the wood it will take in; the rest
                // by the Town Center.
                let (around, min, max) = match nearest(Resource::Wood, tc.pos) {
                    Some(tree) if kind == kinds::STOREHOUSE => (tile(tree.pos), 2, 5),
                    _ => (tc_tile, 4, 10),
                };
                if let Some((x, y)) = place(view, kind, around, min, max, rng) {
                    if let Some(b) = builder(&villagers, fogged::nav::centre((x, y)), None, &taken)
                    {
                        out.push(CommandKind::Build {
                            kind,
                            x,
                            y,
                            ids: vec![b],
                        });
                        spend(stock, &cost);
                        self.ordered.push((kind, tick));
                        taken.push(b);
                    }
                }
                break;
            }
        }

        // ----- Villagers, while under the target and the queue is short.
        let cost = kinds::info(kinds::VILLAGER).cost;
        if villagers_all + queued_villagers < order.villagers[age]
            && queued_villagers < 2
            && afford(stock, &cost)
            && view.can_train(tc.id, kinds::VILLAGER).is_ok()
        {
            out.push(CommandKind::Train {
                building: tc.id,
                kind: kinds::VILLAGER,
            });
            spend(stock, &cost);
        }

        // ----- Idle villagers to the resource furthest below its share.
        let mut idle: Vec<&Sighting> = villagers
            .iter()
            .filter(|v| v.job == Job::Idle && !taken.contains(&v.id))
            .copied()
            .collect();
        idle.sort_by_key(|v| v.id);
        for v in idle {
            let pick = Resource::ALL
                .iter()
                .copied()
                .filter(|&r| known(r))
                .max_by_key(|&r| {
                    let i = r.index();
                    (want(i) as i32 - have[i] as i32, shares[i], 3 - i)
                });
            let Some(r) = pick else {
                break;
            };
            if let Some(node) = nearest(r, v.pos) {
                out.push(go(v.id, &node));
                self.sent.push((v.id, node.id));
                have[r.index()] += 1;
            }
        }

        // ----- One gatherer a thought from the resource most over its
        // share to the one most under it.
        if order.rebalances {
            let over = (0..4).max_by_key(|&i| (have[i] as i32 - want(i) as i32, i));
            let under = (0..4)
                .filter(|&i| known(Resource::ALL[i]))
                .max_by_key(|&i| (want(i) as i32 - have[i] as i32, 3 - i));
            if let (Some(o), Some(u)) = (over, under) {
                if have[o] > want(o) && have[u] + 1 < want(u) {
                    let to = Resource::ALL[u];
                    let from = Resource::ALL[o];
                    let mover = villagers
                        .iter()
                        .filter(|v| v.job == Job::Gathering(from) && !taken.contains(&v.id))
                        .min_by_key(|v| v.id)
                        .copied();
                    if let (Some(v), Some(node)) = (mover, nearest(to, tc.pos)) {
                        out.push(go(v.id, &node));
                    }
                }
            }
        }

        // ----- The rally: new villagers go straight to the most-wanted
        // resource.
        let wanted = Resource::ALL
            .iter()
            .copied()
            .filter(|&r| known(r))
            .max_by_key(|&r| {
                let i = r.index();
                (want(i) as i32 - have[i] as i32, shares[i], 3 - i)
            });
        if let Some(node) = wanted
            .and_then(|r| nearest(r, tc.pos))
            .filter(|n| n.left.is_some())
        {
            if self.rally != Some(node.id) {
                out.push(CommandKind::SetRally {
                    building: tc.id,
                    rally: Rally::Entity(node.id),
                });
                self.rally = Some(node.id);
            }
        }
        out
    }
}

/// A villager to send to a site near `to`: an idle one, else one gathering
/// `prefer` (or wood), the nearest first. A villager on a site is left to
/// finish it, and one in `taken` already has an order this thought.
pub(crate) fn builder(
    villagers: &[&Sighting],
    to: Vec2Fx,
    prefer: Option<Resource>,
    taken: &[EntityId],
) -> Option<EntityId> {
    let rank = |v: &Sighting| match v.job {
        Job::Idle => 0,
        Job::Gathering(r) if Some(r) == prefer => 1,
        Job::Gathering(Resource::Wood) => 2,
        Job::Gathering(_) => 3,
        Job::Busy => 4,
        Job::Building(_) | Job::Unknown => 9,
    };
    villagers
        .iter()
        .filter(|v| rank(v) < 9 && !taken.contains(&v.id))
        .min_by_key(|v| (rank(v), v.pos.distance_sq_raw(to), v.id))
        .map(|v| v.id)
}

/// A tile to put `kind` on: the first the view accepts on the rings
/// `min..=max` tiles around `around`, scanned from a corner the dice pick
/// so two towns are not laid out alike.
pub(crate) fn place(
    view: &FoggedView<'_>,
    kind: KindId,
    around: (i32, i32),
    min: i32,
    max: i32,
    rng: &mut Rng,
) -> Option<(i32, i32)> {
    let turn = rng.below(4) as usize;
    for r in min..=max {
        let mut ring = Vec::with_capacity((8 * r) as usize);
        for dx in -r..=r {
            ring.push((dx, -r));
            ring.push((dx, r));
        }
        for dy in (1 - r)..r {
            ring.push((-r, dy));
            ring.push((r, dy));
        }
        let start = turn * ring.len() / 4;
        for k in 0..ring.len() {
            let (dx, dy) = ring[(start + k) % ring.len()];
            let (x, y) = (around.0 + dx, around.1 + dy);
            if view.can_place(kind, x, y).is_ok() && !seals_a_pocket(view, kind, x, y) {
                return Some((x, y));
            }
        }
    }
    None
}

/// Open ground smaller than this beside a new building is a pocket.
const POCKET_TILES: usize = 48;

/// True if `kind` on `(x, y)` would shut some open ground beside it into a
/// pocket of fewer than [`POCKET_TILES`] tiles. Farms packed round a Town
/// Center did that to villagers standing between them, who then stood
/// idle for the rest of the match with every order failing at once. Each
/// open tile touching the footprint is flooded, four ways, as if the
/// building stood; a flood that runs out before the limit is a pocket.
fn seals_a_pocket(view: &FoggedView<'_>, kind: KindId, x: i32, y: i32) -> bool {
    let fp = kinds::info(kind).footprint.max(1) as i32;
    let footprint = fogged::nav::footprint_tiles(x, y, fp);
    let open = |t: (i32, i32)| !footprint.contains(&t) && view.passable(t.0, t.1) == Some(true);
    let mut checked: Vec<(i32, i32)> = Vec::new();
    for &(fx, fy) in &footprint {
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let start = (fx + dx, fy + dy);
            if !open(start) || checked.contains(&start) {
                continue;
            }
            let mut seen = vec![start];
            let mut queue = vec![start];
            while let Some((cx, cy)) = queue.pop() {
                if seen.len() >= POCKET_TILES {
                    break;
                }
                for (ex, ey) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let n = (cx + ex, cy + ey);
                    if open(n) && !seen.contains(&n) {
                        seen.push(n);
                        queue.push(n);
                    }
                }
            }
            if seen.len() < POCKET_TILES {
                return true;
            }
            checked.extend(seen);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_differ_by_difficulty_and_shares_sum_to_one() {
        for d in Difficulty::ALL {
            let o = BuildOrder::for_difficulty(d);
            for age in 0..4 {
                assert_eq!(o.shares[age].iter().sum::<u32>(), 100, "{d:?} age {age}");
                assert!(o.villagers[age] >= 6);
            }
            assert!(o.cadence >= 10 && o.cadence <= 40);
        }
        let easy = BuildOrder::for_difficulty(Difficulty::Easy);
        let hard = BuildOrder::for_difficulty(Difficulty::Hard);
        assert!(easy.villagers[0] < hard.villagers[0]);
        assert!(easy.cadence > hard.cadence, "Hard reacts faster");
        assert!(easy.last_age.index() < hard.last_age.index());
    }

    #[test]
    fn stock_arithmetic() {
        let mut stock = [100, 50, 0, 0];
        assert!(afford(&stock, &[100, 0, 0, 0]));
        assert!(!afford(&stock, &[0, 51, 0, 0]));
        spend(&mut stock, &[30, 20, 0, 0]);
        assert_eq!(stock, [70, 30, 0, 0]);
    }
}
