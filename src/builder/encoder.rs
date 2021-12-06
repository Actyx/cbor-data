use crate::{
    value::{Number, Precision, Timestamp},
    ArrayWriter, DictWriter, Literal, Writer,
};

/// High-level encoding functions to write values in their canonical format.
///
/// ```
/// use cbor_data::{CborBuilder, Encoder, Writer};
///
/// let cbor = CborBuilder::default().encode_u64(12);
///
/// let array = CborBuilder::default().encode_array(|builder| {
///     builder
///         .encode_u64(18)
///         .encode_i64(-12);
/// });
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
/// });
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
    fn encode_undefined(self) -> Self::Output {
        self.write_undefined(None)
    }

    fn encode_null(self) -> Self::Output {
        self.write_null(None)
    }

    fn encode_bool(self, value: bool) -> Self::Output {
        self.write_bool(value, None)
    }

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

    /// Encode a timestamp with given target precision
    ///
    /// Since the CBOR-encoding itself does not carry precision information, the result
    /// is not guaranteed to round-trip as the exact same timestamp. Encoding with `Precision::Seconds`
    /// will discard the `nanos()` part.
    ///
    /// If the `rfc3339` feature flag is enabled, textual representation is chosen for
    /// subsecond precision when encoding as a double-precision floating-point number would
    /// not be enough (float is sufficient for 285 years around 1970 at microsecond precision).
    /// Textual representation retains timezone information in the output.
    fn encode_timestamp(self, timestamp: Timestamp, precision: Precision) -> Self::Output {
        timestamp.encode(self, precision)
    }

    /// Encode a possibly big number
    ///
    /// The number will be encoded as simple integer or float if its mantissa is small enough.
    fn encode_number(self, number: &Number) -> Self::Output {
        number.encode(self)
    }

    /// Encode a string.
    ///
    /// ```
    /// use cbor_data::{CborBuilder, Encoder};
    ///
    /// let cbor = CborBuilder::default().encode_array(|builder| {
    ///     builder.encode_str("hello");
    ///     builder.encode_str(String::new());
    /// });
    ///
    /// # assert_eq!(cbor.as_slice(), vec![0x82, 0x65, b'h', b'e', b'l', b'l', b'o', 0x60]);
    /// ```
    fn encode_str(self, value: impl AsRef<str>) -> Self::Output {
        self.write_str(value.as_ref(), None)
    }

    /// Encode a byte string.
    ///
    /// ```
    /// use cbor_data::{CborBuilder, Encoder};
    ///
    /// let cbor = CborBuilder::default().encode_array(|builder| {
    ///     builder.encode_bytes(b"hello");
    /// });
    ///
    /// # assert_eq!(cbor.as_slice(), vec![0x81, 0x45, b'h', b'e', b'l', b'l', b'o']);
    /// ```
    fn encode_bytes(self, value: impl AsRef<[u8]>) -> Self::Output {
        self.write_bytes(value.as_ref(), None)
    }

    /// Write an array that is then filled by the provided closure using the passed builder.
    ///
    /// see [`trait Encoder`](trait.Encoder.html) for usage examples
    fn encode_array<F>(self, mut f: F) -> Self::Output
    where
        F: FnMut(&mut ArrayWriter<'_>),
    {
        self.write_array(None, |builder| f(builder))
    }

    /// Write a dict that is then filled by the provided closure using the passed builder.
    ///
    /// see [`trait Encoder`](trait.Encoder.html) for usage examples
    fn encode_dict<F>(self, mut f: F) -> Self::Output
    where
        F: FnMut(&mut DictWriter<'_>),
    {
        self.write_dict(None, |builder| f(builder))
    }
}

impl<T: Writer> Encoder for T {}
