//! Mock DTOs for the M2 UX spike.
//!
//! Every fixture here is hand-built. The engine has no scopes, no panel
//! writers, and no `PanelWrite`/`PanelClear` ops (M4 owns all three), so
//! the spike fakes the states the design docs describe and the stories walk
//! them. The content is not arbitrary: it reproduces the worked examples
//! from `docs/design/modules.md` so the gate is judging the real shapes.
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
    UiAction, UiBusChannelView, UiBusSiteView, UiBusView, UiModuleChild, UiModuleFace, UiNodeFace,
    UiNodeHeader, UiNodeView, UiPanelControl, UiPanelControlState, UiPanelControlView,
    UiPanelGroup, UiPanelWidget, UiPlaylistEntry, UiPlaylistFace, UiProducedProduct,
    UiProductPreviewFrame, UiProductTrackingState, UiSlotFieldState, UiSlotValue, UiStatus,
};

use crate::app::node::face_story_fixtures::aurora_preview;

use super::PanelGesture;

/// The root module's scope path. Scope paths are node paths (§6), so the
/// root module's is the root itself.
pub(crate) const ROOT_SCOPE: &str = "/aurora.module";
/// The two embedded plasma instances' scopes — different scopes is exactly
/// why their controls are independent (R8).
pub(crate) const PLASMA_1_SCOPE: &str = "/aurora.module/plasma_1.module";
pub(crate) const PLASMA_2_SCOPE: &str = "/aurora.module/plasma_2.module";

/// A story-only slot address, so the widgets render wired and their drags
/// dispatch into the story's own handler.
fn spike_address(node: &str, slot: &str) -> ProjectSlotAddress {
    ProjectSlotAddress::new(
        ProjectNodeAddress::parse(node).expect("valid spike node address"),
        ProjectSlotRoot::def(),
        SlotPath::parse(slot).expect("valid spike slot path"),
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
            label: label.to_string(),
            address: Some(spike_address(scope, channel)),
            widget: UiPanelWidget::Knob { min, max, step },
            value: UiSlotValue::f32(value),
            live_value: None,
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
            label: label.to_string(),
            address: Some(spike_address(scope, channel)),
            widget: UiPanelWidget::Fader {
                min: 0.0,
                max,
                step: Some(1.0),
            },
            value: UiSlotValue::f32(value),
            live_value: None,
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
            label: label.to_string(),
            address: Some(spike_address(scope, channel)),
            widget: UiPanelWidget::Toggle,
            value: UiSlotValue::bool(value),
            live_value: None,
            unit: None,
            state: UiSlotFieldState::editable(),
            aspects: Vec::new(),
        },
    )
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
    UiPanelGroup::new("plasma", scope).with_controls(vec![
        speed,
        at_default(
            knob(scope, "hue", "hue", 0.32, 0.0, 1.0, None),
            "authored default",
        ),
    ])
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
/// wears, one level in.
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
        children: vec![
            UiModuleChild::leaf("sim", "Shader")
                .with_summary("visual → bus:visual.out")
                .with_preview(
                    UiProducedProduct::visual("output")
                        .with_tracking(UiProductTrackingState::Tracking)
                        .with_frame(UiProductPreviewFrame::new(16, 5))
                        .with_preview(aurora_preview(48, 15, seed + 0.4)),
                ),
        ],
        provenance: Some("PhotomancerArt · v1.2 · CC0-1.0".to_string()),
        auto_save: true,
    }
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
/// what most stories render, and what [`PanelSpike`] clears back from.
pub(crate) fn held_root_face() -> UiModuleFace {
    PanelSpike::new(root_face()).with_held(HELD).face
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
        children: root_children(),
        provenance: Some("Yona · v0.4 · created 2026-07-31".to_string()),
        auto_save: true,
    }
}

