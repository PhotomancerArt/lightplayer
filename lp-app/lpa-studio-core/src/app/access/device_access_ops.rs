//! The device's access list, read and changed on the board (P1's requests).
//!
//! The board merges: `AccessAdd` adds an entry or replaces the one with the
//! same salt, `AccessRemove` drops one by salt, `AccessSetSwitches` sets
//! Bluetooth and "anyone nearby", and each answers the list as it now
//! stands. So two browsers adding keys never erase each other's, and Studio
//! never rewrites the whole store. All four are edit tier.
//!
//! Two conversations run here:
//!
//! - [`sync_access`] — on a USB connect: list, then add the held keys the
//!   device is missing (and re-label any whose name changed), then remove
//!   retired account keys. With no held keys it only lists (a Bluetooth
//!   link at edit, which must never be added to automatically).
//! - [`run_access_ops`] — a gesture from the access panel, or Undo.

use lpa_client::{ClientError, ClientIo, LpClient};
use lpc_access::SALT_BYTES;
use lpc_wire::server::AccessEntryInfo;
use lpc_wire::{ClientRequest, WireServerMsgBody};
use serde::{Deserialize, Serialize};

use super::key_holder::{HeldKey, InstallableKey};

/// A device's access list, as the board last answered it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessListing {
    /// The STORED switch: a change applies at the device's next boot.
    pub ble_enabled: bool,
    pub open: bool,
    pub entries: Vec<AccessEntryInfo>,
}

/// One change to a device's access list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessOp {
    /// Add (or replace, by salt) an entry; its key is derived in the
    /// conversation.
    Add(InstallableKey),
    /// Drop the entry with this salt.
    Remove([u8; SALT_BYTES]),
    /// Set either switch; `None` leaves it.
    Switches {
        ble_enabled: Option<bool>,
        open: Option<bool>,
    },
}

/// What a USB connect must change: the held keys the device is missing,
/// the ones whose label (or tier, or kind) moved, and retired salts still on
/// it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncPlan {
    pub add: Vec<InstallableKey>,
    pub relabel: Vec<InstallableKey>,
    pub remove: Vec<[u8; SALT_BYTES]>,
}

impl SyncPlan {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.relabel.is_empty() && self.remove.is_empty()
    }
}

/// An entry a sync added (not one it re-labelled): what the toast names and
/// Undo removes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddedKey {
    pub salt: [u8; SALT_BYTES],
    pub label: String,
}

/// How a sync ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessSynced {
    pub listing: AccessListing,
    pub added: Vec<AddedKey>,
}

/// Compare a device's list with the keys this browser holds.
pub fn plan_sync(
    listing: &AccessListing,
    held: &[HeldKey],
    stale: &[[u8; SALT_BYTES]],
) -> SyncPlan {
    let mut plan = SyncPlan::default();
    for key in held {
        let key = &key.key;
        match listing.entries.iter().find(|entry| entry.salt == key.salt) {
            None => plan.add.push(key.clone()),
            Some(entry)
                if entry.label != key.label || entry.tier != key.tier || entry.kind != key.kind =>
            {
                plan.relabel.push(key.clone());
            }
            Some(_) => {}
        }
    }
    plan.remove = stale
        .iter()
        .filter(|salt| listing.entries.iter().any(|entry| entry.salt == **salt))
        .copied()
        .collect();
    plan
}

/// List, then install what is missing. `added_at` is the caller's clock,
/// epoch seconds. Stops at the first refusal (a full device, a lost tier).
pub async fn sync_access<Io: ClientIo>(
    client: &mut LpClient<Io>,
    held: &[HeldKey],
    stale: &[[u8; SALT_BYTES]],
    added_at: u64,
) -> Result<AccessSynced, String> {
    let mut listing = send(client, ClientRequest::AccessList).await?;
    let plan = plan_sync(&listing, held, stale);
    let mut added = Vec::new();
    for key in &plan.add {
        listing = send(
            client,
            ClientRequest::AccessAdd {
                entry: key.entry(added_at),
            },
        )
        .await?;
        added.push(AddedKey {
            salt: key.salt,
            label: key.label.clone(),
        });
    }
    for key in &plan.relabel {
        listing = send(
            client,
            ClientRequest::AccessAdd {
                entry: key.entry(added_at),
            },
        )
        .await?;
    }
    for salt in plan.remove {
        listing = send(client, ClientRequest::AccessRemove { salt }).await?;
    }
    Ok(AccessSynced { listing, added })
}

