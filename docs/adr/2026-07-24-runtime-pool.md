# ADR: Runtime pool — plural sessions, per-session wire clients, editor-as-lens

- **Status:** Accepted
- **Date:** 2026-07-24
- **Deciders:** Photomancer
- **Supersedes:** None (builds on `2026-07-15-device-session-model.md`
  and `2026-07-17-rich-object-pattern.md`)
- **Superseded by:** None

## Context

Before this decision, "the runtime Studio is attached to" was a single
slot smeared across two owners: `ServerController.client:
Option<StudioServerClient>` (the one wire client, overwritten by every
`install_client`) and `DeviceController.attachment` (the one
`RuntimeAttachment { None, Sim, Device }`). The consequences were
structural, not incidental:

- Opening a project on the simulator EVICTED a connected device (and
  vice versa) — `open_from_home` refused to open while hardware was
  attached, because the sim's client would have overwritten the
  device's.
- Returning to the gallery killed the simulator: the web shell's
  route→Home policy dispatched a full `DisconnectDevice`
  (`worker.terminate()`), because closing the editor and destroying the
  runtime were the same operation.
- Per-device reconcile state (`device_sync` / `device_versions` /
  `device_storage_id`) lived single-valued on `StudioController`;
  refresh cadence, backoff, and the tick policy were one shared
  singleton derived from the one connect-flow state.

The device-UX direction (D35–D38) demands the opposite model: runtimes
are first-class and PLURAL — a live device card while a project runs on
the sim is the MVP forcing case — and the editor is a *lens*, a UI-only
binding onto one of them. Discovery found the change cheaper than
feared: `ProjectController` was already client-agnostic (all 28 network
methods take `server: &mut StudioServerClient` per call),
`StudioServerClient` is fully self-contained per session, and
`BrowserWorkerProvider` already keys N worker sessions. The singleness
lived in exactly the two slots above, plus 21 `client_mut()` call sites
in `studio_controller.rs`.

`lpa_link::DeviceSession` (the 2026-07-15 ADR) already owns each
hardware link end to end; this decision is the layer above it: who holds
sessions, how many, and what the editor binds to.

## Decision

### RuntimeSession + RuntimePool; absence means not attached

One concept module, `lpa-studio-core/src/app/runtime_pool/`:

- **`RuntimeSession`** bundles what the two slots smeared: a
  `RuntimePayload` (`Sim(SimAttachment)` — connector + worker io — or
  `Device(DeviceHandle)` — always a live `DeviceSession` in product
  code; D22 stays a type-system rule), the session's OWN
  `StudioServerClient` + `ServerState` + requested log level, the
  per-device reconcile bundle, and per-session tick state
  (cadence-by-kind, `BackoffPolicy`, heartbeat bookkeeping). There is
  no `None` arm: absence of a runtime is absence from the pool, and
  "connected" MEANS "a session exists in the pool".
- **`RuntimePool`** is the keyed collection (`BTreeMap<RuntimeId,
  RuntimeSession>`) plus `lens: Option<RuntimeId>`, owned by
  `StudioController`. `RuntimeId` is minted per session and is the
  stable pool key from before identity is known; a device session's
  `dev_` uid association derives from the wire hello when it lands.
- `ServerController` is DELETED (dissolved into the session).
  `DeviceController` keeps the connect flow + provider catalog and
  becomes the **session factory**: connect flows build and return a
  `RuntimePayload`; `StudioController` installs it. The controller
  itself is slotless.

### Per-session wire clients; two named resolution seams

Each session owns its client; nothing installs over anything. The 21
call sites resolve through exactly two seams, and the distinction is
part of the vocabulary:

- **Lens-bound** (14 editor-mirror ops): `pool.lens_session_mut()` —
  the session the editor is a lens on.
- **Session-targeted** (7 device/deploy/reconcile ops):
  `pool.device_session_mut()` — the ≤1 DEVICE session, regardless of
  where the lens is. Device flows never land on the sim.

Each session reconciles on its own client; connect-as-pull state moved
into the session bundle and dies with it.

### Capacity is a policy, never a shape

`SIM_SESSION_CAPACITY = 1`, `DEVICE_SESSION_CAPACITY = 1` — numbers in
one place. `install` evicts only SAME-kind sessions beyond the kind's
capacity (oldest first), and refuses the replace while an operation is
in flight on the session it would evict (the payload is handed back,
never leaked). "+ new simulator", N sims, and the radio-sim bus raise a
number, not the shape.

### Install preserves the lens

Attaching a runtime OBSERVES: `install` claims the lens only when
nothing holds it (empty pool, detached editor, or a same-kind replace
that evicted the lens session — the replacement inherits it). Plugging
in a board while editing on the sim never steals the editor. Flows that
deliberately move the editor (project open, the D29 click) call
`set_lens` explicitly.

### Detach keeps the session — worker AND wire client

