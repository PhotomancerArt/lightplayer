# Phase 6: Update Builder and Example JSON

## Goal

Update `FixtureBuilder` and example fixture configuration to use the new `MappingConfig` format.

## Changes

### 1. Update FixtureBuilder

**File**: `lp-shared/src/project/builder.rs`

Update `FixtureBuilder` struct:

```rust
pub struct FixtureBuilder {
    output_path: LpPathBuf,
    texture_path: LpPathBuf,
    mapping: MappingConfig,  // UPDATE: Change from String to MappingConfig
    color_order: ColorOrder,
    transform: [[f32; 4]; 4],
}
```

Update `mapping()` method:

```rust
impl FixtureBuilder {
    pub fn mapping(mut self, mapping: MappingConfig) -> Self {  // UPDATE: Change parameter type
        self.mapping = mapping;
        self
    }

    // ... other methods ...
}
```

Update `add()` method to use `MappingConfig`:

```rust
pub fn add(self, builder: &mut ProjectBuilder) -> LpPathBuf {
    // ... existing code ...

    let config = FixtureConfig {
        output_spec: NodeSpecifier::from(self.output_path.as_str()),
        texture_spec: NodeSpecifier::from(self.texture_path.as_str()),
        mapping: self.mapping,  // Already MappingConfig, no conversion needed
        color_order: self.color_order,
        transform: self.transform,
    };

    // ... rest of method ...
}
```

Add import for `MappingConfig`:

```rust
use lp_model::nodes::fixture::mapping::MappingConfig;
```

### 2. Update Example Fixture JSON

**File**: `examples/basic/src/fixture.fixture/node.json`

Update to use new `MappingConfig` format with a 9-ring circular display:

```json
{
  "output_spec": "/src/strip.output",
  "texture_spec": "/src/main.texture",
  "mapping": {
    "PathPoints": {
      "paths": [
        {
          "path_spec": {
            "RingArray": {
              "center": [0.5, 0.5],
              "diameter": 1.0,
              "start_ring_inclusive": 0,
              "end_ring_exclusive": 9,
              "ring_lamp_counts": [1, 8, 12, 16, 24, 32, 40, 48, 60],
              "offset_angle": 0.0,
              "order": "InnerFirst"
            }
          }
        }
      ],
      "sample_diameter": 2.0
    }
  },
  "color_order": "Rgb",
  "transform": [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0]
  ]
}
```

**Configuration Details**:

- 9 rings total (ring indices 0-8)
- InnerFirst ordering (channels assigned inside-out)
- Lamp counts: 1 (center), 8, 12, 16, 24, 32, 40, 48, 60
- Center at (0.5, 0.5) - center of texture
- Diameter 1.0 - full texture width/height
- Total LEDs: 241 (1+8+12+16+24+32+40+48+60)

### 3. Update Builder Defaults

**File**: `lp-shared/src/project/builder.rs`

Update `ProjectBuilder::fixture()` to provide default `MappingConfig`:

```rust
pub fn fixture(&mut self, output_path: &str, texture_path: &str) -> FixtureBuilder {
    FixtureBuilder {
        output_path: LpPathBuf::from(output_path),
        texture_path: LpPathBuf::from(texture_path),
        mapping: MappingConfig::PathPoints {  // UPDATE: Provide default
            paths: vec![PathConfig {
                path_spec: PathSpec::RingArray(RingArray {
                    center: (0.5, 0.5),
                    diameter: 0.8,
                    start_ring_inclusive: 0,
                    end_ring_exclusive: 1,
                    ring_lamp_counts: vec![1],
                    offset_angle: 0.0,
                    order: RingOrder::InnerFirst,
                }),
            }],
            sample_diameter: 2.0,
        },
        color_order: ColorOrder::Rgb,
        transform: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    }
}
```

Add imports:

```rust
use lp_model::nodes::fixture::mapping::{MappingConfig, PathConfig, PathSpec, RingArray, RingOrder};
```

## Success Criteria

- `FixtureBuilder` uses `MappingConfig` instead of `String`
- Example fixture JSON uses new `MappingConfig` format
- Builder provides sensible default `MappingConfig`
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`
