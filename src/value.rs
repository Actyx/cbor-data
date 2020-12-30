use std::{borrow::Cow, collections::BTreeMap, convert::TryFrom, iter};

use crate::{
    constants::*,
    reader::{indefinite, integer, tagged_value, Iter},
    Cbor,
};

#[derive(Debug, Clone, PartialEq)]
pub enum CborObject<'a> {
    Array(Vec<CborObject<'a>>),
    Dict(BTreeMap<&'a str, CborObject<'a>>),
    Value(Option<u64>, ValueKind<'a>),
}

/// Low-level decoded form of a CBOR item. Use TaggedValue for inspecting values.
///
/// Beware of the `Neg` variant, which carries `-1 - x`.
///
/// The Owned variants are only generated when decoding indefinite size (byte) strings in order
/// to present a contiguous slice of memory. You will never see these if you used
/// [`Cbor::canonical()`](struct.Cbor#method.canonical).
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

#[derive(Debug, PartialEq, Clone)]
pub struct Tag<'a> {
    pub tag: u64,
    pub bytes: &'a [u8],
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

    /// strip off wrappers of CBOR encoded item
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
                let (len, _bytes, rest) =
                    integer(decoded.bytes).or_else(|| indefinite(decoded.bytes))?;
                let iter = Iter::new(rest, len);
                let mut v = Vec::new();
                for i in iter {
                    v.push(i.value()?.as_object()?);
                }
                Some(CborObject::Array(v))
            }
            Dict => {
                let (mut len, _bytes, rest) =
                    integer(decoded.bytes).or_else(|| indefinite(decoded.bytes))?;
                if len != u64::MAX {
                    len *= 2;
                }
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