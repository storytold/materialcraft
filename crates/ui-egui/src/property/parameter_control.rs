use crate::{MaterialCraftApp, i18n::tr, theme::Tokens};
use egui::{RichText, Ui};
use egui_phosphor::regular as icon;
use materialcraft_engine::{
    core::{BaseParameter, FunctionGraph, Node, ParameterInheritance, ValueType},
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
    let evaluated = (!app.session.dirty && !app.cooker.busy())
        .then(|| app.session.cooked.nodes.get(&node.id))
        .flatten()
        .filter(|result| result.error.is_none())
        .and_then(|result| base.map_or_else(|| result.parameters.get(&descriptor.id), |parameter| result.base_values.get(&parameter)))
        .cloned();
    let can_capture = function.is_none() || evaluated.is_some();
    let tokens = Tokens::get(ui.ctx());
    ui.push_id((node.id, base.is_some(), &descriptor.id), |ui| {
        let rtl = crate::i18n::is_rtl();
        let actions_width = 32.0 + if driven { 26.0 } else { 0.0 } + if base.is_some() { 26.0 } else { 0.0 };
        let row_width = ui.available_width();
        let row_geometry = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(row_width, ui.spacing().interact_size.y));
        let (content_rect, actions_rect) = parameter_header_rects(row_geometry, actions_width, rtl);
        let (content_layout, action_layout) = parameter_header_layouts(rtl);
        // Unlike a fixed-height row, a nested layout grows when a literal editor
        // contains multiple controls (e.g. a vector field plus its slider).
        // RTL puts the content first on the physical right, then the actions.
        ui.allocate_ui_with_layout(egui::vec2(row_width, ui.spacing().interact_size.y), parameter_row_layout(rtl), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.allocate_ui_with_layout(egui::vec2(content_rect.width(), 0.0), content_layout, |ui| {
                ui.set_width(content_rect.width());
                if base.is_some() || driven {
                    ui.label(RichText::new(&descriptor.label).color(if driven { tokens.accent } else { tokens.text }));
                } else {
                    literal(app, ui);
                }
            });
            ui.allocate_ui_with_layout(egui::vec2(actions_rect.width(), 0.0), action_layout, |ui| {
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

/// Split a header into fixed, disjoint content and action rectangles.
/// RTL mirrors their physical placement, not only their text/layout directions.
fn parameter_header_rects(row: egui::Rect, actions_width: f32, rtl: bool) -> (egui::Rect, egui::Rect) {
    let actions_width = actions_width.clamp(0.0, row.width());
    let content_width = (row.width() - actions_width - 4.0).max(0.0);
    let content = if rtl {
        egui::Rect::from_min_size(egui::pos2(row.right() - content_width, row.top()), egui::vec2(content_width, row.height()))
    } else {
        egui::Rect::from_min_size(row.min, egui::vec2(content_width, row.height()))
    };
    let actions = if rtl {
        egui::Rect::from_min_size(row.min, egui::vec2(actions_width, row.height()))
    } else {
        egui::Rect::from_min_size(egui::pos2(row.right() - actions_width, row.top()), egui::vec2(actions_width, row.height()))
    };
    (content, actions)
}

/// Place the content column against the active reading edge.
/// The parent grows vertically to the tallest child instead of clipping at 24px.
fn parameter_row_layout(rtl: bool) -> egui::Layout {
    if rtl { egui::Layout::right_to_left(egui::Align::Min) } else { egui::Layout::left_to_right(egui::Align::Min) }
}

/// Vertical text/slider content follows the reading edge; controls sit on
/// the opposing side while keeping their original logical ordering.
fn parameter_header_layouts(rtl: bool) -> (egui::Layout, egui::Layout) {
    if rtl {
        (egui::Layout::top_down(egui::Align::Max), egui::Layout::left_to_right(egui::Align::Center))
    } else {
        (egui::Layout::top_down(egui::Align::Min), egui::Layout::right_to_left(egui::Align::Center))
    }
}

#[cfg(test)]
mod rtl_layout_tests {
    use super::*;

    #[test]
    fn inspector_header_rectangles_are_mirrored_without_overlap_or_clipping() {
        for width in [240.0, 120.0, 72.0] {
            let row = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(width, 24.0));
            for rtl in [false, true] {
                let (content, actions) = parameter_header_rects(row, 58.0, rtl);
                assert!(row.contains(content.min) && row.contains(content.max));
                assert!(row.contains(actions.min) && row.contains(actions.max));
                assert!(!content.intersects(actions), "header columns overlap in rtl={rtl}");
                if rtl {
                    assert_eq!(content.right(), row.right());
                    assert_eq!(actions.left(), row.left());
                } else {
                    assert_eq!(content.left(), row.left());
                    assert_eq!(actions.right(), row.right());
                }
            }
        }
    }

    #[test]
    fn inspector_header_widgets_remain_in_their_allocated_rectangles() {
        for rtl in [false, true] {
            let ctx = egui::Context::default();
            let mut seen = None;
            let _ = ctx
                .run_ui(egui::RawInput::default(), |ui| {
                    let row = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(260.0, 30.0));
                    let (content, actions) = parameter_header_rects(row, 58.0, rtl);
                    let (content_layout, action_layout) = parameter_header_layouts(rtl);
                    let mut label_rect = egui::Rect::NOTHING;
                    let mut button_rect = egui::Rect::NOTHING;
                    ui.scope_builder(egui::UiBuilder::new().max_rect(content).layout(content_layout), |ui| {
                        ui.set_clip_rect(ui.clip_rect().intersect(content));
                        ui.set_width(content.width());
                        label_rect = ui.label("Width").rect;
                    });
                    ui.scope_builder(egui::UiBuilder::new().max_rect(actions).layout(action_layout), |ui| {
                        ui.set_clip_rect(ui.clip_rect().intersect(actions));
                        button_rect = ui.button("...").rect;
                    });
                    seen = Some((content, actions, label_rect, button_rect));
                })
                .textures_delta
                .clear();
            let (content, actions, label, button) = seen.expect("egui rendered widgets");
            assert!(content.contains(label.min) && content.contains(label.max), "label outside content in rtl={rtl}");
            assert!(actions.contains(button.min) && actions.contains(button.max), "button outside action slot in rtl={rtl}");
            assert!(!label.intersects(button), "label and action overlap in rtl={rtl}");
            if rtl {
                assert!(label.center().x > button.center().x, "RTL label must be right of its action");
            } else {
                assert!(label.center().x < button.center().x, "LTR label must be left of its action");
            }
        }
    }

    #[test]
    fn multi_line_literal_allocates_its_full_height_in_ltr_and_rtl() {
        for rtl in [false, true] {
            let ctx = egui::Context::default();
            let mut geometry = None;
            let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
                let width = 260.0;
                let min_height = ui.spacing().interact_size.y;
                let row = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, min_height));
                let (content_rect, actions_rect) = parameter_header_rects(row, 58.0, rtl);
                let (content_layout, action_layout) = parameter_header_layouts(rtl);
                let mut value = 0.5;
                let row = ui.allocate_ui_with_layout(egui::vec2(width, min_height), parameter_row_layout(rtl), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let editor = ui.allocate_ui_with_layout(egui::vec2(content_rect.width(), 0.0), content_layout, |ui| {
                        ui.label("Vector component");
                        ui.add(egui::Slider::new(&mut value, 0.0..=1.0));
                    });
                    let actions = ui.allocate_ui_with_layout(egui::vec2(actions_rect.width(), 0.0), action_layout, |ui| {
                        let _ = ui.button("...");
                    });
                    (editor.response.rect, actions.response.rect)
                });
                geometry = Some((row.response.rect, row.inner.0, row.inner.1, min_height));
            });
            frame.textures_delta.clear();
            let (row, editor, actions, min_height) = geometry.expect("row drawn");
            assert!(row.height() > min_height * 1.5, "literal editor clipped in rtl={rtl}: {row:?}");
            assert!(row.bottom() >= editor.bottom(), "editor exceeds row in rtl={rtl}");
            assert!(!editor.intersects(actions), "editor overlaps action buttons in rtl={rtl}");
            if rtl {
                assert!(editor.center().x > actions.center().x);
            } else {
                assert!(editor.center().x < actions.center().x);
            }
        }
    }

    #[test]
    fn inspector_parameter_header_flips_both_label_and_action_sides() {
        let (en_content, en_actions) = parameter_header_layouts(false);
        let (he_content, he_actions) = parameter_header_layouts(true);
        assert_eq!(en_content.main_dir, egui::Direction::TopDown);
        assert_eq!(en_content.cross_align, egui::Align::Min);
        assert_eq!(en_actions.main_dir, egui::Direction::RightToLeft);
        assert_eq!(he_content.main_dir, egui::Direction::TopDown);
        assert_eq!(he_content.cross_align, egui::Align::Max);
        assert_eq!(he_actions.main_dir, egui::Direction::LeftToRight);
    }
}
