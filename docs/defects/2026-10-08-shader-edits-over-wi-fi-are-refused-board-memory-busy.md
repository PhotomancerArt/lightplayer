---
status: fixed
found: 2026-10-08      # how: report (Yona, a real C6 edited from Studio over Wi-Fi), reproduced e2e on the emulated C6
fixed: 2026-10-08, #1047 (the gate) and #1057 (the wire half, wire 41)
area: fw-esp32-common `server_payload::request_refusal` × lpc-model `AssetBodyOverlay::ReplaceBody` (Studio's shader edit) × the C6 heap over Wi-Fi
class: assumed-context
related:
  - docs/defects/2026-10-06-a-message-the-heap-cannot-reassemble-resets-the-board.md
  - docs/defects/2026-10-08-a-lan-request-past-8-kb-goes-unanswered-on-a-fragmented-heap.md
  - docs/defects/2026-10-08-a-recompile-on-a-fragmented-wi-fi-heap-resets-the-board.md
  - docs/defects/2026-09-27-fragmented-heap-refuses-every-read.md
  - lp2025/2026-09-27-1218-fragmentation-tolerant-reads (REPORT.md: what an edit leaves in the heap)
---
# Shader edits over Wi-Fi are refused "board memory busy"

**Symptom** — after a few recompiles from Studio over the Wi-Fi (LAN) link,
a real C6 refused the next edit, and every retry:

    request refused: board memory busy (free …, largest block 5216 B; a 7142 B
    request needs 23526 B free and a 6380 B block); retry shortly or send it in
    smaller pieces

Reproduced on the emulated C6 (`lp-cli/tests/emu_edit_frag.rs`, the
shipped image from CI at `3c1524500`, `lp-emu:esp32c6:t1+net=lan`): the
PLAYFUL choker deployed, then Studio's own edit growing one line at a time.
With a LAN client and a USB host both connected, edit 5 (a 7,904 B request)
was refused with the same words — free 41,724 B, largest block 6,184 B, a
6,948 B block asked for — and so was edit 6.

**Root cause** — the gate measured the wrong thing. It holds the heap's
largest block against 3/4 of the whole message plus 1 KiB: the rule for a
file write, whose message is one base64 string. Studio does not send a
shader as one. It applies an edit as an overlay mutation,
`SetArtifactBody { edit: ReplaceBody(Vec<u8>) }`, and that `Vec<u8>` is
serialized as a JSON array of byte values: ~3.5 characters a byte, so the
1,971 B choker shader is a ~7.1 KB request. Decoding it builds one
`Vec<u8>` of one byte an element, grown by doubling: a 2,048 B block. The
gate asked for 6,380 B, so a board that could decode the edit refused it.
Retrying could not help: nothing frees memory between retries.

Under it, the heap over Wi-Fi is smaller and more broken up than over USB.
Free after deploying the choker (emulated, same image):

| Links | Free | Largest block |
|---|---:|---:|
| USB only, Wi-Fi off | 81,548 B | 56,860 B |
| USB, Wi-Fi joined (no LAN client) | 67,448 B | 46,364 B |
| LAN client only | 61,724 B | 40,644 B |
| LAN client and a USB host | 54,276 B | 33,220 B |

Joining costs 14.1 KB, the secure LAN session 13.1 KB, a USB host's session
7.4 KB. The first edit then cuts the largest block to 13–16 KB while free
falls only ~2.3 KB: what a recompile keeps (the new JIT module, the
settings-list copy, the source copy — the 2026-09-27 census) lands in the
middle of the big free tail, and on Studio's path the overlay also keeps
the decoded body, allocated while the 7 KB request is still held. Over USB
with Wi-Fi off the same 24 edits never went below a 16.6 KB block and
nothing was refused.

**Fix** — the gate reads the block off the request's shape
(`serial::request_decode_block`): a string at 3/4 of its length (the old
rule, so a file write asks what it asked before), an array of byte values
at its element count rounded up to a power of two, any other array at its
own `Vec` (64 B an element) and never more than 3/4 of its text; the whole
never more than the old rule. The free-bytes condition is unchanged.
On the emulator, the same run on the fixed image takes edits 5 and 6 and
compiles them (no refusal, no OOM). Flash: +932 B (engine region; split
headroom gate 92,554 → 91,622 B).

**Not fixed here — the next walls, each filed:**

- Over the LAN, a request past ~8.1 KB (edit 7 of the choker, 8,210 B)
  is dropped in reassembly and never answered: the link stays up, the host
  waits for its deadline
  (`2026-10-08-a-lan-request-past-8-kb-goes-unanswered-on-a-fragmented-heap.md`).
- A recompile needs one ~6–7 KB block and ~30 KB of working room; on a
  fragmented heap it OOMs and resets the board
  (`2026-10-08-a-recompile-on-a-fragmented-wi-fi-heap-resets-the-board.md`).
- The byte-array encoding itself (a wire change, so not here): sending
  `ReplaceBody` as text would make the choker edit ~2.2 KB instead of
  ~7.1 KB. With text-sized requests (the same shader written as a file) the
  emulated board took 24 edits with no refusal and no unanswered request
  (it still reset three times in the compile, the entry above).
  It would also lift a latent limit: Studio allows a 10 KB body
  (`MAX_ASSET_BODY_BYTES`, sized for base64), but past ~4.7 KB a byte
  array outgrows the board's 16.6 KB message cap (computed, not walked).
  **Fixed by wire 41 — see "The wire half" below.**

**The wire half (wire 41, fixed 2026-10-08)** — the request is now about
the shader's own size. `AssetBodyOverlay::ReplaceBody` and
`WireCreateNodeRequest`'s `body` and `assets` go as the body's text when it
is UTF-8, else as `{"base64":"…"}` (`lpc_model::body_bytes`; one encoding
per body, so a string is always the text itself). `WIRE_PROTO_VERSION`
40 → 41. Pinned in `lpc-wire`'s
`a_shader_edit_request_is_about_the_size_of_its_source`:

| Shader | Source | Request before | Request after |
|---|---:|---:|---:|
| PLAYFUL choker | 1,971 B | 7,134 B | 2,239 B |
| `projects/test/basic` | 4,365 B | 15,183 B | 4,709 B |

Studio's limit is now the body **as the wire carries it**
(`MAX_ASSET_BODY_BYTES` = `lpc_wire::budget::MAX_ASSET_BODY_ENCODED_BYTES`
= 16,384 B frame budget − 1,024 B envelope = 15,360 B; a shader is its own
size plus a byte a line), under the board's 16,656 B request buffer and
inside one overlay-read reply. The request gate's string rule follows how
`serde_json` decodes a string: escaped text needs under twice its length
(its scratch unescape), base64 3/4, plain text its length; the byte-array
rule left with the byte arrays.

Emulated, `lp-cli/tests/emu_edit_frag.rs` on an image built from that
change (`lp-emu:esp32c6:t1+net=lan`, 24 growing edits of the choker,
requests 2,292 → 3,412 B): **no refusal and no unanswered request** in
either configuration. With a LAN client alone, edits 1–14 compiled, and
edit 15 reset the board in the compile (`requested=7132 … largest_free=7120
… context=shader node: compile`), as did 19 and 21. With a LAN client and a
USB host (the configuration that refused at edit 5), edits 1–12 compiled,
and 13 and 15 reset it the same way (`requested=6580 … largest_free=5344`).
Those resets are the compile entry above, still open.

**Regression coverage** — `request_decode_block::tests` (a byte-array edit
needs its count rounded up; a file write the old rule; other arrays their
own span), `server_payload::request_gate_tests::a_shader_edit_is_measured_by_its_bytes_not_its_text`
(the 2026-10-08 figures). Emulated: `lp-cli/tests/emu_edit_frag.rs`
(`#[ignore]`d, not in CI; `LP_EDIT_FRAG_HOST_LINK=1` for the refusing
configuration). The silicon re-check (Studio over Wi-Fi, the choker,
edits until something says no) is owed.

**Lesson** — a gate that estimates a request's cost has to estimate it
from the request's shape, not from the one request shape it was written
for. The base64 rule was right for the request that found the first
defect and wrong by 3× for the request users send most.
