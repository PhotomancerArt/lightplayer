# Overview: Engine Core and Model Refactor

## Goal

Get a working end-to-end test that:
- Loads a project from memory filesystem
- Initializes nodes (texture + shader + fixture + output)
- Renders a frame
- Syncs with a client
- Hot-reloads a shader file change

## Philosophy

- **Minimal first**: Get a fully working system ASAP, then iterate
- **Use `todo!()`**: Mark unfinished sections with `todo!()` macro instead of comments
- **Incremental**: Small steps that keep things compiling and testable

## Approach

We'll build incrementally across three crates:
1. **lp-model**: Core types and API types for client sync
2. **lp-engine**: Project loading, node runtime, rendering, sync API
3. **lp-engine-client**: Client view and sync logic

Each phase will compile and be testable, even if functionality is stubbed with `todo!()`.

## Success Criteria

- All code compiles
- End-to-end test passes
- Can load project, initialize nodes, render frame, sync with client
- Hot-reload test works (manual trigger is fine)
