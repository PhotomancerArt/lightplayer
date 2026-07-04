# ADR: Crash Recovery Model and Persistent Breadcrumb Contract

## Status

Accepted

## Context

LightPlayer runs a GLSL JIT compiler and shader runtime on a bare-metal
ESP32-C6 with a 300 KB heap and no OS. Many things can go wrong at runtime:
an OOM during shader compilation, a hung shader (an infinite loop in JIT'd
native code that cannot be preempted), a driver fault from a bad hardware
config, or an ordinary panic. Before this change the firmware had in-process
panic recovery (an `unwinding`-based panic handler plus per-node
`catch_unwind`) but nothing for failures that escape it: hangs had no
watchdog, panics that failed to unwind hit `loop {}`, and there was no record
across a reboot of what the device was doing when it died. The worst outcome —
a device stuck in a silent crash/boot loop with no signal to the user — was
reachable.

We needed a layered recovery system that detects where a failure occurred,
degrades only the failing part, survives reboots caused by hangs or hard
faults, and surfaces the failure to the Studio client. The shader fuel system
(deadline checks at JIT loop back-edges) is a separate future effort; this ADR
covers the reboot-based backstop and the blame model.

## Decision

### Two layers, one model

Recovery is two layers over a single frame/path model:

- **Layer 1 (in-process, pre-existing):** panics unwind and are caught
  per-node. Most panics and OOMs recover without a reboot.
- **Layer 2 (new):** for what `catch_unwind` cannot handle — hangs, double
  panics, faults in the panic path — the device reboots, and the next boot
  reads a persistent breadcrumb region to attribute and report the failure.

Both layers share a stack of **recovery frames** (`Boot`, `ProjectLoad`,
`ShaderCompile`, `NodeRender`), each named by stable identity (a node's tree
path, a project name). The frame stack is the unit of blame.

### `lp-recovery` crate: bookkeeping, not policy

