//! Host-OS LightPlayer runtime support.

pub mod host_runtime;
pub mod host_runtime_error;
mod server_loop;

pub use host_runtime::{HostRuntime, create_memory_server_with};
pub use host_runtime_error::HostRuntimeError;
// The root identity-file convention, for embedders that seed the server fs
// (lpa-link's fake device stamps it so the hello carries the scripted uid).
pub use lpa_server::DEVICE_IDENTITY_PATH;

// The build's self-description, embedded as a scannable blob (extracted by
// `lp-cli firmware show` and reported on ServerHello in M4). Target triple
// and profile come from build.rs; no VCS facts in this build.
lpc_model::lp_embed_manifest_core! {
    package: env!("CARGO_PKG_NAME"),
    chip_family: "host",
    chip: "native",
    cargo_target: env!("LP_CARGO_TARGET"),
    profile: env!("LP_BUILD_PROFILE"),
    commit: "unknown",
    dirty: false,
    wire_proto: lpc_wire::WIRE_PROTO_VERSION,
    features: [
        lpa_server::ENGINE_FEATURE_FRAGMENT,
        lpc_model::manifest::feature_fragment(true, lpc_model::LpFeature::GfxLpvm),
    ],
    limits_json: "{}",
}
