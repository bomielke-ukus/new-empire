//! Scenarios (`docs/02` §13, `docs/07` D35): a match set up by hand rather
//! than by the skirmish rules, with objectives the player works toward and
//! triggers that narrate, reveal, reinforce and decide it.
//!
//! A scenario is data: the ground (generated as a skirmish map is, or drawn
//! tile by tile), each side, what stands where, the objectives and the
//! triggers. It rides in [`SimConfig`](crate::SimConfig), so a replay and a
//! save carry it, and its running state ([`ScenarioState`]) is simulation
//! state, hashed like the rest. Things are named as a person writes them:
//! kinds and technologies by their display names (`"Town Center"`,
//! `"town_center"`), objectives, triggers and tagged units by short ids.
//!
//! The objectives belong to the first side, the player's. Once a second the
//! simulation checks the open objectives and then each trigger in order;
//! when every objective that is not optional is done the player has won,
//! and when one is failed, or nothing of theirs is left standing, lost.

use crate::battle::Event;
use crate::civs::Civ;
use crate::command::PlayerId;
use crate::entity::EntityId;
use crate::entity::KindId;
use crate::hash::{HashState, StateHasher};
use crate::kinds::{self, Cost, Resource};
use crate::map::{Terrain, TileMap, MAX_ELEVATION};
use crate::mapgen::MapKind;
use crate::simulation::{Simulation, TICKS_PER_SECOND};
use crate::tech::{self, Age};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A scenario: everything needed to set a match up and play it out.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Scenario {
    /// The ground.
    pub map: ScenarioMap,
    /// Each side, the player's first; at most eight.
    pub sides: Vec<Side>,
    /// Whether the generator's standard start is laid down on a generated
    /// map: each side's Town Center and villagers. Without it only the
    /// generator's trees, mines, bushes and herds are kept.
    #[serde(default)]
    pub standard_start: bool,
    /// What stands on the map at the start, besides.
    #[serde(default)]
    pub placements: Vec<Placement>,
    /// What the player is to do.
    pub objectives: Vec<Objective>,
    /// What happens, and when.
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    /// Whether the skirmish victories hold too: the last side standing, a
    /// Wonder or the relics held ten minutes. Off, the scenario's
    /// objectives alone decide it (an enemy that only arrives later by
    /// trigger is not a side already beaten).
    #[serde(default)]
    pub skirmish_victories: bool,
}

/// The ground a scenario is played on.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ScenarioMap {
    /// Generated from the match seed, as a skirmish map is.
    Generated {
        /// Which generator.
        kind: MapKind,
        /// Edge length in tiles.
        size: u16,
    },
    /// Drawn tile by tile.
    Drawn(DrawnMap),
}

/// A map drawn by hand: a row of letters a row of tiles, north first.
/// `g` grass, `d` dirt, `a` desert, `s` sand, `w` shallow water, `W` deep
/// water, `f` forest (a tree on the tile), `n` snow. Heights, if given, are
/// a row of digits `0`–`3` a row of tile corners, one more row and column
/// than the tiles; without them the map is flat.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DrawnMap {
    /// The tiles.
    pub terrain: Vec<String>,
    /// The corner heights; empty for flat.
    #[serde(default)]
    pub heights: Vec<String>,
}

/// Who plays a side.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Control {
    /// The player (the first side, and only it).
    Player,
    /// The computer opponent, at a difficulty from 0 (Easy) to 3
    /// (Hardest). Read by the application, which runs it.
    Computer(u8),
    /// Nobody: it does what its triggers order and its units' stances do.
    #[default]
    Scripted,
}

/// One side.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Side {
    /// Its name, as the player is told it ("Athens").
    pub name: String,
    /// Who plays it.
    #[serde(default)]
    pub control: Control,
    /// Its civilization, for its bonuses and its buildings' look.
    #[serde(default)]
    pub civ: Option<Civ>,
    /// The age it starts in.
    #[serde(default)]
    pub age: Age,
    /// What it starts with: food, wood, stone, gold.
    #[serde(default = "default_stockpile")]
    pub stockpile: Cost,
    /// Technologies it starts with, by name.
    #[serde(default)]
    pub techs: Vec<String>,
}

fn default_stockpile() -> Cost {
    crate::simulation::DEFAULT_STOCKPILE
}

/// A rectangle of tiles, both corners inside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Area {
    /// One corner.
    pub from: (i32, i32),
    /// The opposite one.
    pub to: (i32, i32),
}

impl Area {
    /// Whether the tile is inside.
    pub fn contains(&self, (x, y): (i32, i32)) -> bool {
        let (x0, x1) = (self.from.0.min(self.to.0), self.from.0.max(self.to.0));
        let (y0, y1) = (self.from.1.min(self.to.1), self.from.1.max(self.to.1));
        x >= x0 && x <= x1 && y >= y0 && y <= y1
    }

