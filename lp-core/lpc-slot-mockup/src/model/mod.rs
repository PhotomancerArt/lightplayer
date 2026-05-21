pub fn register_shapes(
    registry: &mut lpc_model::SlotShapeRegistry,
) -> Result<(), lpc_model::SlotShapeRegistryError> {
    register_default_shape::<crate::engine::FixtureNode>(registry)?;
    register_default_shape::<crate::engine::OutputNode>(registry)?;
    register_default_shape::<crate::engine::TouchState>(registry)?;
    register_default_shape::<crate::source::FixtureDef>(registry)?;
    register_default_shape::<crate::source::NodeInvocationDef>(registry)?;
    register_default_shape::<crate::source::OutputDef>(registry)?;
    register_default_shape::<crate::source::OutputDriverOptionsConfig>(registry)?;
    register_default_shape::<crate::source::ProjectDef>(registry)?;
    register_default_shape::<crate::source::ScalarHint>(registry)?;
    register_default_shape::<crate::source::ShaderDef>(registry)?;
    register_default_shape::<crate::source::ShaderParamDef>(registry)?;
    register_default_shape::<crate::source::TextureDef>(registry)?;
    Ok(())
}

fn register_default_shape<T>(
    registry: &mut lpc_model::SlotShapeRegistry,
) -> Result<(), lpc_model::SlotShapeRegistryError>
where
    T: lpc_model::StaticSlotShape + lpc_model::SlotMutAccess + Default + 'static,
{
    T::ensure_default_registered::<T>(registry).map(|_| ())
}
