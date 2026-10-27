//! Frame plans: per-frame graph work derived once per edit and replayed.
//!
//! A node (or the engine) that re-derives the same structure every frame —
//! parsed paths, binding topology, spans, fragments, uniform slots — keeps it
//! in a [`PlanSlot`] keyed on whatever can change it (the resolver's
//! structure epoch plus a few node-local revisions). The first frame after
//! the key moves builds the plan; every later frame replays it.
//!
//! The contract extends the resolver's (`docs/adr/2026-07-31-resolver-persistent-resolution.md`):
//! **any engine mutation that can change what a plan holds must move a key
//! the plan reads.** It fails silently the same way, so [`PlanMode::Check`]
//! rebuilds the plan on replay and compares (every frame in this crate's
//! tests, every 32nd frame in other debug builds, never in release), and
//! [`FramePlanCounters`] makes "a steady frame builds no plan" a unit-test
//! question. See `docs/adr/2026-10-24-frame-plan-once-per-edit.md` and the
//! crate README's "The frame plan".

mod frame_plan_counters;
mod lane_topology;
mod plan_host;
mod plan_mode;
mod plan_slot;
mod planned_read;

pub use frame_plan_counters::FramePlanCounters;
pub(crate) use lane_topology::LaneTopology;
pub use plan_host::{FramePlanState, PlanHost};
pub use plan_mode::PlanMode;
pub use plan_slot::PlanSlot;
pub use planned_read::PlannedRead;
