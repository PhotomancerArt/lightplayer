use alloc::format;
use alloc::string::String;

use crate::{
    SlotCustomAccess, SlotCustomMutAccess, SlotDataAccess, SlotShapeId, SlotShapeRegistry,
};

use super::{
    SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError, SyntaxEventSource, ValueReader,
};

pub(crate) fn read_custom_slot<S>(
    codec: SlotShapeId,
    data: &mut dyn SlotCustomMutAccess,
    _registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    if codec == crate::slots::ASSET_SLOT_CODEC_ID {
        let Some(slot) = data.as_any_mut().downcast_mut::<crate::slots::AssetSlot>() else {
            value.skip_value()?;
            return Err(SyntaxError::new(
                "",
                None,
                "asset slot codec expected AssetSlot data",
            ));
        };
        return slot.read_slot(value);
    }

    value.skip_value()?;
    Err(SyntaxError::new(
        "",
        None,
        format!("unknown custom slot codec {codec}"),
    ))
}

pub(crate) fn write_custom_slot_json<W>(
    codec: SlotShapeId,
    data: &dyn SlotCustomAccess,
    _registry: &SlotShapeRegistry,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    if codec == crate::slots::ASSET_SLOT_CODEC_ID {
        let Some(slot) = data.as_any().downcast_ref::<crate::slots::AssetSlot>() else {
            return Err(SlotWriteError::InvalidSlotData(
                "asset slot codec expected AssetSlot data".into(),
            ));
        };
        return slot.write_slot_json(value);
    }

    Err(SlotWriteError::InvalidSlotData(format!(
        "unknown custom slot codec {codec}"
    )))
}

pub(crate) fn snapshot_custom_slot_data<'a>(
    codec: SlotShapeId,
    data: &'a dyn SlotCustomAccess,
) -> Result<SlotDataAccess<'a>, String> {
    if codec == crate::slots::ASSET_SLOT_CODEC_ID {
        let Some(slot) = data.as_any().downcast_ref::<crate::slots::AssetSlot>() else {
            return Err("asset slot codec expected AssetSlot data".into());
        };
        return Ok(SlotDataAccess::Value(slot));
    }

    Err(format!("unknown custom slot codec {codec}"))
}
