# Phase 6 — Delete old code

Remove the obsolete files and shims now that the new path is fully
wired (phases 4–5) and verified to behave correctly via the still-
green `mem-profile` command (phase 5 validation).

Depends on phases 4 and 5. **Sequenced after 5** so that the
git history shows clearly: "build new path", then "cut old path".

## Subagent assignment

`generalPurpose` subagent. Pure deletion + shim removal. Safe
because the next phase (7) reruns/extends the test suite to
catch any regression.

## Files to delete

```
lp-riscv/lp-riscv-emu/src/alloc_trace.rs
lp-cli/src/commands/mem_profile/                  (whole directory)
lp-cli/src/commands/heap_summary/                 (whole directory)
```

## Files to update

```
lp-riscv/lp-riscv-emu/src/lib.rs                  # drop pub mod alloc_trace;
lp-riscv/lp-riscv-emu/src/emu/emulator/state.rs   # remove with_alloc_trace + finish_alloc_trace shims
lp-cli/src/commands/mod.rs                        # drop mem_profile, heap_summary
lp-cli/src/main.rs                                # drop MemProfile + HeapSummary subcommands
lp-cli/Cargo.toml                                 # drop rustc-demangle dep (now in lp-riscv-emu)
```

## Steps

### 1. Drop module declarations

In `lp-riscv-emu/src/lib.rs`, remove the line declaring the
`alloc_trace` module.

In `lp-cli/src/commands/mod.rs`, remove the lines declaring the
`mem_profile` and `heap_summary` modules.

### 2. Remove emulator shims

In `lp-riscv/lp-riscv-emu/src/emu/emulator/state.rs`, delete
`with_alloc_trace` and `finish_alloc_trace`. They were only kept
through phases 4–5 to avoid breaking the old call sites; both
call sites are gone after this phase (the CLI command in step 3,
the test in phase 7).

### 3. Remove CLI subcommand registrations

In `lp-cli/src/main.rs`, remove the `MemProfile` and
`HeapSummary` variants from the `Commands` enum, and their match
arms in the dispatch.

### 4. Delete the directories

```bash
rm -rf lp-cli/src/commands/mem_profile/
rm -rf lp-cli/src/commands/heap_summary/
rm    lp-riscv/lp-riscv-emu/src/alloc_trace.rs
```

(Use the Delete tool, not raw `rm`, where the shell directive
demands.)

### 5. Dependency cleanup

`rustc-demangle` was used by `heap_summary/resolver.rs`. After
phase 3, it lives in `lp-riscv-emu/src/profile/alloc.rs`; the
dep should be added to `lp-riscv-emu/Cargo.toml` (if not done
already in phase 3) and removed from `lp-cli/Cargo.toml`.

Check whether `lp-cli` still uses `rustc-demangle` for anything
else first:

```bash
rg 'rustc_demangle|rustc-demangle' lp-cli/
```

If clean, remove from `lp-cli/Cargo.toml`.

### 6. Sweep for stragglers

```bash
rg -l 'alloc_trace|AllocTracer|mem_profile|MemProfile|heap_summary|HeapSummary' \
   lp-riscv lp-cli lp-fw justfile
```

Expected remaining hits (legit):

- `SYSCALL_ALLOC_TRACE`, `ALLOC_TRACE_ALLOC` etc. — syscall
  protocol constants, kept by design.
- `lp-fw/fw-tests/tests/alloc_trace_emu.rs` — test file rename
  is phase 7.
- `examples/mem-profile/` — directory rename is phase 8.
- `justfile` `mem-profile` and `heap-summary` recipes — handled
  in phase 8.

Anything else (e.g. doc references, README entries) should be
flagged for fix.

## Validation

```bash
cargo check --workspace
cargo build --workspace

# Tests pass — same set as before, since:
# - alloc_trace_emu.rs is untouched in this phase (renamed/rewired in 7)
# - profile path was verified manually in phase 5
cargo test --workspace
```

The `alloc_trace_emu.rs` test is the one most likely to fail
here — it called `with_alloc_trace`, which is now gone. If it
still passes, that means it's already been ported to
`with_profile_session` in an earlier phase (unlikely given the
sequencing), OR it's not actually exercised in the default test
target. Either way, **do not modify it in this phase** — phase 7
explicitly handles the rename and rewire.

If the test fails, flag it and stop. Phase 7 is the fix; no need
to merge phase 6 in isolation if the workspace is broken.

(In practice: phase 6 and phase 7 may need to land as a single
commit to keep CI green. Note this in the babysit/PR notes.)

## Out of scope for this phase

- Renaming `alloc_trace_emu.rs` → `profile_alloc_emu.rs` (phase 7).
- Rewriting that test against the new API (phase 7).
- Any changes to `examples/`, `justfile`, or docs (phase 8).
