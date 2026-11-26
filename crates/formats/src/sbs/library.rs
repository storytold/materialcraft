//! Resolve the portable SBS library alias against the application's asset directory.

include!(concat!(env!("OUT_DIR"), "/sbs_assets.rs"));

/// Shared fallback for missing SBS font resources; supplied by the optional craft-fonts build input.
pub const FALLBACK_FONT: &str = "sbs://resources/LiberationMono-Regular.ttf";

fn is_shared_font_asset(relative: &str) -> bool {
    matches!(relative, "resources/LiberationMono-Regular.ttf" | "resources/OFL-LiberationMono.txt")
}

/// Validate alias paths before joining a filesystem root. No package may escape the library.
pub fn relative_path(uri: &str) -> Result<&str, String> {
    craft_sbs::uri::relative_path(uri)
}

/// Resolve a package-relative resource or dependency without leaving the library root.
pub fn resolve(owner: &str, reference: &str) -> Result<String, String> {
    craft_sbs::uri::resolve(owner, reference)
}

/// The native asset root may be configured for installations with a separate data directory.
#[cfg(not(target_arch = "wasm32"))]
pub fn root() -> std::path::PathBuf {
    if let Some(root) = std::env::var_os("MATERIALCRAFT_SBS_DIR") {
        return root.into();
    }

    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
        && parent.join("assets/sbs").is_dir()
    {
        return parent.join("assets/sbs");
    }

    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sbs")
}

/// A canonical library location, including a new filename beneath an existing library folder.
#[cfg(not(target_arch = "wasm32"))]
pub fn contains_file(path: &std::path::Path) -> bool {
    let Ok(root) = root().canonicalize() else { return false };
    if let Ok(file) = path.canonicalize() {
        return file.starts_with(root);
    }

    path.parent().and_then(|parent| parent.canonicalize().ok()).is_some_and(|parent| parent.starts_with(root))
}

/// Read a validated library alias from the configured directory or the embedded fallback.
pub fn read(uri: &str) -> Result<Vec<u8>, String> {
    let relative = relative_path(uri)?;

    // These assets live in craft-fonts, so a source checkout's SBS directory cannot supply them
    if is_shared_font_asset(relative) {
        return read_embedded(relative).map_err(|error| format!("{error}; build with CRAFT_FONTS_DIR containing Liberation Mono Regular"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let root = root();
        if root.exists() || std::env::var_os("MATERIALCRAFT_SBS_DIR").is_some() {
            let root = root.canonicalize().map_err(|error| error.to_string())?;
            let file = root.join(relative).canonicalize().map_err(|error| format!("Missing library asset {uri}: {error}"))?;
            if !file.starts_with(&root) {
                return Err("SBS library symlink escapes its asset directory".into());
            }

            return std::fs::read(file).map_err(|error| error.to_string());
        }
    }

    read_embedded(relative)
}

/// Library files that the browser build downloads from beside the wasm instead of embedding
/// them, as paths below the library root (`resources/…`). Empty on native builds.
pub fn external_assets() -> &'static [&'static str] {
    EXTERNAL_ASSETS
}

/// The largest library asset read into memory, embedded or downloaded
const MAX_ASSET_BYTES: usize = 256 << 20;

/// The downloaded [`external_assets`], filled once by the web app's startup.
static DOWNLOADED: std::sync::RwLock<std::collections::BTreeMap<String, Vec<u8>>> = std::sync::RwLock::new(std::collections::BTreeMap::new());

/// Supply the bytes of one downloaded [`external_assets`] file.
pub fn install(relative: &str, bytes: Vec<u8>) -> Result<(), String> {
    if !EXTERNAL_ASSETS.contains(&relative) {
        return Err(format!("sbs://{relative} is not a downloadable library asset"));
    }

    if bytes.len() > MAX_ASSET_BYTES {
        return Err("downloaded library asset exceeds 256 MiB".into());
    }

    DOWNLOADED.write().unwrap_or_else(std::sync::PoisonError::into_inner).insert(relative.into(), bytes);

    Ok(())
}

fn read_downloaded(relative: &str) -> Result<Vec<u8>, String> {
    if let Some(bytes) = DOWNLOADED.read().unwrap_or_else(std::sync::PoisonError::into_inner).get(relative) {
        return Ok(bytes.clone());
    }

    if EXTERNAL_ASSETS.contains(&relative) {
        return Err(format!("Library asset sbs://{relative} has not downloaded"));
    }

    Err(format!("Missing library asset sbs://{relative}"))
}

/// Compressed assets keep installed and browser builds independent of the source checkout.
fn read_embedded(relative: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let Ok(index) = EMBEDDED_ASSETS.binary_search_by_key(&relative, |(name, _, _)| *name) else {
        return read_downloaded(relative);
    };

    let (_, compressed, size) = EMBEDDED_ASSETS.get(index).ok_or("invalid bundled asset index")?;
    if *size > MAX_ASSET_BYTES {
        return Err("bundled asset exceeds 256 MiB".into());
    }

    let mut bytes = Vec::new();
    flate2::read::DeflateDecoder::new(*compressed).take(*size as u64 + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() != *size {
        return Err("bundled asset has an invalid decompressed length".into());
    }

    Ok(bytes)
}

/// Bounds for scanning the library folder: how deep, and how many packages
#[cfg(not(target_arch = "wasm32"))]
const MAX_DEPTH: usize = 6;
#[cfg(not(target_arch = "wasm32"))]
const MAX_FILES: usize = 2048;

/// Every SBS package in the library as `sbs://` paths, sorted: the files under the asset folder
/// (resource folders skipped), or the bundled packages when there is no folder
pub fn files() -> Vec<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let root = root();
        if root.is_dir() {
            let mut found = Vec::new();
            scan(&root, "", 0, &mut found);
            found.sort();
            return found;
        }
    }
    BUNDLED.iter().map(|uri| uri.to_string()).collect()
}

