//! Maps a control's edit events onto the commands and undo groups they run.

use craft_behavior::value::{EditEvent, EditEvents, Mixed};

use crate::MaterialCraftApp;

/// Runs `command` for each event in order, closing the undo group as the event says, and returns the result of
/// the last command it ran. `Begin` closes the open group; `Update` runs the command; `Commit` runs it, then
/// closes the group; `Cancel` with a prior value runs it with that value, then closes the group; `Set` closes the
/// group, runs the command, and closes it again.
pub(crate) fn send<T>(
    app: &mut MaterialCraftApp,
    events: &EditEvents<T>,
    command: &str,
    params: impl Fn(&T) -> serde_json::Value,
) -> Option<serde_json::Value> {
    let mut last = None;
    for event in events.iter() {
        match event {
            EditEvent::Begin { .. } => app.session.finish_edit(),
            EditEvent::Update { value, .. } => last = app.run(command, params(value)),
            EditEvent::Commit { after, .. } => {
                last = app.run(command, params(after));
                app.session.finish_edit();
            }
            EditEvent::Cancel { before, .. } => {
                if let Some(Mixed::Same(before)) = before {
                    last = app.run(command, params(before));
                }
                app.session.finish_edit();
            }
            EditEvent::Set { after, .. } => last = run_alone(app, command, params(after)),
            _ => {}
        }
    }
    last
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
    let mut last = None;
    for event in events.iter() {
        if let EditEvent::Commit { after, .. } | EditEvent::Set { after, .. } = event {
            last = run_alone(app, command, params(after));
        }
    }
    last
}

/// Runs `command` as an undo step of its own.
fn run_alone(app: &mut MaterialCraftApp, command: &str, params: serde_json::Value) -> Option<serde_json::Value> {
    app.session.finish_edit();
    let result = app.run(command, params);
    app.session.finish_edit();
    result
}
