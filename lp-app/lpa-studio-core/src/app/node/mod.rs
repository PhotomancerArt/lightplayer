//! Data models for Studio node panes.
//!
//! The node UI is intentionally data-driven: controllers project LightPlayer
//! model, slot, binding, and asset state into these `Ui*` structs, and the web
//! crate renders them without needing to understand the runtime model directly.

mod ui_config_slot;
mod ui_consumed_asset;
mod ui_consumed_slot;
mod ui_node_binding;
mod ui_node_child;
mod ui_node_dirty_state;
mod ui_node_header;
mod ui_node_section;
mod ui_node_tab;
mod ui_node_view;
mod ui_produced_product;
mod ui_produced_value;
mod ui_slot_aspect;
mod ui_slot_editor_hint;
mod ui_slot_field_state;
mod ui_slot_record;
mod ui_slot_source_state;
mod ui_slot_value;

pub use ui_config_slot::{UiConfigSlot, UiConfigSlotBody};
pub use ui_consumed_asset::{UiAssetEditorKind, UiConsumedAsset};
pub use ui_consumed_slot::{UiConsumedSlot, UiSlotSource};
pub use ui_node_binding::{UiBindingEndpoint, UiProducedBinding, UiProducedBindings};
pub use ui_node_child::UiNodeChild;
pub use ui_node_dirty_state::UiNodeDirtyState;
pub use ui_node_header::UiNodeHeader;
pub use ui_node_section::UiNodeSection;
pub use ui_node_tab::{UiNodeTab, UiNodeTabBody};
pub use ui_node_view::UiNodeView;
pub use ui_produced_product::{UiProducedProduct, UiProductKind};
pub use ui_produced_value::UiProducedValue;
pub use ui_slot_aspect::{UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow};
pub use ui_slot_editor_hint::{UiSlotEditorHint, UiSlotOption};
pub use ui_slot_field_state::UiSlotFieldState;
pub use ui_slot_record::UiSlotRecord;
pub use ui_slot_source_state::UiSlotSourceState;
pub use ui_slot_value::{UiSlotValue, UiSlotValueKind};

#[cfg(test)]
mod tests {
    use crate::{
        UiConsumedSlot, UiNodeChild, UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeTabBody,
        UiNodeView, UiProducedProduct, UiProducedValue, UiStatus,
    };

    #[test]
    fn node_view_reports_sections_and_children() {
        let view = UiNodeView::new(
            UiNodeHeader::new("Playlist", "Playlist", "/show/playlist")
                .with_status(UiStatus::good("Running")),
            vec![UiNodeTab::main(vec![
                UiNodeSection::ProducedProducts(vec![UiProducedProduct::visual("output")]),
                UiNodeSection::ProducedValues(vec![UiProducedValue::new("Entry time", "3.333")]),
                UiNodeSection::ConsumedValues(vec![UiConsumedSlot::direct(
                    "Default fade",
                    "0.35 s",
                )]),
            ])],
        )
        .with_children(vec![UiNodeChild::new("blast", "Shader", "./blast.toml")]);

        assert!(view.has_sections());
        assert!(view.has_children());
        assert_eq!(view.tabs[0].label, "main");
    }

    #[test]
    fn node_view_empty_tab_body_has_no_sections() {
        let view = UiNodeView::new(
            UiNodeHeader::new("Clock", "Clock", "/show/clock"),
            vec![UiNodeTab::new("main", UiNodeTabBody::Sections(Vec::new()))],
        );

        assert!(!view.has_sections());
        assert!(!view.has_children());
    }
}
