//! The canonical LightPlayer feature registry.
//!
//! [`LpFeature`] enumerates product-level firmware features — the vocabulary
//! shared by the embedded firmware manifest, `ServerHello` capability
//! reporting, and board/catalog surfaces. Variants are serialized as
//! namespaced kebab identifiers (`node.shader`, `svc.radio-espnow`).
//!
//! Feature IDs are API: once a wire form ships it is never renamed or reused.
//! Categories:
//!
//! - `node.*` — engine node-kind runtimes (cargo `node-*` gates on
//!   `lpc-engine`)
//! - `svc.*` — hardware services wired by the embedder (the `Option` services
//!   on `LpServer`)
//! - `gfx.*` — the graphics backend compiled into the build
//! - `diag.*` — diagnostics tier (e.g. unwinding panic recovery)
//! - `shader.*` — shader-engine capabilities (e.g. native f32 math)

use serde::{Deserialize, Serialize};

use crate::node::kind::NodeKind;

/// A product-level firmware feature.
///
/// Which cargo gate or injection implements a feature is deliberately not
/// recorded here — that mapping lives next to the gates (`lpc-engine`) and in
/// the firmware manifest composition layer.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum LpFeature {
    /// Button node runtime.
    #[serde(rename = "node.button")]
    NodeButton,
    /// Clock node runtime.
    #[serde(rename = "node.clock")]
    NodeClock,
    /// Fluid simulation node runtime.
    #[serde(rename = "node.fluid")]
    NodeFluid,
    /// Fixture (mapped-LED) node runtime.
    #[serde(rename = "node.fixture")]
    NodeFixture,
    /// Playlist node runtime.
    #[serde(rename = "node.playlist")]
    NodePlaylist,
    /// Control-radio node runtime.
    #[serde(rename = "node.radio")]
    NodeRadio,
    /// Shader and compute-shader node runtimes.
    #[serde(rename = "node.shader")]
    NodeShader,
    /// Texture node runtime.
    #[serde(rename = "node.texture")]
    NodeTexture,
    /// A button input service is wired by this device.
    #[serde(rename = "svc.button")]
    SvcButton,
    /// An ESP-NOW radio service is wired by this device.
    #[serde(rename = "svc.radio-espnow")]
    SvcRadioEspnow,
    /// CPU shader graphics backend (lp-gfx-lpvm).
    #[serde(rename = "gfx.lpvm")]
    GfxLpvm,
    /// Null graphics backend (no rendering).
    #[serde(rename = "gfx.null")]
    GfxNull,
    /// GPU graphics backend (wgpu).
    #[serde(rename = "gfx.wgpu")]
    GfxWgpu,
    /// Unwinding panic recovery (faults are caught, not fatal).
    #[serde(rename = "diag.unwind")]
    DiagUnwind,
    /// Native IEEE-754 f32 shader math alongside Q16.16.
    #[serde(rename = "shader.f32")]
    ShaderF32,
}

impl LpFeature {
    /// Every feature, in declaration order. Iteration over the registry goes
    /// through this const so call sites stay wildcard-free: adding a variant
    /// without extending it is caught by [`tests::all_is_total_and_unique`].
    pub const ALL: [LpFeature; 15] = [
        LpFeature::NodeButton,
        LpFeature::NodeClock,
        LpFeature::NodeFluid,
        LpFeature::NodeFixture,
        LpFeature::NodePlaylist,
        LpFeature::NodeRadio,
        LpFeature::NodeShader,
        LpFeature::NodeTexture,
        LpFeature::SvcButton,
        LpFeature::SvcRadioEspnow,
        LpFeature::GfxLpvm,
        LpFeature::GfxNull,
        LpFeature::GfxWgpu,
        LpFeature::DiagUnwind,
        LpFeature::ShaderF32,
    ];

    /// The stable wire identifier, identical to the serde form.
    pub const fn wire_name(self) -> &'static str {
        match self {
            LpFeature::NodeButton => "node.button",
            LpFeature::NodeClock => "node.clock",
            LpFeature::NodeFluid => "node.fluid",
            LpFeature::NodeFixture => "node.fixture",
            LpFeature::NodePlaylist => "node.playlist",
            LpFeature::NodeRadio => "node.radio",
            LpFeature::NodeShader => "node.shader",
            LpFeature::NodeTexture => "node.texture",
            LpFeature::SvcButton => "svc.button",
            LpFeature::SvcRadioEspnow => "svc.radio-espnow",
            LpFeature::GfxLpvm => "gfx.lpvm",
            LpFeature::GfxNull => "gfx.null",
            LpFeature::GfxWgpu => "gfx.wgpu",
            LpFeature::DiagUnwind => "diag.unwind",
            LpFeature::ShaderF32 => "shader.f32",
        }
    }

    /// The feature that provides a node kind's runtime, or `None` for kinds
    /// that are never gated (`Project`, `Output` — see the gate comments in
    /// `lpc-engine/Cargo.toml`).
    pub const fn for_node_kind(kind: NodeKind) -> Option<LpFeature> {
        match kind {
            NodeKind::Project => None,
            NodeKind::Output => None,
            NodeKind::Button => Some(LpFeature::NodeButton),
            NodeKind::Clock => Some(LpFeature::NodeClock),
            NodeKind::Texture => Some(LpFeature::NodeTexture),
            NodeKind::Shader => Some(LpFeature::NodeShader),
            NodeKind::ComputeShader => Some(LpFeature::NodeShader),
            NodeKind::Fluid => Some(LpFeature::NodeFluid),
            NodeKind::Playlist => Some(LpFeature::NodePlaylist),
            NodeKind::ControlRadio => Some(LpFeature::NodeRadio),
            NodeKind::Fixture => Some(LpFeature::NodeFixture),
        }
    }
}

