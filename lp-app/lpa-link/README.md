# lpa-link

`lpa-link` provides the mechanism by which an application like Studio or the CLI
connects to an `lp-server`.

Link providers allow discovery and management of their transports and underlying
hardware. A serial link for ESP32 provides firmware flashing, resetting, raw
filesystem access, diagnostics, and a client connection to the running
LightPlayer server.

This crate sits below Studio capabilities and beside `lpa-client`. A link
provider owns discovery, endpoint identity, endpoint status, low-level
management, raw logs, diagnostics, and opening a server/client connection. Once
a connection exists, `lpa-client` remains the typed client API for talking to
`lp-server`.

## Why This Is Not Just Transport

Real LightPlayer links need more than `connect()`. Depending on the provider,
the same low-level surface may need to discover ports or workers, report what is
connected, reset a device, flash firmware, inspect raw filesystem state, read
diagnostics, stream logs, and then open a server connection.

Studio should build product capabilities above this crate. It should not embed
Web Serial, browser-worker, host-process, flashing, or endpoint-management
details directly in UI code.

## Endpoint, Session, Connection

The central lifecycle is:

```text
LinkProvider::discover() -> LinkEndpoint
LinkProvider::connect(endpoint_id) -> LinkSession
LinkProvider::connection(session_id) -> LinkConnection
LinkConnection::server_client() -> lpa-client
```

- **Endpoint:** a provider-visible target that can be opened. It is a
  discoverable candidate, not a live resource. Examples include an ESP32 serial
  port, a browser worker runtime target, a spawnable `fw-host` runtime, or a
  future websocket target.
- **Session:** provider-neutral snapshot/handle for an opened endpoint. The
  provider owns the concrete resources behind the session, such as an open
  serial port, a spawned `fw-host` runtime, or a browser worker identity.
- **Connection:** the handoff from a live session to the `lp-server` protocol
  layer. A connection is not a project session and does not replace the owning
  link session.

## Management API

Some link operations happen below the running `lp-server`: firmware
provisioning, full-device erase, raw filesystem image access, bootloader
probing, and low-level reset/recovery. Providers advertise the operations they
can perform through `LinkCapabilities`, then execute supported operations
through the session-scoped management API:

```rust
provider
    .manage(session.id(), LinkManagementRequest::FlashFirmware)
    .await?;
```

Callers that need live UI feedback can use `manage_with_events` with a
`LinkManagementEventSink`. Providers that can observe progress stream terminal
log lines and compact progress entries while the operation runs; providers that
only have final results fall back to replaying the result logs/progress through
the same event vocabulary.

`LinkManagementRequest` is provider-neutral, while each provider owns the
target-specific work needed to satisfy it. For browser Web Serial ESP32, this
means releasing normal server/protocol ownership of the serial port before
taking exclusive bootloader ownership for flash or erase.

The current implemented management operations are:

- `FlashFirmware`: write the provider-configured LightPlayer ESP32-C6 firmware
  manifest/images to the device.
- `EraseDeviceFlash`: erase the whole device flash so the ESP32 returns to a
  blank, unprovisioned state.

Raw filesystem image erase/read/write are modeled as link-level operations but
are future work. They should operate on direct device/LittleFS image bytes below
the server, not on the server filesystem API used for normal project upload.

## Server Connections

`LinkConnection` is the handoff point from an open link session to the server
protocol. Host providers currently expose a `LinkServerConnection`, which is a
shared host `lpa-client` transport and can be wrapped as a `TokioLpClient` with
`server_client()`.

Browser providers own their browser resource bindings:

- `browser-worker` owns the JavaScript module Worker wrapper and lifecycle.
- `browser-serial-esp32` owns Web Serial permission/open/release/close and ESP32
  probe/flash bindings.

`lpa-studio-core` adapts provider send/receive streams into
`lpa-client::ClientIo`; UI shells should not reimplement provider resource
ownership, request ids, response correlation, server error handling,
heartbeat/log handling, or project deploy ordering.

## DeviceSession

`DeviceSession` (module `device_session`, host features) owns one HARDWARE
link end to end: it takes an owned `Rc<LinkConnector>`, performs the
connect/protocol-open/connection flow itself, and exposes an observable
state machine plus a readiness-gated `lpa_client::ClientIo` channel. Sim
runtimes (browser worker) bypass it — they have no boot, no hello race, and
no management plane.

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

