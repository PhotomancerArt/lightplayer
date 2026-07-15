# Wire hello and protocol versioning

Date: 2026-07-14
Status: accepted

## Context

During heavy development, wire/protocol compatibility is deliberately not
maintained (see AGENTS.md, "Wire/protocol compatibility"): client, server,
and firmware ship in lockstep, wire changes delete the old form outright,
and there are no serde aliases, version shims, or dual-format decode paths.
That policy always named its exit: once devices are fielded and can no
longer be upgraded in lockstep, the mechanism would be "an explicit
capability/version handshake negotiated at connect time — never error-text
sniffing or silent format probing".

Device management work (device-link architecture M2) needs that handshake
now: Studio must be able to tell "this device speaks a different protocol"
apart from "this device is broken", and offer the right remedy (reflash)
instead of surfacing decode garbage.

## Decision

### The hello contract

The protocol gets a self-describing bootstrap, `ServerHello`
(`lp-core/lpc-wire/src/server/hello.rs`):

```
ServerHello {
    proto: u32,            // compare against WIRE_PROTO_VERSION
    fw: FwProvenance {     // package, commit, dirty, profile
        package: String,   // "fw-esp32" | "fw-host" | "fw-browser" | …
        commit: String,    // short git commit, or "unknown"
        dirty: bool,
        profile: String,   // cargo profile, e.g. "release-esp32"
    },
    device_uid: Option<String>,  // stamped "dev_…" identity, if any
}
```

It is delivered two ways, both mandatory for every lpa-server embedder:

- **Unsolicited**: as the first id-0 frame the server loop sends when it
  starts serving (before/with the first heartbeat, following the heartbeat
  id-0 precedent).
- **On request**: `ClientRequest::Hello` is answered with
  `ServerMsgBody::Hello` at any time.

The server is sans-IO: the hello payload is injected by the embedder
(`LpServer::set_hello`) at construction time; the server never reads git,
env, clocks, or ambient state to build it (the one non-ambient exception:
`device_uid` is re-read from the injected base fs's root identity file
when answering a hello request). Boot logs carry the same facts
(`[INIT] fw-esp32 initialized, starting server loop... proto=1 commit=<c>
dirty=<b>`) so a serial capture is self-describing even without a protocol
client.

### The hand-bumped integer rule

`WIRE_PROTO_VERSION` (`lpc-wire`) is a single `u32`, hand-bumped on every
breaking wire change: renamed/removed/retyped fields, changed variants,
changed encodings, changed semantics of existing messages. There is no
minor/patch structure and no capability list — one integer, compared for
equality. Purely additive changes (a new message the other side never has
to understand) need not bump it, but when in doubt, bump; versions are
free, debugging silent misdecodes is not.

### Absence is the mismatch signal

Pre-hello firmware never sends a hello and errors on the `Hello` request.
Therefore a connected, responding server that produces no hello IS the
version-mismatch signal — no dual classifier, no format probing, no
"legacy mode". This covers all pre-M2 firmware with zero compat surface.

### Conservative policy: always upgrade the firmware

When versions differ (or the hello is absent), assume **nothing** works.
No negotiation, no graceful degradation, no per-message compatibility
matrix (explicitly rejected while the no-wire-compat policy holds). The
one smooth path is always "upgrade the firmware to match the Studio/client
in hand" — the client is never expected to speak old protocols. The UX
consumer of this policy (the `Incompatible` device state, reflash
affordance) is `DeviceSession` (device-link M4; see
`docs/adr/2026-07-15-device-session-model.md`); this ADR fixes the
contract it consumes.

The Studio firmware manifest records the wire version it would flash
(`build.wireProto` in `studio-firmware-manifest.mjs` output, extracted
from the const by the packaging recipe), so "manifest we'd flash" vs
"device hello" is a pure integer comparison — no ELF parsing.

## Future intent (recorded, not implemented)

- **Project-data versioning** is a separate axis: a device can speak the
  current protocol but hold project files written by an older format.
  `ServerHello` deliberately does NOT carry a project-format field today —
  fields are added when they become real, not speculatively. When
  project-data upgrade work lands, the hello is where its version belongs.
- **device_uid from firmware** — resolved by device-session M4/P4: the
  stamped identity moved to `/.lp/device.json` at the DEVICE FS ROOT, so
  firmware reads it at boot and `ClientRequest::Hello` answers re-read it
  (the uid is now root-sourced, never `None` on a stamped device).

## Consequences

- lpc-wire carries `WIRE_PROTO_VERSION`, `ServerHello`, `FwProvenance`,
  `ServerMsgBody::Hello`, `ClientRequest::Hello`.
- All three embedders (fw-esp32, fw-host, fw-browser) inject provenance
  and emit the boot hello; fw-esp32's comes from `build.rs`-captured git
  state (`LP_BUILD_COMMIT`/`LP_BUILD_DIRTY`/`LP_BUILD_PROFILE`, falling
  back to `unknown`/`false` when git is absent).
- lpa-client exposes the last-seen hello and a typed `hello()` call; no
  policy lives in the client.
- The boot line keeps its marker substring, but string-grep readiness is
  gone from the app layer: device-session M4 deleted the studio's
  readiness-granting classifier and made the hello the only readiness
  signal; boot-line classification survives in `lpa-link` as
  DIAGNOSIS-ONLY (`BootLineClassifier` — see
  `docs/adr/2026-07-15-device-session-model.md`). The CLI's `fwcheck`
  grep remains until the CLI adopts `DeviceSession` (device-link M5).
