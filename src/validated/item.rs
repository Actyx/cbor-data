use super::iterators::{ArrayIter, BytesIter, DictIter, StringIter};
use crate::{constants::TAG_CBOR_ITEM, Cbor, DebugUsingDisplay, Tags};
use std::fmt::{Debug, Display, Formatter, Write};

/// Low-level decoded form of a CBOR item. Use CborValue for inspecting values.
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

impl<'a> ItemKind<'a> {
    pub fn new(cbor: &'a Cbor) -> Self {
        super::item(cbor.as_slice())
    }
}

/// Representation of a possibly tagged CBOR data item
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

        let e = s.find('e').unwrap_or_else(|| s.len());
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
