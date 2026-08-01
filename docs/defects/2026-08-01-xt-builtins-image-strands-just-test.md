---
status: fixed
found: 2026-08-01      # how: live-debugging (local `just test` after a build-cache wipe)
fixed: this change
area: justfile `ci-prereqs`/`test` + scripts/build-builtins-xt.sh + lpvm-native/tests/xt_corpus_goldens.rs
class: opt-in-degradation
related: [2026-07-29-builtins-elf-uplift-race.md]
---
# The Xtensa builtins image degraded to "skip" for everyone who asked, and to a red suite for the one caller who didn't

**Symptom** — `just test` (recipe `test-xt-host`, `cargo test -p lpvm-native
--features emu-xt,xt-corpus`) went red on a machine where it had been green,
with no source change between the two runs. One test failed:

```
---- a_q32_module_refuses_the_f32_entry_point stdout ----
thread 'a_q32_module_refuses_the_f32_entry_point' panicked at
lp-shader/lpvm-native/tests/xt_corpus_goldens.rs:337:44:
q32 compile: Internal("Xtensa builtins image is empty or was not found at build
time — run scripts/build-builtins-xt.sh (needs the esp toolchain)")
```

Two things made this hard to read. The eight sibling tests in the same file
passed — including the two that confirm the whole Xtensa corpus — because they
*skip*. And `just ci-prereqs`, the recipe whose stated job is building "just the
cross-target prerequisites host tests embed/spawn", did **not** fix it: a full
prereqs pass left the suite red. Running `scripts/build-builtins-xt.sh` by hand
did.

The trigger was the shared cargo build cache being wiped
(`~/.cargo/config.toml` puts every workspace's intermediates under
`~/.cache/cargo-build/{workspace-path-hash}`; it had filled the disk and was
nuked). A fresh worktree arrives at the same place from the other direction:
the ELF is gitignored so it was never there, and the per-worktree cache is cold,
so the build script runs and sees nothing. That is how this was reproduced.

**Root cause** — four facts, each individually reasonable:

1. `lp-xt/fixtures/elf/lps-builtins-xt-app.elf` is **gitignored and
   regenerable** — a cross-target artifact needing the esp toolchain. The
   workspace must build and test without espup, so `lps-builtins-xt-image`'s
   build script embeds an **empty slice** when the ELF is absent. That is a
   designed state, documented as such, and `is_available()` is how a consumer
   asks about it.
2. The state is **observed once, at build-script time, then frozen into the
   compiled rlib**. So the event that flips a green machine red is not
   "someone deleted the ELF" — it is "something made the build script rerun
   while the ELF was absent". Wiping the build cache does exactly that, without
   touching the source tree, which is why the change looks like it came from
   nowhere.
3. The graceful degradation is **opt-in**. Of the nine tests in
   `xt_corpus_goldens.rs`, six never touch the image, two check
   `is_available()` and skip, and one —
   `a_q32_module_refuses_the_f32_entry_point` — needs it and does not check.
   Nothing about that test reads as image-dependent: it asserts a **mode
   guard** (a Q32 module must refuse `call_f32_words`), not code generation.
   But asserting it requires a compiled module, and compiling for Xtensa under
   `rt_emu` means linking against the base image. What it got was
   `NativeError::Internal` from `build_xt_image`, which reads as a codegen
   failure rather than as a missing dev artifact.
4. **No recipe in the local gate built the image.** `scripts/filetests.sh`
   builds it only when an `xtn`/`xtlpn` target is explicitly requested (correct
   there — it must not force an esp build on every `just test-filetests`), and
   CI's `validate-xtensa` job calls the script directly. The one path that
   needed it, `test-xt-host` under `just test`, had no builder at all, and
   `ci-prereqs` listed only the rv32 pair.

(4) is what turns a one-line fix into a stranding. A "prereqs" recipe is a
**claim of completeness** — whoever runs it has concluded that missing
cross-target artifacts are now somebody else's problem. Being right about rv32
and silent about Xtensa is worse than not existing, because it consumes the
diagnostic step that would have found this.

