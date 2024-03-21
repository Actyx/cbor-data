//! Everything in this module and below assumes that we’re dealing with valid CBOR bytes!
use self::iterators::{ArrayIter, BytesIter, DictIter, StringIter};
use crate::{
    constants::*,
    reader::{float, indefinite, integer, major, tags},
    Cbor, ItemKind, Tags,
};

pub mod indexing;
pub mod item;
pub mod iterators;
pub mod tags;

fn skip_bytes(bytes: &[u8]) -> (Option<&[u8]>, &[u8]) {
    let (len, _, mut rest) = integer(bytes).or_else(|| indefinite(bytes)).unwrap();
    if len == u64::MAX {
        // since an item takes at least 1 byte, u64::MAX is an impossible size
        while rest[0] != STOP_BYTE {
            let (len, _, r) = integer(rest).unwrap();
            rest = &r[len as usize..];
        }
        rest = &rest[1..];
        (None, rest)
    } else {
        let len = len as usize;
        (Some(&rest[..len]), &rest[len..])
    }
}

fn skip(bytes: &[u8]) -> (Option<&[u8]>, &[u8]) {
    match major(bytes).unwrap() {
        MAJOR_POS | MAJOR_NEG | MAJOR_LIT => (None, integer(bytes).unwrap().2),
        MAJOR_STR | MAJOR_BYTES => skip_bytes(bytes),
        MAJOR_TAG => skip(integer(bytes).unwrap().2),
        MAJOR_ARRAY => {
            let (len, _, mut rest) = integer(bytes).or_else(|| indefinite(bytes)).unwrap();
            if len == u64::MAX {
                // since an item takes at least 1 byte, u64::MAX is an impossible size
                while rest[0] != STOP_BYTE {
                    rest = skip(rest).1;
                }
                rest = &rest[1..];
            } else {
                for _ in 0..len {
                    rest = skip(rest).1;
                }
            }
            (None, rest)
        }
        MAJOR_DICT => {
            let (len, _, mut rest) = integer(bytes).or_else(|| indefinite(bytes)).unwrap();
            if len == u64::MAX {
                // since an item takes at least 1 byte, u64::MAX is an impossible size
                while rest[0] != STOP_BYTE {
                    rest = skip(rest).1;
                    rest = skip(rest).1;
                }
                rest = &rest[1..];
            } else {
                for _ in 0..len {
                    rest = skip(rest).1;
                    rest = skip(rest).1;
                }
            }
            (None, rest)
        }
        _ => unreachable!(),
    }
}

fn string_iter(bytes: &[u8]) -> StringIter<'_> {
    if bytes[0] & 31 == INDEFINITE_SIZE {
        StringIter::new(&bytes[1..], None)
    } else {
        StringIter::new(bytes, Some(1))
    }
}

fn bytes_iter(bytes: &[u8]) -> BytesIter<'_> {
    if bytes[0] & 31 == INDEFINITE_SIZE {
        BytesIter::new(&bytes[1..], None)
    } else {
        BytesIter::new(bytes, Some(1))
    }
}

fn item(bytes: &[u8]) -> ItemKind {
    use ItemKind::*;

    match major(bytes).unwrap() {
        MAJOR_POS => Pos(integer(bytes).unwrap().0),
        MAJOR_NEG => Neg(integer(bytes).unwrap().0),
        MAJOR_BYTES => Bytes(bytes_iter(bytes)),
        MAJOR_STR => Str(string_iter(bytes)),
        MAJOR_LIT => match bytes[0] & 31 {
            LIT_FALSE => Bool(false),
            LIT_TRUE => Bool(true),
            LIT_NULL => Null,
            LIT_UNDEFINED => Undefined,
            LIT_SIMPLE => Simple(bytes[1]),
            LIT_FLOAT16 | LIT_FLOAT32 | LIT_FLOAT64 => Float(float(bytes).unwrap().0),
            x if x < 24 => Simple(x),
            _ => unreachable!(),
        },
        MAJOR_TAG => item(integer(bytes).unwrap().2),
        MAJOR_ARRAY => {
            let (len, _, arr) = integer(bytes).or_else(|| indefinite(bytes)).unwrap();
            let len = if len == u64::MAX { None } else { Some(len) };
            Array(ArrayIter::new(arr, len))
        }
        MAJOR_DICT => {
            let (len, _, dict) = integer(bytes).or_else(|| indefinite(bytes)).unwrap();
            let len = if len == u64::MAX { None } else { Some(len) };
            Dict(DictIter::new(dict, len))
        }
        _ => unreachable!(),
    }
}

pub fn tagged_item(bytes: &[u8]) -> (Tags, ItemKind) {
    let (tags, rest) = tags(bytes).unwrap();
    let kind = item(rest);
    (tags, kind)
}

/// Iterator over CBOR items provided within a byte slice
///
/// The iterator yields either a known number of elements or until encountering a STOP_BYTE.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CborIter<'a>(&'a [u8], Option<u64>);

impl<'a> CborIter<'a> {
    pub fn new(bytes: &'a [u8], len: Option<u64>) -> Self {
        Self(bytes, len)
    }
    pub fn size(&self) -> Option<u64> {
        self.1
    }
}

impl<'a> Iterator for CborIter<'a> {
    type Item = (Option<&'a [u8]>, &'a Cbor);

    fn next(&mut self) -> Option<Self::Item> {
        let CborIter(b, elems) = self;
        if *elems == Some(0) || *elems == None && b[0] == STOP_BYTE {
            None
        } else {
            let (value, rest) = skip(b);
            let bytes = &b[..b.len() - rest.len()];
            if let Some(x) = elems.as_mut() {
                *x -= 1;
            }
            *b = rest;
            Some((value, Cbor::unchecked(bytes)))
        }
    }
}
