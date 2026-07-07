# ADR: Sans-IO core — the core is IO-free state machines; async belongs to platform edges

- **Status:** Accepted
- **Date:** 2026-07-06
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The core of LightPlayer (engine, server, model, fs abstraction) was
written before any UI existed, before real firmware existed, and before
async Rust was well understood on this project. It was made synchronous
on a maintainability instinct: sync code is simpler to reason about,
test, and debug. That instinct held up, but the *reasons* it held up only
became legible later, as the system grew edges where async is native:

- `fw-esp32` / `fw-core` do IO through embassy (async HAL).
- The browser studio drives IO with `wasm-bindgen-futures`; the simulator
  runs as WASM in a Web Worker whose persistent storage (OPFS) is
  Promise-based.
- `fw-browser` grew a null-waker spin `block_on` to drive async traits
  from sync dispatch — a smell that prompted this decision point.

In 2026-07 (while planning persistent browser storage), we explicitly
revisited: keep the sync core with async handling at the edges, or
refactor the core to async?

Two observations decided it. First, the codebase had already converged —
mostly deliberately, partly by instinct — on the industry pattern for
one-core-many-platforms systems (bare metal + wasm + native): the
**sans-IO core**. Time is injected (`ManualTimeProvider`, caller-supplied
timestamps), transports feed envelopes and drain frames rather than the
server owning sockets, the client pull loop is runtime-neutral, storage
goes through an injected dyn-safe trait (`LpFs`), and the newest core
crate (`lpc-history`) takes even randomness as caller-supplied bytes. An
audit (2026-07-06) found **zero production violations** in core crates:
no clock access, no rng, no executor dependencies.

Second, the pain that raised the question would not actually be cured by
an async core: even with an async `LpFs`, the browser simulator should
not await storage from inside engine operations — the right design there
is memory-primary with asynchronous write-behind, which works identically
over a sync trait. The remaining pains (fw-esp32 bridge ergonomics, the
fw-browser spin executor) are localized edge issues, fixable surgically.

## Decision

The core is a **sans-IO core**: pure state machines that never perform
ambient effects. Effects are injected:

- **Storage** via dyn-safe, synchronous traits (`LpFs`).
- **Transport** via envelope-in / frame-out; the core never owns sockets,
  ports, or message pumps.
- **Time** via providers or caller-supplied timestamps (f64 epoch
  seconds, the workspace convention).
- **Randomness** via caller-supplied bytes (`lpc-history` uid minting is
  the reference example).

Async belongs to **platform edges**, which wrap the core in whatever
executor is native (embassy on device, the event loop in the browser,
anything on host).

**Crate taxonomy.** Core: `lp-base/*`, `lp-core/*` (`lpc-*`),
`lp-shader/*` (`lps-*`, `lpvm-*`, `lpir`), `lp-riscv/*`. Edges: `lpa-*`,
`fw-*`, `lp-cli`, `third_party`, spikes. Edge crates may — and should,
where practical — keep their own logic runtime-neutral (the `lpa-client`
pull loop is the model).

**The rules — the prohibition is executor coupling, not the `async`
keyword.** In core crates:

- No executor/reactor dependencies (embassy, tokio,
  `wasm-bindgen-futures`, `futures-executor`, …).
- No task spawning; no sleeping or waiting except through injected
  timers/deadlines.
- No wall-clock reads, no randomness, no ambient IO of any kind.
- Runtime-neutral `async fn`s and futures are permitted when any edge can
  drive them to completion (the engine's project-read streaming is the
  existing example). If a future needs a particular executor to make
  progress, it belongs in an edge crate.
- Test modules count as edges: a null-waker `block_on` loop is acceptable
  in tests that drive immediately-ready futures, and nowhere else.

## Consequences

- Storage locality is an edge concern. The browser gets memory-primary +
  async write-behind (persistent-browser-fs work); the device reads
  LittleFS directly; neither decision leaks into core.
- Deterministic testing is preserved: manual clocks, explicit ticks, and
  the emulator suites keep their "the core does nothing unless fed"
  property.
- The flash cost of async state machines and boxed `dyn` futures on
  ESP32 is avoided; the core compiles the same everywhere.
- The seam tax is real but bounded and localized: edges own bridging
  (e.g. the fw-esp32 embassy↔sync handoff), and bridge smells are fixed
  at the edge (the `fw-browser` spin `block_on` is dispositioned to the
  worker-boot rework in the persistent-browser-fs milestone).

**Revisit triggers** — this decision is standing, not eternal. Reopen it
when the *core itself* must await mid-operation:

- device-initiated network IO inside core flows (e.g. on-device cloud
  project pull over wifi),
- streaming large assets through the engine,
- a genuinely blocking/remote filesystem in a core path.

When a trigger fires, scope the async seam to the specific state machine
that needs it — do not asyncify the core wholesale.

## Alternatives Considered

- **Wholesale async core refactor.** Rejected: viral churn through
  engine, server, both firmwares, and the emulator test harness during
  heavy feature development; unmeasured flash-size cost on a
  budget-constrained target; scheduling nondeterminism where determinism
  is engineered in; and thin payoff, since the motivating browser-storage
  problem is better solved by write-behind regardless of trait shape.
- **SharedArrayBuffer/Atomics sync bridge for browser storage.** Rejected
  at the edge level: requires cross-origin-isolation headers (COOP/COEP),
  which plain static hosting (GitHub Pages included) cannot provide, and
  adds a second worker plus a shared-memory protocol for a ~100 ms
  durability improvement.

## Follow-ups

- Replace the `fw-browser` null-waker spin `block_on`
  (`lp-fw/fw-browser/src/executor.rs`) during the persistent-browser-fs
  worker-boot rework.
- If fw-esp32 embassy↔sync bridge friction persists, give it a dedicated
  cleanup pass at the edge.
- AGENTS.md carries the enforcement checklist for this ADR.
