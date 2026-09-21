//! Sound, as the game asks for it (`docs/03` §6.1, `docs/04` §8, `docs/05`
//! §5). The game raises a [`Cue`]: a unit heard an order, an axe landed, a
//! building fell, a button was pressed. The [`Mixer`] decides what plays:
//! which of the cue's variations, never the same one twice running; whether
//! it plays at all, since four of one sound at once is the ceiling
//! (`TA-AUDIO-01`); how loud and where, from the listener's view of the
//! world; and at what pitch, a little different every time, so twelve
//! villagers chopping are a texture and not a machine gun (`UX-AUDIO-02`).
//!
//! Nothing here touches a device. The app owns the device and plays what
//! the mixer hands it; a test records it instead. The cues come from the
//! simulation's events (`TA-AUDIO-02`), mapped in [`events`], seen through
//! the listener's fog: nothing plays for what the fog hides.
//!
//! Every cue has a placeholder clip, synthesised in [`placeholder`], the
//! way the placeholder sprites stand in for art until the real sets land.
//! A real recording under `assets/sounds/<cue>/` replaces it by name.

pub mod events;
pub mod placeholder;
pub mod score;

use sim::{Age, Class};
use std::sync::Arc;

pub use score::{Ambience, Bed, Fade, Ground, Layer, Score};
pub use sim::Task;

/// The four buses (`docs/04` §8), each with its own volume.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bus {
    /// Clicks, the invalid buzz, the bell, the fanfares.
    Ui,
    /// Acknowledgments and selection: the units answering.
    Voice,
    /// The world, positional: work, fighting, building, falling.
    World,
    /// The stems and the ambient beds.
    Music,
}

impl Bus {
    /// Every bus, in the order the settings screen lists them.
    pub const ALL: [Bus; 4] = [Bus::Ui, Bus::Voice, Bus::World, Bus::Music];

    /// The name on the settings screen.
    pub const fn name(self) -> &'static str {
        match self {
            Bus::Ui => "UI",
            Bus::Voice => "VOICES",
            Bus::World => "WORLD",
            Bus::Music => "MUSIC",
        }
    }

    /// Index into a per-bus table.
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A sound the game asks for. Which clip plays, how loud and where is
/// the mixer's business, not the caller's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cue {
    /// A unit of the class heard an order: the bark (`docs/03` §6.1).
    Ack(Class),
    /// A unit of the class was selected.
    Select(Class),
    /// A villager's work swing.
    Work(Task),
    /// A hit landed, on a unit or on a building.
    Hit {
        /// Whether the target is a building.
        building: bool,
    },
    /// A unit of the class died; a building of the class fell.
    Death(Class),
    /// A building finished.
    Completed,
    /// A unit stepped out of the building that trained it.
    Trained,
    /// A load reached the stockpile.
    Deposited,
    /// A technology finished.
    Research,
    /// An age reached.
    Fanfare(Age),
    /// A button.
    Click,
    /// A refused action.
    Invalid,
    /// The bell: the side is under attack.
    Alarm,
    /// Something of the side's is gone.
    Loss,
}

/// The classes a unit that answers can be.
const VOICED: [Class; 5] = [
    Class::Villager,
    Class::Infantry,
    Class::Ranged,
    Class::Cavalry,
    Class::Siege,
];

/// The tasks a villager swings at.
pub const TASKS: [Task; 5] = [
    Task::Chop,
    Task::Mine,
    Task::Forage,
    Task::Farm,
    Task::Build,
];

impl Cue {
    /// Every cue the game can raise, in a stable order.
    pub fn all() -> Vec<Cue> {
        let mut out = Vec::new();
        for c in VOICED {
            out.push(Cue::Ack(c));
        }
        for c in VOICED {
            out.push(Cue::Select(c));
        }
        for t in TASKS {
            out.push(Cue::Work(t));
        }
        out.push(Cue::Hit { building: false });
        out.push(Cue::Hit { building: true });
        for c in VOICED {
            out.push(Cue::Death(c));
        }
        out.push(Cue::Death(Class::Building));
        out.extend([Cue::Completed, Cue::Trained, Cue::Deposited, Cue::Research]);
        for a in Age::ALL {
            out.push(Cue::Fanfare(a));
        }
        out.extend([Cue::Click, Cue::Invalid, Cue::Alarm, Cue::Loss]);
        out
    }

