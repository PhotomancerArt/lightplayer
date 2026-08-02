//! The registry-facing WS281x driver: concurrent RMT outputs on one ESP32-C6.
//!
//! This is the seam between `lpc-hardware`'s endpoint/lease vocabulary and
//! [`lp_ws281x`]'s transmitter. It owns no sequencing and no register
//! knowledge — those are [`super::shared_driver::DRIVER`] and
//! [`super::c6_rmt`] respectively — only the mapping from *authored* endpoint
//! names to hardware.
//!
//! Structurally this is `fw-esp32s3`'s `Esp32S3RmtWs281xDriver` with the
//! classic firmware's manifest-index indirection, described next.
//!
//! # Manifest channel → RMT slot: the absorbed-slot guard
//!
//! The block plan is computed **here, at init, from the manifest**: the
//! number of declared `/rmt/ws281xK` resources goes through
//! [`super::c6_rmt::plan_for_declared`] and is published once via
//! [`super::c6_rmt::TX_PLAN`] before any channel is configured. Two declared
//! channels get one block each; a single declared channel absorbs the whole
//! RMT RAM — 192-word window, legacy-class refill margin — because a board
//! with one strip has nothing else to spend the blocks on.
//!
//! The silicon-slot → manifest-index mapping is [`manifest_index_for_slot`]
//! (the plan's `index_of_slot`), and the channel-creation block below
//! *never offers an absorbed slot to `configure_tx` in the first place*. That
//! is the construction the classic backend adopted after the S3 experiment's
//! two-block harness config failed at `configure_tx`: it kept asking esp-hal
//! for every channel, and esp-hal correctly answered `MemoryBlockNotAvailable`
//! for the absorbed ones.
//!
//! [`ChannelSlot`] consequently carries both identities: `timing` is the
//! manifest address (`/rmt/ws281x0`), `rmt_channel` is the silicon slot. The
//! registry only ever sees the former; [`super::c6_rmt`] only ever sees the
//! latter.
//!
//! # The board's side of the count
//!
//! `main.rs` constructs exactly one of these at boot and hands it the RMT
//! peripheral. How many outputs it then offers is the manifest's answer, not
//! this file's: the XIAO C6 profile declares `/rmt/ws281x0` and
//! `/rmt/ws281x1`, and a board profile that declares fewer simply gets fewer
//! — with wider windows. The ceiling is [`TX_CHANNELS`].
//!
//! # Why the pin is bound at `open`, not at construction
//!
//! An endpoint is a board label (`ws281x:rmt:D10`), and which label a project
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
//! boundary; the driver stays GRB exactly as the legacy C6 path does.

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

use crate::output::rmt::c6_rmt::{self, BLOCK_WORDS, TX_CHANNELS, TX_PLAN};
use crate::output::rmt::shared_driver::{
    DRIVER, FRAME_TIMEOUT, install_isr, report_telemetry_if_due,
};

const DRIVER_ID: &str = "esp32c6-rmt-ws281x";
const DISPLAY_LABEL: &str = "ESP32-C6 RMT WS281x";

/// One RMT TX channel this driver can hand out.
struct ChannelSlot {
    /// The manifest resource backing this channel (`/rmt/ws281xK`), claimed
    /// alongside the GPIO for as long as an output holds it. `K` is the
    /// *manifest* index, which under a widened block plan is not the RMT slot
    /// number — see the module docs.
    timing: HwAddress,
    /// The silicon RMT channel the manifest index resolved to (`K *
    /// SLOT_STRIDE`). Everything below `lp_ws281x` speaks this number.
    rmt_channel: u8,
    /// The configured esp-hal channel. `Option` only because
    /// [`Channel::with_pin`] consumes and returns it; it is `Some` except
    /// inside that swap.
    tx: Option<Channel<'static, Blocking, Tx>>,
    /// Held by an open [`Esp32C6RmtWs281xOutput`].
    in_use: bool,
}

