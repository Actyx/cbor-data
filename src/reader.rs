use crate::{
    constants::*,
    value::Tag,
    Cbor, CborValue,
    ValueKind::{self, *},
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

/// Low-level representation of major type 7 values.
///
/// Bool, null, and undefined are represented by L0 while L2–L4 represent the underlying
/// bytes of floating-point numbers (16-, 32-, and 64-bit IEE754).
pub enum Literal {
    L0(u8),
    L1(u8),
    L2(u16),
    L4(u32),
    L8(u64),
}

#[inline]
pub fn major(bytes: &[u8]) -> Option<u8> {
    Some(*bytes.get(0)? >> 5)
}

pub fn careful_literal(bytes: &[u8]) -> Option<(Literal, &[u8])> {
    let (int, b, rest) = integer(bytes)?;
    match b.len() {
        1 => Some((Literal::L0(int as u8), rest)),
        2 => Some((Literal::L1(int as u8), rest)),
        3 => Some((Literal::L2(int as u16), rest)),
        5 => Some((Literal::L4(int as u32), rest)),
        9 => Some((Literal::L8(int as u64), rest)),
        _ => None,
    }
}

pub fn integer(bytes: &[u8]) -> Option<(u64, &[u8], &[u8])> {
    match bytes[0] & 31 {
        // fun fact: explicit bounds checks make the code a lot smaller and faster because
        // otherwise the panic’s line number dictates a separate check for each array access
        24 => check!(
            bytes.len() > 1,
            Some((bytes[1] as u64, &bytes[..2], &bytes[2..]))
        ),
        25 => check!(
            bytes.len() > 2,
            Some((
                ((bytes[1] as u64) << 8) | (bytes[2] as u64),
                &bytes[..3],
                &bytes[3..]
            ))
        ),
        26 => check!(
            bytes.len() > 4,
            Some((
                // fun fact: these expressions compile down to mov-shl-bswap
                ((bytes[1] as u64) << 24)
                    | ((bytes[2] as u64) << 16)
                    | ((bytes[3] as u64) << 8)
                    | (bytes[4] as u64),
                &bytes[..5],
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
                &bytes[..9],
                &bytes[9..],
            ))
        ),
        x if x < 24 => Some(((x as u64), &bytes[..1], &bytes[1..])),
        _ => None,
    }
}

// inline to reuse the bounds check already made by the caller
#[inline(always)]
pub fn indefinite(bytes: &[u8]) -> Option<(u64, &[u8], &[u8])> {
    if bytes[0] & 31 == INDEFINITE_SIZE {
        Some((u64::MAX, &bytes[..1], &bytes[1..]))
    } else {
        None
    }
}

pub fn value_bytes(bytes: &[u8], skip: bool) -> Option<(Cow<[u8]>, &[u8])> {
    let m = major(bytes)?;
    let (len, _, mut rest) = integer(bytes).or_else(|| indefinite(bytes))?;
    if len == u64::MAX {
        // marker for indefinite size
        let mut b = Vec::new();
        while *rest.get(0)? != STOP_BYTE {
            if major(rest)? != m {
                return None;
            }
            let (len, _, r) = integer(rest)?;
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

fn float(bytes: &[u8]) -> Option<(f64, &[u8], &[u8])> {
    integer(bytes).and_then(|(x, b, rest)| match b.len() {
        3 => Some((half::f16::from_bits(x as u16).to_f64(), b, rest)),
        5 => Some((f32::from_bits(x as u32) as f64, b, rest)),
        9 => Some((f64::from_bits(x), b, rest)),
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
    match major(bytes)? {
        MAJOR_POS | MAJOR_NEG | MAJOR_LIT => integer(bytes).map(|(_, _, rest)| rest),
        MAJOR_STR | MAJOR_BYTES => value_bytes(bytes, true).map(|(_, rest)| rest),
        MAJOR_TAG => integer(bytes).and_then(|(_, _, rest)| skip(rest)),
        MAJOR_ARRAY => {
            let (len, _, mut rest) = integer(bytes).or_else(|| indefinite(bytes))?;
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
            let (len, _, mut rest) = integer(bytes).or_else(|| indefinite(bytes))?;
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

pub fn tag(mut bytes: &[u8]) -> Option<(Option<Tag>, &[u8])> {
    let mut tag = None;
    while major(bytes)? == MAJOR_TAG {
        let (v, b, r) = integer(bytes)?;
        tag = Some(Tag { tag: v, bytes: b });
        bytes = r;
    }
    Some((tag, bytes))
}

fn value(bytes: &[u8]) -> Option<(ValueKind, &[u8], &[u8])> {
    match major(bytes)? {
        MAJOR_POS => integer(bytes).map(|(k, b, r)| (Pos(k), b, r)),
        MAJOR_NEG => integer(bytes).map(|(k, b, r)| (Neg(k), b, r)),
        MAJOR_BYTES => match value_bytes(bytes, false)? {
            (Cow::Borrowed(s), rest) => {
                Some((Bytes(s), &bytes[..(bytes.len() - rest.len())], rest))
            }
            _ => None,
        },
        MAJOR_STR => match string(bytes)? {
            (Cow::Borrowed(s), rest) => Some((Str(s), &bytes[..(bytes.len() - rest.len())], rest)),
            _ => None,
        },
        MAJOR_LIT => match bytes[0] & 31 {
            LIT_FALSE => Some((Bool(false), &bytes[..1], &bytes[1..])),
            LIT_TRUE => Some((Bool(true), &bytes[..1], &bytes[1..])),
            LIT_NULL => Some((Null, &bytes[..1], &bytes[1..])),
            LIT_UNDEFINED => Some((Undefined, &bytes[..1], &bytes[1..])),
            LIT_SIMPLE => Some((Simple(bytes[1]), &bytes[..2], &bytes[2..])),
            LIT_FLOAT16 | LIT_FLOAT32 | LIT_FLOAT64 => {
                float(bytes).map(|(k, b, r)| (Float(k), b, r))
            }
            x if x < 24 => Some((Simple(x), &bytes[..1], &bytes[1..])),
            _ => None,
        },
        MAJOR_TAG => integer(bytes).and_then(|(_, _, rest)| value(rest)),
        MAJOR_ARRAY => skip(bytes).map(|rest| (Array, &bytes[..(bytes.len() - rest.len())], rest)),
        MAJOR_DICT => skip(bytes).map(|rest| (Dict, &bytes[..(bytes.len() - rest.len())], rest)),
        _ => None,
    }
}

pub fn tagged_value(bytes: &[u8]) -> Option<CborValue> {
    let (tag, rest) = tag(bytes)?;
    let (kind, bytes, _rest) = value(rest)?;
    Some(CborValue { tag, kind, bytes })
}

pub fn ptr<'b>(mut bytes: &[u8], mut path: impl Iterator<Item = &'b str>) -> Option<CborValue> {
    match path.next() {
        Some(p) => {
            let v = tagged_value(bytes)?.decoded()?;
            bytes = v.bytes;
            match v.kind {
                Array => {
                    let idx = u64::from_str(p).ok()?;
                    let (len, _, rest) = integer(bytes).or_else(|| indefinite(bytes))?;
                    let mut iter = Iter::new(rest, len);
                    ptr(iter.nth(idx as usize)?.as_slice(), path)
                }
                Dict => {
                    let (mut len, _, rest) = integer(bytes).or_else(|| indefinite(bytes))?;
                    if len != u64::MAX {
                        len *= 2;
                    }
                    let mut iter = Iter::new(rest, len);
                    while let Some(key) = iter.next() {
                        if let Str(s) = value(key.as_slice())?.0 {
                            let v = iter.next()?;
                            if s == p {
                                return ptr(v.as_slice(), path);
                            }
                        } else {
                            return None;
                        }
                    }
                    None
                }
                _ => None,
            }
        }
        None => tagged_value(bytes),
    }
}

pub struct Iter<'a>(&'a [u8], Option<u64>);

impl<'a> Iter<'a> {
    pub(crate) fn new(bytes: &'a [u8], len: u64) -> Self {
        if len == u64::MAX {
            Self(bytes, None)
        } else {
            Self(bytes, Some(len))
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = Cbor<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let Iter(b, elems) = self;
        if *elems == Some(0) || *elems == None && *b.get(0)? == STOP_BYTE {
            None
        } else {
            let rest = skip(b)?;
            let bytes = &b[..b.len() - rest.len()];
            if let Some(x) = elems.as_mut() {
                *x -= 1;
            }
            *b = rest;
            Some(Cbor::trusting(bytes))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Cbor, CborOwned};

    use super::*;
    use serde_json::json;

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
        assert_eq!(
            ptr(&*sample(), "c".split('.')),
            Some(CborValue::fake(None, Null))
        );
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
            let cbor = Cbor::trusting(&*bytes);
            assert_eq!(cbor.value(), None);

            let cbor = CborOwned::canonical(bytes, None).unwrap();
            assert_eq!(cbor.value(), Some(CborValue::fake(None, Str(res))));
        }
    }

    #[test]
    fn float() {
        let bytes = vec![0xfau8, 0, 0, 51, 17];
        let cbor = Cbor::trusting(&*bytes);
        assert_eq!(
            cbor.value(),
            Some(CborValue::fake(None, Float(1.8319174824118334e-41)))
        );
        let cbor = CborOwned::canonical(bytes, None).unwrap();
        assert_eq!(
            cbor.value(),
            Some(CborValue::fake(None, Float(1.8319174824118334e-41)))
        );
    }
}
