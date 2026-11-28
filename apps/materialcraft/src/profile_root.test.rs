use super::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    root: PathBuf,
    env: BTreeMap<String, OsString>,
    exe: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = loop {
            let candidate = std::env::temp_dir().join(format!("materialcraft-profile-policy-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            match std::fs::create_dir(&candidate) {
                Ok(()) => break std::fs::canonicalize(candidate).unwrap(),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create owned fixture: {error}"),
            }
        };
        let home = root.join("home");
        let exe = root.join("install/materialcraft");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"fixture executable, never run").unwrap();
        let env = [("HOME", home), ("APPDATA", root.join("roaming")), ("LOCALAPPDATA", root.join("local"))]
            .into_iter()
            .map(|(key, value)| (key.into(), value.into_os_string()))
            .collect();
        Self { root, env, exe }
    }

    fn get(&self, key: &str) -> Option<OsString> {
        self.env.get(key).cloned()
    }

    fn check(&self, platform: Platform, root: &Path) -> Result<PathBuf, String> {
        validate_for(platform, root, |key| self.get(key), Some(&self.exe), probe_owned)
    }

    fn home(&self) -> PathBuf {
        PathBuf::from(self.get("HOME").unwrap())
    }
}

/// The production probe against an owned directory.
fn probe_owned(dir: &Path) -> std::io::Result<()> {
    probe_writable(dir)
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only the directory successfully created by this fixture is owned for removal.
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn the_stable_documents_folder_holding_autosaves_is_fenced_everywhere() {
    let fixture = Fixture::new();
    let documents = fixture.home().join("Documents/MaterialCraft");
    std::fs::create_dir_all(documents.join("autosave")).unwrap();
    std::fs::write(documents.join("autosave").join("1.mcraft"), b"stable autosave bytes").unwrap();
    for platform in [Platform::Windows, Platform::Linux, Platform::Mac] {
        for candidate in [documents.clone(), documents.join("nightly"), documents.join("autosave/nightly")] {
            let existed = candidate.exists();
            let probed = std::cell::Cell::new(false);
            let result = validate_for(
                platform,
                &candidate,
                |key| fixture.get(key),
                Some(&fixture.exe),
                |_| {
                    probed.set(true);
                    Ok(())
                },
            );
            assert!(result.unwrap_err().contains("overlap"));
            assert!(!probed.get(), "reject stable data before the writable probe");
            // The stable folder itself exists (the fixture made it); validation must never create
            // the ones below it.
            assert_eq!(candidate.exists(), existed, "{}", candidate.display());
        }
    }
    assert_eq!(std::fs::read(documents.join("autosave").join("1.mcraft")).unwrap(), b"stable autosave bytes");
}

#[test]
fn portable_settings_from_another_install_are_refused_before_any_probe() {
    let fixture = Fixture::new();
    let stable_install = fixture.root.join("stable portable install");
    let stable_data = stable_install.join("MaterialCraftData");
    std::fs::create_dir_all(&stable_data).unwrap();
    std::fs::write(stable_install.join(PORTABLE_MARKER), b"owned portable fixture").unwrap();
    std::fs::write(stable_data.join("app.ron"), b"stable settings must not change").unwrap();
    for platform in [Platform::Windows, Platform::Linux, Platform::Mac] {
        for candidate in [&stable_data, &stable_data.join("nested nightly")] {
            let probed = std::cell::Cell::new(false);
            let result = validate_for(
                platform,
                candidate,
                |key| fixture.get(key),
                Some(&fixture.exe),
                |_| {
                    probed.set(true);
                    Ok(())
                },
            );
            assert!(result.is_err(), "a different portable install is not a private Nightly profile");
            assert!(!probed.get(), "reject portable data before a writable probe");
            assert!(!stable_data.join("nested nightly").exists());
            assert_eq!(std::fs::read(stable_data.join("app.ron")).unwrap(), b"stable settings must not change");
            assert_eq!(std::fs::read_dir(&stable_data).unwrap().count(), 1);
        }
    }
    let private = fixture.root.join("MaterialCraftData-backup-name/nightly");
    assert!(fixture.check(Platform::Linux, &private).is_ok(), "a merely similar name is not a portable settings component");
}

#[cfg(unix)]
#[test]
fn recovery_aliases_cannot_redirect_to_stable_missing_or_profile_root_data() {
    let fixture = Fixture::new();
    let stable = fixture.home().join("Documents/MaterialCraft/autosave");
    std::fs::create_dir_all(&stable).unwrap();
    std::fs::write(stable.join("stable-autosave"), b"stable recovery bytes").unwrap();
    for (index, target) in [stable.clone(), fixture.root.join("missing recovery target")].into_iter().enumerate() {
        let private = fixture.root.join(format!("nightly-{index}"));
        std::fs::create_dir(&private).unwrap();
        std::os::unix::fs::symlink(&target, private.join(RECOVERY)).unwrap();
        let probed = std::cell::Cell::new(false);
        let result = validate_for(
            Platform::Linux,
            &private,
            |key| fixture.get(key),
            Some(&fixture.exe),
            |_| {
                probed.set(true);
                Ok(())
            },
        );
        assert!(result.is_err(), "Recovery must not escape the private profile or follow a dangling alias");
        assert!(!probed.get());
    }
    let private = fixture.root.join("nightly-self");
    std::fs::create_dir(&private).unwrap();
    std::os::unix::fs::symlink(&private, private.join(RECOVERY)).unwrap();
    assert!(fixture.check(Platform::Linux, &private).is_err(), "Recovery cannot treat all profile settings as recovery entries");
    assert_eq!(std::fs::read(stable.join("stable-autosave")).unwrap(), b"stable recovery bytes");
    assert_eq!(std::fs::read_dir(&stable).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn recovery_alias_to_a_private_subfolder_keeps_the_profile_usable() {
    let fixture = Fixture::new();
    let private = fixture.root.join("nightly-private");
    let recovery = private.join("private-recovery");
    std::fs::create_dir_all(&recovery).unwrap();
    std::os::unix::fs::symlink(&recovery, private.join(RECOVERY)).unwrap();
    assert!(fixture.check(Platform::Linux, &private).is_ok());
    assert_eq!(std::fs::canonicalize(private.join(RECOVERY)).unwrap(), recovery);
    assert_eq!(recovery_dir(&std::fs::canonicalize(&private).unwrap()).unwrap(), recovery);
}

#[test]
fn overlap_check_handles_ancestor_descendant_and_prefix_lookalikes() {
    let fixture = Fixture::new();
    let root = fixture.home().join("nightly/profiles/materialcraft");
    for platform in [Platform::Windows, Platform::Linux, Platform::Mac] {
        assert!(overlaps(&root, &root.join(RECOVERY), platform));
        assert!(overlaps(&root.join(RECOVERY), &root, platform));
        assert!(!overlaps(&root, &root.with_file_name("materialcraft-other"), platform));
    }
}

#[test]
fn linux_fences_app_xdg_subtrees_but_allows_launcher_sibling() {
    let mut fixture = Fixture::new();
    for key in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"] {
        fixture.env.insert(key.into(), fixture.root.join(key).into_os_string());
    }
    for key in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"] {
        let stable = PathBuf::from(fixture.get(key).unwrap()).join("materialcraft");
        std::fs::create_dir_all(&stable).unwrap();
        std::fs::write(stable.join("stable-sentinel"), b"stable bytes").unwrap();
        for candidate in [stable.clone(), stable.join("nightly"), stable.parent().unwrap().to_path_buf()] {
            assert!(fixture.check(Platform::Linux, &candidate).unwrap_err().contains("overlap"));
        }
        assert_eq!(std::fs::read(stable.join("stable-sentinel")).unwrap(), b"stable bytes");
    }
    let data = PathBuf::from(fixture.get("XDG_DATA_HOME").unwrap());
    let nightly = data.join("ArtCraftLauncher/nightly/profiles/materialcraft");
    assert_eq!(fixture.check(Platform::Linux, &nightly).unwrap(), nightly);
    assert_eq!(std::fs::read_dir(nightly).unwrap().count(), 0);
}

#[test]
fn mac_fences_only_app_library_subtrees_and_the_entire_bundle() {
    let mut fixture = Fixture::new();
    fixture.exe = fixture.root.join("install/MaterialCraft Nightly.app/Contents/MacOS/MaterialCraft");
    std::fs::create_dir_all(fixture.exe.parent().unwrap()).unwrap();
    std::fs::write(&fixture.exe, b"fixture executable, never run").unwrap();
    let library = fixture.home().join("Library");
    for suffix in [
        "Application Support/MaterialCraft",
        "Caches/materialcraft",
        "Caches/ai.storyteller.materialcraft",
        "Preferences/MaterialCraft",
        "Saved Application State/ai.storyteller.materialcraft.savedState",
    ] {
        let stable = library.join(suffix);
        assert!(fixture.check(Platform::Mac, &stable.join("nightly")).unwrap_err().contains("overlap"));
        assert!(!stable.exists());
    }
    let inside_resources = fixture.exe.parent().unwrap().parent().unwrap().join("Resources/profile");
    assert!(fixture.check(Platform::Mac, &inside_resources).unwrap_err().contains("overlap"));
    assert!(!inside_resources.exists());
    let valid = library.join("Application Support/ArtCraftLauncher/nightly/profiles/materialcraft");
    assert_eq!(fixture.check(Platform::Mac, &valid).unwrap(), valid);
}

#[test]
fn windows_roaming_local_and_install_fences_remain_active() {
    let fixture = Fixture::new();
    for key in ["APPDATA", "LOCALAPPDATA"] {
        let stable = PathBuf::from(fixture.get(key).unwrap()).join("MaterialCraft");
        assert!(fixture.check(Platform::Windows, &stable.join("nightly")).unwrap_err().contains("overlap"));
        assert!(!stable.exists());
    }
    assert!(fixture.check(Platform::Windows, &fixture.exe.parent().unwrap().join("profile")).is_err());
    assert!(fixture.check(Platform::Windows, &fixture.root.join("Launcher/profiles/materialcraft")).is_ok());
}

#[test]
fn windows_default_msi_install_folders_are_fenced() {
    let mut fixture = Fixture::new();
    for key in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
        let base = fixture.root.join(format!("machine {key}"));
        fixture.env.insert(key.into(), base.clone().into_os_string());
        let machine = base.join("MaterialCraft");
        assert!(fixture.check(Platform::Windows, &machine.join("nightly")).unwrap_err().contains("overlap"));
        assert!(!machine.exists());
    }
    let local = PathBuf::from(fixture.get("LOCALAPPDATA").unwrap());
    let sibling = local.join("Programs").join("ArtCraft").join("ArtCraftLauncher").join("nightly").join("materialcraft");
    assert!(fixture.check(Platform::Windows, &sibling).is_ok(), "the Launcher's own Programs sibling is not MaterialCraft's install");
}

