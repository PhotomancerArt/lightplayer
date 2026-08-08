//! Kind-specific node-card face derivation (node-card P3).
//!
//! Faces are built FROM the already-projected section DTOs — the same rows,
//! previews, edit states, and inline editors the generic sections view
//! renders — so a panel control and its backing slot row can never disagree
//! (one derivation, two presentations). The builder never re-reads slot
//! controllers or project state.
//!
//! **Q13 — binding is publicity.** "On the panel" means "bound to a bus
//! channel": there is no authored panel FLAG any more (the legacy
//! `ShaderSlotDef.panel` / `SlotMeta.panel` pair is deleted). A control
//! reaches a panel by carrying a `panel_target`, which the project walk
//! derives from the binding itself.
//!
//! Publicity is **authored** wiring — plus one additive override: a slot
//! whose declaration carries `panel = "show"` beside its `default_bind`
//! is public through that default wiring too (the fixture's brightness
//! fader). See [`public_panel_target`].
//!
//! - **Shader**: preview = the produced visual product; controls = consumed
//!   uniform slots BOUND to a bus channel, as knobs over the authored
//!   `min`/`max` editing `consumed.<name>.default.some`, snapping to the
//!   uniform's step ([`lpc_model::shader_panel_step`] — authored, else 1
//!   for an i32/u32 shape); a bound uniform with no min/max meta still gets
//!   the default 0..1 knob. The code drawer reuses the inline GLSL asset
//!   editor. The agent handle is decorated later by the studio controller
//!   (`AgentController::decorate_editor_view`), exactly like the sections
//!   path.
//! - **Fixture**: preview = the produced control product (lamp map);
//!   brightness = the `brightness` row as the dominant fader editing
//!   `brightness.some`. That fader is the fixture FACE's own affordance,
//!   named outright rather than flagged; it joins the enclosing module's
//!   panel only when brightness is itself wired to a channel.
//! - **Playlist**: face = the ENTRIES strip only ({entries, active} — P2c
//!   item 2); entries from the def's `entries` map rows (authored name,
//!   `duration` seconds → ms chip, `trigger_ids` non-empty → cue tag),
//!   `active` from the produced `PlaylistState.active_entry` row. Entry
//!   thumbs/click actions reuse the already-built child DTOs (visual
//!   preview snapshot, node-select action). Deriving the face ALSO
//!   enforces the sibling invariant: the children list keeps ONLY the
//!   active entry's child (see [`kind_face`]).
//! - **Output**: face = one row per authored `channels[k]` wire (endpoint,
//!   pin label parsed out of the endpoint spec, authored count, derived
//!   slice start) plus the slot addresses the web edits through. Everything
//!   the builder cannot see from this node's sections — the running device's
//!   BOARD, and the incoming lamp extent that resolves the remainder
//!   channel — is filled afterwards by the studio controller's decoration
//!   pass (`app::studio::output_face_decoration`), the same shape as the
//!   shader face's agent handle.
//! - Every other kind returns `None` — the card keeps today's generic
//!   sections.

use lpc_model::{
    ClockDef, FixtureDef, HwEndpointSpec, LpValue, OutputDef, PlaylistDef, ShaderDef,
    ShaderValueShapeRef, shader_panel_step,
};

use crate::app::project::format_lp_value;
use crate::app::project::node::node_space_section;
use crate::{
    ControllerId, PlaylistActivateOp, ProjectController, ProjectNodeAddress, ProjectSlotAddress,
    UiAction, UiAssetEditor, UiAssetEditorKind, UiConfigSlot, UiConfigSlotBody, UiFixtureFace,
    UiFixturePower, UiNodeChild, UiNodeFace, UiNodeSection, UiOutputChannelRow, UiOutputFace,
    UiPanelControl, UiPanelWidget, UiPlaylistEntry, UiPlaylistFace, UiProducedProduct,
    UiProductKind, UiProductPreview, UiShaderFace, UiSlotAspect, UiSlotAspectKind,
    UiSlotEditorHint, UiSlotSourceState, UiSlotValue, UiSlotValueKind,
};

/// Build the kind-specific face for a node's card from its projected
/// sections and already-built child DTOs. `ty` is the raw tree discriminant
/// (`ShaderDef::KIND`, …).
///
/// **The playlist arm filters `children` in place** — this is the "one live
/// surface" rule (P4): while the strip face is up, the strip represents
/// every entry, so only the ACTIVE entry's child renders as a sibling card
/// below the playlist card. The filter rides the same derivation as the
/// face so the two can never disagree; when the face does not derive
/// (missing entries row, no `active_entry` status yet, active entry's
/// child not mounted), `children` is left untouched and the card falls
/// back to today's full rendering. An **empty** entries map still derives
/// a face — the strip's empty state (with its add affordance) is the
/// card's surface for a freshly created playlist — with no child
/// filtering, since there are no entry children to filter.
pub(in crate::app::project) fn kind_face(
    ty: &str,
    address: &ProjectNodeAddress,
    sections: &[UiNodeSection],
    children: &mut Vec<UiNodeChild>,
    status_detail: Option<&str>,
) -> Option<UiNodeFace> {
    match ty {
        ShaderDef::KIND => shader_face(sections, status_detail).map(UiNodeFace::Shader),
        FixtureDef::KIND => fixture_face(sections).map(UiNodeFace::Fixture),
        PlaylistDef::KIND => {
            let (face, active_child) = playlist_face(address, sections, children)?;
            if let Some(index) = active_child {
                let active = children.swap_remove(index);
                children.clear();
                children.push(active);
            }
            // NOTE: the surviving child does NOT set `UiNodeChild::active`
            // — the web maps that flag onto the pane's *selection* look
            // (`focused || active`, a story-fixture affordance), and the
            // active entry is not thereby the Studio selection. The strip's
            // ACTIVE placard is the active-ness presentation.
            Some(UiNodeFace::Playlist(face))
        }
        OutputDef::KIND => output_face(sections).map(UiNodeFace::Output),
        ClockDef::KIND => clock_face(sections).map(UiNodeFace::Clock),
        // Unknown kinds stay on the generic fallback permanently.
        _ => None,
    }
}

/// The clock card's face: the published time product, the transport
/// instrument (run/pause, rate, scrub, probe-anchored seconds), and (after
/// the decoration pass) the per-reading trace cards riding this timebase —
/// parent D10, reshaped by clock-face v2 and the tape-hero plan's P2.
///
/// `None` — generic-sections fallback — when the node publishes no time
/// product row, which is the state of a clock whose runtime state has not
/// landed yet. The trace cards, and the transport block's numeric
/// `seconds`, are deliberately NOT derivable here: they live in the
/// engine's timebase store, not in any slot, so they arrive through
/// `ProjectController::apply_clock_faces` exactly the way the output
/// face's board facts do.
fn clock_face(sections: &[UiNodeSection]) -> Option<crate::UiClockFace> {
    let product = product_of_kind(sections, UiProductKind::Time)?;
    let mut face = crate::UiClockFace::new(product);
    face.transport = clock_transport(sections);
    if let Some(transport) = face.transport.as_ref() {
        face.controls = clock_transport_control(sections, transport)
            .into_iter()
            .collect();
    }
    Some(face)
}

/// The clock's transport block, lifted from the flattened `transport.*`
/// Debug rows (D4: `SlotController::collect_config` flattens every Debug
/// field to a top-level row regardless of the record that declared it, so
/// there is no `transport` record row to descend into — three sibling rows
/// keyed by their full path).
///
/// Value + address + editability ride straight off each row exactly as the
/// panel controls read them (`row_edit_address`, `UiSlotFieldState
/// ::editable`) — staged edits flow through the same edit-buffer join the
/// rows already carry, so the DTO reflects an in-flight drag immediately
/// (the echo-suppression contract the tape widgets rely on, P3/P4).
///
/// `None` when the three rows have not landed (unread project — the Debug
/// section is absent, or a differently-shaped clock). Numeric `seconds`
/// starts at `0.0`; only `ProjectController::apply_clock_faces` can fill it
/// in, from the cached timebase probe.
///
/// **A wired dimension shows its LIVE reading** (P8, the same rule
/// [`crate::UiPanelControl::shown_display`] follows): once a leaf's
/// gestures are panel writes on its `clock.*` channel, the slot's authored
/// default is no longer what the transport is doing, and the channel's
/// reading — echoed locally the instant a write is dispatched — is. An
/// UNWIRED dimension keeps the staged slot value, which is what its
/// slot-edit gestures move.
fn clock_transport(sections: &[UiNodeSection]) -> Option<crate::UiClockTransport> {
    let rows = debug_rows(sections);
    let play_state_row = rows
        .iter()
        .find(|row| row.key == TRANSPORT_PLAY_STATE_ROW)?;
    let rate_row = rows.iter().find(|row| row.key == TRANSPORT_RATE_ROW)?;
    let scrub_row = rows.iter().find(|row| row.key == TRANSPORT_SCRUB_ROW)?;
    Some(crate::UiClockTransport {
        seconds: 0.0,
        play_state: live_play_state(play_state_row).or_else(|| row_play_state(play_state_row))?,
        rate: live_f32(rate_row).or_else(|| row_f32(rate_row))?,
        scrub_offset_seconds: live_f32(scrub_row).or_else(|| row_f32(scrub_row))?,
        play_state_address: editable_row_address(play_state_row),
        rate_address: editable_row_address(rate_row),
        scrub_address: editable_row_address(scrub_row),
        play_state_override: row_override(play_state_row),
        rate_override: row_override(rate_row),
        scrub_override: row_override(scrub_row),
    })
}

/// The flattened Debug row each transport leaf lands on (D4: a Debug field
/// inside a record becomes a top-level row keyed by its full path).
const TRANSPORT_PLAY_STATE_ROW: &str = "transport.play_state";
/// See [`TRANSPORT_PLAY_STATE_ROW`].
const TRANSPORT_RATE_ROW: &str = "transport.rate";
/// See [`TRANSPORT_PLAY_STATE_ROW`].
const TRANSPORT_SCRUB_ROW: &str = "transport.scrub_offset_seconds";

/// The clock's ONE grouped panel control (P8): the whole tape transport —
/// fader, run/pause, scrub strip — as a single control on the enclosing
/// module's panel, derived from the model-declared grouping.
///
/// The `transport` record carries `panel = "show"`
/// ([`lpc_model::CLOCK_TRANSPORT_SHAPE_NAME`]) and its three leaves each
/// declare a `clock.*` `default_bind`, so the promotion reaches every leaf
/// endpoint. That is what this arm reads — one match arm on the clock kind,
/// deliberately, not a shape→widget registry: there is exactly one grouped
/// widget, and a match that has to be widened is a better signal than a
/// lookup that silently accepts anything.
///
/// Three facts, three different rules (settled 2026-08-07):
///
/// - **Rendering is a shape fact.** The faceplate is whole whatever the
///   wiring says; nothing here subtracts a dimension from it.
/// - **Membership is a wiring fact.** `None` — no panel presence at all —
///   unless at least one leaf is panel-public.
/// - **Dispatch is a per-leaf fact.** Each [`crate::UiPanelWire`] carries
///   its own target-or-address, so a partially wired transport dispatches
///   mixed (panel writes on the wired dimensions, slot edits on the rest).
///
/// **Anchor** (Q22): the group's identity is the RATE leaf's channel, since
/// the fader is the control people mean by "the speed"; if rate
/// specifically is unwired, the next wired sibling in declaration order
/// (play_state, then scrub) stands in, so the group keeps a stable
/// `(scope, channel)` to dedup, reset, and read panel state through.
fn clock_transport_control(
    sections: &[UiNodeSection],
    transport: &crate::UiClockTransport,
) -> Option<UiPanelControl> {
    use crate::{UiPanelWire, UiPanelWireRole};

    let rows = debug_rows(sections);
    // Anchor order, not declaration order: rate leads (Q22).
    let dimensions = [
        (UiPanelWireRole::Rate, TRANSPORT_RATE_ROW),
        (UiPanelWireRole::PlayState, TRANSPORT_PLAY_STATE_ROW),
        (UiPanelWireRole::Scrub, TRANSPORT_SCRUB_ROW),
    ];
    let wired: Vec<(UiPanelWire, Option<&UiConfigSlot>)> = dimensions
        .into_iter()
        .map(|(role, key)| {
            let row = rows.iter().copied().find(|row| row.key == key);
            (
                UiPanelWire {
                    role,
                    address: row.and_then(editable_row_address),
                    panel_target: row.and_then(public_panel_target),
                    live_value: row.and_then(bound_live_value),
                },
                row,
            )
        })
        .collect();
    // Membership: zero panel-public leaves means no panel presence. The
    // card's own tape hero is unaffected — it renders off the transport
    // block, wired or not.
    let (anchor, anchor_row) = wired.iter().find(|(wire, _)| wire.panel_target.is_some())?;

    Some(UiPanelControl {
        label: "Time".to_string(),
        address: anchor.address.clone(),
        widget: UiPanelWidget::Transport {
            transport: transport.clone(),
        },
        // The group's scalar stand-in is the rate: it is the dimension a
        // generic reader (the readout, the panel-emit ladder) can say
        // anything true about. The faceplate shows all three itself.
        value: UiSlotValue::f32(transport.rate),
        emit: crate::UiPanelEmit::Value,
        live_value: anchor.live_value.clone(),
        live_gradient: None,
        panel_target: anchor.panel_target.clone(),
        unit: None,
        state: anchor_row
            .map(|row| row.state.clone())
            .unwrap_or_else(crate::UiSlotFieldState::editable),
        aspects: anchor_row
            .map(UiConfigSlot::visible_aspects)
            .unwrap_or_default(),
        wires: wired.into_iter().map(|(wire, _)| wire).collect(),
    })
}

/// A wired row's live channel reading as an `f32` — what a panel-written
/// dimension is actually doing, echo included.
fn live_f32(row: &UiConfigSlot) -> Option<f32> {
    bound_live_value(row)?.parse().ok()
}

/// A wired row's live channel reading as a [`lpc_model::PlayState`]. The
/// channel carries the state's wire tag, so an unknown tag reads as "no
/// live reading" and the staged slot value stands.
fn live_play_state(row: &UiConfigSlot) -> Option<lpc_model::PlayState> {
    lpc_model::PlayState::parse(&bound_live_value(row)?)
}

/// The row's active debug-override entry: `Some` while the row is dirty
/// (an override is live this session), carrying the address the per-value
/// **Clear** dispatches — the row's own edit entry, or the row address for
/// a scalar whose entry annotation has not landed yet. `None` = clean, no
/// tint, no Clear.
fn row_override(row: &UiConfigSlot) -> Option<ProjectSlotAddress> {
    if row.state.dirty == crate::UiNodeDirtyState::Clean {
        return None;
    }
    row.edit_entry_address
        .clone()
        .or_else(|| row.address.clone())
}

