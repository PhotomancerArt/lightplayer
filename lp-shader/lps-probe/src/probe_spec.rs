//! [`ProbeSpec`]: one probe — a GLSL expression evaluated over a domain.

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::probe_domain::ProbeDomain;
use crate::probe_reduce::ProbeReduce;

/// One probe: a GLSL expression compiled into a wrapper function and
/// evaluated at every site of its domain (× every `vary` step).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbeSpec {
    /// Identifier for the result map; must match `[a-z0-9_]+` (it becomes
    /// part of a GLSL function name).
    pub id: String,
    /// Result type of the expression.
    pub ty: ProbeType,
    /// GLSL expression; may call `render()` and user functions. Position
    /// domains provide `vec2 pos` (pixel space); `Sweep` provides its
    /// `float <var>` instead.
    pub expr: String,
    pub domain: ProbeDomain,
    /// Optional ordered scalar sweep of one uniform binding (outer loop).
    #[serde(default)]
    pub vary: Option<ProbeVary>,
    #[serde(default)]
    pub reduce: ProbeReduce,
}

/// The GLSL type a probe expression evaluates to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeType {
    Float,
    Vec2,
    Vec3,
    Vec4,
    Int,
}

impl ProbeType {
    /// The GLSL type name used in wrapper emission.
    pub fn glsl_name(self) -> &'static str {
        match self {
            ProbeType::Float => "float",
            ProbeType::Vec2 => "vec2",
            ProbeType::Vec3 => "vec3",
            ProbeType::Vec4 => "vec4",
            ProbeType::Int => "int",
        }
    }

    /// Number of result components.
    pub fn component_count(self) -> usize {
        match self {
            ProbeType::Float | ProbeType::Int => 1,
            ProbeType::Vec2 => 2,
            ProbeType::Vec3 => 3,
            ProbeType::Vec4 => 4,
        }
    }
}

/// Ordered scalar sweep of one uniform binding (v1: scalar-only). For each
/// value, in the given order, the binding is written and every domain site
/// is evaluated — approximating frame-sequential behavior for shaders with
/// global state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeVary {
    pub binding: String,
    pub values: Vec<f32>,
}
