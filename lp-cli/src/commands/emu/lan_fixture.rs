//! A virtual LAN's fixture file, and the LAN a host makes from it.
//!
//! `lp-cli emu run --lan <fixture.toml>` and `emu serve --lan
//! <name>=<fixture.toml>` read the networks in range from a file in the
//! format `lp-emu/esp/lp-emu-esp-common/testdata/virtual_lan.toml` is written
//! in (Wi-Fi plan P11): one `[[access_point]]` table per network, with
//!
//! - `name`: the SSID;
//! - `password`: absent for an open network;
//! - `signal_dbm`: the strength a scan reports, a configured number, never a
//!   model;
//! - `hidden` (default `false`): left out of scans, still joinable by name.
//!
//! A file with no `[[access_point]]` is a LAN with nothing in range. Any
//! other key is refused, so a misspelt `pasword` is an error rather than an
//! open network.
//!
//! **Test values only.** A fixture is committed or passed around, and the
//! emulator prints the names it hears: never a real network's name or
//! password.
//!
//! The parser lives here, in the host, rather than in `lp-emu-esp-common`
//! (P11's deviation 4): the emulator takes access points as values, and a
//! file format is a host's choice.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use lp_emu_esp_common::seam::EndpointId;
use lp_emu_esp_common::seam::net::{
    LanConfig, LanDriver, SharedLan, VirtualAccessPoint, VirtualLan,
};
use serde::Deserialize;

/// The board's LAN endpoint port: the firmware's LAN link listens here
/// (`fw-esp32c6/src/net/lan_endpoint_task.rs`), and a forward reaches it.
pub const BOARD_LAN_PORT: u16 = 80;

/// One fixture file, read.
#[derive(Clone, Debug)]
pub struct LanFixture {
    pub path: PathBuf,
    pub access_points: Vec<VirtualAccessPoint>,
}

impl LanFixture {
    /// Read and parse `path`.
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading the LAN fixture {}", path.display()))?;
        let access_points = parse_fixture(&text)
            .map_err(|why| anyhow!("the LAN fixture {}: {why}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            access_points,
        })
    }

    /// A LAN with this fixture's networks in range, at the C6's clock.
    pub fn lan(&self, driver: LanDriver) -> SharedLan {
        let mut lan = VirtualLan::new(LanConfig::new(lp_emu_esp32c6::memmap::CYCLES_PER_US));
        for ap in &self.access_points {
            lan.add_access_point(ap.clone());
        }
        SharedLan::new(lan, driver)
    }

    /// `4 access point(s): home, cafe; hidden: attic` — each name once, in
    /// file order, and never a password.
    pub fn describe(&self) -> String {
        let named = |hidden: bool| {
            let mut names: Vec<&str> = Vec::new();
            for ap in self.access_points.iter().filter(|ap| ap.hidden == hidden) {
                if !names.contains(&ap.name.as_str()) {
                    names.push(&ap.name);
                }
            }
            names.join(", ")
        };
        let (heard, hidden) = (named(false), named(true));
        let mut out = format!("{} access point(s)", self.access_points.len());
        if !heard.is_empty() {
            out.push_str(&format!(": {heard}"));
        }
        if !hidden.is_empty() {
            out.push_str(&format!("; hidden: {hidden}"));
        }
        out
    }
}

/// Forward an OS-picked loopback port to the board's LAN endpoint. The
/// board must already be on `lan` (the builder attaches it at build when
/// the network seam is wanted).
pub fn forward_to_board(lan: &SharedLan, board: EndpointId) -> Result<SocketAddr> {
    let host = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));
    lan.forward(board, host, BOARD_LAN_PORT).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow!(
                "the board is not on its LAN: its network seam is not wanted (`seams=none`, or \
                 `net=real`), so nothing attached it"
            )
        } else {
            anyhow!("forwarding a loopback port to the board's :{BOARD_LAN_PORT}: {e}")
        }
    })
}

/// How a host spells a forward: `lan:127.0.0.1:<port>`, which `lp-cli … lan:`
/// connects to as it is.
pub fn forward_spec(at: SocketAddr) -> String {
    format!("lan:{at}")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureFile {
    #[serde(default)]
    access_point: Vec<FixtureAccessPoint>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureAccessPoint {
    name: String,
    password: Option<String>,
    signal_dbm: i8,
    #[serde(default)]
    hidden: bool,
}

/// A fixture's text → its access points, in file order.
pub fn parse_fixture(text: &str) -> Result<Vec<VirtualAccessPoint>, String> {
    let file: FixtureFile = toml::from_str(text).map_err(|e| e.to_string())?;
    file.access_point
        .into_iter()
        .map(|ap| {
            if ap.name.is_empty() {
                return Err("an [[access_point]] with an empty name".to_string());
            }
            let value = match &ap.password {
                Some(password) => VirtualAccessPoint::secured(&ap.name, password, ap.signal_dbm),
                None => VirtualAccessPoint::open(&ap.name, ap.signal_dbm),
            };
            Ok(if ap.hidden { value.hidden() } else { value })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P11's own fixture reads the way its tests read it.
    #[test]
    fn the_emulators_test_fixture_parses() {
        let text =
            include_str!("../../../../lp-emu/esp/lp-emu-esp-common/testdata/virtual_lan.toml");
        let aps = parse_fixture(text).expect("P11's fixture");
        assert_eq!(aps.len(), 4);
        assert_eq!(
            aps[0],
            VirtualAccessPoint::secured("home", "test-password-1", -45)
        );
        assert_eq!(aps[2], VirtualAccessPoint::open("cafe", -80));
        assert_eq!(
            aps[3],
            VirtualAccessPoint::secured("attic", "test-password-2", -60).hidden()
        );
        let fixture = LanFixture {
            path: PathBuf::from("virtual_lan.toml"),
            access_points: aps,
        };
        assert_eq!(
            fixture.describe(),
            "4 access point(s): home, cafe; hidden: attic"
        );
    }

    #[test]
    fn an_empty_file_is_a_lan_with_nothing_in_range() {
        assert_eq!(parse_fixture("# nothing\n"), Ok(Vec::new()));
    }

    #[test]
    fn a_misspelt_key_is_refused_rather_than_read_as_an_open_network() {
        let err =
            parse_fixture("[[access_point]]\nname = \"home\"\npasword = \"x\"\nsignal_dbm = -50\n")
                .unwrap_err();
        assert!(err.contains("pasword"), "{err}");
        assert!(parse_fixture("[[access_point]]\nname = \"home\"\n").is_err());
        assert!(
            parse_fixture("[[access_point]]\nname = \"\"\nsignal_dbm = -50\n").is_err(),
            "an empty SSID"
        );
    }

    #[test]
    fn a_forward_is_spelled_as_lp_cli_connects_to_it() {
        let at: SocketAddr = "127.0.0.1:28111".parse().unwrap();
        assert_eq!(forward_spec(at), "lan:127.0.0.1:28111");
    }
}
