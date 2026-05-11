### What was built

- Legacy authored configs and source-facing specs moved to `lpc-source::legacy` (including `NodeKind`, per-kind configs, fixture mapping, GLSL options).
- Legacy runtime state, project API types, node change/detail shapes, serializable response wrappers, and legacy message aliases moved to `lpc-wire::legacy`.
- Concrete legacy node runtimes (texture, shader, fixture, output) and legacy project integration moved to `lpc-engine::legacy`; `LegacyProjectRuntime` remains the public runtime name under `lpc_engine::legacy_project`.
- `lpl-model` and `lpl-runtime` workspace crates deleted; callers import from `lpc_*::legacy` (and `legacy_project`) instead.
- `LegacyProjectHooks`, global hook registration, `with_hooks`, and `lpl_runtime::install()` (and related call sites) removed; `LegacyProjectRuntime` methods call the legacy implementation directly again.
- Roadmap docs under `docs/roadmaps/2026-05-01-runtime-core/` updated to describe the folded `lpc-*` layout.

### Decisions for future reference

#### Remove `lpl-*` crates rather than keeping shims

- **Decision:** Eliminate `lpl-model` and `lpl-runtime` in this milestone; no long-lived compatibility re-export crates.
- **Why:** The extra crate boundary duplicated the “legacy vs spine” split without a stable second domain; folding into `lpc-*` modules keeps dependency direction obvious and avoids global registration (`install()`).
- **Rejected alternatives:** Keep thin `lpl-*` shims re-exporting `lpc_*::legacy` — adds indirection and ongoing import churn; defer removal — would prolong two naming schemes across firmware, server, and CLI.
- **Revisit when:** If a future second domain (e.g. visual-only stack) truly needs a separate published crate, split deliberately with a new design, not as a restoration of the old `lpl-*` split.

#### Place legacy source vs wire vs engine concerns by dependency direction

- **Decision:** Configs/source shapes in `lpc-source::legacy`; wire-visible state and protocol in `lpc-wire::legacy`; execution in `lpc-engine::legacy`. Allow `lpc-wire` to depend on `lpc-source` where payloads carry authored config.
- **Why:** Matches the existing `lpc-model` ← `lpc-source` ← `lpc-wire` ← `lpc-engine` direction; keeps `lpc-model` foundation-only.
- **Rejected alternatives:** Put everything in `lpc-engine` — would pull source parsing shapes into the engine dependency cone for wire-only consumers; keep protocol types in a separate “legacy model” crate — that became `lpl-model` and was removed as redundant.

#### Drop hook registration; make `LegacyProjectRuntime` direct again

- **Decision:** Remove `LegacyProjectHooks` / `set_project_hooks` / `with_hooks` and all `install()` paths; implement `init_nodes`, `tick`, `handle_fs_changes`, and `get_changes` on `LegacyProjectRuntime` without a global registry.
- **Why:** The hook layer existed to bridge `lpc-engine` and the split-out `lpl-runtime`; after merge, it only forced ordering bugs and “hooks not installed” failure modes.
- **Rejected alternatives:** Rename and keep hooks for test injection — would preserve global mutable registration; split traits for “spine vs legacy” without merging crates — would keep the same problem with more types.
- **Revisit when:** If tests need swappable legacy backends, prefer explicit constructor injection or a small engine-owned trait object, not process-wide registration.
