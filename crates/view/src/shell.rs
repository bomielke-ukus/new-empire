//! The game shell (`docs/06` M6): the title, the skirmish setup, the pause
//! menu and the results. Like the HUD, each screen is screen-space sprites
//! and the buttons' hit rectangles; the app decides what a click means.
//! Everything is laid out in HUD pixels and scaled on the way out, so the
//! screens read the same on any display.
//!
//! The setup screen is where a match's parameters are chosen, so it is
//! also where the one thing an opponent may have that a player may not,
//! the Hardest gather bonus (`docs/02` §12), is set and declared.

pub use ai::Difficulty;
use audio::Bus;
use sim::{
    ConfigError, MapKind, MapSpec, SimConfig, HARDEST_GATHER_BONUS_PCT, MAX_PLAYERS, POP_CAP_RANGE,
};

use crate::font;
use crate::hud::{fit, Painter};
use crate::minimap::MinimapRect;
use crate::palette::*;
use crate::scene::SpriteInstance;
use crate::settings::{pretty, Control, Settings};
use crate::sprites::{Atlas, Ink};

/// The name on the title screen. `docs/07` Q5 is open: this is the
/// repository's placeholder until the game is named.
pub const TITLE: &str = "NEW EMPIRE";

/// The line under the name.
const TAGLINE: &str = "FROM HAND-AXES TO IRON IN HALF AN HOUR";

/// The most opponents a skirmish can have: every player slot but the
/// human's.
pub const MAX_OPPONENTS: usize = MAX_PLAYERS - 1;

/// The population cap moves in steps of this.
const POP_CAP_STEP: u32 = 25;

/// The map sizes a skirmish offers (`docs/02` §8), tiles per side.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapSize {
    /// 96².
    Tiny,
    /// 128², the default.
    Small,
    /// 168².
    Medium,
    /// 200².
    Large,
    /// 240².
    Giant,
}

impl MapSize {
    /// Every size, smallest first.
    pub const ALL: [MapSize; 5] = [
        MapSize::Tiny,
        MapSize::Small,
        MapSize::Medium,
        MapSize::Large,
        MapSize::Giant,
    ];

    /// Tiles per side.
    pub const fn tiles(self) -> u16 {
        match self {
            MapSize::Tiny => 96,
            MapSize::Small => 128,
            MapSize::Medium => 168,
            MapSize::Large => 200,
            MapSize::Giant => 240,
        }
    }

    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            MapSize::Tiny => "Tiny",
            MapSize::Small => "Small",
            MapSize::Medium => "Medium",
            MapSize::Large => "Large",
            MapSize::Giant => "Giant",
        }
    }

    /// The size with exactly this many tiles per side, if one is offered.
    pub fn from_tiles(tiles: u16) -> Option<MapSize> {
        MapSize::ALL.into_iter().find(|s| s.tiles() == tiles)
    }
}

/// A setting on the setup screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    /// The map generator. One in the slice; the rest come with M8.
    Map,
    /// Tiles per side.
    Size,
    /// How many computer opponents.
    Opponents,
    /// The difficulty of opponent `i` (player `i + 1`).
    Difficulty(usize),
    /// The population cap (`GD-POP-02`).
    PopCap,
    /// The map seed.
    Seed,
}

/// A skirmish as the setup screen holds it: everything `SimConfig` needs,
/// and who the opponents are. [`Setup::config`] is the one place the
/// screen's choices become match parameters, so a Hardest opponent's
/// declared bonus is set here and nowhere in the `ai` crate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Setup {
    /// The map generator.
    pub kind: MapKind,
    /// The map size.
    pub size: MapSize,
    /// One difficulty per opponent; opponent `i` is player `i + 1`.
    pub opponents: Vec<Difficulty>,
    /// The population cap.
    pub pop_cap: u32,
    /// The map seed.
    pub seed: u64,
}

impl Setup {
    /// The default skirmish for a seed: one Standard opponent on a Small
    /// Inland map at the default population cap.
    pub fn new(seed: u64) -> Setup {
        Setup {
            kind: MapKind::Inland,
            size: MapSize::Small,
            opponents: vec![Difficulty::Standard],
            pop_cap: SimConfig::default().pop_cap_max,
            seed,
        }
    }

    /// Players in the match: the human and every opponent.
    pub fn players(&self) -> u8 {
        self.opponents.len() as u8 + 1
    }

    /// The gather bonus a player is set up with, in percent: the declared
    /// bonus of a Hardest opponent (`docs/02` §12), nothing for anyone
    /// else and never for the human.
    pub fn declared_bonus(&self, player: u8) -> i32 {
        match player {
            0 => 0,
            p => match self.opponents.get(p as usize - 1) {
                Some(Difficulty::Hardest) => HARDEST_GATHER_BONUS_PCT,
                _ => 0,
            },
        }
    }

    /// The match parameters these choices amount to.
    pub fn config(&self) -> SimConfig {
        SimConfig {
            map: MapSpec {
                kind: self.kind,
                size: self.size.tiles(),
                players: self.players(),
            },
            pop_cap_max: self.pop_cap,
            gather_bonus_pct: (0..self.players())
                .map(|p| self.declared_bonus(p))
                .collect(),
            ..SimConfig::default()
        }
    }

    /// The setup screen's check (`docs/04` §19): what the engine would
    /// refuse, before a match is started on it.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.config().validate()
    }

    /// Moves a setting one step. Choices from a list wrap; counts stop at
    /// their bounds.
    pub fn adjust(&mut self, field: Field, delta: i32) {
        match field {
            Field::Map => {}
            Field::Size => self.size = cycle(&MapSize::ALL, self.size, delta),
            Field::Opponents => {
                let n = (self.opponents.len() as i32 + delta).clamp(1, MAX_OPPONENTS as i32);
                self.opponents.resize(n as usize, Difficulty::Standard);
            }
            Field::Difficulty(i) => {
                if let Some(d) = self.opponents.get_mut(i) {
                    *d = cycle(&Difficulty::ALL, *d, delta);
                }
            }
            Field::PopCap => {
                let step = delta * POP_CAP_STEP as i32;
                let (lo, hi) = (*POP_CAP_RANGE.start() as i32, *POP_CAP_RANGE.end() as i32);
                self.pop_cap = (self.pop_cap as i32 + step).clamp(lo, hi) as u32;
            }
            Field::Seed => {
                self.seed = if delta < 0 {
                    self.seed.saturating_sub(delta.unsigned_abs() as u64)
                } else {
                    self.seed.saturating_add(delta as u64)
                };
            }
        }
    }
}

/// The entry `delta` steps from `current` in `all`, wrapping.
fn cycle<T: Copy + PartialEq>(all: &[T], current: T, delta: i32) -> T {
    let n = all.len() as i32;
    let i = all.iter().position(|x| *x == current).unwrap_or(0) as i32;
    all[((i + delta).rem_euclid(n)) as usize]
}

