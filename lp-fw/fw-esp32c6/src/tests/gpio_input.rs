//! ESP32-C6 `gpio-input`: the payload's device half.
//!
//! The edge table, the quadrature decoder, the record shapes and the renderer
//! that turns the table into a `--pin-script` live in
//! `fw_checks::checks::gpio_input`, where they are `no_std` and know nothing
//! about this chip. What stays here is what cannot leave, and it is four
//! kinds of chip fact:
//!
//! 1. **Which pad is which.** GPIO20 is the silkscreen's D9, GPIO0/GPIO1 are
//!    D0/D1, GPIO2 is D2. The numbers are in the shared module; that they
//!    exist on this board is here.
//! 2. **The product's button driver.** `Esp32GpioButtonDriver` is
//!    `fw-esp32c6`'s, over `esp_hal::gpio::Input` and `lpc-hardware`'s
//!    `ButtonDebouncer`. This payload **calls** it. It does not re-implement
//!    it, and it does not modify it — the whole point of routing the button
//!    through the shipped driver is that a pass here is a pass for the code
//!    the product runs.
//! 3. **The GPIO interrupt.** `Io::set_interrupt_handler`, `Flex::listen`,
//!    and a handler that reads `GPIO.status` / `GPIO.in_` and clears
//!    `GPIO.status_w1tc`. An ISA and a register map, both of them here.
//! 4. **The self-loop.** Enabling a pad's output driver while its input
//!    buffer stays on is `Flex::set_output_enable`, which is a linker's-eye
//!    view of the same pad the product driver is reading.
//!
//! # Why the harness is a `src/tests/` entry point at all
//!
//! Every payload in this crate that can be a `fw-checks` module and one
//! feature line is exactly that. This one cannot: its subject **is** the
//! product's button driver, which lives in `hardware/button.rs` and needs
//! `esp_hal::gpio`, `lpc_hardware::HwRegistry` and the board manifest. A
//! `fw-checks` module that could call it would have dragged the chip crate
//! and the product's hardware registry into a crate whose README's first rule
//! is "keep it cheap". So this follows `cycle_probe.rs`'s precedent: the
//! portable half is a `fw-checks` module, the chip-bound half is here, and
//! `main.rs` gains two cfg-gated lines.
//!
//! # The self-loop, and why it does not disturb what it measures
//!
//! `Input::new` — which the product driver calls — resets the pad, turns the
//! input buffer on and turns the **output driver off**. That last part is
//! what a self-loop has to undo, and undoing it is all this harness does: a
//! second `Flex` handle on the same pad sets a level, enables the output
//! driver, and puts the input configuration back exactly as `Input::new` left
//! it (buffer on, pull-up). The product's `Input` reads `GPIO.in_` bit 20
//! either way — it does not care which handle configured the pad — so the
//! read path under test is the shipped one, byte for byte.
//!
//! On an emulated configuration none of that happens: the pad is left as the
//! product driver configured it and the level arrives from outside.
//!
//! # What the handler does, and what it must not do
//!
//! `Io::set_interrupt_handler` installs a **user** GPIO handler, and esp-hal
//! is explicit that a user handler owns the clearing: its wrapper reads the
//! status word, calls us, and then only handles the pins an async driver is
//! waiting on. So the handler here writes `status_w1tc` itself, for its own
//! two pads and no others — the bits it did not set are not its to clear.
//!
//! It is `#[ram]`, and it does the least it can: read two levels, run the
//! decoder, stamp the microsecond, put a completed detent in a slot. No
//! allocation, no logging, no waiting. The records are printed by the main
//! loop, which drains the slots.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

