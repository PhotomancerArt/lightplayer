# Scope of Work

Milestone 1 reorganizes the legacy runtime stack back into the `lpc-*` crate
family. The work removes the temporary `lpl-*` split and the hook registration
mechanism, while preserving the existing `LegacyProjectRuntime` behavior and the
legacy shader -> texture -> fixture -> output compatibility slice.

This plan does not design the final `Engine` API, implement pull-based bus
providers, introduce queryable visual outputs, port all legacy nodes to the new
`Node` trait, or remove the `LegacyProjectRuntime` name.

# File Structure

```text
lp-core/
├── lpc-model/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── legacy/                    # NEW only if foundation-only legacy types remain
│           └── mod.rs
├── lpc-source/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── legacy/                    # NEW: authored legacy configs/source specs
│           ├── mod.rs
│           ├── glsl_opts.rs
│           └── nodes/
│               ├── mod.rs
│               ├── kind.rs
│               ├── texture/
│               │   ├── mod.rs
│               │   ├── config.rs
│               │   └── format.rs
│               ├── shader/
│               │   ├── mod.rs
│               │   └── config.rs
│               ├── fixture/
│               │   ├── mod.rs
│               │   ├── config.rs
│               │   └── mapping.rs
│               └── output/
│                   ├── mod.rs
│                   └── config.rs
├── lpc-wire/
│   ├── Cargo.toml                     # UPDATE: may depend on lpc-source
│   └── src/
│       ├── lib.rs
│       └── legacy/                    # NEW: legacy state and protocol payloads
│           ├── mod.rs
│           ├── project/
│           │   ├── mod.rs
│           │   └── api.rs
│           └── nodes/
│               ├── mod.rs
│               ├── texture/state.rs
│               ├── shader/state.rs
│               ├── fixture/state.rs
│               └── output/state.rs
├── lpc-engine/
│   ├── Cargo.toml                     # UPDATE: owns concrete legacy runtime deps
│   └── src/
│       ├── lib.rs
│       ├── legacy/                    # NEW: concrete legacy runtime implementation
│       │   ├── mod.rs
│       │   ├── project.rs             # moved legacy project operations
│       │   ├── nodes/
│       │   └── output/
│       └── legacy_project/
│           ├── mod.rs                 # UPDATE: remove hooks module
│           ├── legacy_loader.rs
│           └── project_runtime/
│               ├── core.rs            # UPDATE: direct method implementations
│               └── types.rs
├── lpl-model/                         # DELETE
└── lpl-runtime/                       # DELETE
```

# Conceptual Architecture

```text
lpc-model
  foundation identity/path/frame types only

      ↑

lpc-source::legacy
  authored legacy node configs and source-side shapes
  TextureConfig / ShaderConfig / FixtureConfig / OutputConfig
  NodeKind / NodeConfig if they remain source-facing

      ↑

lpc-wire::legacy
  legacy runtime state and client/server payloads
  NodeState / NodeChange / NodeDetail / ProjectResponse
  SerializableProjectResponse / LegacyMessage aliases

      ↑

lpc-engine::legacy + lpc-engine::legacy_project
  LegacyProjectRuntime owns fs/output/graphics/frame state
  concrete TextureRuntime / ShaderRuntime / FixtureRuntime / OutputRuntime
  direct init_nodes / tick / handle_fs_changes / get_changes methods
```

Dependency direction is intentional:

```text
lpc-model <- lpc-source <- lpc-wire <- lpc-engine
```

`lpc-wire` may depend on `lpc-source` because authored source/config payloads can
be sent over the wire. `lpc-model` remains foundation-only and does not depend on
source or wire crates.

# Main Components

## Legacy Source

Legacy authored config and source-facing node definitions move from
`lpl-model/src/nodes` into `lpc-source::legacy`. This includes the texture,
shader, fixture, and output config modules, fixture mapping config, texture
format, GLSL compile option structs, `NodeKind`, and the legacy `NodeConfig`
trait if it remains tied to authored config.

## Legacy Wire

Legacy runtime state and client/server protocol payloads move from
`lpl-model/src/project` and `lpl-model/src/nodes/*/state.rs` into
`lpc-wire::legacy`. These types may refer to source configs through
`lpc-source::legacy` and wire status/envelope types through existing `lpc-wire`
modules.

The legacy message aliases should remain available under `lpc_wire::legacy`
rather than through an `lpl-model` crate.

## Legacy Engine

Concrete legacy runtimes move from `lpl-runtime` into `lpc-engine::legacy`.
`LegacyProjectRuntime` keeps its public name, but node operations become direct
methods again instead of delegating through a global hook registry.

The hook surface is removed:

- no `LegacyProjectHooks`;
- no `set_project_hooks`;
- no `with_hooks`;
- no `lpl_runtime::install()` call sites.

## Compatibility Slice

The behavior to preserve is the existing legacy shader -> texture -> fixture ->
output flow:

```text
fixture tick/render
  -> requests texture through RenderContext
    -> lazily renders shaders targeting that texture for the current frame
  -> samples texture into fixture lamp/output data
  -> writes output buffers
output render/flush
  -> visits mutated outputs
```

Filesystem update behavior for node configs, shader source changes, node
deletion, and partial state updates remains part of the slice.
