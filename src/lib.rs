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
    Neg(i64),
    Float(f64),
    String(&'a str),
    Bytes(&'a [u8]),
    Bool(bool),
    Null,
    Undefined,
    Composite(Cbor<'a>),
}

impl Cbor<'static> {
    pub fn new(bytes: impl AsRef<[u8]>) -> Self {
        // FIXME canonicalise (no chunking nor indefinite size)
        Self(Cow::Owned(bytes.as_ref().to_owned()))
    }
}

impl<'a> Cbor<'a> {
    pub fn value(&self) -> Option<(CborValue, Option<u64>)> {
        value(self.b()).map(|v| (v, tag(self.b()).map(|x| x.0)))
    }

    pub fn ptr<'b>(&self, path: impl Iterator<Item = &'b str>) -> Option<CborValue> {
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

fn info(bytes: &[u8]) -> Option<(u64, &[u8])> {
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
    let (len, rest) = info(bytes)?;
    let len = len as usize;
    Some((&rest[..len], &rest[len..]))
}

fn u64(bytes: &[u8]) -> Option<(u64, &[u8])> {
    info(bytes)
}

fn i64(bytes: &[u8]) -> Option<(i64, &[u8])> {
    info(bytes).map(|(x, rest)| (-1 - (x as i64), rest))
}

fn f64(bytes: &[u8]) -> Option<(f64, &[u8])> {
    info(bytes).and_then(|(x, rest)| match bytes.len() - rest.len() {
        5 => Some((f32::from_bits(x as u32) as f64, rest)),
        9 => Some((f64::from_bits(x), rest)),
        _ => None,
    })
}

fn str(bytes: &[u8]) -> Option<(&str, &[u8])> {
    value_bytes(bytes)
        .and_then(|(bytes, rest)| std::str::from_utf8(bytes).ok().map(|s| (s, (rest))))
}

fn skip(bytes: &[u8]) -> &[u8] {
    match major(bytes) {
        MAJOR_POS | MAJOR_NEG | MAJOR_LIT => info(bytes).unwrap().1,
        MAJOR_STR | MAJOR_BYTES => value_bytes(bytes).unwrap().1,
        MAJOR_TAG => skip(info(bytes).unwrap().1),
        MAJOR_ARRAY => {
            let (len, mut rest) = info(bytes).unwrap();
            for _ in 0..len {
                rest = skip(rest);
            }
            rest
        }
        MAJOR_DICT => {
            let (len, mut rest) = info(bytes).unwrap();
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
        info(bytes)
    } else {
        None
    }
}

fn value(bytes: &[u8]) -> Option<CborValue> {
    match major(bytes) {
        MAJOR_POS => Some(CborValue::Pos(u64(bytes)?.0)),
        MAJOR_NEG => Some(CborValue::Neg(i64(bytes)?.0)),
        MAJOR_STR => Some(CborValue::String(str(bytes)?.0)),
        MAJOR_BYTES => Some(CborValue::Bytes(value_bytes(bytes)?.0)),
        MAJOR_LIT => match bytes[0] & 31 {
            20 => Some(CborValue::Bool(false)),
            21 => Some(CborValue::Bool(true)),
            22 => Some(CborValue::Null),
            23 => Some(CborValue::Undefined),
            26 => Some(CborValue::Float(f64(bytes)?.0)),
            27 => Some(CborValue::Float(f64(bytes)?.0)),
            _ => None,
        },
        MAJOR_TAG => info(bytes).and_then(|(_, rest)| value(rest)),
        MAJOR_ARRAY | MAJOR_DICT => {
            let rest = skip(bytes);
            let len = bytes.len() - rest.len();
            Some(CborValue::Composite(Cbor::borrow(&bytes[..len])))
        }
        _ => None,
    }
}

fn ptr<'b>(bytes: &[u8], mut path: impl Iterator<Item = &'b str>) -> Option<CborValue> {
    match path.next() {
        Some(p) => match major(bytes) {
            MAJOR_ARRAY => {
                let mut idx = usize::from_str(p).ok()?;
                let (len, mut rest) = info(bytes)?;
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
                let (len, mut rest) = info(bytes)?;
                for _ in 0..len {
                    let (key, r) = str(rest)?;
                    if key == p {
                        return ptr(r, path);
                    }
                    rest = skip(r);
                }
                None
            }
            _ => None,
        },
        None => value(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        vec![
            0b101_00010,
            0b011_00001,
            b'a',
            0b101_00001,
            0b011_00001,
            b'b',
            0b000_01100,
            0b011_00001,
            b'c',
            0b111_10111,
        ]
    }

    #[test]
    fn must_value() {
        assert_eq!(ptr(&*sample(), "a.b".split('.')), Some(CborValue::Pos(12)));
        assert_eq!(ptr(&*sample(), "c".split('.')), Some(CborValue::Undefined));
    }
}
