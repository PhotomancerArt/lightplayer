---
status: OPEN — root cause narrowed to the graphics stage, mechanism not yet named
found: 2026-09-10      # emulator plan two, M6 (the walk with no board); Yona hit it by hand on 2026-09-09 as "F1"
area: lp-emu/esp/lp-emu-esp32c6, lpc_engine graphics construction
class: emulator-fidelity
related: [lp2025/2026-09-08-0838-emulator-plan-two-web-serial-shim/m6-walk-with-no-board.md]
---
# The emulated C6 spends 8 s of guest time where silicon spends 0.2 s, and the RWDT resets it mid-push

**Symptom** — pushing `catalog/projects/fyeah-sign` to an emulated C6 fails,
always, at the same place, and the board reboots:

```
[INFO] lpa_server::project: [mem] project new after core project: 188k free / 129k used
push failed: transport error: Transport error: device did not respond within 5.0s×2
[INIT] Initializing board...
[INIT] Board initialized, starting runtime... (main stack 71152 B)
[RECOVERY] boot: cause=power-on level=green safe_mode=false prior_boot_complete=true
[RECOVERY] RWDT armed: boot 30000 ms, runtime 8000 ms
```

That is Yona's hands-on trace of 2026-09-09, verbatim, and it reproduces on
current `main` (`2be6b6235`) on **both** paths into the board — Studio's push
over the Web Serial shim in a browser, and `lp-cli upload <dir>
serial:ws://…/bytes` from a terminal. So it is not Studio's, not the
polyfill's, not the door's: **the board goes quiet, and the board's own
console — which `emu serve --console-dir` records whether or not anything is
listening — stops at the same line either way.**

Left alone, it loops: the failed push has already written the project and set
it as the startup project, so every subsequent boot loads it and dies the same
way.

**Cause, as far as it is measured** — the RTC watchdog fires, honestly, on a
guest that really has stopped feeding it for eight seconds of its own time:

```
$ lp-cli emu run --elf …/fw-esp32c6 --flash <the board's flash> --timeout 120s
emu: the chip asked to reset (LP_WDT stage 0 (ResetSystem), into App)
     — 8279541 us emulated, 1308312919 instructions, 1675 bytes on the console
emu: no unmapped accesses
```

8.28 s of emulated time for 1.308 G instructions is ~6.3 ns each: the model is
charging about one cycle per instruction at 160 MHz, which is what it means
to. Nothing is unmapped. `LP_WDT` is modelled faithfully — `periph/lp_wdt.rs`
exists precisely so "a firmware that stops feeding dies here as it does on
silicon" — and the firmware asked for `runtime 8000 ms`. Every layer is doing
what it says.

**What is wrong is the amount of work.** The step that overruns is the one
between two adjacent lines of `Project::new`, and the committed silicon
fixture for the same scenario and the same project has both of them, with
wall-clock stamps:

| | `after core project` → `after graphics` | whole `load_project` |
|---|---|---|
| silicon (`s3-current-fw-valid-project.jsonl`, 2026-08-03, a real XIAO C6) | **0.199 s** | 0.204 s |
| emulated C6 (`lp-emu:esp32c6:t1`, 2026-09-10) | **never completes**; ≥8 s guest, then RWDT | — |

Same binary, same project, same modelled clock, ~40× the guest time. So the
emulated chip is executing roughly forty times the instructions a real one
needs to build this project's graphics stage.

**It is not "the emulator is slow at shaders".** A small project goes through
cleanly on the same board, with the shader compiler reporting single-digit
milliseconds of guest time:

```
$ lp-cli upload catalog/projects/peach-1d serial:ws://…/board/c6-a/bytes
[shader-node] compilation succeeded (elapsed=13ms, lpir_inst_count=128, final_code_size=2108 bytes, float=fixed)
[shader-node] compilation succeeded (elapsed=10ms, lpir_inst_count=107, final_code_size=1924 bytes, float=fixed)
Project uploaded and running.
```

So the cost scales with something `fyeah-sign` has and `peach-1d` does not —
the obvious candidate being its 8.8 KB `fyeah.map2d.json` and the per-lamp
work the graphics stage does from it, but **that is a guess and this defect
does not claim it.**

**The next diagnostic step, not taken here** — the emulator has no hot-PC or
MMIO census facility on the `emu run` path, so nothing in the tree can
currently say *where* those 1.3 G instructions go. Adding one (a PC histogram
over a bounded window, or a per-peripheral access count) is the measurement
that would name the mechanism in one run, and it would pay for itself
immediately: "the guest polls a register the model answers slowly" and "the
guest genuinely does forty times the work" are different bugs with different
owners, and today they are indistinguishable from outside.

**What it blocks** — any emulated walk whose project is heavier than the
watchdog's eight seconds. M6's `s3` and `s7` push `fyeah-sign` because that is
what the silicon capture pushed and the comparison is the point; both trip
this. The full walk uses `peach-1d` for its upload step and says so.

**What it does not mean** — nothing here is evidence against silicon. A real
board does this in 0.2 s and the fixture proves it. This is the emulator being
unfaithful in the one dimension the plan's own anti-oracle warned about from
the other side: *a socket is not deterministic, assert outcomes not cycles* —
and here an outcome (the watchdog) turned out to be a function of cycles after
all, because the firmware's own watchdog reads the guest clock.
