use crate::{
    builder::{WriteToArray, WriteToDict},
    constants::*,
    Cbor, CborBuilder,
    CborValue::{self, *},
    TaggedValue,
};
use std::{borrow::Cow, str::FromStr};

macro_rules! check {
    ($e:expr) => {
        if !($e) {
            return None;
        }
    };
    ($e:expr, $v:expr) => {
        if !($e) {
            return None;
        } else {
            $v
        }
    };
}

pub fn careful_major(bytes: &[u8]) -> Option<u8> {
    Some(*bytes.get(0)? >> 5)
}

pub fn major(bytes: &[u8]) -> u8 {
    bytes[0] >> 5
}

pub fn careful_integer(bytes: &[u8]) -> Option<(u64, &[u8])> {
    match bytes[0] & 31 {
        24 => check!(bytes.len() > 1, Some((bytes[1] as u64, &bytes[2..]))),
        25 => check!(
            bytes.len() > 2,
            Some((((bytes[1] as u64) << 8) | (bytes[2] as u64), &bytes[3..]))
        ),
        26 => check!(
            bytes.len() > 4,
            Some((
                ((bytes[1] as u64) << 24)
                    | ((bytes[2] as u64) << 16)
                    | ((bytes[3] as u64) << 8)
                    | (bytes[4] as u64),
                &bytes[5..],
            ))
        ),
        27 => check!(
            bytes.len() > 8,
            Some((
                ((bytes[1] as u64) << 56)
                    | ((bytes[2] as u64) << 48)
                    | ((bytes[3] as u64) << 40)
                    | ((bytes[4] as u64) << 32)
                    | ((bytes[5] as u64) << 24)
                    | ((bytes[6] as u64) << 16)
                    | ((bytes[7] as u64) << 8)
                    | (bytes[8] as u64),
                &bytes[9..],
            ))
        ),
        x if x < 24 => Some(((x as u64), &bytes[1..])),
        _ => None,
    }
}

pub fn integer(bytes: &[u8]) -> Option<(u64, &[u8])> {
    match bytes[0] & 31 {
        24 => Some((bytes[1] as u64, &bytes[2..])),
        25 => Some((((bytes[1] as u64) << 8) | (bytes[2] as u64), &bytes[3..])),
        26 => Some((
            ((bytes[1] as u64) << 24)
                | ((bytes[2] as u64) << 16)
                | ((bytes[3] as u64) << 8)
                | (bytes[4] as u64),
            &bytes[5..],
        )),
        27 => Some((
            ((bytes[1] as u64) << 56)
                | ((bytes[2] as u64) << 48)
                | ((bytes[3] as u64) << 40)
                | ((bytes[4] as u64) << 32)
                | ((bytes[5] as u64) << 24)
                | ((bytes[6] as u64) << 16)
                | ((bytes[7] as u64) << 8)
                | (bytes[8] as u64),
            &bytes[9..],
        )),
        x if x < 24 => Some(((x as u64), &bytes[1..])),
        _ => None,
    }
}

fn indefinite(bytes: &[u8]) -> Option<(u64, &[u8])> {
    if bytes[0] & 31 == INDEFINITE_SIZE {
        Some((u64::MAX, &bytes[1..]))
    } else {
        None
    }
}

fn value_bytes(bytes: &[u8], skip: bool) -> Option<(Cow<[u8]>, &[u8])> {
    let m = major(bytes);
    let (len, mut rest) = integer(bytes).or_else(|| indefinite(bytes))?;
    if len == u64::MAX {
        // marker for indefinite size
        let mut b = Vec::new();
        while rest[0] != STOP_BYTE {
            if major(rest) != m {
                return None;
            }
            let (len, r) = integer(rest)?;
            if len == u64::MAX {
                return None;
            }
            let len = len as usize;
            if !skip {
                b.extend_from_slice(&r[..len]);
            }
            rest = &r[len..];
        }
        Some((Cow::Owned(b), rest))
    } else {
        let len = len as usize;
        Some((Cow::Borrowed(&rest[..len]), &rest[len..]))
    }
}

