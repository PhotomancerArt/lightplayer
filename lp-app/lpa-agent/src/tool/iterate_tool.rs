//! The single `iterate` tool: JSON schema and dispatch into `lps-probe`.
//!
//! Execution order per call: resolve source (arg or host) → stage the new
//! source first if given (the user should see the edit even if probes fail)
//! → `run_experiment` → optional diff against the session's cached previous
//! compile → serialize with 4-significant-digit rounding. Compile errors are
//! DATA (normal result content); `is_error` is reserved for host/internal
//! failures.

use lp_collection::VecMap;
use lps_probe::{
    BindingValue, CompiledShader, ExperimentSpec, ProbeSpec, ShaderCompileOutcome,
    diff_experiments, run_experiment,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::provider::model_provider::ToolDef;
use crate::tool::iterate_host::AgentHost;

/// The tool's wire name.
pub const ITERATE_TOOL_NAME: &str = "iterate";

/// Client-side cap on staged source size. Mirrors
/// `lpa_studio_core::MAX_ASSET_BODY_BYTES` (this crate must not depend on
/// lpa-studio-core; the values must stay in sync).
pub const MAX_SOURCE_BYTES: usize = 10 * 1024;

/// Result of one `iterate` call, ready for the transcript and the UI.
#[derive(Clone, Debug)]
pub struct IterateOutcome {
    /// JSON text for the `tool_result` content.
    pub content: String,
    /// True only for host/internal failures (never for compile errors).
    pub is_error: bool,
    /// Compact JSON for the UI's tool row.
    pub summary: Value,
}

/// Parsed `iterate` input. Unknown fields are rejected so the model gets an
/// actionable error instead of a silently ignored argument.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IterateInput {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub size: Option<[u32; 2]>,
    #[serde(default)]
    pub bindings: VecMap<String, BindingValue>,
    #[serde(default)]
    pub probes: Vec<ProbeSpec>,
    #[serde(default)]
    pub diff: Option<DiffArg>,
    #[serde(default)]
    pub capture: Option<Value>,
}

/// The `diff` argument; only `{ "vs": "previous" }` is supported.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiffArg {
    pub vs: String,
}

