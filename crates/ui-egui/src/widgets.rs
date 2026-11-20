//! Compact widgets matching the app's look: labelled slim sliders, icon buttons, section
//! headers, channel toggle chips and color swatches.

use craft_ui::buttons::{ButtonVisuals, IconButton, IconButtonStyle};
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
/// tinted well (the shared Craft icon button, `craft_ui::buttons`).
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str, selected: bool, size: f32) -> Response {
    let tk = Tokens::get(ui.ctx());
    let well = |fill, icon| ButtonVisuals { fill, stroke: Stroke::NONE, icon };
    let style = IconButtonStyle {
        normal: well(Color32::TRANSPARENT, tk.text),
        hovered: well(tk.row_hover, tk.text_strong),
        pressed: Some(well(tk.tool_active, tk.text_strong)),
        selected: well(tk.tool_active, tk.text_strong),
        corner_radius: CornerRadius::same(6),
        focus_stroke: Stroke::new(1.0, tk.accent),
    };
    IconButton::new(tooltip, vec2(size, size), &style)
        .selected(selected)
        .show(ui, |ui, rect, color| {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(size * 0.62), color);
        })
        .on_hover_text(tooltip)
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
    if resp.clicked() {
        egui::Popup::toggle_id(ui.ctx(), popup);
    }
    let mut changed = false;
    // Open state lives in memory (`toggle_id` above); `from_response` alone shows it always.
    egui::Popup::from_response(&resp).id(popup).open_memory(None).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
            let lin = [c.r(), c.g(), c.b()].map(|v| materialcraft_engine::image::srgb_to_linear(v as f32 / 255.0));
            *linear = lin;
            changed = true;
        }

        // An exact colour can be typed instead of picked. The draft stays as typed while the
        // field has focus, so a half-finished code is not rewritten; a valid code applies at once.
        let draft_id = popup.with("hex-draft");
        let mut draft = ui.data(|data| data.get_temp::<String>(draft_id)).unwrap_or_else(|| hex_rgb(linear));
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Hex"));
            let response = ui.add(egui::TextEdit::singleline(&mut draft).char_limit(7).desired_width(78.0));
            name(&response, egui::Role::TextInput, "Hex colour code");
            if response.has_focus() {
                ui.data_mut(|data| data.insert_temp(draft_id, draft.clone()));
            }
            if response.changed()
                && let Some(lin) = parse_hex_rgb(&draft)
            {
                *linear = lin;
                changed = true;
            }
            if response.lost_focus() {
                ui.data_mut(|data| data.remove::<String>(draft_id));
            }
        });
    });
    changed
}

/// `#RRGGBB` for linear RGB, through the sRGB transfer curve (what the field shows).
pub fn hex_rgb(linear: &[f32; 3]) -> String {
    let q = |v: f32| (materialcraft_engine::image::linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
    format!("#{:02X}{:02X}{:02X}", q(linear[0]), q(linear[1]), q(linear[2]))
}

/// Linear RGB from `#RRGGBB` or `RRGGBB` (sRGB, as typed in the field). Hex digits only, so a
/// sign or whitespace that string parsing might take is rejected.
pub fn parse_hex_rgb(text: &str) -> Option<[f32; 3]> {
    let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |index: usize| {
        let byte = u8::from_str_radix(hex.get(index..index + 2)?, 16).ok()?;
        Some(materialcraft_engine::image::srgb_to_linear(f32::from(byte) / 255.0))
    };
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// Panel title row with optional trailing content.
pub fn panel_header(ui: &mut Ui, title: &str, trailing: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(22.0);
        ui.label(crate::theme::caption(title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing);
    });
}

#[cfg(test)]
mod tests {
    use super::{hex_rgb, parse_hex_rgb};

    #[test]
    fn hex_codes_round_trip_through_the_srgb_curve() {
        assert_eq!(hex_rgb(&[1.0, 0.0, 0.0]), "#FF0000");
        assert_eq!(hex_rgb(&[0.0, 1.0, 0.0]), "#00FF00");
        assert_eq!(hex_rgb(&[0.0, 0.0, 1.0]), "#0000FF");
        assert_eq!(hex_rgb(&[0.0, 0.0, 0.0]), "#000000");
        let parsed = parse_hex_rgb("#FF8000").unwrap();
        assert!((parsed[0] - 1.0).abs() < 1e-6 && (parsed[1] - 0.215_86).abs() < 1e-3 && parsed[2] == 0.0, "{parsed:?}");
        // With or without the hash; case does not matter.
        assert_eq!(parse_hex_rgb("ff8000"), parse_hex_rgb("#FF8000"));
        for linear in [[0.1, 0.4, 0.9], [0.0, 0.5, 1.0]] {
            let round = parse_hex_rgb(&hex_rgb(&linear)).unwrap();
            assert!(round.iter().zip(linear).all(|(a, b)| (a - b).abs() < 0.005), "{linear:?} -> {round:?}");
        }
    }

    #[test]
    fn invalid_hex_codes_are_rejected() {
        for text in ["", "#", "FFF", "#FFF", "GGGGGG", "+FFFFF", "-FFFFF", "FFFFF ", " FFFFF", "0x1234", "#1234567", "##123456"] {
            assert_eq!(parse_hex_rgb(text), None, "{text:?}");
        }
    }

    #[test]
    fn typing_a_hex_code_sets_the_colour() {
        use egui_kittest::{
            Harness,
            kittest::{By, Queryable},
        };

        let mut harness = Harness::builder().with_size(egui::vec2(360.0, 500.0)).build_ui_state(
            |ui, linear: &mut [f32; 3]| {
                super::color_swatch(ui, "hex-test", linear, 90.0);
            },
            [0.0; 3],
        );
        harness.run_steps(3);
        harness.get(By::new().role(egui::Role::ColorWell).label("hex-test")).click_accesskit();
        harness.run_steps(3);
        let field = harness.get(By::new().role(egui::Role::TextInput).label("Hex colour code"));
        field.set_text_value_accesskit("#FF8000");
        harness.run_steps(3);

        let linear = *harness.state();
        assert!((linear[0] - 1.0).abs() < 1e-6 && (linear[1] - 0.215_86).abs() < 1e-3 && linear[2] == 0.0, "the field set {linear:?}");
    }
}
