//! Shared fixtures for Studio node component stories.

use lpa_studio_core::{
    UiAssetEditorKind, UiBindingEndpoint, UiConfigSlot, UiNodeChild, UiNodeDirtyState,
    UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeTabBody, UiNodeView, UiProducedBinding,
    UiProducedBindings, UiProducedProduct, UiProducedValue, UiProductPreview,
    UiProductTrackingState, UiSlotAsset, UiSlotEditorHint, UiSlotFieldState, UiSlotOptionality,
    UiSlotRecord, UiSlotSourceState, UiSlotUnit, UiSlotValue, UiStatus,
};

const IDLE_GLSL: &str = r#"vec3 palette(float t) {
    return 0.5 + 0.5 * cos(6.28318 * (vec3(0.1, 0.3, 0.6) + t));
}

void mainImage(out vec4 color, in vec2 uv) {
    float glow = smoothstep(0.9, 0.2, length(uv - 0.5));
    color = vec4(palette(glow), 1.0);
}"#;

const BLAST_GLSL: &str = r#"void mainImage(out vec4 color, in vec2 uv) {
    vec3 base = vec3(1.0, 0.18, 0.05);
    float ring = sin(length(uv - 0.5) * 64.0);
    color = vec4(base * ring, 1.0);
}"#;

pub(crate) fn playlist_node_view() -> UiNodeView {
    UiNodeView::new(
        playlist_header(),
        vec![
            UiNodeTab::main(vec![
                UiNodeSection::ProducedProducts(produced_products_fixture()),
                UiNodeSection::ProducedValues(produced_values_fixture()),
                UiNodeSection::ConfigSlots(config_slots_fixture()),
                UiNodeSection::AssetSlots(asset_slots_fixture()),
            ]),
            UiNodeTab::new(
                "raw",
                UiNodeTabBody::Text {
                    title: "Slot extraction notes".to_string(),
                    body: "def.input.time -> config slot\nstate.output -> produced product\nentries.* -> extracted children".to_string(),
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
            UiNodeSection::ConfigSlots(vec![
                UiConfigSlot::value("shader", "Shader", UiSlotValue::string("blast.glsl"))
                    .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Error)),
            ]),
            UiNodeSection::AssetSlots(vec![
                UiConfigSlot::asset(
                    "shader_source",
                    "Shader",
                    UiSlotAsset::new("./blast.glsl", UiAssetEditorKind::Glsl)
                        .with_content("vec3 color = sample(uv2);"),
                )
                .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Error)),
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
    vec![visual_preview_product("output").with_binding_routes(
        Some("bus#visual.out"),
        &[],
        &["Fixture.visual"],
        Some("rev 104"),
    )]
}

pub(crate) fn produced_product_variants_fixture() -> Vec<UiProducedProduct> {
    vec![
        UiProducedProduct::empty("output").with_detail("not resolved"),
        UiProducedProduct::visual("output").with_detail("64 x 36 preview"),
        UiProducedProduct::visual("output")
            .with_detail("64 x 36 preview")
            .with_preview(UiProductPreview::Pending)
            .with_tracking(UiProductTrackingState::Tracking),
        visual_preview_product("output").with_binding_routes(
            Some("bus#visual.out"),
            &[],
            &["Fixture.visual"],
            Some("rev 104"),
        ),
        visual_preview_product("output")
            .with_tracking(UiProductTrackingState::Paused)
            .with_detail("cached preview"),
        visual_error_product("output"),
        UiProducedProduct::control("dmx")
            .with_detail("24 channels")
            .with_binding_routes(None, &["fixture#strip-a"], &[], Some("rev 104")),
    ]
}

pub(crate) fn visual_preview_product(name: &str) -> UiProducedProduct {
    UiProducedProduct::visual(name)
        .with_detail("128 x 72")
        .with_tracking(UiProductTrackingState::Tracking)
        .with_preview(UiProductPreview::VisualSrgb8 {
            width: 16,
            height: 9,
            revision: 104,
            bytes: visual_preview_bytes(16, 9),
        })
}

pub(crate) fn visual_error_product(name: &str) -> UiProducedProduct {
    UiProducedProduct::visual(name)
        .with_detail("128 x 72")
        .with_tracking(UiProductTrackingState::Tracking)
        .with_preview(UiProductPreview::Error {
            message: "render probe failed".to_string(),
        })
}

fn visual_preview_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let u = x as f32 / width.saturating_sub(1).max(1) as f32;
            let v = y as f32 / height.saturating_sub(1).max(1) as f32;
            bytes.push((u * 255.0) as u8);
            bytes.push(((1.0 - v) * 180.0 + 40.0) as u8);
            bytes.push(((u * v) * 220.0 + 24.0) as u8);
        }
    }
    bytes
}

pub(crate) fn produced_values_fixture() -> Vec<UiProducedValue> {
    vec![
        UiProducedValue::new("Entry time", "3.333")
            .with_unit(UiSlotUnit::seconds())
            .with_binding_routes(None, &[], &["idle.Time", "blast.Time"], Some("rev 104")),
    ]
}

pub(crate) fn produced_value_variants_fixture() -> Vec<UiProducedValue> {
    vec![
        UiProducedValue::new("Entry time", "3.33").with_unit(UiSlotUnit::seconds()),
        UiProducedValue::new("FPS", "447").with_unit(UiSlotUnit::hertz()),
        UiProducedValue::new("Peers", "2").with_binding_routes(
            Some("bus#radio.peer_count"),
            &[],
            &["debug.Peers"],
            None,
        ),
    ]
}

