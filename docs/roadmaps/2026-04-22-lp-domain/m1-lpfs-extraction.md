# Milestone 1: lpfs extraction

## Goal

Extract `LpFs` and friends from `lp-core/lp-shared/src/fs/` into a new
top-level crate `lp-base/lpfs/` so `lp-domain` (and any future
foundational crate) can depend on the filesystem abstraction without
dragging in legacy `lp-core`.

## Suggested plan name

`lp-domain-m1`

## Scope

**In scope:**

- Create `lp-base/lpfs/` crate with `LpFs` trait, `LpFsMemory`,
  `LpFsStd` (behind a `std` feature), `LpFsView`, `FsChange`,
  `FsVersion`.
- `no_std + alloc` by default; `std` feature for `LpFsStd`.
- Update all call sites across the workspace to import from `lpfs`
  instead of `lp_core::lp_shared::fs::*` (or however it's currently
  spelled). Known call sites include: `lp-core/lp-engine`,
  `lp-core/lp-server`, `lp-cli/src/commands/dev/`, `lp-fw/`, plus
  whatever else `grep -r "fs::lp_fs" crates/` turns up.
- Delete `lp-core/lp-shared/src/fs/` after the move.
- Workspace `cargo build --workspace` and `cargo test --workspace`
  pass on host. ESP32-C6 firmware build also passes.

**Out of scope:**

- Any new functionality in lpfs (just a move).
- Hot-reload behavior (`FsChange` listening) beyond what already
  exists.
- Anything that depends on lpfs's new location (lp-domain itself
  starts in M2).

## Key decisions

(Mostly carried from roadmap notes; nothing new here.)

- Crate location: `lp-base/lpfs/` (matches the `lp-base/lp-perf/`
  pattern).
- No transitional re-export from `lp-shared`; mechanical update of all
  call sites in one PR.
- `LpFsStd` is behind a `std` feature so `no_std` consumers stay
  clean.

## Deliverables

- `lp-base/lpfs/Cargo.toml`
- `lp-base/lpfs/src/{lib,lp_fs,lp_fs_mem,lp_fs_std,lp_fs_view,fs_event}.rs`
- All updated call sites compiling and tests passing.
- `lp-core/lp-shared/src/fs/` deleted from the tree.
- Workspace `Cargo.toml` updated to include the new crate.

## Dependencies

None. This is the first milestone and is a prerequisite for everything
that follows.

## Execution strategy

**Option A — Direct execution (no plan file).**

Justification: Mechanical move with no design questions. Scope is
fully pinned down by this milestone file (move `fs/` to a new crate,
update call sites, run the verification commands). Risk is thoroughness
(missing a call site), not architecture, and falls inside Composer 2's
ability to grep + verify with `cargo build --workspace`.

Estimated size: ~6 source files moved, ~10–20 call-site files touched,
0 new logic.

> I can implement this milestone without planning. Here is a summary
> of decisions/questions: lpfs lives at `lp-base/lpfs/`; `no_std + alloc`
> default with `std` feature for `LpFsStd`; mechanical move with no
> transitional re-export; all call sites updated in one PR; verification
> via workspace `cargo build && cargo test` plus the ESP32-C6 firmware
> build. If you agree, I will implement now using a Composer 2
> sub-agent. If you want to discuss any of these, let me know now.