    /// The bus it plays on.
    pub const fn bus(self) -> Bus {
        match self {
            Cue::Ack(_) | Cue::Select(_) => Bus::Voice,
            Cue::Work(_)
            | Cue::Hit { .. }
            | Cue::Death(_)
            | Cue::Completed
            | Cue::Trained
            | Cue::Deposited => Bus::World,
            Cue::Research
            | Cue::Fanfare(_)
            | Cue::Click
            | Cue::Invalid
            | Cue::Alarm
            | Cue::Loss => Bus::Ui,
        }
    }

    /// Whether the pitch varies from play to play. A fanfare is a tune,
    /// and a tune in a different key each time is wrong.
    pub const fn varies(self) -> bool {
        !matches!(self, Cue::Fanfare(_))
    }

    /// The name of its folder under `assets/sounds`: `ack-villager`,
    /// `work-chop`, `fanfare-tool`.
    pub fn name(self) -> String {
        match self {
            Cue::Ack(c) => format!("ack-{}", class_name(c)),
            Cue::Select(c) => format!("select-{}", class_name(c)),
            Cue::Work(t) => format!("work-{}", task_name(t)),
            Cue::Hit { building: false } => "hit-unit".to_string(),
            Cue::Hit { building: true } => "hit-building".to_string(),
            Cue::Death(c) => format!("death-{}", class_name(c)),
            Cue::Completed => "completed".to_string(),
            Cue::Trained => "trained".to_string(),
            Cue::Deposited => "deposited".to_string(),
            Cue::Research => "research".to_string(),
            Cue::Fanfare(a) => format!("fanfare-{}", age_name(a)),
            Cue::Click => "click".to_string(),
            Cue::Invalid => "invalid".to_string(),
            Cue::Alarm => "alarm".to_string(),
            Cue::Loss => "loss".to_string(),
        }
    }

    /// The cue with that folder name, if any.
    pub fn from_name(name: &str) -> Option<Cue> {
        Cue::all().into_iter().find(|c| c.name() == name)
    }
}

fn class_name(c: Class) -> &'static str {
    match c {
        Class::Other => "other",
        Class::Villager => "villager",
        Class::Infantry => "infantry",
        Class::Ranged => "ranged",
        Class::Cavalry => "cavalry",
        Class::Siege => "siege",
        Class::Building => "building",
        Class::Animal => "animal",
    }
}

pub(crate) fn age_name(a: Age) -> &'static str {
    match a {
        Age::Stone => "stone",
        Age::Tool => "tool",
        Age::Bronze => "bronze",
        Age::Iron => "iron",
    }
}

fn task_name(t: Task) -> &'static str {
    match t {
        Task::Chop => "chop",
        Task::Mine => "mine",
        Task::Forage => "forage",
        Task::Farm => "farm",
        Task::Build => "build",
    }
}

/// A mono clip.
#[derive(Clone, Debug)]
pub struct Clip {
    /// Samples per second.
    pub rate: u32,
    /// The samples, in -1..=1.
    pub samples: Arc<[f32]>,
}

impl Clip {
    /// How long it plays at pitch 1.
    pub fn duration_ms(&self) -> u64 {
        (self.samples.len() as u64 * 1000) / self.rate.max(1) as u64
    }
}

/// The clips for every cue, one or more variations each, and a loop for
/// every layer.
#[derive(Clone, Debug, Default)]
pub struct Library {
    clips: Vec<(Cue, Vec<Clip>)>,
    layers: Vec<(Layer, Clip)>,
}

impl Library {
    /// Sets the variations of a cue, replacing any it had.
    pub fn insert(&mut self, cue: Cue, clips: Vec<Clip>) {
        self.clips.retain(|(c, _)| *c != cue);
        if !clips.is_empty() {
            self.clips.push((cue, clips));
        }
    }

    /// How many variations a cue has; none means it never plays.
    pub fn variants(&self, cue: Cue) -> usize {
        self.clips
            .iter()
            .find(|(c, _)| *c == cue)
            .map_or(0, |(_, v)| v.len())
    }

