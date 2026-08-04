//! Shared fixtures for node-card face and panel-control stories.
//!
//! Everything here is hand-built mock DTO data mirroring what the real
//! face derivation (`node_face_builder`) produces, so the stories stay a
//! faithful design record. Panel-control aspects are derived through a real
//! `UiConfigSlot` (`visible_aspects()`), so the label-trigger popover
//! content is byte-identical to what the backing slot row would show.

use lpa_studio_core::{
    ArtifactLocation, ControllerId, ProjectEditorOp, ProjectNodeAddress, ProjectSlotAddress,
    ProjectSlotRoot, SlotPath, UiAction, UiAgentAvailability, UiAgentStatus, UiAgentToolRow,
    UiAgentTurn, UiAgentUsage, UiAgentView, UiAssetContent, UiAssetEditor, UiAssetEditorKind,
    UiBindingEndpoint, UiClockFace, UiConfigSlot, UiFixtureFace, UiNodeChild, UiNodeDirtyState,
    UiNodeFace, UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeView, UiOutputBoardFacts,
    UiOutputChannelRow, UiOutputFace, UiOutputPin, UiPanelControl, UiPanelEmit, UiPanelWidget,
    UiPhasorRow, UiPlaylistEntry, UiPlaylistFace, UiProducedProduct, UiProducedValue,
    UiProductPreview, UiProductPreviewFrame, UiProductTrackingState, UiShaderFace, UiShaderUniform,
    UiSlotFieldState, UiSlotSourceState, UiSlotUnit, UiSlotValue, UiStatus, UiTimebaseState,
};

use crate::app::node::node_story_fixtures::{
    control_preview_product, map2d_control_preview_product,
};

/// Story-only slot address so panel fields render wired (dispatch goes to
/// the story's no-op handler).
pub(crate) fn story_slot_address(path: &str) -> ProjectSlotAddress {
    ProjectSlotAddress::new(
        ProjectNodeAddress::parse("/fyeah_sign.show/aurora.shader")
            .expect("valid story node address"),
        ProjectSlotRoot::def(),
        SlotPath::parse(path).expect("valid story slot path"),
    )
}

/// One knob control. Aspects (the label-trigger popover) ride the
/// equivalent config slot, so panel and row present identical detail.
pub(crate) fn knob_control(
    label: &str,
    value: f32,
    min: f32,
    max: f32,
    state: UiSlotFieldState,
    source: UiSlotSourceState,
) -> UiPanelControl {
    knob_control_stepped(label, value, min, max, None, state, source)
}

/// A knob quantized to `step` — what an integer uniform (`i32`/`u32`, or an
/// authored `step`) derives to. `None` is the continuous knob.
///
/// The readout is snapped here exactly as `node_face_builder` snaps it, so
/// a stepped control's value and widget agree in the story the same way they
/// agree in the app.
pub(crate) fn knob_control_stepped(
    label: &str,
    value: f32,
    min: f32,
    max: f32,
    step: Option<f32>,
    state: UiSlotFieldState,
    source: UiSlotSourceState,
) -> UiPanelControl {
    let value = crate::app::node::panel::knob_snap(value, min, step);
    let slot_value = UiSlotValue::f32(value);
    let aspect_slot = UiConfigSlot::value(label, label, slot_value.clone())
        .with_state(state.clone())
        .with_source(source);
    UiPanelControl {
        emit: UiPanelEmit::Value,
        label: label.to_string(),
        address: Some(story_slot_address(&format!("controls.{label}"))),
        widget: UiPanelWidget::Knob { min, max, step },
        value: slot_value,
        live_value: None,
        panel_target: None,
        unit: None,
        state,
        aspects: aspect_slot.visible_aspects(),
    }
}

