---
status: open
found: 2026-10-22      # how: hardware-walk (FS3, the S3 updates plan's M7 P12a sitting, Studio in Mac Chrome as the central)
area: lp-core/lpc-engine output frame geometry (merge_fragment_display_layouts) × lp-fw/fw-esp32s3 heap (Bluetooth on, 620 pixels, no PSRAM)
class: budget-exhaustion
related:
  - docs/defects/2026-10-22-the-s3-truncates-ws281x-frames-at-620-px-with-bluetooth-on.md
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M7 P12a; M9's 620-pixel work)
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M10, PSRAM: #1264)
---
# An S3 running 620 pixels crash-loops when Studio connects over Bluetooth

**Symptom** — FS3 (`fixture-s3`, XIAO ESP32-S3 Plus, 16 MB, antenna attached,
D8:3B:DA:47:29:70) ran the split image with lane L's "Skirt 620" project
(620 pixels, four outputs) loaded from `lpfs`. When Studio, a release bundle
in Mac Chrome as the Web Bluetooth central, joined it over Bluetooth, the
board panicked over and over:

```
[ble] link1: connected (slot 0), heap used 213544 B
[ble] link1: notifications on — link open (ATT MTU 247, frames 180 B + 8, serving, link RAM 7208 B, heap used 220764 B)
…heartbeat… "loaded_projects":[{"handle":1,"path":"/projects/Skirt 620"}]
====================== PANIC ======================
memory allocation of 10240 bytes failed
[RECOVERY] last run crashed (panic): at <no frame>: memory allocation of 10240 bytes failed
```

That was 8 panics in 60 s with nothing pressed (the card only showing the
board), and 6 in 45 s on **main's own release image** (`2026.10.22-8`,
c7604ae63) once it was reflashed, so this is not the update board side's.
Studio's card flipped between "connected" (Access "checking", Update offered)
and offline. With no central, the same image and project ran clean: 0 panics
in 30 s, 585 frames. An update pressed over Bluetooth never got going.

**Cause** — Studio's card read, not Bluetooth itself. The 10,240 bytes are
the merged lamp list of one output's frame geometry:
`merge_fragment_display_layouts` (`lpc-engine`, `output_node.rs`), reached
through `Engine::read_project_output_frame_probe` on the card's first
`projectRead` with an `output_frame` probe (`geometry: "always"`). It builds
a `Vec<ControlLamp2d>` (20 B a lamp) by pushing, so for one of Skirt 620's
310-lamp outputs the 257th lamp doubles it from 256 to 512 places: 10,240 B
asked for while the old 5,120 B is still held. The read has already cloned
the producer's layout (6,200 B) beside the producer's own (6,200 B), and
holds the first output's finished geometry (10,240 B) while it builds the
second. The emulated S3 (`lp-emu:esp32s3:t1`, at `2955aa3fe`) reproduces it
with no Bluetooth central at all: Skirt 620 uploaded, then `accessList`,
`listLoadedProjects` and the card's read over USB, with Bluetooth up and
advertising. The last heartbeat before the read said `[MEM] free=62412
used=216132 largest_free=50172`, and the first read panicked:

```
memory allocation of 10240 bytes failed
  … handle_alloc_error ← RawVec::grow_one
  ← lpc_engine::nodes::output::output_node::merge_fragment_display_layouts
  ← lpc_engine::engine::Engine::read_project_output_frame_probe
  ← stream_project_read_events (the link's project-read stream)
```

What else the read held at that moment was not measured, so how 62 KB free
left no 10 KB block is not known exactly. Bluetooth matters only because it
is what leaves the heap this low: its controller and host take about 37 KB
at boot (`[ble] up: controller +33400 B, host + GATT +3520 B`), and a
central's link takes about 7 KB more (`link RAM 7208 B`). On silicon with no
central, nothing asked for the card's picture, so nothing built the
geometry.

**Not fixed here: pre-PSRAM, expected fixed by M10.** Yona, 2026-10-22: the
620-pixel case on an S3 with no PSRAM does not matter. The S3 plan's M10
(PSRAM, #1264) gives the heap the room. This entry closes when M10's image
is the default and Studio over Bluetooth runs Skirt 620 for 5 minutes with 0
panics on FS3.

**The read is still wasteful, at any heap.** Per 310-lamp output, merging
the geometry peaks at about 27,760 B (the producer's layout, its clone, and
the old and new lists while it doubles) where 12,400 B would do, and it
holds 10,240 B where 6,200 B is enough. A finished change borrows the
producer's layout, reserves the exact lamp count with `try_reserve_exact`,
and answers `Omitted` when the heap cannot hold it, so the client asks
again. It is on the branch `claude/display-layout-exact-merge` (55704c89e,
with a test), filed as a follow-up and not merged.

**What it is not** — the over-the-air path itself. The same board took
X → Y over **USB** in 26.4 s, with the update light lit, on the same
sitting. That run used an `lp-cli` engine cache, so no read-back of the
running engine was needed.
