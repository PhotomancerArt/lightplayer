//! [`UiAgentEditBatch`]: one `edit_project` call as the app chat's tool row
//! shows it — a one-line summary ("added Playlist + 4 patterns, set count
//! 250 and endpoint D6, saved") over an expandable list of the edits, each
//! with how it went. Rejections stay visible: they are counted in the
//! summary and spelled out in the list.
//!
//! Projected from the tool's summary JSON (`lpa_agent`'s `edit_project`
//! summary `rows`), which carries the facts; the words are Studio's, here.

use serde_json::Value;

/// One `edit_project` call's edits and outcomes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAgentEditBatch {
    /// The edits, in the order the agent sent them.
    pub lines: Vec<UiAgentEditLine>,
    /// The batch saved the project afterwards.
    pub saved: bool,
    /// The save was asked for and failed: why.
    pub save_error: Option<String>,
}

/// One edit in a batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAgentEditLine {
    /// The edit's key (`create_node`, `import_pattern`, `set`, …).
    pub verb: String,
    /// What it is about: a node kind, a pattern slug, a node name, a board.
    pub target: String,
    /// The playlist a create or import was attached to.
    pub within: Option<String>,
    /// The slot path (`set`, `ensure`, `remove`) or file (`set_asset`).
    pub path: Option<String>,
    /// A `set`'s value, as compact JSON text.
    pub value: Option<String>,
    pub outcome: UiAgentEditOutcome,
    /// The node the edit landed on, as its tree path
    /// (`/demo.module/fixture.fixture`), when the app said; `None` for an
    /// edit about no node (the board) or one that did not land.
    pub node: Option<String>,
    /// Where that node's card is, and its Show — decorated by the studio's
    /// view from the agent's activity (`None` until then).
    pub place: Option<crate::UiAgentPlace>,
}

/// How one edit went.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiAgentEditOutcome {
    /// It landed; what the app said changed.
    Applied { detail: String },
    /// The app refused it and nothing changed.
    Rejected { reason: String },
    /// Not attempted (an earlier create failed).
    Skipped { reason: String },
}

impl UiAgentEditBatch {
    /// The batch from an `edit_project` tool summary; `None` when the
    /// summary carries no edit rows (an input error, a host failure).
    pub fn from_summary(summary: &Value) -> Option<Self> {
        let rows = summary["rows"].as_array()?;
        let lines = rows.iter().map(UiAgentEditLine::from_summary_row).collect();
        Some(Self {
            lines,
            saved: summary["saved"].as_bool().unwrap_or(false),
            save_error: summary["save_error"].as_str().map(str::to_string),
        })
    }

