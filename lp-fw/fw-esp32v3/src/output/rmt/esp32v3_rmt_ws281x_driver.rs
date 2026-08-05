//! The registry-facing WS281x driver: four concurrent RMT outputs on one
//! classic ESP32.
//!
//! This is the seam between `lpc-hardware`'s endpoint/lease vocabulary and
//! [`lp_ws281x`]'s transmitter. It owns no sequencing and no register
//! knowledge — those are [`super::shared_driver::DRIVER`] and
//! [`super::v3_rmt`] respectively — only the mapping from *authored* endpoint
//! names to hardware.
//!
//! Structurally this is `fw-esp32s3`'s `Esp32S3RmtWs281xDriver` with one extra
//! indirection, described next.
//!
//! # Manifest channel → RMT slot: the absorbed-slot fix
//!
//! The block plan is computed **here, at init, from the manifest**: the
//! number of declared `/rmt/ws281xK` resources goes through
//! [`super::v3_rmt::plan_for_declared`] and is published once via
//! [`super::v3_rmt::TX_PLAN`] before any channel is configured. The
//! DOM-Z-102's four declared channels get two blocks each (slots 0, 2, 4, 6
//! own memory — the same plan the old compile-time constant produced);
//! fewer declared channels get wider windows, down to a single strip owning
//! a 256-word window (see the §12 reasoning in [`super::v3_rmt`]).
//!
//! A channel that takes extra blocks absorbs its neighbours' — so the
//! silicon-slot → manifest-index mapping is [`manifest_index_for_slot`] (the
//! plan's `index_of_slot`), and the channel-creation loop below *never
//! offers an absorbed slot to `configure_tx` in the first place*. That
//! is the difference from the experiment harness, whose
//! `BLOCKS_PER_CHANNEL=2` configuration failed at `configure_tx`: it kept
//! asking esp-hal for channels 0,1,2,3, and esp-hal correctly answered
//! `MemoryBlockNotAvailable` for 1 and 3 because channel 0's and channel 2's
//! two-block windows already owned their RAM.
//!
//! [`ChannelSlot`] consequently carries both identities: `timing` is the
//! manifest address (`/rmt/ws281x1`), `rmt_channel` is the silicon slot (2).
//! The registry only ever sees the former; [`super::v3_rmt`] only ever sees
//! the latter.
//!
//! # Why the pin is bound at `open`, not at construction
//!
//! An endpoint is a board label (`ws281x:local:IO18`), and which label a project
//! drives is authored data that arrives long after boot. So the RMT channel is
//! configured up front (its memory blocks and interrupt are chip resources,
//! not project ones) and its **pin** is connected when a project opens the
//! endpoint. `init_board` hands out no GPIO tokens, so the pad is recreated
//! with [`AnyPin::steal`] under the registry lease that has just granted
//! exclusive use of that address — see the SAFETY note at the call site.
//!
//! # Timing
//!
//! Every channel is opened WS2812-class (GRB, 300 µs latch). The strip's own
//! colour order is the fixture node's `color_order`, applied above this
//! boundary; the driver stays GRB exactly as the S3's and the C6's do.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use esp_hal::Blocking;
use esp_hal::gpio::{AnyPin, Level};
use esp_hal::rmt::{Channel, Rmt, Tx, TxChannelConfig, TxChannelCreator};
use esp_hal::time::Instant;
use lp_ws281x::{ChannelTiming, StartError};
use lpc_hardware::{
    HardwareEndpointError, HardwareLease, HwAddress, HwCapability, HwClaim, HwDriver, HwEndpoint,
    HwEndpointId, HwEndpointKind, HwEndpointSpec, HwEndpointStatus, HwRegistry, OutputError,
    Ws281xConfig, Ws281xDriver, Ws281xOutput,
};

#[cfg(feature = "frame-dump")]
use crate::output::rmt::frame_dump::{self, FrameDump};
use crate::output::rmt::shared_driver::{
    DRIVER, FRAME_TIMEOUT, install_isr, isr_on_app_core, report_telemetry_if_due,
};
use crate::output::rmt::v3_rmt::{self, BLOCK_WORDS, TX_CHANNELS, TX_PLAN};

