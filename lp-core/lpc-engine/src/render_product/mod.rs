//! Render-product handle and minimal sample request/result shapes.

mod render_product_id;
mod render_product_store;
mod sample_request;
mod sample_result;
mod texture_product;

pub use render_product_id::RenderProductId;
pub use render_product_store::{RenderProduct, RenderProductError, RenderProductStore};
pub use sample_request::{RenderSampleBatch, RenderSamplePoint};
pub use sample_result::{RenderSample, RenderSampleBatchResult};
pub use texture_product::{TextureRenderProduct, TextureRenderProductError};

#[cfg(test)]
pub use render_product_store::{CoordinateProduct, SolidColorProduct};

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::{RenderSample, RenderSampleBatch, RenderSampleBatchResult, RenderSamplePoint};

    #[test]
    fn sample_batch_holds_points_and_results_hold_samples() {
        let batch = RenderSampleBatch {
            points: vec![
                RenderSamplePoint { x: 0.0, y: 0.0 },
                RenderSamplePoint { x: 1.0, y: 1.0 },
            ],
        };
        assert_eq!(batch.points.len(), 2);

        let result = RenderSampleBatchResult {
            samples: vec![
                RenderSample {
                    color: [1.0, 0.0, 0.0, 1.0],
                },
                RenderSample {
                    color: [0.0, 1.0, 0.0, 1.0],
                },
            ],
        };
        assert_eq!(result.samples.len(), 2);
        assert_eq!(result.samples[0].color, [1.0, 0.0, 0.0, 1.0]);
    }
}