    /// Every tile inside, row by row.
    pub fn tiles(&self) -> impl Iterator<Item = (i32, i32)> {
        let (x0, x1) = (self.from.0.min(self.to.0), self.from.0.max(self.to.0));
        let (y0, y1) = (self.from.1.min(self.to.1), self.from.1.max(self.to.1));
        (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
    }

    /// The tile in the middle.
    pub fn middle(&self) -> (i32, i32) {
        ((self.from.0 + self.to.0) / 2, (self.from.1 + self.to.1) / 2)
    }
}

/// Something set on the map.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Placement {
    /// What, by name.
    pub kind: String,
    /// Whose: a side's index, or 255 for nobody's (a tree, a mine, a herd).
    pub owner: PlayerId,
    /// Where: the tile, or for a building the corner of its footprint
    /// nearest the map's origin.
    pub at: (i32, i32),
    /// How many: units spread out round `at`.
    #[serde(default = "one")]
    pub count: u16,
    /// A name a trigger may use for it; with a count above one, the first.
    #[serde(default)]
    pub tag: Option<String>,
}

fn one() -> u16 {
    1
}

/// What the player is to do.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Objective {
    /// The name triggers use for it.
    pub id: String,
    /// What the player is told.
    pub text: String,
    /// What fulfils it.
    pub goal: Goal,
    /// Not shown, and not checked, until a trigger shows it.
    #[serde(default)]
    pub hidden: bool,
    /// Not needed to win.
    #[serde(default)]
    pub optional: bool,
}

/// What fulfils an objective.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Goal {
    /// So much of a resource in store at once.
    Stockpile {
        /// Which.
        resource: Resource,
        /// How much.
        amount: i32,
    },
    /// So many of a kind of the player's standing, finished.
    Have {
        /// Which kind, by name.
        kind: String,
        /// How many.
        count: u16,
    },
    /// An age reached.
    Age(Age),
    /// Nothing of a side's left standing, or nothing of one kind of its.
    Destroy {
        /// Whose.
        owner: PlayerId,
        /// Which kind, by name; any unit or building if none.
        #[serde(default)]
        kind: Option<String>,
    },
    /// So many of the player's units inside an area.
    Reach {
        /// Where.
        area: Area,
        /// Which kind, by name; any unit if none.
        #[serde(default)]
        kind: Option<String>,
        /// How many.
        count: u16,
    },
    /// Holding out so long.
    Survive {
        /// Seconds from the start.
        seconds: u32,
    },
    /// Done only when a trigger says so.
    Scripted,
}

/// A trigger: when every condition holds, its actions happen, once or each
/// time they hold.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Trigger {
    /// A name other triggers may wait on.
    #[serde(default)]
    pub id: Option<String>,
    /// What must hold; nothing, and it happens at the start.
    #[serde(default)]
    pub when: Vec<Condition>,
    /// What happens.
    pub then: Vec<Action>,
    /// Whether it happens again each second the conditions hold, rather
    /// than once.
    #[serde(default)]
    pub repeat: bool,
}

/// Something a trigger waits for.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Condition {
    /// So many seconds from the start.
    After(u32),
    /// An objective done.
    Done(String),
    /// An objective failed.
    Failed(String),
    /// Another trigger has happened.
    Fired(String),
    /// A side has at least so many of a kind standing, finished.
    Has {
        /// Whose.
        owner: PlayerId,
        /// Which kind, by name; any unit or building if none.
        #[serde(default)]
        kind: Option<String>,
        /// At least how many.
        count: u16,
    },
    /// A side has at most so many of a kind standing (none: lost them all).
    HasAtMost {
        /// Whose.
        owner: PlayerId,
        /// Which kind, by name; any unit or building if none.
        #[serde(default)]
        kind: Option<String>,
        /// At most how many.
        count: u16,
    },
    /// A side has so much of a resource in store.
    Stockpile {
        /// Whose.
        owner: PlayerId,
        /// Which.
        resource: Resource,
        /// At least how much.
        amount: i32,
    },
    /// A side has reached an age.
    Age {
        /// Whose.
        owner: PlayerId,
        /// Which.
        age: Age,
    },
    /// A side has a technology.
    Researched {
        /// Whose.
        owner: PlayerId,
        /// Which, by name.
        tech: String,
    },
    /// So many of a side's units are inside an area.
    Inside {
        /// Whose.
        owner: PlayerId,
        /// Where.
        area: Area,
        /// Which kind, by name; any unit if none.
        #[serde(default)]
        kind: Option<String>,
        /// At least how many.
        count: u16,
    },
    /// A tagged unit or building is gone (or was never placed).
    Gone(String),
}

