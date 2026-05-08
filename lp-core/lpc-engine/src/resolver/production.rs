//! Resolved value plus production provenance for the engine cache.

use crate::binding::BindingId;
use crate::runtime_product::{RuntimeProduct, RuntimeProductError};
use lpc_model::{NodeId, SlotPath, WithRevision};
use lps_shared::LpsValueF32;

/// One cached production: versioned runtime product and where it came from.
#[derive(Clone, Debug)]
pub struct Production {
    pub product: WithRevision<RuntimeProduct>,
    pub source: ProductionSource,
}

impl Production {
    pub fn new(product: WithRevision<RuntimeProduct>, source: ProductionSource) -> Self {
        Self { product, source }
    }

    pub fn value(
        value: WithRevision<LpsValueF32>,
        source: ProductionSource,
    ) -> Result<Self, RuntimeProductError> {
        let frame = value.changed_at();
        let product = RuntimeProduct::try_value(value.into_value())?;
        Ok(Self::new(WithRevision::new(frame, product), source))
    }

    pub fn as_value(&self) -> Option<&LpsValueF32> {
        self.product.get().as_value()
    }
}

/// Provenance for a [`Production`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionSource {
    Literal,
    Default,
    ProducedSlot { node: NodeId, slot: SlotPath },
    BusBinding { binding: BindingId },
}

#[cfg(test)]
mod tests {
    use super::{Production, ProductionSource};
    use crate::binding::BindingId;
    use crate::runtime_product::{RuntimeProduct, RuntimeProductError};
    use lpc_model::NodeId;
    use lpc_model::Revision;
    use lpc_model::SlotPath;
    use lpc_model::WithRevision;
    use lps_shared::{LpsTexture2DDescriptor, LpsTexture2DValue, LpsValueF32};

    #[test]
    fn production_value_rejects_texture2d() {
        let tv = LpsTexture2DValue::from_guest_descriptor(LpsTexture2DDescriptor {
            ptr: 0,
            width: 1,
            height: 1,
            row_stride: 4,
        });
        assert!(matches!(
            Production::value(
                WithRevision::new(Revision::new(1), LpsValueF32::Texture2D(tv)),
                ProductionSource::Literal,
            ),
            Err(RuntimeProductError::Texture2dValueNotRuntimeProduct),
        ));
    }

    #[test]
    fn production_holds_versioned_runtime_product_and_source() {
        let v = WithRevision::new(Revision::new(3), LpsValueF32::F32(1.25));
        let pv = Production::value(
            v,
            ProductionSource::ProducedSlot {
                node: NodeId::new(9),
                slot: SlotPath::parse("result").unwrap(),
            },
        )
        .expect("production");
        assert!(matches!(
            pv.product.get(),
            RuntimeProduct::Value(inner) if inner.eq(&LpsValueF32::F32(1.25))
        ));
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(1.25)));
        assert_eq!(pv.product.changed_at(), Revision::new(3));
        assert_eq!(
            pv.source,
            ProductionSource::ProducedSlot {
                node: NodeId::new(9),
                slot: SlotPath::parse("result").unwrap(),
            }
        );

        let pv2 = Production::value(
            WithRevision::new(Revision::new(1), LpsValueF32::F32(2.0)),
            ProductionSource::BusBinding {
                binding: BindingId::new(4),
            },
        )
        .expect("production");
        assert_eq!(
            pv2.source,
            ProductionSource::BusBinding {
                binding: BindingId::new(4),
            }
        );
        assert!(matches!(pv2.product.get(), RuntimeProduct::Value(_)));
    }

    #[test]
    fn production_value_preserves_revision() {
        let frame = Revision::new(42);
        let v = WithRevision::new(frame, LpsValueF32::F32(-0.5));
        let pv = Production::value(v, ProductionSource::Literal).expect("production");
        assert_eq!(pv.product.changed_at(), frame);
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(-0.5)));
    }
}
