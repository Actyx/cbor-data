use crate::{constants::*, reader::Literal, CborOwned};

enum Bytes<'a> {
    Owned(Vec<u8>),
    Borrowed(&'a mut Vec<u8>),
}

impl<'a> Bytes<'a> {
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

/// Builder for a single CBOR value.
pub struct CborBuilder<'a>(Bytes<'a>);

/// Builder for an array value, used by `write_array()`.
///
/// Calling the [`finish()`](#method.finish) method will return either the fully constructed
/// CBOR value (if this was the top-level array) or the builder of the array or dict into
/// which this array was placed.
///
/// If you want to recursively create a CBOR structure without statically known recursion limit
/// then you’ll want to take a look at the [`WriteToArray::write_array_rec()`](trait.WriteToArray#tymethod.write_array_rec)
/// method (the compiler would otherwise kindly inform you of a type expansion hitting the recursion
/// limit while instantiating your recursive function).
///
/// see [`trait Encoder`](trait.Encoder) for usage examples
pub struct ArrayBuilder<'a, T>(Bytes<'a>, Box<dyn FnOnce(Bytes<'a>) -> T + 'a>);

/// Builder for a dict value, used by `write_dict()`.
///
/// Calling the [`finish()`](#method.finish) method will return either the fully constructed
/// CBOR value (if this was the top-level dict) or the builder of the array or dict into
/// which this dict was placed.
///
/// If you want to recursively create a CBOR structure without statically known recursion limit
/// then you’ll want to take a look at the [`WriteToDict::write_dict_rec()`](trait.WriteToDict#tymethod.write_dict_rec)
/// method (the compiler would otherwise kindly inform you of a type expansion hitting the recursion
/// limit while instantiating your recursive function).
///
/// see [`trait Encoder`](trait.Encoder) for usage examples
pub struct DictBuilder<'a, T>(Bytes<'a>, Box<dyn FnOnce(Bytes<'a>) -> T + 'a>);

fn finish_cbor(v: Bytes<'_>) -> CborOwned {
    CborOwned::trusting(v.as_slice())
}

impl Default for CborBuilder<'static> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> CborBuilder<'a> {
    /// Create a builder that writes into its own fresh vector.
    pub fn new() -> Self {
        Self(Bytes::Owned(Vec::new()))
    }

    /// Create a builder that clears the given vector and writes into it.
    ///
    /// You can use this to reuse a scratch space across multiple values being built, e.g. by
    /// keeping the same vector in a thread-local variable.
    pub fn with_scratch_space(v: &'a mut Vec<u8>) -> Self {
        v.clear();
        Self(Bytes::Borrowed(v))
    }

    /// Write a unsigned value of up to 64 bits.
    pub fn write_pos(mut self, value: u64, tag: Option<u64>) -> CborOwned {
        ArrayWriter::from(self.0.as_mut()).write_pos(value, tag);
        finish_cbor(self.0)
    }

    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    pub fn write_neg(mut self, value: u64, tag: Option<u64>) -> CborOwned {
        ArrayWriter::from(self.0.as_mut()).write_neg(value, tag);
        finish_cbor(self.0)
    }

    /// Write the given slice as a definite size byte string.
    pub fn write_bytes(mut self, value: &[u8], tag: Option<u64>) -> CborOwned {
        ArrayWriter::from(self.0.as_mut()).write_bytes(value, tag);
        finish_cbor(self.0)
    }

    /// Write the given slice as a definite size string.
    pub fn write_str(mut self, value: &str, tag: Option<u64>) -> CborOwned {
        ArrayWriter::from(self.0.as_mut()).write_str(value, tag);
        finish_cbor(self.0)
    }

    pub fn write_bool(mut self, value: bool, tag: Option<u64>) -> CborOwned {
        ArrayWriter::from(self.0.as_mut()).write_bool(value, tag);
        finish_cbor(self.0)
    }

