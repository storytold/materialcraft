use serde::{Deserialize, Serialize};

/// Which dialogs are open
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Windows {
    pub new_graph: bool,
    pub export: bool,
    pub about: bool,
    pub preferences: bool,
    pub shortcuts: bool,
    pub home: bool,
    pub plugin_manager: bool,
    #[serde(skip)]
    pub package_save: Option<crate::web_files::PackageSave>,
}