/// Apply `ops` in order and answer the list as it then stands.
pub async fn run_access_ops<Io: ClientIo>(
    client: &mut LpClient<Io>,
    ops: &[AccessOp],
    added_at: u64,
) -> Result<AccessListing, String> {
    let mut listing = None;
    for op in ops {
        let request = match op {
            AccessOp::Add(key) => ClientRequest::AccessAdd {
                entry: key.entry(added_at),
            },
            AccessOp::Remove(salt) => ClientRequest::AccessRemove { salt: *salt },
            AccessOp::Switches { ble_enabled, open } => ClientRequest::AccessSetSwitches {
                ble_enabled: *ble_enabled,
                open: *open,
            },
        };
        listing = Some(send(client, request).await?);
    }
    match listing {
        Some(listing) => Ok(listing),
        None => send(client, ClientRequest::AccessList).await,
    }
}

/// One access request, answered with the list.
async fn send<Io: ClientIo>(
    client: &mut LpClient<Io>,
    request: ClientRequest,
) -> Result<AccessListing, String> {
    match client.send_request(request).await {
        Ok(outcome) => match outcome.value.msg {
            WireServerMsgBody::AccessList {
                ble_enabled,
                open,
                entries,
            } => Ok(AccessListing {
                ble_enabled,
                open,
                entries,
            }),
            other => Err(format!(
                "the device answered something else: {}",
                ClientError::unexpected_response("access", other)
            )),
        },
        Err(ClientError::NotPermitted { needs }) => {
            Err(super::not_permitted_sentence(needs).to_string())
        }
        Err(ClientError::Server(error)) => Err(error),
        Err(error) => Err(format!("the device did not answer: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::key_holder::KeyHolder;
    use crate::app::access::test_board::{FakeBoard, block_on};
    use lpc_access::{SecretKind, Tier};

    fn held(label: &str, salt: u8) -> HeldKey {
        HeldKey {
            holder: KeyHolder::Browser,
            key: InstallableKey {
                label: label.to_string(),
                kind: SecretKind::Browser,
                tier: Tier::Edit,
                salt: [salt; 16],
                iterations: 1,
                material: vec![salt; 32],
            },
        }
    }

    #[test]
    fn the_plan_adds_the_missing_relabels_the_renamed_and_removes_the_retired() {
        let listing = AccessListing {
            ble_enabled: true,
            open: false,
            entries: vec![
                AccessEntryInfo::from(&held("Old name", 2).key.entry(1)),
                AccessEntryInfo::from(&held("Retired", 3).key.entry(1)),
            ],
        };
        let plan = plan_sync(
            &listing,
            &[held("New", 1), held("New name", 2)],
            &[[3; 16], [4; 16]],
        );
        assert_eq!(plan.add.len(), 1);
        assert_eq!(plan.add[0].salt, [1; 16]);
        assert_eq!(plan.relabel[0].label, "New name");
        assert_eq!(plan.remove, [[3; 16]]);
    }

    #[test]
    fn a_sync_installs_what_is_missing_and_reports_only_the_additions() {
        let board = FakeBoard::fresh();
        let mut usb = board.usb();
        let synced = block_on(sync_access(&mut usb, &[held("Mine", 1)], &[], 7)).unwrap();
        assert_eq!(synced.added.len(), 1);
        assert_eq!(synced.listing.entries[0].added_at, Some(7));
        assert!(synced.listing.ble_enabled, "no store: Bluetooth on");
        let again = block_on(sync_access(&mut usb, &[held("Mine", 1)], &[], 8)).unwrap();
        assert!(again.added.is_empty());
        assert_eq!(again.listing.entries.len(), 1);
    }

    #[test]
    fn ops_are_refused_by_name_below_edit() {
        let board = FakeBoard::locked(&[("camp", Tier::Play, "x")]);
        let mut ble = board.client();
        let error = block_on(run_access_ops(&mut ble, &[AccessOp::Remove([0; 16])], 1))
            .expect_err("a locked link may not change the list");
        assert!(error.contains("edit device password"), "{error}");
    }
}
