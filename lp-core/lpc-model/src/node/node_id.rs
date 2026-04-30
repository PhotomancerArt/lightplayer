use core::fmt;

/// Opaque **runtime** node id: a compact `u32` handle, never a place-holding
/// string in authored TOML.
///
/// v0 uses `u32` for embedded *performance* (Copy, no heap, cheap compare/hash)
/// instead of a base-62 string, per
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md` (Uid
/// decision). **Authored** identity uses [`NodePath`](crate::TreePath) and
/// [`NodePropSpec`](crate::NodePropSpec), not
/// a serialized Uid in artifact files (see same summary: “runtime-only”). Serde
/// derives on this type support schema/tests in this crate; persisted authored
/// graphs use string paths (e.g. [`NodePath`](crate::TreePath)), not embedding Uid in TOML, per
/// the same M2 “runtime-only / addressing split” story.
#[derive(
    Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodeId(pub u32);

impl NodeId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::NodeId;
    use alloc::string::ToString;
    use core::hash::{Hash, Hasher};

    #[test]
    fn node_id_display_decimal() {
        assert_eq!(NodeId(0).to_string(), "0");
        assert_eq!(NodeId(7).to_string(), "7");
        assert_eq!(NodeId(u32::MAX).to_string(), u32::MAX.to_string());
    }

    #[test]
    fn node_id_equality_and_hashing() {
        #[derive(Default)]
        struct TestHasher(u64);
        impl Hasher for TestHasher {
            fn finish(&self) -> u64 {
                self.0
            }
            fn write(&mut self, bytes: &[u8]) {
                for &b in bytes {
                    self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(b));
                }
            }
        }

        let a = NodeId(5);
        let b = NodeId(5);
        let c = NodeId(7);
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut ha = TestHasher::default();
        let mut hb = TestHasher::default();
        let mut hc = TestHasher::default();
        a.hash(&mut ha);
        b.hash(&mut hb);
        c.hash(&mut hc);
        assert_eq!(ha.finish(), hb.finish());
        assert_ne!(ha.finish(), hc.finish());
    }
}
