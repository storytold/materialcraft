//! The app's dialogs and windows (drawn by [`show`]), the Console and History panels and the History footer, and the
//! file actions behind the File menu

use craft_host::PickRequest;
use craft_ui::bars::{BarItem, BarItems, ItemDisplay};
use craft_ui::contract::{Bind, Show as _, ShowCollection as _, WordSetCx as _};
use craft_ui::list::{EventLevel, EventList, EventListLayout, EventLog, EventLogState, EventWords, ListRow, LogEvent};
use craft_ui::patterns::form_dialogs::{AboutDialog, AboutInfo, AboutLink, Credits};
use craft_ui::patterns::panels::{HistoryEvent, HistoryKey, HistoryPanel, HistoryRow};
use egui::Ui;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::Value;
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// Draws every open dialog and window of the main window, in a fixed order: New graph, Export, Preferences, About,
/// Shortcuts, Home, Plug-in manager, the unsaved-changes prompt, Expose parameter, then the browser's Save package
/// dialog or the update dialogs
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    if app.ui.windows.new_graph {
        crate::new_graph_dialog::show(app, ui);
    }
    if app.ui.windows.export {
        crate::export::show(app, ui);
    }
    if app.ui.windows.preferences {
        crate::preferences::show(app, ui);
    }
    if app.ui.windows.about {
        about(app, ui);
    }
    if app.ui.windows.shortcuts {
        crate::shortcuts_list::show(app, ui);
    }
    if app.ui.windows.home {
        crate::home_screen::show(app, ui);
    }
    if app.ui.windows.plugin_manager {
        crate::plugin_manager::show(app, ui);
    }
    crate::unsaved::show(app, ui);
    crate::properties::ExposeParameterDialog::show(app, ui);
    #[cfg(target_arch = "wasm32")]
    crate::web_files::save_dialog(app, ui);
    #[cfg(not(target_arch = "wasm32"))]
    crate::updates_ui::show(app, ui);
}

