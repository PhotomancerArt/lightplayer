//! **The decision**: what a host should do with a board, as one pure
//! function of the board's manifest and the host's own facts. M7 can
//! re-order these rules without touching the protocol; they emit facts,
//! never UI actions or copy (DM31).
//!
//! **The board is the compatibility authority** (doors #3): this only
//! *predicts* what the board will accept. It compares facts — the chip word,
//! `layout` equal, the board's `loader` at least the build's
//! `requires.loader`, hashes — and never parses a target name; a host build
//! of another target is [`Decision::OtherTarget`] (no offer by default).
//! **Up to date is decided by hashes.** Versions are ordered by M1's own
//! comparison (`lpa_devices::FirmwareAge`), never a second one.
//!
//! The rules, first match wins:
//!
//! | Board | Decision | Row |
//! |---|---|---|
//! | no manifest, or no split layout / chip this host knows | `NeedsUsb` | E9 |
//! | a transfer owned by another live link | `Busy` | E6 |
//! | a core transfer to the host's build | `ContinueUpdate` | E2, E5 |
//! | waiting for its engine (needs-engine, on trial, an engine transfer, or a core transfer to a build this host does not hold) | `Heal` | E1, E2, E13 |
//! | its engine keeps crashing | `ReportCrashing` | E10 |
//! | another target | `OtherTarget` | — |
//! | the same core and engine hashes | `Nothing` | — |
//! | another chip, layout, or a loader too old | `NeedsUsb` | E8 |
//! | the pieces cannot fit its region | `NeedsUsb` | E8 |
//! | it refused this build after a failed trial | `RefusedBuild` | E3 |
//! | its version is newer (no downgrade asked) | `BoardIsNewer` | E7 |
//! | the user holds only play | `NoUpdateForPlayOnly` | E12 |
//! | otherwise | `OfferUpdate` | — |
//!
//! A heal is never withheld: it needs no login (Y8), so play-only users and
//! refused builds do not stop it. Whether the host can get the engine a heal
//! needs is the engine source's question ([`super::engine_source`]); a miss
//! there is E13.

use alloc::string::String;

use lpa_devices::{AppVersion, FirmwareAge};
use lpc_access::Tier;
use lpc_update::{BoardState, PieceKind};

use crate::board_view::BoardView;
use crate::host_build::HostBuild;

/// What the host knows about itself and its user.
#[derive(Clone, Copy, Debug)]
pub struct HostFacts<'a> {
    /// The build this host would put on the board.
    pub build: &'a HostBuild,
    /// The user's tier on this board, if known (`None`: not known yet — an
    /// offer is still made, and the board says `N`/`A` if it must).
    pub user_tier: Option<Tier>,
    /// Offer a build older than the board's (E7).
    pub allow_downgrade: bool,
}

/// Why a board needs a cable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedsUsbWhy {
    /// No manifest: monolithic or pre-update firmware (E9).
    NoUpdateChannel,
    /// A layout or chip this host does not know.
    UnknownBoard,
    /// The host's build is for another chip.
    OtherChip,
    /// The host's build needs another layout.
    OtherLayout,
    /// The board's loader is older than the build needs.
    LoaderTooOld { have: u16, need: u16 },
    /// The pieces cannot fit the board's region.
    DoesNotFit { need: u64, room: u32 },
}

/// What to do. See the module docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Up to date.
    Nothing,
    /// The host holds a build of another target; nothing is offered by
    /// default (moving a board to another target is a deliberate choice).
    OtherTarget {
        board_target: String,
    },
    /// Heal the board with the engine its core needs: no click (Y6), no
    /// login (Y8).
    Heal {
        engine_sha: [u8; 32],
        build_id: String,
    },
    /// The board holds a pending or running transfer to the host's build:
    /// carry on (E2, E5, DM16).
    ContinueUpdate {
        to: String,
    },
    /// The user's button (M7).
    OfferUpdate {
        from: String,
        to: String,
    },
    /// The engine keeps crashing (E10): report it, never heal on its own.
    ReportCrashing {
        build_id: String,
    },
    NeedsUsb {
        why: NeedsUsbWhy,
    },
    /// The board runs a newer version (E7).
    BoardIsNewer,
    /// Another link holds the board's transfer (E6).
    Busy {
        done: u32,
        total: u32,
    },
    /// The user holds only play on this board (E12): no offer.
    NoUpdateForPlayOnly,
    /// The board refused this build after its trial failed (E3): stop
    /// offering it.
    RefusedBuild {
        build: u32,
    },
}

/// The decision for `board` and `host`. See the module docs.
#[must_use]
pub fn decide(board: &BoardView, host: &HostFacts<'_>) -> Decision {
    let Some(m) = &board.manifest else {
        return needs_usb(NeedsUsbWhy::NoUpdateChannel);
    };
    if !board.can_update_over_link() {
        return needs_usb(NeedsUsbWhy::UnknownBoard);
    }
    let build = host.build;
    let holds_boards_core = board.core_sha256() == Some(build.core.sha256);

    if let Some(t) = board.updating() {
        if t.busy {
            return Decision::Busy {
                done: t.done,
                total: t.total,
            };
        }
        if t.kind == PieceKind::Core && t.build_hash == build.build_hash() && !holds_boards_core {
            return Decision::ContinueUpdate {
                to: build.identity.build_id.clone(),
            };
        }
    }
    let waiting_for_engine = matches!(
        m.state,
        BoardState::NeedsEngine | BoardState::OnTrial | BoardState::Updating
    );
    if waiting_for_engine {
        if let Some(engine_sha) = board.engine_sha256() {
            return Decision::Heal {
                engine_sha,
                build_id: m.build_id.clone(),
            };
        }
        return needs_usb(NeedsUsbWhy::UnknownBoard);
    }
    if m.state == BoardState::EngineCrashing {
        return Decision::ReportCrashing {
            build_id: m.build_id.clone(),
        };
    }
    let id = &build.identity;
    if m.target != id.target {
        return Decision::OtherTarget {
            board_target: m.target.clone(),
        };
    }
    if holds_boards_core && board.engine_sha256() == Some(build.engine.sha256) {
        return Decision::Nothing;
    }
    if m.chip != id.chip {
        return needs_usb(NeedsUsbWhy::OtherChip);
    }
    if m.layout != id.layout {
        return needs_usb(NeedsUsbWhy::OtherLayout);
    }
    if m.loader < id.min_loader {
        return needs_usb(NeedsUsbWhy::LoaderTooOld {
            have: m.loader,
            need: id.min_loader,
        });
    }
    let need = u64::from(build.core.len) + u64::from(build.engine.len);
    if need > u64::from(m.region_len) {
        return needs_usb(NeedsUsbWhy::DoesNotFit {
            need,
            room: m.region_len,
        });
    }
    if m.refused_build == Some(build.build_hash()) {
        return Decision::RefusedBuild {
            build: build.build_hash(),
        };
    }
    let age = FirmwareAge::compare(
        AppVersion::parse(&m.version),
        AppVersion::parse(&id.version),
    );
    if age == FirmwareAge::Newer && !host.allow_downgrade {
        return Decision::BoardIsNewer;
    }
    if host.user_tier == Some(Tier::Play) {
        return Decision::NoUpdateForPlayOnly;
    }
    Decision::OfferUpdate {
        from: m.build_id.clone(),
        to: id.build_id.clone(),
    }
}

fn needs_usb(why: NeedsUsbWhy) -> Decision {
    Decision::NeedsUsb { why }
}
