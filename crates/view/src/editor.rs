//! The scenario editor's screens (`docs/02` §13, `GD-CAMP-06`): the
//! strip of tools along the top, the palette along the bottom beside the
//! minimap, and, for the scenario's sides, objectives and triggers, a
//! list down the left whose every field is a chip to click. Like the rest
//! of the shell, this only draws and says where the buttons are; what a
//! chip means is the app's (`app::editor`).

use crate::font;
use crate::hud::{fit, BOTTOM_PANEL, MINIMAP_RESERVE};
use crate::palette::*;
use crate::shell::{LoadRow, Screen, Sheet, ShellAction, ShellInput};
use crate::sprites::{Atlas, Ink};

/// What the editor is doing to the scenario.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tool {
    /// Painting the ground.
    #[default]
    Terrain,
    /// Raising and lowering it.
    Height,
    /// Setting units, buildings and nature down, or taking them up.
    Units,
    /// The title, the briefing and the sides.
    Scenario,
    /// The player's objectives.
    Objectives,
    /// The triggers.
    Triggers,
    /// What is wrong with it.
    Check,
}

impl Tool {
    /// In the order of the strip.
    pub const ALL: [Tool; 7] = [
        Tool::Terrain,
        Tool::Height,
        Tool::Units,
        Tool::Scenario,
        Tool::Objectives,
        Tool::Triggers,
        Tool::Check,
    ];

    /// Its tab.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Terrain => "TERRAIN",
            Tool::Height => "HEIGHT",
            Tool::Units => "UNITS",
            Tool::Scenario => "SCENARIO",
            Tool::Objectives => "OBJECTIVES",
            Tool::Triggers => "TRIGGERS",
            Tool::Check => "CHECK",
        }
    }

    /// Whether it shows the list down the left rather than working on the
    /// map.
    pub fn lists(self) -> bool {
        matches!(
            self,
            Tool::Scenario | Tool::Objectives | Tool::Triggers | Tool::Check
        )
    }
}

/// What a click on the map does with the units tool.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum UnitMode {
    /// Sets the chosen kind down.
    #[default]
    Place,
    /// Takes up what is there.
    Erase,
    /// Names what is there, or stops naming it, for a trigger to wait on
    /// its loss.
    Tag,
}

/// A click in the editor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorAction {
    /// Change tools.
    Tool(Tool),
    /// Paint with this map letter.
    Letter(char),
    /// Paint so many tiles across.
    Brush(i32),
    /// The height tool raises, or lowers.
    Raise(bool),
    /// Show this page of the units palette.
    Page(u8),
    /// Set this kind down.
    Kind(u16),
    /// For this side, or Gaia.
    Owner(u8),
    /// Place, erase or tag.
    Mode(UnitMode),
    /// A field of the list: row and chip. A left click steps it on, a
    /// right click back.
    Chip(u16, u8),
    /// The list, so many rows on.
    Scroll(i32),
    /// A new blank map so many tiles across.
    New(u16),
    /// Open the scenario on this row of the start screen.
    Open(usize),
    /// Write the scenario out.
    Save,
    /// Play it from where it stands.
    Playtest,
    /// Leave the editor.
    Exit,
}

/// A palette button.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Item {
    /// What it says.
    pub label: String,
    /// What it does.
    pub action: EditorAction,
    /// The one chosen.
    pub active: bool,
}

/// A field in a row of the list: a button, or plain words.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Chip {
    /// What it says.
    pub label: String,
    /// What a click does; none for words.
    pub action: Option<EditorAction>,
    /// Highlighted: the row's heading, or a field awaiting the map.
    pub active: bool,
}

impl Chip {
    /// Plain words.
    pub fn words(label: impl Into<String>) -> Chip {
        Chip {
            label: label.into(),
            action: None,
            active: false,
        }
    }
}

/// Everything the editor's screen shows.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Panel {
    /// The tool in hand.
    pub tool: Tool,
    /// The file's name, or UNTITLED.
    pub file: String,
    /// Changed since it was saved.
    pub dirty: bool,
    /// What the check says, or what just happened.
    pub status: String,
    /// The palette, a row of buttons a line.
    pub palette: Vec<Vec<Item>>,
    /// The list, a row of chips a line, for the tools that list.
    pub rows: Vec<Vec<Chip>>,
    /// The first row of the list shown.
    pub top: usize,
    /// A line being typed: what it is for, and what it says so far.
    pub typing: Option<(String, String)>,
    /// What the map is to be clicked for, while a field waits on it.
    pub prompt: Option<String>,
    /// EXIT has been clicked once on unsaved changes.
    pub confirm_exit: bool,
}