const DRIVER_ID: &str = "esp32v3-rmt-ws281x";
const DISPLAY_LABEL: &str = "ESP32 RMT WS281x";

/// How many RMT channels this driver lets transmit **at the same time**.
///
/// The bound is interrupt-service margin, not an RMT limitation. With the
/// four-channel block plan every transmitter wants a refill each 80 µs
/// (12.5 k/s per channel), and one refill costs ≈15 word-times (≈18.75 µs,
/// measured; APB-write-bound, identical quiet or loaded). Two answers, keyed
/// on where the ISR runs this boot:
///
/// * **Dual-core (ISR on the dedicated APP core): 3** — ≈70 % worst-case ISR
///   duty, and the measured clean maximum with transmission overlapping
///   render (DOM-Z-102, zook-dome-1500, 2026-08-04 late: zero trips, zero
///   skips, zero errors on every wire over 130 s; worst steady entry delay
///   35/64 words). **Not 4, and the reason is arithmetic, not tuning**: four
///   coincident 18.75 µs refills ≈ 75 µs against the 80 µs deadline — ≈94 %
///   duty with the trampoline and dispatch on top — and on silicon the two
///   last-serviced wires starved on essentially every frame (4,753/4,755
///   truncated) while the first two ran clean. M4's cap-4-clean measurement
///   was the same arithmetic surviving only because the whole system was
///   quiet-spinning. A fifth-plus wire waits in the admission spin; the
///   render-overlap win survives (18 → 23 fps at 1500), but the wait is why
///   4 declared wires do not reach the engine-bound ceiling (~31 fps) — the
///   pin-mux wave milestone (P7) is the planned path to that.
/// * **Single-core fallback: 2** — the M4-shipped cap, unchanged, proven
///   unregressed with a forced-fallback boot (18 fps / 53 ms / zero trips at
///   1500 — the merged-main baseline exactly).
///
/// ⚠️ In the single-core shape, any transmission this cap admits is only
/// safe while the CPU quietly spins: this chip's app path masks interrupts
/// in stretches long enough to blow the 80 µs refill deadline, so a wire
/// transmitting while the engine runs truncates on nearly every frame
/// (measured before the barrier existed). The provider's end-of-flush
/// barrier guarantees quiet coverage — and it keys off the same
/// [`Ws281xOutput::background_tx_safe`] answer, so cap and barrier switch
/// together. See `Esp32OutputProvider::flush`.
///
/// ⚠️ In the dual-core shape, the cap's safety additionally leans on the
/// `isr-in-ram` feature of lp-ws281x: with the service path in flash, the
/// APP core stalls behind the PRO core's cache misses on the shared SPI bus
/// and even cap 3 trips at boot-time flash traffic (measured: a single
/// service 112 words late during project load).
fn max_concurrent_tx() -> usize {
    if isr_on_app_core() { 3 } else { 2 }
}

/// Channels currently transmitting a frame, across the whole driver.
fn transmitting_channels() -> usize {
    (0..TX_CHANNELS as u8)
        .filter(|&ch| DRIVER.channel(ch).is_some_and(|state| state.is_busy()))
        .count()
}

/// One RMT TX channel this driver can hand out.
struct ChannelSlot {
    /// The manifest resource backing this channel (`/rmt/ws281xK`), claimed
    /// alongside the GPIO for as long as an output holds it. `K` is the
    /// *manifest* index, which on this chip is not the RMT slot number — see
    /// the module docs.
    timing: HwAddress,
    /// The silicon RMT channel the manifest index resolved to (the plan's
    /// `nth_available(K)`). Everything below `lp_ws281x` speaks this number.
    rmt_channel: u8,
    /// The configured esp-hal channel. `Option` only because
    /// [`Channel::with_pin`] consumes and returns it; it is `Some` except
    /// inside that swap.
    tx: Option<Channel<'static, Blocking, Tx>>,
    /// Held by an open [`Esp32V3RmtWs281xOutput`].
    in_use: bool,
}

