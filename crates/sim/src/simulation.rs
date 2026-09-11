//! The simulation itself: state, the tick, and the systems that run in it.
//!
//! Every tick runs the same systems in the same order:
//!
//! 1. apply commands due this tick
//! 2. reseed exhausted farms whose owner can pay
//! 3. order state machines (decide where units need to be)
//! 4. path planning, budgeted
//! 5. animal wandering
//! 6. movement along waypoints, with stuck detection
//! 7. unit separation (movers push idle units aside)
//! 8. nudge anything standing in a blocked tile out of it
//! 9. construction progress
//! 10. production queues (units and technologies)
//! 11. population recount
//!
//! Nothing here reads a clock or a float; see the crate docs.

use crate::command::{Command, CommandKind, CommandQueue, PlayerId};
use crate::entity::{EntityId, KindId, Slot, World, WorldViolation};
use crate::fx::Fx;
use crate::hash::{HashState, StateHasher};
use crate::kinds::{self, Cost, Resource, GAIA, MAX_BUILDERS};
use crate::map::TileMap;
use crate::mapgen::{self, MapSpec};
use crate::nav::{self, NavGrid, Tile};
use crate::orders::{
    GatherPhase, Item, Modifiers, Nav, NavState, Order, Player, Production, QueueItem, Rally,
};
use crate::replay::Replay;
use crate::rng::Rng;
use crate::tech::{self, Age, Effect, TechId, AGE_BUILDINGS_REQUIRED};
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};

/// Simulation ticks per second of game time.
pub const TICKS_PER_SECOND: u32 = 20;
/// Milliseconds of game time per tick.
pub const TICK_MS: u32 = 1000 / TICKS_PER_SECOND;

/// A\* nodes one search may expand.
const PATH_BUDGET_PER_SEARCH: usize = 12_000;
/// A\* nodes all searches in one tick may expand together.
const PATH_BUDGET_PER_TICK: usize = 48_000;
/// Half the minimum distance between two units.
const UNIT_RADIUS: Fx = Fx::from_ratio(28, 100);
/// A unit is "at" a building or node within this distance of a footprint tile centre.
const REACH: Fx = Fx::from_ratio(15, 10);
/// A working unit keeps working until pushed this far from its footprint tile.
const REACH_SLACK: Fx = Fx::from_ratio(225, 100);
/// Ticks without progress before a walker reconsiders.
const STALL_TICKS: u16 = 40;
/// Production queue length.
const QUEUE_LIMIT: usize = 5;
/// How far a villager looks for a replacement node.
const REPLACEMENT_RADIUS: Fx = Fx::from_int(10);

/// Match parameters that are fixed for the whole match.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SimConfig {
    /// The map to generate.
    pub map: MapSpec,
    /// Hard cap on live entities; spawns beyond it are ignored.
    pub max_entities: u32,
    /// Whether wild animals wander.
    pub wander: bool,
    /// Population limit per player.
    pub pop_cap_max: u32,
    /// What every player starts with, indexed by [`Resource::index`].
    #[serde(default = "default_stockpile")]
    pub starting_stockpile: Cost,
}

/// The standard opening stockpile: food, wood, stone, gold.
pub const DEFAULT_STOCKPILE: Cost = [200, 200, 100, 100];

fn default_stockpile() -> Cost {
    DEFAULT_STOCKPILE
}

/// The population caps a skirmish may be set up with (`docs/02` §3.4).
pub const POP_CAP_RANGE: core::ops::RangeInclusive<u32> = 50..=200;

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            map: MapSpec::default(),
            max_entities: 4000,
            wander: true,
            pop_cap_max: 75,
            starting_stockpile: DEFAULT_STOCKPILE,
        }
    }
}

impl SimConfig {
    /// Checks the values a match setup screen may offer.
    ///
    /// The simulation itself honours any config it is given — tests and
    /// scenarios lean on caps far outside these bounds — so this is the
    /// front door's check, not the engine's. Nothing in the replay path
    /// calls it, because a replay recorded under a strange config must
    /// still verify.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !POP_CAP_RANGE.contains(&self.pop_cap_max) {
            return Err(ConfigError::PopCapOutOfRange {
                got: self.pop_cap_max,
            });
        }
        for (resource, &amount) in self.starting_stockpile.iter().enumerate() {
            if amount < 0 {
                return Err(ConfigError::NegativeStockpile { resource, amount });
            }
        }
        Ok(())
    }
}

/// Why a match setup was refused. See [`SimConfig::validate`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConfigError {
    /// `pop_cap_max` is outside [`POP_CAP_RANGE`].
    PopCapOutOfRange {
        /// The value offered.
        got: u32,
    },
    /// A starting stockpile entry is below zero.
    NegativeStockpile {
        /// Which resource, by [`Resource::index`].
        resource: usize,
        /// The value offered.
        amount: i32,
    },
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConfigError::PopCapOutOfRange { got } => write!(
                f,
                "population cap {got} is outside {}..={}",
                POP_CAP_RANGE.start(),
                POP_CAP_RANGE.end()
            ),
            ConfigError::NegativeStockpile { resource, amount } => {
                write!(f, "starting stockpile entry {resource} is {amount}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl HashState for SimConfig {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.map.kind as u8);
        h.write_u16(self.map.size);
        h.write_u8(self.map.players);
        h.write_u32(self.max_entities);
        h.write_bool(self.wander);
        h.write_u32(self.pop_cap_max);
        for v in self.starting_stockpile {
            h.write_i32(v);
        }
    }
}

/// Per-tick diagnostics. Not state: excluded from hashes, saves and equality.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct TickStats {
    /// Path searches run this tick.
    pub path_searches: u32,
    /// A\* nodes expanded this tick.
    pub path_nodes: u32,
    /// Walkers that gave up this tick.
    pub path_failures: u32,
    /// Walkers still waiting for a plan at the end of the tick.
    pub path_deferred: u32,
}

/// Reusable buffers. Deliberately invisible to equality and serialisation.
#[derive(Default)]
struct Scratch {
    path: nav::Scratch,
    head: Vec<u32>,
    next: Vec<u32>,
    stats: TickStats,
}

impl Clone for Scratch {
    fn clone(&self) -> Self {
        Scratch::default()
    }
}
impl PartialEq for Scratch {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for Scratch {}
impl core::fmt::Debug for Scratch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Scratch")
    }
}

/// Why a building cannot be placed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlaceError {
    /// Not a kind players build.
    NotBuildable,
    /// The player has not reached the age it belongs to.
    AgeLocked {
        /// The age it unlocks in.
        needs: Age,
    },
    /// Footprint off the map, on water, or over something.
    Blocked,
    /// Not enough resources.
    Unaffordable,
}

impl core::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlaceError::NotBuildable => write!(f, "not something you can build"),
            PlaceError::AgeLocked { needs } => write!(f, "needs the {}", needs.name()),
            PlaceError::Blocked => write!(f, "cannot build there"),
            PlaceError::Unaffordable => write!(f, "not enough resources"),
        }
    }
}

/// Why a technology cannot be queued. See [`Simulation::can_research`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResearchError {
    /// No such technology.
    UnknownTech,
    /// The building does not exist or belongs to someone else.
    NotYourBuilding,
    /// The technology is researched somewhere else.
    WrongBuilding,
    /// The building is still a site.
    UnderConstruction,
    /// The player has not reached the age it belongs to.
    AgeLocked {
        /// The age it unlocks in.
        needs: Age,
    },
    /// Another technology must come first.
    MissingPrerequisite {
        /// Which one.
        tech: TechId,
    },
    /// Already complete.
    AlreadyResearched,
    /// Already in a queue somewhere.
    AlreadyQueued,
    /// The building's queue is full.
    QueueFull,
    /// An age advance needs more buildings of the current age.
    NeedBuildings {
        /// Complete, counting buildings the player has.
        have: usize,
        /// How many the advance needs.
        need: usize,
    },
    /// Not enough resources.
    Unaffordable,
}

impl core::fmt::Display for ResearchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ResearchError::UnknownTech => write!(f, "no such technology"),
            ResearchError::NotYourBuilding => write!(f, "not your building"),
            ResearchError::WrongBuilding => write!(f, "researched elsewhere"),
            ResearchError::UnderConstruction => write!(f, "still under construction"),
            ResearchError::AgeLocked { needs } => write!(f, "needs the {}", needs.name()),
            ResearchError::MissingPrerequisite { tech } => {
                let name = tech::info(*tech).map_or("another technology", |t| t.name);
                write!(f, "needs {name}")
            }
            ResearchError::AlreadyResearched => write!(f, "already researched"),
            ResearchError::AlreadyQueued => write!(f, "already queued"),
            ResearchError::QueueFull => write!(f, "queue is full"),
            ResearchError::NeedBuildings { have, need } => {
                write!(f, "needs {need} buildings of this age, have {have}")
            }
            ResearchError::Unaffordable => write!(f, "not enough resources"),
        }
    }
}

/// A running match.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Simulation {
    seed: u64,
    tick: u64,
    config: SimConfig,
    rng: Rng,
    map: TileMap,
    /// Hash of the immutable map, folded into every state hash.
    map_hash: u64,
    starts: Vec<(i32, i32)>,
    players: Vec<Player>,
    nav: NavGrid,
    world: World,
    queue: CommandQueue,
    /// Every command ever issued, with its issue tick. This *is* the replay.
    log: Vec<(u64, Command)>,
    #[serde(skip)]
    scratch: Scratch,
}

impl Simulation {
    /// A fresh match at tick 0, with its map generated and populated.
    pub fn new(seed: u64, config: SimConfig) -> Simulation {
        let generated = mapgen::generate(seed, &config.map);
        let mut map_hasher = StateHasher::new();
        generated.tiles.hash_state(&mut map_hasher);
        let players = (0..config.map.players.clamp(1, 8))
            .map(|_| Player::with_stockpile(config.starting_stockpile))
            .collect();
        let nav = NavGrid::from_map(&generated.tiles);
        let mut sim = Simulation {
            seed,
            tick: 0,
            rng: Rng::new(seed),
            map: generated.tiles,
            map_hash: map_hasher.finish(),
            starts: generated.starts,
            players,
            nav,
            world: World::new(),
            queue: CommandQueue::new(),
            log: Vec::new(),
            scratch: Scratch::default(),
            config,
        };
        for s in &generated.spawns {
            sim.spawn(s.kind, s.owner, s.pos);
        }
        sim.nav.refresh();
        sim.recount_population();
        sim
    }

    // ----- accessors -----------------------------------------------------

    /// The seed this match was created with.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Ticks completed so far.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Match parameters.
    pub fn config(&self) -> &SimConfig {
        &self.config
    }

    /// Terrain and elevation.
    pub fn map(&self) -> &TileMap {
        &self.map
    }

    /// Passability.
    pub fn nav(&self) -> &NavGrid {
        &self.nav
    }

    /// Each player's Town Center tile.
    pub fn starts(&self) -> &[(i32, i32)] {
        &self.starts
    }

