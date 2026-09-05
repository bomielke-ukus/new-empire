//! The isometric camera: which part of world-screen space is on the window,
//! and at what zoom.

use crate::iso;

/// Discrete zoom levels; free zoom makes pixel art swim. `0.5` exists for
/// overview renders and the map viewer, not for play.
pub const ZOOM_LEVELS: [f32; 4] = [0.5, 1.0, 1.5, 2.0];

/// Camera state.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Camera {
    /// World-screen point at the centre of the viewport.
    pub focus: (f32, f32),
    /// Index into [`ZOOM_LEVELS`].
    pub zoom_index: usize,
    /// Window size in physical pixels.
    pub viewport: (f32, f32),
    /// Pan limits in world-screen space `(min_x, min_y, max_x, max_y)`.
    pub bounds: (f32, f32, f32, f32),
}

impl Camera {
    /// A camera over a `width × height` tile map, focused on its centre at 1×.
    pub fn new(map_width: i32, map_height: i32, viewport: (f32, f32)) -> Camera {
        let bounds = iso::map_bounds(map_width, map_height);
        let focus = iso::project(map_width as f32 / 2.0, map_height as f32 / 2.0, 0.0);
        Camera {
            focus,
            zoom_index: 1,
            viewport,
            bounds,
        }
    }

    /// The zoom factor.
    pub fn zoom(&self) -> f32 {
        ZOOM_LEVELS[self.zoom_index]
    }

    /// Steps zoom in (`+1`) or out (`-1`), keeping the focus point fixed.
    pub fn zoom_step(&mut self, delta: i32) {
        let n = ZOOM_LEVELS.len() as i32;
        self.zoom_index = (self.zoom_index as i32 + delta).clamp(1, n - 1) as usize;
    }

    /// Sets the zoom index directly, including the overview level.
    pub fn set_zoom_index(&mut self, index: usize) {
        self.zoom_index = index.min(ZOOM_LEVELS.len() - 1);
    }

    /// Moves the focus by a screen-space delta in pixels (so panning speed is
    /// constant on screen regardless of zoom), then clamps to the map.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.focus.0 += dx / self.zoom();
        self.focus.1 += dy / self.zoom();
        self.clamp();
    }

    /// Centres on a world tile position.
    pub fn look_at_tile(&mut self, wx: f32, wy: f32) {
        self.focus = iso::project(wx, wy, 0.0);
        self.clamp();
    }

    /// Keeps the focus inside the map bounds.
    pub fn clamp(&mut self) {
        let (l, t, r, b) = self.bounds;
        self.focus.0 = self.focus.0.clamp(l, r);
        self.focus.1 = self.focus.1.clamp(t, b);
    }

    /// World-screen → window pixels.
    #[inline]
    pub fn to_window(&self, sx: f32, sy: f32) -> (f32, f32) {
        let z = self.zoom();
        (
            (sx - self.focus.0) * z + self.viewport.0 * 0.5,
            (sy - self.focus.1) * z + self.viewport.1 * 0.5,
        )
    }

    /// Window pixels → world-screen.
    #[inline]
    pub fn from_window(&self, px: f32, py: f32) -> (f32, f32) {
        let z = self.zoom();
        (
            (px - self.viewport.0 * 0.5) / z + self.focus.0,
            (py - self.viewport.1 * 0.5) / z + self.focus.1,
        )
    }

    /// Window pixels → world tile coordinates on the ground plane.
    pub fn window_to_world(&self, px: f32, py: f32) -> (f32, f32) {
        let (sx, sy) = self.from_window(px, py);
        iso::unproject(sx, sy)
    }

    /// The world-screen rectangle currently visible, `(min_x, min_y, max_x, max_y)`.
    pub fn visible_rect(&self) -> (f32, f32, f32, f32) {
        let (l, t) = self.from_window(0.0, 0.0);
        let (r, b) = self.from_window(self.viewport.0, self.viewport.1);
        (l, t, r, b)
    }

    /// Uniform data for the GPU: `[focus_x, focus_y, zoom, viewport_w, viewport_h]`.
    pub fn uniform(&self) -> [f32; 6] {
        [
            self.focus.0,
            self.focus.1,
            self.zoom(),
            self.viewport.0,
            self.viewport.1,
            0.0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_of_window_is_focus() {
        let cam = Camera::new(64, 64, (800.0, 600.0));
        let (px, py) = cam.to_window(cam.focus.0, cam.focus.1);
        assert_eq!((px, py), (400.0, 300.0));
        let (sx, sy) = cam.from_window(400.0, 300.0);
        assert_eq!((sx, sy), cam.focus);
        let (wx, wy) = cam.window_to_world(400.0, 300.0);
        assert!((wx - 32.0).abs() < 1e-3 && (wy - 32.0).abs() < 1e-3);
    }

    #[test]
    fn zoom_scales_about_focus() {
        let mut cam = Camera::new(64, 64, (800.0, 600.0));
        let p = (cam.focus.0 + 100.0, cam.focus.1 + 50.0);
        assert_eq!(cam.to_window(p.0, p.1), (500.0, 350.0));
        cam.zoom_step(1);
        assert_eq!(cam.zoom(), 1.5);
        assert_eq!(cam.to_window(p.0, p.1), (550.0, 375.0));
        cam.zoom_step(10);
        assert_eq!(cam.zoom(), 2.0);
        cam.zoom_step(-10);
        assert_eq!(cam.zoom(), 1.0, "play zoom never drops below 1x");
        cam.set_zoom_index(0);
        assert_eq!(cam.zoom(), 0.5);
    }

    #[test]
    fn pan_is_clamped_to_map() {
        let mut cam = Camera::new(16, 16, (800.0, 600.0));
        cam.pan(-100_000.0, -100_000.0);
        assert_eq!((cam.focus.0, cam.focus.1), (cam.bounds.0, cam.bounds.1));
        cam.pan(100_000.0, 100_000.0);
        assert_eq!((cam.focus.0, cam.focus.1), (cam.bounds.2, cam.bounds.3));
        cam.look_at_tile(8.0, 8.0);
        assert_eq!(cam.focus, iso::project(8.0, 8.0, 0.0));
    }

    #[test]
    fn pan_speed_is_screen_constant() {
        let mut a = Camera::new(64, 64, (800.0, 600.0));
        let mut b = a;
        b.zoom_step(1);
        a.pan(30.0, 0.0);
        b.pan(30.0, 0.0);
        assert!(
            b.focus.0 - 0.0 < a.focus.0,
            "zoomed-in camera moves fewer world px"
        );
    }

    #[test]
    fn visible_rect_matches_viewport() {
        let cam = Camera::new(64, 64, (800.0, 600.0));
        let (l, t, r, b) = cam.visible_rect();
        assert_eq!(r - l, 800.0);
        assert_eq!(b - t, 600.0);
    }
}
