//! The scenario editor in the app (`docs/02` §13, `GD-CAMP-06`): its front
//! door, the world made from the scenario and drawn without fog, the
//! brush and the palette on the map, typed lines, and the playtest that
//! goes from the editor into a match and back.
//!
//! What a scenario is and how it changes is `editor`'s; this is where
//! clicks and keys reach it and the world on screen follows it.

use super::*;
use editor::Editor;
use view::editor::{self as ed, EditorAction, Tool, UnitMode};

/// While a brush is dragged the world is rebuilt at most this often; a
/// click rebuilds it at once.
const REBUILD_MS: u128 = 150;

/// The longest line the editor takes.
const MAX_LINE: usize = 240;

/// Where a scenario the front door offers comes from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Source {
    /// One of the player's own, written back where it was.
    Mine(PathBuf),
    /// A campaign's: (campaign, scenario), opened as a copy.
    Shipped(usize, usize),
}

impl App {
    /// The editor's front door: new maps, and the scenarios to open.
    pub(crate) fn open_editor_start(&mut self) {
        let mut files = Vec::new();
        for path in save::campaigns::user_files(&self.scenarios_dir) {
            let title = save::campaigns::parse_scenario(&path).map_or_else(
                |_| "(DOES NOT READ)".to_string(),
                |sc| sc.title.to_uppercase(),
            );
            let name = path
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().to_uppercase());
            files.push((
                LoadRow {
                    title,
                    detail: format!("YOURS: {name}"),
                },
                Source::Mine(path),
            ));
        }
        for (c, camp) in self.campaigns.iter().enumerate() {
            if camp.free {
                continue;
            }
            for (s, sc) in camp.scenarios.iter().enumerate() {
                files.push((
                    LoadRow {
                        title: sc.title.to_uppercase(),
                        detail: format!("{}: OPENS AS A COPY", camp.title.to_uppercase()),
                    },
                    Source::Shipped(c, s),
                ));
            }
        }
        self.editor_files = files;
        self.editor_error = None;
        self.shell = Shell::EditorStart;
    }

    /// A click in the editor or on its front door: `dir` 1 for the left
    /// button, -1 for the right.
    pub(crate) fn editor_action(&mut self, action: EditorAction, dir: i32) {
        let big = self.modifiers.shift_key();
        match action {
            EditorAction::New(size) => {
                self.editor = Some(Editor::new_map(size));
                self.enter_editor(true);
            }
            EditorAction::Open(i) => {
                let Some((_, source)) = self.editor_files.get(i).cloned() else {
                    return;
                };
                let opened = match source {
                    Source::Mine(path) => save::campaigns::parse_scenario(&path)
                        .map(|sc| Editor::open(sc, Some(path))),
                    Source::Shipped(c, s) => self
                        .campaigns
                        .get(c)
                        .and_then(|camp| camp.scenarios.get(s))
                        .cloned()
                        .map(|mut sc| {
                            sc.id.clear();
                            Editor::open(sc, None)
                        })
                        .ok_or_else(|| "That scenario is no longer there".to_string()),
                };
                match opened {
                    Ok(e) => {
                        self.editor = Some(e);
                        self.enter_editor(true);
                    }
                    Err(e) => self.editor_error = Some(e),
                }
            }
            EditorAction::Save => self.save_editor(),
            EditorAction::Playtest => self.start_playtest(),
            EditorAction::Exit => {
                let Some(e) = &mut self.editor else {
                    return;
                };
                if e.dirty && !e.confirm_exit {
                    e.confirm_exit = true;
                    e.note = Some("Unsaved changes: EXIT again leaves them".into());
                    return;
                }
                self.editor = None;
                self.shell = Shell::Title;
            }
            EditorAction::Chip(r, c) => {
                if let Some(e) = &mut self.editor {
                    e.click(r as usize, c as usize, dir, big);
                }
                self.refresh_editor();
            }
            other => {
                if let Some(e) = &mut self.editor {
                    e.choose(other);
                }
            }
        }
    }

    /// Writes the scenario out and lists it with the player's own.
    pub(crate) fn save_editor(&mut self) {
        let dir = self.scenarios_dir.clone();
        let Some(e) = &mut self.editor else {
            return;
        };
        if e.typing.is_some() {
            e.keep_typing();
        }
        if let Err(err) = e.save(&dir) {
            e.note = Some(format!("Not saved: {err}"));
            return;
        }
        if !self.editor_problems.is_empty() {
            if let Some(e) = &mut self.editor {
                let n = self.editor_problems.len();
                e.note = Some(format!(
                    "{} - it has {n} problem{} to put right before it can be played",
                    e.note.clone().unwrap_or_default(),
                    if n == 1 { "" } else { "s" }
                ));
            }
        }
        self.reload_user_campaign();
    }

    /// The player's own scenarios, read again.
    pub(crate) fn reload_user_campaign(&mut self) {
        let (user, errors) = save::campaigns::load_user(&self.scenarios_dir);
        self.campaigns.retain(|c| !c.free);
        self.campaigns.extend(user);
        for e in errors {
            eprintln!("warning: {e}");
        }
    }

    /// Into the editor with the scenario it holds: its world built, fog
    /// off, and, for a new or opened one, the camera on the first side.
    pub(crate) fn enter_editor(&mut self, reset_camera: bool) {
        self.shell = Shell::Editor;
        self.playtest = false;
        self.opponents.clear();
        self.playback = None;
        self.menu = false;
        self.confirm = None;
        self.results = ResultsState::Dismissed;
        self.selection = Selection::new();
        self.build_mode = None;
        self.targeting = None;
        self.cheat = None;
        self.viewer = None;
        self.notices.clear();
        self.narration = None;
        self.painting = None;
        self.last_stroke = None;
        self.edit_drag = None;
        self.rebuild_editor_world();
        if reset_camera {
            let (viewport, dpi) = (self.camera.viewport, self.camera.dpi);
            let map = self.sim.map();
            self.camera = Camera::new(map.width(), map.height(), viewport);
            self.camera.dpi = dpi;
            if let Some(&(sx, sy)) = self.sim.starts().first() {
                self.camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
            }
        }
    }

    /// The world on screen made again from the scenario.
    pub(crate) fn rebuild_editor_world(&mut self) {
        let Some(e) = &mut self.editor else {
            return;
        };
        self.sim = e.preview();
        e.stale = false;
        self.editor_problems = e.scenario.problems();
        self.editor_built = Instant::now();
        self.prev_pos = self.sim.world().pos.clone();
        self.last_fog_tick = None;
        self.last_minimap = Instant::now() - Duration::from_secs(10);
        if let Some(gpu) = &mut self.gpu {
            let chunks = view::terrain::build_all(self.sim.map());
            gpu.renderer.upload_terrain(&gpu.device, &chunks);
        }
    }

    /// The world follows a change at once.
    pub(crate) fn refresh_editor(&mut self) {
        if self.editor.as_ref().is_some_and(|e| e.stale) {
            self.rebuild_editor_world();
        }
    }

    /// Whether a line is being typed into the editor.
    pub(crate) fn editor_typing(&self) -> bool {
        self.shell == Shell::Editor && self.editor.as_ref().is_some_and(|e| e.typing.is_some())
    }

    /// The tile under a window point, if it is the map's and not under the
    /// editor's panels or the minimap.
    pub(crate) fn editor_tile(&self, px: f32, py: f32) -> Option<(i32, i32)> {
        self.editor_point(px, py)
            .map(|(wx, wy)| (wx.floor() as i32, wy.floor() as i32))
            .filter(|&(x, y)| {
                let (w, h) = self.map_size();
                x >= 0 && y >= 0 && x < w && y < h
            })
    }

    /// The corner nearest a window point, for the height tool.
    fn editor_corner(&self, px: f32, py: f32) -> Option<(i32, i32)> {
        self.editor_point(px, py)
            .map(|(wx, wy)| (wx.round() as i32, wy.round() as i32))
            .filter(|&(x, y)| {
                let (w, h) = self.map_size();
                x >= 0 && y >= 0 && x <= w && y <= h
            })
    }

    fn editor_point(&self, px: f32, py: f32) -> Option<(f32, f32)> {
        let e = self.editor.as_ref()?;
        let s = self.ui_scale();
        let (vw, vh) = (self.camera.viewport.0 / s, self.camera.viewport.1 / s);
        if ed::over_panel(e.tool, vw, vh, px / s, py / s)
            || self.minimap_rect().to_uv(px, py).is_some()
        {
            return None;
        }
        Some(self.camera.window_to_world(px, py))
    }

    /// The brush under the cursor: green where it will paint.
    fn brush_tiles(&self) -> Vec<(i32, i32)> {
        let Some(e) = &self.editor else {
            return Vec::new();
        };
        let Some((px, py)) = self.input.cursor else {
            return Vec::new();
        };
        match e.tool {
            Tool::Terrain if e.pick.is_none() => self
                .editor_tile(px, py)
                .map_or_else(Vec::new, |t| e.brushed(t)),
            Tool::Height if e.pick.is_none() => self
                .editor_corner(px, py)
                .map_or_else(Vec::new, |c| e.brushed(c)),
            _ => match (e.pick, self.edit_drag, self.editor_tile(px, py)) {
                // An area being dragged out.
                (Some(_), Some(from), Some(to)) => {
                    let mut v = Vec::new();
                    for y in from.1.min(to.1)..=from.1.max(to.1) {
                        for x in from.0.min(to.0)..=from.0.max(to.0) {
                            v.push((x, y));
                        }
                    }
                    v
                }
                (Some(_), None, Some(t)) => vec![t],
                _ => Vec::new(),
            },
        }
    }

    /// What the units tool would set down, ghosted under the cursor.
    fn editor_ghost(&self) -> Option<Ghost> {
        let e = self.editor.as_ref()?;
        if e.tool != Tool::Units || e.mode != UnitMode::Place || e.pick.is_some() {
            return None;
        }
        let info = kinds::info(e.kind);
        if info.mobile {
            return None;
        }
        let (px, py) = self.input.cursor?;
        let tile = self.editor_tile(px, py)?;
        let (x, y) = Editor::anchor(e.kind, tile);
        let owner = if info.class == sim::Class::Building {
            e.owner
        } else {
            kinds::GAIA
        };
        let age = e
            .scenario
            .sides
            .get(owner as usize)
            .map_or(0, |s| s.age.index() as u8);
        Some(Ghost {
            kind: e.kind,
            x,
            y,
            ok: self.sim.site_clear(e.kind, x, y),
            row: view::palette::row_for_owner(owner),
            player: owner.min(sim::MAX_PLAYERS as u8 - 1),
            age,
            run: None,
        })
    }

    /// A brush stroke at a window point: paint, or lift (`left`) or lower.
    fn editor_stroke(&mut self, px: f32, py: f32, left: bool) {
        let tile = self.editor_tile(px, py);
        let corner = self.editor_corner(px, py);
        let Some(e) = &mut self.editor else {
            return;
        };
        match e.tool {
            Tool::Terrain => {
                if let Some(t) = tile.filter(|t| self.last_stroke != Some(*t)) {
                    e.paint(t);
                    self.last_stroke = Some(t);
                }
            }
            Tool::Height => {
                if let Some(c) = corner.filter(|c| self.last_stroke != Some(*c)) {
                    let up = e.raise == left;
                    e.lift(c, up);
                    self.last_stroke = Some(c);
                }
            }
            _ => {}
        }
    }

    /// The left button down in the editor.
    pub(crate) fn editor_press(&mut self, px: f32, py: f32) {
        if let Some(b) = self
            .screen
            .buttons
            .iter()
            .find(|b| b.contains(px, py))
            .cloned()
        {
            if b.enabled {
                self.cue(Cue::Click, None);
                match b.action {
                    ShellAction::Edit(a) => self.editor_action(a, 1),
                    other => self.shell_action(other),
                }
            }
            return;
        }
        let (mm, map) = (self.minimap_rect(), self.map_size());
        if self.input.left_pressed(&mut self.camera, mm, map, px, py) {
            return;
        }
        let Some(tile) = self.editor_tile(px, py) else {
            return;
        };
        let Some(e) = &mut self.editor else {
            return;
        };
        if let Some(pick) = e.pick {
            if pick.area() {
                self.edit_drag = Some(tile);
            } else {
                e.picked(tile, tile);
                self.refresh_editor();
            }
            return;
        }
        match e.tool {
            Tool::Terrain | Tool::Height => {
                self.painting = Some(true);
                self.last_stroke = None;
                self.editor_stroke(px, py, true);
            }
            Tool::Units => match e.mode {
                UnitMode::Place => {
                    let (x, y) = Editor::anchor(e.kind, tile);
                    if self.sim.site_clear(e.kind, x, y) {
                        e.place(tile);
                        self.refresh_editor();
                    } else {
                        e.note = Some("The ground there is not clear for it".into());
                        self.cue(Cue::Invalid, None);
                    }
                }
                UnitMode::Erase => {
                    if e.erase(tile) {
                        self.refresh_editor();
                    }
                }
                UnitMode::Tag => {
                    e.tag(tile);
                    self.refresh_editor();
                }
            },
            _ => {}
        }
    }

    /// The left button up: a stroke or a dragged area ends.
    pub(crate) fn editor_release(&mut self, px: f32, py: f32) {
        self.painting = None;
        self.last_stroke = None;
        if let Some(from) = self.edit_drag.take() {
            let to = self.editor_tile(px, py).unwrap_or(from);
            if let Some(e) = &mut self.editor {
                e.picked(from, to);
            }
        }
        self.refresh_editor();
    }

    /// The right button: a field stepped back, the ground's letter picked
    /// up, the ground lowered, or what is there taken up; and a field
    /// waiting on the map let go.
    pub(crate) fn editor_right(&mut self, px: f32, py: f32) {
        if let Some(b) = self
            .screen
            .buttons
            .iter()
            .find(|b| b.contains(px, py))
            .cloned()
        {
            if let ShellAction::Edit(a @ EditorAction::Chip(..)) = b.action {
                self.cue(Cue::Click, None);
                self.editor_action(a, -1);
            }
            return;
        }
        let tile = self.editor_tile(px, py);
        let Some(e) = &mut self.editor else {
            return;
        };
        if e.pick.take().is_some() {
            self.edit_drag = None;
            return;
        }
        let Some(tile) = tile else {
            return;
        };
        match e.tool {
            Tool::Terrain => {
                if let Some(c) = e.letter_at(tile) {
                    e.letter = c;
                }
            }
            Tool::Height => {
                self.painting = Some(false);
                self.last_stroke = None;
                self.editor_stroke(px, py, false);
            }
            Tool::Units => {
                let erased = e.erase(tile);
                if erased {
                    self.refresh_editor();
                }
            }
            _ => {}
        }
    }

    /// The right button up: a lowering stroke ends.
    pub(crate) fn editor_right_release(&mut self) {
        if self.painting == Some(false) {
            self.painting = None;
            self.last_stroke = None;
            self.refresh_editor();
        }
    }

    /// The wheel over the list scrolls it; elsewhere it zooms.
    pub(crate) fn editor_wheel(&mut self, steps: i32) {
        let s = self.ui_scale();
        let (vw, vh) = (self.camera.viewport.0 / s, self.camera.viewport.1 / s);
        if let (Some((px, py)), Some(e)) = (self.input.cursor, &mut self.editor) {
            if e.tool.lists()
                && py / s >= ed::TOP_H
                && ed::over_panel(e.tool, vw, vh, px / s, py / s)
            {
                e.choose(EditorAction::Scroll(-steps * 2));
                return;
            }
        }
        let (cx, cy) = self
            .input
            .cursor
            .unwrap_or((self.camera.viewport.0 * 0.5, self.camera.viewport.1 * 0.5));
        self.camera.zoom_step_at(steps, cx, cy);
    }

    /// A key in the editor: a typed line takes them all; otherwise Escape
    /// lets a field waiting on the map go, or leaves.
    pub(crate) fn editor_key(&mut self, code: KeyCode) {
        let shift = self.modifiers.shift_key();
        let Some(e) = &mut self.editor else {
            return;
        };
        if let Some((_, line)) = &mut e.typing {
            match code {
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    e.keep_typing();
                    self.refresh_editor();
                }
                KeyCode::Escape => e.typing = None,
                KeyCode::Backspace => {
                    line.pop();
                }
                code => {
                    if let Some(c) = cheats::text_char(code, shift) {
                        if line.chars().count() < MAX_LINE {
                            line.push(c);
                        }
                    }
                }
            }
            return;
        }
        if code == KeyCode::Escape {
            if e.pick.take().is_some() {
                self.edit_drag = None;
            } else {
                self.editor_action(EditorAction::Exit, 1);
            }
        }
    }

    /// One frame of the editor: the camera, a brush dragged, the world
    /// rebuilt when it is behind, and the panels over it.
    pub(crate) fn frame_editor(&mut self, now: Instant, dt: f32) {
        if self.editor.is_none() {
            self.shell = Shell::Title;
            return;
        }
        if !self.editor_typing() {
            self.input.update_camera(&mut self.camera, dt);
        }
        self.hear_title();
        if let (Some(left), Some((px, py))) = (self.painting, self.input.cursor) {
            self.editor_stroke(px, py, left);
        }
        if self.editor.as_ref().is_some_and(|e| e.stale)
            && self.editor_built.elapsed().as_millis() >= REBUILD_MS
        {
            self.rebuild_editor_world();
        }
        let mut scene = Scene::build_full(
            &self.sim,
            &self.atlas,
            None,
            1.0,
            &SceneOptions {
                selected: &[],
                ghost: self.editor_ghost(),
                sweep: None,
                viewer: None,
            },
        );
        let marks = self.brush_tiles();
        if !marks.is_empty() {
            scene.sprites.extend(view::scene::tile_marks(
                &self.sim,
                &self.atlas,
                &marks,
                true,
            ));
            scene
                .sprites
                .sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.slot.cmp(&b.slot)));
        }
        let Some(e) = &self.editor else {
            return;
        };
        let panel = e.panel(&self.editor_problems);
        let input = self.shell_input();
        let screen = ed::editor_screen(&self.atlas, &input, &panel);
        scene.ui.extend(screen.sprites.iter().cloned());
        self.screen = screen;
        self.count_frame(now);
        self.update_cursor();
        let rect = self.minimap_rect();
        if let Some(gpu) = &mut self.gpu {
            if self.last_minimap.elapsed().as_millis() >= 500 {
                let m = Minimap::render(&self.sim);
                gpu.renderer.upload_minimap(&gpu.device, &gpu.queue, &m);
                self.last_minimap = now;
            }
            if self.last_fog_tick.is_none() {
                let map = self.sim.map();
                let lights = FogLights::lit(map.width(), map.height());
                gpu.renderer.upload_fog(&gpu.device, &gpu.queue, &lights);
                self.last_fog_tick = Some(self.sim.tick());
            }
            gpu.render(&self.camera, &scene, Some(rect));
        }
        self.scene = scene;
    }

    /// The computer's sides of a scenario, each an opponent at its
    /// difficulty.
    pub(crate) fn opponents_for(sc: &sim::Scenario, seed: u64) -> Vec<Opponent> {
        sc.sides
            .iter()
            .enumerate()
            .filter_map(|(p, side)| match side.control {
                sim::scenario::Control::Computer(d) => Some(Opponent::new(
                    p as u8,
                    ai::Difficulty::ALL[d.min(3) as usize],
                    seed,
                )),
                _ => None,
            })
            .collect()
    }

    /// Plays the scenario as it stands, if nothing is wrong with it.
    pub(crate) fn start_playtest(&mut self) {
        let Some(e) = &mut self.editor else {
            return;
        };
        if e.typing.is_some() {
            e.keep_typing();
        }
        let problems = e.scenario.problems();
        if !problems.is_empty() {
            e.tool = Tool::Check;
            e.top = 0;
            e.note = Some(format!(
                "Put {} right before it is played",
                if problems.len() == 1 {
                    "the problem".to_string()
                } else {
                    format!("the {} problems", problems.len())
                }
            ));
            self.refresh_editor();
            return;
        }
        let sc = e.scenario.clone();
        self.editor_camera = Some(self.camera);
        let seed = sc.match_seed();
        self.sim = Simulation::new(seed, sc.config());
        self.opponents = App::opponents_for(&sc, seed);
        self.playback = None;
        self.enter_match();
        self.playtest = true;
    }

    /// From a playtest back into the editor, the camera where it was.
    pub(crate) fn back_to_editor(&mut self) {
        let camera = self.editor_camera.take();
        self.enter_editor(camera.is_none());
        if let Some(c) = camera {
            let (viewport, dpi) = (self.camera.viewport, self.camera.dpi);
            self.camera = c;
            self.camera.viewport = viewport;
            self.camera.dpi = dpi;
        }
    }
}
