//! Selection state: what is selected, control groups, picking and band-box.

use sim::kinds;
use sim::{EntityId, Simulation};
use view::{Atlas, Camera, Scene};

/// The player's current selection and saved groups.
#[derive(Default)]
pub struct Selection {
    /// Selected entity ids, in a stable order.
    pub ids: Vec<EntityId>,
    /// Control groups 0..=9.
    pub groups: Vec<Vec<EntityId>>,
    /// Band-box in progress: window px of the press.
    pub drag_from: Option<(f32, f32)>,
    /// Last idle villager cycled to, for the `.` key.
    pub idle_cursor: usize,
}

impl Selection {
    /// Empty selection with ten empty groups.
    pub fn new() -> Selection {
        Selection {
            groups: vec![Vec::new(); 10],
            ..Default::default()
        }
    }

    /// Drops ids that no longer exist.
    pub fn prune(&mut self, sim: &Simulation) {
        self.ids.retain(|id| sim.world().contains(*id));
        for g in &mut self.groups {
            g.retain(|id| sim.world().contains(*id));
        }
    }

    /// Slots of the selected entities, for the scene and HUD.
    pub fn slots(&self, sim: &Simulation) -> Vec<u32> {
        self.ids
            .iter()
            .filter_map(|id| sim.world().slot(*id))
            .map(|s| s.index() as u32)
            .collect()
    }

    /// Replaces the selection.
    pub fn set(&mut self, ids: Vec<EntityId>) {
        self.ids = ids;
        self.ids.sort();
        self.ids.dedup();
    }

    /// Adds or removes one id (shift-click).
    pub fn toggle(&mut self, id: EntityId) {
        if let Some(i) = self.ids.iter().position(|x| *x == id) {
            self.ids.remove(i);
        } else {
            self.ids.push(id);
            self.ids.sort();
        }
    }

    /// Adds many (shift-drag).
    pub fn extend(&mut self, ids: Vec<EntityId>) {
        self.ids.extend(ids);
        self.ids.sort();
        self.ids.dedup();
    }

    /// Selected entities of `player` that are mobile.
    pub fn own_mobile(&self, sim: &Simulation, player: u8) -> Vec<EntityId> {
        self.filter(sim, |i| {
            sim.world().owner[i] == player && kinds::info(sim.world().kind[i]).mobile
        })
    }

    /// Selected villagers of `player`.
    pub fn own_villagers(&self, sim: &Simulation, player: u8) -> Vec<EntityId> {
        self.filter(sim, |i| {
            sim.world().owner[i] == player && sim.world().kind[i] == kinds::VILLAGER
        })
    }

    /// The first selected building of `player` that trains units.
    pub fn own_trainer(&self, sim: &Simulation, player: u8) -> Option<EntityId> {
        self.filter(sim, |i| {
            sim.world().owner[i] == player
                && kinds::info(sim.world().kind[i]).trains
                && sim.world().construction[i].is_none()
        })
        .into_iter()
        .next()
    }

    fn filter(&self, sim: &Simulation, pred: impl Fn(usize) -> bool) -> Vec<EntityId> {
        self.ids
            .iter()
            .copied()
            .filter(|id| sim.world().slot(*id).is_some_and(|s| pred(s.index())))
            .collect()
    }

    /// Cycles to the next idle villager, returning it.
    pub fn next_idle(&mut self, sim: &Simulation, player: u8) -> Option<EntityId> {
        let idle = sim.idle_villagers(player);
        if idle.is_empty() {
            return None;
        }
        self.idle_cursor %= idle.len();
        let id = idle[self.idle_cursor];
        self.idle_cursor += 1;
        self.set(vec![id]);
        Some(id)
    }
}

/// The entity under a window point: the top-most sprite whose opaque pixel
/// is under the cursor. Overlays (rings, ghosts) are skipped.
pub fn pick(
    scene: &Scene,
    atlas: &Atlas,
    camera: &Camera,
    sim: &Simulation,
    px: f32,
    py: f32,
) -> Option<EntityId> {
    let zoom = camera.zoom();
    for s in scene.sprites.iter().rev() {
        if s.slot == u32::MAX || s.screen {
            continue;
        }
        let (x0, y0) = camera.to_window(s.x, s.y);
        let (dx, dy) = (px - x0, py - y0);
        if dx < 0.0 || dy < 0.0 || dx >= s.w * zoom || dy >= s.h * zoom {
            continue;
        }
        // Map window px into the atlas rect, which may be authored at 2×.
        let mut sx = (dx / (s.w * zoom) * s.uw as f32) as u32;
        let sy = (dy / (s.h * zoom) * s.vh as f32) as u32;
        if s.flip {
            sx = s.uw as u32 - 1 - sx.min(s.uw as u32 - 1);
        }
        let idx = atlas.index_at(s.u as u32 + sx, s.v as u32 + sy);
        if idx == 0 || idx == view::palette::SHADOW {
            continue;
        }
        // Rings share the unit's slot; only the unit's own frame counts.
        let i = s.slot as usize;
        let world = sim.world();
        if world.slots().any(|sl| sl.index() == i) {
            return Some(world.id_at(world.slots().find(|sl| sl.index() == i).unwrap()));
        }
    }
    None
}

/// Entities of `player` whose ground point falls inside a window rectangle.
/// Mobile units take precedence: if any are inside, buildings are left out,
/// so a drag over a town never grabs the Town Center by accident.
pub fn band_box(
    sim: &Simulation,
    camera: &Camera,
    player: u8,
    a: (f32, f32),
    b: (f32, f32),
) -> Vec<EntityId> {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    let world = sim.world();
    let mut mobile = Vec::new();
    let mut fixed = Vec::new();
    for slot in world.slots() {
        let i = slot.index();
        if world.owner[i] != player {
            continue;
        }
        let p = world.pos[i];
        let (wx, wy) = (view::fx_to_f32(p.x), view::fx_to_f32(p.y));
        let (gx, gy) = view::iso::project(wx, wy, view::iso::ground_height(sim.map(), wx, wy));
        let (sx, sy) = camera.to_window(gx, gy);
        if sx >= x0 && sx <= x1 && sy >= y0 && sy <= y1 {
            if kinds::info(world.kind[i]).mobile {
                mobile.push(world.id_at(slot));
            } else {
                fixed.push(world.id_at(slot));
            }
        }
    }
    if mobile.is_empty() {
        fixed
    } else {
        mobile
    }
}

/// Same-kind entities of `player` visible on screen, for double-click.
pub fn same_kind_on_screen(
    sim: &Simulation,
    camera: &Camera,
    player: u8,
    kind: sim::entity::KindId,
) -> Vec<EntityId> {
    let (w, h) = camera.viewport;
    band_box(sim, camera, player, (0.0, 0.0), (w, h))
        .into_iter()
        .filter(|id| {
            sim.world()
                .slot(*id)
                .is_some_and(|s| sim.world().kind[s.index()] == kind)
        })
        .collect()
}
