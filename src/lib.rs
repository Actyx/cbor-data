//! A library for using CBOR as in-memory representation for working with dynamically shaped data.
//!
//! For the details on the data format see [RFC7049](https://tools.ietf.org/html/rfc7049). It is
//! normally meant to be used as a data interchange format that models a superset of the JSON
//! features while employing a more compact binary representation. As such, the data representation
//! is biased towards smaller in-memory size and not towards fastest data access speed.
//!
//! This library presents a range of tradeoffs when using this data format. You can just use the
//! bits you get from the wire or from a file, without paying any initial overhead but with the
//! possibility of panicking during access and having to allocate when extracting (byte) strings
//! in case indefinite size encoding was used. Or you can validate and canonicalise the bits before
//! using them, removing the possibility of pancis and guaranteeing that indexing into the data
//! will never allocate.
//!
//! Regarding performance you should keep in mind that arrays and dictionaries are encoded as flat
//! juxtaposition of its elements, meaning that indexing will have to decode items as it skips over
//! them.
//!
//! CBOR tags are faithfully reported (well, the innermost one, in case multiple are present — the
//! RFC is not perfectly clear here) but not interpreted at this point, meaning that a bignum will
//! come out as a binary string with a tag.

use std::{borrow::Cow, fmt::Debug};

mod builder;
mod constants;
mod reader;

pub use builder::{ArrayBuilder, CborBuilder, DictBuilder, WriteToArray, WriteToDict};
pub use reader::Literal;

/// Wrapper around some bytes (referenced or owned) that allows parsing as CBOR value.
///
/// For details on the format see [RFC7049](https://tools.ietf.org/html/rfc7049).
///
/// When interpreting CBOR messages from the outside (e.g. from the network) then it is
/// advisable to ingest those using the [`canonical`](#method.canonical) constructor.
/// In case the message was encoded for example using [`CborBuilder`](./struct.CborBuilder.html)
/// it is sufficient to use the [`trusting`](#method.trusting) constructor.
///
/// Canonicalisation rqeuires an intermediary data buffer, which can be supplied (and reused)
/// by the caller to save on allocations.
#[derive(Clone, PartialEq)]
pub struct Cbor<'a>(Cow<'a, [u8]>);

impl<'a> Debug for Cbor<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cbor({})",
            self.0
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}

/// Low-level decoded form of a CBOR item. Use TaggedValue for inspecting values.
///
/// Beware of the `Neg` variant, which carries the `-1 - x`.
///
/// The Owned variants are only generated when decoding indefinite size (byte) strings in order
/// to present a contiguous slice of memory. You will never see these if you used
/// [`Cbor::canonical()`](struct.Cbor#method.canonical).
#[derive(Clone, Debug, PartialEq)]
pub enum CborValue<'a> {
    Pos(u64),
    Neg(u64),
    Float(f64),
    Str(&'a str),
    StrOwned(String),
    Bytes(&'a [u8]),
    BytesOwned(Vec<u8>),
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

/// Representation of a possibly tagged CBOR data item.
#[derive(Clone, Debug, PartialEq)]
pub enum TaggedValue<'a> {
    Plain(CborValue<'a>),
    Tagged(u64, CborValue<'a>),
}
use constants::{MAJOR_ARRAY, MAJOR_DICT, MAJOR_TAG};
use reader::{canonicalise, integer, major, ptr, tagged_value};
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
            Plain(Str(s)) => Some(*s),
            Plain(Bytes(b)) => std::str::from_utf8(*b).ok(),
            _ => None,
        }
    }

    pub fn as_composite(&self) -> Option<&Cbor<'a>> {
        match self {
            Tagged(_, Composite(c)) => Some(c),
            Plain(Composite(c)) => Some(c),
            _ => None,
        }
    }
}

impl Cbor<'static> {
    /// Copy the bytes and wrap in Cbor for indexing.
    ///
    /// No checks on the integrity are made, indexing methods may panic if encoded lengths are out of bound.
    pub fn trusting(bytes: impl AsRef<[u8]>) -> Self {
        Self(Cow::Owned(bytes.as_ref().to_owned()))
    }

    /// Copy the bytes while checking for integrity and replacing indefinite (byte) strings with definite ones.
    ///
    /// This constructor will go through and decode the whole provided CBOR bytes and write them into a
    /// vector, thereby
    ///
    ///  - retaining only innermost tags
    ///  - writing arrays and dicts using indefinite size format
    ///  - writing numbers in their smallest form
    ///
    /// The used vector can be provided (to reuse previously allocated memory) or newly created. In the former
    /// case all contents of the provided argument will be cleared.
    pub fn canonical(bytes: impl AsRef<[u8]>, scratch_space: Option<&mut Vec<u8>>) -> Option<Self> {
        canonicalise(
            bytes.as_ref(),
            scratch_space
                .map(|v| CborBuilder::with_scratch_space(v))
                .unwrap_or_else(CborBuilder::new),
        )
    }
}

impl<'a> Cbor<'a> {
    /// Extract the single value represented by this piece of CBOR
    pub fn value(&self) -> Option<TaggedValue> {
        tagged_value(self.as_slice())
    }

    /// Extract a value by indexing into arrays and dicts, with path elements separated by dot.
    ///
    /// The empty string will yield the same as calling [`value()`](#method.value). If path elements
    /// may contain `.` then use [`index_iter()`](#method.index_iter).
    pub fn index(&self, path: &str) -> Option<TaggedValue> {
        ptr(self.as_slice(), path.split_terminator('.'))
    }

