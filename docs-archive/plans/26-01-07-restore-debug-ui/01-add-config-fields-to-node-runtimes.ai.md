# Phase 1: Add config fields to node runtimes

## Goal

Add `config` fields to all node runtime structs to store their configurations.

## Tasks

1. Add `config: TextureNode` field to `TextureNodeRuntime`
2. Add `config: ShaderNode` field to `ShaderNodeRuntime`
3. Add `config: FixtureNode` field to `FixtureNodeRuntime`
4. Add `config: OutputNode` field to `OutputNodeRuntime`
5. Update `new()` methods to initialize configs with default/placeholder values

## Success Criteria

- All node runtime structs have `config` fields
- `new()` methods initialize configs appropriately
- Code compiles without errors

