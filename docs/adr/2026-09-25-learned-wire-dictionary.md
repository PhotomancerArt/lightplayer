# ADR: The learned wire dictionary — JSON Pack learns its names per connection

- **Status:** Accepted
- **Date:** 2026-09-25
- **Deciders:** Photomancer (Yona, G0 of plan `lp2025/2026-09-25-0006-learned-wire-dictionary`)
- **Supersedes:** None (amends `2026-09-24-json-pack-wire-encoding.md`; supersedes D17 of the wire-version-skew vision)
- **Superseded by:** None

## Context

JSON Pack (`2026-09-24-json-pack-wire-encoding.md`, PR #795) packed the
board's replies against a **9,423 B static dictionary**: 438 keys and 191
values, generated from the wire types by a host-only tracer, ranked by a
traffic sample, fingerprinted, and tied to `WIRE_PROTO_VERSION` by a
`check-lint` gate. Both ends had to hold the same dictionary, so any wire
type that gained a field forced a regeneration and a proto bump, and a board
and a Studio built a day apart could not pack to each other. Trimming it to
the names seen in traffic (−5.8 KB) was measured and dismissed. The question
was where names should come from instead:

- **(a)** a per-connection **learned table**, HPACK-style: a name goes out
  in full on first use and as a code afterwards; no static dictionary;
- **(b)** the static dictionary sent **in-band** once per connection and
  cached by fingerprint (the wire-version-skew vision's D17);
- **(c)** a small **seed** plus learning.

A spike (`spike/learned-wire-dict`, `ae04588ae`) measured all three on the
emulated C6. Its full tables are in the plan's `notes.md`.

## Decision

**(a): each packed link learns its names. There is no seed and no static
dictionary.** One protocol constant set replaces the generated dictionary,
its generator, its CI gate and its tie to proto bumps.

### The format (JSON Pack format 2, `lp-json-pack`)

- Key and value codes index `seed ++ learned`. LightPlayer's seed is empty.
- **Learning is a side effect of sending**, with no new tag: an inline key
  (1..=48 B) is learned the first time it goes inline; an inline string
  (2..=31 B) is learned the **second** time (a log of first sightings
  remembers it). Back-references, blobs and numbers are never learned.
- **Capacities are format:** 3,072 B of text, 288 keys, 128 values, 512
  remembered first sightings; when full, that kind of learning stops (no
  eviction). A table is ~6.9 KB and all zero when new.
- Value codes 64..=143 moved into the unassigned `B0..=FF` range, so every
  learned value has a one-byte code and the order values are learned in
  stops deciding reply size.
- **Frame header:** epoch (1 B) + a 16-bit fold of a rolling hash over every
  entry learned and every first sighting remembered, in order (2 B). A
  learned frame is COBS kind `'L'` on the wire.
- Two ends agree on **`PACK_FORMAT_VERSION` = 2** and nothing else. The
  hello carries `pack_format: u8` (0 = cannot pack); `SetEncoding` carries
  `format`. `WIRE_PROTO_VERSION` 28.

### Commit, rollback and resync

- A frame's learning is **tentative**. The board takes a mark before
  serializing and rolls back to it when the frame is not sent: a JSON
  fallback, or a write the io task abandoned (the connect-time "dropping
  message" loss on real hardware). The host rolls back when a frame fails to
  decode. These cover every loss the board knows about, with no round trip.
- A loss the board does **not** know about (bytes lost in flight) is caught
  by the next frame's header. The reader drops that frame and every learned
  frame after it (`WireChunk::Desync`), never decoding against the wrong
  names, until the board resets. The owner calls `PackOptIn::desynced`, which
  re-sends `SetEncoding` under the existing 3 s rate limit: **every accepted
  opt-in starts a new epoch with an empty table**, so the opt-in request is
  the reset request. No new message.
- **A frame coded against the empty table is always the writer's reset**,
  in any epoch: it states a fact ("my table is empty"). This also covers a
  board that rebooted and restarted its epoch count while a host held the
  old table.
- **A reader that starts mid-connection, or after a replug,** holds a fresh
  table. The walk's replug handed one a packed frame of the previous link's
  table (bytes from before the detach). It is dropped like any other out-of-
  step frame, and `PackOptIn::desynced` asks for the reset even before that
  link's hello, because a learned frame on the link is itself proof the
  board packs.
- **Every Hello reply starts a new, empty epoch.** A quick port reopen
  (Studio does one after a push) never bumps the board's link epoch, so the
  board can stay packed while the host's new reader holds a fresh table.
  Found by running the G1 protocol on the emulator: identify's Hello reply
  was among the frames the fresh reader dropped, and the card read
  "pre-hello firmware". A Hello is how a host begins a conversation, so the
  board resets its table before answering one; that reply is coded against
  the empty table, which any reader accepts.
- **Why a state hash and not an entry count** (found during the spike):
  after a torn frame the two sides can learn *different* entries at the same
  index while their counts stay equal, and the next reference would decode
  silently to the wrong name. A count cannot see that; the state hash can.
- **Board RAM (G0 D4):** the table is heap-allocated, zeroed and fallibly,
  when the server is about to answer an opt-in `packed`, and freed when the
  link goes back to JSON. A board running a show with nobody attached pays
  nothing. No heap for it: the answer becomes `json`.

### Captures

A learned capture decodes **from its connection's start** (or from the
board's next reset). `lp-cli wire unpack` names each frame it cannot read
(`<learned frame: table unknown, epoch N, M bytes>`, counted as
`unreadable`) and never guesses. Every host reader keeps **one**
`WireStream` per link for the link's whole life.

## Measurements

Every emulated number names its configuration; none is hardware-validated
(G1, a desk sitting, is the parity check).

**Bytes, the same messages both ways** (`lp-emu:esp32c6:t1`, lp-emu
`7190618fc`). A fresh Studio session, headless: connect, push the PLAYFUL
choker, 90 s of device-card reads, 150 s of the editor lens, 90 s on the
Fixture. 953 packed frames, recorded by the door's wire tap, 0 torn and 0
desyncs. The static column re-encodes each of those 953 messages with
`main`'s static-dictionary writer.

| class | n | JSON median | static median | learned median |
|---|---:|---:|---:|---:|
| lens reply, steady | 626 | 2,031 | 633 | **631** (−0.3 %) |
| device-card read | 260 | 600 | 306 | 310 (+1.3 %) |
| heartbeat | 31 | 635 | 169 | 131 (−22.5 %) |
| lens sync (big) | 3 | 15,886 | 6,348 | 6,158 (−3.0 %) |
| hello (packed) | 1 | 580 | 176 | 448 |

Whole session, board→host: static 523,358 B → learned 524,495 B (+0.22 %).
From connect to the lens's first read: 96,031 → 98,234 B (+2.3 %). The
first ~7 KB after connect costs +14.8 %: that is where names go out in full.

The spike's replay of an older recorded session measured the steady lens
reply at 729 B against 712 B static, and value-code order was the gap. With
all 128 learned values on one-byte codes, it closed.

**Instructions per reply** (`lp-emu:esp32c6:t2`, lp-emu `7190618fc`;
`minstret` around `ser_learned_frame_to`, a scratch image never committed):
a steady lens reply takes **164,006** instructions for 626 B (median of 32);
the first full read (4 frames, 20,393 B) takes 6,628,278. The static path is
gone, so its figure comes from the spike's build: 154,097 and 5,320,555
(+6 % steady; the first read's +25 % is mostly the second-sighting log's
linear scan). At 160 MHz the steady difference is ~0.06 ms, an emulator
count, not a silicon time.

**Flash** (C6, `just fw-esp32c6-size-check` on `main` `dc9b07abd` and on
this change): image 2,867,280 → 2,860,752 B (**−6,528 B**); `.rodata`
−9,408, `.text` +2,870, `.data` +8, `.bss` +248.

**Loss:** `lp-json-pack/tests/learned_loss.rs` drives a board table and a
host table over the traffic sample three times, with 1 in 23 frames
abandoned, 1 in 29 lost and 1 in 31 torn, over 24 seeds: 0 wrong decodes,
recovery within one re-ask. On the emulator, the door's `LP_EMU_WIRE_TEAR`
fault tears a packed frame of the shipped image in flight; the reader drops
the three frames after it as out of step, re-asks, and decodes every reply
after the board's reset.

## Consequences

- Flash: the dictionary tables leave the image (C6 `.rodata` −9,408 B) and
  the learning code arrives (`.text` +2,870 B): **−6,528 B** of C6 image
  against `main` (`dc9b07abd`), measured with `just fw-esp32c6-size-check`
  on both.
- RAM: **~6.9 KB of heap per packed link**, only while a host has it packed.
  The heap ratchet samples boot, where nothing has opted in.
- **Version skew:** a new wire field, enum variant or string needs nothing
  done for packing. Board and host pack to each other whenever their
  `PACK_FORMAT_VERSION` matches, whatever build each is. Message-shape skew
  is the wire-version-skew plan's problem; the panel surface never packs
  (`2026-09-25-panel-surface.md`).
- Deleted: `lpc-wire`'s `wire_dictionary.rs` (generated), `wire_dictionary_gen/`,
  the `wire-dict` bin and feature, `just wire-dict` / `wire-dict-check`, and
  the rule that a dictionary change needs a proto bump. The rule that
  replaces it: **a change to the tag table, the learning rule or the
  capacities bumps `PACK_FORMAT_VERSION` and `WIRE_PROTO_VERSION`**.
- The first connect costs more than a static dictionary did (names go out
  in full once), and a desync costs the replies in flight during one opt-in
  round trip. Studio's pull loop already retries a lost reply by deadline.
- A torn frame is now detected at the next frame's header even when its
  bytes happen to decode, which the static dictionary could not do.

## Alternatives Considered

- **(b) An in-band static dictionary, cached by fingerprint.** Stateless per
  frame, 0 board RAM, captures decodable from any point, and it removes the
  proto-bump coupling too. Rejected at G0: it keeps the 9.4 KB of flash, the
  generator and its drift gate (for ranking), and adds a 5,703 B frame on
  every first connect of a new fingerprint plus a host-side cache in three
  places (Studio, lp-cli, the emulator door). Yona chose (a) at G0 ("a
  learned, I think"); its RAM is paid only while a host has the link packed.
- **(c) A seed plus learning.** Trims ~1–2 KB off the first connect and
  nothing off the steady state (703–706 B against (a)'s spike 729 B), but
  brings dictionary agreement back — the thing (a) removes — and a seed big
  enough to matter (240 keys/64 values, 2.4 KB) is a quarter of the old
  dictionary.
- **Learning every value on first sighting.** 57 % of the distinct value
  strings of a recorded session are seen once (paths, node names); they
  fill a device-sized table and the steady lens reply grows 19 %.
- **A count-only frame header.** Misses the equal-count divergence above.
- **QPACK-style acknowledgements.** Overkill: the board already knows about
  the commonest loss (its own abandoned write), and the host's request
  stream carries the one reset it needs.

## Follow-ups

- The emulator's C6 link model never loses bytes; `LP_EMU_WIRE_TEAR` is a
  door-level fault injector until the link-loss fidelity follow-up lands
  (`docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md`).
- `FrameScanner` reports a mid-body tear as two drops (its torn-write resync
  rule makes a phantom empty frame); the second is harmless but miscounted.
- A standing-subscription live view would make an LZ layer worth measuring
  again; a learned table and LZ compose.
