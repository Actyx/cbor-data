use std::borrow::Cow;

use crate::{constants::*, reader::tagged_value, Cbor};

/// Low-level decoded form of a CBOR item. Use TaggedValue for inspecting values.
///
/// Beware of the `Neg` variant, which carries `-1 - x`.
///
/// The Owned variants are only generated when decoding indefinite size (byte) strings in order
/// to present a contiguous slice of memory. You will never see these if you used
/// [`Cbor::canonical()`](struct.Cbor#method.canonical).
#[derive(Debug, PartialEq)]
pub enum CborValue<'a> {
    Pos(u64),
    Neg(u64),
    Float(f64),
    Str(&'a str),
    Bytes(&'a [u8]),
    Bool(bool),
    Null,
    Undefined,
    Composite(Cbor<'a>),
}
use CborValue::*;

impl<'a> CborValue<'a> {
    pub const fn with_tag(self, tag: u64) -> TaggedValue<'a> {
        Tagged(tag, self)
    }
    pub const fn without_tag(self) -> TaggedValue<'a> {
        Plain(self)
    }

    fn copied(&self) -> Self {
        match self {
            Pos(x) => Pos(*x),
            Neg(x) => Neg(*x),
            Float(x) => Float(*x),
            Str(s) => Str(*s),
            Bytes(b) => Bytes(*b),
            Bool(b) => Bool(*b),
            Null => Null,
            Undefined => Undefined,
            Composite(c) => Composite(Cbor::trusting(c.as_slice())),
        }
    }
}

/// Representation of a possibly tagged CBOR data item.
#[derive(Debug, PartialEq)]
pub enum TaggedValue<'a> {
    Plain(CborValue<'a>),
    Tagged(u64, CborValue<'a>),
}
use TaggedValue::*;

// TODO flesh out and extract data more thoroughly
impl<'a> TaggedValue<'a> {
    /// strip off wrappers of CBOR encoded item
    fn decoded(&self) -> Option<Self> {
        match self {
            Plain(p) => Some(Plain(p.copied())),
            Tagged(TAG_CBOR_ITEM, Bytes(b)) => tagged_value(*b)?.decoded(),
            Tagged(tag, p) => Some(Tagged(*tag, p.copied())),
        }
    }

    fn plain(&self) -> &CborValue<'a> {
        match self {
            Plain(p) => p,
            Tagged(_, p) => p,
        }
    }

    fn tag(&self) -> Option<u64> {
        match self {
            Plain(_) => None,
            Tagged(tag, _) => Some(*tag),
        }
    }

    /// Make a copy of the TaggedValue while still referencing the same bytes.
    pub fn copied(&self) -> Self {
        match self {
            Plain(p) => Plain(p.copied()),
            Tagged(tag, p) => Tagged(*tag, p.copied()),
        }
    }

    /// Try to interpret this value as a 64bit unsigned integer.
    ///
    /// Returns None if it is not an integer type or does not fit into 64 bits.
    pub fn as_u64(&self) -> Option<u64> {
        // TODO should also check for bigint
        match self.decoded()? {
            Plain(Pos(x)) => Some(x),
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
    pub fn as_str(&self) -> Option<Cow<str>> {
        let decoded = self.decoded()?;
        let tag = decoded.tag();
        match self.plain() {
            Str(s) => Some(Cow::Borrowed(*s)),
            Bytes(b) if tag != Some(TAG_BIGNUM_POS) && tag != Some(TAG_BIGNUM_NEG) => {
                std::str::from_utf8(b).ok().map(Cow::Borrowed)
            }
            _ => None,
        }
    }

    pub fn as_composite(&self) -> Option<Cbor<'a>> {
        match self.decoded()? {
            Tagged(_, Composite(c)) => Some(c),
            Plain(Composite(c)) => Some(c),
            _ => None,
        }
    }
}
