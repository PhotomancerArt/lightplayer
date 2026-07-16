//! One consumed shader uniform, projected for editor completions.

/// A uniform the edited shader consumes (a `consumed` map entry on the
/// shader def), carried on [`crate::UiAssetEditor::uniforms`] so the GLSL
/// editor can offer it as a completion.
///
/// `glsl_type` is the type name the generated uniform header declares
/// (`lpc_model::glsl_type_for_lp_type` — the same mapping the header
/// generator uses), so completions never show a type the compiler would
/// disagree with.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiShaderUniform {
    /// The uniform's GLSL identifier (the consumed map key), e.g. `"time"`.
    pub name: String,
    /// GLSL type name as declared by the generated header, e.g. `"float"`.
    pub glsl_type: String,
}
