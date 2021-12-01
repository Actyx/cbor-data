use super::low_level::Bytes;
use crate::Writer;
use std::marker::PhantomData;

/// Builder for an array value, used by `write_array_ret()`.
///
/// see [`trait Encoder`](trait.Encoder.html) for usage examples
pub struct ArrayWriter<'a> {
    bytes: Bytes<'a>,
    count: u64,
    max_definite: Option<u64>,
}

impl<'a> ArrayWriter<'a> {
    pub(crate) fn new(bytes: &'a mut Vec<u8>, max_definite: Option<u64>) -> Self {
        Self {
            bytes: Bytes::Borrowed(bytes),
            count: 0,
            max_definite,
        }
    }
    fn non_tracking(&mut self, max_definite: Option<u64>) -> ArrayWriter {
        ArrayWriter {
            bytes: self.bytes.copy(),
            count: 0,
            max_definite,
        }
    }
    /// Configure the limit above which indefinite size encoding will be used.
    ///
    /// The default is 255, which is the largest size up to which definite size is at least as
    /// compact as indefinite size. Set to 23 to avoid moving bytes around when finishing the array.
    /// Set to `None` to always use indefinite size encoding.
    pub fn set_max_definite_size(&mut self, size: Option<u64>) {
        self.max_definite = size;
    }

    pub(crate) fn count(&self) -> u64 {
        self.count
    }
}

impl<'a> Writer for ArrayWriter<'a> {
    type Output = Self;

    fn bytes<T>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> T) -> T {
        self.count += 1;
        f(self.bytes.as_mut())
    }

    fn into_output(self) -> Self::Output {
        self
    }

    fn max_definite(&self) -> Option<u64> {
        self.max_definite
    }
}

/// Builder for a dict value, used by `write_dict_rec()`.
///
/// see [`trait Encoder`](trait.Encoder.html) for usage examples
pub struct DictWriter<'a>(ArrayWriter<'a>);

impl<'a> DictWriter<'a> {
    pub(crate) fn new(bytes: &'a mut Vec<u8>, max_definite: Option<u64>) -> Self {
        Self(ArrayWriter::new(bytes, max_definite))
    }

    pub(crate) fn count(&self) -> u64 {
        self.0.count
    }

    pub fn max_definite(&self) -> Option<u64> {
        self.0.max_definite
    }

    /// Configure the limit above which indefinite size encoding will be used.
    ///
    /// The default is 255, which is the largest size up to which definite size is at least as
    /// compact as indefinite size. Set to 23 to avoid moving bytes around when finishing the array.
    /// Set to `None` to always use indefinite size encoding.
    pub fn set_max_definite_size(&mut self, size: Option<u64>) {
        self.0.max_definite = size;
    }

    /// Add one key–value pair to the dict.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer, Encoder};
    /// let cbor = CborBuilder::new().encode_dict(|builder| {
    ///     builder.with_key("the answer", |b| b.encode_u64(42));
    /// });
    /// ```
    pub fn with_key(
        &mut self,
        key: &str,
        f: impl FnOnce(SingleBuilder<'_, '_>) -> SingleResult,
    ) -> &mut Self {
        self.with_cbor_key(|b| b.write_str(key, None), f)
    }

    pub fn with_cbor_key(
        &mut self,
        k: impl FnOnce(SingleBuilder<'_, '_>) -> SingleResult,
        v: impl FnOnce(SingleBuilder<'_, '_>) -> SingleResult,
    ) -> &mut Self {
        k(SingleBuilder(&mut self.0.non_tracking(self.0.max_definite)));
        v(SingleBuilder(&mut self.0));
        self
    }

    pub fn try_write_pair<E>(
        &mut self,
        f: impl FnOnce(KeyBuilder<'_, '_>) -> Result<SingleResult, E>,
    ) -> Result<&mut Self, E> {
        f(KeyBuilder(&mut self.0))?;
        Ok(self)
    }
}

/// Builder for the first step of [`try_write_pair`](struct.DictWriter.html#method.try_write_pair)
///
/// This builder can be used for exactly one item (which may be a complex one, like an array)
/// and returns a [`SingleBuilder`](struct.SingleBuilder.html) that needs to be used to write
/// the value for this dictionary entry.
pub struct KeyBuilder<'a, 'b>(&'b mut ArrayWriter<'a>);

impl<'a, 'b> Writer for KeyBuilder<'a, 'b> {
    type Output = SingleBuilder<'a, 'b>;

    fn bytes<T>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> T) -> T {
        self.0.non_tracking(self.0.max_definite).bytes(f)
    }

    fn into_output(self) -> Self::Output {
        SingleBuilder(self.0)
    }

    fn max_definite(&self) -> Option<u64> {
        self.0.max_definite
    }
}

/// Builder for the single value of a dict key.
///
/// This builder can be used for exactly one item (which may be a complex one, like an array)
/// and returns a [`SingleResult`](struct.SingleResult.html) to prove to its
/// [`DictWriter`](struct.DictWriter.html) that it has been used.
pub struct SingleBuilder<'a, 'b>(&'b mut ArrayWriter<'a>);

/// Result value of using a [`SingleBuilder`](struct.SingleBuilder.html) proving that it has been used.
///
/// This value needs to be returned to [`DictWriter.with_key()`](struct.DictWriter.html#method.with_key).
/// You can only obtain it by using the `SingleBuilder`.
pub struct SingleResult {
    ph: PhantomData<u8>,
}

impl<'a, 'b> Writer for SingleBuilder<'a, 'b> {
    type Output = SingleResult;

    fn bytes<T>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> T) -> T {
        self.0.bytes(f)
    }

    fn into_output(self) -> Self::Output {
        SingleResult { ph: PhantomData }
    }

    fn max_definite(&self) -> Option<u64> {
        self.0.max_definite
    }
}
