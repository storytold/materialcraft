use serde_json::json;

use super::shown::Shown;
use crate::MaterialCraftApp;

/// One entry of the 2D view's output menu
pub(super) struct OutputChoice {
    pub(super) label: String,
    pub(super) current: bool,
    /// What `view.2d` runs with to show it
    pub(super) parameters: serde_json::Value,
}

impl OutputChoice {
    /// The outputs to choose from: every output of the graph when an output node is shown, else the outputs of the shown node
    pub(super) fn all(app: &MaterialCraftApp, shown: &Shown) -> Vec<OutputChoice> {
        let Ok(graph) = app.session.graph() else { return Vec::new() };

        let shows_output = graph.node(shown.node).is_ok_and(|node| node.def == "output");
        if shows_output {
            return graph
                .nodes
                .iter()
                .filter(|node| node.def == "output")
                .map(|node| {
                    let (label, identifier) = (crate::graph::node_label(node), materialcraft_engine::eval::identifier(node));
                    let label = if label == identifier { label } else { format!("{label} ({identifier})") };
                    OutputChoice { label, current: node.id == shown.node, parameters: json!({"node": node.id.0}) }
                })
                .collect();
        }

        let port = if app.session.view2d.node == Some(shown.node) { app.session.view2d.port } else { 0 };
        app.session
            .output_ports(shown.node)
            .into_iter()
            .enumerate()
            .map(|(index, label)| OutputChoice { label, current: index == port, parameters: json!({"node": shown.node.0, "port": index}) })
            .collect()
    }
}
