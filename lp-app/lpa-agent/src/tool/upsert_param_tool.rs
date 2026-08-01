//! The `upsert_param` tool: create or update one f32 param def record.
//!
//! Its own tool, not an `iterate` argument — a write op deserves an
//! explicit call. Execution order: pre-check the name against the declared
//! uniform set (fresh compile of the current source) and the def record
//! set → dispatch the upsert through the host's Save-gated overlay path →
//! bounded engine-verdict wait (a def change flips the node's
//! needs-compile, so the engine's fresh verdict is the real outcome) →
//! report `{applied, param, engine}`. Bad names are DATA (actionable
//! in-band errors); `is_error` is reserved for host failures, exactly like
//! `iterate`.

use lps_probe::{CompiledShader, RESERVED_UNIFORM, round_sig4, top_level_uniform_names};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::provider::model_provider::ToolDef;
use crate::tool::iterate_host::{AgentHost, ParamUpsert};
use crate::tool::iterate_tool::{ENGINE_VERDICT_BUDGET_MS, IterateOutcome, yield_to_ui};
use crate::tool::tool_phase::ToolPhase;

/// The tool's wire name.
pub const UPSERT_PARAM_TOOL_NAME: &str = "upsert_param";

/// Parsed `upsert_param` input. Unknown fields are rejected so the model
/// gets an actionable error instead of a silently ignored argument.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpsertParamInput {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub default: Option<f32>,
    #[serde(default)]
    pub min: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
    #[serde(default)]
    pub step: Option<f32>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub panel: Option<bool>,
}

impl UpsertParamInput {
    fn into_upsert(self) -> ParamUpsert {
        ParamUpsert {
            name: self.name,
            label: self.label,
            default: self.default,
            min: self.min,
            max: self.max,
            step: self.step,
            unit: self.unit,
            panel: self.panel,
        }
    }
}

/// Execute one `upsert_param` call. Same outcome/phase contract as
/// [`crate::tool::iterate_tool::run_iterate`].
pub async fn run_upsert_param(
    input_json: &Value,
    host: &mut dyn AgentHost,
    progress: &mut dyn FnMut(ToolPhase),
) -> IterateOutcome {
    let input: UpsertParamInput = match serde_json::from_value(input_json.clone()) {
        Ok(input) => input,
        Err(e) => {
            return data_outcome(
                json!({ "input_error": format!("invalid upsert_param input: {e}") }),
                json!({ "input_error": true }),
            );
        }
    };
    let upsert = input.into_upsert();
    let name = upsert.name.clone();

    if name == RESERVED_UNIFORM {
        return data_outcome(
            json!({ "error": format!(
                "`{RESERVED_UNIFORM}` is engine-managed (written every frame); it takes no param record"
            ) }),
            json!({ "note": format!("param `{name}`"), "error": "reserved uniform" }),
        );
    }

    // Pre-check: the name must be a declared uniform (fresh compile of the
    // current source) or an existing def record — anything else is a typo
    // or a not-yet-declared uniform, reported in-band with both sets.
    progress(ToolPhase::Compiling);
    yield_to_ui().await;
    let (declared, source_compiles): (Vec<String>, bool) = match host.current_source() {
        Ok(source) => match CompiledShader::compile(&source) {
            Ok(compiled) => (top_level_uniform_names(compiled.signatures()), true),
            // The staged source often does NOT compile yet at upsert time
            // (declare uniform → upsert record → fix the code is a normal
            // repair order): fall back to a textual declaration scan so a
            // broken compile never blocks creating the record.
            Err(_) => (textual_uniform_names(&source), false),
        },
        Err(e) => return host_error(format!("reading current source failed: {}", e.message)),
    };
    let defs = host.shader_params().await.unwrap_or_default();
    let known_def = defs.iter().any(|def| def.name == name);
    if !declared.contains(&name) && !known_def {
        let def_names: Vec<&str> = defs.iter().map(|def| def.name.as_str()).collect();
        let compile_note = if source_compiles {
            ""
        } else {
            " (note: the current source does not compile; declared names were scanned textually)"
        };
        return data_outcome(
            json!({ "error": format!(
                "`{name}` is neither a declared uniform nor an existing param record — \
                 declare the uniform first (or fix the name){compile_note}. \
                 declared: {declared:?}, def records: {def_names:?}"
            ) }),
            json!({ "note": format!("param `{name}`"), "error": "unknown param name" }),
        );
    }

    progress(ToolPhase::Staging);
    yield_to_ui().await;
    if let Err(e) = host.upsert_param(&upsert).await {
        return host_error(format!("param upsert failed: {}", e.message));
    }

    // The def change flips the node's needs-compile: the SAME verdict
    // window as a staged source edit reports the post-upsert engine state.
    progress(ToolPhase::AwaitingEngine);
    yield_to_ui().await;
    let engine = host.await_engine_verdict(ENGINE_VERDICT_BUDGET_MS).await;

    progress(ToolPhase::Finishing);
    yield_to_ui().await;

    let mut content = serde_json::Map::new();
    content.insert("applied".into(), json!(true));
    content.insert("param".into(), upsert_echo(&upsert));
    let mut summary = json!({
        "note": format!("param `{name}`"),
        "applied": true,
        "staged": true,
    });
    if let Some(verdict) = &engine {
        let engine = crate::tool::iterate_tool::engine_json(verdict);
        content.insert("engine".into(), engine.clone());
        summary
            .as_object_mut()
            .expect("summary is an object")
            .insert("engine".into(), engine);
    }
    IterateOutcome {
        content: Value::Object(content).to_string(),
        is_error: false,
        summary,
    }
}

