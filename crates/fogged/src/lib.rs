//! What a player knows of a match, and nothing more.
//!
//! [`FoggedView`] wraps a simulation for one player and answers only with
//! what that player has seen or can see (`docs/02` §9 `GD-FOG-01`) and
//! with the player's own state. It is the whole interface the `ai` crate
//! is given (`docs/04` §6 `TA-AI-01`, `docs/07` D7): that crate depends on
//! this one and not on `sim`, so it cannot name the world even by
//! accident. The types a computer opponent needs to read a view and write
//! a command are re-exported here; the simulation itself is not.

pub use sim::kinds;
pub use sim::nav;
pub use sim::tech;
pub use sim::{
    Age, Class, Command, CommandKind, EntityId, Formation, Fx, Item, KindId, Memory, Modifiers,
    PlaceError, Player, PlayerId, Rally, ResearchError, Rng, Stance, Terrain, TrainError, Vec2Fx,
    Visibility,
};

use sim::Simulation;

/// A match as one player sees it.
pub struct FoggedView<'a> {
    sim: &'a Simulation,
    player: PlayerId,
}

/// What one of the player's own units is doing. Nothing is known of
/// anyone else's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Job {
    /// Standing idle.
    Idle,
    /// Gathering a resource.
    Gathering(kinds::Resource),
    /// Constructing a site: walking to it or working on it.
    Building(EntityId),
    /// Repairing one of the side's buildings: walking to it or working on
    /// it (`GD-BUILD-02`).
    Repairing(EntityId),
    /// Hunting an animal, to gather its carcass after (`GD-ECON-06`).
    Hunting(EntityId),
    /// Anything else: walking, fighting, sheltering.
    Busy,
    /// Someone else's: not known.
    Unknown,
}

/// Something that happened to the player's own side this tick, as the
/// bell and the minimap tell a player. Nothing of anyone else's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// One of the player's things was hit and the side had not been told
    /// lately.
    Alarm {
        /// Where.
        pos: Vec2Fx,
    },
    /// One of the player's things died or fell.
    Loss {
        /// What.
        kind: KindId,
        /// Where.
        pos: Vec2Fx,
    },
}

/// A unit or building in sight, or one of the player's own anywhere.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sighting {
    /// Its handle, for commands.
    pub id: EntityId,
    /// What it is.
    pub kind: KindId,
    /// Whose it is.
    pub owner: PlayerId,
    /// Where it is, in tiles.
    pub pos: Vec2Fx,
    /// Hit points left.
    pub health: Fx,
    /// Still under construction.
    pub site: bool,
    /// What it is doing; [`Job::Unknown`] for anyone else's.
    pub job: Job,
    /// Sheltering inside a building. Only ever true for the player's own.
    pub inside: bool,
    /// For a node or a farm: what it yields and how much is left, as a
    /// player sees by clicking on it.
    pub resource: Option<(kinds::Resource, i32)>,
    /// A hunted animal lying where it fell, its food still on it: a node
    /// for as long as it lasts (`GD-ECON-06`).
    pub carcass: bool,
}

/// A building or node seen once and now out of sight, as it was.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Remembered {
    /// Its handle when it was seen, for orders; ignored by the simulation
    /// if it is gone.
    pub id: EntityId,
    /// The anchor tile.
    pub tile: (i32, i32),
    /// What stood there.
    pub kind: KindId,
    /// Whose it was.
    pub owner: PlayerId,
    /// The owner's age when it was last seen (by [`Age::index`]).
    pub age: u8,
    /// Still under construction when last seen.
    pub site: bool,
}

