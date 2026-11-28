//! App-owned nightly profile fences; no stable fallback, migration or process environment changes.

use std::ffi::OsString;

use std::path::{Component, Path, PathBuf};

/// A marker beside an executable that makes the stable copy portable (`craft_update`'s convention).
const PORTABLE_MARKER: &str = "portable.txt";
/// The data folder a portable copy keeps beside itself.
const PORTABLE_DATA_DIR: &str = "MaterialCraftData";
/// The folder `Documents/MaterialCraft` holds the stable autosaves in.
const USER_DATA_DIR: &str = "MaterialCraft";
/// The private recovery folder inside the nightly profile (the same name the stable app's autosave
/// folder takes under its Documents folder).
const RECOVERY: &str = materialcraft_ui_egui::data_root::RECOVERY;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Platform {
    Windows,
    Linux,
    Mac,
}

impl Platform {
    fn current() -> Result<Self, String> {
        match std::env::consts::OS {
            "windows" => Ok(Self::Windows),
            "linux" => Ok(Self::Linux),
            "macos" => Ok(Self::Mac),
            _ => Err("nightly-v1 is not defined for this operating system".into()),
        }
    }

    fn ignores_case(self) -> bool {
        self != Self::Linux
    }
}

fn absolute(path: PathBuf, label: &str) -> Result<PathBuf, String> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("{label} must be absolute and contain no parent traversal"));
    }
    Ok(path)
}

fn required_base(env: &impl Fn(&str) -> Option<OsString>, key: &str) -> Result<PathBuf, String> {
    let value = env(key).filter(|v| !v.is_empty()).ok_or_else(|| format!("cannot establish the stable {key} boundary"))?;
    absolute(PathBuf::from(value), key)
}

fn xdg_base(env: &impl Fn(&str) -> Option<OsString>, key: &str, fallback: &str) -> Result<PathBuf, String> {
    match env(key).filter(|v| !v.is_empty()) {
        Some(value) => absolute(PathBuf::from(value), key),
        None => Ok(required_base(env, "HOME")?.join(fallback)),
    }
}

/// Resolve existing ancestors before creating anything. A dangling link or inaccessible ancestor
/// is not a missing directory that may be replaced or followed later.
fn canonical_future(path: &Path) -> Result<PathBuf, String> {
    let mut parent = path;
    let mut missing = Vec::new();
    loop {
        match std::fs::symlink_metadata(parent) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(parent.file_name().ok_or("the profile root has no existing ancestor")?.to_os_string());
                parent = parent.parent().ok_or("the profile root has no existing ancestor")?;
            }
            Err(error) => return Err(format!("cannot inspect the profile boundary: {error}")),
        }
    }
    let mut resolved = std::fs::canonicalize(parent).map_err(|e| format!("cannot resolve the profile boundary: {e}"))?;
    for part in missing.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

fn overlaps(a: &Path, b: &Path, platform: Platform) -> bool {
    let parts = |p: &Path| {
        p.components()
            .map(|c| {
                let value = c.as_os_str().to_string_lossy().into_owned();
                if platform.ignores_case() { value.to_lowercase() } else { value }
            })
            .collect::<Vec<_>>()
    };
    let (a, b) = (parts(a), parts(b));
    a.starts_with(&b) || b.starts_with(&a)
}

fn installation_boundary(platform: Platform, exe: &Path) -> Result<PathBuf, String> {
    let parent = exe.parent().ok_or("cannot establish the installation boundary")?;
    if platform == Platform::Mac {
        // A profile in Resources is still inside the versioned install, not beside MacOS.
        if let Some(bundle) = exe.ancestors().find(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("app"))) {
            return Ok(bundle.to_path_buf());
        }
    }
    Ok(parent.to_path_buf())
}

/// The stable folder holding autosaves and exported documents in the user's Documents folder.
/// It is resolved the same way `user_directory::user_dir` resolves it: `USERPROFILE` on Windows,
/// `HOME` elsewhere.
fn documents_boundary(env: &impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
    let home = match env("USERPROFILE").filter(|value| !value.is_empty()) {
        Some(value) => absolute(PathBuf::from(value), "USERPROFILE")?,
        None => required_base(env, "HOME")?,
    };
    Ok(home.join("Documents").join(USER_DATA_DIR))
}

