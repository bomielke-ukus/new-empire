//! The render rig, and the proof that it still matches the spec.
//!
//! `assets/render/rig.json` is the frozen camera and light setup every sprite
//! in the game is rendered through. Blender reads it to build the scene; this
//! module reads it to check that the numbers in it still describe the
//! projection `docs/05` §1 specifies.
//!
//! That check matters more than it looks. The rig is a pile of angles that are
//! individually plausible and collectively either exactly right or subtly
//! wrong, and "subtly wrong" does not announce itself — it shows up as sprites
//! that do not quite sit on their tiles, six months and four thousand frames
//! later. So nothing here trusts a number in the file: every one is
//! recomputed from the camera's Euler angles and compared.

use serde::Deserialize;
use std::path::Path;

/// Everything is checked to six decimal places, which is the precision the rig
/// file stores. Anything looser would let a real error through.
const EPSILON: f64 = 1e-5;

type Vec3 = [f64; 3];

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(v: Vec3) -> Vec3 {
    let m = dot(v, v).sqrt();
    [v[0] / m, v[1] / m, v[2] / m]
}

/// Blender's `rotation_euler` in XYZ mode is `Rz · Ry · Rx`. Every rotation in
/// the rig leaves Y at zero, so this only needs X then Z.
fn rotate_xz(x_deg: f64, z_deg: f64, v: Vec3) -> Vec3 {
    let (x, z) = (x_deg.to_radians(), z_deg.to_radians());
    let (cx, sx) = (x.cos(), x.sin());
    let after_x = [v[0], v[1] * cx - v[2] * sx, v[1] * sx + v[2] * cx];
    let (cz, sz) = (z.cos(), z.sin());
    [
        after_x[0] * cz - after_x[1] * sz,
        after_x[0] * sz + after_x[1] * cz,
        after_x[2],
    ]
}

/// The camera basis implied by a Blender rotation. A Blender camera looks down
/// its local −Z with local +Y up and +X right.
pub struct Basis {
    pub forward: Vec3,
    pub up: Vec3,
    pub right: Vec3,
}

impl Basis {
    pub fn from_euler(euler: [f64; 3]) -> Self {
        Basis {
            forward: rotate_xz(euler[0], euler[2], [0.0, 0.0, -1.0]),
            up: rotate_xz(euler[0], euler[2], [0.0, 1.0, 0.0]),
            right: rotate_xz(euler[0], euler[2], [1.0, 0.0, 0.0]),
        }
    }

    /// Projects a world point to screen, in the down-positive convention
    /// `docs/05` §1 uses, in units where the camera basis is unscaled.
    pub fn project(&self, p: Vec3) -> (f64, f64) {
        (dot(p, self.right), -dot(p, self.up))
    }
}

#[derive(Deserialize)]
pub struct Projection {
    pub tile_px: [u32; 2],
    pub elevation_step_px: u32,
    pub authoring_scale: u32,
    pub supersample: u32,
}

#[derive(Deserialize)]
pub struct CameraBasis {
    pub forward: Vec3,
    pub up: Vec3,
    pub right: Vec3,
}

#[derive(Deserialize)]
pub struct Camera {
    pub elevation_deg: f64,
    pub rotation_euler_xyz_deg: [f64; 3],
    pub location: Vec3,
    pub basis: CameraBasis,
}

#[derive(Deserialize)]
pub struct FacingRig {
    pub name: String,
    pub subject_z_rotation_deg: f64,
    pub screen_direction: [f64; 2],
}

#[derive(Deserialize)]
pub struct ScreenPosition {
    pub right: f64,
    pub up: f64,
}

#[derive(Deserialize)]
pub struct Light {
    pub name: String,
    pub rotation_euler_xyz_deg: [f64; 3],
    pub direction_toward_light: Vec3,
    pub screen_position: ScreenPosition,
    pub energy: f64,
}

#[derive(Deserialize)]
pub struct ClassRig {
    pub sprite_px: [u32; 2],
    pub render_px: [u32; 2],
    pub ortho_scale: f64,
    pub anchor_px: [u32; 2],
    pub camera_up_shift: f64,
}

