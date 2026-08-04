//! Bit-banged SMI (MDIO) pin hunt for the DOM-WLE-LAN v2.65.
//!
//! The EMAC scan found no PHY on the conventional MDC=23/MDIO=18 in any
//! clock topology, and MDC/MDIO are GPIO-matrix-routable — so this sweeps
//! candidate (MDC, MDIO) pin pairs with a software SMI master, reading PHY
//! ID registers at every address. Stage B repeats the sweep with each
//! candidate pin driven high first, to catch a gated oscillator or PHY
//! power-enable (WT32-ETH01 / Olimex-POE style).
//!
//! RMII data pins (19/21/22/25/26/27) and UART0 (1/3) are excluded. GPIO0
//! is the likely REF_CLK input — excluded from driving. All candidates are
//! only driven post-boot, so strapping pins are safe.

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Flex, InputConfig, Pull},
};
use esp_println::println;

esp_bootloader_esp_idf::esp_app_desc!();

fn identify(id1: u16, id2: u16) -> &'static str {
    match (id1, id2 & 0xFFF0) {
        (0x0007, 0xC0F0) => "LAN8720A",
        (0x0007, 0xC130) => "LAN8742A",
        (0x001C, 0xC810) => "RTL8201",
        (0x0243, 0x0C50) => "IP101",
        (0x0022, 0x1560) => "KSZ8081",
        _ => "unknown",
    }
}

struct Smi;

impl Smi {
    fn clock_bit(mdc: &mut Flex<'_>, mdio: &mut Flex<'_>, bit: bool, delay: &Delay) {
        mdc.set_low();
        if bit {
            mdio.set_high();
        } else {
            mdio.set_low();
        }
        delay.delay_micros(2);
        mdc.set_high();
        delay.delay_micros(2);
    }

    fn read_bit(mdc: &mut Flex<'_>, mdio: &Flex<'_>, delay: &Delay) -> bool {
        mdc.set_low();
        delay.delay_micros(2);
        mdc.set_high();
        delay.delay_micros(2);
        mdio.is_high()
    }

    /// One IEEE 802.3 clause-22 read frame.
    fn read(mdc: &mut Flex<'_>, mdio: &mut Flex<'_>, phy: u8, reg: u8, delay: &Delay) -> u16 {
        mdio.set_output_enable(true);
        for _ in 0..32 {
            Self::clock_bit(mdc, mdio, true, delay); // preamble
        }
        // ST=01 OP=10 (read)
        for &b in &[false, true, true, false] {
            Self::clock_bit(mdc, mdio, b, delay);
        }
        for i in (0..5).rev() {
            Self::clock_bit(mdc, mdio, (phy >> i) & 1 == 1, delay);
        }
        for i in (0..5).rev() {
            Self::clock_bit(mdc, mdio, (reg >> i) & 1 == 1, delay);
        }
        // Turnaround: release the line, PHY drives the second TA bit.
        mdio.set_output_enable(false);
        let _ = Self::read_bit(mdc, mdio, delay);
        let _ = Self::read_bit(mdc, mdio, delay);
        let mut value: u16 = 0;
        for _ in 0..16 {
            value = (value << 1) | Self::read_bit(mdc, mdio, delay) as u16;
        }
        mdc.set_low();
        value
    }
}

