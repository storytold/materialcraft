use std::path::{Path, PathBuf};

use materialcraft_automation::AutomationError;

/// Why a subcommand failed: printed as `error: {e}`, exit 1.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    #[error(transparent)]
    Automation(#[from] AutomationError),
    /// A `--set` of `cook`, `export` or `render` the graph refused.
    #[error("--set {id}: {source}")]
    Set { id: String, source: AutomationError },
    /// The engine's commands take paths as JSON text, which holds only Unicode.
    #[error("{}: the path is not valid Unicode", .0.display())]
    NotUnicode(PathBuf),
    #[error("{}: {source}", path.display())]
    File { path: PathBuf, source: std::io::Error },
    #[error("script: {0}")]
    Script(serde_json::Error),
    #[error("the batch stopped at an error")]
    BatchStopped,
    #[error("{}: {source}", path.display())]
    Write { path: PathBuf, source: materialcraft_engine::formats::FormatError },
    #[error("{0}")]
    Serve(std::io::Error),
}

/// A path as the text the engine's commands take.
pub(crate) fn unicode(path: &Path) -> Result<&str, CliError> {
    path.to_str().ok_or_else(|| CliError::NotUnicode(path.to_owned()))
}
