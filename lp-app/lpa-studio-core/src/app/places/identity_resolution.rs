//! A1–A4: which identity a live session wears, from the evidence it
//! offers.
//!
//! Design: `~/.photomancer/planning/lp2025/2026-08-04-1748-hardware-anchored-device-identity/design.md`
//! §3 (the acquisition table) and §4 (the lazy re-key). The rules, in
//! precedence order:
//!
//! | Rule | Source | Yields |
//! |---|---|---|
//! | A1 | the hello's `HardwareFacts::base_mac` | [`HardwareId::EspEfuse`] |
//! | A2 | a download-mode ROM MAC read (`DeviceSnapshot::probed_mac`, already normalized) | [`HardwareId::EspEfuse`] |
//! | A3 | the stamped uid — `/.lp/device.json`, else the hello's `device_uid` | [`HardwareId::Minted`] |
//! | A4 | none of the above | anonymous: the session keys by `runtime-N`, exactly today |
//!
//! The order is a contract, not a preference: silicon outranks a stamp
//! because the stamp is erasable and the silicon is not. A board that
//! answers with BOTH (the migration case) resolves to its derived uid and
//! reports the stamped one as [`ResolvedIdentity::rekey_from`], which is
//! what the caller feeds
//! [`DeviceRegistry::rekey_or_merge`](super::DeviceRegistry::rekey_or_merge).
//!
//! Pure by design — no wire, no registry, no clock. The controller
//! gathers the evidence and acts on the answer.

use lpc_history::{PrefixedUid, UidPrefix};

use super::HardwareId;

/// What one session says about its own identity, gathered at connect.
/// Every field is optional: absence is the normal case for some transport
/// class or firmware age, never an error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdentityEvidence {
    /// A1: the hello's `HardwareFacts::base_mac`, as reported.
    pub hello_base_mac: Option<String>,
    /// A2: the base MAC a download-mode read banked on this session
    /// (`lpa_link::DeviceSnapshot::probed_mac`) — already normalized by
    /// `lpa_link::normalize_base_mac`, re-parsed here anyway because the
    /// rule belongs to whoever mints an identity from it.
    pub probed_mac: Option<String>,
    /// A3: the stamped `dev…` uid, from `/.lp/device.json` when the file
    /// exists, else the hello's `device_uid`. Unparseable text is treated
    /// as absent (a device may hold anything).
    pub stamped_uid: Option<String>,
    /// The name the legacy identity file carried, if any. Never authority
    /// (D34: the registry names devices) — a display fallback for a board
    /// with no registry row yet.
    pub file_name: Option<String>,
}

/// The identity a session resolved to, plus the migration it implies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIdentity {
    /// Where the identity came from — the registry row's origin column.
    pub hardware_id: HardwareId,
    /// The registry key: `hardware_id.device_uid()` (design §2, I2).
    pub uid: PrefixedUid,
    /// A stamped uid seen alongside a DERIVED one, i.e. this board was
    /// remembered under the old scheme. The caller re-keys the registry
    /// row from it before recording the sighting (design §4). `None`
    /// when nothing was stamped, or when the stamp IS the identity (A3 —
    /// there is nothing to migrate from).
    pub rekey_from: Option<PrefixedUid>,
}

/// Resolve one session's identity per the A1–A4 table. `None` is A4: the
/// board stays session-scoped, which is exactly today's unstamped
/// behavior and not a failure.
pub fn resolve_identity(evidence: &IdentityEvidence) -> Option<ResolvedIdentity> {
    let stamped = evidence.stamped_uid.as_deref().and_then(parse_device_uid);
    // A1 then A2: either is silicon, and the hello is the reading we
    // trust first because it comes from the running firmware rather than
    // across the esptool-js boundary.
    let silicon = evidence
        .hello_base_mac
        .as_deref()
        .and_then(HardwareId::from_base_mac)
        .or_else(|| {
            evidence
                .probed_mac
                .as_deref()
                .and_then(HardwareId::from_base_mac)
        });
    if let Some(hardware_id) = silicon {
        let uid = hardware_id.device_uid();
        return Some(ResolvedIdentity {
            hardware_id,
            uid,
            rekey_from: stamped.filter(|stamped| *stamped != uid),
        });
    }
    // A3: no silicon to anchor to — the stamped uid IS the identity
    // (host-class embedders and pre-2026-08-03 firmware).
    let uid = stamped?;
    Some(ResolvedIdentity {
        hardware_id: HardwareId::Minted { uid },
        uid,
        rekey_from: None,
    })
}

