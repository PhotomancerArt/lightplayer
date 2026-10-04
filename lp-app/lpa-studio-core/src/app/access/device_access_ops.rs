//! The device's access list, read and changed on the board (P1's requests).
//!
//! The board merges: `AccessAdd` adds an entry or replaces the one with the
//! same salt, `AccessRemove` drops one by salt, `AccessSetSwitches` sets
//! Bluetooth and who nearby gets in with no password, and each answers the
//! list as it now stands. So two browsers adding keys never erase each
//! other's, and Studio never rewrites the whole store. All four are edit
//! tier.
//!
//! Two conversations run here:
//!
//! - [`sync_access`] — on a USB connect: list, remove retired account keys,
//!   then add the held keys the device is missing (and re-label any whose
//!   name changed). With no held keys it only lists (a Bluetooth link at
//!   edit, which must never be added to automatically).
//! - [`run_access_ops`] — a gesture from the access panel, or Undo.
//!
//! **Automatic room.** A device holds at most
//! [`lpc_access::MAX_SECRETS_PER_FILE`] entries, and every dev-server origin
//! is its own browser key, so a desk board fills up. Before an add that
//! needs a slot on a full device, both conversations drop the browser key
//! added longest ago ([`make_room`]) — never one this browser holds, never
//! an account's entry, never a password — and say which.

use lpa_client::{ClientError, ClientIo, LpClient};
use lpc_access::{MAX_SECRETS_PER_FILE, OpenTo, SALT_BYTES, SecretKind};
use lpc_wire::server::AccessEntryInfo;
use lpc_wire::{ClientRequest, WireServerMsgBody};
use serde::{Deserialize, Serialize};

use super::key_holder::{HeldKey, InstallableKey};

/// A device's access list, as the board last answered it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessListing {
    /// The STORED switch: a change applies at the device's next boot.
    pub ble_enabled: bool,
    /// Who nearby gets in with no password.
    pub open: OpenTo,
    pub entries: Vec<AccessEntryInfo>,
}

impl Default for AccessListing {
    fn default() -> Self {
        Self {
            ble_enabled: false,
            open: OpenTo::Nobody,
            entries: Vec::new(),
        }
    }
}

/// One change to a device's access list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessOp {
    /// Add (or replace, by salt) an entry; its key is derived in the
    /// conversation.
    Add(InstallableKey),
    /// Drop the entry with this salt.
    Remove([u8; SALT_BYTES]),
    /// Set either setting; `None` leaves it.
    Switches {
        ble_enabled: Option<bool>,
        open: Option<OpenTo>,
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

/// A browser key dropped to make room: what the toast or the panel names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DroppedKey {
    pub label: String,
    pub added_at: Option<u64>,
}

/// How a sync ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessSynced {
    pub listing: AccessListing,
    pub added: Vec<AddedKey>,
    /// Browser keys dropped to make room for what was added.
    pub dropped: Vec<DroppedKey>,
}

/// How a panel change (or an Undo) ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessChanged {
    pub listing: AccessListing,
    /// Browser keys dropped to make room for what was added.
    pub dropped: Vec<DroppedKey>,
}

/// What a full device that cannot make room says.
pub const FULL_SENTENCE: &str = "This device is full, and nothing on it can make room on its own — remove something from its list.";

