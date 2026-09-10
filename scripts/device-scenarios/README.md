# Device scenarios — the golden-trace library (multi-device roadmap M8)

Each `<id>.json` here puts a real board into a **known state** and captures
what Studio observes as a committed trace fixture. The runner holds your
hand end to end:

```bash
just device-scenario                 # status table: specified / captured / pending
just device-scenario run s1-blank-flash --port /dev/cu.usbmodem11201
```

`run` checks preconditions, runs the automated setup **in the foreground**
(a backgrounded espflash has died silently mid-write before), releases the
port, prints the exact browser steps, and starts a local HTTP capture sink.
Open the running `just studio-dev` URL with the printed
`?capture-sink=…` parameter appended — Studio's device event log (M0)
streams every lifecycle event and raw serial line to the runner, which
writes `lp-app/lpa-link/testdata/device-traces/<id>.jsonl` as it arrives
and validates it against the spec's `expect` list when you press Enter.

**Commit validated captures.** They are fixtures, not story PNGs — the
CI-canonical-baseline rule does not apply. Three consumers depend on them:

1. `lp-app/lpa-link/tests/trace_replay.rs` replays the raw `rx` lines
   through the boot classifier and pins it against real bytes.
2. The M9 virtual device's fidelity gate diffs its output against them.
3. Live-jank diagnosis-by-diff: a trace exported from a misbehaving
   session (the card's "Copy device trace" button) compares against the
   golden capture for that scenario.

## Spec format

```jsonc
{
  "id": "s1-blank-flash",
  "title": "Blank flash (fully erased)",
  "board": "C6 or S3 (native USB)",       // what to plug in
  "needs": [],                             // scenarios whose end-state this starts from
  "expect": ["state:blank-flash"],         // kind:value matchers; "a|b" = either
  "setup": [                               // automated, run in order, foreground
    { "describe": "…", "run": "espflash … --port {port}", "verified": false }
  ],
  "manual": ["…exact browser steps…"]      // printed one by one at hand-off
}
```

`verified: false` marks a first-run command the runner warns about — watch
it once, fix the spec if it is wrong, then flip the flag. `{port}` is
substituted from `--port` or the interactive picker (which uses the
passive `hardware list`; it never opens or resets a port).

## Hard rules the runner enforces (each has broken a sitting)

- Nothing touches the serial port after hand-off — setup strictly precedes
  the browser taking the port.
- Flash in the foreground; never `--monitor` in setup (it would hold the
  port the browser needs).
- Port listing is passive (`hardware list`), never `--probe`, during a
  sitting.

---

# The emulated lane

```bash
just device-scenario run s1 --emu --shots /tmp/shots
just device-scenario run --emu            # every scenario that has one
just device-scenario check-guard          # the overwrite guard, proved by trying
```

The same scenarios, **with no board at all** (emulator plan two, M6). A
scenario has two halves and each one changes:

| | on a board | with `--emu` |
|---|---|---|
| `setup:` | `espflash` puts a chip into a known state | `lp-cli emu serve` holds a board on a **fresh `--state-dir`**. "Erase the entire flash" is not a command any more; it is a board that was never written. |
| `manual:` | a person reads the steps and clicks | `emulated.steps`, driven in **headless** Chrome |
| the port | `/dev/cu.usbmodem…`, picked from `hardware list` | a board id on a door. `hardware list` is never called in this lane and no `lsof` guard applies. |

**One `emu serve` per scenario**, on an ephemeral port, so `blank` means blank.
The **page** still comes from the worktree's own canonical `just studio-dev` —
never a substitute server — and the two are joined by `?emu=<url>`, which
composes with `?capture-sink=` because nothing reads anything else's flag.

## The `emulated` block

```jsonc
"emulated": {
  "note": "…why this board spelling is the honest counterpart of the setup above…",
  "boards": ["c6-a=blank,kind=rom-up"],   // `--board` args; `{fw}` = the packaged C6 ELF
  "steps": [
    { "do": "connect", "board": "c6-a", "describe": "mirrors the manual: line" },
    { "do": "settle",  "words": ["Ready"] },
    { "do": "shot",    "name": "s2-ready" },
    { "do": "await",   "match": "state:ready" }
  ]
}
```

Verbs: `connect`, `cancel-connect`, `settle`, `text`, `project`, `push`,
`flash`, `card`, `detach`, `attach`, `registry`, `shot`, `mark`, `await`.

- **`await`** is the only step that is about the TRACE, and it goes last on
  purpose: everything before it has already been observed on the card, so a
  failure there is about the trace and not about the walk.
- **`mark` + `"fresh": true`** draws a line under everything captured so far.
  `s8` needs it: its `expect` is `flow:connecting`, which the *first* connect
  already produces, so without it a lane would pass the spec while proving
  nothing about the re-pick.

## Rules of the emulated lane

- **THE GUARD.** An emulated run can only ever write a name containing
  `.emu.` — `<id>.emu.jsonl`, or `<id>.emu.failed.jsonl` for a finding. It is
  code (`assertLaneMayWrite`, called at every write site), not a convention,
  because a board's bytes are not reproducible and a clobbered silicon fixture
  is gone. `check-guard` proves it by trying and prints the silicon fixtures'
  sha256 so a before/after is checkable rather than asserted.
- **No prompts, and no "keep as the golden fixture".** There is no person at a
  chooser, so there is nothing to ask; a capture that misses its `expect` list
  is filed as a FINDING and says so. The silicon lane's `k` branch does not
  exist here.
- **The `expect` matchers do not change.** The whole claim is that an emulated
  run satisfies the same list a board satisfied. A matcher loosened to make
  the emulator pass is the failure this lane exists to make visible.
- **Headless, always.** The silicon lane's `spawnSync("open", [url])` opens a
  visible window; the emulated lane never does.
- **Nothing polls and nothing times.** Page-side waits are MutationObserver
  promises; trace-side waits are the sink receiving a record. An agent-driven
  tab is throttled to ~1 Hz, so a duration measured here would be a
  measurement of the throttle.

## Diffing a capture against its silicon fixture

```bash
node scripts/emu/trace-diff.mjs \
  lp-app/lpa-link/testdata/device-traces/s1-blank-flash.jsonl \
  lp-app/lpa-link/testdata/device-traces/s1-blank-flash.emu.failed.jsonl
```

It aligns the state/flow/pool/mgmt/sync sequences, counts anomalies per side,
and compares the *set* of normalised boot-line shapes. It never compares wall
clock, session id, endpoint id or line counts, and it prints that list every
run. `docs/reports/2026-09-10-studio-walk-with-no-board.md` is the worked
example, with every difference in all six scenarios named and classified.

## ⚠️ Read this before running either lane today

Eight of the ten device-event record kinds lost their producer in `0a1b51d13`
(2026-08-25) — `state`, `flow`, `rx`, `tx`, `mgmt`, `sweep`, `sync` and
`anomaly` are emitted by nothing in the repository, and every committed
fixture predates that by three weeks. **Five of the six scenarios with a
fixture name matchers nothing can satisfy, on a board or off one.** Filed as
`docs/defects/2026-09-10-eight-of-ten-device-event-kinds-lost-their-producer.md`.
Until it is fixed, a capture sitting of any kind produces findings rather than
fixtures, and that is the instrument's fault rather than the sitting's.
