use std::{
    borrow::Cow,
    collections::BTreeMap,
    convert::TryFrom,
    fmt::{Display, Formatter},
    iter,
};

use crate::{
    constants::*,
    reader::{integer, major, tagged_value, Iter},
    visit::visit,
    Cbor,
};

/// Lifted navigation structure for a CborValue.
///
/// You can obtain this using [`CborValue::as_object()`](struct.CborValue.html#method.as_object).
#[derive(Debug, Clone, PartialEq)]
pub enum CborObject<'a> {
    Array(Vec<CborObject<'a>>),
    Dict(BTreeMap<&'a str, CborObject<'a>>),
    Value(Option<u64>, ValueKind<'a>),
}

impl<'a> CborObject<'a> {
    pub fn depth(&self) -> usize {
        match self {
            CborObject::Array(v) => 1 + v.iter().map(|o| o.depth()).max().unwrap_or(1),
            CborObject::Dict(d) => 1 + d.values().map(|o| o.depth()).max().unwrap_or(1),
            CborObject::Value(_, _) => 1,
        }
    }
}

/// Low-level decoded form of a CBOR item. Use CborValue for inspecting values.
///
/// Beware of the `Neg` variant, which carries `-1 - x`.
#[derive(Debug, PartialEq, Clone)]
pub enum ValueKind<'a> {
    Pos(u64),
    Neg(u64),
    Float(f64),
    Str(&'a str),
    Bytes(&'a [u8]),
    Bool(bool),
    Null,
    Undefined,
    Simple(u8),
    Array,
    Dict,
}
use ValueKind::*;

impl<'a> Display for ValueKind<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Pos(x) => write!(f, "{}", x),
            Neg(x) => write!(f, "{}", -1 - (*x as i128)),
            Float(x) => {
                if *x == 0f64 && x.is_sign_negative() {
                    write!(f, "-0.0")
                } else {
                    write!(f, "{:?}", x)
                }
            }
            Str(s) => write!(f, "\"{}\"", s.escape_debug()),
            Bytes(b) => write!(
                f,
                "0x{}",
                b.iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join("")
            ),
            Bool(b) => write!(f, "{}", b),
            Null => write!(f, "null"),
            Undefined => write!(f, "undefined"),
            Simple(b) => write!(f, "simple({})", b),
            Array => write!(f, "array"),
            Dict => write!(f, "dict"),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Tag<'a> {
    pub tag: u64,
    pub bytes: &'a [u8],
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Tags<'a> {
    pub bytes: &'a [u8],
}

impl<'a> Tags<'a> {
    pub fn new(bytes: &'a [u8]) -> Option<(Self, &'a [u8])> {
        let mut remaining = bytes;
        while let Some(value) = remaining.get(0) {
            if (*value >> 5) != MAJOR_TAG {
                break;
            }
            let (_, _, r) = integer(remaining)?;
            remaining = r;
        }
        let len = bytes.len() - remaining.len();
        Some((
            Self {
                bytes: &bytes[..len],
            },
            remaining,
        ))
    }

    #[cfg(test)]
    pub fn fake(tags: impl IntoIterator<Item = u64>) -> Self {
        let mut data = Vec::new();
        crate::builder::write_tags(&mut data, tags);
        Self { bytes: data.leak() }
    }

    pub fn last(&self) -> Option<u64> {
        (*self).last()
    }
}

impl<'a> Iterator for Tags<'a> {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            None
        } else {
            let (tag, _, remaining) = integer(self.bytes)?;
            self.bytes = remaining;
            Some(tag)
        }
    }
}

/// Representation of a possibly tagged CBOR data item.
#[derive(Debug, Clone)]
pub struct CborValue<'a> {
    pub tag: Option<Tag<'a>>,
    pub kind: ValueKind<'a>,
    pub bytes: &'a [u8],
}

impl<'a> PartialEq<CborValue<'_>> for CborValue<'a> {
    fn eq(&self, other: &CborValue<'_>) -> bool {
        self.tag() == other.tag() && self.kind == other.kind
    }
}

