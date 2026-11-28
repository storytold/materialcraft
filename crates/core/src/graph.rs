use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{CoreError, Result, Value};

use crate::{Attributes, Comment, Connection, ExposedParameter, Frame, GraphKind, Node, NodeId, OutputFormat, Pin, Preset};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub name: String,
    /// Package folder, retained so external SBS references keep their identity.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    #[serde(default)]
    pub function: Option<crate::FunctionGraph>,
    /// Declared SBS return type, including functions whose implementation is unavailable.
    #[serde(default)]
    pub function_type: u32,
    pub nodes: Vec<Node>,
    pub connections: Vec<Connection>,
    #[serde(default)]
    pub frames: Vec<Frame>,
    #[serde(default)]
    pub comments: Vec<Comment>,
    #[serde(default)]
    pub pins: Vec<Pin>,
    /// log2 of the graph ("parent") size per axis
    pub size: [u32; 2],
    /// An authored absolute output size, in log2 texels, independent of the default parent size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absolute_output_size: Option<[i32; 2]>,
    /// The signed graph output size offset from its caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_output_size: Option<crate::GraphSizeOffset>,
    /// Authored function supplying the two output size exponents or offsets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_size_function: Option<crate::FunctionGraph>,
    #[serde(default)]
    pub format: OutputFormat,
    #[serde(default)]
    pub seed: i32,
    /// Whether a nested graph starts from its own seed or adds to its instance seed.
    #[serde(default)]
    pub seed_mode: crate::ParameterInheritance,
    /// An authored expression for the graph seed offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_function: Option<crate::FunctionGraph>,
    #[serde(default)]
    pub pixel_ratio: i32,
    #[serde(default)]
    pub pixel_ratio_mode: crate::ParameterInheritance,
    #[serde(default = "default_tiling")]
    pub tiling: [bool; 2],
    #[serde(default)]
    pub tiling_mode: crate::ParameterInheritance,
    #[serde(default)]
    pub physical_size: [f32; 3],
    #[serde(default)]
    pub exposed: Vec<ExposedParameter>,
    /// Current values of the exposed inputs (missing = default)
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    /// Named sets of input values
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<Preset>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub input_images: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub input_images_linear: BTreeSet<String>,
    /// SBS image bindings without an explicit preview interpretation use the resource's file transfer.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub input_images_from_file: BTreeSet<String>,
    #[serde(default)]
    pub attributes: Attributes,
    /// The id the next node, frame, comment or pin gets
    pub next_id: u64,
}

/// Bounds for each default parent dimension, as log2 (1 px .. 8192 px)
pub const MIN_SIZE_LOG2: u32 = 0;
pub const MAX_SIZE_LOG2: u32 = 13;

impl Graph {
    /// Round a requested parent dimension up to a supported power of two.
    pub fn parent_size_log2(pixels: u64) -> u32 {
        pixels.clamp(1 << MIN_SIZE_LOG2, 1 << MAX_SIZE_LOG2).next_power_of_two().ilog2()
    }

    /// Display text never changes the identifier used by SBS references
    pub fn display_name(&self) -> &str {
        if self.attributes.label.is_empty() { &self.name } else { &self.attributes.label }
    }

    pub fn new(name: &str) -> Graph {
        Graph {
            name: name.into(),
            folder: String::new(),
            function: None,
            function_type: 0,
            nodes: Vec::new(),
            connections: Vec::new(),
            frames: Vec::new(),
            comments: Vec::new(),
            pins: Vec::new(),
            size: [10, 10],
            absolute_output_size: None,
            relative_output_size: None,
            output_size_function: None,
            format: OutputFormat::U16,
            seed: 0,
            seed_mode: crate::ParameterInheritance::default(),
            seed_function: None,
            pixel_ratio: 0,
            pixel_ratio_mode: crate::ParameterInheritance::default(),
            tiling: [true, true],
            tiling_mode: crate::ParameterInheritance::default(),
            physical_size: [0.0; 3],
            exposed: Vec::new(),
            inputs: BTreeMap::new(),
            presets: Vec::new(),
            input_images: BTreeMap::new(),
            input_images_linear: BTreeSet::new(),
            input_images_from_file: BTreeSet::new(),
            attributes: Attributes::default(),
            next_id: 1,
        }
    }

    pub fn sbs_path(&self) -> String {
        if self.folder.is_empty() { self.name.clone() } else { format!("{}/{}", self.folder, self.name) }
    }

