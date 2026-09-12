//! The isometric camera: which part of world-screen space is on the window,
//! and at what zoom.

use crate::iso;

/// Discrete zoom levels; free zoom makes pixel art swim. These are in
/// sprite pixels per *logical* pixel: on a 2× display every level draws
/// twice as many device pixels, so `1.0` looks the same size everywhere.
pub const ZOOM_LEVELS: [f32; 6] = [0.5, 0.75, 1.0, 1.5, 2.0, 3.0];
/// The index of `1.0`.
pub const DEFAULT_ZOOM_INDEX: usize = 2;

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
    /// Device pixels per logical pixel: 1 on an ordinary display, 2 on a
    /// Retina one. Folded into [`Camera::zoom`], so nothing downstream has
    /// to know.
    pub dpi: f32,
}

impl Camera {
    /// A camera over a `width × height` tile map, focused on its centre at 1×.
    pub fn new(map_width: i32, map_height: i32, viewport: (f32, f32)) -> Camera {
        let bounds = iso::map_bounds(map_width, map_height);
        let focus = iso::project(map_width as f32 / 2.0, map_height as f32 / 2.0, 0.0);
        Camera {
            focus,
            zoom_index: DEFAULT_ZOOM_INDEX,
            viewport,
            bounds,
            dpi: 1.0,
        }
    }

    /// Device pixels per sprite pixel: the zoom level times the display
    /// scale. This is what projection, picking and the renderer use.
    pub fn zoom(&self) -> f32 {
        ZOOM_LEVELS[self.zoom_index] * self.dpi
    }

    /// The zoom level as the player understands it, display scale aside.
    pub fn zoom_level(&self) -> f32 {
        ZOOM_LEVELS[self.zoom_index]
    }

    /// Steps zoom in (`+1`) or out (`-1`), keeping the focus point fixed.
    pub fn zoom_step(&mut self, delta: i32) {
        let n = ZOOM_LEVELS.len() as i32;
        self.zoom_index = (self.zoom_index as i32 + delta).clamp(0, n - 1) as usize;
    }

    /// Steps zoom keeping the world point under window pixel `(px, py)`
    /// where it is, which is what a wheel over the map should do.
    pub fn zoom_step_at(&mut self, delta: i32, px: f32, py: f32) {
        let before = self.from_window(px, py);
        self.zoom_step(delta);
        let after = self.from_window(px, py);
        self.focus.0 += before.0 - after.0;
        self.focus.1 += before.1 - after.1;
        self.clamp();
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
        assert_eq!(cam.zoom(), 1.0, "a fresh camera is at 1x");
        let p = (cam.focus.0 + 100.0, cam.focus.1 + 50.0);
        assert_eq!(cam.to_window(p.0, p.1), (500.0, 350.0));
        cam.zoom_step(1);
        assert_eq!(cam.zoom(), 1.5);
        assert_eq!(cam.to_window(p.0, p.1), (550.0, 375.0));
        cam.zoom_step(10);
        assert_eq!(cam.zoom(), 3.0, "clamped at the top");
        cam.zoom_step(-10);
        assert_eq!(cam.zoom(), 0.5, "and at the bottom");
        cam.set_zoom_index(0);
        assert_eq!(cam.zoom(), 0.5);
    }

    #[test]
    fn zoom_at_a_point_keeps_it_under_the_cursor() {
        let mut cam = Camera::new(64, 64, (800.0, 600.0));
        let (px, py) = (650.0, 120.0);
        let world = cam.window_to_world(px, py);
        cam.zoom_step_at(1, px, py);
        let after = cam.window_to_world(px, py);
        assert!((world.0 - after.0).abs() < 1e-3 && (world.1 - after.1).abs() < 1e-3);
        assert_ne!(
            cam.focus,
            Camera::new(64, 64, (800.0, 600.0)).focus,
            "the focus moved"
        );
    }

    #[test]
    fn display_scale_doubles_device_pixels_not_the_view() {
        let mut one = Camera::new(64, 64, (800.0, 600.0));
        let mut two = Camera::new(64, 64, (1600.0, 1200.0));
        two.dpi = 2.0;
        assert_eq!(two.zoom_level(), one.zoom_level());
        assert_eq!(two.zoom(), 2.0 * one.zoom());
        // Both windows show the same slice of the world.
        let a = one.visible_rect();
        let b = two.visible_rect();
        assert!((a.2 - a.0 - (b.2 - b.0)).abs() < 1e-3);
        assert!((a.3 - a.1 - (b.3 - b.1)).abs() < 1e-3);
        // A point lands at twice the device pixel.
        let p = (one.focus.0 + 100.0, one.focus.1 + 50.0);
        let (x1, y1) = one.to_window(p.0, p.1);
        let (x2, y2) = two.to_window(p.0, p.1);
        assert_eq!((x2, y2), (x1 * 2.0, y1 * 2.0));
        // Panning by the same device distance moves the same world distance
        // only when the caller scales it, which `Input` does.
        one.pan(30.0, 0.0);
        two.pan(60.0, 0.0);
        assert!((one.focus.0 - two.focus.0).abs() < 1e-3);
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
