# lp-recovery

Crash-recovery bookkeeping for LightPlayer firmware: a small persistent
"breadcrumb" region (RTC fast RAM on ESP32) holding an eagerly-maintained
stack of recovery frames, crash records written zero-alloc from panic/OOM
context, and boot-time analysis that turns reset reasons + leftover state
into "here is what crashed last run".

Design rationale and the persistent-region contract are recorded in
`docs/adr/2026-07-04-crash-recovery-model.md`.

This used to be described as the second of two layers, the first being
in-process `catch_unwind` per node (`catch_node_panic`, wrapping a node call
in the `unwinding` crate and turning a caught panic into a `NodeError`
without a reboot). **That layer is gone** — no RV32 target unwinds any more,
per `docs/adr/2026-08-02-rv32-firmwares-are-abort-tier.md` — and nothing in
*this* crate changed when it went, because none of its bookkeeping was ever
gated on unwinding (`src/lib.rs:8-13`). The practical difference: a crash now
always costs a reboot to record. What remains at the call site
(`lp-core/lpc-engine/src/node/catch_node_panic.rs:1-20`) is the half that was
never conditional on unwinding — `catch_node_panic_framed` still pushes a
recovery frame around every node call, so the blame ledger knows what was
running when the device died, and still denies entry to a red-gated path.
Frame stack, blame, yellow → red escalation, hierarchical parent gating, and
safe mode all work exactly as
`docs/adr/2026-07-04-crash-recovery-model.md` describes; only the "caught
in-process, no reboot" outcome is gone.

The reboot backstop (this crate + platform glue) is therefore the whole
recovery story now: hangs caught by the hardware watchdog, double panics,
and panic-path failures reboot, and the next boot reads the region to blame
and report the failure.

Key rules:

- `no_std`, zero-alloc core — several entry points run in panic context.
- Torn-write discipline: payload first, then one visibility word; a reset
  mid-write never produces a half-valid record.
- Frame guards must not be held across `.await` in code sharing the stack
  with other tasks (see `FrameGuard` docs).
- Power-on invalidates the region by definition (RTC RAM does not survive
  power loss) — see `RecoveryRegion::is_valid` (`src/recovery_region.rs`),
  whose only exclusion is `ResetCause::PowerOn`. Note the drift this leaves
  in `ResetCause::UserReset`'s doc comment (`src/reset_cause.rs:9-11`),
  which claims a user/tool reset "clears blame like a power-on would, by
  policy": in the actual code `UserReset` only keeps `blames_code()` false
  for *that boot's own* crash record (so a user-initiated reset is never
  itself blamed on the code path) — it does **not** invalidate the region or
  touch the ledger. An existing red/yellow entry survives a `UserReset`
  exactly as it survives a `SoftwareReset` or `WatchdogReset`: demoted one
  step by `Ledger::on_boot` (below), not cleared. Only `PowerOn` clears
  anything, and only because the region is unreadable garbage after it.

## Blame ledger

Every reboot-causing crash is recorded (by the next boot, reading the region
left behind) against the crashing path and every parent prefix:

- First crash on a path → **yellow**: watched and reported, nothing
  disabled. Enough clean completions (`tuning::CLEAN_COMPLETIONS_TO_GREEN`)
  clear it back to green.
- Second crash while yellow → **red**: `enter` on that path (or anything
  under it) is denied with a legible reason. This is what "a fault is never
  black" (`docs/adr/2026-09-02-fault-is-never-black.md`) exists downstream
  of: a red-gated node stops running, and something else must say so.
- **Every boot** (any cause but `PowerOn`) demotes red entries to yellow —
  `Ledger::on_boot` (`src/ledger.rs`), called unconditionally whenever
  `RecoveryRegion::is_valid` passes — giving the path one retry per boot, so
  nothing bricks permanently. If the retry crashes again it goes straight
  back to red on the *next* boot's ledger update.
- **Hierarchical escalation**: a parent that saw crashes under two distinct
  children goes red itself (a→b→c and a→b→f crashing gates b).
- Two consecutive boots dying before the boot-complete milestone put the
  next boot in **safe mode** (`BootAssessment::safe_mode`) — callers skip
  project auto-load but keep the device reachable.

The ledger is bookkeeping + queries; enforcement lives in callers. All
thresholds are tuning knobs in `tuning`, not architecture.

Backends: `InMemoryBackend` (host/tests), ESP32 RTC-RAM and emulator
backends live in the respective firmware crates.

## Clearing the ledger

`RecoveryHandle::clear_ledger` (`src/recovery.rs`, backed by
`Ledger::clear` in `src/ledger.rs`) is the user-facing "Clear faults" verb's
device-side half (`docs/adr/2026-09-02-fault-is-never-black.md`). It is the
only way to lift a quarantine short of losing power:

- **Clears**: every path entry (red or yellow, back to empty/green) and the
  consecutive-incomplete-boot (safe-mode) counter. A path admitted again
  starts with no history at all — the next crash on it, if there is one,
  starts back at yellow.
- **Keeps**: the crash record (`last_crash`), `boot_count`, and
  `generation`. These live outside the ledger by design — the next
  heartbeat still reports what the last crash was, with `boots_ago` intact.
  Clearing forgives blame; it does not erase history.
- **Not the same as a reset.** A software/watchdog/user reset only demotes
  red entries to yellow for one retry (the `on_boot` rule above) — it never
  wipes an entry, and a repeat-offending path re-reds within two more
  crashes regardless of how many times the board is reset. Only power-on
  (which invalidates the whole region, see `is_valid` above) and this verb
  clear a path outright. If the underlying condition is still present —
  the same OOM, the same panic — the freshly-cleared path simply re-accuses:
  first crash back to yellow, second crash back to red. That recurrence is
  the intended, honest outcome, not a bug in the verb.
