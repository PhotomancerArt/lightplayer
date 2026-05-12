//! Common engine product vocabulary.
//!
//! Products are lazy graph values carried through slots. Concrete product
//! materialization contracts live under [`crate::products`].

pub use crate::products::control::{
    ControlHint, ControlLayout, ControlRenderRequest, ControlRenderTarget, ControlSampleFormat,
    ControlSpan,
};
pub use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, TextureRenderProductError, VisualSample,
    VisualSampleBatch, VisualSampleBatchResult, VisualSamplePoint,
};
pub use lpc_model::{ControlExtent, ControlProduct, VisualProduct};
