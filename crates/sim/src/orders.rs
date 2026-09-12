//! What a unit is doing, as an explicit state machine, and the per-player
//! state the economy needs.

use crate::entity::{EntityId, KindId};
use crate::fx::Fx;
use crate::hash::{HashState, StateHasher};
use crate::kinds::{Cost, Resource, CARRY_CAPACITY};
use crate::tech::{Age, TechId};
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};

/// How a unit answers enemies it has not been ordered at (`docs/02` §8.1,
/// `GD-STANCE-01`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum Stance {
    /// Pursues enemies it can see, then returns to where it stood.
    Aggressive = 0,
    /// Attacks enemies it can see, does not chase far. Soldiers' default.
    #[default]
    Defensive = 1,
    /// Attacks in range, never moves.
    StandGround = 2,
    /// Never attacks; runs for the Town Center when hit. Villagers'
    /// default.
    Passive = 3,
}

impl Stance {
    /// Every stance, in panel order.
    pub const ALL: [Stance; 4] = [
        Stance::Aggressive,
        Stance::Defensive,
        Stance::StandGround,
        Stance::Passive,
    ];

    /// The stance a fresh unit of `kind` takes.
    pub fn default_for(kind: KindId) -> Stance {
        if kind == crate::kinds::VILLAGER {
            Stance::Passive
        } else {
            Stance::Defensive
        }
    }

    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            Stance::Aggressive => "Aggressive",
            Stance::Defensive => "Defensive",
            Stance::StandGround => "Stand ground",
            Stance::Passive => "Passive",
        }
    }
}

/// The shape a group takes when ordered somewhere together (`UX-CMD-08`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum Formation {
    /// No shape: spread over the nearest open tiles, each at its own pace.
    /// Villagers' default.
    #[default]
    None = 0,
    /// Ranks abreast, facing the way they walk. Soldiers' default.
    Line = 1,
    /// As square as the count allows.
    Box = 2,
    /// Alternate ranks offset by half a step, with room between.
    Staggered = 3,
    /// Two wings with a gap between.
    Flank = 4,
}

impl Formation {
    /// Every formation, in the order the panel cycles them.
    pub const ALL: [Formation; 5] = [
        Formation::None,
        Formation::Line,
        Formation::Box,
        Formation::Staggered,
        Formation::Flank,
    ];

    /// The formation a fresh unit of `kind` takes.
    pub fn default_for(kind: KindId) -> Formation {
        if kind == crate::kinds::VILLAGER {
            Formation::None
        } else {
            Formation::Line
        }
    }

    /// The next one round.
    pub const fn next(self) -> Formation {
        match self {
            Formation::None => Formation::Line,
            Formation::Line => Formation::Box,
            Formation::Box => Formation::Staggered,
            Formation::Staggered => Formation::Flank,
            Formation::Flank => Formation::None,
        }
    }

    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            Formation::None => "No formation",
            Formation::Line => "Line",
            Formation::Box => "Box",
            Formation::Staggered => "Staggered",
            Formation::Flank => "Flank",
        }
    }
}

/// What a unit goes back to once the fight it picked is over.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Then {
    /// Stand where the fight ended.
    Idle,
    /// Walk back to where it stood before engaging.
    Return(Vec2Fx),
    /// Carry on advancing to a point, engaging on the way.
    AttackMove(Vec2Fx),
    /// Carry on patrolling between two points.
    Patrol(Vec2Fx, Vec2Fx, u8),
}

/// A unit's current job.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Order {
    /// Nothing. Villagers idle here are what the idle counter counts.
    #[default]
    Idle,
    /// Walk to a point and stop.
    Move {
        /// Destination.
        target: Vec2Fx,
    },
    /// Close with an enemy and hit it until one of them is dead.
    Attack {
        /// The enemy.
        target: EntityId,
        /// What to do once it is dead or out of reach.
        then: Then,
        /// How far from `origin` a stance lets the chase go; `None` for an
        /// ordered attack, which chases anywhere.
        leash: Option<(Vec2Fx, Fx)>,
    },
    /// Advance to a point, engaging anything seen on the way (`UX-CMD-02`).
    AttackMove {
        /// Destination.
        target: Vec2Fx,
    },
    /// Walk between two points, engaging (`UX-CMD-03`).
    Patrol {
        /// Where the patrol was ordered from.
        from: Vec2Fx,
        /// The far end.
        to: Vec2Fx,
        /// 0 heading `to`, 1 heading `from`.
        leg: u8,
    },
    /// Running from an attacker toward safety (`GD-STANCE-02`).
    Flee {
        /// Where safety is.
        target: Vec2Fx,
    },
    /// Gather from a node, carrying loads home until it is gone.
    Gather {
        /// The tree, bush or vein.
        node: EntityId,
        /// What it yields; remembered so a replacement can be found.
        resource: Resource,
        /// Where in the cycle.
        phase: GatherPhase,
    },
    /// Construct a building.
    Build {
        /// The site.
        site: EntityId,
        /// Standing next to it and working.
        working: bool,
    },
}

