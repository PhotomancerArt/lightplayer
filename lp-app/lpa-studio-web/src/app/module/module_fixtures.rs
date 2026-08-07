//! Story DTOs for the module face, panel, and play mode.
//!
//! Every fixture here is hand-built: it mirrors the shapes
//! `ProjectController::module_face` derives, so the storybook stays
//! deterministic instead of depending on a live engine. The content is not
//! arbitrary — it reproduces the worked examples from
//! `docs/design/modules.md`, so the stories judge the real shapes.
//!
//! - **E3 (drop-in embed):** `plasma` embedded in a host that already has a
//!   visual; the host's panel shows a plasma group.
//! - **E4 (shared control + detach):** one host `speed` control drives both
//!   plasma instances; plasma_1 has been touched, so it is engaged and
//!   detached while plasma_2 still follows the host.
//! - **E2 (playlist meta-switching):** two entries binding one `speed`
//!   channel with different meta — 0–1 "Drift" vs 0–10 "Whirl".
//! - **The scarf (panel.md §3):** `brightness` held at a dim value while
//!   the authored default is bright.

use lpa_studio_core::{
    LpValue, ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot, SlotEditOp, SlotPath,
    UiAction, UiBusChannelPreview, UiBusChannelView, UiBusSiteOrigin, UiBusSiteView, UiBusView,
    UiModuleFace, UiNodeChild, UiNodeFace, UiNodeHeader, UiNodeSection, UiNodeView, UiPanelControl,
    UiPanelControlState, UiPanelControlView, UiPanelEmit, UiPanelGroup, UiPanelWidget,
    UiPlaylistEntry, UiPlaylistFace, UiProducedProduct, UiProductKind, UiProductPreviewFrame,
    UiProductTrackingState, UiSlotFieldState, UiSlotValue, UiStatus,
};

use crate::app::node::face_story_fixtures::aurora_preview;
use crate::app::node::node_story_fixtures::control_preview_product;

use super::PanelGesture;

/// The root module's scope path. Scope paths are node paths (§6), so the
/// root module's is the root itself.
pub(crate) const ROOT_SCOPE: &str = "/aurora.module";
/// The two embedded plasma instances' scopes — different scopes is exactly
/// why their controls are independent (R8).
pub(crate) const PLASMA_1_SCOPE: &str = "/aurora.module/plasma_1.module";
pub(crate) const PLASMA_2_SCOPE: &str = "/aurora.module/plasma_2.module";

/// The structured scope behind each fixture scope path — what the real
/// derivation carries on `UiPanelGroup::target` and every control's
/// `panel_target`, so the reset/clear affordances render and the story
/// handlers can resolve a gesture back to its display scope.
pub(crate) fn scope_target(scope: &str) -> lpc_wire::WireScopeRef {
    let module = |owner: u32| lpc_wire::WireScopeRef::Module {
        owner: lpa_studio_core::NodeId::new(owner),
    };
    // The playlist fixture's two entries are SINK scopes — two identities
    // for one channel name, which is the whole E2 point.
    let sink = |entry: u32| lpc_wire::WireScopeRef::Sink {
        owner: lpa_studio_core::NodeId::new(4),
        entry,
    };
    match scope {
        ROOT_SCOPE => module(1),
        PLASMA_1_SCOPE => module(2),
        PLASMA_2_SCOPE => module(3),
        "/aurora.module/set.playlist/drift.shader" => sink(0),
        "/aurora.module/set.playlist/whirl.shader" => sink(1),
        other => panic!("unknown fixture scope {other}"),
    }
}

/// The display path a structured fixture scope stands for (the reverse of
/// [`scope_target`]).
pub(crate) fn scope_display(target: &lpc_wire::WireScopeRef) -> &'static str {
    match target {
        lpc_wire::WireScopeRef::Module { owner } => match owner.0 {
            1 => ROOT_SCOPE,
            2 => PLASMA_1_SCOPE,
            3 => PLASMA_2_SCOPE,
            other => panic!("unknown fixture scope owner {other}"),
        },
        lpc_wire::WireScopeRef::Sink { entry: 0, .. } => "/aurora.module/set.playlist/drift.shader",
        lpc_wire::WireScopeRef::Sink { .. } => "/aurora.module/set.playlist/whirl.shader",
    }
}

/// A story-only slot address, so the widgets render wired and their drags
/// dispatch into the story's own handler.
fn walk_address(node: &str, slot: &str) -> ProjectSlotAddress {
    ProjectSlotAddress::new(
        ProjectNodeAddress::parse(node).expect("valid story node address"),
        ProjectSlotRoot::def(),
        SlotPath::parse(slot).expect("valid story slot path"),
    )
}

/// One knob control on a panel.
fn knob(
    scope: &str,
    channel: &str,
    label: &str,
    value: f32,
    min: f32,
    max: f32,
    step: Option<f32>,
) -> UiPanelControlView {
    UiPanelControlView::new(
        channel,
        UiPanelControl {
            emit: UiPanelEmit::Value,
            label: label.to_string(),
            address: Some(walk_address(scope, channel)),
            widget: UiPanelWidget::Knob { min, max, step },
            value: UiSlotValue::f32(value),
            live_value: None,
            live_gradient: None,
            panel_target: Some(lpa_studio_core::UiPanelTarget {
                scope: scope_target(scope),
                channel: channel.to_string(),
                engaged: false,
            }),
            unit: None,
            state: UiSlotFieldState::editable(),
            aspects: Vec::new(),
        },
    )
}

