# ADR: Studio logging model

- **Status:** Accepted
- **Date:** 2026-07-05
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The Studio console drowned in debug noise: an 80-entry ring received a
Debug-level "heartbeat" entry every second, there was no filtering of any
kind, entries had no timestamps, and "source" was a free-form string that
dropped the richer provenance the producers actually had (firmware module
paths stayed buried in message text; link endpoint/session ids were
discarded; `Trace` collapsed into `Debug`). Five ingestion paths each
hand-mapped into the UI entry type, and studio-side code could not use the
standard `log` macros at all. Firmware verbosity was fixed at flash time
(`LevelFilter::Info` hardcoded at logger init).

## Decision

1. **Structured log entries with a closed origin set.** `UiLogEntry` carries
   a timestamp (`f64` **seconds** since Unix epoch, stamped at ingestion by a
   clock injected into `StudioController` — device clocks are unsynced and
   untrusted), a five-level severity (`Trace..Error`, `Ord`), and a
   `UiLogSource { origin, detail }` where `origin` is the closed enum
   `Studio | Link | Server | Device` and `detail` is optional free text
   (module path, endpoint id, transport label). Origin is the only source
   filter dimension; detail is display/search text.

2. **Display-side filtering with core-owned state.** The ring keeps 1000 raw
   entries regardless of filter; `LogFilter` (min-level threshold, default
   Info+, plus per-origin toggles) is applied when building `UiConsoleView`,
   which carries the passing entries, a hidden count, and the filter state
   for the toolbar. Relaxing the filter therefore reveals already-captured
   history. Filter mutations ride `StudioCommand::Console(...)` — synchronous
   state changes, deliberately not `UiAction`s.

3. **Heartbeats are telemetry, not log entries.** Healthy heartbeats update
   status UI only; recovery/safe-mode conditions still surface as Warn/Error
   entries. Structured provenance is preserved at every ingestion point:
   firmware `[LEVEL] module: msg` serial lines are parsed into
   (level, module→detail, message), link entries keep `Trace` and carry the
   endpoint id as detail, worker log targets become detail.

4. **The `log` crate is the studio-side logging API.** A stateless
   `log::Log` sink buffers records in a `thread_local!` queue (bounded 1024,
   oldest-dropped with a drop-count entry; wasm32 is single-threaded so no
   locking); the studio actor drains it into the ring each batch/tick with
   origin Studio and the module target as detail. Hand-built entries remain
   only as UX policy mapping (`from_notice` / `from_error`). Ring entry is
   the single JS-console mirroring point (a controller `on_entry` hook the
   web shell installs), so every entry reaches the browser console exactly
   once.

5. **Runtime device log level over the wire.** `ClientMsgBody::SetLogLevel
   { level }` (acked by `ServerMsgBody::SetLogLevel`) sets
   `log::set_max_level` process-globally on whichever platform serves the
   protocol (ESP32, emulator, browser worker, host). The wire `LogLevel`
   gained `Trace` and deliberately has no `Off` — a client can never fully
   silence a device. Nothing is persisted: reboot reverts to the init
   default (Info), and Studio tracks its last request optimistically (no
   read-back), reset per connection. Device loggers are constructed
   permissive so the global `log::max_level()` is the single runtime gate.

## Consequences

- The console defaults quiet (Info+) while Debug/Trace history stays
  recoverable from the ring; "N hidden" makes suppression visible.
- Any wasm-side crate gets console logging for free via `log::` macros, with
  module-path attribution.
- Firmware debug output is now reachable without reflashing, via the console
  toolbar's device-level selector.
- The five bespoke ingestion mappings shrank to policy-bearing parsers with
  unit tests; new origins require extending a closed enum (intentional
  friction).
- `UiLogEntry` lost `Eq` (f64 timestamp); equality-based tests compare fields.
- Wire compatibility was broken (new `LogLevel::Trace` variant and message
  types) under the existing heavy-dev no-compat policy.

## Alternatives Considered

- **Per-module (RUST_LOG-style) filter dimensions** — rejected for now: the
  closed origin set keeps the toolbar simple and the filter state small;
  module paths remain visible/searchable as detail. Revisit if origins stop
  being the useful cut.
- **Ingestion-side filtering (drop below-threshold entries at the door)** —
  rejected: raising verbosity would show nothing until new entries arrive,
  and the 1000-entry ring is cheap.
- **Demoting heartbeats to Trace instead of removing them** — rejected: they
  are telemetry with a dedicated UI; keeping them would burn ring capacity
  (one entry/second) for no diagnostic value.
- **Milliseconds or integer timestamps** — f64 seconds chosen as the repo/user
  convention for f64 time values.
- **Web constructing `UiAction`s for the device-level selector** — rejected;
  the toolbar sends a `ConsoleCommand` that the actor converts into the
  `DeviceOp::SetLogLevel` action at intake, preserving the "controllers
  create actions, web renders metadata" split.
- **An `Off` wire level** — rejected: remote-silencing a device you may only
  reach over that same log channel is a footgun.

## Follow-ups

- Structured `ServerMsgBody::Log` frames from firmware (the receive path is
  live and mapped; nothing sends it yet) to replace prefix-parsed serial text.
- Host-process `lpa-server` stdout capture into the console (today those
  logs are terminal-only).
- Console filter persistence (session-only today) and text search.
- Pixel-tolerance story-image comparator (byte-equality flaps on ~10–20
  icon-heavy stories due to sub-pixel AA jitter).