/// Which page of the settings screen is up.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SettingsPage {
    /// The settings, the volumes and the general keys.
    #[default]
    Keys,
    /// The command panels' letters (`GD-A11Y-02`).
    Letters,
}

/// What the settings screen is waiting for a new key for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Capture {
    /// A general control.
    Control(Control),
    /// A panel letter.
    Letter(char),
}

/// What a shell button does when clicked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShellAction {
    /// Title: open the setup screen.
    NewGame,
    /// Title: open the load screen (not yet).
    LoadGame,
    /// Title: open the replay list (not yet).
    WatchReplay,
    /// Title: open the settings (not yet).
    Settings,
    /// Title: close the game.
    Quit,
    /// Setup: move a setting by a step.
    Adjust(Field, i32),
    /// Setup: pick a seed at random.
    Shuffle,
    /// Setup: start the match.
    Start,
    /// Setup: back to the title.
    Back,
    /// Pause menu: close it.
    Resume,
    /// Pause menu: give up the match (`GD-WIN-01`).
    Resign,
    /// Pause menu and results: abandon the match for the title.
    QuitToTitle,
    /// Results: put the panel away and watch the match play out.
    KeepWatching,
    /// Pause menu: write a save of the match as it stands.
    Save,
    /// Load screen: load the save on this row.
    Load(usize),
    /// Replay screen: watch the recording on this row.
    Watch(usize),
    /// Settings: the next HUD size along.
    SettingScale(i32),
    /// Settings: edge scrolling on or off.
    ToggleEdgeScroll,
    /// Settings: fullscreen or a window.
    ToggleFullscreen,
    /// Settings: wait for a new key for this control.
    Rebind(Control),
    /// Settings: wait for a new key for this panel letter.
    RebindLetter(char),
    /// Settings: show this page.
    SettingsPage(SettingsPage),
    /// Settings: everything back to the defaults.
    ResetSettings,
    /// Settings: a bus's volume, a step of ten percent up or down.
    Volume(Bus, i32),
    /// Settings: the first-time hints on or off.
    ToggleHints,
}

/// A clickable region on a shell screen.
#[derive(Clone, PartialEq, Debug)]
pub struct ShellButton {
    /// Window px.
    pub x: f32,
    /// Window px.
    pub y: f32,
    /// Size.
    pub w: f32,
    /// Size.
    pub h: f32,
    /// What it does.
    pub action: ShellAction,
    /// Label text.
    pub label: String,
    /// Whether clicking does anything right now.
    pub enabled: bool,
    /// Why not, when it does not.
    pub reason: String,
}

impl ShellButton {
    /// True if a window point is inside.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// A built shell screen.
#[derive(Clone, Default, Debug)]
pub struct Screen {
    /// Screen-space sprites in draw order.
    pub sprites: Vec<SpriteInstance>,
    /// Clickable buttons.
    pub buttons: Vec<ShellButton>,
    /// Where the map preview goes, in window px, if the screen has one.
    pub preview: Option<MinimapRect>,
}

/// What every shell screen needs to draw.
#[derive(Clone, Copy, Debug)]
pub struct ShellInput {
    /// The window, in device px.
    pub viewport: (f32, f32),
    /// Device pixels per HUD pixel, as for the HUD.
    pub ui_scale: f32,
    /// Cursor position in device px, for button hover.
    pub hover: Option<(f32, f32)>,
}

/// How the match ended, for the results screen.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Results {
    /// VICTORY, DEFEAT, or REPLAY OVER.
    pub heading: String,
    /// One line on how it was decided.
    pub why: String,
    /// Every side, in player order.
    pub sides: Vec<Side>,
}

/// One side on the results screen.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Side {
    /// Player index, for the colour.
    pub player: u8,
    /// "YOU", or the opponent's difficulty.
    pub name: String,
    /// The score (`docs/02` §10).
    pub score: u64,
    /// Whether the side is still in the match.
    pub standing: bool,
}

/// Height of a setup row.
const ROW_H: f32 = 18.0;
/// A menu button.
const MENU_W: f32 = 240.0;
/// A menu button.
const MENU_H: f32 = 28.0;
/// A step button on the setup screen.
const STEP_W: f32 = 20.0;
/// A step button on the setup screen.
const STEP_H: f32 = 16.0;

/// A screen under construction, in HUD pixels.
struct Sheet<'a> {
    p: Painter<'a>,
    buttons: Vec<ShellButton>,
    vw: f32,
    vh: f32,
    hover: Option<(f32, f32)>,
    scale: f32,
}

impl<'a> Sheet<'a> {
    fn new(atlas: &'a Atlas, input: &ShellInput) -> Sheet<'a> {
        let s = input.ui_scale.max(0.5);
        Sheet {
            p: Painter::new(atlas),
            buttons: Vec::new(),
            vw: input.viewport.0 / s,
            vh: input.viewport.1 / s,
            hover: input.hover.map(|(x, y)| (x / s, y / s)),
            scale: s,
        }
    }

    /// Covers the whole window: the shell screens stand on nothing.
    fn backdrop(&mut self) {
        self.p.rect(0.0, 0.0, self.vw, self.vh, BLACK, 0);
    }

    /// The framed panel the HUD's overlays use.
    fn panel(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.p.rect(x, y, w, h, BLACK, 0);
        self.p
            .rect(x + 2.0, y + 2.0, w - 4.0, h - 4.0, BROWN_DARK, 0);
        self.p.rect(x + 2.0, y + 2.0, w - 4.0, 2.0, GOLD, 0);
        self.p.rect(x + 2.0, y + h - 4.0, w - 4.0, 2.0, GOLD, 0);
    }

    /// Text centred on a vertical line.
    fn centred(&mut self, cx: f32, y: f32, text: &str, ink: Ink, scale: f32) {
        let w = font::width(text) as f32 * scale;
        self.p.text_in((cx - w / 2.0).round(), y, text, ink, scale);
    }

    /// A button with its label centred, in the `(x, y, w, h)` rectangle.
    /// A disabled one is greyed and says why beside it.
    fn button(
        &mut self,
        rect: (f32, f32, f32, f32),
        action: ShellAction,
        label: &str,
        enabled: bool,
        reason: &str,
    ) {
        let (x, y, w, h) = rect;
        let hover = self
            .hover
            .is_some_and(|(hx, hy)| hx >= x && hx < x + w && hy >= y && hy < y + h);
        let (fill, ink) = if !enabled {
            (GREY_DARK, Ink::White)
        } else if hover {
            (TAN, Ink::Black)
        } else {
            (BROWN, Ink::Black)
        };
        self.p.rect(x, y, w, h, BLACK, 0);
        self.p.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, fill, 0);
        let label = fit(label, w - 6.0);
        let lw = font::width(&label) as f32;
        self.p.text_in(
            (x + (w - lw) / 2.0).round(),
            (y + (h - font::GLYPH_H as f32) / 2.0).round(),
            &label,
            ink,
            1.0,
        );
        if !enabled && !reason.is_empty() {
            self.p
                .text(x + w + 8.0, y + (h - 7.0) / 2.0, reason, false, 1.0);
        }
        self.buttons.push(ShellButton {
            x,
            y,
            w,
            h,
            action,
            label,
            enabled,
            reason: reason.to_string(),
        });
    }

