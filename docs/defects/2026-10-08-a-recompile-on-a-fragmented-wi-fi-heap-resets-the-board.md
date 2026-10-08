---
status: open
found: 2026-10-08      # how: e2e (lp-cli/tests/emu_edit_frag.rs on the emulated C6, net=lan)
area: lpc-engine `ShaderNode::ensure_compiled` × lps-frontend / lpvm-native compile allocations × the C6 heap over Wi-Fi
class: budget-exhaustion
related:
  - docs/defects/2026-10-08-shader-edits-over-wi-fi-are-refused-board-memory-busy.md
  - docs/defects/2026-08-29-shader-jit-compile-transient-starves-classic-heap.md
  - lp2025/2026-09-27-1218-fragmentation-tolerant-reads (REPORT.md: "The edit cycle")
---
# A recompile on a fragmented Wi-Fi heap resets the board

**Symptom** — editing the PLAYFUL choker's shader over the emulated C6's
LAN link (`lp-emu:esp32c6:t1+net=lan`, the shipped image from CI at
`3c1524500`, a USB host also connected), with each edit a line longer,
the board reset three times in 24 edits (edits 12, 16 and 18):

    allocation failed: requested=6028 align=1 free=22760 used=278776 largest_free=5568 retry_ok=false context=shader node: compile
    [OOM] FRAGMENTED: 22760 B free in total but only 5568 B in one piece

    allocation failed: requested=7132 align=1 free=19768 used=281768 largest_free=5104 retry_ok=false context=shader node: compile

Between edits the same board had ~53 KB free with a ~13.5 KB largest
block. Two of four LAN-client-only runs also reset mid-way (the console
was not kept; same configuration otherwise).

**Root cause** — a compile needs one contiguous block of ~2.4–3× the
shader source (5,904 B for the 1,971 B choker in the 2026-09-27 census:
the lexer's tokens; 6,028 and 7,132 B here for 2.5–2.8 KB) on top of a
~20–30 KB working set, and every one of those allocations is infallible.
Over Wi-Fi the heap starts ~27 KB smaller (joined 14.1 KB, the secure LAN
session 13.1 KB) and each recompile leaves its kept objects in the big
free tail, so the free tail is in pieces smaller than the token block. The
recovery ledger records the OOM and the board comes back, but the edit is
lost and the link drops.

**Fix** — none yet. Options:

- Make the compile's big asks small: stream or chunk the token list
  (`lps-frontend`), so no compile needs a block proportional to its source.
- Place what a compile keeps out of the free tail (the JIT module, the
  settings-list copy, the source copy): a fixed code region for the JIT, or
  allocate the kept objects after the compile's scratch is freed (the
  2026-09-27 plan's P3).
- Check before compiling: with the heap short of the compile's block,
  keep the last good shader and say "board memory busy" on the node,
  rather than reset.
- Give the Wi-Fi side back memory (the LAN session's buffers between
  requests).

The on-device compiler stays on the device; none of these moves it.

**Regression coverage** — none yet; `LP_EDIT_FRAG_VIA=fs
LP_EDIT_FRAG_HOST_LINK=1` on `lp-cli/tests/emu_edit_frag.rs` reproduces it
(`#[ignore]`d), the board's console beside it
(`LP_EDIT_FRAG_CONSOLE=<file>` writes `<file>.board`).

**Lesson** — the request gate and the reassembly guard protect the bytes
on their way in; the compile that follows is the larger ask, and it is
still an abort.