#[test]
fn folders_under_a_stable_portable_copy_are_refused_before_any_probe() {
    let fixture = Fixture::new();
    let portable = fixture.root.join("D/MC");
    std::fs::create_dir_all(&portable).unwrap();
    std::fs::write(portable.join(PORTABLE_MARKER), b"stable portable copy").unwrap();
    std::fs::write(portable.join("materialcraft.exe"), b"stable portable executable, never run").unwrap();
    for platform in [Platform::Windows, Platform::Linux, Platform::Mac] {
        for candidate in [portable.join("nightly"), portable.join("deeper/nightly")] {
            let probed = std::cell::Cell::new(false);
            let result = validate_for(
                platform,
                &candidate,
                |key| fixture.get(key),
                Some(&fixture.exe),
                |_| {
                    probed.set(true);
                    Ok(())
                },
            );
            assert!(result.unwrap_err().contains("portable"));
            assert!(!probed.get());
            assert!(!candidate.exists());
        }
    }
    // A portable.txt that no MaterialCraft executable sits beside is someone else's file.
    let unrelated = fixture.root.join("user folder");
    std::fs::create_dir_all(&unrelated).unwrap();
    std::fs::write(unrelated.join(PORTABLE_MARKER), b"another app's marker").unwrap();
    assert!(fixture.check(Platform::Windows, &unrelated.join("Launcher/nightly/materialcraft")).is_ok());
}

