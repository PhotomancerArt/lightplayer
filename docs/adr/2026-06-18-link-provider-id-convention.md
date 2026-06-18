# ADR: Link Provider ID Convention

- **Status:** Accepted
- **Date:** 2026-06-18
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

`lpa-link` provider IDs are becoming durable application vocabulary. Studio, the
CLI, tests, and future agent harnesses will use these IDs to describe how an app
discovers, manages, and connects to LightPlayer runtimes or devices.

The initial provider names used `local-host` and `local-browser`, and early
serial planning used names such as `serial-esp32-host`. That ordering did not
scale cleanly once the same mechanism needed different host and browser
capabilities, such as host serial versus browser Web Serial, or host websocket
discovery versus browser websocket constraints.

## Decision

Use kebab-case provider IDs with this grammar:

```text
{environment}-{mechanism}-{target?}
```

The environment identifies where the provider runs, such as `host` or
`browser`. The mechanism identifies how the provider reaches or owns the runtime
or device, such as `process`, `worker`, `serial`, or `websocket`. The target is
optional and is included when management behavior is target-specific.

Canonical examples:

- `host-process`
- `browser-worker`
- `host-serial-esp32`
- `browser-serial-esp32`
- `host-websocket`
- `browser-websocket`

Use Rust module/type names that match the provider ID in Rust style, such as
`providers::host_serial_esp32::HostSerialEsp32Provider`.

## Consequences

Provider IDs now expose capability differences before the caller inspects the
provider. For example, `host-websocket` and `browser-websocket` can differ in
discovery, permissions, and network constraints without overloading a generic
`websocket` provider name.

ESP32 serial providers remain target-specific because flashing, reset,
boot-mode handling, and raw filesystem access are device-family behavior, not
generic serial behavior.

The early `local-host` and `local-browser` names are renamed to `host-process`
and `browser-worker` instead of being carried forward as permanent aliases.

## Alternatives Considered

- `{mechanism}-{target}-{environment}`, such as `serial-esp32-host`.
  - Rejected because it groups host and browser variants apart even though the
    environment determines discovery, permissions, and management capabilities.
- Generic provider IDs such as `websocket` or `serial`.
  - Rejected for providers whose behavior differs materially by environment or
    target family.
- Keep `local-host` and `local-browser`.
  - Rejected because "local" hides the containment model that matters to the
    link layer: host process versus browser worker.

## Follow-ups

- Add `browser-serial-esp32` when Web Serial hardware support is implemented.
- Add `host-websocket` and `browser-websocket` separately when websocket
  discovery and connection behavior is ready.
