use super::iterators::{ArrayIter, BytesIter, DictIter, StringIter};
use crate::{constants::TAG_CBOR_ITEM, Cbor, CborValue, DebugUsingDisplay, Tags};
use std::fmt::{Debug, Display, Formatter, Write};

/// Low-level encoding of a CBOR item. Use [`CborValue`](value/enum.CborValue.html) for inspecting values.
///
/// You can obtain this representation from [`Cbor::kind`](struct.Cbor.html#method.kind) or
/// [`TaggedItem::kind`](struct.TaggedItem.html#method.kind).
///
/// Beware of the `Neg` variant, which carries `-1 - x`.
#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub enum ItemKind<'a> {
    Pos(u64),
    Neg(u64),
    Float(f64),
    Str(StringIter<'a>),
    Bytes(BytesIter<'a>),
    Bool(bool),
    Null,
    Undefined,
    Simple(u8),
    Array(ArrayIter<'a>),
    Dict(DictIter<'a>),
}

impl<'a> Debug for ItemKind<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pos(arg0) => f.debug_tuple("Pos").field(arg0).finish(),
            Self::Neg(arg0) => f.debug_tuple("Neg").field(arg0).finish(),
            Self::Float(arg0) => f.debug_tuple("Float").field(arg0).finish(),
            Self::Str(arg0) => f
                .debug_tuple("Str")
                .field(&DebugUsingDisplay(arg0))
                .finish(),
            Self::Bytes(arg0) => f
                .debug_tuple("Bytes")
                .field(&DebugUsingDisplay(arg0))
                .finish(),
            Self::Bool(arg0) => f.debug_tuple("Bool").field(arg0).finish(),
            Self::Null => write!(f, "Null"),
            Self::Undefined => write!(f, "Undefined"),
            Self::Simple(arg0) => f.debug_tuple("Simple").field(arg0).finish(),
            Self::Array(arg0) => write!(f, "Array({:?})", arg0.size()),
            Self::Dict(arg0) => write!(f, "Dict({:?})", arg0.size()),
        }
    }
}

impl<'a> Display for ItemKind<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemKind::Pos(_) => write!(f, "positive number"),
            ItemKind::Neg(_) => write!(f, "negative number"),
            ItemKind::Float(_) => write!(f, "floating-point number"),
            ItemKind::Str(_) => write!(f, "text string"),
            ItemKind::Bytes(_) => write!(f, "byte string"),
            ItemKind::Bool(_) => write!(f, "boolean"),
            ItemKind::Null => write!(f, "null"),
            ItemKind::Undefined => write!(f, "undefined"),
            ItemKind::Simple(_) => write!(f, "simple value"),
            ItemKind::Array(_) => write!(f, "array"),
            ItemKind::Dict(_) => write!(f, "dictionary"),
        }
    }
}

impl<'a> ItemKind<'a> {
    pub fn new(cbor: &'a Cbor) -> Self {
        super::item(cbor.as_slice())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKindShort {
    Pos,
    Neg,
    Float,
    Str,
    Bytes,
    Bool,
    Null,
    Undefined,
    Simple,
    Array,
    Dict,
}

impl Display for ItemKindShort {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemKindShort::Pos => write!(f, "positive number"),
            ItemKindShort::Neg => write!(f, "negative number"),
            ItemKindShort::Float => write!(f, "floating-point number"),
            ItemKindShort::Str => write!(f, "text string"),
            ItemKindShort::Bytes => write!(f, "byte string"),
            ItemKindShort::Bool => write!(f, "boolean"),
            ItemKindShort::Null => write!(f, "null"),
            ItemKindShort::Undefined => write!(f, "undefined"),
            ItemKindShort::Simple => write!(f, "simple value"),
            ItemKindShort::Array => write!(f, "array"),
            ItemKindShort::Dict => write!(f, "dictionary"),
        }
    }
}

impl<'a> From<ItemKind<'a>> for ItemKindShort {
    fn from(kind: ItemKind<'a>) -> Self {
        match kind {
            ItemKind::Pos(_) => ItemKindShort::Pos,
            ItemKind::Neg(_) => ItemKindShort::Neg,
            ItemKind::Float(_) => ItemKindShort::Float,
            ItemKind::Str(_) => ItemKindShort::Str,
            ItemKind::Bytes(_) => ItemKindShort::Bytes,
            ItemKind::Bool(_) => ItemKindShort::Bool,
            ItemKind::Null => ItemKindShort::Null,
            ItemKind::Undefined => ItemKindShort::Undefined,
            ItemKind::Simple(_) => ItemKindShort::Simple,
            ItemKind::Array(_) => ItemKindShort::Array,
            ItemKind::Dict(_) => ItemKindShort::Dict,
        }
    }
}

/// Representation of a possibly tagged CBOR data item
///
/// You can obtain this representation using [`Cbor::tagged_item`](struct.Cbor.html#method.tagged_item).
///
/// It holds an iterable representation of the tags, a decoded [`ItemKind`](enum.ItemKind.html)
/// and a reference to the underlying bytes for the whole item. Since all these are shallow
/// references to existing data, this structure itself is `Copy`.
#[derive(Clone, Copy, PartialEq)]
pub struct TaggedItem<'a> {
    tags: Tags<'a>,
    kind: ItemKind<'a>,
    cbor: &'a Cbor,
}

