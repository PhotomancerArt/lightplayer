//! Sim identity and its sidecar: what makes a sim a remembered device.
//!
//! A sim is a device with a link and a record — no new flow, no `is_sim`
//! anywhere in the fold. Two facts make that work, and this module owns
//! both.
//!
//! # The identity is minted, and it is shaped like silicon's
//!
//! Studio mints a **locally administered** MAC (the `0x02` bit set and the
//! multicast `0x01` bit cleared on octet 0 — the IEEE range reserved for
//! exactly this) and derives the uid from it with the same
//! [`HardwareId::device_uid`] derivation a real board's efuse MAC goes
//! through. Those derivation bytes are a G1-approved contract: reuse, never
//! re-derive. The registry row therefore records `hardware_id:
//! "efuse:<mac>"` like any board, and `rekey_or_merge`, the identity line
//! and every join stay exactly as they are (Q2).
//!
//! Random bytes are the caller's (sans-IO): the web shell hands in
//! `crypto.getRandomValues`, tests hand in fixed bytes, and this module has
//! no opinion about where entropy comes from.
//!
//! # The sidecar is the sole "this is a sim" fact
//!
//! `/device-sims/<uid>.json`, beside `/device-frames/<uid>.json`, with the
//! same posture and its own [`SIM_RECORD_VERSION`]:
//!
//! - **Absence means "not a sim"** — a board that never had one simply is
//!   not one, which is every board in every existing library.
//! - **Unreadable or foreign-version reads as absent** (`debug!`, never a
//!   user-facing error, never a migration).
//! - **Cache-like in its failure modes, user-owned in its lifetime.** Unlike
//!   a frame snapshot, losing one is not free: the target and the base MAC
//!   are what the sim boots with, so a lost sidecar costs the device, not a
//!   picture. That is why `Forget` deletes it deliberately and nothing else
//!   ever does.
//!
//! Nothing about a sim rides the wire. The hello reports a `board_id` and a
//! base MAC exactly as silicon does; what says "sim" is this file and the
//! registry row's `transport`, both of them Studio's own bookkeeping.

use lpa_devices::identity::{DeviceUid, EndpointKey, MacAddress};
use lpa_devices::link::LinkInfo;
use lpfs::{AsLpPath, FsError, LpFs};
use serde::{Deserialize, Serialize};

use crate::app::places::{HardwareId, RegisteredDevice};

use super::device_records::SIM_TRANSPORT;

/// Where the sidecars live inside the library store, beside `/registry.json`
/// and `/device-frames/`.
pub const DEVICE_SIMS_DIR: &str = "/device-sims";

/// The sidecar's own format version. Bump on any change to the bytes a
/// reader could misread; an older reader treats a newer file as absent.
pub const SIM_RECORD_VERSION: u32 = 1;

/// The `kind` every sidecar carries. A discriminator, not a mode: it exists
/// so a future device-scoped sidecar in this directory reads as foreign
/// rather than as a sim with missing fields.
pub const SIM_RECORD_KIND: &str = "sim";

/// The endpoint scheme a sim's link is reached at.
///
/// Endpoints are the model's weakest identity binding and the effects
/// layer's routing key, so a scheme prefix is the honest way to say "this
/// link is served by the sim transport" without the model learning a second
/// kind of device.
pub const SIM_ENDPOINT_PREFIX: &str = "sim:";

/// `/device-sims/<uid>.json`.
pub fn sim_record_path(uid: &str) -> String {
    format!("{DEVICE_SIMS_DIR}/{uid}.json")
}

/// The endpoint key a sim with this uid is reached at.
pub fn sim_endpoint(uid: &str) -> EndpointKey {
    EndpointKey(format!("{SIM_ENDPOINT_PREFIX}{uid}"))
}

/// The uid inside a sim endpoint key, or `None` for any other endpoint.
pub fn uid_from_sim_endpoint(endpoint: &str) -> Option<&str> {
    endpoint.strip_prefix(SIM_ENDPOINT_PREFIX)
}

