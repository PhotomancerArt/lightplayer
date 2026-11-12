# Phase 3: Add config() getter methods to node runtimes

## Goal

Add public getter methods to access configs from node runtimes.

## Tasks

1. Add `pub fn config(&self) -> &TextureNode` to `TextureNodeRuntime`
2. Add `pub fn config(&self) -> &ShaderNode` to `ShaderNodeRuntime`
3. Add `pub fn config(&self) -> &FixtureNode` to `FixtureNodeRuntime`
4. Add `pub fn config(&self) -> &OutputNode` to `OutputNodeRuntime`

## Success Criteria

- All node runtimes have `config()` getter methods
- Methods return immutable references to configs
- Code compiles without errors

