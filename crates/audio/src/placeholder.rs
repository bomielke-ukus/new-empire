//! Placeholder sounds, synthesised: a clip for every cue so the game is
//! heard before a single recording exists, the way the placeholder sprites
//! stand in for art. They are honest about what they are, short tones and
//! noise bursts with the right shape and length, so the feel of the
//! feedback (`UX-AUDIO-01`) can be judged now and the recordings dropped
//! in later without a code change (`docs/07` Q7).

use crate::score::{Bed, Layer};
use crate::{Clip, Cue, Library, Task};
use sim::{Age, Class};
use std::sync::Arc;

/// Samples per second: plenty for foley, small in memory.
pub const RATE: u32 = 22_050;

/// A clip for every cue, with the variations the spec asks for: three to
/// five per acknowledgment, two or three for the rest; and a loop for
/// every layer, the stems and the beds.
pub fn library() -> Library {
    let mut lib = Library::default();
    for cue in Cue::all() {
        lib.insert(cue, clips_for(cue));
    }
    for (layer, clip) in layers() {
        lib.insert_layer(layer, clip);
    }
    lib
}

/// A loop for every layer.
pub fn layers() -> Vec<(Layer, Clip)> {
    Layer::all()
        .into_iter()
        .map(|l| {
            let clip = match l {
                Layer::Title => title(),
                Layer::Stem(age) => stem(age),
                Layer::Combat => combat(),
                Layer::Bed(b) => bed(b),
            };
            (l, clip)
        })
        .collect()
}

