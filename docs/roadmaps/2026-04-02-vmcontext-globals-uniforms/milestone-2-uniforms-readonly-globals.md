# Milestone II: Uniforms and Readonly Globals

## Goal

Collect uniform and global metadata from GLSL, build the full dynamic VMContext type, and enable
readonly access to uniforms and globals.

## Suggested Plan Name

`vmcontext-globals-uniforms-milestone-2`

## Scope

**In scope:**

- Collect uniform declarations from naga AST
- Collect global variable declarations from naga AST
- Build complete VMContext GlslType with uniforms and globals sections
- Path-based offset lookup (caching)
- Emit `Op::Load` with `vmctx + offset` for uniform/global reads
- Name-based uniform access API for host
- Readonly global support (globals that are never written)

**Out of scope:**

- Global writes (mutable globals)
- `_init()` function with real initialization
- Defaults buffer
- Global reset between invocations

## Key Decisions

1. **Uniforms by name, not binding**: Host looks up offsets via `GlslType` metadata
2. **Uniforms read-only**: Shader cannot write to uniform section
3. **Globals initially readonly**: Mutability deferred to Milestone III

## Deliverables

- `lps-frontend` collects uniform/global metadata
- `VmContextBuilder` in `lpvm` constructs full type
- Path-based offset resolution with caching
- Filetests demonstrating uniform reads
- Filetests demonstrating global reads

## Dependencies

- Milestone I: VMContext Foundation

## Estimated Scope

~600 lines: metadata collection (200), type building (150), load emission (150), tests (100)