/// Execute one `iterate` call. `prev_compiled` is the session's cache of the
/// last successfully compiled shader (used by `diff`, updated on success).
pub fn run_iterate(
    input_json: &Value,
    host: &mut dyn AgentHost,
    prev_compiled: &mut Option<CompiledShader>,
) -> IterateOutcome {
    let input: IterateInput = match serde_json::from_value(input_json.clone()) {
        Ok(input) => input,
        Err(e) => {
            return data_outcome(
                json!({ "input_error": format!("invalid iterate input: {e}") }),
                json!({ "input_error": true }),
            );
        }
    };
    let note = input.note.clone();

    // 10 KB pre-check: actionable in-band message instead of a failed
    // overlay write.
    if let Some(source) = &input.source
        && source.len() > MAX_SOURCE_BYTES
    {
        return data_outcome(
            json!({ "error": format!(
                "source is {} bytes; the asset limit is {MAX_SOURCE_BYTES} — shorten the shader",
                source.len()
            ) }),
            json!({ "note": note, "staged": false, "error": "source too large" }),
        );
    }

    // Resolve the source; stage first when new source was given.
    let staged = input.source.is_some();
    let source = match &input.source {
        Some(source) => {
            if let Err(e) = host.stage_source(source) {
                return host_error(format!("staging source failed: {}", e.message));
            }
            source.clone()
        }
        None => match host.current_source() {
            Ok(source) => source,
            Err(e) => return host_error(format!("reading current source failed: {}", e.message)),
        },
    };

    let spec = ExperimentSpec {
        size: input.size.unwrap_or([256, 256]),
        bindings: input.bindings,
        probes: input.probes,
        led_points: host.led_points(),
    };
    let mut result = run_experiment(&source, &spec);

    // Diff against the previous compile, before the cache is updated.
    let diff_value = input.diff.as_ref().map(|arg| {
        if arg.vs != "previous" {
            return json!({ "available": false, "reason": format!(
                "unsupported diff target {:?}; only \"previous\" is supported", arg.vs
            ) });
        }
        match (prev_compiled.as_ref(), result.compiled.as_ref()) {
            (Some(prev), Some(cur)) => match diff_experiments(prev, cur, &spec) {
                Ok(report) => {
                    serde_json::to_value(report.rounded()).expect("diff report serializes")
                }
                Err(reason) => json!({ "available": false, "reason": reason }),
            },
            (None, _) => json!({ "available": false,
                "reason": "no previous compiled shader in this session" }),
            (_, None) => json!({ "available": false,
                "reason": "current shader did not compile" }),
        }
    });
    if let Some(compiled) = result.compiled.take() {
        *prev_compiled = Some(compiled);
    }

    let shader_ok = matches!(result.shader, ShaderCompileOutcome::Ok);
    let probe_count = result.probes.len();
    let warning_count = result.warnings.len();

    let mut content = serde_json::to_value(result.rounded()).expect("result serializes");
    let obj = content.as_object_mut().expect("result is an object");
    if let Some(note) = &note {
        obj.insert("note".into(), json!(note));
    }
    obj.insert("staged".into(), json!(staged));
    if let Some(diff) = diff_value {
        obj.insert("diff".into(), diff);
    }
    if input.capture.is_some() {
        obj.insert(
            "capture".into(),
            json!({ "available": false,
                "reason": "capture lands with the M6 preview snapshot seam" }),
        );
    }

    IterateOutcome {
        content: content.to_string(),
        is_error: false,
        summary: json!({
            "note": note,
            "staged": staged,
            "shader_ok": shader_ok,
            "probes": probe_count,
            "warnings": warning_count,
        }),
    }
}

/// The tool definition offered to the model every turn.
pub fn iterate_tool_def() -> ToolDef {
    ToolDef {
        name: ITERATE_TOOL_NAME.into(),
        description: DESCRIPTION.into(),
        input_schema: input_schema(),
    }
}

const DESCRIPTION: &str = "\
Run one iteration on the shader: optionally stage new GLSL source (an unsaved \
edit the user can Save or revert), then compile it and evaluate probes on the \
f32 reference interpreter. Every call returns compile diagnostics, a health \
report over a 16x16 grid plus the LED points (NaN/Inf counts, near-black \
fraction, mean luminance, clip fraction), per-probe results, and warnings. \
Compile errors come back as normal data — read the diagnostics and iterate. \
Probes evaluate a GLSL expression over a domain (point/points/grid/line/leds \
or a scalar sweep), optionally swept over ordered `vary` values of one \
uniform, and reduced (`none` raw rows, `stats`, or `histogram`). Position \
coordinates are normalized [0,1]^2. Caps: 8 probes, 4096 evals per probe \
(|domain| x |vary|), 64 raw rows for reduce `none`, 16384 total evals; \
violations skip the probe with an actionable reason. Evaluation is not free \
— a maximum-size experiment takes seconds — so size domains to the question: \
prefer small grids or lines with `stats`/`histogram` over huge raw dumps.";