Key contracts:

- **Hello-first readiness.** The session is `Ready` ONLY when the
  unsolicited wire `ServerHello` arrives with a matching
  `WIRE_PROTO_VERSION`. A wrong-proto hello, a non-hello frame before any
  hello, or a started-but-silent server (boot marker seen, deadline expired)
  is `Incompatible` — the single affordance is reflashing. The boot-line
  classifier (`BootLineClassifier`) is DIAGNOSIS-ONLY: it explains the
  non-ready states and never grants readiness.
- **Injected timers.** `DeviceTimers` wraps a caller-supplied sleep factory
  (tokio on host, gloo on wasm) plus per-operation deadlines
  (connect / ready / request-idle). `lpa-link` has no executor dependency;
  readiness runs inside the session's own async methods — `wait_ready()` or
  the channel's first use — with no background task.
- **Readiness-gated channel.** `client_io()` returns a `ClientIo` that
  drives readiness on first use and errors cleanly outside
  `Ready` + `DeviceMode::AppProtocol`. Nothing is ever written to a device
  that is not ready, and no-firmware gate errors carry the classifiable
  `NO_FIRMWARE_DETECTED_PREFIX`.
- **Mode-exclusive wire.** `DeviceMode` (`AppProtocol` / `Management`) gates
  access by construction: `try_begin_management()` takes the wire (RAII
  guard releases it) and the app-protocol channel is invalidated while held.
  P3's management orchestration builds on this.
- **Observation.** Pull `snapshot()` (state + link session record + recent
  boot lines) or subscribe a `DeviceEventSink` (`Rc`-based, `!Send`) for
  state transitions, device console lines, and progress. On
  `Incompatible`/`Unresponsive`/`Gone` the session record's status becomes
  `LinkSessionStatus::Error`.

## Providers

`LinkProviderRegistry` is a **catalog + factory**, not a store of live
providers. Applications can inspect the provider kinds compiled into
`lpa-link` without duplicating the feature/target matrix, then create an
OWNED connector per open flow:

```rust
let registry =
    lpa_link::providers::LinkProviderRegistry::from_env(lpa_link::providers::LinkEnv::default());
let providers = registry.descriptors(); // catalog, for picker UI
let connector = registry.create_connector(kind)?; // Rc<LinkConnector>, per open flow
```

The returned `LinkProviderDescriptor` values contain provider kinds, labels,
and low-level `LinkCapabilities`. The registry only catalogs kinds compiled
for the current feature/target matrix, so every descriptor it returns is
usable in the current build/runtime. `LinkProviderKind` owns the stable
kebab-case key used at app boundaries. Product surfaces such as Studio should
map these descriptors into their own UX-facing provider cards, intents,
ordering, and recovery actions.

### Connector ownership

`LinkConnector` is the enum-dispatched owned handle over one concrete
provider (used because `LinkProvider` has async methods and is not
object-safe). The connection OWNER — Studio's link flow today, `DeviceSession`
next — holds `Rc<LinkConnector>` and hands clones to client I/O adapters;
nothing borrows a shared mutable registry on hot paths. All `LinkProvider`
methods take `&self`: each provider keeps its endpoint/session state behind
internal `RefCell`s with borrows scoped to synchronous sections, never across
an `await`. A connector is created per open flow and may discover several
endpoints, but once connected it serves that one connection.

Tests preconfigure connectors (`FakeProvider::with_endpoint`,
`with_device_endpoint`, the `with_*_error` knobs) and hand them to the
registry with `insert(provider)`; `create_connector` then returns that shared
preconfigured instance for the kind, so scripted state survives re-opens.

