//! Engine-managed storage and batch sampling for render products.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;

use core::any::Any;

use lpc_model::Revision;

use crate::gfx::LpGraphics;

use super::{
    RenderProductId, RenderSampleBatch, RenderSampleBatchResult, RenderTextureRequest,
    TextureRenderProduct,
};

pub type NativeTexturePayload<'a> = (u32, u32, &'a [u8], lps_shared::TextureStorageFormat);

/// Failure when sampling a render product through [`RenderProductStore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderProductError {
    UnknownProduct { id: RenderProductId },
    SampleCountMismatch,
    NotRenderable,
    RenderFailed { message: alloc::string::String },
}

/// Full/native RGBA payload materialization failed for wire sync (M4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderProductMaterializeError {
    UnknownProduct { id: RenderProductId },
    NotCpuTextureProduct,
    UnsupportedTextureFormatForWire,
}

impl RenderProductError {
    pub fn unknown_product(id: RenderProductId) -> Self {
        Self::UnknownProduct { id }
    }
}

/// Store-backed render product used by legacy resource projection and tests.
///
/// Runtime dataflow uses [`super::RenderProduct`] as a graph handle and dispatches
/// render requests back to the owning node. This trait remains for concrete,
/// already-materialized products that live in a store.
pub trait StoredRenderProduct {
    fn sample_batch(
        &self,
        request: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, RenderProductError>;

    /// Render this product into a complete texture when it supports full-frame materialization.
    fn render_texture(
        &mut self,
        request: &RenderTextureRequest,
        graphics: Option<&dyn LpGraphics>,
    ) -> Result<TextureRenderProduct, RenderProductError> {
        let _ = (request, graphics);
        Err(RenderProductError::NotRenderable)
    }

    /// For downcasting to concrete products (texture materialization, diagnostics).
    fn as_any(&self) -> &dyn Any;
}

/// Maps [`RenderProductId`] to product implementations for [`crate::engine::Engine`].
///
/// [`insert`](RenderProductStore::insert) allocates ids monotonically; ids are not reused for
/// the lifetime of this store.
pub struct RenderProductStore {
    next_id: u32,
    products: BTreeMap<RenderProductId, Box<dyn StoredRenderProduct>>,
    /// Last engine frame where this id's backing product contents were replaced.
    changed_at: BTreeMap<RenderProductId, Revision>,
}

impl RenderProductStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: 0,
            products: BTreeMap::new(),
            changed_at: BTreeMap::new(),
        }
    }

    /// Allocates a new id. Ids increase monotonically and are never reused after allocation.
    pub fn insert(&mut self, product: Box<dyn StoredRenderProduct>) -> RenderProductId {
        let id = RenderProductId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.products.insert(id, product);
        self.changed_at.insert(id, Revision::default());
        id
    }

    pub fn get(&self, id: RenderProductId) -> Option<&dyn StoredRenderProduct> {
        self.products.get(&id).map(|b| b.as_ref())
    }

    /// Iterate active ids ordered by allocation (matches internal map order).
    pub fn ids(&self) -> impl Iterator<Item = RenderProductId> + '_ {
        self.products.keys().copied()
    }

    pub fn revision(&self, id: RenderProductId) -> Revision {
        self.changed_at
            .get(&id)
            .copied()
            .unwrap_or(Revision::default())
    }

    /// Replace an existing product id, e.g. after re-rendering into a texture-backed product.
    pub fn replace(
        &mut self,
        id: RenderProductId,
        product: Box<dyn StoredRenderProduct>,
        revision: Revision,
    ) -> Result<(), RenderProductError> {
        if !self.products.contains_key(&id) {
            return Err(RenderProductError::unknown_product(id));
        }
        self.products.insert(id, product);
        self.changed_at.insert(id, revision);
        Ok(())
    }

    pub fn sample_batch(
        &self,
        id: RenderProductId,
        request: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, RenderProductError> {
        let product = self
            .products
            .get(&id)
            .ok_or_else(|| RenderProductError::unknown_product(id))?;
        let result = product.sample_batch(request)?;
        if result.samples.len() != request.points.len() {
            return Err(RenderProductError::SampleCountMismatch);
        }
        Ok(result)
    }

    pub fn render_texture(
        &mut self,
        id: RenderProductId,
        request: &RenderTextureRequest,
        graphics: Option<&dyn LpGraphics>,
    ) -> Result<TextureRenderProduct, RenderProductError> {
        self.products
            .get_mut(&id)
            .ok_or_else(|| RenderProductError::unknown_product(id))?
            .render_texture(request, graphics)
    }

    /// Full/native RGBA16 wire payload when the backing product is [`TextureRenderProduct`] in RGBA16.
    pub fn try_materialize_native_texture_payload(
        &self,
        id: RenderProductId,
    ) -> Result<NativeTexturePayload<'_>, RenderProductMaterializeError> {
        let product = self
            .products
            .get(&id)
            .ok_or(RenderProductMaterializeError::UnknownProduct { id })?;
        let tex = product
            .as_any()
            .downcast_ref::<TextureRenderProduct>()
            .ok_or(RenderProductMaterializeError::NotCpuTextureProduct)?;
        use lps_shared::TextureStorageFormat;
        let fmt = tex.storage_format();
        if fmt != TextureStorageFormat::Rgba16Unorm {
            return Err(RenderProductMaterializeError::UnsupportedTextureFormatForWire);
        }
        let bytes = tex
            .try_raw_bytes()
            .ok_or(RenderProductMaterializeError::UnsupportedTextureFormatForWire)?;
        Ok((tex.width(), tex.height(), bytes, fmt))
    }
}

