//! The control channel's capture hooks: methods that drive the app into a state a look comparison captures, or
//! report what is on screen. Each reaches its state through the app's own functions or the library's synthetic input
//! and changes no behaviour. `ui.click` and `ui.focus` are the library's methods, not hooks.
//!
//! Methods (params as JSON objects):
//! - `ui.toast {text, error?}`: post a toast as the app does.
//! - `ui.update.status {status, version?, message?}`: show an updater outcome without the network (desktop only).
//! - `ui.explorer.select {target}`: select an Explorer row (an `ExplorerTarget`; no `document` is the active one).
//! - `ui.explorer.menu {target, open?, pick?}`: open the row's context menu by a secondary click, then move to the
//!   submenu labelled `open` once it has drawn; or run the row-menu command `pick`.
//! - `ui.explorer.rename {target, name?}`: select the row, press F2 and type `name`; a following `ui.inspect` reads
//!   the editor's text.
//! - `ui.properties.parameterMenu {id, param, base?, menu?}`: open a parameter's actions menu, or with
//!   `menu: "inheritance"` a base parameter's inheritance menu.
//! - `ui.properties.expose {id, param, identifier?, label?, group?, description?, open?}`: open the Expose
//!   Parameter dialog with fields filled and, for `open: "group"`, its Group list open.
//! - `ui.library.select {entry, menu?}`: select a library entry, and open its context menu.
//! - `ui.library.filter {open}`: open or close the Library filter popup.
//! - `ui.graph.openFunction {node, parameter, base?}`: open a parameter's function editor; with the FX-Map editor
//!   open, `node` is the pattern node.
//! - `ui.drag.hold {graph, document?, pos}`: press on the graph's Explorer row and drag to `pos`, held.
//! - `ui.drag.release {drop?}`: release it there (`drop`, default true) or cancel it with Escape.
//! - `ui.set` keys: `preferencesPage`, `cookHold`, `view3d {sceneBrowser, tool, displayToolsHidden, selected,
//!   inspected}`.
//! - `ui.inspect` adds `widgets` (the library's access listing, rects `[x0, y0, x1, y1]`), `wires`, `nodeSearch`,
//!   `view3d`, `view2dImage`, `view3dPicture`, `preferencesPage`, `cookHold`, `librarySelected`, `drag` and
//!   `hookErrors`.

use craft_automation::{access, control::Reply, synthetic};
use craft_ui::runtime::windows::WindowHost;
use egui::{Rect, ViewportId};
use serde_json::{Map, Value, json};

use crate::MaterialCraftApp;
use crate::explorer::ExplorerTarget;
use crate::i18n::tr;
use crate::view3d::{Subject, Tool};

/// A request to open a parameter's menu, left for the owning panel to serve on its next show.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParameterMenu {
    /// The node whose parameter it is.
    pub node: materialcraft_engine::core::NodeId,
    /// The parameter's id.
    pub param: String,
    /// The menu of the base value.
    pub base: bool,
    /// The menu of the inheritance setting.
    pub inheritance: bool,
}

/// The graph a `ui.drag.hold` holds, reported by `ui.inspect {drag}` until `ui.drag.release`.
struct HeldDrag {
    document: u64,
    graph: String,
}

/// Input that waits for a control to appear in the access tree: the menu or dialog it belongs to has not drawn yet.
enum Pending {
    /// Move the pointer to the submenu row with this label (`ui.explorer.menu {open}`).
    Hover(String),
    /// Focus the field with this label and open its list with Alt+Down (`ui.properties.expose {open: "group"}`).
    OpenList(String),
}

impl Pending {
    fn label(&self) -> &str {
        match self {
            Pending::Hover(label) | Pending::OpenList(label) => label,
        }
    }
}

/// What the hooks hold between frames: requests and records that live until a pass serves or reports them.
#[derive(Default)]
pub(crate) struct Hooks {
    /// A parameter menu a panel has to open; the panel clears it once the menu reports closed.
    pub(crate) parameter_menu: Option<ParameterMenu>,
    /// A library filter request (`Some(true)`: open the popup); the Library clears it once the popup reports closed.
    pub(crate) library_filter: Option<bool>,
    /// Set by the Properties panel in a pass it binds `parameter_menu`; `after_draw` reports a request no pass bound.
    pub(crate) parameter_menu_shown: bool,
    /// Set by the Library in a pass it binds `library_filter`; `after_draw` reports a request no pass bound.
    pub(crate) library_filter_shown: bool,
    drag: Option<HeldDrag>,
    pending: Option<Pending>,
    /// Requests that did not land, for the next `ui.inspect`.
    errors: Vec<String>,
}

