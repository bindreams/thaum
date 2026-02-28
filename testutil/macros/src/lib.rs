use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, punctuated::Punctuated, ItemFn, Path, Token};

/// Register a test function with runtime preconditions.
///
/// Each argument must resolve to a function `fn() -> Result<(), String>` in
/// scope. Paths are supported (e.g. `preconditions::valgrind`).
///
/// ```ignore
/// #[requires(preconditions::valgrind, preconditions::thaum)]
/// fn callgrind_trivial_lex() { /* ... */ }
/// ```
#[proc_macro_attribute]
pub fn requires(attr: TokenStream, item: TokenStream) -> TokenStream {
    let reqs = parse_macro_input!(attr with Punctuated::<Path, Token![,]>::parse_terminated);
    let func = parse_macro_input!(item as ItemFn);
    let name = &func.sig.ident;
    let name_str = name.to_string();

    let req_exprs: Vec<_> = reqs.iter().collect();

    let expanded = quote! {
        #func

        ::testutil::inventory::submit!(::testutil::TestDef {
            name: #name_str,
            requires: &[#(#req_exprs),*],
            body: #name,
        });
    };

    expanded.into()
}
