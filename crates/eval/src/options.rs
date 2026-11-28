use craft_texture::Image;
use materialcraft_core::{NodeId, OutputFormat, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::AtomicBool;

/// What to cook and how
#[derive(Clone)]
pub struct Options<'a> {
    pub environment: materialcraft_core::EvaluationEnvironment,
    pub parent_tiling: Option<[bool; 2]>,
    pub parent_pixel_ratio: Option<i32>,
    pub parent_base: Option<crate::ResolvedBaseParameters>,
    /// Cap on the larger side of every node's output (previews); `None` = full size
    pub max_size: Option<u32>,
    /// Cook only these nodes and what they depend on (`None` = the whole graph)
    pub only: Option<Vec<NodeId>>,
    /// Images for the graph's input nodes, by identifier (graph instances)
    pub inputs: BTreeMap<String, Image>,
    /// Upstream storage precision for caller-supplied images. Unspecified floating inputs use full precision.
    pub input_formats: BTreeMap<String, OutputFormat>,
    /// Resolved source properties for each image wired to a graph instance.
    pub input_bases: BTreeMap<String, crate::ResolvedBaseParameters>,
    /// Caller textures which have only their base level when passed through an instance.
    pub base_level_inputs: BTreeSet<String>,
    /// Previously accepted samples for this editor graph. Full cooks start without editor state.
    pub previous_sampling: Option<&'a crate::PreviewSampling>,
    pub(crate) preview_input_keys: BTreeMap<String, u64>,
    /// Original storage of decoded external files. Wired graph inputs use their upstream format.
    pub bitmap_formats: BTreeMap<String, OutputFormat>,
    /// Values for the graph's exposed parameters (graph instances), over the graph's own
    pub overrides: BTreeMap<String, Value>,
    /// Exposed inputs whose fixed instance arguments can be folded during function evaluation.
    pub literal_inputs: Vec<String>,
    /// Decodes a package resource (bitmap) by name
    pub resource: &'a crate::ResourceDecoder<'a>,
    /// Set from another thread to stop between nodes
    pub cancel: Option<&'a AtomicBool>,
    pub depth: u32,
    /// Graph addresses being cooked around this one, outermost first (cycle detection)
    pub graph_stack: Vec<usize>,
    /// Root package supplies the shared registry when evaluating an external graph
    pub registry: Option<&'a materialcraft_core::Package>,
    pub resource_root: Option<&'a crate::ResourceDecoder<'a>>,
}

impl<'a> Options<'a> {
    pub fn new(resource: &'a crate::ResourceDecoder<'a>) -> Options<'a> {
        Options {
            environment: Default::default(),
            parent_tiling: None,
            parent_pixel_ratio: None,
            parent_base: None,
            max_size: None,
            only: None,
            inputs: BTreeMap::new(),
            input_formats: BTreeMap::new(),
            input_bases: BTreeMap::new(),
            base_level_inputs: BTreeSet::new(),
            previous_sampling: None,
            preview_input_keys: BTreeMap::new(),
            bitmap_formats: BTreeMap::new(),
            overrides: BTreeMap::new(),
            literal_inputs: Vec::new(),
            resource,
            cancel: None,
            depth: 0,
            graph_stack: Vec::new(),
            registry: None,
            resource_root: None,
        }
    }
}
