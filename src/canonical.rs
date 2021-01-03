use std::borrow::Cow;

use crate::{
    constants::*,
    reader::{careful_literal, indefinite, integer, major, tag, value, value_bytes},
    ArrayWriter, CborBuilder, CborOwned, CborValue, DictWriter,
};

pub fn canonicalise(bytes: &[u8], builder: CborBuilder<'_>) -> Option<CborOwned> {
    let (tag, bytes) = tag(bytes)?;
    let tag = tag.map(|x| x.tag);
    match major(bytes)? {
        MAJOR_POS => Some(builder.write_pos(integer(bytes)?.0, tag)),
        MAJOR_NEG => Some(builder.write_neg(integer(bytes)?.0, tag)),
        MAJOR_BYTES => {
            if tag == Some(TAG_CBOR_ITEM) {
                // drop top-level CBOR item encoding wrapper
                canonicalise(value_bytes(bytes, false)?.0.as_ref(), builder)
            } else {
                Some(builder.write_bytes(value_bytes(bytes, false)?.0.as_ref(), tag))
            }
        }
        MAJOR_STR => Some(builder.write_str(
            std::str::from_utf8(value_bytes(bytes, false)?.0.as_ref()).ok()?,
            tag,
        )),
        // TODO keep definite size arrays definite size if len < 24
        MAJOR_ARRAY => {
            let (cbor, result) = builder.write_array_rec(tag, |b| canonicalise_array(bytes, b));
            result.map(|_| cbor)
        }
        MAJOR_DICT => {
            let (cbor, result) = builder.write_dict_rec(tag, |b| canonicalise_dict(bytes, b));
            result.map(|_| cbor)
        }
        MAJOR_LIT => Some(builder.write_lit(careful_literal(bytes)?.0, tag)),
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

fn canonicalise_array<'a>(bytes: &'a [u8], mut builder: ArrayWriter) -> Option<&'a [u8]> {
    fn one(bytes: &mut &[u8], builder: &mut ArrayWriter) -> Option<()> {
        let (tag, b) = tag(bytes)?;
        let tag = tag.map(|x| x.tag);
        match major(b)? {
            MAJOR_POS => builder.write_pos(update3(bytes, integer(b))?, tag),
            MAJOR_NEG => builder.write_neg(update3(bytes, integer(b))?, tag),
            MAJOR_BYTES => {
                if tag == Some(TAG_CBOR_ITEM) {
                    // drop CBOR item encoding wrapper - may choose to use these later for more efficient skipping
                    let decoded = update(bytes, value_bytes(b, false))?;
                    // the line above has advanced the main loop’s reference, here we advance a temporary one
                    one(&mut decoded.as_ref(), builder)?
                } else {
                    builder.write_bytes(update(bytes, value_bytes(b, false))?.as_ref(), tag)
                }
            }
            MAJOR_STR => builder.write_str(
                std::str::from_utf8(update(bytes, value_bytes(b, false))?.as_ref()).ok()?,
                tag,
            ),
            MAJOR_ARRAY => {
                let mut res = None;
                builder.write_array_rec(tag, |builder| {
                    res = canonicalise_array(b, builder);
                });
                *bytes = res?;
            }
            MAJOR_DICT => {
                let mut res = None;
                builder.write_dict_rec(tag, |builder| {
                    res = canonicalise_dict(b, builder);
                });
                *bytes = res?;
            }
            MAJOR_LIT => builder.write_lit(update(bytes, careful_literal(b))?, tag),
            _ => return None,
        }
        Some(())
    }

    // at this point the first byte (indefinite array) has already been written
    let (len, _, mut bytes) = integer(bytes).or_else(|| indefinite(bytes))?;
    if len == u64::MAX {
        // marker for indefinite size
        while *bytes.get(0)? != STOP_BYTE {
            one(&mut bytes, &mut builder)?;
        }
        Some(&bytes[1..])
    } else {
        for _ in 0..len {
            one(&mut bytes, &mut builder)?;
        }
        Some(bytes)
    }
}

