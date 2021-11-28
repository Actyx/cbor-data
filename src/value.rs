use self::{CborValue::*, Number::*};
use crate::{constants::*, Cbor, CborOwned, ItemKind, TaggedItem};
use std::{
    borrow::Cow,
    collections::{btree_map::Entry, BTreeMap},
    fmt::Debug,
};

/// Lifted navigation structure for a CborValue.
///
/// You can obtain this using [`Cbor::decode()`](struct.Cbor.html#method.decode).
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
    Timestamp {
        /// timestamp value in seconds since the Unix epoch
        unix_epoch: i64,
        /// fractional part in nanoseconds, to be added
        nanos: u32,
        /// timezone to use when encoding as a string
        tz_sec_east: i32,
    },
    Str(Cow<'a, str>),
    Bytes(Cow<'a, [u8]>),
    /// Structural constraints for a tag are violated (like tag 0 on a number)
    Invalid,
    /// Unknown tags are present, you may want to manually interpret the TaggedItem
    Unknown,
}

macro_rules! arr {
    ($item:ident, $($i:ident),+) => {
        if let ItemKind::Array(mut a) = $item.kind() {
            if let ($(_,Some($i),)+ None) = ($(stringify!($i), a.next(),)+ a.next()) {
                ($($i),+)
            } else {
                return Invalid
            }
        } else {
            return Invalid
        }
    };
}

impl<'a> CborValue<'a> {
    pub fn new(item: TaggedItem<'a>) -> Self {
        match item.tags().single() {
            #[cfg(feature = "chrono")]
            Some(TAG_ISO8601) => {
                if let ItemKind::Str(s) = item.kind() {
                    chrono::DateTime::parse_from_rfc3339(s.as_cow().as_ref())
                        .map(|dt| Timestamp {
                            unix_epoch: dt.timestamp(),
                            nanos: dt.timestamp_subsec_nanos(),
                            tz_sec_east: dt.offset().local_minus_utc(),
                        })
                        .unwrap_or(Invalid)
                } else {
                    Invalid
                }
            }
            Some(TAG_EPOCH) => match item.kind() {
                ItemKind::Pos(t) => Timestamp {
                    unix_epoch: t.min(i64::MAX as u64) as i64,
                    nanos: 0,
                    tz_sec_east: 0,
                },
                ItemKind::Neg(t) => Timestamp {
                    unix_epoch: -1 - t.min(i64::MAX as u64) as i64,
                    nanos: 0,
                    tz_sec_east: 0,
                },
                ItemKind::Float(t) => Timestamp {
                    unix_epoch: t.min(i64::MAX as f64) as i64,
                    nanos: ((t - t.floor()) * 1e9) as u32,
                    tz_sec_east: 0,
                },
                _ => Invalid,
            },
            Some(t @ (TAG_BIGNUM_POS | TAG_BIGNUM_NEG)) => {
                if let ItemKind::Bytes(bytes) = item.kind() {
                    Number(Decimal {
                        exponent: 0,
                        mantissa: bytes.as_cow(),
                        inverted: t == TAG_BIGNUM_NEG,
                    })
                } else {
                    Invalid
                }
            }
            Some(t @ (TAG_BIGDECIMAL | TAG_BIGFLOAT)) => {
                let (exp, mant) = arr!(item, a, b);
                let exponent = match exp.kind() {
                    ItemKind::Pos(x) => i128::from(x),
                    ItemKind::Neg(x) => -1 - i128::from(x),
                    _ => return Invalid,
                };
                if let Number(n) = Self::new(mant.tagged_item()) {
                    match n {
                        Int(mut n) => {
                            let inverted = n < 0;
                            if inverted {
                                n = -1 - n;
                            }
                            let start = n.leading_zeros() as usize / 8;
                            let bytes = n.to_be_bytes();
                            if t == TAG_BIGDECIMAL {
                                Number(Decimal {
                                    exponent,
                                    mantissa: Cow::Owned(bytes[start..].to_vec()),
                                    inverted,
                                })
                            } else {
                                Number(Float {
                                    exponent,
                                    mantissa: Cow::Owned(bytes[start..].to_vec()),
                                    inverted,
                                })
                            }
                        }
                        Decimal {
                            exponent: e,
                            mantissa,
                            inverted,
                        } if e == 0 => {
                            if t == TAG_BIGDECIMAL {
                                Number(Decimal {
                                    exponent,
                                    mantissa,
                                    inverted,
                                })
                            } else {
                                Number(Float {
                                    exponent,
                                    mantissa,
                                    inverted,
                                })
                            }
                        }
                        _ => Invalid,
                    }
                } else {
                    Invalid
                }
            }
            Some(TAG_CBOR_ITEM) => {
                if let ItemKind::Bytes(b) = item.kind() {
                    if let Some(b) = b.as_slice() {
                        Cbor::unchecked(b).decode()
                    } else {
                        CborOwned::unchecked(b.to_vec()).decode().make_static()
                    }
                } else {
                    Invalid
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
                    b.map(|bytes| Bytes(Cow::Owned(bytes))).unwrap_or(Invalid)
                } else {
                    Invalid
                }
            }
            None => match item.kind() {
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
            },
            _ => Unknown,
        }
    }

    pub fn make_static(self) -> CborValue<'static> {
        match self {
            Array(a) => Array(a.into_iter().map(ms).collect()),
            Dict(d) => Dict(d.into_iter().map(|(k, v)| (ms(k), ms(v))).collect()),
            Undefined => Undefined,
            Null => Null,
            Bool(b) => Bool(b),
            Number(n) => Number(n.make_static()),
            Timestamp {
                unix_epoch,
                nanos,
                tz_sec_east,
            } => Timestamp {
                unix_epoch,
                nanos,
                tz_sec_east,
            },
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

/// Representation of a number extracted from a CBOR item
#[derive(Debug, Clone, PartialEq)]
pub enum Number<'a> {
    /// an integer number from major types 0 or 1
    Int(i128),
    /// a floating-point number from major type 7
    IEEE754(f64),
    /// a big integer or big decimal with value `mantissa * 10.pow(exponent)`
    Decimal {
        exponent: i128,
        mantissa: Cow<'a, [u8]>,
        /// if this is true, then the mantissa bytes represent `-1 - mantissa`
        inverted: bool,
    },
    /// a big integer or big decimal with value `mantissa * 2.pow(exponent)`
    Float {
        exponent: i128,
        mantissa: Cow<'a, [u8]>,
        /// if this is true, then the mantissa bytes represent `-1 - mantissa`
        inverted: bool,
    },
}

impl<'a> Number<'a> {
    fn make_static(self) -> Number<'static> {
        match self {
            Int(i) => Int(i),
            IEEE754(f) => IEEE754(f),
            Decimal {
                exponent,
                mantissa,
                inverted,
            } => Decimal {
                exponent,
                mantissa: ms(mantissa),
                inverted,
            },
            Float {
                exponent,
                mantissa,
                inverted,
            } => Float {
                exponent,
                mantissa: ms(mantissa),
                inverted,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CborBuilder, CborOwned, Encoder, Literal, Writer};

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
            CborValue::Timestamp {
                unix_epoch: 417176225,
                nanos: 345_000_000,
                tz_sec_east: 7200
            }
        );

        let cbor = CborBuilder::new().write_str("2183-03-22T12:17:05.345-03:00", [TAG_ISO8601]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp {
                unix_epoch: 6728627825,
                nanos: 345_000_000,
                tz_sec_east: -10800
            }
        );

        let cbor = CborBuilder::new().write_str("1833-03-22T02:17:05.345-13:00", [TAG_ISO8601]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp {
                unix_epoch: -4316316175,
                nanos: 345_000_000,
                tz_sec_east: -46800
            }
        );
    }

    #[test]
    fn epoch() {
        let cbor = CborBuilder::new().write_pos(1234567, [TAG_EPOCH]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp {
                unix_epoch: 1234567,
                nanos: 0,
                tz_sec_east: 0
            }
        );

        let cbor = CborBuilder::new().write_neg(1234566, [TAG_EPOCH]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp {
                unix_epoch: -1234567,
                nanos: 0,
                tz_sec_east: 0
            }
        );

        let cbor = CborBuilder::new()
            .write_lit(Literal::L8(12_345.900_000_014_5_f64.to_bits()), [TAG_EPOCH]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp {
                unix_epoch: 12345,
                nanos: 900_000_014,
                tz_sec_east: 0
            }
        );

        let cbor = CborBuilder::new()
            .write_lit(Literal::L8(12_345.900_000_015_5_f64.to_bits()), [TAG_EPOCH]);
        assert_eq!(
            cbor.decode(),
            CborValue::Timestamp {
                unix_epoch: 12345,
                nanos: 900_000_015,
                tz_sec_east: 0
            }
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
            Number(Float {
                exponent: -3,
                mantissa: [13_u8][..].into(),
                inverted: false
            })
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_neg(12, []);
        });
        assert_eq!(
            cbor.decode(),
            Number(Float {
                exponent: -3,
                mantissa: [12_u8][..].into(),
                inverted: true
            })
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_pos(0x010203, []);
        });
        assert_eq!(
            cbor.decode(),
            Number(Float {
                exponent: -3,
                mantissa: [1, 2, 3][..].into(),
                inverted: false
            })
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_bytes([1, 2, 3].as_ref(), [TAG_BIGNUM_POS]);
        });
        assert_eq!(
            cbor.decode(),
            Number(Float {
                exponent: -3,
                mantissa: [1, 2, 3][..].into(),
                inverted: false
            })
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_neg(0x010203, []);
        });
        assert_eq!(
            cbor.decode(),
            Number(Float {
                exponent: -3,
                mantissa: [1, 2, 3][..].into(),
                inverted: true
            })
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGFLOAT], |b| {
            b.write_neg(2, []);
            b.write_bytes([1, 2, 3].as_ref(), [TAG_BIGNUM_NEG]);
        });
        assert_eq!(
            cbor.decode(),
            Number(Float {
                exponent: -3,
                mantissa: [1, 2, 3][..].into(),
                inverted: true
            })
        );

        let cbor = CborBuilder::new().write_array([TAG_BIGDECIMAL], |b| {
            b.write_pos(2, []);
            b.write_pos(0xff01020304, []);
        });
        assert_eq!(
            cbor.decode(),
            Number(Decimal {
                exponent: 2,
                mantissa: [255, 1, 2, 3, 4][..].into(),
                inverted: false
            })
        );
    }
}
