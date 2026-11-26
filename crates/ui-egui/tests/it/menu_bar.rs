//! The menu bar's own items: suspending and resuming the engine, the toolbar switch and the Plugin Manager

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};
use materialcraft_ui_egui::{MaterialCraftApp, UiState, menus};

fn harness() -> Harness<'static, MaterialCraftApp> {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
    app.ui.windows.home = false;
    let mut harness = Harness::builder().with_size(vec2(1600.0, 960.0)).with_pixels_per_point(1.0).build_eframe(move |cc| {
        MaterialCraftApp::install_runtime(&cc.egui_ctx, None).expect("runtime");
        app
    });
    harness.run_steps(6);
    harness
}

/// Opens the menu titled `menu` and picks its row whose name holds `item`.
fn choose(harness: &mut Harness<'static, MaterialCraftApp>, menu: &str, item: &str) {
    harness.get(By::new().role(Role::MenuItem).label(menu)).click();
    harness.run_steps(3);
    let rows = [Role::MenuItem, Role::MenuItemCheckBox, Role::MenuItemRadio];
    harness.get_all(By::new().label_contains(item)).find(|node| rows.contains(&node.accesskit_node().role())).expect("the menu lists the item").click();
    harness.run_steps(3);
}

/// Whether the main toolbar shows, found by its Open button's command label.
fn toolbar_shown(harness: &Harness<'static, MaterialCraftApp>) -> bool {
    let open = craft_ui::runtime::commands::resolve(&harness.ctx, "file.open").label.clone();
    harness.query_all(By::new().role(Role::Toolbar)).any(|bar| bar.query(By::new().role(Role::Button).label(&open)).is_some())
}

#[test]
fn a_suspended_engine_cooks_nothing_until_it_is_resumed() {
    let mut harness = harness();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        harness.step();
        if !harness.state().cooker.busy() && !harness.state().session.dirty {
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!harness.state().cooker.busy() && !harness.state().session.dirty, "the initial cook must settle");

    choose(&mut harness, "Tools", "Suspend engine");
    assert!(harness.state().cooker.suspended);
    assert!(!menus::enabled(harness.state(), "graph.cook"));

    harness.state_mut().session.dirty = true;
    harness.run_steps(5);
    assert!(!harness.state().cooker.busy() && harness.state().session.dirty, "the edit waits");

    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().cooker.suspended, "Shift+Esc resumes the engine");
    assert!(harness.state().cooker.busy() || !harness.state().session.dirty, "and what changed cooks");
}

#[test]
fn cancel_rendering_is_only_offered_while_a_cook_runs() {
    let harness = harness();
    assert_eq!(menus::enabled(harness.state(), "ui.cook.cancel"), harness.state().cooker.busy());
}

#[test]
fn the_main_toolbar_and_the_plugin_manager_open_from_the_menus() {
    let mut harness = harness();
    assert!(toolbar_shown(&harness));

    choose(&mut harness, "Windows", "Main toolbar");
    assert!(!harness.state().ui.prefs.show_toolbar);
    assert!(!toolbar_shown(&harness), "the toolbar is hidden");

    choose(&mut harness, "Tools", "Plugin Manager...");
    assert!(harness.state().ui.windows.plugin_manager);
    assert!(harness.query_by_label_contains("MaterialCraft does not load plugins yet").is_some());
}