    /// All editable Function canvases, including processor and parameter functions
    pub fn functions(&self) -> impl Iterator<Item = &crate::FunctionGraph> {
        self.function.iter().chain(self.output_size_function.iter()).chain(self.seed_function.iter()).chain(
            self.nodes
                .iter()
                .flat_map(|node| node.base.functions.values().chain(node.functions.values()).chain(node.params.values().flat_map(value_functions))),
        )
    }

    pub fn functions_mut(&mut self) -> impl Iterator<Item = &mut crate::FunctionGraph> {
        self.function.iter_mut().chain(self.output_size_function.iter_mut()).chain(self.seed_function.iter_mut()).chain(
            self.nodes.iter_mut().flat_map(|node| {
                node.base.functions.values_mut().chain(node.functions.values_mut()).chain(node.params.values_mut().flat_map(value_functions_mut))
            }),
        )
    }

    pub fn instances(&self) -> impl Iterator<Item = &crate::GraphInstance> {
        self.nodes
            .iter()
            .filter_map(|node| node.instance.as_ref())
            .chain(self.functions().flat_map(|function| function.nodes.iter().filter_map(|node| node.instance.as_ref())))
    }

    pub fn instances_mut(&mut self) -> impl Iterator<Item = &mut crate::GraphInstance> {
        self.function
            .iter_mut()
            .chain(self.output_size_function.iter_mut())
            .chain(self.seed_function.iter_mut())
            .flat_map(|function| function.nodes.iter_mut().filter_map(|node| node.instance.as_mut()))
            .chain(self.nodes.iter_mut().flat_map(|node| {
                node.instance.iter_mut().chain(
                    node.base
                        .functions
                        .values_mut()
                        .chain(node.functions.values_mut())
                        .chain(node.params.values_mut().flat_map(value_functions_mut))
                        .flat_map(|function| function.nodes.iter_mut().filter_map(|node| node.instance.as_mut())),
                )
            }))
    }

    pub fn kind(&self) -> GraphKind {
        if self.function.is_some() { GraphKind::Function } else { GraphKind::Material }
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub fn node(&self, id: NodeId) -> Result<&Node> {
        self.nodes.iter().find(|n| n.id == id).ok_or(CoreError::NoNode(id.0))
    }

    pub fn node_mut(&mut self, id: NodeId) -> Result<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id).ok_or(CoreError::NoNode(id.0))
    }

    /// Add a node of definition `def` at `pos`; returns its id
    pub fn add_node(&mut self, def: &str, pos: [f32; 2]) -> NodeId {
        let id = NodeId(self.alloc_id());
        self.nodes.push(Node::new(id, def, pos));
        id
    }

    /// Remove nodes and every connection touching them
    pub fn remove_nodes(&mut self, ids: &[NodeId]) {
        self.nodes.retain(|n| !ids.contains(&n.id));
        self.connections.retain(|c| !ids.contains(&c.from) && !ids.contains(&c.to));
    }

    pub fn connect(&mut self, from: NodeId, from_port: &str, to: NodeId, to_port: &str) -> Result<()> {
        self.node(from)?;
        self.node(to)?;
        if from == to || self.reaches(to, from) {
            return Err(CoreError::Cycle);
        }
        self.connections.retain(|c| !(c.to == to && c.to_port == to_port));
        self.connections.push(Connection { from, from_port: from_port.into(), to, to_port: to_port.into() });
        Ok(())
    }

    pub fn disconnect(&mut self, to: NodeId, to_port: &str) -> bool {
        let before = self.connections.len();
        self.connections.retain(|c| !(c.to == to && c.to_port == to_port));
        before != self.connections.len()
    }

    /// The connection feeding `to.to_port`
    pub fn input_of(&self, to: NodeId, to_port: &str) -> Option<&Connection> {
        self.connections.iter().find(|c| c.to == to && c.to_port == to_port)
    }

    /// Whether `b` is downstream of (or equal to) `a`
    pub fn reaches(&self, a: NodeId, b: NodeId) -> bool {
        let mut stack = vec![a];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(n) = stack.pop() {
            if n == b {
                return true;
            }
            if !seen.insert(n) {
                continue;
            }
            stack.extend(self.connections.iter().filter(|c| c.from == n).map(|c| c.to));
        }
        false
    }

    pub fn topo_order(&self) -> Vec<NodeId> {
        crate::GraphIndex::new(self).topo_order()
    }

    /// Every node upstream of `id` (not including it)
    pub fn upstream(&self, id: NodeId) -> Vec<NodeId> {
        crate::GraphIndex::new(self).upstream(id)
    }

    /// Every node downstream of `id` (not including it)
    pub fn downstream(&self, id: NodeId) -> Vec<NodeId> {
        crate::GraphIndex::new(self).downstream(id)
    }

    /// The value of exposed input `id` (its default when unset)
    pub fn input_value(&self, id: &str) -> Option<&Value> {
        self.inputs.get(id).or_else(|| self.exposed.iter().find(|e| e.id == id).map(|e| &e.default))
    }

    /// Default parent size used when the graph has no caller.
    pub fn parent_size_px(&self) -> [u32; 2] {
        self.size.map(|axis| 1 << axis.clamp(MIN_SIZE_LOG2, MAX_SIZE_LOG2))
    }

    /// Literal graph size in pixels. Cooking also evaluates the output size function.
    pub fn size_px(&self) -> [u32; 2] {
        self.resolved_output_size(None, None).0
    }

    /// Resolve this graph's output for the caller's parent size, including graph instances.
    pub fn output_size_px(&self, parent: [u32; 2]) -> [u32; 2] {
        self.resolved_output_size(Some((parent, [0; 2])), None).0
    }

    /// The SBS size setting before applying it to a particular caller.
    pub fn output_size_setting(&self) -> (crate::SizeMode, [i32; 2]) {
        use crate::{GraphSizeOffset, SizeMode};
        match (self.absolute_output_size, self.relative_output_size) {
            (Some(size), _) => (SizeMode::Absolute, size),
            (_, Some(GraphSizeOffset::Parent(offset))) => (SizeMode::RelativeToParent, offset),
            (_, Some(GraphSizeOffset::Input(offset))) => (SizeMode::RelativeToInput, offset),
            _ => (SizeMode::RelativeToInput, [0; 2]),
        }
    }

    /// Resolve both the allocated dimensions and the portion of the requested size below one texel.
    pub fn resolved_output_size(&self, parent: Option<([u32; 2], [i32; 2])>, evaluated: Option<[i32; 2]>) -> ([u32; 2], [i32; 2]) {
        let (mode, literal) = self.output_size_setting();
        let size = evaluated.unwrap_or(literal);
        if mode == crate::SizeMode::Absolute {
            return (size.map(|side| 1 << side.clamp(0, MAX_SIZE_LOG2 as i32)), [0; 2]);
        }
        let (extent, underflow) = parent.unwrap_or_else(|| (self.parent_size_px(), [0; 2]));
        if size == [0; 2] {
            return (extent, underflow);
        }
        crate::offset_dimensions(extent, underflow, size)
    }
}

