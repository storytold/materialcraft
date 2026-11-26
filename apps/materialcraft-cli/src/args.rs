//! The command line as clap derive types. Values arrive typed, so the commands never validate a
//! string; what clap can't say alone is a [`UsageError`], printed and exited like clap's own.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::error::ErrorKind;
use clap::{ArgGroup, Args, FromArgMatches, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

/// The end of `--help` of every command.
const EXIT_STATUS: &str = "Exit status:\n  0  success\n  1  a command failed\n  2  usage error\n\nA process ended by a signal exits 128 + the signal number (130 for Ctrl-C), as in the shell.";

const INPUTS: &str =
    "An <INPUT> is an .sbs, .mcraft or .mcmat file, or `sample` (the built-in atomic sample). Packages save as native SBS. See docs/control-protocol.md.";

/// Headless MaterialCraft
#[derive(Parser, Debug)]
#[command(name = "materialcraft-cli", propagate_version = true, arg_required_else_help = true, after_help = EXIT_STATUS)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// JSON graph inspection, or the published interface of a .mcmat
    #[command(after_help = INPUTS)]
    Info(InfoArgs),
    /// Publish the selected material graph with its reachable source dependencies
    #[command(after_help = INPUTS)]
    Publish(PublishArgs),
    /// List the engine commands (id, label, params)
    Commands(CommandsArgs),
    /// List the node library (ids, ports, parameters)
    Nodes(NodesArgs),
    /// Run one command and print its JSON result; -o saves the package
    #[command(after_help = INPUTS)]
    Run(RunArgs),
    /// Run a script of commands
    ///
    /// The script is JSON: [{"id", "params"}, ...] or {"open": <input>, "steps": [...], "save": <out.sbs>}.
    Batch(BatchArgs),
    /// A new package with one graph from a template (empty, pbr, atomics)
    New(NewArgs),
    /// Print the published outputs, sizes and means for a material, or an authoring graph inspection
    #[command(after_help = INPUTS)]
    Cook(CookArgs),
    /// Write output images, or material.mtlx and its baked textures with --profile materialx
    ///
    /// --normal-format requires --profile and declares the input convention. --graph-path selects
    /// a literal SBS path, including numeric names, instead of --graph. Named profiles choose their
    /// own format and filenames: leave out --format and --pattern.
    #[command(after_help = INPUTS)]
    Export(ExportArgs),
    /// Render the material on a 3D scene, or one node's output (--view 2d), to PNG
    ///
    /// No GPU needed. The height output raises the surface; --hq is a beauty render with a finer
    /// mesh, ambient occlusion and a key light.
    #[command(after_help = INPUTS)]
    Render(RenderArgs),
    /// MCP server on stdio: headless, or bridged to `materialcraft --control <port>`
    ///
    /// Headless, file access is only below the roots.
    Mcp(McpArgs),
    /// JSON lines ({"id","method","params"}) on stdio, or on 127.0.0.1:<port> with a token
    Serve(ServeArgs),
}

#[derive(Args, Debug)]
pub(crate) struct InfoArgs {
    #[arg(value_name = "INPUT")]
    pub(crate) input: PathBuf,
}

