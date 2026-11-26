//! Menu actions and shortcuts share stable command ids

use craft_keymap::{Chord, CommandRef};
use serde_json::Value;

use crate::MaterialCraftApp;

pub use crate::menu_item::Row;

const fn it(menu: &'static str, label: &'static str, id: &'static str, shortcut: Option<&'static str>) -> Row {
    Row { menu, sub: None, label, id, shortcut }
}

const fn sub(menu: &'static str, sub: &'static str, label: &'static str, id: &'static str, shortcut: Option<&'static str>) -> Row {
    Row { menu, sub: Some(sub), label, id, shortcut }
}

const fn sep(menu: &'static str) -> Row {
    Row { menu, sub: None, label: "-", id: "", shortcut: None }
}

/// The number of rows in `parts`, the length of [`joined`]'s table.
const fn total(parts: &[&[Row]]) -> usize {
    let (mut sum, mut part) = (0, 0);
    while part < parts.len() {
        sum += parts[part].len();
        part += 1;
    }
    sum
}

/// `parts` one after another, so a submenu table the Explorer also shows on its own is written once.
const fn joined<const N: usize>(parts: &[&[Row]]) -> [Row; N] {
    let mut out = [sep(""); N];
    let (mut at, mut part) = (0, 0);
    while part < parts.len() {
        let mut row = 0;
        while row < parts[part].len() {
            out[at] = parts[part][row];
            at += 1;
            row += 1;
        }
        part += 1;
    }
    out
}

pub const TOP: [&str; 5] = ["File", "Edit", "Tools", "Windows", "Help"];

const PACKAGE: &str = "explorer.package";
const FOLDER: &str = "explorer.folder";
const GRAPH: &str = "explorer.graph";
const RESOURCE: &str = "explorer.resource";

pub(crate) const EXPLORER_NEW: &[Row] = &[
    sub(PACKAGE, "New", "Graph", "ui.newGraph", None),
    sub(PACKAGE, "New", "Function Graph", "ui.newFunctionGraph", None),
    sub(PACKAGE, "New", "Folder", "folder.create", None),
];
pub(crate) const EXPLORER_IMPORT: &[Row] =
    &[sub(PACKAGE, "Import", "Bitmap", "file.importBitmap", None), sub(PACKAGE, "Import", "Vector graphics", "file.importVector", None)];
pub(crate) const EXPLORER_RESOURCE: &[Row] = &[it(RESOURCE, "Add to graph", "graph.resource.add", None)];

