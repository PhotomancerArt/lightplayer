//! The `params` section of an `iterate` result: GLSL-declared uniforms vs
//! the def-side param records, with orphans flagged both ways.
//!
//! Declared truth is the compiled signature ([`lps_probe::uniform_leaves`]);
//! def truth is the host's `consumed`-map records
//! ([`crate::tool::iterate_host::AgentHost::shader_params`]). The diff runs
//! at TOP-LEVEL uniform-name granularity — def records are keyed by uniform
//! name, so a struct uniform is one name however many leaves it flattens to
//! — and skips the reserved `outputSize` (engine-synthesized, never backed
//! by a record). `declared_only` orphans are the class the engine fails at
//! render time on ("missing uniform field"); `def_only` orphans are stale
//! records.

use lps_probe::{CompiledShader, RESERVED_UNIFORM, round_sig4, top_level_uniform_names};
use serde_json::{Value, json};

use crate::tool::iterate_host::ParamDefRecord;

/// Build the `params` section. `compiled` = the current call's successful
/// compile (`None` = compile failed, declared side unavailable); `defs` =
/// the host's def records (`None` = host without def knowledge). Sections
/// only carry what is available; the orphan diff needs both sides.
pub(crate) fn params_json(
    compiled: Option<&CompiledShader>,
    defs: Option<&[ParamDefRecord]>,
) -> Value {
    let mut section = serde_json::Map::new();
    if let Some(compiled) = compiled {
        let declared: Vec<Value> = lps_probe::uniform_leaves(compiled.signatures())
            .iter()
            .map(|leaf| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), json!(leaf.path));
                obj.insert("type".into(), json!(leaf.glsl_type));
                if leaf.reserved {
                    obj.insert("reserved".into(), json!(true));
                }
                Value::Object(obj)
            })
            .collect();
        section.insert("declared".into(), Value::Array(declared));
    } else {
        section.insert(
            "declared".into(),
            json!({ "unavailable": "shader did not compile" }),
        );
    }
    if let Some(defs) = defs {
        section.insert(
            "def_records".into(),
            Value::Array(defs.iter().map(def_record_json).collect()),
        );
    }
    if let (Some(compiled), Some(defs)) = (compiled, defs) {
        let declared_names: Vec<String> = top_level_uniform_names(compiled.signatures())
            .into_iter()
            .filter(|name| name != RESERVED_UNIFORM)
            .collect();
        let declared_only: Vec<&str> = declared_names
            .iter()
            .filter(|name| !defs.iter().any(|def| def.name == **name))
            .map(String::as_str)
            .collect();
        let def_only: Vec<&str> = defs
            .iter()
            .filter(|def| def.name != RESERVED_UNIFORM && !declared_names.contains(&def.name))
            .map(|def| def.name.as_str())
            .collect();
        section.insert(
            "orphans".into(),
            json!({ "declared_only": declared_only, "def_only": def_only }),
        );
    }
    Value::Object(section)
}

/// One def record, compact: rounded f32s, absent options omitted, `false`
/// flags omitted.
fn def_record_json(def: &ParamDefRecord) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("name".into(), json!(def.name));
    if !def.label.is_empty() {
        obj.insert("label".into(), json!(def.label));
    }
    for (key, value) in [
        ("default", def.default),
        ("min", def.min),
        ("max", def.max),
        ("step", def.step),
    ] {
        if let Some(value) = value {
            obj.insert(key.into(), json!(round_sig4(value)));
        }
    }
    if def.panel {
        obj.insert("panel".into(), json!(true));
    }
    if let Some(unit) = &def.unit {
        obj.insert("unit".into(), json!(unit));
    }
    if def.bound {
        obj.insert("bound".into(), json!(true));
    }
    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEED_SHADER: &str = "layout(binding = 0) uniform float time;\n\
         layout(binding = 1) uniform float speed;\n\
         layout(binding = 2) uniform vec2 outputSize;\n\
         vec4 render(vec2 pos) { return vec4(fract(time * speed), 0.0, 0.0, 1.0); }";

    fn compile() -> CompiledShader {
        CompiledShader::compile(SPEED_SHADER).expect("compiles")
    }

    fn def(name: &str) -> ParamDefRecord {
        ParamDefRecord {
            name: name.into(),
            ..ParamDefRecord::default()
        }
    }

    #[test]
    fn orphans_are_reported_both_ways_and_output_size_is_skipped() {
        let compiled = compile();
        let defs = vec![def("time"), def("stale")];
        let section = params_json(Some(&compiled), Some(&defs));
        assert_eq!(section["orphans"]["declared_only"], json!(["speed"]));
        assert_eq!(section["orphans"]["def_only"], json!(["stale"]));
        // outputSize is declared (flagged reserved) but never an orphan.
        assert!(
            section["declared"]
                .as_array()
                .expect("declared")
                .iter()
                .any(|leaf| leaf["name"] == "outputSize" && leaf["reserved"] == true),
            "{section}"
        );
    }

    #[test]
    fn matching_sides_produce_no_orphans() {
        let compiled = compile();
        let defs = vec![def("time"), def("speed")];
        let section = params_json(Some(&compiled), Some(&defs));
        assert_eq!(section["orphans"]["declared_only"], json!([]));
        assert_eq!(section["orphans"]["def_only"], json!([]));
    }

    #[test]
    fn def_records_serialize_compact_and_rounded() {
        let defs = vec![ParamDefRecord {
            name: "speed".into(),
            label: "Speed".into(),
            default: Some(1.23456),
            min: Some(0.0),
            max: None,
            step: Some(1.0),
            panel: true,
            unit: Some("x".into()),
            bound: false,
        }];
        let section = params_json(None, Some(&defs));
        let record = &section["def_records"][0];
        assert_eq!(record["label"], "Speed");
        assert_eq!(record["default"], f64::from(1.235_f32), "4 sig digits");
        assert_eq!(record["min"], 0.0);
        assert!(record.get("max").is_none(), "absent options omitted");
        assert!(record.get("bound").is_none(), "false flags omitted");
        assert_eq!(record["panel"], true);
        assert_eq!(record["step"], 1.0, "the knob's quantization is visible");
        assert_eq!(record["unit"], "x");
        // Declared side is unavailable without a compile; no orphan diff.
        assert_eq!(section["declared"]["unavailable"], "shader did not compile");
        assert!(section.get("orphans").is_none());
    }

    #[test]
    fn hosts_without_def_knowledge_get_declared_only() {
        let compiled = compile();
        let section = params_json(Some(&compiled), None);
        assert!(section["declared"].is_array());
        assert!(section.get("def_records").is_none());
        assert!(section.get("orphans").is_none());
    }
}
