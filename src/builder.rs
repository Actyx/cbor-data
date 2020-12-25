use crate::{constants::*, Cbor};
use std::{borrow::Cow, marker::PhantomData};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct CborBuilder(Vec<u8>);

pub struct ArrayBuilder<T>(Vec<u8>, PhantomData<T>);

pub struct DictBuilder<T>(Vec<u8>, PhantomData<T>);

impl CborBuilder {
    pub fn new(mut v: Vec<u8>) -> Self {
        v.clear();
        Self(v)
    }

    pub fn write_pos(mut self, value: u64, tag: Option<u64>) -> Cbor<'static> {
        write_positive(&mut self.0, value, tag);
        Cbor::finish(self.0)
    }

    pub fn write_neg(mut self, value: u64, tag: Option<u64>) -> Cbor<'static> {
        write_neg(&mut self.0, value, tag);
        Cbor::finish(self.0)
    }

    pub fn write_bytes(mut self, value: &[u8], tag: Option<u64>) -> Cbor<'static> {
        write_bytes(&mut self.0, value, tag);
        Cbor::finish(self.0)
    }

    pub fn write_str(mut self, value: &str, tag: Option<u64>) -> Cbor<'static> {
        write_str(&mut self.0, value, tag);
        Cbor::finish(self.0)
    }

    pub fn write_bool(mut self, value: bool, tag: Option<u64>) -> Cbor<'static> {
        write_bool(&mut self.0, value, tag);
        Cbor::finish(self.0)
    }

    pub fn write_null(mut self, tag: Option<u64>) -> Cbor<'static> {
        write_null(&mut self.0, tag);
        Cbor::finish(self.0)
    }

    pub fn write_undefined(mut self, tag: Option<u64>) -> Cbor<'static> {
        write_undefined(&mut self.0, tag);
        Cbor::finish(self.0)
    }

    pub fn write_array(mut self, tag: Option<u64>) -> ArrayBuilder<Cbor<'static>> {
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_ARRAY);
        ArrayBuilder(self.0, PhantomData)
    }

    pub fn write_dict(mut self, tag: Option<u64>) -> DictBuilder<Cbor<'static>> {
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_DICT);
        DictBuilder(self.0, PhantomData)
    }

    pub fn write_lit(mut self, value: u64, tag: Option<u64>) -> Cbor<'static> {
        write_tag(&mut self.0, tag);
        write_info(&mut self.0, MAJOR_LIT, value);
        Cbor::finish(self.0)
    }
}

pub trait Finish {
    fn finish(v: Vec<u8>) -> Self;
}

impl<T> Finish for ArrayBuilder<T> {
    fn finish(v: Vec<u8>) -> Self {
        ArrayBuilder(v, PhantomData)
    }
}

impl<T> Finish for DictBuilder<T> {
    fn finish(v: Vec<u8>) -> Self {
        DictBuilder(v, PhantomData)
    }
}

impl Finish for Cbor<'static> {
    fn finish(mut v: Vec<u8>) -> Self {
        v.shrink_to_fit();
        Self(Cow::Owned(v))
    }
}

impl<T: Finish> ArrayBuilder<T> {
    pub fn finish(mut self) -> T {
        self.0.push(STOP_BYTE);
        T::finish(self.0)
    }

    pub fn write_pos(&mut self, value: u64, tag: Option<u64>) {
        write_positive(&mut self.0, value, tag);
    }

    pub fn write_neg(&mut self, value: u64, tag: Option<u64>) {
        write_neg(&mut self.0, value, tag);
    }

    pub fn write_bytes(&mut self, value: &[u8], tag: Option<u64>) {
        write_bytes(&mut self.0, value, tag);
    }

    pub fn write_str(&mut self, value: &str, tag: Option<u64>) {
        write_str(&mut self.0, value, tag);
    }

    pub fn write_bool(&mut self, value: bool, tag: Option<u64>) {
        write_bool(&mut self.0, value, tag);
    }

    pub fn write_null(&mut self, tag: Option<u64>) {
        write_null(&mut self.0, tag);
    }

    pub fn write_undefined(&mut self, tag: Option<u64>) {
        write_undefined(&mut self.0, tag);
    }

    pub fn write_array(mut self, tag: Option<u64>) -> ArrayBuilder<Self> {
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_ARRAY);
        ArrayBuilder(self.0, PhantomData)
    }

    pub fn write_dict(mut self, tag: Option<u64>) -> DictBuilder<Self> {
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_DICT);
        DictBuilder(self.0, PhantomData)
    }

    pub fn write_lit(mut self, value: u64, tag: Option<u64>) {
        write_tag(&mut self.0, tag);
        write_info(&mut self.0, MAJOR_LIT, value);
    }
}

impl<T: Finish> DictBuilder<T> {
    pub fn finish(mut self) -> T {
        self.0.push(STOP_BYTE);
        T::finish(self.0)
    }

    pub fn write_pos(&mut self, key: &str, value: u64, tag: Option<u64>) {
        write_str(&mut self.0, key, None);
        write_positive(&mut self.0, value, tag);
    }

    pub fn write_neg(&mut self, key: &str, value: u64, tag: Option<u64>) {
        write_str(&mut self.0, key, None);
        write_neg(&mut self.0, value, tag);
    }

    pub fn write_bytes(&mut self, key: &str, value: &[u8], tag: Option<u64>) {
        write_str(&mut self.0, key, None);
        write_bytes(&mut self.0, value, tag);
    }

    pub fn write_str(&mut self, key: &str, value: &str, tag: Option<u64>) {
        write_str(&mut self.0, key, None);
        write_str(&mut self.0, value, tag);
    }

    pub fn write_bool(&mut self, key: &str, value: bool, tag: Option<u64>) {
        write_str(&mut self.0, key, None);
        write_bool(&mut self.0, value, tag);
    }

    pub fn write_null(&mut self, key: &str, tag: Option<u64>) {
        write_str(&mut self.0, key, None);
        write_null(&mut self.0, tag);
    }

    pub fn write_undefined(&mut self, key: &str, tag: Option<u64>) {
        write_str(&mut self.0, key, None);
        write_undefined(&mut self.0, tag);
    }

    pub fn write_array(mut self, key: &str, tag: Option<u64>) -> ArrayBuilder<Self> {
        write_str(&mut self.0, key, None);
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_ARRAY);
        ArrayBuilder(self.0, PhantomData)
    }

    pub fn write_dict(mut self, key: &str, tag: Option<u64>) -> DictBuilder<Self> {
        write_str(&mut self.0, key, None);
        write_tag(&mut self.0, tag);
        write_indefinite(&mut self.0, MAJOR_DICT);
        DictBuilder(self.0, PhantomData)
    }

    pub fn write_lit(mut self, key: &str, value: u64, tag: Option<u64>) {
        write_str(&mut self.0, key, None);
        write_tag(&mut self.0, tag);
        write_info(&mut self.0, MAJOR_LIT, value);
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
