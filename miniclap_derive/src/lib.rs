extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Field, Ident};

struct App {
    args: Vec<Arg>,
    // TODO: Add flags, options, subcommands
}

struct Arg {
    name: Ident,
}

fn extract_options(fields: &mut dyn Iterator<Item = &Field>) -> App {
    let mut args = Vec::new();
    for f in fields {
        args.push(Arg {
            name: f.ident.clone().unwrap(),
        });
    }
    App { args }
}

#[proc_macro_derive(MiniClap)]
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
    let mut matches = Vec::new();
    let mut fields = Vec::new();

    for (i, arg) in opts.args.iter().enumerate() {
        let name = &arg.name;
        let arg_name = format_ident!("arg_{}", &arg.name);
        let missing = format!("Missing argument `{}`", name);
        decls.push(quote! { let mut #arg_name = None; });
        matches.push(quote! { #i => #arg_name = Some(arg.parse().unwrap()) });
        fields.push(quote! { #name: #arg_name.expect(#missing) });
    }
    matches.push(quote! { _ => panic!("Too many args") });

    let name = &input.ident;
    let gen = quote! {
        impl ::miniclap::MiniClap for #name {
            fn parse_internal(args: &mut dyn Iterator<Item = ::std::ffi::OsString>) -> Self {
                #(#decls)*
                let mut num_args = 0;
                let _bin_name = args.next();
                for arg in args {
                    let arg = arg.to_str().unwrap();
                    match num_args {
                        #(#matches),*
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
