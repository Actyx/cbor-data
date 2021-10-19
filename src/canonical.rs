use std::borrow::Cow;

use crate::{
    builder::CborOutput,
    constants::*,
    reader::{careful_literal, indefinite, integer, major, tags, value, value_bytes},
    ArrayWriter, CborBuilder, CborValue, DictWriter, Writer,
};

pub fn canonicalise<O: CborOutput>(bytes: &[u8], builder: CborBuilder<'_, O>) -> Option<O::Output> {
    let (tags, bytes) = tags(bytes)?;
    match major(bytes)? {
        MAJOR_POS => Some(builder.write_pos(integer(bytes)?.0, tags)),
        MAJOR_NEG => Some(builder.write_neg(integer(bytes)?.0, tags)),
        MAJOR_BYTES => {
            if tags.single() == Some(TAG_CBOR_ITEM) {
                // drop top-level CBOR item encoding wrapper
                canonicalise(value_bytes(bytes, false)?.0.as_ref(), builder)
            } else {
                Some(builder.write_bytes(value_bytes(bytes, false)?.0.as_ref(), tags))
            }
        }
        MAJOR_STR => Some(builder.write_str(
            std::str::from_utf8(value_bytes(bytes, false)?.0.as_ref()).ok()?,
            tags,
        )),
        // TODO keep definite size arrays definite size if len < 24
        MAJOR_ARRAY => {
            let (cbor, result) = builder.write_array_ret(tags, |b| canonicalise_array(bytes, b));
            result.map(|_| cbor)
        }
        MAJOR_DICT => {
            let (cbor, result) = builder.write_dict_ret(tags, |b| canonicalise_dict(bytes, b));
            result.map(|_| cbor)
        }
        MAJOR_LIT => Some(builder.write_lit(careful_literal(bytes)?.0, tags)),
        _ => None,
    }
}

fn update<'a, T>(b: &mut &'a [u8], val: Option<(T, &'a [u8])>) -> Option<T> {
    let (t, r) = val?;
    *b = r;
    Some(t)
}

fn update3<'a, T>(b: &mut &'a [u8], val: Option<(T, &'a [u8], &'a [u8])>) -> Option<T> {
    let (t, _, r) = val?;
    *b = r;
    Some(t)
}

fn canonicalise_array<'a>(bytes: &'a [u8], builder: &mut ArrayWriter) -> Option<&'a [u8]> {
    fn one(bytes: &mut &[u8], builder: &mut ArrayWriter) -> Option<()> {
        let (tags, b) = tags(bytes)?;
        match major(b)? {
            MAJOR_POS => {
                builder.write_pos(update3(bytes, integer(b))?, tags);
            }
            MAJOR_NEG => {
                builder.write_neg(update3(bytes, integer(b))?, tags);
            }
            MAJOR_BYTES => {
                if tags.single() == Some(TAG_CBOR_ITEM) {
                    // drop CBOR item encoding wrapper - may choose to use these later for more efficient skipping
                    let decoded = update(bytes, value_bytes(b, false))?;
                    // the line above has advanced the main loop’s reference, here we advance a temporary one
                    one(&mut decoded.as_ref(), builder)?
                } else {
                    builder.write_bytes(update(bytes, value_bytes(b, false))?.as_ref(), tags);
                }
            }
            MAJOR_STR => {
                builder.write_str(
                    std::str::from_utf8(update(bytes, value_bytes(b, false))?.as_ref()).ok()?,
                    tags,
                );
            }
            MAJOR_ARRAY => {
                *bytes = builder
                    .write_array_ret(tags, |builder| canonicalise_array(b, builder))
                    .1?;
            }
            MAJOR_DICT => {
                *bytes = builder
                    .write_dict_ret(tags, |builder| canonicalise_dict(b, builder))
                    .1?;
            }
            MAJOR_LIT => {
                builder.write_lit(update(bytes, careful_literal(b))?, tags);
            }
            _ => return None,
        }
        Some(())
    }

    // at this point the first byte (indefinite array) has already been written
    let (len, _, mut bytes) = integer(bytes).or_else(|| indefinite(bytes))?;
    if len == u64::MAX {
        // marker for indefinite size
        while *bytes.get(0)? != STOP_BYTE {
            one(&mut bytes, builder)?;
        }
        Some(&bytes[1..])
    } else {
        for _ in 0..len {
            one(&mut bytes, builder)?;
        }
        Some(bytes)
    }
}

