//! The scenario editor (`docs/02` §13, `GD-CAMP-06`): a scenario's ground
//! painted tile by tile and raised corner by corner, what stands where,
//! its sides, its objectives and its triggers, written out as the RON file
//! the campaigns read and played from where it stands.
//!
//! The editor holds the scenario as data and nothing else. The world on
//! screen is a match made from it and never stepped ([`Editor::preview`]),
//! so what is drawn is exactly what will be played. The lists of the
//! scenario, objectives and triggers tools are rebuilt from the scenario
//! each time ([`Editor::rows`]); a click names a row and a chip of it, and
//! [`Editor::click`] finds what that chip stands for and does it.

use sim::kinds::{self, Class, Resource};
use sim::scenario::{
    letter_of, Action, Area, Condition, Control, DrawnMap, Goal, Objective, Placement, Scenario,
    ScenarioMap, Side, Trigger,
};
use sim::{tech, Age, Civ, KindId, PlayerId, Simulation, MAX_ELEVATION};
use std::path::{Path, PathBuf};
use view::editor::{Chip, EditorAction, Item, Panel, Tool, UnitMode};

/// The map letters, as the terrain palette lists them.
pub const LETTERS: [(char, &str); 8] = [
    ('g', "GRASS"),
    ('d', "DIRT"),
    ('a', "DESERT"),
    ('s', "SAND"),
    ('w', "SHALLOW WATER"),
    ('W', "DEEP WATER"),
    ('f', "FOREST"),
    ('n', "SNOW"),
];

/// What a side starts with, as the scenario tool steps through it.
const STOCKPILES: [[i32; 4]; 5] = [
    [0, 0, 0, 0],
    [100, 100, 50, 50],
    sim::DEFAULT_STOCKPILE,
    [500, 500, 300, 300],
    [2000, 2000, 1000, 1000],
];

/// The pages of the units palette.
const PAGES: [&str; 3] = ["UNITS", "BUILDINGS", "NATURE"];

/// A line being typed, and what it is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Typing {
    /// The scenario's name.
    Title,
    /// A paragraph of the briefing.
    Briefing(usize),
    /// A side's name.
    SideName(usize),
    /// What an objective tells the player.
    Objective(usize),
    /// What a trigger's narrator says: trigger, action.
    Say(usize, usize),
    /// Why a trigger loses the scenario: trigger, action.
    Lose(usize, usize),
}

/// A field waiting on the map: a tile, or an area dragged.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pick {
    /// The area of an objective to reach.
    GoalArea(usize),
    /// The area a trigger's condition looks in: trigger, condition.
    InsideArea(usize, usize),
    /// Where a trigger sets units down: trigger, action.
    PlaceAt(usize, usize),
    /// Where a trigger sends a side: trigger, action.
    AttackTo(usize, usize),
    /// What a trigger reveals: trigger, action.
    Reveal(usize, usize),
}

impl Pick {
    /// Whether it wants an area dragged rather than a tile clicked.
    pub fn area(self) -> bool {
        matches!(
            self,
            Pick::GoalArea(_) | Pick::InsideArea(..) | Pick::Reveal(..)
        )
    }

    fn prompt(self) -> &'static str {
        if self.area() {
            "Drag an area on the map (right click or Esc to leave it)"
        } else {
            "Click a tile on the map (right click or Esc to leave it)"
        }
    }
}

/// What a chip of the list does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Act {
    None,
    Type(Typing),
    Pick(Pick),
    Seed,
    Victories,
    AddBriefing,
    DelBriefing(usize),
    SideControl(usize),
    SideCiv(usize),
    SideAge(usize),
    SideStock(usize),
    AddSide,
    DelSide(usize),
    AddObjective,
    DelObjective(usize),
    GoalType(usize),
    GoalParam(usize, u8),
    Hidden(usize),
    Optional(usize),
    AddTrigger,
    DelTrigger(usize),
    Repeat(usize),
    AddWhen(usize),
    AddThen(usize),
    WhenType(usize, usize),
    WhenParam(usize, usize, u8),
    DelWhen(usize, usize),
    ThenType(usize, usize),
    ThenParam(usize, usize, u8),
    DelThen(usize, usize),
}

/// The scenario being edited and the editor's hands.
#[derive(Clone, Debug)]
pub struct Editor {
    /// The scenario, always on a drawn map.
    pub scenario: Scenario,
    /// Where it was read from or last written, if anywhere it may be
    /// written back to.
    pub path: Option<PathBuf>,
    /// The tool in hand.
    pub tool: Tool,
    /// The map letter painted.
    pub letter: char,
    /// Tiles across the brush paints, or corners it lifts: 1, 3 or 5.
    pub brush: i32,
    /// The height tool raises; otherwise it lowers.
    pub raise: bool,
    /// The units palette's page.
    pub page: usize,
    /// The kind set down.
    pub kind: KindId,
    /// Whose it is.
    pub owner: PlayerId,
    /// What a click on the map does with the units tool.
    pub mode: UnitMode,
    /// The first row of the list shown.
    pub top: usize,
    /// A line being typed and what it says so far.
    pub typing: Option<(Typing, String)>,
    /// A field waiting on the map.
    pub pick: Option<Pick>,
    /// Changed since read or written.
    pub dirty: bool,
    /// The world on screen is older than the scenario.
    pub stale: bool,
    /// What just happened, until the next change.
    pub note: Option<String>,
    /// EXIT clicked once with changes unsaved.
    pub confirm_exit: bool,
}

/// A fresh side.
fn side(name: &str, control: Control) -> Side {
    Side {
        name: name.into(),
        control,
        civ: Some(Civ::Egyptians),
        age: Age::Stone,
        stockpile: sim::DEFAULT_STOCKPILE,
        techs: Vec::new(),
        start: None,
    }
}

fn place(kind: KindId, owner: PlayerId, at: (i32, i32), count: u16) -> Placement {
    Placement {
        kind: kinds::info(kind).name.to_string(),
        owner,
        at,
        count,
        tag: None,
    }
}

/// The units, buildings and nature the palette and the fields offer.
fn page_kinds(page: usize) -> Vec<KindId> {
    kinds::all()
        .iter()
        .filter(|k| k.name != "Unknown" && k.name != "Food" && k.name != "Wood")
        .filter(|k| match page {
            0 => k.mobile && k.class != Class::Animal,
            1 => !k.mobile && k.class == Class::Building,
            _ => k.class == Class::Other || k.class == Class::Animal,
        })
        .map(|k| k.id)
        .filter(|&id| !matches!(kinds::info(id).name, "Stone" | "Gold"))
        .collect()
}

/// A side's own kinds: its units and buildings.
fn own_kinds() -> Vec<KindId> {
    let mut v = page_kinds(0);
    v.extend(page_kinds(1));
    v
}

/// Whether a kind is nature's rather than a side's.
fn natural(kind: KindId) -> bool {
    page_kinds(2).contains(&kind)
}

fn step_in<T: Clone + PartialEq>(all: &[T], current: T, dir: i32) -> T {
    let i = all.iter().position(|v| *v == current).unwrap_or(0) as i32;
    let n = all.len().max(1) as i32;
    all[((i + dir).rem_euclid(n)) as usize].clone()
}

