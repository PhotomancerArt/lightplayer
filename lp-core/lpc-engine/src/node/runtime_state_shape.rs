//! Named registration path for node-owned runtime state shapes.

use lpc_model::{SlotShapeRegistry, SlotShapeRegistryError, StaticSlotShape};

/// Fixed Rust-authored shape for a runtime state root exposed by a live node.
///
/// Authored project model shapes should be resolved from the generated static
/// catalog. Artifact- or instance-specific runtime shapes should be registered
/// by their owner. This trait is for the middle case: stable engine runtime
/// state roots such as `ShaderState` and `ButtonState`.
pub trait RuntimeStateShape: StaticSlotShape {
    fn register_runtime_state_shape(
        registry: &mut SlotShapeRegistry,
    ) -> Result<bool, SlotShapeRegistryError> {
        match Self::shape_name() {
            Some(name) => {
                registry.ensure_runtime_state_shape_named(Self::SHAPE_ID, name, Self::slot_shape())
            }
            None => registry.ensure_runtime_state_shape(Self::SHAPE_ID, Self::slot_shape()),
        }
    }
}

impl<T> RuntimeStateShape for T where T: StaticSlotShape {}