    /// A stack of menu buttons centred on `cx`, from `y` down.
    fn menu(&mut self, cx: f32, y: f32, entries: &[(ShellAction, &str, bool, &str)]) -> f32 {
        let x = (cx - MENU_W / 2.0).round();
        let mut by = y;
        for (action, label, enabled, reason) in entries {
            self.button((x, by, MENU_W, MENU_H), *action, label, *enabled, reason);
            by += MENU_H + 8.0;
        }
        by
    }

    /// Everything above is in HUD pixels; the window wants device pixels.
    fn finish(self, preview: Option<MinimapRect>) -> Screen {
        let s = self.scale;
        let mut sprites = self.p.out;
        let mut buttons = self.buttons;
        let mut preview = preview;
        if s != 1.0 {
            for sp in &mut sprites {
                sp.x *= s;
                sp.y *= s;
                sp.w *= s;
                sp.h *= s;
            }
            for b in &mut buttons {
                b.x *= s;
                b.y *= s;
                b.w *= s;
                b.h *= s;
            }
            if let Some(r) = &mut preview {
                r.cx *= s;
                r.cy *= s;
                r.w *= s;
                r.h *= s;
            }
        }
        Screen {
            sprites,
            buttons,
            preview,
        }
    }
}

/// The title screen: the name and the main menu.
pub fn title(atlas: &Atlas, input: &ShellInput) -> Screen {
    let mut s = Sheet::new(atlas, input);
    s.backdrop();
    let cx = (s.vw / 2.0).round();
    let ty = (s.vh * 0.2).round();
    s.centred(cx, ty, TITLE, Ink::Gold, 4.0);
    s.centred(cx, ty + 40.0, TAGLINE, Ink::White, 1.0);
    let entries = [
        (ShellAction::NewGame, "NEW GAME", true, ""),
        (ShellAction::LoadGame, "LOAD GAME", true, ""),
        (ShellAction::WatchReplay, "WATCH REPLAY", true, ""),
        (ShellAction::Settings, "SETTINGS", true, ""),
        (ShellAction::Quit, "QUIT", true, ""),
    ];
    let my = (s.vh * 0.42).round().max(ty + 64.0);
    s.menu(cx, my, &entries);
    s.centred(
        cx,
        s.vh - 24.0,
        "ENTER: NEW GAME   ESC: QUIT",
        Ink::White,
        1.0,
    );
    let version = format!("V{}", env!("CARGO_PKG_VERSION"));
    let vw = font::width(&version) as f32;
    s.p.text(s.vw - vw - 8.0, s.vh - 12.0, &version, false, 1.0);
    s.finish(None)
}

/// The width the setup panel wants; narrower windows lose the preview.
const SETUP_W: f32 = 780.0;
/// Below this panel width the map preview is left out.
const PREVIEW_MIN_W: f32 = 740.0;
/// The preview diamond's width.
const PREVIEW_W: f32 = 220.0;

/// The skirmish setup screen: the settings, the preview and START.
/// `error` is what the engine's check refused, if anything.
pub fn setup(atlas: &Atlas, input: &ShellInput, setup: &Setup, error: Option<&str>) -> Screen {
    let mut s = Sheet::new(atlas, input);
    s.backdrop();
    let rows = 3 + setup.opponents.len() + 2;
    let pw = SETUP_W.min(s.vw - 16.0);
    let with_preview = pw >= PREVIEW_MIN_W;
    let rows_h = rows as f32 * ROW_H;
    let body_h = if with_preview {
        rows_h.max(PREVIEW_W / 2.0 + 16.0)
    } else {
        rows_h
    };
    let ph = 40.0 + body_h + 16.0 + 14.0 + 12.0 + 40.0 + 16.0;
    let x = ((s.vw - pw) / 2.0).round();
    let y = ((s.vh - ph) / 2.0).max(4.0).round();
    s.panel(x, y, pw, ph);
    s.centred(x + pw / 2.0, y + 10.0, "SKIRMISH", Ink::Gold, 2.0);

    // The settings, one row each: label, minus, value, plus, note.
    let (label_x, minus_x, value_x, value_w) = (x + 16.0, x + 200.0, x + 224.0, 120.0);
    let plus_x = value_x + value_w + 4.0;
    let note_x = plus_x + STEP_W + 12.0;
    let mut ry = y + 40.0;
    let mut row = |s: &mut Sheet<'_>,
                   label: &str,
                   value: &str,
                   field: Field,
                   enabled: bool,
                   reason: &str,
                   swatch: Option<u8>| {
        let ty = ry + (ROW_H - 7.0) / 2.0 - 1.0;
        let mut lx = label_x;
        if let Some(p) = swatch {
            s.p.rect(lx, ty - 2.0, 11.0, 11.0, BLACK, 0);
            s.p.rect(lx + 1.0, ty - 1.0, 9.0, 9.0, P_BASE, row_for_owner(p));
            lx += 16.0;
        }
        s.p.text(lx, ty, label, false, 1.0);
        let by = ry + (ROW_H - STEP_H) / 2.0;
        s.button(
            (minus_x, by, STEP_W, STEP_H),
            ShellAction::Adjust(field, -1),
            "-",
            enabled,
            "",
        );
        s.centred(value_x + value_w / 2.0, ty, value, Ink::Gold, 1.0);
        s.button(
            (plus_x, by, STEP_W, STEP_H),
            ShellAction::Adjust(field, 1),
            "+",
            enabled,
            "",
        );
        if !reason.is_empty() {
            s.p.text(note_x, ty, reason, false, 1.0);
        }
        ry += ROW_H;
        ty
    };
    row(
        &mut s,
        "MAP",
        &kind_name(setup.kind),
        Field::Map,
        false,
        "MORE MAPS IN M8",
        None,
    );
    row(
        &mut s,
        "SIZE",
        &format!(
            "{} {}",
            setup.size.name().to_uppercase(),
            setup.size.tiles()
        ),
        Field::Size,
        true,
        "",
        None,
    );
    row(
        &mut s,
        "OPPONENTS",
        &setup.opponents.len().to_string(),
        Field::Opponents,
        true,
        "",
        None,
    );
    for (i, d) in setup.opponents.iter().enumerate() {
        let player = i as u8 + 1;
        let bonus = setup.declared_bonus(player);
        let ty = row(
            &mut s,
            &format!("PLAYER {}", player + 1),
            &d.name().to_uppercase(),
            Field::Difficulty(i),
            true,
            "",
            Some(player),
        );
        // The declared bonus (`GD-AI-01`): the one thing an opponent may
        // have that the player may not, said where it is chosen.
        if bonus > 0 {
            s.p.text_in(
                note_x,
                ty,
                &format!("+{bonus}% GATHER RATE"),
                Ink::Gold,
                1.0,
            );
        }
    }
    row(
        &mut s,
        "POPULATION CAP",
        &setup.pop_cap.to_string(),
        Field::PopCap,
        true,
        "",
        None,
    );
    let seed_ty = row(
        &mut s,
        "SEED",
        &setup.seed.to_string(),
        Field::Seed,
        true,
        "",
        None,
    );
    s.button(
        (note_x, seed_ty - 5.0, 70.0, STEP_H),
        ShellAction::Shuffle,
        "SHUFFLE",
        true,
        "",
    );

    // The map for this seed, as the minimap will show it.
    let preview = with_preview.then(|| {
        let px = x + pw - 16.0 - PREVIEW_W;
        let py = y + 40.0;
        s.p.text(px, py + 4.0, "PREVIEW", false, 1.0);
        MinimapRect {
            cx: px + PREVIEW_W / 2.0,
            cy: py + 16.0 + PREVIEW_W / 4.0,
            w: PREVIEW_W,
            h: PREVIEW_W / 2.0,
        }
    });

    let below = y + 40.0 + body_h + 16.0;
    s.centred(
        x + pw / 2.0,
        below,
        "OPPONENTS PLAY BY YOUR RULES AND SEE ONLY WHAT THEY SCOUT. HARDEST DECLARES ITS BONUS.",
        Ink::White,
        1.0,
    );
    if let Some(e) = error {
        let text = fit(&e.to_uppercase(), pw - 32.0);
        let tw = font::width(&text) as f32;
        let ex = (x + (pw - tw) / 2.0).round() - 4.0;
        s.p.rect(ex, below + 12.0, tw + 8.0, 11.0, RED_DARK, 0);
        s.p.text(ex + 4.0, below + 14.0, &text, false, 1.0);
    }
    let by = y + ph - 16.0 - 26.0;
    s.button(
        (x + 16.0, by, 120.0, 26.0),
        ShellAction::Back,
        "BACK",
        true,
        "",
    );
    s.button(
        (x + pw - 16.0 - 120.0, by, 120.0, 26.0),
        ShellAction::Start,
        "START",
        error.is_none(),
        "",
    );
    s.centred(
        x + pw / 2.0,
        by + 10.0,
        "ENTER: START   ESC: BACK",
        Ink::White,
        1.0,
    );
    s.finish(preview)
}

