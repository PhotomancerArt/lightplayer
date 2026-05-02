//! M4.1 resource summaries and explicit buffer / render-product payload projection.

use alloc::vec::Vec;

use alloc::string::String;

use lpc_model::{
    FrameId,
    resource::{RenderProductId, ResourceRef, RuntimeBufferId},
};
use lpc_wire::{
    RenderProductPayloadRequest, RenderProductPayloadSpecifier, ResourceSummarySpecifier,
    RuntimeBufferPayloadSpecifier, WireChannelSampleFormat, WireColorLayout, WireRenderProductKind,
    WireRenderProductPayload, WireResourceAvailability, WireResourceKindSummary,
    WireResourceMetadataSummary, WireResourceSummary, WireRuntimeBufferKind,
    WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload, WireTextureFormat,
};

use crate::render_product::{RenderProduct, RenderProductMaterializeError, TextureRenderProduct};
use crate::runtime_buffer::{
    RuntimeBuffer, RuntimeBufferMetadata, RuntimeChannelSampleFormat as RChFmt, RuntimeColorLayout,
    RuntimeTextureFormat,
};

#[derive(Clone)]
pub(crate) enum BufferPayloadInterest {
    All,
    Ids(Vec<RuntimeBufferId>),
}

impl BufferPayloadInterest {
    fn wants(&self, id: RuntimeBufferId) -> bool {
        match self {
            BufferPayloadInterest::All => true,
            BufferPayloadInterest::Ids(ids) => ids.iter().any(|x| *x == id),
        }
    }
}

pub(crate) fn buffer_payload_interest(
    spec: &RuntimeBufferPayloadSpecifier,
) -> Option<BufferPayloadInterest> {
    match spec {
        RuntimeBufferPayloadSpecifier::None => None,
        RuntimeBufferPayloadSpecifier::All => Some(BufferPayloadInterest::All),
        RuntimeBufferPayloadSpecifier::ByIds(ids) => Some(BufferPayloadInterest::Ids(ids.clone())),
    }
}

#[derive(Clone)]
pub(crate) enum RenderPayloadInterest {
    All,
    Ids(Vec<RenderProductId>),
}

impl RenderPayloadInterest {
    fn wants(&self, id: RenderProductId) -> bool {
        match self {
            RenderPayloadInterest::All => true,
            RenderPayloadInterest::Ids(ids) => ids.iter().any(|x| *x == id),
        }
    }
}

pub(crate) fn render_payload_interest(
    req: &RenderProductPayloadRequest,
) -> Option<RenderPayloadInterest> {
    match req.specifier {
        RenderProductPayloadSpecifier::None => None,
        RenderProductPayloadSpecifier::All => Some(RenderPayloadInterest::All),
        RenderProductPayloadSpecifier::ByIds(ref ids) => {
            Some(RenderPayloadInterest::Ids(ids.clone()))
        }
    }
}

fn resource_changed_since(since_frame: FrameId, changed: FrameId) -> bool {
    changed.as_i64() > since_frame.as_i64() || since_frame == FrameId::default()
}

pub(crate) fn summarize_runtime_buffers_if_requested(
    _since_frame: FrameId,
    spec: ResourceSummarySpecifier,
    buffers: &crate::runtime_buffer::RuntimeBufferStore,
    out: &mut Vec<WireResourceSummary>,
) {
    match spec {
        ResourceSummarySpecifier::None | ResourceSummarySpecifier::RenderProducts => return,
        ResourceSummarySpecifier::RuntimeBuffers | ResourceSummarySpecifier::All => {}
    }

    for (id, ver) in buffers.iter() {
        push_buffer_summary(out, ver.value(), ver.changed_frame(), id);
    }
}

fn push_buffer_summary(
    out: &mut Vec<WireResourceSummary>,
    buf: &RuntimeBuffer,
    changed: FrameId,
    id: RuntimeBufferId,
) {
    let (kind, meta, avail) = match wire_kind_and_meta_for_runtime_buffer(buf) {
        Ok(x) => x,
        Err(reason) => {
            out.push(WireResourceSummary {
                resource_ref: ResourceRef::runtime_buffer(id),
                changed_frame: changed,
                kind: WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::Raw),
                metadata: WireResourceMetadataSummary::Raw,
                byte_length_hint: None,
                availability: WireResourceAvailability::Error(String::from(reason)),
            });
            return;
        }
    };
    out.push(WireResourceSummary {
        resource_ref: ResourceRef::runtime_buffer(id),
        changed_frame: changed,
        kind: WireResourceKindSummary::RuntimeBuffer(kind),
        metadata: meta,
        byte_length_hint: Some(buf.bytes.len() as u64),
        availability: avail,
    });
}

