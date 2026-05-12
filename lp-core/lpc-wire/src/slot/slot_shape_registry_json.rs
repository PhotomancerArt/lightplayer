//! Direct JSON writer for slot shape registries.

use alloc::format;
use lpc_model::SlotShapeRegistry;

use crate::json::json_writer::{JsonValue, JsonWriterError};

/// Write a slot shape registry snapshot shape without cloning the registry.
///
/// The emitted JSON matches [`lpc_model::SlotShapeRegistrySnapshot`].
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
    for (id, entry) in registry.iter() {
        shapes.prop(&format!("{}", id.raw()))?.serde(entry)?;
    }
    shapes.finish()?;

    object.finish()
}
