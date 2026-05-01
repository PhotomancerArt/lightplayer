//! Resolved value plus production provenance for the engine cache.

use crate::binding::BindingId;
use lpc_model::{NodeId, PropPath, Versioned};
use lps_shared::LpsValueF32;

/// One cached production: versioned runtime value and where it came from.
#[derive(Clone, Debug)]
pub struct ProducedValue {
    pub value: Versioned<LpsValueF32>,
    pub source: ProductionSource,
}

impl ProducedValue {
    pub fn new(value: Versioned<LpsValueF32>, source: ProductionSource) -> Self {
        Self { value, source }
    }
}

/// Provenance for a [`ProducedValue`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionSource {
    Literal,
    Default,
    NodeOutput { node: NodeId, output: PropPath },
    BusBinding { binding: BindingId },
}

#[cfg(test)]
mod tests {
    use super::{ProducedValue, ProductionSource};
    use crate::binding::BindingId;
    use lpc_model::FrameId;
    use lpc_model::NodeId;
    use lpc_model::Versioned;
    use lpc_model::prop::prop_path::parse_path;
    use lps_shared::LpsValueF32;

    #[test]
    fn produced_value_holds_versioned_and_source() {
        let v = Versioned::new(FrameId::new(3), LpsValueF32::F32(1.25));
        let pv = ProducedValue::new(
            v,
            ProductionSource::NodeOutput {
                node: NodeId::new(9),
                output: parse_path("result").unwrap(),
            },
        );
        assert!(pv.value.get().eq(&LpsValueF32::F32(1.25)));
        assert_eq!(pv.value.changed_frame(), FrameId::new(3));
        assert_eq!(
            pv.source,
            ProductionSource::NodeOutput {
                node: NodeId::new(9),
                output: parse_path("result").unwrap(),
            }
        );

        let pv2 = ProducedValue::new(
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
    }
}
