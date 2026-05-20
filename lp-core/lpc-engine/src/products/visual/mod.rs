//! Visual-product handle and minimal sample request/result shapes.

mod coordinates;
mod render_texture_request;
mod sample_request;
mod sample_result;
mod texture_product;

pub use coordinates::{
    normalized_f32_to_q16, normalized_q16_to_pixel_q16, pixel_q16_to_normalized_q16,
    texel_center_to_uv_q16, texture_uv_q16_to_texel,
};
pub use lpc_model::VisualProduct;
pub use render_texture_request::RenderTextureRequest;
pub use sample_request::{
    TextureSampleBatch, TextureUvSamplePoint, VisualSampleBufferRequest, VisualSampleTarget,
};
pub use sample_result::{VisualSample, VisualSampleBatchResult};
pub use texture_product::{TextureRenderProduct, TextureRenderProductError};
#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::{TextureSampleBatch, TextureUvSamplePoint, VisualSample, VisualSampleBatchResult};

    #[test]
    fn sample_batch_holds_points_and_results_hold_samples() {
        let batch = TextureSampleBatch {
            points: vec![
                TextureUvSamplePoint { u_q16: 0, v_q16: 0 },
                TextureUvSamplePoint {
                    u_q16: 65536,
                    v_q16: 65536,
                },
            ],
            time_seconds: 0.0,
        };
        assert_eq!(batch.points.len(), 2);

        let result = VisualSampleBatchResult {
            samples: vec![
                VisualSample {
                    rgba_unorm16: [u16::MAX, 0, 0, u16::MAX],
                },
                VisualSample {
                    rgba_unorm16: [0, u16::MAX, 0, u16::MAX],
                },
            ],
        };
        assert_eq!(result.samples.len(), 2);
        assert_eq!(result.samples[0].rgba_unorm16, [u16::MAX, 0, 0, u16::MAX]);
    }
}
