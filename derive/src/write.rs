use crate::{is_one_tuple, is_transparent};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
use syn::{
    spanned::Spanned, Attribute, Data, DataEnum, DataStruct, Error, Fields, FieldsNamed,
    FieldsUnnamed, Index,
};

pub fn code_write(data: Data, attrs: Vec<Attribute>) -> Result<TokenStream, Error> {
    if let Some(a) = attrs.iter().find(is_transparent) {
        if let Data::Struct(s) = &data {
            if is_one_tuple(&s.fields) {
                let t = &s.fields.iter().next().unwrap().ty;
                return Ok(quote_spanned! {t.span()=>
                    <#t as ::cbor_data::codec::WriteCbor>::write_cbor(&self.0, w)
                });
            }
        }
        return Err(Error::new(
            a.tokens.span(),
            "transparent representation only possible on 1-tuples",
        ));
    }

    match data {
        Data::Struct(DataStruct { fields, .. }) => write_fields(&fields, None),
        Data::Enum(DataEnum {
            variants,
            brace_token,
            ..
        }) => {
            if variants.is_empty() {
                return Err(Error::new(
                    brace_token.span,
                    "empty enums are not supported by WriteCbor",
                ));
            }
            let vars = variants
                .iter()
                .map(|v| {
                    let ident = &v.ident;
                    let name = ident.to_string();

                    if let Some(a) = v.attrs.iter().find(is_transparent) {
                        if is_one_tuple(&v.fields) {
                            let f = v.fields.iter().next().unwrap();
                            let t = &f.ty;
                            return Ok(quote_spanned! {t.span()=>
                                Self::#ident(b0) => {
                                    w.write_dict(None, |w| {
                                        w.with_key(#name, |w| <#t as ::cbor_data::codec::WriteCbor>::write_cbor(b0, w));
                                    })
                                }
                            });
                        }
                        return Err(Error::new(
                            a.tokens.span(),
                            "transparent representation only possible on 1-tuples",
                        ));
                    }

                    let bindings = v
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(i, _)| Ident::new(&*format!("b{}", i), Span::call_site()))
                        .collect::<Vec<_>>();
                    let pat = match &v.fields {
                        Fields::Named(f) => {
                            let field = f.named.iter().zip(bindings.iter()).map(|(f, b)| {
                                let i = f.ident.as_ref().unwrap();
                                quote!(#i: #b)
                            });
                            quote!(Self::#ident { #(#field),* })
                        }
                        Fields::Unnamed(_f) => {
                            quote!(Self::#ident(#(#bindings),*))
                        }
                        Fields::Unit => quote!(Self::#ident),
                    };
                    let code = write_fields(&v.fields, Some(bindings))?;
                    Ok(quote! {
                        #pat => w.write_dict(None, |w| {
                            w.with_key(#name, |w| #code);
                        }),
                    })
                })
                .collect::<Result<Vec<TokenStream>, Error>>()?;
            Ok(quote! {
                match self {
                    #(#vars)*
                }
            })
        }
        Data::Union(u) => Err(Error::new(
            u.union_token.span,
            "WriteCbor does not support `union`",
        )),
    }
}

fn write_fields(f: &Fields, bindings: Option<Vec<Ident>>) -> Result<TokenStream, Error> {
    match f {
        Fields::Named(FieldsNamed { named: f, .. }) => {
            let field = f.iter().enumerate().map(|(idx, f)| {
                let i = f.ident.as_ref().unwrap();
                let n = i.to_string();
                let t = &f.ty;
                let b = if let Some(bindings) = &bindings {
                    bindings[idx].to_token_stream()
                } else {
                    quote!(&self.#i)
                };
                quote_spanned! {f.span()=>
                    w.with_key(#n, |w| <#t as ::cbor_data::codec::WriteCbor>::write_cbor(#b, w));
                }
            });
            Ok(quote! {
                ::cbor_data::Writer::write_dict(w, None, |w| {
                    #(#field)*
                })
            })
        }
        Fields::Unnamed(FieldsUnnamed { unnamed: f, .. }) => {
            let field = f.iter().enumerate().map(|(idx, f)| {
                let i = Index::from(idx);
                let t = &f.ty;
                let b = if let Some(bindings) = &bindings {
                    bindings[idx].to_token_stream()
                } else {
                    quote!(&self.#i)
                };
                quote_spanned! {f.span()=>
                    <#t as ::cbor_data::codec::WriteCbor>::write_cbor(#b, &mut w);
                }
            });
            Ok(quote! {
                ::cbor_data::Writer::write_array(w, None, |mut w| {
                    #(#field)*
                })
            })
        }
        Fields::Unit => Ok(quote!(::cbor_data::Writer::write_null(w, None))),
    }
}
