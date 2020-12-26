use crate::{constants::*, reader::Literal, Cbor};

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

/// Builder for an array value.
///
/// Calling the [`finish()`](#method.finish) method will return either the fully constructed
/// CBOR value (if this was the top-level array) or the builder of the array or dict into
/// which this array was placed.
///
/// If you want to recursively create a CBOR structure without statically known recursion limit
/// then you’ll want to take a look at the [`WriteToArray::write_array_rec()`](trait.WriteToArray#tymethod.write_array_rec)
/// method (the compiler would otherwise kindly inform you of a type expansion hitting the recursion
/// limit while instantiating your recursive function).
pub struct ArrayBuilder<'a, T>(Bytes<'a>, Box<dyn FnOnce(Bytes<'a>) -> T + 'a>);

/// Builder for an dict value.
///
/// Calling the [`finish()`](#method.finish) method will return either the fully constructed
/// CBOR value (if this was the top-level dict) or the builder of the array or dict into
/// which this dict was placed.
///
/// If you want to recursively create a CBOR structure without statically known recursion limit
/// then you’ll want to take a look at the [`WriteToDict::write_dict_rec()`](trait.WriteToDict#tymethod.write_dict_rec)
/// method (the compiler would otherwise kindly inform you of a type expansion hitting the recursion
/// limit while instantiating your recursive function).
pub struct DictBuilder<'a, T>(Bytes<'a>, Box<dyn FnOnce(Bytes<'a>) -> T + 'a>);

fn finish_cbor(v: Bytes<'_>) -> Cbor<'static> {
    Cbor::trusting(v.as_slice())
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
    pub fn write_pos(mut self, value: u64, tag: Option<u64>) -> Cbor<'static> {
        write_positive(self.0.as_mut(), value, tag);
        finish_cbor(self.0)
    }

    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    pub fn write_neg(mut self, value: u64, tag: Option<u64>) -> Cbor<'static> {
        write_neg(self.0.as_mut(), value, tag);
        finish_cbor(self.0)
    }

    /// Write the given slice as a definite size byte string.
    pub fn write_bytes(mut self, value: &[u8], tag: Option<u64>) -> Cbor<'static> {
        write_bytes(self.0.as_mut(), value, tag);
        finish_cbor(self.0)
    }

    /// Write the given slice as a definite size string.
    pub fn write_str(mut self, value: &str, tag: Option<u64>) -> Cbor<'static> {
        write_str(self.0.as_mut(), value, tag);
        finish_cbor(self.0)
    }

    pub fn write_bool(mut self, value: bool, tag: Option<u64>) -> Cbor<'static> {
        write_bool(self.0.as_mut(), value, tag);
        finish_cbor(self.0)
    }

    pub fn write_null(mut self, tag: Option<u64>) -> Cbor<'static> {
        write_null(self.0.as_mut(), tag);
        finish_cbor(self.0)
    }

    pub fn write_undefined(mut self, tag: Option<u64>) -> Cbor<'static> {
        write_undefined(self.0.as_mut(), tag);
        finish_cbor(self.0)
    }

    /// Write custom literal value — [RFC7049 §2.3](https://tools.ietf.org/html/rfc7049#section-2.3) is required reading.
    pub fn write_lit(mut self, value: Literal, tag: Option<u64>) -> Cbor<'static> {
        write_tag(self.0.as_mut(), tag);
        write_lit(self.0.as_mut(), value);
        finish_cbor(self.0)
    }

    /// Write a top-level array that is then filled by the returned builder.
    pub fn write_array(mut self, tag: Option<u64>) -> ArrayBuilder<'a, Cbor<'static>> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_ARRAY);
        ArrayBuilder(self.0, Box::new(finish_cbor))
    }

    /// Write a top-level dict that is then filled by the returned builder.
    pub fn write_dict(mut self, tag: Option<u64>) -> DictBuilder<'a, Cbor<'static>> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_DICT);
        DictBuilder(self.0, Box::new(finish_cbor))
    }
}

