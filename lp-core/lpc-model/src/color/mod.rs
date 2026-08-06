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
//! # One representation everywhere
//!
//! Every surface — [`crate::LpValue`] storage, wire, serde JSON, and the
//! def codec — carries the SAME shape (`docs/design/color.md` §5; ADR
//! 2026-08-05-gradient-stops-string-storage): snake-case token strings for
//! `space`/`method`, and the stop list as one compact
//! [stops literal](stops_string) (`"#000 #f80@.5 (0.211,-0.017,-0.039)"`).
//! Metadata stays structural; the part that scales with content is one
//! string. This is what keeps a whole `GradientConfig` a few hundred wire
//! bytes instead of the ~17.7 KiB the original padded-array recipe cost —
//! larger than an entire project-read frame.
//!
//! The `LpValue` conversion is written by hand for the same reason
//! [`crate::PhasorConfig`]'s is (`#[derive(SlotValue)]` infers `LpType`s
//! only for a fixed scalar whitelist), and serde is hand-written because
//! printing the literal is space-dependent (hex is confined to the
//! sRGB-shaped spaces).
//!
//! See `docs/design/color.md` §4 (authoring spaces), §5 (storage recipe +
//! literal grammar), §6 (interpolation), and §7 (where conversion happens).

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