/// The fixture face's dominant brightness fader (0–255, like
/// `FixtureDef.brightness`).
pub(crate) fn fader_control(
    value: f32,
    state: UiSlotFieldState,
    source: UiSlotSourceState,
) -> UiPanelControl {
    let slot_value = UiSlotValue::f32(value);
    let aspect_slot = UiConfigSlot::value("brightness", "brightness", slot_value.clone())
        .with_state(state.clone())
        .with_source(source);
    UiPanelControl {
        emit: UiPanelEmit::Value,
        label: "brightness".to_string(),
        address: Some(story_slot_address("brightness.some")),
        widget: UiPanelWidget::Fader {
            min: 0.0,
            max: 255.0,
            step: Some(1.0),
        },
        value: slot_value,
        live_value: None,
        panel_target: None,
        unit: None,
        state,
        aspects: aspect_slot.visible_aspects(),
    }
}

/// One pill toggle control.
pub(crate) fn toggle_control(
    label: &str,
    value: bool,
    state: UiSlotFieldState,
    source: UiSlotSourceState,
) -> UiPanelControl {
    let slot_value = UiSlotValue::bool(value);
    let aspect_slot = UiConfigSlot::value(label, label, slot_value.clone())
        .with_state(state.clone())
        .with_source(source);
    UiPanelControl {
        emit: UiPanelEmit::Value,
        label: label.to_string(),
        address: Some(story_slot_address(&format!("controls.{label}"))),
        widget: UiPanelWidget::Toggle,
        value: slot_value,
        live_value: None,
        panel_target: None,
        unit: None,
        state,
        aspects: aspect_slot.visible_aspects(),
    }
}

/// Bus binding used by every "bound" control state.
pub(crate) fn bound_source() -> UiSlotSourceState {
    UiSlotSourceState::Bound(UiBindingEndpoint::new("bus:master-tempo"))
}

/// The shader face's knob row: bound speed (violet, with the live bus
/// reading leading its readout), plain hue, live-edited scale, and the
/// mirror toggle.
pub(crate) fn shader_controls(speed_bound: bool) -> Vec<UiPanelControl> {
    let speed_source = if speed_bound {
        bound_source()
    } else {
        UiSlotSourceState::Unset
    };
    let mut speed = knob_control(
        "speed",
        1.6,
        0.0,
        4.0,
        UiSlotFieldState::editable(),
        speed_source,
    );
    // A bound knob carries the channel's quantized live reading
    // (display-only; the authored 1.6 stays the edit target).
    speed.live_value = speed_bound.then(|| "2.72".to_string());
    vec![
        speed,
        knob_control(
            "hue",
            0.32,
            0.0,
            1.0,
            UiSlotFieldState::editable(),
            UiSlotSourceState::Unset,
        ),
        knob_control(
            "scale",
            2.0,
            0.0,
            4.0,
            UiSlotFieldState::editable()
                .with_dirty(UiNodeDirtyState::Dirty)
                .with_debug(true),
            UiSlotSourceState::Unset,
        ),
        toggle_control(
            "mirror",
            true,
            UiSlotFieldState::editable(),
            UiSlotSourceState::Unset,
        ),
    ]
}

// -- phasor period knob + clock face (P7 items 4-5) -----------------------

/// A phasor slot's period knob (P7 item 5): the ONE control a phasor gets.
///
/// Its number is seconds-per-cycle and its emit family re-wraps that number
/// into a whole `PhasorConfig` on the way out, so the slot's waveform and
/// phase offset — never panel-editable (settled D11 v1) — survive a turn.
///
/// `shared` is the channel-driven case: an authored config channel puts the
/// knob on the module panel, and every reader of that channel rides the one
/// integrator it retunes (parent D3), which is what the violet bound
/// treatment is saying.
pub(crate) fn period_knob(label: &str, seconds: f32, shared: bool) -> UiPanelControl {
    let source = if shared {
        UiSlotSourceState::Bound(UiBindingEndpoint::new("bus:speed"))
    } else {
        UiSlotSourceState::Unset
    };
    let mut control = knob_control(
        label,
        seconds,
        0.0,
        120.0,
        UiSlotFieldState::editable(),
        source,
    );
    control.emit = UiPanelEmit::PhasorPeriod {
        waveform: lpa_studio_core::Waveform::Ramp,
        phase_offset: 0.0,
    };
    control.unit = Some(UiSlotUnit::seconds());
    control.value = control.value.clone().with_unit(UiSlotUnit::seconds());
    if shared {
        control.panel_target = Some(lpa_studio_core::UiPanelTarget {
            scope: lpc_wire::WireScopeRef::Module {
                owner: lpa_studio_core::NodeId::new(1),
            },
            channel: "speed".to_string(),
            engaged: false,
        });
    }
    control
}

