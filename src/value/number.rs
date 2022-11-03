use self::Number::*;
use crate::{constants::*, Encoder, ItemKind, TaggedItem, Writer};
use std::{borrow::Cow, marker::PhantomData};

/// Representation of a number extracted from a CBOR item
#[derive(Debug, Clone, PartialEq)]
pub enum Number<'a> {
    /// an integer number from major types 0 or 1
    Int(i128),
    /// a floating-point number from major type 7
    IEEE754(f64),
    /// a big integer or big decimal with value `mantissa * 10.pow(exponent)`
    Decimal(Exponential<'a, Ten>),
    /// a big integer or big decimal with value `mantissa * 2.pow(exponent)`
    Float(Exponential<'a, Two>),
}

impl<'a> Number<'a> {
    /// Interpret the given item as per [RFC8949 §3.4.4](https://www.rfc-editor.org/rfc/rfc8949.html#section-3.4.4)
    pub(crate) fn from_bignum(item: TaggedItem<'a>) -> Option<Self> {
        let tag = item.tags().single()?;
        let (exp, mant) = if let ItemKind::Array(mut a) = item.kind() {
            if let (Some(e), Some(m), None) = (a.next(), a.next(), a.next()) {
                (e, m)
            } else {
                return None;
            }
        } else {
            return None;
        };
        let exponent = match exp.kind() {
            ItemKind::Pos(x) => i128::from(x),
            ItemKind::Neg(x) => -1 - i128::from(x),
            _ => return None,
        };
        if !matches!(
            mant.kind(),
            ItemKind::Pos(_) | ItemKind::Neg(_) | ItemKind::Bytes(_)
        ) {
            // This check ensures that Decimal(e) below has exponent == 0
            return None;
        }
        if let super::CborValue::Number(n) = mant.decode() {
            match n {
                Int(mut n) => {
                    let inverted = n < 0;
                    if inverted {
                        n = -1 - n;
                    }
                    let start = n.leading_zeros() as usize / 8;
                    let bytes = n.to_be_bytes();
                    if tag == TAG_BIGDECIMAL {
                        Some(Decimal(Exponential {
                            exponent,
                            mantissa: Cow::Owned(bytes[start..].to_vec()),
                            inverted,
                            _ph: PhantomData,
                        }))
                    } else {
                        Some(Float(Exponential {
                            exponent,
                            mantissa: Cow::Owned(bytes[start..].to_vec()),
                            inverted,
                            _ph: PhantomData,
                        }))
                    }
                }
                Decimal(e) => {
                    if tag == TAG_BIGDECIMAL {
                        Some(Decimal(e.with_exponent(exponent)))
                    } else {
                        Some(Float(e.with_exponent(exponent)))
                    }
                }
                _ => None,
            }
        } else {
            None
        }
    }

    pub(crate) fn encode<E: Encoder>(&self, encoder: E) -> E::Output {
        match self {
            Number::Int(i) => {
                let i = *i;
                if i >= 0 {
                    if i <= (u64::MAX as i128) {
                        encoder.write_pos(i as u64, None)
                    } else {
                        let bytes = i.to_be_bytes();
                        let first = bytes
                            .iter()
                            .enumerate()
                            .find_map(|(idx, byte)| if *byte != 0 { Some(idx) } else { None })
                            .unwrap();
                        encoder.write_bytes(&bytes[first..], [TAG_BIGNUM_POS])
                    }
                } else if i >= -(u64::MAX as i128 + 1) {
                    encoder.write_neg((-1 - i) as u64, None)
                } else {
                    let bytes = (-1 - i).to_be_bytes();
                    let first = bytes
                        .iter()
                        .enumerate()
                        .find_map(|(idx, byte)| if *byte != 0 { Some(idx) } else { None })
                        .unwrap();
                    encoder.write_bytes(&bytes[first..], [TAG_BIGNUM_NEG])
                }
            }
            Number::IEEE754(f) => encoder.encode_f64(*f),
            Number::Decimal(d) => encode_big(d, encoder),
            Number::Float(d) => encode_big(d, encoder),
        }
    }

    /// Cut ties with possibly referenced byte slices, allocating if necessary
    pub fn make_static(self) -> Number<'static> {
        match self {
            Int(i) => Int(i),
            IEEE754(f) => IEEE754(f),
            Decimal(e) => Decimal(e.make_static()),
            Float(e) => Float(e.make_static()),
        }
    }

    pub fn get_type(&self) -> &'static str {
        match self {
            Int(_) => "small integer",
            IEEE754(_) => "small float",
            Decimal(_) => "big decimal",
            Float(_) => "big float",
        }
    }
}

