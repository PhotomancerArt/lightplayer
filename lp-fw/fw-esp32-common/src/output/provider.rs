//! ESP32 OutputProvider implementation.
//!
//! The provider is the compatibility layer used by the engine. Hardware-specific
//! details live in capability drivers registered on the root `HardwareSystem`.

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::cell::RefCell;
use lp_collection::VecMap;

use lpc_hardware::OutputError;
use lpc_hardware::{
    HardwareEndpointError, HardwareSystem, HwEndpointSpec, WS281X_MAX_LEDS_PER_CHANNEL,
    Ws281xConfig, Ws281xOutput, ws281x_capped_byte_count,
};
use lpc_shared::DisplayPipeline;
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};

use super::power_gate::{PowerGateController, is_all_black};
const FRAME_INTERVAL_US: u64 = 16_667;
const MID_FRAME_US: u64 = 8_333;

/// Channels reserved up front in [`Esp32OutputProvider::channels`].
///
/// A **reservation, not a limit** — `VecMap` still grows past it. It exists so
/// that opening the Nth channel does not reallocate and memcpy the previous
/// N-1 `ChannelState`s while they are live, which is a transient peak of twice
/// the steady-state size on the one path that runs when the heap is already at
/// its high-water mark.
///
/// 8 is the widest RMT TX slot count in the family (the classic's eight,
/// before the block plan absorbs slots into wider windows). At today's
/// `ChannelState` size this reservation costs well under a kilobyte.
///
/// ⚠️ Historical note worth keeping: this used to matter enormously.
/// `ChannelState` carried a 3,084-byte inline white-point LUT, so the same
/// growth asked for 12,864 contiguous bytes and OOM'd the classic ESP32 with
/// 11,228 free — `docs/defects/2026-08-01-classic-rmt-open-fault.md`. The LUT
/// is gone; this guard remains because the growth pattern is what turned a
/// tight heap into a hard fault, and it will do so again for whatever the next
/// large per-channel field is.
const RESERVED_CHANNELS: usize = 8;

struct ChannelState {
    /// ⚠️ Declared before `frame` on purpose: fields drop in declaration
    /// order, and the output's own `Drop` is what stops an in-flight
    /// transmission that may still be reading `frame`'s bytes. Swapping these
    /// two fields would free the frame under a running transmitter.
    output: Box<dyn Ws281xOutput>,
    byte_count: u32,
    pipeline: DisplayPipeline,
    /// The channel's own rendered frame, alive between writes.
    ///
    /// It used to be a `Vec` allocated inside every `write`, which is fine
    /// while one handle is written at a time. It is not fine for a concurrent
    /// transmission, where every channel's bytes must stay alive from its
    /// `start` until its `wait_complete` — so the storage belongs to the
    /// channel, not to the call. Sized from the channel's granted
    /// `byte_count`, so this is the same memory the per-write allocation held,
    /// just held for longer.
    frame: Vec<u8>,
    /// Which of the board's power gates hold this channel's supply up (bit
    /// `n` = gate `n`); zero on every board that declares none.
    ///
    /// Resolved once, at `open`, from the endpoint's address — the write path
    /// must not walk gate descriptors or compare addresses per frame.
    gate_mask: u32,
}

/// ESP32 OutputProvider implementation.
pub struct Esp32OutputProvider {
    hardware_system: Rc<HardwareSystem>,
    channels: RefCell<VecMap<i32, ChannelState>>,
    next_handle: RefCell<i32>,
    /// The board's switched power rails, or `None` on a board with none.
    ///
    /// `Option` outside the `RefCell`, not inside: the ungated board's write
    /// path then costs a discriminant test rather than a borrow, which is the
    /// "zero overhead beyond a `None` check" the design asked for.
    power_gates: Option<RefCell<PowerGateController>>,
}

impl Esp32OutputProvider {
    pub fn new(hardware_system: Rc<HardwareSystem>) -> Self {
        Self {
            hardware_system,
            channels: RefCell::new(VecMap::with_capacity(RESERVED_CHANNELS)),
            next_handle: RefCell::new(1),
            power_gates: None,
        }
    }

    /// Give the provider the board's power gates, built by the chip crate
    /// (which owns the GPIO and the clock). A builder rather than a `new`
    /// parameter so the chips with no gated rail construct exactly as before.
    pub fn with_power_gates(mut self, gates: PowerGateController) -> Self {
        self.power_gates = Some(RefCell::new(gates));
        self
    }
}

