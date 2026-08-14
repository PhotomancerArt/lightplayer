//! Dioxus components: the canvas, its floating furniture, the keyboard
//! grammar, and the object-properties pane. Hosts compose them — there is
//! no wrapper editor component; the canvas IS the surface.

pub mod canvas;
pub mod floats;
pub mod keys;
pub mod object_properties;
pub mod reference;
pub mod view_options;
pub mod wheel;