impl<'a> FoggedView<'a> {
    /// `player`'s view of `sim`.
    pub fn new(sim: &'a Simulation, player: PlayerId) -> FoggedView<'a> {
        FoggedView { sim, player }
    }

    /// Whose view this is.
    pub fn player(&self) -> PlayerId {
        self.player
    }

    /// The current tick.
    pub fn tick(&self) -> u64 {
        self.sim.tick()
    }

    /// The match seed, so an opponent can seed its own dice and stay
    /// deterministic.
    pub fn seed(&self) -> u64 {
        self.sim.seed()
    }

    /// Map size in tiles.
    pub fn size(&self) -> (i32, i32) {
        (self.sim.map().width(), self.sim.map().height())
    }

    fn fog(&self) -> Option<&'a sim::Fog> {
        self.sim.fog(self.player)
    }

    /// What the player knows of a tile.
    pub fn visibility(&self, x: i32, y: i32) -> Visibility {
        self.fog().map_or(Visibility::Unexplored, |f| f.state(x, y))
    }

    /// True if the tile is in sight now.
    pub fn visible(&self, x: i32, y: i32) -> bool {
        self.visibility(x, y) == Visibility::Visible
    }

    /// True if the tile has been seen at least once.
    pub fn explored(&self, x: i32, y: i32) -> bool {
        self.visibility(x, y) != Visibility::Unexplored
    }

    /// The terrain of an explored tile; nothing is known of the rest.
    pub fn terrain(&self, x: i32, y: i32) -> Option<Terrain> {
        self.explored(x, y).then(|| self.sim.map().terrain(x, y))
    }

    /// The elevation of an explored tile.
    pub fn elevation(&self, x: i32, y: i32) -> Option<u8> {
        self.explored(x, y).then(|| self.sim.map().elevation(x, y))
    }

    /// Whether an explored tile can be walked on, as last seen: a building
    /// that has gone up since is not known until the tile is seen again.
    pub fn passable(&self, x: i32, y: i32) -> Option<bool> {
        match self.visibility(x, y) {
            Visibility::Unexplored => None,
            Visibility::Visible => Some(self.sim.nav().passable(x, y)),
            Visibility::Explored => Some(
                self.sim.map().walkable(x, y)
                    && self.fog().and_then(|f| f.remembered(x, y)).is_none(),
            ),
        }
    }

