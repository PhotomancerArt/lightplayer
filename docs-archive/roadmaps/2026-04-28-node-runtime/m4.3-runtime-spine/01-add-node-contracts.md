# Phase 1 — Add New `node` Contracts

sub-agent: yes
parallel: -

# Scope of phase

Add the new engine-side runtime spine contracts under
`lp-core/lpc-engine/src/node/`:

- `Node`
- `TickContext`
- `DestroyCtx`
- `MemPressureCtx`
- `PressureLevel`
- `NodeError`

Wire the new module into `lpc-engine` exports without replacing or modifying
the legacy runtime path beyond import/export clarity.

Out of scope:

- Do not replace `nodes::LegacyNodeRuntime`.
- Do not change current `ProjectRuntime` behavior.
- Do not implement resolver logic yet; `TickContext` may expose only the
  minimal data/accessors that can compile before later phases fill in real
  resolution.
- Do not add `Send + Sync` bounds to `Node` unless the compiler requires it
  for an existing concrete use.

# Code organization reminders

- Prefer one concept per file.
- Public contracts and entry points go near the top of files.
- Helpers live near the bottom.
- Tests live at the bottom of the module file.
- Keep `node/` (new spine) distinct from `nodes/` (legacy runtimes).

# Sub-agent reminders

- Do not commit.
- Stay strictly within this phase.
- Do not suppress warnings or add broad `#[allow(...)]`.
- Do not weaken, skip, or delete tests.
- If a context shape needs resolver/artifact types that do not exist yet,
  use narrow placeholder-friendly fields/methods, not stubs that panic.
- Report files changed, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` in this plan directory first.

Create:

```text
lp-core/lpc-engine/src/node/
├── mod.rs
├── node.rs
├── contexts.rs
├── node_error.rs
└── pressure_level.rs
```

Update:

- `lp-core/lpc-engine/src/lib.rs` to `pub mod node;`
- Re-export the important public contracts from `lpc-engine` root if that
  matches existing style:
  - `Node`
  - `TickContext`
  - `DestroyCtx`
  - `MemPressureCtx`
  - `PressureLevel`
  - `NodeError`

Suggested initial shapes:

```rust
pub trait Node {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError>;
    fn destroy(&mut self, ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError>;
    fn handle_memory_pressure(
        &mut self,
        level: PressureLevel,
        ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError>;
    fn props(&self) -> &dyn crate::prop::RuntimePropAccess;
}
```

`NodeError` should be focused and allocation-friendly, not tied to legacy
`crate::error::Error`. Use `alloc::string::String`.

Possible minimal enum:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeError {
    Message(String),
}
```

Add convenience constructors such as `NodeError::msg`.

`PressureLevel` can be simple:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PressureLevel {
    Low,
    Medium,
    High,
    Critical,
}
```

`TickContext` must not expose full mutable tree access. For phase 1, it can
carry and expose:

- current node id: `NodeId`
- current frame id: `FrameId`

Later phases will add resolver/bus/artifact access. Keep fields private if
that helps preserve API flexibility.

`DestroyCtx` and `MemPressureCtx` can initially carry:

- current node id
- current frame id
- optional/empty marker fields if needed

Add unit tests proving:

- `Node` is object-safe with a dummy node.
- `props()` returns a `RuntimePropAccess`.
- context accessors return node/frame ids.
- `PressureLevel` ordering is sensible if derived.
- `NodeError::msg` stores the message.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-engine
cargo test -p lpc-engine node::
```
