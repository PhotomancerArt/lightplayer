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