    /// How many edits the app refused.
    pub fn rejected(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| matches!(line.outcome, UiAgentEditOutcome::Rejected { .. }))
            .count()
    }

    /// How many edits were never attempted.
    pub fn skipped(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| matches!(line.outcome, UiAgentEditOutcome::Skipped { .. }))
            .count()
    }

    /// Whether anything in the batch went wrong (a refused or skipped
    /// edit, or a failed save).
    pub fn has_problems(&self) -> bool {
        self.rejected() > 0 || self.skipped() > 0 || self.save_error.is_some()
    }

    /// The row's one line: what the applied edits amount to, then what
    /// went wrong. "added Playlist + 4 patterns, set count 250 and endpoint
    /// D6, set the board, saved"; "set count 250, 1 rejected".
    pub fn summary(&self) -> String {
        let applied: Vec<&UiAgentEditLine> = self
            .lines
            .iter()
            .filter(|line| matches!(line.outcome, UiAgentEditOutcome::Applied { .. }))
            .collect();
        let mut parts: Vec<String> = Vec::new();

        // Added: node kinds in order of first appearance, then patterns.
        let mut kinds: Vec<(String, usize)> = Vec::new();
        let mut patterns = 0usize;
        for line in &applied {
            match line.verb.as_str() {
                "create_node" => match kinds.iter_mut().find(|(kind, _)| *kind == line.target) {
                    Some((_, count)) => *count += 1,
                    None => kinds.push((line.target.clone(), 1)),
                },
                "import_pattern" => patterns += 1,
                _ => {}
            }
        }
        let mut added: Vec<String> = kinds
            .into_iter()
            .map(|(kind, count)| match count {
                1 => kind,
                many => format!("{many} {kind}s"),
            })
            .collect();
        match patterns {
            0 => {}
            1 => added.push("a pattern".to_string()),
            many => added.push(format!("{many} patterns")),
        }
        if !added.is_empty() {
            parts.push(format!("added {}", added.join(" + ")));
        }

        // Set: each value by its leaf name (and short value) while the
        // list stays short, a count past that.
        let set: Vec<String> = applied
            .iter()
            .filter(|line| matches!(line.verb.as_str(), "set" | "ensure"))
            .map(|line| line.set_phrase())
            .collect();
        match set.len() {
            0 => {}
            1..=3 => parts.push(format!("set {}", join_and(&set))),
            many => parts.push(format!("set {many} values")),
        }

        let removed: Vec<String> = applied
            .iter()
            .filter(|line| matches!(line.verb.as_str(), "remove_node" | "remove"))
            .map(|line| match line.verb.as_str() {
                "remove" => leaf(line.path.as_deref().unwrap_or(&line.target)).to_string(),
                _ => line.target.clone(),
            })
            .collect();
        match removed.len() {
            0 => {}
            1 => parts.push(format!("removed {}", removed[0])),
            many => parts.push(format!("removed {many}")),
        }

        let replaced: Vec<&str> = applied
            .iter()
            .filter(|line| line.verb == "set_asset")
            .map(|line| line.path.as_deref().unwrap_or(&line.target))
            .collect();
        match replaced.len() {
            0 => {}
            1 => parts.push(format!("replaced {}", replaced[0])),
            many => parts.push(format!("replaced {many} files")),
        }

        if applied.iter().any(|line| line.verb == "set_target") {
            parts.push("set the board".to_string());
        }
        if self.saved {
            parts.push("saved".to_string());
        }
        if parts.is_empty() {
            parts.push("no changes".to_string());
        }
        let rejected = self.rejected();
        if rejected > 0 {
            parts.push(format!("{rejected} rejected"));
        }
        let skipped = self.skipped();
        if skipped > 0 {
            parts.push(format!("{skipped} skipped"));
        }
        if self.save_error.is_some() {
            parts.push("not saved".to_string());
        }
        parts.join(", ")
    }
}

impl UiAgentEditLine {
    fn from_summary_row(row: &Value) -> Self {
        let text = |key: &str| row[key].as_str().map(str::to_string);
        let reason = text("reason").unwrap_or_default();
        let outcome = if row["ok"].as_bool().unwrap_or(false) {
            UiAgentEditOutcome::Applied {
                detail: text("detail").unwrap_or_default(),
            }
        } else if row["skipped"].as_bool().unwrap_or(false) {
            UiAgentEditOutcome::Skipped { reason }
        } else {
            UiAgentEditOutcome::Rejected { reason }
        };
        Self {
            verb: text("edit").unwrap_or_default(),
            target: text("target").unwrap_or_default(),
            within: text("in"),
            path: text("path"),
            value: match &row["value"] {
                Value::Null => None,
                value => Some(value.to_string()),
            },
            outcome,
            node: text("node"),
            place: None,
        }
    }

    /// The edit in words, for the expanded list: "import spiral into
    /// playlist", "set output ports[0].endpoint = "ws281x:local:D6"".
    pub fn text(&self) -> String {
        let within = self
            .within
            .as_deref()
            .map(|playlist| format!(" into {playlist}"))
            .unwrap_or_default();
        let path = self.path.as_deref().unwrap_or("");
        match self.verb.as_str() {
            "create_node" => format!("add a {}{within}", self.target),
            "import_pattern" => format!("import {}{within}", self.target),
            "remove_node" => format!("remove {}", self.target),
            "set" => match &self.value {
                Some(value) => format!("set {} {path} = {value}", self.target),
                None => format!("set {} {path}", self.target),
            },
            "ensure" => format!("add {} {path}", self.target),
            "remove" => format!("remove {} {path}", self.target),
            "set_asset" => format!("replace {} {path}", self.target),
            "set_target" => format!("set the board to {}", self.target),
            verb => format!("{verb} {}", self.target),
        }
    }