#[test]
fn linux_fences_the_stable_flatpak_data_folder() {
    let fixture = Fixture::new();
    let flatpak = fixture.home().join(".var/app/ai.storyteller.materialcraft");
    for candidate in [flatpak.clone(), flatpak.join("data/nightly")] {
        assert!(fixture.check(Platform::Linux, &candidate).unwrap_err().contains("overlap"));
    }
    assert!(!flatpak.exists());
    assert!(fixture.check(Platform::Linux, &fixture.home().join(".var/app/ai.storyteller.launcher/nightly")).is_ok());
}

#[cfg(unix)]
#[test]
fn any_profile_entry_alias_must_stay_inside_the_profile() {
    let fixture = Fixture::new();
    let stable = fixture.home().join(".local/share/materialcraft/app.ron");
    std::fs::create_dir_all(stable.parent().unwrap()).unwrap();
    std::fs::write(&stable, b"stable layout bytes").unwrap();
    for name in ["app.ron", "cache", RECOVERY] {
        let private = fixture.root.join(format!("nightly-{name}"));
        std::fs::create_dir(&private).unwrap();
        std::os::unix::fs::symlink(&stable, private.join(name)).unwrap();
        let probed = std::cell::Cell::new(false);
        let result = validate_for(
            Platform::Linux,
            &private,
            |key| fixture.get(key),
            Some(&fixture.exe),
            |_| {
                probed.set(true);
                Ok(())
            },
        );
        assert!(result.is_err(), "{name} must not lead outside the profile");
        assert!(!probed.get());
    }
    let private = fixture.root.join("nightly-inside");
    std::fs::create_dir_all(private.join("real-cache")).unwrap();
    std::fs::write(private.join("app.ron"), b"{}").unwrap();
    std::os::unix::fs::symlink(private.join("real-cache"), private.join("cache")).unwrap();
    assert!(fixture.check(Platform::Linux, &private).is_ok(), "regular files and links that stay inside are fine");
    assert_eq!(std::fs::read(&stable).unwrap(), b"stable layout bytes");
}