fn clock(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn upper(s: &str) -> String {
    s.to_uppercase()
}

impl Editor {
    /// A blank map of grass, `size` tiles across, with the player's side
    /// and an opponent, a Town Center and villagers each, and the one
    /// objective a match needs: defeat the other side.
    pub fn new_map(size: u16) -> Editor {
        let n = size.clamp(16, 256) as i32;
        let (a, b) = (n / 4, n - n / 4 - 3);
        let scenario = Scenario {
            title: "A new scenario".into(),
            briefing: vec!["Write the story here.".into()],
            id: String::new(),
            seed: Some(1),
            map: ScenarioMap::Drawn(DrawnMap {
                terrain: vec!["g".repeat(n as usize); n as usize],
                heights: Vec::new(),
            }),
            sides: vec![
                side("Your people", Control::Player),
                side("The enemy", Control::Computer(1)),
            ],
            standard_start: false,
            placements: vec![
                place(kinds::TOWN_CENTER, 0, (a, a), 1),
                place(kinds::VILLAGER, 0, (a + 4, a + 4), 3),
                place(kinds::TOWN_CENTER, 1, (b, b), 1),
                place(kinds::VILLAGER, 1, (b - 2, b - 2), 3),
            ],
            objectives: vec![Objective {
                id: "o1".into(),
                text: "Defeat the enemy".into(),
                goal: Goal::Destroy {
                    owner: 1,
                    kind: None,
                },
                hidden: false,
                optional: false,
            }],
            triggers: Vec::new(),
            skirmish_victories: false,
        };
        Editor::open(scenario, None)
    }

    /// Opens a scenario. One on a generated map is baked onto a drawn one
    /// first: its ground, and what the generator set on it, as placements.
    pub fn open(scenario: Scenario, path: Option<PathBuf>) -> Editor {
        let scenario = match scenario.map {
            ScenarioMap::Drawn(_) => scenario,
            ScenarioMap::Generated { .. } => bake(scenario),
        };
        Editor {
            scenario,
            path,
            tool: Tool::Terrain,
            letter: 'g',
            brush: 3,
            raise: true,
            page: 0,
            kind: kinds::VILLAGER,
            owner: 0,
            mode: UnitMode::Place,
            top: 0,
            typing: None,
            pick: None,
            dirty: false,
            stale: true,
            note: None,
            confirm_exit: false,
        }
    }

    /// The match the scenario makes, for drawing and for playing.
    pub fn preview(&self) -> Simulation {
        Simulation::new(self.scenario.match_seed(), self.scenario.config())
    }

    fn drawn(&mut self) -> &mut DrawnMap {
        match &mut self.scenario.map {
            ScenarioMap::Drawn(d) => d,
            ScenarioMap::Generated { .. } => unreachable!("opened onto a drawn map"),
        }
    }

    /// Tiles across and down.
    pub fn size(&self) -> (i32, i32) {
        match &self.scenario.map {
            ScenarioMap::Drawn(d) => (
                d.terrain.first().map_or(0, |r| r.chars().count()) as i32,
                d.terrain.len() as i32,
            ),
            ScenarioMap::Generated { size, .. } => (*size as i32, *size as i32),
        }
    }

    fn on_map(&self, (x, y): (i32, i32)) -> bool {
        let (w, h) = self.size();
        x >= 0 && y >= 0 && x < w && y < h
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.stale = true;
        self.note = None;
        self.confirm_exit = false;
    }

    /// The tiles a brush centred on `(x, y)` covers.
    pub fn brushed(&self, (x, y): (i32, i32)) -> Vec<(i32, i32)> {
        let r = (self.brush - 1) / 2;
        let mut out = Vec::new();
        for dy in -r..=r {
            for dx in -r..=r {
                if self.on_map((x + dx, y + dy)) {
                    out.push((x + dx, y + dy));
                }
            }
        }
        out
    }

    /// Paints the brush's tiles with the chosen letter. True if any
    /// changed.
    pub fn paint(&mut self, at: (i32, i32)) -> bool {
        let letter = self.letter;
        let tiles = self.brushed(at);
        let d = self.drawn();
        let mut changed = false;
        for (x, y) in tiles {
            let row: Vec<char> = d.terrain[y as usize].chars().collect();
            if row[x as usize] != letter {
                let mut row = row;
                row[x as usize] = letter;
                d.terrain[y as usize] = row.into_iter().collect();
                changed = true;
            }
        }
        if changed {
            self.changed();
        }
        changed
    }

    /// The letter under a tile, for the right click that picks it up.
    pub fn letter_at(&self, (x, y): (i32, i32)) -> Option<char> {
        match &self.scenario.map {
            ScenarioMap::Drawn(d) => d.terrain.get(y as usize)?.chars().nth(x as usize),
            ScenarioMap::Generated { .. } => None,
        }
    }

    /// Raises (or lowers, `up` false) the corners of the brush about the
    /// corner `(cx, cy)` a level, and the ground around them with them so
    /// no two neighbouring corners are more than a level apart. True if
    /// any changed.
    pub fn lift(&mut self, (cx, cy): (i32, i32), up: bool) -> bool {
        let (w, h) = self.size();
        let r = (self.brush - 1) / 2;
        let d = self.drawn();
        if d.heights.is_empty() {
            d.heights = vec!["0".repeat(w as usize + 1); h as usize + 1];
        }
        let mut grid: Vec<Vec<i32>> = d
            .heights
            .iter()
            .map(|row| {
                row.chars()
                    .map(|c| c.to_digit(10).unwrap_or(0) as i32)
                    .collect()
            })
            .collect();
        let before = grid.clone();
        let mut fixed = Vec::new();
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x > w || y > h {
                    continue;
                }
                let v = &mut grid[y as usize][x as usize];
                *v = (*v + if up { 1 } else { -1 }).clamp(0, MAX_ELEVATION as i32);
                fixed.push((x, y));
            }
        }
        // The ground around follows, a level a corner, out to where it
        // already fits.
        let mut moved = true;
        while moved {
            moved = false;
            for y in 0..=h {
                for x in 0..=w {
                    for (dx, dy) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx > w || ny < 0 || ny > h {
                            continue;
                        }
                        let a = grid[y as usize][x as usize];
                        let b = grid[ny as usize][nx as usize];
                        if (a - b).abs() <= 1 {
                            continue;
                        }
                        // The corner not being worked on gives way.
                        let (mx, my, to) = if up {
                            if a > b {
                                (nx, ny, a - 1)
                            } else {
                                (x, y, b - 1)
                            }
                        } else if a < b {
                            (nx, ny, a + 1)
                        } else {
                            (x, y, b + 1)
                        };
                        if fixed.contains(&(mx, my)) {
                            continue;
                        }
                        grid[my as usize][mx as usize] = to;
                        moved = true;
                    }
                }
            }
        }
        if grid == before {
            return false;
        }
        let d = self.drawn();
        d.heights = grid
            .iter()
            .map(|row| {
                row.iter()
                    .map(|v| char::from_digit(*v as u32, 10).unwrap_or('0'))
                    .collect()
            })
            .collect();
        self.changed();
        true
    }

    /// Where a building of `kind` centred on the tile clicked has its
    /// corner.
    pub fn anchor(kind: KindId, (x, y): (i32, i32)) -> (i32, i32) {
        let info = kinds::info(kind);
        if info.mobile {
            (x, y)
        } else {
            let fp = info.footprint.max(1) as i32;
            (x - (fp - 1) / 2, y - (fp - 1) / 2)
        }
    }

    /// Sets the chosen kind down on a tile: nature's, or the chosen side's.
    pub fn place(&mut self, at: (i32, i32)) -> bool {
        if !self.on_map(at) {
            return false;
        }
        let owner = if natural(self.kind) {
            kinds::GAIA
        } else {
            self.owner
        };
        let at = Editor::anchor(self.kind, at);
        self.scenario
            .placements
            .push(place(self.kind, owner, at, 1));
        self.changed();
        true
    }

    /// The placement standing on a tile, the last set down first.
    fn placement_at(&self, (x, y): (i32, i32)) -> Option<usize> {
        self.scenario.placements.iter().rposition(|p| {
            let Some(kind) = kinds::by_name(&p.kind) else {
                return false;
            };
            let info = kinds::info(kind);
            if info.mobile {
                // A group spreads about its tile.
                let r = if p.count > 1 { 1 } else { 0 };
                (x - p.at.0).abs() <= r && (y - p.at.1).abs() <= r
            } else {
                let fp = info.footprint.max(1) as i32;
                if p.count > 1 {
                    // A row of buildings.
                    x >= p.at.0
                        && x < p.at.0 + fp * p.count as i32
                        && y >= p.at.1
                        && y < p.at.1 + fp
                } else {
                    x >= p.at.0 && x < p.at.0 + fp && y >= p.at.1 && y < p.at.1 + fp
                }
            }
        })
    }

    /// Takes up what stands on a tile. True if anything did.
    pub fn erase(&mut self, at: (i32, i32)) -> bool {
        match self.placement_at(at) {
            Some(i) => {
                self.scenario.placements.remove(i);
                self.changed();
                true
            }
            None => false,
        }
    }

    /// Names what stands on a tile, for a trigger to wait on its loss, or
    /// stops naming it. Returns the name given.
    pub fn tag(&mut self, at: (i32, i32)) -> Option<String> {
        let i = self.placement_at(at)?;
        if self.scenario.placements[i].tag.take().is_some() {
            self.changed();
            self.note = Some("No longer named".into());
            return None;
        }
        let base = self.scenario.placements[i].kind.to_lowercase();
        let mut n = 1;
        let name = loop {
            let name = format!("{base} {n}");
            if !self
                .scenario
                .placements
                .iter()
                .any(|p| p.tag.as_deref() == Some(name.as_str()))
            {
                break name;
            }
            n += 1;
        };
        self.scenario.placements[i].tag = Some(name.clone());
        self.changed();
        self.note = Some(format!("Named {name}: a trigger may wait on it being gone"));
        Some(name)
    }

    /// The field waiting on the map gets the tile clicked, or the area
    /// dragged from `from` to `to`.
    pub fn picked(&mut self, from: (i32, i32), to: (i32, i32)) {
        let Some(pick) = self.pick.take() else {
            return;
        };
        let area = Area {
            from: (from.0.min(to.0), from.1.min(to.1)),
            to: (from.0.max(to.0), from.1.max(to.1)),
        };
        let sc = &mut self.scenario;
        match pick {
            Pick::GoalArea(o) => {
                if let Some(Goal::Reach { area: a, .. }) =
                    sc.objectives.get_mut(o).map(|o| &mut o.goal)
                {
                    *a = area;
                }
            }
            Pick::InsideArea(t, c) => {
                if let Some(Condition::Inside { area: a, .. }) =
                    sc.triggers.get_mut(t).and_then(|t| t.when.get_mut(c))
                {
                    *a = area;
                }
            }
            Pick::Reveal(t, a) => {
                if let Some(Action::Reveal(r)) =
                    sc.triggers.get_mut(t).and_then(|t| t.then.get_mut(a))
                {
                    *r = area;
                }
            }
            Pick::PlaceAt(t, a) => {
                if let Some(Action::Place(p)) =
                    sc.triggers.get_mut(t).and_then(|t| t.then.get_mut(a))
                {
                    p.at = to;
                }
            }
            Pick::AttackTo(t, a) => {
                if let Some(Action::Attack { to: at, .. }) =
                    sc.triggers.get_mut(t).and_then(|t| t.then.get_mut(a))
                {
                    *at = to;
                }
            }
        }
        self.changed();
    }

    /// The typed line is kept.
    pub fn keep_typing(&mut self) {
        let Some((what, line)) = self.typing.take() else {
            return;
        };
        let line = line.trim().to_string();
        let sc = &mut self.scenario;
        let slot: Option<&mut String> = match what {
            Typing::Title => Some(&mut sc.title),
            Typing::Briefing(i) => sc.briefing.get_mut(i),
            Typing::SideName(i) => sc.sides.get_mut(i).map(|s| &mut s.name),
            Typing::Objective(i) => sc.objectives.get_mut(i).map(|o| &mut o.text),
            Typing::Say(t, a) | Typing::Lose(t, a) => {
                match sc.triggers.get_mut(t).and_then(|t| t.then.get_mut(a)) {
                    Some(Action::Say(s)) | Some(Action::Lose(s)) => Some(s),
                    _ => None,
                }
            }
        };
        if let Some(slot) = slot {
            if *slot != line {
                *slot = line;
                self.changed();
            }
        }
    }

    /// What a typed line is for, as the editor asks for it.
    pub fn typing_label(what: Typing) -> String {
        match what {
            Typing::Title => "The scenario's name".into(),
            Typing::Briefing(i) => format!("Briefing paragraph {}", i + 1),
            Typing::SideName(i) => format!("Side {}'s name", i + 1),
            Typing::Objective(i) => format!("Objective {}", i + 1),
            Typing::Say(t, _) => format!("What trigger {} says", t + 1),
            Typing::Lose(t, _) => format!("Why trigger {} loses it", t + 1),
        }
    }

    fn current_text(&self, what: Typing) -> String {
        let sc = &self.scenario;
        match what {
            Typing::Title => sc.title.clone(),
            Typing::Briefing(i) => sc.briefing.get(i).cloned().unwrap_or_default(),
            Typing::SideName(i) => sc.sides.get(i).map(|s| s.name.clone()).unwrap_or_default(),
            Typing::Objective(i) => sc
                .objectives
                .get(i)
                .map(|o| o.text.clone())
                .unwrap_or_default(),
            Typing::Say(t, a) | Typing::Lose(t, a) => {
                match sc.triggers.get(t).and_then(|t| t.then.get(a)) {
                    Some(Action::Say(s)) | Some(Action::Lose(s)) => s.clone(),
                    _ => String::new(),
                }
            }
        }
    }

    // --- The list --------------------------------------------------------

    fn side_label(&self, p: PlayerId) -> String {
        if p == kinds::GAIA {
            return "NATURE".into();
        }
        match self.scenario.sides.get(p as usize) {
            Some(s) => format!("{} {}", p + 1, upper(&s.name)),
            None => format!("NO SIDE {}", p + 1),
        }
    }

    fn objective_label(&self, id: &str) -> String {
        match self.scenario.objectives.iter().position(|o| o.id == id) {
            Some(i) => format!("OBJECTIVE {}", i + 1),
            None => "NO OBJECTIVE".into(),
        }
    }

    fn trigger_label(&self, id: &str) -> String {
        match self
            .scenario
            .triggers
            .iter()
            .position(|t| t.id.as_deref() == Some(id))
        {
            Some(i) => format!("TRIGGER {}", i + 1),
            None => "NO TRIGGER".into(),
        }
    }

    fn kind_label(kind: &Option<String>, none: &str) -> String {
        match kind {
            Some(k) => upper(k),
            None => none.into(),
        }
    }

    fn area_label(a: &Area) -> String {
        format!("AREA {},{} TO {},{}", a.from.0, a.from.1, a.to.0, a.to.1)
    }

    fn goal_chips(&self, i: usize, g: &Goal) -> Vec<(Chip, Act)> {
        let b = |label: String, n: u8| (button(label), Act::GoalParam(i, n));
        match g {
            Goal::Stockpile { resource, amount } => vec![
                b(upper(resource.name()), 0),
                b(format!("{amount} IN STORE"), 1),
            ],
            Goal::Have { kind, count } => {
                vec![b(upper(kind), 0), b(format!("AT LEAST {count}"), 1)]
            }
            Goal::Age(age) => vec![b(upper(age.name()), 0)],
            Goal::Destroy { owner, kind } => vec![
                b(self.side_label(*owner), 0),
                b(Editor::kind_label(kind, "EVERYTHING"), 1),
            ],
            Goal::Reach { area, kind, count } => vec![
                (
                    pick_chip(
                        Editor::area_label(area),
                        self.pick == Some(Pick::GoalArea(i)),
                    ),
                    Act::Pick(Pick::GoalArea(i)),
                ),
                b(Editor::kind_label(kind, "ANY UNIT"), 1),
                b(format!("AT LEAST {count}"), 2),
            ],
            Goal::Survive { seconds } => vec![b(format!("FOR {}", clock(*seconds)), 0)],
            Goal::Scripted => vec![(Chip::words("DONE BY A TRIGGER"), Act::None)],
        }
    }

    fn when_chips(&self, t: usize, c: usize, cond: &Condition) -> Vec<(Chip, Act)> {
        let b = |label: String, n: u8| (button(label), Act::WhenParam(t, c, n));
        match cond {
            Condition::After(s) => vec![b(format!("AT {}", clock(*s)), 0)],
            Condition::Done(o) | Condition::Failed(o) => vec![b(self.objective_label(o), 0)],
            Condition::Fired(f) => vec![b(self.trigger_label(f), 0)],
            Condition::Since { trigger, seconds } => vec![
                b(self.trigger_label(trigger), 0),
                b(format!("{} AFTER", clock(*seconds)), 1),
            ],
            Condition::Has { owner, kind, count } => vec![
                b(self.side_label(*owner), 0),
                b(Editor::kind_label(kind, "ANYTHING"), 1),
                b(format!("AT LEAST {count}"), 2),
            ],
            Condition::HasAtMost { owner, kind, count } => vec![
                b(self.side_label(*owner), 0),
                b(Editor::kind_label(kind, "ANYTHING"), 1),
                b(format!("AT MOST {count}"), 2),
            ],
            Condition::Stockpile {
                owner,
                resource,
                amount,
            } => vec![
                b(self.side_label(*owner), 0),
                b(upper(resource.name()), 1),
                b(format!("AT LEAST {amount}"), 2),
            ],
            Condition::Age { owner, age } => {
                vec![b(self.side_label(*owner), 0), b(upper(age.name()), 1)]
            }
            Condition::Researched { owner, tech } => {
                vec![b(self.side_label(*owner), 0), b(upper(tech), 1)]
            }
            Condition::Inside {
                owner,
                area,
                kind,
                count,
            } => vec![
                b(self.side_label(*owner), 0),
                (
                    pick_chip(
                        Editor::area_label(area),
                        self.pick == Some(Pick::InsideArea(t, c)),
                    ),
                    Act::Pick(Pick::InsideArea(t, c)),
                ),
                b(Editor::kind_label(kind, "ANY UNIT"), 2),
                b(format!("AT LEAST {count}"), 3),
            ],
            Condition::Gone(tag) => vec![b(upper(tag), 0)],
            Condition::Any(_) | Condition::Not(_) => vec![(
                Chip::words(fit_words(
                    &upper(&format!("{cond:?}").replace('"', "'")),
                    60,
                )),
                Act::None,
            )],
        }
    }

    fn then_chips(&self, t: usize, a: usize, act: &Action) -> Vec<(Chip, Act)> {
        let b = |label: String, n: u8| (button(label), Act::ThenParam(t, a, n));
        match act {
            Action::Say(s) => vec![(
                button(format!("'{}'", upper(s))),
                Act::Type(Typing::Say(t, a)),
            )],
            Action::Lose(s) => vec![(
                button(format!("'{}'", upper(s))),
                Act::Type(Typing::Lose(t, a)),
            )],
            Action::Show(o) | Action::Complete(o) | Action::Fail(o) => {
                vec![b(self.objective_label(o), 0)]
            }
            Action::Place(p) => vec![
                b(self.side_label(p.owner), 0),
                b(upper(&p.kind), 1),
                b(format!("{} OF THEM", p.count), 2),
                (
                    pick_chip(
                        format!("AT {},{}", p.at.0, p.at.1),
                        self.pick == Some(Pick::PlaceAt(t, a)),
                    ),
                    Act::Pick(Pick::PlaceAt(t, a)),
                ),
            ],
            Action::Give {
                owner,
                resource,
                amount,
            } => vec![
                b(self.side_label(*owner), 0),
                b(upper(resource.name()), 1),
                b(format!("{amount}"), 2),
            ],
            Action::Reveal(area) => vec![(
                pick_chip(
                    Editor::area_label(area),
                    self.pick == Some(Pick::Reveal(t, a)),
                ),
                Act::Pick(Pick::Reveal(t, a)),
            )],
            Action::Attack { owner, to } => vec![
                b(self.side_label(*owner), 0),
                (
                    pick_chip(
                        format!("TO {},{}", to.0, to.1),
                        self.pick == Some(Pick::AttackTo(t, a)),
                    ),
                    Act::Pick(Pick::AttackTo(t, a)),
                ),
            ],
            Action::Win => Vec::new(),
        }
    }

    /// The list of the tool in hand, a row of chips a line.
    fn rows(&self, problems: &[String]) -> Vec<Vec<(Chip, Act)>> {
        let sc = &self.scenario;
        let mut rows: Vec<Vec<(Chip, Act)>> = Vec::new();
        match self.tool {
            Tool::Scenario => {
                rows.push(vec![
                    (heading("TITLE"), Act::None),
                    (button(upper(&sc.title)), Act::Type(Typing::Title)),
                ]);
                rows.push(vec![
                    (heading("SEED"), Act::None),
                    (
                        button(sc.seed.map_or("FROM THE NAME".into(), |s| s.to_string())),
                        Act::Seed,
                    ),
                ]);
                rows.push(vec![
                    (heading("SKIRMISH VICTORIES"), Act::None),
                    (
                        button(if sc.skirmish_victories {
                            "HOLD"
                        } else {
                            "DO NOT HOLD"
                        }),
                        Act::Victories,
                    ),
                ]);
                for (i, p) in sc.briefing.iter().enumerate() {
                    rows.push(vec![
                        (heading(format!("BRIEFING {}", i + 1)), Act::None),
                        (
                            button(fit_words(&upper(p), 70)),
                            Act::Type(Typing::Briefing(i)),
                        ),
                        (button("X"), Act::DelBriefing(i)),
                    ]);
                }
                rows.push(vec![(button("ADD A BRIEFING PARAGRAPH"), Act::AddBriefing)]);
                for (i, s) in sc.sides.iter().enumerate() {
                    let mut row = vec![
                        (heading(format!("SIDE {}", i + 1)), Act::None),
                        (button(upper(&s.name)), Act::Type(Typing::SideName(i))),
                        (button(control_label(s.control)), Act::SideControl(i)),
                        (
                            button(s.civ.map_or("ANY CIVILIZATION".into(), |c| upper(c.name()))),
                            Act::SideCiv(i),
                        ),
                        (button(upper(s.age.name())), Act::SideAge(i)),
                        (
                            button(format!(
                                "{}F {}W {}S {}G",
                                s.stockpile[0], s.stockpile[1], s.stockpile[2], s.stockpile[3]
                            )),
                            Act::SideStock(i),
                        ),
                    ];
                    if i > 0 && i + 1 == sc.sides.len() {
                        row.push((button("X"), Act::DelSide(i)));
                    }
                    rows.push(row);
                }
                if sc.sides.len() < sim::MAX_PLAYERS {
                    rows.push(vec![(button("ADD A SIDE"), Act::AddSide)]);
                }
            }
            Tool::Objectives => {
                for (i, o) in sc.objectives.iter().enumerate() {
                    let mut row = vec![
                        (heading(format!("{}.", i + 1)), Act::None),
                        (button(upper(&o.text)), Act::Type(Typing::Objective(i))),
                        (button(goal_name(&o.goal)), Act::GoalType(i)),
                    ];
                    row.extend(self.goal_chips(i, &o.goal));
                    row.push((
                        button(if o.hidden { "HIDDEN AT FIRST" } else { "SHOWN" }),
                        Act::Hidden(i),
                    ));
                    row.push((
                        button(if o.optional { "OPTIONAL" } else { "NEEDED" }),
                        Act::Optional(i),
                    ));
                    row.push((button("X"), Act::DelObjective(i)));
                    rows.push(row);
                }
                rows.push(vec![(button("ADD AN OBJECTIVE"), Act::AddObjective)]);
            }
            Tool::Triggers => {
                for (t, tr) in sc.triggers.iter().enumerate() {
                    rows.push(vec![
                        (heading(format!("TRIGGER {}", t + 1)), Act::None),
                        (
                            button(if tr.repeat { "EVERY SECOND" } else { "ONCE" }),
                            Act::Repeat(t),
                        ),
                        (button("ADD A CONDITION"), Act::AddWhen(t)),
                        (button("ADD AN ACTION"), Act::AddThen(t)),
                        (button("X"), Act::DelTrigger(t)),
                    ]);
                    if tr.when.is_empty() {
                        rows.push(vec![(
                            Chip::words("   WHEN THE SCENARIO STARTS"),
                            Act::None,
                        )]);
                    }
                    for (c, cond) in tr.when.iter().enumerate() {
                        let mut row = vec![
                            (Chip::words("   WHEN"), Act::None),
                            (button(condition_name(cond)), Act::WhenType(t, c)),
                        ];
                        row.extend(self.when_chips(t, c, cond));
                        row.push((button("X"), Act::DelWhen(t, c)));
                        rows.push(row);
                    }
                    for (a, act) in tr.then.iter().enumerate() {
                        let mut row = vec![
                            (Chip::words("   THEN"), Act::None),
                            (button(action_name(act)), Act::ThenType(t, a)),
                        ];
                        row.extend(self.then_chips(t, a, act));
                        row.push((button("X"), Act::DelThen(t, a)));
                        rows.push(row);
                    }
                }
                rows.push(vec![(button("ADD A TRIGGER"), Act::AddTrigger)]);
            }
            Tool::Check => {
                if problems.is_empty() {
                    rows.push(vec![(
                        Chip::words("NOTHING IS WRONG: IT CAN BE PLAYED"),
                        Act::None,
                    )]);
                }
                for p in problems {
                    rows.push(vec![(Chip::words(upper(p)), Act::None)]);
                }
            }
            Tool::Terrain | Tool::Height | Tool::Units => {}
        }
        rows
    }

    /// A click on chip `chip` of row `row`, left (`dir` 1) or right (-1),
    /// with shift for big steps.
    pub fn click(&mut self, row: usize, chip: usize, dir: i32, big: bool) {
        let act = self
            .rows(&[])
            .get(row)
            .and_then(|r| r.get(chip))
            .map_or(Act::None, |(_, a)| *a);
        self.act(act, dir, big);
    }

    fn act(&mut self, act: Act, dir: i32, big: bool) {
        let n_obj = self.scenario.objectives.len();
        let sides = self.scenario.sides.len();
        let middle = {
            let (w, h) = self.size();
            (w / 2, h / 2)
        };
        let step = |v: i32, by: i32, min: i32, max: i32| {
            (v + dir * by * if big { 10 } else { 1 }).clamp(min, max)
        };
        let owner_step = |p: PlayerId, gaia: bool| -> PlayerId {
            let mut all: Vec<PlayerId> = (0..sides as PlayerId).collect();
            if gaia {
                all.push(kinds::GAIA);
            }
            step_in(&all, p, dir)
        };
        let obj_ids: Vec<String> = self
            .scenario
            .objectives
            .iter()
            .map(|o| o.id.clone())
            .collect();
        let trig_ids: Vec<String> = self
            .scenario
            .triggers
            .iter()
            .filter_map(|t| t.id.clone())
            .collect();
        let tags: Vec<String> = self
            .scenario
            .placements
            .iter()
            .filter_map(|p| p.tag.clone())
            .collect();
        let kind_step = |k: &Option<String>, list: &[KindId]| -> Option<String> {
            let mut all: Vec<Option<String>> = vec![None];
            all.extend(
                list.iter()
                    .map(|&id| Some(kinds::info(id).name.to_string())),
            );
            let cur = k
                .as_ref()
                .and_then(|n| kinds::by_name(n))
                .map(|id| kinds::info(id).name.to_string());
            step_in(&all, cur, dir)
        };
        let name_step = |k: &str, list: &[KindId]| -> String {
            let names: Vec<&str> = list.iter().map(|&id| kinds::info(id).name).collect();
            let cur = kinds::by_name(k).map_or("", |id| kinds::info(id).name);
            step_in(&names, cur, dir).to_string()
        };
        let id_step = |cur: &str, ids: &[String]| -> String {
            if ids.is_empty() {
                return cur.to_string();
            }
            step_in(ids, cur.to_string(), dir)
        };
        let res_step = |r: Resource| step_in(&Resource::ALL, r, dir);
        let age_step = |a: Age| step_in(&Age::ALL, a, dir);
        let first_obj = obj_ids.first().cloned().unwrap_or_default();
        let sc = &mut self.scenario;
        match act {
            Act::None => return,
            Act::Type(what) => {
                let text = self.current_text(what);
                self.typing = Some((what, text));
                return;
            }
            Act::Pick(p) => {
                self.pick = if self.pick == Some(p) { None } else { Some(p) };
                return;
            }
            Act::Seed => {
                let s = sc.seed.unwrap_or(1) as i64;
                sc.seed = Some((s + dir as i64 * if big { 100 } else { 1 }).max(1) as u64);
            }
            Act::Victories => sc.skirmish_victories = !sc.skirmish_victories,
            Act::AddBriefing => sc.briefing.push("More of the story.".into()),
            Act::DelBriefing(i) => {
                if i < sc.briefing.len() {
                    sc.briefing.remove(i);
                }
            }
            Act::SideControl(i) => {
                if i > 0 {
                    let all = [
                        Control::Computer(0),
                        Control::Computer(1),
                        Control::Computer(2),
                        Control::Computer(3),
                        Control::Scripted,
                    ];
                    sc.sides[i].control = step_in(&all, sc.sides[i].control, dir);
                }
            }
            Act::SideCiv(i) => {
                let mut all: Vec<Option<Civ>> = vec![None];
                all.extend(Civ::ALL.iter().map(|c| Some(*c)));
                sc.sides[i].civ = step_in(&all, sc.sides[i].civ, dir);
            }
            Act::SideAge(i) => sc.sides[i].age = age_step(sc.sides[i].age),
            Act::SideStock(i) => {
                sc.sides[i].stockpile = step_in(&STOCKPILES, sc.sides[i].stockpile, dir)
            }
            Act::AddSide => {
                if sides < sim::MAX_PLAYERS {
                    sc.sides
                        .push(side(&format!("Side {}", sides + 1), Control::Computer(1)));
                }
            }
            Act::DelSide(i) => {
                if i > 0 && i + 1 == sides {
                    sc.sides.pop();
                    let gone = i as PlayerId;
                    sc.placements.retain(|p| p.owner != gone);
                    if self.owner >= gone && self.owner != kinds::GAIA {
                        self.owner = 0;
                    }
                }
            }
            Act::AddObjective => {
                let id = fresh_id("o", &obj_ids);
                sc.objectives.push(Objective {
                    id,
                    text: "A new objective".into(),
                    goal: Goal::Scripted,
                    hidden: false,
                    optional: false,
                });
            }
            Act::DelObjective(i) => {
                if i < n_obj {
                    sc.objectives.remove(i);
                }
            }
            Act::GoalType(i) => {
                let all = [
                    "DESTROY",
                    "HAVE",
                    "STOCKPILE",
                    "REACH AN AGE",
                    "REACH AN AREA",
                    "SURVIVE",
                    "SCRIPTED",
                ];
                let next = step_in(&all, goal_name(&sc.objectives[i].goal), dir);
                sc.objectives[i].goal = match next {
                    "DESTROY" => Goal::Destroy {
                        owner: 1.min(sides as PlayerId - 1),
                        kind: None,
                    },
                    "HAVE" => Goal::Have {
                        kind: "Villager".into(),
                        count: 10,
                    },
                    "STOCKPILE" => Goal::Stockpile {
                        resource: Resource::Food,
                        amount: 500,
                    },
                    "REACH AN AGE" => Goal::Age(Age::Tool),
                    "REACH AN AREA" => Goal::Reach {
                        area: Area {
                            from: (middle.0 - 2, middle.1 - 2),
                            to: (middle.0 + 2, middle.1 + 2),
                        },
                        kind: None,
                        count: 1,
                    },
                    "SURVIVE" => Goal::Survive { seconds: 600 },
                    _ => Goal::Scripted,
                };
            }
            Act::GoalParam(i, n) => match &mut sc.objectives[i].goal {
                Goal::Stockpile { resource, amount } => match n {
                    0 => *resource = res_step(*resource),
                    _ => *amount = step(*amount, 50, 0, 100_000),
                },
                Goal::Have { kind, count } => match n {
                    0 => *kind = name_step(kind, &own_kinds()),
                    _ => *count = step(*count as i32, 1, 1, 500) as u16,
                },
                Goal::Age(a) => *a = age_step(*a),
                Goal::Destroy { owner, kind } => match n {
                    0 => *owner = owner_step(*owner, false),
                    _ => *kind = kind_step(kind, &own_kinds()),
                },
                Goal::Reach { kind, count, .. } => match n {
                    1 => *kind = kind_step(kind, &page_kinds(0)),
                    _ => *count = step(*count as i32, 1, 1, 500) as u16,
                },
                Goal::Survive { seconds } => {
                    *seconds = step(*seconds as i32, 30, 10, 36_000) as u32
                }
                Goal::Scripted => {}
            },
            Act::Hidden(i) => sc.objectives[i].hidden = !sc.objectives[i].hidden,
            Act::Optional(i) => sc.objectives[i].optional = !sc.objectives[i].optional,
            Act::AddTrigger => {
                let id = fresh_id("t", &trig_ids);
                sc.triggers.push(Trigger {
                    id: Some(id),
                    when: vec![Condition::After(10)],
                    then: vec![Action::Say("Something happens.".into())],
                    repeat: false,
                });
            }
            Act::DelTrigger(t) => {
                if t < sc.triggers.len() {
                    sc.triggers.remove(t);
                }
            }
            Act::Repeat(t) => sc.triggers[t].repeat = !sc.triggers[t].repeat,
            Act::AddWhen(t) => sc.triggers[t].when.push(Condition::After(60)),
            Act::AddThen(t) => sc.triggers[t]
                .then
                .push(Action::Say("Something happens.".into())),
            Act::DelWhen(t, c) => {
                if c < sc.triggers[t].when.len() {
                    sc.triggers[t].when.remove(c);
                }
            }
            Act::DelThen(t, a) => {
                if a < sc.triggers[t].then.len() {
                    sc.triggers[t].then.remove(a);
                }
            }
            Act::WhenType(t, c) => {
                let all = [
                    "AFTER",
                    "OBJECTIVE DONE",
                    "OBJECTIVE FAILED",
                    "TRIGGER FIRED",
                    "SINCE A TRIGGER",
                    "HAS",
                    "HAS AT MOST",
                    "STOCKPILE",
                    "AGE",
                    "RESEARCHED",
                    "UNITS IN AN AREA",
                    "NAMED ONE GONE",
                ];
                let own = sc.triggers[t].id.clone().unwrap_or_default();
                let other = trig_ids.iter().find(|i| **i != own).cloned().unwrap_or(own);
                let next = step_in(&all, condition_name(&sc.triggers[t].when[c]), dir);
                sc.triggers[t].when[c] = match next {
                    "AFTER" => Condition::After(60),
                    "OBJECTIVE DONE" => Condition::Done(first_obj),
                    "OBJECTIVE FAILED" => Condition::Failed(first_obj),
                    "TRIGGER FIRED" => Condition::Fired(other),
                    "SINCE A TRIGGER" => Condition::Since {
                        trigger: other,
                        seconds: 60,
                    },
                    "HAS" => Condition::Has {
                        owner: 0,
                        kind: None,
                        count: 1,
                    },
                    "HAS AT MOST" => Condition::HasAtMost {
                        owner: 1.min(sides as PlayerId - 1),
                        kind: None,
                        count: 0,
                    },
                    "STOCKPILE" => Condition::Stockpile {
                        owner: 0,
                        resource: Resource::Food,
                        amount: 500,
                    },
                    "AGE" => Condition::Age {
                        owner: 0,
                        age: Age::Tool,
                    },
                    "RESEARCHED" => Condition::Researched {
                        owner: 0,
                        tech: tech::all()
                            .iter()
                            .find(|t| t.advances_age().is_none())
                            .map_or("Woodworking", |t| t.name)
                            .to_string(),
                    },
                    "UNITS IN AN AREA" => Condition::Inside {
                        owner: 0,
                        area: Area {
                            from: (middle.0 - 2, middle.1 - 2),
                            to: (middle.0 + 2, middle.1 + 2),
                        },
                        kind: None,
                        count: 1,
                    },
                    _ => Condition::Gone(tags.first().cloned().unwrap_or_default()),
                };
            }
            Act::WhenParam(t, c, n) => match &mut sc.triggers[t].when[c] {
                Condition::After(s) => *s = step(*s as i32, 10, 0, 36_000) as u32,
                Condition::Done(o) | Condition::Failed(o) => *o = id_step(o, &obj_ids),
                Condition::Fired(f) => *f = id_step(f, &trig_ids),
                Condition::Since { trigger, seconds } => match n {
                    0 => *trigger = id_step(trigger, &trig_ids),
                    _ => *seconds = step(*seconds as i32, 10, 0, 36_000) as u32,
                },
                Condition::Has { owner, kind, count } => match n {
                    0 => *owner = owner_step(*owner, false),
                    1 => *kind = kind_step(kind, &own_kinds()),
                    _ => *count = step(*count as i32, 1, 0, 500) as u16,
                },
                Condition::HasAtMost { owner, kind, count } => match n {
                    0 => *owner = owner_step(*owner, false),
                    1 => *kind = kind_step(kind, &own_kinds()),
                    _ => *count = step(*count as i32, 1, 0, 500) as u16,
                },
                Condition::Stockpile {
                    owner,
                    resource,
                    amount,
                } => match n {
                    0 => *owner = owner_step(*owner, false),
                    1 => *resource = res_step(*resource),
                    _ => *amount = step(*amount, 50, 0, 100_000),
                },
                Condition::Age { owner, age } => match n {
                    0 => *owner = owner_step(*owner, false),
                    _ => *age = age_step(*age),
                },
                Condition::Researched { owner, tech: name } => match n {
                    0 => *owner = owner_step(*owner, false),
                    _ => {
                        let names: Vec<&str> = tech::all()
                            .iter()
                            .filter(|t| t.advances_age().is_none())
                            .map(|t| t.name)
                            .collect();
                        let cur = tech::by_name(name)
                            .and_then(tech::info)
                            .map_or("", |t| t.name);
                        *name = step_in(&names, cur, dir).to_string();
                    }
                },
                Condition::Inside {
                    owner, kind, count, ..
                } => match n {
                    0 => *owner = owner_step(*owner, false),
                    2 => *kind = kind_step(kind, &page_kinds(0)),
                    _ => *count = step(*count as i32, 1, 1, 500) as u16,
                },
                Condition::Gone(tag) => *tag = id_step(tag, &tags),
                Condition::Any(_) | Condition::Not(_) => {}
            },
            Act::ThenType(t, a) => {
                let all = [
                    "SAY", "SHOW", "COMPLETE", "FAIL", "PLACE", "GIVE", "REVEAL", "ATTACK", "WIN",
                    "LOSE",
                ];
                let next = step_in(&all, action_name(&sc.triggers[t].then[a]), dir);
                let kind = if natural(self.kind) {
                    kinds::CLUBMAN
                } else {
                    self.kind
                };
                sc.triggers[t].then[a] = match next {
                    "SAY" => Action::Say("Something happens.".into()),
                    "SHOW" => Action::Show(first_obj),
                    "COMPLETE" => Action::Complete(first_obj),
                    "FAIL" => Action::Fail(first_obj),
                    "PLACE" => Action::Place(place(kind, 1.min(sides as PlayerId - 1), middle, 3)),
                    "GIVE" => Action::Give {
                        owner: 0,
                        resource: Resource::Food,
                        amount: 100,
                    },
                    "REVEAL" => Action::Reveal(Area {
                        from: (middle.0 - 4, middle.1 - 4),
                        to: (middle.0 + 4, middle.1 + 4),
                    }),
                    "ATTACK" => Action::Attack {
                        owner: 1.min(sides as PlayerId - 1),
                        to: middle,
                    },
                    "WIN" => Action::Win,
                    _ => Action::Lose("The scenario is lost".into()),
                };
            }
            Act::ThenParam(t, a, n) => match &mut sc.triggers[t].then[a] {
                Action::Show(o) | Action::Complete(o) | Action::Fail(o) => {
                    *o = id_step(o, &obj_ids)
                }
                Action::Place(p) => match n {
                    0 => p.owner = owner_step(p.owner, true),
                    1 => {
                        let mut all = own_kinds();
                        all.extend(page_kinds(2));
                        p.kind = name_step(&p.kind, &all);
                    }
                    _ => p.count = step(p.count as i32, 1, 1, 50) as u16,
                },
                Action::Give {
                    owner,
                    resource,
                    amount,
                } => match n {
                    0 => *owner = owner_step(*owner, false),
                    1 => *resource = res_step(*resource),
                    _ => *amount = step(*amount, 50, -100_000, 100_000),
                },
                Action::Attack { owner, .. } => *owner = owner_step(*owner, false),
                Action::Say(_) | Action::Lose(_) | Action::Reveal(_) | Action::Win => {}
            },
        }
        self.changed();
    }

    // --- The screen ------------------------------------------------------

    /// The palette of the tool in hand.
    fn palette(&self) -> Vec<Vec<Item>> {
        let item = |label: &str, action: EditorAction, active: bool| Item {
            label: label.to_string(),
            action,
            active,
        };
        let brushes = || {
            [1, 3, 5]
                .iter()
                .map(|&b| {
                    item(
                        &format!("BRUSH {b}"),
                        EditorAction::Brush(b),
                        self.brush == b,
                    )
                })
                .collect::<Vec<_>>()
        };
        match self.tool {
            Tool::Terrain => vec![
                LETTERS
                    .iter()
                    .map(|(c, name)| item(name, EditorAction::Letter(*c), self.letter == *c))
                    .collect(),
                brushes(),
            ],
            Tool::Height => vec![
                vec![
                    item("RAISE", EditorAction::Raise(true), self.raise),
                    item("LOWER", EditorAction::Raise(false), !self.raise),
                ],
                brushes(),
            ],
            Tool::Units => {
                let mut owners: Vec<Item> = vec![
                    item(
                        "PLACE",
                        EditorAction::Mode(UnitMode::Place),
                        self.mode == UnitMode::Place,
                    ),
                    item(
                        "ERASE",
                        EditorAction::Mode(UnitMode::Erase),
                        self.mode == UnitMode::Erase,
                    ),
                    item(
                        "NAME",
                        EditorAction::Mode(UnitMode::Tag),
                        self.mode == UnitMode::Tag,
                    ),
                ];
                for (i, s) in self.scenario.sides.iter().enumerate() {
                    owners.push(item(
                        &format!("{} {}", i + 1, upper(&s.name)),
                        EditorAction::Owner(i as u8),
                        self.owner == i as u8,
                    ));
                }
                let pages = PAGES
                    .iter()
                    .enumerate()
                    .map(|(i, p)| item(p, EditorAction::Page(i as u8), self.page == i))
                    .collect();
                let kinds_row = page_kinds(self.page)
                    .into_iter()
                    .map(|k| {
                        item(
                            &upper(kinds::info(k).name),
                            EditorAction::Kind(k),
                            self.kind == k,
                        )
                    })
                    .collect();
                vec![owners, pages, kinds_row]
            }
            Tool::Scenario | Tool::Objectives | Tool::Triggers | Tool::Check => Vec::new(),
        }
    }

    /// Everything the screen shows; `problems` is what the check says.
    pub fn panel(&self, problems: &[String]) -> Panel {
        let rows = self
            .rows(problems)
            .into_iter()
            .enumerate()
            .map(|(r, row)| {
                row.into_iter()
                    .enumerate()
                    .map(|(c, (mut chip, act))| {
                        if act != Act::None {
                            chip.action = Some(EditorAction::Chip(r as u16, c as u8));
                        }
                        chip
                    })
                    .collect()
            })
            .collect();
        let status = match (&self.note, problems.len()) {
            (Some(n), _) => n.clone(),
            (None, 0) => match self.tool {
                Tool::Terrain => {
                    "Left click paints; right click picks up the ground's letter".into()
                }
                Tool::Height => {
                    "Left click raises or lowers a corner; right click the other way".into()
                }
                Tool::Units => "Left click as the mode says; right click erases".into(),
                _ => "Left click a field steps it on, right click back; shift for big steps".into(),
            },
            (None, 1) => format!("1 problem: {}", problems[0]),
            (None, n) => format!("{n} problems: {}", problems[0]),
        };
        Panel {
            tool: self.tool,
            file: self
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map_or("UNTITLED".into(), |n| n.to_string_lossy().to_string()),
            dirty: self.dirty,
            status,
            palette: self.palette(),
            rows,
            top: self.top,
            typing: self
                .typing
                .as_ref()
                .map(|(what, line)| (Editor::typing_label(*what), line.clone())),
            prompt: self.pick.map(|p| p.prompt().to_string()),
            confirm_exit: self.confirm_exit,
        }
    }

    /// The palette buttons and the strip: everything but the list's chips.
    pub fn choose(&mut self, action: EditorAction) {
        match action {
            EditorAction::Tool(t) => {
                self.tool = t;
                self.top = 0;
                self.pick = None;
            }
            EditorAction::Letter(c) => self.letter = c,
            EditorAction::Brush(b) => self.brush = b,
            EditorAction::Raise(r) => self.raise = r,
            EditorAction::Page(p) => {
                self.page = p as usize;
                if let Some(&k) = page_kinds(self.page).first() {
                    self.kind = k;
                }
            }
            EditorAction::Kind(k) => {
                self.kind = k;
                self.mode = UnitMode::Place;
            }
            EditorAction::Owner(o) => self.owner = o,
            EditorAction::Mode(m) => self.mode = m,
            EditorAction::Scroll(n) => {
                let len = self.rows(&[]).len();
                self.top =
                    (self.top as i64 + n as i64).clamp(0, len.saturating_sub(1) as i64) as usize;
            }
            _ => {}
        }
    }

    /// Writes the scenario out: where it came from, or a new file under
    /// `dir` named after its title.
    pub fn save(&mut self, dir: &Path) -> Result<PathBuf, String> {
        let path = match &self.path {
            Some(p) => p.clone(),
            None => {
                let base = slug(&self.scenario.title);
                let mut n = 1;
                loop {
                    let name = if n == 1 {
                        format!("{base}.ron")
                    } else {
                        format!("{base}-{n}.ron")
                    };
                    let p = dir.join(name);
                    if !p.exists() {
                        break p;
                    }
                    n += 1;
                }
            }
        };
        let mut out = self.scenario.clone();
        out.id = String::new();
        save::campaigns::write_scenario(&path, &out)?;
        self.path = Some(path.clone());
        self.dirty = false;
        self.confirm_exit = false;
        self.note = Some(format!(
            "Saved as {}",
            path.file_name()
                .map_or(String::new(), |n| n.to_string_lossy().to_string())
        ));
        Ok(path)
    }
}

