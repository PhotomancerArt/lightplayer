//! **What the person asked for** (DS6): an [`UpdateIntent`] the driver
//! carries, applied as one step after [`decide`](crate::decide()) by
//! [`decide_for_intent`], so the decision table stays the automatic answer
//! and a deliberate choice is a separate, small table:
//!
//! | Intent | Decision | Becomes |
//! |---|---|---|
//! | `Auto` | any | itself |
//! | `Install` | `Heal` of the host's own build (the board runs its core) | itself: finishing |
//! | `Install` | `Heal` of another build (E13: waiting for an engine the host is not restoring) | the offer rows for the host's build (a core install) |
//! | `Install` | `ReportCrashing` | the offer rows; `Nothing` (the board holds this build) stays `ReportCrashing` — that is `Reinstall` |
//! | `Install { allow_downgrade }` | `BoardIsNewer`, `OfferUpdate` | the offer rows, downgrade as the intent says |
//! | `Reinstall` | `ReportCrashing` | `Reinstall`: the board's own engine, by hashes |
//! | `Install`, `Reinstall` | anything else | itself |
//!
//! No intent overrides `OtherTarget`, `RefusedBuild` (the board answers a
//! core install of its refused build `N`/`F` forever: QY2), `NeedsUsb`,
//! `Busy`, `NoUpdateForPlayOnly`, `ContinueUpdate`, nor a refusal the board
//! sends. `Install` needs no `go` (the person's press is the go);
//! `Reinstall`'s own engine install needs none either, and on any other
//! decision it is `Auto` (an offered update still waits for `go`).

use crate::board_view::BoardView;
use crate::decide::decision::{Decision, HostFacts, decide_offer};

/// What the person asked the driver to do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdateIntent {
    /// The decision table as it stands: heal, continue, an offered update
    /// once the caller says go, report the rest.
    #[default]
    Auto,
    /// Put the driver's own build on the board (Update, "Install X",
    /// "Other version…"): past a needs-engine board's heal of another
    /// build (E13), a crashing engine (E10), and — with `allow_downgrade`,
    /// the person chose an older build (N9) — a newer board (E7).
    Install { allow_downgrade: bool },
    /// Write the crashing board's own engine again (E10), from the host's
    /// build when it is the board's, else from the cache or the store. The
    /// driver does this at most once.
    Reinstall,
}

impl UpdateIntent {
    /// Whether this intent lets the offer rows go to an older build.
    #[must_use]
    pub fn allows_downgrade(self) -> bool {
        matches!(
            self,
            Self::Install {
                allow_downgrade: true
            }
        )
    }
}

