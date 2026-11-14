//! The Histogram panel: the plot with its selectable range, and what the image and the selection hold

use egui::{Color32, Painter, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use egui_phosphor::regular as icon;
use materialcraft_engine::HistogramChannel;
use serde_json::{Value, json};

use super::histogram::{BINS, Histogram, level};
use super::shown::Shown;
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// Width of the column with the channel choice and the counts
const STATISTICS_WIDTH: f32 = 164.0;
const MIN_PLOT_WIDTH: f32 = 48.0;
/// Distance between the diagonal lines over the unused part of the range
const HATCH_SPACING: f32 = 6.0;
const TEXT_SIZE: f32 = 10.5;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, shown: &Shown) {
    let mut close = false;
    crate::widgets::panel_header(ui, tr("Histogram"), |ui| {
        close = crate::widgets::icon_button(ui, icon::X, tr("Close the Histogram panel"), false, 18.0).clicked();
    });

    let channel = if shown.image.is_gray() { HistogramChannel::Luminance } else { app.session.view2d.histogram_channel };
    let histogram = counted(app, shown, channel);
    let height = ui.available_height().max(1.0);

    ui.horizontal_top(|ui| {
        let width = (ui.available_width() - STATISTICS_WIDTH - ui.spacing().item_spacing.x).max(MIN_PLOT_WIDTH);
        plot(app, ui, &histogram, vec2(width, height));
        egui::ScrollArea::vertical().id_salt("view2d-histogram-statistics-scroll").max_height(height).show(ui, |ui| {
            ui.vertical(|ui| statistics(app, ui, shown, &histogram, channel));
        });
    });

    if close {
        app.run("view.2d", json!({"histogram": false}));
    }
}

/// The histogram of the shown image, counted once per image and channel
fn counted(app: &mut MaterialCraftApp, shown: &Shown, channel: HistogramChannel) -> Histogram {
    let key = shown.key ^ (channel as u64 + 1).wrapping_mul(0x5851_f42d_4c95_7f2d);
    match &app.view2d.histogram {
        Some((cached, histogram)) if *cached == key => histogram.clone(),
        _ => {
            let histogram = Histogram::of(&shown.image, channel);
            app.view2d.histogram = Some((key, histogram.clone()));
            histogram
        }
    }
}

/// The columns, the hatched unused range and the selection; a drag selects a range and a click clears it
fn plot(app: &mut MaterialCraftApp, ui: &mut Ui, histogram: &Histogram, size: egui::Vec2) {
    let tokens = Tokens::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    crate::widgets::name(&response, egui::Role::Image, "Histogram plot");
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, tokens.field);

    let fraction_at = |x: f32| ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
    if response.drag_started() {
        app.view2d.histogram_drag = ui.input(|input| input.pointer.press_origin()).map(|origin| fraction_at(origin.x));
    }
    if response.dragged()
        && let (Some(start), Some(pointer)) = (app.view2d.histogram_drag, response.interact_pointer_pos())
    {
        app.run("view.2d", json!({"histogramSelection": [start, fraction_at(pointer.x)]}));
    }
    if response.drag_stopped() {
        app.view2d.histogram_drag = None;
    }
    if response.clicked() {
        app.run("view.2d", json!({"histogramSelection": Value::Null}));
    }

    let x_of = |fraction: f32| rect.left() + fraction * rect.width();
    hatch(&painter, Rect::from_min_max(rect.min, pos2(x_of(histogram.used[0]), rect.bottom())), tokens.button_border);
    hatch(&painter, Rect::from_min_max(pos2(x_of(histogram.used[1]), rect.top()), rect.max), tokens.button_border);

    let colors: &[Color32] = if histogram.series.len() == 1 {
        &[tokens.text_dim]
    } else {
        &[
            Color32::from_rgba_unmultiplied(235, 80, 80, 150),
            Color32::from_rgba_unmultiplied(80, 210, 90, 150),
            Color32::from_rgba_unmultiplied(90, 130, 255, 150),
        ]
    };
    let peak = histogram.peak() as f32;
    let column_width = rect.width() / BINS as f32;
    for (bins, color) in histogram.series.iter().zip(colors) {
        for (column, count) in bins.iter().enumerate() {
            let share = *count as f32 / peak;
            let height = rect.height() * if app.session.view2d.histogram_sqrt { share.sqrt() } else { share };
            let left = rect.left() + column as f32 * column_width;
            painter.rect_filled(Rect::from_min_max(pos2(left, rect.bottom() - height), pos2(left + column_width, rect.bottom())), 0.0, *color);
        }
    }

    if let Some([low, high]) = app.session.view2d.histogram_selection {
        let selection = Rect::from_min_max(pos2(x_of(low), rect.top()), pos2(x_of(high), rect.bottom()));
        painter.rect_filled(selection, 0.0, tokens.accent.gamma_multiply(0.18));
        for x in [selection.left(), selection.right()] {
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, tokens.accent));
        }
    }
}

