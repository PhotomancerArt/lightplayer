# Phase 8: Cleanup, validation, and m6 known-limitation note

> Read `00-notes.md` and `00-design.md` for shared context.
> This is the final phase. After it, the dispatcher writes
> `summary.md`, archives the plan dir, and commits.

## Scope of phase

Sweep the diff for stragglers, run full project validation, and add a
short forward-pointer to m6 noting that JIT symbols may be stale if the
JIT relinks (since `SYSCALL_JIT_MAP_UNLOAD` is not yet implemented).

### In scope

- Walk the full diff (`git diff --stat`, then `git diff` per file) for:
  - Stray `TODO`s introduced by phases 1-7 that are not in the
    deliberately-deferred set.
  - `dbg!`, `println!` outside test code.
  - Commented-out code.
  - `#[allow(...)]` added to silence warnings.
  - `#[ignore]` added to make tests pass.
  - Scratch files in the repo (`*.scratch`, `tmp_*`, `.bak`, etc.).
- Run `cargo +nightly fmt --all` (per `.cursorrules`).
- Run the project-wide validation gate.
- Add a short paragraph to one of:
  - `docs/roadmaps/2026-04-19-cpu-profile/m6-*.md` (if a m6 roadmap
    file exists), or
  - a new `docs/future-work/2026-04-20-jit-symbol-staleness-on-relink.md`
    (if no m6 file exists yet)
  noting that `SYSCALL_JIT_MAP_UNLOAD` is unimplemented in m5 and that
  a relink before the next `SYSCALL_JIT_MAP_LOAD` produces stale
  symbols (latest-wins covers replacement of the same module at the
  same base, but doesn't reclaim addresses freed by an unload).

### Out of scope

- Implementing `SYSCALL_JIT_MAP_UNLOAD` (deferred — only documenting it).
- Re-running phase 4's fw-esp32 / fw-emu builds individually if the
  workspace gate already covers them.
- Fixing pre-existing warnings unrelated to this plan.

## Code Organization Reminders

- Don't reorganize files in this phase. If you spot a structural
  problem, leave a one-line `TODO(future)` note (or add a future-work
  doc) — don't refactor here.
- The future-work doc, if added, should follow the same shape as
  `docs/future-work/2026-04-20-jit-symbols-in-panic-backtrace.md`:
  short context, scope, rationale.

## Sub-agent Reminders

- Do **not** commit. The dispatcher commits at the very end.
- Do **not** suppress warnings — fix the cause.
- Do **not** disable, skip, or weaken any test (including
  pre-existing).
- If validation fails on something this plan didn't introduce
  (pre-existing breakage), stop and report rather than fixing it
  here.
- If validation fails on something this plan *did* introduce, stop
  and report — the cleanup phase is not the right place to debug a
  real bug.
- Report back: files changed, validation output, any deviations.

## Implementation Details

### 1. Sweep

```bash
git diff --stat
```

Then for each touched file, eyeball `git diff <file>` for the items
listed in "In scope". Fix straightforward stragglers in place.
Anything bigger: stop and report.

### 2. Format

```bash
cargo +nightly fmt --all
```

If this changes anything outside files this plan touched, leave those
changes out of the diff (`git restore <file>`) — don't churn unrelated
files.

### 3. Add the m6 / future-work note

Check `docs/roadmaps/2026-04-19-cpu-profile/` for an m6 file. If
present, append a short "### Known limitation from m5" section noting:

> `SYSCALL_JIT_MAP_UNLOAD` is reserved but not implemented in m5. If
> the JIT relinks during a profile run and a new module reuses
> address ranges from a freed module, the old symbols stay in the
> overlay (latest-inserted wins on overlap, but freed-and-reused
> ranges still appear under the freed name). m6 should either
> implement UNLOAD or document this as accepted behaviour.

If no m6 file exists yet, instead create
`docs/future-work/2026-04-20-jit-symbol-staleness-on-relink.md` with
the same content, plus a one-line context paragraph and a pointer
back to `docs/plans-old/2026-04-20-cpu-profile-m5-jit-symbols/`.

### 4. Validation gate

Run the project's full validation gate (per `AGENTS.md` / `.cursorrules`):

```bash
rustup update nightly
just check
just build-ci
just test
```

Or, in one go: `just ci`.

All must pass cleanly. No new warnings.

If `just` is not available in the agent environment, fall back to
the explicit set:

```bash
cargo build -p lp-server
cargo test  -p lp-server --no-run
cargo test  -p lp-riscv-emu
cargo test  -p lp-riscv-emu-shared
cargo test  -p lp-cli
cargo test  -p lpvm-native
cargo test  -p fw-tests --test profile_jit_symbols_emu \
                        --test profile_alloc_emu \
                        --test scene_render_emu
cargo check -p fw-emu   --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

All must pass cleanly with no warnings.

## Validate

(Validation **is** the work here.) Final state must be:

- `git status` shows no untracked scratch files.
- `git diff` is the m5 implementation + this phase's documentation
  note + any small format/cleanup edits, nothing extraneous.
- The full validation gate above passes.
