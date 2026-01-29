//! Proc macro for #[lpfx_impl] attribute
//!
//! This macro exists solely to make Rust recognize the #[lpfx_impl] attribute.
//! It expands to nothing - the actual parsing is done by the codegen tool.

use proc_macro::TokenStream;

/// #[lpfx_impl] attribute - expands to nothing
#[proc_macro_attribute]
pub fn lpfx_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Just return the item unchanged - this macro exists only to register the attribute
    item
}
