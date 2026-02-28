use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    bracketed, parse_macro_input, punctuated::Punctuated, FnArg, Ident, ItemFn, ItemImpl, LitStr, Path, Token, Type,
};

// #[testutil::test] argument parsing ==========================================

/// Parsed arguments for `#[testutil::test(...)]`.
#[derive(Default)]
struct TestArgs {
    requires: Vec<Path>,
    name: Option<String>,
    labels: Vec<Ident>,
    ignore: IgnoreArg,
}

#[derive(Default)]
enum IgnoreArg {
    #[default]
    No,
    Yes,
    WithReason(String),
}

impl Parse for TestArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = TestArgs::default();

        if input.is_empty() {
            return Ok(args);
        }

        loop {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "requires" => {
                    let _eq: Token![=] = input.parse()?;
                    let content;
                    bracketed!(content in input);
                    args.requires = Punctuated::<Path, Token![,]>::parse_terminated(&content)?
                        .into_iter()
                        .collect();
                }
                "name" => {
                    let _eq: Token![=] = input.parse()?;
                    let lit: LitStr = input.parse()?;
                    args.name = Some(lit.value());
                }
                "labels" => {
                    let _eq: Token![=] = input.parse()?;
                    let content;
                    bracketed!(content in input);
                    args.labels = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
                        .into_iter()
                        .collect();
                }
                "ignore" => {
                    if input.peek(Token![=]) {
                        let _eq: Token![=] = input.parse()?;
                        let lit: LitStr = input.parse()?;
                        args.ignore = IgnoreArg::WithReason(lit.value());
                    } else {
                        args.ignore = IgnoreArg::Yes;
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown argument `{other}`; expected requires, name, labels, or ignore"),
                    ));
                }
            }

            if input.is_empty() {
                break;
            }
            let _comma: Token![,] = input.parse()?;
            if input.is_empty() {
                break; // trailing comma
            }
        }

        Ok(args)
    }
}

// #[testutil::test] ===========================================================

/// Register a test function with the testutil harness.
///
/// Replaces both `#[test]` and `#[requires(...)]`. Supports optional arguments:
///
/// - `requires = [path1, path2]` — runtime preconditions (`fn() -> Result<(), String>`)
/// - `name = "display name"` — custom name shown in test output
/// - `labels = [ident1, ident2]` — prepended as `[ident1][ident2]` for nextest filtering
/// - `ignore` or `ignore = "reason"` — statically ignore the test
///
/// Parameters annotated with `#[fixture]` are automatically injected.
///
/// ```ignore
/// #[testutil::test(requires = [preconditions::valgrind], labels = [slow])]
/// fn my_test(#[fixture] image: &ConformanceImage) { /* ... */ }
/// ```
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as TestArgs);
    let func = parse_macro_input!(item as ItemFn);
    expand_test_def(args, func)
}

/// Deprecated: use `#[testutil::test(requires = [...])]` instead.
#[proc_macro_attribute]
pub fn requires(attr: TokenStream, item: TokenStream) -> TokenStream {
    let reqs = parse_macro_input!(attr with Punctuated::<Path, Token![,]>::parse_terminated);
    let func = parse_macro_input!(item as ItemFn);
    let args = TestArgs {
        requires: reqs.into_iter().collect(),
        ..TestArgs::default()
    };
    expand_test_def(args, func)
}

// Shared expansion logic ======================================================

fn expand_test_def(args: TestArgs, func: ItemFn) -> TokenStream {
    // Guard: reject #[test] on the same function.
    for attr in &func.attrs {
        if attr.path().is_ident("test") {
            return syn::Error::new_spanned(
                attr,
                "remove #[test] — #[testutil::test] already registers this function",
            )
            .to_compile_error()
            .into();
        }
    }

    let name = &func.sig.ident;
    let name_str = name.to_string();

    // Build display_name from labels + name arg.
    let display_name_expr = build_display_name(&name_str, &args.name, &args.labels);

    // Build ignore expression.
    let ignore_expr = match &args.ignore {
        IgnoreArg::No => quote! { ::testutil::Ignore::No },
        IgnoreArg::Yes => quote! { ::testutil::Ignore::Yes },
        IgnoreArg::WithReason(reason) => quote! { ::testutil::Ignore::WithReason(#reason) },
    };

    let req_exprs: Vec<_> = args.requires.iter().collect();

    // Collect #[fixture] parameters.
    let mut fixture_bindings = Vec::new();
    let mut fixture_req_exprs = Vec::new();
    let mut clean_params = Vec::new();

    for param in &func.sig.inputs {
        if let FnArg::Typed(pat_type) = param {
            let has_fixture = pat_type.attrs.iter().any(|a| a.path().is_ident("fixture"));
            if has_fixture {
                let binding = &pat_type.pat;
                let ty = strip_reference(&pat_type.ty);
                fixture_bindings.push((binding.clone(), ty.clone()));
                fixture_req_exprs.push(ty.clone());

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
    let call_args: Vec<_> = fixture_bindings.iter().map(|(binding, _)| binding).collect();
    let has_fixtures = !fixture_bindings.is_empty();

    let fixture_requires_expr = if has_fixtures {
        quote! { &[#(<#fixture_req_exprs as ::testutil::FixtureStorage>::REQUIRES),*] }
    } else {
        quote! { &[] }
    };

    let body_expr = if has_fixtures {
        quote! {
            || {
                #(#fixture_setup)*
                #name(#(#call_args),*);
            }
        }
    } else {
        quote! { || { #name(); } }
    };

    let expanded = quote! {
        #(#attrs)*
        #vis #fn_token #name(#(#clean_params),*) #ret #block

        ::testutil::inventory::submit!(::testutil::TestDef {
            name: #name_str,
            display_name: #display_name_expr,
            requires: &[#(#req_exprs),*],
            fixture_requires: #fixture_requires_expr,
            ignore: #ignore_expr,
            body: #body_expr,
        });
    };
    expanded.into()
}

/// Build the `display_name: Option<&'static str>` expression.
///
/// - No labels, no name → `None`
/// - Labels and/or name → `Some("[label1][label2] display name")`
fn build_display_name(fn_name: &str, custom_name: &Option<String>, labels: &[Ident]) -> proc_macro2::TokenStream {
    if labels.is_empty() && custom_name.is_none() {
        return quote! { None };
    }

    let mut display = String::new();
    for label in labels {
        display.push('[');
        display.push_str(&label.to_string());
        display.push(']');
    }
    if !labels.is_empty() {
        display.push(' ');
    }
    match custom_name {
        Some(n) => display.push_str(n),
        None => display.push_str(fn_name),
    }
    quote! { Some(#display) }
}

// #[testutil::fixture] ========================================================

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

    let ty = &impl_block.self_ty;
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

// Helpers =====================================================================

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