fn button(label: impl Into<String>) -> Chip {
    Chip {
        label: label.into(),
        action: Some(EditorAction::Chip(0, 0)),
        active: false,
    }
}

fn heading(label: impl Into<String>) -> Chip {
    Chip {
        label: label.into(),
        action: None,
        active: true,
    }
}

fn pick_chip(label: String, waiting: bool) -> Chip {
    Chip {
        label,
        action: Some(EditorAction::Chip(0, 0)),
        active: waiting,
    }
}

/// A long line cut to `max` characters.
fn fit_words(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{cut}...")
    }
}

fn control_label(c: Control) -> String {
    match c {
        Control::Player => "THE PLAYER".into(),
        Control::Computer(d) => format!(
            "COMPUTER, {}",
            upper(ai::Difficulty::ALL[d.min(3) as usize].name())
        ),
        Control::Scripted => "SCRIPTED".into(),
    }
}

fn goal_name(g: &Goal) -> &'static str {
    match g {
        Goal::Destroy { .. } => "DESTROY",
        Goal::Have { .. } => "HAVE",
        Goal::Stockpile { .. } => "STOCKPILE",
        Goal::Age(_) => "REACH AN AGE",
        Goal::Reach { .. } => "REACH AN AREA",
        Goal::Survive { .. } => "SURVIVE",
        Goal::Scripted => "SCRIPTED",
    }
}