A new `no_std`, zero-alloc crate `lp-base/lp-recovery` owns the truthful
ledger — what ran, what failed, how often — and exposes pure decision
*queries* (is this path gated? should this boot enter safe mode?).
Enforcement (skipping project load, surfacing node errors, resetting the SoC)
lives in the callers (fw-esp32 boot, the engine's node dispatch). The
crash-path code stays minimal and stable; behavior is tuned in the callers and
in a `tuning` module of named constants, not by touching the ledger.

The crate is reached through a global instance (the `log`-crate pattern),
because the panic handler and deeply nested engine code both need it without
threading a context through every signature. Access is serialized by
`critical-section`; reentrant access (a panic inside a recovery operation)
degrades to a no-op rather than deadlocking.

### Persistent region contract

A fixed `#[repr(C)]` region (≤ 1 KB) lives in RTC fast RAM on ESP32
(`#[ram(unstable(rtc_fast, persistent))]`), which survives software and
watchdog resets but not power loss. It holds a magic + version + CRCs, boot
counters, the frame stack, a crash record, and the blame ledger.

- **Eager maintenance:** every frame enter/leave writes the region
  immediately. A watchdog reset gives no crash-time hook, so the stack *as it
  stands* must already be the blame record.
- **Torn-write discipline:** payload bytes are written first, then a single
  visibility word (the stack `depth`, or the crash record `state`) is flipped,
  with a compiler fence between. A reset mid-write never yields a half-valid
  record; it loses the in-flight push at worst.
- **CRC coverage:** the CRCs cover only the slow-changing header and boot
  bookkeeping. The hot frame stack and crash record are deliberately excluded
  (they change per-frame and a watchdog may interrupt them by design) and rely
  on the visibility-word discipline instead.
- **Validation:** the region is trusted only if magic + version + CRC hold
  AND the reset reason is not power-on. Power-on renders it invalid by
  definition — RTC RAM is undefined after power loss, and a lucky CRC match
  must not resurrect stale blame.

### Reset-reason interpretation

The ESP32 `SocResetReason` maps to a platform-agnostic `ResetCause`:
power-on (region invalid), software reset (our panic path wrote details),
watchdog reset (the eager stack is the record), brownout (a power problem —
never blame the code path), and USB-UART/JTAG (espflash/dev-tool reset —
user-initiated, never a crash). Unmapped causes are `Unknown` and do not
blame code, so an exotic reset cannot manufacture false blame; explicit crash
records are still honored regardless.

### Blame, levels, and escalation

Crashes (both reboot-causing and caught-in-process) are recorded against the
crashing path and every parent prefix:

- First crash on a path → **yellow** (watched, reported, nothing disabled).
- Second crash on the same path while yellow → **red**: entering it (or
  anything under it) is denied with a legible error.
- **Hierarchical escalation:** a parent that saw crashes under two distinct
  children goes red itself (a→b→c and a→b→f gate b), so a subtree that keeps
  failing in different leaves is disabled at the common ancestor.
- Red demotes to yellow at the next boot — one retry per boot, so nothing
  bricks permanently while crash loops are still stopped.
- Enough clean completions of a yellow path clear it back to green.

Two consecutive boots that die before a boot-complete milestone put the next
boot in **safe mode**: transport and server come up so Studio can reach the
device, but project auto-load is skipped. All thresholds are tuning knobs.

### Surfacing

Recovery state rides the existing unsolicited `Heartbeat` as an optional
`RecoveryStatus` (level, reset reason, safe mode, last crash, watched paths).
Gated-entry errors flow through the existing `NodeError` / `ServerError`
paths. Snapshot→wire conversion lives in lpa-server so the recovery crate
stays serde-free.

## Consequences

- The firmware always reboots out of a hang or hard fault (no more `loop {}`)
  and the next boot can tell the user what died and where.
- Repeatedly-crashing nodes/projects/shaders are automatically disabled with a
  legible reason, in-process, without a reboot in the common case.
- Cost: ~14 KB flash and a ≤ 1 KB RTC-RAM region. The concurrent toml→json
  work is expected to recover comparable flash.
- **Accepted limitations:**
  - Power cycle clears all recovery state (RTC RAM is not power-persistent).
    A user pulling power is a reasonable "let me retry" signal; persisting the
    bad-path list to flash after a successful boot is deferred.
  - A single RWDT (~8 s, tightened from a generous boot budget after the first
    server-loop feed); no per-task watchdogs. Liveness of the I/O task is
    proven to the feeder via an aggregator flag.
  - Blame is innermost-frame attribution and can misattribute — notably a
    fragmentation OOM blames whichever innocent allocation hit the wall. Heap
    stats are recorded in the crash record to aid diagnosis, and the
    yellow-before-red tolerance absorbs one-off misfires.
  - Host runtimes (fw-host, fw-browser) do not install a recovery global:
    they can run several engine instances per process, and a process-wide
    frame stack would interleave. Instrumentation is inert there.

## Alternatives Considered

- **Flash-persisted red state.** Rejected for v1: writing flash from a crash
  or pre-reset path is risky and slow. Lazy persistence after a clean boot is
  noted as future work.
- **Merging recovery phase-tracking into `lp-perf`.** Rejected: lp-perf is a
  stateless, fire-and-forget event emitter with `&'static str` names; recovery
  needs a stateful, eagerly-persisted stack with dynamic names, gating, and
  crash-context safety. An optional future bridge can emit lp-perf events from
  frame enter/leave.
- **Shader fuel instead of a watchdog.** Complementary, not a substitute — a
  fuel system is the right first line against hung shaders but cannot catch a
  codegen bug that omits the check, a driver fault, or a hang outside shader
  code. The hardware watchdog remains the backstop. Separate plan.
- **Wall-clock reset-to-green.** Rejected in favor of gating the
  return-to-green transition on the suspect path being successfully
  re-exercised, so time cannot flip a path green just before it crashes again.

## Follow-ups

- Shader fuel / back-edge deadline checks (separate plan).
- Deliberate clean reboot on yellow as a fragmentation cure.
- Lazy flash persistence of the bad-path list after a successful boot.
- Per-instance recovery contexts for host/browser multi-runtime targets.
- lp-perf bridge; richer Studio recovery UI; CI placement of the slow
  emulator recovery suite.
