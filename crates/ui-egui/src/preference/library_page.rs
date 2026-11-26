//! Preferences ▸ Library: the library's previews, their cache and its layout

use craft_ui::buttons::Button;
use craft_ui::contract::Show as _;
use craft_ui::patterns::preferences::{GroupSpec, PageSpec, PrefValue, RowKind, RowSpec};
use egui::Ui;

use super::preferences_page::{LIBRARY, LIBRARY_TITLE};
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::state::Prefs;

const REBUILD: &str = "library.rebuild";
#[cfg(not(target_arch = "wasm32"))]
const DISK: &str = "library.diskMib";
#[cfg(not(target_arch = "wasm32"))]
const PREVIEWS: &str = "library.maxPreviews";
const LIST: &str = "library.list";

#[cfg(not(target_arch = "wasm32"))]
fn disk_range() -> std::ops::RangeInclusive<i64> {
    0..=i64::from(materialcraft_engine::LibraryCachePrefs::MAX_DISK_MIB)
}

#[cfg(not(target_arch = "wasm32"))]
fn previews_range() -> std::ops::RangeInclusive<i64> {
    1..=i64::from(materialcraft_engine::LibraryCachePrefs::MAX_PREVIEWS)
}

pub(super) fn page(prefs: &Prefs) -> PageSpec<'_> {
    let rebuild = RowSpec::new(REBUILD, tr("Rebuild thumbnails"), RowKind::CustomRows).help(tr("Previews rebuild as they come into view."));
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut cache = vec![rebuild];
    #[cfg(not(target_arch = "wasm32"))]
    cache.extend([
        RowSpec::new(DISK, tr("Disk limit (MiB, 0 = off)"), RowKind::int(i64::from(prefs.library_cache.disk_mib), disk_range())),
        RowSpec::new(PREVIEWS, tr("Maximum stored previews"), RowKind::int(i64::from(prefs.library_cache.max_previews), previews_range())),
    ]);

    let mut page = PageSpec::new(LIBRARY, tr(LIBRARY_TITLE), Vec::new());
    page.groups = vec![
        GroupSpec::new(Some(tr("Library preview cache")), cache),
        GroupSpec::new(Some(tr("Layout")), vec![RowSpec::new(LIST, tr("Show the library as a list"), RowKind::Bool(prefs.library_list))]),
    ];
    page
}

pub(super) fn set(prefs: &mut Prefs, key: &str, value: PrefValue) {
    match (key, value) {
        #[cfg(not(target_arch = "wasm32"))]
        (DISK, PrefValue::Int(mib)) => {
            if let Some(mib) = super::whole(mib, disk_range()) {
                prefs.library_cache.disk_mib = mib;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        (PREVIEWS, PrefValue::Int(count)) => {
            if let Some(count) = super::whole(count, previews_range()) {
                prefs.library_cache.max_previews = count;
            }
        }
        (LIST, PrefValue::Bool(on)) => prefs.library_list = on,
        _ => {}
    }
}

/// The Rebuild thumbnails button throws the stored previews away at once, not on Apply: it changes no preference.
pub(super) fn custom_row(app: &mut MaterialCraftApp, ui: &mut Ui, key: &str) -> bool {
    if key == REBUILD && Button::new(tr("Rebuild thumbnails")).show(ui).clicked() {
        app.run("library.cache.clear", serde_json::json!({}));
    }
    false
}

pub(super) fn reset(prefs: &mut Prefs, defaults: &Prefs) {
    prefs.library_cache = defaults.library_cache;
    prefs.library_list = defaults.library_list;
}
