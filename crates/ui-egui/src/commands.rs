//! MaterialCraft's commands as the framework runs them: the one place that builds `CommandDef`s, and the one path
//! that runs a command by reference.

use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use craft_keymap::{Binding, CommandDef, CommandKind, CommandRef, CommandScope, Platform, Scope};
use craft_menu::{Item, MenuBar, Node};
use craft_tracked::Tracked;
use craft_ui::runtime::commands::{AppCatalog, CommandDefError, CommandSet as _, CommandState, CommandStates, FirePhase, FireSource, Fired};
use materialcraft_engine::NodeLibrary;
use serde_json::{Value, json};

use crate::MaterialCraftApp;
use crate::graph::editor_target::EditorTarget;
use crate::i18n::tr;
use crate::menus::{self, CATALOG, TOP};
use crate::node_shortcuts::NODE_ADD;
use crate::shell::modality::Modality;

/// The scope of the graph editor's keys: node creation, shown while the graph canvas has the keys.
pub(crate) const GRAPH_SCOPE: CommandScope = CommandScope::new("materialcraft.graph");

/// The scope of the 2D view's keys.
pub(crate) const VIEW2D_SCOPE: CommandScope = CommandScope::new("materialcraft.view2d");

/// The scope of the 3D view's keys.
pub(crate) const VIEW3D_SCOPE: CommandScope = CommandScope::new("materialcraft.view3d");

/// A key of the graph canvases, in [`GRAPH_SCOPE`]: the defaults `catalog` gives and `node_shortcuts::migrate`
/// keeps a saved node key off.
pub(crate) struct GraphKey {
    /// The command id.
    pub(crate) id: &'static str,
    /// The `via` parameter that tells this binding's command apart from another binding of the same id.
    pub(crate) via: Option<&'static str>,
    /// The name the shortcuts list shows.
    pub(crate) label: &'static str,
    /// The default chord, as a shortcut text.
    pub(crate) chord: &'static str,
}

impl GraphKey {
    /// The command this key runs, with its `via` parameter.
    fn command(&self) -> CommandRef {
        match self.via {
            Some(via) => CommandRef::with_params(self.id, json!({ "via": via })),
            None => CommandRef::new(self.id),
        }
    }
}

/// The graph canvases' own keys. Tab is a second key of the node search, enabled only while the preference asks for it,
/// so with the preference off Tab moves the focus.
pub(crate) const GRAPH_DEFAULTS: &[GraphKey] = &[
    GraphKey { id: "graph.wire.swap", via: None, label: "Swap links", chord: "X" },
    GraphKey { id: "ui.graph.frame", via: None, label: "Frame graph", chord: "F" },
    GraphKey { id: "ui.graph.nodeSearch", via: None, label: "Open node search", chord: "Space" },
    GraphKey { id: "ui.graph.nodeSearch", via: Some("tab"), label: "Open node search (Tab)", chord: "Tab" },
];

/// The modes of `graph.align`, as the graph toolbar offers them.
const ALIGN: &[(&str, &str)] = &[
    ("left", "Align left"),
    ("top", "Align top"),
    ("hcenter", "Align centers horizontally"),
    ("vcenter", "Align centers vertically"),
    ("hdistribute", "Distribute horizontally"),
    ("vdistribute", "Distribute vertically"),
];

/// UI actions only menus show; graph-w8 routes them.
const MENU_ONLY: &[(&str, &str)] = &[
    ("ui.graph.copyInfo", "Copy node info"),
    ("ui.graph.view2d", "View in 2D view"),
    ("ui.function.copyInfo", "Copy node info"),
    ("ui.function.openReference", "Open reference"),
    ("ui.function.setOutput", "Set as output"),
    ("ui.function.add", "Add node"),
    ("ui.function.annotate", "Add annotation"),
    ("ui.pattern.add", "Add node"),
];

/// Annotation commands the toolbar and menus run with the parameters of the moment; the bare id answers every one.
const ANNOTATIONS: &[(&str, &str)] =
    &[("graph.frame.add", "Add frame around the selection"), ("graph.comment.add", "Add comment"), ("graph.pin.add", "Add navigation pin")];

