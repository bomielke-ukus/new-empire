//! Campaigns on disk (`docs/02` §13, `docs/07` D35): a directory a
//! campaign, holding `campaign.ron` (its title, a line about it, its
//! scenarios in order) and a RON file a scenario. And the player's
//! progress through them: which scenarios they have won, kept beside the
//! settings, which unlocks the one after each.
//!
//! A scenario that does not read, or fails its check
//! ([`Scenario::problems`]), is left out with what is wrong with it, so a
//! broken file costs one scenario rather than the menu.

use serde::{Deserialize, Serialize};
use sim::Scenario;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// What `campaign.ron` says.
#[derive(Clone, Debug, Deserialize)]
struct CampaignFile {
    /// The campaign's name.
    title: String,
    /// A line about it.
    #[serde(default)]
    about: String,
    /// Its place among the campaigns: lower first.
    #[serde(default)]
    order: u32,
    /// Its scenarios' file names, without `.ron`, in the order played.
    scenarios: Vec<String>,
}

/// A campaign and its scenarios, read and checked.
#[derive(Clone, Debug)]
pub struct Campaign {
    /// Its directory's name.
    pub id: String,
    /// Its name.
    pub title: String,
    /// A line about it.
    pub about: String,
    /// Its place among the campaigns.
    pub order: u32,
    /// Its scenarios, in the order played; each one's id is
    /// `campaign/file`.
    pub scenarios: Vec<Scenario>,
}

/// Reads every campaign under `dir`, in their order, and says what was
/// left out and why.
pub fn load_all(dir: &Path) -> (Vec<Campaign>, Vec<String>) {
    let mut campaigns = Vec::new();
    let mut errors = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (campaigns, errors);
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("campaign.ron").is_file())
        .collect();
    dirs.sort();
    for d in dirs {
        match load(&d) {
            Ok((c, mut errs)) => {
                errors.append(&mut errs);
                campaigns.push(c);
            }
            Err(e) => errors.push(e),
        }
    }
    campaigns.sort_by(|a, b| (a.order, &a.id).cmp(&(b.order, &b.id)));
    (campaigns, errors)
}

/// Reads one campaign directory.
pub fn load(dir: &Path) -> Result<(Campaign, Vec<String>), String> {
    let id = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("campaign")
        .to_string();
    let path = dir.join("campaign.ron");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: CampaignFile =
        ron::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut errors = Vec::new();
    let mut scenarios = Vec::new();
    for name in &file.scenarios {
        match read_scenario(&dir.join(format!("{name}.ron"))) {
            Ok(mut sc) => {
                sc.id = format!("{id}/{name}");
                scenarios.push(sc);
            }
            Err(e) => errors.push(e),
        }
    }
    Ok((
        Campaign {
            id,
            title: file.title,
            about: file.about,
            order: file.order,
            scenarios,
        },
        errors,
    ))
}

/// Reads and checks one scenario file.
pub fn read_scenario(path: &Path) -> Result<Scenario, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sc: Scenario = ron::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let problems = sc.problems();
    if problems.is_empty() {
        Ok(sc)
    } else {
        Err(format!("{}: {}", path.display(), problems.join("; ")))
    }
}

