//! Compact widgets matching the app's look: labelled slim sliders, icon buttons, section
//! headers, channel toggle chips and color swatches.

use egui::{Color32, CornerRadius, Response, RichText, Sense, Stroke, Ui, vec2};

use crate::theme::Tokens;

/// Give a custom widget an accessible name and role (screen readers, AccessKit, tests).
pub fn name(resp: &Response, role: egui::Role, label: &str) {
    let label = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(role, true, label.clone()));
}

/// The drag-and-drop payload of type `T` released on `resp` this frame. Unlike
/// [`Response::dnd_release_payload`] (which clears a payload of any type), it leaves payloads
/// of other types for the next drop target.
pub fn dnd_release<T: std::any::Any + Send + Sync>(resp: &Response) -> Option<std::sync::Arc<T>> {
    resp.dnd_hover_payload::<T>().and_then(|_| resp.dnd_release_payload::<T>())
}

/// A labelled slider in two rows: "Label ........ value" over a slim track with a round knob.
/// Returns the track response (`.changed()` on a drag or valid numeric edit).
pub fn slider(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, decimals: usize) -> Response {
    slider_with_settings(ui, label, value, range, decimals, None, None)
}

/// Keep a short slider track while allowing the full supported numeric range
pub(crate) fn slider_with_numeric_range(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    track_range: std::ops::RangeInclusive<f32>,
    numeric_range: std::ops::RangeInclusive<f32>,
    decimals: usize,
) -> Response {
    slider_with_settings(ui, label, value, track_range, decimals, None, Some(numeric_range))
}

pub(crate) fn input_slider(ui: &mut Ui, label: &str, value: &mut f32, input: &materialcraft_engine::core::ExposedParameter, decimals: usize) -> Response {
    let range = if input.clamp { input.min..=input.max } else { input.min.min(*value)..=input.max.max(*value) };
    slider_with_settings(ui, label, value, range, decimals, Some(input), None)
}