impl Default for RenderProductStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
use super::RenderSample;

/// Test double that returns the same RGBA color for every sample point.
#[cfg(test)]
pub struct SolidColorProduct {
    pub color: [f32; 4],
}

#[cfg(test)]
impl StoredRenderProduct for SolidColorProduct {
    fn sample_batch(
        &self,
        request: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, RenderProductError> {
        let samples = request
            .points
            .iter()
            .map(|_| RenderSample { color: self.color })
            .collect();
        Ok(RenderSampleBatchResult { samples })
    }

    fn render_texture(
        &mut self,
        request: &super::RenderTextureRequest,
        _graphics: Option<&dyn crate::gfx::LpGraphics>,
    ) -> Result<TextureRenderProduct, RenderProductError> {
        let mut pixels = alloc::vec::Vec::new();
        let px_count = usize::try_from(request.width)
            .ok()
            .and_then(|w| {
                usize::try_from(request.height)
                    .ok()
                    .map(|h| w.saturating_mul(h))
            })
            .ok_or_else(|| RenderProductError::RenderFailed {
                message: alloc::format!(
                    "texture dimensions {}x{} overflow usize",
                    request.width,
                    request.height
                ),
            })?;
        pixels.reserve(px_count.saturating_mul(request.format.bytes_per_pixel()));
        let rgba = [
            f32_to_unorm16(self.color[0]),
            f32_to_unorm16(self.color[1]),
            f32_to_unorm16(self.color[2]),
            f32_to_unorm16(self.color[3]),
        ];
        for _ in 0..px_count {
            match request.format {
                lps_shared::TextureStorageFormat::Rgba16Unorm => {
                    for channel in rgba {
                        pixels.extend_from_slice(&channel.to_le_bytes());
                    }
                }
                lps_shared::TextureStorageFormat::Rgb16Unorm => {
                    for channel in &rgba[0..3] {
                        pixels.extend_from_slice(&channel.to_le_bytes());
                    }
                }
                lps_shared::TextureStorageFormat::R16Unorm => {
                    pixels.extend_from_slice(&rgba[0].to_le_bytes());
                }
            }
        }
        TextureRenderProduct::new(request.width, request.height, request.format, pixels).map_err(
            |e| RenderProductError::RenderFailed {
                message: alloc::format!("solid color texture product: {e}"),
            },
        )
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[cfg(test)]
fn f32_to_unorm16(value: f32) -> u16 {
    libm::floorf(value.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
}

/// Test double that returns `[x, y, 0.0, 1.0]` from each [`RenderSamplePoint`].
#[cfg(test)]
pub struct CoordinateProduct;

#[cfg(test)]
impl StoredRenderProduct for CoordinateProduct {
    fn sample_batch(
        &self,
        request: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, RenderProductError> {
        let samples = request
            .points
            .iter()
            .map(|p| RenderSample {
                color: [p.x, p.y, 0.0, 1.0],
            })
            .collect();
        Ok(RenderSampleBatchResult { samples })
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::vec;

    use lpc_model::Revision;

    use super::{
        CoordinateProduct, RenderProductError, RenderProductStore, RenderSampleBatchResult,
        SolidColorProduct, StoredRenderProduct,
    };
    use crate::render_product::{RenderSampleBatch, RenderSamplePoint};

    #[test]
    fn store_samples_registered_solid_product() {
        let mut store = RenderProductStore::new();
        let id = store.insert(Box::new(SolidColorProduct {
            color: [0.25, 0.5, 0.75, 1.0],
        }));
        let request = RenderSampleBatch {
            points: vec![
                RenderSamplePoint { x: 0.0, y: 0.0 },
                RenderSamplePoint { x: 1.0, y: 1.0 },
            ],
        };
        let result = store.sample_batch(id, &request).expect("sample");
        assert_eq!(result.samples.len(), 2);
        assert_eq!(result.samples[0].color, [0.25, 0.5, 0.75, 1.0]);
        assert_eq!(result.samples[1].color, [0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn store_samples_coordinate_product() {
        let mut store = RenderProductStore::new();
        let id = store.insert(Box::new(CoordinateProduct));
        let request = RenderSampleBatch {
            points: vec![RenderSamplePoint { x: 0.1, y: 0.2 }],
        };
        let result = store.sample_batch(id, &request).expect("sample");
        assert_eq!(result.samples[0].color, [0.1, 0.2, 0.0, 1.0]);
    }

    #[test]
    fn store_errors_for_unknown_product() {
        let store = RenderProductStore::new();
        let request = RenderSampleBatch {
            points: vec![RenderSamplePoint { x: 0.0, y: 0.0 }],
        };
        let missing = super::RenderProductId::new(99);
        let err = store
            .sample_batch(missing, &request)
            .expect_err("unknown id");
        assert_eq!(err, RenderProductError::UnknownProduct { id: missing });
    }

    #[test]
    fn store_replace_keeps_id_and_updates_sampling() {
        let mut store = RenderProductStore::new();
        let id = store.insert(Box::new(SolidColorProduct {
            color: [0.0, 0.0, 0.0, 1.0],
        }));
        store
            .replace(
                id,
                Box::new(SolidColorProduct {
                    color: [1.0, 0.0, 0.0, 1.0],
                }),
                Revision::new(1),
            )
            .expect("replace");
        let request = RenderSampleBatch {
            points: vec![RenderSamplePoint { x: 0.0, y: 0.0 }],
        };
        let result = store.sample_batch(id, &request).expect("sample");
        assert_eq!(result.samples[0].color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn store_errors_on_sample_count_mismatch() {
        struct BadProduct;

        impl StoredRenderProduct for BadProduct {
            fn sample_batch(
                &self,
                _request: &RenderSampleBatch,
            ) -> Result<RenderSampleBatchResult, RenderProductError> {
                Ok(super::RenderSampleBatchResult {
                    samples: vec![super::RenderSample { color: [0.0; 4] }],
                })
            }

            fn as_any(&self) -> &dyn core::any::Any {
                self
            }
        }

        let mut store = RenderProductStore::new();
        let id = store.insert(Box::new(BadProduct));
        let request = RenderSampleBatch {
            points: vec![
                RenderSamplePoint { x: 0.0, y: 0.0 },
                RenderSamplePoint { x: 1.0, y: 1.0 },
            ],
        };
        let err = store.sample_batch(id, &request).expect_err("mismatch");
        assert_eq!(err, RenderProductError::SampleCountMismatch);
    }
}
