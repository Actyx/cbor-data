use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, DeriveInput, Error, Fields, Meta, NestedMeta};

mod read;
mod write;

#[proc_macro_derive(WriteCbor, attributes(cbor))]
pub fn derive_write(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (g_impl, g_type, g_where) = input.generics.split_for_impl();

    let code = match write::code_write(input.data, input.attrs) {
        Ok(code) => code,
        Err(e) => {
            return e
                .into_iter()
                .map(Error::into_compile_error)
                .collect::<TokenStream>()
                .into()
        }
    };

    let ret = quote! {
        impl #g_impl ::cbor_data::codec::WriteCbor for #name #g_type #g_where {
            fn write_cbor<W: ::cbor_data::Writer>(&self, w: W) -> W::Output {
                #code
            }
        }
    };
    ret.into()
}

#[proc_macro_derive(ReadCbor, attributes(cbor))]
pub fn derive_read(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (g_impl, g_type, g_where) = input.generics.split_for_impl();

    let code = match read::code_read(input.data, input.attrs) {
        Ok(code) => code,
        Err(e) => {
            return e
                .into_iter()
                .map(Error::into_compile_error)
                .collect::<TokenStream>()
                .into()
        }
    };

    let name_string = name.to_string();
    let ret = quote! {
        impl #g_impl ::cbor_data::codec::ReadCbor for #name #g_type #g_where {
            fn fmt(f: &mut impl ::std::fmt::Write) -> ::std::fmt::Result {
                write!(f, #name_string)
            }

            fn read_cbor_impl(cbor: &::cbor_data::Cbor) -> ::cbor_data::codec::Result<Self>
            where
                Self: Sized,
            {
                #code
            }
        }
    };
    ret.into()
}

fn is_one_tuple(f: &Fields) -> bool {
    match f {
        Fields::Unnamed(f) => f.unnamed.len() == 1,
        _ => false,
    }
}

fn is_transparent(a: &&Attribute) -> bool {
    if let Ok(Meta::List(l)) = a.parse_meta() {
        l.path.is_ident("cbor")
            && l.nested
                .iter()
                .any(|m| matches!(m, NestedMeta::Meta(Meta::Path(p)) if p.is_ident("transparent")))
    } else {
        false
    }
}
