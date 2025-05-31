use crate::{is_one_tuple, is_transparent};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{spanned::Spanned, Attribute, Data, DataEnum, Error, Fields, FieldsNamed, FieldsUnnamed};

pub fn code_read(data: Data, attrs: Vec<Attribute>) -> Result<TokenStream, Error> {
    if let Some(a) = attrs.iter().find(is_transparent) {
        if let Data::Struct(s) = &data {
            if is_one_tuple(&s.fields) {
                let t = &s.fields.iter().next().unwrap().ty;
                return Ok(quote_spanned! {t.span()=>
                    Ok(Self(::cbor_data::codec::ReadCbor::read_cbor(cbor)?))
                });
            }
        }
        return Err(Error::new(
            a.tokens.span(),
            "transparent representation only possible on 1-tuples",
        ));
    }

    match data {
        Data::Struct(s) => read_fields(&s.fields, quote!(Self)),
        Data::Enum(DataEnum {
            variants,
            brace_token,
            ..
        }) => {
            if variants.is_empty() {
                return Err(Error::new(
                    brace_token.span,
                    "empty enums are not supported by ReadCbor",
                ));
            }
            let vars = variants
                .iter()
                .map(|v| {
                    let i = &v.ident;
                    let n = i.to_string();

                    if let Some(a) = v.attrs.iter().find(is_transparent) {
                        if is_one_tuple(&v.fields) {
                            let t = &v.fields.iter().next().unwrap().ty;
                            return Ok(quote_spanned! {t.span()=>
                                if let Some(cbor) = d.get(#n) {
                                    Ok(Self::#i(::cbor_data::codec::ReadCbor::read_cbor(cbor)?))
                                }
                            });
                        }
                        return Err(Error::new(
                            a.tokens.span(),
                            "transparent representation only possible on 1-tuples",
                        ));
                    }

                    let c = read_fields(&v.fields, quote!(Self::#i))?;
                    Ok(quote! {
                        if let Some(cbor) = d.get(#n) {
                            #c
                        }
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let known = variants.iter().map(|v| v.ident.to_string());
            Ok(quote! {
                let d = cbor.try_dict()?;
                let d = d
                    .iter()
                    .filter_map(|(k, v)| k.decode().to_str().map(|k| (k, v)))
                    .collect::<::std::collections::BTreeMap<_, _>>();
                #(#vars else)*
                {
                    Err(::cbor_data::codec::CodecError::NoKnownVariant {
                        known: &[#(#known,)*][..],
                        present: d.into_keys().map(|c| c.into_owned()).collect(),
                    })
                }
            })
        }
        Data::Union(u) => Err(Error::new(
            u.union_token.span,
            "WriteCbor does not support `union`",
        )),
    }
}

fn read_fields(f: &Fields, constructor: TokenStream) -> Result<TokenStream, Error> {
    match f {
        Fields::Named(FieldsNamed { named: f, .. }) => {
            let field = f.iter().map(|f| {
                let i = f.ident.as_ref().unwrap();
                let n = i.to_string();
                quote_spanned! {f.span()=>
                    #i: ::cbor_data::codec::ReadCbor::read_cbor(d.get(#n).ok_or(::cbor_data::codec::CodecError::MissingField(#n))?.as_ref())?,
                }
            });
            Ok(quote! {
                let d = cbor.try_dict()?;
                let d = d
                    .iter()
                    .filter_map(|(k, v)| k.decode().to_str().map(|k| (k, v)))
                    .collect::<::std::collections::BTreeMap<_, _>>();
                Ok(#constructor {
                    #(#field)*
                })
            })
        }
        Fields::Unnamed(FieldsUnnamed { unnamed: f, .. }) => {
            let n = f.len();
            let field = f.iter().enumerate().map(|(i, f)| {
                quote_spanned! {f.span()=>
                    ::cbor_data::codec::ReadCbor::read_cbor(a.get(#i).ok_or(::cbor_data::codec::CodecError::TupleSize { expected: #n, found: #i })?.as_ref())?,
                }
            });
            Ok(quote! {
                let a = cbor.try_array()?;
                Ok(#constructor(#(#field)*))
            })
        }
        Fields::Unit => Ok(quote! {
            if cbor.decode().is_null() {
                Ok(#constructor)
            } else {
                Err(::cbor_data::codec::CodecError::type_error("null", &cbor.tagged_item()))
            }
        }),
    }
}