/// One phasor row for the clock's listing.
pub(crate) fn phasor_row(
    origin: &str,
    detail: &str,
    shared: bool,
    phase: f32,
    cycle: u32,
    period_seconds: f32,
) -> UiPhasorRow {
    UiPhasorRow {
        origin: origin.to_string(),
        detail: Some(detail.to_string()),
        shared,
        phase,
        phase_display: lpa_studio_core::format_phase(phase),
        cycle,
        period_display: lpa_studio_core::format_period_seconds(period_seconds),
    }
}

/// A clock face in one of the listing's three states.
pub(crate) fn clock_face(timebase: UiTimebaseState, phasors: Vec<UiPhasorRow>) -> UiClockFace {
    let mut face =
        UiClockFace::new(UiProducedProduct::time("product").with_detail("node 2 output 0"));
    face.readings = vec![
        UiProducedValue::new("Seconds", "42.35").with_unit(UiSlotUnit::seconds()),
        UiProducedValue::new("Delta seconds", "0.033").with_unit(UiSlotUnit::seconds()),
    ];
    face.timebase = timebase;
    face.phasors = phasors;
    face
}

/// A clock node card with the face installed.
pub(crate) fn clock_node_view(face: UiClockFace) -> UiNodeView {
    let header = UiNodeHeader::new("Clock", "Clock", "/fyeah_sign.show/clock.clock")
        .with_status(UiStatus::good("Running"))
        .with_summary("1.0x");
    let mut view =
        UiNodeView::new(header, vec![UiNodeTab::main(Vec::new())]).with_node_id("clock-fyeah");
    view.face = Some(UiNodeFace::Clock(face));
    view
}

/// Wide aurora-ish visual hero for the shader face (deterministic bytes).
pub(crate) fn shader_hero_product() -> UiProducedProduct {
    UiProducedProduct::visual("output")
        .with_detail("256 x 256")
        .with_tracking(UiProductTrackingState::Tracking)
        .with_frame(UiProductPreviewFrame::new(16, 7))
        .with_preview(aurora_preview(48, 21, 0.0))
}

/// Deterministic aurora-ish pixel field (the spike's preview vibe) at a
/// story-friendly resolution.
pub(crate) fn aurora_preview(width: u32, height: u32, seed: f32) -> UiProductPreview {
    let mut bytes = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let u = x as f32 / width.max(1) as f32;
            let v = y as f32 / height.max(1) as f32;
            let field =
                ((u * 6.0 + seed).sin() + ((u + v) * 4.0 + seed * 2.0).sin() + (v * 7.0).sin())
                    / 3.0
                    * 0.5
                    + 0.5;
            bytes.push((20.0 + 60.0 * field) as u8);
            bytes.push((40.0 + 180.0 * field) as u8);
            bytes.push((70.0 + 150.0 * (1.0 - field * 0.5)) as u8);
        }
    }
    UiProductPreview::VisualSrgb8 {
        width,
        height,
        revision: 104,
        bytes: bytes.into(),
    }
}

const AURORA_GLSL: &str = "\
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float speed;

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    float rim = smoothstep(0.35, 0.95, length(uv - 0.5));
    float drift = time * speed * mix(1.0, 0.25, rim);
    return vec4(0.2 + 0.8 * sin(drift + uv.xyx), 1.0);
}
";

/// The code drawer's inline GLSL editor.
pub(crate) fn shader_code_editor() -> UiAssetEditor {
    UiAssetEditor {
        artifact: ArtifactLocation::file("/aurora.glsl"),
        kind: UiAssetEditorKind::Glsl,
        source: "aurora.glsl".to_string(),
        content: Some(UiAssetContent::from_bytes(AURORA_GLSL.as_bytes(), true, 4)),
        in_flight: false,
        failure: None,
        shader_error: None,
        uniforms: vec![
            UiShaderUniform {
                name: "time".to_string(),
                glsl_type: "float".to_string(),
            },
            UiShaderUniform {
                name: "speed".to_string(),
                glsl_type: "float".to_string(),
            },
        ],
        agent: None,
    }
}

