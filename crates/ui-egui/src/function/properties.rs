use crate::{MaterialCraftApp, graph::node_selection, i18n::tr};
use egui::Ui;
use materialcraft_engine::{
    core::{FunctionGraph, Value},
    nodes::function::node_label,
};

fn current(app: &MaterialCraftApp) -> Option<(&FunctionGraph, u32)> {
    let editor = app.function_editor.as_ref()?;
    let graph = editor.current(app)?;
    let id = node_selection::inspected(&editor.selection, editor.inspected_node)?;
    graph.node(id)?;
    Some((graph, id))
}

pub(crate) fn title(app: &MaterialCraftApp) -> Option<String> {
    let (graph, id) = current(app)?;
    let label = node_label(graph.node(id)?);
    Some(format!("{label} - {}", tr("Properties")))
}

pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) -> bool {
    let Some((graph, id)) = current(app) else { return false };
    let Some(node) = graph.node(id) else { return false };
    let label = node_label(node);
    let op = node.op.clone();
    let value = node.value.clone();
    let instance = node.instance.clone();
    let source = node.source.clone();
    let missing = node.missing.is_some();
    let output = materialcraft_engine::nodes::function::output_node(graph).is_some_and(|n| n.id == id);
    let target = app.function_editor.as_ref().and_then(|e| e.command_target());
    let Some(target) = target else { return false };
    craft_ui::patterns::Inspector::new(tr("Properties")).target(id).header(tr(label)).show(ui, |ui, _| {
        if output {
            let _ = craft_ui::display::Text::new(tr("Output node")).show(ui);
        }
        if let Some(instance) = &instance {
            let package = if instance.package.is_empty() { tr("This package") } else { tr(&instance.package) };
            let _ = craft_ui::display::Text::new(format!("{}: {package}", tr("Source"))).show(ui);
            let _ = craft_ui::display::Text::new(tr(&instance.graph)).show(ui);
            if let Some(Err(error)) = &source {
                let _ = craft_ui::display::Text::new(error).tone(craft_theme::variant::Tone::Bad).show(ui);
            }
        }
        if missing {
            let _ = craft_ui::display::Text::new(tr("Missing function operation")).tone(craft_theme::variant::Tone::Bad).show(ui);
            return;
        }
        let Some(mut value) = value else { return };
        let events = if (op.starts_with("get_") || op == "isdefined")
            && let Value::Text(name) = &value
        {
            let entries: Vec<_> = target
                .variables(&app.session, id)
                .ok()
                .unwrap_or_default()
                .into_iter()
                .map(|v| craft_ui::contract::Entry::new(v.name.clone(), v.name))
                .collect();
            let mut shown = name.clone();
            let output = craft_ui::pickers::ComboBox::new(tr("Variable name"), craft_ui::contract::Bind::bound(&mut shown), &entries)
                .placeholder(tr("Variable name"))
                .show(ui);
            output.edit.events.map(Value::Text)
        } else {
            crate::value_editor::show(ui, &value)
        };
        let _ = crate::edit_commit::send(
            app,
            &events,
            "graph.function.edit",
            |value| serde_json::json!({"target":target, "edit":{"action":"value", "id":id, "value":value}, "merge":true}),
        );
    });
    true
}