/// The actual data writing methods of ArrayBuilder.
pub trait WriteToArray {
    /// Write a unsigned value of up to 64 bits.
    fn write_pos(&mut self, value: u64, tag: Option<u64>);
    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    fn write_neg(&mut self, value: u64, tag: Option<u64>);
    /// Write the given slice as a definite size byte string.
    fn write_bytes(&mut self, value: &[u8], tag: Option<u64>);
    /// Write the given slice as a definite size string.
    fn write_str(&mut self, value: &str, tag: Option<u64>);
    fn write_bool(&mut self, value: bool, tag: Option<u64>);
    fn write_null(&mut self, tag: Option<u64>);
    fn write_undefined(&mut self, tag: Option<u64>);
    /// Write custom literal value — [RFC7049 §2.3](https://tools.ietf.org/html/rfc7049#section-2.3) is required reading.
    fn write_lit(&mut self, value: Literal, tag: Option<u64>);
    /// Write a nested array using the given closure that receives an array builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// use cbor_data::{CborBuilder, WriteToArray, WriteToDict};
    ///
    /// let mut cbor = CborBuilder::default().write_array(None);
    /// cbor.write_array_rec(None, &mut |builder| {
    ///     builder.write_pos(42, None);
    /// });
    /// let cbor = cbor.finish();
    ///
    /// assert_eq!(cbor.as_slice(), vec![0x9fu8, 0x9f, 0x18, 42, 0xff, 0xff]);
    /// ```
    fn write_array_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToArray));
    /// Write a nested dict using the given closure that receives an dict builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// use cbor_data::{CborBuilder, WriteToArray, WriteToDict};
    ///
    /// let mut cbor = CborBuilder::default().write_array(None);
    /// cbor.write_array_rec(None, &mut |builder| {
    ///     builder.write_pos(42, None);
    /// });
    /// let cbor = cbor.finish();
    ///
    /// assert_eq!(cbor.as_slice(), vec![0x9fu8, 0x9f, 0x18, 42, 0xff, 0xff]);
    /// ```
    fn write_dict_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToDict));
}

impl<'a, T: 'static> WriteToArray for ArrayBuilder<'a, T> {
    fn write_pos(&mut self, value: u64, tag: Option<u64>) {
        WriteToArray::write_pos(&mut self.0.as_mut(), value, tag)
    }

    fn write_neg(&mut self, value: u64, tag: Option<u64>) {
        WriteToArray::write_neg(&mut self.0.as_mut(), value, tag)
    }

    fn write_bytes(&mut self, value: &[u8], tag: Option<u64>) {
        WriteToArray::write_bytes(&mut self.0.as_mut(), value, tag)
    }

    fn write_str(&mut self, value: &str, tag: Option<u64>) {
        WriteToArray::write_str(&mut self.0.as_mut(), value, tag)
    }

    fn write_bool(&mut self, value: bool, tag: Option<u64>) {
        WriteToArray::write_bool(&mut self.0.as_mut(), value, tag)
    }

    fn write_null(&mut self, tag: Option<u64>) {
        WriteToArray::write_null(&mut self.0.as_mut(), tag)
    }

    fn write_undefined(&mut self, tag: Option<u64>) {
        WriteToArray::write_undefined(&mut self.0.as_mut(), tag)
    }

    fn write_lit(&mut self, value: Literal, tag: Option<u64>) {
        WriteToArray::write_lit(&mut self.0.as_mut(), value, tag)
    }

    fn write_array_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToArray)) {
        WriteToArray::write_array_rec(&mut self.0.as_mut(), tag, f)
    }

    fn write_dict_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToDict)) {
        WriteToArray::write_dict_rec(&mut self.0.as_mut(), tag, f)
    }
}