/// One horizontal fader control (the dominant brightness gesture).
fn fader(scope: &str, channel: &str, label: &str, value: f32, max: f32) -> UiPanelControlView {
    UiPanelControlView::new(
        channel,
        UiPanelControl {
            emit: UiPanelEmit::Value,
            label: label.to_string(),
            address: Some(walk_address(scope, channel)),
            widget: UiPanelWidget::Fader {
                min: 0.0,
                max,
                step: Some(1.0),
            },
            value: UiSlotValue::f32(value),
            live_value: None,
            live_gradient: None,
            panel_target: Some(lpa_studio_core::UiPanelTarget {
                scope: scope_target(scope),
                channel: channel.to_string(),
                engaged: false,
            }),
            unit: None,
            state: UiSlotFieldState::editable(),
            aspects: Vec::new(),
        },
    )
}

/// One pill toggle control.
fn toggle(scope: &str, channel: &str, label: &str, value: bool) -> UiPanelControlView {
    UiPanelControlView::new(
        channel,
        UiPanelControl {
            emit: UiPanelEmit::Value,
            label: label.to_string(),
            address: Some(walk_address(scope, channel)),
            widget: UiPanelWidget::Toggle,
            value: UiSlotValue::bool(value),
            live_value: None,
            live_gradient: None,
            panel_target: Some(lpa_studio_core::UiPanelTarget {
                scope: scope_target(scope),
                channel: channel.to_string(),
                engaged: false,
            }),
            unit: None,
            state: UiSlotFieldState::editable(),
            aspects: Vec::new(),
        },
    )
}

/// One palette swatch control (M4 P3) — the closed face of the chooser, on
/// a module panel. Its value is a whole `GradientConfig`, built through the
/// model's own storage exactly as the projection builds one.
fn swatch(
    scope: &str,
    channel: &str,
    label: &str,
    config: &lpc_model::GradientConfig,
) -> UiPanelControlView {
    UiPanelControlView::new(
        channel,
        UiPanelControl {
            emit: UiPanelEmit::Gradient,
            label: label.to_string(),
            address: Some(walk_address(scope, channel)),
            widget: UiPanelWidget::PaletteSwatch,
            value: crate::app::node::node_story_fixtures::gradient_slot_value(config),
            live_value: None,
            live_gradient: None,
            panel_target: Some(lpa_studio_core::UiPanelTarget {
                scope: scope_target(scope),
                channel: channel.to_string(),
                engaged: false,
            }),
            unit: None,
            state: UiSlotFieldState::editable(),
            aspects: Vec::new(),
        },
    )
}

/// A panel of palette swatches in the three panel states — the module-panel
/// half of the P3 gate, where the node card's `palette-swatch` stories are
/// the other. Static and cycle sit side by side on purpose: the two modes
/// are the widget's whole design question.
pub(crate) fn palette_panel() -> UiPanelGroup {
    use crate::app::node::node_story_fixtures::{palette_cycle, sunset_gradient};
    let held = lpc_model::GradientConfig::Static(sunset_gradient());
    UiPanelGroup::new("Palettes", ROOT_SCOPE)
        .with_target(scope_target(ROOT_SCOPE))
        .with_controls(vec![
            at_default(
                swatch(ROOT_SCOPE, "palette", "at default", &held),
                "authored palette",
            ),
            {
                // Following: a config channel drives the slot, and what
                // comes back is the channel's summary in words — the strips
                // keep showing the authored config.
                let mut view = swatch(ROOT_SCOPE, "cycle", "following", &palette_cycle());
                view.control.live_value = Some(
                    lpa_studio_core::app::project::format_gradient_summary(&palette_cycle()),
                );
                view.with_state(
                    UiPanelControlState::ReadFollowing,
                    Some("show \u{b7} palette"),
                )
            },
            engaged(
                swatch(ROOT_SCOPE, "held", "engaged", &palette_cycle()),
                "show \u{b7} palette",
            ),
        ])
}

/// Put a control in Read-following-automation, displaying `live`.
fn following(mut view: UiPanelControlView, live: &str, source: &str) -> UiPanelControlView {
    view.control.live_value = Some(live.to_string());
    view.with_state(UiPanelControlState::ReadFollowing, Some(source))
}

/// Put a control in Engaged (Latch), noting what it displaced.
fn engaged(view: UiPanelControlView, displaced: &str) -> UiPanelControlView {
    view.with_state(UiPanelControlState::Engaged, Some(displaced))
}

/// Put a control in Read-at-default with an explicit origin caption.
fn at_default(view: UiPanelControlView, source: &str) -> UiPanelControlView {
    view.with_state(UiPanelControlState::ReadDefault, Some(source))
}

// ---------------------------------------------------------------- plasma

/// One embedded plasma instance's panel. Both instances bind the same
/// channel names — they stay independent because they are different scopes
/// (R8), which the scope path on the group heading spells out.
fn plasma_panel(scope: &str, speed: UiPanelControlView) -> UiPanelGroup {
    // The group's LABEL carries the instance identity now that the scope
    // path has moved into the heading's detail popup — two copies of one
    // effect have to be tellable apart from the rule alone.
    UiPanelGroup::new(instance_label(scope), scope)
        .with_target(scope_target(scope))
        .with_controls(vec![
            speed,
            at_default(
                knob(scope, "hue", "hue", 0.32, 0.0, 1.0, None),
                "authored default",
            ),
        ])
}