/// The channels, shared between the driver and every output it hands out.
///
/// Indexed by **manifest** channel `K`, not by RMT slot: a board that declares
/// only some `/rmt/ws281xK` resources leaves the others `None` rather than
/// shifting the numbering. Sized by [`TX_CHANNELS`] — the most channels any
/// plan can offer — with the plan deciding how many entries are ever filled.
type ChannelTable = Rc<RefCell<[Option<ChannelSlot>; TX_CHANNELS]>>;

/// WS281x driver over the classic-ESP32 RMT, one endpoint per board-labelled
/// GPIO and up to one live output per declared `/rmt/ws281xK` resource.
pub struct Esp32V3RmtWs281xDriver {
    registry: Rc<HwRegistry>,
    channels: ChannelTable,
}

impl Esp32V3RmtWs281xDriver {
    /// Compute and publish the block plan from the manifest's declared
    /// channel count, bind the RMT interrupt, and configure one TX channel
    /// per `/rmt/ws281xK` resource, on the slot the plan gives it.
    ///
    /// Channels are configured (memory blocks, divider, idle level) but not
    /// connected to any pin: that happens in [`Ws281xDriver::open`]. A resource
    /// the chip cannot back — an index past [`TX_CHANNELS`], or one whose
    /// slot the [`BlockPlan`](lp_ws281x::BlockPlan) absorbed — is skipped
    /// silently at the [`manifest_index_for_slot`] step rather than offered
    /// and then failing to open.
    pub fn new(registry: Rc<HwRegistry>, mut rmt: Rmt<'static, Blocking>) -> Self {
        // The plan, once, before the ISR and before any configure: windows
        // must never change after init (`RmtHw::ram_words` contract).
        let declared = declared_ws281x_channels(&registry);
        if let Some(plan) = v3_rmt::plan_for_declared(declared) {
            if let Err(error) = TX_PLAN.init(plan) {
                log::error!("Esp32V3RmtWs281xDriver: block plan already published: {error:?}");
            }
        }

        install_isr(&mut rmt);

        let config = TxChannelConfig::default()
            .with_clk_divider(1)
            .with_idle_output(true)
            .with_idle_output_level(Level::Low)
            .with_carrier_modulation(false);

        let mut slots: [Option<ChannelSlot>; TX_CHANNELS] = [const { None }; TX_CHANNELS];

        // The eight creators are distinct types (`ChannelCreator<_, _, K>`), so
        // they cannot be iterated; each is named once and consumed only when
        // the block plan says its slot owns memory AND the manifest declares
        // the timing resource that maps to it. An absorbed slot never reaches
        // `configure_tx` — that is the fix this port exists to make.
        macro_rules! adopt_slot {
            ($($slot:literal => $creator:ident),+ $(,)?) => {
                $(
                    if let Some(index) = manifest_index_for_slot($slot) {
                        if declares_timing_resource(&registry, index) {
                            slots[index] = adopt_channel(
                                index,
                                $slot,
                                rmt.$creator.configure_tx(
                                    &config.with_memsize(TX_PLAN.blocks($slot)),
                                ),
                            );
                        }
                    }
                )+
            };
        }
        adopt_slot!(
            0 => channel0,
            1 => channel1,
            2 => channel2,
            3 => channel3,
            4 => channel4,
            5 => channel5,
            6 => channel6,
            7 => channel7,
        );

        // The wrap bit is global on this chip and load-bearing (findings §11.5
        // — without it ping-pong refill does not work at all), so it is set
        // once here, after esp-hal has finished touching `APB_CONF` in
        // `configure_tx`.
        v3_rmt::init_tx();

        // One-shot refill-cost floor measurement, telemetry builds only —
        // prints [PROBE] lines before any output can open. See the module.
        #[cfg(feature = "ws281x_telemetry")]
        super::refill_floor_probe::run();

        log::info!(
            "Esp32V3RmtWs281xDriver: {} WS281x channels for {} declared (plan={:?} \
             ch0_window_words={} ch0_half_words={})",
            slots.iter().flatten().count(),
            declared,
            TX_PLAN.get().map(|plan| *plan.as_array()),
            TX_PLAN.window_words(0, BLOCK_WORDS),
            TX_PLAN.window_words(0, BLOCK_WORDS) / 2,
        );

        Self {
            registry,
            channels: Rc::new(RefCell::new(slots)),
        }
    }

