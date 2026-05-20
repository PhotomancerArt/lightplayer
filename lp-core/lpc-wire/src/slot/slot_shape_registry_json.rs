//! Direct JSON writer for slot shape registries.

use alloc::format;
use lpc_model::SlotShapeRegistry;

use crate::json::json_writer::{JsonValue, JsonWriterError};

/// Write a slot shape registry snapshot shape without cloning the registry.
///
/// The emitted JSON matches [`lpc_model::SlotShapeRegistrySnapshot`] and
/// includes static catalog shapes plus dynamic registry entries.
pub fn write_slot_shape_registry_snapshot_json<W>(
    value: JsonValue<'_, W>,
    registry: &SlotShapeRegistry,
) -> Result<(), JsonWriterError<W::Error>>
where
    W: crate::json::json_write::JsonWrite,
{
    let mut object = value.object()?;
    object.prop("ids_revision")?.serde(&registry.ids_revision)?;

    let mut shapes = object.prop("shapes")?.object()?;
    for id in lpc_model::slot_shapes::static_slot_shape_ids()
        .iter()
        .copied()
    {
        if registry.get(&id).is_none()
            && let Some(entry) = SlotShapeRegistry::static_catalog_entry(id)
        {
            shapes.prop(&format!("{}", id.raw()))?.serde(&entry)?;
        }
    }
    for (id, entry) in registry.iter() {
        shapes.prop(&format!("{}", id.raw()))?.serde(entry)?;
    }
    shapes.finish()?;

    object.finish()
}
