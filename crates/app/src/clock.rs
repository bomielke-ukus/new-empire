//! Fixed-timestep clock: turns wall-clock time into whole simulation ticks.
//!
//! The simulation only ever advances by whole 50 ms ticks. The renderer runs
//! at whatever rate the display wants and interpolates between the last two
//! sim states using [`FixedClock::alpha`]. Game speed scales how much sim
//! time a real second is worth; it never changes the tick length.

use std::time::{Duration, Instant};

/// Accumulates real time and hands out ticks.
pub struct FixedClock {
    tick_len: Duration,
    accumulator: Duration,
    last: Instant,
    /// Sim seconds per real second. 1.0 is normal speed.
    pub speed: f32,
    /// Upper bound on ticks per frame, so a stall (window drag, debugger)
    /// does not turn into a spiral of catch-up ticks.
    pub max_ticks_per_frame: u32,
    paused: bool,
}

impl FixedClock {
    /// A clock producing ticks of `tick_ms` milliseconds.
    pub fn new(tick_ms: u32) -> FixedClock {
        FixedClock {
            tick_len: Duration::from_millis(tick_ms as u64),
            accumulator: Duration::ZERO,
            last: Instant::now(),
            speed: 1.0,
            max_ticks_per_frame: 8,
            paused: false,
        }
    }

    /// Whether the clock is paused.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Pauses or resumes. Resuming discards time that passed while paused.
    pub fn set_paused(&mut self, paused: bool) {
        if self.paused && !paused {
            self.last = Instant::now();
            self.accumulator = Duration::ZERO;
        }
        self.paused = paused;
    }

    /// Advances to `now` and returns how many ticks the sim should run.
    pub fn advance(&mut self, now: Instant) -> u32 {
        let real = now.saturating_duration_since(self.last);
        self.last = now;
        if self.paused {
            return 0;
        }
        // Scale in whole nanoseconds. `Duration::mul_f32` rounds through f32
        // seconds and can turn 51 ms into 50.999998 ms, which then misses a
        // tick boundary the integer maths would have hit.
        let scaled = (real.as_nanos() as f64 * f64::from(self.speed.max(0.0))) as u64;
        self.accumulator += Duration::from_nanos(scaled);
        let mut ticks = 0;
        while self.accumulator >= self.tick_len && ticks < self.max_ticks_per_frame {
            self.accumulator -= self.tick_len;
            ticks += 1;
        }
        if ticks == self.max_ticks_per_frame {
            // We fell behind; drop the backlog rather than chase it.
            self.accumulator = Duration::ZERO;
        }
        ticks
    }

    /// Fraction of the way from the previous tick to the next, in `[0, 1)`.
    /// Renderers use it to interpolate positions.
    #[allow(dead_code)] // consumed by the sprite renderer from M1
    pub fn alpha(&self) -> f32 {
        (self.accumulator.as_secs_f32() / self.tick_len.as_secs_f32()).clamp(0.0, 0.999_99)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_one_tick_per_tick_length() {
        let mut c = FixedClock::new(50);
        let t0 = c.last;
        assert_eq!(c.advance(t0 + Duration::from_millis(49)), 0);
        assert_eq!(c.advance(t0 + Duration::from_millis(100)), 2);
        assert!(c.alpha() < 0.01);
        assert_eq!(c.advance(t0 + Duration::from_millis(125)), 0);
        assert!((c.alpha() - 0.5).abs() < 0.01);
    }

    #[test]
    fn speed_scales_tick_rate() {
        let mut c = FixedClock::new(50);
        c.speed = 2.0;
        let t0 = c.last;
        assert_eq!(c.advance(t0 + Duration::from_millis(100)), 4);
        c.speed = 0.5;
        assert_eq!(c.advance(t0 + Duration::from_millis(200)), 1);
    }

    #[test]
    fn caps_catch_up_and_drops_backlog() {
        let mut c = FixedClock::new(50);
        let t0 = c.last;
        assert_eq!(
            c.advance(t0 + Duration::from_secs(10)),
            c.max_ticks_per_frame
        );
        assert_eq!(
            c.advance(t0 + Duration::from_secs(10) + Duration::from_millis(10)),
            0
        );
        assert!(c.alpha() < 0.25);
    }

    #[test]
    fn pause_freezes_and_resume_does_not_jump() {
        let mut c = FixedClock::new(50);
        let t0 = c.last;
        c.set_paused(true);
        assert_eq!(c.advance(t0 + Duration::from_secs(5)), 0);
        c.set_paused(false);
        let t1 = c.last;
        assert_eq!(c.advance(t1 + Duration::from_millis(60)), 1);
    }
}
