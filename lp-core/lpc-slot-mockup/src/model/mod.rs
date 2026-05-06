mod shape_builder;

pub(crate) use shape_builder::{
    field, id, map, mapping_shape, option, record, reference, value, version,
};

pub fn register_shapes(registry: &mut lpc_model::SlotShapeRegistry) {
    crate::source::register_shapes(registry);
    crate::engine::register_shapes(registry);
}
