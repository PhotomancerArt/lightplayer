//! One device as the gallery's *Devices* roster shows it.

use lpc_wire::{BuildFacts, HardwareFacts};

use crate::UiLogEntry;
use crate::app::home::card_ui_state::CardUiState;
use crate::app::roster::RosterCardState;

/// A device card. Visually distinct from package cards by contract: the
/// renderer gives it a hardware header (status circle + transport) so it
/// never reads as "just another project". The card's health lives in
/// [`RosterCardState`] (the 14-state vocabulary, derived from evidence by
/// `derive_roster_card_state`); the project chip is identity, not status.
#[derive(Clone, Debug, PartialEq)]
pub struct UiDeviceCard {
    /// `dev_…` uid when the device is registered; `None` for a live
    /// connection that has no stamped identity yet.
    pub uid: Option<String>,
    pub name: String,
    /// Transport label ("USB" today; a different glyph for networked
    /// later). Empty while a connect is still resolving the provider.
    pub transport: String,
    /// Where the card stands in the honest roster vocabulary.
    pub state: RosterCardState,
    /// The project the device holds (live cards) or last ran (offline
    /// cards) — identity for the header chip, never health.
    pub project: Option<UiDeviceProjectChip>,
    /// Running-firmware build facts from the live link's hello (provenance
    /// + the feature set compiled into the image) — Technical evidence for
    /// the card's rich-object detail; `None` for remembered (offline)
    /// cards and pre-hello links.
    pub fw: Option<BuildFacts>,
    /// What the live link's hello says this UNIT has wired (services,
    /// board identity) — the runtime half of the same report. `None`
    /// wherever `fw` is `None`.
    pub hardware: Option<HardwareFacts>,
    /// Chip identity from passive/probe evidence (M5): the setup form's
    /// board picker leads with matching boards. Distinct from
    /// `hardware.board_id` (the device's own post-provision report).
    pub detected_chip: Option<String>,
    /// Device-level safe-mode output ceiling (0–255) reported by the live
    /// session's heartbeat. A power cycle is the only exit, so the card
    /// must both flag the state AND say how to leave it. `None` when the
    /// device reports no clamp (and always on offline/sim cards).
    pub safe_clamp: Option<u8>,
    /// D36: this card is the live SIMULATOR session, wearing the same card
    /// grammar with the sim presentation (sim glyph, no connect ceremony,
    /// no rename, its own rich-object sections). The sim is not a device
    /// (D22) — `uid` stays `None` and no registry entry ever backs it.
    pub sim: bool,
    /// The session's per-device console tail (D42), oldest first — the
    /// card's console strip and Console tab render this. Always empty on
    /// remembered (offline) cards: no session, no console.
    pub console_tail: Vec<UiLogEntry>,
    /// The card's UI view-state (selected tab, open sheet, in-place op).
    /// Core-owned + keyed by [`Self::identity_key`], so it survives the
    /// card ⇄ pane growth and session replaces. The gallery/lens builder
    /// leaves this default; the controller overlays the persisted state.
    pub ui: CardUiState,
}

impl UiDeviceCard {
    /// The card's CANONICAL identity — the ONE key both the UI-state map
    /// and the scene-fork's `view-transition-name` consume (2026-07-25
    /// alignment). Names are NOT unique (erase + re-provision mints a new
    /// `dev_…` uid under the same name; a keyed list with duplicate keys
    /// panics Dioxus — the 2026-07-15 crash). Registered/stamped cards
    /// key by uid; the (≤1) sim card by a reserved token.
    ///
    /// FALLBACK CAVEAT (fork-flagged, upgrade before/with the morph):
    /// an identity-less LIVE card (a board mid-provision, before its uid
    /// is stamped) still falls back to its display NAME — a rename can
    /// re-key it. The robust fix is to fall back to the session's
    /// `RuntimeId` (stable across the session); that threads the id onto
    /// the card and lands in the fork-coordination pass before P2.
    pub fn identity_key(&self) -> &str {
        if self.sim {
            return "runtime-sim";
        }
        self.uid.as_deref().unwrap_or(&self.name)
    }

    /// Back-compat alias for keyed rendering — the same canonical key.
    pub fn render_key(&self) -> &str {
        self.identity_key()
    }

    /// Whether the CARD-OWNED op flow aimed at `target_uid` rides THIS
    /// card (state-flow model §2). The managed device's stamped uid
    /// addresses its card directly; an identity-less blank board (mid
    /// first-provision, no uid yet) has its op ride the live —
    /// non-offline — hardware card.
    ///
    /// The ONE rule, shared by the two places that must agree: the
    /// controller's view build (`overlay_card_ui`) and the actor's
    /// progressive patch ([`crate::UiStudioView::apply_card_op`]). They
    /// drifted apart once already — a flash's progress reached neither
    /// surface — so both call here.
    pub fn takes_card_op(&self, target_uid: Option<&str>) -> bool {
        if self.sim {
            return false;
        }
        match target_uid {
            Some(uid) => self.uid.as_deref() == Some(uid),
            // A uid-less op belongs to THE anonymous hardware session (there
            // is at most one), so it rides any uid-less card — including the
            // Offline card that `op_in_flight` pins when the session dies
            // mid-op. The offline exclusion only guards uid'd REGISTRY cards
            // of other devices from adopting a stray anonymous op. Before
            // pinned anonymous cards existed, "not offline" was equivalent;
            // once a recovery write killed its own session, the pinned card
            // was offline, the op refused to ride it, and the user got a
            // bare "Not seen yet" instead of the replug instruction (bench,
            // 2026-07-31 — bootloader-mode arm).
            None => self.uid.is_none() || !matches!(self.state, RosterCardState::Offline { .. }),
        }
    }
}

/// The header chip naming the device's project: thumbnail seed + display
/// name. Identity only — the status line and circle carry health. On
/// offline/error cards the renderer mutes it (last-known, not current).
#[derive(Clone, Debug, PartialEq)]
pub struct UiDeviceProjectChip {
    /// `prj_…` uid — thumbnail seed and the push/review target key.
    pub uid: String,
    /// Display name (library slug; a deleted project falls back to uid).
    pub name: String,
}
