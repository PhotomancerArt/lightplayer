# M2.4 Future Work

## Texture Node As Many-To-Many Materialization Boundary

- **Idea:** Reintroduce `TextureNode` as a first-class runtime materialization
  or cache node for many-to-many visual/fixture relationships.
- **Why not now:** M2.4 does not have a first-class texture resource, so making
  texture own materialization would be more ceremony than truth.
- **Useful context:** This preserves an important idea from old LightPlayer:
  complex projects may route several visuals and fixtures through shared texture
  targets or caches.

## First-Class Texture Resource

- **Idea:** Add a texture resource alongside runtime buffers and render products
  if runtime/shared materialized textures become a core concept.
- **Why not now:** The MVP render flow can have fixtures own transient
  full-texture materialization from lazy shader render products.
- **Useful context:** This should be considered before bringing texture nodes
  back as true resource owners.