fn clips_for(cue: Cue) -> Vec<Clip> {
    match cue {
        Cue::Ack(c) => (0..4).map(|v| ack(c, v, false)).collect(),
        Cue::Select(c) => (0..2).map(|v| ack(c, v, true)).collect(),
        Cue::Work(t) => (0..3).map(|v| work(t, v)).collect(),
        Cue::Hit { building } => (0..3).map(|v| hit(building, v)).collect(),
        Cue::Impact => (0..2).map(impact).collect(),
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
        Cue::Idle => vec![chime(&[1320.0], 160, 0.25)],
        // Two notes falling: the placeholder for "cannot afford".
        Cue::Poor => vec![Synth::new()
            .tone(330.0, 300.0, 90, Wave::Triangle, 0.4)
            .gap(40)
            .tone(260.0, 230.0, 150, Wave::Triangle, 0.4)
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

/// A stone coming down: a heavy crack and the ground answering.
fn impact(variation: usize) -> Clip {
    let v = 1.0 + variation as f32 * 0.08;
    Synth::new()
        .burst(70, 900.0 * v, 0.7)
        .tone(70.0 * v, 45.0, 260, Wave::Sine, 0.6)
        .done()
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

// ----- the score and the beds -----------------------------------------------

/// Pentatonic on A: the notes the placeholder tunes are made of.
const A3: f32 = 220.0;
const C4: f32 = 261.63;
const D4: f32 = 293.66;
const E4: f32 = 329.63;
const G4: f32 = 392.0;
const A4: f32 = 440.0;

/// A note by its MIDI number: 69 is A4, 440 Hz.
fn hz(midi: i32) -> f32 {
    440.0 * 2f32.powf((midi - 69) as f32 / 12.0)
}

/// Each age's stem is its own piece: its own tempo, mode and tune, so an
/// age reached is heard as well as seen (`docs/05` §5.3). The band grows
/// with the player: the Stone Age is a frame drum and a bone flute; the
/// Tool Age brings the lyre and a shaker; the Bronze Age a horn over the
/// low chorus; the Iron Age war drums, a snare and the horn high and loud.
fn stem(age: Age) -> Clip {
    match age {
        Age::Stone => stone_stem(),
        Age::Tool => tool_stem(),
        Age::Bronze => bronze_stem(),
        Age::Iron => iron_stem(),
    }
}

/// Sixty beats a minute in A minor pentatonic: the drum on the twos, the
/// flute's phrase, a long rest, its answer. Sparse.
fn stone_stem() -> Clip {
    let mut l = Loop::new(16_000);
    for beat in (0..16u32).step_by(2) {
        l.drum(beat * 1000, 0.9);
    }
    let phrase = [
        (1000, A4, 600),
        (1700, G4, 500),
        (2300, E4, 900),
        (3500, D4, 600),
        (4200, C4, 1200),
        (9000, E4, 600),
        (9700, G4, 500),
        (10_300, A4, 1400),
    ];
    for (at, f, ms) in phrase {
        l.flute(at, f, ms, 0.5);
    }
    l.finish(0.5)
}

/// Eighty beats a minute in D dorian, lighter on its feet: the lyre picks
/// out a running figure, the flute sings over it, a shaker ticks the
/// off-beats and the drum walks.
fn tool_stem() -> Clip {
    const BEAT: u32 = 750;
    let mut l = Loop::new(32 * BEAT);
    for beat in 0..32u32 {
        let at = beat * BEAT;
        match beat % 4 {
            0 => l.drum(at, 0.9),
            2 => l.drum(at, 0.6),
            3 => l.drum(at + BEAT / 2, 0.4),
            _ => {}
        }
        l.shaker(at + BEAT / 2, 0.35);
    }
    // The lyre's figure, two bars long, round the chords of D dorian.
    let figure = [
        [62, 69, 74, 69],
        [60, 67, 72, 67],
        [65, 69, 72, 69],
        [64, 67, 71, 67],
    ];
    for bar in 0..8u32 {
        for (k, &n) in figure[(bar % 4) as usize].iter().enumerate() {
            l.pluck(bar * 4 * BEAT + k as u32 * BEAT, hz(n), 0.4);
            l.pluck(
                bar * 4 * BEAT + k as u32 * BEAT + BEAT / 2,
                hz(n + 12),
                0.15,
            );
        }
    }
    // The tune: up through the mode, a turn, home.
    let tune = [
        (0, 69, 2),
        (2, 72, 1),
        (3, 74, 1),
        (4, 76, 2),
        (6, 74, 1),
        (7, 72, 1),
        (8, 71, 3),
        (12, 72, 1),
        (13, 71, 1),
        (14, 69, 2),
        (16, 67, 2),
        (18, 69, 1),
        (19, 71, 1),
        (20, 72, 2),
        (22, 71, 1),
        (23, 67, 1),
        (24, 69, 4),
    ];
    for (beat, n, beats) in tune {
        l.flute(beat * BEAT, hz(n), beats * BEAT - 60, 0.45);
    }
    l.finish(0.5)
}

/// Ninety-six beats a minute in G mixolydian, a procession: the drum on
/// every beat with the weight on one and three, a bass walking the roots,
/// the chorus holding the fifth, and a horn with the tune.
fn bronze_stem() -> Clip {
    const BEAT: u32 = 625;
    let mut l = Loop::new(32 * BEAT);
    l.drone(&[hz(43), hz(43) * 1.004, hz(50)], Wave::Saw, 380.0, 0.12);
    for beat in 0..32u32 {
        let at = beat * BEAT;
        l.drum(at, if beat % 2 == 0 { 0.9 } else { 0.45 });
    }
    // The bass: root, fifth, the flat seventh, the fifth, a bar each.
    let roots = [43, 50, 41, 50, 43, 48, 50, 43];
    for (bar, &n) in roots.iter().enumerate() {
        let at = bar as u32 * 4 * BEAT;
        l.bass(at, hz(n), 2 * BEAT - 40, 0.5);
        l.bass(at + 2 * BEAT, hz(n), BEAT - 40, 0.35);
        l.bass(at + 3 * BEAT, hz(n + 7), BEAT - 40, 0.35);
    }
    let tune = [
        (0, 67, 2),
        (2, 71, 2),
        (4, 74, 3),
        (7, 72, 1),
        (8, 71, 2),
        (10, 69, 2),
        (12, 65, 4),
        (16, 67, 2),
        (18, 69, 1),
        (19, 71, 1),
        (20, 72, 2),
        (22, 74, 2),
        (24, 71, 2),
        (26, 69, 2),
        (28, 67, 4),
    ];
    for (beat, n, beats) in tune {
        l.horn(beat * BEAT, hz(n), beats * BEAT - 50, 0.4);
    }
    // The lyre answers in the long notes.
    for (beat, n) in [(13, 74), (14, 72), (15, 69), (29, 74), (30, 71), (31, 67)] {
        l.pluck(beat * BEAT, hz(n), 0.3);
    }
    l.finish(0.5)
}

/// A hundred and twelve beats a minute in C harmonic minor, to war: the
/// drums doubled on every beat, a snare on two and four, a low saw bass
/// hammering the root, and the horn high over it all with the raised
/// seventh's bite.
fn iron_stem() -> Clip {
    const BEAT: u32 = 536;
    let mut l = Loop::new(32 * BEAT);
    l.drone(&[hz(36), hz(43)], Wave::Saw, 300.0, 0.1);
    for beat in 0..32u32 {
        let at = beat * BEAT;
        l.drum(at, 1.0);
        l.drum(at + BEAT / 2, 0.5);
        if beat % 2 == 1 {
            l.snare(at, 0.5);
        }
    }
    let ostinato = [36, 36, 43, 44];
    for beat in 0..32u32 {
        let n = ostinato[(beat % 4) as usize];
        l.bass(beat * BEAT, hz(n), BEAT - 60, 0.45);
    }
    let tune = [
        (0, 72, 2),
        (2, 75, 1),
        (3, 79, 1),
        (4, 77, 2),
        (6, 75, 1),
        (7, 74, 1),
        (8, 72, 2),
        (10, 71, 2),
        (12, 72, 4),
        (16, 79, 2),
        (18, 80, 1),
        (19, 79, 1),
        (20, 77, 2),
        (22, 75, 2),
        (24, 74, 1),
        (25, 75, 1),
        (26, 71, 2),
        (28, 72, 4),
    ];
    for (beat, n, beats) in tune {
        l.horn(beat * BEAT, hz(n), beats * BEAT - 40, 0.45);
        l.horn(beat * BEAT, hz(n - 12), beats * BEAT - 40, 0.2);
    }
    l.finish(0.5)
}

/// The title theme: twenty-four seconds, slower and wider than any age's
/// stem. A low drone of the chorus, a heartbeat on the frame drum, the bone
/// flute's long phrase rising and falling over it, the lyre answering.
fn title() -> Clip {
    let mut l = Loop::new(24_000);
    l.drone(
        &[A3 / 2.0, A3 / 2.0 * 1.003, E4 / 2.0],
        Wave::Saw,
        350.0,
        0.1,
    );
    for beat in (0..24_000u32).step_by(3000) {
        l.drum(beat, 0.55);
        l.drum(beat + 400, 0.3);
    }
    let phrase = [
        (1500, E4, 1400),
        (3000, G4, 900),
        (4000, A4, 2200),
        (6600, G4, 700),
        (7400, E4, 700),
        (8200, D4, 2400),
        (13_500, C4, 900),
        (14_500, D4, 900),
        (15_500, E4, 1800),
        (17_600, A3, 2800),
    ];
    for (at, f, ms) in phrase {
        l.flute(at, f, ms, 0.45);
    }
    let lyre = [A3, C4, E4, A4, E4, C4];
    for (k, at) in (10_800..13_200).step_by(400).enumerate() {
        l.pluck(at, lyre[k % lyre.len()], 0.3);
    }
    for (k, at) in (20_600..23_400).step_by(450).enumerate() {
        l.pluck(at, lyre[(k + 2) % lyre.len()], 0.25);
    }
    l.finish(0.5)
}

/// The combat stem: eight seconds of fast drums and a pulse under them.
fn combat() -> Clip {
    let mut l = Loop::new(8000);
    let beat = 8000 / 19;
    for n in 0..19u32 {
        let at = n * beat;
        l.drum(at, if n.is_multiple_of(4) { 1.0 } else { 0.5 });
        l.burst(at + beat / 2, 40, 2500.0, 0.25);
    }
    l.drone(&[55.0, 55.3], Wave::Square, 200.0, 0.2);
    l.finish(0.5)
}

/// A bed: eight seconds of the ground's own noise, low.
fn bed(bed: Bed) -> Clip {
    let mut l = Loop::new(8000);
    match bed {
        Bed::Forest => {
            l.noise_bed(600.0, 0.15, &[0.13]);
            for (i, at) in [300, 1900, 2600, 4400, 5100, 6800].into_iter().enumerate() {
                let f = 2400.0 + (i % 3) as f32 * 350.0;
                l.tone(at, f, f * 1.3, 90, Wave::Sine, 0.25);
                l.tone(at + 130, f * 1.2, f, 70, Wave::Sine, 0.2);
            }
        }
        Bed::Surf => l.noise_bed(400.0, 0.5, &[0.125]),
        Bed::Wind => l.noise_bed(1000.0, 0.4, &[0.11, 0.29]),
        Bed::Field => {
            l.noise_bed(300.0, 0.12, &[0.07]);
            l.tone(3000, 2800.0, 3400.0, 80, Wave::Sine, 0.12);
        }
    }
    l.finish(0.5)
}

/// A fixed-length buffer that sounds are laid onto, wrapping at the end
/// so a tail runs on into the loop's start; finished with a cross-fade
/// across the seam, so it loops without a click.
struct Loop {
    out: Vec<f32>,
    seed: u64,
}

impl Loop {
    fn new(ms: u32) -> Loop {
        Loop {
            out: vec![0.0; samples(ms)],
            seed: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// Lays `v` onto the loop from `at_ms`, wrapping.
    fn lay(&mut self, at_ms: u32, v: &[f32], gain: f32) {
        let n = self.out.len();
        let start = samples(at_ms);
        for (i, s) in v.iter().enumerate() {
            self.out[(start + i) % n] += s * gain;
        }
    }

    fn tone(&mut self, at_ms: u32, from: f32, to: f32, ms: u32, wave: Wave, gain: f32) {
        let v = tone_samples(from, to, ms, wave);
        self.lay(at_ms, &v, gain);
    }

    fn burst(&mut self, at_ms: u32, ms: u32, cutoff: f32, gain: f32) {
        let v = burst_samples(&mut self.seed, ms, cutoff);
        self.lay(at_ms, &v, gain);
    }

    /// The frame drum: a thud with a little skin noise.
    fn drum(&mut self, at_ms: u32, gain: f32) {
        self.tone(at_ms, 90.0, 50.0, 90, Wave::Sine, 0.8 * gain);
        self.burst(at_ms, 50, 180.0, 0.5 * gain);
    }

    /// The bone flute: a sine with breath on it.
    fn flute(&mut self, at_ms: u32, f: f32, ms: u32, gain: f32) {
        self.tone(at_ms, f, f * 1.003, ms, Wave::Sine, gain);
        self.burst(at_ms, ms, 3000.0, 0.04 * gain);
    }

    /// The lyre: a plucked string, gone in half a second.
    fn pluck(&mut self, at_ms: u32, f: f32, gain: f32) {
        self.tone(at_ms, f, f * 0.995, 450, Wave::Triangle, gain);
    }

    /// A horn: two saws a breath apart, swelling in, held, low-passed so
    /// it is brass and not a buzz.
    fn horn(&mut self, at_ms: u32, f: f32, ms: u32, gain: f32) {
        let v = held_samples(&[f, f * 1.004], ms, Wave::Saw, 1600.0, 70);
        self.lay(at_ms, &v, gain);
    }

    /// A bass note: a triangle held for its length, dark.
    fn bass(&mut self, at_ms: u32, f: f32, ms: u32, gain: f32) {
        let v = held_samples(&[f], ms, Wave::Triangle, 700.0, 15);
        self.lay(at_ms, &v, gain);
    }

    /// A shaker: a hiss, gone at once.
    fn shaker(&mut self, at_ms: u32, gain: f32) {
        self.burst(at_ms, 35, 7000.0, gain);
    }

    /// A snare: a crack of noise over a short knock.
    fn snare(&mut self, at_ms: u32, gain: f32) {
        self.burst(at_ms, 110, 2600.0, gain);
        self.tone(at_ms, 190.0, 150.0, 60, Wave::Sine, 0.5 * gain);
    }

    /// A sustained chord under the whole loop, low-passed, each
    /// frequency rounded to whole cycles so the seam is silent.
    fn drone(&mut self, freqs: &[f32], wave: Wave, cutoff: f32, gain: f32) {
        let n = self.out.len();
        let seconds = n as f32 / RATE as f32;
        let a = 1.0 - (-std::f32::consts::TAU * cutoff / RATE as f32).exp();
        for f in freqs {
            let f = (f * seconds).round() / seconds;
            let mut y = 0.0_f32;
            for (i, out) in self.out.iter_mut().enumerate() {
                let phase = (i as f32 * f / RATE as f32).fract();
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
                y += a * (raw - y);
                *out += y * gain / freqs.len() as f32;
            }
        }
    }

    /// Filtered noise over the whole loop, its level wandering with the
    /// slow waves given in hertz.
    fn noise_bed(&mut self, cutoff: f32, gain: f32, lfo_hz: &[f32]) {
        let n = self.out.len();
        let a = 1.0 - (-std::f32::consts::TAU * cutoff / RATE as f32).exp();
        let mut y = 0.0_f32;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let x = noise(&mut self.seed);
            y += a * (x - y);
            let wander: f32 = lfo_hz
                .iter()
                .map(|f| (t * f * std::f32::consts::TAU).sin())
                .sum::<f32>()
                / lfo_hz.len() as f32;
            self.out[i] += y * gain * (0.55 + 0.45 * wander);
        }
    }

    /// The clip: the last tenth of a second cross-faded into the first so
    /// the seam is silent, then the peak brought to `peak`.
    fn finish(mut self, peak: f32) -> Clip {
        let x = samples(100).min(self.out.len() / 4);
        let n = self.out.len();
        let tail: Vec<f32> = self.out[n - x..].to_vec();
        for (i, t) in tail.iter().enumerate() {
            let w = i as f32 / x as f32;
            self.out[i] = self.out[i] * w + t * (1.0 - w);
        }
        self.out.truncate(n - x);
        let top = self
            .out
            .iter()
            .fold(0.0_f32, |m, s| m.max(s.abs()))
            .max(1e-6);
        let k = peak / top;
        Clip {
            rate: RATE,
            samples: Arc::from(self.out.iter().map(|s| s * k).collect::<Vec<_>>()),
        }
    }
}

/// White noise in -1..1 from a generator of the caller's.
fn noise(seed: &mut u64) -> f32 {
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *seed = x;
    (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// A tone sliding from `from` to `to` Hz with the standard envelope.
fn tone_samples(from: f32, to: f32, ms: u32, wave: Wave) -> Vec<f32> {
    let n = samples(ms);
    let mut phase = 0.0_f32;
    (0..n)
        .map(|i| {
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
            raw * envelope(i, n)
        })
        .collect()
}

/// A held note: the frequencies summed, swelling in over `attack_ms`,
/// held, and let go over the last tenth, through a one-pole low-pass at
/// `cutoff` Hz.
fn held_samples(freqs: &[f32], ms: u32, wave: Wave, cutoff: f32, attack_ms: u32) -> Vec<f32> {
    let n = samples(ms);
    let attack = samples(attack_ms).max(1);
    let release = (n / 10).max(1);
    let a = 1.0 - (-std::f32::consts::TAU * cutoff / RATE as f32).exp();
    let mut phases = vec![0.0_f32; freqs.len()];
    let mut y = 0.0_f32;
    (0..n)
        .map(|i| {
            let mut raw = 0.0;
            for (p, f) in phases.iter_mut().zip(freqs) {
                *p = (*p + f / RATE as f32) % 1.0;
                raw += match wave {
                    Wave::Sine => (*p * std::f32::consts::TAU).sin(),
                    Wave::Square => {
                        if *p < 0.5 {
                            0.6
                        } else {
                            -0.6
                        }
                    }
                    Wave::Triangle => 1.0 - 4.0 * (*p - 0.5).abs(),
                    Wave::Saw => 2.0 * *p - 1.0,
                };
            }
            y += a * (raw / freqs.len() as f32 - y);
            let up = (i as f32 / attack as f32).min(1.0);
            let down = ((n - i) as f32 / release as f32).min(1.0);
            y * up * down
        })
        .collect()
}

/// Filtered noise with the standard envelope.
fn burst_samples(seed: &mut u64, ms: u32, cutoff: f32) -> Vec<f32> {
    let n = samples(ms);
    let a = 1.0 - (-std::f32::consts::TAU * cutoff / RATE as f32).exp();
    let mut y = 0.0_f32;
    (0..n)
        .map(|i| {
            let x = noise(seed);
            y += a * (x - y);
            y * envelope(i, n)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every layer has its loop, seconds long, under half scale, and
    /// seamless: the seam's two sides are within a whisker of each other.
    #[test]
    fn every_layer_has_a_seamless_loop() {
        let lib = library();
        assert!(
            lib.missing_layers().is_empty(),
            "{:?}",
            lib.missing_layers()
        );
        for l in Layer::all() {
            let c = lib.layer(l).unwrap();
            let ms = c.duration_ms();
            assert!((4000..=30_000).contains(&ms), "{l:?}: {ms} ms");
            let peak = c.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.3 && peak <= 0.5001, "{l:?}: peak {peak}");
            let (first, last) = (c.samples[0], *c.samples.last().unwrap());
            assert!((first - last).abs() < 0.08, "{l:?}: seam {first} to {last}");
        }
        // Each age is its own piece, down to its length: a different tempo
        // and tune, not the same loop with another layer on it.
        let lengths: Vec<u64> = Age::ALL
            .iter()
            .map(|&a| lib.layer(Layer::Stem(a)).unwrap().duration_ms())
            .collect();
        for (i, a) in lengths.iter().enumerate() {
            for b in &lengths[i + 1..] {
                assert_ne!(a, b, "the ages differ: {lengths:?}");
            }
        }
    }

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
