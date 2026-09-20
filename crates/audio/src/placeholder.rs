//! Placeholder sounds, synthesised: a clip for every cue so the game is
//! heard before a single recording exists, the way the placeholder sprites
//! stand in for art. They are honest about what they are, short tones and
//! noise bursts with the right shape and length, so the feel of the
//! feedback (`UX-AUDIO-01`) can be judged now and the recordings dropped
//! in later without a code change (`docs/07` Q7).

use crate::{Clip, Cue, Library, Task};
use sim::{Age, Class};
use std::sync::Arc;

/// Samples per second: plenty for foley, small in memory.
pub const RATE: u32 = 22_050;

/// A clip for every cue, with the variations the spec asks for: three to
/// five per acknowledgment, two or three for the rest.
pub fn library() -> Library {
    let mut lib = Library::default();
    for cue in Cue::all() {
        lib.insert(cue, clips_for(cue));
    }
    lib
}

fn clips_for(cue: Cue) -> Vec<Clip> {
    match cue {
        Cue::Ack(c) => (0..4).map(|v| ack(c, v, false)).collect(),
        Cue::Select(c) => (0..2).map(|v| ack(c, v, true)).collect(),
        Cue::Work(t) => (0..3).map(|v| work(t, v)).collect(),
        Cue::Hit { building } => (0..3).map(|v| hit(building, v)).collect(),
        Cue::Death(c) => (0..2).map(|v| death(c, v)).collect(),
        Cue::Completed => vec![chime(&[523.0, 784.0], 140, 0.5)],
        Cue::Trained => vec![chime(&[880.0], 220, 0.4)],
        Cue::Deposited => (0..2)
            .map(|v| {
                Synth::new()
                    .tone(1700.0 + v as f32 * 150.0, 1700.0, 60, Wave::Sine, 0.35)
                    .done()
            })
            .collect(),
        Cue::Research => vec![chime(&[523.0, 659.0, 784.0], 120, 0.5)],
        Cue::Fanfare(a) => vec![fanfare(a)],
        Cue::Click => vec![Synth::new()
            .tone(1100.0, 900.0, 12, Wave::Square, 0.3)
            .done()],
        Cue::Invalid => vec![Synth::new()
            .tone(110.0, 100.0, 140, Wave::Square, 0.35)
            .done()],
        Cue::Alarm => vec![bell()],
        Cue::Loss => vec![Synth::new()
            .tone(140.0, 70.0, 220, Wave::Sine, 0.6)
            .burst(120, 400.0, 0.3)
            .done()],
    }
}

/// The bark: a short two-note call, its voice by class, its contour by
/// variation. A selection is the first note alone.
fn ack(class: Class, variation: usize, short: bool) -> Clip {
    let (base, wave, ms) = match class {
        Class::Villager => (440.0, Wave::Sine, 110),
        Class::Infantry => (170.0, Wave::Square, 140),
        Class::Ranged => (330.0, Wave::Triangle, 120),
        Class::Cavalry => (620.0, Wave::Triangle, 160),
        Class::Siege => (90.0, Wave::Saw, 240),
        _ => (300.0, Wave::Sine, 100),
    };
    // Up a semitone per variation, and the second note rises or falls.
    let f = base * 1.0595_f32.powi(variation as i32);
    let mut s = Synth::new().tone(f, f * 1.05, ms, wave, 0.45);
    if !short {
        let second = if variation.is_multiple_of(2) {
            f * 1.25
        } else {
            f * 0.8
        };
        s = s.gap(20).tone(second, second * 0.97, ms + 30, wave, 0.4);
    }
    s.done()
}

/// A swing: the sound of the tool meeting the material.
fn work(task: Task, variation: usize) -> Clip {
    let v = 1.0 + variation as f32 * 0.08;
    match task {
        Task::Chop => Synth::new()
            .burst(50, 1400.0 * v, 0.7)
            .tone(220.0 * v, 160.0, 60, Wave::Sine, 0.35)
            .done(),
        Task::Mine => Synth::new()
            .tone(2400.0 * v, 2300.0, 40, Wave::Triangle, 0.3)
            .burst(40, 3000.0, 0.4)
            .done(),
        Task::Forage => Synth::new().burst(120, 900.0 * v, 0.25).done(),
        Task::Farm => Synth::new().burst(150, 600.0 * v, 0.3).done(),
        Task::Build => Synth::new()
            .tone(700.0 * v, 500.0, 50, Wave::Square, 0.35)
            .burst(30, 2000.0, 0.4)
            .done(),
    }
}

/// A blow landing: a thud, lower and longer on a building.
fn hit(building: bool, variation: usize) -> Clip {
    let v = 1.0 + variation as f32 * 0.1;
    if building {
        Synth::new()
            .burst(110, 300.0 * v, 0.6)
            .tone(90.0 * v, 60.0, 130, Wave::Sine, 0.5)
            .done()
    } else {
        Synth::new()
            .burst(40, 1200.0 * v, 0.5)
            .tone(200.0 * v, 120.0, 60, Wave::Sine, 0.4)
            .done()
    }
}

/// A fall: a unit's cry down, a building's rumble.
fn death(class: Class, variation: usize) -> Clip {
    let v = 1.0 + variation as f32 * 0.12;
    match class {
        Class::Building => Synth::new()
            .burst(600, 180.0 * v, 0.8)
            .tone(55.0 * v, 40.0, 500, Wave::Sine, 0.5)
            .done(),
        _ => Synth::new()
            .tone(320.0 * v, 90.0, 320, Wave::Triangle, 0.5)
            .done(),
    }
}

/// Notes in a row, each ringing into the next.
fn chime(notes: &[f32], ms: u32, gain: f32) -> Clip {
    let mut s = Synth::new();
    for f in notes {
        s = s.tone(*f, *f, ms, Wave::Sine, gain);
    }
    s.done()
}

