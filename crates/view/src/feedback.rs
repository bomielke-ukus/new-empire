//! Short-lived impact cues, collected once per simulation tick by either viewer.
//! This presentation history never changes the simulation or its replay.
use sim::{EntityId, Event, Replay, ReplayError, Simulation, Vec2Fx};

use crate::{fx_to_f32, iso, palette, Atlas, Scene};

const HIT_TICKS: u64 = 4;

#[derive(Clone, Debug)]
struct Impact {
    target: EntityId,
    pos: Vec2Fx,
    tick: u64,
}

/// A 200 ms impact flash at the place damage actually landed. Multiple hits on
/// one target coalesce; tick time makes pause and speed controls apply to it.
#[derive(Default)]
pub struct CombatFeedback {
    impacts: Vec<Impact>,
    last_tick: Option<u64>,
}

impl CombatFeedback {
    /// Call after every tick, including each tick in a fast-forwarded frame.
    pub fn observe(&mut self, sim: &Simulation) {
        let tick = sim.tick();
        if self.last_tick == Some(tick) {
            return;
        }
        if self.last_tick.is_some_and(|last| tick < last) {
            self.impacts.clear();
        }
        self.last_tick = Some(tick);
        self.impacts
            .retain(|hit| tick.saturating_sub(hit.tick) < HIT_TICKS);
        for event in sim.events() {
            if let Event::Hit {
                target,
                pos,
                damage,
            } = *event
            {
                if damage > 0 {
                    self.impacts.retain(|hit| hit.target != target);
                    self.impacts.push(Impact { target, pos, tick });
                }
            }
        }
    }

    /// Render replay history through the same event collector as the live app.
    pub fn replay(&mut self, replay: &Replay) -> Result<Simulation, ReplayError> {
        replay.validate()?;
        *self = Self::default();
        let mut sim = Simulation::new(replay.seed, replay.config.clone());
        let mut commands = replay.commands.iter().peekable();
        while sim.tick() < replay.ticks {
            while commands.peek().is_some_and(|(tick, _)| *tick == sim.tick()) {
                sim.issue(commands.next().unwrap().1.clone());
            }
            sim.step();
            self.observe(&sim);
        }
        Ok(sim)
    }

    /// Add a small expanding spark, outlined for contrast against any terrain.
    pub fn decorate(&self, scene: &mut Scene, sim: &Simulation, atlas: &Atlas) {
        for hit in &self.impacts {
            let age = sim.tick().saturating_sub(hit.tick);
            if age >= HIT_TICKS {
                continue;
            }
            let (x, y) = (fx_to_f32(hit.pos.x), fx_to_f32(hit.pos.y));
            let (gx, gy) = iso::project(x, y, iso::ground_height(sim.map(), x, y));
            let radius = 3.0 + age as f32;
            for (dx, dy) in [(radius, 0.0), (-radius, 0.0), (0.0, radius), (0.0, -radius)] {
                let mut ray = crate::scene::overlay(
                    atlas.solid(palette::BLACK),
                    gx + dx - 2.0,
                    gy - 24.0 + dy - 2.0,
                    0,
                    x + y + 0.8,
                    u32::MAX,
                );
                scene.sprites.push(ray);
                ray.u = atlas.solid(palette::GOLD_LIGHT).x;
                ray.v = atlas.solid(palette::GOLD_LIGHT).y;
                ray.x += 1.0;
                ray.y += 1.0;
                ray.w = 2.0;
                ray.h = 2.0;
                scene.sprites.push(ray);
            }
        }
        scene
            .sprites
            .sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.slot.cmp(&b.slot)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_history_survives_frames_expires_and_preserves_replay() {
        let replay: Replay =
            ron::from_str(include_str!("../../sim/tests/corpus/battle-40v40.ron")).unwrap();
        let mut feedback = CombatFeedback::default();
        let actual = feedback.replay(&replay).unwrap();
        let expected = replay.run(|_, _| {}).unwrap();
        assert_eq!(actual.state_hash(), expected.state_hash());
        assert_eq!(actual.replay(), expected.replay());
        // At the decisive death the hit still needs to be shown, even though
        // the target has become a corpse. A repeated render cannot duplicate it.
        assert!(!feedback.impacts.is_empty());
        let count = feedback.impacts.len();
        feedback.observe(&actual);
        assert_eq!(feedback.impacts.len(), count);
        let mut actual = actual;
        for _ in 0..HIT_TICKS {
            actual.step();
            feedback.observe(&actual);
        }
        assert!(feedback.impacts.is_empty());
        let mut invalid = replay;
        invalid.version = u32::MAX;
        assert!(feedback.replay(&invalid).is_err());
    }
}
