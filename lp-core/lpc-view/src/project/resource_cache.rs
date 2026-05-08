//! Client-side cache for store-backed resource summaries and payloads.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use lpc_model::{ResourceDomain, ResourceRef};
use lpc_wire::{
    WireChannelSampleFormat, WireRenderProductPayload, WireResourceSummary,
    WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload,
};

/// Cached resource summaries and payload bytes from project sync.
#[derive(Debug, Default)]
pub struct ClientResourceCache {
    summaries: BTreeMap<ResourceRef, WireResourceSummary>,
    runtime_buffer_bytes: BTreeMap<ResourceRef, Vec<u8>>,
    runtime_buffer_metadata: BTreeMap<ResourceRef, WireRuntimeBufferMetadataPayload>,
    render_product_bytes: BTreeMap<ResourceRef, Vec<u8>>,
}

impl ClientResourceCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Latest summary for a ref, if cached.
    #[must_use]
    pub fn summary(&self, resource_ref: ResourceRef) -> Option<&WireResourceSummary> {
        self.summaries.get(&resource_ref)
    }

    /// Apply store summaries; when non-empty, membership is authoritative per domain present.
    pub fn apply_summaries(&mut self, summaries: &[WireResourceSummary]) {
        if summaries.is_empty() {
            return;
        }

        let mut domains: BTreeSet<ResourceDomain> = BTreeSet::new();
        let mut refs: BTreeSet<ResourceRef> = BTreeSet::new();

        for s in summaries {
            domains.insert(s.resource_ref.domain);
            refs.insert(s.resource_ref);
            self.summaries.insert(s.resource_ref, s.clone());
        }

        self.summaries.retain(|r, _| {
            if !domains.contains(&r.domain) {
                return true;
            }
            refs.contains(r)
        });

        self.runtime_buffer_bytes.retain(|r, _| {
            if r.domain != ResourceDomain::RuntimeBuffer {
                return true;
            }
            if !domains.contains(&ResourceDomain::RuntimeBuffer) {
                return true;
            }
            refs.contains(r)
        });
        self.runtime_buffer_metadata.retain(|r, _| {
            if r.domain != ResourceDomain::RuntimeBuffer {
                return true;
            }
            if !domains.contains(&ResourceDomain::RuntimeBuffer) {
                return true;
            }
            refs.contains(r)
        });

        self.render_product_bytes.retain(|r, _| {
            if r.domain != ResourceDomain::RenderProduct {
                return true;
            }
            if !domains.contains(&ResourceDomain::RenderProduct) {
                return true;
            }
            refs.contains(r)
        });
    }

    pub fn apply_runtime_buffer_payloads(&mut self, payloads: &[WireRuntimeBufferPayload]) {
        for p in payloads {
            self.runtime_buffer_bytes
                .insert(p.resource_ref, p.bytes.clone());
            self.runtime_buffer_metadata
                .insert(p.resource_ref, p.metadata.clone());
        }
    }

    pub fn apply_render_product_payloads(&mut self, payloads: &[WireRenderProductPayload]) {
        for p in payloads {
            self.render_product_bytes
                .insert(p.resource_ref, p.bytes.clone());
        }
    }

    /// Cached bytes for a runtime buffer, if the client requested its payload.
    #[must_use]
    pub fn runtime_buffer_bytes(&self, resource_ref: ResourceRef) -> Option<&[u8]> {
        self.runtime_buffer_bytes
            .get(&resource_ref)
            .map(Vec::as_slice)
    }

    /// Cached output-channel bytes projected for simple byte-oriented previews.
    pub fn output_channel_preview_bytes(
        &self,
        resource_ref: ResourceRef,
    ) -> Result<Vec<u8>, alloc::string::String> {
        if resource_ref.domain != ResourceDomain::RuntimeBuffer {
            return Err(alloc::format!(
                "expected runtime-buffer resource, got {:?}/{}",
                resource_ref.domain,
                resource_ref.id
            ));
        }

        let bytes = self
            .runtime_buffer_bytes
            .get(&resource_ref)
            .ok_or_else(|| {
                alloc::format!(
                    "no cached runtime-buffer payload for ref {:?}/{}",
                    resource_ref.domain,
                    resource_ref.id
                )
            })?;
        match self.runtime_buffer_metadata.get(&resource_ref) {
            Some(WireRuntimeBufferMetadataPayload::OutputChannels {
                sample_format: WireChannelSampleFormat::U16,
                ..
            }) => Ok(bytes
                .chunks_exact(2)
                .map(|chunk| chunk[1])
                .collect::<Vec<_>>()),
            Some(WireRuntimeBufferMetadataPayload::OutputChannels {
                sample_format: WireChannelSampleFormat::U8,
                ..
            })
            | None => Ok(bytes.clone()),
            Some(_) => Ok(bytes.clone()),
        }
    }

    /// Cached bytes for a materialized render product, if the client requested its payload.
    #[must_use]
    pub fn render_product_bytes(&self, resource_ref: ResourceRef) -> Option<&[u8]> {
        self.render_product_bytes
            .get(&resource_ref)
            .map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use lpc_model::project::Revision;
    use lpc_model::{RenderProductId, RuntimeBufferId};
    use lpc_wire::{
        WireChannelSampleFormat, WireRenderProductKind, WireRenderProductPayload,
        WireResourceAvailability, WireResourceKindSummary, WireResourceMetadataSummary,
        WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload,
        WireTextureFormat,
    };

    fn sample_buffer_summary(id: u32, frame: i64) -> WireResourceSummary {
        let buf_id = RuntimeBufferId::new(id);
        WireResourceSummary {
            resource_ref: ResourceRef::runtime_buffer(buf_id),
            revision: Revision::new(frame),
            kind: WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::OutputChannels),
            metadata: WireResourceMetadataSummary::OutputChannels {
                channels: 3,
                sample_format: WireChannelSampleFormat::U8,
            },
            byte_length_hint: Some(9),
            availability: WireResourceAvailability::Available,
        }
    }

    #[test]
    fn project_resource_cache_applies_summaries_without_payloads() {
        let mut cache = ClientResourceCache::new();
        let s = sample_buffer_summary(1, 1);
        let r = s.resource_ref;
        cache.apply_summaries(&[s]);
        assert_eq!(cache.summary(r).map(|x| x.byte_length_hint), Some(Some(9)));
    }

    #[test]
    fn project_resource_cache_resolves_runtime_buffer_payload() {
        let mut cache = ClientResourceCache::new();
        let r = ResourceRef::runtime_buffer(RuntimeBufferId::new(7));
        cache.apply_summaries(&[sample_buffer_summary(7, 1)]);
        cache.apply_runtime_buffer_payloads(&[WireRuntimeBufferPayload {
            resource_ref: r,
            revision: Revision::new(2),
            metadata: WireRuntimeBufferMetadataPayload::OutputChannels {
                channels: 3,
                sample_format: WireChannelSampleFormat::U8,
            },
            bytes: Vec::from([1u8, 2, 3]),
        }]);

        assert_eq!(
            cache.runtime_buffer_bytes(r),
            Some(Vec::from([1u8, 2, 3]).as_slice())
        );
    }

    #[test]
    fn project_resource_cache_resolves_render_product_payload() {
        let mut cache = ClientResourceCache::new();
        let r = ResourceRef::render_product(RenderProductId::new(4));
        cache.apply_summaries(&[WireResourceSummary {
            resource_ref: r,
            revision: Revision::new(1),
            kind: WireResourceKindSummary::RenderProduct(WireRenderProductKind::Texture),
            metadata: WireResourceMetadataSummary::Texture {
                width: 2,
                height: 2,
                format: WireTextureFormat::Rgb8,
            },
            byte_length_hint: Some(12),
            availability: WireResourceAvailability::Available,
        }]);

        cache.apply_render_product_payloads(&[WireRenderProductPayload {
            resource_ref: r,
            revision: Revision::new(2),
            width: 2,
            height: 2,
            format: WireTextureFormat::Rgb8,
            bytes: Vec::from([9u8, 9, 9]),
        }]);

        assert_eq!(
            cache.render_product_bytes(r),
            Some(Vec::from([9u8, 9, 9]).as_slice())
        );
    }

    #[test]
    fn project_resource_cache_prunes_payload_bytes_when_buffer_summaries_shrink() {
        let mut cache = ClientResourceCache::new();
        let a = sample_buffer_summary(1, 1);
        let b = sample_buffer_summary(2, 1);
        let ref_b = b.resource_ref;
        cache.apply_summaries(&[a, b]);
        cache.apply_runtime_buffer_payloads(&[WireRuntimeBufferPayload {
            resource_ref: ref_b,
            revision: Revision::new(1),
            metadata: WireRuntimeBufferMetadataPayload::Raw,
            bytes: Vec::from([7u8, 8]),
        }]);

        assert_eq!(
            cache.runtime_buffer_bytes(ref_b),
            Some(Vec::from([7u8, 8]).as_slice())
        );

        cache.apply_summaries(&[sample_buffer_summary(1, 2)]);
        assert!(cache.runtime_buffer_bytes(ref_b).is_none());
    }

    #[test]
    fn project_resource_cache_resolves_output_u16_payload_as_high_bytes() {
        let mut cache = ClientResourceCache::new();
        let r = ResourceRef::runtime_buffer(RuntimeBufferId::new(5));
        cache.apply_summaries(&[sample_buffer_summary(5, 1)]);
        cache.apply_runtime_buffer_payloads(&[WireRuntimeBufferPayload {
            resource_ref: r,
            revision: Revision::new(1),
            metadata: WireRuntimeBufferMetadataPayload::OutputChannels {
                channels: 2,
                sample_format: WireChannelSampleFormat::U16,
            },
            bytes: Vec::from([7u8, 10, 0, 20]),
        }]);

        assert_eq!(
            cache.output_channel_preview_bytes(r).unwrap(),
            Vec::from([10u8, 20])
        );
    }
}