/// A representation of a bignum
///
/// The base is statically known while the exponent is dynamic. The mantissa is not guaranteed
/// to have optimal encoding, i.e. it can have leading zero bytes.
///
/// The represented value is `m * base.pow(exponent)`, where
///
///  - `m = mantissa` for `inverted == false`, and
///  - `m = -1 - mantissa` for `inverted = true`.
#[derive(Debug, Clone, PartialEq, Hash)]
pub struct Exponential<'a, B: Base> {
    exponent: i128,
    /// mantissa in big-endian format
    mantissa: Cow<'a, [u8]>,
    /// if this is true, then the mantissa bytes represent `-1 - mantissa`
    inverted: bool,
    _ph: PhantomData<B>,
}

impl<'a> Exponential<'a, Ten> {
    /// Interpret as a bignum assuming this is an appropriately tagged byte string
    pub(crate) fn from_bytes(item: TaggedItem<'a>) -> Option<Self> {
        let tag = item.tags().single()?;
        if let ItemKind::Bytes(bytes) = item.kind() {
            Some(Exponential {
                exponent: 0,
                mantissa: bytes.as_cow(),
                inverted: tag == TAG_BIGNUM_NEG,
                _ph: PhantomData,
            })
        } else {
            None
        }
    }

    fn with_exponent<BB: Base>(self, exponent: i128) -> Exponential<'a, BB> {
        Exponential {
            exponent,
            mantissa: self.mantissa,
            inverted: self.inverted,
            _ph: PhantomData,
        }
    }
}

impl<'a, B: Base> Exponential<'a, B> {
    pub fn new(exponent: i128, mantissa: Cow<'a, [u8]>, inverted: bool) -> Self {
        Self {
            exponent,
            mantissa,
            inverted,
            _ph: PhantomData,
        }
    }

    /// Cut ties with possibly referenced byte slices, allocating if necessary
    pub fn make_static(self) -> Exponential<'static, B> {
        Exponential {
            exponent: self.exponent,
            mantissa: super::ms(self.mantissa),
            inverted: self.inverted,
            _ph: PhantomData,
        }
    }

    /// Get a reference to the exponential's exponent.
    pub fn exponent(&self) -> i128 {
        self.exponent
    }

    /// Get a reference to the exponential's mantissa.
    pub fn mantissa(&self) -> &[u8] {
        self.mantissa.as_ref()
    }

    /// Get a reference to the exponential's inverted.
    pub fn inverted(&self) -> bool {
        self.inverted
    }
}

pub trait Base {
    const BASE: u64;
    const TAG: u64;
}
#[derive(Debug, Clone, PartialEq)]
pub struct Two;
impl Base for Two {
    const BASE: u64 = 2;
    const TAG: u64 = TAG_BIGFLOAT;
}
#[derive(Debug, Clone, PartialEq)]
pub struct Ten;
impl Base for Ten {
    const BASE: u64 = 10;
    const TAG: u64 = TAG_BIGDECIMAL;
}

