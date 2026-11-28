//! Embeds the Noto Sans Georgian face from the optional craft-fonts build input
//! (storytold/craft-fonts) when `CRAFT_FONTS_DIR` points at a checkout, for native and web builds.
//! Without it `GEORGIAN_FONT` is empty and the UI uses installed system fonts (desktop only).
//! `CRAFT_FONTS_REQUIRED=1` turns an unusable checkout into a build error (releases). No font file
//! is committed here.
#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const MAX_MANIFEST_BYTES: u64 = 1 << 20;
const MAX_FONT_BYTES: u64 = 16 << 20;

fn size_ok(path: &Path, limit: u64) -> Result<()> {
    let len = std::fs::metadata(path)?.len();
    if len == 0 || len > limit {
        return Err(format!("{} is empty or exceeds its size limit", path.display()).into());
    }
    Ok(())
}

/// The checkout-relative `relative`, resolved inside `root` (a manifest must not embed other files).
fn checkout_file(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() || relative.contains(['\\', ':']) || !Path::new(relative).components().all(|part| matches!(part, Component::Normal(_))) {
        return Err("font manifest path is not checkout-relative".into());
    }
    // Registered before canonicalize() can fail, so a file added later reruns the script.
    println!("cargo::rerun-if-changed={}", root.join(relative).display());
    let path = root.join(relative).canonicalize()?;
    if !path.starts_with(root) {
        return Err("font manifest path escapes the checkout".into());
    }
    println!("cargo::rerun-if-changed={}", path.display());
    Ok(path)
}

fn georgian(directory: &Path) -> Result<PathBuf> {
    let root = directory.canonicalize()?;
    let manifest = checkout_file(&root, "fonts/manifest.txt")?;
    size_ok(&manifest, MAX_MANIFEST_BYTES)?;
    let entries = craft_fonts_build::parse_manifest(&std::fs::read_to_string(&manifest)?)?;
    let face = entries
        .iter()
        .find(|face| face.family == "Noto Sans Georgian" && face.style == "Regular")
        .ok_or("Noto Sans Georgian Regular is missing from fonts/manifest.txt (needs craft-fonts a71c20e3 or later)")?;
    let font = checkout_file(&root, &face.file)?;
    size_ok(&font, MAX_FONT_BYTES)?;
    let bytes = std::fs::read(&font)?;
    if !bytes.starts_with(b"\0\x01\0\0") || format!("{:x}", Sha256::digest(&bytes)) != face.sha256 {
        return Err("Noto Sans Georgian does not match its catalog checksum or TrueType format".into());
    }
    Ok(font)
}

fn main() {
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_REQUIRED");
    let required = std::env::var_os("CRAFT_FONTS_REQUIRED").is_some_and(|v| v == "1");
    let selected = match std::env::var_os("CRAFT_FONTS_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => georgian(Path::new(&dir)).map(Some),
        None if required => Err("CRAFT_FONTS_REQUIRED=1 requires CRAFT_FONTS_DIR for the Georgian UI font".into()),
        None => Ok(None),
    };
    let font = match selected {
        Ok(font) => font,
        Err(e) if required => {
            println!("cargo::error=craft-fonts: {e}");
            None
        }
        Err(e) => {
            println!("cargo::warning=building without the Georgian UI font from craft-fonts: {e}");
            None
        }
    };
    let src = match font {
        Some(path) => format!("pub static GEORGIAN_FONT: &[u8] = include_bytes!({:?});\n", path.display().to_string()),
        None => "pub static GEORGIAN_FONT: &[u8] = &[];\n".to_owned(),
    };
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("craft_fonts.rs");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}
