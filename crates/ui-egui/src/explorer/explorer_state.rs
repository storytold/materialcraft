use super::ExplorerTarget;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExplorerState {
    pub selected: Option<ExplorerTarget>,
    /// The rows whose children are hidden; session only
    #[serde(skip)]
    #[allow(dead_code)] // P3(panels-w4): pn.A32
    pub closed: HashSet<ExplorerTarget>,
    pub rename: Option<(ExplorerTarget, String)>,
    /// The pointer was over the Explorer last frame, so Enter, F2 and Delete act on its selection
    #[serde(skip)]
    pub hovered: bool,
}