/// Linking keeps a resource outside the package; MaterialCraft only embeds resources so far
pub(crate) const EXPLORER_LINK: &[Row] = &[sub(PACKAGE, "Link", "Bitmap", "", None), sub(PACKAGE, "Link", "Vector graphics", "", None)];
pub(crate) const EXPLORER_SEND_TO: &[Row] = &[sub(PACKAGE, "Send to", "TextureCraft", "file.sendToTextureCraft", None)];
/// Explorer actions that wrap engine commands or the operating system: computing every output,
/// showing the package file and opening the packages it depends on
pub(crate) const EXPLORER_COMPUTE: &str = "ui.explorer.computeOutputs";
pub(crate) const EXPLORER_SHOW_IN_FOLDER: &str = "ui.explorer.showInFolder";
pub(crate) const EXPLORER_OPEN_DEPENDENCIES: &str = "ui.explorer.openDependencies";
pub(crate) const EXPLORER_VIEW_OUTPUTS: &str = "ui.explorer.viewOutputs";
pub(crate) const EXPLORER_EXPORT_OUTPUTS: &str = "ui.explorer.exportOutputs";
pub(crate) const EXPLORER_EXPORT_MATERIAL: &str = "ui.explorer.exportMaterial";
const PACKAGE_PARTS: &[&[Row]] = &[
    EXPLORER_NEW,
    EXPLORER_IMPORT,
    EXPLORER_LINK,
    &[
        sep(PACKAGE),
        it(PACKAGE, "Reload", "file.reload", None),
        it(PACKAGE, "Save", "file.save", Some("Ctrl+S")),
        it(PACKAGE, "Save as...", "file.saveAs", Some("Ctrl+Shift+S")),
        it(PACKAGE, "Save a copy as...", "file.saveCopyAs", None),
        sep(PACKAGE),
        it(PACKAGE, "Export material...", "file.exportMaterial", None),
        it(PACKAGE, "Export with dependencies...", "", None),
    ],
    EXPLORER_SEND_TO,
    &[
        sep(PACKAGE),
        it(PACKAGE, "Copy", "explorer.copy", None),
        it(PACKAGE, "Paste", "explorer.paste", None),
        it(PACKAGE, "Close package", "package.close", None),
        sep(PACKAGE),
        it(PACKAGE, "Compute outputs", EXPLORER_COMPUTE, None),
        it(PACKAGE, "Show in folder...", EXPLORER_SHOW_IN_FOLDER, None),
        it(PACKAGE, "Dependency manager...", "", None),
        it(PACKAGE, "Open dependencies", EXPLORER_OPEN_DEPENDENCIES, None),
    ],
];
pub(crate) const EXPLORER_PACKAGE: &[Row] = &joined::<{ total(PACKAGE_PARTS) }>(PACKAGE_PARTS);
pub(crate) const EXPLORER_FOLDER: &[Row] = &[
    it(FOLDER, "Copy", "explorer.copy", None),
    it(FOLDER, "Paste", "explorer.paste", None),
    it(FOLDER, "Remove", "folder.remove", Some("Delete")),
    it(FOLDER, "Rename", "folder.rename", Some("F2")),
];
pub(crate) const EXPLORER_GRAPH: &[Row] = &[
    it(GRAPH, "Open", "graph.open", Some("Enter")),
    it(GRAPH, "Copy", "explorer.copy", None),
    it(GRAPH, "Paste", "explorer.paste", None),
    it(GRAPH, "Remove", "graph.delete", Some("Delete")),
    it(GRAPH, "Rename", "graph.rename", Some("F2")),
    sep(GRAPH),
    it(GRAPH, "View outputs in 3D view", EXPLORER_VIEW_OUTPUTS, None),
    it(GRAPH, "Compute outputs", EXPLORER_COMPUTE, None),
    it(GRAPH, "Export outputs...", EXPLORER_EXPORT_OUTPUTS, None),
    it(GRAPH, "Export material...", EXPLORER_EXPORT_MATERIAL, None),
];

