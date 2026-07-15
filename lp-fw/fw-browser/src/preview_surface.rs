//! Card presentation surface: an `OffscreenCanvas` transferred into the
//! worker and configured as a wgpu surface.
//!
//! The surface is configured with a **non-sRGB** color format because the
//! `lp-gfx-wgpu` present blit performs the sRGB encode itself (matching the
//! CPU tier's byte conversion — single-encode invariant, see
//! `lp_gfx_wgpu::surface_blit`).

use crate::gpu::WorkerGpu;

/// One configured card surface (GPU-tier presentation target).
pub(crate) struct PreviewSurface {
    surface: wgpu::Surface<'static>,
    width: u32,
    height: u32,
}

impl PreviewSurface {
    /// Wrap and configure a transferred `OffscreenCanvas`.
    pub(crate) fn attach(
        gpu: &WorkerGpu,
        canvas: web_sys::OffscreenCanvas,
    ) -> Result<Self, String> {
        let (width, height) = (canvas.width(), canvas.height());
        if width == 0 || height == 0 {
            return Err(format!(
                "preview surface canvas has zero dimension ({width}x{height})"
            ));
        }
        let surface = gpu
            .instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas))
            .map_err(|error| format!("create surface from OffscreenCanvas: {error}"))?;
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .ok_or_else(|| {
                format!(
                    "surface offers no non-sRGB color format (offered: {:?})",
                    capabilities.formats
                )
            })?;
        surface.configure(
            &gpu.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: Vec::new(),
            },
        );
        Ok(Self {
            surface,
            width,
            height,
        })
    }

    pub(crate) fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }
}
