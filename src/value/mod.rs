use self::{CborValue::*, Number::*};
use crate::{constants::*, Cbor, CborOwned, ItemKind, TaggedItem};
use std::{
    borrow::Cow,
    collections::{btree_map::Entry, BTreeMap},
    fmt::Debug,
};

mod number;
mod timestamp;

pub use number::{Exponential, Number};
pub use timestamp::{Precision, Timestamp};

/// Lifted navigation structure for a CborValue.
///
/// You can obtain this using [`Cbor::decode()`](../struct.Cbor.html#method.decode).
/// This will reference existing bytes as much as possible, but in some cases it
/// has to allocate, e.g. when some needed slices are not contiguous in the underlying
/// `Cbor`.
#[derive(Debug, Clone, PartialEq)]
pub enum CborValue<'a> {
    Array(Vec<Cow<'a, Cbor>>),
    Dict(BTreeMap<Cow<'a, Cbor>, Cow<'a, Cbor>>),
    Undefined,
    Null,
    Bool(bool),
    Number(Number<'a>),
    Timestamp(Timestamp),
    Str(Cow<'a, str>),
    Bytes(Cow<'a, [u8]>),
    /// Structural constraints for a tag are violated (like tag 0 on a number)
    Invalid,
    /// Unknown tags are present, you may want to manually interpret the TaggedItem
    Unknown,
}

impl<'a> CborValue<'a> {
    pub fn new(item: TaggedItem<'a>) -> Self {
        Self::from_item(item).unwrap_or(Invalid)
    }

    fn from_item(item: TaggedItem<'a>) -> Option<Self> {
        match item.tags().single() {
            #[cfg(feature = "rfc3339")]
            Some(TAG_ISO8601) => Timestamp::from_string(item).map(Timestamp),
            Some(TAG_EPOCH) => Timestamp::from_epoch(item).map(Timestamp),
            Some(TAG_BIGNUM_POS | TAG_BIGNUM_NEG) => {
                Some(Number(Decimal(Exponential::from_bytes(item)?)))
            }
            Some(TAG_BIGDECIMAL | TAG_BIGFLOAT) => Number::from_bignum(item).map(CborValue::Number),
            Some(TAG_CBOR_ITEM) => {
                if let ItemKind::Bytes(b) = item.kind() {
                    if let Some(b) = b.as_slice() {
                        Some(Cbor::unchecked(b).decode())
                    } else {
                        Some(CborOwned::unchecked(b.to_vec()).decode().make_static())
                    }
                } else {
                    None
                }
            }
            Some(t @ (TAG_BASE64 | TAG_BASE64URL)) => {
                if let ItemKind::Str(s) = item.kind() {
                    let s = s.as_cow();
                    let b = if t == TAG_BASE64 {
                        base64::decode(s.as_bytes())
                    } else {
                        base64::decode_config(s.as_bytes(), base64::URL_SAFE_NO_PAD)
                    };
                    b.map(|bytes| Bytes(Cow::Owned(bytes))).ok()
                } else {
                    None
                }
            }
            None => Some(match item.kind() {
                ItemKind::Pos(x) => Number(Int(x.into())),
                ItemKind::Neg(x) => Number(Int(-1_i128 - i128::from(x))),
                ItemKind::Float(f) => Number(IEEE754(f)),
                ItemKind::Str(s) => Str(s.as_cow()),
                ItemKind::Bytes(b) => Bytes(b.as_cow()),
                ItemKind::Bool(b) => Bool(b),
                ItemKind::Null => Null,
                ItemKind::Undefined => Undefined,
                ItemKind::Simple(_) => Unknown,
                ItemKind::Array(a) => Array(a.map(Cow::Borrowed).collect()),
                ItemKind::Dict(d) => Dict(d.fold(BTreeMap::new(), |mut acc, (k, v)| {
                    if let Entry::Vacant(e) = acc.entry(Cow::Borrowed(k)) {
                        e.insert(Cow::Borrowed(v));
                    }
                    acc
                })),
            }),
            _ => Some(Unknown),
        }
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, Undefined)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Null)
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Unknown)
    }

    pub fn is_invalid(&self) -> bool {
        matches!(self, Invalid)
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Bool(b) = self {
            Some(*b)
        } else {
            None
        }
    }

    pub fn as_number(&self) -> Option<&Number> {
        if let Number(n) = self {
            Some(n)
        } else {
            None
        }
    }

    pub fn to_number(self) -> Option<Number<'a>> {
        if let Number(n) = self {
            Some(n)
        } else {
            None
        }
    }

    pub fn as_timestamp(&self) -> Option<Timestamp> {
        if let Timestamp(t) = self {
            Some(*t)
        } else {
            None
        }
    }

    pub fn as_str(&self) -> Option<&Cow<str>> {
        if let Str(s) = self {
            Some(s)
        } else {
            None
        }
    }

    pub fn to_str(self) -> Option<Cow<'a, str>> {
        if let Str(s) = self {
            Some(s)
        } else {
            None
        }
    }

    pub fn as_bytes(&self) -> Option<&Cow<[u8]>> {
        if let Bytes(b) = self {
            Some(b)
        } else {
            None
        }
    }

    pub fn to_bytes(self) -> Option<Cow<'a, [u8]>> {
        if let Bytes(b) = self {
            Some(b)
        } else {
            None
        }
    }

    /// Cut all ties to the underlying byte slice, which often implies allocations
    pub fn make_static(self) -> CborValue<'static> {
        match self {
            Array(a) => Array(a.into_iter().map(ms).collect()),
            Dict(d) => Dict(d.into_iter().map(|(k, v)| (ms(k), ms(v))).collect()),
            Undefined => Undefined,
            Null => Null,
            Bool(b) => Bool(b),
            Number(n) => Number(n.make_static()),
            Timestamp(t) => Timestamp(t),
            Str(s) => Str(ms(s)),
            Bytes(b) => Bytes(ms(b)),
            Invalid => Invalid,
            Unknown => Unknown,
        }
    }
}

