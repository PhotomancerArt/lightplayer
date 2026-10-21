---
status: open
found: 2026-10-20      # how: the drawing plan's emulated C6 measurement (P4): projects/test/fluid never lit; LP_WDT reset during its first frame
area: lpc-model `SlotOptionReader::get_or` through lpc-engine's `TickContext`, on rv32; the Fluid node's `coords` read
class: stand-in-divergence
related:
  - docs/adr/2026-09-24-pattern-space.md
---
# An absent option read through `get_or` hung the emulated C6

**Symptom** — with the Fluid node's new optional `coords` key, a fluid that
does not author it (`projects/test/fluid`, every fluid before 2026-10-20)
hung the emulated C6 in its first frame until the watchdog reset the chip
(`LP_WDT stage 0`). The same project with `"coords": "pixels"` written out,
or `"pattern"`, ran (39 fps). On the host the same read returned the default
and every test passed, including the pixels-mode byte-identity test.

**Workaround (shipped with the key)** — the Fluid node reads `coords.some`
through `TickContext::resolve_static_consumed` and maps the typed
`is_absent_option` error to Pixels, the way the fixture reads its absent
`power` (`fluid_node.rs`, `read_coords`). The fixture's own `get_or` reads
(`brightness`, `gamma_correction`) are present in every shipped fixture, so
they never took the absent path on a board.

**Still open** — why `get_or` on an absent option hangs on rv32 and not on
the host. Until that is known, an absent optional def slot read through a
view's `get_or` is a trap on the board: read absent options by their `some`
path. Reproduce: revert `read_coords` to
`def.coords().get_or(ctx, ShaderCoords::Pixels)?`, build the C6 image, and
`lp-cli emu run --merged <merged.bin> --host-link --upload projects/test/fluid
--time-grade t2 --timeout 15s`.