/// "plasma 1" from `/aurora.module/plasma_1.module` — the embedded node's
/// own name, which is what distinguishes two instances on a rule.
fn instance_label(scope: &str) -> String {
    scope
        .rsplit('/')
        .next()
        .and_then(|leaf| leaf.split('.').next())
        .map(|name| name.replace('_', " "))
        .unwrap_or_else(|| "module".to_string())
}

/// Either plasma instance's panel in its **Read** form: nothing touched,
/// so both walk outward and inherit the host's `speed` writer (R5). The two
/// instances are byte-identical apart from their scope — which is the
/// point: the only thing that makes plasma_1 different in the stories is
/// that somebody turned its knob.
pub(crate) fn plasma_read_panel(scope: &str) -> UiPanelGroup {
    plasma_panel(
        scope,
        following(
            knob(scope, "speed", "speed", 0.62, 0.0, 1.0, None),
            "0.62",
            "inherited · Aurora Sign",
        ),
    )
}

/// plasma_1's panel with its knob already touched — E4's detach, applied
/// through the same path a live touch takes.
pub(crate) fn plasma_one_panel() -> UiPanelGroup {
    let mut panel = plasma_read_panel(PLASMA_1_SCOPE);
    engage_group(&mut panel, PLASMA_1_SCOPE, "speed", 0.82);
    panel
}

/// An embedded plasma module's own face — the SAME component the root
/// wears, one level in. No `auto_save`: persistence is per project folder,
/// so only the root module presents that switch (P11).
pub(crate) fn plasma_face(panel: UiPanelGroup, seed: f32) -> UiModuleFace {
    UiModuleFace {
        preview: Some(
            UiProducedProduct::visual("output")
                .with_detail("mirrors visual.out")
                .with_tracking(UiProductTrackingState::Tracking)
                .with_frame(UiProductPreviewFrame::new(16, 5))
                .with_preview(aurora_preview(48, 15, seed)),
        ),
        panel,
        wiring: Some(plasma_wiring()),
        wiring_open: false,
        provenance: Some("PhotomancerArt · v1.2 · CC0-1.0".to_string()),
        auto_save: None,
    }
}

/// The plasma module's own children, as sibling cards below ITS card — the
/// nesting rail is the same one level down, which is the other half of the
/// "one face at every depth" claim.
pub(crate) fn plasma_children(seed: f32) -> Vec<UiNodeChild> {
    vec![
        product_child("sim", "Shader", "shader.json", "visual → bus:visual.out").with_sections(
            vec![UiNodeSection::ProducedProducts(vec![
                UiProducedProduct::visual("output")
                    .with_tracking(UiProductTrackingState::Tracking)
                    .with_frame(UiProductPreviewFrame::new(16, 5))
                    .with_preview(aurora_preview(48, 15, seed + 0.4)),
            ])],
        ),
    ]
}

/// The plasma scope's wiring: `time` and `speed` have no local writer, so
/// they resolve outward (R5); `visual.out` is written locally and published
/// up (R7).
fn plasma_wiring() -> UiBusView {
    UiBusView {
        channels: vec![
            channel(
                "time",
                "Instant",
                Some("12.44"),
                vec![],
                vec![site("sim", "time")],
            ),
            channel(
                "speed",
                "Float",
                Some("0.82"),
                vec![site("panel", "held")],
                vec![site("sim", "speed")],
            ),
            UiBusChannelView {
                primary_visual: true,
                ..channel(
                    "visual.out",
                    "Color",
                    Some("visual product #7:0"),
                    vec![site("sim", "visual")],
                    vec![site("plasma", "output")],
                )
            },
        ],
    }
}

// ------------------------------------------------------------------ root

/// The controls the stories start out **held**: the scarf's brightness in
/// the root scope, and plasma_1's speed in its own scope (E4's detach).
/// They are applied to the pristine Read face the same way a touch would
/// be, so clearing them lands back on exactly the Read form below.
pub(crate) const HELD: &[(&str, &str, f32)] = &[
    (ROOT_SCOPE, "brightness", 96.0),
    (PLASMA_1_SCOPE, "speed", 0.82),
];

/// The root module's panel in its **Read** form: its own scope's channels,
/// plus each embedded module's panel as a nested group (R8). The two plasma
/// groups are two independent presentations of the same effect.
pub(crate) fn root_panel() -> UiPanelGroup {
    UiPanelGroup::new("Aurora Sign", ROOT_SCOPE)
        .with_target(scope_target(ROOT_SCOPE))
        .with_controls(root_controls())
        .with_groups(vec![
            plasma_read_panel(PLASMA_1_SCOPE),
            plasma_read_panel(PLASMA_2_SCOPE),
        ])
}