/// Stages of the gather cycle.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum GatherPhase {
    /// Walking to the node.
    ToNode,
    /// At the node, extracting.
    Working,
    /// Walking a load to a drop-off.
    ToDropoff {
        /// The building to deliver to.
        dropoff: EntityId,
    },
}

impl HashState for Order {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            Order::Idle => h.write_u8(0),
            Order::Move { target } => {
                h.write_u8(1);
                h.write(target);
            }
            Order::Gather {
                node,
                resource,
                phase,
            } => {
                h.write_u8(2);
                h.write(node);
                h.write_u8(resource.index() as u8);
                match phase {
                    GatherPhase::ToNode => h.write_u8(0),
                    GatherPhase::Working => h.write_u8(1),
                    GatherPhase::ToDropoff { dropoff } => {
                        h.write_u8(2);
                        h.write(dropoff);
                    }
                }
            }
            Order::Build { site, working } => {
                h.write_u8(3);
                h.write(site);
                h.write_bool(*working);
            }
            Order::Attack {
                target,
                then,
                leash,
            } => {
                h.write_u8(4);
                h.write(target);
                match then {
                    Then::Idle => h.write_u8(0),
                    Then::Return(p) => {
                        h.write_u8(1);
                        h.write(p);
                    }
                    Then::AttackMove(p) => {
                        h.write_u8(2);
                        h.write(p);
                    }
                    Then::Patrol(a, b, leg) => {
                        h.write_u8(3);
                        h.write(a);
                        h.write(b);
                        h.write_u8(*leg);
                    }
                }
                match leash {
                    None => h.write_u8(0),
                    Some((origin, radius)) => {
                        h.write_u8(1);
                        h.write(origin);
                        h.write(radius);
                    }
                }
            }
            Order::AttackMove { target } => {
                h.write_u8(5);
                h.write(target);
            }
            Order::Patrol { from, to, leg } => {
                h.write_u8(6);
                h.write(from);
                h.write(to);
                h.write_u8(*leg);
            }
            Order::Flee { target } => {
                h.write_u8(7);
                h.write(target);
            }
        }
    }
}

/// Progress of a trip.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum NavState {
    /// Waiting for a path.
    Planning,
    /// Following waypoints.
    Walking,
    /// Reached the goal (or as near as it gets).
    Arrived,
    /// Gave up: unreachable or hopelessly stuck.
    Failed,
}

/// Where a unit is walking, and the waypoints it will take.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Nav {
    /// Final destination.
    pub goal: Vec2Fx,
    /// Remaining waypoints, next first.
    pub waypoints: Vec<Vec2Fx>,
    /// Where the trip is.
    pub state: NavState,
    /// Closest the unit has been to the goal, for stuck detection.
    pub best: Fx,
    /// Ticks without getting closer.
    pub stalled: u16,
    /// Times the unit has asked the field for a new heading on this trip.
    /// A diagnostic, not an allowance: giving up is decided by
    /// `no_progress`, so a long detour can re-steer as often as it needs.
    pub replans: u8,
    /// Stop when within this distance of the goal.
    pub arrive: Fx,
    /// The flow field this trip follows: the destination tile, or the
    /// footprint of the thing being walked to. Units on the same errand
    /// share one field.
    pub field: crate::flow::FieldKey,
    /// Closest the unit has been to the goal, for telling a jam from a
    /// detour.
    pub best_goal: Fx,
    /// Where the unit was when `best_goal` was last improved.
    pub anchor: Vec2Fx,
    /// Ticks since `best_goal` last improved.
    pub no_progress: u16,
    /// Top speed for this trip in tiles per second, so a group in formation
    /// moves at its slowest member's pace; `Fx::MAX` for the unit's own.
    #[serde(default = "no_pace")]
    pub pace: Fx,
}

fn no_pace() -> Fx {
    Fx::MAX
}

impl Nav {
    /// A new trip to a point, following the field to that point's tile.
    pub fn to(goal: Vec2Fx, arrive: Fx) -> Nav {
        let t = crate::nav::tile_of(goal);
        Nav::along(goal, arrive, (t.0, t.1, 0))
    }

