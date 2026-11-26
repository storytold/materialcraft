//! Preferences ▸ 3D View: what a new 3D view shows and how its camera moves

use craft_ui::contract::Entry;
use craft_ui::patterns::preferences::{GroupSpec, PageSpec, PrefValue, RowKind, RowSpec};
use egui::Ui;
use materialcraft_engine::geom::view::Background;

use super::preferences_page::{VIEW3D, VIEW3D_TITLE};
use super::{edit_view, slider};
use crate::i18n::tr;
use crate::state::Prefs;

const BACKGROUND: &str = "view3d.environmentBackground";
const SAMPLES: &str = "view3d.samples";
const PAN_LIMIT: &str = "view3d.panLimit";
const PAN_SPEED: &str = "view3d.panSpeed";

/// The renderer's rays per pixel side, with the option ids and names the sample count list shows
const SAMPLE_COUNTS: [(u32, &str, &str); 4] = [(1, "1", "Low (1 spp)"), (2, "2", "Medium (4 spp)"), (3, "3", "9 spp"), (4, "4", "High (16 spp)")];
/// The furthest the 3D camera's target may move from the mesh, in mesh radii
const MAX_PAN_LIMIT: f64 = 20.0;

pub(super) fn page(prefs: &Prefs) -> PageSpec<'_> {
    let view = prefs.default_view3d.clone().unwrap_or_default();
    let samples = SAMPLE_COUNTS.iter().find(|(count, ..)| *count == view.samples).map_or("", |(_, id, _)| *id);
    let options = SAMPLE_COUNTS.iter().map(|(_, id, label)| Entry::new(*id, tr(*label))).collect();

    let mut page = PageSpec::new(VIEW3D, tr(VIEW3D_TITLE), Vec::new());
    page.groups = vec![
        GroupSpec::new(
            Some(tr("Misc")),
            vec![
                RowSpec::new(BACKGROUND, tr("Environment shown behind the mesh by default"), RowKind::Bool(view.background == Background::Environment)),
                RowSpec::new(SAMPLES, tr("Sample count"), RowKind::choice(samples, options)),
            ],
        ),
        GroupSpec::new(
            Some(tr("Camera")),
            vec![RowSpec::new(PAN_LIMIT, tr("3D pan limit (mesh radii)"), RowKind::Custom), RowSpec::new(PAN_SPEED, tr("3D pan speed"), RowKind::Custom)],
        ),
    ];
    page
}

pub(super) fn set(prefs: &mut Prefs, key: &str, value: PrefValue) {
    match (key, value) {
        (BACKGROUND, PrefValue::Bool(shown)) => {
            edit_view(prefs, |view| view.background = if shown { Background::Environment } else { Background::default() });
        }
        (SAMPLES, PrefValue::Choice(id)) => {
            if let Some((count, ..)) = SAMPLE_COUNTS.iter().find(|(_, known, _)| *known == id) {
                edit_view(prefs, |view| view.samples = *count);
            }
        }
        _ => {}
    }
}

pub(super) fn custom_row(prefs: &mut Prefs, ui: &mut Ui, key: &str) -> bool {
    let (label, value, range, step, decimals) = match key {
        PAN_LIMIT => (tr("3D pan limit (mesh radii)"), &mut prefs.camera.pan_limit, 0.0..=MAX_PAN_LIMIT, 0.1, 1),
        PAN_SPEED => (tr("3D pan speed"), &mut prefs.camera.pan_speed, 0.1..=4.0, 0.01, 2),
        _ => return false,
    };
    let mut shown = f64::from(*value);
    let changed = slider(ui, label, &mut shown, range, step, decimals);
    if changed {
        *value = shown as f32;
    }
    changed
}

pub(super) fn reset(prefs: &mut Prefs, defaults: &Prefs) {
    prefs.camera = defaults.camera;
    if let Some(view) = &mut prefs.default_view3d {
        let built_in = materialcraft_engine::View3d::default();
        view.background = built_in.background;
        view.samples = built_in.samples;
    }
}
