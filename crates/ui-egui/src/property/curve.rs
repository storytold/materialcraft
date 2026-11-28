use crate::theme::Tokens;
use egui::{PointerButton, Pos2, Sense, Stroke, Ui, pos2, vec2};
use materialcraft_engine::core::Curve;

/// A tone curve editor: drag points, double-click to add one, right-click a point to remove it. Returns the new curve when it changed
pub fn curve_editor(ui: &mut Ui, id: &str, curve: &Curve) -> Option<Curve> {
    let tokens = Tokens::get(ui.ctx());
    let width = ui.available_width().min(240.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, width * 0.75), Sense::click_and_drag());
    crate::widgets::name(&response, egui::Role::Canvas, "Curve editor");
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, tokens.field);
    for i in 1..4 {
        let t = i as f32 / 4.0;
        painter.line_segment(
            [pos2(rect.left() + rect.width() * t, rect.top()), pos2(rect.left() + rect.width() * t, rect.bottom())],
            Stroke::new(1.0, tokens.separator),
        );
        painter.line_segment(
            [pos2(rect.left(), rect.top() + rect.height() * t), pos2(rect.right(), rect.top() + rect.height() * t)],
            Stroke::new(1.0, tokens.separator),
        );
    }
    let to_screen = |x: f32, y: f32| pos2(rect.left() + x * rect.width(), rect.bottom() - y * rect.height());
    let from_screen = |s: Pos2| (((s.x - rect.left()) / rect.width()).clamp(0.0, 1.0), ((rect.bottom() - s.y) / rect.height()).clamp(0.0, 1.0));
    let curve_points: Vec<Pos2> = (0..=64).map(|i| i as f32 / 64.0).map(|x| to_screen(x, curve.eval(x).clamp(0.0, 1.0))).collect();
    painter.add(egui::Shape::line(curve_points, Stroke::new(2.0, tokens.accent)));
    let drag_id = ui.make_persistent_id(id).with("drag");
    let mut dragging = ui.data(|d| d.get_temp::<Option<usize>>(drag_id).flatten());
    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    let nearest_point = |pointer_position| {
        curve
            .points
            .iter()
            .enumerate()
            .map(|(i, point)| (i, to_screen(point.x, point.y).distance(pointer_position)))
            .filter(|(_, d)| *d < 10.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let nearest = pointer.and_then(nearest_point);
    for (i, point) in curve.points.iter().enumerate() {
        let hot = Some(i) == nearest || Some(i) == dragging;
        painter.circle(
            to_screen(point.x, point.y),
            if hot { 5.0 } else { 4.0 },
            tokens.panel,
            Stroke::new(1.5, if hot { tokens.text_strong } else { tokens.text }),
        );
    }
    let mut edited_curve: Option<Curve> = None;
    if response.drag_started_by(PointerButton::Primary) {
        dragging = ui.input(|input| input.pointer.press_origin()).and_then(nearest_point);
    }
    if response.dragged_by(PointerButton::Primary)
        && let (Some(i), Some(pointer_position)) = (dragging, pointer)
    {
        let (x, y) = from_screen(pointer_position);
        let mut updated_curve = curve.clone();
        let last = updated_curve.points.len().saturating_sub(1);
        if let Some(point) = updated_curve.points.get_mut(i) {
            // End points keep their x
            if i != 0 && i != last {
                point.x = x;
            }
            point.y = y;
            let moved_x = point.x;

            // Stable sorting breaks ties by the points' previous positions.
            let new_index = updated_curve.points.iter().enumerate().filter(|(index, other)| other.x.total_cmp(&moved_x).then(index.cmp(&i)).is_lt()).count();
            updated_curve.points.sort_by(|a, b| a.x.total_cmp(&b.x));
            dragging = Some(new_index);
            edited_curve = Some(updated_curve);
        }
    }
    if response.drag_stopped() {
        dragging = None;
    }
    ui.data_mut(|d| d.insert_temp(drag_id, dragging));
    if response.double_clicked()
        && let Some(pointer_position) = pointer
    {
        let (x, _) = from_screen(pointer_position);
        let mut updated_curve = curve.clone();
        updated_curve.points.push(materialcraft_engine::core::CurvePoint::new(x, curve.eval(x)));
        updated_curve.points.sort_by(|a, b| a.x.total_cmp(&b.x));
        edited_curve = Some(updated_curve);
    }
    if response.secondary_clicked()
        && let Some(i) = nearest
        && i != 0
        && i + 1 != curve.points.len()
    {
        let mut updated_curve = curve.clone();
        updated_curve.points.remove(i);
        edited_curve = Some(updated_curve);
    }
    edited_curve
}

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers, PointerButton, vec2};
    use egui_kittest::{Harness, kittest::Queryable};
    use materialcraft_engine::core::{Curve, CurvePoint};

    fn harness(curve: Curve) -> Harness<'static, Curve> {
        Harness::builder().with_context_setup(crate::theme::install).build_ui_state(
            |ui, curve: &mut Curve| {
                if let Some(edited) = super::curve_editor(ui, "test_curve", curve) {
                    *curve = edited;
                }
            },
            curve,
        )
    }

    fn press(h: &mut Harness<'_, Curve>, start: egui::Pos2) {
        h.event(Event::PointerMoved(start));
        h.event(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.step();
    }

    #[test]
    fn dragging_starts_from_the_point_under_the_initial_press() {
        let mut curve = Curve::default();
        curve.points.insert(1, CurvePoint::new(0.5, 0.5));
        let mut h = harness(curve);
        let start = h.get_by_label("Curve editor").rect().center();
        press(&mut h, start);
        let moved = start + vec2(24.0, -20.0);
        h.event(Event::PointerMoved(moved));
        h.step();

        let points = &h.state().points;
        assert!(points[1].x > 0.5 && points[1].y > 0.5, "the pressed point should follow the pointer");
        assert_eq!(points[0], CurvePoint::new(0.0, 0.0));
        assert_eq!(points[2], CurvePoint::new(1.0, 1.0));
    }

    #[test]
    fn dragging_across_another_point_keeps_the_original_point_captured() {
        let neighbor = CurvePoint::new(0.5, 0.4);
        let curve = Curve { points: vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(0.25, 0.5), neighbor, CurvePoint::new(1.0, 1.0)] };
        let mut h = harness(curve);
        let rect = h.get_by_label("Curve editor").rect();
        let at = |x, y| rect.left_bottom() + vec2(rect.width() * x, -rect.height() * y);
        press(&mut h, at(0.25, 0.5));

        h.event(Event::PointerMoved(at(0.7, 0.6)));
        h.step();
        assert_eq!(h.state().points[1], neighbor);
        let after_crossing = h.state().points[2];
        assert!(after_crossing.x > neighbor.x);

        h.event(Event::PointerMoved(at(0.8, 0.7)));
        h.step();
        assert_eq!(h.state().points[1], neighbor, "the neighboring point stays in place");
        let dragged = h.state().points[2];
        assert!(dragged.x > after_crossing.x && dragged.y > after_crossing.y, "the captured point follows the second movement");
    }
}
