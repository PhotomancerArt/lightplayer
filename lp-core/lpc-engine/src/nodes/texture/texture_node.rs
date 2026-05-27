//! Core texture node: width/height/format metadata for shader output sizing.

use lpc_model::NodeId;
use lpc_model::Revision;
use lpc_model::SlotAccess;
use lpc_model::SlotPath;
use lpc_model::SlotShapeRegistry;
use lpc_model::SlotShapeRegistryError;
use lpc_model::TextureDefView;
use lpc_model::WithRevision;
use lpc_model::nodes::texture::TextureFormat;
use lpc_model::nodes::texture::TextureState;

use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeResourceInitContext, NodeRuntime, PressureLevel,
    ProduceResult, RuntimeStateShape, TickContext,
};
use crate::resource::{RuntimeBuffer, RuntimeBufferId};

#[cfg(test)]
fn size_path() -> SlotPath {
    SlotPath::parse("size").expect("size path")
}

fn texture_format_tag(f: TextureFormat) -> u32 {
    match f {
        TextureFormat::Rgb8 => 0,
        TextureFormat::Rgba8 => 1,
        TextureFormat::R8 => 2,
        TextureFormat::Rgba16 => 3,
    }
}

/// MVP texture node: exposes texture metadata derived from authored config.
pub struct TextureNode {
    node_id: NodeId,
    pixel_format: TextureFormat,
    state: TextureState,
    def_view: Option<TextureDefView>,
    texture_buffer_id: Option<RuntimeBufferId>,
}

impl TextureNode {
    pub fn new(node_id: NodeId) -> Self {
        let pixel_format = TextureFormat::Rgba16;
        Self {
            node_id,
            pixel_format,
            state: TextureState::new(0, 0, texture_format_tag(pixel_format)),
            def_view: None,
            texture_buffer_id: None,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn pixel_format(&self) -> TextureFormat {
        self.pixel_format
    }

    fn def_view(&mut self, ctx: &TickContext<'_>) -> Result<&TextureDefView, NodeError> {
        TextureDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(|e| NodeError::msg(alloc::format!("compile texture def view: {e}")))
    }
}

impl NodeRuntime for TextureNode {
    fn init_resources(&mut self, ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        if self.texture_buffer_id.is_some() {
            return Ok(());
        }
        let id = ctx.insert_runtime_buffer(WithRevision::new(
            Revision::default(),
            RuntimeBuffer::texture_rgba16(0, 0, alloc::vec::Vec::new()),
        ));
        self.texture_buffer_id = Some(id);
        Ok(())
    }

    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        let size: lpc_model::Dim2u = self.def_view(ctx)?.size().get(ctx)?;
        self.state.sync_with_revision(
            ctx.revision(),
            i32::try_from(size.width).unwrap_or(i32::MAX),
            i32::try_from(size.height).unwrap_or(i32::MAX),
            texture_format_tag(self.pixel_format),
        );
        if let Some(buffer_id) = self.texture_buffer_id {
            ctx.with_runtime_buffer_mut(buffer_id, ctx.revision(), |buffer| {
                *buffer =
                    RuntimeBuffer::texture_rgba16(size.width, size.height, alloc::vec::Vec::new());
                Ok(())
            })?;
        }
        Ok(ProduceResult::Produced)
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

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        TextureState::register_runtime_state_shape(registry).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::ArtifactLocation;
    use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};
    use crate::engine::Engine;
    use crate::engine::resolve_with_engine_host;
    use crate::node::test_placeholder_spine;
    use alloc::boxed::Box;
    use lpc_model::{Dim2u, Kind, LpValue, NodeDef, Revision, TextureDef, ToLpValue, TreePath};
    use lpc_wire::{WireChildKind, WireSlotIndex};
    use lps_shared::LpsValueF32;

    #[test]
    fn texture_metadata_props_resolve_on_engine() {
        let (mut engine, tid) = texture_engine(64, 48);

        let w = QueryKey::ConsumedSlot {
            node: tid,
            slot: size_path(),
        };
        let pv = resolve_with_engine_host(&mut engine, w, ResolveLogLevel::Off)
            .expect("resolve")
            .0;
        assert_dim2u_value(&pv.as_value().expect("value"), 64, 48);
    }

    #[test]
    fn texture_tick_reads_authored_size_through_slot_view() {
        let (mut engine, tid) = texture_engine(64, 48);

        let pv = resolve_with_engine_host(
            &mut engine,
            QueryKey::ProducedSlot {
                node: tid,
                slot: SlotPath::parse("width").unwrap(),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve")
        .0;
        assert!(matches!(
            pv.as_value().expect("value"),
            LpsValueF32::I32(64)
        ));
    }

    #[test]
    fn texture_tick_uses_bound_size_override() {
        let (mut engine, tid) = texture_engine(64, 48);
        engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::Literal(texture_size_value(7, 9)),
                    target: BindingTarget::ConsumedSlot {
                        node: tid,
                        slot: size_path(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Ratio,
                    owner: tid,
                },
                Revision::new(2),
            )
            .expect("binding");

        let pv = resolve_with_engine_host(
            &mut engine,
            QueryKey::ProducedSlot {
                node: tid,
                slot: SlotPath::parse("height").unwrap(),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve")
        .0;
        assert!(matches!(pv.as_value().expect("value"), LpsValueF32::I32(9)));
    }

    #[test]
    fn texture_node_exposes_owned_texture_resource_summary() {
        let (mut engine, tid) = texture_engine(64, 48);

        resolve_with_engine_host(
            &mut engine,
            QueryKey::ProducedSlot {
                node: tid,
                slot: SlotPath::parse("width").unwrap(),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve texture width");
        let response = engine.read_project(lpc_wire::ProjectReadRequest::default_debug(None));

        let lpc_wire::ProjectReadResult::Resources(resources) = &response.results[2] else {
            panic!("third result should be resources");
        };
        let texture = resources
            .summaries
            .iter()
            .find(|summary| {
                matches!(
                    summary.metadata,
                    lpc_wire::WireResourceMetadataSummary::Texture {
                        width: 64,
                        height: 48,
                        ..
                    }
                )
            })
            .expect("texture summary");
        assert_eq!(texture.owner, Some(tid));
    }

    fn texture_engine(width: u32, height: u32) -> (Engine, NodeId) {
        let mut engine = Engine::new(TreePath::parse("/t.show").expect("path"));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let (spine, _) = test_placeholder_spine();
        let artifact = engine
            .artifacts_mut()
            .acquire_location(ArtifactLocation::file("/texture.toml"), frame);
        engine
            .artifacts_mut()
            .load_with(&artifact, frame, |_location| {
                Ok(NodeDef::Texture(TextureDef::new(width, height)))
            })
            .expect("load texture artifact");
        let tid = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").expect("name"),
                lpc_model::NodeName::parse("texture").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine,
                artifact,
                frame,
            )
            .expect("add");
        let tex = TextureNode::new(tid);
        engine
            .attach_runtime_node(tid, Box::new(tex), frame)
            .expect("attach");
        (engine, tid)
    }

    fn texture_size_value(width: u32, height: u32) -> LpValue {
        Dim2u { width, height }.to_lp_value()
    }

    fn assert_dim2u_value(value: &LpsValueF32, width: u32, height: u32) {
        assert!(matches!(
            value,
            LpsValueF32::Struct { fields, .. }
                if matches!(fields.as_slice(), [
                    (name_w, LpsValueF32::U32(found_width)),
                    (name_h, LpsValueF32::U32(found_height)),
                ] if name_w == "width"
                    && name_h == "height"
                    && *found_width == width
                    && *found_height == height)
        ));
    }
}
