//! The access panel's Play and Author lines: the device's two passwords and its
//! `open` setting, read off a listing and changed together.
//!
//! The panel offers two lines, each Anyone or Password. Anyone is the
//! device's [`OpenTo`]; Password is the device's password for that tier.
//! One rule ties them: anyone who can author can play, so while Author is
//! Anyone, Play follows it.
//!
//! **Which entries are "the play password".** Every `password` entry at that
//! tier that the signed-in account does not own (account passwords are
//! the account's, and sit with its key in the list below). Setting the
//! password replaces all of them with one entry, labelled
//! [`password_label`], at a fresh salt; Anyone removes them all. So an
//! older named password ("friends") is the play password too, until it is
//! replaced — there is one password per tier, and no names to choose.

use lpc_access::{OpenTo, SALT_BYTES, SecretKind, Tier};

use super::account_keys::AccountKeys;
use super::device_access_ops::{AccessListing, AccessOp};
use super::device_access_record::DeviceAccessRecord;
use super::key_holder::InstallableKey;
use super::login_key_cache::DEFAULT_KDF_ITERATIONS;
use super::ui_access_view::UiPasswordLine;

/// What the panel says after Author goes to Anyone with Play on Password.
pub const PLAY_FOLLOWS_NOTICE: &str = "Author is open now, so play is too.";

/// The label a password set from the panel is stored under.
pub fn password_label(tier: Tier) -> &'static str {
    match tier {
        Tier::Play => "Play password",
        Tier::Edit => "Author password",
    }
}

/// The salts of the device's own passwords at `tier` (see the module doc).
pub fn device_password_salts(
    listing: &AccessListing,
    tier: Tier,
    account: Option<&AccountKeys>,
) -> Vec<[u8; SALT_BYTES]> {
    listing
        .entries
        .iter()
        .filter(|entry| entry.kind == SecretKind::Password && entry.tier == tier)
        .filter(|entry| !account.is_some_and(|account| account.owns_salt(&entry.salt)))
        .map(|entry| entry.salt)
        .collect()
}

/// A Play or Author line's change, as ops on the device and the notice to
/// show once it lands.
#[derive(Debug, PartialEq, Eq)]
pub struct PasswordPlan {
    pub ops: Vec<AccessOp>,
    /// The new entry's salt and password, so this browser can show it.
    pub set: Option<([u8; SALT_BYTES], String)>,
    pub notice: Option<&'static str>,
}

/// Set `tier`'s line to Password (`Some`) or Anyone (`None`).
///
/// - Author → Anyone: open at edit, and both passwords go (Play follows).
/// - Author → Password: the password replaces any other; a device open at
///   edit becomes open at play (Play stays Anyone).
/// - Play → Anyone: open at play; the play password goes.
/// - Play → Password: the password replaces any other, and the device is
///   open to nobody. Refused while Author is Anyone.
///
/// A password is added before the device is closed to anyone, so a failed
/// add never leaves the device shut with no password for that tier.
pub fn plan_password(
    listing: &AccessListing,
    tier: Tier,
    password: Option<&str>,
    new_salt: [u8; SALT_BYTES],
    account: Option<&AccountKeys>,
) -> Result<PasswordPlan, String> {
    let open = listing.open;
    let remove = |tier| {
        device_password_salts(listing, tier, account)
            .into_iter()
            .map(AccessOp::Remove)
    };
    let switch = |open: OpenTo| AccessOp::Switches {
        ble_enabled: None,
        open: Some(open),
    };
    let Some(password) = password else {
        let plan = match tier {
            Tier::Edit => PasswordPlan {
                ops: std::iter::once(switch(OpenTo::Edit))
                    .chain(remove(Tier::Edit))
                    .chain(remove(Tier::Play))
                    .collect(),
                set: None,
                notice: (open == OpenTo::Nobody).then_some(PLAY_FOLLOWS_NOTICE),
            },
            Tier::Play => PasswordPlan {
                ops: (open == OpenTo::Nobody)
                    .then(|| switch(OpenTo::Play))
                    .into_iter()
                    .chain(remove(Tier::Play))
                    .collect(),
                set: None,
                notice: None,
            },
        };
        return Ok(plan);
    };
    if password.trim().is_empty() {
        return Err("type a password".to_string());
    }
    if tier == Tier::Play && open == OpenTo::Edit {
        return Err("Play follows Author while anyone nearby can author.".to_string());
    }
    let add = AccessOp::Add(InstallableKey {
        label: password_label(tier).to_string(),
        kind: SecretKind::Password,
        tier,
        salt: new_salt,
        iterations: DEFAULT_KDF_ITERATIONS,
        material: password.as_bytes().to_vec(),
    });
    let closes = match tier {
        Tier::Edit => (open == OpenTo::Edit).then(|| switch(OpenTo::Play)),
        Tier::Play => (open == OpenTo::Play).then(|| switch(OpenTo::Nobody)),
    };
    Ok(PasswordPlan {
        ops: remove(tier)
            .chain(std::iter::once(add))
            .chain(closes)
            .collect(),
        set: Some((new_salt, password.to_string())),
        notice: None,
    })
}

