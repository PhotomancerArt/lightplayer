//! Shared identity for runtime buffers and render products.
//!
//! Store implementations in `lpc-engine` allocate ids monotonically for the
//! lifetime of a loaded project runtime and do not reuse ids after removal.

/// Small, copyable identifier for a runtime buffer; suitable as a map key.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct RuntimeBufferId(u32);

impl RuntimeBufferId {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// Small, copyable identifier for a render product; suitable as a map key.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct RenderProductId(u32);

impl RenderProductId {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// Which resource family a [`ResourceRef`] refers to.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceDomain {
    RuntimeBuffer,
    RenderProduct,
}

/// Stable resource reference: domain plus raw id (no generation).
///
/// Ids are not reused within a loaded project runtime; removed ids stay invalid.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ResourceRef {
    pub domain: ResourceDomain,
    pub id: u32,
}

impl ResourceRef {
    #[must_use]
    pub const fn runtime_buffer(buffer_id: RuntimeBufferId) -> Self {
        Self {
            domain: ResourceDomain::RuntimeBuffer,
            id: buffer_id.as_u32(),
        }
    }

    #[must_use]
    pub const fn render_product(product_id: RenderProductId) -> Self {
        Self {
            domain: ResourceDomain::RenderProduct,
            id: product_id.as_u32(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RenderProductId, ResourceDomain, ResourceRef, RuntimeBufferId};

    #[test]
    fn runtime_buffer_id_round_trips_raw() {
        let id = RuntimeBufferId::new(42);
        assert_eq!(id.as_u32(), 42);
        assert_eq!(RuntimeBufferId::new(id.as_u32()), id);
    }

    #[test]
    fn render_product_id_round_trips_raw() {
        let id = RenderProductId::new(42);
        assert_eq!(id.as_u32(), 42);
        assert_eq!(RenderProductId::new(id.as_u32()), id);
    }

    #[test]
    fn resource_ref_covers_buffer_and_render_product() {
        let buf = RuntimeBufferId::new(7);
        let rbuf = ResourceRef::runtime_buffer(buf);
        assert_eq!(rbuf.domain, ResourceDomain::RuntimeBuffer);
        assert_eq!(rbuf.id, 7);

        let prod = RenderProductId::new(11);
        let rprod = ResourceRef::render_product(prod);
        assert_eq!(rprod.domain, ResourceDomain::RenderProduct);
        assert_eq!(rprod.id, 11);
    }
}