/// The actual data writing methods of DictBuilder.
pub trait WriteToDict {
    /// Write a unsigned value of up to 64 bits.
    fn write_pos(&mut self, key: &str, value: u64, tag: Option<u64>);
    /// Write a negative value of up to 64 bits — the represented number is `-1 - value`.
    fn write_neg(&mut self, key: &str, value: u64, tag: Option<u64>);
    /// Write the given slice as a definite size byte string.
    fn write_bytes(&mut self, key: &str, value: &[u8], tag: Option<u64>);
    /// Write the given slice as a definite size string.
    fn write_str(&mut self, key: &str, value: &str, tag: Option<u64>);
    fn write_bool(&mut self, key: &str, value: bool, tag: Option<u64>);
    fn write_null(&mut self, key: &str, tag: Option<u64>);
    fn write_undefined(&mut self, key: &str, tag: Option<u64>);
    fn write_lit(&mut self, key: &str, value: Literal, tag: Option<u64>);
    /// Write a nested array using the given closure that receives an array builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// use cbor_data::{CborBuilder, WriteToArray, WriteToDict};
    ///
    /// let mut cbor = CborBuilder::default().write_dict(None);
    /// cbor.write_array_rec("x", None, &mut |builder| {
    ///     builder.write_pos(42, None);
    /// });
    /// let cbor = cbor.finish();
    ///
    /// assert_eq!(cbor.as_slice(), vec![0xbfu8, 0x61, b'x', 0x9f, 0x18, 42, 0xff, 0xff]);
    /// ```
    fn write_array_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToArray),
    );
    /// Write a nested dict using the given closure that receives an dict builder.
    ///
    /// This method is very useful for recursively building a CBOR structure without statically
    /// known recursion limit, avoiding infinite type errors.
    ///
    /// ```
    /// use cbor_data::{CborBuilder, WriteToArray, WriteToDict};
    ///
    /// let mut cbor = CborBuilder::default().write_dict(None);
    /// cbor.write_dict_rec("x", None, &mut |builder| {
    ///     builder.write_pos("y", 42, None);
    /// });
    /// let cbor = cbor.finish();
    ///
    /// assert_eq!(cbor.as_slice(), vec![0xbfu8, 0x61, b'x', 0xbf, 0x61, b'y', 0x18, 42, 0xff, 0xff]);
    /// ```
    fn write_dict_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToDict),
    );
}

impl<'a, T: 'static> WriteToDict for DictBuilder<'a, T> {
    fn write_pos(&mut self, key: &str, value: u64, tag: Option<u64>) {
        WriteToDict::write_pos(&mut self.0.as_mut(), key, value, tag)
    }

    fn write_neg(&mut self, key: &str, value: u64, tag: Option<u64>) {
        WriteToDict::write_neg(&mut self.0.as_mut(), key, value, tag)
    }

    fn write_bytes(&mut self, key: &str, value: &[u8], tag: Option<u64>) {
        WriteToDict::write_bytes(&mut self.0.as_mut(), key, value, tag)
    }

    fn write_str(&mut self, key: &str, value: &str, tag: Option<u64>) {
        WriteToDict::write_str(&mut self.0.as_mut(), key, value, tag)
    }

    fn write_bool(&mut self, key: &str, value: bool, tag: Option<u64>) {
        WriteToDict::write_bool(&mut self.0.as_mut(), key, value, tag)
    }

    fn write_null(&mut self, key: &str, tag: Option<u64>) {
        WriteToDict::write_null(&mut self.0.as_mut(), key, tag)
    }

    fn write_undefined(&mut self, key: &str, tag: Option<u64>) {
        WriteToDict::write_undefined(&mut self.0.as_mut(), key, tag)
    }

    fn write_lit(&mut self, key: &str, value: Literal, tag: Option<u64>) {
        WriteToDict::write_lit(&mut self.0.as_mut(), key, value, tag)
    }

    fn write_array_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToArray),
    ) {
        WriteToDict::write_array_rec(&mut self.0.as_mut(), key, tag, f)
    }

    fn write_dict_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToDict),
    ) {
        WriteToDict::write_dict_rec(&mut self.0.as_mut(), key, tag, f)
    }
}

impl<'a, T: 'static> ArrayBuilder<'a, T> {
    /// Finish building this array and return to the outer context. In case of a
    /// top-level array this returns the complete [`Cbor`](struct.Cbor) value.
    pub fn finish(mut self) -> T {
        self.0.as_mut().push(STOP_BYTE);
        self.1(self.0)
    }

    /// Write a nested array that is then filled by the returned builder.
    /// You can resume building this outer array by using the [`finish`()](struct.ArrayBuilder#method.finish)
    /// method.
    pub fn write_array(mut self, tag: Option<u64>) -> ArrayBuilder<'a, Self> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_ARRAY);
        let Self(v, func) = self;
        ArrayBuilder(v, Box::new(|v| Self(v, func)))
    }

    /// Write a nested dict that is then filled by the returned builder.
    /// You can resume building this outer array by using the [`finish`()](struct.DictBuilder#method.finish)
    /// method.
    pub fn write_dict(mut self, tag: Option<u64>) -> DictBuilder<'a, Self> {
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_DICT);
        let Self(v, func) = self;
        DictBuilder(v, Box::new(|v| Self(v, func)))
    }
}

impl WriteToArray for &mut Vec<u8> {
    fn write_pos(&mut self, value: u64, tag: Option<u64>) {
        write_positive(self, value, tag);
    }