use critical_section::Mutex;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{AnyPin, Event, Flex, Input, InputConfig, Io, Level, Pull};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::{handler, ram};
use fw_checks::checks::gpio_input::{
    ARMED_MARKER, BUTTON_GPIO, BUTTON_SAMPLES, DONE_MARKER, DRIVE_SELECT_GPIO, Detent, Drive,
    ENCODER_A_GPIO, ENCODER_B_GPIO, POLL_MS, Pad, Quadrature, RUN_END_US, SCRIPT, emit_button,
    emit_encoder,
};
use lpc_hardware::{
    ButtonConfig, ButtonDriver, ButtonEventKind, HwRegistry, default_esp32c6_hardware_manifest,
};
use log::info;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::hardware::button::Esp32GpioButtonDriver;
use crate::logger;
// Through the module rather than the re-export, for `cycle_probe.rs`'s
// reason: `serial::Esp32UsbSerialIo` is gated to a named list of harnesses and
// adding a name to that list would be an edit this phase does not need.
use crate::serial::usb_serial::Esp32UsbSerialIo;

/// The endpoint the product's registry knows GPIO20 by. `test_button`'s
/// spelling, and the fact this payload migrates: "D9/GPIO20 with an internal
/// pull-up and a normally-open button to GND".
const BUTTON_ENDPOINT: &str = "button:local:D9";

/// How many detents a run can decode before the main loop drains them.
///
/// The script turns four; the slots are sized well past that so that a
/// machine which fired spurious edges would fill them and be *visible* in the
/// record count rather than silently dropping the difference.
const DETENT_SLOTS: usize = 16;

/// The pads' interrupt mask: the two the encoder listens on, and nothing
/// else. The handler clears exactly these bits.
const ENCODER_MASK: u32 = (1 << ENCODER_A_GPIO) | (1 << ENCODER_B_GPIO);

/// What the interrupt handler owns.
struct EncoderIsr {
    decoder: Quadrature,
    /// Position, direction, and the microsecond it was decoded at.
    slots: [(i32, bool, u64); DETENT_SLOTS],
    /// How many slots the handler has filled.
    written: usize,
    /// Edges seen, whether or not they completed a detent. On the record only
    /// as a diagnostic in the closing line — a detent count that disagrees
    /// with `edges / 4` is the shape a missed edge takes.
    edges: u32,
}

static ENCODER: Mutex<RefCell<EncoderIsr>> = Mutex::new(RefCell::new(EncoderIsr {
    decoder: Quadrature::new(),
    slots: [(0, false, 0); DETENT_SLOTS],
    written: 0,
    edges: 0,
}));

/// The GPIO interrupt, on both edges of both encoder channels.
///
/// This is the path the encoder reader cannot do without: no polling
/// fallback exists, so a machine whose `pin[n].int_ena` / `int_type` /
/// `status` / `pcpu_int` chain does not work produces no encoder records at
/// all rather than late ones.
#[ram]
#[handler]
fn gpio_interrupt() {
    let gpio = esp_hal::peripherals::GPIO::regs();
    let pending = gpio.status().read().bits() & ENCODER_MASK;
    if pending == 0 {
        return;
    }
    let levels = gpio.in_().read().bits();
    let a = levels & (1 << ENCODER_A_GPIO) != 0;
    let b = levels & (1 << ENCODER_B_GPIO) != 0;
    // Read before the critical section: the microsecond clock is SYSTIMER and
    // reading it is a load, but a handler should not hold a lock across
    // anything it does not have to.
    let now_us = Instant::now().as_micros();
    critical_section::with(|cs| {
        let mut isr = ENCODER.borrow_ref_mut(cs);
        isr.edges = isr.edges.saturating_add(1);
        let decoded = isr.decoder.edge(a, b);
        if let Some(detent) = decoded {
            let n = isr.written;
            if n < DETENT_SLOTS {
                isr.slots[n] = (detent.position, detent.cw, now_us);
                isr.written = n + 1;
            }
        }
    });
    // Ours, and only ours. The bits this handler did not set are not its to
    // clear, and esp-hal's wrapper is explicit that a user handler owns this.
    gpio.status_w1tc().write(|w| unsafe { w.bits(pending) });
}

