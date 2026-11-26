use super::{
    NodeSearch,
    editor_target::EditorTarget,
    node_search::{SearchCandidate, SearchPick, SearchWire},
};
use crate::{
    MaterialCraftApp,
    function::{FunctionEditor, library::FunctionLibraryDrag},
    i18n::tr,
};
use craft_behavior::kurbo::Point;
use craft_ui::{
    contract::{ActivateOn, Bind, CollectionEvent, LeadOnOpen, Props as _, Show as _, ShowCollection as _, ShowContainer as _},
    list::{List, ListRow},
    popover::Popover,
    runtime::focus::Initial,
    text::TextField,
};
use egui::{Pos2, Ui};
use materialcraft_engine::{FunctionEdit, LibraryItem, core::NodeId};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    hash::{DefaultHasher, Hash, Hasher},
};

/// The editor whose search candidates are being read.
pub(crate) enum SearchEditor<'a> {
    /// The material graph.
    Material,
    /// The Function editor.
    Function(&'a mut FunctionEditor),
}

/// Opens a search at a graph point, leaving an open search for the same editor unchanged.
pub(crate) fn open(app: &mut MaterialCraftApp, editor: EditorTarget, at: Pos2, wire: Option<SearchWire>) {
    if app.graph.search.as_ref().is_some_and(|search| search.editor == editor) {
        return;
    }
    app.graph.search = Some(NodeSearch { at, text: String::new(), wire, editor });
}

/// Returns candidates whose label, id, and group contain every query word: the material graph's for `function` `None`,
/// else that Function editor's.
pub(crate) fn candidates(app: &MaterialCraftApp, function: Option<&FunctionEditor>, text: &str) -> Vec<SearchCandidate> {
    let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    let mut all: Vec<(String, SearchCandidate)> = Vec::new();
    match function {
        None => {
            for definition in materialcraft_engine::nodes::catalog().iter().filter(|d| d.id != "graph_instance") {
                let hay = format!("{} {} {}", definition.label.to_lowercase(), definition.id.to_lowercase(), definition.category.to_lowercase());
                if words.iter().all(|word| hay.contains(word)) {
                    let candidate = SearchCandidate {
                        label: tr(definition.label).to_string(),
                        group: tr(definition.group()).to_string(),
                        pick: SearchPick::Definition(definition.id),
                    };
                    all.push((definition.label.to_string(), candidate));
                }
            }
            if matches!(app.graph.search.as_ref().and_then(|search| search.wire.as_ref()), Some(SearchWire::To(_))) {
                all.retain(|(_, candidate)| match &candidate.pick {
                    SearchPick::Definition(id) => materialcraft_engine::nodes::find(id).is_some_and(|definition| !definition.outputs.is_empty()),
                    SearchPick::Function(_) => true,
                });
            }
        }
        Some(editor) => {
            let Some(target) = editor.command_target() else { return Vec::new() };
            let mut entries: Vec<(String, SearchCandidate)> = target
                .available_operations(&app.session)
                .map(|operation| {
                    let pick = SearchPick::Function(FunctionLibraryDrag::Operation(operation.id.into()));
                    (operation.id.to_string(), SearchCandidate { label: tr(operation.label).to_string(), group: tr(operation.category).to_string(), pick })
                })
                .collect();
            for entry in app.session.library_entries().iter() {
                if let LibraryItem::FunctionInstance { package, graph } = &entry.item
                    && entry.missing.is_none()
                {
                    let group = entry.category.rsplit('/').next().unwrap_or(&entry.category).to_string();
                    let pick = SearchPick::Function(FunctionLibraryDrag::Instance { package: package.clone(), graph: graph.clone() });
                    entries.push((entry.id.clone(), SearchCandidate { label: entry.label.clone(), group, pick }));
                }
            }
            let mut seen = BTreeSet::new();
            for (id, candidate) in entries {
                let hay = format!("{} {} {}", candidate.label.to_lowercase(), id.to_lowercase(), candidate.group.to_lowercase());
                if !words.iter().all(|word| hay.contains(word)) {
                    continue;
                }
                let source = match &candidate.pick {
                    SearchPick::Function(FunctionLibraryDrag::Operation(operation)) => (operation.clone(), String::new()),
                    SearchPick::Function(FunctionLibraryDrag::Instance { package, graph }) => (package.clone(), graph.clone()),
                    SearchPick::Definition(_) => continue,
                };
                if seen.insert(source) {
                    all.push((candidate.label.clone(), candidate));
                }
            }
        }
    }
    if let Some(first) = words.first() {
        all.sort_by_key(|(label, _)| !label.to_lowercase().starts_with(first));
    }
    all.into_iter().map(|(_, candidate)| candidate).collect()
}

/// Reports the current search state to the inspection interface.
pub(crate) fn inspect(app: &MaterialCraftApp, ctx: &egui::Context) -> Option<Value> {
    let search = app.graph.search.as_ref()?;
    let function = search.editor == EditorTarget::Function;
    let rows = if function {
        app.function_editor.as_ref().map(|editor| candidates(app, Some(editor), &search.text)).unwrap_or_default()
    } else {
        candidates(app, None, &search.text)
    };
    let id = egui::Id::new("materialcraft-node-search-list");
    Some(json!({
        "editor": if function { "function" } else { "material" },
        "at": [search.at.x, search.at.y],
        "text": search.text,
        "selected": craft_ui::contract::lead_position(ctx, id),
        "rows": rows.into_iter().map(|row| row.label).collect::<Vec<_>>()
    }))
}

