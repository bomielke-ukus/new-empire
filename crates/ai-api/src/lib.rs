//! Data-only boundary shared by the host and AI. No simulation dependency,
//! callbacks, downcasts, world references, or command cheats cross this API.
#![forbid(unsafe_code)]

/// Generational handle, preserved when translated to a normal player command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityKey {
    pub index: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tile {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terrain {
    pub kind: u8,
    pub elevation: u8,
    pub walkable: bool,
}

/// Only last-seen identity is remembered; never a live reference to a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Building {
    pub id: EntityKey,
    pub owner: u8,
    pub kind: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileKnowledge {
    pub visible: bool,
    /// None until explored. Static terrain stays known after vision leaves.
    pub terrain: Option<Terrain>,
    /// Last observed building on this tile; may no longer exist under fog.
    pub building: Option<Building>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entity {
    pub id: EntityKey,
    pub owner: u8,
    pub kind: u16,
    pub tile: Tile,
    /// Present only for an owned unit. Enemy orders never cross the boundary.
    pub idle: Option<bool>,
    pub scout: bool,
}

/// Host-owned, sanitised storage. These are public data fields so a host can
/// populate them, but contain no way to recover the underlying simulation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub player: u8,
    pub tick: u64,
    pub width: u16,
    pub height: u16,
    pub tiles: Vec<TileKnowledge>,
    pub entities: Vec<Entity>,
}

impl Observation {
    pub fn view(&self) -> FoggedView<'_> {
        FoggedView(self)
    }
}

/// The only input accepted by the AI decision entry point.
#[derive(Clone, Copy)]
pub struct FoggedView<'a>(&'a Observation);

impl<'a> FoggedView<'a> {
    pub fn player(self) -> u8 {
        self.0.player
    }
    pub fn tick(self) -> u64 {
        self.0.tick
    }
    pub fn size(self) -> (u16, u16) {
        (self.0.width, self.0.height)
    }
    pub fn entities(self) -> &'a [Entity] {
        &self.0.entities
    }
    pub fn tile(self, at: Tile) -> Option<&'a TileKnowledge> {
        let (w, h) = self.size();
        if at.x < 0 || at.y < 0 || at.x >= i32::from(w) || at.y >= i32::from(h) {
            return None;
        }
        self.0
            .tiles
            .get(at.y as usize * usize::from(w) + at.x as usize)
    }
}

/// The initial command vocabulary. The host supplies player identity and
/// translates this into the same queued Move command used by human input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Move { unit: EntityKey, target: Tile },
}