/// The root scope's own channels, all in Read.
pub(crate) fn root_controls() -> Vec<UiPanelControlView> {
    vec![
        // The scarf (panel.md §3): authored bright; the stories hold it
        // dim, and P10 says it must come back dim without one bright frame.
        at_default(
            fader(ROOT_SCOPE, "brightness", "brightness", 200.0, 255.0),
            "authored 200",
        ),
        // E4's host control: an authored control node writes this channel,
        // so the panel control follows it until someone grabs it.
        following(
            knob(ROOT_SCOPE, "speed", "speed", 0.62, 0.0, 1.0, None),
            "0.62",
            "control · Master speed",
        ),
        // "Grab the LFO" (panel.md §3): the knob visibly rides the LFO.
        following(
            knob(ROOT_SCOPE, "hue", "hue", 0.41, 0.0, 1.0, None),
            "0.41",
            "lfo · hue",
        ),
        // A stepped channel: squared blocks, no ticks (knob v2 rule).
        at_default(
            knob(ROOT_SCOPE, "palette", "palette", 2.0, 1.0, 4.0, Some(1.0)),
            "authored default",
        ),
        // R6: no writer anywhere — the channel lists anyway, as an
        // invitation.
        at_default(
            toggle(ROOT_SCOPE, "mirror", "mirror", false),
            "no writer yet",
        ),
    ]
}

/// The root module's face with the story's held controls already engaged —
/// what the panel stories render, and what [`PanelWalk`] clears back from.
pub(crate) fn held_root_face() -> UiModuleFace {
    let Some(UiNodeFace::Module(face)) = held_root_view().face else {
        unreachable!("the root module view wears a module face")
    };
    face
}

/// The root module's face in its pristine Read form.
pub(crate) fn root_face() -> UiModuleFace {
    UiModuleFace {
        preview: Some(
            UiProducedProduct::visual("output")
                .with_detail("256 x 256 · mirrors visual.out")
                .with_tracking(UiProductTrackingState::Tracking)
                .with_frame(UiProductPreviewFrame::new(16, 7))
                .with_preview(aurora_preview(48, 21, 0.0)),
        ),
        panel: root_panel(),
        wiring: Some(root_wiring()),
        wiring_open: false,
        provenance: Some("Yona · v0.4 · created 2026-07-31".to_string()),
        // The project root owns panel persistence (P11).
        auto_save: Some(true),
    }
}

/// A control-first module's face: nothing writes the scope's visual, so
/// the mirror would render cleared and the hero is the scope's `control.out`
/// product instead — the fixture's lamp layout, drawn by the same preview
/// component the fixture card uses.
pub(crate) fn control_root_face() -> UiModuleFace {
    UiModuleFace {
        preview: Some(
            control_preview_product("output")
                .with_detail("16 RGB lamps · mirrors control.out")
                .with_tracking(UiProductTrackingState::Tracking),
        ),
        panel: UiPanelGroup::new("Scanner Rig", ROOT_SCOPE)
            .with_target(scope_target(ROOT_SCOPE))
            .with_controls(vec![at_default(
                fader(ROOT_SCOPE, "brightness", "brightness", 200.0, 255.0),
                "authored 200",
            )]),
        wiring: Some(control_wiring()),
        wiring_open: false,
        provenance: None,
        auto_save: Some(true),
    }
}

/// A control-first scope's wiring: the fixture renders the lamps and the
/// hardware output reads them; no channel carries a visual.
fn control_wiring() -> UiBusView {
    UiBusView {
        channels: vec![
            channel(
                "time",
                "Instant",
                Some("12.44"),
                vec![site("clock", "seconds")],
                vec![site("scanner", "time")],
            ),
            UiBusChannelView {
                // The hero and the value box show the SAME lamps: both
                // hang off the scope's resolved control product.
                preview: Some(UiBusChannelPreview {
                    kind: UiProductKind::Control,
                    preview: control_preview_product("output").preview,
                    tracking: UiProductTrackingState::Tracking,
                    frame: UiProductPreviewFrame::VISUAL_DEFAULT,
                }),
                ..channel(
                    "control.out",
                    "Color",
                    Some("control product #7:0"),
                    vec![site("Fixture", "output")],
                    vec![site("Output", "input")],
                )
            },
        ],
    }
}

/// The root module's children, as sibling cards BELOW its card: two leaves
/// that write host channels, the two embedded plasma modules (each with its
/// own child), and the fixture. All of them — a module's children are
/// collaborators, so there is no active-child filtering the way a playlist
/// has.
pub(crate) fn root_children() -> Vec<UiNodeChild> {
    vec![
        plain_child("clock", "Clock", "clock.json", "seconds → bus:time"),
        // A control node: its bound slot IS its face (R3), and it is the
        // same channel the panel's `speed` knob presents.
        controls_child(
            "Master speed",
            "Button",
            "button.json",
            "value → bus:speed",
            UiPanelGroup::new("Master speed", ROOT_SCOPE)
                .with_target(scope_target(ROOT_SCOPE))
                .with_controls(vec![following(
                    knob(ROOT_SCOPE, "speed", "value", 0.62, 0.0, 1.0, None),
                    "0.62",
                    "this node writes it",
                )]),
        ),
        module_child(
            "plasma_1",
            PLASMA_1_SCOPE,
            "effect",
            plasma_face(plasma_read_panel(PLASMA_1_SCOPE), 3.1),
            plasma_children(3.1),
        ),
        module_child(
            "plasma_2",
            PLASMA_2_SCOPE,
            "effect",
            plasma_face(plasma_read_panel(PLASMA_2_SCOPE), 6.5),
            plasma_children(6.5),
        ),
        // The fixture's brightness is the SAME (scope, channel) as the
        // panel's — one control, two views (panel.md P1). Holding it on the
        // panel holds it here too, which is the multi-client rule made
        // visible across two cards in one column.
        controls_child(
            "halo",
            "Fixture",
            "fixture.json",
            "241 LEDs · input ← bus:visual.out",
            UiPanelGroup::new("halo", ROOT_SCOPE)
                .with_target(scope_target(ROOT_SCOPE))
                .with_controls(vec![at_default(
                    fader(ROOT_SCOPE, "brightness", "brightness", 200.0, 255.0),
                    "authored 200",
                )]),
        ),
    ]
}

