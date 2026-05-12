# Phase 5: Cleanup And Decision

## Scope Of Phase

Clean up temporary experiment code, document the final decision, and run the final validation gate. This is the phase that turns the spike into either a shipped fast-math change or a well-documented no-op decision.

Out of scope:

- Starting the future debug math probe.
- Starting a broad middle-end optimizer project.
- Chasing unrelated profile items such as allocator, memcpy, resolver borrowing, or fixture output.

## Code Organization Reminders

- Remove temporary benchmark-only code unless it remains useful as an explicit `test_jit_math_perf` lab.
- If the harness stays, keep it feature-gated and documented.
- Put final reports in `docs/reports/`.
- Keep code comments concise and tied to non-obvious math or ESP32 PMU details.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Cleanup checklist:

- Remove unused candidate kernels that were not selected.
- Remove stray `TODO(math-perf)` items unless they intentionally point to `future.md`.
- Make sure LUT regeneration instructions exist if LUT data ships.
- Make sure any retained firmware harness:
  - compiles under `test_jit_math_perf`,
  - is excluded from normal server firmware,
  - does not pull in `std`,
  - does not alter normal boot.
- Update comments/docs that still imply saturating math is the normal default.
- If examples no longer need `[glsl_opts]` to select fast math, either remove those blocks or leave a compatibility note, based on the final code direction.
- If some math-mode plumbing remains for tests, document that it is reference/probe infrastructure rather than product tuning.

Decision record:

- Add a final section to `docs/reports/2026-05-12-jit-math-perf.md`:
  - `Decision`
  - `Measured wins`
  - `Rejected candidates`
  - `Remaining risks`
  - `Follow-up work`

Acceptance criteria:

- Normal rendering uses the selected fast math path by default.
- Any selected trig approximation has explicit quality tests.
- Any selected division specialization has tests against the reference helper where exactness is claimed.
- Firmware builds with the compiler included.
- Steady-render profile shows either a real cycle reduction or a documented reason the obvious candidates did not pay off.

## Validate

Run the relevant CI gate locally before pushing:

```bash
just check
just build-ci
just test
```

Also run the shader-pipeline-specific commands from the repo instructions:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

If `rustup update nightly` is needed to match CI, run it before `just check`.
