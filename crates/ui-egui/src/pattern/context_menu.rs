//! Command rows for the FX-Map canvas menu.

use craft_menu::{CommandRef, Item, Node};
use craft_ui::canvas::{ViewCommands, ViewRequest};
use craft_ui::contract::overlay::Anchor;
use craft_ui::contract::{Bind, CollectionEvent, ShowContainer as _, Trigger};
use craft_ui::graph::{AnnotationEvent, NodeGraphResponse};
use craft_ui::menu::Menu;
use egui::{Pos2, Ui};
use materialcraft_engine::{PatternEdit, SelectionTarget, core::PatternNodeKind};
use serde_json::json;

use super::PatternEditor;
use crate::graph::{add_node_menu, edit_menu, wire_actions};
use crate::{MaterialCraftApp, i18n::tr};

pub(super) fn show(app: &mut MaterialCraftApp, editor: &mut PatternEditor, ui: &mut Ui, graph: &NodeGraphResponse<()>, pointer: Option<Pos2>) {
    let requested = graph
        .collection
        .events
        .iter()
        .find_map(|event| match event {
            CollectionEvent::ContextMenu(request) => Some(request.at),
            CollectionEvent::EmptyContextMenu { at, .. } => Some(*at),
            _ => None,
        })
        .or_else(|| {
            graph.annotations.iter().find_map(|event| match event {
                AnnotationEvent::ContextMenu { at, .. } => Some(*at),
                _ => None,
            })
        });
    let anchor = requested
        .or_else(|| pointer.map(|_| editor.view.to_screen(craft_behavior::kurbo::Point::new(f64::from(editor.menu_at.x), f64::from(editor.menu_at.y)))));
    let rows = rows(app, editor);
    let opener = &graph.collection.response;
    let Some(anchor) = anchor else { return };
    let menu = Menu::context(tr("FX-Map"), opener).opened_by(opener, Trigger::Manual).anchor(Anchor::Point(anchor));
    let _ = menu.open(Bind::bound(&mut editor.menu_open)).show(ui, |ui, _| {
        craft_menu::context::rows(ui, &rows);
    });
}

fn rows(app: &MaterialCraftApp, editor: &PatternEditor) -> Vec<Node> {
    let at = [editor.menu_at.x, editor.menu_at.y];
    let item = |command: &str, params: serde_json::Value, label: &str| Node::Item(Item::new(CommandRef::with_params(command, params), tr(label)));
    let kinds = PatternNodeKind::ALL.into_iter().filter(|kind| *kind != PatternNodeKind::Dot).map(|kind| (kind.native_id(), kind.label(), "FX-Map"));
    let mut rows = vec![Node::submenu(tr("Add node"), add_node_menu::rows("ui.pattern.add", kinds, |kind| json!({"kind": kind, "at": at})))];
    for (label, entry) in [("Add Comment", "item:comment"), ("Add Frame", "item:frame"), ("Add Navigation pin", "item:pin")] {
        rows.push(item("ui.pattern.add", json!({"entry": entry, "at": at}), label));
    }
    rows.push(item("ui.pattern.add", json!({"kind": PatternNodeKind::Dot.native_id(), "at": at}), "Add Dot node"));
    if let Some(id) = editor.selection.last().copied() {
        rows.push(item("ui.pattern.setRoot", json!({"id": id}), "Set as root"));
    }
    rows.push(Node::Separator);
    rows.push(edit_menu::row("file.save", "Save package", json!({})));
    rows.extend(edit_menu::selection_rows(editor.selection_parameters("edit.copy")));
    if let Some(row) = wire_actions::swap_row(app, Some(SelectionTarget::Pattern(editor.node))) {
        rows.push(row);
    }
    rows.push(Node::Separator);
    rows.push(item("ui.graph.frameAll", json!({}), "Frame all"));
    rows
}
