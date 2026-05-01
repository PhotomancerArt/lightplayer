//! Resolved value plus production provenance for the engine cache.

use crate::binding::BindingId;
use crate::runtime_product::RuntimeProduct;
use lpc_model::{NodeId, PropPath, Versioned};
use lps_shared::LpsValueF32;

/// One cached production: versioned runtime product and where it came from.
#[derive(Clone, Debug)]
pub struct Production {
    pub product: Versioned<RuntimeProduct>,
    pub source: ProductionSource,
}

impl Production {
    pub fn new(product: Versioned<RuntimeProduct>, source: ProductionSource) -> Self {
        Self { product, source }
    }

    pub fn value(value: Versioned<LpsValueF32>, source: ProductionSource) -> Self {
        let frame = value.changed_frame();
        Self::new(
            Versioned::new(frame, RuntimeProduct::value(value.into_value())),
            source,
        )
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
    NodeOutput { node: NodeId, output: PropPath },
    BusBinding { binding: BindingId },
}

#[cfg(test)]
mod tests {
    use super::{Production, ProductionSource};
    use crate::binding::BindingId;
    use crate::runtime_product::RuntimeProduct;
    use lpc_model::FrameId;
    use lpc_model::NodeId;
    use lpc_model::Versioned;
    use lpc_model::prop::prop_path::parse_path;
    use lps_shared::LpsValueF32;

    #[test]
    fn production_holds_versioned_runtime_product_and_source() {
        let v = Versioned::new(FrameId::new(3), LpsValueF32::F32(1.25));
        let pv = Production::value(
            v,
            ProductionSource::NodeOutput {
                node: NodeId::new(9),
                output: parse_path("result").unwrap(),
            },
        );
        assert!(matches!(
            pv.product.get(),
            RuntimeProduct::Value(inner) if inner.eq(&LpsValueF32::F32(1.25))
        ));
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(1.25)));
        assert_eq!(pv.product.changed_frame(), FrameId::new(3));
        assert_eq!(
            pv.source,
            ProductionSource::NodeOutput {
                node: NodeId::new(9),
                output: parse_path("result").unwrap(),
            }
        );

        let pv2 = Production::value(
            Versioned::new(FrameId::new(1), LpsValueF32::F32(2.0)),
            ProductionSource::BusBinding {
                binding: BindingId::new(4),
            },
        );
        assert_eq!(
            pv2.source,
            ProductionSource::BusBinding {
                binding: BindingId::new(4),
            }
        );
        assert!(matches!(pv2.product.get(), RuntimeProduct::Value(_)));
    }

    #[test]
    fn production_value_preserves_changed_frame() {
        let frame = FrameId::new(42);
        let v = Versioned::new(frame, LpsValueF32::F32(-0.5));
        let pv = Production::value(v, ProductionSource::Literal);
        assert_eq!(pv.product.changed_frame(), frame);
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(-0.5)));
    }
}