/// Diagonal lines over a part of the range no pixel uses
fn hatch(painter: &Painter, area: Rect, color: Color32) {
    if area.width() < 1.0 {
        return;
    }

    let painter = painter.with_clip_rect(area.intersect(painter.clip_rect()));
    let mut start = area.left() - area.height();
    while start < area.right() {
        painter.line_segment([pos2(start, area.bottom()), pos2(start + area.height(), area.top())], Stroke::new(1.0, color));
        start += HATCH_SPACING;
    }
}

/// The channel choice, what the image and the selection hold, the bit depth and the scale
fn statistics(app: &mut MaterialCraftApp, ui: &mut Ui, shown: &Shown, histogram: &Histogram, channel: HistogramChannel) {
    let tokens = Tokens::get(ui.ctx());
    ui.spacing_mut().item_spacing.y = 2.0;

    let channels: &[HistogramChannel] = if shown.image.is_gray() { &[HistogramChannel::Luminance] } else { &HistogramChannel::ALL };
    let combo = egui::ComboBox::from_id_salt("view2d-histogram-channel").width(STATISTICS_WIDTH - 12.0).selected_text(tr(channel.label())).show_ui(ui, |ui| {
        for choice in channels {
            if ui.selectable_label(*choice == channel, tr(choice.label())).clicked() {
                app.run("view.2d", json!({"histogramChannel": choice.id()}));
            }
        }
    });
    crate::widgets::name(&combo.response, egui::Role::ComboBox, "Histogram channel");

    let bits = if app.session.view2d.histogram_bits == 0 { shown.histogram_bits() } else { app.session.view2d.histogram_bits };
    let range = |low: f32, high: f32| format!("{}; {}", level(low, bits), level(high, bits));
    let selection = app.session.view2d.histogram_selection;
    let selected = selection.map(|range| histogram.selected(range));
    let share = selected.map_or(0.0, |selected| selected.pixels as f64 * 100.0 / histogram.pixels.max(1) as f64);
    let lines = [
        (tr("Selected pixels"), format!("{} ({share:.2}%)", selected.map_or(0, |selected| selected.pixels))),
        (tr("Selected range"), selection.map_or_else(|| "-".to_string(), |[low, high]| range(low, high))),
        (tr("Selected max"), selected.map_or(0, |selected| selected.largest).to_string()),
        (tr("Pixels"), histogram.pixels.to_string()),
        (tr("Range"), range(0.0, 1.0)),
        (tr("Used range"), range(histogram.used[0], histogram.used[1])),
    ];
    egui::Grid::new("view2d-histogram-statistics").num_columns(2).spacing([8.0, 1.0]).min_row_height(0.0).show(ui, |ui| {
        for (label, value) in lines {
            ui.label(RichText::new(format!("{label}:")).size(TEXT_SIZE).color(tokens.text_dim));
            ui.label(RichText::new(value).size(TEXT_SIZE).color(tokens.text));
            ui.end_row();
        }
    });

    ui.horizontal(|ui| {
        let depth = egui::ComboBox::from_id_salt("view2d-histogram-bits").width(64.0).selected_text(bits_label(bits)).show_ui(ui, |ui| {
            for choice in [8, 16, 32] {
                if ui.selectable_label(choice == bits, bits_label(choice)).clicked() {
                    app.run("view.2d", json!({"histogramBits": choice}));
                }
            }
        });
        crate::widgets::name(&depth.response, egui::Role::ComboBox, "Histogram bit depth");

        let sqrt = app.session.view2d.histogram_sqrt;
        if crate::widgets::chip(ui, tr("Sqrt"), sqrt).on_hover_text(tr("Plot the square root of the counts, so small counts stay visible")).clicked() {
            app.run("view.2d", json!({"histogramSqrt": !sqrt}));
        }
    });
}

fn bits_label(bits: u32) -> String {
    format!("{bits} {}", tr("bits"))
}
