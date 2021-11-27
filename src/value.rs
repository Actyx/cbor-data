use self::{CborValue::*, Number::*};
use crate::{constants::*, Cbor, ItemKind, TaggedItem};
use std::{
    borrow::Cow,
    collections::{btree_map::Entry, BTreeMap},
    fmt::Debug,
};

/// Lifted navigation structure for a CborValue.
///
/// You can obtain this using [`CborValue::as_object()`](struct.CborValue.html#method.as_object).
#[derive(Debug, Clone, PartialEq)]
pub enum CborValue<'a> {
    Array(Vec<TaggedItem<'a>>),
    Dict(BTreeMap<&'a Cbor, TaggedItem<'a>>),
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
                let exponent = match exp.item() {
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
                            exponent,
                            mantissa,
                            inverted,
                        } if exponent == 0 => {
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
                ItemKind::Array(a) => Array(a.map(|i| i.tagged_item()).collect()),
                ItemKind::Dict(d) => Dict(d.fold(BTreeMap::new(), |mut acc, (k, v)| {
                    if let Entry::Vacant(e) = acc.entry(k) {
                        e.insert(v.tagged_item());
                    }
                    acc
                })),
            },
            _ => Unknown,
        }
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

#[cfg(test)]
mod tests {
    use crate::{CborBuilder, Encoder, Tags};

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

    // #[test]
    // fn base64string() {
    //     fn to_cbor(s: &str, tag: u64) -> CborOwned {
    //         let mut v = vec![0xd8u8, tag as u8, 0x60 | (s.len() as u8)];
    //         v.extend_from_slice(s.as_bytes());
    //         CborOwned::unchecked(v)
    //     }
    //     fn b(bytes: &CborOwned) -> Vec<u8> {
    //         bytes
    //             .value()
    //             .expect("a")
    //             .as_bytes()
    //             .expect("b")
    //             .into_owned()
    //     }

    //     let bytes = to_cbor("a346_-0=", TAG_BASE64URL);
    //     assert_eq!(b(&bytes), vec![107, 126, 58, 255, 237]);

    //     let bytes = to_cbor("a346_-0", TAG_BASE64URL);
    //     assert_eq!(b(&bytes), vec![107, 126, 58, 255, 237]);

    //     let bytes = to_cbor("a346/+0=", TAG_BASE64);
    //     assert_eq!(b(&bytes), vec![107, 126, 58, 255, 237]);

    //     let bytes = to_cbor("a346/+0", TAG_BASE64);
    //     assert_eq!(b(&bytes), vec![107, 126, 58, 255, 237]);
    // }

    #[test]
    fn tags() {
        let tags = Tags::fake(vec![1, 2, 3]);
        assert_eq!(tags.last(), Some(3));
        assert_eq!(tags.first(), Some(1));
        assert_eq!(tags.single(), None);

        let single = Tags::fake(vec![4]);
        assert_eq!(single.last(), Some(4));
        assert_eq!(single.first(), Some(4));
        assert_eq!(single.single(), Some(4));
    }
}
