//! Tools ▸ Plugin Manager: the list is empty until MaterialCraft supports plugins.

use craft_behavior::{ColumnKey, ColumnWidth};
use craft_ui::contract::{Bind, ColumnSpec, Show as _, ShowCollection as _, ShowContainer as _, TooltipSpec};
use craft_ui::{buttons::Button, layout_components::Row, list::ListRow, table::Table, text::TextField};

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// Shows the plugin manager and its unsupported actions.
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
    if !app.ui.windows.plugin_manager {
        return;
    }
    let filter = TextField::search("Filter", craft_ui::contract::bind_tracked!(app.ui => plugin_filter));
    let columns = vec![
        ColumnSpec::new(ColumnKey::new("plugin"), tr("Plugins"), ColumnWidth::Fill { weight: 1.0 }),
        ColumnSpec::new(ColumnKey::new("loaded"), tr("Loaded"), ColumnWidth::Fixed(110.0)),
    ];
    let reason = tr("Plugins are not supported yet");
    let buttons = ["Refresh", "Install...", "Browse..."];
    let mut open = true;
    let dialog = craft_ui::dialog::Dialog::new(tr("Plugin Manager")).opened(Bind::bound(&mut open));
    dialog.show(ui, |body, _| {
        filter.show(body);
        Table::<usize>::new(tr("Plugins"), columns.clone()).empty(tr("No plugins are installed. MaterialCraft does not load plugins yet.")).show_items(
            body,
            &craft_behavior::keyed(&[] as &[usize], |value| *value),
            &mut |_, _| ListRow::new(""),
        );
        Row::new().show(body, |row, _| {
            for label in buttons {
                Button::new(tr(label)).tooltip(TooltipSpec::new(tr(label)).disabled_text(reason.clone())).enabled(false).show(row);
            }
        });
    });
    app.ui.windows.plugin_manager = open;
}
