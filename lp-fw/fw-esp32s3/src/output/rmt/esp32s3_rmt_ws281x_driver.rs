//! The registry-facing WS281x driver: four concurrent RMT outputs on one S3.
//!
//! This is the seam between `lpc-hardware`'s endpoint/lease vocabulary and
//! [`lp_ws281x`]'s transmitter. It owns no sequencing and no register
//! knowledge — those are [`super::shared_driver::DRIVER`] and
//! [`super::s3_rmt`] respectively — only the mapping from *authored* endpoint
//! names to hardware.
//!
//! # Why the pin is bound at `open`, not at construction
//!
//! An endpoint is a board label (`ws281x:rmt:D10`), and which label a project
//! drives is authored data that arrives long after boot. So the RMT channel is
//! configured up front (its memory block and interrupt are chip resources, not
//! project ones) and its **pin** is connected when a project opens the
//! endpoint. `init_board` hands out no GPIO tokens, so the pad is recreated
//! with [`AnyPin::steal`] under the registry lease that has just granted
//! exclusive use of that address — see the SAFETY note at the call site.
//!
//! # Why four opens work where the C6's one did not
//!
//! fw-esp32c6 keeps its channel in `static mut` singletons, so a second open
//! on a different GPIO is refused by construction. Here the per-channel state
//! lives in [`lp_ws281x::ChannelState`] inside the shared driver, and the only
//! thing this type tracks is which RMT channel each open took. Channels are
//! handed out from a free list and returned on `Drop`, so a project can drive
//! every declared `/rmt/ws281xK` at once — which is the whole point of this
//! driver.
//!
//! # Timing
//!
//! Every channel is opened WS2812-class (GRB, 300 µs latch). The strip's own
//! colour order is the fixture node's `color_order`, applied above this
//! boundary; the driver stays GRB exactly as the C6's does.

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
use crate::output::rmt::s3_rmt::{self, BLOCKS_PER_CHANNEL, TX_BLOCKS, TX_CHANNELS};
use crate::output::rmt::shared_driver::{DRIVER, FRAME_TIMEOUT, install_isr};

const DRIVER_ID: &str = "esp32s3-rmt-ws281x";
const DISPLAY_LABEL: &str = "ESP32-S3 RMT WS281x";

/// One RMT TX channel this driver can hand out.
struct ChannelSlot {
    /// The manifest resource backing this channel (`/rmt/ws281xK`), claimed
    /// alongside the GPIO for as long as an output holds it.
    timing: HwAddress,
    /// The configured esp-hal channel. `Option` only because
    /// [`Channel::with_pin`] consumes and returns it; it is `Some` except
    /// inside that swap.
    tx: Option<Channel<'static, Blocking, Tx>>,
    /// Held by an open [`Esp32S3RmtWs281xOutput`].
    in_use: bool,
}

/// The channels, shared between the driver and every output it hands out.
///
/// Indexed by RMT channel number so a slot's position *is* the channel the
/// backend pokes; a board that declares only some `/rmt/ws281xK` resources
/// leaves the others `None` rather than shifting the numbering.
type ChannelTable = Rc<RefCell<[Option<ChannelSlot>; TX_CHANNELS]>>;

/// WS281x driver over the ESP32-S3 RMT, one endpoint per board-labelled GPIO
/// and up to one live output per declared `/rmt/ws281xK` resource.
pub struct Esp32S3RmtWs281xDriver {
    registry: Rc<HwRegistry>,
    channels: ChannelTable,
}

