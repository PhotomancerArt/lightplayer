//! [`LatentReadBackSource`]: whole-texture readback that may answer late.

use alloc::vec::Vec;

use crate::gfx_error::GfxError;
use crate::latent_read_back::LatentReadBack;
use crate::texture_handle::TextureHandle;

/// Reads a texture back without blocking, accepting that the bytes may
/// belong to an earlier call's texture.
///
/// This is deliberately **not** a method on [`crate::LpGraphics`]. Only a
/// backend whose render products stay GPU-resident needs it — the browser
/// GPU tier, which cannot block on a buffer map — and every other backend
/// already serves bytes synchronously. So the host that has such a backend
/// hands it to the engine separately (`LpServer::set_latent_read_back`,
/// behind `lpa-server`'s `latent-read-back` feature), and device images
/// link none of it: their DRAM is byte-identical with or without this
/// trait, which matters because the Xtensa images' main stack is whatever
/// DRAM is left, and the classic's is pinned against a silicon capture.
///
/// The one reader today is the wire probe's texture previews; see
/// `docs/adr/2026-08-05-browser-sample-readback-is-async.md`.
pub trait LatentReadBackSource: Send + Sync {
    /// Read `texture` back into `out`, replacing its contents with tightly
    /// packed logical texels (as [`crate::LpGraphics::read_back`] returns
    /// them). The source sizes `out` itself, so a caller linked into a
    /// device image computes no texel sizes on a path it never takes.
    ///
    /// `tag` names this call's frame (the caller's revision, say); the
    /// answer is the tag of the frame actually written into `out`, or
    /// `None` while no frame has landed yet (`out` untouched). `state` is
    /// caller-owned and must be the same value on every call from one read
    /// site; a texture of a different shape starts that site over.
    fn read_back_latent(
        &self,
        texture: &TextureHandle,
        state: &mut LatentReadBack,
        tag: u64,
        out: &mut Vec<u8>,
    ) -> Result<Option<u64>, GfxError>;
}
