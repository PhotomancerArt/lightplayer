//! The roster card-state vocabulary (device-UX direction, "Card grammar").
//!
//! One honest, transport-independent state per roster card — device or
//! (later) sim runtime — replacing the laundered booleans the gallery used
//! to render. The vocabulary is a first-class concept: the same states may
//! later drive on-device LEDs or richer displays, so [`RosterCardState`]
//! stays free of web/UI types. See
//! `docs/adr/2026-07-16-device-card-state-vocabulary.md`.
//!
//! Concept map:
//! - [`roster_card_state`]: the 14-state enum + its status-line copy.
//! - [`roster_circle`]: the status-circle spec (shape × status family).
//! - [`roster_affordance`]: the one affordance each state carries (identity
//!   only in M2 — wiring lands with the flows that make each state real).
//! - [`roster_evidence`]: evidence inputs + the pure derivation function
//!   (the normative state mapping lives on its module doc).
//! - [`firmware_update`]: the standing "firmware update available" chip
//!   comparison.

pub mod firmware_update;
pub mod roster_affordance;
pub mod roster_card_state;
pub mod roster_circle;
pub mod roster_evidence;

pub use firmware_update::firmware_update_available;
pub use roster_affordance::RosterAffordance;
pub use roster_card_state::{ConnectPhase, DegradedReason, RosterCardState};
pub use roster_circle::{RosterCircle, RosterCircleShape};
pub use roster_evidence::{ConnectEvidence, RosterEvidence, derive_roster_card_state};
