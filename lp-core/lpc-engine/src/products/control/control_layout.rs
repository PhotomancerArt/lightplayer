//! Engine-facing aliases for core control sample layout metadata.
//!
//! The durable vocabulary lives in `lpc-model`. These aliases preserve the
//! existing engine names while the runtime moves onto the shared model types.

pub use lpc_model::{
    ControlSampleEncoding as ControlHint, ControlSampleLayout as ControlLayout,
    ControlSampleSpan as ControlSpan,
};
