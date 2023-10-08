use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Error, Fields, FieldsNamed, GenericArgument,
    Ident, Lit, Meta, MetaList, MetaNameValue, NestedMeta, Path, PathArguments, PathSegment, Type,
    TypePath,
};

/// This function covers the following test.
/// builder/tests/05-method-chaining.rs
#[proc_macro_derive(BuilderOld)]
pub fn derive_old(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let ident = input.ident;
    let vis = input.vis;
    dbg!(&input.data);

    let builder_name = format_ident!("{}Builder", ident);
    let (idents, types): (Vec<Ident>, Vec<Type>) = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields
                .named
                .into_iter()
                .map(|field| {
                    let ident = field.ident;
                    let ty = field.ty;
                    (ident.unwrap(), ty)
                })
                .unzip(),
            _ => panic!("no unnamed fields are allowed"),
        },
        _ => panic!("expects struct"),
    };

    let checks = idents.iter().map(|ident| {
        let err = format!("Required field '{}' is missing", ident.to_string());
        quote! {
            if self.#ident.is_none() {
                return Err(#err.into())
            }
        }
    });

    let expand = quote! {
        #vis struct #builder_name {
           #(#idents: Option<#types>),*
        }

        impl #builder_name {
            #(pub fn #idents(&mut self, #idents: #types) -> &mut Self {
                self.#idents = Some(#idents);
                self
            })*

            pub fn build(&mut self) -> Result<#ident, Box<dyn std::error::Error>> {
                #(#checks)*
                Ok(#ident {
                    #(#idents: self.#idents.clone().unwrap()),*
                })
            }
        }

        impl #ident {
            pub fn builder() -> #builder_name {
                #builder_name {
                    #(#idents: None),*
                }
            }
        }
    };
    proc_macro::TokenStream::from(expand)
}

#[proc_macro_derive(Builder, attributes(builder))]
pub fn derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input tokens into a syntax tree
    let item = parse_macro_input!(input as DeriveInput);
    let struct_name = item.ident;
    let builder_name = format_ident!("{}Builder", struct_name);
    let fields = extract_struct_fields(&item.data);

    let wrapped_fields_stream_iter = fields.named.iter().map(|field| {
        let ty = &field.ty;
        let ident = &field.ident;

        if is_option_type(&ty) {
            quote! {
                #ident: #ty
            }
        } else {
            quote! {
                #ident: std::option::Option<#ty>
            }
        }
    });

    let initial_fileds_stream_iter = fields.named.iter().map(|field| {
        let ty = &field.ty;
        let ident = &field.ident;
        let attrs = &field.attrs;
        let attr_each = parse_attr_each(&attrs);

        if is_vec_type(&ty) && attr_each.is_some() {
            quote! {
                #ident: std::option::Option::Some(vec![])
            }
        } else {
            quote! {
                #ident: std::option::Option::None
            }
        }
    });

    let builder_fields_setter_stream_iter = fields.named.iter().map(|field| {
        let ty = &field.ty;
        let ident = &field.ident;
        let attrs = &field.attrs;
        let attr_each = parse_attr_each(&attrs);

        if is_vec_type(&ty) && attr_each.is_some() {
            match attr_each {
                std::option::Option::Some(AttrParseResult::InvalidKey(meta)) => {
                    return Error::new_spanned(meta, "expected `builder(each = \"...\")`")
                        .to_compile_error()
                }
                std::option::Option::Some(AttrParseResult::Value(lit)) => {
                    let inner_type = extract_inner_type(&ty);
                    let lit_ident = format_ident!("{}", lit);

                    if lit == ident.clone().unwrap().to_string() {
                        let ref_ident = format_ident!("ref_{}", lit);
                        quote! {
                            fn #ident(&mut self, #lit_ident: #inner_type) -> &mut Self {
                                if let std::option::Option::Some(ref mut #ref_ident) = self.#ident {
                                    #ref_ident.push(#lit_ident);
                                } else {
                                    self.#ident = std::option::Option::Some(vec![#lit_ident]);
                                };
                                self
                            }
                        }
                    } else {
                        quote! {
                            fn #lit_ident(&mut self, #lit_ident: #inner_type) -> &mut Self {
                                if let std::option::Option::Some(ref mut #ident) = self.#ident {
                                    #ident.push(#lit_ident);
                                } else {
                                    self.#ident = std::option::Option::Some(vec![#lit_ident]);
                                };
                                self
                            }

                            fn #ident(&mut self, #ident: #ty) -> &mut Self {
                                self.#ident = std::option::Option::Some(#ident);
                                self
                            }
                        }
                    }
                }
                std::option::Option::None => unreachable!(),
            }
        } else {
            if is_option_type(&ty) {
                let inner_type = extract_inner_type(&ty);
                quote! {
                    fn #ident(&mut self, #ident: #inner_type) -> &mut Self {
                        self.#ident = std::option::Option::Some(#ident);
                        self
                    }
                }
            } else {
                quote! {
                    fn #ident(&mut self, #ident: #ty) -> &mut Self {
                        self.#ident = std::option::Option::Some(#ident);
                        self
                    }
                }
            }
        }
    });

    let builder_build_stream_iter = fields.named.iter().map(|field| {
        let ty = &field.ty;
        let ident = &field.ident;

        if is_option_type(&ty) {
            quote! {
                #ident: self.#ident.clone()
            }
        } else {
            quote! {
                #ident: self.#ident.clone().unwrap()
            }
        }
    });

    let expanded = quote! {
        pub struct #builder_name {
            #(#wrapped_fields_stream_iter),*
        }

        impl #struct_name {
            pub fn builder() -> #builder_name {
                #builder_name {
                    #(#initial_fileds_stream_iter),*
                }
            }
        }

        impl #builder_name {
            #(#builder_fields_setter_stream_iter)*

            pub fn build(&mut self) -> std::result::Result<#struct_name, std::boxed::Box<dyn std::error::Error>> {
                Ok(#struct_name {
                    #(#builder_build_stream_iter),*
                })
            }
        }
    };

    // Hand the output tokens back to the compiler
    proc_macro::TokenStream::from(expanded)
}

