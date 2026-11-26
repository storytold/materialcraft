/// The part of the 3D view the Properties panel shows
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Material,
    Lights,
    Camera,
    Environment,
    Renderer,
}

impl Subject {
    /// The English name in the panel's title; the UI translates it
    pub fn label(self) -> &'static str {
        match self {
            Subject::Material => "Material",
            Subject::Lights => "Lights",
            Subject::Camera => "Camera",
            Subject::Environment => "Environment",
            Subject::Renderer => "Renderer",
        }
    }
}