    /// A new trip to `goal` steered by the field for `field`, which may be
    /// a group's shared destination or the footprint of a building or node.
    pub fn along(goal: Vec2Fx, arrive: Fx, field: crate::flow::FieldKey) -> Nav {
        Nav {
            goal,
            waypoints: Vec::new(),
            state: NavState::Planning,
            best: Fx::MAX,
            stalled: 0,
            replans: 0,
            arrive,
            field,
            best_goal: Fx::MAX,
            anchor: Vec2Fx::new(Fx::MIN, Fx::MIN),
            no_progress: 0,
            pace: Fx::MAX,
        }
    }

    /// The same trip, held to `pace` tiles per second.
    pub fn paced(mut self, pace: Fx) -> Nav {
        self.pace = pace;
        self
    }
}

impl HashState for Nav {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&self.goal);
        h.write(&self.waypoints);
        h.write_u8(self.state as u8);
        h.write(&self.best);
        h.write_u16(self.stalled);
        h.write_u8(self.replans);
        h.write(&self.arrive);
        h.write_i32(self.field.0);
        h.write_i32(self.field.1);
        h.write_u8(self.field.2);
        h.write(&self.best_goal);
        h.write(&self.anchor);
        h.write_u16(self.no_progress);
        h.write(&self.pace);
    }
}

/// What a queue slot is producing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Item {
    /// A unit.
    Unit(KindId),
    /// A technology.
    Tech(TechId),
}

/// One item in a building's production queue.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct QueueItem {
    /// What is being produced.
    pub item: Item,
    /// Ticks of work done.
    pub progress: u32,
}

impl QueueItem {
    /// A unit to train.
    pub const fn unit(kind: KindId) -> QueueItem {
        QueueItem {
            item: Item::Unit(kind),
            progress: 0,
        }
    }

    /// A technology to research.
    pub const fn tech(tech: TechId) -> QueueItem {
        QueueItem {
            item: Item::Tech(tech),
            progress: 0,
        }
    }
}

/// Where a building sends what it produces.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Rally {
    /// Nowhere: units step out and wait.
    None,
    /// Walk to a point.
    Point(Vec2Fx),
    /// Go and work on an entity — gather a node or help a site.
    Entity(EntityId),
}

impl HashState for Rally {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            Rally::None => h.write_u8(0),
            Rally::Point(p) => {
                h.write_u8(1);
                h.write(p);
            }
            Rally::Entity(e) => {
                h.write_u8(2);
                h.write(e);
            }
        }
    }
}

/// A building's production state.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Production {
    /// Queued items, head first.
    pub queue: Vec<QueueItem>,
    /// Where finished units go.
    pub rally: Option<Rally>,
}

impl HashState for Production {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.queue.len() as u64);
        for q in &self.queue {
            match q.item {
                Item::Unit(k) => {
                    h.write_u8(0);
                    h.write_u16(k);
                }
                Item::Tech(t) => {
                    h.write_u8(1);
                    h.write_u16(t);
                }
            }
            h.write_u32(q.progress);
        }
        h.write(&self.rally.unwrap_or(Rally::None));
    }
}

/// Everything technology has changed about a player.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Modifiers {
    /// Percent added to the gather rate, per resource.
    pub gather_rate_pct: [i32; 4],
    /// Units added to carry capacity.
    pub carry_bonus: i32,
    /// Food added to a fresh farm.
    pub farm_yield_bonus: i32,
    /// Percent added to villager speed.
    pub villager_speed_pct: i32,
    /// Percent added to construction speed.
    pub build_speed_pct: i32,
    /// Attack added per hit, by [`Class::index`].
    pub attack_bonus: [i32; 8],
    /// Melee armour added, by class.
    pub melee_armour_bonus: [i32; 8],
    /// Pierce armour added, by class.
    pub pierce_armour_bonus: [i32; 8],
    /// Tiles of reach added, by class.
    pub range_bonus: [i32; 8],
}

impl Modifiers {
    /// Gather rate for a resource after modifiers, per second.
    pub fn gather_rate(&self, r: Resource) -> Fx {
        r.gather_rate().mul_div(
            Fx::from_int(100 + self.gather_rate_pct[r.index()]),
            Fx::from_int(100),
        )
    }

    /// Carry capacity after modifiers.
    pub fn carry_capacity(&self) -> i32 {
        (CARRY_CAPACITY + self.carry_bonus).max(1)
    }

    /// Food a fresh farm holds.
    pub fn farm_yield(&self, base: i32) -> i32 {
        base + self.farm_yield_bonus
    }
}

