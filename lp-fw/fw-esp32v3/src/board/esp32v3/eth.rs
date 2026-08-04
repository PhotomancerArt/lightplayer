//! DOM-WLE-LAN (v2.65) Ethernet interface provider: EMAC + RMII + fixed-link
//! PHY, `net-eth` feature only.
//!
//! Everything board-specific about Ethernet lives in this file (plus the pin
//! selection in [`super::init::EthPeripherals`]); the chip-agnostic stack glue
//! is `fw_esp32_common::net`. The configuration is not designed here — it is
//! the ONE configuration the M1 bench spike proved on this exact board
//! (`spikes/net-bringup-classic`, G1 report in the networking roadmap):
//!
//! - **Clock: APLL synthesized on GPIO17, output topology.** The only
//!   working choice. There is no oscillator feeding GPIO0 on this board, and
//!   the GPIO16 output phase fails — GPIO17 (`EMAC_CLK_OUT_180`) is what the
//!   PHY's REF_CLK trace is wired to. ⚠️ APLL mode is incompatible with
//!   wifi/ESP-NOW; on this image that costs nothing (the radio was retired
//!   with the standalone-workspace split, see `main.rs` module docs), but it
//!   is a real fork for any future radio-capable classic carrier.
//! - **No SMI.** See [`FixedLinkPhy`].
//! - **RMII data pins fixed by silicon:** RXD0=25 RXD1=26 CRS_DV=27 TXD0=19
//!   TXD1=22 TX_EN=21 (declared in `init.rs`, where the singletons are).

use esp_hal::ethernet::{
    Ethernet, EthernetDmaStorage, RmiiPinBundle,
    clock::ApllClock,
    mac::{Duplex, LinkState, Speed},
    phy::{MdioBus, Phy, PhyError},
};
use static_cell::ConstStaticCell;

use super::init::EthPeripherals;

/// The concrete driver type the embassy tasks in `main.rs` are declared
/// against (`#[embassy_executor::task]` functions cannot be generic).
pub type EthDriver = Ethernet<'static, esp_hal::Async, FixedLinkPhy>;

/// No-SMI operation: the DOM-WLE-LAN wires no management bus (every pin pair
/// was swept, twice, clocked and unclocked — nothing answers), and the
/// LAN8720A negotiates link on its own (jack LEDs). Assume strap defaults:
/// 100M full duplex after autoneg.
///
/// Copied from the bench spike (`spikes/net-bringup-classic`, feature
/// `fixed-link`), which is where that sweep lives. Consequence to keep in
/// mind: the MAC-side link state is *asserted*, never observed — with no
/// cable the stack simply never gets a DHCP lease, and [`NetStatus`'s]
/// (`fw_esp32_common::net::NetStatus`) `up` flag is defined as "has an IPv4
/// config" for exactly this reason.
pub struct FixedLinkPhy;

impl Phy for FixedLinkPhy {
    fn address(&self) -> u8 {
        0
    }

    fn init<M: MdioBus>(&mut self, _mdio: &mut M) -> Result<(), PhyError> {
        // Runs inside `Ethernet::new`, which `main.rs` calls after
        // `logger::init` — so this reaches the host over the normal wire.
        log::info!("net: fixed-link PHY: no SMI on this board, assuming 100M/full");
        Ok(())
    }

    fn poll_link<M: MdioBus>(
        &mut self,
        _mdio: &mut M,
        _cx: Option<&mut core::task::Context<'_>>,
    ) -> LinkState {
        LinkState {
            up: true,
            speed: Speed::_100M,
            duplex: Duplex::Full,
        }
    }
}

/// EMAC DMA rings: 4 RX + 4 TX slots, ~1.5 KiB buffer each ≈ **12.4 KiB of
/// `.bss`**.
///
/// The spike ran `<10, 10>` (≈31 KiB) because it was alone on the chip. This
/// static is not: it lands in `dram_seg`, which is zero-sum with `.stack`
/// (see `HEAP_SIZE`'s doc in `main.rs` — at the 110 KB heap setting the whole
/// image has ~47 KiB of stack left). P2's traffic is one DHCP exchange plus
/// the later phases' control-plane chatter — light and latency-tolerant, so
/// 4 slots per direction is plenty of burst absorption; a dropped frame under
/// a burst costs a retransmit, not correctness. Revisit with measurements at
/// P5, which is where the feature's cost is formally gated.
static DMA_STORAGE: ConstStaticCell<EthernetDmaStorage<4, 4>> =
    ConstStaticCell::new(EthernetDmaStorage::new());

/// The EMAC MAC address, derived from the factory eFuse base MAC.
///
/// esp-hal's `efuse::interface_mac_address` only derives radio interfaces
/// (Station/AP/Bluetooth) — there is no Ethernet variant on this chip family
/// in the pinned rev. ESP-IDF's convention (base+3) assumes the "four
/// universal MACs" eFuse setting, which we would have to read to trust. So:
/// take the base MAC (unique per device) and set the locally-administered
/// bit, which by construction cannot collide with the factory station MAC —
/// and on this board the APLL clock excludes the radio anyway, so no second
/// interface can coexist to collide with. Revisit if a radio-capable
/// Ethernet carrier ever appears.
fn ethernet_mac() -> [u8; 6] {
    let base = esp_hal::efuse::base_mac_address();
    let mut mac: [u8; 6] = base
        .as_bytes()
        .try_into()
        .expect("efuse base MAC is EUI-48");
    mac[0] |= 0x02;
    mac
}

/// Builds the async EMAC driver for the DOM-WLE-LAN.
///
/// Returns `Err` rather than panicking for the same reason the RMT path in
/// `main.rs` does: a failure here costs the board its network, not its boot,
/// and a board that boots and renders without an IP is strictly more
/// diagnosable than one that reset-loops.
pub fn create_driver(parts: EthPeripherals) -> Result<EthDriver, esp_hal::ethernet::Error> {
    let mac = ethernet_mac();
    log::info!(
        "net: mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} (efuse base, locally administered)",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5]
    );
    Ethernet::new(
        parts.eth,
        DMA_STORAGE.take(),
        mac,
        FixedLinkPhy,
        RmiiPinBundle {
            clock: ApllClock::new(parts.clk_out),
            rxd0: parts.rxd0,
            rxd1: parts.rxd1,
            rx_dv: parts.rx_dv,
            txd0: parts.txd0,
            txd1: parts.txd1,
            tx_en: parts.tx_en,
            mdc: parts.mdc,
            mdio: parts.mdio,
        },
    )
    .map(|driver| driver.into_async())
}