/// Condensed idle chat for the shader face.
pub(crate) fn shader_agent_view(status: UiAgentStatus) -> UiAgentView {
    let turns = match status {
        UiAgentStatus::Streaming => vec![
            UiAgentTurn::User {
                text: "make the ribbons drift slower near the edges".to_string(),
            },
            UiAgentTurn::Assistant {
                text: "Easing the drift toward the rim and re-checking the".to_string(),
            },
        ],
        _ => vec![
            UiAgentTurn::User {
                text: "make the ribbons drift slower near the edges".to_string(),
            },
            UiAgentTurn::Tool(UiAgentToolRow {
                id: "tu_1".to_string(),
                note: Some("ease drift toward the rim".to_string()),
                phase: None,
                done: true,
                staged: true,
                edit_turn: None,
                shader_ok: Some(true),
                probes: 2,
                warnings: 0,
                error: None,
                detail: "{\n  \"probes\": 2,\n  \"shader_ok\": true\n}".to_string(),
            }),
            UiAgentTurn::Assistant {
                text: "Done — falloff now eases the drift toward the rim. \
                       Staged as an unsaved edit."
                    .to_string(),
            },
        ],
    };
    UiAgentView {
        artifact: ArtifactLocation::file("/aurora.glsl"),
        availability: UiAgentAvailability::Ready,
        setup: None,
        status,
        turns,
        usage: UiAgentUsage {
            input_tokens: 2841,
            output_tokens: 512,
            ..UiAgentUsage::default()
        },
        estimated_cost: Some("~$0.0162".to_string()),
        history: Vec::new(),
        history_dropped: 0,
        model: lpa_studio_core::UiAgentModelView {
            effective: Some("claude-sonnet-5".to_string()),
            options: Vec::new(),
            loading: false,
        },
        debug: None,
    }
}

/// Advanced-drawer sections for the shader card: today's slot rows,
/// unchanged.
pub(crate) fn shader_sections() -> Vec<UiNodeSection> {
    vec![UiNodeSection::ConfigSlots(vec![
        UiConfigSlot::value("source", "Source", UiSlotValue::string("aurora.glsl"))
            .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
        UiConfigSlot::value("speed", "Speed", UiSlotValue::f32(1.6)).with_source(bound_source()),
        UiConfigSlot::value("hue", "Hue", UiSlotValue::f32(0.32)),
        UiConfigSlot::value("scale", "Scale", UiSlotValue::f32(2.0)),
        UiConfigSlot::value(
            "output_size",
            "Output size",
            UiSlotValue::string("256 x 256"),
        ),
    ])]
}

/// The complete shader face.
pub(crate) fn shader_face(speed_bound: bool, agent_status: UiAgentStatus) -> UiShaderFace {
    UiShaderFace {
        preview: shader_hero_product(),
        controls: shader_controls(speed_bound),
        agent: Some(shader_agent_view(agent_status)),
        code_drawer: Some(shader_code_editor()),
    }
}

/// A full shader node card view with the face installed (stories exercise
/// the `NodePane` face branch this way; controllers still seed `None`).
pub(crate) fn shader_node_view(speed_bound: bool, agent_status: UiAgentStatus) -> UiNodeView {
    let header = UiNodeHeader::new("Aurora", "Shader", "/fyeah_sign.show/aurora.shader")
        .with_source("aurora.glsl")
        .with_status(UiStatus::good("Running"))
        .with_summary("GPU");
    let mut view = UiNodeView::new(header, vec![UiNodeTab::main(shader_sections())])
        .with_node_id("shader-aurora");
    view.face = Some(UiNodeFace::Shader(shader_face(speed_bound, agent_status)));
    view
}

/// The complete fixture face (ring lamp preview + dominant fader).
pub(crate) fn fixture_face() -> UiFixtureFace {
    UiFixtureFace {
        preview: control_preview_product("output"),
        mapping_editor: None,
        brightness: fader_control(
            184.0,
            UiSlotFieldState::editable(),
            UiSlotSourceState::Unset,
        ),
        // Opted out (budget 0) — the only state with no readout now that an
        // unstated budget falls back to the default guard.
        power: None,
    }
}

