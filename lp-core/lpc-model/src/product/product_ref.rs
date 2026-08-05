//! Lazy graph products that move through slots as values.
//!
//! A product is not a stored payload. It is a small, copyable graph value that
//! says "ask this node output to materialize this kind of data when needed".
//! Products are node-owned capabilities; resources are registry-owned payloads.

use crate::{ControlProduct, TimeProduct, VisualProduct};

/// Lazy product handle carried by [`crate::LpValue::Product`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProductRef {
    /// Renderable visual material, usually produced by shader-like nodes.
    Visual(VisualProduct),
    /// Logical device-control material, usually produced by fixture-like nodes.
    Control(ControlProduct),
    /// Queryable timebase material, usually produced by clock-like nodes.
    Time(TimeProduct),
}

impl ProductRef {
    #[must_use]
    pub const fn visual(product: VisualProduct) -> Self {
        Self::Visual(product)
    }

    #[must_use]
    pub const fn control(product: ControlProduct) -> Self {
        Self::Control(product)
    }

    #[must_use]
    pub const fn time(product: TimeProduct) -> Self {
        Self::Time(product)
    }

    #[must_use]
    pub const fn as_visual(self) -> Option<VisualProduct> {
        match self {
            Self::Visual(product) => Some(product),
            Self::Control(_) | Self::Time(_) => None,
        }
    }

    #[must_use]
    pub const fn as_control(self) -> Option<ControlProduct> {
        match self {
            Self::Control(product) => Some(product),
            Self::Visual(_) | Self::Time(_) => None,
        }
    }

    #[must_use]
    pub const fn as_time(self) -> Option<TimeProduct> {
        match self {
            Self::Time(product) => Some(product),
            Self::Visual(_) | Self::Control(_) => None,
        }
    }
}

/// Structural kind for [`ProductRef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProductKind {
    Visual,
    Control,
    Time,
}

#[cfg(test)]
mod tests {
    use crate::{ControlExtent, ControlProduct, NodeId, ProductRef, TimeProduct, VisualProduct};

    #[test]
    fn product_ref_distinguishes_lazy_product_families() {
        let visual = VisualProduct::new(NodeId::new(1), 0);
        let control = ControlProduct::new(NodeId::new(2), 0, ControlExtent::new(1, 12));
        let time = TimeProduct::new(NodeId::new(3), 0);

        assert_eq!(ProductRef::visual(visual).as_visual(), Some(visual));
        assert_eq!(ProductRef::visual(visual).as_control(), None);
        assert_eq!(ProductRef::visual(visual).as_time(), None);
        assert_eq!(ProductRef::control(control).as_control(), Some(control));
        assert_eq!(ProductRef::control(control).as_visual(), None);
        assert_eq!(ProductRef::control(control).as_time(), None);
        assert_eq!(ProductRef::time(time).as_time(), Some(time));
        assert_eq!(ProductRef::time(time).as_visual(), None);
        assert_eq!(ProductRef::time(time).as_control(), None);
    }
}
