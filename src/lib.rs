//! A library for using CBOR as in-memory representation for working with dynamically shaped data.
//!
//! For the details on the data format see [RFC 8949](https://www.rfc-editor.org/rfc/rfc8949). It is
//! normally meant to be used as a data interchange format that models a superset of the JSON
//! features while employing a more compact binary representation. As such, the data representation
//! is biased towards smaller in-memory size and not towards fastest data access speed.
//!
//! This library presents a range of tradeoffs when using this data format. You can just use the
//! bits you get from the wire or from a file, without paying any initial overhead but with the
//! possibility of panicking during access and panicking when extracting (byte) strings encoded
//! with indefinite size. Or you can validate and canonicalise the bits before
//! using them, removing the possibility of panics and guaranteeing that indexing into the data
//! will never allocate.
//!
//! Regarding performance you should keep in mind that arrays and dictionaries are encoded as flat
//! juxtaposition of its elements, meaning that indexing will have to decode items as it skips over
//! them.
//!
//! Regarding the interpretation of parsed data you have the option of inspecting the particular
//! encoding (by pattern matching on [`CborValue`](struct.CborValue.html)) or extracting the information
//! you need using the API methods. In the latter case, many binary representations may yield the
//! same value, e.g. when asking for an integer the result may stem from a non-optimal encoding
//! (like writing 57 as 64-bit value) or from a BigDecimal with mantissa 570 and exponent -1.

use std::{
    borrow::{Borrow, Cow},
    convert::TryFrom,
    fmt::{Debug, Display, Write},
    ops::Deref,
};

mod builder;
mod canonical;
mod check;
pub mod constants;
mod error;
mod reader;
mod validated;
mod value;
mod visit;

#[cfg(test)]
mod tests;

pub use builder::{
    ArrayWriter, CborBuilder, CborOutput, DictWriter, Encoder, KeyBuilder, NoOutput, SingleBuilder,
    SingleResult, WithOutput, Writer,
};
pub use error::{Error, ErrorKind};
pub use reader::Literal;
pub use validated::{
    indexing::{IndexStr, PathElement},
    item::{ItemKind, TaggedItem},
    iterators::{ArrayIter, BytesIter, DictIter, StringIter},
    tags::Tags,
};
pub use value::{CborValue, Number};
pub use visit::Visitor;

use canonical::canonicalise;
use smallvec::SmallVec;
use validated::indexing::IndexVisitor;
use visit::visit;

/// Wrapper around a byte slice that encodes a valid CBOR item.
///
/// For details on the format see [RFC 8949](https://www.rfc-editor.org/rfc/rfc8949).
///
/// When interpreting CBOR messages from the outside (e.g. from the network) it is
/// advisable to ingest those using the [`CborOwned::canonical`](struct.CborOwned.html#method.canonical) constructor.
/// In case the message was encoded for example using [`CborBuilder`](./struct.CborBuilder.html)
/// it is sufficient to use the [`trusting`](#method.trusting) constructor.
///
/// The Display implementation adheres to the [diagnostic notation](https://datatracker.ietf.org/doc/html/rfc8949#section-8).
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Cbor([u8]);

impl From<&Cbor> for SmallVec<[u8; 16]> {
    fn from(a: &Cbor) -> Self {
        (&a.0).into()
    }
}

impl Debug for Cbor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut groups = 0;
        f.write_str("Cbor(")?;
        if f.alternate() {
            for chunk in self.0.chunks(4) {
                let c = if groups & 15 == 0 { '\n' } else { ' ' };
                f.write_char(c)?;
                groups += 1;
                for byte in chunk {
                    write!(f, "{:02x}", byte)?;
                }
            }
            f.write_char('\n')?;
        } else {
            for chunk in self.0.chunks(4) {
                if groups > 0 {
                    f.write_char(' ')?;
                } else {
                    groups = 1;
                }
                for byte in chunk {
                    write!(f, "{:02x}", byte)?;
                }
            }
        }
        f.write_char(')')
    }
}

