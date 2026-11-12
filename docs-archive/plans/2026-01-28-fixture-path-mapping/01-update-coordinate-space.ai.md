# Phase 1: Update Coordinate Space to [0, 1]

## Goal

Update all fixture code to use texture space [0, 1] instead of fixture space [-1, 1]. This standardizes coordinate space across the application.

## Changes

### 1. Update MappingPoint

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

- Update `MappingPoint.center` comment: change from "UV coordinates in fixture space [-1,-1] to [1,1]" to "Texture space coordinates [0, 1]"
- Update default mapping point in `init()`: change center from `[0.0, 0.0]` to `[0.5, 0.5]` (center of texture space)

### 2. Update Render Code

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

- Remove conversion from fixture space to texture space in `render()`
- Update comments: remove references to fixture space [-1, 1]
- Update transform application: transform now works with [0, 1] input (was [-1, 1])
- Simplify coordinate handling since input is already in texture space

### 3. Update State Extraction

**File**: `lp-engine/src/project/runtime.rs`

- Update `apply_transform_2d` usage comments: change from "fixture space [-1, 1] to texture space [0, 1]" to "texture space [0, 1] to texture space [0, 1]"
- Update comments in state extraction code

### 4. Update All Comments

Search for and update all comments referencing:

- "fixture space [-1, 1]" → "texture space [0, 1]"
- "Fixture space" → "Texture space" where appropriate
- Any coordinate space documentation

## Success Criteria

- `MappingPoint.center` uses texture space [0, 1]
- All comments updated to reflect [0, 1] coordinate space
- Render code expects [0, 1] input (no conversion from [-1, 1])
- Default mapping point uses [0.5, 0.5] (center of texture)
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`
