use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use craft_keymap::{Chord, CommandRef, Keymap, Platform, Scopes};
use egui::{Event, Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, Node};
use serde_json::json;

use super::*;
use crate::UiState;

fn sample_app() -> MaterialCraftApp {
    MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default())
}

/// The whole window around `app`, with the runtime installed, as the eframe driver runs it (sh.A31).
fn harness_of(app: MaterialCraftApp) -> Harness<'static, MaterialCraftApp> {
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| {
        MaterialCraftApp::install_runtime(&cc.egui_ctx, None).expect("runtime");
        app
    });
    harness.run_steps(4);
    harness
}

/// The unsaved-changes prompt as a user opens it: an edit, then Quit.
fn prompt(app: &mut MaterialCraftApp) {
    app.run("graph.node.add", json!({"def": "uniform", "at": [0, 0]}));
    crate::unsaved::guard(app, crate::unsaved::Guarded::Quit);
    assert!(app.unsaved.is_some(), "an edited package asks before quitting");
}

/// A native file dialog asked for and waiting.
fn file_dialog(app: &mut MaterialCraftApp) {
    crate::native_dialogs::ask(app, crate::windows::package_dialog(true, false), crate::native_dialogs::Then::OpenPackage);
}

fn press(harness: &mut Harness<'static, MaterialCraftApp>, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    harness.run_steps(2);
}

