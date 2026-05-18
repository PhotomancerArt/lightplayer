//! Graph-level control product handle.
//!
//! A [`ControlProduct`] is the value that moves through node slots when a node
//! can render logical device-control samples for an output. It is intentionally
//! small: the output owns the destination buffer and the engine dispatches the
//! render request back to the owning runtime node.

use crate::NodeId;

/// Preferred two-dimensional extent for logical control samples.
///
/// The axis names deliberately avoid DMX universe vocabulary. Outputs may map
/// rows to DMX/E1.31/Art-Net universes, GPIO ports, PixLite outputs, or another
/// hardware-specific shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ControlExtent {
    pub rows: u32,
    pub samples_per_row: u32,
}

impl ControlExtent {
    #[must_use]
    pub const fn new(rows: u32, samples_per_row: u32) -> Self {
        Self {
            rows,
            samples_per_row,
        }
    }

    #[must_use]
    pub const fn sample_count(self) -> u32 {
        self.rows.saturating_mul(self.samples_per_row)
    }
}

/// Logical control product produced by a node output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ControlProduct {
    node: NodeId,
    output: u32,
    preferred_extent: ControlExtent,
}

impl ControlProduct {
    #[must_use]
    pub const fn new(node: NodeId, output: u32, preferred_extent: ControlExtent) -> Self {
        Self {
            node,
            output,
            preferred_extent,
        }
    }

    #[must_use]
    pub const fn node(self) -> NodeId {
        self.node
    }

    #[must_use]
    pub const fn output(self) -> u32 {
        self.output
    }

    #[must_use]
    pub const fn preferred_extent(self) -> ControlExtent {
        self.preferred_extent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_product_keeps_owner_output_and_extent() {
        let extent = ControlExtent::new(2, 600);
        let product = ControlProduct::new(NodeId::new(7), 3, extent);

        assert_eq!(product.node(), NodeId::new(7));
        assert_eq!(product.output(), 3);
        assert_eq!(product.preferred_extent(), extent);
        assert_eq!(extent.sample_count(), 1200);
    }
}
