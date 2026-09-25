//! A tiny QR Code encoder (plan D15): byte mode, error-correction level M,
//! versions 1–10, the mask chosen by the standard's penalty rules — and an
//! inline-SVG renderer.
//!
//! Written from the QR Code specification (ISO/IEC 18004, Model 2) and its
//! public descriptions; no library's code was followed. The `qrcodegen`
//! crate is a dev-dependency only: the tests compare this encoder's module
//! matrices against it, mask by mask, so the oracle proves the output
//! without shipping a byte of it.
//!
//! Studio needs one thing from a QR: a `https://…/unlock#…` link a phone
//! camera opens (the Share sheet). Byte mode at level M up to version 10
//! carries 213 bytes, which covers any such link with room to spare.
//!
//! | concept | file |
//! |---|---|
//! | the symbol: encode, place, mask | [`qr_code`] |
//! | the level-M block table, per version | [`qr_version`] |
//! | the eight masks and the penalty score | [`qr_mask`] |
//! | error-correction codewords | [`reed_solomon`] |
//! | drawing it | [`qr_code_svg`] |

pub mod qr_code;
pub mod qr_code_svg;
pub mod qr_mask;
pub mod qr_version;
pub mod reed_solomon;

pub use qr_code::QrCode;
pub use qr_code_svg::QrCodeSvg;
