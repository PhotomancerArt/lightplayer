//! Lazy graph products.
//!
//! Products are values, not resources. A product is a node-owned capability that
//! can be materialized by asking the owning node to render/fill a caller-owned
//! target. Resources are different: they are registry-owned payload objects with
//! ids, summaries, and optional byte payload sync.

pub mod product_ref;

pub use crate::products::control::{
    ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d, ControlProduct,
    ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan,
};
pub use crate::products::visual::VisualProduct;
pub use product_ref::{ProductKind, ProductRef};
