//! Autosave of a modified package: the schedule and the worker.

/// The autosave schedule and worker (`autosave_workspace`, the next time and the running worker move here).
#[derive(Default)]
#[allow(dead_code)] // P3(shell-w4): sh.A2
pub(crate) struct State {
    _private: (),
}
