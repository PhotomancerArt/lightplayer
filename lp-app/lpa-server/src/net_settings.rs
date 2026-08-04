//! Device network settings: `/.lp/net.json` at the server's filesystem ROOT
//! (networking M2-P3).
//!
//! Like `/.lp/device.json` (see [`crate::device_identity`]), network settings
//! are DEVICE-scoped: the file lives at the base-fs root, outside every
//! project storage dir, so it survives project pushes and dies only with a
//! full filesystem wipe — after which the device is simply back on defaults
//! (its image's own medium, DHCP, derived hostname). That wipe-to-blank story
//! is the whole recovery design: there is no migration and nothing to repair.
//!
//! The firmware only ever READS this file, once at boot. Writes come from
//! Studio over the serial wire as a root-path `FsRequest::Write` — the same
//! stamping path device.json uses — so the write-throttling questions that
//! shape `/.lp/panel.json` do not arise here. A settings change applies on
//! the next boot; live re-join feedback is the wire-status milestone's
//! concern (M2-P4), not this file's.
//!
//! Posture is the `/.lp/` family one: lenient load (missing, unparseable, or
//! unknown-version file → defaults, no panic, no migration; alpha
//! bump-and-refuse) and per-field leniency below that — a malformed static
//! address or an unusable hostname drops THAT field with a warning rather
//! than the whole file, because "boots reachable on DHCP with a derived
//! hostname" is strictly better than "boots deaf because one octet was
//! mistyped".

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::net::Ipv4Addr;

use lpc_model::AsLpPath;
use lpfs::LpFs;
use serde::{Deserialize, Serialize};

/// Root path of the network settings file.
pub const NET_SETTINGS_PATH: &str = "/.lp/net.json";

/// Format version. Bump-and-refuse: any other version is ignored wholesale
/// (alpha posture — a dropped file costs one reconfiguration, never a boot).
pub const NET_SETTINGS_VERSION: u32 = 1;

/// The longest hostname the stack will carry (embassy-net's DHCP option 12
/// cap; also a safe mDNS label length for the discovery milestone).
pub const MAX_HOSTNAME_LEN: usize = 32;

/// The persisted file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetSettingsFile {
    pub version: u32,
    /// Which network medium to bring up. `None` means "the image's own
    /// default medium" — an ethernet image brings up ethernet, a wifi image
    /// wifi — so a stock device needs no file at all. An image asked for a
    /// medium it does not have logs why and stays off; it never guesses.
    #[serde(default)]
    pub mode: Option<NetMode>,
    /// Host name announced to the network (DHCP option 12 now, mDNS later).
    /// `None` → the firmware derives `lp-<mac6>` from the interface MAC.
    #[serde(default)]
    pub hostname: Option<String>,
    /// Credentials for `mode: wifi`. Parsed and carried today so the file
    /// format is complete; the first consumer is the C6 STA provider (M2-P6).
    #[serde(default)]
    pub wifi: Option<WifiCredentials>,
    /// Static IPv4 configuration. Absent → DHCP.
    #[serde(default)]
    pub static_ipv4: Option<StaticIpv4Def>,
}

/// The network medium selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetMode {
    Ethernet,
    Wifi,
    Off,
}

/// Wifi STA credentials.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WifiCredentials {
    pub ssid: String,
    /// Empty string = open network.
    #[serde(default)]
    pub password: String,
}

/// Static IPv4 configuration as persisted: dotted-quad strings, because
/// they are what humans type and diff, and because `serde`'s `core::net`
/// coverage is not something this file wants to depend on. Parsing to real
/// addresses happens in [`NetSettingsFile::resolved_static_ipv4`], leniently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticIpv4Def {
    /// Dotted-quad address, e.g. `"192.168.1.50"`.
    pub address: String,
    /// Prefix length, e.g. `24`.
    pub prefix: u8,
    /// Dotted-quad gateway. Absent → no default route.
    #[serde(default)]
    pub gateway: Option<String>,
    /// Dotted-quad DNS servers; the stack keeps at most the first three.
    #[serde(default)]
    pub dns: Vec<String>,
}

