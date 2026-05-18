pub fn register_shapes(
    registry: &mut lpc_model::SlotShapeRegistry,
) -> Result<(), lpc_model::SlotShapeRegistryError> {
    lpc_model::slot_shapes::register_all_static_slot_shapes(registry)?;
    crate::slot_shapes::register_all_static_slot_shapes(registry)
}
