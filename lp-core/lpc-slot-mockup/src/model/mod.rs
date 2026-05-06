use lpc_model::{
    SlotMeta, SlotShape, positive_f32_shape,
    slot::shape::{field, leaf, record, unit, variant},
    xy_shape,
};

pub(crate) fn mapping_shape() -> SlotShape {
    SlotShape::Enum {
        meta: SlotMeta::empty(),
        variants: vec![
            variant(
                "circle",
                record(vec![
                    field("center", leaf(xy_shape())),
                    field("radius", leaf(positive_f32_shape())),
                ]),
            ),
            variant(
                "square",
                record(vec![
                    field("origin", leaf(xy_shape())),
                    field("size", leaf(xy_shape())),
                ]),
            ),
            variant("disabled", unit()),
        ],
    }
}

pub fn register_shapes(registry: &mut lpc_model::SlotShapeRegistry) {
    crate::source::register_shapes(registry);
    crate::engine::register_shapes(registry);
}