const NO_ITEM: &str = "no open package has that Explorer item";

const TOOLS: [(Tool, &str); 3] = [(Tool::Camera, "camera"), (Tool::Light, "light"), (Tool::Select, "select")];

const SUBJECTS: [(Subject, &str); 5] = [
    (Subject::Material, "material"),
    (Subject::Lights, "lights"),
    (Subject::Camera, "camera"),
    (Subject::Environment, "environment"),
    (Subject::Renderer, "renderer"),
];

fn tool_name(tool: Tool) -> &'static str {
    TOOLS.iter().find(|(t, _)| *t == tool).map_or("camera", |(_, name)| *name)
}

/// The `ui.set` keys of state that is not saved with the UI, read off the patch before it merges.
#[derive(Default)]
pub(crate) struct SetKeys {
    page: Option<String>,
    cook_hold: Option<bool>,
    scene_browser: Option<bool>,
    tool: Option<Tool>,
    display_tools_hidden: Option<bool>,
    selected: Option<bool>,
    inspected: Option<Option<Subject>>,
}

impl SetKeys {
    /// Takes the hook keys out of `patch`, or says why one is not valid; a refused patch applies nothing.
    pub(crate) fn take(patch: &mut Value) -> Result<SetKeys, String> {
        let Some(patch) = patch.as_object_mut() else { return Ok(SetKeys::default()) };
        let flag = |value: &Value, key: &str| value.as_bool().ok_or(format!("`{key}` must be true or false"));
        let mut keys = SetKeys::default();
        if let Some(page) = patch.remove("preferencesPage") {
            let name = page.as_str().filter(|name| crate::preferences::PAGES.contains(name));
            keys.page = Some(name.ok_or(format!("unknown preferencesPage `{page}`"))?.to_owned());
        }
        if let Some(hold) = patch.remove("cookHold") {
            keys.cook_hold = Some(flag(&hold, "cookHold")?);
        }
        if let Some(view) = patch.remove("view3d") {
            let view = view.as_object().ok_or("`view3d` must be an object")?;
            for (key, value) in view {
                match key.as_str() {
                    "sceneBrowser" => keys.scene_browser = Some(flag(value, key)?),
                    "displayToolsHidden" => keys.display_tools_hidden = Some(flag(value, key)?),
                    "selected" => keys.selected = Some(flag(value, key)?),
                    "tool" => {
                        let name = value.as_str().unwrap_or_default();
                        let tool = TOOLS.iter().find(|(_, tool)| *tool == name).map(|(tool, _)| *tool);
                        keys.tool = Some(tool.ok_or(format!("unknown view3d tool `{value}` (camera, light, select)"))?);
                    }
                    "inspected" if value.is_null() => keys.inspected = Some(None),
                    "inspected" => {
                        let name = value.as_str().unwrap_or_default();
                        let subject = SUBJECTS.iter().find(|(_, subject)| *subject == name).map(|(subject, _)| *subject);
                        keys.inspected =
                            Some(Some(subject.ok_or(format!("unknown view3d inspected `{value}` (material, lights, camera, environment, renderer, null)"))?));
                    }
                    other => return Err(format!("unknown view3d key `{other}`")),
                }
            }
        }
        Ok(keys)
    }

    /// Applies the keys to `app`.
    pub(crate) fn apply(self, app: &mut MaterialCraftApp) {
        if let Some(page) = self.page {
            app.ui.preferences_page = page;
        }
        if let Some(hold) = self.cook_hold {
            app.cooker.hold = hold;
        }
        let view = &mut app.view3d;
        if let Some(open) = self.scene_browser {
            view.scene_browser = open;
        }
        if let Some(tool) = self.tool {
            view.tool = tool;
        }
        if let Some(hidden) = self.display_tools_hidden {
            view.display_tools_hidden = hidden;
        }
        if let Some(selected) = self.selected {
            view.selected = selected;
        }
        match self.inspected {
            Some(Some(subject)) => crate::view3d::inspect(app, subject),
            Some(None) => app.view3d.inspected = None,
            None => {}
        }
    }
}

