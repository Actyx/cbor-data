use crate::{constants::*, reader::tagged_value};

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

    fn tag(&self) -> Option<u64> {
        self.tag.as_ref().map(|t| t.tag)
    }

    /// Try to interpret this value as a 64bit unsigned integer.
    ///
    /// Returns None if it is not an integer type or does not fit into 64 bits.
    pub fn as_u64(&self) -> Option<u64> {
        // TODO should also check for bigint
        match self.decoded()?.kind {
            Pos(x) => Some(x),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self.decoded()?.kind {
            Pos(x) => Some(x as f64),
            Neg(x) => Some(-1.0 - (x as f64)),
            Float(f) => Some(f),
            _ => None,
        }
    }

    /// Try to interpret this value as string.
    ///
    /// Returns None if the type is not a (byte) string or the bytes are not valid UTF-8.
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
}