fn careful_value_bytes(bytes: &[u8], skip: bool) -> Option<(Cow<[u8]>, &[u8])> {
    let m = major(bytes);
    let (len, mut rest) = careful_integer(bytes).or_else(|| indefinite(bytes))?;
    if len == u64::MAX {
        // marker for indefinite size
        let mut b = Vec::new();
        while *rest.get(0)? != STOP_BYTE {
            if major(rest) != m {
                return None;
            }
            let (len, r) = careful_integer(rest)?;
            if len == u64::MAX || len as usize > r.len() {
                return None;
            }
            let len = len as usize;
            if !skip {
                b.extend_from_slice(&r[..len]);
            }
            rest = &r[len..];
        }
        Some((Cow::Owned(b), rest))
    } else {
        let len = len as usize;
        check!(
            rest.len() >= len,
            Some((Cow::Borrowed(&rest[..len]), &rest[len..]))
        )
    }
}

fn float(bytes: &[u8]) -> Option<(f64, &[u8])> {
    integer(bytes).and_then(|(x, rest)| match bytes.len() - rest.len() {
        5 => Some((f32::from_bits(x as u32) as f64, rest)),
        9 => Some((f64::from_bits(x), rest)),
        _ => None,
    })
}

fn string(bytes: &[u8]) -> Option<(Cow<str>, &[u8])> {
    value_bytes(bytes, false).and_then(|(bytes, rest)| match bytes {
        Cow::Borrowed(b) => std::str::from_utf8(b)
            .ok()
            .map(|s| (Cow::Borrowed(s), rest)),
        Cow::Owned(b) => String::from_utf8(b).ok().map(|s| (Cow::Owned(s), rest)),
    })
}

fn skip(bytes: &[u8]) -> Option<&[u8]> {
    match major(bytes) {
        MAJOR_POS | MAJOR_NEG | MAJOR_LIT => integer(bytes).map(|x| x.1),
        MAJOR_STR | MAJOR_BYTES => value_bytes(bytes, true).map(|x| x.1),
        MAJOR_TAG => skip(integer(bytes)?.1),
        MAJOR_ARRAY => {
            let (len, mut rest) = integer(bytes).or_else(|| indefinite(bytes))?;
            if len == u64::MAX {
                // marker for indefinite size
                while rest[0] != STOP_BYTE {
                    rest = skip(rest)?;
                }
                rest = &rest[1..];
            } else {
                for _ in 0..len {
                    rest = skip(rest)?;
                }
            }
            Some(rest)
        }
        MAJOR_DICT => {
            let (len, mut rest) = integer(bytes).or_else(|| indefinite(bytes))?;
            if len == u64::MAX {
                // marker for indefinite size
                while rest[0] != STOP_BYTE {
                    rest = skip(rest)?;
                    rest = skip(rest)?;
                }
                rest = &rest[1..];
            } else {
                for _ in 0..len {
                    rest = skip(rest)?;
                    rest = skip(rest)?;
                }
            }
            Some(rest)
        }
        _ => unreachable!(),
    }
}

fn careful_tag(mut bytes: &[u8]) -> Option<(Option<u64>, &[u8])> {
    let mut tag = None;
    while careful_major(bytes)? == MAJOR_TAG {
        let (v, r) = careful_integer(bytes)?;
        tag = Some(v);
        bytes = r;
    }
    Some((tag, bytes))
}

fn tag(mut bytes: &[u8]) -> (Option<u64>, &[u8]) {
    let mut tag = None;
    while major(bytes) == MAJOR_TAG {
        let (v, r) = integer(bytes).unwrap();
        tag = Some(v);
        bytes = r;
    }
    (tag, bytes)
}

