use crate::{MaterialCraftApp, function::FunctionTarget, i18n::tr};
use craft_ui::contract::{Show as _, ShowContainer as _};
use egui::Ui;
use materialcraft_engine::core::{FunctionGraph, Value};

pub(crate) fn title(app: &MaterialCraftApp) -> String {
    let kind = app.pattern_editor.as_ref().and_then(|editor| {
        let graph = editor.current(app)?;
        let inspected = crate::graph::node_selection::inspected(&editor.selection, editor.inspected_node);
        graph.nodes.iter().find(|node| Some(node.id) == inspected).map(|node| node.kind)
    });
    format!("{} - {}", kind.map_or("FX-Map", |kind| kind.label()), tr("Properties"))
}

pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let Some(mut editor) = app.pattern_editor.take() else { return };
    let Some(graph) = editor.current(app) else {
        app.pattern_editor = Some(editor);
        return;
    };
    let inspected = crate::graph::node_selection::inspected(&editor.selection, editor.inspected_node);
    let node_data = graph.nodes.iter().find(|node| Some(node.id) == inspected).cloned();
    let root = graph.root;
    let mut edits = Vec::new();
    let mut function_target = None;
    let mut inspector = craft_ui::patterns::panels::Inspector::new(tr("FX-Map")).empty(tr("Select a node to edit its parameters"));
    if let Some(node) = &node_data {
        inspector = inspector.target(node.id).header(tr(node.kind.label()));
    }
    let _ = inspector.show(ui, |ui, _| {
        let Some(node) = &node_data else { return };
        if root == Some(node.id) {
            let _ = craft_ui::display::Text::new(tr("Root node")).show(ui);
        }
        if let Some(definition) = materialcraft_engine::nodes::pattern::definition(node.kind) {
            for parameter in &definition.params {
                let _ = craft_ui::display::Text::new(tr(parameter.label)).show(ui);
                let _ = craft_ui::layout_components::Row::new().wrap(true).show(ui, |ui, _| {
                    let value = node.parameters.get(parameter.id).unwrap_or(&parameter.default);
                    if matches!(value, Value::Function(_)) {
                        if craft_ui::buttons::Button::new(tr("Edit function")).show(ui).response.clicked() {
                            function_target = Some(FunctionTarget::Pattern { node: node.id, parameter: parameter.id.to_string() });
                        }
                        if craft_ui::buttons::Button::new(tr("Reset")).show(ui).response.clicked() {
                            edits.push((materialcraft_engine::PatternEdit::Value { id: node.id, parameter: parameter.id.into(), value: None }, false));
                        }
                    } else {
                        super::parameter_value::show(app, ui, editor.node, node.id, parameter.id, value);
                        if craft_ui::buttons::Button::new(tr("Function")).show(ui).response.clicked() {
                            edits.push((
                                materialcraft_engine::PatternEdit::Value {
                                    id: node.id,
                                    parameter: parameter.id.into(),
                                    value: Some(Value::Function(Box::new(FunctionGraph::constant(value.clone())))),
                                },
                                false,
                            ));
                            function_target = Some(FunctionTarget::Pattern { node: node.id, parameter: parameter.id.to_string() });
                        }
                    }
                });
            }
        }
    });
    for (edit, merge) in edits {
        editor.edit(app, edit, merge);
    }
    if let Some(target) = function_target {
        crate::graph::subgraph_navigation::open_function(app, editor.node, target);
    }
    app.pattern_editor = Some(editor);
}
