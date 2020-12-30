#![allow(dead_code)]

pub const MAJOR_POS: u8 = 0;
pub const MAJOR_NEG: u8 = 1;
pub const MAJOR_BYTES: u8 = 2;
pub const MAJOR_STR: u8 = 3;
pub const MAJOR_ARRAY: u8 = 4;
pub const MAJOR_DICT: u8 = 5;
pub const MAJOR_TAG: u8 = 6;
pub const MAJOR_LIT: u8 = 7;

pub const TAG_ISO8601: u64 = 0; // on string
pub const TAG_EPOCH: u64 = 1; // on non-big integer or float
pub const TAG_BIGNUM_POS: u64 = 2; // on byte string (big endian)
pub const TAG_BIGNUM_NEG: u64 = 3; // on byte string (big endian)
pub const TAG_BIGDECIMAL: u64 = 4; // on array of [exponent, mantissa]
pub const TAG_BIGFLOAT: u64 = 5; // on array of [exponent, mantissa]
pub const TAG_CBOR_ITEM: u64 = 24; // on byte string that contains CBOR
pub const TAG_URI: u64 = 32; // on string; see https://tools.ietf.org/html/rfc3986
pub const TAG_BASE64URL: u64 = 33; // on string
pub const TAG_BASE64: u64 = 34; // on string
pub const TAG_REGEX: u64 = 35; // on string
pub const TAG_MIME: u64 = 36; // on string; see https://tools.ietf.org/html/rfc2045
pub const TAG_CBOR_MARKER: u64 = 55799; // on anything; used only as magic number at beginning of file

pub const LIT_FALSE: u8 = 20;
pub const LIT_TRUE: u8 = 21;
pub const LIT_NULL: u8 = 22;
pub const LIT_UNDEFINED: u8 = 23;
pub const LIT_SIMPLE: u8 = 24;
pub const LIT_FLOAT16: u8 = 25;
pub const LIT_FLOAT32: u8 = 26;
pub const LIT_FLOAT64: u8 = 27;

pub const INDEFINITE_SIZE: u8 = 31;
pub const STOP_BYTE: u8 = 0xff;
