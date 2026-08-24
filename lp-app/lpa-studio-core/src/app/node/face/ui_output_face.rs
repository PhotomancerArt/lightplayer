//! The output card's permanent face: one row per physical wire.

use crate::ProjectSlotAddress;

/// Permanent face for an output node card.
///
/// One output node drives N physical wires: `OutputDef.ports` is a map of
/// port index → `{endpoint, count}`, and the node's single control buffer
/// is split across those ports in key order. The face is that split made
/// visible — a row per wire, with the board diagram (when the running device's
/// board is known) as the picking surface for the wire's pin.
///
/// Two provenances meet here, and they are deliberately kept apart:
///
/// - **Authored + projected** ([`crate::UiOutputPortRow`] keys, endpoints,
///   counts, slot addresses, and the derived slice starts) come from the node's
///   own projected sections in `node_face_builder` — the face-derivation rule
///   (never re-read controller state).
/// - **Hardware and upstream facts** ([`Self::board`], [`Self::total_lamps`],
///   [`Self::span_boundaries`], and the remainder channel's resolved count) are
///   filled by the studio controller's decoration pass
///   (`app::studio::output_face_decoration`), because board identity lives on
///   the device registry and the node's incoming lamp extent lives on the
///   *upstream* node's produced product — neither is visible from this node's
///   sections.
///
/// **"No board known" is a first-class state**: a device provisioned outside
/// Studio has no `board_id`, so `board` stays `None`, the diagram is not
/// rendered, and every channel row stays fully editable.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiOutputFace {
    /// The authored output NAME (D39: "1", "Box 5") when set — what patch
    /// entries reference and what the patch surface shows for this output.
    /// `None` for the unnamed single-output ordinary case.
    pub name: Option<String>,
    /// One row per authored port, in ascending key order.
    pub ports: Vec<UiOutputPortRow>,
    /// Address of the `ports` map slot itself — the target for the generic
    /// map insert/remove entry ops (adding or dropping a wire).
    pub ports_address: Option<ProjectSlotAddress>,
    /// The binding endpoint feeding this output's control input (e.g.
    /// `bus:control.out`), when the input row is bound. The decoration pass
    /// uses it to find the upstream produced control product; the face can
    /// also show it.
    pub input_binding: Option<String>,
    /// Lamps the node's control input actually carries, when known.
    ///
    /// `None` until the upstream produced control product is visible (no
    /// project running, or the input is unbound) — in which case the remainder
    /// channel's [`UiOutputPortRow::resolved_count`] is unknown too.
    pub total_lamps: Option<u32>,
    /// The board's measured LED envelope vs the project's total use of it —
    /// board-wide (every face on the same device shows the same bar).
    /// `None` when the board carries no record or no device is connected.
    pub led_budget: Option<UiLedBudget>,
    /// Lamp indices where the upstream fixture's authored paths begin, in
    /// ascending order — the snapping grid for the channel-boundary gesture
    /// (P2's honest per-path spans).
    ///
    /// Empty when no sample layout has arrived (the upstream product has no
    /// materialized preview yet), which is a "no snapping" state, not an error.
    pub span_boundaries: Vec<u32>,
    /// The running device's board, when Studio knows which board it is.
    /// `None` = "no board known" — the first-class fallback state.
    pub board: Option<UiOutputBoardFacts>,
    /// The patch bay's output side (D34a): one row per port, cells laid
    /// along the wire, each labelled with the fixture it came from.
    ///
    /// Filled by the project controller's `apply_patch_bays` pass — the
    /// placements and the published frame both ride the output-frame probe,
    /// which this node's sections cannot see (same provenance rule as
    /// [`Self::board`]).
    ///
    /// Present for every output the pass can see, INCLUDING one nothing is
    /// patched onto: its ports are the geometry the def declares, and a
    /// wire with free space on it is a real state (manual flow, P5b), not
    /// an absence. `None` only when there is no wire pass at all (no live
    /// project). A card that has nothing to SHOW hides the section itself —
    /// the renderer's call, not a hole in the data (an empty bay is what
    /// the patch surface's free ports are made of).
    pub patch: Option<crate::UiPatchBay>,
}

impl UiOutputFace {
    /// Total lamps the authored ports claim, when every port says.
    ///
    /// `None` when a channel takes the remainder — only the highest-keyed one
    /// may, and what "the rest" is depends on the upstream extent.
    #[must_use]
    pub fn authored_lamps(&self) -> Option<u32> {
        self.ports
            .iter()
            .map(|channel| channel.count)
            .try_fold(0u32, |total, count| Some(total.saturating_add(count?)))
    }

