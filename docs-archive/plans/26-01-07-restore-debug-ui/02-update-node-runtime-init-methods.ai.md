# Phase 2: Update node runtime init() methods to store configs

## Goal

Update all `NodeLifecycle::init()` implementations to store the config parameter in the runtime.

## Tasks

1. Update `TextureNodeRuntime::init()` to store `config` in `self.config`
2. Update `ShaderNodeRuntime::init()` to store `config` in `self.config`
3. Update `FixtureNodeRuntime::init()` to store `config` in `self.config`
4. Update `OutputNodeRuntime::init()` to store `config` in `self.config`

## Success Criteria

- All `init()` methods store configs in runtimes
- Code compiles without errors
- Existing tests still pass

