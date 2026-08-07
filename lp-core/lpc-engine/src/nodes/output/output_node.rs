//! Output demand root: resolves a control product, renders into output-owned samples, and exposes
//! the dirty runtime buffer flushed by [`crate::EngineServices`].

use alloc::vec::Vec;

use lpc_model::{OutputDefView, Revision, SlotPath, WithRevision};

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeResourceInitContext, NodeRuntime, PressureLevel,
    TickContext, err_ctx,
};
use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget, ControlSampleFormat,
};
use crate::resource::{
    RuntimeBuffer, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeChannelSampleFormat,
};

/// The color the `test_pattern` Debug slot paints.
///
/// 25% white on every channel (G2 decision): the slot answers "is this pin
/// wired to that strip?", which needs an unmistakable, graph-independent
/// color — but full white is the maximum-current draw on a long strip, so
/// the pattern stays deliberately dim. If power limits ever become
/// first-class settings, this is the constant a smarter policy replaces.
const TEST_PATTERN_RGB: [u8; 3] = [64, 64, 64];

/// Output node that owns the materialized control sample buffer.
pub struct OutputNode {
    channel_buffer_id: Option<RuntimeBufferId>,
    control_samples: Vec<u16>,
    def_view: Option<OutputDefView>,
    /// Interpretation metadata for the frame currently in the buffer,
    /// latched by the render that produced it.
    ///
    /// The published-frame read hands a client the buffer's bytes verbatim;
    /// these two fields are what make those bytes mean something without a
    /// second render. Both survive the test-pattern bypass on purpose — the
    /// pattern repaints an extent the graph already established, so the
    /// layout and the source product are still the frame's truth.
    published_sample_layout: Option<ControlLayout>,
    published_source_product: Option<ControlProduct>,
}

impl OutputNode {
    #[must_use]
    pub fn new() -> Self {
        Self {
            channel_buffer_id: None,
            control_samples: Vec::new(),
            def_view: None,
            published_sample_layout: None,
            published_source_product: None,
        }
    }

    pub fn channel_buffer_id(&self) -> Option<RuntimeBufferId> {
        self.channel_buffer_id
    }

    fn def_view(&mut self, ctx: &TickContext<'_>) -> Result<&OutputDefView, NodeError> {
        OutputDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(err_ctx("compile output def view"))
    }

