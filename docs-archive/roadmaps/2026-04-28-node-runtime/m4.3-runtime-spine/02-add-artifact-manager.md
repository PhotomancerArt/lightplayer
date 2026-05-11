# Phase 2 — Add Runtime `artifact` Manager

sub-agent: yes
parallel: -

# Scope of phase

Add runtime artifact state/cache support under
`lp-core/lpc-engine/src/artifact/`:

- `ArtifactManager<A>`
- `ArtifactRef`
- `ArtifactEntry<A>`
- `ArtifactState<A>`
- `ArtifactError`

Implement real state/refcount/content-frame behavior with closure-based
loading. Do not introduce `ProjectDomain`.

Out of scope:

- Do not cut over legacy project loading.
- Do not replace `project::legacy_loader`.
- Do not add filesystem watching.
- Do not make artifact references self-dropping unless you can do so without
  unsafe lifetime tricks. Explicit release APIs are acceptable for M4.3.
- Do not require `A: Clone` for normal manager operation unless tests need
  cloned dummy values.

# Code organization reminders

- One concept per file.
- Keep public API near the top.
- Keep helper functions near the bottom.
- Tests live at the bottom of the module file or in the most relevant file.
- Prefer simple, safe ownership over clever handle internals.

# Sub-agent reminders

- Do not commit.
- Do not expand scope into domain runtime.
- Do not suppress warnings.
- Do not weaken tests.
- If you believe `ArtifactRef` needs unsafe pointer semantics, stop and
  report; do not improvise unsafe code.
- Report files changed, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` first.

Create:

```text
lp-core/lpc-engine/src/artifact/
├── mod.rs
├── artifact_manager.rs
├── artifact_ref.rs
├── artifact_entry.rs
├── artifact_state.rs
└── artifact_error.rs
```

Update `lp-core/lpc-engine/src/lib.rs`:

```rust
pub mod artifact;
pub use artifact::{ArtifactEntry, ArtifactError, ArtifactManager, ArtifactRef, ArtifactState};
```

Use current names:

- `lpc_source::SrcArtifactSpec`
- `lpc_model::FrameId`

Suggested safe shape:

```rust
pub struct ArtifactRef {
    handle: u32,
}

impl ArtifactRef {
    pub fn handle(&self) -> u32;
}
```

`ArtifactEntry<A>` should include:

- `spec: SrcArtifactSpec`
- `state: ArtifactState<A>`
- `refcount: u32`
- `content_frame: FrameId`
- `error: Option<ArtifactError>` if not encoded in state

`ArtifactState<A>` should cover at least:

- `Resolved`
- `Loaded(A)`
- `Prepared(A)`
- `Idle(A)`
- `ResolutionError(String)`
- `LoadError(String)`
- `PrepareError(String)`

`ArtifactManager<A>` should include:

- map by handle
- index by `SrcArtifactSpec`
- next handle

Required behavior:

- `acquire_resolved(spec, frame) -> ArtifactRef`
  - Reuse existing entry if spec already known.
  - Increment refcount.
  - Unknown spec creates `Resolved` entry with `content_frame = frame`.
- `load_with(&ref, frame, loader)` or similar:
  - If state is `Resolved`, call `loader(&SrcArtifactSpec) -> Result<A, ArtifactError>`.
  - On success, transition to `Loaded(A)` and set/bump `content_frame = frame`.
  - On failure, transition to `LoadError`.
  - If already `Loaded`/`Prepared`/`Idle`, preserve or promote as appropriate.
- `release(&ref, frame)`:
  - Decrement refcount.
  - If refcount reaches zero and state holds a payload, move it to `Idle`.
  - If refcount reaches zero and state is `Resolved` or error, keep it
    inert or remove it; choose the simpler behavior and document it.
- accessors:
  - `entry(&ref) -> Option<&ArtifactEntry<A>>`
  - `content_frame(&ref) -> Option<FrameId>`
  - `refcount(&ref) -> Option<u32>`

Add tests proving:

- acquire of same spec reuses handle and increments refcount.
- release decrements refcount.
- loaded artifact moves to idle when refcount reaches zero.
- load success bumps `content_frame`.
- load failure records `LoadError`.
- unknown handles return a structured `ArtifactError`.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-engine
cargo test -p lpc-engine artifact::
```
