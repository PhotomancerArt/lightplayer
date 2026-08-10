//! The output card's permanent face: the board, then one row per wire.
//!
//! An output node drives N physical wires (`OutputDef.ports`), and its one
//! control buffer is sliced across them in key order. The face makes that
//! split visible and editable:
//!
//! - the **board** section draws [`BoardDiagram`] in [`DiagramMode::Wired`],
//!   one violet connection per assigned port — the first REAL data into
//!   `wired` (until now the mode was sample-fed from stories only). "No board
//!   known" is first class: the section simply is not there, and a quiet line
//!   in the ports section says where the pins are edited instead;
//! - the **ports** section is one row per wire — key, pin, lamp count —
//!   over a caption carrying the wire's slice and resolved GPIO;
//! - the **patch** section is the wire's own picture: one row per port,
//!   with a bordered, live-pixel cell for each producer run landing on it
//!   (D34a). It is the OUTPUT half of a two-sided bay whose other half
//!   rides each fixture card, showing the same cells in the fixture's own
//!   order. Read-only: the patch document is edited as text.
//!
//! Every edit here is an ORDINARY slot op against the addresses the DTO
//! carries ([`crate::app::node::slot_edit_actions`]): the pin picker writes
//! the endpoint string, the count field writes `ports[k].count.some`,
//! add/remove are the generic map-entry gestures, and the spread gesture is
//! nothing but a sequence of those count writes (so it undoes edit by edit,
//! and needs no op of its own).
//!
//! The diagram is a READ-OUT, not a hit target: `lpa-boards` exposes no
//! hit-test API, and that crate is shared with the board editor. Picking a
//! pin happens in the row's own picker, which lists the eligible pins the
//! decoration pass resolved (rails and screw terminals alike), marking the
//! ones another port already claims.

use dioxus::prelude::*;
use lpa_boards::{BoardDiagram, DiagramMode, WiredConnection, board_by_id};
use lpa_studio_core::{
    LpValue, ProjectSlotAddress, SlotMapKey, UiAction, UiOutputFace as UiOutputFaceData,
    UiOutputPin, UiOutputPortRow, UiSlotFieldState,
};

use crate::app::node::face::PatchBaySection;
use crate::app::node::slot_edit_actions::{
    slot_ensure_present_action, slot_remove_value_action, slot_set_value_action,
};
use crate::app::node::{MapEntryRemoveButton, NodeCardSection, UIntSlotField};
use crate::base::{
    DetailPopover, DetailSection, InlineButton, NodeKindIcon, PopoverCloseHandle, PopoverPlacement,
    StudioIconName,
};
use crate::core::menu_item_action_class;

/// The endpoint an unparseable (or empty) spec is rebuilt from when a pin is
/// picked: the ws281x capability on the device the project is running on.
/// Segment two is the TARGET DEVICE, never a driver mechanism — which
/// peripheral drives the wire is firmware's business.
const DEFAULT_ENDPOINT_PREFIX: &str = "ws281x:local";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OutputFace(
    face: UiOutputFaceData,
    /// Open this port's pin picker on first render (stories).
    #[props(default = None)]
    pin_picker_open: Option<u32>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    // The board the decoration pass named, when Studio ships a diagram for
    // it. `board_facts` already refused unknown ids, so a miss here would be
    // a catalog that moved under us — degrade to the no-board face either way.
    let board = face
        .board
        .as_ref()
        .and_then(|facts| board_by_id(&facts.board_id).map(|board| (facts.clone(), board.clone())));
    let wired = wired_connections(&face);
    // With no diagram above it, the ports section IS the top of the face
    // and takes the first-section treatment (no divider under the header).
    let ports_first = board.is_none();

    rsx! {
        if let Some((facts, display)) = board {
            NodeCardSection { label: "board", first: true,
                div { class: "ux-output-board",
                    BoardDiagram {
                        board: display,
                        mode: DiagramMode::Wired,
                        u: 12.0,
                        wired,
                    }
                }
                p { class: "tw:m-0 tw:px-4 tw:pb-3 tw:text-center tw:font-mono tw:text-[0.68rem] tw:text-dim-foreground",
                    "{facts.display_name}"
                }
            }
        }
        OutputPorts {
            face: face.clone(),
            first: ports_first,
            pin_picker_open,
            on_action,
        }
        // The patch, under the wires that carry it: what each port is
        // actually driving, cell by cell (D34a). Read-only — the document
        // is edited as text.
        if let Some(bay) = face.patch.clone() {
            PatchBaySection { bay }
        }
    }
}

