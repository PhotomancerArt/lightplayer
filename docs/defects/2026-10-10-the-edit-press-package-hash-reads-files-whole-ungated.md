---
status: fixed
found: 2026-10-10      # hardware-walk: RAM research V2 (#1110/#1088 silicon) on loose-c6, release control and #1088 alike
fixed: this change
area: lpa-server file_sync::handle_hash_package (FsRequest::HashPackage) × Studio's Edit press (the library bind's read_running_package) × fw-esp32c6 heap with a Bluetooth central connected
class: untested-path   # the whole-file gate covered Read (2026-10-06) and the pull (#1091); the package hash reads files whole too and was never reached
related:
  - 2026-10-09-studios-pull-reads-a-file-whole-with-no-memory-check.md   # same family: the pull, gated by #1091
  - 2026-10-06-a-message-the-heap-cannot-reassemble-resets-the-board.md  # added the fs-read gate
  - 2026-10-09-a-whole-file-read-over-the-frame-budget-gets-no-reply.md  # the frame-budget half of whole-file reads, still open
  - lp2025/2026-10-09-1203-ram-research   # V2's report and evidence/partB/, F06
---
# Studio's Edit press asks for a package hash that reads every file whole with no memory check, and resets the choker

**Symptom** — `loose-c6` (XIAO C6) running the PLAYFUL Choker, Mac Chrome
connected over Bluetooth running prod Studio. Pressing **Edit** reset the
board out of memory, 3 of 3 runs: on release 2026.10.10-13 (`b4a9098e4`,
which already carries the pull's gate, #1091) and on #1088's image alike.
The RTC record after each reboot:

```
[RECOVERY] last run crashed (oom): at <no frame>: alloc 27091 bytes failed (align 1) in <unset>
[RECOVERY] oom stats: requested=27091 align=1 free=36096 used=265440
```

Free 35.6–36.1 KB, largest block 23.5–24.0 KB across the runs. The decoded
frames ran `handle_client_message → LpFsView::read_file →
LpFsFlash::read_file (lp_fs.rs:234) → vec![0u8; 27091]`; 27,091 B is
exactly `playful-mapping.svg`. No `fs gate:` line appeared in any capture.

**Root cause** — opening the editor on a running board
(`StudioController::connect_running_project`) binds the board's project to
the library before anything else (`bind_running_project_to_library` →
`ProjectController::read_running_package`), and that asks the board for
`FsRequest::HashPackage` of the project directory. The server answers it in
`file_sync::handle_hash_package`, which chroots the directory (the
`LpFsView` in the frames) and runs `lpc_history::hash_package`, which reads
every hashed file whole to hash it. Nothing asks the heap first. The
whole-file gate (`whole_file_gate::whole_file_refusal`, largest block ≥
size + 512 B) guarded `FsRequest::Read` and, since #1091, the pull
(`ChangesSince`); the hash was the third whole-file reader and had no gate,
so it read the 27,091 B SVG into a heap whose largest block was ~3.6 KB too
small. The hash comes before the pull, which is why no `fs gate: pull of`
line was ever printed.

Reproduced on the host before the fix: a dispatch-level test
(`handlers::tests::the_edit_press_hash_is_gated_like_a_read_of_its_biggest_file`)
with the SVG's size and a 23,820 B probe got a `Read` of the file refused
and a `HashPackage` of its directory answered with a hash, i.e. the file read
whole. Not reproduced on silicon or the emulator after the fix.

**Fix** — `file_sync::handle_hash_package_with_headroom`: before hashing,
every file the hasher would read (each hashed path that is not a directory,
the hasher's own walk) asks `whole_file_refusal`; one that would not fit
refuses the hash in `Read`'s words (`FsResponse::PackageHash` with `error`
set and `hash` empty, the reply's existing shape; no wire change) and logs
`fs gate: hash of <prefix> — …`. `handle_fs_request_with_headroom` (the live
dispatch's) routes `HashPackage` through it with the board's probe;
`handle_hash_package` keeps its signature and runs ungated (no probe), like
`handle_changes_since`. Studio already treats a failed read of what the board
runs as "no bind" (a warning in the log, the editor opens unnamed), so the
press opens the editor instead of resetting the board. As with the pull, the
board refuses rather than hashing in pieces, because `LpFs` has no ranged
read; a board whose largest block stays under the project's biggest file
cannot be bound to the library until the block grows.

**Regression coverage** —
`handlers::tests::the_edit_press_hash_is_gated_like_a_read_of_its_biggest_file`
(the dispatch, refused and served),
`file_sync::tests::a_hash_over_a_file_that_does_not_fit_is_refused_not_attempted`
(the rule, the exact fit, and the package's own `.lp/` not asked about), and
`tests/project_read_refusal.rs::a_package_hash_the_heap_cannot_hold_is_refused_and_the_server_stays_alive`
(through `LpServer::tick_and_send`).

**Studio's side, checked** — a refused hash comes back through
`validate_hash_package_response` as `ClientError::Server("hash package failed:
<the refusal's words>")`, then `UiError::Protocol`. `bind_running_project_to_library`
logs it once as a warning ("could not read what the board is running: …") and
returns `NotApplicable`: the editor opens connected and unnamed, and nothing
asks the hash again until the next open or save. The pull that follows is
the already-gated one (a refused sync is recorded as "project sync needs
attention", no retry loop). Pinned for the client half by
`file_sync_ops::tests::a_refused_package_hash_is_an_error_with_the_boards_words`;
the controller's bind has no test of its own for this path.

**Lesson** — the pull's own lesson came true one request later: a gate
added per request kind leaves the next whole-file reader ungated. The three
gated readers are now `Read`, `ChangesSince` and `HashPackage`; anything
else that reaches `read_file` from a client request on a project's files
needs the same question, or a ranged read so none of them needs one.
