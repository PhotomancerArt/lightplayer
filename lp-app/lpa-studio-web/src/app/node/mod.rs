//! Studio node UI components and colocated node UI stories.

mod config_slot_row;
#[cfg(feature = "stories")]
pub(crate) mod config_slot_row_stories;
mod node_children;
mod node_header;
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
mod slot_detail_button;
mod slot_fields;
mod slot_issue_list;
mod slot_record_editor;
#[cfg(feature = "stories")]
pub(crate) mod slot_record_editor_stories;
mod slot_value_editor;
#[cfg(feature = "stories")]
pub(crate) mod slot_value_editor_stories;

pub use config_slot_row::ConfigSlotRow;
pub use node_children::NodeChildren;
pub use node_header::NodeHeader;
pub use node_pane::{DirtyMark, NodePane, NodeSection, ProducedBindingMark};
pub use produced_product_view::ProducedProductView;
pub use produced_products::ProducedProducts;
pub use produced_value_view::ProducedValueView;
pub use produced_values::ProducedValues;
pub(crate) use slot_detail_button::{SlotDetailButton, primary_affordance, slot_row_class};
pub use slot_fields::{
    BoolSlotField, DropdownSlotField, FloatSlotField, IntSlotField, StringSlotField, UIntSlotField,
    Vec2SlotField, Vec3SlotField, XySlotField,
};
pub use slot_issue_list::SlotIssueList;
pub use slot_record_editor::SlotRecordEditor;
pub use slot_value_editor::SlotValueEditor;
