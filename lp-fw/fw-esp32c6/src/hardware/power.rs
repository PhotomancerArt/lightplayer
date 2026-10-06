//! ESP32-C6 power-off: deep sleep with an EXT1 wake on the power button's pin.
//!
//! The server decides *when* (after the frame, with every project unloaded);
//! this decides *whether it can* and then does it. Waking is a reset.
//!
//! [`PowerPlatform::enter_power_off`] does not sleep on the spot: it hands the
//! sleep to the USB link task ([`fw_esp32_common::usb_link::when_drained`]),
//! which runs it once the host has acknowledged what the link holds (the
//! last replies and log lines), no host is up, or a second has passed — the
//! same drain a reboot gets. So it returns `Ok` and the server loop runs on,
//! dark (every project is unloaded), for at most that second.
//!
//! Only LP GPIO 0–7 can be an EXT1 source on the C6. The board manifest marks
//! those `deep-sleep-wake`, and a request for any other pin is refused while
//! the device is still running — a sleep nothing can wake from is the one
//! failure this module exists to make impossible.

extern crate alloc;

use alloc::format;
use alloc::rc::Rc;
use core::cell::Cell;

use critical_section::Mutex;

use esp_hal::delay::Delay;
use esp_hal::gpio::{AnyPin, Input, InputConfig, Pull, RtcPinWithResistors};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::{Ext1WakeupSource, WakeupLevel};
use esp_hal::time::{Duration, Instant};
use lpa_server::{PowerError, PowerOffRequest, PowerPlatform, PowerWakeLevel};
use lpc_hardware::{HardwareSystem, HwAddress, HwCapability};

/// How long the pin must sit at its NON-wake level before sleeping. EXT1 is
/// level-triggered: sleeping while the wake level is still present wakes
/// again at once — a held button, or a switch still bouncing.
const SETTLE_MS: u64 = 100;
const POLL_MS: u32 = 5;
const LOG_EVERY_MS: u64 = 1_000;

/// The sleep the link task runs once drained: the wake pin and whether it
/// wakes on high.
static PENDING_SLEEP: Mutex<Cell<Option<(u8, bool)>>> = Mutex::new(Cell::new(None));

/// The C6's power-off, installed on the server with
/// [`lpa_server::LpServer::set_power_platform`].
pub struct Esp32C6PowerPlatform {
    hardware: Rc<HardwareSystem>,
}

impl Esp32C6PowerPlatform {
    pub fn new(hardware: Rc<HardwareSystem>) -> Self {
        Self { hardware }
    }

    /// The GPIO number behind `request`'s endpoint, if it can wake the chip.
    fn wake_gpio(&self, request: &PowerOffRequest) -> Result<u8, PowerError> {
        let endpoint = self
            .hardware
            .button_endpoints()
            .into_iter()
            .find(|candidate| candidate.spec() == &request.endpoint)
            .ok_or_else(|| {
                PowerError::msg(format!(
                    "unknown power button endpoint {}",
                    request.endpoint
                ))
            })?;
        self.hardware
            .registry()
            .ensure_capability(endpoint.address(), HwCapability::DeepSleepWake)
            .map_err(|error| {
                PowerError::msg(format!(
                    "{} cannot wake the chip from deep sleep (only D0-D2 on the XIAO C6): {error}",
                    request.endpoint
                ))
            })?;
        gpio_number(endpoint.address())
    }
}

impl PowerPlatform for Esp32C6PowerPlatform {
    fn check_power_off(&self, request: &PowerOffRequest) -> Result<(), PowerError> {
        self.wake_gpio(request).map(|_| ())
    }

    fn enter_power_off(&self, request: &PowerOffRequest) -> Result<(), PowerError> {
        let gpio = self.wake_gpio(request)?;
        let wake_high = request.wake_level == PowerWakeLevel::High;
        log::info!(
            "[fw-esp32c6] powering off once the host link drains; wake on gpio{gpio} {}",
            if wake_high { "high" } else { "low" }
        );
        critical_section::with(|cs| PENDING_SLEEP.borrow(cs).set(Some((gpio, wake_high))));
        fw_esp32_common::usb_link::when_drained(sleep_now);
        Ok(())
    }