/// The menu catalog (the target menu tree with our ids)
pub const CATALOG: &[Row] = &[
    sub("File", "New", "Graph...", "ui.newGraph", Some("Ctrl+N")),
    sub("File", "New", "Function graph", "ui.newFunctionGraph", None),
    sub("File", "New", "Empty package", "file.newPackage", None),
    it("File", "Open...", "file.open", Some("Ctrl+O")),
    sep("File"),
    it("File", "Save", "file.save", Some("Ctrl+S")),
    it("File", "Save as...", "file.saveAs", Some("Ctrl+Shift+S")),
    #[cfg(not(target_arch = "wasm32"))]
    it("File", "Save all", "file.saveAll", None),
    it("File", "Close all", "ui.closeAll", None),
    sep("File"),
    it("File", "Import bitmap...", "file.importBitmap", None),
    it("File", "Import vector graphics...", "file.importVector", None),
    it("File", "Export outputs...", "ui.export", Some("Ctrl+Shift+E")),
    it("File", "Export material...", "file.exportMaterial", None),
    it("File", "Send to TextureCraft", "file.sendToTextureCraft", None),
    sep("File"),
    it("File", "Reload resources", "ui.reloadResources", None),
    sep("File"),
    it("File", "Exit", "ui.quit", Some("Ctrl+Q")),
    it("Edit", "Undo", "edit.undo", Some("Ctrl+Z")),
    it("Edit", "Redo", "edit.redo", Some("Ctrl+Y")),
    sep("Edit"),
    it("Edit", "Copy", "edit.copy", Some("Ctrl+C")),
    it("Edit", "Cut", "edit.cut", Some("Ctrl+X")),
    it("Edit", "Paste", "edit.paste", Some("Ctrl+V")),
    it("Edit", "Duplicate", "edit.duplicate", Some("Ctrl+D")),
    it("Edit", "Delete", "edit.delete", Some("Delete")),
    it("Edit", "Delete and relink", "edit.deleteRelink", Some("Backspace")),
    it("Edit", "Select all", "edit.selectAll", Some("Ctrl+A")),
    it("Edit", "Edit node graph", "graph.node.edit", Some("Ctrl+E")),
    it("Edit", "Parent graph", "ui.graph.parent", Some("Alt+ArrowLeft")),
    sep("Edit"),
    it("Edit", "Preferences...", "ui.preferences", None),
    it("Tools", "Cancel rendering", "ui.cook.cancel", Some("Escape")),
    it("Tools", "Suspend engine", "ui.cook.suspend", Some("Shift+Escape")),
    sep("Tools"),
    it("Tools", "Compute graph", "graph.cook", None),
    it("Tools", "Snap to grid", "ui.graph.snapToGrid", None),
    it("Tools", "Compute nodes thumbnails", "ui.graph.computeThumbnails", None),
    it("Tools", "Clear nodes thumbnails", "ui.graph.clearThumbnails", Some("F5")),
    it("Tools", "Previous pin", "ui.graph.previousPin", Some("Shift+F2")),
    it("Tools", "Next pin", "ui.graph.nextPin", Some("F2")),
    it("Tools", "View outputs in 3D view", "ui.graph.viewOutputs", None),
    it("Tools", "Reset and view outputs in 3D view", "ui.graph.resetViewOutputs", None),
    it("Tools", "Clear cache", "ui.clearCache", None),
    sub("Tools", "Preview size", "256 px", "ui.preview.256", None),
    sub("Tools", "Preview size", "512 px", "ui.preview.512", None),
    sub("Tools", "Preview size", "1024 px", "ui.preview.1024", None),
    sub("Tools", "Preview size", "2048 px", "ui.preview.2048", None),
    sub("Tools", "Preview size", "Full size", "ui.preview.0", None),
    it("Tools", "Create graph from selection", "graph.createFromSelection", None),
    sep("Tools"),
    it("Tools", "Plugin Manager...", "ui.pluginManager", None),
    it("Windows", "Reset layout", "ui.resetLayout", None),
    sep("Windows"),
    it("Windows", "Explorer", "ui.view.explorer", None),
    it("Windows", "Graph", "ui.view.graph", None),
    it("Windows", "Properties", "ui.view.properties", None),
    it("Windows", "Console", "ui.view.console", None),
    it("Windows", "3D View", "ui.view.3d", None),
    it("Windows", "2D View", "ui.view.2d", None),
    it("Windows", "Library", "ui.view.library", None),
    it("Windows", "History", "ui.view.history", None),
    sep("Windows"),
    it("Windows", "Main toolbar", "ui.view.toolbar", None),
    it("Help", "Tutorials...", "ui.help.tutorials", None),
    it("Help", "Release notes...", "ui.help.releaseNotes", None),
    it("Help", "Technical requirements...", "ui.help.requirements", None),
    it("Help", "Documentation...", "ui.help.docs", None),
    it("Help", "Automation documentation...", "ui.help.automation", None),
    it("Help", "Shortcuts list...", "ui.help.shortcuts", None),
    sep("Help"),
    it("Help", "Community (Discord)...", "ui.help.discord", None),
    it("Help", "Report a bug...", "ui.help.bug", None),
    #[cfg(not(target_arch = "wasm32"))]
    it("Help", "Export log...", "ui.help.exportLog", None),
    it("Help", "Give feedback...", "ui.help.feedback", None),
    it("Help", "Troubleshooting...", "ui.help.troubleshooting", None),
    sep("Help"),
    it("Help", "All Craft apps...", "ui.help.apps", None),
    it("Help", "ArtCraft website...", "ui.help.website", None),
    sep("Help"),
    it("Help", "Home screen...", "ui.home", None),
    it("Help", "What's new...", "ui.help.news", None),
    sep("Help"),
    #[cfg(not(target_arch = "wasm32"))]
    it("Help", "Check for updates...", "ui.help.checkForUpdates", None),
    it("Help", "About MaterialCraft...", "ui.about", None),
];

