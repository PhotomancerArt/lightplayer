# Probe performance investigation — notes

Created 2026-07-26. Status: complete — discovery done 2026-07-26, P1–P6
implemented 2026-07-27 (see [handoff.md](handoff.md) and `_DONE.md`).

## Initial understanding (from Yona)

Three related performance issues in Studio, now that the feature side is in good shape:

1. **ESP32 probe jerkiness.** Shader probes are relatively large and fixed-size
   (believed 32x32) with no way to change that. Fine in some cases, but on real
   ESP32 hardware the UI is pretty jerky. Worth investigating probe sizing
   strategy (how should probes be sized?) and possibly increasing serial baud
   rate. Not sure how best to do either.
2. **Sim UI chunkiness.** On sim, UI updates arrive chunkily and the UI thread
   becomes unresponsive. Yona's hypothesis: an issue in messaging code or a
   lock, not an underlying compute/perf problem (but could be wrong). Yona is
   happy to collect data.
3. **Active-node-only probing.** Currently only the active node is probed.
   Smart for ESP32 bandwidth, but makes no sense for sim. If #2 is fixed, sim
   should probe all (visible?) nodes.

## Discovery targets

- Probe pipeline: `lp-core/lpc-engine/src/engine/project_read_probes.rs`,
  `project_read_stream.rs`, `lp-core/lpc-view/src/project/project_read_applier.rs`.
  (Note: `lp-shader/lps-probe` is the shader-agent oracle probe system — a
  different "probe"; out of scope here.)
- ESP32 serial transport & baud: `lp-app/lpa-link`, firmware serial config.
- Sim/browser messaging: fw-browser worker <-> Studio client messaging, UI
  update application in Dioxus.

## Discovery findings

### Transport foundation (ADRs)

- Pull model: Studio sends `ProjectReadRequest { since }`; revision-gated reads
  (ADR 2026-07-03) mean steady-state reads carry only the Begin/Runtime/End
  spine. **Probes always execute regardless of `since`** — they are the bulk of
  steady-state traffic.
- Envelope streaming (ADR 2026-07-04): probe payloads chunk as
  `ResultBegin { byte_length, header }` → N × `ResultBytes` → `ResultEnd`,
  under `PROJECT_READ_FRAME_MAX_BYTES = 16 KiB` (lpc-wire `budget.rs:31`),
  runtime chunks 4 KiB raw (`budget.rs:69`).

### ESP32 serial transport (agent report, 2026-07-26)

**Baud rate is a no-op.** Board is ESP32-C6 using the on-chip USB-Serial-JTAG
peripheral (native USB CDC): firmware never configures a UART baud
(`lp-fw/fw-esp32/src/serial/io_task.rs:159-161` — `UsbSerialJtag::new`), and
there is no USB-UART bridge chip. Web Serial's `baudRate`
(`DEFAULT_SERIAL_BAUD_RATE = 921600`, `lp-core/lpc-model/src/config.rs:4`,
threaded to `port.open({ baudRate })` in
`browser_esp32_device_controller.js:301`) is descriptive only — the CDC
endpoint ignores line coding. Raising it changes nothing. Real ceiling = USB
full-speed framing (~1 ms SOF) + software cadences below.

Actual throughput limiters, ranked by likely impact on probe smoothness:

1. **`DEVICE_REFRESH_INTERVAL = 750 ms`**
   (`lp-app/lpa-studio-core/src/app/studio/refresh_cadence.rs:31`; sim = 33 ms
   at `:27`) — device probes update ~1.3×/s *by construction*. Likely the
   dominant cause of perceived jerkiness.
2. **`READINESS_POLL_INTERVAL = 10 ms`** reused as the steady-state browser
   receive poll (`lp-app/lpa-link/src/device_session/device_session.rs:496-509`
   + `device_timers.rs:38`) — up to 10 ms latency *per received frame*; a
   multi-frame probe stream is paced at ~10 ms/frame, ~100 frames/s cap.
3. **Stop-and-wait server sends** — firmware `ServerTransport::send()` awaits
   the write result per frame (`lp-fw/fw-esp32/src/transport.rs:47-77`), depth-1
   request/result channels (`io_task.rs:41-52`), one server frame drained per
   1 ms io-loop tick (`io_task.rs:251-253`, tick at `:194`).
