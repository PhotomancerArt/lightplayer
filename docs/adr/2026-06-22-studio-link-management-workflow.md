# ADR: Studio Link Management Workflow

- **Status:** Accepted
- **Date:** 2026-06-22
- **Deciders:** Photomancer
- **Builds On:** [2026-06-21 Studio UX Layer](./2026-06-21-studio-ux-layer.md)

## Context

Studio now has a UI-independent UX layer that owns `lpa-link`, `lpa-client`,
server state, project state, and action dispatch. The next missing hardware
workflow is the blank-device lifecycle:

- provision a blank ESP32-C6 with packaged LightPlayer firmware;
- reset an existing ESP32-C6 back to blank.

These operations are below the `lp-server` protocol. A blank device may not be
running a server at all, and a full-device erase intentionally destroys the
firmware and server filesystem. Modeling those operations as `lpa-client`
requests or Dioxus component logic would put ownership in the wrong layer.

## Decision

Add a provider-neutral link management API to `lpa-link`:

- providers advertise low-level support through `LinkCapabilities`;
- callers execute session-scoped requests with `LinkProvider::manage`;
- requests use `LinkManagementRequest`;
- results use `LinkManagementResult` plus compact management progress/log data;
- long-running providers can additionally publish `LinkManagementEvent` values
  through `manage_with_events`;
- unsupported providers return `LinkError::OperationUnsupported`.

The initial implemented requests are:

- `FlashFirmware`: write the provider-configured firmware image set to the
  connected target;
- `EraseDeviceFlash`: erase the whole device flash so the ESP32 returns to an
  unprovisioned blank state.

Do not overload reset/reboot vocabulary for destructive blanking.
`LinkOperation::Reset` remains non-destructive runtime/device reset.
`LinkOperation::EraseDeviceFlash` is the destructive full-device erase
capability.

Browser Web Serial ESP32 is the first concrete management provider. It owns Web
Serial permission, port ownership, ESP32 bootloader access, firmware manifest
loading, `esptool-js` integration, and protocol release/reopen behavior. Before
probe, firmware flash, or full erase, it releases normal server/protocol serial
ownership so bootloader tooling can take the port exclusively.

Browser serial server protocol open/reopen also performs a hard reset before
opening the JSON-lines protocol stream. The browser serial client then waits for
the first valid protocol frame before sending the first request, so the initial
project probe is not lost while firmware is still booting. If the readiness wait
or probe still fails, Studio marks the server failed instead of leaving the
server pane in a misleading connected state.

Use the packaged Studio firmware manifest at
`./firmware/esp32c6/manifest.json`. Use a pinned browser esptool module,
`https://cdn.jsdelivr.net/npm/esptool-js@0.6.0/+esm`, as the default development
path. A browser ESM CDN endpoint is required because the raw package ESM imports
dependencies such as `pako` by bare specifier, which browsers cannot resolve
directly. The selected endpoint has been checked against the ESP32-C6 stub
decode path used by reset/provisioning. Applications can override the module
path through `BrowserSerialEsp32Options` when they want to serve the dependency
themselves.

Expose the workflow through `lpa-studio-ux` actions:

- `Provision firmware` is a primary link action when a connected link supports
  firmware flashing and Studio is not attached to a server;
- `Reset to blank` is a tertiary destructive link action when a connected link
  supports whole-device erase;
- both actions carry `ActionConfirmation` metadata and are rendered generically
  by the web UI;
- `StudioUx` clears server/project state before provisioning or erasing because
  either operation invalidates the old server connection;
- after provisioning, Studio attempts to reopen the server protocol and resume
  the normal server/project workflow;
- after reset-to-blank, Studio leaves server/project detached and keeps the
  link context provisionable when the browser still holds the serial permission.
- live management progress is surfaced as pane-scoped `UxActivity`, including
  progress bars and raw esptool terminal output for browser serial flash/erase.

Zip upload/download is out of this slice. If raw filesystem backup/restore is
added later, it should read or write direct device/LittleFS image bytes through
link-level management, not route through the running server filesystem API.

## Consequences

- Blank-device provisioning and reset-to-blank are now part of the same
  `UxAction` language as simulator start, server attach, and project actions.
- Web UI remains presentation-only: it renders action metadata, confirms
  destructive actions, and dispatches the selected `UxAction`.
- Agents and future CLI/desktop shells can inspect the same UX tree and see
  link management actions without learning browser serial or esptool details.
- `lpa-link` becomes the durable home for low-level device management, while
  `lpa-client` remains the durable home for server protocol/project operations.
- Flash/erase no longer need to leave the UI opaque while awaiting a single
  final result; the UX layer can publish live activity updates without moving
  provider ownership into the web UI.
- The default esptool path depends on a pinned remote module. This is acceptable
  for the current development slice and is explicitly overridable for packaged
  deployments.

## Alternatives Considered

- Implement provisioning directly in Dioxus components.
  - Rejected. The web app should not own Web Serial/esptool resource lifecycle or
    decide server/project invalidation policy.
- Treat reset-to-blank as a server filesystem operation.
  - Rejected. A full blank reset destroys firmware and server state; it must be
    below the server protocol.
- Hide provisioning behind server connect failure handling.
  - Rejected. Provisioning is a first-class capability of a connected management
    link, not an error recovery side effect.
- Implement zip filesystem backup/restore now.
  - Deferred. The immediate product need is provisioning a blank device and
    resetting an existing device to blank. Future filesystem backup/restore
    should use raw LittleFS/device image bytes.
- Self-host `esptool-js` immediately.
  - Deferred. The pinned remote module keeps this slice small while preserving
    an explicit override path for deployments.

## Follow-Ups

- Validate provision/reset on real ESP32-C6 hardware in the browser and record
  any timing or reconnect refinements.
- Self-host or vendor the browser esptool module for offline/deployed Studio
  builds.
- Add host-serial ESP32 management support using the same request/result model.
- Add direct raw LittleFS image read/write if backup/restore becomes a priority.
- Add cancellation/retry affordances for long-running management activity if
  flash/erase failures need more recovery control.