/// Four notes, the intervals by age: the motif grows with the player.
fn fanfare(age: Age) -> Clip {
    let steps: [f32; 4] = match age {
        Age::Stone => [1.0, 1.0, 1.5, 1.5],
        Age::Tool => [1.0, 1.25, 1.5, 2.0],
        Age::Bronze => [1.0, 1.5, 1.25, 2.0],
        Age::Iron => [1.0, 1.5, 2.0, 2.5],
    };
    let mut s = Synth::new();
    for (i, k) in steps.iter().enumerate() {
        let ms = if i == 3 { 420 } else { 180 };
        s = s
            .tone(392.0 * k, 392.0 * k, ms, Wave::Triangle, 0.45)
            .gap(20);
    }
    s.done()
}

/// The bell: partials decaying together, struck twice.
fn bell() -> Clip {
    let mut s = Synth::new();
    for _ in 0..2 {
        s = s.chord(&[660.0, 990.0, 1320.0, 2640.0], 700, 0.3).gap(120);
    }
    s.done()
}

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Square,
    Triangle,
    Saw,
}

/// A little sequencer: tones and noise bursts appended in turn, each with
/// an envelope, normalised at the end.
struct Synth {
    out: Vec<f32>,
    seed: u64,
}

impl Synth {
    fn new() -> Synth {
        Synth {
            out: Vec::new(),
            seed: 0x2545_F491_4F6C_DD1D,
        }
    }

    fn noise(&mut self) -> f32 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;
        (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }

    /// Silence.
    fn gap(mut self, ms: u32) -> Synth {
        self.out.extend(std::iter::repeat_n(0.0, samples(ms)));
        self
    }

    /// A tone sliding from `from` to `to` Hz, with a quick attack and a
    /// decay to nothing over its length.
    fn tone(mut self, from: f32, to: f32, ms: u32, wave: Wave, gain: f32) -> Synth {
        let n = samples(ms);
        let mut phase = 0.0_f32;
        for i in 0..n {
            let t = i as f32 / n as f32;
            let f = from + (to - from) * t;
            phase = (phase + f / RATE as f32) % 1.0;
            let raw = match wave {
                Wave::Sine => (phase * std::f32::consts::TAU).sin(),
                Wave::Square => {
                    if phase < 0.5 {
                        0.6
                    } else {
                        -0.6
                    }
                }
                Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
                Wave::Saw => 2.0 * phase - 1.0,
            };
            self.out.push(raw * envelope(i, n) * gain);
        }
        self
    }

    /// Several tones at once, ringing down.
    fn chord(mut self, freqs: &[f32], ms: u32, gain: f32) -> Synth {
        let n = samples(ms);
        let start = self.out.len();
        self.out.extend(std::iter::repeat_n(0.0, n));
        for f in freqs {
            for i in 0..n {
                let v = (i as f32 * f / RATE as f32 * std::f32::consts::TAU).sin();
                self.out[start + i] += v * envelope(i, n) * gain / freqs.len() as f32;
            }
        }
        self
    }

    /// Filtered noise: a one-pole low-pass at `cutoff` Hz, decaying.
    fn burst(mut self, ms: u32, cutoff: f32, gain: f32) -> Synth {
        let n = samples(ms);
        let a = 1.0 - (-std::f32::consts::TAU * cutoff / RATE as f32).exp();
        let mut y = 0.0_f32;
        for i in 0..n {
            let x = self.noise();
            y += a * (x - y);
            self.out.push(y * envelope(i, n) * gain);
        }
        self
    }

    /// The clip, its peak brought to 0.8 so every cue is about as loud.
    fn done(self) -> Clip {
        let peak = self
            .out
            .iter()
            .fold(0.0_f32, |m, s| m.max(s.abs()))
            .max(1e-6);
        let k = 0.8 / peak;
        let mut samples: Vec<f32> = self.out.iter().map(|s| s * k).collect();
        if let Some(last) = samples.last_mut() {
            *last = 0.0;
        }
        Clip {
            rate: RATE,
            samples: Arc::from(samples),
        }
    }
}

fn samples(ms: u32) -> usize {
    (RATE as usize * ms as usize) / 1000
}

/// A 3 ms attack, then a decay that reaches nothing at the end.
fn envelope(i: usize, n: usize) -> f32 {
    let attack = samples(3).max(1);
    let up = (i as f32 / attack as f32).min(1.0);
    let t = i as f32 / n.max(1) as f32;
    up * (1.0 - t) * (1.0 - t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cue has its placeholder: three to five variations for an
    /// acknowledgment, at least one for the rest, every clip short,
    /// within range and not silent.
    #[test]
    fn every_cue_has_a_placeholder_within_range() {
        let lib = library();
        assert!(lib.missing().is_empty(), "{:?}", lib.missing());
        for (cue, clips) in lib.iter() {
            if let Cue::Ack(_) = cue {
                assert!((3..=5).contains(&clips.len()), "{cue:?}: {}", clips.len());
            }
            for c in clips {
                assert_eq!(c.rate, RATE);
                let ms = c.duration_ms();
                assert!((10..=3000).contains(&ms), "{cue:?}: {ms} ms");
                let peak = c.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
                assert!(peak > 0.5 && peak <= 0.8001, "{cue:?}: peak {peak}");
                assert_eq!(*c.samples.last().unwrap(), 0.0, "{cue:?} ends in silence");
            }
        }
        let a = lib.clip(Cue::Ack(Class::Villager), 0).unwrap();
        let b = lib.clip(Cue::Ack(Class::Villager), 1).unwrap();
        assert_ne!(a.samples, b.samples, "variations differ");
    }
}
