# ADR 2026-06-12: AssetSlot Runtime Refresh

## Status

Accepted

## Context

The project registry can discover shader source files as effective assets, but
the runtime previously loaded shader source as plain strings. After a
filesystem refresh reported that `/shader.glsl` changed, the engine knew an
asset changed but the running shader node had no retained asset identity or
revision to decide whether its cached shader source should be invalidated.

The older authored model also had source-specific slot types such as
`SourcePathSlot`, `SourceFileSlot`, and `ShaderSource`. That made source files
special even though the emerging project model treats shader source, fixture
SVGs, future images, and inline bodies as assets.

## Decision

Use `AssetSlot` as the authored slot shape for file-or-inline assets.

`AssetSlotValue` is authored storage:

- `Artifact(ArtifactSpec)` references an artifact relative to the containing
  definition;
- `InlineText { extension, text }` stores UTF-8 text with an optional language
  or extension hint;
- `InlineBytes { extension, bytes }` stores raw bytes with an optional extension
  hint.

`AssetLocation` remains effective project identity after registry discovery.
Artifact-backed slots resolve to `AssetLocation::Artifact`; inline asset slots
resolve to `AssetLocation::Inline { owner, path }`.

`AssetContentType` remains a coarse bridge for current specialized consumers. It
is not the final MIME or requirements model.

Runtime nodes do not get reattached for same-location asset body changes.
`Engine::apply_project_changes` routes changed `AssetLocation`s through
`ProjectRuntimeIndex` to affected runtime nodes. Nodes may implement
`NodeRuntime::refresh_asset` to compare the effective asset revision they last
consumed with the registry's current asset revision and invalidate only their
own cached state.

Shader and compute shader nodes retain their source asset location and revision.
When that asset changes, they read the effective text through
`ProjectRegistry::read_asset_text_if_changed`, replace cached source text, clear
compile errors, and drop compiled shader state so the next render/produce path
compiles from the new source.

## Consequences

Filesystem edits to shader source files update existing runtime shader nodes
without full project reload or runtime node reattach.

Source files are now assets in the model. Source-specific names remain
appropriate only for actual source text handling, not for asset identity or
authored asset references.

The runtime refresh hook is generic enough for future image or binary asset
consumers without adding image loading in this milestone.

The implementation still assumes statically discoverable asset references.
Generic slot requirement metadata, a generic reference walker, richer MIME or
format handling, and image asset loading remain future work.