impl Display for Cbor {
    fn fmt(&self, mut f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // check https://datatracker.ietf.org/doc/html/rfc8949#section-8 for the format
        impl<'a> Visitor<'a, std::fmt::Error> for &mut std::fmt::Formatter<'_> {
            fn visit_simple(&mut self, item: TaggedItem<'a>) -> Result<(), std::fmt::Error> {
                write!(self, "{}", item)?;
                Ok(())
            }

            fn visit_array_begin(
                &mut self,
                array: TaggedItem<'a>,
                size: Option<u64>,
            ) -> Result<bool, std::fmt::Error> {
                for tag in array.tags() {
                    write!(self, "{}(", tag)?;
                }
                write!(self, "[")?;
                if size.is_none() {
                    write!(self, "_ ")?;
                }
                Ok(true)
            }

            fn visit_array_index(
                &mut self,
                _array: TaggedItem<'a>,
                index: u64,
            ) -> Result<bool, std::fmt::Error> {
                if index > 0 {
                    write!(self, ", ")?;
                }
                Ok(true)
            }

            fn visit_array_end(&mut self, array: TaggedItem<'a>) -> Result<(), std::fmt::Error> {
                write!(self, "]")?;
                for _ in array.tags() {
                    write!(self, ")")?;
                }
                Ok(())
            }

            fn visit_dict_begin(
                &mut self,
                dict: TaggedItem<'a>,
                size: Option<u64>,
            ) -> Result<bool, std::fmt::Error> {
                for tag in dict.tags() {
                    write!(self, "{}(", tag)?;
                }
                write!(self, "{{")?;
                if size.is_none() {
                    write!(self, "_ ")?;
                }
                Ok(true)
            }

            fn visit_dict_key(
                &mut self,
                _dict: TaggedItem<'a>,
                key: TaggedItem<'a>,
                is_first: bool,
            ) -> Result<bool, std::fmt::Error> {
                if !is_first {
                    write!(self, ", ")?;
                }
                write!(self, "{}: ", key)?;
                Ok(true)
            }

            fn visit_dict_end(&mut self, dict: TaggedItem<'a>) -> Result<(), std::fmt::Error> {
                write!(self, "}}")?;
                for _ in dict.tags() {
                    write!(self, ")")?;
                }
                Ok(())
            }
        }
        visit(&mut f, self.tagged_item())
    }
}

impl AsRef<[u8]> for Cbor {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<'a> TryFrom<&'a [u8]> for &'a Cbor {
    type Error = error::Error<'static>;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        Cbor::checked(value).map_err(|e| e.rebase(value))
    }
}

impl ToOwned for Cbor {
    type Owned = CborOwned;

    fn to_owned(&self) -> Self::Owned {
        CborOwned::unchecked(&self.0)
    }
}

impl Cbor {
    /// Unconditionally cast the given byte slice as CBOR item
    ///
    /// No checks on the integrity are made, indexing methods may panic if encoded
    /// lengths are out of bound or when encountering invalid encodings.
    /// If you want to carefully treat data obtained from unreliable sources, prefer
    /// [`CborOwned::canonical`](struct.CborOwned.html#method.canonical).
    ///
    /// The results of [`CborBuilder`](struct.CborBuilder.html) can safely be fed to this method.
    pub fn unchecked(bytes: &[u8]) -> &Self {
        unsafe { std::mem::transmute(bytes) }
    }

    /// Unconditionally convert the given bytes as CBOR item
    ///
    /// The borrowed variant is converted using [`unchecked`](#method.unchecked) without
    /// allocating. The owned variant is converted by either reusing the allocated vector
    /// or storing the bytes inline (if they fit) and releasing the vector.
    ///
    /// No checks on the integrity are made, indexing methods may panic if encoded
    /// lengths are out of bound or when encountering invalid encodings.
    /// If you want to carefully treat data obtained from unreliable sources, prefer
    /// [`CborOwned::canonical`](struct.CborOwned.html#method.canonical).
    pub fn from_cow_unchecked(bytes: Cow<'_, [u8]>) -> Cow<'_, Cbor> {
        match bytes {
            Cow::Borrowed(b) => Cow::Borrowed(Cbor::unchecked(b)),
            Cow::Owned(v) => Cow::Owned(CborOwned::unchecked(v)),
        }
    }

    /// Cast the given byte slice as CBOR item if the encoding is valid
    pub fn checked(bytes: &[u8]) -> Result<&Self, Error<'static>> {
        let (cbor, rest) = check::validate(bytes, None).map_err(|e| e.rebase(bytes))?;
        if rest.is_empty() {
            Ok(cbor)
        } else {
            Err(Error::AtSlice(rest, ErrorKind::TrailingGarbage).rebase(bytes))
        }
    }

