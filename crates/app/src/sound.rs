//! The device: what the mixer decides, sounded through `kira`; or recorded,
//! in a test; or dropped, when there is no device to be had. Recordings
//! under `assets/sounds/<cue>/*.wav` replace the placeholder clips of the
//! cues they are named for, the way rendered sprite sets replace the
//! placeholder art (`docs/07` Q7).

use audio::{Bus, Clip, Cue, Library, Play};
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{
    AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Panning, PlaybackRate,
    Tween,
};
use std::path::Path;
use std::sync::Arc;

/// Where sound goes.
pub enum Speaker {
    /// Nowhere: there is no device, or none was opened.
    Silent,
    /// A list, for the tests.
    #[allow(dead_code)]
    Recorder(Vec<Play>),
    /// The device, boxed: it is large next to the others.
    Device(Box<Device>),
}

impl Speaker {
    /// Sounds one play.
    pub fn play(&mut self, play: &Play, clip: &Clip) {
        match self {
            Speaker::Silent => {}
            Speaker::Recorder(plays) => plays.push(*play),
            Speaker::Device(d) => d.play(play, clip),
        }
    }

    /// A bus's volume, 0 to 1.
    pub fn set_volume(&mut self, bus: Bus, volume: f32) {
        if let Speaker::Device(d) = self {
            d.set_volume(bus, volume);
        }
    }

    /// What was recorded so far, taken.
    #[allow(dead_code)]
    pub fn take(&mut self) -> Vec<Play> {
        match self {
            Speaker::Recorder(plays) => std::mem::take(plays),
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
        })
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
        let Some(cue) = Cue::from_name(&name) else {
            eprintln!("warning: {}: not the name of a cue", folder.display());
            continue;
        };
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
        if !clips.is_empty() {
            library.insert(cue, clips);
            replaced.push(name);
        }
    }
    replaced
}
