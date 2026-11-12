# Phase 1: Add Error Variant and Color Order Enum

## Goal

Add `WrongNodeKind` error variant and `ColorOrder` enum to support type-safe node resolution and color ordering.

## Dependencies

- `lp-model` Phase 1
- `lp-engine` Phase 3

## Implementation

### 1. Add WrongNodeKind Error Variant

**File**: `lp-engine/src/error.rs`

```rust
pub enum Error {
    // ... existing variants ...
    WrongNodeKind {
        specifier: String,
        expected: NodeKind,
        actual: NodeKind,
    },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            // ... existing matches ...
            Error::WrongNodeKind { specifier, expected, actual } => {
                write!(f, "Wrong node kind for {}: expected {:?}, got {:?}", specifier, expected, actual)
            }
        }
    }
}
```

### 2. Add ColorOrder Enum

**File**: `lp-model/src/nodes/fixture/config.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorOrder {
    Rgb,
    Grb,
    Rbg,
    Gbr,
    Brg,
    Bgr,
}

impl ColorOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            ColorOrder::Rgb => "rgb",
            ColorOrder::Grb => "grb",
            ColorOrder::Rbg => "rbg",
            ColorOrder::Gbr => "gbr",
            ColorOrder::Brg => "brg",
            ColorOrder::Bgr => "bgr",
        }
    }
    
    pub fn bytes_per_pixel(&self) -> usize {
        3  // All RGB variants are 3 bytes
    }
    
    /// Write RGB values to buffer in the correct order
    pub fn write_rgb(&self, buffer: &mut [u8], offset: usize, r: u8, g: u8, b: u8) {
        if offset + 3 > buffer.len() {
            return;
        }
        match self {
            ColorOrder::Rgb => {
                buffer[offset] = r;
                buffer[offset + 1] = g;
                buffer[offset + 2] = b;
            }
            ColorOrder::Grb => {
                buffer[offset] = g;
                buffer[offset + 1] = r;
                buffer[offset + 2] = b;
            }
            ColorOrder::Rbg => {
                buffer[offset] = r;
                buffer[offset + 1] = b;
                buffer[offset + 2] = g;
            }
            ColorOrder::Gbr => {
                buffer[offset] = g;
                buffer[offset + 1] = b;
                buffer[offset + 2] = r;
            }
            ColorOrder::Brg => {
                buffer[offset] = b;
                buffer[offset + 1] = r;
                buffer[offset + 2] = g;
            }
            ColorOrder::Bgr => {
                buffer[offset] = b;
                buffer[offset + 1] = g;
                buffer[offset + 2] = r;
            }
        }
    }
}
```

### 3. Update FixtureConfig

**File**: `lp-model/src/nodes/fixture/config.rs`

```rust
pub struct FixtureConfig {
    pub output_spec: NodeSpecifier,
    pub texture_spec: NodeSpecifier,
    pub mapping: String,  // todo!() - will be structured type later
    pub lamp_type: String,  // todo!() - will be enum later
    pub color_order: ColorOrder,  // Changed from String
    pub transform: [[f32; 4]; 4],
}
```

### 4. Update Tests

Update any tests that create `FixtureConfig` to use `ColorOrder::Rgb` instead of string.

## Success Criteria

- All code compiles
- `WrongNodeKind` error displays correctly
- `ColorOrder` enum serializes/deserializes correctly
- `ColorOrder::write_rgb()` writes colors in correct order
- Tests pass

## Notes

- Color order enum only supports RGB variants for now (no RGBA)
- `write_rgb()` handles buffer bounds checking
- Error message includes specifier, expected, and actual kinds for debugging
