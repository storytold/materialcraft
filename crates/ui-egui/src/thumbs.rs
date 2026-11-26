use std::collections::{BTreeMap, HashMap};

use egui::{ColorImage, TextureHandle, TextureOptions};
use materialcraft_engine::image::{Image, Kind};
use materialcraft_engine::nodes::{CookContext, catalog, resolve_params};

/// Rendered previews by id: (id, RGBA8 pixels).
pub type Previews = Vec<(String, Vec<u8>)>;

/// Side of node thumbnails and library previews, in pixels
pub const THUMB: u32 = 96;
const LIBRARY_PREVIEW_SIZE: u32 = 72;
const NODE_TEXTURE_TRIM_THRESHOLD: usize = 256;
const NODE_TEXTURE_IDLE_FRAMES: u64 = 120;

#[derive(Default)]
pub struct Thumbs {
    /// (cook key, port) → texture and the frame it was last used
    nodes: HashMap<(u64, usize), (TextureHandle, u64)>,
    #[cfg(not(target_arch = "wasm32"))]
    persistence: Option<preview_cache_settings::PreviewCacheMaintenance>,
    clear_epoch: u64,
    graphs: GraphPreviews,
    frame: u64,
}

/// An image as an egui texture image (sRGB display, `side` px)
pub fn color_image(img: &Image, side: u32) -> ColorImage {
    let small = img.resize(side, side, true);
    ColorImage::from_rgba_unmultiplied([side as usize, side as usize], &small.to_rgba8_display())
}

#[path = "graph_previews.rs"]
mod graph_previews;
use graph_previews::GraphPreviews;
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

impl craft_ui::primitives::ThumbSource<NodeThumb> for NodeFace<'_> {
    fn revision(&self, key: &NodeThumb, request: &craft_ui::primitives::ThumbRequest) -> craft_ui::primitives::Revision {
        let _ = (key, request);
        todo!("sh.A12")
    }

    fn render(&self, key: &NodeThumb, request: &craft_ui::primitives::ThumbRequest) -> craft_ui::primitives::ThumbRender {
        let _ = (key, request);
        todo!("sh.A12")
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
    library: std::sync::Arc<materialcraft_engine::NodeLibrary>,
}

impl craft_ui::primitives::ThumbSource<LibraryThumb> for LibrarySource {
    fn revision(&self, key: &LibraryThumb, request: &craft_ui::primitives::ThumbRequest) -> craft_ui::primitives::Revision {
        let _ = (&self.library, key, request);
        todo!("sh.A12")
    }

    fn render(&self, key: &LibraryThumb, request: &craft_ui::primitives::ThumbRequest) -> craft_ui::primitives::ThumbRender {
        let _ = (key, request);
        todo!("sh.A12")
    }
}

/// The one source of template pictures, for Home and the New graph dialog alike; the function template is blank.
pub struct TemplateSource;

impl craft_ui::primitives::ThumbSource<craft_ui::shell::ThumbKey> for TemplateSource {
    fn revision(&self, key: &craft_ui::shell::ThumbKey, request: &craft_ui::primitives::ThumbRequest) -> craft_ui::primitives::Revision {
        let _ = (key, request);
        todo!("sh.A12")
    }

    fn render(&self, key: &craft_ui::shell::ThumbKey, request: &craft_ui::primitives::ThumbRequest) -> craft_ui::primitives::ThumbRender {
        let _ = (key, request);
        todo!("sh.A12")
    }
}

#[allow(dead_code)] // P3(shell-w15): sh.A12
impl Thumbs {
    /// The store of node output pictures, kept in the context.
    pub fn nodes(&self, ctx: &egui::Context) -> craft_ui::primitives::Thumbnails<NodeThumb> {
        let _ = ctx;
        todo!("sh.A12")
    }

    /// The store of the library's pictures, kept in the context.
    pub fn templates(&self, ctx: &egui::Context) -> craft_ui::primitives::Thumbnails<craft_ui::shell::ThumbKey> {
        let _ = ctx;
        todo!("sh.A12")
    }

    /// The source that renders the library's pictures from `library`.
    pub fn library_source(&self, library: &std::sync::Arc<materialcraft_engine::NodeLibrary>) -> LibrarySource {
        let _ = library;
        todo!("sh.A12")
    }

