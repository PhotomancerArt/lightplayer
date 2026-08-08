//! Front-half-only symbol analysis for editor autocomplete.
//!
//! [`analyze_symbols`] runs the compile pipeline's Header phase (lex →
//! top-level index → signature builders) against arbitrary editor text and
//! returns the user-declared symbols with GLSL-rendered types. It never
//! parses function bodies, type-checks const initializers, or emits code —
//! it is read-only over the compile path and adds no compile-time cost.
//!
//! Error tolerance is all-or-nothing: the first diagnostic aborts the whole
//! analysis (no per-declaration recovery yet). Callers that track a live
//! buffer should keep the last good [`SymbolAnalysis`] when this returns
//! `Err` — mid-edit states (an unbalanced brace while typing a new function)
//! are expected to fail. The resilience tests at the bottom of this file pin
//! exactly what survives and what aborts.

use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::{ParamQualifier, glsl_type_name};

use crate::index::index_tokens;
use crate::lexer::lex;
use crate::{Diagnostic, TopLevelIndex};

use super::function::FunctionSig;
use super::types::StructTypes;
use super::{
    build_array_size_consts, build_function_sigs, build_global_vars, build_struct_types,
    build_uniforms, type_ref_to_lps_with_structs,
};

/// User-declared symbols extracted from GLSL source text.
///
/// All types are pre-rendered GLSL spellings (via
/// [`lps_shared::glsl_type_name`]) so consumers need no compiler-internal
/// types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolAnalysis {
    /// User-defined functions, in declaration order.
    pub functions: Vec<FnSymbol>,
    /// Global variables and global consts, in declaration order
    /// (consts first, then vars — matching the header build order).
    pub globals: Vec<VarSymbol>,
    /// User-declared struct types, in declaration order.
    pub structs: Vec<StructSymbol>,
    /// `layout(...) uniform` declarations, in declaration order.
    pub uniforms: Vec<VarSymbol>,
}

/// A user-defined function with its typed signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSymbol {
    pub name: String,
    /// GLSL spelling of the return type (e.g. `vec3`).
    pub return_type: String,
    pub params: Vec<FnParamSymbol>,
}

/// One parameter of a [`FnSymbol`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnParamSymbol {
    /// Parameter name; `None` for unnamed parameters.
    pub name: Option<String>,
    /// GLSL spelling of the parameter type.
    pub type_name: String,
    pub qualifier: ParamQualifier,
}

/// A named value symbol (global var, const, or uniform) with its GLSL type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarSymbol {
    pub name: String,
    /// GLSL spelling of the declared type.
    pub type_name: String,
}

/// A user-declared struct type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructSymbol {
    pub name: String,
    /// Member fields, in declaration order.
    pub fields: Vec<VarSymbol>,
}

/// Extract user-declared symbols from GLSL source for editor autocomplete.
///
/// Runs only the compile front half (lex, top-level index, and the header
/// signature builders); function bodies and const initializer values are
/// never type-checked, so undefined names inside bodies do not fail the
/// analysis. Returns `Err` with the first diagnostic when the source is not
/// analyzable (see the module docs for the tolerance envelope) — callers
/// should treat that as "keep the previous symbol set".
pub fn analyze_symbols(source: &str) -> Result<SymbolAnalysis, Diagnostic> {
    let tokens = lex(source)?;
    let index = index_tokens(source, &tokens)?;
    let (array_size_consts, _const_init_cache) = build_array_size_consts(source, &tokens, &index)?;
    let structs = build_struct_types(&index, &array_size_consts)?;
    let (uniforms, _uniforms_type, uniforms_size) =
        build_uniforms(&index, &structs, &array_size_consts)?;
    let (global_vars, _globals_type, _global_inits) = build_global_vars(
        source,
        &tokens,
        &index,
        &structs,
        &array_size_consts,
        uniforms_size,
    )?;
    let function_sigs = build_function_sigs(&index, &structs, &array_size_consts)?;

    let mut globals = Vec::new();
    for konst in &index.consts {
        let ty = type_ref_to_lps_with_structs(&konst.ty, &structs, &array_size_consts)?;
        globals.push(VarSymbol {
            name: konst.name.clone(),
            type_name: glsl_type_name(&ty),
        });
    }
    globals.extend(global_vars.iter().map(|(name, info)| VarSymbol {
        name: name.clone(),
        type_name: glsl_type_name(&info.ty),
    }));

    Ok(SymbolAnalysis {
        functions: function_sigs.iter().map(fn_symbol).collect(),
        globals,
        structs: struct_symbols(&index, &structs),
        uniforms: uniforms
            .iter()
            .map(|(name, info)| VarSymbol {
                name: name.clone(),
                type_name: glsl_type_name(&info.ty),
            })
            .collect(),
    })
}

fn fn_symbol(sig: &FunctionSig) -> FnSymbol {
    FnSymbol {
        name: sig.name.clone(),
        return_type: glsl_type_name(&sig.return_ty),
        params: sig
            .params
            .iter()
            .map(|p| FnParamSymbol {
                name: p.name.clone(),
                type_name: glsl_type_name(&p.ty),
                qualifier: p.qualifier,
            })
            .collect(),
    }
}

