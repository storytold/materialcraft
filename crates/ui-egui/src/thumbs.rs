//! The app's thumbnail sources: node faces, library previews and template pictures, each over its own framework
//! `Thumbnails` store kept in the context.

use std::collections::BTreeMap;
use std::sync::Arc;

use craft_host::queue::Cancel;
use craft_ui::primitives::{LoadFailure, LoadedPicture, Revision, ThumbRender, ThumbRequest, ThumbSource, ThumbnailOptions, Thumbnails};
use craft_ui::shell::ThumbKey;
use egui::ColorImage;
use materialcraft_engine::NodeLibrary;
use materialcraft_engine::image::{Image, Kind};
use materialcraft_engine::nodes::{CookContext, catalog, resolve_params};

/// The persistence and clear epoch of the app's picture stores; the stores themselves live in the context, so the
/// app builds this without one.
#[derive(Default)]
pub struct Thumbs {
    #[cfg(not(target_arch = "wasm32"))]
    persistence: Option<preview_cache_settings::PreviewCacheMaintenance>,
    clear_epoch: u64,
}

/// An image as an egui texture image (sRGB display, `side` px)
pub fn color_image(img: &Image, side: u32) -> ColorImage {
    let small = img.resize(side, side, true);
    ColorImage::from_rgba_unmultiplied([side as usize, side as usize], &small.to_rgba8_display())
}

#[cfg(not(target_arch = "wasm32"))]
#[path = "preview_cache_settings.rs"]
mod preview_cache_settings;
#[cfg(not(target_arch = "wasm32"))]
#[path = "preview_disk_cache.rs"]
mod preview_disk_cache;

/// The key of a node output's picture: the cook hash and the output port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeThumb {
    /// The cook result's key.
    pub cook: u64,
    /// The output port.
    pub port: usize,
}

/// A node output's image as a thumbnail source: it renders `ThumbRender::Ready` on the UI thread.
pub struct NodeFace<'a> {
    /// The cooked output.
    pub image: &'a Image,
}

impl ThumbSource<NodeThumb> for NodeFace<'_> {
    // The key is the cook hash of the pixels, so one key never changes picture
    fn revision(&self, _key: &NodeThumb, _request: &ThumbRequest) -> Revision {
        Revision::new(0)
    }

    fn render(&self, _key: &NodeThumb, request: &ThumbRequest) -> ThumbRender {
        ThumbRender::Ready(color_image(self.image, request.side))
    }
}

/// The key of a library picture: a node's preview or a library graph's.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LibraryThumb {
    /// The preview of the node definition with this id.
    Node(String),
    /// The preview of a graph of a package in the library.
    Graph {
        /// The package.
        package: String,
        /// The graph.
        graph: String,
    },
}

/// Renders the library's pictures in the background.
pub struct LibrarySource {
    library: Arc<NodeLibrary>,
    generation: u64,
    #[cfg(not(target_arch = "wasm32"))]
    persistence: Option<preview_cache_settings::PreviewCacheSettings>,
}

impl ThumbSource<LibraryThumb> for LibrarySource {
    fn revision(&self, key: &LibraryThumb, _request: &ThumbRequest) -> Revision {
        match key {
            // Native node previews come from the built-in catalog, which a library reload does not change
            LibraryThumb::Node(_) => Revision::new(0),
            LibraryThumb::Graph { .. } => Revision::new(self.generation),
        }
    }

    fn render(&self, key: &LibraryThumb, request: &ThumbRequest) -> ThumbRender {
        // The browser has no library files to cook graphs from
        #[cfg(target_arch = "wasm32")]
        if matches!(key, LibraryThumb::Graph { .. }) {
            return ThumbRender::Blank;
        }

        let (library, key, side) = (Arc::clone(&self.library), key.clone(), request.side);
        #[cfg(not(target_arch = "wasm32"))]
        let persistence = self.persistence.clone();
        ThumbRender::Later(Box::new(move |cancel: &Cancel| {
            #[cfg(not(target_arch = "wasm32"))]
            let rgba = library_pixels(&library, &key, side, cancel, persistence);
            #[cfg(target_arch = "wasm32")]
            let rgba = library_render(&library, &key, side, cancel);
            Ok(rgba.map_or(LoadedPicture::Blank, |rgba| LoadedPicture::Image(ColorImage::from_rgba_unmultiplied([side as usize; 2], &rgba))))
        }))
    }
}