/// The pads this run may drive, when it is the one driving.
struct Pads<'d> {
    /// `None` on an externally driven run: the button pad is left exactly as
    /// the product driver configured it.
    button: Option<Flex<'d>>,
    a: Flex<'d>,
    b: Flex<'d>,
}

impl Pads<'_> {
    fn drive(&mut self, pad: Pad, level: bool) {
        let level = if level { Level::High } else { Level::Low };
        match pad {
            Pad::Button => {
                if let Some(pin) = self.button.as_mut() {
                    pin.set_level(level);
                }
            }
            Pad::EncoderA => self.a.set_level(level),
            Pad::EncoderB => self.b.set_level(level),
        }
    }
}

/// Wait until `deadline_us` after arming, driving on the way there every
/// scripted edge that falls before it.
///
/// The one place the two sides differ in code rather than in configuration:
/// with `self_loop` false this drives nothing and only waits, because
/// something outside is putting the levels on the pads.
async fn advance_to(
    arm: Instant,
    deadline_us: u64,
    next_edge: &mut usize,
    pads: &mut Pads<'_>,
    self_loop: bool,
) {
    while self_loop
        && *next_edge < SCRIPT.len()
        && u64::from(SCRIPT[*next_edge].at_us) <= deadline_us
    {
        let edge = SCRIPT[*next_edge];
        Timer::at(arm + Duration::from_micros(u64::from(edge.at_us))).await;
        pads.drive(edge.pad, edge.level);
        *next_edge += 1;
    }
    Timer::at(arm + Duration::from_micros(deadline_us)).await;
}

/// Print every detent the handler has finished with, in the order it decoded
/// them, and return the new read cursor.
fn drain_detents(arm_us: u64, read: &mut usize) {
    loop {
        let next = critical_section::with(|cs| {
            let isr = ENCODER.borrow_ref(cs);
            (*read < isr.written).then(|| isr.slots[*read])
        });
        let Some((position, cw, at_us)) = next else {
            return;
        };
        *read += 1;
        emit_encoder(Detent { position, cw }, at_us.saturating_sub(arm_us));
    }
}