impl Esp32S3RmtWs281xDriver {
    /// Bind the RMT interrupt and configure one TX channel per `/rmt/ws281xK`
    /// resource the manifest declares.
    ///
    /// Channels are configured (memory block, divider, idle level) but not
    /// connected to any pin: that happens in [`Ws281xDriver::open`]. A resource
    /// the chip cannot back — an index past the four TX channels, or one the
    /// [`BlockPlan`](lp_ws281x::BlockPlan) absorbed into a lower channel — is
    /// skipped with a log line rather than offered and then failing to open.
    pub fn new(registry: Rc<HwRegistry>, mut rmt: Rmt<'static, Blocking>) -> Self {
        install_isr(&mut rmt);

        let config = TxChannelConfig::default()
            .with_clk_divider(1)
            .with_idle_output(true)
            .with_idle_output_level(Level::Low)
            .with_carrier_modulation(false)
            .with_memsize(BLOCKS_PER_CHANNEL);

        // The four creators are distinct types (`ChannelCreator<_, _, K>`), so
        // they cannot be iterated; each is consumed only when the manifest
        // declares its timing resource. The count offered therefore comes from
        // the manifest, bounded by what the chip has.
        let mut slots: [Option<ChannelSlot>; TX_CHANNELS] = [const { None }; TX_CHANNELS];
        if declares_timing_resource(&registry, 0) {
            slots[0] = adopt_channel(0, rmt.channel0.configure_tx(&config));
        }
        if declares_timing_resource(&registry, 1) {
            slots[1] = adopt_channel(1, rmt.channel1.configure_tx(&config));
        }
        if declares_timing_resource(&registry, 2) {
            slots[2] = adopt_channel(2, rmt.channel2.configure_tx(&config));
        }
        if declares_timing_resource(&registry, 3) {
            slots[3] = adopt_channel(3, rmt.channel3.configure_tx(&config));
        }

        log::info!(
            "Esp32S3RmtWs281xDriver: {} of {} RMT TX channels available for WS281x output",
            slots.iter().flatten().count(),
            TX_CHANNELS,
        );

        Self {
            registry,
            channels: Rc::new(RefCell::new(slots)),
        }
    }

    fn endpoint_id(&self, spec: &HwEndpointSpec) -> HwEndpointId {
        HwEndpointId::for_driver_spec(self.driver_id(), spec)
    }

