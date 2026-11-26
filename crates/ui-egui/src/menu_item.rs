/// A row of the app's menu tables: the menu bar and the Explorer's row menus (`menu` names the row kind's context
/// menu); a separator is the label `"-"`.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // P3(shell-w5): sh.A15
pub struct Row {
    /// The menu the row is in.
    pub menu: &'static str,
    /// The submenu, if any.
    pub sub: Option<&'static str>,
    /// The row's text.
    pub label: &'static str,
    /// The command id.
    pub id: &'static str,
    /// The shortcut text.
    pub shortcut: Option<&'static str>,
}

#[allow(dead_code)] // P3(shell-w5): sh.A15
impl Row {
    /// The framework's menu row for this row.
    pub fn to_craft(&self) -> craft_menu::Row {
        todo!("sh.A15")
    }
}

/// A menu entry with an optional submenu and shortcut
#[derive(Clone, Copy, Debug)]
pub struct MenuItem {
    pub menu: &'static str,
    pub sub: Option<&'static str>,
    pub label: &'static str,
    pub id: &'static str,
    pub shortcut: Option<&'static str>,
}
