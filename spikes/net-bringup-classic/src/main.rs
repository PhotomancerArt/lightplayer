//! M1 bench spike: Ethernet bring-up + PHY identification on the Zook-copy
//! DOM-Z-102-family classic ESP32.
//!
//! Phases, all reported over UART0:
//!   1. MDIO scan (addresses 0–31, registers 2/3 = PHY ID1/ID2) — identifies
//!      the PHY part and proves the clock topology powers the MDIO slave.
//!   2. embassy-net + DHCP over the EMAC.
//!   3. TCP echo server on :7777 (`nc <ip> 7777`), heap numbers every 10 s.
//!
//! Clock topology is unknown for this board; default build assumes the common
//! external-oscillator-into-GPIO0 wiring (LAN8720 module convention). If the
//! scan finds no PHY, retry with `--features osc-en-16` (WT32-ETH01-style
//! gated oscillator), then `--features apll-out-16` / `apll-out-17` (ESP32
//! synthesizes the 50 MHz — NOTE: APLL mode is incompatible with wifi/ESP-NOW,
//! which would be a load-bearing G1 fact).
//!
//! RMII data pins are fixed by silicon: RXD0=25 RXD1=26 CRS_DV=27 TXD0=19
//! TXD1=22 TX_EN=21. MDC/MDIO follow the near-universal 23/18 convention —
//! GPIO18 is also DATA-1 on the DOM terminals, which is exactly the collision
//! the G1 report must confirm or refute.

#![no_std]
#![no_main]

use core::task::{Context, Poll};

use embassy_executor::Spawner;
use embassy_net::{tcp::TcpSocket, Runner, StackResources};
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    ethernet::{
        mac::{Duplex, LinkState, Speed},
        phy::{generic::GenericPhy, MdioBus, Phy, PhyError},
        Ethernet, EthernetDmaStorage, RmiiPinBundle,
    },
    interrupt::software::SoftwareInterruptControl,
    rng::Rng,
    timer::timg::TimerGroup,
};
#[cfg(any(feature = "osc-en-16", feature = "phy-reset-5"))]
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_println::println;
use futures_util::FutureExt;
use static_cell::{ConstStaticCell, StaticCell};

esp_bootloader_esp_idf::esp_app_desc!();

/// Locally-administered MAC for the spike; the real firmware will derive from efuse.
const MAC_ADDR: [u8; 6] = [0x02, 0x4C, 0x50, 0x00, 0x00, 0x01];

static STORAGE: ConstStaticCell<EthernetDmaStorage<10, 10>> =
    ConstStaticCell::new(EthernetDmaStorage::new());
static STACK_RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();

type EthDriver = Ethernet<'static, esp_hal::Async, ScanPhy>;

/// Wraps [`GenericPhy`]: on `init`, first scans every MDIO address and prints
/// ID1/ID2 so the G1 report can name the part, then defers to the generic
/// driver. Link polling is embassy-timer-paced (copied from the esp-hal
/// embassy_ethernet example) instead of GenericPhy's busy loop.
struct ScanPhy {
    phy: GenericPhy,
    timer: Timer,
    cached_link_state: LinkState,
}

impl ScanPhy {
    fn new_auto() -> Self {
        Self {
            phy: GenericPhy::new_auto(),
            timer: Timer::after_ticks(0),
            cached_link_state: LinkState {
                up: false,
                speed: Speed::_100M,
                duplex: Duplex::Full,
            },
        }
    }
}

fn identify(id1: u16, id2: u16) -> &'static str {
    // OUI-derived IDs for the usual suspects on LED-controller boards.
    match (id1, id2 & 0xFFF0) {
        (0x0007, 0xC0F0) => "LAN8720A (SMSC/Microchip)",
        (0x0007, 0xC130) => "LAN8742A (SMSC/Microchip)",
        (0x001C, 0xC810) => "RTL8201 (Realtek)",
        (0x0243, 0x0C50) => "IP101 (IC Plus)",
        (0x0022, 0x1560) => "KSZ8081 (Micrel/Microchip)",
        _ => "unknown — record raw IDs in the G1 report",
    }
}