| Provider key | Rust module/type | Runtime or device | Endpoint kind | Management intent | Status |
|---|---|---|---|---|---|
| `fake` | `providers::fake::FakeProvider` | none (record-level) or scripted `FakeEsp32Device` (feature `fake-device`) | test endpoint | record-level: diagnostics only; device-backed: full set (reset, flash, erase, logs, diagnostics) as scripted transitions | implemented; device-backed sessions return a real host `LinkServerConnection` |
| `host-process` | `providers::host_process::HostProcessProvider` | host process running `fw-host` | spawnable host runtime | logs, diagnostics, future local filesystem/runtime controls | implemented; returns host `LinkServerConnection` |
| `browser-worker` | `providers::browser_worker::BrowserWorkerProvider` | `fw-browser` Web Worker | browser worker runtime | logs, diagnostics, worker lifecycle | implemented; owns Worker wrapper/lifecycle |
| `host-serial-esp32` | `providers::host_serial_esp32::HostSerialEsp32Provider` | ESP32 over host serial | physical serial device | connect (optional reset-after-open), logs, diagnostics; future reset/flash/raw filesystem | implemented for discovery/connect; returns host `LinkServerConnection` |
| `browser-serial-esp32` | `providers::browser_serial_esp32::BrowserSerialEsp32Provider` | ESP32 over Web Serial | physical serial device | connect, provision firmware, erase to blank, reset, logs, diagnostics; future raw filesystem | implemented for browser Web Serial/probe/flash/erase ownership |
| `host-websocket` | future `providers::host_websocket::HostWebsocketProvider` | already-running server over host networking | remote endpoint | host-side discovery/connect/status; limited management | future |
| `browser-websocket` | future `providers::browser_websocket::BrowserWebsocketProvider` | already-running server over browser networking | remote endpoint | browser permission/discovery/connect/status; limited management | future |
| `host-webserver` | future `providers::host_webserver::HostWebserverProvider` | host service owning `fw-host` runtimes | service-managed runtime endpoint | create/stop runtimes, logs, diagnostics | future |

The ESP32 serial providers are intentionally ESP32-specific. Flashing,
resetting, boot-mode handling, and raw filesystem access are target-family
details; a generic serial abstraction can come later if another target earns it.

Provider support is feature-gated:

```bash
cargo check -p lpa-link
cargo test -p lpa-link
cargo check -p lpa-link --features host-process
cargo test -p lpa-link --features host-process
cargo check -p lpa-link --features host-serial-esp32
cargo test -p lpa-link --features host-serial-esp32
cargo check -p lpa-link --features browser-serial-esp32 --target wasm32-unknown-unknown
cargo check -p lpa-link --features browser-worker --target wasm32-unknown-unknown
cargo test  -p lpa-link --features fake-device
```

## Testing with the fake device

The `fake-device` feature (host only) adds a byte-level scriptable device,
`providers::fake_device::FakeEsp32Device`, and upgrades `FakeProvider` to
expose it through the REAL provider path. The point is byte-level fidelity:
every hardware bug so far (pull-before-readiness ordering, fresh-device
misclassification) lived below the record level — in framing, boot-output
classification, and timing — so the fake injects at the byte stream and lets
the real `M!` parser, the real readiness classifier, and the real
orchestration run in tests.

**Boot-state script** (`FakeDeviceScript` / `FakeBootState`): the device is a
sequence of states; reset-signal dances re-run the current state's boot.

- `BlankFlash` — repeats the ROM's `invalid header: 0xffffffff` line.
- `RomDownloadMode` — prints `waiting for download` once.
- `ForeignFirmware` — prints a known replaceable firmware boot string.
- `LightPlayer { boot_delay, project_files, identity }` — scripted boot log
  lines, the real M2-shaped `[INIT] fw-esp32 initialized, starting server
  loop... proto=… commit=… dirty=…` line, then a REAL host `LpServer` over a
  seeded `LpFsMemory` (reusing `fw-host`'s machinery) speaking real `M!`
  frames including the unsolicited wire hello (uid from `identity`). Bytes
  written before the server loop runs are DISCARDED and counted
  (`premature_input_bytes()`), like real hardware.

**Reset sequences**: the hardware transport's DTR/RTS hard-reset dance
replays the current state's boot; the usb-jtag-download dance (the only one
that raises DTR) transitions to `RomDownloadMode`.

**Failure injection** (`FakeFailurePlan`, composable knobs on the stream,
not the script): per-direction latency, stall-after-N-bytes (no EOF),
disconnect (EOF), garble/drop a byte, mid-frame cut (truncate a frame then
stall), and log-flood interleaving between frames.

**Scripted management**: the fake connector's `manage()` implements
`FlashFirmware` / `EraseDeviceFlash` / `ResetRuntime` as scripted state
transitions (`fake_flash` → fresh `LightPlayer` with the image identity in
its provenance; `fake_erase` → `BlankFlash`) with scripted latency and an
optional one-shot scripted failure, emitting `LinkManagementEvent`
logs/progress through the standard result replay.

