use crate::{MaterialCraftApp, i18n::tr, theme::Tokens};
use egui::{RichText, Ui};
use materialcraft_engine::{
    GraphTarget,
    core::{ExposedParameter, NodeId, ParameterEditor, Value, ValueType},
};
use serde_json::json;

pub(crate) struct ExposeParameterDialog {
    target: GraphTarget,
    node: NodeId,
    parameter: String,
    input: ExposedParameter,
    focus_identifier: bool,
}

/// Keep the close control opposite the title while respecting the writing direction.
/// The layout is injected instead of changing process-global language state in tests.
fn modal_header(ui: &mut Ui, rtl: bool, title: impl FnOnce(&mut Ui), close: impl FnOnce(&mut Ui)) {
    let leading = if rtl { egui::Layout::right_to_left(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) };
    let trailing = if rtl { egui::Layout::left_to_right(egui::Align::Center) } else { egui::Layout::right_to_left(egui::Align::Center) };
    ui.with_layout(leading, |ui| {
        title(ui);
        ui.with_layout(trailing, close);
    });
}

/// Native Hebrew places the primary action on the right and Cancel to its left.
/// English retains the previous order (Cancel to the right of OK).
fn modal_actions(ui: &mut Ui, rtl: bool, confirm: impl FnOnce(&mut Ui), cancel: impl FnOnce(&mut Ui)) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if rtl {
            confirm(ui);
            cancel(ui);
        } else {
            cancel(ui);
            confirm(ui);
        }
    });
}

impl ExposeParameterDialog {
    pub(super) fn open(app: &mut MaterialCraftApp, node: NodeId, parameter: &str, label: &str, value: Option<&Value>) {
        let Some(draft) = app.run("graph.exposed.prepare", json!({"label": label, "default": value, "bind": {"node": node, "param": parameter}})) else {
            return;
        };

        let Ok(mut input) = serde_json::from_value::<ExposedParameter>(draft) else {
            return;
        };

        input.step = Some(input.step());
        app.expose_parameter_dialog =
            Some(Self { target: GraphTarget::active(&app.session), node, parameter: parameter.into(), input, focus_identifier: true });
    }

    pub(crate) fn show(app: &mut MaterialCraftApp, context: &egui::Context) {
        let Some(mut dialog) = app.expose_parameter_dialog.take() else {
            return;
        };

        // Commands can switch documents while a UI dialog is open
        if dialog.target != GraphTarget::active(&app.session) || dialog.target.get(&app.session).is_err() {
            return;
        }

        let mut confirm = false;
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::unique("expose-parameter")).show(context, |ui| {
            ui.set_width(620.0_f32.min((context.content_rect().width() - 48.0).max(240.0)));
            let rtl = crate::i18n::is_rtl();
            modal_header(
                ui,
                rtl,
                |ui| {
                    ui.heading(tr("Expose Parameter"));
                },
                |ui| {
                    cancel |= crate::widgets::icon_button(ui, egui_phosphor::regular::X, tr("Close expose parameter"), false, 20.0).clicked();
                },
            );
            ui.separator();
            egui::ScrollArea::vertical().max_height((context.content_rect().height() - 160.0).max(160.0)).show(ui, |ui| {
                dialog.fields(app, ui);
            });
            ui.add_space(8.0);
            ui.label(RichText::new(tr("Changes take effect when the graph input is created.")).italics().color(Tokens::get(context).text_dim));
            let validation = dialog.validation(app);
            if let Some(error) = &validation {
                ui.colored_label(Tokens::get(context).danger, tr(error));
            }
            ui.separator();
            modal_actions(
                ui,
                rtl,
                |ui| {
                    confirm = ui.add_enabled(validation.is_none(), egui::Button::new(tr("OK")).min_size(egui::vec2(100.0, 28.0))).clicked();
                },
                |ui| {
                    cancel |= ui.add_sized([100.0, 28.0], egui::Button::new(tr("Cancel"))).clicked();
                },
            );
        });
        if cancel || response.should_close() {
            return;
        }

