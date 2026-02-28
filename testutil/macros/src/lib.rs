use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, punctuated::Punctuated, FnArg, Ident, ItemFn, ItemImpl, Path, Token, Type};

/// Register a test function with runtime preconditions.
///
/// Each argument must resolve to a function `fn() -> Result<(), String>` in
/// scope. Paths are supported (e.g. `preconditions::valgrind`).
///
/// Parameters annotated with `#[fixture]` are automatically injected from
/// their fixture's storage. The fixture's requirements are merged into the
/// test's requirement list.
///
/// ```ignore
/// #[requires(preconditions::valgrind)]
/// fn my_test(#[fixture] image: &ConformanceImage) { /* ... */ }
/// ```
#[proc_macro_attribute]
pub fn requires(attr: TokenStream, item: TokenStream) -> TokenStream {
    let reqs = parse_macro_input!(attr with Punctuated::<Path, Token![,]>::parse_terminated);
    let func = parse_macro_input!(item as ItemFn);
    let name = &func.sig.ident;
    let name_str = name.to_string();

    let req_exprs: Vec<_> = reqs.iter().collect();

    // Collect #[fixture] parameters.
    let mut fixture_bindings = Vec::new();
    let mut fixture_req_exprs = Vec::new();
    let mut clean_params = Vec::new();

    for param in &func.sig.inputs {
        if let FnArg::Typed(pat_type) = param {
            let has_fixture = pat_type.attrs.iter().any(|a| a.path().is_ident("fixture"));
            if has_fixture {
                // Extract binding name and type.
                let binding = &pat_type.pat;
                let ty = strip_reference(&pat_type.ty);

                fixture_bindings.push((binding.clone(), ty.clone()));
                fixture_req_exprs.push(ty.clone());

                // Emit the parameter without #[fixture].
                let mut clean = pat_type.clone();
                clean.attrs.retain(|a| !a.path().is_ident("fixture"));
                clean_params.push(FnArg::Typed(clean));
            } else {
                clean_params.push(param.clone());
            }
        } else {
            clean_params.push(param.clone());
        }
    }

    // Build the fixture injection code.
    let fixture_setup: Vec<_> = fixture_bindings
        .iter()
        .map(|(binding, ty)| {
            quote! {
                let #binding = <#ty as ::testutil::FixtureStorage>::storage()
                    .get_or_init(|| <#ty as ::testutil::Fixture>::setup())
                    .as_ref()
                    .expect("fixture setup failed");
            }
        })
        .collect();

    // Emit the original function with cleaned parameters.
    let vis = &func.vis;
    let block = &func.block;
    let ret = &func.sig.output;
    let fn_token = &func.sig.fn_token;
    let attrs: Vec<_> = func.attrs.iter().collect();

    // Build the call arguments (just the binding names).
    let call_args: Vec<_> = fixture_bindings.iter().map(|(binding, _)| binding).collect();
    let has_fixtures = !fixture_bindings.is_empty();

    if has_fixtures {
        let expanded = quote! {
            #(#attrs)*
            #vis #fn_token #name(#(#clean_params),*) #ret #block

            ::testutil::inventory::submit!(::testutil::TestDef {
                name: #name_str,
                requires: &[#(#req_exprs),*],
                body: || {
                    #(#fixture_setup)*
                    #name(#(#call_args),*);
                },
            });
        };
        expanded.into()
    } else {
        // No fixtures — simple case.
        let expanded = quote! {
            #(#attrs)*
            #vis #fn_token #name(#(#clean_params),*) #ret #block

            ::testutil::inventory::submit!(::testutil::TestDef {
                name: #name_str,
                requires: &[#(#req_exprs),*],
                body: || { #name(); },
            });
        };
        expanded.into()
    }
}

/// Generate fixture storage and metadata for a `Fixture` impl block.
///
/// ```ignore
/// #[testutil::fixture(docker)]
/// impl testutil::Fixture for ConformanceImage {
///     const SCOPE: Scope = Scope::Static;
///     fn setup() -> Result<Self, String> { /* ... */ }
///     fn teardown(&self) { /* ... */ }
/// }
/// ```
#[proc_macro_attribute]
pub fn fixture(attr: TokenStream, item: TokenStream) -> TokenStream {
    let reqs = parse_macro_input!(attr with Punctuated::<Path, Token![,]>::parse_terminated);
    let impl_block = parse_macro_input!(item as ItemImpl);

    // Extract the type from `impl Fixture for <Type>`.
    let ty = &impl_block.self_ty;

    // Generate a unique static name from the type.
    let static_name = fixture_static_name(ty);

    let req_exprs: Vec<_> = reqs.iter().collect();

    let expanded = quote! {
        #impl_block

        static #static_name: ::std::sync::OnceLock<Result<#ty, String>> = ::std::sync::OnceLock::new();

        impl ::testutil::FixtureStorage for #ty {
            const REQUIRES: &'static [fn() -> Result<(), String>] = &[#(#req_exprs),*];

            fn storage() -> &'static ::std::sync::OnceLock<Result<Self, String>> {
                &#static_name
            }
        }

        ::testutil::inventory::submit!(::testutil::FixtureTeardownDef {
            teardown_if_initialized: || {
                if let Some(Ok(instance)) = #static_name.get() {
                    <#ty as ::testutil::Fixture>::teardown(instance);
                }
            },
        });
    };

    expanded.into()
}

/// Strip a leading `&` from a type (e.g. `&ConformanceImage` → `ConformanceImage`).
fn strip_reference(ty: &Type) -> Type {
    if let Type::Reference(r) = ty {
        (*r.elem).clone()
    } else {
        ty.clone()
    }
}

/// Generate a static name like `__FIXTURE_CONFORMANCE_IMAGE` from a type.
fn fixture_static_name(ty: &Type) -> Ident {
    let ty_str = quote!(#ty)
        .to_string()
        .replace("::", "_")
        .replace(' ', "")
        .to_uppercase();
    format_ident!("__FIXTURE_{}", ty_str)
}