/// The view keys: (scope, command id, label, chord).
const VIEW_KEYS: &[(CommandScope, &str, &str, &str)] = &[
    (VIEW2D_SCOPE, "view2d.fit", "Fit", "F"),
    (VIEW2D_SCOPE, "view2d.tiled", "Tiled view", "Space"),
    (VIEW2D_SCOPE, "view2d.channel.rgba", "RGBA", "C"),
    (VIEW2D_SCOPE, "view2d.channel.r", "Red", "R"),
    (VIEW2D_SCOPE, "view2d.channel.g", "Green", "G"),
    (VIEW2D_SCOPE, "view2d.channel.b", "Blue", "B"),
    (VIEW2D_SCOPE, "view2d.channel.a", "Alpha", "A"),
    (VIEW3D_SCOPE, "view3d.frame", "Frame", "F"),
    (VIEW3D_SCOPE, "view3d.resetCamera", "Reset Position", "R"),
    (VIEW3D_SCOPE, "view3d.resetSelected", "Reset Selected", "Shift+R"),
    (VIEW3D_SCOPE, "view3d.saveRender", "Save Render...", "Alt+S"),
    (VIEW3D_SCOPE, "view3d.copyRender", "Copy Render to Clipboard", "Alt+C"),
    (VIEW3D_SCOPE, "view3d.tool.camera", "Camera tool", "C"),
    (VIEW3D_SCOPE, "view3d.tool.light", "Light tool", "L"),
    (VIEW3D_SCOPE, "view3d.deselect", "Deselect", "Escape"),
];

/// The toolbar's commands and the icon each shows.
const ICONS: &[(&str, &str)] = &[
    ("ui.newGraph", "new_file"),
    // PLAN: the sheet names `open`, which the catalogue gives an external-link arrow; File > Open shows `folder_open`.
    ("file.open", "folder_open"),
    ("file.save", "save"),
    ("edit.undo", "undo"),
    ("edit.redo", "redo"),
    ("ui.export", "export"),
];

/// The scope a [`CommandScope`] names; its names are constants that [`Scope::new`] accepts.
fn scope(of: CommandScope) -> Scope {
    of.scope().unwrap_or_else(Scope::app)
}

/// A def of `command` in `home`, bound to the shortcut text `chord` when there is one.
fn def(command: CommandRef, label: &str, home: &Scope, chord: Option<&str>) -> CommandDef {
    let def = CommandDef::new(command, label, home.clone());
    match chord.and_then(menus::parse_shortcut) {
        Some(chord) => def.with_default(Binding::new(chord)),
        None => def,
    }
}

/// The kind of a menu bar row: the rows `checked` answers for are toggles, the preview sizes one choice of a group.
fn kind(id: &str) -> CommandKind {
    if id.starts_with("ui.preview.") {
        CommandKind::Radio { group: Some("preview".into()) }
    } else if matches!(id, "ui.view.graph" | "ui.graph.snapToGrid" | "ui.cook.suspend" | "ui.view.toolbar") || crate::docking::view_panel(id).is_some() {
        CommandKind::Toggle
    } else {
        CommandKind::Command
    }
}

/// The graph toolbar's quick nodes: atomic node definitions and Function operations, each once.
fn quick_nodes() -> Vec<(&'static str, &'static str)> {
    // PLAN: graph-w13 keeps `ATOMIC_QUICK` (`graph/toolbar.rs:109`) as `pub(crate)`, its (icon, definition) rows, so the
    // catalog and the toolbar read one list.
    let atomic = crate::graph::toolbar::ATOMIC_QUICK.iter().flat_map(|group| group.iter().copied()).map(|(_, def): (&'static str, &'static str)| {
        let label: &'static str = materialcraft_engine::nodes::find(def).map_or(def, |definition| definition.label);
        (def, label)
    });
    let function = crate::function::library::QUICK.iter().flat_map(|group| group.iter().copied()).map(|(operation, _): (&'static str, &'static str)| {
        let label: &'static str = materialcraft_engine::nodes::function::definition(operation).map_or(operation, |definition| definition.label);
        (operation, label)
    });
    let mut seen = BTreeSet::new();
    atomic.chain(function).filter(|(def, _)| seen.insert(*def)).collect()
}