/// The wires themselves: a summary line carrying the spread gesture, the
/// rows, and the add affordance at the bottom — where the new wire appears
/// (house rule: never in a header).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OutputPorts(
    face: UiOutputFaceData,
    first: bool,
    #[props(default = None)] pin_picker_open: Option<u32>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let summary = ports_summary(&face);
    let spread = spread_plan(&face);
    let spread_label = spread_label(&face);
    let spread_title = spread_title(&face);
    let add_key = next_port_key(&face);
    let add_address = face
        .ports_address
        .as_ref()
        .map(|map| map.child_map_entry(SlotMapKey::U32(add_key)));
    let last_key = face.ports.last().map(|port| port.key);

    rsx! {
        NodeCardSection { label: "ports", first,
            div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                    span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.68rem] tw:text-subtle-foreground",
                        "{summary}"
                    }
                    if let Some(handler) = on_action.filter(|_| !spread.is_empty()) {
                        InlineButton {
                            label: spread_label,
                            text: spread_label,
                            title: spread_title.clone(),
                            class: "tw:ml-auto",
                            on_press: {
                                let face = face.clone();
                                move |_| {
                                    for action in spread_actions(&face) {
                                        handler.call(action);
                                    }
                                }
                            },
                        }
                    }
                }
                if face.board.is_none() {
                    p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                        "No board known for this device — wires are still fully editable; pin names are free text in the advanced drawer."
                    }
                }
                if face.ports.is_empty() {
                    p { class: "tw:m-0 tw:text-sm tw:text-subtle-foreground",
                        "No wires yet."
                    }
                }
                for port in face.ports.clone() {
                    OutputPortRow {
                        key: "{port.key}",
                        remainder_allowed: last_key == Some(port.key),
                        picker_open: pin_picker_open == Some(port.key),
                        face: face.clone(),
                        port,
                        on_action,
                    }
                }
                if let Some((address, handler)) = add_address.zip(on_action) {
                    div { class: "tw:flex tw:min-w-0",
                        InlineButton {
                            label: "Add port {add_key}",
                            title: "Add port {add_key} — a new wire at the end of the slice order",
                            icon: StudioIconName::Add,
                            text: "port",
                            on_press: move |_| {
                                handler.call(slot_ensure_present_action(address.clone()));
                            },
                        }
                    }
                }
                // The board's measured LED envelope — a SOFT limit: evidence
                // of what has run clean, so exceeding it reads as attention
                // (advice), never as an error.
                if let Some(budget) = face.led_budget.as_ref() {
                    p {
                        class: if budget.used > budget.budget {
                            "tw:m-0 tw:font-mono tw:text-[0.65rem] tw:text-status-attention-foreground"
                        } else {
                            "tw:m-0 tw:font-mono tw:text-[0.65rem] tw:text-dim-foreground"
                        },
                        title: "The board's measured envelope: the total LEDs this board and firmware have run clean (heap-bound). Exceeding it proceeds with a warning — expect heap pressure.",
                        "{budget.used} / {budget.budget} LEDs · measured envelope"
                    }
                }
            }
        }
    }
}

/// One wire: `ch0 · IO18 · 300`, over a caption reading its slice and the
/// GPIO the pin resolved to.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OutputPortRow(
    face: UiOutputFaceData,
    port: UiOutputPortRow,
    /// This is the highest-keyed port, so it — and only it — may drop its
    /// count and take the remainder.
    remainder_allowed: bool,
    #[props(default = false)] picker_open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let key = port.key;
    // Whether a pin RESOLVED only means something when there is a board to
    // resolve it against; without one, an unresolved pin is the normal state
    // of the whole face, not this wire's fault.
    let board_known = face.board.is_some();
    let caption = port_caption(&port, board_known);
    let entry_address = face
        .ports_address
        .as_ref()
        .map(|map| map.child_map_entry(SlotMapKey::U32(key)));

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0.5 tw:rounded-xs tw:border tw:border-border-muted tw:bg-card-subtle tw:px-2 tw:py-1.5",
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                span { class: port_key_class(&port, board_known), "ch{key}" }
                PortPinCell {
                    face: face.clone(),
                    port: port.clone(),
                    picker_open,
                    on_action,
                }
                PortCountCell {
                    face: face.clone(),
                    port: port.clone(),
                    remainder_allowed,
                    on_action,
                }
                if let Some((address, handler)) = entry_address.zip(on_action) {
                    MapEntryRemoveButton { address, on_action: handler }
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
                span { class: "tw:min-w-0 tw:flex-1 tw:truncate tw:font-mono tw:text-[0.65rem] tw:text-dim-foreground",
                    "{caption}"
                }
                // Live wire health from the device heartbeat, when a device
                // is reporting. "wave 2" is a discrete state (squared block,
                // never a warning): this wire time-shares a transmitter slot
                // and waits its turn each frame — by design at more wires
                // than slots. Torn frames are the actual distress signal.
                if let Some(status) = port.wire_status.as_ref() {
                    if status.waves {
                        span {
                            class: "tw:shrink-0 tw:rounded-xs tw:border tw:border-border-muted tw:px-1 tw:font-mono tw:text-[0.6rem] tw:text-subtle-foreground",
                            title: "This wire transmits in the second wave: it waits for a pooled transmitter slot each frame (worst wait {status.queue_wait_ms} ms). Structural at more wires than slots — not a fault.",
                            "wave 2"
                        }
                    }
                    if status.torn > 0 {
                        span {
                            class: "tw:shrink-0 tw:font-mono tw:text-[0.6rem] tw:text-status-attention-foreground",
                            title: "Frames truncated on the strand since boot (guard-word protection). A growing count is distress; a small constant one usually marks project uploads.",
                            "torn {status.torn}"
                        }
                    } else {
                        span {
                            class: "tw:shrink-0 tw:font-mono tw:text-[0.6rem] tw:text-dim-foreground",
                            title: "Frames transmitted since boot, no tears.",
                            "{status.sent} sent"
                        }
                    }
                }
            }
        }
    }
}

