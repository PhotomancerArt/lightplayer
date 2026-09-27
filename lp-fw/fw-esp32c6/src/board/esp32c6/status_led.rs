//! The board's status LED: booting, running, powering off.
//!
//! The patterns are `lpc_hardware::StatusLedState`'s, pure and host-tested
//! there; which pin, and which level lights it, is
//! `lpc_hardware::board_status_led_for`, keyed on the manifest in effect. This
//! is the half that owns the pin and the clock. A board with no status LED
//! (anything but the XIAO C6 today) gets no pin driven and no task.
//!
//! Deep sleep needs nothing more: the pad is not held, so it stops driving
//! when the chip sleeps, and the power-off pattern leaves it dark anyway.
//! Waking is a reset, and the reset boots into `Booting` again.

use core::cell::RefCell;

use critical_section::Mutex;
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::delay::Delay;
use esp_hal::gpio::{AnyPin, Level, Output, OutputConfig};
use lpc_hardware::{HwGateLevel, StatusLedState, board_status_led_for};

/// How often the task re-reads the pattern. The shortest step in any
/// pattern is 100 ms.
const TICK: Duration = Duration::from_millis(25);

static STATUS_LED: Mutex<RefCell<Option<StatusLed>>> = Mutex::new(RefCell::new(None));

/// Light the board's status LED in [`StatusLedState::Booting`] and spawn the
/// task that animates it. Call once, as soon as the manifest is known.
pub fn start(spawner: Spawner, board_id: &str) {
    let Some(led) = board_status_led_for(board_id) else {
        return;
    };
    let lit_level = match led.lit_level {
        HwGateLevel::Low => Level::Low,
        HwGateLevel::High => Level::High,
    };
    let state = StatusLedState::Booting;
    let lit = state.lit_at(0);
    // SAFETY: the pin is `reserved_reason`'d in the board's manifest (a
    // `lpc-hardware` test holds the two together), so no driver in this image
    // opens it; the board-quirk pins are stolen the same way.
    let pin = unsafe { AnyPin::steal(led.gpio) };
    let out = Output::new(pin, level_for(lit, lit_level), OutputConfig::default());
    critical_section::with(|cs| {
        STATUS_LED.borrow_ref_mut(cs).replace(StatusLed {
            out,
            lit_level,
            state,
            since: Instant::now(),
            lit,
        });
    });
    spawner.spawn(status_led_task().expect("status LED: spawn"));
    log::info!(
        "[fw-esp32c6] Status LED: GPIO{} (lit {})",
        led.gpio,
        if lit_level == Level::Low {
            "LOW"
        } else {
            "HIGH"
        }
    );
}

/// Switch the LED to `state`'s pattern, from its start. A no-op on a board
/// with no status LED, and when the LED already shows `state`.
pub fn show(state: StatusLedState) {
    critical_section::with(|cs| {
        if let Some(led) = STATUS_LED.borrow_ref_mut(cs).as_mut() {
            led.show(state, Instant::now());
        }
    });
}

/// Play [`StatusLedState::PoweringOff`] to its end, blocking: the power-off
/// path runs with the executor held, so the task cannot animate it. Leaves
/// the LED dark.
#[expect(dead_code, reason = "called from power.rs once PR #787 lands")]
pub fn play_power_off() {
    show(StatusLedState::PoweringOff);
    let Some(pattern_ms) = StatusLedState::PoweringOff.pattern_ms() else {
        return;
    };
    let delay = Delay::new();
    loop {
        let finished = critical_section::with(|cs| {
            STATUS_LED
                .borrow_ref_mut(cs)
                .as_mut()
                .is_none_or(|led| led.refresh(Instant::now()) >= pattern_ms)
        });
        if finished {
            return;
        }
        delay.delay_millis(TICK.as_millis() as u32);
    }
}

#[embassy_executor::task]
async fn status_led_task() {
    loop {
        critical_section::with(|cs| {
            if let Some(led) = STATUS_LED.borrow_ref_mut(cs).as_mut() {
                led.refresh(Instant::now());
            }
        });
        Timer::after(TICK).await;
    }
}

struct StatusLed {
    out: Output<'static>,
    lit_level: Level,
    state: StatusLedState,
    since: Instant,
    lit: bool,
}

impl StatusLed {
    fn show(&mut self, state: StatusLedState, now: Instant) {
        if state != self.state {
            self.state = state;
            self.since = now;
            self.refresh(now);
        }
    }

    /// Drive the pin to the pattern at `now`; returns the ms into the state.
    fn refresh(&mut self, now: Instant) -> u64 {
        let elapsed_ms = now.saturating_duration_since(self.since).as_millis();
        let lit = self.state.lit_at(elapsed_ms);
        if lit != self.lit {
            self.out.set_level(level_for(lit, self.lit_level));
            self.lit = lit;
        }
        elapsed_ms
    }
}

fn level_for(lit: bool, lit_level: Level) -> Level {
    match (lit, lit_level) {
        (true, level) => level,
        (false, Level::Low) => Level::High,
        (false, Level::High) => Level::Low,
    }
}
