use std::marker::PhantomData;

use crate::{canonical::canonicalise, constants::*, reader::Literal, CborOwned};

enum Bytes<'a> {
    Owned(Vec<u8>),
    Borrowed(&'a mut Vec<u8>),
}

impl<'a> Bytes<'a> {
    pub fn copy(&mut self) -> Bytes {
        Bytes::Borrowed(self.as_mut())
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            Bytes::Owned(b) => b.as_slice(),
            Bytes::Borrowed(b) => b.as_slice(),
        }
    }

    pub fn as_mut(&mut self) -> &mut Vec<u8> {
        match self {
            Bytes::Owned(b) => b,
            Bytes::Borrowed(b) => *b,
        }
    }
}

/// High-level encoding functions to write values in their canonical format.
///
/// ```
/// use cbor_data::{CborBuilder, Encoder, Writer};
///
/// let cbor = CborBuilder::default().encode_u64(12);
///
/// let (array, ret) = CborBuilder::default().encode_array(|builder| {
///     builder
///         .encode_u64(18)
///         .encode_i64(-12);
///     42
/// });
/// assert_eq!(ret, 42);
///
/// let array2 = CborBuilder::default().with_max_definite_size(Some(1)).write_array(None, |builder| {
///     builder
///         .encode_u64(18)
///         .encode_i64(-12);
/// });
///
/// let dict = CborBuilder::default().encode_dict(|builder| {
///     builder
///         .with_key("a", |b| b.encode_u64(14))
///         .with_key("b", |b| b.encode_i64(-1));
/// }).0;
///
/// let (dict2, ret) = CborBuilder::default().write_dict_ret(None, |builder| {
///     builder
///         .with_key("a", |b| b.encode_u64(14))
///         .with_key("b", |b| b.encode_i64(-1));
///     "hello"
/// });
/// assert_eq!(ret, "hello");
///
/// # assert_eq!(cbor.as_slice(), vec![0x0cu8]);
/// # assert_eq!(array.as_slice(), vec![0x82u8, 0x12, 0x2b]);
/// # assert_eq!(array2.as_slice(), vec![0x9fu8, 0x12, 0x2b, 0xff]);
/// # assert_eq!(dict.as_slice(), vec![0xa2u8, 0x61, b'a', 0x0e, 0x61, b'b', 0x20]);
/// # assert_eq!(dict2.as_slice(), vec![0xa2u8, 0x61, b'a', 0x0e, 0x61, b'b', 0x20]);
/// ```
pub trait Encoder: Writer {
    /// Encode an unsigned integer of at most 64 bit.
    ///
    /// Also to be used for smaller unsigned integers:
    ///
    /// ```
    /// use cbor_data::{CborBuilder, Encoder};
    ///
    /// let short = 12345u16;
    /// let cbor = CborBuilder::default().encode_u64(short.into());
    ///
    /// # assert_eq!(cbor.as_slice(), vec![0x19u8, 48, 57]);
    /// ```
    fn encode_u64(self, value: u64) -> Self::Output {
        self.write_pos(value, None)
    }

    /// Encode a signed integer of at most 64 bit.
    ///
    /// Also to be used for smaller signed integers:
    ///
    /// ```
    /// use cbor_data::{CborBuilder, Encoder};
    ///
    /// let short = -12345i16;
    /// let cbor = CborBuilder::default().encode_i64(short.into());
    ///
    /// # assert_eq!(cbor.as_slice(), vec![0x39u8, 48, 56]);
    /// ```
    fn encode_i64(self, value: i64) -> Self::Output {
        if value < 0 {
            self.write_neg((-1 - value) as u64, None)
        } else {
            self.write_pos(value as u64, None)
        }
    }

    /// Encode a floating-point number of at most 64 bit.
    ///
    /// Also to be used for smaller formats:
    ///
    /// ```
    /// use cbor_data::{CborBuilder, Encoder};
    ///
    /// let single = -3.14f32;
    /// let cbor = CborBuilder::default().encode_f64(single.into());
    ///
    /// # assert_eq!(cbor.as_slice(), vec![0xfbu8, 192, 9, 30, 184, 96, 0, 0, 0]);
    /// ```
    fn encode_f64(self, value: f64) -> Self::Output {
        self.write_lit(Literal::L8(value.to_bits()), None)
    }

