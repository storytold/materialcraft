//! The 3D view's frames: a quick one right away while the camera moves (2×2 rays per pixel at
//! half resolution, less on a slow machine), the full-quality one on a worker thread once the
//! view's size and the camera have rested for [`SETTLE`]; until it lands the last frame stays up,
//! kept in proportion. A frame that is no longer wanted is cancelled. The browser has no threads
//! and renders here.
//!
//! The height output raises the mesh for real ([`Scene::with_relief`]), built on a worker thread
//! and cached until its inputs change.

use std::sync::Arc;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use egui::{ColorImage, Rect, TextureOptions, vec2};
use materialcraft_engine::View3d;
use materialcraft_render::{Lighting, Maps, Scene, Settings};

#[cfg(not(target_arch = "wasm32"))]
use super::frame_job::FrameJob;
use super::frame_scene::FrameScene;
#[cfg(not(target_arch = "wasm32"))]
use super::relief_job::ReliefJob;
use crate::MaterialCraftApp;

/// Seconds the view's size and the camera rest before the full-quality frame renders.
pub const SETTLE: f64 = 0.15;

/// What a moving frame is drawn with, smallest first: (fraction of the view's pixels, rays per
/// pixel along each side). The first is the bare minimum a slow machine falls back to
const MOVE_LEVELS: [(f32, u32); 4] = [(0.5, 1), (0.5, 2), (0.75, 2), (1.0, 2)];
/// Milliseconds a moving frame may take before it is drawn smaller
const MOVE_BUDGET_MS: f32 = 32.0;

/// Rays per pixel times the pixel count, relative to the view: what a level costs
fn move_cost(level: usize) -> f32 {
    let (scale, rays) = MOVE_LEVELS[level.min(MOVE_LEVELS.len() - 1)];
    (scale * rays as f32).powi(2)
}

/// The level (an index of [`MOVE_LEVELS`]) for the next moving frame, given the level of the last
/// one and how long it took: lower when over budget, higher when the next level would fit
fn next_move_level(level: usize, ms: f32) -> usize {
    let level = level.min(MOVE_LEVELS.len() - 1);
    if ms > MOVE_BUDGET_MS {
        return level.saturating_sub(1);
    }

    if level + 1 < MOVE_LEVELS.len() && ms * move_cost(level + 1) / move_cost(level) < MOVE_BUDGET_MS * 0.9 { level + 1 } else { level }
}

/// The most triangles of the displaced mesh in the browser, which builds it on the UI thread
const BROWSER_RELIEF_TRIANGLES: usize = 120_000;

/// Triangle budget of the view's displaced mesh
fn relief_triangles(view: &View3d) -> usize {
    let triangles = materialcraft_render::relief_triangles(view);
    if cfg!(target_arch = "wasm32") { triangles.min(BROWSER_RELIEF_TRIANGLES) } else { triangles }
}

/// A rendered frame: RGBA8 pixels and how long they took.
pub(super) struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    milliseconds: f32,
    scene: FrameScene,
}

/// The view's size in whole points: part of the content key and of `tex_size`.
fn view_size(rect: Rect) -> (u32, u32) {
    (rect.width() as u32, rect.height() as u32)
}

/// Where the last frame goes in `rect`: all of it when it was rendered for this size, otherwise
/// at the view's height and centred. The camera's field of view is vertical, so that is what a
/// new render will show (cropped, or with bars at the sides) while a resize is in progress,
/// instead of a stretched picture.
fn frame_rect(rect: Rect, rendered: Option<(u32, u32)>) -> Rect {
    match rendered {
        Some((width, height)) if height > 0 && (width, height) != view_size(rect) => {
            Rect::from_center_size(rect.center(), vec2(rect.height() * width as f32 / height as f32, rect.height()))
        }
        _ => rect,
    }
}

/// The view renders at a resolution of its own instead of the size it has on screen
fn fixed_resolution(view: &View3d) -> bool {
    view.override_resolution && !view.viewport_resolution
}

/// Where a frame of the view's own resolution goes in `rect`: as large as fits, centred, with bars
/// around it. A view that renders at its size on screen fills `rect`
pub(super) fn frame_area(view: &View3d, rect: Rect) -> Rect {
    if !fixed_resolution(view) || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return rect;
    }

    fitted_frame_area(rect, (view.render_resolution[0], view.render_resolution[1]))
}

