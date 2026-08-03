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
    /// The live session's pool identity (`RuntimeId` rendering), stable for
    /// the session's life. `None` on registry-derived (offline) cards,
    /// which always have a uid instead. The anonymous-card key fallback —
    /// see [`Self::identity_key`].
    pub session_key: Option<String>,
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
    /// The port as the app can name it (endpoint label + grant short id,
    /// e.g. "ESP32 Serial (0x303a:0x1001) · port-2") — the Technical tab's
    /// identification line. `None` on registry (offline) cards and stubs.
    /// Web Serial never exposes the OS path; this is the whole truth.
    pub port_label: Option<String>,
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
    /// ORDER IS LOAD-BEARING: `uid` stays FIRST. `CardUiState` is keyed by
    /// this and must survive session replaces — a stamped board keying by
    /// its (per-session) `RuntimeId` would drop its tab/sheet state on
    /// every replace. Only the anonymous case uses `session_key`: an
    /// identity-less LIVE card (a board mid-provision, before its uid is
    /// stamped) keys by the session's `RuntimeId` so two anonymous boards
    /// never collide — the name fallback used to erase the second board
    /// via `dedupe_by_key` (both were "Connected device"; the multi-board
    /// defect, 2026-08-02). The name remains only for cards with neither
    /// (registry cards, which always have a uid, never reach it).
    pub fn identity_key(&self) -> &str {
        if self.sim {
            return "runtime-sim";
        }
        self.uid
            .as_deref()
            .or(self.session_key.as_deref())
            .unwrap_or(&self.name)
    }

    /// Back-compat alias for keyed rendering — the same canonical key.
    pub fn render_key(&self) -> &str {
        self.identity_key()
    }

    /// Whether the CARD-OWNED op flow running on `session_key` rides THIS
    /// card (state-flow model §2). An operation belongs to the session it
    /// runs on, and a card belongs to at most one session — so this is an
    /// exact match (M4).
    ///
    /// WHY `session_key` AND NOT `identity_key()`: a blank board's card
    /// key IS its session key, but the moment a flash stamps an identity
    /// the same card's key becomes its `uid` — `identity_key` puts uid
    /// first. An op keyed by the card key would lose its card at the exact
    /// instant the flash succeeded. `session_key` is set on every live
    /// card and does not move when the uid lands.
    ///
    /// It still rides an OFFLINE card: `op_in_flight` deliberately pins a
    /// card whose session went `Gone` mid-op, and the pinned card is
    /// exactly where the "unplug the board and plug it back in"
    /// instruction has to appear (bench, 2026-07-31 — half a fix showed a
    /// bare "Not seen yet" with no instruction). Registry cards of other
    /// devices carry no `session_key`, so they cannot adopt a stray op.
    ///
    /// The ONE rule, shared by the two places that must agree: the
    /// controller's view build (`overlay_card_ui`) and the actor's
    /// progressive patch ([`crate::UiStudioView::apply_card_op`]). They
    /// drifted apart once already — a flash's progress reached neither
    /// surface — so both call here.
    pub fn takes_card_op(&self, session_key: &str) -> bool {
        !self.sim && self.session_key.as_deref() == Some(session_key)
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
