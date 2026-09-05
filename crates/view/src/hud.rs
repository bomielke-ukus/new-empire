//! The heads-up display: resource bar, selection panel, command buttons,
//! health bars. Produces screen-space sprites and the buttons' hit rectangles;
//! the app decides what a click means.

use sim::entity::KindId;
use sim::kinds::{self, Resource};
use sim::{GatherPhase, Order, Simulation};

use crate::camera::Camera;
use crate::font;
use crate::fx_to_f32;
use crate::iso;
use crate::palette::*;
use crate::scene::SpriteInstance;
use crate::sprites::{Atlas, Frame};

/// Height of the top resource bar.
pub const TOP_BAR: f32 = 26.0;
/// Height of the bottom panel.
pub const BOTTOM_PANEL: f32 = 112.0;
/// Width reserved on the right of the bottom panel for the minimap.
pub const MINIMAP_RESERVE: f32 = 300.0;

/// What a button does when clicked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Enter placement mode for a building.
    Build(KindId),
    /// Queue a villager at the selected building.
    Train,
    /// Stop the selected units.
    Stop,
    /// Leave placement mode.
    Cancel,
    /// Remove the last queued item.
    CancelTrain,
}

/// A clickable region.
#[derive(Clone, PartialEq, Debug)]
pub struct Button {
    /// Window px.
    pub x: f32,
    /// Window px.
    pub y: f32,
    /// Size.
    pub w: f32,
    /// Size.
    pub h: f32,
    /// What it does.
    pub action: Action,
    /// Label text.
    pub label: String,
    /// Hotkey shown on the button.
    pub hotkey: char,
}

impl Button {
    /// True if a window point is inside.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Everything the HUD needs to draw a frame.
pub struct HudInput<'a> {
    /// The match.
    pub sim: &'a Simulation,
    /// Whose HUD.
    pub player: u8,
    /// For projecting health bars.
    pub camera: &'a Camera,
    /// Selected entity slots.
    pub selected: &'a [u32],
    /// Building being placed, if any.
    pub build_mode: Option<KindId>,
    /// Frames per second, for the corner readout.
    pub fps: f32,
    /// Whether the clock is paused.
    pub paused: bool,
    /// Game speed multiplier.
    pub speed: f32,
    /// Window title-style status; shown top right.
    pub status: &'a str,
}

/// A built HUD.
#[derive(Clone, Default, Debug)]
pub struct Hud {
    /// Screen-space sprites in draw order.
    pub sprites: Vec<SpriteInstance>,
    /// Clickable buttons.
    pub buttons: Vec<Button>,
}

/// Draws screen-space primitives into a sprite list.
pub struct Painter<'a> {
    atlas: &'a Atlas,
    /// Output.
    pub out: Vec<SpriteInstance>,
}

impl<'a> Painter<'a> {
    /// A painter over an atlas.
    pub fn new(atlas: &'a Atlas) -> Painter<'a> {
        Painter {
            atlas,
            out: Vec::new(),
        }
    }

    fn push(&mut self, f: &Frame, x: f32, y: f32, w: f32, h: f32, row: u8) {
        self.out.push(SpriteInstance {
            x: x.round(),
            y: y.round(),
            w,
            h,
            u: f.x,
            v: f.y,
            uw: f.w,
            vh: f.h,
            row,
            flip: false,
            depth: 0.0,
            slot: u32::MAX,
            screen: true,
        });
    }

    /// A filled rectangle in a palette colour (player row 0 unless the
    /// colour is a player ramp index, in which case `row` applies).
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, idx: u8, row: u8) {
        let f = *self.atlas.solid(idx);
        self.push(&f, x, y, w, h, row);
    }

    /// A rectangle outline one pixel wide.
    pub fn outline(&mut self, x: f32, y: f32, w: f32, h: f32, idx: u8) {
        self.rect(x, y, w, 1.0, idx, 0);
        self.rect(x, y + h - 1.0, w, 1.0, idx, 0);
        self.rect(x, y, 1.0, h, idx, 0);
        self.rect(x + w - 1.0, y, 1.0, h, idx, 0);
    }

