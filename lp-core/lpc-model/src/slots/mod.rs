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
mod u32_list;
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
pub use u32_list::u32_list_shape;
pub use xy::{XySlot, xy_shape};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RelativeNodeRef, ResourceRef, RuntimeBufferId, current_revision};
    use alloc::string::String;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct SemanticSlots {
        ratio: RatioSlot,
        positive: PositiveF32Slot,
        render_order: RenderOrderSlot,
        xy: XySlot,
        source_path: SourcePathSlot,
        artifact_path: ArtifactPathSlot,
        dim: Dim2uSlot,
        transform: Affine2dSlot,
        color_order: ColorOrderSlot,
        texture_loc: RelativeNodeRefSlot,
        resource: ResourceRefSlot,
    }

    #[test]
    fn semantic_slots_serialize_as_authored_values_and_stamp_deserialize_version() {
        let slots = SemanticSlots {
            ratio: RatioSlot::new(0.75),
            positive: PositiveF32Slot::new(2.0),
            render_order: RenderOrderSlot::new(10),
            xy: XySlot::new([1.0, 2.0]),
            source_path: SourcePathSlot::new(String::from("shader.glsl")),
            artifact_path: ArtifactPathSlot::new(String::from("./shader.toml")),
            dim: Dim2uSlot::new(Dim2u {
                width: 64,
                height: 32,
            }),
            transform: Affine2dSlot::new(Affine2d::identity()),
            color_order: ColorOrderSlot::new(ColorOrderValue::Grb),
            texture_loc: RelativeNodeRefSlot::new(RelativeNodeRef::parse("..texture").unwrap()),
            resource: ResourceRefSlot::new(ResourceRef::runtime_buffer(RuntimeBufferId::new(4))),
        };

        let authored = toml::to_string_pretty(&slots).unwrap();
        assert!(authored.contains("ratio = 0.75"));
        assert!(authored.contains("source_path = \"shader.glsl\""));
        assert!(authored.contains("color_order = \"grb\""));
        assert!(authored.contains("texture_loc = \"..texture\""));

        let expected_version = current_revision();
        let decoded: SemanticSlots = toml::from_str(&authored).unwrap();

        assert_eq!(decoded.ratio.changed_frame(), expected_version);
        assert_eq!(decoded.dim.changed_frame(), expected_version);
        assert_eq!(decoded.transform.changed_frame(), expected_version);
        assert_eq!(decoded.color_order.value(), &ColorOrderValue::Grb);
        assert_eq!(
            decoded.resource.value(),
            &ResourceRef::runtime_buffer(RuntimeBufferId::new(4))
        );
    }
}
