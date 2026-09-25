# Dormant playlist entries: 25 patterns on the emulated C6

**Date** 2026-09-25 · **Plan** `lp2025/2026-09-24-2351-multi-pattern-projects`
(P6; acceptance criteria AC2–AC5) · **Project**
`catalog/projects/playful-choker-tryout` (25 entries) · **Engine** branch
`feat/multi-pattern-projects-p6`, measured at `5b09557cf` (every engine and
firmware source byte there is the same at the report's own commit; later
commits touch only the catalog, heap-budget records and docs) ·
**Emulators** fw-emu (`lp-cli profile`, the RV32 engine emulator) and
`configuration=lp-emu:esp32c6:t1`, lp-emu at `5b09557cf` (its emulator
sources equal `origin/main` `7190618fc`'s, plus P3's two perf-event names)

Every number below is emulated. Memory figures transfer to silicon; the
times are reports, never gates.

## Summary

| criterion | target | measured | verdict |
|---|---|---|---|
| AC2: heap per dormant entry at load | ≤ 512 B | **343 B** (was ~17.6 KB) | met |
| AC3: 25 entries upload and play on the emulated C6 | post-deploy check passes | passes; 105,456 B free after the first compile; 7 switches, no failure | met |
| AC4: retained heap, end of pass 2 − end of pass 1 | ≤ 256 B | **+1,344 B**, all of it one bounded side store (below) | **missed as stated**; no leak found |
| AC5: never black across a switch | — | the pad holds a lit frame through all 7 C6 switches; the run's only all-dark frames (4) are mid-Fireflies, a near-black pattern, 15 s after its switch | consistent |

## 1. Heap at load: 5 versus 25 entries (AC2)

**Method.** The vision's own: fw-emu,
`lp-cli profile <dir> --collect alloc --mode startup --max-cycles 800000000`,
reading the `project-load` window's `retained` bytes (what the load
allocated and still holds when the window closes;
`measurements/retained.py` in the planning directory computes the same
figure from the trace and agrees to the byte). The 25-entry project is the
committed tryout; the 5-entry variant is the same directory with the
playlist cut to entries 1–5 (same modules, same tour), not committed.

| project | entries | `project-load` retained | commit |
|---|---:|---:|---|
| tryout before dormancy (vision) | 5 | 112,662 B | `5ab249939` |
| tryout, this branch, as #809 shipped it | 5 | 44,539 B | `5b09557cf` |
| 5-entry scratch variant | 5 | 44,539 B | `5b09557cf` |
| the committed tryout | 25 | 51,399 B | `5b09557cf` |

**Per dormant entry: (51,399 − 44,539) / 20 = 343 B**, against about
17.6 KB per entry before. The vision's 1-entry figure was 42,351 B, so a
project now pays about one entry's load plus 343 B per extra entry.

Where the 343 B goes (`stacks.py` + `sdiff.py`, 5 → 25 entries, total
6,860 B):

| bytes | allocations | site | per entry |
|---:|---:|---|---:|
| 4,180 | 40 | `InventoryDerivation::walk_graph_node` clones: the playlist def's own entry records in the registry inventory (name, ref path, duration, fade) | 209 B |
| 1,280 | 20 | `SlotPath::parse` in `attach_projected_nodes_filtered`: each entry's path in the playlist node | 64 B |
| 1,280 | 0 (growth) | `attach_projected_nodes_filtered`'s own vector growth: the playlist node's full entry list (PD4) | 64 B |
| 120 | 20 | `SlotName::parse` under the entry paths | 6 B |

That is the playlist remembering its entries, which is what a dormant entry
is (D1): no tree node, no bindings, no parsed module, no shader.

The first frame's `retained` is 44,475 B at both 5 and 25 entries: the
frame never sees a dormant entry.

## 2. Two full tours (AC4)

