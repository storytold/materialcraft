//! The dock host: every panel (explorer, library, graph, 3D view, 2D view, properties, console,
//! history) is a tab that can be docked, tabbed, resized, floated and closed. craft-ui's
//! `DockArea` draws the frames, tab strips, floating windows and drop previews and owns the
//! gestures; this module names the panels, builds the default arrangement, applies the dock's
//! changes and hosts the panel bodies.
//!
//! The graph is the central canvas: it stays docked, never takes tabs and is the only panel that
//! cannot close or float. The layout persists with `UiState` as a craft-behavior `Workspace`; a
//! layout that does not load or validate becomes the default.

use std::sync::Arc;

use craft_behavior::dock::{Action, DockPlacement, Layout, Location, Node, PanelState, Permissions, SplitSize};
use craft_behavior::flow::Axis;
use craft_ui::contract::{Bind, BodyCx, PlainData, SizeLimits, Tracked};
use craft_ui::docking::{DockConfig, DockPanels, GroupAt, PanelSpec, PanelSpecs};
use craft_ui::patterns::dock::DockArea;
use egui::{Margin, Rect, Ui, vec2};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::MaterialCraftApp;
use crate::i18n::tr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Panel {
    Graph,
    Explorer,
    Library,
    View3d,
    View2d,
    Properties,
    Console,
    History,
}

/// Windows menu items and the panels they show or hide.
pub const VIEWS: [(&str, Panel); 7] = [
    ("ui.view.explorer", Panel::Explorer),
    ("ui.view.properties", Panel::Properties),
    ("ui.view.console", Panel::Console),
    ("ui.view.3d", Panel::View3d),
    ("ui.view.2d", Panel::View2d),
    ("ui.view.library", Panel::Library),
    ("ui.view.history", Panel::History),
];

/// The panel a Windows menu item toggles.
pub fn view_panel(id: &str) -> Option<Panel> {
    VIEWS.iter().find(|(v, _)| *v == id).map(|(_, p)| *p)
}

impl Panel {
    pub const ALL: [Panel; 8] =
        [Panel::Graph, Panel::Explorer, Panel::Library, Panel::View3d, Panel::View2d, Panel::Properties, Panel::Console, Panel::History];

    pub fn name(self) -> &'static str {
        match self {
            Panel::Graph => "Graph",
            Panel::Explorer => "Explorer",
            Panel::Library => "Library",
            Panel::View3d => "3D View",
            Panel::View2d => "2D View",
            Panel::Properties => "Properties",
            Panel::Console => "Console",
            Panel::History => "History",
        }
    }

    /// Where the panel floats when it has never been laid out, cascaded.
    fn float_rect(self) -> Option<Rect> {
        let slot = match self {
            Panel::Console => 0.0,
            Panel::History => 1.0,
            _ => return None,
        };
        Some(Rect::from_min_size(egui::pos2(900.0, 120.0 + 50.0 * slot), vec2(420.0, 320.0)))
    }
}

/// The saved panel arrangement: craft-behavior's workspace over MaterialCraft's panels.
pub type Workspace = craft_behavior::dock::Workspace<Panel>;

/// The graph never moves, floats, closes or takes tabs; panels dock beside it.
pub fn permissions(panel: &Panel) -> Permissions {
    if *panel == Panel::Graph { Permissions::PROTECTED.accepts_splits(true) } else { Permissions::default() }
}

fn tabs(panels: &[Panel]) -> Node<Panel> {
    Node::tabs(panels.to_vec(), 0)
}

fn split(axis: Axis, ratio: f32, first: Node<Panel>, second: Node<Panel>) -> Node<Panel> {
    Node::split(axis, SplitSize::Ratio(ratio), first, second)
}

/// The default: explorer over library on the left; the graph over the 3D and 2D views in the
/// middle; properties on the right. Console and history are closed and open floating.
pub fn default_workspace() -> Workspace {
    use Panel::*;
    let left = split(Axis::Y, 0.36, tabs(&[Explorer]), tabs(&[Library]));
    let views = split(Axis::X, 0.5, tabs(&[View3d]), tabs(&[View2d]));
    let center = split(Axis::Y, 0.52, tabs(&[Graph]), views);
    let root = split(Axis::X, 0.15, left, split(Axis::X, 0.8, center, tabs(&[Properties])));
    let hidden = Panel::ALL
        .into_iter()
        .filter_map(|panel| {
            let rect = panel.float_rect()?;
            let mut location = Location::new(None, DockPlacement::tab(None));
            location.floating = Some(rect);
            Some((panel, location))
        })
        .collect();
    Workspace::from_parts(Layout::new(Some(root), Vec::new()), hidden, Vec::new())
}

