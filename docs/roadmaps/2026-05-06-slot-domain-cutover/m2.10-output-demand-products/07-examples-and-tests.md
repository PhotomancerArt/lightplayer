# Phase 6: Examples And Tests

## Scope Of Phase

Update canonical examples and add tests proving the new product flow.

In scope:

- Update `examples/basic` to use output input bindings.
- Add or update engine tests for shader -> fixture -> output demand flow.
- Add a test proving output-owned buffer rendering.
- Add a test proving fixture is no longer directly wired to an output sink.
- Add a test for autosized output behavior if the implementation supports it.

Out of scope:

- Migrating every old example unless needed for compile/test.
- Rich debug UI rendering of control layout hints.

## Code Organization Reminders

- Keep test helpers below tests in each file.
- Prefer focused tests over broad integration smoke tests when possible.
- Keep examples human-readable TOML.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

Relevant files:

- `examples/basic/project.toml`
- `examples/basic/output.toml`
- `examples/basic/fixture.toml`
- `lp-core/lpc-engine/tests/`
- fixture/output node tests under `lp-core/lpc-engine/src/nodes/`

Evidence to prove:

- Output is the demand root.
- Output resolves `ControlProduct` through bindings.
- Fixture control render writes into output-owned memory.
- Dirty output buffer still flushes through `RuntimeServices`.

## Validate

```bash
cargo test -p lpc-engine
cargo test -p lpc-model
```
