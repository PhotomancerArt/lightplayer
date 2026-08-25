---
status: open
found: 2026-08-24      # how: hardware-walk (dig2go bench, gate-transition logs)
area: fw-esp32-common output/power_gate (is_all_black) + staged frame layout
class: assumed-context
related:
  - 2026-08-21-hello-gate-assumes-fresh-boot.md
  - ../adr/2026-08-08-switched-power-rail-mechanism.md
  - 2026-08-07-2336-dig2go-board-support (plan dir; notes.md 2026-08-24 section)
---
# The power gate's all-black scan counts the alpha channel

**Symptom** — During the dig2go debounce walk (2026-08-24), a test shader
returning `vec4(0.0, 0.0, 0.0, 1.0)` — conventional opaque black — kept
re-asserting the LED power rail (`[power-gate] assert /gpio/12` logged
right after its compile), so the rail never dropped despite every visible
pixel being off. Changing only the alpha to `0.0` (`vec4(0.0)`) made the
identical content scan as black: the rail deasserted on schedule and
stayed down through compile and playback. Clean A/B on silicon.

**Root cause (mechanism to pin in code)** — `is_all_black` scans the
staged `&[u16]` slice under the documented assumption that the data is
"post-gamma, post-brightness, so all-black here is exactly the 'nothing
is lit' the gate wants" (comment at the call site in
`Esp32OutputProvider::write`). The bench shows the slice still carries a
non-emitting channel: an alpha of 1.0 reads as non-zero and defeats the
scan. The assumption "every non-zero word is light" is false for the
actual staged layout.

**Why it matters** — opaque black is the CONVENTIONAL way to write black
in a shader (every example in this repo ends `..., 1.0)`). Any project
whose dark scenes are authored that way keeps the rail energized
forever, silently converting the gate's power-down feature into a no-op
on exactly the boards that shipped it.

**Fix direction** — make the scan see only emitting channels: either
scan post-format-packing (the bytes the wire actually clocks out), or
mask the alpha component when the staged layout carries one. A unit test
should pin `vec4(0,0,0,1)`-shaped frames as black.

**Regression coverage** — none yet; `is_all_black` unit tests exercise
synthetic slices without an alpha-carrying layout.

**Lesson** — "this buffer is exactly the light" is a layout claim, not a
semantics claim, and it rots when the layout gains a channel. A scan
that stands in for "is anything visibly lit" should be defined against
the emitted format, not against whatever slice happens to be handy at
the call site.
