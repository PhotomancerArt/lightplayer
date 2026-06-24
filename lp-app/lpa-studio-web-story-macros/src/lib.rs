//! Attribute macros for Studio web stories.
//!
//! The `#[story]` macro deliberately keeps runtime behavior small. The Studio
//! web build script reads story metadata from source files and generates the
//! registry; this macro validates the story function and makes it callable from
//! that generated registry.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::{Error, FnArg, ItemFn, LitStr, ReturnType, Visibility, parse_macro_input, parse_quote};

/// Mark a zero-argument function as a Studio story.
///
/// ```ignore
/// #[story]
/// fn example() -> Element {
///     /* ... */
/// }
/// ```
///
/// `label` and `description` can be passed as optional display metadata, but
/// ordinary stories should let the build script derive the label from the
/// function name.
///
/// Story route identity is inferred by `lpa-studio-web/build.rs` from the file
/// path plus function name, so this macro intentionally does not accept an id,
/// family, category, or component.
#[proc_macro_attribute]
pub fn story(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut function = parse_macro_input!(item as ItemFn);
    parse_macro_input!(args with StoryArgs::parse);

    if let Err(error) = validate_story_function(&function) {
        return error.to_compile_error().into();
    }

    if matches!(function.vis, Visibility::Inherited) {
        function.vis = parse_quote!(pub(crate));
    }

    quote!(#function).into()
}

struct StoryArgs {
    label: Option<LitStr>,
    description: Option<LitStr>,
}

impl StoryArgs {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut args = Self {
            label: None,
            description: None,
        };
        if input.is_empty() {
            return Ok(args);
        }

        let parser = syn::meta::parser(|meta| args.parse_meta(meta));
        let tokens = input.parse()?;
        parser.parse2(tokens)?;

        Ok(args)
    }

    fn parse_meta(&mut self, meta: syn::meta::ParseNestedMeta<'_>) -> syn::Result<()> {
        if meta.path.is_ident("label") {
            let value = meta.value()?;
            self.set_once("label", value.parse()?)?;
            return Ok(());
        }

        if meta.path.is_ident("description") {
            let value = meta.value()?;
            self.set_once("description", value.parse()?)?;
            return Ok(());
        }

        let path = &meta.path;
        let name = path
            .get_ident()
            .map(ToString::to_string)
            .unwrap_or_else(|| quote!(#path).to_string());
        Err(meta.error(format!(
            "unsupported story argument `{name}`; use `#[story]`, `label = \"...\"`, or `description = \"...\"`"
        )))
    }

    fn set_once(&mut self, key: &'static str, value: LitStr) -> syn::Result<()> {
        let slot = match key {
            "label" => &mut self.label,
            "description" => &mut self.description,
            _ => unreachable!("story arg key is fixed by parser"),
        };
        if slot.is_some() {
            return Err(Error::new(
                value.span(),
                format!("duplicate `{key}` argument in #[story]"),
            ));
        }
        *slot = Some(value);
        Ok(())
    }
}

fn validate_story_function(function: &ItemFn) -> syn::Result<()> {
    if let Some(input) = function.sig.inputs.first() {
        let input_label = match input {
            FnArg::Receiver(_) => "self parameter",
            FnArg::Typed(_) => "parameter",
        };
        return Err(Error::new_spanned(
            input,
            format!("story functions must take no arguments; remove this {input_label}"),
        ));
    }

    if !function.sig.generics.params.is_empty() || function.sig.generics.where_clause.is_some() {
        return Err(Error::new_spanned(
            &function.sig.generics,
            "story functions must not be generic",
        ));
    }

    if matches!(function.sig.output, ReturnType::Default) {
        return Err(Error::new_spanned(
            &function.sig.ident,
            "story functions must return `Element`",
        ));
    }

    Ok(())
}