/// Discover packages in deterministic order within the depth and file limits.
#[cfg(not(target_arch = "wasm32"))]
fn scan(directory: &std::path::Path, prefix: &str, depth: usize, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if found.len() >= MAX_FILES {
            return;
        }

        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        let Ok(kind) = entry.file_type() else { continue };
        let uri = format!("sbs://{prefix}{name}");

        // Symlinks are skipped: `read` refuses files that leave the asset folder anyway
        if kind.is_dir() && depth < MAX_DEPTH && !name.ends_with(".resources") && !name.starts_with('.') {
            scan(&entry.path(), &format!("{prefix}{name}/"), depth + 1, found);
        } else if kind.is_file() && name.to_ascii_lowercase().ends_with(".sbs") && relative_path(&uri).is_ok() {
            found.push(uri);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The shipped fallback preserves the exact SBS and resource bytes.
    #[test]
    fn embedded_library_contains_every_disk_asset() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sbs");
        for (relative, _, _) in EMBEDDED_ASSETS {
            if is_shared_font_asset(relative) {
                assert_eq!(read(&format!("sbs://{relative}")).unwrap(), read_embedded(relative).unwrap());

                continue;
            }

            assert_eq!(read_embedded(relative).unwrap(), std::fs::read(root.join(relative)).unwrap(), "{relative}");
        }
    }

    /// Only the listed browser downloads are accepted, and native builds embed everything instead.
    #[test]
    fn only_listed_library_assets_can_be_installed() {
        assert!(install("functions.sbs", b"<package/>".to_vec()).is_err());
        assert!(install("../outside.png", Vec::new()).is_err());
        assert!(external_assets().is_empty(), "native builds embed every library asset");
        assert!(read_embedded("resources/not-in-the-library.png").unwrap_err().contains("Missing library asset"));
    }

    /// Discovered packages have unique valid aliases and the embedded fallback is present.
    #[test]
    fn the_library_lists_every_package_once_and_bundles_a_fallback() {
        let files = files();
        for uri in BUNDLED {
            assert!(files.iter().any(|file| file == uri), "{uri} in {files:?}");
            assert!(!read(uri).unwrap().is_empty());
        }
        let mut unique = files.clone();
        unique.dedup();

        assert_eq!(unique, files);
        assert!(files.iter().all(|file| relative_path(file).is_ok() && file.ends_with(".sbs")));
    }
}
