---
status: fixed
found: 2026-09-04          # how: hardware-walk (classic ESP32 V3 on a CH340 bridge, the bench walk that verified PR #510)
fixed: this change
area: fw-esp32v3 wire request decode (lpc-wire serde_base64 → FsRequest::Write) under an auto-loaded project; lpa-devices Flash activity stamp outcome copy
class: assumed-context
related:
  - 2026-09-04-pre-flash-hello-stamps-over-a-closed-port.md
  - 2026-08-29-shader-jit-compile-transient-starves-classic-heap.md
  - ../adr/2026-09-04-the-manifest-stamp-streams-beside-a-running-project.md
---
# The classic runs out of heap decoding the `/hardware.json` write while its project runs

**Symptom** — with the stamp now running over an open port (PR #510),
the write itself never gets an answer. 133 ms after the card says
"Writing /hardware.json" the board prints:

    ====================== OOM ======================
    allocation failed: requested=10480 align=1 free=15308 used=171060 largest_free=7616 retry_ok=false
    [OOM] FRAGMENTED: 15308 bytes free but the largest single block is 7616 — 7692 bytes unusable

soft-resets, and boots clean 1 s later. The stamp waits out its 45 s
deadline and the card closes with **"writing the board manifest timed
out — the compiled-in default pin map stands"** — which on this board was
false: `/hardware.json` was already on its littlefs from an earlier
stamp (the flasher keeps the filesystem), the boot log names it before
and after the crash, and the five strips ran on its pin map the whole
time.

## Mechanism

Decoded backtrace (`just decode-backtrace-esp32v3`, commit
`b9749c522696`):

    fw_esp32v3::recovery::panic_path::stage_oom_and_reset
    fw_esp32v3::on_alloc_error
    alloc::raw_vec::… reserve → Vec<u8>::spec_extend
    <serde_json::read::StrRead as Read>::parse_str
    serde_json … deserialize_str::<StringVisitor>
    lpc_wire::serde_base64::deserialize_smart
    <lpc_wire::server::fs_api::FsRequest as Deserialize>::deserialize … visit_enum
    <PhantomData<lpc_wire::message::client::ClientRequest> as DeserializeSeed>::deserialize

The crash is in **decoding the request**, before any file write runs.
The DOM-Z-102 runtime manifest is 6,123 bytes of pretty-printed JSON;
on the wire it travels base64 (~8.2 KB), and `parse_str` reserves a
scratch `Vec<u8>` for the string (the failing 10,480-byte reserve) on
top of the line buffer that already holds the frame and the `Vec<u8>`
the decode will produce — roughly three copies of the payload, in one
request, on a classic whose heap sits at 15 KB free / 7.6 KB largest
hole once the auto-loaded `studio` project and its compiled shader are
resident. The stamp runs at the board's tightest moment by construction:
the ladder only starts it after the hello, and the hello only comes
after auto-load.

Two facts, then:

1. **The board cannot take the stamp while its project runs.** A
   freshly-erased board (the M2 walk) has the heap; a board that
   auto-loads a project on boot does not.
2. **The stamp's timeout copy over-claims.** "The compiled-in default
   pin map stands" is what the reducer says when it heard nothing back;
   here the manifest was present and in force. The honest sentence for a
   quiet stamp is that Studio does not know whether it landed.

## Options (as found)

- Stop the running project before the write and reset after (the
  removal conversation already does `StopAllProjects`); the stamp is
  effective next boot anyway, so a reset follows naturally.
- Minify the manifest on the way out (`runtime_manifest_json` is the
  checked-in pretty JSON, verbatim) — a 2–3× cut, but the three-copies
  decode shape stays.
- Stream the write (chunked `FsRequest::Write`) — the removal of the
  three-copies shape, and a wire change.
- Copy: a timed-out stamp says "not confirmed", never "default stands".

## Fix

Decided in `docs/adr/2026-09-04-the-manifest-stamp-streams-beside-a-running-project.md`:
**stream the write, keep the project running, fix the copy.** Two
corrections to the options above, found on the way: the payload is not
base64 on the wire (`serialize_smart` sends UTF-8 as a JSON string, so it
is the ~6 KB text, escaped), and streaming is **not** a wire change —
`FsRequest::WriteChunk` has been on the wire since the push (M5), with an
O(chunk) `append_file` in the firmware.

- `lpa_client::write_file_in_chunks` (`lp-app/lpa-client/src/device_stamp.rs`)
  writes a file as a run of `WriteChunk`s of `MANIFEST_CHUNK_BYTES`
  (1 KiB, sized for the decode on the board's worst heap — four copies of
  1 KiB is ~5 KB of transient in ~2 KB allocations, inside the 7.6 KB hole
  the bench had), one plain `Write` when it fits. On a failed chunk it
  removes the prefix (best effort) and its error says which state the
  board is in; the loader refuses a torn manifest at boot either way.
  `write_device_file` (`port_client_io.rs`) uses it for the stamp.
- The Flash reducer's copy says only what it knows: a stamp that heard
  nothing back says "the board manifest write was not confirmed — the
  board never answered it, so whatever pin map it has stands"; a stamp the
  board refused carries the conversation's own words; "the compiled-in
  default pin map stands" leaves the reducer.

Stopping the project was the leading candidate and lost on the board's
behaviour, not the heap: it goes dark until a reboot, and a reboot
re-enumerates a native-USB port after the flash activity has settled — a
second reconnect ladder outside the activity that knows how to climb one.

## Regression coverage

- `lpa-client` `device_stamp::tests`: one write when it fits; offset
  chunks that reassemble to the manifest, label-only progress; a refused
  chunk removes the partial file and names the chunk; a board that stops
  answering mid-way is reported with the partial-file state.
- `lpa-devices` `flash::tests`:
  `a_stamp_that_hears_nothing_says_unconfirmed_never_that_the_default_stands`,
  `a_hello_with_no_link_ends_without_claiming_the_default_pin_map`, and
  the failed-stamp test now asserts the conversation's words are the only
  pin-map claim.
- `lp-app/lpa-devices/tests/scenarios.rs`:
  `a_stamp_that_hears_nothing_back_says_unconfirmed_not_that_the_default_stands`,
  `a_stamp_the_board_refused_carries_the_conversations_words_not_a_pin_map_verdict`.
- Bench: see the References below for the before/after transcripts.

## Lesson

A conversation that runs beside a running project has to size its
frames for the board's worst heap, not for the wire's frame budget — the
push's 4 KiB chunk is safe only because the push stops the project first.
And when an effect goes quiet, the reducer knows exactly one thing: that
it heard nothing. Every extra clause ("the default stands") is a guess
dressed as a fact, and on a board that keeps its littlefs across a flash
the guess was wrong.

## References

- Device trace (before): bench tab on the PR #510 worktree's server, 2026-09-04
  (`lp-studio-device-trace` in localStorage; the flash at
  `+134.96s Writing /hardware.json`, OOM at `+135.09s`).
- Device trace (after): bench tab on this fix's worktree server (port
  36001), 2026-09-04 13:59 — a reflash from the card on the same board
  with the same `studio` project auto-loaded: `Writing /hardware.json
  (1/6)` … `(6/6)` at +17.61 s … +18.32 s after the write effect ended
  (~140 ms per chunk), `board manifest written`, outcome `firmware
  installed — domraem/dom-z-102 · fw-esp32v3 cd1139501e18`. No OOM, no
  reset; the strips ran through the stamp. A second flash (14:04) booted
  the board from the chunk-written file — `hardware manifest:
  domraem/dom-z-102 (DOM-Z-102)` at +17.22 s, no "override … is invalid"
  — and stamped it again the same way. (The card's Reset was acknowledged
  but did not reboot this CH340 classic while it was running — the frame
  counter kept climbing — so the boot read needed the flasher's own hard
  reset.)
- Heap-pressure lineage: `2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`.
