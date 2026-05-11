# Phase 3 — Engine + lpvm-native emission, fw crate wiring

Wire `lp-perf` into the four producers and the two firmware
consumers. After this phase, in any build *with* a sink feature
enabled (i.e. fw-emu with `feature = "syscall"`), engine and
lpvm-native code will issue ECALL `SYSCALL_PERF_EVENT` at the four
event sites — but those calls are no-ops for the host because the
syscall handler doesn't exist yet (phase 5). End-to-end visibility
lights up after phase 5 + 7.

In **non**-profile / non-syscall builds (every test today, fw-esp32),
the macros compile to nothing and the resulting binaries are byte-
identical to pre-phase-3 (modulo the new dep edge in Cargo.lock).

This phase can run in parallel with phases 5 and 6 (disjoint files;
all touch lp-engine / lpvm-native / fw-* Cargo.toml or src; phases
5/6 touch lp-riscv-emu / lp-cli).

## Subagent assignment

`generalPurpose` subagent. Multiple files in multiple crates, but
each edit is small and well-scoped per the design doc's "Engine
emission points (m1 set)" table.

## Files to update

```
lp-core/lp-engine/Cargo.toml
lp-core/lp-engine/src/runtime/project.rs
lp-core/lp-engine/src/nodes/shader/runtime.rs

lp-shader/lpvm-native/Cargo.toml
lp-shader/lpvm-native/src/rt_jit/compiler.rs        # path may differ;
                                                    # exact location
                                                    # is wherever
                                                    # link_jit() is
                                                    # called from
                                                    # compile()

lp-fw/fw-emu/Cargo.toml
lp-fw/fw-esp32/Cargo.toml
```

## Edits

### `lp-core/lp-engine/Cargo.toml`

Add to `[dependencies]` (alphabetical within the section):

```toml
lp-perf = { path = "../../lp-base/lp-perf", default-features = false }
```

### `lp-core/lp-engine/src/runtime/project.rs`

Wrap `ProjectRuntime::tick(&mut self, delta_ms: u32)` body. Existing
function structure preserved; add the macro calls at the top and
bottom:

```rust
use lp_perf::{emit_begin, emit_end, EVENT_FRAME, EVENT_PROJECT_LOAD};

// In tick(...):
pub fn tick(&mut self, delta_ms: u32) -> Result<…, …> {
    lp_perf::emit_begin!(EVENT_FRAME);
    let result = /* existing body, unchanged */;
    lp_perf::emit_end!(EVENT_FRAME);
    result
}
```

Note the macro must wrap *every* return path — if `tick`'s body has
early returns, refactor to a single inner function or use a guard.
Cleanest pattern (no extra crate deps):

```rust
pub fn tick(&mut self, delta_ms: u32) -> Result<…, …> {
    lp_perf::emit_begin!(EVENT_FRAME);
    let result = self.tick_inner(delta_ms);
    lp_perf::emit_end!(EVENT_FRAME);
    result
}
fn tick_inner(&mut self, delta_ms: u32) -> Result<…, …> {
    /* body moved here */
}
```

If `tick`'s body is already small/single-return, just inline the
macros without the inner-fn split.

For `EVENT_PROJECT_LOAD`: wrap whichever method is the canonical
"load the project" entry point — likely
`ProjectRuntime::load_from_filesystem(...)` (or whatever its current
name is). Same pattern.

### `lp-core/lp-engine/src/nodes/shader/runtime.rs`

Wrap `ShaderRuntime::compile_shader(...)` (or the equivalent
`compile_*` method that calls into `LpGraphics::compile_shader`):

```rust
use lp_perf::{emit_begin, emit_end, EVENT_SHADER_COMPILE};

pub fn compile_shader(...) -> ... {
    lp_perf::emit_begin!(EVENT_SHADER_COMPILE);
    let result = /* body */;
    lp_perf::emit_end!(EVENT_SHADER_COMPILE);
    result
}
```

If the existing function name differs, search for "compile_shader"
in `lp-engine/src/nodes/shader/` to find the right site. The
emission must wrap the entire compile pipeline (including the
`LpGraphics::compile_shader` trait call), so `shader-link` (emitted
inside lpvm-native) nests cleanly inside `shader-compile`.

