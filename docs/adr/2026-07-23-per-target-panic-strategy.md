# ADR: Per-target panic strategy — unwinding is a device feature, not a default

- **Status:** Accepted
- **Date:** 2026-07-23 (formalizes decisions originally made ~2026-07-03
  with the crash-recovery work; backfilled with fresh context during the
  sim-fuel work)
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

LightPlayer runs the same engine core on targets with fundamentally
different panic capabilities:

- **Device (fw-esp32, fw-emu)** — bare-metal RV32 with no OS and no std
  unwinder. Unwinding does not exist by default; the firmware
  deliberately *builds* it: the `unwinding` crate (DWARF-based bare-metal
  unwinder; `fw-esp32/Cargo.toml`, `fw-emu/Cargo.toml`) plus the
  workspace `panic = "unwind"` profile ("ESP32: panic=unwind for OOM
  recovery via unwinding crate", root `Cargo.toml`), activated through
  the opt-in `panic-recovery` feature on `lpc-engine`. With it,
  `catch_node_panic(_framed)` is a real `catch_unwind` and caught panics
  feed the lp-recovery blame ledger (yellow → red-gate; see
  [2026-07-04-crash-recovery-model.md](2026-07-04-crash-recovery-model.md)).
- **Browser (fw-browser, wasm32)** — unwinding is *impossible*: rustc
  lowers every panic to an abort (a wasm `unreachable`, surfacing as a
  JS `RuntimeError`) regardless of the `panic = "unwind"` profile
  setting, and the `unwinding` crate is native-only. A panic leaves the
  wasm module's Rust state unrecoverable mid-operation.
- **Desktop host (fw-host / lpa-server)** — std unwinding exists, but
  the recovery ledger is deliberately not installed (multi-instance
  interleaving; crash-recovery ADR), so a caught panic would have
  nothing to report to.

The crash-recovery ADR recorded the *ledger* carve-out for host runtimes
but not the unwinding asymmetry itself. That gap became load-bearing
during the fuel work: the device design converts out-of-fuel traps into
panics precisely so the caught panic records blame — and porting fuel to
the wasm backends would have made that panic reachable on a target where
it aborts the module.

## Decision

**Panic-as-control-flow is permitted only under the `panic-recovery`
feature.** Everywhere else, failures must be returned as typed errors.

| Target | Panic lowering | Catcher | Blame ledger |
|---|---|---|---|
| fw-esp32 / fw-emu (`panic-recovery`) | unwind (`unwinding` crate) | real `catch_unwind` per node | yes — caught panics recorded, yellow → red-gate |
| fw-browser (wasm32) | **abort** (`unreachable` → JS RuntimeError) | none (passthrough) | none |
| fw-host / desktop | unwind (std) | passthrough (feature off) | none installed |

Concretely:

- `lpc-engine`'s `catch_node_panic(_framed)` keeps its two variants:
  real catch under `panic-recovery`, passthrough otherwise.
- Code that *wants* the blame ledger (e.g. the shader out-of-fuel
  handler) must branch on the feature: panic under `panic-recovery`,
  plain typed `Err` otherwise, with the same message either way
  (`fuel_exhausted_failure` in `shader_node.rs` is the reference
  pattern).
- New panic-as-control-flow sites require the same gating; an ungated
  `panic!` on an error path is a bug on wasm even when it "works" on the
  targets it was tested on.

## Consequences

- Device keeps its layered recovery (unwind → catch → ledger →
  red-gate) — unchanged.
- Host/browser failures surface as node errors through the ordinary
  typed-error path; they are legible but unrecorded (no latch). If host
  blame ever matters, the crash-recovery ADR's "per-instance recovery
  contexts" follow-up is the path, not panics.
- The asymmetry is now written down: the device/sim behavioral
  difference for the same failing shader (sticky red-gate vs
  error-every-frame) is a documented consequence, not an accident.

## Alternatives Considered

- **Enable `panic-recovery` everywhere** — impossible on wasm32 (no
  unwinding to build on) and pointless on hosts without the ledger.
- **Wasm exception-handling proposal for browser unwinding** — immature
  toolchain support, large surface for one call site; not warranted.
- **Ledger on hosts via per-instance contexts** — real future option
  (crash-recovery ADR follow-up), orthogonal to the panic question:
  typed errors could feed it without panics.

## Follow-ups

- If a second panic-gated site appears, consider a small helper macro so
  the feature-branching pattern stays uniform.
- Per-instance host recovery contexts (inherited follow-up from the
  crash-recovery ADR).