    /// Read access to entities.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Every player's economy.
    pub fn players(&self) -> &[Player] {
        &self.players
    }

    /// One player's economy, if the id is valid.
    pub fn player(&self, p: PlayerId) -> Option<&Player> {
        self.players.get(p as usize)
    }

    /// The RNG's draw count; handy in desync diagnostics.
    pub fn rng_draws(&self) -> u64 {
        self.rng.draws()
    }

    /// Diagnostics for the last tick.
    pub fn stats(&self) -> TickStats {
        self.scratch.stats
    }

    /// Villagers of `p` with nothing to do, in slot order.
    pub fn idle_villagers(&self, p: PlayerId) -> Vec<EntityId> {
        self.world
            .slots()
            .filter(|s| {
                let i = s.index();
                self.world.owner[i] == p
                    && self.world.kind[i] == kinds::VILLAGER
                    && self.world.order[i] == Order::Idle
            })
            .map(|s| self.world.id_at(s))
            .collect()
    }

    /// Whether `p` could place `kind` anchored at `(x, y)` right now.
    pub fn can_place(&self, p: PlayerId, kind: KindId, x: i32, y: i32) -> Result<(), PlaceError> {
        let info = kinds::info(kind);
        if !info.buildable {
            return Err(PlaceError::NotBuildable);
        }
        let Some(pl) = self.players.get(p as usize) else {
            return Err(PlaceError::Unaffordable);
        };
        if pl.age < info.age {
            return Err(PlaceError::AgeLocked { needs: info.age });
        }
        if !self.nav.footprint_clear(x, y, info.footprint as i32) {
            return Err(PlaceError::Blocked);
        }
        if !pl.can_afford(&info.cost) {
            return Err(PlaceError::Unaffordable);
        }
        Ok(())
    }

    /// Whether `p` could queue `tech` at `building` right now.
    ///
    /// The same check `Research` runs before paying, so a command panel can
    /// show why a button is grey with the words the simulation would use.
    pub fn can_research(
        &self,
        p: PlayerId,
        building: EntityId,
        id: TechId,
    ) -> Result<(), ResearchError> {
        let t = tech::info(id).ok_or(ResearchError::UnknownTech)?;
        let bs = self
            .owned_slot(building, p)
            .ok_or(ResearchError::NotYourBuilding)?;
        let i = bs.index();
        if self.world.kind[i] != t.building {
            return Err(ResearchError::WrongBuilding);
        }
        if self.world.construction[i].is_some() {
            return Err(ResearchError::UnderConstruction);
        }
        let player = self
            .players
            .get(p as usize)
            .ok_or(ResearchError::NotYourBuilding)?;
        if player.has_researched(id) {
            return Err(ResearchError::AlreadyResearched);
        }
        // An age advance is researched from exactly the age before it; any
        // other technology from its age onward.
        if t.advances_age().is_some() && player.age > t.age {
            return Err(ResearchError::AlreadyResearched);
        }
        if player.age < t.age {
            return Err(ResearchError::AgeLocked { needs: t.age });
        }
        for &r in t.requires {
            if !player.has_researched(r) {
                return Err(ResearchError::MissingPrerequisite { tech: r });
            }
        }
        if self.tech_queued(p, id) {
            return Err(ResearchError::AlreadyQueued);
        }
        if self.world.production[i]
            .as_ref()
            .is_some_and(|q| q.queue.len() >= QUEUE_LIMIT)
        {
            return Err(ResearchError::QueueFull);
        }
        if t.advances_age().is_some() {
            let have = self.age_buildings(p, player.age);
            if have < AGE_BUILDINGS_REQUIRED {
                return Err(ResearchError::NeedBuildings {
                    have,
                    need: AGE_BUILDINGS_REQUIRED,
                });
            }
        }
        if !player.can_afford(&t.cost) {
            return Err(ResearchError::Unaffordable);
        }
        Ok(())
    }

    /// Complete buildings of `p` from `age` that count toward advancing
    /// (`docs/02` §4: neither Houses, the Town Center nor Farms).
    pub fn age_buildings(&self, p: PlayerId, age: Age) -> usize {
        self.world
            .slots()
            .map(|s| s.index())
            .filter(|&i| {
                let k = self.world.kind[i];
                self.world.owner[i] == p
                    && self.world.construction[i].is_none()
                    && kinds::counts_for_age(k)
                    && kinds::info(k).age == age
            })
            .count()
    }

    /// True if `p` has `id` queued at any building.
    pub fn tech_queued(&self, p: PlayerId, id: TechId) -> bool {
        self.world.slots().any(|s| {
            let i = s.index();
            self.world.owner[i] == p
                && self.world.production[i]
                    .as_ref()
                    .is_some_and(|q| q.queue.iter().any(|q| q.item == Item::Tech(id)))
        })
    }

    /// A player's technology modifiers; the defaults for an owner the match
    /// does not have (Gaia, or a scenario's spare player).
    pub fn modifiers(&self, p: PlayerId) -> Modifiers {
        self.players
            .get(p as usize)
            .map_or_else(Modifiers::default, |pl| pl.modifiers)
    }

    // ----- input -----------------------------------------------------------

    /// Issues a command at the current tick. Returns the tick it will run on.
    pub fn issue(&mut self, command: Command) -> u64 {
        self.log.push((self.tick, command.clone()));
        self.queue.schedule(self.tick, command)
    }

    /// Advances the match by one tick. Fixed system order, no exceptions.
    pub fn step(&mut self) {
        self.scratch.stats = TickStats::default();
        self.apply_commands();
        self.farms();
        self.nav.refresh();
        self.orders();
        self.nav.refresh();
        self.plan_paths();
        self.wander();
        self.movement();
        self.separation();
        self.keep_off_blocked();
        self.construction();
        self.production();
        self.recount_population();
        self.tick += 1;
        #[cfg(feature = "debug-checks")]
        // Failing loudly is the whole point of this build configuration; the
        // shipping build does not compile this line.
        #[allow(clippy::panic)]
        if let Err(v) = self.check() {
            panic!("invariant broken at tick {}: {v}", self.tick);
        }
    }