impl Phy for ScanPhy {
    fn address(&self) -> u8 {
        self.phy.address()
    }

    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<(), PhyError> {
        println!("=== MDIO scan (reg2/reg3 = PHY ID1/ID2) ===");
        let mut found = 0u32;
        for addr in 0u8..32 {
            let id1 = mdio.read(addr, 2);
            let id2 = mdio.read(addr, 3);
            // 0xFFFF/0x0000 on both = nothing driving MDIO at this address.
            if !(id1 == 0xFFFF && id2 == 0xFFFF) && !(id1 == 0 && id2 == 0) {
                println!(
                    "  addr {:2}: ID1={:#06x} ID2={:#06x}  => {}",
                    addr,
                    id1,
                    id2,
                    identify(id1, id2)
                );
                found += 1;
            }
        }
        if found == 0 {
            println!("  NO PHY RESPONDED — wrong clock topology or PHY held in reset.");
            println!("  Retry order: --features osc-en-16, apll-out-16, apll-out-17.");
        }
        println!("=== scan done ({} responder(s)) ===", found);
        self.phy.init(mdio)
    }

    fn poll_link<M: MdioBus>(&mut self, mdio: &mut M, cx: Option<&mut Context<'_>>) -> LinkState {
        if let Some(cx) = cx {
            if matches!(self.timer.poll_unpin(cx), Poll::Pending) {
                return self.cached_link_state;
            }
            self.timer = Timer::after(Duration::from_millis(500));
            let _ = self.timer.poll_unpin(cx);
        }
        self.cached_link_state = self.phy.poll_link(mdio, None);
        self.cached_link_state
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, EthDriver>) {
    runner.run().await
}

#[embassy_executor::task]
async fn heap_report() {
    loop {
        Timer::after(Duration::from_secs(10)).await;
        println!(
            "[heap] used={} free={}",
            esp_alloc::HEAP.used(),
            esp_alloc::HEAP.free()
        );
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(size: 72 * 1024);

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!("net-bringup-classic spike; features: osc-en-16={} apll-out-16={} apll-out-17={} phy-reset-5={}",
        cfg!(feature = "osc-en-16"), cfg!(feature = "apll-out-16"),
        cfg!(feature = "apll-out-17"), cfg!(feature = "phy-reset-5"));

    // WT32-ETH01 convention: the 50 MHz oscillator is gated by GPIO16.
    #[cfg(feature = "osc-en-16")]
    let _osc_en = Output::new(peripherals.GPIO16, Level::High, OutputConfig::default());
    #[cfg(feature = "osc-en-16")]
    Timer::after(Duration::from_millis(50)).await;

    // Optional PHY hardware reset on GPIO5 (ESP32-Ethernet-Kit convention).
    // Off by default: GPIO5 wiring on this board is unknown and it is a
    // strapping pin — only enable once the photo/schematic says so.
    #[cfg(feature = "phy-reset-5")]
    {
        let mut phy_reset = Output::new(peripherals.GPIO5, Level::Low, OutputConfig::default());
        Timer::after(Duration::from_millis(100)).await;
        phy_reset.set_high();
        Timer::after(Duration::from_millis(300)).await;
        core::mem::forget(phy_reset);
    }

    #[cfg(feature = "apll-out-16")]
    let clock = esp_hal::ethernet::clock::ApllClock::new(peripherals.GPIO16);
    #[cfg(feature = "apll-out-17")]
    let clock = esp_hal::ethernet::clock::ApllClock::new(peripherals.GPIO17);
    #[cfg(not(any(feature = "apll-out-16", feature = "apll-out-17")))]
    let clock = esp_hal::ethernet::clock::ExternalRefClock::new(peripherals.GPIO0);

    let eth: EthDriver = Ethernet::new(
        peripherals.ETH,
        STORAGE.take(),
        MAC_ADDR,
        ScanPhy::new_auto(),
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
    .expect("Ethernet init failed")
    .into_async();

    let config = embassy_net::Config::dhcpv4(Default::default());
    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (stack, runner) = embassy_net::new(
        eth,
        config,
        STACK_RESOURCES.init(StackResources::<4>::new()),
        seed,
    );

    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(heap_report().unwrap());

    println!("Waiting for DHCP lease…");
    stack.wait_config_up().await;
    if let Some(cfg) = stack.config_v4() {
        println!("IP:      {}", cfg.address);
        println!("Gateway: {:?}", cfg.gateway);
        println!("Echo server on port 7777 — test with: nc {} 7777", cfg.address.address());
    }

    let mut rx_buffer = [0u8; 1536];
    let mut tx_buffer = [0u8; 1536];
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(30)));
        if let Err(e) = socket.accept(7777).await {
            println!("[echo] accept error: {:?}", e);
            continue;
        }
        println!("[echo] client connected");
        let mut buf = [0u8; 512];
        loop {
            match socket.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if socket.write(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        println!("[echo] client gone");
        socket.close();
        Timer::after(Duration::from_millis(50)).await;
    }
}
