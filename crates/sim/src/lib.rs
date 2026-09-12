//! The deterministic simulation core of New Empire.
//!
//! Everything in this crate obeys one rule: **given the same seed and the same
//! command log, every machine produces bit-identical state, forever.** That is
//! what makes replays, saves, desync detection and lockstep multiplayer
//! possible, and it is why this crate:
//!
//! - contains no floating-point arithmetic ([`Fx`] is Q16.16 fixed point),
//! - owns its single random number generator ([`Rng`]) and never touches
//!   `SystemTime`, `Instant`, or thread-local randomness,
//! - never iterates a `HashMap`,
//! - advances only in whole ticks, only through [`Simulation::step`],
//! - accepts external input only as [`Command`]s.
//!
//! The presentation layer (rendering, audio, UI) sits *outside* this crate,
//! reads its state, and converts to floats there. Nothing flows back in except
//! commands.
#![deny(clippy::float_arithmetic, clippy::float_cmp)]
#![warn(missing_docs)]

pub mod angle;
pub mod combat;
pub mod command;
pub mod entity;
pub mod flow;
pub mod fx;
pub mod hash;
pub mod kinds;
pub mod map;
pub mod mapgen;
pub mod nav;
pub mod noise;
pub mod orders;
pub mod replay;
pub mod rng;
pub mod simulation;
pub mod tech;
mod trig_table;
pub mod vec2;

pub use angle::Angle;
pub use combat::{Armour, Elevation};
pub use command::{
    Command, CommandError, CommandKind, CommandQueue, PlayerId, COMMAND_DELAY, MAX_COMMAND_IDS,
    MAX_PLAYERS,
};
pub use entity::{EntityId, KindId, Slot, World, WorldViolation};
pub use fx::Fx;
pub use hash::{HashState, StateHasher};
pub use kinds::{Class, Combat, DamageType};
pub use map::{Terrain, TileMap, MAX_ELEVATION};
pub use mapgen::{MapKind, MapSpec};
pub use orders::{
    GatherPhase, Item, Modifiers, Nav, NavState, Order, Player, Production, QueueItem, Rally,
};
pub use replay::{Divergence, Replay, ReplayError, Trace, VerifyError};
pub use rng::Rng;
pub use simulation::{ConfigError, PlaceError, ResearchError, TickStats, TrainError, Violation};
pub use simulation::{
    SimConfig, Simulation, DEFAULT_STOCKPILE, POP_CAP_RANGE, TICKS_PER_SECOND, TICK_MS,
};
pub use tech::{Age, Effect, TechId, TechInfo};
pub use vec2::Vec2Fx;
