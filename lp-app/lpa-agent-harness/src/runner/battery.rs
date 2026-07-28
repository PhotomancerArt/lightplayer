//! The supervised prompt battery: file format, light per-run expectation
//! checks, and the summary table.
//!
//! `battery.json` (crate root) carries ~10 prompts spanning the classes
//! that found compiler bugs live: physics/state-in-loops, array-heavy,
//! builtin-heavy, branchy, param-declaring, free-creative. The
//! `agent-battery` bin loops them through the session runner — spend-gated
//! exactly like `agent-run` (BOTH provider env AND `LPA_SPEND_OK=1`;
//! nothing automated ever sets it) — and renders one row per prompt.
//!
//! Expectations are deliberately LIGHT (this is a fuzzer, not a quality
//! eval — quality is `eval_tasks`' job): the shader compiles, the engine
//! verdict is ok, the params section is clean, and the output is not
//! black. Everything here is pure over the parsed [`Dump`], so it is
//! unit-tested without spending a token.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::dump::Dump;

/// The battery file: a list of prompts with their bug-hunting class.
#[derive(Debug, Deserialize)]
pub struct BatteryFile {
    /// Free-text header (ignored by the runner).
    #[serde(default)]
    pub comment: Option<String>,
    pub prompts: Vec<BatteryPrompt>,
}

/// One battery prompt.
#[derive(Debug, Deserialize)]
pub struct BatteryPrompt {
    /// Stable id — names the per-prompt run dir.
    pub id: String,
    /// The bug-hunting class this prompt exercises.
    pub class: String,
    pub prompt: String,
}

impl BatteryFile {
    /// The checked-in battery next to this crate's manifest.
    pub fn default_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("battery.json")
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| format!("invalid battery JSON: {error}"))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::parse(&json)
    }
}

/// The light per-run expectation checks. `None` means the expectation was
/// not observable (e.g. the session never ran an `iterate`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExpectationReport {
    /// The last probed compile succeeded.
    pub compiles: Option<bool>,
    /// The engine accepted the final staged source (the live verdict, not
    /// the probe world's).
    pub engine_ok: Option<bool>,
    /// The final params section has no orphans in either direction.
    pub params_clean: Option<bool>,
    /// The health report says the output is not (near-)black.
    pub non_black: Option<bool>,
}

impl ExpectationReport {
    /// True only when every expectation was observed AND passed.
    pub fn all_pass(&self) -> bool {
        [self.compiles, self.engine_ok, self.params_clean, self.non_black]
            .iter()
            .all(|check| *check == Some(true))
    }
}

/// Evaluate one run's expectations from its debug dump: the LAST iterate
/// result (the session's final state) provides compile/params/health; the
/// last staged edit's engine verdict wins over the in-call engine section
/// (it reflects the final repair, not an intermediate).
pub fn evaluate_run(dump: &Dump) -> ExpectationReport {
    let mut report = ExpectationReport::default();
    let Some(result) = last_iterate_result(dump) else {
        return report;
    };
    let compiles = result["shader"] == "ok";
    report.compiles = Some(compiles);
    report.engine_ok = dump
        .edits
        .last()
        .and_then(|edit| edit.engine_ok)
        .or_else(|| {
            result
                .get("engine")
                .and_then(|engine| engine.get("status"))
                .map(|status| status == "ok")
        });
    if let Some(orphans) = result.pointer("/params/orphans") {
        report.params_clean = Some(
            orphans["declared_only"].as_array().is_some_and(Vec::is_empty)
                && orphans["def_only"].as_array().is_some_and(Vec::is_empty),
        );
    }
    if let Some(near_black) = result
        .pointer("/health/near_black_fraction")
        .and_then(Value::as_f64)
    {
        report.non_black = Some(near_black < 0.999);
    }
    report
}

/// The last tool-result JSON carrying a `shader` key (an `iterate` result).
fn last_iterate_result(dump: &Dump) -> Option<Value> {
    dump.messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|block| match block {
            lpa_agent::ContentBlock::ToolResult { content, .. } => {
                serde_json::from_str::<Value>(content).ok()
            }
            _ => None,
        })
        .filter(|value| value.get("shader").is_some())
        .next_back()
}

