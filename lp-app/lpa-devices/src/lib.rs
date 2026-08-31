//! The device model: an event-fold roster of devices, UI-free and IO-free.
//!
//! Plugging in a device is the product's moment of truth, and the shipped
//! implementation was four state machines, five auxiliary stores and no
//! shared lifetime. This crate is the replacement model: five concepts, one
//! fold, one projection.
//!
//! ```text
//! Roster ──── owns ────► Links (dumb transports) + router
//!    │                   DeviceRecords (persisted identity + prefs)
//!    │                   Journal (flight recorder, all inputs)
//!    └── owns ────► Device (one per known device)
//!                      │  intent      (prescriptive user state)
//!                      │  evidence    (incremental fold of events)
//!                      │  link        (routed by the Roster)
//!                      └─ activity    (Option — supervised reducer)
//!                             projection: view DTO = f(intent, evidence, activity)
//! ```
//!
//! # The rules this crate is built on
//!
//! - **Sans-IO.** No tokio, no embassy, no `wasm-bindgen`, no futures
//!   executor. [`Roster::handle`] takes a caller-supplied timestamp and
//!   returns [`Command`]s; an effects layer outside this crate performs
//!   them. Waiting is [`Command::StartTimer`] out,
//!   [`Event::TimerFired`] in.
//! - **Fold discipline (I6).** New facts enter as events or they do not
//!   enter. [`Evidence`] is written only by the fold; actions write only
//!   [`Intent`] and activity existence.
//! - **Dependency inversion.** The transport contract ([`Link`],
//!   [`LinkEvent`], [`LinkCommand`]) is defined *here* and implemented by
//!   `lpa-link`. The model never calls a transport.
//! - **Total, escapable projection.** [`view::roster_view`] renders every
//!   reachable state, and every card carries at least one
//!   [`Escape`](view::Escape).
//!
//! # Where to start
//!
//! - [`Roster::handle`] — the one entry point.
//! - [`replay`] — the fixture harness; every scenario is a script.
//! - `README.md` and `docs/adr/2026-08-25-event-fold-device-model.md`.

pub mod activity;
pub mod device;
pub mod event;
pub mod evidence;
pub mod identity;
pub mod intent;
pub mod journal;
pub mod link;
pub mod record;
pub mod replay;
pub mod roster;
pub mod time;
pub mod view;
pub mod wire;

pub use activity::{ActivityCell, ActivityKind, ActivityOutcome, CancelPhase, PushActivity};
pub use device::{Device, DeviceStatus};
pub use event::{Action, ActivityMarker, Command, EffectId, EffectRequest, Event, Input};
pub use evidence::{Classification, Evidence, Freshness, IncompatibleReason, Liveness, Presence};
pub use identity::{DeviceId, DeviceUid, EndpointKey, IdentityChain, MacAddress, PeerIdentity};
pub use intent::{ConnectionIntent, Intent};
pub use journal::{Journal, JournalEntry, JournalNote, Scope};
pub use link::{Link, LinkCommand, LinkEvent, LinkId, LinkInfo, ResetKind};
pub use record::DeviceRecord;
pub use roster::{PendingLink, Roster, RosterConfig};
pub use time::{Millis, TimerId};
pub use view::{DeviceView, Escape, LoadedProject, RosterView};
pub use wire::{
    ClientFrame, ClientFrameBody, HelloFacts, LoadedProjectFacts, ServerFrame, ServerFrameBody,
};