/// The root module's children, nested inside the card: two leaves that
/// write host channels, the two embedded plasma modules, and the fixture.
pub(crate) fn root_children() -> Vec<UiModuleChild> {
    vec![
        UiModuleChild::leaf("clock", "Clock").with_summary("seconds → bus:time"),
        UiModuleChild::leaf("Master speed", "Button")
            .with_summary("value → bus:speed")
            .with_controls(vec![following(
                knob(ROOT_SCOPE, "speed", "value", 0.62, 0.0, 1.0, None),
                "0.62",
                "this node writes it",
            )]),
        UiModuleChild::module(
            "plasma_1",
            plasma_face(plasma_read_panel(PLASMA_1_SCOPE), 3.1),
        )
        .with_summary("effect")
        .collapsed(),
        UiModuleChild::module(
            "plasma_2",
            plasma_face(plasma_read_panel(PLASMA_2_SCOPE), 6.5),
        )
        .with_summary("effect")
        .collapsed(),
        // The fixture's brightness is the SAME (scope, channel) as the
        // panel's — one control, two views (panel.md P1). Holding it on the
        // panel holds it here too, which is the multi-client rule made
        // visible inside a single card.
        UiModuleChild::leaf("halo", "Fixture")
            .with_summary("241 LEDs · input ← bus:visual.out")
            .with_controls(vec![at_default(
                fader(ROOT_SCOPE, "brightness", "brightness", 200.0, 255.0),
                "authored 200",
            )]),
    ]
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
        name: name.to_string(),
        kind: Some(kind.to_string()),
        value: value.map(str::to_string),
        value_error: (value.is_none()).then(|| "no writer in any enclosing scope".to_string()),
        primary_visual: false,
        writers,
        readers,
    }
}

/// One writer/reader site.
fn site(node_label: &str, slot: &str) -> UiBusSiteView {
    UiBusSiteView {
        node_label: node_label.to_string(),
        slot: Some(slot.to_string()),
        default_origin: false,
        focus: None,
    }
}

/// The root module as a node card view — the single top-level workspace
/// card (the flat-root reversal, §5).
pub(crate) fn root_module_node_view() -> UiNodeView {
    module_node_view(
        "Aurora Sign",
        ROOT_SCOPE,
        "5 nodes · 2 effects",
        root_face(),
    )
}