    /// Canonical hash of everything that matters. Equal hashes on two
    /// machines at the same tick mean they are in the same world.
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_u64(self.tick);
        h.write(&self.config);
        h.write_u64(self.map_hash);
        h.write(&self.rng);
        for p in &self.players {
            h.write(p);
        }
        h.write(&self.nav);
        h.write(&self.world);
        h.write(&self.queue);
        h.finish()
    }

    /// Verifies every invariant the simulation is supposed to maintain.
    ///
    /// The structural invariants of the entity store, plus the ones that
    /// depend on the map and the players. Cheap enough to run every tick in
    /// tests; not run in shipping builds.
    ///
    /// Deliberately conservative: it asserts only what is genuinely always
    /// true. Two invariants that look obvious are *not* here, because both
    /// fire on legal states, and a checker that cries wolf is one people
    /// switch off:
    ///
    /// - `pop <= pop_cap` — destroying a house lowers the cap below the
    ///   population already alive.
    /// - "every owner is a real player" — [`kinds::GAIA`] owns the trees and
    ///   animals, and a `Spawn` command (a test and scenario facility) can
    ///   name a player the match does not have. `recount_population` skips
    ///   such owners on purpose, and every command from one is a no-op
    ///   because it owns nothing.
    pub fn check(&self) -> Result<(), Violation> {
        self.world.check().map_err(Violation::World)?;

        if self.world.len() as u32 > self.config.max_entities {
            return Err(Violation::OverEntityCap {
                live: self.world.len() as u32,
                cap: self.config.max_entities,
            });
        }

        let max_x = Fx::from_int(self.map.width());
        let max_y = Fx::from_int(self.map.height());
        let inside = |p: Vec2Fx| p.x >= Fx::ZERO && p.x < max_x && p.y >= Fx::ZERO && p.y < max_y;

        for slot in self.world.slots() {
            let i = slot.index();
            let s = i as u32;
            if !inside(self.world.pos[i]) {
                return Err(Violation::PositionOutOfMap {
                    slot: s,
                    pos: self.world.pos[i],
                });
            }
            if let Some(t) = self.world.move_target[i] {
                if !inside(t) {
                    return Err(Violation::TargetOutOfMap { slot: s, target: t });
                }
            }
            if self.world.health[i] < Fx::ZERO {
                return Err(Violation::NegativeHealth {
                    slot: s,
                    health: self.world.health[i],
                });
            }
            if self.world.resource[i] < 0 {
                return Err(Violation::NegativeResource {
                    slot: s,
                    amount: self.world.resource[i],
                });
            }
            if let Some((_, n)) = self.world.carry[i] {
                if n < 0 {
                    return Err(Violation::NegativeCarry { slot: s, amount: n });
                }
            }
            if self.world.facing[i] >= 8 {
                return Err(Violation::BadFacing {
                    slot: s,
                    facing: self.world.facing[i],
                });
            }
        }

        for (player, p) in self.players.iter().enumerate() {
            for (resource, &amount) in p.stockpile.iter().enumerate() {
                if amount < 0 {
                    return Err(Violation::NegativeStockpile {
                        player,
                        resource,
                        amount,
                    });
                }
            }
            for (resource, &amount) in p.gathered.iter().enumerate() {
                if amount < 0 {
                    return Err(Violation::NegativeGathered {
                        player,
                        resource,
                        amount,
                    });
                }
            }
            if p.pop_cap > self.config.pop_cap_max {
                return Err(Violation::PopCapAboveLimit {
                    player,
                    pop_cap: p.pop_cap,
                    limit: self.config.pop_cap_max,
                });
            }
        }
        Ok(())
    }

    /// Everything needed to reproduce this match up to the current tick.
    pub fn replay(&self) -> Replay {
        Replay {
            version: Replay::VERSION,
            seed: self.seed,
            config: self.config.clone(),
            ticks: self.tick,
            commands: self.log.clone(),
        }
    }

    // ----- entity lifecycle -------------------------------------------------

    fn spawn(&mut self, kind: KindId, owner: PlayerId, pos: Vec2Fx) -> Option<EntityId> {
        if self.world.len() as u32 >= self.config.max_entities {
            return None;
        }
        let info = kinds::info(kind);
        let pos = self.clamp_to_map(pos);
        let resource = info.resource.map_or(0, |(_, amount)| amount);
        let id = self.world.spawn_with_resource(
            kind,
            owner,
            pos,
            Fx::from_int(info.max_health),
            resource,
        );
        if info.footprint > 0 {
            let (ax, ay) = nav::anchor_tile(pos, info.footprint as i32);
            self.nav.block_footprint(ax, ay, info.footprint as i32);
        }
        if info.trains {
            self.world.production[id.index()] = Some(Production::default());
        }
        Some(id)
    }

    fn remove(&mut self, id: EntityId) -> bool {
        let Some(slot) = self.world.slot(id) else {
            return false;
        };
        let i = slot.index();
        let info = kinds::info(self.world.kind[i]);
        if info.footprint > 0 {
            let (ax, ay) = nav::anchor_tile(self.world.pos[i], info.footprint as i32);
            self.nav.unblock_footprint(ax, ay, info.footprint as i32);
            // Relabel now, not at the end of the tick.
            //
            // `remove` is reachable from the middle of `orders()`, when a
            // villager exhausts a node. Every villager processed after it in
            // the same pass queries `NavGrid::connected` to approach its own
            // node — and those queries would read component labels from
            // before this tile opened up. In a debug build the assertion in
            // `component` catches it; in release it silently answers from
            // stale data, so a villager can decide a reachable node is
            // unreachable. `step` already refreshes after `orders` for
            // exactly this reason; the gap was queries *within* the pass.
            //
            // A relabel is a full BFS, but a node is removed once in its
            // life, so this costs a sweep per exhausted node rather than one
            // per tick.
            self.nav.refresh();
        }
        if self.world.construction[i].is_some() {
            let cost = info.cost;
            if let Some(p) = self.players.get_mut(self.world.owner[i] as usize) {
                // Unfinished sites refund what has not been built yet.
                let done = self.world.construction[i]
                    .unwrap_or(0)
                    .min(info.build_work());
                let total = info.build_work().max(1);
                let back = cost.map(|c| c - c * done as i32 / total as i32);
                p.refund(&back);
            }
        }
        self.world.despawn(id)
    }

    // ----- commands ---------------------------------------------------------

    fn apply_commands(&mut self) {
        for cmd in self.queue.drain_due(self.tick) {
            self.apply(cmd);
        }
    }

    fn apply(&mut self, cmd: Command) {
        let p = cmd.player;
        match cmd.kind {
            CommandKind::Spawn { kind, pos } => {
                self.spawn(kind, p, pos);
            }
            CommandKind::Despawn { id } => {
                if self.owned_slot(id, p).is_some() {
                    self.remove(id);
                }
            }
            CommandKind::Move { ids, target } => {
                let units: Vec<Slot> = ids
                    .iter()
                    .filter_map(|&id| self.owned_mobile(id, p))
                    .collect();
                if units.is_empty() {
                    return;
                }
                let target = self.clamp_to_map(target);
                let (tx, ty) = nav::tile_of(target);
                let spots = self.nav.spread(tx, ty, units.len(), None);
                for (n, slot) in units.iter().enumerate() {
                    let goal = match spots.get(n) {
                        Some(&t) if n == 0 && t == (tx, ty) => target,
                        Some(&t) => nav::centre(t),
                        None => target,
                    };
                    let i = slot.index();
                    self.world.order[i] = Order::Move { target: goal };
                    self.world.nav[i] = Some(Nav::to(goal, Fx::from_ratio(15, 100)));
                }
            }
            CommandKind::Stop { ids } => {
                for id in ids {
                    if let Some(slot) = self.owned_mobile(id, p) {
                        self.world.order[slot.index()] = Order::Idle;
                        self.world.nav[slot.index()] = None;
                    }
                }
            }
            CommandKind::Gather { ids, node } => {
                let Some(ns) = self.world.slot(node) else {
                    return;
                };
                let kind = self.world.kind[ns.index()];
                if !self.gatherable_by(ns.index(), p) {
                    return;
                }
                let resource = kinds::info(kind)
                    .resource
                    .map(|(r, _)| r)
                    .unwrap_or(Resource::Food);
                for id in ids {
                    if let Some(slot) = self.owned_villager(id, p) {
                        let i = slot.index();
                        self.world.order[i] = Order::Gather {
                            node,
                            resource,
                            phase: GatherPhase::ToNode,
                        };
                        self.world.nav[i] = None;
                        self.world.work[i] = Fx::ZERO;
                    }
                }
            }
            CommandKind::Build { kind, x, y, ids } => {
                if self.can_place(p, kind, x, y).is_err() {
                    return;
                }
                let info = kinds::info(kind);
                self.players[p as usize].pay(&info.cost);
                let pos = nav::building_centre(x, y, info.footprint as i32);
                let Some(site) = self.spawn(kind, p, pos) else {
                    self.players[p as usize].refund(&info.cost);
                    return;
                };
                let i = site.index();
                self.world.construction[i] = Some(0);
                self.world.health[i] = Fx::ONE;
                // A farm is seeded when it is finished, not when it is pegged out.
                self.world.resource[i] = 0;
                self.assign_builders(&ids, p, site);
            }
            CommandKind::Assist { ids, site } => {
                match self.owned_slot(site, p) {
                    Some(s) if self.world.construction[s.index()].is_some() => {}
                    _ => return,
                }
                self.assign_builders(&ids, p, site);
            }
            CommandKind::Train { building, kind } => {
                let Some(bs) = self.owned_slot(building, p) else {
                    return;
                };
                let i = bs.index();
                let binfo = kinds::info(self.world.kind[i]);
                let uinfo = kinds::info(kind);
                if !binfo.trains
                    || self.world.construction[i].is_some()
                    || !uinfo.mobile
                    || kind != kinds::VILLAGER
                {
                    return;
                }
                let full = self.world.production[i]
                    .as_ref()
                    .is_some_and(|q| q.queue.len() >= QUEUE_LIMIT);
                if full || !self.players[p as usize].pay(&uinfo.cost) {
                    return;
                }
                self.world.production[i]
                    .get_or_insert_with(Production::default)
                    .queue
                    .push(QueueItem::unit(kind));
            }
            CommandKind::CancelTrain { building } => {
                let Some(bs) = self.owned_slot(building, p) else {
                    return;
                };
                let i = bs.index();
                if let Some(q) = self.world.production[i].as_mut() {
                    if let Some(item) = q.queue.pop() {
                        let cost = match item.item {
                            Item::Unit(k) => kinds::info(k).cost,
                            Item::Tech(t) => tech::info(t).map_or([0; 4], |t| t.cost),
                        };
                        self.players[p as usize].refund(&cost);
                    }
                }
            }
            CommandKind::Research { building, tech } => {
                if self.can_research(p, building, tech).is_err() {
                    return;
                }
                let Some(bs) = self.owned_slot(building, p) else {
                    return;
                };
                let Some(t) = tech::info(tech) else {
                    return;
                };
                if !self.players[p as usize].pay(&t.cost) {
                    return;
                }
                self.world.production[bs.index()]
                    .get_or_insert_with(Production::default)
                    .queue
                    .push(QueueItem::tech(tech));
            }
            CommandKind::SetAutoReseed { enabled } => {
                if let Some(pl) = self.players.get_mut(p as usize) {
                    pl.auto_reseed = enabled;
                }
            }
            CommandKind::SetRally { building, rally } => {
                let Some(bs) = self.owned_slot(building, p) else {
                    return;
                };
                let i = bs.index();
                if kinds::info(self.world.kind[i]).trains {
                    self.world.production[i]
                        .get_or_insert_with(Production::default)
                        .rally = Some(rally);
                }
            }
        }
    }

    fn assign_builders(&mut self, ids: &[EntityId], p: PlayerId, site: EntityId) {
        for &id in ids {
            if let Some(slot) = self.owned_villager(id, p) {
                let i = slot.index();
                self.world.order[i] = Order::Build {
                    site,
                    working: false,
                };
                self.world.nav[i] = None;
            }
        }
    }

    // ----- orders ------------------------------------------------------------

    fn orders(&mut self) {
        let slots: Vec<Slot> = self
            .world
            .slots()
            .filter(|s| {
                self.world.owner[s.index()] != GAIA
                    && kinds::info(self.world.kind[s.index()]).mobile
            })
            .collect();
        for slot in slots {
            let i = slot.index();
            match self.world.order[i] {
                Order::Idle => {}
                Order::Move { .. } => {
                    if self.nav_settled(i) {
                        self.world.nav[i] = None;
                        self.world.order[i] = Order::Idle;
                    }
                }
                Order::Gather {
                    node,
                    resource,
                    phase,
                } => self.tick_gather(slot, node, resource, phase),
                Order::Build { site, working } => self.tick_build(slot, site, working),
            }
        }
    }

    /// True once a walker has arrived or given up.
    fn nav_settled(&self, i: usize) -> bool {
        match &self.world.nav[i] {
            None => true,
            Some(n) => matches!(n.state, NavState::Arrived | NavState::Failed),
        }
    }

    fn nav_failed(&self, i: usize) -> bool {
        matches!(&self.world.nav[i], Some(n) if n.state == NavState::Failed)
    }

    fn tick_gather(&mut self, slot: Slot, node: EntityId, resource: Resource, phase: GatherPhase) {
        let i = slot.index();
        let me = self.world.owner[i];
        let node_slot = self
            .world
            .slot(node)
            .filter(|s| self.gatherable_by(s.index(), me));

        match phase {
            GatherPhase::ToNode => {
                let Some(ns) = node_slot else {
                    return self.gather_node_gone(slot, resource);
                };
                if self.nav_failed(i) {
                    self.world.nav[i] = None;
                    self.world.order[i] = Order::Idle;
                    return;
                }
                if !self.nav_settled(i) {
                    return;
                }
                self.world.nav[i] = None;
                if self.within_reach(i, ns) {
                    self.world.work[i] = Fx::ZERO;
                    self.world.order[i] = Order::Gather {
                        node,
                        resource,
                        phase: GatherPhase::Working,
                    };
                } else if let Some(goal) = self.approach(i, ns) {
                    self.world.nav[i] = Some(Nav::to(goal, Fx::from_ratio(2, 10)));
                } else {
                    // Walled in (a building went up against it, say): treat
                    // it as gone and look for another.
                    self.gather_node_gone(slot, resource);
                }
            }
            GatherPhase::Working => {
                let Some(ns) = node_slot else {
                    return self.gather_node_gone(slot, resource);
                };
                if !self.within(i, ns, REACH_SLACK) {
                    self.world.order[i] = Order::Gather {
                        node,
                        resource,
                        phase: GatherPhase::ToNode,
                    };
                    return;
                }
                let n = ns.index();
                self.world.facing[i] = (self.world.pos[n] - self.world.pos[i]).angle().facing8();
                // Switching resources drops the old load.
                if matches!(self.world.carry[i], Some((r, _)) if r != resource) {
                    self.world.carry[i] = None;
                }
                let modifiers = self.modifiers(me);
                let rate = modifiers.gather_rate(resource) / TICKS_PER_SECOND as i32;
                self.world.work[i] += rate;
                if self.world.work[i] < Fx::ONE {
                    return;
                }
                let capacity = modifiers.carry_capacity();
                let carried = self.world.carry[i].map_or(0, |(_, a)| a);
                let take = self.world.work[i]
                    .floor()
                    .min(self.world.resource[n])
                    .min(capacity - carried)
                    .max(0);
                self.world.work[i] -= Fx::from_int(take);
                self.world.resource[n] -= take;
                let carried = carried + take;
                self.world.carry[i] = Some((resource, carried));
                // An exhausted node is gone; an exhausted farm stays, empty,
                // for `farms` to reseed when its owner can pay.
                if self.world.resource[n] <= 0 && self.world.kind[n] != kinds::FARM {
                    self.remove(node);
                }
                if carried >= capacity {
                    self.go_dropoff(slot, node, resource, me);
                }
            }
            GatherPhase::ToDropoff { dropoff } => {
                let ds = self
                    .world
                    .slot(dropoff)
                    .filter(|s| self.is_dropoff_for(s.index(), me));
                let Some(ds) = ds else {
                    return self.go_dropoff(slot, node, resource, me);
                };
                if self.nav_failed(i) {
                    self.world.nav[i] = None;
                    self.world.order[i] = Order::Idle;
                    return;
                }
                if !self.nav_settled(i) {
                    return;
                }
                self.world.nav[i] = None;
                if self.within_reach(i, ds) {
                    if let Some((r, amount)) = self.world.carry[i].take() {
                        self.players[me as usize].deposit(r, amount);
                    }
                    self.world.order[i] = Order::Gather {
                        node,
                        resource,
                        phase: GatherPhase::ToNode,
                    };
                } else if let Some(goal) = self.approach(i, ds) {
                    self.world.nav[i] = Some(Nav::to(goal, Fx::from_ratio(2, 10)));
                } else {
                    self.world.order[i] = Order::Idle;
                }
            }
        }
    }

    /// The node is gone: deliver what we carry, then find another of the
    /// same resource nearby, else idle.
    fn gather_node_gone(&mut self, slot: Slot, resource: Resource) {
        let i = slot.index();
        let me = self.world.owner[i];
        if self.world.carry[i].is_some_and(|(_, a)| a > 0) {
            if let Some(d) = self.nearest_dropoff(i, me) {
                // Keep a dead handle as the node; after depositing we retry the search.
                let node = self.world.id_at(slot);
                self.world.order[i] = Order::Gather {
                    node,
                    resource,
                    phase: GatherPhase::ToDropoff { dropoff: d },
                };
                self.world.nav[i] = None;
                return;
            }
        }
        match self.nearest_node(i, resource) {
            Some(node) => {
                self.world.order[i] = Order::Gather {
                    node,
                    resource,
                    phase: GatherPhase::ToNode,
                };
                self.world.nav[i] = None;
            }
            None => {
                self.world.order[i] = Order::Idle;
                self.world.nav[i] = None;
            }
        }
    }

    fn go_dropoff(&mut self, slot: Slot, node: EntityId, resource: Resource, me: PlayerId) {
        let i = slot.index();
        match self.nearest_dropoff(i, me) {
            Some(d) => {
                self.world.order[i] = Order::Gather {
                    node,
                    resource,
                    phase: GatherPhase::ToDropoff { dropoff: d },
                };
                self.world.nav[i] = None;
            }
            None => {
                self.world.order[i] = Order::Idle;
                self.world.nav[i] = None;
            }
        }
    }

    fn tick_build(&mut self, slot: Slot, site: EntityId, working: bool) {
        let i = slot.index();
        let me = self.world.owner[i];
        let ss = self.world.slot(site).filter(|s| {
            self.world.owner[s.index()] == me && self.world.construction[s.index()].is_some()
        });
        let Some(ss) = ss else {
            self.world.order[i] = Order::Idle;
            self.world.nav[i] = None;
            return;
        };
        if working {
            if self.within(i, ss, REACH_SLACK) {
                self.world.facing[i] = (self.world.pos[ss.index()] - self.world.pos[i])
                    .angle()
                    .facing8();
            } else {
                self.world.order[i] = Order::Build {
                    site,
                    working: false,
                };
            }
            return;
        }
        if self.nav_failed(i) {
            self.world.nav[i] = None;
            self.world.order[i] = Order::Idle;
            return;
        }
        if !self.nav_settled(i) {
            return;
        }
        self.world.nav[i] = None;
        if self.within_reach(i, ss) {
            self.world.order[i] = Order::Build {
                site,
                working: true,
            };
        } else if let Some(goal) = self.approach(i, ss) {
            self.world.nav[i] = Some(Nav::to(goal, Fx::from_ratio(2, 10)));
        } else {
            self.world.order[i] = Order::Idle;
        }
    }

    // ----- spatial helpers ---------------------------------------------------

    fn footprint_of(&self, slot: usize) -> Vec<Tile> {
        let fp = kinds::info(self.world.kind[slot]).footprint as i32;
        if fp == 0 {
            vec![nav::tile_of(self.world.pos[slot])]
        } else {
            let (ax, ay) = nav::anchor_tile(self.world.pos[slot], fp);
            nav::footprint_tiles(ax, ay, fp)
        }
    }

    /// True if unit `i` stands within [`REACH`] of any footprint tile of `target`.
    fn within_reach(&self, i: usize, target: Slot) -> bool {
        self.within(i, target, REACH)
    }

    /// True if unit `i` stands within `dist` of any footprint tile of `target`.
    fn within(&self, i: usize, target: Slot, dist: Fx) -> bool {
        let pos = self.world.pos[i];
        self.footprint_of(target.index())
            .into_iter()
            .any(|t| pos.distance(nav::centre(t)) <= dist)
    }

    /// True if the unit is standing still at a job and should not be shoved.
    fn anchored(&self, i: usize) -> bool {
        matches!(
            self.world.order[i],
            Order::Gather {
                phase: GatherPhase::Working,
                ..
            } | Order::Build { working: true, .. }
        )
    }

    /// The tile a unit should stand on to work at `target`: the nearest
    /// reachable passable tile adjacent to its footprint, preferring one no
    /// other worker is already anchored on so a crowd spreads round a node.
    fn approach(&self, i: usize, target: Slot) -> Option<Vec2Fx> {
        let pos = self.world.pos[i];
        let from = self.standing_tile(i)?;
        let fp = self.footprint_of(target.index());
        let (min_x, max_x) = (fp.iter().map(|t| t.0).min()?, fp.iter().map(|t| t.0).max()?);
        let (min_y, max_y) = (fp.iter().map(|t| t.1).min()?, fp.iter().map(|t| t.1).max()?);
        let taken: Vec<Tile> = self
            .world
            .slots()
            .map(|s| s.index())
            .filter(|&j| j != i && self.anchored(j))
            .map(|j| nav::tile_of(self.world.pos[j]))
            .collect();
        let mut best: Option<(bool, u64, Tile)> = None;
        for y in min_y - 1..=max_y + 1 {
            for x in min_x - 1..=max_x + 1 {
                let inside = x >= min_x && x <= max_x && y >= min_y && y <= max_y;
                if inside || !self.nav.passable(x, y) || !self.nav.connected(from, (x, y)) {
                    continue;
                }
                let occupied = taken.contains(&(x, y));
                let d = pos.distance_sq_raw(nav::centre((x, y)));
                let key = (occupied, d, (x, y));
                if best.is_none_or(|b| (key.0, key.1) < (b.0, b.1)) {
                    best = Some(key);
                }
            }
        }
        best.map(|(_, _, t)| nav::centre(t))
    }

    /// The passable tile a unit counts as standing on.
    fn standing_tile(&self, i: usize) -> Option<Tile> {
        let t = nav::tile_of(self.world.pos[i]);
        if self.nav.passable(t.0, t.1) {
            Some(t)
        } else {
            self.nav.nearest_passable(t.0, t.1, 4, None)
        }
    }

    /// True if `p`'s villagers may gather from entity `n` right now: a
    /// static node with something left, not a site, and — for a farm —
    /// theirs. Gaia's nodes are everyone's; a farm is its owner's.
    fn gatherable_by(&self, n: usize, p: PlayerId) -> bool {
        let k = self.world.kind[n];
        kinds::gatherable(k)
            && self.world.resource[n] > 0
            && self.world.construction[n].is_none()
            && (k != kinds::FARM || self.world.owner[n] == p)
    }

    /// Reseeds every exhausted farm whose owner has auto-reseed on and the
    /// wood to pay for it. Runs before orders, so a villager working a farm
    /// that ran dry last tick finds it full again before it looks elsewhere.
    fn farms(&mut self) {
        let base = kinds::info(kinds::FARM)
            .resource
            .map_or(0, |(_, amount)| amount);
        for slot in self.world.slots().collect::<Vec<_>>() {
            let i = slot.index();
            if self.world.kind[i] != kinds::FARM
                || self.world.resource[i] > 0
                || self.world.construction[i].is_some()
            {
                continue;
            }
            let Some(p) = self.players.get_mut(self.world.owner[i] as usize) else {
                continue;
            };
            if !p.auto_reseed || !p.pay(&kinds::FARM_RESEED_COST) {
                continue;
            }
            self.world.resource[i] = p.modifiers.farm_yield(base);
        }
    }

    fn is_dropoff_for(&self, slot: usize, p: PlayerId) -> bool {
        self.world.owner[slot] == p
            && kinds::info(self.world.kind[slot]).dropoff
            && self.world.construction[slot].is_none()
    }

    fn nearest_dropoff(&self, i: usize, p: PlayerId) -> Option<EntityId> {
        let pos = self.world.pos[i];
        self.world
            .slots()
            .filter(|s| self.is_dropoff_for(s.index(), p))
            .map(|s| (pos.distance_sq_raw(self.world.pos[s.index()]), s))
            .min_by_key(|&(d, s)| (d, s.index()))
            .map(|(_, s)| self.world.id_at(s))
    }

    /// The nearest node of `resource` within [`REPLACEMENT_RADIUS`] that the
    /// villager can stand beside. Candidates are checked nearest first, so
    /// the reachability test runs only until one passes.
    fn nearest_node(&self, i: usize, resource: Resource) -> Option<EntityId> {
        let pos = self.world.pos[i];
        let me = self.world.owner[i];
        let limit = REPLACEMENT_RADIUS.raw() as u64 * REPLACEMENT_RADIUS.raw() as u64;
        let mut candidates: Vec<(u64, Slot)> = self
            .world
            .slots()
            .filter(|s| {
                let k = self.world.kind[s.index()];
                self.gatherable_by(s.index(), me)
                    && kinds::info(k).resource.is_some_and(|(r, _)| r == resource)
            })
            .map(|s| (pos.distance_sq_raw(self.world.pos[s.index()]), s))
            .filter(|&(d, _)| d <= limit)
            .collect();
        candidates.sort_by_key(|&(d, s)| (d, s.index()));
        candidates
            .into_iter()
            .find(|&(_, s)| self.approach(i, s).is_some())
            .map(|(_, s)| self.world.id_at(s))
    }

    // ----- planning and movement ----------------------------------------------

    fn plan_paths(&mut self) {
        let mut budget = PATH_BUDGET_PER_TICK;
        let slots: Vec<Slot> = self
            .world
            .slots()
            .filter(
                |s| matches!(&self.world.nav[s.index()], Some(n) if n.state == NavState::Planning),
            )
            .collect();
        for slot in slots {
            let i = slot.index();
            if budget == 0 {
                self.scratch.stats.path_deferred += 1;
                continue;
            }
            let pos = self.world.pos[i];
            let Some(from) = self.standing_tile(i) else {
                self.fail_nav(i);
                continue;
            };
            let mut goal = self.world.nav[i].as_ref().map(|n| n.goal).unwrap_or(pos);
            let gt = nav::tile_of(goal);
            if !self.nav.passable(gt.0, gt.1) || !self.nav.connected(from, gt) {
                match self.nav.nearest_passable(gt.0, gt.1, 10, Some(from)) {
                    Some(t) => {
                        goal = nav::centre(t);
                        if let Some(n) = self.world.nav[i].as_mut() {
                            n.goal = goal;
                        }
                    }
                    None => {
                        self.fail_nav(i);
                        continue;
                    }
                }
            }
            let per = budget.min(PATH_BUDGET_PER_SEARCH);
            let found = self.nav.find_path(pos, goal, per, &mut self.scratch.path);
            let used = self.scratch.path.expanded;
            self.scratch.path.expanded = 0;
            budget = budget.saturating_sub(used.max(1));
            self.scratch.stats.path_searches += 1;
            self.scratch.stats.path_nodes += used as u32;
            let Some(n) = self.world.nav[i].as_mut() else {
                continue;
            };
            match found {
                Some(way) => {
                    n.waypoints = way;
                    n.state = NavState::Walking;
                    n.best = Fx::MAX;
                    n.stalled = 0;
                }
                None => {
                    n.replans += 1;
                    if n.replans >= 3 {
                        n.state = NavState::Failed;
                        self.scratch.stats.path_failures += 1;
                    }
                }
            }
        }
    }

    fn fail_nav(&mut self, i: usize) {
        if let Some(n) = self.world.nav[i].as_mut() {
            n.state = NavState::Failed;
        }
        self.scratch.stats.path_failures += 1;
    }

    /// Wild animals pick random nearby destinations.
    fn wander(&mut self) {
        if !self.config.wander {
            return;
        }
        let slots: Vec<Slot> = self
            .world
            .slots()
            .filter(|s| {
                self.world.owner[s.index()] == GAIA
                    && kinds::info(self.world.kind[s.index()]).mobile
            })
            .collect();
        for slot in slots {
            let i = slot.index();
            if self.world.move_target[i].is_some() || !self.rng.chance(1, 200) {
                continue;
            }
            let dx = self.rng.range_i32(-3, 4);
            let dy = self.rng.range_i32(-3, 4);
            let target = self.clamp_to_map(self.world.pos[i] + Vec2Fx::from_int(dx, dy));
            let t = nav::tile_of(target);
            if self.nav.passable(t.0, t.1) {
                self.world.move_target[i] = Some(target);
            }
        }
    }

    fn movement(&mut self) {
        for slot in self.world.slots().collect::<Vec<_>>() {
            let i = slot.index();
            let info = kinds::info(self.world.kind[i]);
            if !info.mobile {
                continue;
            }
            let per_second = match self.world.kind[i] {
                kinds::VILLAGER => {
                    let pct = self.modifiers(self.world.owner[i]).villager_speed_pct;
                    if pct == 0 {
                        info.speed_per_second
                    } else {
                        info.speed_per_second
                            .mul_div(Fx::from_int(100 + pct), Fx::from_int(100))
                    }
                }
                _ => info.speed_per_second,
            };
            let speed = per_second / TICKS_PER_SECOND as i32;
            let here = self.world.pos[i];

            // Animals: straight-line wander targets.
            if let Some(target) = self.world.move_target[i] {
                if here != target {
                    self.world.facing[i] = (target - here).angle().facing8();
                }
                let next = here.move_toward(target, speed);
                self.world.pos[i] = next;
                if next == target {
                    self.world.move_target[i] = None;
                }
                continue;
            }

            let Some(n) = self.world.nav[i].as_mut() else {
                continue;
            };
            if n.state != NavState::Walking {
                continue;
            }
            let Some(&w) = n.waypoints.first() else {
                n.state = NavState::Arrived;
                continue;
            };
            let wt = nav::tile_of(w);
            if !self.nav.passable(wt.0, wt.1) {
                // Something was built on the way; plan again.
                n.waypoints.clear();
                n.replans += 1;
                n.state = if n.replans >= 4 {
                    NavState::Failed
                } else {
                    NavState::Planning
                };
                continue;
            }
            if w != here {
                self.world.facing[i] = (w - here).angle().facing8();
            }
            let next = here.move_toward(w, speed);
            self.world.pos[i] = next;
            let to_goal = next.distance(n.goal);
            if next == w {
                n.waypoints.remove(0);
                n.best = Fx::MAX;
                n.stalled = 0;
            }
            if n.waypoints.is_empty() || to_goal <= n.arrive {
                n.state = NavState::Arrived;
                continue;
            }
            // Progress is measured along the path — toward the next waypoint —
            // so a detour that walks away from the goal is not a stall.
            let to_waypoint = next.distance(n.waypoints[0]);
            if to_waypoint < n.best - Fx::from_ratio(1, 100) {
                n.best = to_waypoint;
                n.stalled = 0;
            } else {
                n.stalled += 1;
            }
            if n.stalled > STALL_TICKS {
                n.stalled = 0;
                n.best = Fx::MAX;
                if n.replans < 3 {
                    n.replans += 1;
                    n.waypoints.clear();
                    n.state = NavState::Planning;
                } else if to_goal <= Fx::from_ratio(5, 2) {
                    n.state = NavState::Arrived;
                } else {
                    n.state = NavState::Failed;
                    self.scratch.stats.path_failures += 1;
                }
            }
        }
    }

    /// Pushes overlapping units apart. Walkers shove idle units aside more
    /// than they are shoved, so a crowd parts for someone with somewhere to be.
    fn separation(&mut self) {
        let w = self.nav.width();
        let h = self.nav.height();
        let cells = (w * h) as usize;
        let cap = self.world.capacity();
        self.scratch.head.clear();
        self.scratch.head.resize(cells, u32::MAX);
        self.scratch.next.clear();
        self.scratch.next.resize(cap, u32::MAX);

        let mobile: Vec<usize> = self
            .world
            .slots()
            .map(|s| s.index())
            .filter(|&i| kinds::info(self.world.kind[i]).mobile)
            .collect();
        for &i in &mobile {
            let t = nav::tile_of(self.world.pos[i]);
            if self.nav.in_bounds(t.0, t.1) {
                let c = (t.1 * w + t.0) as usize;
                self.scratch.next[i] = self.scratch.head[c];
                self.scratch.head[c] = i as u32;
            }
        }

        let min_dist = UNIT_RADIUS * 2;
        let limit = min_dist.raw() as u64 * min_dist.raw() as u64;
        for &i in &mobile {
            let t = nav::tile_of(self.world.pos[i]);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (cx, cy) = (t.0 + dx, t.1 + dy);
                    if !self.nav.in_bounds(cx, cy) {
                        continue;
                    }
                    let mut j = self.scratch.head[(cy * w + cx) as usize];
                    while j != u32::MAX {
                        let ju = j as usize;
                        if ju > i {
                            let pi = self.world.pos[i];
                            let pj = self.world.pos[ju];
                            let dsq = pi.distance_sq_raw(pj);
                            if dsq < limit {
                                let delta = pj - pi;
                                let d = delta.length();
                                let dir = if d.is_zero() {
                                    // Coincident: separate along a fixed axis by slot parity.
                                    if i % 2 == 0 {
                                        Vec2Fx::new(Fx::ONE, Fx::ZERO)
                                    } else {
                                        Vec2Fx::new(Fx::ZERO, Fx::ONE)
                                    }
                                } else {
                                    delta.scale_ratio(Fx::ONE, d)
                                };
                                let overlap = min_dist - d;
                                let (wi, wj) = match (self.anchored(i), self.anchored(ju)) {
                                    (true, true) => {
                                        j = self.scratch.next[ju];
                                        continue;
                                    }
                                    (true, false) => (Fx::ZERO, Fx::ONE),
                                    (false, true) => (Fx::ONE, Fx::ZERO),
                                    (false, false) => {
                                        match (self.is_walking(i), self.is_walking(ju)) {
                                            (true, false) => {
                                                (Fx::from_ratio(1, 4), Fx::from_ratio(3, 4))
                                            }
                                            (false, true) => {
                                                (Fx::from_ratio(3, 4), Fx::from_ratio(1, 4))
                                            }
                                            _ => (Fx::HALF, Fx::HALF),
                                        }
                                    }
                                };
                                let ni = pi - dir * (overlap * wi);
                                let nj = pj + dir * (overlap * wj);
                                if self.nav.passable(ni.x.floor(), ni.y.floor()) {
                                    self.world.pos[i] = ni;
                                }
                                if self.nav.passable(nj.x.floor(), nj.y.floor()) {
                                    self.world.pos[ju] = nj;
                                }
                            }
                        }
                        j = self.scratch.next[ju];
                    }
                }
            }
        }
    }

    fn is_walking(&self, i: usize) -> bool {
        self.world.move_target[i].is_some()
            || matches!(&self.world.nav[i], Some(n) if n.state == NavState::Walking)
    }

    /// Anything standing where it cannot stand steps to the nearest open tile.
    fn keep_off_blocked(&mut self) {
        for slot in self.world.slots().collect::<Vec<_>>() {
            let i = slot.index();
            if !kinds::info(self.world.kind[i]).mobile {
                continue;
            }
            let pos = self.clamp_to_map(self.world.pos[i]);
            let t = nav::tile_of(pos);
            if self.nav.passable(t.0, t.1) {
                self.world.pos[i] = pos;
                continue;
            }
            if let Some(free) = self.nav.nearest_passable(t.0, t.1, 6, None) {
                self.world.pos[i] = nav::centre(free);
                if let Some(n) = self.world.nav[i].as_mut() {
                    if n.state == NavState::Walking {
                        n.waypoints.clear();
                        n.state = NavState::Planning;
                    }
                }
            }
        }
    }

    // ----- buildings -----------------------------------------------------------

    fn construction(&mut self) {
        let sites: Vec<Slot> = self
            .world
            .slots()
            .filter(|s| self.world.construction[s.index()].is_some())
            .collect();
        if sites.is_empty() {
            return;
        }
        // Count working builders per site.
        let mut builders: Vec<(EntityId, usize)> =
            sites.iter().map(|s| (self.world.id_at(*s), 0)).collect();
        for s in self.world.slots() {
            if let Order::Build {
                site,
                working: true,
            } = self.world.order[s.index()]
            {
                if let Some(b) = builders.iter_mut().find(|(id, _)| *id == site) {
                    b.1 += 1;
                }
            }
        }
        for (k, site) in sites.iter().enumerate() {
            let i = site.index();
            let n = builders[k].1.min(MAX_BUILDERS) as u32;
            if n == 0 {
                continue;
            }
            let info = kinds::info(self.world.kind[i]);
            let owner = self.world.owner[i];
            let modifiers = self.modifiers(owner);
            // Progress is in hundredths of a builder-tick, so a percentage
            // build-speed bonus applies without rounding to nothing.
            let total = info.build_work().max(1);
            let pace = (100 + modifiers.build_speed_pct).max(1) as u32;
            let done = (self.world.construction[i].unwrap_or(0) + n * pace).min(total);
            self.world.construction[i] = Some(done);
            self.world.health[i] =
                Fx::from_int((info.max_health as i64 * done as i64 / total as i64).max(1) as i32);
            if done >= total {
                self.world.construction[i] = None;
                self.world.health[i] = Fx::from_int(info.max_health);
                if let Some((_, base)) = info.resource {
                    // A finished farm is seeded for free; only reseeds cost.
                    self.world.resource[i] = modifiers.farm_yield(base);
                }
                let id = self.world.id_at(*site);
                for s in self.world.slots().collect::<Vec<_>>() {
                    if matches!(self.world.order[s.index()], Order::Build { site: b, .. } if b == id)
                    {
                        self.world.order[s.index()] = Order::Idle;
                        self.world.nav[s.index()] = None;
                    }
                }
            }
        }
    }

    fn production(&mut self) {
        let buildings: Vec<Slot> = self
            .world
            .slots()
            .filter(|s| {
                let i = s.index();
                self.world.production[i]
                    .as_ref()
                    .is_some_and(|p| !p.queue.is_empty())
                    && self.world.construction[i].is_none()
            })
            .collect();
        for slot in buildings {
            let i = slot.index();
            let owner = self.world.owner[i];
            let Some(head) = self.world.production[i]
                .as_ref()
                .and_then(|p| p.queue.first().copied())
            else {
                continue;
            };
            let total = match head.item {
                Item::Unit(k) => kinds::info(k).build_ticks(),
                Item::Tech(t) => tech::info(t).map_or(0, |t| t.ticks()),
            }
            .max(1);
            let progress = (head.progress + 1).min(total);
            if let Some(p) = self.world.production[i].as_mut() {
                p.queue[0].progress = progress;
            }
            if progress < total {
                continue;
            }
            let kind = match head.item {
                Item::Unit(k) => k,
                Item::Tech(t) => {
                    if let Some(p) = self.world.production[i].as_mut() {
                        p.queue.remove(0);
                    }
                    self.apply_tech(owner, t);
                    continue;
                }
            };
            let info = kinds::info(kind);
            let player = &self.players[owner as usize];
            if player.pop + info.pop_cost > player.pop_cap {
                continue; // Housed out: wait at the door.
            }
            // Step out onto the nearest open tile beside the building.
            let fp = kinds::info(self.world.kind[i]).footprint as i32;
            let (ax, ay) = nav::anchor_tile(self.world.pos[i], fp);
            let Some(exit) = self.nav.nearest_passable(ax, ay + fp / 2 + 1, 4, None) else {
                continue;
            };
            let rally = self.world.production[i].as_ref().and_then(|p| p.rally);
            let Some(unit) = self.spawn(kind, owner, nav::centre(exit)) else {
                // The entity store is full. Keep the paid, completed item
                // at the head until a slot opens or the player cancels it.
                continue;
            };
            if let Some(p) = self.world.production[i].as_mut() {
                p.queue.remove(0);
            }
            self.players[owner as usize].pop += info.pop_cost;
            self.apply_rally(unit, rally);
        }
    }

    /// A technology completes: record it and fold its effects into the
    /// player's modifiers. An age advance is just another effect.
    fn apply_tech(&mut self, owner: PlayerId, id: TechId) {
        let Some(t) = tech::info(id) else {
            return;
        };
        let Some(p) = self.players.get_mut(owner as usize) else {
            return;
        };
        p.mark_researched(id);
        for effect in t.effects {
            match *effect {
                Effect::GatherRate(r, pct) => p.modifiers.gather_rate_pct[r.index()] += pct,
                Effect::CarryCapacity(n) => p.modifiers.carry_bonus += n,
                Effect::FarmYield(n) => p.modifiers.farm_yield_bonus += n,
                Effect::VillagerSpeed(pct) => p.modifiers.villager_speed_pct += pct,
                Effect::BuildSpeed(pct) => p.modifiers.build_speed_pct += pct,
                Effect::AdvanceAge(age) => p.age = age,
            }
        }
    }

    fn apply_rally(&mut self, unit: EntityId, rally: Option<Rally>) {
        let Some(slot) = self.world.slot(unit) else {
            return;
        };
        let i = slot.index();
        match rally {
            None | Some(Rally::None) => {}
            Some(Rally::Point(p)) => {
                let p = self.clamp_to_map(p);
                self.world.order[i] = Order::Move { target: p };
                self.world.nav[i] = Some(Nav::to(p, Fx::from_ratio(15, 100)));
            }
            Some(Rally::Entity(e)) => {
                let Some(es) = self.world.slot(e) else {
                    return;
                };
                let ek = self.world.kind[es.index()];
                if self.gatherable_by(es.index(), self.world.owner[i]) {
                    let resource = kinds::info(ek)
                        .resource
                        .map(|(r, _)| r)
                        .unwrap_or(Resource::Food);
                    self.world.order[i] = Order::Gather {
                        node: e,
                        resource,
                        phase: GatherPhase::ToNode,
                    };
                } else if self.world.construction[es.index()].is_some()
                    && self.world.owner[es.index()] == self.world.owner[i]
                {
                    self.world.order[i] = Order::Build {
                        site: e,
                        working: false,
                    };
                } else {
                    let p = self.world.pos[es.index()];
                    self.world.order[i] = Order::Move { target: p };
                    self.world.nav[i] = Some(Nav::to(p, Fx::from_ratio(15, 100)));
                }
            }
        }
    }

    fn recount_population(&mut self) {
        for p in &mut self.players {
            p.pop = 0;
            p.pop_cap = 0;
        }
        for s in self.world.slots() {
            let i = s.index();
            let owner = self.world.owner[i] as usize;
            if owner >= self.players.len() {
                continue;
            }
            let info = kinds::info(self.world.kind[i]);
            if info.mobile {
                self.players[owner].pop += info.pop_cost;
            } else if self.world.construction[i].is_none() {
                self.players[owner].pop_cap += info.pop_provided;
            }
        }
        let cap = self.config.pop_cap_max;
        for p in &mut self.players {
            p.pop_cap = p.pop_cap.min(cap);
        }
    }

    // ----- helpers --------------------------------------------------------------

    fn owned_slot(&self, id: EntityId, player: PlayerId) -> Option<Slot> {
        let slot = self.world.slot(id)?;
        (self.world.owner[slot.index()] == player).then_some(slot)
    }

    fn owned_mobile(&self, id: EntityId, player: PlayerId) -> Option<Slot> {
        self.owned_slot(id, player)
            .filter(|s| kinds::info(self.world.kind[s.index()]).mobile)
    }

    fn owned_villager(&self, id: EntityId, player: PlayerId) -> Option<Slot> {
        self.owned_slot(id, player)
            .filter(|s| self.world.kind[s.index()] == kinds::VILLAGER)
    }

    fn clamp_to_map(&self, p: Vec2Fx) -> Vec2Fx {
        let max_x = Fx::from_int(self.map.width()) - Fx::EPSILON;
        let max_y = Fx::from_int(self.map.height()) - Fx::EPSILON;
        Vec2Fx::new(p.x.clamp(Fx::ZERO, max_x), p.y.clamp(Fx::ZERO, max_y))
    }
}