/// Every command of the app, including `node.add.<entry id>` per node entry of `library`: the menu catalog's rows
/// first, so the catalog wins a chord an engine command shares, then the graph and view keys, the engine commands, the
/// Recent packages rows and the node keys. The generation is a hash of the rows, so it moves when the library's node
/// entries change and the board rebuilds its keymap then.
pub(crate) fn catalog(platform: Platform, library: &NodeLibrary) -> AppCatalog {
    // PLAN: the defaults are one table on every platform (`Ctrl` in a shortcut text is Command on macOS through the
    // keymap's effective modifiers), so `platform` chooses nothing here; the board builds the keymap per platform.
    let _ = platform;
    let (app, graph) = (Scope::app(), scope(GRAPH_SCOPE));
    let mut defs: Vec<CommandDef> = Vec::new();

    for row in CATALOG.iter().filter(|row| row.label != "-") {
        let mut row_def = def(CommandRef::new(row.id), row.label, &app, row.shortcut).with_kind(kind(row.id));
        if let Some((_, icon)) = ICONS.iter().find(|(id, _)| *id == row.id) {
            row_def = row_def.with_icon(*icon);
        }
        defs.push(row_def);
    }
    for key in GRAPH_DEFAULTS {
        defs.push(def(key.command(), key.label, &graph, Some(key.chord)));
    }
    for (mode, label) in ALIGN {
        defs.push(def(CommandRef::with_params("graph.align", json!({ "mode": mode })), label, &graph, None));
    }
    for (node, label) in quick_nodes() {
        defs.push(def(CommandRef::with_params("ui.graph.quickAdd", json!({ "def": node })), label, &graph, None));
    }
    for (id, label) in MENU_ONLY {
        defs.push(def(CommandRef::new(*id), label, &graph, None));
    }
    for (id, label) in ANNOTATIONS {
        defs.push(def(CommandRef::new(*id), label, &app, None));
    }
    for (home, id, label, chord) in VIEW_KEYS {
        defs.push(def(CommandRef::new(*id), label, &scope(*home), Some(chord)));
    }
    for spec in materialcraft_engine::command_specs() {
        defs.push(def(CommandRef::new(spec.id), spec.label, &app, spec.shortcut));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        defs.push(def(CommandRef::new("ui.openRecent"), "Open recent package", &app, None));
        defs.push(def(CommandRef::new("ui.clearRecent"), "Clear recent packages", &app, None));
    }
    for entry in library.entries(&library.packages).iter().filter(|entry| crate::node_shortcuts::bindable(entry)) {
        defs.push(def(CommandRef::new(format!("{NODE_ADD}{}", entry.id)), &entry.label, &graph, None));
    }

    // An id the menu catalog lists keeps the menu's row; the engine's spec of it is not a second def
    let mut keys = BTreeSet::new();
    defs.retain(|def| keys.insert(def.command.key()));

    let mut hasher = DefaultHasher::new();
    for def in &defs {
        def.command.key().hash(&mut hasher);
        def.label.hash(&mut hasher);
    }

    // The canvases' Hand takes a focused canvas's Space first; production binds Space to the node search and the 2D
    // tiling and has no Space pan
    built(defs, hasher.finish()).replace_defaults(craft_ui::canvas::MomentaryCommands::Hand.id(), Vec::new())
}

/// The catalog of `defs`, leaving out a def the board refuses (reported to the log), so the rest still run.
fn built(mut defs: Vec<CommandDef>, generation: u64) -> AppCatalog {
    loop {
        match AppCatalog::new(defs.clone(), generation) {
            Ok(catalog) => return catalog,
            Err(error) => {
                log::error!("a command was left out of the catalog: {error}");
                let before = defs.len();
                if let CommandDefError::Duplicate { command, .. } | CommandDefError::NotApp { command, .. } = &error {
                    defs.retain(|def| def.command.id != *command && def.command.key() != *command);
                }
                // An error that names no def of the list leaves an empty catalog, which has nothing to refuse
                if defs.len() == before {
                    defs.clear();
                }
            }
        }
    }
}

/// What the states of one frame read once for every command.
struct Now {
    editor: EditorTarget,
    graph: bool,
    selected: usize,
    available: Vec<&'static str>,
}

impl Now {
    fn of(app: &MaterialCraftApp) -> Now {
        let selected = if let Some(editor) = &app.function_editor {
            editor.selection.len()
        } else if crate::pattern::active(app) {
            app.pattern_editor.as_ref().map_or(0, |editor| editor.selection.len())
        } else {
            app.session.selection.len()
        };
        Now {
            editor: EditorTarget::resolve(app),
            graph: app.session.graph().is_ok(),
            selected,
            available: crate::function::library::available_operations(app),
        }
    }
}