fn fitted_frame_area(rect: Rect, (width, height): (u32, u32)) -> Rect {
    let (width, height) = (width.max(1) as f32, height.max(1) as f32);
    let scale = (rect.width() / width).min(rect.height() / height);
    Rect::from_center_size(rect.center(), vec2(width * scale, height * scale))
}

/// Where the texture is drawn in `rect`, and so where the camera's picture is on screen
pub(super) fn shown_rect(app: &MaterialCraftApp, rect: Rect) -> Rect {
    let view = shown_view(app);
    if fixed_resolution(view) {
        app.view3d.shown.as_ref().map_or_else(|| frame_area(view, rect), |(width, height, _)| fitted_frame_area(rect, (*width, *height)))
    } else {
        frame_rect(rect, app.view3d.tex_size)
    }
}

/// The pixels of a resting frame of a view that takes `rect` on screen
pub(super) fn frame_pixels(view: &View3d, rect: Rect, pixels_per_point: f32) -> (u32, u32) {
    let side = |pixels: f32| (pixels as u32).clamp(View3d::MIN_RENDER_SIDE, View3d::MAX_RENDER_SIDE);
    let on_screen = [rect.width() * pixels_per_point, rect.height() * pixels_per_point];
    if !view.override_resolution {
        return (side(on_screen[0]), side(on_screen[1]));
    }

    let scale = view.render_scale as f32 / 100.0;
    let [width, height] = if view.viewport_resolution { on_screen } else { view.render_resolution.map(|pixels| pixels as f32) };
    (side(width * scale), side(height * scale))
}

/// The view without what only the overlays read, so switching a helper on does not render the frame again
fn rendered_part(view: &View3d) -> View3d {
    let fresh = View3d::default();
    View3d {
        show_axis: fresh.show_axis,
        show_grid: fresh.show_grid,
        bounding_box: fresh.bounding_box,
        light_helpers: fresh.light_helpers,
        vertex_tangent_space: fresh.vertex_tangent_space,
        show_resolution: fresh.show_resolution,
        scene_stats: fresh.scene_stats,
        fps: fresh.fps,
        ..view.clone()
    }
}

fn content_key(app: &MaterialCraftApp, size: (u32, u32), pixels: (u32, u32), relief_shown: Option<u64>) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    (size, pixels, app.session.custom_mesh.as_ref().map(|(name, _)| (name, app.session.custom_mesh_generation)), relief_shown).hash(&mut hasher);
    serde_json::to_string(&rendered_part(&app.session.rendered_view3d())).unwrap_or_default().hash(&mut hasher);
    app.session.material_key().hash(&mut hasher);
    hasher.finish()
}

/// Keep the 3D view's texture up to date: a quick frame right away while the camera moves, the
/// full-quality one once the view's size and the camera have rested for [`SETTLE`].
pub(super) fn render(app: &mut MaterialCraftApp, ctx: &egui::Context, rect: Rect, moving: bool) {
    let now = ctx.input(|input| input.time);
    let size = view_size(rect);
    if moving || app.view3d.size.is_some_and(|last| last != size) {
        app.view3d.changed_at = Some(now);
    }
    app.view3d.size = Some(size);

    let relief_shown = update_relief(app, ctx);
    let key = content_key(app, size, frame_pixels(&app.session.view3d, rect, ctx.pixels_per_point()), relief_shown);
    collect(app, ctx, key);
    if app.view3d.key == Some((key, true)) || (moving && app.view3d.key == Some((key, false))) {
        return;
    }

    if moving {
        // As large as keeps the camera following the pointer, with 2×2 rays so edges are not stair-stepped
        let level = app.view3d.move_level.min(MOVE_LEVELS.len() - 1);
        let (scale, rays) = MOVE_LEVELS[level];
        let pixels = moving_pixels(&app.session.view3d, rect, ctx.pixels_per_point(), scale);
        if let Some(frame) = render_now(app, pixels, rays, false) {
            app.view3d.move_level = next_move_level(level, frame.milliseconds);
            show_frame(app, ctx, frame, key, false, size);
        }
        ctx.request_repaint_after(Duration::from_secs_f64(SETTLE));
        return;
    }

    let wait = app.view3d.changed_at.map_or(0.0, |changed| SETTLE - (now - changed));
    if wait > 0.0 {
        // Mid-resize or the camera just stopped: the last frame stays up (see `frame_rect`)
        ctx.request_repaint_after(Duration::from_secs_f64(wait.min(SETTLE)));
        return;
    }

    if app.view3d.job.as_ref().is_some_and(|job| job.key == key) {
        return;
    }

    start_full(app, ctx, rect, key, size);
}

