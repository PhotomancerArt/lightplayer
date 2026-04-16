//! Proc macro for #[lpfn_impl] attribute
//!
//! This macro exists solely to make Rust recognize the #[lpfn_impl] attribute.
//! It expands to nothing - the actual parsing is done by the codegen tool.

use proc_macro::TokenStream;

/// #[lpfn_impl] attribute - expands to nothing
#[proc_macro_attribute]
pub fn lpfn_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Just return the item unchanged - this macro exists only to register the attribute
    item
}
