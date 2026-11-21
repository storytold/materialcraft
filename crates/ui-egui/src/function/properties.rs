use egui::Ui;
use materialcraft_engine::{core::FunctionGraph, nodes::function::node_label};

use crate::{MaterialCraftApp, graph::node_selection, i18n::tr};

fn current(app: &MaterialCraftApp) -> Option<(&FunctionGraph, u32)> {
    let editor = app.function_editor.as_ref()?;
    let graph = editor.current(app)?;
    let id = node_selection::inspected(&editor.selection, editor.inspected_node)?;
    graph.node(id)?;
    Some((graph, id))
}

pub(crate) fn title(app: &MaterialCraftApp) -> Option<String> {
    let (graph, id) = current(app)?;
    let node = graph.node(id)?;
    let label = node_label(node);
    Some(format!("{label} - {}", tr("Properties")))
}

fn instance_source_label(package: &str) -> String {
    format!("{}: {}", tr("Source"), if package.is_empty() { tr("This package") } else { package })
}

pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) -> bool {
    let Some((graph, id)) = current(app) else { return false };

    let Some(node) = graph.node(id) else { return false };

    let label = node_label(node);
    ui.heading(tr(label));
    if materialcraft_engine::nodes::function::output_node(graph).is_some_and(|node| node.id == id) {
        ui.label(tr("Output node"));
    }

    if let Some(instance) = &node.instance {
        ui.label(instance_source_label(&instance.package));
        ui.label(&instance.graph);
        if let Some(Err(error)) = &node.source {
            ui.colored_label(egui::Color32::from_rgb(255, 90, 180), error);
        }
    }
    if node.missing.is_some() {
        ui.colored_label(egui::Color32::from_rgb(255, 90, 180), tr("Missing function operation"));
        return true;
    }

    let mut value = node.value.clone();
    let changed = if (node.op.starts_with("get_") || node.op == "isdefined")
        && let Some(materialcraft_engine::core::Value::Text(name)) = &mut value
    {
        let variables = app
            .function_editor
            .as_ref()
            .and_then(|editor| editor.command_target())
            .and_then(|target| target.variables(&app.session, id).ok())
            .unwrap_or_default();
        super::variable_picker::show(ui, id, name, &variables)
    } else {
        value.as_mut().is_some_and(|value| crate::value_editor::show(ui, value))
    };
    if changed
        && let Some(value) = value
        && let Some(mut editor) = app.function_editor.take()
    {
        editor.edit(app, materialcraft_engine::FunctionEdit::Value { id, value }, true);
        app.function_editor = Some(editor);
    }

    true
}

#[cfg(test)]
mod hebrew_instance_source_tests {
    use super::instance_source_label;

    #[test]
    fn function_source_in_active_inspector_localizes_english_and_hebrew_without_changing_package_name() {
        let before = crate::i18n::current().code.to_string();
        crate::i18n::set_language("he");
        assert_eq!(instance_source_label(""), "מקור: החבילה הזו");
        assert_eq!(instance_source_label("vendor.package"), "מקור: vendor.package");
        crate::i18n::set_language("en");
        assert_eq!(instance_source_label(""), "Source: This package");
        assert_eq!(instance_source_label("vendor.package"), "Source: vendor.package");
        crate::i18n::set_language(&before);
    }
}
