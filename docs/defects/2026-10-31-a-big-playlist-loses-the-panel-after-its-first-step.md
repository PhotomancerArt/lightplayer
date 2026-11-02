---
status: fixed          # firmware: the board must take the release with this change (over the air is fine)
found: 2026-10-31      # how: report (Yona at a festival, the "flower" XIAO ESP32-C6 over Bluetooth from his phone)
fixed: this change
area: lpc-engine project read (slot roots) × lpc-shared ProjectReadStreamSink frame budget × lpc-view applier
class: uptime-dependent-size
related:
  - 2026-09-27-fragmented-heap-refuses-every-read.md
  - 2026-10-06-a-wifi-joined-c6-refuses-every-project-switch.md   # read frames cut to half the largest block
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
---
# A big playlist loses the card's panel once it has stepped

**Symptom** — on the "flower" C6 (the PLAYFUL Choker Tryout: one playlist
of 25 patterns, each playing 30 s), Yona connected over Bluetooth and some
or all of the card's panel controls were missing. Brightness was missing
most often; sometimes every control was. Resetting the board usually fixed
it; reconnecting never did.

**Root cause** — one slot root outgrew the read frame, and the board's age
decided when.

*The root.* A ProjectRead sends each node's slots as one `SlotRoot` event,
and an event must fit one frame: 16 KiB on every link, half the heap's
largest free block when that is less (`lpa_server::read_frame_budget`). The
tryout's playlist def is the biggest root in the project: 15,568 B of wire
JSON at load, every field of 25 entries carrying a revision stamp
(`changed_at`, `presence_revision`, `keys_revision`).

*The stamps.* When the playlist steps, the engine re-derives its def
(`ProjectRegistry::set_entry_resident`), and every stamp in it becomes the
current revision. Revisions count frames, so a stamp is `0` at load, three
digits a few seconds later, four after a minute or so, and one digit more
each tenfold of uptime. Each digit is ~240 B on this root: 16,285 B after
the first step on a host server, and over 16 KiB on the emulated C6 after
two minutes.

*The read.* An event too big for its frame ends the read with a terminal
error. Studio's staged first sync reads all of a project's slots in one
page, so the page failed, and the client applies a node query only at its
end: every slot root of the read was dropped, not just the playlist's.
Brightness lives in the fixture's def, which no later read sends again (a
delta read only sends roots that changed), so it never came back. The
controls that did show came from roots that changed after the connect. And
once the playlist stepped past the skeleton's revision, every delta read
carried its def too and failed.

A reset brings the stamps back to `0` until the first step, so it usually
helped if the connect came soon after. A reconnect read the same oversized
root again.

On the emulated C6 (`lp-emu:esp32c6:t1`, shipped image of `da1b5c5b6`,
Bluetooth on, no central) the first sync passed at 60 s and failed at
120 s and 180 s with `project-read event exceeded frame budget of 16384
bytes`, with 127 KB free and a 65.5 KB largest block: fragmentation had no
part in it, though on a fragmented heap the halved frame makes it fail
sooner.

**Fix** — a slot root whose event does not fit the read's frame now
travels in pieces: `SlotRootBegin { name, shape, byte_length }`, then
`SlotRootBytes { offset, bytes }` (consecutive slices of the root data's
wire JSON, each sized so its base64 fits the frame), then `SlotRootEnd`
(`lpc_engine`'s `stream_slot_root`; the frame is
`ProjectReadEventSink::frame_budget`, the sink's own, halved on a
fragmented heap). Each piece serializes the root again into its window
(`lpc_wire::ser_write_json_slice`), so the board never allocates the whole
encoding. The client joins the pieces and decodes the same
`WireSlotRootSnapshot` (`lpc-view`'s `NodeState`). A root that fits its
frame is sent whole, byte for byte as before.

No `WIRE_PROTO_VERSION` bump: the new events are additive, and a board
sends them only where it used to end the read with an error. A Studio
older than the board sees an unknown event there instead of that error.
Studio and the firmware release deploy together.

**Regression coverage** —
`lpa-studio-core`'s `studio_big_slot_root_e2e_tests` (the tryout with a
50-entry playlist on a real server: the card's panel leads with the master
and the Pattern instrument lists every entry, on 16 KiB frames and on the
8 KiB frames of a fragmented heap; both fail without the fix),
`lpc-view`'s piece tests (joined like a whole root; a gap, or another
event inside the pieces, is a protocol error), and
`lp-cli/tests/emu_panel_reads.rs` (the shipped image, the tryout after two
minutes of play and again a step later: every read of the first sync ends;
`just test-emu-c6-cli-boards`).

**Not fixed here** —

- The binding-graph probe is one event too (6.6 KB for the tryout) and the
  card's panel needs it. On a heap whose largest block is under ~13 KB the
  halved frame cannot carry it.
- Every pattern step still re-stamps the whole playlist def, so each delta
  read after a step re-sends ~16 KB, now in pieces. Over Bluetooth that is
  a slower read every 30 s, not a lost one.
- A first sync that fails partway still leaves the client with every slot
  root of that read dropped and no retry until the next connect.

**Lesson** — a payload whose size depends on how long the board has run
passes every test taken at boot. Revision stamps, counters and uptimes are
text on the wire, and text grows. When a payload has a hard limit, test it
at the size it reaches after hours, not at load, and send anything that can
grow past the limit in pieces.