/// A fixture inside its declared budget: the readout is a setup number.
pub(crate) fn fixture_face_within_budget() -> UiFixtureFace {
    UiFixtureFace {
        power: Some(lpa_studio_core::UiFixturePower {
            estimated_draw_ma: 780,
            budget_ma: 1000,
            scale: 1.0,
        }),
        ..fixture_face()
    }
}

/// A fixture actively shedding current to stay inside its budget.
pub(crate) fn fixture_face_limiting() -> UiFixtureFace {
    UiFixtureFace {
        power: Some(lpa_studio_core::UiFixturePower {
            estimated_draw_ma: 2400,
            budget_ma: 1000,
            scale: 0.41,
        }),
        ..fixture_face()
    }
}

/// The fyeah corpus doc trimmed to the letters fit for a storybook: the
/// full sign spells something saltier than baseline PNGs should. Keeps the
/// real import framing (canvas) and multi-path chain; the engineering
/// corpus itself stays complete.
pub(crate) fn fyeah_presentable_doc() -> lpc_mapping::Map2dDoc {
    let mut doc = lpc_mapping::corpus::fyeah();
    doc.objects
        .retain(|object| !matches!(object.name.as_str(), "p2" | "p3" | "p4"));
    doc
}

/// A fixture face whose lamp layout comes from a shared mapping-corpus
/// document (16×16 fixture render target, like the real fyeah fixture).
pub(crate) fn map2d_fixture_face(doc: &lpc_mapping::Map2dDoc) -> UiFixtureFace {
    UiFixtureFace {
        preview: map2d_control_preview_product("output", doc, (16, 16)),
        mapping_editor: None,
        brightness: fader_control(
            184.0,
            UiSlotFieldState::editable(),
            UiSlotSourceState::Unset,
        ),
        power: None,
    }
}

/// Advanced-drawer sections for the fixture card.
pub(crate) fn fixture_sections() -> Vec<UiNodeSection> {
    vec![UiNodeSection::ConfigSlots(vec![
        UiConfigSlot::value("mapping", "Mapping", UiSlotValue::string("ring · 241 pts")),
        UiConfigSlot::value("input", "Input", UiSlotValue::string("Evening set")).with_source(
            UiSlotSourceState::Bound(UiBindingEndpoint::new("playlist:evening-set")),
        ),
        UiConfigSlot::value("driver", "Driver", UiSlotValue::string("auto")),
        UiConfigSlot::value("channel", "Channel", UiSlotValue::string("D10")),
    ])]
}

/// A full fixture node card view with the face installed.
pub(crate) fn fixture_node_view() -> UiNodeView {
    let header = UiNodeHeader::new("Halo ring", "Fixture", "/fyeah_sign.show/halo.fixture")
        .with_source("halo.json")
        .with_status(UiStatus::good("Running"))
        .with_summary("241 LEDs");
    let mut view = UiNodeView::new(header, vec![UiNodeTab::main(fixture_sections())])
        .with_node_id("fixture-halo");
    view.face = Some(UiNodeFace::Fixture(fixture_face()));
    view
}

/// The same fixture card carrying a specific power state.
pub(crate) fn fixture_node_view_with_face(face: UiFixtureFace) -> UiNodeView {
    let mut view = fixture_node_view();
    view.face = Some(UiNodeFace::Fixture(face));
    view
}

