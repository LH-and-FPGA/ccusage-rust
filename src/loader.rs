//! Discover and stream-parse all JSONL files under the configured Claude
//! data directories.
//!
//! Two-pass design (matches ccusage):
//!   1. List `**/*.jsonl` under each `<base>/projects/`.
//!   2. For each file, read the *earliest* timestamp once and sort files by it
//!      so cross-file dedup processes older messages first.
//!   3. Stream-parse line by line, dropping invalid JSON / schema mismatches
//!      silently, deduping by `messageId:requestId`.

use anyhow::Result;
use jiff::Timestamp;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::schema::UsageEntry;

#[derive(Debug, Clone)]
pub struct LoadedEntry {
    pub entry: UsageEntry,
    pub project: String,
    pub session_id: String,
    pub source_file: PathBuf,
}

pub fn list_jsonl_files(claude_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for base in claude_paths {
        let projects = base.join("projects");
        if !projects.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&projects).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|s| s.to_str()) == Some("jsonl") {
                files.push(entry.into_path());
            }
        }
    }
    files
}

/// Cheaply scan a JSONL file for the earliest valid `timestamp`. Used only for
/// sort ordering; we don't validate the rest of the line.
pub fn earliest_timestamp(file: &Path) -> Option<Timestamp> {
    let f = File::open(file).ok()?;
    let reader = BufReader::new(f);
    let mut earliest: Option<Timestamp> = None;
    for line in reader.lines().map_while(|l| l.ok()) {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ts_str = v.get("timestamp").and_then(|t| t.as_str());
        let Some(ts_str) = ts_str else { continue };
        let Ok(ts) = ts_str.parse::<Timestamp>() else {
            continue;
        };
        earliest = Some(match earliest {
            None => ts,
            Some(prev) if ts < prev => ts,
            Some(prev) => prev,
        });
    }
    earliest
}

pub fn sort_files_by_earliest(files: &mut Vec<PathBuf>) {
    let mut keyed: Vec<(Option<Timestamp>, PathBuf)> = files
        .drain(..)
        .map(|p| (earliest_timestamp(&p), p))
        .collect();
    keyed.sort_by(|a, b| match (a.0, b.0) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, _) => std::cmp::Ordering::Greater,
        (_, None) => std::cmp::Ordering::Less,
        (Some(x), Some(y)) => x.cmp(&y),
    });
    *files = keyed.into_iter().map(|(_, p)| p).collect();
}

/// Stream-parse all files, deduping via `messageId:requestId` and dropping
/// entries with zero token activity (those are non-usage events like
/// permission-mode markers).
pub fn load_all(claude_paths: &[PathBuf]) -> Result<Vec<LoadedEntry>> {
    let mut files = list_jsonl_files(claude_paths);
    sort_files_by_earliest(&mut files);

    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<LoadedEntry> = Vec::new();

    for file in files {
        let project = crate::paths::project_from_path(&file);
        let session_id = crate::paths::session_id_from_path(&file).unwrap_or_default();
        let f = match File::open(&file) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(f);
        for line in reader.lines().map_while(|l| l.ok()) {
            if line.trim().is_empty() {
                continue;
            }
            let entry: UsageEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.is_empty() {
                continue;
            }
            if let Some(key) = entry.dedup_key() {
                if !seen.insert(key) {
                    continue;
                }
            }
            out.push(LoadedEntry {
                entry,
                project: project.clone(),
                session_id: session_id.clone(),
                source_file: file.clone(),
            });
        }
    }

    Ok(out)
}
