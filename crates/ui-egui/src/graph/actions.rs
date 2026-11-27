use crate::{MaterialCraftApp, docking::Panel};
use craft_behavior::canvas::CanvasView;
use craft_ui::canvas::{ViewCommands, ViewRequest};
use egui::pos2;
use serde_json::{Value, json};

/// Reports whether a graph command is available for the active editor and its parameters.
pub(crate) fn enabled(app: &MaterialCraftApp, command: &str, params: &Value) -> Option<bool> {
    let target = super::editor_target::EditorTarget::resolve(app);
    let refs = match target {
        super::editor_target::EditorTarget::Material => app.graph.selected_wire_refs(),
        super::editor_target::EditorTarget::Function => app.function_editor.as_ref().map_or_else(Vec::new, |editor| editor.selected_wire_refs(app)),
        super::editor_target::EditorTarget::Pattern => app.pattern_editor.as_ref().map_or_else(Vec::new, |editor| editor.selected_wire_refs(app)),
    };
    let selection_target = match target {
        super::editor_target::EditorTarget::Function => {
            app.function_editor.as_ref().and_then(|editor| editor.command_target()).map(materialcraft_engine::SelectionTarget::Function)
        }
        super::editor_target::EditorTarget::Pattern => app.pattern_editor.as_ref().map(|editor| materialcraft_engine::SelectionTarget::Pattern(editor.node)),
        super::editor_target::EditorTarget::Material => None,
    };
    if let Some(enabled) = super::wire_actions::enabled(app, command, &refs, selection_target.as_ref()) {
        return Some(enabled);
    }
    let selected = match target {
        super::editor_target::EditorTarget::Material => app.session.selection.len(),
        super::editor_target::EditorTarget::Function => app.function_editor.as_ref().map_or(0, |editor| editor.selection.len()),
        super::editor_target::EditorTarget::Pattern => app.pattern_editor.as_ref().map_or(0, |editor| editor.selection.len()),
    };
    match command {
        "graph.align" => Some(selected > 1),
        "ui.graph.quickAdd" => Some(target != super::editor_target::EditorTarget::Pattern && params.get("def").and_then(Value::as_str).is_some()),
        "ui.graph.nodeSearch" | "ui.graph.frame" => Some(true),
        "edit.delete" | "edit.deleteRelink" if target == super::editor_target::EditorTarget::Material && refs.is_empty() => return None,
        "edit.delete" | "edit.deleteRelink" if target == super::editor_target::EditorTarget::Material => Some(true),
        "graph.wire.swap" | "graph.wire.delete" | "edit.delete" if !refs.is_empty() => {
            super::wire_actions::enabled(app, command, &refs, selection_target.as_ref())
        }
        "ui.graph.viewOutputs" | "ui.graph.resetViewOutputs" => Some(app.session.graph().is_ok_and(|graph| graph.function.is_none())),
        "ui.graph.previousPin" | "ui.graph.nextPin" => Some(app.graph.annotation_target.items(&app.session).is_ok_and(|items| !items.pins.is_empty())),
        "ui.graph.copyInfo" | "ui.graph.view2d" => Some(true),
        command if command.starts_with("ui.graph.") => Some(app.session.graph().is_ok()),
        _ => None,
    }
}

/// Returns a canvas view centred on the next or previous pin.
pub(crate) fn pin_target(app: &MaterialCraftApp, forward: bool) -> Option<CanvasView> {
    let graph = app.graph.annotation_target.items(&app.session).ok()?;
    let view = crate::annotation::camera(app);
    let center = view.center;
    let current = graph.pins.iter().position(|pin| (f64::from(pin.pos[0]) - center.x).abs() < 0.1 && (f64::from(pin.pos[1]) - center.y).abs() < 0.1);
    let count = graph.pins.len();
    if count == 0 {
        return None;
    }
    let index = match (current, forward) {
        (Some(index), true) => (index + 1) % count,
        (Some(index), false) => (index + count - 1) % count,
        (None, true) => 0,
        (None, false) => count - 1,
    };
    let pin = graph.pins.get(index)?;
    let mut target = view;
    target.center = craft_behavior::kurbo::Point::new(f64::from(pin.pos[0]), f64::from(pin.pos[1]));
    Some(target)
}

