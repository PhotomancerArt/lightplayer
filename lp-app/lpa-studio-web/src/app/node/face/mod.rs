//! Kind-specific node card faces (permanent face + drawers grammar).
//!
//! A face is the permanent top of a node card: preview + panel controls
//! (+ chat on shader). It never leaves; the code/advanced drawers expand
//! beneath it and growth is downward-only from a stable top. Nodes without
//! a face keep today's generic section rendering ([`super::NodePane`]'s
//! fallback branch).

mod fixture_face;
mod node_card_drawers;
mod node_card_section;
mod node_face_body;
mod playlist_face;
mod shader_face;

pub use fixture_face::FixtureFace;
pub use node_card_drawers::NodeCardDrawers;
pub use node_card_section::NodeCardSection;
pub use node_face_body::NodeFaceBody;
pub use playlist_face::PlaylistFace;
pub use shader_face::ShaderFace;
