//! Shell views for parent def change detection.

use lpc_model::{
    EnumSlot, NodeDef, NodeInvocation, NodeKind,
    nodes::{
        button::ButtonDef,
        clock::ClockDef,
        fixture::FixtureDef,
        fluid::FluidDef,
        output::OutputDef,
        playlist::PlaylistDef,
        project::ProjectDef,
        radio::ControlRadioDef,
        shader::{ComputeShaderDef, ShaderDef},
        texture::TextureDef,
    },
};

/// Parent-facing view: inline invocation bodies replaced with kind-only stubs.
pub fn def_shell(def: &NodeDef) -> NodeDef {
    match def {
        NodeDef::Project(project) => {
            let mut shell = project.clone();
            for invocation in shell.nodes.entries.values_mut() {
                *invocation = EnumSlot::new(invocation_shell(invocation.value()));
            }
            NodeDef::Project(shell)
        }
        NodeDef::Playlist(playlist) => {
            let mut shell = playlist.clone();
            for entry in shell.entries.entries.values_mut() {
                entry.node = EnumSlot::new(invocation_shell(entry.node.value()));
            }
            NodeDef::Playlist(shell)
        }
        other => other.clone(),
    }
}

fn invocation_shell(invocation: &NodeInvocation) -> NodeInvocation {
    match invocation {
        NodeInvocation::Unset | NodeInvocation::Ref(_) => invocation.clone(),
        NodeInvocation::Def(body) => NodeInvocation::inline(kind_stub(body.value().kind())),
    }
}

fn kind_stub(kind: NodeKind) -> NodeDef {
    match kind {
        NodeKind::Project => NodeDef::Project(ProjectDef::default()),
        NodeKind::Button => NodeDef::Button(ButtonDef::default()),
        NodeKind::Clock => NodeDef::Clock(ClockDef::default()),
        NodeKind::Texture => NodeDef::Texture(TextureDef::default()),
        NodeKind::Shader => NodeDef::Shader(ShaderDef::default()),
        NodeKind::ComputeShader => NodeDef::ComputeShader(ComputeShaderDef::default()),
        NodeKind::Fluid => NodeDef::Fluid(FluidDef::default()),
        NodeKind::Playlist => NodeDef::Playlist(PlaylistDef::default()),
        NodeKind::ControlRadio => NodeDef::ControlRadio(ControlRadioDef::default()),
        NodeKind::Output => NodeDef::Output(OutputDef::default()),
        NodeKind::Fixture => NodeDef::Fixture(FixtureDef::default()),
    }
}

/// True when full authored bodies differ.
pub fn body_changed(before: &NodeDef, after: &NodeDef) -> bool {
    before != after
}

/// True when parent shell views differ (inline bodies stripped to kind stubs).
pub fn shell_changed(before: &NodeDef, after: &NodeDef) -> bool {
    def_shell(before) != def_shell(after)
}

pub fn is_container_def(def: &NodeDef) -> bool {
    matches!(def, NodeDef::Project(_) | NodeDef::Playlist(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::NodeDef;

    fn parse_def(text: &str) -> NodeDef {
        NodeDef::from_toml_str(text).expect("node def")
    }

    #[test]
    fn inline_child_body_edit_does_not_change_parent_shell() {
        let before = parse_def(
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "a.glsl" }
"#,
        );
        let after = parse_def(
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "b.glsl" }
"#,
        );
        assert!(body_changed(&before, &after));
        assert!(!shell_changed(&before, &after));
    }

    #[test]
    fn inline_child_kind_flip_changes_parent_shell() {
        let before = parse_def(
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
"#,
        );
        let after = parse_def(
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Clock"
"#,
        );
        assert!(shell_changed(&before, &after));
    }
}