/// The pin cell: a picker trigger when a board is known, a plain readout
/// when it is not (the endpoint stays editable as text in the advanced
/// drawer, which is the honest fallback rather than a picker with no list).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PortPinCell(
    face: UiOutputFaceData,
    port: UiOutputPortRow,
    #[props(default = false)] picker_open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let label = pin_display(&port);
    let board_known = face.board.is_some();
    let cell_class = pin_cell_class(&port, board_known);
    let pins = face
        .board
        .as_ref()
        .map(|board| board.pins.clone())
        .unwrap_or_default();
    let wiring = port.endpoint_address.clone().zip(on_action);

    let Some((address, handler)) = wiring.filter(|_| !pins.is_empty()) else {
        return rsx! {
            span {
                class: "{cell_class} tw:min-w-0 tw:truncate",
                title: "{port.endpoint_display}",
                "{label}"
            }
        };
    };

    rsx! {
        DetailPopover {
            icon: StudioIconName::NodeKind(NodeKindIcon::Output),
            label: format!("Pin for port {}", port.key),
            title: port.endpoint_display.clone(),
            placement: PopoverPlacement::BottomStart,
            initially_open: picker_open,
            layer_keeps_layout: true,
            trigger: Some(rsx! {
                span { class: "tw:min-w-0 tw:truncate", "{label}" }
            }),
            trigger_class: cell_class.to_string(),
            trigger_open_class: format!("{cell_class} tw:border-border-strong"),
            DetailSection { title: "Drive this wire from", meta: Some("output-eligible pins".to_string()),
                div { class: "tw:grid tw:max-h-64 tw:gap-0.5 tw:overflow-y-auto",
                    for pin in pins {
                        PinPickerRow {
                            key: "{pin.label}",
                            current: pin.label == port.pin_label,
                            port_key: port.key,
                            endpoint: port.endpoint_display.clone(),
                            address: address.clone(),
                            pin,
                            on_action: handler,
                        }
                    }
                }
            }
        }
    }
}

/// One eligible pin in the picker. A pin another port already claims is
/// SHOWN and still pickable — two wires on one pin is an authoring mistake
/// the face marks rather than hides.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PinPickerRow(
    pin: UiOutputPin,
    /// The port doing the picking (so its own claim reads "current"
    /// rather than "taken").
    port_key: u32,
    current: bool,
    endpoint: String,
    address: ProjectSlotAddress,
    on_action: EventHandler<UiAction>,
) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    let claimed = pin.assigned_to.filter(|assigned| *assigned != port_key);
    let next = endpoint_with_pin(&endpoint, &pin.label);
    let title = next.clone();

    rsx! {
        button {
            class: menu_item_action_class(),
            r#type: "button",
            title: "{title}",
            onclick: move |event| {
                event.stop_propagation();
                on_action.call(slot_set_value_action(address.clone(), LpValue::String(next.clone())));
                if let Some(mut close) = close {
                    close.close();
                }
            },
            span { class: "tw:font-mono", "{pin.label}" }
            span { class: "tw:text-dim-foreground", "gpio {pin.gpio}" }
            if current {
                span { class: "tw:ml-auto tw:text-status-bound-foreground", "current" }
            } else if let Some(claimed) = claimed {
                span { class: "tw:ml-auto tw:text-status-attention-foreground", "ch{claimed}" }
            }
        }
    }
}

