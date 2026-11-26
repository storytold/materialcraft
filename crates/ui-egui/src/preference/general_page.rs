//! Preferences ▸ General: recent packages, history, language, appearance, views, autosave and previews

use craft_theme::{AppChoice, ModeChoice, ThemePair};
use craft_ui::contract::Entry;
use craft_ui::patterns::preferences::{GroupSpec, PageSpec, PrefValue, RowKind, RowSpec};
use egui::Ui;

use super::preferences_page::{GENERAL, GENERAL_TITLE};
use super::{Texts, slider, whole};
use crate::i18n::tr;
use crate::state::Prefs;

const RECENT: &str = "general.recent";
const UNDO: &str = "general.undo";
const LANGUAGE: &str = "general.language";
const BRIGHTNESS: &str = "general.brightness";
const SCALE: &str = "general.scale";
const INVERT_ZOOM: &str = "general.invertZoom";
#[cfg(not(target_arch = "wasm32"))]
const AUTOSAVE: &str = "general.autosave";
#[cfg(not(target_arch = "wasm32"))]
const VERSIONS: &str = "general.autosaveVersions";
const PREVIEW: &str = "general.previewSize";
/// The Brightness options' ids: the index of each `appearance::brightness_choices` entry.
const BRIGHTNESS_IDS: [&str; 4] = ["0", "1", "2", "3"];

/// The most undo steps the history slider reaches
const MAX_UNDO_STEPS: f64 = 10_000.0;
/// The longest autosave interval the slider reaches, in minutes
#[cfg(not(target_arch = "wasm32"))]
const MAX_AUTOSAVE_MINUTES: f64 = 60.0;
/// The interface scale the slider spans
const SCALES: std::ops::RangeInclusive<f64> = 0.75..=2.0;

fn recent_range() -> std::ops::RangeInclusive<i64> {
    1..=Prefs::MAX_RECENT as i64
}

#[cfg(not(target_arch = "wasm32"))]
fn versions_range() -> std::ops::RangeInclusive<i64> {
    1..=i64::from(Prefs::MAX_AUTOSAVE_VERSIONS)
}

/// The Brightness entry `appearance` shows: the one equal to its theme and mode, with the app defaults where it
/// chooses nothing; a choice none of the entries equals shows the app-default entry.
fn brightness(appearance: &AppChoice) -> usize {
    let defaults = crate::appearance::app_defaults();
    let choices = crate::appearance::brightness_choices();
    let at = |theme: &ThemePair, mode: ModeChoice| choices.iter().position(|(t, m)| t == theme && *m == mode);
    at(appearance.theme.as_ref().unwrap_or(&defaults.theme), appearance.mode.unwrap_or(defaults.mode))
        .or_else(|| at(&defaults.theme, defaults.mode))
        .unwrap_or(0)
}

pub(super) fn page<'a>(prefs: &'a Prefs, texts: &'a Texts) -> PageSpec<'a> {
    let languages = crate::i18n::LANGUAGES.iter().map(|language| Entry::new(language.code, language.name)).collect();
    let brightness_value = BRIGHTNESS_IDS.get(brightness(&prefs.appearance)).copied().unwrap_or(BRIGHTNESS_IDS[0]);
    let brightness_options = BRIGHTNESS_IDS.iter().zip(crate::appearance::brightness_labels()).map(|(id, label)| Entry::new(*id, label)).collect();
    let preview_value = crate::preview_resolution::SIZES
        .iter()
        .position(|size| *size == prefs.preview_size)
        .and_then(|at| texts.preview_ids.get(at))
        .map_or("", String::as_str);
    let preview_options = texts.preview_ids.iter().zip(&texts.preview_labels).map(|(id, label)| Entry::new(id.as_str(), label.clone())).collect();

    let mut groups = vec![
        GroupSpec::new(
            Some(tr("Recent documents")),
            vec![RowSpec::new(RECENT, tr("Recent packages list contains"), RowKind::int(prefs.recent_limit as i64, recent_range()).unit_text(tr("packages")))],
        ),
        GroupSpec::new(Some(tr("History")), vec![RowSpec::new(UNDO, tr("Undo history (steps)"), RowKind::Custom)]),
        GroupSpec::new(Some(tr("Language")), vec![RowSpec::new(LANGUAGE, tr("Language"), RowKind::choice(prefs.language.as_str(), languages))]),
        GroupSpec::new(
            Some(tr("Appearance")),
            vec![
                RowSpec::new(BRIGHTNESS, tr("Brightness"), RowKind::choice(brightness_value, brightness_options)),
                RowSpec::new(SCALE, tr("Interface scale"), RowKind::Custom),
            ],
        ),
        GroupSpec::new(Some(tr("Views")), vec![RowSpec::new(INVERT_ZOOM, tr("Invert zoom in views"), RowKind::Bool(prefs.invert_zoom))]),
    ];
    #[cfg(not(target_arch = "wasm32"))]
    groups.push(GroupSpec::new(
        Some(tr("Auto backup")),
        vec![
            RowSpec::new(AUTOSAVE, tr("Autosave every (minutes, 0 = off)"), RowKind::Custom),
            RowSpec::new(VERSIONS, tr("Keep up to"), RowKind::int(i64::from(prefs.autosave_versions), versions_range()).unit_text(tr("versions")))
                .help(&texts.autosave_help),
        ],
    ));
    groups.push(GroupSpec::new(
        Some(tr("Cooker")),
        vec![RowSpec::new(PREVIEW, tr("Preview size while editing"), RowKind::choice(preview_value, preview_options))],
    ));

    let mut page = PageSpec::new(GENERAL, tr(GENERAL_TITLE), Vec::new());
    page.groups = groups;
    page
}

