//! `lp-cli serve --relay <origin>`: the host board on the cloud relay.
//!
//! The host board dials the relay the way a C6 does — the same
//! `lpc-relay` client, one session at a time — so Studio's relay work (M8)
//! has a board-shaped peer to build against before the firmware lands.
//! Its account entries come from its own access store (`/.lp/access.json`);
//! with `LP_CLOUD_SESSION` set, the signed-in account's key is fetched once
//! and installed there first ([`install_account_key`]).
//!
//! Every session through the relay is `LinkTrust::Relayed` on the host
//! board, exactly as on a C6: an account key opens it at that key's tier,
//! the board's password at the password's, and the anonymous key at
//! nothing.

pub mod relay_device_leg;
pub mod relay_host_transport;
pub mod relay_route_link;

use relay_device_leg::run_device_leg;
use relay_host_transport::RelayHostTransport;

use anyhow::{Result, bail};
use lpc_access::{DeviceAccessFile, SecretEntry, SecretKind, Tier, hmac_sha256};
use lpc_model::AsLpPath;
use lpc_relay::{RelayAccount, RelayBoardId, RelayClientConfig};
use lpc_shared::transport::ServerTransport;
use lpfs::LpFs;
use tokio::sync::mpsc;

use crate::client::relay_session::account_access;

/// The host board's name at the relay, for people.
pub fn host_board_label() -> String {
    let host = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "this computer".to_string());
    format!("lp-cli on {host}")
}

/// A stable relay id for a host board served from `seed` (its directory, or
/// "memory"): a locally administered MAC, so it never claims a real one.
pub fn host_board_id(seed: &str) -> RelayBoardId {
    let digest = hmac_sha256(b"lp-cli host board", seed.as_bytes());
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&digest[..6]);
    mac[0] = (mac[0] | 0x02) & 0xfe;
    RelayBoardId(mac)
}

/// The relay's plain-HTTP device-leg address for `origin`:
/// `https://lightplayer.app` → `lightplayer.app:80` (the device leg is
/// never TLS); `http://127.0.0.1:2812` → `127.0.0.1:2812`.
pub fn device_leg_address(origin: &str) -> Result<(String, u16)> {
    let (rest, default_port) = if let Some(rest) = origin.strip_prefix("https://") {
        (rest, None)
    } else if let Some(rest) = origin.strip_prefix("http://") {
        (rest, Some(80))
    } else {
        bail!("--relay takes an http:// or https:// origin, e.g. https://lightplayer.app");
    };
    let authority = rest.trim_end_matches('/');
    if authority.is_empty() || authority.contains('/') {
        bail!("--relay takes an origin (scheme and host), not a path: {origin}");
    }
    match (authority.rsplit_once(':'), default_port) {
        (Some((host, port)), Some(_)) => {
            let port: u16 = port
                .parse()
                .map_err(|_| anyhow::anyhow!("'{port}' is not a port in {origin}"))?;
            Ok((host.to_string(), port))
        }
        // An https origin's port is its TLS port; the device leg is port 80.
        (Some((host, _)), None) => Ok((host.to_string(), 80)),
        (None, _) => Ok((authority.to_string(), 80)),
    }
}

/// The account entries in the host board's access store.
pub fn relay_accounts(fs: &dyn LpFs) -> Vec<RelayAccount> {
    let store = fs
        .read_file(DeviceAccessFile::PATH.as_path())
        .ok()
        .and_then(|bytes| DeviceAccessFile::from_json(&bytes).ok())
        .unwrap_or_default();
    RelayAccount::from_entries(&store.secrets)
}

/// Fetch the signed-in account's key from `origin` and install it in the
/// host board's store, as Studio does over USB. Returns the account's name;
/// prints nothing itself, and never the key or the session.
pub async fn install_account_key(fs: &dyn LpFs, origin: &str, session: &str) -> Result<String> {
    let (access, name) = account_access(origin, session).await?;
    let entry = SecretEntry::from_password(
        format!("{name}'s account"),
        Tier::Edit,
        &access.key_secret,
        access.key_salt,
        1,
    )
    .with_kind(SecretKind::Account);
    let mut store = fs
        .read_file(DeviceAccessFile::PATH.as_path())
        .ok()
        .and_then(|bytes| DeviceAccessFile::from_json(&bytes).ok())
        .unwrap_or_default();
    store
        .upsert_secret(entry)
        .map_err(|error| anyhow::anyhow!("the access store refused the key: {error}"))?;
    let json = store
        .to_json()
        .map_err(|error| anyhow::anyhow!("the access store did not serialize: {error}"))?;
    fs.write_file(DeviceAccessFile::PATH.as_path(), json.as_bytes())
        .map_err(|error| anyhow::anyhow!("could not write the access store: {error}"))?;
    Ok(name)
}

/// Put the host board on the relay at `origin`: spawn its device leg on the
/// current tokio runtime and wrap `inner` so the relay's sessions are links
/// of the server too.
pub fn start_relay_host<T: ServerTransport>(
    inner: T,
    origin: &str,
    board: RelayBoardId,
    label: String,
    accounts: Vec<RelayAccount>,
) -> Result<RelayHostTransport<T>> {
    let (host, port) = device_leg_address(origin)?;
    let config = RelayClientConfig {
        host,
        port,
        board_mac: board.0,
        label,
        wire_proto: lpc_wire::WIRE_PROTO_VERSION,
        max_routes: 1,
        firmware: String::from(env!("LP_APP_VERSION")),
    };
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    tokio::spawn(run_device_leg(config, accounts, event_tx, command_rx));
    Ok(RelayHostTransport::new(inner, event_rx, command_tx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_leg_is_plain_http_on_port_80_or_the_dev_port() {
        assert_eq!(
            device_leg_address("https://lightplayer.app").unwrap(),
            ("lightplayer.app".to_string(), 80)
        );
        assert_eq!(
            device_leg_address("https://lightplayer.app:443/").unwrap(),
            ("lightplayer.app".to_string(), 80)
        );
        assert_eq!(
            device_leg_address("http://127.0.0.1:2812").unwrap(),
            ("127.0.0.1".to_string(), 2812)
        );
        assert!(device_leg_address("ws://x").is_err());
        assert!(device_leg_address("https://x/relay").is_err());
    }

    #[test]
    fn a_host_board_id_is_stable_and_locally_administered() {
        let id = host_board_id("/srv/lamp");
        assert_eq!(id, host_board_id("/srv/lamp"));
        assert_ne!(id, host_board_id("memory"));
        assert_eq!(id.0[0] & 0x03, 0x02, "locally administered, unicast");
    }
}
