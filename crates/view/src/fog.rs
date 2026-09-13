//! Player-specific observation builder. The AI sees only `ai_api` data; raw
//! simulation access stays here. Call after every simulation tick to retain
//! sightings. Create a fresh Fog for each match/load (history is not saved yet).
//!
//! Vision counts change only when a source enters/leaves a tile. Radius stamps
//! are cached. u32 counts avoid the byte overflow possible with 256 overlapping
//! units. This is host-derived state and never changes a simulation hash.
use ai_api::{
    Building, Entity, EntityKey, FoggedView, Intent, Observation, Terrain, Tile, TileKnowledge,
};
use sim::{kinds, nav, Command, CommandKind, EntityId, Order, Simulation, TileMap};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Source {
    tile: (i32, i32),
    radius: i32,
}

pub struct Fog {
    player: u8,
    seed: u64,
    config: sim::SimConfig,
    map: TileMap,
    sources: BTreeMap<EntityId, Source>,
    stamps: BTreeMap<i32, Vec<(i32, i32)>>,
    counts: Vec<u32>,
    visible: BTreeSet<usize>,
    observation: Observation,
}

impl Fog {
    /// Invalid player identities fail closed, including Gaia.
    pub fn new(sim: &Simulation, player: u8) -> Option<Self> {
        sim.player(player)?;
        let map = sim.map().clone();
        let size = (map.width() * map.height()) as usize;
        Some(Self {
            player,
            seed: sim.seed(),
            config: sim.config().clone(),
            map,
            sources: BTreeMap::new(),
            stamps: BTreeMap::new(),
            counts: vec![0; size],
            visible: BTreeSet::new(),
            observation: Observation {
                player,
                tick: sim.tick(),
                width: sim.map().width() as u16,
                height: sim.map().height() as u16,
                tiles: vec![TileKnowledge::default(); size],
                entities: Vec::new(),
            },
        })
    }

    /// Updates this match's observation. The map is immutable in a Simulation;
    /// the caller must not reuse Fog across matches or rewinds.
    pub fn update(&mut self, sim: &Simulation) -> FoggedView<'_> {
        assert!(
            sim.tick() >= self.observation.tick,
            "recreate Fog after rewind"
        );
        assert_eq!(sim.seed(), self.seed, "recreate Fog for a new match");
        assert_eq!(sim.config(), &self.config, "recreate Fog for a new match");
        let world = sim.world();
        let mut sources = BTreeMap::new();
        for slot in world.slots() {
            let i = slot.index();
            if world.owner[i] != self.player
                || world.dying[i] != 0
                || world.inside[i].is_some()
                || world.construction[i].is_some()
            {
                continue;
            }
            let tile = nav::tile_of(world.pos[i]);
            if !self.map.in_bounds(tile.0, tile.1) {
                continue;
            }
            let radius = kinds::info(world.kind[i]).combat.line_of_sight
                + i32::from(self.map.elevation(tile.0, tile.1) > 0);
            sources.insert(world.id_at(slot), Source { tile, radius });
        }
        let removed: Vec<_> = self
            .sources
            .iter()
            .filter(|(id, src)| sources.get(id) != Some(src))
            .map(|(_, s)| *s)
            .collect();
        let added: Vec<_> = sources
            .iter()
            .filter(|(id, src)| self.sources.get(id) != Some(src))
            .map(|(_, s)| *s)
            .collect();
        for source in removed {
            self.stamp(source, false);
        }
        for source in added {
            self.stamp(source, true);
        }
        self.sources = sources;
        self.observation.tick = sim.tick();
        self.observation.entities.clear();
        // Refresh only currently visible tiles. Hidden building memories remain
        // unchanged when an unseen building moves, changes owner or disappears.
        for &index in &self.visible {
            let x = index as i32 % self.map.width();
            let y = index as i32 / self.map.width();
            self.observation.tiles[index] = TileKnowledge {
                visible: true,
                terrain: Some(Terrain {
                    kind: self.map.terrain(x, y) as u8,
                    elevation: self.map.elevation(x, y),
                    walkable: self.map.walkable(x, y),
                }),
                building: None,
            };
        }
        for slot in world.slots() {
            let i = slot.index();
            if world.dying[i] != 0 || world.inside[i].is_some() {
                continue;
            }
            let kind = kinds::info(world.kind[i]);
            let tile = nav::tile_of(world.pos[i]);
            let anchor = nav::anchor_tile(world.pos[i], i32::from(kind.footprint));
            let footprint = nav::footprint_tiles(anchor.0, anchor.1, i32::from(kind.footprint));
            let seen = footprint.iter().any(|&(x, y)| self.is_visible(x, y));
            if world.owner[i] != self.player && !seen {
                continue;
            }
            let id = key(world.id_at(slot));
            self.observation.entities.push(Entity {
                id,
                owner: world.owner[i],
                kind: world.kind[i],
                tile: Tile {
                    x: tile.0,
                    y: tile.1,
                },
                idle: (world.owner[i] == self.player)
                    .then_some(matches!(world.order[i], Order::Idle)),
                scout: world.kind[i] == kinds::SCOUT,
            });
            if kind.buildable {
                for (x, y) in footprint {
                    if self.is_visible(x, y) {
                        let index = self.index(x, y);
                        self.observation.tiles[index].building = Some(Building {
                            id,
                            owner: world.owner[i],
                            kind: world.kind[i],
                        });
                    }
                }
            }
        }
        self.observation.view()
    }

    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    fn index(&self, x: i32, y: i32) -> usize {
        (y * self.map.width() + x) as usize
    }
    fn is_visible(&self, x: i32, y: i32) -> bool {
        self.map.in_bounds(x, y) && self.counts[self.index(x, y)] > 0
    }

    fn stamp(&mut self, source: Source, add: bool) {
        let offsets = self.stamps.entry(source.radius).or_insert_with(|| {
            let r = source.radius;
            (-r..=r)
                .flat_map(|y| (-r..=r).map(move |x| (x, y)))
                .filter(|&(x, y)| x * x + y * y <= r * r)
                .collect()
        });
        for &(dx, dy) in offsets.iter() {
            let (x, y) = (source.tile.0 + dx, source.tile.1 + dy);
            if !self.map.in_bounds(x, y) || !terrain_line_visible(&self.map, source.tile, (x, y)) {
                continue;
            }
            let index = (y * self.map.width() + x) as usize;
            if add {
                self.counts[index] += 1;
                self.visible.insert(index);
            } else {
                self.counts[index] -= 1;
                if self.counts[index] == 0 {
                    self.visible.remove(&index);
                    self.observation.tiles[index].visible = false;
                }
            }
        }
    }
}

