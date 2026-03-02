use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    bracketed, parse_macro_input, punctuated::Punctuated, FnArg, Ident, ItemFn, LitStr, Path, ReturnType, Token, Type,
};

// #[testutil::test] argument parsing ==========================================

/// Parsed arguments for `#[testutil::test(...)]`.
#[derive(Default)]
struct TestArgs {
    requires: Vec<Path>,
    name: Option<String>,
    labels: Option<Vec<Ident>>,
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
                    args.labels = Some(
                        Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
                            .into_iter()
                            .collect(),
                    );
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
/// Parameters annotated with `#[fixture]` are automatically injected. An optional
/// type argument specifies the fixture type when it differs from the parameter type
/// (e.g. `#[fixture(TestName)] name: &str` uses deref coercion).
///
/// ```ignore
/// #[testutil::test(requires = [preconditions::valgrind], labels = [slow])]
/// fn my_test(#[fixture] dir: &TempDir) { /* ... */ }
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

    // Build display_name from custom name (labels go in `kind`, not in the name).
    let display_name_expr = build_display_name(&args.name);

    // Build labels. None = inherit module defaults; Some([]) = explicit opt-out.
    let labels_explicit = args.labels.is_some();
    let label_strs: Vec<String> = args
        .labels
        .unwrap_or_default()
        .iter()
        .map(|id| id.to_string())
        .collect();

