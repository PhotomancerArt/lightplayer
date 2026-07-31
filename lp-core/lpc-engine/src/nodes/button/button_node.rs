//! Runtime hardware button node: polls a debounced input and produces control maps.

use alloc::boxed::Box;
use alloc::format;
use lp_collection::VecMap;

use lpc_hardware::{ButtonConfig, ButtonEventKind, ButtonInput};
use lpc_model::{
    ButtonDefView, ButtonState, ControlMessage, HwEndpointSpec, MapSlot, Revision, SlotAccess,
    SlotPath, SlotShapeRegistry, SlotShapeRegistryError,
};
use lpc_wire::{WireButtonEvent, WireNodeCommand};

use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RuntimeStateShape, TickContext,
};

/// How many synthetic presses a single button node holds at once.
///
/// Bounded on purpose: the set lives in a fixed array so a misbehaving (or
/// simply chatty) client cannot grow node state on an embedded target.
const SYNTHETIC_PRESS_CAPACITY: usize = 4;

/// TTL applied when a `Press` command asks for `ttl_ms == 0`.
const DEFAULT_PRESS_TTL_MS: u32 = 5000;

/// Runtime node for `kind = "Button"` artifacts.
pub struct ButtonNode {
    state: ButtonState,
    def_view: Option<ButtonDefView>,
    input: Option<Box<dyn ButtonInput>>,
    opened: Option<OpenedButton>,
    held_id_seq: Option<(u32, u32)>,
    fallback_now_ms: u64,
    /// Last debounced hardware level, latched from `poll` edges.
    hw_pressed: bool,
    /// Merged state published on the previous produce; `down`/`up` edges
    /// derive from CHANGES against it.
    effective_pressed: bool,
    /// Node-owned message sequence, bumped once per effective edge. The
    /// hardware event's own sequence is not forwarded any more: with two
    /// sources feeding one logical button, only a node-owned counter stays
    /// collision-free for consumers that dedup by `(id, seq)`.
    edge_seq: u32,
    /// Clicks queued by `WireButtonEvent::Click`, applied one produce at a
    /// time (queue in `handle_command`, merge in `produce`).
    pending_clicks: u8,
    /// True while the frame that a queued click made effective is the most
    /// recent one, so the next produce releases it.
    click_active: bool,
    /// Sustained synthetic presses, keyed by client-generated `press_id`.
    presses: [Option<SyntheticPress>; SYNTHETIC_PRESS_CAPACITY],
}

impl ButtonNode {
    pub fn new() -> Self {
        Self {
            state: ButtonState::default(),
            def_view: None,
            input: None,
            opened: None,
            held_id_seq: None,
            fallback_now_ms: 0,
            hw_pressed: false,
            effective_pressed: false,
            edge_seq: 0,
            pending_clicks: 0,
            click_active: false,
            presses: [None; SYNTHETIC_PRESS_CAPACITY],
        }
    }

    fn read_config(&mut self, ctx: &mut TickContext<'_>) -> Result<ButtonRuntimeConfig, NodeError> {
        let def = ButtonDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(|e| NodeError::msg(format!("compile button def view: {e}")))?;
        Ok(ButtonRuntimeConfig {
            endpoint: def.endpoint().get(ctx)?,
            id: def.id().get::<_, u32>(ctx)?,
            stable_ms: u64::from(def.stable_ms().get::<_, u32>(ctx)?),
        })
    }

    fn ensure_input(
        &mut self,
        config: &ButtonRuntimeConfig,
        ctx: &TickContext<'_>,
    ) -> Result<(), NodeError> {
        let opened = OpenedButton {
            endpoint: config.endpoint.clone(),
            stable_ms: config.stable_ms,
        };
        if self.opened.as_ref() == Some(&opened) && self.input.is_some() {
            return Ok(());
        }

        let service = ctx
            .button_service()
            .ok_or_else(|| NodeError::msg("button node has no button service"))?;
        let input = service
            .open_button_by_spec(&config.endpoint, ButtonConfig::new(config.stable_ms))
            .map_err(|error| NodeError::msg(format!("open button {}: {error}", config.endpoint)))?;
        self.input = Some(input);
        self.opened = Some(opened);
        // A reopened endpoint starts from a clean edge state; synthetic
        // presses are client-owned and survive, so a live hold re-arms with a
        // fresh down edge on the next produce.
        self.held_id_seq = None;
        self.hw_pressed = false;
        self.effective_pressed = false;
        Ok(())
    }

