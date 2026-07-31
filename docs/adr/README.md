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
| Host-process `lpa-server` stdout capture into the Studio console (terminal-only today) | `2026-07-05-studio-logging-model` | Host-process workflow needs in-console server logs |
| Console filter persistence and text search (session-only, no search today) | `2026-07-05-studio-logging-model` | Console usage patterns make refiltering per session annoying |
| Per-item overlay gating (fetch-full-on-change assumes small overlays) | `2026-07-04-studio-editing-model` (a) | Measured overlay fetch cost matters |
| Singular `ProjectRegistry::mutate` bypasses policy/type validation (only `mutate_batch` enforces) | `2026-07-04-studio-editing-model` (d) | Any new caller of `mutate` |
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
| Offline artifact upgrader (Studio/desktop) consuming `schemas/history/` shape dumps + fixtures | `2026-07-05-artifact-format-version-and-schema-snapshots` | Fielded devices hold old-format projects that must survive a breaking bump |
| CI check that a `PROJECT_FORMAT_VERSION` bump lands with a `schemas/history/` snapshot | `2026-07-05-artifact-format-version-and-schema-snapshots` | The first real format bump |
| CLI adoption of `DeviceSession` (lp-cli still hand-rolls provider/session bundles; `fwcheck`'s boot-line grep dies then) | `2026-07-15-device-session-model` | Device-link M5 (CLI) work begins |
| Websocket / server-lightplayer connector classes on the capability model | `2026-07-15-device-session-model` | A remote (non-serial) device class becomes real |
| Fuel heatmap / GLSL probe synergy (trap pixel = probe selection; vmctx `metadata` reserved for trace state) | `2026-07-20-lpvm-native-fuel` | Probes landed (`lps-probe`, 2026-07-25); revisit with probe/agent-activity visualization work |
| Per-function shared trap stub (shrink back-edge fuel checks from 7 to ~5 words) | `2026-07-20-lpvm-native-fuel` | ESP32 16 KB JIT chunk budget gets tight |
| Compute-tick / shader-init fuel blame route (traps abort bounded but bypass the panic/blame ledger) | `2026-07-20-lpvm-native-fuel` | Runaway compute shaders show up in practice |
| Budgeted/async shader compile (spread the ~194 ms device compile across frames instead of one long frame per apply) | `2026-07-14-shader-auto-apply` | The per-apply frame stall matters in practice |
| Classic-ESP32 (LX6) host execution: the Xtensa guest image is one flat code-region buffer, correct only under an **offset** I-bus alias; classic's SRAM1 alias is word-mirrored and `build_xt_image` rejects it | `2026-07-30-isa-parameterized-host-emu-engine` | An LX6 host execution target is wanted |
| Xtensa shader-code budget: ~28 KiB free in the 112 KiB text region after ~84 KiB of builtins; overflow is an explicit error | `2026-07-30-isa-parameterized-host-emu-engine` | A real shader hits it — the fix is `lps-builtins-xt-app/link.ld`'s split, not the host region |
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
| Probe/agent-activity visualization (render probe domains/results on the preview) | `2026-07-25-studio-shader-agent-architecture` | M6 capture lands (the node-UX pass landed 2026-07-26 as `node-card-faces` without it); likely its own plan |
| Live-sim binding push (agent binding overrides reach only the probe oracle today; transient push needs visible indication + auto-clear) | `2026-07-25-studio-shader-agent-architecture` | Outward-from-GLSL capability work begins |
| In-web local model provider (the reason `ModelProvider` exists) | `2026-07-25-studio-shader-agent-architecture` | A credible in-browser model is worth serving |
| Unreserve the `iterate` tool's `capture` field | `2026-07-25-studio-shader-agent-architecture` | The M6 preview snapshot seam lands |
| Shader face perf line (cycle model exists engine-side; face reserves no space yet) | `2026-07-26-node-card-faces` | The perf-line run of work is prioritized |
| Playlist strip evolutions: timeline view, cue-trigger UI, autoplay-to-cue controls | `2026-07-26-node-card-faces` | Playlist interaction work resumes |
| Fixture mapping editor drawer (face's planned custom drawer; fixture has only advanced today) | `2026-07-26-node-card-faces` | Fixture mapping UX work begins |
| Hardware placard-follow walk under a live trigger (the rest of the refinement round — live knob values, friendly titles, entry-thumb warming, knob keyboard a11y, activate-by-click, drawer-state re-home — landed 2026-07-27, PR #158) | `2026-07-26-node-card-faces` | The next hardware walk |
| Sim button press / debug pokes adopt the runtime command channel as new `WireNodeCommand` variants | `2026-07-27-runtime-node-command-channel` | Sim-button UX or runtime debug tooling work begins |
| Cache-friendly prompt shape: the per-turn system prompt embeds the current shader source ahead of the cache prefix, so staged edits invalidate the cache (measured; live sessions read ~0 cached tokens) | `2026-07-25-studio-shader-agent-architecture` (2026-07-27 addendum) | Next agent round — P0; redesign + eval re-measure, not a blind fix |
| Pre-4.6 Anthropic thinking shape (`enabled` + `budget_tokens`; current `adaptive` shape 400s on Sonnet/Haiku 4.5) | `2026-07-25-studio-shader-agent-architecture` (2026-07-27 addendum) | An older Anthropic model is configured deliberately |
| OpenRouter reasoning opt-in (`reasoning: {}` request field, provider-gated) | `2026-07-25-studio-shader-agent-architecture` (2026-07-27 addendum) | OpenRouter thinking visibility is asked for |
| Display-driven per-surface probe sizing, capped by the runtime tier | `2026-07-27-completion-based-refresh-pacing` | Multi-node probing has soaked and card-size feedback plumbing is worth the plumbing |
| Probe revision-gating on the wire (skip unchanged probe bytes; display-layout's `IfChanged` read is the precedent) | `2026-07-27-completion-based-refresh-pacing` | Steady-state probe bytes dominate tick cost on a real link |
| Sim "non-collapsed" probe scope becomes real (collapse is view-local today, so sim probes ALL nodes) | `2026-07-27-completion-based-refresh-pacing` | The ui-state-audit plan moves live collapse state into core |
| Packed base64 geometry encoding (`points_packed`-style additive field) | `2026-07-27-map2d-document-architecture` | An imported mapping document approaches the 10 KiB asset body budget |
| Legacy `MappingConfig` variant retirement (`PathPoints`/`RingArray`/`PointList`/`SvgPath`) | `2026-07-27-map2d-document-architecture` | M5 one-home mapping editing lands and shipped projects are migrated |
| Share-envelope format migration (`format` mismatches are refused outright during alpha, never migrated) | `2026-07-28-share-envelopes`; `../debt/library-format-migration-gap.md` | The authored formats settle enough that migration is written once, not weekly |
| Wire-read (`FsRequest`) export for device-hosted projects absent from the local library (editor-popup export is library-backed only) | `2026-07-28-share-envelopes` | Someone needs to export a project that only exists on a device |
| Size guard on node share envelopes (a large binary asset base64s into something no clipboard should carry) | `2026-07-28-share-envelopes` | A real shader-with-texture share hits a clipboard limit |
| CLA / DCO-with-explicit-grant mechanism (recorded as intent only) | `2026-07-29-license-provenance-discipline` | The first outside contribution to relicensing-sensitive code is proposed |
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

## Relationship To Shared Planning

Plans, roadmap-level plans, reviews, reports, scratch notes, and phase prompts
live in the personal planning workspace configured by `PHOTOMANCER_PLANNING_ROOT`
or `~/.photomancer/planning`.

Only durable decisions graduate into `docs/adr/`.