    // Build ignore expression.
    let ignore_expr = match &args.ignore {
        IgnoreArg::No => quote! { ::testutil::Ignore::No },
        IgnoreArg::Yes => quote! { ::testutil::Ignore::Yes },
        IgnoreArg::WithReason(reason) => quote! { ::testutil::Ignore::WithReason(#reason) },
    };

    let req_exprs: Vec<_> = args.requires.iter().collect();

    // Collect #[fixture] / #[fixture(Type)] parameters.
    let mut fixture_bindings = Vec::new(); // (binding_pat, fixture_type, param_type)
    let mut fixture_req_exprs = Vec::new();
    let mut clean_params = Vec::new();

    for param in &func.sig.inputs {
        if let FnArg::Typed(pat_type) = param {
            let fixture_attr = pat_type.attrs.iter().find(|a| a.path().is_ident("fixture"));
            if let Some(attr) = fixture_attr {
                let binding = &pat_type.pat;
                let param_ty = &pat_type.ty;

                // Determine fixture type: from attribute arg or from parameter type.
                let fixture_ty = parse_fixture_type_arg(attr).unwrap_or_else(|| strip_reference(param_ty));

                fixture_bindings.push((binding.clone(), fixture_ty.clone(), param_ty.clone()));
                fixture_req_exprs.push(fixture_ty);

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

    // Build fixture injection code: call setup() directly, hold value in a local.
    let fixture_setup: Vec<_> = fixture_bindings
        .iter()
        .enumerate()
        .map(|(i, (binding, fixture_ty, param_ty))| {
            let inst_name = format_ident!("__fixture_inst_{}", i);
            quote! {
                let #inst_name = <#fixture_ty as ::testutil::Fixture>::setup()
                    .expect("fixture setup failed");
                let #binding: #param_ty = &#inst_name;
            }
        })
        .collect();

    // Emit the original function with cleaned parameters.
    let vis = &func.vis;
    let block = &func.block;
    let ret = &func.sig.output;
    let fn_token = &func.sig.fn_token;
    let attrs: Vec<_> = func.attrs.iter().collect();
    let call_args: Vec<_> = fixture_bindings.iter().map(|(binding, _, _)| binding).collect();
    let has_fixtures = !fixture_bindings.is_empty();

    let fixture_requires_expr = if has_fixtures {
        quote! { &[#(<#fixture_req_exprs as ::testutil::FixtureMeta>::REQUIRES),*] }
    } else {
        quote! { &[] }
    };

    // Body always sets the test context, then optionally injects fixtures.
    let body_expr = if has_fixtures {
        quote! {
            || {
                ::testutil::set_current_test(#name_str, ::core::module_path!());
                #(#fixture_setup)*
                #name(#(#call_args),*);
            }
        }
    } else {
        quote! {
            || {
                ::testutil::set_current_test(#name_str, ::core::module_path!());
                #name();
            }
        }
    };

    let expanded = quote! {
        #(#attrs)*
        #vis #fn_token #name(#(#clean_params),*) #ret #block

        ::testutil::inventory::submit!(::testutil::TestDef {
            name: #name_str,
            module: ::core::module_path!(),
            display_name: #display_name_expr,
            requires: &[#(#req_exprs),*],
            fixture_requires: #fixture_requires_expr,
            ignore: #ignore_expr,
            labels: &[#(#label_strs),*],
            labels_explicit: #labels_explicit,
            body: #body_expr,
        });
    };
    expanded.into()
}

/// Build the `display_name: Option<&'static str>` expression.
///
/// - No custom name → `None` (runner uses function name)
/// - Custom name → `Some("display name")`
fn build_display_name(custom_name: &Option<String>) -> proc_macro2::TokenStream {
    match custom_name {
        Some(n) => quote! { Some(#n) },
        None => quote! { None },
    }
}

// #[testutil::fixture] ========================================================

/// Define a fixture from a function. The return type (inside `Result`) is the
/// fixture type; `#[fixture]` parameters are dependencies injected before the
/// body runs.
///
/// ```ignore
/// #[testutil::fixture(docker_available)]
/// fn temp_dir(#[fixture(TestName)] name: &str) -> Result<TempDir, String> {
///     // ...
/// }
/// ```
///
/// Generates `impl Fixture for TempDir` and `impl FixtureMeta for TempDir`.
#[proc_macro_attribute]
pub fn fixture(attr: TokenStream, item: TokenStream) -> TokenStream {
    let reqs = parse_macro_input!(attr with Punctuated::<Path, Token![,]>::parse_terminated);
    let mut func = parse_macro_input!(item as ItemFn);

    let req_exprs: Vec<_> = reqs.iter().collect();

    // Extract the fixture type from the return type: Result<T, String> → T.
    let fixture_ty = match extract_result_ok_type(&func.sig.output) {
        Some(ty) => ty,
        None => {
            return syn::Error::new_spanned(&func.sig.output, "fixture function must return Result<T, String>")
                .to_compile_error()
                .into();
        }
    };

    // Collect #[fixture] / #[fixture(Type)] parameters on the setup function.
    let mut dep_bindings = Vec::new(); // (binding_pat, fixture_type, param_type)
    let mut clean_inputs = syn::punctuated::Punctuated::new();

    for param in &func.sig.inputs {
        if let FnArg::Typed(pat_type) = param {
            let fixture_attr = pat_type.attrs.iter().find(|a| a.path().is_ident("fixture"));
            if let Some(attr) = fixture_attr {
                let binding = &pat_type.pat;
                let param_ty = &pat_type.ty;
                let dep_fixture_ty = parse_fixture_type_arg(attr).unwrap_or_else(|| strip_reference(param_ty));
                dep_bindings.push((binding.clone(), dep_fixture_ty, param_ty.clone()));
            } else {
                clean_inputs.push(param.clone());
            }
        } else {
            clean_inputs.push(param.clone());
        }
    }

    // Strip #[fixture] params from the function signature.
    func.sig.inputs = clean_inputs;

    // Prepend dependency injection code to the function body.
    if !dep_bindings.is_empty() {
        let dep_setup: Vec<_> = dep_bindings
            .iter()
            .enumerate()
            .map(|(i, (binding, fixture_ty, param_ty))| {
                let inst_name = format_ident!("__fixture_dep_{}", i);
                quote! {
                    let #inst_name = <#fixture_ty as ::testutil::Fixture>::setup()
                        .expect("fixture dependency setup failed");
                    let #binding: #param_ty = &#inst_name;
                }
            })
            .collect();

        let original_stmts = &func.block.stmts;
        func.block = syn::parse_quote!({
            #(#dep_setup)*
            #(#original_stmts)*
        });
    }

    let func_name = &func.sig.ident;

    let expanded = quote! {
        #func

        impl ::testutil::Fixture for #fixture_ty {
            fn setup() -> Result<Self, String> {
                #func_name()
            }
        }

        impl ::testutil::FixtureMeta for #fixture_ty {
            const REQUIRES: &'static [fn() -> Result<(), String>] = &[#(#req_exprs),*];
        }
    };

    expanded.into()
}

// Helpers =====================================================================

/// Strip a leading `&` from a type (e.g. `&TempDir` → `TempDir`).
fn strip_reference(ty: &Type) -> Type {
    if let Type::Reference(r) = ty {
        (*r.elem).clone()
    } else {
        ty.clone()
    }
}

/// Parse the optional type argument from `#[fixture(Type)]`.
/// Returns `None` for bare `#[fixture]`.
fn parse_fixture_type_arg(attr: &syn::Attribute) -> Option<Type> {
    match &attr.meta {
        syn::Meta::List(list) => syn::parse2::<Type>(list.tokens.clone()).ok(),
        _ => None,
    }
}

/// Extract `T` from a return type of `Result<T, String>` or `Result<T, E>`.
fn extract_result_ok_type(ret: &ReturnType) -> Option<Type> {
    let ReturnType::Type(_, ty) = ret else {
        return None;
    };
    let Type::Path(type_path) = ty.as_ref() else {
        return None;
    };
    let last_seg = type_path.path.segments.last()?;
    if last_seg.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last_seg.arguments else {
        return None;
    };
    let first_arg = args.args.first()?;
    let syn::GenericArgument::Type(ok_ty) = first_arg else {
        return None;
    };
    Some(ok_ty.clone())
}