/// Shows the shared node search for one editor, at its graph point through that editor's view.
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, mut editor: SearchEditor<'_>) {
    let (target, view) = match &editor {
        SearchEditor::Material => (EditorTarget::Material, app.ui.graph_camera.view(app.graph.view)),
        SearchEditor::Function(editor) => (EditorTarget::Function, editor.view),
    };
    let Some(search) = app.graph.search.as_ref().filter(|search| search.editor == target) else { return };
    let at = view.to_screen(Point::new(f64::from(search.at.x), f64::from(search.at.y)));
    let mut open = true;
    let mut text = search.text.clone();
    let mut picked = None;
    let mut hasher = DefaultHasher::new();
    text.trim().hash(&mut hasher);
    app.session.library_generation().hash(&mut hasher);
    let stamp = hasher.finish();
    let function = match &editor {
        SearchEditor::Material => None,
        SearchEditor::Function(editor) => Some(&**editor),
    };
    let result = Popover::new("Node search", Bind::bound(&mut open)).at_point(at).initial_focus(Initial::FirstField).show(ui, |ui, _| {
        TextField::search("Search nodes...", Bind::bound(&mut text)).drives(egui::Id::new("materialcraft-node-search-list")).show(ui);
        let rows = candidates(app, function, &text);
        let list = List::new("Search results")
            .id(egui::Id::new("materialcraft-node-search-list"))
            .select(craft_ui::contract::SelectMode::Single)
            .activate(ActivateOn::Click)
            .lead_on_open(LeadOnOpen::First)
            .empty("No matching node")
            .max_rows(12)
            .rows_stamp(stamp);
        let mut item =
            |index: &usize, _cx: &mut craft_ui::contract::ItemCx<'_, '_, usize>| ListRow::new(rows[*index].label.clone()).trailing(rows[*index].group.clone());
        let response = list.show_items(ui, &SearchRows(&rows), &mut item);
        for event in response.events {
            if let CollectionEvent::Activate(index) = event {
                picked = rows.get(index).map(|candidate| candidate.pick.clone());
            }
        }
    });
    if let Some(search) = app.graph.search.as_mut() {
        search.text = text;
    }
    if let Some(pick) = picked {
        if let Some(search) = app.graph.search.take() {
            pick_candidate(app, &mut editor, &search, pick);
        }
    } else if !open || result.output.closed.is_some() {
        app.graph.search = None;
    }
}

struct SearchRows<'a>(&'a [SearchCandidate]);

impl craft_behavior::DisplayOrder<usize> for SearchRows<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn key(&self, at: usize) -> usize {
        at
    }

    fn index_of(&self, key: &usize) -> Option<usize> {
        (*key < self.0.len()).then_some(*key)
    }
}

impl craft_behavior::ItemSource<usize> for SearchRows<'_> {}

fn pick_candidate(app: &mut MaterialCraftApp, editor: &mut SearchEditor<'_>, search: &NodeSearch, pick: SearchPick) {
    match (editor, pick) {
        (SearchEditor::Material, SearchPick::Definition(def)) => {
            let mut params = json!({"def": def, "at": [search.at.x, search.at.y - 20.0]});
            if let Some(SearchWire::From((node, port))) = &search.wire {
                params["connectFrom"] = json!({"node": node.0, "port": port});
            }
            if let Some(id) = app.run("graph.node.add", params).and_then(|result| result.get("id").and_then(Value::as_u64))
                && let Some(SearchWire::To((to, port))) = &search.wire
                && let Some(output) = app.session.output_ports(NodeId(id)).first().cloned()
            {
                app.run("graph.connect", json!({"from": id, "fromPort": output, "to": to.0, "toPort": port}));
            }
        }
        (SearchEditor::Function(editor), SearchPick::Function(entry)) => {
            let before = editor.selection.clone();
            let at = [search.at.x - crate::graph::NODE_W * 0.5, search.at.y - 20.0];
            crate::function::library::insert(app, editor, &entry, at);
            let Some(&id) = editor.selection.first().filter(|_| editor.selection != before && editor.selection.len() == 1) else { return };
            match &search.wire {
                Some(SearchWire::From((from, _))) => {
                    let inputs = editor.current(app).and_then(|graph| graph.node(id)).map_or(0, |node| node.inputs.len());
                    if inputs > 0
                        && let Ok(from) = u32::try_from(from.0)
                    {
                        editor.edit(app, FunctionEdit::Connect { from, to: id, input: 0 }, false);
                    }
                }
                Some(SearchWire::To((to, input))) => {
                    if let (Ok(from), Ok(to), Ok(input)) = (u32::try_from(id), u32::try_from(to.0), input.parse()) {
                        editor.edit(app, FunctionEdit::Connect { from, to, input }, false);
                    }
                }
                None => {}
            }
            editor.selection = vec![id];
            editor.inspected_node = Some(id);
        }
        _ => {}
    }
}
