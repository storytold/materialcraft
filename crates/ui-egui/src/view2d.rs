//! The 2D view: one node output at full preview resolution. A toolbar above the image (background
//! image, save, copy, output choice, UV overlay and the two panels), the image with pan and zoom,
//! the Information and Histogram panels under it, and a toolbar below (channels, backdrop and
//! color handling on the left; tiling, sizing and zoom on the right).

mod bottom_toolbar;
mod display;
mod histogram;
mod histogram_panel;
mod histogram_selection;
mod information_panel;
mod output_choice;
mod pixel_readout;
mod shown;
mod top_toolbar;
mod uv_overlay;
mod view2d_state;
mod viewport;

use egui::{Rect, Ui, pos2, vec2};

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;
use shown::Shown;
pub use view2d_state::View2dState;

/// Height of the row with the Information and Histogram panels, when the view is tall enough
const PANELS_HEIGHT: f32 = 184.0;
/// The most of the view's height the panels take, so a short view keeps showing the image
const PANELS_SHARE: f32 = 0.5;
/// Height of the toolbar below the image: its buttons and the margin around them
const BOTTOM_TOOLBAR_HEIGHT: f32 = 26.0;
const MIN_IMAGE_HEIGHT: f32 = 40.0;
/// The share of the panel row the Information panel takes beside the Histogram panel
const INFORMATION_SHARE: f32 = 0.55;
/// Space between a panel's edge and its content
const PANEL_MARGIN: egui::Vec2 = vec2(6.0, 2.0);

pub fn title(app: &MaterialCraftApp) -> String {
    let name = app.session.view2d_image().and_then(|(node, _)| shown_name(app, node));
    format!("{} - {}", name.unwrap_or_default(), tr("2D View"))
}

/// The name of what is shown: the port of a node with several outputs, else the node
fn shown_name(app: &MaterialCraftApp, node: materialcraft_engine::core::NodeId) -> Option<String> {
    let ports = app.session.output_ports(node);
    if ports.len() > 1 {
        let port = if app.session.view2d.node == Some(node) { app.session.view2d.port } else { 0 };
        if let Some(name) = ports.get(port) {
            return Some(name.clone());
        }
    }

    app.session.graph().ok()?.node(node).ok().map(crate::graph::node_label)
}

pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let shown = Shown::current(app);
    top_toolbar::show(app, ui, shown.as_ref());

    // The parts under the toolbar get exact rectangles, so the lower toolbar always ends at the view's edge
    let below = ui.available_rect_before_wrap();
    let view = &app.session.view2d;
    let (information, histogram) = (shown.is_some() && view.information, shown.is_some() && view.histogram);
    let toolbar_top = (below.bottom() - BOTTOM_TOOLBAR_HEIGHT).max(below.top());
    let space = toolbar_top - below.top();
    let panels_height = if information || histogram { PANELS_HEIGHT.min(space * PANELS_SHARE).min(space - MIN_IMAGE_HEIGHT).max(0.0) } else { 0.0 };
    let panels_top = toolbar_top - panels_height;

    let image_area = Rect::from_min_max(below.min, pos2(below.right(), panels_top));
    let mut image_ui = ui.new_child(egui::UiBuilder::new().max_rect(image_area));
    viewport::show(app, &mut image_ui, shown.as_ref(), image_area.height());

    if let Some(shown) = &shown
        && panels_height > 0.0
    {
        panels(app, ui, shown, Rect::from_min_max(pos2(below.left(), panels_top), pos2(below.right(), toolbar_top)), information, histogram);
    }

    let mut toolbar_ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(below.left(), toolbar_top), below.max)));
    bottom_toolbar::show(app, &mut toolbar_ui);
    ui.advance_cursor_after_rect(below);
}

/// The Information and Histogram panels side by side in `row`; one alone takes the whole row
fn panels(app: &mut MaterialCraftApp, ui: &mut Ui, shown: &Shown, row: Rect, information: bool, histogram: bool) {
    let tokens = Tokens::get(ui.ctx());
    ui.painter().rect_filled(row, 0.0, tokens.panel);

    let divider = if information && histogram {
        row.left() + row.width() * INFORMATION_SHARE
    } else if information {
        row.right()
    } else {
        row.left()
    };
    let mut panel = |app: &mut MaterialCraftApp, area: Rect, body: fn(&mut MaterialCraftApp, &mut Ui, &Shown)| {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink2(PANEL_MARGIN)));
        child.set_clip_rect(area.intersect(ui.clip_rect()));
        body(app, &mut child, shown);
    };
    if information {
        panel(app, Rect::from_min_max(row.min, pos2(divider, row.bottom())), information_panel::show);
    }
    if histogram {
        panel(app, Rect::from_min_max(pos2(divider, row.top()), row.max), histogram_panel::show);
    }
    if information && histogram {
        ui.painter().line_segment([pos2(divider, row.top()), pos2(divider, row.bottom())], egui::Stroke::new(1.0, tokens.separator));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_image(app: &mut MaterialCraftApp) {
    let Some((node, image)) = app.session.view2d_image().map(|(node, image)| (node, image.clone())) else { return };

    let name = shown_name(app, node).unwrap_or_else(|| "image".into());
    let dialog = craft_dialogs::FileDialog::save().file_name(format!("{name}.png")).filter("PNG", &["png"]).filter("OpenEXR", &["exr"]).filter("TGA", &["tga"]);
    crate::native_dialogs::ask(app, dialog, crate::native_dialogs::Then::SaveImage(Box::new(image)));
}