/// Playlist entries: three timed entries plus one cue entry; Aurora (key 1)
/// is playing. Every entry carries the child-select action the P4
/// derivation attaches (clicking a chip focuses the entry's child node).
pub(crate) fn playlist_entries() -> Vec<UiPlaylistEntry> {
    vec![
        UiPlaylistEntry {
            key: 0,
            name: "Sunrise".to_string(),
            duration_ms: Some(180_000),
            cue: false,
            thumb: Some(aurora_preview(18, 10, 3.1)),
            action: Some(entry_select_action("Sunrise")),
        },
        UiPlaylistEntry {
            key: 1,
            name: "Aurora".to_string(),
            duration_ms: Some(270_000),
            cue: false,
            thumb: Some(aurora_preview(18, 10, 4.8)),
            action: Some(entry_select_action("Aurora")),
        },
        UiPlaylistEntry {
            key: 2,
            name: "Embers".to_string(),
            duration_ms: Some(165_000),
            cue: false,
            thumb: Some(aurora_preview(18, 10, 6.5)),
            action: Some(entry_select_action("Embers")),
        },
        UiPlaylistEntry {
            key: 3,
            name: "Tide".to_string(),
            duration_ms: None,
            cue: true,
            thumb: Some(aurora_preview(18, 10, 8.2)),
            action: Some(entry_select_action("Tide")),
        },
    ]
}

/// The node-select action a strip entry dispatches (story mock — dispatch
/// goes to the story's no-op handler).
fn entry_select_action(name: &str) -> UiAction {
    UiAction::from_op(ControllerId::new("story.module"), ProjectEditorOp::Focus)
        .with_label(format!("Select {name}"))
}

/// The playlist face with Aurora active.
pub(crate) fn playlist_face() -> UiPlaylistFace {
    UiPlaylistFace {
        entries: playlist_entries(),
        active: Some(1),
    }
}

/// The active child: Aurora's own shader card (face and all), rendered
/// BELOW the playlist card as a sibling (P2c item 2). Like the production
/// derivation, it does NOT wear the `active` flag — the web maps that onto
/// the pane's *selection* look, and the strip's ACTIVE placard is the
/// active-ness presentation (P6 item 7).
pub(crate) fn playlist_active_child() -> UiNodeChild {
    let mut child =
        UiNodeChild::new("Aurora", "Shader", "./aurora.json").with_sections(shader_sections());
    child.status = UiStatus::good("Running");
    child.summary = Some("playing, 1:12 remaining".to_string());
    child.face = Some(UiNodeFace::Shader(shader_face(true, UiAgentStatus::Idle)));
    child
}

/// Advanced-drawer sections for the playlist card.
pub(crate) fn playlist_sections() -> Vec<UiNodeSection> {
    vec![UiNodeSection::ConfigSlots(vec![
        UiConfigSlot::value(
            "mode",
            "Mode",
            UiSlotValue::string("sequence · crossfade 2s"),
        ),
        UiConfigSlot::value("entries", "Entries", UiSlotValue::u32(4))
            .with_detail("4 child invocations"),
        UiConfigSlot::value(
            "default_fade",
            "Default fade",
            UiSlotValue::f32(0.35).with_unit(UiSlotUnit::seconds()),
        ),
    ])]
}

/// A full playlist node card view with the face installed. `children`
/// carries EXACTLY the active child — the story-level stand-in for the P4
/// derivation invariant (one rendering of the active child, zero of the
/// others); it renders below the card as a sibling.
pub(crate) fn playlist_node_face_view() -> UiNodeView {
    let header = UiNodeHeader::new(
        "Evening set",
        "Playlist",
        "/fyeah_sign.show/evening.playlist",
    )
    .with_source("evening.json")
    .with_status(UiStatus::good("Running"))
    .with_summary("playing 2/4");
    let mut view = UiNodeView::new(header, vec![UiNodeTab::main(playlist_sections())])
        .with_node_id("playlist-evening")
        .with_children(vec![playlist_active_child()]);
    view.face = Some(UiNodeFace::Playlist(playlist_face()));
    view
}

/// An empty playlist card (no entries authored yet).
pub(crate) fn empty_playlist_node_view() -> UiNodeView {
    let header = UiNodeHeader::new(
        "Evening set",
        "Playlist",
        "/fyeah_sign.show/evening.playlist",
    )
    .with_source("evening.json")
    .with_status(UiStatus::neutral("Idle"))
    .with_summary("no entries");
    let mut view = UiNodeView::new(header, vec![UiNodeTab::main(playlist_sections())])
        .with_node_id("playlist-empty");
    view.face = Some(UiNodeFace::Playlist(UiPlaylistFace {
        entries: Vec::new(),
        active: None,
    }));
    view
}

// -- output face ---------------------------------------------------------------

