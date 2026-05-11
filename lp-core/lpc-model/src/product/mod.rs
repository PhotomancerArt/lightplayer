//! Lazy graph products.
//!
//! Products are values, not resources. A product is a node-owned capability that
//! can be materialized by asking the owning node to render/fill a caller-owned
//! target. Resources are different: they are registry-owned payload objects with
//! ids, summaries, and optional byte payload sync.

pub mod control_product;
pub mod product_ref;
pub mod visual_product;

pub use control_product::{ControlExtent, ControlProduct};
pub use product_ref::{ProductKind, ProductRef};
pub use visual_product::VisualProduct;