/// craft-behavior's checks plus the graph's: a saved or hand-edited file cannot hide, float,
/// group or collapse it.
pub fn validate(workspace: &Workspace) -> Result<(), String> {
    workspace.validate().map_err(|error| error.to_string())?;
    let layout = workspace.layout();
    if layout.floating.iter().any(|window| window.contains(&Panel::Graph)) {
        return Err("The graph cannot float".into());
    }
    if workspace.hidden().iter().any(|(panel, _)| *panel == Panel::Graph) {
        return Err("The graph cannot be hidden".into());
    }
    let mut found = false;
    let mut nodes: Vec<&Node<Panel>> = layout.root.iter().collect();
    while let Some(node) = nodes.pop() {
        match node {
            Node::Split { first, second, .. } => {
                nodes.push(first);
                nodes.push(second);
            }
            Node::Tabs { panels, .. } if panels.contains(&Panel::Graph) => {
                if panels.len() != 1 {
                    return Err("The graph cannot share a tab group".into());
                }
                found = true;
            }
            Node::Stack { entries, .. } if entries.iter().any(|entry| entry.panel == Panel::Graph) => {
                return Err("The graph cannot be collapsed".into());
            }
            Node::Dock { columns, .. } if columns.iter().flat_map(|column| &column.groups).any(|group| group.panels.contains(&Panel::Graph)) => {
                return Err("The graph cannot be in a dock".into());
            }
            _ => {}
        }
    }
    if found { Ok(()) } else { Err("The panel layout must contain the graph".into()) }
}

// ---------------------------------------------------------------------------------------------
// The live dock

/// The dock layout (part of `UiState`; serialized as a craft-behavior `Workspace`).
#[derive(Clone, Debug)]
pub struct Dock {
    workspace: Workspace,
    /// A panel to bring in front of the other floating windows on the next frame.
    raise: Option<Panel>,
}

impl Default for Dock {
    fn default() -> Self {
        Dock { workspace: default_workspace(), raise: None }
    }
}

impl PartialEq for Dock {
    fn eq(&self, other: &Self) -> bool {
        self.workspace == other.workspace
    }
}

impl Serialize for Dock {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.workspace.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Dock {
    /// Never fails: a layout that cannot be read or fails the graph's rules is the default.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Dock::from_value(Value::deserialize(d)?))
    }
}