impl OutputProvider for Esp32OutputProvider {
    fn open(
        &self,
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, OutputError> {
        let options = options.unwrap_or_default();
        log::debug!(
            "Esp32OutputProvider::open: endpoint={endpoint}, byte_count={byte_count}, format={format:?}"
        );

        if format != OutputFormat::Ws2811 {
            log::warn!("Esp32OutputProvider::open: Unsupported format: {format:?}");
            return Err(OutputError::InvalidConfig {
                reason: format!("Unsupported format: {format:?}"),
            });
        }
        if byte_count < 3 {
            log::warn!("Esp32OutputProvider::open: byte_count {byte_count} too small");
            return Err(OutputError::InvalidConfig {
                reason: "byte_count must be at least 3 (one LED)".into(),
            });
        }

        let (byte_count, truncated) = ws281x_capped_byte_count(byte_count);
        if truncated {
            log::warn!(
                "Esp32OutputProvider::open: endpoint={endpoint} asked for more than \
                 {WS281X_MAX_LEDS_PER_CHANNEL} LEDs; truncating to {byte_count} bytes"
            );
        }
        let output = self
            .hardware_system
            .open_ws281x_by_spec(endpoint, Ws281xConfig::new(byte_count))
            .map_err(endpoint_error_to_output_error)?;
        let pipeline = DisplayPipeline::new(byte_count / 3, options.clone()).map_err(|error| {
            OutputError::InvalidConfig {
                reason: format!("DisplayPipeline allocation failed: {error}"),
            }
        })?;

        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id);

        log::info!(
            "Esp32OutputProvider::open: Opened channel handle={handle_id}, endpoint={endpoint}, byte_count={byte_count}"
        );

        let mut frame = Vec::new();
        frame.resize(((byte_count / 3) * 3) as usize, 0);

        // The endpoint's address is what a gate's `feeds` names, and it is
        // only reachable through the driver that offered it — the opaque
        // `Box<dyn Ws281xOutput>` carries no address. Resolved here, once,
        // because `ws281x_endpoints` allocates and opens are rare.
        let gate_mask = match &self.power_gates {
            None => 0,
            Some(gates) => {
                let address = self
                    .hardware_system
                    .ws281x_endpoints()
                    .into_iter()
                    .find(|candidate| candidate.spec() == endpoint)
                    .map(|candidate| candidate.address().clone());
                gates.borrow().mask_for(address.as_ref())
            }
        };

        self.channels.borrow_mut().insert(
            handle_id,
            ChannelState {
                output,
                byte_count,
                pipeline,
                frame,
                gate_mask,
            },
        );

        // The board's measured total-LED envelope, if it carries one. A SOFT
        // limit: exceeding it warns and proceeds — the record is evidence of
        // what has run clean (heap binds before frame time on the classic),
        // never a refusal. Checked at open, not per frame: opens are rare and
        // the sum only changes here.
        if let Some(budget) = self
            .hardware_system
            .registry()
            .manifest()
            .soft_limits()
            .and_then(|limits| limits.total_leds.as_ref())
        {
            let total_leds: u32 = self
                .channels
                .borrow()
                .iter()
                .map(|(_, channel)| channel.byte_count / 3)
                .sum();
            if total_leds > budget.value {
                log::warn!(
                    "Esp32OutputProvider::open: total LEDs across outputs ({total_leds}) \
                     exceed the board's measured envelope of {} — proceeding; expect heap \
                     pressure. Envelope provenance: {}",
                    budget.value,
                    budget.measured,
                );
            }
        }

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();
        log::debug!(
            "Esp32OutputProvider::write: handle={}, data_len={}",
            handle_id,
            data.len()
        );

        let mut channels = self.channels.borrow_mut();
        let channel = channels.get_mut(&handle_id).ok_or_else(|| {
            log::warn!("Esp32OutputProvider::write: Invalid handle {handle_id}");
            OutputError::InvalidHandle { handle: handle_id }
        })?;

        // Energise before staging, never after: this returns only once the
        // rail's settle window has fully elapsed, so the `start` below cannot
        // clock data into an unpowered strip. `data` is already post-gamma,
        // post-brightness and post-power-limit, so all-black here is exactly
        // the "nothing is lit" the gate wants — and the scan is reached only
        // on a gated channel of a gated board.
        if let Some(gates) = &self.power_gates {
            if channel.gate_mask != 0 {
                gates
                    .borrow_mut()
                    .on_frame(channel.gate_mask, is_all_black(data));
            }
        }

        // The previous frame may still be on the wire — `start` below returns
        // without waiting, and the transmitter keeps reading `channel.frame`
        // until it finishes. Wait it out before anything below touches state
        // it may still be reading (the resize moves the frame's bytes; the
        // pipeline overwrites them). In the steady state a whole render has
        // passed since the frame started, so this returns without spinning;
        // it only actually waits when a frame hung, in which case it aborts
        // and reports rather than staging over a live transmission.
        channel.output.wait_complete()?;

        let mut num_leds = (channel.byte_count / 3) as usize;
        let expected_len = num_leds * 3;

        if data.len() > expected_len {
            let (new_byte_count, truncated) = capped_byte_count_for_len(data.len());
            // A frame past the cap keeps `data.len() > expected_len` true forever;
            // only act (and warn) when the granted size actually changes, so the
            // steady state neither re-resizes nor logs per frame.
            if new_byte_count != channel.byte_count {
                if truncated {
                    log::warn!(
                        "Esp32OutputProvider::write: handle={handle_id} grew past \
                         {WS281X_MAX_LEDS_PER_CHANNEL} LEDs; truncating to {new_byte_count} bytes"
                    );
                }
                channel.output.resize(Ws281xConfig::new(new_byte_count))?;
                channel.pipeline.resize(new_byte_count / 3);
                channel.byte_count = new_byte_count;
                num_leds = (channel.byte_count / 3) as usize;
            }
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        // The channel owns its frame storage, so a resize is the only time
        // this allocates and a steady-state write allocates nothing at all.
        channel.frame.resize(num_leds * 3, 0);

        channel.pipeline.write_frame(0, data);
        channel.pipeline.write_frame(FRAME_INTERVAL_US, data);
        let ChannelState {
            output,
            pipeline,
            frame,
            ..
        } = channel;
        pipeline.tick(MID_FRAME_US, frame);

        // Start the transmission and return without waiting for it. Handles
        // written back to back in one flush therefore transmit concurrently —
        // the frame pays for its slowest wave of wires, not the sum of all of
        // them — and the wait happens in [`OutputProvider::flush`], the
        // frame-wide barrier the engine calls after the last write.
        //
        // SAFETY: `frame` is heap storage owned by this channel's entry in
        // `self.channels`, so its bytes stay in place when the map grows or
        // the `ChannelState` moves. Every path that mutates or moves those
        // bytes — this function's resize and staging above — first calls
        // `wait_complete`, and dropping the entry drops `output` (which stops
        // the transmission) before `frame` (see the field-order note on
        // `ChannelState`). That is exactly the lifetime `start`'s contract
        // asks for.
        unsafe { output.start(frame) }
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();
        self.channels
            .borrow_mut()
            .remove(&handle_id)
            .ok_or_else(|| OutputError::InvalidHandle { handle: handle_id })?;
        Ok(())
    }

    /// Wait out the frame's transmissions — for the outputs that need it.
    ///
    /// Two modes per channel, decided by [`Ws281xOutput::background_tx_safe`]:
    ///
    /// * **Barrier** (the default, and every output that has not proven
    ///   otherwise): the wire time is spent here, inside a quiet spin, not
    ///   under the engine's next tick. That placement is load-bearing when
    ///   the transmission's ISR shares the render core: the classic ESP32's
    ///   app path masks interrupts in stretches long enough to starve the
    ///   refill deadline (measured 2026-08-04 — wires transmitting while the
    ///   engine ran truncated ~99 % of their frames).
    /// * **Background-safe** (the classic with its refill ISR on the
    ///   dedicated APP core): the wait is skipped and the wire time overlaps
    ///   the next render. The frame-lifetime guarantee does not move an inch
    ///   — `write`'s wait-before-stage above waits the previous frame out
    ///   before anything touches the channel's storage, and that wait is
    ///   mandatory in both modes. The hang detector also rides that deferred
    ///   wait: `FRAME_TIMEOUT` (50 ms) runs from `start`, and by the next
    ///   write a ~33 ms frame interval plus ≤ ~31 ms of worst-case wire time
    ///   can exceed it only for frames far past dome scale — worth
    ///   re-checking if `WS281X_MAX_LEDS_PER_CHANNEL` ever grows.
    ///
    /// The engine calls this once per frame regardless of modes — the
    /// barrier-call-count test and the wrapper-forwarding guard from the
    /// concurrent-flush ADR stay exactly as meaningful. Every failure is
    /// logged with its handle; the first is returned.
    fn flush(&self) -> Result<(), OutputError> {
        let mut first_error: Option<OutputError> = None;
        let mut channels = self.channels.borrow_mut();
        for (handle_id, channel) in channels.iter_mut() {
            if channel.output.background_tx_safe() {
                continue;
            }
            if let Err(error) = channel.output.wait_complete() {
                log::warn!("Esp32OutputProvider::flush: handle={handle_id}: {error}");
                first_error.get_or_insert(error);
            }
        }

        // The deassert decision belongs at the frame barrier, after the last
        // write: it is the one point where every wire of the frame is known.
        //
        // Its wires are drained unconditionally, background-safe or not — the
        // barrier above deliberately leaves those transmitting, and cutting
        // the supply out from under a transmission is the failure this whole
        // sequence exists to avoid. The drain costs nothing in the steady
        // state because `expired` is false until the debounce runs out, and
        // false again immediately after: one deassert per off-transition.
        if let Some(gates) = &self.power_gates {
            let mut gates = gates.borrow_mut();
            let expired = gates.expired();
            if expired != 0 {
                for (handle_id, channel) in channels.iter_mut() {
                    if channel.gate_mask & expired == 0 {
                        continue;
                    }
                    if let Err(error) = channel.output.wait_complete() {
                        log::warn!(
                            "Esp32OutputProvider::flush: handle={handle_id} did not drain before \
                             power-gate deassert: {error}"
                        );
                        first_error.get_or_insert(error);
                    }
                }
                gates.deassert(expired);
            }
        }

        match first_error {
            None => Ok(()),
            Some(error) => Err(error),
        }
    }

    fn hardware_generation(&self) -> u64 {
        self.hardware_system.registry().generation()
    }
}

fn capped_byte_count_for_len(data_len: usize) -> (u32, bool) {
    ws281x_capped_byte_count(((data_len / 3) * 3) as u32)
}

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    //! Defense-in-depth backstop, tested directly: the engine seam
    //! (`lpc-engine`'s `wire_slice`) is what guarantees parity across
    //! providers now (P3), but `Esp32OutputProvider` keeps its own
    //! `WS281X_MAX_LEDS_PER_CHANNEL` check on top, and that check gets its
    //! own coverage here, at the raised 1024-lamp bound.