/// Something a trigger does.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Action {
    /// The narrator tells the player something.
    Say(String),
    /// An objective appears.
    Show(String),
    /// An objective is done.
    Complete(String),
    /// An objective is failed.
    Fail(String),
    /// Something is set on the map.
    Place(Placement),
    /// A side is given, or loses, so much of a resource.
    Give {
        /// Whose.
        owner: PlayerId,
        /// Which.
        resource: Resource,
        /// How much; less than nothing takes away.
        amount: i32,
    },
    /// The player is shown the ground in an area.
    Reveal(Area),
    /// A side's soldiers attack-move to a tile.
    Attack {
        /// Whose.
        owner: PlayerId,
        /// Where to.
        to: (i32, i32),
    },
    /// The player has won.
    Win,
    /// The player has lost, and is told why.
    Lose(String),
}

/// How an objective stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ObjectiveStatus {
    /// Not shown yet.
    #[default]
    Hidden,
    /// Shown, not yet done.
    Open,
    /// Done.
    Done,
    /// Failed.
    Failed,
}

/// How a scenario ended for the player.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Outcome {
    /// Every objective needed is done, or a trigger said so.
    Won,
    /// Why not.
    Lost(String),
}

/// A scenario's running state.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ScenarioState {
    /// Each objective's status, in the scenario's order.
    pub(crate) objectives: Vec<ObjectiveStatus>,
    /// How often each trigger has happened.
    pub(crate) fired: Vec<u32>,
    /// The tagged units and buildings.
    pub(crate) tags: BTreeMap<String, EntityId>,
    /// How it ended.
    pub(crate) outcome: Option<Outcome>,
}

impl HashState for ScenarioState {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.objectives.len() as u32);
        for s in &self.objectives {
            h.write_u8(*s as u8);
        }
        h.write_u32(self.fired.len() as u32);
        for &n in &self.fired {
            h.write_u32(n);
        }
        h.write_u32(self.tags.len() as u32);
        for (tag, id) in &self.tags {
            h.write_str(tag);
            h.write(id);
        }
        match &self.outcome {
            None => h.write_u8(0),
            Some(Outcome::Won) => h.write_u8(1),
            Some(Outcome::Lost(why)) => {
                h.write_u8(2);
                h.write_str(why);
            }
        }
    }
}

impl HashState for Scenario {
    fn hash_state(&self, h: &mut StateHasher) {
        // A scenario is fixed for the match; its debug form names every
        // field and value in order, which is all the hash needs.
        h.write_str(&format!("{self:?}"));
    }
}

impl Scenario {
    /// The match setup for this scenario: the map's size and sides, and
    /// each side's civilization, with the scenario itself aboard.
    pub fn config(&self) -> crate::SimConfig {
        let size = match &self.map {
            ScenarioMap::Generated { size, .. } => *size,
            ScenarioMap::Drawn(d) => d.terrain.len().max(1) as u16,
        };
        let kind = match &self.map {
            ScenarioMap::Generated { kind, .. } => *kind,
            ScenarioMap::Drawn(_) => MapKind::Inland,
        };
        crate::SimConfig {
            map: crate::MapSpec {
                kind,
                size,
                players: self.sides.len().clamp(1, 8) as u8,
            },
            civs: if self.sides.iter().any(|s| s.civ.is_some()) {
                self.sides
                    .iter()
                    .map(|s| s.civ.unwrap_or(Civ::Greeks))
                    .collect()
            } else {
                Vec::new()
            },
            scenario: Some(Box::new(self.clone())),
            ..crate::SimConfig::default()
        }
    }