    fn endpoint_id(&self, spec: &HwEndpointSpec) -> HwEndpointId {
        HwEndpointId::for_driver_spec(self.driver_id(), spec)
    }

    /// The lowest manifest channel that is configured, unclaimed, and whose
    /// timing resource the registry still reports as free.
    fn free_channel(&self) -> Option<usize> {
        let slots = self.channels.borrow();
        slots.iter().enumerate().find_map(|(index, slot)| {
            let slot = slot.as_ref()?;
            if slot.in_use
                || !self
                    .registry
                    .endpoint_status_for(&slot.timing)
                    .is_available()
            {
                return None;
            }
            Some(index)
        })
    }

    /// How many RMT channels this board offers at all — the denominator in the
    /// "all channels in use" message, and zero on a board whose manifest
    /// declares no WS281x timing resource.
    fn offered_channels(&self) -> usize {
        self.channels.borrow().iter().flatten().count()
    }

    fn endpoint_status(&self, gpio_address: &HwAddress) -> HwEndpointStatus {
        let gpio_status = self.registry.endpoint_status_for(gpio_address);
        if !gpio_status.is_available() {
            return gpio_status;
        }
        match self.free_channel() {
            Some(_) => HwEndpointStatus::Available,
            None => HwEndpointStatus::Unavailable {
                reason: format!(
                    "all {} RMT WS281x channels are in use",
                    self.offered_channels()
                ),
            },
        }
    }

    fn gpio_for_endpoint(
        &self,
        endpoint_id: &HwEndpointId,
    ) -> Result<HwAddress, HardwareEndpointError> {
        for endpoint in self.endpoints() {
            if endpoint.id() == endpoint_id {
                return Ok(endpoint.address().clone());
            }
        }

        Err(HardwareEndpointError::UnknownEndpoint {
            kind: HwEndpointKind::Ws281x,
            endpoint_id: endpoint_id.clone(),
        })
    }

    /// Connect `gpio` to manifest channel `index` and arm its wire timing.
    ///
    /// Called with the registry lease for both addresses already held. Returns
    /// the RMT slot the channel actually drives, which is what the output
    /// handle then talks to.
    fn bind_channel(&self, index: usize, gpio: u8) -> Result<u8, HardwareEndpointError> {
        let mut slots = self.channels.borrow_mut();
        let Some(slot) = slots[index].as_mut() else {
            return Err(HardwareEndpointError::Other {
                message: format!("RMT WS281x channel {index} is not configured"),
            });
        };
        let Some(tx) = slot.tx.take() else {
            return Err(HardwareEndpointError::Other {
                message: format!("RMT WS281x channel {index} has no transmitter"),
            });
        };
        let ch = slot.rmt_channel;

        // SAFETY: `init_board` drops the concrete HAL GPIO tokens after
        // startup, so the pad has to be recreated here. Exclusivity is the
        // registry's: the caller holds a lease on this GPIO address for the
        // lifetime of the output handle, and every path to a pin in this
        // firmware goes through such a lease, so no second token for this pad
        // can exist while this one does. `gpio` was checked against the chip's
        // pin list by `gpio_number`, so the panicking branch of `steal` is
        // unreachable.
        let pin = unsafe { AnyPin::steal(gpio) };
        slot.tx = Some(tx.with_pin(pin));

        // Leave the window all-STOP until the first frame prefills it, so a
        // spurious start can only transmit nothing.
        v3_rmt::clear_ram(ch);

        DRIVER
            .configure_default_clock(ch, &ChannelTiming::WS2812)
            .map_err(|error| HardwareEndpointError::Other {
                message: format!("RMT channel {ch} timing configuration failed: {error:?}"),
            })?;
        slot.in_use = true;
        Ok(ch)
    }
}

impl HwDriver for Esp32V3RmtWs281xDriver {
    fn driver_id(&self) -> &str {
        DRIVER_ID
    }

    fn display_label(&self) -> &str {
        DISPLAY_LABEL
    }
}

