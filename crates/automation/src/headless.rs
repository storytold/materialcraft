//! The headless backend: a [`Session`] plus file I/O and windowless renders. Synchronous and UI-free; the MCP server, the RPC server and the CLI all drive it

use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

use materialcraft_engine::core::NodeId;
use materialcraft_engine::formats::material::MATERIAL_EXT;
use materialcraft_engine::{Session, commands, file_cmds, inspect, material_cmds};
use materialcraft_render::{Lighting, Maps, Scene, Settings};
use serde_json::{Value, json};

use crate::budgets::{HQ_TRIANGLES, MAX_LOCAL_RENDER_SIDE, MAX_PNG_SIDE, check_png, encode_png};
use crate::workspace::{MAX_READ_BYTES, authorize_command};
use crate::{AutomationError, bad};
pub use crate::{Filesystem, RenderOptions, Rendered};

const MAX_EXPORTED_MATERIAL_BYTES: u64 = 256 << 20;

struct FullCache {
    document_id: u64,
    graph: usize,
    session_revision: u64,
    headless_revision: u64,
    complete: bool,
    cooked: materialcraft_engine::eval::Cooked,
}

pub struct Headless {
    pub session: Session,
    fs: Filesystem,
    scene: Option<(String, Scene)>,
    light: Option<Lighting>,
    revision: u64,
    full_cache: Option<FullCache>,
}

/// Read a file under `fs`'s access rules
fn read_file(fs: &Filesystem, path: &str) -> Result<Vec<u8>, AutomationError> {
    match fs {
        Filesystem::Denied => Err(bad("file access is not granted: start the server with --read-root <dir>")),
        Filesystem::TrustedLocal => read_trusted(path),
        Filesystem::Workspace(w) => w.read(path),
    }
}

/// Read a local file under the same bound as capability workspaces: trusted access widens where a
/// path may point, not how much memory one read may take.
fn read_trusted(path: &str) -> Result<Vec<u8>, AutomationError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        // Reject named pipes without waiting for a writer, even if the path changes during open
        options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
    }
    let file = options.open(path).map_err(|error| AutomationError::Io(format!("cannot read {path}: {error}")))?;
    let metadata = file.metadata().map_err(|e| AutomationError::Io(format!("cannot read {path}: {e}")))?;
    if !metadata.is_file() {
        return Err(bad(format!("not a regular file: `{path}`")));
    }

    if metadata.len() > MAX_READ_BYTES {
        return Err(bad(format!("file too large: `{path}` is {} bytes (limit {MAX_READ_BYTES})", metadata.len())));
    }

    let mut bytes = Vec::new();
    file.take(MAX_READ_BYTES + 1).read_to_end(&mut bytes).map_err(|e| AutomationError::Io(format!("cannot read {path}: {e}")))?;
    if bytes.len() as u64 > MAX_READ_BYTES {
        return Err(bad(format!("file too large: `{path}` exceeds the {MAX_READ_BYTES}-byte limit")));
    }

    Ok(bytes)
}

impl Headless {
    pub fn new(session: Session, fs: Filesystem) -> Headless {
        Headless { session, fs, scene: None, light: None, revision: 0, full_cache: None }
    }

    fn invalidate_full_cache(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.full_cache = None;
    }

    fn full_cache_matches(&self, graph: usize) -> bool {
        self.full_cache.as_ref().is_some_and(|c| {
            c.document_id == self.session.document_id
                && c.graph == graph
                && c.session_revision == self.session.content_revision
                && c.headless_revision == self.revision
        })
    }

    /// The session a fresh server starts with: the atomic sample
    pub fn default_session() -> Session {
        materialcraft_engine::sample_session()
    }