/// The map generator's name for the setup screen.
fn kind_name(kind: MapKind) -> String {
    match kind {
        MapKind::Flat => "FLAT".to_string(),
        MapKind::Inland => "INLAND".to_string(),
    }
}

/// The settings screen's second page (`GD-A11Y-02`): every letter the
/// command panels use, the key it is bound to, and CHANGE; the footer
/// shared with the first page.
fn letters_page(
    s: &mut Sheet<'_>,
    settings: &Settings,
    capturing: Option<Capture>,
    error: Option<&str>,
    (x, y, pw, ph): (f32, f32, f32, f32),
) {
    let letters: Vec<char> = crate::hud::command_letters().into_iter().collect();
    let per_column = letters.len().div_ceil(2);
    let col_w = (pw - 32.0) / 2.0;
    s.p.text_in(
        x + 16.0,
        y + 40.0 + (ROW_H - 7.0) / 2.0 - 1.0,
        "PANEL LETTERS",
        Ink::Gold,
        1.0,
    );
    for (n, &letter) in letters.iter().enumerate() {
        let cx = x + 16.0 + (n / per_column) as f32 * col_w;
        let ry = y + 40.0 + ROW_H * (1 + n % per_column) as f32;
        let ty = ry + (ROW_H - 7.0) / 2.0 - 1.0;
        s.p.text(cx, ty, &format!("LETTER {letter}"), false, 1.0);
        let waiting = capturing == Some(Capture::Letter(letter));
        let value = if waiting {
            "PRESS A KEY".to_string()
        } else {
            settings.letter_shown(letter)
        };
        s.centred(cx + 160.0, ty, &value, Ink::Gold, 1.0);
        s.button(
            (cx + 230.0, ry + (ROW_H - STEP_H) / 2.0, 70.0, STEP_H),
            ShellAction::RebindLetter(letter),
            "CHANGE",
            !waiting,
            "",
        );
    }
    let ry = y + 40.0 + ROW_H * (per_column + 1) as f32 + 4.0;
    let note = if capturing.is_some() {
        "PRESS THE NEW KEY. ESC KEEPS THE OLD ONE."
    } else {
        "A LETTER KEEPS ITS MEANING ON EVERY PANEL. A KEY ANOTHER LETTER HOLDS IS SWAPPED."
    };
    s.centred(x + pw / 2.0, ry, note, Ink::White, 1.0);
    if let Some(e) = error {
        let text = fit(&e.to_uppercase(), pw - 32.0);
        let tw = font::width(&text) as f32;
        let ex = (x + (pw - tw) / 2.0).round() - 4.0;
        s.p.rect(ex, ry + 12.0, tw + 8.0, 11.0, RED_DARK, 0);
        s.p.text(ex + 4.0, ry + 14.0, &text, false, 1.0);
    }
    footer(s, x, y, pw, ph);
}

/// BACK, DEFAULTS and the line between them, at the foot of either page.
fn footer(s: &mut Sheet<'_>, x: f32, y: f32, pw: f32, ph: f32) {
    let by = y + ph - 16.0 - 26.0;
    s.button(
        (x + 16.0, by, 120.0, 26.0),
        ShellAction::Back,
        "BACK",
        true,
        "",
    );
    s.button(
        (x + pw - 16.0 - 120.0, by, 120.0, 26.0),
        ShellAction::ResetSettings,
        "DEFAULTS",
        true,
        "",
    );
    s.centred(
        x + pw / 2.0,
        by + 10.0,
        "CHANGES ARE KEPT AT ONCE. ESC: BACK",
        Ink::White,
        1.0,
    );
}