impl Ws281xDriver for Esp32V3RmtWs281xDriver {
    fn endpoints(&self) -> Vec<HwEndpoint> {
        if self.offered_channels() == 0 {
            return Vec::new();
        }

        let mut endpoints = Vec::new();
        for resource in self.registry.manifest().resources() {
            if !resource.supports(HwCapability::GpioOutput)
                || !has_board_assigned_label(resource.address(), resource.display_label())
            {
                continue;
            }
            let address = resource.address().clone();
            let spec = ws281x_local_spec(resource.display_label());
            endpoints.push(HwEndpoint::new(
                self.endpoint_id(&spec),
                spec,
                HwEndpointKind::Ws281x,
                self.driver_id(),
                address,
                resource.display_label(),
                self.endpoint_status(resource.address()),
            ));
        }
        endpoints
    }

    fn open(
        &self,
        endpoint_id: &HwEndpointId,
        config: Ws281xConfig,
    ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
        validate_byte_count(config.byte_count())?;
        let gpio_address = self.gpio_for_endpoint(endpoint_id)?;
        let gpio = gpio_number(&gpio_address)?;

        let endpoint = self
            .endpoints()
            .into_iter()
            .find(|endpoint| endpoint.id() == endpoint_id)
            .ok_or_else(|| HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Ws281x,
                endpoint_id: endpoint_id.clone(),
            })?;
        if !endpoint.is_available() {
            return Err(HardwareEndpointError::EndpointUnavailable {
                endpoint_id: endpoint_id.clone(),
                reason: endpoint
                    .status()
                    .unavailable_reason()
                    .unwrap_or("endpoint unavailable")
                    .into(),
            });
        }

        let index =
            self.free_channel()
                .ok_or_else(|| HardwareEndpointError::EndpointUnavailable {
                    endpoint_id: endpoint_id.clone(),
                    reason: format!(
                        "all {} RMT WS281x channels are in use",
                        self.offered_channels()
                    ),
                })?;
        let timing_address = HwAddress::rmt_ws281x(index as u8);

        self.registry
            .ensure_capability(&gpio_address, HwCapability::GpioOutput)?;
        self.registry
            .ensure_capability(&timing_address, HwCapability::Rmt)?;
        self.registry
            .ensure_capability(&timing_address, HwCapability::Ws281xOutput)?;
        let lease = self.registry.claim_bundle(HwClaim::new(
            self.driver_id(),
            vec![gpio_address.clone(), timing_address],
        ))?;

        let ch = match self.bind_channel(index, gpio) {
            Ok(ch) => ch,
            Err(error) => {
                let _ = self.registry.release(&lease);
                return Err(error);
            }
        };

        log::info!(
            "Esp32V3RmtWs281xDriver::open: endpoint={endpoint_id} gpio={} ws281x_ch={index} \
             rmt_slot={ch} bytes={}",
            gpio_address.as_str(),
            config.byte_count(),
        );
        #[cfg(feature = "frame-dump")]
        frame_dump::log_open(endpoint_id, config.byte_count());

        Ok(Box::new(Esp32V3RmtWs281xOutput {
            registry: Rc::clone(&self.registry),
            channels: Rc::clone(&self.channels),
            lease: Some(lease),
            index,
            channel: ch,
            byte_count: config.byte_count(),
            in_flight: None,
            #[cfg(feature = "frame-dump")]
            dump: FrameDump::new(),
        }))
    }
}

/// One opened WS281x output: an RMT channel, its registry lease, and the frame
/// size writes must match.
///
/// Never named outside this module — callers get a `Box<dyn Ws281xOutput>`.
struct Esp32V3RmtWs281xOutput {
    registry: Rc<HwRegistry>,
    channels: ChannelTable,
    lease: Option<HardwareLease>,
    /// Manifest channel index — the slot table's key.
    index: usize,
    /// RMT slot — what `lp_ws281x` and the register backend address.
    channel: u8,
    byte_count: u32,
    /// The frame begun by [`Ws281xOutput::start`] and not yet waited out by
    /// [`Ws281xOutput::wait_complete`]. `None` while the channel is idle.
    in_flight: Option<InFlightFrame>,
    /// Serial transcript of the frames this channel transmitted. Present only
    /// in a `frame-dump` build — see [`super::frame_dump`] for why the gate is
    /// compile-time rather than a runtime flag.
    #[cfg(feature = "frame-dump")]
    dump: FrameDump,
}

