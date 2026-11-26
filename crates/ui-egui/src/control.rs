//! The JSON control channel: agents drive the running app with the same commands as the menus,
//! inspect session and UI state, invoke menu items, synthesize mouse and keyboard input and take
//! screenshots.
//!
//! The transport and the channel are the framework's (`craft_automation`): [`poll`] runs the
//! requests in the logic phase and [`ui_pass`] issues captures and answers input waiters at the
//! end of each pass. The framework's methods come first (`ui.click`, `ui.focus`, `ui.move`,
//! `ui.wheel`, `ui.key`, `ui.type`, `ui.release`, `ui.drop`, `ui.dialog.*`, `ui.screenshot`), then
//! the app's table below, then the control hooks (`crate::control_hooks`).
//!
//! The app's methods (params as JSON objects):
//! - `command.run {id, params?}`: run an engine command (programmatic: never opens a dialog).
//! - `command.list {filter?}`: every command with params docs and enabled state.
//! - `session.inspect`: package, graph, nodes, wires, parameters, cook results, views, history.
//! - `ui.inspect {query?, role?, focused?, limit?}`: the on-screen widgets (the framework's
//!   access tree, filtered by those keys) and the UI state (panels, windows, graph camera, node
//!   rectangles and ports on screen, the shown toasts, the held unsaved-changes prompt).
//! - `ui.menu.list` / `ui.menu.invoke {id}`: the menu catalog; behave like a menu click, so behind
//!   a dialog or prompt an item lists as disabled and invoking it returns an error, except the
//!   commands [`crate::shell::modality::Modality::allows`] lets through.
//! - `ui.set {...}`: merge fields into the UI state (e.g. `{"prefs": {"thumbnails": false}}`).
//! - `ui.input {events}`: event descriptions run as the framework's input methods (see
//!   [`parse_events`]); replies once the input is processed.
//! - `view.screenshot {view?: "3d"|"2d"}`: PNG of a view, `{base64, mime, width, height}`.
//! - `ui.resize {width, height}`: the window's inner size in points.

use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use craft_automation::control::Reply;
use craft_automation::synthetic::{self, Click, KeyPress, Move, Release, Wheel};
use craft_automation::{access, dialogs, screenshot};
use craft_theme::variant::Tone;
use craft_ui::runtime::commands::{ClipItem, ClipboardHandle};
use craft_ui::runtime::env::WindowKey;
use craft_ui::runtime::motion;
use craft_ui::runtime::windows::WindowHost;
use craft_ui::shell::WindowDialogs;
use egui::{Pos2, ViewportId, pos2, vec2};
use serde_json::{Map, Value, json};

use crate::MaterialCraftApp;

/// Why `ui.screenshot {path}` is refused: the desktop app takes no automation root, so it has no folder it may
/// write a capture to; captures come back inline.
const NO_WRITE_ROOT: &str = "ui.screenshot writes no file: this app has no automation write root";

/// The `ui.inspect` keys the framework's widget listing reads ([`access::inspect`] refuses any other).
const WIDGET_FILTERS: [&str; 4] = ["query", "role", "focused", "limit"];

/// The next `ui.screenshot` token: each pending capture needs its own.
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

fn ok(v: Value) -> Value {
    json!({ "ok": true, "result": v })
}

fn err(e: impl std::fmt::Display) -> Value {
    json!({ "ok": false, "error": e.to_string() })
}

/// The logic phase of the control channel: collects finished screenshots and runs every request the channel
/// holds. The logic phase runs once per frame, before its passes, so a frame that runs a second pass takes no
/// request twice.
pub fn poll(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let Some(mut channel) = app.control.take() else { return };
    let host = WindowHost::of(ctx);
    // The app has one window and no document identity a capture could be attributed to.
    channel.captures.note_windows(&host, None);
    let finished = channel.captures.drain(motion::now(), None);
    channel.finish_captures(finished, &mut refuse_write);
    let settle = screenshot::settle_ms(ctx.global_style().animation_time);
    let mut ran = false;
    while let Some(request) = channel.next_request() {
        let reply = dispatch(app, ctx, &host, &request.method, &request.params);
        channel.settle(request, reply, None, settle);
        ran = true;
    }
    app.control = Some(channel);
    if ran {
        ctx.request_repaint();
    }
}