    /// Write an array that is then filled by the provided closure using the passed builder.
    ///
    /// see [`trait Encoder`](trait.Encoder) for usage examples
    fn encode_array<F, T>(self, mut f: F) -> (Self::Output, T)
    where
        F: FnMut(&mut ArrayWriter<'_>) -> T,
    {
        self.write_array_ret(None, |builder| f(builder))
    }

    /// Write a dict that is then filled by the provided closure using the passed builder.
    ///
    /// see [`trait Encoder`](trait.Encoder) for usage examples
    fn encode_dict<F, T>(self, mut f: F) -> (Self::Output, T)
    where
        F: FnMut(&mut DictWriter<'_>) -> T,
    {
        self.write_dict_ret(None, |builder| f(builder))
    }
}

impl<T: Writer> Encoder for T {}

/// Low-level primitives for emitting CBOR items.
///
/// The methods of this trait give you full control over the encoding of values according to the
/// CBOR specification (apart from the technically allowed non-optimal integer encodings). It
/// allows you to emit any item tagged with any number you desire.
///
/// If you are looking for convenient methods of writing end-user data types please refer to
/// the [`Encoder`](trait.Encoder) trait.
pub trait Writer: Sized {
    type Output;
    #[doc(hidden)]
    // internal helper method — do not use!
    /// contract: each call to this method MUST corresopnd to a single CBOR item being written!
    fn bytes<T>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> T) -> T;
    #[doc(hidden)]
    // internal helper method — do not use!
    fn to_output(self) -> Self::Output;

    /// Configured maximum array or dict length up to which definite size encoding is used.
    fn max_definite(&self) -> Option<u64>;

    /// Write a unsigned value of up to 64 bits.
    fn write_pos(mut self, value: u64, tag: Option<u64>) -> Self::Output {
        self.bytes(|b| write_positive(b, value, tag));
        self.to_output()
    }

    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    fn write_neg(mut self, value: u64, tag: Option<u64>) -> Self::Output {
        self.bytes(|b| write_neg(b, value, tag));
        self.to_output()
    }

    /// Write the given slice as a definite size byte string.
    fn write_bytes(mut self, value: &[u8], tag: Option<u64>) -> Self::Output {
        self.bytes(|b| write_bytes(b, value, tag));
        self.to_output()
    }

    /// Write the given slice as a definite size string.
    fn write_str(mut self, value: &str, tag: Option<u64>) -> Self::Output {
        self.bytes(|b| write_str(b, value, tag));
        self.to_output()
    }

    fn write_bool(mut self, value: bool, tag: Option<u64>) -> Self::Output {
        self.bytes(|b| write_bool(b, value, tag));
        self.to_output()
    }

    fn write_null(mut self, tag: Option<u64>) -> Self::Output {
        self.bytes(|b| write_null(b, tag));
        self.to_output()
    }

    fn write_undefined(mut self, tag: Option<u64>) -> Self::Output {
        self.bytes(|b| write_undefined(b, tag));
        self.to_output()
    }

    /// Write custom literal value — [RFC 7049 §2.3](https://tools.ietf.org/html/rfc7049#section-2.3) is required reading.
    fn write_lit(mut self, value: Literal, tag: Option<u64>) -> Self::Output {
        self.bytes(|b| {
            write_tag(b, tag);
            write_lit(b, value)
        });
        self.to_output()
    }

    /// Write a nested array using the given closure that receives an array builder.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer};
    /// let cbor = CborBuilder::default().write_array(None, |builder| {
    ///     builder.write_array_ret(None, |mut builder| {
    ///         builder.write_pos(42, None);
    ///     });
    /// });
    /// # assert_eq!(cbor.as_slice(), vec![0x81u8, 0x81, 0x18, 42]);
    /// ```
    fn write_array<F>(self, tag: Option<u64>, f: F) -> Self::Output
    where
        F: FnOnce(&mut ArrayWriter<'_>),
    {
        self.write_array_ret(tag, f).0
    }

    /// Write a nested array using the given closure that receives an array builder.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer};
    /// let (cbor, ret) = CborBuilder::default().write_array_ret(None, |builder| {
    ///     builder.write_array_ret(None, |mut builder| {
    ///         builder.write_pos(42, None);
    ///     });
    ///     42
    /// });
    /// assert_eq!(ret, 42);
    /// # assert_eq!(cbor.as_slice(), vec![0x81u8, 0x81, 0x18, 42]);
    /// ```
    fn write_array_ret<T, F>(mut self, tag: Option<u64>, f: F) -> (Self::Output, T)
    where
        F: FnOnce(&mut ArrayWriter<'_>) -> T,
    {
        let max_definite = self.max_definite();
        let ret = self.bytes(|b| {
            write_tag(b, tag);
            let pos = b.len();
            write_indefinite(b, MAJOR_ARRAY);
            let mut writer = ArrayWriter::new(b, max_definite);
            let ret = f(&mut writer);
            let max_definite = writer.max_definite;
            finish_array(writer.count, b, pos, MAJOR_ARRAY, max_definite);
            ret
        });
        (self.to_output(), ret)
    }

    /// Write a nested dict using the given closure that receives a dict builder.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer};
    /// let cbor = CborBuilder::default().write_array(None, |builder | {
    ///     builder.write_dict_ret(None, |mut builder| {
    ///         builder.with_key("y", |b| b.write_pos(42, None));
    ///     });
    /// });
    /// # assert_eq!(cbor.as_slice(), vec![0x81u8, 0xa1, 0x61, b'y', 0x18, 42]);
    /// ```
    fn write_dict<F>(self, tag: Option<u64>, f: F) -> Self::Output
    where
        F: FnOnce(&mut DictWriter<'_>),
    {
        self.write_dict_ret(tag, f).0
    }

    /// Write a nested dict using the given closure that receives a dict builder.
    ///
    /// ```
    /// # use cbor_data::{CborBuilder, Writer};
    /// let (cbor, ret) = CborBuilder::default().write_array_ret(None, |builder | {
    ///     builder.write_dict_ret(None, |mut builder| {
    ///         builder.with_key("y", |b| b.write_pos(42, None));
    ///     });
    ///     42
    /// });
    /// assert_eq!(ret, 42);
    /// # assert_eq!(cbor.as_slice(), vec![0x81u8, 0xa1, 0x61, b'y', 0x18, 42]);
    /// ```
    fn write_dict_ret<T, F>(mut self, tag: Option<u64>, f: F) -> (Self::Output, T)
    where
        F: FnOnce(&mut DictWriter<'_>) -> T,
    {
        let max_definite = self.max_definite();
        let ret = self.bytes(|b| {
            write_tag(b, tag);
            let pos = b.len();
            write_indefinite(b, MAJOR_DICT);
            let mut writer = DictWriter::new(b, max_definite);
            let ret = f(&mut writer);
            let max_definite = writer.0.max_definite;
            finish_array(writer.0.count, b, pos, MAJOR_DICT, max_definite);
            ret
        });
        (self.to_output(), ret)
    }

    /// Interpret the given bytes as a single CBOR item and write it to this builder,
    /// canonicalising its contents like [`CborOwned::canonical()`](struct.CborOwned#method.canonical)
    fn write_canonical(mut self, bytes: &[u8]) -> Option<Self::Output> {
        let max_definite = self.max_definite();
        let c = self.bytes(|b| {
            canonicalise(
                bytes,
                CborBuilder::append_to(b).with_max_definite_size(max_definite),
            )
        });
        if c.is_some() {
            Some(self.to_output())
        } else {
            None
        }
    }

    /// Assume that the given bytes are a well-formed single CBOR item and write it to this builder.
    ///
    /// If those bytes are not valid CBOR you get to keep the pieces!
    fn write_trusting(mut self, bytes: &[u8]) -> Self::Output {
        self.bytes(|b| b.extend_from_slice(bytes));
        self.to_output()
    }
}

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
        CborOwned::trusting(bytes)
    }
}
/// Marker type for builders that only append to a provided vector
pub struct NoOutput;
impl CborOutput for NoOutput {
    type Output = ();
    fn output(_bytes: &[u8]) -> Self::Output {}
}

/// Builder for a single CBOR value.
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

    fn to_output(self) -> Self::Output {
        O::output(self.bytes.as_slice())
    }

    fn max_definite(&self) -> Option<u64> {
        self.max_definite
    }
}

/// Builder for an array value, used by `write_array_ret()`.
///
/// see [`trait Encoder`](trait.Encoder) for usage examples
pub struct ArrayWriter<'a> {
    bytes: Bytes<'a>,
    count: u64,
    max_definite: Option<u64>,
}

impl<'a> ArrayWriter<'a> {
    fn new(bytes: &'a mut Vec<u8>, max_definite: Option<u64>) -> Self {
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
}

impl<'a> Writer for ArrayWriter<'a> {
    type Output = Self;

