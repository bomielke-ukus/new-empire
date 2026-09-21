//! The performance readout (`F4`): what a frame and a tick cost, where
//! the tick's time went, and the budgets they are held to (`docs/04` §12,
//! `docs/03` §9). A meter, not a control: it has no binding, it draws over
//! the world in the corner, and it exists so the measurement on known
//! hardware, the one a shared runner cannot take, can be read off the
//! downloaded build without a terminal.
//!
//! The numbers come from the app's clock; this module only keeps the
//! windows and lays out the lines, so what it shows is pinned by a golden
//! image with fixed samples.

use std::collections::VecDeque;

use sim::Timings;

/// Samples kept per series: two seconds of frames at sixty a second, six
/// seconds of ticks at twenty.
pub const WINDOW: usize = 120;

/// The simulation's share of the 50 ms tick, from `docs/04` §12.
pub const TICK_BUDGET_MS: f32 = 19.0;

/// The frame, from `docs/03` §9 (`UX-PERF-01`): 60 fps with headroom.
pub const FRAME_BUDGET_MS: f32 = 8.0;

/// `docs/04` §12's budget for each phase, in milliseconds, in
/// [`Timings::PHASES`] order and then the opponents' thinking. Commands
/// and orders share the combat row's three milliseconds, so they carry
/// no budget of their own.
pub const PHASE_BUDGET_MS: [Option<f32>; 8] = [
    None,
    None,
    Some(6.0),
    Some(3.0),
    Some(3.0),
    Some(2.0),
    Some(1.0),
    Some(4.0),
];

/// A window of recent samples, in nanoseconds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Series {
    samples: VecDeque<u64>,
}

impl Series {
    /// Records one sample, dropping the oldest past [`WINDOW`].
    pub fn push(&mut self, ns: u64) {
        if self.samples.len() == WINDOW {
            self.samples.pop_front();
        }
        self.samples.push_back(ns);
    }

    /// The window's mean, p99 and max.
    pub fn stat(&self) -> Stat {
        if self.samples.is_empty() {
            return Stat::default();
        }
        let mut sorted: Vec<u64> = self.samples.iter().copied().collect();
        sorted.sort_unstable();
        let ms = |ns: u64| ns as f32 / 1_000_000.0;
        let sum: u64 = sorted.iter().sum();
        Stat {
            mean_ms: ms(sum) / sorted.len() as f32,
            p99_ms: ms(sorted[((sorted.len() - 1) as f32 * 0.99) as usize]),
            max_ms: ms(*sorted.last().expect("not empty")),
        }
    }
}

/// One series summarised, in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stat {
    /// The window's mean.
    pub mean_ms: f32,
    /// The window's 99th percentile.
    pub p99_ms: f32,
    /// The window's worst.
    pub max_ms: f32,
}

/// The app's accumulator: a series per thing measured.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meter {
    frames: Series,
    ticks: Series,
    thinking: Series,
    phases: [Series; 7],
}

impl Meter {
    /// One frame took `ns` from the last.
    pub fn frame(&mut self, ns: u64) {
        self.frames.push(ns);
    }

    /// One tick ran with these phases, after the opponents thought for
    /// `thinking` nanoseconds.
    pub fn tick(&mut self, t: &Timings, thinking: u64) {
        self.ticks.push(t.total() + thinking);
        self.thinking.push(thinking);
        for (series, ns) in self.phases.iter_mut().zip(t.as_array()) {
            series.push(ns);
        }
    }

    /// What the box shows now.
    pub fn readout(&self, fps: f32, entities: usize, sprites: usize) -> Readout {
        let mut phases = [Stat::default(); 8];
        for (k, series) in self.phases.iter().enumerate() {
            phases[k] = series.stat();
        }
        phases[7] = self.thinking.stat();
        Readout {
            fps,
            frame: self.frames.stat(),
            tick: self.ticks.stat(),
            phases,
            entities,
            sprites,
        }
    }
}

/// What the readout shows: every number the box draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Readout {
    /// Frames per second, as the title bar counts them.
    pub fps: f32,
    /// Wall time between frames.
    pub frame: Stat,
    /// The whole tick: the phases and the thinking together.
    pub tick: Stat,
    /// The phases in [`Timings::PHASES`] order, then the thinking.
    pub phases: [Stat; 8],
    /// Live entities, trees and all.
    pub entities: usize,
    /// World sprites drawn last frame.
    pub sprites: usize,
}

