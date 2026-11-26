use materialcraft_geom::view::View3d;
use serde::{Deserialize, Serialize};

/// The file extension of a saved 3D view state
pub const VIEW_STATE_EXTENSION: &str = "mcview";

/// A saved state of the 3D view: its scene, camera, lights, environment, display and material settings
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct View3dStateFile {
    /// Always [`View3dStateFile::FORMAT`]: what tells this file from other JSON
    pub(crate) format: String,
    pub(crate) version: u32,
    pub(crate) view: View3d,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl View3dStateFile {
    pub(crate) const FORMAT: &'static str = "materialcraft-3d-view";
    pub(crate) const VERSION: u32 = 1;
    /// The most a state file may hold: far more than any view, small enough to read at once
    pub(crate) const MAX_BYTES: u64 = 1 << 20;

    pub(crate) fn of(view: &View3d) -> View3dStateFile {
        View3dStateFile { format: View3dStateFile::FORMAT.into(), version: View3dStateFile::VERSION, view: view.clone() }
    }

    /// The state in `text`, or why it is not one this version reads
    pub(crate) fn parse(text: &str) -> std::result::Result<View3dStateFile, String> {
        let state: View3dStateFile = serde_json::from_str(text).map_err(|error| format!("not a 3D view state: {error}"))?;

        if state.format != View3dStateFile::FORMAT {
            return Err("not a 3D view state".into());
        }

        if state.version > View3dStateFile::VERSION {
            return Err(format!("saved by a newer version (state version {})", state.version));
        }
        Ok(state)
    }
}
