//! Headless MaterialCraft command line: inspect, script, cook, export and render packages, and
//! serve agents over MCP or JSON lines. Never opens a window. `run` is the whole program, so it can
//! be tested in-process as well as through the binary; `materialcraft-cli --help` lists the
//! subcommands. The conventions are RFC-0018's: exit 0, 1 (a command failed) or 2 (a usage error),
//! results on stdout, `error: <message>` on stderr.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod args;
mod error;

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use clap::CommandFactory;
use materialcraft_automation::security::{client_token, server_token};
use materialcraft_automation::{AuthorizedWorkspace, BridgeClient, Filesystem, Headless, MaterialCraftMcp, RenderOptions, rpc};
use serde_json::{Value, json};

use args::{
    Assignment, BatchArgs, Cli, Command, CommandsArgs, CookArgs, ExportArgs, GraphArgs, InfoArgs, McpArgs, NewArgs, NodesArgs, PublishArgs, RenderArgs,
    RootArgs, RunArgs, ServeArgs, parse,
};
use error::{CliError, unicode};

/// Writes a line. A closed pipe (`materialcraft-cli nodes | head`) is not worth an error: the rest
/// of the output is dropped.
macro_rules! outln {
    ($to:expr, $($arg:tt)*) => {
        drop(writeln!($to, $($arg)*))
    };
}

/// `materialcraft-cli <version> (<commit> <date>)`'s version part.
fn version() -> String {
    let sha = env!("MATERIALCRAFT_BUILD_SHA");
    let short = sha.get(..7).unwrap_or(sha);
    let date = env!("MATERIALCRAFT_BUILD_DATE");
    if short.is_empty() { materialcraft_engine::VERSION.to_string() } else { format!("{} ({short} {date})", materialcraft_engine::VERSION) }
}

/// The command line's clap definition.
pub fn command() -> clap::Command {
    Cli::command().version(version())
}

/// A Launcher-only protocol argument, in either the separate or the `=` form. The Launcher's
/// Nightly protocol belongs to the GUI alone and is refused before any parsing.
fn nightly_protocol_argument(args: &[OsString]) -> Option<String> {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .find(|arg| ["--launch-profile", "--profile-root"].iter().any(|flag| arg == flag || arg.starts_with(&format!("{flag}="))))
}

/// Run the CLI on its arguments (without the program name), writing results to `out` and
/// diagnostics to `err`. Returns the process exit code: 0 on success, 1 on failure, 2 for a usage
/// error. Help and the version go to `out`, exit 0. `mcp` and `serve` speak on the process's own
/// stdin and stdout.
pub fn run<I, T>(args: I, out: &mut dyn Write, err: &mut dyn Write) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if let Some(argument) = nightly_protocol_argument(&args) {
        outln!(err, "error: {argument} is the MaterialCraft Nightly GUI's protocol; this tool has no such channel");
        return 2;
    }
    // Help and the version (`help`, `-h`, `<subcommand> --help`, `-V`) come back as errors too.
    let cli = match parse(&args) {
        Ok(cli) => cli,
        Err(e) => {
            let to: &mut dyn Write = if e.use_stderr() { err } else { out };
            let _ = write!(to, "{}", e.render());
            return e.exit_code();
        }
    };
    match dispatch(cli.command, out, err) {
        Ok(()) => 0,
        Err(e) => {
            outln!(err, "error: {e}");
            1
        }
    }
}

fn dispatch(command: Command, out: &mut dyn Write, err: &mut dyn Write) -> Result<(), CliError> {
    match command {
        Command::Info(a) => info(&a, out),
        Command::Publish(a) => publish(&a, out),
        Command::Commands(a) => commands(&a, out),
        Command::Nodes(a) => nodes(&a, out),
        Command::Run(a) => run_command(&a, out),
        Command::Batch(a) => batch(&a, out),
        Command::New(a) => new(&a, out),
        Command::Cook(a) => cook(&a, out),
        Command::Export(a) => export(&a, out),
        Command::Render(a) => render(&a, out),
        Command::Mcp(a) => mcp(&a),
        Command::Serve(a) => serve(&a, err),
    }
}

/// A headless session on `input` (an .sbs package, or `sample`), with local file access
fn open(input: &Path) -> Result<Headless, CliError> {
    let mut h = Headless::new(Headless::default_session(), Filesystem::TrustedLocal);
    if input != Path::new("sample") {
        h.load(unicode(input)?)?;
    }
    Ok(h)
}