    /// The lowest RMT channel that is configured, unclaimed, and whose timing
    /// resource the registry still reports as free.
    fn free_channel(&self) -> Option<u8> {
        let slots = self.channels.borrow();
        slots.iter().enumerate().find_map(|(ch, slot)| {
            let slot = slot.as_ref()?;
            if slot.in_use
                || !self
                    .registry
                    .endpoint_status_for(&slot.timing)
                    .is_available()
            {
                return None;
            }
            Some(ch as u8)
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

    /// Connect `gpio` to RMT channel `ch` and arm the channel's wire timing.
    ///
    /// Called with the registry lease for both addresses already held.
    fn bind_channel(&self, ch: u8, gpio: u8) -> Result<(), HardwareEndpointError> {
        let mut slots = self.channels.borrow_mut();
        let Some(slot) = slots[ch as usize].as_mut() else {
            return Err(HardwareEndpointError::Other {
                message: format!("RMT channel {ch} is not configured"),
            });
        };
        let Some(tx) = slot.tx.take() else {
            return Err(HardwareEndpointError::Other {
                message: format!("RMT channel {ch} has no transmitter"),
            });
        };

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
        s3_rmt::clear_ram(&TX_BLOCKS, ch);

        DRIVER
            .configure_default_clock(ch, &ChannelTiming::WS2812)
            .map_err(|error| HardwareEndpointError::Other {
                message: format!("RMT channel {ch} timing configuration failed: {error:?}"),
            })?;
        slot.in_use = true;
        Ok(())
    }
}

impl HwDriver for Esp32S3RmtWs281xDriver {
    fn driver_id(&self) -> &str {
        DRIVER_ID
    }

    fn display_label(&self) -> &str {
        DISPLAY_LABEL
    }
}

impl Ws281xDriver for Esp32S3RmtWs281xDriver {
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

        let ch = self
            .free_channel()
            .ok_or_else(|| HardwareEndpointError::EndpointUnavailable {
                endpoint_id: endpoint_id.clone(),
                reason: format!(
                    "all {} RMT WS281x channels are in use",
                    self.offered_channels()
                ),
            })?;
        let timing_address = HwAddress::rmt_ws281x(ch);

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

        if let Err(error) = self.bind_channel(ch, gpio) {
            let _ = self.registry.release(&lease);
            return Err(error);
        }

        log::info!(
            "Esp32S3RmtWs281xDriver::open: endpoint={endpoint_id} gpio={} rmt_ch={ch} bytes={}",
            gpio_address.as_str(),
            config.byte_count(),
        );
        #[cfg(feature = "frame-dump")]
        frame_dump::log_open(endpoint_id, config.byte_count());

        Ok(Box::new(Esp32S3RmtWs281xOutput {
            registry: Rc::clone(&self.registry),
            channels: Rc::clone(&self.channels),
            lease: Some(lease),
            channel: ch,
            byte_count: config.byte_count(),
            #[cfg(feature = "frame-dump")]
            dump: FrameDump::new(),
        }))
    }
}

/// One opened WS281x output: an RMT channel, its registry lease, and the frame
/// size writes must match.
///
/// Never named outside this module — callers get a `Box<dyn Ws281xOutput>`.
struct Esp32S3RmtWs281xOutput {
    registry: Rc<HwRegistry>,
    channels: ChannelTable,
    lease: Option<HardwareLease>,
    channel: u8,
    byte_count: u32,
    /// Serial transcript of the frames this channel transmitted. Present only
    /// in a `frame-dump` build — see [`super::frame_dump`] for why the gate is
    /// compile-time rather than a runtime flag.
    #[cfg(feature = "frame-dump")]
    dump: FrameDump,
}

impl Ws281xOutput for Esp32S3RmtWs281xOutput {
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

        // Reported after the send, not before it: the transcript is evidence
        // about bytes that reached the wire, and a frame that timed out or was
        // refused is not one of them.
        #[cfg(feature = "frame-dump")]
        self.dump.on_write(data);
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

impl Drop for Esp32S3RmtWs281xOutput {
    /// Stop anything in flight, hand the RMT channel back to the free list, and
    /// release the lease — in that order, so the channel is never offered to a
    /// new open while its transmitter is still running.
    fn drop(&mut self) {
        DRIVER.abort(self.channel);
        if let Some(slot) = self.channels.borrow_mut()[self.channel as usize].as_mut() {
            slot.in_use = false;
        }
        if let Some(lease) = self.lease.take() {
            if let Err(error) = self.registry.release(&lease) {
                log::warn!("Esp32S3RmtWs281xOutput: failed to release hardware lease: {error}");
            }
        }
    }
}

/// Does the manifest declare `/rmt/ws281x<ch>` with both WS281x capabilities,
/// and does the block plan leave that channel with memory of its own?
fn declares_timing_resource(registry: &HwRegistry, ch: u8) -> bool {
    if !TX_BLOCKS.is_available(ch) {
        return false;
    }
    let address = HwAddress::rmt_ws281x(ch);
    registry
        .ensure_capability(&address, HwCapability::Rmt)
        .is_ok()
        && registry
            .ensure_capability(&address, HwCapability::Ws281xOutput)
            .is_ok()
}

/// Turn a freshly configured esp-hal channel into a slot, or log why not.
fn adopt_channel(
    ch: u8,
    configured: Result<Channel<'static, Blocking, Tx>, esp_hal::rmt::ConfigError>,
) -> Option<ChannelSlot> {
    match configured {
        Ok(tx) => {
            s3_rmt::enable_tx_interrupts(ch);
            Some(ChannelSlot {
                timing: HwAddress::rmt_ws281x(ch),
                tx: Some(tx),
                in_use: false,
            })
        }
        Err(error) => {
            log::error!("Esp32S3RmtWs281xDriver: RMT channel {ch} configure_tx failed: {error:?}");
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
            reason: format!("invalid ESP32-S3 GPIO address: {address}"),
        })?;
    if !gpio_exists(gpio) {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("ESP32-S3 has no GPIO {gpio}"),
        });
    }
    Ok(gpio)
}

/// Does this chip have GPIO `pin` at all?
///
/// The ESP32-S3 numbers **GPIO0-21 and GPIO26-48** — there is no GPIO22-25 and
/// no GPIO49+. This is not a policy filter (reserving pins is the manifest's
/// job); it is the physical pin list, and it is checked because
/// [`AnyPin::steal`] *panics* on a number the chip does not have. A
/// hand-written `/hardware.json` naming `/gpio/22` must fail the open, not the
/// boot.
fn gpio_exists(pin: u8) -> bool {
    pin <= 21 || (26..=48).contains(&pin)
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
