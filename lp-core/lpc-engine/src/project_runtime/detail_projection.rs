//! M4.1 legacy [`lpc_wire::legacy::NodeDetail`] projection (compatibility-facing).

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::lp_path::LpPathBuf;
use lpc_model::resource::ResourceRef;
use lpc_model::{FrameId, NodeId};
use lpc_source::legacy::nodes::NodeKind;
use lpc_source::legacy::nodes::texture::{TextureConfig, TextureFormat};
use lpc_wire::WireNodeSpecifier;
use lpc_wire::legacy::nodes::fixture::FixtureState;
use lpc_wire::legacy::nodes::output::OutputState;
use lpc_wire::legacy::nodes::shader::ShaderState;
use lpc_wire::legacy::nodes::texture::TextureState;
use lpc_wire::legacy::{NodeDetail, NodeState};

use crate::engine::Engine;
use crate::tree::{EntryState, NodeEntry};

use super::compatibility_projection::CompatibilityProjection;
use super::kind::legacy_node_kind_from_ty;

pub(crate) fn build_node_detail_map(
    engine: &Engine,
    compatibility: &CompatibilityProjection,
    detail_specifier: &WireNodeSpecifier,
    current_frame: FrameId,
) -> BTreeMap<NodeId, NodeDetail> {
    let wanted = detail_handle_set(engine, detail_specifier);
    let mut out = BTreeMap::new();

    for entry in engine.tree().entries() {
        if entry.id == engine.tree().root() {
            continue;
        }

        let Some(ty) = path_leaf_ty(entry) else {
            continue;
        };
        let Some(kind) = legacy_node_kind_from_ty(ty) else {
            continue;
        };

        if !wanted.contains(&entry.id) {
            continue;
        }

        let Some(config) = compatibility.node_config_box_for(entry.id) else {
            continue;
        };

        let ver_frame = projection_frame_stamp(entry.created_frame, entry.change_frame);

        let state = match kind {
            NodeKind::Texture => NodeState::Texture(build_texture_state(
                compatibility,
                entry,
                ver_frame,
                current_frame,
            )),
            NodeKind::Shader => NodeState::Shader(build_shader_state(entry, ver_frame)),
            NodeKind::Output => NodeState::Output(build_output_state(entry, ver_frame)),
            NodeKind::Fixture => {
                NodeState::Fixture(build_fixture_state(engine, entry, ver_frame, current_frame))
            }
        };

        out.insert(
            entry.id,
            NodeDetail {
                path: LpPathBuf::from(entry.path.to_string()),
                config,
                state,
            },
        );
    }

    out
}

fn projection_frame_stamp(created: FrameId, changed: FrameId) -> FrameId {
    FrameId::new(changed.0.max(created.0).max(FrameId::default().0))
}

fn detail_handle_set(engine: &Engine, spec: &WireNodeSpecifier) -> BTreeSet<NodeId> {
    let mut set = BTreeSet::new();
    match spec {
        WireNodeSpecifier::None => {}
        WireNodeSpecifier::All => {
            for entry in engine.tree().entries() {
                if entry.id == engine.tree().root() {
                    continue;
                }
                if path_leaf_ty(entry)
                    .and_then(legacy_node_kind_from_ty)
                    .is_some()
                {
                    set.insert(entry.id);
                }
            }
        }
        WireNodeSpecifier::ByHandles(handles) => {
            for &h in handles {
                set.insert(h);
            }
        }
    }
    set
}

fn path_leaf_ty<N>(entry: &NodeEntry<N>) -> Option<&str> {
    Some(entry.path.0.last()?.ty.0.as_str())
}

fn output_node_id_owning_sink(
    engine: &Engine,
    sink_id: crate::runtime_buffer::RuntimeBufferId,
) -> Option<NodeId> {
    for entry in engine.tree().entries() {
        if path_leaf_ty(entry)? != "output" {
            continue;
        }
        if let EntryState::Alive(node) = &entry.state {
            if node.runtime_output_sink_buffer_id() == Some(sink_id) {
                return Some(entry.id);
            }
        }
    }
    None
}

fn build_texture_state(
    compatibility: &CompatibilityProjection,
    entry: &NodeEntry<Box<dyn crate::node::Node>>,
    ver_frame: FrameId,
    current_frame: FrameId,
) -> TextureState {
    let mut state = TextureState::new(FrameId::default());
    state.width.set(
        ver_frame,
        compatibility
            .node_config_box_for(entry.id)
            .and_then(|cfg| {
                cfg.as_any()
                    .downcast_ref::<TextureConfig>()
                    .map(|c| c.width)
            })
            .unwrap_or(0),
    );
    state.height.set(
        ver_frame,
        compatibility
            .node_config_box_for(entry.id)
            .and_then(|cfg| {
                cfg.as_any()
                    .downcast_ref::<TextureConfig>()
                    .map(|c| c.height)
            })
            .unwrap_or(0),
    );
    state.format.set(ver_frame, TextureFormat::Rgba16);
    let _ = current_frame;
    state.texture_data.set_inline(ver_frame, Vec::new());
    state
}

fn build_shader_state(
    entry: &NodeEntry<Box<dyn crate::node::Node>>,
    ver_frame: FrameId,
) -> ShaderState {
    let mut st = ShaderState::new(FrameId::default());
    if let EntryState::Alive(node) = &entry.state {
        if let Some(pw) = node.shader_projection_wire() {
            st.glsl_code.set(ver_frame, String::from(pw.glsl_source));
            st.error
                .set(ver_frame, pw.compilation_error.map(String::from));
            st.render_product.set(
                ver_frame,
                pw.render_product_id.map(ResourceRef::render_product),
            );
        }
    }
    st
}

fn build_output_state(
    entry: &NodeEntry<Box<dyn crate::node::Node>>,
    ver_frame: FrameId,
) -> OutputState {
    let mut st = OutputState::new(FrameId::default());
    if let EntryState::Alive(node) = &entry.state {
        if let Some(cid) = node.runtime_output_sink_buffer_id() {
            st.channel_data
                .set_resource(ver_frame, ResourceRef::runtime_buffer(cid));
        }
    }
    st
}

fn build_fixture_state(
    engine: &Engine,
    entry: &NodeEntry<Box<dyn crate::node::Node>>,
    ver_frame: FrameId,
    current_frame: FrameId,
) -> FixtureState {
    let mut st = FixtureState::new(FrameId::default());
    if let EntryState::Alive(node) = &entry.state {
        if let Some(fx) = node.fixture_projection_info() {
            if let Some(lc) = fx.lamp_colors_buffer_id {
                st.lamp_colors
                    .set_resource(ver_frame, ResourceRef::runtime_buffer(lc));
            }
            st.texture_handle.set(ver_frame, Some(fx.texture_node_id));
            if let Some(out_id) = output_node_id_owning_sink(engine, fx.output_sink_buffer_id) {
                st.output_handle.set(ver_frame, Some(out_id));
            }
        }
    }
    st.mapping_cells.set(ver_frame, Vec::new());
    let _ = current_frame;
    st
}