/// `--graph` or `--graph-path`: the graph an operation works on.
#[derive(Args, Debug, Default)]
pub(crate) struct GraphArgs {
    /// The graph, by SBS path or index
    #[arg(long, value_name = "SBS-PATH|INDEX", value_parser = graph_selector, conflicts_with = "graph_path")]
    pub(crate) graph: Option<Selector>,
    /// The graph by a literal SBS path, including a numeric name
    #[arg(long, value_name = "SBS-PATH")]
    pub(crate) graph_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Selector {
    Index(u64),
    Name(String),
}

fn graph_selector(s: &str) -> Result<Selector, std::convert::Infallible> {
    Ok(s.parse::<u64>().map_or_else(|_| Selector::Name(s.to_string()), Selector::Index))
}

impl GraphArgs {
    /// The `graph` parameter of the engine's commands.
    pub(crate) fn value(&self) -> Option<Value> {
        match (&self.graph, &self.graph_path) {
            (_, Some(path)) => Some(json!(path)),
            (Some(Selector::Index(index)), None) => Some(json!(index)),
            (Some(Selector::Name(name)), None) => Some(json!(name)),
            (None, None) => None,
        }
    }
}

#[derive(Args, Debug)]
#[command(group = ArgGroup::new("selected").required(true).args(["graph", "graph_path"]))]
pub(crate) struct PublishArgs {
    #[arg(value_name = "PROJECT")]
    pub(crate) input: PathBuf,
    #[arg(value_name = "OUT.MCMAT")]
    pub(crate) output: PathBuf,
    #[command(flatten)]
    pub(crate) graph: GraphArgs,
}

#[derive(Args, Debug)]
pub(crate) struct CommandsArgs {
    /// Only commands whose id, label or parameters contain this text
    #[arg(long, value_name = "TEXT")]
    pub(crate) filter: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct NodesArgs {
    /// Only nodes whose id, ports or parameters contain this text
    #[arg(long, value_name = "TEXT")]
    pub(crate) filter: Option<String>,
    /// Only this category
    #[arg(long, value_name = "TEXT")]
    pub(crate) category: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct RunArgs {
    #[arg(value_name = "INPUT")]
    pub(crate) input: PathBuf,
    /// The command to run (see `commands`)
    #[arg(value_name = "COMMAND-ID")]
    pub(crate) id: String,
    /// The command's parameters as JSON
    #[arg(value_name = "PARAMS-JSON", value_parser = json_value)]
    pub(crate) params: Option<Value>,
    /// Save the package afterwards
    #[arg(short = 'o', value_name = "OUT.SBS")]
    pub(crate) output: Option<PathBuf>,
    /// Let -o replace the input
    #[arg(long, requires = "output")]
    pub(crate) in_place: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("not JSON: {0}")]
pub(crate) struct BadJson(serde_json::Error);

fn json_value(s: &str) -> Result<Value, BadJson> {
    serde_json::from_str(s).map_err(BadJson)
}

#[derive(Args, Debug)]
pub(crate) struct BatchArgs {
    #[arg(value_name = "SCRIPT.JSON")]
    pub(crate) script: PathBuf,
}

#[derive(Args, Debug)]
pub(crate) struct NewArgs {
    /// empty, pbr or atomics
    #[arg(value_name = "TEMPLATE")]
    pub(crate) template: String,
    #[arg(value_name = "OUT.SBS")]
    pub(crate) output: PathBuf,
    /// Graph size in pixels (1024)
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..))]
    pub(crate) size: Option<u32>,
    /// The package and graph name
    #[arg(long, value_name = "NAME")]
    pub(crate) name: Option<String>,
}

/// `<id>=<value>`, from `--set` or `--input`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Assignment {
    pub(crate) id: String,
    pub(crate) value: String,
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("`{0}` is not <identifier>=<value>")]
pub(crate) struct BadAssignment(String);

impl FromStr for Assignment {
    type Err = BadAssignment;

    fn from_str(s: &str) -> Result<Self, BadAssignment> {
        match s.split_once('=') {
            Some((id, value)) if !id.is_empty() => Ok(Assignment { id: id.to_string(), value: value.to_string() }),
            _ => Err(BadAssignment(s.to_string())),
        }
    }
}

/// `--size`: pixels, or WIDTHxHEIGHT.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Size {
    Square(u32),
    Rectangle(u32, u32),
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("`{0}` is not pixels or WIDTHxHEIGHT")]
pub(crate) struct BadSize(String);

impl FromStr for Size {
    type Err = BadSize;

    fn from_str(s: &str) -> Result<Self, BadSize> {
        let pixels = |side: &str| side.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| BadSize(s.to_string()));
        match s.split_once('x') {
            Some((width, height)) => Ok(Size::Rectangle(pixels(width)?, pixels(height)?)),
            None => Ok(Size::Square(pixels(s)?)),
        }
    }
}

impl Size {
    pub(crate) fn value(self) -> Value {
        match self {
            Size::Square(side) => json!(side),
            Size::Rectangle(width, height) => json!([width, height]),
        }
    }
}

