//! Camera control from keyboard and mouse.
//!
//! Edge scroll, WASD/arrows, middle-drag, discrete wheel zoom, and
//! click-or-drag on the minimap. Everything is frame-rate independent.

use std::collections::HashSet;
use view::minimap::MinimapRect;
use view::Camera;
use winit::keyboard::KeyCode;

/// Pan speed in window px per second at any zoom.
const PAN_SPEED: f32 = 700.0;
/// Width of the edge-scroll band, px.
const EDGE_BAND: f32 = 14.0;

/// Accumulated input state.
#[derive(Default)]
pub struct Input {
    /// Keys currently held.
    pub held: HashSet<KeyCode>,
    /// Cursor in window px, if inside the window.
    pub cursor: Option<(f32, f32)>,
    /// Middle-button drag in progress: last cursor position.
    pub dragging: Option<(f32, f32)>,
    /// Left button held on the minimap.
    pub scrubbing: bool,
    /// Whether edge scrolling is enabled.
    pub edge_scroll: bool,
    /// Window has focus; edge scroll is suppressed without it.
    pub focused: bool,
    /// Wheel travel not yet turned into a zoom step. A trackpad sends many
    /// small deltas; a notched wheel sends whole ones. Both step once per
    /// unit of travel rather than once per event.
    pub wheel: f32,
}

/// Pixel-delta scroll travel that counts as one zoom step.
const WHEEL_PIXELS_PER_STEP: f32 = 60.0;

impl Input {
    /// Adds wheel travel and returns how many whole zoom steps it amounts
    /// to (positive in, negative out), keeping the remainder.
    pub fn wheel_steps(&mut self, lines: Option<f32>, pixels: Option<f32>) -> i32 {
        let travel = lines.unwrap_or(0.0) + pixels.unwrap_or(0.0) / WHEEL_PIXELS_PER_STEP;
        // A change of direction starts afresh: leftover travel one way must
        // not eat a deliberate turn the other way.
        if travel * self.wheel < 0.0 {
            self.wheel = 0.0;
        }
        self.wheel += travel;
        let steps = self.wheel.trunc();
        self.wheel -= steps;
        steps as i32
    }
}

impl Input {
    /// Fresh state with edge scrolling on.
    pub fn new() -> Input {
        Input {
            edge_scroll: true,
            focused: true,
            ..Default::default()
        }
    }

    /// Applies held keys and edge scrolling for a frame of `dt` seconds.
    pub fn update_camera(&self, cam: &mut Camera, dt: f32) {
        // Speeds are in logical pixels; the camera pans in device pixels.
        let step = PAN_SPEED * dt * cam.dpi;
        let band = EDGE_BAND * cam.dpi;
        let (mut dx, mut dy) = (0.0, 0.0);
        let held = |k: KeyCode| self.held.contains(&k);
        if held(KeyCode::KeyA) || held(KeyCode::ArrowLeft) {
            dx -= step;
        }
        if held(KeyCode::KeyD) || held(KeyCode::ArrowRight) {
            dx += step;
        }
        if held(KeyCode::KeyW) || held(KeyCode::ArrowUp) {
            dy -= step;
        }
        if held(KeyCode::KeyS) || held(KeyCode::ArrowDown) {
            dy += step;
        }
        if self.edge_scroll && self.focused && self.dragging.is_none() {
            if let Some((cx, cy)) = self.cursor {
                let (w, h) = cam.viewport;
                if cx < band {
                    dx -= step;
                } else if cx > w - band {
                    dx += step;
                }
                if cy < band {
                    dy -= step;
                } else if cy > h - band {
                    dy += step;
                }
            }
        }
        if dx != 0.0 || dy != 0.0 {
            cam.pan(dx, dy);
        }
    }