Closing the editor is `ProjectOp::DetachLens`: the mirror drops, the
lens id releases, and EVERY session stays — sim worker self-ticking,
wire clients attached, device reconcile state intact. The web shell's
route→Home policy dispatches this instead of the retired
`DisconnectDevice` teardown; explicit disconnect affordances keep their
full teardown meaning. The sim card (D36) and the project card's
"Running in simulator" chip render from the live pool while the gallery
is up. Destruction is explicit: **stop-sim** lives in the sim card's
danger zone (rich-object ADR) and closes the provider session
(`worker.terminate()`); a page reload still kills workers — D37
re-derives from the URL.

### Lens moves quiesce, then rebuild; nothing acked is lost

Edit state (mirror, overlay buffer, dirty tracking) belongs to the
LENS, not the session. Moving the lens (gallery return, project open,
D29 click, stop-sim of the lens session) runs quiesce-then-rebuild:

- **Quiesce** is the actor's serialized dispatch — every edit action is
  fully awaited (its ack landed) before the next queued command runs,
  and the lens op's Foreground class cancels an in-flight passive pull
  at a frame boundary first. Departing wire logs drain into the ring;
  `project.reset()` drops the mirror; the lens id releases.
- **Rebuild** on attach runs the existing connect sequence against the
  target session's client. Acked overlay state is server-side, so the
  rebuilt mirror recovers it; dirty state is overlay-derived and
  cross-client-correct by construction (editing-model ADR).

This makes rebuild-on-switch the cost model — deliberately, instead of
N live mirrors (see Alternatives).

### Early D29: click a device, edit live

Clicking a live device card dispatches `ProjectOp::OpenDeviceProject`:
move the lens onto the DEVICE session and open its running project
against the device's own client (the device reports its loaded handle).
`ProjectOp::OpenSimProject` is the sim card's mirror arm. This is the
D29 grammar without the M5 routes — pulled forward so the
one-consolidated-gate hardware walk can judge serial editor latency
live. Clicking a project still always opens on the sim, never a
takeover of a running installation.

### Per-session tick policy

Cadence is data per session kind (sim fast, device calm); backoff is
per session; only the LENS session runs the fallible passive project
pull. Non-lens sessions get a slow status heartbeat
(`DEVICE_HEARTBEAT_INTERVAL`, 2 s) that issues NO wire operation — it
drains session-buffered logs and surfaces device-state changes through
the change gate — so each session sees at most one wire op per actor
batch. The published UI delay is the minimum over sessions.

### SDI: one lens shown; the URL is the focused document

The model is multi-document; the INTERFACE is single-document. One lens
at a time, no in-editor runtime switcher (D38), and the MDI affordance
is spatial: the runtime roster renders at the TOP of Home, so the
gallery reads window-switcher-first, library-second. The URL is the
focused document — this binds D37 for M5: `#/sim/<project-key>` and
`#/device/<dev-uid>` become the reload-derivable lens addresses; the
pool APIs those routes bind to (`attach_lens`, session lookup by kind /
uid) exist now. Desktop multi-window is the true-MDI future.

### `places::RuntimePlace` is vestigial — docs aligned, no migration

The `places/` capacity-1 seam (`PlaceDescriptor { capacity: Some(1) }`
on `RuntimePlace`) predates the pool and has ZERO production callers —
the real constraint was always the structural single slot, and is now
the pool's capacity policy. Resolution: the `places/` docs now say so
(capacity there describes a place's storage-slot shape, not how many
runtimes may attach — that number lives in `runtime_pool`), and the
seam stays as-is until a real caller shapes it. No migration, because
there is nothing to migrate.

### PreviewHost relationship: shared present seam, disjoint machinery

`PreviewHost` (2026-07-16 ADR) and the pool share the same worker JS,
wasm module, and per-runtime present seam (`PreviewFrame` /
`PresentFrame` / `attach_preview_surface`, routed by `runtime_id`) —
that seam works on ANY runtime including a sim session's boot runtime.
What does NOT transfer is the machinery: pool isolation, leases, LRU
eviction, and GPU zero-readback belong to previews; the boot runtime is
CPU-tier, self-ticking, and undroppable. They stay separate services
with a ROUTING rule at the gallery: a project card whose project runs
in the sim session should show the sim's live frames rather than lease
a preview slot (one live source of truth per project).