/// The [`LinkInfo`] a sim's link wears.
///
/// `usb: None` and `serial_number: None` are facts, not omissions: there is
/// no USB device and no serial number, and inventing either would put a
/// fake identity rung under a device whose identity Studio minted on
/// purpose.
pub fn sim_link_info(uid: &str, display_name: &str) -> LinkInfo {
    LinkInfo {
        label: format!("Sim · {display_name}"),
        endpoint: sim_endpoint(uid),
        usb: None,
        serial_number: None,
    }
}

/// The on-disk shape of `/device-sims/<uid>.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimRecord {
    pub version: u32,
    /// Always [`SIM_RECORD_KIND`] today.
    pub kind: String,
    /// The hardware target this sim runs: a board id, or the Desktop board
    /// id. What the runtime manifest is looked up by.
    pub target: String,
    /// The minted, locally administered base MAC this sim reports.
    /// `aa:bb:cc:dd:ee:ff`, lowercase — the hello's own form.
    pub base_mac: String,
    /// Epoch seconds (the studio clock) when the sim was created.
    pub created_at: f64,
}

impl SimRecord {
    /// A v1 record for `target`, minted at `created_at`.
    pub fn new(target: impl Into<String>, base_mac: impl Into<String>, created_at: f64) -> Self {
        Self {
            version: SIM_RECORD_VERSION,
            kind: SIM_RECORD_KIND.to_string(),
            target: target.into(),
            base_mac: base_mac.into(),
            created_at,
        }
    }
}

/// Mint a sim's identity from six caller-supplied random bytes.
///
/// Octet 0 gets the locally-administered bit set (`0x02`) and the multicast
/// bit cleared (`0x01`), which is what makes the address legal to invent:
/// the IEEE reserves that range for addresses nobody bought. It also makes
/// the two rejected addresses unreachable by construction — all-zero needs
/// bit 1 clear and all-ones needs bit 0 set — so this cannot mint the
/// identity a *failed* efuse read looks like.
///
/// The uid comes from [`HardwareId::device_uid`] and nowhere else: two
/// Studio installs must agree on a device's uid, and the derivation bytes
/// are a G1-approved contract.
pub fn mint_sim_identity(random: &[u8; 6]) -> (MacAddress, DeviceUid) {
    let mut mac = *random;
    mac[0] = (mac[0] | 0x02) & !0x01;
    let hardware_id = HardwareId::EspEfuse { mac };
    let text = format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    (
        MacAddress(text),
        DeviceUid(hardware_id.device_uid().to_string()),
    )
}

/// A newly minted sim, before anything is written.
///
/// Pure on purpose (sans-IO): minting decides identity, the caller's library
/// settle performs the two writes, and a test can assert what would be
/// written without a store.
#[derive(Clone, Debug, PartialEq)]
pub struct NewSimRecord {
    pub uid: String,
    /// The registry row — a remembered device like any other, keyed on the
    /// derived uid and carrying `transport: "sim"`.
    pub row: RegisteredDevice,
    /// The sidecar that says it is a sim.
    pub sidecar: SimRecord,
}

/// Mint a sim of `target`, optionally already named.
///
/// The row is deliberately ordinary: `hardware_id` is the `efuse:` form,
/// `board_id` is the target, `last_seen_at` is the creation moment so a
/// fresh sim sorts as recent on the remembered line. The ONE thing that is
/// not ordinary is `transport`, and the sidecar is what backs it up.
pub fn new_sim_record(
    target: &str,
    name: Option<&str>,
    random: &[u8; 6],
    created_at: f64,
) -> NewSimRecord {
    let (mac, uid) = mint_sim_identity(random);
    let row = RegisteredDevice {
        uid: uid.0.clone(),
        name: name.unwrap_or_default().to_string(),
        transport: SIM_TRANSPORT.to_string(),
        last_seen_at: created_at,
        board_id: Some(target.to_string()),
        hardware_id: Some(
            HardwareId::from_base_mac(&mac.0)
                .map(|id| id.to_string())
                .unwrap_or_default(),
        ),
        ..RegisteredDevice::default()
    };
    NewSimRecord {
        uid: uid.0,
        row,
        sidecar: SimRecord::new(target, mac.0, created_at),
    }
}