/// Whether `command` can run now, before the modality.
fn enabled(app: &MaterialCraftApp, now: &Now, command: &CommandRef) -> bool {
    let id = command.id.as_str();
    if id.starts_with(NODE_ADD) {
        return now.editor == EditorTarget::Material && now.graph;
    }

    if id.starts_with("view2d.") || id.starts_with("view3d.") {
        return true;
    }

    let param = |name: &str| command.params.get(name).and_then(Value::as_str);
    match id {
        "ui.graph.nodeSearch" if param("via").is_some() => app.ui.prefs.tab_opens_node_search && menus::enabled_with(app, id, &command.params),
        "graph.align" if param("mode").is_some() => now.selected > 1,
        "ui.graph.quickAdd" => {
            let Some(node) = param("def") else { return false };
            match now.editor {
                EditorTarget::Function => now.available.contains(&node),
                EditorTarget::Material => now.graph && crate::graph::toolbar::ATOMIC_QUICK.iter().any(|group| group.iter().any(|(_, def)| *def == node)),
                EditorTarget::Pattern => false,
            }
        }
        _ => menus::enabled_with(app, id, &command.params),
    }
}

/// The checked state of a toggle or radio command of the menu bar.
fn checked(app: &MaterialCraftApp, id: &str) -> Option<bool> {
    match id {
        "ui.view.graph" => Some(true),
        "ui.graph.snapToGrid" => Some(app.ui.prefs.snap_to_grid),
        "ui.cook.suspend" => Some(app.cooker.suspended),
        "ui.view.toolbar" => Some(app.ui.prefs.show_toolbar),
        id if id.starts_with("ui.preview.") => {
            id.strip_prefix("ui.preview.").and_then(|size| size.parse::<u32>().ok()).map(|size| size == app.ui.prefs.preview_size)
        }
        id => crate::docking::view_panel(id).map(|panel| app.ui.dock.is_open(panel)),
    }
}

/// Which commands are enabled, checked and shown, for this frame. `into` is edited only when a state differs, so its
/// revision moves only on a change.
pub(crate) fn states(app: &MaterialCraftApp, ctx: &egui::Context, into: &mut Tracked<CommandStates>) {
    let modality = Modality::capture(app, ctx);
    let now = Now::of(app);
    let wanted: Vec<(CommandRef, CommandState)> = app
        .catalog
        .defs()
        .iter()
        .map(|def| {
            let command = &def.command;
            let state = CommandState::new().enabled(modality.allows(&command.id) && enabled(app, &now, command));
            let state = match checked(app, &command.id) {
                Some(checked) => state.checked(checked),
                None => state,
            };
            (command.clone(), state)
        })
        .collect();
    if wanted.iter().any(|(command, state)| into.get(command) != Some(state)) {
        into.edit(|states| {
            for (command, state) in wanted {
                states.set(command, state);
            }
        });
    }
}

/// Whether `id` acts once per key press: a held key adds no second node and opens no second search.
fn press_only(id: &str) -> bool {
    id.starts_with(NODE_ADD) || id == "ui.graph.nodeSearch"
}

/// Runs the commands that fired since the last frame. The runtime hands each fire over once, in the frame whose
/// input fired it, so a second pass of that frame runs none again. A key's release runs nothing; a held key repeats
/// every command but those that add or open something at the pointer. Ctrl+E does nothing by key where the menu row
/// still edits (a Function or FX-Map editor, a linked graph instance).
pub(crate) fn run_fired(app: &mut MaterialCraftApp, ctx: &egui::Context, fired: &[Fired]) {
    for fire in fired {
        let id = fire.command.id.as_str();
        let runs = match fire.phase {
            FirePhase::Press => true,
            FirePhase::Repeat => !press_only(id),
            FirePhase::Release => false,
        };
        if !runs || (fire.source == FireSource::Key && !crate::graph::editor_actions::shortcut_enabled(app, id)) {
            continue;
        }
        run(app, ctx, &fire.command);
    }
}

/// Runs one command by reference, unless a dialog or prompt blocks it: a node key adds its entry at the pointer, a
/// view command goes to its view, and every other id to the app's UI actions or the engine command of that id.
pub(crate) fn run(app: &mut MaterialCraftApp, ctx: &egui::Context, command: &CommandRef) {
    let id = command.id.as_str();
    if !Modality::capture(app, ctx).allows(id) {
        return;
    }

    if let Some(entry) = id.strip_prefix(NODE_ADD) {
        add_at_pointer(app, ctx, entry);
        return;
    }

    if id.starts_with("view2d.") && crate::view2d::fired(app, id) {
        return;
    }

    if id.starts_with("view3d.") && crate::view3d::fired(app, ctx, id) {
        return;
    }

    menus::dispatch(app, ctx, id, &command.params);
}

