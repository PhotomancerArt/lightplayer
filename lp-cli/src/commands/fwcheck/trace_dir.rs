use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Local;

pub struct TraceDir {
    pub dir: PathBuf,
    pub trace_txt: PathBuf,
    pub records_jsonl: PathBuf,
    pub report_txt: PathBuf,
}

pub fn create_trace_dir(target: &str, check: &str, note: Option<&str>) -> Result<TraceDir> {
    let timestamp = Local::now().format("%Y-%m-%dT%H-%M-%S");
    let mut name = format!("{timestamp}--{target}--{check}");
    if let Some(note) = note.and_then(sanitize_note) {
        name.push_str("--");
        name.push_str(&note);
    }
    let dir = PathBuf::from("traces").join(name);
    std::fs::create_dir_all(&dir).with_context(|| format!("create trace dir {}", dir.display()))?;
    Ok(TraceDir {
        trace_txt: dir.join("trace.txt"),
        records_jsonl: dir.join("records.jsonl"),
        report_txt: dir.join("report.txt"),
        dir,
    })
}

fn sanitize_note(note: &str) -> Option<String> {
    let mut out = String::new();
    for ch in note.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.' | ' ') && !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_owned();
    (!trimmed.is_empty()).then_some(trimmed)
}
