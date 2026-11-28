use serde::{Deserialize, Serialize};

/// How a node's output size follows its surroundings ("Output size" base parameter)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SizeMode {
    /// The size of the main (first connected) input; the graph size when none is connected
    #[default]
    RelativeToInput,
    /// The graph ("parent") size
    RelativeToParent,
    /// `offset` is the absolute log2 size (e.g. 10 = 1024 px)
    Absolute,
}

/// The authored base for a relative graph output size and its signed offsets in octaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphSizeOffset {
    Input([i32; 2]),
    Parent([i32; 2]),
}

/// Apply per-axis log2 offsets and carry requested exponents below a single texel to descendants.
pub fn offset_dimensions(parent: [u32; 2], underflow: [i32; 2], offset: [i32; 2]) -> ([u32; 2], [i32; 2]) {
    let shift: [i32; 2] = std::array::from_fn(|axis| underflow[axis].saturating_add(offset[axis].clamp(-12, 13)));

    // Internal images may be one texel; the editor minimum would break size functions
    let size = std::array::from_fn(|axis| (f64::from(parent[axis]) * 2f64.powi(shift[axis])).round().clamp(1.0, 8192.0) as u32);
    let size_underflow =
        std::array::from_fn(|axis| if parent[axis].is_power_of_two() { shift[axis].saturating_add(parent[axis].ilog2() as i32).min(0) } else { 0 });

    (size, size_underflow)
}