/// Any module as a node card view, so the spike wears the real card chrome
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
/// model (R2); the spike names them after the entry's own child node so
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
    UiPanelGroup::new(label, scope.clone()).with_controls(vec![at_default(
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
    UiPanelGroup::new("Panel states", ROOT_SCOPE).with_controls(vec![
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
/// The dev-server walk is the point of the spike: turning a knob must
/// actually engage it, and resetting must actually let go. This holds the
/// mock face plus a pristine baseline, applies the widget's own
/// `SlotEditOp::SetValue` dispatch as a panel write, and applies
/// [`PanelGesture`]s as clears.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PanelSpike {
    /// The face the stories render.
    pub face: UiModuleFace,
    /// The untouched face, for restoring cleared controls.
    baseline: UiModuleFace,
}

impl PanelSpike {
    /// Wrap a **Read-form** face as walkable spike state. The face passed
    /// in becomes the clear-target, so it must have nothing engaged.
    pub fn new(face: UiModuleFace) -> Self {
        Self {
            baseline: face.clone(),
            face,
        }
    }

    /// Pre-engage controls, exactly as if they had been touched — same
    /// transition a live drag makes, so what a reset lands on is the same
    /// Read form either way.
    pub fn with_held(mut self, held: &[(&str, &str, f32)]) -> Self {
        for (scope, channel, value) in held {
            engage_face(&mut self.face, scope, channel, *value);
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
        visit_controls(&mut self.face, &mut |view| {
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
            PanelGesture::SetAutoSave(next) => self.face.auto_save = *next,
            PanelGesture::ToggleGroup { scope } => {
                visit_groups(&mut self.face, &mut |group| {
                    if group.scope == *scope {
                        group.collapsed = !group.collapsed;
                    }
                });
            }
            PanelGesture::ClearControl { scope, channel } => {
                self.restore(|group_scope, view| group_scope == scope && view.channel == *channel);
            }
            // "Reset means reset": the clear descends into nested groups
            // (panel.md P-Q4's lean), which is why a scope prefix match is
            // the right test rather than equality.
            PanelGesture::ClearScope { scope } => {
                self.restore(|group_scope, _| group_scope.starts_with(scope.as_str()));
            }
        }
    }

    /// Copy matching controls back from the pristine baseline.
    fn restore(&mut self, matches: impl Fn(&str, &UiPanelControlView) -> bool) {
        let mut pristine = Vec::new();
        visit_controls_scoped(&mut self.baseline.clone(), &mut |scope, view| {
            pristine.push((scope.to_string(), view.clone()));
        });
        visit_controls_scoped(&mut self.face, &mut |scope, view| {
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

/// Engage one control everywhere it appears on a face: the same
/// `(scope, channel)` is ONE control (panel.md P1), so a nested panel group
/// and the child card that repeats it move together.
fn engage_face(face: &mut UiModuleFace, scope: &str, channel: &str, value: f32) {
    visit_controls_scoped(face, &mut |control_scope, view| {
        if control_scope == scope && view.channel == channel {
            hold(view, value);
        }
    });
}

/// The same, within one panel group tree.
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
}

/// Visit every control on a face — the module's own panel, its nested
/// groups, its leaf children's controls, and every nested module face.
fn visit_controls(face: &mut UiModuleFace, visit: &mut impl FnMut(&mut UiPanelControlView)) {
    visit_controls_scoped(face, &mut |_, view| visit(view));
}

/// The same walk, carrying each control's owning scope.
fn visit_controls_scoped(
    face: &mut UiModuleFace,
    visit: &mut impl FnMut(&str, &mut UiPanelControlView),
) {
    visit_group_controls(&mut face.panel, visit);
    let scope = face.panel.scope.clone();
    for child in &mut face.children {
        for view in &mut child.controls {
            visit(&scope, view);
        }
        if let Some(module) = child.module.as_mut() {
            visit_controls_scoped(module, visit);
        }
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

/// Visit every panel group on a face, including nested module faces.
fn visit_groups(face: &mut UiModuleFace, visit: &mut impl FnMut(&mut UiPanelGroup)) {
    visit_group(&mut face.panel, visit);
    for child in &mut face.children {
        if let Some(module) = child.module.as_mut() {
            visit_groups(module, visit);
        }
    }
}

/// Walk one group and its nested groups.
fn visit_group(group: &mut UiPanelGroup, visit: &mut impl FnMut(&mut UiPanelGroup)) {
    visit(group);
    for nested in &mut group.groups {
        visit_group(nested, visit);
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::UiPanelControlState;

    use super::{
        HELD, PLASMA_1_SCOPE, PLASMA_2_SCOPE, PanelSpike, ROOT_SCOPE, held_root_face, root_face,
    };
    use crate::app::module::PanelGesture;

    fn spike() -> PanelSpike {
        PanelSpike::new(root_face()).with_held(HELD)
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
        let mut spike = spike();
        // brightness (root) + plasma_1's speed are held.
        assert_eq!(spike.face.panel.engaged_total(), 2);

        spike.apply_gesture(&PanelGesture::ClearScope {
            scope: ROOT_SCOPE.to_string(),
        });

        assert_eq!(
            spike.face.panel.engaged_total(),
            0,
            "reset means reset — the nested plasma writer goes too"
        );
    }

    #[test]
    fn clearing_one_control_leaves_its_siblings_alone() {
        let mut spike = spike();
        spike.apply_gesture(&PanelGesture::ClearControl {
            scope: PLASMA_1_SCOPE.to_string(),
            channel: "speed".to_string(),
        });

        assert_eq!(spike.face.panel.groups[0].engaged_total(), 0);
        assert_eq!(
            spike.face.panel.engaged_here(),
            1,
            "the root's own held brightness is untouched"
        );
        // Clearing restores the Read form, not just the state flag: the
        // control falls back into whatever was driving the channel.
        assert_eq!(
            spike.face.panel.groups[0].controls[0].state,
            UiPanelControlState::ReadFollowing
        );
    }

    #[test]
    fn auto_save_and_group_disclosure_are_view_gestures() {
        let mut spike = spike();
        assert!(spike.face.auto_save);
        spike.apply_gesture(&PanelGesture::SetAutoSave(false));
        assert!(!spike.face.auto_save);

        assert!(!spike.face.panel.groups[1].collapsed);
        spike.apply_gesture(&PanelGesture::ToggleGroup {
            scope: PLASMA_2_SCOPE.to_string(),
        });
        assert!(spike.face.panel.groups[1].collapsed);
    }
}