    use alloc::rc::Rc;
    use alloc::vec;

    use lpc_hardware::{
        HardwareSystem, HwManifest, HwRegistry, OutputError, WS281X_MAX_LEDS_PER_CHANNEL,
    };
    use lpc_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};

    use super::Esp32OutputProvider;

    fn provider() -> Esp32OutputProvider {
        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        Esp32OutputProvider::new(Rc::new(HardwareSystem::with_virtual_drivers(registry)))
    }

    fn open_ws281x(provider: &Esp32OutputProvider, byte_count: u32) -> OutputChannelHandle {
        provider
            .open(
                &lpc_hardware::HwEndpointSpec::from_static("ws281x:local:D10"),
                byte_count,
                OutputFormat::Ws2811,
                None,
            )
            .expect("ws281x:local:D10 opens on the virtual single-RMT board")
    }

    /// A channel authored for 1500 lamps must grant exactly the shared cap —
    /// `WS281X_MAX_LEDS_PER_CHANNEL * 3` bytes, 1024 lamps now that P3 raised
    /// it from 256 — never the full request. Proven by capacity, not by
    /// reaching into the opaque `Box<dyn Ws281xOutput>`: a write one lamp
    /// short of the granted size must still be rejected as a length
    /// mismatch, and a write of exactly the granted size must succeed.
    #[test]
    fn open_caps_a_request_above_the_shared_bound_to_exactly_1024_lamps() {
        let provider = provider();
        let cap_bytes = (WS281X_MAX_LEDS_PER_CHANNEL * 3) as u32;
        let handle = open_ws281x(&provider, 1500 * 3);

        let short = vec![0u16; (cap_bytes - 3) as usize];
        assert!(
            matches!(
                provider.write(handle, &short),
                Err(OutputError::DataLengthMismatch { expected, .. }) if expected == cap_bytes
            ),
            "the channel must be sized to exactly the capped byte count, not the requested one"
        );

        let exact = vec![0u16; cap_bytes as usize];
        provider
            .write(handle, &exact)
            .expect("a write of exactly the capped size succeeds");
    }

    /// The total-LED soft limit is SOFT: an open that pushes the board past
    /// its measured envelope warns (log-only) and must still succeed — a
    /// refusal would invert the measured-records convention.
    #[test]
    fn opening_past_the_soft_led_envelope_still_succeeds() {
        use lpc_hardware::{HwMeasuredLimit, HwSoftLimits};
        let manifest = HwManifest::virtual_single_rmt_gpio_board().with_soft_limits(HwSoftLimits {
            total_leds: Some(HwMeasuredLimit {
                value: 10,
                measured: "test envelope".into(),
            }),
        });
        let registry = Rc::new(HwRegistry::new(manifest));
        let provider =
            Esp32OutputProvider::new(Rc::new(HardwareSystem::with_virtual_drivers(registry)));

        // 100 lamps against an envelope of 10: warns, proceeds.
        let handle = provider
            .open(
                &lpc_hardware::HwEndpointSpec::from_static("ws281x:local:D10"),
                100 * 3,
                OutputFormat::Ws2811,
                None,
            )
            .expect("a soft limit must never refuse an open");
        let frame = vec![0u16; 100 * 3];
        provider.write(handle, &frame).expect("and writes proceed");
    }

    /// 375 and 1024 lamps both sit at or under the cap: `open` must grant
    /// exactly what was asked, with no truncation.
    #[test]
    fn open_grants_the_full_request_at_or_under_the_shared_bound() {
        for lamps in [375u32, WS281X_MAX_LEDS_PER_CHANNEL as u32] {
            let provider = provider();
            let byte_count = lamps * 3;
            let handle = open_ws281x(&provider, byte_count);

            let exact = vec![0u16; byte_count as usize];
            provider.write(handle, &exact).unwrap_or_else(|error| {
                panic!("{lamps}-lamp channel should not be capped: {error}")
            });
        }
    }

    /// The write-time grow path caps identically to the open-time path: a
    /// channel that opened small and then receives a larger frame is capped
    /// to the same 1024-lamp bound, not the frame's own size.
    #[test]
    fn write_caps_a_grown_frame_above_the_shared_bound_to_exactly_1024_lamps() {
        let provider = provider();
        let cap_bytes = (WS281X_MAX_LEDS_PER_CHANNEL * 3) as u32;
        let handle = open_ws281x(&provider, 3);

        let grown = vec![0u16; (1500 * 3) as usize];
        provider
            .write(handle, &grown)
            .expect("a grown frame is capped, not rejected");

        let short = vec![0u16; (cap_bytes - 3) as usize];
        assert!(
            matches!(
                provider.write(handle, &short),
                Err(OutputError::DataLengthMismatch { expected, .. }) if expected == cap_bytes
            ),
            "the channel must have grown to exactly the capped byte count, not the frame's own size"
        );
    }
}