fn wire_kind_and_meta_for_runtime_buffer(
    buf: &RuntimeBuffer,
) -> Result<
    (
        WireRuntimeBufferKind,
        WireResourceMetadataSummary,
        WireResourceAvailability,
    ),
    &'static str,
> {
    use crate::runtime_buffer::RuntimeBufferKind;
    let kind = match buf.kind {
        RuntimeBufferKind::Texture => WireRuntimeBufferKind::Texture,
        RuntimeBufferKind::FixtureColors => WireRuntimeBufferKind::FixtureColors,
        RuntimeBufferKind::OutputChannels => WireRuntimeBufferKind::OutputChannels,
        RuntimeBufferKind::Raw => WireRuntimeBufferKind::Raw,
    };
    let (meta, avail) = match &buf.metadata {
        RuntimeBufferMetadata::Raw => (
            WireResourceMetadataSummary::Raw,
            WireResourceAvailability::Available,
        ),
        RuntimeBufferMetadata::Texture {
            width,
            height,
            format,
        } => {
            let wf = runtime_texture_wire_format(*format)?;
            (
                WireResourceMetadataSummary::Texture {
                    width: *width,
                    height: *height,
                    format: wf,
                },
                WireResourceAvailability::Available,
            )
        }
        RuntimeBufferMetadata::FixtureColors { channels, layout } => (
            WireResourceMetadataSummary::FixtureColors {
                channels: *channels,
                layout: fixture_color_wire_layout(*layout)?,
            },
            WireResourceAvailability::Available,
        ),
        RuntimeBufferMetadata::OutputChannels {
            channels,
            sample_format,
        } => (
            WireResourceMetadataSummary::OutputChannels {
                channels: *channels,
                sample_format: output_sample_wire_format(*sample_format)?,
            },
            WireResourceAvailability::Available,
        ),
    };
    Ok((kind, meta, avail))
}

fn runtime_texture_wire_format(f: RuntimeTextureFormat) -> Result<WireTextureFormat, &'static str> {
    Ok(match f {
        RuntimeTextureFormat::Rgba16 => WireTextureFormat::Rgba16,
        RuntimeTextureFormat::Rgb8 => WireTextureFormat::Rgb8,
    })
}

fn fixture_color_wire_layout(l: RuntimeColorLayout) -> Result<WireColorLayout, &'static str> {
    match l {
        RuntimeColorLayout::Rgb8 => Ok(WireColorLayout::Rgb8),
    }
}

fn output_sample_wire_format(f: RChFmt) -> Result<WireChannelSampleFormat, &'static str> {
    match f {
        RChFmt::U8 => Ok(WireChannelSampleFormat::U8),
        RChFmt::U16 => Ok(WireChannelSampleFormat::U16),
    }
}

pub(crate) fn summarize_render_products_if_requested(
    _since_frame: FrameId,
    spec: ResourceSummarySpecifier,
    products: &crate::render_product::RenderProductStore,
    out: &mut Vec<WireResourceSummary>,
) {
    match spec {
        ResourceSummarySpecifier::None | ResourceSummarySpecifier::RuntimeBuffers => return,
        ResourceSummarySpecifier::RenderProducts | ResourceSummarySpecifier::All => {}
    }

    for id in products.ids() {
        let changed = products.changed_frame(id);
        push_render_product_summary(out, products.get(id), id, changed);
    }
}

