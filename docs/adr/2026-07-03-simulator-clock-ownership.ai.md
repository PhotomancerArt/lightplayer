# ADR: Simulator Clock Ownership

- **Status:** Accepted
- **Date:** 2026-07-03
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The browser simulator is real firmware (`fw-browser`) compiled to WASM and run
inside a Web Worker. It has no clock of its own: its `ManualTimeProvider` only
advances when a `tick` envelope arrives. Historically the *client transport*
supplied those ticks. `BrowserWorkerClientIo::receive()` posted
`Tick { delta_ms: Some(16) }` to the worker on every ~4 ms poll iteration while
it waited for a protocol response.

That coupling produced three problems:

- The simulator only advanced while a protocol request was in flight. Idle
  shader previews froze even though the user expected them to animate.
- During an active read the sim ran up to ~4x real time (16 sim-ms per 4
  real-ms), so previews sped up whenever the UI happened to be polling.
- The Studio passive-refresh loop doubled as the sim's heartbeat, which forced a
  frantic 33 ms poll cadence and coupled refresh policy to transport identity.

Real hardware has a real clock; only the browser simulator lacked one. The fix
had to give the simulator its own clock without giving up the deterministic,
message-driven ticking that firmware tests and Studio stories rely on to pin
exact frame numbers.

## Decision

The **worker owns the simulator clock**, selected at boot via a tick mode. The
runtime itself is unchanged: it always advances its clock by exactly the delta
each `tick` carries and treats that delta as opaque. Only the *source* of the
delta differs between modes:

- **Self-ticking** (default for the Studio simulator): the worker JS runs its
  own `setInterval` at a ~30 fps sim cadence and, on each fire, measures the
  real elapsed time with `performance.now()` and ticks the runtime with that
  *measured* delta. Previews animate at roughly real time whether or not a
  request is in flight.
- **Explicit** (tests, stories, emulator-style harnesses): no worker timer runs.
  Time advances only when the host sends a `tick` envelope with a chosen delta.
  A fixed delta gives byte-for-byte deterministic advancement.

Mode is chosen at worker startup through a `tick_mode` field on the `boot`
envelope, plumbed from `BrowserWorkerOptions::tick_mode` (default
`SelfTicking`). `BrowserWorkerClientIo::receive()` no longer posts `Tick`; it is
now a pure consumer that polls worker output and produces no simulation side
effects.

## Consequences

- Idle shader previews animate at ~1x real time — the headline user-visible fix.
- Sim speed no longer depends on UI poll cadence; it tracks wall-clock time.
- No `Tick` message originates from client IO code. The receive loop keeps its
  poll-sleep structure (event-driven receive is a later milestone) but has no
  clock side effects.
- The 33 ms simulator refresh interval is now purely a *preview re-read* rate,
  not a sim heartbeat. It is retained so self-ticked previews stay visibly fresh;
  the broader cadence-policy cleanup is deferred.
- Firmware tests and stories keep deterministic frames by using explicit mode:
  the runtime advances only on the ticks they supply.
- Runtime code is identical across both modes, so self-ticking cannot diverge
  from the deterministically tested path — only the delta source changes.

## Alternatives Considered

- **Keep transport-driven ticks, just slow them down:** would still freeze idle
  previews and re-couple sim time to whether a request is pending.
- **A Rust-side timer inside `fw-browser`:** Web Workers have no reliable timer
  primitive reachable from the current wasm-bindgen surface without extra glue,
  and `performance.now()`/`setInterval` already live naturally in the worker JS
  that owns the `postMessage` boundary. Keeping the timer in JS also keeps the
  runtime's tick contract deterministic and host-driven.
- **Self-ticking only, no explicit mode:** simplest, but loses deterministic
  frame pinning for tests and stories — the exact property that makes the
  browser runtime testable.
- **rAF-style vsync ticking:** Workers have no `requestAnimationFrame`
  guarantee, so a measured-delta interval is the portable choice.

## Follow-ups

- Event-driven (postMessage-push) receive so the poll-sleep loop can retire.
- Full passive-refresh cadence policy cleanup, decoupled from transport identity.
- Consider a runtime-switchable mode (mid-session self-tick <-> explicit) if a
  harness ever needs to freeze a live simulator.
