//! Engine-side property reflection (`LpsValueF32`) and explicit runtime outputs.

mod runtime_output_access;
mod runtime_prop_access;

pub use runtime_output_access::{
    EMPTY_RUNTIME_OUTPUTS, EMPTY_RUNTIME_STATE, EmptyRuntimeOutputs, EmptyRuntimeState,
    RuntimeOutputAccess, RuntimeStateAccess,
};
pub use runtime_prop_access::RuntimePropAccess;
