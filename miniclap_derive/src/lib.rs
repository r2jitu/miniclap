extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::{format_ident, quote};
use syn::{Field, Ident, Lit, Meta, NestedMeta};

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

struct App {
    by_pos: Vec<Arg>,
    by_name: Vec<NamedArg>,
}

struct Arg {
    name: Ident,
}

struct NamedArg {
    arg: Arg,
    short: Option<String>,
    long: Option<String>,
}

fn extract_options(fields: &mut dyn Iterator<Item = &Field>) -> App {
    let mut by_pos = Vec::new();
    let mut by_name = Vec::new();
    for f in fields {
        let attrs = attrs_from_field(f);
        println!(
            "Attrs for `{}` are: {:?}",
            f.ident.as_ref().unwrap().to_string(),
            attrs
        );

        let mut short = None;
        let mut long = None;

        for a in attrs {
            match a {
                Attr::Short(c) => short = Some(c.to_string()),
                Attr::Long(name) => long = Some(name),
            }
        }

        let arg = Arg {
            name: f.ident.clone().unwrap(),
        };

        if short.is_some() || long.is_some() {
            by_name.push(NamedArg { arg, short, long });
        } else {
            by_pos.push(arg);
        }
    }
    App { by_pos, by_name }
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

    let mut name_matches = Vec::new();
    for arg in opts.by_name {
        let name = &arg.arg.name;
        let arg_name = format_ident!("arg_{}", name);
        let missing = format!("Missing argument `{}`", name);
        let short = arg.short.map(|x| format!("-{}", x));
        let long = arg.long.map(|x| format!("--{}", x));
        let pattern = match (short, long) {
            (Some(short), Some(long)) => quote! { #short | #long },
            (Some(short), None) => quote! { #short },
            (None, Some(long)) => quote! { #long },
            _ => panic!(),
        };
        let assign = quote! {{
            let value_os = args.next().expect("Missing value for argument");
            let value = value_os.to_str().expect("Invalid string");
            #arg_name = Some(value.parse().expect("Invalid argument type"))
        }};
        decls.push(quote! { let mut #arg_name = None; });
        name_matches.push(quote! { #pattern => #assign });
        fields.push(quote! { #name: #arg_name.expect(#missing) });
    }
    let name_matches = if name_matches.len() > 0 {
        quote! {
            match arg {
                #(#name_matches),*,
                _ => panic!("Invalid named argument"),
            }
        }
    } else {
        quote! { panic!("No named args expected") }
    };

    let mut pos_matches = Vec::new();
    for (i, arg) in opts.by_pos.iter().enumerate() {
        let name = &arg.name;
        let arg_name = format_ident!("arg_{}", name);
        let missing = format!("Missing argument `{}`", name);
        decls.push(quote! { let mut #arg_name = None; });
        pos_matches.push(quote! { #i => #arg_name = Some(arg.parse().expect("Invalid argument type")) });
        fields.push(quote! { #name: #arg_name.expect(#missing) });
    }
    let pos_matches = if pos_matches.len() > 0 {
        quote! {
            match num_args {
                #(#pos_matches),*,
                _ => panic!("Too many args"),
            }
            num_args += 1;
        }
    } else {
        quote! { panic!("No positional args expected") }
    };
    if opts.by_pos.len() > 0 {
        decls.push(quote! { let mut num_args = 0; });
    }

    let name = &input.ident;
    quote!(
        impl ::miniclap::MiniClap for #name {
            fn parse_internal(args: &mut dyn Iterator<Item = ::std::ffi::OsString>) -> Self {
                #(#decls)*
                let _bin_name = args.next();
                while let Some(arg_os) = args.next() {
                    let arg = arg_os.to_str().unwrap();
                    if arg.chars().next() == Some('-') {
                        #name_matches
                    } else {
                        #pos_matches
                    }
                }
                Self {
                    #(#fields),*
                }
            }
        }
    )
    .into()
}
