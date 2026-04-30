# lpc-view

Client-side view/cache for the LightPlayer engine.

This crate owns client-specific representations and helpers for applying wire
updates, maintaining a local tree view, and exposing UI-friendly access to
engine state.

It is still a core crate because it models one engine's local view. Application
client transports live in `lp-app/lpa-client`.

It should depend on `lpc-model` and `lpc-wire`, not on `lps-shared`. Client
property views use portable wire values rather than runtime shader values.
