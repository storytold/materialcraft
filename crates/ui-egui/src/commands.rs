//! MaterialCraft's commands as the framework runs them: the one place that builds `CommandDef`s, and the one path
//! that runs a command by reference.

use craft_keymap::{CommandRef, CommandScope, Platform};
use craft_menu::MenuBar;
use craft_tracked::Tracked;
use craft_ui::runtime::commands::{AppCatalog, CommandStates, Fired};
use materialcraft_engine::NodeLibrary;

use crate::MaterialCraftApp;

/// The scope of the graph editor's keys: node creation, shown while the graph canvas has the keys.
#[allow(dead_code)] // P3(shell-w5): sh.A13
pub(crate) const GRAPH_SCOPE: CommandScope = CommandScope::new("materialcraft.graph");

/// The scope of the 2D view's keys.
#[allow(dead_code)] // P3(shell-w5): sh.A13
pub(crate) const VIEW2D_SCOPE: CommandScope = CommandScope::new("materialcraft.view2d");

/// The scope of the 3D view's keys.
#[allow(dead_code)] // P3(shell-w5): sh.A13
pub(crate) const VIEW3D_SCOPE: CommandScope = CommandScope::new("materialcraft.view3d");

/// Every command of the app, including `node.add.<entry id>` per node entry of `library`.
#[allow(dead_code)] // P3(shell-w5): sh.A13
pub(crate) fn catalog(platform: Platform, library: &NodeLibrary) -> AppCatalog {
    let _ = (platform, library);
    todo!("sh.A13")
}

/// Which commands are enabled, checked and shown, for this frame.
#[allow(dead_code)] // P3(shell-w5): sh.A13
pub(crate) fn states(app: &MaterialCraftApp, ctx: &egui::Context, into: &mut Tracked<CommandStates>) {
    let _ = (app, ctx, into);
    todo!("sh.A13")
}

/// Runs the commands that fired since the last frame.
#[allow(dead_code)] // P3(shell-w5): sh.A13
pub(crate) fn run_fired(app: &mut MaterialCraftApp, ctx: &egui::Context, fired: &[Fired]) {
    let _ = (app, ctx, fired);
    todo!("sh.A13")
}

/// Runs one command by reference: the app's UI actions or the engine command of that id.
#[allow(dead_code)] // P3(shell-w5): sh.A13
pub(crate) fn run(app: &mut MaterialCraftApp, ctx: &egui::Context, command: &CommandRef) {
    let _ = (app, ctx, command);
    todo!("sh.A13")
}

/// The menu bar the shell draws, from the menu tables.
#[allow(dead_code)] // P3(shell-w5): sh.A13
pub(crate) fn menu_bar(app: &MaterialCraftApp, bar: &mut Tracked<MenuBar>) {
    let _ = (app, bar);
    todo!("sh.A13")
}