/// Lightweight declaration scan for source that does not compile: every
/// identifier following `uniform <type>` (precision qualifiers skipped).
/// Good enough for the pre-check; the compiled signature stays the truth
/// whenever the source compiles.
fn textual_uniform_names(source: &str) -> Vec<String> {
    let source = strip_comments(source);
    let mut names = Vec::new();
    let mut tokens = source
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty());
    while let Some(token) = tokens.next() {
        if token != "uniform" {
            continue;
        }
        // <precision?> <type> <name>
        let mut next = tokens.next();
        if matches!(next, Some("highp" | "mediump" | "lowp")) {
            next = tokens.next();
        }
        if next.is_none() {
            break;
        }
        if let Some(name) = tokens.next() {
            names.push(name.to_string());
        }
    }
    names
}

/// Drop `//` line and `/* */` block comments (the word `uniform` in prose
/// must not fool the declaration scan).
fn strip_comments(source: &str) -> String {
    let mut cleaned = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '/' {
            cleaned.push(c);
            continue;
        }
        match chars.peek() {
            Some('/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        cleaned.push('\n');
                        break;
                    }
                }
            }
            Some('*') => {
                chars.next();
                let mut prev = ' ';
                for next in chars.by_ref() {
                    if prev == '*' && next == '/' {
                        break;
                    }
                    prev = next;
                }
                cleaned.push(' ');
            }
            _ => cleaned.push(c),
        }
    }
    cleaned
}

/// Echo the written fields (rounded), so the transcript records what landed.
fn upsert_echo(upsert: &ParamUpsert) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("name".into(), json!(upsert.name));
    if let Some(label) = &upsert.label {
        obj.insert("label".into(), json!(label));
    }
    for (key, value) in [
        ("default", upsert.default),
        ("min", upsert.min),
        ("max", upsert.max),
        ("step", upsert.step),
    ] {
        if let Some(value) = value {
            obj.insert(key.into(), json!(round_sig4(value)));
        }
    }
    if let Some(unit) = &upsert.unit {
        obj.insert("unit".into(), json!(unit));
    }
    if let Some(panel) = upsert.panel {
        obj.insert("panel".into(), json!(panel));
    }
    Value::Object(obj)
}

/// The tool definition offered to the model every turn.
pub fn upsert_param_tool_def() -> ToolDef {
    ToolDef {
        name: UPSERT_PARAM_TOOL_NAME.into(),
        description: DESCRIPTION.into(),
        input_schema: input_schema(),
    }
}

const DESCRIPTION: &str = "\
Create or update the def-side param record for one float uniform: label, \
default value, min/max range, an optional `step` the knob snaps to, display \
unit, and the `panel` flag that puts a knob on the node card. Use it to \
repair a `declared_only` orphan from \
`iterate`'s params section (the engine cannot render a declared uniform \
without a record) or to polish an existing record. Only the fields you pass \
are written; `name` must match a declared uniform or an existing record. \
f32 params only. The edit lands as an unsaved change exactly like staged \
source — the user Saves or reverts it. Returns the engine's post-update \
verdict.";

fn input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name"],
        "properties": {
            "name": { "type": "string",
                "description": "The uniform name (a declared uniform or an existing record)." },
            "label": { "type": "string",
                "description": "Human-readable display label." },
            "default": { "type": "number",
                "description": "Authored default value (inert while the param is bus-bound)." },
            "min": { "type": "number", "description": "Range minimum (knob/slider lower bound)." },
            "max": { "type": "number", "description": "Range maximum (knob/slider upper bound)." },
            "step": { "type": "number",
                "description": "Knob quantization: values snap to whole multiples of this. Pass 1 for a whole-number param (\"how many\"); omit for a continuous one." },
            "unit": { "type": "string",
                "description": "Display unit suffix (e.g. \"Hz\", \"%\")." },
            "panel": { "type": "boolean",
                "description": "Show this param as a knob on the node card's front panel." }
        }
    })
}

