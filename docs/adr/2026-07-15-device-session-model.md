# ADR: DeviceSession — one owned hardware link with hello-first readiness

- **Status:** Accepted
- **Date:** 2026-07-15
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Before this decision, "a connected device" was smeared across four
layers with no single owner:

- The studio's `LinkController` drove provider open/discover/connect
  flows while holding `registry.borrow_mut()` across provider awaits
  (six sites), and its "reopen" re-used a connection whose serial
  thread was already dead.
- Readiness was decided by TWO string-grep boot-line classifiers with
  DISAGREEING rules (the browser client-io required marker OR frame;
  the test-edge io required marker AND frame), duplicated between
  `lpa-studio-core` and test edges.
- Per-provider-kind client-io adapters (`server_io_from_link_connection`)
  matched on transport kinds inside the studio; host serial had NO
  product adapter at all (the desktop hole), and timeouts existed only
  as an ad-hoc bounded poll inside one adapter — a mid-request stall
  after readiness hung forever.
- The simulator (browser worker) rode the same code paths as hardware
  and was told apart by flags (`should_auto_load_demo_project`,
  `is_hardware_link`).

Both M5-era hardware bugs (pull-before-readiness ordering, fresh-device
misclassification) lived in exactly this band — below the record-level
APIs, in framing/classification/timing — which is also why the fake
device (`FakeEsp32Device`) injects at the byte stream.

The wire hello handshake (`docs/adr/2026-07-14-wire-hello-versioning.md`)
had just landed and supplied the missing readiness primitive: an
unsolicited, self-describing `ServerHello` carrying the wire protocol
version.

## Decision

One type, `lpa_link::DeviceSession` (module
`lp-app/lpa-link/src/device_session/`), owns each attached HARDWARE
device end to end: the connector that opened it, the live link
session/connection, the app-protocol channel, an observable state
machine, readiness, timeouts, management, and reconnect.

### Hardware-only session owning its connector

`DeviceSession::connect(connector, endpoint_id, timers, sink)` takes an
owned `Rc<LinkConnector>` and performs the connect / protocol-open /
connection handoff itself. The connector instance moves INTO the
session for its lifetime; consumers reach the wire only through the
session's gated surfaces. `LinkProviderRegistry` is demoted to a
**catalog + factory**: `descriptors()` for picker UI,
`create_connector(kind)` for a fresh owned connector per open flow.
Nothing borrows a shared mutable registry on hot paths — the
borrow-across-await violations disappeared structurally, and all
`LinkProvider` methods take `&self` with internally scoped borrows.

### Device-class capabilities, not transport leaks

The session never matches on transport kinds for behavior. What a
device can do comes from connector-level `LinkCapabilities` and
descriptor metadata (labels, flash-capable classes); the studio derives
connect actions and transport labels from the catalog rather than
hardcoding `BrowserSerialEsp32`. Future connector classes (websocket,
server-lightplayer) slot in as new catalog entries whose capabilities
say what they support — a websocket device simply has no flash
capability and no boot classification, and nothing above the connector
needs to learn its transport. The two wire shapes that exist today
(`DeviceWire::Transport` on host; `DeviceWire::BrowserLines` on wasm,
where `M!` lines ARE the frames) are private to the session.

### Hello-first readiness; the boot classifier is diagnosis-only

Readiness is granted by exactly one thing: the unsolicited wire
`ServerHello` whose `proto` matches `WIRE_PROTO_VERSION` (the handshake
of `2026-07-14-wire-hello-versioning.md`; this ADR is the policy
consumer named there). The observable state machine:

```text
connect() ──▶ Booting ──hello(proto ok)──────────────▶ Ready { hello }
                │
                ├─boot lines match no-firmware sig──▶ BlankFlash
                │                                     Bootloader
                │                                     ForeignFirmware
                ├─non-hello frame / wrong proto──────▶ Incompatible
                ├─deadline, server marker seen───────▶ Incompatible (NoHello)
                ├─deadline, no classification────────▶ Unresponsive
                └─stream EOF / transport lost────────▶ Gone
Ready ──transport lost / close()──────────────────────▶ Gone
```

The boot-line classifier (`BootLineClassifier`, moved into `lpa-link`)
is DIAGNOSIS-ONLY: it explains why a device is not ready (blank flash,
ROM bootloader, recognized foreign firmware, silence) and never grants
readiness. The two disagreeing studio classifiers are deleted. An
`M!`-speaking peer that never identifies itself — a non-hello first
frame, a wrong-proto hello, or a started-but-silent server (boot marker
seen, deadline expired) — is `Incompatible`, whose ONE affordance is
reflash (the conservative always-upgrade policy of the hello ADR).

Readiness is driven ON DEMAND — `wait_ready()` or the channel's first
use — with no background task, consistent with the single-actor world
on both wasm and host.

### Mode-exclusive wire access

A hardware device has one wire. `DeviceMode` (`AppProtocol` /
`Management`) gates it by construction: the readiness-gated channel
(`client_io()`) errors cleanly outside `Ready` + `AppProtocol` (nothing
is ever written to a device that is not ready), and
`try_begin_management()` takes the wire behind an RAII guard — refused
while an app-protocol request is mid-send/receive (`ChannelUseGuard`
counter), so the wire is never torn down under a request.