/// Write `uid`'s sidecar.
pub fn write_sim_record(fs: &dyn LpFs, uid: &str, record: &SimRecord) -> Result<(), FsError> {
    // A struct of plain serde types cannot fail to serialize.
    write_sim_record_bytes(fs, uid, &serde_json::to_vec(record).unwrap_or_default())
}

/// Write `uid`'s sidecar from bytes the caller already encoded, so the
/// library host stays codec-free the way the frame sidecar's write does.
pub fn write_sim_record_bytes(fs: &dyn LpFs, uid: &str, bytes: &[u8]) -> Result<(), FsError> {
    fs.write_file(sim_record_path(uid).as_path(), bytes)
}

/// Read `uid`'s sidecar. `None` when there is none — which is what every
/// board that is not a sim looks like — or when what is there should not be
/// trusted (see the module doc's posture).
pub fn read_sim_record(fs: &dyn LpFs, uid: &str) -> Option<SimRecord> {
    let bytes = match fs.read_file(sim_record_path(uid).as_path()) {
        Ok(bytes) => bytes,
        Err(FsError::NotFound(_)) => return None,
        Err(error) => {
            log::debug!("sim record for {uid} unreadable: {error}");
            return None;
        }
    };
    decode(&bytes)
}

/// Decode sidecar bytes, or `None` for anything a reader should not trust.
pub fn decode(bytes: &[u8]) -> Option<SimRecord> {
    let record: SimRecord = match serde_json::from_slice(bytes) {
        Ok(record) => record,
        Err(error) => {
            log::debug!("sim record unreadable: {error}");
            return None;
        }
    };
    if record.version != SIM_RECORD_VERSION {
        log::debug!(
            "sim record version {} is not {SIM_RECORD_VERSION}; ignored",
            record.version
        );
        return None;
    }
    if record.kind != SIM_RECORD_KIND {
        log::debug!(
            "device sidecar kind {:?} is not a sim; ignored",
            record.kind
        );
        return None;
    }
    Some(record)
}