4. **64-byte inbound read buffer per 1 ms tick** (`io_task.rs:233`) — caps
   host→device at ~30-60 KB/s (matters for uploads, less for probe downlink).
5. **base64-in-JSON, no compression** (`lpc-wire/src/budget.rs:54-74`) — 4/3
   inflation on probe pixel bytes; framing is `\nM!<json>\n` text lines shared
   with log output (`io_task.rs:315-348`).

Other constants: `WRITE_CHUNK_SIZE = 256 B` + `WRITE_TIMEOUT = 250 ms`/chunk +
liveness tick per chunk (`io_task.rs:59,70,80-98`); ~16.7 KiB stack JSON buffer
paid per send incl. tiny acks (TODO at `io_task.rs:297-309`); host-not-draining
latch (`usb_connection.rs:22-27`); no RTS/CTS (DTR/RTS only for reset).

### Probe pipeline (agent report, 2026-07-26)

**Probes are stateless riders on every `ProjectReadRequest`** — rebuilt from
scratch each passive refresh in
`lpa-studio-core/src/app/project/project_sync.rs:381-429`
(`probe_requests` / `product_probe_requests`; one `RenderProductProbeRequest`
per visual product, one `ControlProductProbeRequest` per control product,
optional `BindingGraph` probe whenever the bus pane is open).

**Probe size — confirmed fixed 32×32, chosen by the client:**
- `UiProductPreviewFrame::VISUAL_DEFAULT = Self::new(32, 32)`
  (`lpa-studio-core/src/app/node/ui_produced_product.rs:48-50`), used at
  `project_sync.rs:405-407` with `WireTextureFormat::Srgb8`. Compile-time
  const; no config/env/per-node override anywhere.
- The wire request carries `width`/`height`
  (`lpc-wire/.../probe/render_product_probe.rs:11-16`), and the engine renders
  **natively at the requested resolution** (`lpc-engine/src/engine/
  project_read_probes.rs:32-40`) — no render-large-then-downsample. So probe
  size is already a per-request knob; only the client constant pins it.
- On-device cost per visual probe frame: render + `rgba16_linear_to_srgb8`
  gamma encode = 3072 `libm::powf` calls (`project_read_probes.rs:54-57,
  211-229`) + ~16.7 KiB stack JSON serialize buffer per send.
- Bytes: 32×32×3 = 3072 raw → 4096 base64 chars; sits just under
  `PROJECT_READ_RUNTIME_CHUNK_BYTES = 4096` so travels unchunked as one
  `ProjectReadProbeEvent::Result`; ~4.2 KB JSON per visual probe.
- Control probes scale with fixture extent (`sample_count × 2` bytes u16).

**Active-node-only logic — single policy point:**
- `project_controller.rs:1783-1789` `node_subscribes_products`:
  `Default => is_focused_node`, plus `ProjectProductSubscriptionIntent::
  Subscribed/Unsubscribed` overrides that **exist but are never set in
  production** (`node_controller.rs:17-27`; doc says transport intent reserved
  for future). `subscribed_products()` (`:1791-1816`) walks the tree and always
  unions in the primary visual product (ADR 2026-07-16-primary-visual-product).
  At most one node is focused (`focus_editor_target`, `:1818-1829`).
- Multi-node seams: (a) make the `Default` arm policy-aware per runtime kind
  (Sim vs Device — `RefreshCadence::for_kind` precedent at
  `refresh_cadence.rs:93-98`); (b) wire the existing intent enum to a
  `ProjectOp`; (c) filter by visibility (`NodeControllerState.collapsed`
  exists as a proxy).

**Scheduling:** timer-driven pull. Chain: `web_app.rs:428-438` use_future loop
→ `StudioCommand::RefreshTick` → `studio_actor.rs:293-307` (coalesced,
cancel-preemptable) → `project_controller.rs:1872-1881` `run_refresh` →
`server.project_read(...)`. Sim 33 ms / device 750 ms / verdict-chase 250 ms×3
(`refresh_cadence.rs`).

