use crate::resource::resource_domain::ResourceDomain;
use crate::resource::runtime_buffer_id::RuntimeBufferId;
use crate::resource::visual_product_id::VisualProductId;

/// Stable resource reference: domain plus raw id (no generation).
///
/// Ids are not reused within a loaded project runtime; removed ids stay invalid.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
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
    pub const fn visual_product(product_id: VisualProductId) -> Self {
        Self {
            domain: ResourceDomain::VisualProduct,
            id: product_id.as_u32(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::resource::resource_domain::ResourceDomain;
    use crate::resource::resource_ref::ResourceRef;
    use crate::resource::runtime_buffer_id::RuntimeBufferId;
    use crate::resource::visual_product_id::VisualProductId;

    #[test]
    fn resource_ref_covers_buffer_and_visual_product() {
        let buf = RuntimeBufferId::new(7);
        let rbuf = ResourceRef::runtime_buffer(buf);
        assert_eq!(rbuf.domain, ResourceDomain::RuntimeBuffer);
        assert_eq!(rbuf.id, 7);

        let prod = VisualProductId::new(11);
        let rprod = ResourceRef::visual_product(prod);
        assert_eq!(rprod.domain, ResourceDomain::VisualProduct);
        assert_eq!(rprod.id, 11);
    }
}
