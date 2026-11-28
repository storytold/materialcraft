use super::*;

fn commands_after_press_at(pos: egui::Pos2) -> Vec<ViewportCommand> {
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut output = Vec::new();
    for events in [
        vec![egui::Event::PointerMoved(pos)],
        vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }],
        vec![egui::Event::PointerMoved(pos + egui::vec2(3.0, 3.0))],
        vec![egui::Event::PointerMoved(pos + egui::vec2(9.0, 9.0))],
    ] {
        let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
        let mut frame = ctx.run_ui(input, resize_zones);
        frame.textures_delta.clear();
        output.extend(frame.viewport_output.remove(&egui::ViewportId::ROOT).map(|root| root.commands).unwrap_or_default());
    }
    output
}

#[test]
fn a_press_on_a_window_edge_starts_a_resize() {
    use egui::viewport::ResizeDirection::{East, North, SouthEast, West};
    for (pos, direction) in
        [(egui::pos2(400.0, 1.0), North), (egui::pos2(1.0, 300.0), West), (egui::pos2(799.0, 300.0), East), (egui::pos2(799.0, 599.0), SouthEast)]
    {
        let commands = commands_after_press_at(pos);
        assert!(commands.contains(&ViewportCommand::BeginResize(direction)), "{pos:?}: {commands:?}");
    }
}

#[test]
fn a_press_inside_the_window_does_not_resize() {
    let commands = commands_after_press_at(egui::pos2(400.0, 300.0));
    assert!(!commands.iter().any(|command| matches!(command, ViewportCommand::BeginResize(_))), "{commands:?}");
}

/// The frame decision of a window whose bar the app draws, with the caption buttons of `layout`.
fn app_frame(layout: craft_window::Layout) -> craft_window::policy::Decision {
    use craft_window::policy::{CaptionFamily, Captions, Decision};
    let desktop = craft_window::desktop::Desktop { kind: craft_window::desktop::DesktopKind::Other, ssd: Some(false), gtk_theme: None, icon_theme: None };
    Decision { frame: Frame::Integrated(Captions { family: CaptionFamily::Generic, layout }), overridden: Some(true), desktop }
}

#[test]
fn the_menu_bar_shows_window_buttons_only_when_the_app_draws_the_frame() {
    let mut app = crate::MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
    let mut labels_drawn = |frame: Option<craft_window::policy::Decision>| {
        let ctx = egui::Context::default();
        crate::framework::install_headless(&ctx);
        if let Some(frame) = frame {
            WindowHost::of(&ctx).set_root_frame(frame);
        }
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| crate::menus::menu_bar(&mut app, ui));
        output.textures_delta.clear();
        let nodes = output.platform_output.accesskit_update.map(|update| update.nodes).unwrap_or_default();
        nodes.iter().filter_map(|(_, node)| node.label().map(str::to_owned)).collect::<Vec<_>>()
    };

    let desktop = labels_drawn(None);
    assert!(!desktop.iter().any(|label| label == "Close"), "{desktop:?}");

    let drawn = labels_drawn(Some(app_frame(craft_window::Layout::default())));
    for button in ["Minimize", "Maximize", "Close"] {
        assert!(drawn.iter().any(|label| label == button), "{button} in {drawn:?}");
    }

    let left = labels_drawn(Some(app_frame(craft_window::Layout { left: vec![craft_window::Button::Close], right: Vec::new() })));
    assert!(left.iter().any(|label| label == "Close"), "{left:?}");
    assert!(!left.iter().any(|label| label == "Minimize"), "{left:?}");
}
