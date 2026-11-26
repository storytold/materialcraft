//! Pointer drags on a Properties slider land on exact round values at every pixel scale.
use egui::{Event, Modifiers, PointerButton, Rect, pos2, vec2};
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use materialcraft_engine::core::{NodeId, Value};
use materialcraft_ui_egui::{MaterialCraftApp, UiState};
use serde_json::json;

/// The step the Properties slider gives an angle, in degrees (one decimal).
const ANGLE_STEP: f32 = 0.1;
/// The step the Properties slider gives a parameter on a short range (three decimals).
const FINE_STEP: f32 = 0.001;
/// How far off the exact value the drag aims, as a share of the step: positive, so the unsnapped value differs from
/// the round one, and small enough that, with the calibration's error, it stays inside the step the drag snaps to.
const AIM_OFFSET_STEPS: f32 = 0.1;

fn harness(ppp: f32, definition: &str) -> (Harness<'static, MaterialCraftApp>, NodeId) {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    let id = NodeId(app.run("graph.node.add", json!({"def":definition})).unwrap()["id"].as_u64().unwrap());
    app.session.reset_history();
    let mut h = Harness::builder()
        .with_size(vec2(1600.0, 960.0))
        .with_pixels_per_point(ppp)
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx: &egui::Context| {
            MaterialCraftApp::install_runtime(ctx, None).expect("runtime");
        })
        .build_ui_state(
            |ui, app: &mut MaterialCraftApp| {
                app.step(&ui.ctx().clone());
                app.draw(ui);
            },
            app,
        );
    h.run_steps(8);
    (h, id)
}

fn slider(h: &Harness<'static, MaterialCraftApp>, label: &str, index: usize) -> Rect {
    h.get_all(By::new().role(egui::accesskit::Role::Slider).label(label)).nth(index).unwrap().rect()
}

/// Presses on the slider's track at `from` (a share of its width), drags to `x` and releases there.
fn drag(h: &mut Harness<'static, MaterialCraftApp>, label: &str, index: usize, from: f32, x: f32) {
    let rect = slider(h, label, index);
    let start = pos2(rect.left() + rect.width() * from, rect.center().y);
    let target = pos2(x, rect.center().y);
    h.event(Event::PointerMoved(start));
    h.step();
    h.event(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerMoved(start + vec2(10.0, 0.0)));
    h.step();
    h.event(Event::PointerMoved(target));
    h.step();
    h.event(Event::PointerButton { pos: target, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
    // Past the double-click time, so the next press starts a drag of its own.
    h.run_steps(30);
}

/// Drags the first slider named `label`, over `-half_span..=half_span`, to `value` by the slider's own geometry: the
/// track is linear and centred on the slider (the thumb insets both ends alike), and its scale is read from a first
/// drag released a quarter width right of the centre, which lands on `probe()`. The aim is `AIM_OFFSET_STEPS` of
/// `step` off the value, or past the end for an end of the range. Returns the probe's value.
fn drag_to(
    h: &mut Harness<'static, MaterialCraftApp>,
    label: &str,
    value: f32,
    half_span: f32,
    step: f32,
    probe: impl Fn(&Harness<'static, MaterialCraftApp>) -> f32,
) -> f32 {
    // The slider's track travel insets each end by half the thumb, so measure its value scale with a probe drag.
    let rect = slider(h, label, 0);
    drag(h, label, 0, 0.2, rect.center().x + rect.width() / 4.0);
    let probed = probe(h);
    let points_per_unit = rect.width() / 4.0 / probed;
    // An end of the range is reached by dragging past it, which holds the value there.
    let past_end = if value.abs() >= half_span { 4.0 * value.signum() } else { 0.0 };
    let x = rect.center().x + (value + AIM_OFFSET_STEPS * step) * points_per_unit + past_end;
    drag(h, label, 0, 0.1, x);
    probed
}

/// The angle in degrees.
fn angle(h: &Harness<'static, MaterialCraftApp>, id: NodeId) -> f32 {
    match h.state().session.graph().unwrap().node(id).unwrap().params.get("angle") {
        Some(Value::Float(v)) => *v * 360.0,
        None => 0.0,
        other => panic!("unexpected angle {other:?}"),
    }
}

fn offset(h: &Harness<'static, MaterialCraftApp>, id: NodeId) -> [f32; 2] {
    match h.state().session.graph().unwrap().node(id).unwrap().params.get("offset") {
        Some(Value::Float2(v)) => *v,
        None => [0.0, 0.0],
        other => panic!("unexpected offset {other:?}"),
    }
}

#[test]
fn dragging_can_reach_exact_round_angles_at_multiple_pixel_scales() {
    for ppp in [1.0, 2.0] {
        for degrees in [90.0, -90.0, 0.0, 360.0, -360.0] {
            let (mut h, id) = harness(ppp, "directional_blur");
            let probed = drag_to(&mut h, "Angle", degrees, 360.0, ANGLE_STEP, |h| angle(h, id));
            let params = &h.state().session.graph().unwrap().node(id).unwrap().params;
            assert_eq!(params.get("angle"), Some(&Value::Float(degrees / 360.0)), "angle={degrees}, pixels_per_point={ppp}");
            if degrees == 90.0 {
                // The drag is one undo step: undo returns to the probe's value, redo to the round one.
                h.state_mut().run("edit.undo", json!({})).unwrap();
                assert_eq!(angle(&h, id), probed);
                h.state_mut().run("edit.redo", json!({})).unwrap();
                assert_eq!(angle(&h, id), 90.0);
            }
        }
    }
}

#[test]
fn fractional_parameter_values_remain_reachable() {
    let (mut h, id) = harness(1.0, "transformation");
    drag_to(&mut h, "Y", 0.25, 1.0, FINE_STEP, |h| offset(h, id)[1]);
    drag_to(&mut h, "X", -0.5, 1.0, FINE_STEP, |h| offset(h, id)[0]);
    assert_eq!(offset(&h, id), [-0.5, 0.25]);
}
