//! The 3D view in the Properties panel: the material model and its parameters, the lights, the
//! camera, the environment or the renderer, whichever was last asked for from the view

use egui::{RichText, Ui};
use egui_phosphor::regular as icon;
use materialcraft_engine::View3d;
use materialcraft_engine::geom::view::{
    Background, HeightTechnique, MaterialParameter, MaterialValue, MeshInfo, PARAMETERS, ParameterKind, PointLight, Projection, Shader,
};
use materialcraft_engine::view_cmds::ENVIRONMENTS;
use serde_json::{Value, json};

use super::Subject;
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::properties::section;
use crate::theme::Tokens;

const SWATCH_WIDTH: f32 = 64.0;
const NUMBER_WIDTH: f32 = 64.0;
/// The most repeats a tiling slider reaches; typing or a command goes further
const TILING_SLIDER_LIMIT: f32 = 16.0;
const SUBJECTS: [Subject; 5] = [Subject::Material, Subject::Lights, Subject::Camera, Subject::Environment, Subject::Renderer];

/// The panel's title while it shows a part of the 3D view
pub(crate) fn title(app: &MaterialCraftApp) -> Option<String> {
    Some(format!("{} - {}", tr(super::inspected(app)?.label()), tr("Properties")))
}

/// Draw the inspected part of the 3D view. Returns false when the panel shows something else
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) -> bool {
    let Some(subject) = super::inspected(app) else { return false };

    ui.push_id(("view3d-properties", subject.label()), |ui| {
        header(app, ui, subject);
        match subject {
            Subject::Material => material(app, ui),
            Subject::Lights => lights(app, ui),
            Subject::Camera => camera(app, ui),
            Subject::Environment => environment(app, ui),
            Subject::Renderer => renderer(app, ui),
        }
    });
    true
}

/// The parts of the 3D view to switch between, and a button that gives the panel back to the graph
fn header(app: &mut MaterialCraftApp, ui: &mut Ui, subject: Subject) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("3D View")).strong());
        for other in SUBJECTS {
            if crate::widgets::chip(ui, tr(other.label()), other == subject).clicked() {
                super::inspect(app, other);
            }
        }
        if crate::widgets::icon_button(ui, icon::X, tr("Back to the selection's properties"), false, 18.0).clicked() {
            app.view3d.inspected = None;
        }
    });
    ui.add_space(4.0);
}

/// Change one `view.3d` setting
fn set(app: &mut MaterialCraftApp, parameter: &str, value: impl Into<Value>) {
    app.run("view.3d", json!({parameter: value.into()}));
}

fn checkbox(app: &mut MaterialCraftApp, ui: &mut Ui, label: &str, parameter: &str, mut on: bool) {
    if ui.checkbox(&mut on, tr(label)).changed() {
        set(app, parameter, on);
    }
}

fn slider(app: &mut MaterialCraftApp, ui: &mut Ui, label: &str, parameter: &str, mut value: f32, range: std::ops::RangeInclusive<f32>, decimals: usize) {
    if crate::widgets::slider(ui, tr(label), &mut value, range, decimals).changed() {
        set(app, parameter, value);
    }
}

/// A labelled color swatch. Returns the new color when it was changed
fn color_row(ui: &mut Ui, id: &str, label: &str, mut color: [f32; 3]) -> Option<[f32; 3]> {
    ui.horizontal(|ui| {
        ui.label(tr(label));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| crate::widgets::color_swatch(ui, id, &mut color, SWATCH_WIDTH)).inner
    })
    .inner
    .then_some(color)
}

/// A labelled number field. Returns the new number when it was changed
fn number_row(ui: &mut Ui, label: &str, mut value: f32, range: std::ops::RangeInclusive<f32>, speed: f32) -> Option<f32> {
    ui.horizontal(|ui| {
        ui.label(tr(label));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let field = ui.add_sized([NUMBER_WIDTH, 18.0], egui::DragValue::new(&mut value).range(range).speed(speed).max_decimals(3));
            crate::widgets::name(&field, egui::Role::SpinButton, label);
            field.changed()
        })
        .inner
    })
    .inner
    .then_some(value)
}

