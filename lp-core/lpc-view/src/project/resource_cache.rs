//! Client-side cache for store-backed resource summaries and payloads.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use lpc_model::{ResourceDomain, ResourceRef};
use lpc_wire::{
    WireChannelSampleFormat, WireResourceSummary, WireRuntimeBufferMetadataPayload,
    WireRuntimeBufferPayload,
};

/// Cached resource summaries and payload bytes from project sync.
#[derive(Debug, Default)]
pub struct ClientResourceCache {
    summaries: BTreeMap<ResourceRef, WireResourceSummary>,
    runtime_buffer_bytes: BTreeMap<ResourceRef, Vec<u8>>,
    runtime_buffer_metadata: BTreeMap<ResourceRef, WireRuntimeBufferMetadataPayload>,
}

impl ClientResourceCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn summary_count(&self) -> usize {
        self.summaries.len()
    }

    /// Latest summary for a ref, if cached.
    #[must_use]
    pub fn summary(&self, resource_ref: ResourceRef) -> Option<&WireResourceSummary> {
        self.summaries.get(&resource_ref)
    }

    /// Iterate cached resource summaries in stable resource-ref order.
    pub fn summaries(&self) -> impl Iterator<Item = &WireResourceSummary> {
        self.summaries.values()
    }

    /// Additively upsert store summaries.
    ///
    /// A gated (revision-filtered) read carries only the summaries whose buffer
    /// changed since the client's `since`, so this must merge rather than treat
    /// the batch as authoritative membership. Removed resources are pruned
    /// separately via [`Self::prune_to_membership`], driven by the read's
    /// membership list (G4/G7).
    pub fn apply_summaries(&mut self, summaries: &[WireResourceSummary]) {
        for s in summaries {
            self.summaries.insert(s.resource_ref, s.clone());
        }
    }

    /// Prune cached resources whose ref is not in `membership`.
    ///
    /// A gated read includes the full current resource-ref set only when the
    /// store's `ids_revision` moved past the client's `since`; the client then
    /// drops any locally-cached summary and payload for a ref absent from that
    /// list.
    pub fn prune_to_membership(&mut self, membership: &[ResourceRef]) {
        let keep: BTreeSet<ResourceRef> = membership.iter().copied().collect();
        self.summaries.retain(|r, _| keep.contains(r));
        self.runtime_buffer_bytes.retain(|r, _| keep.contains(r));
        self.runtime_buffer_metadata.retain(|r, _| keep.contains(r));
    }

    pub fn apply_runtime_buffer_payloads(&mut self, payloads: &[WireRuntimeBufferPayload]) {
        for p in payloads {
            self.runtime_buffer_bytes
                .insert(p.resource_ref, p.bytes.clone());
            self.runtime_buffer_metadata
                .insert(p.resource_ref, p.metadata.clone());
        }
    }

    /// Cached bytes for a runtime buffer, if the client requested its payload.
    #[must_use]
    pub fn runtime_buffer_bytes(&self, resource_ref: ResourceRef) -> Option<&[u8]> {
        self.runtime_buffer_bytes
            .get(&resource_ref)
            .map(Vec::as_slice)
    }

    /// Cached runtime-buffer payload bytes and metadata, if requested.
    #[must_use]
    pub fn runtime_buffer_payload(
        &self,
        resource_ref: ResourceRef,
    ) -> Option<(&[u8], &WireRuntimeBufferMetadataPayload)> {
        Some((
            self.runtime_buffer_bytes.get(&resource_ref)?.as_slice(),
            self.runtime_buffer_metadata.get(&resource_ref)?,
        ))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use lpc_model::RuntimeBufferId;
    use lpc_model::project::Revision;
    use lpc_wire::{
        WireChannelSampleFormat, WireResourceAvailability, WireResourceKindSummary,
        WireResourceMetadataSummary, WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload,
        WireRuntimeBufferPayload,
    };

    fn sample_buffer_summary(id: u32, frame: i64) -> WireResourceSummary {
        let buf_id = RuntimeBufferId::new(id);
        WireResourceSummary {
            resource_ref: ResourceRef::runtime_buffer(buf_id),
            owner: None,
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
    fn project_resource_cache_apply_summaries_is_additive() {
        // A gated read carries only the changed summary; unchanged entries and their
        // payloads must survive (apply_summaries no longer treats the batch as
        // authoritative membership).
        let mut cache = ClientResourceCache::new();
        let a = sample_buffer_summary(1, 1);
        let b = sample_buffer_summary(2, 1);
        let ref_a = a.resource_ref;
        let ref_b = b.resource_ref;
        cache.apply_summaries(&[a, b]);
        cache.apply_runtime_buffer_payloads(&[WireRuntimeBufferPayload {
            resource_ref: ref_b,
            revision: Revision::new(1),
            metadata: WireRuntimeBufferMetadataPayload::Raw,
            bytes: Vec::from([7u8, 8]),
        }]);

        // Re-apply only the changed summary for ref_a.
        cache.apply_summaries(&[sample_buffer_summary(1, 2)]);

        assert_eq!(
            cache.summary(ref_a).map(|s| s.revision),
            Some(Revision::new(2))
        );
        assert!(cache.summary(ref_b).is_some());
        assert_eq!(
            cache.runtime_buffer_bytes(ref_b),
            Some(Vec::from([7u8, 8]).as_slice())
        );
    }

    #[test]
    fn project_resource_cache_prune_to_membership_drops_absent_refs() {
        let mut cache = ClientResourceCache::new();
        let a = sample_buffer_summary(1, 1);
        let b = sample_buffer_summary(2, 1);
        let ref_a = a.resource_ref;
        let ref_b = b.resource_ref;
        cache.apply_summaries(&[a, b]);
        cache.apply_runtime_buffer_payloads(&[WireRuntimeBufferPayload {
            resource_ref: ref_b,
            revision: Revision::new(1),
            metadata: WireRuntimeBufferMetadataPayload::Raw,
            bytes: Vec::from([7u8, 8]),
        }]);

        // Membership lists only ref_a; ref_b was removed and must be pruned.
        cache.prune_to_membership(&[ref_a]);

        assert!(cache.summary(ref_a).is_some());
        assert!(cache.summary(ref_b).is_none());
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
