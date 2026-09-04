# ADR: The board-manifest stamp streams in chunks beside a running project

- **Status:** Accepted
- **Date:** 2026-09-04
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Related:** `docs/defects/2026-09-04-classic-ooms-decoding-the-manifest-write.md`,
  `docs/adr/2026-08-25-event-fold-device-model.md` (the flash ladder),
  `docs/defects/2026-09-04-pre-flash-hello-stamps-over-a-closed-port.md`

## Context

After a flash, the Flash activity stamps the picked board's runtime
manifest onto the device as `/hardware.json` (board-selection D4,
effective next boot). Until this ADR the stamp was one
`FsRequest::Write` carrying the checked-in manifest verbatim — 6,123
bytes for the DOM-Z-102.

The stamp runs at the board's tightest moment by construction: the ladder
starts it after the post-flash hello, and the hello comes after the board
has auto-loaded its project and compiled its shader. On the bench classic
(15 KB free, 7.6 KB largest hole with the `studio` project resident) the
request never reached the filesystem: decoding it asked for a 10,480-byte
serde_json scratch buffer and the board soft-reset. The decode of one
`Write` holds about four copies of the payload at once — the line
`String` with the frame, the unescape scratch, the decoded `String`, and
`deserialize_smart`'s base64 attempt — so the frame size, not the file
size, is what the board has to afford.

The stamp then waited out its 45 s deadline and the card said "the
compiled-in default pin map stands", which was false: the flasher keeps
littlefs, and the board was running the manifest an earlier stamp wrote.

Four options were on the table:

1. **Stop the running project before the write, reboot after.**
   `StopAllProjects` frees the heap (the removal conversation already
   does it), and the stamp is effective next boot anyway.
2. **Minify the manifest on the way out** (6,123 → 4,792 bytes).
3. **Stream the write** as a run of `FsRequest::WriteChunk`s.
4. **Fix the copy**: a stamp that heard nothing back says the write was
   not confirmed, never that the default stands.

## Decision

**Option 3, with option 4.** The stamp writes `/hardware.json` as a run of
`WriteChunk`s of `lpa_client::MANIFEST_CHUNK_BYTES` (1 KiB) each, and
the project keeps running. Chunk 0 truncates, later offsets append (the
server's existing stateless-chunk contract), and a manifest that fits one
chunk still goes as one plain `Write`.

`WriteChunk` is already on the wire — the push has used it since M5, and
the firmware's `append_file` is O(chunk) — so this is a change in how the
stamp uses the protocol, not a protocol change. The chunk is sized for the
decode on the board's worst heap, not for the frame budget: four copies of
1 KiB is ~5 KB of transient in allocations of ~2 KB at most, inside the
7.6 KB hole the bench had. The push's 4 KiB chunk runs after a
`StopAllProjects` on a freed heap and stays as it is.

A chunked write has one failure mode a single write does not — a torn
file — and the conversation owns it: on a failed chunk it removes the
prefix (best effort) and its error says which state the board is in. The
firmware's manifest loader refuses a torn manifest at boot and falls back
to the compiled default with a warning, so both states boot the same; the
words differ in what Studio actually did.

The reducer's copy now says only what the reducer knows. A stamp the
board answered with an error carries the conversation's words onto the
card. A stamp that heard nothing back says "the board manifest write was
not confirmed — the board never answered it, so whatever pin map it has
stands". The phrase "the compiled-in default pin map stands" leaves the
reducer entirely.

## Consequences

- The stamp survives the board's tightest heap without changing what the
  board is doing. The LEDs keep running through the stamp; no reboot, no
  second reconnect ladder.
- Six round trips instead of one for the largest manifest. Each is a
  short exchange on a board that is answering; the stamp's 45 s budget
  was sized against the ready-wait, and the chunks fit inside its
  headroom. `RosterConfig::stamp_deadline_ms` documents the arithmetic.
- The card's terminal shows the write's progress per chunk
  (`Writing /hardware.json (3/6)`), label-only so the flash bar does not
  restart at 0 for the second half of a flash.
- A mid-write failure can leave the board on its compiled default where
  a single-frame write would have left the previous manifest intact.
  The conversation minimises the window (delete on failure) and says so.
- Any future conversation that runs beside a running project inherits
  the rule: size frames for the board's worst heap, not the wire budget.

## Alternatives Considered

**Stop the project, write, reboot** — the leading candidate going in.
It frees the heap and the write is one frame again. Rejected because of
what it does to the board: it goes dark until the reboot, and the reboot
re-enumerates a native-USB port (C6, S3) *after* the Flash activity has
settled — the flash ladder exists precisely because that re-enumeration
is where the old flow broke, and this would run it twice, the second time
outside the activity that knows how. On a UART bridge the reboot is
cheap, but the stamp has to work on both. It also makes the pin-map stamp
a side effect on the project the user was running, which nobody asked
for. A fresh flash has no project to stop, so the cost lands exactly on
the reflash-with-littlefs case that the defect was about.

**Minify the manifest** — a third off the payload, but the decode shape
stays: a 4.8 KB string still asks for a ~5 KB `String`, a scratch buffer
that grows past it, and the line buffer, against a 7.6 KB largest hole.
Marginal, not robust, and it makes the file on the device harder to read
over the CLI for no gain once the write is chunked.

**Chunk at the push's 4 KiB** — the same scratch growth that failed
(8–10 KB) on the same heap. The stamp's chunk is its own constant with
the reasoning next to it.

**Slim the firmware's decode** (borrow instead of the scratch `String`,
skip the base64 attempt for text) — worth doing on its own merits, but it
changes the firmware being flashed, and two copies of a 6 KB payload in a
15 KB heap is still a coin toss. Not a fix for the stamp.

## Follow-ups

- `deserialize_smart`'s base64 attempt allocates ¾ of the string before
  failing on the first non-base64 byte; a text-first check would drop one
  of the four copies for every fs write on every board. Firmware-side,
  independent of this decision.
- The push conversation's chunk (`FILE_SYNC_CHUNK_BYTES`) is safe today
  because the push stops the project first. If a push ever runs beside a
  running project, it needs this ADR's sizing, not the frame budget's.