/// The count cell: the authored lamp count as an ordinary uint field, or the
/// remainder port's resolved count as a readout — with the one gesture
/// that flips between them (set the option / clear it).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PortCountCell(
    face: UiOutputFaceData,
    port: UiOutputPortRow,
    remainder_allowed: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let resolved = port.resolved_count;

    if port.count.is_none() {
        // The remainder wire. Its count is not authored, so there is nothing
        // to edit here — only the gesture that freezes what it resolves to.
        let pin_count = resolved
            .zip(face.ports_address.clone())
            .zip(on_action)
            .map(|((count, _), handler)| (count, handler));
        return rsx! {
            span {
                class: "tw:ml-auto tw:inline-flex tw:flex-none tw:items-baseline tw:gap-1 tw:rounded-xs tw:border tw:border-dashed tw:border-border-muted tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-xs tw:text-muted-foreground",
                title: "This wire takes whatever is left of the buffer",
                span { class: "tw:text-dim-foreground", "rest" }
                span { {resolved.map_or_else(|| "—".to_string(), |count| count.to_string())} }
            }
            if let Some((count, handler)) = pin_count {
                InlineButton {
                    label: "Author this wire's count",
                    title: "Author this wire's count as {count} instead of taking the remainder",
                    icon: StudioIconName::Add,
                    on_press: {
                        let face = face.clone();
                        let port = port.clone();
                        move |_| {
                            for action in count_edit_actions(&face, &port, count) {
                                handler.call(action);
                            }
                        }
                    },
                }
            }
        };
    }

    let count = port.count.unwrap_or_default();
    let clear = remainder_allowed
        .then(|| face.ports_address.as_ref())
        .flatten()
        .and_then(|map| {
            map.child_map_entry(SlotMapKey::U32(port.key))
                .child_field("count")
        })
        .zip(on_action);

    rsx! {
        span { class: "tw:ml-auto tw:flex-none",
            UIntSlotField {
                value: count,
                state: UiSlotFieldState::editable(),
                min: Some(0.0),
                address: port.count_address.clone(),
                on_action,
            }
        }
        if let Some((address, handler)) = clear {
            InlineButton {
                label: "Take the remainder",
                title: "Drop the count — this wire takes whatever is left of the buffer",
                icon: StudioIconName::Remove,
                on_press: move |_| {
                    handler.call(slot_remove_value_action(address.clone()));
                },
            }
        }
    }
}

// -- derivations ---------------------------------------------------------------

/// The diagram's connections: one per port whose pin resolved to a GPIO
/// on the known board. A port naming a pin this board lacks contributes
/// nothing to draw — the row still shows it, which is where that mistake is
/// legible.
fn wired_connections(face: &UiOutputFaceData) -> Vec<WiredConnection> {
    face.ports
        .iter()
        .filter_map(|port| {
            let gpio = u8::try_from(port.gpio?).ok()?;
            Some(WiredConnection {
                gpio,
                title: format!("ch{}", port.key),
                extra: port.resolved_count.map(|count| format!("×{count}")),
            })
        })
        .collect()
}

/// The ports section's summary: how many wires, over how many lamps.
fn ports_summary(face: &UiOutputFaceData) -> String {
    let wires = face.ports.len();
    let noun = if wires == 1 { "wire" } else { "wires" };
    match (face.total_lamps, face.authored_lamps()) {
        (Some(total), _) => format!("{wires} {noun} · {total} lamps"),
        (None, Some(authored)) => format!("{wires} {noun} · {authored} lamps authored"),
        (None, None) => format!("{wires} {noun} · lamps unknown"),
    }
}

/// The row caption: the wire's slice, and what its pin resolved to.
///
/// "unresolved" is a claim about the BOARD, so it may only be made when there
/// is one: with no board known, an un-translated pin label is the normal
/// state of every wire on the card and saying otherwise would be five false
/// alarms in a row.
fn port_caption(port: &UiOutputPortRow, board_known: bool) -> String {
    let slice = match (port.slice_start, port.resolved_count) {
        (Some(start), Some(count)) if count > 0 => {
            format!("lamps {start}–{}", start + count - 1)
        }
        (Some(start), Some(_)) => format!("lamps {start}– (empty)"),
        (Some(start), None) => format!("lamps {start}–?"),
        (None, _) => "slice unknown".to_string(),
    };
    match pin_state(port, board_known) {
        PinState::Wired(gpio) => format!("{slice} · gpio {gpio}"),
        PinState::Unnamed => format!("{slice} · no pin"),
        PinState::Named => format!("{slice} · {}", port.pin_label),
        PinState::NotOnBoard => format!("{slice} · {} unresolved", port.pin_label),
    }
}

