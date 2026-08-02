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
| [brightness-applied-before-gamma](brightness-applied-before-gamma.md) | carried | 2026-08-01 | lpc-engine fixture node (value pipeline) | dim gamma-on fixtures collapse to ~1 wire code (30× resolution loss at brightness 38); projects work around it by shipping gamma off |
| [lps-probe-perf-test-load-sensitive](lps-probe-perf-test-load-sensitive.md) | carried | 2026-08-01 | lps-probe/tests | spurious full-gate reds whenever a dev server or sibling session runs; ~20% wall-clock headroom |
| [bundled-firmware-chip-unplumbed](bundled-firmware-chip-unplumbed.md) | carried | 2026-07-27 | studio-web roster cards | the "firmware update available" chip is implemented, tested and story-visible, but `bundled_fw` is never supplied in production — the feature never fires |
| [story-capture-pipeline](story-capture-pipeline.md) | carried | 2026-07-08 | studio-web/story-capture | ~15 min + flake retries per UI change; visual gates block under load |
| [web-serial-js-untestable](web-serial-js-untestable.md) | carried | 2026-07-10 | lpa-link/browser-serial | JS session/flash layer ships untested; bugs surface only on hardware |
| [library-format-migration-gap](library-format-migration-gap.md) | carried | 2026-07-08 | studio library/formats + share envelopes | breaking format changes silently invalidate durable authored data (library projects, pasted envelopes); failures surface as per-node parser errors — entry enumerates every surface and what it checks today |
| [gpu-tier-cannot-sample-led-output](gpu-tier-cannot-sample-led-output.md) | carried | 2026-07-09 | fw-browser/tier + lp-gfx-wgpu | gallery previews are dead for fixture-bearing projects; tier selection never asks what the project needs |
| [firmware-capability-reporting](firmware-capability-reporting.md) | retired | 2026-07-30 | lpc-engine/nodes + lpc-wire | a gated-out node's runtime was silently inert — paid down 2026-08-01: hello carries build/hardware facts, placeholders report `Unsupported`, studio says "Not on this device" |
| [firmware-partition-constants-transcribed](firmware-partition-constants-transcribed.md) | carried | 2026-07-30 | lp-fw/fw-esp32c6 | the C6 hardcodes lpfs offset/size copied by hand from partitions.csv; a verbatim port to the S3 would have erased running code |
| [output-channel-led-cap-silent-truncation](output-channel-led-cap-silent-truncation.md) | retired | 2026-05-18 | lp-fw/fw-esp32-common + fw-esp32c6 output | `MAX_LEDS = 256` per-channel cap is silent (no log, no error) and duplicated in two crates; a long strip renders truncated with no diagnostic |
| [s3-frame-cost-scales-per-fixture](s3-frame-cost-scales-per-fixture.md) | carried | 2026-07-31 | lpc-engine resolver + lpc-hardware registry | Frame cost is flat ~8.4 ms/fixture: per-frame dataflow re-resolution + per-frame endpoint-status recomputation; the shader JIT is ~1%, sends 11% |
| [c6-on-legacy-ws281x-driver](c6-on-legacy-ws281x-driver.md) | retired | 2026-07-31 | lp-fw/fw-esp32c6/src/output | C6 ran its own single-channel WS281x driver, not `lp-ws281x`; retired 2026-08-01 when the C6 moved onto the shared core and gained its second channel |
| [example-shaders-not-compile-gated](example-shaders-not-compile-gated.md) | carried | 2026-07-29 | examples GLSL + CI + lps-filetests | an example shader can compile on the host yet fail on 4 of 5 targets; the break surfaces only when a human opens it in Studio |
| [safe-mode-dim-boot-unproven](safe-mode-dim-boot-unproven.md) | retired | 2026-08-01 | fw-esp32c6/bootctl + lpc-engine safe clamp | RETIRED 2026-08-01: dim boot seen on silicon (serial: record found/consumed/clamped 26/255; heartbeat outputClamp; eyes: dim) |
| [studio-no-reconnect-after-replug](studio-no-reconnect-after-replug.md) | carried | 2026-07-31 | lpa-link/browser-serial + studio device cards | every bootloader-mode op ends on a replug Studio cannot see; the op card waits forever and the user reloads the tab |
