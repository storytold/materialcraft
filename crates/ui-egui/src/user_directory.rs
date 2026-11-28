/// `Documents/MaterialCraft/<sub>` in the user's home folder, `$MATERIALCRAFT_DATA_DIR/<sub>` when
/// overridden, or the Nightly profile's private folder instead of either.
#[cfg(not(target_arch = "wasm32"))]
pub fn user_dir(sub: &str) -> std::path::PathBuf {
    // A Nightly keeps every autosaved copy inside its own profile, never in the stable Documents
    // folder the unqualified app owns.
    if let Some(root) = crate::data_root::root() {
        return match sub {
            "autosave" => root.join(crate::data_root::RECOVERY),
            other => root.join(other),
        };
    }
    if let Some(root) = std::env::var_os("MATERIALCRAFT_DATA_DIR").filter(|v| !v.is_empty()) {
        return std::path::PathBuf::from(root).join(sub);
    }
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("Documents").join("MaterialCraft").join(sub)
}
