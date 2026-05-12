//! Compiled serial compute shader.

use alloc::boxed::Box;
use alloc::format;
use core::cell::RefCell;

use lps_shared::{LpsModuleSig, LpsValueF32};
use lpvm::{LpvmInstance, LpvmModule};

use crate::compute_abi::COMPUTE_TICK_FN;
use crate::error::LpsError;

/// Backend-erased operations on a serial compute shader instance.
pub(crate) trait ComputeShaderBackend {
    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), LpsError>;
    fn get_global(&mut self, path: &str) -> Result<LpsValueF32, LpsError>;
    fn call_tick(&mut self) -> Result<(), LpsError>;
}

struct BackendAdapter<M: LpvmModule> {
    _module: M,
    instance: M::Instance,
}

impl<M: LpvmModule + 'static> ComputeShaderBackend for BackendAdapter<M> {
    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), LpsError> {
        self.instance
            .set_uniform(path, value)
            .map_err(|e| LpsError::Render(format!("set compute input `{path}`: {e}")))
    }

    fn get_global(&mut self, path: &str) -> Result<LpsValueF32, LpsError> {
        self.instance
            .get_global(path)
            .map_err(|e| LpsError::Render(format!("get compute output `{path}`: {e}")))
    }

    fn call_tick(&mut self) -> Result<(), LpsError> {
        self.instance
            .call_compute_tick(COMPUTE_TICK_FN)
            .map_err(|e| LpsError::Render(format!("call compute `{COMPUTE_TICK_FN}`: {e}")))
    }
}

/// A compiled serial compute shader with persistent per-instance globals.
pub struct LpsComputeShader {
    inner: RefCell<Box<dyn ComputeShaderBackend>>,
    meta: LpsModuleSig,
    tick_fn_index: usize,
}

impl LpsComputeShader {
    pub(crate) fn new<M: LpvmModule + 'static>(
        module: M,
        meta: LpsModuleSig,
        tick_fn_index: usize,
    ) -> Result<Self, LpsError> {
        let instance = module
            .instantiate()
            .map_err(|e| LpsError::Compile(format!("instantiate: {e}")))?;
        let inner: Box<dyn ComputeShaderBackend> = Box::new(BackendAdapter {
            _module: module,
            instance,
        });
        Ok(Self {
            inner: RefCell::new(inner),
            meta,
            tick_fn_index,
        })
    }

    /// Module metadata (function signatures, uniform/global layouts).
    #[must_use]
    pub fn meta(&self) -> &LpsModuleSig {
        &self.meta
    }

    /// Index of `tick` in [`Self::meta`].
    #[must_use]
    pub fn tick_fn_index(&self) -> usize {
        self.tick_fn_index
    }

    /// Write consumed inputs and execute one serial compute tick.
    ///
    /// Globals are not reset before this call. Plain shader globals may act as
    /// persistent runtime state across ticks.
    pub fn tick(&self, inputs: &[(&str, LpsValueF32)]) -> Result<(), LpsError> {
        let mut inner = self.inner.borrow_mut();
        for (path, value) in inputs {
            inner.set_uniform(path, value)?;
        }
        inner.call_tick()
    }

    /// Read a produced private global by path.
    pub fn get_output(&self, path: &str) -> Result<LpsValueF32, LpsError> {
        self.inner.borrow_mut().get_global(path)
    }
}