/// Bookkeeping for one started frame: when it began (the hang deadline is
/// measured from here) and, in a `frame-dump` build, where its bytes live.
struct InFlightFrame {
    started: Instant,
    /// The frame bytes, for the post-completion transcript. Dereferenceable
    /// for exactly as long as the [`Ws281xOutput::start`] contract holds the
    /// caller to — until `wait_complete` returns.
    #[cfg(feature = "frame-dump")]
    ptr: *const u8,
    #[cfg(feature = "frame-dump")]
    len: usize,
}

impl Ws281xOutput for Esp32V3RmtWs281xOutput {
    /// True exactly when the refill ISR is on the dedicated APP core this
    /// boot: refills then survive anything the render core does, so the
    /// provider may let this wire's transmission overlap the next render.
    /// In the single-core fallback this is `false` and the M4 barrier
    /// semantics apply unchanged — same flag as [`max_concurrent_tx`], so
    /// the cap and the barrier always switch together.
    fn background_tx_safe(&self) -> bool {
        isr_on_app_core()
    }

    fn write(&mut self, data: &[u8]) -> Result<(), OutputError> {
        // The borrow of `data` provably outlives the transmission: `start`'s
        // contract holds until `wait_complete` returns, and both happen inside
        // this call.
        unsafe { self.start(data) }?;
        self.wait_complete()
    }

    unsafe fn start(&mut self, data: &[u8]) -> Result<(), OutputError> {
        let expected_len = byte_len_for_byte_count(self.byte_count);
        if data.len() != expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        // Admission control: hold the start until fewer than
        // [`max_concurrent_tx`] other channels are transmitting. The bound is
        // this chip's per-core ISR budget, not a driver limitation — see the
        // function. The in-flight channels complete by interrupt with no help
        // from this thread, so the spin always ends; the deadline is pure
        // paranoia against a wedged sibling, and on expiry the start proceeds
        // (worst case is the over-subscription the cap exists to avoid, for
        // one frame, on a board that already has a hung channel to report).
        let admission_wait = Instant::now();
        let cap = max_concurrent_tx();
        while transmitting_channels() >= cap && admission_wait.elapsed() <= FRAME_TIMEOUT {}

        // SAFETY: forwarding this method's own contract — the caller keeps
        // `data` alive, in place, and unmodified until `wait_complete` returns
        // (or drops this output, whose `Drop` aborts the channel first).
        unsafe { DRIVER.start_frame(self.channel, data) }
            .map_err(|error| start_error_to_output_error(self.channel, error))?;

        self.in_flight = Some(InFlightFrame {
            started: Instant::now(),
            #[cfg(feature = "frame-dump")]
            ptr: data.as_ptr(),
            #[cfg(feature = "frame-dump")]
            len: data.len(),
        });
        Ok(())
    }