/// The strip of tabs, and the status line under it.
pub const TOP_H: f32 = 42.0;
/// A row of the list.
pub const ROW_H: f32 = 18.0;
/// A row of the palette.
const ITEM_H: f32 = 20.0;
/// The widest the list gets.
const LIST_W: f32 = 700.0;

/// The list's rectangle, in HUD px.
pub fn list_rect(vw: f32, vh: f32) -> (f32, f32, f32, f32) {
    let w = (vw * 0.62).min(LIST_W).floor();
    (0.0, TOP_H, w, (vh - TOP_H - BOTTOM_PANEL).max(ROW_H))
}

/// Whether a HUD-px point is on the editor's panels rather than the map.
pub fn over_panel(tool: Tool, vw: f32, vh: f32, x: f32, y: f32) -> bool {
    if y < TOP_H || y >= vh - BOTTOM_PANEL {
        return true;
    }
    let (lx, ly, lw, lh) = list_rect(vw, vh);
    tool.lists() && x >= lx && x < lx + lw && y >= ly && y < ly + lh
}

/// How many rows of the list fit, on the window size given (HUD px).
pub fn rows_shown(vh: f32) -> usize {
    let (_, _, _, h) = list_rect(1.0, vh);
    (((h - 30.0) / ROW_H).floor() as usize).max(1)
}

/// The editor over the world: tabs, status, palette and list.
pub fn editor_screen(atlas: &Atlas, input: &ShellInput, panel: &Panel) -> Screen {
    let mut s = Sheet::new(atlas, input);
    let (vw, vh) = (s.vw, s.vh);
    // The strip.
    s.p.rect(0.0, 0.0, vw, TOP_H, BLACK, 0);
    s.p.rect(0.0, 0.0, vw, TOP_H - 2.0, BROWN_DARK, 0);
    let mut x = 4.0;
    for tool in Tool::ALL {
        let w = font::width(tool.label()) as f32 + 16.0;
        s.button(
            (x, 3.0, w, 18.0),
            ShellAction::Edit(EditorAction::Tool(tool)),
            tool.label(),
            true,
            "",
        );
        if tool == panel.tool {
            s.p.rect(x + 1.0, 20.0, w - 2.0, 2.0, GOLD, 0);
        }
        x += w + 4.0;
    }
    let exit = if panel.confirm_exit {
        "CONFIRM EXIT"
    } else {
        "EXIT"
    };
    let mut rx = vw - 4.0;
    for (action, label) in [
        (EditorAction::Exit, exit),
        (EditorAction::Playtest, "PLAYTEST"),
        (EditorAction::Save, "SAVE"),
    ] {
        let w = font::width(label) as f32 + 16.0;
        rx -= w;
        if rx > x {
            s.button(
                (rx, 3.0, w, 18.0),
                ShellAction::Edit(action),
                label,
                true,
                "",
            );
        }
        rx -= 4.0;
    }
    let file = format!(
        "{}{}",
        panel.file.to_uppercase(),
        if panel.dirty { " *" } else { "" }
    );
    let fw =
        s.p.text_in(6.0, 27.0, &fit(&file, vw * 0.3), Ink::Gold, 1.0);
    if let Some(prompt) = &panel.prompt {
        let text = fit(&prompt.to_uppercase(), vw - fw - 30.0);
        s.p.text_in(fw + 20.0, 27.0, &text, Ink::Gold, 1.0);
    } else {
        let text = fit(&panel.status.to_uppercase(), vw - fw - 30.0);
        s.p.text(fw + 20.0, 27.0, &text, false, 1.0);
    }

    // The palette, beside the minimap.
    let py = vh - BOTTOM_PANEL;
    let pw = (vw - MINIMAP_RESERVE).max(200.0);
    s.p.rect(0.0, py, vw, BOTTOM_PANEL, BLACK, 0);
    s.p.rect(0.0, py + 2.0, vw, BOTTOM_PANEL - 2.0, BROWN_DARK, 0);
    let mut iy = py + 8.0;
    for row in &panel.palette {
        let mut ix = 8.0;
        for item in row {
            let w = (font::width(&item.label) as f32 + 12.0).max(36.0);
            if ix + w > pw && ix > 8.0 {
                ix = 8.0;
                iy += ITEM_H + 4.0;
            }
            if iy + ITEM_H > vh - 4.0 {
                break;
            }
            s.button(
                (ix, iy, w, ITEM_H),
                ShellAction::Edit(item.action),
                &item.label,
                true,
                "",
            );
            if item.active {
                s.p.outline(ix - 1.0, iy - 1.0, w + 2.0, ITEM_H + 2.0, GOLD);
            }
            ix += w + 4.0;
        }
        iy += ITEM_H + 6.0;
    }

    // The list.
    if panel.tool.lists() {
        let (lx, ly, lw, lh) = list_rect(vw, vh);
        s.panel(lx, ly, lw, lh);
        let shown = rows_shown(vh);
        let mut y = ly + 8.0;
        let bottom = ly + lh - 22.0;
        for (r, row) in panel.rows.iter().enumerate().skip(panel.top) {
            if y + ROW_H > bottom {
                break;
            }
            let mut cx = lx + 10.0;
            for chip in row {
                let w = (font::width(&chip.label) as f32 + 10.0).min(lw - 48.0);
                if cx + w > lx + lw - 10.0 && cx > lx + 30.0 {
                    cx = lx + 30.0;
                    y += ROW_H;
                    if y + ROW_H > bottom {
                        break;
                    }
                }
                match chip.action {
                    Some(action) => {
                        s.button(
                            (cx, y, w, ROW_H - 2.0),
                            ShellAction::Edit(action),
                            &chip.label,
                            true,
                            "",
                        );
                        if chip.active {
                            s.p.outline(cx - 1.0, y - 1.0, w + 2.0, ROW_H, GOLD);
                        }
                    }
                    None => {
                        let ink = if chip.active { Ink::Gold } else { Ink::White };
                        s.p.text_in(cx, y + 4.0, &fit(&chip.label, w), ink, 1.0);
                    }
                }
                cx += w + 4.0;
            }
            let _ = r;
            y += ROW_H;
        }
        // Scrolling, when there is more than fits.
        if panel.rows.len() > shown {
            let by = ly + lh - 20.0;
            s.button(
                (lx + lw - 112.0, by, 50.0, 16.0),
                ShellAction::Edit(EditorAction::Scroll(-(shown as i32 / 2).max(1))),
                "UP",
                panel.top > 0,
                "",
            );
            s.button(
                (lx + lw - 58.0, by, 50.0, 16.0),
                ShellAction::Edit(EditorAction::Scroll((shown as i32 / 2).max(1))),
                "DOWN",
                panel.top + shown < panel.rows.len(),
                "",
            );
        }
    }

    // A line being typed, over the palette.
    if let Some((what, line)) = &panel.typing {
        let h = 34.0;
        let y = py - h - 6.0;
        let w = (vw - 40.0).min(900.0);
        let x = ((vw - w) / 2.0).round();
        s.panel(x, y, w, h);
        let head = format!(
            "{}: ENTER KEEPS IT, ESC LEAVES IT AS IT WAS",
            what.to_uppercase()
        );
        s.p.text_in(x + 10.0, y + 7.0, &fit(&head, w - 20.0), Ink::Gold, 1.0);
        let text = format!("{}_", line.to_uppercase());
        // The end of a long line is what is being typed: show that.
        let max = w - 20.0;
        let mut shown = text.as_str();
        while font::width(shown) as f32 > max && shown.len() > 1 {
            shown = &shown[1..];
        }
        s.p.text(x + 10.0, y + 19.0, shown, false, 1.0);
    }
    s.finish(None)
}

