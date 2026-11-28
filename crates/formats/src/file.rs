#[cfg(not(target_arch = "wasm32"))]
use crate::{FormatError, Result};

pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base.chars().map(|c| if c.is_alphanumeric() || "._- ".contains(c) { c } else { '_' }).collect();
    let cleaned = cleaned.trim_start_matches('.').to_string();
    if cleaned.is_empty() { "resource".into() } else { cleaned }
}

/// The file stem of an exported output: `pattern` with `$(graph)`, `$(identifier)` and `$(usage)` replaced, sanitized for file systems
pub fn output_name(pattern: &str, graph: &str, identifier: &str, usage: &str) -> String {
    let tokens = [("graph)", graph), ("identifier)", identifier), ("usage)", usage)];
    let mut stem = String::with_capacity(pattern.len());
    let mut rest = pattern;
    while let Some((literal, tail)) = rest.split_once("$(") {
        stem.push_str(literal);
        if let Some((remaining, value)) = tokens.iter().find_map(|&(token, value)| tail.strip_prefix(token).map(|remaining| (remaining, value))) {
            stem.push_str(value);
            rest = remaining;
        } else {
            stem.push_str("$(");
            rest = tail;
        }
    }
    stem.push_str(rest);
    safe_file_name(&stem)
}

/// Write `bytes` to `path` through a temporary file and a rename, so a failed write never leaves a truncated file behind
#[cfg(not(target_arch = "wasm32"))]
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("out");
    let mut file = None;
    let mut tmp = std::path::PathBuf::new();
    for attempt in 0..100u32 {
        tmp = parent.join(format!(".{name}.{}.{attempt}.tmp", std::process::id()));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(opened) => {
                file = Some(opened);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(FormatError::Io(format!("{}: {e}", tmp.display()))),
        }
    }
    let Some(mut file) = file else {
        return Err(FormatError::Io(format!("{}: could not create a temporary file", path.display())));
    };
    let res = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    let res = res.and_then(|_| std::fs::rename(&tmp, path));
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res.map_err(|e| FormatError::Io(format!("{}: {e}", path.display())))
}

#[cfg(all(test, unix))]
mod tests {
    use super::write_atomic;

    #[test]
    fn write_atomic_does_not_follow_a_planted_temp_symlink() -> std::io::Result<()> {
        let dir = std::env::temp_dir().join(format!("mc-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let target = dir.join("out.png");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, b"safe")?;
        std::os::unix::fs::symlink(&victim, dir.join("out.png.tmp"))?;
        write_atomic(&target, b"new").map_err(std::io::Error::other)?;
        assert_eq!(std::fs::read(&victim)?, b"safe");
        assert_eq!(std::fs::read(&target)?, b"new");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
