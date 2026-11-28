use crate::{MaterialCraftApp, i18n::tr, theme::Tokens};
use egui::{RichText, Ui};
use egui_phosphor::regular as icon;
use materialcraft_engine::{
    core::{BaseParameter, FunctionGraph, Node, ParameterInheritance, Value, ValueType},
    nodes::ParameterDescriptor,
};
use serde_json::json;

pub(super) fn header(app: &mut MaterialCraftApp, ui: &mut Ui, node: &Node, descriptor: &ParameterDescriptor, base: BaseParameter) -> bool {
    show(app, ui, node, descriptor, Some(base), |_, _| {})
}

pub(super) fn show(
    app: &mut MaterialCraftApp,
    ui: &mut Ui,
    node: &Node,
    descriptor: &ParameterDescriptor,
    base: Option<BaseParameter>,
    literal: impl FnOnce(&mut MaterialCraftApp, &mut Ui),
) -> bool {
    let function = base.map_or_else(|| node.functions.get(&descriptor.id), |parameter| node.base.functions.get(&parameter));
    let bound = function
        .and_then(FunctionGraph::input_identifier)
        .filter(|identifier| app.session.graph().is_ok_and(|graph| graph.exposed.iter().any(|input| input.id == *identifier)))
        .or_else(|| base.is_none().then(|| node.bindings.get(&descriptor.id).map(String::as_str)).flatten());
    let driven = function.is_some() || bound.is_some();
    let evaluated = crate::cook::preview_ready(app)
        .then(|| app.session.cooked.nodes.get(&node.id))
        .flatten()
        .filter(|result| result.error.is_none())
        .and_then(|result| base.map_or_else(|| result.parameters.get(&descriptor.id), |parameter| result.base_values.get(&parameter)))
        .cloned();
    let can_capture = function.is_none() || evaluated.is_some();
    let tokens = Tokens::get(ui.ctx());
    ui.push_id((node.id, base.is_some(), &descriptor.id), |ui| {
        let actions_width = 32.0 + if driven { 26.0 } else { 0.0 } + if base.is_some() { 26.0 } else { 0.0 };
        let content_width = (ui.available_width() - actions_width).max(40.0);
        // The curve and gradient editors are taller than one control row. Reserve their height so
        // the rows that follow do not overlap the lower part of their pointer area.
        let literal_height = if base.is_some() || driven {
            0.0
        } else {
            match &descriptor.default {
                Value::Curve(_) => content_width.min(240.0) * 0.75 + ui.spacing().interact_size.y + ui.spacing().item_spacing.y,
                Value::Gradient(_) => 30.0 + ui.spacing().interact_size.y + ui.spacing().item_spacing.y,
                _ => 0.0,
            }
        };
        let row_height = ui.spacing().interact_size.y.max(literal_height);
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_height), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.vertical(|ui| {
                ui.set_width(content_width);
                if base.is_some() || driven {
                    ui.label(RichText::new(&descriptor.label).color(if driven { tokens.accent } else { tokens.text }));
                } else {
                    literal(app, ui);
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let menu = ui.menu_button(icon::DOTS_THREE, |ui| {
                    for (mode, label) in [("empty", "Empty function"), ("constant", "Constant value")] {
                        if ui.add_enabled(mode == "empty" || can_capture, egui::Button::new(tr(label))).clicked() {
                            let parameters = json!({
                                "id": node.id, "param": descriptor.id, "base": base.is_some(), "mode": mode,
                                "value": (mode == "constant").then_some(evaluated.as_ref()).flatten()
                            });
                            if app.run("graph.node.function", parameters).is_some() {
                                edit(app, node, descriptor, base);
                            }
                            ui.close();
                        }
                    }
                    if driven && ui.button(tr("Edit function")).clicked() {
                        edit(app, node, descriptor, base);
                        ui.close();
                    }
                    if let Some(input) = bound
                        && ui.button(format!("{}: {input}", tr("Edit exposed input"))).clicked()
                    {
                        app.inspected_graph = Some(materialcraft_engine::GraphTarget::active(&app.session));
                        ui.close();
                    }
                    if base.is_none() {
                        ui.separator();
                        if ui.add_enabled(can_capture, egui::Button::new(tr("Expose as new graph input"))).clicked() {
                            super::ExposeParameterDialog::open(app, node.id, &descriptor.id, &descriptor.label, evaluated.as_ref());
                            ui.close();
                        }
                        let inputs = app
                            .session
                            .graph()
                            .map(|graph| {
                                graph
                                    .exposed
                                    .iter()
                                    .filter(|input| ValueType::of(&input.default) == ValueType::of(&descriptor.default))
                                    .map(|input| (input.id.clone(), input.label.clone()))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        if !inputs.is_empty() {
                            ui.menu_button(tr("Bind to input"), |ui| {
                                for (identifier, label) in inputs {
                                    if ui.button(label).clicked() {
                                        app.run("graph.node.bind", json!({"id": node.id, "param": descriptor.id, "base": base.is_some(), "input": identifier}));
                                        ui.close();
                                    }
                                }
                            });
                        }
                    }
                    ui.separator();
                    if ui.button(tr("Reset to default")).clicked() {
                        app.run("graph.node.reset", json!({"id": node.id, "param": descriptor.id, "base": base.is_some()}));
                        ui.close();
                    }
                });
                crate::widgets::name(&menu.response, egui::Role::Button, &format!("Parameter actions: {}", descriptor.label));
                if driven && crate::widgets::icon_button(ui, icon::FUNCTION, tr("Edit function"), false, 18.0).clicked() {
                    edit(app, node, descriptor, base);
                }
                if let Some(parameter) = base {
                    inheritance(app, ui, node, parameter);
                }
            });
        });
    });
    if let Some(input) = bound {
        ui.label(RichText::new(format!("{} {input}", icon::LINK)).small().color(tokens.accent));
    }

    driven
}