/// The channels, shared between the driver and every output it hands out.
///
/// Indexed by **manifest** channel `K`, not by RMT slot: a board that declares
/// only some `/rmt/ws281xK` resources leaves the others `None` rather than
/// shifting the numbering. Sized by [`TX_CHANNELS`] — the most channels any
/// plan can offer — with the plan deciding how many entries are ever filled.
type ChannelTable = Rc<RefCell<[Option<ChannelSlot>; TX_CHANNELS]>>;

/// WS281x driver over the ESP32-C6 RMT, one endpoint per board-labelled GPIO
/// and up to one live output per declared `/rmt/ws281xK` resource.
pub struct Esp32C6RmtWs281xDriver {
    registry: Rc<HwRegistry>,
    channels: ChannelTable,
}

impl Esp32C6RmtWs281xDriver {
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
        if let Some(plan) = c6_rmt::plan_for_declared(declared) {
            if let Err(error) = TX_PLAN.init(plan) {
                // A harness or a second construction raced us with a different
                // plan; the stored one stays authoritative.
                log::error!("Esp32C6RmtWs281xDriver: block plan already published: {error:?}");
            }
        }

        install_isr(&mut rmt);

        let config = TxChannelConfig::default()
            .with_clk_divider(1)
            .with_idle_output(true)
            .with_idle_output_level(Level::Low)
            .with_carrier_modulation(false);

        let mut slots: [Option<ChannelSlot>; TX_CHANNELS] = [const { None }; TX_CHANNELS];

        // The two creators are distinct types (`ChannelCreator<_, _, K>`), so
        // they cannot be iterated; each is named once and consumed only when
        // the block plan says its slot owns memory AND the manifest declares
        // the timing resource that maps to it. An absorbed slot never reaches
        // `configure_tx`. `memsize` is the slot's own share of the plan — up
        // to all four blocks for a lone channel (the RX blocks absorbed, which
        // esp-hal permits and marks reserved).
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
        );

        // No `{:?}` here: this crate builds with `-Zfmt-debug=none` (flash
        // budget), which formats Debug to nothing — measured on the desk as a
        // boot line reading `plan=`.
        log::info!(
            "Esp32C6RmtWs281xDriver: {} WS281x channels for {} declared \
             (ch0_blocks={} ch0_window_words={} ch0_half_words={})",
            slots.iter().flatten().count(),
            declared,
            TX_PLAN.blocks(0),
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
        c6_rmt::clear_ram(ch);

        DRIVER
            .configure_default_clock(ch, &ChannelTiming::WS2812)
            .map_err(|error| HardwareEndpointError::Other {
                message: format!("RMT channel {ch} timing configuration failed: {error:?}"),
            })?;
        slot.in_use = true;
        Ok(ch)
    }
}

impl HwDriver for Esp32C6RmtWs281xDriver {
    fn driver_id(&self) -> &str {
        DRIVER_ID
    }

    fn display_label(&self) -> &str {
        DISPLAY_LABEL
    }
}

impl Ws281xDriver for Esp32C6RmtWs281xDriver {
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
            let spec = ws281x_rmt_spec(resource.display_label());
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
            "Esp32C6RmtWs281xDriver::open: endpoint={endpoint_id} gpio={} ws281x_ch={index} \
             rmt_slot={ch} bytes={}",
            gpio_address.as_str(),
            config.byte_count(),
        );

        Ok(Box::new(Esp32C6RmtWs281xOutput {
            registry: Rc::clone(&self.registry),
            channels: Rc::clone(&self.channels),
            lease: Some(lease),
            index,
            channel: ch,
            byte_count: config.byte_count(),
        }))
    }
}

/// One opened WS281x output: an RMT channel, its registry lease, and the frame
/// size writes must match.
///
/// Never named outside this module — callers get a `Box<dyn Ws281xOutput>`.
struct Esp32C6RmtWs281xOutput {
    registry: Rc<HwRegistry>,
    channels: ChannelTable,
    lease: Option<HardwareLease>,
    /// Manifest channel index — the slot table's key.
    index: usize,
    /// RMT slot — what `lp_ws281x` and the register backend address.
    channel: u8,
    byte_count: u32,
}