fn struct_symbols(index: &TopLevelIndex, structs: &StructTypes) -> Vec<StructSymbol> {
    index
        .structs
        .iter()
        .map(|decl| {
            let fields = structs
                .get(&decl.name)
                .and_then(|ty| match ty {
                    lps_shared::LpsType::Struct { members, .. } => Some(members),
                    _ => None,
                })
                .map(|members| {
                    members
                        .iter()
                        .map(|member| VarSymbol {
                            name: member.name.clone().unwrap_or_default(),
                            type_name: glsl_type_name(&member.ty),
                        })
                        .collect()
                })
                .unwrap_or_default();
            StructSymbol {
                name: decl.name.clone(),
                fields,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn complete_shader_yields_all_symbol_kinds() {
        let source = "\
struct Light { vec3 color; float intensity; };
const int COUNT = 4;
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
float phase = 0.25;
vec3 tonemap(vec3 color, float exposure) { return color * exposure; }
vec4 render(vec2 pos) { return vec4(tonemap(vec3(pos, 0.0), 1.0), 1.0); }
";
        let symbols = analyze_symbols(source).expect("complete shader analyzes");

        let fn_names: Vec<&str> = symbols.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(fn_names, ["tonemap", "render"]);
        let tonemap = &symbols.functions[0];
        assert_eq!(tonemap.return_type, "vec3");
        assert_eq!(tonemap.params.len(), 2);
        assert_eq!(tonemap.params[0].name.as_deref(), Some("color"));
        assert_eq!(tonemap.params[0].type_name, "vec3");
        assert_eq!(tonemap.params[0].qualifier, ParamQualifier::In);
        assert_eq!(tonemap.params[1].name.as_deref(), Some("exposure"));
        assert_eq!(tonemap.params[1].type_name, "float");

        assert_eq!(
            symbols.globals,
            [
                VarSymbol {
                    name: "COUNT".to_string(),
                    type_name: "int".to_string(),
                },
                VarSymbol {
                    name: "phase".to_string(),
                    type_name: "float".to_string(),
                },
            ]
        );

        assert_eq!(symbols.structs.len(), 1);
        assert_eq!(symbols.structs[0].name, "Light");
        assert_eq!(
            symbols.structs[0].fields,
            [
                VarSymbol {
                    name: "color".to_string(),
                    type_name: "vec3".to_string(),
                },
                VarSymbol {
                    name: "intensity".to_string(),
                    type_name: "float".to_string(),
                },
            ]
        );

        assert_eq!(
            symbols.uniforms,
            [
                VarSymbol {
                    name: "outputSize".to_string(),
                    type_name: "vec2".to_string(),
                },
                VarSymbol {
                    name: "time".to_string(),
                    type_name: "float".to_string(),
                },
            ]
        );
    }

    #[test]
    fn defined_render_appears_as_function_symbol() {
        let symbols = analyze_symbols("vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }")
            .expect("render-only shader analyzes");
        assert!(symbols.functions.iter().any(|f| f.name == "render"));
    }

    #[test]
    fn struct_typed_and_array_symbols_render_glsl_names() {
        let source = "\
struct Light { vec3 color; };
const int N = 2;
Light lights[N];
Light pick(Light lights[2], int i) { return lights[0]; }
";
        let symbols = analyze_symbols(source).expect("struct/array shader analyzes");
        assert_eq!(symbols.globals[1].name, "lights");
        assert_eq!(symbols.globals[1].type_name, "Light[2]");
        assert_eq!(symbols.functions[0].return_type, "Light");
        assert_eq!(symbols.functions[0].params[0].type_name, "Light[2]");
    }

    #[test]
    fn out_qualifier_is_reported() {
        let symbols = analyze_symbols("void split(vec2 v, out float x) { x = v.x; }")
            .expect("out-param shader analyzes");
        assert_eq!(
            symbols.functions[0].params[1].qualifier,
            ParamQualifier::Out
        );
    }

    // ---- Resilience characterization: what SURVIVES ----
    //
    // These pin the current all-or-nothing tolerance envelope so future
    // per-declaration recovery work has a baseline to diff against.

    #[test]
    fn survives_garbage_statements_inside_balanced_body() {
        // The index only brace-counts bodies; body statements are never
        // parsed by the front half, so lexable nonsense inside is fine.
        let source = "\
float ok(float t) { return t; }
vec3 broken(vec3 c) { this is ( not ) valid glsl ; ; ; }
";
        let symbols = analyze_symbols(source).expect("balanced garbage body analyzes");
        let names: Vec<&str> = symbols.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["ok", "broken"]);
    }

    #[test]
    fn survives_undefined_names_in_global_initializer() {
        // Global-var initializers are parsed but not type-checked by the
        // front half, so a reference to an undefined name still analyzes.
        let symbols =
            analyze_symbols("float x = someUndefinedName;").expect("undefined init name analyzes");
        assert_eq!(symbols.globals[0].name, "x");
    }

    // ---- Resilience characterization: what ABORTS ----

    #[test]
    fn aborts_on_unbalanced_brace() {
        // The dominant mid-edit state: a new function before its `}` exists.
        assert!(analyze_symbols("float ok(float t) { return t; }\nvec3 f() {").is_err());
    }

    #[test]
    fn aborts_on_stray_lexer_rejected_char() {
        assert!(analyze_symbols("int x = 1 & 2;").is_err());
        assert!(analyze_symbols("float y = 0.0; @").is_err());
    }

    #[test]
    fn aborts_on_unterminated_block_comment() {
        assert!(analyze_symbols("float x = 1.0; /* dangling").is_err());
    }

    #[test]
    fn aborts_on_incomplete_top_level_decl_at_eof() {
        assert!(analyze_symbols("float ok(float t) { return t; }\nfloat x").is_err());
    }

    #[test]
    fn aborts_on_unknown_type_name_in_signature() {
        // A half-typed type name in a signature fails signature building.
        assert!(analyze_symbols("ve render(vec2 pos) { return pos; }").is_err());
    }

    #[test]
    fn failed_analysis_returns_no_partial_symbols() {
        // Even with valid declarations before the error point, the whole
        // analysis aborts — callers must keep-last-good client-side.
        let source = "\
float ok(float t) { return t; }
vec3 f() {";
        assert!(analyze_symbols(source).is_err());
    }
}