/// How much the face actually knows about where a wire lands — the one
/// distinction both the caption and the two chip tones are keyed off.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PinState {
    /// Named, and the known board translates it to this GPIO.
    Wired(u32),
    /// Named, but no board is known to translate it against.
    Named,
    /// Named, and the known board has no such pin — the fault the face
    /// exists to surface.
    NotOnBoard,
    /// The endpoint names no pin at all.
    Unnamed,
}

fn pin_state(port: &UiOutputPortRow, board_known: bool) -> PinState {
    match (port.gpio, port.pin_label.is_empty(), board_known) {
        (Some(gpio), _, _) => PinState::Wired(gpio),
        (None, true, _) => PinState::Unnamed,
        (None, false, true) => PinState::NotOnBoard,
        (None, false, false) => PinState::Named,
    }
}

/// What the pin cell reads: the board's own label, or the fact that there
/// isn't one.
fn pin_display(port: &UiOutputPortRow) -> String {
    if port.pin_label.is_empty() {
        return "no pin".to_string();
    }
    port.pin_label.clone()
}

/// Violet marks a wire that lands on a real pin of the known board — the
/// house bound/wired colour, never green. A pin the KNOWN board lacks takes
/// attention, because that IS the fault the face exists to surface; a pin
/// nothing has been checked against stays neutral.
fn port_key_class(port: &UiOutputPortRow, board_known: bool) -> &'static str {
    const BASE: &str = "tw:inline-flex tw:flex-none tw:items-center tw:rounded-xs tw:border tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[0.68rem] tw:font-bold";
    match pin_state(port, board_known) {
        PinState::Wired(_) => {
            "tw:inline-flex tw:flex-none tw:items-center tw:rounded-xs tw:border tw:border-status-bound-border tw:bg-status-bound-bg tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[0.68rem] tw:font-bold tw:text-status-bound-foreground"
        }
        PinState::NotOnBoard => {
            "tw:inline-flex tw:flex-none tw:items-center tw:rounded-xs tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[0.68rem] tw:font-bold tw:text-status-attention-foreground"
        }
        PinState::Named | PinState::Unnamed => BASE,
    }
}

/// The pin cell wears the same tone as its port chip, so the row reads as
/// one wire rather than two independent chips.
fn pin_cell_class(port: &UiOutputPortRow, board_known: bool) -> &'static str {
    match pin_state(port, board_known) {
        PinState::Wired(_) => {
            "tw:inline-flex tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:rounded-xs tw:border tw:border-status-bound-border tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-xs tw:text-status-bound-foreground tw:transition-colors tw:hover:bg-status-bound-bg"
        }
        PinState::NotOnBoard => {
            "tw:inline-flex tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:rounded-xs tw:border tw:border-status-attention-border tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-xs tw:text-status-attention-foreground tw:transition-colors tw:hover:bg-status-attention-bg"
        }
        PinState::Named => {
            "tw:inline-flex tw:min-w-0 tw:items-center tw:rounded-xs tw:border tw:border-border-muted tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-xs tw:text-muted-foreground"
        }
        PinState::Unnamed => {
            "tw:inline-flex tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:rounded-xs tw:border tw:border-dashed tw:border-border-muted tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-xs tw:text-subtle-foreground tw:transition-colors tw:hover:border-border-strong"
        }
    }
}

/// The key the add affordance creates: one past the highest authored one, so
/// a new wire lands at the END of the slice order (where an added thing
/// belongs).
fn next_port_key(face: &UiOutputFaceData) -> u32 {
    face.ports
        .iter()
        .map(|port| port.key)
        .max()
        .map_or(0, |key| key.saturating_add(1))
}

/// Rebuild an endpoint spec around a new pin label, keeping the capability
/// and target segments the wire already declares. An empty or malformed spec
/// (a freshly added port's default) is rebuilt from
/// [`DEFAULT_ENDPOINT_PREFIX`].
fn endpoint_with_pin(endpoint: &str, pin_label: &str) -> String {
    let mut segments = endpoint.split(':');
    match (segments.next(), segments.next(), segments.next()) {
        (Some(capability), Some(target), Some(_))
            if !capability.is_empty() && !target.is_empty() =>
        {
            format!("{capability}:{target}:{pin_label}")
        }
        _ => format!("{DEFAULT_ENDPOINT_PREFIX}:{pin_label}"),
    }
}

// -- the spread gesture --------------------------------------------------------