        if confirm {
            let mut parameters = json!(dialog.input);
            parameters["bind"] = json!({"node": dialog.node, "param": dialog.parameter});
            if app.run("graph.exposed.add", parameters).is_some() {
                return;
            }
        }

        app.expose_parameter_dialog = Some(dialog);
    }

    fn fields(&mut self, app: &MaterialCraftApp, ui: &mut Ui) {
        egui::Grid::new("expose-fields").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
            ui.label(tr("Identifier"));
            let response = text(ui, "Identifier", &mut self.input.id);
            if self.focus_identifier {
                response.request_focus();
                self.focus_identifier = false;
            }
            ui.end_row();
            ui.label(tr("Label"));
            text(ui, "Label", &mut self.input.label);
            ui.end_row();
            ui.label(tr("Group"));
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 30.0).max(80.0);
                let response = ui.add(egui::TextEdit::singleline(&mut self.input.group).desired_width(width));
                crate::widgets::name(&response, egui::Role::TextInput, "Group");
                ui.menu_button(egui_phosphor::regular::CARET_DOWN, |ui| {
                    let groups: std::collections::BTreeSet<_> = app
                        .session
                        .graph()
                        .into_iter()
                        .flat_map(|graph| &graph.exposed)
                        .map(|input| input.group.as_str())
                        .filter(|group| !group.is_empty())
                        .collect();
                    if ui.button(tr("None")).clicked() {
                        self.input.group.clear();
                        ui.close();
                    }
                    for group in groups {
                        if ui.button(group).clicked() {
                            self.input.group = group.into();
                            ui.close();
                        }
                    }
                });
            });
            ui.end_row();
            ui.label(tr("Description"));
            let response = ui.add(egui::TextEdit::multiline(&mut self.input.description).desired_width(ui.available_width()).desired_rows(4));
            crate::widgets::name(&response, egui::Role::TextInput, "Description");
            ui.end_row();
            ui.label(tr("Type / Editor"));
            ui.horizontal(|ui| {
                ui.add_enabled(
                    false,
                    egui::Button::new(tr(ValueType::of(&self.input.default).map_or("Value", ValueType::label))).min_size(egui::vec2(160.0, 24.0)),
                );
                let selected = self.input.editor();
                let response = egui::ComboBox::from_id_salt("expose-editor").selected_text(tr(selected.label())).width(200.0).show_ui(ui, |ui| {
                    for editor in ParameterEditor::choices(&self.input.default) {
                        if ui.selectable_label(selected == *editor, tr(editor.label())).clicked() {
                            self.input.editor = Some(*editor);
                        }
                    }
                });
                crate::widgets::name(&response.response, egui::Role::ComboBox, "Editor");
            });
            ui.end_row();
            ui.label(tr("Default"));
            let editor = self.input.editor();
            let step = self.input.step();
            match &mut self.input.default {
                Value::Float(value) if editor == ParameterEditor::Slider => {
                    ui.spacing_mut().slider_width = (ui.available_width() - 60.0).max(100.0);
                    let response =
                        ui.add(egui::Slider::new(value, self.input.min..=self.input.max).clamping(egui::SliderClamping::Never).step_by(f64::from(step)));
                    crate::widgets::name(&response, egui::Role::Slider, "Default");
                }
                Value::Int(value) if editor == ParameterEditor::Slider => {
                    ui.spacing_mut().slider_width = (ui.available_width() - 60.0).max(100.0);
                    let response = ui.add(egui::Slider::new(value, self.input.min as i32..=self.input.max as i32).clamping(egui::SliderClamping::Never));
                    crate::widgets::name(&response, egui::Role::Slider, "Default");
                }
                _ => {
                    let mut preview = self.input.clone();
                    preview.label = tr("Default").into();
                    preview.clamp = false;
                    if let Some(value) = super::input::input_editor(ui, &preview, &self.input.default)
                        && let Some(value) = self.input.default.coerce_json(&value)
                    {
                        self.input.default = value;
                    }
                }
            }
            ui.end_row();
            if matches!(self.input.editor(), ParameterEditor::Slider | ParameterEditor::Input | ParameterEditor::Color) {
                for (label, value) in [("Min", &mut self.input.min), ("Max", &mut self.input.max)] {
                    ui.label(tr(label));
                    let response = ui.add(egui::DragValue::new(value).speed(0.01));
                    crate::widgets::name(&response, egui::Role::SpinButton, label);
                    ui.end_row();
                }
                ui.label(tr("Clamp"));
                let response = ui.checkbox(&mut self.input.clamp, tr("Limit values to Min and Max"));
                crate::widgets::name(&response, egui::Role::CheckBox, "Clamp");
                ui.end_row();
                ui.label(tr("Step"));
                let step = self.input.step.get_or_insert(0.01);
                let response = ui.add(egui::DragValue::new(step).speed(0.01).max_decimals(6));
                crate::widgets::name(&response, egui::Role::SpinButton, "Step");
                ui.end_row();
            }
        });
    }

    fn validation(&self, app: &MaterialCraftApp) -> Option<&'static str> {
        let identifier = self.input.id.trim();
        if identifier.is_empty() || !identifier.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
            return Some("Use letters, digits and underscores for the identifier.");
        }

        if app.session.graph().is_ok_and(|graph| graph.exposed.iter().any(|input| input.id == identifier)) {
            return Some("An input already uses this identifier.");
        }

        if !self.input.min.is_finite() || !self.input.max.is_finite() || self.input.min > self.input.max {
            return Some("Min must not exceed Max.");
        }

        if self.input.step.is_some_and(|step| !step.is_finite() || step <= 0.0) {
            return Some("Step must be greater than zero.");
        }

        None
    }
}