    /// One variation of a cue.
    pub fn clip(&self, cue: Cue, variant: usize) -> Option<&Clip> {
        self.clips
            .iter()
            .find(|(c, _)| *c == cue)
            .and_then(|(_, v)| v.get(variant))
    }

    /// Every cue that has a clip, with its variations.
    pub fn iter(&self) -> impl Iterator<Item = (Cue, &[Clip])> {
        self.clips.iter().map(|(c, v)| (*c, v.as_slice()))
    }

    /// The cues that have nothing to play.
    pub fn missing(&self) -> Vec<Cue> {
        Cue::all()
            .into_iter()
            .filter(|c| self.variants(*c) == 0)
            .collect()
    }

    /// Sets a layer's loop.
    pub fn insert_layer(&mut self, layer: Layer, clip: Clip) {
        self.layers.retain(|(l, _)| *l != layer);
        self.layers.push((layer, clip));
    }

    /// A layer's loop, if it has one.
    pub fn layer(&self, layer: Layer) -> Option<&Clip> {
        self.layers
            .iter()
            .find(|(l, _)| *l == layer)
            .map(|(_, c)| c)
    }

    /// The layers that have nothing to play.
    pub fn missing_layers(&self) -> Vec<Layer> {
        Layer::all()
            .into_iter()
            .filter(|l| self.layer(*l).is_none())
            .collect()
    }
}

/// Where the listener is: the camera's focus in world-screen pixels, and
/// how far the view reaches from it, so a sound at the edge of the screen
/// is at the edge of the stereo field and one beyond it is quieter.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Listener {
    /// The centre of the view, world-screen.
    pub focus: (f32, f32),
    /// Half the view's width and height, world-screen.
    pub half: (f32, f32),
}

impl Default for Listener {
    fn default() -> Listener {
        Listener {
            focus: (0.0, 0.0),
            half: (640.0, 360.0),
        }
    }
}

/// One sound to play now: the mixer's answer to a cue.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Play {
    /// What was asked for.
    pub cue: Cue,
    /// Which of its variations.
    pub variant: usize,
    /// On which bus.
    pub bus: Bus,
    /// 0 to 1, from where it is relative to the listener; the bus volume
    /// is on top of this.
    pub gain: f32,
    /// -1 (left) to 1 (right).
    pub pan: f32,
    /// The playback rate: 1 is the clip as recorded.
    pub rate: f32,
}

/// The most instances of one cue that play at once (`TA-AUDIO-01`).
pub const MAX_VOICES: usize = 4;
/// The pitch varies by up to this fraction each way (`TA-AUDIO-01`).
pub const PITCH_VARIATION: f32 = 0.05;
/// A world sound off the screen is attenuated to no lower than this: the
/// economy is heard running, not silenced (`docs/03` §6.1).
pub const OFFSCREEN_FLOOR: f32 = 0.2;
/// How many screen half-widths out the attenuation reaches the floor.
pub const OFFSCREEN_REACH: f32 = 2.0;
/// How wide the stereo field is at the edge of the screen.
const PAN_WIDTH: f32 = 0.75;

/// Decides what plays. Time is milliseconds on the caller's clock, wall
/// time in the app: a voice is busy for the length of its clip whatever
/// the match clock does.
#[derive(Clone, Debug)]
pub struct Mixer {
    library: Library,
    listener: Listener,
    volumes: [f32; 4],
    /// The last variation each cue played, so the next differs.
    last: Vec<(Cue, usize)>,
    /// The cues playing and when each ends.
    voices: Vec<(Cue, u64)>,
    rng: u64,
}

