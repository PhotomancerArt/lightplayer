# Future Work

## WGSL Source Support

- **Idea:** Add `wgsl = """..."""` and path language detection for WGSL.
- **Why not now:** The current compiler/runtime path is GLSL-first; adding a
  second frontend would expand this work beyond source loading.
- **Useful context:** `ShaderSource` should leave room for additional language
  variants without changing the node-facing versioned source API.

## Shader Imports And Dependency Graphs

- **Idea:** Track imported shader/library files as dependencies of a compiled
  shader source.
- **Why not now:** The immediate reload bug only needs direct source changes.
- **Useful context:** `SourceVersion` should be opaque enough to later include
  transitive dependency revisions.

## Binary Artifact Materializers

- **Idea:** Add first-class binary source specs with `path` and `base64` forms
  for LUTs, images, meshes, or fixture data.
- **Why not now:** Shader source and node defs are the current product pressure.
- **Useful context:** Keep artifact identity/revision separate from resident
  bytes so binary data can decode lazily.

## Node Invocation Overrides

- **Idea:** Add invocation-owned bindings, overrides, labels, or enable flags
  beside the `def` field in `[nodes.<name>]`.
- **Why not now:** This plan reserves the namespace but only needs `def` for
  one-file examples and source reload.
- **Useful context:** Keeping `nodes.<name>.def` separate from invocation-owned
  fields is the reason the plan does not use direct `[nodes.x] kind = ...` as
  the canonical inline shape.

## Library-Backed Artifact Resolution

- **Idea:** Resolve `lib:package/item` locators for node defs and shader source.
- **Why not now:** Path and inline sources must be made clean first.
- **Useful context:** Authored fields should stay named `path` where they mean
  relative locator, while `ArtifactLocator` remains capable of parsing `lib:`.
