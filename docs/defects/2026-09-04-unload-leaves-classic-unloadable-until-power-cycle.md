---
status: open
found: 2026-09-04      # how: silicon bench, plan 2026-09-04-1358-classic-heap-fragmentation-research
area: lpa-server LoadProject headroom gate vs the classic's heap after a load/compile/unload cycle
class: stand-in-divergence   # largest-free-block stands in for "can a load succeed"; unload restores volume, not contiguity
related:
  - 2026-09-04-read-gate-refuses-on-largest-block-proxy.md
  - 2026-08-29-load-project-resets-instead-of-refusing.md
  - ../reports/2026-09-04-classic-heap-fragmentation.md
---
# After one unload, the classic cannot load any project until it is power-cycled

**Shape** — desk classic (DOM-Z-102, first-fit build from this branch),
2026-09-05. Boot auto-loads `/projects/studio`; `stopAllProjects`; then
`LoadProject /projects/studio` — the same project, just unloaded:

```
load refused: heap headroom too low (largest free block 39663 B < 65536 B);
power-cycle the device or load a smaller project
```

Heartbeats around it (`bench/bench-llff-reload.csv` in the planning dir):

| phase | free | used | largest |
|---|---:|---:|---:|
| boot, before auto-load | 170,332 | 16,036 | 94,780 |
| `studio` resident, steady | 34,160 | 152,208 | 25,536 |
| after `stopAllProjects` | 166,172 | 20,196 | 39,655 |

Unloading gives back the bytes (166 KB free) but not the contiguity: 4.6 KB
of residents born during the load and compile stay standing in region 0
and cap its largest hole at 39.7 KB; region 1's 72 KiB tail is likewise
split. The 64 KiB gate then refuses every load — smaller projects included,
since the gate does not look at the project — and the remedy text is the
only true statement in the message: power-cycle.

**Why it matters** — Studio's ordinary flow (connect, stop, upload, load)
passes through exactly this state. A user who stops a project on the
classic and loads another gets a refusal that reads as "your project is too
big", and nothing short of a power cycle clears it.

**Mechanism** — the emulator's pinning table
(`lp-cli profile … --collect alloc`, fragmentation section) names the
residents' kind: JIT link metadata (`link_compiled_module_jit`), the
project-manager map entry, the fs-event map, `String` clones. They are born
mid-transient and survive the unload, so the free space around them cannot
coalesce. The gate reads largest-block as "can a load succeed", and after
an unload that proxy is at its worst while the heap has more free bytes
than at any time since boot.

**Fix direction** — two levers from the report, either of which clears
this: residents-first packing so load-time residents land at the front
(report lever 1), or a fallible load path that attempts the load and fails
cleanly on the allocation that cannot be served instead of pre-refusing on
a proxy (the same shape as lever 7 for reads). A cheaper stopgap is to
make `stopAllProjects` drop the residents that only the unloaded project
needed (the JIT link metadata and per-project maps) — verify with the
heartbeat's `largest_free_block` returning toward the boot figure.