/// Runs a graph action, including command parameters and editor-specific routing.
pub(crate) fn invoke(app: &mut MaterialCraftApp, ctx: &egui::Context, command: &str, params: &Value) -> bool {
    use super::editor_target::EditorTarget;
    let target = EditorTarget::resolve(app);
    match command {
        "graph.wire.swap" => return super::wire_actions::swap_selected_wires(app),
        "graph.wire.delete" => return super::wire_actions::delete_selected_wires(app),
        "edit.delete" | "edit.deleteRelink" => {
            let refs = match target {
                EditorTarget::Material => app.graph.selected_wire_refs(),
                EditorTarget::Function => app.function_editor.as_ref().map_or_else(Vec::new, |editor| editor.selected_wire_refs(app)),
                EditorTarget::Pattern => app.pattern_editor.as_ref().map_or_else(Vec::new, |editor| editor.selected_wire_refs(app)),
            };
            if !refs.is_empty() {
                return super::wire_actions::delete_selected_wires(app);
            }
            if target != EditorTarget::Material {
                return false;
            }
            if app.session.selection.is_empty() {
                return false;
            }
        }
        "graph.align" => {
            let Some(mode) = params.get("mode").and_then(Value::as_str) else { return false };
            let mut parameters = match target {
                EditorTarget::Material => json!({"graph": app.session.graph, "ids": app.session.selection.iter().map(|id| id.0).collect::<Vec<_>>() }),
                EditorTarget::Function => app.function_editor.as_ref().map_or(Value::Null, |editor| editor.selection_parameters(command)),
                EditorTarget::Pattern => app.pattern_editor.as_ref().map_or(Value::Null, |editor| editor.selection_parameters(command)),
            };
            if let Some(object) = parameters.as_object_mut() {
                object.insert("mode".into(), Value::String(mode.into()));
            }
            app.run("graph.align", parameters);
            return true;
        }
        "ui.graph.quickAdd" => {
            let Some(def) = params.get("def").and_then(Value::as_str) else { return false };
            match target {
                EditorTarget::Material => super::navigation::add_node_near_selection(app, def),
                EditorTarget::Function => crate::function::library::add_near_selection(app, def),
                EditorTarget::Pattern => return false,
            }
            return true;
        }
        "ui.graph.nodeSearch" => {
            let Some(pointer) = ctx.input(|input| input.pointer.latest_pos()) else { return true };
            let (view, rect) = match target {
                EditorTarget::Material => (app.ui.graph_camera.view(app.graph.view), app.graph.view),
                EditorTarget::Function => app
                    .function_editor
                    .as_ref()
                    .map_or((CanvasView::new(egui::Rect::NOTHING, craft_behavior::kurbo::Point::ZERO, 1.0), egui::Rect::NOTHING), |editor| {
                        (editor.view, editor.view.rect)
                    }),
                EditorTarget::Pattern => app
                    .pattern_editor
                    .as_ref()
                    .map_or((CanvasView::new(egui::Rect::NOTHING, craft_behavior::kurbo::Point::ZERO, 1.0), egui::Rect::NOTHING), |editor| {
                        (editor.view, editor.view.rect)
                    }),
            };
            if rect.contains(pointer) {
                let point = view.to_doc(pointer);
                let at = pos2(point.x as f32, point.y as f32);
                if target == EditorTarget::Pattern {
                    if let Some(editor) = &mut app.pattern_editor {
                        editor.open_menu(at);
                    }
                } else {
                    crate::graph::search::open(app, target, at, None);
                }
            }
            return true;
        }
        "ui.graph.frame" => {
            let request = ViewRequest::Command(ViewCommands::FitLayers);
            match target {
                EditorTarget::Material => app.graph.request_view(request),
                EditorTarget::Function => {
                    if let Some(editor) = &mut app.function_editor {
                        editor.queued.push(request);
                    }
                }
                EditorTarget::Pattern => {
                    if let Some(editor) = &mut app.pattern_editor {
                        editor.queued.push(request);
                    }
                }
            }
            return true;
        }
        "ui.graph.copyInfo" => {
            let Some(id) = params.get("id") else { return false };
            let Some(result) = app.run("graph.node.info", json!({"id": id})) else { return true };
            let Some(text) = result.get("text").and_then(Value::as_str) else { return true };
            let _ = craft_ui::runtime::commands::ClipboardHandle::of(ctx).offer(&[craft_ui::runtime::commands::ClipItem::Text(text.into())]);
            return true;
        }
        "ui.graph.view2d" => {
            app.run("view.2d", params.clone());
            app.ui.dock.focus(Panel::View2d);
            return true;
        }
        "ui.function.copyInfo" | "ui.function.openReference" | "ui.function.setOutput" | "ui.function.add" | "ui.function.annotate"
            if target == EditorTarget::Function =>
        {
            return crate::function::actions::invoke_ui(app, ctx, command, params);
        }
        "ui.pattern.add" if target == EditorTarget::Pattern => return crate::pattern::actions::invoke_ui(app, command, params),
        "graph.node.edit" => {
            if !crate::menus::enabled(app, command) {
                return true;
            }
            let Some(result) = app.run(command, Value::Null) else { return true };
            let Some(node) = result.get("node").and_then(Value::as_u64) else { return true };
            let node = materialcraft_engine::core::NodeId(node);
            match result.get("kind").and_then(Value::as_str) {
                Some("pattern") => {
                    app.function_editor = None;
                    app.pattern_editor = Some(crate::pattern::PatternEditor::new(app.session.graph, node));
                }
                Some("function") => super::subgraph_navigation::open_function(app, node, crate::function::FunctionTarget::Parameter("function".into())),
                Some("graph") => {
                    if let Some(index) = result.get("graph").and_then(Value::as_u64).and_then(|index| usize::try_from(index).ok()) {
                        if result.get("document").and_then(Value::as_u64) == Some(app.session.document_id) {
                            super::subgraph_navigation::open_instance(app, index);
                        } else {
                            super::subgraph_navigation::open_external_instance(app, &result, index);
                        }
                    }
                }
                _ => {}
            }
            app.ui.dock.focus(Panel::Graph);
            return true;
        }
        "ui.graph.snapToGrid" => app.ui.prefs.snap_to_grid = !app.ui.prefs.snap_to_grid,
        "ui.graph.viewOutputs" | "ui.graph.resetViewOutputs" => {
            app.run("view.3d.outputs", json!({"reset": command == "ui.graph.resetViewOutputs"}));
        }
        "ui.graph.computeThumbnails" => {
            app.graph.thumbnails_cleared = false;
            app.ui.prefs.thumbnails = true;
            app.session.dirty = true;
        }
        "ui.graph.clearThumbnails" => {
            app.thumbs.clear_nodes(ctx);
            app.graph.thumbnails_cleared = true;
        }
        "ui.graph.previousPin" | "ui.graph.nextPin" => {
            if let Some(view) = pin_target(app, command == "ui.graph.nextPin") {
                crate::annotation::set_camera(app, view);
            }
        }
        _ => return false,
    }
    true
}
