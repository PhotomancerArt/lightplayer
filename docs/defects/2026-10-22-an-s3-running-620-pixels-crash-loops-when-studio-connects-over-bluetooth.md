---
status: open
found: 2026-10-22      # how: hardware-walk (FS3, the S3 updates plan's M7 P12a sitting, Studio in Mac Chrome as the central)
area: lp-fw/fw-esp32s3 heap (Bluetooth on, a 620-pixel project) × Studio's Bluetooth session
class: budget-exhaustion
related:
  - docs/defects/2026-10-22-the-s3-truncates-ws281x-frames-at-620-px-with-bluetooth-on.md
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M7 P12a; M9's 620-pixel work)
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

**Cause (not found)** — the heap is near its end once the Bluetooth link is
up with a 620-pixel project running: about 220 KB used of the S3's arena. A
10 KB allocation on the link's first requests then fails. Studio's card asks
for the board's access list, project and picture over the link, and the
running engine's update session builds its manifest. Which of these
allocates the 10,240 bytes is not yet known; the panic carries no frame.

**What it is not** — the over-the-air path itself. The same board took
X → Y over **USB** in 26.4 s, with the update light lit, on the same
sitting. That run used an `lp-cli` engine cache, so no read-back of the
running engine was needed.

**Next** — reproduce with the panic's allocation named (the S3's
`heap_peak_diag`, or a `[MEM]` line per link request), on the emulated S3
with the same project first (emulator first). Then ask the director whether
it is M9's (620 pixels) or the radio's.
