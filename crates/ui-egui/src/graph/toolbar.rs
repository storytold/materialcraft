use crate::MaterialCraftApp;
use craft_behavior::{EditEvent, IconId};
use craft_ui::{
    bars::{Bar, BarItem, BarItems, BarKind, BreadcrumbBar},
    buttons::{IconButton, ToggleButton},
    contract::{Bind, Entry, Props as _, Show as _, ShowContainer as _},
    display::{Shade, Text},
    layout_components::Row,
    pickers::Select,
    runtime::commands::CommandRef,
};
use serde_json::json;

const ALIGN_MODES: [&str; 6] = ["left", "top", "hcenter", "vcenter", "hdistribute", "vdistribute"];

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
    let function = app.function_editor.is_some();
    let pattern = !function && crate::pattern::active(app);
    let selection = selection(app, function, pattern);
    let center = crate::annotation::camera(app).center;
    let actions = [
        ("graph.frame.add", annotation_frame(app, &selection)),
        ("graph.comment.add", json!({})),
        ("graph.pin.add", json!({"at": {"x": center.x as f32, "y": center.y as f32}})),
    ];

    let mut frame_all = false;
    Bar::new(crate::i18n::tr("Graph tools"), BarKind::Options).trailing(|ui, _| trailing(app, ui, function, pattern)).show(ui, |ui, _| {
        let frame = IconButton::new(craft_ui::primitives::IconSpec::svg(IconId::new("frame_all"))).label(crate::i18n::tr("Frame all"));
        frame_all = frame.show(ui).clicked();

        let align_items = ALIGN_MODES.iter().map(|mode| BarItem::new(CommandRef::with_params("graph.align", json!({"mode": mode})))).collect();
        BarItems::new(align_items).show(ui);
        BarItems::new(vec![BarItem::new(CommandRef::new("ui.graph.snapToGrid"))]).show(ui);

        let items = actions.iter().map(|(id, params)| BarItem::new(CommandRef::with_params(*id, params.clone())).section(1)).collect();
        BarItems::new(items).show(ui);
    });
    if frame_all {
        app.ui.frame_all = true;
    }

    if !pattern {
        let groups = if function { function_items(app) } else { atomic_items() };
        Bar::new(crate::i18n::tr("Quick nodes"), BarKind::Options).show(ui, |ui, _| {
            for (section, group) in groups.iter().enumerate() {
                let items =
                    group.iter().map(|def| BarItem::new(CommandRef::with_params("ui.graph.quickAdd", json!({"def": def}))).section(section as u16)).collect();
                BarItems::new(items).show(ui);
            }
        });
    }
}

fn selection(app: &MaterialCraftApp, function: bool, pattern: bool) -> Vec<u64> {
    if pattern {
        app.pattern_editor.as_ref().map(|editor| editor.selection.iter().copied().map(u64::from).collect()).unwrap_or_default()
    } else if function {
        app.function_editor.as_ref().map(|editor| editor.selection.iter().copied().map(u64::from).collect()).unwrap_or_default()
    } else {
        app.session.selection.iter().map(|id| id.0).collect()
    }
}

fn trailing(app: &mut MaterialCraftApp, ui: &mut egui::Ui, function: bool, pattern: bool) {
    Row::new().show(ui, |ui, _| {
        if super::subgraph_navigation::can_return(app) {
            IconButton::command(craft_ui::primitives::IconSpec::svg(IconId::new("return_parent")), "ui.graph.parent")
                .label(crate::i18n::tr("Return to parent graph"))
                .show(ui);
            let crumbs = super::subgraph_navigation::crumbs(app);
            let picked = BreadcrumbBar::new(crate::i18n::tr("Graph path"), &crumbs).show(ui).picked;
            if let Some(steps) = picked {
                super::subgraph_navigation::back(app, steps);
            }
        }
        if pattern {
            if app.pattern_editor.as_ref().and_then(|editor| editor.current(app)).is_some_and(|graph| graph.root.is_none()) {
                Text::new(crate::i18n::tr("Set a node as the root")).show(ui);
            }
        } else {
            ToggleButton::new(crate::i18n::tr("Show node thumbnails"), Bind::bound(&mut app.ui.prefs.thumbnails))
                .icon(craft_ui::primitives::IconSpec::svg(IconId::new("image")))
                .icon_only()
                .show(ui);
            if !function {
                parent_size(app, ui);
            }
        }
    });
}

fn parent_size(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
    let Ok(graph) = app.session.graph() else { return };
    let [width, height] = graph.size;
    let mut new_width = width;
    let mut new_height = height;
    let entries: Vec<_> = (4..=13u32).map(|level| Entry::new(level, (1u32 << level).to_string())).collect();
    Text::new(crate::i18n::tr("Parent size:")).shade(Shade::Muted).show(ui);
    let height_out = Select::new(crate::i18n::tr("Parent height"), Bind::bound(&mut new_height), &entries).show(ui);
    Row::new().show(ui, |ui, _| {
        Text::new("x").show(ui);
        let width_out = Select::new(crate::i18n::tr("Parent width"), Bind::bound(&mut new_width), &entries).show(ui);
        let changed = height_out.events.iter().chain(width_out.events.iter()).any(|event| matches!(event, EditEvent::Set { .. } | EditEvent::Commit { .. }));
        if changed {
            app.run("graph.size", json!({"log2": [new_width, new_height]}));
            app.session.finish_edit();
        }
    });
}

fn atomic_items() -> Vec<Vec<&'static str>> {
    ATOMIC_QUICK.iter().map(|group| group.to_vec()).collect()
}

fn function_items(app: &MaterialCraftApp) -> Vec<Vec<&'static str>> {
    let available = crate::function::library::available_operations(app);
    crate::function::library::QUICK
        .iter()
        .map(|group| group.iter().filter_map(|(operation, _)| available.contains(operation).then_some(*operation)).collect())
        .collect()
}

fn annotation_frame(app: &MaterialCraftApp, selection: &[u64]) -> serde_json::Value {
    let bounds = app.graph.rects.iter().filter(|(id, _)| selection.contains(&id.0)).fold(egui::Rect::NOTHING, |bounds, (_, rect)| bounds.union(*rect));
    if !bounds.is_finite() {
        return json!({});
    }
    let view = crate::annotation::camera(app);
    let start = view.to_doc(bounds.min);
    let size = bounds.size() / view.zoom as f32;
    json!({"rect": [start.x as f32 - 24.0, start.y as f32 - 44.0, size.x + 48.0, size.y + 68.0]})
}

/// The quick atomic nodes, by toolbar section; the command catalog reads it too.
pub(crate) const ATOMIC_QUICK: &[&[&str]] = &[
    &[
        "bitmap",
        "blend",
        "blur",
        "channels_shuffle",
        "curve",
        "directional_blur",
        "directional_warp",
        "distance",
        "emboss",
        "fx_map",
        "gradient_dynamic",
        "gradient_map",
        "grayscale_conversion",
        "hsl",
        "levels",
        "normal",
        "pixel_processor",
        "svg",
        "sharpen",
        "text",
        "transformation",
        "uniform",
        "value_processor",
        "warp",
    ],
    &["input_color", "input_grayscale", "input_value", "output", "dot"],
];
