//! Input definitions and runtime values.

use alloc::string::String;
use alloc::vec::Vec;

/// Declared type of an effect input (Rust/WGSL-style names + semantic types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxInputType {
    F32,
    I32,
    Bool,
    Vec3,
    Color,
    Palette,
}

/// UI hint for presenting an input (optional).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxPresentation {
    Slider,
    Toggle,
    Choice,
    ColorPicker,
    PalettePicker,
}

/// One labeled option for `ui = { choices = [...] }`.
#[derive(Debug, Clone, PartialEq)]
pub struct FxChoice {
    pub value: i32,
    pub label: String,
}

/// Runtime value compatible with manifest defaults and ranges.
#[derive(Debug, Clone, PartialEq)]
pub enum FxValue {
    F32(f32),
    I32(i32),
    Bool(bool),
    Vec3([f32; 3]),
}

/// Fully validated definition for one named input.
#[derive(Debug, Clone, PartialEq)]
pub struct FxInputDef {
    pub input_type: FxInputType,
    pub label: Option<String>,
    pub default: Option<FxValue>,
    pub min: Option<FxValue>,
    pub max: Option<FxValue>,
    pub presentation: Option<FxPresentation>,
    pub choices: Option<Vec<FxChoice>>,
    pub unit: Option<String>,
    pub role: Option<String>,
}
