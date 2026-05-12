# Phase 4: A/B report + fw-emu profiling

## Scope of phase

Create **`docs/reports/<YYYY-MM-DD>-lpvm-cranelift-vi-c-ab.md`** (use the date you
run measurements). Capture **methodology** and **results** with emphasis on
**memory** comparison on **fw-emu**; compile time and execution deltas also on
**fw-emu** (relative old-vs-new treated as indicative for ESP32).

Reserve a **Manual ESP32 checklist** section for you to fill after flashing.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Report skeleton (markdown)

Suggested sections:

- **Purpose** — VI-C, link to roadmap + this plan.
- **Environments** — Host OS, Rust toolchain, git SHAs or branch names for “old”
  vs “new” compiler worktrees (old tree only available until old stack is
  removed).
- **fw-emu gate** — Commands from Phase 1 + pass/fail date.
- **Memory (primary)** — How measured (e.g. `lp-cli` mem profile building
  `fw-emu` with `alloc-trace`, or `fw-tests` alloc-trace test + log analysis).
  Table: metric → old → new → notes.
- **Compile time** — Same shader/scene scenario if applicable; wall time or
  logged compile duration from engine.
- **Execution / frame time** — fw-emu or host-driven scenario; methodology.
- **Firmware binary size** — `fw-esp32` release-esp32 artifact size (new path);
  compare to old worktree if you still have it.
- **Manual ESP32 (placeholder)** — Checklist: flash, shader compile, render,
  OOM smoke, serial/timing quirks; fill when you run hardware.
- **Known issues / follow-ups** — Regressions, deferred optimizations.

### 2. Profiling hooks (reference)

- `lp-cli` / `mem_profile`: see `lp-cli/src/commands/mem_profile/handler.rs`
  (builds `fw-emu` with `alloc-trace`).
- `fw-tests/tests/alloc_trace_emu.rs` — integration pattern for alloc tracing.
- `fw-emu` feature `alloc-trace` in `lp-fw/fw-emu/Cargo.toml`.

Document the **exact commands** you ran in the report so the experiment is
repeatable.

### 3. No code changes required

If the report is the only deliverable this phase, the phase is still “done” when
the file exists with methodology + fw-emu numbers (or explicit “blocked on X”).

## Validate

- Report file exists under `docs/reports/` with dated filename.
- At least one **fw-emu**-based measurement recorded (memory and/or compile), or
  a clear blocker documented.

Hardware section can stay **TBD** until you complete manual validation.
