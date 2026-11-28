use craft_texture::{Image, Kind};
use materialcraft_core::{Curve, FunctionGraph, Gradient, Value};
use std::collections::BTreeMap;

/// Graph inputs available to a cook, including any fixed by its enclosing instance.
pub trait GraphVariables: Sync {
    fn graph_values(&self) -> &BTreeMap<String, Value>;
    fn literal_inputs(&self) -> &[String] {
        &[]
    }
    /// Integer variables written by an image-node property before the pattern runs.
    fn assigned_integers(&self) -> &[String] {
        &[]
    }
}

impl GraphVariables for BTreeMap<String, Value> {
    fn graph_values(&self) -> &BTreeMap<String, Value> {
        self
    }
}

impl std::ops::Deref for dyn GraphVariables + '_ {
    type Target = BTreeMap<String, Value>;

    fn deref(&self) -> &Self::Target {
        self.graph_values()
    }
}

/// Where a cook runs: output size, seed, tiling, inputs and resolved parameters
pub struct CookContext<'a> {
    pub w: u32,
    pub h: u32,
    pub seed: u32,
    pub format: materialcraft_core::OutputFormat,
    /// Storage precision of each image input, in the same order as inputs.
    pub input_formats: &'a [materialcraft_core::OutputFormat],
    pub tile: bool,
    /// By input port order; `None` = not connected
    pub inputs: &'a [Option<Image>],
    pub params: &'a BTreeMap<String, Value>,
    /// The package's bitmap resources, decoded on demand
    pub resource: &'a (dyn Fn(&str) -> Option<Image> + Sync),
    /// Exposed graph inputs (for `get_*` nodes in function graphs)
    pub vars: &'a dyn GraphVariables,
}

impl CookContext<'_> {
    pub fn bitmap_color_space(&self) -> crate::BitmapColorSpace {
        crate::BitmapColorSpace::from_index(self.i("colour_space"))
    }

    /// A decoder that only accepts the two original request types can still supply the stored channels.
    pub fn bitmap_resource(&self, name: &str) -> Option<Image> {
        let space = self.bitmap_color_space();
        let image = (self.resource)(&space.resource_key(name));
        if image.is_none() && space == crate::BitmapColorSpace::FromFile { (self.resource)(&craft_texture::resource_key(name, true)) } else { image }
    }

    pub fn tiling_axes(&self) -> [bool; 2] {
        self.vars.get("$tiling").and_then(Value::as_i32).map_or([self.tile; 2], |mask| [mask & 1 != 0, mask & 2 != 0])
    }

    pub fn value(&self, id: &str) -> Option<&Value> {
        self.params.get(id)
    }

    pub fn f(&self, id: &str) -> f32 {
        self.value(id).and_then(Value::as_f32).filter(|v| v.is_finite()).unwrap_or(0.0)
    }

    pub fn i(&self, id: &str) -> i32 {
        self.value(id).and_then(Value::as_i32).unwrap_or(0)
    }

    pub fn b(&self, id: &str) -> bool {
        self.value(id).and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn v4(&self, id: &str) -> [f32; 4] {
        self.value(id).and_then(Value::as_vec4).unwrap_or([0.0; 4])
    }

    pub fn v2(&self, id: &str) -> [f32; 2] {
        let v = self.v4(id);
        [v[0], v[1]]
    }

    pub fn text(&self, id: &str) -> &str {
        match self.value(id) {
            Some(Value::Text(s)) => s,
            _ => "",
        }
    }

    pub fn curve(&self, id: &str) -> Curve {
        match self.value(id) {
            Some(Value::Curve(c)) => c.clone(),
            _ => Curve::default(),
        }
    }

    pub fn gradient(&self, id: &str) -> Gradient {
        match self.value(id) {
            Some(Value::Gradient(g)) => g.clone(),
            _ => Gradient::default(),
        }
    }

    pub fn function(&self, id: &str) -> Option<&FunctionGraph> {
        match self.value(id) {
            Some(Value::Function(g)) => Some(g.as_ref()),
            _ => None,
        }
    }

    /// Input `i` if connected
    pub fn input(&self, i: usize) -> Option<&Image> {
        self.inputs.get(i).and_then(Option::as_ref)
    }

    /// Input `i` resampled to the output size (`None` if not connected)
    pub fn sized(&self, i: usize) -> Option<Image> {
        self.input(i).map(|img| img.resize(self.w, self.h, self.tile))
    }

    /// Input `i` at the output size as gray, or a constant gray `v` when unconnected
    pub fn gray_or(&self, i: usize, v: f32) -> Image {
        self.sized(i).map(|img| img.to_gray()).unwrap_or_else(|| Image::filled(self.w, self.h, Kind::Gray, [v; 4]))
    }

    /// Input `i` at the output size (kind kept), or constant `v` of `kind` when unconnected
    pub fn any_or(&self, i: usize, kind: Kind, v: [f32; 4]) -> Image {
        self.sized(i).unwrap_or_else(|| Image::filled(self.w, self.h, kind, v))
    }

    /// The kind an "Any" output takes: the main input's, else `fallback`
    pub fn main_kind(&self, fallback: Kind) -> Kind {
        self.inputs.iter().flatten().next().map_or(fallback, |i| i.kind())
    }

    /// A gray image from `f(u, v)` at the output size
    pub fn gen_gray(&self, f: impl Fn(f32, f32) -> f32 + Sync) -> Image {
        Image::generate_uv(self.w, self.h, Kind::Gray, |u, v| [f(u, v), 0.0, 0.0, 0.0])
    }

    pub fn gen_color(&self, f: impl Fn(f32, f32) -> [f32; 4] + Sync) -> Image {
        Image::generate_uv(self.w, self.h, Kind::Color, f)
    }
}
