use serde_json::Value;

/// Builds one edit command row named `label` (English, translated here) with its parameters.
pub(crate) fn row(command: &str, label: &str, params: Value) -> craft_menu::Node {
    craft_menu::Node::Item(craft_menu::Item::new(craft_keymap::CommandRef::with_params(command, params), crate::i18n::tr(label)))
}

/// Builds the selection edit rows in their established menu order.
pub(crate) fn selection_rows(params: Value) -> Vec<craft_menu::Node> {
    let commands = [
        ("Paste", "edit.paste"),
        ("Paste without link", "edit.pasteWithoutLinks"),
        ("Select all", "edit.selectAll"),
        ("Copy selection", "edit.copy"),
        ("Cut selection", "edit.cut"),
        ("Delete selection", "edit.delete"),
        ("Delete and relink", "edit.deleteRelink"),
        ("Duplicate selection", "edit.duplicate"),
        ("Duplicate selection without link", "edit.duplicateWithoutLinks"),
        ("Select upstream nodes", "graph.selectUpstream"),
        ("Select downstream nodes", "graph.selectDownstream"),
        ("Disable selection", "graph.node.disable"),
    ];
    let mut rows = Vec::with_capacity(commands.len() + 1);
    for (label, command) in commands {
        if command == "edit.copy" {
            rows.push(craft_menu::Node::Separator);
        }
        let parameters = if matches!(command, "edit.paste" | "edit.pasteWithoutLinks") { params.clone() } else { Value::Null };
        rows.push(craft_menu::Node::Item(craft_menu::Item::new(craft_keymap::CommandRef::with_params(command, parameters), crate::i18n::tr(label))));
    }
    rows
}