    fn next_now_ms(&mut self, ctx: &TickContext<'_>) -> u64 {
        if let Some(now_ms) = ctx.now_ms() {
            self.fallback_now_ms = now_ms;
            return now_ms;
        }
        self.fallback_now_ms = self.fallback_now_ms.saturating_add(1);
        self.fallback_now_ms
    }

    /// One frame of the hardware/synthetic merge.
    ///
    /// There is ONE logical button: the hardware level, the click phase, and
    /// the synthetic press set fold into a single `effective_pressed`, and the
    /// `down`/`held`/`up` slots derive from CHANGES in it. Overlapping sources
    /// therefore add no spurious edges, and neither source's release ends the
    /// other's hold.
    ///
    /// Split out of [`NodeRuntime::produce`] so the merge rule is unit
    /// testable without a live [`TickContext`].
    fn advance_frame(
        &mut self,
        revision: Revision,
        id: u32,
        now_ms: u64,
        hw_event: Option<ButtonEventKind>,
    ) {
        if let Some(kind) = hw_event {
            self.hw_pressed = matches!(kind, ButtonEventKind::Pressed);
        }
        self.sweep_presses(now_ms);
        let click_pressed = self.advance_click_phase();
        let effective = self.hw_pressed || click_pressed || self.has_active_press();

        let mut down = MapSlot::default();
        let mut up = MapSlot::default();
        if effective != self.effective_pressed {
            self.effective_pressed = effective;
            self.edge_seq = self.edge_seq.wrapping_add(1);
            if effective {
                self.held_id_seq = Some((id, self.edge_seq));
                down = one_message_map(revision, id, self.edge_seq);
            } else {
                self.held_id_seq = None;
                up = one_message_map(revision, id, self.edge_seq);
            }
        }
        let held = self
            .held_id_seq
            .map(|(held_id, seq)| one_message_map(revision, held_id, seq))
            .unwrap_or_default();

        self.state.down = down;
        self.state.held = held;
        self.state.up = up;
    }

    /// Expire (and lazily stamp) synthetic presses against the produce clock.
    ///
    /// `handle_command` cannot stamp deadlines itself: its `time_s` is the
    /// engine's frame clock, a different domain from the [`TickContext`] time
    /// provider `produce` polls the debouncer with. Entries therefore arrive
    /// with a TTL budget and get their deadline on the first produce that
    /// sees them — which is also what makes a renewal simply clear the stamp.
    fn sweep_presses(&mut self, now_ms: u64) {
        for slot in &mut self.presses {
            let Some(press) = slot.as_mut() else {
                continue;
            };
            match press.expires_at_ms {
                None => {
                    press.expires_at_ms = Some(now_ms.saturating_add(u64::from(press.ttl_ms)));
                }
                Some(deadline) if now_ms >= deadline => *slot = None,
                Some(_) => {}
            }
        }
    }

    fn has_active_press(&self) -> bool {
        self.presses.iter().any(Option::is_some)
    }

    /// Click phasing: a queued click is effective for exactly one produce and
    /// released on the next, so queued clicks serialize into distinct down/up
    /// pairs instead of merging into one long hold. If another source holds
    /// the button meanwhile, the click simply dissolves into that hold.
    fn advance_click_phase(&mut self) -> bool {
        if self.click_active {
            self.click_active = false;
            return false;
        }
        if self.pending_clicks == 0 {
            return false;
        }
        self.pending_clicks -= 1;
        self.click_active = true;
        true
    }

    /// Insert or renew a sustained synthetic press.
    fn begin_press(&mut self, press_id: u32, ttl_ms: u32) -> Result<(), NodeError> {
        let ttl_ms = if ttl_ms == 0 {
            DEFAULT_PRESS_TTL_MS
        } else {
            ttl_ms
        };
        if let Some(press) = self
            .presses
            .iter_mut()
            .filter_map(Option::as_mut)
            .find(|press| press.press_id == press_id)
        {
            press.ttl_ms = ttl_ms;
            press.expires_at_ms = None;
            return Ok(());
        }
        let Some(slot) = self.presses.iter_mut().find(|slot| slot.is_none()) else {
            return Err(NodeError::msg(format!(
                "button already holds {SYNTHETIC_PRESS_CAPACITY} synthetic presses"
            )));
        };
        *slot = Some(SyntheticPress {
            press_id,
            ttl_ms,
            expires_at_ms: None,
        });
        Ok(())
    }

