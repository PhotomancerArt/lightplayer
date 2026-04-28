# Summary: M9 Access L-values for `out` / `inout`

## What was built

- **`lower_lvalue.rs`** — Writable call-actual resolution for `out` / `inout`: classifies Naga `Access` / `AccessIndex` (and related) expressions into direct aggregate addresses, temp-slot paths with post-call writeback, or structured rejection (uniforms, read-only params, unsupported shapes).
- **`lower_call.rs` integration** — Pointer-formal arguments use the resolver; post-call writebacks delegate to existing store/access helpers rather than duplicating mutation logic in call lowering.
- **Supporting updates** across `lower_access.rs`, `lower_stmt.rs`, `lower_struct.rs`, `lower_array.rs`, `lower_array_multidim.rs`, `lower_ctx.rs`, `lower_expr.rs`, and `naga_util.rs` for address computation, VMContext global roots, and access writeback.
- **Filetests** — Focused coverage in `function/access-lvalue-local-out-inout.glsl` and `function/access-lvalue-global-out-inout.glsl`; marker retirement / smoke in `function/edge-lvalue-out.glsl`; uniform rejection coverage in `type_errors/uniform-out-actual.glsl`.
- **Regression guard** — Early `Access` dispatch in `lower_expr.rs` narrowed so uniform struct array field reads (`uniform/struct-array-field.glsl`) are not mis-handled as aggregate deferred loads.

## Decisions for future reference

#### Temp/writeback vs direct addresses

- **Decision:** Non-aggregate leaves (scalars, vectors, matrices, lanes, cells) use a temporary slot plus post-call writeback by default; direct pointers are passed only when the destination is stable aggregate storage compatible with the aggregate pointer ABI (`AggregateSlot` locals/params/globals and known byte offsets).
- **Why:** Matches the existing stack/vreg model (no independent addresses for vector/matrix lanes) while avoiding copies for whole aggregates where layout already matches the callee.
- **Rejected alternatives:** Passing fabricated pointers for flat vregs; duplicating every `Statement::Store` path inside `lower_call.rs`.
- **Revisit when:** The local storage model gains durable per-lane addresses or the aggregate ABI changes.

#### Uniform and read-only rejection

- **Decision:** Uniform roots and uniform-derived paths are rejected before the callee sees a writable pointer, with messaging aligned to existing uniform write errors; `AggregateSlot::ParamReadOnly` stays non-writable through access chains.
- **Why:** VMContext uniform layout is read-only; Writable pointers must not alias read-only buffers.
- **Rejected alternatives:** Late failure in store lowering only.
- **Revisit when:** Uniform write modes or storage classes are implemented end-to-end.

#### Acceptance targets

- **Decision:** M9 acceptance is `wasm.q32`, `rv32c.q32`, and `rv32n.q32` only; no new `jit.q32` annotations or validation runs for this milestone.
- **Why:** `jit.q32` is deprecated; keeping it out avoids churn and false expectations.
- **Rejected alternatives:** Broadening gates to deprecated backends.
- **Revisit when:** If a deprecated backend is formally unsupported and annotations are bulk-removed workspace-wide (separate cleanup).