**The un-ported half** — this is the Xtensa twin of
[`builtins-elf-uplift-race`](2026-07-29-builtins-elf-uplift-race.md), and the
resemblance is not incidental. That fix had two halves: harden the *reader*
(`build.rs` retries and validates) and order the *builder* (make `test` depend
on the builtins build before the parallel half). `lps-builtins-xt-image/build.rs`
got the reader half — it carries `copy_image`, the retry loop, and a comment
citing that entry by name. The builder half was never ported. One of two.

That is the same under-scoping the registry's own index already names for the
call-boundary invariant: *when a fix establishes an invariant, enumerate every
place it applies before closing the entry.* Here the enumeration was one
sentence — "the rv32 builtins ELF and the Xtensa builtins ELF are the same kind
of thing".

**A second source of truth, noted in passing** — `xt_builtins_image.rs` and
`xt_pipeline_f32.rs` decide availability by `std::fs::read`ing the ELF from the
source tree; `rt_emu` uses the *embedded* bytes. The two can disagree in both
directions (build the ELF without rebuilding the embedding crate, or wipe the
cache and rebuild the crate without the ELF). Not what failed here, and not
worth unifying today, but it means "is the image available?" has two answers.

**Fix** — three small pieces, mirroring the rv32 entry's shape:

- `scripts/build-builtins-xt.sh` gains `--if-toolchain`, which turns a missing
  esp toolchain from an error into a no-op `exit 0` with a note. Toolchain
  detection stays in the one place that already does it properly (two install
  shapes, espup and the CI action).
- `justfile` gains `build-xt-builtins`, which is that invocation. `ci-prereqs`
  now includes it, so the recipe's completeness claim is true; `test` depends
  on it **before** `_test-parallel`, both so a cache wipe cannot strand
  `test-xt-host` and so the `[parallel]` half has no writer for this artifact
  either — the same ordering argument as the rv32 half, and the reason it is
  *not* a dependency of `test-xt-host` itself, which runs inside that parallel
  half. Measured: **1m45** on the first build in a fresh worktree (the image has
  never existed there), **0.7s** thereafter — the rv32 half's 0.5s, near enough.
  Where espup is not installed it prints one line and does nothing, so the
  recipe is safe everywhere.
- `scripts/build-builtins-xt.sh` also stops copying the linked binary over the
  published ELF when the bytes are identical. `cp` always bumps mtime and
  `build.rs` watches that exact path, so an unconditional copy would have added
  a re-embed plus a rebuild of `lpvm-native` and everything above it to **every**
  `just test` — a cost that did not matter while only `filetests.sh --target xtn`
  called this, and would have started mattering the moment `test` did.
- `xt_corpus_goldens.rs` gains an `image_available(what)` helper and routes all
  three image-dependent tests through it, including
  `a_q32_module_refuses_the_f32_entry_point`. The guard is now stated once, with
  the reason it is mandatory, instead of open-coded twice and forgotten once.

**Regression coverage** — none that is honest, and the gap is worth naming.
CI's `validate-xtensa` job already asserts *both* builtins images are present
before running, precisely so the Xtensa tests cannot pass by skipping — which
means **no gate anywhere runs this suite with the image absent**. The "skips
cleanly without espup" contract is therefore asserted only by hand, and this is
the second time a caller has been able to break it unobserved. Verified by hand
here: with the ELF absent, the failing test reproduces verbatim before the
change and prints `SKIP: Xtensa builtins image absent — cannot confirm the
Q32/f32 entry-point guard` after it; with the ELF built by `just
build-xt-builtins`, all nine tests pass.

**Lesson** — an absent input modelled as a *legal degraded value* rather than
an error only degrades gracefully for the consumers that interrogate it. Every
other consumer gets the raw failure, and gets it worse than if the value had
been an error outright: the message names the **subject** ("compile failed")
where an error would have named the **environment**, and it surfaces at the
first use rather than at the point of substitution. If a degraded value must
exist, the checking must be structural — one accessor that cannot be bypassed,
or one helper every call site is documented to route through — because "skips
loudly when absent" is a property of the callers, not of the value, and callers
get added. Alongside that: a gitignored regenerable artifact needs its
regeneration hung off the gate that *consumes* it, not off the gate that
happens to want it (filetests) or the CI job that already knows about it
(`validate-xtensa`) — and when the twin of an existing artifact appears, port
**both** halves of the twin's fix or write down which half you skipped.
