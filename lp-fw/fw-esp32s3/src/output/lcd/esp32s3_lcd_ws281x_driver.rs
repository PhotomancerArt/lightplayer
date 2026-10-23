//! The registry-facing WS281x driver over LCD_CAM: up to eight lanes from
//! one DMA stream.
//!
//! The endpoints are the RMT driver's (`ws281x:local:<board label>`, one per
//! board-labelled output pad, [`crate::output::ws281x_endpoints`]), so a
//! project opens unchanged on either. What differs is behind them:
//!
//! - **A lane is a bit of the bus**, `LCD_DATA_OUT<k>`, routed to its pad
//!   through the GPIO matrix when the endpoint opens. Lanes come from the
//!   manifest's `/lcd/ws281xK` resources (K < 8), the only authority on how
//!   many there are, exactly as `/rmt/ws281xK` is for the RMT driver.
//! - **All lanes clock out together.** A frame lasts as long as the longest
//!   lane; shorter ones are held low after their last bit.
//! - **No refill deadline.** The frame is encoded once into internal RAM and
//!   GDMA feeds the bus, so radio interrupts, masked critical sections and a
//!   flash write with the cache off cannot touch a frame in flight. FS3
//!   measured all three (the LED-channels spike), which is what lets
//!   [`Ws281xOutput::background_tx_safe`] say `true` here.
//!
//! # Pins
//!
//! Refused at open, with the reason: GPIO26–32 (the in-package flash) and
//! GPIO33–37 (octal PSRAM on R8 parts), GPIO19/20 (USB-Serial-JTAG: driving
//! them drops the host link). The board profile reserves the rest it must.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use esp_hal::Blocking;
use esp_hal::gpio::{AnyPin, OutputSignal};
use esp_hal::lcd_cam::lcd::i8080::I8080;
use lp_ws281x::parallel::LANES;
use lpc_hardware::{
    HardwareEndpointError, HardwareLease, HwAddress, HwCapability, HwClaim, HwDriver, HwEndpoint,
    HwEndpointId, HwEndpointKind, HwEndpointSpec, HwEndpointStatus, HwRegistry, HwResource,
    OutputError, Ws281xConfig, Ws281xDriver, Ws281xOutput,
};

use super::lcd_bus::LcdBus;
use crate::output::ws281x_endpoints::{
    byte_len_for_byte_count, endpoint_error_to_output_error, gpio_number, has_board_assigned_label,
    validate_byte_count, ws281x_local_spec,
};

const DRIVER_ID: &str = "esp32s3-lcd-ws281x";
const DISPLAY_LABEL: &str = "ESP32-S3 LCD_CAM WS281x";

/// One lane's bookkeeping on the driver side.
#[derive(Clone, Copy)]
struct LaneSlot {
    /// Declared by the manifest (`/lcd/ws281xK`).
    declared: bool,
    in_use: bool,
    /// The pad the lane drives while open.
    gpio: Option<u8>,
}

struct Shared {
    bus: LcdBus,
    lanes: [LaneSlot; LANES],
}

/// WS281x over LCD_CAM, one endpoint per board-labelled GPIO and one live
/// output per declared lane.
pub struct Esp32S3LcdWs281xDriver {
    registry: Rc<HwRegistry>,
    shared: Rc<RefCell<Shared>>,
}

impl Esp32S3LcdWs281xDriver {
    /// Configure the bus (no pads yet) and read the declared lanes.
    pub fn new(registry: Rc<HwRegistry>, lcd: I8080<'static, Blocking>) -> Self {
        let mut lanes = [LaneSlot {
            declared: false,
            in_use: false,
            gpio: None,
        }; LANES];
        for (index, lane) in lanes.iter_mut().enumerate() {
            lane.declared = registry
                .ensure_capability(
                    &HwAddress::lcd_ws281x(index as u8),
                    HwCapability::Ws281xOutput,
                )
                .is_ok();
        }
        log::info!(
            "Esp32S3LcdWs281xDriver: {} lanes declared (bus {} Hz, 3 slots a bit)",
            lanes.iter().filter(|l| l.declared).count(),
            super::lcd_bus::BUS_HZ,
        );
        Self {
            registry,
            shared: Rc::new(RefCell::new(Shared {
                bus: LcdBus::new(lcd),
                lanes,
            })),
        }
    }

    /// Attach the CAM-half frame check (`ws281x_frame_check`).
    #[cfg(feature = "ws281x_frame_check")]
    pub fn attach_check(&self, check: super::frame_check::FrameCheck) {
        self.shared.borrow_mut().bus.check = Some(check);
    }

    fn endpoint_id(&self, spec: &HwEndpointSpec) -> HwEndpointId {
        HwEndpointId::for_driver_spec(self.driver_id(), spec)
    }