fn data_outcome(content: Value, summary: Value) -> IterateOutcome {
    IterateOutcome {
        content: content.to_string(),
        is_error: false,
        summary,
    }
}

fn host_error(message: String) -> IterateOutcome {
    IterateOutcome {
        content: json!({ "error": &message }).to_string(),
        is_error: true,
        summary: json!({ "error": message }),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use lps_probe::LedPoint;

    use super::*;
    use crate::tool::iterate_host::{
        EngineStatusKind, EngineVerdict, HostError, HostFuture, ParamDefRecord, ShaderContext,
    };

    const SPEED_SHADER: &str = "layout(binding = 0) uniform float speed;\n\
         vec4 render(vec2 pos) { return vec4(fract(speed), 0.0, 0.0, 1.0); }";

    fn run(input: &Value, host: &mut FakeHost) -> IterateOutcome {
        futures_executor::block_on(run_upsert_param(input, host, &mut |_| {}))
    }

    #[test]
    fn schema_requires_name_and_rejects_unknown_fields() {
        let def = upsert_param_tool_def();
        assert_eq!(def.name, "upsert_param");
        assert_eq!(def.input_schema["required"], json!(["name"]));
        assert_eq!(def.input_schema["additionalProperties"], false);

        let input: UpsertParamInput = serde_json::from_value(json!({
            "name": "speed", "label": "Speed", "default": 1.0,
            "min": 0.0, "max": 4.0, "unit": "x", "panel": true,
        }))
        .expect("full input parses");
        assert_eq!(input.name, "speed");
        assert_eq!(input.panel, Some(true));

        let sparse: UpsertParamInput =
            serde_json::from_value(json!({ "name": "speed" })).expect("name-only input parses");
        assert_eq!(sparse.label, None);

        serde_json::from_value::<UpsertParamInput>(json!({ "name": "s", "lable": "typo" }))
            .expect_err("unknown fields are rejected");
        serde_json::from_value::<UpsertParamInput>(json!({ "label": "no name" }))
            .expect_err("name is required");
    }

    #[test]
    fn declared_uniform_upserts_and_reports_the_engine_verdict() {
        let mut host = FakeHost::new(SPEED_SHADER);
        host.verdict = Some(EngineVerdict {
            status: EngineStatusKind::Ok,
            message: None,
            line_col: None,
        });
        let mut phases = Vec::new();
        let outcome = futures_executor::block_on(run_upsert_param(
            &json!({ "name": "speed", "label": "Speed", "default": 1.0, "panel": true }),
            &mut host,
            &mut |phase| phases.push(phase),
        ));
        assert!(!outcome.is_error);
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["applied"], true);
        assert_eq!(content["param"]["name"], "speed");
        assert_eq!(content["param"]["label"], "Speed");
        assert!(
            content["param"].get("min").is_none(),
            "unset fields omitted"
        );
        assert_eq!(content["engine"]["status"], "ok");
        assert_eq!(outcome.summary["staged"], true);
        assert_eq!(outcome.summary["note"], "param `speed`");
        // The host saw exactly one upsert, and the verdict wait used the
        // staged-edit budget.
        assert_eq!(host.upserts.borrow().len(), 1);
        assert_eq!(host.upserts.borrow()[0].name, "speed");
        assert_eq!(
            host.verdict_budgets.borrow().as_slice(),
            [ENGINE_VERDICT_BUDGET_MS]
        );
        assert_eq!(
            phases,
            vec![
                ToolPhase::Compiling,
                ToolPhase::Staging,
                ToolPhase::AwaitingEngine,
                ToolPhase::Finishing,
            ]
        );
    }

    #[test]
    fn unknown_name_is_an_actionable_in_band_error() {
        let mut host = FakeHost::new(SPEED_SHADER);
        host.params = Some(vec![ParamDefRecord {
            name: "brightness".into(),
            ..ParamDefRecord::default()
        }]);
        let outcome = run(&json!({ "name": "sped" }), &mut host);
        assert!(!outcome.is_error, "bad names are data");
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        let error = content["error"].as_str().expect("error");
        assert!(error.contains("`sped`"), "{error}");
        assert!(error.contains("speed"), "declared set listed: {error}");
        assert!(error.contains("brightness"), "def set listed: {error}");
        assert!(host.upserts.borrow().is_empty(), "nothing dispatched");
    }

    #[test]
    fn def_only_names_pass_the_precheck() {
        // The record exists but the uniform is (currently) undeclared —
        // polishing a stale record is allowed.
        let mut host = FakeHost::new("vec4 render(vec2 pos) { return vec4(1.0); }");
        host.params = Some(vec![ParamDefRecord {
            name: "legacy".into(),
            ..ParamDefRecord::default()
        }]);
        let outcome = run(&json!({ "name": "legacy", "label": "Legacy" }), &mut host);
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["applied"], true, "{content}");
        assert_eq!(host.upserts.borrow().len(), 1);
    }

    #[test]
    fn broken_compile_falls_back_to_the_textual_declaration_scan() {
        // The live-reported blocker: staged source declares new uniforms
        // but does NOT compile (naga limitation mid-repair) — the upsert
        // must still accept the declared names.
        let mut host = FakeHost::new(
            "layout(binding = 4) uniform float gravity;\n\
             layout(binding = 5) uniform highp float ballCount;\n\
             vec4 render(vec2 pos) { this does not compile }",
        );
        let outcome = run(&json!({ "name": "gravity", "default": 1.0 }), &mut host);
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["applied"], true, "{content}");
        assert_eq!(host.upserts.borrow().len(), 1);

        let refused = run(&json!({ "name": "sizeVariance" }), &mut host);
        let content: Value = serde_json::from_str(&refused.content).expect("json");
        let error = content["error"].as_str().expect("error");
        assert!(error.contains("does not compile"), "{error}");
        assert!(error.contains("ballCount"), "scanned set listed: {error}");
    }

    #[test]
    fn textual_scan_reads_declarations_only() {
        let names = textual_uniform_names(
            "layout(binding = 0) uniform float speed;\n\
             uniform highp vec2 wind; // uniform in a comment\n\
             float not_a_uniform;",
        );
        assert_eq!(names, vec!["speed".to_string(), "wind".to_string()]);
    }

    #[test]
    fn reserved_output_size_is_refused() {
        let mut host = FakeHost::new(SPEED_SHADER);
        let outcome = run(&json!({ "name": "outputSize" }), &mut host);
        assert!(!outcome.is_error);
        assert!(
            outcome.content.contains("engine-managed"),
            "{}",
            outcome.content
        );
        assert!(host.upserts.borrow().is_empty());
    }

    #[test]
    fn host_refusal_is_an_error_result() {
        let mut host = FakeHost::new(SPEED_SHADER);
        host.fail_upsert = true;
        let outcome = run(&json!({ "name": "speed" }), &mut host);
        assert!(outcome.is_error);
        assert!(outcome.content.contains("param upsert failed"));
    }

    // -- helpers ----------------------------------------------------------

    struct FakeHost {
        source: String,
        params: Option<Vec<ParamDefRecord>>,
        upserts: RefCell<Vec<ParamUpsert>>,
        fail_upsert: bool,
        verdict: Option<EngineVerdict>,
        verdict_budgets: RefCell<Vec<u32>>,
    }

    impl FakeHost {
        fn new(source: &str) -> Self {
            Self {
                source: source.to_string(),
                params: Some(Vec::new()),
                upserts: RefCell::new(Vec::new()),
                fail_upsert: false,
                verdict: None,
                verdict_budgets: RefCell::new(Vec::new()),
            }
        }
    }

    impl AgentHost for FakeHost {
        fn current_source(&self) -> Result<String, HostError> {
            Ok(self.source.clone())
        }

        fn stage_source<'a>(
            &'a mut self,
            _source: &'a str,
        ) -> HostFuture<'a, Result<(), HostError>> {
            Box::pin(async { Ok(()) })
        }

        fn shader_params(&mut self) -> HostFuture<'_, Option<Vec<ParamDefRecord>>> {
            let params = self.params.clone();
            Box::pin(async move { params })
        }

        fn upsert_param<'a>(
            &'a mut self,
            upsert: &'a ParamUpsert,
        ) -> HostFuture<'a, Result<(), HostError>> {
            Box::pin(async move {
                if self.fail_upsert {
                    return Err(HostError::new("overlay write refused"));
                }
                self.upserts.borrow_mut().push(upsert.clone());
                Ok(())
            })
        }

        fn await_engine_verdict(
            &mut self,
            budget_ms: u32,
        ) -> HostFuture<'_, Option<EngineVerdict>> {
            self.verdict_budgets.borrow_mut().push(budget_ms);
            let verdict = self.verdict.clone();
            Box::pin(async move { verdict })
        }

        fn led_points(&self) -> Vec<LedPoint> {
            Vec::new()
        }

        fn shader_context(&self) -> ShaderContext {
            ShaderContext::default()
        }
    }
}
