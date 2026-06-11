# ADR 2026-06-11: Asset Source Model

## Status

Accepted

## Context

`ProjectRegistry` derives a project view by walking the loaded project graph.
That graph includes node definitions and assets. Early registry code treated
every asset as a non-definition file identified by `ArtifactLocation`, which was
enough for shader source files but too narrow for the model we need.

Source files are assets, but not all assets are source files. Future image
assets need the same reference/discovery/materialization path. Source can also
be inline today, and other asset kinds may gain inline bodies later.

## Decision

Make assets a first-class `lpc-model::asset` concept.

`ArtifactLocation` remains durable file identity. It answers "which file-like
artifact is this?"

`AssetSource` is project asset identity. It answers "where does this referenced
project asset come from?" Initial variants are:

- artifact-backed assets, identified by `ArtifactLocation`;
- inline assets, identified by owner `NodeDefLocation` plus `SlotPath`;
- URL assets as reserved future vocabulary.

`AssetKind` is the specialization point for how callers should interpret or
materialize bytes/text. Initial kinds include shader source, compute shader
source, fixture SVG, image, text, and binary.

`ProjectInventory.assets` is keyed by `AssetSource`, and `AssetEntry` carries
the `AssetKind`, state, and revision. Inline assets are inventory entries, but
they are not registered in `ArtifactStore`. Artifact-backed assets continue to
use `ArtifactStore` for durable location tracking, filesystem reads, overlay
body replacement, and filesystem change revisions.

Source-file APIs remain named `source` where they specifically deal with
authored source text. They now sit under the asset model: a file-backed
`SourceFileRef` carries an `AssetSource`, and source materialization is a
text-specific wrapper over asset-backed bytes plus inline slot text.

Normal registry operation does not scan or snapshot every file. Assets and
definitions are discovered by walking static authored references in the current
effective project graph.

## Consequences

The project view can represent file-backed shader sources, inline shader
sources, fixture SVGs, and future image assets with one inventory shape.

Engine/runtime consumers can key loaded assets by `AssetSource` instead of
assuming every runtime asset maps directly to a file.

The model still assumes statically discoverable references. Dynamic asset or
node-definition references will need a later design.

`AssetKind::Image` exists as vocabulary before image loading is implemented, so
image support can reuse the same identity and inventory path when it arrives.
