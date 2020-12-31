/// Major type 0: positive number of up to 64 bit
pub const MAJOR_POS: u8 = 0;
/// Major type 1: negative number of up to 64 bit
pub const MAJOR_NEG: u8 = 1;
/// Major type 2: byte string
pub const MAJOR_BYTES: u8 = 2;
/// Major type 3: utf-8 string
pub const MAJOR_STR: u8 = 3;
/// Major type 4: array
pub const MAJOR_ARRAY: u8 = 4;
/// Major type 5: dictionary
pub const MAJOR_DICT: u8 = 5;
/// Major type 6: tag applied to the following item, with a value of up to 64 bit
pub const MAJOR_TAG: u8 = 6;
/// Major type 7: simple values and floating-point numbers
pub const MAJOR_LIT: u8 = 7;

/// String tag: ISO8601 timestamp, see [RFC 4287 §3.3](https://tools.ietf.org/html/rfc4287#section-3.3)
pub const TAG_ISO8601: u64 = 0;
/// Integer or floating-point tag: seconds since the Unix epoch (possibly negative)
pub const TAG_EPOCH: u64 = 1;
/// Byte string tag: positive bigint, big endian encoding
pub const TAG_BIGNUM_POS: u64 = 2;
/// Byte string tag: negative bigint, big endian encoding
pub const TAG_BIGNUM_NEG: u64 = 3;
/// Array tag: big decimal (i.e. base 10), encoded as integer exponent and any number mantissa
pub const TAG_BIGDECIMAL: u64 = 4;
/// Array tag: big float (i.e. base 2), encoded as integer exponent and any number mantissa
pub const TAG_BIGFLOAT: u64 = 5;
/// Byte string tag: contents shall be interpreted as nested CBOR item
pub const TAG_CBOR_ITEM: u64 = 24;
/// String tag: [RFC 3986](https://tools.ietf.org/html/rfc3986) URI
pub const TAG_URI: u64 = 32;
/// String tag: base64url encoded byte string, see [RFC 4648](https://tools.ietf.org/html/rfc4648)
pub const TAG_BASE64URL: u64 = 33;
/// String tag: base64 encoded byte string, see [RFC 4648](https://tools.ietf.org/html/rfc4648)
pub const TAG_BASE64: u64 = 34;
/// String tag: regular expression (PCRE or ECMA)
pub const TAG_REGEX: u64 = 35;
/// String tag: mime encoded payload, including headers, see [RFC 2045](https://tools.ietf.org/html/rfc2045)
pub const TAG_MIME: u64 = 36;
/// Marker for tagging the top-level CBOR item such that it cannot be misinterpreted as JSON
pub const TAG_CBOR_MARKER: u64 = 55799;

/// Simple value: FALSE
pub const LIT_FALSE: u8 = 20;
/// Simple value: TRUE
pub const LIT_TRUE: u8 = 21;
/// Simple value: NULL
pub const LIT_NULL: u8 = 22;
/// Simple value: UNDEFINED
pub const LIT_UNDEFINED: u8 = 23;
/// Simple value encoded as following byte
pub const LIT_SIMPLE: u8 = 24;
/// half-precision floating-point value in the next two bytes
pub const LIT_FLOAT16: u8 = 25;
/// single-precision floating-point value in the next four bytes
pub const LIT_FLOAT32: u8 = 26;
/// double-precision floating-point value in the next eight bytes
pub const LIT_FLOAT64: u8 = 27;

pub(crate) const INDEFINITE_SIZE: u8 = 31;
pub(crate) const STOP_BYTE: u8 = 0xff;