impl HashState for Modifiers {
    fn hash_state(&self, h: &mut StateHasher) {
        for v in self.gather_rate_pct {
            h.write_i32(v);
        }
        h.write_i32(self.carry_bonus);
        h.write_i32(self.farm_yield_bonus);
        h.write_i32(self.villager_speed_pct);
        h.write_i32(self.build_speed_pct);
        for table in [
            &self.attack_bonus,
            &self.melee_armour_bonus,
            &self.pierce_armour_bonus,
            &self.range_bonus,
        ] {
            for v in table {
                h.write_i32(*v);
            }
        }
    }
}

/// Per-player economy.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Player {
    /// Stockpile, indexed by [`Resource::index`].
    pub stockpile: Cost,
    /// Population in use.
    pub pop: u32,
    /// Population available from completed buildings, capped by the match limit.
    pub pop_cap: u32,
    /// Running totals gathered, for the score screen.
    pub gathered: Cost,
    /// Current age.
    pub age: Age,
    /// What technology has changed.
    pub modifiers: Modifiers,
    /// Technologies completed, sorted.
    pub researched: Vec<TechId>,
    /// Whether exhausted farms are reseeded automatically.
    pub auto_reseed: bool,
}

impl Player {
    /// A player with the standard opening stockpile.
    pub fn new() -> Player {
        Player::with_stockpile([200, 200, 100, 100])
    }

    /// A player with a chosen opening stockpile.
    pub fn with_stockpile(stockpile: Cost) -> Player {
        Player {
            stockpile,
            pop: 0,
            pop_cap: 0,
            gathered: [0; 4],
            age: Age::Stone,
            modifiers: Modifiers::default(),
            researched: Vec::new(),
            auto_reseed: true,
        }
    }

    /// True if the technology is complete.
    pub fn has_researched(&self, tech: TechId) -> bool {
        self.researched.binary_search(&tech).is_ok()
    }

    /// Records a completed technology.
    pub fn mark_researched(&mut self, tech: TechId) {
        if let Err(i) = self.researched.binary_search(&tech) {
            self.researched.insert(i, tech);
        }
    }

    /// True if the stockpile covers `cost`.
    pub fn can_afford(&self, cost: &Cost) -> bool {
        (0..4).all(|i| self.stockpile[i] >= cost[i])
    }

    /// Deducts `cost`. Returns false, changing nothing, if unaffordable.
    pub fn pay(&mut self, cost: &Cost) -> bool {
        if !self.can_afford(cost) {
            return false;
        }
        for (have, need) in self.stockpile.iter_mut().zip(cost) {
            *have -= need;
        }
        true
    }

    /// Refunds `cost`.
    pub fn refund(&mut self, cost: &Cost) {
        for (have, back) in self.stockpile.iter_mut().zip(cost) {
            *have += back;
        }
    }

    /// Adds gathered resources.
    pub fn deposit(&mut self, r: Resource, amount: i32) {
        self.stockpile[r.index()] += amount;
        self.gathered[r.index()] += amount;
    }
}

impl Default for Player {
    fn default() -> Self {
        Player::new()
    }
}

impl HashState for Player {
    fn hash_state(&self, h: &mut StateHasher) {
        for v in self.stockpile {
            h.write_i32(v);
        }
        h.write_u32(self.pop);
        h.write_u32(self.pop_cap);
        for v in self.gathered {
            h.write_i32(v);
        }
        h.write_u8(self.age as u8);
        h.write(&self.modifiers);
        h.write_u64(self.researched.len() as u64);
        for t in &self.researched {
            h.write_u16(*t);
        }
        h.write_bool(self.auto_reseed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paying_and_depositing() {
        let mut p = Player::new();
        assert!(p.can_afford(&[50, 0, 0, 0]));
        assert!(!p.can_afford(&[0, 0, 0, 500]));
        assert!(p.pay(&[50, 30, 0, 0]));
        assert_eq!(p.stockpile, [150, 170, 100, 100]);
        assert!(!p.pay(&[0, 0, 0, 500]));
        assert_eq!(
            p.stockpile,
            [150, 170, 100, 100],
            "failed payment changes nothing"
        );
        p.deposit(Resource::Gold, 10);
        assert_eq!(p.stockpile[3], 110);
        assert_eq!(p.gathered, [0, 0, 0, 10]);
        p.refund(&[50, 0, 0, 0]);
        assert_eq!(p.stockpile[0], 200);
    }

    #[test]
    fn hashes_distinguish_orders() {
        let mut a = StateHasher::new();
        Order::Idle.hash_state(&mut a);
        let mut b = StateHasher::new();
        Order::Move {
            target: Vec2Fx::ZERO,
        }
        .hash_state(&mut b);
        assert_ne!(a.finish(), b.finish());
    }
}
