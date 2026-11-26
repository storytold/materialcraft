use materialcraft_engine::core::PatternNodeKind;

use crate::MaterialCraftApp;

/// Add an FX-Map node of `kind` in the middle of the open FX-Map canvas
pub(crate) fn add(app: &mut MaterialCraftApp, kind: PatternNodeKind) {
    if let Some(mut editor) = app.pattern_editor.take() {
        editor.menu_at = egui::pos2(editor.view.center.x as f32, editor.view.center.y as f32);
        editor.edit(app, materialcraft_engine::PatternEdit::Add { kind, at: editor.menu_at.into(), snap: app.ui.prefs.snap_to_grid }, false);
        app.pattern_editor = Some(editor);
    }
}
