//! Everything between the simulation and the pixels that does not need a GPU.
//!
//! The GPU renderer (`crates/render`) and the software rasteriser here consume
//! the same inputs — terrain vertex buffers, sorted sprite instances, the
//! palette and atlas — so a PNG from [`raster`] is a faithful preview of a
//! frame, and the CPU-side logic is unit-testable without a window.
//!
//! Floats are fine here. Nothing flows back into the simulation.

pub mod camera;
pub mod combat_view;
pub mod feedback;
pub mod fog;
pub mod font;
pub mod hints;
pub mod hud;
pub mod iso;
pub mod minimap;
pub mod notify;
pub mod palette;
pub mod palette_table;
pub mod raster;
pub mod scene;
pub mod settings;
pub mod sheets;
pub mod shell;
pub mod sprites;
pub mod terrain;

pub use camera::Camera;
pub use fog::FogLights;
pub use hints::{Conditions, Hint, Hints};
pub use hud::{Action, Button, Hud, HudInput};
pub use notify::{Notice, NoticeKind, Notices};
pub use scene::{Ghost, Scene, SceneOptions, SpriteInstance, Sweep, SWEEP_MS};
pub use settings::{Control, Settings};
pub use shell::{LoadRow, Screen, Setup, ShellAction, ShellButton, ShellInput};
pub use sprites::{Anim, Atlas, Frame, Ink};
pub use terrain::{ChunkMesh, TerrainVertex, CHUNK_TILES};

/// Converts a fixed-point value to `f32` for presentation.
#[inline]
pub fn fx_to_f32(v: sim::Fx) -> f32 {
    v.raw() as f32 / 65536.0
}
