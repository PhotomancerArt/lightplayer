//! WS281x LED output contracts.
//!
//! WS281x drivers combine a GPIO output resource with a timing peripheral such
//! as RMT. The opened [`Ws281xOutput`](ws281x_driver::Ws281xOutput) accepts raw
//! 8-bit RGB bytes; color pipeline work happens in higher-level output
//! providers.

pub mod virtual_ws281x_driver;
pub mod ws281x_driver;