impl<'a> Display for CborValue<'a> {
    fn fmt(&self, mut f: &mut Formatter<'_>) -> std::fmt::Result {
        type Res<T> = Result<T, std::fmt::Error>;
        impl<'a> crate::visit::Visitor<'a, std::fmt::Error> for &mut Formatter<'_> {
            fn visit_simple(&mut self, item: CborValue) -> Res<()> {
                if let Some(t) = item.tag() {
                    write!(*self, "{}|", t)?;
                }
                write!(*self, "{}", item.kind)
            }
            fn visit_array_begin(&mut self, size: Option<u64>, tag: Option<u64>) -> Res<bool> {
                if let Some(t) = tag {
                    write!(*self, "{}|", t)?;
                }
                write!(*self, "[")?;
                if size.is_none() {
                    write!(*self, "_ ")?;
                }
                Ok(true)
            }
            fn visit_array_index(&mut self, idx: u64) -> Res<bool> {
                if idx > 0 {
                    write!(*self, ", ")?;
                }
                Ok(true)
            }
            fn visit_array_end(&mut self) -> Res<()> {
                write!(*self, "]")
            }
            fn visit_dict_begin(&mut self, size: Option<u64>, tag: Option<u64>) -> Res<bool> {
                if let Some(t) = tag {
                    write!(*self, "{}|", t)?;
                }
                write!(*self, "{{")?;
                if size.is_none() {
                    write!(*self, "_ ")?;
                }
                Ok(true)
            }
            fn visit_dict_key(&mut self, key: &str, is_first: bool) -> Res<bool> {
                if !is_first {
                    write!(*self, ", ")?;
                }
                write!(*self, "\"{}\": ", key.escape_debug())?;
                Ok(true)
            }
            fn visit_dict_end(&mut self) -> Res<()> {
                write!(*self, "}}")
            }
        }
        visit(&mut f, self.clone())
    }
}

// TODO flesh out and extract data more thoroughly
impl<'a> CborValue<'a> {
    #[cfg(test)]
    pub fn fake(tag: Option<u64>, kind: ValueKind<'a>) -> Self {
        Self {
            tag: tag.map(|tag| Tag { tag, bytes: b"" }),
            kind,
            bytes: b"",
        }
    }

    /// Strip off wrappers of CBOR item encoding: sees through byte strings with
    /// [`TAG_CBOR_ITEM`](constants/constant.TAG_CBOR_ITEM.html).
    pub fn decoded(&self) -> Option<Self> {
        if let (Some(TAG_CBOR_ITEM), Bytes(b)) = (self.tag(), &self.kind) {
            tagged_value(b)?.decoded()
        } else {
            Some(self.clone())
        }
    }

    /// Get value of the innermost tag if one was provided.
    pub fn tag(&self) -> Option<u64> {
        self.tag.as_ref().map(|t| t.tag)
    }

    /// Try to interpret this value as a 64bit unsigned integer.
    ///
    /// Currently does not check floats or big integers.
    pub fn as_u64(&self) -> Option<u64> {
        // TODO should also check for bigint
        match self.decoded()?.kind {
            Pos(x) => Some(x),
            _ => None,
        }
    }

    /// Try to interpret this value as a signed integer.
    ///
    /// Currently does not check floats or big integers.
    pub fn as_i64(&self) -> Option<i64> {
        match self.decoded()?.kind {
            Pos(x) => i64::try_from(x).ok(),
            Neg(x) => i64::try_from(x).ok().map(|x| -1 - x),
            _ => None,
        }
    }

    /// Try to interpret this value as a signed integer.
    ///
    /// Currently does not check floats or big integers.
    pub fn as_i32(&self) -> Option<i32> {
        match self.decoded()?.kind {
            Pos(x) => i32::try_from(x).ok(),
            Neg(x) => i32::try_from(x).ok().map(|x| -1 - x),
            _ => None,
        }
    }

    /// Try to interpret this value as a floating-point number.
    ///
    /// TODO: add proper representations for the big number types supported by CBOR
    pub fn as_f64(&self) -> Option<f64> {
        let decoded = self.decoded()?;
        match decoded.kind {
            Pos(x) => Some(x as f64),
            Neg(x) => Some(-1.0 - (x as f64)),
            Float(f) => Some(f),
            Bytes(b) if decoded.tag() == Some(TAG_BIGNUM_POS) => Some(bytes_to_float(b)),
            Bytes(b) if decoded.tag() == Some(TAG_BIGNUM_NEG) => Some(-bytes_to_float(b)),
            Array if decoded.tag() == Some(TAG_BIGDECIMAL) => {
                let cbor = Cbor::trusting(decoded.bytes);
                let exponent = cbor.index_iter(iter::once("0"))?.as_i32()?;
                let mantissa = cbor.index_iter(iter::once("1"))?.as_f64()?;
                Some(mantissa * 10f64.powi(exponent))
            }
            Array if decoded.tag() == Some(TAG_BIGFLOAT) => {
                let cbor = Cbor::trusting(decoded.bytes);
                let exponent = cbor.index_iter(iter::once("0"))?.as_i32()?;
                let mantissa = cbor.index_iter(iter::once("1"))?.as_f64()?;
                Some(mantissa * 2f64.powi(exponent))
            }
            _ => None,
        }
    }

