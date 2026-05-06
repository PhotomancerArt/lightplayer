pub fn register_shapes(registry: &mut lpc_model::SlotShapeRegistry) {
    crate::source::register_shapes(registry);
    crate::engine::register_shapes(registry);
}