/// A simulation invariant that does not hold. See [`Simulation::check`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Violation {
    /// The entity store itself is inconsistent.
    World(WorldViolation),
    /// More live entities than `max_entities` allows.
    OverEntityCap {
        /// How many are live.
        live: u32,
        /// The configured cap.
        cap: u32,
    },
    /// A live entity is outside the map.
    PositionOutOfMap {
        /// Which slot.
        slot: u32,
        /// Its position.
        pos: Vec2Fx,
    },
    /// A move order points outside the map.
    TargetOutOfMap {
        /// Which slot.
        slot: u32,
        /// The target.
        target: Vec2Fx,
    },
    /// A live entity has negative health.
    NegativeHealth {
        /// Which slot.
        slot: u32,
        /// Its health.
        health: Fx,
    },
    /// A resource node holds a negative amount.
    NegativeResource {
        /// Which slot.
        slot: u32,
        /// The amount.
        amount: i32,
    },
    /// A villager carries a negative amount.
    NegativeCarry {
        /// Which slot.
        slot: u32,
        /// The amount.
        amount: i32,
    },
    /// A facing is outside `0..8`.
    BadFacing {
        /// Which slot.
        slot: u32,
        /// The value.
        facing: u8,
    },
    /// A stockpile went negative, so something was spent that was not there.
    NegativeStockpile {
        /// Which player.
        player: usize,
        /// Which resource index.
        resource: usize,
        /// The amount.
        amount: i32,
    },
    /// A running gathered total went backwards.
    NegativeGathered {
        /// Which player.
        player: usize,
        /// Which resource index.
        resource: usize,
        /// The amount.
        amount: i32,
    },
    /// A player's population headroom exceeds the match limit.
    PopCapAboveLimit {
        /// Which player.
        player: usize,
        /// Their cap.
        pop_cap: u32,
        /// The match limit.
        limit: u32,
    },
}