/// Sweep every ordered (MDC, MDIO) pair in `pins`, all 32 PHY addresses.
/// `skip` is a pin currently repurposed as an enable line.
fn sweep(pins: &mut [(u8, Flex<'static>)], skip: Option<u8>, delay: &Delay) -> u32 {
    let mut hits = 0;
    let n = pins.len();
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            let (ni, nj) = (pins[i].0, pins[j].0);
            if Some(ni) == skip || Some(nj) == skip {
                continue;
            }
            let (mdc, mdio) = if i < j {
                let (l, r) = pins.split_at_mut(j);
                (&mut l[i].1, &mut r[0].1)
            } else {
                let (l, r) = pins.split_at_mut(i);
                (&mut r[0].1, &mut l[j].1)
            };
            for addr in 0u8..32 {
                let id1 = Smi::read(mdc, mdio, addr, 2, delay);
                if id1 == 0xFFFF || id1 == 0x0000 || id1 == 0x7FFF {
                    continue;
                }
                // Require a stable re-read before believing it.
                let id1b = Smi::read(mdc, mdio, addr, 2, delay);
                if id1b != id1 {
                    continue;
                }
                let id2 = Smi::read(mdc, mdio, addr, 3, delay);
                println!(
                    "  HIT mdc=GPIO{} mdio=GPIO{} addr={} ID1={:#06x} ID2={:#06x} => {}",
                    ni, nj, addr, id1, id2, identify(id1, id2)
                );
                hits += 1;
            }
            // Leave the pair released for the next combination.
            let (a, b) = if i < j {
                let (l, r) = pins.split_at_mut(j);
                (&mut l[i].1, &mut r[0].1)
            } else {
                let (l, r) = pins.split_at_mut(i);
                (&mut r[0].1, &mut l[j].1)
            };
            a.set_output_enable(false);
            b.set_output_enable(false);
        }
    }
    hits
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let delay = Delay::new();

    println!("=== mdio-hunt: bit-banged SMI pin sweep (DOM-WLE-LAN) ===");

    let mut pins: [(u8, Flex<'static>); 14] = [
        // GPIO0 was excluded as "the clock input", but link LEDs blink with
        // no clock on GPIO0 — so the PHY self-clocks and GPIO0 is free to be
        // an SMI pin (static census is exactly an idle MDC/MDIO). Post-boot
        // driving is strap-safe.
        (0, Flex::new(peripherals.GPIO0)),
        (2, Flex::new(peripherals.GPIO2)),
        (4, Flex::new(peripherals.GPIO4)),
        (5, Flex::new(peripherals.GPIO5)),
        (12, Flex::new(peripherals.GPIO12)),
        (13, Flex::new(peripherals.GPIO13)),
        (14, Flex::new(peripherals.GPIO14)),
        (15, Flex::new(peripherals.GPIO15)),
        (16, Flex::new(peripherals.GPIO16)),
        (17, Flex::new(peripherals.GPIO17)),
        (18, Flex::new(peripherals.GPIO18)),
        (23, Flex::new(peripherals.GPIO23)),
        (32, Flex::new(peripherals.GPIO32)),
        (33, Flex::new(peripherals.GPIO33)),
    ];
    for (_, p) in pins.iter_mut() {
        p.set_input_enable(true);
        p.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        p.set_high(); // idle level once output-enabled
        p.set_output_enable(false);
    }

    // Stage 0: which pins are toggling? A LAN8720A in REF_CLK-out mode puts
    // (aliased) 50 MHz on GPIO0; a live RMII RX pair also toggles. Static
    // GPIO0 means the PHY is unclocked and the board expects APLL-out.
    println!("--- stage 0: transition census (10k samples/pin) ---");
    {
        let probe = |name: u8, p: &Flex<'_>| {
            let mut transitions = 0u32;
            let mut last = p.is_high();
            for _ in 0..10_000 {
                let now = p.is_high();
                if now != last {
                    transitions += 1;
                    last = now;
                }
            }
            if transitions > 0 {
                println!("  GPIO{}: {} transitions", name, transitions);
            }
        };
        for (n, p) in pins.iter() {
            probe(*n, p);
        }
        println!("--- stage 0 done (unlisted pins were static) ---");
    }

    println!("--- stage A: plain sweep ---");
    let hits = sweep(&mut pins, None, &delay);
    println!("--- stage A done: {} hit(s) ---", hits);

    if hits == 0 {
        println!("--- stage B: enable-pin hunt (drive one pin high, re-sweep) ---");
        let candidates = [16u8, 17, 12, 5, 4, 33, 32, 2, 15, 13, 14];
        for e in candidates {
            let idx = pins.iter().position(|(n, _)| *n == e).unwrap();
            pins[idx].1.set_high();
            pins[idx].1.set_output_enable(true);
            delay.delay_micros(300_000);
            let h = sweep(&mut pins, Some(e), &delay);
            println!("  [enable GPIO{} high] {} hit(s)", e, h);
            pins[idx].1.set_output_enable(false);
        }
        println!("--- stage B done ---");

        println!("--- stage B2: active-LOW enable hunt (drive one pin low, re-sweep) ---");
        for e in candidates {
            let idx = pins.iter().position(|(n, _)| *n == e).unwrap();
            pins[idx].1.set_low();
            pins[idx].1.set_output_enable(true);
            delay.delay_micros(300_000);
            let h = sweep(&mut pins, Some(e), &delay);
            println!("  [enable GPIO{} low] {} hit(s)", e, h);
            pins[idx].1.set_output_enable(false);
            pins[idx].1.set_high();
        }
        println!("--- stage B2 done ---");
    }

    println!("=== mdio-hunt finished ===");
    loop {
        delay.delay_micros(1_000_000);
    }
}