fn edit(app: &mut MaterialCraftApp, node: &Node, descriptor: &ParameterDescriptor, base: Option<BaseParameter>) {
    if base.is_none()
        && let Some(input) = node.bindings.get(&descriptor.id)
        && app.run("graph.node.bind", json!({"id": node.id, "param": descriptor.id, "base": base.is_some(), "input": input})).is_none()
    {
        return;
    }

    app.run("graph.node.function.edit", json!({"id": node.id, "param": descriptor.id, "base": base.is_some()}));
}

fn inheritance(app: &mut MaterialCraftApp, ui: &mut Ui, node: &Node, parameter: BaseParameter) {
    let current = parameter.inheritance(&node.base);
    let modes = [
        (ParameterInheritance::RelativeToInput, "Relative to input"),
        (ParameterInheritance::RelativeToParent, "Relative to parent"),
        (ParameterInheritance::Absolute, "Absolute"),
    ];
    let label = modes.iter().find(|(mode, _)| *mode == current).map_or("", |(_, label)| *label);
    let symbol = match current {
        ParameterInheritance::RelativeToInput => icon::SIGN_IN,
        ParameterInheritance::RelativeToParent => icon::TREE_STRUCTURE,
        ParameterInheritance::Absolute => icon::EQUALS,
    };
    let menu = ui.menu_button(symbol, |ui| {
        for (mode, label) in modes {
            if ui.selectable_label(mode == current, tr(label)).clicked() {
                app.run("graph.node.base", json!({"id": node.id, "param": parameter.identifier(), "inheritance": mode}));
                ui.close();
            }
        }
    });
    crate::widgets::name(&menu.response, egui::Role::Button, &format!("{} inheritance: {label}", parameter.label()));
    menu.response.on_hover_text(tr(label));
}

#[cfg(test)]
mod tests {
    use egui_kittest::{
        Harness,
        kittest::{By, Queryable},
    };
    use serde_json::json;

    /// A curve editor is taller than a control row; the row that follows must start below it, or
    /// its pointer area covers the lower part of the curve and the point there cannot be dragged.
    #[test]
    fn a_tall_literal_editor_reserves_its_height() {
        let mut app = crate::MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        let id = materialcraft_engine::core::NodeId(app.run("graph.node.add", json!({"def": "curve"})).unwrap()["id"].as_u64().unwrap());
        app.run("graph.select", json!({"ids": [id.0]})).unwrap();
        let mut harness = Harness::builder().with_size(egui::vec2(420.0, 900.0)).build_ui_state(
            move |ui, app: &mut crate::MaterialCraftApp| {
                let node = app.session.graph().unwrap().node(id).unwrap().clone();
                super::super::node::show(app, ui, &node);
                let _ = ui.button("after the curve row");
            },
            app,
        );
        harness.run_steps(3);

        let canvas = harness.get(By::new().role(egui::Role::Canvas).label("Curve editor")).rect();
        let after = harness.get(By::new().role(egui::Role::Button).label("after the curve row")).rect();
        assert!(after.top() >= canvas.bottom(), "the next row overlaps the curve editor: canvas {canvas:?}, next row {after:?}");
    }
}
