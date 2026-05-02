use alloc::vec::Vec;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use lpc_model::NodeId;
use lpc_model::Versioned;
use lpc_model::project::FrameId;

use crate::legacy::compatibility::LegacyCompatBytesField;

/// Mapping cell - represents a post-transform sampling region
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappingCell {
    /// Output channel index
    pub channel: u32,
    /// Center coordinates in texture space [0, 1] (post-transform)
    pub center: [f32; 2],
    /// Sampling radius
    pub radius: f32,
}

/// Fixture node state - runtime values
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureState {
    /// Lamp color values: inline RGB bytes or runtime-buffer ref (fixture colors buffer).
    pub lamp_colors: LegacyCompatBytesField,
    /// Post-transform mapping cells (sampling regions) — inline compatibility snapshot for M4.1.
    pub mapping_cells: Versioned<Vec<MappingCell>>,
    /// Resolved texture handle (if fixture has been initialized)
    pub texture_handle: Versioned<Option<NodeId>>,
    /// Resolved output handle (if fixture has been initialized)
    pub output_handle: Versioned<Option<NodeId>>,
}

impl FixtureState {
    /// Create a new FixtureState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            lamp_colors: LegacyCompatBytesField::new(frame_id),
            mapping_cells: Versioned::new(frame_id, Vec::new()),
            texture_handle: Versioned::new(frame_id, None),
            output_handle: Versioned::new(frame_id, None),
        }
    }

    /// Merge fields from a partial update into this state
    ///
    /// Only fields that are present in `other` (non-default values) are merged.
    /// Fields not present in the partial update are preserved from `self`.
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        self.lamp_colors.merge_from(&other.lamp_colors, frame_id);
        if !other.mapping_cells.value().is_empty() {
            self.mapping_cells
                .set(frame_id, other.mapping_cells.value().clone());
        }
        if other.texture_handle.value().is_some() {
            self.texture_handle
                .set(frame_id, *other.texture_handle.value());
        }
        if other.output_handle.value().is_some() {
            self.output_handle
                .set(frame_id, *other.output_handle.value());
        }
    }
}

pub struct SerializableFixtureState<'a> {
    state: &'a FixtureState,
    since_frame: FrameId,
}

impl<'a> SerializableFixtureState<'a> {
    pub fn new(state: &'a FixtureState, since_frame: FrameId) -> Self {
        Self { state, since_frame }
    }
}

impl Serialize for SerializableFixtureState<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let is_initial = self.since_frame == FrameId::default();
        let include_lamp = is_initial || self.state.lamp_colors.changed_frame() > self.since_frame;
        let include_mapping =
            is_initial || self.state.mapping_cells.changed_frame() > self.since_frame;
        let include_tex =
            is_initial || self.state.texture_handle.changed_frame() > self.since_frame;
        let include_out = is_initial || self.state.output_handle.changed_frame() > self.since_frame;

        let count = usize::from(include_lamp)
            + usize::from(include_mapping)
            + usize::from(include_tex)
            + usize::from(include_out);
        let mut st = serializer.serialize_struct("FixtureState", count)?;
        if include_lamp {
            st.serialize_field("lamp_colors", &self.state.lamp_colors)?;
        }
        if include_mapping {
            st.serialize_field("mapping_cells", self.state.mapping_cells.value())?;
        }
        if include_tex {
            st.serialize_field("texture_handle", self.state.texture_handle.value())?;
        }
        if include_out {
            st.serialize_field("output_handle", self.state.output_handle.value())?;
        }
        st.end()
    }
}

impl Serialize for FixtureState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut st = serializer.serialize_struct("FixtureState", 4)?;
        st.serialize_field("lamp_colors", &self.lamp_colors)?;
        st.serialize_field("mapping_cells", self.mapping_cells.value())?;
        st.serialize_field("texture_handle", self.texture_handle.value())?;
        st.serialize_field("output_handle", self.output_handle.value())?;
        st.end()
    }
}

#[derive(Deserialize)]
struct FixtureStateHelper {
    lamp_colors: Option<LegacyCompatBytesField>,
    mapping_cells: Option<Vec<MappingCell>>,
    texture_handle: Option<Option<NodeId>>,
    output_handle: Option<Option<NodeId>>,
}

impl<'de> Deserialize<'de> for FixtureState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = FixtureStateHelper::deserialize(deserializer)?;
        let frame_id = FrameId::default();
        let mut state = FixtureState::new(frame_id);
        if let Some(v) = helper.lamp_colors {
            state.lamp_colors = v;
        }
        if let Some(v) = helper.mapping_cells {
            state.mapping_cells.set(frame_id, v);
        }
        if let Some(v) = helper.texture_handle {
            state.texture_handle.set(frame_id, v);
        }
        if let Some(v) = helper.output_handle {
            state.output_handle.set(frame_id, v);
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;
    use alloc::format;
    use alloc::vec;
    use lpc_model::{ResourceRef, RuntimeBufferId};

    #[test]
    fn lamp_colors_inline_roundtrip() {
        use base64::Engine;
        let bytes = vec![1u8, 2, 3];
        let enc = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let j = format!(r#"{{"lamp_colors":"{}"}}"#, enc);
        let s: FixtureState = json::from_str(&j).unwrap();
        assert_eq!(s.lamp_colors.inline_bytes(), bytes.as_slice());
    }

    #[test]
    fn lamp_colors_resource_roundtrip() {
        let rid = ResourceRef::runtime_buffer(RuntimeBufferId::new(99));
        let mut s = FixtureState::new(FrameId::new(1));
        s.lamp_colors.set_resource(FrameId::new(2), rid);
        let j = json::to_string(&s).unwrap();
        assert!(j.contains("lamp_colors"));
        assert!(j.contains("$lp:res/runtime_buffer/99"));
        let back: FixtureState = json::from_str(&j).unwrap();
        assert_eq!(back.lamp_colors.resource_ref(), Some(rid));
    }

    #[test]
    fn partial_merge_keeps_lamp_colors_when_omitted() {
        let f = FrameId::new(5);
        let mut existing = FixtureState::new(f);
        existing.lamp_colors.set_inline(f, vec![7, 8, 9]);
        let partial = FixtureState::new(FrameId::default());
        existing.merge_from(&partial, FrameId::new(6));
        assert_eq!(existing.lamp_colors.inline_bytes(), &[7, 8, 9]);
    }

    #[test]
    fn json_field_names_unchanged() {
        let mut s = FixtureState::new(FrameId::new(1));
        s.mapping_cells.set(FrameId::new(1), vec![]);
        let j = json::to_string(&s).unwrap();
        assert!(j.contains("lamp_colors"));
        assert!(j.contains("mapping_cells"));
        assert!(j.contains("texture_handle"));
        assert!(j.contains("output_handle"));
    }
}