fn condition_name(c: &Condition) -> &'static str {
    match c {
        Condition::After(_) => "AFTER",
        Condition::Done(_) => "OBJECTIVE DONE",
        Condition::Failed(_) => "OBJECTIVE FAILED",
        Condition::Fired(_) => "TRIGGER FIRED",
        Condition::Since { .. } => "SINCE A TRIGGER",
        Condition::Has { .. } => "HAS",
        Condition::HasAtMost { .. } => "HAS AT MOST",
        Condition::Stockpile { .. } => "STOCKPILE",
        Condition::Age { .. } => "AGE",
        Condition::Researched { .. } => "RESEARCHED",
        Condition::Inside { .. } => "UNITS IN AN AREA",
        Condition::Gone(_) => "NAMED ONE GONE",
        Condition::Any(_) => "ANY OF",
        Condition::Not(_) => "NOT",
    }
}

fn action_name(a: &Action) -> &'static str {
    match a {
        Action::Say(_) => "SAY",
        Action::Show(_) => "SHOW",
        Action::Complete(_) => "COMPLETE",
        Action::Fail(_) => "FAIL",
        Action::Place(_) => "PLACE",
        Action::Give { .. } => "GIVE",
        Action::Reveal(_) => "REVEAL",
        Action::Attack { .. } => "ATTACK",
        Action::Win => "WIN",
        Action::Lose(_) => "LOSE",
    }
}