### `lp-shader/lpvm-native/Cargo.toml`

Add to `[dependencies]` (alphabetical):

```toml
lp-perf = { path = "../../lp-base/lp-perf", default-features = false }
```

(Note: lpvm-native has both target-conditional and feature-conditional
deps. `lp-perf` goes in the unconditional `[dependencies]` block —
it has no features by default and is `#![no_std]`, so it works for
both rv32 builds and host-target builds.)

### `lp-shader/lpvm-native/src/rt_jit/compiler.rs` (or equivalent)

Find the call site of `link_jit(...)` inside `compile(...)`. Wrap
*only the link_jit call*, not the surrounding codegen:

```rust
use lp_perf::{emit_begin, emit_end, EVENT_SHADER_LINK};

// inside compile():
lp_perf::emit_begin!(EVENT_SHADER_LINK);
let elf = link_jit(&object_bytes)?;
lp_perf::emit_end!(EVENT_SHADER_LINK);
```

If the `?` early-return is a problem (we'd skip the End emission on
failure), use a small guard:

```rust
lp_perf::emit_begin!(EVENT_SHADER_LINK);
let result = link_jit(&object_bytes);
lp_perf::emit_end!(EVENT_SHADER_LINK);
let elf = result?;
```

The second pattern is preferred — Begin/End pairing must always
match for the timeline to make sense, even on error paths.

(Locate the exact file with `rg "link_jit\(" lp-shader/lpvm-native`.)

### `lp-fw/fw-emu/Cargo.toml`

Add `lp-perf` with the `syscall` feature, immediately after the
existing `lp-riscv-emu-guest` line:

```toml
lp-perf = { path = "../../lp-base/lp-perf", features = ["syscall"] }
```

(Unconditional — fw-emu always wants the syscall sink. The existing
`profile` feature on `fw-emu` doesn't need to gate this; `lp-perf`
calls inside `lp-engine` are noop in non-profile fw-emu builds because
`lp-engine`'s own `lp-perf` dep is featureless. fw-emu's enable of
`syscall` means: *when* lp-perf is invoked (which only happens in
profile builds because fw-emu is the *only* binary that ever sees
lp-engine code with sink features enabled), it dispatches to ECALL.)

Actually, simpler statement: every fw-emu build dispatches lp-perf
to ECALL, but engine emission sites are only *reached* in builds
where the engine is exercised, which is always — but the resulting
ECALL is no-op'd by the host run loop until phase 5 lands a handler
and phase 7 installs a `ProfileSession` with collectors. Cost is
one ECALL per emission site per frame; negligible.

If you want to gate even harder (only emit when `--collect events`
is selected), that requires guest-side knowledge of host config,
which isn't worth the complexity. Skip.

### `lp-fw/fw-esp32/Cargo.toml`

Add `lp-perf` with no features (default = noop):

```toml
lp-perf = { path = "../../lp-base/lp-perf", default-features = false }
```

This means esp32 builds get the new dep edge but *zero behavior
change* — the macros compile to nothing.

## Validation

```bash
cargo build -p lp-engine
cargo build -p lpvm-native
cargo build -p fw-emu
cargo build -p fw-esp32                     # may need cross target setup
cargo build --workspace                     # nothing else should break

# Verify no behavior change in test suite
cargo test -p lp-engine
cargo test -p lpvm-native
```

No new tests required in this phase — engine and lpvm-native test
suites just verify nothing broke.

## Out of scope for this phase

- Host syscall handler for `SYSCALL_PERF_EVENT` (phase 5).
- `ProfileSession::on_perf_event` (phase 4).
- Mode system (phase 6).
- CLI changes (phase 7).
- e2e test for emitted events (phase 8).

After this phase, `cargo build -p fw-emu` produces a binary that
issues ECALL `SYSCALL_PERF_EVENT(10, ...)` at the four sites, but
the host emulator's syscall handler will reject syscall 10 as
unknown (logged, but harmless). End-to-end completion happens after
phase 5.