    /// End a sustained synthetic press. An unknown id is a normal data-level
    /// rejection — a Release that lost the race with its own TTL lands here.
    fn end_press(&mut self, press_id: u32) -> Result<(), NodeError> {
        let Some(slot) = self
            .presses
            .iter_mut()
            .find(|slot| slot.is_some_and(|press| press.press_id == press_id))
        else {
            return Err(NodeError::msg(format!(
                "button has no synthetic press {press_id}"
            )));
        };
        *slot = None;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ButtonRuntimeConfig {
    endpoint: HwEndpointSpec,
    id: u32,
    stable_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpenedButton {
    endpoint: HwEndpointSpec,
    stable_ms: u64,
}

/// One sustained synthetic press held by the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SyntheticPress {
    /// Client-generated id, scoped to this node, pairing Press with Release.
    press_id: u32,
    /// Lifetime granted by the latest Press command for this id.
    ttl_ms: u32,
    /// Deadline in the produce clock's domain; `None` until the next produce
    /// stamps it (see [`ButtonNode::sweep_presses`]).
    expires_at_ms: Option<u64>,
}

impl NodeRuntime for ButtonNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        let config = self.read_config(ctx)?;
        self.ensure_input(&config, ctx)?;
        let now_ms = self.next_now_ms(ctx);

        // Poll every frame regardless of synthetic state: the debouncer is a
        // sampler, and skipping it would stall the hardware edge detector.
        let hw_event = self
            .input
            .as_mut()
            .ok_or_else(|| NodeError::msg("button input missing after open"))?
            .poll(now_ms)
            .map(|event| event.kind());

        self.advance_frame(ctx.revision(), config.id, now_ms, hw_event);
        Ok(ProduceResult::Produced)
    }

    /// Synthetic button events (the wire runtime command channel): queue the
    /// event here, merge it into the one logical button state in `produce`.
    ///
    /// Writing `down`/`held`/`up` directly would be lost — `produce` rebuilds
    /// all three from the poll each frame — so nothing is stamped here.
    /// Sustained presses carry a TTL rather than trusting a Release to
    /// arrive: a dropped Release would otherwise wedge the button held
    /// forever.
    fn handle_command(&mut self, command: &WireNodeCommand, _time_s: f32) -> Result<(), NodeError> {
        let WireNodeCommand::ButtonEvent { event } = command else {
            return Err(NodeError::msg("button does not support this command"));
        };
        match event {
            WireButtonEvent::Click => {
                self.pending_clicks = self.pending_clicks.saturating_add(1);
                Ok(())
            }
            WireButtonEvent::Press { press_id, ttl_ms } => self.begin_press(*press_id, *ttl_ms),
            WireButtonEvent::Release { press_id } => self.end_press(*press_id),
        }
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        self.input = None;
        self.opened = None;
        self.held_id_seq = None;
        self.hw_pressed = false;
        self.effective_pressed = false;
        self.pending_clicks = 0;
        self.click_active = false;
        self.presses = [None; SYNTHETIC_PRESS_CAPACITY];
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        Ok(())
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        ButtonState::register_runtime_state_shape(registry).map(|_| ())
    }
}

fn one_message_map(revision: Revision, id: u32, seq: u32) -> MapSlot<u32, ControlMessage> {
    let mut entries = VecMap::new();
    entries.insert(id, ControlMessage::new(id, seq));
    MapSlot::with_version(revision, entries)
}

pub fn button_down_path() -> SlotPath {
    SlotPath::parse("down").expect("button down path")
}

pub fn button_held_path() -> SlotPath {
    SlotPath::parse("held").expect("button held path")
}

