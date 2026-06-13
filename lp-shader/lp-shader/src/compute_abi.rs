//! Serial compute shader ABI validation.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use lp_collection::VecMap;

use lps_shared::path_resolve::LpsTypePathExt;
use lps_shared::{LpsModuleSig, LpsType};

use crate::error::LpsError;

pub const COMPUTE_TICK_FN: &str = "tick";

/// Authored consumed and produced data interface for a serial compute shader.
///
/// Higher-level domain code builds this from shader slot definitions. The
/// compiled shader is then validated against the lowered module metadata before
/// execution, so missing uniforms, missing globals, and shape mismatches fail at
/// compile time instead of becoming silent runtime data corruption.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputeAbi {
    /// Uniform-backed values written before each compute tick.
    pub consumed: VecMap<String, LpsType>,
    /// Private-global-backed values read after a compute tick.
    pub produced: VecMap<String, ComputeOutputAbi>,
}

/// Expected shader-visible representation of a produced slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputeOutputAbi {
    /// A single value stored in a private global with the same name as the slot.
    Value(LpsType),
    /// A fixed private-global array interpreted as a map using a sentinel key.
    SentinelArray {
        element: LpsType,
        len: u32,
        key: String,
    },
}

/// Validate the serial compute entry point.
pub fn validate_compute_tick_sig(meta: &LpsModuleSig) -> Result<usize, LpsError> {
    let (index, sig) = meta
        .functions
        .iter()
        .enumerate()
        .find(|(_, sig)| sig.name == COMPUTE_TICK_FN)
        .ok_or_else(|| {
            LpsError::Validation(format!(
                "compute shader must define `void {COMPUTE_TICK_FN}()`"
            ))
        })?;

    if !sig.parameters.is_empty() {
        return Err(LpsError::Validation(format!(
            "compute `{COMPUTE_TICK_FN}` must take no parameters"
        )));
    }
    if sig.return_type != LpsType::Void {
        return Err(LpsError::Validation(format!(
            "compute `{COMPUTE_TICK_FN}` must return void"
        )));
    }
    Ok(index)
}

/// Validate the authored compute ABI against lowered module metadata.
pub fn validate_compute_abi(meta: &LpsModuleSig, abi: &ComputeAbi) -> Result<(), LpsError> {
    for (name, expected) in &abi.consumed {
        let actual = meta
            .uniforms_type
            .as_ref()
            .ok_or_else(|| LpsError::Validation(String::from("compute shader has no uniforms")))?
            .type_at_path(name)
            .map_err(|e| {
                LpsError::Validation(format!("missing consumed compute slot `{name}`: {e}"))
            })?;
        require_type_eq(format!("consumed `{name}`"), expected, &actual)?;
    }

    for (name, expected) in &abi.produced {
        let actual = meta
            .globals_type
            .as_ref()
            .ok_or_else(|| LpsError::Validation(String::from("compute shader has no globals")))?
            .type_at_path(name)
            .map_err(|e| {
                LpsError::Validation(format!("missing produced compute slot `{name}`: {e}"))
            })?;
        match expected {
            ComputeOutputAbi::Value(expected) => {
                require_type_eq(format!("produced `{name}`"), expected, &actual)?;
            }
            ComputeOutputAbi::SentinelArray { element, len, key } => {
                let expected_array = LpsType::Array {
                    element: Box::new(element.clone()),
                    len: *len,
                };
                require_type_eq(format!("produced `{name}`"), &expected_array, &actual)?;
                validate_sentinel_key(name, element, key)?;
            }
        }
    }

    Ok(())
}

fn validate_sentinel_key(name: &str, element: &LpsType, key: &str) -> Result<(), LpsError> {
    let key_ty = element.type_at_path(key).map_err(|e| {
        LpsError::Validation(format!(
            "produced sentinel array `{name}` key field `{key}` is missing: {e}"
        ))
    })?;
    if key_ty != LpsType::UInt {
        return Err(LpsError::Validation(format!(
            "produced sentinel array `{name}` key field `{key}` must be uint/u32, got {key_ty:?}"
        )));
    }
    Ok(())
}

fn require_type_eq(label: String, expected: &LpsType, actual: &LpsType) -> Result<(), LpsError> {
    if expected != actual {
        return Err(LpsError::Validation(format!(
            "compute ABI mismatch for {label}: expected {expected:?}, got {actual:?}"
        )));
    }
    Ok(())
}
