---
status: fixed
found: 2026-10-06      # how: e2e (PR B's LAN tuning runs: `lp-cli link rtt lan:` on an emulated C6), then silicon (G1 desk numbers)
fixed: this change
area: lp-link `Inbox::push_fragment` × fw-esp32-common `server_payload::request_refusal` × lpc-wire `serde_base64` × lpa-server fs reads
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

On silicon (fixture-c6, m6-split `128aea9ac`), with USB, LAN and Bluetooth
links open and a 10,240 B file written and read back over the LAN:

    [RECOVERY] last run crashed (oom): alloc 10242 bytes failed (align 1)

(10,242 is the base64 decoder's output estimate for the 13,656-character
blob: the decoded write, the third allocation below.)

**Root cause** — two infallible allocations on a request's way in, each
sized by the request rather than by the heap:

1. **Reassembly (fixed).** lp-link's inbox grew a message's buffer with
   `reserve_exact`, doubling up to `max_message` (16 KiB), so a message
   whose next doubling did not fit the largest block aborted the
   program.
2. **The text copy.** `decode_client_payload` parses the reassembled JSON
   with `serde_json`, and `serde_base64::deserialize_smart` built a
   `String` of the base64 text before decoding it, so an 8 KB write needed
   a ~13.6 KB block a second time.
3. **The decoded blob.** One block of 3/4 of the text, sized by the
   request. Nothing checked the heap first.

A file read has the twin on the way out: the whole file in one block.
Uploads pass because their chunks are about 5.5 KB. The project-read gate
guards project reads, but nothing guarded a request or an fs read.

**Fix** —

- **Reassembly:** growth tries the doubled size, then exactly what is
  needed (`try_reserve_exact`). A message neither fits is dropped to its
  end and counted as oversize, and the session carries on.
- **No text copy:** the base64 deserializers decode the text serde_json
  lends (a visitor on the borrowed `&str`), not a `String` of it.
- **A request gate:** before decoding a request of 2 KB or more, every
  transport (USB, the classic's UART, the radio mux) asks the chip's heap
  (`set_request_headroom_probe`, all three chips) for a block of 3/4 of it
  plus 1 KiB, and 16 KiB free beyond it. Short of that it answers the
  request's id from the link's own send ring — `request refused: board
  memory busy … retry shortly or send it in smaller pieces` — and drops it.
- **An fs-read gate:** a file read whose file needs more than the largest
  block (its size plus 512 B) answers `read refused: board memory busy`
  before reading.

Not done: streaming fs transfers in small pieces (no contiguous blob at
all). The gates make an oversized transfer a refusal rather than a reset;
a smaller one still needs its block.

**Regression coverage** — `lp_link::inbox::tests::a_message_the_heap_cannot_hold_is_dropped_not_fatal`;
`server_payload::request_gate_tests::a_request_the_heap_cannot_decode_is_refused_in_words`;
`lpa_server::handlers::tests::a_read_bigger_than_the_largest_block_is_refused_not_attempted`.
Emulated: PR B's `run_tune.sh` with `lp-cli link rtt`'s defaults (8 KB
writes, a 10 KB file) on `lp-emu:esp32c6:t1+net=lan`. The silicon re-check
(three links, the 10 KB file) is owed.

**Lesson** — on a board whose largest block is smaller than its largest
message, every allocation sized by a peer's input has to be fallible.
This one was found by a measurement tool's default, not by an attacker,
which is the same input.