/// A static IPv4 configuration that actually parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticIpv4 {
    pub address: Ipv4Addr,
    pub prefix: u8,
    pub gateway: Option<Ipv4Addr>,
    pub dns: Vec<Ipv4Addr>,
}

/// Read the settings file. Missing, unparseable, or wrong-version → `None`
/// (boot on defaults).
pub fn read(fs: &dyn LpFs) -> Option<NetSettingsFile> {
    let bytes = fs.read_file(NET_SETTINGS_PATH.as_path()).ok()?;
    let file = match lpc_wire::json::from_slice::<NetSettingsFile>(&bytes) {
        Ok(file) => file,
        Err(error) => {
            log::warn!("net settings: ignoring unparseable /.lp/net.json: {error:?}");
            return None;
        }
    };
    if file.version != NET_SETTINGS_VERSION {
        log::warn!(
            "net settings: ignoring /.lp/net.json with unknown version {} (expected {})",
            file.version,
            NET_SETTINGS_VERSION
        );
        return None;
    }
    Some(file)
}

impl NetSettingsFile {
    /// The hostname, if it is one the stack can actually announce: 1..=32
    /// chars of `[a-z0-9-]` (case folded by the author, not here). Anything
    /// else is dropped with a warning — the derived default takes over.
    pub fn valid_hostname(&self) -> Option<&str> {
        let name = self.hostname.as_deref()?;
        let ok = !name.is_empty()
            && name.len() <= MAX_HOSTNAME_LEN
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !name.starts_with('-')
            && !name.ends_with('-');
        if !ok {
            log::warn!(
                "net settings: hostname {name:?} is not a usable DNS label (1..={MAX_HOSTNAME_LEN} of [a-z0-9-]); using the derived default"
            );
            return None;
        }
        Some(name)
    }

    /// The static IPv4 configuration, if present AND parseable. A malformed
    /// address or prefix drops the whole static block with a warning — the
    /// device then boots on DHCP, which is reachable and diagnosable, rather
    /// than deaf on a half-configured static address. Malformed DNS entries
    /// are dropped individually.
    pub fn resolved_static_ipv4(&self) -> Option<StaticIpv4> {
        let def = self.static_ipv4.as_ref()?;
        let address: Ipv4Addr = match def.address.parse() {
            Ok(address) => address,
            Err(_) => {
                log::warn!(
                    "net settings: static_ipv4.address {:?} is not a dotted-quad IPv4 address; falling back to DHCP",
                    def.address
                );
                return None;
            }
        };
        if def.prefix == 0 || def.prefix > 30 {
            log::warn!(
                "net settings: static_ipv4.prefix {} is out of range (1..=30); falling back to DHCP",
                def.prefix
            );
            return None;
        }
        let gateway = match def.gateway.as_deref() {
            None => None,
            Some(raw) => match raw.parse::<Ipv4Addr>() {
                Ok(gateway) => Some(gateway),
                Err(_) => {
                    log::warn!(
                        "net settings: static_ipv4.gateway {raw:?} is not a dotted-quad IPv4 address; falling back to DHCP"
                    );
                    return None;
                }
            },
        };
        let dns = def
            .dns
            .iter()
            .filter_map(|raw| match raw.parse::<Ipv4Addr>() {
                Ok(server) => Some(server),
                Err(_) => {
                    log::warn!(
                        "net settings: dropping static_ipv4.dns entry {raw:?} (not a dotted-quad IPv4 address)"
                    );
                    None
                }
            })
            .collect();
        Some(StaticIpv4 {
            address,
            prefix: def.prefix,
            gateway,
            dns,
        })
    }
}

#[cfg(test)]
mod tests {
    use lpfs::LpFsMemory;

    use super::*;

    fn write(fs: &LpFsMemory, json: &str) {
        fs.write_file(NET_SETTINGS_PATH.as_path(), json.as_bytes())
            .unwrap();
    }

