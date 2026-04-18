//! Discover Claude Code data directories.
//!
//! Mirrors `apps/ccusage/src/data-loader.ts` `getClaudePaths`:
//! - `CLAUDE_CONFIG_DIR` (comma-separated) takes precedence; if set but no path
//!   contains a `projects/` subdir, return an error.
//! - Otherwise check `$XDG_CONFIG_HOME/claude` (or `~/.config/claude`) and
//!   `~/.claude`, in that order, deduped by canonical path.
//! - A path is "valid" iff `<path>/projects` is a directory.

use anyhow::{anyhow, Result};
use std::env;
use std::path::{Path, PathBuf};

const PROJECTS_SUBDIR: &str = "projects";
const ENV_VAR: &str = "CLAUDE_CONFIG_DIR";

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn xdg_config_dir() -> Option<PathBuf> {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        let p = PathBuf::from(xdg);
        if !p.as_os_str().is_empty() {
            return Some(p);
        }
    }
    home_dir().map(|h| h.join(".config"))
}

fn is_valid(base: &Path) -> bool {
    base.join(PROJECTS_SUBDIR).is_dir()
}

fn push_unique(out: &mut Vec<PathBuf>, seen: &mut Vec<PathBuf>, p: PathBuf) {
    let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
    if !seen.contains(&canon) {
        seen.push(canon);
        out.push(p);
    }
}

pub fn discover() -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    if let Ok(env_paths) = env::var(ENV_VAR) {
        let trimmed = env_paths.trim();
        if !trimmed.is_empty() {
            for raw in trimmed.split(',') {
                let p = PathBuf::from(raw.trim());
                if p.as_os_str().is_empty() {
                    continue;
                }
                if is_valid(&p) {
                    push_unique(&mut out, &mut seen, p);
                }
            }
            if out.is_empty() {
                return Err(anyhow!(
                    "{ENV_VAR} is set ({env_paths}) but none of the paths contain a `{PROJECTS_SUBDIR}/` subdir"
                ));
            }
            return Ok(out);
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(xdg) = xdg_config_dir() {
        candidates.push(xdg.join("claude"));
    }
    if let Some(home) = home_dir() {
        candidates.push(home.join(".claude"));
    }

    for c in candidates {
        if is_valid(&c) {
            push_unique(&mut out, &mut seen, c);
        }
    }

    if out.is_empty() {
        return Err(anyhow!(
            "no Claude data directories found. Tried `~/.config/claude/{PROJECTS_SUBDIR}` and `~/.claude/{PROJECTS_SUBDIR}`. Set {ENV_VAR} to override."
        ));
    }
    Ok(out)
}

/// Extract the project directory name from a JSONL file path.
/// Path shape: `<base>/projects/<project>/.../<sessionId>.jsonl`.
pub fn project_from_path(jsonl_path: &Path) -> String {
    let mut parts = jsonl_path.components().peekable();
    while let Some(c) = parts.next() {
        if c.as_os_str() == PROJECTS_SUBDIR {
            if let Some(next) = parts.next() {
                let s = next.as_os_str().to_string_lossy().to_string();
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    "unknown".to_string()
}

/// Extract the session id from a JSONL file path: `<...>/<sessionId>.jsonl`.
pub fn session_id_from_path(jsonl_path: &Path) -> Option<String> {
    jsonl_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
}