/// The UI phase of the control channel, at the end of each pass's UI: counts the pass toward screenshot settling,
/// issues the captures that are due, and answers the input waiters whose input the pass consumed. A pass egui runs
/// again counts nothing toward settling, as its picture never shows; the input waiters are polled on every pass,
/// as the framework reads which pass consumed the input from the host.
pub(crate) fn ui_pass(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let Some(channel) = app.control.as_mut() else { return };
    let host = WindowHost::of(ctx);
    if !craft_ui::runtime::frame::will_rerun(ctx) {
        channel.captures.note_pass();
        let expired = channel.captures.issue(&host, motion::now(), None);
        channel.finish_captures(expired, &mut refuse_write);
    }
    channel.poll_input(&host);
    if channel.captures.any_unissued() {
        ctx.request_repaint();
    }
}

/// The app's file authority for captures: it has no automation root, so every write is refused.
fn refuse_write(_path: &str, _png: &[u8]) -> Result<(), String> {
    Err(NO_WRITE_ROOT.to_owned())
}

/// The reply to one request: the framework's screenshot, input and dialog methods, `ui.input`, the app's table,
/// then the control hooks.
fn dispatch(app: &mut MaterialCraftApp, ctx: &egui::Context, host: &WindowHost, method: &str, params: &Value) -> Reply {
    // No canvas maps document points yet: a request in document coordinates is refused (`synthetic::NO_DOCUMENT`).
    let documents = |_: usize, _: Pos2| -> Option<Pos2> { None };
    match method {
        "ui.screenshot" => return screenshot_request(params),
        "ui.input" => {
            let Some(list) = params.get("events").and_then(Value::as_array) else {
                return Reply::from_result(Err("`events` (an array) is required".into()));
            };
            let queued = parse_events(list).and_then(|calls| {
                // Every call was checked as the framework reads it, so none is refused part way.
                calls.iter().try_for_each(|(method, params)| {
                    synthetic::input_method(method, params, host, ViewportId::ROOT, &documents)
                        .unwrap_or_else(|| Err(format!("`{method}` is not an input method")))
                })
            });
            return after_input(queued);
        }
        _ => {}
    }
    if let Some(queued) = synthetic::input_method(method, params, host, ViewportId::ROOT, &documents) {
        return after_input(queued);
    }
    if let Some(result) = WindowDialogs::of(ctx, &WindowKey::main()).with(|dialog_host| dialogs::dialog_method(dialog_host, ctx, method, params)) {
        return Reply::from_result(result);
    }
    if let Some(reply) = app_method(app, ctx, method, params) {
        return Reply::Done(reply);
    }
    crate::control_hooks::handle(app, ctx, method, params).unwrap_or_else(|| Reply::Done(err(format!("unknown method `{method}`"))))
}

/// The reply to queued input: once the app has processed it, or the error that queued none.
fn after_input(queued: Result<(), String>) -> Reply {
    match queued {
        Ok(()) => Reply::AfterInput,
        Err(e) => Reply::from_result(Err(e)),
    }
}

/// `ui.screenshot {region?}`: the main window, answered inline once the capture settles.
fn screenshot_request(params: &Value) -> Reply {
    match screenshot::ScreenshotParams::from_params(params) {
        Ok(capture) if capture.path.is_some() => Reply::from_result(Err(NO_WRITE_ROOT.to_owned())),
        Ok(capture) => Reply::screenshot(NEXT_TOKEN.fetch_add(1, Ordering::Relaxed), capture, ViewportId::ROOT),
        Err(e) => Reply::from_result(Err(e)),
    }
}

fn point(v: &Value, key: &str) -> Option<Pos2> {
    let a = v.get(key)?.as_array()?;
    let (x, y) = (a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32);
    (x.is_finite() && y.is_finite()).then(|| pos2(x, y))
}

/// The framework's name for the event's `button`.
fn button(v: &Value) -> &'static str {
    match v.get("button").and_then(Value::as_str) {
        Some("right" | "secondary") => "right",
        Some("middle") => "middle",
        _ => "left",
    }
}

/// The framework's modifier flags for the event's `modifiers` names.
fn modifiers(v: &Value) -> Value {
    let mut m = Map::new();
    for k in v.get("modifiers").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        let flag = match k.to_ascii_lowercase().as_str() {
            // A shortcut's Ctrl is Command on macOS, where the real Control key is a different chord; the
            // framework's `command` is Command on macOS and Ctrl elsewhere
            "ctrl" | "control" | "cmd" | "command" | "meta" => "command",
            "shift" => "shift",
            "alt" | "option" => "alt",
            _ => continue,
        };
        m.insert(flag.into(), Value::Bool(true));
    }
    Value::Object(m)
}

