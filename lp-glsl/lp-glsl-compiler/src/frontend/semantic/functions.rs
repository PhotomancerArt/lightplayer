//! User-defined function registry and type checking

use crate::error::{ErrorCode, GlslError};
use crate::frontend::semantic::type_check::can_implicitly_convert;
use crate::frontend::semantic::types::Type;
use hashbrown::HashMap;

use alloc::{format, string::String, vec::Vec};
#[derive(Clone)]
pub struct FunctionRegistry {
    functions: HashMap<String, Vec<FunctionSignature>>,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub return_type: Type,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub ty: Type,
    pub qualifier: ParamQualifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamQualifier {
    In,    // Copy in (default)
    Out,   // Copy out (Phase 8)
    InOut, // Copy in and out (Phase 8)
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    pub fn register_function(&mut self, sig: FunctionSignature) -> Result<(), GlslError> {
        self.functions
            .entry(sig.name.clone())
            .or_insert_with(Vec::new)
            .push(sig);
        Ok(())
    }

    pub fn lookup_function(
        &self,
        name: &str,
        arg_types: &[Type],
    ) -> Result<&FunctionSignature, GlslError> {
        let overloads = self.functions.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("undefined function `{name}`"))
                .with_note(format!("function `{name}` is not defined"))
        })?;

        // Try exact match first
        for sig in overloads {
            if Self::exact_match(sig, arg_types) {
                return Ok(sig);
            }
        }

        // Try with implicit conversions
        for sig in overloads {
            if Self::convertible_match(sig, arg_types) {
                return Ok(sig);
            }
        }

        Err(GlslError::new(
            ErrorCode::E0114,
            format!("no matching overload for function `{name}`"),
        )
        .with_note(format!(
            "cannot find function `{name}` that accepts arguments of type {arg_types:?}"
        )))
    }

    fn exact_match(sig: &FunctionSignature, arg_types: &[Type]) -> bool {
        if sig.parameters.len() != arg_types.len() {
            return false;
        }
        sig.parameters
            .iter()
            .zip(arg_types)
            .all(|(p, a)| p.ty == *a)
    }

    fn convertible_match(sig: &FunctionSignature, arg_types: &[Type]) -> bool {
        if sig.parameters.len() != arg_types.len() {
            return false;
        }

        for (param, arg_ty) in sig.parameters.iter().zip(arg_types) {
            // For 'in' parameters, arg must be convertible to param type
            if !can_implicitly_convert(arg_ty, &param.ty) {
                return false;
            }
        }

        true
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