    /// Text at a pixel position, at an integer scale. Returns the width drawn.
    pub fn text(&mut self, x: f32, y: f32, text: &str, dark: bool, scale: f32) -> f32 {
        let mut cx = x;
        for ch in text.chars() {
            if let Some(g) = self.atlas.glyph(ch, dark) {
                let g = *g;
                if ch != ' ' {
                    self.push(&g, cx, y, g.w as f32 * scale, g.h as f32 * scale, 0);
                }
            }
            cx += font::ADVANCE as f32 * scale;
        }
        cx - x
    }

    /// A labelled button.
    pub fn button(&mut self, b: &Button, hover: bool) {
        let fill = if hover { TAN } else { BROWN };
        self.rect(b.x, b.y, b.w, b.h, BROWN_DARK, 0);
        self.rect(b.x + 1.0, b.y + 1.0, b.w - 2.0, b.h - 2.0, fill, 0);
        self.outline(b.x, b.y, b.w, b.h, BLACK);
        self.text(b.x + 6.0, b.y + 6.0, &b.label, true, 1.0);
        let key = format!("({})", b.hotkey);
        let kw = font::width(&key);
        self.text(
            b.x + b.w - kw as f32 - 4.0,
            b.y + b.h - 11.0,
            &key,
            true,
            1.0,
        );
    }
}

