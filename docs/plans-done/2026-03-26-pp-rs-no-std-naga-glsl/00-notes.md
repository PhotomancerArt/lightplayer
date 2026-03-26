# Plan notes: `pp-rs` `#![no_std]` for Naga GLSL on bare-metal RISC-V

## Scope of work

- Publish a **`light-player/pp-rs`** fork (GitHub) of the crates.io **`pp-rs`** crate (v0.2.1, Naga’s GLSL preprocessor dependency).
- Make the fork **`#![no_std]` + `alloc`** so **`naga` `glsl-in`** can compile for **`riscv32imac-unknown-none-elf`** (and other `*-none-*` targets with `alloc`).
- Wire **`lp2025`** to use the fork via **`[patch.crates-io]`** (same pattern as **`esp-println`** fork in-tree, **`lp-cranelift`** git deps).
- **Out of scope for this plan (follow-up):** re-plumbing **`lp-engine` / `lpir-cranelift`** embedded GLSL→JIT on **`fw-esp32`** once **`lp-glsl-naga`** builds for the firmware target. This plan stops at **“`cargo check -p lp-glsl-naga --target riscv32imac-unknown-none-elf` succeeds.”**

## Current state of the codebase

- **`lp-glsl-naga`** is **`#![no_std]`** and depends on **`naga`** with **`default-features = false, features = ["glsl-in"]`**.
- **`naga` `glsl-in`** enables optional **`pp-rs`**. **`pp-rs` 0.2.1** uses **`std::collections::{HashMap, HashSet}`**, **`std::rc::Rc`**, **`std::str::Chars`**, etc. — no **`no_std`** mode.
- **`wasm32-unknown-unknown`** provides **`libstd`**, so **web-demo** compiles **`pp-rs`** unchanged.
- **`riscv32imac-unknown-none-elf`** has **no `libstd`**; **`cargo check -p lp-glsl-naga --target riscv32imac-unknown-none-elf`** fails in **`pp-rs`** with **`can't find crate for std`**.
- **`lp2025`** root **`Cargo.toml`** already uses **`[patch.crates-io]`** for **`esp-println`** and commented local patches for **`lp-cranelift`** / **`lp-regalloc2`** using **`../…`** siblings.

## Questions (resolved / open)

### Q1 — Where does the fork live locally?

**Context:** Team convention uses sibling repos next to **`lp2025`**.

**Suggested answer:** Clone **`https://github.com/light-player/pp-rs`** as **`../pp-rs`** (one directory up from **`lp2025`**, same parent as **`lp2025`**).

**Answer (user):** Yes — GitHub patch on **light-player** org; local clone **one level up** from this project (sibling of **`lp2025`**).

### Q2 — Repo naming on GitHub

**Context:** Crates.io name remains **`pp-rs`**; the git repo can be **`light-player/pp-rs`** to match upstream naming, or **`light-player/lp-pp-rs`** for disambiguation.

**Suggested answer:** **`light-player/pp-rs`**, default branch **`main`**, tag or crate version bump **`0.2.2`** (or **`0.2.1-lightplayer.1`**) on publish if you ever publish to crates.io; for patch-only, version in **`Cargo.toml`** can stay **`0.2.1`** to match naga’s lock resolution or bump patch for clarity.

**Answer:** Adopt suggested default — repo **`light-player/pp-rs`**, branch **`main`**, fork **`Cargo.toml`** version **`0.2.2`** (or next patch) for clarity vs crates.io **0.2.1**.

### Q3 — Patch wiring: git vs path

**Context:** **`lp2025`** can patch with **`git`** (CI reproducible) or **`path = "../pp-rs"`** for local iteration.

**Suggested answer:** Commit **`git`** patch in **`lp2025`** `Cargo.toml` (like **`lp-cranelift`**). Document optional commented **`path`** for local dev, mirroring **`#[patch."https://github.com/light-player/lp-cranelift"]`** block.

**Answer:** Yes — **git as default** (`pp-rs = { git = "https://github.com/light-player/pp-rs", branch = "main" }`), with a **commented** `# pp-rs = { path = "../pp-rs" }` (or equivalent) for local iteration.

### Q4 — Upstream contribution

**Context:** gfx-rs/wgpu may accept a **`no_std`** PR for **`pp-rs`** or naga could vendor a fork.

**Suggested answer:** Open an upstream PR to **`pp-rs`** (or wgpu monorepo if **`pp-rs`** moves) after fork is validated; keep **`light-player/pp-rs`** until merged.

**Answer:** **No immediate upstream work** — treat an upstream PR as **optional** later if it still makes sense; keep **`light-player/pp-rs`** as the supported path for **lp2025** until then.

## Notes

- **`fw-tests`** (`scene_render_emu`, `alloc_trace_emu`) assert **`NodeStatus::Ok`** on the shader after sync so the suite **fails** while **`fw-emu`** builds **`lp-engine`** without **`std`** (no JIT). They should pass again once embedded GLSL codegen is wired and the firmware enables the real compiler path.
- **`pp-rs`:** **`https://github.com/light-player/pp-rs`** (v**0.2.2**), wired via **`[patch.crates-io]`** `git` in root **`Cargo.toml`**. Local override: uncomment **`path = "../pp-rs"`** after cloning the repo next to **`lp2025`**.
- Disabling the preprocessor entirely in **naga** is **not** a small feature flag: the GLSL lexer is built around **`pp_rs::Preprocessor`**. This plan **does not** pursue preprocessor removal.
- Binary-size win from dropping **`pp-rs`** would be small vs **naga** itself; **`no_std`** **`pp-rs`** keeps full preprocessor behavior.
