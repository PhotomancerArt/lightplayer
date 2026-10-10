---
status: open
found: 2026-10-09      # report: RAM research E7 (gates against real peaks) on loose-c6; Studio's "Open in editor" reset the choker
area: lpa-server file_sync::handle_changes_since (FsRequest::ChangesSince, the initial-pull path) × fw-esp32c6 heap with a Bluetooth central connected
class: budget-exhaustion   # a job with no memory check beside one that has it (FsRequest::Read's fs gate)
related:
  - 2026-10-06-a-message-the-heap-cannot-reassemble-resets-the-board.md   # added the fs-read gate; ChangesSince was not covered
  - 2026-10-09-a-phones-bluetooth-link-leaves-the-choker-under-the-read-gate.md
  - lp2025/2026-10-09-1203-ram-research   # E7 report and its evidence/silicon/
---
# Studio's pull reads a project's files whole with no memory check, and resets a tight board

**Symptom** — `loose-c6` (XIAO C6) running the PLAYFUL Choker with Mac
Chrome connected over Bluetooth, on a research image that held 4 KB of heap
back at boot (standing in for the ~3 KB a runtime project switch lost on the
2026-10-09 walk). Pressing **Open in editor** in Studio (lightplayer.app
build `cdd5b5f50`) reset the board. Its RTC record after the reboot:

```
[RECOVERY] last run crashed (oom): at <no frame>: alloc 27091 bytes failed (align 1) in <unset>
[RECOVERY] oom stats: requested=27091 align=1 free=38056 used=263480
[OOM] FRAGMENTED: 38056 B free in total but only 26412 B in one piece
```

27,091 B is exactly `playful-mapping.svg`, the choker's largest file.

**Root cause** — Studio's new pull ("Pulled from XIAO ESP32-C6" in the
Projects list) enumerates the project with `FsRequest::ChangesSince`, and
`lpa_server::file_sync::handle_changes_since` reads every upserted file
whole (`fs.read_file`) before cutting it into the page's chunks. Nothing
checks the heap first. `FsRequest::Read` has a gate for exactly this
(`handlers::fs_read_refusal`: largest block ≥ size + 512 B, else "board
memory busy"), added by
`2026-10-06-a-message-the-heap-cannot-reassemble-resets-the-board.md`; the
pull's path never goes through it.

Reproduced over USB with the central still connected
(`evidence/silicon/` of the E7 report): `FsRequest::Read` of the SVG was
refused in words (`largest block 26404 B; a 27091 B file needs 27603 B`);
the `ChangesSince` page that reaches the SVG (cursor `/module.json`:331)
reset the board with the record above.

The ballast is not what makes it possible. On the shipped release
(2026.10.08-23, no ballast) the same board with the central connected
reported a 26,224–26,232 B largest block in every heartbeat, already under
the SVG's 27,091 B (the research census image saw 28.3–30.4 KB). Pressing
Open in editor on the shipped image was not tried.

**Fix** — none yet. The obvious one: read a file into a page chunk by
chunk, so a pull never asks for more than a chunk; or, at least, refuse the page with
`fs_read_refusal`'s rule before `read_file`.

**Regression coverage** — none. No test pulls a project on a heap whose
largest block is under its largest file.

**Lesson** — a gate added for one request kind does not guard the job:
every path that reads a whole file needs the same rule, or none should
read a whole file.