    /// Read the `test_pattern` Debug slot from the EFFECTIVE def for this frame.
    ///
    /// Nothing is latched: the override lives in the node's slot data (overlay
    /// on top of the authored base), so the bypass switches on and off purely
    /// by what this read returns.
    ///
    /// "Absent" (an output attached with no project def behind it) reads as
    /// **off** and is logged, never fatal: this slot is a diagnostic, and a
    /// diagnostic must not be able to stop an output pushing pixels. A path
    /// that cannot exist in the `OutputDef` shape is a different thing — a
    /// code bug — and it still fails loudly, out of `def_view`, because
    /// compiling the view is exactly that shape check.
    fn test_pattern_active(&mut self, ctx: &mut TickContext<'_>) -> Result<bool, NodeError> {
        let reader = self.def_view(ctx)?.test_pattern();
        match reader.get(ctx) {
            Ok(active) => Ok(active),
            Err(error) => {
                log::debug!("[output] test_pattern unavailable: {error}");
                Ok(false)
            }
        }
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

    fn runtime_output_sample_layout(&self) -> Option<&ControlLayout> {
        self.published_sample_layout.as_ref()
    }

    fn runtime_output_source_product(&self) -> Option<ControlProduct> {
        self.published_source_product
    }

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        // Bypass, not overwrite: while the Debug slot is on, the graph resolve
        // is skipped entirely, so upstream demand from this root stops. Other
        // outputs are unaffected. The frame the slot goes back to false we fall
        // straight through to the normal resolve below — no black frame.
        //
        // A pattern only ever REPAINTS an extent the graph already established;
        // before the first rendered frame there is nothing to repaint, so that
        // frame renders the graph and establishes it.
        if self.test_pattern_active(ctx)? && !self.control_samples.is_empty() {
            self.fill_solid(TEST_PATTERN_RGB);
            return self.publish_channel_buffer(ctx);
        }

        let prod = ctx
            .resolve(&QueryKey::ConsumedSlot {
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
        let layout = ctx.render_control(control, &request, target)?;
        self.published_sample_layout = Some(layout);
        self.published_source_product = Some(control);

        self.publish_channel_buffer(ctx)
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        // Nothing droppable here, deliberately. This handler used to clear
        // `control_samples` at `High` (the #303 compile window). Measurement
        // on 2026-08-04 showed the output's own `produce` resizes that buffer
        // back to the control extent EARLIER in the same tick than the shader
        // compile runs (compiles happen at render time), so the drop frees
        // nothing at the compile instant and only re-does the allocation.
        // Removed in M6 P4; see
        // `docs/defects/2026-08-04-compile-window-drops-rebuilt-before-compile.md`
        // and the 2026-08-04 amendment to
        // `docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md`.
        // The runtime buffer this node publishes into was never touched here
        // anyway — its lifecycle belongs to the sink registration path.
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

    use crate::dataflow::resolver::{Production, ProductionSource, ResolveError, TickResolver};
    use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
    use crate::resource::RuntimeBuffer;

    const BUFFER: RuntimeBufferId = RuntimeBufferId::new(1);
    /// One RGB lamp: the smallest extent that still exercises the triplet layout.
    const ONE_LAMP: ControlExtent = ControlExtent::new(1, 3);
    /// The white the bypass paints, in the u16 unorm units the buffer carries.
    /// The pattern color as the 16-bit unorm samples `fill_solid` publishes,
    /// derived from the const so the tests track a retune automatically.
    const PATTERN16: u16 = TEST_PATTERN_RGB[0] as u16 * 257;

    fn node_id() -> NodeId {
        NodeId::new(1)
    }

    /// Minimal [`TickResolver`] standing in for the engine's session bridge.
    ///
    /// It counts resolves of the output's `input` slot — that is how "the graph
    /// was consulted" is asserted, never inferred — and paints whatever
    /// `graph_color` currently is, so a color change that does NOT reach the
    /// buffer proves the bypass. `test_pattern` stands in for the effective def:
    /// the field the Studio's Debug section writes.
    struct FakeResolver {
        buffer: RuntimeBuffer,
        buffer_frame: Revision,
        /// Resolves of the output's `input` slot: the graph edge the bypass skips.
        input_resolve_calls: u32,
        /// Calls into the render path: the work the bypass skips.
        render_control_calls: u32,
        /// Effective value of the `test_pattern` Debug slot, read every frame.
        test_pattern: bool,
        /// When false the Debug slot has no def behind it and fails to resolve,
        /// as it does for an output attached outside a loaded project.
        test_pattern_resolvable: bool,
        graph_extent: ControlExtent,
        graph_color: [u16; 3],
    }

    impl FakeResolver {
        fn new() -> Self {
            Self {
                buffer: RuntimeBuffer::output_channels_u16(0, Vec::new()),
                buffer_frame: Revision::default(),
                input_resolve_calls: 0,
                render_control_calls: 0,
                test_pattern: false,
                test_pattern_resolvable: true,
                graph_extent: ONE_LAMP,
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
        fn resolve(&mut self, query: &QueryKey) -> Result<Production, ResolveError> {
            let path = query
                .consumed_slot_path()
                .ok_or_else(|| ResolveError::new(String::from("unexpected query kind")))?;
            match path.to_string().as_str() {
                "test_pattern" if self.test_pattern_resolvable => Ok(Production::leaf(
                    WithRevision::new(Revision::new(1), LpValue::Bool(self.test_pattern)),
                    ProductionSource::Literal,
                )),
                "test_pattern" => Err(ResolveError::new(String::from(
                    "unresolved consumed slot test_pattern",
                ))),
                "input" => {
                    self.input_resolve_calls += 1;
                    Ok(Production::leaf(
                        WithRevision::new(
                            Revision::new(1),
                            LpValue::Product(ProductRef::Control(ControlProduct::new(
                                node_id(),
                                0,
                                self.graph_extent,
                            ))),
                        ),
                        ProductionSource::Literal,
                    ))
                }
                other => Err(ResolveError::new(alloc::format!(
                    "fake resolver has no slot {other}"
                ))),
            }
        }

        fn resolve_static_consumed(
            &mut self,
            node: NodeId,
            path: &'static str,
        ) -> Result<Production, ResolveError> {
            let slot = SlotPath::parse(path)
                .map_err(|e| ResolveError::new(alloc::format!("bad static path: {e}")))?;
            self.resolve(&QueryKey::ConsumedSlot { node, slot })
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
            self.render_control_calls += 1;
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

    /// Run one `consume` at `frame`, as the engine's tick would.
    fn consume_at(
        node: &mut OutputNode,
        resolver: &mut FakeResolver,
        frame: Revision,
    ) -> Result<(), NodeError> {
        let shapes = SlotShapeRegistry::default();
        let mut ctx = TickContext::new(node_id(), frame, resolver, &shapes);
        node.consume(&mut ctx)
    }

    #[test]
    fn a_graph_frame_establishes_the_channel_extent() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        assert_eq!(resolver.published_samples(), vec![1000, 2000, 3000]);
        assert_eq!(resolver.input_resolve_calls, 1);
        assert_eq!(resolver.render_control_calls, 1);
    }

    #[test]
    fn the_test_pattern_bypasses_the_graph_at_the_established_sample_count() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        resolver.test_pattern = true;
        // A graph color change from here on must NOT reach the buffer.
        resolver.graph_color = [4000, 5000, 6000];
        let resolves_before = resolver.input_resolve_calls;
        let renders_before = resolver.render_control_calls;

        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");

        assert_eq!(
            resolver.published_samples(),
            vec![PATTERN16, PATTERN16, PATTERN16],
            "buffer should hold the solid test color as u16 unorm triplets",
        );
        assert_eq!(
            resolver.input_resolve_calls, resolves_before,
            "the graph resolve must be skipped entirely while the pattern is active",
        );
        assert_eq!(
            resolver.render_control_calls, renders_before,
            "the render path must be skipped entirely while the pattern is active",
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
    fn the_test_pattern_holds_across_frames_and_repaints_every_one() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        resolver.test_pattern = true;
        for frame in 2..=4i64 {
            consume_at(&mut node, &mut resolver, Revision::new(frame)).expect("pattern frame");
            assert_eq!(
                resolver.buffer_frame,
                Revision::new(frame),
                "every held frame republishes, so the flush path stays fed",
            );
            assert_eq!(
                resolver.published_samples(),
                vec![PATTERN16, PATTERN16, PATTERN16]
            );
        }

        assert_eq!(
            resolver.input_resolve_calls, 1,
            "only the establishing graph frame ever resolved the input",
        );
        assert_eq!(resolver.render_control_calls, 1);
    }

    #[test]
    fn clearing_the_test_pattern_renders_the_graph_again_on_the_same_frame() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");
        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");

        resolver.graph_color = [4000, 5000, 6000];
        resolver.test_pattern = false;
        consume_at(&mut node, &mut resolver, Revision::new(3)).expect("post-clear frame");

        assert_eq!(
            resolver.published_samples(),
            vec![4000, 5000, 6000],
            "no black frame: the graph renders the very frame the override ends",
        );
        assert_eq!(resolver.input_resolve_calls, 2);
    }

    #[test]
    fn the_def_bool_is_read_every_frame_not_latched() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        // on -> off -> on, with nothing but the effective def changing.
        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");
        assert_eq!(
            resolver.published_samples(),
            vec![PATTERN16, PATTERN16, PATTERN16]
        );

        resolver.test_pattern = false;
        consume_at(&mut node, &mut resolver, Revision::new(3)).expect("graph frame");
        assert_eq!(resolver.published_samples(), vec![1000, 2000, 3000]);

        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(4)).expect("pattern frame");
        assert_eq!(
            resolver.published_samples(),
            vec![PATTERN16, PATTERN16, PATTERN16]
        );

        assert_eq!(
            resolver.input_resolve_calls, 2,
            "exactly the two non-pattern frames consulted the graph",
        );
    }

    #[test]
    fn the_test_pattern_never_resizes_the_channel_extent() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = ControlExtent::new(2, 3);
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");

        assert_eq!(
            resolver.published_samples(),
            vec![PATTERN16; 6],
            "the pattern repaints the last-established extent, it does not resize it",
        );
        assert_eq!(
            resolver.buffer.metadata,
            RuntimeBufferMetadata::OutputChannels {
                channels: 2,
                sample_format: RuntimeChannelSampleFormat::U16,
            },
        );
    }

    #[test]
    fn the_bypass_publishes_the_same_buffer_shape_as_the_render_path() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");
        let graph_kind = resolver.buffer.kind.clone();
        let graph_metadata = resolver.buffer.metadata.clone();
        let graph_len = resolver.buffer.bytes.len();

        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");

        assert_eq!(resolver.buffer.kind, graph_kind);
        assert_eq!(resolver.buffer.metadata, graph_metadata);
        assert_eq!(resolver.buffer.bytes.len(), graph_len);
    }

    #[test]
    fn the_test_pattern_before_any_frame_falls_through_to_the_graph() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.test_pattern = true;

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("first frame");

        assert_eq!(
            resolver.input_resolve_calls, 1,
            "with no established extent there is nothing to repaint, so the graph runs",
        );
        assert_eq!(resolver.published_samples(), vec![1000, 2000, 3000]);
    }