**Sim-side red flags found by this sweep:**
1. `lpa-studio-core/src/app/server/browser_worker_client_io.rs:20,55-85` —
   `receive()` does `for _ in 0..240 { sleep_ms(4).await; poll }` —
   unconditional ≥4 ms `setTimeout` **before** each poll, per frame received;
   comment says "Event-driven receive is future work (M7)". Multi-frame reads
   pay ≥4 ms/frame; setTimeout(4) is subject to browser clamping/throttling.
2. `lpa-studio-web/src/app/node/produced_product_view.rs:203-225,364-374` —
   `ProductPixelGrid` renders a 32×32 preview as **1024 keyed `<span>`s with
   1024 `format!`-ed inline-style Strings**, per preview, per view snapshot.
3. On sim the engine bumps revision every tick (`engine.rs:390`), so
   `view_if_changed` (`studio_controller.rs:818-838`) **never gates**: a full
   `UiStudioView` is rebuilt and Dioxus-diffed at 30 Hz regardless of change.
   (Contrast: gallery/preview-lab uses a separate transferable-ArrayBuffer →
   canvas path — `fw_browser_worker.js:150-192` `previewFrame` — proof a
   zero-copy path exists and probes don't use it.)
4. `ProjectReadStreamSink.flush` clones all pending events incl. probe bytes
   per frame (`lpc-shared/src/transport/server.rs:252-276`).

**Existing knob inventory** (none env/config-backed): probe res + format
(consts), which-nodes policy (`:1785`), unwired per-node intent, binding-graph
probe rides every read when bus pane open (`include_values: true`), cadences,
16 KiB frame budget / 4 KiB chunk (`budget.rs:31,69`), sim worker self-tick
33 ms (`fw_browser_worker.js:14`), sim receive poll 240×4 ms, preview-lab
config matrix (`preview_lab_config.rs:118-121`) already has a tunable-size UI
precedent (sizes 64/96/128).

Note: the probe agent's "~45 ms serial per probe at 921600 baud" estimate is
superseded by the transport finding that CDC ignores baud — the wire is faster
than that; software pacing dominates.

### Sim messaging path (agent report, 2026-07-26)

Full hop-by-hop trace confirmed Yona's "messaging or lock" hypothesis — no
blocking waits (no Atomics.wait/sync XHR/busy loops), but three structural
causes of chunkiness, ranked:

1. **`ProductPixelGrid` DOM storm (largest single cost).**
   `produced_product_view.rs:205-225,364-374` — each 32×32 preview = 1024
   keyed `<span>`s with `format!`-ed inline styles, re-diffed/rewritten up to
   30×/s per visible product (primary visual always subscribed). Fix direction:
   `<canvas>` + `putImageData` (or ImageBitmap), like the preview-lab path.
2. **Two free-running 33 ms clocks beating, sampled on a 4 ms setTimeout
   grid, no completion-based pacing.** Worker self-tick
   (`fw_browser_worker.js:14,285`) vs UI RefreshTick (`web_app.rs:429-438`);
   `ProtocolIn` is only queued and answered on the worker's *next* tick
   (`fw-browser/src/runtime.rs:213-219`), so each round trip = 0-33 ms worker
   latency + ≥4 ms/frame poll latency (`browser_worker_client_io.rs:63-64`,
   sleep *before* drain). Tick timer never awaits the previous pull → pulls
   run back-to-back with zero idle when a pull exceeds 33 ms. Fix direction:
   event-driven receive (already flagged "M7 future work" in code) +
   completion-based re-arm of the refresh timer.
3. **Per-tick trace log forces full view rebuild + console writes at 30 Hz.**
   `runtime.rs:257-263` pushes an unconditional trace Log envelope every worker
   tick → `record_session_logs` → `mark_dirty` (`studio_controller.rs:865-885`)
   → `view_if_changed` rebuilds the entire `UiStudioView` (node tree, bus
   view, console view cloning up to 1000 ring entries) → cloned again at
   `web_app.rs:456` → deep `PartialEq`. Plus `console.debug` per line on main
   thread (`web_app.rs:623-631`) and worker-side mirror (`logger.rs:49-59`).
   Fix direction: level-gate the tick trace; don't mark the view dirty for
   log-only deltas (or split console out of the monolithic view).

Supporting detail: 3 encodes + 3 decodes per protocol frame (serde_json in
worker ×2, JSON.parse in worker JS, structured clone, serde_wasm_bindgen on
main, serde_json inner-frame decode on main); `postMany` = one postMessage per
envelope, no coalescing/transferables on the protocol path (binary transferable
path exists but only for PreviewHost); whole drained batch decoded
synchronously in one tick (`pending_server_messages.rs:87-108`); every
`UxUpdate::Activity/Log` clones the whole `UiStudioView`
(`studio_actor.rs:266-274`). PreviewHost path is the well-behaved precedent:
fps-scheduled, in-flight backpressure, transferable pixels, throttle-immune
worker sleeper (`frame_schedule.rs:66-92`, `preview_sleep.rs`).

## Synthesis

- **#1 device jerkiness:** *not* bandwidth-bound at the wire level; baud is a
  no-op (USB-CDC). Dominant: 750 ms fixed refresh cadence. Secondary: 10 ms/
  frame receive poll, stop-and-wait sends, on-device sRGB powf ×3072/frame,
  base64+JSON. Levers: faster/completion-based device cadence, smaller probe
  size on device, sRGB LUT instead of powf, (later) gate unchanged probe bytes.
- **#2 sim chunkiness:** three structural causes above; none are engine
  compute. Fix = canvas previews + event-driven receive/completion pacing +
  trace-log gating.
- **#3 multi-node probing:** one-line policy at `project_controller.rs:1785`;
  `ProjectProductSubscriptionIntent` enum already exists unwired; sensible sim
  policy = probe all non-collapsed nodes' products, keep focused-only on
  device.

## Questions

Drafted for Yona (see chat 2026-07-26): dead-end confirmation on baud; sim-first
ordering; event-driven receive now vs later; device pacing model; probe sizing
strategy (discussion); multi-node scope on sim; measurement-first vs fix-first.

## User answers / scope changes

2026-07-26, Yona: **all yes** to Q1–Q7:

- Q1: baud-rate work dropped (dead end, USB-CDC ignores it).
- Q2: sim-first ordering (#2 → #3), device (#1) last.
- Q3: event-driven receive now (do the flagged "M7" work).
- Q4: completion-based pacing replaces fixed cadence. Yona: "completion-based
  pacing was how I did it when I made something similar, seems right to me."
- Q5: fix-first; perf marks/instrumentation as validation, Yona collects
  before/after data.
- Q6: sim multi-node policy = all non-collapsed nodes' products; device keeps
  focused-only + primary visual.
- Q7: probe sizing = runtime-tiered now (16×16 device / 32×32 sim), record
  display-driven-capped-by-tier as follow-on once multi-node probing exists.
  sRGB powf → 256-entry LUT on device in scope.

## Future work

Deliberately deferred — no wire/protocol changes were made anywhere on this
plan. Indexed (where durable) in `docs/adr/README.md`'s open-follow-ups table
under `2026-07-27-completion-based-refresh-pacing`:

- **Probe revision-gating**: skip unchanged probe bytes on the wire; the
  display-layout `IfChanged` read is the precedent.
- **Binary/transferable protocol frames on the sim path**: PreviewHost already
  has a zero-copy transferable path the probe path does not use.
- **Firmware transport**: stop-and-wait server sends, the 64-byte inbound read
  buffer per 1 ms tick, the ~16.7 KiB stack JSON buffer per send (incl. tiny
  acks).
- **Display-driven per-surface probe sizing**, capped by the runtime tier
  (Yona's Q7 answer: tier now, display-driven later once multi-node probing
  has soaked).
- **Wiring `ProjectProductSubscriptionIntent` to a user-facing toggle**: the
  per-node override enum has existed unwired since M2a and remains the durable
  seam.
- **Sim "non-collapsed" scope becoming real**: collapse state is view-local
  today so sim probes all nodes; the ui-state-audit plan moving live collapse
  state into core makes the refinement effective.
