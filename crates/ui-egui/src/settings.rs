//! `UiState` as a stored settings document, and the import of the eframe-era file.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use craft_settings::{LegacyFile, SectionError};
use serde::Deserialize;

use crate::UiState;

impl craft_settings::Settings for UiState {
    const APP_ID: &'static str = "materialcraft";
    const SCHEMA_VERSION: u32 = 1;
}

/// The key eframe stored the whole `UiState` under: an entry of the desktop's RON map, the browser's item name.
const EFRAME_KEY: &str = "ui";

/// The old eframe storage file (or the browser's localStorage item, keyed by `path`), read once and never removed.
/// The desktop file is eframe's RON map of keys to RON text; the browser item is the `"ui"` entry's text itself.
pub(crate) fn eframe_legacy(path: PathBuf) -> LegacyFile<UiState> {
    LegacyFile { path, parse: parse_eframe }
}

/// The old brightness levels, as eframe-era files wrote them. An unknown value (a newer file) reads as the default,
/// which serde wants last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Brightness {
    Dark,
    MediumLight,
    Light,
    #[default]
    #[serde(other)]
    MediumDark,
}

impl Brightness {
    /// Its place in `appearance::brightness_choices()`, which lists the levels darkest first.
    fn index(self) -> usize {
        match self {
            Brightness::Dark => 0,
            Brightness::MediumDark => 1,
            Brightness::MediumLight => 2,
            Brightness::Light => 3,
        }
    }
}

/// The `Prefs` fields eframe-era files held that `Prefs` no longer has. The accent is not read: the theme carries it.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct LegacyPrefs {
    brightness: Option<Brightness>,
    node_shortcuts: BTreeMap<String, String>,
}

/// The part of the old `UiState` that `UiState` cannot read itself.
#[derive(Default, Deserialize)]
#[serde(default)]
struct LegacyUi {
    prefs: LegacyPrefs,
}

/// Reads the `"ui"` entry (from eframe's RON map, or the browser item that is the entry) and parses it as `UiState`,
/// carrying the removed preferences over: the brightness becomes the appearance choice and the node shortcuts become
/// command overrides.
fn parse_eframe(bytes: &[u8]) -> Result<(UiState, Vec<SectionError>), String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let map = ron::from_str::<HashMap<String, String>>(text).ok();
    let entry: &str = match map.as_ref().map(|map| map.get(EFRAME_KEY)) {
        Some(Some(entry)) => entry,
        Some(None) => return Err(format!("no \"{EFRAME_KEY}\" entry")),
        None => text,
    };
    let mut ui: UiState = ron::from_str(entry).map_err(|error| error.to_string())?;
    let legacy: LegacyUi = ron::from_str(entry).map_err(|error| error.to_string())?;
    apply_legacy_prefs(&mut ui, legacy.prefs);
    Ok((ui, Vec::new()))
}

/// The one mapping of the removed preferences onto their replacements.
fn apply_legacy_prefs(ui: &mut UiState, legacy: LegacyPrefs) {
    let brightness = legacy.brightness.unwrap_or_default();
    if let Some((theme, mode)) = crate::appearance::brightness_choices().get(brightness.index()).cloned() {
        ui.prefs.appearance = craft_theme::AppChoice::new(Some(theme), Some(mode), ui.prefs.appearance.size);
    }
    ui.shortcut_overrides = crate::node_shortcuts::migrate(&legacy.node_shortcuts, craft_keymap::Platform::current());
}

// `craft_settings::testing` exists only off the browser.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use craft_settings::testing::MemoryFiles;
    use craft_settings::{Location, OpenOptions, SettingsStore, Source};
    use std::path::Path;
    use std::sync::Arc;

    /// An eframe `app.ron` whose `"ui"` entry holds `prefs` (RON text) and nothing else.
    fn eframe_file(prefs: &str) -> String {
        let ui = format!("(prefs:({prefs}))");
        let map: HashMap<String, String> = [(EFRAME_KEY.to_owned(), ui)].into();
        ron::to_string(&map).unwrap()
    }

    fn open(files: &Arc<MemoryFiles>, legacy: &Path) -> (SettingsStore<UiState>, craft_settings::LoadReport) {
        // `Location::Memory` reads and writes nothing; a folder over in-memory files exercises the TOML document.
        let options =
            OpenOptions::new(Location::Dir(PathBuf::from("/settings"))).files(files.clone()).legacy(vec![eframe_legacy(legacy.to_path_buf())]);
        SettingsStore::open_with(options)
    }

    #[test]
    fn ui_state_round_trips_through_the_settings_store_and_the_old_eframe_file_imports_once() {
        let files = Arc::new(MemoryFiles::new());
        let legacy = Path::new("/eframe/app.ron");
        craft_settings::SettingsFiles::write(&*files, legacy, &eframe_file("brightness:light,accent:orange,nodeShortcuts:{\"blur\":\"Ctrl+Shift+B\"}"))
            .unwrap();

        let (mut store, report) = open(&files, legacy);
        assert_eq!(report.source, Source::Legacy(legacy.to_path_buf()));
        let imported = store.get().clone();
        let mut expected = UiState::default();
        apply_legacy_prefs(
            &mut expected,
            LegacyPrefs { brightness: Some(Brightness::Light), node_shortcuts: [("blur".to_owned(), "Ctrl+Shift+B".to_owned())].into() },
        );
        assert_eq!(imported.prefs.appearance, expected.prefs.appearance, "the brightness became the appearance");
        assert_eq!(imported.shortcut_overrides, expected.shortcut_overrides, "the node map became overrides");
        assert_ne!(imported.prefs.appearance, UiState::default().prefs.appearance);

        // Every kind of field TOML has to hold: an `Option` inside a table, maps, sets, the dock layout.
        store.update(|ui| {
            ui.prefs.default_view3d = Some(materialcraft_engine::View3d { focal_length: 50.0, ..Default::default() });
            ui.prefs.ui_scale = 1.25;
            ui.recent = vec!["/a.sbs".into(), "/b.sbs".into()];
            ui.library_expanded.insert("Filters".into());
            ui.library_favorites.push("blur".into());
            ui.preferences_page = "library".into();
            ui.dock.toggle(crate::docking::Panel::Explorer);
        });
        store.flush().unwrap();
        let saved = store.get().clone();

        let (reopened, report) = open(&files, legacy);
        assert_eq!(report.source, Source::File, "the old file is not imported again");
        assert_eq!(*reopened.get(), saved);
        assert!(files.text(legacy).is_some(), "the old file is never removed");
    }

    #[test]
    fn an_old_brightness_value_reads_as_its_theme_and_an_unknown_one_as_the_default() {
        let parsed = |brightness: &str| parse_eframe(eframe_file(&format!("brightness:{brightness}")).as_bytes()).unwrap().0.prefs.appearance;
        let choices = crate::appearance::brightness_choices();
        let levels = ["dark", "mediumDark", "mediumLight", "light"];
        let read: Vec<_> = levels.iter().map(|level| parsed(level)).collect();
        for (appearance, (theme, mode)) in read.iter().zip(choices.iter()) {
            assert_eq!((appearance.theme.as_ref(), appearance.mode.as_ref()), (Some(theme), Some(mode)));
        }
        assert_eq!(parsed("sepia"), parsed("mediumDark"), "an unknown level reads as the default");
        assert_eq!(parse_eframe(eframe_file("").as_bytes()).unwrap().0.prefs.appearance, parsed("mediumDark"), "a missing level too");
    }
}
