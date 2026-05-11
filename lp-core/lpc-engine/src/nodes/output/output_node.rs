//! Output demand root: resolves a control product, renders into output-owned samples, and exposes
//! the dirty runtime buffer flushed by [`crate::EngineServices`].

use alloc::vec::Vec;

use lpc_model::{Revision, SlotPath, WithRevision};

use crate::control_product::{ControlRenderRequest, ControlRenderTarget, ControlSampleFormat};
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeResourceInitContext, NodeRuntime, PressureLevel,
    TickContext,
};
use crate::resolver::QueryKey;
use crate::runtime_buffer::{
    RuntimeBuffer, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeChannelSampleFormat,
};

/// Output node that owns the materialized control sample buffer.
pub struct OutputNode {
    channel_buffer_id: Option<RuntimeBufferId>,
    control_samples: Vec<u16>,
}

impl OutputNode {
    #[must_use]
    pub fn new() -> Self {
        Self {
            channel_buffer_id: None,
            control_samples: Vec::new(),
        }
    }

    pub fn channel_buffer_id(&self) -> Option<RuntimeBufferId> {
        self.channel_buffer_id
    }
}

pub fn output_input_path() -> SlotPath {
    SlotPath::parse("input").expect("output input path")
}

impl NodeRuntime for OutputNode {
    fn init_resources(&mut self, ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        if self.channel_buffer_id.is_some() {
            return Ok(());
        }
        let id = ctx.insert_runtime_buffer(WithRevision::new(
            Revision::default(),
            RuntimeBuffer::output_channels_u16(0, Vec::new()),
        ));
        self.channel_buffer_id = Some(id);
        Ok(())
    }

    fn runtime_output_sink_buffer_id(&self) -> Option<RuntimeBufferId> {
        self.channel_buffer_id
    }

    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let prod = ctx
            .resolve(QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot: output_input_path(),
            })
            .map_err(|e| NodeError::msg(alloc::format!("resolve output input: {}", e.message)))?;

        let control = match prod.product.get() {
            lpc_model::LpValue::Product(lpc_model::ProductRef::Control(product)) => *product,
            _ => return Err(NodeError::msg("output expected control product from input")),
        };

        let extent = control.preferred_extent();
        let sample_count = extent.sample_count() as usize;
        self.control_samples.resize(sample_count, 0);
        let request = ControlRenderRequest::unorm16(extent);
        let target = ControlRenderTarget::new(
            extent,
            ControlSampleFormat::Unorm16,
            &mut self.control_samples,
        );
        let _layout = ctx.render_control(control, &request, target)?;

        let buffer_id = self
            .channel_buffer_id
            .ok_or_else(|| NodeError::msg("output channel buffer not initialized"))?;
        ctx.with_runtime_buffer_mut(buffer_id, ctx.revision(), |buffer| {
            buffer.kind = RuntimeBufferKind::OutputChannels;
            buffer.metadata = RuntimeBufferMetadata::OutputChannels {
                channels: (self.control_samples.len() / 3) as u32,
                sample_format: RuntimeChannelSampleFormat::U16,
            };
            buffer
                .bytes
                .resize(self.control_samples.len().saturating_mul(2), 0);
            for (chunk, sample) in buffer
                .bytes
                .chunks_exact_mut(2)
                .zip(self.control_samples.iter())
            {
                chunk.copy_from_slice(&sample.to_le_bytes());
            }
            Ok(())
        })?;
        Ok(())
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        Ok(())
    }
}
