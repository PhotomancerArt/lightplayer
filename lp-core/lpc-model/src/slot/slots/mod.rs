//! Concrete semantic slot field types.
//!
//! [`ValueSlot`](crate::ValueSlot) is the generic versioned leaf storage.
//! Types in this module are domain-specific leaves that own their shape,
//! metadata, editor hints, and conversion semantics.

mod affine2d;
mod artifact_path;
mod color_order;
mod dim2u;
mod positive_f32;
mod ratio;
mod relative_node_ref;
mod render_order;
mod resource_ref;
mod source_path;
mod xy;

pub use affine2d::{Affine2d, Affine2dSlot, affine2d_shape};
pub use artifact_path::{ArtifactPathSlot, artifact_path_shape};
pub use color_order::{ColorOrderSlot, ColorOrderValue, color_order_shape};
pub use dim2u::{Dim2u, Dim2uSlot, dim2u_shape};
pub use positive_f32::{PositiveF32Slot, positive_f32_shape};
pub use ratio::{RatioSlot, ratio_shape};
pub use relative_node_ref::{RelativeNodeRefSlot, relative_node_ref_shape};
pub use render_order::{RenderOrderSlot, render_order_shape};
pub use resource_ref::{
    ResourceRefSlot, render_product_resource_shape, resource_ref_shape,
    runtime_buffer_resource_shape,
};
pub use source_path::{SourcePathSlot, source_path_shape};
pub use xy::{XySlot, xy_shape};
