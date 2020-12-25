#![allow(dead_code)]

pub const MAJOR_POS: u8 = 0;
pub const MAJOR_NEG: u8 = 1;
pub const MAJOR_BYTES: u8 = 2;
pub const MAJOR_STR: u8 = 3;
pub const MAJOR_ARRAY: u8 = 4;
pub const MAJOR_DICT: u8 = 5;
pub const MAJOR_TAG: u8 = 6;
pub const MAJOR_LIT: u8 = 7;

pub const TAG_ISO8601: u64 = 0;
pub const TAG_EPOCH: u64 = 1;
pub const TAG_BIGNUM_POS: u64 = 2;
pub const TAG_BIGNUM_NEG: u64 = 3;
pub const TAG_FRACTION: u64 = 4;
pub const TAG_BIGFLOAT: u64 = 5;
pub const TAG_CBOR_ITEM: u64 = 24;
pub const TAG_CBOR_MARKER: u64 = 55799;

pub const LIT_FALSE: u8 = 20;
pub const LIT_TRUE: u8 = 21;
pub const LIT_NULL: u8 = 22;
pub const LIT_UNDEFINED: u8 = 23;
pub const LIT_FLOAT32: u8 = 26;
pub const LIT_FLOAT64: u8 = 27;

pub const INDEFINITE_SIZE: u8 = 31;
pub const STOP_BYTE: u8 = 0xff;
