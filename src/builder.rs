use crate::{constants::*, Cbor};

#[derive(Default)]
pub struct CborBuilder(Vec<u8>);

pub struct ArrayBuilder<T>(Vec<u8>, Box<dyn FnOnce(Vec<u8>) -> T>);

pub struct DictBuilder<T>(Vec<u8>, Box<dyn FnOnce(Vec<u8>) -> T>);

fn finish_cbor(mut v: Vec<u8>) -> Cbor<'static> {
    v.shrink_to_fit();
    Cbor::new(v)
}

impl CborBuilder {
    pub fn new(mut v: Vec<u8>) -> Self {
        v.clear();
        Self(v)
    }

    pub fn write_pos(mut self, value: u64, tag: Option<u64>) -> Cbor<'static> {
        write_positive(&mut self.0, value, tag);
        finish_cbor(self.0)
    }

    pub fn write_neg(mut self, value: u64, tag: Option<u64>) -> Cbor<'static> {
        write_neg(&mut self.0, value, tag);
        finish_cbor(self.0)
    }

    pub fn write_bytes(mut self, value: &[u8], tag: Option<u64>) -> Cbor<'static> {
        write_bytes(&mut self.0, value, tag);
        finish_cbor(self.0)
    }

    pub fn write_str(mut self, value: &str, tag: Option<u64>) -> Cbor<'static> {
        write_str(&mut self.0, value, tag);
        finish_cbor(self.0)
    }

    pub fn write_bool(mut self, value: bool, tag: Option<u64>) -> Cbor<'static> {
        write_bool(&mut self.0, value, tag);
        finish_cbor(self.0)
    }

    pub fn write_null(mut self, tag: Option<u64>) -> Cbor<'static> {
        write_null(&mut self.0, tag);
        finish_cbor(self.0)
    }

    pub fn write_undefined(mut self, tag: Option<u64>) -> Cbor<'static> {
        write_undefined(&mut self.0, tag);
        finish_cbor(self.0)
    }

    pub fn write_array(mut self, tag: Option<u64>) -> ArrayBuilder<Cbor<'static>> {
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_ARRAY);
        ArrayBuilder(self.0, Box::new(finish_cbor))
    }

    pub fn write_dict(mut self, tag: Option<u64>) -> DictBuilder<Cbor<'static>> {
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_DICT);
        DictBuilder(self.0, Box::new(finish_cbor))
    }

    pub fn write_lit(mut self, value: u64, tag: Option<u64>) -> Cbor<'static> {
        write_tag(&mut self.0, tag);
        write_info(&mut self.0, MAJOR_LIT, value);
        finish_cbor(self.0)
    }
}

pub trait WriteToArray {
    fn write_pos(&mut self, value: u64, tag: Option<u64>);
    fn write_neg(&mut self, value: u64, tag: Option<u64>);
    fn write_bytes(&mut self, value: &[u8], tag: Option<u64>);
    fn write_str(&mut self, value: &str, tag: Option<u64>);
    fn write_bool(&mut self, value: bool, tag: Option<u64>);
    fn write_null(&mut self, tag: Option<u64>);
    fn write_undefined(&mut self, tag: Option<u64>);
    fn write_lit(&mut self, value: u64, tag: Option<u64>);
    fn write_array_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToArray));
    fn write_dict_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToDict));
}

impl<T: 'static> WriteToArray for ArrayBuilder<T> {
    fn write_pos(&mut self, value: u64, tag: Option<u64>) {
        WriteToArray::write_pos(&mut &mut self.0, value, tag)
    }

    fn write_neg(&mut self, value: u64, tag: Option<u64>) {
        WriteToArray::write_neg(&mut &mut self.0, value, tag)
    }

    fn write_bytes(&mut self, value: &[u8], tag: Option<u64>) {
        WriteToArray::write_bytes(&mut &mut self.0, value, tag)
    }

    fn write_str(&mut self, value: &str, tag: Option<u64>) {
        WriteToArray::write_str(&mut &mut self.0, value, tag)
    }

    fn write_bool(&mut self, value: bool, tag: Option<u64>) {
        WriteToArray::write_bool(&mut &mut self.0, value, tag)
    }

    fn write_null(&mut self, tag: Option<u64>) {
        WriteToArray::write_null(&mut &mut self.0, tag)
    }

    fn write_undefined(&mut self, tag: Option<u64>) {
        WriteToArray::write_undefined(&mut &mut self.0, tag)
    }

    fn write_lit(&mut self, value: u64, tag: Option<u64>) {
        WriteToArray::write_lit(&mut &mut self.0, value, tag)
    }

    fn write_array_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToArray)) {
        WriteToArray::write_array_rec(&mut &mut self.0, tag, f)
    }

    fn write_dict_rec(&mut self, tag: Option<u64>, f: &mut dyn FnMut(&mut dyn WriteToDict)) {
        WriteToArray::write_dict_rec(&mut &mut self.0, tag, f)
    }
}