/// Writes a scenario as a file the campaigns and the editor read.
pub fn write_scenario(path: &Path, sc: &Scenario) -> Result<(), String> {
    let config = ron::ser::PrettyConfig::new().depth_limit(3);
    let text = ron::ser::to_string_pretty(sc, config).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The scenarios the player has won.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Progress {
    /// Their ids.
    pub won: BTreeSet<String>,
}

impl Progress {
    /// Reads the progress file, or none yet.
    pub fn read(path: &Path) -> Progress {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| ron::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Writes it.
    pub fn write(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let text = ron::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Whether a campaign's scenario may be played: the first always, and
    /// each after one won.
    pub fn unlocked(&self, campaign: &Campaign, index: usize) -> bool {
        index == 0
            || campaign
                .scenarios
                .get(index - 1)
                .is_some_and(|prev| self.won.contains(&prev.id))
    }

    /// Whether it has been won.
    pub fn has_won(&self, scenario: &Scenario) -> bool {
        self.won.contains(&scenario.id)
    }
}

/// Where a scenario is in the campaigns: (campaign, scenario).
pub fn locate(campaigns: &[Campaign], id: &str) -> Option<(usize, usize)> {
    campaigns.iter().enumerate().find_map(|(c, camp)| {
        camp.scenarios
            .iter()
            .position(|s| s.id == id)
            .map(|s| (c, s))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/campaigns")
    }

    /// Every scenario shipped reads and passes its check, and its text is
    /// all in characters the font has, so nothing a player is told is
    /// missing a letter.
    #[test]
    fn every_shipped_campaign_reads_and_checks() {
        let (campaigns, errors) = load_all(&shipped());
        assert!(errors.is_empty(), "{errors:#?}");
        let font = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 :/-.%+()?,![]=;'";
        for c in &campaigns {
            assert!(!c.scenarios.is_empty(), "{} has no scenarios", c.id);
            for sc in &c.scenarios {
                assert!(!sc.title.is_empty(), "{} has no title", sc.id);
                let mut text: Vec<&str> = vec![&sc.title];
                text.extend(sc.briefing.iter().map(String::as_str));
                text.extend(sc.objectives.iter().map(|o| o.text.as_str()));
                for t in &sc.triggers {
                    for a in &t.then {
                        match a {
                            sim::scenario::Action::Say(s) | sim::scenario::Action::Lose(s) => {
                                text.push(s)
                            }
                            _ => {}
                        }
                    }
                }
                for line in text {
                    let bad: String = line
                        .chars()
                        .filter(|c| !font.contains(c.to_ascii_uppercase()))
                        .collect();
                    assert!(bad.is_empty(), "{}: {bad:?} not in the font: {line}", sc.id);
                }
                // And it starts: a match can be made of it.
                let sim = sim::Simulation::new(sc.match_seed(), sc.config());
                assert!(sim.scenario().is_some());
            }
        }
    }

    #[test]
    fn progress_unlocks_the_next_scenario_and_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("ne-campaign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let camp = dir.join("tale");
        std::fs::create_dir_all(&camp).unwrap();
        std::fs::write(
            camp.join("campaign.ron"),
            r#"(title: "A Tale", about: "Two parts.", scenarios: ["one", "two", "missing"])"#,
        )
        .unwrap();
        let scenario = |title: &str| {
            format!(
                r#"(title: "{title}", map: Generated(kind: Flat, size: 48),
                   sides: [(name: "Us", control: Player)],
                   objectives: [(id: "x", text: "Win", goal: Scripted)])"#
            )
        };
        std::fs::write(camp.join("one.ron"), scenario("One")).unwrap();
        std::fs::write(camp.join("two.ron"), scenario("Two")).unwrap();
        let (campaigns, errors) = load_all(&dir);
        assert_eq!(errors.len(), 1, "the missing file is reported: {errors:?}");
        let c = &campaigns[0];
        assert_eq!(c.scenarios.len(), 2);
        assert_eq!(c.scenarios[1].id, "tale/two");
        let path = dir.join("progress.ron");
        let mut p = Progress::read(&path);
        assert!(p.unlocked(c, 0) && !p.unlocked(c, 1));
        p.won.insert(c.scenarios[0].id.clone());
        p.write(&path).unwrap();
        let p = Progress::read(&path);
        assert!(p.unlocked(c, 1) && p.has_won(&c.scenarios[0]));
        assert_eq!(locate(&campaigns, "tale/two"), Some((0, 1)));
        // A scenario written out reads back the same.
        let out = dir.join("written.ron");
        write_scenario(&out, &c.scenarios[0]).unwrap();
        let back = read_scenario(&out).unwrap();
        assert_eq!(&back, &c.scenarios[0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
