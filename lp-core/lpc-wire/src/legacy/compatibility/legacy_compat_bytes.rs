//! M4.1 legacy compatibility: inline bytes vs store [`ResourceRef`] for heavy wire fields.
//!
//! JSON keeps semantic keys (`channel_data`, `lamp_colors`, `texture_data`). Inline values
//! remain a standard base64 string.
//!
//! Resource references use a reserved ASCII prefix so we avoid `serde(untagged)` /
//! `deserialize_any`: **`serde_json_core` (embedded sync) does not support `deserialize_any`**, so
//! resource-backed fields encode as a **single JSON string** `$lp:res/<domain>/<id>` (domain matches
//! [`ResourceDomain`](lpc_model::ResourceDomain) snake_case JSON). Standard base64 does not use `$`,
//! so inline payloads cannot collide with this prefix.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use lpc_model::project::FrameId;
use lpc_model::{ResourceDomain, ResourceRef, Versioned};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Prefix for wire-encoded [`ResourceRef`] on heavy byte fields (`serde_json_core`-safe).
pub const LEGACY_COMPAT_RESOURCE_STR_PREFIX: &str = "$lp:res/";

/// Payload for a heavy byte field: compatibility snapshot or resource handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyCompatBytesBody {
    Inline(Vec<u8>),
    Resource(ResourceRef),
}

/// Frame-tracked heavy byte field (output channels, fixture colors, texture pixels).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyCompatBytesField {
    inner: Versioned<LegacyCompatBytesBody>,
}

impl LegacyCompatBytesField {
    #[must_use]
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            inner: Versioned::new(frame_id, LegacyCompatBytesBody::Inline(Vec::new())),
        }
    }

    #[must_use]
    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    #[must_use]
    pub fn body(&self) -> &LegacyCompatBytesBody {
        self.inner.value()
    }

    /// Bytes when [`LegacyCompatBytesBody::Inline`]; empty when only a resource ref is set.
    #[must_use]
    pub fn inline_bytes(&self) -> &[u8] {
        match self.body() {
            LegacyCompatBytesBody::Inline(v) => v.as_slice(),
            LegacyCompatBytesBody::Resource(_) => &[],
        }
    }

    #[must_use]
    pub fn resource_ref(&self) -> Option<ResourceRef> {
        match self.body() {
            LegacyCompatBytesBody::Resource(r) => Some(*r),
            LegacyCompatBytesBody::Inline(_) => None,
        }
    }

    pub fn set_inline(&mut self, frame_id: FrameId, bytes: Vec<u8>) {
        self.inner
            .set(frame_id, LegacyCompatBytesBody::Inline(bytes));
    }

    pub fn set_resource(&mut self, frame_id: FrameId, resource: ResourceRef) {
        self.inner
            .set(frame_id, LegacyCompatBytesBody::Resource(resource));
    }

    /// Partial merge: empty inline in `other` means “omit” (preserve `self`).
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        match other.body() {
            LegacyCompatBytesBody::Resource(r) => {
                self.set_resource(frame_id, *r);
            }
            LegacyCompatBytesBody::Inline(v) => {
                if !v.is_empty() {
                    self.set_inline(frame_id, v.clone());
                }
            }
        }
    }
}

fn resource_domain_wire_name(domain: ResourceDomain) -> &'static str {
    match domain {
        ResourceDomain::RuntimeBuffer => "runtime_buffer",
        ResourceDomain::RenderProduct => "render_product",
    }
}

fn parse_resource_domain(s: &str) -> Option<ResourceDomain> {
    match s {
        "runtime_buffer" => Some(ResourceDomain::RuntimeBuffer),
        "render_product" => Some(ResourceDomain::RenderProduct),
        _ => None,
    }
}

#[must_use]
pub fn encode_legacy_compat_resource_str(resource: ResourceRef) -> String {
    format!(
        "{}{}/{}",
        LEGACY_COMPAT_RESOURCE_STR_PREFIX,
        resource_domain_wire_name(resource.domain),
        resource.id
    )
}

fn decode_legacy_compat_resource_str(s: &str) -> Option<ResourceRef> {
    let rest = s.strip_prefix(LEGACY_COMPAT_RESOURCE_STR_PREFIX)?;
    let (dom, id_str) = rest.rsplit_once('/')?;
    let id: u32 = id_str.parse().ok()?;
    let domain = parse_resource_domain(dom)?;
    Some(ResourceRef { domain, id })
}

impl Serialize for LegacyCompatBytesField {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.body() {
            LegacyCompatBytesBody::Inline(v) => {
                use base64::Engine;
                let enc = base64::engine::general_purpose::STANDARD.encode(v);
                enc.serialize(serializer)
            }
            LegacyCompatBytesBody::Resource(resource) => {
                encode_legacy_compat_resource_str(*resource).serialize(serializer)
            }
        }
    }
}

struct LegacyCompatBytesFieldVisitor;

impl<'de> Visitor<'de> for LegacyCompatBytesFieldVisitor {
    type Value = LegacyCompatBytesField;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a base64 string or a $lp:res/… resource token")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
        let frame_id = FrameId::default();
        let mut out = LegacyCompatBytesField::new(frame_id);
        if let Some(resource) = decode_legacy_compat_resource_str(s) {
            out.set_resource(frame_id, resource);
        } else {
            use base64::Engine;
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(s.as_bytes()) {
                out.set_inline(frame_id, decoded);
            }
        }
        Ok(out)
    }

    fn visit_borrowed_str<E: de::Error>(self, s: &'de str) -> Result<Self::Value, E> {
        self.visit_str(s)
    }

    fn visit_string<E: de::Error>(self, s: String) -> Result<Self::Value, E> {
        self.visit_str(&s)
    }
}

impl<'de> Deserialize<'de> for LegacyCompatBytesField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(LegacyCompatBytesFieldVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    #[test]
    fn resource_str_round_trips() {
        let r = ResourceRef {
            domain: ResourceDomain::RenderProduct,
            id: 42,
        };
        let s = encode_legacy_compat_resource_str(r);
        assert_eq!(s, "$lp:res/render_product/42");
        let mut f = LegacyCompatBytesField::new(FrameId::default());
        f.set_resource(FrameId::new(1), r);
        let j = json::to_string(&f).unwrap();
        let g: LegacyCompatBytesField = json::from_str(&j).unwrap();
        assert_eq!(g.resource_ref(), Some(r));
    }
}
