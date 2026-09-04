---
status: open
found: 2026-09-04          # how: hardware-walk (classic ESP32 V3 on a CH340 bridge, the bench walk that verified PR #510)
area: fw-esp32v3 wire request decode (lpc-wire serde_base64 → FsRequest::Write) under an auto-loaded project; lpa-devices Flash activity stamp outcome copy
class: assumed-context
related:
  - 2026-09-04-pre-flash-hello-stamps-over-a-closed-port.md
  - 2026-08-29-shader-jit-compile-transient-starves-classic-heap.md
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

## Options (not decided)

- Stop the running project before the write and reset after (the
  removal conversation already does `StopAllProjects`); the stamp is
  effective next boot anyway, so a reset follows naturally.
- Minify the manifest on the way out (`runtime_manifest_json` is the
  checked-in pretty JSON, verbatim) — a 2–3× cut, but the three-copies
  decode shape stays.
- Stream the write (chunked `FsRequest::Write`) — the removal of the
  three-copies shape, and a wire change.
- Copy: a timed-out stamp says "not confirmed", never "default stands".

## References

- Device trace: bench tab on this worktree's server, 2026-09-04
  (`lp-studio-device-trace` in localStorage; the flash at
  `+134.96s Writing /hardware.json`, OOM at `+135.09s`).
- Heap-pressure lineage: `2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`.