/// Checks `params` as the framework's input method `method` reads them.
fn check_input(method: &str, params: &Value) -> Result<(), String> {
    match method {
        "ui.click" => Click::from_params(params).map(drop),
        "ui.move" => Move::from_params(params).map(drop),
        "ui.wheel" => Wheel::from_params(params).map(drop),
        "ui.key" => KeyPress::from_params(params).map(drop),
        "ui.release" => Release::from_params(params).map(drop),
        // `ui.type` takes any string, which the event's `text` already is.
        _ => Ok(()),
    }
}

/// The framework input calls (method and params) a list of event descriptions equals, in order: `move` is a
/// `ui.move`, `down` a held `ui.click` and `up` a `ui.release`, `click` a `ui.click` and `doubleClick` two, `drag`
/// a held press, `steps` moves and a release, `scroll` a `ui.wheel`, `key` a `ui.key` and `text` a `ui.type`. Each
/// call is checked as the framework reads it, so a bad event refuses the whole list before any input is queued.
pub fn parse_events(list: &[Value]) -> Result<Vec<(String, Value)>, String> {
    const MAX_INPUT_CALLS: usize = 10_000;
    let mut calls = Vec::new();
    for (i, e) in list.iter().enumerate() {
        let ty = e.get("type").and_then(Value::as_str).ok_or(format!("event {i}: `type` is required"))?;
        let mods = modifiers(e);
        let need = |k: &str| point(e, k).ok_or(format!("event {i} ({ty}): `{k}` [x, y] is required"));
        let event: Vec<(&str, Value)> = match ty {
            "move" => {
                let pos = need("pos")?;
                vec![("ui.move", json!({"x": pos.x, "y": pos.y, "modifiers": mods}))]
            }
            "down" => {
                let pos = need("pos")?;
                vec![("ui.click", json!({"x": pos.x, "y": pos.y, "button": button(e), "hold": true, "modifiers": mods}))]
            }
            "up" => {
                let pos = need("pos")?;
                vec![("ui.release", json!({"x": pos.x, "y": pos.y, "button": button(e)}))]
            }
            "click" | "doubleClick" => {
                let pos = need("pos")?;
                let times = if ty == "click" { 1 } else { 2 };
                vec![("ui.click", json!({"x": pos.x, "y": pos.y, "button": button(e), "modifiers": mods})); times]
            }
            "drag" => {
                let (from, to) = (need("from")?, need("to")?);
                let steps = e.get("steps").and_then(Value::as_u64).unwrap_or(8).clamp(1, 120) as usize;
                let mut drag = vec![("ui.click", json!({"x": from.x, "y": from.y, "button": button(e), "hold": true, "modifiers": mods}))];
                for s in 1..=steps {
                    let at = from + (to - from) * (s as f32 / steps as f32);
                    drag.push(("ui.move", json!({"x": at.x, "y": at.y, "modifiers": mods})));
                }
                drag.push(("ui.release", json!({"x": to.x, "y": to.y, "button": button(e)})));
                drag
            }
            "scroll" => {
                let pos = need("pos")?;
                let d = point(e, "delta").unwrap_or(pos2(0.0, -120.0));
                vec![("ui.wheel", json!({"x": pos.x, "y": pos.y, "dx": d.x, "dy": d.y, "modifiers": mods}))]
            }
            "key" => {
                let name = e.get("key").and_then(Value::as_str).ok_or(format!("event {i} (key): `key` is required"))?;
                vec![("ui.key", json!({"key": name, "modifiers": mods}))]
            }
            "text" => {
                let t = e.get("text").and_then(Value::as_str).ok_or(format!("event {i} (text): `text` is required"))?;
                vec![("ui.type", json!({"text": t}))]
            }
            other => return Err(format!("event {i}: unknown type `{other}`")),
        };
        for (method, params) in event {
            check_input(method, &params).map_err(|e| format!("event {i} ({ty}): {e}"))?;
            calls.push((method.to_owned(), params));
        }

        if calls.len() > MAX_INPUT_CALLS {
            return Err("too many input events".into());
        }
    }

    Ok(calls)
}