fn value(bytes: &[u8]) -> Option<CborValue> {
    match major(bytes) {
        MAJOR_POS => Some(Pos(integer(bytes)?.0)),
        MAJOR_NEG => Some(Neg(integer(bytes)?.0)),
        MAJOR_BYTES => match value_bytes(bytes, false)? {
            (Cow::Borrowed(s), _) => Some(Bytes(s)),
            (Cow::Owned(s), _) => Some(BytesOwned(s)),
        },
        MAJOR_STR => match string(bytes)? {
            (Cow::Borrowed(s), _) => Some(Str(s)),
            (Cow::Owned(s), _) => Some(StrOwned(s)),
        },
        MAJOR_LIT => match bytes[0] & 31 {
            LIT_FALSE => Some(Bool(false)),
            LIT_TRUE => Some(Bool(true)),
            LIT_NULL => Some(Null),
            LIT_UNDEFINED => Some(Undefined),
            LIT_FLOAT32 => Some(Float(float(bytes)?.0)),
            LIT_FLOAT64 => Some(Float(float(bytes)?.0)),
            _ => None,
        },
        MAJOR_TAG => integer(bytes).and_then(|(_, rest)| value(rest)),
        MAJOR_ARRAY | MAJOR_DICT => {
            let rest = skip(bytes)?;
            let len = bytes.len() - rest.len();
            Some(Composite(Cbor::borrow(&bytes[..len])))
        }
        _ => None,
    }
}

pub fn tagged_value(bytes: &[u8]) -> Option<TaggedValue> {
    value(bytes).map(|v| match tag(bytes).0 {
        Some(tag) => v.with_tag(tag),
        None => v.without_tag(),
    })
}

pub fn ptr<'b>(mut bytes: &[u8], mut path: impl Iterator<Item = &'b str>) -> Option<TaggedValue> {
    match path.next() {
        Some(p) => {
            while major(bytes) == MAJOR_TAG {
                bytes = integer(bytes)?.1;
            }
            match major(bytes) {
                MAJOR_ARRAY => {
                    let mut idx = u64::from_str(p).ok()?;
                    let (len, mut rest) = integer(bytes).or_else(|| indefinite(bytes))?;
                    if len == u64::MAX {
                        // marker for indefinite size
                        while idx > 0 && rest[0] != STOP_BYTE {
                            rest = skip(rest)?;
                            idx -= 1;
                        }
                        if rest[0] == STOP_BYTE {
                            None
                        } else {
                            ptr(rest, path)
                        }
                    } else if idx < len {
                        while idx > 0 {
                            rest = skip(rest)?;
                            idx -= 1;
                        }
                        ptr(rest, path)
                    } else {
                        None
                    }
                }
                MAJOR_DICT => {
                    let (len, mut rest) = integer(bytes).or_else(|| indefinite(bytes))?;
                    if len == u64::MAX {
                        // marker for indefinite size
                        while rest[0] != STOP_BYTE {
                            let (key, r) = string(rest)?;
                            if key == p {
                                return ptr(r, path);
                            }
                            rest = skip(r)?;
                        }
                        None
                    } else {
                        for _ in 0..len {
                            let (key, r) = string(rest)?;
                            if key == p {
                                return ptr(r, path);
                            }
                            rest = skip(r)?;
                        }
                        None
                    }
                }
                _ => None,
            }
        }
        None => tagged_value(bytes),
    }
}

