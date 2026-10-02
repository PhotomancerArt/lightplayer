---
status: fixed
found: 2026-10-02      # how: e2e — the C6 repartition's migration walk (W1, an early close)
fixed: this change
area: lpa-client device_stamp (the board-manifest stamp) / fw-esp32-common hardware::manifest_loader
class: write-ordering
related:
  - lp2025/2026-10-01-1843-c6-repartition
  - docs/defects/2026-09-04-classic-ooms-decoding-the-manifest-write.md
  - docs/adr/2026-09-04-the-manifest-stamp-streams-beside-a-running-project.md
---
# A tab closed while Studio stamps the board manifest leaves `/hardware.json` truncated

**Symptom** — in an early version of the migration walk, the browser was
closed as soon as the card stopped saying "Flashing firmware", which is
before the activity's last step (stamping `/hardware.json`) had finished.
The chip then held a **6,144-byte** `/hardware.json` where the board
manifest is 6,802 bytes (`lp-cli hardware lpfs report --image` of the chip,
SHA-256 `413349c8…` against `0374979c…`). The same truncation is the
likeliest explanation of an earlier plain-update run (W2) whose
`/hardware.json` also differed. Observed on `lp-emu:esp32c6:t1`.

**Root cause (confirmed on the host)** — the stamp
(`lpa_client::write_file_in_chunks`, called by Studio's wire conversation,
the browser-serial provider and `lp-cli hardware stamp`) wrote the manifest
straight to `/hardware.json` as stateless `FsRequest::WriteChunk`s of
`MANIFEST_CHUNK_BYTES` (1,024): chunk 0 truncates and each later chunk
appends, so the file on the board is a prefix at every chunk boundary.
6,144 is exactly six of the seven chunks. When the link goes away between
chunks, the clean-up delete cannot reach the board either, and the prefix
stays. A `/hardware.json` that does not parse is refused whole at boot (the
loader falls back to the compiled-in manifest — it was never half-used), so
the board kept booting, but its stamped manifest was gone until the next
stamp. Reproduced without the emulator: the regression test below, run
against the old unjournaled stamp, fails with `cut after 1 of 7 requests:
the board lost its stamped manifest`.

**main is affected, not only the migration.** The stamp is not part of the
repartition: it runs after **every** firmware update (the flash activity's
`Stamping` phase, `lpa-devices` `activity/flash.rs`) and in
`lp-cli hardware stamp`, and `device_stamp.rs` was unchanged by this PR
against `origin/main`. Any update whose tab closed, cable came out or board
reset during the stamp's seven round trips — on main today — could leave a
torn manifest. The repartition only made it likelier to be noticed, because
its walk compares files byte for byte.

**Fix** — the stamp is journaled, with no new wire request (the wire has no
rename, and `WIRE_PROTO_VERSION` is unchanged):
`lpa_client::stamp_board_manifest` writes the whole manifest to
`/hardware.json.next`, then to `/hardware.json`, then deletes the staged
copy. The firmware's loader (`fw-esp32-common`
`hardware::manifest_loader::settle_staged_stamp`, shared by the C6, S3 and
classic) settles a staged copy at boot — which is when a manifest takes
effect anyway: one that parses is written over the live file (the stamp got
that far, so the live file may be torn) and one that does not is dropped
(the stamp never reached the live file, which is therefore the previous
stamp, whole). The staged copy is deleted only after the live write, so a
power cut during boot's own settle is redone on the next boot. All three
stamp callers (Studio's wire conversation, the browser-serial provider's
`stamp_board_manifest`, `lp-cli hardware stamp`) take the journaled path,
and the stamp's error now says which state the board is in ("the board
keeps the manifest it had" / "the board finishes the stamp when it next
boots"). The loader's refusal of a manifest that does not parse is now an
`error!`, not a `warn!`, and names the byte count.

A board that already holds a torn `/hardware.json` from before this fix
has no staged copy to recover from: it boots on its compiled-in manifest,
as before, until it is stamped again — which the next update does.

**Regression coverage** — `fw-esp32-common`
`hardware::manifest_loader::tests::a_stamp_cut_after_any_request_leaves_the_board_a_whole_manifest`
runs the real client conversation against the real server's file handler
(`lpa_server::handlers::handle_fs_request`) on a board filesystem, cuts the
link after each of the stamp's 15 requests in turn, boots, and asserts the
board runs a whole manifest — the previous one or the new one, never the
fallback — and that boot leaves no staged copy. It fails on the old stamp
(above). Beside it: `a_torn_hardware_json_is_refused_whole_at_every_length`
(every prefix of the XIAO manifest, the defect's 6,144 included, loads as
nothing or as the whole manifest) and `boot_settles_a_staged_copy_once`;
and in `lpa-client` `device_stamp::tests`, the request order and the two
interrupted-stamp messages. These run under `--features server`, which no
CI recipe used to turn on for `fw-esp32-common` (its `lp_fs` legacy-guard
tests were unrun in CI too); `test-rust-core` now runs that crate's
`--features server` tests.

**Lesson** — "the activity ended" and "the last write landed" are different
moments; a walk that keys off the first will eventually cut the second. And
a file written in more than one request is torn at every request boundary:
if a reader must never see it torn, the writer needs a journal (or an
atomic rename), not a faster write.
