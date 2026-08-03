//! MDIO hunt with the EMAC APLL 50 MHz clock RUNNING.
//!
//! Stage 0 proved the PHY is unclocked at rest (GPIO0 static), so the board
//! must expect the ESP32 to generate the RMII clock. A `TolerantPhy` makes
//! `Ethernet::new` succeed regardless of PHY discovery, keeping the APLL
//! output alive while we (a) let the EMAC probe MDC=23/MDIO=18 and (b)
//! bit-bang every other candidate pair.
//!
//! Default: clock out on GPIO17 (`EMAC_CLK_180`, the common WROOM-board
//! choice). `--features apll-out-16` for GPIO16 (`EMAC_CLK_OUT`).

#![no_std]
#![no_main]

use core::task::Context;

use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    ethernet::{
        clock::ApllClock,
        mac::{Duplex, LinkState, Speed},
        phy::{generic::GenericPhy, MdioBus, Phy, PhyError},
        Ethernet, EthernetDmaStorage, RmiiPinBundle,
    },
    gpio::{Flex, InputConfig, Pull},
};
use esp_println::println;
use net_bringup_classic::{identify, sweep};
use static_cell::ConstStaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

static STORAGE: ConstStaticCell<EthernetDmaStorage<4, 4>> =
    ConstStaticCell::new(EthernetDmaStorage::new());

/// Scans over the EMAC's own MDIO pins, then claims success so the driver
/// (and its APLL clock) stays alive.
struct TolerantPhy;

impl Phy for TolerantPhy {
    fn address(&self) -> u8 {
        0
    }

    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<(), PhyError> {
        println!("[emac] scanning its own MDC/MDIO pair under APLL:");
        let mut found = 0;
        for addr in 0u8..32 {
            let id1 = mdio.read(addr, 2);
            let id2 = mdio.read(addr, 3);
            if !(id1 == 0xFFFF && id2 == 0xFFFF) && !(id1 == 0 && id2 == 0) {
                println!(
                    "  EMAC HIT addr={} ID1={:#06x} ID2={:#06x} => {}",
                    addr,
                    id1,
                    id2,
                    identify(id1, id2)
                );
                found += 1;
            }
        }
        println!("[emac] pair scan done ({} hits); continuing regardless", found);
        let mut inner = GenericPhy::new_auto();
        match inner.init(mdio) {
            Ok(()) => println!("[emac] GenericPhy init OK"),
            Err(_) => println!("[emac] GenericPhy not found (expected); keeping clock alive"),
        }
        Ok(())
    }

    fn poll_link<M: MdioBus>(&mut self, _mdio: &mut M, _cx: Option<&mut Context<'_>>) -> LinkState {
        LinkState {
            up: false,
            speed: Speed::_100M,
            duplex: Duplex::Full,
        }
    }
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let delay = Delay::new();

    #[cfg(feature = "apll-out-16")]
    let clock = ApllClock::new(peripherals.GPIO16);
    #[cfg(not(feature = "apll-out-16"))]
    let clock = ApllClock::new(peripherals.GPIO17);

    println!(
        "=== mdio-hunt-apll: clock out on GPIO{} ===",
        if cfg!(feature = "apll-out-16") { 16 } else { 17 }
    );

    let eth = Ethernet::new(
        peripherals.ETH,
        STORAGE.take(),
        [0x02, 0x4C, 0x50, 0x00, 0x00, 0x02],
        TolerantPhy,
        RmiiPinBundle {
            clock,
            rxd0: peripherals.GPIO25,
            rxd1: peripherals.GPIO26,
            rx_dv: peripherals.GPIO27,
            txd0: peripherals.GPIO19,
            txd1: peripherals.GPIO22,
            tx_en: peripherals.GPIO21,
            mdc: peripherals.GPIO23,
            mdio: peripherals.GPIO18,
        },
    )
    .expect("Ethernet init failed even with TolerantPhy");
    println!("[emac] driver alive, APLL running");

    // Bit-bang the pins the EMAC didn't claim. GPIO0 stays untouched (it may
    // be fed by the CH340 auto-boot circuit); the clock pin is consumed.
    let mut pins: [(u8, Flex<'static>); 10] = [
        (2, Flex::new(peripherals.GPIO2)),
        (4, Flex::new(peripherals.GPIO4)),
        (5, Flex::new(peripherals.GPIO5)),
        (12, Flex::new(peripherals.GPIO12)),
        (13, Flex::new(peripherals.GPIO13)),
        (14, Flex::new(peripherals.GPIO14)),
        (15, Flex::new(peripherals.GPIO15)),
        #[cfg(feature = "apll-out-16")]
        (17, Flex::new(peripherals.GPIO17)),
        #[cfg(not(feature = "apll-out-16"))]
        (16, Flex::new(peripherals.GPIO16)),
        (32, Flex::new(peripherals.GPIO32)),
        (33, Flex::new(peripherals.GPIO33)),
    ];
    for (_, p) in pins.iter_mut() {
        p.set_input_enable(true);
        p.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        p.set_high();
        p.set_output_enable(false);
    }

    println!("--- sweep under APLL ---");
    let hits = sweep(&mut pins, None, &delay);
    println!("--- sweep done: {} hit(s) ---", hits);

    if hits == 0 {
        println!("--- enable hunts under APLL (high then low) ---");
        let candidates: &[u8] = &[12, 5, 4, 33, 32, 2, 15, 13, 14];
        for &e in candidates {
            let idx = pins.iter().position(|(n, _)| *n == e).unwrap();
            pins[idx].1.set_high();
            pins[idx].1.set_output_enable(true);
            delay.delay_micros(300_000);
            let h = sweep(&mut pins, Some(e), &delay);
            println!("  [GPIO{} high] {} hit(s)", e, h);
            pins[idx].1.set_low();
            delay.delay_micros(300_000);
            let h = sweep(&mut pins, Some(e), &delay);
            println!("  [GPIO{} low] {} hit(s)", e, h);
            pins[idx].1.set_output_enable(false);
        }
        println!("--- enable hunts done ---");
    }

    println!("=== mdio-hunt-apll finished ===");
    let _keep_alive = eth;
    loop {
        delay.delay_micros(1_000_000);
    }
}