/// Distribute `total` lamps over `wires` ports.
///
/// Two modes, one shape. The plain split gives every wire `total / wires` and
/// hands the remainder to the LAST one. When the upstream fixture published
/// per-path spans (P2's honest spans), those lamp indices are a real grid:
/// each cut moves to the boundary nearest its even position, so wires end on
/// path ends instead of mid-strip. A boundary already consumed by an earlier
/// cut is not offered again, and a cut with no boundary left of its own falls
/// back to the even position — snapping is a convenience, never a trap.
fn spread_counts(total: u32, wires: usize, boundaries: &[u32]) -> Vec<u32> {
    if wires == 0 {
        return Vec::new();
    }
    let even = total / wires as u32;
    let candidates: Vec<u32> = boundaries
        .iter()
        .copied()
        .filter(|boundary| *boundary > 0 && *boundary < total)
        .collect();

    let mut starts = Vec::with_capacity(wires + 1);
    starts.push(0u32);
    for index in 1..wires {
        let previous = starts[index - 1];
        let ideal = even.saturating_mul(index as u32).max(previous);
        let snapped = candidates
            .iter()
            .copied()
            .filter(|boundary| *boundary > previous)
            .min_by_key(|boundary| boundary.abs_diff(ideal));
        starts.push(snapped.unwrap_or(ideal).min(total));
    }
    starts.push(total);

    (0..wires)
        .map(|index| starts[index + 1].saturating_sub(starts[index]))
        .collect()
}

/// The ports the spread gesture would actually rewrite, as
/// `(port key, new count)`.
///
/// Two ports are deliberately left alone: one whose authored count is
/// already the computed one (a no-op edit is still an undo step), and the
/// LAST port when it takes the remainder — the remainder already resolves
/// to exactly the computed count, so freezing it would only cost the node its
/// one self-adjusting wire.
fn spread_plan(face: &UiOutputFaceData) -> Vec<(u32, u32)> {
    let Some(total) = face.total_lamps else {
        return Vec::new();
    };
    if face.ports.len() < 2 || face.ports_address.is_none() {
        return Vec::new();
    }
    let counts = spread_counts(total, face.ports.len(), &face.span_boundaries);
    let last = face.ports.len() - 1;
    face.ports
        .iter()
        .zip(counts)
        .enumerate()
        .filter(|(index, (port, count))| {
            !(*index == last && port.count.is_none()) && port.count != Some(*count)
        })
        .map(|(_, (port, count))| (port.key, count))
        .collect()
}

/// The spread gesture as ordinary slot edits, in port order.
fn spread_actions(face: &UiOutputFaceData) -> Vec<UiAction> {
    spread_plan(face)
        .into_iter()
        .filter_map(|(key, count)| {
            let port = face.ports.iter().find(|port| port.key == key)?;
            Some(count_edit_actions(face, port, count))
        })
        .flatten()
        .collect()
}

/// Write `count` to a port, whether or not the option is already set: a
/// present count edits its interior `some` through the address the DTO
/// carries; an absent one is included first (the generic option gesture) and
/// then written.
fn count_edit_actions(
    face: &UiOutputFaceData,
    port: &UiOutputPortRow,
    count: u32,
) -> Vec<UiAction> {
    if let Some(address) = &port.count_address {
        return vec![slot_set_value_action(address.clone(), LpValue::U32(count))];
    }
    let Some(some) = face
        .ports_address
        .as_ref()
        .and_then(|map| {
            map.child_map_entry(SlotMapKey::U32(port.key))
                .child_field("count")
        })
        .and_then(|count| count.child_field("some"))
    else {
        return Vec::new();
    };
    vec![
        slot_ensure_present_action(some.clone()),
        slot_set_value_action(some, LpValue::U32(count)),
    ]
}

/// Whether the snapping grid is real: a boundary that actually falls inside
/// the buffer is the only thing that changes what the gesture does.
fn spread_snaps(face: &UiOutputFaceData) -> bool {
    let Some(total) = face.total_lamps else {
        return false;
    };
    face.span_boundaries
        .iter()
        .any(|boundary| *boundary > 0 && *boundary < total)
}

fn spread_label(face: &UiOutputFaceData) -> &'static str {
    if spread_snaps(face) {
        "fit counts to strips"
    } else {
        "divide lamps evenly"
    }
}