#[test]
fn ambiguous_environment_and_non_native_sandbox_are_refused_before_creation() {
    let mut fixture = Fixture::new();
    let candidate = fixture.home().join("nightly/profiles/materialcraft");
    fixture.env.insert("XDG_CACHE_HOME".into(), "relative/cache".into());
    assert!(fixture.check(Platform::Linux, &candidate).unwrap_err().contains("absolute"));
    fixture.env.remove("XDG_CACHE_HOME");
    fixture.env.insert("MATERIALCRAFT_CACHE_DIR".into(), fixture.home().join("custom-cache").into_os_string());
    assert!(fixture.check(Platform::Linux, &candidate).unwrap_err().contains("MATERIALCRAFT_CACHE_DIR"));
    fixture.env.remove("MATERIALCRAFT_CACHE_DIR");
    fixture.env.insert("MATERIALCRAFT_NO_PREFS".into(), "1".into());
    assert!(fixture.check(Platform::Linux, &candidate).unwrap_err().contains("MATERIALCRAFT_NO_PREFS"));
    fixture.env.remove("MATERIALCRAFT_NO_PREFS");
    // The stable build accepts this variable; a nightly profile must not be moved by it.
    fixture.env.insert("MATERIALCRAFT_DATA_DIR".into(), fixture.home().join("custom-data").into_os_string());
    assert!(fixture.check(Platform::Linux, &candidate).unwrap_err().contains("MATERIALCRAFT_DATA_DIR"));
    fixture.env.remove("MATERIALCRAFT_DATA_DIR");
    for inherited in ["ai.storyteller.materialcraft", "org.freedesktop.other"] {
        fixture.env.insert("FLATPAK_ID".into(), inherited.into());
        assert!(fixture.check(Platform::Linux, &candidate).unwrap_err().contains("Flatpak"), "{inherited}");
        fixture.env.remove("FLATPAK_ID");
    }
    fixture.env.remove("HOME");
    assert!(fixture.check(Platform::Linux, &candidate).unwrap_err().contains("HOME"));
    assert!(!candidate.exists());
}

/// The nightly bundle runs under an app id of its own, so its sandbox data folder is this channel's
/// data root; that one instance is accepted, and its profile stays inside it.
#[test]
fn the_nightly_flatpaks_own_instance_accepts_its_own_profile() {
    let mut fixture = Fixture::new();
    fixture.env.insert("FLATPAK_ID".into(), NIGHTLY_FLATPAK_ID.into());
    let private = fixture.home().join(".var/app/ai.storyteller.materialcraft.nightly/data/materialcraft-nightly");
    assert_eq!(fixture.check(Platform::Linux, &private).unwrap(), private);
    // Stable's Flatpak folder is still fenced even inside the nightly instance.
    let stable = fixture.home().join(".var/app/ai.storyteller.materialcraft");
    assert!(fixture.check(Platform::Linux, &stable).unwrap_err().contains("overlap"));
}

#[test]
fn non_directory_and_failed_write_probe_never_fall_back_to_stable() {
    let fixture = Fixture::new();
    let file = fixture.home().join("not-a-directory");
    std::fs::write(&file, b"do not replace me").unwrap();
    assert!(fixture.check(Platform::Linux, &file).is_err());
    assert_eq!(std::fs::read(file).unwrap(), b"do not replace me");
    let candidate = fixture.home().join("nightly");
    let calls = std::cell::Cell::new(0);
    let result = validate_for(
        Platform::Linux,
        &candidate,
        |key| fixture.get(key),
        Some(&fixture.exe),
        |_| {
            calls.set(calls.get() + 1);
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "injected write refusal"))
        },
    );
    assert!(result.unwrap_err().contains("not writable"));
    assert_eq!(calls.get(), 1);
    assert!(!fixture.home().join(".local/share/materialcraft").exists());
}

#[cfg(unix)]
#[test]
fn symlink_and_dangling_aliases_cannot_create_or_modify_stable_data() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let stable = fixture.home().join(".local/share/materialcraft");
    std::fs::create_dir_all(&stable).unwrap();
    std::fs::write(stable.join("sentinel"), b"untouched").unwrap();
    let alias = fixture.home().join("alias");
    symlink(&stable, &alias).unwrap();
    assert!(fixture.check(Platform::Linux, &alias.join("nightly")).unwrap_err().contains("overlap"));
    let missing = fixture.home().join("missing-target");
    let dangling = fixture.home().join("dangling");
    symlink(&missing, &dangling).unwrap();
    assert!(fixture.check(Platform::Linux, &dangling.join("nightly")).is_err());
    assert!(!missing.exists());
    assert!(!stable.join("nightly").exists());
    assert_eq!(std::fs::read(stable.join("sentinel")).unwrap(), b"untouched");
}
