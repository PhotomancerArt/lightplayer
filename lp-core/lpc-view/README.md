# lpc-view

Client-side view/cache for the LightPlayer engine.

This crate owns client-specific representations and helpers for applying wire
updates, maintaining a local tree view, and exposing UI-friendly access to
engine state.

It is still a core crate because it models one engine's local view. Application
client transports live in `lp-app/lpa-client`.

It should depend on `lpc-model` and `lpc-wire`, not on `lps-shared`. Client
property views use portable `LpValue` from wire updates, not runtime shader
values (`LpsValueF32`).

Project reads retain the latest runtime status alongside the structural project
mirror. UI/controller layers can therefore summarize frame counters, runtime
buffers, and server memory without owning protocol response details themselves.

**Naming:** Structures that mirror engine state locally use natural `*View`
suffixes (`ProjectView`, `NodeTreeView`, `PropAccessView`, `PropsMapView`, …).
Reserve `Client*` for genuine client-side types (for example `lpa-client`'s
`LpClient` or this crate's `ClientResourceCache`), not for cached
tree/property mirrors.