/// Adds the library entry `entry` under the pointer in the material graph. The pointer is its latest position at the
/// fire; with none (it left the window, or a touch ended in the browser) or outside the graph canvas, nothing is
/// added.
fn add_at_pointer(app: &mut MaterialCraftApp, ctx: &egui::Context, entry: &str) {
    let Some(pointer) = ctx.input(|input| input.pointer.latest_pos()) else { return };
    if EditorTarget::resolve(app) != EditorTarget::Material || !app.graph.view.contains(pointer) {
        return;
    }

    let at = app.ui.graph_camera.view(app.graph.view).to_doc(pointer);
    // The node's title row sits under the pointer, centred across the card
    let at = [at.x as f32 - crate::graph::node_card::WIDTH * 0.5, at.y as f32 - 20.0];
    app.run("library.add", json!({"entry": entry, "at": at, "snapToGrid": app.ui.prefs.snap_to_grid}));
}

/// The menu bar the shell draws, from the menu tables: every row is a command row, whose shortcut, enabled and checked
/// state come from the command board. `bar` is edited only when the model differs.
pub(crate) fn menu_bar(app: &MaterialCraftApp, bar: &mut Tracked<MenuBar>) {
    let rows: Vec<craft_menu::Row> = CATALOG.iter().map(crate::menu_item::Row::to_craft).collect();
    let tops: Vec<&str> = TOP.iter().map(|top| tr(top)).collect();
    let mut wanted = craft_menu::from_items(&tops, &rows);
    #[cfg(not(target_arch = "wasm32"))]
    recent(app, &mut wanted);
    #[cfg(target_arch = "wasm32")]
    let _ = app;

    if **bar != wanted {
        bar.edit(|held| *held = wanted);
    }
}

/// The Recent packages submenu after File > Open: a row per recent path, then Clear recent packages.
#[cfg(not(target_arch = "wasm32"))]
fn recent(app: &MaterialCraftApp, bar: &mut MenuBar) {
    let Some(file) = bar.menus.iter_mut().find(|menu| menu.title == tr("File")) else { return };
    let Some(open) = file.children.iter().position(|node| matches!(node, Node::Item(item) if item.command.id == "file.open")) else { return };

    let mut rows: Vec<Node> =
        app.ui.recent.iter().map(|path| Node::Item(Item::new(CommandRef::with_params("ui.openRecent", json!({ "path": path })), path.clone()))).collect();
    rows.push(Node::Separator);
    rows.push(Node::Item(Item::new("ui.clearRecent", tr("Clear recent packages")).english("Clear recent packages")));
    let submenu = Node::submenu(tr("Recent packages"), rows).english("Recent packages").enabled(!app.ui.recent.is_empty());
    file.children.insert(open + 1, submenu);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_adds(catalog: &AppCatalog) -> BTreeSet<String> {
        catalog.defs().iter().filter_map(|def| def.command.id.strip_prefix(NODE_ADD).map(str::to_owned)).collect()
    }

    #[test]
    fn node_add_commands_follow_the_library_and_their_generation_moves() {
        let plain = NodeLibrary::default();

        // A package graph the Library files under the atomic nodes adds a node entry
        let mut package = materialcraft_engine::sample_session().pkg.clone();
        for graph in &mut package.graphs {
            graph.attributes.category = "Atomic nodes".into();
        }
        let mut grown = NodeLibrary::default();
        grown.packages.insert("sbs://extra.sbs".into(), std::sync::Arc::new(package));

        let (before, after) = (catalog(Platform::Linux, &plain), catalog(Platform::Linux, &grown));
        for (library, catalog) in [(&plain, &before), (&grown, &after)] {
            let entries = library.entries(&library.packages);
            let bindable: BTreeSet<String> = entries.iter().filter(|entry| crate::node_shortcuts::bindable(entry)).map(|entry| entry.id.clone()).collect();
            assert_eq!(node_adds(catalog), bindable, "one node.add per node entry and none for the other entries");
            assert!(entries.iter().any(|entry| !crate::node_shortcuts::bindable(entry)), "the library also has entries that are not nodes");
        }

        assert!(node_adds(&after).is_superset(&node_adds(&before)) && node_adds(&after).len() > node_adds(&before).len(), "the package adds a node entry");
        assert_ne!(before.generation(), after.generation(), "the board rebuilds its keymap for the new node keys");
        assert_eq!(catalog(Platform::Linux, &plain).generation(), before.generation(), "an unchanged library keeps its generation");
    }
}