    fn host_attached(&self) -> bool {
        crate::board::esp32c6::usb_connection::host_enumerated()
    }
}

/// Deep sleep on the pending wake pin; run by the link task (see the module
/// docs). Never returns: waking is a reset.
fn sleep_now() -> ! {
    let Some((gpio, wake_high)) = critical_section::with(|cs| PENDING_SLEEP.borrow(cs).get())
    else {
        // Unreachable: the request is stored before the sleep is handed
        // over. A reset is the safe way out.
        crate::board::esp32c6::restart::restart()
    };
    {
        // The RTC and super watchdogs live in the LP domain and keep counting
        // through deep sleep: left armed, they would reset the chip awake. The
        // release wait below may also outlast the watchdog's timeout.
        let mut rtc = Rtc::new(unsafe { esp_hal::peripherals::LPWR::steal() });
        rtc.rwdt.disable();
        rtc.swd.disable();

        // Say "powering off" on the status LED (three flashes, then dark)
        // while the watchdogs are off: it blocks for under a second. Deep
        // sleep leaves the pad undriven, so the LED stays dark until the wake
        // reset lights it again. A no-op on a board with no status LED.
        crate::board::esp32c6::status_led::play_power_off();

        wait_for_non_wake_level(gpio, wake_high);

        esp_println::println!(
            "[fw-esp32c6] deep sleep; wake on gpio{gpio} {}",
            if wake_high { "high" } else { "low" }
        );

        // The pad keeps its RTC pull through sleep: pull toward the non-wake
        // level, exactly as the running button input did.
        let mut pin = unsafe { AnyPin::steal(gpio) };
        pin.rtcio_pullup(!wake_high);
        pin.rtcio_pulldown(wake_high);
        let level = if wake_high {
            WakeupLevel::High
        } else {
            WakeupLevel::Low
        };
        let mut pins: [(&mut dyn RtcPinWithResistors, WakeupLevel); 1] = [(&mut pin, level)];
        let ext1 = Ext1WakeupSource::new(&mut pins);
        rtc.sleep_deep(&[&ext1]);
    }
}

/// Block until the pin has read its non-wake level for [`SETTLE_MS`].
fn wait_for_non_wake_level(gpio: u8, wake_high: bool) {
    let pull = if wake_high { Pull::Down } else { Pull::Up };
    let input = Input::new(
        unsafe { AnyPin::steal(gpio) },
        InputConfig::default().with_pull(pull),
    );
    let at_wake_level = || {
        if wake_high {
            input.is_high()
        } else {
            input.is_low()
        }
    };
    let delay = Delay::new();
    let started = Instant::now();
    let settle = Duration::from_millis(SETTLE_MS);
    let log_every = Duration::from_millis(LOG_EVERY_MS);
    let mut quiet_since = (!at_wake_level()).then(Instant::now);
    let mut last_log = Instant::now();

    loop {
        if at_wake_level() {
            quiet_since = None;
            if last_log.elapsed() >= log_every {
                esp_println::println!(
                    "[fw-esp32c6] deep sleep waits for gpio{gpio} to be released ({} ms)",
                    started.elapsed().as_millis()
                );
                last_log = Instant::now();
            }
        } else {
            let since = *quiet_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= settle {
                return;
            }
        }
        delay.delay_millis(POLL_MS);
    }
}

fn gpio_number(address: &HwAddress) -> Result<u8, PowerError> {
    let raw = address
        .as_str()
        .strip_prefix("/gpio/")
        .ok_or_else(|| PowerError::msg(format!("power button is not on a GPIO: {address}")))?;
    let gpio = raw
        .parse::<u8>()
        .map_err(|_| PowerError::msg(format!("invalid GPIO address: {address}")))?;
    if gpio > 7 {
        return Err(PowerError::msg(format!(
            "gpio{gpio} is not an LP GPIO; the C6 wakes only on GPIO0-7"
        )));
    }
    Ok(gpio)
}
