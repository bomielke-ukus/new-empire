//! Minimal deterministic scouting policy. No simulation or presentation crate
//! is available here. Economy, combat and difficulty are later M5 chunks.
#![forbid(unsafe_code)]
use ai_api::{FoggedView, Intent, Tile};

/// At most one decision per 20 simulation ticks; repeated calls on the same
/// tick do not duplicate commands. No wall clock or random generator is used.
#[derive(Default)]
pub struct ScoutAi {
    next_tick: u64,
}

impl ScoutAi {
    pub fn decide(&mut self, view: FoggedView<'_>) -> Option<Intent> {
        if view.tick() < self.next_tick {
            return None;
        }
        self.next_tick = view.tick().saturating_add(20);
        let scout = view
            .entities()
            .iter()
            .filter(|e| e.owner == view.player() && e.scout && e.idle == Some(true))
            .min_by_key(|e| e.id)?;
        let (width, height) = view.size();
        // Walk to a known, walkable frontier. Pathfinding remains the normal
        // player's pathfinding; this policy cannot query hidden obstacles.
        let target = (0..i32::from(height))
            .flat_map(|y| (0..i32::from(width)).map(move |x| Tile { x, y }))
            .filter(|&at| {
                at != scout.tile
                    && view.tile(at).is_some_and(|t| {
                        t.terrain.is_some_and(|t| t.walkable) && t.building.is_none()
                    })
            })
            .filter(|at| {
                [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                    view.tile(Tile {
                        x: at.x + dx,
                        y: at.y + dy,
                    })
                    .is_some_and(|t| t.terrain.is_none())
                })
            })
            .min_by_key(|at| {
                let dx = i64::from(at.x) - i64::from(scout.tile.x);
                let dy = i64::from(at.y) - i64::from(scout.tile.y);
                (dx * dx + dy * dy, at.y, at.x)
            })?;
        Some(Intent::Move {
            unit: scout.id,
            target,
        })
    }
}