impl Dock {
    /// The dock for a saved layout; the default when it is not a valid workspace.
    pub fn from_value(value: Value) -> Dock {
        let default = default_workspace();
        let loaded = serde_json::from_value::<Workspace>(value).ok();
        let Some(mut workspace) = loaded.filter(|w| validate(w).is_ok()) else { return Dock::default() };
        workspace.reconcile(|_| true, permissions, &default);
        if validate(&workspace).is_err() {
            return Dock::default();
        }
        Dock { workspace, raise: None }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn is_open(&self, p: Panel) -> bool {
        self.workspace.layout().contains(&p)
    }

    /// Open and the front tab of its group.
    pub fn is_visible(&self, p: Panel) -> bool {
        self.workspace.layout().panel_state(&p) == PanelState::Front
    }

    /// Puts the default layout back. It replaces the workspace instead of editing it, which ends a drag in progress.
    pub fn reset(&mut self) {
        self.workspace.replace(default_workspace());
        self.raise = None;
    }

    /// Hide a visible panel, otherwise bring it up.
    pub fn toggle(&mut self, p: Panel) {
        if self.is_visible(p) { self.close(p) } else { self.focus(p) }
    }

    pub fn close(&mut self, p: Panel) {
        if let Err(error) = self.workspace.set_visible(p, false, None, false, permissions) {
            log::warn!("panel {p:?} was not closed: {error}");
        }
    }

    /// Open `p` (where it was when closed, else where it lives by default) and bring it to the
    /// front.
    pub fn focus(&mut self, p: Panel) {
        let mut next = self.workspace.clone();
        match next.set_visible(p, true, None, false, permissions) {
            Ok(()) if validate(&next).is_ok() => {
                self.workspace = next;
                self.raise = Some(p);
            }
            Ok(()) => log::warn!("panel {p:?} was not opened: it would break the graph's rules"),
            Err(error) => log::warn!("panel {p:?} was not opened: {error}"),
        }
    }

    /// One change from the dock area's gestures. A change that is rejected leaves the layout as
    /// it was.
    fn apply(&mut self, action: Action<Panel>) {
        let mut next = self.workspace.clone();
        match next.apply(action, false, permissions) {
            Ok(_) if validate(&next).is_ok() => self.workspace = next,
            Ok(_) => log::warn!("a panel move would break the graph's rules"),
            Err(error) => log::warn!("a panel move was not applied: {error}"),
        }
    }
}

/// What the dock area keeps between passes: the workspace it shows and the panels' specs.
#[derive(Default)]
pub(crate) struct LiveDock {
    shown: Option<Arc<Workspace>>,
    specs: Tracked<DockSpecs>,
}

impl LiveDock {
    /// The `Arc` the dock shows for `workspace`: the last one while the arrangement is the same,
    /// so an idle pass keeps the workspace's revision and the dock reads no spec.
    fn shown(&mut self, workspace: &Workspace) -> Arc<Workspace> {
        if let Some(shown) = self.shown.as_ref().filter(|shown| ***shown == *workspace) {
            return Arc::clone(shown);
        }
        let shown = Arc::new(workspace.clone());
        self.shown = Some(Arc::clone(&shown));
        shown
    }
}

/// What the dock reads of the panels: their tab names, in [`Panel::ALL`] order.
#[derive(Clone, Debug, Default, PartialEq, PlainData)]
struct DockSpecs {
    labels: Vec<String>,
}

impl PanelSpecs<Panel> for DockSpecs {
    fn spec(&self, panel: &Panel) -> PanelSpec {
        let label = Panel::ALL.iter().position(|p| p == panel).and_then(|index| self.labels.get(index)).cloned().unwrap_or_default();
        // Every panel body is MaterialCraft's own egui code: the dock runs it with the window's style.
        let spec = PanelSpec::new(label).limits(limits(panel)).float_size(vec2(420.0, 320.0)).host_body();
        if *panel == Panel::Graph { spec.canvas() } else { spec }
    }

    fn permissions(&self, panel: &Panel) -> Permissions {
        permissions(panel)
    }
}

/// Smallest body sizes a divider drag leaves.
fn limits(panel: &Panel) -> SizeLimits {
    SizeLimits::at_least(if *panel == Panel::Graph { vec2(240.0, 160.0) } else { vec2(120.0, 60.0) })
}

/// The tab name of `panel`: what it shows (the selection, the scene, the image) in the interface
/// language.
fn label(app: &MaterialCraftApp, panel: Panel) -> String {
    match panel {
        Panel::Properties => crate::properties::title(app),
        Panel::Graph => app.session.graph().map(|g| format!("{} - {}", g.name, tr("Graph"))).unwrap_or_else(|_| tr("Graph").into()),
        Panel::View3d => format!("{} - {}", crate::view3d::scene_label(app), tr("3D View")),
        Panel::View2d => crate::view2d::title(app),
        p => tr(p.name()).to_string(),
    }
}

/// Reads the tab names, moving the specs' revision only when one changed.
fn refresh_specs(app: &mut MaterialCraftApp) {
    let labels: Vec<String> = Panel::ALL.iter().map(|panel| label(app, *panel)).collect();
    if app.docks.specs.labels != labels {
        app.docks.specs.labels = labels;
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing

/// Draw the dock (graph and panels) into `ui`.
pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    refresh_specs(app);
    let shown = app.docks.shown(&app.ui.dock.workspace);
    // The specs leave the app while its panels borrow it, and come back with their revision.
    let specs = std::mem::take(&mut app.docks.specs);
    let raise = app.ui.dock.raise.take();
    // Panels arrange in a split tree around the graph; floating title bars only move windows.
    let config = DockConfig::default().new_dock_zones(false).title_bar_docks(false);
    let output = {
        let mut panels = Panels { app: &mut *app };
        let mut area = DockArea::new("Panels", Bind::given(Arc::clone(&shown)), &specs, &mut panels, config);
        if let Some(panel) = raise {
            area = area.raise(panel);
        }
        area.show(ui)
    };
    app.docks.specs = specs;
    if let Some(error) = &output.error {
        log::warn!("docking: {:?}", error.error);
    }
    // A body that changed the layout (a panel opened from its own menu) wins over this frame's
    // gestures, which were made against the old one.
    if app.ui.dock.workspace != *shown {
        craft_ui::runtime::drag::cancel(&ctx, craft_behavior::CancelReason::HostChanged);
        return;
    }
    for action in output.actions.into_iter().chain(output.requested) {
        app.ui.dock.apply(action);
    }
}

/// The panels' contents for the dock area.
struct Panels<'a> {
    app: &'a mut MaterialCraftApp,
}

impl DockPanels<Panel> for Panels<'_> {
    fn body(&mut self, panel: &Panel, ui: &mut Ui, _: &mut BodyCx<'_>) {
        let app = &mut *self.app;
        match panel {
            // The graph is its group's `group_view`.
            Panel::Graph => {}
            Panel::View3d => crate::view3d::show(app, ui),
            Panel::View2d => crate::view2d::show(app, ui),
            _ => {
                egui::Frame::new().inner_margin(Margin::symmetric(6, 4)).show(ui, |ui| match panel {
                    Panel::Explorer => crate::explorer::show(app, ui),
                    Panel::Library => crate::library::show(app, ui),
                    Panel::Properties => crate::properties::show(app, ui),
                    Panel::Console => crate::windows::console(app, ui),
                    Panel::History => crate::windows::history(app, ui),
                    _ => {}
                });
            }
        }
    }