fn text(ui: &mut Ui, label: &str, value: &mut String) -> egui::Response {
    let response = ui.add(egui::TextEdit::singleline(value).desired_width(ui.available_width()));
    crate::widgets::name(&response, egui::Role::TextInput, label);
    response
}

#[cfg(test)]
mod rtl_layout_tests {
    use super::{modal_actions, modal_header};

    /// Actual egui rectangles, without modifying the process-global language.
    #[test]
    fn exposed_parameter_modal_mirrors_header_and_buttons_for_hebrew() {
        fn positions(rtl: bool) -> (f32, f32, f32, f32) {
            use std::cell::Cell;
            let ctx = egui::Context::default();
            let title_x = Cell::new(f32::NAN);
            let close_x = Cell::new(f32::NAN);
            let confirm_x = Cell::new(f32::NAN);
            let cancel_x = Cell::new(f32::NAN);
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 280.0))), ..Default::default() };
            let mut output = ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    modal_header(ui, rtl, |ui| title_x.set(ui.label("Expose Parameter").rect.center().x), |ui| close_x.set(ui.button("X").rect.center().x));
                    modal_actions(ui, rtl, |ui| confirm_x.set(ui.button("OK").rect.center().x), |ui| cancel_x.set(ui.button("Cancel").rect.center().x));
                });
            });
            output.textures_delta.clear();
            (title_x.get(), close_x.get(), confirm_x.get(), cancel_x.get())
        }

        let (title_en, close_en, ok_en, cancel_en) = positions(false);
        assert!(title_en < close_en, "English header must keep title left and close right: {title_en}, {close_en}");
        assert!(ok_en < cancel_en, "English buttons must keep OK left of Cancel: {ok_en}, {cancel_en}");

        let (title_he, close_he, ok_he, cancel_he) = positions(true);
        assert!(title_he > close_he, "Hebrew header must put title right and close left: {title_he}, {close_he}");
        assert!(ok_he > cancel_he, "Hebrew primary action must be to the right: {ok_he}, {cancel_he}");
        assert!(title_he.is_finite() && close_he.is_finite() && ok_he.is_finite() && cancel_he.is_finite());
    }
}