/// The names of the rows of [`Readout::phases`].
pub const ROWS: [&str; 8] = [
    "COMMANDS", "ORDERS", "PATHS", "MOVEMENT", "COMBAT", "ECONOMY", "FOG", "THINKING",
];

impl Readout {
    /// The lines of the box, each as four columns: what, mean, p99 and
    /// the budget or the worst. The first line is the header.
    pub fn lines(&self) -> Vec<[String; 4]> {
        let ms = |v: f32| format!("{v:.2}");
        let mut out = vec![[
            String::new(),
            "MEAN MS".to_string(),
            "P99".to_string(),
            "BUDGET".to_string(),
        ]];
        out.push([
            format!("FRAME {:.0} FPS", self.fps),
            ms(self.frame.mean_ms),
            ms(self.frame.p99_ms),
            format!("{FRAME_BUDGET_MS:.0}"),
        ]);
        out.push([
            "TICK".to_string(),
            ms(self.tick.mean_ms),
            ms(self.tick.p99_ms),
            format!("{TICK_BUDGET_MS:.0}"),
        ]);
        for (k, name) in ROWS.iter().enumerate() {
            out.push([
                format!("  {name}"),
                ms(self.phases[k].mean_ms),
                ms(self.phases[k].p99_ms),
                PHASE_BUDGET_MS[k].map_or(String::new(), |b| format!("{b:.0}")),
            ]);
        }
        out.push([
            format!("ENTITIES {}", self.entities),
            String::new(),
            String::new(),
            format!("SPRITES {}", self.sprites),
        ]);
        out
    }

    /// Fixed numbers for a golden image and the docs.
    pub fn sample() -> Readout {
        let stat = |mean: f32, p99: f32| Stat {
            mean_ms: mean,
            p99_ms: p99,
            max_ms: p99 * 2.0,
        };
        Readout {
            fps: 60.0,
            frame: stat(16.6, 17.4),
            tick: stat(2.31, 4.12),
            phases: [
                stat(0.01, 0.03),
                stat(0.34, 0.71),
                stat(0.62, 1.85),
                stat(0.41, 0.66),
                stat(0.28, 0.52),
                stat(0.09, 0.14),
                stat(0.12, 0.21),
                stat(0.44, 1.60),
            ],
            entities: 412,
            sprites: 537,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_series_keeps_the_window_and_summarises_it() {
        let mut s = Series::default();
        assert_eq!(s.stat(), Stat::default());
        for n in 1..=(WINDOW as u64 + 10) {
            s.push(n * 1_000_000);
        }
        // The first ten fell off the front.
        let st = s.stat();
        assert_eq!(st.max_ms, (WINDOW as f32) + 10.0);
        assert!((st.mean_ms - 70.5).abs() < 0.01, "{st:?}");
        assert_eq!(st.p99_ms, 128.0, "the 99th of 120, counted from eleven");
    }

    #[test]
    fn the_meter_sums_the_phases_and_the_thinking_into_the_tick() {
        let mut m = Meter::default();
        m.frame(16_000_000);
        m.tick(
            &Timings {
                paths: 600_000,
                fog: 100_000,
                ..Timings::default()
            },
            300_000,
        );
        let r = m.readout(60.0, 5, 7);
        assert!((r.tick.mean_ms - 1.0).abs() < 1e-6);
        assert!((r.phases[2].mean_ms - 0.6).abs() < 1e-6);
        assert!((r.phases[7].mean_ms - 0.3).abs() < 1e-6);
        assert_eq!(r.frame.max_ms, 16.0);
        assert_eq!((r.entities, r.sprites), (5, 7));
    }

    #[test]
    fn the_lines_carry_the_budgets_of_the_specs() {
        let lines = Readout::sample().lines();
        assert_eq!(lines.len(), 1 + 2 + ROWS.len() + 1);
        assert_eq!(lines[1][0], "FRAME 60 FPS");
        assert_eq!(lines[1][3], "8");
        assert_eq!(lines[2][3], "19");
        // Paths get §12's six milliseconds; commands share combat's.
        assert_eq!(lines[3][0], "  COMMANDS");
        assert_eq!(lines[3][3], "");
        assert_eq!(lines[5][0], "  PATHS");
        assert_eq!(lines[5][3], "6");
        assert_eq!(lines[10][0], "  THINKING");
        assert_eq!(lines[10][3], "4");
        assert_eq!(lines[11][0], "ENTITIES 412");
    }
}