Typical wiring (see `lpa-studio-core`'s `studio_link_e2e_tests`):

```rust
let provider = FakeProvider::new().with_device_endpoint(
    "fake-device-0",
    "Fake ESP32",
    FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new())),
);
let device = provider.device(&LinkEndpointId::new("fake-device-0")).unwrap();
// registry.insert(provider); connect through the normal provider path;
// device.set_failure_plan(...) / device.premature_input_bytes() from tests.
```

## Design Notes

- **Provider:** source of endpoints and management behavior, such as
  `host-process`, `browser-worker`, or ESP32 serial providers.
- **Endpoint:** discoverable candidate target. It has identity, status, and
  `LinkCapabilities`, but no live resource ownership.
- **Session:** provider-neutral snapshot/handle for a connected endpoint or
  launched runtime. Provider-private session state owns concrete resources.
- **Connection:** server protocol handoff to `lp-server`, consumed by
  `lpa-client`.
- **Capabilities:** low-level operations below Studio product actions: reset,
  flash, raw filesystem image access, logs, diagnostics, and similar
  device/runtime controls.
- Public domain types use `Link*` names where they cross crate boundaries:
  `LinkProvider`, `LinkEndpoint`, `LinkSession`, `LinkConnection`, and related
  IDs/status types.
- Provider modules and methods use natural names such as `host_process`,
  `browser_worker`, `discover`, `status`, `connect`, and `logs`.
- Public provider IDs use kebab-case and generally follow
  `{environment}-{mechanism}-{target?}`, such as `host-process`,
  `browser-worker`, `host-serial-esp32`, `browser-serial-esp32`,
  `host-websocket`, and `browser-websocket`. The target segment is optional when
  the mechanism already carries the whole contract. Include it when management
  details are target-specific. Rust modules/types use Rust naming, such as
  `providers::host_serial_esp32::HostSerialEsp32Provider`.
- The model is plural-first. Multiple host or browser runtime instances should
  be natural, even if the first Studio UI exposes only one session.
- `host-process` endpoints are spawnable. Calling `connect()` creates a new
  in-process `fw-host` runtime instance and records provider-private session
  state that owns its lifecycle.
- A `LinkConnection` is a server/client connection, not a project session.
  Project sessions belong above this layer.
- `browser-worker` owns the worker wrapper source under
  `src/providers/browser_worker`. Apps pass same-origin
  `fw_browser_module_path` and `fw_browser_wasm_path` options for the generated
  `fw-browser` sidecar artifacts.
- `browser-serial-esp32` owns Web Serial access and ESP32 probe/flash/erase
  bindings under `src/providers/browser_serial_esp32`. In the browser, its
  wasm-bound session adapter delegates Web Serial lifecycle to the app-served
  `BrowserEsp32DeviceController` at
  `/lpa-link/browser_esp32_device_controller.js`. The controller owns the
  selected `SerialPort`, reader/writer locks, raw serial log pump, best-effort
  reset signaling, and the handoff between normal protocol reading and
  `esptool-js` bootloader operations. Flash and erase stream esptool
  terminal/progress events through `LinkManagementEventSink`. Apps pass
  same-origin `firmware_manifest_path` and optional `esptool_module_path`
  options for app-owned assets. The default esptool module is pinned to the
  browser ESM endpoint `https://cdn.jsdelivr.net/npm/esptool-js@0.6.0/+esm` for
  development. The jsDelivr ESM rewrite is important because the raw package
  imports dependencies such as `pako` by bare specifier, which browsers cannot
  resolve without a bundler or import map, and it exposes ESP32-C6 flasher stub
  JSON with the named exports expected by `esptool-js`. A deployed app can
  override the default with a hosted module path. The provider releases normal
  protocol ownership before probe/flash/erase takes exclusive bootloader access.
  Opening the normal serial server protocol opens the port once, starts reading
  immediately, then attempts a best-effort hard reset while boot output is being
  captured. Reset signal failures are diagnostic; readiness is classified from
  serial output and protocol frames.
- Direct filesystem access means raw/full filesystem image management below the
  running `lp-server`. Normal project upload should use `lpa-client` and the
  server filesystem/project protocol once firmware is running.
