//! Who may do what over channel 3 (DM15, Y2, Y8, D3a): one pure function.
//!
//! | Operation | Allowed when |
//! |---|---|
//! | `Q` / `M` | always |
//! | engine install (by hashes) | always (Y8): the bytes must hash to the core's digest slot anyway |
//! | core install (any other offer) | `Trusted` (USB), or a held tier ≥ edit — see QY2 below |
//! | read-back `G` | `Trusted`, or a held tier ≥ play |
//! | resume / takeover of a transfer | the rule of the transfer's kind |
//!
//! The **held tier** of a link that is not `Trusted` is the highest of
//! `OpenTo`'s tier, the tier its core-side login granted, and its
//! `Keyed(tier)`. A `Relayed` link (through the cloud relay) holds only its
//! key's tier and its grant: `OpenTo` never applies over the relay.
//!
//! # QY2 — still open with Yona
//!
//! May a board open to anyone nearby at Author take a core install over
//! radio with no password? [`CORE_INSTALL_FOLLOWS_OPEN_TO`] is the one-line
//! switch, and it ships **yes** (Y2, "like WLED"):
//!
//! - **yes:** a core install follows the held tier like every other edit,
//!   `OpenTo` included;
//! - **no:** a core install on a link that is not `Trusted` ignores `OpenTo`
//!   and needs a core-side login or a key at edit. `Trusted` USB is
//!   unchanged, and engine installs (Y8) are unaffected either way.
//!
//! Why it is Yona's: an accident can always be undone over USB, but
//! malicious firmware can burn the C6's eFuses, including the ones that turn
//! off USB download mode.
//!
//! # The core reads `/.lp/access.json` (doors #14)
//!
//! The core reads only `secrets` and `open`. Changes to those fields stay
//! additive, and no firmware migrates the file on an unconfirmed trial boot.
//! After a rollback, an old core that cannot read a newer file reads it as
//! `locked()` ([`AccessFacts::from_store`]): engine heals still work (no login
//! needed), and core installs over radio wait until the board is back on a
//! build that reads the file.

use alloc::vec::Vec;

use lpc_access::{DeviceAccessFile, OpenTo, SecretEntry, Tier};

use super::board_link::LinkTrust;

/// **QY2's switch.** `true`: a core install follows `OpenTo` like any other
/// edit. `false`: on a link that is not `Trusted`, a core install needs a
/// core-side login or a key at edit, whatever `OpenTo` says.
pub const CORE_INSTALL_FOLLOWS_OPEN_TO: bool = true;

/// What a link asks to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Query,
    EngineInstall,
    CoreInstall,
    ReadBack,
}

/// The access file as the core reads it, once, when the session starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessFacts {
    /// The installed secrets, offered by a core-side login.
    pub secrets: Vec<SecretEntry>,
    /// Who may act without logging in.
    pub open: OpenTo,
    /// QY2 ([`CORE_INSTALL_FOLLOWS_OPEN_TO`] unless a test says otherwise).
    pub core_install_follows_open_to: bool,
}

impl AccessFacts {
    /// From a read access file.
    #[must_use]
    pub fn from_file(file: &DeviceAccessFile) -> Self {
        Self {
            secrets: file.secrets.clone(),
            open: file.open,
            core_install_follows_open_to: CORE_INSTALL_FOLLOWS_OPEN_TO,
        }
    }

    /// From `/.lp/access.json`'s bytes, as the core reads them at start:
    /// no file is a fresh board (`DeviceAccessFile::fresh()`, open at edit);
    /// a file this core cannot read — damaged, held for a layout change, or a
    /// newer shape after a rollback — is `DeviceAccessFile::locked()`.
    #[must_use]
    pub fn from_store(bytes: Option<&[u8]>) -> Self {
        let file = match bytes {
            None => DeviceAccessFile::fresh(),
            Some(b) => {
                DeviceAccessFile::from_json(b).unwrap_or_else(|_| DeviceAccessFile::locked())
            }
        };
        Self::from_file(&file)
    }
}