    pub fn write_null(mut self, tag: Option<u64>) -> CborOwned {
        ArrayWriter::from(self.0.as_mut()).write_null(tag);
        finish_cbor(self.0)
    }

    pub fn write_undefined(mut self, tag: Option<u64>) -> CborOwned {
        ArrayWriter::from(self.0.as_mut()).write_undefined(tag);
        finish_cbor(self.0)
    }

    /// Write custom literal value — [RFC 7049 §2.3](https://tools.ietf.org/html/rfc7049#section-2.3) is required reading.
    pub fn write_lit(mut self, value: Literal, tag: Option<u64>) -> CborOwned {
        ArrayWriter::from(self.0.as_mut()).write_lit(value, tag);
        finish_cbor(self.0)
    }

    /// Write an array that is then filled by the returned builder.
    ///
    /// see [`trait Encoder`](trait.Encoder) for usage examples
    pub fn write_array(mut self, tag: Option<u64>) -> ArrayBuilder<'a, CborOwned> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_ARRAY);
        ArrayBuilder(self.0, Box::new(finish_cbor))
    }

    /// Write an array that is then filled by the provided closure using the passed builder.
    ///
    /// see [`trait Encoder`](trait.Encoder) for usage examples
    pub fn write_array_rec<F, T>(mut self, tag: Option<u64>, f: F) -> (CborOwned, T)
    where
        F: FnMut(ArrayWriter<'_>) -> T,
    {
        let ret = ArrayWriter::from(self.0.as_mut()).write_array_rec(tag, f);
        (finish_cbor(self.0), ret)
    }

    /// Write a dict that is then filled by the returned builder.
    ///
    /// see [`trait Encoder`](trait.Encoder) for usage examples
    pub fn write_dict(mut self, tag: Option<u64>) -> DictBuilder<'a, CborOwned> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_DICT);
        DictBuilder(self.0, Box::new(finish_cbor))
    }

    /// Write a dict that is then filled by the provided closure using the passed builder.
    ///
    /// see [`trait Encoder`](trait.Encoder) for usage examples
    pub fn write_dict_rec<F, T>(mut self, tag: Option<u64>, f: F) -> (CborOwned, T)
    where
        F: FnMut(DictWriter<'_>) -> T,
    {
        let ret = ArrayWriter::from(self.0.as_mut()).write_dict_rec(tag, f);
        (finish_cbor(self.0), ret)
    }

    /// Write an array that is then filled by the provided closure using the passed builder.
    ///
    /// see [`trait Encoder`](trait.Encoder) for usage examples
    pub fn encode_array<F>(self, mut f: F) -> CborOwned
    where
        F: FnMut(ArrayWriter<'_>),
    {
        self.write_array_rec(None, |builder| f(builder)).0
    }

    /// Write a dict that is then filled by the provided closure using the passed builder.
    ///
    /// see [`trait Encoder`](trait.Encoder) for usage examples
    pub fn encode_dict<F>(self, mut f: F) -> CborOwned
    where
        F: FnMut(DictWriter<'_>),
    {
        self.write_dict_rec(None, |builder| f(builder)).0
    }
}

/// Builder for an array value, used by `write_array_rec()`.
///
/// see [`trait Encoder`](trait.Encoder) for usage examples
pub struct ArrayWriter<'a>(Bytes<'a>);

impl<'a> From<&'a mut Vec<u8>> for ArrayWriter<'a> {
    fn from(v: &'a mut Vec<u8>) -> Self {
        Self(Bytes::Borrowed(v))
    }
}

impl<'a> ArrayWriter<'a> {
    /// Write a unsigned value of up to 64 bits.
    pub fn write_pos(&mut self, value: u64, tag: Option<u64>) {
        write_positive(self.0.as_mut(), value, tag);
    }

    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    pub fn write_neg(&mut self, value: u64, tag: Option<u64>) {
        write_neg(self.0.as_mut(), value, tag);
    }

