//! The registry-facing WS281x driver: N wires time-sharing four pooled RMT
//! transmitters on one classic ESP32.
//!
//! This is the seam between `lpc-hardware`'s endpoint/lease vocabulary and
//! [`lp_ws281x`]'s transmitter. It owns the wire↔slot pooling and nothing
//! else — sequencing and register knowledge are
//! [`super::shared_driver::DRIVER`] and [`super::v3_rmt`] respectively.
//!
//! # Wires and slots are different things
//!
//! A **wire** is what the manifest declares (`/rmt/ws281xK` + a board-labelled
//! pad) and what a project opens: a GPIO, a frame buffer, a lease. A **slot**
//! is a silicon transmitter: one of at most
//! [`v3_rmt::POOLED_SLOT_CAP`] two-block RMT channels, each with the
//! measured-clean 80 µs refill geometry. Up to four wires transmit at once —
//! one slot each — and further wires wait for a slot and take it over by
//! rebinding the slot's output signal to their pad through the GPIO matrix
//! ([`v3_rmt::route_rmt_to_gpio`]). Five wires transmit as a wave of four
//! plus a wave of one; eight as two waves. The design target is eight wires
//! ("Fadecandy shape"); five (the Zook dome) is the first hardware-validated
//! instance — see `docs/future/2026-08-05-pinmux-8wire-validation.md` for
//! what remains unvalidated.
//!
//! The block plan is computed **here, at init, from the manifest**: declared
//! wire count → [`super::v3_rmt::plan_for_declared`] (which caps silicon
//! slots at four) → published once via [`super::v3_rmt::TX_PLAN`] before any
//! channel is configured. Slots 0/2/4/6 own memory; a channel that takes
//! extra blocks absorbs its neighbours', and an absorbed slot never reaches
//! `configure_tx` (the experiment harness's
//! `MemoryBlockNotAvailable` lesson).
//!
//! # Pads: parked low or transmitting, never floating after open
//!
//! An endpoint is a board label (`ws281x:local:IO18`); which pad a project
//! drives is authored data that arrives long after boot. Opening a wire parks
//! its pad — plain GPIO function, output enabled, solid low
//! ([`v3_rmt::park_gpio`]) — so the strand idles clean before its first
//! frame. Acquiring a slot routes the slot's RMT signal onto the pad; losing
//! the slot to another wire parks the pad again first. A parked strand holds
//! its latched frame indefinitely (WS281x latch on quiet-low), which is
//! exactly the right visual for a wire waiting its wave. `init_board` hands
//! out no GPIO tokens, so pads are recreated with `AnyPin::steal` under the
//! registry lease that grants exclusive use of the address — see the SAFETY
//! notes in `v3_rmt`.
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
use esp_hal::gpio::Level;
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
/// (12.5 k/s per channel). Two answers, keyed on where the ISR runs this
/// boot:
///
/// * **Dual-core (ISR on the dedicated APP core): 4** — every declared wire
///   transmits in one wave, fully overlapped with render. This was 3 until
///   the `fill_half` hoists (lp-ws281x, 2026-08-05) cut the measured
///   64-word service cost from ~11.2 µs to ~8.1 µs against a raw APB floor
///   of ~3.2 µs (`refill_floor_probe` — the refill was code-bound, not
///   bus-bound; the old ~18.75 µs figure priced four coincident refills at
///   ≈94 % of the deadline and starved the two last-serviced wires).
///   Re-measured after the hoists (DOM-Z-102, zook-dome-1500, 170 s /
///   5,327 frames): **zero trips, zero skips, zero errors on all four
///   wires, 31.3 fps** — the engine-bound ceiling, up from 23.8 at cap 3.
///   Worst steady entry delay 51/64 words (ch6, the last-serviced wire),
///   worst refill lag 17 words; the boot-time flash-stall service (112
///   words late, once, during project load) no longer trips the guard.
///   That 51-word worst entry is the number to watch if a fifth declared
///   wire ever admission-waits here — P7 pin-mux waves remains the path
///   past four.
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
/// and boot-time flash traffic alone blew the deadline (measured: a single
/// service 112 words late during project load).
fn max_concurrent_tx() -> usize {
    if isr_on_app_core() { 4 } else { 2 }
}