/// A child card with no face at all — a node whose whole story is its
/// header row (the clock).
fn plain_child(name: &str, kind: &str, detail: &str, summary: &str) -> UiNodeChild {
    let mut child = UiNodeChild::new(name, kind, detail);
    child.summary = Some(summary.to_string());
    child.status = UiStatus::good("Running");
    child
}

/// A child card whose face is a product preview (the plasma shader).
fn product_child(name: &str, kind: &str, detail: &str, summary: &str) -> UiNodeChild {
    plain_child(name, kind, detail, summary)
}

/// A leaf child card whose face is exactly its bound controls (R3). The
/// group carries the ENCLOSING scope — a leaf introduces none of its own.
fn controls_child(
    name: &str,
    kind: &str,
    detail: &str,
    summary: &str,
    controls: UiPanelGroup,
) -> UiNodeChild {
    let mut child = plain_child(name, kind, detail, summary);
    child.face = Some(UiNodeFace::Controls(controls));
    child
}

/// A child module card: the same module face one level in, with its own
/// children hanging below it on the same rail.
fn module_child(
    name: &str,
    scope: &str,
    summary: &str,
    face: UiModuleFace,
    children: Vec<UiNodeChild>,
) -> UiNodeChild {
    let mut child = plain_child(name, "Module", scope, summary);
    child.face = Some(UiNodeFace::Module(face));
    child.children = children;
    child
}

/// The root scope's wiring — what the sidebar bus pane used to show, now
/// hung off the module that owns the scope.
pub(crate) fn root_wiring() -> UiBusView {
    UiBusView {
        channels: vec![
            channel(
                "time",
                "Instant",
                Some("12.44"),
                vec![site("clock", "seconds")],
                vec![site("plasma_1", "time"), site("plasma_2", "time")],
            ),
            channel(
                "speed",
                "Float",
                Some("0.62"),
                vec![site("Master speed", "value")],
                vec![site("plasma_2", "speed")],
            ),
            channel(
                "hue",
                "Float",
                Some("0.41"),
                vec![site("hue lfo", "out")],
                vec![site("plasma_1", "hue")],
            ),
            channel(
                "brightness",
                "Float",
                Some("96"),
                vec![site("panel", "held")],
                vec![site("halo", "brightness")],
            ),
            UiBusChannelView {
                primary_visual: true,
                ..channel(
                    "visual.out",
                    "Color",
                    Some("visual product #5:0"),
                    vec![site("plasma_1", "publish")],
                    vec![site("halo", "input")],
                )
            },
        ],
    }
}

/// One bus channel row for the wiring drawer.
fn channel(
    name: &str,
    kind: &str,
    value: Option<&str>,
    writers: Vec<UiBusSiteView>,
    readers: Vec<UiBusSiteView>,
) -> UiBusChannelView {
    UiBusChannelView {
        // This fixture leaves `scope` unset on purpose: it pins how a row
        // with no structured scope renders (the shape a channel takes
        // before its owning scope resolves).
        scope: None,
        scope_label: None,
        name: name.to_string(),
        kind: Some(kind.to_string()),
        value: value.map(str::to_string),
        value_error: (value.is_none()).then(|| "no writer in any enclosing scope".to_string()),
        primary_visual: false,
        contended: false,
        preview: None,
        gradient: None,
        writers,
        readers,
    }
}

/// One writer/reader site.
fn site(node_label: &str, slot: &str) -> UiBusSiteView {
    UiBusSiteView {
        node_label: node_label.to_string(),
        slot: Some(slot.to_string()),
        origin: UiBusSiteOrigin::Authored,
        publish: false,
        shadowed: false,
        child_scope: None,
        focus: None,
    }
}

/// The root module as a node card view with its children below it — the
/// single top-level workspace card (the flat-root reversal, §5) and the
/// column of sibling cards it heads.
pub(crate) fn root_module_node_view() -> UiNodeView {
    module_node_view(
        "Aurora Sign",
        ROOT_SCOPE,
        "5 nodes · 2 effects",
        root_face(),
    )
    .with_children(root_children())
}

/// The root module view with the story's held controls already engaged.
pub(crate) fn held_root_view() -> UiNodeView {
    PanelWalk::new(root_module_node_view()).with_held(HELD).view
}

/// Any module as a node card view, so the story wears the real card chrome
/// (header, kind, collapse) instead of a mock frame.
pub(crate) fn module_node_view(
    name: &str,
    path: &str,
    summary: &str,
    face: UiModuleFace,
) -> UiNodeView {
    let header = UiNodeHeader::new(name, "Module", path)
        .with_source("module.json")
        .with_status(UiStatus::good("Running"))
        .with_summary(summary);
    let mut view = UiNodeView::new(header, Vec::new()).with_node_id(path);
    view.face = Some(UiNodeFace::Module(face));
    view
}

// -------------------------------------------------------------- playlist