/// The output card's node address — every channel edit address hangs off it.
const OUTPUT_NODE_PATH: &str = "/fyeah_sign.show/strips.output";

/// Story-only slot address on the output card, so the channel rows render
/// wired (dispatch goes to the story's no-op handler).
fn output_slot_address(path: &str) -> ProjectSlotAddress {
    ProjectSlotAddress::new(
        ProjectNodeAddress::parse(OUTPUT_NODE_PATH).expect("valid story node address"),
        ProjectSlotRoot::def(),
        SlotPath::parse(path).expect("valid story slot path"),
    )
}

/// One wire as `node_face_builder` derives it: the endpoint verbatim, its
/// pin label, and the two edit addresses. `count: None` is the remainder
/// channel, which by the same rule as the builder carries NO count address
/// (an absent option has nothing to write to until it is included).
pub(crate) fn output_channel(key: u32, pin: &str, count: Option<u32>) -> UiOutputChannelRow {
    UiOutputChannelRow {
        key,
        endpoint_display: format!("ws281x:local:{pin}"),
        pin_label: pin.to_string(),
        gpio: None,
        count,
        resolved_count: count,
        slice_start: None,
        endpoint_address: Some(output_slot_address(&format!("channels[{key}].endpoint"))),
        count_address: count.map(|_| output_slot_address(&format!("channels[{key}].count.some"))),
    }
}

/// Hand the channels their slice starts exactly the way the builder's
/// `resolve_authored_slices` does: a count advances the cursor, and the
/// count-less wire takes the remainder.
fn resolve_story_slices(channels: &mut [UiOutputChannelRow]) {
    let mut start = Some(0u32);
    for channel in channels {
        channel.slice_start = start;
        match channel.count {
            Some(count) => start = start.map(|start| start.saturating_add(count)),
            None => start = None,
        }
    }
}

/// The board facts the decoration pass fills, derived from the real embedded
/// display manifest so a story cannot drift from the catalog: every
/// output-eligible pin, RAILS AND SCREW TERMINALS alike (the terminals list
/// is separate and easy to drop), with each channel's claim marked.
fn output_board_facts(board_id: &str, channels: &[UiOutputChannelRow]) -> UiOutputBoardFacts {
    let board = lpa_boards::board_by_id(board_id).expect("board in the embedded catalog");
    let pins = board
        .pins()
        .map(|pin| (&pin.label, pin.role, pin.gpio))
        .chain(
            board
                .hw
                .terminals
                .iter()
                .map(|terminal| (&terminal.label, terminal.role, terminal.gpio)),
        )
        .filter(|(_, role, _)| role.output_eligible())
        .filter_map(|(label, _, gpio)| {
            Some(UiOutputPin {
                assigned_to: channels
                    .iter()
                    .find(|channel| channel.pin_label == *label)
                    .map(|channel| channel.key),
                label: label.clone(),
                gpio: u32::from(gpio?),
            })
        })
        .collect();
    UiOutputBoardFacts {
        board_id: board.board_id.clone(),
        display_name: board.display_name.clone(),
        pins,
    }
}

/// A complete output face, assembled the way builder-then-decoration
/// assembles one: slices from the authored counts, board facts and each
/// wire's GPIO from the known board, and the incoming extent resolving the
/// remainder wire.
pub(crate) fn output_face(
    board_id: Option<&str>,
    mut channels: Vec<UiOutputChannelRow>,
    total_lamps: Option<u32>,
    span_boundaries: Vec<u32>,
) -> UiOutputFace {
    resolve_story_slices(&mut channels);
    let board = board_id.map(|board_id| output_board_facts(board_id, &channels));
    if let Some(board) = &board {
        for channel in &mut channels {
            channel.gpio = board
                .pins
                .iter()
                .find(|pin| pin.label == channel.pin_label)
                .map(|pin| pin.gpio);
        }
    }
    let mut face = UiOutputFace {
        channels,
        channels_address: Some(output_slot_address("channels")),
        input_binding: Some("bus:show.control".to_string()),
        total_lamps: None,
        span_boundaries,
        board,
    };
    if let Some(total) = total_lamps {
        face.resolve_extent(total);
    }
    face
}