/// A row's scalar `f32` value, when its body is a plain value of that kind.
fn row_f32(row: &UiConfigSlot) -> Option<f32> {
    match &row.body {
        UiConfigSlotBody::Value(value) => match value.kind {
            UiSlotValueKind::F32(value) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

/// A row's [`PlayState`], when its body is the state's wire tag. The slot
/// carries the enum as a string leaf, so an unknown tag reads as "no
/// transport" rather than a guessed state.
fn row_play_state(row: &UiConfigSlot) -> Option<lpc_model::PlayState> {
    match &row.body {
        UiConfigSlotBody::Value(value) => match &value.kind {
            UiSlotValueKind::String(value) => lpc_model::PlayState::parse(value),
            _ => None,
        },
        _ => None,
    }
}

/// [`row_edit_address`], but `None` when the row itself is not editable —
/// the DTO's own "not editable" signal (transport rows are ordinarily
/// always writable Debug fields, but the address should never invite a
/// dispatch the row can't accept).
fn editable_row_address(row: &UiConfigSlot) -> Option<ProjectSlotAddress> {
    if !row.state.editable {
        return None;
    }
    row_edit_address(row)
}

/// The shader card's face: visual hero, panel knobs, space section, code
/// drawer. `None` when the node produces no visual output row (nothing to
/// be a face of).
///
/// `status_detail` is the node's error text, which is where the D1
/// declared-vs-entry mismatch surfaces — see
/// [`node_space_section::shader_space_section`].
fn shader_face(sections: &[UiNodeSection], status_detail: Option<&str>) -> Option<UiShaderFace> {
    let preview = product_of_kind(sections, UiProductKind::Visual)?;
    Some(UiShaderFace {
        preview,
        controls: shader_panel_controls(sections),
        // Decorated by the studio controller's view build (the project
        // walk stays agent-free, same rule as the sections path).
        agent: None,
        code_drawer: glsl_inline_editor(sections),
        space: node_space_section::shader_space_section(&config_rows(sections), status_detail),
    })
}

/// The fixture card's face: lamp preview + dominant brightness fader. Both
/// pieces are required — a fixture whose `brightness` row carries no
/// mappable editor hint keeps the generic sections.
///
/// Q13 (binding-is-publicity) deleted the `panel` flag that used to pick
/// this row, so the fader is now named outright: **brightness is the
/// fixture face's own affordance**, not a panel entry. It reaches the
/// enclosing module's panel only through the usual door — a `panel_target`,
/// i.e. brightness wired to a bus channel — which the block below resolves.
fn fixture_face(sections: &[UiNodeSection]) -> Option<UiFixtureFace> {
    let preview = product_of_kind(sections, UiProductKind::Control)?;
    let rows = config_rows(sections);
    let (key, mut brightness) = rows
        .iter()
        .filter(|slot| slot_field_name(&slot.key) == "brightness")
        .find_map(|slot| {
            Some((
                slot.key.clone(),
                panel_control_from_row(slot, panel_widget(slot)?)?,
            ))
        })?;
    // Same rule the shader knobs follow: when the fader's slot is wired to
    // a bus channel, the fader drives that channel (a panel write) rather
    // than an authored default it can no longer affect. The wiring
    // decorates the authored `brightness` row itself (the graph overlays —
    // authored facts, then the default overlay carrying the declared
    // `panel = "show"` promotion), so the scan below covers whichever row
    // carries the wiring. The standard shape today: FixtureDef declares
    // `default_bind = "bus:brightness"` with the hint, so every fixture's
    // fader is channel-backed with zero authoring — brightness is the
    // scarf's own control (panel.md P10).
    let field = slot_field_name(&key).to_string();
    let wired = || rows.iter().filter(|row| row.key == field);
    if brightness.panel_target.is_none() {
        brightness.panel_target = wired().find_map(|row| public_panel_target(row));
    }
    if brightness.live_value.is_none() {
        brightness.live_value = wired().find_map(|row| bound_live_value(row));
    }
    Some(UiFixtureFace {
        preview,
        brightness,
        mapping_editor: inline_editor_of_kind(sections, UiAssetEditorKind::Map2d),
        power: fixture_power(sections),
        space: node_space_section::fixture_space_section(&rows),
    })
}

/// The bare field name a config row's key ends in: `a.b[key]` → `b`.
fn slot_field_name(key: &str) -> &str {
    let field = key.rsplit('.').next().unwrap_or(key);
    field.split('[').next().unwrap_or(field)
}

/// The fixture's power readout, present only when the fixture is limited.
///
/// Every value here is produced runtime state, including the budget: the node
/// publishes the budget actually in force after an unstated one has fallen back
/// to the default, so this never re-derives the defaulting rule and can never
/// report a percentage against a budget nothing is enforcing.
///
/// A zero budget is a deliberate opt-out, and gets no readout.
fn fixture_power(sections: &[UiNodeSection]) -> Option<UiFixturePower> {
    let budget_ma = produced_u32(sections, "power_budget_ma")?;
    if budget_ma == 0 {
        return None;
    }
    Some(UiFixturePower {
        estimated_draw_ma: produced_u32(sections, "estimated_draw_ma").unwrap_or(0),
        budget_ma,
        scale: produced_f32(sections, "power_scale").unwrap_or(1.0),
    })
}

/// First produced product row of the wanted kind; an `Empty`-kind row (the
/// output exists but nothing resolved yet) is the stable-face fallback.
///
/// Shared with the module-face derivation (`ProjectController::apply_module_faces`):
/// a module's hero is its own `output` mirror, chosen by exactly the rule
/// the shader hero uses, so the two heroes can never disagree about what
/// "the visual this card produces" means.
pub(in crate::app::project) fn product_of_kind(
    sections: &[UiNodeSection],
    kind: UiProductKind,
) -> Option<UiProducedProduct> {
    let products = sections.iter().find_map(|section| match section {
        UiNodeSection::ProducedProducts(products) => Some(products),
        _ => None,
    })?;
    products
        .iter()
        .find(|product| product.kind == kind)
        .or_else(|| {
            products
                .iter()
                .find(|product| product.kind == UiProductKind::Empty)
        })
        .cloned()
}

/// Flattened top-level config rows (composite rows are NOT descended here;
/// panel scans that need record fields descend explicitly).
fn config_rows(sections: &[UiNodeSection]) -> Vec<&UiConfigSlot> {
    sections
        .iter()
        .filter_map(|section| match section {
            UiNodeSection::ConfigSlots(slots) => Some(slots),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Flattened Debug rows — the `config_rows` twin over `UiNodeSection
/// ::DebugSlots` instead. Debug rows are already flat at the section level
/// (`SlotController::partition_debug` lifts every Debug field to a
/// top-level row regardless of nesting depth in the record that declared
/// it), so this needs no record-descent the way `config_rows` never needed
/// one either.
fn debug_rows(sections: &[UiNodeSection]) -> Vec<&UiConfigSlot> {
    sections
        .iter()
        .filter_map(|section| match section {
            UiNodeSection::DebugSlots(slots) => Some(slots),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The first inline GLSL editor among the node's asset rows — the code
/// drawer reuses it verbatim (it is the SAME editor the sections view
/// renders, minus the studio-level agent decoration).
fn glsl_inline_editor(sections: &[UiNodeSection]) -> Option<UiAssetEditor> {
    inline_editor_of_kind(sections, UiAssetEditorKind::Glsl)
}

/// First inline editor of `kind` anywhere in the asset/config slot rows
/// (records searched recursively) — the face derives from section DTOs,
/// never from controller state.
fn inline_editor_of_kind(
    sections: &[UiNodeSection],
    kind: UiAssetEditorKind,
) -> Option<UiAssetEditor> {
    fn in_slots(slots: &[UiConfigSlot], kind: UiAssetEditorKind) -> Option<UiAssetEditor> {
        slots.iter().find_map(|slot| match &slot.body {
            UiConfigSlotBody::Asset(asset) if asset.editor == kind => asset.inline_editor.clone(),
            UiConfigSlotBody::Record(record) => in_slots(&record.fields, kind),
            _ => None,
        })
    }
    sections.iter().find_map(|section| match section {
        UiNodeSection::AssetSlots(slots)
        | UiNodeSection::ConfigSlots(slots)
        | UiNodeSection::DebugSlots(slots) => in_slots(slots, kind),
        _ => None,
    })
}

// -- shader panel controls ---------------------------------------------------

/// Knob controls from the shader's `consumed` map: one per uniform that is
/// BOUND to a bus channel and whose `default` value is present (the knob
/// edits `consumed.<name>.default.some` through the standard slot path, or
/// writes the channel when the binding gives it a panel target). The knob
/// wears the binding's aspect, so it rolls up violet exactly like the
/// binding row.
fn shader_panel_controls(sections: &[UiNodeSection]) -> Vec<UiPanelControl> {
    let rows = config_rows(sections);
    let Some(UiConfigSlotBody::Record(consumed)) = rows
        .iter()
        .find(|row| row.key == "consumed")
        .map(|row| &row.body)
    else {
        return Vec::new();
    };
    // One phasor on the def → the knob is just "Speed"; several need the
    // uniform's name to tell them apart (G2 feedback: "phase period" was
    // expert vocabulary). A def can also carry a plain VALUE uniform the
    // author already labeled "Speed" (the pre-migration idiom) — the plain
    // label yields to it rather than rendering two knobs with one name.
    let lone_phasor = consumed
        .fields
        .iter()
        .filter(|entry| uniform_kind(entry).as_deref() == Some(PHASOR_SLOT_KIND))
        .count()
        == 1
        && !consumed.fields.iter().any(|entry| {
            uniform_kind(entry).as_deref() != Some(PHASOR_SLOT_KIND) && uniform_speed_label(entry)
        });
    consumed
        .fields
        .iter()
        .filter_map(|entry| match uniform_kind(entry).as_deref() {
            // A timebase uniform has no `default` to turn: its value comes
            // from the scope's clock. The ONE thing a person tunes about a
            // phasor is how long a cycle takes, so that is the one control
            // it gets (settled D11 v1 — waveform and phase offset stay
            // card-face/def-editable, because a waveform is how ONE reader
            // shapes a possibly-shared phase).
            Some(PHASOR_SLOT_KIND) => phasor_period_control(entry, &rows, lone_phasor),
            // `seconds` is unbounded time. There is nothing to set: no
            // period, no range, no default. It gets no knob at all.
            Some(SECONDS_SLOT_KIND) => None,
            // A palette has no `default` either — its value is the whole
            // `GradientConfig` on the slot's `gradient` option (M4 P3).
            Some(PALETTE_SLOT_KIND) => palette_swatch_control(entry, &rows),
            _ => shader_uniform_control(entry, &rows),
        })
        .collect()
}

/// Whether a uniform's display label is "Speed" (the pre-migration idiom a
/// plain phasor label must not collide with).
fn uniform_speed_label(entry: &UiConfigSlot) -> bool {
    let UiConfigSlotBody::Record(record) = &entry.body else {
        return false;
    };
    string_field(&record.fields, "label")
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| entry.label.clone())
        .eq_ignore_ascii_case("speed")
}

/// The `kind` discriminant string on a uniform's record row
/// (`value` / `phasor` / `seconds`).
fn uniform_kind(entry: &UiConfigSlot) -> Option<String> {
    let UiConfigSlotBody::Record(record) = &entry.body else {
        return None;
    };
    string_field(&record.fields, "kind")
}

/// `ShaderSlotKind::Phasor`'s wire tag.
const PHASOR_SLOT_KIND: &str = "phasor";
/// `ShaderSlotKind::Seconds`'s wire tag.
const SECONDS_SLOT_KIND: &str = "seconds";
/// `ShaderSlotKind::Palette`'s wire tag.
const PALETTE_SLOT_KIND: &str = "palette";

/// Widest period a phasor knob reaches when the uniform authors no range:
/// one hour. The knob sweeps LOG-period, so the slow decades cost no
/// resolution at the fast end — and G3 judged the old two-minute floor
/// (30/hr) "too fast" for the slow end of the sweep. An author who wants a
/// different range says so with `min`/`max` like any other uniform.
const PHASOR_PERIOD_MAX_SECONDS: f32 = 3600.0;

/// One phasor uniform → its **period** knob (P7 item 5).
///
/// Unlike a value uniform's knob, this one exists whether or not the slot's
/// wiring is public: a phasor's period is the card's own affordance the way
/// a fixture's brightness fader is. What publicity decides is only where
/// the knob *also* shows up and what a gesture writes:
///
/// - **authored config channel** (public `panel_target`) → the knob joins
///   the enclosing module's panel and gestures are PANEL WRITES of a whole
///   `PhasorConfig` onto that channel, which is what every reader of the
///   channel then integrates at (parent D3: one shared integrator);
/// - **slot-local, or `default_bind`-only wiring** → card face only, and
///   gestures are ordinary slot edits at `consumed[<name>].phasor.some`.
///
/// Both paths carry the slot's own waveform and phase offset through
/// untouched ([`crate::UiPanelEmit::PhasorPeriod`]) — the period is the
/// only field a panel may move.
fn phasor_period_control(
    entry: &UiConfigSlot,
    top_rows: &[&UiConfigSlot],
    lone_phasor: bool,
) -> Option<UiPanelControl> {
    let UiConfigSlotBody::Record(record) = &entry.body else {
        return None;
    };
    let fields = &record.fields;
    let name = map_entry_name(entry);
    // The authored config row. Absent (option off) = nothing to turn: the
    // engine runs the slot on `PhasorConfig::default()`, and a knob that
    // edited a slot which is not there would need the option-on gesture the
    // generic row already owns.
    let config_row = uniform_field(fields, "phasor")
        .filter(|row| row.optionality.is_some_and(|opt| opt.included))?;
    let UiConfigSlotBody::Value(config_value) = &config_row.body else {
        return None;
    };
    let UiSlotValueKind::Struct { fields: config, .. } = &config_value.kind else {
        return None;
    };
    let period = struct_f32(config, "period_seconds")?;

    let min = option_f32_field(fields, "min").unwrap_or(0.0);
    let max = option_f32_field(fields, "max").unwrap_or(PHASOR_PERIOD_MAX_SECONDS);
    let mut control = UiPanelControl {
        // The knob's LABEL: "Period" when the def has one phasor; the
        // uniform's name joins only to disambiguate several. The M4 P6
        // gate picked the plain-seconds voice (retiring the PROVISIONAL
        // reciprocal Speed readout), so the knob speaks the seconds the
        // slot actually stores — the clock face's own vocabulary.
        label: if lone_phasor {
            "Period".to_string()
        } else {
            format!(
                "{} period",
                string_field(fields, "label")
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(|| entry.label.clone())
            )
        },
        address: row_edit_address(config_row),
        widget: UiPanelWidget::Knob {
            min,
            max,
            // Continuous: a period is seconds, not a count.
            step: None,
        },
        // The knob turns the PERIOD, so its value is that one number even
        // though the slot it writes is the whole record. The FACE readout
        // presents it in plain seconds ("100 s") with the unit riding the
        // string, so the control carries no unit suffix.
        value: UiSlotValue::f32(period).with_unit(crate::UiSlotUnit::seconds()),
        emit: crate::UiPanelEmit::PhasorPeriod {
            waveform: struct_waveform(config),
            phase_offset: struct_f32(config, "phase_offset").unwrap_or(0.0),
        },
        live_value: None,
        // A phasor is not a palette; nothing to carry structurally.
        live_gradient: None,
        panel_target: None,
        unit: None,
        state: config_row.state.clone(),
        aspects: config_row.visible_aspects(),
        wires: Vec::new(),
    };
    // The wiring facts live on the binding-derived row keyed by the bare
    // uniform name, exactly as they do for a value uniform's knob.
    if let Some(binding) = uniform_binding_aspect(top_rows, &name) {
        replace_binding_aspect(&mut control.aspects, binding);
    }
    control.panel_target = top_rows
        .iter()
        .find(|row| row.key == name)
        .and_then(|row| public_panel_target(row));
    // The live reading rides the same binding-derived row (the channel's
    // PhasorConfig reads back as its period) — without it the knob snapped
    // back to the authored value after every gesture, because the writes it
    // was landing had no display path (G2: "stuck at 100").
    control.live_value = top_rows
        .iter()
        .find(|row| row.key == name)
        .and_then(|row| bound_live_value(row));
    Some(control)
}

/// One palette uniform → its **swatch** control (M4 P3).
///
/// The palette twin of [`phasor_period_control`], and the differences are
/// the interesting part:
///
/// - the value is the WHOLE `GradientConfig` on the slot's `gradient`
///   option, not one field of a record, so the emit family carries no
///   shaping to preserve ([`crate::UiPanelEmit::Gradient`]);
/// - there is no range, because a palette is not a number.
///
/// Everything else is the same rule: the control exists whether or not the
/// wiring is public (a palette is the card's own affordance), and publicity
/// only decides whether a pick becomes a PANEL WRITE on the config channel
/// — where every reader of that channel takes the config whole
/// (`resolve_gradient_config`) — or an ordinary slot edit at
/// `consumed[<name>].gradient.some`.
///
/// An ABSENT `gradient` option still gets a control, seeded with
/// `GradientConfig::default()` — exactly what the engine runs the slot on.
/// This is the realistic authored case: most palette slots are authored
/// with just a `default_bind` and no inline `gradient`, so the option
/// arrives absent and the swatch is how the first palette gets picked at
/// all. The first pick's `AssignValue` at `…gradient.some`
/// materializes the option (the overlay's ensure-present rule).
fn palette_swatch_control(
    entry: &UiConfigSlot,
    top_rows: &[&UiConfigSlot],
) -> Option<UiPanelControl> {
    let UiConfigSlotBody::Record(record) = &entry.body else {
        return None;
    };
    let fields = &record.fields;
    let name = map_entry_name(entry);
    let config_row = uniform_field(fields, "gradient")?;
    let present = config_row.optionality.is_some_and(|opt| opt.included);
    let (config_value, address) = if present {
        let UiConfigSlotBody::Value(config_value) = &config_row.body else {
            return None;
        };
        // The value has to READ as a palette, or the swatch has nothing to
        // sample and the web layer would fall back to a bare display anyway.
        crate::app::project::gradient_config_value(&config_value.kind.to_lp_value())?;
        (config_value.clone(), row_edit_address(config_row))
    } else {
        let default_value = crate::UiSlotValue::from_lp_value(&lpc_model::ToLpValue::to_lp_value(
            &lpc_model::GradientConfig::default(),
        ))
        .with_editor(crate::UiSlotEditorHint::Gradient);
        let address = config_row
            .address
            .as_ref()
            .and_then(|address| address.child_field("some"));
        (default_value, address)
    };

    let mut control = UiPanelControl {
        label: string_field(fields, "label")
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| entry.label.clone()),
        address,
        widget: UiPanelWidget::PaletteSwatch,
        value: config_value,
        emit: crate::UiPanelEmit::Gradient,
        live_value: None,
        live_gradient: None,
        panel_target: None,
        // A palette carries its own vocabulary in the readout (`5 stops`,
        // `↻ 4 · 3/min`); a unit suffix has nothing to add.
        unit: None,
        state: config_row.state.clone(),
        aspects: config_row.visible_aspects(),
        wires: Vec::new(),
    };
    // The wiring facts live on the binding-derived row keyed by the bare
    // uniform name, exactly as they do for a knob.
    if let Some(binding) = uniform_binding_aspect(top_rows, &name) {
        replace_binding_aspect(&mut control.aspects, binding);
    }
    let top_row = top_rows.iter().find(|row| row.key == name);
    control.panel_target = top_row.and_then(|row| public_panel_target(row));
    // A driven palette reads back BOTH ways: the config summary for the
    // readout's text surfaces (the `format_live_panel_value` gradient branch),
    // and the config itself for the strips — so the swatch shows the palette
    // that is playing, including one this panel just wrote. The summary alone
    // could not do that: a `GradientConfig` does not survive the round trip
    // through display text.
    control.live_value = top_row.and_then(|row| bound_live_value(row));
    control.live_gradient = top_row.and_then(|row| bound_live_gradient(row));
    Some(control)
}

/// A named `f32` inside a struct-valued slot payload.
fn struct_f32(fields: &[(String, UiSlotValue)], name: &str) -> Option<f32> {
    match fields.iter().find(|(field, _)| field == name)?.1.kind {
        UiSlotValueKind::F32(value) => Some(value),
        _ => None,
    }
}

/// The config's waveform, defaulting to `Ramp` — the same fallback the
/// model uses, so an unreadable payload cannot silently reshape a phasor
/// the next time its period is turned.
fn struct_waveform(fields: &[(String, UiSlotValue)]) -> lpc_model::Waveform {
    let Some((_, value)) = fields.iter().find(|(field, _)| field == "waveform") else {
        return lpc_model::Waveform::default();
    };
    let UiSlotValueKind::String(tag) = &value.kind else {
        return lpc_model::Waveform::default();
    };
    lpc_model::Waveform::parse(tag).unwrap_or_default()
}

/// One uniform entry (a `ShaderSlotDef` record row) → its knob control.
fn shader_uniform_control(
    entry: &UiConfigSlot,
    top_rows: &[&UiConfigSlot],
) -> Option<UiPanelControl> {
    let UiConfigSlotBody::Record(record) = &entry.body else {
        return None;
    };
    let fields = &record.fields;
    let name = map_entry_name(entry);
    // Q13, binding-is-publicity: "on the panel" IS "bound to a bus
    // channel". Membership is therefore read off the binding-derived row
    // the project walk appends for a wired uniform (keyed by the bare
    // uniform name) — it carries a `panel_target` exactly when the binding
    // resolves to a `(scope, channel)`. The legacy authored `panel` flag is
    // gone; an unbound uniform gets no knob anywhere.
    //
    // GV fix 1: publicity is AUTHORED wiring only. A uniform reached solely
    // through its own `default_bind` (the probe marks that binding
    // `WireBindingOrigin::Default`, and the derived endpoint carries
    // `default_origin`) is plumbing the author never asked for — fyeah's
    // `time` is bound to `bus:time` by its shape, and a time knob on the
    // panel is noise. The channel stays wired and readable; it just is not
    // a control.
    let panel_target = top_rows
        .iter()
        .find(|row| row.key == name)
        .and_then(|row| public_panel_target(row))?;

    let default_row = uniform_field(fields, "default")?;
    // Whole-number uniforms ("how many meteors") snap: the authored `step`
    // when present, else 1 for an i32/u32-shaped uniform.
    let step = shader_panel_step(
        option_f32_field(fields, "step"),
        &ShaderValueShapeRef::builtin(&string_field(fields, "value").unwrap_or_default()),
    );
    let min = option_f32_field(fields, "min").unwrap_or(0.0);
    let control = panel_control_from_row(
        default_row,
        UiPanelWidget::Knob {
            min,
            max: option_f32_field(fields, "max").unwrap_or(1.0),
            step,
        },
    )?;

    let label = string_field(fields, "label")
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| entry.label.clone());
    let unit = string_field(fields, "unit")
        .filter(|unit| !unit.is_empty())
        .map(|unit| crate::app::project::slot::slot_controller::ui_slot_unit(&unit));

    let mut control = UiPanelControl { label, ..control };
    if let Some(step) = step {
        snap_control_display(&mut control, min, step);
    }
    if unit.is_some() {
        control.unit = unit;
    }
    if let Some(binding) = uniform_binding_aspect(top_rows, &name) {
        replace_binding_aspect(&mut control.aspects, binding);
    }
    // A wired uniform's live bus reading rides the binding-derived row
    // (keyed by the bare uniform name), not the authored default row the
    // knob edits — mirror it onto the control for display (P6 item 1).
    if control.live_value.is_none() {
        control.live_value = top_rows
            .iter()
            .find(|row| row.key == name)
            .and_then(|row| bound_live_value(row));
    }
    // The panel-write target is the membership fact itself: the knob writes
    // the consumed (scope, channel) down the command path rather than
    // editing an authored default it can no longer affect.
    if control.panel_target.is_none() {
        control.panel_target = Some(panel_target);
    }
    Some(control)
}

/// Snap a stepped knob's readout onto the step grid, so a `count` uniform
/// never *reads* `2.37` even when the stored default predates the step (the
/// grid is a display rule here; the slot itself only changes when the knob
/// is moved, and the knob's own gestures snap on the way out).
///
/// The grid is anchored at `min`, matching the widget's own quantization —
/// the readout and the pointer must agree on which position they are at.
fn snap_control_display(control: &mut UiPanelControl, min: f32, step: f32) {
    let UiSlotValueKind::F32(value) = control.value.kind else {
        return;
    };
    let snapped = min + ((value - min) / step).round() * step;
    if snapped == value {
        return;
    }
    control.value.kind = UiSlotValueKind::F32(snapped);
    control.value.display = format_lp_value(&LpValue::F32(snapped));
}

/// A row's live bus reading (display-only), from the bound source endpoint
/// the project walk decorates with the consumed channel's current value
/// (P6 item 1).
fn bound_live_value(slot: &UiConfigSlot) -> Option<String> {
    match &slot.source {
        UiSlotSourceState::Bound(endpoint) => endpoint.live_value.clone(),
        _ => None,
    }
}

/// The same row's live reading as a gradient config — present only when the
/// channel carries one, which is what keeps every other control's payload
/// exactly as it was.
fn bound_live_gradient(slot: &UiConfigSlot) -> Option<lpc_model::GradientConfig> {
    match &slot.source {
        UiSlotSourceState::Bound(endpoint) => endpoint.live_gradient.clone(),
        _ => None,
    }
}

/// A row's panel-write target — the `(scope, channel)` its bound source
/// endpoint consumes — but only when the wiring is **public**: authored, or a
/// default-origin endpoint whose slot declares `panel = "show"`. A bare
/// `default_bind` the loader materialized does not make its slot public
/// (GV fix 1); the hint is the one additive override on that rule
/// (ADR 2026-08-03-panel-visibility-is-derived, amended — the fixture's
/// brightness fader is the motivating case).
fn public_panel_target(slot: &UiConfigSlot) -> Option<crate::UiPanelTarget> {
    match &slot.source {
        UiSlotSourceState::Bound(endpoint) if !endpoint.default_origin || endpoint.panel_hint => {
            endpoint.panel_target.clone()
        }
        _ => None,
    }
}

/// A map-entry row's key segment (the trailing bracket key of the row's
/// key, e.g. `consumed[speed]` → `speed`, `entries[2]` → `2`).
fn map_entry_name(entry: &UiConfigSlot) -> String {
    let key = entry.key.as_str();
    if let Some(stripped) = key.strip_suffix(']')
        && let Some(open) = stripped.rfind('[')
    {
        return stripped[open + 1..].trim_matches('"').to_string();
    }
    key.rsplit('.').next().unwrap_or(key).to_string()
}

/// A bound uniform's Binding aspect, taken from the binding-derived row the
/// project walk appends for wired runtime slots (`extra_config`): the row
/// keyed by the bare uniform name whose aspects announce `Bound`.
fn uniform_binding_aspect(top_rows: &[&UiConfigSlot], name: &str) -> Option<UiSlotAspect> {
    let row = top_rows.iter().find(|row| row.key == name)?;
    row.visible_aspects()
        .into_iter()
        .find(|aspect| aspect.kind == UiSlotAspectKind::Binding && aspect.affordance.is_some())
}

/// Swap the control's Binding aspect for the wired one (append when the
/// default row carried none).
fn replace_binding_aspect(aspects: &mut Vec<UiSlotAspect>, binding: UiSlotAspect) {
    match aspects
        .iter_mut()
        .find(|aspect| aspect.kind == UiSlotAspectKind::Binding)
    {
        Some(slot) => *slot = binding,
        None => aspects.push(binding),
    }
}

fn uniform_field<'a>(fields: &'a [UiConfigSlot], name: &str) -> Option<&'a UiConfigSlot> {
    let suffix = format!(".{name}");
    fields.iter().find(|field| field.key.ends_with(&suffix))
}

/// A present `OptionSlot<ValueSlot<f32>>` field's value.
fn option_f32_field(fields: &[UiConfigSlot], name: &str) -> Option<f32> {
    let field = uniform_field(fields, name)?;
    if !field.optionality.is_some_and(|opt| opt.included) {
        return None;
    }
    match &field.body {
        UiConfigSlotBody::Value(UiSlotValue {
            kind: UiSlotValueKind::F32(value),
            ..
        }) => Some(*value),
        _ => None,
    }
}

/// A plain string field's value.
fn string_field(fields: &[UiConfigSlot], name: &str) -> Option<String> {
    match &uniform_field(fields, name)?.body {
        UiConfigSlotBody::Value(UiSlotValue {
            kind: UiSlotValueKind::String(value),
            ..
        }) => Some(value.clone()),
        UiConfigSlotBody::Value(UiSlotValue {
            kind: UiSlotValueKind::Unset,
            ..
        })
        | UiConfigSlotBody::Empty => None,
        _ => None,
    }
}

// -- playlist face -------------------------------------------------------------

/// The playlist card's face: the entries strip plus the index of the ACTIVE
/// entry's child in `children`. `None` — full-rendering fallback — when the
/// def has no entries row, or when entries exist but the produced
/// `active_entry` status has not arrived, the active key names no strip
/// entry, or the active entry's child is not among the built child DTOs.
/// An empty entries map derives an empty-strip face (`active: None`, no
/// child to filter) — a freshly created playlist's card is the strip's
/// empty state, not the generic fallback.
fn playlist_face(
    address: &ProjectNodeAddress,
    sections: &[UiNodeSection],
    children: &[UiNodeChild],
) -> Option<(UiPlaylistFace, Option<usize>)> {
    let rows = config_rows(sections);
    let UiConfigSlotBody::Record(entries_map) = rows
        .iter()
        .find(|row| row.key == "entries")
        .map(|row| &row.body)?
    else {
        return None;
    };
    let mut entries: Vec<(UiPlaylistEntry, Option<usize>)> = entries_map
        .fields
        .iter()
        .filter_map(|row| playlist_entry(address, row, children))
        .collect();
    if entries.is_empty() {
        let face = UiPlaylistFace {
            entries: Vec::new(),
            active: None,
        };
        return Some((face, None));
    }
    // The status seam: `PlaylistState.active_entry` (produced u32 =
    // entries-map key), projected as a ProducedValues row. Its display is
    // `u32::to_string`, so the parse is the exact inverse.
    let active_key = produced_u32(sections, "active_entry")?;
    // The ACTIVE entry is already playing, so its chip keeps the child's
    // select action (activating it would be a no-op poke); every other
    // chip activates. P7 spec: "the activate op for non-active entries".
    for (entry, child) in &mut entries {
        if entry.key == active_key {
            entry.action = child.and_then(|index| children[index].action.clone());
        }
    }
    let active_child = entries
        .iter()
        .find(|(entry, _)| entry.key == active_key)
        .and_then(|(_, child)| *child)?;

    let face = UiPlaylistFace {
        entries: entries.into_iter().map(|(entry, _)| entry).collect(),
        active: Some(active_key),
    };
    Some((face, Some(active_child)))
}

/// One `entries[<key>]` record row → its strip entry plus the index of its
/// mounted child DTO (matched by the loader's naming rule: the authored
/// entry `name`, else `entry_<key>`). Dangling entries (no mounted child)
/// still chip into the strip, name-only and inert.
fn playlist_entry(
    address: &ProjectNodeAddress,
    row: &UiConfigSlot,
    children: &[UiNodeChild],
) -> Option<(UiPlaylistEntry, Option<usize>)> {
    let UiConfigSlotBody::Record(record) = &row.body else {
        return None;
    };
    let key: u32 = map_entry_name(row).parse().ok()?;
    let fields = &record.fields;

    let authored_name = string_field(fields, "name").filter(|name| !name.is_empty());
    let child_name = authored_name
        .clone()
        .unwrap_or_else(|| format!("entry_{key}"));
    let child = children
        .iter()
        .position(|child| child_tree_name(child) == Some(child_name.as_str()));

    let name = authored_name
        .or_else(|| child.map(|index| children[index].label.clone()))
        .unwrap_or_else(|| format!("Entry {key}"));
    let activate_label = format!("Activate {name}");
    let entry = UiPlaylistEntry {
        key,
        name,
        // Authored seconds (`PositiveF32`) → the strip's m:ss chip unit.
        duration_ms: option_f32_field(fields, "duration")
            .map(|seconds| (f64::from(seconds) * 1000.0).round() as u64),
        cue: option_list_field_is_non_empty(fields, "trigger_ids"),
        thumb: child.and_then(|index| child_visual_snapshot(&children[index])),
        // Entry click = activate NOW through the runtime command channel
        // (P7). A poke, not an edit: nothing stages in the overlay. Every
        // entry gets it, mounted child or not — activation addresses the
        // entries-map key, which exists independent of child mounting.
        action: Some(
            UiAction::from_op(
                ControllerId::new(ProjectController::NODE_ID),
                PlaylistActivateOp {
                    node: address.clone(),
                    entry: key,
                },
            )
            .with_label(activate_label),
        ),
    };
    Some((entry, child))
}

/// A produced-value row's u32 reading, keyed by the produced slot's path.
fn produced_u32(sections: &[UiNodeSection], key: &str) -> Option<u32> {
    sections
        .iter()
        .find_map(|section| match section {
            UiNodeSection::ProducedValues(values) => Some(values),
            _ => None,
        })?
        .iter()
        .find(|value| value.key == key)?
        .value
        .parse()
        .ok()
}

fn produced_f32(sections: &[UiNodeSection], key: &str) -> Option<f32> {
    sections
        .iter()
        .find_map(|section| match section {
            UiNodeSection::ProducedValues(values) => Some(values),
            _ => None,
        })?
        .iter()
        .find(|value| value.key == key)?
        .value
        .parse()
        .ok()
}

/// The child's tree segment name, parsed from its address detail
/// (`/main.show/playlist.playlist/idle.shader` → `idle`).
fn child_tree_name(child: &UiNodeChild) -> Option<&str> {
    let segment = child.detail.rsplit('/').next()?;
    Some(segment.split_once('.').map_or(segment, |(name, _)| name))
}

/// The child's visual-output preview snapshot, when one has already landed
/// (previews are reused, never re-plumbed — a child with no cached probe
/// renders a name-only chip).
fn child_visual_snapshot(child: &UiNodeChild) -> Option<UiProductPreview> {
    child.sections.iter().find_map(|section| match section {
        UiNodeSection::ProducedProducts(products) => products
            .iter()
            .find(|product| {
                product.kind == UiProductKind::Visual
                    && matches!(product.preview, UiProductPreview::VisualSrgb8 { .. })
            })
            .map(|product| product.preview.clone()),
        _ => None,
    })
}

/// A present option list field carrying at least one element.
fn option_list_field_is_non_empty(fields: &[UiConfigSlot], name: &str) -> bool {
    uniform_field(fields, name).is_some_and(|field| {
        field.optionality.is_some_and(|opt| opt.included)
            && matches!(
                &field.body,
                UiConfigSlotBody::Value(UiSlotValue {
                    kind: UiSlotValueKind::Array(values),
                    ..
                }) if !values.is_empty()
            )
    })
}

// -- output face ---------------------------------------------------------------

/// The output card's face: one row per authored `channels[k]` wire, plus the
/// slice arithmetic that can be done from the authored counts alone.
///
/// `None` — generic-sections fallback — only when the node carries no
/// `channels` map row at all. An EMPTY map still derives a face: an output
/// with no wires is exactly the state whose surface should be "add a wire",
/// the same reasoning as the playlist's empty strip.
///
/// Board identity and the incoming lamp extent are deliberately absent here:
/// neither is visible from this node's sections, and the builder never reads
/// controller state. The studio controller's decoration pass fills them.
fn output_face(sections: &[UiNodeSection]) -> Option<UiOutputFace> {
    let rows = config_rows(sections);
    let channels_row = rows.iter().find(|row| row.key == "channels")?;
    let UiConfigSlotBody::Record(channels_map) = &channels_row.body else {
        return None;
    };
    let mut channels: Vec<UiOutputChannelRow> = channels_map
        .fields
        .iter()
        .filter_map(output_channel_row)
        .collect();
    // The slice order IS the key order (the engine's `planned_wires` walks
    // the map ascending); the projected map is already sorted, and sorting
    // here keeps the face's arithmetic true to the engine regardless.
    channels.sort_by_key(|channel| channel.key);
    resolve_authored_slices(&mut channels);

    Some(UiOutputFace {
        led_budget: None,
        channels,
        channels_address: channels_row.address.clone(),
        input_binding: bound_endpoint_label(&rows, "input"),
        // Filled by the decoration pass (board + upstream extent).
        total_lamps: None,
        span_boundaries: Vec::new(),
        board: None,
    })
}

/// Walk the channels in key order and hand each its slice start, mirroring
/// the engine's `planned_wires`: a lamp count advances the cursor, and the
/// count-less channel takes the remainder — so nothing after it has a
/// defined start (an authoring mistake the engine refuses outright; the face
/// leaves those starts `None` rather than inventing one).
fn resolve_authored_slices(channels: &mut [UiOutputChannelRow]) {
    let mut start = Some(0u32);
    for channel in channels {
        channel.slice_start = start;
        match channel.count {
            Some(count) => {
                channel.resolved_count = Some(count);
                start = start.map(|start| start.saturating_add(count));
            }
            None => start = None,
        }
    }
}

/// One `channels[<key>]` record row → its wire row. Rows whose key does not
/// parse as a channel index, or which carry no `endpoint` field, are not
/// wires and are dropped.
fn output_channel_row(row: &UiConfigSlot) -> Option<UiOutputChannelRow> {
    let UiConfigSlotBody::Record(record) = &row.body else {
        return None;
    };
    let key: u32 = map_entry_name(row).parse().ok()?;
    let fields = &record.fields;
    let endpoint_row = uniform_field(fields, "endpoint")?;
    let endpoint_display = string_field(fields, "endpoint").unwrap_or_default();

    Some(UiOutputChannelRow {
        wire_status: None,
        key,
        pin_label: endpoint_pin_label(&endpoint_display),
        endpoint_display,
        // Resolved by the decoration pass against the known board.
        gpio: None,
        count: option_u32_field(fields, "count"),
        resolved_count: None,
        slice_start: None,
        endpoint_address: row_edit_address(endpoint_row),
        // The count is an `OptionSlot`, so a PRESENT count edits its
        // interior `some` and an ABSENT one has no value address (including
        // it is the option-toggle gesture the generic row already owns) —
        // exactly the rule `row_edit_address` encodes.
        count_address: uniform_field(fields, "count").and_then(row_edit_address),
    })
}

/// The board's own label for the wire an endpoint spec names — the spec's
/// config segment (`ws281x:local:IO18` → `IO18`).
///
/// Endpoint↔pin translation lives in core, never in the web layer. An
/// unparseable spec yields an empty label, which reads downstream as
/// "unresolved": shown, never hidden.
fn endpoint_pin_label(endpoint: &str) -> String {
    HwEndpointSpec::parse(endpoint)
        .map(|spec| spec.config().to_string())
        .unwrap_or_default()
}

/// A present `OptionSlot<ValueSlot<u32>>` field's value.
fn option_u32_field(fields: &[UiConfigSlot], name: &str) -> Option<u32> {
    let field = uniform_field(fields, name)?;
    if !field.optionality.is_some_and(|opt| opt.included) {
        return None;
    }
    match &field.body {
        UiConfigSlotBody::Value(UiSlotValue {
            kind: UiSlotValueKind::U32(value),
            ..
        }) => Some(*value),
        _ => None,
    }
}

/// The endpoint label a top-level row is bound to, when it is bound.
fn bound_endpoint_label(rows: &[&UiConfigSlot], key: &str) -> Option<String> {
    match &rows.iter().find(|row| row.key == key)?.source {
        UiSlotSourceState::Bound(endpoint) => Some(endpoint.label.clone()),
        _ => None,
    }
}

// -- generic panel controls --------------------------------------------------

/// Project one config row into a panel control wearing `widget`: value,
/// state, and popover aspects are the row's own; the edit address follows
/// the slot-row rule (a present option row's value edits target the
/// interior `.some` slot).
fn panel_control_from_row(slot: &UiConfigSlot, widget: UiPanelWidget) -> Option<UiPanelControl> {
    let UiConfigSlotBody::Value(value) = &slot.body else {
        return None;
    };
    Some(UiPanelControl {
        label: slot.label.clone(),
        address: row_edit_address(slot),
        widget,
        value: value.clone(),
        emit: crate::UiPanelEmit::Value,
        live_value: bound_live_value(slot),
        live_gradient: bound_live_gradient(slot),
        panel_target: public_panel_target(slot),
        unit: value.unit.clone(),
        state: slot.state.clone(),
        aspects: slot.visible_aspects(),
        // One row, one channel: only a GROUPED control carries wires.
        wires: Vec::new(),
    })
}

/// The widget a value row renders as, from its editor hint (Knob → knob,
/// Slider → fader, bool → toggle). `None` = no mappable widget, so the row
/// has no panel presentation at all.
///
/// Q13 (binding-is-publicity): this answers "what would this row look like
/// as a control", never "does it belong on a panel". Membership is a
/// binding question now — a generic control reaches a module panel only by
/// carrying a `panel_target` — and the one caller left is the fixture face,
/// whose brightness fader is that face's OWN affordance rather than a panel
/// entry (`docs/design/panel.md` P1: it still shows up on the module panel
/// when, and only when, brightness is wired to a channel).
fn panel_widget(slot: &UiConfigSlot) -> Option<UiPanelWidget> {
    let UiConfigSlotBody::Value(value) = &slot.body else {
        return None;
    };
    match &value.editor {
        UiSlotEditorHint::Knob { min, max, step } => Some(UiPanelWidget::Knob {
            min: *min,
            max: *max,
            step: *step,
        }),
        UiSlotEditorHint::Slider { min, max, step } => Some(UiPanelWidget::Fader {
            min: *min,
            max: *max,
            step: *step,
        }),
        // The Gradient hint is what P1 declared on both palette storage
        // forms, so any gradient-shaped row reads as a swatch here — the
        // same hint the slot ROW dispatches its read-only strip on (P2).
        UiSlotEditorHint::Gradient => Some(UiPanelWidget::PaletteSwatch),
        _ => matches!(value.kind, UiSlotValueKind::Bool(_)).then_some(UiPanelWidget::Toggle),
    }
}

/// Value-edit target for a row: a present option row's edits go to the
/// interior `some` slot (the same rule `config_slot_row` applies).
fn row_edit_address(slot: &UiConfigSlot) -> Option<ProjectSlotAddress> {
    match &slot.optionality {
        Some(optionality) if optionality.included => slot
            .address
            .as_ref()
            .and_then(|address| address.child_field("some")),
        Some(_) => None,
        None => slot.address.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ControllerId, ProjectEditorOp, ProjectNodeAddress, ProjectSlotRoot, UiAction,
        UiBindingEndpoint, UiProducedValue, UiSlotFieldState, UiSlotOptionality, UiSlotSourceState,
    };
    use lpc_model::SlotPath;

    /// The stable authored address faces are built for; entry activation
    /// actions carry it (P7's runtime command channel).
    fn test_address() -> ProjectNodeAddress {
        ProjectNodeAddress::parse("/demo.module/node.playlist").expect("valid address")
    }

    /// [`super::kind_face`] for a HEALTHY node — the status detail is only
    /// read for the shader face's space mismatch (D1), and every test that
    /// cares calls the real function directly. Shadows the glob import on
    /// purpose so the face tests stay about faces.
    fn kind_face(
        ty: &str,
        address: &ProjectNodeAddress,
        sections: &[UiNodeSection],
        children: &mut Vec<UiNodeChild>,
    ) -> Option<UiNodeFace> {
        super::kind_face(ty, address, sections, children, None)
    }

    #[test]
    fn shader_face_builds_knobs_from_bound_uniforms() {
        let sections = shader_sections();

        let face =
            kind_face("shader", &test_address(), &sections, &mut Vec::new()).expect("shader face");
        let UiNodeFace::Shader(face) = face else {
            panic!("expected a shader face");
        };

        assert_eq!(face.preview.kind, UiProductKind::Visual);
        assert_eq!(face.controls.len(), 1, "only the bound uniform");
        let control = &face.controls[0];
        assert_eq!(control.label, "Speed");
        assert_eq!(
            control.widget,
            UiPanelWidget::Knob {
                min: 0.0,
                max: 4.0,
                step: None
            }
        );
        assert_eq!(control.value.kind, UiSlotValueKind::F32(2.0));
        assert_eq!(
            control
                .address
                .as_ref()
                .expect("knob edits are addressed")
                .path
                .to_string(),
            "consumed[speed].default.some",
            "knob edits the uniform default's interior slot"
        );
        assert!(face.agent.is_none(), "agent decoration is studio-level");
    }

    #[test]
    fn integer_uniform_knobs_step_by_one_and_read_on_the_grid() {
        // A `count` uniform ("how many meteors"): u32-shaped, 1..=4, with an
        // off-grid stored default the panel must not repeat back.
        let sections = vec![
            UiNodeSection::ProducedProducts(vec![UiProducedProduct::visual("Output")]),
            UiNodeSection::ConfigSlots(vec![
                UiConfigSlot::record(
                    "consumed",
                    "Consumed",
                    vec![uniform_record("count", "u32", 2.37, 1.0, 4.0, None)],
                ),
                bound_row("count", channel_endpoint("bus:count", "count", 1)),
            ]),
        ];

        let Some(UiNodeFace::Shader(face)) =
            kind_face("shader", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a shader face");
        };
        assert_eq!(
            face.controls[0].widget,
            UiPanelWidget::Knob {
                min: 1.0,
                max: 4.0,
                step: Some(1.0)
            },
            "an integer-shaped uniform snaps to whole numbers"
        );
        assert_eq!(
            face.controls[0].value.kind,
            UiSlotValueKind::F32(2.0),
            "the off-grid stored default reads on the grid"
        );
        assert_eq!(face.controls[0].value.display, "2.0");
    }

    #[test]
    fn authored_step_beats_the_shape_and_float_uniforms_stay_continuous() {
        let sections = |name: &str, uniform| {
            vec![
                UiNodeSection::ProducedProducts(vec![UiProducedProduct::visual("Output")]),
                UiNodeSection::ConfigSlots(vec![
                    UiConfigSlot::record("consumed", "Consumed", vec![uniform]),
                    bound_row(name, channel_endpoint("bus:x", name, 1)),
                ]),
            ]
        };
        let step_of = |name: &str, uniform| {
            let Some(UiNodeFace::Shader(face)) = kind_face(
                "shader",
                &test_address(),
                &sections(name, uniform),
                &mut Vec::new(),
            ) else {
                panic!("expected a shader face");
            };
            let UiPanelWidget::Knob { step, .. } = face.controls[0].widget else {
                panic!("expected a knob");
            };
            step
        };

        assert_eq!(
            step_of(
                "count",
                uniform_record("count", "u32", 2.0, 1.0, 4.0, Some(2.0))
            ),
            Some(2.0),
            "an authored step overrides the shape's implied 1"
        );
        assert_eq!(
            step_of(
                "speed",
                uniform_record("speed", "f32", 2.37, 0.0, 4.0, None)
            ),
            None,
            "a plain f32 uniform keeps sliding continuously"
        );
    }

    /// A uniform record row: value shape, default, range, and an optional
    /// authored step. Whether it reaches the panel is a BINDING question
    /// (Q13), answered by the companion [`bound_row`].
    fn uniform_record(
        name: &str,
        shape: &str,
        default: f32,
        min: f32,
        max: f32,
        step: Option<f32>,
    ) -> UiConfigSlot {
        let prefix = format!("consumed[{name}]");
        let mut fields = vec![
            UiConfigSlot::value(
                format!("{prefix}.kind"),
                "Kind",
                UiSlotValue::string("value"),
            ),
            UiConfigSlot::value(
                format!("{prefix}.value"),
                "Value",
                UiSlotValue::string(shape),
            ),
            option_f32(&format!("{prefix}.default"), "Default", default),
            option_f32(&format!("{prefix}.min"), "Min", min),
            option_f32(&format!("{prefix}.max"), "Max", max),
            UiConfigSlot::value(
                format!("{prefix}.label"),
                "Label",
                UiSlotValue::string(name),
            ),
        ];
        if let Some(step) = step {
            fields.push(option_f32(&format!("{prefix}.step"), "Step", step));
        }
        UiConfigSlot::record(prefix.clone(), name, fields).with_address(address(&prefix))
    }

    #[test]
    fn bound_uniform_wears_the_binding_rows_violet_aspect() {
        let sections = shader_sections_with(channel_endpoint("bus:time", "time", 2));

        let Some(UiNodeFace::Shader(face)) =
            kind_face("shader", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a shader face");
        };
        assert!(
            face.controls[0].bound(),
            "the knob rolls up the binding row's Bound affordance"
        );
    }

    #[test]
    fn a_bus_wired_uniform_gets_a_panel_write_target() {
        // panel.md P8: the knob for a uniform that CONSUMES a bus channel
        // writes that (scope, channel) down the command channel instead of
        // editing the authored default it can no longer affect.
        let sections = shader_sections_with(channel_endpoint("bus:glow", "glow", 3));

        let Some(UiNodeFace::Shader(face)) =
            kind_face("shader", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a shader face");
        };
        let target = face.controls[0]
            .panel_target
            .as_ref()
            .expect("a bus-wired uniform's knob targets its channel");
        assert_eq!(target.channel, "glow");
        assert_eq!(
            target.scope,
            lpc_wire::WireScopeRef::Module {
                owner: lpc_model::NodeId::new(3)
            }
        );
    }

    /// Q13, binding-is-publicity: an UNBOUND uniform gets no knob anywhere
    /// (there is no authored flag left to ask), while a bound one keeps its
    /// slot address so the authored default is still editable behind the
    /// panel write.
    #[test]
    fn only_bound_uniforms_get_knobs_and_they_keep_their_slot_address() {
        let Some(UiNodeFace::Shader(face)) = kind_face(
            "shader",
            &test_address(),
            &shader_sections(),
            &mut Vec::new(),
        ) else {
            panic!("expected a shader face");
        };
        // `time` is unbound in the fixture; only `speed` reaches the panel.
        assert_eq!(face.controls.len(), 1);
        assert_eq!(face.controls[0].label, "Speed");
        assert_eq!(
            face.controls[0]
                .panel_target
                .as_ref()
                .expect("membership IS the target")
                .channel,
            "speed"
        );
        assert!(
            face.controls[0].address.is_some(),
            "and it still has a slot address to edit"
        );

        // Strip the binding row and the knob goes with it.
        let mut unbound = shader_sections();
        if let UiNodeSection::ConfigSlots(rows) = &mut unbound[1] {
            rows.retain(|row| row.key != "speed");
        }
        let Some(UiNodeFace::Shader(face)) =
            kind_face("shader", &test_address(), &unbound, &mut Vec::new())
        else {
            panic!("expected a shader face");
        };
        assert!(
            face.controls.is_empty(),
            "an unbound uniform is not on the panel, got {:?}",
            face.controls
        );
    }

    /// GV fix 1: a uniform reached only through its own `default_bind`
    /// (`WireBindingOrigin::Default`, surfaced as `default_origin` on the
    /// endpoint) is NOT public. fyeah's `time` is exactly this shape — the
    /// shader shape binds it to `bus:time`, and a time knob is noise on
    /// every panel it would reach.
    #[test]
    fn a_default_bound_uniform_gets_no_knob() {
        let sections = shader_sections_with(
            channel_endpoint("bus:time", "time", 2)
                .with_default_origin()
                .with_live_value("12.5"),
        );

        let Some(UiNodeFace::Shader(face)) =
            kind_face("shader", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a shader face");
        };
        assert!(
            face.controls.is_empty(),
            "default-origin wiring is not publicity, got {:?}",
            face.controls
                .iter()
                .map(|control| control.label.as_str())
                .collect::<Vec<_>>()
        );

        // The same row, AUTHORED, is public — the exclusion turns on the
        // origin flag alone.
        let authored = shader_sections_with(channel_endpoint("bus:time", "time", 2));
        let Some(UiNodeFace::Shader(face)) =
            kind_face("shader", &test_address(), &authored, &mut Vec::new())
        else {
            panic!("expected a shader face");
        };
        assert_eq!(face.controls.len(), 1);
    }

    #[test]
    fn bound_uniform_mirrors_the_wired_rows_live_reading() {
        // The binding-derived row, decorated with the channel's quantized
        // live reading by the project walk (P6 item 1).
        let sections = shader_sections_with(
            channel_endpoint("bus:master-tempo", "master-tempo", 4).with_live_value("2.72"),
        );

        let Some(UiNodeFace::Shader(face)) =
            kind_face("shader", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a shader face");
        };
        assert_eq!(
            face.controls[0].live_value.as_deref(),
            Some("2.72"),
            "the display-only live reading rides the control"
        );
        assert_eq!(
            face.controls[0].value.kind,
            UiSlotValueKind::F32(2.0),
            "the authored default stays the edit target"
        );
    }

    #[test]
    fn fixture_face_projects_the_panel_fader_at_the_interior_address() {
        let sections = fixture_sections();

        let Some(UiNodeFace::Fixture(face)) =
            kind_face("fixture", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a fixture face");
        };

        assert_eq!(face.preview.kind, UiProductKind::Control);
        assert_eq!(
            face.brightness.widget,
            UiPanelWidget::Fader {
                min: 0.0,
                max: 255.0,
                step: Some(1.0)
            }
        );
        assert_eq!(
            face.brightness
                .address
                .as_ref()
                .expect("fader edits are addressed")
                .path
                .to_string(),
            "brightness.some"
        );
        assert!(
            face.brightness
                .aspects
                .iter()
                .any(|aspect| aspect.kind == UiSlotAspectKind::Optionality),
            "the fader's popover keeps the option row's aspects"
        );
    }

    #[test]
    fn other_kinds_and_faceless_sections_stay_generic() {
        assert_eq!(
            kind_face(
                "clock",
                &test_address(),
                &shader_sections(),
                &mut Vec::new()
            ),
            None
        );
        assert_eq!(
            kind_face(
                "playlist",
                &test_address(),
                &shader_sections(),
                &mut Vec::new()
            ),
            None
        );
        // A shader with no produced visual row keeps the sections view.
        let no_products = vec![UiNodeSection::ConfigSlots(Vec::new())];
        assert_eq!(
            kind_face("shader", &test_address(), &no_products, &mut Vec::new()),
            None
        );
        // A fixture whose rows carry no mappable editor hint keeps the
        // sections view.
        let unflagged = vec![
            UiNodeSection::ProducedProducts(vec![UiProducedProduct::control("Output")]),
            UiNodeSection::ConfigSlots(vec![UiConfigSlot::value(
                "brightness",
                "Brightness",
                UiSlotValue::u32(64),
            )]),
        ];
        assert_eq!(
            kind_face("fixture", &test_address(), &unflagged, &mut Vec::new()),
            None
        );
    }

    #[test]
    fn playlist_face_derives_the_strip_and_keeps_only_the_active_child() {
        let sections = playlist_sections(Some(1));
        let mut children = playlist_children();

        let Some(UiNodeFace::Playlist(face)) =
            kind_face("playlist", &test_address(), &sections, &mut children)
        else {
            panic!("expected a playlist face");
        };

        assert_eq!(face.active, Some(1), "ACTIVE follows the produced status");
        assert_eq!(face.entries.len(), 2);
        let idle = &face.entries[0];
        assert_eq!((idle.key, idle.name.as_str()), (1, "idle"));
        assert_eq!(idle.duration_ms, None);
        assert!(!idle.cue);
        let cued = &face.entries[1];
        assert_eq!((cued.key, cued.name.as_str()), (2, "active"));
        assert_eq!(cued.duration_ms, Some(4000), "authored seconds → ms");
        assert!(cued.cue, "non-empty trigger_ids reads as a cue entry");
        assert!(
            cued.thumb.is_some(),
            "the non-active entry reuses its child's cached snapshot"
        );
        assert!(
            cued.action.is_some(),
            "strip click reuses the child's node-select action"
        );

        // The one-live-surface rule: only the ACTIVE entry's child remains.
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].label, "Idle");
        assert!(
            !children[0].active,
            "the child card must NOT wear the selection look (the web maps \
             `active` onto pane focus); the strip placard presents active-ness"
        );
    }

    #[test]
    fn playlist_without_active_status_keeps_all_children_and_no_face() {
        let sections = playlist_sections(None);
        let mut children = playlist_children();

        assert_eq!(
            kind_face("playlist", &test_address(), &sections, &mut children),
            None
        );
        assert_eq!(children.len(), 2, "fallback renders every child as today");
    }

    #[test]
    fn playlist_with_no_entries_derives_an_empty_strip_face() {
        // A freshly created playlist: entries map present but empty, and no
        // entry children. The face must still derive (the strip's empty
        // state carries the add affordance) instead of falling back to the
        // generic sections view.
        let mut sections = playlist_sections(Some(1));
        if let UiNodeSection::ConfigSlots(rows) = &mut sections[2]
            && let UiConfigSlotBody::Record(entries) = &mut rows[0].body
        {
            entries.fields.clear();
        }
        let mut children = Vec::new();

        let Some(UiNodeFace::Playlist(face)) =
            kind_face("playlist", &test_address(), &sections, &mut children)
        else {
            panic!("expected an empty playlist face");
        };
        assert!(face.entries.is_empty());
        assert_eq!(face.active, None);

        // Same shape without the produced status yet: still the empty face.
        let mut early = playlist_sections(None);
        if let UiNodeSection::ConfigSlots(rows) = &mut early[1]
            && let UiConfigSlotBody::Record(entries) = &mut rows[0].body
        {
            entries.fields.clear();
        }
        assert!(matches!(
            kind_face("playlist", &test_address(), &early, &mut Vec::new()),
            Some(UiNodeFace::Playlist(_))
        ));
    }

    #[test]
    fn playlist_whose_active_entry_has_no_mounted_child_falls_back() {
        // Status names entry 7: not in the strip, no mounted child.
        let sections = playlist_sections(Some(7));
        let mut children = playlist_children();

        assert_eq!(
            kind_face("playlist", &test_address(), &sections, &mut children),
            None
        );
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn unnamed_playlist_entries_fall_back_to_the_loader_naming_rule() {
        // Entry 3 has no authored name; its child mounts as `entry_3`.
        let mut sections = playlist_sections(Some(1));
        if let UiNodeSection::ConfigSlots(rows) = &mut sections[2]
            && let UiConfigSlotBody::Record(entries) = &mut rows[0].body
        {
            entries.fields.push(UiConfigSlot::record(
                "entries[3]",
                "3",
                vec![UiConfigSlot::value(
                    "entries[3].fade_after",
                    "Fade after",
                    UiSlotValue::f32(0.5),
                )],
            ));
        }
        let mut children = playlist_children();
        children.push(child("entry_3", "Entry 3"));

        let Some(UiNodeFace::Playlist(face)) =
            kind_face("playlist", &test_address(), &sections, &mut children)
        else {
            panic!("expected a playlist face");
        };
        let entry = face.entries.iter().find(|entry| entry.key == 3).unwrap();
        assert_eq!(entry.name, "Entry 3", "falls back to the child's label");
        assert!(entry.action.is_some(), "matched via the entry_<key> rule");
    }

    #[test]
    fn output_face_projects_every_wire_with_its_slice_and_edit_addresses() {
        let sections = output_sections(&[
            (0, "ws281x:local:IO18", Some(100)),
            (1, "ws281x:local:IO2", None),
        ]);

        let Some(UiNodeFace::Output(face)) =
            kind_face("output", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected an output face");
        };

        assert_eq!(face.channels.len(), 2);
        let first = &face.channels[0];
        assert_eq!((first.key, first.pin_label.as_str()), (0, "IO18"));
        assert_eq!(first.endpoint_display, "ws281x:local:IO18");
        assert_eq!((first.count, first.resolved_count), (Some(100), Some(100)));
        assert_eq!(first.slice_start, Some(0));
        assert_eq!(
            slot_path(&first.endpoint_address),
            "channels[0].endpoint",
            "the wire is edited through the normal slot write path"
        );
        assert_eq!(
            slot_path(&first.count_address),
            "channels[0].count.some",
            "a PRESENT count edits its interior option slot"
        );

        let second = &face.channels[1];
        assert_eq!((second.key, second.pin_label.as_str()), (1, "IO2"));
        assert_eq!(second.count, None, "the highest key may omit its count");
        assert_eq!(
            second.resolved_count, None,
            "the remainder waits for the decoration pass to say how big it is"
        );
        assert_eq!(
            second.slice_start,
            Some(100),
            "slices start where the previous one ended"
        );
        assert_eq!(
            second.count_address, None,
            "an ABSENT option has no value address — including it is the toggle"
        );

        assert_eq!(face.input_binding.as_deref(), Some("bus:control.out"));
        assert_eq!(slot_path(&face.channels_address), "channels");
        // Hardware and upstream facts are the decoration pass's business.
        assert_eq!(face.board, None);
        assert_eq!(face.total_lamps, None);
        assert!(face.channels.iter().all(|channel| channel.gpio.is_none()));
        assert_eq!(face.authored_lamps(), None, "the remainder is open-ended");
    }

    #[test]
    fn the_single_countless_channel_takes_the_whole_extent() {
        // The shape every format-2 output migrated into: one wire, no count.
        let sections = output_sections(&[(0, "ws281x:local:D10", None)]);

        let Some(UiNodeFace::Output(mut face)) =
            kind_face("output", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected an output face");
        };
        assert_eq!(face.channels[0].slice_start, Some(0));
        assert_eq!(face.channels[0].resolved_count, None);

        // What the decoration pass then does with a 241-lamp buffer.
        face.resolve_extent(241);
        assert_eq!(face.total_lamps, Some(241));
        assert_eq!(face.channels[0].resolved_count, Some(241));
    }

    #[test]
    fn fully_counted_channels_total_without_any_runtime_help() {
        let sections = output_sections(&[
            (0, "ws281x:local:IO18", Some(60)),
            (1, "ws281x:local:IO16", Some(60)),
            (2, "ws281x:local:IO14", Some(30)),
        ]);

        let Some(UiNodeFace::Output(face)) =
            kind_face("output", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected an output face");
        };
        assert_eq!(
            face.channels
                .iter()
                .map(|channel| channel.slice_start)
                .collect::<Vec<_>>(),
            [Some(0), Some(60), Some(120)]
        );
        assert_eq!(face.authored_lamps(), Some(150));
    }

    #[test]
    fn an_empty_channels_map_still_derives_a_face_and_a_missing_one_does_not() {
        // No wires yet: the face's empty state (with the map's own add
        // affordance) is the card's surface, exactly like a new playlist.
        let empty = output_sections(&[]);
        let Some(UiNodeFace::Output(face)) =
            kind_face("output", &test_address(), &empty, &mut Vec::new())
        else {
            panic!("expected an empty output face");
        };
        assert!(face.channels.is_empty());
        assert_eq!(face.authored_lamps(), Some(0));

        // No `channels` row at all is not an output card we understand.
        let no_channels = vec![UiNodeSection::ConfigSlots(Vec::new())];
        assert_eq!(
            kind_face("output", &test_address(), &no_channels, &mut Vec::new()),
            None
        );
    }

    #[test]
    fn an_unparseable_endpoint_leaves_the_pin_unresolved_but_shown() {
        let sections = output_sections(&[(0, "nonsense", Some(4))]);

        let Some(UiNodeFace::Output(face)) =
            kind_face("output", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected an output face");
        };
        assert_eq!(face.channels[0].endpoint_display, "nonsense");
        assert_eq!(face.channels[0].pin_label, "");
    }

    // -- fixtures ------------------------------------------------------------

    fn slot_path(address: &Option<ProjectSlotAddress>) -> String {
        address
            .as_ref()
            .expect("the row is addressed")
            .path
            .to_string()
    }

    /// Output sections as the project walk projects them: the bound `input`
    /// product row plus the `channels` map, one record per authored wire.
    fn output_sections(wires: &[(u32, &str, Option<u32>)]) -> Vec<UiNodeSection> {
        let entries = wires
            .iter()
            .map(|(key, endpoint, count)| {
                let prefix = format!("channels[{key}]");
                let endpoint_key = format!("{prefix}.endpoint");
                let count_key = format!("{prefix}.count");
                let count_row = match count {
                    Some(count) => {
                        UiConfigSlot::value(&count_key, "Count", UiSlotValue::u32(*count))
                            .with_address(address(&count_key))
                            .with_optionality(UiSlotOptionality::included(true))
                    }
                    None => UiConfigSlot::empty(&count_key, "Count")
                        .with_address(address(&count_key))
                        .with_optionality(UiSlotOptionality::excluded(true)),
                };
                UiConfigSlot::record(
                    prefix.clone(),
                    key.to_string(),
                    vec![
                        UiConfigSlot::value(
                            &endpoint_key,
                            "Endpoint",
                            UiSlotValue::string(*endpoint),
                        )
                        .with_address(address(&endpoint_key)),
                        count_row,
                    ],
                )
                .with_address(address(&prefix))
            })
            .collect();

        vec![UiNodeSection::ConfigSlots(vec![
            UiConfigSlot::value("input", "Input", UiSlotValue::unset()).with_source(
                UiSlotSourceState::Bound(UiBindingEndpoint::new("bus:control.out")),
            ),
            UiConfigSlot::record("channels", "Channels", entries).with_address(address("channels")),
        ])]
    }

    fn address(path: &str) -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.module/node.shader").expect("valid address"),
            ProjectSlotRoot::Def,
            SlotPath::parse(path).expect("valid path"),
        )
    }

    fn option_f32(key: &str, label: &str, value: f32) -> UiConfigSlot {
        UiConfigSlot::value(key, label, UiSlotValue::f32(value))
            .with_address(address(key))
            .with_optionality(UiSlotOptionality::included(true))
    }

    /// Shader sections with `speed` and `time` uniforms, where `speed` is
    /// wired to `bus:speed` — which, since Q13, is the ONLY thing that puts
    /// it on the panel. `time` stays unbound and therefore knob-less.
    fn shader_sections() -> Vec<UiNodeSection> {
        shader_sections_with(channel_endpoint("bus:speed", "speed", 1))
    }

    /// The same sections with `speed`'s binding-derived row carrying
    /// `endpoint` — the seam every Q13 case varies.
    fn shader_sections_with(endpoint: UiBindingEndpoint) -> Vec<UiNodeSection> {
        let uniform = |name: &str| {
            let prefix = format!("consumed[{name}]");
            let fields = vec![
                UiConfigSlot::value(
                    format!("{prefix}.kind"),
                    "Kind",
                    UiSlotValue::string("value"),
                ),
                option_f32(&format!("{prefix}.default"), "Default", 2.0),
                option_f32(&format!("{prefix}.min"), "Min", 0.0)
                    .with_state(UiSlotFieldState::editable()),
                option_f32(&format!("{prefix}.max"), "Max", 4.0),
                UiConfigSlot::value(
                    format!("{prefix}.label"),
                    "Label",
                    UiSlotValue::string("Speed"),
                ),
            ];
            UiConfigSlot::record(prefix.clone(), name, fields).with_address(address(&prefix))
        };

        vec![
            UiNodeSection::ProducedProducts(vec![UiProducedProduct::visual("Output")]),
            UiNodeSection::ConfigSlots(vec![
                UiConfigSlot::record(
                    "consumed",
                    "Consumed",
                    vec![uniform("speed"), uniform("time")],
                ),
                bound_row("speed", endpoint),
            ]),
        ]
    }

    // -- phasor period knob (P7 item 5) -----------------------------------

    /// A `phasor`-kind uniform record with an authored config.
    fn phasor_uniform(name: &str, period: f32, waveform: &str, offset: f32) -> UiConfigSlot {
        let prefix = format!("consumed[{name}]");
        let config = UiSlotValue {
            kind: UiSlotValueKind::Struct {
                name: Some("PhasorConfig".to_string()),
                fields: vec![
                    ("period_seconds".to_string(), UiSlotValue::f32(period)),
                    ("waveform".to_string(), UiSlotValue::string(waveform)),
                    ("phase_offset".to_string(), UiSlotValue::f32(offset)),
                ],
            },
            ..UiSlotValue::f32(period)
        };
        let fields = vec![
            UiConfigSlot::value(
                format!("{prefix}.kind"),
                "Kind",
                UiSlotValue::string("phasor"),
            ),
            UiConfigSlot::value(format!("{prefix}.phasor"), "Phasor", config)
                .with_address(address(&format!("{prefix}.phasor")))
                .with_optionality(UiSlotOptionality::included(true)),
            option_f32(&format!("{prefix}.default"), "Default", 0.0),
            UiConfigSlot::value(
                format!("{prefix}.label"),
                "Label",
                UiSlotValue::string("Phase"),
            ),
        ];
        UiConfigSlot::record(prefix.clone(), name, fields).with_address(address(&prefix))
    }

    fn phasor_sections(uniform: UiConfigSlot, wiring: Option<UiConfigSlot>) -> Vec<UiNodeSection> {
        let mut rows = vec![UiConfigSlot::record("consumed", "Consumed", vec![uniform])];
        rows.extend(wiring);
        vec![
            UiNodeSection::ProducedProducts(vec![UiProducedProduct::visual("Output")]),
            UiNodeSection::ConfigSlots(rows),
        ]
    }

    fn phasor_face(sections: &[UiNodeSection]) -> UiShaderFace {
        let Some(UiNodeFace::Shader(face)) =
            kind_face("shader", &test_address(), sections, &mut Vec::new())
        else {
            panic!("expected a shader face");
        };
        face
    }

    /// A phasor slot gets ONE knob — the period, in seconds — and it gets it
    /// whether or not the slot is wired: unlike a value uniform's default,
    /// a period is the card's own affordance. Slot-local means the gesture
    /// is an ordinary slot edit at `…phasor.some`.
    #[test]
    fn a_phasor_slot_gets_exactly_one_knob_and_it_edits_the_config_slot() {
        let face = phasor_face(&phasor_sections(
            phasor_uniform("phase", 20.0, "ramp", 0.0),
            None,
        ));

        assert_eq!(face.controls.len(), 1, "one control per phasor slot");
        let control = &face.controls[0];
        assert_eq!(
            control.label, "Period",
            "a lone phasor knob wears the plain label"
        );
        assert_eq!(control.value.kind, UiSlotValueKind::F32(20.0));
        assert_eq!(
            control.unit, None,
            "the auto-denominated rate readout carries its unit in the string"
        );
        assert_eq!(
            control.widget,
            UiPanelWidget::Knob {
                min: 0.0,
                max: PHASOR_PERIOD_MAX_SECONDS,
                step: None,
            },
            "unauthored range falls back to 0..3600 s, continuous"
        );
        assert_eq!(
            control
                .address
                .as_ref()
                .expect("period edits are addressed")
                .path
                .to_string(),
            "consumed[phase].phasor.some",
            "the knob edits the interior config slot"
        );
        assert!(
            control.panel_target.is_none(),
            "unwired: the knob is the card's own, not a panel entry"
        );
    }

    /// The gesture writes a WHOLE config, so the slot's shaping has to ride
    /// along — a panel may move the period and nothing else (settled D11 v1).
    #[test]
    fn the_period_knob_carries_the_slots_shaping_through_the_gesture() {
        let face = phasor_face(&phasor_sections(
            phasor_uniform("phase", 4.0, "triangle", 0.25),
            None,
        ));

        assert_eq!(
            face.controls[0].emit,
            crate::UiPanelEmit::PhasorPeriod {
                waveform: lpc_model::Waveform::Triangle,
                phase_offset: 0.25,
            }
        );
    }

    /// Publicity follows the ordinary derived rules: an AUTHORED binding on
    /// the phasor slot puts the knob on the enclosing module's panel and
    /// turns its gestures into panel writes onto the config channel (parent
    /// D3 — one shared integrator for every reader). `default_bind`-only
    /// wiring is not publicity, so the knob stays on the card.
    #[test]
    fn phasor_publicity_follows_the_authored_binding() {
        let authored = phasor_face(&phasor_sections(
            phasor_uniform("phase", 100.0, "ramp", 0.0),
            Some(bound_row(
                "phase",
                channel_endpoint("bus:speed", "speed", 3),
            )),
        ));
        let target = authored.controls[0]
            .panel_target
            .as_ref()
            .expect("an authored config channel is a panel write target");
        assert_eq!(target.channel, "speed");

        let defaulted = phasor_face(&phasor_sections(
            phasor_uniform("phase", 100.0, "ramp", 0.0),
            Some(bound_row(
                "phase",
                channel_endpoint("bus:speed", "speed", 3).with_default_origin(),
            )),
        ));
        assert_eq!(defaulted.controls.len(), 1, "the card keeps its knob");
        assert!(
            defaulted.controls[0].panel_target.is_none(),
            "default-origin wiring is not publicity"
        );
    }

    /// An authored `min`/`max` beats the default range, exactly as it does
    /// for a value uniform's knob.
    #[test]
    fn an_authored_range_bounds_the_period_knob() {
        let mut uniform = phasor_uniform("phase", 20.0, "ramp", 0.0);
        if let UiConfigSlotBody::Record(record) = &mut uniform.body {
            record
                .fields
                .push(option_f32("consumed[phase].min", "Min", 1.0));
            record
                .fields
                .push(option_f32("consumed[phase].max", "Max", 30.0));
        }

        let face = phasor_face(&phasor_sections(uniform, None));

        assert_eq!(
            face.controls[0].widget,
            UiPanelWidget::Knob {
                min: 1.0,
                max: 30.0,
                step: None,
            }
        );
    }

    /// `seconds` is unbounded time: no period, no range, no default —
    /// nothing to turn, so no knob anywhere.
    #[test]
    fn a_seconds_slot_gets_no_knob() {
        let mut uniform = phasor_uniform("t", 0.0, "ramp", 0.0);
        if let UiConfigSlotBody::Record(record) = &mut uniform.body
            && let Some(kind) = record.fields.first_mut()
        {
            *kind = UiConfigSlot::value("consumed[t].kind", "Kind", UiSlotValue::string("seconds"));
        }

        let face = phasor_face(&phasor_sections(
            uniform,
            Some(bound_row("t", channel_endpoint("bus:time", "time", 3))),
        ));

        assert!(
            face.controls.is_empty(),
            "seconds has nothing to set, got {:?}",
            face.controls
                .iter()
                .map(|control| control.label.as_str())
                .collect::<Vec<_>>()
        );
    }

    /// A phasor whose config option is OFF has no slot to edit: the engine
    /// runs it on the default shaping, and turning a knob would have to
    /// perform the option-on gesture the generic row already owns.
    #[test]
    fn a_phasor_with_no_authored_config_gets_no_knob() {
        let mut uniform = phasor_uniform("phase", 20.0, "ramp", 0.0);
        if let UiConfigSlotBody::Record(record) = &mut uniform.body {
            record
                .fields
                .retain(|field| !field.key.ends_with(".phasor"));
        }

        assert!(
            phasor_face(&phasor_sections(uniform, None))
                .controls
                .is_empty()
        );
    }

    // -- palette swatch (M4 P3) --------------------------------------------

    /// A palette uniform record: `kind = palette`, the authored config on a
    /// present `gradient` option, and a label — the shape the shader
    /// projection reads.
    fn palette_uniform(name: &str, config: &lpc_model::GradientConfig) -> UiConfigSlot {
        let prefix = format!("consumed[{name}]");
        let fields = vec![
            UiConfigSlot::value(
                format!("{prefix}.kind"),
                "Kind",
                UiSlotValue::string("palette"),
            ),
            UiConfigSlot::value(
                format!("{prefix}.gradient"),
                "Gradient",
                UiSlotValue::from_lp_value(&lpc_model::ToLpValue::to_lp_value(config)),
            )
            .with_address(address(&format!("{prefix}.gradient")))
            .with_optionality(UiSlotOptionality::included(true)),
            UiConfigSlot::value(
                format!("{prefix}.label"),
                "Label",
                UiSlotValue::string("Palette"),
            ),
        ];
        UiConfigSlot::record(prefix.clone(), name, fields).with_address(address(&prefix))
    }

    fn ramp(stops: usize) -> lpc_model::Gradient {
        lpc_model::Gradient {
            space: lpc_model::Colorspace::Oklab,
            method: lpc_model::InterpMethod::Linear,
            stops: (0..stops)
                .map(|index| lpc_model::GradientStop {
                    at: index as f32 / (stops - 1) as f32,
                    c: [index as f32 / stops as f32, 0.1, -0.1],
                })
                .collect(),
        }
    }

    /// A palette slot gets ONE swatch, whatever the wiring: like a phasor's
    /// period, the palette is the card's own affordance. Slot-local means
    /// the pick is an ordinary slot edit at `…gradient.some`, and the whole
    /// config is what the control carries.
    #[test]
    fn a_palette_slot_gets_one_swatch_editing_its_config_slot() {
        let config = lpc_model::GradientConfig::Static(ramp(3));
        let face = phasor_face(&phasor_sections(palette_uniform("palette", &config), None));

        assert_eq!(face.controls.len(), 1, "one control per palette slot");
        let control = &face.controls[0];
        assert_eq!(control.label, "Palette");
        assert_eq!(control.widget, UiPanelWidget::PaletteSwatch);
        assert_eq!(control.emit, crate::UiPanelEmit::Gradient);
        assert_eq!(control.unit, None);
        assert_eq!(
            control.gradient_config(),
            Some(config),
            "the control carries the WHOLE config, not a field of it"
        );
        assert_eq!(
            control
                .address
                .as_ref()
                .expect("palette picks are addressed")
                .path
                .to_string(),
            "consumed[palette].gradient.some",
        );
        assert!(
            control.panel_target.is_none(),
            "unwired: the swatch is the card's own, not a panel entry"
        );
        // The row's display is the palette summary (P2), never the padded
        // 24-entry storage dump.
        assert_eq!(control.value.display, "oklab \u{b7} linear \u{b7} 3 stops");
    }

    /// The realistic authored case: a hand-authored palette slot usually
    /// arrives with the `gradient` option ABSENT (just a `default_bind`) —
    /// and still gets its swatch, seeded with the same default the engine
    /// runs the slot on.
    /// The first pick's `AssignValue` at `…gradient.some` materializes the
    /// option (the overlay's ensure-present rule).
    #[test]
    fn an_absent_gradient_option_still_gets_a_default_seeded_swatch() {
        let prefix = "consumed[palette]";
        let fields = vec![
            UiConfigSlot::value(
                format!("{prefix}.kind"),
                "Kind",
                UiSlotValue::string("palette"),
            ),
            UiConfigSlot::value(
                format!("{prefix}.gradient"),
                "Gradient",
                UiSlotValue::unset(),
            )
            .with_address(address(&format!("{prefix}.gradient")))
            .with_optionality(UiSlotOptionality::excluded(true)),
            UiConfigSlot::value(
                format!("{prefix}.label"),
                "Label",
                UiSlotValue::string("Palette"),
            ),
        ];
        let uniform = UiConfigSlot::record(prefix.to_string(), "palette", fields)
            .with_address(address(prefix));

        let face = phasor_face(&phasor_sections(uniform, None));

        assert_eq!(face.controls.len(), 1, "the absent option still presents");
        let control = &face.controls[0];
        assert_eq!(control.widget, UiPanelWidget::PaletteSwatch);
        assert_eq!(
            control.gradient_config(),
            Some(lpc_model::GradientConfig::default()),
            "the swatch shows what the engine actually runs"
        );
        assert_eq!(
            control
                .address
                .as_ref()
                .expect("the first pick lands at the option's some")
                .path
                .to_string(),
            "consumed[palette].gradient.some",
        );
    }

    /// Publicity is the ordinary derived rule, and a wired palette's live
    /// reading is the channel's own config summary.
    #[test]
    fn palette_publicity_follows_the_authored_binding() {
        let config = lpc_model::GradientConfig::Cycle {
            set: vec![ramp(2), ramp(3)],
            step_seconds: 20.0,
            fade_seconds: 0.5,
        };
        let mut endpoint = channel_endpoint("bus:palette", "palette", 3);
        endpoint.live_value = crate::app::project::format_live_panel_value(
            &lpc_model::ToLpValue::to_lp_value(&config),
        );
        let face = phasor_face(&phasor_sections(
            palette_uniform("palette", &config),
            Some(bound_row("palette", endpoint)),
        ));

        let control = &face.controls[0];
        assert_eq!(
            control
                .panel_target
                .as_ref()
                .expect("an authored config channel is a panel write target")
                .channel,
            "palette"
        );
        assert_eq!(
            control.live_value.as_deref(),
            Some("cycle \u{b7} 2 palettes \u{b7} every 20 s \u{b7} 0.5 s fade"),
            "a driven palette reads back as words; the strip keeps the authored config"
        );
    }

    /// An absent `gradient` option has no slot to edit (the engine runs the
    /// default palette), and a non-gradient payload has nothing to sample —
    /// both fall back to the generic row rather than an empty swatch.
    #[test]
    fn a_palette_without_a_readable_config_gets_no_swatch() {
        let config = lpc_model::GradientConfig::Static(ramp(2));
        let mut absent = palette_uniform("palette", &config);
        if let UiConfigSlotBody::Record(record) = &mut absent.body {
            record
                .fields
                .retain(|field| !field.key.ends_with(".gradient"));
        }
        assert!(
            phasor_face(&phasor_sections(absent, None))
                .controls
                .is_empty(),
            "option off: nothing to pick into"
        );

        let mut mis_shaped = palette_uniform("palette", &config);
        if let UiConfigSlotBody::Record(record) = &mut mis_shaped.body
            && let Some(row) = record
                .fields
                .iter_mut()
                .find(|field| field.key.ends_with(".gradient"))
        {
            row.body = UiConfigSlotBody::Value(UiSlotValue::f32(0.5));
        }
        assert!(
            phasor_face(&phasor_sections(mis_shaped, None))
                .controls
                .is_empty(),
            "a non-palette payload behind a palette slot renders no swatch"
        );
    }

    /// The hint mapping is the other entry point: any `Gradient`-hinted
    /// value row reads as a swatch, the same hint the slot row draws its
    /// read-only strip from (P2).
    #[test]
    fn the_gradient_hint_maps_to_the_swatch_widget() {
        let value = UiSlotValue::from_lp_value(&lpc_model::ToLpValue::to_lp_value(
            &lpc_model::GradientConfig::Static(ramp(4)),
        ))
        .with_editor(UiSlotEditorHint::Gradient);
        let slot = UiConfigSlot::value("palette", "Palette", value);

        assert_eq!(panel_widget(&slot), Some(UiPanelWidget::PaletteSwatch));
    }

    // -- clock face (P7 item 4) --------------------------------------------

    /// The clock's face is the published handle plus the tiny seconds
    /// readout (clock-face v2 — the Delta row is gone outright); the trace
    /// cards arrive later, from the timebase probe, so a freshly derived
    /// face is `Unread` with no cards — NOT an empty listing, which would
    /// read as "nothing is running".
    #[test]
    fn clock_face_derives_the_product_and_waits_for_the_timebase_probe() {
        let sections = vec![
            UiNodeSection::ProducedProducts(vec![
                UiProducedProduct::time("Product").with_detail("node 2 output 0"),
            ]),
            UiNodeSection::ProducedValues(vec![
                UiProducedValue::new("Seconds", "3.5").with_key("seconds"),
                UiProducedValue::new("Delta seconds", "0.033").with_key("delta_seconds"),
            ]),
        ];

        let Some(UiNodeFace::Clock(face)) =
            kind_face("clock", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a clock face");
        };
        assert_eq!(face.product.kind, UiProductKind::Time);
        assert_eq!(
            face.transport, None,
            "no Debug rows yet — the transport block waits, same as the probe"
        );
        assert_eq!(face.timebase, crate::UiTimebaseState::Unread);
        assert!(face.phasors.is_empty());
    }

    fn transport_row(field: &str, value: UiSlotValue) -> UiConfigSlot {
        let key = format!("transport.{field}");
        UiConfigSlot::value(&key, field, value).with_address(clock_slot_address(&key))
    }

    fn clock_slot_address(path: &str) -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.module/node.clock").expect("valid address"),
            ProjectSlotRoot::Def,
            SlotPath::parse(path).expect("valid path"),
        )
    }

    /// The transport block lifts value + address + editability straight off
    /// the flattened Debug rows — the tape widgets' whole read surface.
    #[test]
    fn clock_face_lifts_the_transport_block_from_the_debug_rows() {
        let sections = vec![
            UiNodeSection::ProducedProducts(vec![
                UiProducedProduct::time("Product").with_detail("node 2 output 0"),
            ]),
            UiNodeSection::DebugSlots(vec![
                transport_row("play_state", UiSlotValue::string("paused")),
                transport_row("rate", UiSlotValue::f32(2.0)),
                transport_row("scrub_offset_seconds", UiSlotValue::f32(-12.4)),
            ]),
        ];

        let Some(UiNodeFace::Clock(face)) =
            kind_face("clock", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a clock face");
        };
        let transport = face.transport.expect("three rows present → block present");
        assert_eq!(transport.play_state, lpc_model::PlayState::Paused);
        assert_eq!(transport.rate, 2.0);
        assert_eq!(transport.scrub_offset_seconds, -12.4);
        assert_eq!(transport.seconds, 0.0, "numeric seconds is probe-only");
        assert_eq!(
            transport.rate_address,
            Some(clock_slot_address("transport.rate")),
            "editable row → dispatch address"
        );
        assert!(transport.play_state_address.is_some());
        assert!(transport.scrub_address.is_some());
    }

    /// A read-only row keeps its value but withholds the dispatch address —
    /// the widgets render inert chrome instead of dead handlers (P4).
    #[test]
    fn a_read_only_transport_row_withholds_its_dispatch_address() {
        let read_only = UiSlotFieldState {
            editable: false,
            ..UiSlotFieldState::editable()
        };
        let sections = vec![
            UiNodeSection::ProducedProducts(vec![
                UiProducedProduct::time("Product").with_detail("node 2 output 0"),
            ]),
            UiNodeSection::DebugSlots(vec![
                transport_row("play_state", UiSlotValue::string("playing")),
                transport_row("rate", UiSlotValue::f32(1.0)).with_state(read_only),
                transport_row("scrub_offset_seconds", UiSlotValue::f32(0.0)),
            ]),
        ];

        let Some(UiNodeFace::Clock(face)) =
            kind_face("clock", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a clock face");
        };
        let transport = face.transport.expect("values still lift");
        assert_eq!(transport.rate, 1.0);
        assert_eq!(transport.rate_address, None, "read-only → no dispatch");
        assert!(
            transport.play_state_address.is_some(),
            "siblings unaffected"
        );
    }

    /// A dirty Debug row (an active session override) lifts its own edit
    /// entry as the override marker — the tape's changed-tint flag and
    /// per-value Clear target in one; clean rows lift `None`.
    #[test]
    fn an_active_override_lifts_its_clear_target() {
        let dirty = UiSlotFieldState::editable().with_dirty(crate::UiNodeDirtyState::Dirty);
        let sections = vec![
            UiNodeSection::ProducedProducts(vec![
                UiProducedProduct::time("Product").with_detail("node 2 output 0"),
            ]),
            UiNodeSection::DebugSlots(vec![
                transport_row("play_state", UiSlotValue::string("playing")),
                transport_row("rate", UiSlotValue::f32(2.0))
                    .with_state(dirty)
                    .with_edit_entry_address(clock_slot_address("transport.rate")),
                transport_row("scrub_offset_seconds", UiSlotValue::f32(0.0)),
            ]),
        ];

        let Some(UiNodeFace::Clock(face)) =
            kind_face("clock", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a clock face");
        };
        let transport = face.transport.expect("block present");
        assert_eq!(
            transport.rate_override,
            Some(clock_slot_address("transport.rate")),
            "dirty row → its edit entry is the Clear target"
        );
        assert_eq!(transport.play_state_override, None, "clean row → no tint");
        assert_eq!(transport.scrub_override, None);
    }

    // -- the grouped Transport control (P8) ---------------------------------

    /// A transport leaf as the default-binding overlay decorates it: a
    /// default-origin bound endpoint carrying the promoted `panel = "show"`
    /// hint and its own `clock.*` panel target — which is exactly what
    /// makes the leaf panel-PUBLIC.
    fn wired_transport_row(field: &str, value: UiSlotValue, channel: &str) -> UiConfigSlot {
        let endpoint = crate::UiBindingEndpoint::new(format!("bus:{channel}"))
            .with_default_origin()
            .with_panel_hint()
            .with_panel_target(crate::UiPanelTarget {
                scope: test_scope(),
                channel: channel.to_string(),
                engaged: false,
            });
        transport_row(field, value).with_source(UiSlotSourceState::Bound(endpoint))
    }

    fn test_scope() -> lpc_wire::WireScopeRef {
        lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        }
    }

    fn clock_sections(rows: Vec<UiConfigSlot>) -> Vec<UiNodeSection> {
        vec![
            UiNodeSection::ProducedProducts(vec![
                UiProducedProduct::time("Product").with_detail("node 2 output 0"),
            ]),
            UiNodeSection::DebugSlots(rows),
        ]
    }

    fn clock_controls(rows: Vec<UiConfigSlot>) -> Vec<UiPanelControl> {
        let sections = clock_sections(rows);
        let Some(UiNodeFace::Clock(face)) =
            kind_face("clock", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a clock face");
        };
        face.controls
    }

    /// The default project's shape: all three leaves default-bound and
    /// promoted by the record's `panel = "show"`. ONE control — the whole
    /// faceplate — anchored on the rate channel, carrying a wire per
    /// dimension.
    #[test]
    fn a_promoted_transport_derives_exactly_one_grouped_control() {
        let controls = clock_controls(vec![
            wired_transport_row(
                "play_state",
                UiSlotValue::string("playing"),
                lpc_model::bus::well_known::CLOCK_PLAY_STATE_CHANNEL,
            ),
            wired_transport_row(
                "rate",
                UiSlotValue::f32(2.0),
                lpc_model::bus::well_known::CLOCK_RATE_CHANNEL,
            ),
            wired_transport_row(
                "scrub_offset_seconds",
                UiSlotValue::f32(0.0),
                lpc_model::bus::well_known::CLOCK_SCRUB_CHANNEL,
            ),
        ]);

        assert_eq!(controls.len(), 1, "three channels, ONE control");
        let control = &controls[0];
        assert_eq!(control.label, "Time");
        let UiPanelWidget::Transport { transport } = &control.widget else {
            panic!("the grouped control wears the Transport widget");
        };
        assert_eq!(transport.rate, 2.0, "the faceplate carries the whole block");
        // Anchor (Q22): the group's identity is the rate leaf's channel.
        assert_eq!(
            control
                .panel_target
                .as_ref()
                .map(|target| target.channel.as_str()),
            Some("clock.rate")
        );
        // And every dimension carries its own dispatch facts.
        let channels: Vec<Option<&str>> = control
            .wires
            .iter()
            .map(|wire| {
                wire.panel_target
                    .as_ref()
                    .map(|target| target.channel.as_str())
            })
            .collect();
        assert_eq!(
            channels,
            vec![
                Some("clock.rate"),
                Some("clock.play_state"),
                Some("clock.scrub"),
            ]
        );
        assert_eq!(
            control
                .wire(crate::UiPanelWireRole::Scrub)
                .and_then(|wire| wire.address.clone()),
            Some(clock_slot_address("transport.scrub_offset_seconds")),
            "the slot-edit fallback address rides each wire too"
        );
    }

    /// Membership is a WIRING fact: a transport nothing has wired has no
    /// panel presence at all. Its card face is untouched — the tape hero
    /// still renders, it just dispatches slot edits.
    #[test]
    fn an_unwired_transport_reaches_no_panel() {
        let sections = clock_sections(vec![
            transport_row("play_state", UiSlotValue::string("playing")),
            transport_row("rate", UiSlotValue::f32(1.0)),
            transport_row("scrub_offset_seconds", UiSlotValue::f32(0.0)),
        ]);
        let Some(UiNodeFace::Clock(face)) =
            kind_face("clock", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a clock face");
        };

        assert!(face.controls.is_empty(), "no wired leaf, no panel control");
        assert!(
            face.transport.is_some(),
            "the card's own tape hero is a shape fact, not a wiring one"
        );
        assert!(face.transport_wires().is_empty());
    }

    /// PARTIAL wiring: only play_state is public. The faceplate still
    /// renders whole, the group anchors on the next wired sibling in
    /// declaration order (Q22 — rate is unwired, so play_state stands in),
    /// and the unwired dimensions keep their slot-edit addresses.
    #[test]
    fn a_partly_wired_transport_renders_whole_and_dispatches_mixed() {
        let controls = clock_controls(vec![
            wired_transport_row(
                "play_state",
                UiSlotValue::string("paused"),
                lpc_model::bus::well_known::CLOCK_PLAY_STATE_CHANNEL,
            ),
            transport_row("rate", UiSlotValue::f32(1.0)),
            transport_row("scrub_offset_seconds", UiSlotValue::f32(0.0)),
        ]);

        assert_eq!(controls.len(), 1, "one wired leaf is enough to be on");
        let control = &controls[0];
        assert_eq!(
            control
                .panel_target
                .as_ref()
                .map(|target| target.channel.as_str()),
            Some("clock.play_state"),
            "rate is unwired, so the anchor migrates to the next wired sibling"
        );
        let rate = control
            .wire(crate::UiPanelWireRole::Rate)
            .expect("the fader dimension is still on the faceplate");
        assert!(rate.panel_target.is_none(), "…but it is not wired");
        assert_eq!(
            rate.address,
            Some(clock_slot_address("transport.rate")),
            "so its gesture falls back to a slot edit at its own address"
        );
    }

    /// A wired dimension's LIVE channel reading leads on the faceplate (the
    /// panel-write echo path): once gestures are panel writes, the authored
    /// slot default is no longer what the transport is doing. An unwired
    /// sibling keeps its staged slot value.
    #[test]
    fn a_wired_dimension_shows_its_live_reading() {
        let live_rate = {
            let endpoint = crate::UiBindingEndpoint::new("bus:clock.rate")
                .with_default_origin()
                .with_panel_hint()
                .with_live_value("4")
                .with_panel_target(crate::UiPanelTarget {
                    scope: test_scope(),
                    channel: "clock.rate".to_string(),
                    engaged: true,
                });
            transport_row("rate", UiSlotValue::f32(1.0))
                .with_source(UiSlotSourceState::Bound(endpoint))
        };
        let sections = clock_sections(vec![
            wired_transport_row(
                "play_state",
                UiSlotValue::string("playing"),
                lpc_model::bus::well_known::CLOCK_PLAY_STATE_CHANNEL,
            )
            .with_source(UiSlotSourceState::Bound(
                crate::UiBindingEndpoint::new("bus:clock.play_state")
                    .with_default_origin()
                    .with_panel_hint()
                    .with_live_value("paused")
                    .with_panel_target(crate::UiPanelTarget {
                        scope: test_scope(),
                        channel: "clock.play_state".to_string(),
                        engaged: true,
                    }),
            )),
            live_rate,
            transport_row("scrub_offset_seconds", UiSlotValue::f32(-2.0)),
        ]);
        let Some(UiNodeFace::Clock(face)) =
            kind_face("clock", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a clock face");
        };

        let transport = face.transport.expect("block present");
        assert_eq!(transport.rate, 4.0, "the channel reading leads");
        assert_eq!(transport.play_state, lpc_model::PlayState::Paused);
        assert_eq!(
            transport.scrub_offset_seconds, -2.0,
            "the unwired dimension keeps its staged slot value"
        );
    }

    /// A clock with no produced product row keeps the generic sections —
    /// the same stable-face rule every other kind follows.
    #[test]
    fn a_clock_with_no_product_row_stays_generic() {
        assert_eq!(
            kind_face(
                "clock",
                &test_address(),
                &[UiNodeSection::ConfigSlots(Vec::new())],
                &mut Vec::new()
            ),
            None
        );
    }

    /// The binding-derived row the project walk appends for a wired uniform
    /// (key = the bare uniform name, source = Bound).
    fn bound_row(name: &str, endpoint: UiBindingEndpoint) -> UiConfigSlot {
        UiConfigSlot::empty(name, "Speed").with_source(UiSlotSourceState::Bound(endpoint))
    }

    /// A consumed-bus endpoint carrying the `(scope, channel)` panel target
    /// the project walk derives — the Q13 membership fact.
    fn channel_endpoint(display: &str, channel: &str, owner: u32) -> UiBindingEndpoint {
        UiBindingEndpoint::new(display).with_panel_target(crate::UiPanelTarget {
            scope: lpc_wire::WireScopeRef::Module {
                owner: lpc_model::NodeId::new(owner),
            },
            channel: channel.to_string(),
            engaged: false,
        })
    }

    /// Playlist sections: produced visual output, the `active_entry` status
    /// row (when `active` is given), and the `entries` map (1 = "idle",
    /// name-only; 2 = "active", 4 s duration + one trigger id).
    fn playlist_sections(active: Option<u32>) -> Vec<UiNodeSection> {
        let entry_1 = UiConfigSlot::record(
            "entries[1]",
            "1",
            vec![UiConfigSlot::value(
                "entries[1].name",
                "Name",
                UiSlotValue::string("idle"),
            )],
        );
        let entry_2 = UiConfigSlot::record(
            "entries[2]",
            "2",
            vec![
                UiConfigSlot::value("entries[2].name", "Name", UiSlotValue::string("active")),
                option_f32("entries[2].duration", "Duration", 4.0),
                UiConfigSlot::value(
                    "entries[2].trigger_ids",
                    "Trigger ids",
                    UiSlotValue::array(vec![UiSlotValue::u32(1)]),
                )
                .with_optionality(UiSlotOptionality::included(true)),
            ],
        );

        let mut sections = vec![UiNodeSection::ProducedProducts(vec![
            UiProducedProduct::visual("Output"),
        ])];
        if let Some(active) = active {
            sections.push(UiNodeSection::ProducedValues(vec![
                UiProducedValue::new("Active entry", active.to_string()).with_key("active_entry"),
            ]));
        }
        sections.push(UiNodeSection::ConfigSlots(vec![UiConfigSlot::record(
            "entries",
            "Entries",
            vec![entry_1, entry_2],
        )]));
        sections
    }

    /// Children as the loader mounts them: `idle` (no cached preview) and
    /// `active` (with a cached visual snapshot), both with select actions.
    fn playlist_children() -> Vec<UiNodeChild> {
        let mut active = child("active", "Active");
        active.sections = vec![UiNodeSection::ProducedProducts(vec![
            UiProducedProduct::visual("Output").with_preview(UiProductPreview::VisualSrgb8 {
                width: 2,
                height: 2,
                revision: 1,
                bytes: vec![0u8; 12].into(),
            }),
        ])];
        vec![child("idle", "Idle"), active]
    }

    fn child(name: &str, label: &str) -> UiNodeChild {
        let mut child = UiNodeChild::new(
            label,
            "Shader",
            format!("/main.module/playlist.playlist/{name}.shader"),
        );
        child.action = Some(
            UiAction::from_op(ControllerId::new("test.module"), ProjectEditorOp::Focus)
                .with_label(format!("Focus {label}")),
        );
        child
    }

    #[test]
    fn a_bus_wired_brightness_fader_drives_the_channel() {
        // The scarf's own control (panel.md P10): once brightness is wired
        // to a channel, dimming it must engage a panel writer — that is
        // what persists across a replug. The fixture path derives its
        // target from the same binding-derived row the shader knobs use.
        let mut sections = fixture_sections();
        if let UiNodeSection::ConfigSlots(rows) = &mut sections[1] {
            rows.push(UiConfigSlot::empty("brightness", "Brightness").with_source(
                UiSlotSourceState::Bound(
                    UiBindingEndpoint::new("bus:brightness").with_panel_target(
                        crate::UiPanelTarget {
                            scope: lpc_wire::WireScopeRef::Module {
                                owner: lpc_model::NodeId::new(1),
                            },
                            channel: "brightness".to_string(),
                            engaged: true,
                        },
                    ),
                ),
            ));
        }

        let Some(UiNodeFace::Fixture(face)) =
            kind_face("fixture", &test_address(), &sections, &mut Vec::new())
        else {
            panic!("expected a fixture face");
        };
        let target = face
            .brightness
            .panel_target
            .as_ref()
            .expect("a wired brightness fader targets its channel");
        assert_eq!(target.channel, "brightness");
        assert!(target.engaged, "and reports the engaged writer");
    }

    #[test]
    fn an_unwired_brightness_fader_still_edits_its_slot() {
        let Some(UiNodeFace::Fixture(face)) = kind_face(
            "fixture",
            &test_address(),
            &fixture_sections(),
            &mut Vec::new(),
        ) else {
            panic!("expected a fixture face");
        };
        assert!(face.brightness.panel_target.is_none());
        assert!(face.brightness.address.is_some());
    }

    fn fixture_sections() -> Vec<UiNodeSection> {
        let brightness_value = UiSlotValue::u32(64).with_editor(UiSlotEditorHint::Slider {
            min: 0.0,
            max: 255.0,
            step: Some(1.0),
        });
        vec![
            UiNodeSection::ProducedProducts(vec![UiProducedProduct::control("Output")]),
            UiNodeSection::ConfigSlots(vec![
                UiConfigSlot::value("brightness", "Brightness", brightness_value)
                    .with_address(address("brightness"))
                    .with_optionality(UiSlotOptionality::included(true)),
            ]),
        ]
    }
}
