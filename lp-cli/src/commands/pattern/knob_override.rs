//! `--set knob=value`: override a `value` slot's default before the preview
//! project is built.
//!
//! A knob is named either by its slot (`tail`) or by the bus channel it is
//! bound to (`bus:tail`, or bare `tail` again). The override rewrites the
//! authored `default` in the in-memory copy of the def; nothing on disk
//! changes. An unbound slot reads its default, and so does a bound one whose
//! channel nobody in the rig writes — which is every channel a swatch rig
//! has, so the default IS the value the shader sees.

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// One parsed `--set`.
#[derive(Debug, Clone, PartialEq)]
pub struct KnobOverride {
    pub name: String,
    pub value: f64,
}

impl KnobOverride {
    pub fn parse(text: &str) -> Result<Self> {
        let (name, value) = text
            .split_once('=')
            .with_context(|| format!("--set {text:?}: expected KNOB=VALUE"))?;
        let name = name.trim();
        if name.is_empty() {
            bail!("--set {text:?}: empty knob name");
        }
        let value: f64 = value
            .trim()
            .parse()
            .with_context(|| format!("--set {text:?}: value is not a number"))?;
        Ok(Self {
            name: name.to_string(),
            value,
        })
    }

    /// Apply to one def; returns how many slots it changed.
    pub fn apply(&self, def: &mut Value) -> usize {
        let bus_name = self
            .name
            .strip_prefix("bus:")
            .unwrap_or(&self.name)
            .to_string();
        let bound: Vec<String> = def
            .get("bindings")
            .and_then(Value::as_object)
            .map(|bindings| {
                bindings
                    .iter()
                    .filter(|(_, binding)| {
                        binding.get("source").and_then(Value::as_str)
                            == Some(&format!("bus:{bus_name}"))
                    })
                    .map(|(slot, _)| slot.clone())
                    .collect()
            })
            .unwrap_or_default();
        let Some(consumed) = def.get_mut("consumed").and_then(Value::as_object_mut) else {
            return 0;
        };
        let mut hits = 0;
        for (slot, spec) in consumed.iter_mut() {
            let named = *slot == self.name || bound.contains(slot);
            let is_value = spec.get("kind").and_then(Value::as_str) == Some("value");
            if named && is_value {
                spec["default"] = serde_json::json!(self.value);
                hits += 1;
            }
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_by_slot_or_bus_channel() {
        let def = json!({
            "bindings": { "len": { "source": "bus:tail" } },
            "consumed": {
                "len": { "kind": "value", "default": 0.1 },
                "phase": { "kind": "phasor", "default": 0 }
            }
        });
        for name in ["len", "tail", "bus:tail"] {
            let mut copy = def.clone();
            let hits = KnobOverride::parse(&format!("{name}=0.4"))
                .unwrap()
                .apply(&mut copy);
            assert_eq!(hits, 1, "{name}");
            assert_eq!(copy["consumed"]["len"]["default"], json!(0.4));
        }
        let mut copy = def.clone();
        assert_eq!(
            KnobOverride::parse("phase=1").unwrap().apply(&mut copy),
            0,
            "only value slots take an override"
        );
    }

    #[test]
    fn refuses_malformed_text() {
        assert!(KnobOverride::parse("tail").is_err());
        assert!(KnobOverride::parse("=1").is_err());
        assert!(KnobOverride::parse("tail=wide").is_err());
    }
}
