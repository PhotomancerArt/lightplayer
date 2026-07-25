//! [`ExperimentSpec`]: what to compile, bind, and probe.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_collection::VecMap;
use lps_shared::{LpsType, LpsValueF32};
use serde::{Deserialize, Serialize};

use crate::probe_spec::ProbeSpec;

/// One experiment: uniform bindings, probes, and the LED points the shader
/// is targeting. The tool layer speaks JSON; all fields have lenient
/// defaults so a minimal experiment is `{}` (health-only run).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExperimentSpec {
    /// Pixel-space extent. Maps normalized coordinates → `pos` arguments;
    /// also written to the `outputSize` uniform if the shader declares it.
    #[serde(default = "default_size")]
    pub size: [u32; 2],
    /// Uniform writes, keyed by path syntax as `encode_uniform_write`
    /// expects (e.g. `"time"`, `"cfg.speed"`).
    #[serde(default)]
    pub bindings: VecMap<String, BindingValue>,
    /// Probes to evaluate; may be empty (health-only run).
    #[serde(default)]
    pub probes: Vec<ProbeSpec>,
    /// Caller-supplied LED points (normalized coordinates); used by
    /// `ProbeDomain::Leds` and by health. Fixture knowledge stays out of
    /// this crate.
    #[serde(default)]
    pub led_points: Vec<LedPoint>,
}

impl Default for ExperimentSpec {
    fn default() -> Self {
        Self {
            size: default_size(),
            bindings: VecMap::new(),
            probes: Vec::new(),
            led_points: Vec::new(),
        }
    }
}

fn default_size() -> [u32; 2] {
    [256, 256]
}

/// One LED sample point, in normalized [0,1]² coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LedPoint {
    pub label: String,
    pub channel: u32,
    pub at: [f32; 2],
}

/// A JSON-friendly uniform value; coerced to the declared uniform leaf type
/// when applied ([`BindingValue::coerce_to`]).
///
/// (`LpsValueF32` itself has no serde support, so the JSON boundary uses
/// this small shape-based enum instead.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BindingValue {
    /// `true`/`false` → `bool`.
    Bool(bool),
    /// A number → `float`, `int`, or `uint` per the declared type.
    Scalar(f32),
    /// `[x, y]`, `[x, y, z]`, `[x, y, z, w]` → `vec2`/`vec3`/`vec4`.
    Vec(Vec<f32>),
}

impl BindingValue {
    /// Coerce to a concrete [`LpsValueF32`] for the declared leaf type.
    pub fn coerce_to(&self, ty: &LpsType) -> Result<LpsValueF32, String> {
        match (self, ty) {
            (BindingValue::Scalar(x), LpsType::Float) => Ok(LpsValueF32::F32(*x)),
            (BindingValue::Scalar(x), LpsType::Int) => Ok(LpsValueF32::I32(*x as i32)),
            (BindingValue::Scalar(x), LpsType::UInt) => Ok(LpsValueF32::U32(*x as u32)),
            (BindingValue::Scalar(x), LpsType::Bool) => Ok(LpsValueF32::Bool(*x != 0.0)),
            (BindingValue::Bool(b), LpsType::Bool) => Ok(LpsValueF32::Bool(*b)),
            (BindingValue::Vec(v), LpsType::Vec2) if v.len() == 2 => {
                Ok(LpsValueF32::Vec2([v[0], v[1]]))
            }
            (BindingValue::Vec(v), LpsType::Vec3) if v.len() == 3 => {
                Ok(LpsValueF32::Vec3([v[0], v[1], v[2]]))
            }
            (BindingValue::Vec(v), LpsType::Vec4) if v.len() == 4 => {
                Ok(LpsValueF32::Vec4([v[0], v[1], v[2], v[3]]))
            }
            (v, ty) => Err(format!("cannot coerce {v:?} to uniform type {ty:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn minimal_spec_deserializes_with_defaults() {
        let spec: ExperimentSpec = serde_json::from_str("{}").expect("parse");
        assert_eq!(spec.size, [256, 256]);
        assert!(spec.bindings.is_empty());
        assert!(spec.probes.is_empty());
        assert!(spec.led_points.is_empty());
    }

    #[test]
    fn binding_value_json_shapes() {
        let m: VecMap<String, BindingValue> =
            serde_json::from_str(r#"{"a": 1.5, "b": [1, 2], "c": true, "d": 3}"#).expect("parse");
        assert_eq!(m.get(&"a".to_string()), Some(&BindingValue::Scalar(1.5)));
        assert_eq!(
            m.get(&"b".to_string()),
            Some(&BindingValue::Vec(alloc::vec![1.0, 2.0]))
        );
        assert_eq!(m.get(&"c".to_string()), Some(&BindingValue::Bool(true)));
        assert_eq!(m.get(&"d".to_string()), Some(&BindingValue::Scalar(3.0)));
    }

    #[test]
    fn coercion_follows_declared_type() {
        assert!(matches!(
            BindingValue::Scalar(2.0).coerce_to(&LpsType::Int),
            Ok(LpsValueF32::I32(2))
        ));
        assert!(matches!(
            BindingValue::Scalar(2.5).coerce_to(&LpsType::Float),
            Ok(LpsValueF32::F32(x)) if x == 2.5
        ));
        assert!(
            BindingValue::Vec(alloc::vec![1.0])
                .coerce_to(&LpsType::Vec2)
                .is_err()
        );
    }
}