/// Queues the library's synthetic input `method` in the main window, validated as a client's request would be.
fn input(ctx: &egui::Context, method: &str, params: Value) -> Result<(), String> {
    let host = WindowHost::of(ctx);
    synthetic::input_method(method, &params, &host, ViewportId::ROOT, &|_, _| None)
        .unwrap_or_else(|| Err(format!("`{method}` is not a synthetic input method")))
}

/// The rect of the control `control` (`{name, role?, within?}`) in the main window's last settled access tree.
fn rect_of(ctx: &egui::Context, control: &Value) -> Result<Rect, String> {
    access::Target::from_params(control)?.rect(&WindowHost::of(ctx), ViewportId::ROOT)
}

/// The library's rect form, as its `widgets` listing writes it.
fn rect_json(rect: Rect) -> Value {
    json!([rect.min.x, rect.min.y, rect.max.x, rect.max.y])
}

/// The rect of the canvas named `name` (an image surface), with what `inner` computes inside it; null when the
/// canvas is not on screen.
fn canvas(ctx: &egui::Context, name: &str, inner: impl FnOnce(Rect) -> Value) -> Value {
    match rect_of(ctx, &json!({"name": tr(name), "role": "Image"})) {
        Ok(area) => inner(area),
        Err(_) => Value::Null,
    }
}

/// A target without `document` names the active document, so a states file need not know document ids, which keep
/// counting up as packages close and open.
fn explorer_target(app: &MaterialCraftApp, p: &Value) -> Result<ExplorerTarget, String> {
    let mut target = p.get("target").cloned().unwrap_or(Value::Null);
    if let Some(fields) = target.as_object_mut() {
        fields.entry("document").or_insert_with(|| json!(app.session.document_id));
    }
    serde_json::from_value(target)
        .map_err(|error| format!("`target` must be an Explorer target such as {{\"kind\": \"graph\", \"document\": 0, \"index\": 0}}: {error}"))
}

/// The `control` target of the Explorer row of `target`: its name inside the Explorer panel.
fn explorer_row(app: &MaterialCraftApp, target: &ExplorerTarget) -> Result<Value, String> {
    if !crate::explorer::exists(app, target) {
        return Err(NO_ITEM.into());
    }
    let package = app.session.package(target.document()).map_err(|error| error.to_string())?;
    let name = target.name(package).ok_or(NO_ITEM)?;
    Ok(json!({"name": name, "within": tr("Explorer")}))
}

/// Leaves `pending` for `step`, reporting one it replaces that never drew.
fn wait_for(app: &mut MaterialCraftApp, pending: Pending) {
    if let Some(old) = app.hooks.pending.replace(pending) {
        app.hooks.errors.push(format!("`{}` never drew before the next request", old.label()));
    }
}

/// The fields the hooks add to `ui.inspect`; it drains the errors they collected.
pub(crate) fn inspect(app: &mut MaterialCraftApp, ctx: &egui::Context, into: &mut Map<String, Value>) {
    let host = WindowHost::of(ctx);
    let widgets = access::inspect(&host, ViewportId::ROOT, &json!({})).ok().and_then(|mut listing| listing.get_mut("widgets").map(Value::take));
    into.insert("widgets".into(), json!(widgets));
    into.insert(
        "wires".into(),
        json!(app.graph.wires.iter().map(|((to, port), points)| json!({"to": to.0, "toPort": port, "points": points.map(|p| [p.x, p.y])})).collect::<Vec<_>>()),
    );
    into.insert("nodeSearch".into(), json!(crate::graph::search::inspect(app, ctx)));
    let view = &app.view3d;
    let inspected = crate::view3d::inspected(app).and_then(|subject| SUBJECTS.iter().find(|(s, _)| *s == subject).map(|(_, name)| *name));
    into.insert(
        "view3d".into(),
        json!({
            "sceneBrowser": view.scene_browser,
            "tool": tool_name(view.tool),
            "displayToolsHidden": view.display_tools_hidden,
            "selected": view.selected,
            "inspected": inspected,
        }),
    );
    into.insert(
        "view2dImage".into(),
        canvas(ctx, "2D view", |area| json!({"area": rect_json(area), "image": crate::view2d::image_rect(app, area).map(rect_json)})),
    );
    into.insert(
        "view3dPicture".into(),
        canvas(ctx, "3D view", |area| json!({"area": rect_json(area), "shown": rect_json(crate::view3d::picture_rect(app, area))})),
    );
    into.insert("preferencesPage".into(), json!(app.ui.preferences_page));
    into.insert("cookHold".into(), json!(app.cooker.hold));
    into.insert("librarySelected".into(), json!(app.ui.library_selected));
    into.insert("drag".into(), json!(app.hooks.drag.as_ref().map(|drag| json!({"document": drag.document, "graph": drag.graph}))));
    let mut errors = std::mem::take(&mut app.hooks.errors);
    if let Some(pending) = &app.hooks.pending {
        errors.push(format!("`{}` has not drawn yet", pending.label()));
    }
    into.insert("hookErrors".into(), json!(errors));
}

