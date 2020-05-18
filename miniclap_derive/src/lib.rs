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
    index: Option<usize>,
    short: Option<String>,
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

            let is_positional = short.is_none() && long.is_none();
            let mut is_required = true;
            let mut is_flag = false;
            let mut is_multiple = false;
            let index = if is_positional {
                Some(by_position.len())
            } else {
                None
            };

            match f.ty {
                syn::Type::Path(syn::TypePath {
                    path: syn::Path { ref segments, .. },
                    ..
                }) => match segments.last().unwrap().ident.to_string().as_str() {
                    "Option" => {
                        // TODO: Add a check for positional
                        is_required = false;
                    }
                    "Vec" => {
                        is_multiple = true;
                        is_required = false;
                    }
                    "bool" => {
                        if !is_positional {
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

            if is_positional {
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
        } else {
            match (self.is_multiple, &self.default_value) {
                (false, Some(lit)) => quote! { let mut #arg_var = #lit; },
                (false, None) => quote! { let mut #arg_var = None; },
                (true, _) => quote! { let mut #arg_var = Vec::new(); },
            }
        }
    }

    fn pattern(&self) -> TokenStream {
        let short = self.short.as_ref().map(|x| format!("-{}", x));
        let long = self.long.as_ref().map(|x| format!("--{}", x));
        match (index, short, long) {
            (None, Some(short), Some(long)) => quote! { #short | #long },
            (None, Some(short), None) => quote! { #short },
            (None, None, Some(long)) => quote! { #long },
            (Some(i), None, None) => quote! { #i },
            _ => unreachable!(),
        }
    }

    fn parse(&self) -> TokenStream {
        let name_string = self.name.to_string();
        if self.index.is_some() {
            quote! {
                arg.parse().map_err(|e| Error::parse_failed(#name_string, Box::new(e)))?
            }
        } else {
            quote! {
                ::miniclap::__get_value(#name_string, opt_value, &mut args)?.parse()
                    .map_err(|e| Error::parse_failed(#name_string, Box::new(e)))?
            }
        }
    }

    fn assign(&self, value: TokenStream) -> TokenStream {
        let arg_var = self.arg_var();
        let name_string = self.name.to_string();
        if self.is_flag {
            quote! {
                #arg_var = match opt_value.map(|v| v.parse()) {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => return Err(Error::parse_failed(#name_string, Box::new(e))),
                    None => true,
                }
            }
        } else {
            match (self.is_multiple, &self.default_value) {
                (false, Some(_)) => quote! { #arg_var = #value },
                (false, None) => quote! { #arg_var = Some(#value) },
                (true, _) => quote! { #arg_var.push(#value) },
            }
        }
    }

    fn matcher(&self) -> TokenStream {
        let pattern = self.pattern();
        let parse = self.parse();
        let assign = self.assign(parse);
        quote! { #pattern => #assign }
    }

    fn retrieve(&self) -> TokenStream {
        let arg_var = self.arg_var();
        let name_string = self.name.to_string();
        if self.is_flag {
            quote! { #arg_var }
        } else {
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
        }
    }

    fn field(&self) -> TokenStream {
        let name = &self.name;
        let retrieve = self.retrieve();
        quote! { #name: #retrieve }
    }
}

struct Generator {
    decls: Vec<TokenStream>,
    fields: Vec<TokenStream>,
    post_matching: Vec<TokenStream>,
}

impl Generator {
    fn add_args(&mut self, args: &[Arg]) -> Vec<TokenStream> {
        let mut matches = Vec::new();
        for arg in args {
            self.decls.push(arg.declare());
            matches.push(arg.matcher());
            self.fields.push(arg.field());
        }
        matches
    }

    fn gen_switch_matcher(&mut self, args: &[Arg]) -> TokenStream {
        if args.is_empty() {
            return quote! { return Err(Error::unknown_switch(arg)) };
        }

        let matches = self.add_args(args);
        quote! {
            let opt_value = ::miniclap::__split_arg_value(&mut arg);
            match arg {
                #(#matches),*,
                _ => return Err(Error::unknown_switch(arg)),
            }
        }
    }

    fn gen_position_matcher(&mut self, args: &[Arg]) -> TokenStream {
        if args.is_empty() {
            return quote! { return Err(Error::too_many_positional(arg)) };
        }

        let matches = self.add_args(args);
        self.decls.push(quote! { let mut num_args = 0; });
        quote! {
            match num_args {
                #(#matches),*,
                _ => return Err(Error::too_many_positional(arg)),
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
                fn __parse_internal(
                    mut args: &mut dyn ::std::iter::Iterator<Item = ::std::ffi::OsString>,
                ) -> ::std::result::Result<Self, ::miniclap::Error> {
                    use ::std::string::String;
                    use ::std::vec::Vec;
                    use ::std::boxed::Box;
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