fn forbidden_roots(platform: Platform, env: &impl Fn(&str) -> Option<OsString>, exe: &Path) -> Result<Vec<PathBuf>, String> {
    let mut roots = match platform {
        Platform::Windows => {
            let mut roots =
                ["APPDATA", "LOCALAPPDATA"].into_iter().map(|key| required_base(env, key).map(|p| p.join(USER_DATA_DIR))).collect::<Result<Vec<_>, _>>()?;
            // The MSI's default per-machine folder (materialcraft.wxs). A custom install folder is
            // not known here; the executable's own folder is fenced below.
            for key in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
                if let Some(base) = env(key).filter(|v| !v.is_empty()) {
                    roots.push(absolute(PathBuf::from(base), key)?.join(USER_DATA_DIR));
                }
            }
            roots
        }
        Platform::Linux => {
            let mut roots =
                [("XDG_CONFIG_HOME", ".config"), ("XDG_DATA_HOME", ".local/share"), ("XDG_CACHE_HOME", ".cache"), ("XDG_STATE_HOME", ".local/state")]
                    .into_iter()
                    .map(|(key, fallback)| xdg_base(env, key, fallback).map(|p| p.join("materialcraft")))
                    .collect::<Result<Vec<_>, _>>()?;
            roots.extend(["/usr/bin/materialcraft", "/usr/local/bin/materialcraft", "/opt/materialcraft"].map(PathBuf::from));
            // The stable Flatpak keeps its data in its own per-app folder.
            roots.push(required_base(env, "HOME")?.join(".var/app/ai.storyteller.materialcraft"));
            roots
        }
        Platform::Mac => {
            let home = required_base(env, "HOME")?;
            let library = home.join("Library");
            vec![
                library.join("Application Support/MaterialCraft"),
                library.join("Caches/materialcraft"),
                library.join("Caches/ai.storyteller.materialcraft"),
                library.join("Preferences/MaterialCraft"),
                library.join("Preferences/ai.storyteller.materialcraft.plist"),
                library.join("Saved Application State/ai.storyteller.materialcraft.savedState"),
                home.join("Applications/MaterialCraft.app"),
                PathBuf::from("/Applications/MaterialCraft.app"),
            ]
        }
    };
    roots.push(documents_boundary(env)?);
    roots.push(installation_boundary(platform, exe)?);
    roots.into_iter().map(|p| canonical_future(&p)).collect()
}

fn private_subdirectory(root: &Path, name: impl AsRef<Path>, label: &str) -> Result<PathBuf, String> {
    let chosen = canonical_future(&absolute(root.to_path_buf(), "the nightly profile root")?)?;
    let child = canonical_future(&chosen.join(name))?;
    if child == chosen || !child.starts_with(&chosen) {
        return Err(format!("the nightly {label} must stay inside its selected profile"));
    }
    Ok(child)
}

/// Every existing entry directly in the profile (Recovery, app.ron, cache and so on) must resolve
/// inside it, so a link there cannot point Nightly's writes at another app's data.
fn entries_stay_inside(root: &Path) -> Result<(), String> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("cannot inspect the nightly profile: {error}")),
    };
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot inspect the nightly profile: {e}"))?;
        private_subdirectory(root, entry.file_name(), "profile entry")?;
    }
    // The preview cache is the one nested destination Nightly writes and clears on its own: a link
    // at cache/previews would put its thumbnails in, and clear, the stable app's cache instead.
    private_subdirectory(root, std::path::Path::new("cache").join("previews"), "preview cache")?;
    Ok(())
}

/// Recognise the shared portable data namespace in both the supplied and resolved path.
fn is_portable_layout(path: &Path, platform: Platform) -> bool {
    // A folder under a portable copy belongs to that copy too. Above the chosen folder itself, a
    // marker only counts beside a MaterialCraft executable, so an unrelated portable.txt higher up
    // (in a user folder or at a drive root) does not refuse the profile.
    path.ancestors().enumerate().any(|(depth, ancestor)| {
        ancestor.join(PORTABLE_MARKER).is_file() && (depth == 0 || ["materialcraft.exe", "materialcraft"].iter().any(|exe| ancestor.join(exe).is_file()))
    }) || path.components().any(|component| match component {
        Component::Normal(name) if platform.ignores_case() => name.to_string_lossy().eq_ignore_ascii_case(PORTABLE_DATA_DIR),
        Component::Normal(name) => name == PORTABLE_DATA_DIR,
        _ => false,
    })
}

/// The app's exclusive writable probe: create a file only this process can make and remove it.
pub fn probe_writable(dir: &Path) -> std::io::Result<()> {
    let probe = dir.join(format!(".nightly-write-probe-{}", std::process::id()));
    std::fs::OpenOptions::new().write(true).create_new(true).open(&probe)?;
    std::fs::remove_file(probe)
}

