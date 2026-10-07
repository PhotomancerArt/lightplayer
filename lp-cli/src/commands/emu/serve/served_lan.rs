//! A LAN `emu serve` holds: `--lan <name>=<fixture.toml>`, shared by every
//! board whose spec says `lan=<name>`.
//!
//! The boards of one serve each run on an OS thread of their own with their
//! own guest clock, so a served LAN is a [`LanDriver::WallClock`] one: every
//! board's machine drives it, on the host's clock. **Not deterministic** —
//! the deterministic form of two boards on one LAN is `lp-emu-esp32c6`'s
//! lockstep runner, never this door.
//!
//! A board on a served LAN has a **forward** ([`BoardOnLan::forward`]): a
//! loopback port carried to its LAN endpoint (`:80`), which `GET /boards`
//! lists as `lan:127.0.0.1:<port>`. It is a door of its own beside the
//! board's USB door, so the USB door's one-client rule is untouched.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Mutex;

use anyhow::{Result, bail};
use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::EndpointId;
use lp_emu_esp_common::seam::net::{LanDriver, ProbeId, SharedLan};

use super::super::lan_fixture::{LanFixture, forward_spec};

/// One `--lan` a serve declared.
#[derive(Debug)]
pub struct ServedLan {
    pub name: String,
    pub fixture: LanFixture,
    pub lan: SharedLan,
    /// The door's probe on this LAN (`GET /lans/<name>/browse`), put on the
    /// segment the first time a browse asks.
    probe: Mutex<Option<ProbeId>>,
    /// One browse at a time: a browse clears what the probe heard before it.
    pub browsing: tokio::sync::Mutex<()>,
}

impl ServedLan {
    /// The probe the door asks with, put on the LAN the first time.
    pub fn probe(&self) -> ProbeId {
        let mut probe = self.probe.lock().expect("probe slot poisoned");
        *probe.get_or_insert_with(|| self.lan.with(|lan| lan.add_probe()))
    }
}

/// A board's place on a served LAN, before its machine is built.
#[derive(Clone, Debug)]
pub struct LanSeat {
    pub name: String,
    pub lan: SharedLan,
    /// Its participant on the LAN (`<participant>/net`): the board's seat in
    /// the serve, so no two boards share one.
    pub participant: ParticipantId,
}

/// A board on a served LAN, once its machine is built and forwarded to.
#[derive(Clone, Debug)]
pub struct BoardOnLan {
    pub name: String,
    pub lan: SharedLan,
    pub endpoint: EndpointId,
    /// Where a host connects: `127.0.0.1:<port>`, carried to the board's
    /// `:80`.
    pub forward: SocketAddr,
}

impl BoardOnLan {
    /// `lan:127.0.0.1:<port>`, as `lp-cli … lan:` takes it.
    pub fn forward_spec(&self) -> String {
        forward_spec(self.forward)
    }

    /// The board's address on the LAN, once its DHCP exchange finished.
    pub fn address(&self) -> Option<Ipv4Addr> {
        self.lan.address(self.endpoint)
    }

    /// The board's next DHCP lease gets a different address than its last
    /// (the `renumber` control verb; walk step W9).
    pub fn renumber_next_lease(&self) {
        self.lan.renumber_next_lease(self.endpoint);
    }
}

/// Every `--lan <name>=<fixture>`, read.
pub fn parse_lans(specs: &[String]) -> Result<Vec<ServedLan>> {
    let mut out: Vec<ServedLan> = Vec::with_capacity(specs.len());
    for text in specs {
        let Some((name, path)) = text.split_once('=') else {
            bail!("--lan `{text}`: expected <name>=<fixture.toml>, for example home=lan.toml");
        };
        let name = name.trim();
        if !is_path_segment(name) {
            bail!(
                "--lan `{text}`: `{name}` is not a LAN name — letters, digits, `-` and `_`, \
                 because the name is a path segment (`/lans/<name>/…`)"
            );
        }
        if out.iter().any(|l| l.name == name) {
            bail!("--lan `{text}`: a second LAN called `{name}`");
        }
        let fixture = LanFixture::read(std::path::Path::new(path.trim()))?;
        out.push(ServedLan {
            name: name.to_string(),
            lan: fixture.lan(LanDriver::WallClock),
            fixture,
            probe: Mutex::new(None),
            browsing: tokio::sync::Mutex::new(()),
        });
    }
    Ok(out)
}

/// Letters, digits, `-` and `_`: an id that is also a URL path segment.
pub fn is_path_segment(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lan_is_a_name_and_a_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lan.toml");
        std::fs::write(
            &path,
            "[[access_point]]\nname = \"lp-walk-net\"\npassword = \"correct-horse-42\"\nsignal_dbm = -50\n",
        )
        .unwrap();
        let lans = parse_lans(&[format!("home={}", path.display())]).expect("one LAN");
        assert_eq!(lans[0].name, "home");
        assert_eq!(lans[0].fixture.access_points.len(), 1);
        assert_eq!(lans[0].lan.driver(), LanDriver::WallClock);
        assert_eq!(lans[0].probe(), lans[0].probe(), "one probe per LAN");

        let twice = parse_lans(&[
            format!("home={}", path.display()),
            format!("home={}", path.display()),
        ]);
        assert!(twice.is_err(), "the name is the route");
        assert!(parse_lans(&[format!("ho/me={}", path.display())]).is_err());
        assert!(parse_lans(&[path.display().to_string()]).is_err());
        assert!(parse_lans(&["home=/nowhere/lan.toml".to_string()]).is_err());
    }
}