/// Says exactly what clicking will do — the resulting per-wire counts — so
/// the gesture is judgeable before it is taken (G-A feedback: "spread" alone
/// did not explain itself).
fn spread_title(face: &UiOutputFaceData) -> String {
    let Some(total) = face.total_lamps else {
        return String::new();
    };
    let counts = spread_counts(total, face.ports.len(), &face.span_boundaries);
    let mut parts: Vec<String> = counts.iter().map(u32::to_string).collect();
    if let (Some(last), Some(port)) = (parts.last_mut(), face.ports.last()) {
        if port.count.is_none() {
            *last = format!("{last} (rest)");
        }
    }
    let plan = parts.join(" · ");
    if spread_snaps(face) {
        format!("Rewrite the wire counts to {plan} — cut where the mapping's strips end")
    } else {
        format!("Rewrite the wire counts to an even {plan}")
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot, SlotPath, UiOutputBoardFacts,
        UiOutputFace, UiOutputPin, UiOutputPortRow,
    };

    use super::{
        endpoint_with_pin, next_port_key, port_caption, port_key_class, ports_summary,
        spread_counts, spread_label, spread_plan, wired_connections,
    };

    fn ports_address() -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.project/strips.output").expect("valid node address"),
            ProjectSlotRoot::def(),
            SlotPath::parse("ports").expect("valid slot path"),
        )
    }

    /// A face as the builder plus decoration leave it: wires with authored
    /// counts, slices resolved from `total`.
    fn face(total: Option<u32>, wires: &[(u32, &str, Option<u32>)]) -> UiOutputFace {
        let mut start = Some(0u32);
        let ports = wires
            .iter()
            .map(|(key, pin, count)| {
                let slice_start = start;
                match count {
                    Some(count) => start = start.map(|start| start + count),
                    None => start = None,
                }
                UiOutputPortRow {
                    key: *key,
                    endpoint_display: format!("ws281x:local:{pin}"),
                    pin_label: (*pin).to_string(),
                    count: *count,
                    resolved_count: *count,
                    slice_start,
                    ..UiOutputPortRow::default()
                }
            })
            .collect();
        let mut face = UiOutputFace {
            ports,
            ports_address: Some(ports_address()),
            ..UiOutputFace::default()
        };
        if let Some(total) = total {
            face.resolve_extent(total);
        }
        face
    }

    #[test]
    fn an_even_split_puts_the_remainder_on_the_last_wire() {
        assert_eq!(spread_counts(1200, 5, &[]), vec![240, 240, 240, 240, 240]);
        assert_eq!(spread_counts(1000, 3, &[]), vec![333, 333, 334]);
        assert_eq!(spread_counts(7, 1, &[]), vec![7]);
        assert_eq!(spread_counts(0, 3, &[]), vec![0, 0, 0]);
        assert!(spread_counts(100, 0, &[]).is_empty());
    }

    #[test]
    fn spans_snap_the_cuts_to_path_boundaries() {
        // Four authored paths of 90/110/95/105 over 400 lamps, two wires:
        // the even cut at 200 moves to the nearest path start (200 → 200 is
        // exact here, so use a grid that does not line up).
        let boundaries = [0, 90, 200, 295];
        assert_eq!(spread_counts(400, 2, &boundaries), vec![200, 200]);
        // Three wires: ideal cuts at 133 and 266 snap to 90 and 295.
        assert_eq!(spread_counts(400, 3, &boundaries), vec![90, 205, 105]);
    }

    #[test]
    fn the_dome_story_s_before_state_spreads_to_exactly_its_after_state() {
        // The pair the visual gate compares: 1500 lamps, five wires, and an
        // upstream fixture whose five authored paths start at these lamps.
        assert_eq!(
            spread_counts(1500, 5, &[0, 280, 610, 900, 1210]),
            vec![280, 330, 290, 310, 290]
        );
    }

    #[test]
    fn a_boundary_is_never_reused_and_a_starved_cut_falls_back_to_even() {
        // One usable boundary, three wires: the first cut takes it, the
        // second has none left and lands on its even position.
        assert_eq!(spread_counts(300, 3, &[0, 90, 300]), vec![90, 110, 100]);
    }

    #[test]
    fn the_plan_leaves_the_remainder_wire_and_already_correct_wires_alone() {
        // 1200 lamps over 4 wires: 300 each. Wire 0 is already right, wire 3
        // takes the remainder, so only 1 and 2 get written.
        let face = face(
            Some(1200),
            &[
                (0, "IO18", Some(300)),
                (1, "IO16", Some(100)),
                (2, "IO14", Some(500)),
                (3, "IO2", None),
            ],
        );

        assert_eq!(spread_plan(&face), vec![(1, 300), (2, 300)]);
    }

    #[test]
    fn the_plan_writes_the_last_wire_when_its_count_is_authored() {
        let face = face(Some(90), &[(0, "IO18", Some(10)), (1, "IO16", Some(80))]);

        assert_eq!(spread_plan(&face), vec![(0, 45), (1, 45)]);
    }

    #[test]
    fn nothing_spreads_without_an_extent_or_a_second_wire() {
        assert!(
            spread_plan(&face(None, &[(0, "IO18", Some(10)), (1, "IO2", None)])).is_empty(),
            "the incoming extent is what there is to spread"
        );
        assert!(
            spread_plan(&face(Some(300), &[(0, "IO18", None)])).is_empty(),
            "one wire has nothing to spread across"
        );
    }

    #[test]
    fn the_label_says_which_split_the_gesture_performs() {
        let mut face = face(Some(400), &[(0, "IO18", Some(200)), (1, "IO2", None)]);
        assert_eq!(spread_label(&face), "divide lamps evenly");
        face.span_boundaries = vec![0, 90];
        assert_eq!(spread_label(&face), "fit counts to strips");
        // A grid that is only the buffer's own start is not a grid.
        face.span_boundaries = vec![0];
        assert_eq!(spread_label(&face), "divide lamps evenly");
    }

    #[test]
    fn only_wires_on_a_real_gpio_reach_the_diagram() {
        let mut face = face(Some(400), &[(0, "IO18", Some(200)), (1, "IO27", None)]);
        face.ports[0].gpio = Some(18);
        face.board = Some(UiOutputBoardFacts {
            board_id: "domraem/dom-z-102".to_string(),
            display_name: "DOM-Z-102".to_string(),
            pins: vec![UiOutputPin {
                label: "IO18".to_string(),
                gpio: 18,
                assigned_to: Some(0),
            }],
        });

        let wired = wired_connections(&face);
        assert_eq!(wired.len(), 1, "IO27 resolves to no gpio: {wired:?}");
        assert_eq!(wired[0].gpio, 18);
        assert_eq!(wired[0].title, "ch0");
        assert_eq!(wired[0].extra.as_deref(), Some("×200"));
    }

    #[test]
    fn the_summary_prefers_the_incoming_extent_over_the_authored_one() {
        let resolved = face(Some(1200), &[(0, "IO18", Some(300)), (1, "IO2", None)]);
        assert_eq!(ports_summary(&resolved), "2 wires · 1200 lamps");

        let authored = face(None, &[(0, "IO18", Some(300)), (1, "IO2", Some(60))]);
        assert_eq!(ports_summary(&authored), "2 wires · 360 lamps authored");

        let unknown = face(None, &[(0, "IO18", None)]);
        assert_eq!(ports_summary(&unknown), "1 wire · lamps unknown");
    }

    #[test]
    fn the_caption_reads_the_slice_and_names_an_unresolved_pin() {
        let mut face = face(Some(400), &[(0, "IO18", Some(200)), (1, "IO27", None)]);
        face.ports[0].gpio = Some(18);
        assert_eq!(port_caption(&face.ports[0], true), "lamps 0–199 · gpio 18");
        assert_eq!(
            port_caption(&face.ports[1], true),
            "lamps 200–399 · IO27 unresolved",
            "the KNOWN board has no IO27"
        );
    }

    #[test]
    fn without_a_board_a_pin_is_shown_plainly_rather_than_called_unresolved() {
        let face = face(Some(400), &[(0, "IO18", Some(200)), (1, "IO27", None)]);
        assert_eq!(port_caption(&face.ports[0], false), "lamps 0–199 · IO18");
        assert_eq!(port_caption(&face.ports[1], false), "lamps 200–399 · IO27");
        // And the row stays neutral: nothing has been checked, so nothing is
        // wrong (violet stays reserved for a wire on a real pin).
        for port in &face.ports {
            let chip = port_key_class(port, false);
            assert!(!chip.contains("status-attention"), "{chip}");
            assert!(!chip.contains("status-bound"), "{chip}");
        }
    }

    #[test]
    fn a_picked_pin_keeps_the_wire_s_capability_and_target() {
        assert_eq!(
            endpoint_with_pin("ws281x:local:IO18", "IO2"),
            "ws281x:local:IO2"
        );
        assert_eq!(
            endpoint_with_pin("ws281x:remote7:IO18", "D5"),
            "ws281x:remote7:D5"
        );
        assert_eq!(endpoint_with_pin("", "IO18"), "ws281x:local:IO18");
        assert_eq!(endpoint_with_pin("garbage", "IO18"), "ws281x:local:IO18");
    }

    #[test]
    fn the_added_wire_lands_past_the_highest_key() {
        assert_eq!(next_port_key(&face(None, &[])), 0);
        assert_eq!(
            next_port_key(&face(None, &[(0, "IO18", Some(1)), (4, "IO2", None)])),
            5
        );
    }
}