    fn trusted(&self) -> bool {
        matches!(self.fs, Filesystem::TrustedLocal)
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, AutomationError> {
        read_file(&self.fs, path)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), AutomationError> {
        match &self.fs {
            Filesystem::Denied => Err(bad("file access is not granted: start the server with --write-root <dir>")),
            Filesystem::TrustedLocal => materialcraft_engine::formats::write_atomic(Path::new(path), bytes).map_err(|e| AutomationError::Io(e.to_string())),
            Filesystem::Workspace(w) => w.write(path, bytes),
        }
    }

    fn create_dir_all(&self, dir: &str) -> Result<(), AutomationError> {
        match &self.fs {
            _ if dir == "." => Ok(()),
            Filesystem::Denied => Err(bad("file access is not granted: start the server with --write-root <dir>")),
            Filesystem::TrustedLocal => std::fs::create_dir_all(dir).map_err(|e| AutomationError::Io(format!("cannot create {dir}: {e}"))),
            Filesystem::Workspace(w) => w.create_dir_all(dir),
        }
    }

    pub fn inspect(&mut self) -> Value {
        self.session.cook_if_dirty();
        inspect::session(&self.session, None)
    }

    pub fn command_list(&self, filter: Option<&str>) -> Value {
        commands::list(&self.session, filter)
    }

    /// Run an engine command. Untrusted callers can't reach host paths through it
    pub fn command_run(&mut self, id: &str, params: Value) -> Result<Value, AutomationError> {
        if id == "material.cook" {
            return Ok(materialcraft_engine::material_bake::cook(&self.session, &params, |path| {
                self.read(path).map_err(|e| materialcraft_engine::EngineError::Other(e.to_string()))
            })?);
        }
        if !self.trusted() {
            authorize_command(id, &params)?;
        }
        let result = materialcraft_engine::commands::execute_request(&mut self.session, id, &params)?;
        if self.session.dirty {
            self.invalidate_full_cache();
        }
        Ok(result)
    }

    /// Run `[{id, params?}]` in order, stopping at the first error (reported with its index)
    pub fn batch(&mut self, steps: &[Value]) -> Value {
        let mut results = Vec::new();
        for (i, step) in steps.iter().enumerate() {
            let id = step.get("id").and_then(Value::as_str).unwrap_or_default();
            match self.command_run(id, step.get("params").cloned().unwrap_or(Value::Null)) {
                Ok(v) => results.push(v),
                Err(e) => return batch_reply(results, Some((i, id, e.to_string()))),
            }
        }
        batch_reply(results, None)
    }

    /// Open an `.sbs` or `.mcraft` package (or a `.mcmat` material's source), using the granted file access
    pub fn open(&mut self, path: &str) -> Result<Value, AutomationError> {
        self.load(path)?;
        Ok(self.inspect())
    }

    /// Open the document without generating preview images when a caller only needs its interface.
    pub fn load(&mut self, path: &str) -> Result<(), AutomationError> {
        let bytes = self.read(path)?;

        // Workspace save paths stay relative to their granted roots
        let document = self.session.open_packages().into_iter().find(|entry| entry.2 == Some(path)).map(|entry| entry.0);
        if let Some(document) = document {
            self.session.activate_document(document)?;
            self.invalidate_full_cache();
            return Ok(());
        }

        match &self.fs {
            Filesystem::TrustedLocal => file_cmds::open_bytes_linked(&mut self.session, &bytes, Some(path.into()), &file_cmds::linked_files(path))?,
            Filesystem::Workspace(workspace) => {
                let document = workspace.document_path(path)?;
                file_cmds::open_bytes_scoped(&mut self.session, &bytes, &document.to_string_lossy(), &|file| workspace.read_dependency(file))?;
            }
            Filesystem::Denied => return Err(bad("file access is not granted")),
        }
        if self.session.path.is_some() {
            self.session.path = Some(path.to_string());
        }
        self.invalidate_full_cache();

        Ok(())
    }

    /// Save SBS with adjacent resources, or an independent MaterialCraft archive.
    pub fn save(&mut self, path: Option<&str>) -> Result<Value, AutomationError> {
        let path = match path {
            Some(p) => file_cmds::package_path(p),
            None => self.session.path.clone().ok_or_else(|| bad("the package has no file yet (new, or opened from another program): give a `path`"))?,
        };

        let (destination, workspace_write) = match &self.fs {
            Filesystem::Workspace(workspace) => {
                let (destination, writer, save_path) = workspace.save_destination(&path)?;
                (destination.to_string_lossy().into_owned(), Some((writer, save_path)))
            }
            Filesystem::TrustedLocal => (path.clone(), None),
            Filesystem::Denied => return Err(bad("file access is not granted")),
        };
        let files = file_cmds::save_files_at(&self.session, &destination)?;
        let bytes = if file_cmds::is_mcraft(&path) {
            let bytes = files.zip().map_err(|error| bad(error.to_string()))?;
            if let Some((writer, save_path)) = &workspace_write {
                writer.write(save_path, &bytes)?;
            } else {
                self.write(&path, &bytes)?;
            }
            bytes.len()
        } else {
            match &workspace_write {
                None => materialcraft_engine::formats::project::write_files(Path::new(&path), &files).map_err(|error| bad(error.to_string()))?,
                Some((writer, save_path)) => {
                    let parent = save_path.rsplit_once('/').map_or(String::new(), |(parent, _)| format!("{parent}/"));
                    for name in files.files.keys().filter(|name| *name != &files.entry) {
                        writer.check_write(&format!("{parent}{name}"))?;
                    }
                    writer.create_dir_all(&format!("{parent}resources"))?;
                    for (name, bytes) in &files.files {
                        if name == &files.entry {
                            continue;
                        }

                        let target = format!("{parent}{name}");
                        if let Some((directory, _)) = target.rsplit_once('/') {
                            writer.create_dir_all(directory)?;
                        }
                        writer.write_resource(&target, bytes.as_slice())?;
                    }
                    writer.write(save_path, files.xml().map_err(|error| bad(error.to_string()))?)?;
                }
            }
            files.xml().map_err(|error| bad(error.to_string()))?.len()
        };
        self.session.path = Some(path.clone());
        self.session.modified = false;
        Ok(json!({"path": path, "bytes": bytes}))
    }

    /// Embed an image file as a bitmap resource
    pub fn import_bitmap(&mut self, path: &str, name: Option<&str>) -> Result<Value, AutomationError> {
        let bytes = self.read(path)?;
        let file = Path::new(path).file_name().and_then(|f| f.to_str()).unwrap_or("bitmap.png").to_string();
        let name = file_cmds::add_resource(&mut self.session, &file, bytes, name)?;
        self.invalidate_full_cache();
        Ok(json!({"name": name}))
    }

    /// Export every output of a graph into folder `dir`
    pub fn export(&mut self, p: &Value) -> Result<Value, AutomationError> {
        let dir = p.get("dir").and_then(Value::as_str).ok_or_else(|| bad("`dir` is required"))?;
        let (files, mut report) = materialcraft_engine::material_bake::export(&self.session, p, |path| {
            self.read(path).map_err(|e| materialcraft_engine::EngineError::Other(e.to_string()))
        })?;
        self.create_dir_all(dir)?;

        let mut written = Vec::new();
        for exported in files {
            let path = if dir == "." { exported.file.clone() } else { format!("{}/{}", dir.trim_end_matches('/'), exported.file) };
            self.write(&path, &exported.bytes)?;
            written.push(path);
        }
        report["files"] = json!(written);
        Ok(report)
    }

    /// Export a portable material below the granted write root
    pub fn export_material(&mut self, p: &Value) -> Result<Value, AutomationError> {
        let path = p.get("path").and_then(Value::as_str).ok_or_else(|| bad("`path` is required"))?;
        let path = if path.to_ascii_lowercase().ends_with(&format!(".{MATERIAL_EXT}")) { path.to_string() } else { format!("{path}.{MATERIAL_EXT}") };
        match &self.fs {
            Filesystem::Denied => return Err(bad("file access is not granted: start the server with --write-root <dir>")),
            Filesystem::Workspace(w) => w.check_write(&path)?,
            Filesystem::TrustedLocal => {}
        }

        let gi = material_cmds::publish_graph(&self.session, p)?;
        let bytes = if self.trusted() {
            material_cmds::material_bytes(&self.session, gi)?
        } else {
            material_cmds::material_bytes_with_resource_limit(&self.session, gi, MAX_EXPORTED_MATERIAL_BYTES)?
        };
        if bytes.len() as u64 > MAX_EXPORTED_MATERIAL_BYTES {
            return Err(bad(format!("the material is {} bytes (limit {MAX_EXPORTED_MATERIAL_BYTES}); reduce source asset sizes", bytes.len())));
        }

        self.write(&path, &bytes)?;
        Ok(json!({"path": path, "bytes": bytes.len()}))
    }

    /// Render the assigned material or a node output as PNG
    pub fn render_png(&mut self, o: &RenderOptions) -> Result<Rendered, AutomationError> {
        self.render_png_with_cancel(o, None)
    }

    /// Render the assigned material or a node output as PNG, honouring an optional cancellation flag.
    pub fn render_png_with_cancel(&mut self, o: &RenderOptions, cancel: Option<&std::sync::atomic::AtomicBool>) -> Result<Rendered, AutomationError> {
        if is_cancelled(cancel) {
            return Err(AutomationError::Other("render cancelled".into()));
        }
        let max = if self.trusted() { MAX_LOCAL_RENDER_SIDE } else { MAX_PNG_SIDE };
        let view = o.view.as_deref().unwrap_or("3d");
        let out = match view {
            "2d" => {
                if o.full {
                    self.render_2d_full(o, max, cancel)
                } else {
                    self.session.cook_if_dirty();
                    self.render_2d_from(&self.session.cooked, o, max)
                }
            }
            "3d" => {
                if o.full {
                    self.render_3d_full(o, max, cancel)
                } else {
                    self.session.cook_if_dirty();
                    Ok(self.render_3d(o, max))
                }
            }
            other => Err(bad(format!("unknown view `{other}` (3d or 2d)"))),
        };
        let (w, h, rgba) = out?;
        let png = encode_png(w, h, rgba)?;
        check_png(png.len())?;
        Ok(Rendered { width: w, height: h, png })
    }

    fn resolve_2d_target(&self, options: &RenderOptions) -> Result<(NodeId, usize), AutomationError> {
        if let Some(n) = options.node {
            return Ok((NodeId(n), options.port.unwrap_or(0)));
        }
        let g = self.session.graph()?;
        let id = self
            .session
            .view2d
            .node
            .filter(|id| g.node(*id).is_ok())
            .or_else(|| self.session.selection.last().copied())
            .or_else(|| g.nodes.iter().find(|n| n.def == "output").map(|n| n.id))
            .ok_or_else(|| bad("nothing to show: the graph has no outputs; pass `node`"))?;
        let port = options.port.unwrap_or_else(|| if Some(id) == self.session.view2d.node { self.session.view2d.port } else { 0 });
        Ok((id, port))
    }

    fn render_2d_full(
        &mut self,
        options: &RenderOptions,
        maximum_side: u32,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<(u32, u32, Vec<u8>), AutomationError> {
        let (target_id, port) = self.resolve_2d_target(options)?;
        let doc_id = self.session.document_id;
        let graph = self.session.graph;
        let s_rev = self.session.content_revision;
        let rev = self.revision;
        let cached_hit = self.full_cache_matches(graph) && self.full_cache.as_ref().is_some_and(|c| c.cooked.image(target_id, port).is_some());
        if !cached_hit {
            let partial = self.session.cook_full_with(graph, Some(vec![target_id]), cancel);
            if partial.cancelled || is_cancelled(cancel) {
                return Err(AutomationError::Other("render cancelled".into()));
            }
            if self.full_cache_matches(graph)
                && let Some(entry) = self.full_cache.as_mut()
            {
                entry.cooked.nodes.extend(partial.nodes);
            } else {
                self.full_cache =
                    Some(FullCache { document_id: doc_id, graph, session_revision: s_rev, headless_revision: rev, complete: false, cooked: partial });
            }
        }
        let cooked = self.full_cache.as_ref().map(|c| &c.cooked).ok_or_else(|| bad("full cook cache unavailable"))?;
        let image =
            cooked.image(target_id, port).or_else(|| cooked.image(target_id, 0)).ok_or_else(|| bad(format!("node {} has no cooked output", target_id.0)))?;
        render_2d_image(image, options, maximum_side)
    }

    fn render_2d_from(
        &self,
        cooked: &materialcraft_engine::eval::Cooked,
        options: &RenderOptions,
        maximum_side: u32,
    ) -> Result<(u32, u32, Vec<u8>), AutomationError> {
        let (target_id, port) = self.resolve_2d_target(options)?;
        let image =
            cooked.image(target_id, port).or_else(|| cooked.image(target_id, 0)).ok_or_else(|| bad(format!("node {} has no cooked output", target_id.0)))?;
        render_2d_image(image, options, maximum_side)
    }

    fn render_3d_full(
        &mut self,
        options: &RenderOptions,
        maximum_side: u32,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<(u32, u32, Vec<u8>), AutomationError> {
        let doc_id = self.session.document_id;
        let graph = self.session.graph;
        let s_rev = self.session.content_revision;
        let rev = self.revision;
        let cached_hit = self.full_cache_matches(graph) && self.full_cache.as_ref().is_some_and(|c| c.complete);
        if !cached_hit {
            let full = self.session.cook_full_with(graph, None, cancel);
            if full.cancelled || is_cancelled(cancel) {
                return Err(AutomationError::Other("render cancelled".into()));
            }
            self.full_cache = Some(FullCache { document_id: doc_id, graph, session_revision: s_rev, headless_revision: rev, complete: true, cooked: full });
        }
        let sources = self.session.material_preview.sources(&self.session.pkg);
        let mut cross_graph_nodes: std::collections::BTreeMap<usize, Vec<NodeId>> = std::collections::BTreeMap::new();
        for source in sources.values() {
            if source.graph != graph {
                let nodes = cross_graph_nodes.entry(source.graph).or_default();
                if !nodes.contains(&source.node) {
                    nodes.push(source.node);
                }
            }
        }
        let mut extra_cooked = std::collections::BTreeMap::new();
        for (other_graph, nodes) in cross_graph_nodes {
            let cooked = self.session.cook_full_with(other_graph, Some(nodes), cancel);
            if cooked.cancelled || is_cancelled(cancel) {
                return Err(AutomationError::Other("render cancelled".into()));
            }
            extra_cooked.insert(other_graph, cooked);
        }
        let maps = {
            let cooked = self.full_cache.as_ref().map(|c| &c.cooked).ok_or_else(|| bad("full cook cache unavailable"))?;
            let mut out = Vec::with_capacity(sources.len());
            for (channel, source) in sources {
                let port = source.port_index(&self.session.pkg).unwrap_or(0);
                let graph_cooked = if source.graph == graph { Some(cooked) } else { extra_cooked.get(&source.graph) };
                let Some(graph_cooked) = graph_cooked else {
                    return Err(bad(format!("channel `{}` references unavailable graph {}", channel.id(), source.graph)));
                };
                if let Some(err) = graph_cooked.nodes.get(&source.node).and_then(|r| r.error.as_deref()) {
                    return Err(bad(format!("channel `{}` failed to cook: {err}", channel.id())));
                }
                if let Some(img) = graph_cooked.image(source.node, port).or_else(|| graph_cooked.image(source.node, 0)) {
                    out.push((channel.id(), img.clone()));
                }
            }
            out
        };
        Ok(self.render_3d_maps(options, maximum_side, maps.iter().map(|(u, img)| (*u, img)).collect()))
    }

    fn render_3d(&mut self, o: &RenderOptions, max: u32) -> (u32, u32, Vec<u8>) {
        let maps = self.session.material_maps().into_iter().map(|(k, img)| (k, img.clone())).collect::<Vec<_>>();
        self.render_3d_maps(o, max, maps.iter().map(|(k, img)| (*k, img)).collect())
    }

    fn render_3d_maps(&mut self, o: &RenderOptions, max: u32, raw_maps: Vec<(&str, &materialcraft_engine::image::Image)>) -> (u32, u32, Vec<u8>) {
        let v = &self.session.rendered_view3d();
        let scene_id = o.scene.clone().unwrap_or_else(|| v.scene.clone());
        let custom = self.session.custom_mesh.clone().filter(|_| scene_id == "custom");
        let key = custom.as_ref().map_or_else(|| scene_id.clone(), |(name, _)| format!("custom:{name}:{}", self.session.custom_mesh_generation));

        if self.scene.as_ref().is_none_or(|(id, _)| *id != key) {
            let scene = match custom {
                Some((_, mesh)) => Scene::new((*mesh).clone()),
                None => Scene::builtin(&scene_id),
            };

            self.scene = Some((key, scene));
        }

        if self.light.as_ref().is_none_or(|l| l.name != v.environment) {
            self.light = Some(Lighting::new(&v.environment));
        }

        let tiling = o.tiling.unwrap_or(v.tiling).clamp(0.1, 64.0);
        let depth = o.displacement.unwrap_or(v.height_scale).clamp(0.0, 0.5);
        let view = materialcraft_engine::View3d { yaw: o.yaw.unwrap_or(v.yaw), pitch: o.pitch.unwrap_or(v.pitch), tiling, height_scale: depth, ..v.clone() };
        let mut settings = Settings { supersample: view.samples.max(2), ..Settings::for_view(&view) };

        let (w, h) = (o.width.unwrap_or(1024).clamp(16, max), o.height.unwrap_or(768).clamp(16, max));
        let maps = Maps::from_usages(raw_maps);
        let (Some((_, scene)), Some(light)) = (&self.scene, &self.light) else { return (w, h, vec![0; (w * h * 4) as usize]) };
        let triangles = if o.hq { HQ_TRIANGLES } else { materialcraft_render::relief_triangles(&view) };
        let displaced = materialcraft_render::raised(scene, &maps, &view, triangles);

        if o.hq {
            settings.supersample = 3;
            settings.ao_rays = 8;
            settings.key_light = 2.5;
        }
        (w, h, materialcraft_render::render(displaced.as_ref().unwrap_or(scene), &maps, light, &settings, w, h))
    }
}

fn is_cancelled(cancel: Option<&std::sync::atomic::AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
}

fn render_2d_image(image: &materialcraft_engine::image::Image, options: &RenderOptions, maximum_side: u32) -> Result<(u32, u32, Vec<u8>), AutomationError> {
    let tiling = options.tiling.unwrap_or(1.0).clamp(0.1, 64.0);
    let has_tiling = (tiling - 1.0).abs() > 1e-6;
    if let Some([cx, cy, cw, ch]) = options.crop {
        if !cx.is_finite() || !cy.is_finite() || !cw.is_finite() || !ch.is_finite() || cw <= 0.0 || ch <= 0.0 {
            return Err(bad("`crop` must be [x, y, width, height] with positive width and height"));
        }
        let default_w = (image.width() as f32 * cw).round().max(16.0) as u32;
        let default_h = (image.height() as f32 * ch).round().max(16.0) as u32;
        let (w, h) = (options.width.unwrap_or(default_w).clamp(16, maximum_side), options.height.unwrap_or(default_h).clamp(16, maximum_side));
        let cropped =
            materialcraft_engine::image::Image::generate_uv(w, h, image.kind(), |u, v| image.sample_stored(cx + u * cw * tiling, cy + v * ch * tiling, true));
        return Ok((w, h, cropped.to_rgba8_display()));
    }
    let (w, h) = (options.width.unwrap_or(image.width()).clamp(16, maximum_side), options.height.unwrap_or(image.height()).clamp(16, maximum_side));
    let out = if has_tiling {
        materialcraft_engine::image::Image::generate_uv(w, h, image.kind(), |u, v| image.sample_stored(u * tiling, v * tiling, true))
    } else {
        image.resize(w, h, true)
    };
    Ok((w, h, out.to_rgba8_display()))
}

pub fn batch_reply(results: Vec<Value>, failed: Option<(usize, &str, String)>) -> Value {
    json!({
        "completed": results.len(),
        "results": results,
        "failed": failed.map(|(index, id, error)| json!({ "index": index, "id": id, "error": error })),
    })
}

#[cfg(test)]
#[path = "headless.test.rs"]
mod tests;
