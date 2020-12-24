use std::{borrow::Cow, str::FromStr};

const MAJOR_POS: u8 = 0;
const MAJOR_NEG: u8 = 1;
const MAJOR_BYTES: u8 = 2;
const MAJOR_STR: u8 = 3;
const MAJOR_ARRAY: u8 = 4;
const MAJOR_DICT: u8 = 5;
const MAJOR_TAG: u8 = 6;
const MAJOR_LIT: u8 = 7;

#[derive(Clone, Debug, PartialEq)]
pub struct Cbor<'a>(Cow<'a, [u8]>);

#[derive(Clone, Debug, PartialEq)]
pub enum CborValue<'a> {
    Pos(u64),
    Neg(u64),
    Float(f64),
    String(&'a str),
    Bytes(&'a [u8]),
    Bool(bool),
    Null,
    Undefined,
    Composite(Cbor<'a>),
}
use CborValue::*;

impl<'a> CborValue<'a> {
    pub fn with_tag(self, tag: u64) -> TaggedValue<'a> {
        Tagged(tag, self)
    }
    pub fn without_tag(self) -> TaggedValue<'a> {
        Plain(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TaggedValue<'a> {
    Plain(CborValue<'a>),
    Tagged(u64, CborValue<'a>),
}
use TaggedValue::*;

// TODO flesh out and extract data more thoroughly
impl<'a> TaggedValue<'a> {
    /// Try to interpret this value as a 64bit unsigned integer.
    ///
    /// Returns None if it is not an integer type or does not fit into 64 bits.
    pub fn as_u64(&self) -> Option<u64> {
        // TODO should also check for bigint
        match self {
            Plain(Pos(x)) => Some(*x),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Plain(Pos(x)) => Some(*x as f64),
            Plain(Neg(x)) => Some(-1.0 - (*x as f64)),
            Plain(Float(f)) => Some(*f),
            _ => None,
        }
    }

    /// Try to interpret this value as string.
    ///
    /// Returns None if the type is not a (byte) string or the bytes are not valid UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Plain(String(s)) => Some(*s),
            Plain(Bytes(b)) => std::str::from_utf8(*b).ok(),
            _ => None,
        }
    }
}

impl Cbor<'static> {
    pub fn new(bytes: impl AsRef<[u8]>) -> Self {
        let mut v = Vec::with_capacity(bytes.as_ref().len());
        v.copy_from_slice(bytes.as_ref());
        Self(Cow::Owned(v))
    }
}

impl<'a> Cbor<'a> {
    pub fn value(&self) -> Option<TaggedValue> {
        tagged_value(self.b())
    }

    pub fn ptr<'b>(&self, path: impl Iterator<Item = &'b str>) -> Option<TaggedValue> {
        ptr(self.b(), path)
    }

    fn borrow(bytes: &'a [u8]) -> Self {
        Self(Cow::Borrowed(bytes))
    }

    fn b(&self) -> &[u8] {
        self.0.as_ref()
    }
}

fn major(bytes: &[u8]) -> u8 {
    bytes[0] >> 5
}

fn integer(bytes: &[u8]) -> Option<(u64, &[u8])> {
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

fn value_bytes(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let (len, rest) = integer(bytes)?;
    let len = len as usize;
    Some((&rest[..len], &rest[len..]))
}

fn float(bytes: &[u8]) -> Option<(f64, &[u8])> {
    integer(bytes).and_then(|(x, rest)| match bytes.len() - rest.len() {
        5 => Some((f32::from_bits(x as u32) as f64, rest)),
        9 => Some((f64::from_bits(x), rest)),
        _ => None,
    })
}

fn string(bytes: &[u8]) -> Option<(&str, &[u8])> {
    value_bytes(bytes)
        .and_then(|(bytes, rest)| std::str::from_utf8(bytes).ok().map(|s| (s, (rest))))
}

fn skip(bytes: &[u8]) -> &[u8] {
    match major(bytes) {
        MAJOR_POS | MAJOR_NEG | MAJOR_LIT => integer(bytes).unwrap().1,
        MAJOR_STR | MAJOR_BYTES => value_bytes(bytes).unwrap().1,
        MAJOR_TAG => skip(integer(bytes).unwrap().1),
        MAJOR_ARRAY => {
            let (len, mut rest) = integer(bytes).unwrap();
            for _ in 0..len {
                rest = skip(rest);
            }
            rest
        }
        MAJOR_DICT => {
            let (len, mut rest) = integer(bytes).unwrap();
            for _ in 0..len {
                rest = skip(rest);
                rest = skip(rest);
            }
            rest
        }
        _ => unreachable!(),
    }
}

fn tag(bytes: &[u8]) -> Option<(u64, &[u8])> {
    if major(bytes) == MAJOR_TAG {
        integer(bytes)
    } else {
        None
    }
}

fn value(bytes: &[u8]) -> Option<CborValue> {
    match major(bytes) {
        MAJOR_POS => Some(Pos(integer(bytes)?.0)),
        MAJOR_NEG => Some(Neg(integer(bytes)?.0)),
        MAJOR_STR => Some(String(string(bytes)?.0)),
        MAJOR_BYTES => Some(Bytes(value_bytes(bytes)?.0)),
        MAJOR_LIT => match bytes[0] & 31 {
            20 => Some(Bool(false)),
            21 => Some(Bool(true)),
            22 => Some(Null),
            23 => Some(Undefined),
            26 => Some(Float(float(bytes)?.0)),
            27 => Some(Float(float(bytes)?.0)),
            _ => None,
        },
        MAJOR_TAG => integer(bytes).and_then(|(_, rest)| value(rest)),
        MAJOR_ARRAY | MAJOR_DICT => {
            let rest = skip(bytes);
            let len = bytes.len() - rest.len();
            Some(Composite(Cbor::borrow(&bytes[..len])))
        }
        _ => None,
    }
}

fn tagged_value(bytes: &[u8]) -> Option<TaggedValue> {
    value(bytes).map(|v| match tag(bytes) {
        Some((tag, _)) => v.with_tag(tag),
        None => v.without_tag(),
    })
}

fn ptr<'b>(bytes: &[u8], mut path: impl Iterator<Item = &'b str>) -> Option<TaggedValue> {
    match path.next() {
        Some(p) => match major(bytes) {
            MAJOR_ARRAY => {
                let mut idx = usize::from_str(p).ok()?;
                let (len, mut rest) = integer(bytes)?;
                if idx < (len as usize) {
                    while idx > 0 {
                        rest = skip(rest);
                        idx -= 1;
                    }
                    ptr(bytes, path)
                } else {
                    None
                }
            }
            MAJOR_DICT => {
                let (len, mut rest) = integer(bytes)?;
                for _ in 0..len {
                    let (key, r) = string(rest)?;
                    if key == p {
                        return ptr(r, path);
                    }
                    rest = skip(r);
                }
                None
            }
            _ => None,
        },
        None => tagged_value(bytes),
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(ptr(&*sample(), "c".split('.')), Some(Plain(Null)));
    }

    #[test]
    #[ignore]
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
            let cbor = Cbor::new(bytes);
            assert_eq!(cbor.value(), Some(Plain(String(res))));
        }
    }
}
