use crate::{
    check::{value_bytes, MkErr},
    constants::*,
    error::{ErrorKind, InternalError},
    reader::{careful_literal, indefinite, integer, major, tags},
    validated::iterators::BytesIter,
    ArrayWriter, DictWriter,
    ErrorKind::*,
    ParseError,
    WhileParsing::*,
    Writer,
};

/// Canonicalise the input bytes into the output Writer
pub fn canonicalise<W: Writer>(bytes: &[u8], builder: W) -> Result<W::Output, ParseError> {
    let (rest, cbor) = canonical(bytes, builder).map_err(|e| e.rebase(bytes))?;
    if rest.is_empty() {
        Ok(cbor)
    } else {
        Err(InternalError::new(rest, TrailingGarbage).rebase(bytes))
    }
}

fn canonical<W: Writer>(bytes: &[u8], builder: W) -> Result<(&[u8], W::Output), InternalError> {
    let (tags, bytes) = tags(bytes).ok_or_else(|| InternalError::new(bytes, InvalidInfo))?;
    match major(bytes).ok_or_else(|| InternalError::new(bytes, UnexpectedEof(ItemHeader)))? {
        MAJOR_POS => integer(bytes)
            .map(|(x, _, r)| (r, builder.write_pos(x, tags)))
            .header_value(bytes),
        MAJOR_NEG => integer(bytes)
            .map(|(x, _, r)| (r, builder.write_neg(x, tags)))
            .header_value(bytes),
        MAJOR_BYTES => {
            if tags.single() == Some(TAG_CBOR_ITEM) {
                // drop top-level CBOR item encoding wrapper
                value_bytes(bytes, true, false).and_then(|(b, rest)| {
                    canonicalise(b.as_ref(), builder)
                        .map(|out| (rest, out))
                        .map_err(|e| {
                            let mut offset = e.offset();
                            let iter = if bytes[0] & 31 == INDEFINITE_SIZE {
                                BytesIter::new(&bytes[1..], None)
                            } else {
                                BytesIter::new(bytes, Some(1))
                            };
                            for slice in iter {
                                if offset < slice.len() {
                                    return InternalError::new(&slice[offset..], e.kind());
                                }
                                offset -= slice.len();
                            }
                            InternalError::new(rest, e.kind())
                        })
                })
            } else {
                value_bytes(bytes, true, false)
                    .map(|(b, rest)| (rest, builder.write_bytes(b.as_ref(), tags)))
            }
        }
        MAJOR_STR => value_bytes(bytes, true, true).map(|(b, rest)| {
            let s = unsafe { std::str::from_utf8_unchecked(b.as_ref()) };
            (rest, builder.write_str(s, tags))
        }),
        MAJOR_ARRAY => {
            let (cbor, result) = builder.write_array_ret(tags, |b| canonicalise_array(bytes, b));
            result.map(|rest| (rest, cbor))
        }
        MAJOR_DICT => {
            let (cbor, result) = builder.write_dict_ret(tags, |b| canonicalise_dict(bytes, b));
            result.map(|rest| (rest, cbor))
        }
        MAJOR_LIT => careful_literal(bytes)
            .map(|(lit, rest)| (rest, builder.write_lit(lit, tags)))
            .ok_or_else(|| InternalError::new(bytes, ErrorKind::InvalidInfo)),
        _ => unreachable!(),
    }
}

fn canonicalise_array<'a>(
    bytes: &'a [u8],
    mut builder: &mut ArrayWriter,
) -> Result<&'a [u8], InternalError<'a>> {
    // at this point the first byte (indefinite array) has already been written
    let (len, _, mut bytes) = integer(bytes)
        .or_else(|| indefinite(bytes))
        .header_value(bytes)?;
    if len == u64::MAX {
        // marker for indefinite size
        while *bytes
            .get(0)
            .ok_or_else(|| InternalError::new(bytes, UnexpectedEof(ArrayItem)))?
            != STOP_BYTE
        {
            bytes = canonical(bytes, &mut builder)?.0;
        }
        Ok(&bytes[1..])
    } else {
        for _ in 0..len {
            if bytes.is_empty() {
                return Err(InternalError::new(bytes, UnexpectedEof(ArrayItem)));
            }
            bytes = canonical(bytes, &mut builder)?.0;
        }
        Ok(bytes)
    }
}

