//! Visual-product handle and minimal sample request/result shapes.

mod render_texture_request;
mod sample_request;
mod sample_result;
mod texture_product;

pub use lpc_model::VisualProduct;
pub use render_texture_request::RenderTextureRequest;
pub use sample_request::{
    VisualSampleBatch, VisualSampleBufferRequest, VisualSamplePoint, VisualSampleTarget,
};
pub use sample_result::{VisualSample, VisualSampleBatchResult};
pub use texture_product::{TextureRenderProduct, TextureRenderProductError};
#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::{VisualSample, VisualSampleBatch, VisualSampleBatchResult, VisualSamplePoint};

    #[test]
    fn sample_batch_holds_points_and_results_hold_samples() {
        let batch = VisualSampleBatch {
            points: vec![
                VisualSamplePoint { x_q16: 0, y_q16: 0 },
                VisualSamplePoint {
                    x_q16: 65536,
                    y_q16: 65536,
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
