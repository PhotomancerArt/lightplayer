//! The device setup flow: connect → flash → provision → device home.
//!
//! **Design: `docs/design/device-setup-flow.md`.** That document's §2
//! transition table and this module's reducer arms are ONE artifact — a
//! change to either lands in the same commit as the change to the other,
//! and [`reducer::tests`] is the enforcement. §7 of the doc records every
//! place the implementation and the ratified flow-spec differ, so the two
//! can be reconciled rather than drift.
//!
//! The shape (design §6):
//!
//! - [`SetupFlow`] is a **pure reducer**: `(State, Event) → (State,
//!   Vec<Command>)`. No I/O, no async, no UI types, no clock.
//! - [`SetupTarget`] is the **capability boundary**: the machine asks
//!   `needs_connect` / `needs_flash` / `can_rename`, never "is this the
//!   simulator".
//! - [`BoardVerdict`] is the **board-state enum** one probe pass produces
//!   alongside `detected_chip`, biased so that nothing but a wire hello can
//!   be called LightPlayer.
//! - [`derive_device_name`] is the **naming derivation** of design §3.
//! - [`dispatch_for`] is the **thin executor**: `SetupCommand` → the op
//!   values the studio already runs. It decides nothing.

pub mod command;
pub mod event;
pub mod executor;
pub mod naming;
pub mod reducer;
pub mod state;
pub mod target;
pub mod verdict;

pub use command::SetupCommand;
pub use event::{SetupEvent, SetupEventKind};
pub use executor::{SetupDispatch, SetupExecutorContext, dispatch_for};
pub use naming::{derive_device_name, month_day_label, unique_device_name};
pub use reducer::{SetupContext, SetupFlow, SetupStep, reduce};
pub use state::{
    BoardPickState, CloseReason, ConnectHint, ProvisionPhase, ProvisionState, SetupState,
    SetupStateKind,
};
pub use target::{HardwareSetupTarget, SetupCapabilities, SetupTarget, SimulatorSetupTarget};
pub use verdict::{BoardProbe, BoardVerdict, ProbeEvidence, classify_board, known_device_for};