/// The E2 playlist: two entries binding one `speed` channel with different
/// meta. Each entry is its own sink scope, so their panel state never
/// mixes.
pub(crate) fn playlist_face() -> UiPlaylistFace {
    UiPlaylistFace {
        entries: vec![
            UiPlaylistEntry {
                key: 0,
                name: "Drift".to_string(),
                duration_ms: Some(180_000),
                cue: false,
                thumb: Some(aurora_preview(18, 10, 3.1)),
                action: None,
            },
            UiPlaylistEntry {
                key: 1,
                name: "Whirl".to_string(),
                duration_ms: Some(240_000),
                cue: false,
                thumb: Some(aurora_preview(18, 10, 6.5)),
                action: None,
            },
        ],
        active: Some(0),
    }
}

/// The active entry's sink scope path. Sink scopes are anonymous in the
/// model (R2); the fixtures name them after the entry's own child node so
/// they are legible on screen and parse as node paths.
pub(crate) fn entry_scope(entry: u32) -> String {
    let child = if entry == 0 { "drift" } else { "whirl" };
    format!("/aurora.module/set.playlist/{child}.shader")
}

/// The active entry's panel. The control is re-derived from whichever slot
/// is bound in the ACTIVE sink scope (R9), so switching entries swaps the
/// label, the range, and the widget's whole feel — same channel name,
/// different control.
pub(crate) fn entry_panel(entry: u32) -> UiPanelGroup {
    let scope = entry_scope(entry);
    let (label, value, max) = match entry {
        0 => ("Drift", 0.5_f32, 1.0_f32),
        _ => ("Whirl", 6.5_f32, 10.0_f32),
    };
    UiPanelGroup::new(label, scope.clone())
        .with_target(scope_target(&scope))
        .with_controls(vec![at_default(
            knob(&scope, "speed", label, value, 0.0, max, None),
            "authored default",
        )])
}

/// Entry A's panel with its knob already held at 0.35 — the tweak that has
/// to survive switching to Whirl and back, because the two entries are two
/// sink scopes and therefore two identities (P1).
pub(crate) fn entry_held_panel(entry: u32) -> UiPanelGroup {
    let mut panel = entry_panel(entry);
    if entry == 0 {
        engage_group(&mut panel, &entry_scope(entry), "speed", 0.35);
    }
    panel
}

// ---------------------------------------------------- three-state fixture

/// One panel holding exactly the three states, side by side, for the
/// P-Q2 comparison.
pub(crate) fn three_state_panel() -> UiPanelGroup {
    UiPanelGroup::new("Panel states", ROOT_SCOPE)
        .with_target(scope_target(ROOT_SCOPE))
        .with_controls(vec![
            at_default(
                knob(
                    ROOT_SCOPE,
                    "palette",
                    "at default",
                    2.0,
                    1.0,
                    4.0,
                    Some(1.0),
                ),
                "nothing writes it",
            ),
            following(
                knob(ROOT_SCOPE, "hue", "following", 0.41, 0.0, 1.0, None),
                "0.41",
                "lfo · hue",
            ),
            engaged(
                knob(ROOT_SCOPE, "speed", "engaged", 0.82, 0.0, 1.0, None),
                "lfo · speed",
            ),
            at_default(
                toggle(ROOT_SCOPE, "mirror", "at default", false),
                "nothing writes it",
            ),
            {
                let mut view = toggle(ROOT_SCOPE, "beat", "following", false);
                view.control.live_value = Some("true".to_string());
                view.with_state(UiPanelControlState::ReadFollowing, Some("audio · beat"))
            },
            engaged(
                toggle(ROOT_SCOPE, "strobe", "engaged", true),
                "audio · beat",
            ),
            at_default(
                fader(ROOT_SCOPE, "brightness", "at default", 200.0, 255.0),
                "nothing writes it",
            ),
            following(
                fader(ROOT_SCOPE, "master", "following", 180.0, 255.0),
                "180",
                "dimmer · out",
            ),
            engaged(
                fader(ROOT_SCOPE, "held", "engaged", 96.0, 255.0),
                "authored 200",
            ),
        ])
}

// -------------------------------------------------------- walkable state

/// Live fixture state for the walkable stories.
///
/// The dev-server walk is the point: turning a knob must actually engage
/// it, and resetting must actually let go. This holds the story node view — the card's face AND the sibling child cards below it —
/// plus a pristine baseline, applies the widget's own
/// `SlotEditOp::SetValue` dispatch as a panel write, and applies
/// [`PanelGesture`]s as clears.
///
/// The whole view is the unit rather than the face, because a control's
/// identity is `(scope, channel)` (panel.md P1) and the same control now
/// genuinely appears on two different cards — the module's panel, and the
/// leaf child's own face below it. Holding one has to move the other, and
/// that only works if the walk covers the tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PanelWalk {
    /// The node view the stories render.
    pub view: UiNodeView,
    /// The untouched view, for restoring cleared controls.
    baseline: UiNodeView,
}

impl PanelWalk {
    /// Wrap a **Read-form** node view as walkable story state. The view
    /// passed in becomes the clear-target, so it must have nothing engaged.
    pub fn new(view: UiNodeView) -> Self {
        Self {
            baseline: view.clone(),
            view,
        }
    }

    /// The root module's face, for the panel-only stories.
    pub fn face(&self) -> UiModuleFace {
        let Some(UiNodeFace::Module(face)) = self.view.face.clone() else {
            unreachable!("the walk's root view wears a module face")
        };
        face
    }