fn is_ui(id: &str) -> bool {
    id.starts_with("ui.")
}

/// Whether an item can run now
pub fn enabled(app: &MaterialCraftApp, id: &str) -> bool {
    enabled_with(app, id, &Value::Null)
}

/// Whether the command `id` with `params` can run now; [`enabled`] is this with no parameters.
pub(crate) fn enabled_with(app: &MaterialCraftApp, id: &str, params: &Value) -> bool {
    if let Some(enabled) = crate::history::enabled(app, id) {
        return enabled;
    }

    if let Some(enabled) = crate::graph::editor_actions::enabled(app, id) {
        return enabled;
    }

    if let Some(enabled) = crate::graph::actions::enabled(app, id, params) {
        return enabled;
    }

    if id == "ui.cook.cancel" {
        return app.cooker.busy();
    }

    if id == "graph.cook" && app.cooker.suspended {
        return false;
    }

    if matches!(id, "edit.paste" | "edit.pasteWithoutLinks") {
        return app.session.graph().is_ok() && app.session.clipboard.as_ref().is_some_and(|clip| !clip.nodes.is_empty());
    }

    if is_ui(id) {
        return true;
    }

    materialcraft_engine::find(id).is_some_and(|c| (c.enabled)(&app.session).is_ok())
}

/// Whether an id is implemented at all (parity report)
pub fn implemented(id: &str) -> bool {
    is_ui(id) || materialcraft_engine::find(id).is_some()
}

/// Invoke a menu item (menus, shortcuts, the control channel's `ui.menu.invoke`), unless a dialog or prompt blocks it:
/// the command with no parameters, through [`crate::commands::run`], which holds the refusal.
pub fn invoke(app: &mut MaterialCraftApp, ctx: &egui::Context, id: &str) {
    crate::commands::run(app, ctx, &CommandRef::new(id));
}