impl core::fmt::Display for Violation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Violation::World(v) => write!(f, "{v}"),
            Violation::OverEntityCap { live, cap } => {
                write!(f, "{live} live entities exceeds the cap of {cap}")
            }
            Violation::PositionOutOfMap { slot, pos } => {
                write!(f, "slot {slot} is at {pos:?}, outside the map")
            }
            Violation::TargetOutOfMap { slot, target } => {
                write!(f, "slot {slot} is ordered to {target:?}, outside the map")
            }
            Violation::NegativeHealth { slot, health } => {
                write!(f, "slot {slot} has health {health}")
            }
            Violation::NegativeResource { slot, amount } => {
                write!(f, "slot {slot} holds {amount} resource")
            }
            Violation::NegativeCarry { slot, amount } => {
                write!(f, "slot {slot} carries {amount}")
            }
            Violation::BadFacing { slot, facing } => {
                write!(f, "slot {slot} faces {facing}, which is not one of 8")
            }
            Violation::NegativeStockpile {
                player,
                resource,
                amount,
            } => write!(
                f,
                "player {player} has {amount} of resource {resource}: something \
                 was spent that was never gathered"
            ),
            Violation::NegativeGathered {
                player,
                resource,
                amount,
            } => write!(
                f,
                "player {player} has gathered {amount} of resource {resource}"
            ),
            Violation::PopCapAboveLimit {
                player,
                pop_cap,
                limit,
            } => write!(
                f,
                "player {player} has population headroom {pop_cap}, above the \
                 match limit of {limit}"
            ),
        }
    }
}

