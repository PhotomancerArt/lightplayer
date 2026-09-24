//! How precisely Studio asks for the pixels it previews.
//!
//! Every live preview a Studio surface pulls over a link — the lens's
//! control-product previews and published output frames, and the device
//! card's frame — asks for [`PREVIEW_SAMPLE_FORMAT`]: 8 bits per sample,
//! half the bytes of the 16 the engine renders at. The screen draws 8-bit
//! anyway (Yona, lean-wire D1, 2026-09-23). The engine rounds to nearest
//! (`round(v / 257)`), and the samples stay LINEAR — Studio's lamp decode
//! widens them back (`UiControlProductPreview::unorm16_sample`) before the
//! sRGB transfer.
//!
//! [`CLOSE_INSPECTION_SAMPLE_FORMAT`] keeps full precision one constant away
//! for a deliberate close-inspection surface. No surface asks for it yet.

use lpc_wire::WireChannelSampleFormat;

/// The format every live preview asks for.
pub const PREVIEW_SAMPLE_FORMAT: WireChannelSampleFormat = WireChannelSampleFormat::U8;

/// Full precision, for a surface that inspects samples rather than shows
/// them. Unused until such a surface exists.
pub const CLOSE_INSPECTION_SAMPLE_FORMAT: WireChannelSampleFormat = WireChannelSampleFormat::U16;
