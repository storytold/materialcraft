//! The ids of the Preferences pages, which `UiState::preferences_page` and `ui.set {preferencesPage}` name, and
//! their English titles, in the order the page list shows them.

/// General: recent packages, history, language, appearance, views, autosave and previews.
pub(crate) const GENERAL: &str = "general";
/// Projects: the default 3D view.
pub(crate) const PROJECTS: &str = "projects";
/// Graph: how the graph canvases respond.
pub(crate) const GRAPH: &str = "graph";
/// 3D View: what a new 3D view shows and how its camera moves.
pub(crate) const VIEW3D: &str = "view3d";
/// Library: previews, their cache and the layout.
pub(crate) const LIBRARY: &str = "library";
/// Shortcuts: node creation shortcuts.
pub(crate) const SHORTCUTS: &str = "shortcuts";
/// Every page id, in list order.
pub(crate) const PAGES: [&str; 6] = [GENERAL, PROJECTS, GRAPH, VIEW3D, LIBRARY, SHORTCUTS];

/// The General page's title.
pub(super) const GENERAL_TITLE: &str = "General";
/// The Projects page's title.
pub(super) const PROJECTS_TITLE: &str = "Projects";
/// The Graph page's title.
pub(super) const GRAPH_TITLE: &str = "Graph";
/// The 3D View page's title.
pub(super) const VIEW3D_TITLE: &str = "3D View";
/// The Library page's title.
pub(super) const LIBRARY_TITLE: &str = "Library";
/// The Shortcuts page's title.
pub(super) const SHORTCUTS_TITLE: &str = "Shortcuts";
