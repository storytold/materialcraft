//! Node creation shortcuts add a library entry under the pointer in the material graph: each bindable entry is the
//! command `node.add.<entry id>`, whose keys are the user's keymap overrides

use std::collections::BTreeMap;

use craft_keymap::{Chord, Overrides, Platform};
use materialcraft_engine::LibraryEntry;

/// The library categories whose entries can have a creation shortcut
pub(crate) const CATEGORIES: [&str; 2] = ["Atomic nodes", "Graph items"];

/// The command id prefix of a node creation shortcut; the rest of the id is the library entry's id.
pub(crate) const NODE_ADD: &str = "node.add.";

/// Whether `entry` can have a creation shortcut: an entry of [`CATEGORIES`] that the library provides.
// PLAN: the Shortcuts page (shell-w11, `preference/shortcuts_page.rs:51`) lists its entries with this rule.
pub(crate) fn bindable(entry: &LibraryEntry) -> bool {
    entry.missing.is_none() && CATEGORIES.contains(&entry.category.as_str())
}

/// Preserve the stored syntax where Ctrl means Command on macOS
pub(crate) fn text(chord: Chord) -> String {
    chord.portable().replace("Ctrl+", "Control+").replace("Cmd+", "Ctrl+")
}

/// The saved node-shortcut map (entry id to shortcut text, `Ctrl` meaning Command on macOS) as overrides for
/// `node.add.<id>`. A chord the app catalog already holds on `platform`, at app scope or on the graph canvas, is
/// dropped, as the Preferences refused it when it was set; so is a text that does not parse.
pub(crate) fn migrate(old: &BTreeMap<String, String>, platform: Platform) -> Overrides {
    let app = crate::menus::shortcut_table().into_iter().map(|(_, text)| text);
    let graph = crate::commands::GRAPH_DEFAULTS.iter().map(|key| key.chord);
    let held: Vec<Chord> = app.chain(graph).filter_map(crate::menus::parse_shortcut).map(|chord| chord.effective(platform)).collect();

    let mut overrides = Overrides::new();
    for (entry, text) in old {
        let Some(chord) = crate::menus::parse_shortcut(text) else { continue };
        if held.contains(&chord.effective(platform)) {
            continue;
        }
        overrides.insert(format!("{NODE_ADD}{entry}"), vec![chord.to_string()]);
    }
    overrides
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_keymap::{Key, Mods};

    #[test]
    fn a_shortcut_reads_back_as_the_keys_that_made_it() {
        let saved: BTreeMap<String, String> = [
            ("node:blur".to_owned(), "Ctrl+B".to_owned()),
            ("node:levels".to_owned(), "Control+Shift+L".to_owned()),
            ("node:blend".to_owned(), "Ctrl+S".to_owned()),
        ]
        .into();

        for platform in [Platform::Mac, Platform::Windows, Platform::Linux] {
            let overrides = migrate(&saved, platform);
            let bound = |entry: &str| {
                let texts = overrides.get(&format!("{NODE_ADD}{entry}"))?;
                Chord::parse(texts.first()?).ok().map(|chord| chord.effective(platform))
            };

            assert_eq!(bound("node:blur"), Some(Chord::new(Mods::CMD, Key::B).effective(platform)), "Ctrl in the saved text is Command on {platform:?}");
            let control_shift = Chord::new(Mods { ctrl: true, shift: true, ..Mods::NONE }, Key::L);
            assert_eq!(bound("node:levels"), Some(control_shift.effective(platform)), "Control stays the Control key on {platform:?}");
            assert_eq!(bound("node:blend"), None, "Save already holds Ctrl+S on {platform:?}");
        }
    }
}
