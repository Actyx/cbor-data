//! A library for using CBOR as in-memory representation for working with dynamically shaped data.
//!
//! For the details on the data format see [RFC7049](https://tools.ietf.org/html/rfc7049). It is
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
//! encoding (by pattern matching on [`CborValue`](struct.CborValue)) or extracting the information
//! you need using the API methods. In the latter case, many binary representations may yield the
//! same value, e.g. when asking for an integer the result may stem from a non-optimal encoding
//! (like writing 57 as 64-bit value) or from a BigDecimal with mantissa 570 and exponent -1.

use std::fmt::Debug;

mod builder;
mod canonical;
mod constants;
mod reader;
mod value;

#[cfg(test)]
mod tests;

pub use builder::{ArrayBuilder, CborBuilder, DictBuilder, WriteToArray, WriteToDict};
use canonical::canonicalise;
pub use reader::Literal;
pub use value::{CborValue, ValueKind};

use constants::{MAJOR_ARRAY, MAJOR_DICT, MAJOR_TAG};
use reader::{integer, major, ptr, tagged_value};

/// Wrapper around some bytes (referenced or owned) that allows parsing as CBOR value.
///
/// For details on the format see [RFC7049](https://tools.ietf.org/html/rfc7049).
///
/// When interpreting CBOR messages from the outside (e.g. from the network) it is
/// advisable to ingest those using the [`canonical`](#method.canonical) constructor.
/// In case the message was encoded for example using [`CborBuilder`](./struct.CborBuilder.html)
/// it is sufficient to use the [`trusting`](#method.trusting) constructor.
///
/// Canonicalisation rqeuires an intermediary data buffer, which can be supplied (and reused)
/// by the caller to save on allocations.
#[derive(PartialEq)]
pub struct Cbor<'a>(&'a [u8]);

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

impl<'a> Cbor<'a> {
    /// Wrap in Cbor for indexing.
    ///
    /// No checks on the integrity are made, indexing methods may panic if encoded
    /// lengths are out of bound or when encountering indefinite size (byte) strings.
    /// If you want to carefully treat data obtained from unreliable sources, prefer
    /// [`CborOwned::canonical`](struct.CborOwned#method.canonical). The results of
    /// [`CborBuilder`](struct.CborBuilder) can also safely be fed to this method.
    pub fn trusting(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    /// Copy the underlying bytes to create a fully owned CBOR value.
    ///
    /// No checks on the integrity are made, indexing methods may panic if encoded
    /// lengths are out of bound or when encountering indefinite size (byte) strings.
    /// If you want to carefully treat data obtained from unreliable sources, prefer
    /// [`CborOwned::canonical`](struct.CborOwned#method.canonical). The results of
    /// [`CborBuilder`](struct.CborBuilder) can also safely be fed to this method.
    pub fn to_owned(&self) -> CborOwned {
        CborOwned::trusting(self.as_ref())
    }
}

impl<'a> AsRef<[u8]> for Cbor<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<'a> Cbor<'a> {
    /// A view onto the underlying bytes
    pub fn as_slice(&self) -> &'a [u8] {
        self.0
    }

    /// Extract the single value represented by this piece of CBOR
    pub fn value(&self) -> Option<CborValue<'a>> {
        tagged_value(self.as_slice())
    }

    /// Extract a value by indexing into arrays and dicts, with path elements separated by dot.
    ///
    /// The empty string will yield the same as calling [`value()`](#method.value). If path elements
    /// may contain `.` then use [`index_iter()`](#method.index_iter).
    pub fn index(&self, path: &str) -> Option<CborValue<'a>> {
        ptr(self.as_slice(), path.split_terminator('.'))
    }

    /// Extract a value by indexing into arrays and dicts, with path elements yielded by an iterator.
    ///
    /// The empty iterator will yield the same as calling [`value()`](#method.value).
    pub fn index_iter<'b>(&self, path: impl IntoIterator<Item = &'b str>) -> Option<CborValue<'a>> {
        ptr(self.as_slice(), path.into_iter())
    }

    /// Check if this CBOR contains an array as its top-level item.
    /// Returns false also in case of data format problems.
    pub fn is_array(&self) -> bool {
        let mut bytes = self.as_slice();
        while major(bytes) == Some(MAJOR_TAG) {
            bytes = match integer(bytes) {
                Some((_, _, r)) => r,
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
                Some((_, _, r)) => r,
                None => return false,
            };
        }
        major(bytes) == Some(MAJOR_DICT)
    }
}

#[derive(PartialEq, Clone)]
pub struct CborOwned(Vec<u8>);

impl Debug for CborOwned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Cbor::trusting(&*self.0).fmt(f)
    }
}

impl CborOwned {
    /// Copy the bytes and wrap for indexing.
    ///
    /// No checks on the integrity are made, indexing methods may panic if encoded lengths are out of bound.
    /// If you want to carefully treat data obtained from unreliable sources, prefer
    /// [`canonical()`](#method.canonical).
    pub fn trusting(bytes: impl Into<Vec<u8>>) -> Self {
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

    /// Borrow the underlying bytes for Cbor interpretation.
    pub fn borrow(&self) -> Cbor {
        Cbor::trusting(self.as_ref())
    }

    /// A view onto the underlying bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Extract the single value represented by this piece of CBOR.
    pub fn value(&self) -> Option<CborValue> {
        self.borrow().value()
    }

    /// Extract a value by indexing into arrays and dicts, with path elements separated by dot.
    ///
    /// The empty string will yield the same as calling [`value()`](#method.value). If path elements
    /// may contain `.` then use [`index_iter()`](#method.index_iter).
    pub fn index(&self, path: &str) -> Option<CborValue> {
        self.borrow().index(path)
    }

    /// Extract a value by indexing into arrays and dicts, with path elements yielded by an iterator.
    ///
    /// The empty iterator will yield the same as calling [`value()`](#method.value).
    pub fn index_iter<'b>(&self, path: impl IntoIterator<Item = &'b str>) -> Option<CborValue> {
        self.borrow().index_iter(path)
    }
}

impl AsRef<[u8]> for CborOwned {
    fn as_ref(&self) -> &[u8] {
        &*self.0
    }
}
