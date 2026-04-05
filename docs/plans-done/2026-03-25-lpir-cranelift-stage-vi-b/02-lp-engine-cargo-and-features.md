# Phase 2: `lp-engine` / `lp-server` Cargo and features

## Scope

Swap compiler dependency to `lpir-cranelift`, remove unused Cranelift / JIT util
deps, and forward features through `lp-server`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. `lp-engine/Cargo.toml`

**Remove:**

- `lp-glsl-cranelift`
- `cranelift-codegen`
- `lp-glsl-jit-util`

**Add:**

```toml
lpir-cranelift = { path = "../../lp-shader/lpir-cranelift", default-features = false }
```

**Features** (mirror `lpir-cranelift`):

```toml
[features]
default = ["std", "cranelift-optimizer", "cranelift-verifier"]
panic-recovery = ["dep:unwinding"]
cranelift-optimizer = ["lpir-cranelift/cranelift-optimizer"]
cranelift-verifier = ["lpir-cranelift/cranelift-verifier"]
std = [
    "lp-shared/std",
    "lpir-cranelift/std",
]
```

### 2. `lp-server/Cargo.toml`

Replace any `lp-engine/...` feature edges that still name `lp-glsl-cranelift`
with `lpir-cranelift` equivalents (optimizer / verifier / `std`).

### 3. Workspace / other crates

Grep for `lp-glsl-cranelift` under `lp-core/` and `lp-fw/` that exist only to
satisfy `lp-engine`. Remove or redirect if obsolete.

### 4. Do not edit `runtime.rs` yet

This phase should leave `runtime.rs` broken on purpose so Phase 3 is a focused
rewrite, or apply minimal stubs if the workspace must compile between phases.
Prefer: complete Phase 2 + 3 in one working session so `cargo check -p lp-engine`
passes after Phase 3.

## Validate

```bash
cargo check -p lp-engine
# Expect errors in runtime.rs until Phase 3 — acceptable if phases merged same session
```
