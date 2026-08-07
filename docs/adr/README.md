# Architecture Decision Records

Architecture Decision Records, or ADRs, capture durable architecture and process
decisions for this repo.

Use ADRs for decisions that choose a direction among plausible alternatives and
have lasting architectural, operational, security, data-model, API, workflow,
product, embedded, or cross-repo/process consequences.

Do not create ADRs for ordinary feature work, bug fixes, UI copy/layout
changes, mechanical refactors, tests, scripts, helpers, or phase sequencing
unless they set a broader precedent.

## Filename

Use date-based filenames:

```text
YYYY-MM-DD-short-title.md
```

Date-based names keep files sortable and reduce conflicts between parallel
branches.

## Status

Use one of:

- `Proposed`
- `Accepted`
- `Superseded`
- `Rejected`
- `Deferred` — a design-heavy decision consciously postponed; pair it with a
  "Revisit when" trigger and list it in the Deferred Decisions index below.

Treat ADRs as durable history. If a decision changes, create a new ADR that
supersedes the old one instead of rewriting old context heavily.

## Deferred Decisions

Small deferrals live in the creating ADR's **Follow-ups** section; design-heavy
deferrals get their own ADR with `Status: Deferred` and a "Revisit when" line;
and this index is the one place that tracks every open cross-ADR follow-up so a
deferral is never silently lost — new ADRs add their open items here, and a
follow-up is struck from the table once it lands (or is checked off in its
source ADR).

The table below is the open-follow-ups/deferred index built in M7/P4 by scanning
every ADR's Follow-ups section. It lists the still-open items that carry a
recognizable revisit trigger; each row points back to its source ADR, which
holds the full context.

