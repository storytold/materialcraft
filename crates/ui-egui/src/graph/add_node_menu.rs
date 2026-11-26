/// Builds command rows for the supplied node entries in catalog order.
pub(crate) fn rows(
    command: &'static str,
    entries: impl IntoIterator<Item = (&'static str, &'static str, &'static str)>,
    params: impl Fn(&str) -> serde_json::Value,
) -> Vec<craft_menu::Node> {
    let mut rows = Vec::new();
    let mut previous_category = None;
    for (id, label, category) in entries {
        if previous_category.is_some_and(|previous| previous != category) {
            rows.push(craft_menu::Node::Separator);
        }
        previous_category = Some(category);
        rows.push(craft_menu::Node::Item(craft_menu::Item::new(craft_keymap::CommandRef::with_params(command, params(id)), crate::i18n::tr(label))));
    }
    rows
}