pub async fn run_gpio_input(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt, usb_device, _gpio18, _flash, _gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);

    let usb_serial = UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    logger::set_log_serial(serial_io_shared);
    logger::init(logger::log_write_bytes);

    Timer::after(Duration::from_millis(100)).await;

    // The transcript header, first, before any record.
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "gpio-input",
            chip: "esp32c6",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );

    // Which side is driving? The pad answers, and the answer is stable for
    // the whole run: an input with a pull-down that something outside is
    // holding high means an outside driver is present. Nothing is wired to
    // this pad on the desk board, so silicon always reads low and self-loops.
    let select = Input::new(
        unsafe { AnyPin::steal(DRIVE_SELECT_GPIO) },
        InputConfig::default().with_pull(Pull::Down),
    );
    // The pull needs a moment against the pad's capacitance before the first
    // read means anything.
    Timer::after(Duration::from_millis(5)).await;
    let drive = if select.is_high() {
        Drive::External
    } else {
        Drive::SelfLoop
    };
    let self_loop = drive == Drive::SelfLoop;

    // The encoder pads. Input always; output as well when this run is the one
    // driving them, resting at the Gray sequence's (0, 0).
    let mut enc_a = Flex::new(unsafe { AnyPin::steal(ENCODER_A_GPIO) });
    let mut enc_b = Flex::new(unsafe { AnyPin::steal(ENCODER_B_GPIO) });
    for (pin, pad) in [
        (&mut enc_a, Pad::EncoderA),
        (&mut enc_b, Pad::EncoderB),
    ] {
        if self_loop {
            pin.set_level(if pad.rest_level() {
                Level::High
            } else {
                Level::Low
            });
            pin.set_output_enable(true);
        } else {
            pin.apply_input_config(&InputConfig::default().with_pull(Pull::Down));
        }
        pin.set_input_enable(true);
    }

    // The handler goes in before anything listens, so that no edge can arrive
    // while esp-hal's single-shot default handler is still bound.
    let mut io = Io::new(unsafe { esp_hal::peripherals::IO_MUX::steal() });
    io.set_interrupt_handler(gpio_interrupt);
    enc_a.listen(Event::AnyEdge);
    enc_b.listen(Event::AnyEdge);

    // The button, through the product's own driver: its registry, its
    // manifest, its endpoint, its `Input`, its debouncer.
    let hardware_registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
    let button_driver = Esp32GpioButtonDriver::new(hardware_registry);
    let button_endpoint = button_driver
        .endpoints()
        .into_iter()
        .find(|endpoint| endpoint.spec().as_str() == BUTTON_ENDPOINT)
        .expect("D9/GPIO20 button endpoint exists");
    let mut button = button_driver
        .open(button_endpoint.id(), ButtonConfig::default())
        .expect("D9/GPIO20 button opens");

    // The self-loop's other half: put the output driver back on the pad the
    // product driver just turned it off on, and restore the input
    // configuration it set. `Input::new` did `set_output_enable(false)`,
    // `set_input_enable(true)` and a pull-up; this is that, plus the output.
    let button_pad = self_loop.then(|| {
        let mut pin = Flex::new(unsafe { AnyPin::steal(BUTTON_GPIO) });
        pin.set_level(Level::High);
        pin.set_output_enable(true);
        pin.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        pin.set_input_enable(true);
        pin
    });
    let mut pads = Pads {
        button: button_pad,
        a: enc_a,
        b: enc_b,
    };

    info!(
        "[gpio-input] drive={} button=gpio{BUTTON_GPIO} encoder=gpio{ENCODER_A_GPIO}/gpio{ENCODER_B_GPIO} select=gpio{DRIVE_SELECT_GPIO} poll_ms={POLL_MS} samples={BUTTON_SAMPLES}",
        drive.as_str()
    );
    // Everything the emulated side's script does is timed from this line
    // reaching the host, so it is the last thing printed before the clock
    // starts.
    info!("{ARMED_MARKER}");
    let arm = Instant::now();
    let arm_us = arm.as_micros();
    let mut next_edge = 0usize;
    let mut read = 0usize;

    // Phase one: the button, on its own sample grid.
    //
    // `now_ms` handed to the debouncer is the sample INDEX times the period,
    // not a clock reading — see the shared module for why. The clock is still
    // real: `advance_to` sleeps to an absolute deadline, so sample k happens
    // at arming plus k periods of guest time however long the work took.
    for k in 0..BUTTON_SAMPLES {
        let at_us = u64::from(k) * u64::from(POLL_MS) * 1_000;
        advance_to(arm, at_us, &mut next_edge, &mut pads, self_loop).await;
        if let Some(event) = button.poll(u64::from(k) * u64::from(POLL_MS)) {
            emit_button(
                matches!(event.kind(), ButtonEventKind::Pressed),
                arm.elapsed().as_micros(),
                k + 1,
            );
        }
    }

    // Phase two: the encoder's edges, and the slots the handler fills. The
    // tick is finer than the 5 ms between transitions, so a detent is printed
    // in the same order it was decoded and long before the sentinel.
    let mut tick_us = u64::from(BUTTON_SAMPLES) * u64::from(POLL_MS) * 1_000;
    while tick_us <= RUN_END_US {
        advance_to(arm, tick_us, &mut next_edge, &mut pads, self_loop).await;
        drain_detents(arm_us, &mut read);
        tick_us += 2_000;
    }
    drain_detents(arm_us, &mut read);

    let edges = critical_section::with(|cs| ENCODER.borrow_ref(cs).edges);
    info!("[gpio-input] detents={read} edges={edges}");
    info!("{DONE_MARKER}");

    // The pads stay configured and the select pin stays alive: dropping
    // either would reset a pad the transcript has just made claims about.
    let _ = select;
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
