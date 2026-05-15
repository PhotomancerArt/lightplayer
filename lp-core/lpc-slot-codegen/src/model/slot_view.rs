#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaticSlotView {
    pub(crate) type_path: String,
    pub(crate) view_name: String,
    pub(crate) fields: Vec<StaticSlotViewField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaticSlotViewField {
    pub(crate) method_name: String,
    pub(crate) slot_name: String,
    pub(crate) accessor_name: String,
    pub(crate) some_accessor_name: Option<String>,
}
