//! Loopback JSON-lines control server (`--control <port>`): one request per line,
//! `{"id", "method", "params"}`, one reply per line. A client authenticates with the token from
//! `MATERIALCRAFT_CONTROL_TOKEN` or, when that is unset, the token kept (owner-only) in
//! `<temp>/materialcraft-control-<port>.token`. The transport, its authentication, framing,
//! connection limits and idle timeout are the framework's (`craft_automation::server`); the
//! requests reach the UI thread through `craft_automation::control::ControlChannel`.

use std::sync::mpsc::Receiver;

use craft_automation::control::ControlRequest;
use craft_automation::security::server_token;
use craft_automation::server::{self, Listening};
use craft_ui::runtime::windows::WindowHost;

/// Listen on `127.0.0.1:port` and return the requests for the UI thread with the server's handle, which stops the
/// server when dropped, so the caller keeps it for the app's life. Bind and token failures are printed and start no
/// server.
pub fn start(port: u16, ctx: egui::Context) -> Option<(Receiver<ControlRequest>, Listening)> {
    // Exactly one source: the framework refuses a token given both ways.
    let supplied = std::env::var("MATERIALCRAFT_CONTROL_TOKEN").ok().map(|token| token.trim().to_string());
    let path = std::env::temp_dir().join(format!("materialcraft-control-{port}.token"));
    let (provided, token_file) = if let Some(token) = supplied.as_deref() { (Some(token), None) } else { (None, Some(path.as_path())) };
    let token = match server_token(provided, token_file) {
        Ok(token) => token,
        Err(e) => {
            eprintln!("materialcraft: control server disabled: {e}");
            return None;
        }
    };
    if token_file.is_some() {
        eprintln!("materialcraft: control token written to {}", path.display());
    }
    match server::listen(port, token, WindowHost::of(&ctx)) {
        Ok((requests, listening)) => {
            eprintln!("materialcraft: control server listening on {}", listening.addr);
            Some((requests, listening))
        }
        Err(e) => {
            eprintln!("materialcraft: could not start the control server: {e}");
            None
        }
    }
}