/// Renders `key` at `side`, through the disk cache when persistence is on.
#[cfg(not(target_arch = "wasm32"))]
fn library_pixels(
    library: &Arc<NodeLibrary>,
    key: &LibraryThumb,
    side: u32,
    cancel: &Cancel,
    persistence: Option<preview_cache_settings::PreviewCacheSettings>,
) -> Option<Vec<u8>> {
    let disk = persistence.and_then(preview_disk_cache::PreviewDiskCache::configured);
    let fingerprint = disk.as_ref().filter(|disk| disk.enabled()).and_then(|_| match key {
        LibraryThumb::Node(id) => {
            use sha2::Digest;
            Some(sha2::Sha256::digest(format!("materialcraft-atomic-preview-v1:{id}")).into())
        }
        LibraryThumb::Graph { package, graph } => library.preview_fingerprint(package, graph, cancel.flag()),
    });
    match (disk, fingerprint) {
        (Some(disk), Some(fingerprint)) => disk.get_or_render(fingerprint, side, cancel, || library_render(library, key, side, cancel)),
        _ => library_render(library, key, side, cancel),
    }
}

/// Cooks `key`'s picture at `side`: display sRGB, straight alpha; `None` when cancelled or it makes no image.
fn library_render(library: &Arc<NodeLibrary>, key: &LibraryThumb, side: u32, cancel: &Cancel) -> Option<Vec<u8>> {
    if cancel.is_cancelled() {
        return None;
    }

    match key {
        LibraryThumb::Node(id) => render_native(id, side),
        LibraryThumb::Graph { package, graph } => {
            NodeLibrary::preview_cancellable(library, package, graph, side, cancel.flag()).map(|image| image.resize(side, side, true).to_rgba8_display())
        }
    }
}

/// The one source of template pictures, for Home and the New graph dialog alike; the function template is blank.
pub struct TemplateSource;

impl ThumbSource<ThumbKey> for TemplateSource {
    // Templates are built into the app, so their pictures never change within a run
    fn revision(&self, _key: &ThumbKey, _request: &ThumbRequest) -> Revision {
        Revision::new(0)
    }

    fn render(&self, key: &ThumbKey, request: &ThumbRequest) -> ThumbRender {
        match key {
            ThumbKey::Template(id) if id != crate::template_card::FUNCTION_TEMPLATE => {
                let (id, side) = (id.clone(), request.side);
                ThumbRender::Later(Box::new(move |cancel: &Cancel| render_template(&id, side, cancel)))
            }
            _ => ThumbRender::Blank,
        }
    }
}

/// Cooks template `id` and renders it on the studio sphere at `side`.
fn render_template(id: &str, side: u32, cancel: &Cancel) -> Result<LoadedPicture, LoadFailure> {
    let light = materialcraft_render::Lighting::new("studio");
    let scene = materialcraft_render::Scene::builtin("sphere");
    let settings = materialcraft_render::Settings { distance: 2.7, supersample: 2, ..Default::default() };
    let mut session = materialcraft_engine::Session::new(materialcraft_engine::core::Package::new("t"));
    session.pkg.graphs.push(materialcraft_engine::samples::template(id, "t", 8).map_err(|_| LoadFailure::Missing)?);
    session.material_preview = materialcraft_engine::MaterialPreview::for_graph(&session.pkg, 0);
    session.preview_size = Some(256);
    session.cook();
    let maps = materialcraft_render::Maps::from_usages(session.material_maps());
    let rgba = materialcraft_render::render_cancellable(&scene, &maps, &light, &settings, side, side, cancel.flag()).ok_or(LoadFailure::Missing)?;
    Ok(LoadedPicture::Image(ColorImage::from_rgba_unmultiplied([side as usize; 2], &rgba)))
}

impl Thumbs {
    /// The store of node output pictures, kept in the context.
    pub fn nodes(&self, ctx: &egui::Context) -> Thumbnails<NodeThumb> {
        Thumbnails::in_context(ctx, "materialcraft-node-thumbs", ThumbnailOptions::new().max_side(96))
    }

    /// The store of the library's pictures, kept in the context: one render at a time, so the foreground cook keeps
    /// the other cores.
    pub fn library_thumbs(&self, ctx: &egui::Context) -> Thumbnails<LibraryThumb> {
        // The browser has no threads; zero lanes runs one job a frame inline
        let lanes = if cfg!(target_arch = "wasm32") { 0 } else { 1 };
        Thumbnails::in_context(ctx, "materialcraft-library-thumbs", ThumbnailOptions::new().max_side(80).lanes(lanes))
    }

    /// The store of template pictures, kept in the context.
    pub fn templates(&self, ctx: &egui::Context) -> Thumbnails<ThumbKey> {
        Thumbnails::in_context(ctx, "materialcraft-template-thumbs", ThumbnailOptions::new().max_side(96))
    }

