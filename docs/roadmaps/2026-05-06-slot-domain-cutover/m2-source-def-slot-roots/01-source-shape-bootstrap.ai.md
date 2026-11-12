# Phase 1: Source Shape Bootstrap

## Scope Of Phase

Add generated static source shape registration to `lpc-source` and make sure the
crate can host slot-derived source roots.

In scope:

- Add `lpc-slot-codegen` as a build dependency of `lpc-source`.
- Add `lp-core/lpc-source/build.rs`.
- Include generated `slot_shapes` from `lpc-source/src/lib.rs`.
- Enable `lpc-model` derive support for `lpc-source`.
- Add any small source-local test helpers needed to create a registry.
- Add or promote source-specific typed/semantic leaf support only if needed by
  later source defs and not already available in `lpc-model`.

Out of scope:

- Converting all source defs.
- Changing `examples/basic`.
- Runtime/engine integration.
- Fixture mapping refactors.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO` and only when it has a tracked
  follow-up.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-source/Cargo.toml`
- `lp-core/lpc-source/build.rs`
- `lp-core/lpc-source/src/lib.rs`
- `lp-core/lpc-model/src/slot/slots/*`
- `lp-core/lpc-slot-codegen/src/lib.rs`

Expected changes:

- Add:

```toml
[build-dependencies]
lpc-slot-codegen = { path = "../lpc-slot-codegen" }
```

- Enable the `derive` feature on the `lpc-model` dependency in `lpc-source`.
- Add a build script mirroring `lpc-slot-mockup/build.rs`.
- Add:

```rust
pub mod slot_shapes {
    include!(concat!(env!("OUT_DIR"), "/slot_shapes.rs"));
}
```

- Keep `lpc-source` `#![no_std]`; the build dependency can be `std` because it
  runs on the host.
- Add a smoke test that an empty generated registration pass compiles, or wait
  until phase 2 if no roots exist yet.

## Validate

```bash
cargo fmt --package lpc-source
cargo check -p lpc-source
cargo test -p lpc-source --lib --tests
```
