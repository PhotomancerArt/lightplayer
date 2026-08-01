//! Engine-side feature derivation for the firmware manifest.
//!
//! [`supported_features`] answers "which [`LpFeature`]s does this *build of
//! the engine* contribute?" from `cfg!` — the same compile-time facts as the
//! `node-*` gates themselves, so the answer cannot drift from the build.
//!
//! The engine only speaks for features it owns (the node gates and
//! `panic-recovery`). Embedder-owned features — `svc.*` services, the `gfx.*`
//! backend, `shader.f32` — are supplied by the firmware manifest composition
//! layer, which unions them with this list.

use alloc::vec::Vec;

use lpc_model::LpFeature;

/// Who answers for a feature: the engine's own cargo gates, or the embedder /
/// composition layer.
enum FeatureOrigin {
    /// The engine owns the gate; the bool is `cfg!` truth for this build.
    Engine(bool),
    /// Supplied by the embedder at manifest composition / server construction.
    Embedder,
}

/// Wildcard-free classification: a new [`LpFeature`] variant is a compile
/// error here until someone decides which side owns it.
const fn origin(feature: LpFeature) -> FeatureOrigin {
    match feature {
        LpFeature::NodeButton => FeatureOrigin::Engine(cfg!(feature = "node-button")),
        LpFeature::NodeClock => FeatureOrigin::Engine(cfg!(feature = "node-clock")),
        LpFeature::NodeFluid => FeatureOrigin::Engine(cfg!(feature = "node-fluid")),
        LpFeature::NodeFixture => FeatureOrigin::Engine(cfg!(feature = "node-fixture")),
        LpFeature::NodePlaylist => FeatureOrigin::Engine(cfg!(feature = "node-playlist")),
        LpFeature::NodeRadio => FeatureOrigin::Engine(cfg!(feature = "node-radio")),
        LpFeature::NodeShader => FeatureOrigin::Engine(cfg!(feature = "node-shader")),
        LpFeature::NodeTexture => FeatureOrigin::Engine(cfg!(feature = "node-texture")),
        LpFeature::DiagUnwind => FeatureOrigin::Engine(cfg!(feature = "panic-recovery")),
        LpFeature::SvcButton => FeatureOrigin::Embedder,
        LpFeature::SvcRadioEspnow => FeatureOrigin::Embedder,
        LpFeature::GfxLpvm => FeatureOrigin::Embedder,
        LpFeature::GfxNull => FeatureOrigin::Embedder,
        LpFeature::GfxWgpu => FeatureOrigin::Embedder,
        LpFeature::ShaderF32 => FeatureOrigin::Embedder,
    }
}

/// The engine-owned features enabled in this build, in [`LpFeature::ALL`]
/// order.
pub fn supported_features() -> Vec<LpFeature> {
    LpFeature::ALL
        .iter()
        .copied()
        .filter(|feature| matches!(origin(*feature), FeatureOrigin::Engine(true)))
        .collect()
}

/// One feature's contribution to [`ENGINE_FEATURE_FRAGMENT`]: its quoted,
/// comma-terminated wire name when engine-owned and enabled, else empty.
const fn engine_fragment(feature: LpFeature) -> &'static str {
    match origin(feature) {
        FeatureOrigin::Engine(true) => lpc_model::manifest::feature_fragment(true, feature),
        FeatureOrigin::Engine(false) | FeatureOrigin::Embedder => "",
    }
}

/// The engine-owned enabled features as a pre-quoted, comma-terminated JSON
/// fragment for `lp_embed_manifest_core!`'s `features:` list. Derived from
/// the same [`origin`] classification as [`supported_features`] — the two
/// cannot drift (asserted by test as well).
pub const ENGINE_FEATURE_FRAGMENT: &str = lpc_model::lp_const_concat!(
    engine_fragment(LpFeature::ALL[0]),
    engine_fragment(LpFeature::ALL[1]),
    engine_fragment(LpFeature::ALL[2]),
    engine_fragment(LpFeature::ALL[3]),
    engine_fragment(LpFeature::ALL[4]),
    engine_fragment(LpFeature::ALL[5]),
    engine_fragment(LpFeature::ALL[6]),
    engine_fragment(LpFeature::ALL[7]),
    engine_fragment(LpFeature::ALL[8]),
    engine_fragment(LpFeature::ALL[9]),
    engine_fragment(LpFeature::ALL[10]),
    engine_fragment(LpFeature::ALL[11]),
    engine_fragment(LpFeature::ALL[12]),
    engine_fragment(LpFeature::ALL[13]),
    engine_fragment(LpFeature::ALL[14]),
);

// A new LpFeature variant grows ALL past this fragment list — fail the build
// here until the list above covers it.
const _: () = assert!(LpFeature::ALL.len() == 15);

#[cfg(test)]
mod tests {
    use super::*;

    /// Under the crate's default feature set (all eight node gates on,
    /// `panic-recovery` off) the derivation yields exactly the eight `node.*`
    /// features. The expected list is written out by hand — independent of
    /// the `cfg!` match — so a wrong gate string or dropped arm in `origin`
    /// fails here instead of shipping.
    #[test]
    #[cfg(all(
        feature = "node-button",
        feature = "node-clock",
        feature = "node-fluid",
        feature = "node-fixture",
        feature = "node-playlist",
        feature = "node-radio",
        feature = "node-shader",
        feature = "node-texture",
        not(feature = "panic-recovery")
    ))]
    fn default_build_yields_the_eight_node_features() {
        assert_eq!(
            supported_features(),
            alloc::vec![
                LpFeature::NodeButton,
                LpFeature::NodeClock,
                LpFeature::NodeFluid,
                LpFeature::NodeFixture,
                LpFeature::NodePlaylist,
                LpFeature::NodeRadio,
                LpFeature::NodeShader,
                LpFeature::NodeTexture,
            ]
        );
    }

    /// The const JSON fragment and the runtime list are projections of the
    /// same `origin` classification — assert they agree byte-for-byte.
    #[test]
    fn engine_feature_fragment_matches_supported_features() {
        let expected: alloc::string::String = supported_features()
            .iter()
            .map(|f| alloc::format!("\"{}\",", f.wire_name()))
            .collect();
        assert_eq!(ENGINE_FEATURE_FRAGMENT, expected);
    }

    /// Every gated node kind's feature is engine-owned, and the derivation
    /// agrees with the per-kind gates: a kind's runtime attaches iff its
    /// feature is reported. Mirrors
    /// `every_node_kind_is_explicitly_gated_or_always_on` in the project
    /// loader tests.
    #[test]
    fn node_kind_features_are_engine_owned() {
        use lpc_model::NodeKind;
        for kind in [
            NodeKind::Project,
            NodeKind::Output,
            NodeKind::Button,
            NodeKind::Clock,
            NodeKind::Texture,
            NodeKind::Shader,
            NodeKind::ComputeShader,
            NodeKind::Fluid,
            NodeKind::Playlist,
            NodeKind::ControlRadio,
            NodeKind::Fixture,
        ] {
            if let Some(feature) = LpFeature::for_node_kind(kind) {
                assert!(
                    matches!(origin(feature), FeatureOrigin::Engine(_)),
                    "{feature:?} (from {kind:?}) must be engine-owned"
                );
            }
        }
    }
}