fn input_schema() -> Value {
    let vec2 = json!({ "type": "array", "items": { "type": "number" },
        "minItems": 2, "maxItems": 2 });
    let domain = json!({
        "description": "Where the expression is evaluated. Coordinates are normalized [0,1]^2 (scaled to pixel space by `size`).",
        "oneOf": [
            { "type": "object", "additionalProperties": false, "required": ["point"],
              "properties": { "point": { "type": "object", "additionalProperties": false,
                "required": ["at"], "properties": { "at": vec2 } } } },
            { "type": "object", "additionalProperties": false, "required": ["points"],
              "properties": { "points": { "type": "object", "additionalProperties": false,
                "required": ["at"], "properties": { "at": { "type": "array", "items": vec2 } } } } },
            { "type": "object", "additionalProperties": false, "required": ["grid"],
              "properties": { "grid": { "type": "object", "additionalProperties": false,
                "required": ["nx", "ny"], "properties": {
                    "nx": { "type": "integer", "minimum": 1 },
                    "ny": { "type": "integer", "minimum": 1 },
                    "rect": { "type": "array", "items": { "type": "number" },
                        "minItems": 4, "maxItems": 4,
                        "description": "[x0, y0, x1, y1]; default the unit square." } } } } },
            { "type": "object", "additionalProperties": false, "required": ["line"],
              "properties": { "line": { "type": "object", "additionalProperties": false,
                "required": ["from", "to", "n"], "properties": {
                    "from": vec2, "to": vec2,
                    "n": { "type": "integer", "minimum": 1,
                        "description": "Evenly spaced points, endpoints inclusive." } } } } },
            { "type": "object", "additionalProperties": false, "required": ["leds"],
              "properties": { "leds": { "type": "object", "additionalProperties": false,
                "properties": { "indices": { "type": "array",
                    "items": { "type": "integer", "minimum": 0 },
                    "description": "LED indices; omit for all LED points." } } } } },
            { "type": "object", "additionalProperties": false, "required": ["sweep"],
              "properties": { "sweep": { "type": "object", "additionalProperties": false,
                "required": ["var", "from", "to", "n"], "properties": {
                    "var": { "type": "string",
                        "description": "The expression gets `float <var>` instead of `vec2 pos`." },
                    "from": { "type": "number" }, "to": { "type": "number" },
                    "n": { "type": "integer", "minimum": 1 } } } } }
        ]
    });
    let probe = json!({
        "type": "object", "additionalProperties": false,
        "required": ["id", "ty", "expr", "domain"],
        "properties": {
            "id": { "type": "string", "pattern": "^[a-z0-9_]+$" },
            "ty": { "enum": ["float", "vec2", "vec3", "vec4", "int"],
                "description": "Result type of the expression." },
            "expr": { "type": "string",
                "description": "GLSL expression; may call render() and user functions. Position domains provide `vec2 pos` (pixel space)." },
            "domain": domain,
            "vary": { "type": "object", "additionalProperties": false,
                "required": ["binding", "values"],
                "properties": {
                    "binding": { "type": "string" },
                    "values": { "type": "array", "items": { "type": "number" } } },
                "description": "Ordered scalar sweep of one uniform (outer loop); approximates frame-sequential behavior for shaders with global state." },
            "reduce": {
                "description": "Default `none` (raw rows, capped at 64 total). Prefer `stats` or `histogram` for larger domains.",
                "oneOf": [
                    { "const": "none" },
                    { "const": "stats" },
                    { "type": "object", "additionalProperties": false,
                      "required": ["histogram"],
                      "properties": { "histogram": { "type": "object",
                        "additionalProperties": false, "required": ["bins"],
                        "properties": { "bins": { "type": "integer",
                            "minimum": 1, "maximum": 255 } } } } }
                ]
            }
        }
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "source": { "type": "string",
                "description": "New full GLSL source. When present it is staged as an unsaved edit AND compiled for this experiment. Must define `vec4 render(vec2 pos)`. Max 10240 bytes." },
            "note": { "type": "string",
                "description": "One-line intent, shown in the UI next to this call." },
            "size": { "type": "array", "items": { "type": "integer", "minimum": 1 },
                "minItems": 2, "maxItems": 2,
                "description": "Pixel-space extent [width, height]; default [256, 256]. Written to the `outputSize` uniform if the shader declares it." },
            "bindings": { "type": "object",
                "additionalProperties": { "oneOf": [
                    { "type": "number" }, { "type": "boolean" },
                    { "type": "array", "items": { "type": "number" },
                      "minItems": 2, "maxItems": 4 } ] },
                "description": "Uniform writes keyed by path (e.g. \"time\", \"cfg.speed\"). Numbers coerce to the declared type; arrays map to vec2/vec3/vec4." },
            "probes": { "type": "array", "maxItems": 8, "items": probe },
            "diff": { "type": "object", "additionalProperties": false,
                "required": ["vs"], "properties": { "vs": { "const": "previous" } },
                "description": "Include a render diff vs the last compiled source in this session (max/mean delta, changed fraction, changed bbox)." },
            "capture": { "type": "object",
                "description": "Reserved; preview capture is not yet available." }
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
    use lps_probe::LedPoint;

    use super::*;
    use crate::tool::iterate_host::{HostError, ShaderContext};

    const RED: &str = "vec4 render(vec2 pos) { return vec4(1.0, 0.0, 0.0, 1.0); }";
    const GREEN: &str = "vec4 render(vec2 pos) { return vec4(0.0, 1.0, 0.0, 1.0); }";

    #[test]
    fn schema_is_wellformed_and_input_round_trips() {
        let def = iterate_tool_def();
        assert_eq!(def.name, "iterate");
        assert!(def.description.contains("size domains to the question"));
        assert_eq!(def.input_schema["type"], "object");

        let input: IterateInput = serde_json::from_value(json!({
            "source": RED,
            "note": "solid red",
            "size": [128, 128],
            "bindings": { "time": 1.5, "cfg.color": [1, 0, 0] },
            "probes": [
                { "id": "center", "ty": "vec4", "expr": "render(pos)",
                  "domain": { "point": { "at": [0.5, 0.5] } } },
                { "id": "row", "ty": "float", "expr": "render(pos).r",
                  "domain": { "line": { "from": [0, 0.5], "to": [1, 0.5], "n": 8 } },
                  "reduce": "stats",
                  "vary": { "binding": "time", "values": [0, 1] } }
            ],
            "diff": { "vs": "previous" },
            "capture": {}
        }))
        .expect("parse");
        assert_eq!(input.probes.len(), 2);
        assert_eq!(input.size, Some([128, 128]));
    }

    #[test]
    fn unknown_input_fields_are_rejected_in_band() {
        let mut host = FakeHost::new(RED);
        let mut cache = None;
        let outcome = run_iterate(&json!({ "sorce": "typo" }), &mut host, &mut cache);
        assert!(!outcome.is_error);
        assert!(
            outcome.content.contains("input_error"),
            "{}",
            outcome.content
        );
        assert!(host.staged.is_empty());
    }

    #[test]
    fn health_only_call_uses_current_source() {
        let mut host = FakeHost::new(RED);
        let mut cache = None;
        let outcome = run_iterate(&json!({}), &mut host, &mut cache);
        assert!(!outcome.is_error);
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["shader"], "ok");
        assert_eq!(content["staged"], false);
        assert_eq!(content["health"]["nan_count"], 0);
        assert!(host.staged.is_empty());
        assert!(cache.is_some(), "successful compile is cached");
    }

    #[test]
    fn source_is_staged_before_probes_and_compile_errors_are_data() {
        let mut host = FakeHost::new(RED);
        let mut cache = None;
        let broken = "vec4 render(vec2 pos) { return oops; }";
        let outcome = run_iterate(&json!({ "source": broken }), &mut host, &mut cache);
        assert!(!outcome.is_error, "compile errors are data");
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert!(
            content["shader"]["err"]["diagnostics"].is_array(),
            "{content}"
        );
        assert_eq!(content["staged"], true);
        // Staged even though the compile failed.
        assert_eq!(host.staged, vec![broken.to_string()]);
        assert!(cache.is_none());
        assert_eq!(outcome.summary["shader_ok"], false);
    }

    #[test]
    fn oversized_source_is_precheck_rejected_without_staging() {
        let mut host = FakeHost::new(RED);
        let mut cache = None;
        let big = format!("// {}\n{RED}", "x".repeat(MAX_SOURCE_BYTES));
        let outcome = run_iterate(&json!({ "source": big }), &mut host, &mut cache);
        assert!(!outcome.is_error);
        assert!(
            outcome.content.contains("the asset limit is 10240"),
            "{}",
            outcome.content
        );
        assert!(
            host.staged.is_empty(),
            "oversized source must not be staged"
        );
    }

    #[test]
    fn capture_is_reserved() {
        let mut host = FakeHost::new(RED);
        let mut cache = None;
        let outcome = run_iterate(&json!({ "capture": {} }), &mut host, &mut cache);
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["capture"]["available"], false);
        assert!(
            content["capture"]["reason"]
                .as_str()
                .expect("reason")
                .contains("M6"),
            "{content}"
        );
    }

    #[test]
    fn diff_reports_against_cached_previous_compile() {
        let mut host = FakeHost::new(RED);
        let mut cache = None;

        // First call: nothing to diff against yet.
        let outcome = run_iterate(
            &json!({ "diff": { "vs": "previous" } }),
            &mut host,
            &mut cache,
        );
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["diff"]["available"], false);

        // Second call with new source: full-field change vs the cached red.
        let outcome = run_iterate(
            &json!({ "source": GREEN, "diff": { "vs": "previous" } }),
            &mut host,
            &mut cache,
        );
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["diff"]["max_delta"], 1.0);
        assert_eq!(content["diff"]["changed_fraction"], 1.0);

        // Third call, same source: diff vs the cached green is zero.
        let outcome = run_iterate(
            &json!({ "diff": { "vs": "previous" } }),
            &mut host,
            &mut cache,
        );
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["diff"]["max_delta"], 0.0);
    }

    #[test]
    fn probe_results_are_rounded_for_transport() {
        let mut host =
            FakeHost::new("vec4 render(vec2 pos) { return vec4(0.123456, 0.0, 0.0, 1.0); }");
        let mut cache = None;
        let outcome = run_iterate(
            &json!({ "probes": [ { "id": "p", "ty": "float", "expr": "render(pos).r",
                "domain": { "point": { "at": [0.5, 0.5] } } } ] }),
            &mut host,
            &mut cache,
        );
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        let value = content["probes"]["p"]["values"]["rows"][0]["value"][0]
            .as_f64()
            .expect("value");
        // JSON carries the rounded f32 widened to f64.
        assert_eq!(value, f64::from(0.1235_f32), "rounded to 4 sig digits");
    }

    #[test]
    fn host_failures_are_error_results() {
        let mut host = FakeHost::new(RED);
        host.fail_stage = true;
        let mut cache = None;
        let outcome = run_iterate(&json!({ "source": GREEN }), &mut host, &mut cache);
        assert!(outcome.is_error);
        assert!(outcome.content.contains("staging source failed"));
    }

    // -- helpers ----------------------------------------------------------

    struct FakeHost {
        source: String,
        staged: Vec<String>,
        fail_stage: bool,
    }

    impl FakeHost {
        fn new(source: &str) -> Self {
            Self {
                source: source.to_string(),
                staged: Vec::new(),
                fail_stage: false,
            }
        }
    }

    impl AgentHost for FakeHost {
        fn current_source(&self) -> Result<String, HostError> {
            Ok(self.source.clone())
        }

        fn stage_source(&mut self, source: &str) -> Result<(), HostError> {
            if self.fail_stage {
                return Err(HostError::new("overlay write refused"));
            }
            self.staged.push(source.to_string());
            self.source = source.to_string();
            Ok(())
        }

        fn led_points(&self) -> Vec<LedPoint> {
            vec![LedPoint {
                label: "led0".into(),
                channel: 0,
                at: [0.5, 0.5],
            }]
        }

        fn shader_context(&self) -> ShaderContext {
            ShaderContext::default()
        }
    }
}
