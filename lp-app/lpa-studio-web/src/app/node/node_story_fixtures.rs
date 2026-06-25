//! Shared fixtures for Studio node component stories.

use lpa_studio_core::{
    UiAssetEditorKind, UiBindingEndpoint, UiConfigSlot, UiConsumedAsset, UiConsumedSlot,
    UiNodeChild, UiNodeDirtyState, UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeTabBody,
    UiNodeView, UiProducedBinding, UiProducedBindings, UiProducedProduct, UiProducedValue,
    UiSlotEditorHint, UiSlotFieldState, UiSlotRecord, UiSlotSourceState, UiSlotValue, UiStatus,
};

pub(crate) fn playlist_node_view() -> UiNodeView {
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

pub(crate) fn error_node_view() -> UiNodeView {
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
    view
}

pub(crate) fn playlist_header() -> UiNodeHeader {
    UiNodeHeader::new("Playlist", "Playlist", "/fyeah_sign.show/playlist.playlist")
        .with_source("playlist.toml")
        .with_status(UiStatus::good("Running"))
        .with_summary("entry 1")
        .with_detail("Node has run recently with no reported errors.")
}

pub(crate) fn produced_products_fixture() -> Vec<UiProducedProduct> {
    vec![
        UiProducedProduct::visual("output")
            .with_detail("128 x 72")
            .with_binding_routes(
                Some("bus#visual.out"),
                &[],
                &["Fixture.visual"],
                Some("rev 104"),
            ),
    ]
}

pub(crate) fn produced_product_variants_fixture() -> Vec<UiProducedProduct> {
    vec![
        UiProducedProduct::empty("output").with_detail("not resolved"),
        UiProducedProduct::visual("output")
            .with_detail("128 x 72")
            .with_binding_routes(
                Some("bus#visual.out"),
                &[],
                &["Fixture.visual"],
                Some("rev 104"),
            ),
        UiProducedProduct::control("dmx")
            .with_detail("24 channels")
            .with_binding_routes(None, &["fixture#strip-a"], &[], Some("rev 104")),
    ]
}

pub(crate) fn produced_values_fixture() -> Vec<UiProducedValue> {
    vec![
        UiProducedValue::new("Entry time", "3.333")
            .with_detail("seconds")
            .with_binding_routes(None, &[], &["idle.Time", "blast.Time"], Some("rev 104")),
    ]
}

pub(crate) fn produced_value_variants_fixture() -> Vec<UiProducedValue> {
    vec![
        UiProducedValue::new("Entry time", "320").with_detail("s"),
        UiProducedValue::new("FPS", "447").with_detail("Hz"),
        UiProducedValue::new("Peers", "2").with_binding_routes(
            Some("bus#radio.peer_count"),
            &[],
            &["debug.Peers"],
            None,
        ),
    ]
}

pub(crate) fn consumed_slots_fixture() -> Vec<UiConsumedSlot> {
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

pub(crate) fn consumed_assets_fixture() -> Vec<UiConsumedAsset> {
    vec![
        UiConsumedAsset::new("Playlist", "./playlist.toml", UiAssetEditorKind::Text)
            .with_detail("artifact, rev 22")
            .with_summary("[[entries]]\nname = \"idle\"\nsource = \"./idle.toml\""),
    ]
}

pub(crate) fn children_fixture() -> Vec<UiNodeChild> {
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

pub(crate) fn config_record_fixture() -> UiSlotRecord {
    UiSlotRecord::new(vec![
        UiConfigSlot::value("enabled", "Enabled", UiSlotValue::bool(true)),
        UiConfigSlot::value(
            "shader",
            "Shader",
            UiSlotValue::string("./idle.glsl").with_editor(UiSlotEditorHint::Text),
        )
        .with_detail("asset ref"),
        UiConfigSlot::value(
            "fade_after",
            "Fade after",
            UiSlotValue::f32(0.35)
                .with_detail("s")
                .with_editor(UiSlotEditorHint::number()),
        )
        .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
        UiConfigSlot::value("time", "Time", UiSlotValue::f32(3.333).with_detail("s")).with_source(
            UiSlotSourceState::Bound(
                UiBindingEndpoint::new("bus#time.seconds").with_detail("global clock"),
            ),
        ),
        UiConfigSlot::record(
            "transform",
            "Transform",
            vec![
                UiConfigSlot::value(
                    "origin",
                    "Origin",
                    UiSlotValue::vec2([0.42, 0.58]).with_editor(UiSlotEditorHint::Xy),
                ),
                UiConfigSlot::value("scale", "Scale", UiSlotValue::vec2([1.0, 1.0])),
                UiConfigSlot::value("tint", "Tint", UiSlotValue::vec3([1.0, 0.42, 0.2])),
            ],
        )
        .with_detail("record"),
    ])
}

pub(crate) fn config_row_states_fixture() -> Vec<UiConfigSlot> {
    vec![
        UiConfigSlot::value("direct", "Direct value", UiSlotValue::f32(0.72)),
        UiConfigSlot::value("bound", "Bound value", UiSlotValue::f32(3.333)).with_source(
            UiSlotSourceState::Bound(UiBindingEndpoint::new("bus#time.seconds")),
        ),
        UiConfigSlot::value("dirty", "Edited value", UiSlotValue::string("idle.glsl"))
            .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
        UiConfigSlot::value("invalid", "Invalid value", UiSlotValue::f32(-1.0))
            .with_state(UiSlotFieldState::editable().with_invalid("value must be non-negative")),
        UiConfigSlot::empty("optional_trigger", "Optional trigger")
            .with_source(UiSlotSourceState::Unset),
        UiConfigSlot::record(
            "record",
            "Nested record",
            vec![UiConfigSlot::value(
                "child",
                "Child value",
                UiSlotValue::bool(true),
            )],
        ),
    ]
}

pub(crate) fn slot_value_variants_fixture() -> Vec<UiSlotValue> {
    vec![
        UiSlotValue::string("./idle.glsl").with_editor(UiSlotEditorHint::Text),
        UiSlotValue::i32(-4),
        UiSlotValue::u32(128),
        UiSlotValue::f32(0.35).with_detail("s"),
        UiSlotValue::f32(0.72).with_editor(UiSlotEditorHint::slider(0.0, 1.0)),
        UiSlotValue::bool(true),
        UiSlotValue::vec2([0.42, 0.58]),
        UiSlotValue::vec3([1.0, 0.42, 0.2]),
        UiSlotValue::string("blast").with_editor(UiSlotEditorHint::dropdown([
            ("idle", "Idle"),
            ("blast", "Blast"),
            ("strobe", "Strobe"),
        ])),
        UiSlotValue::vec2([0.42, 0.58]).with_editor(UiSlotEditorHint::Xy),
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