    #[test]
    fn missing_unparseable_or_wrong_version_reads_as_defaults() {
        let fs = LpFsMemory::new();
        assert_eq!(read(&fs), None);

        write(&fs, "not json");
        assert_eq!(read(&fs), None);

        write(&fs, r#"{"version":99,"mode":"off"}"#);
        assert_eq!(read(&fs), None);
    }

    #[test]
    fn reads_a_full_file() {
        let fs = LpFsMemory::new();
        write(
            &fs,
            r#"{
                "version": 1,
                "mode": "ethernet",
                "hostname": "porch-sign",
                "wifi": {"ssid": "attic", "password": "hunter2"},
                "static_ipv4": {
                    "address": "192.168.1.50",
                    "prefix": 24,
                    "gateway": "192.168.1.1",
                    "dns": ["1.1.1.1", "9.9.9.9"]
                }
            }"#,
        );
        let file = read(&fs).unwrap();
        assert_eq!(file.mode, Some(NetMode::Ethernet));
        assert_eq!(file.valid_hostname(), Some("porch-sign"));
        assert_eq!(file.wifi.as_ref().unwrap().ssid, "attic");
        let static_v4 = file.resolved_static_ipv4().unwrap();
        assert_eq!(static_v4.address, Ipv4Addr::new(192, 168, 1, 50));
        assert_eq!(static_v4.prefix, 24);
        assert_eq!(static_v4.gateway, Some(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(
            static_v4.dns,
            alloc::vec![Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(9, 9, 9, 9)]
        );
    }

    #[test]
    fn minimal_file_leaves_everything_defaulted() {
        let fs = LpFsMemory::new();
        write(&fs, r#"{"version":1}"#);
        let file = read(&fs).unwrap();
        assert_eq!(file.mode, None);
        assert_eq!(file.valid_hostname(), None);
        assert_eq!(file.wifi, None);
        assert_eq!(file.resolved_static_ipv4(), None);
    }

    #[test]
    fn unusable_hostname_is_dropped_not_fatal() {
        let fs = LpFsMemory::new();
        for bad in [
            r#"{"version":1,"hostname":""}"#,
            r#"{"version":1,"hostname":"has spaces"}"#,
            r#"{"version":1,"hostname":"-leading"}"#,
            r#"{"version":1,"hostname":"way-too-long-for-a-dhcp-option-twelve-label"}"#,
        ] {
            write(&fs, bad);
            let file = read(&fs).unwrap();
            assert_eq!(file.valid_hostname(), None, "{bad}");
        }
    }

    #[test]
    fn malformed_static_config_falls_back_to_dhcp() {
        let fs = LpFsMemory::new();
        for bad in [
            r#"{"version":1,"static_ipv4":{"address":"not-an-ip","prefix":24}}"#,
            r#"{"version":1,"static_ipv4":{"address":"192.168.1.50","prefix":0}}"#,
            r#"{"version":1,"static_ipv4":{"address":"192.168.1.50","prefix":31}}"#,
            r#"{"version":1,"static_ipv4":{"address":"192.168.1.50","prefix":24,"gateway":"bogus"}}"#,
        ] {
            write(&fs, bad);
            let file = read(&fs).unwrap();
            assert_eq!(file.resolved_static_ipv4(), None, "{bad}");
        }
    }

    #[test]
    fn malformed_dns_entries_drop_individually() {
        let fs = LpFsMemory::new();
        write(
            &fs,
            r#"{"version":1,"static_ipv4":{"address":"10.0.0.2","prefix":16,"dns":["10.0.0.1","nope","8.8.8.8"]}}"#,
        );
        let static_v4 = read(&fs).unwrap().resolved_static_ipv4().unwrap();
        assert_eq!(
            static_v4.dns,
            alloc::vec![Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(8, 8, 8, 8)]
        );
        assert_eq!(static_v4.gateway, None);
    }
}