#[cfg(test)]
mod concurrent_flush_tests {
    //! The deferred-wait transmission contract, proven against a probe output:
    //! `write` waits out the channel's *previous* frame before touching its
    //! storage, starts the new one, and returns without waiting for it — so
    //! handles written back to back transmit concurrently.

    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    use lpc_hardware::{
        HardwareEndpointError, HardwareSystem, HwAddress, HwDriver, HwEndpoint, HwEndpointId,
        HwEndpointKind, HwEndpointSpec, HwManifest, HwRegistry, OutputError, Ws281xConfig,
        Ws281xDriver, Ws281xOutput,
    };
    use lpc_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};

    use super::Esp32OutputProvider;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ProbeEvent {
        /// `wait_complete`, with whether this output still had a frame in
        /// flight when it was called.
        Wait { had_in_flight: bool },
        /// The non-blocking `start`.
        Start,
        /// The blocking `write` — the provider must never fall back to it.
        BlockingWrite,
    }

    #[derive(Default)]
    struct ProbeLog {
        events: Vec<ProbeEvent>,
        /// Outputs with a started, un-waited frame — across ALL probe outputs,
        /// which is what makes concurrency observable.
        in_flight: usize,
    }

    struct ProbeOutput {
        log: Rc<RefCell<ProbeLog>>,
        in_flight: bool,
        /// What this output claims via `background_tx_safe` — the provider's
        /// flush must skip waiting exactly the outputs that claim `true`.
        background_safe: bool,
    }

    impl Ws281xOutput for ProbeOutput {
        fn background_tx_safe(&self) -> bool {
            self.background_safe
        }

        fn write(&mut self, _data: &[u8]) -> Result<(), OutputError> {
            self.log.borrow_mut().events.push(ProbeEvent::BlockingWrite);
            Ok(())
        }

        fn resize(&mut self, _config: Ws281xConfig) -> Result<(), OutputError> {
            Ok(())
        }

        unsafe fn start(&mut self, _data: &[u8]) -> Result<(), OutputError> {
            assert!(
                !self.in_flight,
                "start while a frame is in flight violates the wait-first contract"
            );
            let mut log = self.log.borrow_mut();
            log.events.push(ProbeEvent::Start);
            log.in_flight += 1;
            self.in_flight = true;
            Ok(())
        }

        fn wait_complete(&mut self) -> Result<(), OutputError> {
            let mut log = self.log.borrow_mut();
            log.events.push(ProbeEvent::Wait {
                had_in_flight: self.in_flight,
            });
            if self.in_flight {
                log.in_flight -= 1;
                self.in_flight = false;
            }
            Ok(())
        }
    }

    /// Offers `ws281x:local:P0` / `ws281x:local:P1` and opens a [`ProbeOutput`]
    /// for either; claims nothing from the registry — leases are not what these
    /// tests are about.
    struct ProbeDriver {
        log: Rc<RefCell<ProbeLog>>,
        /// Bit `n` set = endpoint `Pn`'s outputs report `background_tx_safe`.
        background_safe: u32,
    }

    const PROBE_ENDPOINTS: usize = 2;

    impl HwDriver for ProbeDriver {
        fn driver_id(&self) -> &str {
            "probe-ws281x"
        }

        fn display_label(&self) -> &str {
            "Probe WS281x"
        }
    }

    impl Ws281xDriver for ProbeDriver {
        fn endpoints(&self) -> Vec<HwEndpoint> {
            (0..PROBE_ENDPOINTS)
                .map(|n| {
                    let spec = HwEndpointSpec::parse(alloc::format!("ws281x:local:P{n}"))
                        .expect("probe spec parses");
                    HwEndpoint::new(
                        HwEndpointId::for_driver_spec(self.driver_id(), &spec),
                        spec,
                        HwEndpointKind::Ws281x,
                        self.driver_id(),
                        HwAddress::gpio(10 + n as u32),
                        "probe",
                        lpc_hardware::HwEndpointStatus::Available,
                    )
                })
                .collect()
        }

        fn open(
            &self,
            endpoint_id: &HwEndpointId,
            _config: Ws281xConfig,
        ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
            let background_safe = (0..PROBE_ENDPOINTS).any(|n| {
                let spec = HwEndpointSpec::parse(alloc::format!("ws281x:local:P{n}"))
                    .expect("probe spec parses");
                HwEndpointId::for_driver_spec(self.driver_id(), &spec) == *endpoint_id
                    && self.background_safe & (1 << n) != 0
            });
            Ok(Box::new(ProbeOutput {
                log: Rc::clone(&self.log),
                in_flight: false,
                background_safe,
            }))
        }
    }

    fn probe_provider() -> (Esp32OutputProvider, Rc<RefCell<ProbeLog>>) {
        probe_provider_with_background(0)
    }

    /// [`probe_provider`] with bit `n` of `background_safe` making endpoint
    /// `Pn`'s outputs claim `background_tx_safe`.
    fn probe_provider_with_background(
        background_safe: u32,
    ) -> (Esp32OutputProvider, Rc<RefCell<ProbeLog>>) {
        let log = Rc::new(RefCell::new(ProbeLog::default()));
        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        let mut system = HardwareSystem::new(registry);
        system.add_ws281x_driver(Box::new(ProbeDriver {
            log: Rc::clone(&log),
            background_safe,
        }));
        (Esp32OutputProvider::new(Rc::new(system)), log)
    }

    fn open_probe(provider: &Esp32OutputProvider, n: usize) -> OutputChannelHandle {
        provider
            .open(
                &lpc_hardware::HwEndpointSpec::parse(alloc::format!("ws281x:local:P{n}"))
                    .expect("probe spec parses"),
                3,
                OutputFormat::Ws2811,
                None,
            )
            .expect("probe endpoint opens")
    }

    /// One handle, two writes: each write waits out the previous frame
    /// *before* starting the next — never after — and never falls back to the
    /// blocking write. The first wait finds no frame; the second finds the one
    /// the first write left in flight, which is the deferred wait working.
    #[test]
    fn write_waits_for_the_previous_frame_then_starts_without_blocking() {
        let (provider, log) = probe_provider();
        let handle = open_probe(&provider, 0);

        let frame = vec![0u16; 3];
        provider.write(handle, &frame).expect("first write");
        provider.write(handle, &frame).expect("second write");

        assert_eq!(
            log.borrow().events,
            vec![
                ProbeEvent::Wait {
                    had_in_flight: false
                },
                ProbeEvent::Start,
                ProbeEvent::Wait {
                    had_in_flight: true
                },
                ProbeEvent::Start,
            ],
        );
    }

    /// Two handles written back to back — the shape of one engine flush — are
    /// both still transmitting afterwards: `write` starts and returns rather
    /// than paying each wire time sequentially.
    #[test]
    fn back_to_back_writes_transmit_concurrently() {
        let (provider, log) = probe_provider();
        let first = open_probe(&provider, 0);
        let second = open_probe(&provider, 1);

        let frame = vec![0u16; 3];
        provider.write(first, &frame).expect("first wire");
        provider.write(second, &frame).expect("second wire");

        assert_eq!(
            log.borrow().in_flight,
            2,
            "both wires must be in flight at once; a blocking write would leave at most one"
        );
    }

    /// `flush` is the frame-wide barrier: after it, nothing is in flight, and
    /// every in-flight channel got a `wait_complete`.
    #[test]
    fn flush_waits_out_every_started_transmission() {
        let (provider, log) = probe_provider();
        let first = open_probe(&provider, 0);
        let second = open_probe(&provider, 1);

        let frame = vec![0u16; 3];
        provider.write(first, &frame).expect("first wire");
        provider.write(second, &frame).expect("second wire");
        assert_eq!(log.borrow().in_flight, 2);

        provider.flush().expect("flush");
        assert_eq!(log.borrow().in_flight, 0, "flush must drain every wire");
    }

    /// A background-safe output's transmission overlaps the next render:
    /// `flush` must not wait it out — and the frame-lifetime guarantee then
    /// rides on the *next* write's wait-before-stage, which must still find
    /// and drain the in-flight frame.
    #[test]
    fn flush_skips_background_safe_outputs_and_the_next_write_still_waits() {
        let (provider, log) = probe_provider_with_background(0b11);
        let first = open_probe(&provider, 0);
        let second = open_probe(&provider, 1);

        let frame = vec![0u16; 3];
        provider.write(first, &frame).expect("first wire");
        provider.write(second, &frame).expect("second wire");
        assert_eq!(log.borrow().in_flight, 2);

        let waits_before = wait_count(&log);
        provider.flush().expect("flush");
        assert_eq!(
            wait_count(&log),
            waits_before,
            "flush must not wait out a background-safe output"
        );
        assert_eq!(
            log.borrow().in_flight,
            2,
            "both wires keep transmitting across the barrier"
        );

        // The deferred wait is not optional: the next write on the handle
        // must drain the in-flight frame before staging over its bytes.
        provider.write(first, &frame).expect("next frame");
        assert!(
            log.borrow().events.iter().rev().any(|e| *e
                == ProbeEvent::Wait {
                    had_in_flight: true
                }),
            "wait-before-stage must find the overlapped frame in flight"
        );
    }

    /// Mixed wires — one barrier, one background-safe — wait exactly the
    /// barrier one at flush.
    #[test]
    fn flush_waits_only_the_barrier_output_in_a_mixed_frame() {
        let (provider, log) = probe_provider_with_background(0b10);
        let barrier = open_probe(&provider, 0);
        let background = open_probe(&provider, 1);

        let frame = vec![0u16; 3];
        provider.write(barrier, &frame).expect("barrier wire");
        provider.write(background, &frame).expect("background wire");
        assert_eq!(log.borrow().in_flight, 2);

        provider.flush().expect("flush");
        assert_eq!(
            log.borrow().in_flight,
            1,
            "the barrier wire is drained, the background wire keeps going"
        );
    }

    fn wait_count(log: &Rc<RefCell<ProbeLog>>) -> usize {
        log.borrow()
            .events
            .iter()
            .filter(|e| matches!(e, ProbeEvent::Wait { .. }))
            .count()
    }

    /// Closing a handle mid-flight must reach the output's own drop (which on
    /// hardware stops the transmitter) — the provider must not require a wait
    /// before close.
    #[test]
    fn close_drops_the_output_with_a_frame_still_in_flight() {
        let (provider, log) = probe_provider();
        let handle = open_probe(&provider, 0);

        let frame = vec![0u16; 3];
        provider.write(handle, &frame).expect("write");
        assert_eq!(log.borrow().in_flight, 1);
        provider.close(handle).expect("close");
    }
}

