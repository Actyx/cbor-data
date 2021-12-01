use crate::CborOwned;
use std::marker::PhantomData;

mod encoder;
mod low_level;
mod writer;
mod writers;

pub use encoder::Encoder;
use low_level::*;
pub use writer::Writer;
pub use writers::{ArrayWriter, DictWriter, KeyBuilder, SingleBuilder, SingleResult};

/// Marker trait to distinguish a builder that emits an owned value from one that appends to a vector
pub trait CborOutput {
    type Output;
    fn output(bytes: &[u8]) -> Self::Output;
}
/// Marker type for builders that emit an owned value
pub struct WithOutput;
impl CborOutput for WithOutput {
    type Output = CborOwned;
    fn output(bytes: &[u8]) -> Self::Output {
        CborOwned::unchecked(bytes)
    }
}
/// Marker type for builders that only append to a provided vector
pub struct NoOutput;
impl CborOutput for NoOutput {
    type Output = ();
    fn output(_bytes: &[u8]) -> Self::Output {}
}

/// Builder for a single CBOR value.
///
/// [`CborOwned::canonical`](struct.CborOwned.html#method.canonical) uses the default configuration,
/// which implies writing bytes into a fresh `Vec<u8>` and finally moving them into a SmallVec. You
/// can minimise allocations by reusing the build buffer, and you can influence whether definite or
/// indefinite length encoding is used for arrays and dictionaries.
///
/// # Example
///
/// ```rust
/// use cbor_data::{Cbor, CborBuilder, Writer};
///
/// // this could come from a thread-local in real code:
/// let mut build_buffer = Vec::new();
/// // buffer will be cleared before use
/// build_buffer.extend_from_slice(b"some garbage");
///
/// let bytes_from_elsewhere = [0x82, 1, 2];
/// assert_eq!(Cbor::checked(&bytes_from_elsewhere).unwrap().to_string(), "[1, 2]");
///
/// let cbor = CborBuilder::with_scratch_space(&mut build_buffer)
///     .with_max_definite_size(Some(1))
///     .write_canonical(bytes_from_elsewhere.as_ref())
///     .unwrap();
///
/// // now it is using indefinite-length encoding, since the array has more than 1 item
/// assert_eq!(cbor.to_string(), "[_ 1, 2]");
/// assert_eq!(cbor.as_slice(), [0x9f, 1, 2, 0xff]);
/// ```
pub struct CborBuilder<'a, O: CborOutput> {
    bytes: Bytes<'a>,
    max_definite: Option<u64>,
    ph: PhantomData<O>,
}

impl Default for CborBuilder<'static, WithOutput> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> CborBuilder<'a, WithOutput> {
    /// Create a builder that writes into its own fresh vector.
    pub fn new() -> Self {
        Self {
            bytes: Bytes::Owned(Vec::new()),
            max_definite: Some(255),
            ph: PhantomData,
        }
    }

    /// Create a builder that clears the given vector and writes into it.
    ///
    /// You can use this to reuse a scratch space across multiple values being built, e.g. by
    /// keeping the same vector in a thread-local variable.
    pub fn with_scratch_space(v: &'a mut Vec<u8>) -> Self {
        v.clear();
        Self {
            bytes: Bytes::Borrowed(v),
            max_definite: Some(255),
            ph: PhantomData,
        }
    }
}

impl<'a> CborBuilder<'a, NoOutput> {
    /// Append the CBOR bytes to the given vector and do not return a separate output value.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer};
    /// let mut v = Vec::new();
    /// let result: () = CborBuilder::append_to(&mut v).write_pos(12, None);
    ///
    /// assert_eq!(v, vec![12u8])
    /// ```
    pub fn append_to(v: &'a mut Vec<u8>) -> Self {
        Self {
            bytes: Bytes::Borrowed(v),
            max_definite: Some(255),
            ph: PhantomData,
        }
    }
}

impl<'a, O: CborOutput> CborBuilder<'a, O> {
    /// Configure the limit above which indefinite size encoding will be used.
    ///
    /// The default is 255, which is the largest size up to which definite size is at least as
    /// compact as indefinite size. Set to 23 to avoid moving bytes around when finishing the array.
    /// Set to `None` to always use indefinite size encoding.
    pub fn with_max_definite_size(self, max_definite: Option<u64>) -> Self {
        Self {
            bytes: self.bytes,
            max_definite,
            ph: PhantomData,
        }
    }
}

impl<'a, O: CborOutput> Writer for CborBuilder<'a, O> {
    type Output = O::Output;

    fn bytes<T>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> T) -> T {
        f(self.bytes.as_mut())
    }

    fn into_output(self) -> Self::Output {
        O::output(self.bytes.as_slice())
    }

    fn max_definite(&self) -> Option<u64> {
        self.max_definite
    }
}
