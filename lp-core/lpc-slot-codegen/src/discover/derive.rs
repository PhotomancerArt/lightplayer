pub(crate) fn has_derive(attrs: &[syn::Attribute], derive_name: &str) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .any(|attr| {
            attr.meta.require_list().is_ok_and(|meta| {
                meta.tokens
                    .to_string()
                    .split(',')
                    .any(|derive| derive.trim().ends_with(derive_name))
            })
        })
}
