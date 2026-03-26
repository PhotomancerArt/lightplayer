# Design: `pp-rs` `#![no_std]` fork (`light-player/pp-rs`)

## Scope of work

Same as [00-notes.md](./00-notes.md): fork **`pp-rs`**, make it **`no_std` + `alloc`**, patch **`lp2025`** so **`lp-glsl-naga`** checks for **`riscv32imac-unknown-none-elf`**.

## File structure

```
photomancer/                    # parent of lp2025 (example)
├── lp2025/
│   └── Cargo.toml              # UPDATE: [patch.crates-io] pp-rs -> git or path
├── pp-rs/                      # NEW (clone): light-player/pp-rs fork
│   ├── Cargo.toml              # UPDATE: no_std, hashbrown dep, version bump
│   └── src/
│       ├── lib.rs              # UPDATE: #![no_std], extern crate alloc
│       ├── lexer.rs            # UPDATE: core::str::Chars
│       ├── pp.rs               # UPDATE: hashbrown, alloc::rc::Rc, core::*
│       ├── pp/
│       │   └── if_parser.rs    # UPDATE: same
│       └── token.rs            # UPDATE if any std references
└── docs/plans/2026-03-26-pp-rs-no-std-naga-glsl/   # this plan
```

## Conceptual architecture

```
┌─────────────────────────────────────────────────────────────┐
│  fw-esp32 / riscv32imac-unknown-none-elf  (future wiring)   │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  lp-glsl-naga  (#![no_std])                                  │
│    └── naga (glsl-in) ──► pp-rs  ◄── MUST be no_std+alloc   │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  light-player/pp-rs  (fork of crates.io pp-rs 0.2.1)         │
│  HashMap/HashSet: hashbrown │ Rc: alloc │ Chars: core       │
└─────────────────────────────────────────────────────────────┘

lp2025 Cargo.toml
  [patch.crates-io]
  pp-rs = { git = "https://github.com/light-player/pp-rs", branch = "main" }
```

## Main components

| Piece | Role |
|--------|------|
| **`light-player/pp-rs`** | Drop-in replacement for **`pp-rs` 0.2.1** with **`#![no_std]`** and **`hashbrown`** for maps/sets. |
| **`lp2025` `[patch.crates-io]`** | Forces all workspace uses of **`pp-rs`** (via **naga**) to the fork. |
| **Validation** | **`cargo check -p lp-glsl-naga --target riscv32imac-unknown-none-elf`** plus existing host tests (**`cargo test -p lp-glsl-naga`**, **`web-demo`** / **`wasm32`** check) to ensure no regression. |
| **Upstream** | **Optional:** PR to canonical **`pp-rs`** / **wgpu** later — not part of the initial milestone. |

## Decisions (from notes)

- Fork on **light-player** org; local clone sibling **`../pp-rs`**.
- **`lp2025` consumption:** **`[patch.crates-io]`** with **git** to **`light-player/pp-rs`** **`main`** by default; **commented `path = "../pp-rs"`** for local dev (same idea as **lp-cranelift** blocks).