    /// Write the given slice as a definite size byte string.
    pub fn write_bytes(&mut self, value: &[u8], tag: Option<u64>) {
        write_bytes(self.0.as_mut(), value, tag);
    }

    /// Write the given slice as a definite size string.
    pub fn write_str(&mut self, value: &str, tag: Option<u64>) {
        write_str(self.0.as_mut(), value, tag);
    }

    pub fn write_bool(&mut self, value: bool, tag: Option<u64>) {
        write_bool(self.0.as_mut(), value, tag);
    }

    pub fn write_null(&mut self, tag: Option<u64>) {
        write_null(self.0.as_mut(), tag);
    }

    pub fn write_undefined(&mut self, tag: Option<u64>) {
        write_undefined(self.0.as_mut(), tag);
    }

    /// Write custom literal value — [RFC 7049 §2.3](https://tools.ietf.org/html/rfc7049#section-2.3) is required reading.
    pub fn write_lit(&mut self, value: Literal, tag: Option<u64>) {
        write_tag(self.0.as_mut(), tag);
        write_lit(self.0.as_mut(), value);
    }

    /// Write a nested array that is then filled by the returned builder.
    /// You can resume building this outer array by using the [`finish`()](struct.ArrayBuilder#method.finish)
    /// method.
    pub fn write_array(mut self, tag: Option<u64>) -> ArrayBuilder<'a, Self> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_ARRAY);
        ArrayBuilder(self.0, Box::new(Self))
    }

    /// Write a nested array using the given closure that receives an array builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// # use cbor_data::CborBuilder;
    /// let (cbor, _) = CborBuilder::default().write_array_rec(None, |mut builder| {
    ///     builder.write_array_rec(None, |mut builder| {
    ///         builder.write_pos(42, None);
    ///     });
    /// });
    /// # assert_eq!(cbor.as_slice(), vec![0x9fu8, 0x9f, 0x18, 42, 0xff, 0xff]);
    /// ```
    pub fn write_array_rec<T, F>(&mut self, tag: Option<u64>, mut f: F) -> T
    where
        F: FnMut(ArrayWriter<'_>) -> T,
    {
        let v = self.0.as_mut();
        write_tag(v, tag);
        write_indefinite(v, MAJOR_ARRAY);
        let ret = f(v.into());
        v.push(STOP_BYTE);
        ret
    }

    /// Write a nested dict that is then filled by the returned builder.
    /// You can resume building this outer array by using the [`finish`()](struct.DictBuilder#method.finish)
    /// method.
    pub fn write_dict(mut self, tag: Option<u64>) -> DictBuilder<'a, Self> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_DICT);
        DictBuilder(self.0, Box::new(Self))
    }

    /// Write a nested dict using the given closure that receives an dict builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// # use cbor_data::CborBuilder;
    /// let (cbor, _) = CborBuilder::default().write_array_rec(None, |mut builder | {
    ///     builder.write_dict_rec(None, |mut builder| {
    ///         builder.write_pos("y", 42, None);
    ///     });
    /// });
    /// # assert_eq!(cbor.as_slice(), vec![0x9fu8, 0xbf, 0x61, b'y', 0x18, 42, 0xff, 0xff]);
    /// ```
    pub fn write_dict_rec<T, F>(&mut self, tag: Option<u64>, mut f: F) -> T
    where
        F: FnMut(DictWriter<'_>) -> T,
    {
        let v = self.0.as_mut();
        write_tag(v, tag);
        write_indefinite(v, MAJOR_DICT);
        let ret = f(v.into());
        v.push(STOP_BYTE);
        ret
    }
}

/// Builder for a dict value, used by `write_dict_rec()`.
///
/// see [`trait Encoder`](trait.Encoder) for usage examples
pub struct DictWriter<'a>(Bytes<'a>);

