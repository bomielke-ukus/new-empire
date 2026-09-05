//! The isometric projection.
//!
//! "World-screen" space is the map projected at 1× with no camera: pixel
//! coordinates where the top corner of tile (0, 0) at elevation 0 is the
//! origin, +x runs screen-right, +y runs screen-down. The camera later scales
//! and offsets this into the window. Both renderers use exactly these
//! functions, so a unit stands on the same pixel of the same tile in each.

/// Tile diamond width in pixels at 1×.
pub const TILE_W: f32 = 64.0;
/// Tile diamond height in pixels at 1×.
pub const TILE_H: f32 = 32.0;
/// Vertical pixels per elevation level at 1×.
pub const ELEV_PX: f32 = 16.0;

/// Projects a world point (tiles) at height `h` (levels) to world-screen px.
#[inline]
pub fn project(wx: f32, wy: f32, h: f32) -> (f32, f32) {
    (
        (wx - wy) * (TILE_W / 2.0),
        (wx + wy) * (TILE_H / 2.0) - h * ELEV_PX,
    )
}

/// Inverse of [`project`] on the ground plane (`h = 0`).
#[inline]
pub fn unproject(sx: f32, sy: f32) -> (f32, f32) {
    let a = sx / (TILE_W / 2.0); // wx - wy
    let b = sy / (TILE_H / 2.0); // wx + wy
    ((a + b) * 0.5, (b - a) * 0.5)
}

/// Screen-space direction of a world direction, normalised.
pub fn direction(dx: f32, dy: f32) -> (f32, f32) {
    let sx = (dx - dy) * (TILE_W / 2.0);
    let sy = (dx + dy) * (TILE_H / 2.0);
    let len = (sx * sx + sy * sy).sqrt();
    if len == 0.0 {
        (0.0, 0.0)
    } else {
        (sx / len, sy / len)
    }
}

/// Pixel bounds `(min_x, min_y, max_x, max_y)` of a whole map in world-screen
/// space, including the tallest possible elevation.
pub fn map_bounds(width: i32, height: i32) -> (f32, f32, f32, f32) {
    let w = width as f32;
    let h = height as f32;
    let (left, _) = project(0.0, h, 0.0);
    let (right, _) = project(w, 0.0, 0.0);
    let (_, top) = project(0.0, 0.0, sim::MAX_ELEVATION as f32);
    let (_, bottom) = project(w, h, 0.0);
    (left, top, right, bottom)
}

/// Bilinear ground height (in levels) at a world point, from corner heights.
pub fn ground_height(map: &sim::TileMap, wx: f32, wy: f32) -> f32 {
    let x0 = wx.floor();
    let y0 = wy.floor();
    let fx = wx - x0;
    let fy = wy - y0;
    let (x0, y0) = (x0 as i32, y0 as i32);
    let c = |cx: i32, cy: i32| map.corner(cx, cy) as f32;
    let top = c(x0, y0) * (1.0 - fx) + c(x0 + 1, y0) * fx;
    let bottom = c(x0, y0 + 1) * (1.0 - fx) + c(x0 + 1, y0 + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn projection_shape() {
        assert_eq!(project(0.0, 0.0, 0.0), (0.0, 0.0));
        assert_eq!(project(1.0, 0.0, 0.0), (32.0, 16.0)); // +x goes right-down
        assert_eq!(project(0.0, 1.0, 0.0), (-32.0, 16.0)); // +y goes left-down
        assert_eq!(project(1.0, 1.0, 0.0), (0.0, 32.0)); // diamond bottom
        assert_eq!(project(1.0, 1.0, 2.0), (0.0, 0.0)); // two levels lift 32 px
    }

    #[test]
    fn unproject_round_trips() {
        for &(x, y) in &[(0.0, 0.0), (3.5, 1.25), (-2.0, 7.0), (120.0, 119.5)] {
            let (sx, sy) = project(x, y, 0.0);
            let (bx, by) = unproject(sx, sy);
            assert!(close(bx, x) && close(by, y), "{x},{y} -> {bx},{by}");
        }
    }

    #[test]
    fn directions() {
        let (dx, dy) = direction(1.0, 0.0);
        assert!(dx > 0.0 && dy > 0.0, "world +x is screen south-east");
        let (dx, dy) = direction(1.0, 1.0);
        assert!(close(dx, 0.0) && dy > 0.0, "world +x+y is straight down");
        let (dx, dy) = direction(-1.0, 1.0);
        assert!(dx < 0.0 && close(dy, 0.0), "world -x+y is straight left");
        assert_eq!(direction(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn bounds_contain_every_tile_corner() {
        let (l, t, r, b) = map_bounds(10, 6);
        for y in 0..=6 {
            for x in 0..=10 {
                for h in 0..=3 {
                    let (sx, sy) = project(x as f32, y as f32, h as f32);
                    assert!(
                        sx >= l && sx <= r && sy >= t && sy <= b,
                        "({x},{y},{h}) outside"
                    );
                }
            }
        }
        assert_eq!((l, r), (-192.0, 320.0));
    }

    #[test]
    fn ground_height_interpolates() {
        let mut m = sim::TileMap::new(2, 2);
        m.set_corner(1, 0, 2);
        assert!(close(ground_height(&m, 0.0, 0.0), 0.0));
        assert!(close(ground_height(&m, 1.0, 0.0), 2.0));
        assert!(close(ground_height(&m, 0.5, 0.0), 1.0));
        assert!(close(ground_height(&m, 0.5, 0.5), 0.5));
    }
}
