//! What the 3D view shows and how: the scene, camera, lights, environment, display options,
//! material model and renderer settings. Plain data, so the engine keeps it in the session and
//! the renderer reads it without either depending on the other.

#[macro_use]
mod choice;

mod ambient_light;
mod background;
mod color_space;
mod height_technique;
mod material_parameter;
mod material_value;
mod mesh_info;
mod parameter_kind;
mod point_light;
mod projection;
mod shader;
mod view3d;
mod view_channels;

pub use ambient_light::AmbientLight;
pub use background::Background;
pub use color_space::ColorSpace;
pub use height_technique::HeightTechnique;
pub use material_parameter::{MaterialParameter, PARAMETERS};
pub use material_value::MaterialValue;
pub use mesh_info::MeshInfo;
pub use parameter_kind::ParameterKind;
pub use point_light::PointLight;
pub use projection::Projection;
pub use shader::Shader;
pub use view_channels::ViewChannels;
pub use view3d::{UNITS_PER_MESH_UNIT, View3d};
