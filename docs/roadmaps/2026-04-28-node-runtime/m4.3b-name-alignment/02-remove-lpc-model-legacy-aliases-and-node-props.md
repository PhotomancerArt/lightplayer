# Phase 2 — Remove `lpc-model` Legacy Aliases and `NodeProps`

sub-agent: yes
parallel: -

# Scope of phase

Remove legacy compatibility names from `lpc-model`:

- Remove `NodeSpecifier` compatibility alias.
- Remove `nodes` compatibility module.
- Remove `NodeProps` from `lpc-model`.

Update workspace call sites to canonical names:

- `NodeSpec`
- `NodeId`

Out of scope:

- Introducing a replacement property trait in another crate.
- Renaming source/wire/view types.
- Behavior changes to node identifiers, paths, or property values.

# Code organization reminders

- Prefer one concept per file.
- Keep canonical node concepts in `lp-core/lpc-model/src/node/`.
- Do not leave compatibility aliases in crate roots or broad prelude-style
  modules.
- Tests belong at the bottom of the module they cover.

# Sub-agent reminders

- Do not commit.
- Do not expand scope into semantic node/runtime changes.
- Do not suppress warnings or weaken tests.
- If a real consumer of `NodeProps` appears outside the known
  self-tests/re-exports, stop and report the call site and likely home.
- Report changed files, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` in this directory first.

In `lp-core/lpc-model/src/node/`:

- Remove `node_props.rs` if its only remaining consumers are internal
  tests/docs/re-exports.
- Remove `mod node_props;` and `pub use` entries for `NodeProps`.
- Keep `node_spec.rs` as the canonical node specifier file/type.

In `lp-core/lpc-model/src/lib.rs`:

- Remove `pub type NodeSpecifier = NodeSpec` or any equivalent alias.
- Remove the legacy `nodes` module if present.

Update call sites:

- Replace `NodeSpecifier` with `NodeSpec`.
- Replace `lpc_model::nodes::...` paths with canonical module/root exports.
- Remove `NodeProps` re-exports from `lpv-model` or other consumers if
  they only forwarded the old trait.

Search targets:

```bash
rg "NodeSpecifier|NodeProps|lpc_model::nodes|pub mod nodes|mod nodes" .
```

Expected result:

- No active Rust code uses `NodeSpecifier`, `NodeProps`, or
  `lpc_model::nodes`.
- Historical plan text may mention these names, but active READMEs/design
  docs should not recommend them.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-model
cargo check -p lpv-model
cargo test -p lpc-model
```