    /// Convert the given bytes to a CBOR item if the encoding is valid
    ///
    /// The borrowed variant is converted using [`checked`](#method.checked) without
    /// allocating. The owned variant is converted using [`CborOwned::canonical`](struct.CborOwned.html#method.canonical).
    pub fn from_cow_checked(bytes: Cow<'_, [u8]>) -> Result<Cow<'_, Cbor>, Error<'static>> {
        match bytes {
            Cow::Borrowed(b) => Cbor::checked(b).map(Cow::Borrowed).map_err(|e| e.rebase(b)),
            Cow::Owned(v) => CborOwned::canonical(v, false).map(Cow::Owned),
        }
    }

    /// A view onto the underlying bytes
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    // pub fn decode(&self) -> CborValue<'_> {
    //     todo!()
    // }

    pub fn item(&self) -> ItemKind<'_> {
        ItemKind::new(self)
    }

    pub fn tagged_item(&self) -> TaggedItem<'_> {
        TaggedItem::new(self)
    }

    /// Extract a value by indexing into arrays and dicts, with path elements yielded by an iterator.
    ///
    /// Returns None if an index doesn’t exist or the indexed object is neither an array nor a dict.
    /// When the object under consideration is an array, the next path element must represent an
    /// integer number.
    ///
    /// Providing an empty iterator will yield the current Cbor item.
    ///
    /// Returns a borrowed Cbor unless the traversal entered a TAG_CBOR_ITEM byte string with indefinite
    /// encoding (in which case the bytes need to be assembled into a Vec before continuing). This cannot
    /// happen if the item being indexed stems from [`CborOwned::canonical`](struct.CborOwned.html#method.canonical).
    pub fn index<'a, 'b>(
        &'a self,
        path: impl IntoIterator<Item = PathElement<'b>>,
    ) -> Option<Cow<'a, Cbor>> {
        visit(&mut IndexVisitor::new(path.into_iter()), self.tagged_item()).unwrap_err()
    }

    /// Extract a value by indexing into arrays and dicts, with path elements yielded by an iterator.
    ///
    /// Returns None if an index doesn’t exist or the indexed object is neither an array nor a dict.
    /// When the object under consideration is an array, the next path element must represent an
    /// integer number.
    ///
    /// Providing an empty iterator will yield the current Cbor item.
    ///
    /// # Panics
    ///
    /// Panics if this CBOR item contains a TAG_CBOR_ITEM byte string that has been index into by this
    /// path traversal. Use [`CborOwned::canonical`](struct.CborOwned.html#method.canonical) to ensure
    /// that this cannot happen.
    pub fn index_borrowed<'a, 'b>(
        &'a self,
        path: impl IntoIterator<Item = PathElement<'b>>,
    ) -> Option<&'a Cbor> {
        self.index(path).map(|cow| match cow {
            Cow::Borrowed(b) => b,
            Cow::Owned(_) => panic!("indexing required allocation"),
        })
    }

    /// Visit the interesting parts of this CBOR item as guided by the given
    /// [`Visitor`](trait.Visitor.html).
    ///
    /// Returns `false` if the visit was not even begun due to invalid or non-canonical CBOR.
    pub fn visit<'a, 'b, Err, V: Visitor<'a, Err> + 'b>(
        &'a self,
        visitor: &'b mut V,
    ) -> Result<(), Err> {
        visit(visitor, self.tagged_item())
    }
}