pub(super) fn set(prefs: &mut Prefs, key: &str, value: PrefValue) {
    match (key, value) {
        (RECENT, PrefValue::Int(count)) => {
            if let Some(count) = whole(count, recent_range()) {
                prefs.recent_limit = count;
            }
        }
        (LANGUAGE, PrefValue::Choice(code)) => prefs.language = code,
        (BRIGHTNESS, PrefValue::Choice(id)) => {
            let at = id.parse::<usize>().ok();
            if let Some((theme, mode)) = at.and_then(|at| crate::appearance::brightness_choices().get(at).cloned()) {
                prefs.appearance.theme = Some(theme);
                prefs.appearance.mode = Some(mode);
            }
        }
        (INVERT_ZOOM, PrefValue::Bool(on)) => prefs.invert_zoom = on,
        #[cfg(not(target_arch = "wasm32"))]
        (VERSIONS, PrefValue::Int(count)) => {
            if let Some(count) = whole(count, versions_range()) {
                prefs.autosave_versions = count;
            }
        }
        (PREVIEW, PrefValue::Choice(id)) => {
            if let Ok(size) = id.parse() {
                prefs.preview_size = size;
            }
        }
        _ => {}
    }
}

pub(super) fn custom_row(prefs: &mut Prefs, ui: &mut Ui, key: &str) -> bool {
    match key {
        UNDO => {
            let mut steps = f64::from(prefs.history.undo_limit);
            let changed = slider(ui, tr("Undo history (steps)"), &mut steps, 1.0..=MAX_UNDO_STEPS, 1.0, 0);
            if changed {
                prefs.history.undo_limit = steps.round() as u32;
            }
            changed
        }
        SCALE => {
            let mut scale = f64::from(prefs.ui_scale);
            let changed = slider(ui, tr("Interface scale"), &mut scale, SCALES, 0.01, 2);
            if changed {
                prefs.ui_scale = scale as f32;
            }
            changed
        }
        #[cfg(not(target_arch = "wasm32"))]
        AUTOSAVE => {
            let mut minutes = f64::from(prefs.autosave_minutes);
            let changed = slider(ui, tr("Autosave every (minutes, 0 = off)"), &mut minutes, 0.0..=MAX_AUTOSAVE_MINUTES, 1.0, 0);
            if changed {
                prefs.autosave_minutes = minutes.round() as u32;
            }
            changed
        }
        _ => false,
    }
}

pub(super) fn reset(prefs: &mut Prefs, defaults: &Prefs) {
    prefs.recent_limit = defaults.recent_limit;
    prefs.history = defaults.history;
    prefs.language = defaults.language.clone();
    prefs.appearance.theme = defaults.appearance.theme.clone();
    prefs.appearance.mode = defaults.appearance.mode;
    prefs.ui_scale = defaults.ui_scale;
    prefs.invert_zoom = defaults.invert_zoom;
    prefs.autosave_minutes = defaults.autosave_minutes;
    prefs.autosave_versions = defaults.autosave_versions;
    prefs.preview_size = defaults.preview_size;
}
