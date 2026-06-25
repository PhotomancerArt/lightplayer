use dioxus::prelude::*;
use lpa_studio_core::{
    UiAssetEditorKind, UiBindingEndpoint, UiConsumedAsset, UiConsumedSlot, UiNodeChild,
    UiNodeDirtyState, UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeTabBody, UiNodeView,
    UiProducedBinding, UiProducedBindings, UiProducedProduct, UiProducedValue, UiStatus,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::{
    ConsumedAssets, ConsumedSlots, NodeChildren, NodeHeader, NodePane, ProducedProducts,
    ProducedValues,
};

#[story(description = "Header anatomy with identity, status, source, and runtime summary.")]
pub(crate) fn header() -> Element {
    rsx! {
        NodeHeader { header: playlist_header() }
    }
}

#[story(description = "Product outputs that get the primary node visual treatment.")]
pub(crate) fn produced_products() -> Element {
    rsx! {
        ProducedProducts { products: produced_products_fixture() }
    }
}

#[story(description = "Non-product outputs rendered as compact value boxes.")]
pub(crate) fn produced_values() -> Element {
    rsx! {
        ProducedValues { values: produced_values_fixture() }
    }
}

#[story(description = "Recursive consumed slots with direct, bound, child, and dirty states.")]
pub(crate) fn consumed_values() -> Element {
    rsx! {
        ConsumedSlots { slots: consumed_slots_fixture() }
    }
}

#[story(description = "Assets pulled out of consumed slots for editor-level treatment.")]
pub(crate) fn consumed_assets() -> Element {
    rsx! {
        ConsumedAssets { assets: consumed_assets_fixture() }
    }
}

#[story(description = "Children extracted from the slot tree and rendered outside the node pane.")]
pub(crate) fn children() -> Element {
    rsx! {
        NodeChildren { items: children_fixture() }
    }
}

#[story(description = "A composed node pane showing the current node anatomy direction.")]
pub(crate) fn node_pane() -> Element {
    rsx! {
        NodePane { view: playlist_node_view() }
    }
}

#[story(description = "Node pane with an error status and projection issues.")]
pub(crate) fn error_node() -> Element {
    let mut view = UiNodeView::new(
        UiNodeHeader::new("blast", "Shader", "/show/playlist/blast")
            .with_source("blast.glsl")
            .with_status(UiStatus::error("Error"))
            .with_summary("compile failed")
            .with_detail("unknown identifier `uv2` at line 18"),
        vec![UiNodeTab::main(vec![
            UiNodeSection::ConsumedValues(vec![
                UiConsumedSlot::direct("Shader", "blast.glsl").with_dirty(UiNodeDirtyState::Error),
            ]),
            UiNodeSection::ConsumedAssets(vec![
                UiConsumedAsset::new("Shader", "./blast.glsl", UiAssetEditorKind::Glsl)
                    .with_summary("vec3 color = sample(uv2);"),
            ]),
        ])],
    )
    .with_node_id("shader-blast");
    view.issues = vec!["Shader compile failed".to_string()];

    rsx! {
        NodePane { view }
    }
}

fn playlist_node_view() -> UiNodeView {
    UiNodeView::new(
        playlist_header(),
        vec![
            UiNodeTab::main(vec![
                UiNodeSection::ProducedProducts(produced_products_fixture()),
                UiNodeSection::ProducedValues(produced_values_fixture()),
                UiNodeSection::ConsumedValues(consumed_slots_fixture()),
                UiNodeSection::ConsumedAssets(consumed_assets_fixture()),
            ]),
            UiNodeTab::new(
                "raw",
                UiNodeTabBody::Text {
                    title: "Slot extraction notes".to_string(),
                    body: "def.input.time -> consumed value\nstate.output -> produced product\nentries.* -> extracted children".to_string(),
                },
            ),
        ],
    )
    .with_node_id("playlist")
    .with_children(children_fixture())
}

fn playlist_header() -> UiNodeHeader {
    UiNodeHeader::new("Playlist", "Playlist", "/fyeah_sign.show/playlist.playlist")
        .with_source("playlist.toml")
        .with_status(UiStatus::good("Running"))
        .with_summary("entry 1")
        .with_detail("Node has run recently with no reported errors.")
}

fn produced_products_fixture() -> Vec<UiProducedProduct> {
    vec![
        UiProducedProduct::visual("output")
            .with_detail("128 x 72")
            .with_binding_routes(
                Some("bus#visual.out"),
                &[],
                &["Fixture.visual"],
                Some("rev 104"),
            ),
        UiProducedProduct::control("fixture-control")
            .with_detail("657 samples")
            .with_binding_routes(
                Some("bus#control.fixture"),
                &["Output.main"],
                &[],
                Some("rev 44"),
            ),
    ]
}

fn produced_values_fixture() -> Vec<UiProducedValue> {
    vec![
        UiProducedValue::new("Entry time", "3.333")
            .with_detail("seconds")
            .with_binding_routes(None, &[], &["idle.Time", "blast.Time"], Some("rev 104")),
        UiProducedValue::new("Active", "idle").with_detail("entry 1"),
        UiProducedValue::new("Progress", "0.333")
            .with_detail("normalized")
            .with_dirty(UiNodeDirtyState::Saving),
    ]
}

fn consumed_slots_fixture() -> Vec<UiConsumedSlot> {
    vec![
        UiConsumedSlot::bound("Time", UiBindingEndpoint::new("bus#time.seconds")),
        UiConsumedSlot::direct("Idle entry", "1"),
        UiConsumedSlot::direct("Default fade", "0.35 s").with_dirty(UiNodeDirtyState::Dirty),
        UiConsumedSlot::group(
            "Entries",
            vec![
                UiConsumedSlot::child("idle", "./idle.shader"),
                UiConsumedSlot::child("blast", "./blast.shader"),
                UiConsumedSlot::bound("blast.trigger", UiBindingEndpoint::new("bus#trigger"))
                    .with_detail("optional trigger"),
            ],
        )
        .with_detail("2 child invocations"),
    ]
}

fn consumed_assets_fixture() -> Vec<UiConsumedAsset> {
    vec![
        UiConsumedAsset::new("Playlist", "./playlist.toml", UiAssetEditorKind::Text)
            .with_detail("artifact, rev 22")
            .with_summary("[[entries]]\nname = \"idle\"\nsource = \"./idle.toml\""),
        UiConsumedAsset::new("Shader", "./blast.glsl", UiAssetEditorKind::Glsl)
            .with_detail("artifact, rev 41")
            .with_summary(
                "void mainImage(out vec4 color, in vec2 uv) {\n    color = vec4(uv, 1.0, 1.0);\n}",
            ),
        UiConsumedAsset::new("Fixture map", "./fyeah-mapping.svg", UiAssetEditorKind::Svg)
            .with_detail("inline editor planned")
            .with_summary("<svg viewBox=\"0 0 128 72\">...</svg>")
            .with_dirty(UiNodeDirtyState::Dirty),
    ]
}

fn children_fixture() -> Vec<UiNodeChild> {
    vec![
        UiNodeChild::new("idle", "Shader", "./idle.toml")
            .active("active, fade_after 0.12 s")
            .with_sections(vec![
                UiNodeSection::ProducedProducts(vec![
                    UiProducedProduct::visual("output").with_detail("128 x 72"),
                ]),
                UiNodeSection::ConsumedValues(vec![
                    UiConsumedSlot::bound("Time", UiBindingEndpoint::new("../playlist#entry_time")),
                    UiConsumedSlot::direct("Shader", "idle.glsl"),
                ]),
            ]),
        UiNodeChild::new("blast", "Shader", "./blast.toml").with_sections(vec![
            UiNodeSection::ConsumedValues(vec![
                UiConsumedSlot::bound("Time", UiBindingEndpoint::new("../playlist#entry_time")),
                UiConsumedSlot::bound("Trigger", UiBindingEndpoint::new("bus#trigger")),
                UiConsumedSlot::direct("Shader", "blast.glsl"),
            ]),
        ]),
    ]
}

trait NodeStoryProductExt {
    fn with_binding_routes(
        self,
        bus_target: Option<&str>,
        target_bindings: &[&str],
        consumers: &[&str],
        revision: Option<&str>,
    ) -> Self;
}

impl NodeStoryProductExt for UiProducedProduct {
    fn with_binding_routes(
        mut self,
        bus_target: Option<&str>,
        target_bindings: &[&str],
        consumers: &[&str],
        revision: Option<&str>,
    ) -> Self {
        self.binding = produced_binding(bus_target, target_bindings, consumers, revision);
        self
    }
}

trait NodeStoryValueExt {
    fn with_binding_routes(
        self,
        bus_target: Option<&str>,
        target_bindings: &[&str],
        consumers: &[&str],
        revision: Option<&str>,
    ) -> Self;

    fn with_dirty(self, dirty: UiNodeDirtyState) -> Self;
}

impl NodeStoryValueExt for UiProducedValue {
    fn with_binding_routes(
        mut self,
        bus_target: Option<&str>,
        target_bindings: &[&str],
        consumers: &[&str],
        revision: Option<&str>,
    ) -> Self {
        self.binding = produced_binding(bus_target, target_bindings, consumers, revision);
        self
    }

    fn with_dirty(mut self, dirty: UiNodeDirtyState) -> Self {
        self.dirty = dirty;
        self
    }
}

trait NodeStorySlotExt {
    fn child(label: impl Into<String>, child: impl Into<String>) -> UiConsumedSlot;
}

impl NodeStorySlotExt for UiConsumedSlot {
    fn child(label: impl Into<String>, child: impl Into<String>) -> UiConsumedSlot {
        UiConsumedSlot {
            label: label.into(),
            value: None,
            detail: None,
            source: lpa_studio_core::UiSlotSource::Child(child.into()),
            dirty: UiNodeDirtyState::Clean,
            children: Vec::new(),
            issues: Vec::new(),
        }
    }
}

trait NodeStoryAssetExt {
    fn with_dirty(self, dirty: UiNodeDirtyState) -> Self;
}

impl NodeStoryAssetExt for UiConsumedAsset {
    fn with_dirty(mut self, dirty: UiNodeDirtyState) -> Self {
        self.dirty = dirty;
        self
    }
}

fn produced_binding(
    bus_target: Option<&str>,
    target_bindings: &[&str],
    consumers: &[&str],
    revision: Option<&str>,
) -> UiProducedBinding {
    UiProducedBinding {
        bindings: UiProducedBindings {
            bus_target: bus_target.map(UiBindingEndpoint::new),
            target_bindings: target_bindings
                .iter()
                .map(|target| UiBindingEndpoint::new(*target))
                .collect(),
            consumers: consumers
                .iter()
                .map(|consumer| UiBindingEndpoint::new(*consumer))
                .collect(),
        },
        revision: revision.map(str::to_string),
    }
}