fn default_tiling() -> [bool; 2] {
    [true, true]
}

fn value_functions(value: &Value) -> impl Iterator<Item = &crate::FunctionGraph> {
    let (function, pattern) = match value {
        Value::Function(function) => (Some(function.as_ref()), None),
        Value::PatternGraph(pattern) => (None, Some(pattern.as_ref())),
        _ => (None, None),
    };
    function.into_iter().chain(
        pattern
            .into_iter()
            .flat_map(|pattern| &pattern.nodes)
            .flat_map(|node| node.parameters.values())
            .filter_map(|value| if let Value::Function(function) = value { Some(function.as_ref()) } else { None }),
    )
}

fn value_functions_mut(value: &mut Value) -> impl Iterator<Item = &mut crate::FunctionGraph> {
    let (function, pattern) = match value {
        Value::Function(function) => (Some(function.as_mut()), None),
        Value::PatternGraph(pattern) => (None, Some(pattern.as_mut())),
        _ => (None, None),
    };
    function.into_iter().chain(
        pattern
            .into_iter()
            .flat_map(|pattern| &mut pattern.nodes)
            .flat_map(|node| node.parameters.values_mut())
            .filter_map(|value| if let Value::Function(function) = value { Some(function.as_mut()) } else { None }),
    )
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn connect_refuses_cycles_and_replaces_inputs() {
        let mut g = Graph::new("g");
        let a = g.add_node("uniform", [0.0, 0.0]);
        let b = g.add_node("blur", [100.0, 0.0]);
        let c = g.add_node("blur", [200.0, 0.0]);
        g.connect(a, "output", b, "input").unwrap();
        g.connect(b, "output", c, "input").unwrap();
        assert!(matches!(g.connect(c, "output", a, "input"), Err(CoreError::Cycle)));
        assert!(matches!(g.connect(c, "output", c, "input"), Err(CoreError::Cycle)));
        g.connect(a, "output", c, "input").unwrap();
        assert_eq!(g.connections.len(), 2);
        assert_eq!(g.topo_order().first(), Some(&a));
        assert_eq!(g.upstream(c), vec![a]);
        g.remove_nodes(&[a]);
        assert_eq!(g.connections.len(), 0);
    }
}