    #[test]
    fn an_unreadable_debug_slot_never_stops_the_output() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        // No project def behind the node — exactly the engine-level harness
        // case, and any future one where an output outlives its def.
        resolver.test_pattern_resolvable = false;

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("graph frame");

        assert_eq!(
            resolver.published_samples(),
            vec![1000, 2000, 3000],
            "an unreadable diagnostic reads as off; the output keeps pushing pixels",
        );
        assert_eq!(resolver.input_resolve_calls, 2);
    }

    /// The compile-window broadcast drops NOTHING here (M6 P4). The #303
    /// handler cleared `control_samples`; measurement showed the output's own
    /// `produce` resizes it back earlier in the same tick than the compile
    /// runs, so the drop freed nothing at the compile instant. See
    /// `docs/defects/2026-08-04-compile-window-drops-rebuilt-before-compile.md`.
    #[test]
    fn memory_pressure_does_not_drop_the_control_samples() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");
        let samples_before = node.control_samples.clone();
        assert!(
            !samples_before.is_empty(),
            "an empty baseline would prove nothing"
        );

        for level in [
            PressureLevel::Low,
            PressureLevel::Medium,
            PressureLevel::High,
            PressureLevel::Critical,
        ] {
            let mut ctx = MemPressureCtx::new(node_id(), Revision::new(2));
            node.handle_memory_pressure(level, &mut ctx)
                .expect("handle pressure");
        }

        assert_eq!(
            node.control_samples, samples_before,
            "control_samples must survive a pressure broadcast"
        );
    }

    #[test]
    fn the_output_still_accepts_no_runtime_commands() {
        let mut node = output_node();

        // The Debug slot rides the overlay, not the command channel: nothing
        // here is wire state, and WIRE_PROTO_VERSION is untouched.
        assert!(
            node.handle_command(
                &lpc_wire::WireNodeCommand::PlaylistActivateEntry { entry: 1 },
                0.0,
            )
            .is_err(),
        );
    }
}
