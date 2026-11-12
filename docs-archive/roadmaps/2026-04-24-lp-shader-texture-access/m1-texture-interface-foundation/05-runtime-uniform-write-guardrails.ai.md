# Scope of Phase

Ensure existing scalar/aggregate uniform write paths do not accidentally expose
texture ABI descriptor internals or accept ordinary value writes for
`LpsType::Texture2D`.

Depends on phases 1 and 3. It can be done after phase 4 for simpler validation.

In scope:

- Audit uniform write encoding for `LpsType::Texture2D`.
- Add explicit guardrails/errors where normal `set_uniform` paths encounter
  texture uniforms.
- Add focused tests if behavior is reachable from existing APIs.

Out of scope:

- A complete runtime texture binding API.
- Runtime texture format/shape validation.
- Texture sampling or render-time binding.
- Changing metadata to expose ABI fields.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

# Implementation Details

Relevant files:

- `lp-shader/lpvm/src/set_uniform.rs`
- `lp-shader/lpvm/src/data_error.rs`
- `lp-shader/lpvm/src/lpvm_abi.rs`
- `lp-shader/lps-shared/src/lps_value_f32.rs`
- `lp-shader/lps-shared/src/lps_value_q32.rs`
- `lp-shader/lps-shared/src/path_resolve.rs`

Current behavior:

- `encode_uniform_write` resolves a path with `type_at_path`, encodes an
  `LpsValueF32`, then checks encoded byte length against `type_size`.
- `encode_uniform_write_q32` does the same for pre-encoded Q32 values.
- `Texture2D` should be a logical leaf with ABI size 16, but callers should not
  write `tex.ptr`, `tex.width`, or a made-up aggregate value through ordinary
  scalar uniform APIs.

Audit all exhaustive matches introduced by `LpsType::Texture2D` in lpvm and
shared value conversion code.

Preferred behavior:

- `set_uniform("tex", ordinary_value)` should fail with a clear type mismatch
  explaining that texture uniforms require typed texture binding/descriptor
  helpers.
- `set_uniform("tex.ptr", ...)` should fail because `Texture2D` has no public
  fields.
- Do not add a fake `LpsValueF32::Texture2D` unless the implementation is
  already clearly called for by existing APIs. This milestone chose a small
  `Texture2DUniform` helper in `lp-shader`; broader runtime binding belongs to
  later milestones.

If no code change is needed because existing matches reject `Texture2D`
naturally and path resolution treats it as a leaf, add focused tests proving
that behavior.

Tests to add where most local:

- A constructed `LpsModuleSig` with `uniforms_type` containing
  `tex: LpsType::Texture2D` rejects `encode_uniform_write("tex", ...)` with a
  useful error.
- `encode_uniform_write("tex.ptr", ...)` rejects with a path error.
- If `encode_uniform_write_q32` is reachable, add the equivalent rejection test.

# Validate

Run from the workspace root:

```bash
cargo test -p lpvm
cargo check -p lpvm
```

