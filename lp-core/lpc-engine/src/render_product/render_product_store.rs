//! Engine-managed storage and batch sampling for render products.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;

use core::any::Any;

use lpc_model::FrameId;

use super::{RenderProductId, RenderSampleBatch, RenderSampleBatchResult, TextureRenderProduct};

pub type NativeTexturePayload<'a> = (u32, u32, &'a [u8], lps_shared::TextureStorageFormat);

/// Failure when sampling a render product through [`RenderProductStore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderProductError {
    UnknownProduct { id: RenderProductId },
    SampleCountMismatch,
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

/// Sampleable render product; heavy or GPU-backed implementations live behind this boundary.
pub trait RenderProduct {
    fn sample_batch(
        &self,
        request: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, RenderProductError>;

    /// For downcasting to concrete products (texture materialization, diagnostics).
    fn as_any(&self) -> &dyn Any;
}

/// Maps [`RenderProductId`] to product implementations for [`crate::engine::Engine`].
///
/// [`insert`](RenderProductStore::insert) allocates ids monotonically; ids are not reused for
/// the lifetime of this store.
pub struct RenderProductStore {
    next_id: u32,
    products: BTreeMap<RenderProductId, Box<dyn RenderProduct>>,
    /// Last engine frame where this id's backing product contents were replaced.
    changed_at: BTreeMap<RenderProductId, FrameId>,
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
    pub fn insert(&mut self, product: Box<dyn RenderProduct>) -> RenderProductId {
        let id = RenderProductId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.products.insert(id, product);
        self.changed_at.insert(id, FrameId::default());
        id
    }

    pub fn get(&self, id: RenderProductId) -> Option<&dyn RenderProduct> {
        self.products.get(&id).map(|b| b.as_ref())
    }

    /// Iterate active ids ordered by allocation (matches internal map order).
    pub fn ids(&self) -> impl Iterator<Item = RenderProductId> + '_ {
        self.products.keys().copied()
    }

    pub fn changed_frame(&self, id: RenderProductId) -> FrameId {
        self.changed_at
            .get(&id)
            .copied()
            .unwrap_or(FrameId::default())
    }

    /// Replace an existing product id, e.g. after re-rendering into a texture-backed product.
    pub fn replace(
        &mut self,
        id: RenderProductId,
        product: Box<dyn RenderProduct>,
        changed_frame: FrameId,
    ) -> Result<(), RenderProductError> {
        if !self.products.contains_key(&id) {
            return Err(RenderProductError::unknown_product(id));
        }
        self.products.insert(id, product);
        self.changed_at.insert(id, changed_frame);
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
impl RenderProduct for SolidColorProduct {
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

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// Test double that returns `[x, y, 0.0, 1.0]` from each [`RenderSamplePoint`].
#[cfg(test)]
pub struct CoordinateProduct;

#[cfg(test)]
impl RenderProduct for CoordinateProduct {
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

    use lpc_model::FrameId;

    use super::{
        CoordinateProduct, RenderProduct, RenderProductError, RenderProductStore,
        RenderSampleBatchResult, SolidColorProduct,
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
                FrameId::new(1),
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

        impl RenderProduct for BadProduct {
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
