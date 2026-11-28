use crate::{i18n::tr, theme::Tokens};
use egui::{Color32, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use materialcraft_engine::core::{Gradient, GradientKey};

#[derive(Clone, Copy, Debug, PartialEq)]
struct IdentifiedKey {
    id: u64,
    pos: f32,
    color: [f32; 4],
    midpoint: f32,
}

/// A gradient editor: the ramp with its keys below; drag keys, click the ramp to add one, pick a key's color, right-click a key to remove it
pub fn gradient_editor(ui: &mut Ui, id: &str, gradient: &Gradient) -> Option<Gradient> {
    let tokens = Tokens::get(ui.ctx());
    let width = ui.available_width().min(260.0);
    let pad = 8.0;
    let (rect, ramp_resp) = ui.allocate_exact_size(vec2(width, 34.0), Sense::click());
    crate::widgets::name(&ramp_resp, egui::Role::Canvas, "Gradient editor");

    let bar = Rect::from_min_max(pos2(rect.left() + pad, rect.top()), pos2(rect.right() - pad, rect.top() + 18.0));
    let axis = |pos: f32| bar.left() + pos.clamp(0.0, 1.0) * bar.width();
    let unaxis = |x: f32| ((x - bar.left()) / bar.width().max(1.0)).clamp(0.0, 1.0);

    let painter = ui.painter_at(rect.expand(2.0));
    let n = 64;
    for i in 0..n {
        let t0 = i as f32 / n as f32;
        let c = gradient.eval(t0 + 0.5 / n as f32);
        let q = |v: f32| (materialcraft_engine::image::linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0) as u8;
        let r = Rect::from_min_max(pos2(bar.left() + bar.width() * t0, bar.top()), pos2(bar.left() + bar.width() * (t0 + 1.0 / n as f32) + 0.5, bar.bottom()));
        painter.rect_filled(r, 0.0, Color32::from_rgb(q(c[0]), q(c[1]), q(c[2])));
    }
    painter.rect_stroke(bar, 2.0, Stroke::new(1.0, tokens.field_border), egui::StrokeKind::Inside);

    // Sync identified keys from persistent storage so egui tracks each key by a stable ID
    let keys_storage_id = ui.make_persistent_id(id).with("keys_v2");
    let next_id_storage = ui.make_persistent_id(id).with("next_id");
    let mut next_id: u64 = ui.data(|d| d.get_temp(next_id_storage)).unwrap_or(1);
    let mut stored_keys: Vec<IdentifiedKey> = ui.data(|d| d.get_temp(keys_storage_id)).unwrap_or_default();

    // Reconcile stored_keys with gradient.keys
    if stored_keys.len() != gradient.keys.len() {
        stored_keys = gradient
            .keys
            .iter()
            .map(|k| {
                let kid = next_id;
                next_id += 1;
                IdentifiedKey { id: kid, pos: k.pos, color: k.color, midpoint: k.midpoint }
            })
            .collect();
    } else {
        for (sk, gk) in stored_keys.iter_mut().zip(&gradient.keys) {
            sk.pos = gk.pos;
            sk.color = gk.color;
            sk.midpoint = gk.midpoint;
        }
    }

    let selection_id = ui.make_persistent_id(id).with("sel_v2");
    let mut selected_id: u64 = ui.data(|d| d.get_temp(selection_id)).unwrap_or_else(|| stored_keys.first().map(|k| k.id).unwrap_or(0));
    if !stored_keys.iter().any(|k| k.id == selected_id) {
        selected_id = stored_keys.first().map(|k| k.id).unwrap_or(0);
    }

    let mut edited_gradient: Option<Gradient> = None;
    let mut any_handle_interacted = false;
    let handle_top = bar.bottom() + 2.0;

    let mut key_to_remove: Option<u64> = None;
    let current_keys = stored_keys.clone();
    for k in &current_keys {
        let x = axis(k.pos);
        let hit = Rect::from_min_size(pos2(x - 7.0, handle_top - 1.0), vec2(14.0, 14.0));
        let resp = ui.interact(hit, ui.make_persistent_id(id).with(("handle", k.id)), Sense::click_and_drag());
        crate::widgets::name(&resp, egui::Role::Slider, "Gradient key handle");

        if resp.hovered() || resp.dragged() || resp.clicked() {
            any_handle_interacted = true;
        }

        if resp.clicked() {
            selected_id = k.id;
        }

        if resp.secondary_clicked() && stored_keys.len() > 1 {
            key_to_remove = Some(k.id);
        }

        if resp.dragged() {
            selected_id = k.id;
            if let Some(ptr) = resp.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.interact_pos())) {
                let new_pos = unaxis(ptr.x);
                let mut updated_keys = stored_keys.clone();
                if let Some(target) = updated_keys.iter_mut().find(|target| target.id == k.id) {
                    target.pos = new_pos;
                }
                updated_keys.sort_by(|a, b| a.pos.total_cmp(&b.pos));
                stored_keys = updated_keys;
                let mut g =
                    Gradient { keys: stored_keys.iter().map(|item| GradientKey { pos: item.pos, color: item.color, midpoint: item.midpoint }).collect() };
                g.sort();
                edited_gradient = Some(g);
            }
        }

        let is_selected = k.id == selected_id;
        let is_active = is_selected || resp.dragged() || resp.hovered();
        let pts = vec![pos2(x, handle_top), pos2(x - 5.0, handle_top + 9.0), pos2(x + 5.0, handle_top + 9.0)];
        painter.add(egui::Shape::convex_polygon(
            pts,
            if is_active { tokens.accent } else { tokens.text },
            Stroke::new(1.0, if is_selected { tokens.text_strong } else { Color32::TRANSPARENT }),
        ));
    }

    if let Some(remove_id) = key_to_remove {
        stored_keys.retain(|k| k.id != remove_id);
        selected_id = stored_keys.first().map(|k| k.id).unwrap_or(0);
        let mut g = Gradient { keys: stored_keys.iter().map(|item| GradientKey { pos: item.pos, color: item.color, midpoint: item.midpoint }).collect() };
        g.sort();
        edited_gradient = Some(g);
    }

    if ramp_resp.clicked()
        && !any_handle_interacted
        && let Some(ptr) = ramp_resp.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.interact_pos()))
    {
        let t = unaxis(ptr.x);
        let kid = next_id;
        next_id += 1;
        let new_key = IdentifiedKey { id: kid, pos: t, color: gradient.eval(t), midpoint: 0.5 };
        stored_keys.push(new_key);
        stored_keys.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        selected_id = kid;
        let mut g = Gradient { keys: stored_keys.iter().map(|item| GradientKey { pos: item.pos, color: item.color, midpoint: item.midpoint }).collect() };
        g.sort();
        edited_gradient = Some(g);
    }

    ui.data_mut(|d| {
        d.insert_temp(keys_storage_id, stored_keys.clone());
        d.insert_temp(next_id_storage, next_id);
        d.insert_temp(selection_id, selected_id);
    });

    if let Some(selected_key) = stored_keys.iter().find(|k| k.id == selected_id) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} {:.2}", tr("Key at"), selected_key.pos)).size(11.0).color(tokens.text_dim));
            let mut rgb = [selected_key.color[0], selected_key.color[1], selected_key.color[2]];
            if crate::widgets::color_swatch(ui, &format!("{id}-key"), &mut rgb, 60.0) {
                let mut updated_gradient = edited_gradient.clone().unwrap_or_else(|| gradient.clone());
                if let Some(k) = updated_gradient.keys.iter_mut().find(|k| (k.pos - selected_key.pos).abs() < 1e-4) {
                    k.color = [rgb[0], rgb[1], rgb[2], k.color[3]];
                }
                edited_gradient = Some(updated_gradient);
            }
        });
    }

    edited_gradient
}