impl<'a> From<&'a mut Vec<u8>> for DictWriter<'a> {
    fn from(v: &'a mut Vec<u8>) -> Self {
        Self(Bytes::Borrowed(v))
    }
}

impl<'a> DictWriter<'a> {
    /// Write a unsigned value of up to 64 bits.
    pub fn write_pos(&mut self, key: &str, value: u64, tag: Option<u64>) {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_pos(value, tag);
    }

    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    pub fn write_neg(&mut self, key: &str, value: u64, tag: Option<u64>) {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_neg(value, tag);
    }

    /// Write the given slice as a definite size byte string.
    pub fn write_bytes(&mut self, key: &str, value: &[u8], tag: Option<u64>) {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_bytes(value, tag);
    }

    /// Write the given slice as a definite size string.
    pub fn write_str(&mut self, key: &str, value: &str, tag: Option<u64>) {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_str(value, tag);
    }

    pub fn write_bool(&mut self, key: &str, value: bool, tag: Option<u64>) {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_bool(value, tag);
    }

    pub fn write_null(&mut self, key: &str, tag: Option<u64>) {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_null(tag);
    }

    pub fn write_undefined(&mut self, key: &str, tag: Option<u64>) {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_undefined(tag);
    }

    /// Write custom literal value — [RFC 7049 §2.3](https://tools.ietf.org/html/rfc7049#section-2.3) is required reading.
    pub fn write_lit(&mut self, key: &str, value: Literal, tag: Option<u64>) {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_lit(value, tag);
    }

