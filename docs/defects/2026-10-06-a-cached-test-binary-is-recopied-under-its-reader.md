---
status: fixed
found: 2026-10-06      # how: ci (Validate (x64) on PR #997, run 37488717302)
fixed: this change
area: lp-emu/lp-riscv-emu `test_util::ensure_binary_built`
class: unsynchronized-shared-artifact
related:
  - docs/defects/2026-07-29-builtins-elf-uplift-race.md
---
# A cached test binary is re-copied under the test reading it

**Symptom** — `fw-tests --test recovery_emu` failed one test of eight on a PR
that touched no Rust:
`Failed to load ELF: "Failed to parse ELF: Invalid ELF section header
offset/size/alignment"` at `recovery_emu.rs:74`, right after
`ensure_binary_built` returned the fw-emu ELF's path.

**Root cause** — `ensure_binary_built` keeps one stable copy per build
configuration (`target/.lp-test-cache/<key>`) and an in-process map of the
copies it made. Tests in one binary run on parallel threads. Two threads that
both missed the map queue on the cross-process build lock; the first builds,
copies, records and returns, and its caller starts reading the copy. The
second then gets the lock, never re-reads the map, rebuilds (a no-op) and
`fs::copy`s over the same path — which truncates the file the first caller
is reading. The flock orders the builds, but not the reads, which happen
outside it on a file the writer replaces in place.

**Fix** — the map is checked again under the lock, so a thread that waited
for a build another thread finished returns that copy and writes nothing.
And the copy is staged beside the cache path and `rename`d over it, so a
reader in any process sees the old file or the new one whole, never a
truncated one.

**Regression coverage** — none: the window is a few milliseconds of a
parallel test run. `recovery_emu`, `scene_render_emu` and the other
`ensure_binary_built` callers exercise the path on every Validate (x64) run.

**Lesson** — a lock around the build is not a lock around the artifact. A
file other code reads without the lock has to be replaced atomically
(stage, then rename), never rewritten in place — the same shape as the
builtins ELF race, one layer down.