fn camera(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let view = app.session.view3d.clone();

    section(ui, "view3d-camera", tr("Camera"), |ui| {
        slider(app, ui, "Focal Length (mm)", "focalLength", view.focal_length, View3d::FOCAL_LENGTH, 2);
        crate::widgets::combo(ui, tr("Projection"), tr(view.projection.label()), |ui| {
            for &projection in Projection::ALL {
                if ui.selectable_label(view.projection == projection, tr(projection.label())).clicked() {
                    set(app, "projection", projection.id());
                }
            }
        });
        crate::widgets::combo(ui, tr("Background Type"), tr(view.background.label()), |ui| {
            for &background in Background::ALL {
                if ui.selectable_label(view.background == background, tr(background.label())).clicked() {
                    set(app, "background", background.id());
                }
            }
        });
        ui.add_enabled_ui(view.background == Background::Color, |ui| {
            if let Some(color) = color_row(ui, "view3d-background-color", "Background Color", view.background_color) {
                set(app, "backgroundColor", json!(color));
            }
        });
    });

    section(ui, "view3d-resolution", tr("Render resolution"), |ui| {
        checkbox(app, ui, "Override Render Resolution", "overrideResolution", view.override_resolution);
        ui.add_enabled_ui(view.override_resolution, |ui| {
            checkbox(app, ui, "Use viewport resolution", "viewportResolution", view.viewport_resolution);
            ui.add_enabled_ui(!view.viewport_resolution, |ui| {
                let sides = (View3d::MIN_RENDER_SIDE as f32)..=(View3d::MAX_RENDER_SIDE as f32);
                let [width, height] = view.render_resolution;
                if let Some(width) = number_row(ui, "Render width", width as f32, sides.clone(), 4.0) {
                    set(app, "renderResolution", json!([width.round() as u32, height]));
                }
                if let Some(height) = number_row(ui, "Render height", height as f32, sides, 4.0) {
                    set(app, "renderResolution", json!([width, height.round() as u32]));
                }
            });
            crate::widgets::combo(ui, tr("Render resolution scale"), format!("{} %", view.render_scale), |ui| {
                for scale in View3d::RENDER_SCALES {
                    if ui.selectable_label(view.render_scale == scale, format!("{scale} %")).clicked() {
                        set(app, "renderScale", scale);
                    }
                }
            });
        });
    });

    section(ui, "view3d-clip", tr("Clip planes"), |ui| {
        checkbox(app, ui, "Automatic Clip Planes", "autoClip", view.auto_clip);
        ui.add_enabled_ui(!view.auto_clip, |ui| {
            if let Some(near) = number_row(ui, "Near Clip Plane", view.near_clip, View3d::CLIP, 0.5) {
                set(app, "nearClip", near);
            }
            if let Some(far) = number_row(ui, "Far Clip Plane", view.far_clip, View3d::CLIP, 5.0) {
                set(app, "farClip", far);
            }
        });
    });
}

fn lights(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let view = app.session.view3d.clone();

    section(ui, "view3d-ambient", tr("Ambient Light"), |ui| {
        let mut enabled = view.ambient.enabled;
        if ui.checkbox(&mut enabled, tr("Enabled")).changed() {
            set(app, "ambient", json!({"enabled": enabled}));
        }
        if let Some(color) = color_row(ui, "view3d-ambient-color", "Color", view.ambient.color) {
            set(app, "ambient", json!({"color": color}));
        }
    });

    for (light, parameter, title) in [(view.lights[0], "light1", "Point Light 1"), (view.lights[1], "light2", "Point Light 2")] {
        section(ui, parameter, tr(title), |ui| point_light(app, ui, parameter, &light));
    }
}

/// The controls of one point light; `parameter` is its name in `view.3d`
fn point_light(app: &mut MaterialCraftApp, ui: &mut Ui, parameter: &str, light: &PointLight) {
    let mut enabled = light.enabled;
    if ui.checkbox(&mut enabled, tr("Enabled")).changed() {
        set(app, parameter, json!({"enabled": enabled}));
    }
    if let Some(color) = color_row(ui, &format!("{parameter}-color"), "Color", light.color) {
        set(app, parameter, json!({"color": color}));
    }

    let numbers = [
        ("Intensity", "intensity", light.intensity, 0.0..=PointLight::MAX_INTENSITY, 2),
        ("Angle around the scene", "yaw", light.yaw, 0.0..=360.0, 0),
        ("Angle above the horizon", "pitch", light.pitch, -89.0..=89.0, 0),
        ("Distance", "distance", light.distance, PointLight::DISTANCE, 2),
    ];
    for (label, field, mut value, range, decimals) in numbers {
        if crate::widgets::slider(ui, tr(label), &mut value, range, decimals).changed() {
            set(app, parameter, json!({field: value}));
        }
    }
}

