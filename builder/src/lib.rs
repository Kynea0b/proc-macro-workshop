use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident, Type};

#[proc_macro_derive(Builder)]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let ident = input.ident;
    let vis = input.vis;

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

    let expand = quote! {
        #vis struct #builder_name {
           #(#idents: Option<#types>),*
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
