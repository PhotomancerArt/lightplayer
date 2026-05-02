use alloc::string::String;
use lpc_model::ResourceRef;
use lpc_model::Versioned;
use lpc_model::project::FrameId;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

/// Shader node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderState {
    /// Actual GLSL code loaded from file
    pub glsl_code: Versioned<String>,
    /// Compilation/runtime errors
    pub error: Versioned<Option<String>>,
    /// Semantic render-product handle for shader output (not packed into `glsl_code`).
    pub render_product: Versioned<Option<ResourceRef>>,
}

impl ShaderState {
    /// Create a new ShaderState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            glsl_code: Versioned::new(frame_id, String::new()),
            error: Versioned::new(frame_id, None),
            render_product: Versioned::new(frame_id, None),
        }
    }

    /// Merge fields from a partial update into this state
    ///
    /// Only fields that are present in `other` (non-default values) are merged.
    /// Fields not present in the partial update are preserved from `self`.
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        if !other.glsl_code.value().is_empty() {
            self.glsl_code
                .set(frame_id, other.glsl_code.value().clone());
        }
        if let Some(err) = other.error.value() {
            self.error.set(
                frame_id,
                if err.is_empty() {
                    None
                } else {
                    Some(err.clone())
                },
            );
        }
        if other.render_product.value().is_some() {
            self.render_product
                .set(frame_id, *other.render_product.value());
        }
    }
}

pub struct SerializableShaderState<'a> {
    state: &'a ShaderState,
    since_frame: FrameId,
}

impl<'a> SerializableShaderState<'a> {
    pub fn new(state: &'a ShaderState, since_frame: FrameId) -> Self {
        Self { state, since_frame }
    }
}

impl Serialize for SerializableShaderState<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let is_initial = self.since_frame == FrameId::default();
        let include_glsl = is_initial || self.state.glsl_code.changed_frame() > self.since_frame;
        let include_err = is_initial || self.state.error.changed_frame() > self.since_frame;
        let include_rp = is_initial || self.state.render_product.changed_frame() > self.since_frame;

        let count = usize::from(include_glsl) + usize::from(include_err) + usize::from(include_rp);
        let mut st = serializer.serialize_struct("ShaderState", count)?;
        if include_glsl {
            st.serialize_field("glsl_code", self.state.glsl_code.value())?;
        }
        if include_err {
            let s: &str = self.state.error.value().as_deref().unwrap_or("");
            st.serialize_field("error", s)?;
        }
        if include_rp {
            st.serialize_field("render_product", self.state.render_product.value())?;
        }
        st.end()
    }
}

impl Serialize for ShaderState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut st = serializer.serialize_struct("ShaderState", 3)?;
        st.serialize_field("glsl_code", self.glsl_code.value())?;
        let s: &str = self.error.value().as_deref().unwrap_or("");
        st.serialize_field("error", s)?;
        st.serialize_field("render_product", self.render_product.value())?;
        st.end()
    }
}

#[derive(Deserialize)]
struct ShaderStateHelper {
    glsl_code: Option<String>,
    error: Option<String>,
    render_product: Option<ResourceRef>,
}

impl<'de> Deserialize<'de> for ShaderState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = ShaderStateHelper::deserialize(deserializer)?;
        let frame_id = FrameId::default();
        let mut state = ShaderState::new(frame_id);
        if let Some(v) = helper.glsl_code {
            state.glsl_code.set(frame_id, v);
        }
        if let Some(v) = helper.error {
            state
                .error
                .set(frame_id, if v.is_empty() { None } else { Some(v) });
        }
        if let Some(v) = helper.render_product {
            state.render_product.set(frame_id, Some(v));
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::RenderProductId;

    #[test]
    fn test_merge_error_cleared() {
        let frame = FrameId::new(1);
        let mut existing = ShaderState::new(frame);
        existing.error.set(frame, Some("compilation failed".into()));

        let mut update = ShaderState::new(frame);
        update.error.set(frame, Some(String::new()));

        existing.merge_from(&update, frame);
        assert_eq!(existing.error.value(), &None);
    }

    #[test]
    fn test_merge_error_omitted_preserved() {
        let frame = FrameId::new(1);
        let mut existing = ShaderState::new(frame);
        existing.error.set(frame, Some("old error".into()));
        existing.glsl_code.set(frame, "void main() {}".into());

        let mut update = ShaderState::new(frame);
        update.glsl_code.set(frame, "void main() { }".into());

        existing.merge_from(&update, frame);
        assert_eq!(existing.error.value(), &Some("old error".into()));
    }

    #[test]
    fn render_product_roundtrip_json() {
        use crate::json;
        let frame = FrameId::new(2);
        let mut s = ShaderState::new(frame);
        let r = ResourceRef::render_product(RenderProductId::new(5));
        s.render_product.set(frame, Some(r));
        let j = json::to_string(&s).unwrap();
        assert!(j.contains("render_product"));
        let back: ShaderState = json::from_str(&j).unwrap();
        assert_eq!(back.render_product.value(), &Some(r));
    }
}