/// Wrapper around a vector of bytes, for parsing as CBOR.
///
/// For details on the format see [RFC 8949](https://www.rfc-editor.org/rfc/rfc8949).
///
/// When interpreting CBOR messages from the outside (e.g. from the network) it is
/// advisable to ingest those using the [`canonical`](#method.canonical) constructor.
/// In case the message was encoded for example using [`CborBuilder`](./struct.CborBuilder.html)
/// it is sufficient to use the [`trusting`](#method.trusting) constructor.
///
/// Canonicalisation rqeuires an intermediary data buffer, which can be supplied (and reused)
/// by the caller to save on allocations.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
// 16 bytes is the smallest that makes sense on 64bit platforms (size of a fat pointer)
pub struct CborOwned(SmallVec<[u8; 16]>);

impl Debug for CborOwned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(Borrow::<Cbor>::borrow(self), f)
    }
}

impl Display for CborOwned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(Borrow::<Cbor>::borrow(self), f)
    }
}

impl Borrow<Cbor> for CborOwned {
    fn borrow(&self) -> &Cbor {
        Cbor::unchecked(&*self.0)
    }
}

impl AsRef<Cbor> for CborOwned {
    fn as_ref(&self) -> &Cbor {
        Cbor::unchecked(&*self.0)
    }
}

impl AsRef<[u8]> for CborOwned {
    fn as_ref(&self) -> &[u8] {
        &*self.0
    }
}

impl Deref for CborOwned {
    type Target = Cbor;

    fn deref(&self) -> &Self::Target {
        self.borrow()
    }
}

impl TryFrom<&[u8]> for CborOwned {
    type Error = error::Error<'static>;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::canonical(value, false)
    }
}

impl CborOwned {
    /// Copy the bytes and wrap for indexing.
    ///
    /// No checks on the integrity are made, indexing methods may panic if encoded lengths are out of bound.
    /// If you want to carefully treat data obtained from unreliable sources, prefer
    /// [`canonical()`](#method.canonical).
    pub fn unchecked(bytes: impl Into<SmallVec<[u8; 16]>>) -> Self {
        Self(bytes.into())
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
    /// For more configuration options like reusing a scratch space or preferring definite size encoding
    /// see [`CborBuilder`](struct.CborBuilder.html).
    pub fn canonical(
        bytes: impl AsRef<[u8]>,
        permit_trailing_bytes: bool,
    ) -> Result<Self, Error<'static>> {
        let bytes = bytes.as_ref();
        let (rest, ret) = canonicalise(bytes, CborBuilder::new()).map_err(|e| e.rebase(bytes))?;
        if !permit_trailing_bytes && !rest.is_empty() {
            Err(Error::AtSlice(rest, ErrorKind::TrailingGarbage).rebase(bytes))
        } else {
            Ok(ret)
        }
    }
}

/// Generate an iterator of [`PathElement`](struct.PathElement.html) from a string
///
/// A path element is either
///
///  - a string starting with any other character than dot or opening bracket
///    and delimited by the next dot or opening bracket
///  - a number enclosed in brackets
///
/// `None` is returned in case an opening bracket is not matched with a closing one
/// or the characters between brackets are not a valid representation of `u64`.
///
/// # Examples:
///
/// ```rust
/// use cbor_data::{Cbor, index_str, ItemKind};
///
/// let cbor = Cbor::checked(b"eActyx").unwrap();
///
/// // dict key `x`, array index 12, dict key `y`
/// assert_eq!(cbor.index(index_str("x[12].y").unwrap()), None);
/// // empty string means the outermost item
/// assert!(matches!(cbor.index(index_str("").unwrap()).unwrap().item(), ItemKind::Str(s) if s == "Actyx"));
/// ```
pub fn index_str(s: &str) -> Option<IndexStr<'_>> {
    IndexStr::new(s)
}

struct DebugUsingDisplay<'a, T>(&'a T);
impl<'a, T: Display> Debug for DebugUsingDisplay<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.0, f)
    }
}
