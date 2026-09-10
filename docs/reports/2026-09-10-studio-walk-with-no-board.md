# The Studio device walk, run with no board

**Date** 2026-09-10 · **Plan** `2026-09-08-0838-emulator-plan-two-web-serial-shim`
(M6, acceptance criterion 6) · **Commands** `just studio-dev-emu`,
`just walk-no-board`, `just device-scenario run <id> --emu` ·
**Scripts** `scripts/emu/walk-no-board.mjs`, `scripts/emu/emulated-lane.mjs`,
`scripts/emu/studio-driver.mjs`, `scripts/emu/trace-diff.mjs` ·
**Twin of** a hardware capture sitting (`just device-scenario run <id> --port …`)

This is the record of the first time Studio's whole device walk — flash,
connect, identify, upload, unplug, plug back in — was answered with nothing
plugged into anything. Read **§6, What this does not test** and **§7, The two
defects this found** before quoting anything from the middle: the walk went
green and the *diff* is where the milestone's real content is.

---

## 1. The commands, in order, with what they printed

```
$ just studio-dev-emu
Emulated boards:  http://127.0.0.1:28206/boards   (image: target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6)
Emulator console: target/emu-serve/studio-dev/<board>.console.log
Open Studio with: ?emu=ws://127.0.0.1:28206
  predicted:      http://127.0.0.1:37724/?emu=ws://127.0.0.1:28206
  (the URL studio-dev prints below is the source of truth)
```

The two ports are read, never computed. The door takes an ephemeral one and
prints it; the dev server takes this worktree's hashed one.

```
$ just walk-no-board

THE WALK WITH NO BOARD
  emulated board   c6-a=blank,kind=rom-up  (nothing is plugged into anything)
  door             http://127.0.0.1:63607/boards   (pid 21636)
  studio           http://localhost:37724/
  capture sink     http://127.0.0.1:63608/ingest
  the page         http://localhost:37724/devices?emu=ws%3A%2F%2F127.0.0.1%3A63607&capture-sink=http%3A%2F%2F127.0.0.1%3A63608%2Fingest
```

**`?emu=` and `?capture-sink=` compose**, which was checked in the first ten
minutes and is visible in that URL and in the page's own console — two
separate parsers over one query string, neither reading the other's flag:

```
[info] [emu] navigator.serial is a shim over ws://127.0.0.1:28206 (3 board(s))
[info] [studio/lpa_studio_web::device_events_io] device-event capture streaming to http://127.0.0.1:9/ingest
```

Every browser in this milestone ran `--headless=new`. Nothing was ever
fronted, and the existing runner's `spawnSync("open", [url])` — which opens a
visible tab — is not on the emulated path at all.

## 2. The walk, step by step

The board is `kind=rom-up` with an empty flash file: the mask ROM finds no
image at the reset vector, which is what a blank chip is, and it is the only
board kind Studio's esptool-js flow can actually write.

```
  shim installed over 1 board(s): c6-a (a0:f2:62:87:b4:8c)

— flash: Studio flashes the packaged firmware into a blank board
  ✓ 81 device-event record(s): journal=81
— connect: the session comes back on the flashed board
  ✓ 2 device-event record(s): journal=2
— identify: the board says what it is, in its own hello
  identity: fw-esp32c6 2be6b6235aad (dirty)   mac: a0:f2:62:87:b4:8c
  ✓ 508 device-event record(s): journal=508
— upload: push Peach (1D) onto the board from the gallery
  the board:  Project loaded
  Studio:     Note(ActivityEnded { kind: Push, outcome: Succeeded { summary: "project sent to studio — the board is running studio" } })
  ✓ 174 device-event record(s): journal=174
— detach: the cable comes out mid-session
  ✓ 0 device-event record(s): (none)
— reattach: the cable goes back in
  after the replug the card reads: "Ready"
  ✓ 0 device-event record(s): (none)

=== the walk, step by step
  ✓ flash     805 record(s)   walk-1-flash.png
  ✓ connect   0 record(s)   walk-2-connect.png
  ✓ identify  0 record(s)   walk-3-identify.png
  ✓ upload    174 record(s)   walk-4-upload.png
  ✓ detach    0 record(s)   walk-5-detach.png
  ✓ reattach  0 record(s)   walk-6-reattach.png

  door's live registry: c6-a flash=loaded boot=rom-up reboots=3 state=running

✓ the walk finished: flash → connect → identify → upload → detach → re-attach, with no board.
```

