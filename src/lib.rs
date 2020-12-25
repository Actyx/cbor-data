use std::{borrow::Cow, fmt::Debug};

mod builder;
mod constants;
mod reader;

pub use builder::{ArrayBuilder, CborBuilder, DictBuilder};

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
    pub fn new(bytes: impl AsRef<[u8]>) -> Self {
        Self(Cow::Owned(bytes.as_ref().to_owned()))
    }

    /// Copy the bytes while checking for integrity and replacing indefinite (byte) strings with definite ones.
    pub fn canonical(bytes: impl AsRef<[u8]>) -> Option<Self> {
        canonicalise(bytes.as_ref(), CborBuilder::default())
    }
}

impl<'a> Cbor<'a> {
    pub fn value(&self) -> Option<TaggedValue> {
        tagged_value(self.b())
    }

    pub fn index(&self, path: &str) -> Option<TaggedValue> {
        ptr(self.b(), path.split_terminator('.'))
    }

    pub fn index_iter<'b>(&self, path: impl Iterator<Item = &'b str>) -> Option<TaggedValue> {
        ptr(self.b(), path)
    }

    pub fn is_array(&self) -> bool {
        let mut bytes = self.b();
        while major(bytes) == MAJOR_TAG {
            bytes = match integer(bytes) {
                Some((_, r)) => r,
                None => return false,
            };
        }
        major(bytes) == MAJOR_ARRAY
    }

    pub fn is_dict(&self) -> bool {
        let mut bytes = self.b();
        while major(bytes) == MAJOR_TAG {
            bytes = match integer(bytes) {
                Some((_, r)) => r,
                None => return false,
            };
        }
        major(bytes) == MAJOR_DICT
    }

    fn borrow(bytes: &'a [u8]) -> Self {
        Self(Cow::Borrowed(bytes))
    }

    fn b(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::*;

    #[test]
    fn roundtrip_simple() {
        let pos = CborBuilder::default().write_pos(42, Some(56));
        assert_eq!(pos.value(), Some(Tagged(56, Pos(42))));

        let neg = CborBuilder::default().write_neg(42, Some(56));
        assert_eq!(neg.value(), Some(Tagged(56, Neg(42))));

        let bool = CborBuilder::default().write_bool(true, None);
        assert_eq!(bool.value(), Some(Plain(Bool(true))));

        let null = CborBuilder::default().write_null(Some(314));
        assert_eq!(null.value(), Some(Tagged(314, Null)));

        let string = CborBuilder::default().write_str("huhu", Some(TAG_CBOR_MARKER));
        assert_eq!(string.value(), Some(Tagged(55799, Str("huhu"))));

        let bytes = CborBuilder::default().write_bytes(b"abcd", None);
        assert_eq!(bytes.value(), Some(Plain(Bytes(b"abcd"))));
    }

    #[test]
    fn roundtrip_complex() {
        let mut array = CborBuilder::default().write_array(Some(TAG_FRACTION));
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

        let mut dict = CborBuilder::default().write_dict(None);
        dict.write_neg("a", 666, None);
        dict.write_bytes("b", b"defdef", None);
        let the_dict = dict.finish();

        let mut array = CborBuilder::default().write_array(None);
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
}