/// The panel's Play and Author lines, read off `record`'s listing.
pub fn password_lines(
    record: &DeviceAccessRecord,
    account: Option<&AccountKeys>,
) -> (UiPasswordLine, UiPasswordLine) {
    let listing = &record.listing;
    let line = |tier| {
        let salts = device_password_salts(listing, tier, account);
        if salts.is_empty() {
            return UiPasswordLine::NotSet;
        }
        listing
            .entries
            .iter()
            .filter(|entry| salts.contains(&entry.salt))
            .find_map(|entry| record.password_for(entry))
            .map_or(UiPasswordLine::SetElsewhere, |password| {
                UiPasswordLine::Shown(password.to_string())
            })
    };
    match listing.open {
        OpenTo::Edit => (UiPasswordLine::FollowsAuthor, UiPasswordLine::Anyone),
        OpenTo::Play => (UiPasswordLine::Anyone, line(Tier::Edit)),
        OpenTo::Nobody => (line(Tier::Play), line(Tier::Edit)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::device_access_record::SetHere;
    use lpc_wire::server::AccessEntryInfo;

    #[test]
    fn the_lines_follow_open_and_show_only_what_was_set_here() {
        let mut record = DeviceAccessRecord {
            listing: listing(OpenTo::Nobody),
            listed_at: 0.0,
            restart_pending: false,
            set_here: vec![SetHere {
                salt: [1; 16],
                password: "camp-glow-17".to_string(),
            }],
        };
        assert_eq!(
            password_lines(&record, None),
            (
                UiPasswordLine::Shown("camp-glow-17".to_string()),
                UiPasswordLine::SetElsewhere
            )
        );
        record.listing.open = OpenTo::Play;
        assert_eq!(password_lines(&record, None).0, UiPasswordLine::Anyone);
        record.listing.open = OpenTo::Edit;
        assert_eq!(
            password_lines(&record, None),
            (UiPasswordLine::FollowsAuthor, UiPasswordLine::Anyone)
        );
        record.listing.open = OpenTo::Nobody;
        record
            .listing
            .entries
            .retain(|entry| entry.tier == Tier::Edit);
        assert_eq!(password_lines(&record, None).0, UiPasswordLine::NotSet);
    }

    #[test]
    fn author_anyone_opens_at_edit_and_drops_both_passwords() {
        let plan =
            plan_password(&listing(OpenTo::Nobody), Tier::Edit, None, [9; 16], None).unwrap();
        assert_eq!(
            plan.ops,
            [
                AccessOp::Switches {
                    ble_enabled: None,
                    open: Some(OpenTo::Edit)
                },
                AccessOp::Remove([2; 16]),
                AccessOp::Remove([1; 16]),
            ]
        );
        assert_eq!(plan.notice, Some(PLAY_FOLLOWS_NOTICE));
        let quiet = plan_password(&listing(OpenTo::Play), Tier::Edit, None, [9; 16], None).unwrap();
        assert_eq!(quiet.notice, None, "play was already Anyone");
    }

    #[test]
    fn a_password_replaces_the_old_one_then_closes() {
        let plan = plan_password(
            &listing(OpenTo::Play),
            Tier::Play,
            Some("camp-glow-17"),
            [9; 16],
            None,
        )
        .unwrap();
        assert!(matches!(
            plan.ops.as_slice(),
            [
                AccessOp::Remove(old),
                AccessOp::Add(key),
                AccessOp::Switches { open: Some(OpenTo::Nobody), .. },
            ] if *old == [1; 16] && key.salt == [9; 16] && key.label == "Play password"
                && key.tier == Tier::Play && key.kind == SecretKind::Password
        ));
        assert_eq!(plan.set, Some(([9; 16], "camp-glow-17".to_string())));
    }

    #[test]
    fn author_password_on_an_open_board_leaves_play_open() {
        let mut open = listing(OpenTo::Edit);
        open.entries.clear();
        let plan = plan_password(&open, Tier::Edit, Some("x"), [9; 16], None).unwrap();
        assert!(matches!(
            plan.ops.as_slice(),
            [
                AccessOp::Add(_),
                AccessOp::Switches {
                    open: Some(OpenTo::Play),
                    ..
                }
            ]
        ));
    }

    #[test]
    fn play_follows_author_and_a_blank_password_is_refused() {
        assert!(
            plan_password(&listing(OpenTo::Edit), Tier::Play, Some("x"), [9; 16], None).is_err()
        );
        assert!(
            plan_password(
                &listing(OpenTo::Nobody),
                Tier::Edit,
                Some("  "),
                [9; 16],
                None
            )
            .is_err()
        );
    }

    #[test]
    fn play_anyone_only_opens_a_closed_board() {
        let plan =
            plan_password(&listing(OpenTo::Nobody), Tier::Play, None, [9; 16], None).unwrap();
        assert_eq!(
            plan.ops,
            [
                AccessOp::Switches {
                    ble_enabled: None,
                    open: Some(OpenTo::Play)
                },
                AccessOp::Remove([1; 16]),
            ]
        );
    }

    /// A play password (salt 1s), an author password (2s) and a browser key
    /// (3s), open to `open`.
    fn listing(open: OpenTo) -> AccessListing {
        let entry = |label: &str, kind, tier, salt| {
            let mut info = AccessEntryInfo::from(&lpc_access::SecretEntry::from_password(
                label, tier, b"x", [salt; 16], 1,
            ));
            info.kind = kind;
            info
        };
        AccessListing {
            ble_enabled: true,
            open,
            entries: vec![
                entry("friends", SecretKind::Password, Tier::Play, 1),
                entry("Author password", SecretKind::Password, Tier::Edit, 2),
                entry("Brave on Mac", SecretKind::Browser, Tier::Edit, 3),
            ],
        }
    }
}
