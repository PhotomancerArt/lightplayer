//! Group parsed LPFX functions by name and by overload signature.

use crate::lpfn::types::FunctionSignature;
use crate::lpfn::validate::ParsedLpfnFunction;
use std::collections::HashMap;

/// Group functions by GLSL function name.
pub fn group_functions_by_name(
    parsed_functions: &[ParsedLpfnFunction],
) -> HashMap<String, Vec<&ParsedLpfnFunction>> {
    let mut grouped: HashMap<String, Vec<&ParsedLpfnFunction>> = HashMap::new();

    for func in parsed_functions {
        let glsl_name = func.glsl_sig.name.clone();
        grouped.entry(glsl_name).or_default().push(func);
    }

    grouped
}

/// Group functions by unique signature (name + return type + parameters).
pub fn group_by_signature<'a>(
    functions: &'a [&'a ParsedLpfnFunction],
) -> Vec<(Vec<&'a ParsedLpfnFunction>, &'a FunctionSignature)> {
    let mut by_sig: HashMap<String, Vec<&ParsedLpfnFunction>> = HashMap::new();

    for func in functions {
        let key = signature_key(&func.glsl_sig);
        by_sig.entry(key).or_default().push(func);
    }

    by_sig
        .into_values()
        .map(|funcs| {
            let sig = &funcs[0].glsl_sig;
            (funcs, sig)
        })
        .collect()
}

fn signature_key(sig: &FunctionSignature) -> String {
    let mut key = format!("{}:", sig.name);
    key.push_str(&format!("{:?}", sig.return_type));
    for param in &sig.parameters {
        key.push_str(&format!("{:?}{:?}", param.ty, param.qualifier));
    }
    key
}
