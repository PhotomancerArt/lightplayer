//! "Your browsers & account": the device's keys, folded by name.
//!
//! Every dev-server origin is its own browser, so a desk board lists
//! "Brave on Mac" a dozen times. The panel shows each (kind, name) once,
//! with ×N and the span of dates; its trash can removes the lot. This
//! browser's own key is always its own row, first; then other browsers;
//! then the signed-in account's key and passwords; then anything else
//! (another account's key). The device's Play and Author passwords are not
//! here — they are the lines above.

use lpc_access::{SALT_BYTES, SecretKind};

use super::account_keys::AccountKeys;
use super::device_access_ops::AccessListing;
use super::ui_access_view::UiKeyGroup;

/// Group `listing`'s entries, leaving out the `passwords` (the device's
/// own, shown as the Play and Author lines).
pub fn key_groups(
    listing: &AccessListing,
    passwords: &[[u8; SALT_BYTES]],
    this_browser: Option<[u8; SALT_BYTES]>,
    account: Option<&AccountKeys>,
) -> Vec<UiKeyGroup> {
    let mut groups: Vec<UiKeyGroup> = Vec::new();
    for entry in &listing.entries {
        if passwords.contains(&entry.salt) {
            continue;
        }
        let is_this_browser = this_browser == Some(entry.salt);
        let is_account = account.is_some_and(|account| account.owns_salt(&entry.salt));
        let same = groups.iter_mut().find(|group| {
            !is_this_browser
                && !group.is_this_browser
                && group.kind == entry.kind
                && group.label == entry.label
                && group.tier == entry.tier
                && group.is_account == is_account
        });
        match same {
            Some(group) => {
                group.salts.push(entry.salt);
                group.first_added = earliest(group.first_added, entry.added_at);
                group.last_added = group.last_added.max(entry.added_at);
            }
            None => groups.push(UiKeyGroup {
                label: entry.label.clone(),
                kind: entry.kind,
                tier: entry.tier,
                salts: vec![entry.salt],
                is_this_browser,
                is_account,
                first_added: entry.added_at,
                last_added: entry.added_at,
            }),
        }
    }
    groups.sort_by_key(rank);
    groups
}

fn rank(group: &UiKeyGroup) -> u8 {
    match group {
        group if group.is_this_browser => 0,
        group if group.kind == SecretKind::Browser => 1,
        group if group.is_account && group.kind == SecretKind::Account => 2,
        group if group.is_account => 3,
        _ => 4,
    }
}

/// The earlier of two times, where no time is unknown, not earliest.
fn earliest(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_access::{OpenTo, SecretEntry, Tier};
    use lpc_wire::server::AccessEntryInfo;

    #[test]
    fn eleven_origins_fold_into_one_row_and_this_browser_leads() {
        let mut entries = vec![entry("Bluefy on iPhone", SecretKind::Browser, 1, 5)];
        for salt in 2..=12 {
            entry_into(&mut entries, "Brave on Mac", salt, u64::from(salt) * 10);
        }
        entries.push(entry("friends", SecretKind::Password, 20, 1));
        let listing = AccessListing {
            ble_enabled: true,
            open: OpenTo::Nobody,
            entries,
        };
        let groups = key_groups(&listing, &[[20; 16]], Some([7; 16]), None);
        let rows: Vec<(&str, usize, bool)> = groups
            .iter()
            .map(|g| (g.label.as_str(), g.count(), g.is_this_browser))
            .collect();
        assert_eq!(
            rows,
            [
                ("Brave on Mac", 1, true),
                ("Bluefy on iPhone", 1, false),
                ("Brave on Mac", 10, false),
            ]
        );
        assert_eq!(groups[2].first_added, Some(20));
        assert_eq!(groups[2].last_added, Some(120));
    }

    fn entry_into(entries: &mut Vec<AccessEntryInfo>, label: &str, salt: u8, at: u64) {
        entries.push(entry(label, SecretKind::Browser, salt, at));
    }

    fn entry(label: &str, kind: SecretKind, salt: u8, at: u64) -> AccessEntryInfo {
        AccessEntryInfo::from(
            &SecretEntry::from_password(label, Tier::Edit, b"x", [salt; 16], 1)
                .with_kind(kind)
                .with_added_at(at),
        )
    }
}