    fn offered_lanes(&self) -> usize {
        self.shared
            .borrow()
            .lanes
            .iter()
            .filter(|l| l.declared)
            .count()
    }

    /// The lowest declared lane that is unclaimed here and in the registry.
    fn free_lane(&self) -> Option<u8> {
        let shared = self.shared.borrow();
        shared
            .lanes
            .iter()
            .enumerate()
            .find(|(index, lane)| {
                lane.declared
                    && !lane.in_use
                    && self
                        .registry
                        .endpoint_status_for(&HwAddress::lcd_ws281x(*index as u8))
                        .is_available()
            })
            .map(|(index, _)| index as u8)
    }

    fn lane_status(&self) -> HwEndpointStatus {
        match self.free_lane() {
            Some(_) => HwEndpointStatus::Available,
            None => HwEndpointStatus::Unavailable {
                reason: format!("all {} LCD WS281x lanes are in use", self.offered_lanes()),
            },
        }
    }

    fn endpoint_status(&self, gpio: &HwResource, lane: &HwEndpointStatus) -> HwEndpointStatus {
        let gpio_status = self.registry.endpoint_status_of(gpio);
        if !gpio_status.is_available() {
            return gpio_status;
        }
        lane.clone()
    }

    fn gpio_for_endpoint(
        &self,
        endpoint_id: &HwEndpointId,
    ) -> Result<HwAddress, HardwareEndpointError> {
        if self.offered_lanes() > 0 {
            for resource in self.registry.manifest().resources() {
                if !resource.supports(HwCapability::GpioOutput)
                    || !has_board_assigned_label(resource.address(), resource.display_label())
                {
                    continue;
                }
                if self.endpoint_id(&ws281x_local_spec(resource.display_label())) == *endpoint_id {
                    return Ok(resource.address().clone());
                }
            }
        }
        Err(HardwareEndpointError::UnknownEndpoint {
            kind: HwEndpointKind::Ws281x,
            endpoint_id: endpoint_id.clone(),
        })
    }
}

impl HwDriver for Esp32S3LcdWs281xDriver {
    fn driver_id(&self) -> &str {
        DRIVER_ID
    }

    fn display_label(&self) -> &str {
        DISPLAY_LABEL
    }
}

