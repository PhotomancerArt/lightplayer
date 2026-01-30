# Phase 1: Add Project Variants to ClientRequest

## Goal

Add project management variants to `ClientRequest` enum to match `ServerRequest` structure.

## Tasks

1. Update `lp-model/src/message.rs`:
   - Add project management variants to `ClientRequest` enum:
     - `LoadProject { path: String }`
     - `UnloadProject { handle: ProjectHandle }`
     - `ProjectRequest { handle: ProjectHandle, request: ProjectRequest }`
     - `ListAvailableProjects`
     - `ListLoadedProjects`
   - Add necessary imports (`ProjectHandle`, `ProjectRequest`)
   - Update serialization tests to include project variants

2. Verify serialization:
   - Test round-trip serialization for each new variant
   - Ensure JSON tag names match `ServerRequest` structure

## Success Criteria

- [ ] `ClientRequest` has all project management variants
- [ ] All variants serialize/deserialize correctly
- [ ] Tests pass
- [ ] Code compiles without warnings
