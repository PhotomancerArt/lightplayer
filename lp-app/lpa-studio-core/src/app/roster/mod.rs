//! The roster card-state vocabulary (device-UX direction, "Card grammar").
//!
//! One honest, transport-independent state per roster card — device or
//! live sim runtime — replacing the laundered booleans the gallery used
//! to render. The vocabulary is a first-class concept: the same states may
//! later drive on-device LEDs or richer displays, so [`RosterCardState`]
//! stays free of web/UI types. See
//! `docs/adr/2026-07-16-device-card-state-vocabulary.md`.
//!
//! Concept map:
//! - [`roster_card_state`]: the state enum + its status-line copy (grows
//!   as new states earn a place in the vocabulary — count it, don't quote
//!   it here).
//! - [`roster_state_spec`]: the status-circle spec (shape × status family).
//! - [`roster_affordance`]: the one affordance each state carries (identity
//!   only in M2 — wiring lands with the flows that make each state real).
//! - [`roster_evidence`]: evidence inputs + the pure derivation function
//!   (the normative state mapping lives on its module doc).
//! - [`firmware_update`]: the standing "firmware update available" chip
//!   comparison (+ the bundled-image evidence struct).
//! - [`device_rich_object`]: the device as a rich object — the fixed
//!   section schema behind the card's detail trigger, built on
//!   [`crate::app::rich_object`].
//! - [`sim_rich_object`]: the live simulator session as a rich object
//!   (D36) — the same schema, only the honestly-applicable sections.
//! - [`card_tabs`]: sections → the card's icon tabs (M7′ — the card as
//!   control panel; the popover renderers retire, the model survives).

pub mod card_tabs;
pub mod device_rich_object;
pub mod firmware_update;
pub mod roster_affordance;
pub mod roster_card_state;
pub mod roster_evidence;
pub mod roster_state_spec;
pub mod sim_rich_object;

pub use card_tabs::{CardTabView, DeviceCardTab, device_card_tabs};
pub use device_rich_object::{DeviceDetailAffordance, DeviceRichInput, device_rich_object};
pub use firmware_update::{BundledFirmware, firmware_update_available};
pub use roster_affordance::RosterAffordance;
pub use roster_card_state::{ConnectPhase, DegradedReason, DeviceFormatStanding, RosterCardState};
pub use roster_evidence::{ConnectEvidence, RosterEvidence, derive_roster_card_state};
pub use roster_state_spec::{RosterStateSpec, RosterTreatment};
pub use sim_rich_object::{SimDetailAffordance, SimRichInput, sim_rich_object};