/// Answers a hook method, or `None` when `method` is not one of them. A hook that queued input answers
/// [`Reply::AfterInput`], so its reply goes out once that input has run.
pub(crate) fn handle(app: &mut MaterialCraftApp, ctx: &egui::Context, method: &str, p: &Value) -> Option<Reply> {
    let answer = match method {
        "ui.toast" => toast(app, p),
        "ui.update.status" => update_status(app, p),
        "ui.explorer.select" => explorer_select(app, p),
        "ui.explorer.menu" => explorer_menu(app, ctx, p),
        "ui.explorer.rename" => explorer_rename(app, ctx, p),
        "ui.properties.parameterMenu" => parameter_menu(app, p),
        "ui.properties.expose" => expose(app, p),
        "ui.library.select" => library_select(app, ctx, p),
        "ui.library.filter" => match p.get("open").and_then(Value::as_bool) {
            Some(open) => {
                app.hooks.library_filter = Some(open);
                Ok(Some(Value::Null))
            }
            None => Err("`open` (true or false) is required".into()),
        },
        "ui.graph.openFunction" => open_function(app, p),
        "ui.drag.hold" => drag_hold(app, ctx, p),
        "ui.drag.release" => drag_release(app, ctx, p),
        _ => return None,
    };
    // `Ok(None)`: input was queued and the reply waits for it.
    Some(match answer {
        Ok(Some(value)) => Reply::from_result(Ok(value)),
        Ok(None) => Reply::AfterInput,
        Err(error) => Reply::from_result(Err(error)),
    })
}

/// What a hook answers: a value now, `None` once its queued input has run, or why it was refused.
type Answer = Result<Option<Value>, String>;

fn toast(app: &mut MaterialCraftApp, p: &Value) -> Answer {
    let text = p.get("text").and_then(Value::as_str).ok_or("`text` is required")?;
    app.toast(text, p.get("error").and_then(Value::as_bool).unwrap_or(false));
    Ok(Some(Value::Null))
}

#[cfg(not(target_arch = "wasm32"))]
fn update_status(app: &mut MaterialCraftApp, p: &Value) -> Answer {
    let status = p.get("status").and_then(Value::as_str).ok_or("`status` is required")?;
    let text = |key: &str| p.get(key).and_then(Value::as_str);
    crate::updates_ui::scripted_status(app, status, text("version"), text("message"))?;
    Ok(Some(Value::Null))
}

#[cfg(target_arch = "wasm32")]
fn update_status(_: &mut MaterialCraftApp, _: &Value) -> Answer {
    Err("the browser build has no updater".into())
}

fn explorer_select(app: &mut MaterialCraftApp, p: &Value) -> Answer {
    let target = explorer_target(app, p)?;
    if !crate::explorer::exists(app, &target) {
        return Err(NO_ITEM.into());
    }
    crate::explorer::select(app, target);
    Ok(Some(Value::Null))
}

fn explorer_menu(app: &mut MaterialCraftApp, ctx: &egui::Context, p: &Value) -> Answer {
    let target = explorer_target(app, p)?;
    if let Some(command) = p.get("pick").and_then(Value::as_str) {
        if !crate::explorer::exists(app, &target) {
            return Err(NO_ITEM.into());
        }
        // `invoke` refuses the renames with the message that points to `ui.explorer.rename`.
        crate::explorer::invoke(app, target, command)?;
        return Ok(Some(Value::Null));
    }
    let row = explorer_row(app, &target)?;
    input(ctx, "ui.click", json!({"control": row, "button": "secondary"}))?;
    if let Some(label) = p.get("open").and_then(Value::as_str) {
        wait_for(app, Pending::Hover(tr(label).to_owned()));
    }
    Ok(None)
}

