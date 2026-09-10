---
status: fixed
found: 2026-07-22      # how: live-debugging
fixed: c9a75fa0e
area: lpa-link/browser-serial
class: lifecycle-ownership
related: ["2026-07-16-browser-serial-endpoint-lost", "LpFs conformance-suite chip"]
---
# closePort deleted the session-map entry flashFirmware still needed

**Symptom** — Flashing after entering management failed with:

```
Flashing failed: link error: Unknown browser serial session: 1
```

Path: `flashFirmware → getPort → requireSession` — the session id was
valid seconds earlier.

**Root cause** — Two layers both "cleaned up" the session.
`DeviceSession::manage` releases the link via the trait `close()`;
the JS `closePort` implementation deleted the module-map entry — the
entry holding the `SerialPort` grant handle that `flashFirmware` needs
seconds later. Meanwhile the provider's `manage_inner` *also* releases
the protocol itself via `release_session_for_management` — the
purpose-built primitive for exactly this handoff, which has sat dead
with zero callers since the link rewire. The rewire changed who
releases what, and nobody re-decided ownership.

**Fix** — `closePort` releases the streams but *keeps* the map entry:
grant handles persist across close, and ids are stable per port
identity, so a later `flashFirmware` finds the port it was granted.

**Regression coverage** — Now covered (2026-09-09, emulator plan two
M2 → PR #642). The JS session map got a host-testable harness: the
browser-serial conformance suite
(`lp-app/lpa-link/tests/browser_serial_conformance.rs`) drives the
shipped JS over a `navigator.serial` polyfill. This exact
close-keeps-the-grant behaviour is pinned by
`a_closed_port_keeps_its_session_and_a_forgotten_one_does_not` (a
`close()`d port keeps its session, only `forget()` drops it) and
`the_flash_bridge_acquires_the_live_generation` (the flash bridge finds
the port it was granted). See
`../debt/web-serial-js-untestable.md` (retired) for the harness and its
residue. Originally "None": the JS session map had no host-testable
harness, and the conformance-suite chip covered the class of
"browser-side state invisible to host tests".

**Lesson** — When two layers both "clean up" the same resource,
ownership was never actually decided — each layer's cleanup is correct
in isolation and wrong in composition. And a purpose-built primitive
with zero callers (`release_session_for_management`) is a smell that a
rewire changed semantics silently: the primitive encoded the old
ownership decision, and its dead body marks where the new code stopped
honoring it.