/// Channels currently transmitting a frame, across the whole driver.
fn transmitting_channels() -> usize {
    (0..TX_CHANNELS as u8)
        .filter(|&ch| DRIVER.channel(ch).is_some_and(|state| state.is_busy()))
        .count()
}

/// One wire-lease slot: a manifest `/rmt/ws281xK` resource an open output
/// holds (alongside its GPIO) for its lifetime.
struct WireSlot {
    /// The manifest resource backing this wire (`/rmt/ws281xK`).
    timing: HwAddress,
    /// Held by an open [`Esp32V3RmtWs281xOutput`].
    in_use: bool,
}

/// One pooled silicon transmitter.
struct PoolSlot {
    /// The RMT channel number — what `lp_ws281x` and the register backend
    /// speak.
    rmt_channel: u8,
    /// The configured esp-hal channel, held so esp-hal never tears the
    /// channel down. Never touched after init: pin binding is the GPIO
    /// matrix's, via [`v3_rmt::route_rmt_to_gpio`].
    _tx: Channel<'static, Blocking, Tx>,
    /// The pad currently carrying this slot's output signal.
    bound_gpio: Option<u8>,
    /// The wire (manifest index) that last transmitted on this slot.
    owner_wire: Option<usize>,
    /// Bumped on every ownership change. An output records the generation it
    /// started under; a mismatch at wait time proves the frame completed
    /// (slots are only ever taken over `is_complete`), which is what makes
    /// the deferred wait sound across a takeover.
    generation: u64,
}

/// The wire-lease table, shared between the driver and every output.
///
/// Indexed by **manifest** channel `K`; sized by [`TX_CHANNELS`], the most
/// wires any manifest can declare, with `None` for undeclared indices.
type ChannelTable = Rc<RefCell<[Option<WireSlot>; TX_CHANNELS]>>;

/// The transmitter pool, shared the same way. At most
/// [`v3_rmt::POOLED_SLOT_CAP`] entries. All pool state lives behind the
/// single-threaded executor's `RefCell` — the ISR never touches the pool;
/// slot↔frame sequencing below it is `lp_ws281x`'s atomics.
type SlotPool = Rc<RefCell<Vec<PoolSlot>>>;

/// WS281x driver over the classic-ESP32 RMT, one endpoint per board-labelled
/// GPIO and up to one live output per declared `/rmt/ws281xK` resource.
pub struct Esp32V3RmtWs281xDriver {
    registry: Rc<HwRegistry>,
    channels: ChannelTable,
    pool: SlotPool,
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

        // The wire-lease table: one entry per declared `/rmt/ws281xK`.
        let mut slots: [Option<WireSlot>; TX_CHANNELS] = [const { None }; TX_CHANNELS];
        for (index, slot) in slots.iter_mut().enumerate() {
            if index < declared && declares_timing_resource(&registry, index) {
                *slot = Some(WireSlot {
                    timing: HwAddress::rmt_ws281x(index as u8),
                    in_use: false,
                });
            }
        }