    /// Resolve everything that depends on the node's incoming lamp extent:
    /// [`Self::total_lamps`] and the remainder channel's
    /// [`UiOutputPortRow::resolved_count`].
    ///
    /// The arithmetic mirrors the engine's own slice planner
    /// (`lpc_engine`'s `planned_wires`): counts are lamps, slices start where
    /// the previous one ended, and the count-less channel takes what is left —
    /// clamped at zero when the authored counts already overrun the buffer
    /// (the engine clamps there too, loudly).
    pub fn resolve_extent(&mut self, total_lamps: u32) {
        self.total_lamps = Some(total_lamps);
        for channel in &mut self.ports {
            if channel.resolved_count.is_some() {
                continue;
            }
            let Some(start) = channel.slice_start else {
                continue;
            };
            channel.resolved_count = Some(total_lamps.saturating_sub(start));
        }
    }
}

/// One physical wire driven by an output node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiOutputPortRow {
    /// The `ports` map key — port index, and the slice order.
    pub key: u32,
    /// The authored endpoint spec verbatim (`ws281x:local:IO18`).
    pub endpoint_display: String,
    /// The endpoint's config segment — the board's own label for the wire
    /// (`IO18`, `D10`, `LED1`). Empty when the spec is unparseable.
    pub pin_label: String,
    /// GPIO the [`Self::pin_label`] resolves to on the known board.
    ///
    /// `None` while no board is known (the whole face is in its no-board
    /// state) OR when the board carries no such label — an unresolved pin is
    /// SHOWN, never hidden: a strip wired to a pin this board does not have is
    /// exactly what the face exists to surface.
    pub gpio: Option<u32>,
    /// The authored lamp count. `None` = the remainder channel (only the
    /// highest key may omit it).
    pub count: Option<u32>,
    /// Lamps this wire actually drives: the authored count, or — for the
    /// remainder channel — whatever is left of the node's control buffer once
    /// the decoration pass knows the incoming extent. `None` = not yet known.
    pub resolved_count: Option<u32>,
    /// First lamp of this wire's slice, counting from the node's buffer start.
    /// `None` only when a preceding channel's count is missing (an authoring
    /// mistake the engine refuses; the face degrades rather than guesses).
    pub slice_start: Option<u32>,
    /// Address of this channel's `endpoint` slot — the web edits the wire
    /// through the normal slot write path (`ports[k].endpoint`).
    pub endpoint_address: Option<ProjectSlotAddress>,
    /// Address of this port's `count` slot (`ports[k].count`); the row
    /// is an `OptionSlot`, so the value edit targets its interior `some`.
    pub count_address: Option<ProjectSlotAddress>,
    /// Live per-wire transmission status from the device's heartbeat, joined
    /// by GPIO. `None` while no device is reporting (sim, disconnected, a
    /// firmware without per-wire attribution) — absence is a state the face
    /// must render quietly, not an error.
    pub wire_status: Option<UiWireStatus>,
}

/// Live transmission health for one wire, as the face shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiWireStatus {
    /// Frames transmitted since boot.
    pub sent: u32,
    /// Frames torn on the strand (guard-word truncation). Zero is the
    /// healthy steady state; any growth is distress.
    pub torn: u32,
    /// This wire runs in the second wave — it waits for a pooled
    /// transmitter slot each frame (more wires than slots). Discrete state
    /// language: the face renders it as a squared block, not a warning.
    pub waves: bool,
    /// Worst observed wait for a slot, in ms (rounded up from µs).
    pub queue_wait_ms: u32,
}

/// The board's measured total-LED envelope against the project's use of it.
///
/// A SOFT limit (see `lpc-hardware`'s `HwSoftLimits`): `used > budget` is
/// rendered as advice (amber), never as an error — the record is evidence
/// of what has run clean, and exceeding it is exploring, not breaking.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiLedBudget {
    /// Lamps this project drives across every output node's wires.
    pub used: u32,
    /// The board's measured envelope.
    pub budget: u32,
}

/// What Studio knows about the board the running device is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiOutputBoardFacts {
    /// `vendor/product` — the display-manifest id the web renders a diagram
    /// for.
    pub board_id: String,
    /// The board's display name, so the face can name it without re-reading
    /// the manifest.
    pub display_name: String,
    /// Every pin an LED wire may be driven from, rails and screw terminals
    /// alike (the terminals list is separate from the rails in the display
    /// manifest and is easy to drop — the eligible set is the union).
    pub pins: Vec<UiOutputPin>,
}

/// One output-eligible pin on the known board.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiOutputPin {
    /// Silkscreen label — what an endpoint's config segment names.
    pub label: String,
    pub gpio: u32,
    /// Channel key currently driving this pin, when one is.
    pub assigned_to: Option<u32>,
}