**Method.** fw-emu, `lp-cli profile p6-v/tour --collect alloc --mode all
--max-cycles 35000000000`, on a scratch copy of the tryout whose tour step is
4 s and fade 0.5 s (not committed; the committed project tours at 30 s).
The step had to be that long for a reason worth knowing: under the alloc
collector every allocation is expensive in emulated cycles, and fw-emu's
clock follows cycles, so at a 1 s step the tour moved on before each new
entry's deferred compile ran and only the first shader ever compiled. At
4 s every entry compiles and renders (59 compiles, 732 frames, 58
switches). P3's `entry-unload` / `entry-load` markers window each switch.

The live heap is replayed from the trace (`proof/tour-switches.csv` has
every switch). "Retained at the end of a pass" is the live heap at the
first `entry-unload` of the next pass, the same point in the cycle each
time (the idle entry, Soft Noise, has played and is about to be replaced):

| point | live heap |
|---|---:|
| end of pass 1 (switch 26's unload) | 249,343 B |
| end of pass 2 (switch 51's unload) | 250,687 B |
| **difference** | **+1,344 B** |

**Target ≤ 256 B: missed as stated.** Attributed in full by diffing the
whole live heap, by call stack, at the end of switch 26's and switch 51's
load windows (same entry loaded at both): the only stack that differs is

```
+1344 B  +15 allocs  ResolveHost::time_product_phasor < TickResolver::time_product_phasor
                     < shader_node::resolve_or_default_input < NodeRuntime::produce
```

and everything else in the heap is **byte-identical** at the two points
(212,138 B outside that site at both). The site is the engine's
`TimebaseStore`: a shader's own phasors (`PhasorKey::Private { node, slot }`)
stay alive for `PHASOR_IDLE_TICKS` = 120 store ticks after their last
query, so an unloaded entry's phasors outlive the entry by 120 ticks and
then despawn. Traced at every switch, the site plateaus instead of
climbing:

| switches | live allocations at the site | live bytes |
|---|---|---|
| 1–11 (filling) | 6 → 66 | 370 → 2,808 |
| 12–35 | 56–71 | 2,516–2,954 |
| 36–58 | 61–81 | 2,952–4,000 |

The number lingering depends on how many ticks each entry happened to play
(which entries fall inside the last 120 ticks, and their phasor counts), so
the "same point" differs by an entry or so from pass to pass. Nothing
accumulates per switch: no tombstones, no leaked compiled code, no tree
slots (the tree and registry figures are identical to the byte). The
remainder is a bounded side store, not growth. The plan's "stated
tolerance" could be written as "≤ 256 B outside the timebase store's
120-tick despawn window"; that is a ruling for the gate, and this phase did
not change the window, the method, or the project to meet it.

**Peak per switch.** From each switch's unload to the next switch's unload,
the peak live heap sits **25,904 B** above the live heap just before the
unload, for every one of the 58 switches (switch 1, from the idle entry,
+25,976 B). The largest switch is therefore switch 1; the highest peak in
passes 1–2 is 289,759 B of fw-emu's 327,680 B heap. fw-emu keeps the
uploaded project files in its memory filesystem, so its absolute figures
include the files; the C6's (section 3) do not.

The chart is `proof/tour-heap.svg` in the planning directory: live heap at
every frame end, switches marked, the peak per switch as dots.

## 3. The emulated C6 (AC3)

**Method.** `lp-cli emu run --elf <fw-esp32c6, esp32c6+frame-dump,
release-esp32> --link-kind usb --monitor --time-grade t1 --timeout 280s`
(direct boot), then `lp-cli upload catalog/projects/playful-choker-tryout
serial:tcp://…` from the same tree. The free-heap figures are the
firmware's own heartbeats (`memory.freeBytes`, `memory.largestFreeBlock`),
read from the console after `lp-cli wire unpack`.

- **Upload:** the host-side all-entries check passed (all 25 entries load),
  the deploy went through, and the post-deploy check (`project.read` until
  a frame renders with no node error) printed `Project uploaded and
  running.`
- **It tours:** eight compiles in the run: the first entry, then one per
  30 s tour step (7 switches), each succeeding (18–28 ms compile, 3,020–3,960 B
  of code). No node error, no reboot, `no unmapped accesses`.

| when (emulated uptime) | free heap | largest free block |
|---|---:|---:|
| idle firmware, no project (5 s) | 216,604 B | 196,033 B |
| after the first compile (60 s) | **105,456 B** | **76,980 B** |
| after switch 1 (90 s) | 96,804 B | 58,575 B |
| after switch 2 (120 s) | 97,940 B | 66,399 B |
| after switch 3 (150 s), the lowest | 91,284 B | 56,750 B |
| after switch 4 (180 s) | 97,496 B | 59,612 B |
| after switch 5 (210 s) | 96,968 B | 61,539 B |
| after switch 6 (240 s) | 98,352 B | 68,781 B |
| after switch 7 (270 s), the end | **98,400 B** | **66,304 B** |

For scale, #809's own README measured the same rig before dormancy (lp-emu
`14ef539d7`): 88 KB free after the first compile at 5 entries, 24 KB at 8,
where `lp-cli upload`'s post-deploy read was refused. At 25 entries there is
now more room than there was at 5. The free heap moves from entry to entry
with the playing pattern's size (compiled code, uniforms); it does not
trend down across the switches.

## 4. Switch latency (reported, not gated)

**On the emulated C6** (`lp-emu:esp32c6:t1`, the 280 s run above). The pad
shows each switch as a stall in the WS281x frame stream (steady interval
9.40 ms): about 150 ms with no frame while the old entry unloads and the new
one loads, one frame (the held frame), then 35–48 ms more while the new
shader compiles, then the new pattern. From the last frame of the old
pattern to the first frame after the compile:

| switch | unload + load stall | compile stall | total |
|---|---:|---:|---:|
| 1 | 147.0 ms | 39.7 ms | 186.7 ms |
| 2 | 150.6 ms | 39.2 ms | 189.8 ms |
| 3 | 160.4 ms | 38.9 ms | 199.3 ms |
| 4 | 158.3 ms | 47.6 ms | 205.9 ms |
| 5 | 153.8 ms | 40.9 ms | 194.7 ms |
| 6 | 148.5 ms | 34.7 ms | 183.2 ms |
| 7 | 150.0 ms | 37.9 ms | 187.9 ms |

Median **≈ 190 ms** emulated t1. The lamps hold their last colours through
the stall (a WS281x string latches), so it reads as a pause, not a blackout.

**On fw-emu** (`--collect events`, no alloc collector, the 4 s-step tour
variant, esp32c6 cycle model, cycles ÷ 160 MHz): from the `entry-unload`
that begins a switch to the end of the first frame after the new shader's
compile, over 2 frames, 51 switches: min 149.8, median 160.6, max 178.7 ms.
The two emulators agree on the shape and roughly on the size.

**The fyeah-sign trigger (owed from P4, director DD11).**

1. *Emulated C6 with a pin-script press:* not measurable today. Pushing
   `catalog/projects/fyeah-sign` to the emulated C6 fails exactly as the
   open defect describes
   (`docs/defects/2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md`):
   the console stops after `project new after core project: 159k free`, and
   the upload fails with `device did not respond within 10.0s`. The press
   never gets a project to press. Not debugged here.
2. *fw-emu via `lp-cli profile`:* the profile workload drives frames only,
   and fw-emu's button driver is virtual with no way to inject a press, so
   a real press cannot be traced. **A proxy instead**: a scratch copy of
   fyeah-sign with a 3 s tour (not committed) switches idle ⇄ blast through
   the same unload → load → compile path a trigger takes. Idle → blast:
   **119.7–127.4 ms** (median ≈ 126 ms) from the switch's `entry-unload` to
   the end of blast's first frame, over 2 frames; blast → idle ≈ 148 ms.
   fw-emu, esp32c6 cycle model at 160 MHz, emulated. A real press adds the
   button's own 30 ms debounce (`stable_ms`), which this does not model.

So the answer for the gate's question 6 is: on the emulators, a triggered
blast now starts roughly 120–200 ms after the switch is decided, where
before it started on the next frame.

## 5. Layout: one module at two sites

The second stop of each pattern (entries 14–25) points at the same
`./modules/<pattern>/module.json` as the first. The registry handles one
def used at two sites: `lp-cli`'s `examples_valid` loads the tryout with
**every** entry resident at once (`load_from_root_with_every_entry_resident`)
and passes, and both tour traces load and unload every shared module twice
per pass with no failure and byte-identical heap outside the phasor store.
Only one entry is resident on a device at any time anyway.

A playlist entry's `node` is a path and nothing else (`NodeInvocation::Ref`),
so two stops sharing a module start from the same knob defaults; each entry
remembers its own knob values once they are changed (panel writers are
keyed by the entry's scope). Authored per-stop defaults would need per-stop
copies of `shader.json`, which this phase did not make; whether the second
stops should carry different authored settings is left to the final gate.

## 6. Heap-budget ratchet

- **Added:** `scripts/heap-budget-record/engine/catalog/projects/playful-choker-tryout.json`
  (`project-load` retained 51,399 B, the same figure as section 1).
- **Engine records** (`meteor`, `zook-dome`, `projects/test/basic`): all
  pass unchanged. None of them has a multi-entry playlist, so dormancy had
  nothing to reduce there.
- **C6 chip record:** re-measured after the `origin/main` merge. The merge
  had written main's figures as text; the merged tree boots at the branch's
  own (+20 B used). No firmware change in this phase. Logged in
  `docs/debt/heap-budget-record-churns-on-routine-changes.md`.

## 7. What the emulators do not cover

- **Wall-clock time.** Every time here is emulated: t1 counts one cycle per
  instruction; fw-emu's clock follows its cycle model. Neither is graded by
  a transcript for time. Do not gate on any of them.
- **A real press.** No button was pressed anywhere: the fyeah figure is a
  tour-driven proxy, and the C6 cannot take fyeah-sign at all yet (open
  defect above).
- **Flash reads of a real part.** The C6 run was a direct boot on the
  emulator's flash model; a board's flash timing is not modelled at t1.
- **Studio.** Nothing here went through Studio or its Play-mode picker;
  switches were tour-driven. The picker is covered by P7's stories and the
  final gate.
- **fw-emu's filesystem is in RAM**, so its absolute free-heap figures are
  lower than a device's by the project's file bytes; only the windowed
  `retained` figures compare across projects.

## 8. Reproducing

```bash
# AC2 (the scratch 5-entry variant: same dir, playlist cut to entries 1–5)
lp-cli profile catalog/projects/playful-choker-tryout --collect alloc --mode startup --max-cycles 800000000
lp-cli profile <5-entry copy> --collect alloc --mode startup --max-cycles 800000000

# AC4 (a copy with "tour": {"kind":"cycle","step_seconds":4,"fade_seconds":0.5})
lp-cli profile <tour copy> --collect alloc --mode all --max-cycles 35000000000

# AC3 and the C6 latency
cd lp-fw/fw-esp32c6 && cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,frame-dump
lp-cli emu run --elf <fw-esp32c6> --link 127.0.0.1:<port> --link-kind usb --monitor \
  --time-grade t1 --timeout 280s --console console.txt --dump-frames frames.jsonl
lp-cli upload catalog/projects/playful-choker-tryout serial:tcp://127.0.0.1:<port>
lp-cli wire unpack < console.txt | grep heartbeat
```

The analysis scripts (the tour replay, the whole-heap stack diff, the
per-site trend, the frame-stream stalls) and the chart and per-switch table
are in the planning directory's `proof/`.