/// One summary-table row (the bin fills these as runs finish).
pub struct BatteryRow {
    pub id: String,
    pub class: String,
    /// `idle`, or the run/session error.
    pub status: String,
    pub report: ExpectationReport,
    /// Matched triage lines (full text; the table shows the class tags).
    pub triage: Vec<String>,
    /// Display cost estimate (e.g. `~$0.0341`).
    pub cost: Option<String>,
}

/// Render the battery summary as a markdown table plus totals.
pub fn render_summary(rows: &[BatteryRow]) -> String {
    let mut out = String::from(
        "| prompt | class | compiles | engine | params | non-black | triage | cost |\n\
         |---|---|---|---|---|---|---|---|\n",
    );
    for row in rows {
        let triage = if row.status != "idle" {
            "run failed".to_string()
        } else if row.triage.is_empty() {
            "clean".to_string()
        } else {
            row.triage
                .iter()
                .map(|line| class_tag(line))
                .collect::<Vec<_>>()
                .join(" ")
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            row.id,
            row.class,
            cell(row.report.compiles),
            cell(row.report.engine_ok),
            cell(row.report.params_clean),
            cell(row.report.non_black),
            triage,
            row.cost.as_deref().unwrap_or("—"),
        ));
    }
    let passed = rows.iter().filter(|row| row.report.all_pass()).count();
    let hits: usize = rows.iter().map(|row| row.triage.len()).sum();
    out.push_str(&format!(
        "\n{passed}/{} prompts passed every expectation · {hits} triage hit(s)",
        rows.len()
    ));
    if let Some(total) = total_cost_usd(rows) {
        out.push_str(&format!(" · total est. ~${total:.4}"));
    }
    out.push('\n');
    out
}

fn cell(check: Option<bool>) -> &'static str {
    match check {
        Some(true) => "pass",
        Some(false) => "FAIL",
        None => "—",
    }
}

/// The `[class]` tag opening a triage line (or its first word as fallback).
fn class_tag(line: &str) -> String {
    match (line.find('['), line.find(']')) {
        (Some(open), Some(close)) if open < close => line[open..=close].to_string(),
        _ => line.split_whitespace().next().unwrap_or("?").to_string(),
    }
}

