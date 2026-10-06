---
status: open           # reassembly fixed; the decode half is open
found: 2026-10-06      # how: e2e (PR B's LAN tuning runs: `lp-cli link rtt lan:` on an emulated C6)
fixed: this change (reassembly only)
area: lp-link `Inbox::push_fragment` (fixed) × fw-esp32-common `decode_client_payload` / lpc-wire `serde_base64` (open)
class: budget-exhaustion
related:
  - docs/defects/2026-10-06-a-lan-link-strands-the-heap-below-the-load-floor.md
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989)
---
# A request longer than the board's largest free block resets the board

**Symptom** — with a project loaded and a LAN link open, `lp-cli link rtt
lan:…` reset the emulated C6 during its write transfers (8 KB chunks by
default), with no request failing first:

    allocation failed: requested=16384 align=1 free=58788 used=242748 largest_free=13448 … context=engine: tick
    [OOM] FRAGMENTED: 58788 B free in total but only 13448 B in one piece

After the first fix, the same run reset one step later:

    allocation failed: requested=13656 align=1 free=50536 used=251000 largest_free=13448 …

**Root cause** — two infallible allocations on a request's way in, each
sized by the request rather than by the heap:

1. **Reassembly (fixed).** lp-link's inbox grew a message's buffer with
   `reserve_exact`, doubling up to `max_message` (16 KiB), so a message
   whose next doubling did not fit the largest block aborted the
   program.
2. **Decode (open).** `decode_client_payload` parses the reassembled
   JSON with `serde_json`, which copies a base64 string out
   (`StringVisitor`, `serde_base64::deserialize_smart`) before decoding
   it, so an 8 KB write chunk needs a ~13.6 KB block a second time.

Uploads pass because their chunks are about 5.5 KB. The read gate guards
replies, but nothing guards a request.

**Fix (reassembly)** — growth tries the doubled size, then exactly what is
needed (`try_reserve_exact`). A message neither fits is dropped to its end
and counted as oversize, like a too-long one, and the session carries on.
The host's request then times out instead of the board resetting.

**Open** — the decode half. The shapes, for a plan to pick: bound a
request's size by the largest free block before decoding it (refuse
"board memory busy", as reads are), decode base64 in place without the
string copy, or both. Until then, keep host writes at or below an upload
chunk.

**Regression coverage** — `lp_link::inbox::tests::a_message_the_heap_cannot_hold_is_dropped_not_fatal`
(a test hook stands in for a heap that cannot grow). The decode half has
none.

**Lesson** — on a board whose largest block is smaller than its largest
message, every allocation sized by a peer's input has to be fallible.
This one was found by a measurement tool's default, not by an attacker,
which is the same input.