/// Help ▸ About: the version, with the build's commit on a Nightly, the app's links, and on a Nightly a way to report
/// a problem
fn about(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let identity = crate::os_window::identity();
    let nightly = identity == crate::os_window::LaunchIdentity::NightlyV1;
    let version = if nightly {
        // The exact commit the feed names, when the build recorded one. Nothing is invented.
        let commit = if app.build_commit.is_empty() { tr("unavailable") } else { app.build_commit.as_str() };
        format!("{} ({commit})", app.build_version)
    } else {
        app.build_version.clone()
    };
    let mut links = vec![
        AboutLink::new("getartcraft.com/apps/materialcraft", "https://getartcraft.com/apps/materialcraft"),
        AboutLink::new("discord.gg/artcraft", "https://discord.gg/artcraft"),
    ];
    if nightly {
        // Opens only after a click. No diagnostics, paths or documents are sent automatically.
        links.push(AboutLink::new(tr("Report a problem"), "https://github.com/storytold/materialcraft/issues/new"));
    }
    let blurb = format!("{} {}", tr("Node-based procedural materials, built in pure Rust."), tr("An open-source Crafting App by the ArtCraft team and community."));
    let info = AboutInfo::new(crate::os_window::name_for(identity), blurb, version).links(links).built_with("MIT OR Apache-2.0");

    let mut open = true;
    let shown = AboutDialog::new(&info, &Credits::default()).opened(Bind::bound(&mut open)).show(ui);
    if let Some(url) = shown.link {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    app.ui.windows.about = open;
}

/// The Console panel: the session's log, oldest first, warnings in the warning tone, then each node failure in the
/// danger tone and one line for the placeholder nodes
pub fn console(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let mut lines: Vec<(EventLevel, String)> = app
        .session
        .log
        .iter()
        .map(|line| (if line.starts_with(materialcraft_engine::WARNING_PREFIX) { EventLevel::Warning } else { EventLevel::Info }, line.clone()))
        .collect();
    // Placeholders for missing SBS nodes are already listed in the warnings, so they get one line
    let (failed, placeholders) = app.session.cook_failures();
    lines.extend(failed.into_iter().map(|(id, error)| (EventLevel::Error, format!("node {}: {error}", id.0))));
    if placeholders > 0 {
        let text = format!("{placeholders} {}", tr("placeholder node(s) stand in for nodes MaterialCraft doesn't have yet (see the warnings above)"));
        lines.push((EventLevel::Warning, text));
    }

    // The console shows no age, so the events carry no time
    let events: Vec<LogEvent> = lines.into_iter().enumerate().map(|(at, (level, text))| LogEvent::new(at as u64, level, "", text, 0)).collect();
    let log = EventLog::new(&events, &EventLogState::default()).oldest_first();
    let _ = EventList::new(tr("Console")).layout(EventListLayout::Console).show_items(ui, &log, &mut |_, icx| {
        let Some(event) = log.event(icx.at) else { return ListRow::spacer() };
        event.console_row(&icx.words::<EventWords>(), icx.format())
    });
}

/// The History panel's footer: Undo and Redo, drawn by `docking.rs` through `DockPanels::footer`. Their enabled state
/// comes from the command board, as the menu's does
pub(crate) fn history_footer(_app: &mut MaterialCraftApp, ui: &mut Ui) {
    let _ = BarItems::new(vec![BarItem::new("edit.undo").display(ItemDisplay::IconText), BarItem::new("edit.redo").display(ItemDisplay::IconText)])
        .show(ui);
}

/// The History panel: one numbered row per undoable edit of the document, the newest current. Picking a row undoes
/// back to it, once per newer row
pub fn history(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let labels = app.session.document_history_labels(crate::history::document(app));
    let rows: Vec<HistoryRow> = labels.iter().enumerate().map(|(at, label)| HistoryRow::state(at as u64, format!("{}  {label}", at + 1))).collect();
    let current = rows.last().map(|row| row.key);
    let shown = HistoryPanel::new(&rows, current).show(ui);
    for event in shown.events {
        if let HistoryEvent::Select(HistoryKey::State(picked)) = event {
            for _ in picked + 1..rows.len() as u64 {
                if app.run("edit.undo", json!({})).is_none() {
                    break;
                }
            }
        }
    }
}

/// Authoring and published material formats offered by File ▸ Open.
pub const OPEN_EXTS: &[&str] = &["sbs", "mcraft", "mcmat"];

pub(crate) fn package_filters(open: bool, archive: bool) -> Vec<(&'static str, &'static [&'static str])> {
    let mut filters: Vec<(&str, &[&str])> = vec![("SBS XML Package (*.sbs)", &["sbs"]), ("MaterialCraft Project (*.mcraft)", &["mcraft"])];
    if archive {
        filters.reverse();
    }
    if open {
        filters.push(("MaterialCraft Material (*.mcmat)", &["mcmat"]));
        filters.insert(0, ("All formats (*.sbs *.mcraft *.mcmat)", OPEN_EXTS));
    }
    filters
}

/// The pick for opening a package, or for saving one as `archive` (a MaterialCraft Project) or SBS, whose typed name
/// gets that format's extension
pub(crate) fn package_dialog(open: bool, archive: bool) -> PickRequest {
    let request = if open { PickRequest::open("document") } else { PickRequest::save("document").suggested_extension(if archive { "mcraft" } else { "sbs" }) };
    // Browser SBS downloads wrap the XML and its resources in a transport ZIP, which opens as the package
    let download: Option<(&'static str, &'static [&'static str])> = (open && cfg!(target_arch = "wasm32")).then_some(("SBS Package Download (*.zip)", &["zip"][..]));
    package_filters(open, archive).into_iter().chain(download).fold(request, |request, (name, extensions)| request.filter(name, extensions))
}

fn resource_dialog(vector: bool) -> PickRequest {
    resource_filters(vector).into_iter().fold(PickRequest::open("import"), |request, (label, extensions)| request.filter(label, extensions))
}

pub(crate) fn resource_filters(vector: bool) -> Vec<(String, &'static [&'static str])> {
    use materialcraft_engine::formats::{BITMAP_FORMATS, VECTOR_FORMATS};
    let formats = if vector { VECTOR_FORMATS } else { BITMAP_FORMATS };
    formats
        .iter()
        .map(|(label, extensions)| {
            let patterns = extensions.iter().map(|extension| format!("*.{extension}")).collect::<Vec<_>>().join(" ");
            (format!("{label} ({patterns})"), *extensions)
        })
        .collect()
}

/// Import an asset into the active package without creating a graph node.
pub fn import_resource(app: &mut MaterialCraftApp, vector: bool) {
    import_resource_into(app, vector, "Resources");
}

/// Import an asset into `folder` of the active package, which the pick keeps as its target
pub(crate) fn import_resource_into(app: &mut MaterialCraftApp, vector: bool, folder: &str) {
    if !app.session.has_package() {
        return;
    }

    let then = crate::native_dialogs::Then::ImportResource { document: app.session.document_id, vector, folder: folder.to_string() };
    crate::native_dialogs::ask(app, resource_dialog(vector), then);
}

/// After a package opened with warnings (an SBS file using nodes MaterialCraft lacks, say), bring the Console forward
pub fn after_open(app: &mut MaterialCraftApp) {
    let warning_count = app.session.load_warnings.len();
    if warning_count > 0 {
        app.ui.dock.focus(crate::docking::Panel::Console);
        app.toast(format!("{} {warning_count} {}", tr("Opened with"), tr("warnings: see the Console")), false);
    }
}

/// File menu actions that need a file dialog. Returns true when handled
pub fn file_action(app: &mut MaterialCraftApp, id: &str) -> bool {
    if matches!(id, "file.save" | "file.saveAs" | "file.saveCopyAs" | "file.importBitmap" | "file.importVector") && !app.session.has_package() {
        return true;
    }

    use crate::native_dialogs::{self, Then};
    match id {
        "file.open" => native_dialogs::ask(app, package_dialog(true, false), Then::OpenPackage),
        "file.importBitmap" => {
            let then = Then::ImportBitmap(native_dialogs::GraphRef::active(app));
            native_dialogs::ask(app, resource_dialog(false), then);
        }
        "file.importVector" => import_resource(app, true),
        "file.exportMaterial" => crate::export::open(app, crate::export::ExportTab::Mcmat),
        #[cfg(not(target_arch = "wasm32"))]
        "file.save" if has_local_path(app) => {
            if let Some(r) = app.run("file.save", json!({})) {
                let path = r["path"].as_str().unwrap_or_default().to_string();
                app.ui.add_recent(&path);
                app.toast(format!("{} {path}", tr("Saved")), false);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        "file.save" | "file.saveAs" | "file.saveCopyAs" => native_dialogs::save_package(app, id == "file.saveCopyAs"),
        #[cfg(not(target_arch = "wasm32"))]
        "file.saveAll" => save_all(app),
        #[cfg(not(target_arch = "wasm32"))]
        "file.sendToTextureCraft" => {
            if app.run("file.sendToTextureCraft", json!({})).is_some() {
                app.toast(tr("Sent to TextureCraft: find it on the shelf under My smart materials"), false);
            }
        }
        #[cfg(target_arch = "wasm32")]
        "file.save" => crate::web_files::save(app),
        #[cfg(target_arch = "wasm32")]
        "file.saveAs" => crate::web_files::save_as(app, false),
        #[cfg(target_arch = "wasm32")]
        "file.saveCopyAs" => crate::web_files::save_copy(app),
        _ => return false,
    }
    true
}

/// File ▸ Save all: save every modified package with a file and say which still need Save as
#[cfg(not(target_arch = "wasm32"))]
fn save_all(app: &mut MaterialCraftApp) {
    let Some(reply) = app.run("file.saveAll", json!({})) else { return };

    let saved = reply["saved"].as_array().into_iter().flatten();
    let saved_count = saved.clone().count();
    for path in saved.filter_map(Value::as_str) {
        app.ui.add_recent(path);
    }

    let without_file: Vec<&str> = reply["withoutFile"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let failures: Vec<String> = reply["failed"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|failure| format!("{}: {}", failure["package"].as_str().unwrap_or_default(), failure["error"].as_str().unwrap_or_default()))
        .collect();
    let mut message = format!("{saved_count} {}", tr("packages saved"));
    if !without_file.is_empty() {
        message.push_str(&format!("\n{} {}", tr("Use Save as for:"), without_file.join(", ")));
    }
    if !failures.is_empty() {
        message.push_str(&format!("\n{} {}", tr("Not saved:"), failures.join("; ")));
    }

    app.session.log(format!("Save all: {saved_count} saved, {} need Save as, {} failed", without_file.len(), failures.len()));
    app.toast_only(message, !failures.is_empty());
}

/// Whether File ▸ Save writes straight to the package's path (a package opened from a URL has none to write to)
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn has_local_path(app: &MaterialCraftApp) -> bool {
    app.session.path.as_ref().is_some_and(|path| !path.contains("://"))
}

/// Open a package from a path (recent list, command line)
#[cfg(not(target_arch = "wasm32"))]
pub fn open_path(app: &mut MaterialCraftApp, path: &str) {
    open_path_now(app, path);
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn open_path_now(app: &mut MaterialCraftApp, path: &str) {
    if app.run("file.open", json!({"path": path})).is_some() {
        app.ui.add_recent(path);
        app.ui.frame_all = true;
        app.toast(format!("{} {path}", tr("Opened")), false);
        after_open(app);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "windows.test.rs"]
mod tests;