fn environment(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let view = app.session.view3d.clone();

    section(ui, "view3d-environment", tr("Environment"), |ui| {
        checkbox(app, ui, "Enabled", "envEnabled", view.env_enabled);
        slider(app, ui, "Exposure", "exposure", view.exposure, View3d::EXPOSURE, 2);
        slider(app, ui, "Rotation Angle", "envRotation", view.env_rotation, 0.0..=360.0, 0);

        let current = ENVIRONMENTS.iter().find(|(id, _)| *id == view.environment).map_or(view.environment.as_str(), |(_, label)| label);
        crate::widgets::combo(ui, tr("Environment image"), tr(current), |ui| {
            for (id, label) in ENVIRONMENTS {
                if ui.selectable_label(view.environment == *id, tr(label)).clicked() {
                    set(app, "environment", *id);
                }
            }
        });
    });
}

fn renderer(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let view = app.session.view3d.clone();

    section(ui, "view3d-renderer", tr("Renderer"), |ui| {
        let mut samples = view.samples as f32;
        if crate::widgets::slider(ui, tr("Samples per pixel side"), &mut samples, 1.0..=View3d::MAX_SAMPLES as f32, 0).changed() {
            set(app, "samples", samples.round() as u32);
        }
        let mut rays = view.ambient_occlusion as f32;
        if crate::widgets::slider(ui, tr("Ambient occlusion rays"), &mut rays, 0.0..=View3d::MAX_AMBIENT_OCCLUSION as f32, 0).changed() {
            set(app, "ambientOcclusion", rays.round() as u32);
        }
        checkbox(app, ui, "Shadows", "shadows", view.shadows);
        checkbox(app, ui, "Transparency (alpha)", "transparency", view.transparency);
        checkbox(app, ui, "Raise the mesh by the height output", "displacement", view.displacement);
    });

    section(ui, "view3d-preview", tr("Preview resolution"), |ui| {
        ui.horizontal(|ui| crate::preview_resolution::show(app, ui));
    });
}

fn material(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let view = app.session.view3d.clone();
    let tokens = Tokens::get(ui.ctx());

    crate::widgets::combo(ui, tr("Shader"), tr(view.shader.label()), |ui| {
        for &shader in Shader::ALL {
            if ui.selectable_label(view.shader == shader, tr(shader.label())).clicked() {
                set(app, "shader", shader.id());
            }
        }
    });
    if view.shader == Shader::MeshInfo {
        crate::widgets::combo(ui, tr("Shown on the mesh"), tr(view.mesh_info.label()), |ui| {
            for &shown in MeshInfo::ALL {
                if ui.selectable_label(view.mesh_info == shown, tr(shown.label())).clicked() {
                    set(app, "meshInfo", shown.id());
                }
            }
        });
    }
    if view.shader.shows_height() {
        crate::widgets::combo(ui, tr("Height technique"), tr(view.technique.label()), |ui| {
            for &technique in HeightTechnique::ALL {
                if ui.selectable_label(view.technique == technique, tr(technique.label())).clicked() {
                    set(app, "technique", technique.id());
                }
            }
        });
    }
    if view.uses("geometryNormal") {
        checkbox(app, ui, "Flip normal map Y (DirectX)", "normalYFlip", view.normal_y_flip);
    }
    ui.add_space(4.0);

    section(ui, "view3d-uvs", tr("UVs"), |ui| {
        let mut tiling = view.tiling;
        let track = *View3d::REPEATS.start()..=TILING_SLIDER_LIMIT.max(tiling);
        if crate::widgets::slider_with_numeric_range(ui, tr("Tiling"), &mut tiling, track, View3d::REPEATS, 2).changed() {
            set(app, "tiling", tiling);
        }
        checkbox(app, ui, "Enable physical size from graph", "physicalSize", view.physical_size);

        let [across, down] = view.uv_scale;
        let range = |value: f32| *View3d::REPEATS.start()..=TILING_SLIDER_LIMIT.max(value);
        let (mut new_across, mut new_down) = (across, down);
        if crate::widgets::slider_with_numeric_range(ui, tr("UV scale U"), &mut new_across, range(across), View3d::REPEATS, 2).changed() {
            set(app, "uvScale", json!([new_across, down]));
        }
        if crate::widgets::slider_with_numeric_range(ui, tr("UV scale V"), &mut new_down, range(down), View3d::REPEATS, 2).changed() {
            set(app, "uvScale", json!([across, new_down]));
        }
    });

    // The assigned outputs, by usage: a parameter with one shows where its image comes from
    let assigned: Vec<&str> = app.session.material_maps().into_iter().map(|(usage, _)| usage).collect();
    let listed: Vec<&MaterialParameter> = PARAMETERS.iter().filter(|parameter| parameter.applies_to(view.shader)).collect();
    let groups: Vec<&str> = listed.iter().map(|parameter| parameter.group).fold(Vec::new(), |mut groups, name| {
        if !groups.contains(&name) {
            groups.push(name);
        }
        groups
    });

    for name in groups {
        section(ui, &format!("view3d-material-{name}"), tr(name), |ui| {
            for parameter in listed.iter().filter(|parameter| parameter.group == name) {
                let from_graph = parameter.usage.is_some_and(|usage| assigned.contains(&usage));
                parameter_row(app, ui, &view, parameter, from_graph);
            }
            if name == "Height" {
                slider(app, ui, "Height scale", "heightScale", view.height_scale, View3d::HEIGHT_SCALE, 3);
                slider(app, ui, "Height level", "heightLevel", view.height_level, 0.0..=1.0, 2);
                let mut tessellation = view.tessellation;
                if crate::widgets::slider(ui, tr("Tessellation"), &mut tessellation, View3d::TESSELLATION, 0).changed() {
                    set(app, "tessellation", tessellation.round());
                }
            }
        });
    }

    if listed.is_empty() {
        ui.label(RichText::new(tr("This shader has no parameters")).color(tokens.text_dim));
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("Reset Common Parameters")).clicked() {
            app.run("view.3d.resetMaterial", json!({"common": true}));
        }
        if ui.button(tr("Reset Material")).clicked() {
            app.run("view.3d.resetMaterial", json!({}));
            app.run("view.3d.clear", json!({}));
        }
    });
}

