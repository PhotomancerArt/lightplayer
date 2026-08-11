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
    HardwareEndpointError, HardwareSystem, HwEndpointSpec, WS281X_MAX_LEDS_PER_PORT, Ws281xConfig,
    Ws281xOutput, ws281x_capped_byte_count,
};
use lpc_shared::DisplayPipeline;
use lpc_shared::output::{OutputDriverOptions, OutputFormat, OutputPortHandle, OutputProvider};
const FRAME_INTERVAL_US: u64 = 16_667;
const MID_FRAME_US: u64 = 8_333;

/// Ports reserved up front in [`Esp32OutputProvider::ports`].
///
/// A **reservation, not a limit** — `VecMap` still grows past it. It exists so
/// that opening the Nth port does not reallocate and memcpy the previous
/// N-1 `PortState`s while they are live, which is a transient peak of twice
/// the steady-state size on the one path that runs when the heap is already at
/// its high-water mark.
///
/// 8 is the widest RMT TX slot count in the family (the classic's eight,
/// before the block plan absorbs slots into wider windows). At today's
/// `PortState` size this reservation costs well under a kilobyte.
///
/// ⚠️ Historical note worth keeping: this used to matter enormously.
/// `PortState` carried a 3,084-byte inline white-point LUT, so the same
/// growth asked for 12,864 contiguous bytes and OOM'd the classic ESP32 with
/// 11,228 free — `docs/defects/2026-08-01-classic-rmt-open-fault.md`. The LUT
/// is gone; this guard remains because the growth pattern is what turned a
/// tight heap into a hard fault, and it will do so again for whatever the next
/// large per-port field is.
const RESERVED_PORTS: usize = 8;

struct PortState {
    /// ⚠️ Declared before `frame` on purpose: fields drop in declaration
    /// order, and the output's own `Drop` is what stops an in-flight
    /// transmission that may still be reading `frame`'s bytes. Swapping these
    /// two fields would free the frame under a running transmitter.
    output: Box<dyn Ws281xOutput>,
    byte_count: u32,
    pipeline: DisplayPipeline,
    /// The port's own rendered frame, alive between writes.
    ///
    /// It used to be a `Vec` allocated inside every `write`, which is fine
    /// while one handle is written at a time. It is not fine for a concurrent
    /// transmission, where every port's bytes must stay alive from its
    /// `start` until its `wait_complete` — so the storage belongs to the
    /// port, not to the call. Sized from the port's granted
    /// `byte_count`, so this is the same memory the per-write allocation held,
    /// just held for longer.
    frame: Vec<u8>,
}

/// ESP32 OutputProvider implementation.
pub struct Esp32OutputProvider {
    hardware_system: Rc<HardwareSystem>,
    ports: RefCell<VecMap<i32, PortState>>,
    next_handle: RefCell<i32>,
}

impl Esp32OutputProvider {
    pub fn new(hardware_system: Rc<HardwareSystem>) -> Self {
        Self {
            hardware_system,
            ports: RefCell::new(VecMap::with_capacity(RESERVED_PORTS)),
            next_handle: RefCell::new(1),
        }
    }
}