pub trait WriteToDict {
    fn write_pos(&mut self, key: &str, value: u64, tag: Option<u64>);
    fn write_neg(&mut self, key: &str, value: u64, tag: Option<u64>);
    fn write_bytes(&mut self, key: &str, value: &[u8], tag: Option<u64>);
    fn write_str(&mut self, key: &str, value: &str, tag: Option<u64>);
    fn write_bool(&mut self, key: &str, value: bool, tag: Option<u64>);
    fn write_null(&mut self, key: &str, tag: Option<u64>);
    fn write_undefined(&mut self, key: &str, tag: Option<u64>);
    fn write_lit(&mut self, key: &str, value: u64, tag: Option<u64>);
    fn write_array_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToArray),
    );
    fn write_dict_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToDict),
    );
}

impl<T: 'static> WriteToDict for DictBuilder<T> {
    fn write_pos(&mut self, key: &str, value: u64, tag: Option<u64>) {
        WriteToDict::write_pos(&mut &mut self.0, key, value, tag)
    }

    fn write_neg(&mut self, key: &str, value: u64, tag: Option<u64>) {
        WriteToDict::write_neg(&mut &mut self.0, key, value, tag)
    }

    fn write_bytes(&mut self, key: &str, value: &[u8], tag: Option<u64>) {
        WriteToDict::write_bytes(&mut &mut self.0, key, value, tag)
    }

    fn write_str(&mut self, key: &str, value: &str, tag: Option<u64>) {
        WriteToDict::write_str(&mut &mut self.0, key, value, tag)
    }

    fn write_bool(&mut self, key: &str, value: bool, tag: Option<u64>) {
        WriteToDict::write_bool(&mut &mut self.0, key, value, tag)
    }

    fn write_null(&mut self, key: &str, tag: Option<u64>) {
        WriteToDict::write_null(&mut &mut self.0, key, tag)
    }

    fn write_undefined(&mut self, key: &str, tag: Option<u64>) {
        WriteToDict::write_undefined(&mut &mut self.0, key, tag)
    }

    fn write_lit(&mut self, key: &str, value: u64, tag: Option<u64>) {
        WriteToDict::write_lit(&mut &mut self.0, key, value, tag)
    }

    fn write_array_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToArray),
    ) {
        WriteToDict::write_array_rec(&mut &mut self.0, key, tag, f)
    }

    fn write_dict_rec(
        &mut self,
        key: &str,
        tag: Option<u64>,
        f: &mut dyn FnMut(&mut dyn WriteToDict),
    ) {
        WriteToDict::write_dict_rec(&mut &mut self.0, key, tag, f)
    }
}

impl<T: 'static> ArrayBuilder<T> {
    pub fn finish(mut self) -> T {
        self.0.push(STOP_BYTE);
        self.1(self.0)
    }

    pub fn write_array(mut self, tag: Option<u64>) -> ArrayBuilder<Self> {
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_ARRAY);
        let func = self.1;
        ArrayBuilder(self.0, Box::new(|v| Self(v, func)))
    }

    pub fn write_dict(mut self, tag: Option<u64>) -> DictBuilder<Self> {
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_DICT);
        let func = self.1;
        DictBuilder(self.0, Box::new(|v| Self(v, func)))
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

    fn write_lit(&mut self, value: u64, tag: Option<u64>) {
        write_tag(self, tag);
        write_info(self, MAJOR_LIT, value);
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

impl<T: 'static> DictBuilder<T> {
    pub fn finish(mut self) -> T {
        self.0.push(STOP_BYTE);
        self.1(self.0)
    }

    pub fn write_array(mut self, key: &str, tag: Option<u64>) -> ArrayBuilder<Self> {
        write_str(&mut self.0, key, None);
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_ARRAY);
        let func = self.1;
        ArrayBuilder(self.0, Box::new(|v| Self(v, func)))
    }

    pub fn write_dict(mut self, key: &str, tag: Option<u64>) -> DictBuilder<Self> {
        write_str(&mut self.0, key, None);
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_DICT);
        let func = self.1;
        DictBuilder(self.0, Box::new(|v| Self(v, func)))
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

    fn write_lit(&mut self, key: &str, value: u64, tag: Option<u64>) {
        write_str(self, key, None);
        write_tag(self, tag);
        write_info(self, MAJOR_LIT, value);
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

fn write_indefinite(bytes: &mut Vec<u8>, major: u8) {
    bytes.push(major << 5 | INDEFINITE_SIZE);
}
