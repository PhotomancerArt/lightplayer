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
    /// The list as the board last answered it — after every change that
    /// went through, and still the board's own when a change was refused.
    pub listing: AccessListing,
    pub added: Vec<AddedKey>,
    /// Why a change the sync wanted did not happen (a full device, a
    /// refusal), in words for the panel. The list above is still good.
    pub refused: Option<String>,
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
/// epoch seconds.
///
/// Only the list itself can fail the sync (an older firmware, a lost
/// link). Past it, a change the board refuses ends the sync with the list
/// as it then stood and the refusal beside it. A new key that cannot fit
/// (the store is at [`lpc_access::MAX_SECRETS_PER_FILE`]) is not sent at
/// all; the re-labels and removals still run.
///
/// The list must survive a refusal: the G1 walk's spare C6 had a full store,
/// and a sync that threw away the list it had read left the panel with
/// nothing — "Who has access 0" and a Bluetooth switch locked for good.
pub async fn sync_access<Io: ClientIo>(
    client: &mut LpClient<Io>,
    held: &[HeldKey],
    stale: &[[u8; SALT_BYTES]],
    added_at: u64,
) -> Result<AccessSynced, String> {
    let mut listing = send(client, ClientRequest::AccessList).await?;
    let plan = plan_sync(&listing, held, stale);
    let mut added = Vec::new();
    let refused = match apply_sync_plan(client, &plan, added_at, &mut listing, &mut added).await {
        Ok(skipped) => skipped,
        Err(error) => Some(error),
    };
    Ok(AccessSynced {
        listing,
        added,
        refused,
    })
}

/// The changes of a sync, in order, keeping `listing` at the board's last
/// answer. A refusal ends it (`Err`); a key skipped because the list is
/// full does not, and is answered as `Ok(Some(why))`.
async fn apply_sync_plan<Io: ClientIo>(
    client: &mut LpClient<Io>,
    plan: &SyncPlan,
    added_at: u64,
    listing: &mut AccessListing,
    added: &mut Vec<AddedKey>,
) -> Result<Option<String>, String> {
    let mut skipped = None;
    for key in &plan.add {
        if listing.entries.len() >= lpc_access::MAX_SECRETS_PER_FILE {
            skipped.get_or_insert_with(|| full_sentence(&key.label));
            continue;
        }
        *listing = send(
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
        *listing = send(
            client,
            ClientRequest::AccessAdd {
                entry: key.entry(added_at),
            },
        )
        .await?;
    }
    for salt in &plan.remove {
        *listing = send(client, ClientRequest::AccessRemove { salt: *salt }).await?;
    }
    Ok(skipped)
}

/// Why `label` is not on a device whose list is full.
fn full_sentence(label: &str) -> String {
    format!(
        "This device's list is full ({} entries), so \u{201c}{label}\u{201d} could not be added. \
         Remove one to make room.",
        lpc_access::MAX_SECRETS_PER_FILE
    )
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
    use lpc_access::{SecretEntry, SecretKind, Tier};

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

    /// The G1 walk's spare C6: a full store answers its list, and a sync
    /// whose add cannot fit keeps that list and says why — it does not fail.
    #[test]
    fn a_full_store_is_listed_and_the_add_that_cannot_fit_is_named() {
        let full: Vec<SecretEntry> = (0..lpc_access::MAX_SECRETS_PER_FILE as u8)
            .map(|n| held(&format!("guest {n}"), n + 100).key.entry(1))
            .collect();
        let board = FakeBoard::with_entries(full);
        let mut usb = board.usb();
        let synced =
            block_on(sync_access(&mut usb, &[held("Mine", 1)], &[], 7)).expect("the list was read");
        assert_eq!(
            synced.listing.entries.len(),
            lpc_access::MAX_SECRETS_PER_FILE
        );
        assert!(synced.added.is_empty());
        let why = synced.refused.expect("why Mine is not listed");
        assert!(why.contains("full") && why.contains("Mine"), "{why}");
        assert_eq!(
            board.store().secrets.len(),
            lpc_access::MAX_SECRETS_PER_FILE,
            "nothing written"
        );
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