    /// Pre-engage controls, exactly as if they had been touched — same
    /// transition a live drag makes, so what a reset lands on is the same
    /// Read form either way.
    pub fn with_held(mut self, held: &[(&str, &str, f32)]) -> Self {
        for (scope, channel, value) in held {
            let (scope, channel, value) = (*scope, *channel, *value);
            visit_view_controls(&mut self.view, &mut |control_scope, control| {
                if control_scope == scope && control.channel == channel {
                    hold(control, value);
                }
            });
        }
        self
    }

    /// Apply a widget dispatch: the first touch of a control materializes
    /// its panel writer and captures the channel (panel.md P2 — Latch, not
    /// Touch), and every later drag just moves the held value.
    pub fn apply_action(&mut self, action: &UiAction) {
        let Some(SlotEditOp::SetValue { address, value }) = action.op_as::<SlotEditOp>() else {
            return;
        };
        let (address, value) = (address.clone(), value.clone());
        visit_view_controls(&mut self.view, &mut |_, view| {
            if view.control.address.as_ref() != Some(&address) {
                return;
            }
            view.control.value = match &value {
                LpValue::Bool(next) => UiSlotValue::bool(*next),
                LpValue::F32(next) => UiSlotValue::f32(*next),
                _ => view.control.value.clone(),
            };
            if !view.state.engaged() {
                // First touch: the writer materializes here, in the scope
                // where the control lives, and shadows whatever was
                // driving the channel.
                view.source = view.source.clone().or_else(|| Some("project".to_string()));
                view.state = UiPanelControlState::Engaged;
            }
        });
    }

    /// Apply a panel gesture: clears restore Read, disclosure toggles the
    /// group, and auto-save flips the persistence flag.
    pub fn apply_gesture(&mut self, gesture: &PanelGesture) {
        match gesture {
            PanelGesture::SetAutoSave(next) => {
                if let Some(UiNodeFace::Module(face)) = self.view.face.as_mut() {
                    face.auto_save = Some(*next);
                }
            }
            PanelGesture::ClearControl { target } => {
                let scope = scope_display(&target.scope);
                self.restore(|group_scope, view| {
                    group_scope == scope && view.channel == target.channel
                });
            }
            // "Reset means reset": the clear descends into nested groups
            // (panel.md P-Q4's lean), which is why a scope prefix match is
            // the right test rather than equality.
            PanelGesture::ClearScope { scope } => {
                let scope = scope_display(&scope);
                self.restore(|group_scope, _| group_scope.starts_with(scope));
            }
        }
    }

    /// Copy matching controls back from the pristine baseline.
    fn restore(&mut self, matches: impl Fn(&str, &UiPanelControlView) -> bool) {
        let mut pristine = Vec::new();
        visit_view_controls(&mut self.baseline.clone(), &mut |scope, view| {
            pristine.push((scope.to_string(), view.clone()));
        });
        visit_view_controls(&mut self.view, &mut |scope, view| {
            if !matches(scope, view) {
                return;
            }
            if let Some((_, original)) = pristine.iter().find(|(other, candidate)| {
                other.as_str() == scope && candidate.channel == view.channel
            }) {
                *view = original.clone();
            }
        });
    }
}

/// Engage one control everywhere it appears within one panel group tree.
/// The same `(scope, channel)` is ONE control (panel.md P1).
fn engage_group(group: &mut UiPanelGroup, scope: &str, channel: &str, value: f32) {
    visit_group_controls(group, &mut |control_scope, view| {
        if control_scope == scope && view.channel == channel {
            hold(view, value);
        }
    });
}

/// Materialize one control's panel writer at `value` — the Read → Latch
/// transition (P2). The Read caption is kept, because that is what the
/// engaged control displaced and what a clear restores.
fn hold(view: &mut UiPanelControlView, value: f32) {
    view.control.value = UiSlotValue::f32(value);
    view.state = UiPanelControlState::Engaged;
    if let Some(target) = view.control.panel_target.as_mut() {
        target.engaged = true;
    }
}

/// Visit every panel control in a card and everything below it, carrying
/// each control's owning scope.
fn visit_view_controls(
    view: &mut UiNodeView,
    visit: &mut impl FnMut(&str, &mut UiPanelControlView),
) {
    visit_face_controls(view.face.as_mut(), visit);
    for child in &mut view.children {
        visit_child_controls(child, visit);
    }
}

/// The same walk, for a child card and its own children.
fn visit_child_controls(
    child: &mut UiNodeChild,
    visit: &mut impl FnMut(&str, &mut UiPanelControlView),
) {
    visit_face_controls(child.face.as_mut(), visit);
    for nested in &mut child.children {
        visit_child_controls(nested, visit);
    }
}

/// The panel a face owns, if it owns one: a module's panel, or a leaf's
/// bound controls. Every other kind of face has none.
fn face_panel(face: Option<&mut UiNodeFace>) -> Option<&mut UiPanelGroup> {
    match face? {
        UiNodeFace::Module(module) => Some(&mut module.panel),
        UiNodeFace::Controls(group) => Some(group),
        _ => None,
    }
}

