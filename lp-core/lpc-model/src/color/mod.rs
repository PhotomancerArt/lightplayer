//! Color-family value shapes: the palette model.
//!
//! A palette in LightPlayer is a **value**, not a node: a [`Gradient`] is
//! stops plus an authoring space plus an interpolation method, and a
//! [`GradientConfig`] is how one consumer wants that palette read over time
//! (a single gradient, or a timed cycle through a set). Consumers bake the
//! resolved gradient to a height-one texture; nothing here knows about
//! textures.
//!
//! [`GradientConfig`] is the second *declared-config* kind after
//! [`crate::PhasorConfig`]: config, never state. The set and the timings are
//! authored; the cycle position is a pure function of a phasor read at fill
//! time, so re-authoring `step_seconds` changes the rate from that instant
//! without resetting anything.
//!
//! # Two surfaces, deliberately different
//!
//! Every type here has two encodings and they do **not** match:
//!
//! - **serde** (authored JSON, wire) — human-friendly: enums are snake-case
//!   strings, `stops` is a variable-length array of exactly what was
//!   authored.
//! - **[`crate::LpValue`] / [`crate::LpType`]** — the ratified fixed-shape
//!   storage recipe from `docs/design/color.md` §5: enums are `I32` tags,
//!   collections are max-sized arrays plus an explicit `count`, padding is
//!   never read. Shaders and the GPU layout logic depend on that fixed
//!   shape; authors should never have to type it.
//!
//! The conversion between the two is the whole of [`gradient`] and
//! [`gradient_config`]; it is written by hand for the same reason
//! [`crate::PhasorConfig`]'s is (`#[derive(SlotValue)]` infers `LpType`s only
//! for a fixed scalar whitelist, and neither a string-leaf enum member nor a
//! fixed array of structs is on it).
//!
//! See `docs/design/color.md` §4 (authoring spaces), §5 (storage recipes),
//! §6 (interpolation), and §7 (where conversion happens).

pub mod gradient;
pub mod gradient_config;
pub mod stops_string;

pub use gradient::{
    COLORSPACE_SHAPE_NAME, Colorspace, GRADIENT_SHAPE_NAME, Gradient, GradientError, GradientStop,
    INTERP_METHOD_SHAPE_NAME, InterpMethod, MAX_GRADIENT_STOPS, MIN_GRADIENT_STOPS,
    gradient_lp_type,
};
pub use gradient_config::{
    GRADIENT_CONFIG_SHAPE_NAME, GradientConfig, MAX_CYCLE_SET, MIN_CYCLE_SET,
    gradient_config_lp_type,
};
pub use stops_string::{StopsParseError, parse_stops, print_stops};