    /// Everything wrong with the scenario, so it can be refused when it is
    /// loaded rather than misbehave in play: names that name nothing, ids
    /// used twice or never defined, a map whose rows disagree, places off
    /// the map. Empty when it is sound.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let sides = self.sides.len();
        if sides == 0 || sides > 8 {
            out.push(format!("a scenario has 1 to 8 sides, not {sides}"));
        }
        for (i, s) in self.sides.iter().enumerate() {
            if (i == 0) != (s.control == Control::Player) {
                out.push(format!(
                    "side {i} ({}): the first side, and only it, is the player's",
                    s.name
                ));
            }
            if let Control::Computer(d) = s.control {
                if d > 3 {
                    out.push(format!("side {i}: difficulty {d} is not 0 to 3"));
                }
            }
            for t in &s.techs {
                if tech::by_name(t).is_none() {
                    out.push(format!("side {i}: no technology called {t:?}"));
                }
            }
        }
        let (w, h) = match &self.map {
            ScenarioMap::Generated { size, .. } => {
                if !(48..=256).contains(size) {
                    out.push(format!("a generated map is 48 to 256 tiles, not {size}"));
                }
                (*size as i32, *size as i32)
            }
            ScenarioMap::Drawn(d) => {
                if let Err(e) = d.tiles() {
                    out.push(e);
                }
                let h = d.terrain.len() as i32;
                let w = d.terrain.first().map_or(0, |r| r.chars().count() as i32);
                (w, h)
            }
        };
        let on_map = |(x, y): (i32, i32)| x >= 0 && y >= 0 && x < w && y < h;
        let side_ok = |p: PlayerId| (p as usize) < sides;
        let owner_ok = |p: PlayerId| side_ok(p) || p == kinds::GAIA;
        let kind_ok = |k: &str| kinds::by_name(k).is_some();
        let mut tags = Vec::new();
        let mut placement = |pl: &Placement, out: &mut Vec<String>, place: &str| {
            if !kind_ok(&pl.kind) {
                out.push(format!("{place}: no kind called {:?}", pl.kind));
            }
            if !owner_ok(pl.owner) {
                out.push(format!("{place}: no side {}", pl.owner));
            }
            if !on_map(pl.at) {
                out.push(format!("{place}: {:?} is off the map", pl.at));
            }
            if let Some(t) = &pl.tag {
                tags.push(t.clone());
            }
        };
        for (i, pl) in self.placements.iter().enumerate() {
            placement(pl, &mut out, &format!("placement {i}"));
        }
        for (i, t) in self.triggers.iter().enumerate() {
            for a in &t.then {
                if let Action::Place(pl) = a {
                    placement(pl, &mut out, &format!("trigger {i}"));
                }
            }
        }
        let mut ids: Vec<&str> = Vec::new();
        for o in &self.objectives {
            if ids.contains(&o.id.as_str()) {
                out.push(format!("objective {:?} is defined twice", o.id));
            }
            ids.push(&o.id);
            match &o.goal {
                Goal::Have { kind, .. } if !kind_ok(kind) => {
                    out.push(format!("objective {:?}: no kind called {kind:?}", o.id));
                }
                Goal::Destroy { owner, kind } => {
                    if !side_ok(*owner) {
                        out.push(format!("objective {:?}: no side {owner}", o.id));
                    }
                    if kind.as_deref().is_some_and(|k| !kind_ok(k)) {
                        out.push(format!("objective {:?}: no kind called {kind:?}", o.id));
                    }
                }
                Goal::Reach { area, kind, .. } => {
                    if !on_map(area.from) || !on_map(area.to) {
                        out.push(format!("objective {:?}: its area is off the map", o.id));
                    }
                    if kind.as_deref().is_some_and(|k| !kind_ok(k)) {
                        out.push(format!("objective {:?}: no kind called {kind:?}", o.id));
                    }
                }
                _ => {}
            }
        }
        let triggers: Vec<&str> = self
            .triggers
            .iter()
            .filter_map(|t| t.id.as_deref())
            .collect();
        for (i, t) in self.triggers.iter().enumerate() {
            let at = format!("trigger {}", t.id.clone().unwrap_or_else(|| i.to_string()));
            for c in &t.when {
                let bad = match c {
                    Condition::Done(o) | Condition::Failed(o) => {
                        (!ids.contains(&o.as_str())).then(|| format!("no objective {o:?}"))
                    }
                    Condition::Fired(f) => {
                        (!triggers.contains(&f.as_str())).then(|| format!("no trigger {f:?}"))
                    }
                    Condition::Gone(tag) => {
                        (!tags.contains(tag)).then(|| format!("nothing tagged {tag:?}"))
                    }
                    Condition::Has { owner, kind, .. }
                    | Condition::HasAtMost { owner, kind, .. } => {
                        if !side_ok(*owner) {
                            Some(format!("no side {owner}"))
                        } else {
                            kind.as_deref()
                                .filter(|k| !kind_ok(k))
                                .map(|k| format!("no kind called {k:?}"))
                        }
                    }
                    Condition::Inside {
                        owner, area, kind, ..
                    } => {
                        if !side_ok(*owner) {
                            Some(format!("no side {owner}"))
                        } else if !on_map(area.from) || !on_map(area.to) {
                            Some("an area off the map".to_string())
                        } else {
                            kind.as_deref()
                                .filter(|k| !kind_ok(k))
                                .map(|k| format!("no kind called {k:?}"))
                        }
                    }
                    Condition::Researched { owner, tech } => {
                        if !side_ok(*owner) {
                            Some(format!("no side {owner}"))
                        } else {
                            tech::by_name(tech)
                                .is_none()
                                .then(|| format!("no technology called {tech:?}"))
                        }
                    }
                    Condition::Stockpile { owner, .. } | Condition::Age { owner, .. } => {
                        (!side_ok(*owner)).then(|| format!("no side {owner}"))
                    }
                    Condition::After(_) => None,
                };
                if let Some(b) = bad {
                    out.push(format!("{at}: {b}"));
                }
            }
            for a in &t.then {
                let bad = match a {
                    Action::Show(o) | Action::Complete(o) | Action::Fail(o) => {
                        (!ids.contains(&o.as_str())).then(|| format!("no objective {o:?}"))
                    }
                    Action::Give { owner, .. } => {
                        (!side_ok(*owner)).then(|| format!("no side {owner}"))
                    }
                    Action::Attack { owner, to } => {
                        if !side_ok(*owner) {
                            Some(format!("no side {owner}"))
                        } else {
                            (!on_map(*to)).then(|| format!("{to:?} is off the map"))
                        }
                    }
                    Action::Reveal(area) => (!on_map(area.from) || !on_map(area.to))
                        .then(|| "an area off the map".to_string()),
                    Action::Place(_) | Action::Say(_) | Action::Win | Action::Lose(_) => None,
                };
                if let Some(b) = bad {
                    out.push(format!("{at}: {b}"));
                }
            }
        }
        if self.objectives.iter().all(|o| o.optional)
            && !self
                .triggers
                .iter()
                .any(|t| t.then.iter().any(|a| matches!(a, Action::Win)))
        {
            out.push(
                "nothing wins it: no objective that is not optional, and no trigger that wins"
                    .into(),
            );
        }
        out
    }
}