    /// Once per frame from `step`: advances the stores; library pictures hold while `busy`. Returns a cache error to report.
    pub fn frame(&mut self, ctx: &egui::Context, busy: bool) -> Option<String> {
        let _ = (ctx, busy);
        todo!("sh.A12")
    }
}

impl Thumbs {
    /// Opt in only for a native app window; offscreen harnesses never use the user's cache.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_persistence(&mut self, prefs: materialcraft_engine::LibraryCachePrefs) {
        let persistence = preview_cache_settings::PreviewCacheMaintenance::new(prefs);
        self.graphs.persistence = Some(persistence.settings.clone());
        self.persistence = Some(persistence);
    }

    pub fn configure_cache(&mut self, _prefs: materialcraft_engine::LibraryCachePrefs, clear_epoch: u64) {
        let clear = clear_epoch != self.clear_epoch;
        self.clear_epoch = clear_epoch;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(persistence) = &self.persistence {
            persistence.settings.update(_prefs.into(), clear);
        }

        if clear {
            self.graphs.clear();
        }
    }

    pub fn clear_nodes(&mut self) {
        self.nodes.clear();
    }

    /// The thumbnail of a node output, uploaded on first use
    pub fn node(&mut self, ctx: &egui::Context, key: u64, port: usize, img: &Image) -> egui::TextureId {
        let frame = self.frame;
        if let Some((texture, last_used_frame)) = self.nodes.get_mut(&(key, port)) {
            *last_used_frame = frame;
            return texture.id();
        }

        let texture = ctx.load_texture(format!("node-{key}-{port}"), color_image(img, THUMB), TextureOptions::LINEAR);
        let id = texture.id();
        self.nodes.insert((key, port), (texture, frame));
        id
    }

    /// Once per frame: forget thumbnails unused for a few seconds; collect library previews
    pub fn tick(&mut self, ctx: &egui::Context, library: &std::sync::Arc<materialcraft_engine::NodeLibrary>, foreground_busy: bool) -> Option<String> {
        self.frame += 1;
        let frame = self.frame;
        if self.nodes.len() > NODE_TEXTURE_TRIM_THRESHOLD {
            self.nodes.retain(|_, (_, used)| frame - *used < NODE_TEXTURE_IDLE_FRAMES);
        }

        #[cfg(not(target_arch = "wasm32"))]
        let error = self.persistence.as_mut().and_then(|persistence| persistence.tick(ctx));
        #[cfg(target_arch = "wasm32")]
        let error = None;
        self.graphs.collect(ctx, library, foreground_busy);
        error
    }

    /// The preview of library graph `graph` of `package`, cooked in the background on first use;
    /// `None` until it is ready and for graphs that make no image
    pub fn graph(
        &mut self,
        ctx: &egui::Context,
        library: &std::sync::Arc<materialcraft_engine::NodeLibrary>,
        package: &str,
        graph: &str,
    ) -> Option<egui::TextureId> {
        self.graphs.get(ctx, library, package, graph)
    }

    /// The library preview of node `def` (generated in the background on first use)
    pub fn library(&mut self, ctx: &egui::Context, def: &str) -> Option<egui::TextureId> {
        self.graphs.native(ctx, def)
    }
}

/// Wave height used by native node previews
fn sample_height() -> Image {
    Image::generate_uv(LIBRARY_PREVIEW_SIZE, LIBRARY_PREVIEW_SIZE, Kind::Gray, |u, v| {
        [0.5 + 0.25 * (u * std::f32::consts::TAU).sin() + 0.25 * (v * std::f32::consts::TAU).cos(), 0.0, 0.0, 0.0]
    })
}

fn render_native(id: &str) -> Option<Vec<u8>> {
    // Strong relief keeps normal inputs legible in small previews
    const NORMAL_INTENSITY: f32 = 4.0;

    let def = catalog().iter().find(|def| def.id == id && def.id != "graph_instance")?;
    let gray = (!def.inputs.is_empty()).then(sample_height);
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
        w: LIBRARY_PREVIEW_SIZE,
        h: LIBRARY_PREVIEW_SIZE,
        seed: 3,
        tile: true,
        inputs: &inputs,
        params: &params,
        resource: &no_resource,
        vars: &variables,
    };

    let images = (def.cook)(&ctx).ok()?;
    Some(images.first()?.resize(LIBRARY_PREVIEW_SIZE, LIBRARY_PREVIEW_SIZE, true).to_rgba8_display())
}