impl Hud {
    /// Builds the HUD for a frame.
    pub fn build(atlas: &Atlas, input: &HudInput<'_>) -> Hud {
        let (vw, vh) = input.camera.viewport;
        let mut p = Painter::new(atlas);
        let mut buttons = Vec::new();
        let sim = input.sim;
        let world = sim.world();

        // ----- top bar: resources, population, idle villagers, status
        p.rect(0.0, 0.0, vw, TOP_BAR, BROWN_DARK, 0);
        p.rect(0.0, TOP_BAR - 2.0, vw, 2.0, BLACK, 0);
        let mut x = 10.0;
        if let Some(pl) = sim.player(input.player) {
            for r in Resource::ALL {
                let workers = world
                    .slots()
                    .filter(|s| {
                        world.owner[s.index()] == input.player
                            && matches!(world.order[s.index()], Order::Gather { resource, .. } if resource == r)
                    })
                    .count();
                let label = format!("{} {}", r.name().to_uppercase(), pl.stockpile[r.index()]);
                let w = p.text(x, 6.0, &label, false, 2.0);
                p.text(x + w + 4.0, 12.0, &format!("({workers})"), false, 1.0);
                x += w + 40.0;
            }
            let housed = pl.pop >= pl.pop_cap;
            let pop = format!("POP {}/{}", pl.pop, pl.pop_cap);
            if housed {
                p.rect(
                    x - 4.0,
                    3.0,
                    font::width(&pop) as f32 * 2.0 + 8.0,
                    20.0,
                    RED_DARK,
                    0,
                );
            }
            let w = p.text(x, 6.0, &pop, false, 2.0);
            x += w + 40.0;
            let idle = sim.idle_villagers(input.player).len();
            if idle > 0 {
                let label = format!("IDLE {idle}");
                let bw = font::width(&label) as f32 * 2.0 + 8.0;
                p.rect(
                    x - 4.0,
                    3.0,
                    bw,
                    20.0,
                    if idle > 3 { RED } else { GOLD_DARK },
                    0,
                );
                p.text(x, 6.0, &label, false, 2.0);
            }
        }
        let status = format!(
            "{}{} {:.0} FPS {:.1}X",
            input.status,
            if input.paused { " PAUSED" } else { "" },
            input.fps,
            input.speed
        );
        let sw = font::width(&status) as f32;
        p.text(vw - sw - 10.0, 10.0, &status, false, 1.0);

        // ----- bottom panel
        let py = vh - BOTTOM_PANEL;
        p.rect(0.0, py, vw, BOTTOM_PANEL, BROWN_DARK, 0);
        p.rect(0.0, py, vw, 2.0, BLACK, 0);
        let panel_w = vw - MINIMAP_RESERVE;
        let left_w = 260.0_f32.min(panel_w * 0.4);
        p.rect(left_w, py + 8.0, 2.0, BOTTOM_PANEL - 16.0, BLACK, 0);

        // Selection summary.
        let selected: Vec<usize> = input
            .selected
            .iter()
            .map(|&s| s as usize)
            .filter(|&i| i < world.capacity() && world.slots().any(|s| s.index() == i))
            .collect();
        let mut ty = py + 10.0;
        match selected.len() {
            0 => {
                p.text(10.0, ty, "NOTHING SELECTED", false, 1.0);
                ty += 12.0;
                p.text(10.0, ty, "CLICK OR DRAG TO SELECT", false, 1.0);
            }
            1 => {
                let i = selected[0];
                let info = kinds::info(world.kind[i]);
                p.text(10.0, ty, &info.name.to_uppercase(), false, 2.0);
                ty += 20.0;
                let hp = fx_to_f32(world.health[i]);
                p.text(
                    10.0,
                    ty,
                    &format!("HP {:.0}/{}", hp, info.max_health),
                    false,
                    1.0,
                );
                // Health bar.
                let bw = left_w - 20.0;
                p.rect(10.0, ty + 10.0, bw, 6.0, BLACK, 0);
                let frac = (hp / info.max_health as f32).clamp(0.0, 1.0);
                p.rect(
                    11.0,
                    ty + 11.0,
                    (bw - 2.0) * frac,
                    4.0,
                    if frac > 0.5 { GREEN_LIGHT } else { RED },
                    0,
                );
                ty += 22.0;
                if let Some(done) = world.construction[i] {
                    let pct = done * 100 / info.build_ticks().max(1);
                    p.text(10.0, ty, &format!("BUILDING {pct}%"), false, 1.0);
                    ty += 12.0;
                }
                if let Some((r, n)) = world.carry[i] {
                    p.text(
                        10.0,
                        ty,
                        &format!("CARRYING {} {}", n, r.name().to_uppercase()),
                        false,
                        1.0,
                    );
                    ty += 12.0;
                }
                let job = match world.order[i] {
                    Order::Idle if info.mobile => "IDLE",
                    Order::Idle => "",
                    Order::Move { .. } => "MOVING",
                    Order::Gather {
                        phase: GatherPhase::Working,
                        ..
                    } => "GATHERING",
                    Order::Gather {
                        phase: GatherPhase::ToDropoff { .. },
                        ..
                    } => "RETURNING",
                    Order::Gather { .. } => "GOING TO GATHER",
                    Order::Build { working: true, .. } => "BUILDING",
                    Order::Build { .. } => "GOING TO BUILD",
                };
                if !job.is_empty() {
                    p.text(10.0, ty, job, false, 1.0);
                    ty += 12.0;
                }
                if let Some(q) = world.production[i].as_ref() {
                    if let Some(head) = q.queue.first() {
                        let pct = head.progress * 100 / kinds::info(head.kind).build_ticks().max(1);
                        p.text(
                            10.0,
                            ty,
                            &format!(
                                "TRAINING {} {}% ({} QUEUED)",
                                kinds::info(head.kind).name.to_uppercase(),
                                pct,
                                q.queue.len()
                            ),
                            false,
                            1.0,
                        );
                    }
                }
            }
            n => {
                let villagers = selected
                    .iter()
                    .filter(|&&i| world.kind[i] == kinds::VILLAGER)
                    .count();
                p.text(10.0, ty, &format!("{n} SELECTED"), false, 2.0);
                ty += 20.0;
                if villagers > 0 {
                    p.text(10.0, ty, &format!("{villagers} VILLAGERS"), false, 1.0);
                }
            }
        }

        // Command buttons.
        let any_villager = selected
            .iter()
            .any(|&i| world.kind[i] == kinds::VILLAGER && world.owner[i] == input.player);
        let trainer = selected
            .iter()
            .find(|&&i| {
                kinds::info(world.kind[i]).trains
                    && world.owner[i] == input.player
                    && world.construction[i].is_none()
            })
            .copied();
        let any_mobile = selected
            .iter()
            .any(|&i| kinds::info(world.kind[i]).mobile && world.owner[i] == input.player);
        let mut defs: Vec<(Action, &str, char)> = Vec::new();
        if input.build_mode.is_some() {
            defs.push((Action::Cancel, "CANCEL", 'X'));
        } else {
            if any_villager {
                defs.push((Action::Build(kinds::HOUSE), "HOUSE 30W", 'H'));
                defs.push((Action::Build(kinds::STOREHOUSE), "STORE 100W", 'S'));
            }
            if trainer.is_some() {
                defs.push((Action::Train, "VILLAGER 50F", 'V'));
                defs.push((Action::CancelTrain, "UNQUEUE", 'X'));
            }
            if any_mobile {
                defs.push((Action::Stop, "STOP", 'T'));
            }
        }
        let (bw, bh) = (100.0, 30.0);
        for (n, (action, label, key)) in defs.into_iter().enumerate() {
            let col = (n % 3) as f32;
            let rowi = (n / 3) as f32;
            let b = Button {
                x: left_w + 14.0 + col * (bw + 8.0),
                y: py + 10.0 + rowi * (bh + 8.0),
                w: bw,
                h: bh,
                action,
                label: label.to_string(),
                hotkey: key,
            };
            p.button(&b, false);
            buttons.push(b);
        }
        if let Some(kind) = input.build_mode {
            let info = kinds::info(kind);
            p.text(
                left_w + 14.0,
                py + BOTTOM_PANEL - 24.0,
                &format!(
                    "PLACING {}: CLICK TO BUILD, ESC TO CANCEL",
                    info.name.to_uppercase()
                ),
                false,
                1.0,
            );
        }

        // Health bars over selected damaged units and construction bars over sites.
        for &i in &selected {
            let info = kinds::info(world.kind[i]);
            let hp = fx_to_f32(world.health[i]) / info.max_health as f32;
            let under_construction = world.construction[i].is_some();
            if hp >= 0.999 && !under_construction {
                continue;
            }
            let pos = world.pos[i];
            let (wx, wy) = (fx_to_f32(pos.x), fx_to_f32(pos.y));
            let (gx, gy) = iso::project(wx, wy, iso::ground_height(sim.map(), wx, wy));
            let (sx, sy) = input.camera.to_window(gx, gy);
            let lift = if info.footprint > 0 { 60.0 } else { 50.0 } * input.camera.zoom();
            let w = 32.0;
            p.rect(sx - w / 2.0 - 1.0, sy - lift - 1.0, w + 2.0, 6.0, BLACK, 0);
            let frac = if under_construction {
                world.construction[i].unwrap_or(0) as f32 / info.build_ticks().max(1) as f32
            } else {
                hp.clamp(0.0, 1.0)
            };
            let colour = if under_construction {
                GOLD
            } else if hp > 0.5 {
                GREEN_LIGHT
            } else {
                RED
            };
            p.rect(sx - w / 2.0, sy - lift, w * frac, 4.0, colour, 0);
        }

        Hud {
            sprites: p.out,
            buttons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::SimConfig;

    #[test]
    fn hud_draws_resources_and_context_buttons() {
        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        let camera = Camera::new(sim.map().width(), sim.map().height(), (1280.0, 720.0));
        let villager = sim
            .world()
            .slots()
            .find(|s| {
                sim.world().kind[s.index()] == kinds::VILLAGER && sim.world().owner[s.index()] == 0
            })
            .unwrap()
            .index() as u32;
        let tc = sim
            .world()
            .slots()
            .find(|s| {
                sim.world().kind[s.index()] == kinds::TOWN_CENTER
                    && sim.world().owner[s.index()] == 0
            })
            .unwrap()
            .index() as u32;
        let base = HudInput {
            sim: &sim,
            player: 0,
            camera: &camera,
            selected: &[],
            build_mode: None,
            fps: 60.0,
            paused: false,
            speed: 1.0,
            status: "T0",
        };
        let none = Hud::build(&atlas, &base);
        assert!(none.buttons.is_empty());
        assert!(none.sprites.iter().all(|s| s.screen));
        assert!(none.sprites.len() > 40, "resource bar text");

        let v = Hud::build(
            &atlas,
            &HudInput {
                selected: &[villager],
                ..base
            },
        );
        let actions: Vec<_> = v.buttons.iter().map(|b| b.action).collect();
        assert_eq!(
            actions,
            vec![
                Action::Build(kinds::HOUSE),
                Action::Build(kinds::STOREHOUSE),
                Action::Stop
            ]
        );
        assert!(v.buttons[0].contains(v.buttons[0].x + 1.0, v.buttons[0].y + 1.0));

        let t = Hud::build(
            &atlas,
            &HudInput {
                selected: &[tc],
                ..base
            },
        );
        assert!(t.buttons.iter().any(|b| b.action == Action::Train));
        assert!(!t.buttons.iter().any(|b| b.action == Action::Stop));

        let b = Hud::build(
            &atlas,
            &HudInput {
                selected: &[villager],
                build_mode: Some(kinds::HOUSE),
                ..base
            },
        );
        assert_eq!(b.buttons.len(), 1);
        assert_eq!(b.buttons[0].action, Action::Cancel);
    }
}
