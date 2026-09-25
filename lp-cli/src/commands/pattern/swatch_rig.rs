//! The in-memory project one preview runs: the pattern's exported module(s)
//! plus a swatch rig.
//!
//! ```text
//! /project.json          the pattern's own manifest, verbatim
//! /module.json           clock + <exports> + fixture + output
//! /clock.json            the pattern's clock def (or a bare Clock)
//! /<export>/…            the exported folder(s), byte for byte (bar --set)
//! /fixture.json          the pattern's fixture def, re-pointed at the swatch
//! /swatch.map2d.json     the swatch mapping, verbatim
//! /output.json           one output reading bus:control.out
//! ```
//!
//! The fixture keeps the pattern's own `render_size`, `sampling` and every
//! other field it authors — "whatever the pattern's own fixture would pick" —
//! and changes only: the mapping (→ the swatch), `brightness` (→ 1),
//! `gamma_correction` (→ off), `color_order` (→ rgb), and its output channel
//! (→ `bus:control.out`). A pattern whose root module has no fixture gets
//! [`DEFAULT_RENDER_SIZE`] with direct sampling.

use anyhow::{Context, Result, bail};
use lpfs::{LpFsMemory, LpPath};
use serde_json::{Value, json};

use super::knob_override::KnobOverride;
use super::pattern_source::PatternSource;
use super::swatch::Swatch;

/// Render size for a pattern that ships no fixture of its own. 64×64 is
/// square (so no swatch is favoured) and fine enough that a 2D shader's
/// `pos / outputSize` resolves the densest swatch (16×16) at 4 px per lamp.
pub const DEFAULT_RENDER_SIZE: [u32; 2] = [64, 64];

/// The bus the rig's fixture publishes on and its output reads.
const CONTROL_BUS: &str = "bus:control.out";

/// What the rig's fixture ended up with, for the record.
#[derive(Debug, Clone, PartialEq)]
pub struct RigFixture {
    pub render_size: [u32; 2],
    pub sampling: String,
}

/// Build the preview project for `pattern` on `swatch`.
pub fn build_rig(
    pattern: &PatternSource,
    swatch: &Swatch,
    overrides: &[KnobOverride],
) -> Result<(LpFsMemory, RigFixture)> {
    let mut fs = LpFsMemory::new();
    let mut write = |path: &str, bytes: &[u8]| -> Result<()> {
        fs.write_file_mut(LpPath::new(path), bytes)
            .map_err(|e| anyhow::anyhow!("memfs write {path}: {e:?}"))
    };

    write("/project.json", &to_bytes(&pattern.manifest))?;

    let mut hits = vec![0usize; overrides.len()];
    for (path, bytes) in &pattern.export_files {
        let bytes = if path.ends_with(".json") && !overrides.is_empty() {
            match serde_json::from_slice::<Value>(bytes) {
                Ok(mut def) => {
                    for (i, knob) in overrides.iter().enumerate() {
                        hits[i] += knob.apply(&mut def);
                    }
                    to_bytes(&def)
                }
                Err(_) => bytes.clone(),
            }
        } else {
            bytes.clone()
        };
        write(&format!("/{path}"), &bytes)?;
    }
    for (knob, count) in overrides.iter().zip(&hits) {
        if *count == 0 {
            bail!(
                "--set {}: no `value` slot of {} is named that or bound to bus:{}",
                knob.name,
                pattern.slug,
                knob.name.strip_prefix("bus:").unwrap_or(&knob.name)
            );
        }
    }

    let clock = pattern
        .clock
        .clone()
        .unwrap_or_else(|| json!({ "kind": "Clock" }));
    write("/clock.json", &to_bytes(&clock))?;

    let (fixture, rig_fixture) = rig_fixture_def(pattern.fixture.as_ref())?;
    write("/fixture.json", &to_bytes(&fixture))?;
    write("/swatch.map2d.json", swatch.map_json.as_bytes())?;
    write("/output.json", &to_bytes(&output_def()))?;

    let mut nodes = serde_json::Map::new();
    nodes.insert("clock".into(), json!({ "ref": "./clock.json" }));
    for export in &pattern.exports {
        nodes.insert(
            export.clone(),
            json!({ "ref": format!("./{export}/module.json") }),
        );
    }
    nodes.insert("fixture".into(), json!({ "ref": "./fixture.json" }));
    nodes.insert("output".into(), json!({ "ref": "./output.json" }));
    let mut module = json!({ "kind": "Module", "nodes": Value::Object(nodes) });
    if !pattern.provenance.is_null() {
        module["provenance"] = pattern.provenance.clone();
    }
    write("/module.json", &to_bytes(&module))?;

    Ok((fs, rig_fixture))
}