    fn wait_complete(&mut self) -> Result<(), OutputError> {
        let Some(in_flight) = self.in_flight.take() else {
            return Ok(());
        };

        // The hang detector: a frame that outlives its deadline is aborted
        // and reported rather than wedging the render loop forever. The
        // deadline runs from `start` — under the provider's flush barrier a
        // healthy frame is waited out here well inside it. The deadline check
        // reads a timer over APB and is throttled so the common case is a
        // pure SRAM-atomic spin.
        let mut timed_out = false;
        let mut iterations = 0u32;
        while !DRIVER.is_complete(self.channel) {
            iterations = iterations.wrapping_add(1);
            if iterations % 1024 == 0 && in_flight.started.elapsed() > FRAME_TIMEOUT {
                timed_out = true;
                if !DRIVER.abort(self.channel) {
                    // The teardown handshake could not confirm the ISR idle —
                    // reachable only if the ISR core wedged mid-service. The
                    // frame's bytes may still be referenced; shout, because
                    // this is a defect report, not a recoverable condition.
                    log::error!(
                        "RMT channel {}: abort handshake timed out — ISR core wedged mid-service?",
                        self.channel
                    );
                }
                break;
            }
        }

        // After the frame, never during it, and a no-op unless the
        // `ws281x_telemetry` feature is on.
        report_telemetry_if_due();

        if timed_out {
            return Err(OutputError::Other {
                message: format!(
                    "RMT channel {} frame did not complete within {} ms",
                    self.channel,
                    FRAME_TIMEOUT.as_millis(),
                ),
            });
        }

        // Reported after the send, not before it: the transcript is evidence
        // about bytes that reached the wire, and a frame that timed out or was
        // refused is not one of them.
        #[cfg(feature = "frame-dump")]
        {
            // SAFETY: `start`'s contract — the bytes stay alive, in place, and
            // unmodified until this very call returns.
            let data = unsafe { core::slice::from_raw_parts(in_flight.ptr, in_flight.len) };
            self.dump.on_write(data);
        }
        Ok(())
    }

    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError> {
        validate_byte_count(config.byte_count()).map_err(endpoint_error_to_output_error)?;
        self.byte_count = config.byte_count();
        #[cfg(feature = "frame-dump")]
        self.dump.on_resize(config.byte_count());
        Ok(())
    }
}

impl Drop for Esp32V3RmtWs281xOutput {
    /// Stop anything in flight, hand the RMT channel back to the free list, and
    /// release the lease — in that order, so the channel is never offered to a
    /// new open while its transmitter is still running.
    fn drop(&mut self) {
        if !DRIVER.abort(self.channel) {
            // See wait_complete's timeout path: an unconfirmed handshake means
            // the ISR core wedged mid-service and the frame bytes may still be
            // referenced when the channel storage is torn down below.
            log::error!(
                "RMT channel {}: abort handshake timed out in drop — ISR core wedged?",
                self.channel
            );
        }
        if let Some(slot) = self.channels.borrow_mut()[self.index].as_mut() {
            slot.in_use = false;
        }
        if let Some(lease) = self.lease.take() {
            if let Err(error) = self.registry.release(&lease) {
                log::warn!("Esp32V3RmtWs281xOutput: failed to release hardware lease: {error}");
            }
        }
    }
}

/// How many WS281x channels the manifest declares: the highest declared
/// `/rmt/ws281xK` index plus one, bounded by what the chip has.
///
/// Counted as a contiguous prefix would be, but tolerant of a gap: a manifest
/// declaring only `ws281x1` still yields 2, so the index keeps addressing the
/// declared resource rather than silently renumbering it.
fn declared_ws281x_channels(registry: &HwRegistry) -> usize {
    (0..TX_CHANNELS)
        .rev()
        .find(|&index| declares_timing_resource(registry, index))
        .map_or(0, |index| index + 1)
}

/// The manifest channel index an RMT slot backs, or `None` when the slot is
/// absorbed by a lower channel's window (or is past the usable range).
///
/// The guard that keeps the channel-creation macro from ever handing an
/// absorbed slot to `configure_tx`.
fn manifest_index_for_slot(slot: u8) -> Option<usize> {
    if slot as usize >= TX_CHANNELS {
        return None;
    }
    TX_PLAN.index_of_slot(slot)
}

/// Does the manifest declare `/rmt/ws281x<index>` with both WS281x
/// capabilities?
///
/// The block-plan half of the question is already answered by
/// [`manifest_index_for_slot`], which is why — unlike the S3's version of this
/// function — there is no `is_available` check here.
fn declares_timing_resource(registry: &HwRegistry, index: usize) -> bool {
    let address = HwAddress::rmt_ws281x(index as u8);
    registry
        .ensure_capability(&address, HwCapability::Rmt)
        .is_ok()
        && registry
            .ensure_capability(&address, HwCapability::Ws281xOutput)
            .is_ok()
}

