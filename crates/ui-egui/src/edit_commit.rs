//! Maps a control's edit events onto the commands and undo groups they run.

use craft_behavior::value::EditEvents;

use crate::MaterialCraftApp;

/// Runs `command` for each event in order, closing the undo group as the event says, and returns the result of
/// the last command it ran. `Begin` closes the open group; `Update` runs the command; `Commit` runs it, then
/// closes the group; `Cancel` with a prior value runs it with that value, then closes the group; `Set` closes the
/// group, runs the command, and closes it again.
#[allow(dead_code)] // P3(panels-w8): pn.A23
pub(crate) fn send<T>(
    app: &mut MaterialCraftApp,
    events: &EditEvents<T>,
    command: &str,
    params: impl Fn(&T) -> serde_json::Value,
) -> Option<serde_json::Value> {
    let _ = (app, events, command, params);
    todo!("pn.A23")
}

/// Runs the command for `Commit` and `Set` only (graph and folder attributes) and returns the last command's
/// result.
#[allow(dead_code)] // P3(panels-w8): pn.A24
pub(crate) fn commit_only<T>(
    app: &mut MaterialCraftApp,
    events: &EditEvents<T>,
    command: &str,
    params: impl Fn(&T) -> serde_json::Value,
) -> Option<serde_json::Value> {
    let _ = (app, events, command, params);
    todo!("pn.A24")
}