/// Parse a `dev…` uid, rejecting anything else — including a well-formed
/// uid of the wrong family (a `prj` string is not a device).
fn parse_device_uid(s: &str) -> Option<PrefixedUid> {
    let uid: PrefixedUid = s.parse().ok()?;
    (uid.prefix() == UidPrefix::Device).then_some(uid)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: &str = "aa:bb:cc:dd:ee:ff";
    const STAMPED: &str = "devaaaaaaaaaaaaaaaa";

    #[test]
    fn a1_the_hello_mac_wins_and_derives_the_uid() {
        let resolved = resolve_identity(&IdentityEvidence {
            hello_base_mac: Some(MAC.to_string()),
            ..IdentityEvidence::default()
        })
        .expect("a MAC is an identity");
        let expected = HardwareId::from_base_mac(MAC).unwrap();
        assert_eq!(resolved.hardware_id, expected);
        assert_eq!(resolved.uid, expected.device_uid());
        assert_eq!(resolved.rekey_from, None);
    }

    #[test]
    fn a2_the_probed_mac_answers_when_the_hello_has_none() {
        let resolved = resolve_identity(&IdentityEvidence {
            probed_mac: Some(MAC.to_string()),
            ..IdentityEvidence::default()
        })
        .expect("probe evidence is silicon too");
        assert_eq!(
            resolved.hardware_id,
            HardwareId::from_base_mac(MAC).unwrap()
        );
    }

    #[test]
    fn a1_outranks_a2_when_both_are_present() {
        // same session, two readers: the running firmware is the one that
        // cannot have crossed the esptool-js boundary.
        let resolved = resolve_identity(&IdentityEvidence {
            hello_base_mac: Some(MAC.to_string()),
            probed_mac: Some("00:11:22:33:44:55".to_string()),
            ..IdentityEvidence::default()
        })
        .unwrap();
        assert_eq!(
            resolved.hardware_id,
            HardwareId::from_base_mac(MAC).unwrap()
        );
    }

    #[test]
    fn silicon_outranks_a_stamp_and_reports_the_rekey_source() {
        // the migration case: a board remembered under its stamped uid
        // shows up with a MAC. Identity is the MAC; the stamp becomes the
        // registry row to move (design §4).
        let resolved = resolve_identity(&IdentityEvidence {
            hello_base_mac: Some(MAC.to_string()),
            stamped_uid: Some(STAMPED.to_string()),
            ..IdentityEvidence::default()
        })
        .unwrap();
        assert_eq!(
            resolved.uid,
            HardwareId::from_base_mac(MAC).unwrap().device_uid()
        );
        assert_eq!(
            resolved.rekey_from.map(|uid| uid.to_string()),
            Some(STAMPED.to_string())
        );
    }

    #[test]
    fn a_stamp_that_already_equals_the_derived_uid_is_not_a_rekey() {
        // a board stamped by a NEW studio (P4) reports the derived uid
        // back through the file — there is nothing to migrate.
        let derived = HardwareId::from_base_mac(MAC).unwrap().device_uid();
        let resolved = resolve_identity(&IdentityEvidence {
            hello_base_mac: Some(MAC.to_string()),
            stamped_uid: Some(derived.to_string()),
            ..IdentityEvidence::default()
        })
        .unwrap();
        assert_eq!(resolved.rekey_from, None);
    }

    #[test]
    fn a3_falls_back_to_the_stamped_uid() {
        let resolved = resolve_identity(&IdentityEvidence {
            stamped_uid: Some(STAMPED.to_string()),
            file_name: Some("Porch sign".to_string()),
            ..IdentityEvidence::default()
        })
        .expect("host-class / pre-hello-mac firmware keeps its stamp");
        assert_eq!(resolved.uid.to_string(), STAMPED);
        assert!(matches!(resolved.hardware_id, HardwareId::Minted { .. }));
        assert_eq!(resolved.rekey_from, None, "the stamp IS the identity");
    }

    #[test]
    fn a4_no_evidence_is_anonymous() {
        assert_eq!(resolve_identity(&IdentityEvidence::default()), None);
    }

    #[test]
    fn a_failed_efuse_read_is_no_mac_at_all() {
        // all-zero / all-ones are what a failed read reports; they must
        // fall through to A3/A4 rather than collapse every failed board
        // onto one identity.
        let resolved = resolve_identity(&IdentityEvidence {
            hello_base_mac: Some("00:00:00:00:00:00".to_string()),
            stamped_uid: Some(STAMPED.to_string()),
            ..IdentityEvidence::default()
        })
        .unwrap();
        assert_eq!(resolved.uid.to_string(), STAMPED);
        assert!(matches!(resolved.hardware_id, HardwareId::Minted { .. }));

        assert_eq!(
            resolve_identity(&IdentityEvidence {
                hello_base_mac: Some("ff:ff:ff:ff:ff:ff".to_string()),
                ..IdentityEvidence::default()
            }),
            None,
            "no other evidence: anonymous, not a shared identity"
        );
    }

    #[test]
    fn unparseable_stamped_text_counts_as_absent() {
        for stamped in ["", "not-a-uid", "prjaaaaaaaaaaaaaaaa", "devshort"] {
            assert_eq!(
                resolve_identity(&IdentityEvidence {
                    stamped_uid: Some(stamped.to_string()),
                    ..IdentityEvidence::default()
                }),
                None,
                "{stamped:?}"
            );
        }
    }
}
