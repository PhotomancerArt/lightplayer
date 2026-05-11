## Texture Wire Transport

- **Idea:** Add a wire protocol for texture/render-product references where
  runtime values send stable texture identities separately from pixel payload
  updates.
- **Why not now:** M2.1 should establish runtime value domains and render-product
  handles, not design the full resource transport protocol.
- **Useful context:** Actual texture data should travel through a separate
  resource/update channel so two references to the same texture do not duplicate
  pixels. This could land before or after the legacy migration, depending on
  where texture preview/debugging pressure appears first.
