# Phase 5: Cleanup And Validation

## Scope Of Phase

Clean up old visual shader input vocabulary and run final validation.

In scope:

- Remove stale `param_defs` references.
- Ensure docs/rustdocs describe visual shader consumed slots.
- Ensure old implicit `time` path is gone or clearly limited to built-in request
  data.
- Run final targeted validation.

Out of scope:

- Full CI unless explicitly requested.
- Texture input planning beyond the captured future note.

## Code Organization Reminders

- Do not leave compatibility shims unless the plan deliberately chose them.
- If `kind = "shader"` alias remains, mark it as temporary in code comments or
  tests.
- Keep test modules at file bottoms.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Search targets:

```bash
rg "param_defs|kind = \"shader\"|ShaderDef::KIND|build_uniforms\\("
```

Expected final state:

- New authored visual shaders use `kind = "shader/visual"`.
- Visual shader inputs are declared under `consumed`.
- Visual shader `time` is resolved through the slot/bus path when declared.
- `outputSize` remains a render-request built-in.
- Debug UI can inspect visual shader consumed slot declarations and state roots
  as normal slot data.

## Validate

```bash
cargo fmt
cargo test -p lpc-model
cargo test -p lpc-wire
cargo test -p lpc-view
cargo test -p lpc-engine
cargo check -p lpa-client
cargo check -p lpa-server
cargo check -p lp-cli
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

