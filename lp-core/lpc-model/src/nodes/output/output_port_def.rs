use crate::{HwEndpointSpec, OptionSlot, Slotted, ValueSlot};

/// One physical wire driven by an output node.
///
/// An output node owns a single control buffer and splits it across its
/// ports in key order; each port names the wire it drives and how many
/// lamps of that buffer belong to it.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct OutputPortDef {
    /// Wire this port drives, e.g. `ws281x:local:IO18`.
    pub endpoint: ValueSlot<HwEndpointSpec>,

    /// Lamp count carried by this wire.
    ///
    /// Only the highest-keyed port may omit it — an absent count means
    /// "the remainder of the node's control product", so a single-entry map
    /// with no count drives the whole extent.
    pub count: OptionSlot<ValueSlot<u32>>,
}

impl OutputPortDef {
    pub fn new(endpoint: HwEndpointSpec) -> Self {
        Self {
            endpoint: ValueSlot::new(endpoint),
            count: OptionSlot::none(),
        }
    }

    pub fn with_count(endpoint: HwEndpointSpec, count: u32) -> Self {
        Self {
            endpoint: ValueSlot::new(endpoint),
            count: OptionSlot::some(ValueSlot::new(count)),
        }
    }

    pub fn endpoint(&self) -> &HwEndpointSpec {
        self.endpoint.value()
    }

    pub fn count(&self) -> Option<u32> {
        self.count.data.as_ref().map(|count| *count.value())
    }
}

impl Default for OutputPortDef {
    fn default() -> Self {
        Self::new(super::OutputDef::default_endpoint())
    }
}