/// The entry to drop so one more fits, or `None` when there is room
/// already. On a full device it is the `browser` entry added longest ago
/// (one with no time counts as oldest) whose salt is not in `keep`; `Err`
/// when there is none — the account's entries and passwords are never
/// dropped for room.
pub fn make_room<'a>(
    listing: &'a AccessListing,
    keep: &[[u8; SALT_BYTES]],
) -> Result<Option<&'a AccessEntryInfo>, &'static str> {
    if listing.entries.len() < MAX_SECRETS_PER_FILE {
        return Ok(None);
    }
    listing
        .entries
        .iter()
        .filter(|entry| entry.kind == SecretKind::Browser && !keep.contains(&entry.salt))
        .min_by_key(|entry| entry.added_at)
        .map(Some)
        .ok_or(FULL_SENTENCE)
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

/// List, remove the retired, then install what is missing, making room
/// as it goes. `added_at` is the caller's clock, epoch seconds. Stops at
/// the first refusal (a full device nothing can be dropped from, a lost
/// tier).
pub async fn sync_access<Io: ClientIo>(
    client: &mut LpClient<Io>,
    held: &[HeldKey],
    stale: &[[u8; SALT_BYTES]],
    added_at: u64,
) -> Result<AccessSynced, String> {
    let mut listing = send(client, ClientRequest::AccessList).await?;
    let plan = plan_sync(&listing, held, stale);
    // Retired keys go first: their slots are the room a new account key
    // needs on a full device.
    for salt in plan.remove {
        listing = send(client, ClientRequest::AccessRemove { salt }).await?;
    }
    let keep: Vec<[u8; SALT_BYTES]> = held.iter().map(HeldKey::salt).collect();
    let mut added = Vec::new();
    let mut dropped = Vec::new();
    for key in &plan.add {
        listing = add_with_room(client, listing, key, added_at, &keep, &mut dropped).await?;
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
    Ok(AccessSynced {
        listing,
        added,
        dropped,
    })
}

/// Apply `ops` in order and answer the list as it then stands. An add on a
/// full device first drops the oldest browser key whose salt is not in
/// `keep` (this browser's own keys).
pub async fn run_access_ops<Io: ClientIo>(
    client: &mut LpClient<Io>,
    ops: &[AccessOp],
    added_at: u64,
    keep: &[[u8; SALT_BYTES]],
) -> Result<AccessChanged, String> {
    let mut listing: Option<AccessListing> = None;
    let mut dropped = Vec::new();
    for op in ops {
        let next = match op {
            AccessOp::Add(key) => {
                let current = match listing.take() {
                    Some(listing) => listing,
                    None => send(client, ClientRequest::AccessList).await?,
                };
                add_with_room(client, current, key, added_at, keep, &mut dropped).await?
            }
            AccessOp::Remove(salt) => {
                send(client, ClientRequest::AccessRemove { salt: *salt }).await?
            }
            AccessOp::Switches { ble_enabled, open } => {
                send(
                    client,
                    ClientRequest::AccessSetSwitches {
                        ble_enabled: *ble_enabled,
                        open: *open,
                    },
                )
                .await?
            }
        };
        listing = Some(next);
    }
    let listing = match listing {
        Some(listing) => listing,
        None => send(client, ClientRequest::AccessList).await?,
    };
    Ok(AccessChanged { listing, dropped })
}

/// Add `key`, first dropping one browser key when the device is full and
/// `key` is new to it.
async fn add_with_room<Io: ClientIo>(
    client: &mut LpClient<Io>,
    listing: AccessListing,
    key: &InstallableKey,
    added_at: u64,
    keep: &[[u8; SALT_BYTES]],
    dropped: &mut Vec<DroppedKey>,
) -> Result<AccessListing, String> {
    let replaces = listing.entries.iter().any(|entry| entry.salt == key.salt);
    if !replaces && let Some(oldest) = make_room(&listing, keep).map_err(str::to_string)? {
        let salt = oldest.salt;
        dropped.push(DroppedKey {
            label: oldest.label.clone(),
            added_at: oldest.added_at,
        });
        send(client, ClientRequest::AccessRemove { salt }).await?;
    }
    send(
        client,
        ClientRequest::AccessAdd {
            entry: key.entry(added_at),
        },
    )
    .await
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
            open: OpenTo::Nobody,
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
        assert_eq!(synced.listing.open, OpenTo::Edit, "no store: open, for now");
        let again = block_on(sync_access(&mut usb, &[held("Mine", 1)], &[], 8)).unwrap();
        assert!(again.added.is_empty());
        assert_eq!(again.listing.entries.len(), 1);
    }

    #[test]
    fn ops_are_refused_by_name_below_edit() {
        let board = FakeBoard::locked(&[("camp", Tier::Play, "x")]);
        let mut ble = board.client();
        let error = block_on(run_access_ops(
            &mut ble,
            &[AccessOp::Remove([0; 16])],
            1,
            &[],
        ))
        .expect_err("a locked link may not change the list");
        assert!(error.contains("author device password"), "{error}");
    }

    #[test]
    fn a_full_device_drops_its_oldest_other_browser_to_make_room() {
        let mut entries: Vec<_> = (1..=16u8)
            .map(|salt| {
                held("Brave on Mac", salt)
                    .key
                    .entry(1_000 + u64::from(17 - salt))
            })
            .collect();
        // The oldest of all is this browser's own key: it is never dropped.
        entries[0] = held("Mine", 1).key.entry(1);
        let board = FakeBoard::with_entries(entries);
        let mut usb = board.usb();
        let synced = block_on(sync_access(
            &mut usb,
            &[held("Mine", 1), held("New origin", 40)],
            &[],
            9_000,
        ))
        .unwrap();
        assert_eq!(synced.added.len(), 1);
        assert_eq!(
            synced.dropped,
            [DroppedKey {
                label: "Brave on Mac".to_string(),
                added_at: Some(1_001),
            }],
            "salt 16 was added longest ago, after this browser's own"
        );
        assert_eq!(synced.listing.entries.len(), MAX_SECRETS_PER_FILE);
        assert!(synced.listing.entries.iter().any(|e| e.salt == [1; 16]));
        assert!(synced.listing.entries.iter().all(|e| e.salt != [16; 16]));
    }

    #[test]
    fn a_full_device_of_passwords_and_account_keys_says_so() {
        let entries: Vec<_> = (1..=16u8)
            .map(|salt| {
                let mut key = held("friends", salt).key;
                key.kind = SecretKind::Password;
                key.entry(1)
            })
            .collect();
        let board = FakeBoard::with_entries(entries);
        let mut usb = board.usb();
        let error = block_on(sync_access(&mut usb, &[held("Mine", 40)], &[], 9))
            .expect_err("nothing may be dropped");
        assert_eq!(error, FULL_SENTENCE);
        assert_eq!(board.store().secrets.len(), MAX_SECRETS_PER_FILE);
    }

    #[test]
    fn retired_keys_go_before_the_new_ones_come() {
        let mut entries: Vec<_> = (1..=15u8)
            .map(|salt| {
                let mut key = held("friends", salt).key;
                key.kind = SecretKind::Password;
                key.entry(1)
            })
            .collect();
        let mut old_account = held("Old account key", 30).key;
        old_account.kind = SecretKind::Account;
        entries.push(old_account.entry(1));
        let board = FakeBoard::with_entries(entries);
        let mut usb = board.usb();
        let synced = block_on(sync_access(
            &mut usb,
            &[held("New account key", 31)],
            &[[30; 16]],
            9,
        ))
        .unwrap();
        assert!(synced.dropped.is_empty());
        assert_eq!(synced.added.len(), 1);
        assert!(synced.listing.entries.iter().any(|e| e.salt == [31; 16]));
    }
}