pub(crate) fn config_slots_fixture() -> Vec<UiConfigSlot> {
    vec![
        UiConfigSlot::value(
            "time",
            "Time",
            UiSlotValue::f32(3.333).with_unit(UiSlotUnit::seconds()),
        )
        .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new(
            "bus#time.seconds",
        ))),
        UiConfigSlot::value("idle_entry", "Idle entry", UiSlotValue::u32(1)),
        UiConfigSlot::value(
            "default_fade",
            "Default fade",
            UiSlotValue::f32(0.35).with_unit(UiSlotUnit::seconds()),
        )
        .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
        UiConfigSlot::record(
            "entries",
            "Entries",
            vec![
                UiConfigSlot::value("blast_trigger", "Blast trigger", UiSlotValue::bool(false))
                    .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new(
                        "bus#trigger",
                    )))
                    .with_detail("optional trigger"),
            ],
        )
        .with_detail("2 child invocations"),
    ]
}

pub(crate) fn asset_slots_fixture() -> Vec<UiConfigSlot> {
    vec![
        UiConfigSlot::asset(
            "idle_shader",
            "Idle shader",
            UiSlotAsset::new("./idle.glsl", UiAssetEditorKind::Glsl)
                .with_detail("artifact, rev 22")
                .with_content(IDLE_GLSL),
        )
        .with_detail("artifact, rev 22"),
        UiConfigSlot::asset(
            "blast_shader",
            "Blast shader",
            UiSlotAsset::new("./blast.glsl", UiAssetEditorKind::Glsl)
                .with_detail("artifact, rev 19")
                .with_content(BLAST_GLSL),
        )
        .with_detail("artifact, rev 19"),
    ]
}

pub(crate) fn children_fixture() -> Vec<UiNodeChild> {
    vec![
        UiNodeChild::new("idle", "Shader", "./idle.toml")
            .active("active, fade_after 0.12 s")
            .with_sections(vec![
                UiNodeSection::ProducedProducts(vec![
                    UiProducedProduct::visual("output").with_detail("64 x 36 preview"),
                ]),
                UiNodeSection::ConfigSlots(vec![
                    UiConfigSlot::value(
                        "time",
                        "Time",
                        UiSlotValue::f32(3.333).with_unit(UiSlotUnit::seconds()),
                    )
                    .with_source(UiSlotSourceState::Bound(
                        UiBindingEndpoint::new("../playlist#entry_time"),
                    )),
                    UiConfigSlot::value("shader", "Shader", UiSlotValue::string("idle.glsl")),
                ]),
            ]),
        UiNodeChild::new("blast", "Shader", "./blast.toml").with_sections(vec![
            UiNodeSection::ConfigSlots(vec![
                UiConfigSlot::value(
                    "time",
                    "Time",
                    UiSlotValue::f32(3.333).with_unit(UiSlotUnit::seconds()),
                )
                .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new(
                    "../playlist#entry_time",
                ))),
                UiConfigSlot::value("trigger", "Trigger", UiSlotValue::bool(false)).with_source(
                    UiSlotSourceState::Bound(UiBindingEndpoint::new("bus#trigger")),
                ),
                UiConfigSlot::value("shader", "Shader", UiSlotValue::string("blast.glsl")),
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
                .with_unit(UiSlotUnit::seconds())
                .with_editor(UiSlotEditorHint::number()),
        )
        .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
        UiConfigSlot::value(
            "time",
            "Time",
            UiSlotValue::f32(3.333).with_unit(UiSlotUnit::seconds()),
        )
        .with_source(UiSlotSourceState::Bound(
            UiBindingEndpoint::new("bus#time.seconds").with_detail("global clock"),
        )),
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
        UiConfigSlot::value(
            "bound",
            "Bound value",
            UiSlotValue::f32(3.333).with_unit(UiSlotUnit::seconds()),
        )
        .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new(
            "bus#time.seconds",
        ))),
        UiConfigSlot::value("dirty", "Edited value", UiSlotValue::string("idle.glsl"))
            .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
        UiConfigSlot::value("invalid", "Invalid value", UiSlotValue::f32(-1.0))
            .with_state(UiSlotFieldState::editable().with_invalid("value must be non-negative")),
        UiConfigSlot::value(
            "write_failed",
            "Write failed",
            UiSlotValue::string("blast.glsl"),
        )
        .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Error)),
        UiConfigSlot::empty("optional_trigger", "Optional trigger")
            .with_optionality(UiSlotOptionality::excluded(true))
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
        UiSlotValue::f32(0.35).with_unit(UiSlotUnit::seconds()),
        UiSlotValue::f32(0.72).with_editor(UiSlotEditorHint::slider(0.0, 1.0)),
        UiSlotValue::bool(true),
        UiSlotValue::vec2([0.42, 0.58]),
        UiSlotValue::vec3([1.0, 0.42, 0.2]),
        UiSlotValue::vec4([1.0, 0.42, 0.2, 1.0]),
        UiSlotValue::ivec3([-1, 0, 1]),
        UiSlotValue::uvec4([1, 2, 3, 4]),
        UiSlotValue::bvec3([true, false, true]),
        UiSlotValue::mat2x2([[1.0, 0.0], [0.0, 1.0]]),
        UiSlotValue::array(vec![UiSlotValue::f32(0.25), UiSlotValue::f32(0.75)]),
        UiSlotValue::struct_value(
            Some("Envelope".to_string()),
            vec![
                ("attack".to_string(), UiSlotValue::f32(0.1)),
                ("release".to_string(), UiSlotValue::f32(0.8)),
            ],
        ),
        UiSlotValue::enum_value(1, Some(UiSlotValue::string("Loop"))),
        UiSlotValue::unset(),
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