impl Mixer {
    /// A mixer over a library, every bus at full volume.
    pub fn new(library: Library) -> Mixer {
        Mixer {
            library,
            listener: Listener::default(),
            volumes: [1.0; 4],
            last: Vec::new(),
            voices: Vec::new(),
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// The clips.
    pub fn library(&self) -> &Library {
        &self.library
    }

    /// Where the listener is now.
    pub fn set_listener(&mut self, listener: Listener) {
        self.listener = listener;
    }

    /// The listener.
    pub fn listener(&self) -> Listener {
        self.listener
    }

    /// A bus's volume, 0 to 1.
    pub fn set_volume(&mut self, bus: Bus, volume: f32) {
        self.volumes[bus.index()] = volume.clamp(0.0, 1.0);
    }

    /// A bus's volume.
    pub fn volume(&self, bus: Bus) -> f32 {
        self.volumes[bus.index()]
    }

    /// How many of a cue are playing at `now`.
    pub fn playing(&self, cue: Cue, now_ms: u64) -> usize {
        self.voices
            .iter()
            .filter(|(c, end)| *c == cue && *end > now_ms)
            .count()
    }

    /// Asks for a cue, from `at` in world-screen pixels or from nowhere
    /// in particular. What to play, if the cue has a clip and fewer than
    /// [`MAX_VOICES`] of it are already playing.
    pub fn cue(&mut self, cue: Cue, at: Option<(f32, f32)>, now_ms: u64) -> Option<Play> {
        let n = self.library.variants(cue);
        if n == 0 {
            return None;
        }
        self.voices.retain(|(_, end)| *end > now_ms);
        if self.voices.iter().filter(|(c, _)| *c == cue).count() >= MAX_VOICES {
            return None;
        }
        let variant = self.pick(cue, n);
        let (gain, pan) = at.map_or((1.0, 0.0), |p| self.place(p));
        let rate = if cue.varies() {
            1.0 + (self.unit() * 2.0 - 1.0) * PITCH_VARIATION
        } else {
            1.0
        };
        let clip = self.library.clip(cue, variant)?;
        let length = (clip.duration_ms() as f32 / rate) as u64 + 1;
        self.voices.push((cue, now_ms + length));
        Some(Play {
            cue,
            variant,
            bus: cue.bus(),
            gain,
            pan,
            rate,
        })
    }

    /// A variation that is not the one last played, at random among the
    /// rest: round-robin with a shuffle, never twice in a row.
    fn pick(&mut self, cue: Cue, n: usize) -> usize {
        let last = self.last.iter().find(|(c, _)| *c == cue).map(|(_, v)| *v);
        let variant = match (n, last) {
            (1, _) => 0,
            (_, None) => (self.unit() * n as f32) as usize % n,
            (_, Some(l)) => {
                let k = (self.unit() * (n - 1) as f32) as usize % (n - 1);
                if k >= l {
                    k + 1
                } else {
                    k
                }
            }
        };
        match self.last.iter_mut().find(|(c, _)| *c == cue) {
            Some(slot) => slot.1 = variant,
            None => self.last.push((cue, variant)),
        }
        variant
    }

    /// Gain and pan for a world-screen point: full and centred within the
    /// view, panned to the side it is on, quieter beyond the edge down to
    /// [`OFFSCREEN_FLOOR`] at [`OFFSCREEN_REACH`] half-widths out.
    fn place(&self, at: (f32, f32)) -> (f32, f32) {
        let l = self.listener;
        let (hw, hh) = (l.half.0.max(1.0), l.half.1.max(1.0));
        let (dx, dy) = ((at.0 - l.focus.0) / hw, (at.1 - l.focus.1) / hh);
        let out = dx.abs().max(dy.abs());
        let gain = if out <= 1.0 {
            1.0
        } else {
            let t = ((out - 1.0) / (OFFSCREEN_REACH - 1.0)).min(1.0);
            1.0 + (OFFSCREEN_FLOOR - 1.0) * t
        };
        (gain, dx.clamp(-1.0, 1.0) * PAN_WIDTH)
    }

    /// A number in 0..1, from a generator of the mixer's own: the sim's
    /// randomness is never touched by the presentation.
    fn unit(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        placeholder::library()
    }

    /// At most four of one sound play at once, each a little different in
    /// pitch, and a cue with several variations never plays the same one
    /// twice running. A fifth chop at the same moment is dropped; once
    /// the first four end, chops play again.
    ///
    /// REQ: TA-AUDIO-01
    /// REQ: UX-AUDIO-02
    #[test]
    fn four_voices_per_sound_with_varied_pitch_and_no_repeat() {
        let mut m = Mixer::new(library());
        let chop = Cue::Work(Task::Chop);
        let plays: Vec<Play> = (0..12).filter_map(|_| m.cue(chop, None, 0)).collect();
        assert_eq!(plays.len(), MAX_VOICES, "twelve villagers, four voices");
        assert_eq!(m.playing(chop, 0), MAX_VOICES);
        for p in &plays {
            assert!((p.rate - 1.0).abs() <= PITCH_VARIATION + 1e-6, "{}", p.rate);
            assert_eq!(p.bus, Bus::World);
        }
        assert!(
            plays.iter().any(|p| (p.rate - plays[0].rate).abs() > 1e-4),
            "the pitches differ"
        );
        let longest = m.library().clip(chop, 0).unwrap().duration_ms() * 2;
        assert!(m.cue(chop, None, 1).is_none(), "still four playing");
        assert!(m.cue(chop, None, longest).is_some(), "they have ended");
        // Round-robin, never the same twice running.
        let ack = Cue::Ack(Class::Villager);
        assert!(m.library().variants(ack) >= 3, "an acknowledgment has 3-5");
        let mut last = None;
        for i in 0..40u64 {
            let p = m
                .cue(ack, None, 10_000 + i * 5_000)
                .expect("spaced out, so it plays");
            assert_ne!(Some(p.variant), last, "play {i}");
            last = Some(p.variant);
        }
        let fanfare = m.cue(Cue::Fanfare(Age::Tool), None, 500_000).unwrap();
        assert_eq!(fanfare.rate, 1.0, "a tune keeps its key");
    }

    /// A world sound in view plays at full gain, panned to its side; one
    /// past the edge is quieter but never silent; a cue from nowhere is
    /// centred and full.
    #[test]
    fn world_sounds_are_placed_and_off_screen_ones_are_quiet_not_silent() {
        let mut m = Mixer::new(library());
        m.set_listener(Listener {
            focus: (1000.0, 500.0),
            half: (640.0, 360.0),
        });
        let hit = Cue::Hit { building: false };
        let centre = m.cue(hit, Some((1000.0, 500.0)), 0).unwrap();
        assert_eq!((centre.gain, centre.pan), (1.0, 0.0));
        let right = m.cue(hit, Some((1500.0, 500.0)), 0).unwrap();
        assert_eq!(right.gain, 1.0, "still on screen");
        assert!(right.pan > 0.4 && right.pan <= PAN_WIDTH, "{}", right.pan);
        let left_off = m.cue(hit, Some((0.0, 500.0)), 0).unwrap();
        assert!(
            left_off.gain < 1.0 && left_off.gain > OFFSCREEN_FLOOR,
            "{}",
            left_off.gain
        );
        assert_eq!(left_off.pan, -PAN_WIDTH);
        let far = m.cue(hit, Some((1000.0, 500_000.0)), 10_000).unwrap();
        assert!(
            (far.gain - OFFSCREEN_FLOOR).abs() < 1e-5,
            "quiet, not silent: {}",
            far.gain
        );
        let nowhere = m.cue(Cue::Click, None, 20_000).unwrap();
        assert_eq!(
            (nowhere.gain, nowhere.pan, nowhere.bus),
            (1.0, 0.0, Bus::Ui)
        );
        m.set_volume(Bus::Music, 1.7);
        assert_eq!(m.volume(Bus::Music), 1.0, "clamped");
        let mut empty = Mixer::new(Library::default());
        assert!(empty.cue(Cue::Click, None, 0).is_none(), "nothing to play");
    }

    /// Every cue has a folder name that reads back to it, on the bus the
    /// spec puts it on.
    #[test]
    fn every_cue_has_a_name_that_reads_back() {
        let all = Cue::all();
        assert!(all.len() > 30);
        for c in &all {
            assert_eq!(Cue::from_name(&c.name()), Some(*c), "{c:?}");
            assert!(!c.name().contains(' '));
        }
        assert_eq!(Cue::Ack(Class::Villager).name(), "ack-villager");
        assert_eq!(Cue::Fanfare(Age::Bronze).name(), "fanfare-bronze");
        assert_eq!(Cue::Work(Task::Build).bus(), Bus::World);
        assert_eq!(Cue::Alarm.bus(), Bus::Ui);
        assert_eq!(Cue::from_name("nothing"), None);
        assert_eq!(Bus::ALL.map(Bus::name), ["UI", "VOICES", "WORLD", "MUSIC"]);
    }
}