/// Show a finished full-quality frame if it is still the one wanted; drop (and so cancel) one
/// that is not.
fn collect(app: &mut MaterialCraftApp, ctx: &egui::Context, key: u64) {
    let Some(job) = app.view3d.job.take() else { return };
    if job.key != key {
        return;
    }

    match job.rx.try_recv() {
        Ok(Some(frame)) => show_frame(app, ctx, frame, key, true, job.size),
        Ok(None) | Err(TryRecvError::Disconnected) => {}
        Err(TryRecvError::Empty) => app.view3d.job = Some(job),
    }
}

/// The plain scene of the current view and its cache key, building it when the scene changed.
pub(super) fn base_scene(app: &mut MaterialCraftApp) -> (String, Arc<Scene>) {
    let view = &app.session.view3d;
    let scene_key = match (&*view.scene, &app.session.custom_mesh) {
        ("custom", Some((name, _))) => format!("custom:{name}:{}", app.session.custom_mesh_generation),
        _ => view.scene.clone(),
    };

    match &app.view3d.scene {
        Some((key, scene)) if *key == scene_key => (scene_key, scene.clone()),
        _ => {
            let scene = match (&*view.scene, &app.session.custom_mesh) {
                ("custom", Some((_, mesh))) => Scene::new((**mesh).clone()),
                _ => Scene::builtin(&view.scene),
            };
            let scene = Arc::new(scene);
            app.view3d.scene = Some((scene_key.clone(), scene.clone()));
            (scene_key, scene)
        }
    }
}

/// What the displaced scene depends on (scene, height output cook, relief settings, normal map); `None` when nothing is raised.
fn relief_key(app: &MaterialCraftApp, scene_key: &str, maps: &Maps) -> Option<u64> {
    use std::hash::{DefaultHasher, Hash, Hasher};

    let view = &app.session.rendered_view3d();
    let relief = materialcraft_render::relief_of(view)?;
    if maps.height.is_none() || relief.depth <= 0.0 {
        return None;
    }

    let sources = app.session.material_preview.sources(&app.session.pkg);
    let height_source = sources.get(&materialcraft_engine::core::MaterialChannel::Height)?;
    let height_cook = app.session.material_source_key(height_source);
    let shaded_by_normal_map = maps.normal.is_some() && view.uses("geometryNormal");
    let [repeats_u, repeats_v] = materialcraft_render::uv_repeats(view);

    let mut hasher = DefaultHasher::new();
    (scene_key, height_cook, shaded_by_normal_map, relief_triangles(view)).hash(&mut hasher);
    for number in [relief.depth, relief.level, repeats_u, repeats_v] {
        number.to_bits().hash(&mut hasher);
    }
    Some(hasher.finish())
}

/// Collect a finished relief build or start a missing one; returns the key of the displaced scene shown (`None` = plain mesh).
fn update_relief(app: &mut MaterialCraftApp, ctx: &egui::Context) -> Option<u64> {
    let (scene_key, base) = base_scene(app);
    let maps = Maps::from_usages(app.session.material_maps());
    let wanted = relief_key(app, &scene_key, &maps);

    // The plain scene shows while a different relief builds, so the old mesh can be released
    if app.view3d.displaced.as_ref().is_some_and(|(key, _)| Some(*key) != wanted) {
        app.view3d.displaced = None;
    }

    if let Some(job) = app.view3d.relief_job.take() {
        match job.rx.try_recv() {
            Ok(Some(scene)) if Some(job.key) == wanted => app.view3d.displaced = Some((job.key, scene)),
            Ok(_) | Err(TryRecvError::Disconnected) => {}
            // Keep one worker until it finishes; dropping the receiver cannot cancel it
            Err(TryRecvError::Empty) => app.view3d.relief_job = Some(job),
        }
    }

    let wanted = wanted?;

    if app.view3d.displaced.as_ref().is_some_and(|(key, _)| *key == wanted) {
        return Some(wanted);
    }

    if app.view3d.relief_job.is_none() {
        let view = app.session.rendered_view3d();
        start_relief(app, ctx, wanted, base, maps, view);
    }
    None
}

