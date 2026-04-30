//! ResolveSource — provenance for cached resolved values.

use crate::resolver::binding_kind::BindingKind;

/// Where a cached resolved value came from.
///
/// Not on the wire (M4.4 sync ships `(value, frame)` pairs only;
/// provenance stays server-side). Used by the resolver to invalidate
/// correctly and by debug surfaces to explain "why does slot X have
/// this value?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveSource {
    /// Came from `SrcNodeConfig.overrides[prop]`.
    Override(BindingKind),
    /// Came from `Slot.bind` on the artifact.
    ArtifactBind(BindingKind),
    /// Fell through to `Slot.default`.
    Default,
    /// Resolver couldn't satisfy any layer; using default-floor.
    Failed,
}

#[cfg(test)]
mod tests {
    use super::{BindingKind, ResolveSource};

    #[test]
    fn resolve_source_clone_round_trip() {
        let sources = [
            ResolveSource::Override(BindingKind::Bus),
            ResolveSource::ArtifactBind(BindingKind::Literal),
            ResolveSource::Default,
            ResolveSource::Failed,
        ];
        for source in sources {
            let cloned = source.clone();
            assert_eq!(source, cloned);
        }
    }

    #[test]
    fn resolve_source_pattern_match_variants() {
        let s = ResolveSource::Override(BindingKind::NodeProp);
        assert!(matches!(s, ResolveSource::Override(_)));

        let s = ResolveSource::ArtifactBind(BindingKind::Bus);
        assert!(matches!(s, ResolveSource::ArtifactBind(_)));

        let s = ResolveSource::Default;
        assert!(matches!(s, ResolveSource::Default));

        let s = ResolveSource::Failed;
        assert!(matches!(s, ResolveSource::Failed));
    }

    #[test]
    fn resolve_source_override_holds_binding_kind() {
        let s = ResolveSource::Override(BindingKind::Literal);
        match s {
            ResolveSource::Override(k) => assert_eq!(k, BindingKind::Literal),
            _ => panic!("expected Override variant"),
        }
    }
}