/// The writable hook is the app's exclusive probe. Keeping it injected lets filesystem boundary
/// tests exercise the real policy without starting the app or changing global state.
pub fn validate(
    root: &Path,
    env: impl Fn(&str) -> Option<OsString>,
    exe: Option<&Path>,
    writable: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<PathBuf, String> {
    validate_for(Platform::current()?, root, env, exe, writable)
}

fn validate_for(
    platform: Platform,
    root: &Path,
    env: impl Fn(&str) -> Option<OsString>,
    exe: Option<&Path>,
    writable: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<PathBuf, String> {
    absolute(root.to_path_buf(), "the nightly profile root")?;
    // Each of these redirects or disables state the profile is supposed to own.
    for (key, why) in [
        ("MATERIALCRAFT_DATA_DIR", "the autosaves, recovery copies and saved layout"),
        ("MATERIALCRAFT_CACHE_DIR", "the preview cache"),
        ("MATERIALCRAFT_NO_PREFS", "the saved preferences and window layout"),
    ] {
        if env(key).is_some_and(|v| !v.is_empty()) {
            return Err(format!("nightly-v1 cannot be combined with {key}, which would take {why} outside its private profile"));
        }
    }
    if platform != Platform::Windows
        && let Some(id) = env("FLATPAK_ID").filter(|v| !v.is_empty())
    {
        // A Flatpak's data folder and runtime socket namespace belong to its app id. The nightly
        // bundle has an id of its own ([`NIGHTLY_FLATPAK_ID`], named by
        // `packaging/linux/flatpak-bundle.sh`), so its profile is already the sandbox's own; any
        // other instance is the stable app's (or another app's), whose socket this profile must
        // not inherit.
        if id.to_string_lossy().as_ref() != NIGHTLY_FLATPAK_ID {
            return Err("nightly-v1 cannot inherit another Flatpak app's instance; launch through the native Launcher".into());
        }
    }
    let chosen = canonical_future(root)?;
    if is_portable_layout(root, platform) || is_portable_layout(&chosen, platform) {
        return Err("the nightly root must not use a portable MaterialCraft installation or its MaterialCraftData settings folder".into());
    }
    let exe = absolute(exe.ok_or("cannot establish the installation boundary")?.to_path_buf(), "the executable")?;
    let forbidden = forbidden_roots(platform, &env, &exe)?;
    if forbidden.iter().any(|stable| overlaps(&chosen, stable, platform)) {
        return Err("the nightly root must not overlap stable data or an installation".into());
    }
    // Recovery can read, adopt, overwrite and sweep entries. Never let its alias leave this
    // private profile, including toward a stable Documents folder not in our known roots.
    private_subdirectory(&chosen, RECOVERY, "recovery folder")?;
    entries_stay_inside(&chosen)?;
    let mut create = std::fs::DirBuilder::new();
    create.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        create.mode(0o700);
    }
    create.create(&chosen).map_err(|e| format!("cannot create the nightly profile: {e}"))?;
    let confirmed = std::fs::canonicalize(&chosen).map_err(|e| format!("cannot resolve the created nightly profile: {e}"))?;
    if confirmed != chosen || forbidden.iter().any(|stable| overlaps(&confirmed, stable, platform)) {
        return Err("the nightly profile changed while it was being validated".into());
    }
    private_subdirectory(&confirmed, RECOVERY, "recovery folder")?;
    entries_stay_inside(&confirmed)?;
    writable(&confirmed).map_err(|e| format!("the nightly profile is not writable: {e}"))?;
    Ok(confirmed)
}

/// The app id of the nightly Flatpak bundle (`packaging/linux/flatpak-bundle.sh`): the one Flatpak
/// instance whose sandbox is this channel's own. The window identity (`os_window`) and the bundle id
/// differ on purpose, as they do for stable.
pub const NIGHTLY_FLATPAK_ID: &str = "ai.storyteller.materialcraft.nightly";

/// The nightly autosave folder: numbered recovery copies of the open package. Only the unix
/// symlink test reads it, so a Windows test build would have no caller (`-D dead-code`).
#[cfg(all(test, unix))]
pub fn recovery_dir(root: &Path) -> Result<PathBuf, String> {
    private_subdirectory(root, RECOVERY, "recovery folder")
}

#[cfg(test)]
#[path = "profile_root.test.rs"]
mod tests;
