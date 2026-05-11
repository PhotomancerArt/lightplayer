//! Resolved slot value plus production provenance for the engine cache.

use crate::binding::BindingRef;
use crate::resolver::resolver::model_value_to_lps_value_f32;
use crate::wire_bridge::{LpsValueToModelConversionError, lps_value_f32_to_model_value};
use lpc_model::{LpValue, NodeId, SlotPath, WithRevision};
use lps_shared::LpsValueF32;

/// One cached production: versioned slot value and where it came from.
#[derive(Clone, Debug)]
pub struct Production {
    pub product: WithRevision<LpValue>,
    pub source: ProductionSource,
}

impl Production {
    pub fn new(product: WithRevision<LpValue>, source: ProductionSource) -> Self {
        Self { product, source }
    }

    pub fn value(
        value: WithRevision<LpsValueF32>,
        source: ProductionSource,
    ) -> Result<Self, LpsValueToModelConversionError> {
        let revision = value.changed_at();
        let product = lps_value_f32_to_model_value(value.value())?;
        Ok(Self::new(WithRevision::new(revision, product), source))
    }

    pub fn as_value(&self) -> Option<LpsValueF32> {
        model_value_to_lps_value_f32(self.product.value()).ok()
    }
}

/// Provenance for a [`Production`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionSource {
    Literal,
    Default,
    ProducedSlot { node: NodeId, slot: SlotPath },
    BusBinding { binding: BindingRef },
}

#[cfg(test)]
mod tests {
    use super::{Production, ProductionSource};
    use crate::binding::BindingRef;
    use crate::wire_bridge::LpsValueToModelConversionError;
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
            Err(LpsValueToModelConversionError::Texture2dNotPortable),
        ));
    }

    #[test]
    fn production_holds_versioned_value_and_source() {
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
            lpc_model::LpValue::F32(inner) if inner.eq(&1.25)
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
                binding: BindingRef::new(NodeId::new(4), 0),
            },
        )
        .expect("production");
        assert_eq!(
            pv2.source,
            ProductionSource::BusBinding {
                binding: BindingRef::new(NodeId::new(4), 0),
            }
        );
        assert!(matches!(pv2.product.get(), lpc_model::LpValue::F32(_)));
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