impl DrawnMap {
    /// The tiles and heights, or what is wrong with the drawing.
    pub fn tiles(&self) -> Result<TileMap, String> {
        let h = self.terrain.len();
        let w = self.terrain.first().map_or(0, |r| r.chars().count());
        if !(16..=256).contains(&w) || !(16..=256).contains(&h) {
            return Err(format!(
                "a drawn map is 16 to 256 tiles a side, not {w} by {h}"
            ));
        }
        let mut map = TileMap::new(w as u16, h as u16);
        for (y, row) in self.terrain.iter().enumerate() {
            if row.chars().count() != w {
                return Err(format!(
                    "map row {y} is {} tiles, not {w}",
                    row.chars().count()
                ));
            }
            for (x, c) in row.chars().enumerate() {
                let t = terrain_of(c)
                    .ok_or_else(|| format!("map row {y}: {c:?} is no terrain (g d a s w W f n)"))?;
                map.set_terrain(x as i32, y as i32, t);
            }
        }
        if !self.heights.is_empty() {
            if self.heights.len() != h + 1 {
                return Err(format!(
                    "{} rows of heights, not {}",
                    self.heights.len(),
                    h + 1
                ));
            }
            for (cy, row) in self.heights.iter().enumerate() {
                if row.chars().count() != w + 1 {
                    return Err(format!(
                        "height row {cy} is {} corners, not {}",
                        row.chars().count(),
                        w + 1
                    ));
                }
                for (cx, c) in row.chars().enumerate() {
                    let v = c
                        .to_digit(10)
                        .filter(|&v| v <= MAX_ELEVATION as u32)
                        .ok_or_else(|| {
                            format!("height row {cy}: {c:?} is no height (0 to {MAX_ELEVATION})")
                        })?;
                    map.set_corner(cx as i32, cy as i32, v as u8);
                }
            }
            if let Err(((ax, ay), (bx, by))) = map.validate() {
                return Err(format!(
                    "corners ({ax}, {ay}) and ({bx}, {by}) differ by more than one level"
                ));
            }
        }
        Ok(map)
    }
}

/// The terrain a map letter draws.
pub fn terrain_of(c: char) -> Option<Terrain> {
    Some(match c {
        'g' => Terrain::Grass,
        'd' => Terrain::Dirt,
        'a' => Terrain::Desert,
        's' => Terrain::Sand,
        'w' => Terrain::ShallowWater,
        'W' => Terrain::DeepWater,
        'f' => Terrain::ForestFloor,
        'n' => Terrain::Snow,
        _ => return None,
    })
}

/// The letter that draws a terrain.
pub fn letter_of(t: Terrain) -> char {
    match t {
        Terrain::Grass => 'g',
        Terrain::Dirt => 'd',
        Terrain::Desert => 'a',
        Terrain::Sand => 's',
        Terrain::ShallowWater => 'w',
        Terrain::DeepWater => 'W',
        Terrain::ForestFloor => 'f',
        Terrain::Snow => 'n',
    }
}

/// What the presentation shows of an objective.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ObjectiveView<'a> {
    /// What the player is told.
    pub text: &'a str,
    /// How it stands.
    pub status: ObjectiveStatus,
    /// Not needed to win.
    pub optional: bool,
}