/// Whether a link with `trust`, which a login (or the engine's server)
/// granted `granted`, may do `op` under `access`.
#[must_use]
pub fn may(op: Operation, trust: LinkTrust, granted: Option<Tier>, access: &AccessFacts) -> bool {
    let needs = match op {
        Operation::Query | Operation::EngineInstall => return true,
        Operation::ReadBack => Tier::Play,
        Operation::CoreInstall => Tier::Edit,
    };
    if trust == LinkTrust::Trusted {
        return true;
    }
    let keyed = match trust {
        LinkTrust::Keyed(tier) => Some(tier),
        LinkTrust::Relayed(tier) => tier,
        LinkTrust::Trusted | LinkTrust::Untrusted => None,
    };
    let open = if matches!(trust, LinkTrust::Relayed(_))
        || (op == Operation::CoreInstall && !access.core_install_follows_open_to)
    {
        None
    } else {
        access.open.tier()
    };
    let held = [open, granted, keyed].into_iter().flatten().max();
    held.is_some_and(|t| t.satisfies(needs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access(open: OpenTo, follows: bool) -> AccessFacts {
        AccessFacts {
            secrets: Vec::new(),
            open,
            core_install_follows_open_to: follows,
        }
    }

    /// Exhaustive: operation × trust × OpenTo × login outcome × QY2.
    #[test]
    fn the_access_table_under_both_answers_to_qy2() {
        let ops = [
            Operation::Query,
            Operation::EngineInstall,
            Operation::CoreInstall,
            Operation::ReadBack,
        ];
        let trusts = [
            LinkTrust::Trusted,
            LinkTrust::Untrusted,
            LinkTrust::Keyed(Tier::Play),
            LinkTrust::Keyed(Tier::Edit),
            LinkTrust::Relayed(None),
            LinkTrust::Relayed(Some(Tier::Play)),
            LinkTrust::Relayed(Some(Tier::Edit)),
        ];
        let opens = [OpenTo::Nobody, OpenTo::Play, OpenTo::Edit];
        let logins = [None, Some(Tier::Play), Some(Tier::Edit)];
        let mut cases = 0;
        for follows in [true, false] {
            for op in ops {
                for trust in trusts {
                    for open in opens {
                        for login in logins {
                            cases += 1;
                            let got = may(op, trust, login, &access(open, follows));
                            let keyed = match trust {
                                LinkTrust::Keyed(t) => Some(t),
                                LinkTrust::Relayed(t) => t,
                                _ => None,
                            };
                            let relayed = matches!(trust, LinkTrust::Relayed(_));
                            let open_counts = !relayed && (op != Operation::CoreInstall || follows);
                            let held = [open_counts.then(|| open.tier()).flatten(), login, keyed]
                                .into_iter()
                                .flatten()
                                .max();
                            let want = match op {
                                Operation::Query | Operation::EngineInstall => true,
                                _ if trust == LinkTrust::Trusted => true,
                                Operation::ReadBack => held.is_some(),
                                Operation::CoreInstall => held == Some(Tier::Edit),
                            };
                            assert_eq!(
                                got, want,
                                "{op:?} {trust:?} open={open:?} login={login:?} follows={follows}"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 2 * 4 * 7 * 3 * 3);
    }

    #[test]
    fn qy2_is_the_one_difference_between_the_two_answers() {
        // A fresh board open at Author, an untrusted radio link, no login.
        let yes = access(OpenTo::Edit, true);
        let no = access(OpenTo::Edit, false);
        assert!(may(
            Operation::CoreInstall,
            LinkTrust::Untrusted,
            None,
            &yes
        ));
        assert!(!may(
            Operation::CoreInstall,
            LinkTrust::Untrusted,
            None,
            &no
        ));
        // A login at edit, or USB, works under either answer.
        assert!(may(
            Operation::CoreInstall,
            LinkTrust::Untrusted,
            Some(Tier::Edit),
            &no
        ));
        assert!(may(Operation::CoreInstall, LinkTrust::Trusted, None, &no));
        // Heals (Y8) never need anything.
        assert!(may(
            Operation::EngineInstall,
            LinkTrust::Untrusted,
            None,
            &access(OpenTo::Nobody, false)
        ));
        assert!(
            CORE_INSTALL_FOLLOWS_OPEN_TO,
            "ships as yes until Yona answers QY2"
        );
    }

    /// The relay's second lock: a board open at edit gives a relayed link
    /// nothing — a play key reads back and may not install a core, an edit
    /// key may, and the board's own engine heals for anyone (Y8).
    #[test]
    fn over_the_relay_open_never_applies() {
        let open = access(OpenTo::Edit, true);
        assert!(!may(
            Operation::ReadBack,
            LinkTrust::Relayed(None),
            None,
            &open
        ));
        assert!(!may(
            Operation::CoreInstall,
            LinkTrust::Relayed(Some(Tier::Play)),
            None,
            &open
        ));
        assert!(may(
            Operation::ReadBack,
            LinkTrust::Relayed(Some(Tier::Play)),
            None,
            &open
        ));
        assert!(may(
            Operation::CoreInstall,
            LinkTrust::Relayed(Some(Tier::Edit)),
            None,
            &open
        ));
        assert!(may(
            Operation::CoreInstall,
            LinkTrust::Relayed(None),
            Some(Tier::Edit),
            &open
        ));
        assert!(may(
            Operation::EngineInstall,
            LinkTrust::Relayed(None),
            None,
            &open
        ));
    }

    #[test]
    fn a_store_the_core_cannot_read_is_locked_and_a_missing_one_is_fresh() {
        let unreadable = AccessFacts::from_store(Some(b"{\"version\": 99, \"whatever\": true}"));
        assert_eq!(unreadable.open, DeviceAccessFile::locked().open);
        assert!(!may(
            Operation::CoreInstall,
            LinkTrust::Untrusted,
            None,
            &unreadable
        ));
        assert!(may(
            Operation::EngineInstall,
            LinkTrust::Untrusted,
            None,
            &unreadable
        ));
        let damaged = AccessFacts::from_store(Some(b"\x00\x01not json"));
        assert_eq!(damaged.open, DeviceAccessFile::locked().open);
        let missing = AccessFacts::from_store(None);
        assert_eq!(missing.open, DeviceAccessFile::fresh().open);
    }
}