/// One material parameter: a check box that switches it on, its name and its control. One fed
/// by a graph output shows that in place of the control, since the image replaces the value
fn parameter_row(app: &mut MaterialCraftApp, ui: &mut Ui, view: &View3d, parameter: &MaterialParameter, from_graph: bool) {
    let tokens = Tokens::get(ui.ctx());
    let mut enabled = !view.disabled.contains(parameter.id);
    let value = view.value(parameter.id).unwrap_or_else(|| parameter.kind.default());

    ui.horizontal(|ui| {
        let switch = ui.checkbox(&mut enabled, "").on_hover_text(tr("Use this parameter; off reads as if the material had none"));
        crate::widgets::name(&switch, egui::Role::CheckBox, &format!("Use {} {}", parameter.group, parameter.label));
        if switch.changed() {
            app.run("view.3d", json!({"channels": {parameter.id: enabled}}));
        }

        ui.add_enabled_ui(enabled, |ui| {
            ui.vertical(|ui| {
                if !parameter.rendered {
                    ui.label(RichText::new(tr("Stored with the state; this renderer does not show it")).size(10.0).color(tokens.text_dim));
                }
                if from_graph {
                    ui.horizontal(|ui| {
                        ui.label(tr(parameter.label));
                        ui.label(RichText::new(format!("{} {}", icon::FLOW_ARROW, tr("Graph output"))).color(tokens.text_dim));
                    });
                } else if let Some(changed) = parameter_control(ui, parameter, value) {
                    app.run("view.3d", json!({"material": {parameter.id: changed}}));
                }
            });
        });
    });
}

/// The control of a material parameter. Returns its new value when it was changed
fn parameter_control(ui: &mut Ui, parameter: &MaterialParameter, value: MaterialValue) -> Option<MaterialValue> {
    match (parameter.kind, value) {
        (ParameterKind::Slider { min, max, .. }, MaterialValue::Number(mut number)) => {
            crate::widgets::slider(ui, tr(parameter.label), &mut number, min..=max, 2).changed().then_some(MaterialValue::Number(number))
        }
        (ParameterKind::Level { .. }, MaterialValue::Number(mut number)) => {
            crate::widgets::slider(ui, tr(parameter.label), &mut number, 0.0..=1.0, 2).changed().then_some(MaterialValue::Number(number))
        }
        (ParameterKind::Color { .. }, MaterialValue::Color(color)) => color_row(ui, parameter.id, parameter.label, color).map(MaterialValue::Color),
        (ParameterKind::Toggle { .. }, MaterialValue::Toggle(mut on)) => {
            ui.checkbox(&mut on, tr(parameter.label)).changed().then_some(MaterialValue::Toggle(on))
        }
        _ => None,
    }
}
