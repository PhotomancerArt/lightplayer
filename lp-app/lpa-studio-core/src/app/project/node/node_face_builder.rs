//! Kind-specific node-card face derivation (node-card P3).
//!
//! Faces are built FROM the already-projected section DTOs — the same rows,
//! previews, edit states, and inline editors the generic sections view
//! renders — so a panel control and its backing slot row can never disagree
//! (one derivation, two presentations). The builder never re-reads slot
//! controllers or project state.
//!
//! - **Shader**: preview = the produced visual product; controls = consumed
//!   uniform slots whose authored `panel` flag is on (`ShaderSlotDef.panel`),
//!   as knobs over the authored `min`/`max` editing
//!   `consumed.<name>.default.some`, snapping to the uniform's step
//!   ([`lpc_model::shader_panel_step`] — authored, else 1 for an i32/u32
//!   shape); the code drawer reuses the inline GLSL
//!   asset editor. The agent handle is decorated later by the studio
//!   controller (`AgentController::decorate_editor_view`), exactly like the
//!   sections path.
//! - **Fixture**: preview = the produced control product (lamp map);
//!   brightness = the first panel-flagged value row (`SlotMeta::panel`,
//!   seeded by [`lpc_model::Brightness`]) as the dominant fader editing
//!   `brightness.some`.
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
    FixtureDef, HwEndpointSpec, LpValue, OutputDef, PlaylistDef, ShaderDef, ShaderValueShapeRef,
    shader_panel_step,
};

use crate::app::project::format_lp_value;
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
) -> Option<UiNodeFace> {
    match ty {
        ShaderDef::KIND => shader_face(sections).map(UiNodeFace::Shader),
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
        // Unknown kinds stay on the generic fallback permanently.
        _ => None,
    }
}

/// The shader card's face: visual hero, panel knobs, code drawer. `None`
/// when the node produces no visual output row (nothing to be a face of).
fn shader_face(sections: &[UiNodeSection]) -> Option<UiShaderFace> {
    let preview = product_of_kind(sections, UiProductKind::Visual)?;
    Some(UiShaderFace {
        preview,
        controls: shader_panel_controls(sections),
        // Decorated by the studio controller's view build (the project
        // walk stays agent-free, same rule as the sections path).
        agent: None,
        code_drawer: glsl_inline_editor(sections),
    })
}

