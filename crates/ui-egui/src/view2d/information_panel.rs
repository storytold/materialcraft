//! The Information panel: the pixel under the pointer and the pixel under the color sampler

use egui::{FontId, RichText, Sense, Stroke, Ui, vec2};
use egui_phosphor::regular as icon;
use serde_json::{Value, json};

use super::pixel_readout::PixelReadout;
use super::shown::Shown;
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

const SWATCH_SIDE: f32 = 14.0;
const TEXT_SIZE: f32 = 10.5;
/// Space between the lines of one readout
const LINE_SPACING: f32 = 1.0;
/// Lines one readout takes: three color channels with their hue, saturation and value, then alpha
const READOUT_LINES: f32 = 4.0;
/// Space between the columns of one readout
const COLUMN_SPACING: f32 = 14.0;
const REMOVE_BUTTON_SIDE: f32 = 16.0;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, shown: &Shown) {
    let mut close = false;
    crate::widgets::panel_header(ui, tr("Information"), |ui| {
        close = crate::widgets::icon_button(ui, icon::X, tr("Close the Information panel"), false, 18.0).clicked();
    });

    let mut remove_sampler = false;
    egui::ScrollArea::vertical().id_salt("view2d-information").auto_shrink([false, false]).show(ui, |ui| {
        // Each row has its own id and the pointer's row a fixed height, so the sampler's row
        // stays where it is while the pointer comes and goes
        let line_height = ui.ctx().fonts_mut(|fonts| fonts.row_height(&FontId::monospace(TEXT_SIZE))) + LINE_SPACING;
        ui.push_id("pointer", |ui| {
            ui.set_min_height(line_height * READOUT_LINES);
            match app.view2d.hover {
                Some(position) => {
                    row(ui, &PixelReadout::at(&shown.image, position), shown.levels, false);
                }
                None => {
                    ui.label(RichText::new(tr("Move the pointer over the image to read its pixels")).size(TEXT_SIZE).color(Tokens::get(ui.ctx()).text_dim));
                }
            }
        });

        if let Some(sampler) = app.session.view2d.sampler {
            ui.separator();
            ui.push_id("sampler", |ui| {
                remove_sampler = row(ui, &PixelReadout::at_fraction(&shown.image, sampler), shown.levels, true);
            });
        }
    });

    if remove_sampler {
        app.run("view.2d", json!({"sampler": Value::Null}));
    }
    if close {
        app.run("view.2d", json!({"information": false}));
    }
}

/// One readout: its color, its columns and, for the sampler, a button that removes it. True when that button was clicked
fn row(ui: &mut Ui, readout: &PixelReadout, levels: Option<u32>, sampler: bool) -> bool {
    let tokens = Tokens::get(ui.ctx());
    let font = FontId::monospace(TEXT_SIZE);
    let mut remove = false;

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            let (swatch, response) = ui.allocate_exact_size(vec2(SWATCH_SIDE, SWATCH_SIDE), Sense::hover());
            ui.painter().rect(swatch, 2.0, readout.swatch(), Stroke::new(1.0, tokens.field_border), egui::StrokeKind::Inside);
            response.on_hover_text(if sampler { tr("The pixel under the color sampler") } else { tr("The pixel under the pointer") });

            if sampler {
                ui.label(RichText::new(icon::EYEDROPPER).size(13.0).color(tokens.text_dim));
                remove = crate::widgets::icon_button(ui, icon::X, tr("Remove the color sampler"), false, REMOVE_BUTTON_SIDE).clicked();
            }
        });

        // The columns that do not fit beside each other continue on the next line
        let columns = readout.columns(levels);
        let glyph_width = ui.ctx().fonts_mut(|fonts| fonts.glyph_width(&font, '0'));
        let widths: Vec<f32> = columns.iter().map(|lines| lines.iter().map(String::len).max().unwrap_or(0) as f32 * glyph_width).collect();
        ui.vertical(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.spacing_mut().item_spacing = vec2(COLUMN_SPACING, LINE_SPACING);
            let mut remaining = columns.as_slice();
            for count in packed(&widths, ui.available_width(), COLUMN_SPACING) {
                let (line, rest) = remaining.split_at(count.min(remaining.len()));
                remaining = rest;
                ui.horizontal_top(|ui| {
                    for lines in line {
                        ui.vertical(|ui| {
                            for text in lines {
                                ui.label(RichText::new(text).font(font.clone()).color(tokens.text));
                            }
                        });
                    }
                });
            }
        });
    });

    remove
}

/// How many columns each line holds: as many as fit in `available`, in order, and at least one per line
fn packed(widths: &[f32], available: f32, spacing: f32) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut held = 0;
    let mut used = 0.0;

    for width in widths {
        let wider = if held == 0 { *width } else { used + spacing + width };
        if held > 0 && wider > available {
            lines.push(held);
            held = 1;
            used = *width;
        } else {
            held += 1;
            used = wider;
        }
    }
    if held > 0 {
        lines.push(held);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_that_do_not_fit_continue_on_the_next_line() {
        let widths = [100.0, 80.0, 60.0];

        assert_eq!(packed(&widths, 300.0, 10.0), [3], "all three and their two gaps take 260");
        assert_eq!(packed(&widths, 250.0, 10.0), [2, 1]);
        assert_eq!(packed(&widths, 150.0, 10.0), [1, 2]);
        assert_eq!(packed(&widths, 10.0, 10.0), [1, 1, 1], "a column wider than the panel still gets its line");
        assert!(packed(&[], 100.0, 10.0).is_empty());
    }
}
