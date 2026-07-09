//! Studio node UI components and colocated node UI stories.

mod asset_editor;
#[cfg(feature = "stories")]
pub(crate) mod asset_editor_stories;
mod binding_chip;
mod config_slot_row;
#[cfg(feature = "stories")]
pub(crate) mod config_slot_row_stories;
mod node_children;
mod node_detail_popover;
mod node_pane;
#[cfg(feature = "stories")]
pub(crate) mod node_stories;
#[cfg(feature = "stories")]
pub(crate) mod node_story_fixtures;
#[cfg(feature = "stories")]
pub(crate) mod produced_product_stories;
mod produced_product_view;
mod produced_products;
#[cfg(feature = "stories")]
pub(crate) mod produced_value_stories;
mod produced_value_view;
mod produced_values;
mod slot_affine2d_field;
mod slot_detail_button;
mod slot_dimensions_field;
mod slot_edit_actions;
mod slot_fields;
mod slot_gesture_fields;
mod slot_issue_list;
mod slot_matrix_field;
mod slot_option_presence;
#[cfg(feature = "stories")]
pub(crate) mod slot_option_presence_stories;
mod slot_pane;
mod slot_raw_input_popover;
mod slot_record_editor;
#[cfg(feature = "stories")]
pub(crate) mod slot_record_editor_stories;
mod slot_shape_display;
#[cfg(feature = "stories")]
pub(crate) mod slot_shape_display_stories;
mod slot_unit_display;
#[cfg(feature = "stories")]
pub(crate) mod slot_unit_display_stories;
mod slot_value_editor;
#[cfg(feature = "stories")]
pub(crate) mod slot_value_editor_stories;
mod slot_vector_fields;

pub use asset_editor::AssetEditor;
pub(crate) use binding_chip::{BindingChip, BindingChipDirection};
pub use config_slot_row::ConfigSlotRow;
pub use node_children::NodeChildren;
pub(crate) use node_detail_popover::{NodeDetailPopover, node_status_label_class};
pub use node_pane::{NodeDirtyTint, NodePane, NodeSection};
pub use produced_product_view::ProducedProductView;
pub use produced_products::ProducedProducts;
pub use produced_value_view::ProducedValueView;
pub use produced_values::ProducedValues;
pub use slot_affine2d_field::Affine2dSlotField;
pub(crate) use slot_detail_button::{
    SlotDetailButton, SlotDetailRevert, primary_affordance, slot_row_class,
};
pub use slot_dimensions_field::DimensionsSlotField;
pub use slot_fields::{
    BoolSlotField, DropdownSlotField, FloatSlotField, IntSlotField, SliderSlotField,
    StringSlotField, UIntSlotField, XySlotField,
};
pub use slot_gesture_fields::{
    EnumVariantField, MapAddEntry, MapEntryKeyField, MapEntryRemoveButton,
};
pub use slot_issue_list::SlotIssueList;
pub use slot_matrix_field::MatrixSlotField;
pub use slot_option_presence::{
    OptionPresenceActionButton, OptionPresenceCell, OptionPresenceCheckbox, OptionPresenceStyle,
};
pub use slot_pane::{SlotPane, SlotPaneTreatment};
pub use slot_raw_input_popover::SlotRawInputPopover;
pub use slot_record_editor::SlotRecordEditor;
pub(crate) use slot_shape_display::{
    SlotShapeDisplay, SlotShapeDisplayMode, legacy_shape_from_parts,
};
pub(crate) use slot_unit_display::{SlotUnitDisplay, SlotUnitDisplayMode, SlotUnitSuffix};
pub use slot_value_editor::SlotValueEditor;
pub use slot_vector_fields::VectorSlotField;
