//! A saved match (`docs/04` §10, `TA-SAVE-01`): the simulation as it
//! stands, its whole command log inside it, and the opponents' minds,
//! versioned (`TA-DET-06`). The log makes every save a resumable replay:
//! [`Save::replay`] is what `simrunner verify` reads from a save, and
//! [`Save::verify`] checks that the snapshot is what its own replay gives.
//!
//! Files are RON, as replays are, compact rather than pretty: a snapshot
//! of a Tiny map after ten minutes is a few hundred kilobytes of text,
//! read by machines. The
//! file name carries the time, seed, tick and player count, so a directory
//! is listed without reading a byte of any file.

use ai::Opponent;
use serde::{Deserialize, Serialize};
use sim::{Replay, ReplayError, Simulation, STATE_VERSION};
use std::path::{Path, PathBuf};

/// The save file's own layout: the fields of [`Save`]. Bump it when they
/// change shape.
pub const VERSION: u32 = 1;

/// Where the camera was, so a loaded match opens where it was left.
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct View {
    /// The camera's focus, in projected world px.
    pub focus: (f32, f32),
    /// Its zoom level index.
    pub zoom_index: usize,
}

/// A saved match.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Save {
    /// [`VERSION`].
    pub version: u32,
    /// [`sim::STATE_VERSION`] of the build that wrote it.
    pub state_version: u32,
    /// [`Replay::VERSION`] of the build that wrote it.
    pub replay_version: u32,
    /// Seconds since the Unix epoch when it was written; 0 if unknown.
    pub saved_at: u64,
    /// The match seed, repeated from the snapshot for the header.
    pub seed: u64,
    /// The tick it was saved at, repeated from the snapshot.
    pub tick: u64,
    /// Players in the match, repeated from the snapshot.
    pub players: u8,
    /// Where the camera was.
    pub view: View,
    /// The match, whole, with its command log.
    pub sim: Simulation,
    /// The computer opponents, mid-thought.
    pub opponents: Vec<Opponent>,
}

/// The fields read before the snapshot is looked at, so a save from
/// another build is refused by its numbers rather than by a parse error
/// somewhere inside the world.
#[derive(Deserialize)]
struct Header {
    version: u32,
    /// 0 when absent: versions start at 1, and a replay has neither.
    #[serde(default)]
    state_version: u32,
    #[serde(default)]
    replay_version: u32,
    #[serde(default)]
    saved_at: u64,
    #[serde(default)]
    seed: u64,
    #[serde(default)]
    tick: u64,
    #[serde(default)]
    players: u8,
}

/// What a save is, from its header or its file name alone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Summary {
    /// Seconds since the Unix epoch when it was written.
    pub saved_at: u64,
    /// The match seed.
    pub seed: u64,
    /// The tick it was saved at.
    pub tick: u64,
    /// Players in the match.
    pub players: u8,
}

/// Why a save was refused.
#[derive(Clone, PartialEq, Debug)]
pub enum SaveError {
    /// Written by a build with a different layout: which layer, and the
    /// two numbers.
    Version {
        /// "save", "simulation state" or "replay".
        what: &'static str,
        /// The number in the file.
        got: u32,
        /// The number this build reads.
        expected: u32,
    },
    /// Not a save at all: a replay, or anything else that parses.
    NotASave,
    /// The text does not parse.
    Parse(String),
    /// The file could not be read or written.
    Io(String),
    /// The command log inside is ill-formed.
    Replay(ReplayError),
    /// The snapshot is not what its own replay gives.
    Diverged {
        /// The tick of the snapshot.
        tick: u64,
        /// The snapshot's state hash.
        snapshot: u64,
        /// The hash the replay reaches at that tick.
        replayed: u64,
    },
}

impl core::fmt::Display for SaveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SaveError::Version {
                what,
                got,
                expected,
            } => write!(
                f,
                "{what} version {got} but this build reads version {expected}"
            ),
            SaveError::NotASave => write!(f, "not a save file"),
            SaveError::Parse(e) => write!(f, "could not parse: {e}"),
            SaveError::Io(e) => write!(f, "{e}"),
            SaveError::Replay(e) => write!(f, "the command log is ill-formed: {e}"),
            SaveError::Diverged {
                tick,
                snapshot,
                replayed,
            } => write!(
                f,
                "the snapshot at tick {tick} hashes {snapshot:016x} but its replay reaches {replayed:016x}"
            ),
        }
    }
}