    fn write_neg(&mut self, value: u64, tag: Option<u64>) {
        write_neg(self, value, tag);
    }

    fn write_bytes(&mut self, value: &[u8], tag: Option<u64>) {
        write_bytes(self, value, tag);
    }

    fn write_str(&mut self, value: &str, tag: Option<u64>) {
        write_str(self, value, tag);
    }

    fn write_bool(&mut self, value: bool, tag: Option<u64>) {
        write_bool(self, value, tag);
    }

    fn write_null(&mut self, tag: Option<u64>) {
        write_null(self, tag);
    }

    fn write_undefined(&mut self, tag: Option<u64>) {
        write_undefined(self, tag);
    }

    fn write_lit(&mut self, value: Literal, tag: Option<u64>) {
        write_tag(self, tag);
        write_lit(self, value);
    }

    fn write_array_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToArray)) {
        write_tag(self, tag);
        write_indefinite(self, MAJOR_ARRAY);
        f(self);
        self.push(STOP_BYTE);
    }

    fn write_dict_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToDict)) {
        write_tag(self, tag);
        write_indefinite(self, MAJOR_DICT);
        f(self);
        self.push(STOP_BYTE);
    }
}

impl<'a, T: 'static> DictBuilder<'a, T> {
    /// Finish building this dict and return to the outer context. In case of a
    /// top-level dict this returns the complete [`Cbor`](struct.Cbor) value.
    pub fn finish(mut self) -> T {
        self.0.as_mut().push(STOP_BYTE);
        self.1(self.0)
    }

    /// Write a nested array that is then filled by the returned builder.
    /// You can resume building this outer dict by using the [`finish`()](struct.ArrayBuilder#method.finish)
    /// method.
    pub fn write_array(mut self, key: &str, tag: Option<u64>) -> ArrayBuilder<'a, Self> {
        write_str(self.0.as_mut(), key, None);
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_ARRAY);
        let Self(v, func) = self;
        ArrayBuilder(v, Box::new(|v| Self(v, func)))
    }

    /// Write a nested dict that is then filled by the returned builder.
    /// You can resume building this outer dict by using the [`finish`()](struct.DictBuilder#method.finish)
    /// method.
    pub fn write_dict(mut self, key: &str, tag: Option<u64>) -> DictBuilder<'a, Self> {
        write_str(self.0.as_mut(), key, None);
        write_tag(self.0.as_mut(), tag);
        write_indefinite(self.0.as_mut(), MAJOR_DICT);
        let Self(v, func) = self;
        DictBuilder(v, Box::new(|v| Self(v, func)))
    }
}

impl WriteToDict for &mut Vec<u8> {
    fn write_pos(&mut self, key: &str, value: u64, tag: Option<u64>) {
        write_str(self, key, None);
        write_positive(self, value, tag);
    }

    fn write_neg(&mut self, key: &str, value: u64, tag: Option<u64>) {
        write_str(self, key, None);
        write_neg(self, value, tag);
    }

    fn write_bytes(&mut self, key: &str, value: &[u8], tag: Option<u64>) {
        write_str(self, key, None);
        write_bytes(self, value, tag);
    }

    fn write_str(&mut self, key: &str, value: &str, tag: Option<u64>) {
        write_str(self, key, None);
        write_str(self, value, tag);
    }

    fn write_bool(&mut self, key: &str, value: bool, tag: Option<u64>) {
        write_str(self, key, None);
        write_bool(self, value, tag);
    }

    fn write_null(&mut self, key: &str, tag: Option<u64>) {
        write_str(self, key, None);
        write_null(self, tag);
    }

    fn write_undefined(&mut self, key: &str, tag: Option<u64>) {
        write_str(self, key, None);
        write_undefined(self, tag);
    }

    fn write_lit(&mut self, key: &str, value: Literal, tag: Option<u64>) {
        write_str(self, key, None);
        write_tag(self, tag);
        write_lit(self, value);
    }

    fn write_array_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToArray),
    ) {
        write_str(self, key, None);
        write_tag(self, tag);
        write_indefinite(self, MAJOR_ARRAY);
        f(self);
        self.push(STOP_BYTE);
    }

    fn write_dict_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToDict),
    ) {
        write_str(self, key, None);
        write_tag(self, tag);
        write_indefinite(self, MAJOR_DICT);
        f(self);
        self.push(STOP_BYTE);
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
