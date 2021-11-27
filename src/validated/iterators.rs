use super::CborIter;
use crate::Cbor;
use std::{
    borrow::Cow,
    fmt::{Debug, Display, Formatter, Write},
    io::Read,
};

/// Iterator yielding the fragments of a text string item
///
/// Parsing an item can in general not return a contiguous string due to
/// [indefinite-length encoding](https://www.rfc-editor.org/rfc/rfc8949.html#section-3.2.3).
/// This Iterator gives you the choice whether you want to inspect the fragments or build
/// a contiguous string by allocating the necessary memory.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StringIter<'a>(CborIter<'a>);

impl<'a> StringIter<'a> {
    pub(crate) fn new(bytes: &'a [u8], len: Option<u64>) -> Self {
        Self(CborIter::new(bytes, len))
    }

    /// Indicates whether the underlying encoding was definite or indefinite
    ///
    /// If you want to check whether the string is available as contiguous slice, see
    /// [`as_str`](#method.as_str), which will also work for 0- and 1-fragment strings
    /// using indefinite-length encoding.
    pub fn is_indefinite(&self) -> bool {
        self.0.size().is_none()
    }

    /// Returns true if the string consists of zero fragments
    ///
    /// **A return value of `false` does not indicate that the string contains characters!**
    pub fn is_empty(&self) -> bool {
        let mut this = *self;
        this.next().is_none()
    }

    /// Try to borrow the string as a single slice
    ///
    /// This will only succeed for definite length encoding or strings with 0 or 1 fragments.
    pub fn as_str(&self) -> Option<&'a str> {
        let mut this = *self;
        if let Some(first) = this.next() {
            if this.next().is_none() {
                Some(first)
            } else {
                None
            }
        } else {
            Some("")
        }
    }

    /// Extract the full string, borrowing if possible and allocating if necessary
    pub fn as_cow(&self) -> Cow<'a, str> {
        if let Some(s) = self.as_str() {
            Cow::Borrowed(s)
        } else {
            Cow::Owned(self.collect())
        }
    }
}

impl<'a> Debug for StringIter<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("StringIter")
    }
}

impl<'a> Display for StringIter<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for s in *self {
            f.write_str(s)?;
        }
        Ok(())
    }
}

impl<'a> Iterator for StringIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let v = self
            .0
            .next()?
            .0
            .expect("string fragments must be in definite size encoding");
        Some(unsafe { std::str::from_utf8_unchecked(v) })
    }
}

impl<'a, S: AsRef<str>> PartialEq<S> for StringIter<'a> {
    fn eq(&self, other: &S) -> bool {
        let this = *self;
        let mut other = other.as_ref();
        for prefix in this {
            if other.starts_with(prefix) {
                other = &other[prefix.len()..];
            } else {
                return false;
            }
        }
        other.is_empty()
    }
}

/// Iterator yielding the fragments of a byte string item
///
/// Parsing an item can in general not return a contiguous string due to
/// [indefinite-length encoding](https://www.rfc-editor.org/rfc/rfc8949.html#section-3.2.3).
/// This Iterator gives you the choice whether you want to inspect the fragments or build
/// a contiguous string by allocating the necessary memory.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BytesIter<'a>(CborIter<'a>, &'a [u8]);

impl<'a> BytesIter<'a> {
    pub(crate) fn new(bytes: &'a [u8], len: Option<u64>) -> Self {
        Self(CborIter::new(bytes, len), b"")
    }

    /// Indicates whether the underlying encoding was definite or indefinite
    ///
    /// If you want to check whether the string is available as contiguous slice, see
    /// [`as_slice`](#method.as_slice), which will also work for 0- and 1-fragment strings
    /// using indefinite-length encoding.
    pub fn is_indefinite(&self) -> bool {
        self.0.size().is_none()
    }

    /// Returns true if the string consists of zero fragments
    ///
    /// **A return value of `false` does not indicate that the string contains bytes!**
    pub fn is_empty(&self) -> bool {
        let mut this = *self;
        this.next().is_none()
    }

