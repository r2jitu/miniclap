extern crate proc_macro;

use proc_macro2::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::{format_ident, quote};
use syn::{Field, Ident, Lit, Meta};

#[derive(Debug)]
enum Attr {
    Short(char),
    Long(String),
}

impl Attr {
    fn from_field_attribute(field: &Field, attribute: &Meta) -> Attr {
        let field_name = field.ident.as_ref().unwrap().to_string();
        let attr_name = match attribute.path().get_ident() {
            Some(id) => id.to_string(),
            None => abort!(attribute.path(), "Invalid attribute name"),
        };
        match attr_name.as_str() {
            "short" => Attr::Short(match attribute {
                Meta::Path(_) => field_name.chars().next().unwrap(),
                Meta::NameValue(mnv) => match mnv.lit {
                    Lit::Str(ref lit_str) => {
                        let val = lit_str.value();
                        if val.len() > 1 {
                            abort!(lit_str, "`short` may only have a single character")
                        }
                        val.chars().next().unwrap()
                    }
                    Lit::Char(ref lit_char) => lit_char.value(),
                    _ => abort!(mnv.lit, "Only string or char allowed for `short`"),
                },
                _ => abort!(attribute, "Invalid specification for `short`"),
            }),
            "long" => Attr::Long(match attribute {
                Meta::Path(_) => field_name.into(),
                Meta::NameValue(mnv) => match mnv.lit {
                    Lit::Str(ref lit_str) => lit_str.value(),
                    _ => abort!(mnv.lit, "Only string allowed for `long`"),
                },
                _ => abort!(attribute, "Invalid specification for `long`"),
            }),
            _ => abort!(attribute.path(), "Unknown attribute"),
        }
    }

    fn from_field(field: &Field) -> Vec<Attr> {
        field
            .attrs
            .iter()
            // Only process attributes for this crate.
            .filter(|a| a.path.is_ident("miniclap"))
            // Extract nested attributes across all the attributes.
            .flat_map(|a| match a.parse_meta() {
                Ok(Meta::List(list)) => list.nested,
                _ => abort!(a, "Attribute must be a structured list"),
            })
            // Ensure that each attribute is a structured format, not a literal.
            .map(|nm| match nm {
                syn::NestedMeta::Meta(m) => m,
                syn::NestedMeta::Lit(l) => abort!(l, "Literals are not valid attributes"),
            })
            // Parse the attribute
            .map(|meta| Attr::from_field_attribute(&field, &meta))
            .collect()
    }
}

struct Arg {
    name: Ident,
}

/// Argument that is specified by a switch (e.g. -n, --num).
struct SwitchedArg {
    arg: Arg,
    short: Option<String>,
    long: Option<String>,
}

struct App {
    by_position: Vec<Arg>,
    by_switch: Vec<SwitchedArg>,
}

impl App {
    fn from_named_fields(fields: &syn::FieldsNamed) -> App {
        let mut by_position = Vec::new();
        let mut by_switch = Vec::new();
        for f in &fields.named {
            let field_ident = f.ident.clone().unwrap();
            let attrs = Attr::from_field(f);
            println!("Attrs for `{}` are: {:?}", field_ident.to_string(), attrs);

            let mut short = None;
            let mut long = None;

            for a in attrs {
                match a {
                    Attr::Short(c) => short = Some(c.to_string()),
                    Attr::Long(name) => long = Some(name),
                }
            }

            let arg = Arg { name: field_ident };

            if short.is_some() || long.is_some() {
                by_switch.push(SwitchedArg { arg, short, long });
            } else {
                by_position.push(arg);
            }
        }
        App {
            by_position,
            by_switch,
        }
    }

    fn from_derive_input(input: &syn::DeriveInput) -> App {
        match input.data {
            syn::Data::Struct(syn::DataStruct {
                fields: syn::Fields::Named(ref fields),
                ..
            }) => App::from_named_fields(fields),
            _ => {
                abort!(
                    input,
                    "`#[derive(MiniClap)]` only works for non-tuple structs"
                );
            }
        }
    }
}

fn gen_switch_matcher(
    args: &[SwitchedArg],
    decls: &mut Vec<TokenStream>,
    fields: &mut Vec<TokenStream>,
) -> TokenStream {
    let mut matches = Vec::new();
    for arg in args {
        let name = &arg.arg.name;

        let short = arg.short.as_ref().map(|x| format!("-{}", x));
        let long = arg.long.as_ref().map(|x| format!("--{}", x));
        let pattern = match (short, long) {
            (Some(short), Some(long)) => quote! { #short | #long },
            (Some(short), None) => quote! { #short },
            (None, Some(long)) => quote! { #long },
            _ => unreachable!(),
        };

        let name_string = name.to_string();
        let parse = quote! {
            get_value(#name_string).parse().expect("Invalid argument type")
        };

        let arg_var = format_ident!("arg_{}", name);
        let missing = format!("Missing argument `{}`", name);
        decls.push(quote! { let mut #arg_var = None; });
        matches.push(quote! { #pattern => #arg_var = Some(#parse) });
        fields.push(quote! { #name: #arg_var.expect(#missing) });
    }

    quote! {
        let value: Option<String> = arg.find("=").map(|i| {
            let (x, y) = arg.split_at(i);
            arg = x;
            y[1..].into()
        });
        let get_value = |name: &str| value.unwrap_or_else(|| {
            let value_os = args.next().expect(&format!("Missing value for `{}`", name));
            value_os.into_string().expect(&format!("Value for `{}` is an invalid string", name))
        });
        match arg {
            #(#matches),*,
            _ => panic!("Invalid switched argument"),
        }
    }
}

fn gen_position_matcher(
    args: &[Arg],
    decls: &mut Vec<TokenStream>,
    fields: &mut Vec<TokenStream>,
) -> TokenStream {
    let mut position_matches = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let name = &arg.name;
        let arg_var = format_ident!("arg_{}", name);
        let missing = format!("Missing argument `{}`", name);
        let parse = quote! { arg.parse().expect("Invalid argument type") };
        decls.push(quote! { let mut #arg_var = None; });
        position_matches.push(quote! { #i => #arg_var = Some(#parse) });
        fields.push(quote! { #name: #arg_var.expect(#missing) });
    }
    if args.len() > 0 {
        decls.push(quote! { let mut num_args = 0; });
    }

    quote! {
        match num_args {
            #(#position_matches),*,
            _ => panic!("Too many positional argumments"),
        }
        num_args += 1;
    }
}

#[proc_macro_derive(MiniClap, attributes(miniclap))]
#[proc_macro_error]
pub fn derive_miniclap(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: syn::DeriveInput = syn::parse_macro_input!(input);
    let app = App::from_derive_input(&input);

    let mut decls = Vec::new();
    let mut fields = Vec::new();
    let switch_matcher = gen_switch_matcher(&app.by_switch, &mut decls, &mut fields);
    let position_matcher = gen_position_matcher(&app.by_position, &mut decls, &mut fields);

    let name = &input.ident;
    quote!(
        impl ::miniclap::MiniClap for #name {
            fn parse_internal(mut args: &mut dyn Iterator<Item = ::std::ffi::OsString>) -> Self {
                #(#decls)*

                let _bin_name = args.next();
                while let Some(arg_os) = args.next() {
                    let mut arg: &str = &arg_os.to_str().unwrap();
                    if arg.chars().next() == Some('-') {
                        #switch_matcher
                    } else {
                        #position_matcher
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
