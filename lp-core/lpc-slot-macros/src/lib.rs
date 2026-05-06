//! Derive macros for LightPlayer slot access.

use proc_macro::TokenStream;

mod attr;
mod record;

#[proc_macro_derive(SlotRecord, attributes(slot))]
pub fn derive_slot_record(input: TokenStream) -> TokenStream {
    record::derive(input.into()).into()
}