/// A key held down: its press, one repeat, then its release.
fn hold(harness: &mut Harness<'static, MaterialCraftApp>, modifiers: Modifiers, key: Key) {
    for repeat in [false, true] {
        harness.event(Event::Key { key, physical_key: None, pressed: true, repeat, modifiers });
        harness.step();
    }
    harness.event(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
    harness.run_steps(2);
}

fn nodes(harness: &Harness<'static, MaterialCraftApp>) -> usize {
    harness.state().session.graph().unwrap().nodes.len()
}

/// Whether the command board holds `id` (no parameters) as enabled this frame.
fn board_enabled(harness: &Harness<'static, MaterialCraftApp>, id: &str) -> bool {
    harness.state().command_states.get(&CommandRef::new(id)).is_some_and(|state| state.is_enabled())
}

/// The menu row whose name starts with `label`; the name may go on with the row's shortcut text.
fn row<'h>(harness: &'h Harness<'static, MaterialCraftApp>, label: &str) -> Node<'h> {
    harness
        .query_all_by_label_contains(label)
        .find(|node| node.accesskit_node().label().is_some_and(|name| name.starts_with(label)))
        .unwrap_or_else(|| panic!("no row named {label}"))
}

fn open_file_menu(harness: &mut Harness<'static, MaterialCraftApp>) {
    harness.get_by_label("File").click();
    harness.run_steps(3);
}

#[test]
fn catalog_shortcuts_parse_and_ids_are_unique() {
    let mut seen = BTreeSet::new();
    for i in CATALOG.iter().filter(|i| i.label != "-") {
        assert!(seen.insert(i.id), "duplicate menu id {}", i.id);
        if let Some(sc) = i.shortcut {
            assert!(parse_shortcut(sc).is_some(), "{sc}");
        }
    }
    // Almost every item is live
    let live = CATALOG.iter().filter(|i| i.label != "-" && implemented(i.id)).count();
    assert!(live + 1 >= CATALOG.iter().filter(|i| i.label != "-").count());
}

#[test]
fn every_shortcut_is_a_chord_and_the_catalog_wins_a_shared_chord() {
    let library = materialcraft_engine::NodeLibrary::default();
    for platform in [Platform::Mac, Platform::Windows, Platform::Linux] {
        let catalog = crate::commands::catalog(platform, &library);
        let map = Keymap::new(catalog.defs().to_vec(), Scopes::new(), platform);
        let mut owners = BTreeMap::new();
        for (id, text) in shortcut_table() {
            let chord = parse_shortcut(text).unwrap_or_else(|| panic!("{id}: {text}"));
            owners.entry(chord.effective(platform)).or_insert((id, chord));
        }
        for (id, chord) in owners.into_values() {
            assert_eq!(map.holder(&chord), Some(CommandRef::new(id)), "{chord} on {platform:?}");
        }
    }
}

#[test]
fn ctrl_in_a_shortcut_text_is_command_on_macos() {
    let saved = parse_shortcut("Ctrl+Shift+E").unwrap();
    let command = Chord::parse("Cmd+Shift+E").unwrap();
    let control = Chord::parse("Control+Shift+E").unwrap();
    assert_eq!(saved, command, "Ctrl in a saved text is the Command chord");
    assert_ne!(saved.effective(Platform::Mac), control.effective(Platform::Mac), "on macOS it is not the Control key");
    assert_eq!(saved.effective(Platform::Windows), control.effective(Platform::Windows), "elsewhere Command is Ctrl");
}

/// Shortcut dispatch through the runtime: the app's own gating of each chord.
#[test]
fn a_shortcut_needs_exactly_its_modifiers() {
    // Ctrl+N runs New graph, and a held Ctrl+N fires again
    let mut harness = harness_of(sample_app());
    press(&mut harness, Modifiers::COMMAND, Key::N);
    assert!(harness.state().ui.windows.new_graph, "Ctrl+N is bound to New graph");
    harness.state_mut().ui.windows.new_graph = false;
    harness.event(Event::Key { key: Key::N, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    harness.step();
    harness.state_mut().ui.windows.new_graph = false;
    harness.event(Event::Key { key: Key::N, physical_key: None, pressed: true, repeat: true, modifiers: Modifiers::COMMAND });
    harness.run_steps(2);
    assert!(harness.state().ui.windows.new_graph, "a held key runs the shortcut again");

    // A node key adds one node at the pointer while the material graph canvas has the keys
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    let entry = {
        let library = &app.session.library;
        library.entries(&library.packages).into_iter().find(crate::node_shortcuts::bindable).expect("a node entry").id
    };
    app.overrides = Arc::new(crate::node_shortcuts::migrate(&BTreeMap::from([(entry, "B".to_owned())]), Platform::current()));
    let mut harness = harness_of(app);
    let canvas = harness.state().graph.view;
    let at = canvas.left_top() + egui::vec2(24.0, canvas.height() * 0.5);
    harness.event(Event::PointerMoved(at));
    for pressed in [true, false] {
        harness.event(Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        harness.step();
    }
    let before = nodes(&harness);
    hold(&mut harness, Modifiers::NONE, Key::B);
    assert_eq!(nodes(&harness), before + 1, "B adds its node once, and its repeat adds no second one");
    press(&mut harness, Modifiers::ALT, Key::B);
    assert_eq!(nodes(&harness), before + 1, "Alt+B is not the B entry's chord");

    // A focused text field keeps the plain key
    harness.get_by_label("Node label").click();
    harness.run_steps(2);
    press(&mut harness, Modifiers::NONE, Key::B);
    assert_eq!(nodes(&harness), before + 1, "B types into the focused field");

    // In a Function editor the node keys are off
    let owner = harness.state_mut().run("graph.node.add", json!({"def": "pixel_processor", "at": [400, 0]})).unwrap()["id"].as_u64().unwrap();
    let state = harness.state_mut();
    state.function_editor = Some(crate::function::FunctionEditor::new(
        state.session.graph,
        materialcraft_engine::core::NodeId(owner),
        crate::function::FunctionTarget::Parameter("function".into()),
    ));
    harness.run_steps(2);
    let before = nodes(&harness);
    // The canvas takes the keys back from the field
    harness.event(Event::PointerMoved(at));
    for pressed in [true, false] {
        harness.event(Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        harness.step();
    }
    press(&mut harness, Modifiers::NONE, Key::B);
    assert_eq!(nodes(&harness), before, "B in a Function editor adds no material node");
    let app = harness.state();
    let node_keys = app.catalog.defs().iter().filter(|def| def.command.id.starts_with(crate::node_shortcuts::NODE_ADD));
    assert!(node_keys.clone().count() > 0);
    assert!(node_keys.into_iter().all(|def| !app.command_states.get(&def.command).is_some_and(|state| state.is_enabled())), "the node keys are off");

    // Ctrl+E on a linked graph instance does nothing by key; the menu row still opens its graph
    let mut app = sample_app();
    let document = app.session.document_id;
    app.run("file.new", json!({"template": "empty", "name": "Second"})).unwrap();
    let instance = app.run("graph.instance.add", json!({"document": document, "graph": 0})).unwrap()["id"].as_u64().unwrap();
    app.run("graph.select", json!({"ids": [instance]})).unwrap();
    let mut harness = harness_of(app);
    let active = harness.state().session.graph;
    press(&mut harness, Modifiers::COMMAND, Key::E);
    assert_eq!(harness.state().session.graph, active, "Ctrl+E does not follow a linked instance");
    let ctx = harness.ctx.clone();
    invoke(harness.state_mut(), &ctx, "graph.node.edit");
    assert_ne!(harness.state().session.graph, active, "Edit node graph from the menu opens the instanced graph");

    // Escape cancels a cook only while one runs
    let mut harness = harness_of(sample_app());
    let idle = (0..600).any(|_| {
        harness.step();
        !harness.state().cooker.busy()
    });
    assert!(idle, "the opening cook ends");
    assert!(!board_enabled(&harness, "ui.cook.cancel"), "with no cook, Escape is left to the focused component");
    harness.state_mut().run("graph.node.add", json!({"def": "uniform", "at": [0, 0]}));
    let busy = (0..30).any(|_| {
        harness.step();
        harness.state().cooker.busy()
    });
    assert!(busy, "an edit starts a cook");
    assert!(board_enabled(&harness, "ui.cook.cancel"), "Escape cancels the running cook");
}

/// Blocked commands: a blocker and a command give runs or waits, by key and through `invoke`.
#[test]
fn delete_waits_behind_the_unsaved_prompt() {
    // The unsaved prompt: Delete, wire Delete and Export wait
    let mut app = sample_app();
    app.run("edit.selectAll", json!({}));
    let wire = app.session.graph().unwrap().connections.first().map(|wire| (wire.to, wire.to_port.clone())).unwrap();
    app.graph.selected_wires = vec![wire];
    prompt(&mut app);
    let mut harness = harness_of(app);
    let (count, wires) = (nodes(&harness), harness.state().session.graph().unwrap().connections.len());
    press(&mut harness, Modifiers::NONE, Key::Delete);
    assert_eq!(nodes(&harness), count, "Delete does not edit the package behind the prompt");
    assert_eq!(harness.state().session.graph().unwrap().connections.len(), wires, "nor does it delete the selected wire");
    press(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::E);
    assert!(!harness.state().ui.windows.export, "Export waits");
    let ctx = harness.ctx.clone();
    invoke(harness.state_mut(), &ctx, "ui.export");
    assert!(!harness.state().ui.windows.export, "invoke waits too");

    harness.state_mut().unsaved = None;
    harness.run_steps(2);
    press(&mut harness, Modifiers::NONE, Key::Delete);
    assert!(nodes(&harness) < count, "Delete works once the prompt is answered");
    invoke(harness.state_mut(), &ctx, "ui.export");
    assert!(harness.state().ui.windows.export);

    // A file dialog: the commands that show one run, the rest wait
    let mut app = sample_app();
    file_dialog(&mut app);
    let mut harness = harness_of(app);
    assert!(board_enabled(&harness, "file.open"), "Ctrl+O brings the open dialog forward");
    assert!(!board_enabled(&harness, "ui.newGraph"));
    press(&mut harness, Modifiers::COMMAND, Key::N);
    assert!(!harness.state().ui.windows.new_graph, "Ctrl+N waits");
    let ctx = harness.ctx.clone();
    invoke(harness.state_mut(), &ctx, "ui.export");
    assert!(!harness.state().ui.windows.export, "ui.export waits");
    invoke(harness.state_mut(), &ctx, "file.open");
    assert!(crate::native_dialogs::pending(harness.state()), "file.open runs and the dialog stays the one asked for");
}

/// Explorer menu actions resolve through their documented UI or engine handler.
#[test]
fn explorer_menus_name_only_actions_that_exist() {
    for rows in [EXPLORER_PACKAGE, EXPLORER_FOLDER, EXPLORER_GRAPH] {
        for row in rows.iter().filter(|row| row.label != "-" && !row.id.is_empty()) {
            assert!(implemented(row.id) || row.id.starts_with("ui.explorer.") || row.id.starts_with("explorer."), "{}", row.id);
            if let Some(shortcut) = row.shortcut {
                assert!(parse_shortcut(shortcut).is_some(), "{}: {shortcut}", row.label);
            }
        }
    }
}

#[test]
fn menu_bar_items_gray_out_behind_a_prompt() {
    let mut app = sample_app();
    app.ui.recent.push("recent.sbs".into());
    let mut harness = harness_of(app);
    open_file_menu(&mut harness);
    assert!(!row(&harness, "Close all").accesskit_node().is_disabled());
    assert!(!row(&harness, "Recent packages").accesskit_node().is_disabled());

    prompt(harness.state_mut());
    harness.run_steps(3);
    assert!(row(&harness, "Close all").accesskit_node().is_disabled());
    assert!(row(&harness, "Recent packages").accesskit_node().is_disabled());
}

#[test]
fn open_stays_enabled_behind_a_file_dialog() {
    let mut app = sample_app();
    file_dialog(&mut app);
    let mut harness = harness_of(app);
    open_file_menu(&mut harness);
    assert!(!row(&harness, "Open...").accesskit_node().is_disabled());
    assert!(row(&harness, "Close all").accesskit_node().is_disabled());
}

#[test]
fn the_desktops_copy_event_copies_the_selected_nodes() {
    let mut app = sample_app();
    app.run("edit.selectAll", json!({}));
    let mut harness = harness_of(app);
    harness.event(Event::Copy);
    harness.run_steps(2);
    assert!(harness.state().session.clipboard.is_some(), "edit.copy takes the event");
}

#[test]
fn clipboard_events_with_nothing_to_copy_or_paste_stay_with_egui() {
    let mut app = sample_app();
    app.run("edit.selectNone", json!({}));
    app.session.clipboard = None;
    let mut harness = harness_of(app);
    let count = nodes(&harness);
    for id in ["edit.copy", "edit.cut", "edit.paste"] {
        assert!(!board_enabled(&harness, id), "{id} runs nothing, so it is off and its chord stays with egui");
    }

    for event in [Event::Copy, Event::Cut, Event::Paste("text from another app".into())] {
        harness.event(event);
        harness.step();
        assert!(harness.state().toasts.is_empty(), "no error toast for a chord that runs nothing");
    }
    harness.step();
    assert!(harness.state().session.clipboard.is_none());
    assert_eq!(nodes(&harness), count, "no command ran");
}

#[test]
fn a_web_copy_press_runs_edit_copy() {
    // PLAN: `install_runtime` picks the clipboard setting of `InstallSpec` by build target, so a native test sends the
    // web's press and its Copy event through the native install; the runtime's one-fire rule is shell-w3's trial.
    let mut app = sample_app();
    app.run("edit.selectAll", json!({}));
    let mut harness = harness_of(app);
    harness.input_mut().events.extend([
        Event::ModifiersChanged(Modifiers::COMMAND),
        Event::Key { key: Key::C, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND },
        Event::Copy,
    ]);
    harness.step();
    let offers = harness.output().platform_output.commands.iter().filter(|command| matches!(command, egui::OutputCommand::CopyText(_))).count();
    assert!(harness.state().session.clipboard.is_some(), "the press runs edit.copy");
    assert_eq!(offers, 1, "the press and its Copy event copy once");
}

/// A hidden Explorer must not delete its selection.
#[test]
fn review_hidden_explorer_ignores_delete() {
    let mut app = sample_app();
    let graphs = app.session.pkg.graphs.len();
    app.ui.explorer.selected = Some(crate::explorer::ExplorerTarget::Graph { document: app.session.document_id, index: 0 });
    app.ui.dock.close(crate::docking::Panel::Explorer);
    let mut harness = harness_of(app);

    press(&mut harness, Modifiers::NONE, Key::Delete);

    assert_eq!(harness.state().session.pkg.graphs.len(), graphs);
}