/// The settings screen (`GD-A11Y-02`): the HUD size, edge scrolling, the
/// window mode, and every general key with a CHANGE button. `capturing`
/// is the control waiting for its new key; `error` is why the last key
/// was refused.
pub fn settings_screen(
    atlas: &Atlas,
    input: &ShellInput,
    settings: &Settings,
    page: SettingsPage,
    capturing: Option<Capture>,
    error: Option<&str>,
) -> Screen {
    let mut s = Sheet::new(atlas, input);
    s.backdrop();
    let rows = 4 + Control::ALL.len();
    // Wide enough for the audio column beside the keys.
    let pw = 800.0_f32.min(s.vw - 16.0);
    let ph = 40.0 + rows as f32 * ROW_H + 14.0 + 12.0 + 26.0 + 16.0 + 8.0;
    let x = ((s.vw - pw) / 2.0).round();
    let y = ((s.vh - ph) / 2.0).max(4.0).round();
    s.panel(x, y, pw, ph);
    s.centred(x + pw / 2.0, y + 10.0, "SETTINGS", Ink::Gold, 2.0);
    // The other page, from the top right.
    let (other, other_label) = match page {
        SettingsPage::Keys => (SettingsPage::Letters, "PANEL LETTERS"),
        SettingsPage::Letters => (SettingsPage::Keys, "GENERAL KEYS"),
    };
    s.button(
        (x + pw - 16.0 - 130.0, y + 8.0, 130.0, 20.0),
        ShellAction::SettingsPage(other),
        other_label,
        true,
        "",
    );
    if page == SettingsPage::Letters {
        letters_page(&mut s, settings, capturing, error, (x, y, pw, ph));
        return s.finish(None);
    }
    let capturing = match capturing {
        Some(Capture::Control(c)) => Some(c),
        _ => None,
    };
    let (label_x, minus_x, value_x, value_w) = (x + 16.0, x + 236.0, x + 260.0, 140.0);
    let mut ry = y + 40.0;
    // One row: the label, a step button, the value, a step or CHANGE
    // button. `column` is (the label's shift, the buttons' shift, how
    // much narrower the value is), for the audio column. Takes the
    // row's top and hands back the next one's.
    let row = |s: &mut Sheet<'_>,
               ry: f32,
               column: (f32, f32, f32),
               label: &str,
               value: &str,
               minus: (ShellAction, &str),
               plus: (ShellAction, &str, bool)|
     -> f32 {
        let (label_shift, shift, value_w) = (column.0, column.1, value_w - column.2);
        let ty = ry + (ROW_H - 7.0) / 2.0 - 1.0;
        s.p.text(label_x + label_shift, ty, label, false, 1.0);
        let by = ry + (ROW_H - STEP_H) / 2.0;
        if !minus.1.is_empty() {
            s.button(
                (minus_x + shift, by, STEP_W, STEP_H),
                minus.0,
                minus.1,
                true,
                "",
            );
        }
        s.centred(value_x + shift + value_w / 2.0, ty, value, Ink::Gold, 1.0);
        let (pw_, label_) = if plus.1.len() > 1 {
            (70.0, plus.1)
        } else {
            (STEP_W, plus.1)
        };
        s.button(
            (value_x + shift + value_w + 4.0, by, pw_, STEP_H),
            plus.0,
            label_,
            plus.2,
            "",
        );
        ry + ROW_H
    };
    let full = (0.0, 0.0, 0.0);
    // The audio column: its step buttons end at the panel's margin, the
    // value narrowed to a percentage, the label close beside them and
    // clear of the CHANGE buttons.
    let narrow = 60.0;
    let shift = pw - 16.0 - STEP_W - 4.0 - narrow - (value_x - x);
    let audio = (shift + 120.0, shift, value_w - narrow);
    let mut ay = ry;
    let vy = ay + (ROW_H - 7.0) / 2.0 - 1.0;
    s.p.text_in(label_x + audio.0, vy, "VOLUME", Ink::Gold, 1.0);
    ay += ROW_H;
    for bus in Bus::ALL {
        ay = row(
            &mut s,
            ay,
            audio,
            bus.name(),
            &format!("{}%", settings.volume(bus)),
            (ShellAction::Volume(bus, -1), "-"),
            (ShellAction::Volume(bus, 1), "+", true),
        );
    }
    row(
        &mut s,
        ay,
        audio,
        "HINTS",
        if settings.hints { "ON" } else { "OFF" },
        (ShellAction::ToggleHints, "-"),
        (ShellAction::ToggleHints, "+", true),
    );
    ry = row(
        &mut s,
        ry,
        full,
        "HUD SIZE",
        &format!("{}%", (settings.ui_scale * 100.0).round() as i32),
        (ShellAction::SettingScale(-1), "-"),
        (ShellAction::SettingScale(1), "+", true),
    );
    ry = row(
        &mut s,
        ry,
        full,
        "EDGE SCROLL",
        if settings.edge_scroll { "ON" } else { "OFF" },
        (ShellAction::ToggleEdgeScroll, "-"),
        (ShellAction::ToggleEdgeScroll, "+", true),
    );
    ry = row(
        &mut s,
        ry,
        full,
        "WINDOW",
        if settings.fullscreen {
            "FULLSCREEN"
        } else {
            "WINDOWED"
        },
        (ShellAction::ToggleFullscreen, "-"),
        (ShellAction::ToggleFullscreen, "+", true),
    );
    let ky = ry + (ROW_H - 7.0) / 2.0 - 1.0;
    s.p.text_in(label_x, ky, "KEYS", Ink::Gold, 1.0);
    ry += ROW_H;
    for c in Control::ALL {
        let (value, enabled) = if capturing == Some(c) {
            ("PRESS A KEY".to_string(), false)
        } else {
            (pretty(settings.key(c)), true)
        };
        ry = row(
            &mut s,
            ry,
            full,
            c.name(),
            &value,
            (ShellAction::Rebind(c), ""),
            (ShellAction::Rebind(c), "CHANGE", enabled),
        );
    }
    let note = if capturing.is_some() {
        "PRESS THE NEW KEY. ESC KEEPS THE OLD ONE."
    } else {
        "DIGITS AND ESC CANNOT BE TAKEN; ONLY A PAN KEY MAY TAKE A PANEL LETTER."
    };
    s.centred(x + pw / 2.0, ry + 4.0, note, Ink::White, 1.0);
    if let Some(e) = error {
        let text = fit(&e.to_uppercase(), pw - 32.0);
        let tw = font::width(&text) as f32;
        let ex = (x + (pw - tw) / 2.0).round() - 4.0;
        s.p.rect(ex, ry + 16.0, tw + 8.0, 11.0, RED_DARK, 0);
        s.p.text(ex + 4.0, ry + 18.0, &text, false, 1.0);
    }
    footer(&mut s, x, y, pw, ph);
    s.finish(None)
}

/// The pause menu, over a match. `decided` greys RESIGN once the match is
/// over; `confirm` is the button awaiting its second click.
pub fn pause_menu(
    atlas: &Atlas,
    input: &ShellInput,
    decided: bool,
    confirm: Option<ShellAction>,
    saved: Option<&str>,
    replay: bool,
) -> Screen {
    let mut s = Sheet::new(atlas, input);
    let (pw, ph) = (300.0, 40.0 + 4.0 * (MENU_H + 8.0) + 40.0);
    let x = ((s.vw - pw) / 2.0).round();
    let y = ((s.vh - ph) / 2.0).max(4.0).round();
    s.panel(x, y, pw, ph);
    let cx = x + pw / 2.0;
    s.centred(
        cx,
        y + 10.0,
        if decided { "MATCH OVER" } else { "PAUSED" },
        Ink::Gold,
        2.0,
    );
    let resign = if confirm == Some(ShellAction::Resign) {
        "CONFIRM RESIGN"
    } else {
        "RESIGN"
    };
    let quit = if confirm == Some(ShellAction::QuitToTitle) {
        "CONFIRM QUIT"
    } else {
        "QUIT TO TITLE"
    };
    // A replay is watched, not played: nothing to save or give up.
    let entries = [
        (ShellAction::Resume, "RESUME", true, ""),
        (ShellAction::Save, "SAVE GAME", !replay, ""),
        (ShellAction::Resign, resign, !decided && !replay, ""),
        (ShellAction::QuitToTitle, quit, true, ""),
    ];
    let below = s.menu(cx, y + 40.0, &entries);
    if let Some(note) = saved {
        s.centred(cx, below + 2.0, &fit(note, pw - 16.0), Ink::Gold, 1.0);
    }
    s.centred(cx, y + ph - 18.0, "ESC RESUMES   F5 SAVES", Ink::White, 1.0);
    s.finish(None)
}

