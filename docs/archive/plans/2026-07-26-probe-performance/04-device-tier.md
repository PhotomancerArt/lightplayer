# P4 — Device tier: pacing floor, 16×16 probes, sRGB LUT, receive latency

Size: sm. Depends on: P3 (pacing model).

## Scope

Apply the new pacing model to the device runtime and cut per-probe device
cost:

1. Device refresh gap: 750 ms → completion-based with a 150 ms floor
   (constant, tuned at Yona's hardware walk).
2. Device probe resolution tier: 16×16 (sim stays 32×32).
3. Replace per-pixel `powf` sRGB encode with a LUT on the engine side.
4. Reduce the host-side 10 ms/frame receive-poll latency for device serial.

Out of scope: firmware io_task/transport changes (stop-and-wait, 64 B read
buffer, framing) — recorded as future work; probe revision-gating.

## Current state

- `lp-app/lpa-studio-core/src/app/studio/refresh_cadence.rs:31` —
  `DEVICE_REFRESH_INTERVAL = 750ms` (post-P3: a gap, stamped at completion).
- Probe size constant: `UiProductPreviewFrame::VISUAL_DEFAULT = 32×32`
  (`lp-app/lpa-studio-core/src/app/node/ui_produced_product.rs:48-50`), used
  once for real probes at
  `lp-app/lpa-studio-core/src/app/project/project_sync.rs:23,405-407`. The
  wire request (`RenderProductProbeRequest`) already carries width/height;
  the engine renders natively at the requested size
  (`lp-core/lpc-engine/src/engine/project_read_probes.rs:32-40`).
- sRGB encode: `project_read_probes.rs:54-57,211-229` —
  `rgba16_linear_to_srgb8` does per-channel `libm::powf(c, 1/2.4)` (3072
  calls per 32×32 frame) on-device. Code is `no_std`.
- Host receive poll: `lp-app/lpa-link/src/device_session/
  device_session.rs:496-509` (`recv_frame`) pumps then sleeps
  `READINESS_POLL_INTERVAL = 10ms` (`device_timers.rs:38`) — ≥10 ms per
  received frame, so a multi-frame read is paced at ~10 ms/frame.

## Implementation

1. **Pacing floor.** `DEVICE_REFRESH_GAP = 150ms` (rename per P3
   convention). One constant change + doc comment. Note in the PR body that
   the hardware walk may tune it.
2. **Probe tier.** `ProjectSync` (or its caller) must know the session's
   runtime kind — `RefreshCadence::for_kind` (`refresh_cadence.rs:93-98`)
   shows how kind reaches studio-core policy. Add
   `UiProductPreviewFrame::VISUAL_DEVICE = Self::new(16, 16)` and select by
   kind where `VISUAL_PRODUCT_PREVIEW_FRAME` is used
   (`project_sync.rs:405-407`). Keep the frame a per-request value — this is
   the seam the future display-driven sizing (follow-on) will use.
   The 16×16 Srgb8 payload (768 B raw) still fits unchunked.
   Check: `lp-cli/src/debug_ui/ui.rs:401` also hard-codes 32×32 — leave the
   CLI as-is (not a studio surface).
3. **sRGB LUT.** In `project_read_probes.rs`, replace the per-channel
   `powf` with a lookup. Input is 16-bit linear; a full 65536-entry u8 table
   is 64 KiB — too big for ESP32 flash budget appetite. Use a smaller table
   + interpolation or a piecewise-index scheme (e.g. 4096-entry table
   indexed by the top 12 bits, or exact sRGB piecewise curve with the
   linear-segment special-case). Requirements:
   - `no_std`, const-buildable or lazily-init-free (prefer a
     `const fn`-generated or include-baked table; no `once_cell` on
     firmware paths without checking existing patterns).
   - Accuracy: add a test comparing LUT output to the existing `powf` path
     across the full u16 domain; require max error ≤ 1 LSB of u8 output.
   - Keep the existing function signature; swap the internals.
   - Note flash delta in the phase result (memory: ESP32 flash margin is
     tracked; a 4 KiB table is fine, 64 KiB is not).
4. **Receive latency.** In `device_session.rs` `recv_frame`
   (`:496-509`): poll the wire for already-buffered frames *before*
   sleeping (mirror P3's drain-then-wait shape), and/or drop the in-stream
   poll interval (e.g. 2 ms while a request is in flight). Web Serial reads
   are pumped by `pump_console_lines` — check whether a smaller sleep
   meaningfully burns CPU; pick the simplest change that removes the
   10 ms-per-frame tax and document the choice. Do not touch firmware.

## Conventions

- `lpc-engine` is `no_std` + on-device: no allocation in the hot path, no
  new deps without checking firmware feature unification
  (memory: workspace feature unification affects shader frontends).
- Budget constants live in `lpc-wire/src/budget.rs` — no changes needed;
  do not re-derive them.

## Validation

- `cargo test -p lpc-engine` (new LUT accuracy test), `-p lpa-studio-core`.
- `just check build-ci` — build-ci includes firmware targets; confirms
  `no_std` cleanliness.
- Flash-size check if the fw image is affected (LUT lives in lpc-engine and
  links into firmware): note delta via existing size tooling if available.

## Agent reminders

Do not commit unless asked. Do not expand scope. Do not suppress warnings or
disable tests. Stop and report if blocked. Report changes, validation, and
deviations.

ADR: probe tier + pacing covered by P6 ADR.
Review gate: **Yona hardware walk** — batched at PR review; PR body must list
the walk steps (connect device, focus a shader node, observe preview cadence
and UI feel; compare 16×16 legibility).

## Definition of done

Device gap 150 ms completion-based; device probes 16×16; LUT test proves
≤1 LSB error vs powf; multi-frame device reads no longer pay 10 ms/frame;
checks green.

## Implementation Result

Status: done
Completed: 2026-07-27
Commit: e287c3d5d

- Changed: `DEVICE_REFRESH_INTERVAL` 750 ms → 150 ms (gap semantics);
  `UiProductPreviewFrame::VISUAL_DEVICE` (16×16) pushed through
  `ProjectSync::set_visual_preview_frame` — no protocol change (request
  already carries width/height); `srgb8_lut.rs` 4096-entry LUT replaces
  3072 `libm::powf`/frame; in-stream device receive polls at
  `WIRE_FRAME_POLL_INTERVAL = 2 ms` only while a response is pending.
  Firmware untouched.
- Validated: `srgb8_lut_matches_float_reference_within_one_lsb` (all 65536
  inputs, ≤1 LSB); ESP32 clippy in `just check` proves `no_std` fit;
  `just test` green.
- Deviations: none. Hardware retune of the 150 ms floor is the batched PR
  gate. Details in [handoff.md](handoff.md).
