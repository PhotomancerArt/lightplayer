# lps-shared

Shared shader types, utilities, and common code. Used by the frontend and backend to handle semantic
shader concepts, not specific to lpir or any implementation detail.

Texture vocabulary (`TextureBindingSpec`, `TextureStorageFormat`, filters,
wraps, shape hints, `LpsTexture2DDescriptor`, `LpsTexture2DValue`) lives in
`src/texture_format.rs` and is shared with filetests and `lp-shader`. See
[`docs/design/lp-shader-texture-access.md`](../../docs/design/lp-shader-texture-access.md)
for how these pieces fit together.
