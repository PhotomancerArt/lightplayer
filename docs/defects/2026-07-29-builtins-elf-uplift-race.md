---
status: fixed
found: 2026-07-29      # how: live-debugging (intermittent local `just test`)
fixed: this change
area: justfile `test` + lp-shader/lpvm-cranelift/build.rs (rv32 builtins ELF)
class: unsynchronized-shared-artifact
related: [2026-08-01-xt-builtins-image-strands-just-test.md]  # the Xtensa twin: reader half ported, builder half not
---
# Parallel `just test` raced the builtins ELF uplift

**Symptom** — an intermittent `just test` failure in which **all 10**
tests in `lp-shader/lps-filetests/tests/rv32n_imm_range.rs` failed at
once:

```
test result: FAILED. 0 passed; 10 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`finished in 0.00s` with a clean sweep is the tell: nothing executed.
Every failure was the same panic:

```
compile should succeed: Link(Codegen(Unsupported(
  "lps-builtins-emu-app is empty or was not found at build time; run scripts/build-builtins.sh from the workspace root")))
```

Any rerun — the same test standalone, or the whole gate again — passed.
Observed on a branch whose only change was adding an unrelated leaf
workspace member.

**Root cause** — five facts had to line up, and under `just test` they
always did:

1. `test` carried `[parallel]`, so `test-rust` and `test-filetests` ran
   concurrently.
2. `test-filetests` does not merely *read* the rv32 builtins — it
   **builds** them. `scripts/filetests.sh` calls
   `scripts/build-builtins.sh`, which runs `cargo build --target
   riscv32imac-unknown-none-elf --release -p lps-builtins-emu-app`.
3. That build uplifts the linked ELF into
   `target/riscv32imac-unknown-none-elf/release/lps-builtins-emu-app`
   by **remove-then-hardlink**. The path does not exist for the length
   of that window. Measured with a polling probe across a real relink:
   ~20 consecutive polls observed `MISSING`, and the inode changed
   across it.
4. Cargo's build lock is **profile+triple scoped**. The host side holds
   `target/debug/.cargo-lock`; the builtins build holds
   `target/riscv32imac-unknown-none-elf/release/.cargo-lock`. Different
   files — the two cargo processes are not mutually exclusive at all.
   ("Two cargos, one workspace" *reads* like it must serialize.)
5. `lpvm-cranelift/build.rs` embeds that ELF via `include_bytes!`, and
   declares `rerun-if-changed` on that exact path.

(5) is what turns a narrow window into a reliable trap. The build script
is not wandering past the window by chance — the rewrite in (3) is
precisely the event that *invalidates* the build script and makes cargo
rerun it. Reader and writer are aimed at each other.

Inside the window, `build.rs` took its `!exe_path.exists()` branch,
emitted a `cargo:warning` (invisible in a long gate log), and wrote
`LP_BUILTINS_EXE_BYTES: &[u8] = &[]`. The empty embed surfaced minutes
later and one crate away, as every `NativeEmuEngine` test failing
instantly.

Why *that* branch, on *that* branch: adding a workspace member perturbed
the resolve enough that the rv32 build was no longer fresh, so
`build-builtins.sh` actually relinked instead of no-op'ing. A change
unrelated to shaders or RISC-V opened the window.

**Second writer, same shape** — `build-builtins.sh` also runs
`lps-builtins-gen-app` when the builtin source hash changes, and that
generator writes **into the source tree**: `lps-builtin-ids/src/lib.rs`,
`lpvm-cranelift/src/generated_builtin_abi.rs`,
`lps-builtins/src/builtins/**`. Under the old `[parallel] test` those
rewrites landed while `test-rust`'s `cargo test` was compiling the same
files. Not the failure observed here, but a wider blast radius, and no
amount of reader hardening can absorb it.

**Fix** — both halves, covering different things:

- `justfile` — `test` now depends on `build-rv32-builtins` *before* a
  private `[parallel] _test-parallel`. `just` runs a normal recipe's
  dependencies in order, so the builtins are current before anything
  runs concurrently and the parallel half has no writer. Cost measured:
  **0.5s** when the builtins are fresh (the common case); when stale it
  is work `test-filetests` would have done anyway, and only the overlap
  with `test-rust` is lost. This is the only half that covers the
  source-tree generator above.
- `lpvm-cranelift/build.rs` — reading the ELF now retries for a bounded
  2s instead of treating a transient absence as "never built", and
  validates what it read (ELF magic, plus a size that did not move
  across the read, catching a cross-device copy still in flight).
  Retrying is gated on the rv32 output directory existing, so a fresh
  clone that genuinely never built the builtins still reports
  immediately rather than stalling. This half covers the `[parallel]`
  recipes left unrestructured — `build`, `build-ci`, `ci-glsl` — where
  a host compile still overlaps an rv32 builtins build.

Verified: with a 0.5s artificial absence (≈250× the measured real
window), the old build script reproduced the exact `0 passed; 10 failed;
0.00s` signature and the new one passes all 10.

**Regression coverage** — none: the defect lives in a build script's
interaction with a concurrent cargo process. There is no in-process seam
to pin, and a timing test here would itself be flaky. The guard is
structural instead — the dependency order in `test`, and a reader that
can no longer silently produce an empty artifact. This entry is the
memory: a whole test file failing in `0.00s` right after a cross-target
build is an artifact problem, not a logic problem.

**Lesson** — a build script that both `rerun-if-changed`s a path *and*
tolerates that path being missing has encoded a race, because the
trigger and the failure mode are the same event; a missing input there
is not an "absent" case but a "not yet" case. Underneath that sits the
generalizable fact: cargo does not serialize builds that differ in
profile or target triple — the lock is per
`target/<triple>/<profile>/.cargo-lock` — so any recipe that fans a host
build out alongside a cross-target build is sharing mutable filesystem
state with nothing ordering it. And when such a step degrades, it must
not degrade *quietly*: a `cargo:warning` is not a diagnostic inside a
parallel gate, and the degraded value should carry its own cause (the
warning now says `read: No such file…` / `changed size during read…`,
not merely *that* something was wrong) so the failure is legible where
it happens rather than minutes later in another crate.
