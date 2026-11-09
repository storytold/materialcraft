use super::{GraphCamera, NodeSearch};
use crate::{MaterialCraftApp, i18n::tr, state::MAX_RECENT_NODES, theme::Tokens};
use egui::{Pos2, Ui};
use egui_phosphor::regular as icon;
use materialcraft_engine::core::NodeId;
use serde_json::json;

pub(super) fn open_search(app: &mut MaterialCraftApp, ctx: &egui::Context, at: Pos2, connect: Option<(NodeId, String)>) {
    if crate::shell::modality::Modality::capture(app, ctx).is_blocked() {
        return;
    }
    app.graph.search = Some(NodeSearch { at, text: String::new(), sel: 0, connect, focus: true });
}

/// Catalog entries matching `text` (label or id contains every word)
pub(super) fn matches(text: &str) -> Vec<&'static materialcraft_engine::nodes::NodeDefinition> {
    let words: Vec<String> = text.split_whitespace().map(str::to_ascii_lowercase).collect();
    let mut v: Vec<_> = materialcraft_engine::nodes::catalog()
        .iter()
        .filter(|d| d.id != "graph_instance")
        .filter(|d| {
            let hay = format!("{} {} {}", d.label.to_ascii_lowercase(), d.id, d.category.to_ascii_lowercase());
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect();
    if let Some(w) = words.first() {
        v.sort_by_key(|d| !d.label.to_ascii_lowercase().starts_with(w.as_str()));
    }
    v
}

/// With nothing typed, lists the recently picked nodes first, newest first, then the rest in catalog order
/// A typed query keeps its own ranking, and ids no longer in the catalog are skipped
pub(super) fn with_recent_first(
    list: Vec<&'static materialcraft_engine::nodes::NodeDefinition>,
    query: &str,
    recent: &[String],
) -> Vec<&'static materialcraft_engine::nodes::NodeDefinition> {
    if !query.trim().is_empty() {
        return list;
    }
    let mut out: Vec<&'static materialcraft_engine::nodes::NodeDefinition> = Vec::new();
    for def in recent.iter().take(MAX_RECENT_NODES).filter_map(|id| list.iter().copied().find(|d| d.id == id.as_str())) {
        if !out.iter().any(|d| d.id == def.id) {
            out.push(def);
        }
    }
    let rest: Vec<_> = list.into_iter().filter(|d| !out.iter().any(|r| r.id == d.id)).collect();
    out.extend(rest);
    out
}

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, cam: &GraphCamera) {
    let blocked = crate::shell::modality::Modality::capture(app, ui.ctx()).is_blocked();
    let Some(s) = app.graph.search.as_mut() else { return };

    let tk = Tokens::get(ui.ctx());
    let screen = cam.to_screen(s.at);
    let mut close = false;
    let mut pick: Option<&'static str> = None;
    egui::Area::new(egui::Id::unique("node-search")).fixed_pos(screen).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(260.0);
            if blocked {
                ui.disable();
                s.focus = true;
            }
            let mut reveal_selection = s.focus;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(icon::MAGNIFYING_GLASS).color(tk.text_dim));
                let r = ui.add(egui::TextEdit::singleline(&mut s.text).hint_text(tr("Search nodes...")).desired_width(220.0));
                crate::widgets::name(&r, egui::Role::SearchInput, "Search nodes");
                if s.focus && !blocked {
                    r.request_focus();
                    s.focus = false;
                }
                if r.changed() {
                    s.sel = 0;
                    reveal_selection = true;
                }
            });
            let list = with_recent_first(matches(&s.text), &s.text, &app.ui.recent_nodes);
            let (up, down, enter, esc) = ui.input(|i| {
                let pressed = |key| !blocked && i.key_pressed(key);
                (pressed(egui::Key::ArrowUp), pressed(egui::Key::ArrowDown), pressed(egui::Key::Enter), pressed(egui::Key::Escape))
            });
            if down {
                s.sel = (s.sel + 1).min(list.len().saturating_sub(1));
            }
            if up {
                s.sel = s.sel.saturating_sub(1);
            }
            if esc {
                close = true;
            }
            if enter && let Some(d) = list.get(s.sel) {
                pick = Some(d.id);
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (i, d) in list.iter().enumerate() {
                    let r = ui.add(
                        egui::Button::selectable(i == s.sel, format!("{}   ", d.label))
                            .min_size(egui::vec2(ui.available_width(), 0.0))
                            .right_text(egui::RichText::new(d.group()).size(10.0).color(tk.text_dim)),
                    );
                    if i == s.sel && (up || down || reveal_selection) {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        pick = Some(d.id);
                    }
                }
                if list.is_empty() {
                    ui.label(egui::RichText::new(tr("No matching node")).color(tk.text_dim));
                }
            });
        });
    });
    let clicked_out = !blocked
        && ui.input(|i| i.pointer.any_pressed())
        && ui.ctx().pointer_interact_pos().is_some_and(|p| !ui.ctx().layer_id_at(p).is_some_and(|l| l.id == egui::Id::unique("node-search")));
    if let Some(def) = pick.filter(|_| !crate::shell::modality::Modality::capture(app, ui.ctx()).is_blocked()) {
        let s = app.graph.search.take();
        app.ui.add_recent_node(def);
        if let Some(s) = s {
            let mut p = json!({"def": def, "at": [s.at.x, s.at.y - 20.0]});
            if let Some((n, port)) = s.connect {
                p["connectFrom"] = json!({"node": n.0, "port": port});
            }
            app.run("graph.node.add", p);
        }
    } else if close || clicked_out {
        app.graph.search = None;
    }
}

#[cfg(test)]
#[path = "search.test.rs"]
mod tests;