/// Build the displaced scene on a worker thread.
#[cfg(not(target_arch = "wasm32"))]
fn start_relief(app: &mut MaterialCraftApp, ctx: &egui::Context, key: u64, base: Arc<Scene>, maps: Maps, view: View3d) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (repaint, viewport) = (ctx.clone(), ctx.viewport_id());
    let (thread_base, thread_maps, thread_view) = (base.clone(), maps.clone(), view.clone());
    let spawned = std::thread::Builder::new().name("materialcraft-relief".into()).spawn(move || {
        let _ = tx.send(materialcraft_render::raised(&thread_base, &thread_maps, &thread_view, relief_triangles(&thread_view)).map(Arc::new));
        repaint.request_repaint_of(viewport);
    });

    match spawned {
        Ok(_) => app.view3d.relief_job = Some(ReliefJob { key, rx }),
        Err(error) => {
            log::warn!("could not start a 3D view relief thread ({error}); building it here");
            app.view3d.displaced = materialcraft_render::raised(&base, &maps, &view, relief_triangles(&view)).map(|scene| (key, Arc::new(scene)));
        }
    }
}

/// The browser has no threads: the displaced scene builds here (the page pauses).
#[cfg(target_arch = "wasm32")]
fn start_relief(app: &mut MaterialCraftApp, _ctx: &egui::Context, key: u64, base: Arc<Scene>, maps: Maps, view: View3d) {
    app.view3d.displaced = materialcraft_render::raised(&base, &maps, &view, relief_triangles(&view)).map(|scene| (key, Arc::new(scene)));
}

/// The geometry to render next: current relief when available, otherwise the base mesh
fn requested_scene(app: &mut MaterialCraftApp) -> Arc<Scene> {
    let (scene_key, base) = base_scene(app);
    let maps = Maps::from_usages(app.session.material_maps());
    let wanted = relief_key(app, &scene_key, &maps);

    match &app.view3d.displaced {
        Some((key, displaced)) if Some(*key) == wanted => displaced.clone(),
        _ => base,
    }
}

/// Export and picking follow the uploaded frame while replacement work is pending
pub(super) fn shown_scene(app: &mut MaterialCraftApp) -> Arc<Scene> {
    match &app.view3d.shown_scene {
        Some(frame) => frame.scene.clone(),
        None => requested_scene(app),
    }
}

/// The camera and render settings used for the uploaded frame
pub(super) fn shown_view(app: &MaterialCraftApp) -> &View3d {
    app.view3d.shown_scene.as_ref().map_or(&app.session.view3d, |frame| &frame.view)
}

pub(super) fn shown_label(app: &MaterialCraftApp) -> String {
    app.view3d.shown_scene.as_ref().map_or_else(|| super::scene_label(app), |frame| frame.label.clone())
}

/// The scene, lighting, maps and settings of the current view. A frame drawn while the camera
/// moves (`resting` false) leaves out the ambient occlusion rays
fn inputs(app: &mut MaterialCraftApp, supersample: u32, resting: bool) -> Option<(FrameScene, Arc<Lighting>, Maps, Settings)> {
    let scene = requested_scene(app);
    let view = app.session.rendered_view3d();
    if app.view3d.light.as_ref().is_none_or(|light| light.name != view.environment) {
        app.view3d.light = Some(Arc::new(Lighting::new(&view.environment)));
    }

    let mut settings = Settings { supersample, ..Settings::for_view(&view) };
    if !resting {
        settings.ao_rays = 0;
    }
    let maps = Maps::from_usages(app.session.material_maps());
    let light = app.view3d.light.clone()?;
    Some((FrameScene { scene, view, label: super::scene_label(app) }, light, maps, settings))
}

/// The pixels of a frame drawn while the camera moves: `scale` of the view's pixels on screen, in
/// the proportions of the resting frame and never more than it
fn moving_pixels(view: &View3d, rect: Rect, pixels_per_point: f32, scale: f32) -> (u32, u32) {
    let (width, height) = frame_pixels(view, rect, pixels_per_point);
    let area = frame_area(view, rect);
    let on_screen = (area.height() * pixels_per_point).max(1.0);
    let shrink = (on_screen / height as f32).min(1.0) * scale;
    let side = |pixels: u32| ((pixels as f32 * shrink) as u32).clamp(View3d::MIN_RENDER_SIDE, View3d::MAX_RENDER_SIDE);
    (side(width), side(height))
}

