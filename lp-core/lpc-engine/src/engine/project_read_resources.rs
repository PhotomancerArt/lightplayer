//! Resource read helpers.

use alloc::vec::Vec;

use lpc_model::{ResourceRef, Revision};
use lpc_wire::{
    ReadLevel, ResourcePayloadRead, ResourceReadQuery, ResourceReadResult, WireChannelSampleFormat,
    WireColorLayout, WireResourceAvailability, WireResourceKindSummary,
    WireResourceMetadataSummary, WireResourceSummary, WireRuntimeBufferKind,
    WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload, WireTextureFormat,
};

use crate::resource::{
    RuntimeBuffer, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeChannelSampleFormat, RuntimeColorLayout, RuntimeTextureFormat,
};

use super::Engine;

impl Engine {
    pub(super) fn read_project_resources(
        &self,
        since: Option<Revision>,
        query: ResourceReadQuery,
    ) -> ResourceReadResult {
        // `None` means a fresh read; `0` is the bulk-sync guard revision (G2). An item is
        // included iff `changed_at > since`, except at `since == 0` where every live item is
        // included so default-stamped (revision-0) buffers are not lost on a first read.
        let since = since.unwrap_or_default();

        let summaries = match query.level {
            ReadLevel::Ids | ReadLevel::Summary | ReadLevel::Detail => {
                self.resource_summaries(since)
            }
        };
        let runtime_buffer_payloads = self.resource_payloads(since, query.payloads);

        // Membership: when the store's id set changed after `since`, send the full current ref
        // set so the client can prune buffers removed since it synced (G4/G7). `since == 0` is a
        // fresh read that already receives every summary, so no membership list is needed.
        let ids_revision = self.runtime_buffers().ids_revision();
        let membership =
            (since.as_i64() != 0 && ids_revision > since).then(|| self.resource_membership());

        ResourceReadResult {
            level: query.level,
            summaries,
            runtime_buffer_payloads,
            membership,
        }
    }

    fn resource_summaries(&self, since: Revision) -> Vec<WireResourceSummary> {
        self.runtime_buffers()
            .iter()
            .filter(|(_, buffer)| include_since(buffer.changed_at(), since))
            .map(|(id, buffer)| {
                runtime_buffer_summary(
                    id,
                    self.runtime_buffers().owner(id),
                    buffer.changed_at(),
                    buffer.value(),
                )
            })
            .collect()
    }

    fn resource_membership(&self) -> Vec<ResourceRef> {
        self.runtime_buffers()
            .iter()
            .map(|(id, _)| ResourceRef::runtime_buffer(id))
            .collect()
    }

    fn resource_payloads(
        &self,
        since: Revision,
        payloads: ResourcePayloadRead,
    ) -> Vec<WireRuntimeBufferPayload> {
        match payloads {
            ResourcePayloadRead::None => Vec::new(),
            // Non-targeted payload requests are revision-gated like summaries.
            ResourcePayloadRead::All => self
                .runtime_buffers()
                .iter()
                .filter(|(_, buffer)| include_since(buffer.changed_at(), since))
                .map(|(id, buffer)| runtime_buffer_payload(id, buffer.changed_at(), buffer.value()))
                .collect(),
            // Explicit `ByRefs` requests are targeted fetches and bypass `since` (Q3).
            ResourcePayloadRead::ByRefs(refs) => refs
                .into_iter()
                .filter_map(|resource_ref| {
                    let id = runtime_buffer_id_from_ref(resource_ref)?;
                    let buffer = self.runtime_buffers().get(id)?;
                    Some(runtime_buffer_payload(
                        id,
                        buffer.changed_at(),
                        buffer.value(),
                    ))
                })
                .collect(),
        }
    }
}

/// Whether an item stamped at `changed_at` is included by a read since `since`.
///
/// Inclusion is strictly `changed_at > since`, with a bulk-sync guard at `since == 0` that
/// includes every live item (G2), so revision-0 defaults survive a fresh read.
fn include_since(changed_at: Revision, since: Revision) -> bool {
    since.as_i64() == 0 || changed_at > since
}

fn runtime_buffer_id_from_ref(resource_ref: ResourceRef) -> Option<RuntimeBufferId> {
    if resource_ref != ResourceRef::runtime_buffer(RuntimeBufferId::new(resource_ref.id)) {
        return None;
    }
    Some(RuntimeBufferId::new(resource_ref.id))
}