fn explorer_rename(app: &mut MaterialCraftApp, ctx: &egui::Context, p: &Value) -> Answer {
    let target = explorer_target(app, p)?;
    if !matches!(target, ExplorerTarget::Graph { .. } | ExplorerTarget::Folder { .. }) {
        return Err("only graphs and folders are renamed".into());
    }
    let row = explorer_row(app, &target)?;
    crate::explorer::select(app, target);
    // A click on the row gives the list the keyboard focus, where F2 starts the rename.
    input(ctx, "ui.click", json!({"control": row}))?;
    input(ctx, "ui.key", json!({"key": "F2"}))?;
    if let Some(name) = p.get("name").and_then(Value::as_str) {
        input(ctx, "ui.key", json!({"key": "A", "command": true}))?;
        input(ctx, "ui.type", json!({"text": name}))?;
    }
    Ok(None)
}

/// The `id` (a node) and `param` fields the Properties hooks share.
fn node_param(p: &Value) -> Result<(materialcraft_engine::core::NodeId, &str), String> {
    match (p.get("id").and_then(Value::as_u64), p.get("param").and_then(Value::as_str)) {
        (Some(node), Some(param)) => Ok((materialcraft_engine::core::NodeId(node), param)),
        _ => Err("`id` (a node) and `param` are required".into()),
    }
}

fn parameter_menu(app: &mut MaterialCraftApp, p: &Value) -> Answer {
    let (node, param) = node_param(p)?;
    let inheritance = match p.get("menu").and_then(Value::as_str) {
        None | Some("actions") => false,
        Some("inheritance") => true,
        Some(other) => return Err(format!("unknown menu `{other}` (actions, inheritance)")),
    };
    let base = inheritance || p.get("base").and_then(Value::as_bool).unwrap_or(false);
    app.hooks.parameter_menu = Some(ParameterMenu { node, param: param.to_owned(), base, inheritance });
    Ok(Some(json!({"opens": "next frame; a menu the Properties panel does not draw is listed in ui.inspect hookErrors"})))
}

fn expose(app: &mut MaterialCraftApp, p: &Value) -> Answer {
    let (node, param) = node_param(p)?;
    let open_groups = match p.get("open").and_then(Value::as_str) {
        None => false,
        Some("group") => true,
        Some(other) => return Err(format!("unknown list `{other}` (group)")),
    };
    crate::properties::expose(app, node, param)?;
    let text = |key: &str| p.get(key).and_then(Value::as_str);
    let dialog = app.expose_parameter_dialog.as_mut().ok_or("the Expose Parameter dialog did not open")?;
    dialog.preset_fields(text("identifier"), text("label"), text("group"), text("description"));
    if !open_groups {
        return Ok(Some(Value::Null));
    }
    // The Group field appears in the access tree after the dialog's first draw.
    wait_for(app, Pending::OpenList(tr("Group").to_owned()));
    Ok(Some(json!({"opens": "once the dialog has drawn; a list that does not open is listed in ui.inspect hookErrors"})))
}

fn library_select(app: &mut MaterialCraftApp, ctx: &egui::Context, p: &Value) -> Answer {
    let id = p.get("entry").and_then(Value::as_str).ok_or("`entry` (a library entry id) is required")?;
    let entries = crate::library::entries(app);
    let label = entries.iter().find(|entry| entry.id == id).map(|entry| entry.label.clone()).ok_or(format!("the library has no entry `{id}`"))?;
    app.ui.library_selected = Some(id.to_owned());
    if !p.get("menu").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(Some(Value::Null));
    }
    input(ctx, "ui.click", json!({"control": {"name": label, "role": "ListItem"}, "button": "secondary"}))?;
    Ok(None)
}

