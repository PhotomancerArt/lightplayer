# Debt register

Standing burdens we consciously carry. The third register: ADRs record
decisions, defects record failures, debt records **conditions** — a
weak subsystem, a structural tax, a workaround-encrusted area that we
have chosen (for now) to live with. Naming the burden makes carrying
it intentional instead of frustrating.

Entries are **named by slug, not date** — `story-capture-pipeline.md`
— because a debt entry is a long-lived handle cited from defects,
chips, and plans; dates live in frontmatter (`since`/`logged`) and in
the incident log. (Defects and ADRs stay date-named: they are events;
debt is a condition.)

## The filing bar

File debt when a burden is **structural and recurring** — it taxes
work repeatedly, has resisted (or not merited) an immediate fix, and
somebody keeps re-learning its workarounds. One entry per burden, not
per incident: incidents APPEND to the entry's log. Todos, feature
ideas, and one-off deferrals do not belong here — they stay task
chips and planning notes.

## Entry template

```markdown
---
status: carried        # carried | paying-down | retired
since: YYYY-MM-DD      # best-effort inception of the condition
logged: YYYY-MM-DD     # when this entry was filed
area: <subsystem>
related: []            # defects, ADRs, chips, plan dirs
---
# <the burden, named>

**Shape** — what is weak and why it is structural, not one bug.
**Carrying cost** — what it taxes, concretely (time, flakes, blocked
gates, re-learned lore).
**Workarounds** — the operational knowledge that makes it livable
(exact incantations; keep current).
**Incident log** — dated, append-only. The accumulating evidence; a
lengthening log is the paydown-priority signal.
**Exit criteria** — what "paid down" observably means. Debt without an
exit definition is a complaints file.
```

Paying down debt is often a real decision among alternatives (rebuild
vs replace vs relocate) — when it is, the decision becomes an ADR and
the entry links it, flips to `paying-down`, then `retired` (entries
stay in place when retired; the log is the history).

## Index