/// Numeric facts about a firmware build that are true by construction
/// (partition layout, chip RAM). Soft limits — measured envelopes — are a
/// separate, per-(build × board) concern and never live here.
///
/// All fields are optional: a manifest states only what its build actually
/// knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestLimits {
    /// Total flash the build's partition layout targets, in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flash_total_bytes: Option<u64>,
    /// App partition size, in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flash_app_bytes: Option<u64>,
    /// RAM available to the firmware, in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ram_bytes: Option<u64>,
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    /// Every variant appears in `ALL` exactly once, in declaration order.
    /// The match below is wildcard-free: a new variant fails to compile here
    /// until it is classified, and the index assertion fails until it is
    /// added to `ALL`.
    #[test]
    fn all_is_total_and_unique() {
        fn index_in_all(feature: LpFeature) -> usize {
            match feature {
                LpFeature::NodeButton => 0,
                LpFeature::NodeClock => 1,
                LpFeature::NodeFluid => 2,
                LpFeature::NodeFixture => 3,
                LpFeature::NodePlaylist => 4,
                LpFeature::NodeRadio => 5,
                LpFeature::NodeShader => 6,
                LpFeature::NodeTexture => 7,
                LpFeature::SvcButton => 8,
                LpFeature::SvcRadioEspnow => 9,
                LpFeature::GfxLpvm => 10,
                LpFeature::GfxNull => 11,
                LpFeature::GfxWgpu => 12,
                LpFeature::DiagUnwind => 13,
                LpFeature::ShaderF32 => 14,
            }
        }
        for (i, feature) in LpFeature::ALL.iter().enumerate() {
            assert_eq!(index_in_all(*feature), i, "{feature:?} out of place in ALL");
        }
    }

    /// The serde form and `wire_name` agree, and the shipped identifiers are
    /// pinned verbatim — these strings are API and must never drift.
    #[test]
    fn wire_names_are_pinned() {
        let expected = [
            "node.button",
            "node.clock",
            "node.fluid",
            "node.fixture",
            "node.playlist",
            "node.radio",
            "node.shader",
            "node.texture",
            "svc.button",
            "svc.radio-espnow",
            "gfx.lpvm",
            "gfx.null",
            "gfx.wgpu",
            "diag.unwind",
            "shader.f32",
        ];
        for (feature, expected) in LpFeature::ALL.iter().zip(expected) {
            assert_eq!(feature.wire_name(), expected);
            let json = serde_json::to_string(feature).unwrap();
            assert_eq!(json, alloc::format!("\"{expected}\""));
            let back: LpFeature = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *feature);
        }
    }

    /// Node-kind mapping: gated kinds map onto the eight `node.*` features,
    /// ungated kinds map to `None`, and Shader/ComputeShader share a gate —
    /// mirrors `every_node_kind_is_explicitly_gated_or_always_on` in
    /// lpc-engine.
    #[test]
    fn node_kind_mapping_matches_gate_classification() {
        let cases = [
            (NodeKind::Project, None),
            (NodeKind::Output, None),
            (NodeKind::Button, Some(LpFeature::NodeButton)),
            (NodeKind::Clock, Some(LpFeature::NodeClock)),
            (NodeKind::Texture, Some(LpFeature::NodeTexture)),
            (NodeKind::Shader, Some(LpFeature::NodeShader)),
            (NodeKind::ComputeShader, Some(LpFeature::NodeShader)),
            (NodeKind::Fluid, Some(LpFeature::NodeFluid)),
            (NodeKind::Playlist, Some(LpFeature::NodePlaylist)),
            (NodeKind::ControlRadio, Some(LpFeature::NodeRadio)),
            (NodeKind::Fixture, Some(LpFeature::NodeFixture)),
        ];
        for (kind, expected) in cases {
            assert_eq!(LpFeature::for_node_kind(kind), expected, "{kind:?}");
        }
        let node_features: Vec<LpFeature> = LpFeature::ALL
            .iter()
            .copied()
            .filter(|f| f.wire_name().starts_with("node."))
            .collect();
        for feature in node_features {
            assert!(
                cases.iter().any(|(_, mapped)| *mapped == Some(feature)),
                "{feature:?} is a node.* feature no NodeKind maps to"
            );
        }
    }

    /// Limits round-trip, and absent fields serialize to nothing (a manifest
    /// states only what it knows).
    #[test]
    fn manifest_limits_round_trip_and_skip_none() {
        let empty = ManifestLimits::default();
        assert_eq!(serde_json::to_string(&empty).unwrap(), "{}");

        let limits = ManifestLimits {
            flash_total_bytes: Some(4 * 1024 * 1024),
            flash_app_bytes: Some(3 * 1024 * 1024),
            ram_bytes: None,
        };
        let json = serde_json::to_string(&limits).unwrap();
        assert_eq!(
            json,
            "{\"flashTotalBytes\":4194304,\"flashAppBytes\":3145728}"
        );
        let back: ManifestLimits = serde_json::from_str(&json).unwrap();
        assert_eq!(back, limits);

        let defaulted: ManifestLimits = serde_json::from_str("{}").unwrap();
        assert_eq!(defaulted, empty);
    }
}
