//! Node-owned runtime outputs (non-scalar [`crate::runtime_product::RuntimeProduct`]).
//!
//! Use this for handles such as [`crate::runtime_product::RuntimeProduct::Render`]. Scalar
//! bridge data remains on [`super::RuntimePropAccess`] as a temporary path.

use lpc_model::FrameId;
use lpc_model::prop::PropPath;

use crate::runtime_product::RuntimeProduct;

/// Read node-produced outputs that are not representable as [`lps_shared::LpsValueF32`].
pub trait RuntimeOutputAccess {
    fn get(&self, path: &PropPath) -> Option<(RuntimeProduct, FrameId)>;
}

/// No outputs beyond the defaults.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyRuntimeOutputs;

impl RuntimeOutputAccess for EmptyRuntimeOutputs {
    fn get(&self, _path: &PropPath) -> Option<(RuntimeProduct, FrameId)> {
        None
    }
}

/// Shared reference for [`crate::node::Node::outputs`] defaults.
pub const EMPTY_RUNTIME_OUTPUTS: EmptyRuntimeOutputs = EmptyRuntimeOutputs;

/// Reserved for opaque runtime state snapshots (sync/debug tooling). No fields in M4.
pub trait RuntimeStateAccess {}

/// No runtime state surface.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyRuntimeState;

impl RuntimeStateAccess for EmptyRuntimeState {}

pub const EMPTY_RUNTIME_STATE: EmptyRuntimeState = EmptyRuntimeState;
