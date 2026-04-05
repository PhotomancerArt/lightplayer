# Phase 2: `lp-engine` — always compile shaders for server-class builds

## Scope of phase

Remove the **`#[cfg(not(feature = "std"))]`** **`compile_shader`** stub as the **default** for embedded **`lp-server`** consumers. **`ShaderRuntime`** must use the real GLSL → **`lpir_cranelift::jit`** path when **`lpir-cranelift`** provides **`jit`** ( **`glsl`** enabled on the dependency). Optional **opt-out** (e.g. **`minimal`** / **`no-shader-compile`**) only if a concrete consumer needs a smaller **`lp-engine`**; document who uses it.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **`Cargo.toml`**
   - On the **`lpir-cranelift`** dependency, enable **`glsl`** (and existing optimizer/verifier flags) **independent of `std`**.
   - **`default`** features: keep **`std`** for host **default** workspace builds; embedded uses **`default-features = false`** but must still pull **`glsl`** via explicit dependency features (not a separate “shader-jit” product flag unless needed for opt-out).

2. **`src/nodes/shader/runtime.rs`**
   - Replace **`#[cfg(feature = "std")]` / `#[cfg(not(feature = "std"))]`** split on **`compile_shader`** with:
     - **Real** implementation whenever **`lpir_cranelift::jit`** exists for this build (typically **`#[cfg(feature = "glsl")]`** on **`lp-engine`** *if* Phase 1 adds a forwarded **`glsl`** feature here, or **unconditional** if **`lps-naga`** becomes a required dep of **`lp-engine`**). Align **`cfg`** names with Phase 1’s **`Cargo.toml`**.
     - **Stub** only under an explicit **opt-out** feature (e.g. **`no-shader-compile`**) if such a profile is kept; otherwise remove the stub.
   - Keep **`panic-recovery`** **`catch_unwind`** around **`jit`** as today when that feature is on.
   - Host-only helpers (e.g. certain **`format!`** / logging) may stay behind **`std`** if needed; core compile path should use **`alloc`**.

3. **`render_direct_call`**
   - Same gating as **`compile_shader`** — must run on embedded after successful compile.

## Tests to write

- **`lp-engine`** unit/integration tests that still pass on **host** with **`std`**.
- If feasible, a **`no_std`** test crate or **`cargo check`** of **`lp-engine`** **`--no-default-features`** with **`glsl`**-equivalent features (may overlap Phase 4).

## Validate

```bash
cargo +nightly fmt -p lp-engine
cargo check -p lp-engine
cargo check -p lp-engine --no-default-features --features panic-recovery   # plus any glsl/forwarded features from Phase 1 Cargo.toml
cargo test -p lp-engine
```

Fix new warnings introduced in this phase.
