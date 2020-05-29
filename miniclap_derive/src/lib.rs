extern crate proc_macro;

use proc_macro2::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::{format_ident, quote};
use std::collections::BTreeSet;
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

    fn all_from_field(field: &Field) -> Vec<(Meta, Attr)> {
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
            .map(|meta| {
                let attr = Attr::from_field_attribute(&field, &meta);
                (meta, attr)
            })
            .collect()
    }
}

struct Arg {
    name: Ident,
    index: Option<usize>,
    short: Option<char>,
    long: Option<String>,
    default_value: Option<Lit>,
    is_flag: bool,
    is_required: bool,
    is_multiple: bool,
}

struct App {
    by_position: Vec<Arg>,
    by_switch: Vec<Arg>,
}

impl App {
    fn from_named_fields(fields: &syn::FieldsNamed) -> App {
        let mut by_position: Vec<Arg> = Vec::new();
        let mut by_switch: Vec<Arg> = Vec::new();
        let mut short_switches = BTreeSet::new();
        let mut long_switches = BTreeSet::new();
        for f in &fields.named {
            let ident = f.ident.clone().unwrap();
            let attrs = Attr::all_from_field(f);

            let mut short = None;
            let mut long = None;
            let mut default_value = None;

            for (m, a) in attrs {
                match a {
                    Attr::Short(c) => {
                        if short.replace(c).is_some() {
                            abort!(m, "May only specify once");
                        }
                        if !short_switches.insert(c) {
                            abort!(m, "Short already used");
                        }
                    }
                    Attr::Long(name) => {
                        if long.replace(name.clone()).is_some() {
                            abort!(m, "May only specify once");
                        }
                        if !long_switches.insert(name) {
                            abort!(m, "Long already used");
                        }
                    }
                    Attr::DefaultValue(lit) => {
                        if default_value.replace(lit).is_some() {
                            abort!(m, "May only specify once");
                        }
                    }
                }
            }

            let index = if short.is_none() && long.is_none() {
                Some(by_position.len())
            } else {
                None
            };
            let mut is_required = true;
            let mut is_flag = false;
            let mut is_multiple = false;

            match f.ty {
                syn::Type::Path(syn::TypePath {
                    path: syn::Path { ref segments, .. },
                    ..
                }) => match segments.last().unwrap().ident.to_string().as_str() {
                    "Option" => {
                        is_required = false;
                    }
                    "Vec" => {
                        is_multiple = true;
                        is_required = false;
                    }
                    "bool" => {
                        if index.is_none() {
                            is_required = false;
                            is_flag = true;
                        }
                    }
                    _ => (),
                },
                _ => todo!(),
            }

            let arg = Arg {
                name: ident,
                index,
                short,
                long,
                default_value,
                is_flag,
                is_required,
                is_multiple,
            };

            if index.is_some() {
                if let Some(prev) = by_position.last() {
                    if is_required && !prev.is_required {
                        abort!(
                            f.ty,
                            "Required positional argument may not follow optional/multiple \
                            positional argument"
                        );
                    } else if prev.is_multiple {
                        abort!(
                            f,
                            "Previous positional argument was multiple so no other positional args \
                            may follow"
                        );
                    }
                }

                by_position.push(arg);
            } else {
                by_switch.push(arg);
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
    fn arg_var(&self) -> Ident {
        format_ident!("arg_{}", &self.name)
    }

    fn declare(&self) -> TokenStream {
        let arg_var = self.arg_var();
        if self.is_flag {
            quote! { let mut #arg_var = false; }
        } else if self.is_multiple {
            quote! { let mut #arg_var = Vec::new(); }
        } else if let Some(lit) = &self.default_value {
            quote! { let mut #arg_var = #lit; }
        } else {
            quote! { let mut #arg_var = None; }
        }
    }

    fn field(&self) -> TokenStream {
        let arg_var = self.arg_var();
        let retrieve = if self.is_flag {
            quote! { #arg_var }
        } else {
            let name_string = self.name.to_string();
            match (self.is_multiple, &self.default_value, self.is_required) {
                (false, Some(_), _) => quote! { #arg_var },
                (_, None, false) => quote! { #arg_var },
                (false, None, true) => quote! {
                    #arg_var.ok_or_else(|| Error::missing_required_argument(#name_string))?
                },
                (true, Some(lit), false) => quote! {{
                    if #arg_var.is_empty() {
                        #arg_var.push(#lit);
                    }
                    #arg_var
                }},
                (true, _, true) => unreachable!("Currently no way to express multiple + required."),
            }
        };
        let name = &self.name;
        quote! { #name: #retrieve }
    }

    fn handler(&self) -> TokenStream {
        let name_string = self.name.to_string();
        let short = match self.short {
            Some(c) => quote! { Some(#c) },
            None => quote! { None },
        };
        let long = match self.long {
            Some(ref l) => quote! { Some(#l) },
            None => quote! { None },
        };
        let arg_var = self.arg_var();
        if self.is_flag {
            quote! {
                FlagHandler {
                    name: #name_string,
                    short: #short,
                    long: #long,
                    assign: &RefCell::new(|| Ok(#arg_var = true)),
                }
            }
        } else {
            let value = quote! { value };
            let parse = quote! {
                #value.parse().map_err(|e| Error::parse_failed(#name_string, Box::new(e)))?
            };
            let store = match (self.is_multiple, &self.default_value) {
                (false, Some(_)) => quote! { #arg_var = #parse },
                (false, None) => quote! { #arg_var = Some(#parse) },
                (true, _) => quote! { #arg_var.push(#parse) },
            };
            if self.index.is_none() {
                quote! {
                    OptionHandler {
                        name: #name_string,
                        short: #short,
                        long: #long,
                        assign: &RefCell::new(|#value: String| Ok(#store)),
                    }
                }
            } else {
                let is_multiple = self.is_multiple;
                quote! {
                    PositionalHandler {
                        name: #name_string,
                        is_multiple: #is_multiple,
                        assign: &RefCell::new(|#value: String| Ok(#store)),
                    }
                }
            }
        }
    }
}

struct Generator {
    decls: Vec<TokenStream>,
    fields: Vec<TokenStream>,
    flags: Vec<TokenStream>,
    options: Vec<TokenStream>,
    positions: Vec<TokenStream>,
}

impl Generator {
    fn new() -> Generator {
        Generator {
            decls: Vec::new(),
            fields: Vec::new(),
            flags: Vec::new(),
            options: Vec::new(),
            positions: Vec::new(),
        }
    }

    fn add_args(&mut self, args: &[Arg]) {
        for arg in args {
            self.decls.push(arg.declare());
            self.fields.push(arg.field());
            let handler = arg.handler();
            match (arg.is_flag, arg.index) {
                (true, _) => self.flags.push(handler),
                (false, None) => self.options.push(handler),
                (false, Some(_)) => self.positions.push(handler),
            }
        }
    }

    fn gen_impl(name: &Ident, app: &App) -> TokenStream {
        let mut this = Generator::new();
        this.add_args(&app.by_switch);
        this.add_args(&app.by_position);
        let decls = &this.decls;
        let fields = &this.fields;
        let flags = &this.flags;
        let options = &this.options;
        let positions = &this.positions;
        quote!(
            impl ::miniclap::MiniClap for #name {
                fn __parse_internal(
                    mut args: &mut dyn ::std::iter::Iterator<Item = ::std::ffi::OsString>,
                ) -> ::std::result::Result<Self, ::miniclap::Error> {
                    use ::std::string::String;
                    use ::std::vec::Vec;
                    use ::std::boxed::Box;
                    use ::std::option::Option::{self, Some, None};
                    use ::std::result::Result::{Ok, Err};
                    use ::std::cell::RefCell;
                    use ::miniclap::{Error, Result};
                    use ::miniclap::{ArgHandlers, FlagHandler, OptionHandler, PositionalHandler};

                    #(#decls)*

                    ::miniclap::parse_args(args, &ArgHandlers {
                        flags: &[ #(#flags),* ],
                        options: &[ #(#options),* ],
                        positions: &[ #(#positions),* ],
                    })?;

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