/// Recursive JSON merge: objects merge key by key, anything else in `patch` replaces `dst`.
fn merge_json(dst: &mut Value, patch: &Value) {
    match (dst, patch) {
        (Value::Object(d), Value::Object(p)) => {
            for (k, v) in p {
                match d.get_mut(k) {
                    Some(slot) => merge_json(slot, v),
                    None => {
                        d.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (slot, v) => *slot = v.clone(),
    }
}

/// Runs one method of the app's table and returns its `{ok, result}` or `{ok, error}` reply; a method the table
/// does not have is an error. A panic inside the method is an error reply and resets the cook state.
pub fn handle(app: &mut MaterialCraftApp, ctx: &egui::Context, method: &str, p: &Value) -> Value {
    app_method(app, ctx, method, p).unwrap_or_else(|| err(format!("unknown method `{method}`")))
}

/// [`handle`]'s table behind the panic guard; `None` for a method it does not have.
fn app_method(app: &mut MaterialCraftApp, ctx: &egui::Context, method: &str, p: &Value) -> Option<Value> {
    match crate::cook::guard("control request", || Ok(handle_inner(app, ctx, method, p))) {
        Ok(v) => v,
        Err(e) => {
            app.session.dirty = false;
            Some(err(e))
        }
    }
}

fn handle_inner(app: &mut MaterialCraftApp, ctx: &egui::Context, method: &str, p: &Value) -> Option<Value> {
    Some(match method {
        "ping" => ok(json!("pong")),
        "ui.resize" => {
            let size = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).map(|v| v.clamp(640.0, 7680.0) as f32);
            match (size("width"), size("height")) {
                (Some(w), Some(h)) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(w, h)));
                    ok(Value::Null)
                }
                _ => err("`width` and `height` (points, 640..7680) are required"),
            }
        }
        "command.run" => {
            let Some(id) = p.get("id").and_then(Value::as_str) else { return Some(err("`id` is required")) };
            let params = p.get("params").cloned().unwrap_or(Value::Null);
            if matches!(id, "view.3d.saveRender" | "view.3d.exportGeometry" | "view.2d.copyImage") {
                return Some(displayed_command(app, ctx, id, &params));
            }

            match crate::cook::guard("command request", || {
                Ok(materialcraft_engine::commands::execute_request(&mut app.session, id, &params).map_err(|error| error.to_string()))
            }) {
                Ok(Ok(v)) => {
                    app.sync_preferences_after_command(id);
                    app.close_subgraph_after(id);
                    ok(v)
                }
                Ok(Err(e)) => err(e),
                Err(e) => {
                    app.session.dirty = false;
                    err(e)
                }
            }
        }
        "command.list" => ok(materialcraft_engine::commands::list(&app.session, p.get("filter").and_then(Value::as_str))),
        "session.inspect" => {
            match crate::cook::guard("session.inspect", || {
                app.session.cook_if_dirty();
                if app.session.graph().is_err() {
                    return Err(format!("invalid graph {}", app.session.graph));
                }

                Ok(materialcraft_engine::inspect::session(&app.session, None))
            }) {
                Ok(v) => ok(v),
                Err(e) => {
                    app.session.dirty = false;
                    err(e)
                }
            }
        }
        "ui.inspect" => inspect(app, ctx, p),
        "ui.menu.list" => ok(crate::menus::list(app, ctx)),
        "ui.menu.invoke" => {
            let Some(id) = p.get("id").and_then(Value::as_str) else { return Some(err("`id` is required")) };
            if !crate::menus::implemented(id) {
                return Some(err(format!("menu item `{id}` is not implemented")));
            }

            let modality = crate::shell::modality::Modality::capture(app, ctx);
            if let crate::shell::modality::Modality::Blocked(blocker) = modality
                && !modality.allows(id)
            {
                return Some(err(format!("menu item `{id}` cannot run while {} is open", blocker.name())));
            }

            crate::menus::invoke(app, ctx, id);
            ok(Value::Null)
        }
        "ui.set" => {
            // The hooks' keys leave the patch before the merge and apply once the merged state is taken.
            let mut patch = p.clone();
            let keys = match crate::control_hooks::SetKeys::take(&mut patch) {
                Ok(keys) => keys,
                Err(e) => return Some(err(e)),
            };
            let mut cur = serde_json::to_value(&app.ui).unwrap_or(Value::Null);
            if patch.is_object() {
                merge_json(&mut cur, &patch);
            }

            match serde_json::from_value::<crate::state::UiState>(cur) {
                Ok(mut ui) => {
                    ui.graph_camera = ui.graph_camera.sanitized();
                    ui.image_camera = ui.image_camera.sanitized();
                    app.ui = ui;
                    keys.apply(app);
                    ok(Value::Null)
                }
                Err(e) => err(format!("invalid UI state: {e}")),
            }
        }
        "view.screenshot" => {
            let shown = if p.get("view").and_then(Value::as_str) == Some("2d") { &app.view2d.shown } else { &app.view3d.shown };
            let Some((width, height, pixels)) = shown.as_ref() else { return Some(err("that view has not drawn anything yet")) };
            match screenshot::encode_png(*width as usize, *height as usize, pixels.as_slice()) {
                Ok(png) => ok(json!({
                    "base64": base64::engine::general_purpose::STANDARD.encode(png),
                    "mime": "image/png",
                    "width": width,
                    "height": height,
                })),
                Err(e) => err(format!("could not encode the view: {e}")),
            }
        }
        _ => return None,
    })
}