impl<'a> Debug for TaggedItem<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TaggedItem({}, {:?})", self.tags, self.kind)
    }
}

struct W([u8; 32], u8);
impl W {
    pub fn new() -> Self {
        Self([0; 32], 0)
    }
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&self.0.as_ref()[0..self.1 as usize]) }
    }
}
impl std::fmt::Write for W {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let end = self.1 as usize + s.len();
        if end > 24 {
            Err(std::fmt::Error)
        } else {
            self.0.as_mut()[self.1 as usize..end].copy_from_slice(s.as_bytes());
            self.1 = end as u8;
            Ok(())
        }
    }
}

#[allow(clippy::many_single_char_names)]
fn write_float(f: &mut std::fmt::Formatter<'_>, x: f64) -> std::fmt::Result {
    if x == f64::INFINITY {
        write!(f, "Infinity")
    } else if x == f64::NEG_INFINITY {
        write!(f, "-Infinity")
    } else if x.is_nan() {
        write!(f, "NaN")
    } else {
        let mut w = W::new();
        if x != 0.0 && (x.abs() < 1e-6 || x.abs() > 1e16) {
            write!(w, "{:e}", x)?;
        } else {
            write!(w, "{}", x)?;
        }
        let s = w.as_str();

        let e = s.find('e').unwrap_or(s.len());
        let (mantissa, exponent) = s.split_at(e);
        write!(f, "{}", mantissa)?;
        if !mantissa.contains('.') {
            write!(f, ".0")?;
        }
        write!(f, "{}", exponent)?;
        Ok(())
    }
}

impl<'a> Display for TaggedItem<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let (Some(TAG_CBOR_ITEM), ItemKind::Bytes(bytes)) = (self.tags().single(), self.kind()) {
            let indefinite = if bytes.is_indefinite() { "_ " } else { "" };
            let bytes = bytes.as_cow();
            let cbor = Cbor::unchecked(bytes.as_ref());
            return write!(f, "<{}{}>", indefinite, cbor);
        }

        let mut parens = 0;
        for tag in self.tags() {
            write!(f, "{}(", tag)?;
            parens += 1;
        }
        match self.kind() {
            ItemKind::Pos(x) => write!(f, "{}", x)?,
            ItemKind::Neg(x) => write!(f, "{}", -1 - (i128::from(x)))?,
            ItemKind::Float(x) => write_float(f, x)?,
            ItemKind::Str(mut s) => {
                if s.is_indefinite() {
                    if s.is_empty() {
                        write!(f, "\"\"_")?;
                    } else {
                        write!(f, "(_")?;
                        let mut first = true;
                        for s in s {
                            if first {
                                first = false;
                            } else {
                                write!(f, ",")?;
                            }
                            write!(f, " \"{}\"", s.escape_debug())?;
                        }
                        write!(f, ")")?;
                    }
                } else {
                    let s = s.next().unwrap();
                    write!(f, "\"{}\"", s.escape_debug())?;
                }
            }
            ItemKind::Bytes(mut b) => {
                if b.is_indefinite() {
                    if b.is_empty() {
                        write!(f, "''_")?;
                    } else {
                        write!(f, "(_")?;
                        let mut first = true;
                        for b in b {
                            if first {
                                first = false;
                            } else {
                                write!(f, ",")?;
                            }
                            write!(f, " h'")?;
                            for byte in b {
                                write!(f, "{:02x}", byte)?;
                            }
                            write!(f, "'")?;
                        }
                        write!(f, ")")?;
                    }
                } else {
                    let b = b.next().unwrap();
                    write!(f, "h'")?;
                    for byte in b {
                        write!(f, "{:02x}", byte)?;
                    }
                    write!(f, "'")?;
                }
            }
            ItemKind::Bool(b) => write!(f, "{}", b)?,
            ItemKind::Null => write!(f, "null")?,
            ItemKind::Undefined => write!(f, "undefined")?,
            ItemKind::Simple(s) => write!(f, "simple({})", s)?,
            ItemKind::Array(_) => unreachable!(),
            ItemKind::Dict(_) => unreachable!(),
        }
        for _ in 0..parens {
            write!(f, ")")?;
        }
        Ok(())
    }
}

impl<'a> TaggedItem<'a> {
    pub fn new(cbor: &'a Cbor) -> Self {
        let (tags, kind) = super::tagged_item(cbor.as_ref());
        Self { tags, kind, cbor }
    }

    /// Interpret the CBOR item at a higher level
    ///
    /// While [`kind`](#method.kind) gives you precise information on how the item is encoded,
    /// this method interprets the tag-based encoding according to the standard, adding for example
    /// big integers, decimals, and floats, or turning base64-encoded text strings into binary strings.
    pub fn decode(self) -> CborValue<'a> {
        CborValue::new(self)
    }

    /// An iterator over the tags of this item
    pub fn tags(&self) -> Tags<'a> {
        self.tags
    }

    /// A decoded form of the low-level representation of the CBOR item
    pub fn kind(&self) -> ItemKind<'a> {
        self.kind
    }

    /// A reference to the underlying bytes from which this structure has been lifted
    pub fn cbor(&self) -> &'a Cbor {
        self.cbor
    }
}