    /// Cursor moved: continues a drag or a minimap scrub.
    pub fn cursor_moved(
        &mut self,
        cam: &mut Camera,
        minimap: MinimapRect,
        map: (i32, i32),
        px: f32,
        py: f32,
    ) {
        if let Some((lx, ly)) = self.dragging {
            cam.pan(lx - px, ly - py);
            self.dragging = Some((px, py));
        } else if self.scrubbing {
            jump_to_minimap(cam, minimap, map, px, py);
        }
        self.cursor = Some((px, py));
    }

    /// Left press: a minimap click jumps the camera. Returns true if consumed.
    pub fn left_pressed(
        &mut self,
        cam: &mut Camera,
        minimap: MinimapRect,
        map: (i32, i32),
        px: f32,
        py: f32,
    ) -> bool {
        if jump_to_minimap(cam, minimap, map, px, py) {
            self.scrubbing = true;
            true
        } else {
            false
        }
    }
}

/// Moves the camera to the map point under a minimap pixel, if any.
pub fn jump_to_minimap(
    cam: &mut Camera,
    minimap: MinimapRect,
    map: (i32, i32),
    px: f32,
    py: f32,
) -> bool {
    match minimap.to_uv(px, py) {
        Some((u, v)) => {
            cam.look_at_tile(u * map.0 as f32, v * map.1 as f32);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_pan_frame_rate_independently() {
        let mut a = Camera::new(64, 64, (800.0, 600.0));
        let mut b = a;
        let mut input = Input::new();
        input.held.insert(KeyCode::KeyD);
        input.update_camera(&mut a, 0.1);
        for _ in 0..10 {
            input.update_camera(&mut b, 0.01);
        }
        assert!((a.focus.0 - b.focus.0).abs() < 1e-3);
        assert!(a.focus.0 > Camera::new(64, 64, (800.0, 600.0)).focus.0);
    }

    #[test]
    fn edge_scroll_only_with_focus_and_cursor() {
        let base = Camera::new(64, 64, (800.0, 600.0));
        let mut cam = base;
        let mut input = Input::new();
        input.cursor = Some((2.0, 300.0));
        input.update_camera(&mut cam, 0.1);
        assert!(cam.focus.0 < base.focus.0, "left edge scrolls left");
        let mut cam2 = base;
        input.focused = false;
        input.update_camera(&mut cam2, 0.1);
        assert_eq!(cam2.focus, base.focus, "no edge scroll when unfocused");
    }

    #[test]
    fn wheel_travel_accumulates_into_whole_steps() {
        let mut input = Input::new();
        // A trackpad: many small pixel deltas.
        let mut steps = 0;
        for _ in 0..10 {
            steps += input.wheel_steps(None, Some(9.0));
        }
        assert_eq!(steps, 1, "90 px of travel is one step, not ten");
        // A notched wheel: one line per notch, one step per notch.
        assert_eq!(input.wheel_steps(Some(-1.0), None), -1);
        assert_eq!(input.wheel_steps(Some(-2.0), None), -2);
        // Jitter the other way does not step until it adds up.
        assert_eq!(input.wheel_steps(None, Some(-3.0)), 0);
    }

    #[test]
    fn drag_moves_world_with_cursor_and_minimap_jumps() {
        let mut cam = Camera::new(64, 64, (800.0, 600.0));
        let before = cam.focus;
        let mut input = Input::new();
        let mm = MinimapRect::bottom_right((800.0, 600.0), 200.0, 10.0);
        input.dragging = Some((400.0, 300.0));
        input.cursor_moved(&mut cam, mm, (64, 64), 410.0, 300.0);
        assert!(cam.focus.0 < before.0, "dragging right moves the view left");

        input.dragging = None;
        assert!(!input.left_pressed(&mut cam, mm, (64, 64), 10.0, 10.0));
        assert!(input.left_pressed(&mut cam, mm, (64, 64), mm.cx, mm.cy));
        let (wx, wy) = cam.window_to_world(400.0, 300.0);
        assert!(
            (wx - 32.0).abs() < 0.5 && (wy - 32.0).abs() < 0.5,
            "centre of minimap is map centre: {wx},{wy}"
        );
    }
}
