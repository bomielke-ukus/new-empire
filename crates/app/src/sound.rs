//! The device: what the mixer decides, sounded through `kira`; or recorded,
//! in a test; or dropped, when there is no device to be had. Recordings
//! under `assets/sounds/<cue>/*.wav` replace the placeholder clips of the
//! cues they are named for, the way rendered sprite sets replace the
//! placeholder art (`docs/07` Q7).

use audio::{Bus, Clip, Cue, Fade, Layer, Library, Play};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{
    AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Panning, PlaybackRate,
    Tween,
};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// What a test's speaker remembers.
#[derive(Default)]
#[allow(dead_code)]
pub struct Recording {
    /// Every play, in order.
    pub plays: Vec<Play>,
    /// Every fade of a layer, in order.
    pub fades: Vec<Fade>,
}

/// Where sound goes.
pub enum Speaker {
    /// Nowhere: there is no device, or none was opened.
    Silent,
    /// A record, for the tests.
    #[allow(dead_code)]
    Recorder(Recording),
    /// The device, boxed: it is large next to the others.
    Device(Box<Device>),
}

impl Speaker {
    /// Sounds one play.
    pub fn play(&mut self, play: &Play, clip: &Clip) {
        match self {
            Speaker::Silent => {}
            Speaker::Recorder(r) => r.plays.push(*play),
            Speaker::Device(d) => d.play(play, clip),
        }
    }

    /// Fades a layer to a level and a place, starting its loop if it is
    /// not playing.
    pub fn fade(&mut self, fade: Fade, clip: Option<&Clip>) {
        match self {
            Speaker::Silent => {}
            Speaker::Recorder(r) => r.fades.push(fade),
            Speaker::Device(d) => d.fade(fade, clip),
        }
    }

    /// A bus's volume, 0 to 1.
    pub fn set_volume(&mut self, bus: Bus, volume: f32) {
        if let Speaker::Device(d) = self {
            d.set_volume(bus, volume);
        }
    }

    /// The plays recorded so far, taken.
    #[allow(dead_code)]
    pub fn take(&mut self) -> Vec<Play> {
        match self {
            Speaker::Recorder(r) => std::mem::take(&mut r.plays),
            _ => Vec::new(),
        }
    }

    /// The fades recorded so far, taken.
    #[allow(dead_code)]
    pub fn take_fades(&mut self) -> Vec<Fade> {
        match self {
            Speaker::Recorder(r) => std::mem::take(&mut r.fades),
            _ => Vec::new(),
        }
    }
}

/// The audio device: a track per bus, every clip decoded once.
pub struct Device {
    /// Dropping it stops the sound, so it lives as long as the device.
    _manager: AudioManager,
    /// One per [`Bus`], in [`Bus::ALL`] order.
    tracks: Vec<TrackHandle>,
    /// The clips, ready to play.
    sounds: Vec<((Cue, usize), StaticSoundData)>,
    /// The layers playing, looped, each with its handle for the fades.
    loops: Vec<(Layer, StaticSoundHandle)>,
}

impl Device {
    /// Opens the default output and prepares every clip in the library.
    pub fn open(library: &Library) -> Result<Device, String> {
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| format!("{e:?}"))?;
        let mut tracks = Vec::new();
        for _ in Bus::ALL {
            let track = manager
                .add_sub_track(TrackBuilder::new())
                .map_err(|e| format!("{e:?}"))?;
            tracks.push(track);
        }
        let mut sounds = Vec::new();
        for (cue, clips) in library.iter() {
            for (i, clip) in clips.iter().enumerate() {
                sounds.push(((cue, i), data(clip)));
            }
        }
        Ok(Device {
            _manager: manager,
            tracks,
            sounds,
            loops: Vec::new(),
        })
    }

    fn fade(&mut self, fade: Fade, clip: Option<&Clip>) {
        let tween = Tween {
            duration: Duration::from_millis(u64::from(fade.ms)),
            ..Default::default()
        };
        if let Some((_, handle)) = self.loops.iter_mut().find(|(l, _)| *l == fade.layer) {
            handle.set_volume(decibels(fade.level), tween);
            handle.set_panning(Panning(fade.pan), tween);
            return;
        }
        // Not playing yet: nothing to fade out, and a loop to start
        // silent and bring up.
        let Some(clip) = clip.filter(|_| fade.level > 0.0) else {
            return;
        };
        let sound = data(clip)
            .loop_region(..)
            .volume(Decibels::SILENCE)
            .panning(Panning(fade.pan));
        match self.tracks[fade.layer.bus().index()].play(sound) {
            Ok(mut handle) => {
                handle.set_volume(decibels(fade.level), tween);
                self.loops.push((fade.layer, handle));
            }
            Err(e) => eprintln!("warning: {:?} did not start: {e}", fade.layer),
        }
    }

    fn play(&mut self, play: &Play, clip: &Clip) {
        let key = (play.cue, play.variant);
        let sound = match self.sounds.iter().find(|(k, _)| *k == key) {
            Some((_, d)) => d.clone(),
            None => data(clip),
        };
        let sound = sound
            .volume(decibels(play.gain))
            .panning(Panning(play.pan))
            .playback_rate(PlaybackRate(f64::from(play.rate)));
        if let Err(e) = self.tracks[play.bus.index()].play(sound) {
            eprintln!("warning: {:?} did not play: {e}", play.cue);
        }
    }

    fn set_volume(&mut self, bus: Bus, volume: f32) {
        self.tracks[bus.index()].set_volume(decibels(volume), Tween::default());
    }
}

/// A gain as the mixer gives it, 0 to 1, in the decibels the device takes.
fn decibels(gain: f32) -> Decibels {
    if gain <= 0.001 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * gain.log10())
    }
}

/// A clip as the device plays it: the mono samples on both channels.
fn data(clip: &Clip) -> StaticSoundData {
    StaticSoundData {
        sample_rate: clip.rate,
        frames: clip.samples.iter().map(|s| Frame::new(*s, *s)).collect(),
        settings: StaticSoundSettings::default(),
        slice: None,
    }
}

/// Loads the recordings under `dir/<cue>/*.wav` over the placeholders of
/// the cues they are named for, in file-name order as the variations. The
/// names of the cues replaced; a folder that is not a cue is reported.
pub fn recordings(dir: &Path, library: &mut Library) -> Vec<String> {
    let mut replaced = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return replaced;
    };
    let mut folders: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    for folder in folders {
        let name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let (cue, layer) = (Cue::from_name(&name), Layer::from_name(&name));
        if cue.is_none() && layer.is_none() {
            eprintln!(
                "warning: {}: not the name of a cue or a layer",
                folder.display()
            );
            continue;
        }
        let mut files: Vec<_> = std::fs::read_dir(&folder)
            .map(|d| {
                d.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "wav"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        let mut clips = Vec::new();
        for file in files {
            match StaticSoundData::from_file(&file) {
                Ok(d) => clips.push(Clip {
                    rate: d.sample_rate,
                    samples: Arc::from(
                        d.frames
                            .iter()
                            .map(|f| (f.left + f.right) * 0.5)
                            .collect::<Vec<_>>(),
                    ),
                }),
                Err(e) => eprintln!("warning: {}: {e:?}", file.display()),
            }
        }
        if clips.is_empty() {
            continue;
        }
        // A layer is one loop: the first file.
        match (cue, layer) {
            (Some(cue), _) => library.insert(cue, clips),
            (None, Some(layer)) => library.insert_layer(layer, clips.swap_remove(0)),
            (None, None) => unreachable!("checked above"),
        }
        replaced.push(name);
    }
    replaced
}