impl Ws281xOutput for Esp32C6RmtWs281xOutput {
    fn write(&mut self, data: &[u8]) -> Result<(), OutputError> {
        let expected_len = byte_len_for_byte_count(self.byte_count);
        if data.len() != expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        // Blocking send: the borrow of `data` provably outlives the
        // transmission because `send_blocking` does not return until the
        // channel reports complete, and it aborts the channel on every other
        // exit path. The spin callback is the hang detector — a frame that
        // outlives its deadline is aborted and reported rather than wedging
        // the render loop forever.
        let started = Instant::now();
        let mut timed_out = false;
        let result = DRIVER.send_blocking(self.channel, data, || {
            if !timed_out && started.elapsed() > FRAME_TIMEOUT {
                timed_out = true;
                DRIVER.abort(self.channel);
            }
        });

        // After the frame, never during it, and a no-op unless the
        // `ws281x_telemetry` feature is on.
        report_telemetry_if_due();

        if let Err(error) = result {
            return Err(start_error_to_output_error(self.channel, error));
        }
        if timed_out {
            return Err(OutputError::Other {
                message: format!(
                    "RMT channel {} frame did not complete within {} ms",
                    self.channel,
                    FRAME_TIMEOUT.as_millis(),
                ),
            });
        }
        Ok(())
    }

    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError> {
        validate_byte_count(config.byte_count()).map_err(endpoint_error_to_output_error)?;
        self.byte_count = config.byte_count();
        Ok(())
    }
}

impl Drop for Esp32C6RmtWs281xOutput {
    /// Stop anything in flight, hand the RMT channel back to the free list, and
    /// release the lease — in that order, so the channel is never offered to a
    /// new open while its transmitter is still running.
    fn drop(&mut self) {
        DRIVER.abort(self.channel);
        if let Some(slot) = self.channels.borrow_mut()[self.index].as_mut() {
            slot.in_use = false;
        }
        if let Some(lease) = self.lease.take() {
            if let Err(error) = self.registry.release(&lease) {
                log::warn!("Esp32C6RmtWs281xOutput: failed to release hardware lease: {error}");
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
/// [`manifest_index_for_slot`], which is why there is no `is_available` check
/// here.
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
            c6_rmt::enable_tx_interrupts(ch);
            Some(ChannelSlot {
                timing: HwAddress::rmt_ws281x(index as u8),
                rmt_channel: ch,
                tx: Some(tx),
                in_use: false,
            })
        }
        Err(error) => {
            log::error!(
                "Esp32C6RmtWs281xDriver: RMT slot {slot} (ws281x{index}) configure_tx failed: \
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
            reason: format!("invalid ESP32-C6 GPIO address: {address}"),
        })?;
    if !gpio_exists(gpio) {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("ESP32-C6 has no GPIO {gpio}"),
        });
    }
    Ok(gpio)
}

/// Does this chip have GPIO `pin` at all?
///
/// The ESP32-C6 numbers **GPIO0-30** contiguously, every one of them capable of
/// output — taken from `esp-metadata-generated`'s `for_each_gpio!` table for
/// `esp32c6`, the same table [`AnyPin::steal`] asserts against, and it *panics*
/// on a number the chip does not have. This is not a policy filter (reserving
/// pins is the manifest's job — GPIO12/13 are USB D-/D+ and the board manifest
/// is what keeps them out of reach); a hand-written `/hardware.json` naming
/// `/gpio/31` must fail the open, not the boot.
fn gpio_exists(pin: u8) -> bool {
    pin <= 30
}

/// A resource whose display label is still `GPIO<n>` has no board label, and a
/// pin the board does not name is not one a project should be offered.
fn has_board_assigned_label(address: &HwAddress, display_label: &str) -> bool {
    let Some(raw) = address.as_str().strip_prefix("/gpio/") else {
        return false;
    };
    !display_label.eq_ignore_ascii_case(&format!("GPIO{raw}"))
}

fn ws281x_rmt_spec(config: &str) -> HwEndpointSpec {
    HwEndpointSpec::parse(format!("ws281x:rmt:{config}"))
        .expect("manifest display label should form a valid endpoint spec")
}