fn ms<'a, T: ToOwned + ?Sized + 'a>(c: Cow<'a, T>) -> Cow<'static, T> {
    match c {
        Cow::Borrowed(b) => Cow::Owned(b.to_owned()),
        Cow::Owned(o) => Cow::Owned(o),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        constants::*,
        value::{number::Exponential, Number, Timestamp},
        CborBuilder, CborOwned, CborValue, Encoder, Literal, Writer,
    };

    #[test]
    fn display() {
        fn to_cbor_str(f: f64) -> String {
            format!("{}", CborBuilder::new().encode_f64(f))
        }
        assert_eq!(to_cbor_str(1.0), "1.0");
        assert_eq!(to_cbor_str(-1.1), "-1.1");
        assert_eq!(to_cbor_str(0.0), "0.0");
        assert_eq!(to_cbor_str(-0.0), "-0.0");
    }

    #[test]
    fn base64string() {
        fn to_cbor(s: &str, tag: u64) -> CborOwned {
            let mut v = vec![0xd8u8, tag as u8, 0x60 | (s.len() as u8)];
            v.extend_from_slice(s.as_bytes());
            CborOwned::unchecked(v)
        }
        fn b(bytes: &CborOwned) -> Vec<u8> {
            if let CborValue::Bytes(bytes) = bytes.decode() {
                bytes.into_owned()
            } else {
                panic!("no bytes: {}", bytes)
            }
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

    #[test]
    fn tags() {
        let cbor = CborBuilder::new().write_null([1, 2, 3]);
        assert_eq!(cbor.tags().last(), Some(3));
        assert_eq!(cbor.tags().first(), Some(1));
        assert_eq!(cbor.tags().single(), None);

        let cbor = CborBuilder::new().write_null([4]);
        assert_eq!(cbor.tags().last(), Some(4));
        assert_eq!(cbor.tags().first(), Some(4));
        assert_eq!(cbor.tags().single(), Some(4));
    }

    #[test]
    #[cfg(feature = "rfc3339")]
    fn rfc3339() {
        let cbor = CborBuilder::new().write_str("1983-03-22T12:17:05.345+02:00", [TAG_ISO8601]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp(Timestamp::new(417176225, 345_000_000, 7200))
        );

        let cbor = CborBuilder::new().write_str("2183-03-22T12:17:05.345-03:00", [TAG_ISO8601]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp(Timestamp::new(6728627825, 345_000_000, -10800))
        );

        let cbor = CborBuilder::new().write_str("1833-03-22T02:17:05.345-13:00", [TAG_ISO8601]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp(Timestamp::new(-4316316175, 345_000_000, -46800))
        );
    }

    #[test]
    fn epoch() {
        let cbor = CborBuilder::new().write_pos(1234567, [TAG_EPOCH]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp(Timestamp::new(1234567, 0, 0))
        );

        let cbor = CborBuilder::new().write_neg(1234566, [TAG_EPOCH]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp(Timestamp::new(-1234567, 0, 0))
        );

        let cbor = CborBuilder::new()
            .write_lit(Literal::L8(2_345.900_000_014_5_f64.to_bits()), [TAG_EPOCH]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp(Timestamp::new(2345, 900_000_015, 0))
        );

        let cbor = CborBuilder::new()
            .write_lit(Literal::L8(2_345.900_000_015_5_f64.to_bits()), [TAG_EPOCH]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp(Timestamp::new(2345, 900_000_016, 0))
        );
    }

    #[test]
    fn bignum() {
        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_pos(13, []);
        });
        assert_eq!(
            cbor.decode(),
            CborValue::Number(Number::Float(Exponential::new(
                -3,
                [13_u8][..].into(),
                false,
            )))
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_neg(12, []);
        });
        assert_eq!(
            cbor.decode(),
            CborValue::Number(Number::Float(Exponential::new(
                -3,
                [12_u8][..].into(),
                true,
            )))
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_pos(0x010203, []);
        });
        assert_eq!(
            cbor.decode(),
            CborValue::Number(Number::Float(Exponential::new(
                -3,
                [1, 2, 3][..].into(),
                false,
            )))
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_bytes([1, 2, 3].as_ref(), [TAG_BIGNUM_POS]);
        });
        assert_eq!(
            cbor.decode(),
            CborValue::Number(Number::Float(Exponential::new(
                -3,
                [1, 2, 3][..].into(),
                false,
            )))
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_neg(0x010203, []);
        });
        assert_eq!(
            cbor.decode(),
            CborValue::Number(Number::Float(Exponential::new(
                -3,
                [1, 2, 3][..].into(),
                true,
            )))
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_bytes([1, 2, 3].as_ref(), [TAG_BIGNUM_NEG]);
        });
        assert_eq!(
            cbor.decode(),
            CborValue::Number(Number::Float(Exponential::new(
                -3,
                [1, 2, 3][..].into(),
                true,
            )))
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGDECIMAL], |b| {
            b.write_pos(2, []);
            b.write_pos(0xff01020304, []);
        });
        assert_eq!(
            cbor.decode(),
            CborValue::Number(Number::Decimal(Exponential::new(
                2,
                [255, 1, 2, 3, 4][..].into(),
                false,
            )))
        );
    }
}
