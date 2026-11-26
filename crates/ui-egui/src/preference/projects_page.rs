//! Preferences ▸ Projects: the 3D view a new package and a reset start from

use craft_ui::buttons::Button;
use craft_ui::contract::{Entry, Props as _, Show as _};
use craft_ui::display::{Shade, Text};
use craft_ui::patterns::preferences::{GroupSpec, PageSpec, PrefValue, RowKind, RowSpec};
use craft_ui::style::TypeRole;
use egui::Ui;
use materialcraft_engine::view_cmds::ENVIRONMENTS;

use super::edit_view;
use super::preferences_page::{PROJECTS, PROJECTS_TITLE};
use crate::i18n::tr;
use crate::state::Prefs;

const STATUS: &str = "projects.status";
const ENVIRONMENT: &str = "projects.environment";
const AMBIENT: &str = "projects.ambient";
const LIGHT_1: &str = "projects.light1";
const LIGHT_2: &str = "projects.light2";
const BUILT_IN: &str = "projects.builtIn";

pub(super) fn page(prefs: &Prefs) -> PageSpec<'_> {
    let view = prefs.default_view3d.clone().unwrap_or_default();
    let environment = ENVIRONMENTS.iter().find(|(id, _)| *id == view.environment).map_or("", |(id, _)| *id);
    let environments = ENVIRONMENTS.iter().map(|(id, label)| Entry::new(*id, tr(*label))).collect();

    let mut page = PageSpec::new(PROJECTS, tr(PROJECTS_TITLE), Vec::new());
    page.groups = vec![
        GroupSpec::new(
            Some(tr("Default 3D view")),
            vec![
                RowSpec::new(STATUS, tr("Default 3D view"), RowKind::CustomRows),
                RowSpec::new(ENVIRONMENT, tr("Default environment"), RowKind::choice(environment, environments)),
            ],
        ),
        GroupSpec::new(
            Some(tr("Default lighting state")),
            vec![
                RowSpec::new(AMBIENT, tr("Ambient Light"), RowKind::Bool(view.ambient.enabled)),
                RowSpec::new(LIGHT_1, tr("Point Light 1"), RowKind::Bool(view.lights[0].enabled)),
                RowSpec::new(LIGHT_2, tr("Point Light 2"), RowKind::Bool(view.lights[1].enabled)),
                RowSpec::new(BUILT_IN, tr("Use the built-in default"), RowKind::CustomRows).enabled(prefs.default_view3d.is_some()),
            ],
        ),
    ];
    page
}

pub(super) fn set(prefs: &mut Prefs, key: &str, value: PrefValue) {
    match (key, value) {
        (ENVIRONMENT, PrefValue::Choice(id)) => {
            if let Some((id, _)) = ENVIRONMENTS.iter().find(|(known, _)| *known == id) {
                edit_view(prefs, |view| view.environment = (*id).to_owned());
            }
        }
        (AMBIENT, PrefValue::Bool(on)) => edit_view(prefs, |view| view.ambient.enabled = on),
        (LIGHT_1, PrefValue::Bool(on)) => edit_view(prefs, |view| view.lights[0].enabled = on),
        (LIGHT_2, PrefValue::Bool(on)) => edit_view(prefs, |view| view.lights[1].enabled = on),
        _ => {}
    }
}

pub(super) fn custom_row(prefs: &mut Prefs, ui: &mut Ui, key: &str) -> bool {
    match key {
        STATUS => {
            let saved = if prefs.default_view3d.is_some() { tr("A saved default is in use") } else { tr("The built-in default is in use") };
            let _ = Text::new(saved).shade(Shade::Muted).show(ui);
            let _ = Text::new(tr("Save Current State As Default, in the 3D view's Scene menu, saves the whole view here"))
                .role(TypeRole::Caption)
                .shade(Shade::Faint)
                .show(ui);
            false
        }
        BUILT_IN => {
            let pressed = Button::new(tr("Use the built-in default")).enabled(prefs.default_view3d.is_some()).show(ui).clicked();
            if pressed {
                prefs.default_view3d = None;
            }
            pressed
        }
        _ => false,
    }
}

pub(super) fn reset(prefs: &mut Prefs, defaults: &Prefs) {
    prefs.default_view3d = defaults.default_view3d.clone();
}