    /// Extract a value by indexing into arrays and dicts, with path elements yielded by an iterator.
    ///
    /// The empty iterator will yield the same as calling [`value()`](#method.value).
    pub fn index_iter<'b>(&self, path: impl Iterator<Item = &'b str>) -> Option<TaggedValue> {
        ptr(self.as_slice(), path)
    }

    /// Check if this CBOR contains an array as its top-level item.
    /// Returns false also in case of data format problems.
    pub fn is_array(&self) -> bool {
        let mut bytes = self.as_slice();
        while major(bytes) == Some(MAJOR_TAG) {
            bytes = match integer(bytes) {
                Some((_, r)) => r,
                None => return false,
            };
        }
        major(bytes) == Some(MAJOR_ARRAY)
    }

    /// Check if this CBOR contains an dict as its top-level item.
    /// Returns false also in case of data format problems.
    pub fn is_dict(&self) -> bool {
        let mut bytes = self.as_slice();
        while major(bytes) == Some(MAJOR_TAG) {
            bytes = match integer(bytes) {
                Some((_, r)) => r,
                None => return false,
            };
        }
        major(bytes) == Some(MAJOR_DICT)
    }

    fn borrow(bytes: &'a [u8]) -> Self {
        Self(Cow::Borrowed(bytes))
    }

    /// A view onto the underlying bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        builder::{WriteToArray, WriteToDict},
        constants::*,
    };

    #[test]
    fn roundtrip_simple() {
        let pos = CborBuilder::new().write_pos(42, Some(56));
        assert_eq!(pos.value(), Some(Tagged(56, Pos(42))));

        let neg = CborBuilder::new().write_neg(42, Some(56));
        assert_eq!(neg.value(), Some(Tagged(56, Neg(42))));

        let bool = CborBuilder::new().write_bool(true, None);
        assert_eq!(bool.value(), Some(Plain(Bool(true))));

        let null = CborBuilder::new().write_null(Some(314));
        assert_eq!(null.value(), Some(Tagged(314, Null)));

        let string = CborBuilder::new().write_str("huhu", Some(TAG_CBOR_MARKER));
        assert_eq!(string.value(), Some(Tagged(55799, Str("huhu"))));

        let bytes = CborBuilder::new().write_bytes(b"abcd", None);
        assert_eq!(bytes.value(), Some(Plain(Bytes(b"abcd"))));
    }

    #[test]
    fn roundtrip_complex() {
        let mut array = CborBuilder::new().write_array(Some(TAG_FRACTION));
        array.write_pos(5, None);

        let mut dict = array.write_dict(None);
        dict.write_neg("a", 666, None);
        dict.write_bytes("b", b"defdef", None);
        let array = dict.finish();

        let mut array2 = array.write_array(None);
        array2.write_bool(false, None);
        array2.write_str("hello", None);
        let mut array = array2.finish();

        array.write_null(Some(12345));

        let complex = array.finish();

        let mut dict = CborBuilder::new().write_dict(None);
        dict.write_neg("a", 666, None);
        dict.write_bytes("b", b"defdef", None);
        let the_dict = dict.finish();

        let mut array = CborBuilder::new().write_array(None);
        array.write_bool(false, None);
        array.write_str("hello", None);
        let the_array = array.finish();

        let value = complex.value().unwrap().as_composite().unwrap().clone();
        assert_eq!(
            complex.index(""),
            Some(Tagged(TAG_FRACTION, Composite(value)))
        );
        assert_eq!(complex.index("a"), None);
        assert_eq!(complex.index("0"), Some(Plain(Pos(5))));
        assert_eq!(complex.index("1"), Some(Plain(Composite(the_dict))));
        assert_eq!(complex.index("1.a"), Some(Plain(Neg(666))));
        assert_eq!(complex.index("1.b"), Some(Plain(Bytes(b"defdef"))));
        assert_eq!(complex.index("2"), Some(Plain(Composite(the_array))));
        assert_eq!(complex.index("2.0"), Some(Plain(Bool(false))));
        assert_eq!(complex.index("2.1"), Some(Plain(Str("hello"))));
        assert_eq!(complex.index("3"), Some(Tagged(12345, Null)));
    }

    #[test]
    fn canonical() {
        let bytes = vec![
            0xc4u8, 0x84, 5, 0xa2, 0x61, b'a', 0x39, 2, 154, 0x61, b'b', 0x46, b'd', b'e', b'f',
            b'd', b'e', b'f', 0x82, 0xf4, 0x65, b'h', b'e', b'l', b'l', b'o', 0xd9, 48, 57, 0xf6,
        ];
        let complex = Cbor::canonical(&*bytes, None).unwrap();
        let the_dict = Cbor::canonical(&bytes[3..18], None).unwrap();
        let the_array = Cbor::canonical(&bytes[18..26], None).unwrap();

        assert_eq!(complex.index("a"), None);
        assert_eq!(complex.index("0"), Some(Plain(Pos(5))));
        assert_eq!(complex.index("1"), Some(Plain(Composite(the_dict))));
        assert_eq!(complex.index("1.a"), Some(Plain(Neg(666))));
        assert_eq!(complex.index("1.b"), Some(Plain(Bytes(b"defdef"))));
        assert_eq!(complex.index("2"), Some(Plain(Composite(the_array))));
        assert_eq!(complex.index("2.0"), Some(Plain(Bool(false))));
        assert_eq!(complex.index("2.1"), Some(Plain(Str("hello"))));
        assert_eq!(complex.index("3"), Some(Tagged(12345, Null)));
    }
}