    fn bytes<T>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> T) -> T {
        self.count += 1;
        f(self.bytes.as_mut())
    }

    fn to_output(self) -> Self::Output {
        self
    }

    fn max_definite(&self) -> Option<u64> {
        self.max_definite
    }
}

/// Builder for a dict value, used by `write_dict_rec()`.
///
/// see [`trait Encoder`](trait.Encoder) for usage examples
pub struct DictWriter<'a>(ArrayWriter<'a>);

impl<'a> DictWriter<'a> {
    fn new(bytes: &'a mut Vec<u8>, max_definite: Option<u64>) -> Self {
        Self(ArrayWriter::new(bytes, max_definite))
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
        self.0
            .non_tracking(self.0.max_definite)
            .write_str(key, None);
        f(SingleBuilder(&mut self.0));
        self
    }
}

/// Builder for the single value of a dict key.
///
/// This builder can be used for exactly one item (which may be a complex one, like an array)
/// and returns a [`SingleResult`](struct.SingleResult) to prove to its
/// [`DictWriter`](struct.DictWriter) that it has been used.
pub struct SingleBuilder<'a, 'b>(&'b mut ArrayWriter<'a>);

/// Result value of using a [`SingleBuilder`](struct.SingleBuilder) proving that it has been used.
///
/// This value needs to be returned to [`DictWriter.with_key()`](struct.DictWriter#method.with_key).
/// You can only obtain it by using the `SingleBuilder`.
pub struct SingleResult {
    ph: PhantomData<u8>,
}