    /// Try to borrow the string as a single slice
    ///
    /// This will only succeed for definite length encoding or strings with 0 or 1 fragments.
    pub fn as_slice(&self) -> Option<&'a [u8]> {
        let mut this = *self;
        if let Some(first) = this.next() {
            if this.next().is_none() {
                Some(first)
            } else {
                None
            }
        } else {
            Some(b"")
        }
    }

    /// Extract the full string, borrowing if possible and allocating if necessary
    pub fn as_cow(&self) -> Cow<'a, [u8]> {
        if let Some(s) = self.as_slice() {
            Cow::Borrowed(s)
        } else {
            let mut v = Vec::new();
            for b in *self {
                v.extend_from_slice(b);
            }
            Cow::Owned(v)
        }
    }

    /// Extract the full string by allocating a fresh Vec
    pub fn to_vec(self) -> Vec<u8> {
        let mut ret = Vec::new();
        for v in self {
            ret.extend_from_slice(v);
        }
        ret
    }
}

impl<'a> Debug for BytesIter<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("BytesIter")
    }
}

impl<'a> Display for BytesIter<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for b in *self {
            if first {
                first = false;
            } else {
                f.write_char(' ')?;
            }
            for byte in b {
                write!(f, "{:02x}", byte)?;
            }
        }
        Ok(())
    }
}

impl<'a> Iterator for BytesIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        Some(
            self.0
                .next()?
                .0
                .expect("byte string fragments must be in definite size encoding"),
        )
    }
}

impl<'a> Read for BytesIter<'a> {
    fn read(&mut self, mut buf: &mut [u8]) -> std::io::Result<usize> {
        let mut in_hand = self.1;
        let mut read = 0;
        loop {
            let transfer = in_hand.len().min(buf.len());
            let (left, right) = buf.split_at_mut(transfer);
            left.copy_from_slice(&in_hand[..transfer]);
            in_hand = &in_hand[transfer..];
            buf = right;
            read += transfer;
            if buf.is_empty() {
                break;
            }
            // in_hand must be empty
            if let Some(bytes) = self.next() {
                in_hand = bytes;
            } else {
                break;
            }
        }
        self.1 = in_hand;
        Ok(read)
    }
}

impl<'a, S: AsRef<[u8]>> PartialEq<S> for BytesIter<'a> {
    fn eq(&self, other: &S) -> bool {
        let this = *self;
        let mut other = other.as_ref();
        for prefix in this {
            if other.starts_with(prefix) {
                other = &other[prefix.len()..];
            } else {
                return false;
            }
        }
        other.is_empty()
    }
}

/// Iterator over the CBOR items within an array
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArrayIter<'a>(CborIter<'a>);

impl<'a> ArrayIter<'a> {
    pub(crate) fn new(bytes: &'a [u8], len: Option<u64>) -> Self {
        Self(CborIter::new(bytes, len))
    }

    /// Number of items still remaining, or `None` in case of
    /// [indefinite-length encoding](https://www.rfc-editor.org/rfc/rfc8949.html#section-3.2.2)
    pub fn size(&self) -> Option<u64> {
        self.0.size()
    }
}

impl<'a> Debug for ArrayIter<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("ArrayIter")
    }
}
impl<'a> Iterator for ArrayIter<'a> {
    type Item = &'a Cbor;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.0.next()?.1)
    }
}

/// Iterator over the key–value mappings within a dictionary
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DictIter<'a>(CborIter<'a>);

impl<'a> DictIter<'a> {
    pub(crate) fn new(bytes: &'a [u8], len: Option<u64>) -> Self {
        Self(CborIter::new(bytes, len.map(|x| x * 2)))
    }

    /// Number of items still remaining, or `None` in case of
    /// [indefinite-length encoding](https://www.rfc-editor.org/rfc/rfc8949.html#section-3.2.2)
    pub fn size(&self) -> Option<u64> {
        self.0.size().map(|x| x / 2)
    }
}

impl<'a> Debug for DictIter<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("DictIter")
    }
}
impl<'a> Iterator for DictIter<'a> {
    type Item = (&'a Cbor, &'a Cbor);

    fn next(&mut self) -> Option<Self::Item> {
        Some((self.0.next()?.1, self.0.next()?.1))
    }
}