    /// One `set` in the summary: its leaf name, then its value when the
    /// value is short enough to read in passing ("count 250", "endpoint
    /// D6" — a colon-separated value shows its last part).
    fn set_phrase(&self) -> String {
        let name = leaf(self.path.as_deref().unwrap_or(&self.target));
        let short = self
            .value
            .as_deref()
            .and_then(|json| serde_json::from_str::<Value>(json).ok())
            .and_then(|value| match value {
                Value::String(text) => Some(text.rsplit(':').next().unwrap_or("").to_string()),
                Value::Number(number) => Some(number.to_string()),
                Value::Bool(flag) => Some(flag.to_string()),
                _ => None,
            })
            .filter(|text| !text.is_empty() && text.chars().count() <= 16);
        match short {
            Some(value) => format!("{name} {value}"),
            None => name.to_string(),
        }
    }
}

/// The last name in a slot path: `ports[0].endpoint` → `endpoint`.
fn leaf(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// `a`, `a and b`, `a, b and c`.
fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn seans_project_reads_as_one_line() {
        let batch = UiAgentEditBatch::from_summary(&json!({
            "saved": true,
            "rows": [
                { "edit": "create_node", "target": "Playlist", "ok": true, "detail": "playlist" },
                { "edit": "import_pattern", "target": "spiral", "in": "playlist", "ok": true },
                { "edit": "import_pattern", "target": "rainbow", "in": "playlist", "ok": true },
                { "edit": "import_pattern", "target": "fire", "in": "playlist", "ok": true },
                { "edit": "import_pattern", "target": "aurora", "in": "playlist", "ok": true },
                { "edit": "set", "target": "fixture", "path": "count", "value": 250, "ok": true },
                { "edit": "set", "target": "output", "path": "ports[0].endpoint",
                  "value": "ws281x:local:D6", "ok": true },
                { "edit": "set_target", "target": "seeed/xiao-esp32-c6", "ok": true }
            ]
        }))
        .expect("rows");
        assert_eq!(
            batch.summary(),
            "added Playlist + 4 patterns, set count 250 and endpoint D6, set the board, saved"
        );
        assert!(!batch.has_problems());
        assert_eq!(batch.lines[1].text(), "import spiral into playlist");
        assert_eq!(
            batch.lines[6].text(),
            "set output ports[0].endpoint = \"ws281x:local:D6\""
        );
    }

    #[test]
    fn rejections_are_counted_and_kept_out_of_what_landed() {
        let batch = UiAgentEditBatch::from_summary(&json!({
            "saved": false,
            "save_error": "the project has errors",
            "rows": [
                { "edit": "set", "target": "fixture", "path": "count", "value": 250, "ok": true },
                { "edit": "set", "target": "output", "path": "ports[0].endpoint",
                  "value": "ws281x:local:D99", "ok": false, "reason": "no pin D99" },
                { "edit": "import_pattern", "target": "x", "ok": false, "skipped": true,
                  "reason": "an earlier create failed" }
            ]
        }))
        .expect("rows");
        assert_eq!(
            batch.summary(),
            "set count 250, 1 rejected, 1 skipped, not saved"
        );
        assert!(batch.has_problems());
        assert_eq!(
            batch.lines[1].outcome,
            UiAgentEditOutcome::Rejected {
                reason: "no pin D99".into()
            }
        );
    }

    #[test]
    fn long_lists_count_and_one_pattern_reads_naturally() {
        let set = |path: &str| json!({ "edit": "set", "target": "n", "path": path, "ok": true });
        let batch = UiAgentEditBatch::from_summary(&json!({
            "rows": [
                set("a"), set("b"), set("c"), set("d"),
                { "edit": "import_pattern", "target": "spiral", "ok": true },
                { "edit": "create_node", "target": "Output", "ok": true },
                { "edit": "create_node", "target": "Output", "ok": true },
                { "edit": "remove_node", "target": "old", "ok": true }
            ]
        }))
        .expect("rows");
        assert_eq!(
            batch.summary(),
            "added 2 Outputs + a pattern, set 4 values, removed old"
        );
    }

    #[test]
    fn a_summary_without_rows_is_not_a_batch() {
        assert_eq!(
            UiAgentEditBatch::from_summary(&json!({ "input_error": true })),
            None
        );
        let empty = UiAgentEditBatch::from_summary(&json!({ "rows": [] })).expect("rows");
        assert_eq!(empty.summary(), "no changes");
    }
}