pub fn canonicalise(bytes: &[u8], builder: CborBuilder) -> Option<Cbor<'static>> {
    let (tag, bytes) = careful_tag(bytes)?;
    match major(bytes) {
        MAJOR_POS => Some(builder.write_pos(careful_integer(bytes)?.0, tag)),
        MAJOR_NEG => Some(builder.write_neg(careful_integer(bytes)?.0, tag)),
        MAJOR_BYTES => {
            Some(builder.write_bytes(careful_value_bytes(bytes, false)?.0.as_ref(), tag))
        }
        MAJOR_STR => Some(builder.write_str(
            std::str::from_utf8(careful_value_bytes(bytes, false)?.0.as_ref()).ok()?,
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
        MAJOR_LIT => Some(builder.write_lit(careful_integer(bytes)?.0, tag)),
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

fn canonicalise_array<'a>(bytes: &'a [u8], builder: &mut dyn WriteToArray) -> Option<&'a [u8]> {
    fn one(bytes: &mut &[u8], builder: &mut dyn WriteToArray) -> Option<()> {
        let (tag, b) = careful_tag(bytes)?;
        match major(b) {
            MAJOR_POS => builder.write_pos(update(bytes, careful_integer(b))?, tag),
            MAJOR_NEG => builder.write_neg(update(bytes, careful_integer(b))?, tag),
            MAJOR_BYTES => {
                builder.write_bytes(update(bytes, careful_value_bytes(b, false))?.as_ref(), tag)
            }
            MAJOR_STR => builder.write_str(
                std::str::from_utf8(update(bytes, careful_value_bytes(b, false))?.as_ref()).ok()?,
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
            MAJOR_LIT => builder.write_lit(update(bytes, careful_integer(b))?, tag),
            _ => return None,
        }
        Some(())
    }

    // at this point the first byte (indefinite array) has already been written
    let (len, mut bytes) = careful_integer(bytes).or_else(|| indefinite(bytes))?;
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
        if major(bytes) != MAJOR_STR {
            return None;
        }
        let (key, b) = careful_value_bytes(bytes, false)?;
        let key = std::str::from_utf8(key.as_ref()).ok()?;
        let (tag, b) = careful_tag(b)?;
        match major(b) {
            MAJOR_POS => builder.write_pos(key, update(bytes, careful_integer(b))?, tag),
            MAJOR_NEG => builder.write_neg(key, update(bytes, careful_integer(b))?, tag),
            MAJOR_BYTES => builder.write_bytes(
                key,
                update(bytes, careful_value_bytes(b, false))?.as_ref(),
                tag,
            ),
            MAJOR_STR => builder.write_str(
                key,
                std::str::from_utf8(update(bytes, careful_value_bytes(b, false))?.as_ref()).ok()?,
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
            MAJOR_LIT => builder.write_lit(key, update(bytes, careful_integer(b))?, tag),
            _ => return None,
        }
        Some(())
    }

    // at this point the first byte (indefinite array) has already been written
    let (len, mut bytes) = careful_integer(bytes).or_else(|| indefinite(bytes))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use TaggedValue::*;

    fn sample() -> Vec<u8> {
        serde_cbor::to_vec(&json!({
            "a": {
                "b": 12
            },
            "c": null
        }))
        .unwrap()
    }

    #[test]
    fn must_read_serde() {
        assert_eq!(
            ptr(&*sample(), "a.b".split('.')).and_then(|x| x.as_u64()),
            Some(12)
        );
        assert_eq!(ptr(&*sample(), "c".split('.')), Some(Plain(Null)));
    }

    #[test]
    fn indefinite_strings() {
        let cases = vec![
            // 2 chunks (with unicode)
            (
                "exampleα≤β",
                vec![
                    0x7fu8, 0x67, 101, 120, 97, 109, 112, 108, 101, 0x67, 206, 177, 226, 137, 164,
                    206, 178, 0xff,
                ],
            ),
            // 1 chunk
            (
                "example",
                vec![0x7fu8, 0x67, 101, 120, 97, 109, 112, 108, 101, 0xff],
            ),
            // 0 chunks
            ("", vec![0x7fu8, 0xff]),
            // empty chunk
            ("", vec![0x7fu8, 0x60, 0xff]),
        ];

        for (res, bytes) in cases {
            let cbor = Cbor::new(&*bytes);
            assert_eq!(cbor.value(), Some(Plain(StrOwned(res.to_owned()))));

            let cbor = Cbor::canonical(bytes).unwrap();
            assert_eq!(cbor.value(), Some(Plain(Str(res))));
        }
    }
}