    /// The source that renders the library's pictures from `library`; `generation` is the session's
    /// `library_generation`, so a reload re-renders graph pictures while the old ones stand in.
    // PLAN: the K2 decision keys graph pictures on `library_generation`, which the library alone does not carry; callers (panels-w7 `library.rs`, shell-w11 `shortcuts_page.rs`) pass `app.session.library_generation()` as the second argument
    pub fn library_source(&self, library: &Arc<NodeLibrary>, generation: u64) -> LibrarySource {
        LibrarySource {
            library: Arc::clone(library),
            generation,
            #[cfg(not(target_arch = "wasm32"))]
            persistence: self.persistence.as_ref().map(|persistence| persistence.settings.clone()),
        }
    }

    /// Once per frame from `step`: library pictures hold while `busy` or while a cache clear is pending. Returns a
    /// cache error to report. Reads no input, so a second pass in the frame sets the same hold.
    pub fn frame(&mut self, ctx: &egui::Context, busy: bool) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        let clear_pending = self.persistence.as_ref().is_some_and(|persistence| persistence.settings.get().clear_pending);
        #[cfg(target_arch = "wasm32")]
        let clear_pending = false;
        self.library_thumbs(ctx).hold(busy || clear_pending);

        #[cfg(target_arch = "wasm32")]
        return None;
        #[cfg(not(target_arch = "wasm32"))]
        self.persistence.as_mut().and_then(|persistence| persistence.tick(ctx))
    }

    /// Opt in only for a native app window; offscreen harnesses never use the user's cache.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_persistence(&mut self, prefs: materialcraft_engine::LibraryCachePrefs) {
        self.persistence = Some(preview_cache_settings::PreviewCacheMaintenance::new(prefs));
    }

    /// Applies the cache preferences; a new `clear_epoch` drops every library picture so they render again.
    pub fn configure_cache(&mut self, ctx: &egui::Context, _prefs: materialcraft_engine::LibraryCachePrefs, clear_epoch: u64) {
        let clear = clear_epoch != self.clear_epoch;
        self.clear_epoch = clear_epoch;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(persistence) = &self.persistence {
            persistence.settings.update(_prefs.into(), clear);
        }

        if clear {
            self.library_thumbs(ctx).cache().clear(ctx);
        }
    }

    /// Drops every node output picture.
    pub fn clear_nodes(&self, ctx: &egui::Context) {
        self.nodes(ctx).cache().clear(ctx);
    }
}

/// Wave height used by native node previews
fn sample_height(side: u32) -> Image {
    Image::generate_uv(side, side, Kind::Gray, |u, v| {
        [0.5 + 0.25 * (u * std::f32::consts::TAU).sin() + 0.25 * (v * std::f32::consts::TAU).cos(), 0.0, 0.0, 0.0]
    })
}

fn render_native(id: &str, side: u32) -> Option<Vec<u8>> {
    // Strong relief keeps normal inputs legible in small previews
    const NORMAL_INTENSITY: f32 = 4.0;

    let def = catalog().iter().find(|def| def.id == id && def.id != "graph_instance")?;
    let gray = (!def.inputs.is_empty()).then(|| sample_height(side));
    let mut color = None;
    let mut normal = None;
    let no_resource = |_: &str| None;
    let variables = BTreeMap::new();
    let inputs: Vec<Option<Image>> = def
        .inputs
        .iter()
        .enumerate()
        .map(|(index, port)| {
            // Masks and secondary inputs stay empty so the main effect shows
            if index > 0 && !matches!(def.id, "blend" | "height_blend" | "normal_blend" | "rgba_merge") {
                return None;
            }

            let gray = gray.as_ref()?;
            Some(if port.id.contains("normal") || def.category.contains("Normal") && port.kind == materialcraft_engine::nodes::PortKind::Color {
                normal.get_or_insert_with(|| materialcraft_engine::nodes::atomic::height_to_normal(gray, NORMAL_INTENSITY, false, true, false)).clone()
            } else if port.kind == materialcraft_engine::nodes::PortKind::Color {
                // A blue-to-sand palette distinguishes color inputs from the grayscale height
                color.get_or_insert_with(|| gray.map(Kind::Color, |pixel| [0.15 + 0.7 * pixel[0], 0.35 + 0.3 * pixel[0], 0.8 - 0.5 * pixel[0], 1.0])).clone()
            } else if index == 1 {
                gray.map(Kind::Gray, |v| [1.0 - v[0], 0.0, 0.0, 0.0])
            } else {
                gray.clone()
            })
        })
        .collect();

    let params = resolve_params(def, &BTreeMap::new());
    let ctx = CookContext {
        format: materialcraft_engine::core::OutputFormat::U16,
        w: side,
        h: side,
        seed: 3,
        tile: true,
        inputs: &inputs,
        params: &params,
        resource: &no_resource,
        vars: &variables,
    };

    let images = (def.cook)(&ctx).ok()?;
    Some(images.first()?.resize(side, side, true).to_rgba8_display())
}