/// The fixture card's face: lamp preview + dominant brightness fader. Both
/// pieces are required — a fixture without a panel-flagged fader row keeps
/// the generic sections.
fn fixture_face(sections: &[UiNodeSection]) -> Option<UiFixtureFace> {
    let preview = product_of_kind(sections, UiProductKind::Control)?;
    let brightness = config_rows(sections)
        .into_iter()
        .find_map(|slot| panel_control_from_row(slot, panel_widget(slot)?))?;
    Some(UiFixtureFace {
        preview,
        brightness,
        mapping_editor: inline_editor_of_kind(sections, UiAssetEditorKind::Map2d),
        power: fixture_power(sections),
    })
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
fn product_of_kind(sections: &[UiNodeSection], kind: UiProductKind) -> Option<UiProducedProduct> {
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

/// Knob controls from the shader's `consumed` map: one per uniform whose
/// authored `panel` flag is on and whose `default` value is present (the
/// knob edits `consumed.<name>.default.some` through the standard slot
/// path). A uniform wired by a binding additionally wears that binding's
/// aspect, so the knob rolls up violet exactly like the binding row.
fn shader_panel_controls(sections: &[UiNodeSection]) -> Vec<UiPanelControl> {
    let rows = config_rows(sections);
    let Some(UiConfigSlotBody::Record(consumed)) = rows
        .iter()
        .find(|row| row.key == "consumed")
        .map(|row| &row.body)
    else {
        return Vec::new();
    };
    consumed
        .fields
        .iter()
        .filter_map(|entry| shader_uniform_control(entry, &rows))
        .collect()
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
    if !option_bool_field(fields, "panel") {
        return None;
    }

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

    let name = map_entry_name(entry);
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

/// A present `OptionSlot<ValueSlot<bool>>` field carrying `true`.
fn option_bool_field(fields: &[UiConfigSlot], name: &str) -> bool {
    uniform_field(fields, name).is_some_and(|field| {
        field.optionality.is_some_and(|opt| opt.included)
            && matches!(
                &field.body,
                UiConfigSlotBody::Value(UiSlotValue {
                    kind: UiSlotValueKind::Bool(true),
                    ..
                })
            )
    })
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
        live_value: bound_live_value(slot),
        unit: value.unit.clone(),
        state: slot.state.clone(),
        aspects: slot.visible_aspects(),
    })
}

/// The widget a panel-flagged value row renders as, from its editor hint
/// (Knob → knob, Slider → fader, bool → toggle). `None` (no mappable
/// widget) keeps the row off the panel.
fn panel_widget(slot: &UiConfigSlot) -> Option<UiPanelWidget> {
    let UiConfigSlotBody::Value(value) = &slot.body else {
        return None;
    };
    if !value.panel {
        return None;
    }
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
        ProjectNodeAddress::parse("/demo.project/node.playlist").expect("valid address")
    }

    #[test]
    fn shader_face_builds_knobs_from_panel_flagged_uniforms() {
        let sections = shader_sections();

        let face =
            kind_face("shader", &test_address(), &sections, &mut Vec::new()).expect("shader face");
        let UiNodeFace::Shader(face) = face else {
            panic!("expected a shader face");
        };

        assert_eq!(face.preview.kind, UiProductKind::Visual);
        assert_eq!(face.controls.len(), 1, "only the panel-flagged uniform");
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
            UiNodeSection::ConfigSlots(vec![UiConfigSlot::record(
                "consumed",
                "Consumed",
                vec![panel_uniform("count", "u32", 2.37, 1.0, 4.0, None)],
            )]),
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
        let sections = |uniform| {
            vec![
                UiNodeSection::ProducedProducts(vec![UiProducedProduct::visual("Output")]),
                UiNodeSection::ConfigSlots(vec![UiConfigSlot::record(
                    "consumed",
                    "Consumed",
                    vec![uniform],
                )]),
            ]
        };
        let step_of = |uniform| {
            let Some(UiNodeFace::Shader(face)) = kind_face(
                "shader",
                &test_address(),
                &sections(uniform),
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
            step_of(panel_uniform("count", "u32", 2.0, 1.0, 4.0, Some(2.0))),
            Some(2.0),
            "an authored step overrides the shape's implied 1"
        );
        assert_eq!(
            step_of(panel_uniform("speed", "f32", 2.37, 0.0, 4.0, None)),
            None,
            "a plain f32 uniform keeps sliding continuously"
        );
    }

    /// A panel-flagged uniform record row: value shape, default, range, and
    /// an optional authored step.
    fn panel_uniform(
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
            UiConfigSlot::value(format!("{prefix}.panel"), "Panel", UiSlotValue::bool(true))
                .with_optionality(UiSlotOptionality::included(true)),
        ];
        if let Some(step) = step {
            fields.push(option_f32(&format!("{prefix}.step"), "Step", step));
        }
        UiConfigSlot::record(prefix.clone(), name, fields).with_address(address(&prefix))
    }

    #[test]
    fn bound_uniform_wears_the_binding_rows_violet_aspect() {
        let mut sections = shader_sections();
        // The binding-derived row the project walk appends for a wired
        // uniform (key = bare uniform name, source = Bound).
        if let UiNodeSection::ConfigSlots(rows) = &mut sections[1] {
            rows.push(
                UiConfigSlot::empty("speed", "Speed")
                    .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new("bus:time"))),
            );
        }

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
    fn bound_uniform_mirrors_the_wired_rows_live_reading() {
        let mut sections = shader_sections();
        // The binding-derived row, decorated with the channel's quantized
        // live reading by the project walk (P6 item 1).
        if let UiNodeSection::ConfigSlots(rows) = &mut sections[1] {
            rows.push(
                UiConfigSlot::empty("speed", "Speed").with_source(UiSlotSourceState::Bound(
                    UiBindingEndpoint::new("bus:master-tempo").with_live_value("2.72"),
                )),
            );
        }

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
        // A fixture whose rows carry no panel flag keeps the sections view.
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
            ProjectNodeAddress::parse("/demo.project/node.shader").expect("valid address"),
            ProjectSlotRoot::Def,
            SlotPath::parse(path).expect("valid path"),
        )
    }

    fn option_f32(key: &str, label: &str, value: f32) -> UiConfigSlot {
        UiConfigSlot::value(key, label, UiSlotValue::f32(value))
            .with_address(address(key))
            .with_optionality(UiSlotOptionality::included(true))
    }

    fn shader_sections() -> Vec<UiNodeSection> {
        let uniform = |name: &str, panel: bool| {
            let prefix = format!("consumed[{name}]");
            let mut fields = vec![
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
            if panel {
                fields.push(
                    UiConfigSlot::value(
                        format!("{prefix}.panel"),
                        "Panel",
                        UiSlotValue::bool(true),
                    )
                    .with_optionality(UiSlotOptionality::included(true)),
                );
            }
            UiConfigSlot::record(prefix.clone(), name, fields).with_address(address(&prefix))
        };

        vec![
            UiNodeSection::ProducedProducts(vec![UiProducedProduct::visual("Output")]),
            UiNodeSection::ConfigSlots(vec![UiConfigSlot::record(
                "consumed",
                "Consumed",
                vec![uniform("speed", true), uniform("time", false)],
            )]),
        ]
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
            format!("/main.project/playlist.playlist/{name}.shader"),
        );
        child.action = Some(
            UiAction::from_op(ControllerId::new("test.project"), ProjectEditorOp::Focus)
                .with_label(format!("Focus {label}")),
        );
        child
    }

    fn fixture_sections() -> Vec<UiNodeSection> {
        let brightness_value =
            UiSlotValue::u32(64)
                .with_panel(true)
                .with_editor(UiSlotEditorHint::Slider {
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