fn canonicalise_dict<'a>(
    bytes: &'a [u8],
    builder: &mut DictWriter,
) -> Result<&'a [u8], InternalError<'a>> {
    fn pair<'a>(bytes: &mut &'a [u8], w: &mut DictWriter) -> Result<(), InternalError<'a>> {
        w.try_write_pair(|key| {
            let (rest, val) = canonical(bytes, key)?;
            let (rest, res) = canonical(rest, val)?;
            *bytes = rest;
            Ok(res)
        })?;
        Ok(())
    }

    // at this point the first byte (indefinite array) has already been written
    let (len, _, mut bytes) = integer(bytes)
        .or_else(|| indefinite(bytes))
        .header_value(bytes)?;
    if len == u64::MAX {
        // marker for indefinite size
        while *bytes
            .get(0)
            .ok_or_else(|| InternalError::new(bytes, UnexpectedEof(DictItem)))?
            != STOP_BYTE
        {
            pair(&mut bytes, builder)?;
        }
        Ok(&bytes[1..])
    } else {
        for _ in 0..len {
            if bytes.is_empty() {
                return Err(InternalError::new(bytes, UnexpectedEof(DictItem)));
            }
            pair(&mut bytes, builder)?;
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use crate::{constants::TAG_CBOR_ITEM, index_str, CborBuilder, CborOwned, Writer};

    #[test]
    fn remove_cbor_encoding() {
        let item = CborBuilder::default().write_null(None);
        let item_str = CborBuilder::default().write_str("v", None);
        let array = CborBuilder::default()
            .write_array_ret(None, |builder| {
                builder.write_bytes(item.as_slice(), Some(TAG_CBOR_ITEM));
                builder.write_bytes(item_str.as_slice(), Some(TAG_CBOR_ITEM));
            })
            .0;
        let dict = CborBuilder::default()
            .write_dict_ret(None, |builder| {
                builder.with_key("a", |b| b.write_bytes(item.as_slice(), Some(TAG_CBOR_ITEM)));
                builder.with_key("b", |b| {
                    b.write_bytes(item_str.as_slice(), Some(TAG_CBOR_ITEM))
                });
            })
            .0;
        let nested_array = CborBuilder::default()
            .write_array_ret(None, |builder| {
                builder.write_bytes(array.as_slice(), Some(TAG_CBOR_ITEM));
                builder.write_bytes(dict.as_slice(), Some(TAG_CBOR_ITEM));
            })
            .0;
        let nested_dict = CborBuilder::default()
            .write_dict_ret(None, |builder| {
                builder.with_key("a", |b| {
                    b.write_bytes(array.as_slice(), Some(TAG_CBOR_ITEM))
                });
                builder.with_key("b", |b| b.write_bytes(dict.as_slice(), Some(TAG_CBOR_ITEM)));
            })
            .0;
        let encoded_array =
            CborBuilder::default().write_bytes(nested_array.as_slice(), Some(TAG_CBOR_ITEM));
        let encoded_dict =
            CborBuilder::default().write_bytes(nested_dict.as_slice(), Some(TAG_CBOR_ITEM));

        let a = |s: &str| encoded_array.index(index_str(s)).unwrap().to_string();
        let d = |s: &str| encoded_dict.index(index_str(s)).unwrap().to_string();

        assert_eq!(a(""), r#"[<[<null>, <"v">]>, <{"a": <null>, "b": <"v">}>]"#);
        assert_eq!(a("[0]"), r#"[<null>, <"v">]"#);
        assert_eq!(a("[0][0]"), r#"null"#);
        assert_eq!(a("[0][1]"), r#""v""#);
        assert_eq!(a("[1]"), r#"{"a": <null>, "b": <"v">}"#);
        assert_eq!(a("[1].a"), r#"null"#);
        assert_eq!(a("[1].b"), r#""v""#);

        assert_eq!(
            d(""),
            r#"{"a": <[<null>, <"v">]>, "b": <{"a": <null>, "b": <"v">}>}"#
        );
        assert_eq!(d("a"), r#"[<null>, <"v">]"#);
        assert_eq!(d("a[0]"), r#"null"#);
        assert_eq!(d("a[1]"), r#""v""#);
        assert_eq!(d("b"), r#"{"a": <null>, "b": <"v">}"#);
        assert_eq!(d("b.a"), r#"null"#);
        assert_eq!(d("b.b"), r#""v""#);

        assert_eq!(
            encoded_array.to_string(),
            r#"<[<[<null>, <"v">]>, <{"a": <null>, "b": <"v">}>]>"#
        );

        let canonical_array = CborOwned::canonical(encoded_array.as_slice()).unwrap();
        assert_eq!(
            format!("{:?}", canonical_array),
            "Cbor(8282f661 76a26161 f6616261 76)".to_owned()
        );
        assert_eq!(
            canonical_array.to_string(),
            r#"[[null, "v"], {"a": null, "b": "v"}]"#
        );

        let canonical_array = CborBuilder::new()
            .with_max_definite_size(None)
            .write_canonical(encoded_array.as_slice())
            .unwrap();
        assert_eq!(
            format!("{:?}", canonical_array),
            "Cbor(9f9ff661 76ffbf61 61f66162 6176ffff)".to_owned()
        );
        assert_eq!(
            canonical_array.to_string(),
            r#"[_ [_ null, "v"], {_ "a": null, "b": "v"}]"#
        );

        assert_eq!(
            encoded_dict.to_string(),
            r#"<{"a": <[<null>, <"v">]>, "b": <{"a": <null>, "b": <"v">}>}>"#
        );

        let canonical_dict = CborOwned::canonical(encoded_dict.as_slice()).unwrap();
        assert_eq!(
            format!("{:?}", canonical_dict),
            "Cbor(a2616182 f6617661 62a26161 f6616261 76)".to_owned()
        );
        assert_eq!(
            canonical_dict.to_string(),
            r#"{"a": [null, "v"], "b": {"a": null, "b": "v"}}"#
        );

        let canonical_dict = CborBuilder::new()
            .with_max_definite_size(None)
            .write_canonical(encoded_dict.as_slice())
            .unwrap();
        assert_eq!(
            format!("{:?}", canonical_dict),
            "Cbor(bf61619f f66176ff 6162bf61 61f66162 6176ffff)".to_owned()
        );
        assert_eq!(
            canonical_dict.to_string(),
            r#"{_ "a": [_ null, "v"], "b": {_ "a": null, "b": "v"}}"#
        );
    }
}
