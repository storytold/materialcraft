//! The control hooks (`ui.set` keys, panel requests and the collected errors) that scripted runs drive.

use serde_json::{Map, Value};

use crate::MaterialCraftApp;

/// A request to open a parameter's menu, left for the owning panel to serve on its next show.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParameterMenu {
    /// The node whose parameter it is.
    pub node: materialcraft_engine::core::NodeId,
    /// The parameter's id.
    pub param: String,
    /// The menu of the base value.
    pub base: bool,
    /// The menu of the inheritance setting.
    pub inheritance: bool,
}

/// What the hooks hold between frames.
#[derive(Default)]
#[allow(dead_code)] // P3(shell-w7): sh.A32
pub(crate) struct Hooks {
    /// A parameter menu a panel has to open; the panel clears it.
    pub(crate) parameter_menu: Option<ParameterMenu>,
    /// A library filter request (`Some(true)`: entries MaterialCraft lacks); the Library clears it.
    pub(crate) library_filter: Option<bool>,
}

/// The `ui.set` keys the hooks own, taken out of the request before the rest is applied.
#[allow(dead_code)] // P3(shell-w7): sh.A30
pub(crate) struct SetKeys {
    _private: (),
}

#[allow(dead_code)] // P3(shell-w7): sh.A30
impl SetKeys {
    /// Takes the hook keys out of `params`, or says why one is not valid.
    pub(crate) fn take(params: &mut Value) -> Result<SetKeys, String> {
        let _ = params;
        todo!("sh.A30")
    }

    /// Applies the keys to `app`.
    pub(crate) fn apply(self, app: &mut MaterialCraftApp) {
        let _ = app;
        todo!("sh.A30")
    }
}

/// Answers a hook method, or `None` when `method` is not one of them.
#[allow(dead_code)] // P3(shell-w7): sh.A30
pub(crate) fn handle(app: &mut MaterialCraftApp, ctx: &egui::Context, method: &str, params: &Value) -> Option<Result<Value, String>> {
    let _ = (app, ctx, method, params);
    todo!("sh.A30")
}

/// Adds the hooks' fields to `ui.inspect`, draining the errors they collected.
#[allow(dead_code)] // P3(shell-w7): sh.A30
pub(crate) fn inspect(app: &mut MaterialCraftApp, ctx: &egui::Context, into: &mut Map<String, Value>) {
    let _ = (app, ctx, into);
    todo!("sh.A30")
}

/// Once per frame in the logic phase.
#[allow(dead_code)] // P3(shell-w7): sh.A30
pub(crate) fn step(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let _ = (app, ctx);
    todo!("sh.A30")
}

/// Once after the frame is drawn: reports a request no panel served to the hooks' errors and clears it.
#[allow(dead_code)] // P3(shell-w7): sh.A30
pub(crate) fn after_draw(app: &mut MaterialCraftApp) {
    let _ = app;
    todo!("sh.A30")
}
