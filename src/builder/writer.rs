use super::low_level::*;
use crate::{
    canonical::canonicalise, constants::*, ArrayWriter, Cbor, CborBuilder, DictWriter, Literal,
    ParseError,
};

/// Low-level primitives for emitting CBOR items.
///
/// The methods of this trait give you full control over the encoding of values according to the
/// CBOR specification (apart from the technically allowed non-optimal integer encodings). It
/// allows you to emit any item tagged with any number you desire.
///
/// If you are looking for convenient methods of writing end-user data types please refer to
/// the [`Encoder`](trait.Encoder.html) trait.
pub trait Writer: Sized {
    type Output;
    #[doc(hidden)]
    // internal helper method — do not use!
    /// contract: each call to this method MUST corresopnd to a single CBOR item being written!
    fn bytes<T>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> T) -> T;
    #[doc(hidden)]
    // internal helper method — do not use!
    fn into_output(self) -> Self::Output;

    /// Configured maximum array or dict length up to which definite size encoding is used.
    fn max_definite(&self) -> Option<u64>;

    /// Set the maximum array or dict length up to which definite size encoding is used.
    fn set_max_definite(&mut self, max: Option<u64>);

    /// Write a unsigned value of up to 64 bits.
    /// Tags are from outer to inner.
    fn write_pos(mut self, value: u64, tags: impl IntoIterator<Item = u64>) -> Self::Output {
        self.bytes(|b| write_positive(b, value, tags));
        self.into_output()
    }

    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    /// Tags are from outer to inner.
    fn write_neg(mut self, value: u64, tags: impl IntoIterator<Item = u64>) -> Self::Output {
        self.bytes(|b| write_neg(b, value, tags));
        self.into_output()
    }

    /// Write the given slice as a definite size byte string.
    /// Tags are from outer to inner.
    fn write_bytes(mut self, value: &[u8], tags: impl IntoIterator<Item = u64>) -> Self::Output {
        self.bytes(|b| write_bytes(b, value.len(), [value], tags));
        self.into_output()
    }

    /// Write the given slices as a definite size byte string.
    /// Tags are from outer to inner.
    ///
    /// Example:
    /// ```rust
    /// # use cbor_data::{CborBuilder, Writer};
    /// let cbor = CborBuilder::default().write_bytes_chunked([&[0][..], &[1, 2][..]], [12]);
    /// # assert_eq!(cbor.as_slice(), vec![0xccu8, 0x43, 0, 1, 2]);
    /// ```
    fn write_bytes_chunked(
        mut self,
        value: impl IntoIterator<Item = impl AsRef<[u8]>> + Copy,
        tags: impl IntoIterator<Item = u64>,
    ) -> Self::Output {
        let len = value.into_iter().map(|x| x.as_ref().len()).sum();
        self.bytes(|b| write_bytes(b, len, value, tags));
        self.into_output()
    }

    /// Write the given slice as a definite size string.
    /// Tags are from outer to inner.
    fn write_str(mut self, value: &str, tags: impl IntoIterator<Item = u64>) -> Self::Output {
        self.bytes(|b| write_str(b, value.len(), [value], tags));
        self.into_output()
    }

    /// Write the given slice as a definite size string.
    /// Tags are from outer to inner.
    ///
    /// Example:
    /// ```rust
    /// # use cbor_data::{CborBuilder, Writer};
    /// let cbor = CborBuilder::default().write_str_chunked(["a", "b"], [12]);
    /// # assert_eq!(cbor.as_slice(), vec![0xccu8, 0x62, 0x61, 0x62]);
    /// ```
    fn write_str_chunked(
        mut self,
        value: impl IntoIterator<Item = impl AsRef<str>> + Copy,
        tags: impl IntoIterator<Item = u64>,
    ) -> Self::Output {
        let len = value.into_iter().map(|x| x.as_ref().len()).sum();
        self.bytes(|b| write_str(b, len, value, tags));
        self.into_output()
    }

    /// Tags are from outer to inner.
    fn write_bool(mut self, value: bool, tags: impl IntoIterator<Item = u64>) -> Self::Output {
        self.bytes(|b| write_bool(b, value, tags));
        self.into_output()
    }

    /// Tags are from outer to inner.
    fn write_null(mut self, tags: impl IntoIterator<Item = u64>) -> Self::Output {
        self.bytes(|b| write_null(b, tags));
        self.into_output()
    }

    /// Tags are from outer to inner.
    fn write_undefined(mut self, tags: impl IntoIterator<Item = u64>) -> Self::Output {
        self.bytes(|b| write_undefined(b, tags));
        self.into_output()
    }

    /// Write custom literal value — [RFC 8949 §3.3](https://www.rfc-editor.org/rfc/rfc8949#section-3.3) is required reading.
    /// Tags are from outer to inner.
    fn write_lit(mut self, value: Literal, tags: impl IntoIterator<Item = u64>) -> Self::Output {
        self.bytes(|b| {
            write_tags(b, tags);
            write_lit(b, value)
        });
        self.into_output()
    }

    /// Write a nested array using the given closure that receives an array builder.
    /// Tags are from outer to inner.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer};
    /// let cbor = CborBuilder::default().write_array(None, |builder| {
    ///     builder.write_array_ret(None, |builder| {
    ///         builder.write_pos(42, None);
    ///     });
    /// });
    /// # assert_eq!(cbor.as_slice(), vec![0x81u8, 0x81, 0x18, 42]);
    /// ```
    fn write_array<F>(self, tags: impl IntoIterator<Item = u64>, f: F) -> Self::Output
    where
        F: FnOnce(&mut ArrayWriter<'_>),
    {
        self.write_array_ret(tags, f).0
    }

    /// Write a nested array using the given closure that receives an array builder.
    /// Tags are from outer to inner.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer};
    /// let (cbor, ret) = CborBuilder::default().write_array_ret(None, |builder| {
    ///     builder.write_array_ret(None, |builder| {
    ///         builder.write_pos(42, None);
    ///     });
    ///     42
    /// });
    /// assert_eq!(ret, 42);
    /// # assert_eq!(cbor.as_slice(), vec![0x81u8, 0x81, 0x18, 42]);
    /// ```
    fn write_array_ret<T, F>(
        mut self,
        tags: impl IntoIterator<Item = u64>,
        f: F,
    ) -> (Self::Output, T)
    where
        F: FnOnce(&mut ArrayWriter<'_>) -> T,
    {
        let max_definite = self.max_definite();
        let ret = self.bytes(|b| {
            write_tags(b, tags);
            let pos = b.len();
            write_indefinite(b, MAJOR_ARRAY);
            let mut writer = ArrayWriter::new(b, max_definite);
            let ret = f(&mut writer);
            let max_definite = writer.max_definite();
            finish_array(writer.count(), b, pos, MAJOR_ARRAY, max_definite);
            ret
        });
        (self.into_output(), ret)
    }

    /// Write a nested dict using the given closure that receives a dict builder.
    /// Tags are from outer to inner.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer};
    /// let cbor = CborBuilder::default().write_array(None, |builder | {
    ///     builder.write_dict_ret(None, |builder| {
    ///         builder.with_key("y", |b| b.write_pos(42, None));
    ///     });
    /// });
    /// # assert_eq!(cbor.as_slice(), vec![0x81u8, 0xa1, 0x61, b'y', 0x18, 42]);
    /// ```
    fn write_dict<F>(self, tags: impl IntoIterator<Item = u64>, f: F) -> Self::Output
    where
        F: FnOnce(&mut DictWriter<'_>),
    {
        self.write_dict_ret(tags, f).0
    }

    /// Write a nested dict using the given closure that receives a dict builder.
    /// Tags are from outer to inner.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer};
    /// let (cbor, ret) = CborBuilder::default().write_array_ret(None, |builder | {
    ///     builder.write_dict_ret(None, |builder| {
    ///         builder.with_key("y", |b| b.write_pos(42, None));
    ///     });
    ///     42
    /// });
    /// assert_eq!(ret, 42);
    /// # assert_eq!(cbor.as_slice(), vec![0x81u8, 0xa1, 0x61, b'y', 0x18, 42]);
    /// ```
    fn write_dict_ret<T, F>(
        mut self,
        tags: impl IntoIterator<Item = u64>,
        f: F,
    ) -> (Self::Output, T)
    where
        F: FnOnce(&mut DictWriter<'_>) -> T,
    {
        let max_definite = self.max_definite();
        let ret = self.bytes(|b| {
            write_tags(b, tags);
            let pos = b.len();
            write_indefinite(b, MAJOR_DICT);
            let mut writer = DictWriter::new(b, max_definite);
            let ret = f(&mut writer);
            let max_definite = writer.max_definite();
            finish_array(writer.count(), b, pos, MAJOR_DICT, max_definite);
            ret
        });
        (self.into_output(), ret)
    }

    /// Interpret the given bytes as a single CBOR item and write it to this builder,
    /// canonicalising its contents like [`CborOwned::canonical()`](struct.CborOwned.html#method.canonical)
    fn write_canonical(mut self, bytes: &[u8]) -> Result<Self::Output, ParseError> {
        let max_definite = self.max_definite();
        self.bytes(|b| {
            canonicalise(
                bytes,
                CborBuilder::append_to(b).with_max_definite_size(max_definite),
            )
        })
        .map(|_| self.into_output())
    }

    /// Assume that the given bytes are a well-formed single CBOR item and write it to this builder.
    ///
    /// If those bytes are not valid CBOR you get to keep the pieces!
    fn write_trusting(mut self, bytes: &[u8]) -> Self::Output {
        self.bytes(|b| b.extend_from_slice(bytes));
        self.into_output()
    }

    /// Write the given CBOR item
    fn write_item(self, item: &Cbor) -> Self::Output {
        self.write_trusting(item.as_slice())
    }
}

impl<T> Writer for &mut T
where
    T: Writer<Output = T>,
{
    type Output = Self;

    fn bytes<U>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> U) -> U {
        (*self).bytes(f)
    }

    fn into_output(self) -> Self::Output {
        self
    }

    fn max_definite(&self) -> Option<u64> {
        (**self).max_definite()
    }

    fn set_max_definite(&mut self, max: Option<u64>) {
        (**self).set_max_definite(max);
    }
}