impl Simulation {
    /// The scenario this match plays, if it is one.
    pub fn scenario(&self) -> Option<&Scenario> {
        self.config.scenario.as_deref()
    }

    /// The objectives shown so far, in the scenario's order.
    pub fn objectives(&self) -> Vec<ObjectiveView<'_>> {
        let Some(sc) = self.scenario() else {
            return Vec::new();
        };
        sc.objectives
            .iter()
            .zip(&self.scenario_state.objectives)
            .filter(|(_, s)| **s != ObjectiveStatus::Hidden)
            .map(|(o, &status)| ObjectiveView {
                text: &o.text,
                status,
                optional: o.optional,
            })
            .collect()
    }

    /// How the scenario ended for the player, once it has.
    pub fn outcome(&self) -> Option<&Outcome> {
        self.scenario_state.outcome.as_ref()
    }

    /// The line a [`Event::Said`] names.
    pub fn line(&self, trigger: u16, action: u16) -> Option<&str> {
        let t = self.scenario()?.triggers.get(trigger as usize)?;
        match t.then.get(action as usize)? {
            Action::Say(s) => Some(s),
            _ => None,
        }
    }

    /// Sets the scenario's sides up once the map and the placements are
    /// down: their ages and technologies, and their stockpiles.
    pub(crate) fn scenario_setup(&mut self) {
        let Some(sc) = self.config.scenario.clone() else {
            return;
        };
        self.scenario_state = ScenarioState {
            objectives: sc
                .objectives
                .iter()
                .map(|o| {
                    if o.hidden {
                        ObjectiveStatus::Hidden
                    } else {
                        ObjectiveStatus::Open
                    }
                })
                .collect(),
            fired: vec![0; sc.triggers.len()],
            tags: BTreeMap::new(),
            outcome: None,
        };
        for (p, side) in sc.sides.iter().enumerate() {
            let p = p as PlayerId;
            for (age, t) in [
                (Age::Tool, tech::AGE_TOOL),
                (Age::Bronze, tech::AGE_BRONZE),
                (Age::Iron, tech::AGE_IRON),
            ] {
                if side.age >= age {
                    self.apply_tech(p, t);
                }
            }
            for t in side.techs.iter().filter_map(|t| tech::by_name(t)) {
                self.apply_tech(p, t);
            }
            if let Some(pl) = self.players.get_mut(p as usize) {
                pl.stockpile = side.stockpile;
            }
        }
        for pl in &sc.placements {
            self.place(pl);
        }
        // Setting up is not news.
        self.events.clear();
    }

    /// Sets a placement on the map; its tag names the first of it.
    fn place(&mut self, pl: &Placement) {
        let Some(kind) = kinds::by_name(&pl.kind) else {
            return;
        };
        let info = kinds::info(kind);
        let mut first = None;
        if info.mobile {
            let (x, y) = pl.at;
            let tiles = if info.naval {
                self.water.spread(x, y, pl.count as usize, None)
            } else {
                self.nav.spread(x, y, pl.count as usize, None)
            };
            for t in tiles {
                let id = self.spawn(kind, pl.owner, crate::nav::centre(t));
                first = first.or(id);
            }
        } else {
            let fp = info.footprint.max(1) as i32;
            for k in 0..pl.count.max(1) as i32 {
                let (x, y) = (pl.at.0 + k * fp, pl.at.1);
                let id = self.spawn(kind, pl.owner, crate::nav::building_centre(x, y, fp));
                first = first.or(id);
            }
            self.nav.refresh();
            self.water.refresh();
        }
        if let (Some(tag), Some(id)) = (&pl.tag, first) {
            self.scenario_state.tags.insert(tag.clone(), id);
        }
    }

    /// Once a second: the objectives, the triggers, and whether it is over.
    pub(crate) fn scenario_tick(&mut self) {
        if self.config.scenario.is_none()
            || self.scenario_state.outcome.is_some()
            || !self.tick.is_multiple_of(TICKS_PER_SECOND as u64)
        {
            return;
        }
        let sc = self.config.scenario.clone().expect("checked");
        // The open objectives first, so a trigger sees them done this second.
        for (i, o) in sc.objectives.iter().enumerate() {
            if self.scenario_state.objectives[i] == ObjectiveStatus::Open && self.goal_met(&o.goal)
            {
                self.set_objective(i, ObjectiveStatus::Done);
            }
        }
        for (ti, t) in sc.triggers.iter().enumerate() {
            if self.scenario_state.outcome.is_some() {
                break;
            }
            if self.scenario_state.fired[ti] > 0 && !t.repeat {
                continue;
            }
            if !t.when.iter().all(|c| self.holds(&sc, c)) {
                continue;
            }
            self.scenario_state.fired[ti] += 1;
            for (ai, a) in t.then.iter().enumerate() {
                self.act(&sc, ti, ai, a);
            }
        }
        if self.scenario_state.outcome.is_some() {
            return;
        }
        let needed = || {
            sc.objectives
                .iter()
                .zip(&self.scenario_state.objectives)
                .filter(|(o, _)| !o.optional)
        };
        if let Some((o, _)) = needed().find(|(_, s)| **s == ObjectiveStatus::Failed) {
            self.scenario_state.outcome = Some(Outcome::Lost(format!("Failed: {}", o.text)));
        } else if needed().count() > 0 && needed().all(|(_, s)| *s == ObjectiveStatus::Done) {
            self.scenario_state.outcome = Some(Outcome::Won);
        } else if !self.standing(0) {
            self.scenario_state.outcome =
                Some(Outcome::Lost("Nothing is left to fight with".into()));
        }
    }

    fn set_objective(&mut self, i: usize, status: ObjectiveStatus) {
        if self.scenario_state.objectives[i] != status {
            self.scenario_state.objectives[i] = status;
            self.events.push(Event::Objective {
                index: i as u16,
                status,
            });
        }
    }

    fn objective_index(sc: &Scenario, id: &str) -> Option<usize> {
        sc.objectives.iter().position(|o| o.id == id)
    }

    /// How many of `owner`'s are standing, finished: of `kind`, or any unit
    /// or building.
    fn count_standing(&self, owner: PlayerId, kind: Option<KindId>) -> u32 {
        self.world
            .slots()
            .filter(|s| {
                let i = s.index();
                self.world.owner[i] == owner
                    && self.world.dying[i] == 0
                    && self.world.construction[i].is_none()
                    && match kind {
                        Some(k) => self.world.kind[i] == k,
                        None => {
                            let info = kinds::info(self.world.kind[i]);
                            info.mobile || info.footprint > 0
                        }
                    }
            })
            .count() as u32
    }

    /// How many of `owner`'s units (of `kind`) are inside `area`.
    fn count_inside(&self, owner: PlayerId, area: &Area, kind: Option<KindId>) -> u32 {
        self.world
            .slots()
            .filter(|s| {
                let i = s.index();
                self.world.owner[i] == owner
                    && self.world.dying[i] == 0
                    && self.world.inside[i].is_none()
                    && kinds::info(self.world.kind[i]).mobile
                    && kind.is_none_or(|k| self.world.kind[i] == k)
                    && area.contains(crate::nav::tile_of(self.world.pos[i]))
            })
            .count() as u32
    }

    fn goal_met(&self, goal: &Goal) -> bool {
        let named = |k: &Option<String>| k.as_deref().and_then(kinds::by_name);
        match goal {
            Goal::Stockpile { resource, amount } => self
                .players
                .first()
                .is_some_and(|p| p.stockpile[resource.index()] >= *amount),
            Goal::Have { kind, count } => kinds::by_name(kind)
                .is_some_and(|k| self.count_standing(0, Some(k)) >= *count as u32),
            Goal::Age(age) => self.players.first().is_some_and(|p| p.age >= *age),
            Goal::Destroy { owner, kind } => self.count_standing(*owner, named(kind)) == 0,
            Goal::Reach { area, kind, count } => {
                self.count_inside(0, area, named(kind)) >= *count as u32
            }
            Goal::Survive { seconds } => self.tick >= *seconds as u64 * TICKS_PER_SECOND as u64,
            Goal::Scripted => false,
        }
    }

    fn holds(&self, sc: &Scenario, c: &Condition) -> bool {
        let named = |k: &Option<String>| k.as_deref().and_then(kinds::by_name);
        let status =
            |id: &str| Self::objective_index(sc, id).map(|i| self.scenario_state.objectives[i]);
        match c {
            Condition::After(s) => self.tick >= *s as u64 * TICKS_PER_SECOND as u64,
            Condition::Done(o) => status(o) == Some(ObjectiveStatus::Done),
            Condition::Failed(o) => status(o) == Some(ObjectiveStatus::Failed),
            Condition::Fired(f) => sc
                .triggers
                .iter()
                .position(|t| t.id.as_deref() == Some(f.as_str()))
                .is_some_and(|i| self.scenario_state.fired[i] > 0),
            Condition::Has { owner, kind, count } => {
                self.count_standing(*owner, named(kind)) >= *count as u32
            }
            Condition::HasAtMost { owner, kind, count } => {
                self.count_standing(*owner, named(kind)) <= *count as u32
            }
            Condition::Stockpile {
                owner,
                resource,
                amount,
            } => self
                .players
                .get(*owner as usize)
                .is_some_and(|p| p.stockpile[resource.index()] >= *amount),
            Condition::Age { owner, age } => self
                .players
                .get(*owner as usize)
                .is_some_and(|p| p.age >= *age),
            Condition::Researched { owner, tech } => tech::by_name(tech).is_some_and(|t| {
                self.players
                    .get(*owner as usize)
                    .is_some_and(|p| p.has_researched(t))
            }),
            Condition::Inside {
                owner,
                area,
                kind,
                count,
            } => self.count_inside(*owner, area, named(kind)) >= *count as u32,
            Condition::Gone(tag) => match self.scenario_state.tags.get(tag) {
                None => true,
                Some(&id) => self
                    .world
                    .slot(id)
                    .is_none_or(|s| self.world.dying[s.index()] > 0),
            },
        }
    }

    fn act(&mut self, sc: &Scenario, ti: usize, ai: usize, a: &Action) {
        match a {
            Action::Say(_) => self.events.push(Event::Said {
                trigger: ti as u16,
                action: ai as u16,
            }),
            Action::Show(o) => {
                if let Some(i) = Self::objective_index(sc, o) {
                    if self.scenario_state.objectives[i] == ObjectiveStatus::Hidden {
                        self.set_objective(i, ObjectiveStatus::Open);
                    }
                }
            }
            Action::Complete(o) => {
                if let Some(i) = Self::objective_index(sc, o) {
                    self.set_objective(i, ObjectiveStatus::Done);
                }
            }
            Action::Fail(o) => {
                if let Some(i) = Self::objective_index(sc, o) {
                    self.set_objective(i, ObjectiveStatus::Failed);
                }
            }
            Action::Place(pl) => {
                self.place(pl);
                self.recount_population();
            }
            Action::Give {
                owner,
                resource,
                amount,
            } => {
                if let Some(p) = self.players.get_mut(*owner as usize) {
                    let v = &mut p.stockpile[resource.index()];
                    *v = (*v + amount).max(0);
                }
            }
            Action::Reveal(area) => {
                if let Some(f) = self.fog.get_mut(0) {
                    for (x, y) in area.tiles() {
                        f.explore(x, y);
                    }
                }
            }
            Action::Attack { owner, to } => {
                let units: Vec<crate::entity::Slot> = self
                    .world
                    .slots()
                    .filter(|s| {
                        let i = s.index();
                        let info = kinds::info(self.world.kind[i]);
                        self.world.owner[i] == *owner
                            && self.world.dying[i] == 0
                            && self.world.inside[i].is_none()
                            && info.mobile
                            && info.combat.attack > 0
                            && !kinds::gathers(self.world.kind[i])
                    })
                    .collect();
                self.order_attack_move(units, crate::nav::centre(*to));
            }
            Action::Win => self.scenario_state.outcome = Some(Outcome::Won),
            Action::Lose(why) => self.scenario_state.outcome = Some(Outcome::Lost(why.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_and_technology_can_be_named() {
        for k in kinds::all() {
            assert_eq!(kinds::by_name(k.name), Some(k.id), "{}", k.name);
            assert_eq!(
                kinds::by_name(&k.name.to_lowercase().replace(' ', "_")),
                Some(k.id),
                "{}",
                k.name
            );
        }
        for t in tech::all() {
            assert_eq!(tech::by_name(t.name), Some(t.id), "{}", t.name);
        }
        assert_eq!(kinds::by_name("town_center"), Some(kinds::TOWN_CENTER));
        assert_eq!(kinds::by_name("Town  Center"), None);
    }

    #[test]
    fn a_drawn_map_reads_its_letters_and_heights() {
        let rows = |c: &str, n: usize, w: usize| vec![c.repeat(w); n];
        let mut terrain = rows("g", 16, 16);
        terrain[3] = "ggggWWWWggggffff".into();
        let d = DrawnMap {
            terrain: terrain.clone(),
            heights: Vec::new(),
        };
        let map = d.tiles().unwrap();
        assert_eq!((map.width(), map.height()), (16, 16));
        assert_eq!(map.terrain(4, 3), Terrain::DeepWater);
        assert_eq!(map.terrain(15, 3), Terrain::ForestFloor);
        let mut heights = rows("0", 17, 17);
        heights[0] = "01210000000000000".into();
        heights[1] = "00100000000000000".into();
        let d = DrawnMap { terrain, heights };
        assert_eq!(d.tiles().unwrap().corner(2, 0), 2);
        let mut bad = d.clone();
        bad.heights[0] = "03000000000000000".into();
        assert!(bad.tiles().unwrap_err().contains("more than one level"));
        bad.heights[0] = "0".into();
        assert!(bad.tiles().is_err());
        let mut bad = d;
        bad.terrain[0] = "ggggxggggggggggg".into();
        assert!(bad.tiles().unwrap_err().contains("'x'"));
    }
}