    /// The player's own state: stockpile, population, age, technologies.
    pub fn me(&self) -> Option<&'a Player> {
        self.sim.player(self.player)
    }

    /// The player's technology modifiers.
    pub fn modifiers(&self) -> Modifiers {
        self.sim.modifiers(self.player)
    }

    /// Everything the player can see: their own units and buildings
    /// wherever they are, and anyone else's on a tile in sight. Corpses and
    /// rubble are not listed; a carcass with food on it is, as the player
    /// sees it lying there.
    pub fn sightings(&self) -> Vec<Sighting> {
        let world = self.sim.world();
        let Some(fog) = self.fog() else {
            return Vec::new();
        };
        world
            .slots()
            .filter_map(|s| {
                let i = s.index();
                let kind = world.kind[i];
                let carcass = world.dying[i] > 0 && kinds::huntable(kind) && world.resource[i] > 0;
                if world.dying[i] > 0 && !carcass {
                    return None;
                }
                let mine = world.owner[i] == self.player;
                let fp = kinds::info(kind).footprint as i32;
                let (x, y) = sim::nav::anchor_tile(world.pos[i], fp);
                if !mine && !fog.in_sight(kind, x, y) {
                    return None;
                }
                let job = if !mine {
                    Job::Unknown
                } else {
                    match world.order[i] {
                        sim::Order::Idle => Job::Idle,
                        sim::Order::Gather { resource, .. } => Job::Gathering(resource),
                        sim::Order::Build { site, .. } => Job::Building(site),
                        sim::Order::Repair { building, .. } => Job::Repairing(building),
                        sim::Order::Attack {
                            target,
                            then: sim::Then::Hunt(_),
                            ..
                        } => Job::Hunting(target),
                        _ => Job::Busy,
                    }
                };
                Some(Sighting {
                    id: world.id_at(s),
                    kind: world.kind[i],
                    owner: world.owner[i],
                    pos: world.pos[i],
                    health: world.health[i],
                    site: world.construction[i].is_some(),
                    job,
                    inside: mine && world.inside[i].is_some(),
                    resource: kinds::info(kind)
                        .resource
                        .map(|(r, _)| (r, world.resource[i])),
                    carcass,
                })
            })
            .collect()
    }

    /// What repairing one of the player's own buildings would cost now
    /// (`GD-BUILD-02`): half its cost in proportion to the damage, which is
    /// what the repair is charged when it starts. Nothing for anything not
    /// the player's, not finished, or not damaged.
    pub fn repair_cost(&self, id: EntityId) -> Option<kinds::Cost> {
        let world = self.sim.world();
        let i = world.slot(id)?.index();
        if world.owner[i] != self.player || !self.sim.repairable(i) {
            return None;
        }
        let info = kinds::info(world.kind[i]);
        Some(sim::repair_due(
            &info.cost,
            Fx::from_int(info.max_health) - world.health[i],
            info.max_health,
        ))
    }

    /// Buildings and nodes seen once and now out of sight, as they were.
    pub fn remembered(&self) -> Vec<Remembered> {
        self.fog().map_or_else(Vec::new, |f| {
            f.memories()
                .map(|(tile, m)| Remembered {
                    id: m.id,
                    tile,
                    kind: m.kind,
                    owner: m.owner,
                    age: m.age,
                    site: m.site,
                })
                .collect()
        })
    }

    /// The player's villagers with nothing to do.
    pub fn idle_villagers(&self) -> Vec<EntityId> {
        self.sim.idle_villagers(self.player)
    }

    /// What happened to the player's own side during the last tick.
    pub fn events(&self) -> Vec<Event> {
        self.sim
            .events()
            .iter()
            .filter_map(|e| match *e {
                sim::Event::Alarm { player, pos } if player == self.player => {
                    Some(Event::Alarm { pos })
                }
                sim::Event::Death { kind, owner, pos } if owner == self.player => {
                    Some(Event::Loss { kind, pos })
                }
                _ => None,
            })
            .collect()
    }

    /// What one of the player's own buildings has queued, head first;
    /// empty for anyone else's or for a building that trains nothing.
    pub fn queue(&self, building: EntityId) -> Vec<Item> {
        let world = self.sim.world();
        let Some(slot) = world.slot(building) else {
            return Vec::new();
        };
        let i = slot.index();
        if world.owner[i] != self.player {
            return Vec::new();
        }
        world.production[i]
            .as_ref()
            .map_or_else(Vec::new, |p| p.queue.iter().map(|q| q.item).collect())
    }

    /// The match's population limit, which houses cannot raise the cap
    /// past.
    pub fn pop_cap_max(&self) -> u32 {
        self.sim.config().pop_cap_max
    }

    /// Whether the player could build `kind` at all right now.
    pub fn can_build(&self, kind: KindId) -> Result<(), PlaceError> {
        self.sim.can_build(self.player, kind)
    }

    /// Whether the player could place `kind` anchored at a tile they have
    /// explored. Ground never seen answers as blocked: nothing is known
    /// of it.
    pub fn can_place(&self, kind: KindId, x: i32, y: i32) -> Result<(), PlaceError> {
        if !self.explored(x, y) {
            return Err(PlaceError::Blocked);
        }
        self.sim.can_place(self.player, kind, x, y)
    }

    /// Whether the player may queue `kind` at one of their buildings.
    pub fn can_train(&self, building: EntityId, kind: KindId) -> Result<(), TrainError> {
        self.sim.can_train(self.player, building, kind)
    }

    /// Whether the player may queue a technology at one of their buildings.
    pub fn can_research(
        &self,
        building: EntityId,
        tech: tech::TechId,
    ) -> Result<(), ResearchError> {
        self.sim.can_research(self.player, building, tech)
    }

    /// The units a building of `kind` offers the player.
    pub fn roster(&self, kind: KindId) -> Vec<KindId> {
        self.sim.roster(self.player, kind)
    }

    /// A command from this player, ready to issue.
    pub fn command(&self, kind: CommandKind) -> Command {
        Command {
            player: self.player,
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{MapKind, MapSpec, SimConfig};

    fn arena() -> Simulation {
        Simulation::new(
            3,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 64,
                    players: 2,
                },
                wander: false,
                ..SimConfig::default()
            },
        )
    }

    fn spawn(sim: &mut Simulation, player: PlayerId, kind: KindId, x: i32, y: i32) {
        sim.issue(Command {
            player,
            kind: CommandKind::Spawn {
                kind,
                pos: sim::nav::centre((x, y)),
            },
        });
    }

    /// REQ: TA-AI-01
    #[test]
    fn a_view_shows_what_is_in_sight_and_remembers_what_was() {
        let mut sim = arena();
        spawn(&mut sim, 0, kinds::CLUBMAN, 10, 10);
        spawn(&mut sim, 1, kinds::CLUBMAN, 40, 40);
        spawn(&mut sim, 1, kinds::HOUSE, 12, 12);
        for _ in 0..3 {
            sim.step();
        }
        let view = FoggedView::new(&sim, 0);
        assert_eq!(view.player(), 0);
        assert_eq!(view.size(), (64, 64));
        assert_eq!(view.visibility(10, 10), Visibility::Visible);
        assert_eq!(view.visibility(40, 40), Visibility::Unexplored);
        assert!(view.terrain(40, 40).is_none() && view.terrain(10, 10).is_some());
        let seen = view.sightings();
        assert_eq!(seen.len(), 2, "my clubman and their house: {seen:?}");
        assert!(seen
            .iter()
            .any(|s| s.owner == 0 && s.job == Job::Idle && !s.inside));
        assert!(seen.iter().any(|s| s.owner == 1 && s.job == Job::Unknown));
        assert!(seen.iter().any(|s| s.owner == 1 && s.kind == kinds::HOUSE));
        assert!(
            !seen
                .iter()
                .any(|s| s.kind == kinds::CLUBMAN && s.owner == 1),
            "their clubman is out of sight"
        );
        assert!(view.remembered().is_empty(), "in sight is not remembered");
        assert_eq!(
            view.can_place(kinds::HOUSE, 40, 40),
            Err(PlaceError::Blocked)
        );
        assert!(view.can_place(kinds::HOUSE, 8, 8).is_ok());
        assert!(view.me().is_some() && view.modifiers() == sim.modifiers(0));

        // My clubman walks away: the house stays remembered where it was.
        let mine = seen.iter().find(|s| s.owner == 0).unwrap().id;
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Move {
                ids: vec![mine],
                target: sim::nav::centre((30, 10)),
            },
        });
        for _ in 0..300 {
            sim.step();
        }
        let view = FoggedView::new(&sim, 0);
        assert_eq!(view.visibility(12, 12), Visibility::Explored);
        let remembered = view.remembered();
        let house = remembered
            .iter()
            .find(|r| r.kind == kinds::HOUSE && r.owner == 1)
            .unwrap_or_else(|| panic!("{remembered:?}"));
        assert_eq!(
            house.id,
            sim.world().id_at(
                sim.world()
                    .slots()
                    .find(|s| sim.world().kind[s.index()] == kinds::HOUSE)
                    .unwrap()
            )
        );
        assert!(view.queue(house.id).is_empty(), "not mine: nothing known");
        assert_eq!(
            view.passable(house.tile.0, house.tile.1),
            Some(false),
            "as last seen"
        );
        assert!(
            !view.sightings().iter().any(|s| s.kind == kinds::HOUSE),
            "out of sight, out of the list"
        );
        // Theirs sees nothing of mine.
        let theirs = FoggedView::new(&sim, 1);
        assert!(theirs.sightings().iter().all(|s| s.owner == 1));
        assert_eq!(theirs.visibility(30, 10), Visibility::Unexplored);
        assert_eq!(theirs.command(CommandKind::Stop { ids: vec![] }).player, 1);
    }
}