// Integer tile rays: a source may see over one cliff level. A tile more than
// one level above the source and everything behind it remains hidden.
fn terrain_line_visible(map: &TileMap, from: (i32, i32), to: (i32, i32)) -> bool {
    let ceiling = map.elevation(from.0, from.1).saturating_add(1);
    nav::line_tiles(from, to)
        .iter()
        .all(|&(x, y)| map.elevation(x, y) <= ceiling)
}

fn key(id: EntityId) -> EntityKey {
    EntityKey {
        index: id.index() as u32,
        generation: id.generation(),
    }
}

/// Convert an AI intent to a normal player command. Invalid identity, stale
/// generation, foreign/non-mobile/garrisoned/dead unit, and out-of-map targets
/// fail before enqueueing. The simulation still validates ownership at execution.
/// The host supplies `player`; the AI cannot select another player's identity.
pub fn command(sim: &Simulation, player: u8, intent: Intent) -> Option<Command> {
    sim.player(player)?;
    let Intent::Move { unit, target } = intent;
    let id = EntityId::from_parts(unit.index, unit.generation);
    let i = sim.world().slot(id)?.index();
    let world = sim.world();
    if world.owner[i] != player
        || world.dying[i] != 0
        || world.inside[i].is_some()
        || !kinds::info(world.kind[i]).mobile
        || !sim.map().in_bounds(target.x, target.y)
    {
        return None;
    }
    Some(Command {
        player,
        kind: CommandKind::Move {
            ids: vec![id],
            target: nav::centre((target.x, target.y)),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_removal_edges_and_more_than_255_sources() {
        let sim = Simulation::new(1, sim::SimConfig::default());
        let mut fog = Fog::new(&sim, 0).unwrap();
        fog.map = TileMap::new(fog.map.width() as u16, fog.map.height() as u16);
        let source = Source {
            tile: (0, 0),
            radius: 4,
        };
        for _ in 0..300 {
            fog.stamp(source, true);
        }
        assert_eq!(fog.counts[0], 300);
        for _ in 0..299 {
            fog.stamp(source, false);
        }
        assert!(fog.is_visible(0, 0));
        fog.stamp(source, false);
        assert!(fog.visible.is_empty());
        assert!(fog.counts.iter().all(|&v| v == 0));
    }

    #[test]
    fn garrison_removes_scout_vision_and_ungarrison_restores_it() {
        let mut sim = Simulation::new(
            1,
            sim::SimConfig {
                map: sim::MapSpec {
                    kind: sim::MapKind::Flat,
                    size: 64,
                    players: 2,
                },
                wander: false,
                ..sim::SimConfig::default()
            },
        );
        for (kind, at) in [(kinds::TOWN_CENTER, (10, 10)), (kinds::SCOUT, (13, 10))] {
            sim.issue(Command {
                player: 0,
                kind: CommandKind::Spawn {
                    kind,
                    pos: nav::centre(at),
                },
            });
        }
        for _ in 0..3 {
            sim.step();
        }
        let id_of = |kind| {
            sim.world()
                .slots()
                .find(|s| sim.world().kind[s.index()] == kind)
                .map(|s| sim.world().id_at(s))
                .unwrap()
        };
        let scout = id_of(kinds::SCOUT);
        let tc = id_of(kinds::TOWN_CENTER);
        let mut fog = Fog::new(&sim, 0).unwrap();
        fog.update(&sim);
        assert!(fog.sources.contains_key(&scout));
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Garrison {
                ids: vec![scout],
                building: tc,
            },
        });
        for _ in 0..30 {
            sim.step();
            fog.update(&sim);
        }
        assert_eq!(sim.world().inside[scout.index()], Some(tc));
        assert!(!fog.sources.contains_key(&scout));
        assert!(command(
            &sim,
            0,
            Intent::Move {
                unit: key(scout),
                target: Tile { x: 20, y: 20 }
            }
        )
        .is_none());
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Ungarrison { building: tc },
        });
        for _ in 0..3 {
            sim.step();
            fog.update(&sim);
        }
        assert!(sim.world().inside[scout.index()].is_none());
        assert!(fog.sources.contains_key(&scout));
    }

    #[test]
    fn cliff_occlusion_uses_source_height() {
        let mut map = TileMap::new(8, 8);
        for y in 0..=8 {
            for x in 3..=5 {
                map.set_corner(x, y, 2);
            }
        }
        assert!(!terrain_line_visible(&map, (1, 3), (6, 3)));
        assert!(terrain_line_visible(&map, (3, 3), (6, 3)));
        for y in 0..=8 {
            for x in 3..=5 {
                map.set_corner(x, y, 1);
            }
        }
        assert!(terrain_line_visible(&map, (1, 3), (6, 3)));
    }
}
