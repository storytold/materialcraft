//! `UiState` as a stored settings document, and the import of the eframe-era file.

use std::path::PathBuf;

use craft_settings::LegacyFile;

use crate::UiState;

/// The old eframe storage file (or the browser's localStorage item, keyed by `path`), read once and never removed.
#[allow(dead_code)] // P3(shell-w4): sh.A5
pub(crate) fn eframe_legacy(path: PathBuf) -> LegacyFile<UiState> {
    let _ = path;
    todo!("sh.A5")
}