/// Runs the command `id` with `params` that nothing blocks: the app's UI action of that id, else the engine command.
pub(crate) fn dispatch(app: &mut MaterialCraftApp, ctx: &egui::Context, id: &str, params: &Value) {
    if crate::graph::editor_actions::invoke(app, id) {
        return;
    }

    if crate::graph::actions::invoke(app, ctx, id, params) {
        return;
    }

    if let Some(panel) = crate::docking::view_panel(id) {
        app.ui.dock.toggle(panel);
        return;
    }

    if let Some(size) = id.strip_prefix("ui.preview.").and_then(|s| s.parse::<u32>().ok()) {
        app.ui.prefs.preview_size = size;
        app.apply_prefs();
        return;
    }

    match id {
        "ui.newGraph" => crate::new_graph_dialog::open(app, materialcraft_engine::core::GraphKind::Material),
        "ui.newFunctionGraph" => crate::new_graph_dialog::open(app, materialcraft_engine::core::GraphKind::Function),
        "ui.export" => crate::export::open(app, crate::export::ExportTab::FromGraph),
        "ui.quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        "ui.preferences" => app.ui.windows.preferences = true,
        "ui.view.graph" => {}
        "ui.resetLayout" => app.ui.dock = crate::docking::Dock::default(),
        "ui.closeAll" => crate::unsaved::guard(app, crate::unsaved::Guarded::Menu { id: id.to_string() }),
        "file.newPackage" => {
            app.run("file.newPackage", serde_json::json!({}));
        }
        "ui.reloadResources" | "ui.clearCache" => {
            if let Ok(mut c) = app.session.cache.lock() {
                c.clear();
            }
            app.session.dirty = true;
        }
        "ui.home" => app.ui.windows.home = true,
        "ui.help.shortcuts" => {
            app.shortcuts.get_or_insert_with(craft_menu::shortcuts::DialogState::new);
        }
        "ui.about" => app.ui.windows.about = true,
        #[cfg(not(target_arch = "wasm32"))]
        "ui.help.checkForUpdates" => crate::updates_ui::check(app),
        #[cfg(not(target_arch = "wasm32"))]
        "ui.openRecent" => {
            if let Some(path) = params.get("path").and_then(Value::as_str) {
                crate::windows::open_path(app, path);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        "ui.clearRecent" => app.ui.recent.clear(),
        "ui.cook.cancel" => crate::cook::cancel(app),
        "ui.cook.suspend" => crate::cook::toggle_suspended(app),
        "graph.cook" if app.cooker.suspended => {}
        "ui.pluginManager" => app.ui.windows.plugin_manager = true,
        "ui.view.toolbar" => app.ui.prefs.show_toolbar = !app.ui.prefs.show_toolbar,
        #[cfg(not(target_arch = "wasm32"))]
        "ui.help.exportLog" => crate::native_dialogs::export_log(app),
        _ => {
            if let Some(url) = crate::help_links::url(id) {
                ctx.open_url(egui::OpenUrl::new_tab(url));
                return;
            }

            // PLAN: `windows::file_action` (`windows.rs:367`) still takes `ctx`; sh.A7 drops it in shell-w9, so this call has the two-argument form and builds once shell-w9 lands.
            if crate::windows::file_action(app, id) {
                return;
            }

            app.run(id, params.clone());
        }
    }
}

/// Parse "Ctrl+Shift+E" style shortcuts; `Ctrl` in these texts is Command on macOS
pub fn parse_shortcut(s: &str) -> Option<Chord> {
    Chord::parse(&s.replace("Ctrl", "Cmd")).ok()
}

/// The text a shortcut shows on this platform, as the keymap writes it ("⇧⌘E" on macOS, "Ctrl+Shift+E" elsewhere)
pub fn shortcut_label(ctx: &egui::Context, text: &str) -> String {
    let platform = craft_keymap::egui::platform(ctx);
    parse_shortcut(text).map_or_else(|| text.to_string(), |chord| chord.display(platform))
}

/// Every shortcut in effect: the catalog's, then the engine commands' not listed there
pub(crate) fn shortcut_table() -> Vec<(&'static str, &'static str)> {
    let mut table: Vec<(&'static str, &'static str)> = CATALOG.iter().filter_map(|i| Some((i.id, i.shortcut?))).collect();
    for c in materialcraft_engine::command_specs() {
        if table.iter().any(|(id, _)| *id == c.id) {
            continue;
        }
        if let Some(shortcut) = c.shortcut {
            table.push((c.id, shortcut));
        }
    }
    table
}

/// The shortcut text shown for `id` (the catalog's, else the engine command's)
pub fn shortcut_of(id: &str) -> Option<&'static str> {
    CATALOG.iter().find(|i| i.id == id).and_then(|i| i.shortcut).or_else(|| materialcraft_engine::find(id).and_then(|c| c.shortcut))
}

/// Menu items for `ui.menu.list`
pub fn list(app: &MaterialCraftApp, ctx: &egui::Context) -> Value {
    let modality = crate::shell::modality::Modality::capture(app, ctx);
    Value::Array(
        CATALOG
            .iter()
            .filter(|i| i.label != "-")
            .map(|i| {
                serde_json::json!({"menu": i.menu, "submenu": i.sub, "label": i.label, "id": i.id, "shortcut": i.shortcut,
                    "implemented": implemented(i.id), "enabled": modality.allows(i.id) && enabled(app, i.id)})
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "menus.test.rs"]
mod tests;