**Stretch outcome — live sim-card frames DEFERRED.** The planned
factoring (share the CPU blit + frame-request helpers, drive the boot
runtime at card fps with `delta_ms: None` pokes) hit the plan's
stop condition: the sim worker handle is not cleanly reachable from the
web layer. The controller — and with it the pool, the sim connector,
and the wire session — moves into the `StudioActor` at spawn;
`StudioHandle` (commands in, change-gated views out) is deliberately
the ENTIRE UI boundary. Every bridge is forced: pumping card-fps frame
pokes through the actor's ordered command queue entangles frame pacing
with protocol ops (a flash or slow pull stalls frames — the exact
problem PreviewHost's dedicated scheduler exists to avoid), and leaking
the connector to the web layer creates a second drainer on a
single-consumer worker io (racing the client-io receive loop for
protocol frames). It also needs a wire-envelope change (`PreviewFrame`
cannot address the boot runtime; its id is worker-private). The clean
shape is a core-owned present service for pool sessions — a sibling of
PreviewHost's scheduler sharing the blit seam — which is the roadmap's
live-thumbnails future item, not a P5 stretch. The MVP sim card is
glyph + status + project chip, per the plan's Q6 fallback.

## Consequences

- `ServerController` and `RuntimeAttachment` are gone;
  `server_controller.rs` and `runtime_attachment.rs` deleted. The
  `MissingSession` error surface of the retired `client_mut` is
  preserved verbatim, so call-site error behavior is unchanged.
- Sim and device sessions coexist; the `open_from_home` hardware
  refusal and the sim-only branches in attach are deleted. A connected
  device stays attached and reconciled while a project opens on the
  sim.
- Gallery return no longer destroys anything; the sim keeps running
  detached. The e2e harness (`studio_link_e2e_tests.rs` /
  `studio_edit_e2e_tests.rs`) grew rows for coexistence, detach
  quiesce (an edit and the detach queued in the same batch lose
  nothing), keep-running, stop-sim, and pool-fed card states.
- The roster is pool-fed (`HomePoolEvidence`): one evidence bundle per
  device session through the unchanged M2 derivation, plus the sim
  session's card (rich-object schema, danger-zone stop) — and it
  renders at the top of Home (SDI).
- Reload drops the pool (workers die with the page); D37's URL
  re-derivation is M5's contract, bound here.
- The sim-worker recovery requirements deferred from the sim-fuel plan
  (timeout-streak detection → terminate+respawn preserving the
  unsaved-overlay mirror; `NotResponding` sim card) did NOT land in the
  pool milestone; they re-bind to the next sim-runtime lifecycle work
  (see Follow-ups).

## Alternatives Considered

- **Per-session mirrors (N live edit states)**: rejected — edit state
  with the lens keeps one mirror's memory and one overlay protocol
  conversation; acked overlay state is server-side, so
  quiesce-then-rebuild loses nothing user-visible. Rebuild cost is the
  accepted price of switching.
- **A `None`/attachment-style pool slot** (keep `RuntimeAttachment`
  and grow arms): rejected — absence-from-collection is the truthful
  model, removes every `is_attached` flag read, and makes capacity a
  count instead of a shape.
- **Capacity as type shape** (one sim field + one device field):
  rejected — the roadmap's N-sim/radio-bus future would re-litigate
  the structure; a keyed collection under a numeric policy makes that
  future a constant change.
- **Install steals the lens** (newest session gets the editor):
  rejected — plugging in a board while editing would hijack the
  editor; attach is observation (the hardware-attach defect of
  2026-07-17 already taught this once).
- **Gallery return keeps full teardown** (status quo): rejected — it
  makes D36 impossible and conflates two intents; detach-lens vs
  explicit disconnect now carry the two meanings.
- **MDI chrome (in-editor runtime switcher/tabs)**: rejected for MVP
  (D38; SDI record 2026-07-20) — one lens shown at a time, the roster
  at the top of Home is the switcher, the URL is the focused document.
  A where-you-are jump link is the cheap later escalation.
- **Routing sim-card live frames through the actor command queue**
  (shipping the stretch anyway): rejected — see the stretch outcome;
  frame pacing does not belong on the protocol op queue, and a second
  worker-output drainer races the wire. An honest defer beat a forced
  bridge.
- **Generalizing `RuntimePlace` to the pool**: rejected — zero
  production callers means there is no abstraction to rescue; docs
  aligned instead.

## Follow-ups

- **M5 (D29/D30/D37)**: `#/sim/<key>` + `#/device/<uid>` routes over
  `attach_lens`; reload re-derivation; the diverged-resolution popup on
  the D29 click path.
- **Live sim-card frames**: the deferred stretch — a core-owned present
  service for pool sessions sharing PreviewHost's CPU blit seam, plus
  the gallery routing rule (sim frames instead of a preview lease for
  the running project's card). Pairs with the roadmap's live-thumbnails
  item.
- **Sim-worker recovery layer 2** (from `2026-07-23-sim-wasm-fuel`):
  timeout-streak hang detection, terminate+respawn preserving the
  unsaved-overlay mirror, `NotResponding` sim card, PreviewHost
  in-flight deadline — requirements recorded in the M4 plan notes;
  re-bound to the next sim-runtime lifecycle work.
- **Runtime pane (M7)**: render the lens session's rich object in a
  `RichObjectPane` (rich-object ADR follow-up), now with the pool as
  the evidence source.
- **N > 1 sims / radio-sim bus / networked connectors**: raise the
  capacity numbers; the pool shape is ready.
