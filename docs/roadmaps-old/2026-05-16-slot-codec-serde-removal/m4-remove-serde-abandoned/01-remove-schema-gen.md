# Phase 1: Remove Schema-Gen Surface

> Superseded by the measured M4 policy. Do not remove `schema-gen` or
> `schemars` as part of automatic SlotCodec stabilization unless a fresh
> decision calls for it.

## Scope Of Phase

Remove `schema-gen` from `lpc-model` and downstream feature references that
point at `lpc-model/schema-gen`.

In scope:

- Remove `schema-gen` from `lp-core/lpc-model/Cargo.toml`.
- Remove `schemars` from `lpc-model` dependencies.
- Remove `#[cfg_attr(feature = "schema-gen", ...)]` and
  `schemars::JsonSchema` impls/derives from `lpc-model`.
- Update downstream crate feature definitions that reference
  `lpc-model/schema-gen`.
- Keep code compiling without schema-gen.

Out of scope:

- Removing serde itself.
- Adding slot-shape schema generation.
- Removing schema generation from crates that can still own their own host-side
  schemas without `lpc-model/schema-gen`.

## Code Organization Reminders

- Prefer mechanical deletion for schema-gen attributes.
- Do not introduce compatibility features.
- Keep related dependency edits grouped in each `Cargo.toml`.
- Put tests at the bottom of files if any test edits are required.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/Cargo.toml`
- `lp-core/lpc-source/Cargo.toml`
- `lp-core/lpc-wire/Cargo.toml`
- `lp-vis/lpv-model/Cargo.toml`
- all `lp-core/lpc-model/src/**/*.rs`

Search commands:

```bash
rg -n "schema-gen|schemars|JsonSchema" lp-core/lpc-model lp-core/lpc-source lp-core/lpc-wire lp-vis/lpv-model
```

Expected changes:

- Delete the `schema-gen` feature from `lpc-model`.
- Delete `schemars = ...` from `lpc-model`.
- Delete all `cfg_attr(feature = "schema-gen", ...)` inside `lpc-model`.
- Delete explicit `impl schemars::JsonSchema ...` blocks in `lpc-model`.
- Remove `lpc-model/schema-gen` from dependent feature lists.

Decision:

- Future schema generation must come from slot shapes, not serde/schemars model
  derives.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-source -p lpc-wire
cargo check -p lpc-model
cargo check -p lpc-source
cargo check -p lpc-wire
rg -n "schema-gen|schemars|JsonSchema" lp-core/lpc-model
```
