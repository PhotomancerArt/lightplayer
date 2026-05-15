#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaticSlotRecord {
    pub(crate) type_path: String,
    pub(crate) type_name: String,
    pub(crate) fields: Vec<StaticSlotRecordField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaticSlotRecordField {
    pub(crate) rust_name: String,
    pub(crate) slot_name: String,
    pub(crate) type_name: String,
    pub(crate) is_enum: bool,
}