        // The transmitter pool: every slot the plan gave memory to, fully
        // configured up front — memory blocks, interrupts, timing, and an
        // all-STOP window so a spurious start can only transmit nothing. Pin
        // binding is per-transmission (`acquire_slot`), so no pad is touched
        // here.
        //
        // The eight creators are distinct types (`ChannelCreator<_, _, K>`),
        // so they cannot be iterated; each is named once and consumed only
        // when the block plan says its slot owns memory. An absorbed slot
        // never reaches `configure_tx` — the experiment harness's
        // `MemoryBlockNotAvailable` lesson.
        let mut pool: Vec<PoolSlot> = Vec::with_capacity(v3_rmt::POOLED_SLOT_CAP);
        macro_rules! adopt_slot {
            ($($slot:literal => $creator:ident),+ $(,)?) => {
                $(
                    if TX_PLAN.blocks($slot) > 0 {
                        if let Some(pool_slot) = adopt_channel(
                            $slot,
                            rmt.$creator.configure_tx(
                                &config.with_memsize(TX_PLAN.blocks($slot)),
                            ),
                        ) {
                            pool.push(pool_slot);
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
            "Esp32V3RmtWs281xDriver: {} wires over {} pooled slots (declared={} plan={:?} \
             slot0_window_words={} slot0_half_words={})",
            slots.iter().flatten().count(),
            pool.len(),
            declared,
            TX_PLAN.get().map(|plan| *plan.as_array()),
            TX_PLAN.window_words(0, BLOCK_WORDS),
            TX_PLAN.window_words(0, BLOCK_WORDS) / 2,
        );

        Self {
            registry,
            channels: Rc::new(RefCell::new(slots)),
            pool: Rc::new(RefCell::new(pool)),
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

    /// Mark wire `index` open and park its pad low.
    ///
    /// Called with the registry lease for both addresses already held. No
    /// transmitter is bound here — slots are acquired per transmission by
    /// [`acquire_slot`].
    fn bind_wire(&self, index: usize, gpio: u8) -> Result<(), HardwareEndpointError> {
        let mut slots = self.channels.borrow_mut();
        let Some(slot) = slots[index].as_mut() else {
            return Err(HardwareEndpointError::Other {
                message: format!("RMT WS281x wire {index} is not configured"),
            });
        };
        // Lease held by the caller: the pad is exclusively this wire's until
        // the output drops. Park it so the strand idles solid-low from open.
        v3_rmt::park_gpio(gpio);
        slot.in_use = true;
        Ok(())
    }
}

/// Acquire a pooled transmitter for `wire`/`gpio`, or `None` if every slot is
/// busy (or the concurrency cap is reached).
///
/// Preference order: the slot this wire already owns (no re-mux — the steady
/// state for the first four wires costs zero matrix writes), then an unowned
/// slot, then taking over any *completed* slot. A busy slot is never taken:
/// takeover happens only `is_complete`, which is what makes the generation
/// check in `wait_complete` sound.
///
/// The `max_concurrent_tx` guard keeps the single-core fallback at its
/// M4-proven cap even though four slots exist.
fn acquire_slot(pool: &SlotPool, wire: usize, gpio: u8) -> Option<(usize, u8, u64)> {
    if transmitting_channels() >= max_concurrent_tx() {
        return None;
    }
    let mut pool = pool.borrow_mut();
    let mine = pool
        .iter()
        .position(|s| s.owner_wire == Some(wire) && DRIVER.is_complete(s.rmt_channel));
    let pick = mine
        .or_else(|| {
            pool.iter()
                .position(|s| s.owner_wire.is_none() && DRIVER.is_complete(s.rmt_channel))
        })
        .or_else(|| pool.iter().position(|s| DRIVER.is_complete(s.rmt_channel)))?;
    let slot = &mut pool[pick];
    if slot.owner_wire != Some(wire) {
        // Takeover: park the displaced pad first (its strand then holds its
        // latched frame), then point the slot's signal at ours.
        if let Some(old) = slot.bound_gpio {
            if old != gpio {
                v3_rmt::park_gpio(old);
            }
        }
        v3_rmt::route_rmt_to_gpio(slot.rmt_channel, gpio);
        slot.bound_gpio = Some(gpio);
        slot.owner_wire = Some(wire);
        slot.generation = slot.generation.wrapping_add(1);
    }
    Some((pick, slot.rmt_channel, slot.generation))
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

        if let Err(error) = self.bind_wire(index, gpio) {
            let _ = self.registry.release(&lease);
            return Err(error);
        }

        log::info!(
            "Esp32V3RmtWs281xDriver::open: endpoint={endpoint_id} gpio={} wire={index} \
             bytes={} (slot per transmission)",
            gpio_address.as_str(),
            config.byte_count(),
        );
        #[cfg(feature = "frame-dump")]
        frame_dump::log_open(endpoint_id, config.byte_count());

        Ok(Box::new(Esp32V3RmtWs281xOutput {
            registry: Rc::clone(&self.registry),
            channels: Rc::clone(&self.channels),
            pool: Rc::clone(&self.pool),
            lease: Some(lease),
            index,
            gpio,
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
    pool: SlotPool,
    lease: Option<HardwareLease>,
    /// Manifest wire index — the wire table's key.
    index: usize,
    /// The pad this wire drives, leased for the output's lifetime.
    gpio: u8,
    byte_count: u32,
    /// The frame begun by [`Ws281xOutput::start`] and not yet waited out by
    /// [`Ws281xOutput::wait_complete`]. `None` while the wire is idle.
    in_flight: Option<InFlightFrame>,
    /// Serial transcript of the frames this wire transmitted. Present only
    /// in a `frame-dump` build — see [`super::frame_dump`] for why the gate is
    /// compile-time rather than a runtime flag.
    #[cfg(feature = "frame-dump")]
    dump: FrameDump,
}

/// Bookkeeping for one started frame: when it began (the hang deadline is
/// measured from here), which pooled slot carries it and under which
/// generation, and, in a `frame-dump` build, where its bytes live.
struct InFlightFrame {
    started: Instant,
    /// Index into the pool of the slot this frame transmits on.
    pool_idx: usize,
    /// The slot's RMT channel, cached so the wait path never re-borrows the
    /// pool just to poll completion.
    rmt_channel: u8,
    /// The pool generation the slot was acquired under. If it has moved on
    /// by wait time, the frame provably completed (takeover only happens
    /// `is_complete`).
    generation: u64,
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

        // Admission is slot acquisition: spin until a pooled transmitter is
        // free for this wire (its own slot completing, an unowned slot, or
        // taking over any completed slot — never a busy one). The in-flight
        // slots complete by interrupt with no help from this thread, so the
        // spin always ends; the deadline guards against a wedged sibling. On
        // expiry the frame is REFUSED rather than over-subscribed — taking a
        // busy slot would corrupt another wire's live transmission, which is
        // strictly worse than one wire dropping one frame on a board that
        // already has a hung slot to report.
        let admission_wait = Instant::now();
        let (pool_idx, rmt_channel, generation) = loop {
            if let Some(acquired) = acquire_slot(&self.pool, self.index, self.gpio) {
                break acquired;
            }
            if admission_wait.elapsed() > FRAME_TIMEOUT {
                return Err(OutputError::Other {
                    message: format!(
                        "wire {} (gpio {}): no pooled RMT slot freed within {} ms",
                        self.index,
                        self.gpio,
                        FRAME_TIMEOUT.as_millis(),
                    ),
                });
            }
        };

        // SAFETY: forwarding this method's own contract — the caller keeps
        // `data` alive, in place, and unmodified until `wait_complete` returns
        // (or drops this output, whose `Drop` aborts the slot first).
        unsafe { DRIVER.start_frame(rmt_channel, data) }
            .map_err(|error| start_error_to_output_error(rmt_channel, error))?;

        self.in_flight = Some(InFlightFrame {
            started: Instant::now(),
            pool_idx,
            rmt_channel,
            generation,
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

        // If the slot has been taken over since this frame started, the
        // frame provably completed — takeover only happens `is_complete` —
        // and polling the slot now would wait on someone ELSE's frame. The
        // generation is stable for the duration of this call: nothing else
        // runs on the single-threaded executor while we spin below.
        let superseded = self.pool.borrow()[in_flight.pool_idx].generation != in_flight.generation;

        // The hang detector: a frame that outlives its deadline is aborted
        // and reported rather than wedging the render loop forever. The
        // deadline runs from `start` — with background transmission it is
        // typically nearly elapsed by the time the next write waits here. The
        // deadline check reads a timer over APB and is throttled so the
        // common case is a pure SRAM-atomic spin.
        let mut timed_out = false;
        let mut iterations = 0u32;
        while !superseded && !DRIVER.is_complete(in_flight.rmt_channel) {
            iterations = iterations.wrapping_add(1);
            if iterations % 1024 == 0 && in_flight.started.elapsed() > FRAME_TIMEOUT {
                timed_out = true;
                if !DRIVER.abort(in_flight.rmt_channel) {
                    // The teardown handshake could not confirm the ISR idle —
                    // reachable only if the ISR core wedged mid-service. The
                    // frame's bytes may still be referenced; shout, because
                    // this is a defect report, not a recoverable condition.
                    log::error!(
                        "RMT slot {}: abort handshake timed out — ISR core wedged mid-service?",
                        in_flight.rmt_channel
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
                    "RMT slot {} frame did not complete within {} ms",
                    in_flight.rmt_channel,
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
    /// Stop anything in flight, disown and park, hand the wire back, and
    /// release the lease — in that order, so no slot is ever reachable from a
    /// new open while a transmitter this wire started is still running, and
    /// the pad never floats while the lease is alive.
    fn drop(&mut self) {
        {
            let mut pool = self.pool.borrow_mut();
            for slot in pool.iter_mut() {
                if slot.owner_wire == Some(self.index) {
                    // A frame we started may still be on the wire (only if we
                    // still own the slot — a takeover proves completion).
                    if !DRIVER.abort(slot.rmt_channel) {
                        // See wait_complete's timeout path: an unconfirmed
                        // handshake means the ISR core wedged mid-service and
                        // the frame bytes may still be referenced.
                        log::error!(
                            "RMT slot {}: abort handshake timed out in drop — ISR core wedged?",
                            slot.rmt_channel
                        );
                    }
                    slot.owner_wire = None;
                    slot.generation = slot.generation.wrapping_add(1);
                }
                if slot.bound_gpio == Some(self.gpio) {
                    slot.bound_gpio = None;
                }
            }
        }
        // Park while the lease is still held (park needs pad exclusivity);
        // the next open of this endpoint parks again.
        v3_rmt::park_gpio(self.gpio);
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

/// Does the manifest declare `/rmt/ws281x<index>` with both WS281x
/// capabilities?
fn declares_timing_resource(registry: &HwRegistry, index: usize) -> bool {
    let address = HwAddress::rmt_ws281x(index as u8);
    registry
        .ensure_capability(&address, HwCapability::Rmt)
        .is_ok()
        && registry
            .ensure_capability(&address, HwCapability::Ws281xOutput)
            .is_ok()
}

/// Turn a freshly configured esp-hal channel into a pool slot — interrupts
/// enabled, timing compiled, window all-STOP — or log why not.
fn adopt_channel(
    slot: usize,
    configured: Result<Channel<'static, Blocking, Tx>, esp_hal::rmt::ConfigError>,
) -> Option<PoolSlot> {
    let ch = slot as u8;
    match configured {
        Ok(tx) => {
            v3_rmt::enable_tx_interrupts(ch);
            // Leave the window all-STOP until the first frame prefills it, so
            // a spurious start can only transmit nothing.
            v3_rmt::clear_ram(ch);
            if let Err(error) = DRIVER.configure_default_clock(ch, &ChannelTiming::WS2812) {
                log::error!(
                    "Esp32V3RmtWs281xDriver: RMT slot {slot} timing configuration failed: \
                     {error:?}"
                );
                return None;
            }
            Some(PoolSlot {
                rmt_channel: ch,
                _tx: tx,
                bound_gpio: None,
                owner_wire: None,
                generation: 0,
            })
        }
        Err(error) => {
            log::error!("Esp32V3RmtWs281xDriver: RMT slot {slot} configure_tx failed: {error:?}");
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
