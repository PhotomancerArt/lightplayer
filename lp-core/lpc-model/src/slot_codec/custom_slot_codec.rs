use alloc::format;
use alloc::string::String;

use crate::{
    SlotCustomAccess, SlotCustomMutAccess, SlotDataAccess, SlotShapeId, SlotShapeRegistry,
};

use super::{
    SlotDataWriteError, SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError, SyntaxEventSource,
    ValueReader,
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
    if codec == crate::slots::SOURCE_FILE_CODEC_ID {
        let Some(slot) = data
            .as_any_mut()
            .downcast_mut::<crate::slots::SourceFileSlot>()
        else {
            value.skip_value()?;
            return Err(SyntaxError::new(
                "",
                None,
                "source file codec expected SourceFileSlot data",
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
    if codec == crate::slots::SOURCE_FILE_CODEC_ID {
        let Some(slot) = data.as_any().downcast_ref::<crate::slots::SourceFileSlot>() else {
            return Err(SlotWriteError::InvalidSlotData(
                "source file codec expected SourceFileSlot data".into(),
            ));
        };
        return slot.write_slot_json(value);
    }

    Err(SlotWriteError::InvalidSlotData(format!(
        "unknown custom slot codec {codec}"
    )))
}

pub(crate) fn write_custom_slot_toml(
    codec: SlotShapeId,
    data: &dyn SlotCustomAccess,
    _registry: &SlotShapeRegistry,
) -> Result<toml::Value, SlotDataWriteError> {
    if codec == crate::slots::SOURCE_FILE_CODEC_ID {
        let Some(slot) = data.as_any().downcast_ref::<crate::slots::SourceFileSlot>() else {
            return Err(SlotDataWriteError::ShapeDataMismatch {
                message: "source file codec expected SourceFileSlot data".into(),
            });
        };
        return slot.write_slot_toml();
    }

    Err(SlotDataWriteError::ShapeDataMismatch {
        message: format!("unknown custom slot codec {codec}"),
    })
}

pub(crate) fn snapshot_custom_slot_data<'a>(
    codec: SlotShapeId,
    data: &'a dyn SlotCustomAccess,
) -> Result<SlotDataAccess<'a>, String> {
    if codec == crate::slots::SOURCE_FILE_CODEC_ID {
        let Some(slot) = data.as_any().downcast_ref::<crate::slots::SourceFileSlot>() else {
            return Err("source file codec expected SourceFileSlot data".into());
        };
        return Ok(SlotDataAccess::Custom(slot));
    }

    Err(format!("unknown custom slot codec {codec}"))
}