/// Render a frame on this thread.
fn render_now(app: &mut MaterialCraftApp, (width, height): (u32, u32), supersample: u32, resting: bool) -> Option<Frame> {
    let (scene, light, maps, settings) = inputs(app, supersample, resting)?;
    let started = crate::now_ms();
    let pixels = materialcraft_render::render(&scene.scene, &maps, &light, &settings, width, height);
    Some(Frame { width, height, pixels, milliseconds: (crate::now_ms() - started) as f32, scene })
}

/// Rays per pixel along each side of the view's resting frame
fn resting_samples(view: &View3d) -> u32 {
    view.samples.clamp(1, View3d::MAX_SAMPLES)
}

/// Start the full-quality frame (the view's samples per pixel, full resolution) on a worker thread.
#[cfg(not(target_arch = "wasm32"))]
fn start_full(app: &mut MaterialCraftApp, ctx: &egui::Context, rect: Rect, key: u64, size: (u32, u32)) {
    let samples = resting_samples(&app.session.view3d);
    let Some((scene, light, maps, settings)) = inputs(app, samples, true) else { return };
    let (width, height) = frame_pixels(&app.session.view3d, rect, ctx.pixels_per_point());
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let (stop, repaint, viewport) = (cancel.clone(), ctx.clone(), ctx.viewport_id());
    let spawned = std::thread::Builder::new().name("materialcraft-view3d".into()).spawn(move || {
        let started = crate::now_ms();
        let rendered = materialcraft_render::render_cancellable(&scene.scene, &maps, &light, &settings, width, height, &stop);
        let _ = tx.send(rendered.map(|pixels| Frame { width, height, pixels, milliseconds: (crate::now_ms() - started) as f32, scene }));
        repaint.request_repaint_of(viewport);
    });

    match spawned {
        Ok(_) => app.view3d.job = Some(FrameJob { key, size, cancel, rx }),
        Err(error) => {
            log::warn!("could not start a 3D view render thread ({error}); rendering here");
            if let Some(frame) = render_now(app, (width, height), samples, true) {
                show_frame(app, ctx, frame, key, true, size);
            }
        }
    }
}

/// The browser has no threads: the full-quality frame renders here (the page pauses).
#[cfg(target_arch = "wasm32")]
fn start_full(app: &mut MaterialCraftApp, ctx: &egui::Context, rect: Rect, key: u64, size: (u32, u32)) {
    let samples = resting_samples(&app.session.view3d);
    let pixels = frame_pixels(&app.session.view3d, rect, ctx.pixels_per_point());
    if let Some(frame) = render_now(app, pixels, samples, true) {
        show_frame(app, ctx, frame, key, true, size);
    }
}

