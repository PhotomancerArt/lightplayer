---
status: fixed
found: 2026-10-06      # how: hardware-walk (G1 desk numbers, N7, fixture-c6, m6-split 128aea9ac)
fixed: this change
area: lpa-server `handlers::handle_load_project` × `ProjectManager::unload_all_projects` × the 64 KiB load gate
class: reclaim-ordered-behind-its-own-rebuild
related:
  - docs/defects/2026-08-29-load-project-resets-instead-of-refusing.md
  - docs/defects/2026-10-06-a-lan-link-strands-the-heap-below-the-load-floor.md
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989)
---
# A refused project switch leaves the board dark

**Symptom** — `lp-cli upload projects/test/basic lan:…` on fixture-c6 got
`load refused: heap headroom too low (largest free block 42112 B < 65536 B)`,
and afterwards no project ran: the LEDs stayed dark until a reboot.

**Root cause** — a switch frees the running project before it asks the
gate: an upload sends `StopAllProjects`, and `LoadProject` itself unloads
whatever runs, then checks headroom (deliberately, so the gate reads the
heap the load would run in). When the gate (or the load) refused, nothing
put the stopped project back. The refusal was safe for the board's memory
and left the show off. The same code is on main; Wi-Fi made it likely,
because a LAN link's allocations split the heap the stopped project frees.

**Fix** — `ProjectManager` remembers what its last unload stopped, until a
load succeeds. A load that is refused or fails, with nothing running
afterwards, loads those again, without the headroom gate (they ran in this
heap moments ago), and the error reply ends "— the previous project (…) is
running again". If that restore fails too, the error is the original one.

**Regression coverage** — `lpa-server/tests/project_load_refusal.rs::a_refused_switch_leaves_the_previous_project_running`
(the load's own unload, and a `StopAllProjects` first). Silicon re-check of
N7 owed through `main`. Why the heap was too fragmented to load is a
separate question (the LAN memory work).

**Lesson** — a refusal that runs after an irreversible step is not a clean
refusal. Either the check goes first, or the step is undone when it says no.
