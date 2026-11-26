//! Preferences ▸ Graph: how the graph canvases respond and what opening a graph shows

use craft_ui::patterns::preferences::{GroupSpec, PageSpec, PrefValue, RowKind, RowSpec};

use super::preferences_page::{GRAPH, GRAPH_TITLE};
use crate::i18n::tr;
use crate::state::Prefs;

const TAB_SEARCH: &str = "graph.tabOpensNodeSearch";
const WIRE_DROP: &str = "graph.wireDropOpensNodeSearch";
const VIEW_OUTPUTS: &str = "graph.viewOutputsOnOpen";
const THUMBNAILS: &str = "graph.thumbnails";
const SNAP: &str = "graph.snapToGrid";

pub(super) fn page(prefs: &Prefs) -> PageSpec<'_> {
    let mut page = PageSpec::new(GRAPH, tr(GRAPH_TITLE), Vec::new());
    page.groups = vec![
        GroupSpec::new(
            Some(tr("Common")),
            vec![
                RowSpec::new(TAB_SEARCH, tr("Tab key shows the node search"), RowKind::Bool(prefs.tab_opens_node_search)),
                RowSpec::new(WIRE_DROP, tr("Create nodes by dropping a wire on empty canvas"), RowKind::Bool(prefs.wire_drop_opens_node_search)),
                RowSpec::new(VIEW_OUTPUTS, tr("View outputs in 3D view when opening a graph"), RowKind::Bool(prefs.view_outputs_on_open)),
            ],
        ),
        GroupSpec::new(
            Some(tr("Material graph")),
            vec![
                RowSpec::new(THUMBNAILS, tr("Show node thumbnails"), RowKind::Bool(prefs.thumbnails)),
                RowSpec::new(SNAP, tr("Snap to grid"), RowKind::Bool(prefs.snap_to_grid)),
            ],
        ),
    ];
    page
}

pub(super) fn set(prefs: &mut Prefs, key: &str, value: PrefValue) {
    let PrefValue::Bool(on) = value else { return };
    match key {
        TAB_SEARCH => prefs.tab_opens_node_search = on,
        WIRE_DROP => prefs.wire_drop_opens_node_search = on,
        VIEW_OUTPUTS => prefs.view_outputs_on_open = on,
        THUMBNAILS => prefs.thumbnails = on,
        SNAP => prefs.snap_to_grid = on,
        _ => {}
    }
}

pub(super) fn reset(prefs: &mut Prefs, defaults: &Prefs) {
    prefs.tab_opens_node_search = defaults.tab_opens_node_search;
    prefs.wire_drop_opens_node_search = defaults.wire_drop_opens_node_search;
    prefs.view_outputs_on_open = defaults.view_outputs_on_open;
    prefs.thumbnails = defaults.thumbnails;
    prefs.snap_to_grid = defaults.snap_to_grid;
}
