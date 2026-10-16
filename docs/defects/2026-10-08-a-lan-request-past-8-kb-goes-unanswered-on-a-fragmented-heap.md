---
status: open
found: 2026-10-08      # how: e2e (lp-cli/tests/emu_edit_frag.rs on the emulated C6, net=lan)
area: lp-link `Inbox::grow_partial` × fw-esp32-common LAN link (`lan_link_config`, 1 KiB frames)
class: silent-drop
related:
  - docs/defects/2026-10-08-shader-edits-over-wi-fi-are-refused-board-memory-busy.md
  - docs/defects/2026-10-06-a-message-the-heap-cannot-reassemble-resets-the-board.md
---
# A LAN request past ~8 KB goes unanswered on a fragmented heap

**Symptom** — Studio's shader edits of the PLAYFUL choker over the
emulated C6's LAN link (`lp-emu:esp32c6:t1+net=lan`, the shipped image
from CI at `3c1524500`) stop being answered once the request passes
~8.1–9 KB (edit 7, 8,210 B, with a USB host also connected; edit 12,
8,985 B, with the LAN client alone). Nothing is logged, no refusal comes
back, the link stays up, and small requests on it are still answered. A
fresh link (a reconnect) does not help: every later, longer edit is
dropped the same way. The heap at the time: ~49–56 KB free, largest block
~13.5 KB.

**Root cause** — reassembly needs two large blocks at once. A LAN frame
carries ~1 KB, and lp-link's inbox grows a message's buffer by doubling
(1 K, 2 K, 4 K, 8 K). Past 8 KB the next growth asks for the doubled size,
then for exactly what is needed — a `Vec` reallocation, so the 8 KB buffer
and the new one are both alive. With one ~13.5 KB hole and nothing else
over ~8 KB, neither fits, and the 2026-10-06 fix drops the message to its
end as "oversize" rather than abort. The drop is counted but never
answered: the inbox has dropped the bytes that carried the request's id,
so the transport has no one to refuse.

**Fix** — none yet. Options, cheapest first:

- Shrink the requests (Studio's shader edit as text, ~3.4× smaller: a wire
  change; see the related entry).
- Answer the drop: keep the first ~64 bytes of a message the inbox drops
  for want of heap, hand them to the transport as an event, and refuse the
  request's id "board memory busy" as the decode gate does. A user then
  sees words instead of a hang.
- Reassemble a long message in fragment-sized pieces and copy it once
  into one exact block at its end: one block of the message's size instead
  of the old buffer and the new one together. That moves the cliff from
  ~8 KB to the largest block (~13.5 KB here).

**Regression coverage** — none yet; `lp-cli/tests/emu_edit_frag.rs`
reproduces it (`#[ignore]`d), reporting `NO ANSWER in 20s` per edit.

**Lesson** — a fallible allocation that turns an OOM into a drop still
needs someone to tell. The 2026-10-06 fix made the reassembly survive; it
did not make the request end.

## 2026-10-10: on silicon, over Bluetooth, after the reassembly fix

The third option above landed the same day (`942f0e237`, "a message past
8 KB reassembles on a fragmented heap": fragments spill into pieces and
are copied once into one exact block at the end), and this entry was not
updated. It moved the cliff where it said it would — to the largest
block — and the drop is still unanswered there.

RAM research E17 (`lp2025/2026-10-09-1203-ram-research`, CX1
`14:C1:9F:E6:54:90`, a research image of `research/ram-e17` with the BLE
controller's `acl_buf_count` at 12, meteor running, a Mac Chrome central
over `spikes/ble-lab`'s pipe): a 10,000 B `FsRequest::Write` (a ~13.4 KB
message) over the Bluetooth link was never answered, twice in two runs.
The host sent it and waited out its 190 s; nothing was logged, no
refusal came back, the link stayed up with 0 resends, and 3 KB writes on
the same link and image were answered. Every heartbeat while it waited
said `largestFreeBlock: 10888` (42,832 before the link). The same 10 KB
writes were answered on two other images whose largest block dipped only
to 12.7–13.5 KB. Nothing showed the knob dropping
anything itself; what the heartbeats show is the block it left. The evidence is in the program's
`experiments/e17-ble-central-sitting/evidence/step4/acl12-w{a,b}/`.

So the second option — answering the drop — is what is left: on any
link, a request larger than the largest block still ends in silence.
