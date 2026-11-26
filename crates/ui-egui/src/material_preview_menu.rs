use materialcraft_engine::{
    MaterialSource,
    core::{MaterialChannel, NodeId},
};
use serde_json::json;

/// Builds assignment rows for a node in `graph`, which need not be the open graph.
pub(crate) fn rows(app: &crate::MaterialCraftApp, graph: usize, node: NodeId) -> Vec<craft_menu::Node> {
    let Some(document) = app.session.pkg.graphs.get(graph) else {
        return Vec::new();
    };
    let Ok(node_ref) = document.node(node) else {
        return Vec::new();
    };
    let ports = if node_ref.def == "output" {
        vec![("output".to_owned(), materialcraft_engine::eval::identifier(node_ref))]
    } else {
        materialcraft_engine::eval::ports(&app.session.pkg, document, node_ref).1.into_iter().map(|port| (port.id, port.label)).collect()
    };
    let ports: Vec<_> = ports.into_iter().filter(|(port, _)| MaterialSource { graph, node, port: port.clone() }.is_image(&app.session.pkg)).collect();
    if ports.is_empty() {
        return Vec::new();
    }
    [("View in 3D view", false), ("Reset and view in 3D view", true)]
        .into_iter()
        .map(|(title, reset)| {
            let children = ports
                .iter()
                .map(|(port, label)| {
                    let channels: Vec<_> = MaterialChannel::ALL
                        .into_iter()
                        .map(|channel| {
                            let label = channel.label().to_owned();
                            craft_menu::Node::Item(craft_menu::Item::new(
                                craft_keymap::CommandRef::with_params(
                                    "view.3d.assign",
                                    json!({"graph": graph, "node": node.0, "port": port, "channel": channel.id(), "reset": reset}),
                                ),
                                crate::i18n::tr(&label),
                            ))
                        })
                        .collect();
                    if ports.len() == 1 { channels } else { vec![craft_menu::Node::submenu(crate::i18n::tr(label), channels)] }
                })
                .flatten()
                .collect();
            craft_menu::Node::submenu(crate::i18n::tr(title), children)
        })
        .collect()
}