impl Ws281xDriver for Esp32S3LcdWs281xDriver {
    fn endpoints(&self) -> Vec<HwEndpoint> {
        if self.offered_lanes() == 0 {
            return Vec::new();
        }
        let lane = self.lane_status();
        let mut endpoints = Vec::new();
        for resource in self.registry.manifest().resources() {
            if !resource.supports(HwCapability::GpioOutput)
                || !has_board_assigned_label(resource.address(), resource.display_label())
            {
                continue;
            }
            let spec = ws281x_local_spec(resource.display_label());
            endpoints.push(HwEndpoint::new(
                self.endpoint_id(&spec),
                spec,
                HwEndpointKind::Ws281x,
                self.driver_id(),
                resource.address().clone(),
                resource.display_label(),
                self.endpoint_status(resource, &lane),
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
        if let Some(reason) = pad_refusal(gpio) {
            return Err(HardwareEndpointError::EndpointUnavailable {
                endpoint_id: endpoint_id.clone(),
                reason: reason.into(),
            });
        }
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
        let lane = self
            .free_lane()
            .ok_or_else(|| HardwareEndpointError::EndpointUnavailable {
                endpoint_id: endpoint_id.clone(),
                reason: format!("all {} LCD WS281x lanes are in use", self.offered_lanes()),
            })?;
        let lane_address = HwAddress::lcd_ws281x(lane);
        self.registry
            .ensure_capability(&gpio_address, HwCapability::GpioOutput)?;
        let lease = self.registry.claim_bundle(HwClaim::new(
            self.driver_id(),
            vec![gpio_address.clone(), lane_address],
        ))?;

        let bytes = byte_len_for_byte_count(config.byte_count());
        let opened = self.shared.borrow_mut().bus.open_lane(lane, bytes, |lcd| {
            // SAFETY: `init_board` keeps no GPIO tokens, so the pad is
            // recreated here. Exclusivity is the registry's: the lease just
            // claimed covers this GPIO for the life of the output, and
            // `gpio_number` checked the pin exists, so `steal` cannot panic.
            // The i8080 builder keeps no guard for the pin (unlike the RMT
            // channel's), so a later rebind of the same pad cannot disconnect
            // it (docs/defects/2026-09-02-same-gpio-rebind-disconnects-the-pad.md).
            bind_lane(lcd, lane, unsafe { AnyPin::steal(gpio) })
        });
        if let Err(error) = opened {
            let _ = self.registry.release(&lease);
            return Err(error);
        }
        {
            let mut shared = self.shared.borrow_mut();
            shared.lanes[lane as usize].in_use = true;
            shared.lanes[lane as usize].gpio = Some(gpio);
            #[cfg(feature = "ws281x_frame_check")]
            if let Some(check) = shared.bus.check.as_mut() {
                check.bind(lane, gpio);
            }
        }
        log::info!(
            "Esp32S3LcdWs281xDriver::open: endpoint={endpoint_id} gpio={} lane={lane} bytes={}",
            gpio_address.as_str(),
            config.byte_count(),
        );
        Ok(Box::new(Esp32S3LcdWs281xOutput {
            registry: Rc::clone(&self.registry),
            shared: Rc::clone(&self.shared),
            lease: Some(lease),
            lane,
            byte_count: config.byte_count(),
        }))
    }
}

struct Esp32S3LcdWs281xOutput {
    registry: Rc<HwRegistry>,
    shared: Rc<RefCell<Shared>>,
    lease: Option<HardwareLease>,
    lane: u8,
    byte_count: u32,
}

impl Ws281xOutput for Esp32S3LcdWs281xOutput {
    fn write(&mut self, data: &[u8]) -> Result<(), OutputError> {
        // SAFETY: `data` outlives this call, and `wait_complete` below has
        // encoded it before returning.
        unsafe { self.start(data)? };
        self.wait_complete()
    }

    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError> {
        validate_byte_count(config.byte_count()).map_err(endpoint_error_to_output_error)?;
        self.shared
            .borrow_mut()
            .bus
            .resize_lane(self.lane, byte_len_for_byte_count(config.byte_count()))?;
        self.byte_count = config.byte_count();
        Ok(())
    }

    unsafe fn start(&mut self, data: &[u8]) -> Result<(), OutputError> {
        // SAFETY: forwarded — the caller keeps `data` until this output's
        // next `wait_complete` or drop, which is the bus's contract too.
        unsafe { self.shared.borrow_mut().bus.start(self.lane, data) }
    }

    fn wait_complete(&mut self) -> Result<(), OutputError> {
        self.shared.borrow_mut().bus.wait_complete(self.lane)
    }

    /// GDMA feeds the bus from internal RAM with no CPU involved, so a frame
    /// survives whatever the render core does next, masked interrupts and
    /// flash writes included. Measured on FS3 (LED-channels spike, 2026-10-22).
    fn background_tx_safe(&self) -> bool {
        true
    }
}

impl Drop for Esp32S3LcdWs281xOutput {
    fn drop(&mut self) {
        {
            let mut shared = self.shared.borrow_mut();
            let gpio = shared.lanes[self.lane as usize].gpio.take();
            let lane = self.lane;
            shared.bus.close_lane(lane, || {
                if let Some(gpio) = gpio {
                    // SAFETY: the lease on this pad is still held (released
                    // below), so no other token for it exists.
                    let pin = unsafe { AnyPin::steal(gpio) };
                    lane_signal(lane).disconnect_from(&pin);
                }
            });
            shared.lanes[self.lane as usize].in_use = false;
        }
        if let Some(lease) = self.lease.take() {
            if let Err(error) = self.registry.release(&lease) {
                log::warn!("Esp32S3LcdWs281xOutput: failed to release hardware lease: {error}");
            }
        }
    }
}

/// Why a pad cannot carry a lane on this chip, if it cannot.
fn pad_refusal(gpio: u8) -> Option<&'static str> {
    match gpio {
        19 | 20 => Some("GPIO19/20 are USB-Serial-JTAG; driving them drops the host link"),
        26..=32 => Some("GPIO26–32 belong to the in-package flash"),
        33..=37 => Some("GPIO33–37 belong to the octal PSRAM"),
        _ => None,
    }
}

fn bind_lane(
    lcd: I8080<'static, Blocking>,
    lane: u8,
    pin: AnyPin<'static>,
) -> I8080<'static, Blocking> {
    match lane {
        0 => lcd.with_data0(pin),
        1 => lcd.with_data1(pin),
        2 => lcd.with_data2(pin),
        3 => lcd.with_data3(pin),
        4 => lcd.with_data4(pin),
        5 => lcd.with_data5(pin),
        6 => lcd.with_data6(pin),
        _ => lcd.with_data7(pin),
    }
}

fn lane_signal(lane: u8) -> OutputSignal {
    match lane {
        0 => OutputSignal::LCD_DATA_0,
        1 => OutputSignal::LCD_DATA_1,
        2 => OutputSignal::LCD_DATA_2,
        3 => OutputSignal::LCD_DATA_3,
        4 => OutputSignal::LCD_DATA_4,
        5 => OutputSignal::LCD_DATA_5,
        6 => OutputSignal::LCD_DATA_6,
        _ => OutputSignal::LCD_DATA_7,
    }
}