/// One save on the load screen.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LoadRow {
    /// What the match is: the seed and how far it got.
    pub title: String,
    /// When it was saved and who was in it.
    pub detail: String,
}

/// The most saves the load screen lists.
pub const LOAD_ROWS: usize = 10;

/// The load screen, or with `watch` the replay screen: the files newest
/// first, one button each, and what went wrong with the last one tried,
/// if anything.
pub fn load_screen(
    atlas: &Atlas,
    input: &ShellInput,
    rows: &[LoadRow],
    error: Option<&str>,
    watch: bool,
) -> Screen {
    let mut s = Sheet::new(atlas, input);
    s.backdrop();
    let shown = rows.len().clamp(1, LOAD_ROWS);
    let (row_h, gap) = (24.0, 4.0);
    let pw = 640.0_f32.min(s.vw - 16.0);
    let ph = 40.0 + shown as f32 * (row_h + gap) + 16.0 + 14.0 + 12.0 + 26.0 + 16.0;
    let x = ((s.vw - pw) / 2.0).round();
    let y = ((s.vh - ph) / 2.0).max(4.0).round();
    s.panel(x, y, pw, ph);
    let heading = if watch { "WATCH REPLAY" } else { "LOAD GAME" };
    s.centred(x + pw / 2.0, y + 10.0, heading, Ink::Gold, 2.0);
    let mut ry = y + 40.0;
    if rows.is_empty() {
        let none = if watch {
            "NO RECORDINGS YET. EVERY MATCH PLAYED IS RECORDED."
        } else {
            "NO SAVES YET. F5 OR THE PAUSE MENU SAVES A MATCH."
        };
        s.centred(x + pw / 2.0, ry + 8.0, none, Ink::White, 1.0);
        ry += row_h + gap;
    }
    let button_w = 280.0;
    for (i, row) in rows.iter().take(LOAD_ROWS).enumerate() {
        s.button(
            (x + 16.0, ry, button_w, row_h),
            if watch {
                ShellAction::Watch(i)
            } else {
                ShellAction::Load(i)
            },
            &row.title,
            true,
            "",
        );
        s.p.text(
            x + 16.0 + button_w + 12.0,
            ry + (row_h - 7.0) / 2.0,
            &fit(&row.detail, pw - button_w - 44.0),
            false,
            1.0,
        );
        ry += row_h + gap;
    }
    if rows.len() > LOAD_ROWS {
        s.centred(
            x + pw / 2.0,
            ry + 2.0,
            &format!("AND {} OLDER", rows.len() - LOAD_ROWS),
            Ink::White,
            1.0,
        );
    }
    if let Some(e) = error {
        let text = fit(&e.to_uppercase(), pw - 32.0);
        let tw = font::width(&text) as f32;
        let ex = (x + (pw - tw) / 2.0).round() - 4.0;
        s.p.rect(ex, ry + 14.0, tw + 8.0, 11.0, RED_DARK, 0);
        s.p.text(ex + 4.0, ry + 16.0, &text, false, 1.0);
    }
    let by = y + ph - 16.0 - 26.0;
    s.button(
        (x + 16.0, by, 120.0, 26.0),
        ShellAction::Back,
        "BACK",
        true,
        "",
    );
    s.centred(
        x + pw / 2.0 + 60.0,
        by + 10.0,
        "ENTER: THE NEWEST   ESC: BACK",
        Ink::White,
        1.0,
    );
    s.finish(None)
}