impl<'a, 'b> Writer for SingleBuilder<'a, 'b> {
    type Output = SingleResult;

    fn bytes<T>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> T) -> T {
        self.0.bytes(f)
    }

    fn to_output(self) -> Self::Output {
        SingleResult { ph: PhantomData }
    }

    fn max_definite(&self) -> Option<u64> {
        self.0.max_definite
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

    fn to_output(self) -> Self::Output {
        self
    }

    fn max_definite(&self) -> Option<u64> {
        (**self).max_definite()
    }
}

fn write_positive(bytes: &mut Vec<u8>, value: u64, tag: Option<u64>) {
    write_tag(bytes, tag);
    write_info(bytes, MAJOR_POS, value);
}

fn write_neg(bytes: &mut Vec<u8>, value: u64, tag: Option<u64>) {
    write_tag(bytes, tag);
    write_info(bytes, MAJOR_NEG, value);
}

fn write_str(bytes: &mut Vec<u8>, value: &str, tag: Option<u64>) {
    write_tag(bytes, tag);
    write_info(bytes, MAJOR_STR, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn write_bytes(bytes: &mut Vec<u8>, value: &[u8], tag: Option<u64>) {
    write_tag(bytes, tag);
    write_info(bytes, MAJOR_BYTES, value.len() as u64);
    bytes.extend_from_slice(value);
}

fn write_bool(bytes: &mut Vec<u8>, value: bool, tag: Option<u64>) {
    write_tag(bytes, tag);
    write_info(
        bytes,
        MAJOR_LIT,
        if value {
            LIT_TRUE.into()
        } else {
            LIT_FALSE.into()
        },
    );
}

fn write_null(bytes: &mut Vec<u8>, tag: Option<u64>) {
    write_tag(bytes, tag);
    write_info(bytes, MAJOR_LIT, LIT_NULL.into());
}

fn write_undefined(bytes: &mut Vec<u8>, tag: Option<u64>) {
    write_tag(bytes, tag);
    write_info(bytes, MAJOR_LIT, LIT_UNDEFINED.into());
}

fn write_tag(bytes: &mut Vec<u8>, tag: Option<u64>) {
    if let Some(tag) = tag {
        write_info(bytes, MAJOR_TAG, tag);
    }
}

fn write_info(bytes: &mut Vec<u8>, major: u8, value: u64) -> usize {
    if value < 24 {
        bytes.push(major << 5 | (value as u8));
        1
    } else if value < 0x100 {
        bytes.push(major << 5 | 24);
        bytes.push(value as u8);
        2
    } else if value < 0x1_0000 {
        bytes.push(major << 5 | 25);
        bytes.push((value >> 8) as u8);
        bytes.push(value as u8);
        3
    } else if value < 0x1_0000_0000 {
        bytes.push(major << 5 | 26);
        bytes.push((value >> 24) as u8);
        bytes.push((value >> 16) as u8);
        bytes.push((value >> 8) as u8);
        bytes.push(value as u8);
        5
    } else {
        bytes.push(major << 5 | 27);
        bytes.push((value >> 56) as u8);
        bytes.push((value >> 48) as u8);
        bytes.push((value >> 40) as u8);
        bytes.push((value >> 32) as u8);
        bytes.push((value >> 24) as u8);
        bytes.push((value >> 16) as u8);
        bytes.push((value >> 8) as u8);
        bytes.push(value as u8);
        9
    }
}

fn write_lit(bytes: &mut Vec<u8>, value: Literal) {
    match value {
        Literal::L0(v) => bytes.push(MAJOR_LIT << 5 | v),
        Literal::L1(v) => {
            bytes.push(MAJOR_LIT << 5 | 24);
            bytes.push(v);
        }
        Literal::L2(v) => {
            bytes.push(MAJOR_LIT << 5 | 25);
            bytes.push((v >> 8) as u8);
            bytes.push(v as u8);
        }
        Literal::L4(v) => {
            bytes.push(MAJOR_LIT << 5 | 26);
            bytes.push((v >> 24) as u8);
            bytes.push((v >> 16) as u8);
            bytes.push((v >> 8) as u8);
            bytes.push(v as u8);
        }
        Literal::L8(v) => {
            bytes.push(MAJOR_LIT << 5 | 27);
            bytes.push((v >> 56) as u8);
            bytes.push((v >> 48) as u8);
            bytes.push((v >> 40) as u8);
            bytes.push((v >> 32) as u8);
            bytes.push((v >> 24) as u8);
            bytes.push((v >> 16) as u8);
            bytes.push((v >> 8) as u8);
            bytes.push(v as u8);
        }
    }
}

fn write_indefinite(bytes: &mut Vec<u8>, major: u8) {
    bytes.push(major << 5 | INDEFINITE_SIZE);
}

fn finish_array(count: u64, b: &mut Vec<u8>, pos: usize, major: u8, max_definite: Option<u64>) {
    if Some(count) > max_definite {
        // indefinite encoding saves bytes here
        b.push(STOP_BYTE);
    } else {
        // otherwise prefer definite encoding
        let end = b.len();
        // use main vector as scratch space, will clean up below
        let head_len = write_info(b, major, count);
        if head_len > 1 {
            // save header bytes onto stack
            let mut buf = [0u8; 9];
            let buf = &mut buf[0..head_len];
            buf.copy_from_slice(&b[end..]);
            // need to shift back the array contents to make room for longer header
            let to_move = pos + 1..end;
            let new_start = pos + head_len;
            b.copy_within(to_move, new_start);
            // now put the new header in place
            b[pos..new_start].copy_from_slice(buf);
        } else {
            b[pos] = b[end];
        }
        // written header included the `pos` byte, so clean up
        b.pop();
    }
}
