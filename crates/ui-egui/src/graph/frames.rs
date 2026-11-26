use crate::MaterialCraftApp;
use egui::{Pos2, Rect, vec2};
use materialcraft_engine::core::NodeId;
use craft_ui::testing::access::{AccessTree, tree_of};

/// The whole app in an offscreen egui context, driven one frame at a time
pub(super) struct Frames {
    pub(super) ctx: egui::Context,
    pub(super) app: MaterialCraftApp,
    pub(super) time: f64,
    pub(super) canvas_text: Vec<String>,
    pub(super) access: Option<AccessTree>,
}

impl Frames {
    /// `session` after a few frames, so the canvas is laid out
    pub(super) fn new(session: materialcraft_engine::Session) -> Frames {
        let ctx = egui::Context::default();
        let app = MaterialCraftApp::headless(session, crate::UiState::default());
        let mut f = Frames { ctx, app, time: 0.0, canvas_text: Vec::new(), access: None };
        f.app.install_runtime(&f.ctx, None).expect("runtime");
        for _ in 0..3 {
            f.run(Vec::new(), egui::Modifiers::NONE);
        }
        f
    }

    /// Draw a desktop-sized frame with synthetic input
    pub(super) fn run(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        self.time += 0.05;
        let events = std::iter::once(egui::Event::ModifiersChanged(modifiers)).chain(events).collect();
        let screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 960.0)));
        let input = egui::RawInput { events, screen_rect, time: Some(self.time), ..Default::default() };
        let app = &mut self.app;
        eframe::App::raw_input_hook(&mut self.app, &self.ctx, &mut input);
        let mut output = self.ctx.run_ui(input, |ui| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        });
        let mut frame = eframe::Frame::_new_kittest();
        eframe::App::logic(&mut self.app, &self.ctx, &mut frame);
        self.access = Some(tree_of(&mut output));
        let canvas_shapes: Vec<_> = output.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Text(text) if self.app.graph.view.contains(text.pos))).map(|shape| shape.shape.clone()).collect();
        self.canvas_text = craft_ui::testing::text::painted(&canvas_shapes);
        // These interaction tests inspect shapes without uploading textures
        output.textures_delta.clear();
    }

    /// The bounds of a named control with `role` in the last frame.
    pub(super) fn rect_of(&self, role: egui::accesskit::Role, name: &str) -> Rect {
        self.access.as_ref().into_iter().flat_map(AccessTree::keys).find(|key| self.access.as_ref().is_some_and(|tree| tree.role_of(*key) == Some(craft_ui::runtime::access::Role::Node(role)) && tree.name_of(*key) == Some(name)))
            .and_then(|key| self.access.as_ref().and_then(|tree| tree.bounds_of(key))).unwrap_or_else(|| panic!("missing {role:?} control {name}"))
    }

    /// The midpoint of a published wire curve.
    pub(super) fn wire_midpoint(curve: &[Pos2; 4]) -> Pos2 {
        let p = (curve[0].to_vec2() + 3.0 * curve[1].to_vec2() + 3.0 * curve[2].to_vec2() + curve[3].to_vec2()) / 8.0;
        Pos2::new(p.x, p.y)
    }

    /// Press and release the primary button at `pos`, a second after any earlier click so the two never pair into a double-click
    pub(super) fn click(&mut self, pos: Pos2, modifiers: egui::Modifiers) {
        self.time += 1.0;
        self.run(vec![egui::Event::PointerMoved(pos)], modifiers);
        for pressed in [true, false] {
            self.run(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers }], modifiers);
        }
    }

    /// The center of node `id` on screen last frame
    pub(super) fn center(&self, id: NodeId) -> Pos2 {
        self.app.graph.rects.iter().find(|(n, _)| *n == id).map(|(_, r)| r.center()).unwrap_or_else(|| panic!("node {id:?} is not on screen"))
    }

    /// The node the 2D view shows once the graph has cooked
    pub(super) fn shown(&mut self) -> Option<NodeId> {
        self.app.session.cook();
        self.app.session.view2d_image().map(|(id, _)| id)
    }
}
