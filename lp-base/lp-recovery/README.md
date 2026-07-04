# lp-recovery

Crash-recovery bookkeeping for LightPlayer firmware: a small persistent
"breadcrumb" region (RTC fast RAM on ESP32) holding an eagerly-maintained
stack of recovery frames, crash records written zero-alloc from panic/OOM
context, and boot-time analysis that turns reset reasons + leftover state
into "here is what crashed last run".

Design rationale and the persistent-region contract are recorded in
`docs/adr/2026-07-04-crash-recovery-model.md`.

Part of a two-layer recovery design:

1. **In-process** (exists in the engine): panics unwind and are caught
   per-node; most failures never reboot.
2. **Reboot backstop** (this crate + platform glue): hangs caught by the
   hardware watchdog, double panics, and panic-path failures reboot, and the
   next boot reads the region to blame and report the failure.

Key rules:

- `no_std`, zero-alloc core — several entry points run in panic context.
- Torn-write discipline: payload first, then one visibility word; a reset
  mid-write never produces a half-valid record.
- Frame guards must not be held across `.await` in code sharing the stack
  with other tasks (see `FrameGuard` docs).
- Power-on invalidates the region by definition (RTC RAM does not survive
  power loss); `UserReset`/`Brownout` never blame the code path.

## Blame ledger

Crashes (reboot-causing AND caught-in-process) are recorded against the
crashing path and every parent prefix:

- First crash on a path → **yellow**: watched and reported, nothing
  disabled. Enough clean completions (`tuning::CLEAN_COMPLETIONS_TO_GREEN`)
  clear it back to green.
- Second crash while yellow → **red**: `enter` on that path (or anything
  under it) is denied with a legible reason. Red demotes to yellow at the
  next boot — one retry per boot, so nothing bricks permanently.
- **Hierarchical escalation**: a parent that saw crashes under two distinct
  children goes red itself (a→b→c and a→b→f crashing gates b).
- Two consecutive boots dying before the boot-complete milestone put the
  next boot in **safe mode** (`BootAssessment::safe_mode`) — callers skip
  project auto-load but keep the device reachable.

The ledger is bookkeeping + queries; enforcement lives in callers. All
thresholds are tuning knobs in `tuning`, not architecture.

Backends: `InMemoryBackend` (host/tests), ESP32 RTC-RAM and emulator
backends live in the respective firmware crates.
