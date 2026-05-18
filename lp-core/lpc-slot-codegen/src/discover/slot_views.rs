use crate::error::SlotShapeCodegenError;
use crate::model::{StaticSlotView, StaticSlotViewField};
use std::path::Path;

pub(crate) fn discover_static_slot_views(
    src_dir: &Path,
) -> Result<Vec<StaticSlotView>, SlotShapeCodegenError> {
    Ok(super::slot_records::discover_static_slot_records(src_dir)?
        .into_iter()
        .map(|record| StaticSlotView {
            view_name: format!("{}View", record.type_name),
            type_path: record.type_path,
            fields: record
                .fields
                .into_iter()
                .map(|field| StaticSlotViewField {
                    accessor_name: format!("{}_accessor", field.rust_name),
                    some_accessor_name: (field.type_name == "OptionSlot")
                        .then(|| format!("{}_some_accessor", field.rust_name)),
                    method_name: field.rust_name,
                    slot_name: field.slot_name,
                })
                .collect(),
        })
        .collect())
}