impl std::error::Error for SaveError {}

fn check(what: &'static str, got: u32, expected: u32) -> Result<(), SaveError> {
    if got == expected {
        Ok(())
    } else {
        Err(SaveError::Version {
            what,
            got,
            expected,
        })
    }
}

/// Reads a save's header and refuses another build's by its numbers.
pub fn summary(text: &str) -> Result<Summary, SaveError> {
    let h: Header = ron::from_str(text).map_err(|e| SaveError::Parse(e.to_string()))?;
    if h.state_version == 0 || h.replay_version == 0 {
        return Err(SaveError::NotASave);
    }
    check("save", h.version, VERSION)?;
    check("simulation state", h.state_version, STATE_VERSION)?;
    check("replay", h.replay_version, Replay::VERSION)?;
    Ok(Summary {
        saved_at: h.saved_at,
        seed: h.seed,
        tick: h.tick,
        players: h.players,
    })
}

impl Save {
    /// A save of the match as it stands.
    pub fn new(sim: &Simulation, opponents: &[Opponent], view: View, saved_at: u64) -> Save {
        Save {
            version: VERSION,
            state_version: STATE_VERSION,
            replay_version: Replay::VERSION,
            saved_at,
            seed: sim.seed(),
            tick: sim.tick(),
            players: sim.players().len() as u8,
            view,
            sim: sim.clone(),
            opponents: opponents.to_vec(),
        }
    }

    /// Compact RON.
    pub fn to_ron(&self) -> Result<String, SaveError> {
        ron::to_string(self).map_err(|e| SaveError::Parse(e.to_string()))
    }

    /// Reads a save. The versions are checked before the snapshot is
    /// parsed, and the command log inside is validated as a replay.
    pub fn from_ron(text: &str) -> Result<Save, SaveError> {
        summary(text)?;
        let save: Save = ron::from_str(text).map_err(|e| SaveError::Parse(e.to_string()))?;
        save.replay().validate().map_err(SaveError::Replay)?;
        Ok(save)
    }

    /// The match so far as a replay: everything issued up to the save.
    pub fn replay(&self) -> Replay {
        self.sim.replay()
    }

    /// What the save is, from its own header.
    pub fn summary(&self) -> Summary {
        Summary {
            saved_at: self.saved_at,
            seed: self.seed,
            tick: self.tick,
            players: self.players,
        }
    }

    /// Runs the save's replay from the start and requires the snapshot's
    /// state hash at the end: a save is its own replay (`TA-SAVE-01`).
    /// Returns the hash.
    pub fn verify(&self) -> Result<u64, SaveError> {
        let replayed = self.replay().run(|_, _| {}).map_err(SaveError::Replay)?;
        let (snapshot, replayed) = (self.sim.state_hash(), replayed.state_hash());
        if snapshot != replayed {
            return Err(SaveError::Diverged {
                tick: self.sim.tick(),
                snapshot,
                replayed,
            });
        }
        Ok(snapshot)
    }

    /// The file name a save gets: when (UTC), the seed, the tick and the
    /// players, so a directory sorts by time and lists without reading.
    pub fn file_name(&self) -> String {
        format!(
            "{}-seed{}-tick{}-p{}.ron",
            compact_stamp(self.saved_at),
            self.seed,
            self.tick,
            self.players
        )
    }
}

/// A save on disk, known from its file name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    /// Where it is.
    pub path: PathBuf,
    /// The file stem.
    pub name: String,
    /// What the name says it is.
    pub summary: Summary,
}

fn io(e: std::io::Error, path: &Path) -> SaveError {
    SaveError::Io(format!("{}: {e}", path.display()))
}

/// Writes a save into `dir` under its own name, making the directory.
pub fn write(dir: &Path, save: &Save) -> Result<PathBuf, SaveError> {
    std::fs::create_dir_all(dir).map_err(|e| io(e, dir))?;
    let path = dir.join(save.file_name());
    let text = save.to_ron()?;
    std::fs::write(&path, format!("{text}\n")).map_err(|e| io(e, &path))?;
    Ok(path)
}