/// The results screen, over the decided match.
pub fn results(atlas: &Atlas, input: &ShellInput, r: &Results) -> Screen {
    let mut s = Sheet::new(atlas, input);
    let rows = r.sides.len() as f32;
    let (pw, ph) = (
        460.0,
        40.0 + 14.0 + 16.0 + rows * 14.0 + 16.0 + MENU_H + 16.0,
    );
    let x = ((s.vw - pw) / 2.0).round();
    let y = ((s.vh - ph) / 2.0).max(4.0).round();
    s.panel(x, y, pw, ph);
    let cx = x + pw / 2.0;
    s.centred(cx, y + 10.0, &r.heading, Ink::Gold, 2.0);
    s.centred(cx, y + 40.0, &r.why, Ink::White, 1.0);
    let (col_side, col_score, col_status) = (x + 24.0, x + 250.0, x + 340.0);
    let hy = y + 58.0;
    s.p.text_in(col_side, hy, "SIDE", Ink::Gold, 1.0);
    s.p.text_in(col_score, hy, "SCORE", Ink::Gold, 1.0);
    s.p.text_in(col_status, hy, "STATUS", Ink::Gold, 1.0);
    for (i, side) in r.sides.iter().enumerate() {
        let ly = hy + 14.0 + i as f32 * 14.0;
        s.p.rect(col_side, ly - 1.0, 9.0, 9.0, BLACK, 0);
        s.p.rect(
            col_side + 1.0,
            ly,
            7.0,
            7.0,
            P_BASE,
            row_for_owner(side.player),
        );
        s.p.text(
            col_side + 14.0,
            ly,
            &fit(&side.name.to_uppercase(), col_score - col_side - 20.0),
            false,
            1.0,
        );
        s.p.text(col_score, ly, &side.score.to_string(), false, 1.0);
        s.p.text(
            col_status,
            ly,
            if side.standing { "STANDING" } else { "OUT" },
            false,
            1.0,
        );
    }
    let by = y + ph - 16.0 - MENU_H;
    s.button(
        (x + 16.0, by, 200.0, MENU_H),
        ShellAction::KeepWatching,
        "KEEP WATCHING",
        true,
        "",
    );
    s.button(
        (x + pw - 16.0 - 200.0, by, 200.0, MENU_H),
        ShellAction::QuitToTitle,
        "BACK TO TITLE",
        true,
        "",
    );
    s.finish(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ShellInput {
        ShellInput {
            viewport: (1280.0, 720.0),
            ui_scale: 1.0,
            hover: None,
        }
    }

    fn find(screen: &Screen, action: ShellAction) -> &ShellButton {
        screen
            .buttons
            .iter()
            .find(|b| b.action == action)
            .unwrap_or_else(|| panic!("no button for {action:?}"))
    }

    fn inside(screen: &Screen, input: &ShellInput) -> bool {
        screen.buttons.iter().all(|b| {
            b.x >= 0.0
                && b.y >= 0.0
                && b.x + b.w <= input.viewport.0
                && b.y + b.h <= input.viewport.1
        })
    }

    /// The setup's choices become the match's parameters, and only a
    /// Hardest opponent gets the declared bonus: never the human.
    ///
    /// REQ: GD-AI-01
    #[test]
    fn the_setup_is_the_match_and_only_hardest_gets_its_declared_bonus() {
        let mut setup = Setup::new(7);
        assert_eq!(setup.players(), 2);
        setup.opponents = vec![Difficulty::Easy, Difficulty::Hardest, Difficulty::Hard];
        setup.size = MapSize::Medium;
        setup.pop_cap = 100;
        let config = setup.config();
        assert_eq!(config.map.players, 4);
        assert_eq!(config.map.size, 168);
        assert_eq!(config.map.kind, MapKind::Inland);
        assert_eq!(config.pop_cap_max, 100);
        assert_eq!(
            config.gather_bonus_pct,
            vec![0, 0, HARDEST_GATHER_BONUS_PCT, 0]
        );
        assert_eq!(setup.declared_bonus(0), 0, "the human never has one");
        setup
            .validate()
            .expect("a setup the screen offers is valid");
    }

    /// Every setup the screen can reach passes the engine's check: the
    /// arrows stop at the bounds the engine enforces.
    #[test]
    fn every_reachable_setup_is_valid_and_the_arrows_stop_at_the_bounds() {
        let mut setup = Setup::new(1);
        for _ in 0..10 {
            setup.adjust(Field::Opponents, 1);
        }
        assert_eq!(setup.opponents.len(), MAX_OPPONENTS);
        for _ in 0..10 {
            setup.adjust(Field::PopCap, 1);
        }
        assert_eq!(setup.pop_cap, *POP_CAP_RANGE.end());
        setup.validate().expect("the largest setup");
        for _ in 0..20 {
            setup.adjust(Field::PopCap, -1);
            setup.adjust(Field::Opponents, -1);
        }
        assert_eq!(setup.pop_cap, *POP_CAP_RANGE.start());
        assert_eq!(setup.opponents.len(), 1, "always at least one opponent");
        setup.validate().expect("the smallest setup");
        // Lists wrap both ways.
        assert_eq!(setup.size, MapSize::Small);
        setup.adjust(Field::Size, -2);
        assert_eq!(setup.size, MapSize::Giant);
        setup.adjust(Field::Size, 1);
        assert_eq!(setup.size, MapSize::Tiny);
        setup.adjust(Field::Difficulty(0), -1);
        assert_eq!(setup.opponents[0], Difficulty::Easy);
        setup.adjust(Field::Difficulty(0), -1);
        assert_eq!(setup.opponents[0], Difficulty::Hardest);
        setup.adjust(Field::Difficulty(5), 1);
        // The seed stops at zero rather than wrapping to twenty digits.
        setup.seed = 0;
        setup.adjust(Field::Seed, -1);
        assert_eq!(setup.seed, 0);
        setup.adjust(Field::Seed, 1);
        assert_eq!(setup.seed, 1);
        for size in MapSize::ALL {
            assert_eq!(MapSize::from_tiles(size.tiles()), Some(size));
        }
        assert_eq!(MapSize::from_tiles(100), None);
    }

    /// The title lists the shell's five entries, with the ones that do
    /// not exist yet greyed and saying so, and every button on screen.
    #[test]
    fn the_title_offers_a_new_game_and_says_what_is_not_there_yet() {
        let input = input();
        let screen = title(&Atlas::placeholder(), &input);
        assert_eq!(screen.buttons.len(), 5);
        assert!(find(&screen, ShellAction::NewGame).enabled);
        assert!(find(&screen, ShellAction::LoadGame).enabled);
        assert!(find(&screen, ShellAction::WatchReplay).enabled);
        assert!(find(&screen, ShellAction::Quit).enabled);
        assert!(find(&screen, ShellAction::Settings).enabled);
        assert!(
            screen.buttons.iter().all(|b| b.enabled),
            "every entry is built"
        );
        assert!(inside(&screen, &input));
        assert!(screen.preview.is_none());
        assert!(screen.sprites.iter().all(|s| s.screen));
        // Every glyph of the name is in the font.
        assert!(TITLE.chars().all(font::has));
    }

    /// The setup screen has a step button either side of every setting,
    /// a preview, START and BACK, and says what a Hardest opponent gets.
    ///
    /// REQ: GD-AI-01
    #[test]
    fn the_setup_screen_declares_the_hardest_bonus_beside_the_opponent() {
        let input = input();
        let atlas = Atlas::placeholder();
        let mut s = Setup::new(1);
        let plain = setup(&atlas, &input, &s, None);
        for field in [Field::Size, Field::Opponents, Field::PopCap, Field::Seed] {
            assert!(find(&plain, ShellAction::Adjust(field, -1)).enabled);
            assert!(find(&plain, ShellAction::Adjust(field, 1)).enabled);
        }
        assert!(
            !find(&plain, ShellAction::Adjust(Field::Map, 1)).enabled,
            "one map kind in the slice"
        );
        assert!(find(&plain, ShellAction::Start).enabled);
        assert!(find(&plain, ShellAction::Back).enabled);
        assert!(find(&plain, ShellAction::Shuffle).enabled);
        assert!(find(&plain, ShellAction::Adjust(Field::Difficulty(0), 1)).enabled);
        assert!(plain.preview.is_some(), "a preview at 1280 wide");
        assert!(inside(&plain, &input));

        s.opponents = vec![Difficulty::Standard, Difficulty::Hardest];
        let declared = setup(&atlas, &input, &s, None);
        assert!(
            declared.sprites.len() > plain.sprites.len() + 12,
            "the bonus line is drawn beside the Hardest opponent"
        );
        assert!(find(&declared, ShellAction::Adjust(Field::Difficulty(1), -1)).enabled);

        // A refused setup greys START and shows the reason.
        let refused = setup(&atlas, &input, &s, Some("pop cap 500 is outside 50..=200"));
        assert!(!find(&refused, ShellAction::Start).enabled);
        assert!(refused.sprites.len() > declared.sprites.len());

        // A narrow window keeps every button but drops the preview.
        let narrow = ShellInput {
            viewport: (640.0, 480.0),
            ..input
        };
        let small = setup(&atlas, &narrow, &s, None);
        assert!(small.preview.is_none());
        assert!(inside(&small, &narrow));
        assert_eq!(small.buttons.len(), declared.buttons.len());
    }

    /// The pause menu and the results scale with the HUD, and the results
    /// list every side.
    #[test]
    fn the_overlays_scale_with_the_hud_and_the_results_list_every_side() {
        let atlas = Atlas::placeholder();
        let one = pause_menu(&atlas, &input(), false, None, None, false);
        let two = pause_menu(
            &atlas,
            &ShellInput {
                ui_scale: 2.0,
                ..input()
            },
            false,
            None,
            None,
            false,
        );
        let (a, b) = (
            find(&one, ShellAction::Resume),
            find(&two, ShellAction::Resume),
        );
        assert_eq!(b.w, a.w * 2.0);
        assert_eq!(b.h, a.h * 2.0);
        assert!(find(&one, ShellAction::Resign).enabled);
        let decided = pause_menu(
            &atlas,
            &input(),
            true,
            Some(ShellAction::QuitToTitle),
            Some("SAVED 20260919-190512-SEED3-TICK4321-P2"),
            false,
        );
        assert!(find(&decided, ShellAction::Save).enabled);
        let watching = pause_menu(&atlas, &input(), false, None, None, true);
        assert!(!find(&watching, ShellAction::Save).enabled);
        assert!(!find(&watching, ShellAction::Resign).enabled);
        assert!(find(&watching, ShellAction::QuitToTitle).enabled);
        assert!(
            decided.sprites.len() > one.sprites.len() + 20,
            "the save note is drawn"
        );
        assert!(!find(&decided, ShellAction::Resign).enabled);
        assert_eq!(
            find(&decided, ShellAction::QuitToTitle).label,
            "CONFIRM QUIT"
        );

        let r = Results {
            heading: "DEFEAT".into(),
            why: "YOU RESIGNED".into(),
            sides: vec![
                Side {
                    player: 0,
                    name: "YOU".into(),
                    score: 1200,
                    standing: false,
                },
                Side {
                    player: 1,
                    name: "HARD".into(),
                    score: 3400,
                    standing: true,
                },
            ],
        };
        let screen = results(&atlas, &input(), &r);
        assert!(find(&screen, ShellAction::KeepWatching).enabled);
        assert!(find(&screen, ShellAction::QuitToTitle).enabled);
        let more = results(
            &atlas,
            &input(),
            &Results {
                sides: [r.sides.clone(), r.sides.clone()].concat(),
                ..r
            },
        );
        assert!(more.sprites.len() > screen.sprites.len());
        assert!(inside(&more, &input()));
    }

    /// The settings screen has a step either side of the three settings,
    /// a CHANGE button per control showing its key, the control being
    /// rebound saying so with its button greyed, an error line, and BACK
    /// and DEFAULTS.
    ///
    /// REQ: GD-A11Y-02
    #[test]
    fn the_settings_screen_lists_every_control_with_its_key() {
        let atlas = Atlas::placeholder();
        let mut settings = Settings::default();
        let plain = settings_screen(&atlas, &input(), &settings, SettingsPage::Keys, None, None);
        assert!(find(&plain, ShellAction::SettingScale(-1)).enabled);
        assert!(find(&plain, ShellAction::SettingScale(1)).enabled);
        assert!(find(&plain, ShellAction::ToggleEdgeScroll).enabled);
        assert!(find(&plain, ShellAction::ToggleFullscreen).enabled);
        for b in Bus::ALL {
            assert!(find(&plain, ShellAction::Volume(b, -1)).enabled, "{b:?}");
            assert!(find(&plain, ShellAction::Volume(b, 1)).enabled, "{b:?}");
        }
        assert!(find(&plain, ShellAction::ToggleHints).enabled);
        for c in Control::ALL {
            assert!(find(&plain, ShellAction::Rebind(c)).enabled, "{c:?}");
        }
        assert!(find(&plain, ShellAction::Back).enabled);
        assert!(find(&plain, ShellAction::ResetSettings).enabled);
        assert!(inside(&plain, &input()));
        assert!(inside(
            &settings_screen(
                &atlas,
                &ShellInput {
                    viewport: (960.0, 540.0),
                    ..input()
                },
                &settings,
                SettingsPage::Keys,
                None,
                None
            ),
            &ShellInput {
                viewport: (960.0, 540.0),
                ..input()
            }
        ));
        let waiting = settings_screen(
            &atlas,
            &input(),
            &settings,
            SettingsPage::Keys,
            Some(Capture::Control(Control::Pause)),
            None,
        );
        assert!(!find(&waiting, ShellAction::Rebind(Control::Pause)).enabled);
        assert!(find(&waiting, ShellAction::Rebind(Control::Faster)).enabled);
        settings.bind(Control::Pause, "F6").unwrap();
        let refused = settings_screen(
            &atlas,
            &input(),
            &settings,
            SettingsPage::Keys,
            None,
            Some("H IS A COMMAND KEY ON THE PANELS"),
        );
        assert!(refused.sprites.len() > plain.sprites.len() + 20);
        // The second page: a CHANGE per panel letter, the one waiting
        // greyed, and the way back to the first.
        assert!(find(&plain, ShellAction::SettingsPage(SettingsPage::Letters)).enabled);
        let letters = settings_screen(
            &atlas,
            &input(),
            &settings,
            SettingsPage::Letters,
            Some(Capture::Letter('H')),
            None,
        );
        for l in crate::hud::command_letters() {
            assert_eq!(
                find(&letters, ShellAction::RebindLetter(l)).enabled,
                l != 'H',
                "{l}"
            );
        }
        assert!(find(&letters, ShellAction::SettingsPage(SettingsPage::Keys)).enabled);
        assert!(find(&letters, ShellAction::Back).enabled);
        assert!(inside(&letters, &input()));
    }

    /// The load screen lists a button per save, newest first as given,
    /// says so when there are none, shows what went wrong with a load,
    /// and never lists more than it has room for.
    #[test]
    fn the_load_screen_lists_the_saves_and_says_what_went_wrong() {
        let atlas = Atlas::placeholder();
        let none = load_screen(&atlas, &input(), &[], None, false);
        assert_eq!(none.buttons.len(), 1, "BACK alone");
        assert!(find(&none, ShellAction::Back).enabled);
        let rows: Vec<LoadRow> = (0..14)
            .map(|i| LoadRow {
                title: format!("SEED {i} AT 1:0{}", i % 10),
                detail: "2026-09-19 19:05 UTC - 2 PLAYERS".into(),
            })
            .collect();
        let full = load_screen(&atlas, &input(), &rows, None, false);
        assert_eq!(full.buttons.len(), LOAD_ROWS + 1);
        let replays = load_screen(&atlas, &input(), &rows[..3], None, true);
        assert_eq!(replays.buttons.len(), 4);
        assert!(find(&replays, ShellAction::Watch(2)).enabled);
        assert!(replays
            .buttons
            .iter()
            .all(|b| !matches!(b.action, ShellAction::Load(_))));
        assert_eq!(find(&full, ShellAction::Load(0)).label, "SEED 0 AT 1:00");
        assert!(find(&full, ShellAction::Load(LOAD_ROWS - 1)).enabled);
        assert!(full
            .buttons
            .iter()
            .all(|b| b.action != ShellAction::Load(LOAD_ROWS)));
        assert!(inside(&full, &input()));
        let two = load_screen(&atlas, &input(), &rows[..2], None, false);
        let failed = load_screen(
            &atlas,
            &input(),
            &rows[..2],
            Some("simulation state version 9 but this build reads version 1"),
            false,
        );
        assert!(failed.sprites.len() > two.sprites.len() + 20);
        assert_eq!(failed.buttons.len(), 3);
    }
}
