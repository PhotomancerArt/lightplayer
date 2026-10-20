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

**Fix** — candidate: PR #1091 (open; this entry closes when it merges).
The pull asks `whole_file_gate::whole_file_refusal` — the rule `fs_read_refusal`
now shares — before each `read_file`, and a file that would not fit refuses
the page in `Read`'s words (`error` set, no entries; no wire change). It
refuses rather than reading in chunks because `LpFs` has no ranged read.
So a board whose largest block stays under the project's biggest file (the
shipped release with a central connected: ~26.2 KB against the 27,091 B SVG)
cannot pull that project until the block grows or the central leaves; a
ranged `LpFs` read would remove that, and is not in the candidate.

**Regression coverage** — candidate: `file_sync::tests::a_pull_page_whose_file_does_not_fit_is_refused_not_attempted`
and `a_refusal_names_the_file_it_reached_not_the_whole_project` (PR #1091).

**Lesson** — a gate added for one request kind does not guard the job:
every path that reads a whole file needs the same rule, or none should
read a whole file.

**2026-10-10, same family** — #1091 merged (`afa288b66`) and gates the
pull, but the Edit press still reset the choker on the same 27,091 B SVG:
Studio asks for `FsRequest::HashPackage` (the library bind) before it
pulls, and the hash read every file whole with no gate.
`2026-10-10-the-edit-press-package-hash-reads-files-whole-ungated.md` gates
it with the same rule.
