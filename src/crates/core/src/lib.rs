//! Shared simulation: no graphics, audio, window system, or networking dependencies.
/// The vector types the simulation speaks (for scripts and tools).
pub use glam;
pub mod sim;
pub mod bot_nav;
pub mod terrain;
pub mod grass;
pub mod map_pack;
pub mod catalog;
pub mod defs;
pub mod registry;
pub mod jsonc;
pub mod modes;
pub mod mods;
pub mod look;
pub mod conditions;
pub mod map_catalog;
pub mod equipment;
pub mod control;
pub mod water;
pub mod combat;
pub mod feed;
pub mod names;
pub mod vote;
