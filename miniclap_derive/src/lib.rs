extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::{format_ident, quote};
use syn::{Field, Ident, Lit, Meta, NestedMeta};

struct App {
    args: Vec<Arg>,
}

struct Arg {
    name: Ident,
}

#[derive(Debug)]
enum Attr {
    Short(char),
    Long(String),
}

fn attrs_from_field(f: &Field) -> Vec<Attr> {
    let field_name = f.ident.as_ref().unwrap().to_string();
    f.attrs
        .iter()
        // Only process attributes for this crate.
        .filter(|a| a.path.is_ident("miniclap"))
        // Extract nested attributes across all the attributes.
        .flat_map(|a| match a.parse_meta() {
            Ok(Meta::List(list)) => list.nested,
            _ => abort!(a, "Attribute must be a structured list"),
        })
        // Ensure that each attribute is a structured format, not a literal.
        .map(|m| match m {
            NestedMeta::Meta(m) => m,
            NestedMeta::Lit(l) => abort!(l, "Literals are not valid attributes"),
        })
        // Parse the attribute
        .map(|m| {
            let name = match m.path().get_ident() {
                Some(id) => id.to_string(),
                None => abort!(m.path(), "Invalid attribute name"),
            };
            match name.as_str() {
                "short" => Attr::Short(match m {
                    Meta::Path(_) => field_name.chars().next().unwrap(),
                    Meta::NameValue(name) => match name.lit {
                        Lit::Str(lit_str) => {
                            let val = lit_str.value();
                            if val.len() > 1 {
                                abort!(lit_str, "`short` may only have a single character")
                            }
                            val.chars().next().unwrap()
                        }
                        Lit::Char(lit_char) => lit_char.value(),
                        _ => abort!(name.lit, "Only string or char allowed for `short`"),
                    },
                    _ => abort!(m, "Invalid specification for `short`"),
                }),
                "long" => Attr::Long(match m {
                    Meta::Path(_) => field_name.clone(),
                    Meta::NameValue(name) => match name.lit {
                        Lit::Str(lit_str) => lit_str.value(),
                        _ => abort!(name.lit, "Only string allowed for `long`"),
                    },
                    _ => abort!(m, "Invalid specification for `long`"),
                }),
                _ => abort!(m.path(), "Unknown attribute"),
            }
        })
        .collect()
}

fn extract_options(fields: &mut dyn Iterator<Item = &Field>) -> App {
    let mut args = Vec::new();
    for f in fields {
        let attrs = attrs_from_field(f);
        println!(
            "Attrs for `{}` are: {:?}",
            f.ident.as_ref().unwrap().to_string(),
            attrs
        );
        args.push(Arg {
            name: f.ident.clone().unwrap(),
        });
    }
    App { args }
}

#[proc_macro_derive(MiniClap, attributes(miniclap))]
#[proc_macro_error]
pub fn derive_miniclap(input: TokenStream) -> TokenStream {
    let input: syn::DeriveInput = syn::parse_macro_input!(input);
    let opts = match input.data {
        syn::Data::Struct(syn::DataStruct {
            fields: syn::Fields::Named(ref fields),
            ..
        }) => extract_options(&mut fields.named.iter()),
        _ => {
            panic!("`#[derive(MiniClap)]` only works for non-tuple structs");
        }
    };

    let mut decls = Vec::new();
    let mut fields = Vec::new();

    // let mut name_matches = Vec::new();
    // for arg in opts.flags {
    //     todo!()
    // }
    // name_matches.push(quote! { _ => () });

    let mut pos_matches = Vec::new();
    for (i, arg) in opts.args.iter().enumerate() {
        let name = &arg.name;
        let arg_name = format_ident!("arg_{}", &arg.name);
        let missing = format!("Missing argument `{}`", name);
        decls.push(quote! { let mut #arg_name = None; });
        pos_matches.push(quote! { #i => #arg_name = Some(arg.parse().unwrap()) });
        fields.push(quote! { #name: #arg_name.expect(#missing) });
    }
    pos_matches.push(quote! { _ => panic!("Too many args") });

    let name = &input.ident;
    let gen = quote! {
        impl ::miniclap::MiniClap for #name {
            fn parse_internal(args: &mut dyn Iterator<Item = ::std::ffi::OsString>) -> Self {
                #(#decls)*
                let mut num_args = 0;
                let _bin_name = args.next();
                for arg in args {
                    let arg = arg.to_str().unwrap();

                    // By name
                    // match arg {
                    //     #(#name_matches),*
                    // }

                    // By position
                    match num_args {
                        #(#pos_matches),*
                    }
                    num_args += 1;
                }
                Self {
                    #(#fields),*
                }
            }
        }
    };
    gen.into()
}