/// Walk one face's panel, nested groups included.
fn visit_face_controls(
    face: Option<&mut UiNodeFace>,
    visit: &mut impl FnMut(&str, &mut UiPanelControlView),
) {
    if let Some(panel) = face_panel(face) {
        visit_group_controls(panel, visit);
    }
}

/// Walk one group and its nested groups.
fn visit_group_controls(
    group: &mut UiPanelGroup,
    visit: &mut impl FnMut(&str, &mut UiPanelControlView),
) {
    let scope = group.scope.clone();
    for view in &mut group.controls {
        visit(&scope, view);
    }
    for nested in &mut group.groups {
        visit_group_controls(nested, visit);
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiNodeFace, UiPanelControlState};

    use super::{
        HELD, PLASMA_1_SCOPE, PLASMA_2_SCOPE, PanelWalk, ROOT_SCOPE, held_root_face,
        root_module_node_view, scope_target,
    };
    use crate::app::module::PanelGesture;

    fn walk() -> PanelWalk {
        PanelWalk::new(root_module_node_view()).with_held(HELD)
    }

    #[test]
    fn the_two_plasma_instances_are_independent_groups() {
        let panel = held_root_face().panel;
        assert_eq!(panel.groups.len(), 2);
        assert_eq!(panel.groups[0].scope, PLASMA_1_SCOPE);
        assert_eq!(panel.groups[1].scope, PLASMA_2_SCOPE);
        // E4: plasma_1 was touched and detached; plasma_2 still follows,
        // even though the two groups are otherwise identical.
        assert_eq!(panel.groups[0].engaged_total(), 1);
        assert_eq!(panel.groups[1].engaged_total(), 0);
        assert_eq!(
            panel.groups[1].controls[0].state,
            UiPanelControlState::ReadFollowing
        );
    }

    #[test]
    fn clearing_a_scope_descends_into_its_nested_groups() {
        let mut walk = walk();
        // brightness (root) + plasma_1's speed are held.
        assert_eq!(walk.face().panel.engaged_total(), 2);

        walk.apply_gesture(&PanelGesture::ClearScope {
            scope: scope_target(ROOT_SCOPE),
        });

        assert_eq!(
            walk.face().panel.engaged_total(),
            0,
            "reset means reset — the nested plasma writer goes too"
        );
    }

    #[test]
    fn clearing_one_control_leaves_its_siblings_alone() {
        let mut walk = walk();
        walk.apply_gesture(&PanelGesture::ClearControl {
            target: lpa_studio_core::UiPanelTarget {
                scope: scope_target(PLASMA_1_SCOPE),
                channel: "speed".to_string(),
                engaged: true,
            },
        });

        assert_eq!(walk.face().panel.groups[0].engaged_total(), 0);
        assert_eq!(
            walk.face().panel.engaged_here(),
            1,
            "the root's own held brightness is untouched"
        );
        // Clearing restores the Read form, not just the state flag: the
        // control falls back into whatever was driving the channel.
        assert_eq!(
            walk.face().panel.groups[0].controls[0].state,
            UiPanelControlState::ReadFollowing
        );
    }

    #[test]
    fn auto_save_is_a_view_gesture() {
        let mut walk = walk();
        assert_eq!(walk.face().auto_save, Some(true));
        walk.apply_gesture(&PanelGesture::SetAutoSave(false));
        assert_eq!(walk.face().auto_save, Some(false));
    }

    /// Groups are bordered clusters in a wrapping row, never folded away —
    /// so their labels have to carry the instance identity that the scope
    /// path used to.
    #[test]
    fn side_by_side_groups_are_told_apart_by_their_labels() {
        let panel = held_root_face().panel;
        assert_eq!(panel.groups[0].label, "plasma 1");
        assert_eq!(panel.groups[1].label, "plasma 2");
        assert_ne!(panel.groups[0].label, panel.groups[1].label);
        // And the path is still reachable — in the heading's popup.
        let aspects = panel.groups[0].detail_aspects();
        assert!(
            aspects
                .iter()
                .any(|aspect| aspect.rows.iter().any(|row| row.value == PLASMA_1_SCOPE))
        );
    }

    /// The G2 revision-1 claim: children are sibling cards under the module
    /// card, not sections inside its face — and every one of them renders,
    /// with no active-child filtering the way a playlist has.
    #[test]
    fn children_hang_below_the_card_not_inside_the_face() {
        let view = root_module_node_view();
        assert_eq!(view.children.len(), 5);
        // An embedded module keeps its own children on the same rail, one
        // level down: the nesting grammar does not change with depth.
        let plasma = &view.children[2];
        assert_eq!(plasma.label, "plasma_1");
        assert!(matches!(plasma.face, Some(UiNodeFace::Module(_))));
        assert_eq!(plasma.children.len(), 1);
    }

    /// A leaf's controls and the module panel's are ONE control (P1), even
    /// though they now live on two different cards: holding it on the panel
    /// has to move the fixture card below.
    #[test]
    fn one_control_two_cards_move_together() {
        let walk = walk();
        let Some(UiNodeFace::Controls(halo)) = &walk.view.children[4].face else {
            panic!("the fixture child wears a controls face");
        };
        assert_eq!(halo.controls[0].channel, "brightness");
        assert_eq!(halo.controls[0].state, UiPanelControlState::Engaged);
        assert_eq!(
            halo.controls[0].control.value.display,
            walk.face().panel.controls[0].control.value.display,
            "the panel and the fixture card show the same held value"
        );
    }
}