    /// Try to interpret this value as byte string.
    ///
    /// This returns a `Cow` because it may need to allocate a vector when decoding base64 strings.
    pub fn as_bytes(&self) -> Option<Cow<'a, [u8]>> {
        let decoded = self.decoded()?;
        match decoded.kind {
            Bytes(b) => Some(Cow::Borrowed(b)),
            Str(s) if decoded.tag() == Some(TAG_BASE64) => base64::decode(s).ok().map(Cow::Owned),
            Str(s) if decoded.tag() == Some(TAG_BASE64URL) => {
                base64::decode_config(s, base64::URL_SAFE)
                    .ok()
                    .map(Cow::Owned)
            }
            _ => None,
        }
    }

    /// Try to interpret this value as string.
    ///
    /// Returns None if the type is not a (byte) string or the bytes are not valid UTF-8.
    /// base64 encoded strings (TAG_BASE64 or TAG_BASE64URL) are not decoded but returned
    /// as they are (even when their binary decoded form is valid UTF-8).
    pub fn as_str(&self) -> Option<&'a str> {
        let decoded = self.decoded()?;
        let tag = decoded.tag();
        match self.kind {
            Str(s) => Some(s),
            Bytes(b) if tag != Some(TAG_BIGNUM_POS) && tag != Some(TAG_BIGNUM_NEG) => {
                std::str::from_utf8(b).ok()
            }
            _ => None,
        }
    }

    /// Lift a representation from this CBOR item that turns arrays into vectors and
    /// dicts into BTreeMaps.
    ///
    /// This method is mostly useful for diagnostics and tests.
    pub fn as_object(&self) -> Option<CborObject<'a>> {
        let decoded = self.decoded()?;
        match decoded.kind {
            Array => {
                let info = integer(decoded.bytes);
                let rest = info.map(|x| x.2).unwrap_or_else(|| &decoded.bytes[1..]);
                let len = info.map(|x| x.0);
                let iter = Iter::new(rest, len);
                let mut v = Vec::new();
                for i in iter {
                    v.push(i.value()?.as_object()?);
                }
                Some(CborObject::Array(v))
            }
            Dict => {
                let info = integer(decoded.bytes);
                let rest = info.map(|x| x.2).unwrap_or_else(|| &decoded.bytes[1..]);
                let len = info.map(|x| x.0 * 2);
                let mut iter = Iter::new(rest, len);
                let mut m = BTreeMap::new();
                while let Some(c) = iter.next() {
                    if let Str(key) = c.value()?.kind {
                        let value = iter.next()?.value()?.as_object()?;
                        m.insert(key, value);
                    } else {
                        return None;
                    }
                }
                Some(CborObject::Dict(m))
            }
            _ => Some(CborObject::Value(decoded.tag(), decoded.kind)),
        }
    }
}

fn bytes_to_float(bytes: &[u8]) -> f64 {
    let mut ret = 0.0;
    for x in bytes {
        ret = ret * 256.0 + (*x as f64);
    }
    ret
}

#[cfg(test)]
mod tests {
    use crate::CborOwned;

    use super::*;

    #[test]
    fn base64string() {
        fn to_cbor(s: &str, tag: u64) -> CborOwned {
            let mut v = vec![0xd8u8, tag as u8, 0x60 | (s.len() as u8)];
            v.extend_from_slice(s.as_bytes());
            CborOwned::trusting(v)
        }
        fn b(bytes: &CborOwned) -> Vec<u8> {
            bytes
                .value()
                .expect("a")
                .as_bytes()
                .expect("b")
                .into_owned()
        }

        let bytes = to_cbor("a346_-0=", TAG_BASE64URL);
        assert_eq!(b(&bytes), vec![107, 126, 58, 255, 237]);

        let bytes = to_cbor("a346_-0", TAG_BASE64URL);
        assert_eq!(b(&bytes), vec![107, 126, 58, 255, 237]);

        let bytes = to_cbor("a346/+0=", TAG_BASE64);
        assert_eq!(b(&bytes), vec![107, 126, 58, 255, 237]);

        let bytes = to_cbor("a346/+0", TAG_BASE64);
        assert_eq!(b(&bytes), vec![107, 126, 58, 255, 237]);
    }
}