fn open_function(app: &mut MaterialCraftApp, p: &Value) -> Answer {
    let (Some(node), Some(parameter)) = (p.get("node").and_then(Value::as_u64), p.get("parameter").and_then(Value::as_str)) else {
        return Err("`node` and `parameter` are required".into());
    };
    if let Some(editor) = &app.pattern_editor {
        let pattern_node = u32::try_from(node).map_err(|_| "`node` must be a pattern node of the open FX-Map")?;
        let owner = editor.node;
        let target = crate::function::FunctionTarget::Pattern { node: pattern_node, parameter: parameter.to_owned() };
        crate::graph::subgraph_navigation::open_function(app, owner, target);
        app.reconcile_editors();
        return if app.function_editor.is_some() { Ok(Some(Value::Null)) } else { Err("that pattern parameter holds no function".into()) };
    }
    let base = p.get("base").and_then(Value::as_bool).unwrap_or(false);
    match app.run("graph.node.function.edit", json!({"id": node, "param": parameter, "base": base})) {
        Some(_) if app.function_editor.is_some() => Ok(Some(Value::Null)),
        Some(_) => Err("the parameter's function editor did not open".into()),
        None => Err("the parameter has no function to edit (see the toast)".into()),
    }
}

fn drag_hold(app: &mut MaterialCraftApp, ctx: &egui::Context, p: &Value) -> Answer {
    let Some([x, y]) = p.get("pos").and_then(Value::as_array).and_then(|a| Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])) else {
        return Err("`pos` [x, y] is required".into());
    };
    let document = p.get("document").and_then(Value::as_u64).unwrap_or(app.session.document_id);
    let package = app.session.package(document).map_err(|_| format!("no open package {document}"))?;
    let found = match p.get("graph") {
        Some(Value::Number(index)) => index.as_u64().and_then(|index| usize::try_from(index).ok()).and_then(|index| Some((index, package.graphs.get(index)?))),
        Some(Value::String(name)) => package.graph_by_name(name),
        _ => return Err("`graph` (an index or a name) is required".into()),
    };
    let (index, graph) = found.ok_or("the package has no such graph")?;
    let graph = graph.sbs_path();
    let row = explorer_row(app, &ExplorerTarget::Graph { document, index })?;
    input(ctx, "ui.click", json!({"control": row, "hold": true}))?;
    input(ctx, "ui.move", json!({"x": x, "y": y}))?;
    app.hooks.drag = Some(HeldDrag { document, graph });
    Ok(None)
}

fn drag_release(app: &mut MaterialCraftApp, ctx: &egui::Context, p: &Value) -> Answer {
    if app.hooks.drag.is_none() {
        return Err("no drag is held".into());
    }
    if !p.get("drop").and_then(Value::as_bool).unwrap_or(true) {
        input(ctx, "ui.key", json!({"key": "Escape"}))?;
    }
    input(ctx, "ui.release", json!({"button": "left"}))?;
    app.hooks.drag = None;
    Ok(None)
}

/// Once per frame in the logic phase: serves the pending input that waits for its control. It reads the access tree
/// the window host kept from the last settled pass, never the frame's input, so a frame's second pass changes
/// nothing; it waits while queued input has not all run, since the tree before that is the one without the menu.
pub(crate) fn step(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let Some(label) = app.hooks.pending.as_ref().map(|pending| pending.label().to_owned()) else { return };
    if !WindowHost::of(ctx).injected_idle() {
        return;
    }
    let found = rect_of(ctx, &json!({"name": label}));
    // `Target::rect` reports a missing tree or control through the framework's stable error constants.
    if let Err(error) = &found
        && (error.starts_with(access::NO_CONTROL) || error == access::NO_TREE)
    {
        return;
    }
    let Some(pending) = app.hooks.pending.take() else { return };
    let served = found.and_then(|rect| match pending {
        Pending::Hover(_) => input(ctx, "ui.move", json!({"x": rect.center().x, "y": rect.center().y})),
        Pending::OpenList(_) => {
            input(ctx, "ui.focus", json!({"label": label}))?;
            input(ctx, "ui.key", json!({"key": "Down", "alt": true}))
        }
    });
    if let Err(error) = served {
        app.hooks.errors.push(format!("`{label}`: {error}"));
    }
}

/// Once after the frame is drawn: a request no panel bound this frame is reported to the hooks' errors and cleared.
pub(crate) fn after_draw(app: &mut MaterialCraftApp) {
    let hooks = &mut app.hooks;
    if !std::mem::take(&mut hooks.parameter_menu_shown)
        && let Some(menu) = hooks.parameter_menu.take()
    {
        hooks.errors.push(format!("ui.properties.parameterMenu: the Properties panel did not draw the menu of {} on node {}", menu.param, menu.node.0));
    }
    if !std::mem::take(&mut hooks.library_filter_shown) && hooks.library_filter.take().is_some() {
        hooks.errors.push("ui.library.filter: the Library panel did not draw its filter button".into());
    }
}