/// `--size` of `render`: WIDTHxHEIGHT.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Dimensions {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("`{0}` is not <W>x<H>")]
pub(crate) struct BadDimensions(String);

impl FromStr for Dimensions {
    type Err = BadDimensions;

    fn from_str(s: &str) -> Result<Self, BadDimensions> {
        match s.parse::<Size>() {
            Ok(Size::Rectangle(width, height)) => Ok(Dimensions { width, height }),
            _ => Err(BadDimensions(s.to_string())),
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct CookArgs {
    #[arg(value_name = "INPUT")]
    pub(crate) input: PathBuf,
    #[command(flatten)]
    pub(crate) graph: GraphArgs,
    /// Give a graph input a value, `<id>=<value>` (JSON, else a string; repeatable)
    #[arg(long = "set", value_name = "ID=VALUE")]
    pub(crate) set: Vec<Assignment>,
    /// Feed an input an image, `<id>=<image>` (repeatable)
    #[arg(long = "input", value_name = "ID=IMAGE")]
    pub(crate) inputs: Vec<Assignment>,
    /// A named preset of the material
    #[arg(long, value_name = "NAME")]
    pub(crate) preset: Option<String>,
    /// Output size: pixels, or WIDTHxHEIGHT
    #[arg(long, value_name = "PX|WxH")]
    pub(crate) size: Option<Size>,
    /// Cook at full size instead of the preview size
    #[arg(long)]
    pub(crate) full: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Profile {
    Maps,
    Materialx,
}

impl Profile {
    /// The engine's spelling.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Profile::Maps => "maps",
            Profile::Materialx => "materialx",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum NormalFormat {
    Opengl,
    Directx,
}

impl NormalFormat {
    /// The engine's spelling.
    pub(crate) fn name(self) -> &'static str {
        match self {
            NormalFormat::Opengl => "opengl",
            NormalFormat::Directx => "directx",
        }
    }
}

#[derive(Args, Debug)]
#[command(group = ArgGroup::new("destination").required(true).args(["dir", "out"]))]
pub(crate) struct ExportArgs {
    #[arg(value_name = "INPUT")]
    pub(crate) input: PathBuf,
    /// The folder to write into
    #[arg(value_name = "DIR")]
    pub(crate) dir: Option<PathBuf>,
    /// The folder to write into, in place of DIR
    #[arg(long, value_name = "DIR")]
    pub(crate) out: Option<PathBuf>,
    #[command(flatten)]
    pub(crate) graph: GraphArgs,
    /// Give a graph input a value, `<id>=<value>` (repeatable)
    #[arg(long = "set", value_name = "ID=VALUE")]
    pub(crate) set: Vec<Assignment>,
    /// Feed an input an image, `<id>=<image>` (repeatable)
    #[arg(long = "input", value_name = "ID=IMAGE")]
    pub(crate) inputs: Vec<Assignment>,
    /// A named preset of the material
    #[arg(long, value_name = "NAME")]
    pub(crate) preset: Option<String>,
    /// Image format
    #[arg(long, value_name = "EXT", value_parser = ["png", "jpg", "tga", "bmp", "webp", "exr", "hdr"], conflicts_with = "profile")]
    pub(crate) format: Option<String>,
    /// Bits per channel
    #[arg(long, value_name = "BITS", value_parser = depth)]
    pub(crate) depth: Option<u32>,
    /// Output size: pixels, or WIDTHxHEIGHT
    #[arg(long, value_name = "PX|WxH")]
    pub(crate) size: Option<Size>,
    /// File name pattern, such as `$(graph)_$(identifier)`
    #[arg(long, value_name = "PATTERN", conflicts_with = "profile")]
    pub(crate) pattern: Option<String>,
    /// A named profile: the maps, or a MaterialX material with its baked textures
    #[arg(long, value_enum)]
    pub(crate) profile: Option<Profile>,
    /// The convention of the input's normal map
    #[arg(long, value_enum, requires = "profile")]
    pub(crate) normal_format: Option<NormalFormat>,
    /// Scales signed MaterialX displacement (1)
    #[arg(long, value_name = "UNITS", requires = "profile", value_parser = finite)]
    pub(crate) displacement_scale: Option<f64>,
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("`{0}` is not 8, 16 or 32")]
pub(crate) struct BadDepth(String);

fn depth(s: &str) -> Result<u32, BadDepth> {
    s.parse::<u32>().ok().filter(|d| [8, 16, 32].contains(d)).ok_or_else(|| BadDepth(s.to_string()))
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("`{0}` is not a finite number")]
pub(crate) struct NotFinite(String);

fn finite(s: &str) -> Result<f64, NotFinite> {
    s.parse::<f64>().ok().filter(|n| n.is_finite()).ok_or_else(|| NotFinite(s.to_string()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum View {
    #[value(name = "3d")]
    ThreeD,
    #[value(name = "2d")]
    TwoD,
}

impl View {
    pub(crate) fn name(self) -> &'static str {
        match self {
            View::ThreeD => "3d",
            View::TwoD => "2d",
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct RenderArgs {
    #[arg(value_name = "INPUT")]
    pub(crate) input: PathBuf,
    #[arg(value_name = "OUT.PNG")]
    pub(crate) output: PathBuf,
    /// Image size
    #[arg(long, value_name = "WxH")]
    pub(crate) size: Option<Dimensions>,
    /// A 3D scene, or one node's output
    #[arg(long, value_enum)]
    pub(crate) view: Option<View>,
    /// The 3D scene (rounded_cube, cube, sphere, cylinder, plane, material_ball, ...)
    #[arg(long, value_name = "ID", value_parser = scene)]
    pub(crate) scene: Option<String>,
    /// 2D: the node whose output to show
    #[arg(long, value_name = "ID")]
    pub(crate) node: Option<u64>,
    /// Camera yaw in degrees
    #[arg(long, value_name = "DEG", allow_negative_numbers = true, value_parser = finite32)]
    pub(crate) yaw: Option<f32>,
    /// Camera pitch in degrees
    #[arg(long, value_name = "DEG", allow_negative_numbers = true, value_parser = finite32)]
    pub(crate) pitch: Option<f32>,
    #[command(flatten)]
    pub(crate) graph: GraphArgs,
    /// Give a graph input a value, `<id>=<value>` (repeatable)
    #[arg(long = "set", value_name = "ID=VALUE")]
    pub(crate) set: Vec<Assignment>,
    /// Cook at full size instead of the preview size
    #[arg(long)]
    pub(crate) full: bool,
    /// 3D: how often the UVs repeat
    #[arg(long, value_name = "N", value_parser = finite32)]
    pub(crate) tiling: Option<f32>,
    /// 3D: a beauty render with a finer mesh, ambient occlusion and a key light
    #[arg(long)]
    pub(crate) hq: bool,
    /// 3D, with --hq: displacement depth in scene radii (0.04)
    #[arg(long, value_name = "RADII", requires = "hq", value_parser = finite32)]
    pub(crate) displacement: Option<f32>,
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("`{0}` is not a finite number")]
pub(crate) struct NotFinite32(String);

fn finite32(s: &str) -> Result<f32, NotFinite32> {
    s.parse::<f32>().ok().filter(|n| n.is_finite()).ok_or_else(|| NotFinite32(s.to_string()))
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("`{name}` is not a scene (one of {})", known.join(", "))]
pub(crate) struct UnknownScene {
    name: String,
    known: Vec<&'static str>,
}

fn scene(s: &str) -> Result<String, UnknownScene> {
    let known: Vec<&'static str> = materialcraft_engine::view_cmds::SCENES.iter().map(|(id, _)| *id).collect();
    if known.contains(&s) { Ok(s.to_string()) } else { Err(UnknownScene { name: s.to_string(), known }) }
}

/// The folders a server may read from and write to; with neither, it has no file access.
#[derive(Args, Debug, Default)]
pub(crate) struct RootArgs {
    /// Serve file access below this folder (alias --automation-read-root)
    #[arg(long, alias = "automation-read-root", value_name = "DIR")]
    pub(crate) read_root: Option<PathBuf>,
    /// Allow writes below this folder (alias --automation-write-root)
    #[arg(long, alias = "automation-write-root", value_name = "DIR")]
    pub(crate) write_root: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub(crate) struct McpArgs {
    /// Bridge to a running app's control port instead of a headless session
    #[arg(long, value_name = "PORT")]
    pub(crate) bridge: Option<u16>,
    /// The app's control token (MATERIALCRAFT_CONTROL_TOKEN; else the file the app wrote)
    #[arg(long, value_name = "TOKEN", env = "MATERIALCRAFT_CONTROL_TOKEN", hide_env_values = true, requires = "bridge")]
    pub(crate) token: Option<String>,
    #[command(flatten)]
    pub(crate) roots: RootArgs,
}

#[derive(Args, Debug)]
pub(crate) struct ServeArgs {
    /// Listen on 127.0.0.1:<PORT> with a token (0 picks one), instead of stdio
    #[arg(long, value_name = "PORT")]
    pub(crate) port: Option<u16>,
    #[command(flatten)]
    pub(crate) roots: RootArgs,
}

/// A mistake in the command line that clap can't see by itself.
#[derive(Debug, PartialEq, thiserror::Error)]
pub(crate) enum UsageError {
    #[error("{output} {} is the {input}, which it would replace: choose another file{}", path.display(), if *in_place_helps { ", or pass --in-place" } else { "" })]
    WouldReplace { output: &'static str, input: &'static str, path: PathBuf, in_place_helps: bool },
    #[error("--in-place is for an -o that is the INPUT, and this is another file")]
    InPlaceNotUsed,
    #[error("{flag} is for a headless server, and --bridge connects to an app")]
    RootsWithBridge { flag: &'static str },
}

impl UsageError {
    /// The clap error, printed with the usage and exited with 2 like clap's own.
    pub(crate) fn into_clap(self, subcommand: &str) -> clap::Error {
        let mut command = crate::command();
        // Built, so a subcommand's usage names the program: `materialcraft-cli run`.
        command.build();
        let mut command = command.find_subcommand_mut(subcommand).map_or_else(crate::command, std::mem::take);
        command.error(ErrorKind::ArgumentConflict, self.to_string())
    }
}

/// Whether two paths name the same existing file, through links and `.` and `..`.
fn same_file(a: &Path, b: &Path) -> bool {
    matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

fn would_replace(output: &'static str, input: &'static str, to: &Path, from: &Path, in_place_helps: bool) -> Result<(), UsageError> {
    if same_file(to, from) { Err(UsageError::WouldReplace { output, input, path: to.to_owned(), in_place_helps }) } else { Ok(()) }
}

impl Command {
    /// The checks clap can't express: what would replace what, and flags the mode doesn't use.
    fn check(&self) -> Result<(), (&'static str, UsageError)> {
        match self {
            Command::Run(a) => match (&a.output, a.in_place) {
                (Some(out), false) => would_replace("-o", "<INPUT>", out, &a.input, true).map_err(|e| ("run", e)),
                (Some(out), true) if !same_file(out, &a.input) => Err(("run", UsageError::InPlaceNotUsed)),
                _ => Ok(()),
            },
            Command::Publish(a) => would_replace("<OUT.MCMAT>", "<PROJECT>", &a.output, &a.input, false).map_err(|e| ("publish", e)),
            Command::Render(a) => would_replace("<OUT.PNG>", "<INPUT>", &a.output, &a.input, false).map_err(|e| ("render", e)),
            Command::Mcp(a) if a.bridge.is_some() => match (&a.roots.read_root, &a.roots.write_root) {
                (Some(_), _) => Err(("mcp", UsageError::RootsWithBridge { flag: "--read-root" })),
                (_, Some(_)) => Err(("mcp", UsageError::RootsWithBridge { flag: "--write-root" })),
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }
}

/// Parses the arguments (without the program name). Help and the version come back as errors too,
/// which print to stdout and exit 0.
pub(crate) fn parse(args: &[OsString]) -> Result<Cli, clap::Error> {
    let matches = crate::command().try_get_matches_from(std::iter::once(OsString::from("materialcraft-cli")).chain(args.iter().cloned()))?;
    let cli = Cli::from_arg_matches(&matches)?;
    cli.command.check().map_err(|(sub, e)| e.into_clap(sub))?;
    Ok(cli)
}
