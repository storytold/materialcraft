pub use craft_sbs::SBS_EXT;
pub(super) use craft_sbs::{FIRST_UID, MAX_XML_NODES, SELF_DEPENDENCY};

pub(crate) const MAX_GRAPHS: usize = 4096;

pub(crate) const MAX_NODES: usize = 100_000;

pub(super) const UPDATER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(super) const MAX_WARNINGS: usize = 600;

/// Most wires one SBS may carry, across all graphs (audit: MaterialCraft #2).
pub(super) const MAX_WIRES: usize = 1_000_000;
pub(super) const NODE_HALF_SIZE: [f32; 2] = [50.0, 40.0];
