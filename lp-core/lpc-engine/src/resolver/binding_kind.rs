//! BindingKind — discriminator for Binding variants without payload.

/// Discriminator for `Binding` variants without payload.
///
/// Used by `ResolveSource` to record provenance of a cached value
/// (cheaper than carrying a full `Binding` clone in every cache
/// entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingKind {
    Bus,
    Literal,
    NodeProp,
}

#[cfg(test)]
mod tests {
    use super::BindingKind;

    #[test]
    fn binding_kind_clone_round_trip() {
        let kinds = [
            BindingKind::Bus,
            BindingKind::Literal,
            BindingKind::NodeProp,
        ];
        for kind in kinds {
            let cloned = kind;
            assert_eq!(kind, cloned);
        }
    }

    #[test]
    fn binding_kind_debug_prints() {
        let k = BindingKind::Bus;
        let s = alloc::format!("{:?}", k);
        assert_eq!(s, "Bus");
    }

    #[test]
    fn binding_kind_pattern_match_variants() {
        let k = BindingKind::NodeProp;
        assert!(matches!(k, BindingKind::NodeProp));
        assert!(!matches!(k, BindingKind::Bus));
        assert!(!matches!(k, BindingKind::Literal));
    }
}
