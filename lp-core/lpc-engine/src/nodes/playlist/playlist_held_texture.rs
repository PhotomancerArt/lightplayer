//! The frame a playlist holds across a switch, on the texture path.
//!
//! A fixture authored with `"sampling": "texture_area"` (the default when a
//! fixture names no sampling, and the declared-strip idiom) renders the
//! playlist into a texture target and area-samples it, so the texture path
//! can drive lamps too (plan PD5). It holds exactly like the sample path
//! ([`super::playlist_held_frame`]): the switch frame's output is copied
//! into a held texture at the request's size, shown until the new entry
//! renders for real, then faded from.
//!
//! Two render targets live for one switch — the held frame and the fade's
//! incoming frame — created on the capture frame and the fade's first frame
//! and dropped when the switch ends, never per call. A render of a different
//! size than the held one (a canvas preview beside a fixture) is not held:
//! it cuts to the live product.
//!
//! The copy is `blend_textures(src, src, 0.0, dst)`: at alpha 0 the blend is
//! exactly the first operand on every backend's unorm16 grid, and it keeps
//! the copy a GPU-resident op (no read-back).

use lp_gfx::{LpGraphics, TextureHandle};
use lpc_model::Revision;

use crate::node::{NodeError, err_ctx};

/// The held frame and the fade scratch of the texture path, alive for one
/// switch.
#[derive(Default)]
pub(super) struct PlaylistHeldTexture {
    held: Option<TextureHandle>,
    fade: Option<TextureHandle>,
    /// The frame a mid-fade switch re-captured the blend on: once per frame.
    recaptured_at: Option<Revision>,
}

impl PlaylistHeldTexture {
    /// Whether a frame is held.
    pub(super) fn is_held(&self) -> bool {
        self.held.is_some()
    }

    /// The held frame, when it has `target`'s size.
    pub(super) fn held_for(&self, target: &TextureHandle) -> Option<&TextureHandle> {
        self.held.as_ref().filter(|held| same_shape(held, target))
    }

    /// Keep a copy of `shown` (the switch frame's output) as the held frame.
    /// Once per switch: a second render of the capture frame keeps the first
    /// copy.
    pub(super) fn capture(
        &mut self,
        graphics: &dyn LpGraphics,
        shown: &TextureHandle,
    ) -> Result<(), NodeError> {
        if self.held.is_some() {
            return Ok(());
        }
        let mut held = graphics
            .create_render_target(shown.width(), shown.height())
            .map_err(err_ctx("playlist held texture"))?;
        copy_texture(graphics, shown, &mut held)?;
        self.held = Some(held);
        Ok(())
    }

    /// A switch decided mid-fade: the blend shown this frame becomes the
    /// held frame (once per frame).
    pub(super) fn recapture(
        &mut self,
        graphics: &dyn LpGraphics,
        shown: &TextureHandle,
        revision: Revision,
    ) -> Result<(), NodeError> {
        if self.recaptured_at == Some(revision) {
            return Ok(());
        }
        let Some(held) = self.held.as_mut().filter(|held| same_shape(held, shown)) else {
            return Ok(());
        };
        copy_texture(graphics, shown, held)?;
        self.recaptured_at = Some(revision);
        Ok(())
    }

    /// Show the held frame in `target`.
    pub(super) fn show(
        &self,
        graphics: &dyn LpGraphics,
        target: &mut TextureHandle,
    ) -> Result<bool, NodeError> {
        let Some(held) = self.held_for(target) else {
            return Ok(false);
        };
        copy_texture(graphics, held, target)?;
        Ok(true)
    }

    /// The fade's render target for the incoming entry, at `target`'s size:
    /// created on the fade's first frame, reused after.
    pub(super) fn fade_target(
        &mut self,
        graphics: &dyn LpGraphics,
        target: &TextureHandle,
    ) -> Result<&mut TextureHandle, NodeError> {
        let stale = self
            .fade
            .as_ref()
            .is_none_or(|fade| !same_shape(fade, target));
        if stale {
            self.fade = None;
            self.fade = Some(
                graphics
                    .create_render_target(target.width(), target.height())
                    .map_err(err_ctx("playlist fade texture"))?,
            );
        }
        Ok(self.fade.as_mut().expect("fade target created above"))
    }

    /// The held frame and the fade target, for the blend.
    pub(super) fn held_and_fade(&self) -> Option<(&TextureHandle, &TextureHandle)> {
        Some((self.held.as_ref()?, self.fade.as_ref()?))
    }

    /// The switch is over: drop both targets.
    pub(super) fn release(&mut self) {
        *self = Self::default();
    }

    /// Whether any render target is alive.
    #[cfg(test)]
    pub(super) fn holds_memory(&self) -> bool {
        self.held.is_some() || self.fade.is_some()
    }
}

fn same_shape(a: &TextureHandle, b: &TextureHandle) -> bool {
    a.width() == b.width() && a.height() == b.height() && a.format() == b.format()
}

fn copy_texture(
    graphics: &dyn LpGraphics,
    source: &TextureHandle,
    target: &mut TextureHandle,
) -> Result<(), NodeError> {
    graphics
        .blend_textures(source, source, 0.0, target)
        .map_err(err_ctx("playlist texture copy"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_captured_texture_is_shown_exactly_and_released_with_the_switch() {
        let graphics = lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl);
        let mut shown = graphics.create_render_target(2, 1).expect("target");
        let texels: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        graphics.write_texture(&mut shown, &texels).expect("write");
        let mut held = PlaylistHeldTexture::default();

        held.capture(&graphics, &shown).expect("capture");
        graphics.clear_texture(&mut shown).expect("clear");
        assert!(held.show(&graphics, &mut shown).expect("show"));

        assert_eq!(
            graphics.read_back(&shown).expect("read").into_bytes(),
            texels
        );
        held.release();
        assert!(!held.holds_memory());
    }

    /// A second render on the capture frame (another consumer of the same
    /// size) keeps the first copy: one held target per switch.
    #[test]
    fn capture_is_once_per_switch() {
        let graphics = lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl);
        let mut first = graphics.create_render_target(1, 1).expect("target");
        graphics
            .write_texture(&mut first, &[1, 0, 2, 0, 3, 0, 4, 0])
            .expect("write");
        let mut second = graphics.create_render_target(1, 1).expect("target");
        graphics
            .write_texture(&mut second, &[9, 0, 9, 0, 9, 0, 9, 0])
            .expect("write");
        let mut held = PlaylistHeldTexture::default();

        held.capture(&graphics, &first).expect("capture");
        held.capture(&graphics, &second)
            .expect("second capture is a no-op");
        assert!(held.show(&graphics, &mut second).expect("show"));

        assert_eq!(
            graphics.read_back(&second).expect("read").into_bytes(),
            [1, 0, 2, 0, 3, 0, 4, 0]
        );
    }

    #[test]
    fn a_target_of_another_size_is_not_held() {
        let graphics = lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl);
        let shown = graphics.create_render_target(2, 1).expect("target");
        let mut preview = graphics.create_render_target(4, 4).expect("preview");
        let mut held = PlaylistHeldTexture::default();
        held.capture(&graphics, &shown).expect("capture");

        assert!(!held.show(&graphics, &mut preview).expect("show"));
    }
}
