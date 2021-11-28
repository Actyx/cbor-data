use self::Number::*;
use crate::{constants::*, ItemKind, TaggedItem};
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
    pub fn from_bignum(item: TaggedItem<'a>) -> Option<Self> {
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

    pub fn make_static(self) -> Number<'static> {
        match self {
            Int(i) => Int(i),
            IEEE754(f) => IEEE754(f),
            Decimal(e) => Decimal(e.make_static()),
            Float(e) => Float(e.make_static()),
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
    pub fn from_bytes(item: TaggedItem<'a>) -> Option<Self> {
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

impl<'a, B: Base> Exponential<'a, B> {
    pub fn new(exponent: i128, mantissa: Cow<'a, [u8]>, inverted: bool) -> Self {
        Self {
            exponent,
            mantissa,
            inverted,
            _ph: PhantomData,
        }
    }

    pub fn make_static(self) -> Exponential<'static, B> {
        Exponential {
            exponent: self.exponent,
            mantissa: super::ms(self.mantissa),
            inverted: self.inverted,
            _ph: PhantomData,
        }
    }
}

pub trait Base {
    fn base() -> u64;
}
#[derive(Debug, Clone, PartialEq)]
pub struct Two;
impl Base for Two {
    fn base() -> u64 {
        2
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Ten;
impl Base for Ten {
    fn base() -> u64 {
        10
    }
}