fn slider_with_settings(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    decimals: usize,
    settings: Option<&materialcraft_engine::core::ExposedParameter>,
    numeric_range: Option<std::ops::RangeInclusive<f32>>,
) -> Response {
    let tokens = Tokens::get(ui.ctx());

    // Imported widget limits may be reversed or non-finite
    let first = if range.start().is_finite() { *range.start() } else { 0.0 };
    let last = if range.end().is_finite() { *range.end() } else { 1.0 };
    let (lo, hi) = (first.min(last), first.max(last));
    let (numeric_lo, numeric_hi) = numeric_range.as_ref().map_or((lo, hi), |range| (*range.start(), *range.end()));
    let constrain = |value: f32, clamp: bool| {
        let value = if let Some(step) = settings.and_then(|input| input.step).filter(|step| step.is_finite() && *step > 0.0) {
            let snapped = ((f64::from(value) / f64::from(step)).round() * f64::from(step)) as f32;
            if snapped.is_finite() { snapped } else { value }
        } else {
            value
        };
        if clamp { value.clamp(numeric_lo, numeric_hi) } else { value }
    };
    let width = ui.available_width();
    let mut edited = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(tokens.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let id = ui.next_auto_id();
            // Keep incomplete input (such as "-") across frames while editing
            let draft = id.with("numeric-draft");
            let editing = ui.memory(|m| m.has_focus(id) || m.had_focus_last_frame(id));
            let mut text = editing.then(|| ui.data(|d| d.get_temp::<String>(draft))).flatten().unwrap_or_else(|| format!("{:.*}", decimals, value));
            let response = ui.add(egui::TextEdit::singleline(&mut text).id(id).desired_width(44.0).align(egui::Align2::RIGHT_CENTER).frame(egui::Frame::NONE));
            name(&response, egui::Role::TextInput, label);
            if (response.changed() || response.lost_focus())
                && !ui.input(|i| i.key_pressed(egui::Key::Escape))
                && let Ok(parsed_value) = text.trim().parse::<f32>()
                && parsed_value.is_finite()
            {
                let parsed_value = constrain(parsed_value, settings.is_none_or(|input| input.clamp));
                edited = parsed_value != *value;
                *value = parsed_value;
            }
            let focused = response.has_focus();
            ui.data_mut(|d| {
                if focused {
                    d.insert_temp(draft, text);
                } else {
                    d.remove::<String>(draft);
                }
            });
        });
    });
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, 14.0), Sense::click_and_drag());
    if edited {
        response.mark_changed();
    }
    let (accessible_value, accessible_label) = (*value as f64, label.to_string());
    response.widget_info(move || egui::WidgetInfo::slider(true, accessible_value, accessible_label.clone()));
    if (response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let value_at = |x: f32| {
            let fraction = ((x - rect.left() - 5.0) / (rect.width() - 10.0).max(1.0)).clamp(0.0, 1.0);
            f64::from(lo) + (f64::from(hi) - f64::from(lo)) * f64::from(fraction)
        };

        // Snap only within half a pixel so intermediate values remain reachable
        let radius = 0.5 / ui.ctx().pixels_per_point();
        let aimed_value = egui::emath::smart_aim::best_in_range_f64(value_at(pointer.x - radius), value_at(pointer.x + radius)) as f32;
        let aimed_value = constrain(aimed_value, true);
        if (aimed_value - *value).abs() > f32::EPSILON {
            *value = aimed_value;
            response.mark_changed();
        }
    }
    let fraction = if hi > lo { ((*value - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
    track(ui, &response, rect.left() + 5.0..=rect.right() - 5.0, rect.center().y, fraction, 2.0, 4.5);
    response
}

/// A small inline slider for the top tool bar: "Label value" above a short track.
pub fn bar_slider(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32) -> Response {
    let tk = Tokens::get(ui.ctx());
    let (lo, hi) = (*range.start(), *range.end());
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click_and_drag());
    let (v, l) = (*value as f64, label.to_string());
    resp.widget_info(move || egui::WidgetInfo::slider(true, v, l.clone()));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let value_at = |x: f32| {
            let t = ((x - rect.left() - 4.0) / (rect.width() - 8.0).max(1.0)).clamp(0.0, 1.0);
            f64::from(lo) + (f64::from(hi) - f64::from(lo)) * f64::from(t)
        };
        // Snap to a round value only within half a physical pixel (in points), so
        // round values are reachable by aiming at them while fractional values
        // between round steps remain reachable.
        let radius = 0.5 / ui.ctx().pixels_per_point();
        let v = egui::emath::smart_aim::best_in_range_f64(value_at(p.x - radius), value_at(p.x + radius)) as f32;
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    let font = egui::FontId::proportional(11.0);
    p.text(rect.left_top() + vec2(2.0, 2.0), egui::Align2::LEFT_TOP, label, font.clone(), tk.text);
    p.text(rect.right_top() + vec2(-2.0, 2.0), egui::Align2::RIGHT_TOP, format!("{:.0}", value), font, tk.text_strong);
    let t = if hi > lo { ((*value - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
    track(ui, &resp, rect.left() + 4.0..=rect.right() - 4.0, rect.bottom() - 7.0, t, 1.5, 4.0);
    resp
}

/// A slider track from `x` at height `y`: the rail, the accent up to fraction `t` and a round knob.
fn track(ui: &Ui, resp: &Response, x: std::ops::RangeInclusive<f32>, y: f32, t: f32, width: f32, knob: f32) {
    let tk = Tokens::get(ui.ctx());
    let p = ui.painter();
    let (x0, x1) = (*x.start(), *x.end());
    let xk = x0 + (x1 - x0) * t;
    p.line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], Stroke::new(width, tk.button_border));
    p.line_segment([egui::pos2(x0, y), egui::pos2(xk, y)], Stroke::new(width, tk.accent));
    let ring = if resp.hovered() || resp.dragged() { tk.text_strong } else { tk.text };
    p.circle(egui::pos2(xk, y), knob, tk.panel, Stroke::new(1.5, ring));
}

/// One line of `text` cut with "…" to fit `max_width`, for list rows that shrink with their panel.
pub fn elided(painter: &egui::Painter, text: impl Into<String>, size: f32, color: Color32, max_width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.into(), egui::FontId::proportional(size), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    painter.layout_job(job)
}

/// A square icon button (Phosphor glyph) in a rounded tool well: `selected` sits in the accent
/// tinted well, a hovered one in the row-hover well and a pressed one in the tool well; keyboard
/// focus draws an inset accent outline. `tooltip` is its accessible name and tooltip. A
/// non-finite, non-positive or oversized (over 4096 points) size gives an inert zero-size response.
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str, selected: bool, size: f32) -> Response {
    let tk = Tokens::get(ui.ctx());
    let valid = size.is_finite() && size > 0.0 && size <= 4096.0;
    let (rect, response) = ui.allocate_exact_size(if valid { vec2(size, size) } else { egui::Vec2::ZERO }, if valid { Sense::click() } else { Sense::hover() });
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, valid && ui.is_enabled(), selected, tooltip));
    if !valid {
        return response;
    }
    let (fill, color) = if selected || response.is_pointer_button_down_on() {
        (tk.tool_active, tk.text_strong)
    } else if response.hovered() {
        (tk.row_hover, tk.text_strong)
    } else {
        (Color32::TRANSPARENT, tk.text)
    };
    let radius = CornerRadius::same(6);
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, radius, fill);
    }
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(size * 0.62), color);
    if response.has_focus() && rect.width() > 4.0 && rect.height() > 4.0 {
        ui.painter().rect_stroke(rect.shrink(2.0), radius, Stroke::new(1.0, tk.accent), egui::StrokeKind::Inside);
    }
    response.on_hover_text(tooltip)
}