    /// Write a nested array that is then filled by the returned builder.
    /// You can resume building this outer dict by using the [`finish`()](struct.ArrayBuilder#method.finish)
    /// method.
    pub fn write_array(mut self, key: &str, tag: Option<u64>) -> ArrayBuilder<'a, Self> {
        write_str(self.0.as_mut(), key, None);
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_ARRAY);
        ArrayBuilder(self.0, Box::new(Self))
    }

    /// Write a nested array using the given closure that receives an array builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// # use cbor_data::CborBuilder;
    /// let (cbor, _) = CborBuilder::default().write_dict_rec(None, |mut builder| {
    ///     builder.write_array_rec("x", None, |mut builder| {
    ///         builder.write_pos(42, None);
    ///     });
    /// });
    /// # assert_eq!(cbor.as_slice(), vec![0xbfu8, 0x61, b'x', 0x9f, 0x18, 42, 0xff, 0xff]);
    /// ```
    pub fn write_array_rec<T, F>(&mut self, key: &str, tag: Option<u64>, f: F) -> T
    where
        F: FnMut(ArrayWriter<'_>) -> T,
    {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_array_rec(tag, f)
    }

    /// Write a nested dict that is then filled by the returned builder.
    /// You can resume building this outer dict by using the [`finish`()](struct.DictBuilder#method.finish)
    /// method.
    pub fn write_dict(mut self, key: &str, tag: Option<u64>) -> DictBuilder<'a, Self> {
        write_str(self.0.as_mut(), key, None);
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_DICT);
        DictBuilder(self.0, Box::new(Self))
    }

    /// Write a nested dict using the given closure that receives an dict builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// # use cbor_data::CborBuilder;
    /// let (cbor, _) = CborBuilder::default().write_dict_rec(None, |mut builder | {
    ///     builder.write_dict_rec("x", None, |mut builder| {
    ///         builder.write_pos("y", 42, None);
    ///     });
    /// });
    /// # assert_eq!(cbor.as_slice(), vec![0xbfu8, 0x61, b'x', 0xbf, 0x61, b'y', 0x18, 42, 0xff, 0xff]);
    /// ```
    pub fn write_dict_rec<T, F>(&mut self, key: &str, tag: Option<u64>, f: F) -> T
    where
        F: FnMut(DictWriter<'_>) -> T,
    {
        write_str(self.0.as_mut(), key, None);
        ArrayWriter::from(self.0.as_mut()).write_dict_rec(tag, f)
    }

    /// Use [`Encoder`](trait.Encoder) methods for writing an entry into the dictionary.
    ///
    /// ```
    /// use cbor_data::{CborBuilder, Encoder};
    ///
    /// let cbor = CborBuilder::default().encode_dict(|mut builder| {
    ///     builder
    ///         .with_key("x")
    ///         .encode_u64(25);
    /// });
    /// # assert_eq!(cbor.as_slice(), vec![0xbf, 0x61, b'x', 0x18, 25, 0xff]);
    /// ```
    pub fn with_key(self, key: &'a str) -> DictValueWriter<'a> {
        DictValueWriter(self.0, key)
    }
}

impl<'a, T: 'a> ArrayBuilder<'a, T> {
    /// Finish building this array and return to the outer context. In case of a
    /// top-level array this returns the complete [`Cbor`](struct.Cbor) value.
    pub fn finish(mut self) -> T {
        self.0.as_mut().push(STOP_BYTE);
        self.1(self.0)
    }

    /// Write a unsigned value of up to 64 bits.
    pub fn write_pos(&mut self, value: u64, tag: Option<u64>) {
        ArrayWriter::from(self.0.as_mut()).write_pos(value, tag);
    }

    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    pub fn write_neg(&mut self, value: u64, tag: Option<u64>) {
        ArrayWriter::from(self.0.as_mut()).write_neg(value, tag);
    }

    /// Write the given slice as a definite size byte string.
    pub fn write_bytes(&mut self, value: &[u8], tag: Option<u64>) {
        ArrayWriter::from(self.0.as_mut()).write_bytes(value, tag);
    }

    /// Write the given slice as a definite size string.
    pub fn write_str(&mut self, value: &str, tag: Option<u64>) {
        ArrayWriter::from(self.0.as_mut()).write_str(value, tag);
    }

    pub fn write_bool(&mut self, value: bool, tag: Option<u64>) {
        ArrayWriter::from(self.0.as_mut()).write_bool(value, tag);
    }

    pub fn write_null(&mut self, tag: Option<u64>) {
        ArrayWriter::from(self.0.as_mut()).write_null(tag);
    }

    pub fn write_undefined(&mut self, tag: Option<u64>) {
        ArrayWriter::from(self.0.as_mut()).write_undefined(tag);
    }

    /// Write custom literal value — [RFC 7049 §2.3](https://tools.ietf.org/html/rfc7049#section-2.3) is required reading.
    pub fn write_lit(&mut self, value: Literal, tag: Option<u64>) {
        ArrayWriter::from(self.0.as_mut()).write_lit(value, tag);
    }

    /// Write a nested array that is then filled by the returned builder.
    /// You can resume building this outer array by using the [`finish`()](struct.ArrayBuilder#method.finish)
    /// method.
    pub fn write_array(mut self, tag: Option<u64>) -> ArrayBuilder<'a, Self> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_ARRAY);
        let Self(bytes, f) = self;
        ArrayBuilder(bytes, Box::new(|v| Self(v, f)))
    }

    /// Write a nested array using the given closure that receives an array builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// # use cbor_data::CborBuilder;
    /// let mut cbor = CborBuilder::default().write_array(None);
    /// cbor.write_array_rec(None, |mut builder| {
    ///     builder.write_pos(42, None);
    /// });
    /// let cbor = cbor.finish();
    /// # assert_eq!(cbor.as_slice(), vec![0x9fu8, 0x9f, 0x18, 42, 0xff, 0xff]);
    /// ```
    pub fn write_array_rec<F, U>(&mut self, tag: Option<u64>, f: F) -> U
    where
        F: FnMut(ArrayWriter<'_>) -> U,
    {
        ArrayWriter::from(self.0.as_mut()).write_array_rec(tag, f)
    }

    /// Write a nested dict that is then filled by the returned builder.
    /// You can resume building this outer array by using the [`finish`()](struct.DictBuilder#method.finish)
    /// method.
    pub fn write_dict(mut self, tag: Option<u64>) -> DictBuilder<'a, Self> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_DICT);
        let Self(bytes, f) = self;
        DictBuilder(bytes, Box::new(|v| Self(v, f)))
    }

    /// Write a nested dict using the given closure that receives an dict builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// # use cbor_data::CborBuilder;
    /// let mut cbor = CborBuilder::default().write_array(None);
    /// cbor.write_dict_rec(None, |mut builder| {
    ///     builder.write_pos("n", 42, None);
    /// });
    /// let cbor = cbor.finish();
    /// # assert_eq!(cbor.as_slice(), vec![0x9fu8, 0xbf, 0x61, b'n', 0x18, 42, 0xff, 0xff]);
    /// ```
    pub fn write_dict_rec<F, U>(&mut self, tag: Option<u64>, f: F) -> U
    where
        F: FnMut(DictWriter<'_>) -> U,
    {
        ArrayWriter::from(self.0.as_mut()).write_dict_rec(tag, f)
    }
}