/// Turn a freshly configured esp-hal channel into a slot, or log why not.
fn adopt_channel(
    index: usize,
    slot: usize,
    configured: Result<Channel<'static, Blocking, Tx>, esp_hal::rmt::ConfigError>,
) -> Option<ChannelSlot> {
    let ch = slot as u8;
    match configured {
        Ok(tx) => {
            v3_rmt::enable_tx_interrupts(ch);
            Some(ChannelSlot {
                timing: HwAddress::rmt_ws281x(index as u8),
                rmt_channel: ch,
                tx: Some(tx),
                in_use: false,
            })
        }
        Err(error) => {
            log::error!(
                "Esp32V3RmtWs281xDriver: RMT slot {slot} (ws281x{index}) configure_tx failed: \
                 {error:?}"
            );
            None
        }
    }
}

fn validate_byte_count(byte_count: u32) -> Result<(), HardwareEndpointError> {
    if byte_count < 3 {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: "WS281x byte_count must be at least 3".into(),
        });
    }
    Ok(())
}

fn byte_len_for_byte_count(byte_count: u32) -> usize {
    ((byte_count / 3) as usize) * 3
}

fn start_error_to_output_error(channel: u8, error: StartError) -> OutputError {
    match error {
        StartError::Busy => OutputError::Other {
            message: format!("RMT channel {channel} is still transmitting the previous frame"),
        },
        other => OutputError::InvalidConfig {
            reason: format!("RMT channel {channel} cannot start a frame: {other:?}"),
        },
    }
}

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}

fn gpio_number(address: &HwAddress) -> Result<u8, HardwareEndpointError> {
    let Some(raw) = address.as_str().strip_prefix("/gpio/") else {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("WS281x endpoint address is not a GPIO: {address}"),
        });
    };
    let gpio = raw
        .parse::<u8>()
        .map_err(|_| HardwareEndpointError::UnsupportedConfig {
            reason: format!("invalid classic-ESP32 GPIO address: {address}"),
        })?;
    if !gpio_exists(gpio) {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("classic ESP32 has no GPIO {gpio}"),
        });
    }
    if !gpio_can_output(gpio) {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("classic-ESP32 GPIO {gpio} is input-only and cannot drive WS281x data"),
        });
    }
    Ok(gpio)
}

/// Does this chip have GPIO `pin` at all?
///
/// The classic ESP32 numbers **GPIO0-5, GPIO12-19, GPIO21-23, GPIO25-27 and
/// GPIO32-39**: 6-11 are the SPI flash pins and are not bonded out on
/// WROOM-class modules, and 20, 24 and 28-31 do not exist at all. Taken from
/// `esp-metadata-generated`'s `for_each_gpio!` table for `esp32` — the same
/// table [`AnyPin::steal`] asserts against, and it *panics* on a number the
/// chip does not have. This is not a policy filter (reserving pins is the
/// manifest's job); a hand-written `/hardware.json` naming `/gpio/20` must
/// fail the open, not the boot.
fn gpio_exists(pin: u8) -> bool {
    matches!(pin, 0..=5 | 12..=19 | 21..=23 | 25..=27 | 32..=39)
}

/// Can this chip's GPIO `pin` drive an output?
///
/// **GPIO34-39 are input-only** on the classic ESP32 (no output driver in the
/// pad at all — `for_each_gpio!` lists an empty output-signal set for each).
/// The RMT would happily route its signal there and nothing would ever appear
/// on the wire, which is a much worse failure than a rejected open: it looks
/// like a driver bug on a board that is simply miswired. The S3 needs no
/// equivalent check — every S3 GPIO can output.
fn gpio_can_output(pin: u8) -> bool {
    !matches!(pin, 34..=39)
}

/// A resource whose display label is still `GPIO<n>` has no board label, and a
/// pin the board does not name is not one a project should be offered.
fn has_board_assigned_label(address: &HwAddress, display_label: &str) -> bool {
    let Some(raw) = address.as_str().strip_prefix("/gpio/") else {
        return false;
    };
    !display_label.eq_ignore_ascii_case(&format!("GPIO{raw}"))
}

fn ws281x_local_spec(config: &str) -> HwEndpointSpec {
    HwEndpointSpec::parse(format!("ws281x:local:{config}"))
        .expect("manifest display label should form a valid endpoint spec")
}
