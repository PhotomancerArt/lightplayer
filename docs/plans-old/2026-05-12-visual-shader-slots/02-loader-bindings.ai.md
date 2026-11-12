# Phase 2: Visual Shader Binding Registration

## Scope Of Phase

Teach the project loader to register consumed visual shader bindings.

In scope:

- Register optional source bindings for every `ShaderDef.consumed_slots` entry.
- Register fallback default binding for compatible `consumed.time`.
- Add focused loader tests.

Out of scope:

- Shader runtime uniform resolution.
- New binding syntax.
- General semantic default binding rules.

## Code Organization Reminders

- Keep loader helpers small and near existing binding helper functions in
  `project_loader.rs`.
- Prefer shared helpers between compute and visual shaders where the code reads
  cleaner.
- Do not hide domain-specific rules in generic binding code.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant file:

- `lp-core/lpc-engine/src/engine/project_loader.rs`

Expected changes:

- In the `NodeDef::Shader(config)` attach branch, after attaching the runtime
  node and registering `output`, iterate `config.consumed_slots.entries.keys()`
  and call `register_optional_source_binding`.
- Add helper like:

  ```rust
  fn register_visual_shader_default_time_binding(...)
  ```

- The helper should register:

  ```text
  source = bus#time.seconds
  target = consumed slot <shader>.time
  priority = BindingPriority::default_fallback()
  kind = Kind::Instant
  ```

- Only register if:
  - consumed slot `time` exists;
  - it is `ShaderSlotKind::Value`;
  - its value shape is `f32`;
  - `bindings.time.source` is absent.
- Authored `[bindings.time]` should win by being registered at authored
  priority.

Tests to add/update:

- Visual shader with `consumed.time` resolves `time` through bus after a clock is
  present.
- Authored `bindings.time` suppresses the default binding.
- Non-`f32` or non-value `time` does not receive the fallback binding.

## Validate

```bash
cargo fmt
cargo test -p lpc-engine project_loader
```