/// Advanced-drawer sections for the output card — the same rows the face
/// derives from, so the drawer stays the free-text fallback for anything the
/// face does not offer (a pin no board declares, a hand-written endpoint).
pub(crate) fn output_sections() -> Vec<UiNodeSection> {
    vec![UiNodeSection::ConfigSlots(vec![
        UiConfigSlot::value("input", "Input", UiSlotValue::string("Show control")).with_source(
            UiSlotSourceState::Bound(UiBindingEndpoint::new("bus:show.control")),
        ),
        UiConfigSlot::value("options", "Options", UiSlotValue::string("GRB · gamma 2.2")),
        UiConfigSlot::value("test_pattern", "Test pattern", UiSlotValue::string("off")),
    ])]
}

/// A full output node card view with the face installed.
pub(crate) fn output_node_view(face: UiOutputFace, summary: &str) -> UiNodeView {
    let header = UiNodeHeader::new("Strips", "Output", OUTPUT_NODE_PATH)
        .with_source("strips.json")
        .with_status(UiStatus::good("Running"))
        .with_summary(summary);
    let mut view = UiNodeView::new(header, vec![UiNodeTab::main(output_sections())])
        .with_node_id("output-strips");
    view.face = Some(UiNodeFace::Output(face));
    view
}

/// The M5 face-editor fixture: the mapping asset's inline-editor plumbing
/// with the document body pre-resolved (no fetch round-trip in stories).
pub(crate) fn map2d_fixture_face_editing(doc: &lpc_mapping::Map2dDoc) -> UiFixtureFace {
    let mut face = map2d_fixture_face(doc);
    face.mapping_editor = Some(UiAssetEditor {
        artifact: ArtifactLocation::file("/fyeah.map2d.json"),
        kind: UiAssetEditorKind::Map2d,
        source: "fyeah.map2d.json".to_string(),
        content: Some(UiAssetContent::from_bytes(
            doc.to_json().as_bytes(),
            false,
            104,
        )),
        in_flight: false,
        failure: None,
        shader_error: None,
        uniforms: Vec::new(),
        agent: None,
    });
    face
}

#[cfg(test)]
mod tests {
    use super::{output_channel, output_face};

    /// The output fixtures do real parsing (slot paths) and real catalog
    /// lookups (`board_by_id`), both behind `expect`. Nothing else runs them
    /// on the host — a story only fails in the capture browser — so this is
    /// the guard that keeps a renamed board or a rejected path shape from
    /// reaching CI as a blank baseline.
    #[test]
    fn the_output_fixtures_assemble_the_way_the_real_derivation_does() {
        let face = output_face(
            Some("domraem/dom-z-102"),
            vec![
                output_channel(0, "IO18", Some(280)),
                output_channel(1, "IO2", None),
            ],
            Some(1500),
            vec![0, 280],
        );

        assert_eq!(
            face.channels[0].gpio,
            Some(18),
            "IO18 resolves on the desk board"
        );
        assert_eq!(face.channels[0].slice_start, Some(0));
        assert_eq!(face.channels[1].slice_start, Some(280));
        assert_eq!(
            face.channels[1].resolved_count,
            Some(1220),
            "the count-less wire takes what is left of the 1500"
        );
        let board = face
            .board
            .as_ref()
            .expect("the desk board is in the catalog");
        assert!(!board.display_name.is_empty());
        let assigned: Vec<(&str, Option<u32>)> = board
            .pins
            .iter()
            .map(|pin| (pin.label.as_str(), pin.assigned_to))
            .collect();
        assert_eq!(
            assigned,
            [
                ("IO13", None),
                ("IO18", Some(0)),
                ("IO16", None),
                ("IO14", None),
                ("IO2", Some(1)),
            ]
        );

        // The other board a story draws: its LED outputs live in the separate
        // terminals list, which a naive `pins()` walk drops entirely.
        let dig_uno = output_face(
            Some("quinled/dig-uno"),
            vec![output_channel(0, "LED1", None)],
            Some(241),
            Vec::new(),
        );
        assert!(
            dig_uno.channels[0].gpio.is_some(),
            "LED1 is an eligible pin"
        );
    }
}