fn print(out: &mut dyn Write, v: &Value) {
    outln!(out, "{}", serde_json::to_string_pretty(v).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")));
}

/// Open `--graph` and apply every `--set <input>=<value>` (the value is JSON, else a string)
fn select_graph(h: &mut Headless, graph: &GraphArgs, set: &[Assignment]) -> Result<(), CliError> {
    if let Some(g) = graph.value() {
        h.command_run("graph.open", json!({"graph": g}))?;
    }
    for Assignment { id, value } in set {
        let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.clone()));
        h.command_run("graph.exposed.set", json!({"id": id, "value": value})).map_err(|source| CliError::Set { id: id.clone(), source })?;
    }
    Ok(())
}

fn published(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mcmat"))
}

fn info(a: &InfoArgs, out: &mut dyn Write) -> Result<(), CliError> {
    let mut h = open(&a.input)?;
    let report = if published(&a.input) { h.command_run("material.info", Value::Null)? } else { h.inspect() };
    print(out, &report);
    Ok(())
}

fn commands(a: &CommandsArgs, out: &mut dyn Write) -> Result<(), CliError> {
    print(out, &open(Path::new("sample"))?.command_list(a.filter.as_deref()));
    Ok(())
}

fn nodes(a: &NodesArgs, out: &mut dyn Write) -> Result<(), CliError> {
    let mut h = open(Path::new("sample"))?;
    print(out, &h.command_run("library.list", json!({"filter": a.filter, "category": a.category}))?);
    Ok(())
}

fn run_command(a: &RunArgs, out: &mut dyn Write) -> Result<(), CliError> {
    let mut h = open(&a.input)?;
    print(out, &h.command_run(&a.id, a.params.clone().unwrap_or(Value::Null))?);
    if let Some(path) = &a.output {
        h.save(Some(unicode(path)?))?;
    }
    Ok(())
}

fn batch(a: &BatchArgs, out: &mut dyn Write) -> Result<(), CliError> {
    let text = std::fs::read_to_string(&a.script).map_err(|source| CliError::File { path: a.script.clone(), source })?;
    let script: Value = serde_json::from_str(&text).map_err(CliError::Script)?;
    let (input, steps, save) = match &script {
        Value::Array(s) => ("sample", s.clone(), None),
        obj => (
            obj.get("open").and_then(Value::as_str).unwrap_or("sample"),
            obj.get("steps").and_then(Value::as_array).cloned().unwrap_or_default(),
            obj.get("save").and_then(Value::as_str),
        ),
    };
    let mut h = open(Path::new(input))?;
    let report = h.batch(&steps);
    print(out, &report);
    if !report["failed"].is_null() {
        return Err(CliError::BatchStopped);
    }
    if let Some(path) = save {
        h.save(Some(path))?;
    }
    Ok(())
}

fn new(a: &NewArgs, out: &mut dyn Write) -> Result<(), CliError> {
    let mut h = Headless::new(Headless::default_session(), Filesystem::TrustedLocal);
    h.command_run("file.newPackage", json!({"name": a.name.as_deref().unwrap_or("Untitled package")}))?;
    h.command_run("file.new", json!({"template": a.template, "size": a.size.unwrap_or(1024), "name": a.name}))?;
    h.command_run("graph.delete", json!({"graph": 0}))?;
    print(out, &h.save(Some(unicode(&a.output)?))?);
    Ok(())
}

fn publish(a: &PublishArgs, out: &mut dyn Write) -> Result<(), CliError> {
    let mut h = open(&a.input)?;
    print(out, &h.export_material(&json!({"path": unicode(&a.output)?, "graph": a.graph.value()}))?);
    Ok(())
}

/// The parameters of `material.cook` and `file.export`: the values, the images and the size.
fn cook_parameters(graph: &GraphArgs, set: &[Assignment], inputs: &[Assignment], size: Option<args::Size>, preset: Option<&str>) -> Value {
    let values: serde_json::Map<String, Value> =
        set.iter().map(|a| (a.id.clone(), serde_json::from_str(&a.value).unwrap_or_else(|_| json!(a.value)))).collect();
    let inputs: serde_json::Map<String, Value> = inputs.iter().map(|a| (a.id.clone(), json!(a.value))).collect();
    json!({"graph": graph.value(), "size": size.map(args::Size::value), "preset": preset, "values": values, "inputs": inputs})
}