    /// The graph editor, edge to edge in its group.
    fn group_view(&mut self, panels: &[Panel], _: usize, ui: &mut Ui, at: GroupAt, _: &mut BodyCx<'_>) -> Option<f32> {
        let GroupAt::Show(rect) = at else { return None };
        if !panels.contains(&Panel::Graph) {
            return None;
        }
        crate::graph::show(self.app, ui);
        Some(rect.height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_workspace_has_every_panel_and_is_valid() {
        let w = default_workspace();
        assert!(validate(&w).is_ok());
        for panel in Panel::ALL {
            assert!(w.layout().contains(&panel) || w.hidden().iter().any(|(p, _)| *p == panel), "{panel:?} is neither shown nor hidden");
        }
    }

    #[test]
    fn floating_panels_stop_while_a_dialog_is_open() {
        let mut app = crate::MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        let ctx = egui::Context::default();
        let modal = |app: &mut crate::MaterialCraftApp| app.frame_input(&ctx, Default::default(), 1.0).app_modal;

        assert_eq!(modal(&mut app), None);
        app.ui.windows.unsaved = Some(crate::unsaved::Guarded::Quit);
        assert_eq!(modal(&mut app), Some(egui::ViewportId::ROOT), "the panels stop on the frame the prompt opens");
        app.ui.windows.unsaved = None;
        app.ui.windows.shortcuts = true;
        assert_eq!(modal(&mut app), None, "the Shortcuts list does not block");
    }

    #[test]
    fn a_focused_panel_is_raised_for_one_frame() {
        let mut app = crate::MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        let ctx = egui::Context::default();
        crate::framework::install_headless(&ctx);
        app.ui.dock.focus(Panel::Properties);
        assert_eq!(app.ui.dock.raise, Some(Panel::Properties));

        // The first frame only installs the fonts
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.draw(ui)).textures_delta.clear();
        }

        assert_eq!(app.ui.dock.raise, None);
    }

    #[test]
    fn default_dock_round_trips() {
        let dock = Dock::default();
        let json = serde_json::to_string(&dock).unwrap();
        let back: Dock = serde_json::from_str(&json).unwrap();
        assert_eq!(back, dock);
    }

    #[test]
    fn only_a_reset_replaces_the_workspace() {
        let mut dock = Dock::default();
        let start = dock.workspace().generation();
        dock.focus(Panel::Console);
        dock.close(Panel::Library);
        assert_eq!(dock.workspace().generation(), start);
        dock.reset();
        assert_ne!(dock.workspace().generation(), start);
        assert_eq!(dock, Dock::default());
    }

    #[test]
    fn the_graph_stays_put_and_panels_come_back() {
        let mut dock = Dock::default();
        dock.close(Panel::Graph);
        assert!(dock.is_open(Panel::Graph));
        let before = dock.clone();
        for p in [Panel::Properties, Panel::Library, Panel::View3d] {
            dock.close(p);
            assert!(!dock.is_open(p));
            dock.focus(p);
            assert!(dock.is_visible(p), "{p:?}");
            assert_eq!(dock, before, "{p:?} came back elsewhere");
        }
        dock.focus(Panel::Console);
        assert!(dock.is_visible(Panel::Console));
        assert_eq!(serde_json::from_value::<Dock>(serde_json::json!([1, 2])).unwrap(), Dock::default());
    }
}