#[derive(Deserialize)]
pub struct Render {
    pub view_transform: String,
    pub film_transparent: bool,
    pub colour_mode: String,
}

#[derive(Deserialize)]
pub struct Rig {
    pub version: u32,
    pub projection: Projection,
    pub camera: Camera,
    pub facings: Vec<FacingRig>,
    pub lights: Vec<Light>,
    pub classes: std::collections::BTreeMap<String, ClassRig>,
    pub render: Render,
}

impl Rig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn basis(&self) -> Basis {
        Basis::from_euler(self.camera.rotation_euler_xyz_deg)
    }

    /// Pixels per world unit along the camera's right axis, at 1× zoom.
    pub fn px_per_unit_1x(&self) -> f64 {
        self.projection.tile_px[0] as f64 / std::f64::consts::SQRT_2
    }

    /// Re-derives `ortho_scale` for a render of the given pixel size.
    pub fn ortho_scale(&self, render_px: [u32; 2]) -> f64 {
        let tile_at_render = (self.projection.tile_px[0]
            * self.projection.authoring_scale
            * self.projection.supersample) as f64;
        render_px[0].max(render_px[1]) as f64 * std::f64::consts::SQRT_2 / tile_at_render
    }

    /// Every way the rig can disagree with the spec, as a list of complaints.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let b = self.basis();
        let close = |a: f64, want: f64| (a - want).abs() < EPSILON;

        // The declared basis has to be the one the Euler angles actually produce.
        for (label, got, declared) in [
            ("forward", b.forward, self.camera.basis.forward),
            ("up", b.up, self.camera.basis.up),
            ("right", b.right, self.camera.basis.right),
        ] {
            if (0..3).any(|i| !close(got[i], declared[i])) {
                out.push(format!(
                    "camera {label} is declared {declared:?} but rotation {:?} gives {got:?}",
                    self.camera.rotation_euler_xyz_deg
                ));
            }
        }

        // The camera must sit opposite the way it looks, or it renders the void.
        let to_camera = norm(self.camera.location);
        if (0..3).any(|i| !close(to_camera[i], -b.forward[i])) {
            out.push(format!(
                "camera is at {:?}, which is not on the opposite side of the \
                 subject from its view direction {:?}",
                self.camera.location, b.forward
            ));
        }

        // docs/05 §1: a tile is exactly twice as wide as it is tall.
        let corners = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let projected: Vec<(f64, f64)> = corners.iter().map(|c| b.project(*c)).collect();
        let width = projected.iter().map(|p| p.0).fold(f64::MIN, f64::max)
            - projected.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        let height = projected.iter().map(|p| p.1).fold(f64::MIN, f64::max)
            - projected.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        let ratio = width / height;
        if !close(
            ratio,
            self.projection.tile_px[0] as f64 / self.projection.tile_px[1] as f64,
        ) {
            out.push(format!(
                "a tile projects {ratio:.6}:1, but docs/05 §1 specifies {}:{}. \
                 The camera elevation must be exactly 30°, since the ratio is 1/sin(elevation).",
                self.projection.tile_px[0], self.projection.tile_px[1]
            ));
        }

        // Elevation: one step is half a tile height on screen.
        let z_rise = -b.project([0.0, 0.0, 1.0]).1;
        if !close(z_rise, self.camera.elevation_deg.to_radians().cos()) {
            out.push(format!(
                "a world Z unit rises {z_rise:.6} on screen, but a {}° camera gives {:.6}",
                self.camera.elevation_deg,
                self.camera.elevation_deg.to_radians().cos()
            ));
        }

        // docs/05 §1: an elevation step is half a tile height.
        if self.projection.elevation_step_px * 2 != self.projection.tile_px[1] {
            out.push(format!(
                "an elevation step is {} px against a {} px tile height; docs/05 §1 \
                 makes it exactly half",
                self.projection.elevation_step_px, self.projection.tile_px[1]
            ));
        }

        // Facings: turning the subject must point it the declared way on screen.
        if self.facings.len() != crate::manifest::AUTHORED_FACINGS.len() {
            out.push(format!(
                "the rig declares {} facings; docs/05 §2.1 authors {}",
                self.facings.len(),
                crate::manifest::AUTHORED_FACINGS.len()
            ));
        }
        for (f, expected) in self.facings.iter().zip(crate::manifest::AUTHORED_FACINGS) {
            if f.name != expected.name() {
                out.push(format!(
                    "facing '{}' is out of order; docs/05 §2.1 authors {:?}",
                    f.name,
                    crate::manifest::AUTHORED_FACINGS.map(|f| f.name())
                ));
            }
            // Subject forward at rotation 0 is +Y.
            let forward = rotate_xz(0.0, f.subject_z_rotation_deg, [0.0, 1.0, 0.0]);
            let (sx, sy) = b.project(forward);
            let n = norm([sx, sy, 0.0]);
            if !close(n[0], f.screen_direction[0]) || !close(n[1], f.screen_direction[1]) {
                out.push(format!(
                    "facing {} rotated {}° points ({:.6}, {:.6}) on screen, not the \
                     declared ({}, {})",
                    f.name,
                    f.subject_z_rotation_deg,
                    n[0],
                    n[1],
                    f.screen_direction[0],
                    f.screen_direction[1]
                ));
            }
        }

        // Lights: the Euler angles must aim a Blender sun's local +Z along the
        // declared direction, and that direction must sit where the rig says it
        // does in screen space.
        for l in &self.lights {
            let aimed = rotate_xz(
                l.rotation_euler_xyz_deg[0],
                l.rotation_euler_xyz_deg[2],
                [0.0, 0.0, 1.0],
            );
            if (0..3).any(|i| !close(aimed[i], l.direction_toward_light[i])) {
                out.push(format!(
                    "light '{}' rotation {:?} aims at {aimed:?}, not the declared {:?}",
                    l.name, l.rotation_euler_xyz_deg, l.direction_toward_light
                ));
            }
            let (right, up) = (
                dot(l.direction_toward_light, b.right),
                dot(l.direction_toward_light, b.up),
            );
            if !close(right, l.screen_position.right) || !close(up, l.screen_position.up) {
                out.push(format!(
                    "light '{}' sits at screen ({right:.6}, {up:.6}), not the declared \
                     ({:.6}, {:.6})",
                    l.name, l.screen_position.right, l.screen_position.up
                ));
            }
            if l.direction_toward_light[2] <= 0.0 {
                out.push(format!(
                    "light '{}' is below the horizon; it would light the subject from underneath",
                    l.name
                ));
            }
        }

        // The one thing docs/08 §7 fixes about the lighting: the key comes from
        // the upper left and is the strongest.
        match self.lights.iter().find(|l| l.name == "key") {
            None => out.push("the rig has no light named 'key'".to_string()),
            Some(key) => {
                if key.screen_position.right >= 0.0 || key.screen_position.up <= 0.0 {
                    out.push(format!(
                        "the key light is at screen ({:.3}, {:.3}); docs/08 §7 puts it \
                         high and to the LEFT, which is negative right and positive up",
                        key.screen_position.right, key.screen_position.up
                    ));
                }
                if let Some(brighter) = self
                    .lights
                    .iter()
                    .find(|l| l.name != "key" && l.energy >= key.energy)
                {
                    out.push(format!(
                        "'{}' is at least as strong as the key light; the key has to \
                         be the light that defines form",
                        brighter.name
                    ));
                }
            }
        }

        // Sizes must agree with the class table the manifest validates against.
        for (name, class) in &self.classes {
            let Some(spec) = class_by_name(name) else {
                out.push(format!(
                    "the rig has a size class '{name}' the manifest does not"
                ));
                continue;
            };
            let (w, h) = spec.size();
            if class.sprite_px != [w, h] {
                out.push(format!(
                    "class {name} is {:?} in the rig but {:?} in docs/05 §2.3",
                    class.sprite_px,
                    [w, h]
                ));
            }
            let want_render = [
                w * self.projection.authoring_scale * self.projection.supersample,
                h * self.projection.authoring_scale * self.projection.supersample,
            ];
            if class.render_px != want_render {
                out.push(format!(
                    "class {name} renders at {:?}; {}× authoring and {}× supersample \
                     of {:?} is {want_render:?}",
                    class.render_px,
                    self.projection.authoring_scale,
                    self.projection.supersample,
                    [w, h]
                ));
            }
            // The camera slides along its own up axis so the subject's origin
            // lands on the anchor instead of at the frame centre.
            let frame = [
                w * self.projection.authoring_scale,
                h * self.projection.authoring_scale,
            ];
            if class.anchor_px[0] != frame[0] / 2 {
                out.push(format!(
                    "class {name} anchors at x {}; the camera is centred, so the \
                     origin lands at x {}",
                    class.anchor_px[0],
                    frame[0] / 2
                ));
            }
            let px_per_unit = self.px_per_unit_1x() * self.projection.authoring_scale as f64;
            let landed = frame[1] as f64 / 2.0 + class.camera_up_shift * px_per_unit;
            if (landed - class.anchor_px[1] as f64).abs() > 1e-3 {
                out.push(format!(
                    "class {name} shifts the camera {:.6} up, which puts the origin at \
                     y {landed:.3}, but the anchor is declared at y {}",
                    class.camera_up_shift, class.anchor_px[1]
                ));
            }
            if class.anchor_px[1] >= frame[1] {
                out.push(format!(
                    "class {name} anchors at y {}, outside its {} px frame",
                    class.anchor_px[1], frame[1]
                ));
            }

            let want_ortho = self.ortho_scale(class.render_px);
            if !close(class.ortho_scale, want_ortho) {
                out.push(format!(
                    "class {name} declares ortho_scale {:.6}; the derivation gives {want_ortho:.6}",
                    class.ortho_scale
                ));
            }
        }
        for spec in ALL_CLASSES {
            if !self.classes.contains_key(class_name(spec)) {
                out.push(format!(
                    "the rig has no entry for size class {}",
                    class_name(spec)
                ));
            }
        }

        // Colour management is the classic Blender trap: the default view
        // transform silently changes every colour the quantiser then matches.
        if self.render.view_transform != "Standard" {
            out.push(format!(
                "view_transform is '{}'; it must be 'Standard', or the colours you \
                 texture are not the colours that reach the palette",
                self.render.view_transform
            ));
        }
        if !self.render.film_transparent {
            out.push(
                "film_transparent is off; the sprite would carry a background and \
                 the game draws its own shadow"
                    .to_string(),
            );
        }
        if self.render.colour_mode != "RGBA" {
            out.push(format!(
                "colour_mode is '{}'; `atlas quantize` needs the alpha to cut the \
                 sprite out",
                self.render.colour_mode
            ));
        }

        out
    }
}

