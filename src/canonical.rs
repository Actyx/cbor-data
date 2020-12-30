use crate::{
    constants::*,
    reader::{careful_literal, indefinite, integer, major, tag, value_bytes},
    CborBuilder, CborOwned, WriteToArray, WriteToDict,
};

// TODO canonicalise CBOR-encoded byte string values
pub fn canonicalise(bytes: &[u8], builder: CborBuilder<'_>) -> Option<CborOwned> {
    let (tag, bytes) = tag(bytes)?;
    let tag = tag.map(|x| x.tag);
    match major(bytes)? {
        MAJOR_POS => Some(builder.write_pos(integer(bytes)?.0, tag)),
        MAJOR_NEG => Some(builder.write_neg(integer(bytes)?.0, tag)),
        MAJOR_BYTES => Some(builder.write_bytes(value_bytes(bytes, false)?.0.as_ref(), tag)),
        MAJOR_STR => Some(builder.write_str(
            std::str::from_utf8(value_bytes(bytes, false)?.0.as_ref()).ok()?,
            tag,
        )),
        // TODO keep definite size arrays definite size if len < 24
        MAJOR_ARRAY => {
            let mut builder = builder.write_array(tag);
            canonicalise_array(bytes, &mut builder)?;
            Some(builder.finish())
        }
        MAJOR_DICT => {
            let mut builder = builder.write_dict(tag);
            canonicalise_dict(bytes, &mut builder)?;
            Some(builder.finish())
        }
        MAJOR_LIT => Some(builder.write_lit(careful_literal(bytes)?.0, tag)),
        _ => None,
    }
}

fn update<'a, T>(b: &mut &'a [u8], val: Option<(T, &'a [u8])>) -> Option<T> {
    match val {
        Some((t, r)) => {
            *b = r;
            Some(t)
        }
        None => None,
    }
}

fn update3<'a, T>(b: &mut &'a [u8], val: Option<(T, &'a [u8], &'a [u8])>) -> Option<T> {
    match val {
        Some((t, _, r)) => {
            *b = r;
            Some(t)
        }
        None => None,
    }
}

fn canonicalise_array<'a>(bytes: &'a [u8], builder: &mut dyn WriteToArray) -> Option<&'a [u8]> {
    fn one(bytes: &mut &[u8], builder: &mut dyn WriteToArray) -> Option<()> {
        let (tag, b) = tag(bytes)?;
        let tag = tag.map(|x| x.tag);
        match major(b)? {
            MAJOR_POS => builder.write_pos(update3(bytes, integer(b))?, tag),
            MAJOR_NEG => builder.write_neg(update3(bytes, integer(b))?, tag),
            MAJOR_BYTES => builder.write_bytes(update(bytes, value_bytes(b, false))?.as_ref(), tag),
            MAJOR_STR => builder.write_str(
                std::str::from_utf8(update(bytes, value_bytes(b, false))?.as_ref()).ok()?,
                tag,
            ),
            MAJOR_ARRAY => {
                let mut res = None;
                builder.write_array_rec(tag, &mut |builder| {
                    res = canonicalise_array(b, builder);
                });
                *bytes = res?;
            }
            MAJOR_DICT => {
                let mut res = None;
                builder.write_dict_rec(tag, &mut |builder| {
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

fn canonicalise_dict<'a>(bytes: &'a [u8], builder: &mut dyn WriteToDict) -> Option<&'a [u8]> {
    fn one(bytes: &mut &[u8], builder: &mut dyn WriteToDict) -> Option<()> {
        if major(bytes)? != MAJOR_STR {
            return None;
        }
        let (key, b) = value_bytes(bytes, false)?;
        let key = std::str::from_utf8(key.as_ref()).ok()?;
        let (tag, b) = tag(b)?;
        let tag = tag.map(|x| x.tag);
        match major(b)? {
            MAJOR_POS => builder.write_pos(key, update3(bytes, integer(b))?, tag),
            MAJOR_NEG => builder.write_neg(key, update3(bytes, integer(b))?, tag),
            MAJOR_BYTES => {
                builder.write_bytes(key, update(bytes, value_bytes(b, false))?.as_ref(), tag)
            }
            MAJOR_STR => builder.write_str(
                key,
                std::str::from_utf8(update(bytes, value_bytes(b, false))?.as_ref()).ok()?,
                tag,
            ),
            MAJOR_ARRAY => {
                let mut res = None;
                builder.write_array_rec(key, tag, &mut |builder| {
                    res = canonicalise_array(b, builder);
                });
                *bytes = res?;
            }
            MAJOR_DICT => {
                let mut res = None;
                builder.write_dict_rec(key, tag, &mut |builder| {
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
