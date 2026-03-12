# Phase 8: Demo and Templates

## Goal

Update demo_project.rs and project templates to use Rgba16. No migration machinery.

## Tasks

### 8.1 demo_project.rs

- Texture nodes created with Rgba16 (or rely on hardcoded runtime from phase 2)
- If demo constructs TextureConfig explicitly, ensure format is Rgba16 if we add format to config later; for now runtime hardcodes it
- Verify demo renders correctly on ESP32

### 8.2 Project templates

- `lp-cli create project` and similar: any template texture nodes should produce Rgba16
- `lp-model` project API: `TextureFormat::Rgba8` usages → `TextureFormat::Rgba16` where appropriate (see grep from 00-notes)

### 8.3 Examples

- `examples/basic`, `examples/fast`: update if they have texture config
- Shaders unchanged (they output Q32; runtime converts to u16)

### 8.4 Cleanup

- Remove or deprecate Rgba8-only paths if everything is 16-bit
- Update tests that assumed u8 texture