fn canonicalise_dict<'a>(bytes: &'a [u8], builder: &mut DictWriter) -> Option<&'a [u8]> {
    fn key<'b>(bytes_ref: &mut &'b [u8]) -> Option<Cow<'b, str>> {
        use crate::reader::ValueResult::*;

        let (tags, rest) = tags(bytes_ref)?;
        let (value, bytes, rest) = value(rest)?;
        *bytes_ref = rest;
        match value {
            V(kind) => {
                let value = CborValue { tags, kind, bytes };
                if let Some(s) = value.as_str() {
                    return Some(Cow::Borrowed(s));
                }
                // FIXME replace by proper Number type once available
                if let Some(n) = value.as_f64() {
                    return Some(Cow::Owned(n.to_string()));
                }
                None
            }
            S(s) => Some(Cow::Owned(s)),
            B(b) => String::from_utf8(b).ok().map(Cow::Owned),
        }
    }
    fn one(bytes: &mut &[u8], key: &str, builder: &mut DictWriter) -> Option<()> {
        let (tags, b) = tags(bytes)?;
        match major(b)? {
            MAJOR_POS => {
                let pos = update3(bytes, integer(b))?;
                builder.with_key(key, |b| b.write_pos(pos, tags));
            }
            MAJOR_NEG => {
                let neg = update3(bytes, integer(b))?;
                builder.with_key(key, |b| b.write_neg(neg, tags));
            }
            MAJOR_BYTES => {
                let decoded = update(bytes, value_bytes(b, false))?;
                if tags.single() == Some(TAG_CBOR_ITEM) {
                    // the line above has advanced the main loop’s reference, here we advance a temporary one
                    one(&mut decoded.as_ref(), key, builder)?
                } else {
                    builder.with_key(key, |b| b.write_bytes(decoded.as_ref(), tags));
                }
            }
            MAJOR_STR => {
                let value = update(bytes, value_bytes(b, false))?;
                let value = std::str::from_utf8(value.as_ref()).ok()?;
                builder.with_key(key, |b| b.write_str(value, tags));
            }
            MAJOR_ARRAY => {
                let mut res = None;
                builder.with_key(key, |bb| {
                    bb.write_array_ret(tags, |builder| {
                        res = canonicalise_array(b, builder);
                    })
                    .0
                });
                *bytes = res?;
            }
            MAJOR_DICT => {
                let mut res = None;
                builder.with_key(key, |bb| {
                    bb.write_dict_ret(tags, |builder| {
                        res = canonicalise_dict(b, builder);
                    })
                    .0
                });
                *bytes = res?;
            }
            MAJOR_LIT => {
                let value = update(bytes, careful_literal(b))?;
                builder.with_key(key, |b| b.write_lit(value, tags));
            }
            _ => return None,
        }
        Some(())
    }

    // at this point the first byte (indefinite array) has already been written
    let (len, _, mut bytes) = integer(bytes).or_else(|| indefinite(bytes))?;
    if len == u64::MAX {
        // marker for indefinite size
        while *bytes.get(0)? != STOP_BYTE {
            let key = key(&mut bytes)?;
            one(&mut bytes, key.as_ref(), builder)?;
        }
        Some(&bytes[1..])
    } else {
        for _ in 0..len {
            let key = key(&mut bytes)?;
            one(&mut bytes, key.as_ref(), builder)?;
        }
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CborValue;
    use crate::{value::ValueKind::*, CborOwned};

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

        let a = |s: &str| encoded_array.index(s).unwrap().decoded().unwrap();
        let af = |s: &str| encoded_array.index(s).unwrap();
        let d = |s: &str| encoded_dict.index(s).unwrap().decoded().unwrap();
        let df = |s: &str| encoded_dict.index(s).unwrap();

        assert_eq!(a(""), CborValue::fake(None, Array));
        assert_eq!(a("0"), CborValue::fake(None, Array));
        assert_eq!(
            af("0"),
            CborValue::fake(Some(TAG_CBOR_ITEM), Bytes(array.as_slice()))
        );
        assert_eq!(a("0.0"), CborValue::fake(None, Null));
        assert_eq!(a("0.1"), CborValue::fake(None, Str("v")));
        assert_eq!(a("1"), CborValue::fake(None, Dict));
        assert_eq!(
            af("1"),
            CborValue::fake(Some(TAG_CBOR_ITEM), Bytes(dict.as_slice()))
        );
        assert_eq!(a("1.a"), CborValue::fake(None, Null));
        assert_eq!(a("1.b"), CborValue::fake(None, Str("v")));

        assert_eq!(d(""), CborValue::fake(None, Dict));
        assert_eq!(d("a"), CborValue::fake(None, Array));
        assert_eq!(
            df("a"),
            CborValue::fake(Some(TAG_CBOR_ITEM), Bytes(array.as_slice()))
        );
        assert_eq!(d("a.0"), CborValue::fake(None, Null));
        assert_eq!(d("a.1"), CborValue::fake(None, Str("v")));
        assert_eq!(d("b"), CborValue::fake(None, Dict));
        assert_eq!(
            df("b"),
            CborValue::fake(Some(TAG_CBOR_ITEM), Bytes(dict.as_slice()))
        );
        assert_eq!(d("b.a"), CborValue::fake(None, Null));
        assert_eq!(d("b.b"), CborValue::fake(None, Str("v")));

        assert_eq!(
            encoded_array.to_string(),
            "24|0x82d8184a82d81841f6d818426176d8184ea26161d81841f66162d818426176"
        );

        let canonical_array = CborOwned::canonical(encoded_array.as_slice()).unwrap();
        assert_eq!(
            format!("{:?}", canonical_array),
            "Cbor(82 82 f6 61 76 a2 61 61 f6 61 62 61 76)".to_owned()
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
            "Cbor(9f 9f f6 61 76 ff bf 61 61 f6 61 62 61 76 ff ff)".to_owned()
        );
        assert_eq!(
            canonical_array.to_string(),
            r#"[_ [_ null, "v"], {_ "a": null, "b": "v"}]"#
        );

        assert_eq!(
            encoded_dict.to_string(),
            "24|0xa26161d8184a82d81841f6d8184261766162d8184ea26161d81841f66162d818426176"
        );

        let canonical_dict = CborOwned::canonical(encoded_dict.as_slice()).unwrap();
        assert_eq!(
            format!("{:?}", canonical_dict),
            "Cbor(a2 61 61 82 f6 61 76 61 62 a2 61 61 f6 61 62 61 76)".to_owned()
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
            "Cbor(bf 61 61 9f f6 61 76 ff 61 62 bf 61 61 f6 61 62 61 76 ff ff)".to_owned()
        );
        assert_eq!(
            canonical_dict.to_string(),
            r#"{_ "a": [_ null, "v"], "b": {_ "a": null, "b": "v"}}"#
        );
    }
}