pub fn button_up_path() -> SlotPath {
    SlotPath::parse("up").expect("button up path")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn click_fires_down_and_held_on_one_frame_then_up_on_the_next() {
        let mut node = ButtonNode::new();
        node.handle_command(&click(), 0.0).expect("click accepted");

        frame(&mut node, 0, None);
        assert_eq!(down_seq(&node), Some(1));
        assert_eq!(held_seq(&node), Some(1));
        assert!(node.state.up.is_empty());

        frame(&mut node, 10, None);
        assert!(node.state.down.is_empty());
        assert!(node.state.held.is_empty());
        assert_eq!(up_seq(&node), Some(2));

        frame(&mut node, 20, None);
        assert!(node.state.down.is_empty());
        assert!(node.state.held.is_empty());
        assert!(node.state.up.is_empty());
    }

    #[test]
    fn queued_clicks_serialize_into_separate_edge_pairs() {
        let mut node = ButtonNode::new();
        node.handle_command(&click(), 0.0).expect("first click");
        node.handle_command(&click(), 0.0).expect("second click");

        frame(&mut node, 0, None);
        assert!(!node.state.down.is_empty(), "first click down");
        frame(&mut node, 10, None);
        assert!(!node.state.up.is_empty(), "first click up");
        frame(&mut node, 20, None);
        assert!(!node.state.down.is_empty(), "second click down");
        frame(&mut node, 30, None);
        assert!(!node.state.up.is_empty(), "second click up");
        frame(&mut node, 40, None);
        assert!(node.state.down.is_empty());
        assert!(node.state.up.is_empty());
    }

    #[test]
    fn press_holds_across_frames_and_release_emits_the_up_edge() {
        let mut node = ButtonNode::new();
        node.handle_command(&press(1, 1000), 0.0)
            .expect("press accepted");

        frame(&mut node, 0, None);
        assert_eq!(down_seq(&node), Some(1));
        assert_eq!(held_seq(&node), Some(1));

        for now_ms in [10, 20, 30] {
            frame(&mut node, now_ms, None);
            assert!(node.state.down.is_empty(), "no repeat down at {now_ms}");
            assert_eq!(held_seq(&node), Some(1), "still held at {now_ms}");
            assert!(node.state.up.is_empty(), "no up at {now_ms}");
        }

        node.handle_command(&release(1), 0.0).expect("release");
        frame(&mut node, 40, None);
        assert!(node.state.held.is_empty());
        assert_eq!(up_seq(&node), Some(2));
    }

    #[test]
    fn press_auto_releases_at_ttl_expiry() {
        let mut node = ButtonNode::new();
        node.handle_command(&press(1, 100), 0.0).expect("press");

        frame(&mut node, 0, None);
        assert_eq!(down_seq(&node), Some(1));
        frame(&mut node, 99, None);
        assert_eq!(held_seq(&node), Some(1));

        frame(&mut node, 100, None);
        assert!(node.state.held.is_empty(), "TTL expiry releases the hold");
        assert_eq!(up_seq(&node), Some(2));
    }

    #[test]
    fn renewing_a_press_extends_its_ttl() {
        let mut node = ButtonNode::new();
        node.handle_command(&press(1, 100), 0.0).expect("press");
        frame(&mut node, 0, None);

        node.handle_command(&press(1, 100), 0.0).expect("renewal");
        frame(&mut node, 50, None);
        assert!(node.state.down.is_empty(), "renewal is not a new edge");

        frame(&mut node, 100, None);
        assert_eq!(held_seq(&node), Some(1), "original deadline was extended");
        assert!(node.state.up.is_empty());

        frame(&mut node, 150, None);
        assert!(node.state.held.is_empty(), "renewed deadline expires");
        assert_eq!(up_seq(&node), Some(2));
    }

    #[test]
    fn press_with_zero_ttl_uses_the_default_ttl() {
        let mut node = ButtonNode::new();
        node.handle_command(&press(1, 0), 0.0).expect("press");

        frame(&mut node, 0, None);
        assert_eq!(
            node.presses[0].expect("press stored").expires_at_ms,
            Some(u64::from(DEFAULT_PRESS_TTL_MS))
        );
    }

    #[test]
    fn hardware_and_synthetic_presses_merge_into_one_logical_button() {
        let mut node = ButtonNode::new();
        node.handle_command(&press(1, 10_000), 0.0).expect("press");

        frame(&mut node, 0, None);
        assert_eq!(down_seq(&node), Some(1));

        // Hardware presses on top of the synthetic hold: no second down.
        frame(&mut node, 10, Some(ButtonEventKind::Pressed));
        assert!(node.state.down.is_empty(), "no extra down edge");
        assert_eq!(held_seq(&node), Some(1));

        // Hardware releases first: the synthetic hold keeps the button down.
        frame(&mut node, 20, Some(ButtonEventKind::Released));
        assert!(
            node.state.up.is_empty(),
            "synthetic hold survives hw release"
        );
        assert_eq!(held_seq(&node), Some(1));

        // Only when the last source ends does the up edge fire.
        node.handle_command(&release(1), 0.0).expect("release");
        frame(&mut node, 30, None);
        assert!(node.state.held.is_empty());
        assert_eq!(up_seq(&node), Some(2));
    }

    #[test]
    fn synthetic_release_during_a_hardware_hold_keeps_the_button_down() {
        let mut node = ButtonNode::new();

        frame(&mut node, 0, Some(ButtonEventKind::Pressed));
        assert_eq!(down_seq(&node), Some(1));

        node.handle_command(&press(1, 10_000), 0.0).expect("press");
        frame(&mut node, 10, None);
        assert!(node.state.down.is_empty(), "no extra down edge");

        node.handle_command(&release(1), 0.0).expect("release");
        frame(&mut node, 20, None);
        assert!(
            node.state.up.is_empty(),
            "hardware hold survives synthetic release"
        );
        assert_eq!(held_seq(&node), Some(1));

        frame(&mut node, 30, Some(ButtonEventKind::Released));
        assert_eq!(up_seq(&node), Some(2));
    }

    #[test]
    fn click_during_an_active_hold_adds_no_edges() {
        let mut node = ButtonNode::new();
        node.handle_command(&press(1, 10_000), 0.0).expect("press");
        frame(&mut node, 0, None);
        assert_eq!(down_seq(&node), Some(1));

        node.handle_command(&click(), 0.0).expect("click");
        for now_ms in [10, 20, 30] {
            frame(&mut node, now_ms, None);
            assert!(node.state.down.is_empty(), "no down at {now_ms}");
            assert!(node.state.up.is_empty(), "no up at {now_ms}");
            assert_eq!(held_seq(&node), Some(1), "still held at {now_ms}");
        }

        node.handle_command(&release(1), 0.0).expect("release");
        frame(&mut node, 40, None);
        assert_eq!(up_seq(&node), Some(2), "exactly one up edge for the pair");
    }

    #[test]
    fn a_fifth_concurrent_press_is_rejected() {
        let mut node = ButtonNode::new();
        for press_id in 1..=4 {
            node.handle_command(&press(press_id, 1000), 0.0)
                .unwrap_or_else(|e| panic!("press {press_id} accepted: {e}"));
        }

        let err = node
            .handle_command(&press(5, 1000), 0.0)
            .expect_err("overflow rejected");
        assert!(err.to_string().contains("4 synthetic presses"), "{err}");

        // The full set is untouched by the rejected command.
        assert_eq!(node.presses.iter().filter(|slot| slot.is_some()).count(), 4);
        assert!(
            node.presses
                .iter()
                .flatten()
                .all(|press| press.press_id != 5)
        );
    }

    #[test]
    fn renewing_an_existing_press_does_not_consume_a_slot() {
        let mut node = ButtonNode::new();
        for press_id in 1..=4 {
            node.handle_command(&press(press_id, 1000), 0.0)
                .expect("press accepted");
        }

        node.handle_command(&press(2, 2000), 0.0)
            .expect("renewal of a held id is accepted even when full");

        assert_eq!(node.presses.iter().filter(|slot| slot.is_some()).count(), 4);
    }

    #[test]
    fn releasing_an_unknown_press_id_is_rejected_and_leaves_state_untouched() {
        let mut node = ButtonNode::new();
        node.handle_command(&press(1, 1000), 0.0).expect("press");
        frame(&mut node, 0, None);

        let err = node
            .handle_command(&release(9), 0.0)
            .expect_err("unknown id rejected");
        assert!(err.to_string().contains("no synthetic press 9"), "{err}");

        frame(&mut node, 10, None);
        assert_eq!(held_seq(&node), Some(1), "the real hold is untouched");
        assert!(node.state.up.is_empty());
    }

    #[test]
    fn non_button_commands_are_rejected() {
        let mut node = ButtonNode::new();

        for command in [
            WireNodeCommand::PlaylistActivateEntry { entry: 1 },
            WireNodeCommand::OutputTestPattern {
                pattern: lpc_wire::WireOutputTestPattern::Clear,
                ttl_ms: 0,
            },
        ] {
            let err = node
                .handle_command(&command, 0.0)
                .expect_err("non-button command rejected");
            assert!(err.to_string().contains("does not support"), "{err}");
        }
    }

    const TEST_BUTTON_ID: u32 = 7;

    fn frame(node: &mut ButtonNode, now_ms: u64, hw_event: Option<ButtonEventKind>) {
        node.advance_frame(Revision::new(1), TEST_BUTTON_ID, now_ms, hw_event);
    }

    fn click() -> WireNodeCommand {
        WireNodeCommand::ButtonEvent {
            event: WireButtonEvent::Click,
        }
    }

    fn press(press_id: u32, ttl_ms: u32) -> WireNodeCommand {
        WireNodeCommand::ButtonEvent {
            event: WireButtonEvent::Press { press_id, ttl_ms },
        }
    }

    fn release(press_id: u32) -> WireNodeCommand {
        WireNodeCommand::ButtonEvent {
            event: WireButtonEvent::Release { press_id },
        }
    }

    fn down_seq(node: &ButtonNode) -> Option<u32> {
        message_seq(&node.state.down)
    }

    fn held_seq(node: &ButtonNode) -> Option<u32> {
        message_seq(&node.state.held)
    }

    fn up_seq(node: &ButtonNode) -> Option<u32> {
        message_seq(&node.state.up)
    }

    fn message_seq(slot: &MapSlot<u32, ControlMessage>) -> Option<u32> {
        slot.entries
            .get(&TEST_BUTTON_ID)
            .map(|message| message.seq())
    }
}
