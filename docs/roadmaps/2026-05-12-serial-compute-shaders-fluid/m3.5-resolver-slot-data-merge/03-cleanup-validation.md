# Phase 3: Cleanup And Validation

## Scope

Clean up naming/docs from the resolver merge pass and run focused validation.

Out of scope:

- Further performance optimization unless a clear regression appears.
- Fluid node implementation.

## Code Organization Reminders

- Remove dead helper functions or stale comments.
- Keep rustdocs semantic, not plan-specific.

## Sub-agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings.
- If blocked, stop and report.

## Implementation Details

- Update resolver module docs to mention aggregate-capable resolved slots.
- Search for stale leaf-only comments around `Production` and resolver cache.
- Write `summary.md` with built work and decisions.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-engine
cargo check -p lpc-engine
```