/// The editor's front door: a new blank map of a size, or a scenario to
/// open, the player's own and the shipped campaigns' (opened as a copy).
pub fn editor_start(
    atlas: &Atlas,
    input: &ShellInput,
    rows: &[LoadRow],
    error: Option<&str>,
) -> Screen {
    let mut s = Sheet::new(atlas, input);
    s.backdrop();
    let pw = (s.vw - 40.0).min(640.0);
    let shown = rows.len().min(14);
    let ph = 150.0 + shown.max(1) as f32 * 24.0 + 50.0;
    let x = ((s.vw - pw) / 2.0).round();
    let y = ((s.vh - ph) / 2.0).max(4.0).round();
    s.panel(x, y, pw, ph);
    let cx = x + pw / 2.0;
    s.centred(cx, y + 12.0, "SCENARIO EDITOR", Ink::Gold, 2.0);
    s.p.text_in(x + 24.0, y + 44.0, "A NEW MAP", Ink::Gold, 1.0);
    let sizes = [(48, "SMALL"), (64, "MEDIUM"), (96, "LARGE"), (128, "HUGE")];
    let bw = ((pw - 48.0 - 3.0 * 8.0) / 4.0).floor();
    for (i, (size, name)) in sizes.iter().enumerate() {
        s.button(
            (x + 24.0 + i as f32 * (bw + 8.0), y + 58.0, bw, 24.0),
            ShellAction::Edit(EditorAction::New(*size)),
            &format!("{name} {size}"),
            true,
            "",
        );
    }
    s.p.text_in(x + 24.0, y + 98.0, "OPEN A SCENARIO", Ink::Gold, 1.0);
    let mut ry = y + 112.0;
    if rows.is_empty() {
        s.p.text(x + 24.0, ry + 4.0, "NONE YET", false, 1.0);
        ry += 24.0;
    }
    for (i, row) in rows.iter().take(shown).enumerate() {
        let bw = (pw * 0.5).floor();
        s.button(
            (x + 24.0, ry, bw, 20.0),
            ShellAction::Edit(EditorAction::Open(i)),
            &row.title,
            true,
            "",
        );
        s.p.text(
            x + 32.0 + bw,
            ry + 6.0,
            &fit(&row.detail, pw - bw - 56.0),
            false,
            1.0,
        );
        ry += 24.0;
    }
    if let Some(e) = error {
        s.centred(cx, y + ph - 58.0, &fit(e, pw - 16.0), Ink::Gold, 1.0);
    }
    s.button(
        ((cx - 100.0).round(), y + ph - 40.0, 200.0, 28.0),
        ShellAction::Back,
        "BACK",
        true,
        "",
    );
    s.finish(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(w: f32, h: f32) -> ShellInput {
        ShellInput {
            viewport: (w, h),
            ui_scale: 1.0,
            hover: None,
        }
    }

    fn panel() -> Panel {
        Panel {
            tool: Tool::Triggers,
            file: "nile.ron".into(),
            dirty: true,
            status: "2 problems: trigger 1: no objective \"x\"".into(),
            palette: vec![vec![Item {
                label: "ADD A TRIGGER".into(),
                action: EditorAction::Chip(0, 0),
                active: false,
            }]],
            rows: (0..60)
                .map(|r| {
                    vec![
                        Chip::words(format!("WHEN {r}")),
                        Chip {
                            label: "HAS SIDE 2 CLUBMAN AT LEAST 5".into(),
                            action: Some(EditorAction::Chip(r, 1)),
                            active: false,
                        },
                        Chip {
                            label: "X".into(),
                            action: Some(EditorAction::Chip(r, 2)),
                            active: false,
                        },
                    ]
                })
                .collect(),
            top: 0,
            typing: None,
            prompt: None,
            confirm_exit: false,
        }
    }

    /// The editor's screen keeps to the window at the smallest size the
    /// game allows, every tab and file button is there, the list scrolls,
    /// and the map is free where the panels are not.
    ///
    /// REQ: GD-CAMP-06
    #[test]
    fn the_editor_screen_fits_and_its_list_scrolls() {
        for (w, h) in [(1024.0, 640.0), (1280.0, 720.0), (1920.0, 1080.0)] {
            let input = input(w, h);
            let screen = editor_screen(&Atlas::placeholder(), &input, &panel());
            for tool in Tool::ALL {
                assert!(screen
                    .buttons
                    .iter()
                    .any(|b| b.action == ShellAction::Edit(EditorAction::Tool(tool))));
            }
            for a in [
                EditorAction::Save,
                EditorAction::Playtest,
                EditorAction::Exit,
            ] {
                assert!(screen
                    .buttons
                    .iter()
                    .any(|b| b.action == ShellAction::Edit(a)));
            }
            assert!(screen
                .buttons
                .iter()
                .all(|b| b.x >= 0.0 && b.y >= 0.0 && b.x + b.w <= w && b.y + b.h <= h));
            let down = screen
                .buttons
                .iter()
                .find(|b| matches!(b.action, ShellAction::Edit(EditorAction::Scroll(n)) if n > 0))
                .expect("a list longer than fits scrolls");
            assert!(down.enabled);
            assert!(!over_panel(Tool::Triggers, w, h, w - 300.0, h / 2.0));
            assert!(over_panel(Tool::Triggers, w, h, 20.0, h / 2.0));
            assert!(!over_panel(Tool::Terrain, w, h, 20.0, h / 2.0));
        }
        let mut p = panel();
        p.typing = Some(("trigger 1 says".into(), "THE NILE ".repeat(30)));
        let screen = editor_screen(&Atlas::placeholder(), &input(1280.0, 720.0), &p);
        assert!(!screen.sprites.is_empty());
    }

    /// REQ: GD-CAMP-06
    #[test]
    fn the_start_screen_offers_new_maps_and_the_scenarios_to_open() {
        let rows: Vec<LoadRow> = (0..3)
            .map(|i| LoadRow {
                title: format!("SCENARIO {i}"),
                detail: "YOURS".into(),
            })
            .collect();
        let screen = editor_start(&Atlas::placeholder(), &input(1024.0, 640.0), &rows, None);
        for size in [48, 64, 96, 128] {
            assert!(screen
                .buttons
                .iter()
                .any(|b| b.action == ShellAction::Edit(EditorAction::New(size))));
        }
        assert!(screen
            .buttons
            .iter()
            .any(|b| b.action == ShellAction::Edit(EditorAction::Open(2))));
        assert!(screen.buttons.iter().any(|b| b.action == ShellAction::Back));
    }
}