/// An id not yet taken: `o1`, `o2`, ...
fn fresh_id(prefix: &str, taken: &[String]) -> String {
    (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken.contains(id))
        .expect("an id is free")
}

/// A file name from a title: lower case, words joined by dashes.
pub fn slug(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s
        .split('-')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() {
        "scenario".into()
    } else {
        s
    }
}

/// A scenario on a generated map, made over onto a drawn one: the ground
/// tile by tile, the heights, and everything the generator set down but
/// the drawn forests' trees as placements.
fn bake(sc: Scenario) -> Scenario {
    let sim = Simulation::new(sc.match_seed(), sc.config());
    let map = sim.map();
    let (w, h) = (map.width(), map.height());
    let terrain = (0..h)
        .map(|y| (0..w).map(|x| letter_of(map.terrain(x, y))).collect())
        .collect();
    let heights = (0..=h)
        .map(|y| {
            (0..=w)
                .map(|x| char::from_digit(map.corner(x, y) as u32, 10).unwrap_or('0'))
                .collect()
        })
        .collect();
    let world = sim.world();
    let mut placements: Vec<Placement> = Vec::new();
    for s in world.slots() {
        let i = s.index();
        let kind = world.kind[i];
        let info = kinds::info(kind);
        let pos = world.pos[i];
        let (tx, ty) = (pos.x.floor(), pos.y.floor());
        if kind == kinds::TREE && map.terrain(tx, ty) == sim::Terrain::ForestFloor {
            continue;
        }
        // Units are kept where they stand; a building's anchor is its
        // corner, half its footprint back from its centre.
        let at = if info.mobile {
            (tx, ty)
        } else {
            let half = sim::Fx::from_ratio(info.footprint.max(1) as i32, 2);
            ((pos.x - half).round(), (pos.y - half).round())
        };
        placements.push(place(kind, world.owner[i], at, 1));
    }
    // What the scenario placed itself is in the world already.
    Scenario {
        map: ScenarioMap::Drawn(DrawnMap { terrain, heights }),
        standard_start: false,
        placements,
        ..sc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(sim: &Simulation, p: PlayerId, kind: KindId) -> usize {
        let w = sim.world();
        w.slots()
            .filter(|s| w.owner[s.index()] == p && w.kind[s.index()] == kind)
            .count()
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ne-editor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A new map is a playable match as it stands: two sides, a town
    /// each, an objective, nothing wrong.
    ///
    /// REQ: GD-CAMP-06
    #[test]
    fn a_new_map_is_a_scenario_that_can_be_played_at_once() {
        let e = Editor::new_map(64);
        assert_eq!(e.size(), (64, 64));
        assert_eq!(e.scenario.problems(), Vec::<String>::new());
        let sim = e.preview();
        assert_eq!(sim.map().width(), 64);
        assert_eq!(sim.players().len(), 2);
        assert!(sim.standing(0) && sim.standing(1));
    }

    /// The brush paints the ground; right click's letter is read back;
    /// heights rise in a cone and keep their neighbours a level apart.
    ///
    /// REQ: GD-CAMP-06
    #[test]
    fn the_ground_is_painted_and_raised_and_stays_valid() {
        let mut e = Editor::new_map(48);
        e.letter = 'W';
        e.brush = 3;
        assert!(e.paint((10, 10)));
        assert!(!e.paint((10, 10)), "painting it again changes nothing");
        assert_eq!(e.letter_at((9, 9)), Some('W'));
        assert_eq!(e.letter_at((11, 11)), Some('W'));
        assert_eq!(e.letter_at((12, 12)), Some('g'));
        assert!(e.dirty && e.stale);
        e.brush = 1;
        for _ in 0..MAX_ELEVATION {
            assert!(e.lift((30, 30), true));
        }
        assert!(!e.lift((30, 30), true), "no higher than the highest");
        let sim = e.preview();
        let top = MAX_ELEVATION;
        assert_eq!(sim.map().corner(30, 30), top);
        assert_eq!(sim.map().corner(31, 30), top - 1);
        assert_eq!(sim.map().corner(30 + top as i32, 30), 0);
        assert!(sim.map().validate().is_ok());
        assert_eq!(e.scenario.problems(), Vec::<String>::new());
        for _ in 0..MAX_ELEVATION {
            e.lift((30, 30), false);
        }
        assert_eq!(e.preview().map().corner(30, 30), 0);
        // The edge of the map is no trouble.
        e.brush = 5;
        assert!(e.lift((0, 0), true));
        assert!(e.preview().map().validate().is_ok());
    }

    /// Units and buildings are set down, named and taken up; nature is
    /// nobody's.
    ///
    /// REQ: GD-CAMP-06
    #[test]
    fn units_buildings_and_nature_are_set_down_named_and_taken_up() {
        let mut e = Editor::new_map(48);
        let before = e.scenario.placements.len();
        e.kind = kinds::BARRACKS;
        e.owner = 1;
        assert!(e.place((24, 24)));
        e.kind = kinds::GOLD_MINE;
        assert!(e.place((30, 30)));
        let p = &e.scenario.placements;
        assert_eq!(p.len(), before + 2);
        assert_eq!(p[before].owner, 1);
        assert_eq!(p[before + 1].owner, kinds::GAIA);
        let sim = e.preview();
        assert_eq!(count(&sim, 1, kinds::BARRACKS), 1);
        // Named, and a trigger may wait on it.
        let name = e.tag((24, 24)).expect("named");
        assert_eq!(name, "barracks 1");
        e.scenario.triggers.push(Trigger {
            id: Some("t1".into()),
            when: vec![Condition::Gone(name)],
            then: vec![Action::Win],
            repeat: false,
        });
        assert_eq!(e.scenario.problems(), Vec::<String>::new());
        assert!(e.erase((30, 30)));
        assert!(!e.erase((40, 2)));
        assert_eq!(e.scenario.placements.len(), before + 1);
    }

    /// Every field of every objective, condition and action can be
    /// stepped through both ways without leaving the scenario wrong in a
    /// way the check does not name, and a typed line is kept.
    ///
    /// REQ: GD-CAMP-06
    #[test]
    fn every_field_of_the_lists_steps_and_the_scenario_stays_readable() {
        let mut e = Editor::new_map(48);
        e.tool = Tool::Triggers;
        e.act(Act::AddTrigger, 1, false);
        e.act(Act::AddTrigger, 1, false);
        e.tool = Tool::Objectives;
        e.act(Act::AddObjective, 1, false);
        e.kind = kinds::VILLAGER;
        e.tag((13, 13)).expect("a villager stands there");
        // Each condition type and each action type, each field stepped.
        for _ in 0..12 {
            e.act(Act::WhenType(0, 0), 1, false);
            for n in 0..4 {
                e.act(Act::WhenParam(0, 0, n), 1, false);
                e.act(Act::WhenParam(0, 0, n), -1, true);
            }
            assert!(
                e.scenario.problems().is_empty(),
                "{:?}: {:?}",
                e.scenario.triggers[0].when[0],
                e.scenario.problems()
            );
        }
        for _ in 0..10 {
            e.act(Act::ThenType(0, 0), 1, false);
            for n in 0..4 {
                e.act(Act::ThenParam(0, 0, n), 1, false);
                e.act(Act::ThenParam(0, 0, n), -1, false);
            }
            assert!(
                e.scenario.problems().is_empty(),
                "{:?}: {:?}",
                e.scenario.triggers[0].then[0],
                e.scenario.problems()
            );
        }
        for _ in 0..7 {
            e.act(Act::GoalType(1), 1, false);
            for n in 0..3 {
                e.act(Act::GoalParam(1, n), 1, false);
                e.act(Act::GoalParam(1, n), -1, false);
            }
            let p = e.scenario.problems();
            assert!(
                p.iter().all(|p| p.contains("nothing wins")),
                "{:?}: {p:?}",
                e.scenario.objectives[1].goal
            );
        }
        // A line typed into a Say.
        e.tool = Tool::Triggers;
        e.act(Act::ThenType(0, 0), 1, false);
        while !matches!(e.scenario.triggers[0].then[0], Action::Say(_)) {
            e.act(Act::ThenType(0, 0), 1, false);
        }
        e.act(Act::Type(Typing::Say(0, 0)), 1, false);
        e.typing.as_mut().unwrap().1 = "The river rises.".into();
        e.keep_typing();
        assert_eq!(
            e.scenario.triggers[0].then[0],
            Action::Say("The river rises.".into())
        );
        // A field picked off the map.
        e.act(Act::AddThen(0), 1, false);
        e.scenario.triggers[0].then[1] = Action::Attack {
            owner: 1,
            to: (0, 0),
        };
        e.act(Act::Pick(Pick::AttackTo(0, 1)), 1, false);
        assert_eq!(e.pick, Some(Pick::AttackTo(0, 1)));
        e.picked((5, 6), (7, 8));
        assert_eq!(
            e.scenario.triggers[0].then[1],
            Action::Attack {
                owner: 1,
                to: (7, 8)
            }
        );
        // The rows' chips land where the click says.
        let panel = e.panel(&[]);
        assert!(panel.rows.len() >= 4);
        let (r, c) = panel
            .rows
            .iter()
            .enumerate()
            .find_map(|(r, row)| {
                row.iter()
                    .position(|chip| chip.label == "ONCE")
                    .map(|c| (r, c))
            })
            .expect("a trigger's ONCE chip");
        e.click(r, c, 1, false);
        assert!(e.scenario.triggers[0].repeat);
    }

    /// A scenario is written where its title says and read back the same;
    /// a second untitled one does not overwrite the first.
    ///
    /// REQ: GD-CAMP-06
    #[test]
    fn a_scenario_is_saved_by_its_title_and_reads_back_the_same() {
        let dir = scratch("save");
        let mut e = Editor::new_map(48);
        e.scenario.title = "The Nile Rises!".into();
        e.paint((3, 3));
        let path = e.save(&dir).unwrap();
        assert_eq!(path.file_name().unwrap(), "the-nile-rises.ron");
        assert!(!e.dirty);
        let back = save::campaigns::read_scenario(&path).unwrap();
        assert_eq!(back, e.scenario);
        let mut other = Editor::new_map(48);
        other.scenario.title = "The Nile Rises!".into();
        let second = other.save(&dir).unwrap();
        assert_eq!(second.file_name().unwrap(), "the-nile-rises-2.ron");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A scenario on a generated map opens onto a drawn one that plays
    /// the same ground and the same things on it.
    ///
    /// REQ: GD-CAMP-06
    #[test]
    fn a_generated_map_opens_baked_onto_a_drawn_one() {
        let dir = scratch("generated");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("gen.ron");
        std::fs::write(
            &file,
            r#"(title: "Gen", seed: Some(5), map: Generated(kind: Inland, size: 64),
                sides: [(name: "Us", control: Player), (name: "Them", control: Computer(1))],
                standard_start: true,
                objectives: [(id: "o", text: "Win", goal: Destroy(owner: 1))])"#,
        )
        .unwrap();
        let sc = save::campaigns::parse_scenario(&file).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let original = Simulation::new(sc.match_seed(), sc.config());
        let e = Editor::open(sc, None);
        assert!(matches!(e.scenario.map, ScenarioMap::Drawn(_)));
        assert_eq!(e.scenario.problems(), Vec::<String>::new());
        let baked = e.preview();
        assert_eq!(
            baked.map().terrain_histogram(),
            original.map().terrain_histogram()
        );
        for kind in [
            kinds::TOWN_CENTER,
            kinds::VILLAGER,
            kinds::BERRY_BUSH,
            kinds::GOLD_MINE,
        ] {
            for p in [0, 1, kinds::GAIA] {
                assert_eq!(
                    count(&baked, p, kind),
                    count(&original, p, kind),
                    "{} of player {p}",
                    kinds::info(kind).name
                );
            }
        }
    }
}
