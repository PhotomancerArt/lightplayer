//! Visual artifacts and their substructure types.
//!
//! See `docs/design/lightplayer/domain.md` for the Visual taxonomy
//! (Pattern / Effect / Transition / Stack / Live / Playlist) and
//! `docs/design/lpfx/overview.md` for the lpfx vocabulary.

pub mod effect;
pub mod live;
pub mod params_table;
pub mod pattern;
pub mod playlist;
pub mod shader_ref;
pub mod stack;
pub mod transition;
pub mod transition_ref;
pub mod visual_input;

pub use effect::Effect;
pub use live::{Live, LiveCandidate};
pub use params_table::ParamsTable;
pub use pattern::Pattern;
pub use playlist::{Playlist, PlaylistBehavior, PlaylistEntry};
pub use shader_ref::ShaderRef;
pub use stack::{EffectRef, Stack};
pub use transition::Transition;
pub use transition_ref::TransitionRef;
pub use visual_input::VisualInput;