fn cook(a: &CookArgs, out: &mut dyn Write) -> Result<(), CliError> {
    let mut h = open(&a.input)?;
    if published(&a.input) || a.size.is_some() || !a.inputs.is_empty() || a.preset.is_some() {
        print(out, &h.command_run("material.cook", cook_parameters(&a.graph, &a.set, &a.inputs, a.size, a.preset.as_deref()))?);
    } else {
        select_graph(&mut h, &a.graph, &a.set)?;
        print(out, &h.command_run("graph.cook", json!({"full": a.full}))?);
    }
    Ok(())
}

fn export(a: &ExportArgs, out: &mut dyn Write) -> Result<(), CliError> {
    let mut h = open(&a.input)?;
    let mut p = cook_parameters(&a.graph, &a.set, &a.inputs, a.size, a.preset.as_deref());
    // clap's group makes one of DIR and --out required, and keeps them from being both.
    let Some(dir) = a.out.as_deref().or(a.dir.as_deref()) else { return Ok(()) };
    p["dir"] = json!(unicode(dir)?);
    p["format"] = json!(a.format);
    p["depth"] = json!(a.depth);
    p["pattern"] = json!(a.pattern);
    p["profile"] = json!(a.profile.map(|profile| profile.name()));
    p["normalFormat"] = json!(a.normal_format.map(|format| format.name()));
    if let Some(scale) = a.displacement_scale.and_then(serde_json::Number::from_f64) {
        p["displacementScale"] = scale.into();
    }
    print(out, &h.export(&p)?);
    Ok(())
}

fn render(a: &RenderArgs, out: &mut dyn Write) -> Result<(), CliError> {
    let mut h = open(&a.input)?;
    select_graph(&mut h, &a.graph, &a.set)?;
    let options = RenderOptions {
        view: a.view.map(|view| view.name().to_string()),
        width: a.size.map(|s| s.width),
        height: a.size.map(|s| s.height),
        yaw: a.yaw,
        pitch: a.pitch,
        scene: a.scene.clone(),
        node: a.node,
        port: None,
        full: a.full,
        tiling: a.tiling,
        hq: a.hq,
        displacement: a.displacement,
    };
    let rendered = h.render_png(&options)?;
    materialcraft_engine::formats::write_atomic(&a.output, &rendered.png).map_err(|source| CliError::Write { path: a.output.clone(), source })?;
    print(out, &json!({"path": a.output.to_string_lossy(), "width": rendered.width, "height": rendered.height}));
    Ok(())
}

/// File access for servers: none without roots, else only below them
fn workspace(roots: &RootArgs) -> Result<Filesystem, CliError> {
    if roots.read_root.is_none() && roots.write_root.is_none() {
        return Ok(Filesystem::Denied);
    }
    Ok(Filesystem::Workspace(AuthorizedWorkspace::new(roots.read_root.as_deref(), roots.write_root.as_deref())?))
}

fn mcp(a: &McpArgs) -> Result<(), CliError> {
    let rt = tokio::runtime::Runtime::new().map_err(CliError::Serve)?;
    let server = match a.bridge {
        Some(port) => MaterialCraftMcp::bridge(BridgeClient::new(port, client_token(port, a.token.clone())?)),
        None => MaterialCraftMcp::headless(Headless::new(Headless::default_session(), workspace(&a.roots)?)),
    };
    rt.block_on(server.serve_stdio()).map_err(|e| CliError::Automation(materialcraft_automation::AutomationError::Other(e.to_string())))
}

fn serve(a: &ServeArgs, err: &mut dyn Write) -> Result<(), CliError> {
    let h = Arc::new(Mutex::new(Headless::new(Headless::default_session(), workspace(&a.roots)?)));
    match a.port {
        Some(port) => {
            // Bind first: a port in use must not replace its server's token file, and port 0's token file is named after the port actually picked
            let listener = rpc::bind_tcp(port)?;
            let addr = listener.local_addr().map_err(CliError::Serve)?;
            let (token, file) = server_token(addr.port())?;
            outln!(err, "materialcraft-cli: serving JSON lines on {addr}");
            if let Some(f) = &file {
                outln!(err, "materialcraft-cli: token in {}", f.display());
            }
            rpc::serve_listener(listener, h, token)?;
        }
        None => rpc::serve_lines(&h, std::io::stdin().lock(), std::io::stdout().lock()).map_err(CliError::Serve)?,
    }
    Ok(())
}