/// Remove `uid`'s sidecar (Forget). A missing file is the goal state, not an
/// error.
pub fn delete_sim_record(fs: &dyn LpFs, uid: &str) -> Result<(), FsError> {
    match fs.delete_file(sim_record_path(uid).as_path()) {
        Ok(()) | Err(FsError::NotFound(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use lpfs::LpFsMemory;

    use super::*;

    const RANDOM: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];

    /// The whole point of the identity rule: an address nobody bought, and
    /// a uid derived the ONE way every uid is derived.
    #[test]
    fn a_minted_identity_is_locally_administered_and_derives_its_uid() {
        let (mac, uid) = mint_sim_identity(&RANDOM);

        assert_eq!(mac.0, "12:22:33:44:55:66", "0x11 | 0x02, & !0x01");
        let octet0 = u8::from_str_radix(&mac.0[..2], 16).unwrap();
        assert_eq!(octet0 & 0x02, 0x02, "locally administered");
        assert_eq!(octet0 & 0x01, 0x00, "not multicast");

        let derived = HardwareId::from_base_mac(&mac.0)
            .expect("a locally administered MAC parses like any other")
            .device_uid()
            .to_string();
        assert_eq!(uid.0, derived, "no second derivation exists");
    }

    /// Studio must mint the same uid twice for the same address, and
    /// different uids for different ones — the join depends on it.
    #[test]
    fn minting_is_deterministic_in_the_bytes() {
        assert_eq!(mint_sim_identity(&RANDOM), mint_sim_identity(&RANDOM));
        assert_ne!(
            mint_sim_identity(&RANDOM),
            mint_sim_identity(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x67])
        );
    }

    /// The two rejected addresses (what a FAILED efuse read looks like) are
    /// unreachable however unlucky the entropy is.
    #[test]
    fn no_random_bytes_can_mint_the_failed_read_addresses() {
        for random in [[0x00; 6], [0xff; 6]] {
            let (mac, _) = mint_sim_identity(&random);
            assert!(
                HardwareId::from_base_mac(&mac.0).is_some(),
                "{} was rejected",
                mac.0
            );
        }
    }

    #[test]
    fn a_new_sim_is_an_ordinary_row_that_says_sim_in_one_column() {
        let minted = new_sim_record("lightplayer/desktop", Some("Bench sim"), &RANDOM, 1_800.0);

        assert_eq!(minted.row.uid, minted.uid);
        assert_eq!(minted.row.transport, "sim");
        assert_eq!(minted.row.name, "Bench sim");
        assert_eq!(minted.row.board_id.as_deref(), Some("lightplayer/desktop"));
        assert_eq!(
            minted.row.hardware_id.as_deref(),
            Some("efuse:12:22:33:44:55:66"),
            "the efuse form, so rekey_or_merge and the identity line are untouched"
        );
        assert_eq!(minted.row.last_seen_at, 1_800.0);
        assert_eq!(minted.sidecar.target, "lightplayer/desktop");
        assert_eq!(minted.sidecar.base_mac, "12:22:33:44:55:66");
        assert_eq!(minted.sidecar.created_at, 1_800.0);
        assert_eq!(minted.sidecar.version, SIM_RECORD_VERSION);
        assert_eq!(minted.sidecar.kind, SIM_RECORD_KIND);
    }

    #[test]
    fn an_unnamed_sim_gets_no_invented_name() {
        let minted = new_sim_record("lightplayer/desktop", None, &RANDOM, 1.0);
        assert!(
            minted.row.name.is_empty(),
            "naming is the registry's own rule, not minting's"
        );
    }

    #[test]
    fn the_link_info_names_the_sim_and_claims_no_hardware() {
        let info = sim_link_info("dev123", "Desktop");

        assert_eq!(info.label, "Sim · Desktop");
        assert_eq!(info.endpoint.0, "sim:dev123");
        assert_eq!(info.usb, None, "there is no USB device to describe");
        assert_eq!(info.serial_number, None);
        assert_eq!(uid_from_sim_endpoint(&info.endpoint.0), Some("dev123"));
        assert_eq!(uid_from_sim_endpoint("usb-1"), None);
    }

    #[test]
    fn the_sidecar_round_trips_as_camel_case_json_with_its_own_version() {
        let record = SimRecord::new("seeed/xiao-esp32-c6", "12:22:33:44:55:66", 42.5);
        let bytes = serde_json::to_vec(&record).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();

        assert!(text.contains("\"version\":1"), "{text}");
        assert!(text.contains("\"kind\":\"sim\""), "{text}");
        assert!(text.contains("\"baseMac\":\"12:22:33:44:55:66\""), "{text}");
        assert!(text.contains("\"createdAt\":42.5"), "{text}");
        assert_eq!(decode(&bytes), Some(record));
    }

    /// The posture: anything this reader should not trust is "not a sim",
    /// never an error and never a migration.
    #[test]
    fn untrusted_bytes_read_as_not_a_sim() {
        assert_eq!(decode(b"not json"), None);
        assert_eq!(decode(b"{}"), None);

        let record = SimRecord::new("lightplayer/desktop", "12:22:33:44:55:66", 1.0);
        let mut foreign: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        foreign["version"] = serde_json::json!(SIM_RECORD_VERSION + 1);
        assert_eq!(decode(&serde_json::to_vec(&foreign).unwrap()), None);

        let mut other_kind: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        other_kind["kind"] = serde_json::json!("twin");
        assert_eq!(
            decode(&serde_json::to_vec(&other_kind).unwrap()),
            None,
            "a future sidecar in this directory is foreign, not a broken sim"
        );
    }

    #[test]
    fn the_store_helpers_key_by_uid_and_tolerate_absence() {
        let fs = LpFsMemory::new();
        let record = SimRecord::new("lightplayer/desktop", "12:22:33:44:55:66", 7.0);

        assert_eq!(sim_record_path("dev1"), "/device-sims/dev1.json");
        assert_eq!(read_sim_record(&fs, "dev1"), None, "absence is not a sim");
        delete_sim_record(&fs, "dev1").expect("deleting nothing is fine");

        write_sim_record(&fs, "dev1", &record).unwrap();
        assert_eq!(read_sim_record(&fs, "dev1"), Some(record));
        assert_eq!(read_sim_record(&fs, "dev2"), None, "keyed by uid");

        delete_sim_record(&fs, "dev1").unwrap();
        assert_eq!(read_sim_record(&fs, "dev1"), None, "Forget takes it away");
    }
}
