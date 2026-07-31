//! Output demand root: resolves a control product, renders into output-owned samples, and exposes
//! the dirty runtime buffer flushed by [`crate::EngineServices`].

use alloc::vec::Vec;

use lpc_model::{Revision, SlotPath, WithRevision};

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeResourceInitContext, NodeRuntime, PressureLevel,
    TickContext,
};
use crate::products::control::{ControlRenderRequest, ControlRenderTarget, ControlSampleFormat};
use crate::resource::{
    RuntimeBuffer, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeChannelSampleFormat,
};

/// How long a solid test pattern holds when the command carries no explicit TTL.
///
/// A command channel is fire-and-forget: if the client that lit the pattern
/// goes away, the pattern must expire on its own rather than strand an
/// installation in test mode.
const DEFAULT_TEST_PATTERN_TTL_MS: u32 = 2000;

/// A live solid test pattern: the wire color plus the frame time it lapses at.
///
/// Only the `Solid` wire variant is ever stored — `Clear` is the absence of
/// this state, not a value of it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TestPattern {
    /// The commanded 8-bit color, kept in wire units so a re-send with the
    /// same color compares equal without round-trip scaling.
    rgb: [u8; 3],
    /// Engine frame time (seconds) at which the pattern lapses. Same clock as
    /// [`TickContext::time_seconds`] and `handle_command`'s `time_s`.
    expires_at_s: f32,
}

/// Output node that owns the materialized control sample buffer.
pub struct OutputNode {
    channel_buffer_id: Option<RuntimeBufferId>,
    control_samples: Vec<u16>,
    test_pattern: Option<TestPattern>,
}

impl OutputNode {
    #[must_use]
    pub fn new() -> Self {
        Self {
            channel_buffer_id: None,
            control_samples: Vec::new(),
            test_pattern: None,
        }
    }

    pub fn channel_buffer_id(&self) -> Option<RuntimeBufferId> {
        self.channel_buffer_id
    }

    /// Overwrite every established sample with one color, in place.
    ///
    /// Keeps the LAST-ESTABLISHED sample count: a test pattern never resizes
    /// the channel extent, it only repaints it, so the flush path sees exactly
    /// the buffer shape the real render produced.
    fn fill_solid(&mut self, rgb: [u8; 3]) {
        // 8-bit unorm to 16-bit unorm: 0..=255 maps onto 0..=65535 exactly.
        let rgb16 = [
            u16::from(rgb[0]) * 257,
            u16::from(rgb[1]) * 257,
            u16::from(rgb[2]) * 257,
        ];
        for (index, sample) in self.control_samples.iter_mut().enumerate() {
            *sample = rgb16[index % 3];
        }
    }