fn encode_big<B: Base, E: Encoder>(d: &Exponential<B>, encoder: E) -> E::Output {
    let first = d
        .mantissa()
        .iter()
        .enumerate()
        .find_map(|(idx, byte)| if *byte != 0 { Some(idx) } else { None })
        .unwrap_or_else(|| d.mantissa().len());
    let bytes = &d.mantissa()[first..];
    if bytes.len() <= 8 {
        let mut be_bytes = [0u8; 8];
        be_bytes[8 - bytes.len()..].copy_from_slice(bytes);
        let num = u64::from_be_bytes(be_bytes);
        if d.exponent() == 0 {
            if d.inverted() {
                encoder.write_neg(num, None)
            } else {
                encoder.write_pos(num, None)
            }
        } else {
            encoder.write_array([B::TAG], |builder| {
                let exp = d.exponent();
                if exp >= 0 {
                    builder.write_pos(exp as u64, None);
                } else {
                    builder.write_neg((-1 - exp) as u64, None);
                }
                if d.inverted() {
                    builder.write_neg(num, None);
                } else {
                    builder.write_pos(num, None);
                }
            })
        }
    } else if d.exponent() == 0 {
        if d.inverted() {
            encoder.write_bytes(bytes, [TAG_BIGNUM_NEG])
        } else {
            encoder.write_bytes(bytes, [TAG_BIGNUM_POS])
        }
    } else {
        encoder.write_array([B::TAG], |builder| {
            let exp = d.exponent();
            if exp >= 0 {
                builder.write_pos(exp as u64, None);
            } else {
                builder.write_neg((-1 - exp) as u64, None);
            }
            if d.inverted() {
                builder.write_bytes(bytes, [TAG_BIGNUM_NEG]);
            } else {
                builder.write_bytes(bytes, [TAG_BIGNUM_POS]);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tests::hex, CborBuilder};

    #[test]
    fn encode() {
        fn e(n: Number) -> String {
            CborBuilder::new().encode_number(&n).to_string()
        }
        fn d(exp: i128, mant: &str, inv: bool) -> String {
            e(Decimal(Exponential::new(exp, hex(mant).into(), inv)))
        }
        fn f(exp: i128, mant: &str, inv: bool) -> String {
            e(Float(Exponential::new(exp, hex(mant).into(), inv)))
        }

        assert_eq!(e(Int(0)), "0");
        assert_eq!(e(Int(1)), "1");
        assert_eq!(e(Int(-1)), "-1");
        assert_eq!(e(Int(u64::MAX.into())), "18446744073709551615");
        assert_eq!(e(Int(-1 - u64::MAX as i128)), "-18446744073709551616");
        assert_eq!(e(Int(u64::MAX as i128 + 1)), "2(h'010000000000000000')");
        assert_eq!(e(Int(-2 - u64::MAX as i128)), "3(h'010000000000000000')");

        assert_eq!(e(IEEE754(-0.0)), "-0.0");
        assert_eq!(e(IEEE754(1.3e34)), "1.3e34");

        assert_eq!(d(0, "", false), "0");
        assert_eq!(d(0, "", true), "-1");
        assert_eq!(d(0, "01", false), "1");
        assert_eq!(d(0, "01", true), "-2");
        assert_eq!(d(0, "ffffffffffffffff", false), "18446744073709551615");
        assert_eq!(d(0, "ffffffffffffffff", true), "-18446744073709551616");
        assert_eq!(
            d(0, "010203040506070809", false),
            "2(h'010203040506070809')"
        );
        assert_eq!(d(0, "010203040506070809", true), "3(h'010203040506070809')");
        assert_eq!(d(1, "", false), "4([1, 0])");
        assert_eq!(d(1, "", true), "4([1, -1])");
        assert_eq!(d(1, "01", false), "4([1, 1])");
        assert_eq!(d(1, "01", true), "4([1, -2])");
        assert_eq!(
            d(1, "ffffffffffffffff", false),
            "4([1, 18446744073709551615])"
        );
        assert_eq!(
            d(1, "ffffffffffffffff", true),
            "4([1, -18446744073709551616])"
        );
        assert_eq!(
            d(1, "010203040506070809", false),
            "4([1, 2(h'010203040506070809')])"
        );
        assert_eq!(
            d(1, "010203040506070809", true),
            "4([1, 3(h'010203040506070809')])"
        );
        assert_eq!(d(-1, "", false), "4([-1, 0])");
        assert_eq!(d(-1, "", true), "4([-1, -1])");
        assert_eq!(d(-1, "01", false), "4([-1, 1])");
        assert_eq!(d(-1, "01", true), "4([-1, -2])");
        assert_eq!(
            d(-1, "ffffffffffffffff", false),
            "4([-1, 18446744073709551615])"
        );
        assert_eq!(
            d(-1, "ffffffffffffffff", true),
            "4([-1, -18446744073709551616])"
        );
        assert_eq!(
            d(-1, "010203040506070809", false),
            "4([-1, 2(h'010203040506070809')])"
        );
        assert_eq!(
            d(-1, "010203040506070809", true),
            "4([-1, 3(h'010203040506070809')])"
        );

        assert_eq!(f(0, "", false), "0");
        assert_eq!(f(0, "", true), "-1");
        assert_eq!(f(0, "01", false), "1");
        assert_eq!(f(0, "01", true), "-2");
        assert_eq!(f(0, "ffffffffffffffff", false), "18446744073709551615");
        assert_eq!(f(0, "ffffffffffffffff", true), "-18446744073709551616");
        assert_eq!(
            f(0, "010203040506070809", false),
            "2(h'010203040506070809')"
        );
        assert_eq!(f(0, "010203040506070809", true), "3(h'010203040506070809')");
        assert_eq!(f(1, "", false), "5([1, 0])");
        assert_eq!(f(1, "", true), "5([1, -1])");
        assert_eq!(f(1, "01", false), "5([1, 1])");
        assert_eq!(f(1, "01", true), "5([1, -2])");
        assert_eq!(
            f(1, "ffffffffffffffff", false),
            "5([1, 18446744073709551615])"
        );
        assert_eq!(
            f(1, "ffffffffffffffff", true),
            "5([1, -18446744073709551616])"
        );
        assert_eq!(
            f(1, "010203040506070809", false),
            "5([1, 2(h'010203040506070809')])"
        );
        assert_eq!(
            f(1, "010203040506070809", true),
            "5([1, 3(h'010203040506070809')])"
        );
        assert_eq!(f(-1, "", false), "5([-1, 0])");
        assert_eq!(f(-1, "", true), "5([-1, -1])");
        assert_eq!(f(-1, "01", false), "5([-1, 1])");
        assert_eq!(f(-1, "01", true), "5([-1, -2])");
        assert_eq!(
            f(-1, "ffffffffffffffff", false),
            "5([-1, 18446744073709551615])"
        );
        assert_eq!(
            f(-1, "ffffffffffffffff", true),
            "5([-1, -18446744073709551616])"
        );
        assert_eq!(
            f(-1, "010203040506070809", false),
            "5([-1, 2(h'010203040506070809')])"
        );
        assert_eq!(
            f(-1, "010203040506070809", true),
            "5([-1, 3(h'010203040506070809')])"
        );
    }
}