| Entry | Status | Since | Area | Cost in one line |
| --- | --- | --- | --- | --- |
| [bounds-asserted-in-the-wrong-unit](bounds-asserted-in-the-wrong-unit.md) | carried | 2026-06-12 | cross-cutting — lp-collection, lps-glsl, lpvm-native regalloc | limits written in the unit that was easy to count, not the one the consumer enforces; all three instances passed their tests, and one miscompiles silently |
| [two-green-prs-can-red-main](two-green-prs-can-red-main.md) | carried | 2026-08-02 | CI / merge policy | CI never builds the merge result, so two PRs touching opposite sides of an interface both pass and main breaks; cost is misattributed blame, and a build canary would collapse it |
| [per-frame-optimisations-are-unpriced-in-ram](per-frame-optimisations-are-unpriced-in-ram.md) | carried | 2026-08-01 | lpc-engine dataflow + the classic ESP32 image | every "compute once, keep it" win ships with a cycle number and no byte number; #243 cost the classic ~8.3 KB (~90 LEDs) and a day of hardware bisecting to find |
| [c6-scan-truncation-accepted](c6-scan-truncation-accepted.md) | carried | 2026-08-01 | fw-esp32c6 ws281x default config | 2ch default truncates ~28% of frames during WiFi scans (editing-time only); reopens on OPC/E1.31 streaming |
| [brightness-applied-before-gamma](brightness-applied-before-gamma.md) | carried | 2026-08-01 | lpc-engine fixture node (value pipeline) | dim gamma-on fixtures collapse to ~1 wire code (30× resolution loss at brightness 38); projects work around it by shipping gamma off |
| [lps-probe-perf-test-load-sensitive](lps-probe-perf-test-load-sensitive.md) | carried | 2026-08-01 | lps-probe/tests | spurious full-gate reds whenever a dev server or sibling session runs; ~20% wall-clock headroom |
| [bundled-firmware-chip-unplumbed](bundled-firmware-chip-unplumbed.md) | carried | 2026-07-27 | studio-web roster cards | the "firmware update available" chip is implemented, tested and story-visible, but `bundled_fw` is never supplied in production — the feature never fires |
| [local-gate-misses-what-ci-checks](local-gate-misses-what-ci-checks.md) | carried | 2026-06-22 | justfile local gate vs CI | `just check` skips wasm32, lpa-studio-web's host test cfg, all test code, and the `stories` feature; "green locally" is not evidence and every Studio phase re-learns the four extra commands |
| [legacy-mapping-variants](legacy-mapping-variants.md) | paying-down | 2026-07-27 | fixture-mapping | pre-Map2d mapping variants ride along beside the Map2d document, so every mapping consumer carries the old shapes too |
| [story-capture-pipeline](story-capture-pipeline.md) | carried | 2026-07-08 | studio-web/story-capture | ~15 min + flake retries per UI change; visual gates block under load |
| [web-serial-js-untestable](web-serial-js-untestable.md) | carried | 2026-07-10 | lpa-link/browser-serial | JS session/flash layer ships untested; bugs surface only on hardware |
| [library-format-migration-gap](library-format-migration-gap.md) | paying-down | 2026-07-08 | studio library/formats + share envelopes | `lp-app/lpa-upgrade` (2026-08-04, PR #344) closed the silent-failure and no-migration-path gaps everywhere; still open: bare-node migration and v1–v3 support, each refused honestly today and each with its own trigger |
| [gpu-tier-cannot-sample-led-output](gpu-tier-cannot-sample-led-output.md) | retired | 2026-07-09 | fw-browser/tier + lp-gfx-wgpu | retired 2026-08-05: browser GPU tier samples via async readback (one frame of latency) |
| [firmware-capability-reporting](firmware-capability-reporting.md) | retired | 2026-07-30 | lpc-engine/nodes + lpc-wire | a gated-out node's runtime was silently inert — paid down 2026-08-01: hello carries build/hardware facts, placeholders report `Unsupported`, studio says "Not on this device" |
| [firmware-partition-constants-transcribed](firmware-partition-constants-transcribed.md) | carried | 2026-07-30 | lp-fw/fw-esp32c6 | the C6 hardcodes lpfs offset/size copied by hand from partitions.csv; a verbatim port to the S3 would have erased running code |
| [output-channel-led-cap-silent-truncation](output-channel-led-cap-silent-truncation.md) | retired | 2026-05-18 | lp-fw/fw-esp32-common + fw-esp32c6 output | `MAX_LEDS = 256` per-channel cap is silent (no log, no error) and duplicated in two crates; a long strip renders truncated with no diagnostic |
| [s3-frame-cost-scales-per-fixture](s3-frame-cost-scales-per-fixture.md) | carried | 2026-07-31 | lpc-engine resolver + lpc-hardware registry | Frame cost is flat ~8.4 ms/fixture: per-frame dataflow re-resolution + per-frame endpoint-status recomputation; the shader JIT is ~1%, sends 11% |
| [per-lamp-data-stored-three-times](per-lamp-data-stored-three-times.md) | carried | 2026-07-31 | lpc-model fixture mapping slots + lpc-engine fixture/output nodes | a lamp's position is stored three times and its colour twice — 31.6 of the classic's 89.5 B/LED; the big half is a wire-visible slot schema, so it did not land with the measurement |
| [c6-on-legacy-ws281x-driver](c6-on-legacy-ws281x-driver.md) | retired | 2026-07-31 | lp-fw/fw-esp32c6/src/output | C6 ran its own single-channel WS281x driver, not `lp-ws281x`; retired 2026-08-01 when the C6 moved onto the shared core and gained its second channel |
| [panel-state-serde-flash-cost](panel-state-serde-flash-cost.md) | carried | 2026-08-02 | lpa-server/panel_state + serde surface | a SECOND LpValue JSON codec in the image costs 50,512 B of C6 flash; the first one is already there and unused by panel state |
| [example-shaders-not-compile-gated](example-shaders-not-compile-gated.md) | carried | 2026-07-29 | examples GLSL + CI + lps-filetests | an example shader can compile on the host yet fail on 4 of 5 targets; the break surfaces only when a human opens it in Studio |
| [clock-transport-has-no-transport-ui](clock-transport-has-no-transport-ui.md) | carried | 2026-05-12 | clock node + studio faces | scrubbing a show means typing seconds into a generic slider; the misfit also keeps the `Debug` name provisional |
| [bus-time-precision](bus-time-precision.md) | carried | 2026-05-12 | lpc-engine clock/timebase + shader `seconds` uniforms | unbounded seconds are f32 (~8 ms ulp after a day) and saturate at ±32768 s in fixed mode — a fixed-mode `seconds` animation silently freezes after 9.1 h |
| [project-reload-drops-debug-silently](project-reload-drops-debug-silently.md) | carried | 2026-07-04 | lpa-server project lifecycle | the documented recovery path discards every pending edit and Debug override with no return value, event, or notice |
| [registry-apis-without-production-callers](registry-apis-without-production-callers.md) | carried | 2026-07-04 | lpc-registry project APIs | `discard_overlay` is public, test-only API that duplicates the `MutationOp::Clear` path and silently drifts from it |
| [save-notice-assumes-header-dispatch](save-notice-assumes-header-dispatch.md) | carried | 2026-07-04 | lpa-studio-core save flow | "no persisted edits to write" is phrased for the gated header path but the asset editors dispatch the same op ungated |
| [safe-mode-dim-boot-unproven](safe-mode-dim-boot-unproven.md) | retired | 2026-08-01 | fw-esp32c6/bootctl + lpc-engine safe clamp | RETIRED 2026-08-01: dim boot seen on silicon (serial: record found/consumed/clamped 26/255; heartbeat outputClamp; eyes: dim) |
| [studio-no-reconnect-after-replug](studio-no-reconnect-after-replug.md) | carried | 2026-07-31 | lpa-link/browser-serial + studio device cards | every bootloader-mode op ends on a replug Studio cannot see; the op card waits forever and the user reloads the tab |
| [safe-mode-board-rescue-hole](safe-mode-board-rescue-hole.md) | carried | 2026-08-04 | lp-cli/lpa-link upload + project-format migration | upload cannot reach a safe-mode board, so a board wedged in safe mode holding an old-format project has no non-destructive rescue path (Upgrade's push leg cannot complete) |
| [browser-gpu-tier-cannot-render-control-products](browser-gpu-tier-cannot-render-control-products.md) | carried | 2026-08-05 | lp-gfx browser GPU tier + lpa-server preview host | control renders need a blocking readback the browser GPU tier lacks, so module control previews sit "not tracked" forever and the preview host retries the failed render every tick |
| [palette-row-favours-the-name](palette-row-favours-the-name.md) | carried | 2026-08-06 | lpa-studio-web palette chooser row | the gradient gets 96px of a ~280px row and its label the rest, so the surface you scan by eye gives least space to the thing being chosen; a full-width-strip rethink trades ~9 visible rows for ~6 |
| [wasm-cloud-check-not-in-just-check](wasm-cloud-check-not-in-just-check.md) | carried | 2026-08-07 | justfile local gate vs lpa-cloud-client wasm32 build | `check-wasm-cloud` (the fast gate for lpa-cloud-client's browser feature combo) is not in `just check`'s chain; only `just studio-web-build` (slow) would otherwise catch a regression |
| [hotlinked-provider-avatar-posture](hotlinked-provider-avatar-posture.md) | carried | 2026-08-07 | lp-cloud-domain CloudUser.picture_url + avatar rendering | provider photo URLs are hotlinked and refreshed every login with no verified posture on expiry, rate limits, or the privacy leak of a live per-render request to the provider |
| [placement-rotation-quirks](placement-rotation-quirks.md) | carried | 2026-08-13 | lpa-mapping-editor canvas (dived doc layers) | the marquee is a doc-space rect and wiring numbers are doc-space text, so both render rotated over a rotated placement — accepted v1 (one-project-canvas Q6); revisit with the parked viewport-rotation work |