fn canonicalise_dict<'a>(bytes: &'a [u8], mut builder: DictWriter) -> Option<&'a [u8]> {
    fn key<'b>(bytes_ref: &mut &'b [u8]) -> Option<Cow<'b, str>> {
        use crate::reader::ValueResult::*;

        let (tag, rest) = tag(bytes_ref)?;
        let (value, bytes, rest) = value(rest)?;
        *bytes_ref = rest;
        match value {
            V(kind) => {
                let value = CborValue { tag, kind, bytes };
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
        let (tag, b) = tag(bytes)?;
        let tag = tag.map(|x| x.tag);
        match major(b)? {
            MAJOR_POS => builder.write_pos(key, update3(bytes, integer(b))?, tag),
            MAJOR_NEG => builder.write_neg(key, update3(bytes, integer(b))?, tag),
            MAJOR_BYTES => {
                if tag == Some(TAG_CBOR_ITEM) {
                    // drop CBOR item encoding wrapper - may choose to use these later for more efficient skipping
                    let decoded = update(bytes, value_bytes(b, false))?;
                    // the line above has advanced the main loop’s reference, here we advance a temporary one
                    one(&mut decoded.as_ref(), key, builder)?
                } else {
                    builder.write_bytes(key, update(bytes, value_bytes(b, false))?.as_ref(), tag)
                }
            }
            MAJOR_STR => builder.write_str(
                key,
                std::str::from_utf8(update(bytes, value_bytes(b, false))?.as_ref()).ok()?,
                tag,
            ),
            MAJOR_ARRAY => {
                let mut res = None;
                builder.write_array_rec(key, tag, |builder| {
                    res = canonicalise_array(b, builder);
                });
                *bytes = res?;
            }
            MAJOR_DICT => {
                let mut res = None;
                builder.write_dict_rec(key, tag, |builder| {
                    res = canonicalise_dict(b, builder);
                });
                *bytes = res?;
            }
            MAJOR_LIT => builder.write_lit(key, update(bytes, careful_literal(b))?, tag),
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
            one(&mut bytes, key.as_ref(), &mut builder)?;
        }
        Some(&bytes[1..])
    } else {
        for _ in 0..len {
            let key = key(&mut bytes)?;
            one(&mut bytes, key.as_ref(), &mut builder)?;
        }
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::ValueKind::*;
    use crate::CborValue;

    #[test]
    fn remove_cbor_encoding() {
        let item = CborBuilder::default().write_null(None);
        let item_str = CborBuilder::default().write_str("v", None);
        let array = CborBuilder::default()
            .write_array_rec(None, |mut builder| {
                builder.write_bytes(item.as_slice(), Some(TAG_CBOR_ITEM));
                builder.write_bytes(item_str.as_slice(), Some(TAG_CBOR_ITEM));
            })
            .0;
        let dict = CborBuilder::default()
            .write_dict_rec(None, |mut builder| {
                builder.write_bytes("a", item.as_slice(), Some(TAG_CBOR_ITEM));
                builder.write_bytes("b", item_str.as_slice(), Some(TAG_CBOR_ITEM));
            })
            .0;
        let nested_array = CborBuilder::default()
            .write_array_rec(None, |mut builder| {
                builder.write_bytes(array.as_slice(), Some(TAG_CBOR_ITEM));
                builder.write_bytes(dict.as_slice(), Some(TAG_CBOR_ITEM));
            })
            .0;
        let nested_dict = CborBuilder::default()
            .write_dict_rec(None, |mut builder| {
                builder.write_bytes("a", array.as_slice(), Some(TAG_CBOR_ITEM));
                builder.write_bytes("b", dict.as_slice(), Some(TAG_CBOR_ITEM));
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
            format!(
                "{:?}",
                CborOwned::canonical(encoded_array.as_slice(), None).unwrap()
            ),
            "Cbor(9f 9f f6 61 76 ff bf 61 61 f6 61 62 61 76 ff ff)".to_owned()
        );
        assert_eq!(
            format!(
                "{:?}",
                CborOwned::canonical(encoded_dict.as_slice(), None).unwrap()
            ),
            "Cbor(bf 61 61 9f f6 61 76 ff 61 62 bf 61 61 f6 61 62 61 76 ff ff)".to_owned()
        );
    }
}
