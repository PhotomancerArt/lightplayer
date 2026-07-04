//! Concrete semantic slot field types.
//!
//! [`ValueSlot`](crate::ValueSlot) is the generic revision-tracked leaf storage.
//! Types in this module are domain-specific leaves that own their shape,
//! metadata, editor hints, and conversion semantics.

mod affine2d;
mod artifact_path;
mod asset_slot;
mod color_order;
mod control_product;
mod dim2u;
pub mod node_invocation_slot;
mod positive_f32;
mod ratio;
mod relative_node_ref;
mod render_order;
mod resource_ref;
mod u32_list;
mod visual_product;
mod xy;

pub use affine2d::{Affine2d, Affine2dSlot};
pub use artifact_path::{ArtifactPath, ArtifactPathSlot};
pub(crate) use asset_slot::ASSET_SLOT_CODEC_ID;
pub use asset_slot::{AssetSlot, AssetSlotValue};
pub use color_order::{ColorOrderSlot, ColorOrderValue};
pub use control_product::ControlProductSlot;
pub use dim2u::{Dim2u, Dim2uSlot};
pub use positive_f32::{PositiveF32, PositiveF32Slot};
pub use ratio::{Ratio, RatioSlot};
pub use relative_node_ref::RelativeNodeRefSlot;
pub use render_order::{RenderOrder, RenderOrderSlot};
pub use resource_ref::ResourceRefSlot;
pub use visual_product::VisualProductSlot;
pub use xy::{Xy, XySlot};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RelativeNodeRef, ResourceRef, Revision, RuntimeBufferId, set_current_revision};
    use alloc::string::String;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct SemanticSlots {
        ratio: RatioSlot,
        positive: PositiveF32Slot,
        render_order: RenderOrderSlot,
        xy: XySlot,
        asset: AssetSlot,
        artifact_path: ArtifactPathSlot,
        dim: Dim2uSlot,
        transform: Affine2dSlot,
        color_order: ColorOrderSlot,
        texture_loc: RelativeNodeRefSlot,
        resource: ResourceRefSlot,
    }

    #[test]
    fn semantic_slots_serialize_as_authored_values_and_stamp_deserialize_version() {
        set_current_revision(Revision::new(10));
        let slots = SemanticSlots {
            ratio: RatioSlot::new(Ratio(0.75)),
            positive: PositiveF32Slot::new(PositiveF32(2.0)),
            render_order: RenderOrderSlot::new(RenderOrder(10)),
            xy: XySlot::new(Xy([1.0, 2.0])),
            asset: AssetSlot::path("shader.glsl"),
            artifact_path: ArtifactPathSlot::new(ArtifactPath(String::from("./shader.json"))),
            dim: Dim2uSlot::new(Dim2u {
                width: 64,
                height: 32,
            }),
            transform: Affine2dSlot::new(Affine2d::identity()),
            color_order: ColorOrderSlot::new(ColorOrderValue::Grb),
            texture_loc: RelativeNodeRefSlot::new(RelativeNodeRef::parse("..texture").unwrap()),
            resource: ResourceRefSlot::new(ResourceRef::runtime_buffer(RuntimeBufferId::new(4))),
        };

        let authored = serde_json::to_string_pretty(&slots).unwrap();
        assert!(authored.contains("\"ratio\": 0.75"));
        assert!(authored.contains("\"asset\": \"shader.glsl\""));
        assert!(authored.contains("\"color_order\": \"grb\""));
        assert!(authored.contains("\"texture_loc\": \"..texture\""));

        let decoded: SemanticSlots = serde_json::from_str(&authored).unwrap();

        let stamped_revision = decoded.ratio.revision();
        assert_eq!(decoded.dim.revision(), stamped_revision);
        assert_eq!(decoded.transform.revision(), stamped_revision);
        assert_eq!(decoded.color_order.value(), &ColorOrderValue::Grb);
        assert_eq!(
            decoded.resource.value(),
            &ResourceRef::runtime_buffer(RuntimeBufferId::new(4))
        );
    }
}
