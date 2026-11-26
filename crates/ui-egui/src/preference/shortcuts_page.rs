//! Preferences ▸ Shortcuts: keys that add an atomic node or a graph item under the pointer in the material graph.
//! Each node entry's shortcut is a binding of its `node.add.<entry id>` command in the graph scope, held in the
//! draft's overrides until Apply.

use craft_behavior::IconId;
use craft_keymap::{Binding, Chord, CommandRef, Key, Overrides, Policy};
use craft_menu::shortcuts::ChordRecorder;
use craft_ui::buttons::{IconButton, ToggleButton};
use craft_ui::contract::{Bind, Props as _, Show as _, ShowContainer as _};
use craft_ui::display::{Elide, Icon, Picture, Shade, Text};
use craft_ui::layout_components::Row;
use craft_ui::list::ThumbAspect;
use craft_ui::patterns::preferences::{PageSpec, RowKind, RowSpec};
use craft_ui::primitives::{IconSpec, Priority, ThumbPaint, ThumbRequest, Thumbnails};
use craft_ui::style::TypeRole;
use craft_ui::text::TextField;
use egui::Ui;
use materialcraft_engine::node_library::LibraryItem;

use super::preferences_page::{SHORTCUTS, SHORTCUTS_TITLE};
use crate::MaterialCraftApp;
use crate::i18n::{tr, trf};
use crate::node_shortcuts::{NODE_ADD, bindable};
use crate::thumbs::{LibrarySource, LibraryThumb};

const NODES: &str = "shortcuts.nodes";
/// The side of a row's node picture, in points
const ICON_SIDE: f32 = 22.0;
// The icon catalog has no flow-arrow glyph, so spline stands in for entries without a node picture.
/// What a row shows where it has no node picture.
const FLOW_GLYPH: &str = "spline";
/// The clear-all button's icon.
const CLEAR_GLYPH: &str = "rotate_ccw";

/// The command that adds library entry `entry`.
fn command(entry: &str) -> CommandRef {
    CommandRef::new(format!("{NODE_ADD}{entry}"))
}

/// The binding texts `overrides` give the command `key`, when it has any.
fn assigned<'a>(overrides: &'a Overrides, key: &str) -> Option<&'a [String]> {
    overrides.get(key).filter(|texts| !texts.is_empty())
}

pub(super) fn page() -> PageSpec<'static> {
    PageSpec::new(
        SHORTCUTS,
        tr(SHORTCUTS_TITLE),
        vec![RowSpec::new(NODES, tr("Node creation shortcuts"), RowKind::CustomRows).keywords("shortcut keys node creation search")],
    )
}

/// Removes every node creation shortcut from `overrides`; returns whether there was any.
fn clear_nodes(overrides: &mut Overrides) -> bool {
    let keys: Vec<String> = overrides.iter().filter(|(key, _)| key.starts_with(NODE_ADD)).map(|(key, _)| key.to_owned()).collect();
    for key in &keys {
        overrides.remove(key);
    }
    !keys.is_empty()
}

pub(super) fn reset(overrides: &mut Overrides) {
    clear_nodes(overrides);
}

