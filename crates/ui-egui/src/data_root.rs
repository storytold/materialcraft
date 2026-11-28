//! The Nightly profile's private data root, selected once by the desktop app after the Launcher
//! protocol validated it. Stable leaves it unset, so every path below keeps its old resolution.
//!
//! The app crate owns the launch profile (`apps/materialcraft/src/launch_profile.rs`); this module
//! only remembers the validated folder for the libraries that write beside the user's settings.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The folder inside the root that holds crash-recovery copies (the stable app uses
/// `Documents/MaterialCraft/autosave` for the same files).
pub const RECOVERY: &str = "Recovery";

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Take the validated private root. Only the desktop app calls this, once, before any window.
pub fn install(root: PathBuf) -> Result<(), String> {
    if !root.is_absolute() {
        return Err("the nightly data root must be absolute".into());
    }
    ROOT.set(root).map_err(|_| "the nightly data root was already selected".into())
}

/// The private root, or `None` for a stable launch.
pub fn root() -> Option<&'static Path> {
    ROOT.get().map(PathBuf::as_path)
}

/// A folder inside the private root, or `None` for a stable launch.
pub fn sub(name: &str) -> Option<PathBuf> {
    root().map(|root| root.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stable_launch_has_no_private_root() {
        assert_eq!(root(), None);
        assert_eq!(sub("cache"), None);
    }
}