| Item | Source ADR | Revisit trigger |
|---|---|---|
| Structured `ServerMsgBody::Log` frames from firmware (receive path live and mapped; nothing sends it — device logs are prefix-parsed serial text) | `2026-07-05-studio-logging-model` | Serial-text parsing breaks down or per-record metadata is needed |
| Rotatable share tokens (today the project uid IS the permanent link; "unshare" = flip visibility) | `2026-08-06-cloud-service-architecture` | A real need to revoke a leaked URL without changing project visibility |
| Postgres metastore adapter behind the same port (single-machine SQLite is deliberate) | `2026-08-06-cloud-service-architecture` | A genuine multi-instance need — scale or zero-downtime deploys with real users |
| Host-process `lpa-server` stdout capture into the Studio console (terminal-only today) | `2026-07-05-studio-logging-model` | Host-process workflow needs in-console server logs |
| Console filter persistence and text search (session-only, no search today) | `2026-07-05-studio-logging-model` | Console usage patterns make refiltering per session annoying |
| Per-item overlay gating (fetch-full-on-change assumes small overlays) | `2026-07-04-studio-editing-model` (a) | Measured overlay fetch cost matters |
| ~~Singular `ProjectRegistry::mutate` bypasses policy/type validation (only `mutate_batch` enforces)~~ — **closed 2026-08-01**: `mutate` now runs the same `validate_mutation`; the bypass survives only as the crate-private `stage_dedicated_op` the node-authoring ops use to write `Fixed` containers | `2026-07-04-studio-editing-model` (d) | Closed |
| Alternative dirty modes (touched-mode / deliberate value pinning) — minimal-diff normalization fixed dirty to "differs from saved" | `2026-07-04-studio-editing-model` (f) | A concrete pinning/touched-mode use case appears |
| Device-pane adoption of the pane grammar (`StudioPane`/`DetailPopover`/`UiPaneAction`) | `2026-07-05-studio-pane-grammar` (a) | Next device-pane UX work |
| Save visibility while scrolling (project header scrolls with the sidebar; the strip was always visible) | `2026-07-05-studio-pane-grammar` (b) | The M2a UX gate or later use flags losing always-visible Save |
| Tint-variant loser's story removal (D7 pick pending at the M2a gate) | `2026-07-05-studio-pane-grammar` (c) | The tint pick is recorded in the M2a plan notes |
| Probe payload optimization (binary/compressed preview encoding, delta frames, transferable sim frames on PreviewHost's zero-copy precedent) | `2026-07-04-client-pull-loop-and-actor` (b); `2026-07-27-completion-based-refresh-pacing` | Steady-state tick cost is dominated by raw probe bytes; own design pass with measurements |
| Native/tokio actor parity: `tokio::spawn`/`LocalSet` spawn helper + native timer factory | `2026-07-04-client-pull-loop-and-actor` (c) | A native Studio shell exists |
| Layout-header semantic chunking (per-lamp-range events) | `2026-07-04-envelope-streaming` | Display-layout fixtures grow ~4×+ past the 16 KiB frame budget |
| Sub-root slot progressive patching | `2026-07-04-envelope-streaming`; `2026-06-27-project-read-event-frames` | `SlotMirrorView` can apply partial root snapshots safely |
| Real-hardware Studio smoke of the gated multi-frame serial read | `2026-07-04-envelope-streaming` | Post-merge hardware validation pass |
| Binary/compressed payload encoding for project-read frames | `2026-06-27-project-read-event-frames`; `2026-06-27-ser-write-json-raw-value` | JSON/base64 overhead becomes material after the bounded-transport contract settles |
| Membership-only `ids_revision` bump (strictly on id add/remove) | `2026-07-03-revision-gated-project-reads` | A correctness-neutral chattiness lean-out is worth doing |
| Flatten the now single-variant `AssetSlotValue` enum; directory-per-node layout | `2026-07-04-json-only-artifacts` | Studio editing work touches asset/node layout |
| ELF-symbol `Content` guardrail check in CI | `2026-07-04-json-only-artifacts` | CI ground-truth guardrail is prioritized |
| Concrete `UxRegistry`; operation-metadata derive macros | `2026-06-21-studio-ux-layer` | Dynamic UX nodes need registration/dispatch, or the manual op-metadata model has more usage pressure |
| `Ui*`→`*View` / `*Ux`→`*Controller` / `App*`→domain-noun renames | `2026-06-24-studio-core-and-layer-vocabulary` | The crate/layer refactor reaches the naming pass |
| Host-serial ESP32 management; self-hosted/vendored browser esptool; raw LittleFS backup/restore; long-management cancel/retry | `2026-06-22-studio-link-management-workflow` | Host-serial support, offline builds, backup, or flash/erase recovery is prioritized |
| Cancellation/retry affordances and section-aware Device activity | `2026-06-22-studio-device-ux-workflow`; `2026-06-22-studio-link-management-workflow` | Hardware workflows settle and need finer recovery control |
| CI/browser tooling for `wasm-bindgen-test`/Playwright worker smoke | `2026-06-17-browser-firmware-runtime`; `2026-06-17-studio-link-and-local-runtimes` | Browser-runtime CI execution is provisioned |
| ~~Offline artifact upgrader (Studio/desktop) consuming `schemas/history/` shape dumps + fixtures~~ — **closed 2026-08-04**: `lp-app/lpa-upgrade` (`2026-08-04-project-format-migration-architecture`) | `2026-07-05-artifact-format-version-and-schema-snapshots` | Closed |
| ~~CI check that a `PROJECT_FORMAT_VERSION` bump lands with a `schemas/history/` snapshot~~ — **closed 2026-08-04**: `lpa-upgrade`'s `the_current_format_has_a_history_snapshot` test (`2026-08-04-project-format-migration-architecture`) | `2026-07-05-artifact-format-version-and-schema-snapshots` | Closed |
| CLI adoption of `DeviceSession` (lp-cli still hand-rolls provider/session bundles; `fwcheck`'s boot-line grep dies then) | `2026-07-15-device-session-model` | Device-link M5 (CLI) work begins |
| Websocket / server-lightplayer connector classes on the capability model | `2026-07-15-device-session-model` | A remote (non-serial) device class becomes real |
| Fuel heatmap / GLSL probe synergy (trap pixel = probe selection; vmctx `metadata` reserved for trace state) | `2026-07-20-lpvm-native-fuel` | Probes landed (`lps-probe`, 2026-07-25); revisit with probe/agent-activity visualization work |
| Per-function shared trap stub (shrink back-edge fuel checks from 7 to ~5 words) | `2026-07-20-lpvm-native-fuel` | ESP32 16 KB JIT chunk budget gets tight |
| Compute-tick / shader-init fuel blame route (traps abort bounded but bypass the panic/blame ledger) | `2026-07-20-lpvm-native-fuel` | Runaway compute shaders show up in practice |
| Budgeted/async shader compile (spread the ~194 ms device compile across frames instead of one long frame per apply) | `2026-07-14-shader-auto-apply` | The per-apply frame stall matters in practice |
| Classic-ESP32 (LX6) host execution: the Xtensa guest image is one flat code-region buffer, correct only under an **offset** I-bus alias; classic's SRAM1 alias is word-mirrored and `build_xt_image` rejects it | `2026-07-30-isa-parameterized-host-emu-engine` | An LX6 host execution target is wanted |
| ~~Xtensa shader-code budget: ~28 KiB free in the 112 KiB text region after ~84 KiB of builtins~~ — **closed 2026-08-01**: the builtins image is flash-resident, so the shader has the whole 128 KiB SRAM code region | `2026-07-30-isa-parameterized-host-emu-engine` | Closed; the budget is now the shader's own size |
| Measured Xtensa cycle model (`CycleModel::InstructionCount` is the honest default; rv32 has a measured C6 table) | `2026-07-30-isa-parameterized-host-emu-engine`; `2026-07-28-emu-core-crate-family` | The filetest perf column needs Xtensa numbers to mean something |
| Sim-worker recovery layer 2 (timeout-streak detection → terminate+respawn preserving the unsaved-overlay mirror; NotResponding sim roster card; PreviewHost in-flight deadline) | `2026-07-23-sim-wasm-fuel`; `2026-07-24-runtime-pool` | Next sim-runtime lifecycle work (the pool landed without it; requirements in the M4 plan notes) |
| Live sim-card frames: core-owned present service for pool sessions sharing PreviewHost's CPU blit seam + the gallery routing rule (sim frames instead of a preview lease) | `2026-07-24-runtime-pool` | The roadmap's live-thumbnails item is prioritized |
| In-card setup FORM (blank device tab as a date-named `YYYY-MM-DD HH:MM LightPlayer` name field + Flash button); only the one-click provision dispatch landed | `2026-07-26-card-view-state-ownership` | The provisioning UX gets its next pass |
| Node-arm UI signals re-home (this ADR did the device-card arm only; the first node slice — `NodeCardUiState` drawers + agent collapse + composer-draft mirror — landed 2026-07-27, PR #158) | `2026-07-26-card-view-state-ownership` | The state audit's node wave begins |
| Scene-fork `view-transition-name` switches to `identity_key()` (one canonical per-card key) | `2026-07-26-card-view-state-ownership` | `claude/spike-view-transitions` lands |
| Post-reset auto-reconnect tuning + factory-reset stuck-state root cause (editor-sever/return-to-gallery half is in) | `2026-07-26-card-view-state-ownership` | The device reset/flash flow is re-tested on hardware |
| Real vmctx block on the browser path (guest shader shares the fw-browser module's linear memory with vmctx at address 0) | `2026-07-23-sim-wasm-fuel` | Browser memory-layout work or probe trace state lands |
| Per-instance host recovery contexts (typed errors could feed a host blame ledger without panics) | `2026-07-23-per-target-panic-strategy`; `2026-07-04-crash-recovery-model` | Host-side blame for failing shaders becomes a product need |
| Worker offload for probe evaluation (interp fuel now bounds infinite-loop shaders — `lpir::InterpLimits` + `lps-probe` `MAX_OPS_PER_EVAL` — so this is purely about main-thread jank) | `2026-07-25-shader-probe-experiment-api`; `2026-07-25-studio-shader-agent-architecture` | Live walks show probe-eval jank |
| lps-glsl as a linter pass over the probe unit (better spans than the naga oracle) | `2026-07-25-shader-probe-experiment-api` | Span quality on probe/agent diagnostics becomes a complaint |
| Frontend dialect gap: engine frontends accept bare uniforms, the probe oracle (naga) requires `layout(binding=N)` — a shader can render on-engine yet fail health | `2026-07-25-shader-probe-experiment-api` | A live gate or demo shader hits it; align or lint in the agent path |
| Display-only board allowlist shrinks (DevKitC v4, QuinLED Dig-Uno gain runtime manifests) | `2026-07-31-board-display-metadata-split` | A classic-ESP32 (v3) `HardwareTarget` lands |
| Probe/agent-activity visualization (render probe domains/results on the preview) | `2026-07-25-studio-shader-agent-architecture` | M6 capture lands (the node-UX pass landed 2026-07-26 as `node-card-faces` without it); likely its own plan |
| Live-sim binding push (agent binding overrides reach only the probe oracle today; transient push needs visible indication + auto-clear) | `2026-07-25-studio-shader-agent-architecture` | Outward-from-GLSL capability work begins |
| In-web local model provider (the reason `ModelProvider` exists) | `2026-07-25-studio-shader-agent-architecture` | A credible in-browser model is worth serving |
| Unreserve the `iterate` tool's `capture` field | `2026-07-25-studio-shader-agent-architecture` | The M6 preview snapshot seam lands |
| Shader face perf line (cycle model exists engine-side; face reserves no space yet) | `2026-07-26-node-card-faces` | The perf-line run of work is prioritized |
| Playlist strip evolutions: timeline view, cue-trigger UI, autoplay-to-cue controls | `2026-07-26-node-card-faces` | Playlist interaction work resumes |
| Fixture mapping editor drawer (face's planned custom drawer; fixture has only advanced today) | `2026-07-26-node-card-faces` | Fixture mapping UX work begins |
| Hardware placard-follow walk under a live trigger (the rest of the refinement round — live knob values, friendly titles, entry-thumb warming, knob keyboard a11y, activate-by-click, drawer-state re-home — landed 2026-07-27, PR #158) | `2026-07-26-node-card-faces` | The next hardware walk |
| ~~Sim button press / debug pokes adopt the runtime command channel as new `WireNodeCommand` variants~~ — **obsolete 2026-08-01** (`2026-08-01-debug-slots-taxonomy`): button input is punted to the input initiative (record/replay needs source-level injection), and debug pokes are Debug slots, not commands. The channel stays the right home for genuine events | `2026-07-27-runtime-node-command-channel` | Obsolete |
| Cache-friendly prompt shape: the per-turn system prompt embeds the current shader source ahead of the cache prefix, so staged edits invalidate the cache (measured; live sessions read ~0 cached tokens) | `2026-07-25-studio-shader-agent-architecture` (2026-07-27 addendum) | Next agent round — P0; redesign + eval re-measure, not a blind fix |
| Pre-4.6 Anthropic thinking shape (`enabled` + `budget_tokens`; current `adaptive` shape 400s on Sonnet/Haiku 4.5) | `2026-07-25-studio-shader-agent-architecture` (2026-07-27 addendum) | An older Anthropic model is configured deliberately |
| OpenRouter reasoning opt-in (`reasoning: {}` request field, provider-gated) | `2026-07-25-studio-shader-agent-architecture` (2026-07-27 addendum) | OpenRouter thinking visibility is asked for |
| Display-driven per-surface probe sizing, capped by the runtime tier | `2026-07-27-completion-based-refresh-pacing` | Multi-node probing has soaked and card-size feedback plumbing is worth the plumbing |
| Probe revision-gating on the wire (skip unchanged probe bytes; display-layout's `IfChanged` read is the precedent) | `2026-07-27-completion-based-refresh-pacing` | Steady-state probe bytes dominate tick cost on a real link |
| Sim "non-collapsed" probe scope becomes real (collapse is view-local today, so sim probes ALL nodes) | `2026-07-27-completion-based-refresh-pacing` | The ui-state-audit plan moves live collapse state into core |
| Packed base64 geometry encoding (`points_packed`-style additive field) | `2026-07-27-map2d-document-architecture` | An imported mapping document approaches the 10 KiB asset body budget |
| Legacy `MappingConfig` variant retirement (`PathPoints`/`RingArray`/`PointList`/`SvgPath`) | `2026-07-27-map2d-document-architecture` | M5 one-home mapping editing lands and shipped projects are migrated |
| ~~Editing individual instances of a live repeat (per-instance overrides without expanding)~~ — **closed 2026-08-05**: answered as write-through tessellation authoring with inert instances, NOT overrides (`2026-08-05-map2d-editor-selection-tree-model`) | `2026-08-05-map2d-format-2-repeat-and-gaps` | Closed |
| Symmetries beyond rotation (mirror; combined groups) | `2026-08-05-map2d-format-2-repeat-and-gaps` | A fixture that actually has one shows up |
| `Map2dShape::Group(Vec<...>)` in the format (additive format 3, loud refusal; the editor path model is already arity-general) | `2026-08-05-map2d-editor-selection-tree-model` | A real grouping use case beyond rotational repeat shows up |
| Node ids in the map2d document (stable selection across structural edits; positional paths + drop-on-dangle serve today) | `2026-08-05-map2d-editor-selection-tree-model` | Collaborative or long-lived selection state needs stability |
| Share-envelope format migration — **partly closed 2026-08-04**: envelope-carried project *content* now gates-and-migrates via `lpa-upgrade` (`2026-08-04-project-format-migration-architecture`); the envelope's own `format` field and bare-node migration remain refuse-outright | `2026-07-28-share-envelopes`; `../debt/library-format-migration-gap.md` | Bare-node migration: the `artifact_format` stamp becomes universal enough to migrate against |
| `just test-browser-shader-frontend` joins CI's path-gated Validate job (local `test-rust` runs it; until then the only engine-level Naga-frontend coverage is local-gate-only) | `2026-08-05-sampler2d-authored-surface` | Shader-path CI budget is next reviewed, or a frontend-coupled regression reaches a PR unflagged |
| Generated `palette_at(t)`-style helper becomes the documented palette API (pure generated GLSL over the standard spelling; zero new parser surface) | `2026-08-05-sampler2d-authored-surface` | M5's gate — the fyeah-sign port shows whether the `vec2(t, 0.0)` idiom reads acceptably |
| Browser CPU tier converges onto `LpsGlsl` (`fw-browser/src/runtime.rs` reserves this; would shrink the combined-sampler bridge to the GPU tier's copy) | `2026-08-05-sampler2d-authored-surface` | Frontend convergence becomes a product priority |
| Persistent last-frame snapshots for offline device cards (in-session only today; the ▶ box falls back to board + Reconnect) | `2026-08-06-honest-device-preview` | The M6 project-thumb `<img>` seam / LibraryStore metadata lands |
| Browser GPU tier cannot render control products (debt `browser-gpu-tier-cannot-render-control-products`; async readback preferred; per-tick retry must become a classified failure) | `2026-08-06-honest-device-preview` | The active fix task lands, or a second surface hits it |
| ~~Open-in-sim auto-selects the inherited board~~ — **closed 2026-08-06**: landed as `SetupEvent::SetUpElsewhere` (PR #369, both landing shapes incl. the D37 re-attach) | `2026-08-06-honest-device-preview` | Closed |
| ▶ meta-row project chip truncates hard beside the Editor button at card width | `2026-08-06-honest-device-preview` | It grates at a walk |
| Wire-read (`FsRequest`) export for device-hosted projects absent from the local library (editor-popup export is library-backed only) | `2026-07-28-share-envelopes` | Someone needs to export a project that only exists on a device |
| Size guard on node share envelopes (a large binary asset base64s into something no clipboard should carry) | `2026-07-28-share-envelopes` | A real shader-with-texture share hits a clipboard limit |
| Per-board partition table selection (4/8/16 MB chosen at build time, so no board assumption is baked into `partitions.csv`) | `2026-07-30-esp32s3-partition-floor` | A second ESP32-S3 board with a different flash size actually exists |
| Classic-ESP32 (LX6) address windows for the Xtensa backtrace walk (the ABI is shared, the memory map is not) | `2026-07-30-xtensa-backtrace-window-spill` | A classic-ESP32 firmware target needs frames in its crash reports |
| Xtensa exception-frame walking (crashes arriving through the exception vector rather than `panic!`; `walk_frames_from` already takes an explicit `(ra, sp)` for it) | `2026-07-30-xtensa-backtrace-window-spill` | The S3 needs backtraces for hardware faults, not just panics |
| Emitter peephole for the Xtensa integer-div-by-zero guard (`Movi`+`BranchRr` → a single `BranchZ(Beqz)`; `MOVEQZ`/`MOVNEZ` fusion, both already encoded/decoded/emulated in `lp-xt-inst`/`lp-xt-emu`) | `2026-07-30-integer-division-never-traps` | Xtensa code-size or instruction-count pressure makes trimming the guard worth it |
| Firmware as a *writer* of the boot-control sector (latch degraded state across a power cycle; today it only reads and consumes) | `2026-07-30-boot-control-sector` | The boot-complete redefinition and brownout blame policy land |
| Firmware consumption of the boot-control safe-mode clamp (bits `8..16` are now ASSIGNED — Studio writes skip+clamp and the format defines clamp-overrides-skip precedence; firmware still only honors the skip) | `2026-07-30-boot-control-sector` | The safe-clamp / fixture mA-limiter work begins |
| Last-crash summary mirrored into the boot-control sector (the RTC crash record is unreadable from a board that never boots) | `2026-07-30-boot-control-sector` | Post-mortem from a non-booting board is needed |
| `lpfs` partition subtype is `spiffs` but the filesystem is littlefs (`esp-idf-part` supports `littlefs`) | `2026-07-30-boot-control-sector` | The partition table is being changed for another reason anyway |
| Surfacing link mode in the UI + the "waiting for a device in bootloader mode" confirmation that makes the BOOT-button ritual learnable | `2026-07-30-bootloader-mode-detection` | M5 of the device-recovery plan |
| Flapping-device heuristic (enumeration-drop counting), deliberately offer-only — it must never trigger a probe, which would reboot a device that may just have a loose cable | `2026-07-30-bootloader-mode-detection` | M9 of the device-recovery plan |
| Backup-format promotion to user-facing (external docs + per-version fixture archives) | `2026-07-31-device-backup-archive-format` | The format is promised outside the repo |
| C6 migration to `lp-ws281x` (`docs/debt/c6-on-legacy-ws281x-driver.md`) | `2026-07-31-lp-ws281x-multi-channel-driver-adoption` | A second C6 channel is wanted, a `lp-ws281x` fix needs to reach the C6, or maintaining two drivers becomes its own tax |
| `MAX_LEDS` silent truncation and duplication (`docs/debt/output-channel-led-cap-silent-truncation.md`) | `2026-07-31-lp-ws281x-multi-channel-driver-adoption` | A long strip is authored and the cap bites with no diagnostic |
| Float→int through the soft-float ABI (`__fixsfsi`/`__fixunssfsi` are skipped because the ABI leaves out-of-range and NaN undefined) | `2026-07-31-soft-float-via-compiler-builtins` | The C6 harness's probe data shows the ROM matching `compiler_builtins` at those edges |
| Xtensa `F32Lowering` arm (`Unsupported` today — the S3 has an FPU, so soft float would be the wrong default) | `2026-07-31-soft-float-via-compiler-builtins` | The Xtensa FPU emulator and emitter land (roadmap M6/M7) |
| Soft-float performance measurement on the C6 (nothing here says how slow Float mode is) | `2026-07-31-soft-float-via-compiler-builtins` | A perf surface exists to show it (roadmap D3) |
| `Debug` naming re-check (ratified as provisional — the corpus is all diagnostics, but the clock's `rate`/`scrub_offset_seconds` read as transport) | `2026-08-01-debug-slots-taxonomy` (a) | The clock's transport controls move to a transport surface |
| Debug indication on preview/play surfaces (D8 covered the workspace only; a running installation in test-pattern mode shows nothing outside the editor) | `2026-08-01-debug-slots-taxonomy` (b) | The panels/play-mode work defines its own chrome |
| `TEST_PATTERN_RGB` is full white — the max-current case on long strips, deliberate for pin discovery | `2026-08-01-debug-slots-taxonomy` (c) | Someone runs it on a long strip and wants it dimmer |
| Slew shaping on panel writers (emission is immediate; the seam is writer-side shaping) | `2026-08-02-panel-writers-and-state-persistence` | `panel.md` P-Q1 gets an answer, or a control visibly zippers |
| Panel state encoded through `slot_codec` instead of a second serde codec (24,912 B of duplicate C6 flash) | `2026-08-02-panel-writers-and-state-persistence` | Flash headroom tightens again — `docs/debt/panel-state-serde-flash-cost.md` |
| A kind with no face publishes no panel controls (`ComputeShader`'s bound uniforms reach the wiring drawer, never a knob) | `2026-08-03-panel-visibility-is-derived` | A compute-driven module needs its knobs — `examples/meteor` already does |
| Authored panel layouts: a curated promoted-control list per module, as an additive override on the derived default | `2026-08-03-panel-visibility-is-derived` | A published/vendored module needs a curated public API |
| A minted `status-engaged` token family (engaged currently borrows `status-attention` amber; Yona leans maybe-blue) | `2026-08-03-panel-visibility-is-derived` | Yona settles the engaged treatment — do not change it before then |
| Share-envelope hygiene for device refs (a derived uid embeds a MAC; associations and history events carry uids) | `2026-08-04-device-identity-anchored-in-silicon`; `2026-07-28-share-envelopes` | Envelopes stop being version-and-refuse and start travelling |
| Multi-studio registry sync (uids now agree across installs by construction; syncing rows is its own feature) | `2026-08-04-device-identity-anchored-in-silicon` | Someone runs two Studio installs against one fleet |
| Retiring the hello's `device_uid` field and the `/.lp/device.json` read | `2026-08-04-device-identity-anchored-in-silicon` | No fielded board still needs migrating |
| An LFO node for panel-reachable waveform/offset/modulation (the panel exposes a phasor's period ONLY) | `2026-08-04-time-is-a-product` | A module wants modulation the Speed knob cannot express |
| Transport UI over the breakpoint log (play/pause/scrub as a first-class surface) | `2026-08-04-time-is-a-product` | `docs/debt/clock-transport-has-no-transport-ui.md` exit criteria |
| v1–v3 project-format migration (below `lpa-upgrade`'s floor; types are deleted, corpus is `schemas/history/` snapshots only) | `2026-08-04-project-format-migration-architecture` | A real holder of pre-v4 project data appears |
| Safe-mode board rescue hole (upload cannot reach a safe-mode board, so pull→migrate→push cannot run on one) | `2026-08-04-project-format-migration-architecture` | `docs/debt/safe-mode-board-rescue-hole.md` — first field occurrence |
| Server session-set switching (several concurrently-valid sessions per browser, switched without a re-auth round trip) | `2026-08-07-provider-based-auth` | Switch-account usage or complaints show the lean re-auth round trip is the friction point |
| Local password method for self-host (the `local` connection's password method, sibling to the dev picker) | `2026-08-07-provider-based-auth` | A self-host deployment target is prioritized |
| Identities link table `(connection, subject) → user` for multi-provider accounts on one user | `2026-08-07-provider-based-auth` | A second connection type ships and accounts need to merge across them |

## Relationship To Shared Planning

Plans, roadmap-level plans, reviews, reports, scratch notes, and phase prompts
live in the personal planning workspace configured by `PHOTOMANCER_PLANNING_ROOT`
or `~/.photomancer/planning`.

Only durable decisions graduate into `docs/adr/`.