/// Sum the parsable `~$x.xxxx` per-run estimates; `None` if no row has one.
fn total_cost_usd(rows: &[BatteryRow]) -> Option<f64> {
    let costs: Vec<f64> = rows
        .iter()
        .filter_map(|row| row.cost.as_deref())
        .filter_map(|cost| cost.split('$').nth(1)?.parse::<f64>().ok())
        .collect();
    if costs.is_empty() {
        None
    } else {
        Some(costs.iter().sum())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::dump::parse_dump;

    /// The CHECKED-IN battery file must parse and span the bug-finding
    /// classes the phase calls for.
    #[test]
    fn checked_in_battery_parses_and_spans_the_classes() {
        let battery = BatteryFile::parse(include_str!("../../battery.json")).expect("parses");
        assert!(
            (9..=14).contains(&battery.prompts.len()),
            "~10 prompts, got {}",
            battery.prompts.len()
        );
        for class in [
            "physics-state-in-loops",
            "array-heavy",
            "builtin-heavy",
            "branchy",
            "param-declaring",
            "free-creative",
        ] {
            assert!(
                battery.prompts.iter().any(|p| p.class == class),
                "missing class {class}"
            );
        }
        // Ids are unique (they name run dirs).
        let mut ids: Vec<&str> = battery.prompts.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), battery.prompts.len(), "duplicate prompt ids");
    }

    fn dump_with(results: &[Value], engine_ok: Option<bool>) -> Dump {
        let messages: Vec<Value> = results
            .iter()
            .enumerate()
            .map(|(i, content)| {
                json!({ "role": "user", "content": [
                    { "type": "tool_result", "tool_use_id": format!("tu_{i}"),
                      "content": content.to_string() },
                ] })
            })
            .collect();
        let edits: Vec<Value> = engine_ok
            .map(|ok| vec![json!({ "turn": 1, "note": null, "engine_ok": ok, "source": "s" })])
            .unwrap_or_default();
        parse_dump(
            &json!({
                "format": 1, "artifact": "shader.glsl", "provider": "anthropic",
                "model": "m", "usage_total": { "input_tokens": 0, "output_tokens": 0 },
                "turns": [], "edits": edits, "messages": messages,
            })
            .to_string(),
        )
        .expect("dump parses")
    }

    #[test]
    fn healthy_final_iterate_passes_every_expectation() {
        let dump = dump_with(
            &[json!({
                "shader": "ok",
                "params": { "orphans": { "declared_only": [], "def_only": [] } },
                "health": { "near_black_fraction": 0.12 },
                "engine": { "status": "ok" },
            })],
            Some(true),
        );
        let report = evaluate_run(&dump);
        assert_eq!(
            report,
            ExpectationReport {
                compiles: Some(true),
                engine_ok: Some(true),
                params_clean: Some(true),
                non_black: Some(true),
            }
        );
        assert!(report.all_pass());
    }

    #[test]
    fn the_last_iterate_result_wins_and_failures_are_flagged() {
        // First result healthy, LAST one broken + black + orphaned — the
        // final state is what the battery judges.
        let dump = dump_with(
            &[
                json!({ "shader": "ok",
                    "params": { "orphans": { "declared_only": [], "def_only": [] } },
                    "health": { "near_black_fraction": 0.0 } }),
                json!({ "shader": { "err": { "diagnostics": [] } },
                    "params": { "orphans": { "declared_only": ["speed"], "def_only": [] } },
                    "health": { "near_black_fraction": 1.0 } }),
            ],
            Some(false),
        );
        let report = evaluate_run(&dump);
        assert_eq!(report.compiles, Some(false));
        assert_eq!(report.engine_ok, Some(false));
        assert_eq!(report.params_clean, Some(false));
        assert_eq!(report.non_black, Some(false));
        assert!(!report.all_pass());
    }

    #[test]
    fn a_session_without_iterate_results_observes_nothing() {
        let dump = dump_with(&[], None);
        let report = evaluate_run(&dump);
        assert_eq!(report, ExpectationReport::default());
        assert!(!report.all_pass(), "unobserved is not a pass");
    }

    #[test]
    fn engine_verdict_prefers_the_last_edit_record() {
        // The in-call engine section said error (mid-repair), but the last
        // edit's resolved verdict is ok — the edit record wins.
        let dump = dump_with(
            &[json!({ "shader": "ok", "engine": { "status": "error" } })],
            Some(true),
        );
        assert_eq!(evaluate_run(&dump).engine_ok, Some(true));
    }

    #[test]
    fn summary_table_renders_rows_totals_and_triage_tags() {
        let rows = vec![
            BatteryRow {
                id: "bouncing-balls".into(),
                class: "physics-state-in-loops".into(),
                status: "idle".into(),
                report: ExpectationReport {
                    compiles: Some(true),
                    engine_ok: Some(true),
                    params_clean: Some(true),
                    non_black: Some(true),
                },
                triage: vec![],
                cost: Some("~$0.0316".into()),
            },
            BatteryRow {
                id: "step-waves".into(),
                class: "builtin-heavy".into(),
                status: "idle".into(),
                report: ExpectationReport {
                    compiles: Some(true),
                    engine_ok: Some(false),
                    params_clean: None,
                    non_black: Some(true),
                },
                triage: vec!["[wasm-stack-leak] Q32 inline-emit ...".into()],
                cost: Some("~$0.0208".into()),
            },
        ];
        let table = render_summary(&rows);
        assert!(table.contains("| bouncing-balls | physics-state-in-loops | pass | pass | pass | pass | clean | ~$0.0316 |"), "{table}");
        assert!(
            table.contains("| step-waves | builtin-heavy | pass | FAIL | — | pass | [wasm-stack-leak] | ~$0.0208 |"),
            "{table}"
        );
        assert!(table.contains("1/2 prompts passed"), "{table}");
        assert!(table.contains("1 triage hit(s)"), "{table}");
        assert!(table.contains("total est. ~$0.0524"), "{table}");
    }
}