Three consecutive runs, identical in every line above.

Screenshots, one per step, at `target/walk-no-board/shots/walk-{1..6}-*.png`
(archived with the milestone's G2 packet). The full trace is
`target/walk-no-board/walk.jsonl` and the per-step index —
which records each step produced — is `target/walk-no-board/walk-steps.json`.

**Three of those six steps are gated on the BOARD's words, not Studio's**, and
that was a deliberate choice each time:

- **flash** — the card says `Flashing firmware…` and then stops saying it, and
  the door's live registry reads `flash=loaded boot=rom-up reboots=3`. The
  door's `flash` word answers "does an image magic sit at the reset vector"
  (DD34), recomputed per flush, so `loaded` here means what Studio wrote is at
  the vector the ROM jumps to. `reboots=3` is esptool's reset dance plus its
  `hard_reset`. **Both halves of that wait are load-bearing and the first
  version of this step had neither**: "the card stopped saying `needs
  firmware`" is satisfied the instant the card switches to `Flashing
  firmware…`, so the step returned mid-flash, screenshotted a progress bar,
  and every step after it raced a flash that was still running. That artefact
  produced three spurious `device did not respond within 5.0s` push failures
  before it was found; they are not in this record because they were not
  real.
- **identify** — `fw-esp32c6 2be6b6235aad (dirty)` on the card is the board's
  own hello. It is the build this Studio serves, so the board is running what
  Studio wrote and says so itself.
- **upload** — `Project loaded` is the firmware's line, off the wire. Two
  earlier versions of this step passed on weaker predicates and both are named
  in the code so nobody writes them again: "the page mentions the project" is
  satisfied by the *chooser's own button* the moment the project is picked,
  and "Nothing loaded went away" is satisfied when the push *starts*.

## 3. The upload step, and what the project's size decides

`Peach (1D)` pushes cleanly: the board says `Project loaded`, Studio says
`Succeeded { summary: "project sent to studio — the board is running studio" }`,
three runs out of three.

**`Fyeah Sign` does not, and cannot.** Same walk, same board, same code, only
the project changed:

```
$ WALK_PROJECT="Fyeah Sign" just walk-no-board
— upload: push Fyeah Sign onto the board from the gallery
  ✗ waiting for the board to say `Project loaded`: wait deadline
  door's live registry: c6-a flash=loaded boot=rom-up reboots=9 state=running
```

`reboots=9` against the clean run's `reboots=3` is the whole story: the board
reset six more times inside the step. That is **F1, Yona's push hang**, filed
as
`docs/defects/2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md`
and summarised in §7. It is the project's graphics stage overrunning the
firmware's own eight-second RTC watchdog, on an emulated chip that needs ~40×
the guest time silicon needs for that step — and once the failed push has set
the startup project, every subsequent boot loads it and dies the same way,
which is the reboot loop those nine count.

**The upload legs agree** on what lands, which is invariant 9. `lp-cli upload
catalog/projects/peach-1d serial:ws://…/board/c6-a/bytes` writes the same
`/projects/studio/…` files, compiles the same two shaders (`elapsed=13ms` and
`elapsed=10ms` of guest time, `final_code_size=2108` and `1924` bytes) and
prints `Project uploaded and running.` Both legs fail on `fyeah-sign`, and
fail at the same line.

### A number worth having, and a claim retracted

The emulator executes about **7.6 M instructions per wall second** while
modelling a 160 MHz CPU: `20000000 us emulated, 172070742 instructions` in
`22.71 s` of wall time on an idle board, and `1308312919 instructions` for
`8279541 us emulated` on a busy one. So when the guest is *busy* it runs
roughly **21× slower than real time**; when it is idle, guest time jumps and
it looks real-time.

An earlier draft of this record used that number to claim Studio's fixed
five-second device deadline made *every* emulated push report failure. **That
was wrong and is retracted.** The failures it rested on came from this walk's
own too-loose flash gate (§2), not from the deadline; with the gate fixed,
`Peach (1D)` clears Studio's five seconds every time. The deadline is a real
constraint on a board this slow and it will bite on a bigger project — but
nothing here has measured where.

## 4. The six scenarios, with no board

`just device-scenario run <id> --emu`. Each gets its own `lp-cli emu serve` on
a fresh `--state-dir` (which is what "erase the flash" means when there is no
chip) and the worktree's own `just studio-dev` for the page.

| scenario | the walk | its `expect` | verdict |
|---|---|---|---|
| `s1-blank-flash` | connect → card reads **Blank flash — needs firmware**; door `flash=blank boot=rom-up` | `state:blank-flash` | ✗ FINDING |
| `s2-fresh-fw-no-lpfs` | connect → **Ready**, hello identity on the card | `state:ready` | ✗ FINDING |
| `s3-current-fw-valid-project` | connect → Ready → push Fyeah Sign | `state:ready` | ✗ FINDING |
| `s7-unplug-mid-op` | connect → Ready → push → cable out → cable in | `state:gone` | ✗ FINDING |
| `s8-repick-granted-port` | connect → Ready → re-pick the SAME board | `flow:connecting` | ✗ FINDING |
| `s9-two-boards` | connect c6-a → Ready → connect c6-b → Ready; **both MACs on the page at once** | `pool:install` | ✗ FINDING |

**Every walk succeeded. Every `expect` list failed.** That is not the emulator
failing: five of those six matchers name record kinds that no code in the
repository emits any more, and the sixth (`pool:install`) survived as a kind
but changed what triggers it. §7 has the evidence. **A hardware capture
sitting held today would fail identically**, because the producer is in
`lpa-studio-core`, above the link layer, where the lane makes no difference.

Not run, with reasons:

- **`s4-old-firmware` and `s4b-no-hello`** — an emulator-first capture of
  these was the brief's stated bonus, and it is not taken. `just fixture-fw
  old-proto|no-hello` builds the boards, so the *mechanism* exists; what does
  not exist is any way for such a capture to be worth having while
  `state:incompatible(proto-mismatch)` and `state:incompatible(no-hello)` are
  unproducible. A first-ever fixture that could not carry the evidence its
  spec asks for would be the worst kind of artefact to create.
- **`s5-foreign-firmware`** — needs WLED on an S3; there is no emulated S3.
- **`s6-corrupt-lpfs-project`** — the spec itself says SKIP until a wire-level
  corruption path exists. Nothing has one.
- **`s10-classic-ch34x`** — plan three.

### Corrections to the brief's scenario table

The brief's table is right in every particular that was checkable, including
the eleventh spec (`s4b`) the plan never counted and the two `.failed.jsonl`
findings. Two things it could not have known:

1. `s9`'s emulated form is **two C6s, not C6 + S3**. There is no emulated S3.
   PD4's claim is about two boards being two identities rather than about two
   chips, and two identical chips with different eFuse MACs is the *harder*
   case for the defects the scenario exists to catch. Both MACs are on the
   page at once (`a0:f2:62:87:b4:8c` and `…:8d`), both cards reach Ready, and
   the door reports two independent boards. PD4 holds.
2. "the emulated run validates against the spec's own `expect` list" is not
   achievable by any lane today, and the reason is a Studio regression rather
   than anything about emulation.

## 5. The diff, per scenario, with every difference named

`node scripts/emu/trace-diff.mjs <silicon>.jsonl <emulated>.emu.failed.jsonl`.
The tool aligns the state/flow/pool/mgmt/sync sequences, counts anomalies per
side, and compares the *set* of normalised boot-line shapes. It never compares
wall clock, session id, endpoint id or line counts, and it says so in its own
output.

### `s1-blank-flash`

```
  record census
    flow           6       0
    journal        0      75
    pool           1       0
    rx            89       0
    state          2       0
    sweep          1       0

  state sequence — 2 difference(s)
   - state  · → booting
   - state  booting → blank-flash

  anomaly count  silicon=0  emulated=0

  distinct boot-line SHAPES (digits → #):  silicon=6  emulated=4  shared=2
    only in silicon: 4
      - Saved PC:0x#
      - ffffff
      - invalESP-ROM:esp32c6-#
      - rst:0x# (USB_UART_HPSYS),boot:0x# (SPI_FAST_FLASH_BOOT)
    only in emulated: 2
      + ESP-ROM:esp32c6-#
      + rst:0x# (POWERON),boot:0x# (SPI_FAST_FLASH_BOOT)
```

Every difference, classified:

| difference | class | why |
|---|---|---|
| `state`, `flow`, `pool`, `sweep` present on silicon, zero on the emulated side | **defect** | `docs/defects/2026-09-10-eight-of-ten-device-event-kinds-lost-their-producer.md`. Nothing to do with either board. |
| `rst:0x# (USB_UART_HPSYS)` vs `rst:0x# (POWERON)` | **modelled** | The silicon capture followed `espflash erase-flash`, which resets the chip over USB-serial. The emulated blank board was never written and never reset — it is at power-on. Each chip reports its own reset cause correctly. |
| `Saved PC:0x#` (silicon only) | **modelled** | A consequence of the line above: the ROM prints it on a non-power-on reset. |
| `invalESP-ROM:esp32c6-#` and `ffffff` (silicon) vs `ESP-ROM:esp32c6-#` (emulated) | **modelled, and the interesting one** | These are the same two lines. On silicon `invalid header: 0xffffffff` and `ESP-ROM:esp32c6-20220919` arrive *interleaved* and the classifier sees `inval` + `ESP-ROM…` + `ffffff`. That is `docs/defects/2026-08-02-serial-line-interleaving.md` happening on the wire. **The emulator delivers whole lines and never reproduces it.** See §6: a whole class of defect stays on hardware. |

### `s2`, `s3`, `s7`, `s8`, `s9`

The boot-line differences are the same set in all five, so they are named once.
`s2`'s output, elided to the boot-line half:

```
  distinct boot-line SHAPES (digits → #):  silicon=62  emulated=32  shared=26
    only in silicon: 36
      - [I (#) boot: ESP-IDF v5.#-beta1-#-gea5e0ff298-dirt 2nd stage bootloader]
      - [I (#) boot: Partition Table:] … [I (#) boot: End of partition table]
      - [I (#) esp_image: segment #: paddr=# vaddr=# size=…] × 6
      - [I (#) boot.esp32c6: SPI Flash Size : 4MB / SPI Mode : DIO / SPI Speed : 40MHz]
      - SPIWP:0x#, mode:DIO clock div:#, load:0x#,len:0x#, entry 0x#
      - rst:0x# (USB_UART_HPSYS),boot:0x# (SPI_FAST_FLASH_BOOT), Saved PC:0x#
      - [RECOVERY] boot: cause=user-reset …
      - [INIT] fw-esp32 initialized … commit=1ce45a201a29 dirty=false
      - [INIT] Board initialized, starting runtime...
      - [perf] frame=# fps=# elapsed=5001ms recv=0ms tick=1ms send=0ms total=1ms
    only in emulated: 6
      + [WARN] … [FS] Mount failed (filesystem corrupt), formatting partition...
      + [INFO] … [FS] Formatted and mounted fresh filesystem
      + [INIT] Board initialized, starting runtime... (main stack # B)
      + [INIT] fw-esp32 initialized … commit=2be6b6235aad dirty=true
      + [RECOVERY] boot: cause=power-on …
      + [INFO] … [stack] heartbeat: high-water # B of # B (# B headroom)
```

| difference | class | why |
|---|---|---|
| the whole ESP-IDF **second-stage bootloader** log, the partition table, the `esp_image` segment lines, `SPIWP`/`mode:DIO`/`load:`/`entry` | **modelled — a lane choice, and a reversible one** | The emulated `s2`/`s3`/`s7`/`s8`/`s9` boards are `kind=elf`: the image is loaded at its entry point, so the ROM's second stage never runs. A `kind=rom-up` board does run it — the walk in §2 is rom-up and its trace has the whole boot. The lane could take rom-up for these too, at the cost of a flash step per scenario; it takes elf because those five scenarios are about a board that is *already running firmware*. |
| `cause=user-reset` + `rst:0x# (USB_UART_HPSYS)` vs `cause=power-on` | **modelled** | Same as `s1`: silicon was reset by the host's flash; the emulated board powers on. |
| `commit=1ce45a201a29 dirty=false` vs `commit=2be6b6235aad dirty=true` | **cosmetic** | Two firmware builds five weeks apart. The fixtures are 2026-08-03. |
| `[stack] heartbeat: high-water …` and `(main stack # B)` (emulated only) | **cosmetic** | `fw_esp32c6::stack_probe` did not exist in the August build. |
| `[FS] Mount failed (filesystem corrupt), formatting partition...` + `Formatted and mounted fresh filesystem` (emulated only) | **modelled — and a strength** | The emulated flash file is `0xff` end to end (verified: `xxd` of a fresh `c6-a.flash.bin`), so lpfs finds no filesystem and formats one. The silicon board also had its lpfs erased, but it formatted on the boot that followed `espflash flash --after hard-reset`, *before* the capture tab opened. The emulated lane captures from power-on; the silicon sitting could not. |
| `[perf] … elapsed=5001ms … tick=1ms … total=1ms` vs `elapsed=5000ms … tick=0ms … total=0ms` | **modelled** | The same line. The emulator's clock is exact; silicon jitters by a millisecond. Nothing here is asserted on. |
| `anomaly count silicon=0 emulated=0` in all six | **no difference** | Worth stating: the interleaving in `s1` produced garbled *lines*, not frame-parse anomalies, so neither side records one. |
| `sync empty` × 3 (silicon `s2` only), `pool install (Device)` | **defect** | Producer gone; see §7. On `s9` the silicon fixture has `pool install (Device)` twice — one per board — and today's `pool install` fires on a device **lens** open instead, with detail `device lens <uid>`. Same kind, different trigger. |

**No difference in any of the six is unexplained.**

## 6. What this walk does not test

The honest residue, and it has not moved:

- **Chromium's own USB stack.** A polyfilled `navigator.serial` grants itself,
  so the walk needs no chooser, no permission, no MCX profile, no
  `just serial-grant` and no bench port block — one of the shim's quieter
  wins, and none of it was wired back in. The cost is that none of it is
  tested: minutes-long device-loss reporting, Brave's grant revocation on
  reload, and the real chooser stay on hardware.
- **The byte-level interleaving a real USB-serial path exhibits.** `s1`'s diff
  is the evidence: silicon chopped `invalid header` and `ESP-ROM:` into each
  other; the emulator never does. The defect class that
  `docs/defects/2026-08-02-serial-line-interleaving.md` names cannot be
  reproduced here.
- **Anything analog or radio.** Unchanged by this plan.
- **Wall-clock behaviour.** The emulator is ~21× slower than silicon while the
  CPU is busy, so any product timeout is being tested against the wrong
  number — in both directions. A timeout that is too tight will bite here and
  not on a board; a timeout that is too loose will never be exercised here at
  all.
- **A half-open byte client.** M5 measured zero 409s across seven walks and
  this milestone saw none either; the case where a client dies without closing
  is still unmeasured, not ruled out.

## 7. The two defects this found

Both are filed; neither is fixed here, and the `expect` matchers are unchanged.

1. **`docs/defects/2026-09-10-eight-of-ten-device-event-kinds-lost-their-producer.md`**
   — `state`, `flow`, `rx`, `tx`, `mgmt`, `sweep`, `sync` and `anomaly` are
   emitted by nothing in the repo. They went in `0a1b51d13` (2026-08-25),
   three weeks after every committed fixture was captured. The scenario
   library's matchers are therefore unsatisfiable on any lane, and
   `trace_replay.rs` passes only because the *old* fixtures still carry the
   `rx` lines nothing writes any more.

2. **`docs/defects/2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md`**
   — **F1, Yona's push hang, reproduced.** Pushing `fyeah-sign` to an emulated
   board fails at the same line every time and the board reboots, on *both*
   paths into it. `lp-cli emu run` names the reset: `LP_WDT stage 0
   (ResetSystem) — 8279541 us emulated, 1308312919 instructions`, no unmapped
   accesses. The silicon fixture did the same step in **0.199 s**. Same
   binary, same project, same modelled clock, ~40× the guest time. The
   mechanism is not named: the emulator has no hot-PC or MMIO census on the
   `emu run` path, and adding one is the measurement that would settle it in a
   single run.

## 8. The residue — what could not be run, and why

- **No emulator-first `s4`/`s4b` capture** (§4).
- **The reference image had to be built rather than copied.** The sanctioned
  shortcut — copy `d6cfaa205-esp32c6+server+radio` from a sibling worktree
  after checking its PROVENANCE — was taken, verified
  (`features=esp32c6,server,radio commit=d6cfaa2051aeab553… spike=none`,
  sha256 `cde3680c…` matching its own `SHA256SUMS`) — and then removed under
  this agent by the nightly `~/bin/cargo-clean.sh` sweep, along with every
  other copy on the machine. See the deviations in the milestone report.
- **The `pool:install` kind is live but its trigger moved**, and no scenario
  step was added to fire it, because adding one would change the procedure the
  silicon fixture recorded and make the two traces incomparable.