/// A flat button filled with the accent, white lettering (a dialog's main action).
pub fn primary_button<'a>(ui: &Ui, text: &str) -> egui::Button<'a> {
    let tk = Tokens::get(ui.ctx());
    egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(tk.accent_strong).stroke(Stroke::NONE)
}

/// A labelled dropdown on one row: the label, then the box filling the rest of the row, so a
/// narrow panel shortens the box (its text truncated) instead of pushing the row past its edge.
pub fn combo(ui: &mut Ui, label: &str, selected: impl Into<egui::WidgetText>, menu: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(label);
        let r = egui::ComboBox::from_id_salt(label)
            .width(ui.available_width().max(60.0))
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(selected)
            .show_ui(ui, menu);
        name(&r.response, egui::Role::ComboBox, label);
    });
}

/// A collapsible section header with a chevron ("> Size").
pub fn section(ui: &mut Ui, id: &str, title: &str, default_open: bool, body: impl FnOnce(&mut Ui)) {
    let tk = Tokens::get(ui.ctx());
    let id = ui.make_persistent_id(id);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
    let open = state.is_open();
    let resp = ui
        .horizontal(|ui| {
            let chev = if open { egui_phosphor::regular::CARET_DOWN } else { egui_phosphor::regular::CARET_RIGHT };
            ui.label(RichText::new(chev).size(10.0).color(tk.text_dim));
            ui.label(RichText::new(title).strong().color(tk.text_strong));
        })
        .response
        .interact(Sense::click());
    if resp.clicked() {
        state.toggle(ui);
    }
    state.show_body_unindented(ui, |ui| {
        ui.indent(id.with("in"), body);
    });
}

/// A channel chip ("color", "metal", "rough"…): toggled = the accent-tinted well.
pub fn chip(ui: &mut Ui, text: &str, on: bool) -> Response {
    let tk = Tokens::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(text.into(), egui::FontId::proportional(11.0), tk.text);
    let size = vec2(galley.size().x + 14.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    name(&resp, egui::Role::Button, text);
    let p = ui.painter();
    let fill = if on {
        tk.tool_active
    } else if resp.hovered() {
        tk.row_hover
    } else {
        tk.row
    };
    p.rect_filled(rect, 5.0, fill);
    p.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(11.0), if on { tk.text_strong } else { tk.text_dim });
    resp
}

/// A color swatch button for a linear RGB value; opens a picker on click. Returns true when
/// the color changed.
pub fn color_swatch(ui: &mut Ui, id: &str, linear: &mut [f32; 3], width: f32) -> bool {
    let tk = Tokens::get(ui.ctx());
    let srgb = linear.map(|v| (materialcraft_engine::image::linear_to_srgb(v) * 255.0 + 0.5) as u8);
    let mut c = Color32::from_rgb(srgb[0], srgb[1], srgb[2]);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 18.0), Sense::click());
    name(&resp, egui::Role::ColorWell, id);
    ui.painter().rect(rect, 4.0, c, Stroke::new(1.0, tk.field_border), egui::StrokeKind::Inside);
    let popup = ui.make_persistent_id(id);
    let mut changed = false;
    egui::Popup::menu(&resp)
        .id(popup)
        .align(egui::RectAlign::TOP_END)
        .align_alternatives(&[egui::RectAlign::BOTTOM_END])
        .gap(8.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(200.0);
            ui.spacing_mut().slider_width = 192.0;
            if crate::color_picker::color_picker_color32(ui, &mut c, crate::color_picker::Alpha::Opaque) {
                let lin = [c.r(), c.g(), c.b()].map(|v| materialcraft_engine::image::srgb_to_linear(v as f32 / 255.0));
                *linear = lin;
                changed = true;
            }
        });
    changed
}

/// Panel title row with optional trailing content.
pub fn panel_header(ui: &mut Ui, title: &str, trailing: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(22.0);
        ui.label(crate::theme::caption(title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing);
    });
}