fn extract_struct_fields(data: &Data) -> &FieldsNamed {
    match *data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => fields,
            _ => panic!("invalid fields"),
        },
        _ => panic!("invalid data"),
        // Data::Enum(_) => {}
        // Data::Union(_) => {}
    }
}

fn is_option_type(ty: &Type) -> bool {
    match last_path_segment(&ty) {
        std::option::Option::Some(path_seg) => path_seg.ident == "Option",
        std::option::Option::None => false,
    }
}

fn is_vec_type(ty: &Type) -> bool {
    match last_path_segment(&ty) {
        std::option::Option::Some(path_seg) => path_seg.ident == "Vec",
        std::option::Option::None => false,
    }
}

fn extract_inner_type(ty: &Type) -> &GenericArgument {
    match last_path_segment(&ty) {
        std::option::Option::Some(PathSegment {
            ident: _,
            arguments: PathArguments::AngleBracketed(ref gen_arg),
        }) => gen_arg.args.first(),
        _ => std::option::Option::None,
    }
    .expect("invalid option type")
}

fn last_path_segment(ty: &Type) -> std::option::Option<&PathSegment> {
    match ty {
        &Type::Path(TypePath {
            qself: std::option::Option::None,
            path:
                Path {
                    segments: ref seg,
                    leading_colon: _,
                },
        }) => seg.last(),
        _ => std::option::Option::None,
    }
}

enum AttrParseResult {
    Value(String),
    InvalidKey(Meta),
}

fn parse_attr_each(attrs: &[Attribute]) -> std::option::Option<AttrParseResult> {
    attrs.iter().find_map(|attr| match attr.parse_meta() {
        Ok(meta) => match meta {
            Meta::List(MetaList {
                ref path,
                paren_token: _,
                ref nested,
            }) => {
                (path.get_ident()? == "builder").then(|| ())?;

                if let NestedMeta::Meta(Meta::NameValue(MetaNameValue {
                    path,
                    eq_token: _,
                    lit: Lit::Str(ref litstr),
                })) = nested.first()?
                {
                    if path.get_ident()?.to_string() == "each" {
                        std::option::Option::Some(AttrParseResult::Value(litstr.value()))
                    } else {
                        std::option::Option::Some(AttrParseResult::InvalidKey(meta))
                    }
                } else {
                    std::option::Option::None
                }
            }
            _ => std::option::Option::None,
        },
        _ => std::option::Option::None,
    })
}
