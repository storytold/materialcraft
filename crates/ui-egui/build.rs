//! Embeds the translation catalogs as `$OUT_DIR/craft_i18n.rs`: a static `CRAFT_I18N: &craft_i18n::Embedded` of
//! (locale, shared catalog, app catalog) for each locale of the craft-i18n checkout `CRAFT_I18N_DIR` that has a
//! MaterialCraft catalog. Unset, the list is empty and the interface shows English.

use std::path::{Path, PathBuf};

/// The app catalog's file name in each locale folder.
const APP_CATALOG: &str = "materialcraft.tsv";

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=CRAFT_I18N_DIR");
    let Some(out) = std::env::var_os("OUT_DIR") else {
        println!("cargo::error=OUT_DIR is unset");
        return;
    };
    let mut rows = String::new();
    if let Some(dir) = std::env::var_os("CRAFT_I18N_DIR").filter(|dir| !dir.is_empty()) {
        let locales = PathBuf::from(dir).join("locales");
        println!("cargo::rerun-if-changed={}", locales.display());
        // shortcut: a locale without its own MaterialCraft catalog is not embedded, even as a region's fallback; embed
        // the fallback parents too once the app offers a regional language.
        let mut found: Vec<PathBuf> =
            std::fs::read_dir(&locales).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|path| path.join(APP_CATALOG).is_file()).collect();
        found.sort();
        for locale in found {
            let code = locale.file_name().and_then(|name| name.to_str());
            let (Some(code), Some(shared), Some(app)) = (code, literal(&locale.join("common.tsv")), literal(&locale.join(APP_CATALOG))) else {
                println!("cargo::warning=CRAFT_I18N_DIR: {} is not a UTF-8 path; its catalogs are not embedded", locale.display());
                continue;
            };
            rows.push_str(&format!("    ({code:?}, {shared}, {app}),\n"));
        }
    }
    let code = format!("/// The embedded catalogs: (locale, shared catalog, app catalog).\nstatic CRAFT_I18N: &craft_i18n::Embedded = &[\n{rows}];\n");
    if let Err(error) = std::fs::write(Path::new(&out).join("craft_i18n.rs"), code) {
        println!("cargo::error=craft_i18n.rs: {error}");
    }
}

/// An expression for the catalog at `path`: its text embedded, or `""` when the locale has none. `None` when the path
/// is not UTF-8, which `include_str!` can't name.
fn literal(path: &Path) -> Option<String> {
    if !path.is_file() {
        return Some("\"\"".to_owned());
    }
    println!("cargo::rerun-if-changed={}", path.display());
    path.to_str().map(|path| format!("include_str!({path:?})"))
}
