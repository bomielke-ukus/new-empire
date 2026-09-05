//! Everything between the simulation and the pixels that does not need a GPU.
//!
//! The GPU renderer (`crates/render`) and the software rasteriser here consume
//! the same inputs — terrain vertex buffers, sorted sprite instances, the
//! palette and atlas — so a PNG from [`raster`] is a faithful preview of a
//! frame, and the CPU-side logic is unit-testable without a window.
//!
//! Floats are fine here. Nothing flows back into the simulation.

pub mod camera;
pub mod iso;
pub mod minimap;
pub mod palette;
pub mod raster;
pub mod scene;
pub mod sprites;
pub mod terrain;

pub use camera::Camera;
pub use scene::{Scene, SpriteInstance};
pub use sprites::{Atlas, Frame};
pub use terrain::{ChunkMesh, TerrainVertex, CHUNK_TILES};

/// Converts a fixed-point value to `f32` for presentation.
#[inline]
pub fn fx_to_f32(v: sim::Fx) -> f32 {
    v.raw() as f32 / 65536.0
}