#[cfg(test)]
mod power_gate_tests {
    //! The switched-rail state machine, proven against a probe pin and a probe
    //! output sharing one event log and one hand-advanced clock, so both the
    //! *ordering* (energise → settle → transmit) and the *drain* (nothing on
    //! the wire when the rail drops) are observable facts rather than timing.
    //!
    //! Its own probes rather than `concurrent_flush_tests`': those exist to
    //! pin the transmission contract and must stay readable as exactly that.

    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::{Cell, RefCell};

    use lpc_hardware::{
        HardwareEndpointError, HardwareSystem, HwAddress, HwDriver, HwEndpoint, HwEndpointId,
        HwEndpointKind, HwEndpointSpec, HwGateLevel, HwManifest, HwPowerGate, HwRegistry,
        OutputError, Ws281xConfig, Ws281xDriver, Ws281xOutput,
    };
    use lpc_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};

    use super::Esp32OutputProvider;
    use crate::output::power_gate::{PowerGateController, PowerGatePin};

    const SETTLE_MS: u32 = 50;
    const DEBOUNCE_MS: u32 = 1_000;
    /// What one read of the probe clock costs. Non-zero so the settle spin —
    /// which is a `while now() < deadline` loop — always terminates without a
    /// real sleep; small enough that its drift never reaches the debounce.
    const TICK_US: u64 = 100;

    /// Hand-advanced monotonic µs. `read` is what the state machine sees (and
    /// it ticks); `peek` is what the probes stamp events with, so stamping
    /// never moves the clock.
    #[derive(Default)]
    struct Clock {
        now_us: Cell<u64>,
    }

    impl Clock {
        fn read(&self) -> u64 {
            let now = self.now_us.get();
            self.now_us.set(now + TICK_US);
            now
        }

        fn peek(&self) -> u64 {
            self.now_us.get()
        }

        fn advance_ms(&self, ms: u64) {
            self.now_us.set(self.now_us.get() + ms * 1_000);
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Event {
        /// The gate pin driven to a physical level, stamped with the clock and
        /// with how many wires were transmitting at that instant — which is
        /// what makes "never cut power with a frame in flight" checkable.
        Level {
            high: bool,
            at_us: u64,
            in_flight: usize,
        },
        Start {
            at_us: u64,
        },
    }

    #[derive(Default)]
    struct Log {
        events: Vec<Event>,
        in_flight: usize,
    }

    impl Log {
        fn levels(&self) -> Vec<(bool, u64, usize)> {
            self.events
                .iter()
                .filter_map(|event| match event {
                    Event::Level {
                        high,
                        at_us,
                        in_flight,
                    } => Some((*high, *at_us, *in_flight)),
                    Event::Start { .. } => None,
                })
                .collect()
        }
    }

    struct ProbePin {
        log: Rc<RefCell<Log>>,
        clock: Rc<Clock>,
    }

    impl PowerGatePin for ProbePin {
        fn set_level(&mut self, high: bool) {
            let at_us = self.clock.peek();
            let mut log = self.log.borrow_mut();
            let in_flight = log.in_flight;
            log.events.push(Event::Level {
                high,
                at_us,
                in_flight,
            });
        }
    }

    struct ProbeOutput {
        log: Rc<RefCell<Log>>,
        clock: Rc<Clock>,
        in_flight: bool,
        background_safe: bool,
    }

    impl Ws281xOutput for ProbeOutput {
        fn background_tx_safe(&self) -> bool {
            self.background_safe
        }

        fn write(&mut self, _data: &[u8]) -> Result<(), OutputError> {
            Ok(())
        }

        fn resize(&mut self, _config: Ws281xConfig) -> Result<(), OutputError> {
            Ok(())
        }

        unsafe fn start(&mut self, _data: &[u8]) -> Result<(), OutputError> {
            let at_us = self.clock.peek();
            let mut log = self.log.borrow_mut();
            log.events.push(Event::Start { at_us });
            log.in_flight += 1;
            self.in_flight = true;
            Ok(())
        }

        fn wait_complete(&mut self) -> Result<(), OutputError> {
            if self.in_flight {
                self.log.borrow_mut().in_flight -= 1;
                self.in_flight = false;
            }
            Ok(())
        }
    }

    /// Two endpoints, `ws281x:local:P0` at `/gpio/10` and `P1` at `/gpio/11` —
    /// the addresses a gate's `feeds` names.
    struct ProbeDriver {
        log: Rc<RefCell<Log>>,
        clock: Rc<Clock>,
        background_safe: bool,
    }

    const PROBE_ENDPOINTS: usize = 2;

    fn probe_spec(n: usize) -> HwEndpointSpec {
        HwEndpointSpec::parse(alloc::format!("ws281x:local:P{n}")).expect("probe spec parses")
    }

    impl HwDriver for ProbeDriver {
        fn driver_id(&self) -> &str {
            "probe-ws281x"
        }

        fn display_label(&self) -> &str {
            "Probe WS281x"
        }
    }

    impl Ws281xDriver for ProbeDriver {
        fn endpoints(&self) -> Vec<HwEndpoint> {
            (0..PROBE_ENDPOINTS)
                .map(|n| {
                    let spec = probe_spec(n);
                    HwEndpoint::new(
                        HwEndpointId::for_driver_spec(self.driver_id(), &spec),
                        spec,
                        HwEndpointKind::Ws281x,
                        self.driver_id(),
                        HwAddress::gpio(10 + n as u32),
                        "probe",
                        lpc_hardware::HwEndpointStatus::Available,
                    )
                })
                .collect()
        }

        fn open(
            &self,
            _endpoint_id: &HwEndpointId,
            _config: Ws281xConfig,
        ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
            Ok(Box::new(ProbeOutput {
                log: Rc::clone(&self.log),
                clock: Rc::clone(&self.clock),
                in_flight: false,
                background_safe: self.background_safe,
            }))
        }
    }

    struct Harness {
        provider: Esp32OutputProvider,
        log: Rc<RefCell<Log>>,
        clock: Rc<Clock>,
    }

    fn harness(gate: HwPowerGate) -> Harness {
        harness_with_background(gate, false)
    }

    fn harness_with_background(gate: HwPowerGate, background_safe: bool) -> Harness {
        let log = Rc::new(RefCell::new(Log::default()));
        let clock = Rc::new(Clock::default());
        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        let mut system = HardwareSystem::new(registry);
        system.add_ws281x_driver(Box::new(ProbeDriver {
            log: Rc::clone(&log),
            clock: Rc::clone(&clock),
            background_safe,
        }));

        let pin = Box::new(ProbePin {
            log: Rc::clone(&log),
            clock: Rc::clone(&clock),
        }) as Box<dyn PowerGatePin>;
        let ticker = Rc::clone(&clock);
        let gates = PowerGateController::new(Box::new(move || ticker.read()), [(gate, pin)]);

        Harness {
            provider: Esp32OutputProvider::new(Rc::new(system)).with_power_gates(gates),
            log,
            clock,
        }
    }

    fn active_high_gate() -> HwPowerGate {
        HwPowerGate::new(HwAddress::gpio(12), HwGateLevel::High, SETTLE_MS)
            .with_off_debounce_ms(DEBOUNCE_MS)
    }

    fn open_probe(provider: &Esp32OutputProvider, n: usize) -> OutputChannelHandle {
        provider
            .open(&probe_spec(n), 3, OutputFormat::Ws2811, None)
            .expect("probe endpoint opens")
    }

    fn lit() -> Vec<u16> {
        vec![0, 0, 1]
    }

    fn black() -> Vec<u16> {
        vec![0u16; 3]
    }

    /// Construction alone must leave the rail down, before any frame and
    /// before anything else can idle the pad: on the dig2go the gate pin is
    /// MTDI, the flash-voltage strap, and high is the state that stops the
    /// board booting.
    #[test]
    fn construction_drives_the_rail_off_before_any_frame() {
        let harness = harness(active_high_gate());
        assert_eq!(harness.log.borrow().levels(), vec![(false, 0, 0)]);
        // The construction stamp is the clock's origin: nothing may read the
        // clock before the pin is parked.
    }

    /// The assert edge: pin high, then the full settle window, and only then
    /// the wire starts. Clocking data into an unpowered strip phantom-powers
    /// the first controller through its protection diode, so the gap is the
    /// point of the test — not the ordering alone, which a single-threaded
    /// loop would give for free.
    #[test]
    fn a_lit_frame_energises_and_settles_before_the_wire_starts() {
        let harness = harness(active_high_gate());
        let handle = open_probe(&harness.provider, 0);

        harness.provider.write(handle, &lit()).expect("lit frame");

        let log = harness.log.borrow();
        let asserted_at = match log.levels().as_slice() {
            [(false, _, _), (true, at_us, _)] => *at_us,
            other => panic!("expected off-then-on, got {other:?}"),
        };
        let started_at = log
            .events
            .iter()
            .find_map(|event| match event {
                Event::Start { at_us } => Some(*at_us),
                Event::Level { .. } => None,
            })
            .expect("the frame must reach the wire");
        assert!(
            started_at - asserted_at >= u64::from(SETTLE_MS) * 1_000,
            "the wire started {}µs after the assert, short of the {SETTLE_MS}ms settle",
            started_at - asserted_at
        );
    }

    /// A rail already up does not re-settle: the steady state must not spend a
    /// settle window per frame.
    #[test]
    fn a_second_lit_frame_neither_re_asserts_nor_re_settles() {
        let harness = harness(active_high_gate());
        let handle = open_probe(&harness.provider, 0);

        harness.provider.write(handle, &lit()).expect("first");
        harness.provider.flush().expect("flush");
        let before = harness.clock.peek();
        harness.provider.write(handle, &lit()).expect("second");
        let elapsed = harness.clock.peek() - before;

        let levels: Vec<bool> = harness
            .log
            .borrow()
            .levels()
            .into_iter()
            .map(|(high, _, _)| high)
            .collect();
        assert_eq!(
            levels,
            vec![false, true],
            "the pin must be driven exactly twice: off at construction, on at the first lit frame"
        );
        assert!(
            elapsed < u64::from(SETTLE_MS) * 1_000,
            "a lit frame on an already-energised rail waited {elapsed}µs"
        );
    }

    /// Black short of the debounce keeps the rail up — a shader's transient
    /// black is not an off.
    #[test]
    fn black_frames_short_of_the_debounce_keep_the_rail_up() {
        let harness = harness(active_high_gate());
        let handle = open_probe(&harness.provider, 0);

        harness.provider.write(handle, &lit()).expect("lit frame");
        harness.provider.flush().expect("flush");
        harness.clock.advance_ms(u64::from(DEBOUNCE_MS) - 1);
        harness.provider.write(handle, &black()).expect("black");
        harness.provider.flush().expect("flush");

        assert_eq!(
            harness.log.borrow().levels().len(),
            2,
            "the rail must still be up one millisecond short of the debounce"
        );
    }

    /// Past the debounce the rail drops exactly once, and coming back up is
    /// one assert — no chatter across a black/lit alternation.
    #[test]
    fn the_rail_drops_once_past_the_debounce_and_does_not_chatter() {
        let harness = harness(active_high_gate());
        let handle = open_probe(&harness.provider, 0);

        harness.provider.write(handle, &lit()).expect("lit frame");
        harness.provider.flush().expect("flush");
        harness.clock.advance_ms(u64::from(DEBOUNCE_MS));

        // Several black frames past the debounce: the first drops the rail,
        // the rest find it already down.
        for _ in 0..3 {
            harness.provider.write(handle, &black()).expect("black");
            harness.provider.flush().expect("flush");
        }
        // ...and back to lit, which re-energises exactly once.
        for _ in 0..3 {
            harness.provider.write(handle, &lit()).expect("lit");
            harness.provider.flush().expect("flush");
        }

        let levels: Vec<bool> = harness
            .log
            .borrow()
            .levels()
            .into_iter()
            .map(|(high, _, _)| high)
            .collect();
        assert_eq!(levels, vec![false, true, false, true]);
    }

    /// Any lit frame restarts the debounce, so a strobe never drops the rail.
    #[test]
    fn a_lit_frame_resets_the_debounce() {
        let harness = harness(active_high_gate());
        let handle = open_probe(&harness.provider, 0);

        harness.provider.write(handle, &lit()).expect("lit frame");
        harness.provider.flush().expect("flush");

        for _ in 0..4 {
            harness.clock.advance_ms(u64::from(DEBOUNCE_MS) * 3 / 4);
            harness.provider.write(handle, &black()).expect("black");
            harness.provider.flush().expect("flush");
            harness.provider.write(handle, &lit()).expect("lit");
            harness.provider.flush().expect("flush");
        }

        assert_eq!(
            harness.log.borrow().levels().len(),
            2,
            "each lit frame must push the debounce out; the rail never dropped"
        );
    }

    /// The rail may only drop with the wires drained. A background-safe output
    /// is deliberately left transmitting by the flush barrier, so this is the
    /// case where the deassert path has to do the draining itself.
    #[test]
    fn the_rail_never_drops_with_a_frame_in_flight() {
        let harness = harness_with_background(active_high_gate(), true);
        let handle = open_probe(&harness.provider, 0);

        harness.provider.write(handle, &lit()).expect("lit frame");
        harness.provider.flush().expect("flush");
        assert_eq!(
            harness.log.borrow().in_flight,
            1,
            "a background-safe wire keeps transmitting across the barrier"
        );

        harness.clock.advance_ms(u64::from(DEBOUNCE_MS));
        harness.provider.write(handle, &black()).expect("black");
        harness.provider.flush().expect("flush");

        let levels = harness.log.borrow().levels();
        let (high, _, in_flight) = *levels.last().expect("a deassert");
        assert!(!high, "the rail must be down");
        assert_eq!(
            in_flight, 0,
            "the rail dropped while a wire was transmitting"
        );
    }

    /// Polarity is metadata: an active-low gate asserts by driving the pin
    /// low and idles high.
    #[test]
    fn an_active_low_gate_inverts_both_edges() {
        let gate = HwPowerGate::new(HwAddress::gpio(12), HwGateLevel::Low, SETTLE_MS)
            .with_off_debounce_ms(DEBOUNCE_MS);
        let harness = harness(gate);
        let handle = open_probe(&harness.provider, 0);

        harness.provider.write(handle, &lit()).expect("lit frame");
        harness.provider.flush().expect("flush");
        harness.clock.advance_ms(u64::from(DEBOUNCE_MS));
        harness.provider.write(handle, &black()).expect("black");
        harness.provider.flush().expect("flush");

        let levels: Vec<bool> = harness
            .log
            .borrow()
            .levels()
            .into_iter()
            .map(|(high, _, _)| high)
            .collect();
        assert_eq!(levels, vec![true, false, true]);
    }

    /// `feeds` scopes a gate to the wires it names: a channel it does not feed
    /// neither energises the rail nor holds it up.
    #[test]
    fn a_channel_the_gate_does_not_feed_never_touches_the_rail() {
        let gate = HwPowerGate::new(HwAddress::gpio(12), HwGateLevel::High, SETTLE_MS)
            .with_off_debounce_ms(DEBOUNCE_MS)
            .with_feeds([HwAddress::gpio(10)]);
        let harness = harness(gate);
        let fed = open_probe(&harness.provider, 0);
        let unfed = open_probe(&harness.provider, 1);

        harness.provider.write(unfed, &lit()).expect("unfed lit");
        harness.provider.flush().expect("flush");
        assert_eq!(
            harness.log.borrow().levels().len(),
            1,
            "an unfed channel must not energise the rail"
        );

        harness.provider.write(fed, &lit()).expect("fed lit");
        harness.provider.flush().expect("flush");
        harness.clock.advance_ms(u64::from(DEBOUNCE_MS));

        // The unfed channel stays lit throughout; only the fed one going black
        // decides the rail.
        harness.provider.write(unfed, &lit()).expect("unfed lit");
        harness.provider.write(fed, &black()).expect("fed black");
        harness.provider.flush().expect("flush");

        let levels: Vec<bool> = harness
            .log
            .borrow()
            .levels()
            .into_iter()
            .map(|(high, _, _)| high)
            .collect();
        assert_eq!(levels, vec![false, true, false]);
    }
}