impl std::error::Error for Violation {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapgen::MapKind;

    fn flat(size: u16, players: u8) -> SimConfig {
        SimConfig {
            map: MapSpec {
                kind: MapKind::Flat,
                size,
                players,
            },
            wander: false,
            ..SimConfig::default()
        }
    }

    fn inland(seed: u64) -> Simulation {
        Simulation::new(seed, SimConfig::default())
    }

    fn spawn_cmd(player: PlayerId, kind: KindId, x: i32, y: i32) -> Command {
        Command {
            player,
            kind: CommandKind::Spawn {
                kind,
                pos: nav::centre((x, y)),
            },
        }
    }

    fn run(sim: &mut Simulation, ticks: u32) {
        for _ in 0..ticks {
            sim.step();
        }
    }

    fn owned(sim: &Simulation, p: PlayerId, kind: KindId) -> Vec<EntityId> {
        sim.world()
            .slots()
            .filter(|s| sim.world().owner[s.index()] == p && sim.world().kind[s.index()] == kind)
            .map(|s| sim.world().id_at(s))
            .collect()
    }

    fn nearest_kind(sim: &Simulation, kind: KindId, from: Vec2Fx) -> EntityId {
        sim.world()
            .slots()
            .filter(|s| sim.world().kind[s.index()] == kind)
            .min_by_key(|s| (from.distance_sq_raw(sim.world().pos[s.index()]), s.index()))
            .map(|s| sim.world().id_at(s))
            .expect("kind present")
    }

