# Milestone 4 — Uniform-struct-with-array-field

## Status

**Complete** — 2026-04-23. Uniform array fields work on `wasm.q32`,
`rv32c.q32`, and `rv32n.q32`. `uniform/struct-array-field.glsl` and
`uniform/array.glsl` both pass fully.

## Goal

Extend `load_lps_value_from_vmctx` to recurse through array-typed struct
members at std430-strided offsets, enabling the canonical
`uniform { Light lights[8]; }` shader pattern. Small, isolated change to
the global-load path.

## Suggested plan name

`lp-shader-aggregates-m4-uniform-array-field`

## Scope

### In scope

- Extend `load_lps_value_from_vmctx` (`lp-shader/lps-frontend/src/
  lower_expr.rs:993`, recursive struct-member loader for global/uniform
  data) to handle array-typed members. For an array member with
  std430-strided element layout, emit per-element loads at
  `outer_offset + element_idx × array_stride + member_offset`. Recurses
  naturally through the leaf type — scalar/vec/mat/struct/array.
- Mirror update to the `AccessIndex` arm in `lower_expr_vec_uncached`
  for `GlobalVariable` of a uniform whose path traverses an array
  member (today: errors out with "unsupported"; after M4: produces the
  recursive load).
- Add 1–2 new filetests under
  `lp-shader/lps-filetests/filetests/uniform/`:
  - `struct-array-field.glsl`: uniform with `Light lights[8]` field;
    read `lights[i].position`, `lights[i].color`, etc.
  - Possibly `array-of-struct.glsl`: uniform top-level
    `Particle particles[8]` (no enclosing struct), if not already
    covered elsewhere.
- Toggle off any `@unimplemented(jit.q32)` markers exposed by the new
  filetests (and `wasm.q32` / `rv32*` per Q7).

### Out of scope

- Anything outside the uniform load path.
- The local/param array-of-struct case (handled in M3).
- Read-only-`in` optimisation (M5).

## Key decisions

- **No design questions expected.** The recursion shape is dictated by
  std430 layout rules; the existing `load_lps_value_from_vmctx`
  recursion through struct members is the template; adding the array
  arm is a mechanical extension.

## Deliverables

- Modified files:
  - `lp-shader/lps-frontend/src/lower_expr.rs` (recursion extension in
    `load_lps_value_from_vmctx` and `AccessIndex` arm)
- New filetests:
  - `lp-shader/lps-filetests/filetests/uniform/struct-array-field.glsl`
  - Possibly `lp-shader/lps-filetests/filetests/uniform/array-of-struct.glsl`
- `@unimplemented` markers toggled off as features pass.

## Dependencies

- **Requires M1 complete** (unified ABI baseline).
- Independent of M2 and M3 — M4 only touches the uniform-load path,
  which doesn't share code with the local-storage / call-ABI work.
- Practically scheduled after M2 so the shader test programs can use
  struct types end-to-end (otherwise the `struct-array-field` filetest
  can't even be parsed by the frontend test harness without struct
  support landing first).

## Execution strategy

**Option A — Direct execution (no plan file).**

Justification: ~30 lines of new recursion in a single function plus
1–2 new filetests. Isolated from the rest of the system. No design
questions; the std430 recursion shape is fully determined by the type.
A Composer 2 sub-agent can implement straight from this milestone file.

**Suggested chat opener:**

> I can implement this milestone without planning. Here is a summary of
> decisions/questions:
>
> - Extend `load_lps_value_from_vmctx` recursion to walk array members
>   at std430-strided offsets.
> - Mirror update to `AccessIndex` arm for uniform-via-array.
> - Add 1–2 new filetests; toggle off `@unimplemented` markers across
>   all backends per Q7.
> - No design questions; recursion shape is dictated by std430.
>
> If you agree, I will implement now using a Composer 2 sub-agent. If
> you want to discuss any of these, let me know now.
