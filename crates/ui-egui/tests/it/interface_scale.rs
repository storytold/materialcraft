//! Dragging the interface scale must not move its slider under a held pointer: the drag edits only the draft, and
//! the scale changes on Apply.
use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::{
    Harness,
    kittest::{By, NodeT as _, Queryable},
};
use materialcraft_ui_egui::{MaterialCraftApp, UiState};

fn harness(scale: f32) -> Harness<'static, MaterialCraftApp> {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    app.ui.windows.preferences = true;
    app.ui.prefs.ui_scale = scale;
    let mut h = Harness::builder()
        .with_size(vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            MaterialCraftApp::install_runtime(ctx, None).expect("runtime");
        })
        .build_ui_state(|ui, app: &mut MaterialCraftApp| materialcraft_ui_egui::preferences::show(app, ui), app);
    h.run_steps(10);
    h
}

fn point(h: &mut Harness<'static, MaterialCraftApp>, pos: Pos2, pressed: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}

/// The draft scale the slider shows.
fn shown(h: &Harness<'static, MaterialCraftApp>) -> f64 {
    h.get(By::new().role(Role::Slider).label("Interface scale")).accesskit_node().numeric_value().expect("the slider has a value")
}

#[test]
fn scale_drags_are_stable_and_apply_on_release() {
    for (initial, fraction) in [(1.0, 0.7), (1.0, 0.1), (1.5, 0.2)] {
        let mut h = harness(initial);
        let zoom = h.ctx.zoom_factor();
        let track = h.get(By::new().role(Role::Slider).label("Interface scale")).rect();
        let target = pos2(track.left() + track.width() * fraction, track.center().y);
        point(&mut h, track.center(), None);
        eprintln!("DIAG scale_drags_are_stable_and_apply_on_release: track {track:?} hover shown {} dragged {:?}", shown(&h), h.ctx.dragged_id());
        point(&mut h, track.center(), Some(true));
        eprintln!("DIAG scale_drags_are_stable_and_apply_on_release: press shown {} dragged {:?}", shown(&h), h.ctx.dragged_id());
        point(&mut h, target, None);
        eprintln!(
            "DIAG scale_drags_are_stable_and_apply_on_release: moved shown {} dragged {:?} draft {:?}",
            shown(&h),
            h.ctx.dragged_id(),
            h.state().ui.windows.preferences
        );

        let chosen = shown(&h);
        assert!((chosen - f64::from(initial)).abs() > 0.1, "the drag did not change the draft");
        for _ in 0..15 {
            point(&mut h, target, None);
            assert!((shown(&h) - chosen).abs() < 0.001, "the slider drifted under a stationary pointer");
            assert!((h.ctx.zoom_factor() - zoom).abs() < 0.001, "the zoom changed before Apply");
            assert!((h.state().ui.prefs.ui_scale - initial).abs() < 0.001, "the drag reached the committed scale");
        }
        point(&mut h, target, Some(false));
        h.run_steps(5);
        assert!((shown(&h) - chosen).abs() < 0.001, "release changed the dragged value");
        assert!((h.state().ui.prefs.ui_scale - initial).abs() < 0.001, "release applied without Apply");

        h.get(By::new().role(Role::Button).label("Apply")).click();
        h.run_steps(4);
        assert!((f64::from(h.state().ui.prefs.ui_scale) - chosen).abs() < 0.001, "Apply did not set the dragged scale");
    }
}
