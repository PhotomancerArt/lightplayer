//! What the board looks like before anyone connects: its address, its name,
//! and the advertisement that carries them.
//!
//! - **Address:** a random *static* address derived from the base MAC, as in
//!   the spike, so the same board is the same device to a phone across boots
//!   (Bluefy's `getDevices()` finds it again with no chooser).
//! - **Name:** `LP-<short>`. `<short>` is the board's own name (the one a
//!   person gave it, `LpServer::device_name`), else the loaded project's
//!   folder, else the last four hex digits of the MAC, cut on a character
//!   to fit the advertising packet. It follows a rename and the loaded
//!   project with no reboot: the server loop's upkeep calls
//!   [`refresh_advertised_name`], and a change restarts advertising. The
//!   GAP device name (what a connected central reads) is fixed when the
//!   GATT server is built, so it is the board's name at boot
//!   ([`name_at_boot`]) and follows a rename at the next one.
//! - **Payload:** flags + complete local name in the advertisement; the NUS
//!   service UUID in the scan response (a 128-bit UUID does not fit beside a
//!   name in 31 bytes), which is what Studio's chooser filters on.

use core::cell::RefCell;
use core::fmt::Write as _;

use critical_section::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_hal::efuse;
use trouble_host::prelude::*;

use super::ble_task::{BleController, BleStackError};
use super::nus_service::{NUS_SERVICE_UUID_LE, NusServer};

/// Longest advertised name: 31-byte advertisement − flags (3) − the name
/// structure's own length and type bytes (2).
pub const ADV_NAME_MAX: usize = 26;
const PREFIX: &str = "LP-";
/// How often the upkeep looks at the loaded project.
const NAME_REFRESH_MS: u64 = 5_000;
/// How often to advertise. trouble-host defaults to 160 ms; every advertising
/// event is air time the ESP-NOW receiver loses (desk, 2026-09-24: 0.21 % RX
/// loss with BLE off; advertising at 160 ms 2.61 %, at 546.25 ms 0.98 %, at
/// 1022.5 ms 0.68 % — one 3-minute window each, same board, same peer, ~10 cm).
/// 546.25 ms is on Apple's list of recommended intervals.
const ADV_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_micros(546_250);

type AdvName = heapless::String<ADV_NAME_MAX>;

static NAME: Mutex<RefCell<AdvName>> = Mutex::new(RefCell::new(heapless::String::new()));
static NAME_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static LAST_REFRESH_MS: Mutex<RefCell<Option<u64>>> = Mutex::new(RefCell::new(None));

/// The random static address for this board: the base MAC, little-endian as
/// bt-hci stores it, with the top two bits set (a static address).
pub fn static_address() -> Address {
    let mac = efuse::base_mac_address();
    let mut addr = [0u8; 6];
    for (i, b) in mac.as_bytes().iter().rev().enumerate() {
        addr[i] = *b;
    }
    addr[5] |= 0xC0;
    Address::random(addr)
}

/// `LP-xxxx` from the last two base-MAC bytes: the name with no project, and
/// the GAP device name (fixed when the GATT server is built).
pub fn mac_name() -> AdvName {
    let mac = efuse::base_mac_address();
    let mac = mac.as_bytes();
    let mut name = AdvName::new();
    let _ = write!(name, "{PREFIX}{:02x}{:02x}", mac[4], mac[5]);
    name
}

/// Seed the advertised name with the board's own name (the identity file's,
/// read by the chip at boot), before [`super::start`]: the GAP device name
/// is built from it, and the first advertisement says it before the server
/// loop's first upkeep. `None` (never named) leaves the MAC name.
pub fn name_at_boot(name: Option<&str>) {
    let Some(name) = name else {
        return;
    };
    let name = project_name(name);
    critical_section::with(|cs| *NAME.borrow_ref_mut(cs) = name);
}

/// The name the GATT server is built with: the board's name at boot when it
/// has one ([`name_at_boot`]), else the MAC name.
pub(super) fn gap_name() -> AdvName {
    critical_section::with(|cs| {
        let name = NAME.borrow_ref(cs);
        if name.is_empty() {
            mac_name()
        } else {
            name.clone()
        }
    })
}

/// `LP-` + `project` (a project folder or the board's name), cut on a
/// character boundary to fit. Control characters are dropped: the board's
/// name is cleaned when stored, and a folder name never has any.
pub fn project_name(project: &str) -> AdvName {
    let mut name = AdvName::new();
    let _ = name.push_str(PREFIX);
    for c in project.chars().filter(|c| !c.is_control()) {
        if name.push(c).is_err() {
            break;
        }
    }
    name
}

/// Server-loop upkeep hook: every few seconds, make the advertised name
/// follow the board's own name, else the first loaded project, else the
/// MAC.
pub fn refresh_advertised_name(server: &lpa_server::LpServer, now_ms: u64) {
    let due = critical_section::with(|cs| {
        let mut last = LAST_REFRESH_MS.borrow_ref_mut(cs);
        let due = last.is_none_or(|at| now_ms.saturating_sub(at) >= NAME_REFRESH_MS);
        if due {
            *last = Some(now_ms);
        }
        due
    });
    if !due {
        return;
    }
    // The board's name first (held by the server: no flash read here); a
    // project's name is its folder's, the last component of its path.
    let named = server.device_name().map(project_name);
    let wanted = named
        .or_else(|| {
            server
                .project_manager()
                .list_loaded_projects()
                .first()
                .and_then(|loaded| {
                    loaded
                        .path
                        .as_str()
                        .rsplit('/')
                        .find(|part| !part.is_empty())
                        .map(project_name)
                })
        })
        .unwrap_or_else(mac_name);
    let changed = critical_section::with(|cs| {
        let mut name = NAME.borrow_ref_mut(cs);
        if *name == wanted {
            false
        } else {
            *name = wanted;
            true
        }
    });
    if changed {
        NAME_CHANGED.signal(());
    }
}

/// Resolves when the advertised name has changed since the last call.
pub async fn name_changed() {
    NAME_CHANGED.wait().await;
}

/// Advertise, connectable, until a central connects; the connection comes
/// back with the GATT server attached.
pub async fn advertise(
    peripheral: &mut Peripheral<'static, BleController, DefaultPacketPool>,
    server: &'static NusServer<'static>,
) -> Result<GattConnection<'static, 'static, DefaultPacketPool>, BleStackError> {
    let name = gap_name();
    let mut adv = [0u8; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut adv[..],
    )?;
    let mut scan = [0u8; 31];
    let scan_len = AdStructure::encode_slice(
        &[AdStructure::ServiceUuids128(&[NUS_SERVICE_UUID_LE])],
        &mut scan[..],
    )?;
    let advertiser = peripheral
        .advertise(
            &AdvertisementParameters {
                interval_min: ADV_INTERVAL,
                interval_max: ADV_INTERVAL,
                ..Default::default()
            },
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv[..adv_len],
                scan_data: &scan[..scan_len],
            },
        )
        .await?;
    log::info!("[ble] advertising as {name}");
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    Ok(conn)
}
