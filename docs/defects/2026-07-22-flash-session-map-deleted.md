---
status: fixed
found: 2026-07-22      # how: live-debugging
fixed: f80077871
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

**Regression coverage** — None: the JS session map has no
host-testable harness today. Noted as a gap; the conformance-suite
chip covers the class of "browser-side state invisible to host tests".

**Lesson** — When two layers both "clean up" the same resource,
ownership was never actually decided — each layer's cleanup is correct
in isolation and wrong in composition. And a purpose-built primitive
with zero callers (`release_session_for_management`) is a smell that a
rewire changed semantics silently: the primitive encoded the old
ownership decision, and its dead body marks where the new code stopped
honoring it.