/// Upload a frame and remember what it shows.
fn show_frame(app: &mut MaterialCraftApp, ctx: &egui::Context, frame: Frame, key: u64, full: bool, size: (u32, u32)) {
    let state = &mut app.view3d;
    let image = ColorImage::from_rgba_unmultiplied([frame.width as usize, frame.height as usize], &frame.pixels);
    match &mut state.tex {
        Some(texture) => texture.set(image, TextureOptions::LINEAR),
        None => state.tex = Some(ctx.load_texture("view3d", image, TextureOptions::LINEAR)),
    }

    state.render_ms = frame.milliseconds;
    state.renders += 1;
    state.shown = Some((frame.width, frame.height, Arc::new(frame.pixels)));
    state.shown_scene = Some(frame.scene);
    state.key = Some((key, full));
    state.tex_size = Some(size);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn reloading_a_custom_mesh_with_the_same_name_invalidates_scene_and_frame() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        let first_mesh = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let second_mesh = b"v 0 0 0\nv 2 0 0\nv 0 1 0\nf 1 2 3\n";
        let size = (64, 48);

        app.session.load_mesh_bytes("same.obj", first_mesh).unwrap();
        let (first_scene_key, first_scene) = base_scene(&mut app);
        let first_scene_weak = Arc::downgrade(&first_scene);
        let first_frame_key = content_key(&app, size, size, None);
        app.session.load_mesh_bytes("same.obj", second_mesh).unwrap();
        let (second_scene_key, second_scene) = base_scene(&mut app);
        let second_frame_key = content_key(&app, size, size, None);

        assert_ne!(first_scene_key, second_scene_key);
        assert_ne!(first_frame_key, second_frame_key);
        assert!(!Arc::ptr_eq(&first_scene, &second_scene));
        assert_ne!(first_scene.mesh.positions, second_scene.mesh.positions);
        drop(first_scene);
        assert!(first_scene_weak.upgrade().is_none());
    }

    #[test]
    fn relief_tracks_assigned_height_while_another_graph_is_open() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        app.session.preview_size = Some(16);
        app.session.cook();
        let (scene_key, _) = base_scene(&mut app);
        let original = relief_key(&app, &scene_key, &Maps::from_usages(app.session.material_maps())).unwrap();
        app.session.pkg.graphs.push(materialcraft_engine::core::Graph::new("Other"));
        app.session.graph = 1;
        app.session.cook();
        assert_eq!(relief_key(&app, &scene_key, &Maps::from_usages(app.session.material_maps())), Some(original));
        app.session.pkg.graphs[0].seed += 1;
        app.session.cook();
        assert_ne!(relief_key(&app, &scene_key, &Maps::from_usages(app.session.material_maps())), Some(original));
    }

    #[test]
    fn changing_relief_keeps_one_worker_and_discards_stale_geometry() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        app.session.cook();
        let (scene_key, base) = base_scene(&mut app);
        let maps = Maps::from_usages(app.session.material_maps());
        let old_key = relief_key(&app, &scene_key, &maps).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        app.view3d.relief_job = Some(ReliefJob { key: old_key, rx });
        let ctx = egui::Context::default();
        for index in 1..=20 {
            app.session.view3d.height_scale = index as f32 * 0.001;
            assert_eq!(update_relief(&mut app, &ctx), None);
            assert_eq!(app.view3d.relief_job.as_ref().unwrap().key, old_key, "a second subdivision worker started");
            assert!(app.view3d.busy(), "geometry work must count as a busy view");
        }
        let wanted = relief_key(&app, &scene_key, &maps).unwrap();
        assert_ne!(old_key, wanted);
        app.view3d.displaced = Some((wanted, base.clone()));
        tx.send(Some(Arc::new(Scene::builtin("sphere")))).unwrap();
        assert_eq!(update_relief(&mut app, &ctx), Some(wanted));
        assert!(app.view3d.relief_job.is_none());
        assert!(!app.view3d.busy());
        assert!(Arc::ptr_eq(&app.view3d.displaced.as_ref().unwrap().1, &base), "outdated geometry replaced the current scene");
    }

    #[test]
    fn a_pending_relief_change_keeps_the_scene_of_the_displayed_frame() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        app.session.preview_size = Some(16);
        app.session.cook();
        let (scene_key, _) = base_scene(&mut app);
        let maps = Maps::from_usages(app.session.material_maps());
        let old_key = relief_key(&app, &scene_key, &maps).unwrap();
        let displayed_scene = Arc::new(Scene::builtin("sphere"));
        let displayed_camera = app.session.view3d.yaw;
        let displayed_label = super::super::scene_label(&app);
        app.view3d.displaced = Some((old_key, displayed_scene.clone()));
        let context = egui::Context::default();
        let frame = render_now(&mut app, (16, 16), 1, false).unwrap();
        show_frame(&mut app, &context, frame, 0, true, (16, 16));
        let (_sender, receiver) = std::sync::mpsc::channel();
        app.view3d.relief_job = Some(ReliefJob { key: old_key, rx: receiver });

        app.session.view3d.height_scale += 0.001;
        app.session.view3d.yaw += 90.0;
        app.session.view3d.scene = "rounded_cube".into();
        update_relief(&mut app, &context);

        assert!(Arc::ptr_eq(&shown_scene(&mut app), &displayed_scene), "export and picking must use the mesh still visible on screen");
        assert_eq!(shown_view(&app).yaw, displayed_camera, "picking and overlays must use the displayed camera");
        assert_eq!(shown_label(&app), displayed_label, "export filenames must describe the displayed mesh");
    }

    #[test]
    fn replacing_displayed_frames_releases_each_previous_scene() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        app.session.preview_size = Some(16);
        app.session.cook();
        let context = egui::Context::default();
        let mut previous = None;
        let mut peak_displayed_vertex_bytes = 0;
        let mut peak_frame_transition_vertex_bytes = 0;

        for index in 0..8 {
            let (scene_key, _) = base_scene(&mut app);
            let maps = Maps::from_usages(app.session.material_maps());
            let key = relief_key(&app, &scene_key, &maps).unwrap();
            let scene = Arc::new(Scene::builtin(if index % 2 == 0 { "sphere" } else { "rounded_cube" }));
            let current = Arc::downgrade(&scene);
            let vertex_bytes = scene.mesh.positions.capacity() * std::mem::size_of::<materialcraft_engine::geom::glam::Vec3>();
            peak_displayed_vertex_bytes = peak_displayed_vertex_bytes.max(vertex_bytes);
            app.view3d.displaced = Some((key, scene));
            let frame = render_now(&mut app, (16, 16), 1, false).unwrap();
            let previous_vertex_bytes = app
                .view3d
                .shown_scene
                .as_ref()
                .map_or(0, |frame| frame.scene.mesh.positions.capacity() * std::mem::size_of::<materialcraft_engine::geom::glam::Vec3>());
            peak_frame_transition_vertex_bytes = peak_frame_transition_vertex_bytes.max(previous_vertex_bytes + vertex_bytes);
            show_frame(&mut app, &context, frame, key, true, (16, 16));

            app.view3d.displaced = None;
            app.session.view3d.height_scale += 0.001;

            assert!(current.upgrade().is_some(), "visible geometry must survive until the next uploaded frame");
            if let Some(old) = previous.replace(current) {
                assert!(old.upgrade().is_none(), "replacing a displayed frame must release its old geometry");
            }
        }

        drop(app);
        assert!(previous.unwrap().upgrade().is_none(), "view shutdown must release the final displayed scene");
        println!(
            "distinct displayed scenes: 8; peak displayed vertex capacity: {peak_displayed_vertex_bytes} bytes; peak displayed plus replacement vertex capacity: {peak_frame_transition_vertex_bytes} bytes; previous scene bytes retained after upload: 0; final scene bytes retained after shutdown: 0"
        );
    }

    #[test]
    fn a_new_relief_key_releases_the_old_displaced_scene_while_the_worker_is_busy() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        app.session.cook();
        let (scene_key, _) = base_scene(&mut app);
        let maps = Maps::from_usages(app.session.material_maps());
        let mut senders = Vec::new();
        let mut largest_old_vertex_buffer = 0;
        let mut peak_retained_vertex_bytes = 0;

        for index in 0..8 {
            let old_key = relief_key(&app, &scene_key, &maps).unwrap();
            let scene = if index % 2 == 0 { "sphere" } else { "rounded_cube" };
            let old_scene = Arc::new(Scene::builtin(scene));
            let old_scene_weak = Arc::downgrade(&old_scene);
            let old_vertex_bytes = old_scene.mesh.positions.capacity() * old_scene.mesh.positions.first().map_or(0, std::mem::size_of_val);
            largest_old_vertex_buffer = largest_old_vertex_buffer.max(old_vertex_bytes);
            app.view3d.displaced = Some((old_key, old_scene));

            let (sender, receiver) = std::sync::mpsc::channel();
            senders.push(sender);
            app.view3d.relief_job = Some(ReliefJob { key: old_key, rx: receiver });
            app.session.view3d.height_scale += 0.001;
            assert_eq!(update_relief(&mut app, &egui::Context::default()), None);
            assert!(old_scene_weak.upgrade().is_none(), "old relief retained at least {old_vertex_bytes} vertex bytes while the next builds");
            let current_scene = &app.view3d.scene.as_ref().unwrap().1;
            let current_vertex_bytes = current_scene.mesh.positions.capacity() * current_scene.mesh.positions.first().map_or(0, std::mem::size_of_val);
            peak_retained_vertex_bytes = peak_retained_vertex_bytes.max(current_vertex_bytes);
        }

        println!(
            "distinct relief scenes: 8; largest old vertex buffer: {largest_old_vertex_buffer} bytes; retained old scene vertex bytes after each new key: 0; peak retained current-scene vertex bytes: {peak_retained_vertex_bytes}"
        );
    }

    #[test]
    fn a_frame_for_another_size_keeps_its_proportions() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(400.0, 200.0));
        assert_eq!(frame_rect(rect, None), rect);
        assert_eq!(frame_rect(rect, Some((400, 200))), rect);

        // Rendered narrower (the view was widened): full height, centred, bars at the sides
        let narrower = frame_rect(rect, Some((300, 200)));
        assert_eq!((narrower.width(), narrower.height(), narrower.center()), (300.0, 200.0, rect.center()));

        // A degenerate size never divides by zero
        assert_eq!(frame_rect(rect, Some((300, 0))), rect);
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn a_view_with_its_own_resolution_is_fitted_and_rendered_at_that_size() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 400.0));
        let view = View3d::default();
        assert_eq!(frame_area(&view, rect), rect);
        assert_eq!(frame_pixels(&view, rect, 2.0), (800, 800));

        // The view's size on screen, scaled
        let scaled = View3d { override_resolution: true, render_scale: 50, ..view.clone() };
        assert_eq!(frame_area(&scaled, rect), rect);
        assert_eq!(frame_pixels(&scaled, rect, 2.0), (400, 400));

        // A resolution of its own: bars above and below a wide frame
        let fixed = View3d { override_resolution: true, viewport_resolution: false, render_resolution: [1920, 1080], render_scale: 50, ..view };
        let area = frame_area(&fixed, rect);
        assert_eq!((area.width(), area.height(), area.center()), (400.0, 225.0, rect.center()));
        assert_eq!(frame_pixels(&fixed, rect, 2.0), (960, 540));

        // A frame drawn while the camera moves is no larger than the view shows it
        assert_eq!(moving_pixels(&fixed, rect, 1.0, 0.5), (200, 112));
        assert_eq!(moving_pixels(&View3d::default(), rect, 1.0, 0.5), (200, 200));
    }

    #[test]
    fn a_retained_fixed_resolution_frame_keeps_its_uploaded_aspect() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 400.0));
        app.session.view3d.override_resolution = true;
        app.session.view3d.viewport_resolution = false;
        app.session.view3d.render_resolution = [1080, 1920];
        let context = egui::Context::default();
        let scene = FrameScene { scene: Arc::new(Scene::builtin("rounded_cube")), view: app.session.view3d.clone(), label: "Rounded cube".into() };
        show_frame(&mut app, &context, Frame { width: 16, height: 9, pixels: vec![0; 16 * 9 * 4], milliseconds: 0.0, scene }, 1, true, (400, 400));

        let retained = shown_rect(&app, rect);
        assert_eq!((retained.width(), retained.height()), (400.0, 225.0));

        let scene = FrameScene { scene: Arc::new(Scene::builtin("rounded_cube")), view: app.session.view3d.clone(), label: "Rounded cube".into() };
        show_frame(&mut app, &context, Frame { width: 9, height: 16, pixels: vec![0; 9 * 16 * 4], milliseconds: 0.0, scene }, 2, true, (400, 400));
        let replacement = shown_rect(&app, rect);
        assert_eq!((replacement.width(), replacement.height()), (225.0, 400.0));
    }

    #[test]
    fn switching_a_helper_on_keeps_the_rendered_frame() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        let key = content_key(&app, (400, 300), (400, 300), None);

        app.session.view3d.show_grid = true;
        app.session.view3d.bounding_box = true;
        app.session.view3d.scene_stats = true;
        assert_eq!(content_key(&app, (400, 300), (400, 300), None), key);

        app.session.view3d.wireframe = true;
        assert_ne!(content_key(&app, (400, 300), (400, 300), None), key, "the wireframe is part of the picture");
    }
}

#[cfg(test)]
mod move_level_tests {
    use super::*;

    #[test]
    fn the_moving_frame_grows_when_cheap_and_shrinks_when_slow() {
        assert_eq!(next_move_level(0, 3.5), 1, "one ray at half size in 3.5 ms leaves room for 2×2 rays");
        assert_eq!(next_move_level(1, 11.0), 2, "2×2 at half size in 11 ms leaves room for three quarters");
        assert_eq!(next_move_level(2, 25.0), 2, "full size would take about 44 ms");
        assert_eq!(next_move_level(2, 3.0), 3);
        assert_eq!(next_move_level(3, 20.0), 3, "the largest level stays");
        assert_eq!(next_move_level(3, 80.0), 2);
        assert_eq!(next_move_level(0, 500.0), 0, "a very slow machine stays at one ray at half size");
        assert_eq!(next_move_level(9, 5.0), 3, "a stale level is clamped");
    }
}
