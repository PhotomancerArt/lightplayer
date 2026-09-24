//! How precisely Studio asks for the pixels it previews.
//!
//! Every live preview a Studio surface pulls over a link — the lens's
//! control-product previews and published output frames, and the device
//! card's frame — asks for [`PREVIEW_SAMPLE_FORMAT`]: 8 bits per sample,
//! half the bytes of the 16 the engine renders at. The screen draws 8-bit
//! anyway (Yona, lean-wire D1, 2026-09-23).
//!
//! The 8 bits are sRGB-ENCODED (`Srgb8`, lean-wire follow-ups A2), not
//! linear: the engine sends the correctly rounded display code
//! (`lpc_wire::linear16_to_srgb8`), which is exactly the code the lamp view
//! would have drawn from the full 16-bit sample. Linear 8-bit spent 13 of its
//! 256 levels below the first on-screen step above black, and a dim picture
//! (the PLAYFUL choker at brightness 0.2) fell from 125 distinct on-screen
//! levels to 52. Studio decodes the codes back to linear unorm16
//! (`UiControlProductPreview::unorm16_sample`) and the lamp view re-encodes
//! them to the same codes.
//!
//! [`CLOSE_INSPECTION_SAMPLE_FORMAT`] keeps full precision one constant away
//! for a deliberate close-inspection surface. No surface asks for it yet.

use lpc_wire::WireChannelSampleFormat;

/// The format every live preview asks for.
pub const PREVIEW_SAMPLE_FORMAT: WireChannelSampleFormat = WireChannelSampleFormat::Srgb8;

/// Full precision, for a surface that inspects samples rather than shows
/// them. Unused until such a surface exists.
pub const CLOSE_INSPECTION_SAMPLE_FORMAT: WireChannelSampleFormat = WireChannelSampleFormat::U16;
