/// A row of the app's menu tables: the menu bar and the Explorer's row menus (`menu` names the row kind's context
/// menu); a separator is the label `"-"`.
#[derive(Clone, Copy, Debug)]
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

impl Row {
    /// The framework's menu row for this row: its label translated, with the English kept for search, at the path of
    /// its menu and submenu. The command board gives it its shortcut, enabled and checked state.
    pub fn to_craft(&self) -> craft_menu::Row {
        let menu = crate::i18n::tr(self.menu);
        let path: Vec<&str> = std::iter::once(menu).chain(self.sub.map(crate::i18n::tr)).collect();
        if self.label == "-" {
            return craft_menu::Row::separator(&path);
        }

        craft_menu::Row::new(&path, self.id, crate::i18n::tr(self.label)).english(self.label)
    }
}
