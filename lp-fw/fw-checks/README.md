# fw-checks

`fw-checks` defines shared firmware checks for LightPlayer.

A firmware check is a runnable firmware mode used to answer a focused question:
does a feature work on target hardware, how much memory does a workload use, or
how fast is a firmware path on the actual device. Checks sit between unit tests,
filetests, profiles, and demos: they are small, named scenarios with known
features, markers, traces, and optional structured records.

The crate is `no_std` by default so firmware crates can depend on it. Enable the
`std` feature for host-side reporting used by `lp-cli fwcheck`.

## Adding A Check

1. Add a module under `src/checks/<name>/`.
2. Add a `FwCheck` variant and a `FwCheckConfig` entry.
3. Add a feature flag in the target firmware crate, such as `fw-esp32`.
4. Add a small firmware harness that initializes the board/logging and delegates
   to shared check code where possible.
5. Emit structured records with `fw_checks::emit_record_json(...)` when the
   check has measurements worth summarizing.

The `lp-cli fwcheck` command reads the shared config to build, run, capture, and
report checks.

```bash
cargo run -p lp-cli -- fwcheck list
cargo run -p lp-cli -- fwcheck run esp32c6 shader-compile-stress --note baseline
```

By default, `fwcheck run` shows a compact build/flash/run progress view and an
inline summary report. Pass `--verbose` to stream raw build, flash, and firmware
serial output while the check runs.

Hardware runs write a timestamped directory under `traces/` containing:

- `trace.txt`: the full normalized serial log
- `records.jsonl`: structured records extracted from the mixed serial log
- `report.txt`: a short host-side summary when the check has a reporter
