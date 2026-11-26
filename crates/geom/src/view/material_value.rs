use serde::{Deserialize, Serialize};

/// The value of one material parameter
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MaterialValue {
    Toggle(bool),
    Number(f32),
    Pair([f32; 2]),
    /// Linear RGB
    Color([f32; 3]),
}
