//! Crash-recovery bookkeeping for LightPlayer firmware.
//!
//! This crate is the truthful ledger of a two-layer recovery system:
//!
//! - **Layer 1 (in-process)**: panics are unwound and caught per-node
//!   (`catch_unwind` in the engine). Most failures recover without a reboot.
//! - **Layer 2 (reboot backstop)**: hangs, double panics, and crashes in the
//!   panic path itself are handled by rebooting and reading a small
//!   **persistent breadcrumb region** that survives software and watchdog
//!   resets (RTC fast RAM on ESP32; plain buffers on host/emu).
//!
//! Both layers share one model: a stack of [`FrameKind`] **recovery frames**
//! ("compiling shader fire.glsl", "rendering node /x") maintained **eagerly**
//! in the persistent region — a watchdog reset gives no crash-time hook, so
//! the stack on entry *is* the blame record.
//!
//! This crate does bookkeeping and pure decision queries only. Enforcement
//! (skipping project load, surfacing node errors, actually resetting the
//! SoC) lives in the callers.
//!
//! # Core constraints
//!
//! - `no_std`, **zero-alloc**: several entry points run in panic/OOM context.
//! - All region mutations follow a torn-write discipline: payload bytes are
//!   written first, then a single word (stack depth, crash-record state)
//!   makes them visible. A reset mid-write never yields a half-valid record.
//! - Guards must not be held across `.await` in code that shares the stack
//!   with other tasks (see [`FrameGuard`]).

#![no_std]

mod backend;
mod crash_record;
mod frame_guard;
mod frame_kind;
mod frame_path;
mod frame_record;
mod in_memory_backend;
mod ledger;
mod path_entry;
mod recovery;
mod recovery_level;
mod recovery_region;
mod recovery_stack;
mod reset_cause;
mod snapshot;
pub mod tuning;

pub use backend::RecoveryBackend;
pub use crash_record::{
    CRASH_FRAME_NAME_CAP, CRASH_MSG_CAP, CRASH_PC_CAP, CompactFrameName, CrashCause, CrashMsg,
    CrashRecord, OomStats,
};
pub use frame_guard::FrameGuard;
pub use frame_kind::FrameKind;
pub use frame_path::{FramePath, MAX_FRAME_DEPTH};
pub use frame_record::{FRAME_NAME_CAP, FrameRecord};
pub use in_memory_backend::InMemoryBackend;
pub use ledger::{GatedInfo, Ledger};
pub use path_entry::{ENTRY_NAME_CAP, PathEntry};
pub use recovery::{
    BootAssessment, EnterDenied, EnteredFrame, Recovery, RecoveryHandle, clear_tentative_crash,
    enter, finalize_crash_and_reset, is_initialized, mark_boot_complete, record_recovered_crash,
    set_global, snapshot, stage_crash,
};
pub use recovery_level::RecoveryLevel;
pub use recovery_region::{REGION_MAGIC, REGION_MAX_SIZE, REGION_VERSION, RecoveryRegion};
pub use reset_cause::ResetCause;
pub use snapshot::{CrashSnapshot, FrameNameRef, PathNames, RecoverySnapshot};