/// `ui.inspect`: the framework's widget listing (when the window has an access tree), the app's state, then the
/// control hooks' fields.
fn inspect(app: &mut MaterialCraftApp, ctx: &egui::Context, p: &Value) -> Value {
    let mut reply = Map::new();
    let host = WindowHost::of(ctx);
    // A context that never presented a frame (a bare test context, the first frame) has no tree to list.
    if host.access_tree_view(ViewportId::ROOT).is_some() {
        let filters: Map<String, Value> =
            p.as_object().into_iter().flatten().filter(|(key, _)| WIDGET_FILTERS.contains(&key.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
        match access::inspect(&host, ViewportId::ROOT, &Value::Object(filters)) {
            Ok(Value::Object(widgets)) => reply.extend(widgets),
            Ok(_) => {}
            Err(e) => return err(e),
        }
    }

    let shown: &MaterialCraftApp = app;
    let rect = |r: egui::Rect| json!([r.min.x, r.min.y, r.width(), r.height()]);
    let target = crate::graph::editor_target::EditorTarget::resolve(shown);
    let (view, nodes, ports) = (shown.graph.view, &shown.graph.rects, &shown.graph.ports);
    let toasts = craft_ui::runtime::toast::cards(ctx, &WindowKey::main());
    let fields = json!({
        "state": shown.ui,
        "unsaved": crate::unsaved::held(shown),
        "graphView": rect(view),
        "selection": shown.session.selection,
        "inspectedNode": shown.graph.inspected_node,
        "inspectedGraph": shown.inspected_graph.map(|target| json!({"document": target.document, "graph": target.graph})),
        "inspectedFolder": shown.inspected_folder.as_ref().map(|(document, path)| json!({"document": document, "folder": path})),
        "subgraph": match target {
            crate::graph::editor_target::EditorTarget::Function => shown.function_editor.as_ref()
                .map(|editor| json!({"kind": "function", "graph": shown.session.graph, "node": editor.node, "target": editor.command_target(), "selection": editor.selection, "selectedWires": editor.selected_wire_refs(shown), "inspectedNode": editor.inspected_node})),
            crate::graph::editor_target::EditorTarget::Pattern => shown.pattern_editor.as_ref()
                .map(|editor| json!({"kind": "pattern", "graph": editor.graph, "node": editor.node.0, "selection": editor.selection, "selectedWires": editor.selected_wire_refs(shown), "inspectedNode": editor.inspected_node})),
            crate::graph::editor_target::EditorTarget::Material => None,
        },
        "annotationSelection": shown.session.annotation_selection,
        "annotationTarget": shown.session.annotation_target,
        "pins": shown.graph.pins.iter().map(|(id, bounds)| json!({"id": id, "rect": rect(*bounds)})).collect::<Vec<_>>(),
        "frames": shown.graph.frames.iter().map(|(id, bounds)| json!({"id": id, "rect": rect(*bounds)})).collect::<Vec<_>>(),
        "comments": shown.graph.comments.iter().map(|(id, bounds)| json!({"id": id, "rect": rect(*bounds)})).collect::<Vec<_>>(),
        "nodes": nodes.iter().map(|(id, r)| json!({"id": id.0, "rect": rect(*r)})).collect::<Vec<_>>(),
        "ports": ports.iter().map(|port| json!({
            "node": port.node.0,
            "port": port.port,
            "label": port.label,
            "output": port.output,
            "pos": [port.pos.x, port.pos.y],
        })).collect::<Vec<_>>(),
        "fps": shown.fps,
        "uiMs": shown.ui_ms,
        "cooking": shown.cooker.busy(),
        "render3dMs": shown.view3d.render_ms,
        "toasts": toasts.iter().map(|card| json!({
            "text": card.toast.message.as_str(),
            "error": card.toast.tone == Tone::Bad,
        })).collect::<Vec<_>>(),
        "dialogs": WindowDialogs::of(ctx, &WindowKey::main()).with(|dialog_host| dialogs::inspect(dialog_host)),
    });
    if let Value::Object(fields) = fields {
        reply.extend(fields);
    }
    crate::control_hooks::inspect(app, ctx, &mut reply);
    ok(Value::Object(reply))
}

/// Give a programmatic command the same currently displayed bytes the toolbar or dialog uses
fn displayed_command(app: &mut MaterialCraftApp, ctx: &egui::Context, id: &str, params: &Value) -> Value {
    use materialcraft_engine::DisplaySnapshot;

    let snapshot = match id {
        "view.3d.saveRender" => app.view3d.shown.as_ref().map(|(width, height, pixels)| DisplaySnapshot::render(*width, *height, pixels.clone())),
        "view.3d.exportGeometry" => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                crate::view3d::displayed_mesh(app).map(DisplaySnapshot::geometry)
            }
            #[cfg(target_arch = "wasm32")]
            {
                return err("exporting geometry by path needs the desktop app");
            }
        }
        "view.2d.copyImage" => {
            app.view2d.shown.as_ref().map(|(width, height, pixels)| DisplaySnapshot::image(*width, *height, pixels.clone(), app.view2d.shown_premultiplied))
        }
        _ => return err(format!("unknown display command `{id}`")),
    };
    let Some(snapshot) = snapshot else { return err("that view has not drawn anything yet") };

    app.session.begin_display_handoff(snapshot);
    let result = crate::cook::guard("command request", || {
        Ok(materialcraft_engine::commands::execute_request(&mut app.session, id, params).map_err(|error| error.to_string()))
    });
    let output = app.session.finish_display_handoff();
    match result {
        Ok(Ok(value)) if id == "view.2d.copyImage" => {
            let Some((width, height, pixels, premultiplied)) = output.and_then(DisplaySnapshot::into_image) else {
                return err("the copy command returned no displayed image");
            };
            let size = [width as usize, height as usize];
            let image =
                if premultiplied { egui::ColorImage::from_rgba_premultiplied(size, &pixels) } else { egui::ColorImage::from_rgba_unmultiplied(size, &pixels) };
            match ClipboardHandle::of(ctx).offer(&[ClipItem::Image(std::sync::Arc::new(image))]) {
                Ok(_) => ok(value),
                Err(error) => err(format!("could not copy the image: {error}")),
            }
        }
        Ok(Ok(value)) => ok(value),
        Ok(Err(error)) | Err(error) => err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_host::{PickRequest, ScriptedFiles};

    #[test]
    fn input_events_become_frames() {
        let calls = |event: Value| parse_events(&[event]).unwrap();
        let methods = |calls: &[(String, Value)]| calls.iter().map(|(method, _)| method.clone()).collect::<Vec<_>>();

        // A click carries its own move to the point; a double click therefore gives more calls than a click.
        let click = calls(json!({"type": "click", "pos": [10, 20]}));
        let double = calls(json!({"type": "doubleClick", "pos": [10, 20]}));
        assert!(double.len() > click.len(), "{double:?}");
        assert!(methods(&double).iter().all(|method| method == "ui.click"), "{double:?}");

        // A drag holds its press over the moves and releases at the end.
        let drag = calls(json!({"type": "drag", "from": [0, 0], "to": [100, 0], "steps": 4}));
        let (first, rest) = drag.split_first().unwrap();
        let (last, moves) = rest.split_last().unwrap();
        assert_eq!((first.0.as_str(), &first.1["hold"]), ("ui.click", &json!(true)), "{drag:?}");
        assert_eq!(last.0, "ui.release", "{drag:?}");
        assert!(!moves.is_empty() && methods(moves).iter().all(|method| method == "ui.move"), "{drag:?}");
        let xs: Vec<f64> = moves.iter().filter_map(|(_, params)| params["x"].as_f64()).collect();
        assert!(xs.windows(2).all(|pair| pair[0] < pair[1]), "the moves head for `to`: {xs:?}");

        assert_eq!(methods(&calls(json!({"type": "key", "key": "Space"}))), ["ui.key"]);
        assert_eq!(methods(&calls(json!({"type": "text", "text": "blur"}))), ["ui.type"]);
        assert_eq!(methods(&calls(json!({"type": "scroll", "pos": [5, 5], "delta": [0, -60]}))), ["ui.wheel"]);

        assert!(parse_events(&[json!({"type": "key", "key": "NoSuchKey"})]).is_err());
        assert!(parse_events(&[json!({"type": "click"})]).is_err());
        assert!(parse_events(&[json!({"pos": [1, 2]})]).is_err());
        assert!(parse_events(&vec![json!({"type": "text", "text": "a"}); 10_001]).is_err());
    }

    fn app() -> MaterialCraftApp {
        MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::state::UiState::default())
    }

    /// Opens the unsaved-changes prompt as a user's Quit does: an edit leaves unsaved work, then the guard holds the
    /// quit for the prompt.
    fn hold_quit(app: &mut MaterialCraftApp, ctx: &egui::Context) {
        let edit = handle(app, ctx, "command.run", &json!({"id": "graph.rename", "params": {"name": "Held"}}));
        assert_eq!(edit["ok"], json!(true), "{edit}");
        assert!(app.session.modified);
        crate::unsaved::guard(app, crate::unsaved::Guarded::Quit);
        assert_eq!(crate::unsaved::held(app), Some(&crate::unsaved::Guarded::Quit));
    }

    /// `reply` is a refusal that names `blocker`, the one input is blocked behind.
    fn assert_refused_behind(reply: &Value, blocker: crate::shell::modality::Blocker, app: &MaterialCraftApp, ctx: &egui::Context) {
        assert_eq!(crate::shell::modality::Modality::capture(app, ctx), crate::shell::modality::Modality::Blocked(blocker));
        assert_eq!(reply["ok"], json!(false), "{reply}");
        assert!(reply["error"].as_str().is_some_and(|error| error.contains(blocker.name())), "{reply}");
    }

    #[test]
    fn a_menu_item_returns_an_error_behind_a_dialog() {
        let mut app = app();
        let ctx = egui::Context::default();
        hold_quit(&mut app, &ctx);

        let reply = handle(&mut app, &ctx, "ui.menu.invoke", &json!({"id": "ui.export"}));
        assert_refused_behind(&reply, crate::shell::modality::Blocker::UnsavedPrompt, &app, &ctx);
        assert!(!app.ui.windows.export);

        let list = handle(&mut app, &ctx, "ui.menu.list", &json!({}));
        assert!(list["result"].as_array().unwrap().iter().all(|item| item["enabled"] == json!(false)), "{list}");

        app.unsaved = None;
        let reply = handle(&mut app, &ctx, "ui.menu.invoke", &json!({"id": "ui.export"}));
        assert_eq!(reply["ok"], json!(true), "{reply}");
        assert!(app.ui.windows.export);
    }

    #[test]
    fn a_menu_item_reports_the_blocker_behind_a_file_dialog_or_an_update_window() {
        let ctx = egui::Context::default();
        let mut behind_dialog = app();
        // A held service keeps the pick open, as a dialog on screen is.
        let files = std::sync::Arc::new(ScriptedFiles::held());
        behind_dialog.host = std::sync::Arc::new(behind_dialog.host.with_files(files.clone()));
        crate::native_dialogs::ask(&mut behind_dialog, PickRequest::open("document"), crate::native_dialogs::Then::OpenPackage);
        assert_eq!(files.requests().len(), 1);

        let open = handle(&mut behind_dialog, &ctx, "ui.menu.invoke", &json!({"id": "file.open"}));
        assert_eq!(open["ok"], json!(true), "{open}");
        assert_eq!(files.requests().len(), 1, "Open behind an open file dialog asks for no second pick");
        let reply = handle(&mut behind_dialog, &ctx, "ui.menu.invoke", &json!({"id": "file.save"}));
        assert_refused_behind(&reply, crate::shell::modality::Blocker::FileDialog, &behind_dialog, &ctx);
        let list = handle(&mut behind_dialog, &ctx, "ui.menu.list", &json!({}));
        let open = list["result"].as_array().unwrap().iter().find(|item| item["id"] == json!("file.open")).unwrap();
        assert_eq!(open["enabled"], json!(true), "{list}");

        let mut behind_update = app();
        crate::updates_ui::open_prompt_for_test(&mut behind_update);
        let reply = handle(&mut behind_update, &ctx, "ui.menu.invoke", &json!({"id": "ui.export"}));
        assert_refused_behind(&reply, crate::shell::modality::Blocker::Update, &behind_update, &ctx);
        assert!(!behind_update.ui.windows.export);
    }

    #[test]
    fn command_run_stays_available_behind_a_dialog() {
        let mut app = app();
        let ctx = egui::Context::default();
        hold_quit(&mut app, &ctx);
        let reply = handle(&mut app, &ctx, "command.run", &json!({"id": "edit.selectAll"}));
        assert_eq!(reply["ok"], json!(true), "{reply}");
        assert_eq!(crate::unsaved::held(&app), Some(&crate::unsaved::Guarded::Quit));
    }

    #[test]
    fn ui_set_merges_nested_objects_instead_of_resetting_siblings() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.ui.prefs.ui_scale = 1.5;
        app.ui.prefs.autosave_minutes = 9;
        let r = handle(&mut app, &ctx, "ui.set", &json!({"prefs": {"thumbnails": false}}));
        assert_eq!(r["ok"], json!(true), "{r}");
        assert!(!app.ui.prefs.thumbnails);
        assert_eq!(app.ui.prefs.ui_scale, 1.5);
        assert_eq!(app.ui.prefs.autosave_minutes, 9);
    }

    #[test]
    fn cache_commands_and_saved_preferences_stay_in_sync() {
        let mut app = app();
        let ctx = egui::Context::default();
        let reply = handle(&mut app, &ctx, "command.run", &json!({"id": "library.cache.configure", "params": {"diskMib": 256, "maxPreviews": 8000}}));

        assert_eq!(reply["ok"], json!(true), "{reply}");

        app.apply_prefs();

        assert_eq!(app.ui.prefs.library_cache, app.session.library_cache_prefs);
        assert_eq!(app.ui.prefs.library_cache.disk_mib, 256);

        let saved: crate::UiState = serde_json::from_value(serde_json::to_value(&app.ui).unwrap()).unwrap();
        let mut restored = MaterialCraftApp::headless(materialcraft_engine::sample_session(), saved);

        assert_eq!(restored.session.library_cache_prefs, app.session.library_cache_prefs);

        let reply = handle(&mut restored, &ctx, "ui.set", &json!({"prefs": {"libraryCache": {"diskMib": 32}}}));

        assert_eq!(reply["ok"], json!(true), "{reply}");

        restored.apply_prefs();

        assert_eq!(restored.session.library_cache_prefs.disk_mib, 32);
        assert_eq!(restored.session.library_cache_prefs.max_previews, 8000);

        let reply = handle(&mut restored, &ctx, "command.run", &json!({"id": "library.cache.clear"}));

        assert_eq!(reply["ok"], json!(true), "{reply}");
        assert_eq!(restored.session.library_cache_clear_epoch, 1);
    }

    #[test]
    fn ui_set_clamps_camera_zoom() {
        let mut app = app();
        let ctx = egui::Context::default();
        let r = handle(&mut app, &ctx, "ui.set", &json!({"graphCamera": {"zoom": 0.0}, "imageCamera": {"zoom": 0.0, "fit": false}}));
        assert_eq!(r["ok"], json!(true), "{r}");
        assert!(app.ui.graph_camera.zoom > 0.0);
        assert!(app.ui.image_camera.zoom > 0.0);
        let c = &app.ui.graph_camera;
        assert!((c.pan[0] + 100.0 / c.zoom).is_finite());
    }

    #[test]
    fn session_inspect_panic_returns_error_and_resets_cook_state() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.session.preview_size = Some(64);
        app.session.cook();
        let valid_graph = app.session.graph;
        app.session.graph = app.session.graph.wrapping_add(1_000);
        app.session.dirty = true;
        let r = handle(&mut app, &ctx, "session.inspect", &json!({}));
        assert_eq!(r["ok"], json!(false), "{r}");
        assert!(!app.session.dirty, "cook state not reset after panic: dirty={}", app.session.dirty);
        app.session.graph = valid_graph;
        let r = handle(&mut app, &ctx, "session.inspect", &json!({}));
        assert_eq!(r["ok"], json!(true), "{r}");
    }
}