impl<'a, T: 'a> DictBuilder<'a, T> {
    /// Finish building this dict and return to the outer context. In case of a
    /// top-level dict this returns the complete [`Cbor`](struct.Cbor) value.
    pub fn finish(mut self) -> T {
        self.0.as_mut().push(STOP_BYTE);
        self.1(self.0)
    }

    /// Write a unsigned value of up to 64 bits.
    pub fn write_pos(&mut self, key: &str, value: u64, tag: Option<u64>) {
        DictWriter::from(self.0.as_mut()).write_pos(key, value, tag);
    }

    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    pub fn write_neg(&mut self, key: &str, value: u64, tag: Option<u64>) {
        DictWriter::from(self.0.as_mut()).write_neg(key, value, tag);
    }

    /// Write the given slice as a definite size byte string.
    pub fn write_bytes(&mut self, key: &str, value: &[u8], tag: Option<u64>) {
        DictWriter::from(self.0.as_mut()).write_bytes(key, value, tag);
    }

    /// Write the given slice as a definite size string.
    pub fn write_str(&mut self, key: &str, value: &str, tag: Option<u64>) {
        DictWriter::from(self.0.as_mut()).write_str(key, value, tag);
    }

    pub fn write_bool(&mut self, key: &str, value: bool, tag: Option<u64>) {
        DictWriter::from(self.0.as_mut()).write_bool(key, value, tag);
    }

    pub fn write_null(&mut self, key: &str, tag: Option<u64>) {
        DictWriter::from(self.0.as_mut()).write_null(key, tag);
    }

    pub fn write_undefined(&mut self, key: &str, tag: Option<u64>) {
        DictWriter::from(self.0.as_mut()).write_undefined(key, tag);
    }

    /// Write custom literal value — [RFC 7049 §2.3](https://tools.ietf.org/html/rfc7049#section-2.3) is required reading.
    pub fn write_lit(&mut self, key: &str, value: Literal, tag: Option<u64>) {
        DictWriter::from(self.0.as_mut()).write_lit(key, value, tag);
    }

    /// Write a nested array that is then filled by the returned builder.
    /// You can resume building this outer dict by using the [`finish`()](struct.ArrayBuilder#method.finish)
    /// method.
    pub fn write_array(mut self, key: &str, tag: Option<u64>) -> ArrayBuilder<'a, Self> {
        write_str(self.0.as_mut(), key, None);
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_ARRAY);
        let Self(bytes, f) = self;
        ArrayBuilder(bytes, Box::new(|v| Self(v, f)))
    }

    /// Write a nested array using the given closure that receives an array builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// # use cbor_data::CborBuilder;
    /// let mut cbor = CborBuilder::default().write_dict(None);
    /// cbor.write_array_rec("x", None, |mut builder| {
    ///     builder.write_pos(42, None);
    /// });
    /// let cbor = cbor.finish();
    /// # assert_eq!(cbor.as_slice(), vec![0xbfu8, 0x61, b'x', 0x9f, 0x18, 42, 0xff, 0xff]);
    /// ```
    pub fn write_array_rec<F, U>(&mut self, key: &str, tag: Option<u64>, f: F) -> U
    where
        F: FnMut(ArrayWriter<'_>) -> U,
    {
        DictWriter::from(self.0.as_mut()).write_array_rec(key, tag, f)
    }

    /// Write a nested dict that is then filled by the returned builder.
    /// You can resume building this outer dict by using the [`finish`()](struct.DictBuilder#method.finish)
    /// method.
    pub fn write_dict(mut self, key: &str, tag: Option<u64>) -> DictBuilder<'a, Self> {
        write_str(self.0.as_mut(), key, None);
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_DICT);
        let Self(bytes, f) = self;
        DictBuilder(bytes, Box::new(|v| Self(v, f)))
    }

    /// Write a nested dict using the given closure that receives an dict builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// # use cbor_data::CborBuilder;
    /// let mut cbor = CborBuilder::default().write_dict(None);
    /// cbor.write_dict_rec("x", None, |mut builder| {
    ///     builder.write_pos("y", 42, None);
    /// });
    /// let cbor = cbor.finish();
    /// # assert_eq!(cbor.as_slice(), vec![0xbfu8, 0x61, b'x', 0xbf, 0x61, b'y', 0x18, 42, 0xff, 0xff]);
    /// ```
    pub fn write_dict_rec<F, U>(&mut self, key: &str, tag: Option<u64>, f: F) -> U
    where
        F: FnMut(DictWriter<'_>) -> U,
    {
        DictWriter::from(self.0.as_mut()).write_dict_rec(key, tag, f)
    }

    /// Use [`Encoder`](trait.Encoder) methods for writing an entry into the dictionary.
    ///
    /// ```
    /// use cbor_data::{CborBuilder, Encoder};
    ///
    /// let cbor = CborBuilder::default()
    ///     .write_dict(None)
    ///     .with_key("x")
    ///     .encode_u64(25)
    ///     .finish();
    /// # assert_eq!(cbor.as_slice(), vec![0xbf, 0x61, b'x', 0x18, 25, 0xff]);
    /// ```
    pub fn with_key(self, key: &'a str) -> DictValueBuilder<'a, T> {
        DictValueBuilder(self.0, self.1, key)
    }
}

/// High-level encoding functions to write values in their canonical format.
///
/// ```
/// use cbor_data::{CborBuilder, Encoder};
///
/// let cbor = CborBuilder::default().encode_u64(12);
///
/// let array = CborBuilder::default().encode_array(|mut builder| {
///     builder
///         .encode_u64(18)
///         .encode_i64(-12);
/// });
///
/// let array2 = CborBuilder::default()
///     .write_array(None)
///     .encode_u64(18)
///     .encode_i64(-12)
///     .finish();
///
/// let dict = CborBuilder::default().encode_dict(|mut builder| {
///     builder
///         .with_key("a").encode_u64(14)
///         .with_key("b").encode_i64(-1);
/// });
///
/// let dict2 = CborBuilder::default()
///     .write_dict(None)
///     .with_key("a").encode_u64(14)
///     .with_key("b").encode_i64(-1)
///     .finish();
///
/// # assert_eq!(cbor.as_slice(), vec![0x0cu8]);
/// # assert_eq!(array.as_slice(), vec![0x9fu8, 0x12, 0x2b, 0xff]);
/// # assert_eq!(array2.as_slice(), vec![0x9fu8, 0x12, 0x2b, 0xff]);
/// # assert_eq!(dict.as_slice(), vec![0xbfu8, 0x61, b'a', 0x0e, 0x61, b'b', 0x20, 0xff]);
/// # assert_eq!(dict2.as_slice(), vec![0xbfu8, 0x61, b'a', 0x0e, 0x61, b'b', 0x20, 0xff]);
/// ```
pub trait Encoder: Sized {
    type Output;
    /// # Safety
    ///
    /// Internal function, do not use!
    unsafe fn writer(&mut self) -> ArrayWriter;
    /// # Safety
    ///
    /// Internal function, do not use!
    unsafe fn finish(self) -> Self::Output;

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
    fn encode_u64(mut self, value: u64) -> Self::Output {
        unsafe { self.writer() }.write_pos(value, None);
        unsafe { self.finish() }
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
    fn encode_i64(mut self, value: i64) -> Self::Output {
        if value < 0 {
            unsafe { self.writer() }.write_neg((-1 - value) as u64, None);
        } else {
            unsafe { self.writer() }.write_pos(value as u64, None);
        }
        unsafe { self.finish() }
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
    fn encode_f64(mut self, value: f64) -> Self::Output {
        unsafe { self.writer() }.write_lit(Literal::L8(value.to_bits()), None);
        unsafe { self.finish() }
    }
}

impl<'a> Encoder for CborBuilder<'a> {
    type Output = CborOwned;

    unsafe fn writer(&mut self) -> ArrayWriter {
        self.0.as_mut().into()
    }

    unsafe fn finish(self) -> Self::Output {
        finish_cbor(self.0)
    }
}

impl<'a> Encoder for ArrayWriter<'a> {
    type Output = Self;

    unsafe fn writer(&mut self) -> ArrayWriter {
        self.0.as_mut().into()
    }

    unsafe fn finish(self) -> Self::Output {
        self
    }
}

/// DSL helper for creating dictionary entries using [`DictWriter::with_key()`](struct.DictWriter#method.with_key).
pub struct DictValueWriter<'a>(Bytes<'a>, &'a str);

impl<'a> Encoder for DictValueWriter<'a> {
    type Output = DictWriter<'a>;

    unsafe fn writer(&mut self) -> ArrayWriter {
        let mut w = ArrayWriter::from(self.0.as_mut());
        w.write_str(self.1, None);
        w
    }

    unsafe fn finish(self) -> Self::Output {
        DictWriter(self.0)
    }
}

impl<'a, T: 'a> Encoder for ArrayBuilder<'a, T> {
    type Output = Self;

    unsafe fn writer(&mut self) -> ArrayWriter {
        self.0.as_mut().into()
    }

    unsafe fn finish(self) -> Self::Output {
        self
    }
}

/// DSL helper for creating dictionary entries using [`DictBuilder::with_key()`](struct.DictBuilder#method.with_key).
pub struct DictValueBuilder<'a, T>(Bytes<'a>, Box<dyn FnOnce(Bytes<'a>) -> T + 'a>, &'a str);

impl<'a, T: 'a> Encoder for DictValueBuilder<'a, T> {
    type Output = DictBuilder<'a, T>;

    unsafe fn writer(&mut self) -> ArrayWriter {
        let mut w = ArrayWriter::from(self.0.as_mut());
        w.write_str(self.2, None);
        w
    }

    unsafe fn finish(self) -> Self::Output {
        DictBuilder(self.0, self.1)
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

fn write_info(bytes: &mut Vec<u8>, major: u8, value: u64) {
    if value < 24 {
        bytes.push(major << 5 | (value as u8))
    } else if value < 0x100 {
        bytes.push(major << 5 | 24);
        bytes.push(value as u8);
    } else if value < 0x1_0000 {
        bytes.push(major << 5 | 25);
        bytes.push((value >> 8) as u8);
        bytes.push(value as u8);
    } else if value < 0x1_0000_0000 {
        bytes.push(major << 5 | 26);
        bytes.push((value >> 24) as u8);
        bytes.push((value >> 16) as u8);
        bytes.push((value >> 8) as u8);
        bytes.push(value as u8);
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
