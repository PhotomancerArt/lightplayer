# Overview: Restore Debug UI

## Goal

Restore the debug UI to fully display textures, shaders, and fixtures with their actual data and configurations.

## Problem

After the filesystem-based projects refactor, node configs are no longer stored in ProjectConfig. The debug UI needs access to these configs to display:
- Texture images and metadata
- Shader GLSL code and compilation status
- Fixture details with texture previews and mapping overlays

## Solution

Store node configs in their respective runtimes. This keeps configs with their runtimes and avoids reloading from the filesystem. Update debug UI panel functions to use the existing helper functions (`render_texture()`, `render_shader_panel()`, `render_fixture()`) which already have good implementations.

## Approach

1. Add `config` fields to all node runtime structs
2. Update `init()` methods to store configs
3. Add getter methods to access configs
4. Update panel functions to iterate nodes, get configs/data, and call helper functions
5. Remove dead code annotations from helper functions

## Success Criteria

- Textures panel shows actual texture images with metadata
- Shaders panel shows GLSL code and compilation status
- Fixtures panel shows fixture details with texture previews
- All code compiles without warnings
- Debug UI displays all node information correctly