### Timeouts → Unresponsive; reconnect = rebuild

All timing comes through an injected `DeviceTimers` factory (tokio
sleeps on host, gloo on wasm; the `StudioActor` pattern), keeping
`lpa-link` executor-free per `2026-07-06-sans-io-core.md`. Per-operation
deadlines: connect, ready, and request-idle; a request-idle expiry
lands the device on `Unresponsive`. This leans on the
every-request-gets-a-response server invariant (failed handlers answer
with an `Error` frame) — a deadline expiry therefore means a genuinely
unresponsive wire, not a slow handler.

Terminal states (`Gone`, `Incompatible`, `Unresponsive`, the diagnosis
states) are sticky under passive observation. The one way out is an
explicit rebuild: `reconnect()` — and `manage()`'s post-operation step —
opens a brand-new provider session and transport on the same endpoint
(the old serial thread is dead, never reopened), clears observed-line
state so stale boot lines cannot classify the new link, and re-runs
readiness from `Booting`. Channels handed out earlier read the current
wire through the session, so they survive link generations.
`manage(request, sink)` wraps the whole flash/erase cycle in that
shape: release the link → run the connector operation (events folded
into `DeviceEvent`) → rebuild → re-ready; post-erase the state lands on
`BlankFlash`, which IS success for an erase.

### Sim is not a device — a plumbing rule, not UX

D22 ("the sim is not a device") is enforced in the type system, not by
flags. The studio's `DeviceController` holds a
`RuntimeAttachment { None, Sim(SimAttachment), Device(DeviceHandle) }`:
the sim arm is connector + worker io with no boot, no readiness states,
no hello, and no management plane; the device arm is always a
`DeviceSession`. Deploy environment, pane visibility, and transport
labels derive from the attachment kind and session state — the old
sim-detection flags are deleted.

### Identity rides the hello

Device identity lives at the device fs ROOT (`/.lp/device.json`);
firmware reads it at boot and `ClientRequest::Hello` answers re-read
it, so `hello.device_uid` is authoritative and re-stamp-after-push is
gone (recorded as the amendment in
`2026-07-14-wire-hello-versioning.md`, "Future intent").

## Consequences

- The studio's `LinkController`, its `LinkState` machinery, both studio
  client-io adapters, and the studio boot classifier are DELETED;
  `DeviceController` keeps view/action wiring, the catalog, and the
  attachment; `ServerController` attaches via
  `attach_device_session(&session)` using the session's channel. The
  desktop client-io hole is closed as a side effect (host serial goes
  through the same session).
- Everything in the session layer stays `!Send` (`Rc`-based sink,
  wasm holds `JsFuture`/`Closure` across awaits); the CLI will drive it
  single-threaded when it adopts the session (device-link M5).
- No `RefCell` borrow is held across an await anywhere in the layer;
  the only awaits are injected timers and the transport.
- The e2e regression harness (`studio_link_e2e_tests.rs`) runs the REAL
  path against the byte-level fake device, with rows for
  Incompatible (hello suppressed / proto mismatch), Unresponsive with
  reconnect recovery, reconnect-after-Gone, and erase-lands-BlankFlash.
- Failure surfaces are populated instead of vestigial:
  `LinkSessionStatus::Error` carries management/rebuild failures, and
  the snapshot derives `LinkEndpointStatus` from the state.

## Alternatives Considered

- **Boot-line classification as readiness** (status quo): rejected —
  the two live implementations already disagreed (OR vs AND), the
  signal cannot carry a protocol version, and the hello handshake made
  string-grep readiness redundant. The classifier survives only as
  diagnosis.
- **Background readiness task**: rejected — it would force an executor
  choice into `lpa-link` (against the sans-IO rule) and add
  cross-task state for no benefit; on-demand driving matches the
  single-actor consumers on both platforms.
- **Reopening the existing connection after management/disconnect**:
  rejected — the serial thread behind a closed transport is gone;
  "reopen" that reuses the dead channel was the finding-8 bug. Rebuild
  is honest and also what management needs anyway.
- **Routing the sim through DeviceSession**: rejected (D22) — a sim has
  no boot, no hello race, no bootloader, no flash. Pretending it is a
  device forces every state to grow a fake arm; a separate attachment
  variant is smaller and truthful.
- **`Send` bounds on the session layer**: rejected — the wasm build
  holds JS futures across awaits; `Send` would split the API in two for
  a thread-safety no consumer needs (one actor drives everything).
- **Per-message capability negotiation on version mismatch**: rejected
  in `2026-07-14-wire-hello-versioning.md`; this ADR only consumes that
  policy (`Incompatible` → reflash).

## Follow-ups

- CLI adoption of `DeviceSession` (device-link M5) — lp-cli still
  hand-rolls provider/session bundles; `fwcheck`'s boot-line grep dies
  then.
- Host-side flash/erase management via espflash-lib (M5) and an esptool
  simulator for the browser path (M6).
- Connect UX redesign (M7): step-by-step readiness activity view,
  non-ESP32-worded connect copy.
- Websocket / server-lightplayer connector classes when they become
  real; the capability model above is the contract they slot into.