impl OutputProvider for Esp32OutputProvider {
    fn open(
        &self,
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputPortHandle, OutputError> {
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
                 {WS281X_MAX_LEDS_PER_PORT} LEDs; truncating to {byte_count} bytes"
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
        let handle = OutputPortHandle::new(handle_id);

        log::info!(
            "Esp32OutputProvider::open: Opened port handle={handle_id}, endpoint={endpoint}, byte_count={byte_count}"
        );

        let mut frame = Vec::new();
        frame.resize(((byte_count / 3) * 3) as usize, 0);

        self.ports.borrow_mut().insert(
            handle_id,
            PortState {
                output,
                byte_count,
                pipeline,
                frame,
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
                .ports
                .borrow()
                .iter()
                .map(|(_, port)| port.byte_count / 3)
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

    fn write(&self, handle: OutputPortHandle, data: &[u16]) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();
        log::debug!(
            "Esp32OutputProvider::write: handle={}, data_len={}",
            handle_id,
            data.len()
        );

        let mut ports = self.ports.borrow_mut();
        let port = ports.get_mut(&handle_id).ok_or_else(|| {
            log::warn!("Esp32OutputProvider::write: Invalid handle {handle_id}");
            OutputError::InvalidHandle { handle: handle_id }
        })?;

        // The previous frame may still be on the wire — `start` below returns
        // without waiting, and the transmitter keeps reading `port.frame`
        // until it finishes. Wait it out before anything below touches state
        // it may still be reading (the resize moves the frame's bytes; the
        // pipeline overwrites them). In the steady state a whole render has
        // passed since the frame started, so this returns without spinning;
        // it only actually waits when a frame hung, in which case it aborts
        // and reports rather than staging over a live transmission.
        port.output.wait_complete()?;

        let mut num_leds = (port.byte_count / 3) as usize;
        let expected_len = num_leds * 3;

        if data.len() > expected_len {
            let (new_byte_count, truncated) = capped_byte_count_for_len(data.len());
            // A frame past the cap keeps `data.len() > expected_len` true forever;
            // only act (and warn) when the granted size actually changes, so the
            // steady state neither re-resizes nor logs per frame.
            if new_byte_count != port.byte_count {
                if truncated {
                    log::warn!(
                        "Esp32OutputProvider::write: handle={handle_id} grew past \
                         {WS281X_MAX_LEDS_PER_PORT} LEDs; truncating to {new_byte_count} bytes"
                    );
                }
                port.output.resize(Ws281xConfig::new(new_byte_count))?;
                port.pipeline.resize(new_byte_count / 3);
                port.byte_count = new_byte_count;
                num_leds = (port.byte_count / 3) as usize;
            }
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        // The port owns its frame storage, so a resize is the only time
        // this allocates and a steady-state write allocates nothing at all.
        port.frame.resize(num_leds * 3, 0);

        port.pipeline.write_frame(0, data);
        port.pipeline.write_frame(FRAME_INTERVAL_US, data);
        let PortState {
            output,
            pipeline,
            frame,
            ..
        } = port;
        pipeline.tick(MID_FRAME_US, frame);

        // Start the transmission and return without waiting for it. Handles
        // written back to back in one flush therefore transmit concurrently —
        // the frame pays for its slowest wave of wires, not the sum of all of
        // them — and the wait happens in [`OutputProvider::flush`], the
        // frame-wide barrier the engine calls after the last write.
        //
        // SAFETY: `frame` is heap storage owned by this port's entry in
        // `self.ports`, so its bytes stay in place when the map grows or
        // the `PortState` moves. Every path that mutates or moves those
        // bytes — this function's resize and staging above — first calls
        // `wait_complete`, and dropping the entry drops `output` (which stops
        // the transmission) before `frame` (see the field-order note on
        // `PortState`). That is exactly the lifetime `start`'s contract
        // asks for.
        unsafe { output.start(frame) }
    }

    fn close(&self, handle: OutputPortHandle) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();
        self.ports
            .borrow_mut()
            .remove(&handle_id)
            .ok_or_else(|| OutputError::InvalidHandle { handle: handle_id })?;
        Ok(())
    }

    /// Wait out the frame's transmissions — for the outputs that need it.
    ///
    /// Two modes per port, decided by [`Ws281xOutput::background_tx_safe`]:
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
    ///   before anything touches the port's storage, and that wait is
    ///   mandatory in both modes. The hang detector also rides that deferred
    ///   wait: `FRAME_TIMEOUT` (50 ms) runs from `start`, and by the next
    ///   write a ~33 ms frame interval plus ≤ ~31 ms of worst-case wire time
    ///   can exceed it only for frames far past dome scale — worth
    ///   re-checking if `WS281X_MAX_LEDS_PER_PORT` ever grows.
    ///
    /// The engine calls this once per frame regardless of modes — the
    /// barrier-call-count test and the wrapper-forwarding guard from the
    /// concurrent-flush ADR stay exactly as meaningful. Every failure is
    /// logged with its handle; the first is returned.
    fn flush(&self) -> Result<(), OutputError> {
        let mut first_error: Option<OutputError> = None;
        for (handle_id, port) in self.ports.borrow_mut().iter_mut() {
            if port.output.background_tx_safe() {
                continue;
            }
            if let Err(error) = port.output.wait_complete() {
                log::warn!("Esp32OutputProvider::flush: handle={handle_id}: {error}");
                first_error.get_or_insert(error);
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
    //! `WS281X_MAX_LEDS_PER_PORT` check on top, and that check gets its
    //! own coverage here, at the raised 1024-lamp bound.

    use alloc::rc::Rc;
    use alloc::vec;

    use lpc_hardware::{
        HardwareSystem, HwManifest, HwRegistry, OutputError, WS281X_MAX_LEDS_PER_PORT,
    };
    use lpc_shared::output::{OutputFormat, OutputPortHandle, OutputProvider};

    use super::Esp32OutputProvider;

    fn provider() -> Esp32OutputProvider {
        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        Esp32OutputProvider::new(Rc::new(HardwareSystem::with_virtual_drivers(registry)))
    }

    fn open_ws281x(provider: &Esp32OutputProvider, byte_count: u32) -> OutputPortHandle {
        provider
            .open(
                &lpc_hardware::HwEndpointSpec::from_static("ws281x:local:D10"),
                byte_count,
                OutputFormat::Ws2811,
                None,
            )
            .expect("ws281x:local:D10 opens on the virtual single-RMT board")
    }

    /// A port authored for 1500 lamps must grant exactly the shared cap —
    /// `WS281X_MAX_LEDS_PER_PORT * 3` bytes, 1024 lamps now that P3 raised
    /// it from 256 — never the full request. Proven by capacity, not by
    /// reaching into the opaque `Box<dyn Ws281xOutput>`: a write one lamp
    /// short of the granted size must still be rejected as a length
    /// mismatch, and a write of exactly the granted size must succeed.
    #[test]
    fn open_caps_a_request_above_the_shared_bound_to_exactly_1024_lamps() {
        let provider = provider();
        let cap_bytes = (WS281X_MAX_LEDS_PER_PORT * 3) as u32;
        let handle = open_ws281x(&provider, 1500 * 3);

        let short = vec![0u16; (cap_bytes - 3) as usize];
        assert!(
            matches!(
                provider.write(handle, &short),
                Err(OutputError::DataLengthMismatch { expected, .. }) if expected == cap_bytes
            ),
            "the port must be sized to exactly the capped byte count, not the requested one"
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
        for lamps in [375u32, WS281X_MAX_LEDS_PER_PORT as u32] {
            let provider = provider();
            let byte_count = lamps * 3;
            let handle = open_ws281x(&provider, byte_count);

            let exact = vec![0u16; byte_count as usize];
            provider
                .write(handle, &exact)
                .unwrap_or_else(|error| panic!("{lamps}-lamp port should not be capped: {error}"));
        }
    }

    /// The write-time grow path caps identically to the open-time path: a
    /// port that opened small and then receives a larger frame is capped
    /// to the same 1024-lamp bound, not the frame's own size.
    #[test]
    fn write_caps_a_grown_frame_above_the_shared_bound_to_exactly_1024_lamps() {
        let provider = provider();
        let cap_bytes = (WS281X_MAX_LEDS_PER_PORT * 3) as u32;
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
            "the port must have grown to exactly the capped byte count, not the frame's own size"
        );
    }
}

#[cfg(test)]
mod concurrent_flush_tests {
    //! The deferred-wait transmission contract, proven against a probe output:
    //! `write` waits out the port's *previous* frame before touching its
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
    use lpc_shared::output::{OutputFormat, OutputPortHandle, OutputProvider};

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

    fn open_probe(provider: &Esp32OutputProvider, n: usize) -> OutputPortHandle {
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
    /// every in-flight port got a `wait_complete`.
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
