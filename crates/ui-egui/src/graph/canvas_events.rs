//! Maps the framework node graph's events onto commands.

use craft_ui::graph::NodeGraphResponse;

use crate::MaterialCraftApp;

/// Maps the events, calls `annotations::apply`, `context_menu::show`, `wire_actions::wire_menu` and the node search
/// open; a graph change queues a fit.
#[allow(dead_code)] // P3(graph-w6): ge.A3
pub(crate) fn apply(app: &mut MaterialCraftApp, ui: &mut egui::Ui, response: &NodeGraphResponse<()>, nodes: &[materialcraft_engine::core::NodeId]) {
    let _ = (app, ui, response, nodes);
    todo!("ge.A3")
}