/// The pattern's fixture def re-pointed at the swatch (see module docs).
fn rig_fixture_def(pattern_fixture: Option<&Value>) -> Result<(Value, RigFixture)> {
    let mut def = pattern_fixture.cloned().unwrap_or_else(|| {
        json!({
            "kind": "Fixture",
            "render_size": { "width": DEFAULT_RENDER_SIZE[0], "height": DEFAULT_RENDER_SIZE[1] },
            "bindings": { "input": { "source": "bus:visual.out" } },
            "sampling": "direct",
        })
    });
    let object = def
        .as_object_mut()
        .context("the pattern's fixture def is not a JSON object")?;
    object.insert(
        "mapping".into(),
        json!({ "kind": "Map2d", "source": "swatch.map2d.json" }),
    );
    object.insert("brightness".into(), json!(1.0));
    object.insert("gamma_correction".into(), json!(false));
    object.insert("color_order".into(), json!("rgb"));
    let bindings = object
        .entry("bindings")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("fixture bindings is not an object")?;
    bindings.insert("output".into(), json!({ "target": CONTROL_BUS }));

    let size = |axis: &str, fallback: u32| {
        def.get("render_size")
            .and_then(|size| size.get(axis))
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .unwrap_or(fallback)
    };
    let rig = RigFixture {
        render_size: [
            size("width", DEFAULT_RENDER_SIZE[0]),
            size("height", DEFAULT_RENDER_SIZE[1]),
        ],
        sampling: def
            .get("sampling")
            .and_then(Value::as_str)
            .unwrap_or("direct")
            .to_string(),
    };
    Ok((def, rig))
}

/// One output on the control bus. Its options are the display pipeline's
/// (white point, LUT, interpolation), which run downstream of the runtime
/// buffer the preview reads — set to identity anyway, so the def says what
/// the record means.
fn output_def() -> Value {
    json!({
        "kind": "Output",
        "ports": { "0": { "endpoint": "ws281x:local:D10" } },
        "bindings": { "input": { "source": CONTROL_BUS } },
        "options": {
            "white_point": [1.0, 1.0, 1.0],
            "interpolation_enabled": false,
            "dithering_enabled": false,
            "lut_enabled": false
        }
    })
}

/// Serialize with every object's `kind` key FIRST.
///
/// The def parsers read the `kind` tag before the fields it selects, and
/// `serde_json::Value`'s map (no `preserve_order` in this workspace) sorts
/// keys, which would put `bindings` ahead of it. Every other key keeps the
/// map's order.
fn to_bytes(value: &Value) -> Vec<u8> {
    let mut out = String::new();
    write_kind_first(value, &mut out);
    out.into_bytes()
}

fn write_kind_first(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            out.push('{');
            let kind = map.get_key_value("kind");
            let rest = map.iter().filter(|(key, _)| *key != "kind");
            for (i, (key, item)) in kind.into_iter().chain(rest).enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_kind_first(item, out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_kind_first(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_is_written_first_at_every_depth() {
        let value = json!({
            "bindings": { "a": 1 },
            "kind": "Fixture",
            "consumed": { "x": { "default": 0, "kind": "value" } }
        });
        let text = String::from_utf8(to_bytes(&value)).unwrap();
        assert!(text.starts_with("{\"kind\":\"Fixture\""), "{text}");
        assert!(
            text.contains("{\"kind\":\"value\",\"default\":0}"),
            "{text}"
        );
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), value);
    }
}