/// `decision` (from [`decide`](crate::decide())) as `intent` changes it. See
/// the module docs.
#[must_use]
pub fn decide_for_intent(
    decision: Decision,
    board: &BoardView,
    host: &HostFacts<'_>,
    intent: UpdateIntent,
) -> Decision {
    match intent {
        UpdateIntent::Auto => decision,
        UpdateIntent::Install { allow_downgrade } => {
            let host = HostFacts {
                allow_downgrade,
                ..*host
            };
            let finishing_own = board.core_sha256() == Some(host.build.core.sha256);
            match decision {
                Decision::Heal { .. } if finishing_own => decision,
                Decision::Heal { .. } | Decision::BoardIsNewer | Decision::OfferUpdate { .. } => {
                    decide_offer(board, &host)
                }
                Decision::ReportCrashing { .. } => match decide_offer(board, &host) {
                    Decision::Nothing => decision,
                    offer => offer,
                },
                other => other,
            }
        }
        UpdateIntent::Reinstall => match (decision, board.engine_sha256()) {
            (Decision::ReportCrashing { build_id }, Some(engine_sha)) => Decision::Reinstall {
                engine_sha,
                build_id,
            },
            (other, _) => other,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    use lpc_access::Tier;
    use lpc_update::{BoardManifest, BoardState, PieceKind, TransferView, sha256_to_hex};

    use crate::decide::decision::{NeedsUsbWhy, decide};
    use crate::host_build::HostIdentity;
    use crate::host_build_facts::{HostBuildFacts, HostPieceFacts};

    const INSTALL: UpdateIntent = UpdateIntent::Install {
        allow_downgrade: false,
    };
    const DOWNGRADE: UpdateIntent = UpdateIntent::Install {
        allow_downgrade: true,
    };
    const ALL: [UpdateIntent; 4] = [
        UpdateIntent::Auto,
        INSTALL,
        DOWNGRADE,
        UpdateIntent::Reinstall,
    ];

    #[test]
    fn auto_is_the_table_unchanged() {
        for m in [crashing(), needs_engine(), newer(), board_x()] {
            let d = decide_as(&m, UpdateIntent::Auto, None);
            assert_eq!(d, decide_plain(&m, None));
        }
    }

    #[test]
    fn install_passes_a_heal_of_another_build_e13() {
        assert_eq!(decide_plain(&needs_engine(), None), heal_of_x());
        assert_eq!(decide_as(&needs_engine(), INSTALL, None), offer_y());
    }

    #[test]
    fn install_keeps_a_heal_of_its_own_build() {
        // The new core on trial fetching its engine: finishing, not a new
        // core install.
        let mut m = needs_engine();
        m.state = BoardState::OnTrial;
        m.core_sha256 = sha256_to_hex(&[0xBB; 32]);
        m.engine_sha256 = sha256_to_hex(&[0xBE; 32]);
        m.build_id = y().identity.build_id;
        for intent in ALL {
            assert!(
                matches!(decide_as(&m, intent, None), Decision::Heal { .. }),
                "{intent:?}"
            );
        }
    }

    #[test]
    fn install_passes_a_crashing_engine_unless_the_board_holds_its_build() {
        assert_eq!(decide_as(&crashing(), INSTALL, None), offer_y());
        let mut same = crashing();
        same.core_sha256 = sha256_to_hex(&[0xBB; 32]);
        same.engine_sha256 = sha256_to_hex(&[0xBE; 32]);
        same.build_id = y().identity.build_id;
        assert!(matches!(
            decide_as(&same, INSTALL, None),
            Decision::ReportCrashing { .. }
        ));
    }

    #[test]
    fn install_passes_a_newer_board_only_with_allow_downgrade() {
        assert_eq!(decide_as(&newer(), INSTALL, None), Decision::BoardIsNewer);
        assert!(matches!(
            decide_as(&newer(), DOWNGRADE, None),
            Decision::OfferUpdate { .. }
        ));
        // The intent's flag governs, whatever the facts said.
        let y = y();
        let facts = HostFacts {
            build: &y,
            user_tier: None,
            allow_downgrade: true,
        };
        let board = BoardView::from_manifest(newer());
        let d = decide(&board, &facts);
        assert!(matches!(d, Decision::OfferUpdate { .. }));
        assert_eq!(
            decide_for_intent(d, &board, &facts, INSTALL),
            Decision::BoardIsNewer
        );
    }

    #[test]
    fn no_intent_passes_what_the_board_or_the_user_would_refuse() {
        let mut other_target = board_x();
        other_target.target = "esp32c6-8mb-variant".into();
        let mut refused = board_x();
        refused.refused_build = Some(y().build_hash());
        let mut crashing_refused = crashing();
        crashing_refused.refused_build = Some(y().build_hash());
        let mut busy = needs_engine();
        busy.state = BoardState::Updating;
        busy.transfer = Some(TransferView {
            kind: PieceKind::Core,
            done: 4096,
            total: 20_000,
            busy: true,
            build_hash: 7,
        });
        let mut continuing = busy.clone();
        if let Some(t) = &mut continuing.transfer {
            t.busy = false;
            t.build_hash = y().build_hash();
        }
        let mut old_loader = needs_engine();
        old_loader.loader = 0;
        let refused_y = Decision::RefusedBuild {
            build: y().build_hash(),
        };
        let cases: [(BoardManifest, Option<Tier>, Decision); 7] = [
            (
                other_target,
                None,
                Decision::OtherTarget {
                    board_target: "esp32c6-8mb-variant".into(),
                },
            ),
            (refused, None, refused_y.clone()),
            (crashing_refused, None, refused_y),
            (
                busy,
                None,
                Decision::Busy {
                    done: 4096,
                    total: 20_000,
                },
            ),
            (
                continuing,
                None,
                Decision::ContinueUpdate {
                    to: y().identity.build_id,
                },
            ),
            (
                old_loader,
                None,
                Decision::NeedsUsb {
                    why: NeedsUsbWhy::LoaderTooOld { have: 0, need: 1 },
                },
            ),
            (
                needs_engine(),
                Some(Tier::Play),
                Decision::NoUpdateForPlayOnly,
            ),
        ];
        for (m, tier, want) in cases {
            for intent in [INSTALL, DOWNGRADE] {
                assert_eq!(decide_as(&m, intent, tier), want, "{intent:?} on {m:?}");
            }
        }
        let absent = BoardView::absent();
        let y = y();
        let facts = HostFacts {
            build: &y,
            user_tier: None,
            allow_downgrade: false,
        };
        for intent in ALL {
            let d = decide_for_intent(decide(&absent, &facts), &absent, &facts, intent);
            assert!(matches!(d, Decision::NeedsUsb { .. }), "{intent:?}");
        }
    }

    #[test]
    fn reinstall_turns_only_a_crashing_engine_into_an_engine_install() {
        assert_eq!(
            decide_as(&crashing(), UpdateIntent::Reinstall, None),
            Decision::Reinstall {
                engine_sha: [0xAE; 32],
                build_id: x_id(),
            }
        );
        for m in [needs_engine(), newer(), board_x()] {
            assert_eq!(
                decide_as(&m, UpdateIntent::Reinstall, None),
                decide_plain(&m, None),
                "Reinstall is Auto on {m:?}"
            );
        }
    }

    #[test]
    fn only_install_allows_a_downgrade() {
        assert!(DOWNGRADE.allows_downgrade());
        for intent in [UpdateIntent::Auto, INSTALL, UpdateIntent::Reinstall] {
            assert!(!intent.allows_downgrade());
        }
    }

    // ---- Helpers ----------------------------------------------------------------

    /// Y, by its facts alone: no bytes (what a card holds).
    fn y() -> HostBuildFacts {
        HostBuildFacts::from_parts(
            HostIdentity {
                target: "esp32c6-4mb".into(),
                chip: "esp32c6".into(),
                version: "2026.10.06-1".into(),
                build_id: "2026.10.06-1+bbbbbbbbbbbb".into(),
                wire_proto: 36,
                layout: 1,
                min_loader: 1,
            },
            HostPieceFacts {
                sha256: [0xBB; 32],
                len: 20_000,
            },
            HostPieceFacts {
                sha256: [0xBE; 32],
                len: 40_000,
            },
        )
    }

    fn x_id() -> String {
        "2026.10.05-1+aaaaaaaaaaaa".into()
    }

    /// A board running X, a release older than Y.
    fn board_x() -> BoardManifest {
        BoardManifest {
            proto: 1,
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: "2026.10.05-1".into(),
            build_id: x_id(),
            wire_proto: 36,
            core_sha256: "aa".repeat(32),
            core_len: 18_000,
            engine_sha256: "ae".repeat(32),
            engine_len: Some(38_000),
            layout: 1,
            loader: 1,
            region_len: 3_375_104,
            state: BoardState::Running,
            refused_build: None,
            transfer: None,
        }
    }

    fn needs_engine() -> BoardManifest {
        BoardManifest {
            state: BoardState::NeedsEngine,
            engine_len: None,
            ..board_x()
        }
    }

    fn crashing() -> BoardManifest {
        BoardManifest {
            state: BoardState::EngineCrashing,
            ..board_x()
        }
    }

    fn newer() -> BoardManifest {
        BoardManifest {
            version: "2026.10.07-1".into(),
            ..board_x()
        }
    }

    fn heal_of_x() -> Decision {
        Decision::Heal {
            engine_sha: [0xAE; 32],
            build_id: x_id(),
        }
    }

    fn offer_y() -> Decision {
        Decision::OfferUpdate {
            from: x_id(),
            to: y().identity.build_id,
        }
    }

    /// The table alone, no intent step.
    fn decide_plain(m: &BoardManifest, tier: Option<Tier>) -> Decision {
        let y = y();
        let facts = HostFacts {
            build: &y,
            user_tier: tier,
            allow_downgrade: false,
        };
        decide(&BoardView::from_manifest(m.clone()), &facts)
    }

    fn decide_as(m: &BoardManifest, intent: UpdateIntent, tier: Option<Tier>) -> Decision {
        let y = y();
        let facts = HostFacts {
            build: &y,
            user_tier: tier,
            allow_downgrade: false,
        };
        let board = BoardView::from_manifest(m.clone());
        decide_for_intent(decide(&board, &facts), &board, &facts, intent)
    }
}