/// Reads a save from a file.
pub fn read(path: &Path) -> Result<Save, SaveError> {
    let text = std::fs::read_to_string(path).map_err(|e| io(e, path))?;
    Save::from_ron(&text)
}

/// Every save in `dir` by its file name, newest first. Files not named
/// as saves are left out; a save from another build is listed and refused
/// on reading, with its numbers.
pub fn list(dir: &Path) -> Vec<Entry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_stem()?.to_str()?.to_string();
            if path.extension()? != "ron" {
                return None;
            }
            let summary = parse_name(&name)?;
            Some(Entry {
                path,
                name,
                summary,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.summary
            .saved_at
            .cmp(&a.summary.saved_at)
            .then_with(|| b.name.cmp(&a.name))
    });
    out
}

/// `20260919-190512-seed3-tick4321-p2` back into a summary.
fn parse_name(name: &str) -> Option<Summary> {
    let mut parts = name.split('-');
    let ymd = parts.next()?;
    let hms = parts.next()?;
    let seed = parts.next()?.strip_prefix("seed")?.parse().ok()?;
    let tick = parts.next()?.strip_prefix("tick")?.parse().ok()?;
    let players = parts.next()?.strip_prefix('p')?.parse().ok()?;
    if parts.next().is_some() || ymd.len() != 8 || hms.len() != 6 {
        return None;
    }
    let n = |s: &str| s.parse::<i64>().ok();
    let (y, mo, d) = (n(&ymd[..4])?, n(&ymd[4..6])?, n(&ymd[6..])?);
    let (h, mi, s) = (n(&hms[..2])?, n(&hms[2..4])?, n(&hms[4..])?);
    let days = days_from_civil(y, mo, d);
    let secs = days * 86_400 + h * 3600 + mi * 60 + s;
    Some(Summary {
        saved_at: u64::try_from(secs).ok()?,
        seed,
        tick,
        players,
    })
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The proleptic Gregorian date of a day count since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `(year, month, day, hour, minute, second)` in UTC.
fn civil(secs: u64) -> (i64, i64, i64, i64, i64, i64) {
    let secs = secs as i64;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    (y, m, d, rem / 3600, rem % 3600 / 60, rem % 60)
}

/// `20260919-190512`, for a file name.
pub fn compact_stamp(secs: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// `2026-09-19 19:05 UTC`, for a screen.
pub fn stamp(secs: u64) -> String {
    let (y, mo, d, h, mi, _) = civil(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02} UTC")
}

/// `12:34`, minutes and seconds of match time at a tick.
pub fn clock(tick: u64) -> String {
    let secs = tick / sim::TICKS_PER_SECOND as u64;
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Recordings of matches played in the app (`docs/04` §10, `TA-DET-05`):
/// one replay file per match, named like a save for when the match
/// started, its seed, how far it got and its players, so the WATCH
/// REPLAY screen lists a directory without reading a file.
pub mod replays {
    use super::{compact_stamp, io, Entry, SaveError};
    use sim::Replay;
    use std::path::{Path, PathBuf};

    /// The file name a recording gets.
    pub fn file_name(replay: &Replay, started_at: u64) -> String {
        format!(
            "{}-seed{}-tick{}-p{}.ron",
            compact_stamp(started_at),
            replay.seed,
            replay.ticks,
            replay.config.map.players
        )
    }

    /// Writes a recording into `dir` under its own name, making the
    /// directory. Compact RON, as `simrunner` writes replays.
    pub fn write(dir: &Path, replay: &Replay, started_at: u64) -> Result<PathBuf, SaveError> {
        std::fs::create_dir_all(dir).map_err(|e| io(e, dir))?;
        let path = dir.join(file_name(replay, started_at));
        let text = ron::to_string(replay).map_err(|e| SaveError::Parse(e.to_string()))?;
        std::fs::write(&path, format!("{text}\n")).map_err(|e| io(e, &path))?;
        Ok(path)
    }

    /// Reads a recording and validates it as a replay, its version
    /// included (`TA-DET-06`), and its setup as the engine would.
    pub fn read(path: &Path) -> Result<Replay, SaveError> {
        let text = std::fs::read_to_string(path).map_err(|e| io(e, path))?;
        let replay: Replay = ron::from_str(&text).map_err(|e| SaveError::Parse(e.to_string()))?;
        replay.validate().map_err(SaveError::Replay)?;
        replay
            .config
            .validate()
            .map_err(|e| SaveError::Parse(format!("match setup: {e}")))?;
        Ok(replay)
    }

    /// Every recording in `dir`, newest first, from the names alone.
    pub fn list(dir: &Path) -> Vec<Entry> {
        super::list(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai::Difficulty;
    use fogged::FoggedView;
    use sim::{Command, MapKind, MapSpec, SimConfig, Source};

    fn fresh(seed: u64) -> (Simulation, Vec<Opponent>) {
        let sim = Simulation::new(
            seed,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Inland,
                    size: 96,
                    players: 2,
                },
                ..SimConfig::default()
            },
        );
        let bots = vec![
            Opponent::new(0, Difficulty::Standard, seed),
            Opponent::new(1, Difficulty::Hard, seed),
        ];
        (sim, bots)
    }

    /// One tick with the opponents playing; returns what they issued.
    fn tick(sim: &mut Simulation, bots: &mut [Opponent]) -> Vec<Command> {
        let mut issued = Vec::new();
        for bot in bots.iter_mut() {
            let commands = {
                let view = FoggedView::new(sim, bot.player());
                bot.think(&view)
            };
            for c in commands {
                sim.issue_from(c.clone(), Source::Ai);
                issued.push(c);
            }
        }
        sim.step();
        issued
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("new-empire-save-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A save read back is the match it was: the same state hash, and
    /// played on with its opponents it issues the same commands and
    /// hashes the same every tick as the match that was never saved.
    ///
    /// REQ: TA-SAVE-01
    #[test]
    fn a_loaded_save_continues_exactly_as_the_unsaved_match_would() {
        let (mut sim, mut bots) = fresh(11);
        for _ in 0..600 {
            tick(&mut sim, &mut bots);
        }
        let view = View {
            focus: (12.5, 34.0),
            zoom_index: 2,
        };
        let save = Save::new(&sim, &bots, view, 1_789_000_000);
        let text = save.to_ron().unwrap();
        assert!(text.len() < 8 << 20, "a Tiny-map save is not enormous");
        let mut loaded = Save::from_ron(&text).unwrap();
        assert_eq!(loaded.sim.state_hash(), sim.state_hash());
        assert_eq!(loaded.sim.tick(), 600);
        assert_eq!(loaded.view, view);
        assert_eq!(loaded.opponents.len(), 2);
        assert_eq!(loaded.opponents[1].difficulty(), Difficulty::Hard);
        assert_eq!(loaded.summary().tick, 600);
        let mut commands = 0;
        for t in 0..300 {
            let a = tick(&mut sim, &mut bots);
            let b = tick(&mut loaded.sim, &mut loaded.opponents);
            assert_eq!(a, b, "tick {}: the opponents think the same", 600 + t);
            assert_eq!(
                loaded.sim.state_hash(),
                sim.state_hash(),
                "tick {}: the worlds agree",
                600 + t
            );
            commands += a.len();
        }
        assert!(commands > 0, "the opponents were playing");
        // The save is its own replay, before and after playing on.
        assert_eq!(save.verify().unwrap(), save.sim.state_hash());
        assert_eq!(
            Save::new(&loaded.sim, &loaded.opponents, view, 0)
                .verify()
                .unwrap(),
            sim.state_hash()
        );
    }

    /// A save from another build is refused by its numbers before the
    /// snapshot is read; a replay is not a save; a snapshot that is not
    /// what its replay gives fails verification and says so.
    ///
    /// REQ: TA-DET-06
    #[test]
    fn another_builds_save_is_refused_with_both_version_numbers() {
        let (mut sim, mut bots) = fresh(3);
        for _ in 0..40 {
            tick(&mut sim, &mut bots);
        }
        let save = Save::new(&sim, &bots, View::default(), 0);
        let text = save.to_ron().unwrap();
        let old = text.replacen("state_version:1", "state_version:7", 1);
        let err = Save::from_ron(&old).unwrap_err();
        assert_eq!(
            err,
            SaveError::Version {
                what: "simulation state",
                got: 7,
                expected: STATE_VERSION
            }
        );
        assert!(err.to_string().contains("version 7"));
        assert!(err
            .to_string()
            .contains(&format!("version {STATE_VERSION}")));
        let old = text.replacen("version:1", "version:9", 1);
        assert!(matches!(
            Save::from_ron(&old),
            Err(SaveError::Version {
                what: "save",
                got: 9,
                ..
            })
        ));
        let replay = ron::to_string(&sim.replay()).unwrap();
        assert_eq!(summary(&replay), Err(SaveError::NotASave));
        assert!(matches!(
            Save::from_ron("garbage"),
            Err(SaveError::Parse(_))
        ));
        // A snapshot from one seed with a log claiming another: the
        // replay reaches a different world.
        let forged = text.replace("seed:3", "seed:4");
        let forged = Save::from_ron(&forged).expect("it parses");
        assert!(matches!(
            forged.verify(),
            Err(SaveError::Diverged { tick: 40, .. })
        ));
    }

    /// Saves are written under names that carry their time, seed, tick
    /// and players, listed newest first without reading them, and read
    /// back; files that are not saves are left out of the list.
    #[test]
    fn a_directory_of_saves_lists_newest_first_by_name_alone() {
        let dir = scratch_dir("list");
        assert!(list(&dir).is_empty(), "no directory, no saves");
        let (mut sim, mut bots) = fresh(5);
        for _ in 0..20 {
            tick(&mut sim, &mut bots);
        }
        let first = Save::new(&sim, &bots, View::default(), 1_700_000_000);
        for _ in 0..20 {
            tick(&mut sim, &mut bots);
        }
        let second = Save::new(&sim, &bots, View::default(), 1_700_003_600);
        let p1 = write(&dir, &first).unwrap();
        let p2 = write(&dir, &second).unwrap();
        assert_eq!(
            p1.file_name().unwrap().to_str().unwrap(),
            "20231114-221320-seed5-tick20-p2.ron"
        );
        std::fs::write(dir.join("notes.txt"), "not a save").unwrap();
        std::fs::write(dir.join("odd-name.ron"), "(version:1)").unwrap();
        let entries = list(&dir);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, p2, "newest first");
        assert_eq!(entries[0].summary, second.summary());
        assert_eq!(entries[1].summary, first.summary());
        let back = read(&p1).unwrap();
        assert_eq!(back.sim.state_hash(), first.sim.state_hash());
        assert_eq!(stamp(first.saved_at), "2023-11-14 22:13 UTC");
        assert_eq!(clock(20), "0:01");
        assert_eq!(clock(20 * 754), "12:34");
        assert_eq!(civil_from_days(days_from_civil(2026, 9, 19)), (2026, 9, 19));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A recording is written under a name that says when the match
    /// started and how far it got, listed like a save, and read back as
    /// the replay it is; one from another build is refused by version.
    ///
    /// REQ: TA-DET-05
    #[test]
    fn a_recording_is_a_replay_file_named_for_its_match() {
        let dir = scratch_dir("replays");
        let (mut sim, mut bots) = fresh(6);
        for _ in 0..50 {
            tick(&mut sim, &mut bots);
        }
        let replay = sim.replay();
        let path = replays::write(&dir, &replay, 1_700_000_000).unwrap();
        assert_eq!(
            path.file_name().unwrap().to_str().unwrap(),
            "20231114-221320-seed6-tick50-p2.ron"
        );
        let listed = replays::list(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].summary.tick, 50);
        assert_eq!(listed[0].summary.seed, 6);
        let back = replays::read(&path).unwrap();
        assert_eq!(back, replay);
        assert_eq!(back.trace_digest().unwrap(), replay.trace_digest().unwrap());
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            dir.join("old.ron"),
            text.replacen("version:1", "version:7", 1),
        )
        .unwrap();
        let err = replays::read(&dir.join("old.ron")).unwrap_err();
        assert!(matches!(err, SaveError::Replay(_)));
        assert!(err.to_string().contains("version 7"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