/// The page's rows: the heading with the clear-all button, the searches, and a row per node entry with its picture,
/// its name and the field that records its keys. Returns whether the draft changed.
pub(super) fn rows(app: &mut MaterialCraftApp, ui: &mut Ui) -> bool {
    let Some(overrides) = app.ui.preferences.draft.as_ref().map(|draft| draft.overrides.clone()) else { return false };
    let ctx = ui.ctx().clone();

    let any = overrides.iter().any(|(key, texts)| key.starts_with(NODE_ADD) && !texts.is_empty());
    let mut clear = false;
    Row::new().show(ui, |ui, _| {
        let _ = Text::new(tr("Node creation shortcuts")).role(TypeRole::SectionCaption).show(ui);
        clear = IconButton::new(IconId::new(CLEAR_GLYPH)).label(tr("Remove every node creation shortcut")).enabled(any).show(ui).clicked();
    });
    let _ = Text::new(tr("For the material graph: press the keys over the graph to add the node under the pointer")).shade(Shade::Muted).show(ui);

    let state = &mut app.ui.preferences;
    Row::new().show(ui, |ui, _| {
        let _ = TextField::search(tr("Search"), Bind::bound(&mut state.shortcut_search)).placeholder(tr("Search")).show(ui);
        let _ = TextField::search(tr("Search by shortcut"), Bind::bound(&mut state.shortcut_key_search)).placeholder(tr("Search by shortcut")).show(ui);
        let _ = ToggleButton::new(tr("Show all assigned"), Bind::bound(&mut state.only_assigned)).show(ui);
    });
    let (search, key_search, only_assigned) = (state.shortcut_search.to_lowercase(), state.shortcut_key_search.to_lowercase(), state.only_assigned);
    let capturing = state.capturing.clone();

    let catalog = crate::library::entries(app);
    let store = app.thumbs.library_thumbs(&ctx);
    let source = app.thumbs.library_source(&app.session.library);
    let request = ThumbRequest::for_cell(ICON_SIDE, ctx.pixels_per_point());

    // The recorders report the chords of their own pass's keys, so a second pass of a frame records nothing again
    let mut started = None;
    let mut pressed = None;
    for entry in catalog.iter().filter(|entry| entry.missing.is_none() && bindable(entry)) {
        let label = tr(&entry.label).to_owned();
        let command = command(&entry.id);
        let shortcut = assigned(&overrides, &command.key()).map(|texts| texts.join(", "));
        let by_key = match &shortcut {
            Some(text) => key_search.is_empty() || text.to_lowercase().contains(&key_search),
            None => !only_assigned && key_search.is_empty(),
        };
        if !by_key || !(search.is_empty() || label.to_lowercase().contains(&search)) {
            continue;
        }

        let recording = capturing.as_deref() == Some(entry.id.as_str());
        Row::new().id_salt(entry.id.as_str()).show(ui, |ui, _| {
            picture(ui, &store, &source, &request, &entry.item);
            // shortcut: the name and the recorder take their own widths, not the old 260 and 220 columns; set a column grid if rows must line up
            let _ = Text::new(label.clone()).elide(Elide::End).show(ui);
            let shown = if recording { tr("Press keys, Backspace to clear, Esc to cancel").to_owned() } else { shortcut.clone().unwrap_or_default() };
            let out = ChordRecorder::new(trf("Shortcut for {label}", &[("label", &label)]), recording, shown).show(ui);
            if out.started {
                started = Some(entry.id.clone());
            }
            if let Some(chord) = out.chords.first() {
                pressed = Some((command.clone(), *chord));
            }
        });
    }

    let mut changed = false;
    if let Some((command, chord)) = pressed {
        changed |= take_chord(app, &ctx, &command, chord);
    }
    if started.is_some() {
        app.ui.preferences.capturing = started;
    }
    if clear && let Some(draft) = app.ui.preferences.draft.as_mut() {
        changed |= clear_nodes(&mut draft.overrides);
    }
    changed
}

/// A node entry's picture, or the flow glyph for an entry without one and while its picture is not ready.
fn picture(ui: &mut Ui, store: &Thumbnails<LibraryThumb>, source: &LibrarySource, request: &ThumbRequest, item: &LibraryItem) {
    let state = match item {
        LibraryItem::Node { def } if def != "dot" => Some(store.state(ui.ctx(), source, &LibraryThumb::Node(def.clone()), request, Priority::VISIBLE)),
        _ => None,
    };
    match state.filter(|state| state.shown().is_some()) {
        Some(state) => {
            let _ = Picture::painter("", ThumbAspect::SQUARE, |cx, rect| {
                store.paint(cx, rect, &state, &ThumbPaint::default());
            })
            .show(ui);
        }
        None => {
            let _ = Icon::new(IconSpec::svg(IconId::new(FLOW_GLYPH))).show(ui);
        }
    }
}

/// What a chord recorded for `command` does: bare Escape stops recording, bare Backspace or Delete clears the entry,
/// a chord an app command holds is refused with an error toast, and any other chord becomes the entry's shortcut in
/// the draft, taken off another node entry that had it. Returns whether the draft changed.
fn take_chord(app: &mut MaterialCraftApp, ctx: &egui::Context, command: &CommandRef, chord: Chord) -> bool {
    app.ui.preferences.capturing = None;
    let Some(overrides) = app.ui.preferences.draft.as_ref().map(|draft| draft.overrides.clone()) else { return false };
    if chord == Chord::plain(Key::Escape) {
        return false;
    }
    if chord == Chord::plain(Key::Backspace) || chord == Chord::plain(Key::Delete) {
        return app.ui.preferences.draft.as_mut().and_then(|draft| draft.overrides.remove(&command.key())).is_some();
    }

    // The window's live keymap over the draft's overrides: its holder rule and its platform's modifiers decide
    let Some(live) = craft_ui::runtime::commands::keymap(ctx) else { return false };
    let mut keymap = craft_keymap::Keymap::clone(&live).with_overrides(overrides);
    if let Some(holder) = keymap.holder(&chord).filter(|holder| !holder.id.starts_with(NODE_ADD)) {
        let text = crate::node_shortcuts::text(chord);
        let name = keymap.bindable(&holder).map_or_else(|| holder.id.clone(), |def| tr(&def.label).to_owned());
        app.toast(trf("{text} is already the shortcut of {command}", &[("text", &text), ("command", &name)]), true);
        return false;
    }
    match keymap.assign(command, Binding::new(chord), Policy::Steal) {
        Ok(_) => match app.ui.preferences.draft.as_mut() {
            Some(draft) => {
                draft.overrides = keymap.overrides().clone();
                true
            }
            None => false,
        },
        Err(error) => {
            app.toast(tr(&error.to_string()).to_owned(), true);
            false
        }
    }
}
