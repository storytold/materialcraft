use serde::{Deserialize, Serialize};

use materialcraft_engine::{CameraPrefs, HistoryPrefs, LibraryCachePrefs};

/// Preferences (Edit ▸ Preferences)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// UI language code (`i18n::LANGUAGES`)
    pub language: String,
    pub ui_scale: f32,
    /// Largest node output while editing (0 = full size)
    pub preview_size: u32,
    /// Minutes between autosaves of a modified package (0 = off)
    pub autosave_minutes: u32,
    pub history: HistoryPrefs,
    pub library_cache: LibraryCachePrefs,
    /// Show node thumbnails in the graph
    pub thumbnails: bool,
    pub snap_to_grid: bool,
    /// Library tiles: large icons or a list
    pub library_list: bool,
    /// Open the home screen when the app starts
    pub show_home: bool,
    /// 3D view camera: pan limit and speed
    pub camera: CameraPrefs,
    /// The 3D view a reset and a new document start from (3D view ▸ Scene ▸ Save Current State As Default)
    pub default_view3d: Option<materialcraft_engine::View3d>,
    /// How many packages File ▸ Recent packages lists
    pub recent_limit: usize,
    /// Scrolling up zooms out in the graph, the 2D view and the 3D view
    pub invert_zoom: bool,
    /// Autosaved copies kept for each package, newest first
    pub autosave_versions: u32,
    /// Tab over the graph opens the node search
    pub tab_opens_node_search: bool,
    /// A wire dropped on empty canvas opens the node search to connect a new node
    pub wire_drop_opens_node_search: bool,
    /// Opening a material graph shows its outputs in the 3D view
    pub view_outputs_on_open: bool,
    /// Windows ▸ Main toolbar
    pub show_toolbar: bool,
    /// The app's own theme, mode and size choice over the app's defaults (`appearance::root_env`). A file without it
    /// loads the fresh-install choice, which follows the defaults; the old `brightness` value is the migration's.
    pub appearance: craft_theme::AppChoice,
}

impl Prefs {
    /// Most packages the recent list can hold
    pub const MAX_RECENT: usize = 30;
    /// Most autosaved copies kept for a package
    pub const MAX_AUTOSAVE_VERSIONS: u32 = 50;
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            language: crate::i18n::os_language().into(),
            ui_scale: 1.0,
            preview_size: 512,
            autosave_minutes: 5,
            history: HistoryPrefs::default(),
            library_cache: LibraryCachePrefs::default(),
            thumbnails: true,
            snap_to_grid: true,
            library_list: false,
            show_home: true,
            camera: CameraPrefs::default(),
            default_view3d: None,
            recent_limit: 10,
            invert_zoom: false,
            autosave_versions: 6,
            tab_opens_node_search: true,
            wire_drop_opens_node_search: true,
            view_outputs_on_open: true,
            show_toolbar: true,
            appearance: craft_theme::AppChoice::new(None, None, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_persist_and_default_when_missing() {
        // Missing preferences use their defaults
        let old: Prefs = serde_json::from_value(serde_json::json!({"uiScale": 1.25})).unwrap();

        assert_eq!(old.camera, CameraPrefs::default());
        assert_eq!(old.ui_scale, 1.25);
        assert!(old.snap_to_grid);
        assert_eq!(old.history, HistoryPrefs::default());
        assert_eq!(old.library_cache, LibraryCachePrefs::default());
        assert_eq!((old.recent_limit, old.autosave_versions, old.invert_zoom), (10, 6, false));
        assert!(old.tab_opens_node_search && old.wire_drop_opens_node_search && old.view_outputs_on_open && old.show_toolbar);
        assert_eq!(old.appearance, Prefs::default().appearance);

        let (theme, mode) = crate::appearance::brightness_choices()[3].clone();
        let appearance = craft_theme::AppChoice::new(Some(theme), Some(mode), Some(craft_theme::Size::Comfortable));
        assert_ne!(appearance, old.appearance);
        let p = Prefs {
            camera: CameraPrefs { pan_limit: 8.0, pan_speed: 0.5 },
            history: HistoryPrefs { undo_limit: 500 },
            library_cache: LibraryCachePrefs { disk_mib: 512, max_previews: 12000 },
            recent_limit: 4,
            invert_zoom: true,
            appearance,
            ..Prefs::default()
        };

        let v = serde_json::to_value(&p).unwrap();

        assert_eq!(v["camera"], serde_json::json!({"panLimit": 8.0, "panSpeed": 0.5}));
        assert_eq!(v["history"]["undoLimit"], 500);
        assert_eq!(v["libraryCache"], serde_json::json!({"diskMib": 512, "maxPreviews": 12000}));
        assert_eq!((v["recentLimit"].clone(), v["invertZoom"].clone()), (serde_json::json!(4), serde_json::json!(true)));
        assert!(v.get("appearance").is_some(), "the theme choice is saved under `appearance`");
        assert_eq!(serde_json::from_value::<Prefs>(v).unwrap(), p);
    }

    #[test]
    fn the_default_3d_view_persists() {
        assert_eq!(Prefs::default().default_view3d, None);

        let view = materialcraft_engine::View3d { scene: "sphere".into(), focal_length: 50.0, show_grid: true, ..Default::default() };
        let saved = serde_json::to_value(Prefs { default_view3d: Some(view.clone()), ..Prefs::default() }).unwrap();
        assert_eq!(saved["defaultView3d"]["focalLength"], 50.0);
        assert_eq!(serde_json::from_value::<Prefs>(saved).unwrap().default_view3d, Some(view));
    }
}
