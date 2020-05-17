extern crate proc_macro;

use proc_macro2::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::{format_ident, quote};
use syn::{Field, Ident, Lit, Meta};

#[derive(Debug)]
enum Attr {
    Short(char),
    Long(String),
    DefaultValue(Lit),
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
                Meta::Path(_) => field_name,
                Meta::NameValue(mnv) => match mnv.lit {
                    Lit::Str(ref lit_str) => lit_str.value(),
                    _ => abort!(mnv.lit, "Only string allowed for `long`"),
                },
                _ => abort!(attribute, "Invalid specification for `long`"),
            }),
            "default_value" => Attr::DefaultValue(match attribute {
                Meta::NameValue(mnv) => mnv.lit.clone(),
                _ => abort!(attribute, "Attribute must be used as `default_value = ...`"),
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
    default_value: Option<Lit>,
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
            let ident = f.ident.clone().unwrap();
            let attrs = Attr::from_field(f);
            println!("Attrs for `{}` are: {:?}", ident.to_string(), attrs);

            let mut short = None;
            let mut long = None;
            let mut default_value = None;

            for a in attrs {
                // TODO: Validate the options aren't used multiple times
                match a {
                    Attr::Short(c) => short = Some(c.to_string()),
                    Attr::Long(name) => long = Some(name),
                    Attr::DefaultValue(lit) => default_value = Some(lit),
                }
            }

            let arg = Arg {
                name: ident,
                default_value,
            };

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

impl Arg {
    fn declare(&self, arg_var: &Ident) -> TokenStream {
        match &self.default_value {
            Some(lit) => quote! { let mut #arg_var = #lit; },
            None => quote! { let mut #arg_var = None; },
        }
    }

    fn assign(&self, arg_var: &Ident, value: TokenStream) -> TokenStream {
        match &self.default_value {
            Some(_) => quote! { #arg_var = #value },
            None => quote! { #arg_var = Some(#value) },
        }
    }

    fn retrieve(&self, arg_var: &Ident) -> TokenStream {
        let name_string = self.name.to_string();
        match &self.default_value {
            Some(_) => quote! { #arg_var },
            None => quote! {
                #arg_var.ok_or_else(|| Error::missing_required_argument(#name_string))?
            },
        }
    }
}

impl std::ops::Deref for SwitchedArg {
    type Target = Arg;

    fn deref(&self) -> &Self::Target {
        &self.arg
    }
}

struct Generator {
    decls: Vec<TokenStream>,
    fields: Vec<TokenStream>,
    post_matching: Vec<TokenStream>,
}

impl Generator {
    fn gen_switch_matcher(&mut self, args: &[SwitchedArg]) -> TokenStream {
        let mut matches = Vec::new();
        for arg in args {
            let name = &arg.arg.name;
            let arg_var = format_ident!("arg_{}", name);
            self.decls.push(arg.declare(&arg_var));

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
                ::miniclap::__get_value(#name_string, opt_value, &mut args)?.parse()
                    .map_err(|e| Error::parse_failed(#name_string, Box::new(e)))?
            };
            let assign = arg.assign(&arg_var, parse);
            matches.push(quote! { #pattern => #assign });

            let retrieve = arg.retrieve(&arg_var);
            self.fields.push(quote! { #name: #retrieve });
        }

        quote! {
            let opt_value = ::miniclap::__split_arg_value(&mut arg);
            match arg {
                #(#matches),*,
                _ => return Err(Error::unknown_argument(arg)),
            }
        }
    }

    fn gen_position_matcher(&mut self, args: &[Arg]) -> TokenStream {
        let mut position_matches = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let name = &arg.name;
            let arg_var = format_ident!("arg_{}", name);
            self.decls.push(arg.declare(&arg_var));

            let name_string = name.to_string();
            let parse = quote! {
                arg.parse().map_err(|e| Error::parse_failed(#name_string, Box::new(e)))?
            };
            let assign = arg.assign(&arg_var, parse);
            position_matches.push(quote! { #i => #assign });

            let retrieve = arg.retrieve(&arg_var);
            self.fields.push(quote! { #name: #retrieve });
        }
        self.decls.push(quote! { let mut num_args = 0; });

        quote! {
            match num_args {
                #(#position_matches),*,
                _ => return Err(Error::too_many_arguments(arg)),
            }
            num_args += 1;
        }
    }

    fn gen_impl(name: &Ident, app: &App) -> TokenStream {
        let mut this = Generator {
            decls: Vec::new(),
            fields: Vec::new(),
            post_matching: Vec::new(),
        };

        let switch_matcher = this.gen_switch_matcher(&app.by_switch);
        let position_matcher = this.gen_position_matcher(&app.by_position);
        let decls = &this.decls;
        let fields = &this.fields;
        let post_matching = &this.post_matching;
        quote!(
            impl ::miniclap::MiniClap for #name {
                fn __parse_internal(mut args: &mut dyn Iterator<Item = ::std::ffi::OsString>) -> Result<Self, ::miniclap::Error> {
                    use ::std::string::String;
                    use ::std::option::Option::{self, Some, None};
                    use ::std::result::Result::{Ok, Err};
                    use ::miniclap::{Error, Result};

                    #(#decls)*

                    let _bin_name = args.next();
                    while let Some(arg_os) = args.next() {
                        let mut arg: &str = &arg_os.to_str().ok_or_else(Error::invalid_utf8)?;
                        if arg.chars().next() == Some('-') {
                            #switch_matcher
                        } else {
                            #position_matcher
                        }
                    }

                    #(#post_matching)*

                    Ok(Self {
                        #(#fields),*
                    })
                }
            }
        )
    }
}

#[proc_macro_derive(MiniClap, attributes(miniclap))]
#[proc_macro_error]
pub fn derive_miniclap(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: syn::DeriveInput = syn::parse_macro_input!(input);
    let app = App::from_derive_input(&input);
    let name = &input.ident;
    Generator::gen_impl(name, &app).into()
}
