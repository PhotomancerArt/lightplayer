#[cfg(feature = "node-button")]
pub mod button;
#[cfg(feature = "node-clock")]
pub mod clock;
#[cfg(feature = "node-fixture")]
pub mod fixture;
#[cfg(feature = "node-fluid")]
pub mod fluid;
pub mod module;
pub mod output;
mod placeholder;
// Always declared — `playlist::playlist_output_path` stays compiled even
// when `node-playlist` is off (see `playlist/mod.rs`); the `PlaylistNode`
// runtime itself is gated inside that module.
pub mod playlist;
#[cfg(feature = "node-radio")]
pub mod radio;
#[cfg(feature = "node-shader")]
pub mod shader;
#[cfg(feature = "node-texture")]
pub mod texture;

#[cfg(feature = "node-button")]
pub use button::{ButtonNode, button_down_path, button_held_path, button_up_path};
#[cfg(feature = "node-clock")]
pub use clock::{ClockNode, clock_product_path, clock_seconds_path};
#[cfg(feature = "node-fixture")]
pub use fixture::fixture_node::{
    FixtureMap2dSource, FixtureMapping, FixtureNode, FixturePatchSource, fixture_input_path,
};
#[cfg(feature = "node-fluid")]
pub use fluid::{FluidNode, MsaFluidSolver, fluid_emitters_path, fluid_output_path};
pub use module::ModuleNode;
pub use output::output_node::{
    FragmentCoverage, FragmentPlacement, OutputFragment, OutputNode, output_input_path,
};
pub use placeholder::CorePlaceholderNode;
pub use playlist::playlist_output_path;
#[cfg(feature = "node-playlist")]
pub use playlist::{PlaylistNode, PlaylistRuntimeEntry};
#[cfg(feature = "node-radio")]
pub use radio::{ControlRadioNode, control_radio_input_path, control_radio_output_path};
#[cfg(feature = "node-shader")]
pub use shader::compute_shader_node::ComputeShaderNode;
#[cfg(feature = "node-shader")]
pub use shader::shader_node::{ShaderNode, shader_output_path};
#[cfg(feature = "node-texture")]
pub use texture::texture_node::TextureNode;
