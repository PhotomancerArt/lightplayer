# Phase 7 — Runtime Spine Integration Tests

sub-agent: yes
parallel: -

# Scope of phase

Add focused integration-style tests for the new M4.3 engine spine. These
tests should prove the new modules work together without cutting over the
legacy runtime.

Out of scope:

- Do not port legacy nodes.
- Do not change app/server behavior.
- Do not add wire/view prop deltas.
- Do not add large fixtures or slow end-to-end tests.

# Code organization reminders

- Prefer concise tests with helpers at the bottom.
- Use dummy artifacts/nodes/props rather than legacy runtime nodes.
- Keep test helpers local unless multiple modules need them.
- Avoid debug prints.

# Sub-agent reminders

- Do not commit.
- Do not expand into implementation refactors unless a small fix is needed
  for tests to compile.
- Do not suppress warnings.
- Do not weaken tests.
- If the earlier phases left an API gap, report it rather than redesigning
  the whole spine in tests.
- Report files changed, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md`, `00-design.md`, and phase files 01-06 first.

Add tests either:

- under existing module test blocks, if each behavior belongs to a module,
  or
- as `lp-core/lpc-engine/tests/runtime_spine.rs` if crate integration tests
  can access the public APIs cleanly.

Cover at least:

1. **Artifact lifecycle**
   - acquire artifact
   - load with closure
   - release to idle
   - verify content frame/refcount

2. **Literal/default resolution**
   - dummy `SrcNodeConfig` with literal override resolves to `LpsValueF32`
   - artifact default resolves when no override exists

3. **Bus resolution**
   - claim/publish on `Bus`
   - resolver reads bus value and stores `ResolvedSlot`

4. **NodeProp resolution**
   - dummy target implements `RuntimePropAccess`
   - resolver reads `outputs` prop from target
   - non-outputs target is rejected or falls back according to phase-4
     behavior

5. **TickContext**
   - dummy `Node::tick` calls `ctx.resolve`
   - `changed_since` sees resolver cache frame
   - `artifact_changed_since` compares content frame

6. **Side-by-side legacy boundary**
   - a simple compile/test assertion that `LegacyNodeRuntime` is still
     exported from `nodes` and the new `Node` is exported from `node`.

Keep tests deterministic and fast. Do not involve shader compilation or
firmware targets.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-engine
cargo test -p lpc-engine runtime_spine
cargo test -p lpc-engine node::
cargo test -p lpc-engine resolver::
cargo test -p lpc-engine artifact::
```