fn runtime_buffer_summary(
    id: RuntimeBufferId,
    owner: Option<lpc_model::NodeId>,
    revision: lpc_model::Revision,
    buffer: &RuntimeBuffer,
) -> WireResourceSummary {
    WireResourceSummary {
        resource_ref: ResourceRef::runtime_buffer(id),
        owner,
        revision,
        kind: WireResourceKindSummary::RuntimeBuffer(runtime_buffer_kind(&buffer.kind)),
        metadata: runtime_buffer_metadata_summary(&buffer.metadata),
        byte_length_hint: Some(buffer.bytes.len() as u64),
        availability: WireResourceAvailability::Available,
    }
}

fn runtime_buffer_payload(
    id: RuntimeBufferId,
    revision: lpc_model::Revision,
    buffer: &RuntimeBuffer,
) -> WireRuntimeBufferPayload {
    WireRuntimeBufferPayload {
        resource_ref: ResourceRef::runtime_buffer(id),
        revision,
        metadata: runtime_buffer_metadata_payload(&buffer.metadata),
        bytes: buffer.bytes.clone(),
    }
}

fn runtime_buffer_kind(kind: &RuntimeBufferKind) -> WireRuntimeBufferKind {
    match kind {
        RuntimeBufferKind::Texture => WireRuntimeBufferKind::Texture,
        RuntimeBufferKind::FixtureColors => WireRuntimeBufferKind::FixtureColors,
        RuntimeBufferKind::OutputChannels => WireRuntimeBufferKind::OutputChannels,
        RuntimeBufferKind::Raw => WireRuntimeBufferKind::Raw,
    }
}

fn runtime_buffer_metadata_summary(
    metadata: &RuntimeBufferMetadata,
) -> WireResourceMetadataSummary {
    match metadata {
        RuntimeBufferMetadata::Texture {
            width,
            height,
            format,
        } => WireResourceMetadataSummary::Texture {
            width: *width,
            height: *height,
            format: texture_format(*format),
        },
        RuntimeBufferMetadata::FixtureColors { channels, layout } => {
            WireResourceMetadataSummary::FixtureColors {
                channels: *channels,
                layout: color_layout(*layout),
            }
        }
        RuntimeBufferMetadata::OutputChannels {
            channels,
            sample_format,
        } => WireResourceMetadataSummary::OutputChannels {
            channels: *channels,
            sample_format: channel_sample_format(*sample_format),
        },
        RuntimeBufferMetadata::Raw => WireResourceMetadataSummary::Raw,
    }
}

fn runtime_buffer_metadata_payload(
    metadata: &RuntimeBufferMetadata,
) -> WireRuntimeBufferMetadataPayload {
    match metadata {
        RuntimeBufferMetadata::Texture {
            width,
            height,
            format,
        } => WireRuntimeBufferMetadataPayload::Texture {
            width: *width,
            height: *height,
            format: texture_format(*format),
        },
        RuntimeBufferMetadata::FixtureColors { channels, layout } => {
            WireRuntimeBufferMetadataPayload::FixtureColors {
                channels: *channels,
                layout: color_layout(*layout),
            }
        }
        RuntimeBufferMetadata::OutputChannels {
            channels,
            sample_format,
        } => WireRuntimeBufferMetadataPayload::OutputChannels {
            channels: *channels,
            sample_format: channel_sample_format(*sample_format),
        },
        RuntimeBufferMetadata::Raw => WireRuntimeBufferMetadataPayload::Raw,
    }
}

fn texture_format(format: RuntimeTextureFormat) -> WireTextureFormat {
    match format {
        RuntimeTextureFormat::Rgba16 => WireTextureFormat::Rgba16,
        RuntimeTextureFormat::Rgb8 => WireTextureFormat::Srgb8,
    }
}

fn color_layout(layout: RuntimeColorLayout) -> WireColorLayout {
    match layout {
        RuntimeColorLayout::Rgb8 => WireColorLayout::Rgb8,
    }
}

fn channel_sample_format(format: RuntimeChannelSampleFormat) -> WireChannelSampleFormat {
    match format {
        RuntimeChannelSampleFormat::U8 => WireChannelSampleFormat::U8,
        RuntimeChannelSampleFormat::U16 => WireChannelSampleFormat::U16,
    }
}
