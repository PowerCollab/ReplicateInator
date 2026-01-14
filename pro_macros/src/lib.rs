extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, DeriveInput, Meta, Lit, Expr,
    punctuated::Punctuated,
    Token,
};

#[proc_macro_derive(ConnectionMessage, attributes(connection_message))]
pub fn derive_message(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = ast.ident;

    let mut is_authentication = false;
    
    for attr in ast.attrs {
        if attr.path().is_ident("connection_message") {
            let list = attr.parse_args_with(
                Punctuated::<Meta, Token![,]>::parse_terminated
            );
            
            if let Ok(list) = list {
                for meta in list {
                    match meta {
                        Meta::NameValue(nv) => {
                            if nv.path.is_ident("authentication") {
                                if let Expr::Lit(expr_lit) = nv.value {
                                    if let Lit::Bool(b) = expr_lit.lit {
                                        is_authentication = b.value;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let expanded = if is_authentication {
        quote! {
            impl MessageTrait for #name {
                fn as_authentication(&self) -> bool { true }
            }
        }
    } else {
        quote! {
            impl MessageTrait for #name {
                fn as_authentication(&self) -> bool { false }
            }
        }
    };

    expanded.into()
}