use crate::manifest::Class;

const ALL_CLASSES: [Class; 8] = [
    Class::Foot,
    Class::Mounted,
    Class::Heavy,
    Class::SmallBuilding,
    Class::MediumBuilding,
    Class::LargeBuilding,
    Class::Wonder,
    Class::Terrain,
];

fn class_name(c: Class) -> &'static str {
    match c {
        Class::Foot => "Foot",
        Class::Mounted => "Mounted",
        Class::Heavy => "Heavy",
        Class::SmallBuilding => "SmallBuilding",
        Class::MediumBuilding => "MediumBuilding",
        Class::LargeBuilding => "LargeBuilding",
        Class::Wonder => "Wonder",
        Class::Terrain => "Terrain",
    }
}

fn class_by_name(name: &str) -> Option<Class> {
    ALL_CLASSES.into_iter().find(|c| class_name(*c) == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig() -> Rig {
        Rig::load(Path::new("../../assets/render/rig.json")).expect("the rig must parse")
    }

    #[test]
    fn the_shipped_rig_agrees_with_the_spec() {
        let problems = rig().problems();
        assert!(
            problems.is_empty(),
            "rig disagrees with the spec:\n  {}",
            problems.join("\n  ")
        );
    }

    #[test]
    fn a_tile_projects_exactly_two_to_one() {
        let rig = rig();
        let b = rig.basis();
        let (x1, y1) = b.project([1.0, 0.0, 0.0]);
        let (x2, y2) = b.project([0.0, 1.0, 0.0]);
        // The two tile edges are mirror images across the screen vertical.
        assert!((x1 + x2).abs() < EPSILON);
        assert!((y1 - y2).abs() < EPSILON);
        // And each edge has the 1:2 screen slope that makes the diamond 2:1.
        assert!((y1 / x1).abs() - 0.5 < EPSILON);
    }

    #[test]
    fn thirty_degrees_is_forced_by_the_projection() {
        // Not a preference: any other elevation gives a tile that is not 2:1.
        for elevation in [26.565_f64, 30.0, 35.264] {
            let ratio = 1.0 / elevation.to_radians().sin();
            if (elevation - 30.0).abs() < 1e-9 {
                assert!((ratio - 2.0).abs() < EPSILON, "30° must give 2:1");
            } else {
                assert!((ratio - 2.0).abs() > 0.1, "{elevation}° must not give 2:1");
            }
        }
        assert!((rig().camera.elevation_deg - 30.0).abs() < EPSILON);
    }

    #[test]
    fn mirrored_facings_line_up_with_the_authored_ones() {
        // SE, E and NE are SW, W and NW flipped horizontally (docs/05 §2.1), so
        // the authored facings must all point left or straight up and down —
        // an authored facing pointing right would have no mirror to be.
        for f in &rig().facings {
            assert!(
                f.screen_direction[0] <= EPSILON,
                "{} points right on screen, so mirroring cannot produce its pair",
                f.name
            );
        }
    }

    #[test]
    fn a_broken_rig_is_caught() {
        let mut rig = rig();
        // Tilt the camera to the "26.565°" figure people quote for 2:1 art.
        rig.camera.rotation_euler_xyz_deg[0] = 63.435;
        let problems = rig.problems();
        assert!(
            problems.iter().any(|p| p.contains("projects")),
            "a wrong camera elevation must fail the tile ratio check, got: {problems:?}"
        );
    }

    #[test]
    fn a_camera_shift_that_misses_the_anchor_is_caught() {
        let mut rig = rig();
        rig.classes.get_mut("Foot").unwrap().camera_up_shift = 0.0;
        assert!(rig
            .problems()
            .iter()
            .any(|p| p.contains("puts the origin at")));
    }

    #[test]
    fn terrain_anchors_at_the_tile_centre_and_everything_else_near_its_feet() {
        let rig = rig();
        let terrain = &rig.classes["Terrain"];
        assert_eq!(terrain.camera_up_shift, 0.0);
        assert_eq!(
            terrain.anchor_px,
            [terrain.sprite_px[0], terrain.sprite_px[1]]
        );
        for (name, c) in &rig.classes {
            if name == "Terrain" {
                continue;
            }
            let frame_h = c.sprite_px[1] * rig.projection.authoring_scale;
            assert!(
                c.anchor_px[1] > frame_h * 3 / 4,
                "{name} anchors at {} in a {frame_h} px frame, nowhere near the ground",
                c.anchor_px[1]
            );
        }
    }

    #[test]
    fn an_elevation_step_that_is_not_half_a_tile_is_caught() {
        let mut rig = rig();
        rig.projection.elevation_step_px = 12;
        assert!(rig.problems().iter().any(|p| p.contains("elevation step")));
    }

    #[test]
    fn a_light_moved_below_the_horizon_is_caught() {
        let mut rig = rig();
        rig.lights[0].direction_toward_light[2] = -0.5;
        assert!(rig
            .problems()
            .iter()
            .any(|p| p.contains("below the horizon")));
    }
}
