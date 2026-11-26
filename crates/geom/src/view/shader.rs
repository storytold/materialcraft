choice! {
    /// The material model the 3D view shades with
    Shader {
        OpenPbr => "openPbr", "OpenPBR";
        Blinn => "blinn", "Blinn";
        Lambert => "lambert", "Lambert";
        MeshInfo => "meshInfo", "Mesh Info";
        MetallicRoughness => "metallicRoughness", "Metallic Roughness";
        MetallicRoughnessCoated => "metallicRoughnessCoated", "Metallic Roughness - Coated";
        MetallicRoughnessSss => "metallicRoughnessSss", "Metallic Roughness - SSS";
        SpecularGlossiness => "specularGlossiness", "Specular Glossiness";
        Unlit => "unlit", "Unlit";
    }
}

impl Shader {
    /// The shader can show the height output, so it offers the height techniques
    pub fn shows_height(self) -> bool {
        !matches!(self, Shader::Lambert | Shader::MeshInfo | Shader::Unlit)
    }

    /// The bit of this shader in [`super::MaterialParameter::shaders`]
    pub const fn bit(self) -> u16 {
        1 << self as u16
    }
}