    #[test]
    fn commands_take_effect_after_the_delay() {
        let mut sim = Simulation::new(1, flat(64, 1));
        sim.issue(spawn_cmd(0, kinds::VILLAGER, 5, 5));
        sim.step();
        assert_eq!(sim.world().len(), 0);
        sim.step();
        assert_eq!(sim.world().len(), 0);
        sim.step();
        assert_eq!(sim.world().len(), 1, "executes at the start of tick 2");
    }

    #[test]
    fn move_walks_there_and_stops() {
        let mut sim = Simulation::new(1, flat(64, 1));
        sim.issue(spawn_cmd(0, kinds::VILLAGER, 2, 2));
        run(&mut sim, 3);
        let id = owned(&sim, 0, kinds::VILLAGER)[0];
        let target = Vec2Fx::from_int(12, 2);
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Move {
                ids: vec![id],
                target,
            },
        });
        // Ten tiles at 0.9 tiles/s is ~11 s = 222 ticks plus the delay.
        run(&mut sim, 240);
        let s = sim.world().slot(id).unwrap();
        assert!(
            sim.world().pos[s.index()].distance(target) <= Fx::from_ratio(2, 10),
            "{:?}",
            sim.world().pos[s.index()]
        );
        assert_eq!(sim.world().order[s.index()], Order::Idle);
        assert!(sim.world().nav[s.index()].is_none());
        // Another player cannot command it.
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Move {
                ids: vec![id],
                target: Vec2Fx::from_int(30, 30),
            },
        });
        let mut other = sim.clone();
        other.issue(Command {
            player: 1,
            kind: CommandKind::Stop { ids: vec![id] },
        });
        run(&mut sim, 5);
        run(&mut other, 5);
        assert_ne!(sim.world().order[s.index()], Order::Idle);
    }

    #[test]
    fn path_detours_around_a_wall_of_trees() {
        let mut sim = Simulation::new(1, flat(40, 1));
        // A wall across x = 20 with a gap at y = 35.
        for y in 0..35 {
            sim.issue(spawn_cmd(0, kinds::TREE, 20, y));
        }
        sim.issue(spawn_cmd(0, kinds::VILLAGER, 5, 5));
        run(&mut sim, 3);
        let id = owned(&sim, 0, kinds::VILLAGER)[0];
        let target = nav::centre((35, 5));
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Move {
                ids: vec![id],
                target,
            },
        });
        // Detour is roughly 30 + 15 + 30 tiles ≈ 75 tiles ≈ 84 s.
        run(&mut sim, 20 * 100);
        let s = sim.world().slot(id).unwrap();
        assert!(
            sim.world().pos[s.index()].distance(target) < Fx::ONE,
            "{:?}",
            sim.world().pos[s.index()]
        );
        assert_eq!(sim.stats().path_failures, 0);
    }

    // REQ: TA-PATH-03
    #[test]
    fn unreachable_goal_goes_to_nearest_reachable_tile() {
        let mut sim = Simulation::new(1, flat(30, 1));
        // Seal off a 3x3 pocket at (20..=22, 10..=12) with trees.
        for y in 9..=13 {
            for x in 19..=23 {
                if x == 19 || x == 23 || y == 9 || y == 13 {
                    sim.issue(spawn_cmd(0, kinds::TREE, x, y));
                }
            }
        }
        sim.issue(spawn_cmd(0, kinds::VILLAGER, 5, 11));
        run(&mut sim, 3);
        let id = owned(&sim, 0, kinds::VILLAGER)[0];
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Move {
                ids: vec![id],
                target: nav::centre((21, 11)),
            },
        });
        run(&mut sim, 20 * 30);
        let s = sim.world().slot(id).unwrap();
        let p = sim.world().pos[s.index()];
        assert_eq!(sim.world().order[s.index()], Order::Idle, "settled");
        let (tx, ty) = nav::tile_of(p);
        assert!(
            !((20..=22).contains(&tx) && (10..=12).contains(&ty)),
            "stopped outside the pocket: {p:?}"
        );
        assert!(
            p.distance(nav::centre((21, 11))) < Fx::from_int(5),
            "but close to it: {p:?}"
        );
    }

    // REQ: RM-M2-02
    #[test]
    fn sixty_villagers_cross_the_map_without_getting_stuck() {
        let mut sim = inland(4);
        let (sx, sy) = sim.starts()[0];
        for k in 0..60 {
            sim.issue(spawn_cmd(
                0,
                kinds::VILLAGER,
                sx - 4 + k % 8,
                sy + 3 + k / 8,
            ));
        }
        run(&mut sim, 3);
        let ids = owned(&sim, 0, kinds::VILLAGER);
        assert!(ids.len() >= 60);
        let (ox, oy) = sim.starts()[1];
        let target = nav::centre((ox, oy + 4));
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Move {
                ids: ids.clone(),
                target,
            },
        });
        // Across a 128-tile map at 0.9 tiles/s, with detours: give it 3 minutes.
        run(&mut sim, 20 * 180);
        let mut far = 0;
        for id in &ids {
            let s = sim.world().slot(*id).unwrap();
            let i = s.index();
            assert!(
                sim.world().nav[i].is_none() && sim.world().order[i] == Order::Idle,
                "villager still busy"
            );
            if sim.world().pos[i].distance(target) > Fx::from_int(8) {
                far += 1;
            }
        }
        assert_eq!(far, 0, "{far} villagers ended far from the target");
        // No two units share a spot.
        let ps: Vec<_> = ids
            .iter()
            .map(|id| sim.world().pos[sim.world().slot(*id).unwrap().index()])
            .collect();
        for a in 0..ps.len() {
            for b in a + 1..ps.len() {
                assert!(
                    ps[a].distance(ps[b]) > Fx::from_ratio(1, 10),
                    "units stacked"
                );
            }
        }
    }

    // REQ: RM-M2-01
    #[test]
    fn gathers_all_four_resources_and_delivers_them() {
        let mut sim = inland(2);
        let (sx, sy) = sim.starts()[0];
        let tc = nav::centre((sx, sy));
        for k in 0..12 {
            sim.issue(spawn_cmd(
                0,
                kinds::VILLAGER,
                sx - 3 + k % 6,
                sy + 3 + k / 6,
            ));
        }
        run(&mut sim, 3);
        let vill = owned(&sim, 0, kinds::VILLAGER);
        assert!(vill.len() >= 12);
        let nodes = [
            nearest_kind(&sim, kinds::BERRY_BUSH, tc),
            nearest_kind(&sim, kinds::TREE, tc),
            nearest_kind(&sim, kinds::STONE_MINE, tc),
            nearest_kind(&sim, kinds::GOLD_MINE, tc),
        ];
        for (k, node) in nodes.iter().enumerate() {
            sim.issue(Command {
                player: 0,
                kind: CommandKind::Gather {
                    ids: vill[k * 3..k * 3 + 3].to_vec(),
                    node: *node,
                },
            });
        }
        let before = sim.player(0).unwrap().stockpile;
        run(&mut sim, 20 * 150);
        let after = sim.player(0).unwrap().stockpile;
        for r in Resource::ALL {
            assert!(
                after[r.index()] > before[r.index()] + 20,
                "{}: {} -> {}",
                r.name(),
                before[r.index()],
                after[r.index()]
            );
        }
        assert_eq!(sim.stats().path_failures, 0);
        // Everyone is still working, and nobody is carrying more than a load.
        for id in &vill[..12] {
            let i = sim.world().slot(*id).unwrap().index();
            assert!(
                matches!(sim.world().order[i], Order::Gather { .. }),
                "{:?}",
                sim.world().order[i]
            );
            assert!(sim.world().carry[i].map_or(0, |(_, a)| a) <= kinds::CARRY_CAPACITY);
        }
    }

    #[test]
    fn a_node_exhausted_while_others_gather_elsewhere() {
        // Villagers split across *different* nodes. When one node runs out
        // it is removed, which unblocks its tile and dirties the nav grid —
        // and the villagers still walking to their own nodes then query
        // connectivity inside the same `orders()` pass.
        //
        // `exhausted_node_is_removed_and_villagers_move_to_the_next` cannot
        // reach this: its three villagers share one node, so when it goes
        // they all lose it together and none is left mid-approach.
        let mut sim = inland(3);
        let (sx, sy) = sim.starts()[0];
        let tc = nav::centre((sx, sy));
        let vill = owned(&sim, 0, kinds::VILLAGER);
        assert!(vill.len() >= 3, "need villagers to split up");

        // One villager on the bush (150 food, exhausts soonest), the rest on
        // wood and stone, so somebody is always mid-approach.
        let bush = nearest_kind(&sim, kinds::BERRY_BUSH, tc);
        let tree = nearest_kind(&sim, kinds::TREE, tc);
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Gather {
                ids: vill[..1].to_vec(),
                node: bush,
            },
        });
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Gather {
                ids: vill[1..].to_vec(),
                node: tree,
            },
        });
        // Long enough for the wood node (75) to run out under several
        // villagers while the food gatherer is still walking back and forth.
        run(&mut sim, 20 * 400);
        sim.check().expect("invariants must hold throughout");
        assert!(
            sim.player(0).unwrap().gathered.iter().sum::<i32>() > 0,
            "the scenario must actually gather something"
        );
    }

    // REQ: GD-ECON-01
    #[test]
    fn exhausted_node_is_removed_and_villagers_move_to_the_next() {
        let mut sim = inland(3);
        let (sx, sy) = sim.starts()[0];
        let tc = nav::centre((sx, sy));
        let bush = nearest_kind(&sim, kinds::BERRY_BUSH, tc);
        let vill = owned(&sim, 0, kinds::VILLAGER);
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Gather {
                ids: vill.clone(),
                node: bush,
            },
        });
        // 150 food at 3 × 0.4/s plus walking: well under 4 minutes.
        run(&mut sim, 20 * 240);
        assert!(
            !sim.world().contains(bush),
            "bush should be exhausted and gone"
        );
        let still = vill
            .iter()
            .filter(|id| {
                matches!(
                    sim.world().order[sim.world().slot(**id).unwrap().index()],
                    Order::Gather { .. }
                )
            })
            .count();
        assert_eq!(
            still, 3,
            "villagers should have moved to a neighbouring bush"
        );
        assert!(sim.player(0).unwrap().gathered[Resource::Food.index()] >= 150);
    }

    #[test]
    fn house_and_storehouse_are_built_and_used() {
        let mut sim = inland(5);
        let (sx, sy) = sim.starts()[0];
        let vill = owned(&sim, 0, kinds::VILLAGER);
        let wood_before = sim.player(0).unwrap().stockpile[1];
        assert_eq!(sim.player(0).unwrap().pop_cap, 5);
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Build {
                kind: kinds::HOUSE,
                x: sx + 4,
                y: sy,
                ids: vill[..2].to_vec(),
            },
        });
        run(&mut sim, 3);
        assert_eq!(
            sim.player(0).unwrap().stockpile[1],
            wood_before - 30,
            "paid on placement"
        );
        let house = owned(&sim, 0, kinds::HOUSE)[0];
        let hi = sim.world().slot(house).unwrap().index();
        assert!(sim.world().construction[hi].is_some());
        assert!(
            !sim.nav().passable(sx + 4, sy),
            "site blocks its footprint immediately"
        );
        // Two builders: 25 s of work at 2/tick ≈ 12.5 s plus the walk.
        run(&mut sim, 20 * 30);
        let hi = sim.world().slot(house).unwrap().index();
        assert!(
            sim.world().construction[hi].is_none(),
            "house should be complete"
        );
        assert_eq!(sim.world().health[hi], Fx::from_int(75));
        assert_eq!(sim.player(0).unwrap().pop_cap, 10);
        for id in &vill[..2] {
            assert_eq!(
                sim.world().order[sim.world().slot(*id).unwrap().index()],
                Order::Idle
            );
        }

        // A storehouse by the forest becomes the drop-off for wood.
        let tree = nearest_kind(&sim, kinds::TREE, nav::centre((sx, sy)));
        let tp = sim.world().pos[sim.world().slot(tree).unwrap().index()];
        let (tx, ty) = nav::tile_of(tp);
        // Find a clear 2x2 spot near the tree.
        let mut spot = None;
        'outer: for r in 2..8 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if sim
                        .can_place(0, kinds::STOREHOUSE, tx + dx, ty + dy)
                        .is_ok()
                    {
                        spot = Some((tx + dx, ty + dy));
                        break 'outer;
                    }
                }
            }
        }
        let (bx, by) = spot.expect("somewhere to build");
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Build {
                kind: kinds::STOREHOUSE,
                x: bx,
                y: by,
                ids: vill.clone(),
            },
        });
        run(&mut sim, 20 * 45);
        let store = owned(&sim, 0, kinds::STOREHOUSE)[0];
        let si = sim.world().slot(store).unwrap().index();
        assert!(
            sim.world().construction[si].is_none(),
            "storehouse should be complete"
        );
        // The storehouse may have gone up against the original tree; gather
        // the one nearest the storehouse and let replacement logic do the rest.
        let tree = nearest_kind(&sim, kinds::TREE, sim.world().pos[si]);
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Gather {
                ids: vill.clone(),
                node: tree,
            },
        });
        run(&mut sim, 20 * 60);
        let wood_gain = sim.player(0).unwrap().gathered[1];
        assert!(wood_gain > 30, "wood gathered: {wood_gain}");
        // From beside the tree, the storehouse is the nearest drop-off.
        let ti = sim.world().slot(tree).map(|s| s.index());
        let near = vill
            .iter()
            .map(|id| sim.world().slot(*id).unwrap().index())
            .min_by_key(|&i| {
                ti.map_or(0, |t| {
                    sim.world().pos[i].distance_sq_raw(sim.world().pos[t])
                })
            })
            .unwrap();
        assert_eq!(
            sim.nearest_dropoff(near, 0),
            Some(store),
            "villagers should deliver to the storehouse"
        );
    }

    #[test]
    fn placement_is_validated() {
        let mut sim = inland(6);
        let (sx, sy) = sim.starts()[0];
        assert_eq!(
            sim.can_place(0, kinds::HOUSE, sx, sy),
            Err(PlaceError::Blocked),
            "on the town center"
        );
        assert_eq!(
            sim.can_place(0, kinds::TREE, sx + 4, sy),
            Err(PlaceError::NotBuildable)
        );
        assert_eq!(
            sim.can_place(0, kinds::HOUSE, -5, -5),
            Err(PlaceError::Blocked)
        );
        assert!(sim.can_place(0, kinds::HOUSE, sx + 4, sy).is_ok());
        let count = sim.world().len();
        // Eight clear spots inside the start zone; wood for six.
        let spots: Vec<(i32, i32)> = [sy + 3, sy - 3]
            .iter()
            .flat_map(|&y| {
                [sx - 5, sx - 2, sx + 1, sx + 4]
                    .into_iter()
                    .map(move |x| (x, y))
            })
            .collect();
        for &(x, y) in &spots {
            assert!(
                sim.can_place(0, kinds::HOUSE, x, y).is_ok(),
                "spot {x},{y} should be clear"
            );
            sim.issue(Command {
                player: 0,
                kind: CommandKind::Build {
                    kind: kinds::HOUSE,
                    x,
                    y,
                    ids: vec![],
                },
            });
        }
        run(&mut sim, 3);
        let houses = owned(&sim, 0, kinds::HOUSE).len();
        assert_eq!(houses, 6, "200 wood buys six houses");
        assert_eq!(sim.world().len(), count + 6);
        assert_eq!(
            sim.can_place(0, kinds::HOUSE, spots[7].0, spots[7].1),
            Err(PlaceError::Unaffordable)
        );
        // Cancelling a site refunds it.
        let h = owned(&sim, 0, kinds::HOUSE)[0];
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Despawn { id: h },
        });
        run(&mut sim, 3);
        assert_eq!(sim.player(0).unwrap().stockpile[1], 30 + 200 - 180);
    }

    #[test]
    fn training_with_a_rally_point_and_the_population_cap() {
        let mut sim = Simulation::new(
            7,
            SimConfig {
                pop_cap_max: 6,
                ..SimConfig::default()
            },
        );
        let (sx, sy) = sim.starts()[0];
        let tc = owned(&sim, 0, kinds::TOWN_CENTER)[0];
        let tree = nearest_kind(&sim, kinds::TREE, nav::centre((sx, sy)));
        sim.issue(Command {
            player: 0,
            kind: CommandKind::SetRally {
                building: tc,
                rally: Rally::Entity(tree),
            },
        });
        let food = sim.player(0).unwrap().stockpile[0];
        for _ in 0..3 {
            sim.issue(Command {
                player: 0,
                kind: CommandKind::Train {
                    building: tc,
                    kind: kinds::VILLAGER,
                },
            });
        }
        run(&mut sim, 3);
        assert_eq!(
            sim.player(0).unwrap().stockpile[0],
            food - 150,
            "three villagers paid for"
        );
        assert_eq!(sim.player(0).unwrap().pop, 4, "3 villagers + scout");
        assert_eq!(sim.player(0).unwrap().pop_cap, 5);
        run(&mut sim, 20 * 26);
        assert_eq!(sim.player(0).unwrap().pop, 5, "one trained");
        let newest = *owned(&sim, 0, kinds::VILLAGER).last().unwrap();
        let ni = sim.world().slot(newest).unwrap().index();
        assert!(
            matches!(sim.world().order[ni], Order::Gather { node, .. } if node == tree),
            "rally sends it to the tree"
        );
        run(&mut sim, 20 * 30);
        assert_eq!(sim.player(0).unwrap().pop, 5, "housed: second waits");
        // Build a house: cap rises to the match limit of 6 and one more comes out.
        let vill = owned(&sim, 0, kinds::VILLAGER);
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Build {
                kind: kinds::HOUSE,
                x: sx + 4,
                y: sy,
                ids: vill[..2].to_vec(),
            },
        });
        run(&mut sim, 20 * 40);
        assert_eq!(sim.player(0).unwrap().pop_cap, 6);
        assert_eq!(sim.player(0).unwrap().pop, 6);
        let q = sim.world().production[sim.world().slot(tc).unwrap().index()]
            .as_ref()
            .unwrap();
        assert_eq!(q.queue.len(), 1, "third still waiting");
        sim.issue(Command {
            player: 0,
            kind: CommandKind::CancelTrain { building: tc },
        });
        run(&mut sim, 3);
        assert_eq!(sim.player(0).unwrap().stockpile[0], food - 150 + 50);
    }

    #[test]
    fn identical_runs_hash_identically_and_diverge_on_seed() {
        let run_seed = |seed: u64| {
            let mut sim = inland(seed);
            let vill = owned(&sim, 0, kinds::VILLAGER);
            let (sx, sy) = sim.starts()[0];
            let tree = nearest_kind(&sim, kinds::TREE, nav::centre((sx, sy)));
            sim.issue(Command {
                player: 0,
                kind: CommandKind::Gather {
                    ids: vill,
                    node: tree,
                },
            });
            let mut hashes = Vec::new();
            for _ in 0..400 {
                sim.step();
                hashes.push(sim.state_hash());
            }
            hashes
        };
        assert_eq!(run_seed(99), run_seed(99));
        assert_ne!(run_seed(99), run_seed(100));
    }

    #[test]
    fn map_hash_is_part_of_state() {
        let a = Simulation::new(1, SimConfig::default());
        let b = Simulation::new(
            1,
            SimConfig {
                map: MapSpec {
                    size: 96,
                    ..MapSpec::default()
                },
                ..SimConfig::default()
            },
        );
        assert_ne!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn animals_wander_and_scenery_stays_put() {
        let mut sim = inland(11);
        let snapshot = |sim: &Simulation, kind: KindId| -> Vec<Vec2Fx> {
            sim.world()
                .slots()
                .filter(|s| sim.world().kind[s.index()] == kind)
                .map(|s| sim.world().pos[s.index()])
                .collect()
        };
        let trees = snapshot(&sim, kinds::TREE);
        let gazelles = snapshot(&sim, kinds::GAZELLE);
        run(&mut sim, 300);
        assert_eq!(snapshot(&sim, kinds::TREE), trees);
        assert_ne!(snapshot(&sim, kinds::GAZELLE), gazelles);
        assert!(sim.rng_draws() > 0);
    }
}