fn push_render_product_summary(
    out: &mut Vec<WireResourceSummary>,
    product: Option<&dyn RenderProduct>,
    id: RenderProductId,
    changed: FrameId,
) {
    let Some(product) = product else {
        return;
    };

    let Some(tex) = product.as_any().downcast_ref::<TextureRenderProduct>() else {
        out.push(WireResourceSummary {
            resource_ref: ResourceRef::render_product(id),
            changed_frame: changed,
            kind: WireResourceKindSummary::RenderProduct(WireRenderProductKind::Texture),
            metadata: WireResourceMetadataSummary::Texture {
                width: 0,
                height: 0,
                format: WireTextureFormat::Rgba16,
            },
            byte_length_hint: None,
            availability: WireResourceAvailability::Pending,
        });
        return;
    };

    let fmt = tex.storage_format();
    let wire_fmt_ok = fmt == lps_shared::TextureStorageFormat::Rgba16Unorm;

    let (avail, wf) = if wire_fmt_ok {
        (
            WireResourceAvailability::Available,
            WireTextureFormat::Rgba16,
        )
    } else {
        (WireResourceAvailability::Pending, WireTextureFormat::Rgb8)
    };

    out.push(WireResourceSummary {
        resource_ref: ResourceRef::render_product(id),
        changed_frame: changed,
        kind: WireResourceKindSummary::RenderProduct(WireRenderProductKind::Texture),
        metadata: WireResourceMetadataSummary::Texture {
            width: tex.width(),
            height: tex.height(),
            format: wf,
        },
        byte_length_hint: tex.try_raw_bytes().map(|b| b.len() as u64),
        availability: avail,
    });
}

pub(crate) fn runtime_buffer_payloads_for_request(
    since_frame: FrameId,
    interest: &BufferPayloadInterest,
    buffers: &crate::runtime_buffer::RuntimeBufferStore,
    out: &mut Vec<WireRuntimeBufferPayload>,
) {
    for (id, ver) in buffers.iter() {
        if !interest.wants(id) || !resource_changed_since(since_frame, ver.changed_frame()) {
            continue;
        }

        match wire_runtime_buffer_metadata_payload_for_buffer(ver.value()) {
            Ok(meta) => {
                out.push(WireRuntimeBufferPayload {
                    resource_ref: ResourceRef::runtime_buffer(id),
                    changed_frame: ver.changed_frame(),
                    metadata: meta,
                    bytes: ver.value().bytes.clone(),
                });
            }
            Err(_) => {
                out.push(WireRuntimeBufferPayload {
                    resource_ref: ResourceRef::runtime_buffer(id),
                    changed_frame: ver.changed_frame(),
                    metadata: WireRuntimeBufferMetadataPayload::Raw,
                    bytes: ver.value().bytes.clone(),
                });
            }
        }
    }
}

fn wire_runtime_buffer_metadata_payload_for_buffer(
    buf: &RuntimeBuffer,
) -> Result<WireRuntimeBufferMetadataPayload, ()> {
    match &buf.metadata {
        RuntimeBufferMetadata::Raw => Ok(WireRuntimeBufferMetadataPayload::Raw),
        RuntimeBufferMetadata::Texture {
            width,
            height,
            format,
        } => Ok(WireRuntimeBufferMetadataPayload::Texture {
            width: *width,
            height: *height,
            format: runtime_texture_wire_format(*format).map_err(|_| ())?,
        }),
        RuntimeBufferMetadata::FixtureColors { channels, layout } => {
            Ok(WireRuntimeBufferMetadataPayload::FixtureColors {
                channels: *channels,
                layout: fixture_color_wire_layout(*layout).map_err(|_| ())?,
            })
        }
        RuntimeBufferMetadata::OutputChannels {
            channels,
            sample_format,
        } => Ok(WireRuntimeBufferMetadataPayload::OutputChannels {
            channels: *channels,
            sample_format: output_sample_wire_format(*sample_format).map_err(|_| ())?,
        }),
    }
}

pub(crate) fn render_product_payloads_for_request(
    since_frame: FrameId,
    interest: &RenderPayloadInterest,
    products: &crate::render_product::RenderProductStore,
    out: &mut Vec<WireRenderProductPayload>,
) {
    let ids: Vec<_> = products.ids().collect();
    for id in ids {
        if !interest.wants(id) || !resource_changed_since(since_frame, products.changed_frame(id)) {
            continue;
        }
        match products.try_materialize_native_texture_payload(id) {
            Ok((w, h, bytes, _fmt)) => {
                out.push(WireRenderProductPayload {
                    resource_ref: ResourceRef::render_product(id),
                    changed_frame: products.changed_frame(id),
                    width: w,
                    height: h,
                    format: WireTextureFormat::Rgba16,
                    bytes: bytes.to_vec(),
                });
            }
            Err(RenderProductMaterializeError::NotCpuTextureProduct) => {}
            Err(RenderProductMaterializeError::UnsupportedTextureFormatForWire) => {}
            Err(RenderProductMaterializeError::UnknownProduct { .. }) => {}
        }
    }
}