    /// Copy `control_samples` into the runtime buffer and mark it dirty for this frame.
    ///
    /// Shared by the render path and the test-pattern bypass so both publish
    /// byte-identical buffer kind, metadata, and revision.
    fn publish_channel_buffer(&self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
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
        })
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

    /// Put this output into (or out of) test-pattern mode.
    ///
    /// Queue-in-command / apply-in-consume: this only records intent against
    /// the command's frame time; the bypass itself happens in
    /// [`NodeRuntime::consume`].
    fn handle_command(
        &mut self,
        command: &lpc_wire::WireNodeCommand,
        time_s: f32,
    ) -> Result<(), NodeError> {
        match command {
            lpc_wire::WireNodeCommand::OutputTestPattern { pattern, ttl_ms } => match pattern {
                // Idempotent, and deliberately accepted with nothing active:
                // a "stop testing" click must never fail just because the
                // pattern already lapsed on its own.
                lpc_wire::WireOutputTestPattern::Clear => {
                    self.test_pattern = None;
                    Ok(())
                }
                lpc_wire::WireOutputTestPattern::Solid { r, g, b } => {
                    if self.control_samples.is_empty() {
                        return Err(NodeError::msg(
                            "output has no established channel extent yet",
                        ));
                    }
                    let ttl_ms = if *ttl_ms == 0 {
                        DEFAULT_TEST_PATTERN_TTL_MS
                    } else {
                        *ttl_ms
                    };
                    // Replace AND renew atomically: a re-send is how the
                    // client holds a pattern alive.
                    self.test_pattern = Some(TestPattern {
                        rgb: [*r, *g, *b],
                        expires_at_s: time_s + (ttl_ms as f32 / 1000.0),
                    });
                    Ok(())
                }
            },
            _ => Err(NodeError::msg("output does not support this command")),
        }
    }

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        if let Some(pattern) = self.test_pattern {
            if ctx.time_seconds() < pattern.expires_at_s {
                // Bypass, not overwrite: the graph resolve is skipped
                // entirely, so upstream demand from this root stops while the
                // pattern holds. Other outputs are unaffected.
                self.fill_solid(pattern.rgb);
                return self.publish_channel_buffer(ctx);
            }
            // Lapsed. Drop it and render the graph THIS frame — falling
            // through avoids a black frame at the expiry boundary.
            self.test_pattern = None;
        }

        let prod = ctx
            .resolve(QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot: output_input_path(),
            })
            .map_err(|e| NodeError::msg(alloc::format!("resolve output input: {}", e.message)))?;

        let control = match prod
            .value_leaf()
            .ok_or_else(|| {
                NodeError::msg("output input resolved to aggregate data, expected control product")
            })?
            .get()
        {
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

        self.publish_channel_buffer(ctx)
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

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::string::{String, ToString};
    use alloc::vec;

    use lpc_model::{
        ControlExtent, ControlProduct, ControlSampleLayout, LpValue, NodeId, ProductRef,
        SlotShapeRegistry,
    };
    use lpc_wire::{WireButtonEvent, WireNodeCommand, WireOutputTestPattern};

    use crate::dataflow::resolver::{Production, ProductionSource, ResolveError, TickResolver};
    use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
    use crate::resource::RuntimeBuffer;

    const BUFFER: RuntimeBufferId = RuntimeBufferId::new(1);
    /// One RGB lamp: the smallest extent that still exercises the triplet layout.
    const EXTENT: ControlExtent = ControlExtent::new(1, 3);

    fn node_id() -> NodeId {
        NodeId::new(1)
    }

    /// Minimal [`TickResolver`] standing in for the engine's session bridge.
    ///
    /// It counts `resolve` calls (that is how "the graph was consulted" is
    /// asserted) and paints whatever `graph_color` currently is, so a color
    /// change that does NOT reach the buffer proves the bypass.
    struct FakeResolver {
        buffer: RuntimeBuffer,
        buffer_frame: Revision,
        resolve_calls: u32,
        graph_color: [u16; 3],
    }

    impl FakeResolver {
        fn new() -> Self {
            Self {
                buffer: RuntimeBuffer::output_channels_u16(0, Vec::new()),
                buffer_frame: Revision::default(),
                resolve_calls: 0,
                graph_color: [1000, 2000, 3000],
            }
        }

        /// The published buffer decoded back into u16 samples.
        fn published_samples(&self) -> Vec<u16> {
            self.buffer
                .bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect()
        }
    }

    impl TickResolver for FakeResolver {
        fn resolve(&mut self, _query: QueryKey) -> Result<Production, ResolveError> {
            self.resolve_calls += 1;
            Ok(Production::leaf(
                WithRevision::new(
                    Revision::new(1),
                    LpValue::Product(ProductRef::Control(ControlProduct::new(
                        node_id(),
                        0,
                        EXTENT,
                    ))),
                ),
                ProductionSource::Literal,
            ))
        }

        fn publish_produced_slot(
            &mut self,
            _node: NodeId,
            _slot: SlotPath,
            _production: Production,
        ) -> Result<(), ResolveError> {
            Ok(())
        }

        fn render_texture(
            &mut self,
            _product: VisualProduct,
            _request: &RenderTextureRequest,
        ) -> Result<TextureRenderProduct, ResolveError> {
            Err(ResolveError::new(String::from(
                "fake resolver renders no textures",
            )))
        }

        fn render_control(
            &mut self,
            _product: ControlProduct,
            _request: &ControlRenderRequest,
            target: ControlRenderTarget<'_>,
        ) -> Result<ControlSampleLayout, ResolveError> {
            for (index, sample) in target.samples.iter_mut().enumerate() {
                *sample = self.graph_color[index % 3];
            }
            Ok(ControlSampleLayout::empty())
        }

        fn runtime_buffer_mut(
            &mut self,
            id: RuntimeBufferId,
            frame: Revision,
        ) -> Result<&mut RuntimeBuffer, ResolveError> {
            if id != BUFFER {
                return Err(ResolveError::new(String::from("unknown runtime buffer")));
            }
            self.buffer_frame = frame;
            Ok(&mut self.buffer)
        }
    }

    /// An output whose buffer id is already assigned (stands in for `init_resources`).
    fn output_node() -> OutputNode {
        let mut node = OutputNode::new();
        node.channel_buffer_id = Some(BUFFER);
        node
    }

    /// Run one `consume` at `time_s`, as the engine's tick would.
    fn consume_at(
        node: &mut OutputNode,
        resolver: &mut FakeResolver,
        frame: Revision,
        time_s: f32,
    ) -> Result<(), NodeError> {
        let shapes = SlotShapeRegistry::default();
        let mut ctx = TickContext::with_render_services(
            node_id(),
            frame,
            resolver,
            &shapes,
            None,
            None,
            time_s,
        );
        node.consume(&mut ctx)
    }

    fn solid(r: u8, g: u8, b: u8, ttl_ms: u32) -> WireNodeCommand {
        WireNodeCommand::OutputTestPattern {
            pattern: WireOutputTestPattern::Solid { r, g, b },
            ttl_ms,
        }
    }

    fn clear() -> WireNodeCommand {
        WireNodeCommand::OutputTestPattern {
            pattern: WireOutputTestPattern::Clear,
            ttl_ms: 0,
        }
    }

    #[test]
    fn solid_is_rejected_before_any_frame_has_been_rendered() {
        let mut node = output_node();

        let err = node
            .handle_command(&solid(255, 0, 0, 1000), 0.0)
            .expect_err("no established extent yet");

        assert!(err.to_string().contains("channel extent"), "{err}");
        assert_eq!(node.test_pattern, None);
    }

    #[test]
    fn solid_pattern_replaces_graph_output_at_the_established_sample_count() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();

        consume_at(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");
        assert_eq!(resolver.published_samples(), vec![1000, 2000, 3000]);

        node.handle_command(&solid(255, 128, 0, 1000), 0.0)
            .expect("accepted after a rendered frame");

        // A graph color change from here on must NOT reach the buffer.
        resolver.graph_color = [4000, 5000, 6000];
        let before = resolver.resolve_calls;
        consume_at(&mut node, &mut resolver, Revision::new(2), 0.1).expect("pattern frame");

        assert_eq!(
            resolver.published_samples(),
            vec![255 * 257, 128 * 257, 0],
            "buffer should hold the solid color as u16 unorm triplets",
        );
        assert_eq!(
            resolver.resolve_calls, before,
            "the graph resolve must be skipped entirely while a pattern is active",
        );
        assert_eq!(
            resolver.buffer.metadata,
            RuntimeBufferMetadata::OutputChannels {
                channels: 1,
                sample_format: RuntimeChannelSampleFormat::U16,
            },
        );
        assert_eq!(resolver.buffer.kind, RuntimeBufferKind::OutputChannels);
        assert_eq!(
            resolver.buffer_frame,
            Revision::new(2),
            "the bypass must mark the buffer dirty for this frame, like the render path",
        );
    }

    #[test]
    fn resending_a_different_color_replaces_the_active_pattern() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        node.handle_command(&solid(255, 0, 0, 1000), 0.0)
            .expect("first accepted");
        node.handle_command(&solid(0, 0, 255, 1000), 0.5)
            .expect("second accepted");

        let pattern = node.test_pattern.expect("pattern active");
        assert_eq!(pattern.rgb, [0, 0, 255]);
        assert!((pattern.expires_at_s - 1.5).abs() < 1e-6, "{pattern:?}");
    }

    #[test]
    fn resending_the_same_color_renews_the_expiry() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        node.handle_command(&solid(255, 0, 0, 1000), 0.0)
            .expect("first accepted");
        let first = node.test_pattern.expect("pattern active").expires_at_s;
        node.handle_command(&solid(255, 0, 0, 1000), 0.75)
            .expect("renewal accepted");
        let renewed = node.test_pattern.expect("pattern active");

        assert_eq!(renewed.rgb, [255, 0, 0]);
        assert!(
            renewed.expires_at_s > first,
            "renewal should push the expiry out: {first} -> {}",
            renewed.expires_at_s,
        );
    }

    #[test]
    fn a_zero_ttl_falls_back_to_the_default_hold() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        node.handle_command(&solid(255, 0, 0, 0), 1.0)
            .expect("accepted");

        let pattern = node.test_pattern.expect("pattern active");
        let expected = 1.0 + (DEFAULT_TEST_PATTERN_TTL_MS as f32 / 1000.0);
        assert!(
            (pattern.expires_at_s - expected).abs() < 1e-6,
            "{pattern:?}"
        );
    }

    #[test]
    fn an_expired_pattern_renders_the_graph_again_on_the_same_frame() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        node.handle_command(&solid(255, 0, 0, 1000), 0.0)
            .expect("accepted");
        resolver.graph_color = [4000, 5000, 6000];

        // t = 2.0 is past the 1.0 s expiry: no black frame, straight back to the graph.
        consume_at(&mut node, &mut resolver, Revision::new(2), 2.0).expect("post-expiry frame");

        assert_eq!(resolver.published_samples(), vec![4000, 5000, 6000]);
        assert_eq!(node.test_pattern, None, "the lapsed pattern is dropped");
    }

    #[test]
    fn clear_returns_to_graph_output_immediately() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        node.handle_command(&solid(255, 0, 0, 10_000), 0.0)
            .expect("accepted");
        resolver.graph_color = [4000, 5000, 6000];
        node.handle_command(&clear(), 0.1).expect("clear accepted");

        // Still well inside the TTL: only the Clear can explain graph output.
        consume_at(&mut node, &mut resolver, Revision::new(2), 0.2).expect("post-clear frame");

        assert_eq!(node.test_pattern, None);
        assert_eq!(resolver.published_samples(), vec![4000, 5000, 6000]);
    }

    #[test]
    fn clear_is_accepted_with_no_pattern_active() {
        let mut node = output_node();

        node.handle_command(&clear(), 0.0)
            .expect("clear is idempotent, even before any frame");

        assert_eq!(node.test_pattern, None);
    }

    #[test]
    fn unrelated_commands_are_rejected() {
        let mut node = output_node();

        assert!(
            node.handle_command(&WireNodeCommand::PlaylistActivateEntry { entry: 1 }, 0.0)
                .is_err(),
        );
        assert!(
            node.handle_command(
                &WireNodeCommand::ButtonEvent {
                    event: WireButtonEvent::Click,
                },
                0.0,
            )
            .is_err(),
        );
    }
}
