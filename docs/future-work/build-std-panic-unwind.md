# Full `panic!()` unwinding via `-Z build-std`

## Status: resolved

Both `fw-esp32` and `fw-emu` now use `-Z build-std=core,alloc` to rebuild the
standard library with `panic=unwind`. The emulator test (`fw-tests/tests/unwind_emu.rs`)
validates `panic!()` -> `catch_unwind` through the full stack, including
`core::panicking` frames.

## Root cause of the earlier `build-std` failure

The git version of `cranelift-codegen` (at `lp-cranelift?branch=main`) declares
`hashbrown` with the `alloc` feature enabled. This pulls in `rustc-std-workspace-alloc`
(a crate whose `lib.name` is `alloc`), which conflicts with the real `alloc` crate
that `build-std` rebuilds from source — producing:

```
error[E0464]: multiple candidates for `rmeta` dependency `alloc`
```

The local path patches of cranelift (`[patch."https://github.com/light-player/lp-cranelift"]`)
only enable `hashbrown/default-hasher` without `alloc`, so the conflict disappears.

The patch section header was accidentally commented out (`# [patch."https://..."]`),
causing the path entries to fall into the regalloc2 patch section where they were
silently ignored. Uncommenting the header fixed it.

## Lesson

Any workspace dependency that enables `hashbrown/alloc` will break `build-std`
because of feature unification — the `alloc` feature pulls in `rustc-std-workspace-alloc`,
creating a duplicate `alloc` crate. If upstream `lp-cranelift` needs `hashbrown/alloc`,
it should be gated behind a `std` feature so that bare-metal builds can disable it.
